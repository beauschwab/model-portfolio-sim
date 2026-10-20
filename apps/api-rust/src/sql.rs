//! One portable SQL surface over SQLite and PostgreSQL. Statements are written once with
//! `$n` placeholders; SQLite receives explicit `?n` numbering, and PostgreSQL parameters are
//! bound with the column types the server reports, so the Rust service reads and writes the
//! tables the existing SQLAlchemy metadata created without a second schema dialect.
use serde_json::Value;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq)]
pub enum Sql {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
    Json(Value),
    Bool(bool),
}

impl Sql {
    pub fn text(&self) -> Option<&str> {
        match self {
            Sql::Text(s) => Some(s),
            _ => None,
        }
    }
    pub fn int(&self) -> Option<i64> {
        match self {
            Sql::Int(i) => Some(*i),
            _ => None,
        }
    }
    pub fn float(&self) -> Option<f64> {
        match self {
            Sql::Float(f) => Some(*f),
            Sql::Int(i) => Some(*i as f64),
            _ => None,
        }
    }
    /// JSON columns are text on SQLite and `json` on PostgreSQL.
    pub fn json(&self) -> Result<Option<Value>, String> {
        match self {
            Sql::Null => Ok(None),
            Sql::Json(v) => Ok(Some(v.clone())),
            Sql::Text(s) => serde_json::from_str(s)
                .map(Some)
                .map_err(|e| format!("stored JSON column is invalid: {e}")),
            other => Err(format!("column is not JSON: {other:?}")),
        }
    }
}

impl From<&str> for Sql {
    fn from(v: &str) -> Self {
        Sql::Text(v.to_string())
    }
}
impl From<String> for Sql {
    fn from(v: String) -> Self {
        Sql::Text(v)
    }
}
impl From<i64> for Sql {
    fn from(v: i64) -> Self {
        Sql::Int(v)
    }
}
impl From<f64> for Sql {
    fn from(v: f64) -> Self {
        Sql::Float(v)
    }
}
impl From<Value> for Sql {
    fn from(v: Value) -> Self {
        Sql::Json(v)
    }
}
impl<T: Into<Sql>> From<Option<T>> for Sql {
    fn from(v: Option<T>) -> Self {
        v.map(Into::into).unwrap_or(Sql::Null)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Sqlite,
    Postgres,
}

pub type Row = Vec<Sql>;

pub enum Conn {
    Sqlite(rusqlite::Connection),
    Postgres(Box<postgres::Client>),
}

/// The selected database. `DATABASE_URL` uses the same spellings as the Python service:
/// `sqlite:///path`, `postgresql://...`, `postgres://...` or `postgresql+psycopg://...`.
pub struct Db {
    url: String,
    pub dialect: Dialect,
    idle: Mutex<Vec<Conn>>,
}

fn sqlite_path(url: &str) -> Result<String, String> {
    let rest = url
        .strip_prefix("sqlite:///")
        .ok_or("SQLite URLs use sqlite:///<path>")?;
    if rest.is_empty() || rest == ":memory:" {
        return Err("the durable service needs a file database".into());
    }
    Ok(rest.to_string())
}

impl Db {
    pub fn open(url: &str) -> Result<Db, String> {
        let dialect = if url.starts_with("sqlite:") {
            let path = sqlite_path(url)?;
            if let Some(parent) = std::path::Path::new(&path).parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
            }
            Dialect::Sqlite
        } else if ["postgres://", "postgresql://", "postgresql+psycopg://"]
            .iter()
            .any(|p| url.starts_with(p))
        {
            Dialect::Postgres
        } else {
            return Err("DATABASE_URL must select SQLite or PostgreSQL".into());
        };
        let db = Db {
            url: url.to_string(),
            dialect,
            idle: Mutex::new(Vec::new()),
        };
        drop(db.connect()?);
        Ok(db)
    }

    fn connect(&self) -> Result<Conn, String> {
        match self.dialect {
            Dialect::Sqlite => {
                let c = rusqlite::Connection::open(sqlite_path(&self.url)?)
                    .map_err(|e| e.to_string())?;
                c.busy_timeout(std::time::Duration::from_secs(30))
                    .map_err(|e| e.to_string())?;
                c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
                    .map_err(|e| e.to_string())?;
                Ok(Conn::Sqlite(c))
            }
            Dialect::Postgres => {
                let url = self
                    .url
                    .replacen("postgresql+psycopg://", "postgresql://", 1);
                let client = postgres::Client::connect(&url, postgres::NoTls)
                    .map_err(|e| format!("PostgreSQL connection failed: {e}"))?;
                Ok(Conn::Postgres(Box::new(client)))
            }
        }
    }

    fn take(&self) -> Result<Conn, String> {
        let pooled = self.idle.lock().expect("pool").pop();
        match pooled {
            Some(c) => Ok(c),
            None => self.connect(),
        }
    }
    fn give(&self, conn: Conn) {
        let mut idle = self.idle.lock().expect("pool");
        if idle.len() < 8 {
            idle.push(conn);
        }
    }

