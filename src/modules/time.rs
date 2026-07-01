use crate::runtime::value::RuntimeError;
use crate::source::Span;
use jiff::{Timestamp, fmt::strtime, tz::TimeZone};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn now_epoch_ms() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis().min(i64::MAX as u128) as i64,
        Err(error) => -(error.duration().as_millis().min(i64::MAX as u128) as i64),
    }
}

pub(crate) fn duration_compact(seconds: i64) -> String {
    let mut rest = seconds.max(0);
    let ss = rest % 60;
    rest /= 60;
    let mm = rest % 60;
    rest /= 60;
    let hh = rest % 24;
    let dd = rest / 24;

    if dd > 0 {
        return format!("{dd:>3}d{hh:02}h");
    }

    if hh > 0 {
        return format!("  {hh:>2}h{mm:02}m");
    }

    format!("   {mm:>2}:{ss:02}")
}

pub(crate) fn format_epoch_ms(
    epoch_ms: i64,
    format: &str,
    utc: bool,
    span: Span,
) -> Result<String, RuntimeError> {
    let timestamp = Timestamp::from_millisecond(epoch_ms).map_err(|error| {
        RuntimeError::new("time-format", format!("timestamp is out of range: {error}"))
            .with_span(span)
    })?;
    let time_zone = if utc {
        TimeZone::UTC
    } else {
        TimeZone::try_system().map_err(|error| {
            RuntimeError::new(
                "time-format",
                format!("local timezone lookup failed: {error}"),
            )
            .with_span(span)
        })?
    };
    let zoned = timestamp.to_zoned(time_zone);
    strtime::format(format, &zoned).map_err(|error| {
        RuntimeError::new("time-format", format!("format failed: {error}")).with_span(span)
    })
}
