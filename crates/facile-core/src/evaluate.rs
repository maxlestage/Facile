//! `evaluate(code)`: runs a Rythme program apart, and gives what it printed.
//! For pages that let their readers try Rythme (a playground island).

use std::path::Path;

use rythme::builtins::Builtin;
use rythme::diagnostic::Fallible;
use rythme::driver::Driver;
use rythme::interp::Mode;
use rythme::io;
use rythme::source::Span;
use rythme::value::Value;

use crate::host;
use crate::util::{self, text_arg};

/// At most this many calls: a program that never ends stops.
const STEPS: u64 = 3_000_000;

fn evaluate(_: &mut rythme::interp::Interp, a: &[Value], span: Span) -> Fallible<Value> {
    util::mark_impure();
    let code = text_arg(a, 0, "evaluate", span)?.to_string();
    // The output of the program running now is put aside, and given back after.
    let before = io::take_captured();
    io::capture();
    let mut driver = Driver::new();
    driver.color = false;
    driver.interp.set_step_limit(STEPS);
    let mut ok = false;
    if let Some(m) = driver.load_source("essai.ry", &code, None, Path::new(".")) {
        if let Ok(v) = driver.execute(&m, Mode::Run) {
            ok = true;
            if let Some(v) = v.filter(|v| !matches!(v, Value::Unit)) {
                io::out_line(&v.repr());
            }
        }
    }
    let all = io::take_captured();
    if cfg!(target_arch = "wasm32") || !before.is_empty() {
        io::capture();
        io::out(&before);
    }
    let (mut output, mut errors) = (String::new(), String::new());
    let mut in_error = false;
    for c in all.chars() {
        match c {
            io::ERR_START => in_error = true,
            io::ERR_END => in_error = false,
            c if in_error => errors.push(c),
            c => output.push(c),
        }
    }
    Ok(util::record(vec![("ok", Value::Bool(ok)), ("output", Value::text(&output)), ("errors", Value::text(&errors))]))
}

pub static EVALUATE: &[Builtin] = &[host!(
    "evaluate",
    "evaluate(code)",
    1,
    1,
    evaluate,
    "Runs a Rythme program apart: `{ok, output, errors}`. It stops after a few million calls.",
    "Exécute un programme Rythme à part : `{ok, output, errors}`. Il s'arrête après quelques millions d'appels."
)];
