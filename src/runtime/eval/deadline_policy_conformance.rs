use super::{choose_due_within_deadline, zero_span, WithinDeadline};
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader};
use std::time::{Duration, Instant};

const HEADER: &str = "deadline-policy-v1";
const ROWS: usize = 2925;

fn number(text: &str) -> usize {
    assert!(!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()), "invalid number: {text:?}");
    assert!(text == "0" || !text.starts_with('0'), "noncanonical number: {text}");
    text.parse().expect("number out of range")
}

#[test]
#[ignore = "manual Lean-generated production/model differential check"]
fn production_matches_lean_deadline_policy() {
    let path = std::env::var_os("XSH_DEADLINE_POLICY_CASES").expect("XSH_DEADLINE_POLICY_CASES is required");
    let file = std::fs::File::open(path).expect("cannot open deadline policy corpus");
    let mut lines = BufReader::new(file).lines();
    assert_eq!(lines.next().expect("missing header").expect("invalid header"), HEADER);
    let epoch = Instant::now();
    let mut seen = BTreeSet::new();
    let mut count = 0;
    for line in lines {
        let line = line.expect("invalid corpus input");
        let fields: Vec<_> = line.split(' ').collect();
        assert_eq!(fields.len(), 3, "malformed row {}: {line}", count + 2);
        let now = number(fields[0]);
        assert!(now <= 4, "out-of-domain now: {now}");
        let mut scopes = Vec::new();
        if fields[1] != "-" {
            for item in fields[1].split(',') {
                let (tick, flag) = item.split_once(':').expect("missing scope separator");
                let tick = number(tick);
                assert!(tick <= 3, "out-of-domain tick: {tick}");
                let delivered = match flag {
                    "0" => false,
                    "1" => true,
                    _ => panic!("invalid delivered flag: {flag:?}"),
                };
                scopes.push((tick, delivered));
                assert!(scopes.len() <= 3, "too many scopes");
            }
        }
        let expected = if fields[2] == "-" { None } else {
            let index = number(fields[2]);
            assert!(index < scopes.len(), "selected index out of range: {index}");
            Some(index)
        };
        assert!(seen.insert((now, scopes.clone())), "duplicate input at row {}: {line}", count + 2);
        let deadlines: Vec<_> = scopes.iter().enumerate().map(|(index, &(tick, delivered))| WithinDeadline {
            id: (index % 2) as u64,
            at: epoch + Duration::from_nanos(tick as u64),
            limit: Duration::ZERO,
            span: zero_span(),
            delivered,
        }).collect();
        assert_eq!(choose_due_within_deadline(&deadlines, epoch + Duration::from_nanos(now as u64)), expected,
            "model/source disagreement at row {}: {line}", count + 2);
        count += 1;
        assert!(count <= ROWS, "corpus exceeds bound");
    }
    assert_eq!(count, ROWS, "incomplete corpus");
    assert_eq!(seen.len(), ROWS, "incomplete input domain");
}
