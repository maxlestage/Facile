//! The middlewares of `mw`, run by the engine.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use facile_core::util::field;
use rythme::value::Value;

use crate::http::{Request, Response};

#[derive(Clone)]
enum Kind {
    SecurityHeaders { csp: String },
    Cors { origins: Vec<String>, methods: String, headers: String, credentials: bool, max_age: i64 },
    RateLimit { per_minute: u32, id: usize },
    BasicAuth { expected: String, realm: String },
    Bearer { token: String },
    BodyLimit(usize),
    Headers(Vec<(String, String)>),
    Https,
    Waf,
    BlockBots,
    Csrf,
    Logger,
    Compress,
    RequestId,
    Timing,
}

#[derive(Clone)]
struct Native {
    kind: Kind,
    scope: String,
}

/// The native middlewares of an application, ready to run.
#[derive(Default)]
pub struct Natives {
    list: Vec<Native>,
    /// The headers that every response gets, written once.
    static_headers: Vec<u8>,
    pub compress: bool,
    pub logger: bool,
    pub request_id: bool,
    pub timing: bool,
    /// Whether one of them looks at each request before the cache.
    pub checks: bool,
}

const DEFAULT_CSP: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: https:; font-src 'self' data: https:; connect-src 'self'; media-src 'self' blob: https:; worker-src 'self' blob:; object-src 'none'; frame-ancestors 'self'; base-uri 'self'; form-action 'self'";

impl Natives {
    pub fn from_values(values: &[Value], production: bool) -> Natives {
        let mut list = Vec::new();
        for v in values {
            let scope = field(v, "scope").map(|s| s.display()).unwrap_or_default();
            let kind = field(v, "native").and_then(facile_core::util::name_of).unwrap_or_default();
            let options = field(v, "options").cloned().unwrap_or(Value::Unit);
            let opt = |k: &str| field(&options, k).cloned();
            let mut push = |k: Kind| list.push(Native { kind: k, scope: scope.clone() });
            match kind.as_str() {
                "security_headers" => {
                    push(Kind::SecurityHeaders { csp: opt("csp").map(|c| c.display()).unwrap_or_else(|| DEFAULT_CSP.into()) })
                }
                "cors" => {
                    let origins = match field(v, "origins") {
                        Some(Value::List(l)) => l.iter().map(|o| o.display()).collect(),
                        _ => vec!["*".into()],
                    };
                    push(Kind::Cors {
                        origins,
                        methods: opt("methods").map(|m| list_text(&m)).unwrap_or_else(|| "GET, POST, PUT, PATCH, DELETE, OPTIONS".into()),
                        headers: opt("headers").map(|m| list_text(&m)).unwrap_or_else(|| "Content-Type, Authorization".into()),
                        credentials: matches!(opt("credentials"), Some(Value::Bool(true))),
                        max_age: match opt("max_age") {
                            Some(Value::Int(n)) => n,
                            _ => 86400,
                        },
                    })
                }
                "rate_limit" => {
                    let n = match field(v, "per_minute") {
                        Some(Value::Int(n)) => (*n).clamp(1, u32::MAX as i64) as u32,
                        _ => 60,
                    };
                    let id = next_limiter();
                    push(Kind::RateLimit { per_minute: n, id })
                }
                "basic_auth" => {
                    let user = field(v, "user").map(|x| x.display()).unwrap_or_default();
                    let pass = field(v, "password").map(|x| x.display()).unwrap_or_default();
                    push(Kind::BasicAuth {
                        expected: format!("{user}:{pass}"),
                        realm: field(v, "realm").map(|x| x.display()).unwrap_or_default(),
                    })
                }
                "bearer" => push(Kind::Bearer { token: field(v, "token").map(|x| x.display()).unwrap_or_default() }),
                "body_limit" => push(Kind::BodyLimit(match field(v, "bytes") {
                    Some(Value::Int(n)) => *n as usize,
                    _ => 1 << 20,
                })),
                "headers" => {
                    let hs = match field(v, "headers") {
                        Some(Value::Record(r)) => r.iter().map(|(k, x)| (k.to_ascii_lowercase(), x.display())).collect(),
                        _ => Vec::new(),
                    };
                    push(Kind::Headers(hs))
                }
                "https" => push(Kind::Https),
                "waf" => push(Kind::Waf),
                "block_bots" => push(Kind::BlockBots),
                "csrf" => push(Kind::Csrf),
                "logger" => push(Kind::Logger),
                "compress" => push(Kind::Compress),
                "request_id" => push(Kind::RequestId),
                "timing" => push(Kind::Timing),
                "shield" => {
                    for k in [
                        Kind::Waf,
                        Kind::BlockBots,
                        Kind::SecurityHeaders { csp: DEFAULT_CSP.into() },
                        Kind::Csrf,
                        Kind::RateLimit { per_minute: 300, id: next_limiter() },
                        Kind::Compress,
                        Kind::RequestId,
                    ] {
                        push(k);
                    }
                }
                _ => {}
            }
        }
        let mut n = Natives { list, ..Natives::default() };
        for x in &n.list {
            let global = x.scope.is_empty();
            match &x.kind {
                Kind::Compress => n.compress = true,
                Kind::Logger => n.logger = true,
                Kind::RequestId => n.request_id = true,
                Kind::Timing => n.timing = true,
                Kind::SecurityHeaders { csp } if global => {
                    let mut add = |k: &str, v: &str| {
                        n.static_headers.extend_from_slice(k.as_bytes());
                        n.static_headers.extend_from_slice(b": ");
                        n.static_headers.extend_from_slice(v.as_bytes());
                        n.static_headers.extend_from_slice(b"\r\n");
                    };
                    add("content-security-policy", csp);
                    add("x-content-type-options", "nosniff");
                    add("x-frame-options", "SAMEORIGIN");
                    add("referrer-policy", "strict-origin-when-cross-origin");
                    add("permissions-policy", "camera=(), microphone=(), geolocation=(), interest-cohort=()");
                    add("cross-origin-opener-policy", "same-origin");
                    if production {
                        add("strict-transport-security", "max-age=31536000; includeSubDomains");
                    }
                }
                Kind::Headers(hs) if global => {
                    for (k, v) in hs {
                        if !k.contains(['\r', '\n', ':']) && !v.contains(['\r', '\n']) {
                            n.static_headers.extend_from_slice(format!("{k}: {v}\r\n").as_bytes());
                        }
                    }
                }
                Kind::Cors { origins, .. } if global && origins.len() == 1 && origins[0] == "*" => {
                    n.static_headers.extend_from_slice(b"access-control-allow-origin: *\r\n");
                    n.checks = true;
                }
                Kind::SecurityHeaders { .. } | Kind::Headers(_) => {}
                _ => n.checks = true,
            }
        }
        n
    }

