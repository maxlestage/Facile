//! Views written in Rythme: `div({class: "carte"}, [h2("Titre"), p("Texte")])`.
//!
//! An element is a record `{tag, attrs, children}`; its children are texts,
//! elements and raw HTML (`{raw: "<b>x</b>"}`). The server writes them as
//! HTML; the browser keeps them as a virtual DOM and patches the page.

use std::cell::RefCell;
use std::rc::Rc;

use rythme::builtins::Builtin;
use rythme::diagnostic::Fallible;
use rythme::interp::Interp;
use rythme::source::Span;
use rythme::value::{Record, Shape, Value};
use serde_json::{Map, Value as Json};

use crate::host;
use crate::json::{escape_html, to_json};
use crate::util::{self, error, field, name_of, text_arg, variant_name};

thread_local! {
    static ELEMENT: Rc<Shape> = Rc::new(Shape::new(vec![Rc::from("tag"), Rc::from("attrs"), Rc::from("children")]));
    static EMPTY: Value = Value::Record(Rc::new(Record::new()));
}

/// An element value.
pub fn element(tag: &str, attrs: Value, children: Vec<Value>) -> Value {
    let shape = ELEMENT.with(|s| s.clone());
    Value::Record(Rc::new(Record::with_shape(shape, vec![Value::text(tag), attrs, Value::list(children)])))
}

/// Whether a record is an element (and not attributes).
pub fn is_element(r: &Record) -> bool {
    r.len() == 3 && r.get("tag").is_some() && r.get("children").is_some()
}

fn is_raw(r: &Record) -> bool {
    r.len() == 1 && r.get("raw").is_some()
}

/// Children as the views keep them: texts, elements and raw HTML, lists
/// flattened, `()` and `None` left out (`if ok then p("oui") else ()`).
pub fn push_children(v: &Value, out: &mut Vec<Value>) {
    match v {
        Value::Unit => {}
        Value::Text(_) => out.push(v.clone()),
        Value::List(l) => {
            for x in l.iter() {
                push_children(x, out);
            }
        }
        Value::Record(r) if is_element(r) || is_raw(r) => out.push(v.clone()),
        Value::Variant(var) => match variant_name(v).as_deref() {
            Some("None") => {}
            Some("Some") => push_children(&var.fields[0], out),
            _ => out.push(Value::text(&v.display())),
        },
        other => out.push(Value::text(&other.display())),
    }
}

/// `tag()`, `tag(children)`, `tag(attrs)`, `tag(attrs, children)`.
fn build(tag: &str, a: &[Value]) -> Value {
    let empty = || EMPTY.with(|e| e.clone());
    let mut children = Vec::new();
    let attrs = match a {
        [] => empty(),
        [Value::Record(r)] if !is_element(r) && !is_raw(r) => a[0].clone(),
        [only] => {
            push_children(only, &mut children);
            empty()
        }
        [attrs, rest @ ..] => {
            for c in rest {
                push_children(c, &mut children);
            }
            match attrs {
                Value::Record(r) if !is_element(r) && !is_raw(r) => attrs.clone(),
                other => {
                    let mut first = Vec::new();
                    push_children(other, &mut first);
                    first.append(&mut children);
                    children = first;
                    empty()
                }
            }
        }
    };
    element(tag, attrs, children)
}

fn h(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let tag = text_arg(a, 0, "h", span)?;
    if tag.is_empty() || !tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(error(
            "h",
            util::tr("a tag is letters, digits and `-`", "une balise s'écrit avec des lettres, des chiffres et `-`"),
            span,
        ));
    }
    Ok(build(tag, &a[1..]))
}

fn raw(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let html = text_arg(a, 0, "raw", span)?;
    Ok(util::record(vec![("raw", Value::text(html))]))
}

fn fragment(_: &mut Interp, a: &[Value], _: Span) -> Fallible<Value> {
    let mut children = Vec::new();
    for x in a {
        push_children(x, &mut children);
    }
    Ok(Value::list(children))
}

/// `render(view)`: the HTML of a view, as text.
fn render_fn(_: &mut Interp, a: &[Value], _: Span) -> Fallible<Value> {
    Ok(Value::text(&render(&a[0])))
}

