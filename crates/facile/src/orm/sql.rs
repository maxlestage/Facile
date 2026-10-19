//! SQLite and PostgreSQL: the same queries, written in their SQL.

use std::path::Path;

use super::model::{Kind, Model};
use super::query::{write_order_limit, write_where, Dialect, Query, Sql};
use super::{Cell, Db, Row};

fn column_type(kind: Kind, d: Dialect) -> &'static str {
    match (kind, d) {
        (Kind::Int, Dialect::Sqlite) => "INTEGER",
        (Kind::Int, Dialect::Postgres) => "BIGINT",
        (Kind::Float, Dialect::Sqlite) => "REAL",
        (Kind::Float, Dialect::Postgres) => "DOUBLE PRECISION",
        (Kind::Bool, Dialect::Sqlite) => "INTEGER",
        (Kind::Bool, Dialect::Postgres) => "BOOLEAN",
        _ => "TEXT",
    }
}

/// `CREATE TABLE IF NOT EXISTS ...`, then the statements for the indexes.
fn create_table(m: &Model, d: Dialect) -> Vec<String> {
    let mut cols = vec![match d {
        Dialect::Sqlite => "\"id\" INTEGER PRIMARY KEY AUTOINCREMENT".to_string(),
        Dialect::Postgres => "\"id\" BIGSERIAL PRIMARY KEY".to_string(),
    }];
    for f in &m.fields {
        cols.push(format!("\"{}\" {}", f.name, column_type(f.kind, d)));
    }
    if m.timestamps {
        cols.push("\"created_at\" TEXT".into());
        cols.push("\"updated_at\" TEXT".into());
    }
    let mut out = vec![format!("CREATE TABLE IF NOT EXISTS \"{}\" ({})", m.table, cols.join(", "))];
    out.extend(indexes(m));
    out
}

fn indexes(m: &Model) -> Vec<String> {
    let mut out = Vec::new();
    for f in &m.fields {
        if f.unique {
            out.push(format!("CREATE UNIQUE INDEX IF NOT EXISTS \"{t}_{c}_unique\" ON \"{t}\" (\"{c}\")", t = m.table, c = f.name));
        } else if f.index {
            out.push(format!("CREATE INDEX IF NOT EXISTS \"{t}_{c}_index\" ON \"{t}\" (\"{c}\")", t = m.table, c = f.name));
        }
    }
    out
}

/// `ALTER TABLE ... ADD COLUMN` for the columns the table lacks.
fn add_columns(m: &Model, existing: &[String], d: Dialect) -> Vec<String> {
    let mut out = Vec::new();
    for f in &m.fields {
        if !existing.iter().any(|c| c == &f.name) {
            out.push(format!("ALTER TABLE \"{}\" ADD COLUMN \"{}\" {}", m.table, f.name, column_type(f.kind, d)));
        }
    }
    if m.timestamps {
        for c in ["created_at", "updated_at"] {
            if !existing.iter().any(|x| x == c) {
                out.push(format!("ALTER TABLE \"{}\" ADD COLUMN \"{c}\" TEXT", m.table));
            }
        }
    }
    out
}

fn select_sql(m: &Model, q: &Query, d: Dialect) -> Sql {
    let mut sql = Sql::new(d);
    sql.push("SELECT ");
    for (i, c) in m.columns().iter().enumerate() {
        if i > 0 {
            sql.push(", ");
        }
        sql.ident(c);
    }
    sql.push(" FROM ").ident(&m.table);
    write_where(&mut sql, q);
    write_order_limit(&mut sql, q);
    sql
}

