//! Rythme values to JSON and back.

use std::rc::Rc;

use rythme::interp::Interp;
use rythme::value::{Record, Value};
use serde_json::{Map, Number, Value as Json};

use crate::util::variant_name;

/// A value as JSON: records are objects, lists and sets arrays, symbols
/// texts, `None` and `()` null, `Some(x)` and `Ok(x)` are `x`.
pub fn to_json(v: &Value) -> Json {
    match v {
        Value::Unit => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Int(n) => Json::Number((*n).into()),
        Value::Big(n) => Json::String(n.to_string()),
        Value::Float(f) => Number::from_f64(*f).map(Json::Number).unwrap_or(Json::Null),
        Value::Text(s) => Json::String(s.to_string()),
        Value::Symbol(s) => Json::String(s.name().to_string()),
        Value::List(l) => Json::Array(l.iter().map(to_json).collect()),
        Value::Set(m) => Json::Array(m.keys().map(to_json).collect()),
        Value::Record(r) => {
            let mut o = Map::new();
            for (k, x) in r.iter() {
                o.insert(k.to_string(), to_json(x));
            }
            Json::Object(o)
        }
        Value::Dict(m) => {
            if m.keys().all(|k| matches!(k, Value::Text(_) | Value::Symbol(_) | Value::Int(_))) {
                let mut o = Map::new();
                for (k, x) in m.iter() {
                    o.insert(k.display(), to_json(x));
                }
                Json::Object(o)
            } else {
                Json::Array(m.iter().map(|(k, x)| Json::Array(vec![to_json(k), to_json(x)])).collect())
            }
        }
        Value::Variant(var) => match variant_name(v).as_deref() {
            Some("None") => Json::Null,
            Some("Some") | Some("Ok") => to_json(&var.fields[0]),
            Some("Err") => {
                let mut o = Map::new();
                o.insert("error".into(), to_json(&var.fields[0]));
                Json::Object(o)
            }
            _ if var.fields.is_empty() => Json::String(var.ctor.name.name().to_string()),
            _ => {
                let mut o = Map::new();
                o.insert("type".into(), Json::String(var.ctor.name.name().to_string()));
                o.insert("fields".into(), Json::Array(var.fields.iter().map(to_json).collect()));
                Json::Object(o)
            }
        },
        Value::Func(_) | Value::Builtin(_) | Value::Ctor(_) => Json::Null,
    }
}

pub fn to_json_text(v: &Value) -> String {
    let mut out = String::with_capacity(64);
    write_json(v, &mut out);
    out
}

/// JSON as a value: objects are records, arrays lists, null is `None`.
pub fn from_json(it: &Interp, j: &Json) -> Value {
    match j {
        Json::Null => it.none(),
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => match n.as_i64() {
            Some(i) => Value::Int(i),
            None => Value::Float(n.as_f64().unwrap_or(0.0)),
        },
        Json::String(s) => Value::text(s),
        Json::Array(a) => Value::list(a.iter().map(|x| from_json(it, x)).collect()),
        Json::Object(o) => {
            let mut r = Record::new();
            for (k, x) in o {
                r.set(Rc::from(k.as_str()), from_json(it, x));
            }
            Value::Record(Rc::new(r))
        }
    }
}

/// Escapes a text for HTML (text and attribute values).
pub fn escape_html(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
}

pub fn escaped(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    escape_html(s, &mut out);
    out
}

// ----- straight from and to text -----------------------------------------
//
// The two functions below do what `to_json` and `from_json` do, without the
// `serde_json::Value` in between: each request with a JSON body goes through
// them, and building that value twice costs more than the rest.

/// A value written as JSON text.
pub fn write_json(v: &Value, out: &mut String) {
    use std::fmt::Write;
    match v {
        Value::Unit => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(n) => {
            let _ = write!(out, "{n}");
        }
        Value::Big(n) => write_str(&n.to_string(), out),
        Value::Float(f) if f.is_finite() => {
            let _ = write!(out, "{f}");
            if f.fract() == 0.0 && f.abs() < 1e16 {
                out.push_str(".0");
            }
        }
        Value::Float(_) => out.push_str("null"),
        Value::Text(s) => write_str(s, out),
        Value::Symbol(s) => write_str(&s.name(), out),
        Value::List(l) => {
            out.push('[');
            for (i, x) in l.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_json(x, out);
            }
            out.push(']');
        }
        Value::Set(m) => {
            out.push('[');
            for (i, x) in m.keys().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_json(x, out);
            }
            out.push(']');
        }
        Value::Record(r) => {
            out.push('{');
            for (i, (k, x)) in r.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_str(k, out);
                out.push(':');
                write_json(x, out);
            }
            out.push('}');
        }
        Value::Variant(var) => match variant_name(v).as_deref() {
            Some("None") => out.push_str("null"),
            Some("Some") | Some("Ok") => write_json(&var.fields[0], out),
            _ => {
                let j = to_json(v);
                out.push_str(&serde_json::to_string(&j).unwrap_or_else(|_| "null".into()));
            }
        },
        Value::Dict(_) => {
            let j = to_json(v);
            out.push_str(&serde_json::to_string(&j).unwrap_or_else(|_| "null".into()));
        }
        Value::Func(_) | Value::Builtin(_) | Value::Ctor(_) => out.push_str("null"),
    }
}

