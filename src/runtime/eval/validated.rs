//! The runtime side of validated types: whether a value of the base type
//! passes a validation. The explicit conversion (`.require(T)`, a type test,
//! a type pattern) and every dynamic boundary that tests a value against a
//! static type ask here, so they agree with each other.

use super::{LoweredValue, Value};
use crate::sema::validated::{Validation, is_rel_path};

/// Whether `value`, already known to be a value of the base type, passes.
pub(super) fn value_passes(validation: Validation, value: &Value) -> bool {
    match validation {
        Validation::NonEmpty => matches!(value, Value::List(items) if !items.is_empty()),
        Validation::RelPath => matches!(value, Value::Path(path) if is_rel_path(&path.bytes)),
        // A nominal identity has no runtime representation: a value of the
        // base record is all `.require(Name)` can ask for, and the checker
        // allows no other test of the type on a value it cannot vouch for.
        Validation::Nominal(_) => true,
        Validation::Range(range) => matches!(value, Value::Int(value) if range.contains(*value)),
    }
}

/// `value_passes` for a value in its lowered representation.
pub(super) fn lowered_value_passes(validation: Validation, value: &LoweredValue) -> bool {
    match validation {
        Validation::NonEmpty => match value {
            LoweredValue::List(items) => !items.is_empty(),
            LoweredValue::SharedList(items) => !items.is_empty(),
            _ => false,
        },
        Validation::RelPath => {
            matches!(value, LoweredValue::Path(path) if is_rel_path(&path.bytes))
        }
        Validation::Nominal(_) => true,
        Validation::Range(range) => {
            matches!(value, LoweredValue::Int(value) if range.contains(*value))
        }
    }
}

/// What `value`, which fails `validation`, is called in the failure: the
/// integer itself for a range, whose bounds the expected type already names.
pub(super) fn lowered_failure(validation: Validation, value: &LoweredValue) -> String {
    match (validation, value) {
        (Validation::Range(_), LoweredValue::Int(value)) => value.to_string(),
        _ => validation.failure().to_string(),
    }
}
