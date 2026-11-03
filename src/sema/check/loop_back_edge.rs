use super::{Binding, Checker, Name, Type, ValueBindingLookup};
use crate::sema::types::{CallableKind, ModuleExportType};
use crate::syntax::arena::{
    ArenaAssignTargetKind, ArenaChild, ArenaExprKind, ArenaProgram, ArenaStmtKind, AssignTargetId,
    ExprId, SugarView,
};

const MAX_PROJECTION_DEPTH: usize = 128;

#[derive(Default)]
struct LoopWrites {
    targets: Vec<(Name, Vec<Name>)>,
    runs_effectful_callable: bool,
}

pub(super) fn assign_target_path(
    arena: &ArenaProgram,
    mut target: AssignTargetId,
) -> Option<(Name, Vec<Name>)> {
    let mut path = Vec::new();
    let mut whole = false;
    loop {
        match arena.arena.assign_target(target).kind {
            ArenaAssignTargetKind::Field { base, name } if path.len() < MAX_PROJECTION_DEPTH => {
                path.push(name);
                target = base;
            }
            ArenaAssignTargetKind::Field { base, .. } | ArenaAssignTargetKind::Index { base, .. } => {
                whole = true;
                target = base;
            }
            ArenaAssignTargetKind::Name(name) => {
                if whole {
                    path.clear();
                } else {
                    path.reverse();
                }
                return Some((name, path));
            }
            ArenaAssignTargetKind::Env(_) => return None,
        }
    }
}

impl Binding {
    pub(super) fn forget_narrowing(&mut self, path: &[Name]) {
        self.proof.mutate(path);
        if let Some(original) = &self.unrefined_ty {
            super::proof::restore_projection(&mut self.ty, original, path);
        }
        if path.is_empty() {
            self.unrefined_ty = None;
        }
    }
}

impl Checker {
    /// A loop body runs again after its first pass, so a narrowing that held
    /// when the loop was entered holds at the head only if no pass of the body
    /// can overwrite it. The body is checked once, from the entry facts, so the
    /// facts every write or effectful call in the body would end are ended here.
    pub(super) fn end_narrowings_the_loop_can_overwrite(
        &mut self,
        arena: &ArenaProgram,
        roots: &[ArenaChild],
    ) {
        let writes = self.loop_writes(arena, roots);
        for (name, path) in &writes.targets {
            if let Some(binding) = self.lookup(*name).cloned() {
                self.forget_binding_projection(&binding, path);
            }
        }
        if writes.runs_effectful_callable {
            for scope in &mut self.scopes {
                for binding in scope.values_mut().filter(|binding| binding.mutable) {
                    binding.forget_narrowing(&[]);
                }
            }
        }
    }

    pub(super) fn forget_binding_projection(&mut self, selected: &Binding, path: &[Name]) {
        for scope in &mut self.scopes {
            for binding in scope.values_mut().filter(|binding| {
                binding.id == selected.id && binding.proof.same_binding(&selected.proof)
            }) {
                if binding.mutable {
                    binding.forget_narrowing(path);
                }
            }
        }
    }

    fn loop_writes(&self, arena: &ArenaProgram, roots: &[ArenaChild]) -> LoopWrites {
        let mut writes = LoopWrites::default();
        let mut pending = roots.to_vec();
        while let Some(child) = pending.pop() {
            match child {
                ArenaChild::Stmt(statement) => {
                    if let ArenaStmtKind::Assign { target, .. } = arena.arena.stmt(statement).kind
                        && let Some(written) = assign_target_path(arena, target)
                        && !writes.targets.contains(&written)
                    {
                        writes.targets.push(written);
                    }
                    arena
                        .arena
                        .for_each_stmt_child(statement, SugarView::Core, |child| pending.push(child));
                }
                ArenaChild::Expr(expression) => {
                    if self.expression_runs_effectful_callable(arena, expression) {
                        writes.runs_effectful_callable = true;
                    }
                    arena
                        .arena
                        .for_each_expr_child(expression, |child| pending.push(child));
                }
                ArenaChild::Block(block) => arena
                    .arena
                    .for_each_block_child(block, |child| pending.push(child)),
                ArenaChild::Pattern(pattern) => arena
                    .arena
                    .for_each_pattern_child(pattern, |child| pending.push(child)),
                ArenaChild::BindingTarget(target) => arena
                    .arena
                    .for_each_binding_target_child(target, |child| pending.push(child)),
                ArenaChild::AssignTarget(target) => arena
                    .arena
                    .for_each_assign_target_child(target, |child| pending.push(child)),
                ArenaChild::TypeExpr(_) => {}
            }
        }
        if self.boundary.in_pure {
            writes.runs_effectful_callable = false;
        }
        writes
    }