fn aggregate_sql(m: &Model, q: &Query, func: &str, column: &str, d: Dialect) -> Result<Sql, String> {
    let f = match func {
        "count" => "COUNT",
        "sum" => "SUM",
        "avg" => "AVG",
        "min" => "MIN",
        "max" => "MAX",
        _ => return Err(format!("unknown aggregate `{func}`")),
    };
    let mut sql = Sql::new(d);
    // A limit or an offset applies to the rows, not to the result.
    let inner = q.limit.is_some() || q.offset.is_some();
    sql.push("SELECT ").push(f).push("(");
    if column == "*" {
        sql.push("*");
    } else {
        sql.ident(column);
    }
    sql.push(")");
    if func == "avg" && d == Dialect::Postgres {
        sql.push("::DOUBLE PRECISION");
    }
    if inner {
        let sub = select_sql(m, q, d);
        let start = sql.params.len();
        let _ = start;
        sql.push(" FROM (").push(&sub.text).push(") AS sub");
        sql.params = sub.params;
    } else {
        sql.push(" FROM ").ident(&m.table);
        write_where(&mut sql, q);
    }
    Ok(sql)
}

fn insert_sql(m: &Model, cols: &[String], cells: Vec<Cell>, d: Dialect) -> Sql {
    let mut sql = Sql::new(d);
    sql.push("INSERT INTO ").ident(&m.table);
    if cols.is_empty() {
        sql.push(" DEFAULT VALUES");
    } else {
        sql.push(" (");
        for (i, c) in cols.iter().enumerate() {
            if i > 0 {
                sql.push(", ");
            }
            sql.ident(c);
        }
        sql.push(") VALUES (");
        for (i, v) in cells.into_iter().enumerate() {
            if i > 0 {
                sql.push(", ");
            }
            sql.param(v);
        }
        sql.push(")");
    }
    sql.push(" RETURNING ");
    for (i, c) in m.columns().iter().enumerate() {
        if i > 0 {
            sql.push(", ");
        }
        sql.ident(c);
    }
    sql
}

fn update_sql(m: &Model, q: &Query, cols: &[String], cells: Vec<Cell>, d: Dialect) -> Sql {
    let mut sql = Sql::new(d);
    sql.push("UPDATE ").ident(&m.table).push(" SET ");
    for (i, (c, v)) in cols.iter().zip(cells).enumerate() {
        if i > 0 {
            sql.push(", ");
        }
        sql.ident(c).push(" = ").param(v);
    }
    write_filtered(&mut sql, m, q);
    sql
}

fn delete_sql(m: &Model, q: &Query, d: Dialect) -> Sql {
    let mut sql = Sql::new(d);
    sql.push("DELETE FROM ").ident(&m.table);
    write_filtered(&mut sql, m, q);
    sql
}

/// The rows of a query, for UPDATE and DELETE: with an order or a limit,
/// through their ids (neither database allows a LIMIT there).
fn write_filtered(sql: &mut Sql, m: &Model, q: &Query) {
    if q.limit.is_some() || q.offset.is_some() {
        sql.push(" WHERE \"id\" IN (SELECT \"id\" FROM ").ident(&m.table);
        write_where(sql, q);
        write_order_limit(sql, q);
        sql.push(")");
    } else {
        write_where(sql, q);
    }
}

/// A database error as a message: a unique field taken is said plainly.
fn message(e: String) -> String {
    let l = e.to_lowercase();
    if l.contains("unique") || l.contains("duplicate key") {
        return facile_core::util::tr(
            &format!("a value that must be unique is already taken ({e})"),
            &format!("une valeur qui doit être unique est déjà prise ({e})"),
        );
    }
    e
}

// ----- SQLite -----------------------------------------------------------

#[cfg(feature = "sqlite")]
mod lite {
    use super::*;
    use rusqlite::types::{ToSqlOutput, Value as SqlValue, ValueRef};
    use rusqlite::{params_from_iter, Connection, ToSql};

