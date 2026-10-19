//! Whole pages, finished on the server: `page({title, description}, [h1("Bonjour")])`.
//!
//! The head carries what search engines and social networks read (title,
//! description, canonical URL, Open Graph, Twitter, JSON-LD); islands get the
//! script that brings them to life in the browser.

use facile_core::component::Component;
use facile_core::host;
use facile_core::json::{escaped, to_json_text};
use facile_core::util::{self, error, field, record_arg, tr};
use facile_core::view::{self, element};
use rythme::builtins::Builtin;
use rythme::diagnostic::Fallible;
use rythme::interp::Interp;
use rythme::source::Span;
use rythme::value::Value;

use super::respond::response_value;

/// A setting given: not `()` nor `None`, and `x` for `Some(x)`.
fn given(o: &Value, k: &str) -> Option<Value> {
    match field(o, k)? {
        Value::Unit => None,
        v @ Value::Variant(var) => match facile_core::util::variant_name(v).as_deref() {
            Some("None") => None,
            Some("Some") => Some(var.fields[0].clone()),
            _ => Some(v.clone()),
        },
        v => Some(v.clone()),
    }
}

fn opt_text(o: &Value, k: &str) -> Option<String> {
    given(o, k).map(|v| v.display())
}

fn list_texts(o: &Value, k: &str) -> Vec<String> {
    match field(o, k) {
        Some(Value::List(l)) => l.iter().map(|x| x.display()).collect(),
        Some(Value::Text(t)) => vec![t.to_string()],
        _ => Vec::new(),
    }
}

/// The HTML document of a page.
pub fn document(opts: &Value, body: &Value) -> String {
    let title = opt_text(opts, "title").unwrap_or_else(|| "Facile".into());
    let lang = opt_text(opts, "lang").unwrap_or_else(crate::runtime::default_lang);
    let mut body_html = String::new();
    view::render_into(body, &mut body_html);
    let mut h = String::with_capacity(body_html.len() + 2048);
    h.push_str("<!doctype html>\n<html lang=\"");
    h.push_str(&escaped(&lang));
    h.push('"');
    if let Some(theme) = opt_text(opts, "theme") {
        h.push_str(" data-theme=\"");
        h.push_str(&escaped(&theme));
        h.push('"');
    }
    h.push_str(">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>");
    h.push_str(&escaped(&title));
    h.push_str("</title>\n");
    let meta = |h: &mut String, attr: &str, name: &str, content: &str| {
        h.push_str(&format!("<meta {attr}=\"{}\" content=\"{}\">\n", escaped(name), escaped(content)));
    };
    let description = opt_text(opts, "description");
    if let Some(d) = &description {
        meta(&mut h, "name", "description", d);
    }
    if let Some(r) = opt_text(opts, "robots") {
        meta(&mut h, "name", "robots", &r);
    }
    if let Some(c) = opt_text(opts, "theme_color") {
        meta(&mut h, "name", "theme-color", &c);
    }
    let url = opt_text(opts, "url").or_else(|| opt_text(opts, "canonical"));
    if let Some(u) = &url {
        h.push_str(&format!("<link rel=\"canonical\" href=\"{}\">\n", escaped(u)));
    }
    // Open Graph and Twitter cards.
    meta(&mut h, "property", "og:title", &title);
    meta(&mut h, "property", "og:type", &opt_text(opts, "type").unwrap_or_else(|| "website".into()));
    if let Some(d) = &description {
        meta(&mut h, "property", "og:description", d);
    }
    if let Some(u) = &url {
        meta(&mut h, "property", "og:url", u);
    }
    if let Some(s) = opt_text(opts, "site_name") {
        meta(&mut h, "property", "og:site_name", &s);
    }
    let image = opt_text(opts, "image");
    if let Some(i) = &image {
        meta(&mut h, "property", "og:image", i);
    }
    meta(&mut h, "name", "twitter:card", if image.is_some() { "summary_large_image" } else { "summary" });
    meta(&mut h, "name", "twitter:title", &title);
    if let Some(d) = &description {
        meta(&mut h, "name", "twitter:description", d);
    }
    if let Some(i) = &image {
        meta(&mut h, "name", "twitter:image", i);
    }
    // Other languages of the page: `alternates: {en: "/en/", fr: "/"}`.
    if let Some(Value::Record(alts)) = field(opts, "alternates") {
        for (k, v) in alts.iter() {
            h.push_str(&format!("<link rel=\"alternate\" hreflang=\"{}\" href=\"{}\">\n", escaped(k), escaped(&v.display())));
        }
    }
    let icon = opt_text(opts, "icon").unwrap_or_else(|| "/_facile/icon.svg".into());
    h.push_str(&format!("<link rel=\"icon\" href=\"{}\">\n", escaped(&icon)));
    if let Some(m) = opt_text(opts, "manifest") {
        h.push_str(&format!("<link rel=\"manifest\" href=\"{}\">\n", escaped(&m)));
    }
    if !matches!(field(opts, "base_css"), Some(Value::Bool(false))) {
        h.push_str("<link rel=\"stylesheet\" href=\"/_facile/facile.css\">\n");
    }
    for css in list_texts(opts, "css") {
        h.push_str(&format!("<link rel=\"stylesheet\" href=\"{}\">\n", escaped(&css)));
    }
    for p in list_texts(opts, "preload") {
        let kind = if p.ends_with(".woff2") {
            "font\" type=\"font/woff2\" crossorigin"
        } else if p.ends_with(".js") {
            "script"
        } else {
            "fetch\" crossorigin"
        };
        h.push_str(&format!("<link rel=\"preload\" href=\"{}\" as=\"{kind}>\n", escaped(&p)));
    }
    if let Some(ld) = given(opts, "json_ld") {
        // `</` would end the script early.
        let json = to_json_text(&ld).replace("</", "<\\/");
        h.push_str(&format!("<script type=\"application/ld+json\">{json}</script>\n"));
    }
    if let Some(Value::List(extra)) = field(opts, "head") {
        for x in extra.iter() {
            view::render_into(x, &mut h);
        }
    }
    let islands = body_html.contains("<facile-island");
    if islands {
        h.push_str("<link rel=\"modulepreload\" href=\"/_facile/facile.js\">\n");
    }
    h.push_str("</head>\n<body");
    if let Some(c) = opt_text(opts, "body_class") {
        h.push_str(&format!(" class=\"{}\"", escaped(&c)));
    }
    h.push_str(">\n");
    h.push_str(&body_html);
    h.push('\n');
    if islands {
        h.push_str("<script type=\"module\" src=\"/_facile/facile.js\"></script>\n");
    }
    for js in list_texts(opts, "js") {
        h.push_str(&format!("<script type=\"module\" src=\"{}\"></script>\n", escaped(&js)));
    }
    h.push_str("</body>\n</html>\n");
    h
}

