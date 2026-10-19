# Reference

Every function Facile adds to Rythme, with what it does. This page is written by `facile reference --lang en`, from the code.

## Application

| Function | Effect |
|---|---|
| `app(name)` | A new web application, without routes yet. The file ends with the application to serve. |
| `route(app, method, path, handler)` | The application with one more route: `route(app, :get, "/taches/:id", detail)`. The handler gets the request. |
| `on_get(app, path, handler)` | `route(app, :get, path, handler)`. |
| `on_post(app, path, handler)` | `route(app, :post, path, handler)`. |
| `on_put(app, path, handler)` | `route(app, :put, path, handler)`. |
| `on_patch(app, path, handler)` | `route(app, :patch, path, handler)`. |
| `on_delete(app, path, handler)` | `route(app, :delete, path, handler)`. |
| `middleware(app, middleware)` | The application with a middleware around every route: a function `fn(req, next)`, or one of `mw`. |
| `mount(app, prefix, other_app)` | The routes of another application under a prefix, with its middlewares: `mount(app, "/admin", admin)`. |
| `assets(app, prefix, folder)` | Serves the files of a folder: `assets(app, "/static", "public")`, with gzip, ETag and cache headers. |
| `models(app, models)` | The models whose tables the server creates or completes when it starts. |
| `resource(app, path, model, actions)` | A REST API for a model: GET and POST on the path, GET, PUT, PATCH and DELETE on `path/:id`. Optional: the actions to keep (`[:list, :show]`). |
| `not_found(app, handler)` | The handler for paths without a route (a 404 page). |
| `on_error(app, handler)` | The handler called with the request and the error message when a handler fails. |
| `on_start(app, function)` | A function called once when the server starts (to fill a database, say). |
| `settings(app, settings)` | Settings: `{cache: false}`, `{body_limit: 1000000}`, `{workers: 4}`, `{lang: "fr"}`. |
| `database(app, url)` | The database: `memory://`, `sqlite://app.db`, `postgres://...`. `DATABASE_URL` wins when it is set. |

## Request

| Function | Effect |
|---|---|
| `req.header(name)` | A header of the request: `Some(value)` or `None`. Names have no case. |
| `req.cookie(name)` | A cookie of the request: `Some(value)` or `None`. |
| `req.body()` | The body of the request, as text. |
| `req.json()` | The body read as JSON: `Ok(value)` or `Err(message)`. |
| `req.form()` | The fields of a form sent by the browser, as a record. |
| `req.ip()` | The client's address (the first of X-Forwarded-For behind a proxy). |

## Responses

| Function | Effect |
|---|---|
| `html(content, status)` | An HTML response, from a text or a view. |
| `json(value, status)` | A JSON response: records are objects, lists arrays. |
| `status(response, code)` | The response with another status: `"Introuvable" \|> status(404)`. |
| `redirect(url, code)` | A redirection (303 See Other unless given: 301, 302, 307, 308). |
| `with_header(response, name, value)` | The response with a header. |
| `with_cookie(response, name, value, options)` | The response with a cookie (HttpOnly, SameSite=Lax, Secure in production). Options: `{max_age, path, same_site, http_only, secure}`. |
| `send_file(path)` | The content of a file, with its media type. |
| `no_content()` | An empty response (204). |
| `problem(status, detail)` | A JSON error: `problem(422, "le titre manque")` gives `{"error": "le titre manque"}`. |

## Middlewares

| Function | Effect |
|---|---|
| `next(req)` | Runs the rest of the chain (the next middlewares, then the handler) and returns its response `{status, headers, body}`. |

## mw

