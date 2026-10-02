use super::*;
mod literal_comparison;
mod error_field;
mod membership;
pub(in crate::runtime::eval) use membership::PreparedMembershipLowering;
pub(in crate::runtime::eval::indexed) use error_field::error_field_receiver_type;
#[cfg(test)]
mod literal_comparison_tests;
pub(in crate::runtime::eval) use literal_comparison::{BuildLiteralComparison, PreparedLiteralComparisonSlot, PreparedComparisonLiteral};
use super::super::generic::{OperationSource, OperationSourceOrigin, PreparedFallbackLowering, PreparedOperation, PreparedOperationAuthority, PreparedOperationBinding, PreparedOperationEffects, graph_ground_type};
use crate::sema::inference::{Atom, EffectSummary, OperationBinding, RequirementTemplate, ScopedRoot, ScopedRequirementRoot, SealedOperation};

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedIntegerAddition {
    pub requirement: ScopedRequirementRoot,
    pub left: ScopedRoot,
    pub right: ScopedRoot,
    pub result: ScopedRoot,
}
use crate::sema::operation_graph::{ArithmeticDomain, PreparedLanguageOperation};

fn unprepared(construct: &'static str) -> IrBuildError { IrBuildError::format(construct, None, 0, 0) }

fn fallback_operands(store: &FullStore, instruction: u32, result: bool) -> Result<[u32; 2], IrVerifyError> {
    let words = store.payload(store.data.get(instruction as usize).ok_or_else(|| IrVerifyError::new("fallback instruction is missing"))?.range())?;
    if result {
        if store.tags.get(instruction as usize) != Some(&FullTag::ExprResultFallback) || words.len() != 2 {
            return Err(IrVerifyError::new("Result fallback changes its original lazy instruction"));
        }
        return Ok([words[0], words[1]]);
    }
    if store.tags.get(instruction as usize) != Some(&FullTag::ExprMatch) || words.len() != 3 {
        return Err(IrVerifyError::new("Optional fallback changes its original lazy selection"));
    }
    let block = IrBlockId::from_raw(words[1]).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("Optional fallback arms are missing"))?;
    let arms = store.payload(block.instructions)?;
    if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST || arms.len() != 7 || arms[0] != 2 || arms[2] != 0 || arms[5] != 0 {
        return Err(IrVerifyError::new("Optional fallback changes its lazy branch order"));
    }
    let null = arms[1] as usize;
    let present = arms[4] as usize;
    let literal = store.payload(store.pattern_data.get(null).ok_or_else(|| IrVerifyError::new("Optional fallback null pattern is missing"))?.range())?;
    let slot = store.payload(store.pattern_data.get(present).ok_or_else(|| IrVerifyError::new("Optional fallback binding pattern is missing"))?.range())?;
    if store.patterns.get(null) != Some(&FullPatternTag::Literal) || literal.len() != 1
        || store.values.get(literal[0] as usize) != Some(&FullValueTag::Null)
        || !store.payload(store.value_data.get(literal[0] as usize).ok_or_else(|| IrVerifyError::new("Optional fallback null value is missing"))?.range())?.is_empty()
        || store.patterns.get(present) != Some(&FullPatternTag::Bind) || slot.len() != 1
        || store.tags.get(arms[6] as usize) != Some(&FullTag::ExprParam)
        || store.payload(store.data.get(arms[6] as usize).ok_or_else(|| IrVerifyError::new("Optional fallback present read is missing"))?.range())? != slot {
        return Err(IrVerifyError::new("Optional fallback changes its original null or present branch"));
    }
    Ok([words[0], arms[3]])
}

// A matching bind/read pair can still overwrite another live slot. Retain the
// original physical selection independently of its operand and result types.
fn fallback_lowering(store: &FullStore, instruction: u32, result: bool) -> Result<PreparedFallbackLowering, IrVerifyError> {
    fallback_operands(store, instruction, result)?;
    let words = store.payload(store.data[instruction as usize].range())?;
    let instruction_payload = words.to_vec().into_boxed_slice();
    if result { return Ok(PreparedFallbackLowering::Result { instruction_payload }); }
    let block = &store.blocks[IrBlockId::from_raw(words[1]).ok_or_else(|| IrVerifyError::new("Optional fallback arms are missing"))?.index()];
    let arms = store.payload(block.instructions)?;
    Ok(PreparedFallbackLowering::Optional {
        instruction_payload, arms_flags: block.flags, arms_payload: arms.to_vec().into_boxed_slice(),
        null_pattern_payload: store.payload(store.pattern_data[arms[1] as usize].range())?.to_vec().into_boxed_slice(),
        present_pattern_payload: store.payload(store.pattern_data[arms[4] as usize].range())?.to_vec().into_boxed_slice(),
        present_payload: store.payload(store.data[arms[6] as usize].range())?.to_vec().into_boxed_slice(),
    })
}

type IntegerSlotKey = (bool, u32, u32);

fn integer_slot_key(owner: InstructionOwner, slot: u32) -> IntegerSlotKey {
    match owner { InstructionOwner::Function(function) => (false, function.raw(), slot), InstructionOwner::Driver(step) => (true, step, slot) }
}

#[derive(Default)]
struct IntegerSlotContract {
    declaration: Option<(u32, u32)>,
    conflicting_declaration: bool,
    untyped_write: bool,
    writes: Vec<u32>,
}

struct IntegerStorageIndex {
    owners: Vec<Option<InstructionOwner>>,
    slots: FxHashMap<IntegerSlotKey, IntegerSlotContract>,
    verified_slots: rustc_hash::FxHashSet<IntegerSlotKey>,
    verified_operands: rustc_hash::FxHashSet<u32>,
    #[cfg(test)]
    indexed_instructions: usize,
    #[cfg(test)]
    operand_visits: usize,
}

