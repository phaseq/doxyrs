use rayon::prelude::*;
use roxmltree::Document;
use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use structopt::StructOpt;
use tera::Tera;

mod parser;

#[derive(Debug, StructOpt)]
struct Cli {
    /// Print help message
    #[structopt(short, long)]
    help: bool,

    /// Root directory for the doxygen XML (required)
    #[structopt(long)]
    source: String,

    /// Directory containing the doxygen XML output (required)
    #[structopt(long)]
    xml: String,

    /// HTML output directory (required)
    #[structopt(long)]
    output: String,
}

fn main() {
    let opt = Cli::from_args();

    let source_dir = PathBuf::from(opt.source);

    let xml_dir = PathBuf::from(opt.xml);
    if !xml_dir.exists() {
        println!("--xml path not found: {}", xml_dir.to_string_lossy());
        std::process::exit(1);
    }

    let html_dir = PathBuf::from(opt.output);
    std::fs::create_dir_all(&html_dir.join("images")).unwrap();

    copy_static_files(&html_dir).unwrap();

    // read the index file
    let index_path = xml_dir.join("index.xml");
    let content = std::fs::read_to_string(&index_path).unwrap();
    let doc = Document::parse(&content).unwrap();

    // parse all compounds
    let index = doc
        .root()
        .children()
        .find(|n| n.has_tag_name("doxygenindex"))
        .unwrap();

    let compound_nodes: Vec<_> = index
        .children()
        .filter(|n| {
            n.has_tag_name("compound")
                && n.attribute("kind")
                    .map(|kind| kind == "file" || kind == "page")
                    .unwrap()
        })
        .collect();
    // TODO: we could use par_bridge if we don't care about the order of nodes. Right now we do.
    let compounds: Vec<Compound> = compound_nodes
        .par_iter()
        //.iter()
        .filter_map(|compound| {
            /*let name = compound
                .children()
                .find(|n| n.has_tag_name("name"))
                .unwrap()
                .text()
                .unwrap();
            if name != "mwVerifierNestedEnums.hpp" {
                return None;
            }*/
            let ref_id = compound.attribute("refid").unwrap();
            let kind = compound.attribute("kind").unwrap();
            match kind {
                "file" => {
                    let file = parser::parse_compound_file(&xml_dir, ref_id);
                    if file.scopes.is_empty() {
                        //println!("{} is empty", file.ref_id);
                        None
                    } else {
                        Some(Compound::File(file))
                    }
                }
                "page" => Some(Compound::Page(parser::parse_compound_page(
                    &xml_dir, ref_id,
                ))),
                _ => unimplemented!(),
            }
        })
        .collect();

    write_navigation(&html_dir, &compounds);

    let tera = tera::Tera::new("templates/*.html").unwrap();
    let relink = create_relinker(&source_dir, &html_dir, &compounds);

    compounds.into_iter().par_bridge().for_each(|compound| {
        match compound {
            Compound::File(mut file) => {
                let file_dir = file.common.source.rsplitn(2, '/').nth(1).unwrap();
                let file_path = source_dir.join(&file.common.source);
                let file_path = file_path.to_str().unwrap();

                // update deferred links
                for scope in &mut file.scopes {
                    for section in &mut scope.sections {
                        for member in &mut section.members {
                            member.definition = relink(&member.definition, file_dir, &file_path);
                            member.description = relink(&member.description, file_dir, &file_path);
                        }
                    }
                }

                let file_name = html_dir.join(format!("{}.html", file.common.ref_id));
                write_compound_file(&tera, &file_name, &file);
            }
            Compound::Page(mut page) => {
                let file_dir = page.common.source.rsplitn(2, '/').nth(1).unwrap_or(".");
                let file_path = source_dir.join(&page.common.source);
                let file_path = file_path.to_str().unwrap();

                // update deferred links
                page.description = relink(&page.description, file_dir, &file_path);

                let file_name = html_dir.join(format!("{}.html", page.common.ref_id));
                write_compound_page(&tera, &file_name, &page);
            }
        }
    });
}

