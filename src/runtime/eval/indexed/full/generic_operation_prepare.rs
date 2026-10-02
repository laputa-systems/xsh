use super::*;
use super::super::generic::{PreparedInvocationArgument, PreparedOperationAuthority, PreparedOperationEffects, ScopedOperationRequirement, ScopedOperationSource, ScopedOperationObligation, ScopedOperationWitness, ScopedOperationCode};
use crate::sema::check::SolvedTypes;
use crate::sema::inference::{EffectSet, EffectSummary, OperationBinding, OperationCallId, OperationFamilyId, RequirementId, RequirementTemplate, SchemeId, ScopedRequirementRoot, TypeNode};
use crate::sema::operation_graph::{PreparedLanguageOperation, ValueConstructor};

fn unavailable(message: &'static str) -> IrBuildError { IrBuildError::format(message, None, 0, 0) }

fn empty_effects(graph: &crate::sema::inference::InferenceContext, summary: EffectSummary) -> Result<EffectSet, IrBuildError> {
    if graph.closed_effect_summary(summary).map_err(|_| unavailable("scoped_operation_effect_owner"))? != EffectSummary::Closed(EffectSet::EMPTY) {
        return Err(unavailable("scoped_operation_effect_not_prepared"));
    }
    Ok(EffectSet::EMPTY)
}

fn same_arguments(graph: &crate::sema::inference::InferenceContext, original: &[Option<crate::sema::inference::TypeId>], selected: &[Option<crate::sema::inference::TypeId>]) -> Result<bool, IrBuildError> {
    if original.len() != selected.len() { return Ok(false); }
    for (original, selected) in original.iter().zip(selected) {
        let resolved = |ty: &Option<crate::sema::inference::TypeId>| ty.map(|ty| graph.resolved(ty).map_err(|_| unavailable("scoped_operation_argument_owner"))).transpose();
        if resolved(original)? != resolved(selected)? { return Ok(false); }
    }
    Ok(true)
}

fn authority(solved: &SolvedTypes, candidate: crate::sema::inference::CandidateId) -> Result<PreparedOperationAuthority, IrBuildError> {
    let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(&solved.graph, candidate).map_err(|_| unavailable("scoped_operation_candidate_authority"))? else { return Err(unavailable("scoped_operation_nonlanguage_authority")); };
    if !matches!(metadata.operation, PreparedLanguageOperation::Constructor { kind: ValueConstructor::Ok | ValueConstructor::Err, arity: 1 }) {
        return Err(unavailable("scoped_operation_constructor_not_prepared"));
    }
    Ok(PreparedOperationAuthority::Language { identity: metadata.identity, authority: metadata.authority, operation: metadata.operation, argument_order: metadata.argument_order, statement_result_is_unit: metadata.statement_result_is_unit })
}

fn constructor_operand(tag: FullTag, code: ScopedOperationCode, words: &[u32]) -> Option<u32> {
    match (tag, code, words) {
        (FullTag::ExprOk, ScopedOperationCode::ResultOk, [operand])
        | (FullTag::ExprErr, ScopedOperationCode::ResultErr, [operand, 0]) => Some(*operand),
        _ => None,
    }
}

impl FullBuilder {
    pub(super) fn prepare_scoped_operation_requirement(&mut self, solved: &SolvedTypes, scheme: SchemeId, family: OperationFamilyId, call: OperationCallId) -> Result<Requirement, IrBuildError> {
        let graph = &solved.graph;
        let original = graph.scheme(scheme).map_err(|_| unavailable("scoped_operation_original_scheme"))?;
        let index = original.requirements.iter().position(|template| matches!(*template, RequirementTemplate::Operation { family: original_family, call: original_call } if family == original_family && call == original_call)).ok_or_else(|| unavailable("scoped_operation_original_requirement"))?;
        let requirement = *original.requirement_origins.get(index).ok_or_else(|| unavailable("scoped_operation_original_origin"))?;
        graph.validate_requirement_scoped(ScopedRequirementRoot { requirement, scope: Some(scheme) }).map_err(|_| unavailable("scoped_operation_original_scope"))?;
        let selected = graph.candidate_evidence(requirement).map_err(|_| unavailable("scoped_operation_original_evidence"))?.ok_or_else(|| unavailable("scoped_operation_original_pending"))?;
        let candidate = graph.candidate(selected.candidate).map_err(|_| unavailable("scoped_operation_candidate"))?;
        if !graph.family(family).map_err(|_| unavailable("scoped_operation_family"))?.contains(&selected.candidate)
            || candidate.has_receiver || !candidate.actual_eligibility.is_empty() || !candidate.effect_roles.is_empty() || !candidate.output_effect_roles.is_empty()
            || candidate.failure_projection.is_some() || !selected.dependencies.is_empty() || !selected.callback_invocations.is_empty() || selected.binding.is_some()
            || !selected.effect_roots.is_empty() || !selected.effect_substitutions.is_empty() { return Err(unavailable("scoped_operation_protocol_not_prepared")); }
        let original_call = graph.operation_call(call).map_err(|_| unavailable("scoped_operation_call_owner"))?;
        if original_call.binding != OperationBinding::Slots || original_call.receiver.is_some() || original_call.mono_authority.is_some() || original_call.declared_error_bound.is_some()
            || original_call.arguments.len() != 1 || !original_call.effect_bindings.is_empty() || !original_call.output_effect_bindings.is_empty() { return Err(unavailable("scoped_operation_binding_not_prepared")); }
        let TypeNode::Arrow(signature) = graph.node(graph.resolved(selected.signature).map_err(|_| unavailable("scoped_operation_signature_owner"))?).map_err(|_| unavailable("scoped_operation_signature_owner"))? else { return Err(unavailable("scoped_operation_signature")); };
        if signature.kind != crate::sema::inference::CallableKind::Pure || signature.params.len() != 1 || signature.params[0].defaulted || signature.params[0].rest
            || selected.actual_arguments.len() != 1 || !same_arguments(graph, &original_call.arguments, &selected.actual_arguments)? { return Err(unavailable("scoped_operation_original_arguments")); }
        let reference = |builder: &mut FullBuilder, ty| builder.generic.get_or_insert_with(GenericEvidenceBuilder::default).prepare_reference(graph, scheme, ty, &mut builder.store.semantic, &mut builder.semantic).map_err(|_| unavailable("scoped_operation_type_scope"));
        let expected = ScopedOperationRequirement {
            authority: authority(solved, selected.candidate)?,
            signature: reference(self, selected.signature)?,
            arguments: selected.actual_arguments.iter().map(|ty| reference(self, ty.ok_or_else(|| unavailable("scoped_operation_missing_argument"))?)).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(),
            result: reference(self, selected.result)?,
            effects: PreparedOperationEffects { creation: empty_effects(graph, selected.effects)?, inputs: Box::new([]), outputs: Box::new([]) },
        };
        if reference(self, original_call.result)? != expected.result || reference(self, signature.result)? != expected.result || reference(self, signature.params[0].ty)? != expected.arguments[0] {
            return Err(unavailable("scoped_operation_original_constructor_relation"));
        }
        empty_effects(graph, signature.effects)?;
        Ok(Requirement::Operation(expected))
    }

    pub(super) fn prepare_scoped_operation_sources(&mut self, solved: &SolvedTypes) -> Result<(), IrBuildError> {
        let origins = self.generic_expression_rows.iter().map(|&(instruction, expression, owner)| (instruction, (expression, owner))).collect::<FxHashMap<_, _>>();
        let mut work = 0;
        for (instruction, origin, owner) in self.generic_expression_rows.clone() {
            let Some(operation) = solved.operations.get(&origin) else { continue; };
            let Some(scope) = operation.caller.and_then(|caller| self.generic_declarations.get(&caller).copied()) else { continue; };
            let scheme = self.generic_schemes[&scope];
            let original_scheme = solved.graph.scheme(scheme).map_err(|_| unavailable("scoped_operation_source_scheme"))?;
            let Some(index) = original_scheme.requirement_origins.iter().position(|&requirement| requirement == operation.requirement) else { continue; };
            let RequirementTemplate::Operation { family, call } = original_scheme.requirements[index] else { continue; };
            let Requirement::Operation(expected) = self.prepare_scoped_operation_requirement(solved, scheme, family, call)? else { unreachable!() };
            if self.generic.as_ref().unwrap().scope(scope).map_err(|_| unavailable("scoped_operation_source_scope"))?.requirements.get(index) != Some(&Requirement::Operation(expected.clone()))
                || owner != InstructionOwner::Function(self.generic.as_ref().unwrap().scope(scope).unwrap().owner)
                || operation.binding.supplied_slots != [0] || !operation.binding.default_slots.is_empty() || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some() || !operation.argument_coercions.is_empty() { return Err(unavailable("scoped_operation_original_instruction")); }
            let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| unavailable("scoped_operation_instruction_payload"))?;
            let operand = constructor_operand(self.store.tags[instruction as usize], expected.code().map_err(|_| unavailable("scoped_operation_original_code"))?, words)
                .ok_or_else(|| unavailable("scoped_operation_original_operand_count"))?;
            let recipes = solved.argument_sources.get(&origin).ok_or_else(|| unavailable("scoped_operation_original_argument_source"))?;
            let [recipe] = recipes.as_slice() else { return Err(unavailable("scoped_operation_original_argument_count")); };
            let crate::sema::arguments::ArgumentValueSource::Expression(expression) = recipe.value else { return Err(unavailable("scoped_operation_original_argument_form")); };
            let &(actual_origin, actual_owner) = origins.get(&operand).ok_or_else(|| unavailable("scoped_operation_operand_origin"))?;
            if actual_origin.expression != expression || actual_origin.source != origin.source || actual_origin.namespace != origin.namespace || actual_owner != owner || recipe.name.is_some() {
                return Err(unavailable("scoped_operation_original_operand"));
            }
            let obligations = self.scoped_operation_obligations(solved, operation.requirement, &mut work)?.into_boxed_slice();
            let source = ScopedOperationSource { origin, instruction, scope, requirement: index as u32, original_requirement: operation.requirement, obligations,
                arguments: vec![PreparedInvocationArgument { original: recipe.clone(), instruction: operand, ty: expected.arguments[0] }].into_boxed_slice(), expected };
            self.generic_evidence_mut().add_scoped_operation_source(source).map_err(|_| unavailable("scoped_operation_source_allocation"))?;
            self.generic_evidence_mut().add_requirement_use(super::super::generic::SolvedRequirementUse { instruction, scope, requirement: index as u32 });
        }
        Ok(())
    }

    pub(super) fn prepare_scoped_operation_witness(&mut self, scope: SchemeScopeId, index: usize, contextual: &FxHashMap<RequirementId, RequirementId>) -> Result<RequirementWitness, IrBuildError> {
        let (source_id, source, original) = self.generic.as_ref().unwrap().scoped_operation_sources().find_map(|(id, source)| {
            source.obligations.iter().find(|obligation| obligation.scope == scope && obligation.requirement as usize == index).map(|obligation| (id, source.clone(), obligation.immediate_original))
        }).ok_or_else(|| unavailable("scoped_operation_original_source_missing"))?;
        let actual = *contextual.get(&original).ok_or_else(|| unavailable("scoped_operation_contextual_origin_missing"))?;
        let solved = self.solved.clone().ok_or_else(|| unavailable("scoped_operation_solved_owner"))?;
        let graph = &solved.graph;
        if operation_ancestry(graph, actual, original, &mut 0)?.is_none() { return Err(unavailable("scoped_operation_contextual_origin")); }
        let selected = graph.candidate_evidence(actual).map_err(|_| unavailable("scoped_operation_contextual_evidence"))?.ok_or_else(|| unavailable("scoped_operation_contextual_pending"))?;
        let RequirementTemplate::Operation { family, call } = graph.requirement_template(actual).map_err(|_| unavailable("scoped_operation_contextual_template"))? else { return Err(unavailable("scoped_operation_contextual_kind")); };
        let call = graph.operation_call(call).map_err(|_| unavailable("scoped_operation_contextual_call"))?;
        if !graph.family(family).map_err(|_| unavailable("scoped_operation_contextual_family"))?.contains(&selected.candidate)
            || authority(&solved, selected.candidate)? != source.expected.authority || call.binding != OperationBinding::Slots || call.receiver.is_some()
            || !same_arguments(graph, &call.arguments, &selected.actual_arguments)? || selected.actual_arguments.len() != 1 || selected.binding.is_some() || !selected.dependencies.is_empty()
            || !selected.callback_invocations.is_empty() || !selected.effect_roots.is_empty() || !selected.effect_substitutions.is_empty()
            || !call.effect_bindings.is_empty() || !call.output_effect_bindings.is_empty() { return Err(unavailable("scoped_operation_contextual_authority")); }
        let closed = |builder: &mut FullBuilder, ty| {
            let reference = builder.generic.get_or_insert_with(GenericEvidenceBuilder::default).prepare_closed_reference(graph, ty, &mut builder.store.semantic, &mut builder.semantic).map_err(|_| unavailable("scoped_operation_contextual_type"))?;
            builder.generic.as_ref().unwrap().materialize_reference(reference, &[], &mut builder.store.semantic, &mut builder.semantic).map_err(|_| unavailable("scoped_operation_contextual_materialization"))
        };
        let signature = closed(self, selected.signature)?;
        let arguments = selected.actual_arguments.iter().map(|ty| closed(self, ty.ok_or_else(|| unavailable("scoped_operation_contextual_argument"))?)).collect::<Result<Vec<_>, _>>()?;
        let result = closed(self, selected.result)?;
        if closed(self, call.result)? != result { return Err(unavailable("scoped_operation_contextual_result")); }
        let witness = ScopedOperationWitness { source: source_id, signature, arguments: arguments.into_boxed_slice(), result, operation: source.expected.code().map_err(|_| unavailable("scoped_operation_contextual_code"))?,
            authority: source.expected.authority, effects: PreparedOperationEffects { creation: empty_effects(graph, selected.effects)?, inputs: Box::new([]), outputs: Box::new([]) } };
        Ok(RequirementWitness::Operation(self.generic_evidence_mut().add_scoped_operation_witness(witness).map_err(|_| unavailable("scoped_operation_witness_allocation"))?))
    }

    fn scoped_operation_obligations(&self, solved: &SolvedTypes, original: RequirementId, work: &mut usize) -> Result<Vec<ScopedOperationObligation>, IrBuildError> {
        let mut members = self.generic_schemes.iter().map(|(&scope, &scheme)| (scope, scheme)).collect::<Vec<_>>();
        members.sort_unstable_by_key(|(scope, _)| self.generic.as_ref().unwrap().scope(*scope).map(|scope| scope.owner.raw()).unwrap_or(u32::MAX));
        let mut obligations = Vec::new();
        for (scope, scheme) in members {
            let member = self.generic.as_ref().unwrap().scope(scope).map_err(|_| unavailable("scoped_operation_forwarded_scope"))?;
            let scheme = solved.graph.scheme(scheme).map_err(|_| unavailable("scoped_operation_forwarded_scheme"))?;
            for (index, (&immediate, template)) in scheme.requirement_origins.iter().zip(&scheme.requirements).enumerate() {
                *work = work.checked_add(1).ok_or_else(|| unavailable("scoped_operation_origin_work"))?;
                if *work > 2_000_000 { return Err(unavailable("scoped_operation_origin_work")); }
                if !matches!(template, RequirementTemplate::Operation { .. }) { continue; }
                let Some(ancestry) = operation_ancestry(&solved.graph, immediate, original, work)? else { continue; };
                let Some(Requirement::Operation(expected)) = member.requirements.get(index) else { return Err(unavailable("scoped_operation_forwarded_kind")); };
                obligations.push(ScopedOperationObligation { scope, requirement: index as u32, immediate_original: immediate, ancestry: ancestry.into_boxed_slice(), expected: expected.clone() });
            }
        }
        Ok(obligations)
    }
}

