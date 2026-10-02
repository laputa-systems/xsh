use super::*;
use super::super::generic::{CallableKind, PreparedInvocationArgument, PreparedOperationBinding, ScopedInvocationSource, ScopedInvocationWitness, ScopedInvocationObligation, TemplateInvocationArgument};
use crate::sema::check::SolvedTypes;
use crate::sema::inference::{CallableDomain, InvocationArgumentKind, InvocationCallId, InvocationDefaultTiming, RequirementId, RequirementTemplate, SchemeId, EffectSet, EffectSummary};

fn problem(message: &'static str) -> IrBuildError { IrBuildError::format(message, None, 0, 0) }

impl FullBuilder {
    pub(super) fn prepare_scoped_invocation_requirement(&mut self, graph: &crate::sema::inference::InferenceContext, scheme: SchemeId, call: InvocationCallId) -> Result<Requirement, IrBuildError> {
        let call = graph.invocation_call(call).map_err(|_| problem("scoped_invocation_original_owner"))?;
        if !matches!(call.domain, CallableDomain::Pure | CallableDomain::Exact(crate::sema::inference::CallableKind::Pure)) {
            return Err(problem("scoped_invocation_effect_protocol_not_prepared"));
        }
        let reference = |builder: &mut FullBuilder, ty| builder.generic.get_or_insert_with(GenericEvidenceBuilder::default).prepare_reference(graph, scheme, ty, &mut builder.store.semantic, &mut builder.semantic).map_err(|_| problem("scoped_invocation_type_scope"));
        let callable = reference(self, call.callable)?;
        let result = reference(self, call.result)?;
        let arguments = call.arguments.iter().map(|argument| Ok(TemplateInvocationArgument { kind: argument.kind, ty: reference(self, argument.ty)? })).collect::<Result<Vec<_>, IrBuildError>>()?;
        Ok(Requirement::Invocation { callable, arguments: arguments.into_boxed_slice(), result, domain: call.domain })
    }

    pub(super) fn prepare_scoped_invocation_sources(&mut self, solved: &SolvedTypes) -> Result<(), IrBuildError> {
        let origins = self.generic_expression_rows.iter().map(|&(instruction, expression, owner)| (instruction, (expression, owner))).collect::<FxHashMap<_, _>>();
        let mut obligation_work = 0usize;
        for (instruction, origin, owner) in self.generic_expression_rows.clone() {
            let Some(invocation) = solved.invocations.get(&origin) else { continue; };
            let Some(scope) = invocation.caller.and_then(|caller| self.generic_declarations.get(&caller).copied()) else { continue; };
            let scheme = self.generic_schemes[&scope];
            let original_scheme = solved.graph.scheme(scheme).map_err(|_| problem("scoped_invocation_original_scheme"))?;
            let Some(index) = original_scheme.requirement_origins.iter().position(|&requirement| requirement == invocation.requirement) else { continue; };
            let RequirementTemplate::CallableInvocation { call } = original_scheme.requirements[index] else { return Err(problem("scoped_invocation_requirement_kind")); };
            let expected = self.prepare_scoped_invocation_requirement(&solved.graph, scheme, call)?;
            if self.generic.as_ref().unwrap().scope(scope).map_err(|_| problem("scoped_invocation_scope"))?.requirements.get(index) != Some(&expected)
                || self.store.tags.get(instruction as usize) != Some(&FullTag::ExprDynamicCall) {
                return Err(problem("scoped_invocation_original_instruction"));
            }
            let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("scoped_invocation_payload"))?;
            let callee_instruction = *words.first().ok_or_else(|| problem("scoped_invocation_callee"))?;
            let &(callee_origin, callee_owner) = origins.get(&callee_instruction).ok_or_else(|| problem("scoped_invocation_callee_origin"))?;
            if owner != callee_owner || self.store.tags.get(callee_instruction as usize) != Some(&FullTag::ExprParam) { return Err(problem("scoped_invocation_callee_requires_parameter")); }
            let callable_parameter = *self.store.payload(self.store.data[callee_instruction as usize].range()).map_err(|_| problem("scoped_invocation_callee"))?.first().ok_or_else(|| problem("scoped_invocation_callee"))?;
            let block = words.get(1).copied().and_then(IrBlockId::from_raw).and_then(|id| self.store.blocks.get(id.index())).ok_or_else(|| problem("scoped_invocation_arguments"))?;
            let encoded = self.store.payload(block.instructions).map_err(|_| problem("scoped_invocation_arguments"))?;
            let count = *encoded.first().ok_or_else(|| problem("scoped_invocation_arguments"))? as usize;
            let recipes = solved.argument_sources.get(&origin).ok_or_else(|| problem("scoped_invocation_argument_sources"))?;
            let Requirement::Invocation { arguments: ref types, .. } = expected else { unreachable!() };
            if count > 65536 || count != recipes.len() || count != types.len() || encoded.len() != 1 + count * 2 { return Err(problem("scoped_invocation_argument_order")); }
            let arguments = recipes.iter().zip(types).zip(encoded[1..].chunks_exact(2)).map(|((recipe, ty), encoded)| {
                if encoded[0] != 0 || ty.kind == InvocationArgumentKind::PositionalSplice { return Err(problem("scoped_invocation_splice_not_prepared")); }
                Ok(PreparedInvocationArgument { original: recipe.clone(), instruction: encoded[1], ty: ty.ty })
            }).collect::<Result<Vec<_>, IrBuildError>>()?;
            let obligations = self.scoped_invocation_obligations(solved, invocation.requirement, &mut obligation_work)?;
            self.generic_evidence_mut().add_scoped_invocation_source(ScopedInvocationSource {
                origin, instruction, scope, requirement: index as u32, original_requirement: invocation.requirement,
                callee_instruction, callee_origin, callable_parameter, expected, arguments: arguments.into_boxed_slice(), obligations: obligations.into_boxed_slice(),
            }).map_err(|_| problem("scoped_invocation_source_allocation"))?;
            self.generic_evidence_mut().add_requirement_use(super::super::generic::SolvedRequirementUse { instruction, scope, requirement: index as u32 });
        }
        Ok(())
    }

    pub(super) fn prepare_scoped_invocation_witness(&mut self, scope: SchemeScopeId, index: usize, contextual: &FxHashMap<RequirementId, RequirementId>) -> Result<RequirementWitness, IrBuildError> {
        let (source_id, source, original) = self.generic.as_ref().unwrap().scoped_invocation_sources().find_map(|(id, source)| {
            source.obligations.iter().find(|obligation| obligation.scope == scope && obligation.requirement as usize == index).map(|obligation| (id, source.clone(), obligation.immediate_original))
        }).ok_or_else(|| problem("scoped_invocation_original_source_missing"))?;
        let actual = *contextual.get(&original).ok_or_else(|| problem("scoped_invocation_contextual_origin_missing"))?;
        let solved = self.solved.clone().ok_or_else(|| problem("scoped_invocation_solved_owner"))?;
        if invocation_ancestry(&solved.graph, actual, original)?.is_none() { return Err(problem("scoped_invocation_contextual_origin")); }
        let evidence = solved.graph.invocation_evidence(actual).map_err(|_| problem("scoped_invocation_original_evidence"))?.ok_or_else(|| problem("scoped_invocation_contextual_proof_pending"))?;
        let (signature, binding, timing) = evidence.unique_plan().ok_or_else(|| problem("scoped_invocation_all_not_prepared"))?;
        let EffectSummary::Closed(effects) = solved.graph.closed_effect_summary(evidence.effects).map_err(|_| problem("scoped_invocation_effect_owner"))? else { return Err(problem("scoped_invocation_effect_scope_not_prepared")); };
        if effects != EffectSet::EMPTY || !evidence.native_alternatives.is_empty() || timing != InvocationDefaultTiming::AtCall || binding.rest_slot.is_some() || binding.dynamic.is_some() { return Err(problem("scoped_invocation_protocol_not_prepared")); }
        let reference = self.generic.get_or_insert_with(GenericEvidenceBuilder::default).prepare_closed_reference(&solved.graph, signature, &mut self.store.semantic, &mut self.semantic).map_err(|_| problem("scoped_invocation_signature_not_closed"))?;
        let descriptor = self.materialize_scoped_reference(reference)?;
        let (kind, signature) = self.store.semantic.callable_descriptor(descriptor).map_err(|_| problem("scoped_invocation_descriptor"))?.ok_or_else(|| problem("scoped_invocation_descriptor"))?;
        if kind != CallableKind::Pure { return Err(problem("scoped_invocation_kind_not_prepared")); }
        let witness = self.generic_evidence_mut().add_scoped_invocation_witness(ScopedInvocationWitness {
            source: source_id, signature, descriptor, kind, timing, effects,
            binding: PreparedOperationBinding { supplied_slots: binding.supplied_slots.iter().map(|&slot| slot as u32).collect(), default_slots: binding.default_slots.iter().map(|&slot| slot as u32).collect(), rest_slot: None, dynamic: None, operands: source.arguments.iter().map(|argument| argument.instruction).collect() },
        }).map_err(|_| problem("scoped_invocation_witness_allocation"))?;
        Ok(RequirementWitness::Invocation(witness))
    }
    fn scoped_invocation_obligations(&self, solved: &SolvedTypes, original: RequirementId, work: &mut usize) -> Result<Vec<ScopedInvocationObligation>, IrBuildError> {
        let mut members = self.generic_schemes.iter().map(|(&scope, &scheme)| (scope, scheme)).collect::<Vec<_>>();
        members.sort_unstable_by_key(|(scope, _)| self.generic.as_ref().unwrap().scope(*scope).map(|scope| scope.owner.raw()).unwrap_or(u32::MAX));
        let mut obligations = Vec::new();
        for (scope, scheme) in members {
            let member = self.generic.as_ref().unwrap().scope(scope).map_err(|_| problem("scoped_invocation_member_scope"))?;
            let scheme = solved.graph.scheme(scheme).map_err(|_| problem("scoped_invocation_member_scheme"))?;
            for (index, (&immediate, template)) in scheme.requirement_origins.iter().zip(&scheme.requirements).enumerate() {
                *work = work.checked_add(1).ok_or_else(|| problem("scoped_invocation_origin_work"))?;
                if *work > 2_000_000 { return Err(problem("scoped_invocation_origin_work")); }
                if !matches!(template, RequirementTemplate::CallableInvocation { .. }) { continue; }
                let Some(ancestry) = invocation_ancestry_counted(&solved.graph, immediate, original, work)? else { continue; };
                obligations.push(ScopedInvocationObligation { scope, requirement: index as u32, immediate_original: immediate, ancestry: ancestry.into_boxed_slice(), expected: member.requirements[index].clone() });
                if obligations.len() > 2_000_000 { return Err(problem("scoped_invocation_member_limit")); }
            }
        }
        Ok(obligations)
    }
    fn materialize_scoped_reference(&mut self, reference: TypeRef) -> Result<TypeId, IrBuildError> {
        self.generic.as_ref().unwrap().materialize_reference(reference, &[], &mut self.store.semantic, &mut self.semantic).map_err(|_| problem("scoped_invocation_signature_materialization"))
    }
}

