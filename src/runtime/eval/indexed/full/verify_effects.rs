use super::{FullCallableContract, FullDecoder, FullStore, FullTag, IrVerifyError, Type, schema};
use crate::modules::signature::NativeEffectRule;
use crate::sema::types::CallableKind;
use crate::syntax::node::Effect;
use crate::sema::effects::{EffectContract, EffectSet};
use crate::sema::schemes::SchemeEffectParameterId;
use super::verify_schemes::{ScopedEffectProof, VerifiedSchemeScopes};
use std::collections::BTreeMap;

struct OwnerEffects {
    kind: CallableKind,
    function: usize,
}

impl OwnerEffects {
    fn for_function(store: &FullStore, index: usize) -> Result<Self, IrVerifyError> {
        let metadata = store.function_metadata.get(index).ok_or_else(|| IrVerifyError::new("effect callable metadata is out of bounds"))?;
        let kind = metadata.callable_kind;
        if metadata.reserved != [0; 2] || metadata.flags & 1 != u8::from(kind == CallableKind::Proc) {
            return Err(IrVerifyError::new("callable kind disagrees with executable dispatch"));
        }
        if let FullCallableContract::Scheme(id) = store.functions.get(index)
            .ok_or_else(|| IrVerifyError::new("effect callable owner is out of bounds"))?.contract
            && store.scheme(id)?.callable_kind != kind
        { return Err(IrVerifyError::new("callable kind disagrees with its scheme")); }
        Ok(Self { kind, function: index })
    }

    fn require_contract(&self, proof: &ScopedEffectProof<'_>, budget: &mut super::verify_schemes::SchemeInstantiationBudget, required: &EffectContract<SchemeEffectParameterId>, pure: bool, captured: bool) -> Result<(), IrVerifyError> {
        if self.kind == CallableKind::Pure && !pure {
            return Err(IrVerifyError::new("pure executable body contains an impure operation"));
        }
        proof.require(self.function, required, captured || self.kind == CallableKind::Pure, budget)
    }

    fn require(&self, proof: &ScopedEffectProof<'_>, budget: &mut super::verify_schemes::SchemeInstantiationBudget, required: Option<&[Effect]>, pure: bool, captured: bool) -> Result<(), IrVerifyError> {
        if self.kind == CallableKind::Pure && !pure {
            return Err(IrVerifyError::new("pure executable body contains an impure operation"));
        }
        let Some(required) = required.or(if pure { Some(&[]) } else { None }) else {
            return self.require_contract(proof, budget, &EffectContract::Unknown, pure, captured)
                .map_err(|error| IrVerifyError::new(format!("finite executable body contains an unknown effect contract: {}", error.message)));
        };
        for effect in required {
            self.require_contract(proof, budget, &EffectContract::finite(EffectSet::singleton(effect.clone())), pure, captured)
                .map_err(|error| IrVerifyError::new(format!("executable operation requires effect {} outside its owner contract: {}", effect.as_str(), error.message)))?;
        }
        Ok(())
    }
}

pub(super) fn captures_child_error(decoder: &FullDecoder<'_>, parent: usize, child: usize) -> Result<bool, IrVerifyError> {
    Ok(match decoder.store.tags[parent] {
        FullTag::ExprCapture => true,
        FullTag::ExprRetry => decoder.store.tags[child].kind() != schema::InstructionKind::Expr,
        FullTag::StmtWith => decoder.store.tags[child].kind() == schema::InstructionKind::Expr,
        _ => false,
    })
}

fn captures_error(decoder: &FullDecoder<'_>, instruction: usize, cache: &mut BTreeMap<usize, bool>, parent_of: impl Fn(usize) -> Result<Option<usize>, IrVerifyError>) -> Result<bool, IrVerifyError> {
    if let Some(&captured) = cache.get(&instruction) { return Ok(captured); }
    let mut path = Vec::new();
    let mut child = instruction;
    let captured = loop {
        if let Some(&captured) = cache.get(&child) { break captured; }
        let Some(parent) = parent_of(child)? else { break false };
        path.push(child);
        if captures_child_error(decoder, parent, child)? { break true; }
        child = parent;
    };
    for child in path { cache.insert(child, captured); }
    cache.insert(instruction, captured);
    Ok(captured)
}

pub(super) fn function_effects(store: &FullStore, index: usize) -> Result<Option<Vec<Effect>>, IrVerifyError> {
    let owner = OwnerEffects::for_function(store, index)?;
    let effects = match store.functions[index].contract {
        FullCallableContract::Ground(signature) => store.semantic.signature_effects(signature)?,
        FullCallableContract::Scheme(id) => store.scheme(id)?.effects.concrete(store)?,
    };
    Ok(if owner.kind == CallableKind::Pure { Some(effects.unwrap_or_default()) } else { effects })
}

