use super::*;
use super::super::super::generic::{ScopedNativeMethodRequirement, ScopedNativeMethodSource, ScopedNativeMethodObligation, ScopedNativeMethodReceiver, ScopedNativeMethodWitness, graph_ground_type};
use crate::modules::signature::MethodReceiver;
use crate::sema::registry_graph::RegistryOwner;
use crate::sema::inference::{EffectSet, SchemeId, OperationFamilyId, OperationCallId, RequirementId, ScopedRequirementRoot};
use crate::sema::check::SolvedTypes;

fn canonical_family(solved: &SolvedTypes, family: OperationFamilyId) -> Result<Option<Vec<crate::sema::registry_graph::RegistryCandidate>>, IrBuildError> {
    let graph = &solved.graph;
    let candidates = graph.family(family).map_err(|_| problem("scoped_native_method_original_family"))?;
    if candidates.len() != 2 { return Ok(None); }
    let mut metadata = Vec::new();
    for &candidate in candidates {
        let crate::sema::check::SolvedOperationAuthority::Registry(member) = solved.operation_catalog.candidate(graph, candidate).map_err(|_| problem("scoped_native_method_original_candidate"))? else { return Ok(None); };
        if !matches!((member.owner, member.operation), (RegistryOwner::Method(MethodReceiver::Str), RuntimeOp::TextStartsWith) | (RegistryOwner::Method(MethodReceiver::Bytes), RuntimeOp::BytesStartsWith)) { return Ok(None); }
        if member.entry != "starts_with" || member.binding != crate::modules::signature::ImplBinding::Native || member.semantic_rule != crate::modules::signature::SemanticRule::Standard
            || member.argument_check != crate::modules::signature::ApiArgCheck::Standard || member.lifecycle != crate::sema::registry_graph::RegistryLifecycle::None
            || member.producer_transfer != crate::sema::registry_graph::RegistryProducerTransferPlan::Empty || member.kind != crate::sema::inference::CallableKind::Pure || member.command
            || member.required_effect.is_some() || member.parameters.len() != 1 || member.parameters[0].defaulted { return Err(problem("scoped_native_method_original_registry_protocol")); }
        let template = graph.candidate(candidate).map_err(|_| problem("scoped_native_method_original_candidate"))?;
        let scheme = graph.scheme(template.scheme).map_err(|_| problem("scoped_native_method_candidate_scheme"))?;
        if !template.has_receiver || !template.actual_eligibility.is_empty() || !template.effect_roles.is_empty() || !template.output_effect_roles.is_empty() || template.failure_projection.is_some()
            || !scheme.requirements.is_empty() || !scheme.quantifiers.is_empty() || !scheme.effect_quantifiers.is_empty() { return Err(problem("scoped_native_method_candidate_not_closed")); }
        metadata.push(member);
    }
    if metadata[0].owner == metadata[1].owner { return Err(problem("scoped_native_method_duplicate_family_member")); }
    Ok(Some(metadata))
}

