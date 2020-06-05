use roxmltree::{Document, Node};
use serde::Serialize;
use std::io::Write;
use tera::Tera;

fn main() {
    let xml_dir = "/home/fabianb/Dev/Moduleworks/dev/tools/doxysync/xml/";
    let html_dir = "/home/fabianb/Dev/Moduleworks/dev/tools/doxysync/html/";

    let tera = tera::Tera::new("templates/*.html").unwrap();

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
    for compound in index.children().filter(|n| n.has_tag_name("compound")) {
        let kind = compound.attribute("kind").unwrap();
        if kind != "file" {
            continue;
        }
        let name = compound
            .children()
            .find(|n| n.has_tag_name("name"))
            .unwrap()
            .text()
            .unwrap();
        if name != "mwMachSimVerifier.hpp" {
            continue;
        }
        let ref_id = compound.attribute("refid").unwrap();

        let file = parse_compound_file(&xml_dir, ref_id);
        let file_name = format!("{}/{}.html", html_dir, file.ref_id);
        write_compound_file(&tera, &file_name, &file);
    }
}

fn write_compound_file(tera: &Tera, file_name: &str, file: &CompoundFile) {
    let content = tera
        .render("file.html", &tera::Context::from_serialize(file).unwrap())
        .unwrap();
    let mut f = std::fs::File::create(file_name).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

#[derive(Serialize)]
struct CompoundFile {
    ref_id: String,
    name: String,
    classes: Vec<CompoundClass>,
}

fn parse_compound_file(xml_dir: &str, ref_id: &str) -> CompoundFile {
    let file_name = xml_dir.to_owned() + ref_id + ".xml";
    let content = std::fs::read_to_string(file_name).unwrap();
    let doc = Document::parse(&content).unwrap();
    let compounddef = doc
        .root_element()
        .children()
        .find(|n| n.tag_name().name() == "compounddef")
        .unwrap();

    let name = compounddef
        .get_child_value("compoundname")
        .unwrap()
        .to_owned();

    let mut classes = vec![];

    for node in compounddef.children() {
        match node.tag_name().name() {
            "innerclass" => {
                let ref_id = node.attribute("refid").unwrap();
                let class = parse_compound_class(xml_dir, ref_id);
                classes.push(class);
            }
            _ => {} // TODO: fail here
        }
    }
    CompoundFile {
        ref_id: ref_id.to_owned(),
        name,
        classes,
    }
}

#[derive(Serialize)]
struct CompoundClass {
    ref_id: String,
    name: String,
    sections: Vec<CompoundSection>,
}
#[derive(Serialize)]
struct CompoundSection {
    name: Option<String>,
    members: Vec<CompoundMember>,
}
#[derive(Serialize)]
struct CompoundMember {
    ref_id: String,
    definition: String,
    description: String,
}
fn parse_compound_class(xml_dir: &str, ref_id: &str) -> CompoundClass {
    let file_name = xml_dir.to_owned() + ref_id + ".xml";
    let content = std::fs::read_to_string(file_name);
    /*if content.is_err() {
        return CompoundClass {
            ref_id: ref_id.to_owned(),
            name: "".to_owned(),
            sections: vec![],
        }; // TODO: remove
    }*/
    let content = content.unwrap();
    let doc = Document::parse(&content).unwrap();
    let compounddef = doc
        .root_element()
        .children()
        .find(|n| n.has_tag_name("compounddef"))
        .unwrap();

    let ref_id = compounddef.attribute("id").unwrap().to_owned();

    let name = compounddef
        .get_child_value("compoundname")
        .unwrap()
        .to_owned();

    let sections = compounddef
        .children()
        .filter(|n| n.has_tag_name("sectiondef"))
        .map(|sectiondef| {
            let name = sectiondef.get_child_value("header").map(|v| v.to_owned());
            let members = sectiondef
                .children()
                .filter(|n| {
                    n.has_tag_name("memberdef")
                        && n.attribute("prot").unwrap() == "public"
                        && n.attribute("kind").unwrap() != "friend"
                })
                .map(|memberdef| {
                    let ref_id = memberdef.attribute("id").unwrap().to_owned();
                    let return_type = parse_text(memberdef.get_child("type").unwrap());
                    let name = memberdef
                        .get_child_value("name")
                        .map(|v| tera::escape_html(v));
                    let args = memberdef
                        .get_child_value("argsstring")
                        .map(|v| tera::escape_html(v)); // TODO: extract from param struct
                    let definition = match memberdef.attribute("kind").unwrap() {
                        "function" => {
                            format!("{}{} -> {}", name.unwrap(), args.unwrap(), return_type)
                        }
                        "typedef" => format!("using {} = {}", name.unwrap(), return_type),
                        "variable" => format!(
                            "{}{}",
                            return_type,
                            tera::escape_html(
                                memberdef.get_child_value("initializer").unwrap_or_default()
                            )
                        ),
                        "enum" => {
                            let mut s = String::new();
                            s.push_str(&format!("enum {} {{<br>", name.unwrap()));
                            for value in
                                memberdef.children().filter(|c| c.has_tag_name("enumvalue"))
                            {
                                s.push_str(&format!(
                                    "&nbsp;&nbsp;&nbsp;&nbsp;{}{}<br/>",
                                    value.get_child_value("name").unwrap(),
                                    value.get_child_value("initializer").unwrap_or_default()
                                ))
                            }
                            s.push_str("}");
                            s
                        }
                        //_ => format!("{}", memberdef.get_child_value("definition").unwrap()),
                        _ => panic!(
                            "not implemented: {} ({})",
                            memberdef.attribute("kind").unwrap(),
                            ref_id
                        ),
                    };

                    let description = /*if definition.contains("GetMoveDescription")*/ {
                        let brief = parse_text(memberdef.get_child("briefdescription").unwrap());
                        let detailed =
                            parse_text(memberdef.get_child("detaileddescription").unwrap());
                        brief.to_owned() + &detailed
                    } /*else {
                        "".to_owned()
                    }*/;
                    CompoundMember {
                        ref_id,
                        definition,
                        description,
                    }
                })
                .collect();
            CompoundSection { name, members }
        })
        .collect();
    CompoundClass {
        ref_id,
        name,
        sections,
    }
}

fn parse_text(node: Node) -> String {
    let mut s = String::new();
    for c in node.children() {
        match c.tag_name().name() {
            "" => {
                s.push_str(&tera::escape_html(c.text().unwrap()));
            }
            "para" => {
                s.push_str(&format!("<p>{}</p>", parse_text(c)));
            }
            "simplesect" => {
                s.push_str(&format!(
                    "{}: {}",
                    c.attribute("kind").unwrap(),
                    parse_text(c.get_child("para").unwrap())
                ));
            }
            "ref" => {
                // TODO: add link
                s.push_str(&format!(
                    "<a href=\"refid://{}\">{}</a>",
                    c.attribute("refid").unwrap(),
                    c.text().unwrap()
                ));
            }
            "ulink" => {
                s.push_str(&format!(
                    "<a href=\"{}\">{}</a>",
                    c.attribute("url").unwrap(),
                    parse_text(c)
                ));
            }
            "xrefsect" => {
                // ignore for now, seems to only link against deprecated page
            }
            "parameterlist" => {
                s.push_str("<table class=\"parameterlist\">");
                for item in c.children().filter(|n| n.has_tag_name("parameteritem")) {
                    let name = item.get_child("parameternamelist").unwrap();
                    if let Some(name) = name.get_child_value("parametername") {
                        let description =
                            parse_text(item.get_child("parameterdescription").unwrap());
                        s.push_str(&format!(
                            "<tr><td>{}:</td><td>{}</td></tr>",
                            tera::escape_html(name),
                            description
                        ));
                    }
                }
                s.push_str("</table>");
            }
            tag @ "itemizedlist" | tag @ "orderedlist" => {
                let tag = match tag {
                    "itemizedlist" => "ul",
                    "orderedlist" => "ol",
                    _ => unimplemented!(),
                };
                s.push_str(&format!("<{}>", tag));
                for item in c.children().filter(|n| n.has_tag_name("listitem")) {
                    s.push_str(&format!("<li>{}</li>", parse_text(item)));
                }
                s.push_str(&format!("</{}>", tag));
            }
            "programlisting" => {
                s.push_str("<pre>");
                for item in c.children().filter(|n| n.has_tag_name("codeline")) {
                    s.push_str(&format!("{}<br/>", parse_text(item)));
                }
                s.push_str("</pre>");
            }
            "highlight" => {
                s.push_str(&format!(
                    "<span class=\"highlight-{}\">{}</span>",
                    c.attribute("class").unwrap(),
                    parse_text(c)
                ));
            }
            // tag pass-through
            "bold" => {
                s.push_str(&format!("<bold>{}</bold>", parse_text(c)));
            }
            "emphasis" => {
                s.push_str(&format!("<em>{}</em>", parse_text(c)));
            }
            "verbatim" | "computeroutput" => {
                s.push_str(&format!("<pre>{}</pre>", parse_text(c)));
            }
            // html escape codes
            "linebreak" => {
                s.push_str("&nbsp;");
            }
            "sp" => {
                s.push_str("&nbsp;");
            }
            tag @ "ndash" | tag @ "mdash" | tag @ "zwj" => {
                s.push_str(&format!("&{};", tag));
            }
            _ => {
                println!("WARNING: '{}' not implemented!", c.tag_name().name());
            }
        }
    }
    return s;
}

pub trait NodeExt<'n1, 'n2> {
    fn get_child<'a>(&'a self, tag: &str) -> Option<Node<'n1, 'n2>>;
    fn get_child_value<'a>(&'a self, tag: &str) -> Option<&'a str>;
}
impl<'n1, 'n2> NodeExt<'n1, 'n2> for Node<'n1, 'n2> {
    fn get_child<'a>(&'a self, tag: &str) -> Option<Node<'n1, 'n2>> {
        self.children().find(|n| n.has_tag_name(tag))
    }
    fn get_child_value<'a>(&'a self, tag: &str) -> Option<&'a str> {
        self.children()
            .find(|n| n.has_tag_name(tag))
            .map(|n| n.text())
            .flatten()
    }
}
