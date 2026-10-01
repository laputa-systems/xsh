use super::*;
use crate::source::SourceId;

fn span() -> Span { Span::new(SourceId::new(0), 0, 1) }
fn reason(graph: &mut InferenceContext) -> ReasonId { graph.reason(span(), None).unwrap() }
fn field(label: &str, ty: TypeId) -> RowField { RowField { label: Name::intern(label), ty } }
fn unary(graph: &mut InferenceContext, parameter: TypeId, result: TypeId) -> TypeId {
    graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern("value"), ty: parameter, defaulted: false, rest: false }], result, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap()
}

#[test]
fn nullable_constructor_is_idempotent_without_allocating_another_wrapper() {
    let mut graph=InferenceContext::default();let int=graph.atom(Atom::Int).unwrap();
    let optional=graph.optional(int).unwrap();let count=graph.counters().attempted_nodes;
    assert_eq!(graph.optional(optional).unwrap(),optional);
    assert_eq!(graph.counters().attempted_nodes,count);
}

#[test]
fn nullable_rank_one_result_flattens_a_later_optional_argument_without_training_its_binder() {
    let symbols=crate::symbol::SymbolOwner::new();let _guard=symbols.enter();
    let mut graph=InferenceContext::default();let why=reason(&mut graph);let parameter=graph.fresh(1,span()).unwrap();
    let result=graph.optional(parameter).unwrap();let signature=unary(&mut graph,parameter,result);
    let scheme=graph.generalize(signature,0,Generalization::Allowed,&[]).unwrap();assert_eq!(graph.scheme(scheme).unwrap().quantifiers.len(),1);
    let int=graph.atom(Atom::Int).unwrap();let optional_int=graph.optional(int).unwrap();let string=graph.atom(Atom::Str).unwrap();
    for actual in [optional_int,string] {
        let instance=graph.instantiate(scheme,1,why).unwrap();let TypeNode::Arrow(arrow)=graph.clone_node(instance.ty).unwrap() else {panic!()};
        graph.unify(arrow.params[0].ty,actual,why).unwrap();
        let expected=if actual==optional_int {optional_int}else{graph.optional(string).unwrap()};
        assert_eq!(graph.export_type(arrow.result).unwrap(),graph.export_type(expected).unwrap());
        graph.assignable(expected,arrow.result,why).unwrap();assert!(graph.same_published_type(expected,arrow.result).unwrap());
        graph.validate_scheme_instance(scheme,instance.ty,&instance.substitutions,&instance.effect_substitutions,&instance.effect_roots).unwrap();
    }
    assert_eq!(graph.scheme(scheme).unwrap().quantifiers.len(),1);
}

#[test]
fn nullable_late_binding_normalization_obeys_shared_work_and_structural_depth_limits() {
    for work_limit in [false,true] {
        let mut graph=InferenceContext::default();let why=reason(&mut graph);let int=graph.atom(Atom::Int).unwrap();let expected=graph.optional(int).unwrap();let mut actual=expected;
        for _ in 0..6 {let payload=graph.fresh(0,span()).unwrap();let wrapper=graph.optional(payload).unwrap();graph.unify(payload,actual,why).unwrap();actual=wrapper;}
        if work_limit {graph.limits.work_units=graph.counters().work_units+3;}else{graph.limits.structural_depth=2;}
        let before=graph.counters().work_units;let origins=graph.origins.len();
        assert!(matches!(graph.assignable(expected,actual,why),Err(InferenceError::Limit(_))));
        assert_eq!(graph.origins.len(),origins);assert!(graph.counters().work_units>before);
        if !work_limit {assert!(matches!(graph.same_published_type(expected,actual),Err(InferenceError::Limit(_))));assert!(matches!(graph.export_type(actual),Err(InferenceError::Limit(_))));}
    }
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
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let variable = graph.fresh(1, span()).unwrap();
    let first = graph.generalize(variable, 0, Generalization::Allowed, &[]).unwrap(); let rigid = graph.scheme(first).unwrap().body;
    let int = graph.atom(Atom::Int).unwrap(); let second = graph.generalize(int, 0, Generalization::Allowed, &[]).unwrap();
    graph.schemes[second.index()].value.requirements.push(RequirementTemplate::Add { left: rigid, right: rigid, result: rigid });
    let source = graph.require_add(rigid, rigid, rigid, why).unwrap(); graph.schemes[second.index()].value.requirement_origins.push(source);
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

#[test]
fn shared_import_reuses_legacy_identity_inside_nested_views() {
    use crate::sema::types::Type;
    let mut legacy = crate::sema::constraints::TypeConstraints::default();
    let variable = legacy.fresh(span()); let other = legacy.fresh(span());
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let first = graph.import_type_shared(&Type::List(Box::new(variable.clone())), 2, span()).unwrap();
    let second = graph.import_type_shared(&Type::Optional(Box::new(variable.clone())), 1, span()).unwrap();
    let TypeNode::List(first_item) = *graph.node(first).unwrap() else { panic!() };
    let TypeNode::Optional(second_item) = *graph.node(second).unwrap() else { panic!() };
    assert_eq!(first_item, second_item);
    assert_eq!(graph.variable(first_item).unwrap().unwrap().level, 1);
    let other = graph.import_type_shared(&other, 1, span()).unwrap();
    assert_ne!(first_item, other);
    let int = graph.atom(Atom::Int).unwrap(); graph.unify(first_item, int, why).unwrap();
    assert_eq!(graph.export_type(second).unwrap(), Type::Optional(Box::new(Type::Int)));
    assert!(graph.import_type(&variable, 1, span()).is_err());
}

#[test]
fn shared_import_rolls_back_identities_and_keeps_dynamic_and_recovery_distinct() {
    use crate::sema::types::Type;
    let mut legacy = crate::sema::constraints::TypeConstraints::default(); let variable = legacy.fresh(span());
    let mut graph = InferenceContext::default(); let mut abandoned = None;
    graph.probe::<()>(|graph| { abandoned = Some(graph.import_type_shared(&variable, 1, span())?); Err(InferenceError::InvalidScheme) }).unwrap_err();
    let live = graph.import_type_shared(&variable, 1, span()).unwrap();
    assert_ne!(live, abandoned.unwrap()); assert!(graph.node(abandoned.unwrap()).is_err());
    let dynamic = graph.import_type_shared(&Type::Any, 1, span()).unwrap();
    let recovery = graph.import_type_shared(&Type::Unknown, 1, span()).unwrap();
    assert!(matches!(graph.node(dynamic).unwrap(), TypeNode::Atom(Atom::Any)));
    assert!(matches!(graph.node(recovery).unwrap(), TypeNode::Poison));
    assert!(graph.variable(live).unwrap().is_some());
    assert!(matches!(graph.freeze_scoped(&[ScopedRoot { ty: live, scope: None }]), Err(InferenceError::Unresolved(_))));
}

#[test]
fn recursive_component_keeps_common_binders_and_fresh_member_instances() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let variable = graph.fresh(1, span()).unwrap();
    let first = unary(&mut graph, variable, variable); let second = unary(&mut graph, variable, variable);
    let schemes = graph.generalize_component(&[
        ComponentMember { root: first, requirements: vec![], policy: Generalization::Allowed },
        ComponentMember { root: second, requirements: vec![], policy: Generalization::Allowed },
    ], 0, None).unwrap();
    assert_eq!(graph.canonical_scheme_scope(schemes[0]).unwrap(), graph.canonical_scheme_scope(schemes[1]).unwrap());
    let first_binder = graph.scheme_type_binders(schemes[0]).unwrap(); let second_binder = graph.scheme_type_binders(schemes[1]).unwrap();
    assert_eq!(first_binder, second_binder); assert_eq!(first_binder.len(), 1);
    assert_eq!(graph.scheme_binder_index(schemes[1], first_binder[0]).unwrap(), Some(0));
    for (scheme, atom) in schemes.iter().zip([Atom::Int, Atom::Str]) {
        let instance = graph.instantiate(*scheme, 0, why).unwrap(); let value = graph.atom(atom).unwrap();
        graph.unify(instance.substitutions[0], value, why).unwrap();
        let TypeNode::Arrow(arrow) = graph.node(instance.ty).unwrap() else { panic!() };
        assert_eq!(graph.resolved(arrow.result).unwrap(), value);
    }
    graph.freeze_scoped(&[ScopedRoot { ty: first, scope: Some(schemes[0]) }, ScopedRoot { ty: second, scope: Some(schemes[1]) }]).unwrap();
}

#[test]
fn component_value_restriction_lowers_shared_variables_before_any_generalization() {
    let mut graph = InferenceContext::default(); let variable = graph.fresh(1, span()).unwrap();
    let first = unary(&mut graph, variable, variable); let second = unary(&mut graph, variable, variable);
    let schemes = graph.generalize_component(&[
        ComponentMember { root: first, requirements: vec![], policy: Generalization::Allowed },
        ComponentMember { root: second, requirements: vec![], policy: Generalization::Monomorphic },
    ], 0, None).unwrap();
    assert!(schemes.iter().all(|scheme| graph.scheme(*scheme).unwrap().quantifiers.is_empty()));
    assert_eq!(graph.variable(variable).unwrap().unwrap().level, 0);
}

#[test]
fn component_member_scope_rejects_sibling_only_binders_and_maps_sparse_indices() {
    let mut graph = InferenceContext::default(); let first_var = graph.fresh(1, span()).unwrap(); let second_var = graph.fresh(1, span()).unwrap();
    let first = unary(&mut graph, first_var, first_var); let second = unary(&mut graph, second_var, second_var);
    let schemes = graph.generalize_component(&[
        ComponentMember { root: first, requirements: vec![], policy: Generalization::Allowed },
        ComponentMember { root: second, requirements: vec![], policy: Generalization::Allowed },
    ], 0, None).unwrap();
    let binder = graph.scheme_type_binders(schemes[1]).unwrap()[0];
    assert!(matches!(graph.node(binder).unwrap(), TypeNode::Rigid { index: 1, .. }));
    assert_eq!(graph.scheme_binder_index(schemes[1], binder).unwrap(), Some(0));
    assert_eq!(graph.scheme_binder_index(schemes[0], binder).unwrap(), None);
    assert_eq!(graph.validate_scoped(ScopedRoot { ty: first, scope: Some(schemes[1]) }), Err(InferenceError::ScopeEscape));
}

#[test]
fn component_lexical_capture_requires_exact_authorized_outer_bindings() {
    let mut graph = InferenceContext::default(); let outer_variable = graph.fresh(1, span()).unwrap();
    let outer = unary(&mut graph, outer_variable, outer_variable); let outer_scheme = graph.generalize(outer, 0, Generalization::Allowed, &[]).unwrap();
    let outer_binder = graph.scheme_type_binders(outer_scheme).unwrap()[0];
    let inner_variable = graph.fresh(2, span()).unwrap(); let inner = unary(&mut graph, inner_variable, outer_binder);
    let member = ComponentMember { root: inner, requirements: vec![], policy: Generalization::Allowed };
    assert_eq!(graph.generalize_component(&[member.clone()], 1, None), Err(InferenceError::ScopeEscape));
    let scheme = graph.generalize_component(&[member], 1, Some(outer_scheme)).unwrap()[0];
    graph.freeze_scoped(&[ScopedRoot { ty: inner, scope: Some(scheme) }, ScopedRoot { ty: outer, scope: Some(outer_scheme) }]).unwrap();
}

#[test]
fn component_records_captured_meta_identity_before_enclosing_generalization() {
    let mut graph = InferenceContext::default(); let outer_variable = graph.fresh(1, span()).unwrap();
    let outer = unary(&mut graph, outer_variable, outer_variable);
    let inner_variable = graph.fresh(2, span()).unwrap(); let inner = unary(&mut graph, inner_variable, outer_variable);
    let scheme = graph.generalize_component(&[ComponentMember { root: inner, requirements: vec![], policy: Generalization::Allowed }], 1, None).unwrap()[0];
    assert!(graph.scheme(scheme).unwrap().captures.contains(&outer_variable));
    let outer_scheme = graph.generalize(outer, 0, Generalization::Allowed, &[]).unwrap();
    graph.freeze_scoped(&[ScopedRoot { ty: inner, scope: Some(scheme) }, ScopedRoot { ty: outer, scope: Some(outer_scheme) }]).unwrap();
}

#[test]
fn candidate_trial_rewinds_success_and_retires_its_allocations() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let variable = graph.fresh(1, span()).unwrap(); let int = graph.atom(Atom::Int).unwrap();
    let work = graph.counters().work_units;
    let trial = graph.trial(|graph| { graph.unify(variable, int, why)?; graph.fresh(1, span()) }).unwrap();
    assert_eq!(graph.resolved(variable).unwrap(), variable);
    assert!(graph.node(trial).is_err()); assert!(graph.constraint_origins().is_empty());
    assert!(graph.counters().work_units > work);
    let committed = graph.fresh(1, span()).unwrap(); assert_ne!(trial, committed);
}

#[test]
fn selected_json_candidate_exposes_no_certificate_before_actual_predicate_solves() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let any = graph.atom(Atom::Any).unwrap(); let string = graph.atom(Atom::Str).unwrap();
    let signature = unary(&mut graph, any, string); let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap();
    let candidate = graph.register_candidate(CandidateTemplate { failure_projection: None, identity: Name::intern("json-certificate-test"), public_label: Name::intern("json-certificate-test"), effect_roles: vec![], output_effect_roles: vec![], scheme, has_receiver: false, actual_eligibility: vec![(0, Eligibility::JsonCompatible)], argument_relations: vec![] }).unwrap();
    let family = graph.register_family(&[candidate]).unwrap(); let actual = graph.fresh(0, span()).unwrap(); let output = graph.fresh(0, span()).unwrap();
    let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings: vec![], effect_bindings: vec![], receiver: None, arguments: vec![Some(actual)], result: output, effects: EffectSummary::Closed(EffectSet::EMPTY) }, why).unwrap();
    graph.solve().unwrap(); assert!(graph.candidate_evidence(requirement).unwrap().is_none());
    let path = graph.atom(Atom::Path).unwrap();
    graph.probe::<()>(|graph| { graph.unify(actual, path, why)?; graph.solve() }).unwrap_err();
    assert_eq!(graph.resolved(actual).unwrap(), actual);
    graph.unify(actual, string, why).unwrap(); graph.solve().unwrap();
    assert_eq!(graph.candidate_evidence(requirement).unwrap().unwrap().candidate, candidate);
}

#[test]
fn schemes_publish_only_residual_requirements_and_keep_ground_certificates() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let int = graph.atom(Atom::Int).unwrap(); let ground = graph.require_add(int, int, int, why).unwrap();
    let root = unary(&mut graph, int, int);
    let scheme = graph.generalize(root, 0, Generalization::Allowed, &[ground]).unwrap();
    assert!(graph.scheme(scheme).unwrap().requirements.is_empty());
    assert!(graph.discharge(ground).unwrap().is_some());
    let variable = graph.fresh(1, span()).unwrap(); let generic = graph.require_add(variable, variable, variable, why).unwrap();
    let root = unary(&mut graph, variable, variable);
    let scheme = graph.generalize(root, 0, Generalization::Allowed, &[generic]).unwrap();
    assert_eq!(graph.scheme(scheme).unwrap().requirements.len(), 1);
    let left = graph.fresh(1, span()).unwrap(); let right = graph.fresh(1, span()).unwrap();
    let a = unary(&mut graph, left, left); let b = unary(&mut graph, right, right);
    let schemes = graph.generalize_component(&[ComponentMember { root: a, requirements: vec![ground], policy: Generalization::Allowed }, ComponentMember { root: b, requirements: vec![ground], policy: Generalization::Allowed }], 0, None).unwrap();
    assert!(schemes.iter().all(|scheme| graph.scheme(*scheme).unwrap().requirements.is_empty()));
}