impl FullBuilder {
    pub(in crate::runtime::eval::indexed::full) fn prepare_scoped_native_method_requirement(&mut self, solved: &SolvedTypes, scheme: SchemeId, family: OperationFamilyId, call: OperationCallId) -> Result<Option<Requirement>, IrBuildError> {
        let Some(metadata) = canonical_family(solved, family)? else { return Ok(None); };
        let graph = &solved.graph;
        let original = graph.scheme(scheme).map_err(|_| problem("scoped_native_method_original_scheme"))?;
        let index = original.requirements.iter().position(|template| matches!(*template, RequirementTemplate::Operation { family: original_family, call: original_call } if original_family == family && original_call == call)).ok_or_else(|| problem("scoped_native_method_original_requirement"))?;
        let requirement = *original.requirement_origins.get(index).ok_or_else(|| problem("scoped_native_method_original_origin"))?;
        graph.validate_requirement_scoped(ScopedRequirementRoot { requirement, scope: Some(scheme) }).map_err(|_| problem("scoped_native_method_original_scope"))?;
        if graph.candidate_evidence(requirement).map_err(|_| problem("scoped_native_method_original_evidence"))?.is_some() { return Ok(None); }
        let call = graph.operation_call(call).map_err(|_| problem("scoped_native_method_original_call"))?;
        if call.receiver.is_none() || call.binding != OperationBinding::Slots || call.arguments.len() != 1 || call.arguments[0].is_none()
            || call.mono_authority.is_some() || call.declared_error_bound.is_some() || !call.effect_bindings.is_empty() || !call.output_effect_bindings.is_empty()
            || graph.closed_effect_summary(call.effects).map_err(|_| problem("scoped_native_method_original_effects"))? != EffectSummary::Closed(EffectSet::EMPTY) { return Err(problem("scoped_native_method_original_binding")); }
        let reference = |builder: &mut FullBuilder, ty| builder.generic.get_or_insert_with(GenericEvidenceBuilder::default).prepare_reference(graph, scheme, ty, &mut builder.store.semantic, &mut builder.semantic).map_err(|_| problem("scoped_native_method_original_type_scope"));
        let receiver = reference(self, call.receiver.unwrap())?;
        let arguments = vec![reference(self, call.arguments[0].unwrap())?].into_boxed_slice();
        let result = reference(self, call.result)?;
        let mut candidates = Vec::new();
        let mut parameter_labels = None;
        for member in metadata {
            let body = graph.scheme(member.scheme).map_err(|_| problem("scoped_native_method_candidate_scheme"))?.body;
            let TypeNode::Arrow(arrow) = graph.node(graph.resolved(body).map_err(|_| problem("scoped_native_method_candidate_signature"))?).map_err(|_| problem("scoped_native_method_candidate_signature"))? else { return Err(problem("scoped_native_method_candidate_signature")); };
            let [receiver, prefix] = arrow.params.as_slice() else { return Err(problem("scoped_native_method_candidate_signature")); };
            let labels = [receiver.label, prefix.label];
            if parameter_labels.is_some_and(|original| original != labels) { return Err(problem("scoped_native_method_candidate_labels")); }
            parameter_labels = Some(labels);
            let descriptor = self.intern_checked_callable_type(graph, body)?;
            let (kind, signature) = self.store.semantic.callable_descriptor(descriptor).map_err(|_| problem("scoped_native_method_candidate_signature"))?.ok_or_else(|| problem("scoped_native_method_candidate_signature"))?;
            let candidate = graph.family(family).map_err(|_| problem("scoped_native_method_candidate_family"))?.iter().find_map(|&candidate| graph.candidate(candidate).ok().filter(|template| template.scheme == member.scheme)).ok_or_else(|| problem("scoped_native_method_candidate_owner"))?;
            candidates.push(NativeCallableContract { authority: PreparedOperationAuthority::Registry { identity: member.identity, operation: member.operation, binding: member.binding, argument_check: member.argument_check,
                semantic_rule: member.semantic_rule, lifecycle: member.lifecycle, producer_transfer: member.producer_transfer }, registry_owner: member.owner, signature, kind,
                effects: PreparedOperationEffects { creation: EffectSet::EMPTY, inputs: Box::new([]), outputs: Box::new([]) }, input_eligibility: candidate.actual_eligibility.clone().into_boxed_slice(), argument_relations: candidate.argument_relations.clone().into_boxed_slice() });
        }
        Ok(Some(Requirement::NativeMethod(ScopedNativeMethodRequirement { receiver, arguments, result, candidates: candidates.into_boxed_slice(), parameter_labels: parameter_labels.ok_or_else(|| problem("scoped_native_method_candidate_labels"))? })))
    }

