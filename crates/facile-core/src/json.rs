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
    // The characters to escape are ASCII: the text between them is copied
    // in one go (cutting at their bytes keeps whole characters).
    let mut start = 0;
    for (i, b) in s.bytes().enumerate() {
        let esc = match b {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            b'"' => "&quot;",
            b'\'' => "&#39;",
            _ => continue,
        };
        out.push_str(&s[start..i]);
        out.push_str(esc);
        start = i + 1;
    }
    out.push_str(&s[start..]);
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
    match v {
        Value::Unit => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        // The same digits as `{n}`, without the machinery of `fmt`.
        Value::Int(n) => out.push_str(itoa::Buffer::new().format(*n)),
        Value::Big(n) => write_str(&n.to_string(), out),
        Value::Float(f) if f.is_finite() => write_float(*f, out),
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

/// A finite float as `{f}` writes it, with `.0` after a whole number below 1e16.
///
/// `zmij` finds the same shortest digits as `fmt` about three times faster,
/// and writes them the same way between 1e-5 and 1e16 (`.0` included). The
/// two differ only on a tie (the exact value halfway between two shortest
/// texts: `fmt` rounds it up, `zmij` to even), and a tie needs at least 16
/// significant digits: with 15 or fewer, the text of `zmij` is kept, the
/// others go through `fmt` as before.
fn write_float(f: f64, out: &mut String) {
    if (1e-5..1e16).contains(&f.abs()) {
        let mut buf = zmij::Buffer::new();
        let text = buf.format_finite(f);
        let b = text.as_bytes();
        // Significant digits, counted from the first one that is not a zero
        // (trailing zeros counted too: at worst, a few more go through `fmt`).
        let lead = b.iter().take_while(|&&c| c == b'-' || c == b'0' || c == b'.').count();
        let digits = b.len() - lead - b[lead..].contains(&b'.') as usize;
        if digits <= 15 {
            out.push_str(text);
            return;
        }
    }
    use std::fmt::Write;
    let _ = write!(out, "{f}");
    if f.fract() == 0.0 && f.abs() < 1e16 {
        out.push_str(".0");
    }
}

const LO: u64 = 0x0101_0101_0101_0101;
const HI: u64 = 0x8080_8080_8080_8080;

/// The high bit of each byte of `x` that is zero, and maybe of the bytes
/// above it: not zero exactly when one byte of `x` is zero.
#[inline]
fn zero_bytes(x: u64) -> u64 {
    x.wrapping_sub(LO) & !x & HI
}

/// Whether one of the 8 bytes of `w` is below 0x20, a `"` or a `\` (the
/// classic test for a byte below a value, exact for "one of them" like
/// `zero_bytes`).
#[inline]
fn word_needs_escape(w: u64) -> bool {
    let below = w.wrapping_sub(LO * 0x20) & !w & HI;
    (below | zero_bytes(w ^ (LO * b'"' as u64)) | zero_bytes(w ^ (LO * b'\\' as u64))) != 0
}

fn write_str(s: &str, out: &mut String) {
    out.push('"');
    let bytes = s.as_bytes();
    // Most texts have nothing to escape: looked at 8 bytes at a time, then
    // written in one go.
    let mut clean = 0;
    while let Some(chunk) = bytes.get(clean..clean + 8) {
        if word_needs_escape(u64::from_le_bytes(chunk.try_into().unwrap_or_default())) {
            break;
        }
        clean += 8;
    }
    if !bytes[clean..].iter().any(|&b| b < 0x20 || b == b'"' || b == b'\\') {
        out.push_str(s);
        out.push('"');
        return;
    }
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate().skip(clean) {
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
        let mut vals = rythme::value::Few::<4>::none();
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
        Ok(Value::Record(Rc::new(crate::util::gathered(shape(keys), vals))))
    }
}

thread_local! {
    /// Keys and shapes met before: the same JSON objects come again and again.
    static KEYS: std::cell::RefCell<rustc_hash::FxHashMap<Box<str>, Rc<str>>> = Default::default();
    static SHAPES: std::cell::RefCell<rustc_hash::FxHashMap<Vec<Rc<str>>, Rc<rythme::value::Shape>>> = Default::default();
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

    /// The texts written as `serde_json` writes them, escapes included, at
    /// every place in the words of 8 bytes.
    #[test]
    fn texts_as_serde_writes_them() {
        let specials = [
            "\"", "\\", "\n", "\r", "\t", "\u{0}", "\u{1f}", "\u{7f}", " ", "!", "#", "[", "]", "é", "€", "😀", "a", "\u{a2}", "\u{dc}",
            "\u{9c}",
        ];
        let mut texts = vec![String::new()];
        for len in 0..20 {
            for (k, sp) in specials.iter().enumerate() {
                for at in 0..=len {
                    let mut t: String = (0..len).map(|i| (b'a' + ((i + k) % 26) as u8) as char).collect();
                    t.insert_str(at, sp);
                    texts.push(t);
                }
            }
        }
        for t in texts {
            let mut out = String::new();
            write_str(&t, &mut out);
            assert_eq!(out, serde_json::to_string(&t).unwrap(), "{t:?}");
        }
        for w in [
            0u64,
            u64::MAX,
            0x2020_2020_2020_2020,
            0x1f20_2020_2020_2020,
            0x2121_2121_2121_2122,
            0x5d5d_5c5d_5d5d_5d5d,
            0x8080_8080_8080_80a2,
        ] {
            let bytes = w.to_le_bytes();
            assert_eq!(word_needs_escape(w), bytes.iter().any(|&b| b < 0x20 || b == b'"' || b == b'\\'), "{w:x}");
        }
    }

    #[test]
    fn floats_as_fmt_writes_them() {
        let old = |f: f64| {
            let mut s = format!("{f}");
            if f.fract() == 0.0 && f.abs() < 1e16 {
                s.push_str(".0");
            }
            s
        };
        let mut values =
            vec![0.0, -0.0, 1e-5, 1e16, 1e15, 9999999999999998.0, 0.1, 0.3, 0.1 + 0.2, 1.0 / 3.0, 5e-324, f64::MAX, f64::MIN_POSITIVE];
        for e in -30..=30 {
            values.push(10f64.powi(e));
        }
        // Halves, quarters, ... of large numbers: the ties where `fmt` and `zmij` differ.
        for i in 0..20_000u64 {
            let k = i.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 11;
            for j in [0, 1, 2, 3, 6, 10, 20, 30, 40, 50, 60] {
                values.push(k as f64 / (1u64 << j) as f64);
            }
            values.push(i as f64 * 1.5);
            values.push(i as f64 / 100.0);
            values.push(i as f64 / 7.0);
        }
        let mut x: u64 = 0x2545_F491_4F6C_DD1D;
        for _ in 0..200_000 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let exp = 1023 - 20 + (x >> 52) % 80;
            values.push(f64::from_bits((x & ((1 << 52) - 1)) | (exp << 52) | (x & (1 << 63))));
        }
        let mut out = String::new();
        for v in values {
            for f in [v, -v, f64::from_bits(v.to_bits() + 1), f64::from_bits(v.to_bits().saturating_sub(1))] {
                if f.is_finite() {
                    out.clear();
                    write_float(f, &mut out);
                    assert_eq!(out, old(f), "{f:e}");
                }
            }
        }
    }

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
        // Integers at the edges, objects of more than four keys, a key given twice.
        let text = r#"[-9223372036854775808,9223372036854775807,0,-1,{"a":1,"b":2,"c":3,"d":4,"e":5,"a":6},{"x":{}}]"#;
        let v = parse_json(&it, text.as_bytes()).unwrap();
        let mut out = String::new();
        write_json(&v, &mut out);
        assert_eq!(out, r#"[-9223372036854775808,9223372036854775807,0,-1,{"a":6,"b":2,"c":3,"d":4,"e":5},{"x":{}}]"#);
    }
}
