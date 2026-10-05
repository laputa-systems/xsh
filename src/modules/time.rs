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

const NANOS_PER_SECOND: i64 = 1_000_000_000;
const FORMAT_LIMIT: usize = 65_536;

pub(crate) fn clock_resolution() -> Result<i64, String> {
    let mut resolution: libc::timespec = unsafe { std::mem::zeroed() };
    if unsafe { libc::clock_getres(libc::CLOCK_REALTIME, &mut resolution) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    (resolution.tv_sec as i64).checked_mul(NANOS_PER_SECOND)
        .and_then(|seconds| seconds.checked_add(resolution.tv_nsec as i64))
        .ok_or_else(|| "clock resolution out of range".into())
}

fn calendar(seconds: i64, utc: bool) -> Result<libc::tm, String> {
    let seconds = seconds.try_into().map_err(|_| "timestamp out of range")?;
    let mut tm = unsafe { std::mem::zeroed() };
    let result = unsafe {
        if utc { libc::gmtime_r(&seconds, &mut tm) }
        else { libc::localtime_r(&seconds, &mut tm) }
    };
    if result.is_null() { Err("timestamp out of range".into()) } else { Ok(tm) }
}

// Calendar conversion rejects normalization: February 30 and a missing local
// hour during a DST jump are errors rather than silently becoming another date.
pub(crate) fn from_calendar(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64, utc: bool) -> Result<i64, String> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute) || !(0..=59).contains(&second) {
        return Err("invalid calendar date".into());
    }
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_year = year.checked_sub(1900).and_then(|v| v.try_into().ok()).ok_or("year out of range")?;
    tm.tm_mon = (month - 1) as i32;
    tm.tm_mday = day as i32;
    tm.tm_hour = hour as i32;
    tm.tm_min = minute as i32;
    tm.tm_sec = second as i32;
    tm.tm_isdst = -1;
    let seconds = unsafe { if utc { libc::timegm(&mut tm) } else { libc::mktime(&mut tm) } } as i64;
    let back = calendar(seconds, utc)?;
    if back.tm_year as i64 + 1900 != year || back.tm_mon as i64 + 1 != month
        || back.tm_mday as i64 != day || back.tm_hour as i64 != hour
        || back.tm_min as i64 != minute || back.tm_sec as i64 != second {
        return Err("invalid calendar date".into());
    }
    seconds.checked_mul(NANOS_PER_SECOND).ok_or_else(|| "timestamp out of nanosecond range".into())
}

fn strftime_piece(tm: &libc::tm, directive: &str) -> Result<String, String> {
    let fmt = std::ffi::CString::new(directive).map_err(|_| "format contains NUL")?;
    let mut buffer = vec![0u8; FORMAT_LIMIT + 1];
    let count = unsafe { libc::strftime(buffer.as_mut_ptr().cast(), buffer.len(), fmt.as_ptr(), tm) };
    if count == 0 {
        // A zone name can legitimately be empty; other empty results indicate
        // that the bounded strftime destination could not hold the output.
        if directive.ends_with('Z') { return Ok(String::new()); }
        return Err("date directive could not be formatted".into());
    }
    String::from_utf8(buffer[..count].to_vec()).map_err(|_| "formatted date is not UTF-8".into())
}

