use super::{LoweredValue, RuntimeError, RuntimeOp, Span, Type, Value};
use crate::modules::cli::{ArgValueType, CliDescriptorPlan, parse_scalar_word};
use crate::modules::signature::{ImplBinding, ModuleFnSig};
use std::sync::Arc;

/// Prepared interpretation accompanies the same slots used by ordinary calls.
#[derive(Clone, Debug)]
pub(super) enum ModuleCallPlan {
    SignatureCli(Arc<CliDescriptorPlan>),
    CommandArguments(CommandArgumentsPlan),
}

#[derive(Clone, Debug)]
pub(super) struct CommandArgumentsPlan {
    pub signature: &'static ModuleFnSig,
    pub supplied_slots: Vec<bool>,
    /// Conversion order follows authored operands, independent of slot order.
    pub word_conversions: Vec<CommandWordConversion>,
    pub flag_slots: Vec<usize>,
}

#[derive(Clone, Debug)]
pub(super) struct CommandWordConversion {
    pub slot: usize,
    pub scalar: ArgValueType,
    pub span: Span,
}

pub(super) fn command_word_scalar(ty: &Type) -> Option<ArgValueType> {
    Some(match ty {
        Type::Str => ArgValueType::Str,
        Type::Int => ArgValueType::Int,
        Type::UInt => ArgValueType::UInt,
        Type::Bool => ArgValueType::Bool,
        Type::Path => ArgValueType::Path,
        Type::Duration => ArgValueType::Duration,
        _ => return None,
    })
}

impl ModuleCallPlan {
    pub(super) fn same_identity(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::SignatureCli(left), Self::SignatureCli(right)) => Arc::ptr_eq(left, right),
            _ => std::ptr::eq(self, other),
        }
    }

    pub(super) fn matches_operation(&self, op: RuntimeOp) -> bool {
        match self {
            Self::SignatureCli(plan) => plan.matches_operation(op),
            Self::CommandArguments(plan) => plan.signature.op == op,
        }
    }

    pub(super) fn verify_arguments(&self, op: RuntimeOp, supplied: &[bool]) -> Result<(), String> {
        if !self.matches_operation(op) {
            return Err("module call plan does not match its operation".into());
        }
        let Self::CommandArguments(plan) = self else { return Ok(()); };
        let signature = plan.signature;
        if !signature.command || signature.pure || signature.binding != ImplBinding::Native
            || !matches!(&signature.return_ty, Type::Result(ok, _) if **ok == Type::Unit)
        {
            return Err("module command requires an eligible native signature".into());
        }
        if supplied.len() != signature.params.len() || supplied != plan.supplied_slots
            || signature.params.iter().zip(supplied).any(|(param, present)| !present && !param.defaulted)
        {
            return Err("module command slots do not match its checked signature".into());
        }
        let mut interpreted = vec![false; supplied.len()];
        for &slot in &plan.flag_slots {
            let Some(param) = signature.params.get(slot) else { return Err("module command flag slot is out of bounds".into()); };
            if !supplied[slot] || !param.defaulted || param.ty != Type::Bool || interpreted[slot] {
                return Err("module command flag does not match a supplied Boolean default".into());
            }
            interpreted[slot] = true;
        }
        for conversion in &plan.word_conversions {
            let Some(param) = signature.params.get(conversion.slot) else { return Err("module command conversion slot is out of bounds".into()); };
            if !supplied[conversion.slot] || interpreted[conversion.slot]
                || command_word_scalar(&param.ty).as_ref() != Some(&conversion.scalar)
            {
                return Err("module command conversion does not match its supplied parameter".into());
            }
            interpreted[conversion.slot] = true;
        }
        Ok(())
    }

    pub(super) fn cli_descriptor(&self) -> Option<&CliDescriptorPlan> {
        match self {
            Self::SignatureCli(plan) => Some(plan),
            Self::CommandArguments(_) => None,
        }
    }

    /// All operands have run before word conversion can fail; a failed
    /// conversion must stop before the module performs any host effect.
    pub(super) fn convert_command_words(&self, values: &mut [Option<LoweredValue>]) -> Result<(), RuntimeError> {
        let Self::CommandArguments(plan) = self else { return Ok(()); };
        for conversion in &plan.word_conversions {
            let value = values.get_mut(conversion.slot).and_then(Option::as_mut)
                .ok_or_else(|| RuntimeError::new("lowered-invariant", "checked command operand is missing").with_span(conversion.span))?;
            let already_typed = matches!((&conversion.scalar, &*value),
                (ArgValueType::Str, LoweredValue::Str(_) | LoweredValue::StrView(_))
                | (ArgValueType::Int, LoweredValue::Int(_))
                | (ArgValueType::UInt, LoweredValue::Int(0..))
                | (ArgValueType::Bool, LoweredValue::Bool(_))
                | (ArgValueType::Path, LoweredValue::Path(_))
                | (ArgValueType::Duration, LoweredValue::Duration(_)));
            if already_typed { continue; }
            let raw = super::lowered_ops::lowered_str_value(value).ok_or_else(|| {
                RuntimeError::new("type-error", format!("command argument `{}` expects {:?}", plan.signature.params[conversion.slot].name, conversion.scalar)).with_span(conversion.span)
            })?;
            let parsed = parse_scalar_word(raw, &conversion.scalar).map_err(|error| {
                RuntimeError::new("type-error", format!("command argument `{}` expects {:?}, got `{raw}`: {}", plan.signature.params[conversion.slot].name, conversion.scalar, error.message)).with_span(conversion.span)
            })?;
            *value = match parsed {
                Value::Str(value) => LoweredValue::Str(value),
                Value::Int(value) => LoweredValue::Int(value),
                Value::Bool(value) => LoweredValue::Bool(value),
                Value::Path(value) => LoweredValue::Path(value),
                Value::Duration(value) => LoweredValue::Duration(value),
                _ => return Err(RuntimeError::new("lowered-invariant", "scalar word parser returned a non-scalar value").with_span(conversion.span)),
            };
        }
        Ok(())
    }
}
