use super::*;
use crate::source::SourceId;

fn span() -> Span { Span::new(SourceId::new(0), 0, 1) }
fn reason(graph: &mut InferenceContext) -> ReasonId { graph.reason(span(), None).unwrap() }
fn field(label: &str, ty: TypeId) -> RowField { RowField { label: Name::intern(label), ty } }
fn unary(graph: &mut InferenceContext, parameter: TypeId, result: TypeId) -> TypeId {
    graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern("value"), ty: parameter, defaulted: false, rest: false }], result, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap()
}

#[test]
fn occurs_failure_restores_nested_substitutions() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let variable = graph.fresh(1, span()).unwrap(); let list = graph.list(variable).unwrap();
    assert!(matches!(graph.unify(variable, list, why), Err(InferenceError::Occurs { .. })));
    assert_eq!(graph.resolved(variable).unwrap(), variable);
}

#[test]
fn failed_probe_handles_do_not_alias_later_allocations_or_other_graphs() {
    let mut graph = InferenceContext::default(); let mut abandoned = None;
    let result: Result<(), _> = graph.probe(|graph| { abandoned = Some(graph.atom(Atom::Str)?); Err(InferenceError::InvalidScheme) });
    assert!(result.is_err()); let next = graph.atom(Atom::Str).unwrap();
    assert_ne!(abandoned.unwrap(), next); assert!(graph.node(abandoned.unwrap()).is_err());
    let other = InferenceContext::default(); assert!(other.node(next).is_err());
}

#[test]
fn open_projection_keeps_actual_fields_and_projects_distinct_types() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let entry = graph.fresh(1, span()).unwrap();
    let projected = graph.require_field(entry, Name::intern("name"), 1, why).unwrap();
    let arrow = unary(&mut graph, entry, projected);
    let scheme = graph.generalize(arrow, 0, Generalization::Allowed, &[]).unwrap();
    for atom in [Atom::Str, Atom::Int, Atom::Bool] {
        let instance = graph.instantiate(scheme, 1, why).unwrap();
        let TypeNode::Arrow(signature) = graph.node(instance.ty).unwrap().clone() else { panic!() };
        let value = graph.atom(atom).unwrap(); let tag = graph.atom(Atom::Unit).unwrap();
        let row = graph.row(vec![field("tag", tag), field("name", value)], None).unwrap();
        let actual = graph.record(row).unwrap(); graph.assignable(signature.params[0].ty, actual, why).unwrap();
        assert_eq!(graph.resolved(signature.result).unwrap(), value);
        assert_eq!(graph.row_data(row).unwrap().fields.len(), 2);
    }
}

#[test]
fn width_assignment_is_not_row_equality_or_container_covariance() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap();
    let narrow_row = graph.row(vec![field("name", int)], None).unwrap(); let wide_row = graph.row(vec![field("name", int), field("tag", int)], None).unwrap();
    let narrow = graph.record(narrow_row).unwrap(); let wide = graph.record(wide_row).unwrap();
    graph.assignable(narrow, wide, why).unwrap(); assert!(graph.unify(narrow, wide, why).is_err());
    let narrow_list = graph.list(narrow).unwrap(); let wide_list = graph.list(wide).unwrap();
    assert!(graph.assignable(narrow_list, wide_list, why).is_err());
}

#[test]
fn row_lacks_kind_and_recursive_equations_reject_without_binding() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap();
    let tail = graph.fresh_row(1, span()).unwrap(); let row = graph.row(vec![field("name", int)], Some(tail)).unwrap();
    assert!(graph.unify(tail, int, why).is_err());
    let duplicate = graph.row(vec![field("name", int)], None).unwrap(); let duplicate = graph.row_type(duplicate).unwrap();
    assert!(matches!(graph.unify(tail, duplicate, why), Err(InferenceError::Lacks(_))));
    let recursive = graph.fresh(1, span()).unwrap(); let recursive_row = graph.row(vec![field("name", recursive)], None).unwrap(); let recursive_record = graph.record(recursive_row).unwrap();
    assert!(matches!(graph.unify(recursive, recursive_record, why), Err(InferenceError::Occurs { .. })));
    assert_eq!(graph.row_data(row).unwrap().fields.len(), 1);
}

