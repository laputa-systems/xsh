use crate::runtime::process::{drain_signal_wake, signal_wake_reader};
use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::source::Span;
use rustix::event::{PollFd, PollFlags, poll};
use rustix::fd::BorrowedFd;
use rustix::io::Errno;
use rustix::time::Timespec;
use std::collections::BTreeSet;
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(crate) const POLL_FDS_LIMIT: usize = 64;
const KIND: &str = "unix-poll-fds";

pub(crate) struct PollInterest {
    fd: libc::c_int,
    requested: PollFlags,
}

pub(crate) fn writable(fd: libc::c_int) -> PollInterest {
    PollInterest { fd, requested: PollFlags::OUT }
}

pub(crate) enum PollStep {
    Ready(Vec<(usize, PollFlags)>),
    TimedOut,
    Interrupted,
}

fn invalid(message: impl Into<String>, span: Span) -> RuntimeError {
    RuntimeError::new(KIND, message.into()).with_span(span)
}

fn host(error: io::Error, span: Span) -> RuntimeError {
    RuntimeError::host(KIND, &error).with_span(span)
}

pub(crate) fn interests(entries: &Value, span: Span) -> Result<Vec<PollInterest>, RuntimeError> {
    let Value::List(entries) = entries else {
        return Err(invalid("entries must be a list of {fd, events} records", span));
    };
    if entries.len() > POLL_FDS_LIMIT {
        return Err(invalid(format!("at most {POLL_FDS_LIMIT} descriptors may be polled"), span));
    }
    let mut seen = BTreeSet::new();
    let mut interests = Vec::with_capacity(entries.len());
    for entry in entries {
        let Value::Record(record) = entry else {
            return Err(invalid("each entry must be an {fd, events} record", span));
        };
        let Some(Value::Int(fd)) = record.get("fd") else {
            return Err(invalid("entry fd must be an Int", span));
        };
        let Some(Value::List(events)) = record.get("events") else {
            return Err(invalid("entry events must be a list of strings", span));
        };
        let fd = libc::c_int::try_from(*fd).ok().filter(|fd| *fd >= 0)
            .ok_or_else(|| invalid("fd must be between 0 and 2147483647", span))?;
        crate::startup_stdio::check(fd).map_err(|error| host(error, span))?;
        if !seen.insert(fd) {
            return Err(invalid(format!("descriptor {fd} appears more than once"), span));
        }
        let mut requested = PollFlags::empty();
        for event in events {
            requested |= match event {
                Value::Str(name) if &**name == "readable" => PollFlags::IN,
                Value::Str(name) if &**name == "writable" => PollFlags::OUT,
                Value::Str(name) => return Err(invalid(format!(
                    "unknown requested event {name}; expected readable or writable"), span)),
                _ => return Err(invalid("entry events must be a list of strings", span)),
            };
        }
        interests.push(PollInterest { fd, requested });
    }
    Ok(interests)
}

pub(crate) fn step(
    interests: &[PollInterest],
    deadline: Option<Instant>,
    span: Span,
) -> Result<PollStep, RuntimeError> {
    let wake = signal_wake_reader().map_err(|error| host(error, span))?;
    // SAFETY: the numbers are only addressed by value; the kernel reports a
    // descriptor that is not open as an invalid-descriptor event.
    let borrowed: Vec<BorrowedFd<'_>> = interests.iter()
        .map(|interest| unsafe { BorrowedFd::borrow_raw(interest.fd) }).collect();
    let mut descriptors: Vec<PollFd<'_>> = borrowed.iter().zip(interests)
        .map(|(fd, interest)| PollFd::new(fd, interest.requested)).collect();
    descriptors.push(PollFd::new(&wake, PollFlags::IN));
    let timeout = deadline.map(|deadline| {
        let remaining = deadline.saturating_duration_since(Instant::now());
        Timespec { tv_sec: remaining.as_secs() as i64, tv_nsec: i64::from(remaining.subsec_nanos()) }
    });
    match poll(&mut descriptors, timeout.as_ref()) {
        Ok(_) => {}
        Err(Errno::INTR) => return Ok(PollStep::Interrupted),
        Err(error) => return Err(host(io::Error::from(error), span)),
    }
    let woken = descriptors.pop().is_some_and(|wake| !wake.revents().is_empty());
    if woken {
        drain_signal_wake();
        return Ok(PollStep::Interrupted);
    }
    let ready: Vec<(usize, PollFlags)> = descriptors.iter().enumerate()
        .filter_map(|(index, fd)| (!fd.revents().is_empty()).then_some((index, fd.revents()))).collect();
    if !ready.is_empty() {
        return Ok(PollStep::Ready(ready));
    }
    match deadline {
        Some(deadline) if Instant::now() >= deadline => Ok(PollStep::TimedOut),
        _ => Ok(PollStep::Interrupted),
    }
}

pub(crate) fn deadline(timeout: Option<Duration>, span: Span) -> Result<Option<Instant>, RuntimeError> {
    timeout.map(|timeout| Instant::now().checked_add(timeout)
        .ok_or_else(|| invalid("timeout is too large", span))).transpose()
}

pub(crate) fn report(interests: &[PollInterest], ready: &[(usize, PollFlags)]) -> Value {
    Value::List(ready.iter().map(|(index, revents)| {
        let events = [
            (PollFlags::IN, "readable"),
            (PollFlags::OUT, "writable"),
            (PollFlags::ERR, "error"),
            (PollFlags::HUP, "hangup"),
            (PollFlags::NVAL, "invalid"),
        ].into_iter().filter(|(flag, _)| revents.contains(*flag))
            .map(|(_, name)| Value::Str(name.into())).collect();
        Value::Record(RecordMap::from([
            (Arc::from("fd"), Value::Int(i64::from(interests[*index].fd))),
            (Arc::from("events"), Value::List(events)),
        ]))
    }).collect())
}
