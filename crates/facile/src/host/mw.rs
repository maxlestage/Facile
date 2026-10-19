//! `mw`: the middlewares written in Rust, as fast as the engine.
//!
//! `middleware(app, mw.cors(["*"]))`. They wrap the middlewares written in
//! Rythme: those that can refuse a request (limits, authentication, WAF) run
//! before the cache and the handlers, those that add headers run on every
//! response, cached ones included.

use facile_core::host;
use facile_core::util::{self, error, int_arg, text_arg, tr};
use rythme::builtins::Builtin;
use rythme::diagnostic::Fallible;
use rythme::interp::Interp;
use rythme::source::Span;
use rythme::value::Value;

fn native(kind: &str, mut fields: Vec<(&str, Value)>) -> Value {
    fields.insert(0, ("native", util::sym(kind)));
    fields.push(("scope", Value::text("")));
    util::record(fields)
}

fn opts(a: &[Value], i: usize) -> Value {
    match a.get(i) {
        Some(v @ Value::Record(_)) => v.clone(),
        _ => util::record(vec![]),
    }
}

fn security_headers(_: &mut Interp, a: &[Value], _: Span) -> Fallible<Value> {
    Ok(native("security_headers", vec![("options", opts(a, 0))]))
}

fn cors(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let origins = match a.first() {
        None => Value::list(vec![Value::text("*")]),
        Some(Value::Text(t)) => Value::list(vec![Value::text(t)]),
        Some(l @ Value::List(_)) => l.clone(),
        Some(_) => {
            return Err(error(
                "mw.cors",
                tr(
                    "give the allowed origins: `[\"https://a.fr\"]` or `\"*\"`",
                    "donnez les origines permises : `[\"https://a.fr\"]` ou `\"*\"`",
                ),
                span,
            ))
        }
    };
    Ok(native("cors", vec![("origins", origins), ("options", opts(a, 1))]))
}

fn rate_limit(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let n = int_arg(a, 0, "mw.rate_limit", span)?;
    if n <= 0 {
        return Err(error(
            "mw.rate_limit",
            tr("the limit is a number of requests per minute, above 0", "la limite est un nombre de requêtes par minute, au-dessus de 0"),
            span,
        ));
    }
    Ok(native("rate_limit", vec![("per_minute", Value::Int(n)), ("options", opts(a, 1))]))
}

fn logger(_: &mut Interp, _: &[Value], _: Span) -> Fallible<Value> {
    Ok(native("logger", vec![]))
}

fn compress(_: &mut Interp, _: &[Value], _: Span) -> Fallible<Value> {
    Ok(native("compress", vec![]))
}

fn request_id(_: &mut Interp, _: &[Value], _: Span) -> Fallible<Value> {
    Ok(native("request_id", vec![]))
}

fn timing(_: &mut Interp, _: &[Value], _: Span) -> Fallible<Value> {
    Ok(native("timing", vec![]))
}

fn basic_auth(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let user = text_arg(a, 0, "mw.basic_auth", span)?;
    let pass = text_arg(a, 1, "mw.basic_auth", span)?;
    Ok(native("basic_auth", vec![("user", Value::text(user)), ("password", Value::text(pass)), ("realm", Value::text("Facile"))]))
}

fn bearer(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let token = text_arg(a, 0, "mw.bearer", span)?;
    if token.len() < 16 {
        return Err(error(
            "mw.bearer",
            tr("a token of at least 16 characters, please", "un jeton d'au moins 16 caractères, s'il vous plaît"),
            span,
        ));
    }
    Ok(native("bearer", vec![("token", Value::text(token))]))
}

fn body_limit(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let n = int_arg(a, 0, "mw.body_limit", span)?;
    Ok(native("body_limit", vec![("bytes", Value::Int(n.max(0)))]))
}

fn headers(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let r = util::record_arg(a, 0, "mw.headers", span)?;
    Ok(native("headers", vec![("headers", Value::Record(std::rc::Rc::new(r.clone())))]))
}

fn https(_: &mut Interp, _: &[Value], _: Span) -> Fallible<Value> {
    Ok(native("https", vec![]))
}

fn waf(_: &mut Interp, _: &[Value], _: Span) -> Fallible<Value> {
    Ok(native("waf", vec![]))
}

fn block_bots(_: &mut Interp, _: &[Value], _: Span) -> Fallible<Value> {
    Ok(native("block_bots", vec![]))
}

fn csrf(_: &mut Interp, _: &[Value], _: Span) -> Fallible<Value> {
    Ok(native("csrf", vec![]))
}

fn cache_control(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let v = text_arg(a, 0, "mw.cache_control", span)?;
    Ok(native("headers", vec![("headers", util::record(vec![("cache-control", Value::text(v))]))]))
}

/// Everything a production site wants, in one: WAF, attack robots blocked,
/// security headers, cross-site form checks, a rate limit, compression,
/// request ids and logs.
fn shield(_: &mut Interp, _: &[Value], _: Span) -> Fallible<Value> {
    Ok(native("shield", vec![]))
}