macro_rules! tags {
    ($( $name:ident ),* $(,)?) => {
        $(
            #[allow(non_snake_case)]
            fn $name(_: &mut Interp, a: &[Value], _: Span) -> Fallible<Value> {
                Ok(build(stringify!($name), a))
            }
        )*
        pub static TAGS: &[Builtin] = &[
            $(
                Builtin {
                    name: stringify!($name),
                    sig: concat!(stringify!($name), "(attributes, children)"),
                    min: 0,
                    max: 2,
                    cat: util::CATEGORY,
                    en: concat!("The HTML element <", stringify!($name), ">: `", stringify!($name), "(children)` or `", stringify!($name), "({class: \"x\"}, children)`."),
                    fr: concat!("L'élément HTML <", stringify!($name), "> : `", stringify!($name), "(enfants)` ou `", stringify!($name), "({class: \"x\"}, enfants)`."),
                    example: "",
                    func: $name,
                },
            )*
        ];
    };
}

tags!(
    div, span, p, a, h1, h2, h3, h4, h5, h6, ul, ol, li, nav, header, footer, main, section, article, aside, button, img, form, label,
    strong, em, code, pre, small, br, hr, textarea, video, audio, canvas, iframe, figure, figcaption, blockquote, mark, dl, dt, dd,
    details, summary, svg, table, thead, tbody, tr, th, td, kbd, abbr, cite, picture, address, fieldset, legend, output, progress, meter,
    dialog,
);

pub static VIEW: &[Builtin] = &[
    host!(
        "h",
        "h(tag, attributes, children)",
        1,
        3,
        h,
        "Any HTML element: `h(\"input\", {name: \"q\"})`, `h(\"my-widget\", [\"text\"])`.",
        "N'importe quel élément HTML : `h(\"input\", {name: \"q\"})`, `h(\"mon-widget\", [\"texte\"])`."
    ),
    host!(
        "raw",
        "raw(html)",
        1,
        1,
        raw,
        "HTML written as it is, without escaping (only for HTML you trust).",
        "Du HTML écrit tel quel, sans échappement (seulement pour du HTML de confiance)."
    ),
    host!(
        "fragment",
        "fragment(children, ...)",
        0,
        util::MANY,
        fragment,
        "Several children without an element around them.",
        "Plusieurs enfants sans élément autour."
    ),
    host!("render", "render(view)", 1, 1, render_fn, "The HTML of a view, as text.", "Le HTML d'une vue, en texte."),
];

/// Defines the view functions in an interpreter.
pub fn define(it: &mut Interp) {
    util::define(it, TAGS);
    util::define(it, VIEW);
    util::define(it, crate::markdown::MARKDOWN);
    util::define(it, crate::evaluate::EVALUATE);
    crate::effects::define(it);
}

const VOID: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"];

/// The HTML of a view.
pub fn render(v: &Value) -> String {
    let mut out = String::new();
    render_into(v, &mut out);
    out
}

pub fn render_into(v: &Value, out: &mut String) {
    match v {
        Value::Text(s) => escape_html(s, out),
        Value::List(l) => {
            for x in l.iter() {
                render_into(x, out);
            }
        }
        Value::Record(r) if is_raw(r) => {
            if let Some(Value::Text(s)) = r.get("raw") {
                out.push_str(s);
            }
        }
        Value::Record(r) if is_element(r) => {
            let tag = r.get("tag").map(|t| t.display()).unwrap_or_else(|| "div".into());
            out.push('<');
            out.push_str(&tag);
            if let Some(Value::Record(attrs)) = r.get("attrs") {
                for (k, x) in attrs.iter() {
                    if k.starts_with("on_") || &**k == "key" {
                        continue;
                    }
                    if let Some(s) = attr_value(k, x) {
                        out.push(' ');
                        out.push_str(k);
                        if let Some(s) = s {
                            out.push_str("=\"");
                            escape_html(&s, out);
                            out.push('"');
                        }
                    }
                }
            }
            out.push('>');
            if VOID.contains(&tag.as_str()) {
                return;
            }
            if let Some(c) = r.get("children") {
                render_into(c, out);
            }
            out.push_str("</");
            out.push_str(&tag);
            out.push('>');
        }
        Value::Unit => {}
        Value::Variant(var) => match variant_name(v).as_deref() {
            Some("None") => {}
            Some("Some") => render_into(&var.fields[0], out),
            _ => escape_html(&v.display(), out),
        },
        other => escape_html(&other.display(), out),
    }
}

