//! The request a handler receives: `{method, path, params, query}`, and
//! functions to read the rest of it: `req.header("accept")`,
//! `req.cookie("theme")`, `req.body()`, `req.json()`, `req.form()`, `req.ip()`.
//!
//! The record holds only what its path and query decide; reading anything
//! else marks the response as one that cannot be given again from the cache.

use std::cell::Cell;
use std::rc::Rc;

use facile_core::host;
use facile_core::json::parse_json;
use facile_core::util::{self, error, text_arg, tr};
use rythme::builtins::Builtin;
use rythme::diagnostic::Fallible;
use rythme::interp::Interp;
use rythme::source::Span;
use rythme::symbol::Sym;
use rythme::value::{Record, Shape, Value};

use crate::http::{parse_query, Request};

thread_local! {
    /// The request being answered, while its handler runs.
    static CURRENT: Cell<*const Request> = const { Cell::new(std::ptr::null()) };
    static SHAPE: Rc<Shape> = Rc::new(Shape::new(
        ["method", "path", "params", "query", "header", "cookie", "body", "json", "form", "ip"].iter().map(|k| Rc::from(*k)).collect(),
    ));
    static EMPTY: Value = Value::Record(Rc::new(Record::new()));
    static METHODS: Vec<(String, Value)> = ["get", "post", "put", "patch", "delete", "head", "options"]
        .iter()
        .map(|m| (m.to_ascii_uppercase(), Value::Symbol(Sym::new(m))))
        .collect();
}

/// Runs `f` with `req` as the current request.
pub fn with_request<R>(req: &Request, f: impl FnOnce() -> R) -> R {
    let prev = CURRENT.with(|c| c.replace(req as *const Request));
    let out = f();
    CURRENT.with(|c| c.set(prev));
    out
}

fn current<R>(f: &str, span: Span, g: impl FnOnce(&Request) -> R) -> Fallible<R> {
    util::mark_impure();
    let p = CURRENT.with(|c| c.get());
    if p.is_null() {
        return Err(error(
            f,
            tr(
                "there is no request here: this works while a request is answered",
                "il n'y a pas de requête ici : cela marche pendant qu'une requête est traitée",
            ),
            span,
        ));
    }
    // SAFETY: `with_request` set the pointer to a request that lives until it returns,
    // and resets it before; handlers only run inside it, on this thread.
    Ok(g(unsafe { &*p }))
}

fn query_record(q: &str) -> Value {
    let mut r = Record::new();
    for (k, v) in parse_query(q) {
        if !k.is_empty() {
            r.set(Rc::from(k.as_str()), Value::text(&v));
        }
    }
    Value::Record(Rc::new(r))
}

/// The request record given to the handler.
pub fn request_value(req: &Request, path: &str, params: Value) -> Value {
    let method = METHODS.with(|ms| {
        ms.iter()
            .find(|(m, _)| m == req.method())
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| Value::Symbol(Sym::new(&req.method().to_ascii_lowercase())))
    });
    let shape = SHAPE.with(|s| s.clone());
    let q = req.query();
    let query = if q.is_empty() { EMPTY.with(|e| e.clone()) } else { query_record(q) };
    let vals = vec![
        method,
        Value::text(path),
        params,
        query,
        Value::Builtin(&ACCESSORS[0]),
        Value::Builtin(&ACCESSORS[1]),
        Value::Builtin(&ACCESSORS[2]),
        Value::Builtin(&ACCESSORS[3]),
        Value::Builtin(&ACCESSORS[4]),
        Value::Builtin(&ACCESSORS[5]),
    ];
    Value::Record(Rc::new(Record::with_shape(shape, vals)))
}

fn header(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let name = text_arg(a, 0, "header", span)?.to_string();
    let v = current("header", span, |r| r.header(&name).map(|s| s.to_string()))?;
    Ok(it.option(v.map(|s| Value::text(&s))))
}

fn cookie(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let name = text_arg(a, 0, "cookie", span)?.to_string();
    let v = current("cookie", span, |r| r.cookie(&name))?;
    Ok(it.option(v.map(|s| Value::text(&s))))
}

fn body(_: &mut Interp, _: &[Value], span: Span) -> Fallible<Value> {
    let v = current("body", span, |r| String::from_utf8_lossy(&r.body).into_owned())?;
    Ok(Value::text(&v))
}

fn json(it: &mut Interp, _: &[Value], span: Span) -> Fallible<Value> {
    let parsed = {
        let it: &Interp = it;
        current("json", span, |r| parse_json(it, &r.body))?
    };
    Ok(match parsed {
        Ok(v) => it.ok(v),
        Err(e) => it.err(Value::text(&tr(&format!("the body is not JSON: {e}"), &format!("le corps n'est pas du JSON : {e}")))),
    })
}

fn form(_: &mut Interp, _: &[Value], span: Span) -> Fallible<Value> {
    let text = current("form", span, |r| String::from_utf8_lossy(&r.body).into_owned())?;
    Ok(query_record(&text))
}

fn ip(_: &mut Interp, _: &[Value], span: Span) -> Fallible<Value> {
    let v = current("ip", span, |r| r.client_ip())?;
    Ok(Value::text(&v))
}

/// The functions in the request record, in the order of its shape.
pub static ACCESSORS: [Builtin; 6] = [
    host!(
        "header",
        "req.header(name)",
        1,
        1,
        header,
        "A header of the request: `Some(value)` or `None`. Names have no case.",
        "Un en-tête de la requête : `Some(valeur)` ou `None`. Les noms n'ont pas de casse."
    ),
    host!(
        "cookie",
        "req.cookie(name)",
        1,
        1,
        cookie,
        "A cookie of the request: `Some(value)` or `None`.",
        "Un cookie de la requête : `Some(valeur)` ou `None`."
    ),
    host!("body", "req.body()", 0, 0, body, "The body of the request, as text.", "Le corps de la requête, en texte."),
    host!(
        "json",
        "req.json()",
        0,
        0,
        json,
        "The body read as JSON: `Ok(value)` or `Err(message)`.",
        "Le corps lu comme du JSON : `Ok(valeur)` ou `Err(message)`."
    ),
    host!(
        "form",
        "req.form()",
        0,
        0,
        form,
        "The fields of a form sent by the browser, as a record.",
        "Les champs d'un formulaire envoyé par le navigateur, en enregistrement."
    ),
    host!(
        "ip",
        "req.ip()",
        0,
        0,
        ip,
        "The client's address (the first of X-Forwarded-For behind a proxy).",
        "L'adresse du client (la première de X-Forwarded-For derrière un proxy)."
    ),
];