fn invocation_ancestry(graph: &crate::sema::inference::InferenceContext, current: RequirementId, original: RequirementId) -> Result<Option<Vec<RequirementId>>, IrBuildError> {
    invocation_ancestry_counted(graph, current, original, &mut 0)
}
fn invocation_ancestry_counted(graph: &crate::sema::inference::InferenceContext, mut current: RequirementId, original: RequirementId, work: &mut usize) -> Result<Option<Vec<RequirementId>>, IrBuildError> {
    let mut ancestry = Vec::new();
    for _ in 0..256 {
        *work = work.checked_add(1).ok_or_else(|| problem("scoped_invocation_origin_work"))?;
        if *work > 2_000_000 { return Err(problem("scoped_invocation_origin_work")); }
        if ancestry.contains(&current) { return Err(problem("scoped_invocation_origin_cycle")); }
        ancestry.push(current);
        if current == original { return Ok(Some(ancestry)); }
        let next = graph.requirement_source(current).map_err(|_| problem("scoped_invocation_origin_owner"))?;
        if next == current { return Ok(None); }
        current = next;
    }
    Err(problem("scoped_invocation_origin_depth"))
}

impl FullVerifier {
    pub(super) fn verify_scoped_callable_source(store: &FullStore, generic: &GenericEvidenceStore, source: u32, owner: InstructionOwner, expected: TypeId, instance: Option<InstantiationId>, depth: usize) -> Result<(), IrVerifyError> {
        if depth >= 256 || store.generic_instruction_owners()?.get(source as usize) != Some(&Some(owner)) { return Err(IrVerifyError::new("scoped callable operand is foreign or exceeds depth limit")); }
        let words = store.payload(store.data[source as usize].range())?;
        if let Some(saved) = generic.original_argument_binding(source) {
            return Self::verify_scoped_callable_source(store, generic, saved.initializer, owner, expected, instance, depth + 1);
        }
        match store.tags[source as usize] {
            FullTag::ExprCheckedValue => Self::verify_scoped_callable_source(store, generic, *words.first().ok_or_else(|| IrVerifyError::new("checked callable operand is missing"))?, owner, expected, instance, depth + 1),
            FullTag::ExprFunctionRef => {
                let id = generic.callable_value_at(source)?.ok_or_else(|| IrVerifyError::new("callable argument lacks its original creation proof"))?;
                let contract = generic.callable_value(id)?.contract;
                if store.semantic.callable_descriptor(expected)? != Some((contract.kind, contract.signature)) { return Err(IrVerifyError::new("callable argument changes its materialized descriptor")); }
                Ok(())
            },
            FullTag::ExprParam => {
                let slot = *words.first().ok_or_else(|| IrVerifyError::new("callable parameter slot is missing"))? as usize;
                if let Some(use_) = generic.original_callable_use(source) {
                    let contract = generic.original_callable_binding(use_.binding).ok_or_else(|| IrVerifyError::new("callable argument original binding is missing"))?.contract;
                    if store.semantic.callable_descriptor(expected)? != Some((contract.kind, contract.signature)) { return Err(IrVerifyError::new("callable argument changes its original local descriptor")); }
                    return Ok(());
                }
                let instance = generic.instance(instance.ok_or_else(|| IrVerifyError::new("callable argument requires an active scoped instance"))?)?;
                if instance.parameter_types.get(slot) != Some(&expected) || owner != InstructionOwner::Function(generic.scope(instance.scope)?.owner) { return Err(IrVerifyError::new("callable argument uses another materialized parameter")); }
                Ok(())
            },
            _ => Err(IrVerifyError::new("callable argument source protocol is not prepared")),
        }
    }
    pub(super) fn verify_scoped_invocation_instruction(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32) -> Result<(), IrVerifyError> {
        let (id, source) = generic.scoped_invocation_sources().find(|(_, source)| source.instruction == instruction).ok_or_else(|| IrVerifyError::new("scoped invocation original instruction is missing"))?;
        generic.scoped_invocation_source(id)?;
        if store.tags.get(instruction as usize) != Some(&FullTag::ExprDynamicCall) { return Err(IrVerifyError::new("scoped invocation changes its original opcode")); }
        let words = store.payload(store.data[instruction as usize].range())?;
        if words.first() != Some(&source.callee_instruction) || store.tags.get(source.callee_instruction as usize) != Some(&FullTag::ExprParam)
            || store.payload(store.data[source.callee_instruction as usize].range())?.first() != Some(&source.callable_parameter) { return Err(IrVerifyError::new("scoped invocation changes its original callee parameter")); }
        let block = words.get(1).copied().and_then(IrBlockId::from_raw).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("scoped invocation argument block is missing"))?;
        let encoded = store.payload(block.instructions)?;
        if encoded.first().copied() != Some(source.arguments.len() as u32) || encoded.len() != 1 + source.arguments.len() * 2
            || source.arguments.iter().zip(encoded[1..].chunks_exact(2)).any(|(argument, words)| words != [0, argument.instruction]) { return Err(IrVerifyError::new("scoped invocation original argument order changed")); }
        for argument in &source.arguments { Self::verify_generic_symbolic_source(store, generic, argument.instruction, source.scope, argument.ty, &mut Vec::new())?; }
        Ok(())
    }
}
