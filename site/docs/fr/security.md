# Sécurité

Facile est sûr par défaut là où il le peut, et donne le reste en une ligne : `middleware(app, mw.shield())`.

## Ce qui est toujours vrai

- **Pas d'injection SQL** : les valeurs passent toujours en paramètres ; les noms de tables et de champs sont vérifiés à la création du modèle.
- **Pas d'injection HTML** : tout texte d'une vue est échappé ; `raw` est explicite, et `markdown` affiche le HTML comme du texte et refuse les liens `javascript:`.
- **Pas de découpage de réponse** : un en-tête qui contient un retour à la ligne est refusé.
- **Pas de contrebande de requêtes** : une requête avec `Content-Length` et `Transfer-Encoding: chunked` est refusée ; les têtes de plus de 16 Ko (431) et les corps trop gros (413) aussi.
- **Pas de remontée de dossiers** : `assets` et `send_file` refusent `..`.
- **Des clients lents** n'immobilisent rien : une requête doit arriver entière en 30 secondes, une connexion inactive se ferme après 75.
- **Des cookies sûrs** : `HttpOnly`, `SameSite=Lax`, et `Secure` en production.
- **Des erreurs discrètes** en production : le détail reste dans les journaux.

## `mw.shield()`

| Protection | Détail |
|---|---|
| Pare-feu applicatif | injections SQL, XSS, remontée de dossiers, Log4Shell, injections de commandes dans le chemin et la requête (décodés deux fois) |
| Robots | chemins sondés par les scanners (`/.env`, `/.git`, `/wp-admin`, `*.php`...) et outils d'attaque connus |
| En-têtes | Content-Security-Policy, `X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`, `Permissions-Policy`, `Cross-Origin-Opener-Policy`, HSTS en production |
| CSRF | les requêtes qui écrivent, envoyées par un autre site (`Sec-Fetch-Site`, `Origin`), sont refusées |
| Limite de débit | 300 requêtes par minute et par adresse, partagée entre les workers |
| Compression et identifiants | gzip, `X-Request-Id` |

La Content-Security-Policy par défaut n'autorise que les scripts du site et `wasm-unsafe-eval` (pour les îlots) : pas de script en ligne, pas de script d'un autre domaine. Elle se règle avec `mw.security_headers({csp: "..."})`.

## Authentification

```rythme
admin = app("Admin")
  |> on_get("/", tableau)
  |> middleware(mw.basic_auth("admin", unwrap_or(env("ADMIN_MDP"), "")))

api = app("API")
  |> resource("/commandes", commandes)
  |> middleware(mw.bearer(unwrap_or(env("API_JETON"), "")))

app("Site") |> mount("/admin", admin) |> mount("/api", api)
```

Les secrets sont comparés en temps constant. Pour des sessions, un middleware Rythme lit un cookie et ajoute l'utilisateur à la requête (voir [Middlewares](/docs/middlewares)).
