use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
#[cfg(target_os = "macos")]
use std::ffi::CString;
use std::sync::Arc;

pub(crate) fn hostname(_span: Span) -> Result<String, RuntimeError> {
    Ok(rustix::system::uname()
        .nodename()
        .to_string_lossy()
        .into_owned())
}

pub(crate) fn uname(_span: Span) -> Result<Value, RuntimeError> {
    let uts = rustix::system::uname();
    Ok(Value::Record(crate::runtime::value::RecordMap::from([
        (
            Arc::from("sysname"),
            Value::Str(uts.sysname().to_string_lossy().into_owned().into()),
        ),
        (
            Arc::from("nodename"),
            Value::Str(uts.nodename().to_string_lossy().into_owned().into()),
        ),
        (
            Arc::from("release"),
            Value::Str(uts.release().to_string_lossy().into_owned().into()),
        ),
        (
            Arc::from("version"),
            Value::Str(uts.version().to_string_lossy().into_owned().into()),
        ),
        (
            Arc::from("machine"),
            Value::Str(uts.machine().to_string_lossy().into_owned().into()),
        ),
    ])))
}

pub(crate) fn memory(span: Span) -> Result<Value, RuntimeError> {
    memory_impl(span).map(|memory| {
        Value::Record(crate::runtime::value::RecordMap::from([
            (Arc::from("total"), Value::Int(memory.total)),
            (Arc::from("available"), Value::Int(memory.available)),
            (Arc::from("free"), Value::Int(memory.free)),
            (Arc::from("swap_total"), Value::Int(memory.swap_total)),
            (Arc::from("swap_free"), Value::Int(memory.swap_free)),
        ]))
    })
}

pub(crate) fn os_release(span: Span) -> Result<Value, RuntimeError> {
    os_release_impl(span).map(|release| {
        Value::Record(crate::runtime::value::RecordMap::from([
            (Arc::from("name"), Value::Str(release.name.into())),
            (
                Arc::from("pretty_name"),
                Value::Str(release.pretty_name.into()),
            ),
            (Arc::from("version"), Value::Str(release.version.into())),
            (
                Arc::from("version_id"),
                Value::Str(release.version_id.into()),
            ),
            (Arc::from("id"), Value::Str(release.id.into())),
        ]))
    })
}

struct SystemMemory {
    total: i64,
    available: i64,
    free: i64,
    swap_total: i64,
    swap_free: i64,
}

struct SystemOsRelease {
    name: String,
    pretty_name: String,
    version: String,
    version_id: String,
    id: String,
}

// Superseded Linux bodies, deliberately not kept.
//
// On Linux the registry binds `system.memory` and `system.os_release` to the
// embedded `system` module, which owns the whole `/proc/meminfo` and
// `os-release` policy: the file paths, the parsing, the defaults, and the
// `system-memory` / `system-os-release` error kinds. The native bodies that
// used to do that work were a second implementation of the same policy, so
// they were removed once the embedded entries were verified on Linux.
//
// The entries below are what remains of the native arm. Nothing on Linux
// reaches them — the embedded implementation is selected at lowering time, and
// the calls in `lowered_run` exist only for the platforms that keep a native
// body — so they report the defect of being reached at all rather than
// answering from a retired implementation.
#[cfg(target_os = "linux")]
fn memory_impl(span: Span) -> Result<SystemMemory, RuntimeError> {
    Err(RuntimeError::new(
        "system-memory",
        "the embedded standard-library implementation owns system.memory on Linux",
    )
    .with_span(span))
}

#[cfg(target_os = "linux")]
fn os_release_impl(span: Span) -> Result<SystemOsRelease, RuntimeError> {
    Err(RuntimeError::new(
        "system-os-release",
        "the embedded standard-library implementation owns system.os_release on Linux",
    )
    .with_span(span))
}

#[cfg(target_os = "macos")]
fn memory_impl(span: Span) -> Result<SystemMemory, RuntimeError> {
    let total = sysctl_u64("hw.memsize", span)? as i64;
    Ok(SystemMemory {
        total,
        available: 0,
        free: 0,
        swap_total: 0,
        swap_free: 0,
    })
}

