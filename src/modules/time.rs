use crate::runtime::value::RuntimeError;
use crate::source::Span;
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_FORMAT_FIELD_WIDTH: usize = u16::MAX as usize;
const MAX_FORMAT_OUTPUT_BYTES: usize = 1024 * 1024;
const MAX_DATETIME_INPUT_BYTES: usize = 1024 * 1024;

pub(crate) fn now_epoch_ms() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis().min(i64::MAX as u128) as i64,
        Err(error) => -(error.duration().as_millis().min(i64::MAX as u128) as i64),
    }
}

pub(crate) fn wall_now() -> (i64, i64) {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => (
            duration.as_secs().min(i64::MAX as u64) as i64,
            i64::from(duration.subsec_nanos()),
        ),
        Err(error) => {
            let duration = error.duration();
            let seconds = duration.as_secs().min(i64::MAX as u64) as i64;
            let nanoseconds = i64::from(duration.subsec_nanos());
            if nanoseconds == 0 {
                (-seconds, 0)
            } else {
                (-seconds - 1, 1_000_000_000 - nanoseconds)
            }
        }
    }
}

#[cfg(unix)]
pub(crate) fn clock_resolution(span: Span) -> Result<(i64, i64), RuntimeError> {
    let mut resolution = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    if unsafe { libc::clock_getres(libc::CLOCK_REALTIME, &mut resolution) } != 0 {
        let error = std::io::Error::last_os_error();
        return Err(RuntimeError::host("clock-resolution", &error).with_span(span));
    }
    let seconds = i64::try_from(resolution.tv_sec).map_err(|_| {
        RuntimeError::new("clock-resolution", "clock resolution seconds are out of range")
            .with_span(span)
    })?;
    let nanoseconds = i64::try_from(resolution.tv_nsec).map_err(|_| {
        RuntimeError::new(
            "clock-resolution",
            "clock resolution nanoseconds are out of range",
        )
        .with_span(span)
    })?;
    if seconds < 0 || !(0..1_000_000_000).contains(&nanoseconds) {
        return Err(RuntimeError::new(
            "clock-resolution",
            "the host returned an invalid realtime clock resolution",
        )
        .with_span(span));
    }
    Ok((seconds, nanoseconds))
}

#[cfg(not(unix))]
pub(crate) fn clock_resolution(span: Span) -> Result<(i64, i64), RuntimeError> {
    Err(RuntimeError::new(
        "clock-resolution",
        "realtime clock resolution is unavailable on this platform",
    )
    .with_span(span))
}

pub(crate) fn format_timestamp(
    seconds: i64,
    nanoseconds: i64,
    format: &str,
    timezone: &str,
    calendar: &str,
    locale: &str,
    span: Span,
) -> Result<String, RuntimeError> {
    if !(0..1_000_000_000).contains(&nanoseconds) {
        return Err(RuntimeError::new(
            "time-format",
            "nanoseconds must be between 0 and 999999999",
        )
        .with_span(span));
    }
    if !matches!(calendar, "locale" | "gregorian") {
        return Err(RuntimeError::new(
            "time-format",
            "calendar must be `locale` or `gregorian`",
        )
        .with_span(span));
    }
    let timestamp = match Timestamp::new(seconds, nanoseconds as i32) {
        Ok(timestamp) => timestamp,
        Err(_) => {
            return format_extended_timestamp(
                seconds,
                nanoseconds as u32,
                format,
                timezone,
                calendar,
                locale,
            )
            .map_err(|error| RuntimeError::new("time-format", error).with_span(span));
        }
    };
    let timezone = resolve_timezone(timezone, "time-format", span)?;
    let zoned = timestamp.to_zoned(timezone);
    format_strftime(&zoned, format, calendar, locale)
        .map_err(|error| RuntimeError::new("time-format", error).with_span(span))
}

pub(crate) fn parse_timestamp(
    input: &str,
    reference_seconds: i64,
    timezone: &str,
    span: Span,
) -> Result<(i64, i64), RuntimeError> {
    if input.len() > MAX_DATETIME_INPUT_BYTES {
        return Err(RuntimeError::new(
            "time-parse",
            "date input exceeds 1 MiB",
        )
        .with_span(span));
    }
    let reference = Timestamp::from_second(reference_seconds).map_err(|error| {
        RuntimeError::new("time-parse", error.to_string()).with_span(span)
    })?;
    let timezone = resolve_timezone(timezone, "time-parse", span)?;
    let base = reference.to_zoned(timezone);
    let parsed = parse_datetime::parse_datetime_at_date(base, input).map_err(|error| {
        RuntimeError::new("time-parse", error.to_string()).with_span(span)
    })?;
    Ok((parsed.unix_epoch_second(), i64::from(parsed.subsec_nanosecond())))
}