    impl ToSql for Cell {
        fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
            Ok(match self {
                Cell::Null => ToSqlOutput::Owned(SqlValue::Null),
                Cell::Int(n) => ToSqlOutput::Owned(SqlValue::Integer(*n)),
                Cell::Float(f) => ToSqlOutput::Owned(SqlValue::Real(*f)),
                Cell::Text(s) => ToSqlOutput::Borrowed(ValueRef::Text(s.as_bytes())),
                Cell::Bool(b) => ToSqlOutput::Owned(SqlValue::Integer(*b as i64)),
            })
        }
    }

    fn read(v: ValueRef<'_>, kind: Option<Kind>) -> Cell {
        match (v, kind) {
            (ValueRef::Null, _) => Cell::Null,
            (ValueRef::Integer(n), Some(Kind::Bool)) => Cell::Bool(n != 0),
            (ValueRef::Integer(n), Some(Kind::Float)) => Cell::Float(n as f64),
            (ValueRef::Integer(n), _) => Cell::Int(n),
            (ValueRef::Real(f), _) => Cell::Float(f),
            (ValueRef::Text(t), _) => Cell::Text(String::from_utf8_lossy(t).into_owned()),
            (ValueRef::Blob(b), _) => Cell::Text(String::from_utf8_lossy(b).into_owned()),
        }
    }

    pub fn open(path: &Path) -> Result<Db, String> {
        let c = Connection::open(path).map_err(|e| format!("SQLite {}: {e}", path.display()))?;
        c.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;").map_err(|e| e.to_string())?;
        c.busy_timeout(std::time::Duration::from_secs(5)).map_err(|e| e.to_string())?;
        Ok(Db::Sqlite(c))
    }

    pub fn migrate(c: &mut Connection, m: &Model) -> Result<(), String> {
        let existing: Vec<String> = {
            let mut st = c.prepare(&format!("PRAGMA table_info(\"{}\")", m.table)).map_err(|e| e.to_string())?;
            let names = st.query_map([], |r| r.get::<_, String>(1)).map_err(|e| e.to_string())?;
            names.flatten().collect()
        };
        let statements = if existing.is_empty() {
            create_table(m, Dialect::Sqlite)
        } else {
            let mut s = add_columns(m, &existing, Dialect::Sqlite);
            s.extend(indexes(m));
            s
        };
        for s in statements {
            c.execute_batch(&s).map_err(|e| format!("{s}: {e}"))?;
        }
        Ok(())
    }

    fn rows(c: &Connection, sql: &Sql, kinds: &[Option<Kind>]) -> Result<Vec<Row>, String> {
        let mut st = c.prepare_cached(&sql.text).map_err(|e| e.to_string())?;
        let n = st.column_count();
        let mut out = Vec::new();
        let mut rs = st.query(params_from_iter(sql.params.iter())).map_err(|e| message(e.to_string()))?;
        while let Some(r) = rs.next().map_err(|e| e.to_string())? {
            let mut row = Vec::with_capacity(n);
            for i in 0..n {
                row.push(read(r.get_ref(i).map_err(|e| e.to_string())?, kinds.get(i).copied().flatten()));
            }
            out.push(row);
        }
        Ok(out)
    }

    fn kinds(m: &Model) -> Vec<Option<Kind>> {
        m.columns().iter().map(|c| m.kind_of(c)).collect()
    }

    pub fn select(c: &mut Connection, m: &Model, q: &Query) -> Result<Vec<Row>, String> {
        rows(c, &select_sql(m, q, Dialect::Sqlite), &kinds(m))
    }

    pub fn aggregate(c: &mut Connection, m: &Model, q: &Query, func: &str, column: &str) -> Result<Cell, String> {
        let sql = aggregate_sql(m, q, func, column, Dialect::Sqlite)?;
        let kind = if func == "avg" { Some(Kind::Float) } else { m.kind_of(column) };
        Ok(rows(c, &sql, &[kind])?.into_iter().next().and_then(|r| r.into_iter().next()).unwrap_or(Cell::Null))
    }

    pub fn insert(c: &mut Connection, m: &Model, cols: &[String], cells: Vec<Cell>) -> Result<Row, String> {
        rows(c, &insert_sql(m, cols, cells, Dialect::Sqlite), &kinds(m))?.into_iter().next().ok_or_else(|| "no row inserted".into())
    }

    fn execute(c: &Connection, sql: &Sql) -> Result<i64, String> {
        let mut st = c.prepare_cached(&sql.text).map_err(|e| e.to_string())?;
        st.execute(params_from_iter(sql.params.iter())).map(|n| n as i64).map_err(|e| message(e.to_string()))
    }

    pub fn update(c: &mut Connection, m: &Model, q: &Query, cols: &[String], cells: Vec<Cell>) -> Result<i64, String> {
        execute(c, &update_sql(m, q, cols, cells, Dialect::Sqlite))
    }

    pub fn delete(c: &mut Connection, m: &Model, q: &Query) -> Result<i64, String> {
        execute(c, &delete_sql(m, q, Dialect::Sqlite))
    }

    pub fn raw(c: &mut Connection, text: &str, params: Vec<Cell>) -> Result<(Vec<String>, Vec<Row>), String> {
        let mut st = c.prepare(text).map_err(|e| e.to_string())?;
        let names: Vec<String> = st.column_names().iter().map(|s| s.to_string()).collect();
        let n = names.len();
        let mut out = Vec::new();
        let mut rs = st.query(params_from_iter(params.iter())).map_err(|e| message(e.to_string()))?;
        while let Some(r) = rs.next().map_err(|e| e.to_string())? {
            let mut row = Vec::with_capacity(n);
            for i in 0..n {
                row.push(read(r.get_ref(i).map_err(|e| e.to_string())?, None));
            }
            out.push(row);
        }
        Ok((names, out))
    }
}

