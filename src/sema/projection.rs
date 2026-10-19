use crate::sema::constants::{LiteralConstant, PreparedConstants};
use crate::sema::types::{ModuleExportType, Type};
use crate::symbol::Name;
use crate::syntax::arena::{AstArena, ExprId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionOperation {
    Get,
    Index,
}

/// A proven key selects only a field visible in the checked receiver contract.
/// Runtime access remains fallible and still evaluates the original key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedProjection {
    pub field: Name,
    pub receiver: ExprId,
    /// Full checking may normalize argument spreads into synthetic selectors;
    /// compact checking retains the original argument expression.
    pub key: ExprId,
    pub value_type: Type,
    pub operation: ProjectionOperation,
    pub callable: Option<ModuleExportType>,
}

pub fn resolve_constant_key_projection(
    arena: &AstArena,
    constants: &PreparedConstants,
    receiver_expr: ExprId,
    receiver: &Type,
    key: ExprId,
    operation: ProjectionOperation,
) -> Option<CheckedProjection> {
    let field = constant_string_key(arena, constants, key)?;
    projection_for_visible_field(
        receiver_expr,
        receiver,
        key,
        field,
        operation,
    )
}

fn constant_string_key(
    arena: &AstArena,
    constants: &PreparedConstants,
    key: ExprId,
) -> Option<Name> {
    let LiteralConstant::Str(key) = constants.analyze_expression(arena, key)? else {
        return None;
    };
    Some(Name::intern(&key))
}

pub fn generic_callable_export_for_key(
    arena: &AstArena,
    constants: &PreparedConstants,
    receiver: &Type,
    key: ExprId,
) -> Option<Name> {
    let Some(field) = constant_string_key(arena, constants, key) else {
        return generic_callable_export_for_dynamic_key(receiver);
    };
    match receiver {
        Type::Module(exports)
            if matches!(exports.get(&field), Some(ModuleExportType::GenericCallable { .. })) =>
        {
            Some(field)
        }
        _ => None,
    }
}

pub fn generic_callable_export_for_dynamic_key(receiver: &Type) -> Option<Name> {
    module_has_generic_callable_export(receiver)
}

fn module_has_generic_callable_export(receiver: &Type) -> Option<Name> {
    let Type::Module(exports) = receiver else {
        return None;
    };
    exports
        .exports
        .iter()
        .find_map(|(name, export)| matches!(export, ModuleExportType::GenericCallable { .. }).then_some(*name))
}

fn projection_for_visible_field(
    receiver_expr: ExprId,
    receiver: &Type,
    key_expr: ExprId,
    field: Name,
    operation: ProjectionOperation,
) -> Option<CheckedProjection> {
    let (value_type, callable) = match receiver {
        Type::Record(fields) => (fields.get(&field)?.clone(), None),
        Type::Module(exports) => {
            let export = exports.get(&field)?;
            let value_type = export.field_value_type()?;
            let callable = match export {
                ModuleExportType::Value { .. } => None,
                ModuleExportType::GenericCallable { .. } => return None,
                _ => Some(export.clone()),
            };
            (value_type, callable)
        }
        _ => return None,
    };
    Some(CheckedProjection {
        field,
        receiver: receiver_expr,
        key: key_expr,
        value_type,
        operation,
        callable,
    })
}

pub fn resolve_get_projection(
    arena: &AstArena,
    constants: &PreparedConstants,
    receiver_expr: ExprId,
    receiver: &Type,
    args: &[crate::syntax::arena::ArenaCallArg],
) -> Option<CheckedProjection> {
    let (key, field) = get_key(arena, constants, args)?;
    projection_for_visible_field(
        receiver_expr,
        receiver,
        key,
        field,
        ProjectionOperation::Get,
    )
}

pub fn generic_callable_export_for_get(
    arena: &AstArena,
    constants: &PreparedConstants,
    receiver: &Type,
    args: &[crate::syntax::arena::ArenaCallArg],
) -> Option<Name> {
    match get_key(arena, constants, args) {
        Some((_, field)) => match receiver {
            Type::Module(exports)
                if matches!(exports.get(&field), Some(ModuleExportType::GenericCallable { .. })) =>
            {
                Some(field)
            }
            Type::Module(_) => None,
            _ => None,
        },
        None => module_has_generic_callable_export(receiver),
    }
}

fn get_key(
    arena: &AstArena,
    constants: &PreparedConstants,
    args: &[crate::syntax::arena::ArenaCallArg],
) -> Option<(ExprId, Name)> {
    use crate::syntax::arena::ArenaCallArgKind;
    let [arg] = args else {
        return None;
    };
    let key = match arg.kind {
        ArenaCallArgKind::Positional(key) => key,
        ArenaCallArgKind::Named { name, value, .. } if name == "field" => value,
        ArenaCallArgKind::NamedSpread { value, .. } => {
            let LiteralConstant::Record(fields) = constants.analyze_expression(arena, value)?
            else {
                return None;
            };
            if let Some(Type::Record(visible)) = constants.types.get(&value) {
                if visible.len() != 1 || !visible.contains_key(&Name::intern("field")) {
                    return None;
                }
            } else if fields.len() != 1 {
                return None;
            }
            let LiteralConstant::Str(key) = fields.get(&Name::intern("field"))? else {
                return None;
            };
            return Some((value, Name::intern(key)));
        }
        _ => return None,
    };
    Some((key, constant_string_key(arena, constants, key)?))
}
