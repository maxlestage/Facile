//! The ORM in Rythme: models and queries are values, `db` runs them.
//!
//! ```rythme
//! taches = model("taches", {
//!   titre: field(:text, {required: true, max: 120}),
//!   fait: field(:bool, {default: false}),
//!   projet: belongs_to("projets"),
//! })
//!
//! fn a_faire() = taches |> where(:fait, false) |> order_by(:created_at, :desc) |> db.all
//! ```

use std::cell::{Cell as StdCell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use facile_core::host;
use facile_core::json::{from_json, to_json_text};
use facile_core::util::{self, error, field, func_arg, int_arg, mark_impure, name_arg, name_of, record_arg, text_arg, tr, variant_name};
use rythme::builtins::Builtin;
use rythme::diagnostic::Fallible;
use rythme::interp::Interp;
use rythme::source::Span;
use rythme::symbol::Sym;
use rythme::value::{List, Record, Shape, Value};

use super::model::{model_named, model_of, valid_name, Field, Kind, Model};
use super::query::{cell_of, Cond, Filter, Op, Query};
use super::Cell;

// ----- models -----------------------------------------------------------

const QUERY_LISTS: [&str; 4] = ["wheres", "order", "includes", "pick"];

fn kind_name(v: &Value) -> Option<String> {
    name_of(v).filter(|k| Kind::parse(k).is_some())
}

fn bad_kind(f: &str, span: Span) -> Box<rythme::diagnostic::Diagnostic> {
    error(
        f,
        tr(
            "a field is a kind (:text, :int, :float, :bool, :date, :datetime, :json), `field(kind, options)`, `belongs_to(\"table\")` or `has_many(\"table\", \"field\")`",
            "un champ est une sorte (:text, :int, :float, :bool, :date, :datetime, :json), `field(sorte, options)`, `belongs_to(\"table\")` ou `has_many(\"table\", \"champ\")`",
        ),
        span,
    )
}

fn model(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let table = text_arg(a, 0, "model", span)?;
    if !valid_name(table) {
        return Err(error(
            "model",
            tr("a table name is lowercase letters, digits and `_`", "un nom de table s'écrit en minuscules, chiffres et `_`"),
            span,
        ));
    }
    let fields = record_arg(a, 1, "model", span)?;
    let mut out = Vec::new();
    for (k, v) in fields.iter() {
        if !valid_name(k) || matches!(&**k, "id" | "created_at" | "updated_at") {
            return Err(error(
                "model",
                tr(
                    &format!("`{k}` cannot be a field: lowercase letters, digits and `_`, and not id, created_at or updated_at"),
                    &format!("`{k}` ne peut pas être un champ : minuscules, chiffres et `_`, et pas id, created_at ni updated_at"),
                ),
                span,
            ));
        }
        let f = match v {
            Value::Symbol(_) | Value::Text(_) => {
                let kind = kind_name(v).ok_or_else(|| bad_kind("model", span))?;
                util::record(vec![("name", Value::text(k)), ("kind", util::sym(&kind))])
            }
            Value::Record(r) if r.get("belongs_to").is_some() => {
                let t = r.get("belongs_to").map(|t| t.display()).unwrap_or_default();
                let mut f = (**r).clone();
                f.set(Rc::from("name"), Value::text(&format!("{k}_id")));
                f.set(Rc::from("kind"), util::sym("int"));
                f.set(Rc::from("refers"), Value::text(&t));
                f.set(Rc::from("relation"), Value::text(k));
                Value::Record(Rc::new(f))
            }
            Value::Record(r) if r.get("has_many").is_some() => util::with_field(r, "name", Value::text(k)),
            Value::Record(r) if r.get("kind").is_some() => util::with_field(r, "name", Value::text(k)),
            _ => return Err(bad_kind("model", span)),
        };
        out.push(f);
    }
    let opts = match a.get(2) {
        Some(Value::Record(r)) => Some(r.clone()),
        _ => None,
    };
    let timestamps = !matches!(opts.as_ref().and_then(|o| o.get("timestamps").cloned()), Some(Value::Bool(false)));
    let mut fields = vec![
        ("facile", util::sym("model")),
        ("table", Value::text(table)),
        ("fields", Value::list(out)),
        ("timestamps", Value::Bool(timestamps)),
    ];
    for l in QUERY_LISTS {
        fields.push((l, Value::list(Vec::new())));
    }
    fields.push(("limit", Value::Unit));
    fields.push(("offset", Value::Unit));
    let v = util::record(fields);
    // Known by its table from now on (relations find it there).
    super::model::forget(table);
    model_of(&v).map_err(|e| error("model", e, span))?;
    Ok(v)
}

fn field_fn(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let kind = kind_name(&a[0]).ok_or_else(|| bad_kind("field", span))?;
    let mut r = match a.get(1) {
        Some(Value::Record(o)) => (**o).clone(),
        Some(_) => {
            return Err(error(
                "field",
                tr(
                    "the options are a record: `{required: true, max: 80}`",
                    "les options sont un enregistrement : `{required: true, max: 80}`",
                ),
                span,
            ))
        }
        None => Record::new(),
    };
    r.set(Rc::from("kind"), util::sym(&kind));
    Ok(Value::Record(Rc::new(r)))
}

fn belongs_to(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let t = text_arg(a, 0, "belongs_to", span)?;
    let mut r = match a.get(1) {
        Some(Value::Record(o)) => (**o).clone(),
        _ => Record::new(),
    };
    r.set(Rc::from("belongs_to"), Value::text(t));
    Ok(Value::Record(Rc::new(r)))
}

fn has_many(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let t = text_arg(a, 0, "has_many", span)?;
    let f = name_arg(a, 1, "has_many", span)?;
    let foreign = if f.ends_with("_id") { f } else { format!("{f}_id") };
    Ok(util::record(vec![("has_many", Value::text(t)), ("foreign", Value::text(&foreign))]))
}

// ----- queries ----------------------------------------------------------

fn query_arg<'a>(a: &'a [Value], f: &str, span: Span) -> Fallible<&'a Record> {
    let r = record_arg(a, 0, f, span)?;
    if r.get("facile").and_then(name_of).as_deref() != Some("model") {
        return Err(error(
            f,
            tr(
                "the first argument is a model or a query (`taches |> where(...)`)",
                "le premier argument est un modèle ou une requête (`taches |> where(...)`)",
            ),
            span,
        ));
    }
    Ok(r)
}

