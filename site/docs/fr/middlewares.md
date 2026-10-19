# Middlewares

Un middleware entoure les handlers : il voit la requête avant eux et la réponse après eux. Facile en a de deux sortes, qui se combinent.

## En Rythme : `fn(req, next)`

```rythme
## Ajoute un en-tête à toutes les réponses.
fn signature(req, next)
  res = next(req)
  with_header(res, "x-servi-par", "Facile")
end

## Refuse ceux qui ne sont pas connectés.
fn connecte(req, next) = match req.cookie("session")
  Some(_) -> next(req)
  None -> redirect("/connexion")
end

## Donne une information aux handlers.
fn utilisateur(req, next) = next({..req, utilisateur: lire_session(req)})

app("Site")
  |> middleware(signature)
  |> middleware(connecte)
```

`next(req)` exécute la suite de la chaîne (les middlewares suivants, puis le handler) et renvoie sa réponse sous forme d'enregistrement `{status, headers, body}`, que le middleware peut lire et changer. Un middleware peut aussi répondre sans appeler `next`.

Les middlewares d'une application s'appliquent à **toutes ses routes**, dans l'ordre où ils ont été ajoutés. Pour ne protéger qu'une partie du site, mettez-la dans une application montée avec `mount` (voir [Routes](/docs/routes)).

## En Rust : `mw`

Les middlewares de `mw` sont écrits en Rust et tournent dans le moteur, avant le cache et avant Rythme. Ils ne coûtent presque rien.

| Middleware | Effet |
|---|---|
| `mw.shield()` | tout ce qui suit d'un coup : WAF, robots, en-têtes de sécurité, CSRF, 300 requêtes/min, gzip, identifiants |
| `mw.security_headers()` | CSP (WebAssembly permis), HSTS en production, `X-Frame-Options`, `nosniff`, `Referrer-Policy`, `Permissions-Policy` |
| `mw.waf()` | refuse injections SQL, XSS, remontée de dossiers, Log4Shell, injections de commandes (403) |
| `mw.block_bots()` | refuse les robots d'attaque et les chemins qu'ils sondent (`/.env`, `/wp-admin`...) |
| `mw.csrf()` | refuse les POST, PUT, PATCH, DELETE envoyés par un autre site |
| `mw.cors(["https://a.fr"])` | CORS et requêtes préalables (`"*"` pour tous) |
| `mw.rate_limit(120)` | au plus 120 requêtes par minute et par adresse, puis 429 et `Retry-After` |
| `mw.basic_auth(user, mdp)` | authentification HTTP Basic, comparée en temps constant |
| `mw.bearer(jeton)` | exige `Authorization: Bearer <jeton>` |
| `mw.body_limit(1000000)` | refuse les corps trop gros (413) |
| `mw.compress()` | gzip des textes, JSON, SVG et WebAssembly (calculé une fois pour les réponses en cache) |
| `mw.request_id()` | un `X-Request-Id` sur chaque réponse |
| `mw.timing()` | un en-tête `Server-Timing` |
| `mw.logger()` | une ligne par requête : méthode, chemin, statut, durée |
| `mw.https()` | redirige `http://` vers `https://` derrière un proxy |
| `mw.headers({...})`, `mw.cache_control("...")` | des en-têtes sur chaque réponse |

```rythme
app("API")
  |> middleware(mw.shield())
  |> middleware(mw.cors(["https://mon-front.fr"]))
  |> middleware(mw.logger())
```

## L'ordre

Pour chaque requête, Facile fait dans cet ordre :

1. les middlewares de `mw` qui peuvent refuser (WAF, robots, limite, authentification, CSRF, CORS) ;
2. le cache des réponses pures ;
3. les fichiers statiques ;
4. le routeur, puis **vos middlewares Rythme**, puis le handler ;
5. les en-têtes de `mw` (sécurité, CORS, identifiants), la compression, le journal.
