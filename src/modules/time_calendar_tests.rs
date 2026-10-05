use super::{format, from_calendar, parse};

#[test]
fn calendar_nanosecond_precision_and_negative_epoch() {
    assert_eq!(parse("@-0.000000001", true, None).unwrap(), -1);
    assert_eq!(parse("1969-12-31T23:59:59.999999999Z", true, None).unwrap(), -1);
    assert_eq!(format(-1, "%F %T.%N %s", true).unwrap(), "1969-12-31 23:59:59.999999999 -1");
    assert_eq!(format(123456789, "%3N %12N", true).unwrap(), "123 123456789000");
}

#[test]
fn calendar_timezone_offsets_and_relative_baseline() {
    assert_eq!(parse("1970-01-01 01:30:00+01:30", false, None).unwrap(), 0);
    assert_eq!(parse("200002290000.05", true, None).unwrap(), 951782405000000000);
    assert_eq!(parse("+5 days", true, Some(0)).unwrap(), 432000000000000);
    assert_eq!(parse("2 hours 3 minutes ago", true, Some(0)).unwrap(), -7380000000000);
    assert_eq!(format(0, "%q %z %:z %::z %:::z", true).unwrap(), "1 +0000 +00:00 +00:00:00 +00");
}

#[test]
fn calendar_validation_and_bounded_formats() {
    assert!(from_calendar(2023, 2, 29, 0, 0, 0, true).is_err());
    assert!(from_calendar(2500, 1, 1, 0, 0, 0, true).is_err());
    assert!(parse("2024-02-30", true, None).is_err());
    assert!(parse("@999999999999999999999999999999999999999", true, None).is_err());
    assert!(format(0, "%999999999999999999999Y", true).is_err());
    assert!(format(0, "%65536Y%65536Y", true).is_err());
}

#[test]
fn calendar_relative_weekdays_and_clock_only_inputs() {
    assert_eq!(parse("last thu", true, Some(0)).unwrap(), -604800000000000);
    assert_eq!(parse("this thu", true, Some(0)).unwrap(), 0);
    assert_eq!(parse("next thu", true, Some(0)).unwrap(), 604800000000000);
    assert_eq!(parse("0700", true, Some(0)).unwrap(), 25200000000000);
    assert_eq!(parse("1230j", true, Some(0)).unwrap(), 45000000000000);
    assert_eq!(parse("m9", true, Some(0)).unwrap(), -10800000000000);
    assert_eq!(parse("yesterday 10:00 GMT", true, Some(0)).unwrap(), -50400000000000);
}

#[test]
fn calendar_format_flags_apply_before_bounded_padding() {
    let epoch = parse("1999-06-01", true, None).unwrap();
    assert_eq!(format(epoch, "%10Y-%_5m-%-5d", true).unwrap(), "0000001999-    6-1");
    assert_eq!(format(epoch, "%+6Y %^#B %02j %0e", true).unwrap(), "+01999 JUNE 152 01");
    assert_eq!(format(epoch, "%300S", true).unwrap().len(), 300);
    assert_eq!(format(epoch, "%#z", true).unwrap(), "+0000");
    assert_eq!(parse("2000-02-29 + 2 years", true, None).unwrap(), parse("2002-03-01", true, None).unwrap());
}
