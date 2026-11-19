# Changelog

## 0.1.4 — unreleased

Faster, with the same responses byte for byte (the pages of the site, the
example and its REST API on memory, SQLite and PostgreSQL, a model with
floats on the three databases, and the benchmark compared with 0.1.3; only
the seconds of `Retry-After` follow the clock). Times measured inside the
process, medians of 9 interleaved runs:

- **JSON**: floats are written with `zmij` (already used by `serde_json`)
  instead of `fmt`, about three times faster (75 → 22 ns a float). Both find
  the same shortest digits and write them the same way between 1e-5 and
  1e16; they differ only on a tie, which needs at least 16 significant
  digits: those texts, and the numbers outside that range, still go through
  `fmt`. A test compares the two on nearly two million floats, ties included.
- **`memory://`**: a row keeps its cells in the order of the table's columns
  instead of a map by name: each read finds them by position, without
  hashing the names, and sorting by `id` no longer looks the rows up.
- Together: 30 rows with a float each as JSON 15.6 → 13.7 µs (−12 %), the
  first 5 of them 4.8 → 4.3 µs (−10 %), the same 30 through `resource`
  14.8 → 12.9 µs (−13 %), one of them 2463 → 2330 ns (−5 %); the example's
  REST API 2397 → 2293 ns (−4 %), filtered and sorted 4707 → 4473 ns (−5 %),
  its JSON read from the database 4040 → 3851 ns (−5 %); the benchmark's
  `POST /echo` 1233 → 1172 ns (−5 %).

## 0.1.3 — unreleased

Faster, with the same responses byte for byte (the pages of the site, the
example and its REST API on memory, SQLite and PostgreSQL, and the benchmark
compared with 0.1.2; only the seconds of `Retry-After` follow the clock).
Times measured inside the process, medians of 7 interleaved runs;
`mw.shield()` measured with its rate limit set high:

- **The rate limit** reads the address of the socket without `format!`, and
  an address already counted without making its `String` (the windows are
  still hashed with SipHash: the addresses come from the clients). Behind
  `mw.shield()`, a page of the site from the cache: 488 → 414 ns (−15 %), a
  JSON response: 309 → 242 ns (−22 %).
- **Robots**: the user agent is searched by the first two bytes of the
  patterns instead of the first one, and a path is only compared with the
  prefixes whose second byte it has (30 to 60 ns → 2 to 10 ns). **Firewall**:
  a target with nothing to decode is not searched a second time.
- Together, behind `mw.shield()`: a page of the documentation from the cache
  564 → 376 ns (−33 %), `/static/css/site.css` 674 → 445 ns (−34 %), a
  request behind a proxy (`X-Forwarded-For`) 314 → 270 ns (−14 %), a JSON
  response 308 → 198 ns (−36 %), with the user agent of Firefox 421 → 289 ns
  (−31 %).
- **ORM**: the shape of the records and the kind of each column found once
  per query instead of once per row; the rows of `memory://` keep their
  cells in a map hashed with FxHash (their keys are the model's columns,
  never the client's). **JSON**: texts are looked at 8 bytes at a time for
  what needs escaping (a text of 125 bytes: 86 → 22 ns). 30 rows as JSON:
  19.0 → 15.9 µs (−16 %); the example's REST API: 2735 → 2429 ns (−11 %).
- **Headers** given by the application are checked for line breaks reading
  all their bytes at once instead of character by character: a response
  given `with_cookie` then `with_header` behind a middleware written in
  Rythme, 3594 → 3360 ns (−7 %).

## 0.1.2 — unreleased

Faster, with the same responses byte for byte (the pages of the site, the
example and its REST API on memory, SQLite and PostgreSQL, and the benchmark
compared with 0.1.1). Times measured inside the process, medians of 7
interleaved runs; `mw.shield()` measured with its rate limit set high:

- **The middlewares of `mw`**: those without a scope no longer compare an
  empty prefix with the path at each request and on each response (comparing
  zero bytes at the address of an empty `String` costs about 90 ns on some
  processors). Behind `mw.shield()`, a response from the cache: 1639 → 306 ns
  (−81 %); a page of the site's documentation: 2233 → 464 ns (−79 %); the
  example's REST API: 3606 → 2670 ns (−26 %).
- **Firewall and robots**: the path and the query are searched once for all
  the patterns, without copies when there is nothing to decode (the Unicode
  lowercase is still used for texts that are not ASCII); the user agent is
  read without a copy. `/static/style.css`: 606 → 48 ns (−92 %), the user
  agent of Firefox: 94 → 80 ns (−15 %).
- **Static files**: found without copying their path, their head written
  without building a response. 164 → 90 ns (−46 %).
- **`next(req)`, `with_header`, `with_cookie`, `status`**: the body made by
  Rythme code is given back without a copy, and the content type of JSON,
  HTML and text without a list to make. A page of 30 kB given back by
  `next`: 11 µs → 47 ns; the names of headers read once instead of twice
  (−68 %).
- **Request ids** written without `format!`: 34 → 8 ns. **Queries and
  forms** decoded straight into their fields (−15 to −30 %);
  `req.header(...)` (−33 %), `req.cookie(...)` (−8 %) and the address
  behind a proxy (−20 %) without intermediate copies.

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
