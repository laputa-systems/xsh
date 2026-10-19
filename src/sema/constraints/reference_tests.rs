#[path = "../../../crates/xsh-fuzz/tests/support/inference_oracle.rs"]
mod inference_oracle;

use inference_oracle as oracle;
use super::{
    ConstraintConflict, ConstraintResolutionError, ConstraintType, GeneralizedTypes, RowShape,
    RowTail, TypeConstraints, TypeVariableId,
};
use crate::sema::schemes::{
    SchemeBuildError, SchemeCallableKind, SchemeParameter, SchemeRowParameter,
    SchemeRowParameterId, SchemeRowTail, SchemeType, TypeScheme,
};
use crate::sema::types::Type;
use crate::source::{SourceId, Span};
use crate::symbol::{Name, SymbolOwner};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

struct ActualEquation {
    left: ConstraintType,
    right: ConstraintType,
    contribution: Span,
}

struct ActualProblem {
    constraints: TypeConstraints,
    type_variables: BTreeMap<oracle::TypeVariable, TypeVariableId>,
    row_variables: BTreeMap<oracle::RowVariable, super::RowVariableId>,
    equations: Vec<ActualEquation>,
    observations: Vec<ConstraintType>,
}

struct OutputIds {
    type_variables: BTreeMap<TypeVariableId, oracle::TypeVariable>,
    row_variables: BTreeMap<super::RowVariableId, oracle::RowVariable>,
    next_type: u32,
    next_row: u32,
}

impl OutputIds {
    fn new(problem: &oracle::Problem, actual: &ActualProblem) -> Self {
        let type_variables = actual
            .type_variables
            .iter()
            .map(|(&model, &compiler)| (compiler, model))
            .collect();
        let row_variables = actual
            .row_variables
            .iter()
            .map(|(&model, &compiler)| (compiler, model))
            .collect();
        let next_type = problem
            .type_variables()
            .map(|(variable, _)| variable.0)
            .max()
            .map_or(0, |id| id + 1);
        let next_row = problem
            .row_variables()
            .map(|(variable, _, _)| variable.0)
            .max()
            .map_or(0, |id| id + 1);
        Self {
            type_variables,
            row_variables,
            next_type,
            next_row,
        }
    }

    fn type_variable(&mut self, variable: TypeVariableId) -> oracle::TypeVariable {
        *self.type_variables.entry(variable).or_insert_with(|| {
            let id = oracle::TypeVariable(self.next_type);
            self.next_type += 1;
            id
        })
    }

    fn row_variable(&mut self, variable: super::RowVariableId) -> oracle::RowVariable {
        *self.row_variables.entry(variable).or_insert_with(|| {
            let id = oracle::RowVariable(self.next_row);
            self.next_row += 1;
            id
        })
    }
}

fn span(start: u32) -> Span {
    Span::new(SourceId::new(1), start as usize, start as usize + 1)
}

fn build_actual(problem: &oracle::Problem) -> ActualProblem {
    let mut constraints = TypeConstraints::default();
    let type_variables = problem
        .type_variables()
        .map(|(variable, level)| {
            let Type::Inference(id) = constraints.fresh_at_level(span(variable.0 + 1), level)
            else {
                unreachable!();
            };
            (variable, id)
        })
        .collect::<BTreeMap<_, _>>();
    let row_variables = problem
        .row_variables()
        .map(|(variable, level, lacks)| {
            let id = constraints.fresh_row_at_level(
                span(variable.0 + 1024),
                level,
                lacks.iter().map(|name| Name::intern(name)),
            );
            (variable, id)
        })
        .collect::<BTreeMap<_, _>>();
    let equations = problem
        .equations()
        .iter()
        .enumerate()
        .map(|(index, equation)| ActualEquation {
            left: constraint_type(&equation.left, &type_variables, &row_variables)
                .unwrap_or_else(|error| panic!("equation {index} left: {error}")),
            right: constraint_type(&equation.right, &type_variables, &row_variables)
                .unwrap_or_else(|error| panic!("equation {index} right: {error}")),
            contribution: span(index as u32 + 4096),
        })
        .collect();
    let observations = problem
        .observations()
        .iter()
        .map(|ty| constraint_type(ty, &type_variables, &row_variables))
        .collect::<Result<_, _>>()
        .expect("generated observations use the bounded type and row fragment");
    ActualProblem {
        constraints,
        type_variables,
        row_variables,
        equations,
        observations,
    }
}

