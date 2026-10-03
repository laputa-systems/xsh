use super::*;
use super::super::generic::{OperationSourceOrigin, PreparedTagConstructor, ScopedTagConstructorRequirement, PreparedOperationAuthority, graph_ground_type};
use crate::runtime::eval::lower::tag_constructor::OriginalTagConstructorSource;
use crate::sema::check::{ConstructorAuthority, NominalMemberKind};
use crate::sema::inference::{ScopedRequirementRoot, ScopedRoot, SchemeId, OperationFamilyId, OperationCallId, RequirementTemplate, OperationBinding, TypeNode, EffectSummary, EffectSet};

pub(super) type StagedTagConstructor = (OriginalTagConstructorSource, u32, InstructionOwner, Box<[u32]>);
fn problem(reason: &'static str) -> IrBuildError { IrBuildError::format(reason, None, 0, 0) }

impl FullBuilder {
    pub(super) fn prepare_scoped_tag_constructor_requirement(&mut self, solved: &crate::sema::check::SolvedTypes, scheme: SchemeId, family: OperationFamilyId, call: OperationCallId) -> Result<Option<Requirement>, IrBuildError> {
        let graph = &solved.graph;
        let candidates = graph.family(family).map_err(|_| problem("scoped_tag_original_family"))?;
        let mut tag_candidate = None;
        for &candidate in candidates {
            let authority = solved.operation_catalog.candidate(graph, candidate).map_err(|_| problem("scoped_tag_original_family_authority"))?;
            if matches!(authority, crate::sema::check::SolvedOperationAuthority::Language(metadata)
                if matches!(metadata.operation, crate::sema::operation_graph::PreparedLanguageOperation::Declaration { authority: "language.constructor.tag", .. })) {
                tag_candidate = Some(candidate);
            }
        }
        let Some(tag_candidate) = tag_candidate else { return Ok(None); };
        if candidates.len() != 1 { return Err(problem("scoped_tag_original_family_not_unique")); }
        let original = graph.scheme(scheme).map_err(|_| problem("scoped_tag_original_scheme"))?;
        let index = original.requirements.iter().position(|template| matches!(*template, RequirementTemplate::Operation { family: f, call: c } if f == family && c == call)).ok_or_else(|| problem("scoped_tag_original_requirement"))?;
        let requirement = *original.requirement_origins.get(index).ok_or_else(|| problem("scoped_tag_original_origin"))?;
        graph.validate_requirement_scoped(ScopedRequirementRoot { requirement, scope: Some(scheme) }).map_err(|_| problem("scoped_tag_original_scope"))?;
        let selected = graph.candidate_evidence(requirement).map_err(|_| problem("scoped_tag_original_evidence"))?.ok_or_else(|| problem("scoped_tag_original_pending"))?;
        if selected.candidate != tag_candidate { return Err(problem("scoped_tag_original_selected_member")); }
        let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(graph, selected.candidate).map_err(|_| problem("scoped_tag_original_authority"))? else { return Err(problem("scoped_tag_original_authority")); };
        let crate::sema::operation_graph::PreparedLanguageOperation::Declaration { authority: "language.constructor.tag", identity } = metadata.operation else { return Err(problem("scoped_tag_original_authority")); };
        let nominal = solved.nominal_members.keys().find(|nominal| identity.as_str().as_str() == format!("{:?}", nominal)).copied().ok_or_else(|| problem("scoped_tag_original_member"))?;
        let member = solved.checked_nominal_member(nominal).map_err(|_| problem("scoped_tag_original_member_owner"))?;
        let candidate = graph.candidate(selected.candidate).map_err(|_| problem("scoped_tag_original_candidate"))?;
        let original_call = graph.operation_call(call).map_err(|_| problem("scoped_tag_original_call"))?;
        if member.kind != NominalMemberKind::Tag || member.scope.is_some()
            || !graph.family(family).map_err(|_| problem("scoped_tag_original_family"))?.contains(&selected.candidate)
            || candidate.has_receiver || !candidate.actual_eligibility.is_empty() || !candidate.effect_roles.is_empty() || !candidate.output_effect_roles.is_empty()
            || candidate.failure_projection.is_some() || !selected.dependencies.is_empty() || !selected.callback_invocations.is_empty() || selected.binding.is_some()
            || !selected.effect_roots.is_empty() || !selected.effect_substitutions.is_empty()
            || original_call.binding != OperationBinding::Slots || original_call.receiver.is_some() || original_call.mono_authority.is_some() || original_call.declared_error_bound.is_some()
            || !original_call.effect_bindings.is_empty() || !original_call.output_effect_bindings.is_empty()
            || original_call.arguments.len() != selected.actual_arguments.len() || member.fields.len() != selected.actual_arguments.len() {
            return Err(problem("scoped_tag_original_protocol"));
        }
        let TypeNode::Arrow(signature) = graph.node(graph.resolved(selected.signature).map_err(|_| problem("scoped_tag_signature_owner"))?).map_err(|_| problem("scoped_tag_signature_owner"))? else { return Err(problem("scoped_tag_original_signature")); };
        if signature.kind != crate::sema::inference::CallableKind::Pure || signature.params.len() != member.fields.len()
            || signature.params.iter().any(|parameter| parameter.defaulted || parameter.rest)
            || graph.closed_effect_summary(signature.effects).map_err(|_| problem("scoped_tag_signature_effects"))? != EffectSummary::Closed(EffectSet::EMPTY)
            || graph.closed_effect_summary(selected.effects).map_err(|_| problem("scoped_tag_candidate_effects"))? != EffectSummary::Closed(EffectSet::EMPTY) { return Err(problem("scoped_tag_original_signature")); }
        for ((parameter, &(label, declared)), (original, actual)) in signature.params.iter().zip(&member.fields).zip(original_call.arguments.iter().zip(&selected.actual_arguments)) {
            let resolve = |ty: Option<crate::sema::inference::TypeId>| ty.map(|ty| graph.resolved(ty).map_err(|_| problem("scoped_tag_argument_owner"))).transpose();
            if label.is_some() || graph_ground_type(graph, parameter.ty).map_err(|_| problem("scoped_tag_formal_type"))? != Type::Any
                || graph_ground_type(graph, declared).map_err(|_| problem("scoped_tag_declared_type"))? != Type::Any || resolve(*original)? != resolve(*actual)? {
                return Err(problem("scoped_tag_original_arguments"));
            }
        }
        let result_type = graph_ground_type(graph, selected.result).map_err(|_| problem("scoped_tag_result_type"))?;
        let family_nominal = match nominal { crate::sema::check::QualifiedNominalIdentity::Source { source, namespace, declaration, .. } => crate::sema::check::QualifiedNominalIdentity::Source { source, namespace, declaration, member: None }, _ => return Err(problem("scoped_tag_nominal_owner")) };
        if result_type != Type::Tag(member.family) || solved.nominals.get(&graph.resolved(selected.result).map_err(|_| problem("scoped_tag_result_owner"))?) != Some(&family_nominal)
            || graph.resolved(original_call.result).map_err(|_| problem("scoped_tag_call_result"))? != graph.resolved(selected.result).map_err(|_| problem("scoped_tag_call_result"))?
            || graph.resolved(signature.result).map_err(|_| problem("scoped_tag_signature_result"))? != graph.resolved(selected.result).map_err(|_| problem("scoped_tag_signature_result"))? { return Err(problem("scoped_tag_original_result")); }
        let reference = |builder: &mut FullBuilder, ty| builder.generic.get_or_insert_with(GenericEvidenceBuilder::default).prepare_reference(graph, scheme, ty, &mut builder.store.semantic, &mut builder.semantic).map_err(|_| problem("scoped_tag_type_scope"));
        let expected = ScopedTagConstructorRequirement {
            authority: PreparedOperationAuthority::Language { identity: metadata.identity, authority: metadata.authority, operation: metadata.operation, argument_order: metadata.argument_order, statement_result_is_unit: metadata.statement_result_is_unit },
            nominal, signature: reference(self, selected.signature)?,
            arguments: selected.actual_arguments.iter().map(|ty| reference(self, ty.ok_or_else(|| problem("scoped_tag_missing_argument"))?)).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(),
            result: self.intern_generic_ground_type(&result_type)?,
        };
        expected.verify_supported().map_err(|_| problem("scoped_tag_fixed_authority"))?;
        Ok(Some(Requirement::TagConstructor(expected)))
    }

    pub(super) fn stage_original_tag_constructor(&mut self, row: BuildExprId, instruction: u32, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(source) = scratch.tag_constructor_sources.get(&row) else { return Ok(()); };
        let raw = self.current_owner.ok_or_else(|| problem("tag_constructor_owner"))?;
        let owner = if let Some(driver) = driver_owner_index(raw) { InstructionOwner::Driver(driver as u32) }
            else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| problem("tag_constructor_owner"))?) };
        if self.active_encoded_expressions.get(&row) != Some(&instruction) || self.store.tags.get(instruction as usize) != Some(&FullTag::ExprTag) { return Err(problem("tag_constructor_original_instruction")); }
        match source.plan.origin {
            OperationSourceOrigin::Expression(expression) if self.active_expression_origins.get(&row) == Some(&expression) => {},
            OperationSourceOrigin::Statement(statement) => {
                let solved = self.solved.as_ref().ok_or_else(|| problem("tag_constructor_original_statement_graph"))?;
                let original = solved.checked_tag_tail_constructor(statement).map_err(|_| problem("tag_constructor_original_statement_missing"))?;
                if original.statement != statement || original.application.result != source.plan.checked.ty
                    || !matches!(original.application.authority, ConstructorAuthority::Nominal(authority) if authority == source.plan.authority)
                    || !source.fields.is_empty() { return Err(problem("tag_constructor_original_statement_changed")); }
                self.generic_evidence_mut().register_instruction_origin(instruction, source.plan.origin, owner).map_err(|_| problem("tag_constructor_original_statement_registration"))?;
            }
            _ => return Err(problem("tag_constructor_original_instruction")),
        }
        let fields = source.fields.iter().map(|row| self.active_encoded_expressions.get(row).copied().ok_or_else(|| problem("tag_constructor_original_field_missing"))).collect::<Result<Box<[_]>, _>>()?;
        self.tag_constructor_rows.push((source.clone(), instruction, owner, fields));
        Ok(())
    }

    pub(super) fn prepare_tag_constructors(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.clone() else { return Ok(()); };
        for (source, instruction, owner, fields) in self.tag_constructor_rows.clone() {
            let plan = &source.plan;
            let (application, scope) = match plan.origin {
                OperationSourceOrigin::Expression(expression) => {
                    let application = solved.constructor_applications.get(&expression).ok_or_else(|| problem("tag_constructor_original_application_missing"))?;
                    if solved.expressions.get(&expression) != Some(&application.result)
                        || solved.argument_sources.get(&expression).map(Vec::as_slice).unwrap_or(&[]) != plan.recipes.as_ref() { return Err(problem("tag_constructor_original_expression_changed")); }
                    (application, solved.expression_scope(expression, application.caller).map_err(|_| problem("tag_constructor_original_scope"))?)
                }
                OperationSourceOrigin::Statement(statement) => {
                    let source = solved.checked_tag_tail_constructor(statement).map_err(|_| problem("tag_constructor_original_statement_missing"))?;
                    if source.statement != statement || solved.statement_owners.get(&statement).copied() != source.application.caller { return Err(problem("tag_constructor_original_statement_changed")); }
                    (&source.application, solved.tag_tail_constructor_scope(source.application.caller).map_err(|_| problem("tag_constructor_original_scope"))?)
                }
                _ => return Err(problem("tag_constructor_original_source_kind")),
            };
            if !matches!(application.authority, ConstructorAuthority::Nominal(authority) if authority == plan.authority)
                || application.caller != plan.application.caller || application.result != plan.checked.ty
                || application.requirement != plan.application.requirement || !application.default_slots.is_empty()
                || application.parameters.len() != plan.application.parameters.len() || application.supplied.len() != plan.application.supplied.len()
                || application.parameters.iter().zip(&plan.application.parameters).any(|(left, right)| left.label != right.label || left.ty != right.ty || left.default != right.default)
                || application.supplied.iter().zip(&plan.application.supplied).any(|(left, right)| left.value != right.value || left.actual != right.actual
                    || left.slot != right.slot || left.assignability != right.assignability || left.projection != right.projection) {
                return Err(problem("tag_constructor_original_application_changed"));
            }
            if scope != plan.checked.scope { return Err(problem("tag_constructor_original_scope_changed")); }
            solved.graph.validate_scoped(plan.checked).map_err(|_| problem("tag_constructor_original_result_scope"))?;
            let requirement = application.requirement.ok_or_else(|| problem("tag_constructor_original_requirement_missing"))?;
            solved.graph.validate_requirement_scoped(ScopedRequirementRoot { requirement, scope }).map_err(|_| problem("tag_constructor_original_requirement_scope"))?;
            let selected = solved.graph.candidate_evidence(requirement).map_err(|_| problem("tag_constructor_original_requirement"))?.ok_or_else(|| problem("tag_constructor_original_requirement"))?;
            let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate).map_err(|_| problem("tag_constructor_original_candidate"))? else { return Err(problem("tag_constructor_original_candidate")); };
            if !matches!(metadata.operation, crate::sema::operation_graph::PreparedLanguageOperation::Declaration { authority: "language.constructor.tag", identity }
                if &*identity.as_str() == format!("{:?}", plan.authority)) || metadata.authority != "language.constructor.tag" { return Err(problem("tag_constructor_original_candidate_changed")); }
            let member = solved.checked_nominal_member(plan.authority).map_err(|_| problem("tag_constructor_original_nominal_missing"))?;
            if member.kind != NominalMemberKind::Tag || member.scope.is_some() || member.family != plan.family || member.member != plan.member
                || !match (&member.wire, &plan.wire) { (None, None) => true, (Some(original), Some(current)) => Arc::ptr_eq(original, current), _ => false }
                || member.fields.len() != plan.parameters.len() || fields.len() != plan.parameters.len() {
                return Err(problem("tag_constructor_original_nominal_changed"));
            }
            match (owner, application.caller) {
                (InstructionOwner::Function(owner), Some(caller)) if self.declaration_functions.get(&caller) == Some(&owner) => {},
                (InstructionOwner::Driver(_), None) => {},
                _ => return Err(problem("tag_constructor_original_caller_changed")),
            }
            let mut parameters = Vec::with_capacity(plan.parameters.len());
            for ((ty, parameter), &(label, declared)) in plan.parameters.iter().zip(&application.parameters).zip(&member.fields) {
                if label.is_some() || parameter.label.is_some() { return Err(problem("tag_constructor_original_formal_changed")); }
                solved.graph.validate_scoped(ScopedRoot { ty: parameter.ty, scope }).map_err(|_| problem("tag_constructor_original_formal_scope"))?;
                if graph_ground_type(&solved.graph, parameter.ty).map_err(|_| problem("tag_constructor_original_formal_type"))? != *ty
                    || graph_ground_type(&solved.graph, declared).map_err(|_| problem("tag_constructor_original_declared_type"))? != *ty {
                    return Err(problem("tag_constructor_original_formal_type_changed"));
                }
                parameters.push(self.intern_generic_ground_type(ty)?);
            }
            let result_type = graph_ground_type(&solved.graph, application.result).map_err(|_| problem("tag_constructor_original_result_type"))?;
            let family_authority = match plan.authority { crate::sema::check::QualifiedNominalIdentity::Source { source, namespace, declaration, .. } => crate::sema::check::QualifiedNominalIdentity::Source { source, namespace, declaration, member: None }, _ => return Err(problem("tag_constructor_original_nominal_owner")) };
            if solved.nominals.get(&solved.graph.resolved(application.result).map_err(|_| problem("tag_constructor_original_result_owner"))?) != Some(&family_authority)
                || result_type != plan.checked_type {
                return Err(problem("tag_constructor_original_result_changed"));
            }
            let result = self.intern_generic_ground_type(&result_type)?;
            let prepared_scope = application.caller.and_then(|caller| self.generic_declarations.get(&caller).copied());
            if let Some(prepared) = prepared_scope {
                if self.generic_schemes.get(&prepared).copied() != scope { return Err(problem("tag_constructor_original_prepared_scope")); }
            }
            let mut actuals = Vec::with_capacity(application.supplied.len());
            let mut checked_fields = Vec::new();
            let mut argument_wrappers = Vec::new();
            for (ordinal, (argument, recipe)) in application.supplied.iter().zip(plan.recipes.iter()).enumerate() {
                let mut field = *fields.get(argument.slot).ok_or_else(|| problem("tag_constructor_original_slot"))?;
                while self.store.tags.get(field as usize) == Some(&FullTag::ExprCheckedValue) {
                    let payload = self.store.payload(self.store.data[field as usize].range()).map_err(|_| problem("tag_constructor_original_checked_payload"))?.to_vec().into_boxed_slice();
                    let child = *payload.first().ok_or_else(|| problem("tag_constructor_original_checked_payload"))?;
                    checked_fields.push((field, payload)); field = child;
                }
                let saved = self.prepared_saved_argument_bindings.get(&field).ok_or_else(|| problem("tag_constructor_original_saved_field_missing"))?.clone();
                if !matches!(plan.origin, OperationSourceOrigin::Expression(expression) if saved.call == expression) || saved.ordinal as usize != ordinal || saved.recipe != *recipe || saved.owner != owner {
                    return Err(problem("tag_constructor_original_saved_field_changed"));
                }
                argument_wrappers.push(saved.wrapper);
                let actual_root = ScopedRoot { ty: argument.actual, scope };
                solved.graph.validate_scoped(actual_root).map_err(|_| problem("tag_constructor_original_actual_scope"))?;
                let actual = if let Some(prepared) = prepared_scope {
                    self.generic.get_or_insert_with(GenericEvidenceBuilder::default).prepare_reference(&solved.graph, self.generic_schemes[&prepared], argument.actual, &mut self.store.semantic, &mut self.semantic)
                        .map_err(|_| problem("tag_constructor_original_actual_reference"))?
                } else {
                    let ty = graph_ground_type(&solved.graph, argument.actual).map_err(|_| problem("tag_constructor_original_actual_type"))?;
                    TypeRef::Ground(self.intern_generic_ground_type(&ty)?)
                };
                if saved.ty != actual || saved.scope != prepared_scope { return Err(problem("tag_constructor_original_saved_actual_changed")); }
                actuals.push(actual);
            }
            let requirement_index = if let Some(prepared) = prepared_scope {
                let original = solved.graph.scheme(self.generic_schemes[&prepared]).map_err(|_| problem("tag_constructor_original_scheme"))?;
                if let Some(index) = original.requirement_origins.iter().position(|&origin| origin == requirement) {
                    let Some(Requirement::TagConstructor(expected)) = self.generic.as_ref().unwrap().scope(prepared).map_err(|_| problem("tag_constructor_prepared_scope"))?.requirements.get(index) else { return Err(problem("tag_constructor_scoped_requirement_kind")); };
                    if expected.nominal != plan.authority || expected.arguments.as_ref() != actuals.as_slice() || expected.result != result { return Err(problem("tag_constructor_scoped_requirement_changed")); }
                    Some(index as u32)
                } else if actuals.iter().all(|actual| matches!(actual, TypeRef::Ground(_))) { None }
                else { return Err(problem("tag_constructor_original_requirement_slot")); }
            } else { None };
            let scoped_requirement = if let (Some(prepared), Some(index)) = (prepared_scope, requirement_index) {
                let Requirement::TagConstructor(expected) = &self.generic.as_ref().unwrap().scope(prepared).map_err(|_| problem("tag_constructor_prepared_scope"))?.requirements[index as usize] else { return Err(problem("tag_constructor_scoped_requirement_kind")); };
                Some(expected.clone())
            } else { None };
            let (payload, field_block) = tag_allocation(&self.store, instruction).map_err(|_| problem("tag_constructor_original_allocation"))?;
            self.generic_evidence_mut().add_tag_constructor(PreparedTagConstructor { original: plan.clone(), instruction, owner, result,
                parameters: parameters.into_boxed_slice(), fields, actuals: actuals.into_boxed_slice(), scope: prepared_scope, requirement: requirement_index, scoped_requirement, payload, field_block, argument_wrappers: argument_wrappers.into_boxed_slice(), checked_fields: checked_fields.into_boxed_slice() }).map_err(|_| problem("tag_constructor_receipt_allocation"))?;
        }
        Ok(())
    }
}

