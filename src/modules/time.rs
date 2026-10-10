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
    let resolution = rustix::time::clock_getres(rustix::time::ClockId::Realtime);
    resolution.tv_sec.checked_mul(NANOS_PER_SECOND)
        .and_then(|seconds| seconds.checked_add(i64::from(resolution.tv_nsec)))
        .ok_or_else(|| "clock resolution out of range".into())
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Calendar {
    pub(crate) year: i64,
    pub(crate) month: i64,
    pub(crate) day: i64,
    pub(crate) hour: i64,
    pub(crate) minute: i64,
    pub(crate) second: i64,
    pub(crate) weekday: i64,
    pub(crate) offset_seconds: i64,
    pub(crate) nanosecond: i64,
}

pub(crate) fn to_calendar(epoch_ns: i64, utc: bool) -> Result<Calendar, String> {
    let tm = calendar(epoch_ns.div_euclid(NANOS_PER_SECOND), utc)?;
    Ok(Calendar {
        year: tm.tm_year as i64 + 1900,
        month: tm.tm_mon as i64 + 1,
        day: tm.tm_mday as i64,
        hour: tm.tm_hour as i64,
        minute: tm.tm_min as i64,
        second: tm.tm_sec as i64,
        weekday: tm.tm_wday as i64,
        offset_seconds: tm.tm_gmtoff as i64,
        nanosecond: epoch_ns.rem_euclid(NANOS_PER_SECOND),
    })
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

// Strict conversion rejects impossible civil dates and missing DST hours.
// Explicit normalization delegates gap resolution to the host calendar library
// after a script has performed its own civil arithmetic.
pub(crate) fn from_calendar(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64, utc: bool, normalize: bool) -> Result<i64, String> {
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
    let valid = if normalize {
        back.tm_year == tm.tm_year && back.tm_mon == tm.tm_mon && back.tm_mday == tm.tm_mday
            && back.tm_hour == tm.tm_hour && back.tm_min == tm.tm_min && back.tm_sec == tm.tm_sec
    } else {
        back.tm_year as i64 + 1900 == year && back.tm_mon as i64 + 1 == month
            && back.tm_mday as i64 == day && back.tm_hour as i64 == hour
            && back.tm_min as i64 == minute && back.tm_sec as i64 == second
    };
    if !valid { return Err("invalid calendar date".into()); }
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
            if "_-0^#+".contains(c) { flags.push(c); chars.next(); } else { break; }
        }
        let mut width = String::new();
        while let Some(&(_, c)) = chars.peek() {
            if c.is_ascii_digit() { width.push(c); chars.next(); } else { break; }
        }
        let requested = if width.is_empty() { None } else { Some(width.parse::<usize>().map_err(|_| "date format width too large")?) };
        if requested.is_some_and(|w| w > FORMAT_LIMIT) { return Err(std::format!("format modifier width '{width}' is too large for specifier '%{}'", chars.peek().map(|&(_, c)| c).unwrap_or('?'))); }
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
            _ if !"aAbBcCdDeFgGhHIjklmMnpPrRStTuUVwWxXyYzZ%+".contains(spec) => format[start..end + spec.len_utf8()].into(),
            ('P', 0) => strftime_piece(&tm, "%p")?.to_lowercase(),
            ('k', 0) => std::format!("{:2}", tm.tm_hour),
            ('l', 0) => std::format!("{:2}", (tm.tm_hour + 11) % 12 + 1),
            _ => strftime_piece(&tm, &std::format!("%{spec}"))?,
        };
        if flags.contains('^') { piece = piece.to_uppercase(); }
        else if flags.contains('#') {
            piece = if matches!(spec, 'p' | 'P' | 'Z') { piece.to_lowercase() } else { piece.to_uppercase() };
        }
        if spec != 'N' && colons == 0 {
            let numeric = "CdeGgHIkjlmMSuUVwWyYs q".contains(spec);
            let padding = flags.chars().rev().find(|c| "_-0+".contains(*c));
            let default_width = piece.len();
            if numeric && (padding.is_some() || requested.is_some()) {
                let negative = piece.starts_with('-');
                let digits = piece.trim_start_matches([' ', '0', '-']);
                piece = if digits.is_empty() { "0".into() } else { digits.into() };
                if negative { piece.insert(0, '-'); }
            }
            if padding != Some('-') {
                let n = requested.unwrap_or(default_width);
                let force_sign = padding == Some('+') && matches!(spec, 'Y' | 'G') && requested.is_some_and(|w| w > 4) && !piece.starts_with('-');
                if force_sign { piece.insert(0, '+'); }
                if piece.len() < n {
                    let pad = if padding == Some('_') || (padding.is_none() && (matches!(spec, 'e' | 'k' | 'l') || !numeric)) { ' ' } else { '0' };
                    let fill = pad.to_string().repeat(n - piece.len());
                    if pad == '0' && (piece.starts_with('+') || piece.starts_with('-')) { piece.insert_str(1, &fill); }
                    else { piece.insert_str(0, &fill); }
                }
            }
        }
        if out.len().saturating_add(piece.len()) > FORMAT_LIMIT { return Err("formatted date too large".into()); }
        out.push_str(&piece);
    }
    Ok(out)
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
