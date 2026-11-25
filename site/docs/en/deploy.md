# Deploying

`facile serve` is the production server: it reads `PORT`, starts one worker per core (`WEB_CONCURRENCY` to choose how many), reads `DATABASE_URL`, and stops cleanly on `SIGTERM`.

```sh
facile serve my-app --port 8080 --workers 4
```

## Docker

`facile new` writes a two-stage `Dockerfile`: building `facile`, then a light image that holds only the binary and the application.

```sh
docker build -t my-app .
docker run -p 3000:3000 -e DATABASE_URL=sqlite:///data/app.db my-app
```

## Heroku

The new application holds `heroku.yml` and `app.json`:

```sh
heroku create my-app --stack container
heroku addons:create heroku-postgresql:essential-0
git push heroku main
```

Heroku gives `PORT` and `DATABASE_URL`; Facile connects to PostgreSQL over TLS and creates the tables at startup. Behind Heroku's router, `req.ip()` reads `X-Forwarded-For` and `mw.https()` redirects to HTTPS.

With the Rust buildpack (`emk/rust`) instead of the container stack, Facile's repository shows the setup for its own site: `RustConfig` skips the default build and runs `heroku-build.sh`, which builds the `facile` command into `bin/`, and the `Procfile` serves the application:

```sh
heroku buildpacks:set emk/rust
heroku stack:set heroku-24
```

```text
web: exec bin/facile serve site
```

## Environment variables

| Variable | Effect |
|---|---|
| `PORT` | the port (3000 by default) |
| `WEB_CONCURRENCY` | the number of workers (one per core by default) |
| `DATABASE_URL` | the database: `memory://`, `sqlite://...`, `postgres://...` |
| `FACILE_ENV` | `development` for detailed errors with `facile serve` |
| `SITE_URL` | read by your application with `env("SITE_URL")` |

## Check before sending

```sh
facile check my-app       # the whole application, without running it
facile routes my-app      # the list of routes
```
