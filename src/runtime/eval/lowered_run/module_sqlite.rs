use std::sync::atomic::Ordering;

use super::{
    Arc, ControlFlow, Duration, Evaluator, LoweredValue, NativeArgumentValues, RuntimeError,
    RuntimeOp, Span, Value, lowered_duration_arg, lowered_path_arg, lowered_result_err_value,
    lowered_result_ok, lowered_runtime_value, lowered_str_arg_owned,
};
use crate::modules::sqlite::{self, Database, Error, OpenMode};
use crate::runtime::value::{RecordMap, SqliteDbValue};
use crate::symbol::Name;

fn mode(value: Option<&LoweredValue>, enum_name: &str, default: &'static str, span: Span) -> Result<String, RuntimeError> {
    match value {
        None => Ok(default.to_string()),
        Some(LoweredValue::Tag(tag)) if tag.type_name == Name::intern(enum_name) => Ok(tag.name.to_string()),
        _ => Err(RuntimeError::new("type-error", format!("expected {enum_name}")).with_span(span)),
    }
}

fn timeout(value: Option<&LoweredValue>, span: Span) -> Result<Duration, RuntimeError> {
    match value {
        None => Ok(Duration::from_secs(5)),
        Some(value) => Ok(Duration::from_millis(lowered_duration_arg(Some(value.clone()), "sqlite", span)?.millis)),
    }
}

fn parameters(value: Option<&LoweredValue>, span: Span) -> Result<Vec<Value>, RuntimeError> {
    match value {
        None => Ok(Vec::new()),
        Some(LoweredValue::List(values)) => Ok(values.iter().cloned().map(LoweredValue::into_value).collect()),
        Some(LoweredValue::SharedList(values)) => Ok(values.iter().cloned().map(LoweredValue::into_value).collect()),
        _ => Err(RuntimeError::new("type-error", "sqlite parameters must be a list").with_span(span)),
    }
}

impl Evaluator {
    pub(super) fn eval_lowered_sqlite_open(
        &mut self, op: RuntimeOp, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        self.service_pending_signal(span)?;
        let (operation, result) = if op == RuntimeOp::SqliteOpen {
            let path = lowered_path_arg(values[0].clone(), "sqlite.open", span)?;
            let mode = mode(values.get(1), xsh_registry::enums::SQLITE_OPEN_MODE, "Create", span)?;
            let mode = match mode.as_str() {
                "ReadOnly" => OpenMode::ReadOnly,
                "ReadWrite" => OpenMode::ReadWrite,
                "Create" => OpenMode::Create,
                _ => return Err(RuntimeError::new("type-error", "unknown sqlite open mode").with_span(span)),
            };
            ("open", Database::open(&self.host_path(&path), mode, timeout(values.get(2), span)?))
        } else {
            ("memory", Database::memory(timeout(values.first(), span)?))
        };
        let value = match result {
            Ok(database) => {
                let id = self.push_fs_capability(super::super::FsCapability::Sqlite(database));
                lowered_result_ok(LoweredValue::SqliteDb(SqliteDbValue {
                    id, owner: Arc::clone(&self.resource_owner),
                }))
            }
            Err(error) => lowered_result_err_value(sqlite::failure(operation, error, span)),
        };
        Ok(ControlFlow::Continue(value))
    }

