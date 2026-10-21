use super::inference_oracle::effects as oracle;
use super::span;
use crate::sema::constraints::{
    ConstraintResolutionError, EffectConstraintError, EffectPredicate, EffectVariableId, TypeConstraints,
};
use crate::sema::effects::{EffectContract, EffectSet};
use crate::sema::types::Type;
use crate::syntax::node::Effect;

struct ActualProblem {
    constraints: TypeConstraints,
    variables: Vec<EffectVariableId>,
}

fn actual_atom(atom: oracle::Atom) -> Effect {
    match atom {
        oracle::Atom::Fs => Effect::Fs,
        oracle::Atom::Net => Effect::Net,
        oracle::Atom::Process => Effect::Process,
        oracle::Atom::Env => Effect::Env,
        oracle::Atom::Time => Effect::Time,
        oracle::Atom::Error => Effect::Error,
        oracle::Atom::Io => Effect::Io,
    }
}

fn model_atom(atom: Effect) -> oracle::Atom {
    match atom {
        Effect::Fs => oracle::Atom::Fs,
        Effect::Net => oracle::Atom::Net,
        Effect::Process => oracle::Atom::Process,
        Effect::Env => oracle::Atom::Env,
        Effect::Time => oracle::Atom::Time,
        Effect::Error => oracle::Atom::Error,
        Effect::Io => oracle::Atom::Io,
    }
}

fn actual_set(set: oracle::Set) -> EffectSet {
    EffectSet::new(set.atoms().map(actual_atom))
}

fn actual_term(term: &oracle::Term, variables: &[EffectVariableId]) -> EffectContract<EffectVariableId> {
    match term {
        oracle::Term::Finite(set) => EffectContract::finite(actual_set(*set)),
        oracle::Term::Variable(index) => EffectContract::variable(variables[*index]),
        oracle::Term::Union(terms) => terms.iter().fold(EffectContract::finite(EffectSet::default()), |union, term| {
            union.union(actual_term(term, variables))
        }),
        oracle::Term::Mask(term, mask) => actual_term(term, variables).masked(actual_set(*mask)),
        oracle::Term::CaptureError(term) => actual_term(term, variables).capture_error(),
        oracle::Term::Unknown => EffectContract::Unknown,
    }
}

fn actual_predicate(predicate: &oracle::Predicate, variables: &[EffectVariableId]) -> EffectPredicate {
    match predicate {
        oracle::Predicate::Equal(left, right) => EffectPredicate::Equal(actual_term(left, variables), actual_term(right, variables)),
        oracle::Predicate::Included(left, right) => EffectPredicate::Included(actual_term(left, variables), actual_term(right, variables)),
        oracle::Predicate::Covered(left, right) => EffectPredicate::Covered(actual_term(left, variables), actual_term(right, variables)),
        oracle::Predicate::Excluded(term, excluded) => EffectPredicate::Excluded(actual_term(term, variables), actual_set(*excluded)),
    }
}

fn build_actual(problem: &oracle::Problem) -> Result<ActualProblem, EffectConstraintError> {
    let mut constraints = TypeConstraints::default();
    let variables = (0..problem.variables).map(|index| {
        constraints.fresh_effect_at_level(span(index as u32 + 1), 1)
    }).collect::<Vec<_>>();
    for predicate in &problem.predicates {
        constraints.constrain_effect(actual_predicate(predicate, &variables))?;
    }
    Ok(ActualProblem { constraints, variables })
}

fn actual_relation(problem: &oracle::Problem, actual: &ActualProblem) -> oracle::Relation {
    problem.assignments().iter().map(|assignment| {
        let mut constraints = actual.constraints.clone();
        let result = actual.variables.iter().zip(assignment).try_for_each(|(&variable, &set)| {
            constraints.constrain_effect(EffectPredicate::Equal(
                EffectContract::variable(variable), EffectContract::finite(actual_set(set)),
            ))
        }).and_then(|()| constraints.finish_effect_constraints());
        match result {
            Ok(()) => Some(problem.observations.iter().map(|term| {
                match constraints.resolve_effect(&actual_term(term, &actual.variables)).unwrap() {
                    EffectContract::Unknown => oracle::Value::Unknown,
                    EffectContract::Known(expression) => {
                        let set = expression.closed().expect("every live effect variable has a concrete assignment");
                        oracle::Value::Known(oracle::Set::new(set.iter().map(model_atom)))
                    }
                }
            }).collect()),
            Err(EffectConstraintError::Mismatch) => None,
            Err(error) => panic!("concrete effect assignment failed to finish: {assignment:?}: {error:?}"),
        }
    }).collect()
}

fn compare_problem(problem: &oracle::Problem, seed: u64) -> oracle::Relation {
    let expected = problem.relation();
    let actual = match build_actual(problem) {
        Ok(actual) => actual_relation(problem, &actual),
        Err(EffectConstraintError::Mismatch) => vec![None; expected.len()],
        Err(error) => panic!("effect problem {seed}: {error:?}"),
    };
    assert_eq!(actual, expected, "effect assignment relation differs at seed {seed}: {problem:?}");
    actual
}

