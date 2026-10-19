//! Middlewares written in Rythme: `fn journal(req, next) = next(req)`.
//!
//! Each middleware gets the request and `next`, a function that runs the
//! rest of the chain (the next middlewares, then the handler) and returns
//! its response as a record `{status, headers, body}`. A middleware may
//! answer without calling `next`, change the request it gives to `next`
//! (`next({..req, user: u})`), or change the response it returns.

use std::cell::RefCell;
use std::rc::Rc;

use facile_core::host;
use facile_core::util::{error, tr};
use rythme::builtins::Builtin;
use rythme::diagnostic::Fallible;
use rythme::interp::Interp;
use rythme::source::Span;
use rythme::value::Value;

use super::respond;

/// What a call of `next` continues.
struct Frame {
    middlewares: Rc<[Value]>,
    handler: Value,
    index: usize,
}

thread_local! {
    static FRAMES: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
}

/// Runs the middlewares from `index`, then the handler.
pub fn run(it: &mut Interp, middlewares: &Rc<[Value]>, handler: &Value, index: usize, req: Value, span: Span) -> Fallible<Value> {
    let Some(mw) = middlewares.get(index) else {
        return it.call(handler.clone(), vec![req], span);
    };
    FRAMES.with(|f| f.borrow_mut().push(Frame { middlewares: middlewares.clone(), handler: handler.clone(), index: index + 1 }));
    let out = it.call(mw.clone(), vec![req, Value::Builtin(&NEXT)], span);
    FRAMES.with(|f| f.borrow_mut().pop());
    out
}

fn next(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let frame = FRAMES.with(|f| f.borrow().last().map(|fr| (fr.middlewares.clone(), fr.handler.clone(), fr.index)));
    let Some((mws, handler, index)) = frame else {
        return Err(error("next", tr("`next` is only called by a middleware", "`next` ne s'appelle que dans un middleware"), span));
    };
    // The frame of this middleware stays below while the rest runs: a
    // middleware that calls `next` twice runs the rest twice.
    let v = run(it, &mws, &handler, index, a[0].clone(), span)?;
    Ok(respond::to_value(&respond::from_value(&v)))
}

pub static NEXT: Builtin = host!(
    "next",
    "next(req)",
    1,
    1,
    next,
    "Runs the rest of the chain (the next middlewares, then the handler) and returns its response `{status, headers, body}`.",
    "Exécute la suite de la chaîne (les middlewares suivants, puis le handler) et renvoie sa réponse `{status, headers, body}`."
);
