use super::{Checker, Type};
use crate::syntax::arena::{ArenaFunctionDef, ArenaProgram, ArenaStmtKind, StmtId};

impl Checker {
    // Defaults are checked in their declaration environment before callers are
    // checked. Repeated passes resolve only dependencies with established
    // signatures; neither caller arguments nor the callable body supply a type.
    pub(super) fn infer_default_parameter_types(&mut self, program: &ArenaProgram, source: &str, statements: &[StmtId]) {
        let count = statements.len();
        for _ in 0..=count {
            let before = self.parameter_types.len();
            let mut probe = self.constraint_probe();
            for &id in statements {
                let kind = match program.arena.stmt(id).kind {
                    ArenaStmtKind::Export(inner) => program.arena.stmt(inner).kind,
                    kind => kind,
                };
                match kind {
                    ArenaStmtKind::ProcDef(id) | ArenaStmtKind::PureDef(id) | ArenaStmtKind::StreamDef(id) => {
                        let def = program.arena.function_def(id);
                        probe.in_pure = matches!(kind, ArenaStmtKind::PureDef(_));
                        probe.current_return = Some(probe.type_from_arena(program, def.return_ty));
                        probe.current_effects = def.effects.map(|effects| program.arena.effects(effects).collect());
                        for param in program.arena.params(def.params) {
                            let span = program.arena.span(param.span);
                            if !param.ty_defaulted {
                                let ty = probe.type_from_arena(program, param.ty);
                                self.parameter_types.insert(span, ty.clone());
                                probe.parameter_types.insert(span, ty);
                            } else if !self.parameter_types.contains_key(&span) {
                                if let Some(default) = param.default {
                                    let actual = probe.check_expr_arena(program, source, default, None);
                                    let actual = probe.type_constraints.resolve(&actual).unwrap_or(Type::Invalid);
                                    if parameter_type_is_concrete(&actual) {
                                        self.parameter_types.insert(span, actual.clone());
                                        probe.parameter_types.insert(span, actual);
                                    }
                                }
                            }
                        }
                        self.publish_parameter_types(program, def);
                        probe.publish_parameter_types(program, def);
                    }
                    ArenaStmtKind::Let { .. } | ArenaStmtKind::Var { .. } | ArenaStmtKind::Const { .. } | ArenaStmtKind::Use(_) => {
                        probe.current_return = None;
                        probe.in_pure = false;
                        probe.check_stmt_arena(program, source, id);
                    }
                    _ => {}
                }
            }
            if before == self.parameter_types.len() { break; }
        }
    }

    pub(super) fn checked_parameter_type(&self, program: &ArenaProgram, param: &crate::syntax::arena::ArenaParam) -> Option<Type> {
        self.parameter_types.get(&program.arena.span(param.span)).cloned()
    }

    pub(super) fn publish_parameter_types(&mut self, program: &ArenaProgram, def: &ArenaFunctionDef) {
        for signatures in [&mut self.pures, &mut self.procs, &mut self.streams] {
            if let Some(sig) = signatures.get_mut(&def.name) {
                for (param, slot) in program.arena.params(def.params).iter().zip(&mut sig.params) {
                    if let Some(ty) = self.parameter_types.get(&program.arena.span(param.span)) { slot.ty = ty.clone(); }
                }
            }
        }
    }

    pub(super) fn infer_checked_parameter(&mut self, program: &ArenaProgram, source: &str, param: &crate::syntax::arena::ArenaParam) -> Type {
        let span = program.arena.span(param.span);
        if !param.ty_defaulted {
            let ty = self.type_from_arena(program, param.ty);
            self.parameter_types.insert(span, ty.clone());
            return ty;
        }
        if let Some(ty) = self.checked_parameter_type(program, param) { return ty; }
        if let Some(default) = param.default {
            let actual = self.check_expr_arena(program, source, default, None);
            let actual = self.type_constraints.resolve(&actual).unwrap_or(Type::Invalid);
            if parameter_type_is_concrete(&actual) {
                self.parameter_types.insert(span, actual.clone());
                return actual;
            }
        }
        if self.inferred_returns.is_none() {
            self.error(span, "default does not establish a concrete parameter type; declare an annotation (including in cyclic declaration dependencies)", "check.infer-param");
        }
        Type::Invalid
    }
}

pub(super) fn parameter_type_is_concrete(ty: &Type) -> bool {
    match ty {
        Type::BuiltinParameter(_) | Type::Inference(_) | Type::Any | Type::Unknown | Type::Invalid | Type::Null | Type::DynamicModule => false,
        Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) => parameter_type_is_concrete(inner),
        Type::Map(key, value) => parameter_type_is_concrete(key) && parameter_type_is_concrete(value),
        Type::Result(ok, err) => parameter_type_is_concrete(ok) && parameter_type_is_concrete(err),
        Type::Module(exports) => !exports.is_empty() && exports.values().all(|export| match export {
            crate::sema::types::ModuleExportType::Value { ty, .. } => parameter_type_is_concrete(ty),
            crate::sema::types::ModuleExportType::Pure { sig, .. } | crate::sema::types::ModuleExportType::Proc { sig, .. } =>
                parameter_type_is_concrete(&sig.return_ty) && sig.params.iter().all(|param| parameter_type_is_concrete(&param.ty)),
        }),
        Type::Record(fields) => !fields.is_empty() && fields.values().all(parameter_type_is_concrete),
        _ => true,
    }
}
