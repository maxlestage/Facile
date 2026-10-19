//! Building an application, as a value: `app("Tâches") |> route(:get, "/", accueil)`.
//!
//! Every function returns a new application (values never change); the
//! server reads the last one, the value the file ends with.

use std::rc::Rc;

use facile_core::host;
use facile_core::util::{self, error, field, func_arg, name_arg, record_arg, text_arg, tr};
use rythme::builtins::Builtin;
use rythme::diagnostic::Fallible;
use rythme::interp::Interp;
use rythme::source::Span;
use rythme::value::{List, Record, Value};

pub const LISTS: [&str; 7] = ["routes", "middlewares", "natives", "assets", "models", "on_start", "components"];

fn new_app(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let name = text_arg(a, 0, "app", span)?;
    let mut fields = vec![("facile", util::sym("app")), ("name", Value::text(name))];
    for l in LISTS {
        fields.push((l, Value::list(Vec::new())));
    }
    fields.push(("settings", util::record(vec![])));
    fields.push(("not_found", Value::Unit));
    fields.push(("on_error", Value::Unit));
    Ok(util::record(fields))
}

/// The application given as first argument.
pub fn app_arg<'a>(a: &'a [Value], f: &str, span: Span) -> Fallible<&'a Record> {
    let r = record_arg(a, 0, f, span)?;
    if r.get("facile").and_then(util::name_of).as_deref() != Some("app") {
        return Err(error(
            f,
            tr(
                "the first argument must be an application made by `app(\"name\")`",
                "le premier argument doit être une application faite par `app(\"nom\")`",
            ),
            span,
        ));
    }
    Ok(r)
}

fn list_of<'a>(r: &'a Record, key: &str) -> &'a [Value] {
    match r.get(key) {
        Some(Value::List(l)) => l.as_slice(),
        _ => &[],
    }
}

/// The application with `item` added at the end of its list `key`.
fn push(r: &Record, key: &str, item: Value) -> Value {
    let list = match r.get(key) {
        Some(Value::List(l)) => l.push(item),
        _ => List::new(vec![item]),
    };
    util::with_field(r, key, Value::List(list))
}

const METHODS: [&str; 8] = ["get", "post", "put", "patch", "delete", "head", "options", "any"];

fn route(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let app = app_arg(a, "route", span)?;
    let method = name_arg(a, 1, "route", span)?.to_ascii_lowercase();
    if !METHODS.contains(&method.as_str()) {
        return Err(error(
            "route",
            tr(
                &format!("`{method}` is not an HTTP method: use :get, :post, :put, :patch, :delete, :options or :any"),
                &format!("`{method}` n'est pas une méthode HTTP : utilisez :get, :post, :put, :patch, :delete, :options ou :any"),
            ),
            span,
        ));
    }
    let path = text_arg(a, 2, "route", span)?;
    crate::router::Pattern::parse(path).map_err(|e| error("route", e, span))?;
    let handler = func_arg(a, 3, "route", span)?;
    let r = util::record(vec![
        ("method", util::sym(&method)),
        ("path", Value::text(path)),
        ("handler", handler.clone()),
        ("middlewares", Value::list(Vec::new())),
    ]);
    Ok(push(app, "routes", r))
}

/// `get(app, path, handler)` is `route(app, :get, path, handler)`: one function per method.
macro_rules! method_fns {
    ($( $f:ident => $m:literal ),*) => {
        $(
            fn $f(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
                if a.len() != 3 {
                    return Err(error(concat!("on_", $m), tr("give the application, a path and a function", "donnez l'application, un chemin et une fonction"), span));
                }
                route(it, &[a[0].clone(), util::sym($m), a[1].clone(), a[2].clone()], span)
            }
        )*
    };
}

method_fns!(on_get => "get", on_post => "post", on_put => "put", on_patch => "patch", on_delete => "delete");

fn middleware(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let app = app_arg(a, "middleware", span)?;
    match &a[1] {
        Value::Record(r) if r.get("native").is_some() => Ok(push(app, "natives", a[1].clone())),
        f if f.is_callable() => Ok(push(app, "middlewares", f.clone())),
        _ => Err(error(
            "middleware",
            tr(
                "a middleware is a function `fn(req, next)` or one of `mw` (`mw.cors([\"*\"])`)",
                "un middleware est une fonction `fn(req, next)` ou l'un de `mw` (`mw.cors([\"*\"])`)",
            ),
            span,
        )),
    }
}

