use super::{BTreeMap, BTreeSet, Checker, Diagnostic, Label, Span, Type};
use crate::syntax::arena::{ArenaBindingTargetKind, ArenaExprKind, ArenaExprOrRun, ArenaProgram, ArenaStmtKind, BindingTargetId, StmtId};

#[derive(Clone, Default)]
pub(super) struct LocalInference {
    bindings: BTreeMap<Span, Type>,
    nonmaterial_expressions: BTreeSet<Span>,
    pub(super) checked_bindings: BTreeMap<Span, Type>,
}

impl Checker {
    /// Checked facts carry canonical types, never inference identities. A
    /// failed material contract becomes a recovery fact after its diagnostic.
    pub(super) fn resolve_checked_types(&mut self) {
        let mut reported = self.diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("check.local-inference"))
            .flat_map(|diagnostic| diagnostic.labels.iter().map(|label| label.span)).collect::<BTreeSet<_>>();
        let constraints = &self.type_constraints;
        let diagnostics = &mut self.diagnostics;
        self.expr_types.retain(|span, ty| {
            !self.local_inference.nonmaterial_expressions.contains(span)
                || !matches!(constraints.resolve(ty), Ok(resolved) if resolved.contains_inference())
        });
        for (span, ty) in &mut self.expr_types {
            finalize_type(constraints, ty, *span, &mut reported, diagnostics);
        }
        for ((_, span), fact) in &mut self.stream_stage_types {
            finalize_type(constraints, &mut fact.input, *span, &mut reported, diagnostics);
            finalize_type(constraints, &mut fact.output, *span, &mut reported, diagnostics);
        }
        for (span, ty) in &mut self.function_return_types {
            finalize_type(constraints, ty, *span, &mut reported, diagnostics);
        }
        for fact in &mut self.annotation_facts {
            let span = match fact.kind {
                super::AnnotationFactKind::Binding { span, .. } | super::AnnotationFactKind::DefaultedParam { span, .. } => span,
                super::AnnotationFactKind::InferredPureReturn { body } | super::AnnotationFactKind::ExportedProcReturn { body } => body,
            };
            finalize_type(constraints, &mut fact.ty, span, &mut reported, diagnostics);
        }
        for (span, ty) in &mut self.local_inference.checked_bindings {
            finalize_type(constraints, ty, *span, &mut reported, diagnostics);
        }
        for (span, ty) in &mut self.parameter_types {
            finalize_type(constraints, ty, *span, &mut reported, diagnostics);
        }
        for (span, fact) in &mut self.record_constructor_instances {
            finalize_type(constraints, &mut fact.ty, *span, &mut reported, diagnostics);
            for argument in &mut fact.instance.arguments {
                finalize_type(constraints, argument, *span, &mut reported, diagnostics);
            }
        }
        for (span, projection) in &mut self.projections {
            finalize_projection(constraints, projection, *span, &mut reported, diagnostics);
        }
        for (span, alias) in &mut self.static_callable_aliases {
            finalize_callable(constraints, &mut alias.signature, *span, &mut reported, diagnostics);
        }
        let fallback = Span::at(crate::source::SourceId::new(0), 0);
        for signature in self.procs.values_mut().chain(self.pures.values_mut()).chain(self.streams.values_mut())
            .chain(self.qualified_procs.values_mut()).chain(self.qualified_pures.values_mut()).chain(self.qualified_streams.values_mut())
        {
            for parameter in &mut signature.params {
                finalize_type(constraints, &mut parameter.ty, fallback, &mut reported, diagnostics);
            }
            finalize_type(constraints, &mut signature.return_ty, fallback, &mut reported, diagnostics);
        }
        for module in self.user_modules.values_mut() {
            for ty in module.values.values_mut().chain(module.resolved_types.values_mut()) {
                finalize_type(constraints, ty, fallback, &mut reported, diagnostics);
            }
            for signature in module.procs.values_mut().chain(module.pures.values_mut()).chain(module.streams.values_mut()) {
                for parameter in &mut signature.params {
                    finalize_type(constraints, &mut parameter.ty, fallback, &mut reported, diagnostics);
                }
                finalize_type(constraints, &mut signature.return_ty, fallback, &mut reported, diagnostics);
            }
        }
        for scope in &mut self.scopes {
            for binding in scope.values_mut() {
                binding.ty = constraints.resolve(&binding.ty).unwrap_or(Type::Invalid);
                if binding.ty.contains_inference() { binding.ty = Type::Invalid; }
                if let Some(ty) = &mut binding.unrefined_ty {
                    *ty = constraints.resolve(ty).unwrap_or(Type::Invalid);
                    if ty.contains_inference() { *ty = Type::Invalid; }
                }
            }
        }
    }

    pub(super) fn record_inert_local_discard(&mut self, program: &ArenaProgram, target: BindingTargetId, initializer: ArenaExprOrRun) {
        if matches!(program.arena.binding_target(target).kind, ArenaBindingTargetKind::Name(name) if name == "_") {
            self.record_inert_expression_discard(program, initializer);
        }
    }

    pub(super) fn record_inert_expression_discard(&mut self, program: &ArenaProgram, expression: ArenaExprOrRun) {
        if self.lookup(super::Name::intern("map")).is_none() && empty_map_call(program, expression) {
            self.local_inference.nonmaterial_expressions.insert(super::expr::expr_or_run_span_arena(program, expression));
            if self.graph_generation && let ArenaExprOrRun::Expr(expression) = expression {
                let identity = self.expression_identity(program, expression);
                let outcome = (|| {
                    let mut state = self.generic.borrow_mut();
                    if state.facts.expression_schemes.contains_key(&identity) { return Ok(()); }
                    let Some(&ty) = state.facts.expressions.get(&identity) else { return Ok(()); };
                    let Some(operation) = state.facts.operations.get(&identity) else { return Ok(()); };
                    let requirements = [operation.requirement];
                    let scheme = state.facts.graph.generalize(ty, 0, crate::sema::inference::Generalization::Allowed, &requirements)?;
                    if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).unwrap().requirements.retain(|requirement| !requirements.contains(requirement)); }
                    state.facts.expression_schemes.insert(identity, scheme);
                    Ok::<_, crate::sema::inference::InferenceError>(())
                })();
                if let Err(error) = outcome { self.graph_error(program.arena.expr(expression).span, error); }
            }
        }
    }

    pub(super) fn is_inert_expression_discard(&self, span: Span) -> bool {
        self.local_inference.nonmaterial_expressions.contains(&span)
    }

    pub(super) fn local_binding_expectation(&self, span: Span) -> Option<Type> {
        self.local_inference.bindings.get(&span).and_then(|ty| self.type_constraints.resolve(ty).ok())
    }

    /// Returned local values retain the binding's monomorphic identities. Resolve
    /// earlier contributions before comparison, and let a declared return
    /// anchor remaining holes through the same implicit Ok boundary as expressions.
    pub(super) fn resolve_local_tail_type(&mut self, ty: Type, expected: Option<&Type>, span: Span) -> Type {
        let resolved = self.type_constraints.resolve(&ty).unwrap_or(Type::Invalid);
        if resolved.contains_inference() && let Some(expected) = expected {
            let context = if resolved.is_result() { expected } else { expected.result_ok().unwrap_or(expected) };
            self.expect_type(context, &resolved, span);
        }
        self.type_constraints.resolve(&resolved).unwrap_or(Type::Invalid)
    }

    // Later parameter and return destinations can solve holes nested in a
    // binding. Preserve their identities until every source constraint is checked.
    pub(super) fn record_checked_local_binding(&mut self, program: &ArenaProgram, target: BindingTargetId, span: Span, ty: &Type) {
        if self.current_return.is_some()
            && matches!(program.arena.binding_target(target).kind, ArenaBindingTargetKind::Name(name) if name != "_")
        {
            self.local_inference.checked_bindings.insert(span, ty.clone());
        }
    }

    pub(super) fn safe_value_initializer(&mut self, arena: &ArenaProgram, initializer: ArenaExprOrRun) -> bool {
        self.inert_value_expressions(arena, initializer).is_some()
    }

    pub(super) fn scope_inert_error_propagation(&mut self, arena: &ArenaProgram, operand: crate::syntax::arena::ExprId, propagation: crate::syntax::arena::ExprId, ty: &Type) {
        use crate::sema::inference::{Generalization, InferenceError, TypeNode};
        let ArenaExprKind::Call { callee, .. } = arena.arena.expr(operand).kind else { return; };
        if !matches!(arena.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "Err" && self.lookup(name).is_none())
            || !self.safe_value_initializer(arena, ArenaExprOrRun::Expr(operand)) { return; }
        let identity = self.expression_identity(arena, propagation);
        let constructor = self.expression_identity(arena, operand);
        let span = arena.arena.expr(operand).span;
        let outcome = (|| {
            let value = self.graph_type(ty, span)?;
            let mut state = self.generic.borrow_mut();
            state.facts.graph.charge_source_fact_nodes(1)?;
            state.facts.non_completing_expressions.insert(identity);
            let TypeNode::Result(success, _) = state.facts.graph.node(state.facts.graph.resolved(value)?)? else { return Err(InferenceError::InvalidScheme); };
            if !matches!(state.facts.graph.node(state.facts.graph.resolved(*success)?)?, TypeNode::Meta(_)) { return Ok(()); }
            let requirements = state.facts.operations.get(&constructor).map(|operation| vec![operation.requirement]).unwrap_or_default();
            let scheme = state.facts.graph.generalize(value, if self.current_generic.is_some() { 1 } else { 0 }, Generalization::Allowed, &requirements)?;
            if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).unwrap().requirements.retain(|requirement| !requirements.contains(requirement)); }
            state.facts.expression_schemes.insert(constructor, scheme);
            state.facts.expression_value_scopes.insert(identity, scheme);
            Ok(())
        })();
        if let Err(error) = outcome { self.graph_error(span, error); }
    }

    // A lexical return never supplies the projected success value. Its absent
    // payload remains in the original expression scope through propagation.
    pub(super) fn scope_returning_capture_propagation(&mut self, arena: &ArenaProgram, operand: crate::syntax::arena::ExprId, propagation: crate::syntax::arena::ExprId) {
        if !self.graph_generation || !self.return_inference_expression_returns(arena, operand) { return; }
        let operand = self.expression_identity(arena, operand);
        let propagation = self.expression_identity(arena, propagation);
        let outcome = (|| {
            let mut state = self.generic.borrow_mut();
            let scope = state.facts.expression_schemes.get(&operand).or_else(|| state.facts.expression_value_scopes.get(&operand)).copied();
            if let Some(scope) = scope { state.facts.expression_value_scopes.insert(propagation, scope); }
            if !state.facts.non_completing_expressions.contains(&propagation) {
                state.facts.graph.charge_source_fact_nodes(1)?;
                state.facts.non_completing_expressions.insert(propagation);
            }
            Ok::<_, crate::sema::inference::InferenceError>(())
        })();
        if let Err(error) = outcome { self.graph_error(arena.arena.expr(propagation.expression).span, error); }
    }

    pub(super) fn non_completing_block_tail(&self, arena: &ArenaProgram, block: crate::syntax::arena::BlockId) -> bool {
        let Some(tail) = arena.arena.stmt_ids(arena.arena.block(block).statements).last() else { return false; };
        match arena.arena.stmt(tail).kind {
            ArenaStmtKind::Return(_) => true,
            ArenaStmtKind::Expr(expression) => self.non_completing_expression(arena, expression),
            ArenaStmtKind::If { branches, else_block: Some(other) } => arena.arena.if_branches(branches).iter().all(|branch| self.non_completing_block_tail(arena, branch.block)) && self.non_completing_block_tail(arena, other),
            _ => false,
        }
    }

    pub(super) fn non_completing_expression(&self, arena: &ArenaProgram, expression: crate::syntax::arena::ExprId) -> bool {
        if self.generic.borrow().facts.non_completing_expressions.contains(&self.expression_identity(arena, expression)) { return true; }
        match arena.arena.expr(expression).kind {
            ArenaExprKind::ValueBlock(block) => self.non_completing_block_tail(arena, block),
            ArenaExprKind::If { branches, else_value } => arena.arena.if_expr_branches(branches).iter().all(|branch| self.non_completing_expression(arena, branch.value)) && self.non_completing_expression(arena, else_value),
            _ => false,
        }
    }

    pub(super) fn scope_absent_capture_success(&mut self, arena: &ArenaProgram, error: Type, span: Span) -> Type {
        use crate::sema::inference::Generalization;
        let Some(expression) = self.current_expression else { return Type::Invalid; };
        let identity = self.expression_identity(arena, expression);
        let outcome = (|| {
            let error = self.graph_type(&error, span)?;
            let mut state = self.generic.borrow_mut();
            let level = if self.current_generic.is_some() { 2 } else { 1 };
            let success = state.facts.graph.fresh(level, span)?;
            let value = state.facts.graph.result(success, error)?;
            let scheme = state.facts.graph.generalize(value, level - 1, Generalization::Allowed, &[])?;
            state.facts.expression_schemes.insert(identity, scheme);
            Ok::<_, crate::sema::inference::InferenceError>(value)
        })();
        match outcome { Ok(value) => self.graph_view(value), Err(error) => { self.graph_error(span, error); Type::Invalid } }
    }

    fn inert_error_constructor(&self, arena: &ArenaProgram, callee: crate::syntax::arena::ExprId) -> bool {
        let ArenaExprKind::Field { base, name: variant } = arena.arena.expr(callee).kind else { return false; };
        let family = match arena.arena.expr(base).kind {
            ArenaExprKind::Ident(family) => family,
            ArenaExprKind::Field { base, name: family } => {
                let ArenaExprKind::Ident(namespace) = arena.arena.expr(base).kind else { return false; };
                super::Name::intern(format!("{namespace}.{family}"))
            }
            _ => return false,
        };
        self.error_families.get(&family).is_some_and(|family| family.variants.contains_key(&variant))
    }

    fn inert_value_expressions(&mut self, arena: &ArenaProgram, initializer: ArenaExprOrRun) -> Option<Vec<crate::syntax::arena::ExprId>> {
        let ArenaExprOrRun::Expr(expression) = initializer else { return None; };
        let immutable_name = |name| self.lookup(name).map(|binding| !binding.mutable)
            .unwrap_or_else(|| self.generic.borrow().names.contains_key(&(self.current_namespace, name)));
        let mut pending = vec![expression];
        let mut expressions = Vec::new();
        while let Some(expression) = pending.pop() {
            if self.generic.borrow_mut().facts.graph.charge_source_fact_work(1).is_err() { return None; }
            expressions.push(expression);
            match arena.arena.expr(expression).kind {
                ArenaExprKind::Null | ArenaExprKind::Bool(_) | ArenaExprKind::Int(_) | ArenaExprKind::Float(_)
                | ArenaExprKind::Duration(_) | ArenaExprKind::Str(_) | ArenaExprKind::PathStr(_) | ArenaExprKind::Bytes(_) => {}
                ArenaExprKind::Ident(name) if immutable_name(name) => {}
                ArenaExprKind::Field { base, name } if matches!(arena.arena.expr(base).kind, ArenaExprKind::Ident(module)
                    if self.lookup(module).is_none() && super::api_spec().module(&module.as_str())
                        .is_some_and(|entry| entry.function_overloads(&name.as_str()).is_some())) => {}
                ArenaExprKind::List(elements) => pending.extend(arena.arena.list_element_exprs(elements)),
                ArenaExprKind::Record(fields) => {
                    for field in arena.arena.record_fields(fields) {
                        match field.kind {
                            crate::syntax::arena::ArenaRecordFieldKind::Named { value, .. } | crate::syntax::arena::ArenaRecordFieldKind::Path { value, .. } => pending.push(value),
                            crate::syntax::arena::ArenaRecordFieldKind::Computed { key, value, .. } => { pending.push(key); pending.push(value); }
                            crate::syntax::arena::ArenaRecordFieldKind::Shorthand { name, .. } if immutable_name(name) => {}
                            crate::syntax::arena::ArenaRecordFieldKind::Spread { expr, .. } => pending.push(expr),
                            _ => return None,
                        }
                    }
                }
                ArenaExprKind::Call { callee, args } if matches!(arena.arena.expr(callee).kind, ArenaExprKind::Ident(name) if (name == "Ok" || name == "Err") && self.lookup(name).is_none()) => {
                    for argument in arena.arena.call_args(args) {
                        let value = match argument.kind {
                            crate::syntax::arena::ArenaCallArgKind::Positional(value) | crate::syntax::arena::ArenaCallArgKind::Named { value, .. }
                            | crate::syntax::arena::ArenaCallArgKind::NamedSpread { value, .. } | crate::syntax::arena::ArenaCallArgKind::Splice { value, .. } => value,
                        };
                        pending.push(value);
                    }
                }
                // A resolved error constructor only stores its checked fields.
                // Its arguments still need the same inert-value proof.
                ArenaExprKind::Call { callee, args } if self.inert_error_constructor(arena, callee) => {
                    for argument in arena.arena.call_args(args) {
                        let value = match argument.kind {
                            crate::syntax::arena::ArenaCallArgKind::Positional(value) | crate::syntax::arena::ArenaCallArgKind::Named { value, .. }
                            | crate::syntax::arena::ArenaCallArgKind::NamedSpread { value, .. } | crate::syntax::arena::ArenaCallArgKind::Splice { value, .. } => value,
                        };
                        pending.push(value);
                    }
                }
                _ if self.lookup(super::Name::intern("map")).is_none() && empty_map_call(arena, ArenaExprOrRun::Expr(expression)) => {}
                _ => return None,
            }
        }
        Some(expressions)
    }

    // Erasure leaves the physical container's fresh literal holes unobserved.
    // Their source facts have a scope; referenced values keep their original
    // levels and mutable bindings never acquire a polymorphic value scheme.
    pub(super) fn scope_erased_literal_facts(&mut self, arena: &ArenaProgram, expression: crate::syntax::arena::ExprId, expressions: &[crate::syntax::arena::ExprId]) {
        let identity = self.expression_identity(arena, expression);
        let identities: Vec<_> = expressions.iter().map(|expression| self.expression_identity(arena, *expression)).collect();
        let outcome = (|| {
            let mut state = self.generic.borrow_mut();
            state.facts.graph.charge_source_fact_work(identities.len() as u64)?;
            let Some(&ty) = state.facts.expressions.get(&identity) else { return Ok(()); };
            let scheme = state.facts.graph.generalize(ty, if self.current_generic.is_some() { 1 } else { 0 }, crate::sema::inference::Generalization::Allowed, &[])?;
            state.facts.expression_schemes.insert(identity, scheme);
            for descendant in identities { state.facts.expression_value_scopes.entry(descendant).or_insert(scheme); }
            Ok::<_, crate::sema::inference::InferenceError>(())
        })();
        if let Err(error) = outcome { self.graph_error(arena.arena.expr(expression).span, error); }
    }

    pub(super) fn scope_erased_value_initializer(&mut self, arena: &ArenaProgram, initializer: ArenaExprOrRun, span: Span) {
        if !self.graph_generation { return; }
        let ArenaExprOrRun::Expr(expression) = initializer else { return; };
        let identity = self.expression_identity(arena, expression);
        let Some(expressions) = self.inert_value_expressions(arena, initializer) else { return; };
        let identities: Vec<_> = expressions.into_iter().map(|expression| self.expression_identity(arena, expression)).collect();
        let outcome = (|| {
            let mut state = self.generic.borrow_mut();
            let Some(&ty) = state.facts.expressions.get(&identity) else { return Ok(()); };
            let mut requirements = Vec::new();
            for identity in &identities {
                if let Some(operation) = state.facts.operations.get(identity) { requirements.push(operation.requirement); }
                if let Some(reference) = state.facts.registry_references.get(identity) { requirements.extend_from_slice(&reference.requirements(&state.facts.graph)?); }
            }
            let scheme = state.facts.graph.generalize(ty, if self.current_generic.is_some() { 1 } else { 0 }, crate::sema::inference::Generalization::Allowed, &requirements)?;
            if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).unwrap().requirements.retain(|requirement| !requirements.contains(requirement)); }
            state.facts.expression_schemes.insert(identity, scheme);
            for descendant in identities { state.facts.expression_value_scopes.insert(descendant, scheme); }
            Ok::<_, crate::sema::inference::InferenceError>(())
        })();
        if let Err(error) = outcome { self.graph_error(span, error); }
    }

    pub(super) fn generalize_value_binding(&mut self, arena: &ArenaProgram, target: BindingTargetId, initializer: ArenaExprOrRun, ty: &Type, span: Span) {
        if !self.graph_generation { return; }
        if !ty.contains_graph() { return; }
        let ArenaBindingTargetKind::Name(name) = arena.arena.binding_target(target).kind else { return; };
        if name == "_" { return; }
        let ArenaExprOrRun::Expr(expression) = initializer else { return; };
        let identity = self.expression_identity(arena, expression);
        let Some(expressions) = self.inert_value_expressions(arena, initializer) else { return; };
        let identities: Vec<_> = expressions.into_iter().map(|expression| self.expression_identity(arena, expression)).collect();
        let outcome = (|| {
            let ty = self.graph_type(ty, span)?;
            let mut state = self.generic.borrow_mut();
            let mut requirements = Vec::new();
            for identity in &identities {
                if let Some(operation) = state.facts.operations.get(identity) { requirements.push(operation.requirement); }
                if let Some(reference) = state.facts.registry_references.get(identity) { requirements.extend_from_slice(&reference.requirements(&state.facts.graph)?); }
            }
            if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).unwrap().requirements.retain(|requirement| !requirements.contains(requirement)); }
            let scheme = state.facts.graph.generalize(ty, if self.current_generic.is_some() { 1 } else { 0 }, crate::sema::inference::Generalization::Allowed, &requirements)?;
            state.facts.expression_schemes.insert(identity, scheme);
            if state.facts.registry_references.contains_key(&identity)
                && let Some(callable) = state.facts.expression_callables.get_mut(&identity) {
                callable.scheme = Some(scheme);
            }
            for descendant in identities { state.facts.expression_value_scopes.insert(descendant, scheme); }
            if let Some(binding) = state.facts.bindings.get_mut(&super::BindingIdentity { source: span.source_id, namespace: self.current_namespace, target }) { binding.ty = ty; binding.scheme = Some(scheme); }
            Ok::<_, crate::sema::inference::InferenceError>(scheme)
        })();
        match outcome {
            Ok(scheme) => if let Some(binding) = self.current_scope_mut().get_mut(&name) { binding.value_scheme = Some(scheme); },
            Err(error) => self.graph_error(span, error),
        }
    }

    pub(super) fn infer_local_binding(&mut self, program: &ArenaProgram, target: BindingTargetId, initializer: ArenaExprOrRun, mutable: bool, span: Span, ordinary: Type) -> Type {
        if self.current_return.is_none() { return ordinary; }
        let Some(seed) = local_seed_kind(program, target, initializer, mutable) else { return ordinary; };
        if matches!(seed, LocalSeed::Map) && (self.lookup(super::Name::intern("map")).is_some() || !matches!(ordinary, Type::Map(_, _))) {
            return ordinary;
        }
        if self.current_generic.is_some() && self.graph_generation {
            let origin = super::expr::expr_or_run_span_arena(program, initializer);
            let ty = if ordinary.contains_graph() { ordinary } else {
                let result = (|| {
                    let mut state = self.generic.borrow_mut();
                    let variable = state.facts.graph.fresh(1, origin)?;
                    let ty = match seed {
                        LocalSeed::List => state.facts.graph.list(variable)?,
                        LocalSeed::Nullable => state.facts.graph.optional(variable)?,
                        LocalSeed::Map => {
                            let value = state.facts.graph.fresh(1, origin)?;
                            state.facts.graph.map(variable, value)?
                        }
                    };
                    Ok::<_, crate::sema::inference::InferenceError>(ty)
                })();
                match result { Ok(ty) => self.graph_view(ty), Err(error) => { self.graph_error(origin, error); Type::Invalid } }
            };
            self.local_inference.bindings.insert(span, ty.clone());
            self.local_inference.checked_bindings.insert(span, ty.clone());
            if let ArenaExprOrRun::Expr(expression) = initializer { self.record_graph_expression(program, expression, &ty); }
            return ty;
        }
        let raw = if let Some(ty) = self.local_inference.bindings.get(&span) { ty.clone() } else {
            let origin = super::expr::expr_or_run_span_arena(program, initializer);
            let variable = self.type_constraints.fresh(origin);
            let ty = match seed {
                LocalSeed::List => Type::List(Box::new(variable)),
                LocalSeed::Nullable => Type::Optional(Box::new(variable)),
                LocalSeed::Map => Type::Map(Box::new(variable), Box::new(self.type_constraints.fresh(origin))),
            };
            self.local_inference.bindings.insert(span, ty.clone());
            ty
        };
        {
            let resolved = self.type_constraints.resolve(&raw).unwrap_or(Type::Invalid);
            if resolved.contains_inference() {
                self.diagnostics.push(Diagnostic::error("local type needs an annotation")
                    .with_code("check.local-inference")
                    .with_label(Label::primary(super::expr::expr_or_run_span_arena(program, initializer), "no unique concrete type is established for this initializer")));
                self.local_inference.checked_bindings.insert(span, Type::Invalid);
                return Type::Invalid;
            }
            if matches!(seed, LocalSeed::Map) {
                self.expr_types.insert(super::expr::expr_or_run_span_arena(program, initializer), resolved.clone());
            }
            self.local_inference.checked_bindings.insert(span, resolved);
        }
        raw
    }
}

