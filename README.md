<div align="center">

# Facile

**The full-stack web framework of the [Rythme](https://github.com/maxlestage/Rythme) language.**

A server faster than Vitesse, middlewares, an ORM that talks to all your databases, and a WebAssembly front end: all written in Rythme, from the server to the browser.

**English** · [Français](README.fr.md)

</div>

---

```rythme
use client/compteur

tasks = model("tasks", {
  title: field(:text, {required: true, max: 120}),
  done: field(:bool, {default: false}),
})

fn home(req) = page({title: "My tasks", lang: "en"}, [
  h1("Hello 👋"),
  island(compteur.component, {depart: 0}),
])

app("Tasks")
  |> models([tasks])
  |> resource("/api/tasks", tasks)      # GET, POST, PUT, PATCH, DELETE, validated
  |> on_get("/", home)
  |> middleware(mw.shield())            # WAF, CSP, CSRF, rate limit...
```

```sh
facile dev     # http://localhost:3000
```

That is a whole application: a page rendered by the server with its SEO, a REST API stored in a
database and checked by the rules of the fields, a component that lives in the browser thanks to
Rythme compiled to WebAssembly, and a security shield in front of every request.

## Why Facile

| | |
|---|---|
| ⚡ **Faster than Vitesse** | One thread per core, epoll, pipelining, no lock and no allocation on the way of a request. On Vitesse's own benchmark, Facile serves more requests per second (see below). |
| ♻️ **The cache of purity** | Rythme is purely functional: a response that only reads its path is kept and given again for ~70 ns of CPU, with nothing to configure. Facile watches what the handler reads (headers, database, clock, files) and only keeps what is safe. |
| 🧅 **Middlewares** | In Rythme (`fn(req, next)`), or native in Rust with `mw`: WAF, robots, security headers and CSP, CSRF, CORS, rate limits, Basic/Bearer, gzip, request ids, logs. |
| 🗄️ **An ORM that does it all** | Models, composable queries, validation, relations (`include`), aggregates, pages, transactions (savepoints), hand-written SQL, automatic migrations, a REST API in one line. In memory, SQLite or PostgreSQL. |
| 🏝️ **A WebAssembly front end** | Components are written in Rythme (`init`, `update`, `view`, effects) and run in the browser. The server sends their first view as HTML; only what changes is written again. |
| 🔎 **Pages and SEO** | `page({...})` writes the whole `<head>`: title, description, canonical URL, Open Graph, Twitter, JSON-LD, hreflang. `markdown()` renders documentation, with Rythme code colored. |
| 🛡️ **Safe by default** | Parameterized SQL, escaped texts, checked headers, request smuggling refused, size and time limits, `HttpOnly`/`SameSite` cookies. |
| 🚀 **Ready to deploy** | `facile new` writes a Dockerfile, `heroku.yml`, `app.json` and a GitHub Actions CI. |

## Getting started

```sh
cargo install --git https://github.com/maxlestage/Facile facile --locked
facile new my-app
cd my-app && facile dev
```

| Command | Effect |
|---|---|
| `facile new NAME` | a whole application, ready to deploy |
| `facile dev` | serves with live reload and detailed errors |
| `facile serve` | serves for production (`PORT`, `WEB_CONCURRENCY`, `DATABASE_URL`) |
| `facile check` | checks the whole application, without running it |
| `facile routes` | lists the routes |
| `facile reference` | every function of Facile, as Markdown |

The whole documentation is in [`site/docs/en`](site/docs/en) (and on the site, served by Facile itself): [getting started](site/docs/en/start.md), [routes](site/docs/en/routes.md), [requests and responses](site/docs/en/requests.md), [middlewares](site/docs/en/middlewares.md), [ORM](site/docs/en/orm.md), [views and SEO](site/docs/en/views.md), [WebAssembly islands](site/docs/en/islands.md), [security](site/docs/en/security.md), [deploying](site/docs/en/deploy.md), [reference](site/docs/en/reference.md).

## Performance

Vitesse's benchmark, its routes written in Rythme (`bench/app.ry`), Facile and Vitesse measured in turn (`bench/ab.sh`, median of 7 runs, 128 connections, server on 2 cores):

| Scenario | Facile | Vitesse | Difference |
|---|--:|--:|--:|
| `GET /` (text) | 267,163 | 256,532 | **+4.1%** |
| `GET /json` | 274,179 | 259,325 | **+5.7%** |
| `GET /json`, 12 browser headers | 267,658 | 249,391 | **+7.3%** |
| `GET /users/:id` (parameter + JSON) | 276,757 | 265,733 | **+4.1%** |
| `POST /echo` (JSON read and sent back) | 250,942 | 262,727 | −4.5% |
| `GET /` pipelined ×16 | 2,702,136 | 2,607,071 | **+3.6%** |

Requests per second. Facile leads on five scenarios out of six. On the JSON echo, each request really goes through the Rythme interpreter (reading the JSON into values, calling the handler, writing the JSON): Vitesse, compiled Rust, keeps the lead. With 32 connections, the lead on `GET`s grows to +12% and +21%: Facile's engine answers sooner when the machine is not saturated.

Against actix-web and axum the gap is clear: +1% to +33% over actix-web, +38% to +81% over axum, and pipelined twice actix-web and 10 times axum. Every measure, with and without the cache, and the method: [`site/docs/en/performance.md`](site/docs/en/performance.md).


## How it is made

| Folder | Content |
|---|---|
| [`crates/facile`](crates/facile) | the server and the `facile` command: the engine ([`server`](crates/facile/src/server/mod.rs)), the runtime that runs Rythme applications, the functions added to Rythme ([`host`](crates/facile/src/host)), the native middlewares, the ORM ([`orm`](crates/facile/src/orm)) |
| [`crates/facile-client`](crates/facile-client) | the browser side: the Rythme interpreter and the components' runtime, compiled to WebAssembly ([`facile.js`](crates/facile/assets/facile.js) applies the views to the DOM) |
| [`crates/facile-core`](crates/facile-core) | what both share: views, JSON, Markdown, components, effects |
| [`site`](site) | the site of Facile, written in Rythme: 3D animations in hand-written WebGL, documentation in Markdown |
| [`examples/taches`](examples/taches) | a task list: ORM, REST API, two islands |
| [`bench`](bench) | the benchmark, the routes of Vitesse's written in Rythme |

Each worker loads its own copy of the application (Rythme values belong to the thread that made
them): no request waits for another thread. The WebAssembly is built by
[`scripts/build-wasm.sh`](scripts/build-wasm.sh) and embedded in the binary.

## Working on Facile

```sh
cargo test                                   # engine, ORM (memory, SQLite), test applications
FACILE_TEST_POSTGRES=postgres://... cargo test --test orm    # on PostgreSQL too
scripts/build-wasm.sh                        # after a change on the browser side
node tests/browser/islands.cjs http://localhost:3300         # the islands in Chromium (Playwright)
facile dev site                              # the site, http://localhost:3000
```

## License

MIT.
