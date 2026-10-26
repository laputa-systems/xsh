//! Schema conversions share validation checks with the borrowed value view
//! used by type tests, type patterns, and dynamic call boundaries.

use super::LoweredValue;
use crate::sema::validated::Validation;

#[cfg(test)]
thread_local! { static CHECK_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

#[cfg(test)]
pub(super) fn count_checks<T>(run: impl FnOnce() -> T) -> (T, usize) {
    CHECK_COUNT.set(0);
    let result = run();
    (result, CHECK_COUNT.get())
}

pub(super) fn lowered_value_passes(validation: Validation, value: &LoweredValue) -> bool {
    #[cfg(test)]
    CHECK_COUNT.set(CHECK_COUNT.get() + 1);
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