fn resolve_timezone(timezone: &str, kind: &str, span: Span) -> Result<TimeZone, RuntimeError> {
    let result = match timezone {
        "local" | "" => TimeZone::try_system(),
        "UTC" | "utc" | "GMT" | "gmt" | "Z" => Ok(TimeZone::UTC),
        value => TimeZone::get(value).or_else(|_| TimeZone::posix(value)),
    };
    result.map_err(|error| {
        RuntimeError::new(kind, format!("invalid timezone `{timezone}`: {error}"))
            .with_span(span)
    })
}

#[cfg(target_os = "linux")]
fn format_strftime(
    date: &Zoned,
    format: &str,
    calendar: &str,
    locale: &str,
) -> Result<String, String> {
    let timestamp = date.timestamp();
    let offset = date.offset();
    let offset_info = date.time_zone().to_offset_info(timestamp);
    let zone = std::ffi::CString::new(offset_info.abbreviation())
        .map_err(|_| "timezone abbreviation contains a NUL byte".to_string())?;
    let mut broken_down: libc::tm = unsafe { std::mem::zeroed() };
    broken_down.tm_sec = i32::from(date.second());
    broken_down.tm_min = i32::from(date.minute());
    broken_down.tm_hour = i32::from(date.hour());
    broken_down.tm_mday = i32::from(date.day());
    broken_down.tm_mon = i32::from(date.month()) - 1;
    broken_down.tm_year = i32::from(date.year()) - 1900;
    broken_down.tm_wday = i32::from(date.weekday().to_sunday_zero_offset());
    broken_down.tm_yday = i32::from(date.day_of_year()) - 1;
    broken_down.tm_isdst = i32::from(offset_info.dst().is_dst());
    broken_down.tm_gmtoff = libc::c_long::from(offset.seconds());
    broken_down.tm_zone = zone.as_ptr();
    apply_locale_calendar(&mut broken_down, calendar, locale)?;
    let year = i64::from(broken_down.tm_year) + 1900;
    let month = broken_down.tm_mon + 1;
    let day = broken_down.tm_mday;
    let subsecond = timestamp.subsec_nanosecond();
    let nanoseconds = if subsecond < 0 {
        u32::try_from(1_000_000_000 + subsecond)
            .map_err(|_| "timestamp nanoseconds are out of range".to_string())?
    } else {
        u32::try_from(subsecond)
            .map_err(|_| "timestamp nanoseconds are out of range".to_string())?
    };
    let epoch_seconds = timestamp.as_nanosecond().div_euclid(1_000_000_000);
    format_tm(
        &broken_down,
        format,
        nanoseconds,
        epoch_seconds,
        year,
        month,
        day,
        locale,
    )
}

#[cfg(not(target_os = "linux"))]
fn format_strftime(
    date: &Zoned,
    format: &str,
    _calendar: &str,
    _locale: &str,
) -> Result<String, String> {
    if format.len() > MAX_FORMAT_OUTPUT_BYTES {
        return Err("strftime format exceeds 1 MiB".to_string());
    }
    let bytes = format.as_bytes();
    let mut output = String::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let literal_start = cursor;
        while cursor < bytes.len() && bytes[cursor] != b'%' {
            cursor += 1;
        }
        if literal_start < cursor {
            let literal = std::str::from_utf8(&bytes[literal_start..cursor])
                .map_err(|_| "format is not valid UTF-8".to_string())?;
            append_bounded(&mut output, literal)?;
        }
        if cursor == bytes.len() {
            break;
        }
        if bytes.get(cursor + 1) == Some(&b'%') {
            append_bounded(&mut output, "%")?;
            cursor += 2;
            continue;
        }

        let spec_start = cursor;
        cursor += 1;
        let flags_start = cursor;
        while bytes
            .get(cursor)
            .is_some_and(|flag| b"_-0^#+".contains(flag))
        {
            cursor += 1;
        }
        let flags = &format[flags_start..cursor];

        let width_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        let width = if cursor == width_start {
            None
        } else {
            let parsed = format[width_start..cursor]
                .parse::<usize>()
                .unwrap_or(usize::MAX);
            if parsed > MAX_FORMAT_FIELD_WIDTH {
                return Err(format!(
                    "strftime field width exceeds {MAX_FORMAT_FIELD_WIDTH}"
                ));
            }
            Some(parsed)
        };

        let mut colons = 0;
        while bytes.get(cursor) == Some(&b':') && colons < 3 {
            cursor += 1;
            colons += 1;
        }
        if bytes
            .get(cursor)
            .is_some_and(|byte| *byte == b'E' || *byte == b'O')
        {
            cursor += 1;
        }
        if flags == "+"
            && width.is_none()
            && colons == 0
            && bytes
                .get(cursor)
                .is_none_or(|specifier| !specifier.is_ascii_alphanumeric())
        {
            let rendered = jiff::fmt::strtime::format("%a %b %e %H:%M:%S %Z %Y", date)
                .map_err(|error| error.to_string())?;
            append_bounded(&mut output, &rendered)?;
            continue;
        }
        let Some(&specifier) = bytes.get(cursor) else {
            let literal = std::str::from_utf8(&bytes[spec_start..cursor])
                .map_err(|_| "format is not valid UTF-8".to_string())?;
            append_bounded(&mut output, literal)?;
            continue;
        };
        if !specifier.is_ascii_alphabetic() && specifier != b'+' {
            let literal = std::str::from_utf8(&bytes[spec_start..=cursor])
                .map_err(|_| "format is not valid UTF-8".to_string())?;
            append_bounded(&mut output, literal)?;
            cursor += 1;
            continue;
        }
        cursor += 1;

        let name = if specifier == b'+' {
            "+".to_string()
        } else {
            format!("{}{}", ":".repeat(colons), char::from(specifier))
        };
        let rendered = if name == "+" {
            jiff::fmt::strtime::format("%a %b %e %H:%M:%S %Z %Y", date)
                .map_err(|error| error.to_string())?
        } else if name == "s" {
            date.timestamp()
                .as_nanosecond()
                .div_euclid(1_000_000_000)
                .to_string()
        } else if name == "r" {
            jiff::fmt::strtime::format("%I:%M:%S %p", date)
                .map_err(|error| error.to_string())?
        } else {
            let directive = format!("%{name}");
            jiff::fmt::strtime::format(&directive, date).map_err(|error| error.to_string())?
        };
        let rendered = apply_format_modifiers(&rendered, flags, width, &name)?;
        append_bounded(&mut output, &rendered)?;
    }
    Ok(output)
}

