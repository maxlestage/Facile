# Routes et pages

Une route associe une **méthode HTTP** et un **chemin** à un handler. Les routes s'ajoutent à l'application, dans n'importe quel ordre.

## Déclarer des routes

```rythme
app("Boutique")
  |> on_get("/", accueil)
  |> on_get("/produits/:id", produit)
  |> on_post("/panier", ajouter)
  |> on_put("/panier/:id", remplacer)
  |> on_patch("/panier/:id", modifier)
  |> on_delete("/panier/:id", retirer)
  |> route(:options, "/panier", options)
  |> route(:any, "/sonde", sonde)
```

`on_get(app, chemin, handler)` est `route(app, :get, chemin, handler)`. `:any` accepte toutes les méthodes. Une requête `HEAD` utilise la route `GET`, sans corps dans la réponse.

## Paramètres de chemin

```rythme
fn produit(req) = "Produit numéro {req.params.id}"

fn fichier(req) = "Chemin demandé : {req.params.chemin}"

app("Exemple")
  |> on_get("/produits/:id", produit)
  |> on_get("/fichiers/*chemin", fichier)
```

- `:nom` capture un segment ; sa valeur est un texte (`parse_int(req.params.id)` pour un nombre) ;
- `*nom` capture la fin du chemin, barres obliques comprises ;
- `/a/b/` et `/a/b` sont la même route.

Les chemins sans paramètre sont trouvés dans une table de hachage ; les autres sont comparés segment par segment, dans l'ordre de déclaration.

## Groupes : `mount`

Une application peut en contenir une autre sous un préfixe, avec ses propres middlewares :

```rythme
admin = app("Admin")
  |> on_get("/", tableau_de_bord)
  |> on_get("/commandes", commandes)
  |> middleware(mw.basic_auth("admin", unwrap_or(env("ADMIN_MDP"), "secret")))

app("Boutique")
  |> on_get("/", accueil)
  |> mount("/admin", admin)
```

Les routes de `admin` deviennent `/admin` et `/admin/commandes`, et seuls elles passent par son authentification.

## Fichiers statiques

```rythme
app("Site") |> assets("/static", "public")
```

`public/css/site.css` est servi à `/static/css/site.css`. En production, chaque fichier est lu une fois, compressé en gzip, avec un `ETag` (réponse `304` quand le navigateur l'a déjà) et `Cache-Control` ; un nom qui contient une empreinte (`app.4f3a9c1e.js`) est gardé un an. En développement, les fichiers sont relus à chaque requête.

## Pages introuvables et erreurs

```rythme
fn introuvable(req) = page({title: "Introuvable", status: 404}, [h1("Cette page n'existe pas")])

fn en_cas_d_erreur(req, message) = page({title: "Oups", status: 500}, [h1("Une erreur est survenue")])

app("Site")
  |> not_found(introuvable)
  |> on_error(en_cas_d_erreur)
```

Sans `not_found`, Facile répond une page 404 sobre (ou du JSON si le client n'accepte pas le HTML). Une méthode non prévue sur un chemin connu donne `405` avec l'en-tête `Allow`. En développement, une erreur d'exécution affiche le message complet de Rythme ; en production, une page courte.

## Au démarrage

```rythme
fn remplir() = if db.count(taches) > 0 then Ok([]) else db.insert_all(taches, [{titre: "Bienvenue"}])

app("Tâches") |> models([taches]) |> on_start(remplir)
```

`on_start` est appelé **une fois**, avant que les workers ne commencent à servir.
