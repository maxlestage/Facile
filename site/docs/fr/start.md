# Démarrer

Facile est le framework web du langage [Rythme](https://github.com/maxlestage/Rythme). Une application est un fichier `app.ry` qui se termine par **la valeur de l'application** : ses routes, ses middlewares, ses modèles. Le serveur, l'API et les composants du navigateur s'écrivent tous en Rythme.

## Installer

Facile s'installe avec Cargo (Rust 1.85 ou plus récent) :

```sh
cargo install --git https://github.com/maxlestage/Facile facile --locked
facile version
```

La commande `facile` contient tout : le moteur HTTP, l'interpréteur Rythme, l'ORM avec SQLite et PostgreSQL, et le module WebAssembly qu'elle sert aux navigateurs.

## Une première application

```sh
facile new mon-app
facile dev mon-app        # http://localhost:3000, rechargé à chaque modification
```

`facile new` crée ce dossier :

| Fichier | Rôle |
|---|---|
| `app.ry` | les routes, les pages, les modèles : le serveur |
| `client/compteur.ry` | un composant qui vit dans le navigateur (un îlot) |
| `public/` | les fichiers servis tels quels (CSS, images) |
| `Dockerfile`, `heroku.yml`, `app.json` | le déploiement |

## Le plus petit serveur

```rythme
fn bonjour(req) = "Bonjour, le web !"

app("Bonjour") |> on_get("/", bonjour)
```

Un handler est une **fonction de la requête vers une réponse**. La dernière ligne du fichier est l'application à servir : `app("nom")` en crée une vide, et chaque fonction (`on_get`, `middleware`, `models`...) renvoie une nouvelle application. Les valeurs de Rythme ne changent jamais.

## Une page, une API, un îlot

```rythme
use client/compteur

taches = model("taches", {
  titre: field(:text, {required: true, max: 120}),
  fait: field(:bool, {default: false}),
})

fn accueil(req) = page({title: "Mes tâches", lang: "fr"}, [
  h1("Mes tâches"),
  island(compteur.component, {depart: 3}),
])

app("Tâches")
  |> models([taches])
  |> resource("/api/taches", taches)
  |> on_get("/", accueil)
  |> assets("/static", "public")
  |> middleware(mw.shield())
```

- `page(...)` rend un document HTML complet, avec ses balises SEO ;
- `resource(...)` crée une API REST complète, validée par les règles des champs ;
- `island(...)` calcule le composant sur le serveur puis l'anime dans le navigateur ;
- `mw.shield()` pose d'un coup pare-feu, en-têtes de sécurité, CSRF et limite de débit.

> Les messages d'erreur de Rythme disent **où**, **pourquoi** et **comment corriger**. `facile check` vérifie toute l'application sans la lancer, et `facile dev` affiche l'erreur détaillée dans le navigateur.

## Et ensuite

- [Routes et pages](/docs/routes) : chemins, paramètres, groupes ;
- [ORM](/docs/orm) : modèles, requêtes, relations, transactions ;
- [Îlots WebAssembly](/docs/islands) : des composants en Rythme dans le navigateur ;
- [Déployer](/docs/deploy) : Docker et Heroku.