#[cfg(target_os = "linux")]
fn format_extended_timestamp(
    seconds: i64,
    nanoseconds: u32,
    format: &str,
    timezone: &str,
    calendar: &str,
    locale: &str,
) -> Result<String, String> {
    if format.len() > MAX_FORMAT_OUTPUT_BYTES {
        return Err("strftime format exceeds 1 MiB".to_string());
    }
    let use_utc = matches!(timezone, "UTC" | "utc" | "GMT" | "gmt" | "Z");
    if !use_utc && !matches!(timezone, "local" | "") {
        return Err("extended-year formatting supports only the local timezone or UTC".to_string());
    }
    let timestamp = libc::time_t::try_from(seconds)
        .map_err(|_| "timestamp seconds are outside the host time_t range".to_string())?;
    let mut broken_down: libc::tm = unsafe { std::mem::zeroed() };
    let result = unsafe {
        if use_utc {
            libc::gmtime_r(&timestamp, &mut broken_down)
        } else {
            libc::localtime_r(&timestamp, &mut broken_down)
        }
    };
    if result.is_null() {
        return Err("the host cannot convert this extended timestamp".to_string());
    }
    if use_utc {
        broken_down.tm_zone = c"UTC".as_ptr();
        broken_down.tm_gmtoff = 0;
    }
    apply_locale_calendar(&mut broken_down, calendar, locale)?;
    let year = i64::from(broken_down.tm_year) + 1900;
    let month = broken_down.tm_mon + 1;
    let day = broken_down.tm_mday;
    format_tm(
        &broken_down,
        format,
        nanoseconds,
        i128::from(seconds),
        year,
        month,
        day,
        locale,
    )
}

#[cfg(target_os = "linux")]
fn format_tm(
    broken_down: &libc::tm,
    format: &str,
    nanoseconds: u32,
    epoch_seconds: i128,
    year: i64,
    month: i32,
    day: i32,
    locale: &str,
) -> Result<String, String> {
    if format.len() > MAX_FORMAT_OUTPUT_BYTES {
        return Err("strftime format exceeds 1 MiB".to_string());
    }
    let locale_name = if locale == "locale" { "" } else { locale };
    format_tm_in_locale(
        broken_down,
        format,
        nanoseconds,
        epoch_seconds,
        year,
        month,
        day,
        locale_name,
    )
}

