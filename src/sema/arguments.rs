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
    let mut cursor = StaticArgumentCursor::new(params);
    let mut argument_slots = Vec::with_capacity(args.len());
    for arg in args {
        argument_slots.push(cursor.bind(arg)?);
    }
    let omitted_slots = cursor.omitted_slots();
    if let Some(&slot) = omitted_slots.iter().find(|&&slot| !params[slot].defaulted && !params[slot].rest) {
        return Err(ArgumentExpansionError { span: args.first().map(|arg| arg.span).unwrap_or_else(|| Span::at(crate::source::SourceId::new(0), 0)), message: format!("missing required parameter `{}`", params[slot].name) });
    }
    Ok(StaticArgumentBinding { argument_slots, omitted_slots })
}

/// Contextual argument checking advances the same slot occupancy rules as the
/// completed binding. It can supply a field context before a later entry is
/// checked without certifying missing arguments or constructing source nodes.
pub(crate) struct StaticArgumentCursor<'a> {
    params: &'a [CallableParamType],
    occupied: Vec<bool>,
    next: usize,
}

impl<'a> StaticArgumentCursor<'a> {
    pub(crate) fn new(params: &'a [CallableParamType]) -> Self {
        Self { params, occupied: vec![false; params.len()], next: 0 }
    }

    pub(crate) fn bind(&mut self, arg: &ExpandedArgument) -> Result<usize, ArgumentExpansionError> {
        let slot = if let Some(name) = arg.name {
            self.params.iter().position(|param| !param.rest && param.name == name)
                .ok_or_else(|| ArgumentExpansionError { span: arg.span, message: format!("unknown named parameter `{name}`") })?
        } else {
            while self.next < self.params.len() && self.occupied[self.next] && !self.params[self.next].rest { self.next += 1; }
            if self.next == self.params.len() {
                return Err(ArgumentExpansionError { span: arg.span, message: "too many positional arguments".into() });
            }
            self.next
        };
        if matches!(arg.value, ArgumentValueSource::PositionalSplice(_)) && !self.params[slot].rest {
            return Err(ArgumentExpansionError { span: arg.span, message: "positional splice must bind a rest parameter when combined with named spreading".into() });
        }
        if self.occupied[slot] && !self.params[slot].rest {
            return Err(ArgumentExpansionError { span: arg.span, message: format!("parameter `{}` supplied more than once", self.params[slot].name) });
        }
        self.occupied[slot] = true;
        if arg.name.is_none() && !self.params[slot].rest { self.next = slot + 1; }
        Ok(slot)
    }

    fn omitted_slots(&self) -> Vec<usize> {
        self.occupied.iter().enumerate().filter_map(|(slot, supplied)| (!supplied).then_some(slot)).collect()
    }
}