type TagAllocation = (Box<[u32]>, (u32, Box<[u32]>));
fn tag_allocation(store: &FullStore, instruction: u32) -> Result<TagAllocation, IrVerifyError> {
    if store.tags.get(instruction as usize) != Some(&FullTag::ExprTag) { return Err(IrVerifyError::new("tag constructor has another opcode")); }
    let payload = store.payload(store.data.get(instruction as usize).ok_or_else(|| IrVerifyError::new("tag constructor instruction is missing"))?.range())?;
    if payload.len() < 4 { return Err(IrVerifyError::new("tag constructor payload is incomplete")); }
    let raw = payload[2];
    let block = IrBlockId::from_raw(raw).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("tag constructor fields block is invalid"))?;
    if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST { return Err(IrVerifyError::new("tag constructor fields block has another kind")); }
    Ok((payload.to_vec().into_boxed_slice(), (raw, store.payload(block.instructions)?.to_vec().into_boxed_slice())))
}

impl FullVerifier {
    pub(super) fn verify_tag_constructors(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        for source in generic.tag_constructors() {
            Self::verify_tag_constructor_operand(store, generic, source.instruction, source.owner, &store.semantic.to_type(source.result)?, &mut Vec::new())?;
        }
        for (instruction, tag) in store.tags.iter().enumerate() {
            if *tag == FullTag::ExprTag
                && generic.registered_instruction_origin(instruction as u32, false).is_some() && generic.tag_constructor_at(instruction as u32)?.is_none() {
                return Err(IrVerifyError::new("structured tag constructor lacks its original declaration receipt"));
            }
        }
        Ok(())
    }

