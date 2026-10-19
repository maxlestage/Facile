//! `memory://`: tables in memory, shared by every worker of the server.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, MutexGuard, OnceLock};

use super::model::Model;
use super::query::{keeps, order_cells, Query};
use super::{Cell, Row};

#[derive(Clone, Default)]
pub struct Table {
    columns: Vec<String>,
    next_id: i64,
    rows: BTreeMap<i64, HashMap<String, Cell>>,
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

fn row_of(m: &Model, id: i64, r: &HashMap<String, Cell>) -> Row {
    m.columns().iter().map(|c| if c == "id" { Cell::Int(id) } else { r.get(c).cloned().unwrap_or(Cell::Null) }).collect()
}

/// The ids of the rows of the query, in its order, after its offset and limit.
fn matching(t: &Table, q: &Query) -> Vec<i64> {
    let mut ids: Vec<i64> = t
        .rows
        .iter()
        .filter(|(id, r)| {
            let get = |c: &str| if c == "id" { Cell::Int(**id) } else { r.get(c).cloned().unwrap_or(Cell::Null) };
            keeps(q, &get)
        })
        .map(|(id, _)| *id)
        .collect();
    if !q.order.is_empty() {
        let get = |id: i64, c: &str| if c == "id" { Cell::Int(id) } else { t.rows[&id].get(c).cloned().unwrap_or(Cell::Null) };
        ids.sort_by(|a, b| {
            for (c, desc) in &q.order {
                let o = order_cells(&get(*a, c), &get(*b, c));
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
    Ok(matching(t, q).into_iter().map(|id| row_of(m, id, &t.rows[&id])).collect())
}

pub fn aggregate(m: &Model, q: &Query, func: &str, column: &str) -> Result<Cell, String> {
    let s = store();
    let t = s.get(&m.table).ok_or_else(|| missing(m))?;
    let ids = matching(t, q);
    if func == "count" {
        return Ok(Cell::Int(ids.len() as i64));
    }
    let vals: Vec<Cell> = ids
        .iter()
        .map(|id| if column == "id" { Cell::Int(*id) } else { t.rows[id].get(column).cloned().unwrap_or(Cell::Null) })
        .filter(|c| !matches!(c, Cell::Null))
        .collect();
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
    let r: HashMap<String, Cell> = cols.iter().cloned().zip(cells).collect();
    let row = row_of(m, id, &r);
    t.rows.insert(id, r);
    Ok(row)
}

pub fn update(m: &Model, q: &Query, cols: &[String], cells: Vec<Cell>) -> Result<i64, String> {
    let mut s = store();
    let t = s.get_mut(&m.table).ok_or_else(|| missing(m))?;
    let ids = matching(t, q);
    for id in &ids {
        if let Some(r) = t.rows.get_mut(id) {
            for (c, v) in cols.iter().zip(cells.iter()) {
                r.insert(c.clone(), v.clone());
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
