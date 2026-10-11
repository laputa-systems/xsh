use std::ffi::{CStr, CString};

use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::source::Span;

// libc exposes this POSIX extension on Linux but omits its macOS declaration.
unsafe extern "C" {
    fn nl_langinfo_l(item: libc::nl_item, locale: libc::locale_t) -> *mut libc::c_char;
}

// The locale handle owns every pointer returned by nl_langinfo_l; copy metadata
// before dropping it and never change the process or thread locale.
struct Locale(libc::locale_t);

impl Drop for Locale {
    fn drop(&mut self) {
        unsafe { libc::freelocale(self.0) };
    }
}

impl Locale {
    fn open(name: &str, mask: libc::c_int, span: Span) -> Result<Self, RuntimeError> {
        if name.is_empty() {
            return Err(RuntimeError::new("locale-name", "locale name must be explicit").with_span(span));
        }
        let encoded = CString::new(name).map_err(|_| {
            RuntimeError::new("locale-name", "locale name contains NUL").with_span(span)
        })?;
        // musl accepts arbitrary names while returning C numeric and time data.
        // Reject those names so success means the requested metadata is supported.
        #[cfg(target_env = "musl")]
        if !matches!(name, "C" | "POSIX" | "C.UTF-8") {
            return Err(RuntimeError::new("locale-unavailable",
                format!("numeric and time metadata for locale {name:?} are unsupported by musl")).with_span(span));
        }
        let handle = unsafe { libc::newlocale(mask, encoded.as_ptr(), std::ptr::null_mut()) };
        if handle.is_null() {
            return Err(RuntimeError::new("locale-unavailable",
                format!("cannot load locale {name:?}: {}", std::io::Error::last_os_error())).with_span(span));
        }
        Ok(Self(handle))
    }

    fn bytes(&self, item: libc::nl_item, span: Span) -> Result<Value, RuntimeError> {
        let pointer = unsafe { nl_langinfo_l(item, self.0) };
        if pointer.is_null() {
            return Err(RuntimeError::new("locale-unavailable", "native locale metadata is unavailable").with_span(span));
        }
        Ok(Value::Bytes(unsafe { CStr::from_ptr(pointer) }.to_bytes().to_vec()))
    }
}

pub(crate) fn numeric_info(name: &str, span: Span) -> Result<Value, RuntimeError> {
    let locale = Locale::open(name, libc::LC_NUMERIC_MASK, span)?;
    Ok(Value::Record(RecordMap::from([
        ("decimal_point".into(), locale.bytes(libc::RADIXCHAR, span)?),
        ("thousands_separator".into(), locale.bytes(libc::THOUSEP, span)?),
    ])))
}

pub(crate) fn time_info(name: &str, span: Span) -> Result<Value, RuntimeError> {
    let locale = Locale::open(name, libc::LC_TIME_MASK, span)?;
    let months = [libc::ABMON_1, libc::ABMON_2, libc::ABMON_3, libc::ABMON_4,
        libc::ABMON_5, libc::ABMON_6, libc::ABMON_7, libc::ABMON_8,
        libc::ABMON_9, libc::ABMON_10, libc::ABMON_11, libc::ABMON_12];
    Ok(Value::Record(RecordMap::from([
        ("abbreviated_months".into(), Value::List(months.into_iter().map(|item| locale.bytes(item, span)).collect::<Result<_, _>>()?)),
    ])))
}