pub(super) fn require_callable(decoder: &FullDecoder<'_>, scopes: &VerifiedSchemeScopes, budget: &mut super::verify_schemes::SchemeInstantiationBudget, instruction: usize, effects: Option<&[Effect]>, pure: bool, parent_of: impl Fn(usize) -> Result<Option<usize>, IrVerifyError>) -> Result<(), IrVerifyError> {
    let Some(function) = super::IrFunctionId::from_raw(decoder.owner) else { return Ok(()) };
    if super::driver_owner_index(decoder.owner).is_some() { return Ok(()); }
    let owner = OwnerEffects::for_function(decoder.store, function.index())?;
    let captured = owner.kind != CallableKind::Pure && effects.is_some_and(|effects| effects.contains(&Effect::Error))
        && captures_error(decoder, instruction, &mut BTreeMap::new(), parent_of)?;
    owner.require(&ScopedEffectProof::new(decoder.store, scopes), budget, effects, pure, captured)
}

pub(super) fn verify_owner(decoder: &FullDecoder<'_>, index: usize, scopes: &VerifiedSchemeScopes) -> Result<(), IrVerifyError> {
    let owner = OwnerEffects::for_function(decoder.store, index)?;
    verify_script_implementation(decoder.store, index, &owner)?;
    let mutable_captures = if owner.kind == CallableKind::Pure {
        let captures = decoder.store.functions[index].captures.bounds(decoder.store.captures.len())
            .ok_or_else(|| IrVerifyError::new("pure callable capture range is invalid"))?;
        decoder.store.captures[captures].iter().filter_map(|capture|
            (capture.slot_and_flags & (1 << 31) != 0).then_some((capture.slot_and_flags & !(1 << 31)) as usize))
            .collect::<std::collections::BTreeSet<_>>()
    } else { std::collections::BTreeSet::new() };
    let mut captured = BTreeMap::new();
    let proof = ScopedEffectProof::new(decoder.store, scopes);
    let budget = std::cell::RefCell::new(super::verify_schemes::SchemeInstantiationBudget::new(decoder.store));
    let require = |instruction, effects: Option<&[Effect]>, pure, captured: &mut BTreeMap<usize, bool>| {
        let captures = owner.kind != CallableKind::Pure && effects.is_some_and(|effects| effects.contains(&Effect::Error))
            && captures_error(decoder, instruction, captured, |at| decoder.instruction_parent_of(at))?;
        owner.require(&proof, &mut budget.borrow_mut(), effects, pure, captures).map_err(|error| IrVerifyError::new(format!("{} at callable {index}, instruction {instruction} {:?}", error.message, decoder.store.tags[instruction])))
    };
    for instruction in decoder.instruction_range.clone() {
        let tag = decoder.store.tags[instruction];
        if !mutable_captures.is_empty() && matches!(tag,
            FullTag::StmtAssign | FullTag::StmtAssignInt | FullTag::StmtAssignBool
            | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt | FullTag::StmtAssignPath)
        {
            let (writes, _, _) = decoder.statement_written_slots(instruction)?;
            if writes.iter().any(|slot| mutable_captures.contains(slot)) {
                return Err(IrVerifyError::new("pure executable body writes shared mutable state"));
            }
        }
        match tag {
            FullTag::ExprModuleCall => {
                let schema::ExprView::ModuleCall { op, .. } = decoder.expression_view(instruction)? else { unreachable!() };
                if let Some(bridge) = crate::stdlib::bridge_contract(op) {
                    let effects = bridge.sig.effects.concrete()
                        .map_err(|_| IrVerifyError::new("bridge effect contract is unresolved"))?;
                    require(instruction, effects.as_deref(), bridge.kind == crate::sema::types::CallableKind::Pure, &mut captured)?;
                } else { match crate::modules::signature::native_static_operation_effect_rule(op) {
                    Some(NativeEffectRule::Uniform { effects, pure }) => require(instruction, Some(effects), pure, &mut captured)?,
                    Some(NativeEffectRule::OperandDependent) => {}
                    None => require(instruction, None, false, &mut captured)?,
                } }
            }
            FullTag::ExprMethod => {
                let schema::ExprView::Method { name, .. } = decoder.expression_view(instruction)? else { unreachable!() };
                match crate::modules::signature::native_method_effect_rule(name) {
                    Some(NativeEffectRule::Uniform { effects, pure }) => require(instruction, Some(effects), pure, &mut captured)?,
                    Some(NativeEffectRule::OperandDependent) => {}
                    None => require(instruction, None, false, &mut captured)?,
                }
            }
            FullTag::ExprCall | FullTag::ExprDirectPureCall | FullTag::ExprSelfCall => {
                let target = match decoder.expression_view(instruction)? {
                    schema::ExprView::Call { function, .. } | schema::ExprView::DirectPureCall { function, .. } => function.id.index(),
                    schema::ExprView::SelfCall { .. } => index,
                    _ => unreachable!(),
                };
                match script_implementation_rule(decoder.store, target)? {
                    Some(NativeEffectRule::Uniform { effects, pure }) => require(instruction, Some(effects), pure, &mut captured)?,
                    Some(NativeEffectRule::OperandDependent) => require(instruction, None, false, &mut captured)?,
                    None => {
                        let effects = proof.call_contract(index, instruction, target, &mut budget.borrow_mut())?;
                        let target = OwnerEffects::for_function(decoder.store, target)?;
                        match effects.concrete() {
                            Ok(effects) => require(instruction, effects.as_deref(), target.kind == CallableKind::Pure, &mut captured)?,
                            Err(_) => {
                                let captures = owner.kind != CallableKind::Pure
                                    && captures_error(decoder, instruction, &mut captured, |at| decoder.instruction_parent_of(at))?;
                                owner.require_contract(&proof, &mut budget.borrow_mut(), &effects, target.kind == CallableKind::Pure, captures)?;
                            }
                        }
                    }
                }
            }
            FullTag::ExprTypedCall => {
                let schema::ExprView::TypedCall { signature, kind, .. } = decoder.expression_view(instruction)? else { unreachable!() };
                let signature = decoder.store.semantic.typed_callable_signature(signature.id)?;
                let effects = decoder.store.semantic.signature_effects(signature)?;
                let pure = kind == CallableKind::Pure;
                require(instruction, effects.as_deref().or(if pure { Some(&[]) } else { None }), pure, &mut captured)?;
            }
            FullTag::ExprDynamicCall => {}
            FullTag::ExprExternalCall => {
                let schema::ExprView::ExternalCall { function, .. } = decoder.expression_view(instruction)? else { unreachable!() };
                let rule = crate::stdlib::find_by_namespace(&function.namespace.as_str())
                    .and_then(|module| crate::modules::signature::script_implementation_effect_rule(module.identity, &function.member.as_str()));
                match rule {
                    Some(NativeEffectRule::Uniform { effects, pure }) => require(instruction, Some(effects), pure, &mut captured)?,
                    Some(NativeEffectRule::OperandDependent) | None => require(instruction, None, false, &mut captured)?,
                }
            }
            FullTag::ExprFsFiles | FullTag::ExprFsWalk => {
                require(instruction, Some(&[Effect::Fs]), false, &mut captured)?;
                let (schema::ExprView::FsFiles { result_wrapped, .. } | schema::ExprView::FsWalk { result_wrapped, .. }) = decoder.expression_view(instruction)? else { unreachable!() };
                if !result_wrapped { require(instruction, Some(&[Effect::Error]), false, &mut captured)?; }
            }
            FullTag::ExprGlob | FullTag::ExprFsList | FullTag::ExprFsTempDir
            | FullTag::ExprFsMkdir | FullTag::ExprFsRemove | FullTag::ExprPathReadText | FullTag::ExprPathReadBytes
            | FullTag::ExprPathExists | FullTag::ExprPathExecutable | FullTag::ExprPathDu | FullTag::ExprPathMetadata
            | FullTag::ExprPathReadlink | FullTag::ExprPathResolve | FullTag::ExprPathWrite | FullTag::ExprPathMkdir
            | FullTag::ExprPathRemove | FullTag::ExprArchiveTarCreate => require(instruction, Some(&[Effect::Fs]), false, &mut captured)?,
            FullTag::ExprRunCapture => {
                require(instruction, Some(&[Effect::Process]), false, &mut captured)?;
                let schema::ExprView::RunCapture(run) = decoder.expression_view(instruction)? else { unreachable!() };
                if run.propagate || (matches!(run.kind, crate::syntax::node::RunKind::Plain | crate::syntax::node::RunKind::Status)
                    && has_exit_policy(run.operands)?) {
                    require(instruction, Some(&[Effect::Error]), false, &mut captured)?;
                }
            }
            FullTag::ExprRunPipeline => {
                require(instruction, Some(&[Effect::Process]), false, &mut captured)?;
                let schema::ExprView::RunPipeline { segments, propagate, .. } = decoder.expression_view(instruction)? else { unreachable!() };
                let mut accepts = false;
                for segment in segments { accepts |= has_exit_policy(segment?.operands)?; }
                if propagate || accepts { require(instruction, Some(&[Effect::Error]), false, &mut captured)?; }
            }
            FullTag::StmtRun => {
                require(instruction, Some(&[Effect::Process]), false, &mut captured)?;
                let schema::StmtView::Run { propagate_result, .. } = decoder.statement_view(instruction)? else { unreachable!() };
                if propagate_result { require(instruction, Some(&[Effect::Error]), false, &mut captured)?; }
            }
            FullTag::ExprSpawnRun | FullTag::ExprSpawnCommand | FullTag::ExprWait => require(instruction, Some(&[Effect::Process]), false, &mut captured)?,
            FullTag::StmtCd => require(instruction, Some(&[Effect::Env, Effect::Error]), false, &mut captured)?,
            FullTag::StmtEnv => require(instruction, Some(&[Effect::Env]), false, &mut captured)?,
            FullTag::ExprContextScope => {
                let schema::ExprView::ContextScope { kind, .. } = decoder.expression_view(instruction)? else { unreachable!() };
                let effect = match kind {
                    crate::syntax::arena::ContextScopeKind::Cwd | crate::syntax::arena::ContextScopeKind::Env => Effect::Env,
                    crate::syntax::arena::ContextScopeKind::Within => Effect::Time,
                };
                require(instruction, Some(&[effect]), false, &mut captured)?;
            }
            FullTag::ExprRetry => {
                let schema::ExprView::Retry { delays, .. } = decoder.expression_view(instruction)? else { unreachable!() };
                if delays.len() != 0 { require(instruction, Some(&[Effect::Time]), false, &mut captured)?; }
            }
            FullTag::ExprAbort => require(instruction, Some(&[Effect::Process]), false, &mut captured)?,
            FullTag::ExprTry | FullTag::StmtAssert => require(instruction, Some(&[Effect::Error]), true, &mut captured)?,
            FullTag::StmtExpr => {
                let schema::StmtView::Expr { value, .. } = decoder.statement_view(instruction)? else { unreachable!() };
                if owner.kind != CallableKind::Pure
                    && !proof.permits_direct(index, &Effect::Error)?
                    && !matches!(decoder.expression_view(value.raw() as usize)?, schema::ExprView::Abort { .. })
                {
                    let value = value.raw() as usize;
                    let result = if let Some(ty) = decoder.expression_checked_type_id(value)? {
                        matches!(decoder.store.semantic.type_tag(ty)?, super::super::semantic::TypeTag::Result | super::super::semantic::TypeTag::Any)
                    } else {
                        let ty = decoder.derive_checked_expression_type(value)?.ok_or_else(|| IrVerifyError::new("implicit discard has no executable value contract"))?;
                        ty.is_result() || ty == Type::Any
                    };
                    if result { require(instruction, Some(&[Effect::Error]), true, &mut captured)?; }
                }
            }
            FullTag::StmtPrint => {
                require(instruction, Some(&[]), false, &mut captured)?;
                let schema::StmtView::Print { propagate_result, .. } = decoder.statement_view(instruction)? else { unreachable!() };
                if propagate_result { require(instruction, Some(&[Effect::Error]), false, &mut captured)?; }
            }
            FullTag::StmtDefer => require(instruction, Some(&[]), false, &mut captured)?,
            FullTag::StmtYield | FullTag::StmtYieldDelegate if owner.kind != CallableKind::Stream => {
                return Err(IrVerifyError::new("yield instruction requires a stream callable entry"));
            }
            FullTag::StmtReturn if owner.kind == CallableKind::Stream => {
                let schema::StmtView::Return { value } = decoder.statement_view(instruction)? else { unreachable!() };
                if decoder.derive_checked_expression_type(value.raw() as usize)? != Some(Type::Unit) {
                    return Err(IrVerifyError::new("stream callable entry returns an eager value"));
                }
            }
            _ => {}
        }
    }
    if owner.kind == CallableKind::Stream {
        let result = match decoder.store.functions[index].contract {
            FullCallableContract::Ground(signature) => super::lowered_return_kind(decoder.store.semantic.to_type(decoder.store.semantic.signature_return_type(signature)?)?)?,
            FullCallableContract::Scheme(id) => super::scheme_return_kind(decoder.store, decoder.store.scheme(id)?.return_type)?,
        };
        if result != super::LoweredReturnKind::Plain(super::LoweredType::Stream) {
            return Err(IrVerifyError::new("stream callable kind requires a lazy stream return boundary"));
        }
    }
    Ok(())
}

