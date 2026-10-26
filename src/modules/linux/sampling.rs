//! One counter snapshot for process, CPU, paging, memory and block-I/O tools.

use crate::runtime::value::{RecordMap, RuntimeError, Value};
use crate::source::Span;
use std::collections::BTreeMap;
use std::path::Path;
use std::io::Read;
use std::sync::Arc;

fn error(message: impl Into<String>, span: Span) -> RuntimeError {
    RuntimeError::new("linux-sample", message).with_span(span)
}

fn read(root: &Path, name: &str, span: Span) -> Result<String, RuntimeError> {
    let file = std::fs::File::open(root.join(name))
        .map_err(|failure| RuntimeError::host("linux-sample", &failure).with_span(span))?;
    // Counter tables can scale with CPUs and block devices, but a caller's
    // fixture directory must not turn one snapshot into an unbounded read.
    const MAX_TABLE_BYTES: u64 = 64 * 1024 * 1024;
    let mut text = String::new();
    file.take(MAX_TABLE_BYTES + 1).read_to_string(&mut text)
        .map_err(|failure| RuntimeError::host("linux-sample", &failure).with_span(span))?;
    if text.len() as u64 > MAX_TABLE_BYTES {
        return Err(error(format!("kernel table `{name}` exceeds 64 MiB"), span));
    }
    Ok(text)
}

fn counter(text: &str, span: Span) -> Result<i64, RuntimeError> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(error(format!("invalid decimal kernel counter `{text}`"), span));
    }
    text.parse::<i64>().ok().filter(|number| *number >= 0)
        .ok_or_else(|| error(format!("invalid nonnegative kernel counter `{text}`"), span))
}

fn rows(text: &str, span: Span) -> Result<BTreeMap<&str, Vec<&str>>, RuntimeError> {
    let mut rows = BTreeMap::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let mut words = line.split_whitespace();
        let name = words.next().expect("nonempty counter line");
        if rows.insert(name, words.collect()).is_some() {
            return Err(error(format!("duplicate kernel counter `{name}`"), span));
        }
    }
    Ok(rows)
}

fn required(rows: &BTreeMap<&str, Vec<&str>>, name: &str, span: Span) -> Result<i64, RuntimeError> {
    let fields = rows.get(name).ok_or_else(|| error(format!("missing kernel counter `{name}`"), span))?;
    counter(fields.first().ok_or_else(|| error(format!("empty kernel counter `{name}`"), span))?, span)
}

fn record(fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Record(RecordMap::from_iter(fields.into_iter().map(|(name, value)| (Arc::from(name), value))))
}

fn uptime_ms(text: &str, span: Span) -> Result<i64, RuntimeError> {
    let uptime = text.split_whitespace().next().ok_or_else(|| error("empty uptime", span))?;
    let (seconds, fraction) = uptime.split_once('.').unwrap_or((uptime, ""));
    if !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(error("malformed uptime fraction", span));
    }
    let milliseconds = fraction.bytes().take(3).chain(std::iter::repeat(b'0')).take(3)
        .fold(0_i64, |value, byte| value * 10 + i64::from(byte - b'0'));
    counter(seconds, span)?.checked_mul(1000).and_then(|value| value.checked_add(milliseconds))
        .ok_or_else(|| error("uptime does not fit in milliseconds", span))
}

fn cpu(rows: &BTreeMap<&str, Vec<&str>>, span: Span) -> Result<Value, RuntimeError> {
    let fields = rows.get("cpu").ok_or_else(|| error("missing aggregate CPU counters", span))?;
    let names = ["user", "nice", "system", "idle", "iowait", "irq", "softirq", "steal"];
    if fields.len() < names.len() {
        return Err(error("incomplete aggregate CPU counters", span));
    }
    let values = names.into_iter().zip(fields).map(|(name, value)| Ok((name, Value::Int(counter(value, span)?))))
        .collect::<Result<Vec<_>, RuntimeError>>()?;
    Ok(record(values))
}

fn disks(text: &str, span: Span) -> Result<Value, RuntimeError> {
    let names = ["reads_completed", "reads_merged", "sectors_read", "read_ms", "writes_completed",
        "writes_merged", "sectors_written", "write_ms", "in_flight", "io_ms", "weighted_io_ms"];
    let mut records = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 14 {
            return Err(error("incomplete diskstats record", span));
        }
        let major = counter(fields[0], span)?;
        let minor = counter(fields[1], span)?;
        let mut values = vec![("name", Value::Str(fields[2].into())),
            ("major", Value::Int(major)), ("minor", Value::Int(minor))];
        for (name, value) in names.into_iter().zip(&fields[3..14]) {
            values.push((name, Value::Int(counter(value, span)?)));
        }
        records.push(((major, minor), record(values)));
    }
    records.sort_unstable_by_key(|(device, _)| *device);
    Ok(Value::List(records.into_iter().map(|(_, value)| value).collect()))
}

/// Counters come from several kernel files and are not an atomic transaction.
/// PID plus start_ticks identifies a process across samples; disk sectors are
/// always 512 bytes, pgpgin/out are KiB, and swap counters are host pages.
#[cfg(target_os = "linux")]
pub(crate) fn sample(proc_root: Option<&Path>, span: Span) -> Result<Value, RuntimeError> {
    let root = proc_root.unwrap_or_else(|| Path::new("/proc"));
    let stat_text = read(root, "stat", span)?;
    let stat = rows(&stat_text, span)?;
    let vm_text = read(root, "vmstat", span)?;
    let vm = rows(&vm_text, span)?;
    let tick_rate = i64::try_from(rustix::param::clock_ticks_per_second())
        .ok().filter(|ticks| *ticks > 0).ok_or_else(|| error("invalid host clock tick rate", span))?;
    let boot_time_ms = required(&stat, "btime", span)?.checked_mul(1000)
        .ok_or_else(|| error("boot time does not fit in milliseconds", span))?;
    let clock = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    let sampled_at_ms = clock.tv_sec.checked_mul(1000).and_then(|seconds| seconds.checked_add(clock.tv_nsec / 1_000_000))
        .ok_or_else(|| error("monotonic time does not fit in milliseconds", span))?;
    let mut fields = vec![
        ("sampled_at_ms", Value::Int(sampled_at_ms)),
        ("uptime_ms", Value::Int(uptime_ms(&read(root, "uptime", span)?, span)?)),
        ("ticks_per_second", Value::Int(tick_rate)),
        ("page_size", Value::Int(rustix::param::page_size() as i64)),
        ("cpu", cpu(&stat, span)?),
        ("memory", super::real::parse_meminfo(&read(root, "meminfo", span)?, span)?),
        ("processes", Value::List(crate::modules::process::sample_processes(root, boot_time_ms, tick_rate, span)?)),
        ("disks", disks(&read(root, "diskstats", span)?, span)?),
    ];
    for (name, kernel) in [("context_switches", "ctxt"), ("processes_created", "processes"),
        ("running", "procs_running"), ("blocked", "procs_blocked"), ("interrupts", "intr")] {
        fields.push((name, Value::Int(required(&stat, kernel, span)?)));
    }
    for (name, kernel) in [("page_in_kib", "pgpgin"), ("page_out_kib", "pgpgout"),
        ("swap_in_pages", "pswpin"), ("swap_out_pages", "pswpout")] {
        fields.push((name, Value::Int(required(&vm, kernel, span)?)));
    }
    Ok(record(fields))
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn sample(_proc_root: Option<&Path>, span: Span) -> Result<Value, RuntimeError> {
    Err(RuntimeError::new("linux-unsupported", "Linux sampling requires Linux").with_span(span))
}