#[test]
fn abstract_operation_keeps_finite_family_without_ground_certificate() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let int = graph.atom(Atom::Int).unwrap(); let string = graph.atom(Atom::Str).unwrap();
    let mut candidates = Vec::new();
    for (label, input) in [("int-family", int), ("str-family", string)] {
        let signature = unary(&mut graph, input, int); let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap();
        candidates.push(graph.register_candidate(CandidateTemplate { failure_projection: None, identity: Name::intern(label), public_label: Name::intern(label), effect_roles: vec![], output_effect_roles: vec![], scheme, has_receiver: false, actual_eligibility: vec![], argument_relations: vec![ArgumentRelation::Exact] }).unwrap());
    }
    let family = graph.register_family(&candidates).unwrap(); let argument = graph.fresh(1, span()).unwrap();
    let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings: vec![], effect_bindings: vec![], receiver: None, arguments: vec![Some(argument)], result: int, effects: EffectSummary::Closed(EffectSet::EMPTY) }, why).unwrap();
    let root = unary(&mut graph, argument, int); let scheme = graph.generalize(root, 0, Generalization::Allowed, &[requirement]).unwrap();
    assert_eq!(graph.scheme(scheme).unwrap().requirements.len(), 1);
    graph.solve().unwrap(); assert!(graph.candidate_evidence(requirement).unwrap().is_none());
    let instance = graph.instantiate(scheme, 0, why).unwrap(); let TypeNode::Arrow(signature) = graph.node(instance.ty).unwrap().clone() else { panic!() };
    graph.unify(signature.params[0].ty, string, why).unwrap(); graph.solve().unwrap();
    assert_eq!(graph.candidate_evidence(instance.requirements[0]).unwrap().unwrap().candidate, candidates[1]);
}

#[test]
fn stage_eligibility_preserves_each_checked_operand_domain() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    for (predicate, allowed, denied) in [
        (Eligibility::NonUnit, Atom::Bool, Atom::Unit),
        (Eligibility::Sortable, Atom::Path, Atom::UInt),
        (Eligibility::SortableKey, Atom::Any, Atom::Float),
        (Eligibility::ArgvItem, Atom::Duration, Atom::Bytes),
        (Eligibility::CountKey, Atom::UInt, Atom::Path),
    ] {
        let accepted = graph.atom(allowed).unwrap(); let requirement = graph.require_eligibility(predicate, accepted, why).unwrap(); graph.solve().unwrap(); assert!(graph.eligibility_satisfied(requirement).unwrap());
        let rejected = graph.atom(denied).unwrap(); assert!(graph.probe(|graph| { graph.require_eligibility(predicate, rejected, why)?; graph.solve() }).is_err());
    }
    let unit = graph.atom(Atom::Unit).unwrap(); let error = graph.atom(Atom::Error).unwrap(); let nested = graph.result(unit, error).unwrap();
    let requirement = graph.require_eligibility(Eligibility::NonUnit, nested, why).unwrap(); graph.solve().unwrap(); assert!(graph.eligibility_satisfied(requirement).unwrap());
}

#[test]
fn standalone_effect_roots_generalize_and_instantiate_independently() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap();
    let role = graph.fresh_effect_at(1, None).unwrap(); let root = unary(&mut graph, unit, unit);
    let scheme = graph.generalize_with_effect_roots(root, 0, Generalization::Allowed, &[], &[EffectSummary::Variable(role)]).unwrap();
    assert_eq!(graph.scheme(scheme).unwrap().effect_quantifiers.len(), 1);
    assert!(matches!(graph.scheme(scheme).unwrap().effect_roots[0], EffectSummary::Rigid { .. }));
    let a = graph.instantiate(scheme, 0, why).unwrap(); let b = graph.instantiate(scheme, 0, why).unwrap();
    graph.include_effects(EffectSummary::Closed(EffectSet::FS), EffectSummary::Variable(a.effect_substitutions[0]), why).unwrap();
    graph.include_effects(EffectSummary::Closed(EffectSet::NET), EffectSummary::Variable(b.effect_substitutions[0]), why).unwrap();
    assert_eq!(graph.closed_effect_summary(EffectSummary::Variable(a.effect_substitutions[0])).unwrap(), EffectSummary::Closed(EffectSet::FS));
    assert_eq!(graph.closed_effect_summary(EffectSummary::Variable(b.effect_substitutions[0])).unwrap(), EffectSummary::Closed(EffectSet::NET));
}

#[test]
fn component_schemes_retain_reachable_sibling_obligations_and_hidden_outputs() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let input = graph.fresh(1, span()).unwrap(); let output = graph.fresh(1, span()).unwrap(); let int = graph.atom(Atom::Int).unwrap();
    let first = unary(&mut graph, input, int); let second = unary(&mut graph, input, output);
    let requirement = graph.require_add(input, input, output, why).unwrap();
    let schemes = graph.generalize_component(&[ComponentMember { root: first, requirements: vec![], policy: Generalization::Allowed }, ComponentMember { root: second, requirements: vec![requirement], policy: Generalization::Allowed }], 0, None).unwrap();
    assert_eq!(graph.scheme(schemes[0]).unwrap().requirements.len(), 1);
    assert_eq!(graph.scheme(schemes[0]).unwrap().quantifiers.len(), 2);
    let boolean = graph.atom(Atom::Bool).unwrap();
    assert!(graph.probe(|graph| { let call = graph.instantiate(schemes[0], 0, why)?; let TypeNode::Arrow(signature) = graph.node(call.ty)?.clone() else { panic!() }; graph.unify(signature.params[0].ty, boolean, why)?; graph.solve() }).is_err());
}

#[test]
fn equality_candidate_keeps_independent_operands_until_compatibility_is_known() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let binder = graph.fresh(1, span()).unwrap(); let boolean = graph.atom(Atom::Bool).unwrap();
    let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern("left"), ty: binder, defaulted: false, rest: false }, Parameter { label: Name::intern("right"), ty: binder, defaulted: false, rest: false }], result: boolean, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
    let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap();
    let candidate = graph.register_candidate(CandidateTemplate { failure_projection: None, identity: Name::intern("equality-candidate-test"), public_label: Name::intern("equality"), effect_roles: vec![], output_effect_roles: vec![], scheme, has_receiver: false, actual_eligibility: vec![], argument_relations: vec![ArgumentRelation::Exact, ArgumentRelation::EqualityCompatible] }).unwrap();
    let family = graph.register_family(&[candidate]).unwrap(); let left = graph.fresh(0, span()).unwrap(); let right = graph.fresh(0, span()).unwrap();
    let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings: vec![], effect_bindings: vec![], receiver: None, arguments: vec![Some(left), Some(right)], result: boolean, effects: EffectSummary::Closed(EffectSet::EMPTY) }, why).unwrap();
    graph.solve().unwrap(); assert_ne!(graph.resolved(left).unwrap(), graph.resolved(right).unwrap()); assert!(graph.candidate_evidence(requirement).unwrap().is_none());
    let string = graph.atom(Atom::Str).unwrap(); let optional = graph.optional(string).unwrap();
    graph.unify(left, optional, why).unwrap(); graph.unify(right, string, why).unwrap(); graph.solve().unwrap();
    assert!(graph.candidate_evidence(requirement).unwrap().is_some());
}

#[test]
fn ground_operation_certificate_separates_required_effects_from_caller_budget() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap();
    let signature = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: int, effects: EffectSummary::Closed(EffectSet::TIME) }).unwrap();
    let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap();
    let candidate = graph.register_candidate(CandidateTemplate { failure_projection: None, identity: Name::intern("ground-time-test"), public_label: Name::intern("time.now"), effect_roles: vec![], output_effect_roles: vec![], scheme, has_receiver: false, actual_eligibility: vec![], argument_relations: vec![] }).unwrap();
    let family = graph.register_family(&[candidate]).unwrap(); let budget = graph.fresh_effect_at(1, None).unwrap();
    let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings: vec![], effect_bindings: vec![], receiver: None, arguments: vec![], result: int, effects: EffectSummary::Variable(budget) }, why).unwrap();
    graph.solve().unwrap(); assert_eq!(graph.candidate_evidence(requirement).unwrap().unwrap().effects, EffectSummary::Closed(EffectSet::TIME));
    let root = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: int, effects: EffectSummary::Variable(budget) }).unwrap();
    let scheme = graph.generalize(root, 0, Generalization::Allowed, &[requirement]).unwrap();
    assert!(graph.scheme(scheme).unwrap().requirements.is_empty()); assert!(graph.scheme(scheme).unwrap().effect_roots.is_empty());
}

#[test]
fn operation_candidates_wake_when_caller_effect_budget_is_narrowed() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap(); let mut candidates = Vec::new();
    for (label, effects) in [("empty-effect-test", EffectSet::EMPTY), ("fs-effect-test", EffectSet::FS)] {
        let signature = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: int, effects: EffectSummary::Closed(effects) }).unwrap(); let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap();
        candidates.push(graph.register_candidate(CandidateTemplate { failure_projection: None, identity: Name::intern(label), public_label: Name::intern(label), effect_roles: vec![], output_effect_roles: vec![], scheme, has_receiver: false, actual_eligibility: vec![], argument_relations: vec![] }).unwrap());
    }
    let family = graph.register_family(&candidates).unwrap(); let budget = graph.fresh_effect_at(0, None).unwrap();
    let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![], result: int, effects: EffectSummary::Variable(budget), effect_bindings: vec![], output_effect_bindings: vec![] }, why).unwrap();
    graph.solve().unwrap(); assert!(graph.candidate_evidence(requirement).unwrap().is_none());
    graph.include_effects(EffectSummary::Variable(budget), EffectSummary::Closed(EffectSet::EMPTY), why).unwrap(); graph.solve().unwrap();
    assert_eq!(graph.candidate_evidence(requirement).unwrap().unwrap().candidate, candidates[0]);
}

#[test]
fn opaque_effects_propagate_through_execution_budgets_and_reject_finite_bounds() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let source = graph.fresh_effect_at(0, None).unwrap(); let caller = graph.fresh_effect_at(0, None).unwrap();
    graph.include_effects(EffectSummary::Variable(source), EffectSummary::Variable(caller), why).unwrap();
    graph.include_effects(EffectSummary::Unknown, EffectSummary::Variable(source), why).unwrap();
    assert_eq!(graph.resolved_effect_summary(EffectSummary::Variable(source)).unwrap(), EffectSummary::Unknown);
    assert_eq!(graph.resolved_effect_summary(EffectSummary::Variable(caller)).unwrap(), EffectSummary::Unknown);
    let bounded = graph.fresh_effect_at(0, Some(EffectSet::EMPTY)).unwrap();
    assert!(graph.include_effects(EffectSummary::Unknown, EffectSummary::Variable(bounded), why).is_err());
    assert_eq!(graph.effect_value(bounded).unwrap(), EffectSet::EMPTY);
    let opaque = graph.fresh_effect_at(0, None).unwrap(); graph.include_effects(EffectSummary::Variable(opaque), EffectSummary::Variable(bounded), why).unwrap();
    assert!(graph.include_effects(EffectSummary::Unknown, EffectSummary::Variable(opaque), why).is_err());
    assert_eq!(graph.resolved_effect_summary(EffectSummary::Variable(opaque)).unwrap(), EffectSummary::Variable(opaque));
}

#[test]
fn derived_effects_seal_only_without_latent_input_relationships() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap();
    let creation = graph.fresh_derived_effect_at(1, None).unwrap(); graph.include_effects(EffectSummary::Closed(EffectSet::TIME), EffectSummary::Variable(creation), why).unwrap();
    let root = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Variable(creation) }).unwrap();
    let scheme = graph.generalize(root, 0, Generalization::Allowed, &[]).unwrap();
    assert!(graph.scheme(scheme).unwrap().effect_quantifiers.is_empty());
    let TypeNode::Arrow(signature) = graph.node(graph.scheme(scheme).unwrap().body).unwrap() else { panic!() };
    assert_eq!(signature.effects, EffectSummary::Closed(EffectSet::TIME));
    let latent = graph.fresh_effect_at(1, None).unwrap(); let output = graph.fresh_derived_effect_at(1, None).unwrap();
    graph.include_effects(EffectSummary::Variable(latent), EffectSummary::Variable(output), why).unwrap();
    let callback = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Variable(latent) }).unwrap();
    let root = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("run"), ty: callback, defaulted: false, rest: false }], result: unit, effects: EffectSummary::Variable(output) }).unwrap();
    let scheme = graph.generalize(root, 0, Generalization::Allowed, &[]).unwrap(); assert_eq!(graph.scheme(scheme).unwrap().effect_quantifiers.len(), 2);
    let first = graph.instantiate(scheme, 0, why).unwrap(); let second = graph.instantiate(scheme, 0, why).unwrap();
    for (instance, effects) in [(&first, EffectSet::TIME), (&second, EffectSet::ENV)] {
        let TypeNode::Arrow(signature) = graph.node(instance.ty).unwrap().clone() else { panic!() }; let TypeNode::Arrow(callback) = graph.node(signature.params[0].ty).unwrap().clone() else { panic!() };
        graph.include_effects(EffectSummary::Closed(effects), callback.effects, why).unwrap();
        assert_eq!(graph.closed_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(effects));
    }
    let TypeNode::Arrow(original) = graph.node(graph.scheme(scheme).unwrap().body).unwrap() else { panic!() }; assert!(matches!(original.effects, EffectSummary::Rigid { .. }));
}

#[test]
fn generalization_rejects_a_captured_budget_depending_on_new_latent_parameters() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap();
    let latent = graph.fresh_effect_at(1, None).unwrap(); let captured = graph.fresh_execution_effect(None).unwrap();
    graph.include_effects(EffectSummary::Variable(latent), EffectSummary::Variable(captured), why).unwrap();
    let callback = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Variable(latent) }).unwrap();
    let root = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("run"), ty: callback, defaulted: false, rest: false }], result: unit, effects: EffectSummary::Variable(captured) }).unwrap();
    assert!(matches!(graph.generalize(root, 0, Generalization::Allowed, &[]), Err(InferenceError::ScopeEscape)));
    assert!(matches!(graph.generalize_component(&[ComponentMember { root, requirements: vec![], policy: Generalization::Allowed }], 0, None), Err(InferenceError::ScopeEscape)));
    assert_eq!(graph.resolved_effect_summary(EffectSummary::Variable(latent)).unwrap(), EffectSummary::Variable(latent));
}

#[test]
fn derived_operation_effects_wait_for_type_dependent_candidate_selection() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap(); let int = graph.atom(Atom::Int).unwrap(); let string = graph.atom(Atom::Str).unwrap(); let mut candidates = Vec::new();
    for (label, ty, effects) in [("int-empty-choice", int, EffectSet::EMPTY), ("str-fs-choice", string, EffectSet::FS)] {
        let signature = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("value"), ty, defaulted: false, rest: false }], result: unit, effects: EffectSummary::Closed(effects) }).unwrap(); let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap();
        candidates.push(graph.register_candidate(CandidateTemplate { failure_projection: None, identity: Name::intern(label), public_label: Name::intern(label), effect_roles: vec![], output_effect_roles: vec![], scheme, has_receiver: false, actual_eligibility: vec![], argument_relations: vec![ArgumentRelation::Exact] }).unwrap());
    }
    let family = graph.register_family(&candidates).unwrap(); let argument = graph.fresh(1, span()).unwrap(); let effects = graph.fresh_derived_effect_at(1, None).unwrap();
    let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![Some(argument)], result: unit, effects: EffectSummary::Variable(effects), effect_bindings: vec![], output_effect_bindings: vec![] }, why).unwrap();
    let root = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("value"), ty: argument, defaulted: false, rest: false }], result: unit, effects: EffectSummary::Variable(effects) }).unwrap();
    let scheme = graph.generalize(root, 0, Generalization::Allowed, &[requirement]).unwrap();
    assert_eq!(graph.scheme(scheme).unwrap().effect_quantifiers.len(), 1);
    let call = graph.instantiate(scheme, 0, why).unwrap(); let TypeNode::Arrow(signature) = graph.node(call.ty).unwrap().clone() else { panic!() };
    graph.unify(signature.params[0].ty, string, why).unwrap(); graph.solve().unwrap();
    assert_eq!(graph.closed_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(EffectSet::FS));
    assert_eq!(graph.candidate_evidence(call.requirements[0]).unwrap().unwrap().candidate, candidates[1]);
}

