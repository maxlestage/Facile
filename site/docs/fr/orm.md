# ORM et bases de données

L'ORM de Facile décrit les données avec des **modèles**, les cherche avec des **requêtes** qui sont des valeurs, et les lit ou les écrit avec `db`. Le même code tourne sur trois bases, choisies par une URL.

| URL | Base |
|---|---|
| `memory://` (par défaut) | en mémoire, partagée par les workers : développement et tests |
| `sqlite://app.db` | SQLite, compilé dans `facile` : rien à installer |
| `postgres://user:mdp@hote/base` | PostgreSQL, en TLS (Heroku Postgres, Neon, Supabase...) |

L'URL vient de la variable `DATABASE_URL`, ou de `database(app, "sqlite://app.db")`.

## Modèles

```rythme
projets = model("projets", {
  nom: field(:text, {required: true, unique: true, max: 80}),
  taches: has_many("taches", :projet),
})

taches = model("taches", {
  titre: field(:text, {required: true, min: 2, max: 120}),
  fait: field(:bool, {default: false}),
  priorite: field(:int, {default: 1, min: 1, max: 5}),
  echeance: :date,
  etiquettes: :json,
  courriel: field(:text, {email: true}),
  statut: field(:text, {one_of: ["ouverte", "fermee"], default: "ouverte"}),
  projet: belongs_to("projets"),
})

app("Projets") |> models([projets, taches])
```

Chaque table a un `id` (entier, généré par la base) et, sauf `model(..., {timestamps: false})`, `created_at` et `updated_at`.

| Sorte | Valeur Rythme |
|---|---|
| `:text`, `:int`, `:float`, `:bool` | Text, Int, Float, Bool |
| `:date` | un texte `2026-10-19` |
| `:datetime` | un texte `2026-10-19T08:30:00Z` |
| `:json` | n'importe quelle valeur (liste, enregistrement...) |

Les règles de `field` : `required`, `unique`, `default`, `min` et `max` (valeur pour un nombre, longueur pour un texte), `email`, `one_of`, `index`. Un champ vide (`None`) vaut `None` à la lecture.

**Les tables se créent seules** : au démarrage, `facile` crée les tables des modèles de `models(...)` et ajoute les colonnes qui leur manquent. Les noms de tables et de champs sont vérifiés (minuscules, chiffres, `_`) : ils ne peuvent pas servir d'injection SQL, et toutes les valeurs passent en paramètres.

## Requêtes

Un modèle est aussi la requête de toutes ses lignes. Chaque fonction en renvoie une nouvelle :

```rythme
urgentes = taches
  |> where(:fait, false)                         # égal
  |> where(:priorite, ">=", 4)                   # =, !=, <, <=, >, >=
  |> where(:titre, :contains, "lait")            # :contains, :starts_with, :ends_with (sans casse)
  |> where(:statut, :in, ["ouverte"])            # :in, :not_in
  |> where(:echeance, None)                      # IS NULL
  |> order_by(:echeance)                         # :asc par défaut, ou :desc
  |> order_by(:priorite, :desc)
  |> limit(20)
  |> offset(40)

au_choix = taches |> where_any([[:priorite, 5], [:fait, true]])       # l'une ou l'autre
trouvees = taches |> search([:titre, :courriel], "ada")                # un texte dans l'un des champs
legeres = taches |> pick([:id, :titre])                                # seulement ces champs
```

## Lire

```rythme
db.all(q)                   # la liste des enregistrements
db.first(q)                 # Some(enregistrement) ou None
db.find(taches, 42)         # Some(enregistrement) ou None
db.count(q)                 # un Int
db.exists(q)                # un Bool
db.sum(q, :priorite)        # Some(n), ou None sans lignes ; aussi db.avg, db.min, db.max
db.count_by(q, :statut)     # un Dict : valeur -> nombre de lignes
db.paginate(q, 2, 20)       # {items, page, per_page, total, pages}
```

## Relations

```rythme
avec_projet = taches |> include(:projet) |> db.all       # chaque tâche a son champ `projet` : Some(...) ou None
avec_taches = projets |> include(:taches) |> db.all      # chaque projet a la liste de ses tâches
```

`belongs_to("projets")` enregistre `projet_id` ; `has_many("taches", :projet)` lit les tâches dont `projet_id` est l'id du projet. `include` fait **une requête de plus**, pas une par ligne.

## Écrire

```rythme
match db.insert(taches, {titre: "Lait", projet: maison})
  Ok(tache) -> json(tache, 201)                          # avec son id et ses dates
  Err(e) -> json(e, e.status)                            # {status: 422, error, fields: {titre: "..."}}
end

db.insert_all(taches, [{titre: "A"}, {titre: "B"}])      # toutes ou aucune
db.update(taches, 42, {fait: true})                      # Ok(enregistrement), Err(:not_found), Err({status: 422...})
db.save(taches, tache)                                   # insère sans id, met à jour avec un id
db.update_where(taches |> where(:fait, true), {statut: "fermee"})   # Ok(nombre)
db.delete(taches, 42)                                    # true si elle existait
db.delete_where(taches |> where(:statut, "fermee"))      # le nombre de lignes
db.validate(taches, {titre: "x"})                        # vérifie sans écrire
```

Chaque écriture est vérifiée par les règles des champs ; un champ inconnu est refusé ; une valeur `unique` déjà prise donne `422`.

## Transactions

```rythme
fn virement(req) = db.transaction do
  db.update(comptes, 1, {solde: 50})?
  db.update(comptes, 2, {solde: 150})?
  Ok("fait")
end
```

Tout ce que la fonction écrit est gardé, sauf si elle renvoie `Err(...)` ou s'arrête sur une erreur : alors **rien** n'est gardé. Une transaction dans une autre devient un point de sauvegarde.

## SQL écrit à la main

```rythme
db.sql("SELECT statut, COUNT(*) AS n FROM taches WHERE priorite >= ? GROUP BY statut", [3])
# Ok([{statut: "ouverte", n: 4}, ...]) ou Err(message)
```

Les `?` sont des paramètres (Facile les écrit `$1`, `$2`... pour PostgreSQL).

## Une API REST en une ligne

```rythme
app("API") |> resource("/api/taches", taches)
```

| Requête | Effet |
|---|---|
| `GET /api/taches` | la liste ; `?fait=false` filtre, `?q=lait` cherche, `?order=-id` trie, `?limit=10&offset=20`, ou `?page=2&per_page=10` |
| `POST /api/taches` | crée (JSON ou formulaire) : `201`, ou `422` avec les erreurs |
| `GET /api/taches/:id` | une ligne, ou `404` |
| `PUT /api/taches/:id` | remplace tous les champs |
| `PATCH /api/taches/:id` | change les champs donnés |
| `DELETE /api/taches/:id` | `204`, ou `404` |

`resource(app, chemin, modele, [:list, :show])` ne garde que certaines actions. Les middlewares de l'application s'appliquent aussi à ces routes.