    pub(super) fn verify_tag_constructor_header(store: &FullStore, source: &PreparedTagConstructor, owner: InstructionOwner) -> Result<(), IrVerifyError> {
        if source.owner != owner { return Err(IrVerifyError::new("tag constructor belongs to another lexical owner")); }
        let original = &source.original;
        let instruction = source.instruction;
        let (payload, fields) = tag_allocation(store, instruction)?;
        if payload != source.payload || fields != source.field_block || payload[0] != original.family.symbol().raw()
            || store.string(payload[1])? != original.member.as_str().as_str()
            || fields.1.first().copied() != Some(original.parameters.len() as u32) || fields.1.len() != 1 + original.parameters.len()
            || fields.1[1..] != *source.fields {
            return Err(IrVerifyError::new("tag constructor changes its original canonical family, member or payload"));
        }
        let wire = if payload[3] == 0 { None } else {
            if payload.len() != 5 || payload[3] != 1 { return Err(IrVerifyError::new("tag constructor wire allocation is invalid")); }
            Some(store.wire_enums.get(payload[4] as usize).ok_or_else(|| IrVerifyError::new("tag constructor wire mapping is missing"))?)
        };
        if !match (&original.wire, wire) { (None, None) => true, (Some(original), Some(actual)) => Arc::ptr_eq(original, actual), _ => false } {
            return Err(IrVerifyError::new("tag constructor changes its original wire mapping owner"));
        }
        for (field, payload) in source.checked_fields.iter() {
            if store.tags.get(*field as usize) != Some(&FullTag::ExprCheckedValue) || store.payload(store.data[*field as usize].range())? != payload.as_ref() {
                return Err(IrVerifyError::new("tag constructor changes its original checked payload boundary"));
            }
        }
        Ok(())
    }

