use super::*;
use super::super::super::generic::NativeScalarReceiver;
use crate::sema::check::ExpressionIdentity;
use crate::sema::inference::{TypeId, TypeNode};

// A fused comparison retains the original source expressions and its typed
// slot authority because its operand instructions are absent from the program.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildLiteralComparison {
    pub receiver: BuildFoldedNativeReceiver,
    pub argument: u8,
    pub literal_origin: ExpressionIdentity,
    pub literal: PreparedComparisonLiteral,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum PreparedComparisonLiteral {
    Null,
    Int(i64),
    Str(Box<str>),
    Bool(bool),
}

impl PreparedComparisonLiteral {
    pub(in crate::runtime::eval) fn ty(&self) -> Type {
        match self { Self::Null => Type::Null, Self::Int(_) => Type::Int, Self::Str(_) => Type::Str, Self::Bool(_) => Type::Bool }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct PreparedLiteralComparisonSlot {
    pub origin: ExpressionIdentity,
    pub receiver: NativeScalarReceiver,
    pub argument: u8,
    pub payload: Box<[u32]>,
    pub literal_origin: ExpressionIdentity,
    pub literal: PreparedComparisonLiteral,
    pub literal_tag: FullValueTag,
    pub literal_payload: Box<[u32]>,
}

impl PreparedLiteralComparisonSlot {
    pub(in crate::runtime::eval) fn retained_bytes(&self) -> usize {
        (self.payload.len() + self.literal_payload.len()) * std::mem::size_of::<u32>() + match &self.literal { PreparedComparisonLiteral::Str(value) => value.len(), _ => 0 }
    }
}

impl FullBuilder {
    pub(super) fn prepare_literal_comparison_slot(&mut self, instruction: u32, expression: ExpressionIdentity,
        owner: InstructionOwner, arguments: &[Option<TypeId>],
    ) -> Result<PreparedLiteralComparisonSlot, IrBuildError> {
        let (_, staged, staged_owner) = self.literal_comparison_rows.iter().find(|row| row.0 == instruction)
            .cloned().ok_or_else(|| unprepared("literal_comparison_original_slot_missing"))?;
        let original = &staged.receiver;
        if owner != staged_owner { return Err(unprepared("literal_comparison_original_slot_owner")); }
        let solved = self.solved.clone().ok_or_else(|| unprepared("literal_comparison_original_graph_missing"))?;
        let graph = &solved.graph;
        let operation = solved.operations.get(&expression).ok_or_else(|| unprepared("literal_comparison_original_operation_missing"))?;
        let types = arguments.iter().map(|argument| argument.map(|ty| graph_ground_type(graph, ty)).transpose()).collect::<Result<Vec<_>, _>>()
            .map_err(|_| unprepared("literal_comparison_original_domains"))?;
        let argument = staged.argument as usize;
        if argument > 1 || types.len() != 2 || types[1 - argument].as_ref() != Some(&staged.literal.ty())
            || !matches!(types.as_slice(), [Some(Type::Optional(_)), Some(Type::Null)] | [Some(Type::Null), Some(Type::Optional(_))]
                | [Some(Type::Int), Some(Type::Int)] | [Some(Type::Str), Some(Type::Str)] | [Some(Type::Bool), Some(Type::Bool)]) {
            return Err(unprepared("literal_comparison_original_domains"));
        }
        let literal_type = *solved.expressions.get(&staged.literal_origin).ok_or_else(|| unprepared("literal_comparison_original_literal_missing"))?;
        let literal_scope = solved.expression_scope(staged.literal_origin, operation.caller).map_err(|_| unprepared("literal_comparison_original_literal_scope"))?;
        graph.validate_scoped(ScopedRoot { ty: literal_type, scope: literal_scope }).map_err(|_| unprepared("literal_comparison_original_literal_scope"))?;
        if solved.expression_owners.get(&staged.literal_origin).copied() != operation.caller
            || graph_ground_type(graph, literal_type).map_err(|_| unprepared("literal_comparison_original_literal_type"))? != staged.literal.ty() {
            return Err(unprepared("literal_comparison_original_literal_changed"));
        }
        let expected = types[argument].as_ref().unwrap();
        let checked = *solved.expressions.get(&original.origin).ok_or_else(|| unprepared("literal_comparison_original_operand_missing"))?;
        let scope = solved.expression_scope(original.origin, operation.caller).map_err(|_| unprepared("literal_comparison_original_operand_scope"))?;
        graph.validate_scoped(ScopedRoot { ty: checked, scope }).map_err(|_| unprepared("literal_comparison_original_operand_scope"))?;
        if solved.expression_owners.get(&original.origin).copied() != operation.caller
            || graph_ground_type(graph, checked).map_err(|_| unprepared("literal_comparison_original_operand_type"))? != *expected {
            return Err(unprepared("literal_comparison_original_operand_changed"));
        }
        let receiver = if let Some(binding) = original.binding {
            let bindings = self.generic.as_ref().ok_or_else(|| unprepared("literal_comparison_original_binding_missing"))?
                .native_scalar_binding_sources().collect::<Result<Vec<_>, _>>().map_err(|_| unprepared("literal_comparison_original_binding_owner"))?;
            let mut applications = bindings.into_iter().filter(|(source, _)| *source == binding);
            let (_, application) = applications.next().ok_or_else(|| unprepared("literal_comparison_original_binding_missing"))?;
            if applications.next().is_some() { return Err(unprepared("literal_comparison_original_binding_ambiguous")); }
            NativeScalarReceiver::Binding { binding, application, slot: original.slot }
        } else {
            let declaration = operation.caller.ok_or_else(|| unprepared("literal_comparison_original_parameter_owner"))?;
            let callable = solved.declarations.get(&declaration).ok_or_else(|| unprepared("literal_comparison_original_declaration_missing"))?;
            let TypeNode::Arrow(arrow) = graph.node(graph.resolved(callable.signature).map_err(|_| unprepared("literal_comparison_original_signature"))?).map_err(|_| unprepared("literal_comparison_original_signature"))?
                else { return Err(unprepared("literal_comparison_original_signature")); };
            let parameter = arrow.params.get(original.slot as usize).ok_or_else(|| unprepared("literal_comparison_original_parameter_slot"))?;
            if parameter.label != original.name || graph_ground_type(graph, parameter.ty).map_err(|_| unprepared("literal_comparison_original_parameter_type"))? != *expected {
                return Err(unprepared("literal_comparison_original_parameter_changed"));
            }
            let InstructionOwner::Function(function) = owner else { return Err(unprepared("literal_comparison_original_parameter_owner")); };
            if let Some(&scope) = self.generic_declarations.get(&declaration) {
                NativeScalarReceiver::ScopedParameter { scope, name: original.name, slot: original.slot }
            } else {
                let signature = SignatureId::from_raw(self.store.functions[function.index()].signature).ok_or_else(|| unprepared("literal_comparison_original_parameter_signature"))?;
                NativeScalarReceiver::Parameter { declaration, signature, name: original.name, slot: original.slot }
            }
        };
        let payload: Box<[u32]> = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| unprepared("literal_comparison_original_payload"))?.into();
        if payload.len() != 3 { return Err(unprepared("literal_comparison_original_payload")); }
        let literal_tag = *self.store.values.get(payload[2] as usize).ok_or_else(|| unprepared("literal_comparison_original_literal_missing"))?;
        let literal_payload: Box<[u32]> = self.store.payload(self.store.value_data.get(payload[2] as usize).ok_or_else(|| unprepared("literal_comparison_original_literal_missing"))?.range()).map_err(|_| unprepared("literal_comparison_original_literal_payload"))?.into();
        let recipe = PreparedLiteralComparisonSlot { origin: original.origin, receiver, argument: argument as u8, payload, literal_origin: staged.literal_origin, literal: staged.literal, literal_tag, literal_payload };
        verify_literal(&self.store, &recipe).map_err(|error| IrBuildError::verification("literal_comparison_original_literal_changed", error))?;
        Ok(recipe)
    }
}

impl FullVerifier {
    pub(super) fn verify_literal_comparison_slot(store: &FullStore, generic: &GenericEvidenceStore, operation: &PreparedOperation,
        instruction: u32, owner: InstructionOwner, expected: &Type,
    ) -> Result<bool, IrVerifyError> {
        let recipe = operation.literal_comparison_slot.as_ref().ok_or_else(|| IrVerifyError::new("literal comparison lacks its original slot recipe"))?;
        let source = generic.operation_source(operation.source)?;
        let PreparedOperationAuthority::Language { operation: PreparedLanguageOperation::Equality { op }, .. } = operation.authority else { unreachable!() };
        let words = store.payload(store.data.get(instruction as usize).ok_or_else(|| IrVerifyError::new("literal comparison instruction is missing"))?.range())?;
        let Some(Some(TypeRef::Ground(ty))) = operation.arguments.get(recipe.argument as usize) else { return Err(IrVerifyError::new("literal comparison original operand role is missing")); };
        if *expected != Type::Bool || source.instruction != instruction || source.owner != owner
            || source.expected != operation.authority || generic.registered_instruction_origin(instruction, false) != Some((source.origin, owner))
            || !matches!(source.origin, OperationSourceOrigin::Expression(_)) || store.tags.get(instruction as usize) != Some(&FullTag::BoolLiteralCompareSlot)
            || words != recipe.payload.as_ref() || words.len() != 3 || words.first().and_then(|&index| store.binary_ops.get(index as usize)) != Some(&op)
 {
            return Err(IrVerifyError::new("literal comparison changes its original fused instruction or operand role"));
        }
        let Some(Some(TypeRef::Ground(literal_type))) = operation.arguments.get(1usize.checked_sub(recipe.argument as usize).ok_or_else(|| IrVerifyError::new("literal comparison operand role is invalid"))?) else { return Err(IrVerifyError::new("literal comparison literal role is missing")); };
        if store.semantic.to_type(*literal_type)? != recipe.literal.ty() { return Err(IrVerifyError::new("literal comparison changes its original literal domain")); }
        verify_literal(store, recipe)?;
        let slot = match recipe.receiver {
            NativeScalarReceiver::Parameter { declaration, signature, name, slot } => {
                let function = generic.checked_function(declaration)?;
                let (label, parameter, _) = store.semantic.signature_param(signature, slot as usize)?;
                if owner != InstructionOwner::Function(function.target) || function.signature != signature || label != name || parameter != *ty {
                    return Err(IrVerifyError::new("literal comparison changes its original parameter"));
                }
                slot
            }
            NativeScalarReceiver::ScopedParameter { scope, name, slot } => {
                let declaration = generic.scope(scope)?;
                let parameter = *declaration.parameters.get(slot as usize).ok_or_else(|| IrVerifyError::new("literal comparison scoped parameter is missing"))?;
                if owner != InstructionOwner::Function(declaration.owner) || declaration.parameter_names.get(slot as usize) != Some(&name)
                    || !generic.reference_equals_ground(&store.semantic, scope, parameter, store.semantic.to_type(*ty)?)? {
                    return Err(IrVerifyError::new("literal comparison changes its original scoped parameter"));
                }
                slot
            }
            NativeScalarReceiver::Binding { binding, application, slot } => {
                let application = generic.value_binding(application)?;
                if generic.value_binding_source(application.source)?.binding.named() != Some(binding) || application.contract.owner != owner
                    || application.contract.slot != slot || application.contract.binding_type != *ty {
                    return Err(IrVerifyError::new("literal comparison changes its original immutable binding"));
                }
                slot
            }
            _ => return Err(IrVerifyError::new("literal comparison changes its original slot authority kind")),
        };
        if words[1] != slot { return Err(IrVerifyError::new("literal comparison changes its original slot")); }
        Ok(true)
    }