fn push(r: &Record, key: &str, item: Value) -> Value {
    let list = match r.get(key) {
        Some(Value::List(l)) => l.push(item),
        _ => List::new(vec![item]),
    };
    util::with_field(r, key, Value::List(list))
}

fn check_field(r: &Record, f: &str, name: &str, span: Span) -> Fallible<()> {
    let m = model_of(&Value::Record(Rc::new(r.clone()))).map_err(|e| error(f, e, span))?;
    if m.kind_of(name).is_none() {
        return Err(error(
            f,
            tr(
                &format!("`{}` has no field `{name}` (fields: {})", m.table, m.columns().join(", ")),
                &format!("`{}` n'a pas de champ `{name}` (champs : {})", m.table, m.columns().join(", ")),
            ),
            span,
        ));
    }
    Ok(())
}

fn condition(f: &str, a: &[Value], span: Span) -> Fallible<Value> {
    let name = name_arg(a, 0, f, span)?;
    let (op, value) = match a.len() {
        2 => ("=".to_string(), a[1].clone()),
        3 => (name_arg(a, 1, f, span)?, a[2].clone()),
        _ => {
            return Err(error(
                f,
                tr(
                    "a condition is (field, value) or (field, operator, value)",
                    "une condition est (champ, valeur) ou (champ, opérateur, valeur)",
                ),
                span,
            ))
        }
    };
    if Op::parse(&op).is_none() {
        return Err(error(
            f,
            tr(
                &format!("unknown operator `{op}`: use =, !=, <, <=, >, >=, :contains, :starts_with, :ends_with, :in, :not_in"),
                &format!("opérateur inconnu `{op}` : utilisez =, !=, <, <=, >, >=, :contains, :starts_with, :ends_with, :in, :not_in"),
            ),
            span,
        ));
    }
    Ok(util::record(vec![("field", Value::text(&name)), ("op", Value::text(&op)), ("value", value)]))
}

/// `where(q, :fait, false)`, `where(q, :priorite, ">", 2)`, `where(q, :titre, :contains, "lait")`.
fn where_fn(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let q = query_arg(a, "where", span)?;
    let c = condition("where", &a[1..], span)?;
    check_field(q, "where", &field(&c, "field").map(|x| x.display()).unwrap_or_default(), span)?;
    Ok(push(q, "wheres", c))
}

/// `where_any(q, [[:titre, :contains, "x"], [:notes, :contains, "x"]])`: one of them is enough.
fn where_any(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let q = query_arg(a, "where_any", span)?;
    let Value::List(l) = &a[1] else {
        return Err(error(
            "where_any",
            tr("give a list of conditions: `[[:a, 1], [:b, \">\", 2]]`", "donnez une liste de conditions : `[[:a, 1], [:b, \">\", 2]]`"),
            span,
        ));
    };
    let mut conds = Vec::new();
    for c in l.iter() {
        let parts: Vec<Value> = match c {
            Value::List(p) => p.iter().cloned().collect(),
            _ => {
                return Err(error(
                    "where_any",
                    tr(
                        "each condition is a list `[field, value]` or `[field, operator, value]`",
                        "chaque condition est une liste `[champ, valeur]` ou `[champ, opérateur, valeur]`",
                    ),
                    span,
                ))
            }
        };
        let c = condition("where_any", &parts, span)?;
        check_field(q, "where_any", &field(&c, "field").map(|x| x.display()).unwrap_or_default(), span)?;
        conds.push(c);
    }
    Ok(push(q, "wheres", util::record(vec![("any", Value::list(conds))])))
}

/// `search(q, [:titre, :notes], "lait")`: the rows where one of the fields contains the text.
fn search(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let text = text_arg(a, 2, "search", span)?.trim().to_string();
    if text.is_empty() {
        return Ok(a[0].clone());
    }
    let fields: Vec<String> = match &a[1] {
        Value::List(l) => l.iter().filter_map(name_of).collect(),
        other => name_of(other).into_iter().collect(),
    };
    let conds = fields.iter().map(|f| Value::list(vec![Value::text(f), util::sym("contains"), Value::text(&text)])).collect();
    where_any(it, &[a[0].clone(), Value::list(conds)], span)
}

fn order_by(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let q = query_arg(a, "order_by", span)?;
    let f = name_arg(a, 1, "order_by", span)?;
    check_field(q, "order_by", &f, span)?;
    let dir = match a.get(2) {
        Some(d) => name_of(d).unwrap_or_default(),
        None => "asc".into(),
    };
    if dir != "asc" && dir != "desc" {
        return Err(error("order_by", tr("the order is :asc or :desc", "l'ordre est :asc ou :desc"), span));
    }
    Ok(push(q, "order", util::record(vec![("field", Value::text(&f)), ("dir", util::sym(&dir))])))
}

fn limit(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let q = query_arg(a, "limit", span)?;
    let n = int_arg(a, 1, "limit", span)?;
    Ok(util::with_field(q, "limit", Value::Int(n.max(0))))
}

fn offset(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let q = query_arg(a, "offset", span)?;
    let n = int_arg(a, 1, "offset", span)?;
    Ok(util::with_field(q, "offset", Value::Int(n.max(0))))
}

fn include(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let q = query_arg(a, "include", span)?;
    let rel = name_arg(a, 1, "include", span)?;
    let m = model_of(&a[0]).map_err(|e| error("include", e, span))?;
    let known = m.fields.iter().any(|f| f.refers.as_ref().is_some_and(|(_, r)| *r == rel)) || m.has_many.iter().any(|h| h.name == rel);
    if !known {
        return Err(error(
            "include",
            tr(
                &format!("`{}` has no relation `{rel}`: declare it with `belongs_to` or `has_many`", m.table),
                &format!("`{}` n'a pas de relation `{rel}` : déclarez-la avec `belongs_to` ou `has_many`", m.table),
            ),
            span,
        ));
    }
    Ok(push(q, "includes", Value::text(&rel)))
}