// Bound both individual widths and total output before calling libc, since a
// format comes from untrusted script input and strftime accepts field widths.
pub(crate) fn format(epoch_ns: i64, format: &str, utc: bool) -> Result<String, String> {
    if format.len() > FORMAT_LIMIT || format.contains('\0') { return Err("date format too large or contains NUL".into()); }
    let seconds = epoch_ns.div_euclid(NANOS_PER_SECOND);
    let tm = calendar(seconds, utc)?;
    let mut out = String::new();
    let mut chars = format.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        if ch != '%' { out.push(ch); continue; }
        let mut flags = String::new();
        while let Some(&(_, c)) = chars.peek() {
            if "_-0^#".contains(c) { flags.push(c); chars.next(); } else { break; }
        }
        let mut width = String::new();
        while let Some(&(_, c)) = chars.peek() {
            if c.is_ascii_digit() { width.push(c); chars.next(); } else { break; }
        }
        let requested = if width.is_empty() { None } else { Some(width.parse::<usize>().map_err(|_| "date format width too large")?) };
        if requested.is_some_and(|w| w > FORMAT_LIMIT) { return Err("date format width too large".into()); }
        let mut colons = 0;
        while chars.peek().is_some_and(|&(_, c)| c == ':') { chars.next(); colons += 1; }
        let Some((mut end, mut spec)) = chars.next() else { out.push_str(&format[start..]); break; };
        if spec == 'E' || spec == 'O' {
            if let Some((i, c)) = chars.next() { end = i; spec = c; }
        }
        let mut piece = match (spec, colons) {
            ('N', 0) => {
                let value = std::format!("{:09}", epoch_ns.rem_euclid(NANOS_PER_SECOND));
                let n = requested.unwrap_or(9);
                if n <= 9 { value[..n].into() } else { std::format!("{value}{}", "0".repeat(n - 9)) }
            }
            ('s', 0) => seconds.to_string(),
            ('q', 0) => (tm.tm_mon / 3 + 1).to_string(),
            ('z', 1..=3) => {
                let offset = tm.tm_gmtoff as i64;
                let sign = if offset < 0 { '-' } else { '+' };
                let n = offset.abs();
                if colons == 1 { std::format!("{sign}{:02}:{:02}", n / 3600, n / 60 % 60) }
                else if colons == 2 { std::format!("{sign}{:02}:{:02}:{:02}", n / 3600, n / 60 % 60, n % 60) }
                else if n % 60 != 0 { std::format!("{sign}{:02}:{:02}:{:02}", n / 3600, n / 60 % 60, n % 60) }
                else if n % 3600 != 0 { std::format!("{sign}{:02}:{:02}", n / 3600, n / 60 % 60) }
                else { std::format!("{sign}{:02}", n / 3600) }
            }
            (_, 1..) => format[start..end + spec.len_utf8()].into(),
            _ if !"aAbBcCdDeFgGhHIjklmMnprRStTuUVwWxXyYzZ%+".contains(spec) => format[start..end + spec.len_utf8()].into(),
            _ => strftime_piece(&tm, &format[start..end + spec.len_utf8()])?,
        };
        if flags.contains('^') { piece = piece.to_uppercase(); }
        if spec != 'N' && matches!(spec, 's' | 'q') && !flags.contains('-') {
            if let Some(n) = requested { if piece.len() < n { piece = std::format!("{}{piece}", if flags.contains('_') { " " } else { "0" }.repeat(n - piece.len())); } }
        }
        if out.len().saturating_add(piece.len()) > FORMAT_LIMIT { return Err("formatted date too large".into()); }
        out.push_str(&piece);
    }
    Ok(out)
}

fn parse_epoch(text: &str) -> Result<i64, String> {
    let (negative, value) = if let Some(rest) = text.strip_prefix('-') { (true, rest) } else { (false, text.strip_prefix('+').unwrap_or(text)) };
    let mut parts = value.split('.');
    let digits = parts.next().unwrap_or("");
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) { return Err("invalid epoch timestamp".into()); }
    let whole = digits.parse::<i128>().map_err(|_| "invalid epoch timestamp")?;
    let fraction = parts.next().unwrap_or("");
    if parts.next().is_some() || fraction.len() > 9 || !fraction.bytes().all(|b| b.is_ascii_digit()) { return Err("invalid epoch timestamp".into()); }
    let sub = if fraction.is_empty() { 0 } else { fraction.parse::<i128>().map_err(|_| "invalid epoch timestamp")? * 10i128.pow(9 - fraction.len() as u32) };
    if whole < 0 { return Err("invalid epoch timestamp".into()); }
    let ns = whole.checked_mul(NANOS_PER_SECOND as i128).and_then(|v| v.checked_add(sub)).ok_or("timestamp out of nanosecond range")?;
    i64::try_from(if negative { -ns } else { ns }).map_err(|_| "timestamp out of nanosecond range".into())
}