pub static MW: &[Builtin] = &[
    host!("security_headers", "mw.security_headers(options)", 0, 1, security_headers,
        "Security headers: Content-Security-Policy (WebAssembly allowed), HSTS, X-Content-Type-Options, X-Frame-Options, Referrer-Policy, Permissions-Policy, Cross-Origin-Opener-Policy. Options: `{csp: \"...\"}`.",
        "Les en-têtes de sécurité : Content-Security-Policy (WebAssembly permis), HSTS, X-Content-Type-Options, X-Frame-Options, Referrer-Policy, Permissions-Policy, Cross-Origin-Opener-Policy. Options : `{csp: \"...\"}`."),
    host!("cors", "mw.cors(origins, options)", 0, 2, cors,
        "CORS: the origins allowed to call the API (`\"*\"` for all), and the answers to preflight requests.",
        "CORS : les origines autorisées à appeler l'API (`\"*\"` pour toutes), et les réponses aux requêtes préalables."),
    host!("rate_limit", "mw.rate_limit(per_minute)", 1, 2, rate_limit,
        "At most this many requests per minute and per address; then 429 with Retry-After.",
        "Au plus ce nombre de requêtes par minute et par adresse ; ensuite 429 avec Retry-After."),
    host!("logger", "mw.logger()", 0, 0, logger,
        "A line per request on the standard output: method, path, status, duration.",
        "Une ligne par requête sur la sortie standard : méthode, chemin, statut, durée."),
    host!("compress", "mw.compress()", 0, 0, compress,
        "Gzip for texts, JSON, SVG and WebAssembly when the browser accepts it (computed once for cached responses).",
        "Gzip pour les textes, le JSON, le SVG et le WebAssembly quand le navigateur l'accepte (calculé une fois pour les réponses en cache)."),
    host!("request_id", "mw.request_id()", 0, 0, request_id,
        "An `X-Request-Id` on every response (the one received, or a new one).",
        "Un `X-Request-Id` sur chaque réponse (celui reçu, ou un nouveau)."),
    host!("timing", "mw.timing()", 0, 0, timing,
        "A `Server-Timing` header: the time spent on the request, shown by the browsers' tools.",
        "Un en-tête `Server-Timing` : le temps passé sur la requête, affiché par les outils des navigateurs."),
    host!("basic_auth", "mw.basic_auth(user, password)", 2, 2, basic_auth,
        "HTTP Basic authentication (compared in constant time).", "L'authentification HTTP Basic (comparée en temps constant)."),
    host!("bearer", "mw.bearer(token)", 1, 1, bearer,
        "Requires `Authorization: Bearer <token>` (compared in constant time).", "Exige `Authorization: Bearer <jeton>` (comparé en temps constant)."),
    host!("body_limit", "mw.body_limit(bytes)", 1, 1, body_limit,
        "Refuses bodies larger than this (413).", "Refuse les corps plus grands que cela (413)."),
    host!("headers", "mw.headers(headers)", 1, 1, headers,
        "Headers added to every response: `mw.headers({\"x-powered-by\": \"Facile\"})`.", "Des en-têtes ajoutés à chaque réponse : `mw.headers({\"x-powered-by\": \"Facile\"})`."),
    host!("cache_control", "mw.cache_control(value)", 1, 1, cache_control,
        "A `Cache-Control` header on every response.", "Un en-tête `Cache-Control` sur chaque réponse."),
    host!("https", "mw.https()", 0, 0, https,
        "Behind a proxy (Heroku), redirects `http://` to `https://`.", "Derrière un proxy (Heroku), redirige `http://` vers `https://`."),
    host!("waf", "mw.waf()", 0, 0, waf,
        "A web application firewall: refuses SQL injections, XSS, path traversal, Log4Shell and command injections in paths and queries (403).",
        "Un pare-feu applicatif : refuse les injections SQL, le XSS, la remontée de dossiers, Log4Shell et les injections de commandes dans les chemins et les requêtes (403)."),
    host!("block_bots", "mw.block_bots()", 0, 0, block_bots,
        "Refuses attack robots and the paths they probe (`/.env`, `/wp-admin`...).", "Refuse les robots d'attaque et les chemins qu'ils sondent (`/.env`, `/wp-admin`...)."),
    host!("csrf", "mw.csrf()", 0, 0, csrf,
        "Refuses POST, PUT, PATCH and DELETE requests sent by another site (Origin and Sec-Fetch-Site checked).",
        "Refuse les requêtes POST, PUT, PATCH et DELETE envoyées par un autre site (Origin et Sec-Fetch-Site vérifiés)."),
    host!("shield", "mw.shield()", 0, 0, shield,
        "All the protections at once: WAF, robots, security headers, CSRF, 300 requests a minute, compression, request ids.",
        "Toutes les protections d'un coup : WAF, robots, en-têtes de sécurité, CSRF, 300 requêtes par minute, compression, identifiants de requête."),
];
