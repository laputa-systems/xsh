use super::constraints::{ConstraintCounters, ConstraintMetrics};
use std::cell::RefCell;
use std::sync::Arc;

thread_local! {
    static ACTIVE_COUNTERS: RefCell<Option<Arc<ConstraintCounters>>> = const { RefCell::new(None) };
}

struct RestoreCounters(Option<Arc<ConstraintCounters>>);

impl Drop for RestoreCounters {
    fn drop(&mut self) {
        ACTIVE_COUNTERS.with(|active| {
            *active.borrow_mut() = self.0.take();
        });
    }
}

pub(crate) fn current_counters() -> Option<Arc<ConstraintCounters>> {
    ACTIVE_COUNTERS.with(|active| active.borrow().clone())
}

pub(crate) fn collect<T>(operation: impl FnOnce() -> T) -> (ConstraintMetrics, T) {
    let counters = Arc::new(ConstraintCounters::for_measurement());
    let previous = ACTIVE_COUNTERS.with(|active| active.replace(Some(Arc::clone(&counters))));
    let restore = RestoreCounters(previous);
    let output = operation();
    let metrics = counters.metrics();
    drop(restore);
    (metrics, output)
}