/// `mount(app, "/admin", admin)`: the routes of `admin` under `/admin`, with its middlewares.
fn mount(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let app = app_arg(a, "mount", span)?;
    let prefix = text_arg(a, 1, "mount", span)?.trim_end_matches('/').to_string();
    if !prefix.starts_with('/') {
        return Err(error("mount", tr("the prefix starts with `/`", "le préfixe commence par `/`"), span));
    }
    let sub = app_arg(&a[2..], "mount", span)?;
    let mut out = Value::Record(Rc::new(app.clone()));
    let sub_mws = list_of(sub, "middlewares");
    for r in list_of(sub, "routes") {
        let Value::Record(rr) = r else { continue };
        let path = field(r, "path").map(|p| p.display()).unwrap_or_default();
        let full = if path == "/" { prefix.clone() } else { format!("{prefix}{path}") };
        let mut mws: Vec<Value> = sub_mws.to_vec();
        mws.extend_from_slice(list_of(rr, "middlewares"));
        let mut nr = (**rr).clone();
        nr.set(Rc::from("path"), Value::text(&full));
        nr.set(Rc::from("middlewares"), Value::list(mws));
        let Value::Record(cur) = &out else { unreachable!() };
        out = push(cur, "routes", Value::Record(Rc::new(nr)));
    }
    for n in list_of(sub, "natives") {
        let Value::Record(nr) = n else { continue };
        let scope = field(n, "scope").map(|s| s.display()).unwrap_or_default();
        let scoped = util::with_field(nr, "scope", Value::text(&format!("{prefix}{scope}")));
        let Value::Record(cur) = &out else { unreachable!() };
        out = push(cur, "natives", scoped);
    }
    for key in ["assets", "models", "on_start", "components"] {
        for x in list_of(sub, key) {
            let x = match (key, x) {
                ("assets", Value::Record(ar)) => {
                    let p = field(x, "prefix").map(|s| s.display()).unwrap_or_default();
                    util::with_field(ar, "prefix", Value::text(&format!("{prefix}{p}")))
                }
                _ => x.clone(),
            };
            let Value::Record(cur) = &out else { unreachable!() };
            out = push(cur, key, x);
        }
    }
    Ok(out)
}

fn assets(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let app = app_arg(a, "assets", span)?;
    let prefix = text_arg(a, 1, "assets", span)?;
    let folder = text_arg(a, 2, "assets", span)?;
    if !prefix.starts_with('/') {
        return Err(error("assets", tr("the prefix starts with `/`", "le préfixe commence par `/`"), span));
    }
    Ok(push(app, "assets", util::record(vec![("prefix", Value::text(prefix)), ("folder", Value::text(folder))])))
}

fn models(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let app = app_arg(a, "models", span)?;
    let mut out = Value::Record(Rc::new(app.clone()));
    let items: Vec<Value> = match &a[1] {
        Value::List(l) => l.iter().cloned().collect(),
        other => vec![other.clone()],
    };
    for m in items {
        if field(&m, "facile").and_then(util::name_of).as_deref() != Some("model") {
            return Err(error("models", tr("give models made by `model(...)`", "donnez des modèles faits par `model(...)`"), span));
        }
        let Value::Record(cur) = &out else { unreachable!() };
        out = push(cur, "models", m);
    }
    Ok(out)
}

/// `resource(app, "/api/taches", taches)`: a REST API for a model.
fn resource(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let app = app_arg(a, "resource", span)?;
    let path = text_arg(a, 1, "resource", span)?.trim_end_matches('/').to_string();
    let model = &a[2];
    if field(model, "facile").and_then(util::name_of).as_deref() != Some("model") {
        return Err(error(
            "resource",
            tr("the third argument is a model made by `model(...)`", "le troisième argument est un modèle fait par `model(...)`"),
            span,
        ));
    }
    let only: Option<Vec<String>> = match a.get(3) {
        Some(Value::List(l)) => Some(l.iter().filter_map(util::name_of).collect()),
        _ => None,
    };
    let actions = [
        ("list", "get", path.clone()),
        ("create", "post", path.clone()),
        ("show", "get", format!("{path}/:id")),
        ("replace", "put", format!("{path}/:id")),
        ("update", "patch", format!("{path}/:id")),
        ("delete", "delete", format!("{path}/:id")),
    ];
    let mut out = Value::Record(Rc::new(app.clone()));
    for (action, method, p) in actions {
        if only.as_ref().is_some_and(|o| !o.iter().any(|x| x == action)) {
            continue;
        }
        let handler = util::record(vec![("resource", model.clone()), ("action", util::sym(action))]);
        let r = util::record(vec![
            ("method", util::sym(method)),
            ("path", Value::text(&p)),
            ("handler", handler),
            ("middlewares", Value::list(Vec::new())),
        ]);
        let Value::Record(cur) = &out else { unreachable!() };
        out = push(cur, "routes", r);
    }
    let Value::Record(cur) = &out else { unreachable!() };
    Ok(push(cur, "models", model.clone()))
}

fn hook(key: &'static str, f: &'static str, a: &[Value], span: Span) -> Fallible<Value> {
    let app = app_arg(a, f, span)?;
    let h = func_arg(a, 1, f, span)?;
    if key == "on_start" {
        Ok(push(app, key, h.clone()))
    } else {
        Ok(util::with_field(app, key, h.clone()))
    }
}

fn not_found(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    hook("not_found", "not_found", a, span)
}

fn on_error(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    hook("on_error", "on_error", a, span)
}

fn on_start(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    hook("on_start", "on_start", a, span)
}

