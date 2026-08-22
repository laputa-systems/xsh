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
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524
        - day_of_era / 146_096)
        / 365;
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
    use super::format_epoch_ms_utc;

    #[test]
    fn format_epoch_ms_utc_handles_epoch_leap_day_and_pre_epoch_values() {
        assert_eq!(format_epoch_ms_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_epoch_ms_utc(951_782_400_000), "2000-02-29T00:00:00Z");
        assert_eq!(format_epoch_ms_utc(-1), "1969-12-31T23:59:59Z");
    }
}