#[test]
fn captured_and_expansive_bindings_do_not_generalize() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let captured = graph.fresh(2, span()).unwrap();
    let function = unary(&mut graph, captured, captured); graph.capture(captured, 0, why).unwrap();
    let scheme = graph.generalize(function, 0, Generalization::Allowed, &[]).unwrap(); assert!(graph.scheme(scheme).unwrap().quantifiers.is_empty());
    let variable = graph.fresh(2, span()).unwrap(); let function = unary(&mut graph, variable, variable);
    let scheme = graph.generalize(function, 0, Generalization::Monomorphic, &[]).unwrap(); assert!(graph.scheme(scheme).unwrap().quantifiers.is_empty());
}

#[test]
fn sealed_add_requirements_are_generalized_forwarded_and_discharged() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let left = graph.fresh(1, span()).unwrap(); let right = graph.fresh(1, span()).unwrap(); let result = graph.fresh(1, span()).unwrap();
    let requirement = graph.require_add(left, right, result, why).unwrap();
    let arrow = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern("left"), ty: left, defaulted: false, rest: false }, Parameter { label: Name::intern("right"), ty: right, defaulted: false, rest: false }], result, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
    let scheme = graph.generalize(arrow, 0, Generalization::Allowed, &[requirement]).unwrap();
    for (atom, expected) in [(Atom::Int, SealedOperation::AddInt), (Atom::Float, SealedOperation::AddFloat)] {
        let instance = graph.instantiate(scheme, 1, why).unwrap(); let TypeNode::Arrow(signature) = graph.node(instance.ty).unwrap().clone() else { panic!() };
        let actual = graph.atom(atom).unwrap(); graph.unify(signature.params[0].ty, actual, why).unwrap(); graph.unify(signature.params[1].ty, actual, why).unwrap(); graph.solve().unwrap();
        assert_eq!(graph.discharge(instance.requirements[0]).unwrap().unwrap().operation, expected);
        assert_eq!(graph.resolved(signature.result).unwrap(), actual);
    }
}

#[test]
fn failed_add_probe_does_not_poison_another_discharge() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let variable = graph.fresh(1, span()).unwrap(); let result = graph.fresh(1, span()).unwrap();
    let requirement = graph.require_add(variable, variable, result, why).unwrap(); let boolean = graph.atom(Atom::Bool).unwrap();
    assert!(graph.probe(|graph| { graph.unify(variable, boolean, why)?; graph.solve() }).is_err());
    let int = graph.atom(Atom::Int).unwrap(); graph.unify(variable, int, why).unwrap(); graph.solve().unwrap();
    assert_eq!(graph.discharge(requirement).unwrap().unwrap().operation, SealedOperation::AddInt);
}

#[test]
fn rigid_requirement_with_no_supported_domain_is_rejected() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let variable = graph.fresh(1, span()).unwrap();
    let scheme = graph.generalize(variable, 0, Generalization::Allowed, &[]).unwrap();
    let rigid = graph.scheme(scheme).unwrap().body; let boolean = graph.atom(Atom::Bool).unwrap();
    graph.require_add(rigid, boolean, rigid, why).unwrap();
    assert!(matches!(graph.solve(), Err(InferenceError::UnsupportedOperation(_))));
}

#[test]
fn fixed_effect_inclusions_propagate_and_failed_upper_bound_rolls_back() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let first = graph.fresh_effect(None).unwrap(); let second = graph.fresh_effect(Some(EffectSet::IO)).unwrap();
    graph.include_effects(EffectSummary::Variable(first), EffectSummary::Variable(second), why).unwrap();
    graph.include_effects(EffectSummary::Closed(EffectSet::ENV), EffectSummary::Variable(first), why).unwrap();
    assert_eq!(graph.effect_value(second).unwrap(), EffectSet::ENV);
    assert!(graph.include_effects(EffectSummary::Closed(EffectSet::TIME), EffectSummary::Variable(first), why).is_err());
    assert_eq!(graph.effect_value(first).unwrap(), EffectSet::ENV);
    assert_eq!(graph.effect_value(second).unwrap(), EffectSet::ENV);
}

