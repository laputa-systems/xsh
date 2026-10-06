use super::{Calendar, clock_resolution, format, from_calendar, to_calendar};

#[test]
fn calendar_nanosecond_precision_and_negative_epoch() {
    assert_eq!(to_calendar(-1, true).unwrap(), Calendar {
        year: 1969, month: 12, day: 31, hour: 23, minute: 59, second: 59,
        weekday: 3, offset_seconds: 0, nanosecond: 999999999,
    });
    assert_eq!(format(-1, "%F %T.%N %s", true).unwrap(), "1969-12-31 23:59:59.999999999 -1");
    assert_eq!(format(123456789, "%3N %12N", true).unwrap(), "123 123456789000");
}

#[test]
fn calendar_utc_fields_and_leap_day_round_trip() {
    let epoch = from_calendar(2000, 2, 29, 12, 34, 56, true, false).unwrap();
    let fields = to_calendar(epoch, true).unwrap();
    assert_eq!((fields.year, fields.month, fields.day), (2000, 2, 29));
    assert_eq!((fields.hour, fields.minute, fields.second), (12, 34, 56));
    assert_eq!(fields.weekday, 2);
    assert_eq!(fields.offset_seconds, 0);
    assert_eq!(fields.nanosecond, 0);
    assert_eq!(format(0, "%q %z %:z %::z %:::z", true).unwrap(), "1 +0000 +00:00 +00:00:00 +00");
    assert!(clock_resolution().unwrap() > 0);
}

#[test]
fn calendar_validation_and_bounded_formats() {
    assert!(from_calendar(2023, 2, 29, 0, 0, 0, true, false).is_err());
    assert!(from_calendar(2500, 1, 1, 0, 0, 0, true, false).is_err());
    assert!(format(0, "%999999999999999999999Y", true).is_err());
    assert!(format(0, "%65536Y%65536Y", true).is_err());
}

#[test]
fn calendar_format_flags_apply_before_bounded_padding() {
    let epoch = from_calendar(1999, 6, 1, 0, 0, 0, true, false).unwrap();
    assert_eq!(format(epoch, "%10Y-%_5m-%-5d", true).unwrap(), "0000001999-    6-1");
    assert_eq!(format(epoch, "%+6Y %^#B %02j %0e", true).unwrap(), "+01999 JUNE 152 01");
    assert_eq!(format(epoch, "%300S", true).unwrap().len(), 300);
    assert_eq!(format(epoch, "%#z", true).unwrap(), "+0000");
}
