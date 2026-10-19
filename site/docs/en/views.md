# Views and SEO

A view is built with functions named after the HTML elements. The server renders it as finished HTML.

## Elements

```rythme
fn card(product) = article({class: "card"}, [
  img({src: product.image, alt: product.name, loading: "lazy"}),
  h2(product.name),
  p({class: ["price", if product.sale then "sale" else ()]}, ["{product.price} €"]),
  a({href: "/products/{product.id}"}, ["See"]),
])
```

- `tag()`, `tag(children)`, `tag(attributes)`, `tag(attributes, children)`; a lone text is a child (`h2("Title")`);
- children are texts, views, numbers, and lists (flattened); `()` and `None` give nothing;
- a `true` attribute is written alone (`disabled`), `false` or `()` is not written, a list of classes is joined, a record `style: {color: "red"}` becomes CSS;
- texts are **always escaped**: `raw("<b>...</b>")` writes HTML as it is (only yours);
- `h("input", {...})` gives any element; the common ones have their function (`div`, `span`, `p`, `a`, `h1`...`h6`, `ul`, `ol`, `li`, `nav`, `header`, `footer`, `main`, `section`, `article`, `aside`, `button`, `img`, `form`, `label`, `table`...).

A view is just a value: a function may return one, a list may hold some, `map` makes them.

## Whole pages

```rythme
fn home(req) = page({
  title: "Shop — home",
  description: "Handmade products.",
  lang: "en",
  url: "https://shop.com/",
  image: "https://shop.com/og.png",
  css: ["/static/site.css"],
  js: ["/static/site.js"],
  json_ld: {"@context": "https://schema.org", "@type": "Store", name: "Shop"},
  alternates: {en: "/", fr: "/fr/"},
}, [
  header([nav([a({href: "/"}, ["Home"])])]),
  main([h1("Welcome")]),
])
```

`page` writes the whole `<head>`: title, description, canonical URL, Open Graph, Twitter card, JSON-LD, `hreflang`, icon, stylesheets. It adds Facile's base stylesheet (`base_css: false` to do without) and the islands' script when the page holds some. Also useful: `theme_color`, `robots`, `manifest`, `head: [...]`, `body_class`, `status`.

## Markdown

```rythme
guide = unwrap_or(read_file("docs/guide.md"), "")

fn guide_page(req) = page({title: "Guide"}, [markdown(guide)])
```

`markdown(text)` renders headings (with anchors), emphasis, code (Rythme code is colored), links, images, lists, quotes and tables. HTML written in the text is shown as text. `toc(text)` gives the headings for a table of contents, `highlight(code)` colors Rythme code.

> A file read at the top level of the application (`guide = read_file(...)`) is read **once**, at startup: the pages that show it stay pure and use the cache.
