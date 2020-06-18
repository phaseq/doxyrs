use gumdrop::Options;
use rayon::prelude::*;
use roxmltree::Document;
use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use tera::Tera;

mod parser;

#[derive(Debug, Options)]
struct Cli {
    #[options(help = "Print help message")]
    help: bool,

    #[options(
        no_short,
        help = "Directory containing the doxygen XML output (required)"
    )]
    xml: String,

    #[options(help = "HTML output directory (required)")]
    output: String,
}

fn main() {
    let opt = Cli::parse_args_default_or_exit();
    if opt.xml.is_empty() || opt.output.is_empty() {
        if opt.xml.is_empty() {
            println!("missing required argument: --xml");
        }
        if opt.output.is_empty() {
            println!("missing required argument: --output");
        }
        println!("\n{}", Cli::usage());
        std::process::exit(1);
    }

    let xml_dir = PathBuf::from(opt.xml);
    if !xml_dir.exists() {
        println!("--xml path not found: {}", xml_dir.to_string_lossy());
    }

    let html_dir = PathBuf::from(opt.output);
    std::fs::create_dir_all(&html_dir).unwrap();

    copy_static_files(&html_dir).unwrap();

    let img_dir = "./"; // TODO

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

    let compounds: Vec<Compound> = index
        .children()
        .filter(|n| {
            n.has_tag_name("compound")
                && n.attribute("kind")
                    .map(|kind| kind == "file" || kind == "page")
                    .unwrap()
        })
        .par_bridge()
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
    let relink = create_relinker(&compounds, img_dir);

    compounds.into_iter().par_bridge().for_each(|compound| {
        match compound {
            Compound::File(mut file) => {
                // update deferred links
                for scope in &mut file.scopes {
                    for section in &mut scope.sections {
                        for member in &mut section.members {
                            member.definition = relink(&member.definition);
                            member.description = relink(&member.description);
                        }
                    }
                }

                let file_name = html_dir.join(format!("{}.html", file.common.ref_id));
                write_compound_file(&tera, &file_name, &file);
            }
            Compound::Page(mut page) => {
                // update deferred links
                page.description = relink(&page.description);

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

fn create_relinker(compounds: &[Compound], img_dir: &str) -> Box<dyn Fn(&str) -> String + Sync> {
    let re_refs = regex::Regex::new("refid://([^\"]*)").unwrap();
    let re_imgs = regex::Regex::new("doxyimg://([^\"]*)").unwrap();
    let ref_to_path = create_ref_to_path_map(compounds);
    let img_dir = img_dir.to_owned();

    Box::new(move |v| -> String {
        let v = re_refs.replace_all(v, |caps: &regex::Captures| {
            let cap = &caps[1];
            ref_to_path.get(cap).map(|s| s.as_str()).unwrap_or_else(|| {
                //println!("WARNING: ref not found: {}", cap);
                "refid://not-found"
            })
        });
        let v = re_imgs.replace_all(&v, |caps: &regex::Captures| {
            let path = format!("{}{}", &img_dir, &caps[1]);
            if std::path::PathBuf::from(&path).exists() {
                format!("file://{}", path)
            } else {
                // println!("WARNING: img not found: {}", &caps[1]);
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
    //let content = html_minifier::minify(content).unwrap();
    let mut f = std::fs::File::create(file_name).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

fn write_compound_page(tera: &Tera, file_name: &Path, page: &parser::Page) {
    let context = tera::Context::from_serialize(page).unwrap();
    let content = tera.render("page.html", &context).unwrap();
    //let content = html_minifier::minify(content).unwrap();
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

fn write_navigation(html_dir: &Path, compounds: &[Compound]) {
    let mut doc = json::object! {"sections": json::object!{}, "pages": json::array![]};

    for compound in compounds {
        let common = match compound {
            Compound::File(file) => &file.common,
            Compound::Page(page) => &page.common,
        };
        let snippets: Vec<_> = common.source.split('/').collect();
        let mut section = &mut doc;
        for snippet in &snippets[0..snippets.len() - 1] {
            if section["sections"][*snippet].is_null() {
                section["sections"]
                    .insert(
                        snippet,
                        json::object! {
                            "sections": json::object!{},
                            "pages": json::array![]
                        },
                    )
                    .unwrap();
            }
            section = &mut section["sections"][*snippet];
        }
        let href = format!("{}.html", common.ref_id);
        section["pages"]
            .push(json::array![common.title.as_str(), href])
            .unwrap();
    }

    let f = std::fs::File::create(html_dir.join("nav.js")).unwrap();
    let mut f = std::io::BufWriter::new(f);
    f.write_all(b"let nav=").unwrap();
    doc.write(&mut f).unwrap();
}