#[cfg(target_os = "macos")]
fn os_release_impl(span: Span) -> Result<SystemOsRelease, RuntimeError> {
    let version = sysctl_string("kern.osproductversion", span).unwrap_or_default();
    let pretty_name = if version.is_empty() {
        "macOS".to_string()
    } else {
        format!("macOS {version}")
    };
    Ok(SystemOsRelease {
        name: "macOS".to_string(),
        pretty_name,
        version: version.clone(),
        version_id: version,
        id: "macos".to_string(),
    })
}

#[cfg(target_os = "macos")]
fn sysctl_u64(name: &str, span: Span) -> Result<u64, RuntimeError> {
    let c_name = CString::new(name).map_err(|_| {
        RuntimeError::new("system-sysctl", "sysctl name contains NUL").with_span(span)
    })?;
    let mut value: u64 = 0;
    let mut size = std::mem::size_of::<u64>();
    let rc = unsafe {
        libc::sysctlbyname(
            c_name.as_ptr(),
            (&mut value as *mut u64).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return Err(RuntimeError::new(
            "system-sysctl",
            std::io::Error::last_os_error().to_string(),
        )
        .with_span(span));
    }
    Ok(value)
}

#[cfg(target_os = "macos")]
fn sysctl_string(name: &str, span: Span) -> Result<String, RuntimeError> {
    let c_name = CString::new(name).map_err(|_| {
        RuntimeError::new("system-sysctl", "sysctl name contains NUL").with_span(span)
    })?;
    let mut size = 0usize;
    let rc = unsafe {
        libc::sysctlbyname(
            c_name.as_ptr(),
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return Err(RuntimeError::new(
            "system-sysctl",
            std::io::Error::last_os_error().to_string(),
        )
        .with_span(span));
    }
    let mut buffer = vec![0u8; size];
    let rc = unsafe {
        libc::sysctlbyname(
            c_name.as_ptr(),
            buffer.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return Err(RuntimeError::new(
            "system-sysctl",
            std::io::Error::last_os_error().to_string(),
        )
        .with_span(span));
    }
    if let Some(last) = buffer.last()
        && *last == 0
    {
        buffer.pop();
    }
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn memory_impl(span: Span) -> Result<SystemMemory, RuntimeError> {
    Err(RuntimeError::new(
        "system-memory",
        "memory discovery is unsupported on this platform",
    )
    .with_span(span))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn os_release_impl(span: Span) -> Result<SystemOsRelease, RuntimeError> {
    Err(RuntimeError::new(
        "system-os-release",
        "OS release discovery is unsupported on this platform",
    )
    .with_span(span))
}

#[cfg(test)]
mod tests {
    use super::{memory_impl, os_release_impl};
    use crate::source::{SourceId, Span};

    fn test_span() -> Span {
        Span::new(SourceId::new(0), 0, 0)
    }

    // The Linux entries have no native body to exercise; the embedded module
    // owns them, and the corpus covers their behavior on Linux. These tests
    // cover the platforms that still parse the host text natively.
    #[cfg(not(target_os = "linux"))]
    #[test]
    fn memory_reports_total() {
        let memory = memory_impl(test_span()).expect("read memory");
        assert!(memory.total > 0);
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn os_release_reports_name() {
        let release = os_release_impl(test_span()).expect("read os release");
        assert!(!release.name.is_empty());
        assert!(!release.pretty_name.is_empty());
        assert!(!release.id.is_empty());
    }

    // Being reached on Linux is the defect: the binding must select the
    // embedded implementation, so this arm is only ever a stale route. The
    // retired bodies are matched rather than unwrapped because the record types
    // they would have returned are not `Debug`.
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_native_system_bodies_report_the_retired_route() {
        let Err(memory) = memory_impl(test_span()) else {
            panic!("the native Linux memory body is retired");
        };
        assert!(memory.message.contains("embedded"), "{memory:?}");
        let Err(release) = os_release_impl(test_span()) else {
            panic!("the native Linux release body is retired");
        };
        assert!(release.message.contains("embedded"), "{release:?}");
    }
}
