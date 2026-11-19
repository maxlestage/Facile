//! `memory://`: tables in memory, shared by every worker of the server.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, MutexGuard, OnceLock};

use super::model::Model;
use super::query::{keeps, order_cells, Query};
use super::{Cell, Row};

/// A table: its rows by id, each with its cells in the order of `columns`
/// (a row's cell is found at a position, without hashing the column's name;
/// a row made before a column was added is shorter: that cell is null).
#[derive(Clone, Default)]
pub struct Table {
    columns: Vec<String>,
    next_id: i64,
    rows: BTreeMap<i64, Vec<Cell>>,
}

/// Where a column of a model is in the rows of its table.
#[derive(Clone, Copy)]
enum Slot {
    /// `id`, the key of the row.
    Id,
    At(usize),
    /// A column the table has never had: null.
    Missing,
}

impl Table {
    fn slot(&self, column: &str) -> Slot {
        if column == "id" {
            return Slot::Id;
        }
        self.columns.iter().position(|c| c == column).map_or(Slot::Missing, Slot::At)
    }

    /// The position of a column, added to the table when it has none of that name.
    fn position_or_add(&mut self, column: &str) -> usize {
        match self.columns.iter().position(|c| c == column) {
            Some(i) => i,
            None => {
                self.columns.push(column.to_string());
                self.columns.len() - 1
            }
        }
    }
}

/// The cell of a row at a slot.
#[inline]
fn cell(id: i64, r: &[Cell], slot: Slot) -> Cell {
    match slot {
        Slot::Id => Cell::Int(id),
        Slot::At(i) => r.get(i).cloned().unwrap_or(Cell::Null),
        Slot::Missing => Cell::Null,
    }
}

pub type Snapshot = HashMap<String, Table>;

fn store() -> MutexGuard<'static, HashMap<String, Table>> {
    static STORE: OnceLock<Mutex<HashMap<String, Table>>> = OnceLock::new();
    STORE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner())
}

pub fn create(m: &Model) {
    let mut s = store();
    let t = s.entry(m.table.clone()).or_insert_with(|| Table { next_id: 1, ..Table::default() });
    for c in m.columns() {
        if !t.columns.contains(&c) {
            t.columns.push(c);
        }
    }
}

fn missing(m: &Model) -> String {
    facile_core::util::tr(
        &format!("the table `{}` does not exist: add its model to `models(app, [...])`", m.table),
        &format!("la table `{}` n'existe pas : ajoutez son modèle à `models(app, [...])`", m.table),
    )
}

/// The slots of the columns of a model, in its order.
fn slots(t: &Table, m: &Model) -> Vec<Slot> {
    m.column_names().map(|c| t.slot(c)).collect()
}

fn row_of(slots: &[Slot], id: i64, r: &[Cell]) -> Row {
    slots.iter().map(|s| cell(id, r, *s)).collect()
}

/// The ids of the rows of the query, in its order, after its offset and limit.
fn matching(t: &Table, q: &Query) -> Vec<i64> {
    let mut ids: Vec<i64> = t
        .rows
        .iter()
        .filter(|(id, r)| {
            let get = |c: &str| cell(**id, r, t.slot(c));
            keeps(q, &get)
        })
        .map(|(id, _)| *id)
        .collect();
    if !q.order.is_empty() {
        let order: Vec<(Slot, bool)> = q.order.iter().map(|(c, desc)| (t.slot(c), *desc)).collect();
        // The row is only looked up for a column other than `id`.
        let get = |id: i64, s: Slot| match s {
            Slot::Id => Cell::Int(id),
            _ => cell(id, &t.rows[&id], s),
        };
        ids.sort_by(|a, b| {
            for (s, desc) in &order {
                let o = order_cells(&get(*a, *s), &get(*b, *s));
                let o = if *desc { o.reverse() } else { o };
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
            }
            a.cmp(b)
        });
    }
    let skip = q.offset.unwrap_or(0) as usize;
    let take = q.limit.map(|l| l as usize).unwrap_or(usize::MAX);
    ids.into_iter().skip(skip).take(take).collect()
}

pub fn select(m: &Model, q: &Query) -> Result<Vec<Row>, String> {
    let s = store();
    let t = s.get(&m.table).ok_or_else(|| missing(m))?;
    let slots = slots(t, m);
    Ok(matching(t, q).into_iter().map(|id| row_of(&slots, id, &t.rows[&id])).collect())
}

pub fn aggregate(m: &Model, q: &Query, func: &str, column: &str) -> Result<Cell, String> {
    let s = store();
    let t = s.get(&m.table).ok_or_else(|| missing(m))?;
    let ids = matching(t, q);
    if func == "count" {
        return Ok(Cell::Int(ids.len() as i64));
    }
    let slot = t.slot(column);
    let vals: Vec<Cell> = ids.iter().map(|id| cell(*id, &t.rows[id], slot)).filter(|c| !matches!(c, Cell::Null)).collect();
    if vals.is_empty() {
        return Ok(Cell::Null);
    }
    let num = |c: &Cell| match c {
        Cell::Int(n) => *n as f64,
        Cell::Float(f) => *f,
        _ => 0.0,
    };
    let all_int = vals.iter().all(|c| matches!(c, Cell::Int(_)));
    Ok(match func {
        "sum" if all_int => Cell::Int(vals.iter().map(|c| if let Cell::Int(n) = c { *n } else { 0 }).sum()),
        "sum" => Cell::Float(vals.iter().map(num).sum()),
        "avg" => Cell::Float(vals.iter().map(num).sum::<f64>() / vals.len() as f64),
        "min" => vals.iter().cloned().min_by(order_cells).unwrap_or(Cell::Null),
        "max" => vals.iter().cloned().max_by(order_cells).unwrap_or(Cell::Null),
        _ => Cell::Null,
    })
}