#[test]
fn declaration_facts_retain_rigid_identity_and_publication_rejects_ground_escape() {
    let mut graph = InferenceContext::default();
    let variable = graph.fresh(1, span()).unwrap(); let original = unary(&mut graph, variable, variable);
    let scheme = graph.generalize(original, 0, Generalization::Allowed, &[]).unwrap();
    assert!(matches!(graph.node(graph.resolved(variable).unwrap()).unwrap(), TypeNode::Rigid { scope, .. } if *scope == scheme));
    assert!(graph.validate_scoped(ScopedRoot { ty: original, scope: None }).is_err());
    graph.validate_scoped(ScopedRoot { ty: original, scope: Some(scheme) }).unwrap();
    let graph = graph.freeze_scoped(&[ScopedRoot { ty: original, scope: Some(scheme) }]).unwrap();
    assert_eq!(graph.scheme(scheme).unwrap().quantifiers.len(), 1);
}

#[test]
fn audit_foreign_effect_is_rejected_before_unknown_erasure() {
    let mut other = InferenceContext::default(); let foreign = other.fresh_effect(None).unwrap();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    assert_eq!(graph.include_effects(EffectSummary::Variable(foreign), EffectSummary::Unknown, why), Err(InferenceError::ForeignHandle));
}

#[test]
fn audit_accumulated_lacks_obey_row_label_limit() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::new(Limits { row_labels: 1, ..Limits::default() });
    let tail = graph.fresh_row(1, span()).unwrap(); let int = graph.atom(Atom::Int).unwrap();
    graph.row(vec![field("a", int)], Some(tail)).unwrap();
    assert!(matches!(graph.row(vec![field("b", int)], Some(tail)), Err(InferenceError::Limit("row labels"))));
    assert_eq!(graph.variable(tail).unwrap().unwrap().lacks, &[Name::intern("a")]);
}

#[test]
fn audit_watcher_dispatch_is_charged_even_in_failed_probe() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let shared = graph.fresh(1, span()).unwrap();
    for _ in 0..32 {
        let right = graph.fresh(1, span()).unwrap(); let result = graph.fresh(1, span()).unwrap();
        graph.require_add(shared, right, result, why).unwrap();
    }
    graph.solve().unwrap(); let int = graph.atom(Atom::Int).unwrap();
    let work = graph.counters().work_units; let wakes = graph.counters().wakeups;
    graph.probe::<()>(|graph| { graph.unify(shared, int, why)?; Err(InferenceError::InvalidScheme) }).unwrap_err();
    assert!(graph.counters().work_units - work >= graph.counters().wakeups - wakes);
    assert_eq!(graph.resolved(shared).unwrap(), shared);
}

#[test]
fn audit_latent_effect_scheme_instances_are_independent() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap();
    let effects = graph.fresh_effect(None).unwrap();
    let callback = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Variable(effects) }).unwrap();
    let root = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern("callback"), ty: callback, defaulted: false, rest: false }], result: callback, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
    let scheme = graph.generalize(root, 0, Generalization::Allowed, &[]).unwrap();
    let first = graph.instantiate(scheme, 1, why).unwrap(); let second = graph.instantiate(scheme, 1, why).unwrap();
    let TypeNode::Arrow(first_arrow) = graph.node(first.ty).unwrap().clone() else { panic!() };
    let TypeNode::Arrow(second_arrow) = graph.node(second.ty).unwrap().clone() else { panic!() };
    let fs = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Closed(EffectSet::FS) }).unwrap();
    let net = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Closed(EffectSet::NET) }).unwrap();
    graph.unify(first_arrow.params[0].ty, fs, why).unwrap(); graph.unify(second_arrow.params[0].ty, net, why).unwrap();
}

