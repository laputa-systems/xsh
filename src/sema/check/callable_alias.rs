use super::{CallableType, Checker, FunctionSig, Name, QualifiedName, Span};
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
    fn checked(&self) -> StaticCallableAlias {
        StaticCallableAlias {
            name: self.name,
            method_call: false,
            signature: super::decl::callable_type_from_function_signature(&self.signature),
            pure: self.pure,
            definition: self.signature.definition,
        }
    }
}

impl Checker {
    pub(super) fn resolve_callable_alias_target(&self, program: &ArenaProgram, expression: ExprId) -> Option<CallableAlias> {
        let (name, signature, pure) = match program.arena.expr(expression).kind {
            ArenaExprKind::Ident(name) => {
                if let Some(binding) = self.lookup(name) { return binding.callable_alias.clone(); }
                if let Some(signature) = self.pures.get(&name) { (name, signature, true) }
                else { (name, self.procs.get(&name)?, false) }
            }
            ArenaExprKind::Field { base, name } => {
                let ArenaExprKind::Ident(namespace) = program.arena.expr(base).kind else { return None; };
                if self.lookup(namespace).is_some_and(|binding| !binding.static_namespace) { return None; }
                let qualified = QualifiedName::new(namespace, name);
                if let Some(signature) = self.qualified_pures.get(&qualified) { (name, signature, true) }
                else { (name, self.qualified_procs.get(&qualified)?, false) }
            }
            _ => return None,
        };
        Some(CallableAlias { name, signature: signature.clone(), pure })
    }

    pub(super) fn resolve_callable_alias_call(&self, program: &ArenaProgram, expression: ExprId) -> Option<CallableAlias> {
        if let ArenaExprKind::Field { base, name } = program.arena.expr(expression).kind
            && name == "call"
            && let Some(alias) = self.resolve_callable_alias_call(program, base) {
            return Some(alias);
        }
        let target = self.resolve_callable_alias_target(program, expression)?;
        let aliased = match program.arena.expr(expression).kind {
            ArenaExprKind::Ident(name) => self.lookup(name).is_some_and(|binding| binding.callable_alias.is_some()),
            _ => target.signature.is_alias,
        };
        aliased.then_some(target)
    }

    pub(super) fn record_callable_alias(&mut self, span: Span, alias: &CallableAlias) {
        self.static_callable_aliases.insert(span, alias.checked());
    }

    pub(super) fn attach_callable_alias(&mut self, name: Name, mut alias: CallableAlias, initializer: Span) {
        alias.name = name;
        alias.signature.is_alias = true;
        self.record_callable_alias(initializer, &alias);
        if let Some(binding) = self.scopes.last_mut().and_then(|scope| scope.get_mut(&name)) {
            binding.callable_alias = Some(alias);
        }
    }
}
