use super::SYSLOG_ACTION_SIZE_BUFFER;
use super::common::io_error;
use super::{DEV_KMSG, PROC_MEMINFO, SYSLOG_ACTION_READ_ALL};
use crate::modules::linux::str_value;
use crate::runtime::value::{LiveStream, RuntimeError, StreamValue, Value};
use crate::source::Span;
use rustc_hash::FxHashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::sync::Arc;

const PROC_MODULES: &str = "/proc/modules";

pub(crate) fn meminfo(span: Span) -> Result<Value, RuntimeError> {
    let text = match fs::read_to_string(PROC_MEMINFO) {
        Ok(text) => text,
        Err(error) => return Ok(io_error("linux-meminfo", error, span)),
    };
    match parse_meminfo(&text, span) {
        Ok(value) => Ok(Value::ok(value)),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
    }
}

pub(crate) fn modules(span: Span) -> Result<Value, RuntimeError> {
    let text = match fs::read_to_string(PROC_MODULES) {
        Ok(text) => text,
        Err(error) => return Ok(io_error("linux-modules", error, span)),
    };
    Ok(Value::ok(Value::stream(StreamValue::from_live(
        "linux.modules",
        ModuleStream {
            lines: text
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
                .into_iter(),
        },
    ))))
}

struct ModuleStream {
    lines: std::vec::IntoIter<String>,
}

impl LiveStream for ModuleStream {
    fn next(&mut self, span: Span) -> Result<Option<Value>, RuntimeError> {
        self.lines
            .next()
            .map(|line| parse_module_line(&line, span))
            .transpose()
    }
}

pub(crate) fn dmesg(span: Span) -> Result<Value, RuntimeError> {
    match read_kmsg() {
        Ok(messages) => {
            return Ok(Value::ok(Value::stream(StreamValue::from_live(
                "linux.dmesg",
                KernelMessageStream {
                    messages: messages.into_iter(),
                },
            ))));
        }
        Err(error) if !matches!(error.kind(), io::ErrorKind::NotFound) => {
            if !matches!(
                error.raw_os_error(),
                Some(libc::EACCES | libc::EPERM | libc::ENODEV)
            ) {
                return Ok(io_error("linux-dmesg", error, span));
            }
        }
        Err(_) => {}
    }

    match read_kernel_log() {
        Ok(messages) => Ok(Value::ok(Value::stream(StreamValue::from_live(
            "linux.dmesg",
            KernelMessageStream {
                messages: messages.into_iter(),
            },
        )))),
        Err(error) => Ok(io_error("linux-dmesg", error, span)),
    }
}

struct KernelMessageStream {
    messages: std::vec::IntoIter<String>,
}

impl LiveStream for KernelMessageStream {
    fn next(&mut self, _span: Span) -> Result<Option<Value>, RuntimeError> {
        Ok(self.messages.next().map(str_value))
    }
}

fn parse_meminfo(text: &str, span: Span) -> Result<Value, RuntimeError> {
    let mut values = FxHashMap::default();
    for line in text.lines() {
        let Some((key, value)) = parse_meminfo_line(line, span)? else {
            continue;
        };
        values.insert(key, value);
    }

    Ok(Value::Record(crate::runtime::value::RecordMap::from([
        (
            Arc::from("total"),
            Value::Int(required_meminfo_value(&values, "MemTotal", span)?),
        ),
        (
            Arc::from("free"),
            Value::Int(required_meminfo_value(&values, "MemFree", span)?),
        ),
        (
            Arc::from("available"),
            Value::Int(required_meminfo_value(&values, "MemAvailable", span)?),
        ),
        (
            Arc::from("buffers"),
            Value::Int(required_meminfo_value(&values, "Buffers", span)?),
        ),
        (
            Arc::from("cached"),
            Value::Int(required_meminfo_value(&values, "Cached", span)?),
        ),
        (
            Arc::from("swap_total"),
            Value::Int(required_meminfo_value(&values, "SwapTotal", span)?),
        ),
        (
            Arc::from("swap_free"),
            Value::Int(required_meminfo_value(&values, "SwapFree", span)?),
        ),
    ])))
}

fn parse_meminfo_line(line: &str, span: Span) -> Result<Option<(String, i64)>, RuntimeError> {
    let Some((key, rest)) = line.split_once(':') else {
        return Ok(None);
    };
    let mut fields = rest.split_whitespace();
    let Some(value) = fields.next() else {
        return Ok(None);
    };
    let Some(unit) = fields.next() else {
        return Ok(None);
    };
    if unit != "kB" {
        return Ok(None);
    }
    let value = value.parse::<i64>().map_err(|_| {
        RuntimeError::new(
            "linux-meminfo",
            format!("invalid numeric value for `{key}` in {PROC_MEMINFO}"),
        )
        .with_span(span)
    })?;
    Ok(Some((key.to_string(), value.saturating_mul(1024))))
}