#[test]
fn captured_rigid_effects_form_calculated_unions_without_equality_aliases() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap();
    let latent = graph.fresh_effect_at(1, None).unwrap(); let base = unary(&mut graph, unit, unit);
    let enclosing = graph.generalize_with_effect_roots(base, 0, Generalization::Allowed, &[], &[EffectSummary::Variable(latent)]).unwrap();
    let captured = graph.scheme(enclosing).unwrap().effect_roots[0]; let output = graph.fresh_derived_effect_at(1, None).unwrap();
    graph.include_effects(EffectSummary::Closed(EffectSet::TIME), EffectSummary::Variable(output), why).unwrap();
    graph.include_effects(captured, EffectSummary::Variable(output), why).unwrap();
    assert_eq!(graph.resolved_effect_summary(EffectSummary::Variable(output)).unwrap(), EffectSummary::Variable(output));
    let root = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Variable(output) }).unwrap();
    let roots = GeneralizationRoots { captured_types: vec![], effects: vec![captured] };
    let scheme = graph.generalize_with_roots(root, 0, Generalization::Allowed, &[], &roots, Some(enclosing)).unwrap();
    assert_eq!(graph.scheme(scheme).unwrap().effect_quantifiers.len(), 1);
    assert_eq!(graph.scheme(scheme).unwrap().effect_quantifiers[0].lower, EffectSet::TIME);
    assert!(graph.scheme(scheme).unwrap().effect_captures.contains(&captured));
    assert!(graph.scheme(scheme).unwrap().effect_inclusions.iter().any(|(input, output)| *input == captured && matches!(output, EffectSummary::Rigid { scope, .. } if *scope == scheme)));
    let instance = graph.instantiate(scheme, 1, why).unwrap();
    let TypeNode::Arrow(signature) = graph.node(instance.ty).unwrap().clone() else { panic!() };
    assert!(matches!(graph.resolved_effect_summary(signature.effects).unwrap(), EffectSummary::Variable(_)));
    let work = graph.counters().work_units;
    assert!(graph.probe(|graph| graph.include_effects(signature.effects, EffectSummary::Closed(EffectSet::TIME), why)).is_err());
    assert!(graph.counters().work_units > work);
    assert!(matches!(graph.resolved_effect_summary(signature.effects).unwrap(), EffectSummary::Variable(_)));
}

#[test]
fn captured_effect_inputs_respect_downstream_bounds_in_both_insertion_orders() {
    for capture_first in [false, true] {
        let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap();
        let latent = graph.fresh_effect_at(1, None).unwrap(); let base = unary(&mut graph, unit, unit);
        let enclosing = graph.generalize_with_effect_roots(base, 0, Generalization::Allowed, &[], &[EffectSummary::Variable(latent)]).unwrap();
        let captured = graph.scheme(enclosing).unwrap().effect_roots[0];
        let output = graph.fresh_derived_effect_at(1, None).unwrap();
        let bounded = graph.fresh_derived_effect_at(1, Some(EffectSet::TIME)).unwrap();
        let work = graph.counters().work_units;
        if capture_first {
            graph.include_effects(captured, EffectSummary::Variable(output), why).unwrap();
            assert!(matches!(graph.include_effects(EffectSummary::Variable(output), EffectSummary::Variable(bounded), why), Err(InferenceError::EffectViolation)));
            assert!(graph.effects[output.index()].value.outgoing.is_empty());
            assert!(graph.effects[bounded.index()].value.incoming.is_empty());
        } else {
            graph.include_effects(EffectSummary::Variable(output), EffectSummary::Variable(bounded), why).unwrap();
            assert!(matches!(graph.include_effects(captured, EffectSummary::Variable(output), why), Err(InferenceError::EffectViolation)));
            assert!(graph.effects[output.index()].value.rigid_inputs.is_empty());
        }
        assert!(graph.counters().work_units > work);
        assert_eq!(graph.effect_value(bounded).unwrap(), EffectSet::EMPTY);
    }
}

#[test]
fn calculated_capture_edges_are_bounded_and_failed_propagation_is_rewound() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap();
    let latent = graph.fresh_effect_at(1, None).unwrap(); let base = unary(&mut graph, unit, unit);
    let enclosing = graph.generalize_with_effect_roots(base, 0, Generalization::Allowed, &[], &[EffectSummary::Variable(latent)]).unwrap();
    let captured = graph.scheme(enclosing).unwrap().effect_roots[0];
    let outputs = (0..3).map(|_| graph.fresh_derived_effect_at(1, None).unwrap()).collect::<Vec<_>>();
    for pair in outputs.windows(2) { graph.include_effects(EffectSummary::Variable(pair[0]), EffectSummary::Variable(pair[1]), why).unwrap(); }
    graph.limits.constraints = graph.counters().attempted_constraints as usize + 2;
    assert!(matches!(graph.include_effects(captured, EffectSummary::Variable(outputs[0]), why), Err(InferenceError::Limit("constraints"))));
    assert!(graph.counters().attempted_constraints > graph.limits.constraints as u64);
    for output in outputs { assert!(graph.effects[output.index()].value.rigid_inputs.is_empty()); }
}

#[test]
fn unbound_captured_effect_unions_cannot_be_reported_as_finite_lower_bounds() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap();
    let latent = graph.fresh_effect_at(1, None).unwrap(); let base = unary(&mut graph, unit, unit);
    let enclosing = graph.generalize_with_effect_roots(base, 0, Generalization::Allowed, &[], &[EffectSummary::Variable(latent)]).unwrap();
    let captured = graph.scheme(enclosing).unwrap().effect_roots[0]; let output = graph.fresh_derived_effect_at(1, None).unwrap();
    graph.include_effects(EffectSummary::Closed(EffectSet::TIME), EffectSummary::Variable(output), why).unwrap();
    graph.include_effects(captured, EffectSummary::Variable(output), why).unwrap();
    assert!(graph.closed_effect_summary(EffectSummary::Variable(output)).is_err());
    let root = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Variable(output) }).unwrap();
    let scheme = graph.generalize_with_roots(root, 0, Generalization::Allowed, &[], &GeneralizationRoots { captured_types: vec![], effects: vec![captured] }, Some(enclosing)).unwrap();
    let body = graph.scheme(scheme).unwrap().body;
    graph.freeze_scoped(&[ScopedRoot { ty: body, scope: Some(scheme) }]).unwrap();
}

#[test]
fn captured_bounded_effect_plus_fixed_part_seals_to_its_exact_finite_union() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap();
    let latent = graph.fresh_effect_at(1, Some(EffectSet::TIME)).unwrap(); let base = unary(&mut graph, unit, unit);
    let enclosing = graph.generalize_with_effect_roots(base, 0, Generalization::Allowed, &[], &[EffectSummary::Variable(latent)]).unwrap();
    let captured = graph.scheme(enclosing).unwrap().effect_roots[0]; let output = graph.fresh_derived_effect_at(1, None).unwrap();
    graph.include_effects(EffectSummary::Closed(EffectSet::TIME), EffectSummary::Variable(output), why).unwrap();
    graph.include_effects(captured, EffectSummary::Variable(output), why).unwrap();
    graph.seal_derived_effects(&[EffectSummary::Variable(output)]).unwrap();
    assert_eq!(graph.resolved_effect_summary(EffectSummary::Variable(output)).unwrap(), EffectSummary::Closed(EffectSet::TIME));
    let root = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Variable(output) }).unwrap();
    let scheme = graph.generalize_with_roots(root, 0, Generalization::Allowed, &[], &GeneralizationRoots { captured_types: vec![], effects: vec![captured] }, Some(enclosing)).unwrap();
    assert!(graph.scheme(scheme).unwrap().effect_quantifiers.is_empty());
    assert!(graph.scheme(scheme).unwrap().effect_captures.contains(&captured));
}

#[test]
fn calculated_upper_worklist_handles_cycles_without_sealing_latent_inputs() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap();
    let latent = graph.fresh_effect_at(1, Some(EffectSet::TIME)).unwrap();
    let first = graph.fresh_derived_effect_at(1, None).unwrap(); let second = graph.fresh_derived_effect_at(1, None).unwrap();
    graph.include_effects(EffectSummary::Closed(EffectSet::TIME), EffectSummary::Variable(first), why).unwrap();
    graph.include_effects(EffectSummary::Variable(latent), EffectSummary::Variable(first), why).unwrap();
    graph.include_effects(EffectSummary::Variable(first), EffectSummary::Variable(second), why).unwrap();
    graph.include_effects(EffectSummary::Variable(second), EffectSummary::Variable(first), why).unwrap();
    graph.seal_derived_effects(&[EffectSummary::Variable(second)]).unwrap();
    assert_eq!(graph.resolved_effect_summary(EffectSummary::Variable(latent)).unwrap(), EffectSummary::Variable(latent));
    assert_eq!(graph.resolved_effect_summary(EffectSummary::Variable(first)).unwrap(), EffectSummary::Closed(EffectSet::TIME));
    assert_eq!(graph.resolved_effect_summary(EffectSummary::Variable(second)).unwrap(), EffectSummary::Closed(EffectSet::TIME));
    let callback = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Variable(latent) }).unwrap();
    let root = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("run"), ty: callback, defaulted: false, rest: false }], result: unit, effects: EffectSummary::Variable(second) }).unwrap();
    let scheme = graph.generalize(root, 0, Generalization::Allowed, &[]).unwrap();
    let quantifiers = &graph.scheme(scheme).unwrap().effect_quantifiers;
    assert_eq!(quantifiers.len(), 1); assert!(!quantifiers[0].derived); assert_eq!(quantifiers[0].upper, Some(EffectSet::TIME));
}

#[test]
fn captured_partial_upper_union_stays_symbolic_with_an_exact_derived_bound() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap();
    let latent = graph.fresh_effect_at(1, Some(EffectSet::ENV)).unwrap(); let base = unary(&mut graph, unit, unit);
    let enclosing = graph.generalize_with_effect_roots(base, 0, Generalization::Allowed, &[], &[EffectSummary::Variable(latent)]).unwrap();
    let captured = graph.scheme(enclosing).unwrap().effect_roots[0]; let output = graph.fresh_derived_effect_at(1, None).unwrap();
    graph.include_effects(EffectSummary::Closed(EffectSet::TIME), EffectSummary::Variable(output), why).unwrap(); graph.include_effects(captured, EffectSummary::Variable(output), why).unwrap();
    let root = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Variable(output) }).unwrap();
    let scheme = graph.generalize_with_roots(root, 0, Generalization::Allowed, &[], &GeneralizationRoots { captured_types: vec![], effects: vec![captured] }, Some(enclosing)).unwrap();
    let quantifiers = &graph.scheme(scheme).unwrap().effect_quantifiers;
    assert_eq!(quantifiers.len(), 1); assert!(quantifiers[0].derived); assert_eq!(quantifiers[0].lower, EffectSet::TIME);
    assert_eq!(quantifiers[0].upper, Some(EffectSet(EffectSet::TIME.0 | EffectSet::ENV.0)));
}

#[test]
fn captured_calculated_inputs_wake_operation_dependents_without_losing_provenance() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap();
    let latent = graph.fresh_effect_at(1, None).unwrap(); let base = unary(&mut graph, unit, unit);
    let enclosing = graph.generalize_with_effect_roots(base, 0, Generalization::Allowed, &[], &[EffectSummary::Variable(latent)]).unwrap();
    let captured = graph.scheme(enclosing).unwrap().effect_roots[0];
    let fixed = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Closed(EffectSet::TIME) }).unwrap();
    let fixed = graph.generalize(fixed, 0, Generalization::Allowed, &[]).unwrap();
    let candidate = graph.register_candidate(CandidateTemplate { failure_projection: None, identity: Name::intern("fixed-clock"), public_label: Name::intern("fixed-clock"), effect_roles: vec![], output_effect_roles: vec![], scheme: fixed, has_receiver: false, actual_eligibility: vec![], argument_relations: vec![] }).unwrap();
    let family = graph.register_family(&[candidate]).unwrap(); let output = graph.fresh_derived_effect_at(1, None).unwrap();
    let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![], result: unit, effects: EffectSummary::Variable(output), effect_bindings: vec![], output_effect_bindings: vec![] }, why).unwrap(); graph.solve().unwrap();
    let wakeups = graph.counters().wakeups; let queue_pushes = graph.counters().queue_pushes;
    graph.include_effects(captured, EffectSummary::Variable(output), why).unwrap();
    assert!(graph.counters().wakeups > wakeups); assert!(graph.counters().queue_pushes > queue_pushes);
    graph.solve().unwrap(); graph.seal_derived_effects(&[EffectSummary::Variable(output)]).unwrap();
    assert_eq!(graph.resolved_effect_summary(EffectSummary::Variable(output)).unwrap(), EffectSummary::Variable(output));
    assert_eq!(graph.effect_value(output).unwrap(), EffectSet::TIME);
    assert_eq!(graph.candidate_evidence(requirement).unwrap().unwrap().effects, EffectSummary::Closed(EffectSet::TIME));
    assert!(graph.effects[output.index()].value.derived);
}

#[test]
fn abstract_operation_trials_preserve_row_masks() { assert_abstract_operation_bounds(true); }
#[test]
fn abstract_operation_trials_preserve_latent_effect_bounds() { assert_abstract_operation_bounds(false); }

fn assert_abstract_operation_bounds(row_case: bool) {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    {
        let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap(); let int = graph.atom(Atom::Int).unwrap();
        let (actual, expected) = if row_case {
            let tail = graph.fresh_row(1, span()).unwrap();
            graph.row(vec![RowField { label: Name::intern("reserved"), ty: int }], Some(tail)).unwrap();
            let open = graph.row(vec![], Some(tail)).unwrap(); let actual = graph.record(open).unwrap();
            let closed = graph.row(vec![RowField { label: Name::intern("reserved"), ty: int }], None).unwrap(); let expected = graph.record(closed).unwrap(); (actual, expected)
        } else {
            let latent = graph.fresh_effect_at(1, None).unwrap(); graph.include_effects(EffectSummary::Closed(EffectSet::FS), EffectSummary::Variable(latent), why).unwrap();
            let actual = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Variable(latent) }).unwrap();
            let expected = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap(); (actual, expected)
        };
        let abstract_header = unary(&mut graph, actual, unit); let abstract_scheme = graph.generalize(abstract_header, 0, Generalization::Allowed, &[]).unwrap();
        let TypeNode::Arrow(signature) = graph.node(graph.scheme(abstract_scheme).unwrap().body).unwrap() else { panic!() }; let actual = signature.params[0].ty;
        let expected_header = unary(&mut graph, expected, unit); let expected_scheme = graph.generalize(expected_header, 0, Generalization::Allowed, &[]).unwrap();
        let candidate = graph.register_candidate(CandidateTemplate { failure_projection: None, identity: Name::intern("bounded-operand"), public_label: Name::intern("bounded-operand"), effect_roles: vec![], output_effect_roles: vec![], scheme: expected_scheme, has_receiver: false, actual_eligibility: vec![], argument_relations: vec![ArgumentRelation::Exact] }).unwrap();
        let family = graph.register_family(&[candidate]).unwrap();
        let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![Some(actual)], result: unit, effects: EffectSummary::Closed(EffectSet::EMPTY), effect_bindings: vec![], output_effect_bindings: vec![] }, why).unwrap();
        assert!(matches!(graph.solve(), Err(InferenceError::UnsupportedOperation(id)) if id == requirement));
        assert!(graph.candidate_evidence(requirement).unwrap().is_none());
    }
}

fn apply_invocation_scheme(graph: &mut InferenceContext, kind: InvocationArgumentKind, domain: CallableDomain, why: ReasonId) -> (SchemeId, TypeId) {
    let callback = graph.fresh(1, span()).unwrap(); let value = graph.fresh(1, span()).unwrap(); let result = graph.fresh(1, span()).unwrap();
    let effects = if domain == CallableDomain::Pure { EffectSummary::Closed(EffectSet::EMPTY) } else { EffectSummary::Variable(graph.fresh_derived_effect_at(1, None).unwrap()) };
    let requirement = graph.require_callable_invocation(InvocationCall { callable: callback, arguments: vec![InvocationArgument { kind, ty: value }], result, effects, domain }, why).unwrap();
    graph.solve().unwrap(); assert!(matches!(graph.node(graph.resolved(callback).unwrap()).unwrap(), TypeNode::Meta(_)));
    assert!(graph.invocation_evidence(requirement).unwrap().is_none());
    let root = graph.arrow(Arrow { kind: if domain == CallableDomain::Pure { CallableKind::Pure } else { CallableKind::Proc }, params: vec![Parameter { label: Name::intern("callback"), ty: callback, defaulted: false, rest: false }, Parameter { label: Name::intern("value"), ty: value, defaulted: false, rest: false }], result, effects }).unwrap();
    let scheme = graph.generalize(root, 0, Generalization::Allowed, &[requirement]).unwrap(); (scheme, callback)
}

#[test]
fn callable_invocation_positional_calls_keep_actual_labels_and_independent_instances() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap(); let string = graph.atom(Atom::Str).unwrap();
    let (scheme, original) = apply_invocation_scheme(&mut graph, InvocationArgumentKind::Positional, CallableDomain::Pure, why);
    let original = graph.resolved(original).unwrap();
    for (label, operand, answer) in [("unrelated_name", int, string), ("different_parameter", string, int)] {
        let callback = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern(label), ty: operand, defaulted: false, rest: false }], result: answer, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
        let instance = graph.instantiate(scheme, 0, why).unwrap(); let TypeNode::Arrow(signature) = graph.node(instance.ty).unwrap().clone() else { panic!() };
        graph.unify(signature.params[0].ty, callback, why).unwrap(); graph.unify(signature.params[1].ty, operand, why).unwrap(); graph.solve().unwrap();
        assert_eq!(graph.resolved(signature.result).unwrap(), answer);
        let evidence = graph.invocation_evidence(instance.requirements[0]).unwrap().unwrap();
        assert_eq!(evidence.unique_plan().unwrap().0, callback); assert_eq!(evidence.unique_plan().unwrap().1.supplied_slots, vec![0]); assert!(evidence.unique_plan().unwrap().1.default_slots.is_empty());
        assert_eq!(graph.resolved(original).unwrap(), original);
    }
}

