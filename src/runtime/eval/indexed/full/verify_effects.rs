use super::{FullCallableContract, FullDecoder, FullStore, FullTag, IrVerifyError, Type, schema};
use crate::modules::signature::NativeEffectRule;
use crate::sema::types::CallableKind;
use crate::syntax::node::Effect;
use std::collections::BTreeMap;

struct OwnerEffects {
    kind: CallableKind,
    effects: Option<Vec<Effect>>,
}

impl OwnerEffects {
    fn for_function(store: &FullStore, index: usize) -> Result<Self, IrVerifyError> {
        let metadata = store.function_metadata[index];
        let kind = metadata.callable_kind;
        if metadata.reserved != [0; 2] || metadata.flags & 1 != u8::from(kind == CallableKind::Proc) {
            return Err(IrVerifyError::new("callable kind disagrees with executable dispatch"));
        }
        let effects = match store.functions[index].contract {
            FullCallableContract::Ground(signature) => store.semantic.signature_effects(signature)?,
            FullCallableContract::Scheme(id) => {
                let scheme = store.scheme(id)?;
                if scheme.callable_kind != kind {
                    return Err(IrVerifyError::new("callable kind disagrees with its scheme"));
                }
                if scheme.has_effects {
                    let effects = scheme.effects.bounds(store.scheme_effects.len())
                        .ok_or_else(|| IrVerifyError::new("callable effect range is invalid"))?;
                    if effects.len() > Effect::ALL.len() {
                        return Err(IrVerifyError::new("callable effects exceed the capability set"));
                    }
                    Some(store.scheme_effects[effects].to_vec())
                } else { None }
            }
        };
        Ok(Self { kind, effects: if kind == CallableKind::Pure { Some(effects.unwrap_or_default()) } else { effects } })
    }

    fn require(&self, required: Option<&[Effect]>, pure: bool, captured: bool) -> Result<(), IrVerifyError> {
        if self.kind == CallableKind::Pure && !pure {
            return Err(IrVerifyError::new("pure executable body contains an impure operation"));
        }
        let Some(ceiling) = &self.effects else { return Ok(()) };
        let required = required.or(if pure { Some(&[]) } else { None }).ok_or_else(|| IrVerifyError::new("finite executable body contains an unknown effect contract"))?;
        for effect in required {
            if *effect == Effect::Error && (captured || self.kind == CallableKind::Pure) { continue; }
            if !crate::sema::check::Checker::effects_covers(ceiling, effect) {
                return Err(IrVerifyError::new(format!("executable operation requires effect {} outside its owner contract", effect.as_str())));
            }
        }
        Ok(())
    }
}

fn captures_error(decoder: &FullDecoder<'_>, instruction: usize, cache: &mut BTreeMap<usize, bool>, parent_of: impl Fn(usize) -> Result<Option<usize>, IrVerifyError>) -> Result<bool, IrVerifyError> {
    if let Some(&captured) = cache.get(&instruction) { return Ok(captured); }
    let mut path = Vec::new();
    let mut child = instruction;
    let captured = loop {
        if let Some(&captured) = cache.get(&child) { break captured; }
        let Some(parent) = parent_of(child)? else { break false };
        path.push(child);
        match decoder.store.tags[parent] {
            FullTag::ExprCapture => break true,
            FullTag::ExprRetry => {
                let schema::ExprView::Retry { delays, .. } = decoder.expression_view(parent)? else { unreachable!() };
                let mut delay = false;
                for value in delays { delay |= value?.raw() as usize == child; }
                if !delay { break true; }
            }
            FullTag::StmtWith => {
                let schema::StmtView::With { bindings, .. } = decoder.statement_view(parent)? else { unreachable!() };
                let mut captures = false;
                for binding in bindings { captures |= binding?.1.raw() as usize == child; }
                if captures { break true; }
            }
            _ => {}
        }
        child = parent;
    };
    for child in path { cache.insert(child, captured); }
    cache.insert(instruction, captured);
    Ok(captured)
}

pub(super) fn require_native(decoder: &FullDecoder<'_>, instruction: usize, effects: &[Effect], pure: bool, parent_of: impl Fn(usize) -> Result<Option<usize>, IrVerifyError>) -> Result<(), IrVerifyError> {
    let Some(function) = super::IrFunctionId::from_raw(decoder.owner) else { return Ok(()) };
    if super::driver_owner_index(decoder.owner).is_some() { return Ok(()); }
    let owner = OwnerEffects::for_function(decoder.store, function.index())?;
    let captured = owner.kind != CallableKind::Pure && effects.contains(&Effect::Error)
        && captures_error(decoder, instruction, &mut BTreeMap::new(), parent_of)?;
    owner.require(Some(effects), pure, captured)
}