| Function | Effect |
|---|---|
| `mw.security_headers(options)` | Security headers: Content-Security-Policy (WebAssembly allowed), HSTS, X-Content-Type-Options, X-Frame-Options, Referrer-Policy, Permissions-Policy, Cross-Origin-Opener-Policy. Options: `{csp: "..."}`. |
| `mw.cors(origins, options)` | CORS: the origins allowed to call the API (`"*"` for all), and the answers to preflight requests. |
| `mw.rate_limit(per_minute)` | At most this many requests per minute and per address; then 429 with Retry-After. |
| `mw.logger()` | A line per request on the standard output: method, path, status, duration. |
| `mw.compress()` | Gzip for texts, JSON, SVG and WebAssembly when the browser accepts it (computed once for cached responses). |
| `mw.request_id()` | An `X-Request-Id` on every response (the one received, or a new one). |
| `mw.timing()` | A `Server-Timing` header: the time spent on the request, shown by the browsers' tools. |
| `mw.basic_auth(user, password)` | HTTP Basic authentication (compared in constant time). |
| `mw.bearer(token)` | Requires `Authorization: Bearer <token>` (compared in constant time). |
| `mw.body_limit(bytes)` | Refuses bodies larger than this (413). |
| `mw.headers(headers)` | Headers added to every response: `mw.headers({"x-powered-by": "Facile"})`. |
| `mw.cache_control(value)` | A `Cache-Control` header on every response. |
| `mw.https()` | Behind a proxy (Heroku), redirects `http://` to `https://`. |
| `mw.waf()` | A web application firewall: refuses SQL injections, XSS, path traversal, Log4Shell and command injections in paths and queries (403). |
| `mw.block_bots()` | Refuses attack robots and the paths they probe (`/.env`, `/wp-admin`...). |
| `mw.csrf()` | Refuses POST, PUT, PATCH and DELETE requests sent by another site (Origin and Sec-Fetch-Site checked). |
| `mw.shield()` | All the protections at once: WAF, robots, security headers, CSRF, 300 requests a minute, compression, request ids. |

## Pages and views

| Function | Effect |
|---|---|
| `page(settings, content...)` | A whole HTML page. Settings: `{title, description, lang, url, image, css: [...], js: [...], head: [...], json_ld, theme_color, alternates, status}`. |
| `island(component, props)` | A component computed on the server and brought to life in the browser by WebAssembly: `island(compteur.component, {depart: 0})`. |
| `stylesheet(href)` | A `<link rel="stylesheet">`. |
| `script(src)` | A `<script type="module">` loaded from a file. |
| `meta(attributes)` | A `<meta>` element: `meta({name: "author", content: "Ada"})`. |
| `h(tag, attributes, children)` | Any HTML element: `h("input", {name: "q"})`, `h("my-widget", ["text"])`. |
| `raw(html)` | HTML written as it is, without escaping (only for HTML you trust). |
| `fragment(children, ...)` | Several children without an element around them. |
| `render(view)` | The HTML of a view, as text. |
| `markdown(text)` | Markdown as a view: headings with anchors, emphasis, code (Rythme colored), links, lists, quotes, tables. HTML in the text is shown as text. |
| `toc(text)` | The headings of a Markdown text: `[{level, title, anchor}]`, for a table of contents. |
| `highlight(code)` | Rythme code, colored (`<span class="tok-keyword">`...). |

## Islands: effects

| Function | Effect |
|---|---|
| `with_effects(state, effects)` | What `init` or `update` returns to ask the browser for effects with the new state: `with_effects(s, [after(1000, :tic)])`. |
| `fetch_json(url, to_message)` | Fetches JSON; its result goes to `update` as `to_message(Ok(value))` or `to_message(Err(message))`. |
| `send_json(method, url, body, to_message)` | Sends JSON (`:post`, `:put`, `:patch`, `:delete`); the answer goes to `update` like for `fetch_json`. |
| `after(milliseconds, message)` | Gives the message to `update` after this many milliseconds. |
| `save(key, value)` | Keeps a value in the browser (localStorage), as JSON. |
| `load(key, to_message)` | Reads a value kept by `save`: `to_message(Some(value))` or `to_message(None)`. |
| `navigate(url)` | Goes to another page. |
| `focus(selector)` | Puts the focus on an element of the page (`"#nom"`). |
| `evaluate(code)` | Runs a Rythme program apart: `{ok, output, errors}`. It stops after a few million calls. |

## Models and queries

