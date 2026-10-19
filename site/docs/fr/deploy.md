# Déployer

`facile serve` est le serveur de production : il lit `PORT`, lance un worker par cœur (`WEB_CONCURRENCY` pour en choisir le nombre), lit `DATABASE_URL`, et s'arrête proprement sur `SIGTERM`.

```sh
facile serve mon-app --port 8080 --workers 4
```

## Docker

`facile new` écrit un `Dockerfile` en deux étapes : la compilation de `facile`, puis une image légère qui ne contient que le binaire et l'application.

```sh
docker build -t mon-app .
docker run -p 3000:3000 -e DATABASE_URL=sqlite:///data/app.db mon-app
```

## Heroku

L'application créée contient `heroku.yml` et `app.json` :

```sh
heroku create mon-app --stack container
heroku addons:create heroku-postgresql:essential-0
git push heroku main
```

Heroku donne `PORT` et `DATABASE_URL` ; Facile se connecte à PostgreSQL en TLS et crée les tables au démarrage. Derrière le routeur de Heroku, `req.ip()` lit `X-Forwarded-For` et `mw.https()` redirige vers HTTPS.

Avec le buildpack Rust (`emk/rust`) plutôt que le stack container, le dépôt de Facile montre le montage pour son propre site : `RustConfig` saute la construction par défaut et lance `heroku-build.sh`, qui construit la commande `facile` dans `bin/`, et le `Procfile` sert l'application :

```sh
heroku buildpacks:set emk/rust
heroku stack:set heroku-24
```

```text
web: exec bin/facile serve site
```

## Variables d'environnement

| Variable | Effet |
|---|---|
| `PORT` | le port (3000 par défaut) |
| `WEB_CONCURRENCY` | le nombre de workers (un par cœur par défaut) |
| `DATABASE_URL` | la base : `memory://`, `sqlite://...`, `postgres://...` |
| `FACILE_ENV` | `development` pour les erreurs détaillées avec `facile serve` |
| `SITE_URL` | lu par votre application avec `env("SITE_URL")` |

## Vérifier avant d'envoyer

```sh
facile check mon-app      # toute l'application, sans la lancer
facile routes mon-app     # la liste des routes
```
