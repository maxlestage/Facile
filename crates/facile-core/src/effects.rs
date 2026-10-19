//! What a component asks the browser to do, besides showing its view:
//! `with_effects(state, [fetch_json("/api/taches", Chargees)])`.
//!
//! An effect is a value too; the browser runs it, then gives its result to
//! the component as a message. On the server (computing the first view),
//! effects are left out.

use rythme::builtins::Builtin;
use rythme::diagnostic::Fallible;
use rythme::interp::Interp;
use rythme::source::Span;
use rythme::value::Value;

use crate::host;
use crate::util::{self, error, field, func_arg, int_arg, text_arg, tr};

fn effect(kind: &str, mut fields: Vec<(&str, Value)>) -> Value {
    fields.insert(0, ("effect", util::sym(kind)));
    util::record(fields)
}

fn with_effects(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let effects = match &a[1] {
        Value::List(_) => a[1].clone(),
        v if field(v, "effect").is_some() => Value::list(vec![v.clone()]),
        _ => {
            return Err(error(
                "with_effects",
                tr("give a list of effects: `[after(500, :tic)]`", "donnez une liste d'effets : `[after(500, :tic)]`"),
                span,
            ))
        }
    };
    Ok(util::record(vec![("facile_effects", Value::Bool(true)), ("state", a[0].clone()), ("effects", effects)]))
}

/// The state and the effects of what `init` or `update` returned.
pub fn split(v: Value) -> (Value, Vec<Value>) {
    if matches!(field(&v, "facile_effects"), Some(Value::Bool(true))) {
        let state = field(&v, "state").cloned().unwrap_or(Value::Unit);
        let effects = match field(&v, "effects") {
            Some(Value::List(l)) => l.iter().cloned().collect(),
            _ => Vec::new(),
        };
        return (state, effects);
    }
    (v, Vec::new())
}

fn fetch_json(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let url = text_arg(a, 0, "fetch_json", span)?;
    let to_msg = func_arg(a, 1, "fetch_json", span)?;
    Ok(effect("fetch", vec![("method", Value::text("GET")), ("url", Value::text(url)), ("body", Value::Unit), ("to_msg", to_msg.clone())]))
}

fn send_json(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let method = util::name_arg(a, 0, "send_json", span)?.to_ascii_uppercase();
    let url = text_arg(a, 1, "send_json", span)?;
    let to_msg = func_arg(a, 3, "send_json", span)?;
    Ok(effect(
        "fetch",
        vec![("method", Value::text(&method)), ("url", Value::text(url)), ("body", a[2].clone()), ("to_msg", to_msg.clone())],
    ))
}

fn after(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let ms = int_arg(a, 0, "after", span)?;
    Ok(effect("after", vec![("ms", Value::Int(ms.clamp(0, 86_400_000))), ("msg", a[1].clone())]))
}

fn save(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let key = text_arg(a, 0, "save", span)?;
    Ok(effect("save", vec![("key", Value::text(key)), ("value", a[1].clone())]))
}

fn load(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let key = text_arg(a, 0, "load", span)?;
    let to_msg = func_arg(a, 1, "load", span)?;
    Ok(effect("load", vec![("key", Value::text(key)), ("to_msg", to_msg.clone())]))
}

fn navigate(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let url = text_arg(a, 0, "navigate", span)?;
    Ok(effect("navigate", vec![("url", Value::text(url))]))
}

fn focus(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let selector = text_arg(a, 0, "focus", span)?;
    Ok(effect("focus", vec![("selector", Value::text(selector))]))
}

pub static EFFECTS: &[Builtin] = &[
    host!("with_effects", "with_effects(state, effects)", 2, 2, with_effects,
        "What `init` or `update` returns to ask the browser for effects with the new state: `with_effects(s, [after(1000, :tic)])`.",
        "Ce que renvoie `init` ou `update` pour demander des effets au navigateur avec le nouvel état : `with_effects(s, [after(1000, :tic)])`."),
    host!("fetch_json", "fetch_json(url, to_message)", 2, 2, fetch_json,
        "Fetches JSON; its result goes to `update` as `to_message(Ok(value))` or `to_message(Err(message))`.",
        "Va chercher du JSON ; son résultat arrive à `update` comme `to_message(Ok(valeur))` ou `to_message(Err(message))`."),
    host!("send_json", "send_json(method, url, body, to_message)", 4, 4, send_json,
        "Sends JSON (`:post`, `:put`, `:patch`, `:delete`); the answer goes to `update` like for `fetch_json`.",
        "Envoie du JSON (`:post`, `:put`, `:patch`, `:delete`) ; la réponse arrive à `update` comme pour `fetch_json`."),
    host!("after", "after(milliseconds, message)", 2, 2, after,
        "Gives the message to `update` after this many milliseconds.", "Donne le message à `update` après ce nombre de millisecondes."),
    host!("save", "save(key, value)", 2, 2, save,
        "Keeps a value in the browser (localStorage), as JSON.", "Garde une valeur dans le navigateur (localStorage), en JSON."),
    host!("load", "load(key, to_message)", 2, 2, load,
        "Reads a value kept by `save`: `to_message(Some(value))` or `to_message(None)`.", "Lit une valeur gardée par `save` : `to_message(Some(valeur))` ou `to_message(None)`."),
    host!("navigate", "navigate(url)", 1, 1, navigate, "Goes to another page.", "Va sur une autre page."),
    host!("focus", "focus(selector)", 1, 1, focus, "Puts the focus on an element of the page (`\"#nom\"`).", "Met le focus sur un élément de la page (`\"#nom\"`)."),
];

pub fn define(it: &mut Interp) {
    util::define(it, EFFECTS);
}