#[cfg(feature = "sqlite")]
pub use lite::{
    aggregate as aggregate_sqlite, delete as delete_sqlite, insert as insert_sqlite, migrate as migrate_sqlite, open as open_sqlite,
    raw as raw_sqlite, select as select_sqlite, update as update_sqlite,
};

// ----- PostgreSQL -------------------------------------------------------

#[cfg(feature = "postgres")]
mod pg {
    use super::*;
    use postgres::types::{to_sql_checked, IsNull, ToSql, Type};
    use postgres::Client;

    impl ToSql for Cell {
        fn to_sql(
            &self,
            ty: &Type,
            out: &mut postgres::types::private::BytesMut,
        ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
            match self {
                Cell::Null => Ok(IsNull::Yes),
                Cell::Int(n) => match *ty {
                    Type::INT2 => (*n as i16).to_sql(ty, out),
                    Type::INT4 => (*n as i32).to_sql(ty, out),
                    Type::FLOAT8 => (*n as f64).to_sql(ty, out),
                    Type::FLOAT4 => (*n as f32).to_sql(ty, out),
                    Type::TEXT | Type::VARCHAR => n.to_string().to_sql(ty, out),
                    Type::BOOL => (*n != 0).to_sql(ty, out),
                    _ => n.to_sql(ty, out),
                },
                Cell::Float(f) => match *ty {
                    Type::FLOAT4 => (*f as f32).to_sql(ty, out),
                    Type::INT8 => (*f as i64).to_sql(ty, out),
                    Type::INT4 => (*f as i32).to_sql(ty, out),
                    Type::TEXT | Type::VARCHAR => f.to_string().to_sql(ty, out),
                    _ => f.to_sql(ty, out),
                },
                Cell::Text(s) => match *ty {
                    Type::INT8 => s.parse::<i64>()?.to_sql(ty, out),
                    Type::INT4 => s.parse::<i32>()?.to_sql(ty, out),
                    Type::FLOAT8 => s.parse::<f64>()?.to_sql(ty, out),
                    Type::BOOL => (s == "true").to_sql(ty, out),
                    _ => s.as_str().to_sql(ty, out),
                },
                Cell::Bool(b) => match *ty {
                    Type::INT8 => (*b as i64).to_sql(ty, out),
                    Type::INT4 => (*b as i32).to_sql(ty, out),
                    Type::TEXT | Type::VARCHAR => b.to_string().to_sql(ty, out),
                    _ => b.to_sql(ty, out),
                },
            }
        }