    pub(in crate::runtime::eval::indexed::full) fn verify_literal_comparison_scopes(store: &FullStore, tree: &super::super::super::pattern::PatternTree) -> Result<(), IrVerifyError> {
        let Some(generic) = store.generic.as_deref() else { return Ok(()); };
        if !generic.operations().any(|(_, operation)| operation.literal_comparison_slot.is_some()) { return Ok(()); }
        let index = super::super::callable_prepare::CallableLexicalIndex::new(store, tree)?;
        let owners = store.generic_instruction_owners()?;
        let owner_key = |owner| match owner { InstructionOwner::Function(function) => (false, function.raw()), InstructionOwner::Driver(driver) => (true, driver) };
        let mut writes = std::collections::BTreeSet::new();
        for (instruction, tag) in store.tags.iter().enumerate() {
            if matches!(tag, FullTag::StmtAssign | FullTag::StmtAssignInt | FullTag::StmtAssignBool | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt | FullTag::StmtAssignPath)
                && let (Some(owner), Some(&slot)) = (owners[instruction], store.payload(store.data[instruction].range())?.first()) {
                writes.insert((owner_key(owner), slot));
            }
        }
        for (_, operation) in generic.operations() {
            let Some(recipe) = &operation.literal_comparison_slot else { continue; };
            let source = generic.operation_source(operation.source)?;
            let slot = match recipe.receiver { NativeScalarReceiver::Parameter { slot, .. } | NativeScalarReceiver::ScopedParameter { slot, .. } | NativeScalarReceiver::Binding { slot, .. } => slot, _ => return Err(IrVerifyError::new("literal comparison has another original receiver kind")) };
            if writes.contains(&(owner_key(source.owner), slot)) {
                return Err(IrVerifyError::new("literal comparison slot has an unprepared assignment"));
            }
            if let NativeScalarReceiver::Binding { application, .. } = recipe.receiver {
                if !index.dominates(tree, generic.value_binding(application)?.contract.instruction, source.instruction)? {
                    return Err(IrVerifyError::new("literal comparison is outside its original binding scope"));
                }
            }
        }
        Ok(())
    }
}

fn verify_literal(store: &FullStore, recipe: &PreparedLiteralComparisonSlot) -> Result<(), IrVerifyError> {
    let index = recipe.payload.get(2).copied().ok_or_else(|| IrVerifyError::new("literal comparison value reference is missing"))? as usize;
    let tag = store.values.get(index).copied().ok_or_else(|| IrVerifyError::new("literal comparison value is missing"))?;
    let words = store.payload(store.value_data.get(index).ok_or_else(|| IrVerifyError::new("literal comparison value payload is missing"))?.range())?;
    if tag != recipe.literal_tag || words != recipe.literal_payload.as_ref() { return Err(IrVerifyError::new("literal comparison changes its original literal encoding")); }
    let equal = match (&recipe.literal, tag, words) {
        (PreparedComparisonLiteral::Null, FullValueTag::Null, []) => true,
        (PreparedComparisonLiteral::Int(value), FullValueTag::Int, [low, high]) => ((*low as u64 | (*high as u64) << 32) as i64) == *value,
        (PreparedComparisonLiteral::Bool(value), FullValueTag::Bool, [word]) => *word == u32::from(*value),
        (PreparedComparisonLiteral::Str(value), FullValueTag::Str, [string]) => store.string(*string)? == value.as_ref(),
        _ => false,
    };
    if !equal { return Err(IrVerifyError::new("literal comparison changes its original literal value")); }
    Ok(())
}

impl FullVerifier {
    pub(in crate::runtime::eval::indexed) fn is_literal_comparison(pools: &SemanticPools, operation: &PreparedOperation) -> Result<bool, IrVerifyError> {
        if operation.literal_comparison_slot.is_none() || !matches!(operation.authority, PreparedOperationAuthority::Language {
            operation: PreparedLanguageOperation::Equality { op: BinaryOp::Eq | BinaryOp::Ne }, .. }) || operation.arguments.len() != 2 { return Ok(false); }
        let mut types = Vec::with_capacity(2);
        for reference in &operation.arguments {
            let Some(TypeRef::Ground(ty)) = reference else { return Ok(false); };
            types.push(pools.to_type(*ty)?);
        }
        Ok(matches!(types.as_slice(), [Type::Optional(_), Type::Null] | [Type::Null, Type::Optional(_)]
            | [Type::Int, Type::Int] | [Type::Str, Type::Str] | [Type::Bool, Type::Bool]))
    }

