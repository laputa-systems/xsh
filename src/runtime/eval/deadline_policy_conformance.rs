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

#[test]
fn selection_and_scope_transitions_preserve_deadlines() {
    let epoch = Instant::now();
    let mut evaluator = super::Evaluator::new(Vec::new());
    evaluator.within_deadlines = [(10, 5, false), (20, 3, true), (30, 3, false), (40, 8, false)]
        .into_iter().map(|(id, tick, delivered)| WithinDeadline {
            id, at: epoch + Duration::from_secs(tick), limit: Duration::from_secs(id),
            span: zero_span(), delivered,
        }).collect();
    let snapshot = |deadlines: &[WithinDeadline]| deadlines.iter()
        .map(|d| (d.id, d.at, d.limit, d.span, d.delivered)).collect::<Vec<_>>();
    let original = snapshot(&evaluator.within_deadlines);
    let select = |deadlines: &[WithinDeadline], tick| {
        choose_due_within_deadline(deadlines, epoch + Duration::from_secs(tick))
            .map(|i| deadlines[i].id)
    };
    assert_eq!(select(&evaluator.within_deadlines, 2), None);
    assert_eq!(select(&evaluator.within_deadlines, 5), Some(30));
    evaluator.rearm_swallowed_interruptions();
    let rearmed = snapshot(&evaluator.within_deadlines);
    assert_eq!(rearmed, original.iter().map(|&(id, at, limit, span, _)|
        (id, at, limit, span, false)).collect::<Vec<_>>());
    assert_eq!(select(&evaluator.within_deadlines, 5), Some(20));
    let saved = evaluator.shield_within_deadlines();
    assert_eq!(snapshot(&saved), rearmed);
    assert_eq!(select(&evaluator.within_deadlines, 9), None);
    evaluator.within_deadlines.push(WithinDeadline {
        id: 50, at: epoch, limit: Duration::ZERO, span: zero_span(), delivered: false,
    });
    assert_eq!(select(&evaluator.within_deadlines, 9), Some(50));
    evaluator.unshield_within_deadlines(saved);
    assert_eq!(snapshot(&evaluator.within_deadlines), rearmed);
    assert_eq!(select(&evaluator.within_deadlines, 5), Some(20));
    evaluator.close_within_deadline(20);
    assert_eq!(snapshot(&evaluator.within_deadlines), rearmed.into_iter()
        .filter(|(id, _, _, _, _)| *id != 20).collect::<Vec<_>>());
    assert_eq!(select(&evaluator.within_deadlines, 5), Some(30));
}
