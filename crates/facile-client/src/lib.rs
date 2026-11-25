//! The browser side of Facile, compiled to WebAssembly: the Rythme
//! interpreter runs the components (`init`, `update`, `view`) and gives
//! their views to `facile.js`, which patches the page.
//!
//! Each exported function writes its answer, as JSON, in a buffer the page
//! reads with `facile_out_ptr` and `facile_out_len`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

use facile_core::component::Component;
use facile_core::json::{parse_json, to_json};
use facile_core::util::{field, name_of, variant_name};
use facile_core::view::vdom;
use rythme::driver::Driver;
use rythme::interp::Mode;
use rythme::source::Span;
use rythme::value::Value;
use serde_json::{json, Map, Value as Json};

struct Instance {
    component: String,
    state: Value,
    /// The event handlers of the last view, by position.
    handlers: Vec<Value>,
    /// What to do with the results of effects (fetch, load, after), by number.
    callbacks: HashMap<u32, Value>,
    next_callback: u32,
}

struct Loaded {
    driver: Driver,
    component: Component,
}

thread_local! {
    static COMPONENTS: RefCell<HashMap<String, Loaded>> = RefCell::new(HashMap::new());
    static INSTANCES: RefCell<Vec<Option<Instance>>> = const { RefCell::new(Vec::new()) };
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Memory for `len` bytes the page writes into.
#[no_mangle]
pub extern "C" fn facile_alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len.max(1));
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Gives back memory from `facile_alloc` (the exported functions free their inputs themselves).
///
/// # Safety
/// `ptr` must come from `facile_alloc(len)`.
#[no_mangle]
pub unsafe extern "C" fn facile_free(ptr: *mut u8, len: usize) {
    // SAFETY: allocated by `facile_alloc` with this capacity.
    drop(unsafe { Vec::from_raw_parts(ptr, 0, len.max(1)) });
}