    pub(super) fn verify_tag_constructor_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        let Some(source) = generic.tag_constructor_at(instruction)? else { return Ok(false); };
        let original = &source.original;
        let Type::Tag(family) = original.checked_type else { return Err(IrVerifyError::new("tag constructor loses its original checked nominal type")); };
        let mut expected = expected;
        while let Type::Optional(item) = expected { expected = item; }
        if source.owner != owner || expected != &Type::Tag(family) { return Err(IrVerifyError::new("tag constructor changes its original nominal carrier or lexical owner")); }
        Self::verify_tag_constructor_header(store, source, owner)?;
        if active.len() >= 256 || active.contains(&instruction) { return Err(IrVerifyError::new("tag constructor payload is cyclic or too deep")); }
        active.push(instruction);
        for (argument, &actual) in original.application.supplied.iter().zip(source.actuals.iter()) {
            let mut field = source.fields[argument.slot];
            while store.tags.get(field as usize) == Some(&FullTag::ExprCheckedValue) { field = store.payload(store.data[field as usize].range())?[0]; }
            let saved = generic.original_argument_binding(field).ok_or_else(|| IrVerifyError::new("tag constructor loses its original payload argument"))?;
            if saved.ty != actual || saved.scope != source.scope { return Err(IrVerifyError::new("tag constructor changes its original actual payload type or lexical scope")); }
            if let Some(scope) = source.scope {
                Self::verify_generic_symbolic_source(store, generic, field, scope, actual, active)?;
            } else {
                let TypeRef::Ground(actual) = actual else { return Err(IrVerifyError::new("tag constructor symbolic payload lacks its original quantified owner")); };
                Self::verify_generic_source(store, generic, field, owner, &store.semantic.to_type(actual)?, None, active)?;
            }
        }
        active.pop();
        Ok(true)
    }
}

