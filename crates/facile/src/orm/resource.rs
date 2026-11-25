//! `resource(app, "/api/taches", taches)`: a REST API for a model.
//!
//! - `GET /api/taches`: the rows; `?fait=false` filters on a field,
//!   `?q=lait` searches the text fields, `?order=-created_at` sorts (a `-`
//!   for descending), `?limit=10&offset=20`, or `?page=2&per_page=10` for
//!   `{items, page, per_page, total, pages}`;
//! - `POST /api/taches`: adds the row of the JSON body (or form): 201;
//! - `GET`, `PUT`, `PATCH`, `DELETE /api/taches/:id`.
//!
//! The middlewares of the application run around it like around any route.

use std::cell::RefCell;
use std::rc::Rc;

use facile_core::host;
use facile_core::json::from_json;
use facile_core::util::{self, error, field, tr, variant_name};
use rythme::builtins::Builtin;
use rythme::diagnostic::Fallible;
use rythme::interp::Interp;
use rythme::source::Span;
use rythme::value::{Record, Value};

use super::model::{model_of, Kind};
use crate::host::{chain, respond};

thread_local! {
    static CURRENT: RefCell<Vec<(Value, String)>> = const { RefCell::new(Vec::new()) };
}

pub fn run(it: &mut Interp, mws: &Rc<[Value]>, model: &Value, action: &str, req: Value, span: Span) -> Fallible<Value> {
    CURRENT.with(|c| c.borrow_mut().push((model.clone(), action.to_string())));
    let out = chain::run(it, mws, &Value::Builtin(&HANDLER), 0, req, span);
    CURRENT.with(|c| c.borrow_mut().pop());
    out
}

fn call(it: &mut Interp, name: &str, args: Vec<Value>, span: Span) -> Fallible<Value> {
    let b = super::host::DB.iter().find(|b| b.name == name).expect("db function");
    (b.func)(it, &args, span)
}

fn query_fn(name: &str) -> &'static Builtin {
    super::host::ORM.iter().find(|b| b.name == name).expect("query function")
}

/// The body of the request as a record: JSON, or a form.
fn body_record(it: &mut Interp, req: &Value, span: Span) -> Fallible<Result<Record, Value>> {
    let ct = field(req, "header").cloned().map(|h| it.call(h, vec![Value::text("content-type")], span)).transpose()?;
    let is_form = ct.as_ref().is_some_and(|v| util::payload(v).display().contains("application/x-www-form-urlencoded"));
    let parsed = if is_form {
        let f = field(req, "form").cloned().ok_or_else(|| error("resource", "no form", span))?;
        it.call(f, vec![], span)?
    } else {
        let f = field(req, "json").cloned().ok_or_else(|| error("resource", "no json", span))?;
        let r = it.call(f, vec![], span)?;
        match variant_name(&r).as_deref() {
            Some("Ok") => util::payload(&r),
            _ => {
                return Ok(Err(respond::response_value(
                    400,
                    vec![("content-type".into(), "application/json".into())],
                    &facile_core::json::to_json_text(&util::record(vec![(
                        "error",
                        Value::text(&tr("the body must be a JSON object", "le corps doit être un objet JSON")),
                    )])),
                )))
            }
        }
    };
    match parsed {
        Value::Record(r) => Ok(Ok((*r).clone())),
        _ => Ok(Err(respond::response_value(
            400,
            vec![("content-type".into(), "application/json".into())],
            "{\"error\":\"the body must be a JSON object\"}",
        ))),
    }
}

fn not_found() -> Value {
    respond::response_value(404, vec![("content-type".into(), "application/json".into())], "{\"error\":\"not found\"}")
}

