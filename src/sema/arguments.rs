//! Static argument expansion shared by expression calls and structured stages.

use crate::source::Span;
use crate::symbol::Name;
use crate::syntax::arena::{ArenaCallArg, ArenaCallArgKind, ArenaProgram, ExprId};
use super::types::{CallableParamType, Type};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArgumentValueSource {
    Expression(ExprId),
    RecordField { record: ExprId, field: Name },
    PositionalSplice(ExprId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpandedArgument {
    pub entry_index: usize,
    pub name: Option<Name>,
    pub value: ArgumentValueSource,
    pub ty: Type,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArgumentExpansionError {
    pub span: Span,
    pub message: String,
}

/// Only a checked finite record exposes named fields. Empty `Record` types
/// describe an open runtime record and cannot supply a static call signature.
pub fn expand_named_arguments(
    program: &ArenaProgram,
    args: &[ArenaCallArg],
    mut checked_type: impl FnMut(ExprId) -> Option<Type>,
) -> Result<Vec<ExpandedArgument>, ArgumentExpansionError> {
    let mut expanded = Vec::new();
    for (entry_index, arg) in args.iter().enumerate() {
        let (expr, name, span, splice) = match arg.kind {
            ArenaCallArgKind::NamedSpread { value, span } => {
                let span = program.arena.span(span);
                let Some(Type::Record(fields)) = checked_type(value) else {
                    return Err(ArgumentExpansionError { span, message: "named argument spread requires a checked finite Record".into() });
                };
                if fields.is_empty() {
                    return Err(ArgumentExpansionError { span, message: "open Record has no statically visible fields to spread".into() });
                }
                expanded.extend(fields.into_iter().map(|(field, ty)| ExpandedArgument {
                    entry_index, name: Some(field), value: ArgumentValueSource::RecordField { record: value, field }, ty, span,
                }));
                continue;
            }
            ArenaCallArgKind::Positional(value) => (value, None, program.arena.expr(value).span, false),
            ArenaCallArgKind::Named { name, value, span } => (value, Some(name), program.arena.span(span), false),
            ArenaCallArgKind::Splice { value, span } => (value, None, program.arena.span(span), true),
        };
        expanded.push(ExpandedArgument { entry_index, name,
            value: if splice { ArgumentValueSource::PositionalSplice(expr) } else { ArgumentValueSource::Expression(expr) },
            ty: checked_type(expr).unwrap_or(Type::Unknown), span });
    }
    Ok(expanded)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StaticArgumentBinding {
    /// Each expanded value's destination parameter; rest entries share a slot.
    pub argument_slots: Vec<usize>,
    /// Parameters absent from the source, retained for their ordinary defaults.
    pub omitted_slots: Vec<usize>,
}

/// Err retains a positional outer value and an optional named diagnostic cause.
/// Binding uses ordinary slot occupancy so written argument order is independent
/// of the constructor's value/cause order.
pub(crate) fn bind_err_arguments(args: &[ExpandedArgument]) -> Result<StaticArgumentBinding, ArgumentExpansionError> {
    let params = [
        CallableParamType { name: Name::intern("error"), ty: Type::Any, defaulted: false, rest: false },
        CallableParamType { name: Name::intern("cause"), ty: Type::Error, defaulted: true, rest: false },
    ];
    let binding = bind_static_arguments(&params, args)?;
    for (arg, slot) in args.iter().zip(&binding.argument_slots) {
        if (*slot == 1 && arg.name.is_none()) || (*slot == 0 && arg.name.is_some()) {
            return Err(ArgumentExpansionError { span: arg.span,
                message: "Err requires a positional error and an optional named `cause`".into() });
        }
    }
    Ok(binding)
}

/// Resolve names and positional occupancy before lowering. A record field is
/// supplied even when its value is null; omission is solely a missing slot.
pub fn bind_static_arguments(
    params: &[CallableParamType], args: &[ExpandedArgument],
) -> Result<StaticArgumentBinding, ArgumentExpansionError> {
    let mut occupied = vec![false; params.len()];
    let mut argument_slots = Vec::with_capacity(args.len());
    let mut next = 0;
    for arg in args {
        let slot = if let Some(name) = arg.name {
            params.iter().position(|param| !param.rest && param.name == name)
                .ok_or_else(|| ArgumentExpansionError { span: arg.span, message: format!("unknown named parameter `{name}`") })?
        } else {
            while next < params.len() && occupied[next] && !params[next].rest { next += 1; }
            if next == params.len() {
                return Err(ArgumentExpansionError { span: arg.span, message: "too many positional arguments".into() });
            }
            next
        };
        if matches!(arg.value, ArgumentValueSource::PositionalSplice(_)) && !params[slot].rest {
            return Err(ArgumentExpansionError { span: arg.span, message: "positional splice must bind a rest parameter when combined with named spreading".into() });
        }
        if occupied[slot] && !params[slot].rest {
            return Err(ArgumentExpansionError { span: arg.span, message: format!("parameter `{}` supplied more than once", params[slot].name) });
        }
        occupied[slot] = true;
        argument_slots.push(slot);
        if arg.name.is_none() && !params[slot].rest { next = slot + 1; }
    }
    let omitted_slots = occupied.iter().enumerate().filter_map(|(slot, supplied)| (!supplied).then_some(slot)).collect::<Vec<_>>();
    if let Some(&slot) = omitted_slots.iter().find(|&&slot| !params[slot].defaulted && !params[slot].rest) {
        return Err(ArgumentExpansionError { span: args.first().map(|arg| arg.span).unwrap_or_else(|| Span::at(crate::source::SourceId::new(0), 0)), message: format!("missing required parameter `{}`", params[slot].name) });
    }
    Ok(StaticArgumentBinding { argument_slots, omitted_slots })
}
