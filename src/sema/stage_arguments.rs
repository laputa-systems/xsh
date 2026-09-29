use crate::sema::types::{CallableParamType, Type};
use crate::symbol::Name;
use crate::syntax::arena::{ArenaCallArgKind, ArenaProgram, ArenaStreamStage, ExprId};
use crate::source::Span;
use xsh_registry::stream_parameters::{
    StageParameterDefault, StageParameterType, stage_parameters,
};

pub(crate) fn stage_argument_params(stage: &str) -> Vec<CallableParamType> {
    stage_parameters(stage)
        .iter()
        .map(|parameter| CallableParamType {
            name: Name::intern(parameter.name),
            ty: match parameter.ty {
                StageParameterType::Int => Type::Int,
                StageParameterType::Bool => Type::Bool,
                StageParameterType::Value | StageParameterType::Sequence => Type::Unknown,
                StageParameterType::Columns => Type::List(Box::new(Type::Str)),
            },
            defaulted: parameter.default != StageParameterDefault::Required,
            rest: false,
        })
        .collect()
}

/// A qualified stage descriptor retains the loader's static import identity.
/// A value copied from a module contract is not an imported namespace.
pub(crate) fn stage_namespace_is_imported(program: &ArenaProgram, namespace: Name, owner: Option<Name>) -> bool {
    stage_namespace_owner(program, namespace, owner).is_some()
}

pub(crate) fn stage_namespace_owner(program: &ArenaProgram, namespace: Name, owner: Option<Name>) -> Option<Name> {
    let statements = if let Some(owner) = owner {
        let module = program.modules.iter().find(|module| module.name == owner)?;
        program.module_statements(module).collect::<Vec<_>>()
    } else { program.statement_ids().collect::<Vec<_>>() };
    statements.into_iter().find_map(|statement| {
        let crate::syntax::arena::ArenaStmtKind::Use(id) = program.arena.stmt(statement).kind else { return None; };
        let import = program.arena.use_stmt(id);
        if import.alias.or_else(|| program.arena.names(import.path).last()) != Some(namespace) { return None; }
        let key = import.resolved.as_deref()?;
        program.modules.iter().find(|module| module.key.as_str() == key).map(|module| module.name)
    })
}

/// Bind the optional `block` descriptor using the ordinary argument protocol.
/// Configuration entries retain their original syntax and source evaluation order.
pub(crate) fn stage_callable_argument(
    program: &ArenaProgram, stage: &ArenaStreamStage,
    mut checked_type: impl FnMut(ExprId) -> Option<Type>,
) -> Result<Option<(ExprId, Vec<crate::syntax::arena::ArenaCallArgInput>)>, (Span, String)> {
    use crate::sema::arguments::{ArgumentValueSource, expand_named_arguments, bind_static_arguments};
    use crate::syntax::arena::ArenaCallArgInput;
    if !xsh_registry::stream_parameters::stage_accepts_callable(stage.kind.as_str()) { return Ok(None); }
    let args = program.arena.call_args(stage.args);
    let has_descriptor = args.iter().any(|arg| matches!(arg.kind, ArenaCallArgKind::Positional(_))
        || matches!(arg.kind, ArenaCallArgKind::Named { name, .. } if name == "block"));
    if !has_descriptor { return Ok(None); }
    let mut params = vec![CallableParamType { name: Name::intern("block"), ty: Type::Unknown, defaulted: true, rest: false }];
    params.extend(stage_argument_params(stage.kind.as_str()));
    let expanded = expand_named_arguments(program, args, |expr| checked_type(expr).or(Some(Type::Unknown)))
        .map_err(|error| (error.span, error.message))?;
    let binding = bind_static_arguments(&params, &expanded).map_err(|error| (error.span, error.message))?;
    let Some(index) = binding.argument_slots.iter().position(|slot| *slot == 0) else { return Ok(None); };
    let argument = &expanded[index];
    let ArgumentValueSource::Expression(callee) = argument.value else {
        return Err((argument.span, "stage callable must retain a direct named function identity".into()));
    };
    if stage.block.is_some() { return Err((argument.span, "stage accepts either a block or a named callable".into())); }
    let entry = argument.entry_index;
    let remaining = args.iter().enumerate().filter(|(index, _)| *index != entry).map(|(_, arg)| match arg.kind {
        ArenaCallArgKind::Positional(value) => ArenaCallArgInput::Positional(value),
        ArenaCallArgKind::Named { name, value, .. } => ArenaCallArgInput::Named { name, value, span: program.arena.expr(value).span },
        ArenaCallArgKind::NamedSpread { value, span } => ArenaCallArgInput::NamedSpread { value, span: program.arena.span(span) },
        ArenaCallArgKind::Splice { value, span } => ArenaCallArgInput::Splice { value, span: program.arena.span(span) },
    }).collect();
    Ok(Some((callee, remaining)))
}