impl IntegerStorageIndex {
    fn build(store: &FullStore) -> Result<Self, IrVerifyError> {
        let owners = store.generic_instruction_owners()?;
        let mut slots: FxHashMap<IntegerSlotKey, IntegerSlotContract> = FxHashMap::default();
        for (instruction, owner) in owners.iter().copied().enumerate() {
            let Some(owner) = owner else { continue; };
            let tag = store.tags[instruction];
            if !matches!(tag, FullTag::StmtLetInt | FullTag::StmtLet | FullTag::StmtLetBool | FullTag::StmtAssignInt | FullTag::StmtAssign | FullTag::StmtAssignBool | FullTag::StmtAssignPath | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt) { continue; }
            let words = store.payload(store.data[instruction].range())?;
            let slot = *words.first().ok_or_else(|| IrVerifyError::new("integer storage statement lacks a slot"))?;
            let contract = slots.entry(integer_slot_key(owner, slot)).or_default();
            match tag {
                FullTag::StmtLetInt => {
                    let initializer = *words.get(1).ok_or_else(|| IrVerifyError::new("integer storage initializer is missing"))?;
                    if contract.declaration.replace((instruction as u32, initializer)).is_some() { contract.conflicting_declaration = true; }
                }
                FullTag::StmtLet | FullTag::StmtLetBool => contract.conflicting_declaration = true,
                FullTag::StmtAssignInt => contract.writes.push(instruction as u32),
                _ => contract.untyped_write = true,
            }
        }
        #[cfg(test)]
        let indexed_instructions = owners.len();
        Ok(Self { owners, slots, verified_slots: rustc_hash::FxHashSet::default(), verified_operands: rustc_hash::FxHashSet::default(),
            #[cfg(test)] indexed_instructions,
            #[cfg(test)] operand_visits: 0 })
    }
}

impl FullBuilder {
    pub(super) fn prepare_source_operations(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.as_ref().cloned() else { return Ok(()); };
        for (instruction, expression, owner) in self.generic_expression_rows.clone() {
            let Some(operation) = solved.operations.get(&expression) else { continue; };
            let graph = &solved.graph;
            let Some(selected) = graph.candidate_evidence(operation.requirement).map_err(|_| unprepared("operation_candidate_owner"))? else { continue; };
            let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(graph, selected.candidate).map_err(|_| unprepared("operation_candidate_authority"))? else { continue; };
            let supported = match metadata.operation {
                PreparedLanguageOperation::ErrorField { receiver, field } => field == "message"
                    && self.store.tags.get(instruction as usize) == Some(&FullTag::ExprField)
                    && selected.actual_arguments.len() == 1
                    && selected.actual_arguments[0].is_some_and(|ty| graph_ground_type(graph, ty).ok() == error_field_receiver_type(receiver))
                    && graph_ground_type(graph, selected.result).is_ok_and(|ty| ty == Type::Str),
                PreparedLanguageOperation::Index { map } => self.store.tags.get(instruction as usize) == Some(&FullTag::ExprIndex)
                    && selected.actual_arguments.len() == 2
                    && selected.actual_arguments[0].zip(selected.actual_arguments[1]).is_some_and(|(base, index)| {
                        match (graph_ground_type(graph, base), graph_ground_type(graph, index), graph_ground_type(graph, selected.result)) {
                            (Ok(base), Ok(index), Ok(result)) => GenericEvidenceStore::supports_ground_index(map, &base, &index, &result),
                            _ => false,
                        }
                    }),
                PreparedLanguageOperation::Fallback { result } => self.store.tags.get(instruction as usize) == Some(&if result { FullTag::ExprResultFallback } else { FullTag::ExprMatch })
                    && selected.actual_arguments.len() == 2
                    && selected.actual_arguments.iter().all(|argument| argument.is_some_and(|ty| graph_ground_type(graph, ty).is_ok()))
                    && graph_ground_type(graph, selected.result).is_ok(),
                PreparedLanguageOperation::Arithmetic { domain: ArithmeticDomain::Float | ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int }, .. }
                | PreparedLanguageOperation::Ordering { left: Atom::Str, right: Atom::Str, .. } => true,
                PreparedLanguageOperation::Arithmetic { op: BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem,
                    domain: ArithmeticDomain::Integer { left, right } } if matches!(left, Atom::Int | Atom::UInt) && matches!(right, Atom::Int | Atom::UInt) =>
                    selected.actual_arguments.len() == 2
                    && selected.actual_arguments.iter().zip([left, right]).all(|(argument, atom)| argument.is_some_and(|ty|
                        graph_ground_type(graph, ty).is_ok_and(|ty| ty == if atom == Atom::UInt { Type::UInt } else { Type::Int })))
                    && graph_ground_type(graph, selected.result).is_ok_and(|ty| ty == Type::Int),
                PreparedLanguageOperation::Ordering { op: BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge, left: Atom::Int, right: Atom::Int } =>
                    self.store.tags.get(instruction as usize) == Some(&FullTag::ExprBinary)
                    && selected.actual_arguments.len() == 2
                    && selected.actual_arguments.iter().all(|argument| argument.is_some_and(|ty| graph_ground_type(graph, ty).is_ok_and(|ty| ty == Type::Int)))
                    && graph_ground_type(graph, selected.result).is_ok_and(|ty| ty == Type::Bool),
                PreparedLanguageOperation::Equality { op: BinaryOp::Eq | BinaryOp::Ne } => {
                    let types = selected.actual_arguments.iter().map(|argument| argument.and_then(|ty| graph_ground_type(graph, ty).ok())).collect::<Vec<_>>();
                    (matches!(types.as_slice(), [Some(Type::Str), Some(Type::Str)]
                        | [Some(Type::Null), Some(Type::Optional(_))] | [Some(Type::Optional(_)), Some(Type::Null)])
                        || self.store.tags.get(instruction as usize) == Some(&FullTag::BoolLiteralCompareSlot)
                            && matches!(types.as_slice(), [Some(Type::Int), Some(Type::Int)] | [Some(Type::Bool), Some(Type::Bool)]))
                        && graph_ground_type(graph, selected.result).is_ok_and(|ty| ty == Type::Bool)
                },
                _ => false,
            };
            if !supported { continue; }
            let authority = PreparedOperationAuthority::Language { identity: metadata.identity, authority: metadata.authority,
                operation: metadata.operation, argument_order: metadata.argument_order, statement_result_is_unit: metadata.statement_result_is_unit };
            let scope = operation.caller.and_then(|declaration| self.generic_declarations.get(&declaration).copied());
            let source = self.generic_evidence_mut().add_operation_source(OperationSource {
                origin: OperationSourceOrigin::Expression(expression), identity: metadata.identity, expected: authority.clone(), instruction, owner, scope,
            }).map_err(|_| unprepared("operation_source_allocation"))?;
            let RequirementTemplate::Operation { call, .. } = graph.requirement_template(operation.requirement).map_err(|_| unprepared("operation_requirement"))? else { return Err(unprepared("operation_requirement_kind")); };
            let call = graph.operation_call(call).map_err(|_| unprepared("operation_call_owner"))?;
            if call.binding != OperationBinding::Slots || !selected.callback_invocations.is_empty()
                || operation.binding.dynamic.is_some() || operation.binding.rest_slot.is_some()
                || !operation.argument_coercions.is_empty() { return Err(unprepared("operation_binding_not_prepared")); }
            let ground = |builder: &mut FullBuilder, ty| {
                let ty = graph_ground_type(graph, ty).map_err(|_| unprepared("operation_ground_type"))?;
                builder.intern_generic_ground_type(&ty).map(TypeRef::Ground)
            };
            let receiver = call.receiver.map(|ty| ground(self, ty)).transpose()?;
            let arguments = selected.actual_arguments.iter().map(|ty| ty.map(|ty| ground(self, ty)).transpose()).collect::<Result<Vec<_>, _>>()?;
            let result = ground(self, selected.result)?;
            let closed = |summary| match graph.closed_effect_summary(summary).map_err(|_| unprepared("operation_effect_owner"))? {
                EffectSummary::Closed(bits) => Ok(bits), _ => Err(unprepared("operation_latent_effect_not_prepared")),
            };
            let effects = PreparedOperationEffects {
                creation: closed(selected.effects)?,
                inputs: call.effect_bindings.iter().map(|&(role, summary)| closed(summary).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(),
                outputs: call.output_effect_bindings.iter().map(|&(role, summary)| closed(summary).map(|bits| (role, bits))).collect::<Result<Vec<_>, _>>()?.into_boxed_slice(),
            };
            let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| unprepared("operation_instruction_payload"))?.to_vec();
            let tag = self.store.tags[instruction as usize];
            let literal_comparison_slot = if tag == FullTag::BoolLiteralCompareSlot { Some(self.prepare_literal_comparison_slot(instruction, expression, owner, &selected.actual_arguments)?) } else { None };
            let operands: Box<[u32]> = if literal_comparison_slot.is_some() { Box::new([]) } else if let PreparedLanguageOperation::ErrorField { field, .. } = metadata.operation {
                if tag != FullTag::ExprField || words.len() != 3 || self.store.string(words[1]).ok() != Some(field.as_str().as_str()) { return Err(unprepared("error_field_instruction_not_prepared")); }
                Box::new([words[0]])
            } else if let PreparedLanguageOperation::Fallback { result } = metadata.operation {
                Box::new(fallback_operands(&self.store, instruction, result).map_err(|_| unprepared("fallback_instruction_not_prepared"))?)
            } else if matches!(metadata.operation, PreparedLanguageOperation::Index { .. }) {
                if tag != FullTag::ExprIndex || words.len() != 3 { return Err(unprepared("index_instruction_not_prepared")); }
                Box::new([words[0], words[1]])
            } else {
                if (tag != FullTag::ExprBinary && !(tag == FullTag::IntBinary && matches!(metadata.operation, PreparedLanguageOperation::Arithmetic { domain: ArithmeticDomain::Integer { left: Atom::Int | Atom::UInt, right: Atom::Int | Atom::UInt }, .. }))) || words.len() < 3 { return Err(IrBuildError::verification("operation_instruction_not_prepared", IrVerifyError::new(format!("selected {:?} has physical {tag:?} with {} payload words", metadata.operation, words.len())))); }
                Box::new([words[1], words[2]])
            };
            let fallback_lowering = if let PreparedLanguageOperation::Fallback { result } = metadata.operation {
                Some(fallback_lowering(&self.store, instruction, result).map_err(|_| unprepared("fallback_lowering_not_prepared"))?)
            } else { None };
            let slots = |slots: &[usize]| slots.iter().map(|&slot| u32::try_from(slot).map_err(|_| unprepared("operation_binding_slot_overflow"))).collect::<Result<Vec<_>, _>>().map(Vec::into_boxed_slice);
            self.generic_evidence_mut().add_operation(PreparedOperation {
                source,
                authority,
                receiver, arguments: arguments.into_boxed_slice(), result, effects, fallback_lowering, original_integer_addition: None, range_lowering: None, literal_comparison_slot, membership_lowering: None,
                binding: PreparedOperationBinding { supplied_slots: slots(&operation.binding.supplied_slots)?,
                    default_slots: slots(&operation.binding.default_slots)?, rest_slot: None, dynamic: None, operands },
            }).map_err(|_| unprepared("operation_proof_allocation"))?;
        }
        for (instruction, expression, owner) in self.generic_expression_rows.clone() {
            let Some(&requirement) = solved.additions.get(&expression) else { continue; };
            let graph = &solved.graph;
            let Some(evidence) = graph.discharge(requirement).map_err(|_| unprepared("integer_add_original_requirement"))? else { continue; };
            if evidence.operation != SealedOperation::AddInt { continue; }
            let RequirementTemplate::Add { left, right, result } = graph.requirement_template(requirement).map_err(|_| unprepared("integer_add_original_requirement"))? else {
                return Err(unprepared("integer_add_original_requirement"));
            };
            let declaration = solved.expression_owners.get(&expression).copied();
            let lexical = declaration.and_then(|owner| solved.declarations.get(&owner).map(|declaration| declaration.scheme));
            let original_scope = solved.expression_schemes.get(&expression).copied().or_else(|| solved.expression_value_scopes.get(&expression).copied()).or(lexical);
            let original_integer_addition = PreparedIntegerAddition {
                requirement: ScopedRequirementRoot { requirement, scope: original_scope },
                left: ScopedRoot { ty: left, scope: original_scope },
                right: ScopedRoot { ty: right, scope: original_scope },
                result: ScopedRoot { ty: result, scope: original_scope },
            };
            graph.validate_requirement_scoped(original_integer_addition.requirement).map_err(|_| unprepared("integer_add_original_scope"))?;
            if solved.owner != graph.owner() || evidence.requirement != requirement
                || solved.expressions.get(&expression).is_none_or(|&source| graph.resolved(source).ok() != graph.resolved(result).ok()) {
                return Err(unprepared("integer_add_original_source"));
            }
            for (source, checked) in [(original_integer_addition.left, evidence.left), (original_integer_addition.right, evidence.right), (original_integer_addition.result, evidence.result)] {
                graph.validate_scoped(source).map_err(|_| unprepared("integer_add_original_scope"))?;
                if graph.resolved(source.ty).map_err(|_| unprepared("integer_add_original_relationship"))?
                    != graph.resolved(checked).map_err(|_| unprepared("integer_add_original_relationship"))? {
                    return Err(unprepared("integer_add_original_relationship"));
                }
            }
            let left_type = graph_ground_type(graph, left).map_err(|_| unprepared("integer_add_original_domain"))?;
            let right_type = graph_ground_type(graph, right).map_err(|_| unprepared("integer_add_original_domain"))?;
            if !matches!(left_type, Type::Int | Type::UInt) || !matches!(right_type, Type::Int | Type::UInt)
                || graph_ground_type(graph, result).map_err(|_| unprepared("integer_add_original_domain"))? != Type::Int {
                return Err(unprepared("integer_add_original_domain"));
            }
            if declaration.is_some_and(|declaration| self.declaration_functions.get(&declaration).copied().map(InstructionOwner::Function) != Some(owner))
                || (declaration.is_none() && !matches!(owner, InstructionOwner::Driver(_))) {
                return Err(unprepared("integer_add_original_owner"));
            }
            let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| unprepared("integer_add_original_payload"))?;
            if !matches!(self.store.tags[instruction as usize], FullTag::ExprBinary | FullTag::IntBinary) || words.len() < 3
                || words.first().and_then(|&index| self.store.binary_ops.get(index as usize)) != Some(&BinaryOp::Add) {
                return Err(unprepared("integer_add_original_instruction"));
            }
            let operands = Box::new([words[1], words[2]]);
            let arguments = Box::new([Some(TypeRef::Ground(self.intern_generic_ground_type(&left_type)?)), Some(TypeRef::Ground(self.intern_generic_ground_type(&right_type)?))]);
            let result = TypeRef::Ground(self.intern_generic_ground_type(&Type::Int)?);
            let scope = declaration.and_then(|declaration| self.generic_declarations.get(&declaration).copied());
            let authority = PreparedOperationAuthority::Sealed { operation: evidence.operation };
            let source = self.generic_evidence_mut().add_operation_source(OperationSource {
                origin: OperationSourceOrigin::Expression(expression), identity: authority.identity(), expected: authority.clone(), instruction, owner, scope,
            }).map_err(|_| unprepared("integer_add_source_allocation"))?;
            self.generic_evidence_mut().add_operation(PreparedOperation {
                source, authority, receiver: None, arguments, result,
                effects: PreparedOperationEffects { creation: crate::sema::inference::EffectSet::EMPTY, inputs: Box::new([]), outputs: Box::new([]) },
                binding: PreparedOperationBinding { supplied_slots: Box::new([0, 1]), default_slots: Box::new([]), rest_slot: None, dynamic: None, operands },
                fallback_lowering: None, original_integer_addition: Some(original_integer_addition), range_lowering: None, literal_comparison_slot: None, membership_lowering: None,
            }).map_err(|_| unprepared("integer_add_proof_allocation"))?;
        }
        Ok(())
    }
}

