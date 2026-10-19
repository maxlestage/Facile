//! Small helpers to read the arguments of host functions and build Rythme values.

use std::cell::Cell;
use std::rc::Rc;

use rythme::builtins::{Builtin, Category};
use rythme::diagnostic::{Diagnostic, Fallible};
use rythme::interp::Interp;
use rythme::source::Span;
use rythme::symbol::Sym;
use rythme::value::{Record, Value};

thread_local! {
    /// Set when the computation of a response read something that its path
    /// and its query do not decide (a header, the database, the clock...):
    /// the response cannot then be kept and given again.
    static IMPURE: Cell<bool> = const { Cell::new(false) };
}

/// The response being computed depends on more than its path and query.
pub fn mark_impure() {
    IMPURE.with(|f| f.set(true));
}

/// Starts watching a computation; [`was_impure`] tells afterwards.
pub fn reset_impure() {
    IMPURE.with(|f| f.set(false));
}

pub fn was_impure() -> bool {
    IMPURE.with(|f| f.get())
}

/// A host function, with its documentation.
#[macro_export]
macro_rules! host {
    ($name:literal, $sig:literal, $min:expr, $max:expr, $f:path, $en:literal, $fr:literal) => {
        rythme::builtins::Builtin {
            name: $name,
            sig: $sig,
            min: $min,
            max: $max,
            cat: rythme::builtins::Category::Basics,
            en: $en,
            fr: $fr,
            example: "",
            func: $f,
        }
    };
}

pub const MANY: usize = usize::MAX;

/// Makes host functions known to the interpreter (and to its checker, which
/// reads the interpreter's names).
pub fn define(it: &mut Interp, builtins: &'static [Builtin]) {
    for b in builtins {
        it.globals.insert(Sym::new(b.name), Value::Builtin(b));
    }
}

/// A record whose fields are host functions: `db.all(q)`.
pub fn namespace(it: &mut Interp, name: &str, builtins: &'static [Builtin]) {
    let mut r = Record::new();
    for b in builtins {
        r.set(Rc::from(b.name), Value::Builtin(b));
    }
    it.globals.insert(Sym::new(name), Value::Record(Rc::new(r)));
}

/// A builtin's category, for documentation tools.
pub const CATEGORY: Category = Category::Basics;

/// The English or the French text, in the language of the messages.
pub fn tr(en: &str, fr: &str) -> String {
    if rythme::lang::code() == "fr" {
        fr.to_string()
    } else {
        en.to_string()
    }
}

fn wanted_fr(w: &str) -> &str {
    match w {
        "a Text" => "un Text",
        "a Text or a Symbol" => "un Text ou un Symbol",
        "an Int" => "un Int",
        "a Record" => "un Record",
        "a function" => "une fonction",
        "a List" => "une List",
        _ => w,
    }
}

pub fn error(f: &str, msg: impl Into<String>, span: Span) -> Box<Diagnostic> {
    Diagnostic::error("E021", format!("{f}: {}", msg.into())).at(span).boxed()
}

fn wrong(f: &str, i: usize, wanted: &str, got: &Value, span: Span) -> Box<Diagnostic> {
    let n = i + 1;
    error(
        f,
        tr(
            &format!("argument {n} must be {wanted}, not {} {}", got.type_name(), short(got)),
            &format!("l'argument {n} doit être {}, pas {} {}", wanted_fr(wanted), got.type_name(), short(got)),
        ),
        span,
    )
}

/// A value shown in a message, cut when long.
pub fn short(v: &Value) -> String {
    let s = v.repr();
    if s.chars().count() > 60 {
        let cut: String = s.chars().take(57).collect();
        format!("{cut}...")
    } else {
        s
    }
}

pub fn text_arg<'a>(a: &'a [Value], i: usize, f: &str, span: Span) -> Fallible<&'a str> {
    match a.get(i) {
        Some(Value::Text(s)) => Ok(s),
        Some(v) => Err(wrong(f, i, "a Text", v, span)),
        None => Err(error(f, "missing argument", span)),
    }
}

/// A text, or a symbol written for a name (`:get` as well as `"get"`).
pub fn name_arg(a: &[Value], i: usize, f: &str, span: Span) -> Fallible<String> {
    match a.get(i) {
        Some(Value::Text(s)) => Ok(s.to_string()),
        Some(Value::Symbol(s)) => Ok(s.name().to_string()),
        Some(v) => Err(wrong(f, i, "a Text or a Symbol", v, span)),
        None => Err(error(f, "missing argument", span)),
    }
}

pub fn int_arg(a: &[Value], i: usize, f: &str, span: Span) -> Fallible<i64> {
    match a.get(i) {
        Some(Value::Int(n)) => Ok(*n),
        Some(v) => Err(wrong(f, i, "an Int", v, span)),
        None => Err(error(f, "missing argument", span)),
    }
}

pub fn record_arg<'a>(a: &'a [Value], i: usize, f: &str, span: Span) -> Fallible<&'a Record> {
    match a.get(i) {
        Some(Value::Record(r)) => Ok(r),
        Some(v) => Err(wrong(f, i, "a Record", v, span)),
        None => Err(error(f, "missing argument", span)),
    }
}

pub fn func_arg<'a>(a: &'a [Value], i: usize, f: &str, span: Span) -> Fallible<&'a Value> {
    match a.get(i) {
        Some(v) if v.is_callable() => Ok(v),
        Some(v) => Err(wrong(f, i, "a function", v, span)),
        None => Err(error(f, "missing argument", span)),
    }
}

/// The name of a symbol or a text value.
pub fn name_of(v: &Value) -> Option<String> {
    match v {
        Value::Text(s) => Some(s.to_string()),
        Value::Symbol(s) => Some(s.name().to_string()),
        _ => None,
    }
}

pub fn text(s: impl AsRef<str>) -> Value {
    Value::text(s.as_ref())
}

pub fn sym(s: &str) -> Value {
    Value::Symbol(Sym::new(s))
}

/// A record from fields written in order.
pub fn record(fields: Vec<(&str, Value)>) -> Value {
    let mut r = Record::new();
    for (k, v) in fields {
        r.set(Rc::from(k), v);
    }
    Value::Record(Rc::new(r))
}

/// A field of a record value.
pub fn field<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    match v {
        Value::Record(r) => r.get(key),
        _ => None,
    }
}

pub fn field_text(v: &Value, key: &str) -> Option<String> {
    field(v, key).and_then(name_of)
}

/// The record `r` with the field `key` set to `val` (a new record: values never change).
pub fn with_field(r: &Record, key: &str, val: Value) -> Value {
    let mut r = r.clone();
    r.set(Rc::from(key), val);
    Value::Record(Rc::new(r))
}

/// The name of a variant (`Some`, `Ok`, `Err`, `None`, or a user type's).
pub fn variant_name(v: &Value) -> Option<String> {
    match v {
        Value::Variant(var) => Some(var.ctor.name.name().to_string()),
        _ => None,
    }
}

/// The first field of a variant (`x` in `Some(x)`).
pub fn payload(v: &Value) -> Value {
    Interp::payload(v)
}

pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Unit => false,
        Value::Variant(_) => variant_name(v).as_deref() != Some("None"),
        _ => true,
    }
}
