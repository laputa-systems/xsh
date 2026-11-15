use super::effects::{effect_covers, effect_predicate_implies, EffectContract, EffectPredicate, EffectSet};
use crate::syntax::node::Effect;
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader};

type Contract = EffectContract<u32>;

fn set(bits: usize) -> EffectSet {
    assert!(bits < 128, "out-of-domain input");
    EffectSet::from_bits(bits as u8).unwrap()
}

fn number(text: &str) -> usize {
    let n: usize = text.parse().expect("invalid number");
    assert_eq!(n.to_string(), text, "noncanonical number");
    n
}

fn contracts() -> Vec<Contract> {
    let p = Contract::variable(0);
    let q = Contract::variable(1);
    vec![Contract::Unknown, Contract::finite(set(0)), Contract::finite(set(1)),
        Contract::finite(set(32)), Contract::finite(set(64)), p.clone(), q.clone(),
        p.clone().capture_error(), p.clone().union(q),
        p.clone().union(Contract::finite(set(32))), p.masked(set(1))]
}

fn predicates(c: &[Contract]) -> Vec<EffectPredicate<u32>> {
    use EffectPredicate::*;
    vec![Equal(c[0].clone(), c[1].clone()), Equal(c[5].clone(), c[5].clone()),
        Included(c[5].clone(), c[4].clone()), Excluded(c[5].clone(), set(32)),
        Covered(c[5].clone(), c[1].clone()), Covered(c[5].clone(), c[2].clone()),
        Covered(c[5].clone(), c[3].clone()), Covered(c[5].clone(), c[4].clone()),
        Covered(c[7].clone(), c[1].clone()), Covered(c[7].clone(), c[2].clone()),
        Covered(c[7].clone(), c[4].clone()), Covered(c[6].clone(), c[3].clone()),
        Covered(c[5].clone(), c[5].clone()), Covered(c[0].clone(), c[3].clone()),
        Covered(c[9].clone(), c[3].clone()), Covered(c[10].clone(), c[2].clone())]
}

fn encode(contract: Contract) -> String {
    match contract {
        Contract::Unknown => "u".to_owned(),
        Contract::Known(expression) => {
            let mut masks = [0; 2];
            for (id, mask) in expression.variables() { masks[*id as usize] = mask.bits(); }
            format!("{}:{}:{}", expression.direct().bits(), masks[0], masks[1])
        }
    }
}

#[test]
#[ignore = "manual Lean-generated production/model differential check"]
fn production_matches_lean_effect_algebra() {
    let path = std::env::var_os("XSH_EFFECT_CASES").expect("XSH_EFFECT_CASES is required");
    let mut lines = BufReader::new(std::fs::File::open(path).expect("cannot open effect corpus")).lines();
    assert_eq!(lines.next().expect("missing header").unwrap(), "effect-algebra-v1");
    let contracts = contracts();
    let predicates = predicates(&contracts);
    let mut seen = BTreeSet::new();
    for line in lines {
        let line = line.expect("invalid corpus input");
        let fields: Vec<_> = line.split(' ').collect();
        assert!(fields.len() >= 4, "malformed row: {line}");
        let l = number(fields[1]);
        let r = number(fields[2]);
        match fields[0] {
            "set" => {
                assert_eq!(fields.len(), 6, "malformed set row");
                let left = set(l); let right = set(r);
                assert!(seen.insert((0, l, r)), "duplicate input");
                let union = left.union(right).bits().to_string();
                let included = EffectPredicate::<u32>::Included(Contract::finite(left), Contract::finite(right)).satisfaction() == Some(true);
                let covered = left.iter().all(|e| right.iter().any(|p: Effect| effect_covers(&p, &e)));
                assert_eq!((&*union, if included { "1" } else { "0" }, if covered { "1" } else { "0" }),
                    (fields[3], fields[4], fields[5]), "model/source disagreement: {line}");
            }
            "contract" => {
                assert_eq!(fields.len(), 4, "malformed contract row");
                assert!(l < contracts.len() && r < contracts.len(), "out-of-domain input");
                assert!(seen.insert((1, l, r)), "duplicate input");
                assert_eq!(encode(contracts[l].clone().union(contracts[r].clone())), fields[3], "model/source disagreement: {line}");
            }
            "implies" => {
                assert_eq!(fields.len(), 4, "malformed implication row");
                assert!(l < predicates.len() && r < predicates.len(), "out-of-domain input");
                assert!(seen.insert((2, l, r)), "duplicate input");
                assert_eq!(if effect_predicate_implies(&predicates[l], &predicates[r]) { "1" } else { "0" }, fields[3], "model/source disagreement: {line}");
            }
            _ => panic!("out-of-domain input"),
        }
    }
    assert_eq!(seen.len(), 128 * 128 + 11 * 11 + 16 * 16, "incomplete corpus");
    println!("effect rows: {}", seen.len());
}
