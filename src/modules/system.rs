use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
#[cfg(target_os = "linux")]
use rustc_hash::FxHashMap;
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

#[cfg(target_os = "linux")]
fn memory_impl(span: Span) -> Result<SystemMemory, RuntimeError> {
    let text = std::fs::read_to_string("/proc/meminfo")
        .map_err(|error| RuntimeError::new("system-memory", error.to_string()).with_span(span))?;
    let mut values = FxHashMap::default();
    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let fields = rest.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 2 || fields[1] != "kB" {
            continue;
        }
        let value = fields[0].parse::<i64>().map_err(|_| {
            RuntimeError::new(
                "system-memory",
                format!("invalid numeric value for `{key}` in /proc/meminfo"),
            )
            .with_span(span)
        })?;
        values.insert(key.to_string(), value.saturating_mul(1024));
    }
    let get = |name: &str| {
        values.get(name).copied().ok_or_else(|| {
            RuntimeError::new(
                "system-memory",
                format!("missing `{name}` in /proc/meminfo"),
            )
            .with_span(span)
        })
    };
    Ok(SystemMemory {
        total: get("MemTotal")?,
        available: get("MemAvailable")?,
        free: get("MemFree")?,
        swap_total: get("SwapTotal")?,
        swap_free: get("SwapFree")?,
    })
}

#[cfg(target_os = "linux")]
fn os_release_impl(span: Span) -> Result<SystemOsRelease, RuntimeError> {
    let text = std::fs::read_to_string("/etc/os-release")
        .or_else(|_| std::fs::read_to_string("/usr/lib/os-release"))
        .map_err(|error| {
            RuntimeError::new("system-os-release", error.to_string()).with_span(span)
        })?;
    let values = parse_os_release(&text);
    let name = values
        .get("NAME")
        .cloned()
        .unwrap_or_else(|| "Linux".to_string());
    let pretty_name = values
        .get("PRETTY_NAME")
        .cloned()
        .unwrap_or_else(|| name.clone());
    Ok(SystemOsRelease {
        name,
        pretty_name,
        version: values.get("VERSION").cloned().unwrap_or_default(),
        version_id: values.get("VERSION_ID").cloned().unwrap_or_default(),
        id: values
            .get("ID")
            .cloned()
            .unwrap_or_else(|| "linux".to_string()),
    })
}

#[cfg(target_os = "linux")]
fn parse_os_release(text: &str) -> FxHashMap<String, String> {
    let mut values = FxHashMap::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, raw_value)) = line.split_once('=') else {
            continue;
        };
        values.insert(key.to_string(), unquote_os_release_value(raw_value));
    }
    values
}

#[cfg(target_os = "linux")]
fn unquote_os_release_value(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        let mut result = String::new();
        let mut escaped = false;
        for ch in value[1..value.len() - 1].chars() {
            if escaped {
                result.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else {
                result.push(ch);
            }
        }
        result
    } else {
        value.to_string()
    }
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

    #[test]
    fn memory_reports_total() {
        let memory = memory_impl(test_span()).expect("read memory");
        assert!(memory.total > 0);
    }

    #[test]
    fn os_release_reports_name() {
        let release = os_release_impl(test_span()).expect("read os release");
        assert!(!release.name.is_empty());
        assert!(!release.pretty_name.is_empty());
        assert!(!release.id.is_empty());
    }
}