#[test]
fn audit_unrelated_rigid_requirement_cannot_attach_to_ground_scheme() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let variable = graph.fresh(1, span()).unwrap(); let first = graph.generalize(variable, 0, Generalization::Allowed, &[]).unwrap();
    let rigid = graph.scheme(first).unwrap().body; let int = graph.atom(Atom::Int).unwrap();
    let requirement = graph.require_add(rigid, rigid, rigid, why).unwrap();
    assert!(matches!(graph.generalize(int, 0, Generalization::Allowed, &[requirement]), Err(InferenceError::DisconnectedRequirement(_))));
}

#[test]
fn publication_checks_requirement_scope_even_when_signature_is_ground() {
    let mut graph = InferenceContext::default(); let variable = graph.fresh(1, span()).unwrap();
    let first = graph.generalize(variable, 0, Generalization::Allowed, &[]).unwrap(); let rigid = graph.scheme(first).unwrap().body;
    let int = graph.atom(Atom::Int).unwrap(); let second = graph.generalize(int, 0, Generalization::Allowed, &[]).unwrap();
    graph.schemes[second.index()].value.requirements.push(RequirementTemplate::Add { left: rigid, right: rigid, result: rigid });
    assert!(matches!(graph.freeze_scoped(&[ScopedRoot { ty: int, scope: Some(second) }]), Err(InferenceError::ScopeEscape)));
}

#[test]
fn arrow_assignment_preserves_explicit_effect_upper_bound_and_kind() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap();
    let actual = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Closed(EffectSet::FS) }).unwrap();
    let expected = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Closed(EffectSet::IO) }).unwrap();
    graph.assignable(expected, actual, why).unwrap();
    assert!(graph.unify(expected, actual, why).is_err());
    let erased = graph.atom(Atom::Proc).unwrap(); graph.assignable(erased, actual, why).unwrap();
    let pure = graph.atom(Atom::Pure).unwrap(); assert!(graph.assignable(pure, actual, why).is_err());
}

#[test]
fn rigid_capture_cannot_lower_beyond_its_scope() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let variable = graph.fresh(1, span()).unwrap();
    let scheme = graph.generalize(variable, 0, Generalization::Allowed, &[]).unwrap(); let rigid = graph.scheme(scheme).unwrap().body;
    assert_eq!(graph.capture(rigid, 0, why), Err(InferenceError::ScopeEscape));
}

#[test]
fn effect_schemes_preserve_inclusions_and_monomorphic_alias_restriction() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap();
    let latent = graph.fresh_effect(None).unwrap(); let outward = graph.fresh_effect(None).unwrap();
    graph.include_effects(EffectSummary::Variable(latent), EffectSummary::Variable(outward), why).unwrap();
    let callback = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Variable(latent) }).unwrap();
    let root = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("callback"), ty: callback, defaulted: false, rest: false }], result: unit, effects: EffectSummary::Variable(outward) }).unwrap();
    let scheme = graph.generalize(root, 0, Generalization::Allowed, &[]).unwrap();
    assert_eq!(graph.scheme(scheme).unwrap().effect_quantifiers.len(), 2);
    assert_eq!(graph.scheme(scheme).unwrap().effect_inclusions.len(), 1);
    for bits in [EffectSet::FS, EffectSet::NET] {
        let instance = graph.instantiate(scheme, 0, why).unwrap(); let TypeNode::Arrow(arrow) = graph.node(instance.ty).unwrap().clone() else { panic!() };
        let actual = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Closed(bits) }).unwrap();
        graph.unify(arrow.params[0].ty, actual, why).unwrap();
        let EffectSummary::Variable(id) = arrow.effects else { panic!() };
        assert_eq!(graph.effect_value(id).unwrap(), bits);
    }
    let effect = graph.fresh_effect(None).unwrap(); let callback = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Variable(effect) }).unwrap();
    let monomorphic = graph.generalize(callback, 0, Generalization::Monomorphic, &[]).unwrap();
    let alias = graph.generalize(graph.scheme(monomorphic).unwrap().body, 0, Generalization::Allowed, &[]).unwrap();
    assert!(graph.scheme(alias).unwrap().effect_quantifiers.is_empty());
}