pub(crate) fn parse(text: &str, utc: bool, base_ns: Option<i64>) -> Result<i64, String> {
    let text = text.trim();
    if let Some(value) = text.strip_prefix('@') { return parse_epoch(value); }
    let now = match base_ns { Some(value) => value, None => now_epoch_ms().checked_mul(1_000_000).ok_or("timestamp out of range")? };
    let lower = text.to_ascii_lowercase();
    match lower.as_str() {
        "now" => return Ok(now),
        "" | "today" | "yesterday" | "tomorrow" => {
            let tm = calendar(now / NANOS_PER_SECOND, utc)?;
            let midnight = from_calendar(tm.tm_year as i64 + 1900, tm.tm_mon as i64 + 1, tm.tm_mday as i64, 0, 0, 0, utc)?;
            let delta = if lower == "yesterday" { -86_400_000_000_000 } else if lower == "tomorrow" { 86_400_000_000_000 } else { 0 };
            return midnight.checked_add(delta).ok_or_else(|| "timestamp out of range".into());
        }
        _ => {}
    }
    // Peel relative units from the end so an absolute calendar prefix and
    // multiple relative adjustments share exactly one baseline observation.
    let words: Vec<&str> = text.split_whitespace().collect();
    if !words.is_empty() {
        let ago = words.last().is_some_and(|w| w.eq_ignore_ascii_case("ago"));
        let end = words.len() - usize::from(ago);
        if end > 0 {
            let unit = words[end - 1].to_ascii_lowercase();
            let unit = unit.strip_suffix('s').unwrap_or(&unit);
            let scale: Option<i64> = match unit {
                "sec" | "second" => Some(1), "min" | "minute" => Some(60),
                "hour" => Some(3600), "day" => Some(86400), "week" => Some(604800),
                "fortnight" => Some(1209600), _ => None,
            };
            if scale.is_some() || unit == "month" || unit == "year" {
                let (count, prefix_end) = if end >= 2 {
                    if let Ok(n) = words[end - 2].parse::<i64>() { (n, end - 2) }
                    else if words[end - 2].eq_ignore_ascii_case("next") { (1, end - 2) }
                    else if words[end - 2].eq_ignore_ascii_case("last") { (-1, end - 2) }
                    else if words[end - 2].eq_ignore_ascii_case("this") { (0, end - 2) }
                    else { (1, end - 1) }
                } else { (1, 0) };
                let count = count.checked_mul(if ago { -1 } else { 1 }).ok_or("relative date overflow")?;
                let prefix = words[..prefix_end].join(" ");
                let prefix_relative = words.get(prefix_end.saturating_sub(1)).is_some_and(|word| {
                    matches!(word.trim_end_matches('s').to_ascii_lowercase().as_str(), "second" | "sec" | "minute" | "min" | "hour" | "day" | "week" | "fortnight" | "month" | "year")
                });
                let base = if prefix.is_empty() { now }
                    else if ago && prefix_relative { parse(&std::format!("{prefix} ago"), utc, Some(now))? }
                    else { parse(&prefix, utc, Some(now))? };
                if let Some(scale) = scale.filter(|_| !matches!(unit, "day" | "week" | "fortnight")) {
                    return count.checked_mul(scale).and_then(|v| v.checked_mul(NANOS_PER_SECOND)).and_then(|v| base.checked_add(v)).ok_or_else(|| "relative date overflow".into());
                }
                let mut tm = calendar(base.div_euclid(NANOS_PER_SECOND), utc)?;
                let delta: i32 = count.try_into().map_err(|_| "relative date overflow")?;
                if unit == "month" { tm.tm_mon = tm.tm_mon.checked_add(delta).ok_or("relative date overflow")?; }
                else if unit == "year" { tm.tm_year = tm.tm_year.checked_add(delta).ok_or("relative date overflow")?; }
                else {
                    let days = delta.checked_mul(if unit == "week" { 7 } else if unit == "fortnight" { 14 } else { 1 }).ok_or("relative date overflow")?;
                    tm.tm_mday = tm.tm_mday.checked_add(days).ok_or("relative date overflow")?;
                }
                tm.tm_isdst = -1;
                let seconds = unsafe { if utc { libc::timegm(&mut tm) } else { libc::mktime(&mut tm) } } as i64;
                return seconds.checked_mul(NANOS_PER_SECOND).and_then(|v| v.checked_add(base.rem_euclid(NANOS_PER_SECOND))).ok_or_else(|| "relative date overflow".into());
            }
        }
    }
    let mut input = text.to_owned();
    let mut explicit_offset = None;
    for (suffix, offset) in [(" UTC", 0), (" GMT", 0), ("Z", 0), (" EST", -18000), (" EDT", -14400), (" CST", -21600), (" CDT", -18000), (" MST", -25200), (" MDT", -21600), (" PST", -28800), (" PDT", -25200)] {
        if let Some(rest) = input.strip_suffix(suffix) { input = rest.trim_end().into(); explicit_offset = Some(offset); break; }
    }
    if explicit_offset.is_none() {
        let last = input.split_whitespace().last().unwrap_or("");
        if last.len() == 1 {
            let code = last.as_bytes()[0].to_ascii_uppercase();
            let hours = match code {
                b'A'..=b'I' => Some((code - b'A' + 1) as i64),
                b'K'..=b'M' => Some((code - b'K' + 10) as i64),
                b'N'..=b'Y' => Some(-((code - b'N' + 1) as i64)),
                b'Z' => Some(0), _ => None,
            };
            if let Some(hours) = hours {
                explicit_offset = Some(hours * 3600);
                input.truncate(input.len() - last.len());
                input = input.trim_end().into();
            }
        }
    }
    if explicit_offset.is_none() {
        if let Some(pos) = input.char_indices().rev().find_map(|(i,c)| ((c == '+' || c == '-') && i > 9).then_some(i)) {
            let tail = &input[pos + 1..];
            let digits = tail.replace(':', "");
            if matches!(digits.len(), 2 | 4 | 6) && digits.bytes().all(|b| b.is_ascii_digit()) {
                let h = digits[..2].parse::<i64>().unwrap();
                let m = if digits.len() >= 4 { digits[2..4].parse::<i64>().unwrap() } else { 0 };
                let s = if digits.len() == 6 { digits[4..].parse::<i64>().unwrap() } else { 0 };
                if h > 23 || m > 59 || s > 59 { return Err("invalid timezone offset".into()); }
                explicit_offset = Some((h*3600+m*60+s) * if input.as_bytes()[pos] == b'-' { -1 } else { 1 });
                input.truncate(pos); input = input.trim_end().into();
            }
        }
    }
    if input.is_empty() && explicit_offset.is_some() {
        let tm = calendar(now.div_euclid(NANOS_PER_SECOND), true)?;
        return from_calendar(tm.tm_year as i64 + 1900, tm.tm_mon as i64 + 1, tm.tm_mday as i64, 0, 0, 0, true)?
            .checked_sub(explicit_offset.unwrap() * NANOS_PER_SECOND).ok_or_else(|| "timestamp out of range".into());
    }
    let mut nanos = 0;
    if let Some(dot) = input.rfind('.') {
        let digits = &input[dot + 1..];
        if input[..dot].contains(':') && !digits.is_empty() && digits.len() <= 9 && digits.bytes().all(|b| b.is_ascii_digit()) {
            nanos = digits.parse::<i64>().unwrap() * 10i64.pow(9 - digits.len() as u32);
            input.truncate(dot);
        }
    }
    let text_c = std::ffi::CString::new(input.as_str()).map_err(|_| "date contains NUL")?;
    let current = calendar(now / NANOS_PER_SECOND, utc)?;
    let formats = ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M", "%Y-%m-%d", "%Y%m%d", "%Y/%m/%d", "%m/%d/%Y", "%d %b %Y %H:%M:%S", "%d %b %Y", "%b %d %Y %H:%M:%S", "%b %d %Y", "%a %b %d %H:%M:%S %Y", "%a %b %d %H:%M %Y", "%a, %d %b %Y %H:%M:%S", "%H:%M:%S", "%H:%M", "%Y%m%d%H%M.%S", "%Y%m%d%H%M", "%y%m%d%H%M.%S", "%y%m%d%H%M", "%m%d%H%M.%S", "%m%d%H%M"];
    for pattern in formats {
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        tm.tm_year = current.tm_year; tm.tm_mon = current.tm_mon; tm.tm_mday = current.tm_mday;
        let pattern = std::ffi::CString::new(pattern).unwrap();
        let end = unsafe { libc::strptime(text_c.as_ptr(), pattern.as_ptr(), &mut tm) };
        if end.is_null() || unsafe { *end } != 0 { continue; }
        if let Ok(ns) = from_calendar(tm.tm_year as i64 + 1900, tm.tm_mon as i64 + 1, tm.tm_mday as i64, tm.tm_hour as i64, tm.tm_min as i64, tm.tm_sec as i64, utc || explicit_offset.is_some()) {
            return ns.checked_sub(explicit_offset.unwrap_or(0) * NANOS_PER_SECOND).and_then(|v| v.checked_add(nanos)).ok_or_else(|| "timestamp out of nanosecond range".into());
        }
    }
    Err(std::format!("invalid date: {text}"))
}

#[cfg(test)]
mod tests {
    use super::format_epoch_ms_utc;

    #[test]
    fn format_epoch_ms_utc_handles_epoch_leap_day_and_pre_epoch_values() {
        assert_eq!(format_epoch_ms_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_epoch_ms_utc(951_782_400_000), "2000-02-29T00:00:00Z");
        assert_eq!(format_epoch_ms_utc(-1), "1969-12-31T23:59:59Z");
    }
}

#[cfg(test)]
#[path = "time_calendar_tests.rs"]
mod calendar_tests;