fn verify_script_implementation(store: &FullStore, index: usize, owner: &OwnerEffects) -> Result<(), IrVerifyError> {
    let Some(rule) = script_implementation_rule(store, index)? else { return Ok(()); };
    let NativeEffectRule::Uniform { effects, pure } = rule else {
        return Err(IrVerifyError::new("script implementation has ambiguous public effects"));
    };
    if (owner.kind == CallableKind::Pure) != pure || owner.kind == CallableKind::Stream {
        return Err(IrVerifyError::new("script implementation kind differs from its registry contract"));
    }
    let actual_effects = function_effects(store, index)?;
    let actual = actual_effects.as_deref().ok_or_else(|| IrVerifyError::new("script implementation has unknown effects"))?;
    let return_kind = match store.functions[index].contract {
        FullCallableContract::Ground(signature) => super::lowered_return_kind(store.semantic.to_type(store.semantic.signature_return_type(signature)?)?)?,
        FullCallableContract::Scheme(id) => super::scheme_return_kind(store, store.scheme(id)?.return_type)?,
    };
    let captures_error = owner.kind == CallableKind::Proc && matches!(return_kind, super::LoweredReturnKind::Result(_));
    if actual.iter().any(|effect| !(*effect == Effect::Error && captures_error)
        && !crate::sema::check::Checker::effects_covers(effects, effect)) {
        return Err(IrVerifyError::new("script implementation effects exceed its registry contract"));
    }
    Ok(())
}

