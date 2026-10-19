# Getting started

Facile is the web framework of the [Rythme](https://github.com/maxlestage/Rythme) language. An application is an `app.ry` file that ends with **the value of the application**: its routes, its middlewares, its models. The server, the API and the browser's components are all written in Rythme.

## Install

Facile installs with Cargo (Rust 1.85 or newer):

```sh
cargo install --git https://github.com/maxlestage/Facile facile --locked
facile version
```

The `facile` command holds everything: the HTTP engine, the Rythme interpreter, the ORM with SQLite and PostgreSQL, and the WebAssembly module it serves to browsers.

## A first application

```sh
facile new my-app
facile dev my-app         # http://localhost:3000, reloaded at each change
```

`facile new` creates this folder:

| File | Role |
|---|---|
| `app.ry` | the routes, the pages, the models: the server |
| `client/compteur.ry` | a component that lives in the browser (an island) |
| `public/` | files served as they are (CSS, images) |
| `Dockerfile`, `heroku.yml`, `app.json` | deployment |

## The smallest server

```rythme
fn hello(req) = "Hello, web!"

app("Hello") |> on_get("/", hello)
```

A handler is a **function from the request to a response**. The last line of the file is the application to serve: `app("name")` makes an empty one, and every function (`on_get`, `middleware`, `models`...) returns a new application. Rythme values never change.

## A page, an API, an island

```rythme
use client/compteur

tasks = model("tasks", {
  title: field(:text, {required: true, max: 120}),
  done: field(:bool, {default: false}),
})

fn home(req) = page({title: "My tasks", lang: "en"}, [
  h1("My tasks"),
  island(compteur.component, {depart: 3}),
])

app("Tasks")
  |> models([tasks])
  |> resource("/api/tasks", tasks)
  |> on_get("/", home)
  |> assets("/static", "public")
  |> middleware(mw.shield())
```

- `page(...)` renders a whole HTML document, with its SEO tags;
- `resource(...)` makes a complete REST API, checked by the rules of the fields;
- `island(...)` computes the component on the server, then brings it to life in the browser;
- `mw.shield()` sets a firewall, security headers, CSRF checks and a rate limit at once.

> Rythme's error messages say **where**, **why** and **how to fix it**. `facile check` checks the whole application without running it, and `facile dev` shows the detailed error in the browser.

## Next

- [Routes and pages](/en/docs/routes): paths, parameters, groups;
- [ORM](/en/docs/orm): models, queries, relations, transactions;
- [WebAssembly islands](/en/docs/islands): Rythme components in the browser;
- [Deploying](/en/docs/deploy): Docker and Heroku.
