//! Components: a Rythme file that gives `component = {name, init, update, view}`.
//!
//! The server computes `view(init(props))` to send finished HTML; the browser
//! runs the same file in WebAssembly, then `update(state, message)` and `view`
//! at each event.

use rythme::diagnostic::Fallible;
use rythme::interp::Interp;
use rythme::source::Span;
use rythme::value::Value;

use crate::util::{error, field, tr};

/// A component's functions.
pub struct Component {
    pub name: String,
    pub init: Value,
    pub update: Option<Value>,
    pub view: Value,
}

impl Component {
    pub fn from_value(v: &Value, span: Span) -> Fallible<Component> {
        let bad = || {
            error(
                "island",
                tr("a component is a record {name, init, update, view}", "un composant est un enregistrement {name, init, update, view}"),
                span,
            )
        };
        let name = field(v, "name").map(|n| n.display()).ok_or_else(bad)?;
        let view = field(v, "view").filter(|f| f.is_callable()).cloned().ok_or_else(bad)?;
        let init = field(v, "init").cloned().unwrap_or(Value::Unit);
        let update = field(v, "update").filter(|f| f.is_callable()).cloned();
        Ok(Component { name, init, update, view })
    }

    /// The first state and its effects: `init(props)` (or `init` itself when
    /// it is not a function).
    pub fn initial(&self, it: &mut Interp, props: Value, span: Span) -> Fallible<(Value, Vec<Value>)> {
        let v = if self.init.is_callable() {
            it.call(self.init.clone(), vec![props], span)?
        } else if matches!(self.init, Value::Unit) {
            props
        } else {
            self.init.clone()
        };
        Ok(crate::effects::split(v))
    }

    pub fn render(&self, it: &mut Interp, state: Value, span: Span) -> Fallible<Value> {
        it.call(self.view.clone(), vec![state], span)
    }

    /// The next state and its effects; a component without `update` keeps its state.
    pub fn step(&self, it: &mut Interp, state: Value, msg: Value, span: Span) -> Fallible<(Value, Vec<Value>)> {
        match &self.update {
            Some(u) => Ok(crate::effects::split(it.call(u.clone(), vec![state, msg], span)?)),
            None => Ok((state, Vec::new())),
        }
    }
}
