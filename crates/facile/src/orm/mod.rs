//! The ORM: models, queries and their execution, on three databases chosen
//! by an URL: `memory://` (shared by the workers, for development and
//! tests), `sqlite://app.db` (compiled in) and `postgres://...` (TLS).
//!
//! Each worker has its own connection; values are bound as parameters,
//! names (checked when the model is made) are quoted: no SQL injection.

pub mod host;
pub mod memory;
pub mod model;
pub mod query;
pub mod resource;
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub mod sql;

use std::cell::RefCell;

use model::{Kind, Model};
use query::Query;

/// A value in the database.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
    Bool(bool),
}

impl Cell {
    pub fn as_text(&self) -> String {
        match self {
            Cell::Null => String::new(),
            Cell::Int(n) => n.to_string(),
            Cell::Float(f) => f.to_string(),
            Cell::Text(s) => s.clone(),
            Cell::Bool(b) => b.to_string(),
        }
    }

    /// The cell as a column of this kind stores it; `None` when it cannot be one.
    pub fn coerce(self, kind: Kind) -> Option<Cell> {
        Some(match (kind, self) {
            (_, Cell::Null) => Cell::Null,
            (Kind::Int, Cell::Int(n)) => Cell::Int(n),
            (Kind::Int, Cell::Float(f)) if f.fract() == 0.0 => Cell::Int(f as i64),
            (Kind::Int, Cell::Text(s)) => Cell::Int(s.trim().parse().ok()?),
            (Kind::Int, Cell::Bool(b)) => Cell::Int(b as i64),
            (Kind::Float, Cell::Int(n)) => Cell::Float(n as f64),
            (Kind::Float, Cell::Float(f)) => Cell::Float(f),
            (Kind::Float, Cell::Text(s)) => Cell::Float(s.trim().replace(',', ".").parse().ok()?),
            (Kind::Bool, Cell::Bool(b)) => Cell::Bool(b),
            (Kind::Bool, Cell::Int(n)) => Cell::Bool(n != 0),
            (Kind::Bool, Cell::Text(s)) => match s.as_str() {
                "true" | "on" | "1" | "yes" | "oui" => Cell::Bool(true),
                "false" | "off" | "0" | "no" | "non" | "" => Cell::Bool(false),
                _ => return None,
            },
            (Kind::Text | Kind::Date | Kind::DateTime | Kind::Json, c) => Cell::Text(c.as_text()),
            _ => return None,
        })
    }
}

/// A row, in the order of `Model::columns`.
pub type Row = Vec<Cell>;

pub enum Db {
    Memory,
    #[cfg(feature = "sqlite")]
    Sqlite(rusqlite::Connection),
    #[cfg(feature = "postgres")]
    Postgres(Box<postgres::Client>),
}

thread_local! {
    static DB: RefCell<Option<Db>> = const { RefCell::new(None) };
    /// Inside `db.transaction(...)` with the in-memory database: the tables as they were.
    static SNAPSHOT: RefCell<Vec<memory::Snapshot>> = const { RefCell::new(Vec::new()) };
}

/// Opens this thread's database.
pub fn open(url: &str) -> Result<(), String> {
    let db = if url.starts_with("memory:") || url.is_empty() {
        Db::Memory
    } else if let Some(path) = url.strip_prefix("sqlite://").or_else(|| url.strip_prefix("sqlite:")) {
        #[cfg(feature = "sqlite")]
        {
            sql::open_sqlite(&crate::runtime::resolve(path))?
        }
        #[cfg(not(feature = "sqlite"))]
        {
            let _ = path;
            return Err("SQLite is not compiled in (feature `sqlite`)".into());
        }
    } else if url.starts_with("postgres://") || url.starts_with("postgresql://") {
        #[cfg(feature = "postgres")]
        {
            sql::open_postgres(url)?
        }
        #[cfg(not(feature = "postgres"))]
        {
            return Err("PostgreSQL is not compiled in (feature `postgres`)".into());
        }
    } else {
        return Err(facile_core::util::tr(
            &format!("unknown database `{url}`: use memory://, sqlite://file.db or postgres://..."),
            &format!("base de données inconnue `{url}` : utilisez memory://, sqlite://fichier.db ou postgres://..."),
        ));
    };
    DB.with(|d| *d.borrow_mut() = Some(db));
    Ok(())
}

fn with_db<R>(f: impl FnOnce(&mut Db) -> Result<R, String>) -> Result<R, String> {
    DB.with(|d| {
        let mut d = d.borrow_mut();
        if d.is_none() {
            *d = Some(Db::Memory);
        }
        f(d.as_mut().expect("database"))
    })
}

/// Creates the tables of the models, and adds the columns they lack.
pub fn migrate(models: &[rythme::value::Value]) -> Result<(), String> {
    for v in models {
        let m = model::model_of(v)?;
        with_db(|db| match db {
            Db::Memory => {
                memory::create(&m);
                Ok(())
            }
            #[cfg(feature = "sqlite")]
            Db::Sqlite(c) => sql::migrate_sqlite(c, &m),
            #[cfg(feature = "postgres")]
            Db::Postgres(c) => sql::migrate_postgres(c, &m),
        })?;
    }
    Ok(())
}

pub fn select(m: &Model, q: &Query) -> Result<Vec<Row>, String> {
    with_db(|db| match db {
        Db::Memory => memory::select(m, q),
        #[cfg(feature = "sqlite")]
        Db::Sqlite(c) => sql::select_sqlite(c, m, q),
        #[cfg(feature = "postgres")]
        Db::Postgres(c) => sql::select_postgres(c, m, q),
    })
}

