# WebAssembly islands

An **island** is an interactive component written in Rythme. The server computes its first view and sends it as HTML (fast to show, readable by search engines); then the Rythme interpreter, **compiled to WebAssembly**, brings it to life in the browser. The rest of the page sends no code.

## A component

A component is a file of `client/` that ends with `component = {name, init, update, view}`:

```rythme
# client/compteur.ry

fn init(props) = {n: props.depart}

fn update(state, message) = match message
  :plus -> {..state, n: state.n + 1}
  :minus -> {..state, n: state.n - 1}
end

fn view(state) = div({class: "counter"}, [
  button({on_click: :minus}, ["−"]),
  strong([state.n]),
  button({on_click: :plus}, ["+"]),
])

component = {name: "compteur", init, update, view}
```

This is the Elm architecture: a **state** (a value), **messages**, `update` that computes the new state, `view` that shows it. Nothing changes in place.

## Putting it in a page

```rythme
use client/compteur

fn home(req) = page({title: "Counter"}, [
  h1("A counter"),
  island(compteur.component, {depart: 10}),
])
```

`island(component, props)` calls `init(props)` and `view` on the server, and puts a `<facile-island>` element with the props as JSON. In the browser, `facile.js` loads `facile.wasm` (once for the whole page, cached), the component's file, and takes over when the island is about to be seen.

## Events

| Attribute | Message given to `update` |
|---|---|
| `on_click: :plus` | `:plus` |
| `on_input: Typed` | `Typed(text)`: the function gets the value of the field |
| `on_change: Picked` | `Picked(value)` (a checkbox gives `"true"` or `"false"`) |
| `on_submit: Send` | `Send`; with a function, it gets the fields of the form as a record |
| `on_keydown: Key` | `Key("Enter")` |

Messages can be symbols, lists, or the values of a type of yours:

```rythme
type Message = Typed(text) | Add | Remove(id)
```

After each message, Facile compares the new view with the old one and changes only what changed in the page.

## Effects

`init` and `update` can ask the browser for effects with `with_effects(state, [effects])`. The result of an effect comes back to `update` as a message.

```rythme
type Message = Loaded(result) | Add | Added(result) | Tick

fn init(props) = with_effects({tasks: [], error: None}, [fetch_json("/api/tasks", Loaded)])

fn update(state, message) = match message
  Loaded(Ok(tasks)) -> {..state, tasks: tasks}
  Loaded(Err(e)) -> {..state, error: Some(e)}
  Add -> with_effects(state, [send_json(:post, "/api/tasks", {title: "New"}, Added)])
  Added(Ok(t)) -> {..state, tasks: [t, ..state.tasks]}
  Added(Err(e)) -> {..state, error: Some(e)}
  Tick -> with_effects(state, [after(1000, Tick)])
end
```

| Effect | What it does |
|---|---|
| `fetch_json(url, F)` | `GET`; then `F(Ok(value))` or `F(Err(message))` |
| `send_json(:post, url, body, F)` | `POST`, `PUT`, `PATCH`, `DELETE` with a JSON body |
| `after(ms, message)` | the message after that delay |
| `save(key, value)` | keeps a value in the browser (`localStorage`) |
| `load(key, F)` | `F(Some(value))` or `F(None)` |
| `navigate(url)` | goes to another page |
| `focus("#field")` | puts the focus on an element |

On the server, for the first view, effects are left out: only the state counts.

## What runs in the browser

A component runs in the same Rythme as the server: the same library, views, effects, and `evaluate(code)` (which runs a Rythme program apart: that is how the try-it box of the home page works). What only makes sense on the server (`db`, routes) is not there: `facile check` says so before anything runs.

A component fits in one file. The WebAssembly module weighs about 1 MB compressed: it is the whole Rythme interpreter, with its error messages in six languages, loaded once and kept in the cache.
