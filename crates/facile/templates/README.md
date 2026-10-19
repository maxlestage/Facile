# {{nom}}

An application written in [Rythme](https://github.com/maxlestage/Rythme) with [Facile](https://github.com/maxlestage/Facile).

```sh
facile dev          # http://localhost:3000, reloaded at each change
facile check        # checks the whole application
facile serve        # for production (PORT, WEB_CONCURRENCY, DATABASE_URL)
```

| File | Role |
|---|---|
| `app.ry` | routes, pages, models: the server |
| `client/compteur.ry` | a component that lives in the browser |
| `public/` | files served at `/static` |
| `Dockerfile`, `heroku.yml`, `app.json` | deployment |

## Deploy on Heroku

```sh
heroku create --stack container
heroku addons:create heroku-postgresql:essential-0
git push heroku main
```
