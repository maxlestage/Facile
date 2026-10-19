# Référence

Toutes les fonctions que Facile ajoute à Rythme, avec ce qu'elles font. Cette page est écrite par `facile reference --lang fr`, à partir du code.

## Application

| Fonction | Effet |
|---|---|
| `app(name)` | Une nouvelle application web, encore sans routes. Le fichier se termine par l'application à servir. |
| `route(app, method, path, handler)` | L'application avec une route de plus : `route(app, :get, "/taches/:id", detail)`. Le handler reçoit la requête. |
| `on_get(app, path, handler)` | `route(app, :get, chemin, handler)`. |
| `on_post(app, path, handler)` | `route(app, :post, chemin, handler)`. |
| `on_put(app, path, handler)` | `route(app, :put, chemin, handler)`. |
| `on_patch(app, path, handler)` | `route(app, :patch, chemin, handler)`. |
| `on_delete(app, path, handler)` | `route(app, :delete, chemin, handler)`. |
| `middleware(app, middleware)` | L'application avec un middleware autour de chaque route : une fonction `fn(req, next)`, ou l'un de `mw`. |
| `mount(app, prefix, other_app)` | Les routes d'une autre application sous un préfixe, avec ses middlewares : `mount(app, "/admin", admin)`. |
| `assets(app, prefix, folder)` | Sert les fichiers d'un dossier : `assets(app, "/static", "public")`, avec gzip, ETag et en-têtes de cache. |
| `models(app, models)` | Les modèles dont le serveur crée ou complète les tables au démarrage. |
| `resource(app, path, model, actions)` | Une API REST pour un modèle : GET et POST sur le chemin, GET, PUT, PATCH et DELETE sur `chemin/:id`. En option : les actions à garder (`[:list, :show]`). |
| `not_found(app, handler)` | Le handler des chemins sans route (une page 404). |
| `on_error(app, handler)` | Le handler appelé avec la requête et le message d'erreur quand un handler échoue. |
| `on_start(app, function)` | Une fonction appelée une fois au démarrage du serveur (pour remplir une base, par exemple). |
| `settings(app, settings)` | Des réglages : `{cache: false}`, `{body_limit: 1000000}`, `{workers: 4}`, `{lang: "fr"}`. |
| `database(app, url)` | La base de données : `memory://`, `sqlite://app.db`, `postgres://...`. `DATABASE_URL` l'emporte s'il est défini. |

## Requête

| Fonction | Effet |
|---|---|
| `req.header(name)` | Un en-tête de la requête : `Some(valeur)` ou `None`. Les noms n'ont pas de casse. |
| `req.cookie(name)` | Un cookie de la requête : `Some(valeur)` ou `None`. |
| `req.body()` | Le corps de la requête, en texte. |
| `req.json()` | Le corps lu comme du JSON : `Ok(valeur)` ou `Err(message)`. |
| `req.form()` | Les champs d'un formulaire envoyé par le navigateur, en enregistrement. |
| `req.ip()` | L'adresse du client (la première de X-Forwarded-For derrière un proxy). |

## Réponses

| Fonction | Effet |
|---|---|
| `html(content, status)` | Une réponse HTML, à partir d'un texte ou d'une vue. |
| `json(value, status)` | Une réponse JSON : les enregistrements sont des objets, les listes des tableaux. |
| `status(response, code)` | La réponse avec un autre statut : `"Introuvable" \|> status(404)`. |
| `redirect(url, code)` | Une redirection (303 See Other sauf si donné : 301, 302, 307, 308). |
| `with_header(response, name, value)` | La réponse avec un en-tête. |
| `with_cookie(response, name, value, options)` | La réponse avec un cookie (HttpOnly, SameSite=Lax, Secure en production). Options : `{max_age, path, same_site, http_only, secure}`. |
| `send_file(path)` | Le contenu d'un fichier, avec son type. |
| `no_content()` | Une réponse vide (204). |
| `problem(status, detail)` | Une erreur JSON : `problem(422, "le titre manque")` donne `{"error": "le titre manque"}`. |

## Middlewares

| Fonction | Effet |
|---|---|
| `next(req)` | Exécute la suite de la chaîne (les middlewares suivants, puis le handler) et renvoie sa réponse `{status, headers, body}`. |

## mw