fn operation_ancestry(graph: &crate::sema::inference::InferenceContext, mut current: RequirementId, original: RequirementId, work: &mut usize) -> Result<Option<Vec<RequirementId>>, IrBuildError> {
    let mut ancestry = Vec::new();
    for _ in 0..256 {
        *work = work.checked_add(1).ok_or_else(|| unavailable("scoped_operation_origin_work"))?;
        if *work > 2_000_000 { return Err(unavailable("scoped_operation_origin_work")); }
        if ancestry.contains(&current) { return Err(unavailable("scoped_operation_origin_cycle")); }
        ancestry.push(current);
        if current == original { return Ok(Some(ancestry)); }
        let next = graph.requirement_source(current).map_err(|_| unavailable("scoped_operation_origin_owner"))?;
        if next == current { return Ok(None); }
        current = next;
    }
    Err(unavailable("scoped_operation_origin_depth"))
}

impl FullVerifier {
    pub(super) fn verify_scoped_operation_instruction(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32) -> Result<(), IrVerifyError> {
        let id = generic.scoped_operation_source_at(instruction)?.ok_or_else(|| IrVerifyError::new("scoped operation source is missing"))?;
        let source = generic.scoped_operation_source(id)?;
        let tag = *store.tags.get(instruction as usize).ok_or_else(|| IrVerifyError::new("scoped operation instruction is missing"))?;
        if source.arguments.len() != 1 || constructor_operand(tag, source.expected.code()?, store.payload(store.data[instruction as usize].range())?) != Some(source.arguments[0].instruction) {
            return Err(IrVerifyError::new("scoped operation changes its original constructor instruction"));
        }
        Self::verify_generic_symbolic_source(store, generic, source.arguments[0].instruction, source.scope, source.arguments[0].ty, &mut Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;

    #[test]
    fn scoped_ok_retains_original_operation_and_contextual_certificates() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure preserved(value) { match Ok(value) { Ok(original) => original, Err(_) => value } }\nprint ${preserved(7)}\nprint ${preserved(\"word\")}\n";
            let (_, parsed) = crate::loader::parse_load_entry_source_arena_only(
                "scoped-ok-certificates.xsh", crate::loader::entry_source_from_text("scoped-ok-certificates.xsh", source.to_owned()), Vec::new());
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let solved = Arc::clone(&checked.solved);
            drop(checked); drop(parsed);
            let _symbols = solved.symbol_owner().enter();
            let graph = &solved.graph;
            let counters = graph.counters().clone();
            assert_eq!(solved.operations.len(), 1);
            let operation = solved.operations.values().next().unwrap();
            let selected = graph.candidate_evidence(operation.requirement).unwrap().unwrap();
            assert!(matches!(authority(&solved, selected.candidate).unwrap(), PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Constructor { kind: ValueConstructor::Ok, arity: 1 }, .. }));
            let declaration = &solved.declarations[&operation.caller.unwrap()];
            let scheme = graph.scheme(declaration.scheme).unwrap();
            assert_eq!(scheme.requirements.len(), 1);
            assert_eq!(scheme.requirement_origins, [operation.requirement]);
            assert!(graph.candidate_evidence(scheme.requirement_origins[0]).unwrap().is_some());
            assert_eq!(graph.counters(), &counters);
        });
    }

