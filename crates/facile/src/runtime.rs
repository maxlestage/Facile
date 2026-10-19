//! A worker's application: its interpreter, its routes, its cache.
//!
//! Each worker thread loads the application itself (Rythme values belong to
//! the thread that made them), so a request never waits for another thread.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::Instant;

use facile_core::util::{self, field, name_of};
use rythme::driver::Driver;
use rythme::interp::Mode;
use rythme::source::Span;
use rythme::value::Value;

use crate::host::{chain, request, respond};
use crate::http::{self, Body, Request, Response};
use crate::natives::Natives;
use crate::router::{params_record, Pattern, Router};
use crate::server::Service;
use crate::statics::{safe_join, Asset, Statics};

/// What the whole process knows, the same for every worker.
pub struct Global {
    pub root: PathBuf,
    pub entry: PathBuf,
    pub production: bool,
    pub lang: String,
}

static GLOBAL: OnceLock<Global> = OnceLock::new();

pub fn set_global(g: Global) {
    let _ = GLOBAL.set(g);
}

fn global() -> Option<&'static Global> {
    GLOBAL.get()
}

pub fn production() -> bool {
    global().is_some_and(|g| g.production)
}

pub fn default_lang() -> String {
    global().map(|g| g.lang.clone()).unwrap_or_else(|| "fr".into())
}

/// A path written in the application, from the application's folder.
pub fn resolve(path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    match global() {
        Some(g) => g.root.join(p),
        None => p.to_path_buf(),
    }
}

/// Called on each worker thread before it loads its application.
pub fn prepare_thread() {
    rythme::interp::set_stack_budget(crate::STACK_SIZE - (32 << 20));
}

