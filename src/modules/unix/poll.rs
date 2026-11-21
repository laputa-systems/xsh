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

#[derive(Clone, Copy)]
pub(crate) enum Request {
    Readable,
    Writable,
}

impl PollInterest {
    pub(crate) fn new(fd: libc::c_int, requests: &[Request]) -> Self {
        let requested = requests.iter().fold(PollFlags::empty(), |flags, request| flags | match request {
            Request::Readable => PollFlags::IN,
            Request::Writable => PollFlags::OUT,
        });
        PollInterest { fd, requested }
    }
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
    // Darwin's poll reports nothing for a zero-interest entry, including a
    // closed descriptor, so ask the descriptor itself. An invalid entry is
    // already ready, so the wait does not block past this call.
    let invalid: Vec<usize> = interests.iter().enumerate()
        .filter(|(_, interest)| interest.requested.is_empty())
        .filter_map(|(index, _)| rustix::io::fcntl_getfd(borrowed[index]).is_err().then_some(index))
        .collect();
    let orphaned: Vec<usize> = interests.iter().enumerate()
        .filter(|(index, interest)| !interest.requested.contains(PollFlags::IN) && !invalid.contains(index))
        .filter_map(|(index, _)| fifo_writer_without_reader(borrowed[index]).then_some(index))
        .collect();
    let deadline = if invalid.is_empty() && orphaned.is_empty() { deadline } else { Some(Instant::now()) };
    let mut descriptors: Vec<PollFd<'_>> = borrowed.iter().zip(interests)
        .map(|(fd, interest)| PollFd::new(fd, interest.requested)).collect();
    descriptors.push(PollFd::new(&wake, PollFlags::IN));
    let slice = fifo_slice(&borrowed, interests);
    let timeout = match (deadline, slice) {
        (Some(deadline), slice) => {
            let remaining = deadline.saturating_duration_since(Instant::now());
            Some(slice.map_or(remaining, |slice| remaining.min(slice)))
        }
        (None, slice) => slice,
    }
    .map(|wait| Timespec { tv_sec: wait.as_secs() as i64, tv_nsec: i64::from(wait.subsec_nanos()) });
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
    let mut ready = ready;
    for index in invalid {
        if !ready.iter().any(|(seen, _)| *seen == index) {
            ready.push((index, PollFlags::NVAL));
        }
    }
    for index in orphaned {
        match ready.iter_mut().find(|(seen, _)| *seen == index) {
            Some((_, flags)) => *flags |= PollFlags::ERR,
            None => ready.push((index, PollFlags::ERR)),
        }
    }
    for (index, interest) in interests.iter().enumerate() {
        if interest.requested.contains(PollFlags::IN)
            && !ready.iter().any(|(seen, _)| *seen == index)
            && let Some(flags) = fifo_readiness_missed_by_poll(borrowed[index])
        {
            ready.push((index, flags));
        }
    }
    ready.sort_by_key(|(index, _)| *index);
    if !ready.is_empty() {
        return Ok(PollStep::Ready(ready));
    }
    match deadline {
        Some(deadline) if Instant::now() >= deadline => Ok(PollStep::TimedOut),
        _ => Ok(PollStep::Interrupted),
    }
}

// Darwin implements FIFOs on socket buffers and does not wake `poll` or
// `kqueue` readers when a writer blocks in one write larger than the buffer
// (seen on macOS 27.2); a new poll still reports the data. Waits that include
// a FIFO therefore poll in short slices there.
#[cfg(target_os = "macos")]
fn fifo_slice(fds: &[BorrowedFd<'_>], interests: &[PollInterest]) -> Option<Duration> {
    const SLICE: Duration = Duration::from_millis(20);
    fds.iter().zip(interests)
        .filter(|(_, interest)| !interest.requested.is_empty())
        .any(|(fd, _)| rustix::fs::fstat(fd).is_ok_and(|stat| {
            rustix::fs::FileType::from_raw_mode(stat.st_mode) == rustix::fs::FileType::Fifo
        }))
        .then_some(SLICE)
}

// Darwin's poll and kqueue report neither readable nor hangup on a FIFO whose
// last writer has closed, while select reports it readable (seen on macOS
// 27.2). A FIFO select sees as readable with nothing buffered is therefore at
// end of file, which Linux reports as a hangup.
#[cfg(target_os = "macos")]
pub(crate) fn fifo_readiness_missed_by_poll(fd: BorrowedFd<'_>) -> Option<PollFlags> {
    use std::os::fd::AsRawFd;
    let raw = fd.as_raw_fd();
    let fifo = rustix::fs::fstat(fd).is_ok_and(|stat| {
        rustix::fs::FileType::from_raw_mode(stat.st_mode) == rustix::fs::FileType::Fifo
    });
    if !fifo || raw >= libc::FD_SETSIZE as libc::c_int {
        return None;
    }
    let mut readable = vec![rustix::event::FdSetElement::default(); rustix::event::fd_set_num_elements(1, raw + 1)];
    rustix::event::fd_set_insert(&mut readable, raw);
    let zero = Timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `raw` is an open descriptor borrowed for the call, below
    // FD_SETSIZE, and the set holds enough elements for `raw + 1` descriptors.
    let selected = unsafe { rustix::event::select(raw + 1, Some(&mut readable), None, None, Some(&zero)) }
        .is_ok_and(|ready| ready == 1);
    if !selected {
        return None;
    }
    Some(if rustix::io::ioctl_fionread(fd).is_ok_and(|buffered| buffered > 0) { PollFlags::IN } else { PollFlags::HUP })
}

// Darwin's poll reports a pipe or FIFO whose last reader has closed as
// writable, or as nothing without interest, where Linux reports an error; a
// zero-length write still fails with EPIPE there. The runtime ignores SIGPIPE, so the probe cannot
// terminate the process.
#[cfg(target_os = "macos")]
fn fifo_writer_without_reader(fd: BorrowedFd<'_>) -> bool {
    let fifo = rustix::fs::fstat(fd).is_ok_and(|stat| {
        rustix::fs::FileType::from_raw_mode(stat.st_mode) == rustix::fs::FileType::Fifo
    });
    fifo && matches!(rustix::io::write(fd, &[]), Err(Errno::PIPE))
}

#[cfg(not(target_os = "macos"))]
fn fifo_writer_without_reader(_: BorrowedFd<'_>) -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn fifo_readiness_missed_by_poll(_: BorrowedFd<'_>) -> Option<PollFlags> {
    None
}

#[cfg(not(target_os = "macos"))]
fn fifo_slice(_: &[BorrowedFd<'_>], _: &[PollInterest]) -> Option<Duration> {
    None
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