impl FullVerifier {
    pub(in crate::runtime::eval::indexed) fn is_uint_integer_arithmetic(pools: &SemanticPools, operation: &PreparedOperation) -> Result<bool, IrVerifyError> {
        if matches!(operation.authority, PreparedOperationAuthority::Sealed { operation: SealedOperation::AddInt }) { return Ok(true); }
        if !matches!(operation.authority, PreparedOperationAuthority::Language {
            operation: PreparedLanguageOperation::Arithmetic { domain: ArithmeticDomain::Integer { left: Atom::Int | Atom::UInt, right: Atom::Int | Atom::UInt }, .. }, ..
        }) { return Ok(false); }
        for reference in &operation.arguments {
            if let Some(TypeRef::Ground(ty)) = reference && pools.to_type(*ty)? == Type::UInt { return Ok(true); }
        }
        Ok(false)
    }

    // Each original operand retains its checked descriptor and unsigned bounds,
    // independently of the selected integer implementation.
    pub(in crate::runtime::eval::indexed) fn verify_prepared_integer_arithmetic_contract(pools: &SemanticPools, operation: &PreparedOperation) -> Result<(), IrVerifyError> {
        match operation.authority {
            PreparedOperationAuthority::Sealed { operation: SealedOperation::AddInt } if operation.original_integer_addition.is_some() => {},
            PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Arithmetic {
                op: BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem,
                domain: ArithmeticDomain::Integer { left: Atom::Int | Atom::UInt, right: Atom::Int | Atom::UInt } },
                argument_order: crate::sema::operation_graph::OperationArgumentOrder::SourceOrder, statement_result_is_unit: false, ..
            } if operation.original_integer_addition.is_none() => {},
            _ => return Err(IrVerifyError::new("integer arithmetic lacks its selected original authority")),
        }
        if operation.receiver.is_some() || operation.arguments.len() != 2 || operation.binding.supplied_slots.as_ref() != [0, 1]
            || !operation.binding.default_slots.is_empty() || operation.binding.operands.len() != if operation.literal_comparison_slot.is_some() { 0 } else { 2 }
            || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some() || operation.fallback_lowering.is_some()
            || operation.effects.creation != crate::sema::inference::EffectSet::EMPTY
            || !operation.effects.inputs.is_empty() || !operation.effects.outputs.is_empty() {
            return Err(IrVerifyError::new("integer arithmetic changes its original operand, binding or effect contract"));
        }
        for reference in &operation.arguments {
            let Some(TypeRef::Ground(ty)) = reference else { return Err(IrVerifyError::new("integer arithmetic operand proof is not ground")); };
            if !matches!(pools.to_type(*ty)?, Type::Int | Type::UInt) {
                return Err(IrVerifyError::new("integer arithmetic changes its original signed or unsigned operand domain"));
            }
        }
        let TypeRef::Ground(result) = operation.result else { return Err(IrVerifyError::new("integer arithmetic result proof is not ground")); };
        if pools.to_type(result)? != Type::Int { return Err(IrVerifyError::new("integer arithmetic result differs from its selected Int domain")); }
        Ok(())
    }

    pub(super) fn verify_uint_integer_arithmetic_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32,
        owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>,
    ) -> Result<bool, IrVerifyError> {
        let Some(operation) = generic.operation_at(instruction)? else { return Ok(false); };
        if !Self::is_uint_integer_arithmetic(&store.semantic, operation)? { return Ok(false); }
        Self::verify_prepared_integer_arithmetic_contract(&store.semantic, operation)?;
        let source = generic.operation_source(operation.source)?;
        let op = match operation.authority {
            PreparedOperationAuthority::Sealed { operation: SealedOperation::AddInt } => BinaryOp::Add,
            PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Arithmetic { op, .. }, .. } => op,
            _ => return Err(IrVerifyError::new("integer arithmetic has another selected authority")),
        };
        let tag = store.tags.get(instruction as usize).copied();
        if source.instruction != instruction || source.owner != owner || operation.authority != source.expected
            || generic.registered_instruction_origin(instruction, false) != Some((source.origin, owner))
            || !matches!(tag, Some(FullTag::ExprBinary | FullTag::IntBinary)) || *expected != Type::Int {
            return Err(IrVerifyError::new("unsigned integer arithmetic changes its original source, owner or result domain"));
        }
        let OperationSourceOrigin::Expression(origin) = source.origin else { return Err(IrVerifyError::new("unsigned integer arithmetic has another source identity kind")); };
        let words = store.payload(store.data[instruction as usize].range())?;
        if words.first().and_then(|&index| store.binary_ops.get(index as usize)) != Some(&op)
            || words.get(1..3) != Some(operation.binding.operands.as_ref())
            || tag == Some(FullTag::ExprBinary) && words.get(3).and_then(|&location| IrLocationId::from_raw(location)).and_then(|location| store.location_sources.get(location.index())) != Some(&origin.source) {
            return Err(IrVerifyError::new("unsigned integer arithmetic changes its original encoded operator or operands"));
        }
        for (&operand, reference) in operation.binding.operands.iter().zip(operation.arguments.iter()) {
            let Some(TypeRef::Ground(ty)) = reference else { return Err(IrVerifyError::new("unsigned integer arithmetic operand proof is not ground")); };
            Self::verify_generic_source(store, generic, operand, owner, &store.semantic.to_type(*ty)?, instance, active)?;
        }
        Ok(true)
    }

    pub(in crate::runtime::eval::indexed) fn is_null_optional_equality(pools: &SemanticPools, operation: &PreparedOperation) -> Result<bool, IrVerifyError> {
        if !matches!(operation.authority, PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Equality { op: BinaryOp::Eq | BinaryOp::Ne }, .. })
            || operation.arguments.len() != 2 { return Ok(false); }
        let mut types = Vec::with_capacity(2);
        for reference in &operation.arguments {
            let Some(TypeRef::Ground(ty)) = reference else { return Ok(false); };
            types.push(pools.to_type(*ty)?);
        }
        Ok(matches!(types.as_slice(), [Type::Null, Type::Optional(_)] | [Type::Optional(_), Type::Null]))
    }

    pub(in crate::runtime::eval::indexed) fn verify_prepared_null_optional_equality_contract(pools: &SemanticPools, operation: &PreparedOperation) -> Result<(), IrVerifyError> {
        if !Self::is_null_optional_equality(pools, operation)? || !matches!(operation.authority,
            PreparedOperationAuthority::Language { argument_order: crate::sema::operation_graph::OperationArgumentOrder::SourceOrder, statement_result_is_unit: false, .. })
            || operation.receiver.is_some() || operation.binding.supplied_slots.as_ref() != [0, 1]
            || !operation.binding.default_slots.is_empty() || operation.binding.operands.len() != 2
            || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some() || operation.fallback_lowering.is_some()
            || operation.original_integer_addition.is_some() || operation.range_lowering.is_some()
            || operation.effects.creation != crate::sema::inference::EffectSet::EMPTY
            || !operation.effects.inputs.is_empty() || !operation.effects.outputs.is_empty() {
            return Err(IrVerifyError::new("null and optional equality changes its original domains, binding or effects"));
        }
        let TypeRef::Ground(result) = operation.result else { return Err(IrVerifyError::new("null and optional equality result is not ground")); };
        if pools.to_type(result)? != Type::Bool { return Err(IrVerifyError::new("null and optional equality changes its original Bool result")); }
        Ok(())
    }

    pub(super) fn verify_null_optional_equality_operand(store: &FullStore, generic: &GenericEvidenceStore,
        instruction: u32, owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>,
    ) -> Result<bool, IrVerifyError> {
        let Some(operation) = generic.operation_at(instruction)? else { return Ok(false); };
        if !Self::is_null_optional_equality(&store.semantic, operation)? { return Ok(false); }
        Self::verify_prepared_null_optional_equality_contract(&store.semantic, operation)?;
        let source = generic.operation_source(operation.source)?;
        let PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Equality { op }, .. } = operation.authority else { unreachable!() };
        let OperationSourceOrigin::Expression(origin) = source.origin else { return Err(IrVerifyError::new("null and optional equality has another original identity kind")); };
        if operation.literal_comparison_slot.is_some() { return Self::verify_literal_comparison_slot(store, generic, operation, instruction, owner, expected); }
        let words = store.payload(store.data.get(instruction as usize).ok_or_else(|| IrVerifyError::new("null and optional equality instruction is missing"))?.range())?;
        if source.instruction != instruction || source.owner != owner || operation.authority != source.expected
            || generic.registered_instruction_origin(instruction, false) != Some((source.origin, owner))
            || store.tags.get(instruction as usize) != Some(&FullTag::ExprBinary) || *expected != Type::Bool || words.len() != 4
            || words.first().and_then(|&index| store.binary_ops.get(index as usize)) != Some(&op)
            || words.get(1..3) != Some(operation.binding.operands.as_ref())
            || IrLocationId::from_raw(words[3]).and_then(|location| store.location_sources.get(location.index())) != Some(&origin.source) {
            return Err(IrVerifyError::new("null and optional equality changes its original instruction, operands or source"));
        }
        for (&operand, reference) in operation.binding.operands.iter().zip(operation.arguments.iter()) {
            let Some(TypeRef::Ground(ty)) = reference else { return Err(IrVerifyError::new("null and optional equality operand is not ground")); };
            Self::verify_generic_source(store, generic, operand, owner, &store.semantic.to_type(*ty)?, instance, active)?;
        }
        Ok(true)
    }

    pub(in crate::runtime::eval::indexed) fn verify_prepared_fallback_contract(pools: &SemanticPools, operation: &PreparedOperation) -> Result<(), IrVerifyError> {
        let PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Fallback { result }, argument_order: crate::sema::operation_graph::OperationArgumentOrder::SourceOrder, statement_result_is_unit: false, .. } = operation.authority else {
            return Err(IrVerifyError::new("fallback lacks its selected language authority"));
        };
        if operation.receiver.is_some() || operation.arguments.len() != 2 || operation.binding.supplied_slots.as_ref() != [0, 1]
            || !operation.binding.default_slots.is_empty() || operation.binding.operands.len() != 2
            || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some()
            || operation.effects.creation != crate::sema::inference::EffectSet::EMPTY
            || !operation.effects.inputs.is_empty() || !operation.effects.outputs.is_empty() {
            return Err(IrVerifyError::new("fallback changes its original operand, default or effect contract"));
        }
        if !matches!((&operation.fallback_lowering, result), (Some(PreparedFallbackLowering::Result { .. }), true) | (Some(PreparedFallbackLowering::Optional { .. }), false)) {
            return Err(IrVerifyError::new("fallback lacks its original physical selection receipt"));
        }
        let ground = |reference| match reference { TypeRef::Ground(ty) => pools.to_type(ty), _ => Err(IrVerifyError::new("fallback lacks a ground type proof")) };
        let carrier = ground(operation.arguments[0].ok_or_else(|| IrVerifyError::new("fallback carrier proof is missing"))?)?;
        let output = ground(operation.result)?;
        let success = match (&carrier, result) { (Type::Result(success, _), true) | (Type::Optional(success), false) => success.as_ref(), _ => return Err(IrVerifyError::new("fallback changes its selected carrier kind")) };
        if success != &output { return Err(IrVerifyError::new("fallback result differs from its original carrier success type")); }
        let right = ground(operation.arguments[1].ok_or_else(|| IrVerifyError::new("fallback right operand proof is missing"))?)?;
        if !right.matches_expected(&output) || matches!((&right, &output), (Type::Int, Type::UInt)) {
            return Err(IrVerifyError::new("fallback right operand differs from its checked result domain"));
        }
        Ok(())
    }

    pub(super) fn verify_fallback_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<bool, IrVerifyError> {
        let Some(operation) = generic.operation_at(instruction)? else { return Ok(false); };
        let PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Fallback { result }, .. } = operation.authority else { return Ok(false); };
        Self::verify_prepared_fallback_contract(&store.semantic, operation)?;
        let source = generic.operation_source(operation.source)?;
        if source.instruction != instruction || source.owner != owner || operation.authority != source.expected
            || generic.registered_instruction_origin(instruction, false) != Some((source.origin, owner))
            || fallback_operands(store, instruction, result)?.as_slice() != operation.binding.operands.as_ref()
            || operation.fallback_lowering.as_ref() != Some(&fallback_lowering(store, instruction, result)?) {
            return Err(IrVerifyError::new("fallback changes its original source, owner or lazy operands"));
        }
        if !result {
            let OperationSourceOrigin::Expression(origin) = source.origin else { return Err(IrVerifyError::new("fallback source has another original identity kind")); };
            let words = store.payload(store.data[instruction as usize].range())?;
            if words.get(2).and_then(|&location| IrLocationId::from_raw(location)).and_then(|location| store.location_sources.get(location.index())) != Some(&origin.source) {
                return Err(IrVerifyError::new("Optional fallback changes its original encoded source location"));
            }
        }
        let TypeRef::Ground(output) = operation.result else { return Err(IrVerifyError::new("fallback result is not ground")); };
        if store.semantic.to_type(output)? != *expected { return Err(IrVerifyError::new("fallback operand changes its checked result type")); }
        for (&operand, reference) in operation.binding.operands.iter().zip(operation.arguments.iter()) {
            let Some(TypeRef::Ground(ty)) = reference else { return Err(IrVerifyError::new("fallback operand is not ground")); };
            Self::verify_generic_source(store, generic, operand, owner, &store.semantic.to_type(*ty)?, instance, active)?;
        }
        Ok(true)
    }

    pub(super) fn verify_source_operations(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        let mut storage = IntegerStorageIndex::build(store)?;
        for (_, operation) in generic.operations() {
            let source = generic.operation_source(operation.source)?;
            if matches!(operation.authority, PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::ErrorField { .. }, .. }) {
                if !Self::verify_error_field_operand(store, generic, source.instruction, source.owner, &Type::Str, None, &mut vec![source.instruction])? { return Err(IrVerifyError::new("error field loses its original prepared authority")); }
                continue;
            }
            if matches!(operation.authority, PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Constructor { kind: crate::sema::operation_graph::ValueConstructor::Range, .. }, .. }) {
                if !Self::verify_range_operand(store, generic, source.instruction, source.owner, &Type::Stream(Box::new(Type::Int)), None, &mut vec![source.instruction])? {
                    return Err(IrVerifyError::new("range constructor loses its original prepared authority"));
                }
                continue;
            }
            if GenericEvidenceStore::is_duration_operation(operation) {
                let TypeRef::Ground(result) = operation.result else { return Err(IrVerifyError::new("Duration result is not ground")); };
                if !Self::verify_duration_operand(store, generic, source.instruction, source.owner,
                    &store.semantic.to_type(result)?, None, &mut vec![source.instruction])? {
                    return Err(IrVerifyError::new("Duration operation is missing its original prepared proof"));
                }
                continue;
            }
            if operation.literal_comparison_slot.is_some() {
                if !Self::verify_literal_comparison_operand(store, generic, source.instruction, source.owner, &Type::Bool, None, &mut vec![source.instruction])? {
                    return Err(IrVerifyError::new("literal comparison is missing its original prepared proof"));
                }
                continue;
            }
            if Self::is_null_optional_equality(&store.semantic, operation)? {
                if !Self::verify_null_optional_equality_operand(store, generic, source.instruction, source.owner, &Type::Bool, None, &mut vec![source.instruction])? {
                    return Err(IrVerifyError::new("null and optional equality is missing its original prepared proof"));
                }
                continue;
            }
            if Self::is_uint_integer_arithmetic(&store.semantic, operation)? {
                if !Self::verify_uint_integer_arithmetic_operand(store, generic, source.instruction, source.owner,
                    &Type::Int, None, &mut vec![source.instruction])? {
                    return Err(IrVerifyError::new("unsigned integer arithmetic is missing its original prepared proof"));
                }
                continue;
            }
            let PreparedOperationAuthority::Language { operation: language_operation, .. } = &operation.authority else { return Err(IrVerifyError::new("operation authority lacks an instruction verifier")); };
            if matches!(language_operation, PreparedLanguageOperation::Index { .. }) {
                let TypeRef::Ground(ty) = operation.result else { return Err(IrVerifyError::new("index result is not ground")); };
                if !Self::verify_index_operand(store, generic, source.instruction, source.owner, &store.semantic.to_type(ty)?, &mut vec![source.instruction])? {
                    return Err(IrVerifyError::new("index is missing its original prepared proof"));
                }
                continue;
            }
            if matches!(language_operation, PreparedLanguageOperation::Fallback { .. }) {
                let TypeRef::Ground(ty) = operation.result else { return Err(IrVerifyError::new("fallback result is not ground")); };
                if !Self::verify_fallback_operand(store, generic, source.instruction, source.owner, &store.semantic.to_type(ty)?, None, &mut vec![source.instruction])? {
                    return Err(IrVerifyError::new("fallback is missing its original prepared proof"));
                }
                continue;
            }
            let op = match language_operation {
                PreparedLanguageOperation::Arithmetic { op, domain: ArithmeticDomain::Float | ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int } }
                | PreparedLanguageOperation::Ordering { op, left: Atom::Str, right: Atom::Str }
                | PreparedLanguageOperation::Ordering { op: op @ (BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge), left: Atom::Int, right: Atom::Int }
                | PreparedLanguageOperation::Equality { op } => op,
                _ => return Err(IrVerifyError::new("operation authority lacks an instruction verifier")),
            };
            let tag = store.tags.get(source.instruction as usize).copied();
            let integer = matches!(language_operation, PreparedLanguageOperation::Arithmetic { domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int }, .. });
            if tag != Some(FullTag::ExprBinary) && !(tag == Some(FullTag::IntBinary) && integer) { return Err(IrVerifyError::new("source operation proof is attached to another instruction kind")); }
            let words = store.payload(store.data[source.instruction as usize].range())?;
            let OperationSourceOrigin::Expression(expression) = source.origin else { return Err(IrVerifyError::new("source operation origin has another kind")); };
            if generic.registered_instruction_origin(source.instruction, false) != Some((source.origin, source.owner)) { return Err(IrVerifyError::new("source operation origin disagrees with its registered instruction")); }
            if tag == Some(FullTag::ExprBinary) && words.get(3).and_then(|&location| IrLocationId::from_raw(location)).and_then(|location| store.location_sources.get(location.index())) != Some(&expression.source) { return Err(IrVerifyError::new("source operation origin disagrees with encoded source location")); }
            if words.first().and_then(|&index| store.binary_ops.get(index as usize)) != Some(op) { return Err(IrVerifyError::new("source operation proof disagrees with encoded operator")); }
            if words.get(1..3) != Some(operation.binding.operands.as_ref()) { return Err(IrVerifyError::new("source operation operand origins disagree with instruction")); }
            for (&operand, expected) in operation.binding.operands.iter().zip(operation.arguments.iter()) {
                let Some(TypeRef::Ground(expected)) = expected else { return Err(IrVerifyError::new("source operation operand is not ground")); };
                if tag == Some(FullTag::IntBinary) {
                    if store.semantic.to_type(*expected)? != Type::Int { return Err(IrVerifyError::new("specialized integer operation has another operand domain")); }
                    Self::verify_specialized_integer_operand(store, generic, operand, source.owner, &mut storage, &mut Vec::new())?;
                } else { Self::verify_generic_source(store, generic, operand, source.owner, &store.semantic.to_type(*expected)?, None, &mut Vec::new())?; }
            }
        }
        Ok(())
    }

    fn verify_specialized_integer_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, storage: &mut IntegerStorageIndex, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        if storage.owners.get(instruction as usize) != Some(&Some(owner)) || active.len() >= 256 { return Err(IrVerifyError::new("specialized integer operand is foreign, cyclic, or too deep")); }
        if active.contains(&instruction) {
            let words = store.payload(store.data[instruction as usize].range())?;
            let slot = words.first().copied().ok_or_else(|| IrVerifyError::new("specialized integer operand slot is missing"))?;
            let key = integer_slot_key(owner, slot);
            // An initialized slot anchors reads from its own typed writes;
            // instruction cycles outside that established slot remain invalid.
            if store.tags[instruction as usize] != FullTag::IntSlot || !storage.verified_slots.contains(&key)
                || storage.slots.get(&key).and_then(|contract| contract.declaration).is_some_and(|(declaration, _)| declaration >= instruction) {
                return Err(IrVerifyError::new("specialized integer operand is foreign, cyclic, or too deep"));
            }
            return Ok(());
        }
        if storage.verified_operands.contains(&instruction) { return Ok(()); }
        #[cfg(test)]
        { storage.operand_visits += 1; }
        active.push(instruction);
        let words = store.payload(store.data[instruction as usize].range())?;
        match store.tags[instruction as usize] {
            FullTag::IntInt => {},
            FullTag::IntSlot => {
                if Self::verify_pattern_operand(store, generic, instruction, owner, &Type::Int)?
                    || Self::verify_value_binding_operand(store, generic, instruction, owner, &Type::Int, active)?
                    || matches!(owner, InstructionOwner::Driver(_)) && Self::verify_mutable_binding_operand(store, generic, instruction, owner, &Type::Int, active)? {
                    active.pop();
                    storage.verified_operands.insert(instruction);
                    return Ok(());
                }
                let slot = *words.first().ok_or_else(|| IrVerifyError::new("specialized integer operand slot is missing"))? as usize;
                let InstructionOwner::Function(function) = owner else { return Err(IrVerifyError::new("specialized integer slot lacks a fixed function contract")); };
                let callable = &store.functions[function.index()];
                let key = integer_slot_key(owner, slot as u32);
                if slot >= callable.params.len as usize && storage.slots.get(&key).and_then(|contract| contract.declaration).is_some_and(|(declaration, _)| declaration >= instruction) { return Err(IrVerifyError::new("specialized integer local is read before its storage declaration")); }
                if !storage.verified_slots.contains(&key) {
                    let (declaration, conflicting, untyped, writes) = storage.slots.get(&key).map(|contract| (contract.declaration, contract.conflicting_declaration, contract.untyped_write, contract.writes.clone())).unwrap_or((None, false, false, Vec::new()));
                    if conflicting || untyped { return Err(IrVerifyError::new("specialized integer storage has an untyped or conflicting write")); }
                    if slot < callable.params.len as usize {
                        if declaration.is_some() { return Err(IrVerifyError::new("specialized integer parameter has a conflicting storage declaration")); }
                        let params = callable.params.bounds(store.params.len()).ok_or_else(|| IrVerifyError::new("specialized integer parameter range is invalid"))?;
                        let ty = TypeId::from_raw(store.params[params.start + slot].type_id).ok_or_else(|| IrVerifyError::new("specialized integer parameter lacks a ground type"))?;
                        if store.semantic.to_type(ty)? != Type::Int { return Err(IrVerifyError::new("specialized integer operand disagrees with its parameter contract")); }
                    } else {
                        let (declaration, initializer) = declaration.ok_or_else(|| IrVerifyError::new("specialized integer local lacks a prepared storage contract"))?;
                        if declaration >= instruction { return Err(IrVerifyError::new("specialized integer local is read before its storage declaration")); }
                        Self::verify_specialized_integer_operand(store, generic, initializer, owner, storage, active)?;
                    }
                    // The initial fixed contract anchors self-references in
                    // subsequent typed writes; every write is still checked.
                    storage.verified_slots.insert(key);
                    for write in writes {
                        let words = store.payload(store.data[write as usize].range())?;
                        if !matches!(words.get(1).and_then(|&index| store.assign_ops.get(index as usize)), Some(AssignOp::Set | AssignOp::Add | AssignOp::Sub | AssignOp::Mul | AssignOp::Div | AssignOp::Rem)) { return Err(IrVerifyError::new("specialized integer storage has an invalid assignment operator")); }
                        let value = *words.get(2).ok_or_else(|| IrVerifyError::new("specialized integer storage write lacks its value"))?;
                        Self::verify_specialized_integer_operand(store, generic, value, owner, storage, active)?;
                    }
                }
            }
            FullTag::IntBinary => {
                if !matches!(words.first().and_then(|&index| store.binary_ops.get(index as usize)), Some(BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem)) { return Err(IrVerifyError::new("specialized integer operand has another operator")); }
                if let Some(operation) = generic.operation_at(instruction)? {
                    let TypeRef::Ground(result) = operation.result else { return Err(IrVerifyError::new("specialized integer child result is not ground")); };
                    if store.semantic.to_type(result)? != Type::Int { return Err(IrVerifyError::new("specialized integer child has another result domain")); }
                }
                for &child in words.get(1..3).ok_or_else(|| IrVerifyError::new("specialized integer operand children are missing"))? { Self::verify_specialized_integer_operand(store, generic, child, owner, storage, active)?; }
            }
            _ => return Err(IrVerifyError::new("specialized integer operand lacks a prepared type contract")),
        }
        active.pop();
        storage.verified_operands.insert(instruction);
        Ok(())
    }
}

