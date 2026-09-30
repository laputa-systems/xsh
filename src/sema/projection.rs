use crate::sema::constants::{LiteralConstant, PreparedConstants};
use crate::sema::types::{ModuleExportType, Type};
use crate::symbol::Name;
use crate::syntax::arena::{AstArena, ExprId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionOperation { Get, Index }

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
    let key_expr = key;
    let LiteralConstant::Str(key) = constants.analyze_expression(arena, key)? else { return None; };
    projection_for_visible_field(receiver_expr, receiver, key_expr, Name::intern(&key), operation)
}

fn projection_for_visible_field(receiver_expr: ExprId, receiver: &Type, key_expr: ExprId, field: Name, operation: ProjectionOperation) -> Option<CheckedProjection> {
    let (value_type, callable) = match receiver {
        Type::Record(fields) => (fields.get(&field)?.clone(), None),
        Type::Module(exports) => {
            let export = exports.get(&field)?;
            (export.field_type(), match export { ModuleExportType::Value { .. } => None, _ => Some(export.clone()) })
        }
        _ => return None,
    };
    Some(CheckedProjection { field, receiver: receiver_expr, key: key_expr, value_type, operation, callable })
}

pub fn resolve_get_projection(
    arena: &AstArena,
    constants: &PreparedConstants,
    receiver_expr: ExprId,
    receiver: &Type,
    args: &[crate::syntax::arena::ArenaCallArg],
) -> Option<CheckedProjection> {
    use crate::syntax::arena::ArenaCallArgKind;
    let [arg] = args else { return None; };
    let key = match arg.kind {
        ArenaCallArgKind::Positional(key) => key,
        ArenaCallArgKind::Named { name, value, .. } if name == "field" => value,
        ArenaCallArgKind::NamedSpread { value, .. } => {
            let LiteralConstant::Record(fields) = constants.analyze_expression(arena, value)? else { return None; };
            if let Some(Type::Record(visible)) = constants.types.get(&value) {
                if visible.len() != 1 || !visible.contains_key(&Name::intern("field")) { return None; }
            } else if fields.len() != 1 { return None; }
            let LiteralConstant::Str(key) = fields.get(&Name::intern("field"))? else { return None; };
            return projection_for_visible_field(receiver_expr, receiver, value, Name::intern(key), ProjectionOperation::Get);
        }
        _ => return None,
    };
    resolve_constant_key_projection(arena, constants, receiver_expr, receiver, key, ProjectionOperation::Get)
}