#[test]
fn callable_invocation_rejects_proc_in_pure_context_and_wrong_operand_without_training() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    for kind in [CallableKind::Proc, CallableKind::Pure] {
        let mut graph = InferenceContext::default(); let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap(); let string = graph.atom(Atom::Str).unwrap();
        let operand = if kind == CallableKind::Proc { int } else { string };
        let (scheme, _) = apply_invocation_scheme(&mut graph, InvocationArgumentKind::Positional, CallableDomain::Pure, why);
        let callback = graph.arrow(Arrow { kind, params: vec![Parameter { label: Name::intern("input"), ty: int, defaulted: false, rest: false }], result: int, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
        let instance = graph.instantiate(scheme, 0, why).unwrap(); let TypeNode::Arrow(signature) = graph.node(instance.ty).unwrap().clone() else { panic!() };
        graph.unify(signature.params[0].ty, callback, why).unwrap(); graph.unify(signature.params[1].ty, operand, why).unwrap();
        let failure = graph.solve().unwrap_err();
        if kind == CallableKind::Proc { assert!(matches!(failure, InferenceError::InvalidInvocation { problem: InvocationProblem::CallableKind, .. })); }
        else { assert!(matches!(failure, InferenceError::TypeMismatch { .. })); }
        assert!(matches!(graph.node(graph.resolved(signature.result).unwrap()).unwrap(), TypeNode::Meta(_)));
        assert!(graph.invocation_evidence(instance.requirements[0]).unwrap().is_none());
    }
}

#[test]
fn callable_invocation_named_arguments_retain_their_label_requirement() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    for label in ["payload", "renamed"] {
        let mut graph = InferenceContext::default(); let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap();
        let (scheme, _) = apply_invocation_scheme(&mut graph, InvocationArgumentKind::Named(Name::intern("payload")), CallableDomain::Pure, why);
        let callback = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern(label), ty: int, defaulted: false, rest: false }], result: int, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
        let instance = graph.instantiate(scheme, 0, why).unwrap(); let TypeNode::Arrow(signature) = graph.node(instance.ty).unwrap().clone() else { panic!() };
        graph.unify(signature.params[0].ty, callback, why).unwrap(); graph.unify(signature.params[1].ty, int, why).unwrap();
        if label == "payload" { graph.solve().unwrap(); assert_eq!(graph.resolved(signature.result).unwrap(), int); }
        else { assert!(matches!(graph.solve(), Err(InferenceError::InvalidInvocation { problem: InvocationProblem::UnknownLabel(name), .. }) if name == Name::intern("payload"))); }
    }
}

#[test]
fn callable_invocation_binding_uses_actual_defaults_rest_and_splice_shape() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    for case in 0..7 {
        let mut graph = InferenceContext::default(); let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap(); let string = graph.atom(Atom::Str).unwrap(); let ints = graph.list(int).unwrap();
        let required = Parameter { label: Name::intern("first"), ty: int, defaulted: false, rest: false };
        let mut parameters = vec![required]; let mut arguments = vec![InvocationArgument { kind: InvocationArgumentKind::Positional, ty: int }];
        match case {
            0 | 1 => parameters.push(Parameter { label: Name::intern("second"), ty: string, defaulted: case == 1, rest: false }),
            2 | 3 | 4 => { parameters[0].rest = true; parameters[0].ty = if case == 3 { graph.list(string).unwrap() } else { ints }; if case == 4 { arguments[0].kind = InvocationArgumentKind::PositionalSplice; arguments[0].ty = ints; } },
            5 => { parameters[0].ty = ints; arguments[0].kind = InvocationArgumentKind::PositionalSplice; arguments[0].ty = ints; },
            6 => { arguments.push(InvocationArgument { kind: InvocationArgumentKind::Named(Name::intern("first")), ty: int }); },
            _ => unreachable!(),
        }
        let callback = graph.arrow(Arrow { kind: CallableKind::Pure, params: parameters, result: int, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap(); let result = graph.fresh(0, span()).unwrap();
        let requirement = graph.require_callable_invocation(InvocationCall { callable: callback, arguments, result, effects: EffectSummary::Closed(EffectSet::EMPTY), domain: CallableDomain::Pure }, why).unwrap();
        match case {
            0 => assert!(matches!(graph.solve(), Err(InferenceError::InvalidInvocation { problem: InvocationProblem::MissingArgument(name), .. }) if name == Name::intern("second"))),
            3 => assert!(matches!(graph.solve(), Err(InferenceError::TypeMismatch { .. }))),
            5 => assert!(matches!(graph.solve(), Err(InferenceError::TypeMismatch { .. }))),
            6 => assert!(matches!(graph.solve(), Err(InferenceError::InvalidInvocation { problem: InvocationProblem::DuplicateArgument(_), .. }))),
            _ => { graph.solve().unwrap(); let evidence = graph.invocation_evidence(requirement).unwrap().unwrap(); assert_eq!(evidence.unique_plan().unwrap().1.supplied_slots, vec![0]); assert_eq!(evidence.unique_plan().unwrap().1.default_slots, if case == 1 { vec![1] } else { vec![] }); assert_eq!(evidence.unique_plan().unwrap().1.rest_slot, if case >= 2 { Some(0) } else { None }); },
        }
    }
}

#[test]
fn callable_invocation_effect_outputs_are_fresh_and_stream_defaults_stay_lazy() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap();
    let (scheme, _) = apply_invocation_scheme(&mut graph, InvocationArgumentKind::Positional, CallableDomain::AnyCallable, why);
    assert_eq!(graph.scheme(scheme).unwrap().effect_quantifiers.len(), 1);
    for (kind, effects) in [(CallableKind::Proc, EffectSummary::Closed(EffectSet::TIME)), (CallableKind::Proc, EffectSummary::Closed(EffectSet::ENV)), (CallableKind::Proc, EffectSummary::Unknown), (CallableKind::Stream, EffectSummary::Closed(EffectSet::EMPTY))] {
        let result = if kind == CallableKind::Stream { graph.stream(int).unwrap() } else { int };
        let callback = graph.arrow(Arrow { kind, params: vec![Parameter { label: Name::intern("eager"), ty: int, defaulted: false, rest: false }, Parameter { label: Name::intern("default"), ty: int, defaulted: true, rest: false }], result, effects }).unwrap();
        let instance = graph.instantiate(scheme, 0, why).unwrap(); let TypeNode::Arrow(signature) = graph.node(instance.ty).unwrap().clone() else { panic!() };
        graph.unify(signature.params[0].ty, callback, why).unwrap(); graph.unify(signature.params[1].ty, int, why).unwrap(); graph.solve().unwrap();
        assert_eq!(graph.closed_effect_summary(signature.effects).unwrap(), effects);
        let evidence = graph.invocation_evidence(instance.requirements[0]).unwrap().unwrap(); assert_eq!(evidence.unique_plan().unwrap().1.default_slots, vec![1]);
        assert_eq!(evidence.unique_plan().unwrap().2, if kind == CallableKind::Stream { InvocationDefaultTiming::AtPull } else { InvocationDefaultTiming::AtCall });
    }
}

#[test]
fn callable_invocation_failed_binding_and_effect_proofs_rewind_partial_constraints() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    for argument_failure in [true, false] {
        let mut graph = InferenceContext::default(); let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap(); let string = graph.atom(Atom::Str).unwrap();
        let callback = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("first"), ty: int, defaulted: false, rest: false }, Parameter { label: Name::intern("second"), ty: int, defaulted: false, rest: false }], result: int, effects: EffectSummary::Closed(EffectSet::TIME) }).unwrap();
        let first = graph.fresh(0, span()).unwrap(); let result = graph.fresh(0, span()).unwrap();
        let effects = if argument_failure { EffectSummary::Variable(graph.fresh_derived_effect_at(0, None).unwrap()) } else { EffectSummary::Closed(EffectSet::ENV) };
        let requirement = graph.require_callable_invocation(InvocationCall { callable: callback, arguments: vec![InvocationArgument { kind: InvocationArgumentKind::Positional, ty: first }, InvocationArgument { kind: InvocationArgumentKind::Positional, ty: if argument_failure { string } else { int } }], result, effects, domain: CallableDomain::AnyCallable }, why).unwrap();
        let work = graph.counters().work_units; let failure = graph.solve().unwrap_err();
        if argument_failure { assert!(matches!(failure, InferenceError::TypeMismatch { .. })); } else { assert!(matches!(failure, InferenceError::EffectViolation)); }
        assert!(matches!(graph.node(graph.resolved(first).unwrap()).unwrap(), TypeNode::Meta(_))); assert!(matches!(graph.node(graph.resolved(result).unwrap()).unwrap(), TypeNode::Meta(_)));
        assert!(graph.invocation_evidence(requirement).unwrap().is_none()); assert!(graph.counters().work_units > work);
    }
}

#[test]
fn callable_invocation_payload_handles_retire_after_trials_and_keep_reason_provenance() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap(); let callback = graph.fresh(1, span()).unwrap();
    let old = graph.trial(|graph| {
        let requirement = graph.require_callable_invocation(InvocationCall { callable: callback, arguments: vec![], result: int, effects: EffectSummary::Closed(EffectSet::EMPTY), domain: CallableDomain::Pure }, why)?;
        let RequirementTemplate::CallableInvocation { call } = graph.requirement_template(requirement)? else { panic!() }; Ok(call)
    }).unwrap();
    assert!(matches!(graph.invocation_call(old), Err(InferenceError::ForeignHandle)));
    let requirement = graph.require_callable_invocation(InvocationCall { callable: callback, arguments: vec![], result: int, effects: EffectSummary::Closed(EffectSet::EMPTY), domain: CallableDomain::Pure }, why).unwrap();
    let RequirementTemplate::CallableInvocation { call } = graph.requirement_template(requirement).unwrap() else { panic!() };
    assert_eq!(call.index(), old.index()); assert_ne!(call.generation(), old.generation());
    assert!(graph.constraint_origins().iter().any(|origin| origin.reason == why && matches!(origin.relation, ConstraintRelation::CallableInvocation { call: source } if source == call)));
}

#[test]
fn callable_invocation_scoped_schemes_and_ground_binding_proofs_freeze_together() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap();
    let (scheme, _) = apply_invocation_scheme(&mut graph, InvocationArgumentKind::Positional, CallableDomain::Pure, why);
    let callback = unary(&mut graph, int, int); let instance = graph.instantiate(scheme, 0, why).unwrap(); let TypeNode::Arrow(signature) = graph.node(instance.ty).unwrap().clone() else { panic!() };
    graph.unify(signature.params[0].ty, callback, why).unwrap(); graph.unify(signature.params[1].ty, int, why).unwrap(); graph.solve().unwrap();
    let source = graph.requirements.iter().enumerate().find_map(|(index, slot)| { let id = RequirementId { index: index as u32, generation: slot.generation }; (id != instance.requirements[0] && matches!(slot.value.template, RequirementTemplate::CallableInvocation { .. })).then_some(id) }).unwrap();
    let roots = [ScopedRoot { ty: graph.scheme(scheme).unwrap().body, scope: Some(scheme) }, ScopedRoot { ty: instance.ty, scope: None }];
    let facts = [ScopedRequirementRoot { requirement: source, scope: Some(scheme) }, ScopedRequirementRoot { requirement: instance.requirements[0], scope: None }];
    let frozen = graph.freeze_scoped_with_facts(&roots, &[], &facts).unwrap();
    for fact in facts { frozen.validate_requirement_scoped(fact).unwrap(); }
    assert_eq!(frozen.invocation_evidence(instance.requirements[0]).unwrap().unwrap().unique_plan().unwrap().0, callback);
}

#[test]
fn callable_invocation_pure_domain_has_empty_computed_effects_before_shape_is_known() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let callback = graph.fresh(1, span()).unwrap(); let int = graph.atom(Atom::Int).unwrap();
    graph.require_callable_invocation(InvocationCall { callable: callback, arguments: vec![], result: int, effects: EffectSummary::Closed(EffectSet::TIME), domain: CallableDomain::Pure }, why).unwrap();
    assert!(matches!(graph.solve(), Err(InferenceError::EffectViolation)));
    assert!(matches!(graph.node(graph.resolved(callback).unwrap()).unwrap(), TypeNode::Meta(_)));
}

#[test]
fn callable_invocation_concrete_effect_equation_discharges_before_ground_caller_generalization() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap();
    let (scheme, _) = apply_invocation_scheme(&mut graph, InvocationArgumentKind::Positional, CallableDomain::AnyCallable, why);
    for effects in [EffectSet::TIME, EffectSet::ENV] {
        let callback = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("input"), ty: int, defaulted: false, rest: false }], result: int, effects: EffectSummary::Closed(effects) }).unwrap();
        let instance = graph.instantiate(scheme, 1, why).unwrap(); let TypeNode::Arrow(signature) = graph.node(instance.ty).unwrap().clone() else { panic!() };
        graph.unify(signature.params[0].ty, callback, why).unwrap(); graph.unify(signature.params[1].ty, int, why).unwrap(); graph.solve().unwrap();
        let caller = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: int, effects: EffectSummary::Closed(effects) }).unwrap();
        let caller = graph.generalize(caller, 0, Generalization::Allowed, &instance.requirements).unwrap();
        assert!(graph.scheme(caller).unwrap().requirements.is_empty()); assert!(graph.scheme(caller).unwrap().effect_quantifiers.is_empty());
        assert_eq!(graph.resolved_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(effects));
    }
    assert_eq!(graph.scheme(scheme).unwrap().effect_quantifiers.len(), 1);
}

#[test]
fn masked_effect_inclusion_removes_only_the_retained_error_permission() {
    for actual in [EffectSummary::Closed(EffectSet(EffectSet::TIME.0 | EffectSet::ERROR.0)), EffectSummary::Closed(EffectSet::ENV), EffectSummary::Unknown] {
        let mut graph = InferenceContext::default(); let why = reason(&mut graph);
        let output = graph.fresh_derived_effect_at(1, None).unwrap();
        let expected = if actual == EffectSummary::Closed(EffectSet::ENV) { EffectSummary::Closed(EffectSet::TIME) } else { EffectSummary::Variable(output) };
        graph.include_effects_masked(actual, expected, EffectSet::ERROR, why).unwrap();
        if actual == EffectSummary::Closed(EffectSet::ENV) { assert!(matches!(graph.solve(), Err(InferenceError::EffectViolation))); }
        else { graph.solve().unwrap(); graph.seal_derived_effects(&[expected]).unwrap(); assert_eq!(graph.resolved_effect_summary(expected).unwrap(), if actual == EffectSummary::Unknown { EffectSummary::Unknown } else { EffectSummary::Closed(EffectSet::TIME) }); }
    }
}

#[test]
fn instantiated_invocation_obligations_preserve_exact_source_origins() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let (scheme, _) = apply_invocation_scheme(&mut graph, InvocationArgumentKind::Positional, CallableDomain::Pure, why);
    let source = graph.requirements.iter().enumerate().find_map(|(index, slot)| matches!(slot.value.template, RequirementTemplate::CallableInvocation { .. }).then_some(RequirementId { index: index as u32, generation: slot.generation })).unwrap();
    assert_eq!(graph.requirement_origin(source).unwrap(), source);
    let first = graph.instantiate(scheme, 1, why).unwrap(); let second = graph.instantiate(scheme, 1, why).unwrap();
    assert_eq!(first.requirement_origins, vec![(source, first.requirements[0])]);
    assert_eq!(second.requirement_origins, vec![(source, second.requirements[0])]);
    assert_ne!(first.requirements[0], second.requirements[0]);
    for requirement in first.requirements.into_iter().chain(second.requirements) { assert_eq!(graph.requirement_origin(requirement).unwrap(), source); }
}

