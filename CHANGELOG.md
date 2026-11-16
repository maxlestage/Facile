# Changelog

## 0.1.1 — unreleased

Faster, with the same responses byte for byte (the pages of the site, the
example and the benchmark compared with 0.1.0). Times measured inside the
process (`examples/inproc`), medians of 7 interleaved runs, without the cache:

- **Views**: the elements made by `div(...)`, `p(...)`... are known by their
  shape, without looking their fields up by name; their tag is a text made
  once, not copied for each element; attributes and texts are escaped in
  slices, not character by character. A list of 30 items: 32.9 → 22.7 µs
  (−31 %).
- **ORM**: the rows read from the database no longer copy the names of the
  columns and of the table for each row. 20 rows as JSON: 15.3 → 11.1 µs
  (−27 %).
- **Small records stay inside their value**: the responses of `json(...)` and
  `html(...)`, the route parameters and the JSON objects of up to four keys
  are made without a list to allocate and free. `json({...})`: 543 → 511 ns
  (−6 %), `/users/:id`: 692 → 648 ns (−6 %), a JSON body read and sent back:
  1268 → 1144 ns (−10 %).
- **JSON**: integers are written without the machinery of `fmt` (itoa). 30
  records as JSON: 6.4 → 5.9 µs (−8 %).

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
- **Dependencies at their latest version**: socket2 0.6, sha2 0.11 and
  hmac 0.13, signal-hook 0.4, rusqlite 0.41 (SQLite 3.54), tokio-postgres-rustls
  0.14, webpki-roots 1.0. Facile now needs Rust 1.89 or newer.