pub fn insert(m: &Model, cols: &[String], cells: Vec<Cell>) -> Result<Row, String> {
    let mut s = store();
    let t = s.get_mut(&m.table).ok_or_else(|| missing(m))?;
    let id = t.next_id;
    t.next_id += 1;
    let at: Vec<usize> = cols.iter().map(|c| t.position_or_add(c)).collect();
    let mut r = vec![Cell::Null; t.columns.len()];
    for (i, v) in at.into_iter().zip(cells) {
        r[i] = v;
    }
    let row = row_of(&slots(t, m), id, &r);
    t.rows.insert(id, r);
    Ok(row)
}

pub fn update(m: &Model, q: &Query, cols: &[String], cells: Vec<Cell>) -> Result<i64, String> {
    let mut s = store();
    let t = s.get_mut(&m.table).ok_or_else(|| missing(m))?;
    let ids = matching(t, q);
    let at: Vec<usize> = cols.iter().map(|c| t.position_or_add(c)).collect();
    let width = t.columns.len();
    for id in &ids {
        if let Some(r) = t.rows.get_mut(id) {
            if r.len() < width {
                r.resize(width, Cell::Null);
            }
            for (i, v) in at.iter().zip(cells.iter()) {
                r[*i] = v.clone();
            }
        }
    }
    Ok(ids.len() as i64)
}

pub fn delete(m: &Model, q: &Query) -> Result<i64, String> {
    let mut s = store();
    let t = s.get_mut(&m.table).ok_or_else(|| missing(m))?;
    let ids = matching(t, q);
    for id in &ids {
        t.rows.remove(id);
    }
    Ok(ids.len() as i64)
}

pub fn snapshot() -> Snapshot {
    store().clone()
}

pub fn restore(s: Snapshot) {
    *store() = s;
}

#[cfg(test)]
mod tests {
    use super::super::model::{Field, Kind};
    use super::super::query::{Cond, Filter, Op};
    use super::*;

    fn model(fields: &[(&str, Kind)]) -> Model {
        let field = |(name, kind): &(&str, Kind)| Field {
            name: name.to_string(),
            kind: *kind,
            required: false,
            unique: false,
            index: false,
            default: None,
            min: None,
            max: None,
            email: false,
            one_of: Vec::new(),
            refers: None,
        };
        Model { table: "zz_memory_columns".into(), fields: fields.iter().map(field).collect(), has_many: Vec::new(), timestamps: false }
    }

    #[test]
    fn rows_made_before_a_column_was_added() {
        let v1 = model(&[("a", Kind::Int)]);
        create(&v1);
        assert_eq!(insert(&v1, &["a".into()], vec![Cell::Int(1)]).unwrap(), vec![Cell::Int(1), Cell::Int(1)]);
        let v2 = model(&[("a", Kind::Int), ("b", Kind::Text)]);
        create(&v2);
        let all = Query::default();
        assert_eq!(select(&v2, &all).unwrap(), vec![vec![Cell::Int(1), Cell::Int(1), Cell::Null]]);
        // Columns given in another order, one of them twice: the last one counts.
        let cols: Vec<String> = ["b", "a", "b"].map(String::from).into();
        let row = insert(&v2, &cols, vec![Cell::Text("y".into()), Cell::Int(2), Cell::Text("z".into())]).unwrap();
        assert_eq!(row, vec![Cell::Int(2), Cell::Int(2), Cell::Text("z".into())]);
        // A column the table does not have yet, and a row widened by an update.
        let one = Query {
            filters: vec![Filter::One(Cond { column: "a".into(), op: Op::Eq, value: Cell::Int(1), values: Vec::new() })],
            ..Query::default()
        };
        assert_eq!(update(&v2, &one, &["b".into(), "c".into()], vec![Cell::Text("x".into()), Cell::Int(9)]).unwrap(), 1);
        let v3 = model(&[("c", Kind::Int), ("b", Kind::Text)]);
        let by_b = Query { order: vec![("b".into(), true)], ..Query::default() };
        assert_eq!(
            select(&v3, &by_b).unwrap(),
            vec![vec![Cell::Int(2), Cell::Null, Cell::Text("z".into())], vec![Cell::Int(1), Cell::Int(9), Cell::Text("x".into())]]
        );
        assert_eq!(aggregate(&v3, &all, "sum", "c").unwrap(), Cell::Int(9));
        assert_eq!(aggregate(&v3, &all, "max", "id").unwrap(), Cell::Int(2));
        assert_eq!(aggregate(&v3, &all, "max", "nothing").unwrap(), Cell::Null);
        assert_eq!(delete(&v3, &one).unwrap(), 1);
        assert_eq!(select(&v1, &all).unwrap(), vec![vec![Cell::Int(2), Cell::Int(2)]]);
    }
}
