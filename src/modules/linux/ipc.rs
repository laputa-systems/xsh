use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;

#[cfg(target_os = "linux")]
mod real {
    use super::*;
    use crate::runtime::value::RecordMap;
    use std::ffi::CString;
    use std::io;
    use std::sync::Arc;

    fn host_error(kind: &str, error: io::Error, span: Span) -> Value {
        Value::err(Value::Error(Box::new(RuntimeError::host(kind, &error).with_span(span))))
    }

    fn finish_read(kind: &str, read: io::Result<i64>, close: io::Result<()>, span: Span) -> Value {
        match (read, close) {
            (Ok(value), Ok(())) => Value::ok(Value::Int(value)),
            (Ok(_), Err(error)) | (Err(error), Ok(())) => host_error(kind, error, span),
            (Err(primary), Err(cleanup)) => {
                let mut error = RuntimeError::host(kind, &primary).with_span(span);
                error.message.push_str(&format!("; closing the IPC handle also failed: {cleanup}"));
                Value::err(Value::Error(Box::new(error)))
            }
        }
    }

    fn returned(status: libc::c_int) -> io::Result<()> {
        if status == -1 { Err(io::Error::last_os_error()) } else { Ok(()) }
    }

    pub(super) fn posix_semaphore_value(name: &[u8], span: Span) -> Value {
        let kind = "linux-posix-semaphore-value";
        let name = match CString::new(name) {
            Ok(name) => name,
            Err(_) => return host_error(kind, io::Error::from_raw_os_error(libc::EINVAL), span),
        };
        // SAFETY: name is NUL terminated and flags zero never consume variadic creation arguments.
        let semaphore = unsafe { libc::sem_open(name.as_ptr(), 0) };
        if semaphore == libc::SEM_FAILED {
            return host_error(kind, io::Error::last_os_error(), span);
        }
        let mut value = 0;
        // SAFETY: semaphore is a live sem_open result and value is writable for one c_int.
        let read = returned(unsafe { libc::sem_getvalue(semaphore, &mut value) }).map(|()| i64::from(value));
        // SAFETY: this function owns the successful sem_open reference and closes it exactly once.
        let close = returned(unsafe { libc::sem_close(semaphore) });
        finish_read(kind, read, close, span)
    }

    pub(super) fn posix_message_queue_messages(name: &[u8], span: Span) -> Value {
        let kind = "linux-posix-message-queue-messages";
        let name = match CString::new(name) {
            Ok(name) => name,
            Err(_) => return host_error(kind, io::Error::from_raw_os_error(libc::EINVAL), span),
        };
        // SAFETY: name is NUL terminated and O_CREAT is absent, so no variadic arguments are read.
        let queue = unsafe { libc::mq_open(name.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NONBLOCK) };
        if queue == -1 {
            return host_error(kind, io::Error::last_os_error(), span);
        }
        // SAFETY: mq_attr contains only integer fields and accepts the all-zero representation.
        let mut attributes: libc::mq_attr = unsafe { std::mem::zeroed() };
        // SAFETY: queue is live and attributes points to writable storage for one mq_attr.
        let read = returned(unsafe { libc::mq_getattr(queue, &mut attributes) }).map(|()| i64::from(attributes.mq_curmsgs));
        // SAFETY: this function owns the successful mq_open descriptor and closes it exactly once.
        let close = returned(unsafe { libc::mq_close(queue) });
        finish_read(kind, read, close, span)
    }

    pub(super) fn system_v_semaphore_element(id: i64, index: i64, span: Span) -> Value {
        let kind = "linux-system-v-semaphore-element";
        let (Ok(id), Ok(index)) = (libc::c_int::try_from(id), libc::c_int::try_from(index)) else {
            return host_error(kind, io::Error::from_raw_os_error(libc::EINVAL), span);
        };
        if id < 0 || index < 0 {
            return host_error(kind, io::Error::from_raw_os_error(libc::EINVAL), span);
        }
        let mut fields = RecordMap::new();
        for (name, operation) in [
            ("value", libc::GETVAL),
            ("waiting_for_increase", libc::GETNCNT),
            ("waiting_for_zero", libc::GETZCNT),
            ("pid", libc::GETPID),
        ] {
            // SAFETY: these four semctl commands take no variadic argument and validate id/index in the kernel.
            let value = unsafe { libc::semctl(id, index, operation) };
            if value == -1 {
                return host_error(kind, io::Error::last_os_error(), span);
            }
            fields.insert(Arc::from(name), Value::Int(i64::from(value)));
        }
        Value::ok(Value::Record(fields))
    }
}

#[cfg(not(target_os = "linux"))]
fn unavailable(span: Span) -> Value {
    Value::err(Value::Error(Box::new(RuntimeError::new("unsupported-platform", "Linux IPC observation requires Linux").with_span(span))))
}

pub(crate) fn posix_semaphore_value(name: &[u8], span: Span) -> Result<Value, RuntimeError> {
    #[cfg(target_os = "linux")]
    { Ok(real::posix_semaphore_value(name, span)) }
    #[cfg(not(target_os = "linux"))]
    { let _ = name; Ok(unavailable(span)) }
}

pub(crate) fn posix_message_queue_messages(name: &[u8], span: Span) -> Result<Value, RuntimeError> {
    #[cfg(target_os = "linux")]
    { Ok(real::posix_message_queue_messages(name, span)) }
    #[cfg(not(target_os = "linux"))]
    { let _ = name; Ok(unavailable(span)) }
}

pub(crate) fn system_v_semaphore_element(id: i64, index: i64, span: Span) -> Result<Value, RuntimeError> {
    #[cfg(target_os = "linux")]
    { Ok(real::system_v_semaphore_element(id, index, span)) }
    #[cfg(not(target_os = "linux"))]
    { let _ = (id, index); Ok(unavailable(span)) }
}