fn pick(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let q = query_arg(a, "pick", span)?;
    let Value::List(l) = &a[1] else {
        return Err(error("pick", tr("give a list of fields: `[:id, :titre]`", "donnez une liste de champs : `[:id, :titre]`"), span));
    };
    let mut out = Value::Record(Rc::new(q.clone()));
    for f in l.iter() {
        let name = name_of(f).unwrap_or_default();
        check_field(q, "pick", &name, span)?;
        let Value::Record(cur) = &out else { unreachable!() };
        out = push(cur, "pick", Value::text(&name));
    }
    Ok(out)
}

// ----- rows as values ---------------------------------------------------

thread_local! {
    static SHAPES: RefCell<HashMap<String, Rc<Shape>>> = RefCell::new(HashMap::new());
    static DEPTH: StdCell<usize> = const { StdCell::new(0) };
}

fn shape_of(m: &Model) -> Rc<Shape> {
    SHAPES.with(|s| {
        s.borrow_mut()
            .entry(m.table.clone())
            .or_insert_with(|| Rc::new(Shape::new(m.columns().iter().map(|c| Rc::from(c.as_str())).collect())))
            .clone()
    })
}

pub fn forget_shape(table: &str) {
    SHAPES.with(|s| s.borrow_mut().remove(table));
}

fn value_of_cell(it: &Interp, c: &Cell, kind: Option<Kind>) -> Value {
    match (c, kind) {
        (Cell::Null, _) => it.none(),
        (Cell::Text(t), Some(Kind::Json)) => match serde_json::from_str::<serde_json::Value>(t) {
            Ok(j) => from_json(it, &j),
            Err(_) => Value::text(t),
        },
        (Cell::Int(n), Some(Kind::Bool)) => Value::Bool(*n != 0),
        (Cell::Int(n), Some(Kind::Float)) => Value::Float(*n as f64),
        (Cell::Int(n), _) => Value::Int(*n),
        (Cell::Float(f), _) => Value::Float(*f),
        (Cell::Bool(b), _) => Value::Bool(*b),
        (Cell::Text(t), _) => Value::text(t),
    }
}

/// A row as a record: `{id: 1, titre: "Lait", fait: false, ...}`.
pub fn row_value(it: &Interp, m: &Model, row: &[Cell]) -> Value {
    let cols = m.columns();
    let vals = row.iter().zip(cols.iter()).map(|(c, col)| value_of_cell(it, c, m.kind_of(col))).collect();
    Value::Record(Rc::new(Record::with_shape(shape_of(m), vals)))
}

/// The rows of a query, with their relations when it includes some.
pub fn rows(it: &mut Interp, qv: &Value, span: Span) -> Fallible<Vec<Value>> {
    mark_impure();
    let m = model_of(qv).map_err(|e| error("db", e, span))?;
    let q = Query::from_value(qv, &m).map_err(|e| error("db", e, span))?;
    let rows = super::select(&m, &q).map_err(|e| error("db", e, span))?;
    let mut values: Vec<Value> = rows.iter().map(|r| row_value(it, &m, r)).collect();
    for rel in &q.includes {
        values = attach(it, &m, rel, values, span)?;
    }
    if !q.pick.is_empty() {
        values = values
            .into_iter()
            .map(|v| {
                let mut r = Record::new();
                for p in &q.pick {
                    if let Some(x) = field(&v, p) {
                        r.set(Rc::from(p.as_str()), x.clone());
                    }
                }
                for rel in &q.includes {
                    if let Some(x) = field(&v, rel) {
                        r.set(Rc::from(rel.as_str()), x.clone());
                    }
                }
                Value::Record(Rc::new(r))
            })
            .collect();
    }
    Ok(values)
}

fn id_of(v: &Value) -> Option<i64> {
    match v {
        Value::Int(n) => Some(*n),
        Value::Record(r) => match r.get("id") {
            Some(Value::Int(n)) => Some(*n),
            _ => None,
        },
        Value::Variant(var) if variant_name(v).as_deref() == Some("Some") => id_of(&var.fields[0]),
        _ => None,
    }
}

fn related(table: &str, span: Span) -> Fallible<Rc<Model>> {
    model_named(table).ok_or_else(|| {
        error(
            "include",
            tr(&format!("the model of the table `{table}` is not defined"), &format!("le modèle de la table `{table}` n'est pas défini")),
            span,
        )
    })
}

/// Adds the relation `rel` to each row: the row it belongs to, or the rows it has.
fn attach(it: &mut Interp, m: &Model, rel: &str, values: Vec<Value>, span: Span) -> Fallible<Vec<Value>> {
    if let Some(f) = m.fields.iter().find(|f| f.refers.as_ref().is_some_and(|(_, r)| r == rel)) {
        let (table, _) = f.refers.clone().unwrap_or_default();
        let other = related(&table, span)?;
        let ids: Vec<Cell> = values.iter().filter_map(|v| field(v, &f.name).and_then(id_of)).map(Cell::Int).collect();
        let q = Query {
            filters: vec![Filter::One(Cond { column: "id".into(), op: Op::In, value: Cell::Null, values: ids })],
            ..Query::default()
        };
        let found = super::select(&other, &q).map_err(|e| error("include", e, span))?;
        let by_id: HashMap<i64, Value> = found
            .iter()
            .filter_map(|r| match r.first() {
                Some(Cell::Int(id)) => Some((*id, row_value(it, &other, r))),
                _ => None,
            })
            .collect();
        return Ok(values
            .into_iter()
            .map(|v| {
                let target = field(&v, &f.name).and_then(id_of).and_then(|id| by_id.get(&id).cloned());
                let x = it.option(target);
                match &v {
                    Value::Record(r) => util::with_field(r, rel, x),
                    _ => v,
                }
            })
            .collect());
    }
    if let Some(h) = m.has_many.iter().find(|h| h.name == rel) {
        let other = related(&h.table, span)?;
        if other.kind_of(&h.foreign).is_none() {
            return Err(error(
                "include",
                tr(
                    &format!("`{}` has no field `{}`", other.table, h.foreign),
                    &format!("`{}` n'a pas de champ `{}`", other.table, h.foreign),
                ),
                span,
            ));
        }
        let ids: Vec<Cell> = values.iter().filter_map(|v| field(v, "id").and_then(id_of)).map(Cell::Int).collect();
        let q = Query {
            filters: vec![Filter::One(Cond { column: h.foreign.clone(), op: Op::In, value: Cell::Null, values: ids })],
            ..Query::default()
        };
        let found = super::select(&other, &q).map_err(|e| error("include", e, span))?;
        let fk = other.columns().iter().position(|c| *c == h.foreign).unwrap_or(0);
        let mut groups: HashMap<i64, Vec<Value>> = HashMap::new();
        for r in &found {
            if let Some(Cell::Int(p)) = r.get(fk) {
                groups.entry(*p).or_default().push(row_value(it, &other, r));
            }
        }
        return Ok(values
            .into_iter()
            .map(|v| {
                let kids = field(&v, "id").and_then(id_of).and_then(|id| groups.remove(&id)).unwrap_or_default();
                match &v {
                    Value::Record(r) => util::with_field(r, rel, Value::list(kids)),
                    _ => v,
                }
            })
            .collect());
    }
    Ok(values)
}