#[test]
fn masked_effect_inclusion_generalizes_symbolic_inputs_for_independent_callers() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap();
    let input = graph.fresh_effect_at(1, None).unwrap(); let output = graph.fresh_derived_effect_at(1, None).unwrap();
    let callback = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Variable(input) }).unwrap();
    let root = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("callback"), ty: callback, defaulted: false, rest: false }], result: unit, effects: EffectSummary::Variable(output) }).unwrap();
    let requirement = graph.include_effects_masked(EffectSummary::Variable(input), EffectSummary::Variable(output), EffectSet::ERROR, why).unwrap();
    let scheme = graph.generalize(root, 0, Generalization::Allowed, &[requirement]).unwrap();
    assert_eq!(graph.scheme(scheme).unwrap().effect_quantifiers.len(), 2);
    for permission in [EffectSet::TIME, EffectSet::ENV] {
        let instance = graph.instantiate(scheme, 1, why).unwrap(); let TypeNode::Arrow(signature) = graph.node(instance.ty).unwrap().clone() else { panic!() };
        let actual = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Closed(EffectSet(permission.0 | EffectSet::ERROR.0)) }).unwrap();
        graph.unify(signature.params[0].ty, actual, why).unwrap(); graph.solve().unwrap(); graph.seal_derived_effects(&[signature.effects]).unwrap(); graph.solve().unwrap();
        assert_eq!(graph.resolved_effect_summary(signature.effects).unwrap(), EffectSummary::Closed(permission));
    }
    assert_eq!(graph.scheme(scheme).unwrap().effect_quantifiers.len(), 2);
}

#[test]
fn masked_effect_inclusion_captures_exact_rigid_inputs_and_publishes_the_relation() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap();
    let input = graph.fresh_effect_at(1, None).unwrap(); let parent_root = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Variable(input) }).unwrap();
    let parent = graph.generalize(parent_root, 0, Generalization::Allowed, &[]).unwrap(); let TypeNode::Arrow(parent_arrow) = graph.node(graph.scheme(parent).unwrap().body).unwrap() else { panic!() }; let captured = parent_arrow.effects;
    let output = graph.fresh_derived_effect_at(2, None).unwrap(); let child_root = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Variable(output) }).unwrap();
    let requirement = graph.include_effects_masked(captured, EffectSummary::Variable(output), EffectSet::ERROR, why).unwrap();
    let child = graph.generalize_with_roots(child_root, 1, Generalization::Allowed, &[requirement], &GeneralizationRoots { captured_types: vec![], effects: vec![captured] }, Some(parent)).unwrap();
    assert!(graph.scheme(child).unwrap().effect_captures.contains(&captured));
    assert!(matches!(graph.scheme(child).unwrap().requirements[0], RequirementTemplate::EffectInclusion { actual, expected: EffectSummary::Rigid { scope, .. }, excluded: EffectSet::ERROR } if actual == captured && scope == child));
    let instance = graph.instantiate(child, 2, why).unwrap();
    assert!(matches!(graph.requirement_template(instance.requirements[0]).unwrap(), RequirementTemplate::EffectInclusion { actual, expected: EffectSummary::Variable(_), excluded: EffectSet::ERROR } if actual == captured));
    let roots = [ScopedRoot { ty: graph.scheme(parent).unwrap().body, scope: Some(parent) }, ScopedRoot { ty: graph.scheme(child).unwrap().body, scope: Some(child) }];
    graph.freeze_scoped_with_facts(&roots, &[], &[ScopedRequirementRoot { requirement, scope: Some(child) }]).unwrap();
}

#[test]
fn masked_effect_inclusion_failed_bounds_rewind_but_charge_work() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let input = graph.fresh_effect_at(1, None).unwrap();
    graph.include_effects(EffectSummary::Closed(EffectSet(EffectSet::TIME.0 | EffectSet::ERROR.0)), EffectSummary::Variable(input), why).unwrap();
    let requirement = graph.include_effects_masked(EffectSummary::Variable(input), EffectSummary::Closed(EffectSet::ENV), EffectSet::ERROR, why).unwrap(); let work = graph.counters().work_units;
    assert!(matches!(graph.solve(), Err(InferenceError::EffectViolation))); assert_eq!(graph.effects[input.index()].value.upper, None);
    assert!(!graph.requirement(requirement).unwrap().eligibility); assert!(graph.counters().work_units > work);
}

#[test]
fn instantiated_obligation_correspondences_preserve_two_calls_of_the_same_body() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let (inner, _) = apply_invocation_scheme(&mut graph, InvocationArgumentKind::Positional, CallableDomain::Pure, why);
    let first = graph.instantiate(inner, 1, why).unwrap(); let second = graph.instantiate(inner, 1, why).unwrap();
    let TypeNode::Arrow(first_arrow) = graph.node(first.ty).unwrap().clone() else { panic!() }; let TypeNode::Arrow(second_arrow) = graph.node(second.ty).unwrap().clone() else { panic!() };
    let row = graph.row(vec![RowField { label: Name::intern("first"), ty: first_arrow.result }, RowField { label: Name::intern("second"), ty: second_arrow.result }], None).unwrap(); let result = graph.record(row).unwrap();
    let params = first_arrow.params.into_iter().chain(second_arrow.params).enumerate().map(|(index, parameter)| Parameter { label: Name::intern(format!("input{index}")), ..parameter }).collect();
    let outer = graph.arrow(Arrow { kind: CallableKind::Pure, params, result, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
    let body_requirements = vec![first.requirements[0], second.requirements[0]];
    let outer = graph.generalize(outer, 0, Generalization::Allowed, &body_requirements).unwrap();
    assert_eq!(graph.scheme(outer).unwrap().requirement_origins, body_requirements);
    let instance = graph.instantiate(outer, 1, why).unwrap();
    assert_eq!(instance.requirement_origins, body_requirements.into_iter().zip(instance.requirements.iter().copied()).collect::<Vec<_>>());
    let work = graph.counters().work_units; let pairs = graph.requirement_correspondences(&instance.requirements).unwrap(); assert!(graph.counters().work_units > work);
    assert_eq!(pairs, instance.requirement_origins);
    assert_eq!(graph.requirement_origin(instance.requirements[0]).unwrap(), graph.requirement_origin(instance.requirements[1]).unwrap());
    assert_ne!(graph.requirement_source(instance.requirements[0]).unwrap(), graph.requirement_source(instance.requirements[1]).unwrap());
}

#[test]
fn masked_effect_inclusion_rejects_disjoint_rigid_effect_bounds() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap();
    let input = graph.fresh_effect_at(1, Some(EffectSet(EffectSet::TIME.0 | EffectSet::ERROR.0))).unwrap(); graph.include_effects(EffectSummary::Closed(EffectSet::TIME), EffectSummary::Variable(input), why).unwrap();
    let output = graph.fresh_effect_at(1, Some(EffectSet::ENV)).unwrap();
    let first = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Variable(input) }).unwrap(); let second = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: EffectSummary::Variable(output) }).unwrap();
    let root = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern("first"), ty: first, defaulted: false, rest: false }, Parameter { label: Name::intern("second"), ty: second, defaulted: false, rest: false }], result: unit, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
    let scheme = graph.generalize(root, 0, Generalization::Allowed, &[]).unwrap(); let TypeNode::Arrow(root) = graph.node(graph.scheme(scheme).unwrap().body).unwrap() else { panic!() };
    let TypeNode::Arrow(first) = graph.node(root.params[0].ty).unwrap() else { panic!() }; let actual = first.effects; let TypeNode::Arrow(second) = graph.node(root.params[1].ty).unwrap() else { panic!() }; let expected = second.effects;
    graph.include_effects_masked(actual, expected, EffectSet::ERROR, why).unwrap();
    assert!(matches!(graph.solve(), Err(InferenceError::EffectViolation)));
}

#[test]
fn masked_effect_inclusion_tautology_does_not_create_a_phantom_calculated_binder() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap(); let output = graph.fresh_derived_effect_at(1, None).unwrap();
    let summary = EffectSummary::Variable(output); let requirement = graph.include_effects_masked(summary, summary, EffectSet::ERROR, why).unwrap();
    let root = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: summary }).unwrap();
    let scheme = graph.generalize(root, 0, Generalization::Allowed, &[requirement]).unwrap();
    assert!(graph.scheme(scheme).unwrap().effect_quantifiers.is_empty()); assert!(graph.scheme(scheme).unwrap().requirements.is_empty());
    let TypeNode::Arrow(signature) = graph.node(graph.scheme(scheme).unwrap().body).unwrap() else { panic!() }; assert_eq!(signature.effects, EffectSummary::Closed(EffectSet::EMPTY));
}

#[test]
fn obligation_correspondences_include_candidate_dependencies_and_retire_failed_trials() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap();
    let (scheme, _) = apply_invocation_scheme(&mut graph, InvocationArgumentKind::Positional, CallableDomain::Pure, why); let source = graph.scheme(scheme).unwrap().requirement_origins[0];
    let retired = graph.trial(|graph| graph.instantiate(scheme, 1, why)).unwrap();
    assert!(matches!(graph.requirement_source(retired.requirements[0]), Err(InferenceError::ForeignHandle)));
    let candidate = graph.register_candidate(CandidateTemplate { failure_projection: None, identity: Name::intern("invoke-parameter"), public_label: Name::intern("invoke-parameter"), effect_roles: vec![], output_effect_roles: vec![], scheme, has_receiver: false, actual_eligibility: vec![], argument_relations: vec![ArgumentRelation::Exact, ArgumentRelation::Exact] }).unwrap();
    let family = graph.register_family(&[candidate]).unwrap(); let callback = unary(&mut graph, int, int);
    let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![Some(callback), Some(int)], result: int, effects: EffectSummary::Closed(EffectSet::EMPTY), effect_bindings: vec![], output_effect_bindings: vec![] }, why).unwrap(); graph.solve().unwrap();
    let dependency = graph.candidate_evidence(requirement).unwrap().unwrap().dependencies[0];
    let pairs = graph.requirement_correspondences(&[requirement]).unwrap(); assert!(pairs.contains(&(requirement, requirement))); assert!(pairs.contains(&(source, dependency))); assert_eq!(pairs.len(), 2);
    assert_eq!(graph.requirement_source(dependency).unwrap(), source); assert_eq!(graph.requirement_origin(dependency).unwrap(), source);
    let template = graph.requirement_template(requirement).unwrap(); graph.limits.work_units = graph.counters().work_units + 1;
    assert!(matches!(graph.requirement_correspondences(&[requirement]), Err(InferenceError::Limit("solver work"))));
    assert_eq!(graph.requirement_template(requirement).unwrap(), template);
}

#[test]
fn exact_producer_effect_ports_freshen_and_bind_closed_or_opaque_inputs() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let unit = graph.atom(Atom::Unit).unwrap();
    let pull = EffectSummary::Variable(graph.fresh_effect_at(1, None).unwrap()); let close = EffectSummary::Variable(graph.fresh_effect_at(1, None).unwrap()); let output = EffectSummary::Variable(graph.fresh_derived_effect_at(1, None).unwrap());
    graph.include_effects(pull, output, why).unwrap(); graph.include_effects(close, output, why).unwrap();
    let root = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: unit, effects: output }).unwrap();
    let scheme = graph.generalize_with_effect_roots(root, 0, Generalization::Allowed, &[], &[pull, close]).unwrap();
    for actual in [EffectSummary::Closed(EffectSet::TIME), EffectSummary::Closed(EffectSet::ENV), EffectSummary::Unknown] {
        let instance = graph.instantiate(scheme, 1, why).unwrap(); let formal_pull = instance.effect_roots[0]; let formal_close = instance.effect_roots[1];
        graph.equate_effects(actual, formal_pull, why).unwrap(); graph.equate_effects(formal_close, EffectSummary::Closed(EffectSet::EMPTY), why).unwrap(); graph.solve().unwrap();
        assert_eq!(graph.resolved_effect_summary(formal_pull).unwrap(), actual); assert_eq!(graph.resolved_effect_summary(formal_close).unwrap(), EffectSummary::Closed(EffectSet::EMPTY));
        let TypeNode::Arrow(signature) = graph.node(instance.ty).unwrap().clone() else { panic!() }; graph.seal_derived_effects(&[signature.effects]).unwrap();
        assert_eq!(graph.resolved_effect_summary(signature.effects).unwrap(), actual);
    }
    assert_eq!(graph.scheme(scheme).unwrap().effect_quantifiers.len(), 3);
}

#[test]
fn exact_effect_port_failure_rewinds_binding_and_retains_reason_edges() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let port = graph.fresh_effect_at(1, Some(EffectSet::ENV)).unwrap(); let before = graph.counters().work_units;
    assert!(matches!(graph.equate_effects(EffectSummary::Variable(port), EffectSummary::Closed(EffectSet::TIME), why), Err(InferenceError::EffectViolation)));
    assert_eq!(graph.resolved_effect_summary(EffectSummary::Variable(port)).unwrap(), EffectSummary::Variable(port)); assert_eq!(graph.effect_value(port).unwrap(), EffectSet::EMPTY); assert!(graph.counters().work_units > before);
    graph.equate_effects(EffectSummary::Variable(port), EffectSummary::Closed(EffectSet::ENV), why).unwrap();
    for (actual, expected) in [(EffectSummary::Variable(port), EffectSummary::Closed(EffectSet::ENV)), (EffectSummary::Closed(EffectSet::ENV), EffectSummary::Variable(port))] {
        assert!(graph.constraint_origins().iter().any(|origin| origin.reason == why && matches!(origin.relation, ConstraintRelation::EffectInclusion { actual: left, expected: right } if left == actual && right == expected)));
    }
}

fn projected_iteration_family(graph: &mut InferenceContext, why: ReasonId) -> (OperationFamilyId, TypeId, TypeId, TypeId, Vec<CandidateId>) {
    let int = graph.atom(Atom::Int).unwrap(); let stream = graph.stream(int).unwrap(); let error = graph.atom(Atom::Error).unwrap(); let wrapped = graph.result(stream, error).unwrap();
    let roles = [EffectRole::Pull { source: 0 }, EffectRole::Close { source: 0 }, EffectRole::PullProjection { source: 0, projection: EffectProjection::ResultSuccess }, EffectRole::CloseProjection { source: 0, projection: EffectProjection::ResultSuccess }];
    let mut candidates = Vec::new();
    for (success, source) in [(false, stream), (true, wrapped)] {
        let pull = EffectSummary::Variable(graph.fresh_effect_at(1, None).unwrap()); let close = EffectSummary::Variable(graph.fresh_effect_at(1, None).unwrap()); let execution = EffectSummary::Variable(graph.fresh_derived_effect_at(1, None).unwrap());
        graph.include_effects(pull, execution, why).unwrap(); graph.include_effects(close, execution, why).unwrap();
        if success { graph.include_effects(EffectSummary::Closed(EffectSet::ERROR), execution, why).unwrap(); }
        let root = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("source"), ty: source, defaulted: false, rest: false }], result: int, effects: execution }).unwrap();
        let scheme = graph.generalize_with_effect_roots(root, 0, Generalization::Allowed, &[], &[pull, close]).unwrap();
        let input_roots = graph.scheme(scheme).unwrap().effect_roots.clone(); let pull_index = graph.scheme_effect_binder_index(scheme, input_roots[0]).unwrap().unwrap() as u32; let close_index = graph.scheme_effect_binder_index(scheme, input_roots[1]).unwrap().unwrap() as u32;
        let effect_roles = roles.into_iter().enumerate().map(|(index, role)| {
            let active = if success { index >= 2 } else { index < 2 };
            (role, if active { EffectRoleReference::Binder(if index % 2 == 0 { pull_index } else { close_index }) } else { EffectRoleReference::Fixed(EffectSet::EMPTY) })
        }).collect();
        let candidate = graph.register_candidate(CandidateTemplate { failure_projection: None, identity: Name::intern(if success { "iterate-result-stream" } else { "iterate-stream" }), public_label: Name::intern("iterate"), effect_roles, output_effect_roles: vec![], scheme, has_receiver: false, actual_eligibility: vec![], argument_relations: vec![ArgumentRelation::Exact] }).unwrap(); candidates.push(candidate);
    }
    let family = graph.register_family(&candidates).unwrap(); (family, int, stream, wrapped, candidates)
}

