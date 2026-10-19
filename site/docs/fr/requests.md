# Requêtes et réponses

## La requête

Un handler reçoit un enregistrement :

| Champ | Valeur |
|---|---|
| `req.method` | `:get`, `:post`, `:put`, `:patch`, `:delete`... |
| `req.path` | le chemin, sans la requête (`/produits/42`) |
| `req.params` | les paramètres du chemin : `req.params.id` |
| `req.query` | les paramètres de l'URL : `?page=2` donne `req.query.page`, un texte |

Et des fonctions pour lire le reste :

```rythme
fn info(req) = {
  agent: req.header("user-agent"),        # Some(texte) ou None
  theme: req.cookie("theme"),             # Some(texte) ou None
  ip: req.ip(),                           # la première de X-Forwarded-For derrière un proxy
}

fn creer(req) = match req.json()          # Ok(valeur) ou Err(message)
  Ok(donnees) -> json(donnees, 201)
  Err(e) -> problem(400, e)
end

fn formulaire(req) = do
  champs = req.form()                     # un enregistrement : champs.email
  "Merci, {unwrap_or(get(champs, "nom"), "inconnu")} !"
end
```

`req.body()` donne le corps en texte. Le JSON est lu directement en valeurs Rythme : les objets deviennent des enregistrements, `null` devient `None`.

## Ce que renvoie un handler

| Valeur renvoyée | Réponse |
|---|---|
| un texte | `200`, `text/plain` |
| une vue (`div(...)`) | `200`, HTML |
| `page(...)` | un document HTML complet |
| un enregistrement, une liste, un nombre | `200`, JSON |
| `Ok(x)`, `Some(x)` | la réponse de `x` |
| `None` | `404` |
| `Err(:not_found)`, `Err(:forbidden)`... | le statut correspondant, en JSON |
| `Err({status: 422, ...})` | ce statut, et l'enregistrement en JSON |
| `Err("message")` | `400` et `{"error": "message"}` |
| `()` | `204`, sans corps |

Les erreurs sont des valeurs : l'opérateur `?` de Rythme fait remonter un `Err` jusqu'à la réponse.

```rythme
fn terminer(req) = do
  tache = db.update(taches, req.params.id, {fait: true})?   # Err(:not_found) devient 404
  json(tache)
end
```

## Construire une réponse

```rythme
html("<p>Bonjour</p>")                       # du HTML, ou une vue
json({ok: true}, 201)                        # du JSON, avec un statut
"Introuvable" |> status(404)                 # n'importe quelle réponse, autre statut
redirect("/connexion")                       # 303 See Other (301, 302, 307, 308 possibles)
send_file("rapports/mars.pdf")               # un fichier, avec son type
no_content()                                 # 204
problem(422, "le titre manque")              # {"error": "le titre manque"} et 422

json(produits)
  |> with_header("cache-control", "public, max-age=60")
  |> with_cookie("vu", "oui", {max_age: 86400, same_site: "lax"})
```

`with_cookie` pose `HttpOnly` et `SameSite=Lax` par défaut, et `Secure` en production. Un en-tête ne peut pas contenir de retour à la ligne : Facile le refuse.

## Le cache de la pureté

Rythme est **purement fonctionnel** : une fonction donne toujours le même résultat pour les mêmes arguments. Facile s'en sert : quand une réponse à un `GET` n'a lu que son chemin et sa requête — ni en-tête, ni cookie, ni base de données, ni horloge, ni fichier — elle est gardée et redonnée telle quelle aux requêtes suivantes, en quelques dizaines de nanosecondes.

Rien à déclarer : Facile observe ce que le handler lit. Dès qu'il appelle `req.header`, `db.all`, `now()`, `random()`, `read_file`..., sa réponse n'est pas gardée. Pour s'en passer complètement :

```rythme
app("Site") |> settings({cache: false})
```