impl FullExecution<'_> {
    pub(in crate::runtime::eval) fn tag_constructor_argument_wrapper(&self, instruction: u32) -> Result<(), IrVerifyError> {
        self.decoder.store.verify_generic_owner()?;
        let Some(generic) = self.generic_evidence() else { return Ok(()); };
        let Some(target) = generic.tag_constructor_wrapper_target(instruction) else { return Ok(()); };
        self.tag_constructor(target)?;
        let owner = if let Some(driver) = driver_owner_index(self.decoder.owner) { InstructionOwner::Driver(driver as u32) }
            else { InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("tag constructor wrapper owner is invalid"))?) };
        FullVerifier::original_argument_wrapper_body(self.decoder.store, generic, instruction, owner)?.ok_or_else(|| IrVerifyError::new("tag constructor loses its original argument wrapper"))?;
        Ok(())
    }

    pub(in crate::runtime::eval) fn tag_constructor(&self, instruction: u32) -> Result<(), IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("tag constructor belongs to another body")); }
        self.decoder.store.verify_generic_owner()?;
        let Some(generic) = self.generic_evidence() else { return Ok(()); };
        let Some(source) = generic.tag_constructor_at(instruction)? else {
            if generic.tag_constructor_originally_prepared(instruction) { return Err(IrVerifyError::new("tag constructor lacks its original prepared authority")); }
            return Ok(());
        };
        let owner = if let Some(driver) = driver_owner_index(self.decoder.owner) { InstructionOwner::Driver(driver as u32) }
            else { InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("tag constructor owner is invalid"))?) };
        generic.verify_tag_constructor_frame(instruction, self.instantiation)?;
        FullVerifier::verify_tag_constructor_header(self.decoder.store, source, owner)
    }
}

#[cfg(test)]
#[path = "tag_constructor_prepare/tests.rs"]
mod tests;