/// How an attribute is written: `None` to leave it out, `Some(None)` for a
/// name alone (`disabled`), `Some(Some(text))` for `name="text"`.
pub fn attr_value(name: &str, v: &Value) -> Option<Option<String>> {
    match v {
        Value::Bool(true) => Some(None),
        Value::Bool(false) | Value::Unit => None,
        Value::Variant(var) => match variant_name(v).as_deref() {
            Some("None") => None,
            Some("Some") => attr_value(name, &var.fields[0]),
            _ => Some(Some(v.display())),
        },
        // `class: ["carte", if actif then "actif" else ()]`
        Value::List(l) => {
            let parts: Vec<String> = l.iter().filter(|x| util::truthy(x)).map(|x| x.display()).filter(|s| !s.is_empty()).collect();
            Some(Some(parts.join(" ")))
        }
        // `style: {color: "red", "font-size": "2rem"}`
        Value::Record(r) if name == "style" => {
            let parts: Vec<String> = r.iter().map(|(k, x)| format!("{}: {}", k.replace('_', "-"), x.display())).collect();
            Some(Some(parts.join("; ")))
        }
        Value::Record(_) | Value::Dict(_) => Some(Some(crate::json::to_json_text(v))),
        other => Some(Some(other.display())),
    }
}

/// A view as the browser's virtual DOM: `{"t": tag, "a": {...}, "e": {...},
/// "c": [...]}` for an element, a text for a text, `{"r": html}` for raw
/// HTML. Each event handler (`on_click: :plus`, `on_input: fn(v) = ...`) is
/// pushed on `handlers` and named by its position.
pub fn vdom(v: &Value, handlers: &mut Vec<Value>) -> Json {
    let mut children = Vec::new();
    vdom_children(v, handlers, &mut children);
    match children.len() {
        1 => children.pop().unwrap_or(Json::Null),
        _ => {
            let mut o = Map::new();
            o.insert("t".into(), Json::String("facile-fragment".into()));
            o.insert("c".into(), Json::Array(children));
            Json::Object(o)
        }
    }
}

fn vdom_children(v: &Value, handlers: &mut Vec<Value>, out: &mut Vec<Json>) {
    match v {
        Value::Unit => {}
        Value::Text(s) => out.push(Json::String(s.to_string())),
        Value::List(l) => {
            for x in l.iter() {
                vdom_children(x, handlers, out);
            }
        }
        Value::Record(r) if is_raw(r) => {
            let mut o = Map::new();
            o.insert("r".into(), Json::String(r.get("raw").map(|x| x.display()).unwrap_or_default()));
            out.push(Json::Object(o));
        }
        Value::Record(r) if is_element(r) => {
            let mut o = Map::new();
            o.insert("t".into(), Json::String(r.get("tag").map(|t| t.display()).unwrap_or_default()));
            let mut attrs = Map::new();
            let mut events = Map::new();
            if let Some(Value::Record(a)) = r.get("attrs") {
                for (k, x) in a.iter() {
                    if let Some(event) = k.strip_prefix("on_") {
                        events.insert(event.to_string(), Json::from(handlers.len()));
                        handlers.push(x.clone());
                    } else if let Some(s) = attr_value(k, x) {
                        attrs.insert(k.to_string(), Json::String(s.unwrap_or_default()));
                    }
                }
            }
            if !attrs.is_empty() {
                o.insert("a".into(), Json::Object(attrs));
            }
            if !events.is_empty() {
                o.insert("e".into(), Json::Object(events));
            }
            let mut kids = Vec::new();
            if let Some(c) = r.get("children") {
                vdom_children(c, handlers, &mut kids);
            }
            if !kids.is_empty() {
                o.insert("c".into(), Json::Array(kids));
            }
            out.push(Json::Object(o));
        }
        Value::Variant(var) => match variant_name(v).as_deref() {
            Some("None") => {}
            Some("Some") => vdom_children(&var.fields[0], handlers, out),
            _ => out.push(Json::String(v.display())),
        },
        other => out.push(Json::String(other.display())),
    }
}

/// The attributes of an element (for tests and tools).
pub fn attrs_of(v: &Value) -> Option<&Value> {
    field(v, "attrs")
}

/// The text of a view, without its tags.
pub fn text_content(v: &Value) -> String {
    let mut out = String::new();
    fn walk(v: &Value, out: &mut String) {
        match v {
            Value::Text(s) => out.push_str(s),
            Value::List(l) => l.iter().for_each(|x| walk(x, out)),
            Value::Record(r) if is_element(r) => {
                if let Some(c) = r.get("children") {
                    walk(c, out);
                }
            }
            _ => {
                if let Some(n) = name_of(v) {
                    out.push_str(&n);
                }
            }
        }
    }
    walk(v, &mut out);
    out
}

/// A JSON value as attribute data (`data-props`).
pub fn json_attr(v: &Value) -> String {
    serde_json::to_string(&to_json(v)).unwrap_or_default()
}

thread_local! {
    /// Islands met while rendering a page: the server adds their script.
    pub static ISLANDS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}