enum Compound {
    File(parser::File),
    Page(parser::Page),
}

fn create_relinker(
    source_dir: &Path,
    html_dir: &Path,
    compounds: &[Compound],
) -> Box<dyn Fn(&str, &str, &str) -> String + Sync> {
    let re_refs = regex::Regex::new("refid://([^\"]*)").unwrap();
    let re_imgs = regex::Regex::new("doxyimg://([^\"]*)").unwrap();
    let ref_to_path = create_ref_to_path_map(compounds);
    let source_dir = source_dir.to_str().unwrap().to_owned();
    let html_dir = html_dir.to_str().unwrap().to_owned();

    Box::new(move |v, img_dir, _file_path| -> String {
        let v = re_refs.replace_all(v, |caps: &regex::Captures| {
            let cap = &caps[1];
            ref_to_path.get(cap).map(|s| s.as_str()).unwrap_or_else(|| {
                // println!("WARNING: {}: ref not found: {}", file_path, cap);
                "refid://not-found"
            })
        });
        let v = re_imgs.replace_all(&v, |caps: &regex::Captures| {
            let rel_path = percent_encoding::percent_decode_str(&caps[1])
                .decode_utf8()
                .unwrap();
            let source = format!("{}/{}/{}", &source_dir, &img_dir, rel_path);
            if std::path::PathBuf::from(&source).exists() {
                let filename = source.rsplit('/').next().unwrap();
                // TODO: ensure uniqueness!
                let target = format!("{}/images/{}", html_dir, filename);
                // TODO: check timestamps or similar!
                if !std::path::PathBuf::from(&target).exists() {
                    std::fs::copy(&source, target).unwrap();
                }
                format!("images/{}", filename)
            } else {
                println!("WARNING: img not found: {}", source);
                caps[1].to_owned()
            }
        });
        v.into_owned()
    })
}

fn create_ref_to_path_map(compounds: &[Compound]) -> std::collections::HashMap<String, String> {
    let mut ref_to_path = std::collections::HashMap::<String, String>::new();
    for compound in compounds {
        match compound {
            Compound::File(file) => {
                let filename = format!("{}.html", file.common.ref_id);
                ref_to_path.insert(file.common.ref_id.clone(), filename.clone());
                for class in &file.scopes {
                    ref_to_path.insert(
                        class.ref_id.clone(),
                        format!("{}#{}", filename, class.ref_id),
                    );
                    for section in &class.sections {
                        for member in &section.members {
                            ref_to_path.insert(
                                member.ref_id.clone(),
                                format!("{}#{}", filename, member.ref_id),
                            );
                            for enum_value in &member.enum_values {
                                ref_to_path.insert(
                                    enum_value.ref_id.clone(),
                                    format!("{}#{}", filename, enum_value.ref_id),
                                );
                            }
                        }
                    }
                }
            }
            Compound::Page(page) => {
                let filename = format!("{}.html", page.common.ref_id);
                ref_to_path.insert(page.common.ref_id.clone(), filename);
                // TODO: add paragraph links
            }
        }
    }
    ref_to_path
}

fn copy_static_files(html_dir: &Path) -> std::io::Result<()> {
    let target_dir = html_dir.join("static");
    std::fs::create_dir_all(&target_dir)?;

    let current_exe = std::env::current_exe()?;
    let my_path = current_exe.parent().unwrap();
    let source_dir = if my_path.join("../../static").exists() {
        my_path.join("../../static")
    } else {
        my_path.join("static")
    };

    for entry in std::fs::read_dir(source_dir)? {
        let entry = entry?;
        let from_path = entry.path();
        let to_path = target_dir.join(from_path.file_name().unwrap());
        std::fs::copy(from_path, to_path)?;
    }
    Ok(())
}