#[cfg(test)]
#[path = "operation_prepare/fallback_tests.rs"]
mod fallback_tests;

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::sema::check::Checker;

    fn fixture() -> FullProgram {
        let source = "pure subtract(left: Float, right: Float) -> Float { left - right }\nlet difference = subtract(7.0, 2.0)\n";
        source_fixture(source, PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: ArithmeticDomain::Float })
    }

    // Native sources cannot mutate prepared handles or instruction payloads;
    // these fixtures isolate the retained executable proof boundary.
    pub(in crate::runtime::eval::indexed::full) fn source_fixture(source: &str, expected: PreparedLanguageOperation) -> FullProgram {
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            "operation-proof.xsh", crate::loader::entry_source_from_text("operation-proof.xsh", source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = SourceMap::files(&sources).first().unwrap().id();
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let sealed_integer_addition = matches!(expected, PreparedLanguageOperation::Arithmetic { op: BinaryOp::Add, domain: ArithmeticDomain::Integer { .. } })
            && bodies.solved.additions.values().any(|&requirement| bodies.solved.graph.discharge(requirement).unwrap().is_some_and(|evidence| evidence.operation == SealedOperation::AddInt));
        assert!(sealed_integer_addition || bodies.solved.operations.values().any(|operation| {
            let graph = &bodies.solved.graph;
            let Some(evidence) = graph.candidate_evidence(operation.requirement).unwrap() else { return false; };
            matches!(bodies.solved.operation_catalog.candidate(graph, evidence.candidate).unwrap(),
                crate::sema::check::SolvedOperationAuthority::Language(metadata)
                    if metadata.operation == expected)
        }), "the original source must supply the selected operation {expected:?}; selected operations: {:?}",
            bodies.solved.operations.values().map(|operation| {
                let graph = &bodies.solved.graph;
                graph.candidate_evidence(operation.requirement).unwrap().map(|evidence| (
                    bodies.solved.operation_catalog.candidate(graph, evidence.candidate).unwrap(),
                    evidence.actual_arguments.iter().map(|argument| argument.map(|ty| graph_ground_type(graph, ty))).collect::<Vec<_>>(),
                ))
            }).collect::<Vec<_>>());
        let prepared = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id);
        drop(parsed); drop(declarations); drop(bodies);
        let program = prepared.unwrap();
        program.symbol_owner().with_current(|| FullVerifier::verify(&program).unwrap());
        program
    }

    #[test]
    fn selected_integer_source_operation_requires_prepared_instruction_proof_after_frontend_drop() {
        let source = "pure subtract(left: Int, right: Int) -> Int { left - right }\nlet difference = subtract(7, 2)\n";
        let program = source_fixture(source, PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: ArithmeticDomain::Integer { left: crate::sema::inference::Atom::Int, right: crate::sema::inference::Atom::Int } });
        assert!(program.generic_evidence().is_some(), "selected integer source operation has no prepared proof store");
        assert_eq!(program.generic_evidence().unwrap().operations().count(), 1, "the original selected integer operation must survive preparation");
    }

    #[test]
    fn selected_specialized_integer_source_operation_keeps_the_original_prepared_proof() {
        let source = "pure subtract(left: Int, right: Int) -> Int { let difference = left - right; difference }\nlet difference = subtract(7, 2)\n";
        let program = source_fixture(source, PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: ArithmeticDomain::Integer { left: crate::sema::inference::Atom::Int, right: crate::sema::inference::Atom::Int } });
        assert!(program.store.tags.contains(&FullTag::IntBinary), "the fixture must exercise the specialized integer codec");
        assert!(program.generic_evidence().is_some(), "selected specialized integer operation has no prepared proof store");
        assert_eq!(program.generic_evidence().unwrap().operations().count(), 1, "integer specialization cannot drop its original source contract");
    }

    #[test]
    fn selected_specialized_integer_operations_preserve_checked_local_storage_contracts() {
        let source = "pure subtract(left: Int, right: Int) -> Int { let first = left - right; let second = first - right; second }\nlet difference = subtract(7, 2)\n";
        let program = source_fixture(source, PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int } });
        assert_eq!(program.generic_evidence().unwrap().operations().count(), 2, "each selected operation retains its original source contract through fixed local storage");
    }

    #[test]
    fn selected_specialized_integer_local_storage_rejects_untyped_conflicting_and_foreign_writes() {
        let source = "pure other(value: Int) -> Int { let result = value - 1; result }\npure subtract(left: Int, right: Int) -> Int { var first = left - right; first = 3; first = first - right; let second = first - 1; second }\nlet difference = subtract(7, 2)\n";
        let program = source_fixture(source, PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int } });
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let storage = IntegerStorageIndex::build(&program.store).unwrap();
            let (&key, contract) = storage.slots.iter().find(|(_, contract)| !contract.writes.is_empty()).unwrap();
            let (declaration, _) = contract.declaration.unwrap();
            let write = contract.writes[0];
            let owner = storage.owners[declaration as usize].unwrap();
            let local_read = program.store.tags.iter().enumerate().find_map(|(instruction, &tag)| {
                (tag == FullTag::IntSlot && storage.owners[instruction] == Some(owner)
                    && program.store.payload(program.store.data[instruction].range()).unwrap().first() == Some(&key.2)).then_some(instruction)
            }).unwrap();
            let mut untyped_declaration = program.store.clone();
            untyped_declaration.tags[declaration as usize] = FullTag::StmtLet;
            assert!(FullVerifier::verify_generic_evidence(&untyped_declaration).unwrap_err().message.contains("untyped or conflicting"));
            let mut untyped_write = program.store.clone();
            untyped_write.tags[write as usize] = FullTag::StmtAssign;
            assert!(FullVerifier::verify_generic_evidence(&untyped_write).unwrap_err().message.contains("untyped or conflicting"));
            let mut conflicting = program.store.clone();
            conflicting.tags[write as usize] = FullTag::StmtLetInt;
            assert!(FullVerifier::verify_generic_evidence(&conflicting).unwrap_err().message.contains("untyped or conflicting"));
            let mut wrong_rhs = program.store.clone();
            let words = wrong_rhs.payload(wrong_rhs.data[write as usize].range()).unwrap();
            let rhs = words[2] as usize;
            wrong_rhs.tags[rhs] = FullTag::ExprParam;
            assert!(FullVerifier::verify_generic_evidence(&wrong_rhs).unwrap_err().message.contains("prepared type contract"));
            let mut invalid_assignment = program.store.clone();
            let range = invalid_assignment.data[write as usize].range();
            invalid_assignment.extra[range.start as usize + 1] = u32::MAX;
            assert!(FullVerifier::verify_generic_evidence(&invalid_assignment).unwrap_err().message.contains("invalid assignment operator"));
            let mut foreign_rhs = program.store.clone();
            let outsider = storage.owners.iter().enumerate().find_map(|(instruction, &actual)| {
                (actual.is_some() && actual != Some(owner) && program.store.tags[instruction] == FullTag::IntSlot).then_some(instruction as u32)
            }).unwrap();
            let range = foreign_rhs.data[write as usize].range();
            foreign_rhs.extra[range.start as usize + 2] = outsider;
            assert!(FullVerifier::verify_generic_evidence(&foreign_rhs).unwrap_err().message.contains("foreign, cyclic, or too deep"));
            let mut early_read = program.store.clone();
            let declaration_slot = early_read.payload(early_read.data[declaration as usize].range()).unwrap()[0];
            let (_, first_operation) = generic.operations().find(|(_, operation)| {
                let source = generic.operation_source(operation.source).unwrap();
                source.owner == owner && source.instruction < declaration
            }).unwrap();
            let operand = first_operation.binding.operands[0] as usize;
            assert_eq!(early_read.tags[operand], FullTag::IntSlot);
            let range = early_read.data[operand].range();
            early_read.extra[range.start as usize] = declaration_slot;
            assert!(FullVerifier::verify_generic_evidence(&early_read).unwrap_err().message.contains("read before its storage declaration"));
            assert!(local_read > declaration as usize);
        });
    }

    #[test]
    fn selected_specialized_integer_storage_checks_each_reachable_operand_once() {
        let mut source = String::from("pure subtract(left: Int, right: Int) -> Int { let first = left - 1;\n");
        for index in 0..32 { source.push_str(&format!("let value{index} = first - right;\n")); }
        source.push_str("value31 }\nlet difference = subtract(7, 2)\n");
        let program = source_fixture(&source, PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int } });
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            assert_eq!(generic.operations().count(), 33);
            let mut storage = IntegerStorageIndex::build(&program.store).unwrap();
            let mut requests = 0;
            for _ in 0..8 {
                for (_, operation) in generic.operations() {
                    let owner = generic.operation_source(operation.source).unwrap().owner;
                    for &operand in operation.binding.operands.iter() {
                        FullVerifier::verify_specialized_integer_operand(&program.store, generic, operand, owner, &mut storage, &mut Vec::new()).unwrap();
                        requests += 1;
                    }
                }
            }
            assert_eq!(storage.indexed_instructions, program.store.tags.len());
            assert_eq!(storage.operand_visits, storage.verified_operands.len());
            assert!(storage.operand_visits < requests / 4, "repeated local reads must reuse their retained storage proof");
        });
    }

    #[test]
    fn selected_specialized_integer_nested_and_literal_operands_retain_typed_contracts() {
        let source = "pure subtract(left: Int, right: Int) -> Int { let result = (left - 1) - right; let second = result - 2; second }\nlet difference = subtract(7, 2)\n";
        let program = source_fixture(source, PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int } });
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let owners = program.store.generic_instruction_owners().unwrap();
            assert_eq!(generic.operations().count(), 3);
            let source = generic.operations().find_map(|(_, operation)| {
                let source = generic.operation_source(operation.source).unwrap();
                (program.store.tags[source.instruction as usize] == FullTag::IntBinary).then_some(source)
            }).unwrap();
            let literal = program.store.tags.iter().enumerate().find_map(|(instruction, &tag)| {
                (tag == FullTag::IntInt && owners[instruction] == Some(source.owner)).then_some(instruction)
            }).unwrap();
            let mut wrong_literal = program.store.clone();
            wrong_literal.tags[literal] = FullTag::ExprInt;
            assert!(FullVerifier::verify_generic_evidence(&wrong_literal).unwrap_err().message.contains("prepared type contract"));
            let mut cyclic = program.store.clone();
            let range = cyclic.data[source.instruction as usize].range();
            cyclic.extra[range.start as usize + 1] = source.instruction;
            let (id, _) = generic.operations().find(|(_, operation)| generic.operation_source(operation.source).unwrap().instruction == source.instruction).unwrap();
            cyclic.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().binding.operands[0] = source.instruction;
            assert!(FullVerifier::verify_generic_evidence(&cyclic).unwrap_err().message.contains("original receipt"));
        });
    }

    #[test]
    fn selected_specialized_integer_proof_rejects_rewritten_origin_parameter_domain_and_codec() {
        let source = "pure subtract(left: Int, right: Int, unused: Str) -> Int { let difference = left - right; difference }\nlet difference = subtract(7, 2, \"unused\")\n";
        let program = source_fixture(source, PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int } });
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (_, operation) = generic.operations().next().unwrap();
            let source = generic.operation_source(operation.source).unwrap();
            assert_eq!(program.store.tags[source.instruction as usize], FullTag::IntBinary);
            let mut wrong_origin = program.store.clone();
            let OperationSourceOrigin::Expression(mut expression) = source.origin else { unreachable!() };
            expression.expression = crate::syntax::arena::ExprId::from_index(expression.expression.index() + 1);
            wrong_origin.generic.as_deref_mut().unwrap().test_operation_source_mut(operation.source).unwrap().origin = OperationSourceOrigin::Expression(expression);
            assert!(FullVerifier::verify_generic_evidence(&wrong_origin).unwrap_err().message.contains("original receipt"));
            let mut wrong_slot = program.store.clone();
            let operand = operation.binding.operands[0] as usize;
            assert_eq!(wrong_slot.tags[operand], FullTag::IntSlot);
            let range = wrong_slot.data[operand].range();
            wrong_slot.extra[range.start as usize] = 2;
            assert!(FullVerifier::verify_generic_evidence(&wrong_slot).is_err(), "a Str parameter slot cannot replace the checked Int operand");
            let mut wrong_codec = program.store.clone();
            wrong_codec.tags[operand] = FullTag::ExprParam;
            assert!(FullVerifier::verify_generic_evidence(&wrong_codec).unwrap_err().message.contains("prepared type contract"));
            let mut wrong_operator = program.store.clone();
            let words = wrong_operator.payload(wrong_operator.data[source.instruction as usize].range()).unwrap();
            let opcode = words[0] as usize;
            wrong_operator.binary_ops[opcode] = BinaryOp::Lt;
            assert!(FullVerifier::verify_generic_evidence(&wrong_operator).is_err(), "the encoded opcode cannot replace the originally selected operation");
        });
    }

    #[test]
    fn original_null_optional_equality_preserves_domains_and_decisions_on_both_routes() {
        crate::runtime::eval::run_eval(|| {
            for (operator, opcode) in [("==", BinaryOp::Eq), ("!=", BinaryOp::Ne)] {
                for reversed in [false, true] {
                    let expression = if reversed { format!("null {operator} value") } else { format!("value {operator} null") };
                    let saved_expression = expression.replace("value", "held");
                    let source = format!("pure compare(value: Int?) -> Bool {{ {expression} }}\npure guarded(value: Int?) -> Int {{ if {expression} {{ 1 }} else {{ 2 }} }}\npure preserve(value: Int?) -> Int? {{ value }}\npure saved(value: Int?) -> Bool {{ let held: Int? = preserve(value); if {saved_expression} {{ true }} else {{ false }} }}\npure control() -> Int {{ 3 }}\n");
                    let expected = PreparedLanguageOperation::Equality { op: opcode };
                    let program = Arc::new(source_fixture(&source, expected));
                    let foreign = source_fixture(&source, expected);
                    program.symbol_owner().with_current(|| {
                        let generic = program.generic_evidence().unwrap();
                        assert!(generic.operations().any(|(_, operation)| operation.literal_comparison_slot.as_ref().is_some_and(|recipe|
                            matches!(recipe.receiver, super::super::super::generic::NativeScalarReceiver::Binding { .. }))), "saved nullable comparison retains its original immutable binding authority");
                        let (id, operation) = generic.operations().find(|(_, operation)| operation.literal_comparison_slot.is_some()
                            && FullVerifier::is_null_optional_equality(&program.store.semantic, operation).unwrap()).expect("the fused original equality retains a prepared operation");
                        let original = generic.operation_source(operation.source).unwrap();
                        assert_eq!(program.store.tags[original.instruction as usize], FullTag::BoolLiteralCompareSlot);
                        assert!(operation.binding.operands.is_empty(), "fused operands are represented by an original slot proof");
                        assert_eq!(operation.literal_comparison_slot.as_ref().unwrap().argument, if reversed { 1 } else { 0 });
                        let mut changed_slot = (*program).clone();
                        let range = changed_slot.store.data[original.instruction as usize].range();
                        changed_slot.store.extra[range.start as usize + 1] += 1;
                        assert!(FullVerifier::verify(&changed_slot).is_err());
                        let mut missing_recipe = (*program).clone();
                        missing_recipe.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().literal_comparison_slot = None;
                        assert!(FullVerifier::verify(&missing_recipe).is_err(), "a fused slot cannot use the ordinary two-operand proof");
                        let mut changed_literal = (*program).clone();
                        let payload = changed_literal.store.payload(changed_literal.store.data[original.instruction as usize].range()).unwrap();
                        let literal = payload[2] as usize;
                        changed_literal.store.values[literal] = FullValueTag::Bool;
                        assert!(FullVerifier::verify(&changed_literal).is_err(), "the fused comparison retains the original Null literal domain");
                        let mut missing = (*program).clone();
                        missing.store.generic.as_deref_mut().unwrap().test_remove_operations();
                        assert!(FullVerifier::verify(&missing).is_err());
                        let mut other = (*program).clone();
                        other.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().source = foreign.generic_evidence().unwrap().operations().next().unwrap().1.source;
                        assert!(FullVerifier::verify(&other).is_err());
                        let mut changed_domains = (*program).clone();
                        changed_domains.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().arguments.swap(0, 1);
                        assert!(FullVerifier::verify(&changed_domains).is_err(), "equal runtime outcomes cannot exchange original Null and Optional operand domains");
                        let mut changed_result = (*program).clone();
                        let integer = SemanticPoolBuilder::default().intern_type(&mut changed_result.store.semantic, &Type::Int).unwrap();
                        changed_result.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().result = TypeRef::Ground(integer);
                        assert!(FullVerifier::verify(&changed_result).is_err());
                        let mut joint_operator = (*program).clone();
                        let replacement = if opcode == BinaryOp::Eq { BinaryOp::Ne } else { BinaryOp::Eq };
                        let range = joint_operator.store.data[original.instruction as usize].range();
                        let index = joint_operator.store.extra[range.start as usize] as usize;
                        joint_operator.store.binary_ops[index] = replacement;
                        let proof = joint_operator.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap();
                        let PreparedOperationAuthority::Language { operation, .. } = &mut proof.authority else { unreachable!() };
                        *operation = PreparedLanguageOperation::Equality { op: replacement };
                        let source_id = proof.source;
                        let source = joint_operator.store.generic.as_deref_mut().unwrap().test_operation_source_mut(source_id).unwrap();
                        let PreparedOperationAuthority::Language { operation, .. } = &mut source.expected else { unreachable!() };
                        *operation = PreparedLanguageOperation::Equality { op: replacement };
                        assert!(FullVerifier::verify(&joint_operator).is_err(), "rewriting source, proof and encoded opcode cannot change the original null decision");
                        for recursive in [false, true] {
                            for value in [crate::runtime::value::Value::Null, crate::runtime::value::Value::Int(7)] {
                                let decision = (value == crate::runtime::value::Value::Null) == (opcode == BinaryOp::Eq);
                                for name in ["compare", "guarded", "saved", "control"] {
                                    let function = LoweredFunctionKey::Name(Name::intern(name));
                                    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                                    evaluator.indexed_program = Some(Arc::clone(&program));
                                    let arguments = if name == "control" { vec![] } else { vec![value.clone()] };
                                    let call = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure, &arguments, Span::new(program.store.source_id, 0, 0)).expect("null equality fixture function exists");
                                    let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, call).unwrap();
                                    let expected = match name { "compare" | "saved" => crate::runtime::value::Value::Bool(decision), "guarded" => crate::runtime::value::Value::Int(if decision { 1 } else { 2 }), _ => crate::runtime::value::Value::Int(3) };
                                    assert_eq!(result, expected);
                                }
                            }
                        }
                    });
                }
            }
        });
    }

    #[test]
    fn original_signed_addition_retains_its_sealed_discharge_on_both_routes() {
        crate::runtime::eval::run_eval(|| {
            let expected = PreparedLanguageOperation::Arithmetic { op: BinaryOp::Add, domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int } };
            let program = Arc::new(source_fixture("pure calculate(left: Int, right: Int) -> Int { left + right }\n", expected));
            program.symbol_owner().with_current(|| {
                let (_, operation) = program.generic_evidence().unwrap().operations().next().unwrap();
                assert!(matches!(operation.authority, PreparedOperationAuthority::Sealed { operation: SealedOperation::AddInt }));
                assert!(operation.original_integer_addition.is_some());
                let function = LoweredFunctionKey::Name(Name::intern("calculate"));
                for recursive in [false, true] {
                    let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let call = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure,
                        &[crate::runtime::value::Value::Int(-7), crate::runtime::value::Value::Int(2)], Span::new(program.store.source_id, 0, 0)).expect("calculate function exists");
                    assert_eq!(crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, call).unwrap(), crate::runtime::value::Value::Int(-5));
                }
            });
        });
    }

    #[test]
    fn selected_unsigned_integer_arithmetic_keeps_operand_domains_and_signed_results_on_both_routes() {
        crate::runtime::eval::run_eval(|| {
            for (left, right) in [(Atom::UInt, Atom::Int), (Atom::Int, Atom::UInt), (Atom::UInt, Atom::UInt)] {
                for (operator, opcode, answer) in [("+", BinaryOp::Add, 9), ("-", BinaryOp::Sub, 5), ("*", BinaryOp::Mul, 14), ("/", BinaryOp::Div, 3), ("%", BinaryOp::Rem, 1)] {
                    let spelling = |atom| if atom == Atom::UInt { "UInt" } else { "Int" };
                    let source = format!("pure calculate(left: {}, right: {}) -> Int {{ let value = left {operator} right; value }}\n", spelling(left), spelling(right));
                    let expected = PreparedLanguageOperation::Arithmetic { op: opcode, domain: ArithmeticDomain::Integer { left, right } };
                    let program = Arc::new(source_fixture(&source, expected));
                    let foreign = source_fixture(&source, expected);
                    program.symbol_owner().with_current(|| {
                        let generic = program.generic_evidence().unwrap();
                        let (id, operation) = generic.operations().next().unwrap();
                        let original = generic.operation_source(operation.source).unwrap();
                        assert!(FullVerifier::is_uint_integer_arithmetic(&program.store.semantic, operation).unwrap());
                        for (reference, atom) in operation.arguments.iter().zip([left, right]) {
                            let Some(TypeRef::Ground(ty)) = reference else { panic!("original arithmetic operand must be ground") };
                            assert_eq!(program.store.semantic.to_type(*ty).unwrap(), if atom == Atom::UInt { Type::UInt } else { Type::Int });
                        }
                        let mut missing = (*program).clone();
                        missing.store.generic.as_deref_mut().unwrap().test_remove_operations();
                        assert!(FullVerifier::verify(&missing).is_err());
                        let mut foreign_source = (*program).clone();
                        foreign_source.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().source = foreign.generic_evidence().unwrap().operations().next().unwrap().1.source;
                        assert!(FullVerifier::verify(&foreign_source).is_err());
                        let mut changed_domain = (*program).clone();
                        let integer = SemanticPoolBuilder::default().intern_type(&mut changed_domain.store.semantic, &Type::Int).unwrap();
                        let proof = changed_domain.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap();
                        proof.arguments = Box::new([Some(TypeRef::Ground(integer)), Some(TypeRef::Ground(integer))]);
                        if let PreparedOperationAuthority::Language { operation, .. } = &mut proof.authority {
                            *operation = PreparedLanguageOperation::Arithmetic { op: opcode, domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int } };
                        }
                        assert!(FullVerifier::verify(&changed_domain).is_err(), "rewriting both operands and authority cannot remove their original UInt constraints");
                        if opcode == BinaryOp::Add {
                            let mut missing_discharge = (*program).clone();
                            missing_discharge.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().original_integer_addition = None;
                            assert!(FullVerifier::verify(&missing_discharge).is_err(), "a sealed addition requires its genuine original discharge roots");
                            let mut rewritten_roots = (*program).clone();
                            let roots = rewritten_roots.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().original_integer_addition.as_mut().unwrap();
                            std::mem::swap(&mut roots.left, &mut roots.right);
                            if left != right {
                                assert!(FullVerifier::verify(&rewritten_roots).is_err(), "rewritten original scoped roots cannot replace the sealed discharge");
                            }
                        }
                        let mut wrong_opcode = (*program).clone();
                        let payload = wrong_opcode.store.data[original.instruction as usize].range();
                        let index = wrong_opcode.store.extra[payload.start as usize] as usize;
                        wrong_opcode.store.binary_ops[index] = BinaryOp::Eq;
                        assert!(FullVerifier::verify(&wrong_opcode).is_err());
                        let function = LoweredFunctionKey::Name(Name::intern("calculate"));
                        for recursive in [false, true] {
                            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                            evaluator.indexed_program = Some(Arc::clone(&program));
                            let call = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure,
                                &[crate::runtime::value::Value::Int(7), crate::runtime::value::Value::Int(2)], Span::new(program.store.source_id, 0, 0)).expect("calculate function exists");
                            let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, call);
                            assert_eq!(result.unwrap(), crate::runtime::value::Value::Int(answer));
                            if opcode == BinaryOp::Sub {
                                let call = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure,
                                    &[crate::runtime::value::Value::Int(2), crate::runtime::value::Value::Int(7)], Span::new(program.store.source_id, 0, 0)).unwrap();
                                let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, call);
                                assert_eq!(result.unwrap(), crate::runtime::value::Value::Int(-5), "unsigned operands retain the selected signed arithmetic result");
                            }
                        }
                    });
                }
            }
        });
    }

    #[test]
    fn selected_integer_ordering_keeps_original_authority_after_frontend_disposal_on_both_routes() {
        crate::runtime::eval::run_eval(|| {
            for (operator, opcode) in [("<", BinaryOp::Lt), ("<=", BinaryOp::Le), (">", BinaryOp::Gt), (">=", BinaryOp::Ge)] {
                let source = format!("pure compare(left: Int, right: Int) -> Bool {{ left {operator} right }}\n");
                let expected = PreparedLanguageOperation::Ordering { op: opcode, left: Atom::Int, right: Atom::Int };
                let program = Arc::new(source_fixture(&source, expected));
                let foreign = source_fixture(&source, expected);
                program.symbol_owner().with_current(|| {
                    let generic = program.generic_evidence().unwrap();
                    let (id, operation) = generic.operations().next().unwrap();
                    let original = generic.operation_source(operation.source).unwrap();
                    assert_eq!(program.store.tags[original.instruction as usize], FullTag::ExprBinary);
                    let mut missing = (*program).clone();
                    missing.store.generic.as_deref_mut().unwrap().test_remove_operations();
                    assert!(FullVerifier::verify(&missing).is_err());
                    let mut foreign_source = (*program).clone();
                    foreign_source.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().source = foreign.generic_evidence().unwrap().operations().next().unwrap().1.source;
                    assert!(FullVerifier::verify(&foreign_source).is_err());
                    let mut rewritten = (*program).clone();
                    let range = rewritten.store.data[original.instruction as usize].range();
                    let index = rewritten.store.extra[range.start as usize] as usize;
                    rewritten.store.binary_ops[index] = BinaryOp::Eq;
                    let authority = &mut rewritten.store.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().authority;
                    let PreparedOperationAuthority::Language { operation, .. } = authority else { unreachable!() };
                    *operation = PreparedLanguageOperation::Equality { op: BinaryOp::Eq };
                    assert!(FullVerifier::verify(&rewritten).is_err(), "an equality cannot replace the original integer ordering and its opcode together");
                    let function = LoweredFunctionKey::Name(Name::intern("compare"));
                    for recursive in [false, true] {
                        for (left, right) in [(2, 7), (7, 2), (2, 2)] {
                            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                            evaluator.indexed_program = Some(Arc::clone(&program));
                            let call = || evaluator.call_indexed_direct(function, LoweredFunctionKind::Pure,
                                &[crate::runtime::value::Value::Int(left), crate::runtime::value::Value::Int(right)], Span::new(program.store.source_id, 0, 0)).expect("compare function exists");
                            let result = crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, call);
                            let selected = match opcode { BinaryOp::Lt => left < right, BinaryOp::Le => left <= right, BinaryOp::Gt => left > right, BinaryOp::Ge => left >= right, _ => unreachable!() };
                            assert_eq!(result.unwrap(), crate::runtime::value::Value::Bool(selected));
                        }
                    }
                });
            }
        });
    }

    #[test]
    fn selected_string_ordering_and_equality_require_prepared_source_proofs() {
        for (operator, opcode) in [("<", BinaryOp::Lt), ("<=", BinaryOp::Le), (">", BinaryOp::Gt), (">=", BinaryOp::Ge), ("==", BinaryOp::Eq), ("!=", BinaryOp::Ne)] {
            let source = format!("pure compare(left: Str, right: Str) -> Bool {{ left {operator} right }}\nlet comparison = compare(\"first\", \"second\")\n");
            let expected = if matches!(opcode, BinaryOp::Eq | BinaryOp::Ne) { PreparedLanguageOperation::Equality { op: opcode } }
                else { PreparedLanguageOperation::Ordering { op: opcode, left: crate::sema::inference::Atom::Str, right: crate::sema::inference::Atom::Str } };
            let program = source_fixture(&source, expected);
            assert!(program.generic_evidence().is_some(), "selected string {operator} has no prepared proof store");
            assert_eq!(program.generic_evidence().unwrap().operations().count(), 1, "selected string {operator} loses its source proof");
        }
    }

    #[test]
    fn selected_primitive_operations_reject_missing_or_rewritten_operand_result_and_effect_proofs() {
        for (source, expected) in [
            ("pure subtract(left: Int, right: Int) -> Int { left - right }\nlet value = subtract(7, 2)\n", PreparedLanguageOperation::Arithmetic { op: BinaryOp::Sub, domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int } }),
            ("pure compare(left: Str, right: Str) -> Bool { left < right }\nlet value = compare(\"first\", \"second\")\n", PreparedLanguageOperation::Ordering { op: BinaryOp::Lt, left: Atom::Str, right: Atom::Str }),
            ("pure compare(left: Str, right: Str) -> Bool { left == right }\nlet value = compare(\"first\", \"second\")\n", PreparedLanguageOperation::Equality { op: BinaryOp::Eq }),
        ] {
            let program = source_fixture(source, expected);
            program.symbol_owner().with_current(|| {
                let generic = program.generic_evidence().unwrap();
                let (id, operation) = generic.operations().next().unwrap();
                let instruction = generic.operation_source(operation.source).unwrap().instruction;
                let mut missing = program.store.clone();
                missing.generic.as_deref_mut().unwrap().test_remove_operations();
                assert!(FullVerifier::verify_generic_evidence(&missing).is_err(), "removing the operation proof cannot preserve its selected source authority");
                let mut swapped = program.store.clone();
                swapped.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().binding.operands.swap(0, 1);
                assert!(FullVerifier::verify_generic_evidence(&swapped).unwrap_err().message.contains("original receipt"));
                let mut wrong_result = program.store.clone();
                let result = SemanticPoolBuilder::default().intern_type(&mut wrong_result.semantic, &Type::Str).unwrap();
                wrong_result.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().result = TypeRef::Ground(result);
                assert!(FullVerifier::verify_generic_evidence(&wrong_result).is_err(), "the originally selected result domain cannot be rewritten");
                let mut wrong_argument = program.store.clone();
                let argument = SemanticPoolBuilder::default().intern_type(&mut wrong_argument.semantic, &Type::Bool).unwrap();
                wrong_argument.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().arguments[0] = Some(TypeRef::Ground(argument));
                assert!(FullVerifier::verify_generic_evidence(&wrong_argument).is_err(), "a Bool cannot replace the original numeric or text operand domain");
                let mut wrong_effects = program.store.clone();
                wrong_effects.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().effects.creation = crate::sema::inference::EffectSet::TIME;
                assert!(FullVerifier::verify_generic_evidence(&wrong_effects).unwrap_err().message.contains("original receipt"));
                let mut rewritten = program.store.clone();
                let words = rewritten.payload(rewritten.data[instruction as usize].range()).unwrap();
                let opcode = words[0] as usize;
                rewritten.binary_ops[opcode] = BinaryOp::Add;
                let PreparedOperationAuthority::Language { operation, .. } = &mut rewritten.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().authority else { unreachable!() };
                *operation = PreparedLanguageOperation::Arithmetic { op: BinaryOp::Add, domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int } };
                assert!(FullVerifier::verify_generic_evidence(&rewritten).unwrap_err().message.contains("original receipt"));
            });
        }
    }

    #[test]
    fn selected_source_operation_requires_prepared_instruction_proof() {
        let program = fixture();
        assert!(program.generic_evidence().is_some(), "selected source operation has no prepared proof store");
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let operations = generic.operations().collect::<Vec<_>>();
            assert_eq!(operations.len(), 1);
            assert_eq!(generic.operation_sources().count(), 1);
            let (_, operation) = operations[0];
            let source = generic.operation_source(operation.source).unwrap();
            assert!(generic.operation_at(source.instruction).unwrap().is_some());
            let mut missing = program.store.clone();
            missing.generic.as_deref_mut().unwrap().test_remove_operations();
            assert!(FullVerifier::verify_generic_evidence(&missing).is_err(), "removing the operation proof cannot preserve its selected source authority");
        });
    }

    #[test]
    fn selected_source_operation_rejects_wrong_instruction_operands_binding_and_owner() {
        let program = fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, operation) = generic.operations().next().unwrap();
            let source = generic.operation_source(operation.source).unwrap();
            let mut wrong_tag = program.store.clone();
            wrong_tag.tags[source.instruction as usize] = FullTag::ExprFloat;
            assert!(FullVerifier::verify_generic_evidence(&wrong_tag).unwrap_err().message.contains("another instruction kind"));
            let mut wrong_operator = program.store.clone();
            let words = wrong_operator.payload(wrong_operator.data[source.instruction as usize].range()).unwrap();
            let opcode = words[0] as usize;
            wrong_operator.binary_ops[opcode] = BinaryOp::Add;
            assert!(FullVerifier::verify_generic_evidence(&wrong_operator).is_err(), "the encoded opcode cannot replace the originally selected operation");
            let mut wrong_operand = program.store.clone();
            let proof = wrong_operand.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap();
            proof.binding.operands[0] = proof.binding.operands[1];
            assert!(FullVerifier::verify_generic_evidence(&wrong_operand).unwrap_err().message.contains("original receipt"));
            let mut wrong_binding = program.store.clone();
            wrong_binding.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().binding.supplied_slots.swap(0, 1);
            assert!(FullVerifier::verify_generic_evidence(&wrong_binding).unwrap_err().message.contains("original receipt"));
            let mut wrong_owner = program.store.clone();
            wrong_owner.generic.as_deref_mut().unwrap().test_operation_source_mut(operation.source).unwrap().owner = InstructionOwner::Driver(0);
            assert!(FullVerifier::verify_generic_evidence(&wrong_owner).unwrap_err().message.contains("original receipt"));
            let mut outsider = GenericEvidenceBuilder::default();
            let foreign_scope = outsider.add_scope(super::super::super::generic::SchemeScope {
                owner: IrFunctionId::new(0).unwrap(), quantifiers: Box::new([]), parameters: Box::new([]),
                parameter_names: Box::new([]), parameter_flags: Box::new([]), kind: super::super::super::generic::CallableKind::Pure,
                result: operation.result, requirements: Box::new([]), return_plan: GenericReturnPlan::Value,
            }).unwrap();
            let mut wrong_scope = program.store.clone();
            wrong_scope.generic.as_deref_mut().unwrap().test_operation_source_mut(operation.source).unwrap().scope = Some(foreign_scope);
            assert!(FullVerifier::verify_generic_evidence(&wrong_scope).unwrap_err().message.contains("original receipt"));
            let mut wrong_effects = program.store.clone();
            wrong_effects.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().effects.creation = crate::sema::inference::EffectSet(1);
            assert!(FullVerifier::verify_generic_evidence(&wrong_effects).unwrap_err().message.contains("original receipt"));
            let mut misplaced = program.store.clone();
            misplaced.generic.as_deref_mut().unwrap().test_operation_source_mut(operation.source).unwrap().instruction = operation.binding.operands[0];
            assert!(FullVerifier::verify_generic_evidence(&misplaced).is_err());
            let mut wrong_origin = program.store.clone();
            let OperationSourceOrigin::Expression(mut expression) = source.origin else { unreachable!() };
            expression.source = SourceId::new(123);
            wrong_origin.generic.as_deref_mut().unwrap().test_operation_source_mut(operation.source).unwrap().origin = OperationSourceOrigin::Expression(expression);
            assert!(FullVerifier::verify_generic_evidence(&wrong_origin).unwrap_err().message.contains("original receipt"));
            let mut wrong_location = program.store.clone();
            let words = wrong_location.payload(wrong_location.data[source.instruction as usize].range()).unwrap();
            let location = IrLocationId::from_raw(words[3]).unwrap().index();
            wrong_location.location_sources[location] = SourceId::new(123);
            assert!(FullVerifier::verify_generic_evidence(&wrong_location).unwrap_err().message.contains("encoded source location"));
        });
    }

    #[test]
    fn selected_source_operation_cannot_rewrite_proof_and_instruction_together() {
        let program = fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, operation) = generic.operations().next().unwrap();
            let instruction = generic.operation_source(operation.source).unwrap().instruction;
            let mut rewritten = program.store.clone();
            let words = rewritten.payload(rewritten.data[instruction as usize].range()).unwrap();
            let opcode = words[0] as usize;
            rewritten.binary_ops[opcode] = BinaryOp::Add;
            let PreparedOperationAuthority::Language { operation, .. } = &mut rewritten.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().authority else { unreachable!() };
            *operation = PreparedLanguageOperation::Arithmetic { op: BinaryOp::Add, domain: ArithmeticDomain::Float };
            assert!(FullVerifier::verify_generic_evidence(&rewritten).is_err(), "rewritten execution cannot replace the original selected source contract");
        });
    }

    #[test]
    fn selected_source_operation_ids_cannot_cross_programs_or_rewind() {
        let program = fixture();
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (_, operation) = generic.operations().next().unwrap();
            let source = generic.operation_source(operation.source).unwrap().clone();
            let mut builder = GenericEvidenceBuilder::default();
            builder.register_instruction_origin(source.instruction, source.origin, source.owner).unwrap();
            let checkpoint = builder.checkpoint();
            let old_source = builder.add_operation_source(source.clone()).unwrap();
            let mut proof = operation.clone(); proof.source = old_source;
            let old_operation = builder.add_operation(proof.clone()).unwrap();
            let retired_checkpoint = builder.checkpoint();
            builder.rewind(checkpoint).unwrap();
            let current_source = builder.add_operation_source(source.clone()).unwrap();
            proof.source = current_source;
            let current_operation = builder.add_operation(proof.clone()).unwrap();
            assert!(builder.rewind(retired_checkpoint).is_err());
            let owners = program.store.generic_instruction_owners().unwrap();
            let store = builder.finish(&program.store.semantic, program.store.functions.len(), &owners).unwrap();
            assert!(store.operation_source(old_source).is_err());
            assert!(store.operation(old_operation).is_err());
            assert!(store.operation(current_operation).is_ok());
            assert!(store.operation_source(operation.source).is_err());
            let mut foreign = program.store.clone();
            let id = generic.operations().next().unwrap().0;
            foreign.generic.as_deref_mut().unwrap().test_operation_mut(id).unwrap().source = current_source;
            assert!(FullVerifier::verify_generic_evidence(&foreign).unwrap_err().message.contains("original receipt"));
            let mut stale_builder = GenericEvidenceBuilder::default();
            stale_builder.register_instruction_origin(source.instruction, source.origin, source.owner).unwrap();
            let checkpoint = stale_builder.checkpoint();
            let stale = stale_builder.add_operation_source(source.clone()).unwrap();
            stale_builder.rewind(checkpoint).unwrap();
            stale_builder.add_operation_source(source).unwrap();
            proof.source = stale;
            stale_builder.add_operation(proof).unwrap();
            assert!(stale_builder.finish(&program.store.semantic, program.store.functions.len(), &owners).unwrap_err().message.contains("retired by rewind"));
        });
    }
}