    pub(in crate::runtime::eval::indexed::full) fn prepare_scoped_native_method_sources(&mut self, solved: &SolvedTypes) -> Result<(), IrBuildError> {
        let origins = self.generic_expression_rows.iter().map(|&(instruction, origin, owner)| (instruction, (origin, owner))).collect::<FxHashMap<_, _>>();
        let mut work = 0usize;
        for (instruction, origin, owner) in self.generic_expression_rows.clone() {
            let Some(operation) = solved.operations.get(&origin) else { continue; };
            let Some(scope) = operation.caller.and_then(|caller| self.generic_declarations.get(&caller).copied()) else { continue; };
            let scheme = self.generic_schemes[&scope];
            let original_scheme = solved.graph.scheme(scheme).map_err(|_| problem("scoped_native_method_source_scheme"))?;
            let Some(index) = original_scheme.requirement_origins.iter().position(|&requirement| requirement == operation.requirement) else { continue; };
            let Some(Requirement::NativeMethod(expected)) = self.generic.as_ref().unwrap().scope(scope).map_err(|_| problem("scoped_native_method_source_scope"))?.requirements.get(index).cloned() else { continue; };
            if owner != InstructionOwner::Function(self.generic.as_ref().unwrap().scope(scope).unwrap().owner) || operation.binding.supplied_slots != [0] || !operation.binding.default_slots.is_empty()
                || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some() || !operation.argument_coercions.is_empty() || operation.actual_arguments.len() != 1 { return Err(problem("scoped_native_method_original_source_binding")); }
            let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("scoped_native_method_source_payload"))?.to_vec();
            if self.store.tags[instruction as usize] != FullTag::ExprMethod || words.len() != 4 || self.store.string(words[1]).map_err(|_| problem("scoped_native_method_source_name"))? != "starts_with" { return Err(problem("scoped_native_method_source_opcode")); }
            let block_id = IrBlockId::from_raw(words[2]).ok_or_else(|| problem("scoped_native_method_source_arguments"))?;
            let block = self.store.blocks.get(block_id.index()).ok_or_else(|| problem("scoped_native_method_source_arguments"))?;
            let argument_payload = self.store.payload(block.instructions).map_err(|_| problem("scoped_native_method_source_arguments"))?.to_vec();
            if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST || argument_payload.len() != 2 || argument_payload[0] != 1 { return Err(problem("scoped_native_method_source_argument_count")); }
            let receiver_instruction = words[0];
            let &(receiver_origin, receiver_owner) = origins.get(&receiver_instruction).ok_or_else(|| problem("scoped_native_method_receiver_origin"))?;
            let receiver = solved.expressions.get(&receiver_origin).copied().ok_or_else(|| problem("scoped_native_method_receiver_type"))?;
            if receiver_owner != owner || solved.expression_owners.get(&receiver_origin).copied() != operation.caller || operation.receiver.is_none() { return Err(problem("scoped_native_method_original_receiver")); }
            let reference = |builder: &mut FullBuilder, ty| builder.generic.get_or_insert_with(GenericEvidenceBuilder::default).prepare_reference(&solved.graph, scheme, ty, &mut builder.store.semantic, &mut builder.semantic).map_err(|_| problem("scoped_native_method_source_type_scope"));
            if reference(self, receiver)? != expected.receiver || reference(self, operation.receiver.unwrap())? != expected.receiver
                || reference(self, operation.result)? != expected.result
                || reference(self, *solved.expressions.get(&origin).ok_or_else(|| problem("scoped_native_method_source_result"))?)? != expected.result { return Err(problem("scoped_native_method_original_source_relationship")); }
            let recipes = solved.argument_sources.get(&origin).ok_or_else(|| problem("scoped_native_method_original_recipes"))?;
            let [recipe] = recipes.as_slice() else { return Err(problem("scoped_native_method_original_recipe_count")); };
            let argument_instruction = argument_payload[1];
            let argument_origin = self.original_argument_expression(argument_instruction, origin, 0, recipe, owner)?;
            let argument_type = solved.expressions.get(&argument_origin).copied().ok_or_else(|| problem("scoped_native_method_original_argument_type"))?;
            if reference(self, argument_type)? != expected.arguments[0] || reference(self, operation.actual_arguments[0])? != expected.arguments[0] { return Err(problem("scoped_native_method_original_argument_relation")); }
            let obligations = self.scoped_native_method_obligations(solved, operation.requirement, &mut work)?;
            self.generic_evidence_mut().add_scoped_native_method_source(ScopedNativeMethodSource { origin, instruction, scope, requirement: index as u32, original_requirement: operation.requirement,
                receiver: ScopedNativeMethodReceiver { origin: receiver_origin, instruction: receiver_instruction, ty: expected.receiver }, arguments: vec![PreparedInvocationArgument { original: recipe.clone(), instruction: argument_instruction, ty: expected.arguments[0] }].into_boxed_slice(),
                expected, payload: words.into_boxed_slice(), argument_block: block_id, argument_payload: argument_payload.into_boxed_slice(), method_name: Name::intern("starts_with"), obligations: obligations.into_boxed_slice() }).map_err(|_| problem("scoped_native_method_source_allocation"))?;
            self.generic_evidence_mut().add_requirement_use(super::super::super::generic::SolvedRequirementUse { instruction, scope, requirement: index as u32 });
        }
        Ok(())
    }

    fn scoped_native_method_obligations(&self, solved: &SolvedTypes, original: RequirementId, work: &mut usize) -> Result<Vec<ScopedNativeMethodObligation>, IrBuildError> {
        let mut members = self.generic_schemes.iter().map(|(&scope, &scheme)| (scope, scheme)).collect::<Vec<_>>();
        members.sort_unstable_by_key(|(scope, _)| self.generic.as_ref().unwrap().scope(*scope).map(|member| member.owner.raw()).unwrap_or(u32::MAX));
        let mut obligations = Vec::new();
        for (scope, scheme) in members {
            let member = self.generic.as_ref().unwrap().scope(scope).map_err(|_| problem("scoped_native_method_obligation_scope"))?;
            let scheme = solved.graph.scheme(scheme).map_err(|_| problem("scoped_native_method_obligation_scheme"))?;
            for (index, (&immediate, _)) in scheme.requirement_origins.iter().zip(&scheme.requirements).enumerate() {
                let Some(Requirement::NativeMethod(expected)) = member.requirements.get(index) else { continue; };
                let Some(ancestry) = ancestry(&solved.graph, immediate, original, work)? else { continue; };
                obligations.push(ScopedNativeMethodObligation { scope, requirement: index as u32, immediate_original: immediate, ancestry: ancestry.into_boxed_slice(), expected: expected.clone() });
            }
        }
        Ok(obligations)
    }

    pub(in crate::runtime::eval::indexed::full) fn prepare_scoped_native_method_witness(&mut self, scope: SchemeScopeId, index: usize, contextual: &FxHashMap<RequirementId, RequirementId>) -> Result<RequirementWitness, IrBuildError> {
        let mut found = self.generic.as_ref().unwrap().scoped_native_method_sources().filter_map(|(id, source)| source.obligations.iter().find(|obligation| obligation.scope == scope && obligation.requirement as usize == index).map(|obligation| (id, source.clone(), obligation.immediate_original)));
        let (source_id, source, immediate) = found.next().ok_or_else(|| problem("scoped_native_method_contextual_source"))?;
        if found.next().is_some() { return Err(problem("scoped_native_method_ambiguous_contextual_source")); }
        drop(found);
        let original = *contextual.get(&immediate).ok_or_else(|| problem("scoped_native_method_contextual_requirement"))?;
        let solved = self.solved.as_ref().cloned().ok_or_else(|| problem("scoped_native_method_original_graph"))?;
        let graph = &solved.graph;
        let selected = graph.candidate_evidence(original).map_err(|_| problem("scoped_native_method_contextual_evidence"))?.ok_or_else(|| problem("scoped_native_method_contextual_pending"))?;
        let crate::sema::check::SolvedOperationAuthority::Registry(metadata) = solved.operation_catalog.candidate(graph, selected.candidate).map_err(|_| problem("scoped_native_method_contextual_authority"))? else { return Err(problem("scoped_native_method_contextual_authority")); };
        let candidate = source.expected.candidates.iter().find(|candidate| matches!(candidate.authority, PreparedOperationAuthority::Registry { identity, operation, .. } if identity == metadata.identity && operation == metadata.operation) && candidate.registry_owner == metadata.owner).cloned().ok_or_else(|| problem("scoped_native_method_contextual_family_changed"))?;
        let RequirementTemplate::Operation { family, call } = graph.requirement_template(original).map_err(|_| problem("scoped_native_method_contextual_requirement"))? else { return Err(problem("scoped_native_method_contextual_requirement")); };
        if canonical_family(&solved, family)?.is_none() || !graph.family(family).map_err(|_| problem("scoped_native_method_contextual_family"))?.contains(&selected.candidate) { return Err(problem("scoped_native_method_contextual_family_changed")); }
        let call = graph.operation_call(call).map_err(|_| problem("scoped_native_method_contextual_call"))?;
        if call.binding != OperationBinding::Slots || call.receiver.is_none() || call.arguments.len() != 1 || call.arguments[0].is_none() || call.mono_authority.is_some() || call.declared_error_bound.is_some()
            || selected.actual_arguments.len() != 1 || selected.actual_arguments[0].is_none() || selected.binding.is_some() || !selected.dependencies.is_empty() || !selected.callback_invocations.is_empty()
            || !selected.effect_roots.is_empty() || !selected.effect_substitutions.is_empty() || !call.effect_bindings.is_empty() || !call.output_effect_bindings.is_empty()
            || graph.closed_effect_summary(selected.effects).map_err(|_| problem("scoped_native_method_contextual_effects"))? != EffectSummary::Closed(EffectSet::EMPTY) { return Err(problem("scoped_native_method_contextual_protocol")); }
        let TypeNode::Arrow(signature) = graph.node(graph.resolved(selected.signature).map_err(|_| problem("scoped_native_method_contextual_signature"))?).map_err(|_| problem("scoped_native_method_contextual_signature"))? else { return Err(problem("scoped_native_method_contextual_signature")); };
        if signature.kind != crate::sema::inference::CallableKind::Pure || signature.params.len() != 2 || signature.params.iter().any(|parameter| parameter.defaulted || parameter.rest)
            || graph.resolved(call.receiver.unwrap()).map_err(|_| problem("scoped_native_method_contextual_receiver"))? != graph.resolved(signature.params[0].ty).map_err(|_| problem("scoped_native_method_contextual_receiver"))?
            || graph.resolved(call.arguments[0].unwrap()).map_err(|_| problem("scoped_native_method_contextual_argument"))? != graph.resolved(selected.actual_arguments[0].unwrap()).map_err(|_| problem("scoped_native_method_contextual_argument"))?
            || graph.resolved(call.arguments[0].unwrap()).map_err(|_| problem("scoped_native_method_contextual_argument"))? != graph.resolved(signature.params[1].ty).map_err(|_| problem("scoped_native_method_contextual_argument"))?
            || graph.resolved(call.result).map_err(|_| problem("scoped_native_method_contextual_result"))? != graph.resolved(signature.result).map_err(|_| problem("scoped_native_method_contextual_result"))?
            || graph.resolved(selected.result).map_err(|_| problem("scoped_native_method_contextual_result"))? != graph.resolved(call.result).map_err(|_| problem("scoped_native_method_contextual_result"))? { return Err(problem("scoped_native_method_contextual_relationship")); }
        let mut ground = |ty| { let ty = graph_ground_type(graph, ty).map_err(|_| problem("scoped_native_method_contextual_type"))?; self.intern_generic_ground_type(&ty) };
        let receiver = ground(call.receiver.unwrap())?;
        let arguments = vec![ground(call.arguments[0].unwrap())?].into_boxed_slice();
        let result = ground(call.result)?;
        let witness = self.generic_evidence_mut().add_scoped_native_method_witness(ScopedNativeMethodWitness { source: source_id, candidate, receiver, arguments, result }).map_err(|_| problem("scoped_native_method_witness_allocation"))?;
        Ok(RequirementWitness::NativeMethod(witness))
    }
}