fn write_str(s: &str, out: &mut String) {
    out.push('"');
    // Most texts have nothing to escape: written in one go.
    if !s.bytes().any(|b| b < 0x20 || b == b'"' || b == b'\\') {
        out.push_str(s);
        out.push('"');
        return;
    }
    let mut start = 0;
    for (i, b) in s.bytes().enumerate() {
        let esc = match b {
            b'"' => "\\\"",
            b'\\' => "\\\\",
            b'\n' => "\\n",
            b'\r' => "\\r",
            b'\t' => "\\t",
            0..=0x1f => "",
            _ => continue,
        };
        out.push_str(&s[start..i]);
        if esc.is_empty() {
            use std::fmt::Write;
            let _ = write!(out, "\\u{:04x}", b);
        } else {
            out.push_str(esc);
        }
        start = i + 1;
    }
    out.push_str(&s[start..]);
    out.push('"');
}

/// JSON text read straight into a value (`null` is `None`).
pub fn parse_json(it: &Interp, text: &[u8]) -> Result<Value, String> {
    use serde::de::DeserializeSeed;
    let mut de = serde_json::Deserializer::from_slice(text);
    let v = Seed { it }.deserialize(&mut de).map_err(|e| e.to_string())?;
    de.end().map_err(|e| e.to_string())?;
    Ok(v)
}

struct Seed<'a> {
    it: &'a Interp,
}

impl<'de> serde::de::DeserializeSeed<'de> for Seed<'_> {
    type Value = Value;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> serde::de::Visitor<'de> for Seed<'_> {
    type Value = Value;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("JSON")
    }
    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(self.it.none())
    }
    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(self.it.none())
    }
    fn visit_bool<E>(self, b: bool) -> Result<Value, E> {
        Ok(Value::Bool(b))
    }
    fn visit_i64<E>(self, n: i64) -> Result<Value, E> {
        Ok(Value::Int(n))
    }
    fn visit_u64<E>(self, n: u64) -> Result<Value, E> {
        Ok(i64::try_from(n).map(Value::Int).unwrap_or(Value::Float(n as f64)))
    }
    fn visit_f64<E>(self, f: f64) -> Result<Value, E> {
        Ok(Value::Float(f))
    }
    fn visit_str<E>(self, s: &str) -> Result<Value, E> {
        Ok(Value::text(s))
    }
    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(4));
        while let Some(x) = seq.next_element_seed(Seed { it: self.it })? {
            items.push(x);
        }
        Ok(Value::list(items))
    }
    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut keys: Vec<Rc<str>> = Vec::new();
        let mut vals = Vec::new();
        while let Some(k) = map.next_key::<std::borrow::Cow<'de, str>>()? {
            let v = map.next_value_seed(Seed { it: self.it })?;
            // A key given twice keeps its last value, like JSON parsers do.
            match keys.iter().position(|x| **x == *k) {
                Some(i) => vals[i] = v,
                None => {
                    keys.push(key(&k));
                    vals.push(v);
                }
            }
        }
        Ok(Value::Record(Rc::new(Record::with_shape(shape(keys), vals))))
    }
}

thread_local! {
    /// Keys and shapes met before: the same JSON objects come again and again.
    static KEYS: std::cell::RefCell<std::collections::HashMap<Box<str>, Rc<str>>> = Default::default();
    static SHAPES: std::cell::RefCell<std::collections::HashMap<Vec<Rc<str>>, Rc<rythme::value::Shape>>> = Default::default();
}

fn key(k: &str) -> Rc<str> {
    KEYS.with(|m| {
        let mut m = m.borrow_mut();
        if let Some(r) = m.get(k) {
            return r.clone();
        }
        let r: Rc<str> = Rc::from(k);
        if m.len() < 4096 {
            m.insert(Box::from(k), r.clone());
        }
        r
    })
}

fn shape(keys: Vec<Rc<str>>) -> Rc<rythme::value::Shape> {
    SHAPES.with(|m| {
        let mut m = m.borrow_mut();
        if let Some(s) = m.get(&keys) {
            return s.clone();
        }
        let s = Rc::new(rythme::value::Shape::new(keys.clone()));
        if m.len() < 1024 {
            m.insert(keys, s.clone());
        }
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let it = Interp::new();
        let text = r#"{"a":1,"b":[true,null,2.5,"x\"y\n"],"c":{"d":"\u00e9"},"a":3}"#.as_bytes();
        let v = parse_json(&it, text).unwrap();
        let mut out = String::new();
        write_json(&v, &mut out);
        assert_eq!(out, r#"{"a":3,"b":[true,null,2.5,"x\"y\n"],"c":{"d":"é"}}"#);
        assert!(parse_json(&it, b"{").is_err());
        assert!(parse_json(&it, b"1 2").is_err());
        let mut f = String::new();
        write_json(&Value::Float(2.0), &mut f);
        assert_eq!(f, "2.0");
    }
}