fn page(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let (opts, body) = match a {
        [o @ Value::Record(r), rest @ ..] if !view::is_element(r) => (o.clone(), Value::list(rest.to_vec())),
        [Value::Text(t), rest @ ..] => (util::record(vec![("title", Value::text(t))]), Value::list(rest.to_vec())),
        _ => {
            return Err(error(
                "page",
                tr(
                    "give the page's settings (`{title: \"...\"}`) or its title, then its content",
                    "donnez les réglages de la page (`{title: \"...\"}`) ou son titre, puis son contenu",
                ),
                span,
            ))
        }
    };
    let status = match field(&opts, "status") {
        Some(Value::Int(n)) => *n as u16,
        _ => 200,
    };
    Ok(response_value(status, vec![("content-type".into(), "text/html; charset=utf-8".into())], &document(&opts, &body)))
}

/// `island(compteur.component, {depart: 3})`: a component computed on the
/// server, then brought to life in the browser by WebAssembly.
fn island(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let c = Component::from_value(&a[0], span)?;
    if !c.name.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') || c.name.is_empty() {
        return Err(error(
            "island",
            tr(
                "a component's name is letters, digits, `_` and `-`",
                "le nom d'un composant s'écrit avec des lettres, des chiffres, `_` et `-`",
            ),
            span,
        ));
    }
    let props = a.get(1).cloned().unwrap_or_else(|| util::record(vec![]));
    let (state, _) = c.initial(it, props.clone(), span)?;
    let view = c.render(it, state, span)?;
    crate::runtime::note_component(&c.name);
    let attrs = util::record(vec![
        ("data-component", Value::text(&c.name)),
        ("data-props", Value::text(&to_json_text(&props))),
        ("data-src", Value::text(&format!("/_facile/client/{}.ry", c.name))),
    ]);
    Ok(element("facile-island", attrs, vec![view]))
}

/// `stylesheet("/static/site.css")`, `script("/static/3d.js")`: for `head` and the end of `body`.
fn stylesheet(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let href = util::text_arg(a, 0, "stylesheet", span)?;
    Ok(element("link", util::record(vec![("rel", Value::text("stylesheet")), ("href", Value::text(href))]), vec![]))
}

fn script(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let src = util::text_arg(a, 0, "script", span)?;
    Ok(element("script", util::record(vec![("type", Value::text("module")), ("src", Value::text(src))]), vec![]))
}

fn meta_fn(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let r = record_arg(a, 0, "meta", span)?;
    Ok(element("meta", Value::Record(std::rc::Rc::new(r.clone())), vec![]))
}

pub static PAGE: &[Builtin] = &[
    host!("page", "page(settings, content...)", 1, util::MANY, page,
        "A whole HTML page. Settings: `{title, description, lang, url, image, css: [...], js: [...], head: [...], json_ld, theme_color, alternates, status}`.",
        "Une page HTML entière. Réglages : `{title, description, lang, url, image, css: [...], js: [...], head: [...], json_ld, theme_color, alternates, status}`."),
    host!("island", "island(component, props)", 1, 2, island,
        "A component computed on the server and brought to life in the browser by WebAssembly: `island(compteur.component, {depart: 0})`.",
        "Un composant calculé sur le serveur puis animé dans le navigateur par WebAssembly : `island(compteur.component, {depart: 0})`."),
    host!("stylesheet", "stylesheet(href)", 1, 1, stylesheet,
        "A `<link rel=\"stylesheet\">`.", "Un `<link rel=\"stylesheet\">`."),
    host!("script", "script(src)", 1, 1, script,
        "A `<script type=\"module\">` loaded from a file.", "Un `<script type=\"module\">` chargé depuis un fichier."),
    host!("meta", "meta(attributes)", 1, 1, meta_fn,
        "A `<meta>` element: `meta({name: \"author\", content: \"Ada\"})`.", "Un élément `<meta>` : `meta({name: \"author\", content: \"Ada\"})`."),
];
