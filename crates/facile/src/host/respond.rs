//! Responses: what a handler returns becomes an HTTP response.
//!
//! - a text: `text/plain`;
//! - a view (`div(...)`) or a page (`page(...)`): HTML;
//! - a response made by `html`, `json`, `redirect`, `status`...: as it is;
//! - `Ok(x)` and `Some(x)`: `x`; `None`: 404; `Err(e)`: an error (see [`error_status`]);
//! - `()`: 204; any other value (records, lists, numbers): JSON.

use std::rc::Rc;

use facile_core::host;
use facile_core::json::to_json_text;
use facile_core::util::{self, error, field, int_arg, name_of, record_arg, text_arg, tr, variant_name};
use facile_core::view;
use rythme::builtins::Builtin;
use rythme::diagnostic::Fallible;
use rythme::interp::Interp;
use rythme::source::Span;
use rythme::value::{Record, Shape, Value};

use crate::http::{Body, HeaderText, Response};

thread_local! {
    static RESPONSE: Rc<Shape> = Rc::new(Shape::new(vec![Rc::from("status"), Rc::from("headers"), Rc::from("body")]));
    /// A response record, to recognize the others by their shape.
    static PROBE: Value = Value::Record(Rc::new(Record::with_shape(RESPONSE.with(|s| s.clone()), vec![Value::Unit, Value::Unit, Value::Unit])));
    /// `[["content-type", ...]]` for JSON, HTML and text: made once, shared by the responses.
    static TYPED: [Value; 3] = [JSON, HTML, TEXT].map(|t| Value::list(vec![Value::list(vec![Value::text("content-type"), Value::text(t)])]));
}

pub fn is_response(r: &Record) -> bool {
    r.get("status").is_some() && r.get("headers").is_some() && r.get("body").is_some()
}

fn same_shape(r: &Record) -> bool {
    PROBE.with(|p| match p {
        Value::Record(pr) => std::ptr::eq(pr.keys().as_ptr(), r.keys().as_ptr()),
        _ => false,
    })
}

fn record_of(status: u16, headers: Value, body: Value) -> Value {
    let shape = RESPONSE.with(|s| s.clone());
    Value::Record(Rc::new(Record::with_shape(shape, vec![Value::Int(status as i64), headers, body])))
}

/// A response record.
pub fn response_value(status: u16, headers: Vec<(String, String)>, body: &str) -> Value {
    let hs = headers.into_iter().map(|(k, v)| Value::list(vec![Value::text(&k), Value::text(&v)])).collect();
    record_of(status, Value::list(hs), Value::text(body))
}

/// A response record of one of the three common types (0 JSON, 1 HTML, 2 text).
fn typed(status: u16, kind: usize, body: Value) -> Value {
    record_of(status, TYPED.with(|t| t[kind].clone()), body)
}

/// A response as the record middlewares see (`next(req)` returns it).
pub fn to_value(r: &Response) -> Value {
    let body = String::from_utf8_lossy(r.body.as_slice());
    let hs = r.headers.iter().map(|(k, v)| Value::list(vec![Value::text(k), Value::text(v)])).collect();
    record_of(r.status, Value::list(hs), Value::text(&body))
}

const HTML: &str = "text/html; charset=utf-8";
const JSON: &str = "application/json";
const TEXT: &str = "text/plain; charset=utf-8";

/// The status of an error value: `Err(:not_found)` is 404, `Err({status: 422, ...})`
/// is 422, `Err("message")` is 400.
pub fn error_status(e: &Value) -> u16 {
    match e {
        Value::Symbol(s) => symbol_status(&s.name()).unwrap_or(400),
        Value::Record(r) => match r.get("status") {
            Some(Value::Int(n)) if (100..600).contains(n) => *n as u16,
            _ => 400,
        },
        Value::Int(n) if (400..600).contains(n) => *n as u16,
        _ => 400,
    }
}

fn symbol_status(s: &str) -> Option<u16> {
    Some(match s {
        "bad_request" | "invalid" => 400,
        "unauthorized" => 401,
        "forbidden" => 403,
        "not_found" => 404,
        "method_not_allowed" => 405,
        "conflict" => 409,
        "gone" => 410,
        "unprocessable" | "validation" => 422,
        "too_many_requests" => 429,
        "error" | "internal" => 500,
        "unavailable" => 503,
        _ => return None,
    })
}