fn script_implementation_rule(store: &FullStore, index: usize) -> Result<Option<NativeEffectRule>, IrVerifyError> {
    let metadata = store.function_metadata[index];
    if metadata.owner == super::IR_NONE { return Ok(None); }
    let Some(module) = crate::stdlib::find_by_namespace(store.string(metadata.owner)?) else { return Ok(None); };
    Ok(crate::modules::signature::script_implementation_effect_rule(module.identity, store.string(store.functions[index].name)?))
}

pub(super) fn needs_operand_proof(store: &FullStore, instruction: usize) -> Result<bool, IrVerifyError> {
    let tag = store.tags[instruction];
    if tag == FullTag::ExprTry {
        return store.try_operand(instruction).map(|operand| operand.is_some_and(|value| matches!(store.tags[value], FullTag::ExprErr | FullTag::ExprMatch)));
    }
    if matches!(tag, FullTag::ExprAbort | FullTag::ExprDynamicCall | FullTag::ExprTypedCall | FullTag::ExprProcessCommandArgv | FullTag::ExprProcessCommandBuilder
        | FullTag::ExprSpawnRun | FullTag::ExprSpawnCommand | FullTag::ExprWait | FullTag::ExprCapture) { return Ok(true); }
    if !matches!(tag, FullTag::ExprModuleCall | FullTag::ExprMethod) { return Ok(false); }
    let data = store.data.get(instruction).ok_or_else(|| IrVerifyError::new("native operation has no instruction payload"))?;
    let payload = store.payload(data.range())?;
    let rule = match tag {
        FullTag::ExprModuleCall => {
            let raw = *payload.first().ok_or_else(|| IrVerifyError::new("native operation selector is absent"))?;
            let op = *store.runtime_ops.get(raw as usize).ok_or_else(|| IrVerifyError::new("native operation selector is invalid"))?;
            if crate::modules::signature::native_static_operation_requires_producer_proof(op) { return Ok(true); }
            crate::modules::signature::native_static_operation_effect_rule(op)
        }
        FullTag::ExprMethod => {
            let raw = *payload.get(1).ok_or_else(|| IrVerifyError::new("method operation selector is absent"))?;
            let name = store.string(raw)?;
            if crate::modules::signature::native_method_requires_receiver_proof(name) { return Ok(true); }
            crate::modules::signature::native_method_effect_rule(name)
        }
        _ => unreachable!(),
    };
    Ok(rule == Some(NativeEffectRule::OperandDependent))
}

fn has_exit_policy(operands: super::decode::RunOperandsView<'_>) -> Result<bool, IrVerifyError> {
    let mut accepts = false;
    for option in operands.options { accepts |= matches!(option?, super::decode::RunOptionView::Accept(_)); }
    Ok(accepts)
}

#[cfg(test)]
mod tests;
