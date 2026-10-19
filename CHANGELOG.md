# Changelog

## 0.1.0 — unreleased

The first version of Facile, the web framework of the Rythme language.

- **Engine**: one thread per core with its own event loop and `SO_REUSEPORT`
  socket, HTTP/1.1 with keep-alive and pipelining (one write per batch),
  chunked bodies, `Expect: 100-continue`, limits on heads, bodies and slow
  clients, graceful stop on SIGTERM.
- **Applications in Rythme**: `app`, routes with parameters and wildcards,
  `mount`, static files (gzip, ETag, cache headers), pages with their SEO,
  views, Markdown, responses built from any value, `not_found`, `on_error`,
  `on_start`, `settings`.
- **The cache of purity**: pure `GET` responses are kept and given again.
- **Middlewares**: in Rythme (`fn(req, next)`) and native (`mw.*`: shield,
  WAF, robots, security headers, CSRF, CORS, rate limit, Basic, Bearer, body
  limit, gzip, request ids, timing, logs, HTTPS).
- **ORM**: models with rules, queries as values, relations, aggregates,
  pages, transactions with savepoints, hand-written SQL, automatic
  migrations, REST resources; memory, SQLite and PostgreSQL (TLS).
- **WebAssembly islands**: components in Rythme (`init`, `update`, `view`,
  effects) rendered on the server and run in the browser.
- **The `facile` command**: `new`, `dev` (live reload), `serve`, `check`,
  `routes`, `reference`.
- **The site**, written in Rythme and served by Facile, with its 3D scenes
  and its documentation in French and English.