| Fonction | Effet |
|---|---|
| `mw.security_headers(options)` | Les en-têtes de sécurité : Content-Security-Policy (WebAssembly permis), HSTS, X-Content-Type-Options, X-Frame-Options, Referrer-Policy, Permissions-Policy, Cross-Origin-Opener-Policy. Options : `{csp: "..."}`. |
| `mw.cors(origins, options)` | CORS : les origines autorisées à appeler l'API (`"*"` pour toutes), et les réponses aux requêtes préalables. |
| `mw.rate_limit(per_minute)` | Au plus ce nombre de requêtes par minute et par adresse ; ensuite 429 avec Retry-After. |
| `mw.logger()` | Une ligne par requête sur la sortie standard : méthode, chemin, statut, durée. |
| `mw.compress()` | Gzip pour les textes, le JSON, le SVG et le WebAssembly quand le navigateur l'accepte (calculé une fois pour les réponses en cache). |
| `mw.request_id()` | Un `X-Request-Id` sur chaque réponse (celui reçu, ou un nouveau). |
| `mw.timing()` | Un en-tête `Server-Timing` : le temps passé sur la requête, affiché par les outils des navigateurs. |
| `mw.basic_auth(user, password)` | L'authentification HTTP Basic (comparée en temps constant). |
| `mw.bearer(token)` | Exige `Authorization: Bearer <jeton>` (comparé en temps constant). |
| `mw.body_limit(bytes)` | Refuse les corps plus grands que cela (413). |
| `mw.headers(headers)` | Des en-têtes ajoutés à chaque réponse : `mw.headers({"x-powered-by": "Facile"})`. |
| `mw.cache_control(value)` | Un en-tête `Cache-Control` sur chaque réponse. |
| `mw.https()` | Derrière un proxy (Heroku), redirige `http://` vers `https://`. |
| `mw.waf()` | Un pare-feu applicatif : refuse les injections SQL, le XSS, la remontée de dossiers, Log4Shell et les injections de commandes dans les chemins et les requêtes (403). |
| `mw.block_bots()` | Refuse les robots d'attaque et les chemins qu'ils sondent (`/.env`, `/wp-admin`...). |
| `mw.csrf()` | Refuse les requêtes POST, PUT, PATCH et DELETE envoyées par un autre site (Origin et Sec-Fetch-Site vérifiés). |
| `mw.shield()` | Toutes les protections d'un coup : WAF, robots, en-têtes de sécurité, CSRF, 300 requêtes par minute, compression, identifiants de requête. |

## Pages et vues

| Fonction | Effet |
|---|---|
| `page(settings, content...)` | Une page HTML entière. Réglages : `{title, description, lang, url, image, css: [...], js: [...], head: [...], json_ld, theme_color, alternates, status}`. |
| `island(component, props)` | Un composant calculé sur le serveur puis animé dans le navigateur par WebAssembly : `island(compteur.component, {depart: 0})`. |
| `stylesheet(href)` | Un `<link rel="stylesheet">`. |
| `script(src)` | Un `<script type="module">` chargé depuis un fichier. |
| `meta(attributes)` | Un élément `<meta>` : `meta({name: "author", content: "Ada"})`. |
| `h(tag, attributes, children)` | N'importe quel élément HTML : `h("input", {name: "q"})`, `h("mon-widget", ["texte"])`. |
| `raw(html)` | Du HTML écrit tel quel, sans échappement (seulement pour du HTML de confiance). |
| `fragment(children, ...)` | Plusieurs enfants sans élément autour. |
| `render(view)` | Le HTML d'une vue, en texte. |
| `markdown(text)` | Du Markdown en vue : titres avec ancres, emphase, code (Rythme coloré), liens, listes, citations, tableaux. Le HTML du texte est affiché comme du texte. |
| `toc(text)` | Les titres d'un texte Markdown : `[{level, title, anchor}]`, pour une table des matières. |
| `highlight(code)` | Du code Rythme, coloré (`<span class="tok-keyword">`...). |

## Îlots : effets

| Fonction | Effet |
|---|---|
| `with_effects(state, effects)` | Ce que renvoie `init` ou `update` pour demander des effets au navigateur avec le nouvel état : `with_effects(s, [after(1000, :tic)])`. |
| `fetch_json(url, to_message)` | Va chercher du JSON ; son résultat arrive à `update` comme `to_message(Ok(valeur))` ou `to_message(Err(message))`. |
| `send_json(method, url, body, to_message)` | Envoie du JSON (`:post`, `:put`, `:patch`, `:delete`) ; la réponse arrive à `update` comme pour `fetch_json`. |
| `after(milliseconds, message)` | Donne le message à `update` après ce nombre de millisecondes. |
| `save(key, value)` | Garde une valeur dans le navigateur (localStorage), en JSON. |
| `load(key, to_message)` | Lit une valeur gardée par `save` : `to_message(Some(valeur))` ou `to_message(None)`. |
| `navigate(url)` | Va sur une autre page. |
| `focus(selector)` | Met le focus sur un élément de la page (`"#nom"`). |
| `evaluate(code)` | Exécute un programme Rythme à part : `{ok, output, errors}`. Il s'arrête après quelques millions d'appels. |

## Modèles et requêtes