        fn accepts(_: &Type) -> bool {
            true
        }

        to_sql_checked!();
    }

    fn read(r: &postgres::Row, i: usize) -> Cell {
        let ty = r.columns()[i].type_().clone();
        let cell = match ty {
            Type::INT8 => r.try_get::<_, Option<i64>>(i).ok().flatten().map(Cell::Int),
            Type::INT4 => r.try_get::<_, Option<i32>>(i).ok().flatten().map(|n| Cell::Int(n as i64)),
            Type::INT2 => r.try_get::<_, Option<i16>>(i).ok().flatten().map(|n| Cell::Int(n as i64)),
            Type::FLOAT8 => r.try_get::<_, Option<f64>>(i).ok().flatten().map(Cell::Float),
            Type::FLOAT4 => r.try_get::<_, Option<f32>>(i).ok().flatten().map(|f| Cell::Float(f as f64)),
            Type::BOOL => r.try_get::<_, Option<bool>>(i).ok().flatten().map(Cell::Bool),
            _ => r.try_get::<_, Option<String>>(i).ok().flatten().map(Cell::Text),
        };
        cell.unwrap_or(Cell::Null)
    }

    fn params(cells: &[Cell]) -> Vec<&(dyn ToSql + Sync)> {
        cells.iter().map(|c| c as &(dyn ToSql + Sync)).collect()
    }

    pub fn open(url: &str) -> Result<Db, String> {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let tls_config = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .with_root_certificates(roots)
            .with_no_client_auth();
        // Heroku Postgres asks for TLS with a certificate of its own: `sslmode=require`
        // means encrypted, as libpq does, so certificates are not checked then.
        let require_only = url.contains("sslmode=require") || std::env::var("DYNO").is_ok();
        let tls = if require_only {
            let mut c = tls_config;
            c.dangerous().set_certificate_verifier(std::sync::Arc::new(NoVerify));
            tokio_postgres_rustls::MakeRustlsConnect::new(c)
        } else {
            tokio_postgres_rustls::MakeRustlsConnect::new(tls_config)
        };
        let client = Client::connect(url, tls).map_err(|e| format!("PostgreSQL: {e}"))?;
        Ok(Db::Postgres(Box::new(client)))
    }

    /// `sslmode=require`: encrypted, without checking who answers (as libpq does).
    #[derive(Debug)]
    struct NoVerify;

    impl rustls::client::danger::ServerCertVerifier for NoVerify {
        fn verify_server_cert(
            &self,
            _: &rustls::pki_types::CertificateDer<'_>,
            _: &[rustls::pki_types::CertificateDer<'_>],
            _: &rustls::pki_types::ServerName<'_>,
            _: &[u8],
            _: rustls::pki_types::UnixTime,
        ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _: &[u8],
            _: &rustls::pki_types::CertificateDer<'_>,
            _: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _: &[u8],
            _: &rustls::pki_types::CertificateDer<'_>,
            _: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            rustls::crypto::ring::default_provider().signature_verification_algorithms.supported_schemes()
        }
    }

    pub fn migrate(c: &mut Client, m: &Model) -> Result<(), String> {
        let existing: Vec<String> = c
            .query(
                "SELECT column_name::TEXT FROM information_schema.columns WHERE table_name = $1 AND table_schema = current_schema()",
                &[&m.table],
            )
            .map_err(|e| e.to_string())?
            .iter()
            .map(|r| r.get::<_, String>(0))
            .collect();
        let statements = if existing.is_empty() {
            create_table(m, Dialect::Postgres)
        } else {
            let mut s = add_columns(m, &existing, Dialect::Postgres);
            s.extend(indexes(m));
            s
        };
        for s in statements {
            c.batch_execute(&s).map_err(|e| format!("{s}: {e}"))?;
        }
        Ok(())
    }