#[no_mangle]
pub extern "C" fn facile_out_ptr() -> *const u8 {
    OUT.with(|o| o.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn facile_out_len() -> usize {
    OUT.with(|o| o.borrow().len())
}

/// The text the page wrote at `ptr` (and frees).
unsafe fn take(ptr: *mut u8, len: usize) -> String {
    // SAFETY: the page allocated this buffer with `facile_alloc(len)` and filled `len` bytes.
    let bytes = unsafe { Vec::from_raw_parts(ptr, len, len.max(1)) };
    String::from_utf8_lossy(&bytes).into_owned()
}

fn answer(j: Json) {
    let logs = rythme::io::take_captured();
    let mut j = j;
    if !logs.is_empty() {
        if let Json::Object(o) = &mut j {
            o.insert("log".into(), Json::String(logs));
        }
    }
    let text = serde_json::to_vec(&j).unwrap_or_default();
    OUT.with(|o| *o.borrow_mut() = text);
}

fn prepare() {
    rythme::interp::set_max_depth(600);
    rythme::interp::set_stack_budget(12 << 20);
    rythme::io::capture();
}

/// Loads a component's source: `component = {name, init, update, view}` at
/// its end. Answers `{"ok": true}` or `{"error": "messages"}`.
///
/// # Safety
/// Both buffers come from `facile_alloc` and hold UTF-8.
#[no_mangle]
pub unsafe extern "C" fn facile_load(name: *mut u8, name_len: usize, src: *mut u8, src_len: usize, lang: u32) {
    let (name, src) = unsafe { (take(name, name_len), take(src, src_len)) };
    rythme::lang::set(rythme::lang::Lang::all().nth(lang as usize).unwrap_or(rythme::lang::Lang::En));
    prepare();
    let mut driver = Driver::new();
    driver.color = false;
    facile_core::view::define(&mut driver.interp);
    let file = format!("client/{name}.ry");
    let code = format!("{src}\ncomponent\n");
    let loaded = driver.load_source(&file, &code, None, Path::new("/")).and_then(|m| driver.execute(&m, Mode::Run).ok().flatten());
    let Some(value) = loaded else {
        return answer(json!({ "error": rythme::io::take_captured() }));
    };
    match Component::from_value(&value, Span::default()) {
        Ok(component) => {
            COMPONENTS.with(|c| c.borrow_mut().insert(name, Loaded { driver, component }));
            answer(json!({ "ok": true }));
        }
        Err(d) => answer(json!({ "error": d.render(false) })),
    }
}

/// The view of an instance and the effects to run, as the page reads them.
fn render(id: usize, inst: &mut Instance, loaded: &mut Loaded, effects: Vec<Value>) -> Json {
    let view = match loaded.component.render(&mut loaded.driver.interp, inst.state.clone(), Span::default()) {
        Ok(v) => v,
        Err(d) => return json!({ "error": d.render(false) }),
    };
    let mut handlers = Vec::new();
    let tree = vdom(&view, &mut handlers);
    inst.handlers = handlers;
    let effects: Vec<Json> = effects.iter().filter_map(|e| effect_json(inst, e)).collect();
    json!({ "id": id, "vdom": tree, "effects": effects })
}

/// An effect for the page; its functions and messages stay here, by number.
fn effect_json(inst: &mut Instance, e: &Value) -> Option<Json> {
    let kind = field(e, "effect").and_then(name_of)?;
    let mut o = Map::new();
    o.insert("effect".into(), Json::String(kind.clone()));
    let mut keep = |v: Value| {
        let n = inst.next_callback;
        inst.next_callback += 1;
        inst.callbacks.insert(n, v);
        Json::from(n)
    };
    match kind.as_str() {
        "fetch" => {
            o.insert("method".into(), Json::String(field(e, "method").map(|m| m.display()).unwrap_or_else(|| "GET".into())));
            o.insert("url".into(), Json::String(field(e, "url").map(|m| m.display()).unwrap_or_default()));
            if let Some(b) = field(e, "body").filter(|b| !matches!(b, Value::Unit)) {
                o.insert("body".into(), to_json(b));
            }
            o.insert("cb".into(), keep(field(e, "to_msg").cloned().unwrap_or(Value::Unit)));
        }
        "after" => {
            o.insert("ms".into(), to_json(field(e, "ms").unwrap_or(&Value::Int(0))));
            o.insert("cb".into(), keep(field(e, "msg").cloned().unwrap_or(Value::Unit)));
        }
        "save" => {
            o.insert("key".into(), Json::String(field(e, "key").map(|m| m.display()).unwrap_or_default()));
            o.insert("value".into(), to_json(field(e, "value").unwrap_or(&Value::Unit)));
        }
        "load" => {
            o.insert("key".into(), Json::String(field(e, "key").map(|m| m.display()).unwrap_or_default()));
            o.insert("cb".into(), keep(field(e, "to_msg").cloned().unwrap_or(Value::Unit)));
        }
        "navigate" => {
            o.insert("url".into(), Json::String(field(e, "url").map(|m| m.display()).unwrap_or_default()));
        }
        "focus" => {
            o.insert("selector".into(), Json::String(field(e, "selector").map(|m| m.display()).unwrap_or_default()));
        }
        _ => return None,
    }
    Some(Json::Object(o))
}

/// Starts an instance of a loaded component with its props (JSON).
///
/// # Safety
/// Both buffers come from `facile_alloc` and hold UTF-8.
#[no_mangle]
pub unsafe extern "C" fn facile_mount(name: *mut u8, name_len: usize, props: *mut u8, props_len: usize) {
    let (name, props) = unsafe { (take(name, name_len), take(props, props_len)) };
    prepare();
    let out = COMPONENTS.with(|c| {
        let mut c = c.borrow_mut();
        let Some(loaded) = c.get_mut(&name) else {
            return json!({ "error": format!("component `{name}` is not loaded") });
        };
        let props = parse_json(&loaded.driver.interp, props.as_bytes()).unwrap_or(Value::Unit);
        let (state, effects) = match loaded.component.initial(&mut loaded.driver.interp, props, Span::default()) {
            Ok(x) => x,
            Err(d) => return json!({ "error": d.render(false) }),
        };
        let mut inst = Instance { component: name.clone(), state, handlers: Vec::new(), callbacks: HashMap::new(), next_callback: 0 };
        INSTANCES.with(|all| {
            let mut all = all.borrow_mut();
            let id = all.len();
            let out = render(id, &mut inst, loaded, effects);
            all.push(Some(inst));
            out
        })
    });
    answer(out);
}

/// Gives a message to an instance: `update`, then `view`.
fn deliver(id: usize, msg: Value) -> Json {
    INSTANCES.with(|all| {
        let mut all = all.borrow_mut();
        let Some(Some(inst)) = all.get_mut(id) else {
            return json!({ "error": "no such instance" });
        };
        COMPONENTS.with(|c| {
            let mut c = c.borrow_mut();
            let Some(loaded) = c.get_mut(&inst.component) else {
                return json!({ "error": "component not loaded" });
            };
            match loaded.component.step(&mut loaded.driver.interp, inst.state.clone(), msg, Span::default()) {
                Ok((state, effects)) => {
                    inst.state = state;
                    render(id, inst, loaded, effects)
                }
                Err(d) => json!({ "error": d.render(false) }),
            }
        })
    })
}

/// The message of a handler (`on_click: :plus`) or of a function given the
/// event's value (`on_input: fn(v) = Nom(v)`).
fn message_of(id: usize, handler: Value, value: Value) -> Result<Value, Json> {
    if !handler.is_callable() {
        return Ok(handler);
    }
    INSTANCES.with(|all| {
        let all = all.borrow();
        let Some(Some(inst)) = all.get(id) else { return Err(json!({ "error": "no such instance" })) };
        COMPONENTS.with(|c| {
            let mut c = c.borrow_mut();
            let Some(loaded) = c.get_mut(&inst.component) else { return Err(json!({ "error": "component not loaded" })) };
            loaded.driver.interp.call(handler, vec![value], Span::default()).map_err(|d| json!({ "error": d.render(false) }))
        })
    })
}

/// An event on an element: `kind` 0 for a text value (input, key), 1 for
/// JSON (a form's fields).
///
/// # Safety
/// The buffer comes from `facile_alloc` and holds UTF-8.
#[no_mangle]
pub unsafe extern "C" fn facile_event(id: usize, handler: usize, value: *mut u8, value_len: usize, kind: u32) {
    let value = unsafe { take(value, value_len) };
    prepare();
    let h = INSTANCES.with(|all| all.borrow().get(id).and_then(|i| i.as_ref()).and_then(|i| i.handlers.get(handler).cloned()));
    let Some(h) = h else {
        return answer(json!({ "error": "no such handler" }));
    };
    let v = if kind == 1 {
        COMPONENTS.with(|c| {
            let c = c.borrow();
            let inst_name = INSTANCES.with(|all| all.borrow().get(id).and_then(|i| i.as_ref()).map(|i| i.component.clone()));
            match inst_name.and_then(|n| c.get(&n).map(|l| parse_json(&l.driver.interp, value.as_bytes()))) {
                Some(Ok(v)) => v,
                _ => Value::text(&value),
            }
        })
    } else {
        Value::text(&value)
    };
    let out = match message_of(id, h, v) {
        Ok(msg) => deliver(id, msg),
        Err(e) => e,
    };
    answer(out);
}

/// The result of an effect: `{"ok": v}`, `{"err": "message"}`, `{"some": v}`,
/// `{"none": true}` or `{"tick": true}`.
///
/// # Safety
/// The buffer comes from `facile_alloc` and holds UTF-8.
#[no_mangle]
pub unsafe extern "C" fn facile_callback(id: usize, cb: u32, value: *mut u8, value_len: usize) {
    let value = unsafe { take(value, value_len) };
    prepare();
    let f = INSTANCES.with(|all| all.borrow_mut().get_mut(id).and_then(|i| i.as_mut()).and_then(|i| i.callbacks.remove(&cb)));
    let Some(f) = f else {
        return answer(json!({ "error": "no such callback" }));
    };
    let arg = INSTANCES.with(|all| {
        let all = all.borrow();
        let name = all.get(id).and_then(|i| i.as_ref()).map(|i| i.component.clone()).unwrap_or_default();
        COMPONENTS.with(|c| {
            let c = c.borrow();
            let Some(l) = c.get(&name) else { return Value::Unit };
            let it = &l.driver.interp;
            let parsed = parse_json(it, value.as_bytes()).unwrap_or(Value::Unit);
            match (field(&parsed, "ok"), field(&parsed, "err"), field(&parsed, "some"), field(&parsed, "none")) {
                (Some(v), _, _, _) => it.ok(v.clone()),
                (_, Some(e), _, _) => it.err(e.clone()),
                (_, _, Some(v), _) => it.some(v.clone()),
                (_, _, _, Some(_)) => it.none(),
                _ => Value::Unit,
            }
        })
    });
    let msg = if f.is_callable() {
        match message_of(id, f, arg) {
            Ok(m) => m,
            Err(e) => return answer(e),
        }
    } else {
        f
    };
    let _ = variant_name(&msg);
    answer(deliver(id, msg));
}