    pub(in crate::runtime::eval::indexed) fn verify_prepared_literal_comparison_contract(pools: &SemanticPools, operation: &PreparedOperation) -> Result<(), IrVerifyError> {
        if !Self::is_literal_comparison(pools, operation)? || !matches!(operation.authority,
            PreparedOperationAuthority::Language { argument_order: crate::sema::operation_graph::OperationArgumentOrder::SourceOrder, statement_result_is_unit: false, .. })
            || operation.receiver.is_some() || operation.binding.supplied_slots.as_ref() != [0, 1] || !operation.binding.default_slots.is_empty()
            || !operation.binding.operands.is_empty() || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some()
            || operation.fallback_lowering.is_some() || operation.original_integer_addition.is_some() || operation.range_lowering.is_some()
            || operation.effects.creation != crate::sema::inference::EffectSet::EMPTY || !operation.effects.inputs.is_empty() || !operation.effects.outputs.is_empty() {
            return Err(IrVerifyError::new("literal comparison changes its original equality domains, binding or effects"));
        }
        let TypeRef::Ground(result) = operation.result else { return Err(IrVerifyError::new("literal comparison result is not ground")); };
        if pools.to_type(result)? != Type::Bool { return Err(IrVerifyError::new("literal comparison changes its original Bool result")); }
        let recipe = operation.literal_comparison_slot.as_ref().unwrap();
        if recipe.argument > 1 { return Err(IrVerifyError::new("literal comparison original operand role is invalid")); }
        let Some(TypeRef::Ground(literal)) = operation.arguments[1 - recipe.argument as usize] else { return Err(IrVerifyError::new("literal comparison literal domain is missing")); };
        if pools.to_type(literal)? != recipe.literal.ty() { return Err(IrVerifyError::new("literal comparison changes its original literal domain")); }
        Ok(())
    }

    pub(in crate::runtime::eval::indexed::full) fn verify_literal_comparison_operand(store: &FullStore, generic: &GenericEvidenceStore,
        instruction: u32, owner: InstructionOwner, expected: &Type, _instance: Option<InstantiationId>, _active: &mut Vec<u32>,
    ) -> Result<bool, IrVerifyError> {
        let Some(operation) = generic.operation_at(instruction)? else { return Ok(false); };
        if operation.literal_comparison_slot.is_none() { return Ok(false); }
        Self::verify_prepared_literal_comparison_contract(&store.semantic, operation)?;
        Self::verify_literal_comparison_slot(store, generic, operation, instruction, owner, expected)
    }
}
