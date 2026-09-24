//! The retained embedded uptime parser handles host text through the catalog.

use crate::runtime::eval::Evaluator;
use crate::runtime::value::Value;

#[test]
fn uptime_reads_whole_seconds() {
    let fixture = include_str!("../../tests/fixtures/stdlib/uptime/spellings.txt");
    let rows = fixture
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'));
    let mut count = 0;
    for row in rows {
        let (uptime_text, expected) = row.split_once('\t').expect("tab-separated fixture");
        let expected = expected.parse::<i64>().expect("Int expectation");
        let value = Evaluator::probe_embedded_pure_call(
            "unix",
            "uptime_from_text",
            &[Value::Str(uptime_text.into())],
        )
        .expect("uptime_from_text runs");
        assert_eq!(value, Value::Int(expected), "text `{uptime_text}`");
        count += 1;
    }
    assert!(count >= 8, "the uptime table lost rows");
}