/// `settings(app, {cache: false, body_limit: 1_000_000})`.
fn settings(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let app = app_arg(a, "settings", span)?;
    let new = record_arg(a, 1, "settings", span)?;
    let mut cur = match app.get("settings") {
        Some(Value::Record(r)) => (**r).clone(),
        _ => Record::new(),
    };
    for (k, v) in new.iter() {
        cur.set(k.clone(), v.clone());
    }
    Ok(util::with_field(app, "settings", Value::Record(Rc::new(cur))))
}

fn database(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let app = app_arg(a, "database", span)?;
    let url = text_arg(a, 1, "database", span)?;
    let mut cur = match app.get("settings") {
        Some(Value::Record(r)) => (**r).clone(),
        _ => Record::new(),
    };
    cur.set(Rc::from("database"), Value::text(url));
    Ok(util::with_field(app, "settings", Value::Record(Rc::new(cur))))
}

pub static APP: &[Builtin] = &[
    host!("app", "app(name)", 1, 1, new_app,
        "A new web application, without routes yet. The file ends with the application to serve.",
        "Une nouvelle application web, encore sans routes. Le fichier se termine par l'application à servir."),
    host!("route", "route(app, method, path, handler)", 4, 4, route,
        "The application with one more route: `route(app, :get, \"/taches/:id\", detail)`. The handler gets the request.",
        "L'application avec une route de plus : `route(app, :get, \"/taches/:id\", detail)`. Le handler reçoit la requête."),
    host!("on_get", "on_get(app, path, handler)", 3, 3, on_get,
        "`route(app, :get, path, handler)`.", "`route(app, :get, chemin, handler)`."),
    host!("on_post", "on_post(app, path, handler)", 3, 3, on_post,
        "`route(app, :post, path, handler)`.", "`route(app, :post, chemin, handler)`."),
    host!("on_put", "on_put(app, path, handler)", 3, 3, on_put,
        "`route(app, :put, path, handler)`.", "`route(app, :put, chemin, handler)`."),
    host!("on_patch", "on_patch(app, path, handler)", 3, 3, on_patch,
        "`route(app, :patch, path, handler)`.", "`route(app, :patch, chemin, handler)`."),
    host!("on_delete", "on_delete(app, path, handler)", 3, 3, on_delete,
        "`route(app, :delete, path, handler)`.", "`route(app, :delete, chemin, handler)`."),
    host!("middleware", "middleware(app, middleware)", 2, 2, middleware,
        "The application with a middleware around every route: a function `fn(req, next)`, or one of `mw`.",
        "L'application avec un middleware autour de chaque route : une fonction `fn(req, next)`, ou l'un de `mw`."),
    host!("mount", "mount(app, prefix, other_app)", 3, 3, mount,
        "The routes of another application under a prefix, with its middlewares: `mount(app, \"/admin\", admin)`.",
        "Les routes d'une autre application sous un préfixe, avec ses middlewares : `mount(app, \"/admin\", admin)`."),
    host!("assets", "assets(app, prefix, folder)", 3, 3, assets,
        "Serves the files of a folder: `assets(app, \"/static\", \"public\")`, with gzip, ETag and cache headers.",
        "Sert les fichiers d'un dossier : `assets(app, \"/static\", \"public\")`, avec gzip, ETag et en-têtes de cache."),
    host!("models", "models(app, models)", 2, 2, models,
        "The models whose tables the server creates or completes when it starts.",
        "Les modèles dont le serveur crée ou complète les tables au démarrage."),
    host!("resource", "resource(app, path, model, actions)", 3, 4, resource,
        "A REST API for a model: GET and POST on the path, GET, PUT, PATCH and DELETE on `path/:id`. Optional: the actions to keep (`[:list, :show]`).",
        "Une API REST pour un modèle : GET et POST sur le chemin, GET, PUT, PATCH et DELETE sur `chemin/:id`. En option : les actions à garder (`[:list, :show]`)."),
    host!("not_found", "not_found(app, handler)", 2, 2, not_found,
        "The handler for paths without a route (a 404 page).", "Le handler des chemins sans route (une page 404)."),
    host!("on_error", "on_error(app, handler)", 2, 2, on_error,
        "The handler called with the request and the error message when a handler fails.",
        "Le handler appelé avec la requête et le message d'erreur quand un handler échoue."),
    host!("on_start", "on_start(app, function)", 2, 2, on_start,
        "A function called once when the server starts (to fill a database, say).",
        "Une fonction appelée une fois au démarrage du serveur (pour remplir une base, par exemple)."),
    host!("settings", "settings(app, settings)", 2, 2, settings,
        "Settings: `{cache: false}`, `{body_limit: 1000000}`, `{workers: 4}`, `{lang: \"fr\"}`.",
        "Des réglages : `{cache: false}`, `{body_limit: 1000000}`, `{workers: 4}`, `{lang: \"fr\"}`."),
    host!("database", "database(app, url)", 2, 2, database,
        "The database: `memory://`, `sqlite://app.db`, `postgres://...`. `DATABASE_URL` wins when it is set.",
        "La base de données : `memory://`, `sqlite://app.db`, `postgres://...`. `DATABASE_URL` l'emporte s'il est défini."),
];