/// What a handler returned, as an HTTP response.
pub fn from_value(v: &Value) -> Response {
    match v {
        Value::Text(s) => Response {
            status: 200,
            headers: vec![(HeaderText::Borrowed("content-type"), HeaderText::Borrowed(TEXT))],
            body: Body::Text(s.clone()),
        },
        Value::Unit => Response::empty(204),
        Value::Record(r) if same_shape(r) || is_response(r) => {
            let (st, hs, b) = if same_shape(r) {
                let mut it = r.iter().map(|(_, v)| v);
                (it.next(), it.next(), it.next())
            } else {
                (r.get("status"), r.get("headers"), r.get("body"))
            };
            let status = match st {
                Some(Value::Int(n)) if (100..600).contains(n) => *n as u16,
                _ => 200,
            };
            let mut headers: Vec<(HeaderText, HeaderText)> = Vec::new();
            if let Some(Value::List(hs)) = hs {
                let common = TYPED.with(|t| {
                    t.iter().position(|x| matches!(x, Value::List(l) if std::ptr::eq(l.as_slice().as_ptr(), hs.as_slice().as_ptr()) && l.len() == hs.len()))
                });
                match common {
                    Some(i) => headers.push((HeaderText::Borrowed("content-type"), HeaderText::Borrowed([JSON, HTML, TEXT][i]))),
                    None => {
                        for h in hs.iter() {
                            if let Value::List(kv) = h {
                                if let (Some(k), Some(v)) = (kv.get(0), kv.get(1)) {
                                    let (k, v) = (k.display().to_ascii_lowercase(), v.display());
                                    // A line break would start another header (response splitting).
                                    if k.is_empty() || k.contains(['\r', '\n', ':', ' ']) || v.contains(['\r', '\n']) {
                                        continue;
                                    }
                                    headers.push((HeaderText::Owned(k), HeaderText::Owned(v)));
                                }
                            }
                        }
                    }
                }
            }
            if !same_shape(r) {
                if let Some(Value::Text(path)) = r.get("file") {
                    return file_response(status, headers, path);
                }
            }
            let body = match b {
                Some(Value::Text(s)) => Body::Text(s.clone()),
                Some(other) if !matches!(other, Value::Unit) => Body::Bytes(other.display().into_bytes()),
                _ => Body::Empty,
            };
            Response { status, headers, body }
        }
        Value::Record(r) if view::is_element(r) || (r.len() == 1 && r.get("raw").is_some()) => Response::new(200, HTML, view::render(v)),
        Value::List(l) if !l.is_empty() && l.iter().all(|x| matches!(x, Value::Record(r) if view::is_element(r))) => {
            Response::new(200, HTML, view::render(v))
        }
        Value::Variant(var) => match variant_name(v).as_deref() {
            Some("Ok") | Some("Some") => from_value(&var.fields[0]),
            Some("None") => error_response(404, "not found", "introuvable"),
            Some("Err") => {
                let e = &var.fields[0];
                let status = error_status(e);
                let body = match e {
                    Value::Record(_) => to_json_text(e),
                    Value::Symbol(s) => to_json_text(&util::record(vec![("error", Value::text(&s.name()))])),
                    other => to_json_text(&util::record(vec![("error", Value::text(&other.display()))])),
                };
                Response::new(status, JSON, body)
            }
            _ => Response::new(200, JSON, to_json_text(v)),
        },
        other => Response::new(200, JSON, to_json_text(other)),
    }
}

pub fn error_response(status: u16, en: &str, fr: &str) -> Response {
    let msg = tr(en, fr);
    Response::new(status, JSON, to_json_text(&util::record(vec![("error", Value::text(&msg))])))
}

fn file_response(status: u16, mut headers: Vec<(HeaderText, HeaderText)>, path: &str) -> Response {
    match std::fs::read(crate::runtime::resolve(path)) {
        Ok(bytes) => {
            if !headers.iter().any(|(k, _)| k == "content-type") {
                headers.push((HeaderText::Borrowed("content-type"), HeaderText::Borrowed(crate::http::content_type(path))));
            }
            Response { status, headers, body: Body::Bytes(bytes) }
        }
        Err(_) => error_response(404, "file not found", "fichier introuvable"),
    }
}