fn handler(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let Some((model, action)) = CURRENT.with(|c| c.borrow().last().cloned()) else {
        return Err(error("resource", "no resource", span));
    };
    let req = a[0].clone();
    let m = model_of(&model).map_err(|e| error("resource", e, span))?;
    let id = field(&req, "params").and_then(|p| field(p, "id")).map(|v| v.display());
    let id_value = |id: &Option<String>| id.as_ref().and_then(|i| i.parse::<i64>().ok()).map(Value::Int);
    match action.as_str() {
        "list" => {
            let mut q = model.clone();
            let query: Vec<(String, String)> = match field(&req, "query") {
                Some(Value::Record(r)) => r.iter().map(|(k, v)| (k.to_string(), v.display())).collect(),
                _ => Vec::new(),
            };
            let mut page = None;
            let mut per = None;
            for (k, v) in &query {
                match k.as_str() {
                    "limit" => q = (query_fn("limit").func)(it, &[q, Value::Int(v.parse().unwrap_or(100))], span)?,
                    "offset" => q = (query_fn("offset").func)(it, &[q, Value::Int(v.parse().unwrap_or(0))], span)?,
                    "page" => page = v.parse::<i64>().ok(),
                    "per_page" => per = v.parse::<i64>().ok(),
                    "order" => {
                        let (f, dir) = match v.strip_prefix('-') {
                            Some(f) => (f, "desc"),
                            None => (v.as_str(), "asc"),
                        };
                        if m.kind_of(f).is_some() {
                            q = (query_fn("order_by").func)(it, &[q, Value::text(f), util::sym(dir)], span)?;
                        }
                    }
                    "q" => {
                        let fields: Vec<Value> = m.fields.iter().filter(|f| f.kind == Kind::Text).map(|f| Value::text(&f.name)).collect();
                        q = (query_fn("search").func)(it, &[q, Value::list(fields), Value::text(v)], span)?;
                    }
                    k if m.kind_of(k).is_some() => q = (query_fn("where").func)(it, &[q, Value::text(k), Value::text(v)], span)?,
                    _ => {}
                }
            }
            match page {
                Some(p) => call(it, "paginate", vec![q, Value::Int(p), Value::Int(per.unwrap_or(20))], span),
                None => {
                    let has_limit = query.iter().any(|(k, _)| k == "limit");
                    let q = if has_limit { q } else { (query_fn("limit").func)(it, &[q, Value::Int(1000)], span)? };
                    call(it, "all", vec![q], span)
                }
            }
        }
        "show" => match id_value(&id) {
            Some(id) => {
                let found = call(it, "find", vec![model, id], span)?;
                Ok(if variant_name(&found).as_deref() == Some("Some") { util::payload(&found) } else { not_found() })
            }
            None => Ok(not_found()),
        },
        "create" => {
            let body = match body_record(it, &req, span)? {
                Ok(b) => b,
                Err(resp) => return Ok(resp),
            };
            let r = call(it, "insert", vec![model, Value::Record(Rc::new(body))], span)?;
            if variant_name(&r).as_deref() == Some("Ok") {
                let row = util::payload(&r);
                let v = respond::to_value(&respond::from_value(&row));
                let Value::Record(rec) = &v else { return Ok(v) };
                return Ok(util::with_field(rec, "status", Value::Int(201)));
            }
            Ok(r)
        }
        "replace" | "update" => {
            let Some(id) = id_value(&id) else { return Ok(not_found()) };
            let body = match body_record(it, &req, span)? {
                Ok(b) => b,
                Err(resp) => return Ok(resp),
            };
            let body = body.without("id").without("created_at").without("updated_at");
            let r = if action == "replace" {
                // Every field given again: the ones left out are emptied, as PUT means.
                let mut full = Record::new();
                for f in &m.fields {
                    let v = body.get(&f.name).cloned().unwrap_or(Value::Unit);
                    full.set(Rc::from(f.name.as_str()), v);
                }
                for (k, v) in body.iter() {
                    full.set(k.clone(), v.clone());
                }
                let _ = from_json;
                call(it, "update", vec![model, id, Value::Record(Rc::new(full))], span)?
            } else {
                call(it, "update", vec![model, id, Value::Record(Rc::new(body))], span)?
            };
            Ok(r)
        }
        "delete" => match id_value(&id) {
            Some(id) => {
                let done = call(it, "delete", vec![model, id], span)?;
                Ok(if matches!(done, Value::Bool(true)) { respond::response_value(204, Vec::new(), "") } else { not_found() })
            }
            None => Ok(not_found()),
        },
        _ => Ok(not_found()),
    }
}

static HANDLER: Builtin =
    host!("resource", "resource handler", 1, 1, handler, "The handler of a REST resource.", "Le handler d'une ressource REST.");