#[cfg(target_os = "linux")]
fn format_tm_in_locale(
    broken_down: &libc::tm,
    format: &str,
    nanoseconds: u32,
    epoch_seconds: i128,
    year: i64,
    month: i32,
    day: i32,
    locale_name: &str,
) -> Result<String, String> {
    let locale_name = std::ffi::CString::new(locale_name)
        .map_err(|_| "time locale contains a NUL byte".to_string())?;
    let mut locale = unsafe {
        libc::newlocale(
            libc::LC_TIME_MASK,
            locale_name.as_ptr(),
            std::ptr::null_mut(),
        )
    };
    if locale.is_null() && locale_name.as_bytes().is_empty() {
        locale = unsafe { libc::newlocale(libc::LC_TIME_MASK, c"C".as_ptr(), std::ptr::null_mut()) };
    }
    if locale.is_null() {
        return Err("could not create a time locale".to_string());
    }
    let previous = unsafe { libc::uselocale(locale) };
    if previous.is_null() {
        unsafe { libc::freelocale(locale) };
        return Err("could not select a time locale".to_string());
    }
    let prepared = prepare_extended_strftime(
        format,
        nanoseconds,
        epoch_seconds,
        year,
        month,
        day,
        broken_down,
    );
    let (c_format, replacements) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            unsafe {
                libc::uselocale(previous);
                libc::freelocale(locale);
            }
            return Err(error);
        }
    };
    let mut output = vec![0_u8; MAX_FORMAT_OUTPUT_BYTES + 1];
    let written = unsafe {
        libc::strftime(
            output.as_mut_ptr().cast(),
            output.len(),
            c_format.as_ptr(),
            broken_down,
        )
    };
    let restored = unsafe { libc::uselocale(previous) };
    unsafe { libc::freelocale(locale) };
    if restored.is_null() {
        return Err("could not restore the previous time locale".to_string());
    }
    if written == 0 && !format.is_empty() {
        return Err("strftime output exceeds 1 MiB or is empty".to_string());
    }
    if written > MAX_FORMAT_OUTPUT_BYTES {
        return Err("strftime output exceeds 1 MiB".to_string());
    }
    let mut output = String::from_utf8(output[..written].to_vec())
        .map_err(|_| "strftime output is not valid UTF-8".to_string())?;
    for (placeholder, replacement) in replacements {
        output = output.replace(&placeholder, &replacement);
        if output.len() > MAX_FORMAT_OUTPUT_BYTES {
            return Err("strftime output exceeds 1 MiB".to_string());
        }
    }
    Ok(output)
}

#[cfg(target_os = "linux")]
fn apply_locale_calendar(
    tm: &mut libc::tm,
    calendar: &str,
    locale: &str,
) -> Result<(), String> {
    if calendar == "gregorian" {
        return Ok(());
    }
    if locale == "locale" {
        apply_named_locale_calendar(tm, &active_time_locale())
    } else {
        apply_named_locale_calendar(tm, locale)
    }
}

#[cfg(target_os = "linux")]
fn active_time_locale() -> String {
    let mut locale = std::env::var("LC_ALL").unwrap_or_default();
    if locale.is_empty() {
        locale = std::env::var("LC_TIME").unwrap_or_default();
    }
    if locale.is_empty() {
        locale = std::env::var("LANG").unwrap_or_default();
    }
    locale
}