#[test]
fn projected_effect_roles_select_by_exact_source_shape_and_freshen_each_call() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let (family, int, stream, wrapped, candidates) = projected_iteration_family(&mut graph, why);
    let roles = [EffectRole::Pull { source: 0 }, EffectRole::Close { source: 0 }, EffectRole::PullProjection { source: 0, projection: EffectProjection::ResultSuccess }, EffectRole::CloseProjection { source: 0, projection: EffectProjection::ResultSuccess }];
    let mut previous = None;
    for (success, pull, close) in [(false, EffectSummary::Closed(EffectSet::TIME), EffectSummary::Closed(EffectSet::FS)), (true, EffectSummary::Closed(EffectSet::ENV), EffectSummary::Closed(EffectSet::PROCESS)), (true, EffectSummary::Unknown, EffectSummary::Closed(EffectSet::EMPTY))] {
        let bindings = roles.into_iter().enumerate().map(|(index, role)| (role, if (index >= 2) == success { if index % 2 == 0 { pull } else { close } } else { EffectSummary::Closed(EffectSet::EMPTY) })).collect();
        let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![Some(if success { wrapped } else { stream })], result: int, effects: EffectSummary::Unknown, effect_bindings: bindings, output_effect_bindings: vec![] }, why).unwrap(); graph.solve().unwrap();
        let evidence = graph.candidate_evidence(requirement).unwrap().unwrap().clone(); assert_eq!(evidence.candidate, candidates[usize::from(success)]);
        if let Some(previous) = previous { assert_ne!(evidence.effect_substitutions, previous); } previous = Some(evidence.effect_substitutions.clone());
        graph.seal_derived_effects(&[evidence.effects]).unwrap();
        let expected = match (pull, close) { (EffectSummary::Closed(pull), EffectSummary::Closed(close)) => EffectSummary::Closed(EffectSet(pull.0 | close.0 | if success { EffectSet::ERROR.0 } else { 0 })), _ => EffectSummary::Unknown };
        assert_eq!(graph.resolved_effect_summary(evidence.effects).unwrap(), expected);
    }
    for candidate in candidates { assert_eq!(graph.scheme(graph.candidate(candidate).unwrap().scheme).unwrap().effect_quantifiers.len(), 3); }
}

#[test]
fn projected_effect_roles_reject_wrong_location_and_missing_or_foreign_source_proofs() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let (family, int, stream, wrapped, _) = projected_iteration_family(&mut graph, why);
    let roles = [EffectRole::Pull { source: 0 }, EffectRole::Close { source: 0 }, EffectRole::PullProjection { source: 0, projection: EffectProjection::ResultSuccess }, EffectRole::CloseProjection { source: 0, projection: EffectProjection::ResultSuccess }];
    for case in 0..4 {
        assert!(graph.trial(|graph| {
            let mut bindings: Vec<_> = roles.into_iter().map(|role| (role, EffectSummary::Closed(EffectSet::EMPTY))).collect();
            match case { 0 => bindings[2].1 = EffectSummary::Closed(EffectSet::TIME), 1 => bindings[0].1 = EffectSummary::Unknown, 2 => { bindings.pop(); }, 3 => bindings[2].0 = EffectRole::Pull { source: 1 }, _ => unreachable!() }
            graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![Some(if case == 1 { wrapped } else { stream })], result: int, effects: EffectSummary::Unknown, effect_bindings: bindings, output_effect_bindings: vec![] }, why)?; graph.solve()
        }).is_err());
    }
    let latent = graph.fresh_effect_at(1, None).unwrap(); let parent = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: int, effects: EffectSummary::Variable(latent) }).unwrap(); let parent = graph.generalize(parent, 0, Generalization::Allowed, &[]).unwrap(); let foreign = graph.scheme(parent).unwrap().effect_binders[0];
    let bindings = roles.into_iter().enumerate().map(|(index, role)| (role, if index == 0 { foreign } else { EffectSummary::Closed(EffectSet::EMPTY) })).collect();
    let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![Some(stream)], result: int, effects: EffectSummary::Unknown, effect_bindings: bindings, output_effect_bindings: vec![] }, why).unwrap(); graph.solve().unwrap();
    assert!(matches!(graph.freeze_scoped_with_facts(&[ScopedRoot { ty: int, scope: None }], &[], &[ScopedRequirementRoot { requirement, scope: None }]), Err(InferenceError::ScopeEscape)));
}

#[test]
fn declared_container_erasure_preserves_actual_meta_and_rechecks_its_known_items() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    for json in [false, true] {
        let mut graph = InferenceContext::default(); let why = reason(&mut graph); let any = graph.atom(Atom::Any).unwrap(); let string = graph.atom(Atom::Str).unwrap(); let list_any = graph.list(any).unwrap();
        let signature = unary(&mut graph, list_any, string); let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap();
        let candidate = graph.register_candidate(CandidateTemplate { failure_projection: None, identity: Name::intern("declared-list-input"), public_label: Name::intern("declared-list-input"), effect_roles: vec![], output_effect_roles: vec![], scheme, has_receiver: false, actual_eligibility: if json { vec![(0, Eligibility::JsonCompatible)] } else { vec![] }, argument_relations: vec![ArgumentRelation::DeclaredErasure] }).unwrap();
        let family = graph.register_family(&[candidate]).unwrap(); let actual = graph.fresh(1, span()).unwrap();
        let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![Some(actual)], result: string, effects: EffectSummary::Closed(EffectSet::EMPTY), effect_bindings: vec![], output_effect_bindings: vec![] }, why).unwrap(); graph.solve().unwrap();
        assert!(matches!(graph.node(graph.resolved(actual).unwrap()).unwrap(), TypeNode::Meta(_))); assert!(graph.candidate_evidence(requirement).unwrap().is_none());
        let root = unary(&mut graph, actual, string); let generic = graph.generalize(root, 0, Generalization::Allowed, &[requirement]).unwrap(); assert_eq!(graph.scheme(generic).unwrap().quantifiers.len(), 1);
        for rejected in [true, false] {
            let outcome = graph.trial(|graph| {
                let instance = graph.instantiate(generic, 1, why)?; let TypeNode::Arrow(signature) = graph.node(instance.ty)?.clone() else { panic!() };
                let operand = if rejected { if json { let path = graph.atom(Atom::Path)?; graph.list(path)? } else { string } } else { graph.list(string)? };
                graph.unify(signature.params[0].ty, operand, why)?; graph.solve()?;
                assert_eq!(graph.candidate_evidence(instance.requirements[0])?.unwrap().candidate, candidate); Ok(())
            });
            if rejected { assert!(outcome.is_err()); } else { outcome.unwrap(); }
        }
    }
}

#[test]
fn fixed_parameter_invocation_accepts_unknown_cardinality_splice_without_expanding_it() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let string = graph.atom(Atom::Str).unwrap(); let unit = graph.atom(Atom::Unit).unwrap(); let parts = graph.list(string).unwrap();
    let callback = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("a"), ty: string, defaulted: false, rest: false }, Parameter { label: Name::intern("b"), ty: string, defaulted: false, rest: false }], result: unit, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
    let requirement = graph.require_callable_invocation(InvocationCall { callable: callback, arguments: vec![InvocationArgument { kind: InvocationArgumentKind::PositionalSplice, ty: parts }], result: unit, effects: EffectSummary::Closed(EffectSet::EMPTY), domain: CallableDomain::AnyCallable }, why).unwrap();
    graph.solve().unwrap();
    let evidence = graph.invocation_evidence(requirement).unwrap().unwrap();
    assert!(evidence.unique_plan().unwrap().1.supplied_slots.is_empty()); let dynamic = evidence.unique_plan().unwrap().1.dynamic.as_ref().unwrap();
    assert_eq!(dynamic.segments, vec![InvocationArgumentSegment::DynamicRange { argument: 0, fixed_slots: vec![0, 1], rest_slot: None }]);
    assert_eq!(dynamic.required_slots, vec![0, 1]); assert!(dynamic.runtime_arity_guard && dynamic.runtime_duplicate_guard);
}

#[test]
fn dynamic_invocation_segments_preserve_named_order_defaults_and_rest_destinations() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let int = graph.atom(Atom::Int).unwrap(); let ints = graph.list(int).unwrap();
    let b = Name::intern("b");
    let callback = graph.arrow(Arrow { kind: CallableKind::Stream, params: vec![Parameter { label: Name::intern("a"), ty: int, defaulted: false, rest: false }, Parameter { label: b, ty: int, defaulted: true, rest: false }, Parameter { label: Name::intern("c"), ty: int, defaulted: true, rest: false }, Parameter { label: Name::intern("rest"), ty: ints, defaulted: false, rest: true }], result: ints, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
    let after = graph.plan_invocation_arguments(callback, &[InvocationArgumentKind::PositionalSplice, InvocationArgumentKind::Named(b)]).unwrap();
    let dynamic = after.dynamic.unwrap();
    assert_eq!(dynamic.segments, vec![InvocationArgumentSegment::DynamicRange { argument: 0, fixed_slots: vec![0, 1, 2], rest_slot: Some(3) }, InvocationArgumentSegment::StaticSlot { argument: 1, slot: 1 }]);
    assert!(dynamic.conditional_default_slots.is_empty()); assert_eq!(after.default_slots, vec![2]);
    let before = graph.plan_invocation_arguments(callback, &[InvocationArgumentKind::Named(b), InvocationArgumentKind::PositionalSplice, InvocationArgumentKind::Positional]).unwrap();
    let dynamic = before.dynamic.unwrap();
    assert_eq!(dynamic.segments[1], InvocationArgumentSegment::DynamicRange { argument: 1, fixed_slots: vec![0, 2], rest_slot: Some(3) });
    assert_eq!(dynamic.segments[2], InvocationArgumentSegment::DynamicRange { argument: 2, fixed_slots: vec![0, 2], rest_slot: Some(3) });
    assert!(graph.plan_invocation_arguments(callback, &[InvocationArgumentKind::Named(b), InvocationArgumentKind::Named(b)]).is_err());
}

#[test]
fn dynamic_splice_checks_every_reachable_fixed_type_and_rewinds_partial_bindings() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap(); let string = graph.atom(Atom::Str).unwrap(); let item = graph.fresh(1, span()).unwrap(); let parts = graph.list(item).unwrap();
    let callback = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern("a"), ty: int, defaulted: false, rest: false }, Parameter { label: Name::intern("b"), ty: string, defaulted: false, rest: false }], result: int, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
    let requirement = graph.require_callable_invocation(InvocationCall { callable: callback, arguments: vec![InvocationArgument { kind: InvocationArgumentKind::PositionalSplice, ty: parts }], result: int, effects: EffectSummary::Closed(EffectSet::EMPTY), domain: CallableDomain::Pure }, why).unwrap();
    let work = graph.counters().work_units;
    assert!(matches!(graph.solve(), Err(InferenceError::TypeMismatch { .. })));
    assert_eq!(graph.resolved(item).unwrap(), item); assert!(graph.invocation_evidence(requirement).unwrap().is_none()); assert!(graph.counters().work_units > work);
}

#[test]
fn dynamic_invocation_planning_has_a_shared_hard_work_bound() {
    let mut graph = InferenceContext::default(); let int = graph.atom(Atom::Int).unwrap(); let callback = unary(&mut graph, int, int);
    graph.limits.work_units = graph.counters().work_units + 2;
    assert!(matches!(graph.plan_invocation_arguments(callback, &[InvocationArgumentKind::PositionalSplice]), Err(InvocationPlanError::Graph(InferenceError::Limit("solver work")))));
}

#[test]
fn dynamic_binding_acceptance_matches_bounded_concrete_splice_expansion() {
    fn concrete(parameters: &[Parameter], kinds: &[InvocationArgumentKind], lengths: &[usize]) -> bool {
        let mut occupied = vec![false; parameters.len()]; let mut cursor = 0; let mut splice = 0;
        for kind in kinds {
            if let InvocationArgumentKind::Named(label) = kind {
                let Some(slot) = parameters.iter().position(|parameter| parameter.label == *label && !parameter.rest) else { return false };
                if occupied[slot] { return false; } occupied[slot] = true;
            } else {
                let count = if *kind == InvocationArgumentKind::PositionalSplice { let count = lengths[splice]; splice += 1; count } else { 1 };
                for _ in 0..count {
                    while cursor < parameters.len() && occupied[cursor] && !parameters[cursor].rest { cursor += 1; }
                    if cursor == parameters.len() { return false; }
                    occupied[cursor] = true; if !parameters[cursor].rest { cursor += 1; }
                }
            }
        }
        parameters.iter().zip(occupied).all(|(parameter, supplied)| supplied || parameter.defaulted || parameter.rest)
    }
    fn possible(parameters: &[Parameter], kinds: &[InvocationArgumentKind], lengths: &mut Vec<usize>, remaining: usize) -> bool {
        if remaining == 0 { return concrete(parameters, kinds, lengths); }
        for length in 0..=parameters.len() + 1 {
            lengths.push(length); let accepts = possible(parameters, kinds, lengths, remaining - 1); lengths.pop();
            if accepts { return true; }
        }
        false
    }
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let a = Name::intern("a"); let b = Name::intern("b"); let c = Name::intern("c");
    for fixed in 0..=3 { for rest in [false, true] { for defaults in 0..(1 << fixed) {
        let mut graph = InferenceContext::default(); let int = graph.atom(Atom::Int).unwrap(); let ints = graph.list(int).unwrap();
        let mut parameters: Vec<_> = [a, b, c].into_iter().take(fixed).enumerate().map(|(slot, label)| Parameter { label, ty: int, defaulted: defaults & (1 << slot) != 0, rest: false }).collect();
        if rest { parameters.push(Parameter { label: Name::intern("rest"), ty: ints, defaulted: false, rest: true }); }
        let callback = graph.arrow(Arrow { kind: CallableKind::Pure, params: parameters.clone(), result: int, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
        for length in 0..=3 { for encoding in 0..4usize.pow(length as u32) {
            let mut encoding = encoding;
            let kinds: Vec<_> = (0..length).map(|_| { let digit = encoding % 4; encoding /= 4; match digit { 0 => InvocationArgumentKind::Positional, 1 => InvocationArgumentKind::PositionalSplice, 2 => InvocationArgumentKind::Named(a), _ => InvocationArgumentKind::Named(b) } }).collect();
            let splices = kinds.iter().filter(|kind| **kind == InvocationArgumentKind::PositionalSplice).count();
            let expected = possible(&parameters, &kinds, &mut Vec::new(), splices);
            assert_eq!(graph.plan_invocation_arguments(callback, &kinds).is_ok(), expected, "fixed={fixed},rest={rest},defaults={defaults},kinds={kinds:?}");
        } }
    } } }
}

#[test]
fn generic_fixed_splice_invocation_instantiates_binding_proofs_independently() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let (scheme, original) = apply_invocation_scheme(&mut graph, InvocationArgumentKind::PositionalSplice, CallableDomain::Pure, why);
    let original = graph.resolved(original).unwrap();
    for atom in [Atom::Int, Atom::Str] {
        let item = graph.atom(atom).unwrap(); let parts = graph.list(item).unwrap();
        let callback = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern("a"), ty: item, defaulted: false, rest: false }, Parameter { label: Name::intern("b"), ty: item, defaulted: false, rest: false }], result: item, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
        let instance = graph.instantiate(scheme, 0, why).unwrap(); let TypeNode::Arrow(signature) = graph.node(instance.ty).unwrap().clone() else { panic!() };
        graph.unify(signature.params[0].ty, callback, why).unwrap(); graph.unify(signature.params[1].ty, parts, why).unwrap(); graph.solve().unwrap();
        let evidence = graph.invocation_evidence(instance.requirements[0]).unwrap().unwrap(); assert!(evidence.unique_plan().unwrap().1.dynamic.is_some()); assert_eq!(graph.resolved(evidence.result).unwrap(), item);
        assert_eq!(graph.resolved(original).unwrap(), original);
    }
}

#[test]
fn ground_forwarded_operation_seals_computed_effect_relations_before_generalization() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap();
    let input = graph.fresh_effect_at(1, None).unwrap(); let output = graph.fresh_derived_effect_at(1, None).unwrap();
    graph.include_effects(EffectSummary::Variable(input), EffectSummary::Variable(output), why).unwrap();
    let native = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: int, effects: EffectSummary::Variable(output) }).unwrap();
    let native = graph.generalize_with_effect_roots(native, 0, Generalization::Allowed, &[], &[EffectSummary::Variable(input)]).unwrap();
    let binder = graph.scheme_effect_binder_index(native, graph.scheme(native).unwrap().effect_roots[0]).unwrap().unwrap();
    let candidate = graph.register_candidate(CandidateTemplate { failure_projection: None, identity: Name::intern("forwarded-role"), public_label: Name::intern("forwarded-role"), effect_roles: vec![(EffectRole::Pull { source: 0 }, EffectRoleReference::Binder(binder as u32))], output_effect_roles: vec![], scheme: native, has_receiver: false, actual_eligibility: vec![], argument_relations: vec![] }).unwrap();
    let family = graph.register_family(&[candidate]).unwrap();
    for actual in [EffectSummary::Closed(EffectSet::TIME), EffectSummary::Closed(EffectSet::ENV), EffectSummary::Unknown] { for component in [false, true] {
        let execution = graph.fresh_derived_effect_at(1, None).unwrap();
        let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![], result: int, effects: EffectSummary::Variable(execution), effect_bindings: vec![(EffectRole::Pull { source: 0 }, actual)], output_effect_bindings: vec![] }, why).unwrap();
        let caller = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: int, effects: actual }).unwrap();
        let schemes = if component { graph.generalize_component(&[ComponentMember { root: caller, requirements: vec![requirement], policy: Generalization::Allowed }, ComponentMember { root: caller, requirements: vec![requirement], policy: Generalization::Allowed }], 0, None).unwrap() } else { vec![graph.generalize(caller, 0, Generalization::Allowed, &[requirement]).unwrap()] };
        for caller in schemes { assert!(graph.scheme(caller).unwrap().requirements.is_empty()); assert!(graph.scheme(caller).unwrap().effect_quantifiers.is_empty()); }
        let evidence = graph.candidate_evidence(requirement).unwrap().unwrap(); assert_eq!(graph.resolved_effect_summary(evidence.effects).unwrap(), actual);
    } }
    assert_eq!(graph.scheme(native).unwrap().effect_quantifiers.len(), 2);
}