fn required_meminfo_value(
    values: &FxHashMap<String, i64>,
    name: &str,
    span: Span,
) -> Result<i64, RuntimeError> {
    values.get(name).copied().ok_or_else(|| {
        RuntimeError::new(
            "linux-meminfo",
            format!("missing `{name}` in {PROC_MEMINFO}"),
        )
        .with_span(span)
    })
}

#[cfg(test)]
mod tests {
    use super::{parse_meminfo, parse_meminfo_line, parse_module_line};
    use crate::runtime::value::Value;
    use crate::source::{SourceId, Span};

    fn span() -> Span {
        Span::new(SourceId::new(0), 0, 0)
    }

    fn field(value: &Value, name: &str) -> i64 {
        let Value::Record(record) = value else {
            panic!("meminfo must be a record");
        };
        let Some(Value::Int(value)) = record.get(name) else {
            panic!("meminfo must contain integer {name}");
        };
        *value
    }

    #[test]
    fn meminfo_parser_preserves_units_duplicates_and_saturation() {
        let valid = include_str!("../../../../tests/fixtures/stdlib/meminfo/valid.txt");
        let value = parse_meminfo(valid, span()).expect("valid meminfo");
        assert_eq!(field(&value, "total"), 16_384_000 * 1024);
        assert_eq!(field(&value, "free"), 2_097_152 * 1024);
        assert_eq!(field(&value, "buffers"), 262_144 * 1024);

        let saturation = include_str!("../../../../tests/fixtures/stdlib/meminfo/saturation.txt");
        let value = parse_meminfo(saturation, span()).expect("saturating values");
        assert_eq!(field(&value, "total"), i64::MAX - 1023);
        assert_eq!(field(&value, "free"), i64::MAX);
        assert_eq!(field(&value, "available"), i64::MIN);
    }

    #[test]
    fn meminfo_parser_reports_errors_in_file_then_field_order() {
        let malformed = include_str!("../../../../tests/fixtures/stdlib/meminfo/malformed_unreported.txt");
        let error = parse_meminfo(malformed, span()).err().expect("malformed value");
        assert_eq!(error.kind, "linux-meminfo");
        assert!(error.message.contains("invalid numeric value"));

        let missing = include_str!("../../../../tests/fixtures/stdlib/meminfo/missing_available.txt");
        let error = parse_meminfo(missing, span()).err().expect("missing value");
        assert_eq!(error.kind, "linux-meminfo");
        assert!(error.message.contains("MemAvailable"));
    }

    #[test]
    fn meminfo_parser_rejects_non_decimal_field_spellings() {
        let spellings = include_str!("../../../../tests/fixtures/stdlib/integers/field_spellings.txt");
        for row in spellings.lines().filter(|line| !line.starts_with('#')) {
            let (spelling, expected) = row.split_once('\t').expect("fixture row");
            if spelling.trim() != spelling || spelling.is_empty() {
                continue;
            }
            let line = format!("MemTotal: {spelling} kB");
            let parsed = parse_meminfo_line(&line, span());
            if expected == "null" {
                assert!(parsed.is_err() || parsed.unwrap().is_none(), "{spelling:?}");
            } else {
                let (_, value) = parsed.expect("accepted spelling").expect("parsed line");
                let number: i64 = expected.parse().expect("fixture expectation");
                assert_eq!(value, number.saturating_mul(1024), "{spelling:?}");
            }
        }
    }

