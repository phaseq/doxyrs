use roxmltree::{Document, Node};
use serde::Serialize;

// TODOs: mwVerifierNestedEnums.hpp

#[derive(Serialize)]
pub struct Page {
    pub ref_id: String,
    pub title: String,
    pub description: String,
}

#[derive(Serialize)]
pub struct File {
    pub ref_id: String,
    pub name: String,
    pub scopes: Vec<Scope>,
}

#[derive(Serialize)]
pub struct Scope {
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

pub fn parse_compound_page(xml_dir: &str, ref_id: &str) -> Page {
    let file_name = xml_dir.to_owned() + ref_id + ".xml";
    let content = std::fs::read_to_string(file_name).unwrap();
    let doc = Document::parse(&content).unwrap();
    let compounddef = doc
        .root_element()
        .children()
        .find(|n| n.tag_name().name() == "compounddef")
        .unwrap();

    let title = compounddef.get_child_value("title").unwrap().to_owned();

    let description = parse_text(compounddef.get_child("detaileddescription").unwrap());

    Page {
        ref_id: ref_id.to_owned(),
        title,
        description,
    }
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

    let mut scopes = vec![];

    for node in compounddef.children() {
        match node.tag_name().name() {
            "innerclass" | "innernamespace" => {
                let inner_ref_id = node.attribute("refid").unwrap();
                if let Some(scope) = parse_compound_scope(&name, xml_dir, inner_ref_id) {
                    if !scope.sections.is_empty() {
                        scopes.push(scope);
                    }
                }
            }
            _ => {} // TODO: fail here
        }
    }
    File {
        ref_id: ref_id.to_owned(),
        name,
        scopes,
    }
}

fn parse_compound_scope(parent_file_name: &str, xml_dir: &str, ref_id: &str) -> Option<Scope> {
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

    let ref_id = compounddef.attribute("id").unwrap().to_owned();
    let kind = compounddef.attribute("kind").unwrap().to_owned();

    let name = render_scope_name(&compounddef.get_child_value("compoundname").unwrap());

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
                        && n.get_child("location")
                            .unwrap()
                            .attribute("file")
                            .unwrap()
                            .ends_with(parent_file_name)
                })
                .map(parse_member)
                .collect();
            Section { name, members }
        })
        .filter(|s| !s.members.is_empty())
        .collect();
    Some(Scope {
        ref_id,
        name,
        kind,
        sections,
    })
}

fn render_scope_name(name: &str) -> String {
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
            "<span class=\"member_name\">{}</span>{} → <span class=\"rettype\">{}</span>",
            name, args, return_type
        ),
        "function" => format!("<span class=\"member_name\">{}</span>{}", name, args),
        "typedef" => format!(
            "<span class=\"keyword\">using</span> <span class=\"member_name\">{}</span> = <span class=\"type\">{}</span>",
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
            s.push_str(&format!("<span class=\"keyword\">enum</span> <span class=\"member_name\">{}</span> {{<br>", name));
            for value in memberdef.children().filter(|c| c.has_tag_name("enumvalue")) {
                s.push_str(&format!(
                    "&nbsp;&nbsp;&nbsp;&nbsp;<span class=\"declname\">{}</span> <span class=\"defval\">{}</span><br/>",
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
                            "<tr><td><span class=\"declname\">{}</span>:</td><td>{}</td></tr>",
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
            "image" => {
                let path = c.attribute("name").unwrap();
                s.push_str(&format!("<img src=\"doxyimg://{}\" />", path))
            }
            // tag pass-through
            "bold" => {
                s.push_str(&format!("<bold>{}</bold>", parse_text(c)));
            }
            "emphasis" => {
                s.push_str(&format!("<em>{}</em>", parse_text(c)));
            }
            "verbatim" | "preformatted" => {
                s.push_str(&format!("<pre>{}</pre>", parse_text(c)));
            }
            "computeroutput" => {
                s.push_str(&format!("<tt>{}</tt>", parse_text(c)));
            }
            "superscript" => {
                s.push_str(&format!("<sup>{}</sup>", parse_text(c)));
            }
            "subscript" => {
                s.push_str(&format!("<sub>{}</sub>", parse_text(c)));
            }
            tag @ "sect1" | tag @ "sect2" | tag @ "sect3" | tag @ "sect4" | tag @ "sect5" => {
                let title = c.children().find(|n| n.has_tag_name("title")).unwrap();
                let level = tag.chars().nth(4).unwrap().to_digit(10).unwrap() + 1;
                let id = c.attribute("id").unwrap();
                // TODO: generate proper anchors. Currently:
                // sect.id = md_Developer_guide_Cutsim_Gouge_excess_1cutsim_ge_draw_mode_offset
                // ulink.url = #cutsim_ge_draw_mode_offset
                s.push_str(&format!("<a name=\"{}\"></a>", id));
                s.push_str(&format!("<h{}>{}</h{}>", level, parse_text(title), level));
                s.push_str(&parse_text(c));
            }
            "title" => {} // handled by sectN
            "heading" => {
                let level = c.attribute("level").unwrap().parse::<usize>().unwrap();
                s.push_str(&format!("<h{}>{}</h{}>", level, parse_text(c), level));
            }
            // html escape codes
            "linebreak" => {
                s.push_str("<br/>");
            }
            "hruler" => {
                s.push_str("<hr/>");
            }
            "sp" | "nonbreakablespace" => {
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
