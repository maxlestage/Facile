# Middlewares

A middleware wraps the handlers: it sees the request before them and the response after them. Facile has two kinds, which combine.

## In Rythme: `fn(req, next)`

```rythme
## Adds a header to every response.
fn signature(req, next)
  res = next(req)
  with_header(res, "x-served-by", "Facile")
end

## Turns away those who are not signed in.
fn signed_in(req, next) = match req.cookie("session")
  Some(_) -> next(req)
  None -> redirect("/login")
end

## Gives the handlers something more.
fn user(req, next) = next({..req, user: read_session(req)})

app("Site")
  |> middleware(signature)
  |> middleware(signed_in)
```

`next(req)` runs the rest of the chain (the next middlewares, then the handler) and returns its response as a record `{status, headers, body}`, which the middleware may read and change. A middleware may also answer without calling `next`.

The middlewares of an application apply to **all its routes**, in the order they were added. To guard only part of a site, put it in an application mounted with `mount` (see [Routes](/en/docs/routes)).

## In Rust: `mw`

The middlewares of `mw` are written in Rust and run in the engine, before the cache and before Rythme. They cost almost nothing.

| Middleware | Effect |
|---|---|
| `mw.shield()` | all of the following at once: WAF, robots, security headers, CSRF, 300 requests/min, gzip, ids |
| `mw.security_headers()` | CSP (WebAssembly allowed), HSTS in production, `X-Frame-Options`, `nosniff`, `Referrer-Policy`, `Permissions-Policy` |
| `mw.waf()` | refuses SQL injections, XSS, path traversal, Log4Shell, command injections (403) |
| `mw.block_bots()` | refuses attack robots and the paths they probe (`/.env`, `/wp-admin`...) |
| `mw.csrf()` | refuses POST, PUT, PATCH, DELETE sent by another site |
| `mw.cors(["https://a.com"])` | CORS and preflight requests (`"*"` for all) |
| `mw.rate_limit(120)` | at most 120 requests a minute per address, then 429 and `Retry-After` |
| `mw.basic_auth(user, password)` | HTTP Basic authentication, compared in constant time |
| `mw.bearer(token)` | requires `Authorization: Bearer <token>` |
| `mw.body_limit(1000000)` | refuses bodies that are too large (413) |
| `mw.compress()` | gzip for texts, JSON, SVG and WebAssembly (computed once for cached responses) |
| `mw.request_id()` | an `X-Request-Id` on every response |
| `mw.timing()` | a `Server-Timing` header |
| `mw.logger()` | a line per request: method, path, status, duration |
| `mw.https()` | redirects `http://` to `https://` behind a proxy |
| `mw.headers({...})`, `mw.cache_control("...")` | headers on every response |

```rythme
app("API")
  |> middleware(mw.shield())
  |> middleware(mw.cors(["https://my-front.com"]))
  |> middleware(mw.logger())
```

## The order

For each request, Facile does, in this order:

1. the middlewares of `mw` that may refuse (WAF, robots, limit, authentication, CSRF, CORS);
2. the cache of pure responses;
3. static files;
4. the router, then **your Rythme middlewares**, then the handler;
5. the headers of `mw` (security, CORS, ids), compression, the log.
