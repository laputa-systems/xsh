use super::{ConstraintMetrics, RowShape, RowTail, TypeConstraints};
use crate::sema::types::Type;
use crate::source::{SourceId, Span};
use crate::symbol::{Name, SymbolOwner};
use std::collections::BTreeMap;
use std::sync::Arc;

fn span(start: usize) -> Span {
    Span::new(SourceId::new(0), start, start + 1)
}

fn resolved_output_nodes(ty: &Type) -> usize {
    match ty {
        Type::Int | Type::Str => 1,
        Type::Record(fields) => {
            1 + fields
                .values()
                .map(resolved_output_nodes)
                .sum::<usize>()
        }
        other => panic!("unexpected scaling output type: {other:?}"),
    }
}

fn named_counters(metrics: ConstraintMetrics) -> [(&'static str, u64); 12] {
    [
        ("fresh_variables", metrics.fresh_variables),
        ("generated_constraints", metrics.generated_constraints),
        ("representative_hops", metrics.representative_hops),
        ("compression_writes", metrics.compression_writes),
        ("unifications", metrics.unifications),
        ("occurs_visits", metrics.occurs_visits),
        ("row_field_visits", metrics.row_field_visits),
        ("scheme_instantiations", metrics.scheme_instantiations),
        (
            "successful_generic_call_resolutions",
            metrics.successful_generic_call_resolutions,
        ),
        ("worklist_wakeups", metrics.worklist_wakeups),
        ("rollbacks", metrics.rollbacks),
        ("rollback_changes", metrics.rollback_changes),
    ]
}

#[derive(Clone, Copy)]
struct ScalingObservation {
    size: usize,
    output_nodes: usize,
    metrics: ConstraintMetrics,
}

fn assert_output_normalized_growth(label: &str, observations: &[ScalingObservation]) {
    let mut previous: Option<(usize, [(&'static str, u64); 12])> = None;
    for observation in observations {
        let output_nodes = u64::try_from(observation.output_nodes).unwrap();
        let raw = named_counters(observation.metrics);
        let normalized = raw.map(|(name, value)| (name, value.div_ceil(output_nodes)));
        println!(
            "{label} size={} output_nodes={} normalized_per_output={normalized:?} raw={raw:?}",
            observation.size, observation.output_nodes
        );
        if let Some((previous_size, previous_normalized)) = &previous {
            for ((name, current), (previous_name, previous)) in
                normalized.iter().zip(previous_normalized.iter())
            {
                assert_eq!(name, previous_name);
                let bound = previous.saturating_mul(3).saturating_add(128);
                assert!(
                    current <= &bound,
                    "{label} {name} normalized per output grew from size {previous_size} value {previous} to size {} value {current}, above {bound}",
                    observation.size,
                );
            }
        }
        previous = Some((observation.size, normalized));
    }
}

fn field_name(index: usize) -> Name {
    Name::intern(format!("field_{index:04}"))
}

#[test]
fn alias_chain_solver_work_scales_against_its_ground_output() {
    let mut observations = Vec::new();
    for size in [64, 128, 256, 512] {
        let mut constraints = TypeConstraints::default();
        let variables = (0..size)
            .map(|index| constraints.fresh(span(index)))
            .collect::<Vec<_>>();
        for index in (1..size).rev() {
            constraints
                .constrain(&variables[index], &variables[index - 1], span(index))
                .unwrap();
        }
        constraints
            .constrain(&variables[0], &Type::Int, span(size))
            .unwrap();

        let resolved = constraints.resolve(variables.last().unwrap()).unwrap();
        assert_eq!(resolved, Type::Int);
        let output_nodes = resolved_output_nodes(&resolved);
        assert_eq!(output_nodes, 1);
        let metrics = constraints.metrics();
        assert_eq!(metrics.fresh_variables, u64::try_from(size).unwrap());
        assert_eq!(metrics.generated_constraints, u64::try_from(size).unwrap());
        assert_eq!(metrics.unifications, u64::try_from(size).unwrap());
        observations.push(ScalingObservation {
            size,
            output_nodes,
            metrics,
        });
    }
    assert_output_normalized_growth("alias-chain", &observations);
}

#[test]
fn shallow_record_solver_work_scales_with_resolved_output_nodes() {
    SymbolOwner::new().with_current(|| {
        let mut observations = Vec::new();
        for width in [64, 128, 256, 512] {
            let mut constraints = TypeConstraints::default();
            let receiver = constraints.fresh(span(0));
            let first_field = field_name(0);
            let projection = constraints
                .project_record_field(&receiver, first_field, 0, span(1))
                .unwrap();
            constraints.constrain(&projection, &Type::Int, span(2)).unwrap();
            let fields = (0..width)
                .map(|index| (field_name(index), Type::Int))
                .collect::<BTreeMap<_, _>>();
            let expected = Type::Record(Arc::new(fields));
            constraints.constrain(&receiver, &expected, span(3)).unwrap();

            let resolved = constraints.resolve(&receiver).unwrap();
            assert_eq!(resolved, expected);
            assert_eq!(constraints.resolve(&projection), Ok(Type::Int));
            let output_nodes = resolved_output_nodes(&resolved);
            assert_eq!(output_nodes, width + 1);
            let metrics = constraints.metrics();
            assert_eq!(metrics.row_field_visits, u64::try_from(width + 1).unwrap());
            observations.push(ScalingObservation {
                size: width,
                output_nodes,
                metrics,
            });
        }
        assert_output_normalized_growth("shallow-record", &observations);
    });
}

#[test]
fn shallow_record_conflict_rolls_back_each_partial_field_binding() {
    SymbolOwner::new().with_current(|| {
        let mut observations = Vec::new();
        for width in [64, 128, 256, 512] {
            let mut constraints = TypeConstraints::default();
            let mut variables = Vec::with_capacity(width - 1);
            let expected_fields = (0..width)
                .map(|index| {
                    let ty = if index + 1 == width {
                        Type::Bool
                    } else {
                        let variable = constraints.fresh(span(index));
                        variables.push(variable.clone());
                        variable
                    };
                    (field_name(index), ty)
                })
                .collect::<BTreeMap<_, _>>();
            let actual_fields = (0..width)
                .map(|index| {
                    let ty = if index + 1 == width {
                        Type::Str
                    } else {
                        Type::Int
                    };
                    (field_name(index), ty)
                })
                .collect::<BTreeMap<_, _>>();
            let expected = RowShape {
                fields: expected_fields,
                tail: RowTail::Closed,
            };
            let actual = RowShape {
                fields: actual_fields.clone(),
                tail: RowTail::Closed,
            };

            assert!(constraints.constrain_rows(&expected, &actual, span(width)).is_err());
            for variable in &variables {
                assert_eq!(constraints.resolve(variable), Ok(variable.clone()));
            }
            let actual = Type::Record(Arc::new(actual_fields));
            let resolved_actual = constraints.resolve(&actual).unwrap();
            assert_eq!(resolved_actual, actual);
            assert_eq!(resolved_output_nodes(&resolved_actual), width + 1);
            let metrics = constraints.metrics();
            assert_eq!(metrics.rollbacks, 1);
            assert!(metrics.rollback_changes >= u64::try_from(width - 1).unwrap());
            observations.push(ScalingObservation {
                size: width,
                output_nodes: width + 1,
                metrics,
            });
        }
        assert_output_normalized_growth("shallow-record-rollback", &observations);
    });
}
