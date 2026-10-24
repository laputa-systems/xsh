use std::collections::{BTreeMap, BTreeSet};

const EFFECT_A: u8 = 1;
const EFFECT_B: u8 = 2;
const ERROR: u8 = 4;
const ASSIGNMENT_COUNT: usize = 8;
const MAX_VARIABLES: usize = 3;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct EffectSet(u8);

impl EffectSet {
    const EMPTY: Self = Self(0);

    fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    fn without_error(self) -> Self {
        Self(self.0 & !ERROR)
    }

    fn is_subset(self, other: Self) -> bool {
        self.0 & !other.0 == 0
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Variable {
    Bound(u32),
    Free(u64),
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Term {
    Set(EffectSet),
    Variable(Variable),
    Union(Vec<Self>),
    WithoutError(Box<Self>),
}

impl Term {
    fn evaluate(&self, assignment: &Assignment) -> EffectSet {
        match self {
            Self::Set(effect_set) => *effect_set,
            Self::Variable(variable) => assignment[variable],
            Self::Union(terms) => terms
                .iter()
                .map(|term| term.evaluate(assignment))
                .fold(EffectSet::EMPTY, EffectSet::union),
            Self::WithoutError(term) => term.evaluate(assignment).without_error(),
        }
    }

    fn variables(&self, variables: &mut BTreeSet<Variable>) {
        match self {
            Self::Set(_) => {}
            Self::Variable(variable) => {
                variables.insert(*variable);
            }
            Self::Union(terms) => {
                for term in terms {
                    term.variables(variables);
                }
            }
            Self::WithoutError(term) => term.variables(variables),
        }
    }

    fn substitute_free(&self, variable: u64, replacement: &FreeTerm) -> Self {
        match self {
            Self::Set(effect_set) => Self::Set(*effect_set),
            Self::Variable(Variable::Bound(bound)) => Self::Variable(Variable::Bound(*bound)),
            Self::Variable(Variable::Free(free)) if *free == variable => replacement.to_term(),
            Self::Variable(Variable::Free(free)) => Self::Variable(Variable::Free(*free)),
            Self::Union(terms) => Self::Union(
                terms
                    .iter()
                    .map(|term| term.substitute_free(variable, replacement))
                    .collect(),
            ),
            Self::WithoutError(term) => Self::WithoutError(Box::new(
                term.substitute_free(variable, replacement),
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum FreeTerm {
    Set(EffectSet),
    Variable(u64),
    Union(Vec<Self>),
    WithoutError(Box<Self>),
}

impl FreeTerm {
    fn to_term(&self) -> Term {
        match self {
            Self::Set(effect_set) => Term::Set(*effect_set),
            Self::Variable(variable) => Term::Variable(Variable::Free(*variable)),
            Self::Union(terms) => Term::Union(terms.iter().map(Self::to_term).collect()),
            Self::WithoutError(term) => Term::WithoutError(Box::new(term.to_term())),
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Constraint {
    Included { required: Term, allowed: Term },
    Equal { left: Term, right: Term },
}

impl Constraint {
    fn satisfied_by(&self, assignment: &Assignment) -> bool {
        match self {
            Self::Included { required, allowed } => {
                required.evaluate(assignment).is_subset(allowed.evaluate(assignment))
            }
            Self::Equal { left, right } => left.evaluate(assignment) == right.evaluate(assignment),
        }
    }

    fn variables(&self, variables: &mut BTreeSet<Variable>) {
        match self {
            Self::Included { required, allowed } => {
                required.variables(variables);
                allowed.variables(variables);
            }
            Self::Equal { left, right } => {
                left.variables(variables);
                right.variables(variables);
            }
        }
    }

    fn substitute_free(&self, variable: u64, replacement: &FreeTerm) -> Self {
        match self {
            Self::Included { required, allowed } => Self::Included {
                required: required.substitute_free(variable, replacement),
                allowed: allowed.substitute_free(variable, replacement),
            },
            Self::Equal { left, right } => Self::Equal {
                left: left.substitute_free(variable, replacement),
                right: right.substitute_free(variable, replacement),
            },
        }
    }
}

type Assignment = BTreeMap<Variable, EffectSet>;

fn assignments(variables: &[Variable]) -> Vec<Assignment> {
    assert!(variables.len() <= MAX_VARIABLES);
    let count = ASSIGNMENT_COUNT.pow(variables.len() as u32);
    (0..count)
        .map(|index| {
            variables
                .iter()
                .enumerate()
                .map(|(position, variable)| {
                    let shift = ASSIGNMENT_COUNT.pow(position as u32);
                    (*variable, EffectSet(((index / shift) % ASSIGNMENT_COUNT) as u8))
                })
                .collect()
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EffectValue {
    Known(EffectSet),
    Unknown,
}

impl EffectValue {
    fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::Known(left), Self::Known(right)) => Self::Known(left.union(right)),
            _ => Self::Unknown,
        }
    }

    fn without_error(self) -> Self {
        match self {
            Self::Known(effect_set) => Self::Known(effect_set.without_error()),
            Self::Unknown => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Contract {
    Known(Term),
    Unknown,
}

impl Contract {
    fn evaluate(&self, assignment: &Assignment) -> EffectValue {
        match self {
            Self::Known(term) => EffectValue::Known(term.evaluate(assignment)),
            Self::Unknown => EffectValue::Unknown,
        }
    }

    fn variables(&self, variables: &mut BTreeSet<Variable>) {
        if let Self::Known(term) = self {
            term.variables(variables);
        }
    }

    fn substitute_free(&self, variable: u64, replacement: &FreeTerm) -> Self {
        match self {
            Self::Known(term) => {
                Self::Known(term.substitute_free(variable, replacement))
            }
            Self::Unknown => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EffectScheme {
    binders: Vec<u32>,
    constraints: Vec<Constraint>,
    contract: Contract,
}

impl EffectScheme {
    fn substitute_free(&self, variable: u64, replacement: &FreeTerm) -> Self {
        Self {
            binders: self.binders.clone(),
            constraints: self
                .constraints
                .iter()
                .map(|constraint| constraint.substitute_free(variable, replacement))
                .collect(),
            contract: self.contract.substitute_free(variable, replacement),
        }
    }

    fn variables(&self) -> BTreeSet<Variable> {
        let mut variables = self
            .binders
            .iter()
            .map(|&binder| Variable::Bound(binder))
            .collect::<BTreeSet<_>>();
        for constraint in &self.constraints {
            constraint.variables(&mut variables);
        }
        self.contract.variables(&mut variables);
        variables
    }

    fn free_variables(&self) -> BTreeSet<u64> {
        self.variables()
            .into_iter()
            .filter_map(|variable| match variable {
                Variable::Bound(_) => None,
                Variable::Free(free) => Some(free),
            })
            .collect()
    }

    fn alpha_forms(&self) -> Vec<CanonicalScheme> {
        let mut binders = self.binders.clone();
        binders.sort_unstable();
        binders.dedup();
        if !self.accepts_binder_order(&binders) {
            return Vec::new();
        }
        permutations(&binders)
            .into_iter()
            .filter_map(|permutation| self.canonical_form(&permutation))
            .collect()
    }

    fn accepts_binder_order(&self, binder_order: &[u32]) -> bool {
        let declared_binders = binder_order
            .iter()
            .copied()
            .map(Variable::Bound)
            .collect::<BTreeSet<_>>();
        let scheme_binders = self
            .binders
            .iter()
            .copied()
            .map(Variable::Bound)
            .collect::<BTreeSet<_>>();
        let variables = self.variables();
        declared_binders.len() == binder_order.len()
            && scheme_binders.len() == self.binders.len()
            && declared_binders == scheme_binders
            && variables.len() <= MAX_VARIABLES
            && variables.iter().all(|variable| match variable {
                Variable::Bound(_) => declared_binders.contains(variable),
                Variable::Free(_) => true,
            })
    }

    fn canonical_form(&self, binder_order: &[u32]) -> Option<CanonicalScheme> {
        if !self.accepts_binder_order(binder_order) {
            return None;
        }
        let renaming = binder_order
            .iter()
            .enumerate()
            .map(|(canonical, &binder)| {
                (
                    Variable::Bound(binder),
                    Variable::Bound(canonical as u32),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let constraints = self
            .constraints
            .iter()
            .map(|constraint| rename_constraint(constraint, &renaming))
            .collect::<Vec<_>>();
        let contract = rename_contract(&self.contract, &renaming);
        let free_variables = self.free_variables();
        let variables = (0..binder_order.len())
            .map(|binder| Variable::Bound(binder as u32))
            .chain(free_variables.iter().copied().map(Variable::Free))
            .collect::<Vec<_>>();
        let assignments = assignments(&variables);
        let relation = assignments
            .iter()
            .map(|assignment| {
                constraints
                    .iter()
                    .all(|constraint| constraint.satisfied_by(assignment))
            })
            .collect();
        let contracts = assignments
            .iter()
            .zip(&relation)
            .map(|(assignment, &allowed)| {
                if allowed {
                    Some(contract.evaluate(assignment))
                } else {
                    None
                }
            })
            .collect();
        Some(CanonicalScheme {
            binder_count: binder_order.len(),
            free_variables,
            relation,
            contracts,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CanonicalScheme {
    binder_count: usize,
    free_variables: BTreeSet<u64>,
    relation: Vec<bool>,
    contracts: Vec<Option<EffectValue>>,
}

fn schemes_equivalent(left: &EffectScheme, right: &EffectScheme) -> bool {
    if left.binders.len() != right.binders.len()
        || left.free_variables() != right.free_variables()
    {
        return false;
    }
    let left_forms = left.alpha_forms();
    let right_forms = right.alpha_forms();
    left_forms
        .iter()
        .any(|left_form| right_forms.contains(left_form))
}

fn permutations(values: &[u32]) -> Vec<Vec<u32>> {
    if values.is_empty() {
        return vec![Vec::new()];
    }
    (0..values.len())
        .flat_map(|index| {
            let mut remaining = values.to_vec();
            let selected = remaining.remove(index);
            permutations(&remaining)
                .into_iter()
                .map(move |mut permutation| {
                    permutation.insert(0, selected);
                    permutation
                })
        })
        .collect()
}

fn rename_term(term: &Term, renaming: &BTreeMap<Variable, Variable>) -> Term {
    match term {
        Term::Set(effect_set) => Term::Set(*effect_set),
        Term::Variable(variable) => Term::Variable(
            renaming
                .get(variable)
                .copied()
                .unwrap_or(*variable),
        ),
        Term::Union(terms) => {
            Term::Union(terms.iter().map(|term| rename_term(term, renaming)).collect())
        }
        Term::WithoutError(term) => {
            Term::WithoutError(Box::new(rename_term(term, renaming)))
        }
    }
}

fn rename_constraint(
    constraint: &Constraint,
    renaming: &BTreeMap<Variable, Variable>,
) -> Constraint {
    match constraint {
        Constraint::Included { required, allowed } => Constraint::Included {
            required: rename_term(required, renaming),
            allowed: rename_term(allowed, renaming),
        },
        Constraint::Equal { left, right } => Constraint::Equal {
            left: rename_term(left, renaming),
            right: rename_term(right, renaming),
        },
    }
}

fn rename_contract(contract: &Contract, renaming: &BTreeMap<Variable, Variable>) -> Contract {
    match contract {
        Contract::Known(term) => Contract::Known(rename_term(term, renaming)),
        Contract::Unknown => Contract::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        assignments, schemes_equivalent, Constraint, Contract, EffectScheme, EffectSet,
        EffectValue, FreeTerm, Term, Variable, ASSIGNMENT_COUNT, EFFECT_A, EFFECT_B, ERROR,
    };
    use std::collections::BTreeMap;

    fn set(bits: u8) -> EffectSet {
        EffectSet(bits)
    }

    fn bounded_effect(binder: u32, upper: EffectSet) -> EffectScheme {
        let effect = Term::Variable(Variable::Bound(binder));
        EffectScheme {
            binders: vec![binder],
            constraints: vec![Constraint::Included {
                required: effect.clone(),
                allowed: Term::Set(upper),
            }],
            contract: Contract::Known(effect),
        }
    }

    #[test]
    fn explicit_upper_bound_preserves_every_satisfying_callback_assignment() {
        let restricted = bounded_effect(7, set(EFFECT_A));
        let empty_only = EffectScheme {
            binders: vec![11],
            constraints: vec![Constraint::Equal {
                left: Term::Variable(Variable::Bound(11)),
                right: Term::Set(EffectSet::EMPTY),
            }],
            contract: Contract::Known(Term::Variable(Variable::Bound(11))),
        };
        let restricted_form = restricted.canonical_form(&[7]).unwrap();
        assert_eq!(restricted_form.relation.len(), ASSIGNMENT_COUNT);
        assert_eq!(restricted_form.relation.iter().filter(|&&allowed| allowed).count(), 2);
        let two_atom_bound = bounded_effect(12, set(EFFECT_A | EFFECT_B))
            .canonical_form(&[12])
            .unwrap();
        assert_eq!(
            two_atom_bound
                .relation
                .iter()
                .filter(|&&allowed| allowed)
                .count(),
            4
        );
        assert_eq!(
            restricted_form.contracts,
            assignments(&[Variable::Bound(0)])
                .iter()
                .zip(&restricted_form.relation)
                .map(|(assignment, &allowed)| {
                    if allowed {
                        Some(EffectValue::Known(assignment[&Variable::Bound(0)]))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        );
        assert!(!schemes_equivalent(&restricted, &empty_only));
    }

    #[test]
    fn relation_enumeration_covers_all_assignments_for_three_variables() {
        let variables = [
            Variable::Bound(0),
            Variable::Bound(1),
            Variable::Bound(2),
        ];
        assert_eq!(assignments(&variables).len(), ASSIGNMENT_COUNT.pow(3));
    }

    #[test]
    fn alpha_normalization_rejects_four_bound_variables() {
        let scheme = EffectScheme {
            binders: vec![0, 1, 2, 3],
            constraints: Vec::new(),
            contract: Contract::Unknown,
        };
        assert!(scheme.alpha_forms().is_empty());
    }

    #[test]
    fn union_and_error_subtraction_match_the_finite_set_model() {
        let union = Term::Union(vec![
            Term::Set(set(EFFECT_A)),
            Term::Set(set(EFFECT_B)),
            Term::Set(set(ERROR)),
        ]);
        let captured = Term::WithoutError(Box::new(union));
        let relation = EffectScheme {
            binders: Vec::new(),
            constraints: vec![Constraint::Equal {
                left: captured,
                right: Term::Set(set(EFFECT_A | EFFECT_B)),
            }],
            contract: Contract::Known(Term::Set(set(EFFECT_A | EFFECT_B))),
        };
        assert_eq!(relation.canonical_form(&[]).unwrap().relation, [true]);
        for bits in 0..ASSIGNMENT_COUNT {
            let input = set(bits as u8);
            assert_eq!(input.without_error(), set(bits as u8 & !ERROR));
            assert_eq!(input.without_error().0 & (EFFECT_A | EFFECT_B), input.0 & (EFFECT_A | EFFECT_B));
        }

        let left = Term::Variable(Variable::Bound(1));
        let right = Term::Variable(Variable::Bound(2));
        let union_relation = EffectScheme {
            binders: vec![1, 2],
            constraints: vec![Constraint::Equal {
                left: Term::Union(vec![left, right]),
                right: Term::Set(set(EFFECT_A | EFFECT_B)),
            }],
            contract: Contract::Unknown,
        }
        .canonical_form(&[1, 2])
        .unwrap();
        assert_eq!(union_relation.relation.len(), ASSIGNMENT_COUNT.pow(2));
        assert_eq!(
            union_relation
                .relation
                .iter()
                .filter(|&&allowed| allowed)
                .count(),
            9
        );

        let captured_variable = EffectScheme {
            binders: vec![3],
            constraints: vec![Constraint::Equal {
                left: Term::WithoutError(Box::new(Term::Variable(Variable::Bound(3)))),
                right: Term::Set(set(EFFECT_A | EFFECT_B)),
            }],
            contract: Contract::Known(Term::Variable(Variable::Bound(3))),
        }
        .canonical_form(&[3])
        .unwrap();
        assert_eq!(
            captured_variable
                .relation
                .iter()
                .filter(|&&allowed| allowed)
                .count(),
            2
        );
    }

    #[test]
    fn quantified_binders_are_alpha_renamed_but_free_environment_ids_are_anchored() {
        let left = bounded_effect(3, set(EFFECT_A | EFFECT_B));
        let right = bounded_effect(90, set(EFFECT_A | EFFECT_B));
        assert!(schemes_equivalent(&left, &right));

        let free_left = EffectScheme {
            binders: Vec::new(),
            constraints: Vec::new(),
            contract: Contract::Known(Term::Variable(Variable::Free(4))),
        };
        let free_right = EffectScheme {
            binders: Vec::new(),
            constraints: Vec::new(),
            contract: Contract::Known(Term::Variable(Variable::Free(5))),
        };
        assert!(!schemes_equivalent(&free_left, &free_right));
    }

    #[test]
    fn contract_differences_outside_relation_are_ignored_but_allowed_differences_are_not() {
        let constrained_variable = EffectScheme {
            binders: vec![7],
            constraints: vec![Constraint::Equal {
                left: Term::Variable(Variable::Bound(7)),
                right: Term::Set(EffectSet::EMPTY),
            }],
            contract: Contract::Known(Term::Variable(Variable::Bound(7))),
        };
        let constant_empty = EffectScheme {
            binders: vec![11],
            constraints: vec![Constraint::Equal {
                left: Term::Variable(Variable::Bound(11)),
                right: Term::Set(EffectSet::EMPTY),
            }],
            contract: Contract::Known(Term::Set(EffectSet::EMPTY)),
        };
        let changed_allowed_output = EffectScheme {
            contract: Contract::Known(Term::Set(set(EFFECT_A))),
            ..constant_empty.clone()
        };

        assert!(schemes_equivalent(
            &constrained_variable,
            &constant_empty
        ));
        assert!(!schemes_equivalent(
            &constrained_variable,
            &changed_allowed_output
        ));
    }

    #[test]
    fn free_substitution_preserves_binders_and_upper_bound_relationships() {
        let scheme = EffectScheme {
            binders: vec![7],
            constraints: vec![Constraint::Included {
                required: Term::Variable(Variable::Free(4)),
                allowed: Term::Variable(Variable::Bound(7)),
            }],
            contract: Contract::Known(Term::Union(vec![
                Term::Variable(Variable::Free(4)),
                Term::Variable(Variable::Bound(7)),
            ])),
        };
        let substituted = scheme.substitute_free(
            4,
            &FreeTerm::WithoutError(Box::new(FreeTerm::Union(vec![
                FreeTerm::Set(set(EFFECT_A)),
                FreeTerm::Variable(9),
                FreeTerm::Set(set(ERROR)),
            ]))),
        );
        assert_eq!(substituted.binders, [7]);
        assert_eq!(substituted.free_variables().len(), 1);
        assert!(substituted.free_variables().contains(&9));
        let assignment = BTreeMap::from([
            (Variable::Bound(7), set(EFFECT_A | EFFECT_B)),
            (Variable::Free(9), set(EFFECT_B)),
        ]);
        assert!(substituted.constraints[0].satisfied_by(&assignment));
        assert_eq!(
            substituted.contract.evaluate(&assignment),
            EffectValue::Known(set(EFFECT_A | EFFECT_B))
        );
    }

    #[test]
    fn unknown_contract_stays_distinct_from_empty_through_union_and_capture() {
        let empty = EffectValue::Known(EffectSet::EMPTY);
        assert_ne!(EffectValue::Unknown, empty);
        assert_eq!(EffectValue::Unknown.union(empty), EffectValue::Unknown);
        assert_eq!(EffectValue::Unknown.without_error(), EffectValue::Unknown);
    }
}