fn write_compound_file(tera: &Tera, file_name: &Path, file: &parser::File) {
    let context = tera::Context::from_serialize(file).unwrap();
    let content = tera.render("file.html", &context).unwrap();
    let content = html_minifier::minify(content).unwrap();
    let mut f = std::fs::File::create(file_name).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

fn write_compound_page(tera: &Tera, file_name: &Path, page: &parser::Page) {
    let context = tera::Context::from_serialize(page).unwrap();
    let content = tera.render("page.html", &context).unwrap();
    let content = html_minifier::minify(content).unwrap();
    let mut f = std::fs::File::create(file_name).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

#[derive(Serialize)]
struct Nav<'a> {
    sections: Vec<NavSection<'a>>,
}

#[derive(Serialize)]
struct NavSection<'a> {
    title: &'a str,
    children: Vec<NavLink<'a>>,
}

#[derive(Serialize)]
struct NavLink<'a> {
    href: String,
    text: &'a str,
}

fn to_nav_json_recursive(
    common: &parser::PageCommon,
    ref_to_compound: &std::collections::HashMap<&str, &Compound>,
) -> json::JsonValue {
    // page = [[name, href], [subpage1, subpage2, ...]]

    let href = format!("{}.html", common.ref_id);
    let this_page = json::array![common.title.as_str(), href];
    let mut subpages = json::array![];

    for subpage_ref in common.subpage_refs.iter() {
        let subpage = ref_to_compound[subpage_ref.as_str()];
        if let Compound::Page(subpage) = subpage {
            let subpage = to_nav_json_recursive(&subpage.common, &ref_to_compound);
            subpages.push(subpage).unwrap();
        }
    }

    return json::array![this_page, subpages];
}

fn write_navigation(html_dir: &Path, compounds: &[Compound]) {
    let mut ref_to_parent = std::collections::HashMap::<&str, &str>::new();
    let mut ref_to_compound = std::collections::HashMap::<&str, &Compound>::new();
    for compound in compounds {
        let common = match compound {
            Compound::File(file) => &file.common,
            Compound::Page(page) => &page.common,
        };
        ref_to_compound.insert(&common.ref_id, &compound);
        for child in common.subpage_refs.iter() {
            ref_to_parent.insert(&child, &common.ref_id);
        }
    }

    let mut doc = json::array![];

    for compound in compounds {
        if let Compound::Page(page) = compound {
            let common = &page.common;
            if ref_to_parent.get(common.ref_id.as_str()).is_some() {
                continue; // skip non-root pages
            }
            doc.push(to_nav_json_recursive(common, &ref_to_compound))
                .unwrap();
        } else {
            let common = match compound {
                Compound::File(file) => &file.common,
                Compound::Page(page) => &page.common,
            };

            let snippets: Vec<&str> = common.source.split('/').collect();
            let section =
                snippets[0..snippets.len() - 1]
                    .iter()
                    .fold(&mut doc, |section, snippet| {
                        let mut found_index: Option<usize> = None;
                        for i in 0..section.len() {
                            if section[i][0][0].as_str().unwrap() == *snippet {
                                found_index = Some(i);
                            }
                        }
                        match found_index {
                            Some(found_index) => &mut section[found_index][1],
                            None => {
                                let header = json::array![*snippet, ""];
                                section.push(json::array![header, json::array![]]).unwrap();
                                let last_idx = section.len() - 1;
                                &mut section[last_idx][1]
                            }
                        }
                    });
            let href = format!("{}.html", common.ref_id);
            let this_page = json::array![common.title.as_str(), href.as_str()];
            section
                .push(json::array![this_page, json::array![]])
                .unwrap();
        }
    }

    let f = std::fs::File::create(html_dir.join("nav.js")).unwrap();
    let mut f = std::io::BufWriter::new(f);
    f.write_all(b"let nav=").unwrap();
    doc.write(&mut f).unwrap();
}
