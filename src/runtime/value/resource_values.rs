use super::{ResultValue, RuntimeError, Value};
use rustc_hash::FxHashSet;

/// Borrowed containment includes error payloads and immutable causes so owned
/// resources retain the same lifetime through ordinary data and error metadata.
/// Shared descendants are visited once, without cloning or limiting chain depth.
impl Value {
    pub(crate) fn resource_reachable_values(&self) -> impl Iterator<Item = &Value> {
        ResourceReachableValues {
            pending: vec![self],
            visited: FxHashSet::default(),
        }
    }
}

impl RuntimeError {
    pub(crate) fn resource_reachable_values(&self) -> impl Iterator<Item = &Value> {
        let pending = self
            .payload
            .iter()
            .map(|(_, value)| value)
            .chain(self.cause.iter().map(|cause| cause.as_value()))
            .collect();
        ResourceReachableValues {
            pending,
            visited: FxHashSet::default(),
        }
    }
}

struct ResourceReachableValues<'a> {
    pending: Vec<&'a Value>,
    visited: FxHashSet<*const Value>,
}

impl<'a> Iterator for ResourceReachableValues<'a> {
    type Item = &'a Value;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let value = self.pending.pop()?;
            if !self.visited.insert(value as *const Value) {
                continue;
            }
            match value {
                Value::List(items) | Value::Tag { fields: items, .. } => self.pending.extend(items),
                Value::Map(fields) => self.pending.extend(fields.values()),
                Value::Record(fields) | Value::Module(fields) => {
                    self.pending.extend(fields.iter().map(|(_, value)| value))
                }
                Value::Result(ResultValue::Ok(value) | ResultValue::Err(value)) => {
                    self.pending.push(value)
                }
                Value::Error(error) => self
                    .pending
                    .extend(error.payload.iter().map(|(_, value)| value)),
                _ => {}
            }
            if let Some(cause) = value.error_cause() {
                self.pending.push(cause.as_value());
            }
            return Some(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::process::ProcessStatus;
    use crate::runtime::value::{NetJobValue, RecordMap, RunError};

    #[test]
    fn resource_reachable_values_follow_error_payloads_and_process_causes() {
        let symbols = crate::symbol::SymbolOwner::default();
        let _symbols = symbols.enter();
        let mut payload = RuntimeError::new("inner", "payload");
        payload.payload =
            RecordMap::from([("job".into(), Value::NetJob(Box::new(NetJobValue { id: 7 })))]);
        let process = Value::RunError(Box::new(RunError::from_status(ProcessStatus::exited(9))))
            .with_error_cause(Value::Error(Box::new(payload)))
            .unwrap();
        let value = Value::ok(Value::List(vec![Value::err(process)]));
        let ids = value
            .resource_reachable_values()
            .filter_map(|value| match value {
                Value::NetJob(job) => Some(job.id),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(ids, vec![7]);
    }

    #[test]
    fn resource_reachable_values_walk_long_shared_causes_once_without_recursion() {
        let mut chain = Value::Error(Box::new(RuntimeError::new("leaf", "original")));
        for _ in 0..100_000 {
            chain = Value::Error(Box::new(RuntimeError::new("outer", "translation")))
                .with_error_cause(chain)
                .unwrap();
        }
        let shared = Value::List(vec![chain.clone(), chain]);
        assert_eq!(shared.resource_reachable_values().count(), 100_003);
    }
}