    fn callee_may_be_effectful_callable(&self, arena: &ArenaProgram, callee: ExprId) -> bool {
        match arena.arena.expr(callee).kind {
            ArenaExprKind::Ident(_) => match self.lookup_value_binding_for_expr(arena, callee) {
                ValueBindingLookup::Found(binding) => {
                    matches!(binding.ty, Type::Inference(_)) || is_effectful_callable_type(&binding.ty)
                }
                ValueBindingLookup::Missing => true,
                ValueBindingLookup::NoSite => false,
            },
            ArenaExprKind::Field { name, .. } if name == "call" => true,
            ArenaExprKind::Field { base, name } => match self.static_receiver_type(arena, base) {
                Some(receiver) => member_is_effectful_callable(&receiver, name),
                None => false,
            },
            _ => true,
        }
    }

    fn static_receiver_type(&self, arena: &ArenaProgram, mut receiver: ExprId) -> Option<Type> {
        let mut path = Vec::new();
        loop {
            match arena.arena.expr(receiver).kind {
                ArenaExprKind::Field { base, name } if path.len() < MAX_PROJECTION_DEPTH => {
                    path.push(name);
                    receiver = base;
                }
                ArenaExprKind::Ident(_) => {
                    path.reverse();
                    let ValueBindingLookup::Found(binding) = self.lookup_value_binding_for_expr(arena, receiver)
                    else {
                        return None;
                    };
                    let ty = self.type_constraints.resolve(&binding.ty).ok()?;
                    return super::proof::projected_type(&ty, &path).cloned();
                }
                _ => return None,
            }
        }
    }

    fn expression_runs_effectful_callable(
        &self,
        arena: &ArenaProgram,
        expression: ExprId,
    ) -> bool {
        match arena.arena.expr(expression).kind {
            ArenaExprKind::Ident(name) => {
                self.procs.contains_key(&name)
                    || matches!(
                        self.lookup_value_binding_for_expr(arena, expression),
                        ValueBindingLookup::Found(binding) if is_effectful_callable_type(&binding.ty)
                    )
            }
            ArenaExprKind::Call { callee, .. } => {
                self.callee_may_be_effectful_callable(arena, callee)
                    || self
                        .resolve_callable_alias_call(arena, callee)
                        .is_some_and(|alias| alias.kind != CallableKind::Pure)
            }
            _ => false,
        }
    }
}

fn member_is_effectful_callable(receiver: &Type, name: Name) -> bool {
    match receiver {
        Type::Optional(inner) => member_is_effectful_callable(inner, name),
        Type::Record(fields) => fields.get(&name).is_some_and(is_effectful_callable_type),
        Type::Module(exports) => match exports.get(&name) {
            Some(ModuleExportType::Callable { kind, .. }) => *kind != CallableKind::Pure,
            Some(ModuleExportType::GenericCallable { .. }) => true,
            Some(ModuleExportType::Value { ty, .. }) => is_effectful_callable_type(ty),
            None => false,
        },
        _ => false,
    }
}

fn is_effectful_callable_type(ty: &Type) -> bool {
    match ty {
        Type::Function(kind) => *kind != CallableKind::Pure,
        Type::Callable(callable) => callable.kind != CallableKind::Pure,
        Type::SchemeCallable(_) => true,
        _ => false,
    }
}
