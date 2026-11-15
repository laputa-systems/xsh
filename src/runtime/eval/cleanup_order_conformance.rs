use super::DeferKind;
use super::lowered_run::indexed_run::cleanup_policy::{failure_is_recorded, next_due_defer};
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader};

const HEADER: &str = "cleanup-order-v1";
const ROWS: usize = 14762;

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
fn production_matches_lean_cleanup_order() {
    let path = std::env::var_os("XSH_CLEANUP_ORDER_CASES").expect("XSH_CLEANUP_ORDER_CASES is required");
    let file = std::fs::File::open(path).expect("cannot open cleanup order corpus");
    let mut lines = BufReader::new(file).lines();
    assert_eq!(lines.next().expect("missing header").expect("invalid header"), HEADER);
    let mut seen = BTreeSet::new();
    let mut count = 0;
    for line in lines {
        let line = line.expect("invalid corpus input");
        let fields: Vec<_> = line.split(' ').collect();
        assert_eq!(fields.len(), 5, "malformed row {}: {line}", count + 2);
        assert_eq!(fields[0], "S", "invalid row kind: {:?}", fields[0]);
        let entries: Vec<_> = if fields[1] == "-" { Vec::new() } else { fields[1].split(',').collect() };
        assert!(entries.len() <= 4, "out-of-domain input");
        let actions: Vec<_> = entries.iter().map(|entry| {
            match *entry {
                "AO" => (DeferKind::Always, None), "AF" => (DeferKind::Always, Some(false)), "AX" => (DeferKind::Always, Some(true)),
                "EO" => (DeferKind::OnError, None), "EF" => (DeferKind::OnError, Some(false)), "EX" => (DeferKind::OnError, Some(true)),
                "MO" => (DeferKind::ManagedRelease, None), "MF" => (DeferKind::ManagedRelease, Some(false)), "MX" => (DeferKind::ManagedRelease, Some(true)),
                _ => panic!("out-of-domain input: {entry:?}"),
            }
        }).collect();
        let mut flag = bit(fields[2]);
        let mut next = actions.len();
        let mut trace = Vec::new();
        while let Some(i) = next_due_defer(|index| actions[index].0, next, flag) {
            trace.push(i);
            next = i;
            if let Some(forced) = actions[i].1 { flag |= failure_is_recorded(actions[i].0, forced); }
        }
        assert_eq!((trace, flag), (indices(fields[3], actions.len()), bit(fields[4])), "model/source disagreement at row {}: {line}", count + 2);
        assert!(seen.insert((fields[1].to_owned(), fields[2].to_owned())), "duplicate input at row {}: {line}", count + 2);
        count += 1;
        assert!(count <= ROWS, "corpus exceeds bound");
    }
    assert_eq!(count, ROWS, "incomplete corpus");
    assert_eq!(seen.len(), ROWS, "incomplete input domain");
}
