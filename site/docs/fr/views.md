# Vues et SEO

Une vue se construit avec des fonctions qui portent le nom des éléments HTML. Le serveur la rend en HTML fini.

## Les éléments

```rythme
fn carte(produit) = article({class: "carte"}, [
  img({src: produit.image, alt: produit.nom, loading: "lazy"}),
  h2(produit.nom),
  p({class: ["prix", if produit.promo then "promo" else ()]}, ["{produit.prix} €"]),
  a({href: "/produits/{produit.id}"}, ["Voir"]),
])
```

- `tag()`, `tag(enfants)`, `tag(attributs)`, `tag(attributs, enfants)` ; un texte seul est un enfant (`h2("Titre")`) ;
- les enfants sont des textes, des vues, des nombres, et des listes (aplaties) ; `()` et `None` ne donnent rien ;
- un attribut `true` s'écrit seul (`disabled`), `false` ou `()` ne s'écrit pas, une liste de classes est jointe, un enregistrement `style: {color: "red"}` devient du CSS ;
- les textes sont **toujours échappés** : `raw("<b>...</b>")` écrit du HTML tel quel (seulement le vôtre) ;
- `h("input", {...})` donne n'importe quel élément ; les éléments courants ont leur fonction (`div`, `span`, `p`, `a`, `h1`...`h6`, `ul`, `ol`, `li`, `nav`, `header`, `footer`, `main`, `section`, `article`, `aside`, `button`, `img`, `form`, `label`, `table`...).

Une vue n'est qu'une valeur : une fonction peut en renvoyer, une liste peut en contenir, `map` en produit.

## Des pages complètes

```rythme
fn accueil(req) = page({
  title: "Boutique — accueil",
  description: "Des produits faits main.",
  lang: "fr",
  url: "https://boutique.fr/",
  image: "https://boutique.fr/og.png",
  css: ["/static/site.css"],
  js: ["/static/site.js"],
  json_ld: {"@context": "https://schema.org", "@type": "Store", name: "Boutique"},
  alternates: {fr: "/", en: "/en/"},
}, [
  header([nav([a({href: "/"}, ["Accueil"])])]),
  main([h1("Bienvenue")]),
])
```

`page` écrit tout le `<head>` : titre, description, URL canonique, Open Graph, carte Twitter, JSON-LD, `hreflang`, icône, feuilles de style. Elle ajoute la feuille de style de base de Facile (`base_css: false` pour s'en passer) et le script des îlots quand la page en contient. Réglages utiles aussi : `theme_color`, `robots`, `manifest`, `head: [...]`, `body_class`, `status`.

## Markdown

```rythme
guide = unwrap_or(read_file("docs/guide.md"), "")

fn page_guide(req) = page({title: "Guide"}, [markdown(guide)])
```

`markdown(texte)` rend titres (avec ancres), emphase, code (le code Rythme est coloré), liens, images, listes, citations et tableaux. Le HTML écrit dans le texte est affiché comme du texte. `toc(texte)` donne la liste des titres pour une table des matières, `highlight(code)` colore du code Rythme.

> Un fichier lu au premier niveau de l'application (`guide = read_file(...)`) est lu **une fois**, au démarrage : les pages qui l'affichent restent pures et profitent du cache.
