//! Queries, as values: `taches |> where(:fait, false) |> order_by(:priorite, :desc) |> limit(10)`.
//!
//! A query only describes rows; `db.all(q)`, `db.first(q)`, `db.count(q)`...
//! run it. The same query becomes SQL (SQLite, PostgreSQL) or is evaluated
//! on the rows of the in-memory database.

use std::cmp::Ordering;

use facile_core::util::{field, name_of, tr};
use rythme::value::Value;

use super::model::Model;
use super::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    /// The text contains the value (case ignored).
    Contains,
    StartsWith,
    EndsWith,
    In,
    NotIn,
    IsNull,
    NotNull,
}

impl Op {
    pub fn parse(s: &str) -> Option<Op> {
        Some(match s {
            "=" | "==" | "eq" => Op::Eq,
            "!=" | "<>" | "ne" => Op::Ne,
            "<" | "lt" => Op::Lt,
            "<=" | "le" => Op::Le,
            ">" | "gt" => Op::Gt,
            ">=" | "ge" => Op::Ge,
            "contains" | "like" => Op::Contains,
            "starts_with" => Op::StartsWith,
            "ends_with" => Op::EndsWith,
            "in" => Op::In,
            "not_in" | "not in" => Op::NotIn,
            "is_null" => Op::IsNull,
            "not_null" => Op::NotNull,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Cond {
    pub column: String,
    pub op: Op,
    pub value: Cell,
    /// For `in` and `not_in`.
    pub values: Vec<Cell>,
}

/// One condition, or several of which one is enough (`where_any`).
#[derive(Clone, Debug)]
pub enum Filter {
    One(Cond),
    Any(Vec<Cond>),
}

#[derive(Clone, Debug, Default)]
pub struct Query {
    pub filters: Vec<Filter>,
    pub order: Vec<(String, bool)>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub includes: Vec<String>,
    pub pick: Vec<String>,
}

/// A value of Rythme as a cell of the database, for a column of this kind.
pub fn cell_of(v: &Value) -> Cell {
    match v {
        Value::Unit => Cell::Null,
        Value::Bool(b) => Cell::Bool(*b),
        Value::Int(n) => Cell::Int(*n),
        Value::Float(f) => Cell::Float(*f),
        Value::Text(s) => Cell::Text(s.to_string()),
        Value::Symbol(s) => Cell::Text(s.name().to_string()),
        Value::Variant(var) => match facile_core::util::variant_name(v).as_deref() {
            Some("None") => Cell::Null,
            Some("Some") | Some("Ok") => cell_of(&var.fields[0]),
            _ => Cell::Text(facile_core::json::to_json_text(v)),
        },
        other => Cell::Text(facile_core::json::to_json_text(other)),
    }
}

impl Query {
    /// Reads a query value (or a model value, which is the query of all its rows).
    pub fn from_value(v: &Value, model: &Model) -> Result<Query, String> {
        let mut q = Query::default();
        let list = |k: &str| -> Vec<Value> {
            match field(v, k) {
                Some(Value::List(l)) => l.iter().cloned().collect(),
                _ => Vec::new(),
            }
        };
        let check = |c: &str| -> Result<String, String> {
            if model.kind_of(c).is_some() {
                Ok(c.to_string())
            } else {
                Err(tr(
                    &format!("`{}` has no field `{c}` (fields: {})", model.table, model.columns().join(", ")),
                    &format!("`{}` n'a pas de champ `{c}` (champs : {})", model.table, model.columns().join(", ")),
                ))
            }
        };
        let cond = |w: &Value| -> Result<Cond, String> {
            let column = check(&field(w, "field").and_then(name_of).unwrap_or_default())?;
            let op_name = field(w, "op").and_then(name_of).unwrap_or_else(|| "=".into());
            let op = Op::parse(&op_name).ok_or_else(|| {
                tr(
                    &format!("unknown operator `{op_name}`: use =, !=, <, <=, >, >=, contains, starts_with, ends_with, in, not_in"),
                    &format!("opérateur inconnu `{op_name}` : utilisez =, !=, <, <=, >, >=, contains, starts_with, ends_with, in, not_in"),
                )
            })?;
            let value = field(w, "value").cloned().unwrap_or(Value::Unit);
            let (op, cell) = match (op, cell_of(&value)) {
                (Op::Eq, Cell::Null) => (Op::IsNull, Cell::Null),
                (Op::Ne, Cell::Null) => (Op::NotNull, Cell::Null),
                (op, c) => (op, c),
            };
            let kind = model.kind_of(&column).unwrap_or(super::model::Kind::Text);
            let fit = |c: Cell| -> Result<Cell, String> {
                c.clone().coerce(kind).ok_or_else(|| {
                    tr(
                        &format!("`{column}` holds values of kind {}: {} cannot be compared with it", kind.name(), c.as_text()),
                        &format!("`{column}` contient des valeurs de sorte {} : {} ne peut pas lui être comparé", kind.name(), c.as_text()),
                    )
                })
            };
            let cell = match op {
                Op::Contains | Op::StartsWith | Op::EndsWith => Cell::Text(cell.as_text()),
                _ => fit(cell)?,
            };
            let values = match (&op, &value) {
                (Op::In | Op::NotIn, Value::List(l)) => l.iter().map(|x| fit(cell_of(x))).collect::<Result<Vec<_>, _>>()?,
                (Op::In | Op::NotIn, _) => {
                    return Err(tr("`in` and `not_in` take a list of values", "`in` et `not_in` prennent une liste de valeurs"));
                }
                _ => Vec::new(),
            };
            Ok(Cond { column, op, value: cell, values })
        };
        for w in list("wheres") {
            if let Some(Value::List(any)) = field(&w, "any") {
                let mut cs = Vec::new();
                for c in any.iter() {
                    cs.push(cond(c)?);
                }
                q.filters.push(Filter::Any(cs));
            } else {
                q.filters.push(Filter::One(cond(&w)?));
            }
        }
        for o in list("order") {
            let c = check(&field(&o, "field").and_then(name_of).unwrap_or_default())?;
            let desc = field(&o, "dir").and_then(name_of).is_some_and(|d| d == "desc");
            q.order.push((c, desc));
        }
        q.limit = match field(v, "limit") {
            Some(Value::Int(n)) => Some((*n).max(0)),
            _ => None,
        };
        q.offset = match field(v, "offset") {
            Some(Value::Int(n)) => Some((*n).max(0)),
            _ => None,
        };
        q.includes = list("includes").iter().filter_map(name_of).collect();
        for p in list("pick").iter().filter_map(name_of) {
            q.pick.push(check(&p)?);
        }
        Ok(q)
    }
}

// ----- SQL --------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Sqlite,
    Postgres,
}

/// SQL being written, with its parameters.
pub struct Sql {
    pub text: String,
    pub params: Vec<Cell>,
    dialect: Dialect,
}

impl Sql {
    pub fn new(dialect: Dialect) -> Sql {
        Sql { text: String::new(), params: Vec::new(), dialect }
    }

    pub fn push(&mut self, s: &str) -> &mut Sql {
        self.text.push_str(s);
        self
    }

    /// A parameter: `?` for SQLite, `$n` for PostgreSQL.
    pub fn param(&mut self, c: Cell) -> &mut Sql {
        self.params.push(c);
        match self.dialect {
            Dialect::Sqlite => self.text.push('?'),
            Dialect::Postgres => {
                self.text.push('$');
                self.text.push_str(&self.params.len().to_string());
            }
        }
        self
    }

    pub fn ident(&mut self, name: &str) -> &mut Sql {
        // Names were checked by `valid_name`; quoted all the same.
        self.text.push('"');
        self.text.push_str(name);
        self.text.push('"');
        self
    }
}

fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

fn write_cond(sql: &mut Sql, c: &Cond) {
    let ci = sql.dialect == Dialect::Postgres;
    match c.op {
        Op::IsNull => {
            sql.ident(&c.column).push(" IS NULL");
        }
        Op::NotNull => {
            sql.ident(&c.column).push(" IS NOT NULL");
        }
        Op::In | Op::NotIn => {
            if c.values.is_empty() {
                sql.push(if c.op == Op::In { "1 = 0" } else { "1 = 1" });
                return;
            }
            sql.ident(&c.column).push(if c.op == Op::In { " IN (" } else { " NOT IN (" });
            for (i, v) in c.values.iter().enumerate() {
                if i > 0 {
                    sql.push(", ");
                }
                sql.param(v.clone());
            }
            sql.push(")");
        }
        Op::Contains | Op::StartsWith | Op::EndsWith => {
            let t = like_escape(&c.value.as_text());
            let pattern = match c.op {
                Op::Contains => format!("%{t}%"),
                Op::StartsWith => format!("{t}%"),
                _ => format!("%{t}"),
            };
            if ci {
                sql.push("CAST(").ident(&c.column).push(" AS TEXT) ILIKE ");
            } else {
                sql.ident(&c.column).push(" LIKE ");
            }
            sql.param(Cell::Text(pattern)).push(" ESCAPE '\\'");
        }
        op => {
            sql.ident(&c.column);
            sql.push(match op {
                Op::Eq => " = ",
                Op::Ne => " <> ",
                Op::Lt => " < ",
                Op::Le => " <= ",
                Op::Gt => " > ",
                _ => " >= ",
            });
            sql.param(c.value.clone());
        }
    }
}

/// ` WHERE ...` for the filters of a query.
pub fn write_where(sql: &mut Sql, q: &Query) {
    for (i, f) in q.filters.iter().enumerate() {
        sql.push(if i == 0 { " WHERE " } else { " AND " });
        match f {
            Filter::One(c) => write_cond(sql, c),
            Filter::Any(cs) => {
                sql.push("(");
                if cs.is_empty() {
                    sql.push("1 = 0");
                }
                for (j, c) in cs.iter().enumerate() {
                    if j > 0 {
                        sql.push(" OR ");
                    }
                    write_cond(sql, c);
                }
                sql.push(")");
            }
        }
    }
}

pub fn write_order_limit(sql: &mut Sql, q: &Query) {
    if q.order.is_empty() {
        sql.push(" ORDER BY \"id\"");
    } else {
        for (i, (c, desc)) in q.order.iter().enumerate() {
            sql.push(if i == 0 { " ORDER BY " } else { ", " });
            sql.ident(c).push(if *desc { " DESC" } else { " ASC" });
        }
    }
    if let Some(l) = q.limit {
        sql.push(&format!(" LIMIT {l}"));
    } else if q.offset.is_some() && sql.dialect == Dialect::Sqlite {
        sql.push(" LIMIT -1");
    }
    if let Some(o) = q.offset {
        sql.push(&format!(" OFFSET {o}"));
    }
}

// ----- in memory --------------------------------------------------------

fn cmp_cells(a: &Cell, b: &Cell) -> Option<Ordering> {
    match (a, b) {
        (Cell::Int(x), Cell::Int(y)) => Some(x.cmp(y)),
        (Cell::Int(x), Cell::Float(y)) => (*x as f64).partial_cmp(y),
        (Cell::Float(x), Cell::Int(y)) => x.partial_cmp(&(*y as f64)),
        (Cell::Float(x), Cell::Float(y)) => x.partial_cmp(y),
        (Cell::Text(x), Cell::Text(y)) => Some(x.cmp(y)),
        (Cell::Bool(x), Cell::Bool(y)) => Some(x.cmp(y)),
        (Cell::Null, Cell::Null) => Some(Ordering::Equal),
        _ => None,
    }
}

/// The order of `ORDER BY`: NULL first, like SQLite.
pub fn order_cells(a: &Cell, b: &Cell) -> Ordering {
    match (a, b) {
        (Cell::Null, Cell::Null) => Ordering::Equal,
        (Cell::Null, _) => Ordering::Less,
        (_, Cell::Null) => Ordering::Greater,
        _ => cmp_cells(a, b).unwrap_or(Ordering::Equal),
    }
}

fn holds(c: &Cond, get: &dyn Fn(&str) -> Cell) -> bool {
    let v = get(&c.column);
    let eq = |x: &Cell, y: &Cell| cmp_cells(x, y) == Some(Ordering::Equal);
    match c.op {
        Op::IsNull => matches!(v, Cell::Null),
        Op::NotNull => !matches!(v, Cell::Null),
        Op::Eq => eq(&v, &c.value),
        Op::Ne => !matches!(v, Cell::Null) && !eq(&v, &c.value),
        Op::Lt => cmp_cells(&v, &c.value) == Some(Ordering::Less),
        Op::Le => matches!(cmp_cells(&v, &c.value), Some(Ordering::Less | Ordering::Equal)),
        Op::Gt => cmp_cells(&v, &c.value) == Some(Ordering::Greater),
        Op::Ge => matches!(cmp_cells(&v, &c.value), Some(Ordering::Greater | Ordering::Equal)),
        Op::In => c.values.iter().any(|x| eq(&v, x)),
        Op::NotIn => !matches!(v, Cell::Null) && !c.values.iter().any(|x| eq(&v, x)),
        Op::Contains | Op::StartsWith | Op::EndsWith => {
            if matches!(v, Cell::Null) {
                return false;
            }
            let hay = v.as_text().to_lowercase();
            let needle = c.value.as_text().to_lowercase();
            match c.op {
                Op::Contains => hay.contains(&needle),
                Op::StartsWith => hay.starts_with(&needle),
                _ => hay.ends_with(&needle),
            }
        }
    }
}

/// Whether a row (read by `get`) is kept by the filters.
pub fn keeps(q: &Query, get: &dyn Fn(&str) -> Cell) -> bool {
    q.filters.iter().all(|f| match f {
        Filter::One(c) => holds(c, get),
        Filter::Any(cs) => cs.iter().any(|c| holds(c, get)),
    })
}