fn constraint_type(
    ty: &oracle::Type,
    type_variables: &BTreeMap<oracle::TypeVariable, TypeVariableId>,
    row_variables: &BTreeMap<oracle::RowVariable, super::RowVariableId>,
) -> Result<ConstraintType, String> {
    match ty {
        oracle::Type::Record(row) => Ok(ConstraintType::Record(row_shape(
            row,
            type_variables,
            row_variables,
        )?)),
        _ => Ok(ConstraintType::Ground(actual_type(
            ty,
            type_variables,
            row_variables,
        )?)),
    }
}

fn row_shape(
    row: &oracle::Row,
    type_variables: &BTreeMap<oracle::TypeVariable, TypeVariableId>,
    row_variables: &BTreeMap<oracle::RowVariable, super::RowVariableId>,
) -> Result<RowShape, String> {
    let fields = row
        .fields
        .iter()
        .map(|(name, ty)| {
            Ok((
                Name::intern(name),
                actual_type(ty, type_variables, row_variables)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let tail = match row.tail {
        oracle::RowTail::Closed => RowTail::Closed,
        oracle::RowTail::Variable(variable) => RowTail::Variable(
            *row_variables
                .get(&variable)
                .ok_or_else(|| format!("undeclared row variable {:?}", variable))?,
        ),
    };
    Ok(RowShape { fields, tail })
}

fn actual_type(
    ty: &oracle::Type,
    type_variables: &BTreeMap<oracle::TypeVariable, TypeVariableId>,
    row_variables: &BTreeMap<oracle::RowVariable, super::RowVariableId>,
) -> Result<Type, String> {
    Ok(match ty {
        oracle::Type::Variable(variable) => Type::Inference(
            *type_variables
                .get(variable)
                .ok_or_else(|| format!("undeclared type variable {:?}", variable))?,
        ),
        oracle::Type::Atom(oracle::Atom::Int) => Type::Int,
        oracle::Type::Atom(oracle::Atom::Str) => Type::Str,
        oracle::Type::Atom(oracle::Atom::Bool) => Type::Bool,
        oracle::Type::Atom(oracle::Atom::Unit) => Type::Unit,
        oracle::Type::List(inner) => Type::List(Box::new(actual_type(
            inner,
            type_variables,
            row_variables,
        )?)),
        oracle::Type::Optional(inner) => Type::Optional(Box::new(actual_type(
            inner,
            type_variables,
            row_variables,
        )?)),
        oracle::Type::Result(ok, err) => Type::Result(
            Box::new(actual_type(ok, type_variables, row_variables)?),
            Box::new(actual_type(err, type_variables, row_variables)?),
        ),
        oracle::Type::Record(row) => {
            if row.tail != oracle::RowTail::Closed {
                return Err("nested open records are outside this differential fragment".to_owned());
            }
            let fields = row
                .fields
                .iter()
                .map(|(name, ty)| {
                    Ok((
                        Name::intern(name),
                        actual_type(ty, type_variables, row_variables)?,
                    ))
                })
                .collect::<Result<BTreeMap<_, _>, String>>()?;
            Type::Record(Arc::new(fields))
        }
    })
}

fn apply_equations(actual: &mut ActualProblem) -> Result<(), super::ConstraintTypeConflict> {
    for equation in &actual.equations {
        actual.constraints.constrain_types(
            &equation.left,
            &equation.right,
            equation.contribution,
        )?;
    }
    Ok(())
}

fn resolved_actual_outputs(
    actual: &ActualProblem,
    ids: &mut OutputIds,
) -> Result<Vec<oracle::Type>, ConstraintResolutionError> {
    actual
        .observations
        .iter()
        .map(|observation| {
            let resolved = actual.constraints.resolve_constraint(observation)?;
            model_constraint_type(&resolved, ids)
        })
        .collect()
}

fn model_constraint_type(
    ty: &ConstraintType,
    ids: &mut OutputIds,
) -> Result<oracle::Type, ConstraintResolutionError> {
    Ok(match ty {
        ConstraintType::Variable(variable) => {
            oracle::Type::Variable(ids.type_variable(*variable))
        }
        ConstraintType::Ground(ty) => model_actual_type(ty, ids)?,
        ConstraintType::Record(row) => oracle::Type::Record(model_row_shape(row, ids)?),
        ConstraintType::Rigid(_) => unreachable!("the generated fragment has no rigid types"),
    })
}

fn model_actual_type(
    ty: &Type,
    ids: &mut OutputIds,
) -> Result<oracle::Type, ConstraintResolutionError> {
    Ok(match ty {
        Type::Inference(variable) => oracle::Type::Variable(ids.type_variable(*variable)),
        Type::Int => oracle::Type::Atom(oracle::Atom::Int),
        Type::Str => oracle::Type::Atom(oracle::Atom::Str),
        Type::Bool => oracle::Type::Atom(oracle::Atom::Bool),
        Type::Unit => oracle::Type::Atom(oracle::Atom::Unit),
        Type::List(inner) => oracle::Type::List(Box::new(model_actual_type(inner, ids)?)),
        Type::Optional(inner) => {
            oracle::Type::Optional(Box::new(model_actual_type(inner, ids)?))
        }
        Type::Result(ok, err) => oracle::Type::Result(
            Box::new(model_actual_type(ok, ids)?),
            Box::new(model_actual_type(err, ids)?),
        ),
        Type::Record(fields) => oracle::Type::Record(oracle::Row::closed(
            fields
                .iter()
                .map(|(name, ty)| {
                    Ok((
                        name.as_str().as_str().to_owned(),
                        model_actual_type(ty, ids)?,
                    ))
                })
                .collect::<Result<BTreeMap<_, _>, ConstraintResolutionError>>()?,
        )),
        _ => unreachable!("the generated fragment contains only atoms, variables, and wrappers"),
    })
}

fn model_row_shape(
    row: &RowShape,
    ids: &mut OutputIds,
) -> Result<oracle::Row, ConstraintResolutionError> {
    let fields = row
        .fields
        .iter()
        .map(|(name, ty)| {
            Ok((
                name.as_str().as_str().to_owned(),
                model_actual_type(ty, ids)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, ConstraintResolutionError>>()?;
    let tail = match row.tail {
        RowTail::Closed => oracle::RowTail::Closed,
        RowTail::Variable(variable) => oracle::RowTail::Variable(ids.row_variable(variable)),
        RowTail::Rigid(_) => unreachable!("the generated fragment has no rigid rows"),
    };
    Ok(oracle::Row { fields, tail })
}

fn actual_generalization_roots(
    actual: &mut ActualProblem,
) -> Result<Vec<Type>, super::ConstraintTypeConflict> {
    actual
        .observations
        .clone()
        .into_iter()
        .enumerate()
        .map(|(index, observation)| match observation {
            ConstraintType::Record(_) => {
                let root = actual.constraints.fresh_at_level(span(index as u32 + 8192), 2);
                let Type::Inference(id) = &root else {
                    unreachable!();
                };
                actual.constraints.constrain_types(
                    &ConstraintType::Variable(*id),
                    &observation,
                    span(index as u32 + 8192),
                )?;
                Ok(root)
            }
            ConstraintType::Variable(id) => Ok(Type::Inference(id)),
            ConstraintType::Ground(ty) => Ok(ty),
            ConstraintType::Rigid(_) => unreachable!("the generated fragment has no rigid types"),
        })
        .collect()
}

fn model_scheme_type(ty: &SchemeType, ids: &mut OutputIds) -> oracle::SchemeType {
    match ty {
        SchemeType::Ground(ty) => match model_actual_type(ty, ids) {
            Ok(oracle::Type::Variable(variable)) => oracle::SchemeType::Free(variable),
            Ok(oracle::Type::Atom(atom)) => oracle::SchemeType::Ground(atom),
            Ok(oracle::Type::List(inner)) => {
                oracle::SchemeType::List(Box::new(model_type_as_scheme(*inner)))
            }
            Ok(oracle::Type::Optional(inner)) => {
                oracle::SchemeType::Optional(Box::new(model_type_as_scheme(*inner)))
            }
            Ok(oracle::Type::Result(ok, err)) => oracle::SchemeType::Result(
                Box::new(model_type_as_scheme(*ok)),
                Box::new(model_type_as_scheme(*err)),
            ),
            Ok(oracle::Type::Record(row)) => oracle::SchemeType::Record {
                fields: row
                    .fields
                    .into_iter()
                    .map(|(name, ty)| (name, model_term_as_scheme(ty)))
                    .collect(),
                tail: match row.tail {
                    oracle::RowTail::Closed => oracle::SchemeRowTail::Closed,
                    oracle::RowTail::Variable(variable) => {
                        oracle::SchemeRowTail::Free(variable)
                    }
                },
            },
            Err(error) => unreachable!("generated scheme type resolves: {error:?}"),
        },
        SchemeType::Parameter(parameter) => {
            oracle::SchemeType::Parameter(oracle::TypeParameter(parameter.0))
        }
        SchemeType::List(inner) => {
            oracle::SchemeType::List(Box::new(model_scheme_type(inner, ids)))
        }
        SchemeType::Optional(inner) => {
            oracle::SchemeType::Optional(Box::new(model_scheme_type(inner, ids)))
        }
        SchemeType::Result(ok, err) => oracle::SchemeType::Result(
            Box::new(model_scheme_type(ok, ids)),
            Box::new(model_scheme_type(err, ids)),
        ),
        SchemeType::Record { fields, tail } => oracle::SchemeType::Record {
            fields: fields
                .iter()
                .map(|(name, ty)| (name.as_str().as_str().to_owned(), model_scheme_type(ty, ids)))
                .collect(),
            tail: match tail {
                SchemeRowTail::Closed => oracle::SchemeRowTail::Closed,
                SchemeRowTail::Parameter(parameter) => {
                    oracle::SchemeRowTail::Parameter(oracle::RowParameter(parameter.0))
                }
            },
        },
        _ => unreachable!("the generated fragment contains only bounded scheme forms"),
    }
}

fn model_type_as_scheme(ty: oracle::Type) -> oracle::SchemeType {
    model_term_as_scheme(ty)
}

fn model_term_as_scheme(ty: oracle::Type) -> oracle::SchemeType {
    match ty {
        oracle::Type::Variable(variable) => oracle::SchemeType::Free(variable),
        oracle::Type::Atom(atom) => oracle::SchemeType::Ground(atom),
        oracle::Type::List(inner) => {
            oracle::SchemeType::List(Box::new(model_term_as_scheme(*inner)))
        }
        oracle::Type::Optional(inner) => {
            oracle::SchemeType::Optional(Box::new(model_term_as_scheme(*inner)))
        }
        oracle::Type::Result(ok, err) => oracle::SchemeType::Result(
            Box::new(model_term_as_scheme(*ok)),
            Box::new(model_term_as_scheme(*err)),
        ),
        oracle::Type::Record(row) => oracle::SchemeType::Record {
            fields: row
                .fields
                .into_iter()
                .map(|(name, ty)| (name, model_term_as_scheme(ty)))
                .collect(),
            tail: match row.tail {
                oracle::RowTail::Closed => oracle::SchemeRowTail::Closed,
                oracle::RowTail::Variable(variable) => oracle::SchemeRowTail::Free(variable),
            },
        },
    }
}

fn model_generalized_types(generalized: &GeneralizedTypes, ids: &mut OutputIds) -> oracle::Scheme {
    oracle::Scheme {
        types: generalized
            .types
            .iter()
            .map(|ty| model_scheme_type(ty, ids))
            .collect(),
        type_parameters: generalized
            .type_parameters
            .iter()
            .map(|parameter| oracle::TypeParameter(parameter.0))
            .collect(),
        row_parameters: generalized
            .row_parameters
            .iter()
            .map(|parameter| oracle::SchemeRowParameter {
                id: oracle::RowParameter(parameter.id.0),
                lacks: parameter
                    .lacks
                    .iter()
                    .map(|name| name.as_str().as_str().to_owned())
                    .collect::<BTreeSet<_>>(),
            })
            .collect(),
    }
}

fn actual_trial(actual: &mut ActualProblem) -> Result<bool, ConstraintResolutionError> {
    let equations = &actual.equations;
    actual.constraints.trial(|constraints| {
        for equation in equations {
            if let Err(conflict) = constraints.constrain_types(
                &equation.left,
                &equation.right,
                equation.contribution,
            ) {
                return Err(ConstraintConflict {
                    expected: Type::Any,
                    actual: Type::Any,
                    initializer: conflict.initializer,
                    established: conflict.established,
                    contribution: conflict.contribution,
                    resolution_error: conflict.resolution_error,
                });
            }
        }
        Ok(())
    })
}

#[test]
fn callable_scheme_rejects_record_fields_missing_from_row_lacks() {
    let symbols = SymbolOwner::new();
    symbols.with_current(|| {
        let row_parameter = SchemeRowParameterId(0);
        let result = TypeScheme::new(
            SchemeCallableKind::Pure,
            Vec::new(),
            vec![SchemeRowParameter {
                id: row_parameter,
                lacks: BTreeSet::new(),
            }],
            Vec::<SchemeParameter>::new(),
            SchemeType::Record {
                fields: BTreeMap::from([(Name::intern("field"), SchemeType::Ground(Type::Int))]),
                tail: SchemeRowTail::Parameter(row_parameter),
            },
            None,
            false,
            Some(Vec::new()),
            false,
        );

        assert_eq!(result, Err(SchemeBuildError::RowLacksMismatch));
    });
}

#[test]
fn production_constraints_match_independent_generated_type_and_row_problems() {
    let symbols = SymbolOwner::new();
    symbols.with_current(|| {
        for seed in 0..128 {
            let generated = oracle::generate_problem(seed);
            let expected = generated.problem.solve();
            assert_eq!(
                expected.is_ok(),
                generated.satisfiable,
                "oracle seed {seed}: {expected:?}",
            );
            let mut actual = build_actual(&generated.problem);
            let actual_result = apply_equations(&mut actual);
            assert_eq!(
                actual_result.is_ok(),
                expected.is_ok(),
                "production satisfiability differs at seed {seed}: {actual_result:?} vs {expected:?}",
            );
            if let Ok(solution) = expected {
                let expected_outputs = generated
                    .problem
                    .observations()
                    .iter()
                    .map(|ty| solution.resolve_type(ty).unwrap())
                    .collect::<Vec<_>>();
                let mut ids = OutputIds::new(&generated.problem, &actual);
                let actual_outputs = resolved_actual_outputs(&actual, &mut ids)
                    .unwrap_or_else(|error| panic!("production resolution at seed {seed}: {error:?}"));
                assert!(oracle::types_equivalent(&expected_outputs, &actual_outputs), "resolved outputs differ at seed {seed}: {expected_outputs:?} vs {actual_outputs:?}");

                let expected_scheme = solution
                    .generalize(generated.problem.observations(), 0)
                    .unwrap();
                let roots = actual_generalization_roots(&mut actual)
                    .unwrap_or_else(|error| panic!("production record root at seed {seed}: {error:?}"));
                let actual_scheme = actual
                    .constraints
                    .generalize_types(&roots, 0)
                    .unwrap_or_else(|error| panic!("production generalization at seed {seed}: {error:?}"));
                let actual_scheme = model_generalized_types(&actual_scheme, &mut ids);
                assert!(oracle::schemes_equivalent(&expected_scheme, &actual_scheme), "generalized schemes differ at seed {seed}: {expected_scheme:?} vs {actual_scheme:?}");

                if seed == 1 {
                    let mut mutant = actual_outputs.clone();
                    mutant[0] = oracle::Type::Atom(oracle::Atom::Str);
                    assert!(!oracle::types_equivalent(&expected_outputs, &mutant));
                    let mut mutant_scheme = actual_scheme.clone();
                    mutant_scheme.types[0] = oracle::SchemeType::Ground(oracle::Atom::Str);
                    assert!(!oracle::schemes_equivalent(&expected_scheme, &mutant_scheme));
                }
            }

            let original_observations = generated.problem.observations();
            let mut model_trial = generated.problem.solver();
            let expected_trial = model_trial.trial(generated.problem.equations());
            let mut trial_actual = build_actual(&generated.problem);
            let actual_trial = actual_trial(&mut trial_actual);
            assert_eq!(
                actual_trial.as_ref().map(|success| *success),
                Ok(expected_trial.is_ok()),
                "trial result differs at seed {seed}: {actual_trial:?} vs {expected_trial:?}",
            );
            let model_after_trial = original_observations
                .iter()
                .map(|ty| model_trial.resolve_type(ty).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(model_after_trial, original_observations, "oracle trial failed to roll back at seed {seed}");
            let actual_after_trial = resolved_actual_outputs(
                &trial_actual,
                &mut OutputIds::new(&generated.problem, &trial_actual),
            )
            .unwrap_or_else(|error| panic!("production rollback resolution at seed {seed}: {error:?}"));
            assert_eq!(actual_after_trial, original_observations, "production trial failed to roll back at seed {seed}");
        }
    });
}
