use super::{
    CallableType, Checker, FunctionParamSig, FunctionSig, ModuleExportType, Name, QualifiedName,
    Span, Type,
};
use crate::syntax::arena::{ArenaExprKind, ArenaProgram, ExprId};

/// Checked alias calls retain their signature while executing the original
/// callable value, including its captured environment and prepared defaults.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StaticCallableAlias {
    pub name: Name,
    pub method_call: bool,
    pub signature: CallableType,
    pub pure: bool,
    pub definition: Option<Span>,
}

#[derive(Clone, Debug)]
pub(super) struct CallableAlias {
    pub name: Name,
    pub signature: FunctionSig,
    pub pure: bool,
}

impl CallableAlias {
    fn checked(&self) -> Option<StaticCallableAlias> {
        Some(StaticCallableAlias {
            name: self.name,
            method_call: false,
            signature: super::decl::callable_type_from_function_signature(&self.signature)?,
            pure: self.pure,
            definition: self.signature.definition,
        })
    }
}

impl Checker {
    pub(super) fn resolve_callable_alias_target(
        &self,
        program: &ArenaProgram,
        expression: ExprId,
    ) -> Option<CallableAlias> {
        let span = program.arena.expr(expression).span;
        if matches!(self.expr_types.get(&span), Some(Type::Pure | Type::Proc)) {
            let projected = match program.arena.expr(expression).kind {
                ArenaExprKind::Try(inner) => self.projections.get(&program.arena.expr(inner).span),
                _ => self.projections.get(&span),
            };
            if let Some(projection) = projected
                && let Some(target) = self.resolve_module_callable_alias(
                    program,
                    projection.receiver,
                    projection.field,
                    projection.callable.as_ref()?,
                )
            {
                return Some(target);
            }
            if let ArenaExprKind::Field { base, name } = program.arena.expr(expression).kind
                && let Some(Type::Module(exports)) =
                    self.expr_types.get(&program.arena.expr(base).span)
                && let Some(export) = exports.get(&name)
                && let Some(target) =
                    self.resolve_module_callable_alias(program, base, name, export)
            {
                return Some(target);
            }
        }
        let (name, signature, pure) = match program.arena.expr(expression).kind {
            ArenaExprKind::Ident(name) => {
                match self.lookup_value_binding_for_expr(program, expression) {
                    super::ValueBindingLookup::Found(binding) => {
                        return binding.callable_alias.clone();
                    }
                    super::ValueBindingLookup::Missing => return None,
                    super::ValueBindingLookup::NoSite => {}
                }
                if let Some(signature) = self.pures.get(&name) {
                    (name, self.local_function_signature(*signature)?, true)
                } else {
                    let signature = self.local_function_signature(*self.procs.get(&name)?)?;
                    (name, signature, false)
                }
            }
            ArenaExprKind::Field { base, name } => {
                let ArenaExprKind::Ident(namespace) = program.arena.expr(base).kind else {
                    return None;
                };
                match self.lookup_value_binding_for_expr(program, base) {
                    super::ValueBindingLookup::Found(binding) if binding.static_namespace => {}
                    super::ValueBindingLookup::NoSite => {}
                    super::ValueBindingLookup::Found(_) | super::ValueBindingLookup::Missing => {
                        return None;
                    }
                }
                let qualified = QualifiedName::new(namespace, name);
                if let Some(signature) = self.qualified_pures.get(&qualified).cloned() {
                    (name, signature, true)
                } else {
                    (name, self.qualified_procs.get(&qualified)?.clone(), false)
                }
            }
            _ => return None,
        };
        Some(CallableAlias {
            name,
            signature,
            pure,
        })
    }

    fn resolve_module_callable_alias(
        &self,
        program: &ArenaProgram,
        receiver: ExprId,
        name: Name,
        export: &ModuleExportType,
    ) -> Option<CallableAlias> {
        let static_namespace = match program.arena.expr(receiver).kind {
            ArenaExprKind::Ident(_) => match self.lookup_value_binding_for_expr(program, receiver) {
                super::ValueBindingLookup::Found(binding) => binding.static_namespace,
                super::ValueBindingLookup::Missing => return None,
                super::ValueBindingLookup::NoSite => true,
            },
            _ => false,
        };
        if let ArenaExprKind::Ident(namespace) = program.arena.expr(receiver).kind
            && static_namespace
        {
            let qualified = QualifiedName::new(namespace, name);
            if let Some(signature) = self.qualified_pures.get(&qualified) {
                return Some(CallableAlias {
                    name,
                    signature: signature.clone(),
                    pure: true,
                });
            }
            if let Some(signature) = self.qualified_procs.get(&qualified) {
                return Some(CallableAlias {
                    name,
                    signature: signature.clone(),
                    pure: false,
                });
            }
        }
        let (sig, pure) = match export {
            ModuleExportType::Pure {
                sig,
                optional: false,
            } => (sig, true),
            ModuleExportType::Proc {
                sig,
                optional: false,
            } => (sig, false),
            _ => return None,
        };
        if self.collecting_effects && !pure {
            self.provisional_effects_read.set(true);
        }
        Some(CallableAlias {
            name,
            pure,
            signature: FunctionSig::ground(
                if pure {
                    super::super::schemes::SchemeCallableKind::Pure
                } else {
                    super::super::schemes::SchemeCallableKind::Proc
                },
                None,
                false,
                None,
                None,
                sig
                    .params
                    .iter()
                    .map(|param| FunctionParamSig {
                        schema_expectation: None,
                        name: param.name,
                        ty: param.ty.clone(),
                        defaulted: param.defaulted,
                        rest: param.rest,
                    })
                    .collect(),
                (*sig.return_ty).clone(),
                None,
                sig.effects.clone(),
                false,
                true,
            )?,
        })
    }

    pub(super) fn resolve_callable_alias_call(
        &self,
        program: &ArenaProgram,
        expression: ExprId,
    ) -> Option<CallableAlias> {
        if let ArenaExprKind::Field { base, name } = program.arena.expr(expression).kind
            && name == "call"
            && let Some(alias) = self.resolve_callable_alias_call(program, base)
        {
            return Some(alias);
        }
        let target = self.resolve_callable_alias_target(program, expression)?;
        target.signature.is_alias.then_some(target)
    }

    pub(super) fn record_callable_alias(&mut self, span: Span, alias: &CallableAlias) {
        if let Some(alias) = alias.checked() {
            self.static_callable_aliases.insert(span, alias);
        }
    }

    pub(super) fn attach_callable_alias(
        &mut self,
        name: Name,
        mut alias: CallableAlias,
        initializer: Span,
    ) {
        alias.name = name;
        alias.signature.is_alias = true;
        self.record_callable_alias(initializer, &alias);
        if let Some(binding) = self
            .scopes
            .last_mut()
            .and_then(|scope| scope.get_mut(&name))
        {
            binding.callable_alias = Some(alias);
        }
    }
}
