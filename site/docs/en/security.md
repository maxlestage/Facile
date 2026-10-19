# Security

Facile is safe by default where it can be, and gives the rest in one line: `middleware(app, mw.shield())`.

## What is always true

- **No SQL injection**: values are always parameters; table and field names are checked when the model is made.
- **No HTML injection**: every text of a view is escaped; `raw` is explicit, and `markdown` shows HTML as text and refuses `javascript:` links.
- **No response splitting**: a header holding a line break is refused.
- **No request smuggling**: a request with both `Content-Length` and `Transfer-Encoding: chunked` is refused; so are heads above 16 KB (431) and bodies that are too large (413).
- **No path traversal**: `assets` and `send_file` refuse `..`.
- **Slow clients** block nothing: a request must arrive whole within 30 seconds, an idle connection closes after 75.
- **Safe cookies**: `HttpOnly`, `SameSite=Lax`, and `Secure` in production.
- **Quiet errors** in production: the details stay in the logs.

## `mw.shield()`

| Protection | Details |
|---|---|
| Web application firewall | SQL injections, XSS, path traversal, Log4Shell, command injections in the path and the query (decoded twice) |
| Robots | paths probed by scanners (`/.env`, `/.git`, `/wp-admin`, `*.php`...) and known attack tools |
| Headers | Content-Security-Policy, `X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`, `Permissions-Policy`, `Cross-Origin-Opener-Policy`, HSTS in production |
| CSRF | requests that write, sent by another site (`Sec-Fetch-Site`, `Origin`), are refused |
| Rate limit | 300 requests a minute per address, shared by the workers |
| Compression and ids | gzip, `X-Request-Id` |

The default Content-Security-Policy only allows the site's own scripts and `wasm-unsafe-eval` (for the islands): no inline script, no script from another domain. Change it with `mw.security_headers({csp: "..."})`.

## Authentication

```rythme
admin = app("Admin")
  |> on_get("/", dashboard)
  |> middleware(mw.basic_auth("admin", unwrap_or(env("ADMIN_PASSWORD"), "")))

api = app("API")
  |> resource("/orders", orders)
  |> middleware(mw.bearer(unwrap_or(env("API_TOKEN"), "")))

app("Site") |> mount("/admin", admin) |> mount("/api", api)
```

Secrets are compared in constant time. For sessions, a Rythme middleware reads a cookie and adds the user to the request (see [Middlewares](/en/docs/middlewares)).
