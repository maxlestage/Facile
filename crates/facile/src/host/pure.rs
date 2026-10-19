//! Rythme is purely functional: a handler that reads nothing but its path
//! and query gives the same response each time, and Facile keeps it.
//!
//! The few functions of the language that read the world (the clock,
//! random numbers, files, the environment) are wrapped here: each one marks
//! the response being computed as one that cannot be kept.

use std::sync::OnceLock;

use facile_core::util::mark_impure;
use rythme::builtins::{lookup, Builtin};
use rythme::diagnostic::Fallible;
use rythme::interp::Interp;
use rythme::source::Span;
use rythme::symbol::Sym;
use rythme::value::Value;

macro_rules! impure {
    ($( $f:ident => $name:literal $(, $path:literal)? );* $(;)?) => {
        $(
            fn $f(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
                static ORIGINAL: OnceLock<&'static Builtin> = OnceLock::new();
                mark_impure();
                let b = ORIGINAL.get_or_init(|| lookup($name).expect($name));
                $(
                    // A relative path is read from the application's folder.
                    let _ = $path;
                    if let Some(Value::Text(p)) = a.first() {
                        if !std::path::Path::new(p.as_str()).is_absolute() {
                            let mut args = a.to_vec();
                            args[0] = Value::text(&crate::runtime::resolve(p).to_string_lossy());
                            return (b.func)(it, &args, span);
                        }
                    }
                )?
                (b.func)(it, a, span)
            }
        )*
        const WRAPPED: &[(&str, rythme::builtins::BuiltinFn)] = &[$( ($name, $f) ),*];
    };
}

impure!(
    print_ => "print"; debug_ => "debug"; input_ => "input"; input_lines_ => "input_lines"; args_ => "args";
    options_ => "options"; env_ => "env"; time_ => "time"; random_ => "random"; random_seed_ => "random_seed";
    shuffle_ => "shuffle"; read_file_ => "read_file", "path"; write_file_ => "write_file", "path";
    append_file_ => "append_file", "path"; file_exists_ => "file_exists", "path"; file_info_ => "file_info", "path";
    list_folder_ => "list_folder", "path"; create_folder_ => "create_folder", "path"; delete_file_ => "delete_file", "path";
    now_ => "now"; today_ => "today"; sample_ => "sample";
);

fn wrapped() -> &'static [Builtin] {
    static ALL: OnceLock<Vec<Builtin>> = OnceLock::new();
    ALL.get_or_init(|| {
        WRAPPED
            .iter()
            .filter_map(|(name, f)| {
                let b = lookup(name)?;
                Some(Builtin {
                    name: b.name,
                    sig: b.sig,
                    min: b.min,
                    max: b.max,
                    cat: b.cat,
                    en: b.en,
                    fr: b.fr,
                    example: b.example,
                    func: *f,
                })
            })
            .collect()
    })
}

/// Puts the wrapped functions in place of the originals, under their Ruby names too.
pub fn install(it: &mut Interp) {
    for b in wrapped() {
        it.globals.insert(Sym::new(b.name), Value::Builtin(b));
        for alias in rythme::builtins::aliases(b.name) {
            it.globals.insert(Sym::new(alias), Value::Builtin(b));
        }
    }
}
