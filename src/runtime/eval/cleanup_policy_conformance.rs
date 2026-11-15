use super::DeferKind;
use super::lowered_run::indexed_run::cleanup_policy::{cleanup_disposition, failure_is_recorded, CleanupFailures, Disposition};
use crate::runtime::value::RuntimeError;
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader};

const HEADER: &str = "cleanup-policy-v1";
const ROWS: usize = 349;

fn bit(text: &str) -> bool {
    match text { "0" => false, "1" => true, _ => panic!("invalid flag: {text:?}") }
}

fn number(text: &str) -> usize {
    assert!(!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()), "invalid number: {text:?}");
    assert!(text == "0" || !text.starts_with('0'), "noncanonical number: {text}");
    text.parse().expect("number out of range")
}

fn indices(text: &str, bound: usize) -> Vec<usize> {
    if text == "-" { return Vec::new(); }
    text.split(',').map(|s| { let i = number(s); assert!(i < bound, "selected index out of range: {i}"); i }).collect()
}

#[test]
#[ignore = "manual Lean-generated production/model differential check"]
fn production_matches_lean_cleanup_policy() {
    super::run_eval(|| {
        let path = std::env::var_os("XSH_CLEANUP_CASES").expect("XSH_CLEANUP_CASES is required");
        let file = std::fs::File::open(path).expect("cannot open cleanup policy corpus");
        let mut lines = BufReader::new(file).lines();
        assert_eq!(lines.next().expect("missing header").expect("invalid header"), HEADER);
        let mut seen = BTreeSet::new();
        let mut count = 0;
        for line in lines {
            let line = line.expect("invalid corpus input");
            let fields: Vec<_> = line.split(' ').collect();
            assert_eq!(fields.len(), 4, "malformed row {}: {line}", count + 2);
            let key = match fields[0] {
                "D" => {
                    let (force, primary) = (bit(fields[1]), bit(fields[2]));
                    let expected = match fields[3] { "R" => Disposition::Raise, "P" => Disposition::ReportSecondary, "S" => Disposition::Surface, _ => panic!("invalid disposition") };
                    assert_eq!(cleanup_disposition(force, primary), expected, "model/source disagreement at row {}: {line}", count + 2);
                    format!("D {} {}", fields[1], fields[2])
                }
                "F" => {
                    let (managed, force) = (bit(fields[1]), bit(fields[2]));
                    let kind = if managed { DeferKind::ManagedRelease } else { DeferKind::Always };
                    assert_eq!(failure_is_recorded(kind, force), bit(fields[3]), "model/source disagreement at row {}: {line}", count + 2);
                    format!("F {} {}", fields[1], fields[2])
                }
                "S" => {
                    let entries: Vec<_> = if fields[1] == "-" { Vec::new() } else { fields[1].split(',').collect() };
                    assert!(entries.len() <= 4, "too many errors");
                    let mut failures = CleanupFailures::default();
                    let mut reports = Vec::new();
                    for (i, entry) in entries.iter().enumerate() {
                        assert_eq!(entry.len(), 2, "invalid error flags: {entry:?}");
                        let force = bit(&entry[0..1]);
                        let managed = bit(&entry[1..2]);
                        let mut error = if force { RuntimeError::abort(143, true) } else { RuntimeError::new("plain", "plain") };
                        error.message = i.to_string();
                        let kind = if managed { DeferKind::ManagedRelease } else { DeferKind::Always };
                        let secondary = if failure_is_recorded(kind, force) { failures.record(error, None) } else { Some(error) };
                        if let Some(error) = secondary { reports.push(number(&error.message)); }
                    }
                    let selected: Vec<_> = failures.take().map(|(e, _)| number(&e.message)).into_iter().collect();
                    let expected = indices(fields[2], entries.len());
                    assert!(expected.len() <= 1, "too many selected errors");
                    assert_eq!((selected, reports), (expected, indices(fields[3], entries.len())), "model/source disagreement at row {}: {line}", count + 2);
                    format!("S {}", fields[1])
                }
                _ => panic!("invalid row kind: {:?}", fields[0]),
            };
            assert!(seen.insert(key), "duplicate input at row {}: {line}", count + 2);
            count += 1;
            assert!(count <= ROWS, "corpus exceeds bound");
        }
        assert_eq!(count, ROWS, "incomplete corpus");
        assert_eq!(seen.len(), ROWS, "incomplete input domain");
    });
}
