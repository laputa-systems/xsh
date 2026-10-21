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

pub(crate) struct Collection {
    counters: Arc<ConstraintCounters>,
    restore: Option<RestoreCounters>,
}

impl Collection {
    pub(crate) fn finish(mut self) -> ConstraintMetrics {
        let metrics = self.counters.metrics();
        drop(self.restore.take());
        metrics
    }
}

pub(crate) fn current_counters() -> Option<Arc<ConstraintCounters>> {
    ACTIVE_COUNTERS.with(|active| active.borrow().clone())
}

pub(crate) fn record_diagnostic_work() {
    ACTIVE_COUNTERS.with(|active| {
        if let Some(counters) = active.borrow().as_ref() {
            counters.record_diagnostic_work();
        }
    });
}

pub(crate) fn begin() -> Collection {
    let counters = Arc::new(ConstraintCounters::for_measurement());
    let previous = ACTIVE_COUNTERS.with(|active| active.replace(Some(Arc::clone(&counters))));
    Collection {
        counters,
        restore: Some(RestoreCounters(previous)),
    }
}

#[cfg(test)]
pub(crate) fn collect<T>(operation: impl FnOnce() -> T) -> (ConstraintMetrics, T) {
    let collection = begin();
    let output = operation();
    let metrics = collection.finish();
    (metrics, output)
}