/// `count`, `sum`, `avg`, `min` or `max` of a column (`*` for count).
pub fn aggregate(m: &Model, q: &Query, func: &str, column: &str) -> Result<Cell, String> {
    with_db(|db| match db {
        Db::Memory => memory::aggregate(m, q, func, column),
        #[cfg(feature = "sqlite")]
        Db::Sqlite(c) => sql::aggregate_sqlite(c, m, q, func, column),
        #[cfg(feature = "postgres")]
        Db::Postgres(c) => sql::aggregate_postgres(c, m, q, func, column),
    })
}

pub fn insert(m: &Model, cols: &[String], cells: Vec<Cell>) -> Result<Row, String> {
    with_db(|db| match db {
        Db::Memory => memory::insert(m, cols, cells),
        #[cfg(feature = "sqlite")]
        Db::Sqlite(c) => sql::insert_sqlite(c, m, cols, cells),
        #[cfg(feature = "postgres")]
        Db::Postgres(c) => sql::insert_postgres(c, m, cols, cells),
    })
}

/// Changes the rows of the query; returns how many.
pub fn update(m: &Model, q: &Query, cols: &[String], cells: Vec<Cell>) -> Result<i64, String> {
    with_db(|db| match db {
        Db::Memory => memory::update(m, q, cols, cells),
        #[cfg(feature = "sqlite")]
        Db::Sqlite(c) => sql::update_sqlite(c, m, q, cols, cells),
        #[cfg(feature = "postgres")]
        Db::Postgres(c) => sql::update_postgres(c, m, q, cols, cells),
    })
}

pub fn delete(m: &Model, q: &Query) -> Result<i64, String> {
    with_db(|db| match db {
        Db::Memory => memory::delete(m, q),
        #[cfg(feature = "sqlite")]
        Db::Sqlite(c) => sql::delete_sqlite(c, m, q),
        #[cfg(feature = "postgres")]
        Db::Postgres(c) => sql::delete_postgres(c, m, q),
    })
}

/// SQL written by hand: the names of the columns, then the rows.
pub fn raw(text: &str, params: Vec<Cell>) -> Result<(Vec<String>, Vec<Row>), String> {
    with_db(|db| match db {
        Db::Memory => Err(facile_core::util::tr(
            "`db.sql` needs an SQL database (sqlite:// or postgres://), not memory://",
            "`db.sql` a besoin d'une base SQL (sqlite:// ou postgres://), pas memory://",
        )),
        #[cfg(feature = "sqlite")]
        Db::Sqlite(c) => sql::raw_sqlite(c, text, params),
        #[cfg(feature = "postgres")]
        Db::Postgres(c) => sql::raw_postgres(c, text, params),
    })
}

thread_local! {
    /// How many transactions are open: the ones inside are savepoints.
    static OPEN: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Starts a transaction, or a savepoint inside the one that is open.
pub fn begin() -> Result<(), String> {
    let depth = OPEN.with(|o| o.get());
    let sql = if depth == 0 { "BEGIN".to_string() } else { format!("SAVEPOINT facile_{depth}") };
    with_db(|db| match db {
        Db::Memory => {
            SNAPSHOT.with(|s| s.borrow_mut().push(memory::snapshot()));
            Ok(())
        }
        #[cfg(feature = "sqlite")]
        Db::Sqlite(c) => c.execute_batch(&sql).map_err(|e| e.to_string()),
        #[cfg(feature = "postgres")]
        Db::Postgres(c) => c.batch_execute(&sql).map_err(|e| e.to_string()),
    })?;
    OPEN.with(|o| o.set(depth + 1));
    Ok(())
}

/// Ends the innermost transaction: kept with `commit`, undone otherwise.
pub fn finish(commit: bool) -> Result<(), String> {
    let depth = OPEN.with(|o| o.get());
    if depth == 0 {
        return Ok(());
    }
    OPEN.with(|o| o.set(depth - 1));
    let inner = depth - 1;
    let sql = match (inner, commit) {
        (0, true) => "COMMIT".to_string(),
        (0, false) => "ROLLBACK".to_string(),
        (n, true) => format!("RELEASE SAVEPOINT facile_{n}"),
        (n, false) => format!("ROLLBACK TO SAVEPOINT facile_{n}; RELEASE SAVEPOINT facile_{n}"),
    };
    with_db(|db| match db {
        Db::Memory => {
            if let Some(s) = SNAPSHOT.with(|s| s.borrow_mut().pop()) {
                if !commit {
                    memory::restore(s);
                }
            }
            Ok(())
        }
        #[cfg(feature = "sqlite")]
        Db::Sqlite(c) => c.execute_batch(&sql).map_err(|e| e.to_string()),
        #[cfg(feature = "postgres")]
        Db::Postgres(c) => c.batch_execute(&sql).map_err(|e| e.to_string()),
    })
}

/// The name of the database in use, for messages.
pub fn kind() -> &'static str {
    with_db(|db| {
        Ok(match db {
            Db::Memory => "memory",
            #[cfg(feature = "sqlite")]
            Db::Sqlite(_) => "sqlite",
            #[cfg(feature = "postgres")]
            Db::Postgres(_) => "postgres",
        })
    })
    .unwrap_or("memory")
}

/// Now, as the timestamps of rows write it: `2026-10-19T08:30:00Z`.
pub fn now_iso() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if mo <= 2 { 1 } else { 0 };
    format!("{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem / 60 % 60, rem % 60)
}