    /// Runs `body` in one write transaction. SQLite takes its write lock at `BEGIN
    /// IMMEDIATE`; PostgreSQL serializes through the scoped workspace row lock the callers take.
    /// Any error rolls back, and the connection is discarded rather than reused.
    pub fn write<T, E: From<String>>(
        &self,
        body: impl FnOnce(&mut Tx) -> Result<T, E>,
    ) -> Result<T, E> {
        let mut conn = self.take().map_err(E::from)?;
        let result = match &mut conn {
            Conn::Sqlite(c) => {
                let tx = c
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                    .map_err(|e| E::from(e.to_string()))?;
                let mut handle = Tx::Sqlite(tx);
                let out = body(&mut handle);
                match (out, handle) {
                    (Ok(v), Tx::Sqlite(tx)) => {
                        tx.commit().map(|_| v).map_err(|e| E::from(e.to_string()))
                    }
                    (Err(e), Tx::Sqlite(tx)) => {
                        let _ = tx.rollback();
                        Err(e)
                    }
                    _ => unreachable!(),
                }
            }
            Conn::Postgres(c) => {
                let tx = c.transaction().map_err(|e| E::from(e.to_string()))?;
                let mut handle = Tx::Postgres(tx);
                let out = body(&mut handle);
                match (out, handle) {
                    (Ok(v), Tx::Postgres(tx)) => {
                        tx.commit().map(|_| v).map_err(|e| E::from(e.to_string()))
                    }
                    (Err(e), Tx::Postgres(tx)) => {
                        let _ = tx.rollback();
                        Err(e)
                    }
                    _ => unreachable!(),
                }
            }
        };
        if result.is_ok() {
            self.give(conn);
        }
        result
    }

    /// A read-only statement outside any write transaction.
    pub fn read(&self, sql: &str, params: &[Sql]) -> Result<Vec<Row>, String> {
        self.write(|tx| tx.rows(sql, params))
    }
}

pub enum Tx<'a> {
    Sqlite(rusqlite::Transaction<'a>),
    Postgres(postgres::Transaction<'a>),
}

fn numbered(sql: &str) -> String {
    // `$n` -> `?n`: SQLite's named `$n` parameters would otherwise be numbered by first use.
    let mut out = String::with_capacity(sql.len());
    let mut chars = sql.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' && chars.peek().is_some_and(|d| d.is_ascii_digit()) {
            out.push('?');
        } else {
            out.push(c);
        }
    }
    out
}

fn sqlite_param(v: &Sql) -> rusqlite::types::Value {
    use rusqlite::types::Value as V;
    match v {
        Sql::Null => V::Null,
        Sql::Int(i) => V::Integer(*i),
        Sql::Float(f) => V::Real(*f),
        Sql::Text(s) => V::Text(s.clone()),
        Sql::Json(j) => V::Text(serde_json::to_string(j).expect("JSON value serializes")),
        Sql::Bool(b) => V::Integer(i64::from(*b)),
    }
}

fn pg_param(
    v: &Sql,
    ty: &postgres::types::Type,
) -> Result<Box<dyn postgres::types::ToSql + Sync>, String> {
    use postgres::types::Type;
    let int = |i: i64| -> Result<Box<dyn postgres::types::ToSql + Sync>, String> {
        match *ty {
            Type::INT2 => Ok(Box::new(i16::try_from(i).map_err(|e| e.to_string())?)),
            Type::INT4 => Ok(Box::new(i32::try_from(i).map_err(|e| e.to_string())?)),
            Type::INT8 => Ok(Box::new(i)),
            Type::FLOAT8 => Ok(Box::new(i as f64)),
            Type::FLOAT4 => Ok(Box::new(i as f32)),
            _ => Err(format!("integer parameter for column type {ty}")),
        }
    };
    Ok(match v {
        Sql::Null => match *ty {
            Type::INT2 => Box::new(None::<i16>),
            Type::INT4 => Box::new(None::<i32>),
            Type::INT8 => Box::new(None::<i64>),
            Type::FLOAT4 => Box::new(None::<f32>),
            Type::FLOAT8 => Box::new(None::<f64>),
            Type::BOOL => Box::new(None::<bool>),
            Type::JSON | Type::JSONB => Box::new(None::<Value>),
            _ => Box::new(None::<String>),
        },
        Sql::Int(i) => int(*i)?,
        Sql::Float(f) => match *ty {
            Type::FLOAT8 => Box::new(*f),
            Type::FLOAT4 => Box::new(*f as f32),
            _ => return Err(format!("float parameter for column type {ty}")),
        },
        Sql::Text(s) => match *ty {
            Type::JSON | Type::JSONB => Box::new(
                serde_json::from_str::<Value>(s).map_err(|e| format!("JSON parameter: {e}"))?,
            ),
            _ => Box::new(s.clone()),
        },
        Sql::Json(j) => Box::new(j.clone()),
        Sql::Bool(b) => Box::new(*b),
    })
}