    /// Headers for every response of every path.
    pub fn static_headers(&self) -> &[u8] {
        &self.static_headers
    }

    /// Looks at a request before anything else: an answer when it is refused
    /// (or when it is a CORS preflight).
    pub fn before(&self, req: &Request, production: bool) -> Option<Response> {
        let path = req.path();
        for n in &self.list {
            if !path.starts_with(&n.scope) {
                continue;
            }
            match &n.kind {
                Kind::Waf => {
                    if let Some(r) = waf(req) {
                        return Some(r);
                    }
                }
                Kind::BlockBots => {
                    if is_bot(req) {
                        return Some(Response::text(404, "Not Found"));
                    }
                }
                Kind::Https if production => {
                    if req.header("x-forwarded-proto") == Some("http") {
                        let host = req.header("host").unwrap_or("");
                        let mut r = Response::empty(308);
                        r.set_header("location", format!("https://{host}{}", req.target()));
                        return Some(r);
                    }
                }
                Kind::RateLimit { per_minute, id } => {
                    if let Some(wait) = limited(*id, *per_minute, &req.client_ip()) {
                        let mut r = Response::json(429, "{\"error\":\"too many requests\"}");
                        r.set_header("retry-after", wait.to_string());
                        return Some(r);
                    }
                }
                Kind::BodyLimit(max) => {
                    if req.body.len() > *max {
                        return Some(Response::json(413, "{\"error\":\"body too large\"}"));
                    }
                }
                Kind::BasicAuth { expected, realm } => {
                    let ok = req
                        .header("authorization")
                        .and_then(|h| h.strip_prefix("Basic ").or_else(|| h.strip_prefix("basic ")))
                        .and_then(|b| base64_decode(b.trim()))
                        .is_some_and(|got| constant_eq(&got, expected.as_bytes()));
                    if !ok {
                        let mut r = Response::text(401, "Unauthorized");
                        r.set_header("www-authenticate", format!("Basic realm=\"{realm}\", charset=\"UTF-8\""));
                        return Some(r);
                    }
                }
                Kind::Bearer { token } => {
                    let ok = req
                        .header("authorization")
                        .and_then(|h| h.strip_prefix("Bearer ").or_else(|| h.strip_prefix("bearer ")))
                        .is_some_and(|t| constant_eq(t.trim().as_bytes(), token.as_bytes()));
                    if !ok {
                        let mut r = Response::json(401, "{\"error\":\"unauthorized\"}");
                        r.set_header("www-authenticate", "Bearer");
                        return Some(r);
                    }
                }
                Kind::Csrf => {
                    if !matches!(req.method(), "GET" | "HEAD" | "OPTIONS") && cross_site(req) {
                        return Some(Response::json(403, "{\"error\":\"cross-site request refused\"}"));
                    }
                }
                Kind::Cors { origins, methods, headers, credentials, max_age }
                    if req.method() == "OPTIONS" && req.header("access-control-request-method").is_some() =>
                {
                    let mut r = Response::empty(204);
                    if let Some(o) = allowed_origin(req, origins) {
                        r.set_header("access-control-allow-origin", o);
                        r.set_header("access-control-allow-methods", methods.clone());
                        r.set_header("access-control-allow-headers", headers.clone());
                        r.set_header("access-control-max-age", max_age.to_string());
                        if *credentials {
                            r.set_header("access-control-allow-credentials", "true");
                        }
                        r.set_header("vary", "origin");
                    }
                    return Some(r);
                }
                _ => {}
            }
        }
        None
    }

