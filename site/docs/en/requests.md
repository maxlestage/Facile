# Requests and responses

## The request

A handler gets a record:

| Field | Value |
|---|---|
| `req.method` | `:get`, `:post`, `:put`, `:patch`, `:delete`... |
| `req.path` | the path, without the query (`/products/42`) |
| `req.params` | the path parameters: `req.params.id` |
| `req.query` | the URL parameters: `?page=2` gives `req.query.page`, a text |

And functions to read the rest:

```rythme
fn info(req) = {
  agent: req.header("user-agent"),        # Some(text) or None
  theme: req.cookie("theme"),             # Some(text) or None
  ip: req.ip(),                           # the first of X-Forwarded-For behind a proxy
}

fn create(req) = match req.json()         # Ok(value) or Err(message)
  Ok(data) -> json(data, 201)
  Err(e) -> problem(400, e)
end

fn form_sent(req) = do
  fields = req.form()                     # a record: fields.email
  "Thanks, {unwrap_or(get(fields, "name"), "stranger")}!"
end
```

`req.body()` gives the body as text. JSON is read straight into Rythme values: objects become records, `null` becomes `None`.

## What a handler returns

| Returned value | Response |
|---|---|
| a text | `200`, `text/plain` |
| a view (`div(...)`) | `200`, HTML |
| `page(...)` | a whole HTML document |
| a record, a list, a number | `200`, JSON |
| `Ok(x)`, `Some(x)` | the response of `x` |
| `None` | `404` |
| `Err(:not_found)`, `Err(:forbidden)`... | the matching status, as JSON |
| `Err({status: 422, ...})` | that status, and the record as JSON |
| `Err("message")` | `400` and `{"error": "message"}` |
| `()` | `204`, without a body |

Errors are values: Rythme's `?` operator carries an `Err` up to the response.

```rythme
fn finish(req) = do
  task = db.update(tasks, req.params.id, {done: true})?   # Err(:not_found) becomes 404
  json(task)
end
```

## Building a response

```rythme
html("<p>Hello</p>")                         # HTML, or a view
json({ok: true}, 201)                        # JSON, with a status
"Not here" |> status(404)                    # any response, another status
redirect("/login")                           # 303 See Other (301, 302, 307, 308 too)
send_file("reports/march.pdf")               # a file, with its media type
no_content()                                 # 204
problem(422, "the title is missing")         # {"error": "the title is missing"} and 422

json(products)
  |> with_header("cache-control", "public, max-age=60")
  |> with_cookie("seen", "yes", {max_age: 86400, same_site: "lax"})
```

`with_cookie` sets `HttpOnly` and `SameSite=Lax` by default, and `Secure` in production. A header cannot hold a line break: Facile refuses it.

## The cache of purity

Rythme is **purely functional**: a function always gives the same result for the same arguments. Facile builds on it: when the response to a `GET` read nothing but its path and its query — no header, no cookie, no database, no clock, no file — it is kept and given again as it is to the next requests, in tens of nanoseconds.

Nothing to declare: Facile watches what the handler reads. As soon as it calls `req.header`, `db.all`, `now()`, `random()`, `read_file`..., its response is not kept. To do without it entirely:

```rythme
app("Site") |> settings({cache: false})
```