#[test]
fn production_effect_constraints_match_independent_finite_assignments() {
    let mut accepted = false;
    let mut rejected = false;
    for seed in 0..64 {
        let problem = oracle::generate_problem(seed);
        let actual = compare_problem(&problem, seed);
        accepted |= actual.iter().any(Option::is_some);
        rejected |= actual.iter().any(Option::is_none);
        if seed == 1 {
            let index = actual.iter().position(Option::is_some).unwrap();
            let mut mutant = actual.clone();
            mutant[index] = None;
            assert_ne!(mutant, problem.relation());
            let mut mutant = actual;
            *mutant[index].as_mut().unwrap().last_mut().unwrap() = oracle::Value::Unknown;
            assert_ne!(mutant, problem.relation());
        }
    }
    assert!(accepted && rejected);
}

#[test]
fn production_effect_constraints_match_independent_closed_relations() {
    let mut terms = oracle::Atom::ALL.into_iter().map(|atom| {
        oracle::Term::Finite(oracle::Set::new([atom]))
    }).collect::<Vec<_>>();
    terms.extend([oracle::Term::Finite(oracle::Set::EMPTY), oracle::Term::Unknown]);
    for left in &terms {
        for right in &terms {
            for predicate in [
                oracle::Predicate::Equal(left.clone(), right.clone()),
                oracle::Predicate::Included(left.clone(), right.clone()),
                oracle::Predicate::Covered(left.clone(), right.clone()),
            ] {
                compare_problem(&oracle::Problem::bounded(0, vec![predicate], vec![left.clone(), right.clone()]), 0);
            }
        }
        for atom in oracle::Atom::ALL {
            let predicate = oracle::Predicate::Excluded(left.clone(), oracle::Set::new([atom]));
            compare_problem(&oracle::Problem::bounded(0, vec![predicate], vec![left.clone()]), 0);
        }
    }
    let fs = oracle::Term::Finite(oracle::Set::new([oracle::Atom::Fs]));
    let io = oracle::Term::Finite(oracle::Set::new([oracle::Atom::Io]));
    assert!(oracle::Predicate::Covered(fs.clone(), io.clone()).accepts(&[]));
    assert!(!oracle::Predicate::Covered(io.clone(), fs.clone()).accepts(&[]));
    assert!(!oracle::Predicate::Included(fs.clone(), io.clone()).accepts(&[]));
    assert!(!oracle::Predicate::Excluded(io, oracle::Set::new([oracle::Atom::Fs])).accepts(&[]));
    assert!(oracle::Predicate::Excluded(fs, oracle::Set::new([oracle::Atom::Io])).accepts(&[]));
}

#[test]
fn production_effect_trials_restore_independent_assignment_continuations() {
    enum End { Success, Conflict, Panic }
    let problem = oracle::Problem::bounded(2, vec![oracle::Predicate::Covered(
        oracle::Term::Variable(0), oracle::Term::Finite(oracle::Set::new([oracle::Atom::Io])),
    )], vec![oracle::Term::Variable(0), oracle::Term::Variable(1)]);
    for end in [End::Success, End::Conflict, End::Panic] {
        let mut actual = build_actual(&problem).unwrap();
        let cloned = ActualProblem { constraints: actual.constraints.clone(), variables: actual.variables.clone() };
        let mut fresh = None;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            actual.constraints.trial(|constraints| {
                let variable = constraints.fresh_effect_at_level(span(100), 2);
                fresh = Some(variable);
                constraints.constrain_effect(EffectPredicate::Equal(
                    EffectContract::variable(actual.variables[0]), EffectContract::variable(actual.variables[1]),
                )).unwrap();
                constraints.constrain_effect(EffectPredicate::Equal(
                    EffectContract::variable(actual.variables[1]), EffectContract::variable(variable),
                )).unwrap();
                constraints.constrain_effect(EffectPredicate::Equal(
                    EffectContract::variable(variable), EffectContract::finite(EffectSet::new([Effect::Fs])),
                )).unwrap();
                constraints.finish_effect_constraints().unwrap();
                match end {
                    End::Success => Ok(()),
                    End::Conflict => constraints.constrain(&Type::Int, &Type::Str, span(101)),
                    End::Panic => panic!("interrupt effect candidate after concrete assignment"),
                }
            })
        }));
        match end {
            End::Success => assert_eq!(outcome.unwrap(), Ok(true)),
            End::Conflict => assert_eq!(outcome.unwrap(), Ok(false)),
            End::Panic => assert!(outcome.is_err()),
        }
        assert_eq!(actual.constraints.resolve_effect(&EffectContract::variable(fresh.unwrap())), Err(ConstraintResolutionError::ForeignVariable));
        assert_eq!(actual_relation(&problem, &actual), problem.relation());
        let mut continued = problem.clone();
        for (index, effect) in [oracle::Atom::Fs, oracle::Atom::Time].into_iter().enumerate() {
            let set = oracle::Set::new([effect]);
            continued.predicates.push(oracle::Predicate::Equal(
                oracle::Term::Variable(index), oracle::Term::Finite(set),
            ));
            actual.constraints.constrain_effect(EffectPredicate::Equal(
                EffectContract::variable(actual.variables[index]), EffectContract::finite(actual_set(set)),
            )).unwrap();
        }
        actual.constraints.finish_effect_constraints().unwrap();
        assert_eq!(actual_relation(&continued, &actual), continued.relation());
        assert_eq!(actual_relation(&problem, &cloned), problem.relation());
    }
}
