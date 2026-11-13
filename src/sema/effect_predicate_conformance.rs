use super::{Effect, EffectContract, EffectPredicate, EffectSet};
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader};

const HEADER: &str = "effect-predicate-v1 fs net process env time error io scope=17";
const ROWS: usize = 2 * 128 * 128 * 4 + 16 * 16 * 3 + 16 * 128;

type Contract = EffectContract<u32>;

fn number(text: &str) -> u32 {
    assert!(!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()), "invalid number: {text:?}");
    assert!(text == "0" || !text.starts_with('0'), "noncanonical number: {text}");
    text.parse().expect("number out of range")
}

fn effect_set(text: &str) -> EffectSet {
    let bits = number(text);
    assert!(bits < 128, "unsupported effect bits: {bits}");
    let mapping = [Effect::Fs, Effect::Net, Effect::Process, Effect::Env, Effect::Time, Effect::Error, Effect::Io];
    EffectSet::new(mapping.into_iter().enumerate().filter_map(|(index, effect)|
        (bits & (1 << index) != 0).then_some(effect)))
}

fn contract(text: &str) -> Contract {
    if text == "u" { return Contract::Unknown; }
    let (direct, parameters) = text.split_once(';').expect("missing contract separator");
    let mut result = Contract::finite(effect_set(direct));
    let mut seen = BTreeSet::new();
    if !parameters.is_empty() {
        for item in parameters.split(',') {
            let (id, mask) = item.split_once(':').expect("missing parameter separator");
            let id = number(id);
            assert!(id < 17 && seen.insert(id), "duplicate or out-of-range parameter: {id}");
            let mask = effect_set(mask);
            assert!(!mask.is_empty(), "zero coefficient");
            result = result.union(Contract::variable(id).masked(mask));
        }
    }
    result
}

#[test]
#[ignore = "manual Lean-generated production/model differential check"]
fn production_matches_lean_effect_predicates() {
    let path = std::env::var_os("XSH_EFFECT_PREDICATE_CASES").expect("XSH_EFFECT_PREDICATE_CASES is required");
    let file = std::fs::File::open(path).expect("cannot open effect predicate corpus");
    let mut lines = BufReader::new(file).lines();
    assert_eq!(lines.next().expect("missing header").expect("invalid header"), HEADER);
    let mut count = 0;
    let mut outcomes = [[false; 3]; 4];
    let mut closed_pairs = BTreeSet::new();
    for line in lines {
        let line = line.expect("invalid corpus input");
        let fields: Vec<_> = line.split(' ').collect();
        assert_eq!(fields.len(), 4, "malformed row {}: {line}", count + 2);
        let left = contract(fields[1]);
        let (op, predicate) = match fields[0] {
            "eq" => (0, EffectPredicate::Equal(left, contract(fields[2]))),
            "in" => (1, EffectPredicate::Included(left, contract(fields[2]))),
            "cov" => (2, EffectPredicate::Covered(left, contract(fields[2]))),
            "ex" => (3, EffectPredicate::Excluded(left, effect_set(fields[2]))),
            _ => panic!("unknown operation in row {}: {line}", count + 2),
        };
        if let Some(left) = fields[1].strip_suffix(';') {
            let right = if op == 3 { Some(fields[2]) } else { fields[2].strip_suffix(';') };
            if let Some(right) = right {
                closed_pairs.insert((op, number(left), number(right)));
            }
        }
        let (index, expected) = match fields[3] {
            "t" => (0, Some(true)),
            "f" => (1, Some(false)),
            "?" => (2, None),
            _ => panic!("invalid outcome in row {}: {line}", count + 2),
        };
        outcomes[op][index] = true;
        assert_eq!(predicate.satisfaction(), expected, "model/source disagreement at row {}: {line}", count + 2);
        count += 1;
        assert!(count <= ROWS, "corpus exceeds bound");
    }
    assert_eq!(count, ROWS, "incomplete corpus");
    assert_eq!(closed_pairs.len(), 128 * 128 * 4, "corpus omits a closed-mask comparison");
    assert_eq!(outcomes, [[true; 3]; 4], "corpus lacks an outcome");
}
