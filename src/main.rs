use rayon::prelude::*;
use roxmltree::Document;
use std::io::Write;
use tera::Tera;

mod parser;

enum Compound {
    File(parser::File),
    Page(parser::Page),
}

fn main() {
    let xml_dir = "/home/fabianb/Dev/Moduleworks/dev/tools/doxysync/xml/";
    let img_dir = "/home/fabianb/Dev/Moduleworks/dev/doc/Developer_guide/Cutsim/";
    let html_dir = "/home/fabianb/Dev/Moduleworks/dev/tools/doxysync/html/";

    // read the index file
    let index_path = xml_dir.to_owned() + "index.xml";
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

    let tera = tera::Tera::new("templates/*.html").unwrap();
    let relink = create_relinker(&compounds, html_dir, img_dir);

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

                let file_name = format!("{}{}.html", html_dir, file.ref_id);
                write_compound_file(&tera, &file_name, &file);
            }
            Compound::Page(mut page) => {
                // update deferred links
                page.description = relink(&page.description);

                let file_name = format!("{}{}.html", html_dir, page.ref_id);
                write_compound_page(&tera, &file_name, &page);
            }
        }
    });
}

fn create_relinker(
    compounds: &[Compound],
    html_dir: &str,
    img_dir: &str,
) -> Box<dyn Fn(&str) -> String + Sync> {
    let re_refs = regex::Regex::new("refid://([^\"]*)").unwrap();
    let re_imgs = regex::Regex::new("doxyimg://([^\"]*)").unwrap();
    let ref_to_path = create_ref_to_path_map(html_dir, compounds);
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

fn create_ref_to_path_map(
    html_dir: &str,
    compounds: &[Compound],
) -> std::collections::HashMap<String, String> {
    let mut ref_to_path = std::collections::HashMap::<String, String>::new();
    for compound in compounds {
        match compound {
            Compound::File(file) => {
                let filename = format!("{}{}.html", html_dir, file.ref_id);
                ref_to_path.insert(file.ref_id.clone(), filename.clone());
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
                let filename = format!("{}{}.html", html_dir, page.ref_id);
                ref_to_path.insert(page.ref_id.clone(), filename.clone());
                // TODO: add paragraph links
            }
        }
    }
    ref_to_path
}

fn write_compound_file(tera: &Tera, file_name: &str, file: &parser::File) {
    let content = tera
        .render("file.html", &tera::Context::from_serialize(file).unwrap())
        .unwrap();
    let mut f = std::fs::File::create(file_name).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

fn write_compound_page(tera: &Tera, file_name: &str, page: &parser::Page) {
    let content = tera
        .render("page.html", &tera::Context::from_serialize(page).unwrap())
        .unwrap();
    let mut f = std::fs::File::create(file_name).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}
