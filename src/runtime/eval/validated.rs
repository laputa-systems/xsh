//! Schema conversions share validation checks with the borrowed value view
//! used by type tests, type patterns, and dynamic call boundaries.

use super::LoweredValue;
use crate::sema::validated::Validation;

/// Whether a value in its lowered representation passes a validation.
pub(super) fn lowered_value_passes(validation: Validation, value: &LoweredValue) -> bool {
    super::type_view::ValueView::Lowered(value).passes(validation)
}

/// What `value`, which fails `validation`, is called in the failure: the
/// integer itself for a range, whose bounds the expected type already names.
pub(super) fn lowered_failure(validation: Validation, value: &LoweredValue) -> String {
    match (validation, value) {
        (Validation::Range(_), LoweredValue::Int(value)) => value.to_string(),
        _ => validation.failure().to_string(),
    }
}