fn ancestry(graph: &crate::sema::inference::InferenceContext, mut current: RequirementId, original: RequirementId, work: &mut usize) -> Result<Option<Vec<RequirementId>>, IrBuildError> {
    let mut chain = Vec::new();
    for _ in 0..256 {
        *work = work.checked_add(1).ok_or_else(|| problem("scoped_native_method_ancestry_work"))?;
        if *work > 2_000_000 || chain.contains(&current) { return Err(problem("scoped_native_method_ancestry_work")); }
        chain.push(current);
        if current == original { return Ok(Some(chain)); }
        let next = graph.requirement_source(current).map_err(|_| problem("scoped_native_method_ancestry_owner"))?;
        if current == next { return Ok(None); }
        current = next;
    }
    Err(problem("scoped_native_method_ancestry_depth"))
}

impl FullVerifier {
    fn scoped_native_method_shape<'a>(store: &FullStore, generic: &'a GenericEvidenceStore, instruction: u32) -> Result<&'a ScopedNativeMethodSource, IrVerifyError> {
        let id = generic.scoped_native_method_source_at(instruction)?.ok_or_else(|| IrVerifyError::new("scoped native method source is missing"))?;
        let source = generic.scoped_native_method_source(id)?;
        if source.payload.len() != 4 || source.arguments.len() != 1 || source.expected.arguments.len() != 1
            || store.tags.get(instruction as usize) != Some(&FullTag::ExprMethod) || store.payload(store.data.get(instruction as usize).ok_or_else(|| IrVerifyError::new("scoped native method instruction is missing"))?.range())? != source.payload.as_ref()
            || store.string(source.payload[1])? != source.method_name.as_str().as_str() { return Err(IrVerifyError::new("scoped native method changes its original instruction or spelling")); }
        let block = store.blocks.get(source.argument_block.index()).ok_or_else(|| IrVerifyError::new("scoped native method argument block is missing"))?;
        if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST || store.payload(block.instructions)? != source.argument_payload.as_ref()
            || source.payload[3..].first().and_then(|&location| IrLocationId::from_raw(location)).and_then(|location| store.location_sources.get(location.index())) != Some(&source.origin.source) { return Err(IrVerifyError::new("scoped native method changes its original operand packet or source")); }
        Ok(source)
    }
    pub(in crate::runtime::eval::indexed::full) fn verify_scoped_native_method_instruction(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32) -> Result<(), IrVerifyError> {
        let source = Self::scoped_native_method_shape(store, generic, instruction)?;
        let mut active = vec![instruction];
        Self::verify_generic_symbolic_source(store, generic, source.receiver.instruction, source.scope, source.receiver.ty, &mut active)?;
        for argument in &source.arguments { Self::verify_generic_symbolic_source(store, generic, argument.instruction, source.scope, argument.ty, &mut active)?; }
        Ok(())
    }
    pub(in crate::runtime::eval::indexed::full) fn verify_scoped_native_method_symbolic_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, scope: SchemeScopeId, expected: TypeRef, active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        if generic.scoped_native_method_source_at(instruction)?.is_none() { return Ok(false); }
        let source = Self::scoped_native_method_shape(store, generic, instruction)?;
        if source.scope != scope || !generic.references_equal(&store.semantic, scope, expected, source.expected.result)? { return Err(IrVerifyError::new("scoped native method changes its original result binder or declaration")); }
        Self::verify_generic_symbolic_source(store, generic, source.receiver.instruction, scope, source.receiver.ty, active)?;
        for argument in &source.arguments { Self::verify_generic_symbolic_source(store, generic, argument.instruction, scope, argument.ty, active)?; }
        Ok(true)
    }
    pub(in crate::runtime::eval::indexed::full) fn verify_scoped_native_method_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>, _active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        let Some(id) = generic.scoped_native_method_source_at(instruction)? else { return Ok(false); };
        let source = generic.scoped_native_method_source(id)?;
        if owner != InstructionOwner::Function(generic.scope(source.scope)?.owner) { return Err(IrVerifyError::new("scoped native method belongs to another declaration")); }
        Self::verify_scoped_native_method_instruction(store, generic, instruction)?;
        if let Some(instance) = instance { generic.scoped_native_method_operation(&store.semantic, instruction, Some(instance))?; }
        if *expected != Type::Bool { return Err(IrVerifyError::new("scoped native method changes its canonical Bool result")); }
        Ok(true)
    }
}