#[cfg(target_os = "linux")]
fn apply_named_locale_calendar(tm: &mut libc::tm, locale: &str) -> Result<(), String> {
    let language = locale
        .split(|character| character == '.' || character == '@')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(language.as_str(), "th_th" | "fa_ir" | "am_et") {
        return Ok(());
    }
    let gregorian_year = i32::try_from(i64::from(tm.tm_year) + 1900)
        .map_err(|_| "locale calendar year is outside the supported range".to_string())?;
    if language == "th_th" {
        tm.tm_year = tm
            .tm_year
            .checked_add(543)
            .ok_or_else(|| "Thai calendar year is out of range".to_string())?;
    } else if language == "fa_ir" {
        apply_persian_calendar(tm, gregorian_year)?;
    } else if language == "am_et" {
        apply_ethiopian_calendar(tm, gregorian_year)?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn apply_persian_calendar(tm: &mut libc::tm, gregorian_year: i32) -> Result<(), String> {
    let ordinal = tm.tm_yday;
    let current_start = days_before_month(
        gregorian_year,
        3,
    ) + if is_gregorian_leap(gregorian_year) { 19 } else { 20 };
    let (year, days) = if ordinal >= current_start {
        (gregorian_year - 621, ordinal - current_start)
    } else {
        let previous_year = gregorian_year - 1;
        let previous_start = days_before_month(
            previous_year,
            3,
        ) + if is_gregorian_leap(previous_year) { 19 } else { 20 };
        let days_in_previous_year = if is_gregorian_leap(previous_year) {
            366
        } else {
            365
        };
        (
            gregorian_year - 622,
            days_in_previous_year - previous_start + ordinal,
        )
    };
    let (month, day) = solar_month_day(days);
    tm.tm_year = year - 1900;
    tm.tm_mon = month - 1;
    tm.tm_mday = day;
    tm.tm_yday = days;
    Ok(())
}

#[cfg(target_os = "linux")]
fn apply_ethiopian_calendar(tm: &mut libc::tm, gregorian_year: i32) -> Result<(), String> {
    let ordinal = tm.tm_yday;
    let current_start = days_before_month(gregorian_year, 9) + 10;
    let (year, days) = if ordinal >= current_start {
        (gregorian_year - 7, ordinal - current_start)
    } else {
        let previous_year = gregorian_year - 1;
        let previous_start = days_before_month(previous_year, 9) + 10;
        let days_in_previous_year = if is_gregorian_leap(previous_year) {
            366
        } else {
            365
        };
        (
            gregorian_year - 8,
            days_in_previous_year - previous_start + ordinal,
        )
    };
    tm.tm_year = year - 1900;
    tm.tm_mon = days / 30;
    tm.tm_mday = days % 30 + 1;
    tm.tm_yday = days;
    Ok(())
}

#[cfg(target_os = "linux")]
fn solar_month_day(days: i32) -> (i32, i32) {
    if days < 186 {
        (days / 31 + 1, days % 31 + 1)
    } else {
        let after_six_months = days - 186;
        (6 + after_six_months / 30 + 1, after_six_months % 30 + 1)
    }
}

#[cfg(target_os = "linux")]
fn days_before_month(year: i32, month: i32) -> i32 {
    const DAYS: [i32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let leap_day = i32::from(is_gregorian_leap(year) && month > 2);
    DAYS[(month - 1) as usize] + leap_day
}

#[cfg(target_os = "linux")]
fn is_gregorian_leap(year: i32) -> bool {
    year.rem_euclid(4) == 0 && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0)
}

#[cfg(not(target_os = "linux"))]
fn format_extended_timestamp(
    _seconds: i64,
    _nanoseconds: u32,
    _format: &str,
    _timezone: &str,
    _calendar: &str,
    _locale: &str,
) -> Result<String, String> {
    Err("extended-year formatting is unavailable on this platform".to_string())
}

#[cfg(target_os = "linux")]
fn prepare_extended_strftime(
    format: &str,
    nanoseconds: u32,
    epoch_seconds: i128,
    year: i64,
    month: i32,
    day: i32,
    broken_down: &libc::tm,
) -> Result<(std::ffi::CString, Vec<(String, String)>), String> {
    use std::ffi::CString;

    let bytes = format.as_bytes();
    let mut c_format = String::with_capacity(format.len());
    let mut replacements = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let literal_start = cursor;
        while cursor < bytes.len() && bytes[cursor] != b'%' {
            cursor += 1;
        }
        c_format.push_str(
            std::str::from_utf8(&bytes[literal_start..cursor])
                .map_err(|_| "format is not valid UTF-8".to_string())?,
        );
        if cursor == bytes.len() {
            break;
        }
        let spec_start = cursor;
        cursor += 1;
        let flags_start = cursor;
        while bytes
            .get(cursor)
            .is_some_and(|flag| b"_-0^#+".contains(flag))
        {
            cursor += 1;
        }
        let flags = &format[flags_start..cursor];
        let width_start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            cursor += 1;
        }
        let width = if cursor == width_start {
            None
        } else {
            let width = format[width_start..cursor]
                .parse::<usize>()
                .unwrap_or(usize::MAX);
            if width > MAX_FORMAT_FIELD_WIDTH {
                return Err(format!(
                    "strftime field width exceeds {MAX_FORMAT_FIELD_WIDTH}"
                ));
            }
            Some(width)
        };
        let mut colons = 0;
        while bytes.get(cursor) == Some(&b':') && colons < 3 {
            cursor += 1;
            colons += 1;
        }
        let modifier_byte = if bytes
            .get(cursor)
            .is_some_and(|byte| *byte == b'E' || *byte == b'O')
        {
            let modifier_byte = bytes[cursor];
            cursor += 1;
            Some(modifier_byte)
        } else {
            None
        };
        let modifier = modifier_byte.is_some();
        if flags == "+"
            && width.is_none()
            && colons == 0
            && bytes
                .get(cursor)
                .is_none_or(|specifier| !specifier.is_ascii_alphanumeric())
        {
            c_format.push_str("%a %b %e %H:%M:%S %Z %Y");
            continue;
        }
        let Some(&specifier) = bytes.get(cursor) else {
            c_format.push_str("%%");
            c_format.push_str(&format[spec_start + 1..cursor]);
            continue;
        };
        if !specifier.is_ascii_alphabetic() && specifier != b'+' {
            c_format.push_str(&format[spec_start..=cursor]);
            cursor += 1;
            continue;
        }
        cursor += 1;
        let name = if specifier == b'+' {
            "+".to_string()
        } else {
            format!("{}{}", ":".repeat(colons), char::from(specifier))
        };
        let raw = if colons > 0 && specifier == b'z' {
            Some(format_timezone_offset(broken_down.tm_gmtoff, colons)?)
        } else if colons == 0 && !modifier && specifier == b'q' {
            Some(((month - 1) / 3 + 1).to_string())
        } else if colons == 0 && !modifier && matches!(specifier, b'N' | b'F' | b's') {
            Some(if specifier == b'N' {
                format!("{nanoseconds:09}")
            } else if specifier == b's' {
                epoch_seconds.to_string()
            } else {
                format!(
                    "{}{year:04}-{month:02}-{day:02}",
                    if year > 9999 { "+" } else { "" }
                )
            })
        } else if !flags.is_empty() || width.is_some() {
            let directive = match modifier_byte {
                Some(modifier) => format!("%{}{}", char::from(modifier), char::from(specifier)),
                None => format!("%{}", char::from(specifier)),
            };
            Some(format_strftime_directive(broken_down, &directive)?)
        } else {
            None
        };
        let has_replacement = raw.is_some();
        if let Some(raw) = raw {
            let replacement = apply_format_modifiers(&raw, flags, width, &name)?;
            let mut index = replacements.len();
            let placeholder = loop {
                let candidate = format!("XSHTIME{index}TOKEN");
                if !format.contains(&candidate) {
                    break candidate;
                }
                index += 1;
            };
            c_format.push_str(&placeholder);
            replacements.push((placeholder, replacement));
        } else {
            c_format.push_str(&format[spec_start..cursor]);
        }
        if has_replacement {
            continue;
        }
    }
    if c_format.len() > MAX_FORMAT_OUTPUT_BYTES {
        return Err("strftime format exceeds 1 MiB".to_string());
    }
    let c_format = CString::new(c_format)
        .map_err(|_| "strftime format contains a NUL byte".to_string())?;
    Ok((c_format, replacements))
}