// ----- validation -------------------------------------------------------

fn valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && s[..4].parse::<u32>().is_ok()
        && matches!(s[5..7].parse::<u32>(), Ok(1..=12))
        && matches!(s[8..10].parse::<u32>(), Ok(1..=31))
}

fn valid_datetime(s: &str) -> bool {
    s.len() >= 16 && valid_date(&s[..10]) && matches!(s.as_bytes()[10], b'T' | b' ')
}

fn valid_email(s: &str) -> bool {
    match s.split_once('@') {
        Some((a, b)) => !a.is_empty() && b.contains('.') && !b.starts_with('.') && !b.ends_with('.') && !s.contains(char::is_whitespace),
        None => false,
    }
}

fn is_blank(v: &Value) -> bool {
    match v {
        Value::Unit => true,
        Value::Text(t) => t.trim().is_empty(),
        Value::Variant(_) => variant_name(v).as_deref() == Some("None"),
        _ => false,
    }
}

/// Checks a record for a model: the columns and cells to write, or the
/// messages by field. `partial` for changes (missing fields are not required).
/// The messages of a record that does not follow the rules, by field.
type Invalid = Vec<(String, String)>;

fn prepare(m: &Model, input: &Record, partial: bool, current_id: Option<i64>) -> Result<(Vec<String>, Vec<Cell>), Invalid> {
    let mut cols = Vec::new();
    let mut cells = Vec::new();
    let mut errors: Vec<(String, String)> = Vec::new();
    for (k, _) in input.iter() {
        let known = m.field(k).is_some()
            || matches!(&**k, "id" | "created_at" | "updated_at")
            || m.fields.iter().any(|f| f.refers.as_ref().is_some_and(|(_, r)| **r == **k))
            || m.has_many.iter().any(|h| *h.name == **k);
        if !known {
            errors.push((k.to_string(), tr("is not a field of this model", "n'est pas un champ de ce modèle")));
        }
    }
    for f in &m.fields {
        let relation = f.refers.as_ref().map(|(_, r)| r.as_str());
        let given = input.get(&f.name).or_else(|| relation.and_then(|r| input.get(r)));
        let value = match given {
            Some(v) if !is_blank(v) => Some(v.clone()),
            Some(_) if !partial || f.required => None,
            Some(_) => {
                cols.push(f.name.clone());
                cells.push(Cell::Null);
                continue;
            }
            None if partial => continue,
            None => f.default.clone(),
        };
        let Some(value) = value else {
            if f.required {
                errors.push((f.name.clone(), tr("is required", "est obligatoire")));
            } else if given.is_some() || !partial {
                cols.push(f.name.clone());
                cells.push(Cell::Null);
            }
            continue;
        };
        match check_value(f, &value) {
            Ok(c) => {
                if f.unique && unique_taken(m, f, &c, current_id) {
                    errors.push((f.name.clone(), tr("is already taken", "est déjà pris")));
                }
                cols.push(f.name.clone());
                cells.push(c);
            }
            Err(e) => errors.push((f.name.clone(), e)),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    if m.timestamps {
        let now = super::now_iso();
        if !partial {
            cols.push("created_at".into());
            cells.push(Cell::Text(now.clone()));
        }
        cols.push("updated_at".into());
        cells.push(Cell::Text(now));
    }
    Ok((cols, cells))
}

fn check_value(f: &Field, v: &Value) -> Result<Cell, String> {
    let wrong = || {
        tr(
            &format!("must be of kind {} (got {})", f.kind.name(), util::short(v)),
            &format!("doit être de sorte {} (reçu {})", f.kind.name(), util::short(v)),
        )
    };
    let v = match v {
        Value::Variant(var) if variant_name(v).as_deref() == Some("Some") => var.fields[0].clone(),
        Value::Record(_) if f.refers.is_some() => match id_of(v) {
            Some(id) => Value::Int(id),
            None => return Err(wrong()),
        },
        other => other.clone(),
    };
    let cell = if f.kind == Kind::Json { Cell::Text(to_json_text(&v)) } else { cell_of(&v).coerce(f.kind).ok_or_else(wrong)? };
    match (&cell, f.kind) {
        (Cell::Text(t), Kind::Date) if !valid_date(t) => {
            return Err(tr("must be a date like 2026-10-19", "doit être une date comme 2026-10-19"))
        }
        (Cell::Text(t), Kind::DateTime) if !valid_datetime(t) => {
            return Err(tr(
                "must be a date and time like 2026-10-19T08:30:00Z",
                "doit être une date et une heure comme 2026-10-19T08:30:00Z",
            ))
        }
        _ => {}
    }
    let size = match &cell {
        Cell::Int(n) => Some(*n as f64),
        Cell::Float(x) => Some(*x),
        Cell::Text(t) if f.kind == Kind::Text => Some(t.chars().count() as f64),
        _ => None,
    };
    let what = if f.kind == Kind::Text { tr(" characters", " caractères") } else { String::new() };
    if let (Some(min), Some(s)) = (f.min, size) {
        if s < min {
            return Err(tr(&format!("must be at least {min}{what}"), &format!("doit valoir au moins {min}{what}")));
        }
    }
    if let (Some(max), Some(s)) = (f.max, size) {
        if s > max {
            return Err(tr(&format!("must be at most {max}{what}"), &format!("doit valoir au plus {max}{what}")));
        }
    }
    if f.email {
        if let Cell::Text(t) = &cell {
            if !valid_email(t) {
                return Err(tr("must be an email address", "doit être une adresse e-mail"));
            }
        }
    }
    if !f.one_of.is_empty() && !f.one_of.iter().any(|o| cell_of(o).coerce(f.kind).as_ref() == Some(&cell)) {
        let all: Vec<String> = f.one_of.iter().map(|o| o.display()).collect();
        return Err(tr(&format!("must be one of: {}", all.join(", ")), &format!("doit être l'un de : {}", all.join(", "))));
    }
    Ok(cell)
}

fn unique_taken(m: &Model, f: &Field, c: &Cell, current: Option<i64>) -> bool {
    let mut filters = vec![Filter::One(Cond { column: f.name.clone(), op: Op::Eq, value: c.clone(), values: vec![] })];
    if let Some(id) = current {
        filters.push(Filter::One(Cond { column: "id".into(), op: Op::Ne, value: Cell::Int(id), values: vec![] }));
    }
    let q = Query { filters, ..Query::default() };
    matches!(super::aggregate(m, &q, "count", "*"), Ok(Cell::Int(n)) if n > 0)
}

/// `Err({status: 422, error: "...", fields: {titre: "est obligatoire"}})`.
fn invalid(it: &Interp, errors: Vec<(String, String)>) -> Value {
    let mut fields = Record::new();
    for (k, v) in &errors {
        fields.set(Rc::from(k.as_str()), Value::text(v));
    }
    let summary = errors.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", ");
    it.err(util::record(vec![("status", Value::Int(422)), ("error", Value::text(&summary)), ("fields", Value::Record(Rc::new(fields)))]))
}

fn db_error(it: &Interp, msg: String) -> Value {
    let status = if msg.contains("unique") { 409 } else { 500 };
    it.err(util::record(vec![("status", Value::Int(status)), ("error", Value::text(&msg))]))
}

// ----- db.* -------------------------------------------------------------

fn model_arg(a: &[Value], f: &str, span: Span) -> Fallible<Rc<Model>> {
    query_arg(a, f, span)?;
    model_of(&a[0]).map_err(|e| error(f, e, span))
}

fn by_id(id: i64) -> Query {
    Query { filters: vec![Filter::One(Cond { column: "id".into(), op: Op::Eq, value: Cell::Int(id), values: vec![] })], ..Query::default() }
}

fn db_all(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    query_arg(a, "db.all", span)?;
    Ok(Value::list(rows(it, &a[0], span)?))
}

fn db_first(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let q = query_arg(a, "db.first", span)?;
    let q = util::with_field(q, "limit", Value::Int(1));
    let first = rows(it, &q, span)?.into_iter().next();
    Ok(it.option(first))
}

fn db_find(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let q = query_arg(a, "db.find", span)?;
    let id = match &a[1] {
        Value::Int(n) => *n,
        Value::Text(t) => match t.trim().parse() {
            Ok(n) => n,
            Err(_) => return Ok(it.none()),
        },
        other => match id_of(other) {
            Some(n) => n,
            None => {
                return Err(error(
                    "db.find",
                    tr("an id is an Int (or a text of digits)", "un id est un Int (ou un texte de chiffres)"),
                    span,
                ))
            }
        },
    };
    let cond = util::record(vec![("field", Value::text("id")), ("op", Value::text("=")), ("value", Value::Int(id))]);
    let q = push(q, "wheres", cond);
    let Value::Record(qr) = &q else { unreachable!() };
    let q = util::with_field(qr, "limit", Value::Int(1));
    let first = rows(it, &q, span)?.into_iter().next();
    Ok(it.option(first))
}

fn aggregate_value(it: &Interp, c: Cell) -> Value {
    match c {
        Cell::Null => it.none(),
        Cell::Int(n) => Value::Int(n),
        Cell::Float(f) => Value::Float(f),
        Cell::Bool(b) => Value::Bool(b),
        Cell::Text(t) => Value::text(&t),
    }
}

fn aggregate(it: &mut Interp, a: &[Value], span: Span, func: &str) -> Fallible<Value> {
    mark_impure();
    let name = format!("db.{func}");
    let m = model_arg(a, &name, span)?;
    let q = Query::from_value(&a[0], &m).map_err(|e| error(&name, e, span))?;
    let column = if func == "count" {
        "*".to_string()
    } else {
        let c = name_arg(a, 1, &name, span)?;
        if m.kind_of(&c).is_none() {
            return Err(error(
                &name,
                tr(&format!("`{}` has no field `{c}`", m.table), &format!("`{}` n'a pas de champ `{c}`", m.table)),
                span,
            ));
        }
        c
    };
    let c = super::aggregate(&m, &q, func, &column).map_err(|e| error(&name, e, span))?;
    Ok(match (func, c) {
        ("count", Cell::Int(n)) => Value::Int(n),
        ("count", _) => Value::Int(0),
        (_, c) => aggregate_value(it, c),
    })
}

fn db_count(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    aggregate(it, a, span, "count")
}
fn db_sum(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    aggregate(it, a, span, "sum")
}
fn db_avg(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    aggregate(it, a, span, "avg")
}
fn db_min(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    aggregate(it, a, span, "min")
}
fn db_max(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    aggregate(it, a, span, "max")
}

fn db_exists(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    Ok(Value::Bool(matches!(aggregate(it, a, span, "count")?, Value::Int(n) if n > 0)))
}

/// `db.count_by(q, :statut)`: how many rows for each value of the field, as a Dict.
fn db_count_by(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let f = name_arg(a, 1, "db.count_by", span)?;
    let q = query_arg(a, "db.count_by", span)?;
    check_field(q, "db.count_by", &f, span)?;
    let all = rows(it, &a[0], span)?;
    let mut counts: Vec<(Value, i64)> = Vec::new();
    for r in all {
        let v = field(&r, &f).cloned().unwrap_or(Value::Unit);
        match counts.iter_mut().find(|(k, _)| k.equals(&v)) {
            Some(e) => e.1 += 1,
            None => counts.push((v, 1)),
        }
    }
    Ok(Value::dict(counts.into_iter().map(|(k, n)| (k, Value::Int(n))).collect()))
}

fn db_insert(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    mark_impure();
    let m = model_arg(a, "db.insert", span)?;
    let input = record_arg(a, 1, "db.insert", span)?;
    insert_one(it, &m, input, span)
}

fn insert_one(it: &mut Interp, m: &Model, input: &Record, span: Span) -> Fallible<Value> {
    match prepare(m, input, false, None) {
        Err(errors) => Ok(invalid(it, errors)),
        Ok((cols, cells)) => match super::insert(m, &cols, cells) {
            Ok(row) => {
                let v = row_value(it, m, &row);
                Ok(it.ok(v))
            }
            Err(e) => {
                let _ = span;
                Ok(db_error(it, e))
            }
        },
    }
}

fn db_insert_all(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    mark_impure();
    let m = model_arg(a, "db.insert_all", span)?;
    let Value::List(l) = &a[1] else {
        return Err(error("db.insert_all", tr("give a list of records", "donnez une liste d'enregistrements"), span));
    };
    let mut out = Vec::new();
    super::begin().map_err(|e| error("db.insert_all", e, span))?;
    for (i, r) in l.iter().enumerate() {
        let Value::Record(rec) = r else {
            let _ = super::finish(false);
            return Err(error(
                "db.insert_all",
                tr(&format!("element {i} is not a record"), &format!("l'élément {i} n'est pas un enregistrement")),
                span,
            ));
        };
        let v = insert_one(it, &m, rec, span)?;
        if variant_name(&v).as_deref() == Some("Err") {
            let _ = super::finish(false);
            return Ok(v);
        }
        out.push(util::payload(&v));
    }
    super::finish(true).map_err(|e| error("db.insert_all", e, span))?;
    Ok(it.ok(Value::list(out)))
}

fn id_arg(a: &[Value], i: usize, f: &str, span: Span) -> Fallible<i64> {
    match a.get(i) {
        Some(Value::Text(t)) => t.trim().parse().map_err(|_| error(f, tr("an id is a number", "un id est un nombre"), span)),
        Some(v) => id_of(v).ok_or_else(|| {
            error(f, tr("an id is an Int (or a record with an id)", "un id est un Int (ou un enregistrement avec un id)"), span)
        }),
        None => Err(error(f, "missing id", span)),
    }
}

fn update_one(it: &mut Interp, m: &Model, id: i64, input: &Record, partial: bool) -> Value {
    match prepare(m, input, partial, Some(id)) {
        Err(errors) => invalid(it, errors),
        Ok((cols, cells)) => match super::update(m, &by_id(id), &cols, cells) {
            Ok(0) => it.err(util::sym("not_found")),
            Ok(_) => match super::select(m, &by_id(id)) {
                Ok(rows) => match rows.first() {
                    Some(r) => {
                        let v = row_value(it, m, r);
                        it.ok(v)
                    }
                    None => it.err(util::sym("not_found")),
                },
                Err(e) => db_error(it, e),
            },
            Err(e) => db_error(it, e),
        },
    }
}

fn db_update(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    mark_impure();
    let m = model_arg(a, "db.update", span)?;
    let id = id_arg(a, 1, "db.update", span)?;
    let input = record_arg(a, 2, "db.update", span)?;
    Ok(update_one(it, &m, id, input, true))
}

/// `db.save(taches, tache)`: inserts a record without id, updates one with an id.
fn db_save(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    mark_impure();
    let m = model_arg(a, "db.save", span)?;
    let input = record_arg(a, 1, "db.save", span)?;
    match input.get("id").and_then(id_of) {
        Some(id) => {
            let rest = input.without("id");
            let rest = rest.without("created_at").without("updated_at");
            Ok(update_one(it, &m, id, &rest, true))
        }
        None => insert_one(it, &m, input, span),
    }
}

fn db_update_where(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    mark_impure();
    let m = model_arg(a, "db.update_where", span)?;
    let q = Query::from_value(&a[0], &m).map_err(|e| error("db.update_where", e, span))?;
    let input = record_arg(a, 1, "db.update_where", span)?;
    if m.fields.iter().any(|f| f.unique && input.get(&f.name).is_some()) {
        return Err(error(
            "db.update_where",
            tr(
                "a unique field cannot be given the same value on several rows",
                "un champ unique ne peut pas recevoir la même valeur sur plusieurs lignes",
            ),
            span,
        ));
    }
    match prepare(&m, input, true, None) {
        Err(errors) => Ok(invalid(it, errors)),
        Ok((cols, cells)) => match super::update(&m, &q, &cols, cells) {
            Ok(n) => Ok(it.ok(Value::Int(n))),
            Err(e) => Ok(db_error(it, e)),
        },
    }
}

fn db_delete(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    mark_impure();
    let m = model_arg(a, "db.delete", span)?;
    let id = id_arg(a, 1, "db.delete", span)?;
    let _ = it;
    let n = super::delete(&m, &by_id(id)).map_err(|e| error("db.delete", e, span))?;
    Ok(Value::Bool(n > 0))
}

fn db_delete_where(_: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    mark_impure();
    let m = model_arg(a, "db.delete_where", span)?;
    let q = Query::from_value(&a[0], &m).map_err(|e| error("db.delete_where", e, span))?;
    let n = super::delete(&m, &q).map_err(|e| error("db.delete_where", e, span))?;
    Ok(Value::Int(n))
}

/// `db.paginate(q, page, per_page)`: `{items, page, per_page, total, pages}`.
fn db_paginate(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let q = query_arg(a, "db.paginate", span)?;
    let page = int_arg(a, 1, "db.paginate", span)?.max(1);
    let per = match a.get(2) {
        Some(_) => int_arg(a, 2, "db.paginate", span)?.clamp(1, 1000),
        None => 20,
    };
    let total = match aggregate(it, &[a[0].clone()], span, "count")? {
        Value::Int(n) => n,
        _ => 0,
    };
    let limited = util::with_field(q, "limit", Value::Int(per));
    let Value::Record(lr) = &limited else { unreachable!() };
    let paged = util::with_field(lr, "offset", Value::Int((page - 1) * per));
    let items = rows(it, &paged, span)?;
    let pages = if total == 0 { 1 } else { (total + per - 1) / per };
    Ok(util::record(vec![
        ("items", Value::list(items)),
        ("page", Value::Int(page)),
        ("per_page", Value::Int(per)),
        ("total", Value::Int(total)),
        ("pages", Value::Int(pages)),
    ]))
}

/// `db.transaction(fn() = ...)`: everything the function writes, or nothing
/// when it returns `Err(...)` or stops on an error.
fn db_transaction(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    mark_impure();
    let f = func_arg(a, 0, "db.transaction", span)?.clone();
    let depth = DEPTH.with(|d| d.get());
    if depth > 0 {
        // Inside another transaction: part of it.
        return it.call(f, vec![], span);
    }
    super::begin().map_err(|e| error("db.transaction", e, span))?;
    DEPTH.with(|d| d.set(1));
    let out = it.call(f, vec![], span);
    DEPTH.with(|d| d.set(0));
    let commit = matches!(&out, Ok(v) if variant_name(v).as_deref() != Some("Err"));
    super::finish(commit).map_err(|e| error("db.transaction", e, span))?;
    out
}

/// `db.sql("SELECT ... WHERE x = ?", [x])`: SQL written by hand, with parameters.
fn db_sql(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    mark_impure();
    let text = text_arg(a, 0, "db.sql", span)?;
    let params: Vec<Cell> = match a.get(1) {
        Some(Value::List(l)) => l.iter().map(cell_of).collect(),
        Some(_) => return Err(error("db.sql", tr("the parameters are a list", "les paramètres sont une liste"), span)),
        None => Vec::new(),
    };
    let text = if super::kind() == "postgres" { numbered(text) } else { text.to_string() };
    match super::raw(&text, params) {
        Ok((names, rows)) => {
            let shape = Rc::new(Shape::new(names.iter().map(|n| Rc::from(n.as_str())).collect()));
            let out = rows
                .iter()
                .map(|r| Value::Record(Rc::new(Record::with_shape(shape.clone(), r.iter().map(|c| value_of_cell(it, c, None)).collect()))))
                .collect();
            Ok(it.ok(Value::list(out)))
        }
        Err(e) => Ok(it.err(Value::text(&e))),
    }
}

/// `?` as PostgreSQL writes parameters: `$1`, `$2`... (not inside quotes).
fn numbered(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len() + 8);
    let (mut n, mut quoted) = (0, false);
    for c in sql.chars() {
        match c {
            '\'' => {
                quoted = !quoted;
                out.push(c);
            }
            '?' if !quoted => {
                n += 1;
                out.push_str(&format!("${n}"));
            }
            c => out.push(c),
        }
    }
    out
}