fn html(_: &mut Interp, a: &[Value], _: Span) -> Fallible<Value> {
    let body = match &a[0] {
        Value::Text(s) => s.to_string(),
        v => view::render(v),
    };
    let status = match a.get(1) {
        Some(Value::Int(n)) => *n as u16,
        _ => 200,
    };
    Ok(typed(status, 1, Value::text(&body)))
}

fn json(_: &mut Interp, a: &[Value], _: Span) -> Fallible<Value> {
    let status = match a.get(1) {
        Some(Value::Int(n)) => *n as u16,
        _ => 200,
    };
    Ok(typed(status, 0, Value::Text(to_json_text(&a[0]).into())))
}

fn status(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let code = int_arg(a, 1, "status", span)?;
    if !(100..600).contains(&code) {
        return Err(error("status", tr("an HTTP status is between 100 and 599", "un statut HTTP est entre 100 et 599"), span));
    }
    let r = from_value(&a[0]);
    let mut v = to_value(&r);
    if let Value::Record(rec) = &v {
        v = util::with_field(rec, "status", Value::Int(code));
    }
    Ok(v)
}

fn redirect(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let url = text_arg(a, 0, "redirect", span)?;
    let code = match a.get(1) {
        Some(Value::Int(n)) if [301, 302, 303, 307, 308].contains(n) => *n as u16,
        Some(_) => {
            return Err(error(
                "redirect",
                tr("a redirection is 301, 302, 303, 307 or 308", "une redirection est 301, 302, 303, 307 ou 308"),
                span,
            ))
        }
        None => 303,
    };
    Ok(response_value(code, vec![("location".into(), url.to_string())], ""))
}

fn with_header(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let name = text_arg(a, 1, "with_header", span)?.to_ascii_lowercase();
    let value = a[2].display();
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return Err(error(
            "with_header",
            tr("a header name is letters, digits and `-`", "un nom d'en-tête s'écrit avec des lettres, des chiffres et `-`"),
            span,
        ));
    }
    if value.contains(['\r', '\n']) {
        return Err(error(
            "with_header",
            tr("a header value cannot hold a line break", "une valeur d'en-tête ne peut pas contenir de retour à la ligne"),
            span,
        ));
    }
    let mut r = from_value(&a[0]);
    if name == "set-cookie" {
        r.add_header(&name, value);
    } else {
        r.set_header(&name, value);
    }
    Ok(keep_file(&a[0], to_value(&r)))
}

/// A response record made from a file keeps its file.
fn keep_file(from: &Value, to: Value) -> Value {
    match (field(from, "file"), &to) {
        (Some(f), Value::Record(r)) => util::with_field(r, "file", f.clone()),
        _ => to,
    }
}

/// `with_cookie(res, "theme", "sombre", {max_age: 86400})`.
fn with_cookie(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let name = text_arg(a, 1, "with_cookie", span)?;
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return Err(error(
            "with_cookie",
            tr("a cookie name is letters, digits, `_` and `-`", "un nom de cookie s'écrit avec des lettres, des chiffres, `_` et `-`"),
            span,
        ));
    }
    let value = a[2].display();
    let mut c = format!("{name}={}", cookie_encode(&value));
    let opts = match a.get(3) {
        Some(Value::Record(r)) => Some(r.clone()),
        _ => None,
    };
    let get = |k: &str| opts.as_ref().and_then(|o| o.get(k).cloned());
    let path = get("path").map(|p| p.display()).unwrap_or_else(|| "/".into());
    c.push_str(&format!("; Path={path}"));
    if let Some(Value::Int(age)) = get("max_age") {
        c.push_str(&format!("; Max-Age={age}"));
    }
    if !matches!(get("http_only"), Some(Value::Bool(false))) {
        c.push_str("; HttpOnly");
    }
    let same = get("same_site").map(|s| s.display()).unwrap_or_else(|| "Lax".into());
    c.push_str(&format!("; SameSite={}", capitalize(&same)));
    let secure = match get("secure") {
        Some(Value::Bool(b)) => b,
        _ => crate::runtime::production(),
    };
    if secure {
        c.push_str("; Secure");
    }
    let mut r = from_value(&a[0]);
    r.add_header("set-cookie", c);
    Ok(keep_file(&a[0], to_value(&r)))
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => String::new(),
    }
}

