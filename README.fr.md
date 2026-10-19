<div align="center">

# Facile

**Le framework web full-stack du langage [Rythme](https://github.com/maxlestage/Rythme).**

Un serveur plus rapide que Vitesse, des middlewares, un ORM qui parle à toutes vos bases, et un front-end en WebAssembly : le tout écrit en Rythme, du serveur au navigateur.

[English](README.md) · **Français**

</div>

---

```rythme
use client/compteur

taches = model("taches", {
  titre: field(:text, {required: true, max: 120}),
  fait: field(:bool, {default: false}),
})

fn accueil(req) = page({title: "Mes tâches", lang: "fr"}, [
  h1("Bonjour 👋"),
  island(compteur.component, {depart: 0}),
])

app("Tâches")
  |> models([taches])
  |> resource("/api/taches", taches)     # GET, POST, PUT, PATCH, DELETE, validés
  |> on_get("/", accueil)
  |> middleware(mw.shield())             # WAF, CSP, CSRF, limite de débit...
```

```sh
facile dev     # http://localhost:3000
```

C'est une application complète : une page rendue par le serveur avec son SEO, une API REST
enregistrée dans une base de données et validée par les règles des champs, un composant qui vit
dans le navigateur grâce à Rythme compilé en WebAssembly, et un bouclier de sécurité devant
chaque requête.

## Pourquoi Facile

| | |
|---|---|
| ⚡ **Plus rapide que Vitesse** | Un thread par cœur, epoll, pipelining, aucun verrou ni allocation sur le chemin d'une requête. Sur le banc d'essai de Vitesse, Facile sert plus de requêtes par seconde (voir plus bas). |
| ♻️ **Le cache de la pureté** | Rythme est purement fonctionnel : une réponse qui ne lit que son chemin est gardée et redonnée en ~70 ns de CPU, sans rien configurer. Facile observe ce que lit le handler (en-têtes, base, horloge, fichiers) et ne garde que ce qui est sûr. |
| 🧅 **Middlewares** | En Rythme (`fn(req, next)`), ou natifs en Rust avec `mw` : WAF, robots, en-têtes de sécurité et CSP, CSRF, CORS, limite de débit, Basic/Bearer, gzip, identifiants, journaux. |
| 🗄️ **Un ORM qui gère tout** | Modèles, requêtes composables, validation, relations (`include`), agrégats, pagination, transactions (points de sauvegarde), SQL à la main, migrations automatiques, API REST en une ligne. En mémoire, SQLite ou PostgreSQL. |
| 🏝️ **Front en WebAssembly** | Les composants s'écrivent en Rythme (`init`, `update`, `view`, effets) et tournent dans le navigateur. Le serveur envoie leur première vue en HTML ; seule la partie qui change est réécrite. |
| 🔎 **Pages et SEO** | `page({...})` écrit tout le `<head>` : titre, description, canonique, Open Graph, Twitter, JSON-LD, hreflang. `markdown()` rend la documentation, avec le code Rythme coloré. |
| 🛡️ **Sûr par défaut** | Requêtes SQL paramétrées, textes échappés, en-têtes vérifiés, contrebande de requêtes refusée, limites de taille et de temps, cookies `HttpOnly`/`SameSite`. |
| 🚀 **Prêt à déployer** | `facile new` crée un Dockerfile, `heroku.yml`, `app.json` et une CI GitHub Actions. |

## Démarrer

```sh
cargo install --git https://github.com/maxlestage/Facile facile --locked
facile new mon-app
cd mon-app && facile dev
```

| Commande | Effet |
|---|---|
| `facile new NOM` | une application complète, prête à déployer |
| `facile dev` | sert avec rechargement et erreurs détaillées |
| `facile serve` | sert pour la production (`PORT`, `WEB_CONCURRENCY`, `DATABASE_URL`) |
| `facile check` | vérifie toute l'application, sans la lancer |
| `facile routes` | liste les routes |
| `facile reference` | toutes les fonctions de Facile, en Markdown |

La documentation complète est dans [`site/docs/fr`](site/docs/fr) (et sur le site, servi par Facile lui-même) : [démarrer](site/docs/fr/start.md), [routes](site/docs/fr/routes.md), [requêtes et réponses](site/docs/fr/requests.md), [middlewares](site/docs/fr/middlewares.md), [ORM](site/docs/fr/orm.md), [vues et SEO](site/docs/fr/views.md), [îlots WebAssembly](site/docs/fr/islands.md), [sécurité](site/docs/fr/security.md), [déploiement](site/docs/fr/deploy.md), [référence](site/docs/fr/reference.md).

## Performances

Le banc d'essai de Vitesse, ses routes réécrites en Rythme (`bench/app.ry`), Facile et Vitesse mesurés à tour de rôle (`bench/ab.sh`, médiane de 7 mesures, 128 connexions, serveur sur 2 cœurs) :

| Scénario | Facile | Vitesse | Écart |
|---|--:|--:|--:|
| `GET /` (texte) | 267 163 | 256 532 | **+4,1 %** |
| `GET /json` | 274 179 | 259 325 | **+5,7 %** |
| `GET /json`, 12 en-têtes de navigateur | 267 658 | 249 391 | **+7,3 %** |
| `GET /users/:id` (paramètre + JSON) | 276 757 | 265 733 | **+4,1 %** |
| `POST /echo` (JSON lu et renvoyé) | 250 942 | 262 727 | −4,5 % |
| `GET /` en pipeline ×16 | 2 702 136 | 2 607 071 | **+3,6 %** |

Requêtes par seconde. Facile passe devant sur cinq scénarios sur six. Sur l'écho JSON, chaque requête traverse vraiment l'interpréteur Rythme (lire le JSON en valeurs, appeler le handler, écrire le JSON) : Vitesse, en Rust compilé, garde l'avantage. À 32 connexions, l'écart sur les `GET` monte à +12 et +21 % : le moteur de Facile répond plus vite quand la machine n'est pas saturée.

Contre actix-web et axum, l'écart est net : de +1 à +33 % face à actix-web, de +38 à +81 % face à axum, et en pipeline 2 fois plus qu'actix-web, 10 fois plus qu'axum. Toutes les mesures, avec et sans le cache, et la méthode : [`site/docs/fr/performance.md`](site/docs/fr/performance.md).


## Comment c'est fait

| Dossier | Contenu |
|---|---|
| [`crates/facile`](crates/facile) | le serveur et la commande `facile` : le moteur ([`server`](crates/facile/src/server/mod.rs)), le runtime qui exécute les applications Rythme, les fonctions ajoutées à Rythme ([`host`](crates/facile/src/host)), les middlewares natifs, l'ORM ([`orm`](crates/facile/src/orm)) |
| [`crates/facile-client`](crates/facile-client) | le côté navigateur : l'interpréteur Rythme et le runtime des composants, compilés en WebAssembly ([`facile.js`](crates/facile/assets/facile.js) applique les vues au DOM) |
| [`crates/facile-core`](crates/facile-core) | ce que les deux partagent : vues, JSON, Markdown, composants, effets |
| [`site`](site) | le site de Facile, écrit en Rythme : animations 3D en WebGL écrites à la main, documentation en Markdown |
| [`examples/taches`](examples/taches) | une liste de tâches : ORM, API REST, deux îlots |
| [`bench`](bench) | le banc d'essai, les routes de celui de Vitesse écrites en Rythme |

Chaque worker charge sa propre copie de l'application (les valeurs de Rythme appartiennent au
thread qui les a créées) : aucune requête n'attend un autre thread. Le WebAssembly est construit
par [`scripts/build-wasm.sh`](scripts/build-wasm.sh) et embarqué dans le binaire.

## Développer Facile

```sh
cargo test                                   # moteur, ORM (mémoire, SQLite), applications de test
FACILE_TEST_POSTGRES=postgres://... cargo test --test orm    # aussi sur PostgreSQL
scripts/build-wasm.sh                        # après un changement du côté navigateur
node tests/browser/islands.cjs http://localhost:3300         # les îlots dans Chromium (Playwright)
facile dev site                              # le site, http://localhost:3000
```

Facile dépend de la branche `claude/rythme-framework-3d-site-odfjcn` de Rythme, qui apprend au
vérificateur de Rythme les champs des enregistrements donnés par un programme hôte (`db`, `mw`) ;
elle deviendra `master` une fois fusionnée.

## Licence

MIT.
