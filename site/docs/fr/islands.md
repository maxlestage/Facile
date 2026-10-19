# Îlots WebAssembly

Un **îlot** est un composant interactif écrit en Rythme. Le serveur calcule sa première vue et l'envoie en HTML (rapide à afficher, lisible par les moteurs de recherche) ; puis l'interpréteur Rythme, **compilé en WebAssembly**, le fait vivre dans le navigateur. Le reste de la page n'envoie aucun code.

## Un composant

Un composant est un fichier de `client/` qui se termine par `component = {name, init, update, view}` :

```rythme
# client/compteur.ry

fn init(props) = {n: props.depart}

fn update(etat, message) = match message
  :plus -> {..etat, n: etat.n + 1}
  :moins -> {..etat, n: etat.n - 1}
end

fn view(etat) = div({class: "compteur"}, [
  button({on_click: :moins}, ["−"]),
  strong([etat.n]),
  button({on_click: :plus}, ["+"]),
])

component = {name: "compteur", init, update, view}
```

C'est l'architecture d'Elm : un **état** (une valeur), des **messages**, `update` qui calcule le nouvel état, `view` qui le montre. Rien ne change en place.

## Le placer dans une page

```rythme
use client/compteur

fn accueil(req) = page({title: "Compteur"}, [
  h1("Un compteur"),
  island(compteur.component, {depart: 10}),
])
```

`island(composant, props)` appelle `init(props)` et `view` sur le serveur, et pose un élément `<facile-island>` avec les props en JSON. Dans le navigateur, `facile.js` charge `facile.wasm` (une fois pour toute la page, mis en cache), le fichier du composant, et prend la main quand l'îlot va devenir visible.

## Les événements

| Attribut | Message donné à `update` |
|---|---|
| `on_click: :plus` | `:plus` |
| `on_input: Saisie` | `Saisie(texte)` : la fonction reçoit la valeur du champ |
| `on_change: Choix` | `Choix(valeur)` (une case à cocher donne `"true"` ou `"false"`) |
| `on_submit: Envoyer` | `Envoyer` ; avec une fonction, elle reçoit les champs du formulaire en enregistrement |
| `on_keydown: Touche` | `Touche("Enter")` |

Les messages peuvent être des symboles, des listes, ou les valeurs d'un type à vous :

```rythme
type Message = Saisie(texte) | Ajouter | Retirer(id)
```

Après chaque message, Facile compare la nouvelle vue à l'ancienne et ne change dans la page que ce qui a changé.

## Les effets

`init` et `update` peuvent demander des effets au navigateur avec `with_effects(etat, [effets])`. Le résultat d'un effet revient à `update` comme un message.

```rythme
type Message = Chargees(resultat) | Ajouter | Ajoutee(resultat) | Tic

fn init(props) = with_effects({taches: [], erreur: None}, [fetch_json("/api/taches", Chargees)])

fn update(etat, message) = match message
  Chargees(Ok(taches)) -> {..etat, taches: taches}
  Chargees(Err(e)) -> {..etat, erreur: Some(e)}
  Ajouter -> with_effects(etat, [send_json(:post, "/api/taches", {titre: "Nouvelle"}, Ajoutee)])
  Ajoutee(Ok(t)) -> {..etat, taches: [t, ..etat.taches]}
  Ajoutee(Err(e)) -> {..etat, erreur: Some(e)}
  Tic -> with_effects(etat, [after(1000, Tic)])
end
```

| Effet | Ce qu'il fait |
|---|---|
| `fetch_json(url, F)` | `GET` ; puis `F(Ok(valeur))` ou `F(Err(message))` |
| `send_json(:post, url, corps, F)` | `POST`, `PUT`, `PATCH`, `DELETE` avec un corps JSON |
| `after(ms, message)` | le message après ce délai |
| `save(cle, valeur)` | garde une valeur dans le navigateur (`localStorage`) |
| `load(cle, F)` | `F(Some(valeur))` ou `F(None)` |
| `navigate(url)` | va sur une autre page |
| `focus("#champ")` | met le focus sur un élément |

Sur le serveur, au premier rendu, les effets sont ignorés : seul l'état compte.

## Ce qui tourne dans le navigateur

Un composant s'exécute dans le même Rythme que le serveur : les mêmes fonctions de la bibliothèque, les vues, les effets, et `evaluate(code)` (qui lance un programme Rythme à part : c'est ainsi que fonctionne l'essai de la page d'accueil). Ce qui n'a de sens que sur le serveur (`db`, les routes) n'y est pas : `facile check` le signale avant de lancer quoi que ce soit.

Un composant tient en un fichier. Le module WebAssembly pèse environ 1 Mo compressé : c'est l'interpréteur complet de Rythme, avec ses messages d'erreur en six langues, chargé une fois et gardé en cache.