enum LocalSeed { List, Nullable, Map }

pub(super) fn finalize_type(constraints: &crate::sema::constraints::TypeConstraints, ty: &mut Type, span: Span, reported: &mut BTreeSet<Span>, diagnostics: &mut Vec<Diagnostic>) {
    match constraints.resolve(ty) {
        Ok(resolved) if !resolved.contains_inference() => *ty = resolved,
        Ok(resolved) => {
            let origins = constraints.unresolved(&resolved).unwrap_or_default();
            let new_origins = origins.into_iter().filter(|origin| reported.insert(*origin)).collect::<Vec<_>>();
            if !new_origins.is_empty() {
                let mut diagnostic = Diagnostic::error("material type needs an annotation")
                    .with_code("check.local-inference")
                    .with_label(Label::primary(span, "no unique concrete type is established for this value"));
                for origin in new_origins { diagnostic = diagnostic.with_label(Label::secondary(origin, "type inference started here")); }
                diagnostics.push(diagnostic);
            }
            *ty = Type::Invalid;
        }
        Err(_) => {
            if reported.insert(span) {
                diagnostics.push(Diagnostic::error("type inference cannot publish this contract")
                    .with_code("check.local-inference")
                    .with_label(Label::primary(span, "inference variables must belong to one bounded checking problem")));
            }
            *ty = Type::Invalid;
        }
    }
}