/// `db.validate(taches, record)`: `Ok(record)` or the same `Err` as `db.insert`, without writing.
fn db_validate(it: &mut Interp, a: &[Value], span: Span) -> Fallible<Value> {
    let m = model_arg(a, "db.validate", span)?;
    let input = record_arg(a, 1, "db.validate", span)?;
    Ok(match prepare(&m, input, false, input.get("id").and_then(id_of)) {
        Ok(_) => it.ok(a[1].clone()),
        Err(errors) => invalid(it, errors),
    })
}

pub static ORM: &[Builtin] = &[
    host!("model", "model(table, fields, options)", 2, 3, model,
        "A model: a table and its fields. `model(\"taches\", {titre: :text, fait: field(:bool, {default: false})})`. Options: `{timestamps: false}`. A model is also the query of all its rows.",
        "Un modèle : une table et ses champs. `model(\"taches\", {titre: :text, fait: field(:bool, {default: false})})`. Options : `{timestamps: false}`. Un modèle est aussi la requête de toutes ses lignes."),
    host!("field", "field(kind, options)", 1, 2, field_fn,
        "A field with rules: `field(:text, {required: true, min: 3, max: 120, unique: true, email: true, one_of: [...], default: \"x\", index: true})`.",
        "Un champ avec des règles : `field(:text, {required: true, min: 3, max: 120, unique: true, email: true, one_of: [...], default: \"x\", index: true})`."),
    host!("belongs_to", "belongs_to(table, options)", 1, 2, belongs_to,
        "A relation to a row of another table: `projet: belongs_to(\"projets\")` stores `projet_id`; `include(q, :projet)` adds the row.",
        "Une relation vers une ligne d'une autre table : `projet: belongs_to(\"projets\")` enregistre `projet_id` ; `include(q, :projet)` ajoute la ligne."),
    host!("has_many", "has_many(table, field)", 2, 2, has_many,
        "The rows of another table that refer to this one: `taches: has_many(\"taches\", :projet)`; `include(q, :taches)` adds them.",
        "Les lignes d'une autre table qui renvoient à celle-ci : `taches: has_many(\"taches\", :projet)` ; `include(q, :taches)` les ajoute."),
    host!("where", "where(query, field, operator, value)", 3, 4, where_fn,
        "Keeps the rows where the field has the value (`where(q, :fait, false)`), or compares it: `where(q, :prix, \"<\", 10)`, `:contains`, `:starts_with`, `:in`...",
        "Garde les lignes où le champ a la valeur (`where(q, :fait, false)`), ou le compare : `where(q, :prix, \"<\", 10)`, `:contains`, `:starts_with`, `:in`..."),
    host!("where_any", "where_any(query, conditions)", 2, 2, where_any,
        "Keeps the rows that meet one of the conditions: `where_any(q, [[:ville, \"Paris\"], [:ville, \"Lyon\"]])`.",
        "Garde les lignes qui remplissent l'une des conditions : `where_any(q, [[:ville, \"Paris\"], [:ville, \"Lyon\"]])`."),
    host!("search", "search(query, fields, text)", 3, 3, search,
        "The rows where one of the fields contains the text (case ignored).", "Les lignes où l'un des champs contient le texte (sans tenir compte de la casse)."),
    host!("order_by", "order_by(query, field, direction)", 2, 3, order_by,
        "Sorts the rows by a field, :asc (the default) or :desc; several calls sort by several fields.",
        "Trie les lignes selon un champ, :asc (par défaut) ou :desc ; plusieurs appels trient selon plusieurs champs."),
    host!("limit", "limit(query, n)", 2, 2, limit, "At most n rows.", "Au plus n lignes."),
    host!("offset", "offset(query, n)", 2, 2, offset, "Skips the first n rows.", "Saute les n premières lignes."),
    host!("include", "include(query, relation)", 2, 2, include,
        "Adds a relation to each row (`belongs_to` or `has_many`), in one more query, not one per row.",
        "Ajoute une relation à chaque ligne (`belongs_to` ou `has_many`), en une requête de plus, pas une par ligne."),
    host!("pick", "pick(query, fields)", 2, 2, pick, "Only these fields in the records.", "Seulement ces champs dans les enregistrements."),
];