#[cfg(target_os = "linux")]
fn format_strftime_directive(broken_down: &libc::tm, directive: &str) -> Result<String, String> {
    let directive = std::ffi::CString::new(directive)
        .map_err(|_| "strftime directive contains a NUL byte".to_string())?;
    let mut output = vec![0_u8; MAX_FORMAT_OUTPUT_BYTES + 1];
    let written = unsafe {
        libc::strftime(
            output.as_mut_ptr().cast(),
            output.len(),
            directive.as_ptr(),
            broken_down,
        )
    };
    if written > MAX_FORMAT_OUTPUT_BYTES {
        return Err("strftime output exceeds 1 MiB".to_string());
    }
    String::from_utf8(output[..written].to_vec())
        .map_err(|_| "strftime output is not valid UTF-8".to_string())
}

#[cfg(target_os = "linux")]
fn format_timezone_offset(gmtoff: libc::c_long, colons: usize) -> Result<String, String> {
    let offset = i64::try_from(gmtoff)
        .map_err(|_| "timezone offset is outside the supported range".to_string())?;
    let sign = if offset < 0 { '-' } else { '+' };
    let absolute = offset.unsigned_abs();
    let hours = absolute / 3_600;
    let minutes = absolute % 3_600 / 60;
    let seconds = absolute % 60;
    let formatted = match colons {
        1 => format!("{sign}{hours:02}:{minutes:02}"),
        2 => format!("{sign}{hours:02}:{minutes:02}:{seconds:02}"),
        3 if seconds > 0 => format!("{sign}{hours:02}:{minutes:02}:{seconds:02}"),
        3 if minutes > 0 => format!("{sign}{hours:02}:{minutes:02}"),
        3 => format!("{sign}{hours:02}"),
        _ => return Err("strftime timezone offset uses too many colons".to_string()),
    };
    Ok(formatted)
}

fn apply_format_modifiers(
    original: &str,
    flags: &str,
    width: Option<usize>,
    specifier: &str,
) -> Result<String, String> {
    if specifier == "+" || matches!(specifier, "c" | "r" | "x" | "X") {
        return Ok(original.to_string());
    }

    let text_field = matches!(
        specifier,
        "a" | "A" | "b" | "B" | "h" | "p" | "P" | "Z" | "Q" | ":Q"
    );
    let default_pad = if text_field || matches!(specifier, "e" | "k" | "l") {
        ' '
    } else {
        '0'
    };
    let mut pad = default_pad;
    let mut no_pad = false;
    let mut uppercase = false;
    let mut swap_case = false;
    let mut force_sign = false;
    let mut pad_default = false;
    for flag in flags.chars() {
        match flag {
            '-' => no_pad = true,
            '_' => {
                no_pad = false;
                pad = ' ';
                pad_default = true;
            }
            '0' => {
                no_pad = false;
                pad = '0';
            }
            '^' => {
                uppercase = true;
                swap_case = false;
            }
            '#' if !uppercase => swap_case = true,
            '+' => {
                force_sign = true;
                no_pad = false;
                pad = '0';
            }
            _ => {}
        }
    }

    let mut result = original.to_string();
    if uppercase {
        result = result.to_uppercase();
    } else if swap_case {
        let all_upper = result
            .chars()
            .all(|character| !character.is_alphabetic() || character.is_uppercase());
        let all_lower = result
            .chars()
            .all(|character| !character.is_alphabetic() || character.is_lowercase());
        if all_upper {
            result = result.to_lowercase();
        } else if !all_lower {
            result = result.to_uppercase();
        }
    }

    if no_pad {
        return Ok(strip_default_padding(&result));
    }

    let target_width = width.unwrap_or_else(|| {
        if pad_default || pad != default_pad {
            default_format_width(specifier)
        } else {
            0
        }
    });
    if target_width > MAX_FORMAT_FIELD_WIDTH {
        return Err(format!(
            "strftime field width exceeds {MAX_FORMAT_FIELD_WIDTH}"
        ));
    }
    if target_width > 0 && target_width < result.len() {
        result = strip_default_padding(&result);
    }
    if !text_field && result.len() >= 2 {
        if pad == ' ' && result.starts_with('0') {
            result = strip_default_padding(&result);
        } else if pad == '0' && result.starts_with(' ') {
            result = strip_default_padding(&result);
        }
    }
    if force_sign
        && result
            .chars()
            .next()
            .is_some_and(|character| !matches!(character, '+' | '-') && character.is_ascii_digit())
    {
        let field_width = default_format_width(specifier);
        if width.is_some() || (field_width > 0 && result.len() > field_width) {
            result.insert(0, '+');
        }
    }

    if target_width > result.len() {
        let padding = target_width - result.len();
        let mut padded = String::new();
        padded
            .try_reserve_exact(target_width)
            .map_err(|_| format!("could not allocate strftime field width {target_width}"))?;
        if pad == '0' && (result.starts_with('+') || result.starts_with('-')) {
            let (sign, rest) = result.split_at(1);
            padded.push_str(sign);
            padded.extend(std::iter::repeat_n('0', padding));
            padded.push_str(rest);
        } else {
            padded.extend(std::iter::repeat_n(pad, padding));
            padded.push_str(&result);
        }
        result = padded;
    } else if specifier == "N" && target_width > 0 && target_width <= 9 {
        result.truncate(target_width);
    }
    Ok(result)
}