fn pg_value(row: &postgres::Row, i: usize) -> Result<Sql, String> {
    use postgres::types::Type;
    let ty = row.columns()[i].type_().clone();
    let e = |e: postgres::Error| e.to_string();
    Ok(match ty {
        Type::INT2 => row
            .try_get::<_, Option<i16>>(i)
            .map_err(e)?
            .map_or(Sql::Null, |v| Sql::Int(v.into())),
        Type::INT4 => row
            .try_get::<_, Option<i32>>(i)
            .map_err(e)?
            .map_or(Sql::Null, |v| Sql::Int(v.into())),
        Type::INT8 => row
            .try_get::<_, Option<i64>>(i)
            .map_err(e)?
            .map_or(Sql::Null, Sql::Int),
        Type::FLOAT4 => row
            .try_get::<_, Option<f32>>(i)
            .map_err(e)?
            .map_or(Sql::Null, |v| Sql::Float(v.into())),
        Type::FLOAT8 => row
            .try_get::<_, Option<f64>>(i)
            .map_err(e)?
            .map_or(Sql::Null, Sql::Float),
        Type::BOOL => row
            .try_get::<_, Option<bool>>(i)
            .map_err(e)?
            .map_or(Sql::Null, Sql::Bool),
        Type::JSON | Type::JSONB => row
            .try_get::<_, Option<Value>>(i)
            .map_err(e)?
            .map_or(Sql::Null, Sql::Json),
        _ => row
            .try_get::<_, Option<String>>(i)
            .map_err(e)?
            .map_or(Sql::Null, Sql::Text),
    })
}

impl Tx<'_> {
    pub fn dialect(&self) -> Dialect {
        match self {
            Tx::Sqlite(_) => Dialect::Sqlite,
            Tx::Postgres(_) => Dialect::Postgres,
        }
    }

    pub fn exec(&mut self, sql: &str, params: &[Sql]) -> Result<u64, String> {
        match self {
            Tx::Sqlite(tx) => {
                let values: Vec<_> = params.iter().map(sqlite_param).collect();
                tx.execute(&numbered(sql), rusqlite::params_from_iter(values))
                    .map(|n| n as u64)
                    .map_err(|e| e.to_string())
            }
            Tx::Postgres(tx) => {
                let stmt = tx.prepare(sql).map_err(|e| e.to_string())?;
                let boxed = params
                    .iter()
                    .zip(stmt.params())
                    .map(|(v, t)| pg_param(v, t))
                    .collect::<Result<Vec<_>, _>>()?;
                let refs: Vec<&(dyn postgres::types::ToSql + Sync)> =
                    boxed.iter().map(|b| b.as_ref()).collect();
                tx.execute(&stmt, &refs).map_err(|e| e.to_string())
            }
        }
    }

    pub fn batch(&mut self, sql: &str) -> Result<(), String> {
        match self {
            Tx::Sqlite(tx) => tx.execute_batch(sql).map_err(|e| e.to_string()),
            Tx::Postgres(tx) => tx.batch_execute(sql).map_err(|e| e.to_string()),
        }
    }

    pub fn rows(&mut self, sql: &str, params: &[Sql]) -> Result<Vec<Row>, String> {
        match self {
            Tx::Sqlite(tx) => {
                let values: Vec<_> = params.iter().map(sqlite_param).collect();
                let mut stmt = tx.prepare(&numbered(sql)).map_err(|e| e.to_string())?;
                let width = stmt.column_count();
                let rows = stmt
                    .query_map(rusqlite::params_from_iter(values), |r| {
                        (0..width)
                            .map(|i| {
                                use rusqlite::types::ValueRef as R;
                                Ok(match r.get_ref(i)? {
                                    R::Null => Sql::Null,
                                    R::Integer(v) => Sql::Int(v),
                                    R::Real(v) => Sql::Float(v),
                                    R::Text(t) => Sql::Text(String::from_utf8_lossy(t).into()),
                                    R::Blob(b) => Sql::Text(String::from_utf8_lossy(b).into()),
                                })
                            })
                            .collect::<Result<Vec<_>, rusqlite::Error>>()
                    })
                    .map_err(|e| e.to_string())?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())
            }
            Tx::Postgres(tx) => {
                let stmt = tx.prepare(sql).map_err(|e| e.to_string())?;
                let boxed = params
                    .iter()
                    .zip(stmt.params())
                    .map(|(v, t)| pg_param(v, t))
                    .collect::<Result<Vec<_>, _>>()?;
                let refs: Vec<&(dyn postgres::types::ToSql + Sync)> =
                    boxed.iter().map(|b| b.as_ref()).collect();
                let rows = tx.query(&stmt, &refs).map_err(|e| e.to_string())?;
                rows.iter()
                    .map(|r| (0..r.len()).map(|i| pg_value(r, i)).collect())
                    .collect()
            }
        }
    }

    pub fn one(&mut self, sql: &str, params: &[Sql]) -> Result<Option<Row>, String> {
        let mut rows = self.rows(sql, params)?;
        if rows.len() > 1 {
            return Err("query returned more than one row".into());
        }
        Ok(rows.pop())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn placeholders_are_numbered_explicitly_for_sqlite() {
        assert_eq!(
            super::numbered("a = $2 AND b = $1 AND c = $2"),
            "a = ?2 AND b = ?1 AND c = ?2"
        );
    }
}
