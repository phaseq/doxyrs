use roxmltree::{Document, Node};
use serde::Serialize;
use std::path::Path;

// see here for structure:
// https://raw.githubusercontent.com/doxygen/doxygen/master/templates/xml/compound.xsd

#[derive(Serialize)]
pub struct PageCommon {
    pub ref_id: String,
    pub source: String,
    pub title: String,
    pub has_math: bool,
    pub subpage_refs: Vec<String>,
}

#[derive(Serialize)]
pub struct Page {
    pub common: PageCommon,
    pub description: String,
}

#[derive(Serialize)]
pub struct File {
    pub common: PageCommon,
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
    pub description: Option<String>,
    pub members: Vec<Member>,
}

#[derive(Serialize)]
pub struct Member {
    pub ref_id: String,
    pub definition: String,
    pub description: String,
}

struct Context {
    has_math: bool,
}

pub fn parse_compound_page(xml_dir: &Path, ref_id: &str) -> Page {
    let file_name = xml_dir.join(ref_id.to_owned() + ".xml");
    let content = std::fs::read_to_string(&file_name).unwrap();
    let doc = Document::parse(&content).unwrap();
    let compounddef = doc
        .root_element()
        .children()
        .find(|n| n.tag_name().name() == "compounddef")
        .unwrap();

    let source = compounddef
        .get_child("location")
        .map(|l| l.attribute("file").unwrap())
        .unwrap_or_default()
        .to_owned();

    let title = compounddef.get_child_value("title").unwrap().to_owned();

    let mut context = Context { has_math: false };

    let description = parse_text(
        compounddef.get_child("detaileddescription").unwrap(),
        &mut context,
    );

    let subpage_refs: Vec<_> = compounddef
        .children()
        .filter(|c| c.has_tag_name("innerpage"))
        .map(|n| n.attribute("refid").unwrap().to_owned())
        .collect();

    Page {
        common: PageCommon {
            ref_id: ref_id.to_owned(),
            source,
            title,
            has_math: context.has_math,
            subpage_refs,
        },
        description,
    }
}

pub fn parse_compound_file(xml_dir: &Path, ref_id: &str) -> File {
    let file_name = xml_dir.join(ref_id.to_owned() + ".xml");
    let content = std::fs::read_to_string(file_name).unwrap();
    let doc = Document::parse(&content).unwrap();
    let compounddef = doc
        .root_element()
        .children()
        .find(|n| n.tag_name().name() == "compounddef")
        .unwrap();

    let source = compounddef
        .get_child("location")
        .map(|l| l.attribute("file").unwrap())
        .unwrap_or_default()
        .to_owned();

    let title = compounddef
        .get_child_value("compoundname")
        .unwrap()
        .to_owned();

    let mut scopes = vec![];

    let mut context = Context { has_math: false };

    for node in compounddef.children() {
        match node.tag_name().name() {
            "innerclass" | "innernamespace" => {
                let inner_ref_id = node.attribute("refid").unwrap();
                if let Some(scope) =
                    parse_compound_scope(&title, xml_dir, inner_ref_id, &mut context)
                {
                    if !scope.sections.is_empty() {
                        scopes.push(scope);
                    }
                }
            }
            _ => {} // TODO: fail here
        }
    }
    File {
        common: PageCommon {
            ref_id: ref_id.to_owned(),
            source,
            title,
            has_math: context.has_math,
            subpage_refs: vec![],
        },
        scopes,
    }
}