    const FORWARDED: &str = "pure preserved(value, unused) { let observation = unused; match Ok(value) { Ok(original) => original, Err(_) => value } }\npure forwarded(value, unused) { preserved(value, unused) }\nprint ${forwarded(7, false)}\nprint ${forwarded(\"word\", 9)}\nprint ${forwarded(\"next\", true)}\nprint ${forwarded(11, \"other\")}\n";

    const FORWARDED_ERR: &str = "pure preserved(value, unused) { let observation = unused; match Err(value) { Err(original) => original, Ok(original) => original } }\npure forwarded(value, unused) { preserved(value, unused) }\nprint ${forwarded(7, false)}\nprint ${forwarded(\"word\", 9)}\n";

    #[test]
    fn scoped_err_forwarding_preserves_error_payload_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            for recursive in [false, true] {
                let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                    "scoped-err-forwarding.xsh", crate::loader::entry_source_from_text("scoped-err-forwarding.xsh", FORWARDED_ERR.to_owned()), Vec::new());
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let source_id = SourceMap::files(&sources).first().unwrap().id();
                let checked = Checker::check_arena(&parsed.arena, FORWARDED_ERR);
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                let weak = Arc::downgrade(&checked.solved);
                let counters = checked.solved.graph.counters().clone();
                let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
                let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
                assert_eq!(checked.solved.graph.counters(), &counters);
                drop(checked); drop(parsed);
                assert!(weak.upgrade().is_none());
                let program = evaluator.indexed_program.as_ref().unwrap();
                let generic = program.generic_evidence().unwrap();
                let (source_id, source) = generic.scoped_operation_sources().next().unwrap();
                assert_eq!(generic.scoped_operation_sources().count(), 1);
                assert_eq!(program.store.tags[source.instruction as usize], FullTag::ExprErr);
                for (_, instance) in generic.instances().filter(|(_, instance)| instance.scope == source.scope) {
                    let RequirementWitness::Operation(witness) = instance.requirements[source.requirement as usize] else { panic!(); };
                    let witness = generic.scoped_operation_witness(witness).unwrap();
                    assert_eq!(witness.source, source_id);
                    assert_eq!(witness.operation, ScopedOperationCode::ResultErr);
                    assert_eq!(program.store.semantic.type_children(witness.result).unwrap().unwrap().1, Some(witness.arguments[0]));
                }
                let symbols = program.symbol_owner().clone();
                let target = symbols.with_current(|| LoweredFunctionKey::Name(Name::intern("preserved")));
                let execute = || symbols.with_current(|| evaluator.try_eval_installed_compact_indexed_only_inner(plan)
                    .unwrap_or_else(|_| panic!("prepared error forwarding program remains installed")));
                let output = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(target, recursive, execute);
                assert_eq!(output.status, 0, "{:?}; {:?}", output.diagnostics, output.traceback);
                assert_eq!(output.stdout, b"7\nword\n");
                assert!(output.stderr.is_empty()); assert!(output.diagnostics.is_empty());
            }
        });
    }

    #[test]
    fn scoped_err_rejects_changed_constructor_payload_and_active_owner() {
        crate::runtime::eval::run_eval(|| {
            let program = prepared(FORWARDED_ERR);
            let _symbols = program.symbol_owner().enter();
            let generic = program.generic_evidence().unwrap();
            let (source_id, source) = generic.scoped_operation_sources().next().unwrap();
            let scope = generic.scope(source.scope).unwrap();
            let other = program.store.function_instruction_range(scope.owner.index()).unwrap().find(|&instruction| {
                program.store.tags[instruction] == FullTag::ExprParam && program.store.payload(program.store.data[instruction].range()).unwrap() == [1]
            }).unwrap();
            let range = program.store.data[source.instruction as usize].range().bounds(program.store.extra.len()).unwrap();
            let mut swapped = program.clone();
            swapped.store.extra[range.start] = other as u32;
            assert!(FullVerifier::verify(&swapped).is_err(), "Err retains the error operand's original binder");

            let mut changed_kind = program.clone();
            changed_kind.store.tags[source.instruction as usize] = FullTag::ExprOk;
            assert!(FullVerifier::verify(&changed_kind).is_err(), "equal Result branches do not authorize another constructor");

            let (instance_id, instance) = generic.instances().find(|(_, instance)| instance.scope == source.scope).unwrap();
            let RequirementWitness::Operation(witness_id) = instance.requirements[source.requirement as usize] else { panic!() };
            let mut changed_witness = program.clone();
            changed_witness.store.generic.as_mut().unwrap().test_scoped_operation_witness_mut(witness_id).unwrap().operation = ScopedOperationCode::ResultOk;
            assert!(FullVerifier::verify(&changed_witness).is_err(), "the witness must retain its selected error authority");
            let foreign_frame = generic.instances().find(|(_, instance)| instance.scope != source.scope).unwrap().0;
            assert_eq!(generic.scoped_operation_authority(source.instruction, Some(instance_id)).unwrap(), Some(ScopedOperationCode::ResultErr));
            assert!(generic.scoped_operation_authority(source.instruction, Some(foreign_frame)).is_err());
            assert!(generic.scoped_operation_authority(source.instruction, None).is_err());
            let foreign = prepared(FORWARDED_ERR);
            assert!(foreign.generic_evidence().unwrap().scoped_operation_source(source_id).is_err());

            let unused = prepared("pure preserved(value, other) { match Err(value) { Err(original) => original, Ok(original) => original } }\npure forwarded(value, other) { preserved(value, other) }\n");
            let _unused_symbols = unused.symbol_owner().enter();
            let evidence = unused.generic_evidence().unwrap();
            let (_, source) = evidence.scoped_operation_sources().next().unwrap();
            let scope = evidence.scope(source.scope).unwrap();
            assert_eq!(evidence.instances().count(), 0);
            assert_eq!(source.expected.arguments[0], scope.parameters[0]);
            assert!(matches!(source.expected.arguments[0], TypeRef::Rigid(_)));
            assert_ne!(scope.parameters[0], scope.parameters[1]);
            assert!(source.obligations.iter().any(|obligation| obligation.scope != source.scope));
        });
    }

    fn prepared(source: &str) -> FullProgram {
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            "scoped-ok-runtime.xsh", crate::loader::entry_source_from_text("scoped-ok-runtime.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = SourceMap::files(&sources).first().unwrap().id();
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let weak = Arc::downgrade(&checked.solved);
        let counters = checked.solved.graph.counters().clone();
        let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
        evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
        assert_eq!(checked.solved.graph.counters(), &counters);
        drop(checked); drop(parsed);
        assert!(weak.upgrade().is_none());
        evaluator.indexed_program.as_ref().unwrap().as_ref().clone()
    }

    #[test]
    fn scoped_ok_forwarding_uses_prepared_constructor_witnesses_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            for recursive in [false, true] {
                let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                    "scoped-ok-forwarding.xsh", crate::loader::entry_source_from_text("scoped-ok-forwarding.xsh", FORWARDED.to_owned()), Vec::new());
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let source_id = SourceMap::files(&sources).first().unwrap().id();
                let checked = Checker::check_arena(&parsed.arena, FORWARDED);
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                let weak = Arc::downgrade(&checked.solved);
                let counters = checked.solved.graph.counters().clone();
                let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
                let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).unwrap();
                assert_eq!(checked.solved.graph.counters(), &counters);
                drop(checked); drop(parsed);
                assert!(weak.upgrade().is_none());
                let program = evaluator.indexed_program.as_ref().unwrap();
                let generic = program.generic_evidence().unwrap();
                assert_eq!(generic.scoped_operation_sources().count(), 1);
                let (source_id, source) = generic.scoped_operation_sources().next().unwrap();
                let instances = generic.instances().filter(|(_, instance)| instance.scope == source.scope).collect::<Vec<_>>();
                assert_eq!(instances.len(), 4);
                for (_, instance) in instances {
                    let RequirementWitness::Operation(witness) = instance.requirements[source.requirement as usize] else { panic!("original constructor obligation was dropped"); };
                    let witness = generic.scoped_operation_witness(witness).unwrap();
                    assert_eq!(witness.source, source_id);
                    assert_eq!(witness.operation, ScopedOperationCode::ResultOk);
                }
                let symbols = program.symbol_owner().clone();
                let target = symbols.with_current(|| LoweredFunctionKey::Name(Name::intern("preserved")));
                let execute = || symbols.with_current(|| {
                    evaluator.try_eval_installed_compact_indexed_only_inner(plan).unwrap_or_else(|_| panic!("prepared program remains installed"))
                });
                let output = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(target, recursive, execute);
                assert_eq!(output.status, 0, "{:?}; {:?}", output.diagnostics, output.traceback);
                assert_eq!(output.stdout, b"7\nword\nnext\n11\n");
                assert!(output.stderr.is_empty()); assert!(output.diagnostics.is_empty());
            }
        });
    }

    #[test]
    fn scoped_ok_rejects_wrong_binders_coupled_receipts_and_foreign_roots() {
        crate::runtime::eval::run_eval(|| {
            let program = prepared(FORWARDED);
            let _symbols = program.symbol_owner().enter();
            let generic = program.generic_evidence().unwrap();
            let (source_id, source) = generic.scoped_operation_sources().next().unwrap();
            let scope = generic.scope(source.scope).unwrap();
            let other = program.store.function_instruction_range(scope.owner.index()).unwrap().find(|&instruction| {
                program.store.tags[instruction] == FullTag::ExprParam && program.store.payload(program.store.data[instruction].range()).unwrap() == [1]
            }).unwrap();
            let mut swapped = program.clone();
            let range = swapped.store.data[source.instruction as usize].range().bounds(swapped.store.extra.len()).unwrap();
            swapped.store.extra[range.start] = other as u32;
            assert!(FullVerifier::verify(&swapped).is_err(), "equal Generic storage cannot replace the independently quantified payload");

            let mut coupled = program.clone();
            let changed = coupled.store.generic.as_mut().unwrap().test_scoped_operation_source_mut(source_id).unwrap();
            changed.expected.arguments[0] = scope.parameters[1]; changed.arguments[0].ty = scope.parameters[1]; changed.arguments[0].instruction = other as u32;
            coupled.store.extra[range.start] = other as u32;
            assert!(FullVerifier::verify(&coupled).is_err(), "changing the public receipt and opcode cannot replace the original source contract");

            let (instance_id, instance) = generic.instances().find(|(_, instance)| instance.scope == source.scope).unwrap();
            let RequirementWitness::Operation(witness_id) = instance.requirements[source.requirement as usize] else { panic!() };
            let mut wrong = program.clone();
            wrong.store.generic.as_mut().unwrap().test_scoped_operation_witness_mut(witness_id).unwrap().arguments[0] = instance.parameter_types[1];
            assert!(FullVerifier::verify(&wrong).is_err(), "a contextual witness cannot substitute another argument's binder");
            assert_eq!(generic.scoped_operation_authority(source.instruction, Some(instance_id)).unwrap(), Some(ScopedOperationCode::ResultOk));
            assert!(generic.scoped_operation_authority(source.instruction, None).is_err());

            let foreign = prepared(FORWARDED);
            let foreign_id = foreign.generic_evidence().unwrap().scoped_operation_sources().next().unwrap().0;
            assert!(generic.scoped_operation_source(foreign_id).is_err(), "source handles belong to one prepared program");
            assert!(foreign.generic_evidence().unwrap().scoped_operation_authority(source.instruction, Some(instance_id)).is_err(), "an instance from another prepared root cannot supply a constructor proof");
        });
    }
}
