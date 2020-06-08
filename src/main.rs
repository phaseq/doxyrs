//use rayon::prelude::*;
use roxmltree::Document;
use std::io::Write;
use tera::Tera;

mod parser;

fn main() {
    let xml_dir = "/home/fabianb/Dev/Moduleworks/dev/tools/doxysync/xml/";
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

    let files: Vec<parser::File> = index
        .children()
        .filter(|n| n.has_tag_name("compound") && n.attribute("kind").unwrap() == "file")
        //.par_bridge()
        .filter_map(|compound| {
            let name = compound
                .children()
                .find(|n| n.has_tag_name("name"))
                .unwrap()
                .text()
                .unwrap();
            if name != "mwMachSimVerifier.hpp" {
                return None;
            }
            let ref_id = compound.attribute("refid").unwrap();
            Some(parser::parse_compound_file(&xml_dir, ref_id))
        })
        .collect();

    let mut tera = tera::Tera::new("templates/*.html").unwrap();
    tera.register_filter("reflink", generate_ref_linker(&html_dir, &files));

    for file in &files {
        let file_name = format!("{}/{}.html", html_dir, file.ref_id);
        write_compound_file(&tera, &file_name, &file);
    }
}

fn generate_ref_linker(html_dir: &str, files: &[parser::File]) -> impl tera::Filter {
    let mut ref_to_path = std::collections::HashMap::<String, String>::new();
    for file in files {
        let filename = format!("{}/{}.html", html_dir, file.ref_id);
        ref_to_path.insert(file.ref_id.clone(), filename.clone());
        for class in &file.classes {
            ref_to_path.insert(
                class.ref_id.clone(),
                format!("{}#{}", filename, class.ref_id),
            );
        }
    }

    let re = regex::Regex::new("refid://([^\"]*)").unwrap();

    Box::new(
        move |value: &tera::Value,
              _args: &std::collections::HashMap<String, tera::Value>|
              -> tera::Result<tera::Value> {
            match value.as_str() {
                Some(v) => Ok(tera::to_value(re.replace_all(
                    v,
                    |caps: &regex::Captures| -> &str {
                        let cap = &caps[1];
                        ref_to_path.get(cap).map(|s| s.as_str()).unwrap_or_else(|| {
                            //println!("WARNING: ref not found: {}", cap);
                            "refid://not-found"
                        })
                    },
                ))
                .unwrap()),
                None => Err("reflink filter is only supported for string values!".into()),
            }
        },
    )
}

fn write_compound_file(tera: &Tera, file_name: &str, file: &parser::File) {
    let content = tera
        .render("file.html", &tera::Context::from_serialize(file).unwrap())
        .unwrap();
    let mut f = std::fs::File::create(file_name).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}
