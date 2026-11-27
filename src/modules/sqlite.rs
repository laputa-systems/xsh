use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{Connection, OpenFlags, Statement};

use crate::runtime::value::{FloatValue, Value};

const MAX_RESULT_BYTES: usize = 16 * 1024 * 1024;
const MAX_RESULT_CELLS: usize = 1_000_000;

#[derive(Clone, Copy)]
pub(crate) enum OpenMode {
    ReadOnly,
    ReadWrite,
    Create,
}

pub(crate) struct Table {
    pub(crate) columns: Vec<String>,
    pub(crate) rows: Vec<Vec<Value>>,
}

#[derive(Debug)]
pub(crate) enum Error {
    Invalid(&'static str),
    InvalidData(&'static str),
    Limit,
    Engine(rusqlite::Error),
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self { Self::Engine(error) }
}

pub(crate) struct Database {
    connection: Connection,
}

impl Database {
    pub(crate) fn open(path: &Path, mode: OpenMode, timeout: Duration) -> Result<Self, Error> {
        check_timeout(timeout)?;
        let flags = match mode {
            OpenMode::ReadOnly => OpenFlags::SQLITE_OPEN_READ_ONLY,
            OpenMode::ReadWrite => OpenFlags::SQLITE_OPEN_READ_WRITE,
            OpenMode::Create => OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        };
        Self::configure(Connection::open_with_flags(path, flags)?, timeout)
    }

    pub(crate) fn memory(timeout: Duration) -> Result<Self, Error> {
        check_timeout(timeout)?;
        Self::configure(Connection::open_in_memory()?, timeout)
    }

    fn configure(connection: Connection, timeout: Duration) -> Result<Self, Error> {
        connection.busy_timeout(timeout)?;
        connection.execute_batch("PRAGMA foreign_keys = ON")?;
        Ok(Self { connection })
    }

    pub(crate) fn set_interrupt(&mut self, interrupt: impl FnMut() -> bool + Send + 'static) -> Result<(), Error> {
        self.connection.progress_handler(1000, Some(interrupt))?;
        Ok(())
    }

    pub(crate) fn clear_interrupt(&mut self) -> Result<(), Error> {
        self.connection.progress_handler(0, None::<fn() -> bool>)?;
        Ok(())
    }

    pub(crate) fn execute(&self, sql: &str, params: &[Value]) -> Result<u64, Error> {
        let values = bind_values(params)?;
        let mut statement = self.connection.prepare(sql)?;
        if statement.column_count() != 0 {
            return Err(Error::Invalid("execute requires a statement without result columns"));
        }
        bind(&mut statement, &values)?;
        statement.raw_execute()?;
        Ok(self.connection.changes())
    }

    pub(crate) fn query(&self, sql: &str, params: &[Value]) -> Result<Table, Error> {
        let values = bind_values(params)?;
        let mut statement = self.connection.prepare(sql)?;
        let count = statement.column_count();
        if count == 0 {
            return Err(Error::Invalid("query requires a statement with result columns"));
        }
        bind(&mut statement, &values)?;
        let mut bytes = 0usize;
        let mut columns = Vec::with_capacity(count);
        for index in 0..count {
            let name = statement.column_name(index)?;
            bytes = bytes.checked_add(name.len()).ok_or(Error::Limit)?;
            if bytes > MAX_RESULT_BYTES { return Err(Error::Limit); }
            columns.push(name.to_string());
        }
        let mut rows = Vec::new();
        let mut result_cells = 0usize;
        let mut cursor = statement.raw_query();
        while let Some(row) = cursor.next()? {
            result_cells = result_cells.checked_add(count).ok_or(Error::Limit)?;
            if result_cells > MAX_RESULT_CELLS { return Err(Error::Limit); }
            let mut cells = Vec::with_capacity(count);
            for index in 0..count {
                let cell = match row.get_ref(index)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(value) => Value::Int(value),
                    ValueRef::Real(value) if value.is_finite() => Value::Float(FloatValue(value)),
                    ValueRef::Real(_) => return Err(Error::InvalidData("non-finite REAL")),
                    ValueRef::Text(value) => {
                        bytes = bytes.checked_add(value.len()).ok_or(Error::Limit)?;
                        if bytes > MAX_RESULT_BYTES { return Err(Error::Limit); }
                        let value = std::str::from_utf8(value).map_err(|_| Error::InvalidData("TEXT is not UTF-8"))?;
                        Value::Str(Arc::from(value))
                    }
                    ValueRef::Blob(value) => {
                        bytes = bytes.checked_add(value.len()).ok_or(Error::Limit)?;
                        if bytes > MAX_RESULT_BYTES { return Err(Error::Limit); }
                        Value::Bytes(value.to_vec())
                    }
                };
                cells.push(cell);
            }
            rows.push(cells);
        }
        Ok(Table { columns, rows })
    }

    pub(crate) fn in_transaction(&self) -> bool { !self.connection.is_autocommit() }

    pub(crate) fn close(self) -> Result<(), (Self, rusqlite::Error)> {
        self.connection.close().map_err(|(connection, error)| (Self { connection }, error))
    }
}

fn check_timeout(timeout: Duration) -> Result<(), Error> {
    if timeout.as_millis() > i32::MAX as u128 {
        return Err(Error::Invalid("busy_timeout exceeds SQLite's maximum"));
    }
    Ok(())
}

fn bind_values(params: &[Value]) -> Result<Vec<SqlValue>, Error> {
    params.iter().map(|value| match value {
        Value::Null => Ok(SqlValue::Null),
        Value::Bool(value) => Ok(SqlValue::Integer(i64::from(*value))),
        Value::Int(value) => Ok(SqlValue::Integer(*value)),
        Value::Float(value) if value.0.is_finite() => Ok(SqlValue::Real(value.0)),
        Value::Float(_) => Err(Error::Invalid("non-finite Float parameter")),
        Value::Str(value) => Ok(SqlValue::Text(value.to_string())),
        Value::Bytes(value) => Ok(SqlValue::Blob(value.clone())),
        _ => Err(Error::Invalid("unsupported parameter type")),
    }).collect()
}

fn bind(statement: &mut Statement<'_>, values: &[SqlValue]) -> Result<(), Error> {
    let count = statement.parameter_count();
    if count != values.len() {
        return Err(Error::Invalid("parameter count does not match statement"));
    }
    for index in 1..=count {
        if statement.parameter_name(index).is_some_and(|name| !name.starts_with('?')) {
            return Err(Error::Invalid("named parameters are not supported"));
        }
    }
    for (index, value) in values.iter().enumerate() {
        statement.raw_bind_parameter(index + 1, value)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Database, Error, bind_values};
    use crate::runtime::value::{FloatValue, Value};
    use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
    use std::time::Duration;

    #[test]
    fn invalid_sqlite_text_is_rejected_without_conversion() {
        let db = Database::memory(Duration::ZERO).unwrap();
        let error = match db.query("SELECT CAST(x'80' AS TEXT)", &[]) {
            Err(error) => error,
            Ok(_) => panic!("invalid UTF-8 was accepted"),
        };
        assert!(matches!(error, Error::InvalidData("TEXT is not UTF-8")));
    }

    #[test]
    fn dynamic_parameter_boundary_rejects_unsupported_and_nonfinite_values() {
        assert!(matches!(bind_values(&[Value::List(Vec::new())]), Err(Error::Invalid(_))));
        assert!(matches!(bind_values(&[Value::Float(FloatValue(f64::INFINITY))]), Err(Error::Invalid(_))));
        assert!(matches!(bind_values(&[Value::Float(FloatValue(f64::NAN))]), Err(Error::Invalid(_))));
    }

    #[test]
    fn progress_callback_interrupts_before_first_row_and_connection_recovers() {
        let mut db = Database::memory(Duration::ZERO).unwrap();
        let called = Arc::new(AtomicBool::new(false));
        let seen = Arc::clone(&called);
        db.set_interrupt(move || { seen.store(true, Ordering::Relaxed); true }).unwrap();
        let result = db.query("WITH RECURSIVE n(x) AS (VALUES(0) UNION ALL SELECT x + 1 FROM n WHERE x < 10000000) SELECT sum(x) FROM n", &[]);
        assert!(result.is_err());
        assert!(called.load(Ordering::Relaxed));
        db.clear_interrupt().unwrap();
        assert!(db.query("SELECT 1", &[]).is_ok());
    }
}

pub(crate) fn failure(operation: &str, error: Error, span: crate::source::Span) -> crate::runtime::value::RuntimeError {
    use rusqlite::ffi::ErrorCode;
    use crate::runtime::value::{RecordMap, RuntimeError};

    let (variant, detail, code) = match error {
        Error::Invalid(detail) => ("InvalidArgument", detail.to_string(), None),
        Error::InvalidData(detail) => ("InvalidData", detail.to_string(), None),
        Error::Limit => ("Limit", "query result exceeds its materialization limit".to_string(), None),
        Error::Engine(error) => {
            let raw = match &error {
                rusqlite::Error::SqliteFailure(raw, _) | rusqlite::Error::SqlInputError { error: raw, .. } => Some(raw),
                _ => None,
            };
            match raw {
                Some(raw) => {
                    let variant = match raw.code {
                        ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => "Busy",
                        ErrorCode::ConstraintViolation => "Constraint",
                        ErrorCode::ReadOnly | ErrorCode::CannotOpen | ErrorCode::PermissionDenied => "Open",
                        ErrorCode::OperationInterrupted => "Canceled",
                        ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase | ErrorCode::TypeMismatch => "InvalidData",
                        ErrorCode::TooBig => "Limit",
                        _ => "Engine",
                    };
                    (variant, raw.to_string(), Some(raw.extended_code))
                }
                None => {
                    let detail = match error {
                        rusqlite::Error::MultipleStatement => "SQL contains multiple statements",
                        rusqlite::Error::Empty => "SQL contains no statement",
                        rusqlite::Error::NulError(_) => "SQL or path contains NUL",
                        _ => "SQLite wrapper error",
                    };
                    ("InvalidArgument", detail.to_string(), None)
                }
            }
        }
    };
    let message = format!("sqlite.{operation}: {detail}");
    let facets = xsh_registry::errors::SQLITE_ERROR_VARIANTS.iter()
        .find(|candidate| candidate.name == variant).expect("registered SqliteError variant")
        .facets.iter().map(|facet| facet.name().to_string()).collect();
    let payload = RecordMap::from([
        (Arc::from("message"), Value::Str(message.as_str().into())),
        (Arc::from("operation"), Value::Str(operation.into())),
        (Arc::from("code"), code.map_or(Value::Null, |code| Value::Int(i64::from(code & 255)))),
        (Arc::from("extended_code"), code.map_or(Value::Null, |code| Value::Int(i64::from(code)))),
    ]);
    RuntimeError::structured("SqliteError", variant, payload, facets, message).with_span(span)
}

pub(crate) fn closed(operation: &str, span: crate::source::Span) -> crate::runtime::value::RuntimeError {
    use crate::runtime::value::{RecordMap, RuntimeError};
    let message = format!("sqlite.{operation}: database handle is closed or belongs to another evaluator");
    let payload = RecordMap::from([
        (Arc::from("message"), Value::Str(message.as_str().into())),
        (Arc::from("operation"), Value::Str(operation.into())),
        (Arc::from("code"), Value::Null),
        (Arc::from("extended_code"), Value::Null),
    ]);
    RuntimeError::structured("SqliteError", "Closed", payload, Vec::new(), message).with_span(span)
}
