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

impl Native {
    /// Whether it applies to this path. Most have no scope: that is known
    /// without comparing (comparing zero bytes at the address of an empty
    /// `String` calls `memcmp`, which costs about 90 ns on some processors).
    #[inline]
    fn applies(&self, path: &str) -> bool {
        self.scope.is_empty() || path.starts_with(self.scope.as_str())
    }
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
            if !n.applies(path) {
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
                    if let Some(wait) = req.with_client_ip(|ip| limited(*id, *per_minute, ip)) {
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
            if !n.applies(path) {
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

/// Texts to look for, all in lowercase ASCII, with which of them start with
/// each byte and which have each byte second: one pass over the text finds
/// them all, without the cost of one search (and its calls to `memcmp`)
/// per pattern, and a pattern is only compared where its first two bytes are.
struct Needles {
    list: &'static [&'static str],
    first: [u64; 256],
    /// The patterns whose second byte is this one, and those of one byte.
    second: [u64; 256],
    /// `first` and `second` of the byte in lowercase, for the search without case.
    first_folded: [u64; 256],
    second_folded: [u64; 256],
    /// The patterns of one byte: the only ones that can start at the last byte.
    short: u64,
}

impl Needles {
    const fn new(list: &'static [&'static str]) -> Needles {
        assert!(list.len() <= 64);
        let mut first = [0u64; 256];
        let mut second = [0u64; 256];
        let mut short = 0u64;
        let mut i = 0;
        while i < list.len() {
            let p = list[i].as_bytes();
            let mut j = 0;
            while j < p.len() {
                assert!(p[j].is_ascii() && !p[j].is_ascii_uppercase());
                j += 1;
            }
            first[p[0] as usize] |= 1 << i;
            if p.len() == 1 {
                short |= 1 << i;
                let mut b = 0;
                while b < 256 {
                    second[b] |= 1 << i;
                    b += 1;
                }
            } else {
                second[p[1] as usize] |= 1 << i;
            }
            i += 1;
        }
        let mut first_folded = [0u64; 256];
        let mut second_folded = [0u64; 256];
        let mut b = 0;
        while b < 256 {
            let low = (b as u8).to_ascii_lowercase() as usize;
            first_folded[b] = first[low];
            second_folded[b] = second[low];
            b += 1;
        }
        Needles { list, first, second, first_folded, second_folded, short }
    }

    /// Whether one of them is in `text`: what `text.contains(p)` tells for
    /// each, or `text.to_ascii_lowercase().contains(p)` with `fold`.
    fn found_in(&self, text: &[u8], fold: bool) -> bool {
        let (first, second) = if fold { (&self.first_folded, &self.second_folded) } else { (&self.first, &self.second) };
        let Some(&last) = text.last() else {
            return false;
        };
        for (i, w) in text.windows(2).enumerate() {
            let m = first[w[0] as usize] & second[w[1] as usize];
            if m != 0 && self.starts_at(text, i, m, fold) {
                return true;
            }
        }
        let m = first[last as usize] & self.short;
        m != 0 && self.starts_at(text, text.len() - 1, m, fold)
    }

    /// Whether one of the patterns of the mask `m` starts at `i`.
    fn starts_at(&self, text: &[u8], i: usize, mut m: u64, fold: bool) -> bool {
        let low = |x: u8| if fold { x.to_ascii_lowercase() } else { x };
        while m != 0 {
            let p = self.list[m.trailing_zeros() as usize].as_bytes();
            m &= m - 1;
            if let Some(t) = text.get(i..i + p.len()) {
                if t.iter().zip(p).all(|(x, y)| low(*x) == *y) {
                    return true;
                }
            }
        }
        false
    }
}

static WAF: Needles = Needles::new(WAF_PATTERNS);

/// Whether the path and the query hold an attack.
fn waf_refuses(target: &str) -> bool {
    // Decoded twice: `%252e%252e` hides `..` from one decoding.
    let once = crate::http::decode(target);
    // A text with nothing to decode is given back as it is: no second search.
    let twice = match &once {
        std::borrow::Cow::Borrowed(t) => std::borrow::Cow::Borrowed(*t),
        std::borrow::Cow::Owned(o) => crate::http::decode(o),
    };
    if twice.is_ascii() {
        // Lowercase ASCII is all `to_lowercase` would change.
        WAF.found_in(twice.as_bytes(), true)
    } else {
        // Some other letters become ASCII ones in lowercase (the Kelvin sign is `k`).
        WAF.found_in(twice.to_lowercase().as_bytes(), false)
    }
}

/// A web application firewall on the path and the query.
fn waf(req: &Request) -> Option<Response> {
    waf_refuses(req.target()).then(|| Response::json(403, "{\"error\":\"request refused by the firewall\"}"))
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

static BOTS: Needles = Needles::new(BOT_AGENTS);

/// For each byte, the robots' paths whose second byte is that byte in
/// lowercase: a path is only compared with the prefixes that can start it.
static BOT_PATHS_BY_SECOND: [u32; 256] = by_second(BOT_PATHS);

const BOT_EXTENSIONS: &[&str] = &[".php", ".asp", ".aspx"];

const fn by_second(list: &[&str]) -> [u32; 256] {
    assert!(list.len() <= 32);
    let mut m = [0u32; 256];
    let mut c = 0;
    while c < 256 {
        let low = (c as u8).to_ascii_lowercase();
        let mut i = 0;
        while i < list.len() {
            let p = list[i].as_bytes();
            assert!(p.len() >= 2);
            if p[1] == low {
                m[c] |= 1 << i;
            }
            i += 1;
        }
        c += 1;
    }
    m
}

/// `path.to_ascii_lowercase().starts_with(prefix)`, without the copy.
fn starts_with_lower(path: &[u8], prefix: &str) -> bool {
    path.get(..prefix.len()).is_some_and(|p| p.eq_ignore_ascii_case(prefix.as_bytes()))
}

/// `path.to_ascii_lowercase().ends_with(suffix)`, without the copy.
fn ends_with_lower(path: &[u8], suffix: &str) -> bool {
    path.len() >= suffix.len() && path[path.len() - suffix.len()..].eq_ignore_ascii_case(suffix.as_bytes())
}

fn is_bot(req: &Request) -> bool {
    let path = req.path().as_bytes();
    let mut m = path.get(1).map_or(0, |&b| BOT_PATHS_BY_SECOND[b as usize]);
    while m != 0 {
        let p = BOT_PATHS[m.trailing_zeros() as usize];
        m &= m - 1;
        if starts_with_lower(path, p) {
            return true;
        }
    }
    // The extensions all end with a `p` or an `x`.
    if matches!(path.last(), Some(b'p' | b'P' | b'x' | b'X')) && BOT_EXTENSIONS.iter().any(|e| ends_with_lower(path, e)) {
        return true;
    }
    BOTS.found_in(req.header("user-agent").unwrap_or("").as_bytes(), true)
}

// ----- the rate limit, shared by the workers ---------------------------

const SHARDS: usize = 32;

type Window = HashMap<(usize, String), (u32, Instant)>;

/// A key of the windows read without making its `String`: a request from
/// an address already counted allocates nothing. Hashed and compared like
/// `(usize, String)` (SipHash stays: the addresses come from the clients).
trait WindowKey {
    fn parts(&self) -> (usize, &str);
}

impl WindowKey for (usize, String) {
    fn parts(&self) -> (usize, &str) {
        (self.0, &self.1)
    }
}

impl WindowKey for (usize, &str) {
    fn parts(&self) -> (usize, &str) {
        (self.0, self.1)
    }
}

impl<'a> std::borrow::Borrow<dyn WindowKey + 'a> for (usize, String) {
    fn borrow(&self) -> &(dyn WindowKey + 'a) {
        self
    }
}

impl std::hash::Hash for dyn WindowKey + '_ {
    /// What `(usize, String)` hashes: its two parts, one after the other.
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let (id, ip) = self.parts();
        id.hash(state);
        ip.hash(state);
    }
}

impl PartialEq for dyn WindowKey + '_ {
    fn eq(&self, other: &Self) -> bool {
        self.parts() == other.parts()
    }
}