| Fonction | Effet |
|---|---|
| `model(table, fields, options)` | Un modèle : une table et ses champs. `model("taches", {titre: :text, fait: field(:bool, {default: false})})`. Options : `{timestamps: false}`. Un modèle est aussi la requête de toutes ses lignes. |
| `field(kind, options)` | Un champ avec des règles : `field(:text, {required: true, min: 3, max: 120, unique: true, email: true, one_of: [...], default: "x", index: true})`. |
| `belongs_to(table, options)` | Une relation vers une ligne d'une autre table : `projet: belongs_to("projets")` enregistre `projet_id` ; `include(q, :projet)` ajoute la ligne. |
| `has_many(table, field)` | Les lignes d'une autre table qui renvoient à celle-ci : `taches: has_many("taches", :projet)` ; `include(q, :taches)` les ajoute. |
| `where(query, field, operator, value)` | Garde les lignes où le champ a la valeur (`where(q, :fait, false)`), ou le compare : `where(q, :prix, "<", 10)`, `:contains`, `:starts_with`, `:in`... |
| `where_any(query, conditions)` | Garde les lignes qui remplissent l'une des conditions : `where_any(q, [[:ville, "Paris"], [:ville, "Lyon"]])`. |
| `search(query, fields, text)` | Les lignes où l'un des champs contient le texte (sans tenir compte de la casse). |
| `order_by(query, field, direction)` | Trie les lignes selon un champ, :asc (par défaut) ou :desc ; plusieurs appels trient selon plusieurs champs. |
| `limit(query, n)` | Au plus n lignes. |
| `offset(query, n)` | Saute les n premières lignes. |
| `include(query, relation)` | Ajoute une relation à chaque ligne (`belongs_to` ou `has_many`), en une requête de plus, pas une par ligne. |
| `pick(query, fields)` | Seulement ces champs dans les enregistrements. |

## db

| Fonction | Effet |
|---|---|
| `db.all(query)` | Les lignes d'une requête, en liste d'enregistrements. |
| `db.first(query)` | La première ligne : `Some(enregistrement)` ou `None`. |
| `db.find(model, id)` | La ligne de cet id : `Some(enregistrement)` ou `None`. |
| `db.count(query)` | Le nombre de lignes. |
| `db.exists(query)` | Si la requête a des lignes. |
| `db.sum(query, field)` | La somme d'un champ : `Some(n)`, ou `None` sans lignes. |
| `db.avg(query, field)` | La moyenne d'un champ : `Some(x)` ou `None`. |
| `db.min(query, field)` | La plus petite valeur : `Some(x)` ou `None`. |
| `db.max(query, field)` | La plus grande valeur : `Some(x)` ou `None`. |
| `db.count_by(query, field)` | Combien de lignes pour chaque valeur d'un champ, en Dict. |
| `db.insert(model, record)` | Ajoute une ligne, vérifiée par les règles des champs : `Ok(enregistrement avec son id)` ou `Err({status: 422, error, fields})`. |
| `db.insert_all(model, records)` | Ajoute plusieurs lignes, toutes ou aucune : `Ok(enregistrements)` ou la première `Err`. |
| `db.update(model, id, changes)` | Change les champs donnés d'une ligne : `Ok(enregistrement)`, `Err(:not_found)` ou `Err({status: 422, ...})`. |
| `db.save(model, record)` | Insère un enregistrement sans id, met à jour un enregistrement qui en a un. |
| `db.update_where(query, changes)` | Change toutes les lignes d'une requête : `Ok(nombre de lignes)`. |
| `db.delete(model, id)` | Supprime une ligne : si elle existait. |
| `db.delete_where(query)` | Supprime les lignes d'une requête : combien. |
| `db.paginate(query, page, per_page)` | Une page de lignes : `{items, page, per_page, total, pages}` (20 par page sauf indication). |
| `db.transaction(function)` | Exécute la fonction dans une transaction : gardée si elle renvoie autre chose que `Err(...)`, annulée sinon. |
| `db.sql(sql, parameters)` | Du SQL écrit à la main, avec `?` pour les paramètres : `Ok(enregistrements)` ou `Err(message)`. |
| `db.validate(model, record)` | Vérifie un enregistrement selon les règles des champs, sans l'écrire. |

## Éléments HTML

Chacun s'appelle `tag()`, `tag(enfants)`, `tag(attributs)` ou `tag(attributs, enfants)` ; `h("nom", ...)` donne les autres.

`div`, `span`, `p`, `a`, `h1`, `h2`, `h3`, `h4`, `h5`, `h6`, `ul`, `ol`, `li`, `nav`, `header`, `footer`, `main`, `section`, `article`, `aside`, `button`, `img`, `form`, `label`, `strong`, `em`, `code`, `pre`, `small`, `br`, `hr`, `textarea`, `video`, `audio`, `canvas`, `iframe`, `figure`, `figcaption`, `blockquote`, `mark`, `dl`, `dt`, `dd`, `details`, `summary`, `svg`, `table`, `thead`, `tbody`, `tr`, `th`, `td`, `kbd`, `abbr`, `cite`, `picture`, `address`, `fieldset`, `legend`, `output`, `progress`, `meter`, `dialog`