| Function | Effect |
|---|---|
| `model(table, fields, options)` | A model: a table and its fields. `model("taches", {titre: :text, fait: field(:bool, {default: false})})`. Options: `{timestamps: false}`. A model is also the query of all its rows. |
| `field(kind, options)` | A field with rules: `field(:text, {required: true, min: 3, max: 120, unique: true, email: true, one_of: [...], default: "x", index: true})`. |
| `belongs_to(table, options)` | A relation to a row of another table: `projet: belongs_to("projets")` stores `projet_id`; `include(q, :projet)` adds the row. |
| `has_many(table, field)` | The rows of another table that refer to this one: `taches: has_many("taches", :projet)`; `include(q, :taches)` adds them. |
| `where(query, field, operator, value)` | Keeps the rows where the field has the value (`where(q, :fait, false)`), or compares it: `where(q, :prix, "<", 10)`, `:contains`, `:starts_with`, `:in`... |
| `where_any(query, conditions)` | Keeps the rows that meet one of the conditions: `where_any(q, [[:ville, "Paris"], [:ville, "Lyon"]])`. |
| `search(query, fields, text)` | The rows where one of the fields contains the text (case ignored). |
| `order_by(query, field, direction)` | Sorts the rows by a field, :asc (the default) or :desc; several calls sort by several fields. |
| `limit(query, n)` | At most n rows. |
| `offset(query, n)` | Skips the first n rows. |
| `include(query, relation)` | Adds a relation to each row (`belongs_to` or `has_many`), in one more query, not one per row. |
| `pick(query, fields)` | Only these fields in the records. |

## db

| Function | Effect |
|---|---|
| `db.all(query)` | The rows of a query, as a list of records. |
| `db.first(query)` | The first row: `Some(record)` or `None`. |
| `db.find(model, id)` | The row of this id: `Some(record)` or `None`. |
| `db.count(query)` | The number of rows. |
| `db.exists(query)` | Whether the query has rows. |
| `db.sum(query, field)` | The sum of a field: `Some(n)`, or `None` without rows. |
| `db.avg(query, field)` | The average of a field: `Some(x)` or `None`. |
| `db.min(query, field)` | The smallest value: `Some(x)` or `None`. |
| `db.max(query, field)` | The largest value: `Some(x)` or `None`. |
| `db.count_by(query, field)` | How many rows for each value of a field, as a Dict. |
| `db.insert(model, record)` | Adds a row, checked by the rules of the fields: `Ok(record with its id)` or `Err({status: 422, error, fields})`. |
| `db.insert_all(model, records)` | Adds several rows, all or none: `Ok(records)` or the first `Err`. |
| `db.update(model, id, changes)` | Changes the fields given of a row: `Ok(record)`, `Err(:not_found)` or `Err({status: 422, ...})`. |
| `db.save(model, record)` | Inserts a record without id, updates a record with one. |
| `db.update_where(query, changes)` | Changes every row of a query: `Ok(number of rows)`. |
| `db.delete(model, id)` | Deletes a row: whether it existed. |
| `db.delete_where(query)` | Deletes the rows of a query: how many. |
| `db.paginate(query, page, per_page)` | A page of rows: `{items, page, per_page, total, pages}` (20 per page unless given). |
| `db.transaction(function)` | Runs the function in a transaction: kept if it returns anything but `Err(...)`, undone otherwise. |
| `db.sql(sql, parameters)` | SQL written by hand, with `?` for the parameters: `Ok(records)` or `Err(message)`. |
| `db.validate(model, record)` | Checks a record by the rules of the fields, without writing it. |

## HTML elements

Each is called `tag()`, `tag(children)`, `tag(attributes)` or `tag(attributes, children)`; `h("name", ...)` gives the others.

`div`, `span`, `p`, `a`, `h1`, `h2`, `h3`, `h4`, `h5`, `h6`, `ul`, `ol`, `li`, `nav`, `header`, `footer`, `main`, `section`, `article`, `aside`, `button`, `img`, `form`, `label`, `strong`, `em`, `code`, `pre`, `small`, `br`, `hr`, `textarea`, `video`, `audio`, `canvas`, `iframe`, `figure`, `figcaption`, `blockquote`, `mark`, `dl`, `dt`, `dd`, `details`, `summary`, `svg`, `table`, `thead`, `tbody`, `tr`, `th`, `td`, `kbd`, `abbr`, `cite`, `picture`, `address`, `fieldset`, `legend`, `output`, `progress`, `meter`, `dialog`