fn parse_compound_scope(
    parent_file_name: &str,
    xml_dir: &Path,
    ref_id: &str,
    mut context: &mut Context,
) -> Option<Scope> {
    let file_name = xml_dir.join(ref_id.to_owned() + ".xml");
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

    let name = format!(
        "{} <span class=\"kind_part\">{}</span> {}",
        render_templateparamlist(compounddef, &mut context),
        kind,
        render_scope_name(&compounddef.get_child_value("compoundname").unwrap())
    );

    let sections = compounddef
        .children()
        .filter(|n| n.has_tag_name("sectiondef"))
        .map(|sectiondef| {
            let name = sectiondef.get_child_value("header").map(|v| v.to_owned());
            let description = sectiondef
                .get_child("description")
                .map(|d| parse_text(d, &mut context));
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
                .map(|m| parse_member(m, &mut context))
                .collect();
            Section {
                name,
                description,
                members,
            }
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

fn parse_member(memberdef: Node, mut context: &mut Context) -> Member {
    let ref_id = memberdef.attribute("id").unwrap().to_owned();
    let return_type = parse_text(memberdef.get_child("type").unwrap(), &mut context);
    let name = memberdef
        .get_child_value("name")
        .map(|v| tera::escape_html(v))
        .unwrap();
    let template = render_templateparamlist(memberdef, &mut context);
    let args = render_function_args(memberdef, &mut context);

    let definition = match memberdef.attribute("kind").unwrap() {
        "function" | "event" if !return_type.is_empty() => format!(
            "{}<span class=\"member_name\">{}</span>{} → <span class=\"rettype\">{}</span>",
            template, name, args, return_type
        ),
        "function" | "event" => format!("{}<span class=\"member_name\">{}</span>{}", template, name, args),
        "typedef" => format!(
            "{}<span class=\"keyword\">using</span> <span class=\"member_name\">{}</span> = <span class=\"type\">{}</span>",
            template, name, return_type
        ),
        "variable" | "property" => format!(
            "<span class=\"type\">{}</span> <span class=\"member_name\">{}</span> <span class=\"defval\">{}</span>",
            return_type,
            name,
            tera::escape_html(memberdef.get_child_value("initializer").unwrap_or_default())
        ),
        "enum" => {
            // TODO: embed documentation of enum parameters
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
        },
        //_ => format!("{}", memberdef.get_child_value("definition").unwrap()),
        _ => panic!(
            "not implemented: {} ({})",
            memberdef.attribute("kind").unwrap(),
            ref_id
        ),
    };

    let description = {
        let brief = parse_text(
            memberdef.get_child("briefdescription").unwrap(),
            &mut context,
        );
        let detailed = parse_text(
            memberdef.get_child("detaileddescription").unwrap(),
            &mut context,
        );
        brief + &detailed
    };
    Member {
        ref_id,
        definition,
        description,
    }
}

fn render_templateparamlist(memberdef: Node, mut context: &mut Context) -> String {
    if let Some(templateparamlist) = memberdef.get_child("templateparamlist") {
        let mut s = String::new();
        for param in templateparamlist
            .children()
            .filter(|n| n.has_tag_name("param"))
        {
            if !s.is_empty() {
                s.push_str(", ");
            }
            s.push_str(&parse_text(param.get_child("type").unwrap(), &mut context));
            if let Some(defval) = param.get_child_value("defval") {
                s.push_str(&defval);
            }
        }
        format!(
            "<span class=\"templateparamlist\">template &lt;{}&gt;</span>",
            s
        )
    } else {
        "".to_owned()
    }
}

fn render_function_args(memberdef: Node, mut context: &mut Context) -> String {
    let args: Vec<_> = memberdef
        .children()
        .filter(|n| n.has_tag_name("param"))
        .map(|param| {
            let mut result = format!(
                "<span class=\"type\">{}</span>",
                parse_text(param.get_child("type").unwrap(), &mut context)
                    .replace(" &amp;", "&amp;")
                    .replace(" *", "*")
            );
            if let Some(declname) = param.get_child("declname") {
                result.push_str(&format!(
                    " <span class=\"declname\">{}</span>",
                    parse_text(declname, &mut context)
                ));
            }
            if let Some(defval) = param.get_child("defval") {
                result.push_str(&format!(
                    " = <span class=\"defval\">{}</span>",
                    parse_text(defval, &mut context)
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

fn parse_text(node: Node, mut context: &mut Context) -> String {
    let mut s = String::new();
    let mut skip_next_chars = 0usize;
    for c in node.children() {
        match c.tag_name().name() {
            "" => {
                s.push_str(&tera::escape_html(&c.text().unwrap()[skip_next_chars..]));
                skip_next_chars = 0usize;
            }
            "para" => {
                s.push_str(&format!("<p>{}</p>", parse_text(c, &mut context)));
            }
            "simplesect" => {
                let kind = c.attribute("kind").unwrap();
                let css_class = match kind {
                    "warning" | "attention" => Some("alert-warning"),
                    "info" | "note" | "remark" => Some("alert-info"),
                    _ => None,
                };
                if let Some(css_class) = css_class {
                    s.push_str(&format!(
                        "<div class=\"alert {}\">{}</div>",
                        css_class,
                        parse_text(c.get_child("para").unwrap(), &mut context)
                    ));
                } else {
                    let kind_name = match kind {
                        "return" => "Returns".to_owned(),
                        _ => capitalize_first_letter(&kind),
                    };
                    s.push_str(&format!(
                        "<p>{}: {}</p>",
                        kind_name,
                        parse_text(c.get_child("para").unwrap(), &mut context)
                    ));
                }
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
                    parse_text(c, &mut context)
                ));
            }
            "xrefsect" => {
                let xreftitle = c.get_child_value("xreftitle").unwrap();
                let xrefdescription =
                    parse_text(c.get_child("xrefdescription").unwrap(), &mut context);
                let css_class = "alert-danger";
                s.push_str(&format!(
                    "<div class=\"alert {}\"><h5>{}</h5>{}</div>",
                    css_class, xreftitle, xrefdescription
                ));
            }
            "parameterlist" => {
                s.push_str("<table class=\"parameterlist\">");
                for item in c.children().filter(|n| n.has_tag_name("parameteritem")) {
                    let name = item.get_child("parameternamelist").unwrap();
                    if let Some(name) = name.get_child_value("parametername") {
                        let description = parse_text(
                            item.get_child("parameterdescription").unwrap(),
                            &mut context,
                        );
                        s.push_str(&format!(
                            "<tr><td><span class=\"declname\">{}:</span></td><td>{}</td></tr>",
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
                    s.push_str(&format!("<li>{}</li>", parse_text(item, &mut context)));
                }
                s.push_str(&format!("</{}>", tag));
            }
            "table" => {
                s.push_str("<table>");
                for row in c.children().filter(|n| n.has_tag_name("row")) {
                    s.push_str("<tr>");
                    for entry in row.children().filter(|n| n.has_tag_name("entry")) {
                        let is_th = entry.attribute("thead").unwrap() == "yes";
                        if is_th {
                            s.push_str("<th>");
                        } else {
                            s.push_str("<td>");
                        }
                        s.push_str(&parse_text(entry, &mut context));
                        if is_th {
                            s.push_str("</th>");
                        } else {
                            s.push_str("</td>");
                        }
                    }
                    s.push_str("</tr>");
                }
                s.push_str("</table>");
            }
            "programlisting" => {
                s.push_str("<pre class=\"programlisting\">");
                let mut dedent = usize::max_value();
                for codeline in c.children().filter(|n| n.has_tag_name("codeline")) {
                    // count leading spaces
                    let mut n_indents = 0;
                    let mut has_content = false;
                    if let Some(highlight) =
                        codeline.children().find(|n| n.has_tag_name("highlight"))
                    {
                        for token in highlight.children() {
                            match token.tag_name().name() {
                                "sp" => {
                                    n_indents += 1;
                                }
                                _ => {
                                    has_content = true;
                                    break;
                                }
                            }
                        }
                    }
                    if has_content {
                        dedent = std::cmp::min(dedent, n_indents);
                    }
                }
                for codeline in c.children().filter(|n| n.has_tag_name("codeline")) {
                    let codeline = format!("{}<br/>", parse_text(codeline, &mut context));
                    let codeline = codeline.replacen("&nbsp;", "", dedent);
                    s.push_str(&codeline);
                }
                s.push_str("</pre>");
            }
            "highlight" => {
                s.push_str(&format!(
                    "<span class=\"highlight-{}\">{}</span>",
                    c.attribute("class").unwrap(),
                    parse_text(c, &mut context)
                ));
            }
            "image" => {
                let path = c.attribute("name").unwrap();

                // parse style information like:
                // <image ...></image>{width: 80%}
                let mut style = String::new();
                if let Some(tail) = c.next_sibling().and_then(|s| s.text()) {
                    let tail = tail.trim_start();
                    if tail.starts_with('{') {
                        if let Some(end) = tail.find('}') {
                            style = format!(" style=\"{}\"", &tail[1..end]);
                            skip_next_chars = end + 2;
                        }
                    }
                }
                s.push_str(&format!("<img src=\"doxyimg://{}\"{} />", path, style))
            }
            "formula" => {
                context.has_math = true;
                // bring formula into format that MathJAX understands
                let formula = c.text().unwrap();
                let formula = formula.trim_matches('$');
                s.push_str(&format!("\\({}\\)", formula));
            }
            "htmlonly" => {
                let node_range = c.range();
                let input_text = c.document().input_text();
                s.push_str(&input_text[node_range.start + 10..node_range.end - 11]);
            }
            "variablelist" => {
                s.push_str("<dl class=\"variablelist\">");
                for term in c.children() {
                    match term.tag_name().name() {
                        "varlistentry" => {
                            s.push_str(&format!(
                                "<dt>{}</dt>",
                                parse_text(term.get_child("term").unwrap(), &mut context)
                            ));
                        }
                        "listitem" => {
                            s.push_str(&format!(
                                "<dd>{}</dd>",
                                parse_text(term.get_child("para").unwrap(), &mut context)
                            ));
                        }
                        "" => {}
                        tag @ _ => {
                            panic!("unexpected tag: {}", tag);
                        }
                    }
                }
                s.push_str("</dl>");
            }
            "anchor" => {
                let id = c.attribute("id").unwrap();
                s.push_str(&format!("<a name=\"{}\"></a>", id));
            }
            // tag pass-through
            "bold" => {
                s.push_str(&format!("<bold>{}</bold>", parse_text(c, &mut context)));
            }
            "emphasis" => {
                s.push_str(&format!("<em>{}</em>", parse_text(c, &mut context)));
            }
            "verbatim" | "preformatted" => {
                s.push_str(&format!("<pre>{}</pre>", parse_text(c, &mut context)));
            }
            "computeroutput" => {
                s.push_str(&format!("<tt>{}</tt>", parse_text(c, &mut context)));
            }
            "superscript" => {
                s.push_str(&format!("<sup>{}</sup>", parse_text(c, &mut context)));
            }
            "subscript" => {
                s.push_str(&format!("<sub>{}</sub>", parse_text(c, &mut context)));
            }
            tag @ "sect1" | tag @ "sect2" | tag @ "sect3" | tag @ "sect4" | tag @ "sect5" => {
                let title = c.children().find(|n| n.has_tag_name("title")).unwrap();
                let level = tag.chars().nth(4).unwrap().to_digit(10).unwrap() + 1;
                let id = c.attribute("id").unwrap();
                // TODO: generate proper anchors. Currently:
                // sect.id = md_Developer_guide_Cutsim_Gouge_excess_1cutsim_ge_draw_mode_offset
                // ulink.url = #cutsim_ge_draw_mode_offset
                s.push_str(&format!("<a name=\"{}\"></a>", id));
                s.push_str(&format!(
                    "<h{}>{}</h{}>",
                    level,
                    parse_text(title, &mut context),
                    level
                ));
                s.push_str(&parse_text(c, &mut context));
            }
            "title" => {} // handled by sectN
            "heading" => {
                let level = c.attribute("level").unwrap().parse::<usize>().unwrap();
                s.push_str(&format!(
                    "<h{}>{}</h{}>",
                    level,
                    parse_text(c, &mut context),
                    level
                ));
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
            tag @ "deg" | tag @ "ndash" | tag @ "mdash" | tag @ "zwj" => {
                s.push_str(&format!("&{};", tag));
            }
            _ => {
                println!("WARNING: '{}' not implemented!", c.tag_name().name());
            }
        }
    }
    s
}

fn capitalize_first_letter(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().chain(c).collect(),
    }
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