    fn rows(c: &mut Client, sql: &Sql) -> Result<Vec<Row>, String> {
        let st = c.prepare(&sql.text).map_err(|e| e.to_string())?;
        let rs = c.query(&st, &params(&sql.params)).map_err(|e| message(e.to_string()))?;
        Ok(rs.iter().map(|r| (0..r.len()).map(|i| read(r, i)).collect()).collect())
    }

    pub fn select(c: &mut Client, m: &Model, q: &Query) -> Result<Vec<Row>, String> {
        rows(c, &select_sql(m, q, Dialect::Postgres))
    }

    pub fn aggregate(c: &mut Client, m: &Model, q: &Query, func: &str, column: &str) -> Result<Cell, String> {
        let sql = aggregate_sql(m, q, func, column, Dialect::Postgres)?;
        let cell = rows(c, &sql)?.into_iter().next().and_then(|r| r.into_iter().next()).unwrap_or(Cell::Null);
        // SUM of BIGINT is NUMERIC, read as text: given back as a number.
        Ok(match cell {
            Cell::Text(t) if func != "min" && func != "max" => {
                t.parse::<i64>().map(Cell::Int).or_else(|_| t.parse::<f64>().map(Cell::Float)).unwrap_or(Cell::Text(t))
            }
            c => c,
        })
    }

    pub fn insert(c: &mut Client, m: &Model, cols: &[String], cells: Vec<Cell>) -> Result<Row, String> {
        rows(c, &insert_sql(m, cols, cells, Dialect::Postgres))?.into_iter().next().ok_or_else(|| "no row inserted".into())
    }

    fn execute(c: &mut Client, sql: &Sql) -> Result<i64, String> {
        c.execute(sql.text.as_str(), &params(&sql.params)).map(|n| n as i64).map_err(|e| message(e.to_string()))
    }

    pub fn update(c: &mut Client, m: &Model, q: &Query, cols: &[String], cells: Vec<Cell>) -> Result<i64, String> {
        execute(c, &update_sql(m, q, cols, cells, Dialect::Postgres))
    }

    pub fn delete(c: &mut Client, m: &Model, q: &Query) -> Result<i64, String> {
        execute(c, &delete_sql(m, q, Dialect::Postgres))
    }

    pub fn raw(c: &mut Client, text: &str, ps: Vec<Cell>) -> Result<(Vec<String>, Vec<Row>), String> {
        let st = c.prepare(text).map_err(|e| e.to_string())?;
        let names = st.columns().iter().map(|c| c.name().to_string()).collect();
        let rs = c.query(&st, &params(&ps)).map_err(|e| message(e.to_string()))?;
        Ok((names, rs.iter().map(|r| (0..r.len()).map(|i| read(r, i)).collect()).collect()))
    }
}

#[cfg(feature = "postgres")]
pub use pg::{
    aggregate as aggregate_postgres, delete as delete_postgres, insert as insert_postgres, migrate as migrate_postgres,
    open as open_postgres, raw as raw_postgres, select as select_postgres, update as update_postgres,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orm::query::{Cond, Filter, Op};

    fn model() -> Model {
        Model { table: "taches".into(), fields: vec![], has_many: vec![], timestamps: false }
    }

    #[test]
    fn writes_parameters_per_dialect() {
        let q = Query {
            filters: vec![Filter::One(Cond { column: "id".into(), op: Op::Gt, value: Cell::Int(3), values: vec![] })],
            limit: Some(5),
            ..Query::default()
        };
        let s = select_sql(&model(), &q, Dialect::Postgres);
        assert_eq!(s.text, "SELECT \"id\" FROM \"taches\" WHERE \"id\" > $1 ORDER BY \"id\" LIMIT 5");
        let s = select_sql(&model(), &q, Dialect::Sqlite);
        assert_eq!(s.text, "SELECT \"id\" FROM \"taches\" WHERE \"id\" > ? ORDER BY \"id\" LIMIT 5");
        let s = delete_sql(&model(), &q, Dialect::Sqlite);
        assert!(s.text.starts_with("DELETE FROM \"taches\" WHERE \"id\" IN (SELECT"), "{}", s.text);
    }
}