thread_local! {
    static COMPONENTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// A component shown by `island(...)`: its file is served to the browser.
pub fn note_component(name: &str) {
    COMPONENTS.with(|c| {
        let mut c = c.borrow_mut();
        if !c.iter().any(|n| n == name) {
            c.push(name.to_string());
        }
    });
}

enum Handler {
    Func(Value),
    Resource { model: Value, action: String },
}

struct Entry {
    handler: Handler,
    middlewares: Rc<[Value]>,
}

/// A response kept because it is the same each time.
struct Cached {
    /// The status line and the headers (without date, length, connection).
    head: Vec<u8>,
    body: Rc<[u8]>,
    gz: Option<Rc<[u8]>>,
}

const CACHE_MAX_ENTRIES: usize = 8192;
const CACHE_MAX_BODY: usize = 1 << 20;

pub struct Settings {
    pub cache: bool,
    pub workers: Option<usize>,
    pub body_limit: Option<usize>,
    pub database: Option<String>,
    pub lang: Option<String>,
}

pub struct Runtime {
    driver: Driver,
    router: Router<Entry>,
    natives: Natives,
    statics: Statics,
    app_middlewares: Rc<[Value]>,
    not_found: Option<Value>,
    on_error: Option<Value>,
    cache: rustc_hash::FxHashMap<Box<[u8]>, Cached>,
    cache_enabled: bool,
    log: Vec<u8>,
    production: bool,
    pub name: String,
    pub settings: Settings,
    pub routes: Vec<(String, String)>,
    on_start: Vec<Value>,
    models: Vec<Value>,
}

/// Why an application could not be loaded (its messages were already shown).
#[derive(Debug)]
pub struct LoadError(pub String);

impl Runtime {
    /// Loads, checks and runs the application file. `quiet` keeps the
    /// messages of the checker for itself (the other workers: the first
    /// one already showed them).
    pub fn load(entry: &Path, quiet: bool) -> Result<Runtime, LoadError> {
        if quiet {
            rythme::io::capture();
        }
        let out = Self::load_inner(entry);
        if quiet {
            let _ = rythme::io::take_captured();
        }
        out
    }

    fn load_inner(entry: &Path) -> Result<Runtime, LoadError> {
        let mut driver = Driver::new();
        crate::host::install(&mut driver.interp);
        let Some(module) = driver.load_file(entry) else {
            return Err(LoadError(util::tr("the application has errors", "l'application a des erreurs")));
        };
        let value = match driver.execute(&module, Mode::Run) {
            Ok(Some(v)) => v,
            Ok(None) => return Err(LoadError(ends_with_app())),
            Err(_) => {
                return Err(LoadError(util::tr("the application stopped with an error", "l'application s'est arrêtée sur une erreur")))
            }
        };
        if field(&value, "facile").and_then(name_of).as_deref() != Some("app") {
            return Err(LoadError(ends_with_app()));
        }
        let production = production();
        let list = |k: &str| -> Vec<Value> {
            match field(&value, k) {
                Some(Value::List(l)) => l.iter().cloned().collect(),
                _ => Vec::new(),
            }
        };
        let app_middlewares: Rc<[Value]> = list("middlewares").into();
        let mut router = Router::default();
        let mut routes = Vec::new();
        for r in list("routes") {
            let method = field(&r, "method").and_then(name_of).unwrap_or_else(|| "get".into()).to_ascii_uppercase();
            let path = field(&r, "path").map(|p| p.display()).unwrap_or_default();
            let pattern = Pattern::parse(&path).map_err(LoadError)?;
            let handler = match field(&r, "handler") {
                Some(Value::Record(h)) if h.get("resource").is_some() => Handler::Resource {
                    model: h.get("resource").cloned().unwrap_or(Value::Unit),
                    action: h.get("action").and_then(name_of).unwrap_or_default(),
                },
                Some(f) => Handler::Func(f.clone()),
                None => continue,
            };
            let mut mws: Vec<Value> = app_middlewares.to_vec();
            if let Some(Value::List(l)) = field(&r, "middlewares") {
                mws.extend(l.iter().cloned());
            }
            routes.push((method.clone(), path));
            router.add(&method, pattern, Entry { handler, middlewares: mws.into() });
        }
        let natives = Natives::from_values(&list("natives"), production);
        let mut statics = Statics::new(!production);
        for a in list("assets") {
            let prefix = field(&a, "prefix").map(|p| p.display()).unwrap_or_default();
            let folder = field(&a, "folder").map(|p| p.display()).unwrap_or_default();
            statics.mount(&prefix, resolve(&folder));
        }
        let s = field(&value, "settings").cloned().unwrap_or(Value::Unit);
        let int = |k: &str| match field(&s, k) {
            Some(Value::Int(n)) if *n > 0 => Some(*n as usize),
            _ => None,
        };
        let settings = Settings {
            cache: !matches!(field(&s, "cache"), Some(Value::Bool(false))),
            workers: int("workers"),
            body_limit: int("body_limit"),
            database: field(&s, "database").map(|d| d.display()),
            lang: field(&s, "lang").map(|d| d.display()),
        };
        let callable = |k: &str| field(&value, k).filter(|f| f.is_callable()).cloned();
        Ok(Runtime {
            name: field(&value, "name").map(|n| n.display()).unwrap_or_default(),
            not_found: callable("not_found"),
            on_error: callable("on_error"),
            on_start: list("on_start"),
            models: list("models"),
            cache_enabled: settings.cache,
            settings,
            driver,
            router,
            natives,
            statics,
            app_middlewares,
            cache: Default::default(),
            log: Vec::new(),
            production,
            routes,
        })
    }

    /// Opens the database and creates the tables of the models; with `first`,
    /// runs the `on_start` functions too (once for the whole server).
    pub fn start(&mut self, first: bool) -> Result<(), String> {
        let url = std::env::var("DATABASE_URL").ok().filter(|u| !u.is_empty()).or_else(|| self.settings.database.clone());
        crate::orm::open(url.as_deref().unwrap_or("memory://"))?;
        if first {
            crate::orm::migrate(&self.models)?;
            for f in self.on_start.clone() {
                if let Err(d) = self.driver.interp.call(f, vec![], Span::default()) {
                    return Err(d.render(false));
                }
            }
        }
        Ok(())
    }

    /// Answers one request with an already parsed body (tests and tools).
    pub fn respond(&mut self, req: &Request) -> Vec<u8> {
        let mut out = Vec::new();
        self.handle(req, &mut out);
        out
    }

    fn write(&mut self, req: &Request, resp: &Response, started: Option<Instant>, out: &mut Vec<u8>) {
        resp.write_head(out);
        out.extend_from_slice(self.natives.static_headers());
        let ct = resp.header("content-type").unwrap_or("");
        let gz = if self.natives.compress
            && req.accepts_gzip()
            && resp.body.len() > 1024
            && http::compressible(ct)
            && resp.header("content-encoding").is_none()
        {
            Some(http::gzip(resp.body.as_slice()))
        } else {
            None
        };
        self.finish(req, resp.status, resp.body.as_slice(), gz.as_deref(), started, out);
    }

    /// Ends a response whose head is written: the headers that depend on the
    /// request, then the body.
    fn finish(&mut self, req: &Request, status: u16, body: &[u8], gz: Option<&[u8]>, started: Option<Instant>, out: &mut Vec<u8>) {
        self.natives.dynamic_headers(req, out);
        let body = match gz {
            Some(g) => {
                out.extend_from_slice(b"content-encoding: gzip\r\nvary: accept-encoding\r\n");
                g
            }
            None => body,
        };
        if self.natives.request_id {
            let id = req
                .header("x-request-id")
                .filter(|i| i.len() <= 64 && i.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
            out.extend_from_slice(b"x-request-id: ");
            match id {
                Some(i) => out.extend_from_slice(i.as_bytes()),
                None => out.extend_from_slice(new_id().as_bytes()),
            }
            out.extend_from_slice(b"\r\n");
        }
        if let (true, Some(t)) = (self.natives.timing, started) {
            out.extend_from_slice(format!("server-timing: app;dur={:.3}\r\n", t.elapsed().as_secs_f64() * 1000.0).as_bytes());
        }
        http::write_date(out);
        if !req.keep_alive {
            out.extend_from_slice(b"connection: close\r\n");
        } else if req.http10 {
            out.extend_from_slice(b"connection: keep-alive\r\n");
        }
        if status != 204 && status != 304 && !(100..200).contains(&status) {
            http::write_length(body.len(), out);
        }
        out.extend_from_slice(b"\r\n");
        if req.method() != "HEAD" && status != 204 && status != 304 {
            out.extend_from_slice(body);
        }
        if let (true, Some(t)) = (self.natives.logger, started) {
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            let _ = writeln!(self.log, "{} {} {} {:.2}ms", req.method(), req.target(), status, ms);
        }
    }

    fn serve_asset(&mut self, req: &Request, asset: &Asset, started: Option<Instant>, out: &mut Vec<u8>) {
        if req.header("if-none-match").is_some_and(|t| t.split(',').any(|x| x.trim() == asset.etag)) {
            let mut r = Response::empty(304);
            r.set_header("etag", asset.etag.clone());
            r.set_header("cache-control", asset.cache_control);
            r.write_head(out);
            self.finish(req, 304, &[], None, started, out);
            return;
        }
        let mut r = Response { status: 200, headers: Vec::new(), body: Body::Empty };
        r.set_header("content-type", asset.content_type);
        r.set_header("etag", asset.etag.clone());
        r.set_header("cache-control", asset.cache_control);
        r.write_head(out);
        out.extend_from_slice(self.natives.static_headers());
        let gz = asset.gz.as_deref().filter(|_| req.accepts_gzip());
        let body = asset.body.clone();
        self.finish(req, 200, &body, gz, started, out);
    }

    /// Facile's own files: the browser's runtime and the components' sources.
    fn internal(&self, path: &str) -> Option<Rc<Asset>> {
        let rest = path.strip_prefix("/_facile/")?;
        thread_local! {
            static BUILTIN: RefCell<HashMap<&'static str, Rc<Asset>>> = RefCell::new(HashMap::new());
        }
        if let Some(name) = rest.strip_prefix("client/") {
            if !name.ends_with(".ry") {
                return None;
            }
            let file = safe_join(&resolve("client"), name)?;
            let bytes = std::fs::read(file).ok()?;
            return Some(Rc::new(Asset::new(name, bytes, if self.production { "public, max-age=300" } else { "no-cache" })));
        }
        let (name, bytes, cache): (&'static str, &'static [u8], &'static str) = crate::assets::find(rest)?;
        BUILTIN.with(|b| Some(b.borrow_mut().entry(name).or_insert_with(|| Rc::new(Asset::new(name, bytes.to_vec(), cache))).clone()))
    }

    fn not_found_response(&mut self, req: &Request, path: &str) -> Response {
        let allowed = self.router.methods_for(path);
        if !allowed.is_empty() {
            let mut r = respond::error_response(405, "method not allowed", "méthode non permise");
            r.set_header("allow", allowed.join(", "));
            return r;
        }
        if let Some(h) = self.not_found.clone() {
            let rv = request::request_value(req, path, util::record(vec![]));
            let mws = self.app_middlewares.clone();
            let it = &mut self.driver.interp;
            match request::with_request(req, || chain::run(it, &mws, &h, 0, rv, Span::default())) {
                Ok(v) => {
                    let mut r = respond::from_value(&v);
                    if r.status == 200 {
                        r.status = 404;
                    }
                    return r;
                }
                Err(d) => return self.failure(req, &d.render(false)),
            }
        }
        let wants_html = req.header("accept").is_some_and(|a| a.contains("text/html"));
        if wants_html {
            Response::html(404, crate::assets::page_404(path))
        } else {
            respond::error_response(404, "not found", "introuvable")
        }
    }

    /// The response to a handler that failed: the message in development, a
    /// short page in production, `on_error` when the application has one.
    fn failure(&mut self, req: &Request, message: &str) -> Response {
        eprintln!("facile: {} {}:\n{message}", req.method(), req.target());
        if let Some(h) = self.on_error.clone() {
            let rv = request::request_value(req, req.path(), util::record(vec![]));
            let it = &mut self.driver.interp;
            if let Ok(v) = request::with_request(req, || it.call(h, vec![rv, Value::text(message)], Span::default())) {
                let mut r = respond::from_value(&v);
                if r.status == 200 {
                    r.status = 500;
                }
                return r;
            }
        }
        let wants_html = req.header("accept").is_some_and(|a| a.contains("text/html"));
        match (self.production, wants_html) {
            (true, true) => Response::html(500, crate::assets::page_500(None)),
            (true, false) => respond::error_response(500, "internal error", "erreur interne"),
            (false, true) => Response::html(500, crate::assets::page_500(Some(message))),
            (false, false) => Response::json(500, facile_core::json::to_json_text(&util::record(vec![("error", Value::text(message))]))),
        }
    }
}

fn ends_with_app() -> String {
    util::tr(
        "the application file must end with the application: its last line is the value made by `app(...)`",
        "le fichier de l'application doit se terminer par l'application : sa dernière ligne est la valeur faite par `app(...)`",
    )
}

/// 16 hexadecimal digits, different at each call (not a secret).
fn new_id() -> String {
    thread_local! {
        static STATE: std::cell::Cell<u64> = std::cell::Cell::new(
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1)
                ^ (&STATE as *const _ as u64)
        );
    }
    STATE.with(|s| {
        let mut x = s.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
        format!("{x:016x}")
    })
}

impl Runtime {
    /// When the request started, for the middlewares that tell durations.
    fn clock(&self) -> Option<Instant> {
        (self.natives.logger || self.natives.timing).then(Instant::now)
    }
}

impl Service for Runtime {
    fn handle(&mut self, req: &Request, out: &mut Vec<u8>) {
        let started = self.clock();
        if self.natives.checks {
            if let Some(r) = self.natives.before(req, self.production) {
                return self.write(req, &r, started, out);
            }
        }
        let method = req.method();
        let get = method == "GET" || method == "HEAD";
        // A response given before, for the same path and query.
        if get && self.cache_enabled {
            if let Some(c) = self.cache.get(req.target_bytes()) {
                out.extend_from_slice(&c.head);
                let body = c.body.clone();
                let gz = c.gz.clone().filter(|_| self.natives.compress && req.accepts_gzip());
                return self.finish(req, 200, &body, gz.as_deref(), started, out);
            }
        }
        let path = req.path();
        if get {
            if let Some(asset) = self.internal(path).or_else(|| if self.statics.is_empty() { None } else { self.statics.find(path) }) {
                return self.serve_asset(req, &asset, started, out);
            }
        }
        let Some(found) = self.router.find(method, path) else {
            let r = self.not_found_response(req, path);
            return self.write(req, &r, started, out);
        };
        let params = params_record(&found);
        let entry = &self.router.routes()[found.index].target;
        let rv = request::request_value(req, path, params);
        util::reset_impure();
        let it = &mut self.driver.interp;
        let result = match &entry.handler {
            Handler::Func(h) => {
                let (mws, h) = (entry.middlewares.clone(), h.clone());
                request::with_request(req, || chain::run(it, &mws, &h, 0, rv, Span::default()))
            }
            Handler::Resource { model, action } => {
                let (mws, model, action) = (entry.middlewares.clone(), model.clone(), action.clone());
                request::with_request(req, || crate::orm::resource::run(it, &mws, &model, &action, rv, Span::default()))
            }
        };
        let resp = match result {
            Ok(v) => respond::from_value(&v),
            Err(d) => {
                let msg = d.render(false);
                let r = self.failure(req, &msg);
                return self.write(req, &r, started, out);
            }
        };
        let keep = get
            && self.cache_enabled
            && !util::was_impure()
            && resp.status == 200
            && resp.body.len() <= CACHE_MAX_BODY
            && !resp.headers.iter().any(|(k, _)| k == "set-cookie");
        if keep {
            let mut head = Vec::with_capacity(256);
            resp.write_head(&mut head);
            head.extend_from_slice(self.natives.static_headers());
            let ct = resp.header("content-type").unwrap_or("");
            let body: Rc<[u8]> = Rc::from(resp.body.as_slice());
            let gz = (self.natives.compress && body.len() > 1024 && http::compressible(ct)).then(|| Rc::from(http::gzip(&body)));
            if self.cache.len() >= CACHE_MAX_ENTRIES {
                self.cache.clear();
            }
            let send_gz = gz.clone().filter(|_| req.accepts_gzip());
            out.extend_from_slice(&head);
            self.cache.insert(req.target_bytes().into(), Cached { head, body: body.clone(), gz });
            return self.finish(req, 200, &body, send_gz.as_deref(), started, out);
        }
        self.write(req, &resp, started, out)
    }

    fn after_turn(&mut self) {
        if !self.log.is_empty() {
            let mut stdout = std::io::stdout().lock();
            let _ = stdout.write_all(&self.log);
            let _ = stdout.flush();
            self.log.clear();
        }
    }
}