pub static DB: &[Builtin] =
    &[
        host!(
            "all",
            "db.all(query)",
            1,
            1,
            db_all,
            "The rows of a query, as a list of records.",
            "Les lignes d'une requête, en liste d'enregistrements."
        ),
        host!(
            "first",
            "db.first(query)",
            1,
            1,
            db_first,
            "The first row: `Some(record)` or `None`.",
            "La première ligne : `Some(enregistrement)` ou `None`."
        ),
        host!(
            "find",
            "db.find(model, id)",
            2,
            2,
            db_find,
            "The row of this id: `Some(record)` or `None`.",
            "La ligne de cet id : `Some(enregistrement)` ou `None`."
        ),
        host!("count", "db.count(query)", 1, 1, db_count, "The number of rows.", "Le nombre de lignes."),
        host!("exists", "db.exists(query)", 1, 1, db_exists, "Whether the query has rows.", "Si la requête a des lignes."),
        host!(
            "sum",
            "db.sum(query, field)",
            2,
            2,
            db_sum,
            "The sum of a field (`None` without rows).",
            "La somme d'un champ (`None` sans lignes)."
        ),
        host!("avg", "db.avg(query, field)", 2, 2, db_avg, "The average of a field.", "La moyenne d'un champ."),
        host!("min", "db.min(query, field)", 2, 2, db_min, "The smallest value of a field.", "La plus petite valeur d'un champ."),
        host!("max", "db.max(query, field)", 2, 2, db_max, "The largest value of a field.", "La plus grande valeur d'un champ."),
        host!(
            "count_by",
            "db.count_by(query, field)",
            2,
            2,
            db_count_by,
            "How many rows for each value of a field, as a Dict.",
            "Combien de lignes pour chaque valeur d'un champ, en Dict."
        ),
        host!("insert", "db.insert(model, record)", 2, 2, db_insert,
        "Adds a row, checked by the rules of the fields: `Ok(record with its id)` or `Err({status: 422, error, fields})`.",
        "Ajoute une ligne, vérifiée par les règles des champs : `Ok(enregistrement avec son id)` ou `Err({status: 422, error, fields})`."),
        host!(
            "insert_all",
            "db.insert_all(model, records)",
            2,
            2,
            db_insert_all,
            "Adds several rows, all or none: `Ok(records)` or the first `Err`.",
            "Ajoute plusieurs lignes, toutes ou aucune : `Ok(enregistrements)` ou la première `Err`."
        ),
        host!(
            "update",
            "db.update(model, id, changes)",
            3,
            3,
            db_update,
            "Changes the fields given of a row: `Ok(record)`, `Err(:not_found)` or `Err({status: 422, ...})`.",
            "Change les champs donnés d'une ligne : `Ok(enregistrement)`, `Err(:not_found)` ou `Err({status: 422, ...})`."
        ),
        host!(
            "save",
            "db.save(model, record)",
            2,
            2,
            db_save,
            "Inserts a record without id, updates a record with one.",
            "Insère un enregistrement sans id, met à jour un enregistrement qui en a un."
        ),
        host!(
            "update_where",
            "db.update_where(query, changes)",
            2,
            2,
            db_update_where,
            "Changes every row of a query: `Ok(number of rows)`.",
            "Change toutes les lignes d'une requête : `Ok(nombre de lignes)`."
        ),
        host!(
            "delete",
            "db.delete(model, id)",
            2,
            2,
            db_delete,
            "Deletes a row: whether it existed.",
            "Supprime une ligne : si elle existait."
        ),
        host!(
            "delete_where",
            "db.delete_where(query)",
            1,
            1,
            db_delete_where,
            "Deletes the rows of a query: how many.",
            "Supprime les lignes d'une requête : combien."
        ),
        host!(
            "paginate",
            "db.paginate(query, page, per_page)",
            2,
            3,
            db_paginate,
            "A page of rows: `{items, page, per_page, total, pages}` (20 per page unless given).",
            "Une page de lignes : `{items, page, per_page, total, pages}` (20 par page sauf indication)."
        ),
        host!(
            "transaction",
            "db.transaction(function)",
            1,
            1,
            db_transaction,
            "Runs the function in a transaction: kept if it returns anything but `Err(...)`, undone otherwise.",
            "Exécute la fonction dans une transaction : gardée si elle renvoie autre chose que `Err(...)`, annulée sinon."
        ),
        host!(
            "sql",
            "db.sql(sql, parameters)",
            1,
            2,
            db_sql,
            "SQL written by hand, with `?` for the parameters: `Ok(records)` or `Err(message)`.",
            "Du SQL écrit à la main, avec `?` pour les paramètres : `Ok(enregistrements)` ou `Err(message)`."
        ),
        host!(
            "validate",
            "db.validate(model, record)",
            2,
            2,
            db_validate,
            "Checks a record by the rules of the fields, without writing it.",
            "Vérifie un enregistrement selon les règles des champs, sans l'écrire."
        ),
    ];

pub fn install(it: &mut Interp) {
    util::define(it, ORM);
    util::namespace(it, "db", DB);
    let _ = Sym::new("db");
}