#[test]
fn finite_input_ports_constrain_all_equal_forwarded_effect_ports() {
    for bits in [EffectSet::TIME, EffectSet::ENV] {
        let mut graph = InferenceContext::default(); let why = reason(&mut graph);
        let original = graph.fresh_effect_at(1, None).unwrap(); let forwarded = graph.fresh_effect_at(1, None).unwrap(); let actual = graph.fresh_effect_at(1, Some(bits)).unwrap(); let computed = graph.fresh_derived_effect_at(1, None).unwrap();
        graph.grow_effect(actual, bits).unwrap();
        graph.equate_effects(EffectSummary::Variable(original), EffectSummary::Variable(forwarded), why).unwrap();
        graph.include_effects(EffectSummary::Variable(original), EffectSummary::Variable(computed), why).unwrap();
        graph.equate_effects(EffectSummary::Variable(forwarded), EffectSummary::Variable(actual), why).unwrap();
        graph.seal_derived_effects(&[EffectSummary::Variable(computed)]).unwrap();
        for id in [original, forwarded, actual, computed] { assert_eq!(graph.resolved_effect_summary(EffectSummary::Variable(id)).unwrap(), EffectSummary::Closed(bits)); }
    }
}

#[test]
fn exact_latent_port_binding_preserves_a_calculated_summary_beneath_its_permission_cap() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let allowed = EffectSet(EffectSet::TIME.0 | EffectSet::ENV.0);
    let calculated = graph.fresh_derived_effect_at(0, Some(allowed)).unwrap();
    graph.include_effects(EffectSummary::Closed(EffectSet::ENV), EffectSummary::Variable(calculated), why).unwrap();
    let formal = graph.fresh_effect_at(1, None).unwrap();
    graph.equate_effects(EffectSummary::Variable(formal), EffectSummary::Variable(calculated), why).unwrap();
    graph.seal_derived_effects(&[EffectSummary::Variable(calculated)]).unwrap();
    for id in [formal, calculated] { assert_eq!(graph.resolved_effect_summary(EffectSummary::Variable(id)).unwrap(), EffectSummary::Closed(EffectSet::ENV)); }

    let input = graph.fresh_effect_at(1, Some(EffectSet::IO)).unwrap(); let calculated = graph.fresh_derived_effect_at(1, Some(allowed)).unwrap();
    graph.include_effects(EffectSummary::Closed(EffectSet::ENV), EffectSummary::Variable(calculated), why).unwrap();
    graph.equate_effects(EffectSummary::Variable(input), EffectSummary::Variable(calculated), why).unwrap();
    graph.seal_derived_effects(&[EffectSummary::Variable(input)]).unwrap();
    assert_eq!(graph.resolved_effect_summary(EffectSummary::Variable(input)).unwrap(), EffectSummary::Closed(EffectSet::ENV));

    let latent = graph.fresh_effect_at(1, None).unwrap(); let dependent = graph.fresh_derived_effect_at(1, None).unwrap();
    graph.include_effects(EffectSummary::Variable(latent), EffectSummary::Variable(dependent), why).unwrap();
    graph.equate_effects(EffectSummary::Variable(latent), EffectSummary::Variable(dependent), why).unwrap();
    graph.seal_derived_effects(&[EffectSummary::Variable(dependent)]).unwrap();
    assert!(matches!(graph.resolved_effect_summary(EffectSummary::Variable(latent)).unwrap(), EffectSummary::Variable(_)));
    assert!(matches!(graph.resolved_effect_summary(EffectSummary::Variable(dependent)).unwrap(), EffectSummary::Variable(_)));

    let input = graph.fresh_effect_at(1, None).unwrap(); let shell = graph.fresh_derived_effect_at(1, None).unwrap();
    graph.equate_effects(EffectSummary::Variable(shell), EffectSummary::Variable(input), why).unwrap();
    graph.seal_derived_effects(&[EffectSummary::Variable(shell)]).unwrap();
    assert!(matches!(graph.resolved_effect_summary(EffectSummary::Variable(input)).unwrap(), EffectSummary::Variable(_)));
}

#[test]
fn effect_identity_aliases_preserve_fresh_scheme_ports_and_rewind_failed_probes() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap();
    let actual = graph.fresh_effect_at(1, None).unwrap(); let formal = graph.fresh_effect_at(1, None).unwrap();
    let output = graph.fresh_derived_effect_at(1, None).unwrap();
    graph.include_effects(EffectSummary::Variable(actual), EffectSummary::Variable(output), why).unwrap();
    let requirement = graph.include_effects_masked(EffectSummary::Variable(formal), EffectSummary::Variable(output), EffectSet::EMPTY, why).unwrap();
    let watchers = graph.effects[actual.index()].value.watchers.clone(); let work = graph.counters().work_units;
    let failed: Result<(), _> = graph.probe(|graph| {
        graph.equate_effects(EffectSummary::Variable(formal), EffectSummary::Variable(actual), why)?;
        graph.equate_effects(EffectSummary::Variable(formal), EffectSummary::Closed(EffectSet::TIME), why)?;
        graph.solve()?;
        Err(InferenceError::Boundary("failed source transaction"))
    });
    assert!(failed.is_err()); assert!(graph.counters().work_units > work);
    for id in [formal, actual, output] { assert_eq!(graph.resolved_effect_summary(EffectSummary::Variable(id)).unwrap(), EffectSummary::Variable(id)); }
    assert_eq!(graph.effects[actual.index()].value.watchers, watchers);
    graph.equate_effects(EffectSummary::Variable(formal), EffectSummary::Variable(actual), why).unwrap();
    let root = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: int, effects: EffectSummary::Variable(output) }).unwrap();
    let scheme = graph.generalize_with_effect_roots(root, 0, Generalization::Allowed, &[requirement], &[EffectSummary::Variable(formal)]).unwrap();
    assert_eq!(graph.scheme(scheme).unwrap().effect_quantifiers.len(), 2);
    for actual in [EffectSummary::Closed(EffectSet::TIME), EffectSummary::Closed(EffectSet::ENV), EffectSummary::Unknown] {
        let instance = graph.instantiate(scheme, 0, why).unwrap();
        graph.equate_effects(instance.effect_roots[0], actual, why).unwrap(); graph.solve().unwrap();
        let TypeNode::Arrow(arrow) = graph.node(instance.ty).unwrap() else { panic!() }; let effects = arrow.effects;
        graph.seal_derived_effects(&[effects]).unwrap(); assert_eq!(graph.resolved_effect_summary(effects).unwrap(), actual);
    }
}

#[test]
fn operation_output_role_failure_retains_its_permission_provenance() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap();
    let signature = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: int, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
    let scheme = graph.generalize_with_effect_roots(signature, 0, Generalization::Allowed, &[], &[EffectSummary::Closed(EffectSet::TIME)]).unwrap();
    let candidate = graph.register_candidate(CandidateTemplate { failure_projection: None, identity: Name::intern("calculated-output-budget"), public_label: Name::intern("calculated-output-budget"), scheme, has_receiver: false, effect_roles: vec![], output_effect_roles: vec![(ProducerRole::Pull, 0)], actual_eligibility: vec![], argument_relations: vec![] }).unwrap();
    let family = graph.register_family(&[candidate]).unwrap(); let output = graph.fresh_derived_effect_at(1, Some(EffectSet::ENV)).unwrap();
    let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![], result: int, effects: EffectSummary::Closed(EffectSet::EMPTY), effect_bindings: vec![], output_effect_bindings: vec![(ProducerRole::Pull, EffectSummary::Variable(output))] }, why).unwrap();
    assert_eq!(graph.solve(), Err(InferenceError::OperationEffectViolation { requirement, required: Some(EffectSet::TIME), available: Some(EffectSet::ENV) }));
    assert_eq!(graph.effect_value(output).unwrap(), EffectSet::EMPTY); assert!(graph.candidate_evidence(requirement).unwrap().is_none());
}

#[test]
fn effect_upper_propagation_rewinds_on_work_exhaustion_and_rejects_later_excess() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let mut ports = Vec::new();
    for _ in 0..8 { ports.push(graph.fresh_effect_at(1, None).unwrap()); }
    for pair in ports.windows(2) { graph.include_effects(EffectSummary::Variable(pair[0]), EffectSummary::Variable(pair[1]), why).unwrap(); }
    let work = graph.counters().work_units; graph.limits.work_units = work + 18;
    assert!(matches!(graph.include_effects(EffectSummary::Variable(*ports.last().unwrap()), EffectSummary::Closed(EffectSet::TIME), why), Err(InferenceError::Limit("solver work"))));
    for port in &ports { assert_eq!(graph.effects[port.index()].value.upper, None); }
    assert!(graph.counters().work_units > graph.limits.work_units);
    graph.limits.work_units = Limits::default().work_units;
    graph.include_effects(EffectSummary::Variable(*ports.last().unwrap()), EffectSummary::Closed(EffectSet::TIME), why).unwrap();
    for port in &ports { assert_eq!(graph.effects[port.index()].value.upper, Some(EffectSet::TIME)); }
    assert!(matches!(graph.include_effects(EffectSummary::Closed(EffectSet::ENV), EffectSummary::Variable(ports[0]), why), Err(InferenceError::EffectViolation)));
    for port in &ports { assert_eq!(graph.effect_value(*port).unwrap(), EffectSet::EMPTY); }
}

#[test]
fn yield_item_scheme_rejects_root_stream_and_keeps_nested_stream_data() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let item = graph.fresh(1, span()).unwrap();
    let requirement = graph.require_eligibility(Eligibility::YieldItem, item, why).unwrap(); let root = unary(&mut graph, item, item); let scheme = graph.generalize(root, 0, Generalization::Allowed, &[requirement]).unwrap();
    let int = graph.atom(Atom::Int).unwrap(); let stream = graph.stream(int).unwrap(); let list = graph.list(stream).unwrap(); let row = graph.row(vec![field("nested", stream)], None).unwrap(); let record = graph.record(row).unwrap(); let any = graph.atom(Atom::Any).unwrap();
    for actual in [int, list, record, any, stream] {
        graph.trial(|graph| {
            let instance = graph.instantiate(scheme, 0, why)?; let TypeNode::Arrow(signature) = graph.node(instance.ty)?.clone() else { panic!() };
            graph.unify(signature.params[0].ty, actual, why)?;
            let solved = graph.solve();
            if actual == stream { assert!(matches!(solved, Err(InferenceError::UnsupportedOperation(id)) if id == instance.requirements[0])); }
            else { solved?; assert!(graph.eligibility_satisfied(instance.requirements[0])?); }
            Ok(())
        }).unwrap();
    }
    assert_eq!(graph.scheme(scheme).unwrap().quantifiers.len(), 1);
}

#[test]
fn supported_operation_reports_its_permission_failure_without_retyping_the_operand() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let int = graph.atom(Atom::Int).unwrap(); let string = graph.atom(Atom::Str).unwrap(); let boolean = graph.atom(Atom::Bool).unwrap();
    let mut candidates = Vec::new();
    for (label, operand, effects) in [("integer-effect", int, EffectSet::TIME), ("string-effect", string, EffectSet::FS)] {
        let arrow = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("value"), ty: operand, defaulted: false, rest: false }], result: int, effects: EffectSummary::Closed(effects) }).unwrap(); let scheme = graph.generalize(arrow, 0, Generalization::Allowed, &[]).unwrap();
        candidates.push(graph.register_candidate(CandidateTemplate { failure_projection: None, identity: Name::intern(label), public_label: Name::intern(label), effect_roles: vec![], output_effect_roles: vec![], scheme, has_receiver: false, actual_eligibility: vec![], argument_relations: vec![ArgumentRelation::Exact] }).unwrap());
    }
    for reverse in [false, true] { for actual in [int, boolean] {
        graph.trial(|graph| {
            let ordered: Vec<_> = if reverse { candidates.iter().rev().copied().collect() } else { candidates.clone() }; let family = graph.register_family(&ordered)?;
            let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![Some(actual)], result: int, effects: EffectSummary::Closed(EffectSet::ENV), effect_bindings: vec![], output_effect_bindings: vec![] }, why)?;
            let error = graph.solve().unwrap_err();
            if actual == int { assert!(matches!(error, InferenceError::OperationEffectViolation { requirement: source, required: Some(EffectSet::TIME), available: Some(EffectSet::ENV) } if source == requirement)); }
            else { assert!(matches!(error, InferenceError::UnsupportedOperation(source) if source == requirement)); }
            assert!(graph.candidate_evidence(requirement)?.is_none()); Ok(())
        }).unwrap();
    } }
}

#[test]
fn native_command_admission_keeps_pending_actuals_and_selects_exact_text_domains() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let string = graph.atom(Atom::Str).unwrap(); let path = graph.atom(Atom::Path).unwrap(); let int = graph.atom(Atom::Int).unwrap(); let any = graph.atom(Atom::Any).unwrap(); let unit = graph.atom(Atom::Unit).unwrap();
    let strings = graph.list(string).unwrap(); let paths = graph.list(path).unwrap(); let anys = graph.list(any).unwrap(); let ints = graph.list(int).unwrap();
    let mut candidates = Vec::new();
    for (target_domain, target, argv_domain, argv, label) in [(CommandTextDomain::Str, string, CommandTextDomain::Str, strings, "str-str"), (CommandTextDomain::Str, string, CommandTextDomain::Path, paths, "str-path"), (CommandTextDomain::Path, path, CommandTextDomain::Str, strings, "path-str"), (CommandTextDomain::Path, path, CommandTextDomain::Path, paths, "path-path")] {
        let arrow = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("target"), ty: target, defaulted: false, rest: false }, Parameter { label: Name::intern("arguments"), ty: argv, defaulted: false, rest: false }], result: unit, effects: EffectSummary::Closed(EffectSet::PROCESS) }).unwrap(); let scheme = graph.generalize(arrow, 0, Generalization::Allowed, &[]).unwrap();
        let candidate = graph.register_candidate(CandidateTemplate { failure_projection: None, identity: Name::intern(label), public_label: Name::intern(label), effect_roles: vec![], output_effect_roles: vec![], scheme, has_receiver: false, actual_eligibility: vec![(0, Eligibility::CommandTarget), (1, Eligibility::CommandArgv)], argument_relations: vec![ArgumentRelation::CommandTarget { domain: target_domain }, ArgumentRelation::CommandArgv { element: argv_domain }] }).unwrap(); candidates.push(candidate);
    }
    let family = graph.register_family(&candidates).unwrap();
    graph.trial(|graph| {
        let target = graph.fresh(1, span())?; let argv = graph.fresh(1, span())?;
        let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![Some(target), Some(argv)], result: unit, effects: EffectSummary::Closed(EffectSet::PROCESS), effect_bindings: vec![], output_effect_bindings: vec![] }, why)?; graph.solve()?;
        assert_eq!(graph.resolved(target)?, target); assert_eq!(graph.resolved(argv)?, argv); assert!(graph.candidate_evidence(requirement)?.is_none()); Ok(())
    }).unwrap();
    for target in [string, path, any, int] { for argv in [strings, paths, anys, any, ints] {
        graph.trial(|graph| {
            let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![Some(target), Some(argv)], result: unit, effects: EffectSummary::Closed(EffectSet::PROCESS), effect_bindings: vec![], output_effect_bindings: vec![] }, why)?;
            if target == int || argv == ints { assert!(matches!(graph.solve(), Err(InferenceError::UnsupportedOperation(id)) if id == requirement)); }
            else { graph.solve()?; let evidence = graph.candidate_evidence(requirement)?.unwrap(); let index = usize::from(target == path) * 2 + usize::from(argv == paths); assert_eq!(evidence.candidate, candidates[index]); let RequirementTemplate::Operation { call, .. } = graph.requirement_template(requirement)? else { panic!() }; assert_eq!(graph.operation_call(call)?.arguments, vec![Some(target), Some(argv)]); }
            Ok(())
        }).unwrap();
    } }
}