    pub(super) fn eval_lowered_sqlite_call(
        &mut self, op: RuntimeOp, values: NativeArgumentValues, span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let operation = match op {
            RuntimeOp::SqliteExecute => "execute",
            RuntimeOp::SqliteQuery => "query",
            RuntimeOp::SqliteBegin => "begin",
            RuntimeOp::SqliteCommit => "commit",
            RuntimeOp::SqliteRollback => "rollback",
            RuntimeOp::SqliteInTransaction => "in_transaction",
            RuntimeOp::SqliteClose | RuntimeOp::SqliteCloseIfOpen => "close",
            _ => unreachable!(),
        };
        let index = match values.first() {
            Some(LoweredValue::SqliteDb(handle)) if Arc::ptr_eq(&handle.owner, &self.resource_owner) => {
                handle.id.checked_sub(1).and_then(|id| usize::try_from(id).ok())
            }
            _ => None,
        };
        let Some(index) = index.filter(|index| *index < self.fs_capabilities.len()) else {
            return Ok(ControlFlow::Continue(lowered_result_err_value(sqlite::closed(operation, span))));
        };
        if op == RuntimeOp::SqliteClose || op == RuntimeOp::SqliteCloseIfOpen {
            let Some(owned) = self.fs_capabilities[index].take() else {
                return Ok(ControlFlow::Continue(if op == RuntimeOp::SqliteCloseIfOpen {
                    lowered_result_ok(LoweredValue::Unit)
                } else { lowered_result_err_value(sqlite::closed(operation, span)) }));
            };
            let super::super::FsCapability::Sqlite(database) = owned.value else {
                self.fs_capabilities[index] = Some(owned);
                return Ok(ControlFlow::Continue(lowered_result_err_value(sqlite::closed(operation, span))));
            };
            return Ok(ControlFlow::Continue(match database.close() {
                Ok(()) => {
                    self.forget_owned_resource(owned.owner_scope, xsh_registry::managed_resource::ResourceKind::Capability);
                    lowered_result_ok(LoweredValue::Unit)
                }
                Err((database, error)) => {
                    self.fs_capabilities[index] = Some(super::super::OwnedFsCapability {
                        owner_scope: owned.owner_scope, value: super::super::FsCapability::Sqlite(database),
                    });
                    lowered_result_err_value(sqlite::failure(operation, Error::Engine(error), span))
                }
            }));
        }
        self.service_pending_signal(span)?;
        let (sql, params) = match op {
            RuntimeOp::SqliteExecute | RuntimeOp::SqliteQuery => (
                lowered_str_arg_owned(values.get(1).cloned(), "", operation, span)?, parameters(values.get(2), span)?,
            ),
            RuntimeOp::SqliteBegin => {
                let mode = mode(values.get(1), xsh_registry::enums::SQLITE_TRANSACTION_MODE, "Deferred", span)?;
                let sql = match mode.as_str() {
                    "Deferred" => "BEGIN DEFERRED",
                    "Immediate" => "BEGIN IMMEDIATE",
                    "Exclusive" => "BEGIN EXCLUSIVE",
                    _ => return Err(RuntimeError::new("type-error", "unknown sqlite transaction mode").with_span(span)),
                };
                (sql.to_string(), Vec::new())
            }
            RuntimeOp::SqliteCommit => ("COMMIT".to_string(), Vec::new()),
            RuntimeOp::SqliteRollback => ("ROLLBACK".to_string(), Vec::new()),
            _ => (String::new(), Vec::new()),
        };
        let deadline = self.within_deadlines.iter().filter(|deadline| !deadline.delivered).map(|deadline| deadline.at).min();
        let abandoned = if self.stage_abandon_delivered { None } else { self.stage_abandoned.clone() };
        let pending_primary = crate::runtime::process::signal_snapshot().primary.is_some();
        #[cfg(feature = "native-tests")]
        let test_cancel = if self.signal_state.test_cancel_delivered { None } else { self.test_cancellation.clone() };
        let Some(super::super::OwnedFsCapability { value: super::super::FsCapability::Sqlite(database), .. }) = self.fs_capabilities[index].as_mut() else {
            return Ok(ControlFlow::Continue(lowered_result_err_value(sqlite::closed(operation, span))));
        };
        let result = (|| {
            database.set_interrupt(move || {
                let snapshot = crate::runtime::process::signal_snapshot();
                let signal = snapshot.escalation.is_some() || (!pending_primary && snapshot.primary.is_some());
                let stage = abandoned.as_ref().is_some_and(|flag| flag.load(Ordering::Relaxed));
                #[cfg(feature = "native-tests")]
                let test = test_cancel.as_ref().is_some_and(|cancel| cancel.level() != 0);
                #[cfg(not(feature = "native-tests"))]
                let test = false;
                signal || stage || test || deadline.is_some_and(|at| std::time::Instant::now() >= at)
            })?;
            let result = match op {
                RuntimeOp::SqliteInTransaction => Ok(Value::Bool(database.in_transaction())),
                RuntimeOp::SqliteQuery => database.query(&sql, &params).map(|table| Value::Record(RecordMap::from([
                    (Arc::from("columns"), Value::List(table.columns.into_iter().map(|name| Value::Str(name.into())).collect())),
                    (Arc::from("rows"), Value::List(table.rows.into_iter().map(Value::List).collect())),
                ]))),
                RuntimeOp::SqliteExecute => database.execute(&sql, &params).and_then(|count| i64::try_from(count).map(Value::Int).map_err(|_| Error::InvalidData("change count exceeds Int"))),
                _ => database.execute(&sql, &params).map(|_| Value::Unit),
            };
            let cleared = database.clear_interrupt();
            match result { Err(error) => Err(error), Ok(value) => cleared.map(|()| value) }
        })();
        if result.is_err() { self.service_pending_signal(span)?; }
        let value = match result {
            Ok(value) => lowered_runtime_value(Value::ok(value), span)?,
            Err(error) => lowered_result_err_value(sqlite::failure(operation, error, span)),
        };
        Ok(ControlFlow::Continue(value))
    }
}