#[test]
fn bounded_views_preserve_ground_contracts_and_refuse_unresolved_erasure() {
    use crate::sema::types::Type;
    let mut graph = InferenceContext::default();
    let original = Type::Record(std::collections::BTreeMap::from([(Name::intern("name"), Type::Str), (Name::intern("items"), Type::List(Box::new(Type::Optional(Box::new(Type::UInt)))))]));
    let ty = graph.import_type(&original, 0, span()).unwrap();
    assert_eq!(graph.export_type(ty).unwrap(), original);
    assert_eq!(graph.import_type(&Type::Graph(ty), 0, span()).unwrap(), ty);
    let variable = graph.fresh(1, span()).unwrap(); assert_eq!(graph.export_type(variable), Err(InferenceError::Unresolved(variable)));
    let poisoned = graph.import_type(&Type::Unknown, 0, span()).unwrap(); assert!(matches!(graph.export_type(poisoned), Err(InferenceError::Recovery(_))));
}

#[test]
fn constraint_reasons_survive_success_and_rewind_failed_trials() {
    let mut graph = InferenceContext::default(); let first = reason(&mut graph);
    let second = graph.reason(span(), Some(first)).unwrap();
    let variable = graph.fresh(1, span()).unwrap(); let int = graph.atom(Atom::Int).unwrap();
    graph.unify(variable, int, first).unwrap();
    graph.assignable(int, variable, second).unwrap();
    assert_eq!(graph.constraint_origins().len(), 2);
    assert_eq!(graph.constraint_origins()[0].relation, ConstraintRelation::Equality { left: variable, right: int });
    assert_eq!(graph.constraint_origins()[0].reason, first);
    assert_eq!(graph.constraint_origins()[1].reason, second);
    let before = graph.counters().attempted_constraints;
    graph.probe::<()>(|graph| { graph.unify(int, int, second)?; Err(InferenceError::InvalidScheme) }).unwrap_err();
    assert_eq!(graph.constraint_origins().len(), 2);
    assert_eq!(graph.counters().attempted_constraints, before + 1);
}

#[test]
fn reason_contribution_edges_are_bounded_and_charged_on_failed_trials() {
    let mut graph = InferenceContext::new(Limits { reason_edges: 2, ..Limits::default() });
    let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap();
    graph.unify(int, int, why).unwrap();
    assert_eq!(graph.unify(int, int, why), Err(InferenceError::Limit("reason edges")));
    assert_eq!(graph.constraint_origins().len(), 1);
    assert_eq!(graph.counters().attempted_reason_edges, 2);
}

#[test]
fn work_exhaustion_preserves_pending_requirement_on_solve_entry() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let left = graph.fresh(1, span()).unwrap(); let right = graph.fresh(1, span()).unwrap(); let output = graph.fresh(1, span()).unwrap();
    let requirement = graph.require_add(left, right, output, why).unwrap();
    graph.limits.work_units = graph.counters().work_units;
    assert_eq!(graph.solve(), Err(InferenceError::Limit("solver work")));
    assert_eq!(graph.queue.front(), Some(&requirement));
    assert!(graph.requirement(requirement).unwrap().queued);
}

#[test]
fn work_charges_wide_structural_copies_before_a_failed_match() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap();
    let fields = (0..64).map(|index| ModuleField { label: Name::intern(&format!("field_{index}")), ty: int, optional: false }).collect();
    let module = graph.module(fields).unwrap(); let before = graph.counters().work_units;
    assert!(matches!(graph.unify(module, int, why), Err(InferenceError::TypeMismatch { .. })));
    assert!(graph.counters().work_units - before >= 64);
}