impl Eq for dyn WindowKey + '_ {}

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
    let (count, start) = match map.get_mut(&(id, ip) as &dyn WindowKey) {
        Some(entry) => {
            if now.duration_since(entry.1) >= Duration::from_secs(60) {
                *entry = (0, now);
            }
            entry.0 += 1;
            *entry
        }
        None => {
            map.insert((id, ip.to_string()), (1, now));
            (1, now)
        }
    };
    if count > per_minute {
        Some(60u64.saturating_sub(now.duration_since(start).as_secs()).max(1))
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

    /// The firewall and the robot check as they were written before their
    /// one-pass search: both must refuse exactly the same requests.
    fn waf_before(target: &str) -> bool {
        let target = crate::http::percent_decode(target);
        let target = crate::http::percent_decode(&target).to_lowercase();
        WAF_PATTERNS.iter().any(|p| target.contains(p))
    }

    fn bot_before(path: &str, ua: &str) -> bool {
        let path = path.to_ascii_lowercase();
        if BOT_PATHS.iter().any(|p| path.starts_with(p)) || path.ends_with(".php") || path.ends_with(".asp") || path.ends_with(".aspx") {
            return true;
        }
        let ua = ua.to_ascii_lowercase();
        BOT_AGENTS.iter().any(|b| ua.contains(b))
    }

    fn request(target: &str, ua: Option<&str>) -> Request {
        let mut head = format!("GET {target} HTTP/1.1\r\n");
        let mut r = Request::default();
        r.method = (0, 3);
        r.target = (4, 4 + target.len() as u32);
        r.path_end = r.target.0 + target.find('?').unwrap_or(target.len()) as u32;
        if let Some(ua) = ua {
            let n = head.len() as u32;
            head.push_str("User-Agent: ");
            head.push_str(ua);
            r.headers.push(((n, n + 10), (n + 12, n + 12 + ua.len() as u32)));
            head.push_str("\r\n");
        }
        r.head = head.into_bytes();
        r
    }

    const TARGETS: &[&str] = &[
        "/",
        "/json",
        "/static/style.css",
        "/users/42?onglet=profil&page=2",
        "/?q=union+select+1",
        "/?q=UNION%20SELECT",
        "/?q=Union%2520All%2520Select",
        "/a/%252e%252e/etc",
        "/../etc/passwd",
        "/..%5Cwindows",
        "/?x=%3CScRiPt%3E",
        "/?x=%3Csvg/onload=1",
        "/?u=JavaScript:alert(1)",
        "/?x=${jndi:ldap://x}",
        "/?x=%24%7Benv:HOME%7D",
        "/?c=1;cat%20/etc/hosts",
        "/?c=a|bash",
        "/?c=%00",
        "/?q=benchmar\u{212A}(1)",
        "/?q=BENCHMARK(1)",
        "/?q=\u{130}nformation_schema",
        "/?q=\u{130}NFORMATION_SCHEMA",
        "/?q=information_schem",
        "/?q=' or '1'='1",
        "/?q=%27%20OR%201=1",
        "/?q=x or 1=1--",
        "/?q=pg_sleep(5)",
        "/?q=waitfor%20delay",
        "/?q=; rm -rf /",
        "/?q=$(curl x)",
        "/?q=%2",
        "/?q=%",
        "/caf\u{e9}/cr\u{e8}me",
        "/\u{1F600}?drop%20table",
        "/C:\\Windows",
        "/proc/self/environ",
        "/document.cookie",
        "/onerror=",
        "/ONLOAD=",
        "/<iframe",
        "/xp_cmdshell",
        "/sleep",
        "/sleep(",
        "/..",
        "/.",
    ];

    #[test]
    fn firewall_refuses_what_it_refused() {
        for t in TARGETS {
            assert_eq!(waf_refuses(t), waf_before(t), "{t}");
            assert_eq!(waf(&request(t, None)).is_some(), waf_before(t), "{t}");
        }
        assert!(waf_refuses("/?q=UNION%20SELECT"));
        assert!(waf_refuses("/?q=benchmar\u{212A}(1)"));
        assert!(!waf_refuses("/static/style.css"));
    }

    #[test]
    fn robots_are_the_same() {
        // `starts_with_lower` reads the paths as written: in lowercase.
        assert!(BOT_PATHS.iter().all(|p| !p.bytes().any(|b| b.is_ascii_uppercase())));
        let paths = [
            "/",
            "/.env",
            "/.ENV",
            "/.git/config",
            "/WP-Admin/",
            "/index.php",
            "/INDEX.PHP",
            "/a.asp",
            "/a.AspX",
            "/x.aspx/",
            "/hnap1",
            "/HNAP1",
            "/owa",
            "/owa/",
            "/solr/x",
            "/php",
            "/caf\u{e9}.php",
            "/\u{e9}",
            "/p",
            "/.as",
        ];
        let agents = [
            None,
            Some(""),
            Some("Mozilla/5.0 (X11; Linux x86_64; rv:132.0) Gecko/20100101 Firefox/132.0"),
            Some("sqlmap/1.7"),
            Some("Mozilla/5.0 (compatible; Nmap Scripting Engine)"),
            Some("NUCLEI"),
            Some("gobuste"),
            Some("Mozilla/5.0 zgrab/0.x"),
            Some("caf\u{e9} havij"),
            Some("fimap"),
        ];
        // Paths made of the bytes of the prefixes and extensions, in both cases.
        let alphabet = b"/.envgitsvnawsdstorewp-admlocnhuxr.phpPHPASPXaspx1HN";
        let mut x: u64 = 88_172_645_463_325_252;
        for _ in 0..50_000 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let mut y = x;
            let p: String = (0..x % 14)
                .map(|_| {
                    y = y.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                    alphabet[(y >> 33) as usize % alphabet.len()] as char
                })
                .collect();
            assert_eq!(is_bot(&request(&p, None)), bot_before(&p, ""), "{p}");
        }
        for p in BOT_PATHS {
            assert!(is_bot(&request(p, None)) && is_bot(&request(&p.to_ascii_uppercase(), None)), "{p}");
        }
        for p in paths {
            for ua in agents {
                assert_eq!(is_bot(&request(p, ua)), bot_before(p, ua.unwrap_or("")), "{p} {ua:?}");
            }
        }
    }

    /// The windows as they were counted before they were read without a
    /// `String`: the same answers, the same entries, the same hashes.
    /// The search by the first two bytes finds what `contains` finds, in
    /// texts full of the bytes of the patterns (cut, in capitals, at the ends).
    #[test]
    fn needles_find_what_contains_finds() {
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut rnd = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let all: Vec<&str> = WAF_PATTERNS.iter().chain(BOT_AGENTS).copied().collect();
        let mut alphabet: Vec<u8> = all.iter().flat_map(|p| p.bytes()).collect();
        alphabet.extend(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ \xc3\xa9\xff\x00");
        alphabet.sort();
        alphabet.dedup();
        let mut found = 0;
        for n in 0..100_000 {
            let len = (rnd() % 24) as usize;
            let mut t: Vec<u8> = (0..len).map(|_| alphabet[(rnd() % alphabet.len() as u64) as usize]).collect();
            if n % 3 == 0 {
                let mut p = all[(rnd() % all.len() as u64) as usize].as_bytes().to_vec();
                if rnd() % 2 == 0 {
                    p.make_ascii_uppercase();
                }
                if rnd() % 3 == 0 {
                    p.pop();
                }
                let at = (rnd() % (t.len() as u64 + 1)) as usize;
                t.splice(at..at, p);
            }
            let lower = t.to_ascii_lowercase();
            for (needles, list) in [(&WAF, WAF_PATTERNS), (&BOTS, BOT_AGENTS)] {
                let contains = |text: &[u8]| list.iter().any(|p| text.windows(p.len()).any(|w| w == p.as_bytes()));
                assert_eq!(needles.found_in(&t, true), contains(&lower), "{t:?}");
                assert_eq!(needles.found_in(&t, false), contains(&t), "{t:?}");
                found += contains(&lower) as u32;
            }
        }
        assert!(found > 25_000, "{found}");
        assert!(!WAF.found_in(b"", true) && !WAF.found_in(b"", false));
        assert!(WAF.found_in(b"\0", true) && WAF.found_in(b"a\0", false) && WAF.found_in(b"\0a", true));
    }

    #[test]
    fn rate_limit_as_before() {
        use std::hash::BuildHasher;
        let keys = HashMap::<(usize, String), ()>::new();
        for (id, ip) in [(0, ""), (1, "1.2.3.4"), (usize::MAX, "::1"), (7, "\u{e9}t\u{e9}"), (3, "a\u{ff}b")] {
            let owned = (id, ip.to_string());
            let view: &dyn WindowKey = &(id, ip);
            assert_eq!(keys.hasher().hash_one(&owned), keys.hasher().hash_one(view));
            assert!(std::borrow::Borrow::<dyn WindowKey>::borrow(&owned) == view);
        }
        let id = next_limiter();
        let mut before: HashMap<(usize, String), (u32, Instant)> = HashMap::new();
        let mut old = |ip: &str, per_minute: u32| {
            let now = Instant::now();
            let entry = before.entry((id, ip.to_string())).or_insert((0, now));
            if now.duration_since(entry.1) >= Duration::from_secs(60) {
                *entry = (0, now);
            }
            entry.0 += 1;
            (entry.0 > per_minute).then_some(entry.0)
        };
        for i in 0..50 {
            for ip in ["10.0.0.1", "10.0.0.2", "::1", ""] {
                let per_minute = if ip == "::1" { 20 } else { 5 };
                assert_eq!(limited(id, per_minute, ip).is_some(), old(ip, per_minute).is_some(), "{i} {ip}");
            }
        }
        let shard_total: usize = windows().iter().map(|w| w.lock().unwrap().keys().filter(|k| k.0 == id).count()).sum();
        assert_eq!(shard_total, 4);
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
