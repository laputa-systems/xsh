use super::{DeferKind, RuntimeError};
use crate::trace::Traceback;

#[derive(Default)]
pub(in crate::runtime::eval) struct CleanupFailures(Option<(RuntimeError, Option<Traceback>)>);

pub(in crate::runtime::eval) fn forced(error: &RuntimeError) -> bool {
    error.abort.as_ref().is_some_and(|signal| signal.force)
}

impl CleanupFailures {
    pub(in crate::runtime::eval) fn record(&mut self, error: RuntimeError, traceback: Option<Traceback>) -> Option<RuntimeError> {
        if self.0.as_ref().is_none_or(|(previous, _)| forced(&error) && !forced(previous)) {
            self.0.replace((error, traceback)).map(|(error, _)| error)
        } else {
            Some(error)
        }
    }

    pub(in crate::runtime::eval) fn take(&mut self) -> Option<(RuntimeError, Option<Traceback>)> { self.0.take() }
    pub(in crate::runtime::eval) fn peek(&self) -> Option<&(RuntimeError, Option<Traceback>)> { self.0.as_ref() }
    pub(in crate::runtime::eval) fn peek_mut(&mut self) -> Option<&mut (RuntimeError, Option<Traceback>)> { self.0.as_mut() }
}

pub(in crate::runtime::eval) fn failure_is_recorded(kind: DeferKind, forced: bool) -> bool {
    kind != DeferKind::ManagedRelease || forced
}

pub(in crate::runtime::eval) fn next_due_defer(kind_at: impl Fn(usize) -> DeferKind, next: usize, leaves_with_error: bool) -> Option<usize> {
    (0..next).rev().find(|&index| kind_at(index) != DeferKind::OnError || leaves_with_error)
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::runtime::eval) enum Disposition { Raise, ReportSecondary, Surface }

pub(in crate::runtime::eval) fn cleanup_disposition(forced: bool, primary_fails: bool) -> Disposition {
    if forced { Disposition::Raise } else if primary_fails { Disposition::ReportSecondary } else { Disposition::Surface }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleanup_policy_decisions() {
        for kind in [DeferKind::Always, DeferKind::OnError, DeferKind::ManagedRelease] {
            for force in [false, true] {
                assert_eq!(failure_is_recorded(kind, force), kind != DeferKind::ManagedRelease || force);
            }
        }
        assert_eq!(cleanup_disposition(true, false), Disposition::Raise);
        assert_eq!(cleanup_disposition(true, true), Disposition::Raise);
        assert_eq!(cleanup_disposition(false, true), Disposition::ReportSecondary);
        assert_eq!(cleanup_disposition(false, false), Disposition::Surface);
    }
    #[test]
    fn cleanup_policy_accumulation() {
        crate::runtime::eval::run_eval(|| {
        let mut failures = CleanupFailures::default();
        assert!(failures.record(RuntimeError::new("a", "a"), None).is_none());
        assert_eq!(failures.record(RuntimeError::new("b", "b"), None).unwrap().kind, "b");
        assert_eq!(failures.record(RuntimeError::abort(143, true), None).unwrap().kind, "a");
        assert!(forced(&failures.take().unwrap().0));
        });
    }
}
