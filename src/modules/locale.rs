#[cfg(not(target_env = "musl"))]
use std::ffi::{CStr, CString};
#[cfg(any(target_env = "musl", test))]
use std::{collections::BTreeMap, sync::OnceLock};

use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::source::Span;

fn validate_name(name: &str, span: Span) -> Result<(), RuntimeError> {
    if name.is_empty() || name.as_bytes().contains(&0) {
        return Err(RuntimeError::new("locale-name", "locale name must be explicit and contain no NUL").with_span(span));
    }
    Ok(())
}

#[cfg(any(target_env = "musl", test))]
#[derive(miniserde::Deserialize)]
struct Metadata {
    decimal_point: Vec<u8>,
    thousands_separator: Vec<u8>,
    abbreviated_months: Vec<Vec<u8>>,
}

#[cfg(any(target_env = "musl", test))]
#[derive(miniserde::Deserialize)]
struct Projection {
    locales: BTreeMap<String, Metadata>,
    aliases: BTreeMap<String, String>,
}

// musl does not implement localized numeric or time metadata. These immutable
// byte facts were extracted from GNU libc; unknown names cannot fall back to C.
#[cfg(any(target_env = "musl", test))]
fn projection() -> &'static Projection {
    static DATA: OnceLock<Projection> = OnceLock::new();
    DATA.get_or_init(|| miniserde::json::from_str(include_str!("locale/data.json"))
        .expect("embedded locale metadata must be valid"))
}

#[cfg(target_env = "musl")]
fn projected(name: &str, span: Span) -> Result<&'static Metadata, RuntimeError> {
    validate_name(name, span)?;
    let data = projection();
    let canonical = data.aliases.get(name).map(String::as_str).unwrap_or(name);
    data.locales.get(canonical).ok_or_else(|| RuntimeError::new("locale-unavailable",
        format!("numeric and time metadata for locale {name:?} are unavailable")).with_span(span))
}

// libc exposes this POSIX extension on Linux but omits its macOS declaration.
#[cfg(not(target_env = "musl"))]
unsafe extern "C" {
    fn nl_langinfo_l(item: libc::nl_item, locale: libc::locale_t) -> *mut libc::c_char;
}

// The locale handle owns every pointer returned by nl_langinfo_l; copy metadata
// before dropping it and never change the process or thread locale.
#[cfg(not(target_env = "musl"))]
struct Locale(libc::locale_t);

#[cfg(not(target_env = "musl"))]
impl Drop for Locale {
    fn drop(&mut self) {
        unsafe { libc::freelocale(self.0) };
    }
}

#[cfg(not(target_env = "musl"))]
impl Locale {
    fn open(name: &str, mask: libc::c_int, span: Span) -> Result<Self, RuntimeError> {
        validate_name(name, span)?;
        let encoded = CString::new(name).expect("validated locale name contains no NUL");
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
    #[cfg(target_env = "musl")]
    let (decimal, thousands) = {
        let data = projected(name, span)?;
        (Value::Bytes(data.decimal_point.clone()), Value::Bytes(data.thousands_separator.clone()))
    };
    #[cfg(not(target_env = "musl"))]
    let (decimal, thousands) = {
        let locale = Locale::open(name, libc::LC_NUMERIC_MASK, span)?;
        (locale.bytes(libc::RADIXCHAR, span)?, locale.bytes(libc::THOUSEP, span)?)
    };
    Ok(Value::Record(RecordMap::from([
        ("decimal_point".into(), decimal),
        ("thousands_separator".into(), thousands),
    ])))
}

pub(crate) fn time_info(name: &str, span: Span) -> Result<Value, RuntimeError> {
    #[cfg(target_env = "musl")]
    let months = projected(name, span)?.abbreviated_months.iter().cloned().map(Value::Bytes).collect();
    #[cfg(not(target_env = "musl"))]
    let months = {
        let locale = Locale::open(name, libc::LC_TIME_MASK, span)?;
        [libc::ABMON_1, libc::ABMON_2, libc::ABMON_3, libc::ABMON_4,
            libc::ABMON_5, libc::ABMON_6, libc::ABMON_7, libc::ABMON_8,
            libc::ABMON_9, libc::ABMON_10, libc::ABMON_11, libc::ABMON_12]
            .into_iter().map(|item| locale.bytes(item, span)).collect::<Result<_, _>>()?
    };
    Ok(Value::Record(RecordMap::from([
        ("abbreviated_months".into(), Value::List(months)),
    ])))
}

#[cfg(test)]
mod tests {
    use super::projection;

    #[test]
    fn embedded_metadata_preserves_byte_domains_and_complete_months() {
        let data = projection();
        for metadata in data.locales.values() {
            assert!(!metadata.decimal_point.is_empty());
            assert_eq!(metadata.abbreviated_months.len(), 12);
            assert!(metadata.abbreviated_months.iter().all(|month| !month.is_empty()));
        }
        for canonical in data.aliases.values() {
            assert!(data.locales.contains_key(canonical));
        }
        let utf8 = &data.locales["fr_FR.utf8"];
        let iso = &data.locales["fr_FR.iso88591"];
        assert_eq!(utf8.decimal_point, b",");
        assert_eq!(utf8.thousands_separator, "\u{202f}".as_bytes());
        assert_eq!(utf8.abbreviated_months[1], "févr.".as_bytes());
        assert_eq!(iso.thousands_separator, b"\xa0");
        assert_eq!(iso.abbreviated_months[1], b"f\xe9vr.");
    }
}