    #[test]
    fn module_rows_preserve_fields_and_report_malformed_input() {
        let rows = include_str!("../../../../tests/fixtures/stdlib/modules/rows.txt");
        let mut rows = rows.lines().filter(|line| !line.trim().is_empty());
        let first = parse_module_line(rows.next().expect("first row"), span()).expect("valid row");
        let Value::Record(record) = first else { panic!("module row record") };
        assert_eq!(record.get("name"), Some(&Value::Str("core".into())));
        assert_eq!(record.get("size"), Some(&Value::Int(4096)));
        assert_eq!(
            record.get("used_by"),
            Some(&Value::List(vec![Value::Str("dep_a".into()), Value::Str("dep_b".into())]))
        );
        let second = parse_module_line(rows.next().expect("second row"), span()).expect("valid row");
        let Value::Record(record) = second else { panic!("module row record") };
        assert_eq!(record.get("used_by"), Some(&Value::List(vec![])));
        let third = parse_module_line(rows.next().expect("third row"), span()).expect("valid row");
        let Value::Record(record) = third else { panic!("module row record") };
        assert_eq!(record.get("name"), Some(&Value::Str("dep_b".into())));
        assert_eq!(record.get("used_by"), Some(&Value::List(vec![Value::Str("core".into())])));

        for (fixture, message) in [
            (include_str!("../../../../tests/fixtures/stdlib/modules/missing_size.txt"), "size"),
            (include_str!("../../../../tests/fixtures/stdlib/modules/malformed_size.txt"), "size"),
            (include_str!("../../../../tests/fixtures/stdlib/modules/missing_use_count.txt"), "use count"),
            (include_str!("../../../../tests/fixtures/stdlib/modules/short_row.txt"), "malformed"),
        ] {
            let error = parse_module_line(fixture.trim_end(), span()).err().expect("bad row");
            assert_eq!(error.kind, "linux-modules");
            assert!(error.message.contains(message), "{error:?}");
        }
    }
}

fn parse_module_line(line: &str, span: Span) -> Result<Value, RuntimeError> {
    let mut fields = line.split_whitespace();
    let name = fields
        .next()
        .ok_or_else(|| malformed_modules_line(span))?
        .to_string();
    let size = parse_i64_field(fields.next(), "size", span)?;
    let _used_by_count = parse_i64_field(fields.next(), "use count", span)?;
    let used_by = fields.next().ok_or_else(|| malformed_modules_line(span))?;
    if fields.next().is_none() || fields.next().is_none() {
        return Err(malformed_modules_line(span));
    }

    Ok(Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("name"), str_value(name)),
        (Arc::from("size"), Value::Int(size)),
        (
            Arc::from("used_by"),
            Value::List(
                used_by
                    .trim_end_matches(',')
                    .split(',')
                    .filter(|item| !item.is_empty() && *item != "-")
                    .map(|item| str_value(item.to_string()))
                    .collect(),
            ),
        ),
    ])))
}

fn malformed_modules_line(span: Span) -> RuntimeError {
    RuntimeError::new("linux-modules", format!("malformed line in {PROC_MODULES}")).with_span(span)
}

fn parse_i64_field(field: Option<&str>, name: &str, span: Span) -> Result<i64, RuntimeError> {
    let value = field.ok_or_else(|| {
        RuntimeError::new("linux-modules", format!("missing {name} in {PROC_MODULES}"))
            .with_span(span)
    })?;
    value.parse::<i64>().map_err(|_| {
        RuntimeError::new(
            "linux-modules",
            format!("invalid {name} `{value}` in {PROC_MODULES}"),
        )
        .with_span(span)
    })
}

fn read_kmsg() -> io::Result<Vec<String>> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(DEV_KMSG)?;
    let mut text = String::new();
    match file.read_to_string(&mut text) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::WouldBlock && !text.is_empty() => {}
        Err(error) => return Err(error),
    }
    Ok(text
        .lines()
        .filter_map(parse_kmsg_line)
        .map(str::to_owned)
        .collect())
}

fn parse_kmsg_line(line: &str) -> Option<&str> {
    line.split_once(';').map(|(_, message)| message)
}

fn read_kernel_log() -> io::Result<Vec<String>> {
    let size = klogctl(SYSLOG_ACTION_SIZE_BUFFER, std::ptr::null_mut(), 0)?;
    if size <= 0 {
        return Ok(Vec::new());
    }
    let mut buffer = vec![0_u8; size as usize];
    let read = klogctl(
        SYSLOG_ACTION_READ_ALL,
        buffer.as_mut_ptr().cast(),
        buffer.len() as libc::c_int,
    )?;
    let text = String::from_utf8_lossy(&buffer[..read as usize]);
    Ok(text
        .lines()
        .map(strip_syslog_prefix)
        .map(str::to_owned)
        .collect())
}

fn klogctl(
    action: libc::c_int,
    buffer: *mut libc::c_char,
    length: libc::c_int,
) -> io::Result<libc::c_int> {
    // SAFETY: the action code is selected by this module; when a buffer is
    // provided it points to writable memory for at least `length` bytes.
    let rc = unsafe { libc::klogctl(action, buffer, length) };
    if rc >= 0 {
        Ok(rc)
    } else {
        Err(io::Error::last_os_error())
    }
}

fn strip_syslog_prefix(line: &str) -> &str {
    let Some(rest) = line.strip_prefix('<') else {
        return line;
    };
    let digits = rest.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digits == 0 || rest.as_bytes().get(digits) != Some(&b'>') {
        return line;
    }
    &rest[digits + 1..]
}
