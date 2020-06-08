use roxmltree::{Document, Node};
use serde::Serialize;

#[derive(Serialize)]
pub struct File {
    pub ref_id: String,
    pub name: String,
    pub classes: Vec<Class>,
}

#[derive(Serialize)]
pub struct Class {
    pub ref_id: String,
    pub name: String,
    pub kind: String,
    pub sections: Vec<Section>,
}

#[derive(Serialize)]
pub struct Section {
    pub name: Option<String>,
    pub members: Vec<Member>,
}

#[derive(Serialize)]
pub struct Member {
    pub ref_id: String,
    pub definition: String,
    pub description: String,
}

pub fn parse_compound_file(xml_dir: &str, ref_id: &str) -> File {
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
                if let Some(class) = parse_compound_class(&name, xml_dir, ref_id) {
                    classes.push(class);
                }
            }
            _ => {} // TODO: fail here
        }
    }
    File {
        ref_id: ref_id.to_owned(),
        name,
        classes,
    }
}

fn parse_compound_class(parent_file: &str, xml_dir: &str, ref_id: &str) -> Option<Class> {
    let file_name = xml_dir.to_owned() + ref_id + ".xml";
    let content = std::fs::read_to_string(file_name);
    if content.is_err() {
        return None; // TODO: remove
    }
    let content = content.unwrap();
    let doc = Document::parse(&content).unwrap();
    let compounddef = doc
        .root_element()
        .children()
        .find(|n| n.has_tag_name("compounddef"))
        .unwrap();

    if let Some(main_header) = compounddef.get_child_value("includes") {
        if main_header != parent_file {
            return None; // list class just in the main header, not for forward decls
        }
    }

    let ref_id = compounddef.attribute("id").unwrap().to_owned();
    let kind = compounddef.attribute("kind").unwrap().to_owned();

    let name = render_class_name(&compounddef.get_child_value("compoundname").unwrap());

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
                .map(parse_member)
                .collect();
            Section { name, members }
        })
        .collect();
    Some(Class {
        ref_id,
        name,
        kind,
        sections,
    })
}

fn render_class_name(name: &str) -> String {
    let mut name = tera::escape_html(name).replace("::", "::&#8203;");
    if let Some(pos) = name.rfind("::&#8203;") {
        name.insert_str(pos + 2, "</span><span class=\"name_part\">");
        name.insert_str(0, "<span class=\"namespace_part\">");
    } else {
        name.insert_str(0, "<span class=\"name_part\">");
    }
    name.push_str("</span>");
    name
}

fn parse_member(memberdef: Node) -> Member {
    let ref_id = memberdef.attribute("id").unwrap().to_owned();
    let return_type = parse_text(memberdef.get_child("type").unwrap());
    let name = memberdef
        .get_child_value("name")
        .map(|v| tera::escape_html(v))
        .unwrap();
    let args = render_member_args(memberdef);

    let definition = match memberdef.attribute("kind").unwrap() {
        "function" if !return_type.is_empty() => format!(
            "<span class=\"member_name\">{}</span>{} → <span class=\"type\">{}</span>",
            name, args, return_type
        ),
        "function" => format!("<span class=\"member_name\">{}</span>{}", name, args),
        "typedef" => format!(
            "using <span class=\"member_name\">{}</span> = <span class=\"type\">{}</span>",
            name, return_type
        ),
        "variable" => format!(
            "<span class=\"type\">{}</span> <span class=\"member_name\">{}</span> <span class=\"defval\">{}</span>",
            return_type,
            name,
            tera::escape_html(memberdef.get_child_value("initializer").unwrap_or_default())
        ),
        "enum" => {
            let mut s = String::new();
            s.push_str(&format!("enum {} {{<br>", name));
            for value in memberdef.children().filter(|c| c.has_tag_name("enumvalue")) {
                s.push_str(&format!(
                    "&nbsp;&nbsp;&nbsp;&nbsp;{} {}<br/>",
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

    let description = {
        let brief = parse_text(memberdef.get_child("briefdescription").unwrap());
        let detailed = parse_text(memberdef.get_child("detaileddescription").unwrap());
        brief + &detailed
    };
    Member {
        ref_id,
        definition,
        description,
    }
}

fn render_member_args(memberdef: Node) -> String {
    let args: Vec<_> = memberdef
        .children()
        .filter(|n| n.has_tag_name("param"))
        .map(|param| {
            let mut result = format!(
                "<span class=\"type\">{}</span>",
                parse_text(param.get_child("type").unwrap()).replace(" &amp;", "&amp; ")
            );
            if let Some(declname) = param.get_child("declname") {
                result.push_str(&format!(
                    " <span class=\"declname\">{}</span>",
                    parse_text(declname)
                ));
            }
            if let Some(defval) = param.get_child("defval") {
                result.push_str(&format!(
                    " = <span class=\"defval\">{}</span>",
                    parse_text(defval)
                ));
            }
            result
        })
        .collect();

    let is_multiline = !args.is_empty(); // args.iter().map(|a| a.len() + 2).sum::<usize>() >= 60;
    let newline = "<br/>&nbsp;&nbsp;&nbsp;&nbsp;";
    let mut s = "(".to_owned();
    if is_multiline {
        s.push_str(newline);
    }
    let mut is_first = true;
    for arg in args.iter() {
        if !is_first {
            if is_multiline {
                s.push(',');
                s.push_str(newline);
            } else {
                s.push_str(", ");
            }
        } else {
            is_first = false;
        }
        s.push_str(arg);
    }
    s.push(')');
    s
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
            "table" => {
                s.push_str("<table>");
                for row in c.children().filter(|n| n.has_tag_name("row")) {
                    s.push_str("<tr>");
                    for entry in row.children().filter(|n| n.has_tag_name("entry")) {
                        s.push_str("<td>");
                        s.push_str(&parse_text(entry));
                        s.push_str("</td>");
                    }
                    s.push_str("</tr>");
                }
                s.push_str("</table>");
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
    s
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