fn local_seed_kind(program: &ArenaProgram, target: BindingTargetId, initializer: ArenaExprOrRun, mutable: bool) -> Option<LocalSeed> {
    let ArenaBindingTargetKind::Name(name) = program.arena.binding_target(target).kind else { return None; };
    if name == "_" { return None; }
    let ArenaExprOrRun::Expr(expression) = initializer else { return None; };
    match program.arena.expr(expression).kind {
        ArenaExprKind::List(elements) if elements.is_empty() => Some(LocalSeed::List),
        ArenaExprKind::Null if mutable => Some(LocalSeed::Nullable),
        _ if empty_map_call(program, initializer) => Some(LocalSeed::Map),
        _ => None,
    }
}

pub(super) fn empty_map_call(program: &ArenaProgram, initializer: ArenaExprOrRun) -> bool {
    let ArenaExprOrRun::Expr(expression) = initializer else { return false; };
    let ArenaExprKind::Call { callee, args } = program.arena.expr(expression).kind else { return false; };
    if !args.is_empty() { return false; }
    let ArenaExprKind::Field { base, name } = program.arena.expr(callee).kind else { return false; };
    name == "empty" && matches!(program.arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "map")
}

pub(super) fn finalize_callable(constraints: &crate::sema::constraints::TypeConstraints, callable: &mut crate::sema::types::CallableType, span: Span, reported: &mut BTreeSet<Span>, diagnostics: &mut Vec<Diagnostic>) {
    for parameter in &mut callable.params {
        finalize_type(constraints, &mut parameter.ty, span, reported, diagnostics);
    }
    finalize_type(constraints, &mut callable.return_ty, span, reported, diagnostics);
}

pub(super) fn finalize_projection(constraints: &crate::sema::constraints::TypeConstraints, projection: &mut crate::sema::projection::CheckedProjection, span: Span, reported: &mut BTreeSet<Span>, diagnostics: &mut Vec<Diagnostic>) {
    finalize_type(constraints, &mut projection.value_type, span, reported, diagnostics);
    match &mut projection.callable {
        Some(crate::sema::types::ModuleExportType::Value { ty, .. }) => finalize_type(constraints, ty, span, reported, diagnostics),
        Some(crate::sema::types::ModuleExportType::Proc { sig, .. } | crate::sema::types::ModuleExportType::Pure { sig, .. }) => finalize_callable(constraints, sig, span, reported, diagnostics),
        None => {}
    }
}