#[test]
fn checked_error_and_display_predicates_preserve_their_finite_domains() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let name = Name::intern("LocalError");
    for atom in [Atom::Error, Atom::ProcessError, Atom::ErrorFamily(name), Atom::ErrorVariant { family: name, variant: Name::intern("Failed") }, Atom::ErrorFacet(name), Atom::Any, Atom::Int] {
        let ty = graph.atom(atom).unwrap(); graph.trial(|graph| {
            let requirement = graph.require_eligibility(Eligibility::Error, ty, why)?;
            if matches!(atom, Atom::Any | Atom::Int) { assert!(matches!(graph.solve(), Err(InferenceError::UnsupportedOperation(id)) if id == requirement)); }
            else { graph.solve()?; assert!(graph.eligibility_satisfied(requirement)?); }
            assert_eq!(graph.resolved(ty)?, ty); Ok(())
        }).unwrap();
    }
    for atom in [Atom::Any, Atom::Str, Atom::Int, Atom::UInt, Atom::Bool, Atom::Path, Atom::Duration, Atom::Float, Atom::Unit, Atom::Bytes, Atom::FsRoot] {
        let ty = graph.atom(atom).unwrap(); graph.trial(|graph| {
            let requirement = graph.require_eligibility(Eligibility::Display, ty, why)?;
            if matches!(atom, Atom::Unit | Atom::Bytes | Atom::FsRoot) { assert!(matches!(graph.solve(), Err(InferenceError::UnsupportedOperation(id)) if id == requirement)); }
            else { graph.solve()?; assert!(graph.eligibility_satisfied(requirement)?); }
            Ok(())
        }).unwrap();
    }
}

#[test]
fn operation_failure_projection_obeys_the_source_lexical_error_bound() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let int = graph.atom(Atom::Int).unwrap();
    let allowed = graph.atom(Atom::ErrorFamily(Name::intern("SourceFailure"))).unwrap();
    let unrelated = graph.atom(Atom::ErrorFamily(Name::intern("OtherFailure"))).unwrap();
    let error = graph.fresh(1, span()).unwrap(); let receiver = graph.result(int, error).unwrap();
    let signature = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("values"), ty: receiver, defaulted: false, rest: false }], result: int, effects: EffectSummary::Closed(EffectSet::ERROR) }).unwrap();
    let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap();
    let candidate = graph.register_candidate(CandidateTemplate { failure_projection: Some(OperationFailureProjection::ArgumentResultError { argument: 0 }), identity: Name::intern("iterate-result-error-bound"), public_label: Name::intern("iterate"), effect_roles: vec![], output_effect_roles: vec![], scheme, has_receiver: false, actual_eligibility: vec![], argument_relations: vec![ArgumentRelation::Assignable] }).unwrap();
    let family = graph.register_family(&[candidate]).unwrap();
    for (actual_error, bound, accepted) in [(allowed, allowed, true), (unrelated, allowed, false), (unrelated, graph.atom(Atom::Error).unwrap(), true)] {
        let actual = graph.result(int, actual_error).unwrap();
        let outcome = graph.trial(|graph| {
            let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: Some(bound), receiver: None, arguments: vec![Some(actual)], result: int, effects: EffectSummary::Closed(EffectSet::ERROR), effect_bindings: vec![], output_effect_bindings: vec![] }, why)?;
            graph.solve()?; assert!(graph.candidate_evidence(requirement)?.is_some()); Ok(())
        });
        assert_eq!(outcome.is_ok(), accepted, "actual error {actual_error:?}, lexical bound {bound:?}: {outcome:?}");
    }
}

#[test]
fn pending_error_bounds_survive_forwarding_and_independent_candidate_trials() {
    let owner = crate::symbol::SymbolOwner::new(); let _symbols = owner.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let int = graph.atom(Atom::Int).unwrap(); let string = graph.atom(Atom::Str).unwrap();
    let allowed = graph.atom(Atom::ErrorFamily(Name::intern("SourceFailure"))).unwrap();
    let unrelated = graph.atom(Atom::ErrorFamily(Name::intern("OtherFailure"))).unwrap();
    let error = graph.fresh(1, span()).unwrap(); let wrapped = graph.result(string, error).unwrap(); let mut candidates = Vec::new();
    for (label, parameter, projection) in [("plain-string", string, None), ("result-string", wrapped, Some(OperationFailureProjection::ArgumentResultError { argument: 0 }))] {
        let root = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("values"), ty: parameter, defaulted: false, rest: false }], result: int, effects: EffectSummary::Closed(EffectSet::ERROR) }).unwrap();
        let scheme = graph.generalize(root, 0, Generalization::Allowed, &[]).unwrap();
        candidates.push(graph.register_candidate(CandidateTemplate { failure_projection: projection, identity: Name::intern(label), public_label: Name::intern("iterate"), effect_roles: vec![], output_effect_roles: vec![], scheme, has_receiver: false, actual_eligibility: vec![], argument_relations: vec![ArgumentRelation::Assignable] }).unwrap());
    }
    let family = graph.register_family(&candidates).unwrap(); let source = graph.fresh(1, span()).unwrap(); let bound = graph.fresh(1, span()).unwrap(); let returned = graph.result(int, bound).unwrap();
    let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: Some(bound), receiver: None, arguments: vec![Some(source)], result: int, effects: EffectSummary::Closed(EffectSet::ERROR), effect_bindings: vec![], output_effect_bindings: vec![] }, why).unwrap();
    graph.solve().unwrap(); assert!(graph.candidate_evidence(requirement).unwrap().is_none()); assert_eq!(graph.resolved(source).unwrap(), source);
    let root = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("values"), ty: source, defaulted: false, rest: false }], result: returned, effects: EffectSummary::Closed(EffectSet::ERROR) }).unwrap();
    let scheme = graph.generalize(root, 0, Generalization::Allowed, &[requirement]).unwrap();
    let forwarded = graph.instantiate(scheme, 1, why).unwrap(); let forwarding_scheme = graph.generalize(forwarded.ty, 0, Generalization::Allowed, &forwarded.requirements).unwrap();
    assert_eq!(graph.scheme(forwarding_scheme).unwrap().requirements.len(), 1); assert_eq!(graph.scheme(forwarding_scheme).unwrap().quantifiers.len(), 2);
    let general_error = graph.atom(Atom::Error).unwrap();
    let mut instances = Vec::new();
    for (actual_error, lexical_bound, accepted) in [(unrelated, allowed, false), (allowed, allowed, true), (unrelated, general_error, true), (allowed, unrelated, false)] {
        let actual = graph.result(string, actual_error).unwrap(); let returned = graph.result(int, lexical_bound).unwrap(); let before = graph.counters().work_units;
        let outcome = graph.trial(|graph| {
            let instance = graph.instantiate(forwarding_scheme, 1, why)?;
            let TypeNode::Arrow(signature) = graph.node(instance.ty)?.clone() else { panic!() };
            let RequirementTemplate::Operation { call, .. } = graph.requirement_template(instance.requirements[0])? else { panic!() };
            let TypeNode::Result(_, fresh_bound) = *graph.node(signature.result)? else { panic!() };
            assert_eq!(graph.operation_call(call)?.declared_error_bound, Some(fresh_bound)); assert_ne!(graph.resolved(fresh_bound)?, graph.resolved(bound)?);
            graph.unify(signature.result, returned, why)?;
            graph.assignable(signature.params[0].ty, actual, why)?; graph.solve()?;
            assert!(graph.candidate_evidence(instance.requirements[0])?.unwrap().failure_assignability.is_some());
            instances.push(instance.ty); Ok(())
        });
        assert_eq!(outcome.is_ok(), accepted, "{outcome:?}"); assert!(graph.counters().work_units > before);
    }
    assert!(instances.iter().all(|instance| graph.node(*instance).is_err()));
    let roots = [ScopedRoot { ty: graph.scheme(scheme).unwrap().body, scope: Some(scheme) }, ScopedRoot { ty: graph.scheme(forwarding_scheme).unwrap().body, scope: Some(forwarding_scheme) }];
    graph.freeze_scoped(&roots).unwrap();
}

#[test]
fn command_expansion_accepts_only_scalar_items_or_one_list_layer() {
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let argv = [Atom::Str, Atom::Path, Atom::Int, Atom::UInt, Atom::Bool, Atom::Duration, Atom::Any];
    for atom in argv.into_iter().chain([Atom::Float, Atom::Bytes, Atom::Unit, Atom::Null, Atom::FsRoot]) {
        let item = graph.atom(atom).unwrap(); let list = graph.list(item).unwrap(); let nested = graph.list(list).unwrap(); let stream = graph.stream(item).unwrap();
        for (actual, accepted) in [(item, argv.contains(&atom)), (list, argv.contains(&atom)), (nested, false), (stream, false)] {
            graph.trial(|graph| {
                let requirement = graph.require_eligibility(Eligibility::ArgvExpansion, actual, why)?;
                let outcome = graph.solve();
                assert_eq!(outcome.is_ok(), accepted, "{atom:?} {actual:?}: {outcome:?}");
                if accepted { assert!(graph.eligibility_satisfied(requirement)?); }
                else { assert!(matches!(outcome, Err(InferenceError::UnsupportedOperation(id)) if id == requirement)); }
                assert_eq!(graph.resolved(actual)?, actual); Ok(())
            }).unwrap();
        }
    }
}

#[test]
fn command_expansion_stays_pending_and_freshens_without_argument_shape_training() {
    let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph); let argument = graph.fresh(1, span()).unwrap();
    let requirement = graph.require_eligibility(Eligibility::ArgvExpansion, argument, why).unwrap();
    graph.solve().unwrap(); assert!(!graph.eligibility_satisfied(requirement).unwrap()); assert_eq!(graph.resolved(argument).unwrap(), argument);
    let signature = unary(&mut graph, argument, argument); let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[requirement]).unwrap();
    assert_eq!(graph.scheme(scheme).unwrap().quantifiers.len(), 1); assert_eq!(graph.scheme(scheme).unwrap().requirements.len(), 1);
    let string = graph.atom(Atom::Str).unwrap(); let list = graph.list(string).unwrap(); let nested = graph.list(list).unwrap(); let stream = graph.stream(string).unwrap();
    for (actual, accepted) in [(string, true), (list, true), (nested, false), (stream, false)] {
        let work = graph.counters().work_units;
        let outcome = graph.trial(|graph| {
            let instance = graph.instantiate(scheme, 1, why)?; let TypeNode::Arrow(arrow) = graph.node(instance.ty)?.clone() else { panic!() };
            assert_ne!(graph.resolved(arrow.params[0].ty)?, graph.resolved(argument)?);
            graph.unify(arrow.params[0].ty, actual, why)?; graph.solve()?;
            assert!(graph.eligibility_satisfied(instance.requirements[0])?); Ok(())
        });
        assert_eq!(outcome.is_ok(), accepted, "{outcome:?}"); assert!(graph.counters().work_units > work);
    }
    let unknown_item = graph.fresh(1, span()).unwrap(); let list = graph.list(unknown_item).unwrap();
    assert!(!graph.check_eligibility(Eligibility::ArgvExpansion, list).unwrap()); assert_eq!(graph.resolved(unknown_item).unwrap(), unknown_item);
    let root = graph.scheme(scheme).unwrap().body; graph.freeze_scoped(&[ScopedRoot { ty: root, scope: Some(scheme) }]).unwrap();
}

#[test]
fn command_expansion_counts_the_list_child_depth_and_work() {
    let mut graph = InferenceContext::new(Limits { structural_depth: 0, ..Limits::default() });
    let string = graph.atom(Atom::Str).unwrap(); let list = graph.list(string).unwrap();
    assert!(matches!(graph.check_eligibility(Eligibility::ArgvExpansion, list), Err(InferenceError::Limit("structural depth"))));
    let mut graph = InferenceContext::default(); let string = graph.atom(Atom::Str).unwrap(); let list = graph.list(string).unwrap();
    let before = graph.counters().work_units; graph.limits.work_units = before + 1;
    assert!(matches!(graph.check_eligibility(Eligibility::ArgvExpansion, list), Err(InferenceError::Limit("solver work"))));
    assert!(graph.counters().work_units > graph.limits.work_units); assert_eq!(graph.resolved(list).unwrap(), list);
}

#[test]
fn failed_candidate_error_bounds_keep_live_source_handles_and_failure_phase() {
    let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let int = graph.atom(Atom::Int).unwrap(); let allowed = graph.atom(Atom::ErrorFamily(Name::intern("SourceFailure"))).unwrap(); let actual_error = graph.atom(Atom::ErrorFamily(Name::intern("OtherFailure"))).unwrap();
    let error = graph.fresh(1, span()).unwrap(); let parameter = graph.result(int, error).unwrap(); let projection = OperationFailureProjection::ArgumentResultError { argument: 0 };
    let signature = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: Name::intern("values"), ty: parameter, defaulted: false, rest: false }], result: int, effects: EffectSummary::Closed(EffectSet::ERROR) }).unwrap();
    let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap(); let candidate = graph.register_candidate(CandidateTemplate { failure_projection: Some(projection), identity: Name::intern("failed-error-bound"), public_label: Name::intern("iterate"), effect_roles: vec![], output_effect_roles: vec![], scheme, has_receiver: false, actual_eligibility: vec![], argument_relations: vec![ArgumentRelation::Assignable] }).unwrap();
    let family = graph.register_family(&[candidate]).unwrap();
    graph.trial(|graph| {
        let unsupported = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: Some(allowed), receiver: None, arguments: vec![Some(int)], result: int, effects: EffectSummary::Closed(EffectSet::ERROR), effect_bindings: vec![], output_effect_bindings: vec![] }, why)?;
        assert_eq!(graph.solve(), Err(InferenceError::UnsupportedOperation(unsupported))); Ok(())
    }).unwrap();
    let actual = graph.result(int, actual_error).unwrap();
    let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: Some(allowed), receiver: None, arguments: vec![Some(actual)], result: int, effects: EffectSummary::Closed(EffectSet::ERROR), effect_bindings: vec![], output_effect_bindings: vec![] }, why).unwrap();
    let retained = (graph.nodes.len(), graph.metas.len(), graph.operation_calls.len(), graph.origins.len()); let work = graph.counters().work_units;
    for _ in 0..2 {
        graph.enqueue(requirement).unwrap();
        assert_eq!(graph.solve(), Err(InferenceError::OperationErrorBoundViolation { requirement, bound: allowed, projection }));
        graph.node(allowed).unwrap(); graph.requirement_template(requirement).unwrap();
        assert_eq!((graph.nodes.len(), graph.metas.len(), graph.operation_calls.len(), graph.origins.len()), retained);
        assert_eq!(graph.resolved(actual).unwrap(), actual); assert!(graph.candidate_evidence(requirement).unwrap().is_none());
    }
    assert!(graph.counters().work_units > work);
}

#[test]
fn module_projection_retains_exact_written_callable_promise() {
    let symbols = crate::symbol::SymbolOwner::new(); let _guard = symbols.enter();
    let mut graph = InferenceContext::default(); let why = reason(&mut graph);
    let string = graph.atom(Atom::Str).unwrap(); let int = graph.atom(Atom::Int).unwrap();
    let render = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: Name::intern("value"), ty: string, defaulted: false, rest: false }, Parameter { label: Name::intern("suffix"), ty: string, defaulted: true, rest: false }], result: string, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
    let clock = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![], result: int, effects: EffectSummary::Closed(EffectSet::TIME) }).unwrap();
    let module = graph.module(vec![ModuleField { label: Name::intern("render"), ty: render, optional: true }, ModuleField { label: Name::intern("clock"), ty: clock, optional: false }]).unwrap();
    assert_eq!(graph.require_field(module, Name::intern("render"), 0, why).unwrap(), render);
    assert_eq!(graph.require_field(module, Name::intern("clock"), 0, why).unwrap(), clock);
    assert!(matches!(graph.constraint_origins()[0].relation, ConstraintRelation::ModuleProjection { module: actual, label, result, optional: true } if actual == module && label == Name::intern("render") && result == render));
    let origins = graph.constraint_origins().len();
    assert_eq!(graph.require_field(module, Name::intern("missing"), 0, why), Err(InferenceError::MissingField(Name::intern("missing"))));
    assert_eq!(graph.constraint_origins().len(), origins);
    graph.freeze_scoped(&[ScopedRoot { ty: module, scope: None }, ScopedRoot { ty: render, scope: None }, ScopedRoot { ty: clock, scope: None }]).unwrap();
}