fn default_format_width(specifier: &str) -> usize {
    match specifier {
        "d" | "e" | "m" | "H" | "k" | "I" | "l" | "M" | "S" | "y" | "U" | "W"
        | "V" | "C" | "g" => 2,
        "j" => 3,
        "u" | "w" | "q" => 1,
        "Y" | "G" => 4,
        "N" => 9,
        _ => 0,
    }
}

fn strip_default_padding(value: &str) -> String {
    if value.starts_with('0') && value.len() >= 2 {
        let stripped = value.trim_start_matches('0');
        if stripped.is_empty() {
            return "0".to_string();
        }
        if stripped.chars().next().is_some_and(|character| character.is_ascii_digit()) {
            return stripped.to_string();
        }
    }
    if value.starts_with(' ') {
        return value.trim_start().to_string();
    }
    value.to_string()
}

#[cfg(not(target_os = "linux"))]
fn append_bounded(output: &mut String, text: &str) -> Result<(), String> {
    let new_len = output
        .len()
        .checked_add(text.len())
        .ok_or_else(|| "strftime output exceeds 1 MiB".to_string())?;
    if new_len > MAX_FORMAT_OUTPUT_BYTES {
        return Err("strftime output exceeds 1 MiB".to_string());
    }
    output
        .try_reserve(text.len())
        .map_err(|_| "could not allocate strftime output".to_string())?;
    output.push_str(text);
    Ok(())
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

pub(crate) fn format_epoch_ms_utc(epoch_ms: i64) -> String {
    let seconds = epoch_ms.div_euclid(1_000);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_epoch_days(seconds.div_euclid(86_400));
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_from_epoch_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::{
        clock_resolution, format_epoch_ms_utc, format_strftime, format_timestamp, parse_timestamp,
        wall_now,
    };
    #[cfg(target_os = "linux")]
    use super::{active_time_locale, apply_named_locale_calendar};
    use crate::source::{SourceId, Span};
    use jiff::{Timestamp, tz::TimeZone};

    #[test]
    fn format_epoch_ms_utc_handles_epoch_leap_day_and_pre_epoch_values() {
        assert_eq!(format_epoch_ms_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_epoch_ms_utc(951_782_400_000), "2000-02-29T00:00:00Z");
        assert_eq!(format_epoch_ms_utc(-1), "1969-12-31T23:59:59Z");
    }

    #[test]
    fn wall_now_returns_normalized_nanoseconds() {
        let (_, nanoseconds) = wall_now();
        assert!((0..1_000_000_000).contains(&nanoseconds));
    }

    #[test]
    fn clock_resolution_returns_normalized_fields() {
        let span = Span::at(SourceId::new(0), 0);
        let (seconds, nanoseconds) = clock_resolution(span).unwrap();
        assert!(seconds >= 0);
        assert!((0..1_000_000_000).contains(&nanoseconds));
        assert!(seconds > 0 || nanoseconds > 0);
    }

    #[test]
    fn strftime_formats_modifiers_and_rejects_unbounded_widths() {
        let date = Timestamp::UNIX_EPOCH.to_zoned(TimeZone::UTC);
        assert_eq!(
            format_strftime(&date, "%Y-%m-%d %H:%M:%S %N %_d %^B", "gregorian", "locale")
                .unwrap(),
            "1970-01-01 00:00:00 000000000  1 JANUARY"
        );
        assert!(format_strftime(&date, "%+", "gregorian", "locale")
            .unwrap()
            .contains("1970"));
        assert!(format_strftime(&date, "%99999999999c", "gregorian", "locale").is_err());
        assert_eq!(
            format_strftime(&date, "ending %", "gregorian", "locale").unwrap(),
            "ending %"
        );
    }

    #[test]
    fn strftime_epoch_seconds_floor_negative_fractional_timestamps() {
        let date = Timestamp::new(-2, 500_000_000)
            .unwrap()
            .to_zoned(TimeZone::UTC);
        assert_eq!(format_strftime(&date, "%s", "gregorian", "locale").unwrap(), "-2");
        assert_eq!(
            format_strftime(&date, "%06s", "gregorian", "locale").unwrap(),
            "-00002"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn strftime_formats_quarters_offsets_and_field_modifiers() {
        let span = Span::at(SourceId::new(0), 0);
        assert_eq!(
            format_timestamp(
                0,
                0,
                "%10Y|%-10Y|%+6Y|%q|%z|%:z|%::z|%:::z",
                "UTC",
                "gregorian",
                "C",
                span,
            )
            .unwrap(),
            "0000001970|1970|+01970|1|+0000|+00:00|+00:00:00|+00"
        );
        assert_eq!(
            format_timestamp(
                0,
                0,
                "%_10m|%-^10B",
                "UTC",
                "gregorian",
                "C",
                span,
            )
            .unwrap(),
            "         1|JANUARY"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn strftime_formats_large_years_and_zero_padded_twelve_hour_time() {
        let span = Span::at(SourceId::new(0), 0);
        assert_eq!(
            format_timestamp(
                253_402_330_668,
                123_456_789,
                "%Y %F %r %N %s %Z",
                "UTC",
                "gregorian",
                "locale",
                span,
            )
            .unwrap(),
            "10000 +10000-01-01 08:17:48 AM 123456789 253402330668 UTC"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn locale_calendars_convert_thai_persian_and_ethiopian_fields() {
        let mut fields: libc::tm = unsafe { std::mem::zeroed() };
        fields.tm_year = 2026 - 1900;
        fields.tm_yday = 0;
        fields.tm_mday = 1;
        apply_named_locale_calendar(&mut fields, "th_TH.UTF-8").unwrap();
        assert_eq!(fields.tm_year + 1900, 2569);
        assert_eq!(fields.tm_mon, 0);
        assert_eq!(fields.tm_mday, 1);

        fields.tm_year = 2026 - 1900;
        fields.tm_yday = 0;
        apply_named_locale_calendar(&mut fields, "fa_IR.UTF-8").unwrap();
        assert_eq!(fields.tm_year + 1900, 1404);
        assert_eq!(fields.tm_mon, 9);
        assert_eq!(fields.tm_mday, 11);

        fields.tm_year = 2026 - 1900;
        fields.tm_yday = 0;
        apply_named_locale_calendar(&mut fields, "am_ET.UTF-8").unwrap();
        assert_eq!(fields.tm_year + 1900, 2018);
        assert_eq!(fields.tm_mon, 3);
        assert_eq!(fields.tm_mday, 23);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn locale_strftime_uses_hungarian_month_and_weekday_names() {
        if !active_time_locale().to_ascii_lowercase().starts_with("hu_hu") {
            return;
        }
        let span = Span::at(SourceId::new(0), 0);
        let (seconds, nanoseconds) = parse_timestamp("2025-12-14 13:00:00", 0, "UTC", span).unwrap();
        assert_eq!(
            format_timestamp(
                seconds,
                nanoseconds,
                "%Y. %b %d., %A, %H:%M:%S %Z",
                "UTC",
                "locale",
                "locale",
                span,
            )
            .unwrap(),
            "2025. dec 14., vasárnap, 13:00:00 UTC"
        );
        let (seconds, nanoseconds) =
            parse_timestamp("1997-01-19 08:17:48", 0, "UTC", span).unwrap();
        assert_eq!(
            format_timestamp(
                seconds,
                nanoseconds,
                "%a, %d %b %Y %H:%M:%S %z",
                "UTC",
                "gregorian",
                "C",
                span,
            )
            .unwrap(),
            "Sun, 19 Jan 1997 08:17:48 +0000"
        );
    }

    #[test]
    fn parses_relative_and_epoch_dates_against_reference() {
        let span = Span::at(SourceId::new(0), 0);
        assert_eq!(parse_timestamp("@-1.5", 0, "UTC", span).unwrap(), (-2, 500_000_000));
        assert_eq!(parse_timestamp("+1 day", 0, "UTC", span).unwrap(), (86_400, 0));
        assert_eq!(
            parse_timestamp("10000-01-01", 0, "UTC", span).unwrap(),
            (253_402_300_800, 0)
        );
    }
}