pub(super) fn verify_owner(decoder: &FullDecoder<'_>, index: usize) -> Result<(), IrVerifyError> {
    let owner = OwnerEffects::for_function(decoder.store, index)?;
    let mutable_captures = if owner.kind == CallableKind::Pure {
        let captures = decoder.store.functions[index].captures.bounds(decoder.store.captures.len())
            .ok_or_else(|| IrVerifyError::new("pure callable capture range is invalid"))?;
        decoder.store.captures[captures].iter().filter_map(|capture|
            (capture.slot_and_flags & (1 << 31) != 0).then_some((capture.slot_and_flags & !(1 << 31)) as usize))
            .collect::<std::collections::BTreeSet<_>>()
    } else { std::collections::BTreeSet::new() };
    let mut captured = BTreeMap::new();
    let require = |instruction, effects: Option<&[Effect]>, pure, captured: &mut BTreeMap<usize, bool>| {
        let captures = owner.kind != CallableKind::Pure && effects.is_some_and(|effects| effects.contains(&Effect::Error))
            && captures_error(decoder, instruction, captured, |at| decoder.instruction_parent_of(at))?;
        owner.require(effects, pure, captures).map_err(|error| IrVerifyError::new(format!("{} at callable {index}, instruction {instruction} {:?}", error.message, decoder.store.tags[instruction])))
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
                    require(instruction, bridge.sig.effects.as_deref(), bridge.pure, &mut captured)?;
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
                let target = OwnerEffects::for_function(decoder.store, target)?;
                require(instruction, target.effects.as_deref(), target.kind == CallableKind::Pure, &mut captured)?;
            }
            FullTag::ExprTypedCall => {
                let schema::ExprView::TypedCall { signature, pure, .. } = decoder.expression_view(instruction)? else { unreachable!() };
                let signature = decoder.store.semantic.typed_callable_signature(signature.id)?;
                let effects = decoder.store.semantic.signature_effects(signature)?;
                require(instruction, effects.as_deref().or(if pure { Some(&[]) } else { None }), pure, &mut captured)?;
            }
            FullTag::ExprDynamicCall => {
                require(instruction, None, owner.kind == CallableKind::Pure, &mut captured)?;
            }
            FullTag::ExprExternalCall => require(instruction, None, false, &mut captured)?,
            FullTag::ExprGlob | FullTag::ExprFsFiles | FullTag::ExprFsWalk | FullTag::ExprFsList | FullTag::ExprFsTempDir
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
            FullTag::ExprTry | FullTag::StmtAssert => require(instruction, Some(&[Effect::Error]), true, &mut captured)?,
            FullTag::StmtExpr => {
                let schema::StmtView::Expr { value, .. } = decoder.statement_view(instruction)? else { unreachable!() };
                if owner.kind != CallableKind::Pure && owner.effects.as_ref().is_some_and(|effects| !crate::sema::check::Checker::effects_covers(effects, &Effect::Error)) {
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
        let FullCallableContract::Ground(signature) = decoder.store.functions[index].contract else {
            return Err(IrVerifyError::new("stream callable entry has no stream return contract"));
        };
        let result = decoder.store.semantic.signature_return_type(signature)?;
        if !matches!(decoder.store.semantic.to_type(result)?, Type::Stream(_)) {
            return Err(IrVerifyError::new("stream callable kind requires a lazy stream return boundary"));
        }
    }
    Ok(())
}

pub(super) fn needs_operand_proof(store: &FullStore, instruction: usize) -> Result<bool, IrVerifyError> {
    let tag = store.tags[instruction];
    if tag == FullTag::ExprDynamicCall { return Ok(true); }
    if !matches!(tag, FullTag::ExprModuleCall | FullTag::ExprMethod) { return Ok(false); }
    let data = store.data.get(instruction).ok_or_else(|| IrVerifyError::new("native operation has no instruction payload"))?;
    let payload = store.payload(data.range())?;
    let rule = match tag {
        FullTag::ExprModuleCall => {
            let raw = *payload.first().ok_or_else(|| IrVerifyError::new("native operation selector is absent"))?;
            let op = *store.runtime_ops.get(raw as usize).ok_or_else(|| IrVerifyError::new("native operation selector is invalid"))?;
            crate::modules::signature::native_static_operation_effect_rule(op)
        }
        FullTag::ExprMethod => {
            let raw = *payload.get(1).ok_or_else(|| IrVerifyError::new("method operation selector is absent"))?;
            crate::modules::signature::native_method_effect_rule(store.string(raw)?)
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
