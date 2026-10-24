//! Static argument expansion shared by expression calls and structured stages.

use super::types::{CallableParamType, Type};
use crate::source::Span;
use crate::symbol::Name;
use crate::syntax::arena::{ArenaCallArg, ArenaCallArgKind, ArenaProgram, ExprId};
use rustc_hash::FxHashMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArgumentValueSource {
    Expression(ExprId),
    RecordField { record: ExprId, field: Name },
    PositionalSplice(ExprId),
    CheckedType,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpandedArgument<T = Type> {
    pub entry_index: usize,
    pub name: Option<Name>,
    pub value: ArgumentValueSource,
    pub ty: T,
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
    expand_named_arguments_inner(program, args, |value| Some(checked_type(value).unwrap_or(Type::Unknown)), |ty| match ty {
        Type::Record(fields) => Some(fields.iter().map(|(name, ty)| (*name, ty.clone())).collect()),
        _ => None,
    })
}

pub(crate) fn expand_scheme_named_arguments(
    program: &ArenaProgram,
    args: &[ArenaCallArg],
    checked_type: impl FnMut(ExprId) -> Option<super::schemes::SchemeType>,
) -> Result<Vec<ExpandedArgument<super::schemes::SchemeType>>, ArgumentExpansionError> {
    use super::schemes::SchemeType;
    expand_named_arguments_inner(program, args, checked_type, |ty| match ty {
        SchemeType::Record { fields, tail: super::schemes::SchemeRowTail::Closed } => Some(fields.into_iter().collect()),
        SchemeType::Ground(Type::Record(fields)) => Some(fields.iter().map(|(name, ty)| (*name, SchemeType::Ground(ty.clone()))).collect()),
        _ => None,
    })
}

fn expand_named_arguments_inner<T>(
    program: &ArenaProgram, args: &[ArenaCallArg],
    mut checked_type: impl FnMut(ExprId) -> Option<T>,
    record_fields: impl Fn(T) -> Option<Vec<(Name, T)>>,
) -> Result<Vec<ExpandedArgument<T>>, ArgumentExpansionError> {
    let mut expanded = Vec::new();
    for (entry_index, arg) in args.iter().enumerate() {
        let (expr, name, span, splice) = match arg.kind {
            ArenaCallArgKind::NamedSpread { value, span } => {
                let span = program.arena.span(span);
                let Some(fields) = checked_type(value).and_then(&record_fields) else {
                    return Err(ArgumentExpansionError {
                        span,
                        message: "named argument spread requires a checked finite Record".into(),
                    });
                };
                if fields.is_empty() {
                    return Err(ArgumentExpansionError {
                        span,
                        message: "open Record has no statically visible fields to spread".into(),
                    });
                }
                expanded.extend(
                    fields.into_iter().map(|(field, ty)| ExpandedArgument {
                            entry_index,
                            name: Some(field),
                            value: ArgumentValueSource::RecordField {
                                record: value,
                                field,
                            },
                            ty,
                            span,
                        }),
                );
                continue;
            }
            ArenaCallArgKind::Positional(value) => {
                (value, None, program.arena.expr(value).span, false)
            }
            ArenaCallArgKind::Named { name, value, span } => {
                (value, Some(name), program.arena.span(span), false)
            }
            ArenaCallArgKind::Splice { value, span } => {
                (value, None, program.arena.span(span), true)
            }
        };
        expanded.push(ExpandedArgument {
            entry_index,
            name,
            value: if splice {
                ArgumentValueSource::PositionalSplice(expr)
            } else {
                ArgumentValueSource::Expression(expr)
            },
            ty: checked_type(expr).ok_or_else(|| ArgumentExpansionError { span, message: "call argument has no checked type".into() })?,
            span,
        });
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StaticArgumentKind {
    Positional,
    Named(Name),
    Splice,
}

/// Err retains a positional outer value and an optional named diagnostic cause.
/// Binding uses ordinary slot occupancy so written argument order is independent
/// of the constructor's value/cause order.
pub(crate) fn bind_err_arguments(
    args: &[ExpandedArgument],
) -> Result<StaticArgumentBinding, ArgumentExpansionError> {
    let params = [
        CallableParamType {
            name: Name::intern("error"),
            ty: Type::Any,
            defaulted: false,
            rest: false,
        },
        CallableParamType {
            name: Name::intern("cause"),
            ty: Type::Error,
            defaulted: true,
            rest: false,
        },
    ];
    let binding = bind_static_arguments(&params, args)?;
    for (arg, slot) in args.iter().zip(&binding.argument_slots) {
        if (*slot == 1 && arg.name.is_none()) || (*slot == 0 && arg.name.is_some()) {
            return Err(ArgumentExpansionError {
                span: arg.span,
                message: "Err requires a positional error and an optional named `cause`".into(),
            });
        }
    }
    Ok(binding)
}

/// `hash.verify_file` names its checksum argument by algorithm, so that
/// parameter has no fixed published name. The path binds by position or
/// `path:`, the checksum is the other argument, and written order is
/// irrelevant. Returns the `[path, checksum]` argument indices.
pub(crate) fn bind_hash_verify_file_arguments(args: &[ArenaCallArg]) -> Option<[usize; 2]> {
    let [first, second] = args else { return None };
    let binds_path = |arg: &ArenaCallArg| match arg.kind {
        ArenaCallArgKind::Positional(_) => true,
        ArenaCallArgKind::Named { name, .. } => name == "path",
        ArenaCallArgKind::Splice { .. } | ArenaCallArgKind::NamedSpread { .. } => false,
    };
    match (binds_path(first), binds_path(second)) {
        (true, false) => Some([0, 1]),
        (false, true) => Some([1, 0]),
        _ => None,
    }
}

/// Resolve names and positional occupancy before lowering. A record field is
/// supplied even when its value is null; omission is solely a missing slot.
pub fn bind_static_arguments(
    params: &[CallableParamType],
    args: &[ExpandedArgument],
) -> Result<StaticArgumentBinding, ArgumentExpansionError> {
    if let Some(argument) = args.iter().find(|argument| argument.name.is_some()
        && matches!(argument.value, ArgumentValueSource::PositionalSplice(_)))
    {
        return Err(ArgumentExpansionError { span: argument.span, message: "positional splice cannot name a parameter".into() });
    }
    bind_argument_layout(params, args.iter().map(|argument| (
        match argument.value {
            ArgumentValueSource::PositionalSplice(_) => StaticArgumentKind::Splice,
            _ => argument.name.map_or(StaticArgumentKind::Positional, StaticArgumentKind::Named),
        }, argument.span,
    )))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ArgumentParameterLayout {
    pub(crate) name: Name,
    pub(crate) defaulted: bool,
    pub(crate) rest: bool,
}

pub(crate) fn bind_argument_layout(
    params: &[CallableParamType],
    args: impl ExactSizeIterator<Item = (StaticArgumentKind, Span)>,
) -> Result<StaticArgumentBinding, ArgumentExpansionError> {
    let parameters = params.iter().map(|parameter| ArgumentParameterLayout {
        name: parameter.name, defaulted: parameter.defaulted, rest: parameter.rest,
    }).collect::<Vec<_>>();
    bind_parameter_layout(&parameters, args)
}

pub(crate) fn bind_parameter_layout(
    params: &[ArgumentParameterLayout],
    args: impl ExactSizeIterator<Item = (StaticArgumentKind, Span)>,
) -> Result<StaticArgumentBinding, ArgumentExpansionError> {
    let mut named_slots = FxHashMap::default();
    for (slot, param) in params.iter().enumerate().filter(|(_, param)| !param.rest) {
        named_slots.entry(param.name).or_insert(slot);
    }
    let mut occupied = vec![false; params.len()];
    let mut argument_slots = Vec::with_capacity(args.len());
    let mut next = 0;
    let mut first_span = None;
    for (kind, span) in args {
        first_span.get_or_insert(span);
        let slot = if let StaticArgumentKind::Named(name) = kind {
            named_slots.get(&name).copied()
                .ok_or_else(|| ArgumentExpansionError {
                    span,
                    message: format!("unknown named parameter `{name}`"),
                })?
        } else {
            while next < params.len() && occupied[next] && !params[next].rest {
                next += 1;
            }
            if next == params.len() {
                return Err(ArgumentExpansionError {
                    span,
                    message: "too many positional arguments".into(),
                });
            }
            next
        };
        if kind == StaticArgumentKind::Splice && !params[slot].rest {
            return Err(ArgumentExpansionError { span, message: "positional splice must bind a rest parameter when combined with named spreading".into() });
        }
        if occupied[slot] && !params[slot].rest {
            return Err(ArgumentExpansionError {
                span,
                message: format!("parameter `{}` supplied more than once", params[slot].name),
            });
        }
        occupied[slot] = true;
        argument_slots.push(slot);
        if !matches!(kind, StaticArgumentKind::Named(_)) && !params[slot].rest {
            next = slot + 1;
        }
    }
    let omitted_slots = occupied
        .iter()
        .enumerate()
        .filter_map(|(slot, supplied)| (!supplied).then_some(slot))
        .collect::<Vec<_>>();
    if let Some(&slot) = omitted_slots
        .iter()
        .find(|&&slot| !params[slot].defaulted && !params[slot].rest)
    {
        return Err(ArgumentExpansionError {
            span: first_span
                .unwrap_or_else(|| Span::at(crate::source::SourceId::new(0), 0)),
            message: format!("missing required parameter `{}`", params[slot].name),
        });
    }
    Ok(StaticArgumentBinding {
        argument_slots,
        omitted_slots,
    })
}
