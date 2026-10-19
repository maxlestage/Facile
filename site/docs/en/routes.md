# Routes and pages

A route ties an **HTTP method** and a **path** to a handler. Routes are added to the application, in any order.

## Declaring routes

```rythme
app("Shop")
  |> on_get("/", home)
  |> on_get("/products/:id", product)
  |> on_post("/cart", add)
  |> on_put("/cart/:id", replace)
  |> on_patch("/cart/:id", change)
  |> on_delete("/cart/:id", remove)
  |> route(:options, "/cart", options)
  |> route(:any, "/probe", probe)
```

`on_get(app, path, handler)` is `route(app, :get, path, handler)`. `:any` accepts every method. A `HEAD` request uses the `GET` route, without a body in the response.

## Path parameters

```rythme
fn product(req) = "Product number {req.params.id}"

fn file(req) = "Path asked: {req.params.path}"

app("Example")
  |> on_get("/products/:id", product)
  |> on_get("/files/*path", file)
```

- `:name` captures one segment; its value is a text (`parse_int(req.params.id)` for a number);
- `*name` captures the rest of the path, slashes included;
- `/a/b/` and `/a/b` are the same route.

Paths without parameters are found in a hash table; the others are compared segment by segment, in the order they were declared.

## Groups: `mount`

An application can hold another one under a prefix, with its own middlewares:

```rythme
admin = app("Admin")
  |> on_get("/", dashboard)
  |> on_get("/orders", orders)
  |> middleware(mw.basic_auth("admin", unwrap_or(env("ADMIN_PASSWORD"), "secret")))

app("Shop")
  |> on_get("/", home)
  |> mount("/admin", admin)
```

The routes of `admin` become `/admin` and `/admin/orders`, and only they go through its authentication.

## Static files

```rythme
app("Site") |> assets("/static", "public")
```

`public/css/site.css` is served at `/static/css/site.css`. In production each file is read once, compressed with gzip, with an `ETag` (a `304` when the browser already has it) and `Cache-Control`; a name holding a fingerprint (`app.4f3a9c1e.js`) is kept for a year. During development, files are read again at each request.

## Missing pages and errors

```rythme
fn missing(req) = page({title: "Not found", status: 404}, [h1("This page does not exist")])

fn failed(req, message) = page({title: "Oops", status: 500}, [h1("Something went wrong")])

app("Site")
  |> not_found(missing)
  |> on_error(failed)
```

Without `not_found`, Facile answers a plain 404 page (or JSON when the client does not accept HTML). A method not planned on a known path gives `405` with the `Allow` header. During development, a runtime error shows Rythme's whole message; in production, a short page.

## At startup

```rythme
fn fill() = if db.count(tasks) > 0 then Ok([]) else db.insert_all(tasks, [{title: "Welcome"}])

app("Tasks") |> models([tasks]) |> on_start(fill)
```

`on_start` is called **once**, before the workers start serving.