    /// Headers that depend on the request or on the path (written on each response).
    pub fn dynamic_headers(&self, req: &Request, out: &mut Vec<u8>) {
        let path = req.path();
        for n in &self.list {
            if !path.starts_with(&n.scope) {
                continue;
            }
            match &n.kind {
                Kind::Cors { origins, credentials, .. } if !(n.scope.is_empty() && origins.len() == 1 && origins[0] == "*") => {
                    if let Some(o) = allowed_origin(req, origins) {
                        out.extend_from_slice(format!("access-control-allow-origin: {o}\r\nvary: origin\r\n").as_bytes());
                        if *credentials {
                            out.extend_from_slice(b"access-control-allow-credentials: true\r\n");
                        }
                    }
                }
                Kind::SecurityHeaders { csp } if !n.scope.is_empty() => {
                    out.extend_from_slice(format!("content-security-policy: {csp}\r\nx-content-type-options: nosniff\r\nx-frame-options: SAMEORIGIN\r\nreferrer-policy: strict-origin-when-cross-origin\r\n").as_bytes());
                }
                Kind::Headers(hs) if !n.scope.is_empty() => {
                    for (k, v) in hs {
                        if !k.contains(['\r', '\n', ':']) && !v.contains(['\r', '\n']) {
                            out.extend_from_slice(format!("{k}: {v}\r\n").as_bytes());
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn list_text(v: &Value) -> String {
    match v {
        Value::List(l) => l.iter().map(|x| x.display()).collect::<Vec<_>>().join(", "),
        other => other.display(),
    }
}

fn allowed_origin(req: &Request, origins: &[String]) -> Option<String> {
    if origins.iter().any(|o| o == "*") {
        return Some("*".into());
    }
    let origin = req.header("origin")?;
    origins.iter().any(|o| o == origin).then(|| origin.to_string())
}

/// A request sent by a page of another site.
fn cross_site(req: &Request) -> bool {
    if let Some(site) = req.header("sec-fetch-site") {
        return site.eq_ignore_ascii_case("cross-site");
    }
    match (req.header("origin"), req.header("host")) {
        (Some(origin), Some(host)) if origin != "null" => {
            let o = origin.split("://").nth(1).unwrap_or(origin);
            !o.eq_ignore_ascii_case(host)
        }
        (Some("null"), _) => true,
        _ => false,
    }
}

const WAF_PATTERNS: &[&str] = &[
    "union select",
    "union all select",
    "' or '1'='1",
    "\" or \"1\"=\"1",
    "' or 1=1",
    " or 1=1--",
    "information_schema",
    "drop table",
    "xp_cmdshell",
    "sleep(",
    "benchmark(",
    "pg_sleep",
    "waitfor delay",
    "<script",
    "javascript:",
    "onerror=",
    "onload=",
    "<iframe",
    "<svg/onload",
    "document.cookie",
    "../",
    "..\\",
    "/etc/passwd",
    "/proc/self",
    "c:\\windows",
    "${jndi:",
    "${env:",
    ";cat ",
    "|cat ",
    "$(curl",
    "$(wget",
    ";wget ",
    "|bash",
    "; rm -rf",
    "\0",
];

/// A web application firewall on the path and the query.
fn waf(req: &Request) -> Option<Response> {
    let target = crate::http::percent_decode(req.target());
    // Decoded twice: `%252e%252e` hides `..` from one decoding.
    let target = crate::http::percent_decode(&target).to_lowercase();
    WAF_PATTERNS.iter().any(|p| target.contains(p)).then(|| Response::json(403, "{\"error\":\"request refused by the firewall\"}"))
}

const BOT_PATHS: &[&str] = &[
    "/.env",
    "/.git",
    "/.svn",
    "/.aws",
    "/.ds_store",
    "/wp-admin",
    "/wp-login",
    "/wp-content",
    "/wp-includes",
    "/xmlrpc.php",
    "/phpmyadmin",
    "/pma",
    "/config.php",
    "/cgi-bin",
    "/vendor/phpunit",
    "/actuator",
    "/server-status",
    "/.htaccess",
    "/boaform",
    "/owa/",
    "/solr/",
    "/hnap1",
];
const BOT_AGENTS: &[&str] =
    &["sqlmap", "nikto", "nmap", "masscan", "zgrab", "nuclei", "acunetix", "wpscan", "dirbuster", "gobuster", "nessus", "fimap", "havij"];

fn is_bot(req: &Request) -> bool {
    let path = req.path().to_ascii_lowercase();
    if BOT_PATHS.iter().any(|p| path.starts_with(p)) || path.ends_with(".php") || path.ends_with(".asp") || path.ends_with(".aspx") {
        return true;
    }
    let ua = req.header("user-agent").unwrap_or("").to_ascii_lowercase();
    BOT_AGENTS.iter().any(|b| ua.contains(b))
}

// ----- the rate limit, shared by the workers ---------------------------

const SHARDS: usize = 32;

type Window = HashMap<(usize, String), (u32, Instant)>;

fn windows() -> &'static [Mutex<Window>] {
    static W: OnceLock<Vec<Mutex<Window>>> = OnceLock::new();
    W.get_or_init(|| (0..SHARDS).map(|_| Mutex::new(HashMap::new())).collect())
}

fn next_limiter() -> usize {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// `Some(seconds to wait)` when this address made too many requests this minute.
fn limited(id: usize, per_minute: u32, ip: &str) -> Option<u64> {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in ip.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100000001b3);
    }
    let shard = &windows()[(h as usize) % SHARDS];
    let mut map = shard.lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    if map.len() > 100_000 {
        map.retain(|_, (_, start)| now.duration_since(*start) < Duration::from_secs(60));
    }
    let entry = map.entry((id, ip.to_string())).or_insert((0, now));
    if now.duration_since(entry.1) >= Duration::from_secs(60) {
        *entry = (0, now);
    }
    entry.0 += 1;
    if entry.0 > per_minute {
        Some(60u64.saturating_sub(now.duration_since(entry.1).as_secs()).max(1))
    } else {
        None
    }
}

/// Compares secrets in a time that does not tell where they differ.
pub fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        })
    };
    let bytes: Vec<u8> = s.bytes().filter(|&c| c != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= val(c)? << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64() {
        assert_eq!(base64_decode("YWRhOnNlY3JldA==").unwrap(), b"ada:secret");
        assert_eq!(base64_decode("YQ").unwrap(), b"a");
        assert!(base64_decode("@@").is_none());
    }

    #[test]
    fn rate_limit_counts_per_address() {
        let id = next_limiter();
        assert!(limited(id, 2, "1.2.3.4").is_none());
        assert!(limited(id, 2, "1.2.3.4").is_none());
        assert!(limited(id, 2, "1.2.3.4").is_some());
        assert!(limited(id, 2, "5.6.7.8").is_none());
    }
}