fn cookie_encode(v: &str) -> String {
    let mut out = String::new();
    for b in v.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()/:@".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn send_file(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    util::mark_impure();
    let path = text_arg(a, 0, "send_file", span)?;
    if path.split(['/', '\\']).any(|p| p == "..") {
        return Err(error("send_file", tr("a path with `..` is refused", "un chemin avec `..` est refusé"), span));
    }
    let mut r = response_value(200, vec![("content-type".into(), crate::http::content_type(path).into())], "");
    if let Value::Record(rec) = &r {
        r = util::with_field(rec, "file", Value::text(path));
    }
    Ok(r)
}

fn no_content(_: &mut Interp, _: &[Value], _: Span) -> Fallible<Value> {
    Ok(response_value(204, Vec::new(), ""))
}

/// `problem(422, "le titre manque")`: a JSON error with a status.
fn problem(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let code = int_arg(a, 0, "problem", span)?;
    let detail = match a.get(1) {
        Some(Value::Record(r)) => Value::Record(r.clone()),
        Some(v) => util::record(vec![("error", Value::text(&v.display()))]),
        None => util::record(vec![("error", Value::text(crate::http::reason(code as u16)))]),
    };
    Ok(response_value(code.clamp(100, 599) as u16, vec![("content-type".into(), JSON.into())], &to_json_text(&detail)))
}

/// The record of headers of a response, for tests (`res.headers`).
pub fn header_of(v: &Value, name: &str) -> Option<String> {
    let Value::List(hs) = field(v, "headers")? else { return None };
    hs.iter().find_map(|h| match h {
        Value::List(kv) if kv.get(0).and_then(name_of).is_some_and(|k| k.eq_ignore_ascii_case(name)) => kv.get(1).map(|v| v.display()),
        _ => None,
    })
}

#[allow(dead_code)]
fn unused(a: &[Value], span: Span) -> Fallible<()> {
    record_arg(a, 0, "", span).map(|_| ())
}

pub static RESPOND: &[Builtin] = &[
    host!("html", "html(content, status)", 1, 2, html,
        "An HTML response, from a text or a view.", "Une réponse HTML, à partir d'un texte ou d'une vue."),
    host!("json", "json(value, status)", 1, 2, json,
        "A JSON response: records are objects, lists arrays.", "Une réponse JSON : les enregistrements sont des objets, les listes des tableaux."),
    host!("status", "status(response, code)", 2, 2, status,
        "The response with another status: `\"Introuvable\" |> status(404)`.", "La réponse avec un autre statut : `\"Introuvable\" |> status(404)`."),
    host!("redirect", "redirect(url, code)", 1, 2, redirect,
        "A redirection (303 See Other unless given: 301, 302, 307, 308).", "Une redirection (303 See Other sauf si donné : 301, 302, 307, 308)."),
    host!("with_header", "with_header(response, name, value)", 3, 3, with_header,
        "The response with a header.", "La réponse avec un en-tête."),
    host!("with_cookie", "with_cookie(response, name, value, options)", 3, 4, with_cookie,
        "The response with a cookie (HttpOnly, SameSite=Lax, Secure in production). Options: `{max_age, path, same_site, http_only, secure}`.",
        "La réponse avec un cookie (HttpOnly, SameSite=Lax, Secure en production). Options : `{max_age, path, same_site, http_only, secure}`."),
    host!("send_file", "send_file(path)", 1, 1, send_file,
        "The content of a file, with its media type.", "Le contenu d'un fichier, avec son type."),
    host!("no_content", "no_content()", 0, 0, no_content,
        "An empty response (204).", "Une réponse vide (204)."),
    host!("problem", "problem(status, detail)", 1, 2, problem,
        "A JSON error: `problem(422, \"le titre manque\")` gives `{\"error\": \"le titre manque\"}`.",
        "Une erreur JSON : `problem(422, \"le titre manque\")` donne `{\"error\": \"le titre manque\"}`."),
];
