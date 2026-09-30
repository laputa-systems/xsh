use super::{ CallBinding, Checker, DeclarationIdentity, ExpressionIdentity, ReturnElaboration, SolvedCall, SolvedCallable, SolvedProjection, SolvedTypes, Type};
use crate::sema::inference::{Arrow, CallableKind, EffectSet, EffectSummary, Generalization, InferenceError, Parameter, RequirementId, TypeId, TypeNode};
use crate::source::Span;
use crate::symbol::Name;
use crate::syntax::arena::{ArenaCallArg, ArenaCallArgKind, ArenaExprKind, ArenaFunctionDef, ArenaProgram, BlockId, ExprId, FunctionDefId};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct GenericDeclaration {
    pub signature: TypeId,
    pub params: Vec<TypeId>,
    pub result: TypeId,
    pub kind: CallableKind,
    pub requirements: Vec<RequirementId>,
    pub completions: Vec<Type>,
    pub ambiguous_result_completion: bool,
}

/// Empty collection bodies still use a separate local constraint owner. Keep
/// their existing definition path intact until those variables have the same
/// retained owner as the callable graph, independently of authored headers.
pub(super) fn body_uses_legacy_collection_inference(arena: &ArenaProgram, body: BlockId) -> bool {
    let span = arena.arena.span(arena.arena.block(body).span);
    (0..arena.arena.expr_tags.len()).any(|index| {
        let expression = arena.arena.expr(ExprId::from_index(index));
        if expression.span.source_id != span.source_id || expression.span.start() < span.start() || expression.span.end() > span.end() { return false; }
        match expression.kind {
            ArenaExprKind::List(elements) => elements.is_empty(),
            ArenaExprKind::Call { callee, .. } => matches!(arena.arena.expr(callee).kind,
                ArenaExprKind::Field { base, name } if name == "empty" && matches!(arena.arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "map")),
            _ => false,
        }
    })
}

#[derive(Default)]
pub(super) struct GenericState {
    pub facts: SolvedTypes<crate::sema::inference::InferenceContext>,
    pub pending: BTreeMap<DeclarationIdentity, GenericDeclaration>,
    pub bodies: BTreeMap<BlockId, DeclarationIdentity>,
    pub names: BTreeMap<(Option<Name>, Name), DeclarationIdentity>,
    pub checking: BTreeSet<DeclarationIdentity>,
    pub completed: BTreeSet<DeclarationIdentity>,
    pub rejected: BTreeSet<DeclarationIdentity>,
    pub diagnostics: Vec<super::Diagnostic>,
}

impl Checker {
    pub(super) fn normalize_graph_result_completion(
        &mut self, arena: &ArenaProgram, expression: Option<ExprId>, statement: Option<crate::syntax::arena::StmtId>,
        expected: Option<&Type>, actual: Type, span: Span,
    ) -> Type {
        let Some(owner) = self.current_generic else { return actual; };
        let def = arena.arena.function_def(owner.declaration);
        if def.return_ty_defaulted || !self.type_from_arena(arena, def.return_ty).is_result() { return actual; }
        let Some(expected @ Type::Result(payload, _)) = expected else { return actual; };
        if expected.is_result_unit() || actual.is_result() || matches!(actual, Type::Unknown | Type::Invalid) { return actual; }
        if let Type::Graph(id) = actual {
            let ambiguous = {
                let state = self.generic.borrow();
                state.facts.graph.resolved(id).and_then(|id| state.facts.graph.node(id)).is_ok_and(|node| matches!(node, TypeNode::Meta(_)))
            };
            if ambiguous {
                if self.graph_generation { self.generic.borrow_mut().pending.get_mut(&owner).unwrap().ambiguous_result_completion = true; }
                return actual;
            }
        }
        if self.graph_generation {
            self.expect_type(payload, &actual, span);
            match self.graph_type(expected, span) {
                Ok(result) => {
                    let mut state = self.generic.borrow_mut();
                    if let Some(expression) = expression {
                        let identity = ExpressionIdentity { source: arena.arena.expr(expression).span.source_id, namespace: self.current_namespace, expression };
                        state.facts.result_wrappings.insert(identity, result);
                    } else if let Some(statement) = statement {
                        let identity = super::StatementIdentity { source: arena.arena.stmt(statement).span.source_id, namespace: self.current_namespace, statement };
                        state.facts.result_statement_wrappings.insert(identity, result);
                    }
                }
                Err(error) => self.graph_error(span, error),
            }
        }
        expected.clone()
    }
    pub(super) fn close_graph_effects(&mut self, arena: &ArenaProgram) {
        fn bits(effects: &[super::Effect]) -> EffectSet {
            let mut bits = EffectSet::EMPTY;
            for effect in effects {
                bits.0 |= match effect {
                    super::Effect::Fs => EffectSet::FS.0,
                    super::Effect::Net => EffectSet::NET.0,
                    super::Effect::Process => EffectSet::PROCESS.0,
                    super::Effect::Env => EffectSet::ENV.0,
                    super::Effect::Time => EffectSet::TIME.0,
                    super::Effect::Error => EffectSet::ERROR.0,
                    super::Effect::Io => EffectSet::IO.0,
                };
            }
            bits
        }
        let effects = self.effect_graph.facts(&self.effect_summaries);
        let declarations: Vec<_> = self.generic.borrow().facts.declarations.iter().map(|(id, declaration)| (*id, declaration.clone())).collect();
        for (identity, declaration) in declarations {
            let body = arena.arena.span(arena.arena.block(declaration.body).span);
            let effect_owner = super::EffectDeclarationId { namespace: identity.namespace, body };
            let summary = if declaration.kind == CallableKind::Pure { EffectSummary::Closed(EffectSet::EMPTY) } else {
                let Some(fact) = effects.get(&effect_owner) else { continue; };
                if let Some(bound) = &fact.effective {
                    if fact.required.as_ref().is_none_or(|required| required.iter().any(|effect| !Self::effects_covers(bound, effect))) {
                        self.error(body, "callable body exceeds its declared effect boundary", "check.effect-violation");
                    }
                }
                fact.effective.as_ref().map(|effects| EffectSummary::Closed(bits(effects))).unwrap_or(EffectSummary::Unknown)
            };
            let outcome = (|| {
                let mut state = self.generic.borrow_mut();
                let scheme_body = state.facts.graph.scheme(declaration.scheme)?.body;
                state.facts.graph.set_arrow_effects(declaration.signature, summary)?;
                state.facts.graph.set_arrow_effects(scheme_body, summary)?;
                let calls: Vec<_> = state.facts.calls.values().filter(|call| call.declaration == Some(identity)).map(|call| call.signature).collect();
                for call in calls { state.facts.graph.set_arrow_effects(call, summary)?; }
                Ok::<_, InferenceError>(())
            })();
            if let Err(error) = outcome { self.graph_error(body, error); }
        }
    }
    pub(super) fn record_graph_completion(&mut self, ty: &Type) {
        let Some(owner) = self.current_generic else { return; };
        if !self.graph_generation { return; }
        let ambiguous = if let Type::Graph(id) = ty {
            let state = self.generic.borrow();
            state.facts.graph.resolved(*id).and_then(|id| state.facts.graph.node(id)).is_ok_and(|node| matches!(node, TypeNode::Meta(_)))
        } else { false };
        let mut state = self.generic.borrow_mut();
        let declaration = state.pending.get_mut(&owner).unwrap();
        declaration.completions.push(ty.clone());
        declaration.ambiguous_result_completion |= ambiguous;
    }
    pub(super) fn record_statement_position(&mut self, arena: &ArenaProgram, id: crate::syntax::arena::StmtId, position: super::StatementPosition) {
        let span = arena.arena.stmt(id).span;
        self.statement_positions.insert(span, position);
        if self.graph_generation && self.current_generic.is_some() {
            let identity = super::StatementIdentity { source: span.source_id, namespace: self.current_namespace, statement: id };
            self.generic.borrow_mut().facts.statements.insert(identity, position);
        }
    }
    pub(super) fn finish_graph_declaration(&mut self, arena: &ArenaProgram, def: &ArenaFunctionDef, identity: DeclarationIdentity) {
        let span = arena.arena.span(arena.arena.block(def.body).span);
        let outcome = (|| {
            let (signature, result, kind, requirements) = {
                let state = self.generic.borrow();
                let pending = &state.pending[&identity];
                (pending.signature, pending.result, pending.kind, pending.requirements.clone())
            };
            let mut elaboration = if def.return_ty_defaulted { ReturnElaboration::Value } else {
                let ty = self.type_from_arena(arena, def.return_ty);
                if ty == Type::Unit || ty.is_result_unit() { ReturnElaboration::UnitConsuming } else { ReturnElaboration::Value }
            };
            if !def.return_ty_defaulted && self.type_from_arena(arena, def.return_ty).is_result()
                && elaboration != ReturnElaboration::UnitConsuming {
                let (completions, ambiguous) = {
                    let state = self.generic.borrow();
                    (state.pending[&identity].completions.clone(), state.pending[&identity].ambiguous_result_completion)
                };
                if ambiguous { return Err(InferenceError::Boundary("return payload-versus-Result interpretation needs an annotation")); }
                let _ = completions;
            }
            if def.return_ty_defaulted {
                let returns = self.inferred_returns.clone().unwrap_or_default();
                let mut payload = None;
                for (ty, contribution) in returns {
                    let ty = self.graph_type(&ty, contribution)?;
                    if let Some(previous) = payload {
                        let mut state = self.generic.borrow_mut();
                        let reason = state.facts.graph.reason(contribution, None)?;
                        state.facts.graph.unify(previous, ty, reason)?;
                    } else { payload = Some(ty); }
                }
                let payload = payload.ok_or(InferenceError::Unresolved(result))?;
                let value = self.graph_view(payload);
                let propagated = !self.inferred_propagations.is_empty();
                let wrap = (propagated && !value.is_result()) || (kind == CallableKind::Proc && value == Type::Unit);
                let final_result = if wrap {
                    let error = self.inferred_propagations.first().map(|(ty, _)| ty.clone()).unwrap_or(Type::Error);
                    let error = if self.inferred_propagations.iter().any(|(other, _)| other != &error) { Type::Error } else { error };
                    let error = self.graph_type(&error, span)?;
                    elaboration = ReturnElaboration::ImplicitResult;
                    self.generic.borrow_mut().facts.graph.result(payload, error)?
                } else { payload };
                let mut state = self.generic.borrow_mut();
                let reason = state.facts.graph.reason(span, None)?;
                state.facts.graph.unify(result, final_result, reason)?;
            }
            let scheme = {
                let mut state = self.generic.borrow_mut();
                state.facts.graph.solve()?;
                state.facts.graph.generalize(signature, 0, Generalization::Allowed, &requirements)?
            };
            let mut state = self.generic.borrow_mut();
            let binders = state.facts.graph.scheme_type_binders(scheme)?;
            for call in state.facts.calls.values_mut() {
                if call.declaration == Some(identity) && call.caller == Some(identity) && call.substitutions.is_empty() {
                    call.substitutions = binders.clone();
                    call.requirements = requirements.clone();
                }
            }
            state.facts.declarations.insert(identity, SolvedCallable { scheme, signature, body: def.body, kind, return_elaboration: elaboration });
            state.completed.insert(identity);
            state.checking.remove(&identity);
            Ok::<_, InferenceError>(result)
        })();
        match outcome {
            Ok(result) => {
                let ty = self.graph_view(result);
                self.function_return_types.insert(span, ty.clone());
                if def.return_ty_defaulted && ty.annotation_source().is_some() {
                    let kind = self.generic.borrow().pending[&identity].kind;
                    if kind == CallableKind::Pure {
                        self.annotation_facts.push(super::AnnotationFact { kind: super::AnnotationFactKind::InferredPureReturn { body: span }, ty });
                    } else if self.current_exported && ty.is_result_unit() {
                        self.annotation_facts.push(super::AnnotationFact { kind: super::AnnotationFactKind::ExportedProcReturn { body: span }, ty });
                    }
                }
            }
            Err(error) => {
                { let mut state = self.generic.borrow_mut(); state.checking.remove(&identity); state.rejected.insert(identity); }
                self.graph_error(span, error);
            }
        }
    }

    pub(super) fn graph_call(&mut self, arena: &ArenaProgram, source: &str, callee: ExprId, args: &[ArenaCallArg], span: Span) -> Option<Type> {
        if self.stage_ground_call_adapter { return None; }
        let (namespace, name) = match arena.arena.expr(callee).kind {
            ArenaExprKind::Ident(name) if self.lookup(name).is_none() => (self.current_namespace, name),
            ArenaExprKind::Field { base, name } => match arena.arena.expr(base).kind {
                ArenaExprKind::Ident(namespace) if self.lookup(namespace).is_some_and(|binding| binding.static_namespace) => (Some(namespace), name),
                _ => return None,
            },
            _ => return None,
        };
        let declaration = self.generic.borrow().names.get(&(namespace, name)).copied()?;
        if self.generic.borrow().rejected.contains(&declaration) { return Some(Type::Invalid); }
        let legacy_signature = if namespace == self.current_namespace {
            self.procs.get(&name).or_else(|| self.pures.get(&name)).cloned()
        } else {
            let qualified = crate::symbol::QualifiedName::new(namespace?, name);
            self.qualified_procs.get(&qualified).or_else(|| self.qualified_pures.get(&qualified)).cloned()
        };
        if let Some(signature) = &legacy_signature {
            if self.generic.borrow().pending[&declaration].kind == CallableKind::Proc {
                if self.in_pure { self.error(span, "effectful proc is not allowed in pure functions", "check.pure-effect"); }
                else { self.check_resolved_callable_effects(&signature, &name.to_string(), span); }
                self.invalidate_mutable_narrowings();
            }
            self.record_callee_propagation(&signature.effects, &signature.return_ty, span);
        }
        let expression = self.current_expression?;
        let key = self.expression_identity(arena, expression);
        // A legacy body can allocate new local collection variables during its
        // effect pass. Reuse the solved ground signature to check those actual
        // arguments; a cached result alone cannot anchor the new variables.
        if !self.graph_generation && self.legacy_collection_body
            && legacy_signature.as_ref().is_some_and(|signature|
                !signature.return_ty.contains_graph() && !signature.return_ty.contains_inference()
                    && signature.params.iter().all(|parameter| !parameter.ty.contains_graph() && !parameter.ty.contains_inference())) {
            return None;
        }
        if !self.graph_generation || self.generic.borrow().facts.calls.contains_key(&key) {
            let result = self.generic.borrow().facts.calls.get(&key).and_then(|call| {
                let state = self.generic.borrow();
                match state.facts.graph.node(state.facts.graph.resolved(call.signature).ok()?).ok()? {
                    TypeNode::Arrow(arrow) => Some(arrow.result), _ => None,
                }
            });
            return Some(result.map(|ty| self.graph_view(ty)).unwrap_or(Type::Invalid));
        }
        if !self.generic.borrow().completed.contains(&declaration) && !self.generic.borrow().checking.contains(&declaration) {
            let def = arena.arena.function_def(declaration.declaration).clone();
            let pure = self.generic.borrow().pending[&declaration].kind == CallableKind::Pure;
            let saved_scopes = self.scopes.clone();
            if self.current_generic.is_some() { self.scopes.truncate(1); }
            let saved_namespace = self.current_namespace;
            self.current_namespace = declaration.namespace;
            self.check_function_arena(arena, source, &def, pure);
            self.current_namespace = saved_namespace;
            self.scopes = saved_scopes;
        }
        let outcome = (|| {
            let (signature, requirements, substitutions) = {
                let mut state = self.generic.borrow_mut();
                if let Some(completed) = state.facts.declarations.get(&declaration) {
                    let scheme = completed.scheme;
                    let reason = state.facts.graph.reason(span, None)?;
                    let level = if self.current_generic.is_some() { 1 } else { 0 };
                    let instantiation = state.facts.graph.instantiate(scheme, level, reason)?;
                    (instantiation.ty, instantiation.requirements, instantiation.substitutions)
                } else {
                    (state.pending[&declaration].signature, Vec::new(), Vec::new())
                }
            };
            let arrow = {
                let state = self.generic.borrow();
                let TypeNode::Arrow(arrow) = state.facts.graph.node(state.facts.graph.resolved(signature)?)? else { return Err(InferenceError::InvalidScheme); };
                arrow.clone()
            };
            if self.in_pure && arrow.kind != CallableKind::Pure {
                self.error(span, "effectful proc is not allowed in pure functions", "check.pure-effect");
            }
            let mut supplied = Vec::new();
            let mut actual_arguments = Vec::new();
            let mut seen = BTreeSet::new();
            let rest_slot = arrow.params.iter().position(|parameter| parameter.rest);
            let mut positional = 0;
            for argument in args {
                let (slot, value) = match argument.kind {
                    ArenaCallArgKind::Positional(value) => {
                        let slot = if positional < arrow.params.len() { positional } else { rest_slot.ok_or(InferenceError::InvalidScheme)? };
                        positional += 1;
                        (slot, value)
                    }
                    ArenaCallArgKind::Named { name, value, .. } => (arrow.params.iter().position(|param| param.label == name).ok_or(InferenceError::InvalidScheme)?, value),
                    _ => return Err(InferenceError::InvalidScheme),
                };
                if slot >= arrow.params.len() || (!arrow.params[slot].rest && !seen.insert(slot)) { return Err(InferenceError::InvalidScheme); }
                seen.insert(slot);
                let expected = if arrow.params[slot].rest {
                    let state = self.generic.borrow();
                    match state.facts.graph.node(state.facts.graph.resolved(arrow.params[slot].ty)?)? {
                        TypeNode::List(item) => *item, _ => return Err(InferenceError::InvalidScheme),
                    }
                } else { arrow.params[slot].ty };
                let expected_view = self.graph_view(expected);
                let schema = legacy_signature.as_ref().and_then(|signature| signature.params.get(slot)).and_then(|parameter| {
                    if parameter.rest {
                        parameter.schema_expectation.as_ref().and_then(|schema| schema.children.get(&crate::sema::constants::SchemaComponent::Item)).cloned()
                    } else { parameter.schema_expectation.clone() }
                });
                self.graph_argument_depth += 1;
                let actual = self.check_expr_with_schema_arena(arena, source, crate::syntax::arena::ArenaExprOrRun::Expr(value), Some(&expected_view), schema);
                self.graph_argument_depth -= 1;
                if self.current_generic.is_none() && actual.contains_inference() && !expected_view.contains_graph() {
                    // The ground parameter contract anchors collections still
                    // owned by the local constraint store before they cross
                    // into the retained callable graph.
                    self.expect_type(&expected_view, &actual, arena.arena.expr(value).span);
                }
                let actual = self.graph_type(&actual, arena.arena.expr(value).span)?;
                let mut state = self.generic.borrow_mut();
                let reason = state.facts.graph.reason(arena.arena.expr(value).span, None)?;
                state.facts.graph.assignable(expected, actual, reason)?;
                let argument_identity = ExpressionIdentity { source: arena.arena.expr(value).span.source_id, namespace: self.current_namespace, expression: value };
                state.facts.expressions.insert(argument_identity, actual);
                if let Some(owner) = self.current_generic { state.facts.expression_owners.insert(argument_identity, owner); }
                supplied.push(slot);
                actual_arguments.push(actual);
            }
            let mut defaults = Vec::new();
            for (index, parameter) in arrow.params.iter().enumerate() {
                if seen.contains(&index) || parameter.rest { continue; }
                if !parameter.defaulted { return Err(InferenceError::InvalidScheme); }
                defaults.push(index);
            }
            let mut state = self.generic.borrow_mut();
            state.facts.graph.solve()?;
            if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).unwrap().requirements.extend(requirements.iter().copied()); }
            state.facts.calls.insert(key, SolvedCall {
                signature, declaration: Some(declaration), caller: self.current_generic,
                requirements, substitutions, actual_arguments,
                binding: CallBinding { supplied_slots: supplied, default_slots: defaults, rest_slot },
            });
            state.facts.expressions.insert(key, arrow.result);
            Ok::<_, InferenceError>(arrow.result)
        })();
        Some(match outcome { Ok(ty) => self.graph_view(ty), Err(error) => { self.graph_error(span, error); Type::Invalid } })
    }

    pub(super) fn graph_error(&mut self, span: Span, error: InferenceError) {
        let message = self.graph_error_message(&error);
        let code = if matches!(error, InferenceError::TypeMismatch { .. }) { "check.type-mismatch" } else { "check.type-relationship" };
        self.error(span, &message, code);
        if self.graph_generation { self.generic.borrow_mut().diagnostics.push(self.diagnostics.last().unwrap().clone()); }
    }

    fn graph_error_message(&self, error: &InferenceError) -> String {
        match error {
            InferenceError::TypeMismatch { left, right } => {
                let state = self.generic.borrow();
                let describe = |id| state.facts.graph.export_type(id).map(|ty| ty.to_string()).unwrap_or_else(|_| "a checked type relationship".to_string());
                format!("expected {}, found {}", describe(*left), describe(*right))
            }
            InferenceError::MissingField(field) => format!("record is missing required field `{field}`"),
            InferenceError::DuplicateLabel(field) | InferenceError::Lacks(field) => format!("record field `{field}` conflicts with its row relationship"),
            InferenceError::Occurs { .. } => "infinite type relationship: occurs check failed".to_string(),
            InferenceError::UnsupportedOperation(requirement) => {
                let state = self.generic.borrow();
                match state.facts.graph.requirement_template(*requirement) {
                    Ok(crate::sema::inference::RequirementTemplate::Add { left, right, .. }) => {
                        let describe = |ty| state.facts.graph.export_type(ty).map(|ty| ty.to_string()).unwrap_or_else(|_| "inferred operand".to_string());
                        format!("`+` does not support operand domains {} and {}", describe(left), describe(right))
                    }
                    Err(_) => "operands do not belong to a supported `+` domain".to_string(),
                }
            }
            InferenceError::DisconnectedRequirement(_) => "operation needs a local annotation; its type is independent of the callable signature".to_string(),
            InferenceError::Unresolved(_) => "type relationship needs a local annotation".to_string(),
            InferenceError::Recovery(_) => "an earlier type error prevents establishing this relationship".to_string(),
            InferenceError::ScopeEscape => "generic type parameter escapes its declaring scope".to_string(),
            InferenceError::EffectViolation => "callable effects exceed the checked effect boundary".to_string(),
            InferenceError::Boundary(reason) | InferenceError::Limit(reason) => format!("type relationship cannot be established: {reason}"),
            InferenceError::InvalidScheme => "arguments do not establish a complete callable relationship".to_string(),
            InferenceError::KindMismatch => "type and record-row relationships cannot be interchanged".to_string(),
            InferenceError::ForeignHandle => "type relationship belongs to a different checked source bundle".to_string(),
        }
    }

    pub(super) fn freeze_solved_types(&mut self) -> std::sync::Arc<SolvedTypes> {
        fn annotation_key(fact: &super::AnnotationFact) -> (u8, Span) {
            match fact.kind {
                super::AnnotationFactKind::Binding { span, .. } => (0, span),
                super::AnnotationFactKind::DefaultedParam { span, .. } => (1, span),
                super::AnnotationFactKind::InferredPureReturn { body } => (2, body),
                super::AnnotationFactKind::ExportedProcReturn { body } => (3, body),
            }
        }
        let mut annotations = std::mem::take(&mut self.annotation_facts);
        for fact in &mut annotations { fact.ty = self.resolved_graph_view(fact.ty.clone()); }
        annotations.sort_by_key(annotation_key);
        annotations.dedup_by(|left, right| annotation_key(left) == annotation_key(right));
        self.annotation_facts = annotations;
        self.reveal_types.sort_by_key(|diagnostic| diagnostic.labels.first().map(|label| label.span));
        self.reveal_types.dedup();
        let parameters = std::mem::take(&mut self.parameter_types);
        self.parameter_types = parameters.into_iter().map(|(span, ty)| (span, self.resolved_graph_view(ty))).collect();
        let returns = std::mem::take(&mut self.function_return_types);
        self.function_return_types = returns.into_iter().map(|(span, ty)| (span, self.resolved_graph_view(ty))).collect();
        let expressions = std::mem::take(&mut self.expr_types);
        self.expr_types = expressions.into_iter().map(|(span, ty)| (span, self.resolved_graph_view(ty))).collect();
        let facts = std::mem::take(&mut self.generic.borrow_mut().facts);
        match facts.freeze() {
            Ok(facts) => std::sync::Arc::new(facts),
            Err(error) => {
                let message = self.graph_error_message(&error);
                self.error(Span::at(crate::source::SourceId::new(0), 0), &message, "check.unsolved-relationship");
                std::sync::Arc::new(SolvedTypes::default())
            }
        }
    }

    pub(super) fn resolved_graph_view(&self, ty: Type) -> Type {
        if !ty.contains_graph() { return ty; }
        match ty {
            Type::Graph(id) => self.graph_view(id),
            Type::List(item) => Type::List(Box::new(self.resolved_graph_view(*item))),
            Type::Stream(item) => Type::Stream(Box::new(self.resolved_graph_view(*item))),
            Type::Optional(item) => Type::Optional(Box::new(self.resolved_graph_view(*item))),
            Type::Result(ok, error) => Type::Result(Box::new(self.resolved_graph_view(*ok)), Box::new(self.resolved_graph_view(*error))),
            Type::Map(key, value) => Type::Map(Box::new(self.resolved_graph_view(*key)), Box::new(self.resolved_graph_view(*value))),
            Type::Record(fields) => Type::Record(fields.into_iter().map(|(name, ty)| (name, self.resolved_graph_view(ty))).collect()),
            ty => ty,
        }
    }

    pub(super) fn expression_identity(&self, arena: &ArenaProgram, expression: ExprId) -> ExpressionIdentity {
        ExpressionIdentity { source: arena.arena.expr(expression).span.source_id, namespace: self.current_namespace, expression }
    }

    pub(super) fn graph_type(&mut self, ty: &Type, span: Span) -> Result<TypeId, InferenceError> {
        let level = if self.current_generic.is_some() { 1 } else { 0 };
        let ty = self.type_constraints.resolve(ty).map_err(|_| InferenceError::Boundary("legacy ground view could not be resolved"))?;
        self.generic.borrow_mut().facts.graph.import_type(&ty, level, span)
    }

    pub(super) fn graph_view(&self, ty: TypeId) -> Type {
        fn view(graph: &crate::sema::inference::InferenceContext, ty: TypeId, depth: usize) -> Type {
            if let Ok(ground) = graph.export_type(ty) { return ground; }
            if depth > 64 { return Type::Graph(ty); }
            let node = graph.resolved(ty).and_then(|resolved| graph.node(resolved));
            match node {
                Ok(TypeNode::List(item)) => Type::List(Box::new(view(graph, *item, depth + 1))),
                Ok(TypeNode::Stream(item)) => Type::Stream(Box::new(view(graph, *item, depth + 1))),
                Ok(TypeNode::Optional(item)) => Type::Optional(Box::new(view(graph, *item, depth + 1))),
                Ok(TypeNode::Result(ok, error)) => Type::Result(Box::new(view(graph, *ok, depth + 1)), Box::new(view(graph, *error, depth + 1))),
                Ok(TypeNode::Map(key, value)) => Type::Map(Box::new(view(graph, *key, depth + 1)), Box::new(view(graph, *value, depth + 1))),
                _ => Type::Graph(ty),
            }
        }
        view(&self.generic.borrow().facts.graph, ty, 0)
    }

    pub(super) fn graph_declaration(&self, body: BlockId) -> Option<DeclarationIdentity> {
        self.generic.borrow().bodies.get(&body).copied()
    }

    pub(super) fn graph_method_receiver(&mut self, ty: Type, name: &str, span: Span) -> Type {
        let Type::Graph(receiver) = ty else { return ty; };
        if !self.graph_generation { return self.graph_view(receiver); }
        let receivers: Vec<_> = super::api_spec().method_entries().filter(|(_, methods)|
            methods.iter().any(|method| method.name == name)).map(|(receiver, _)| receiver).collect();
        let [receiver_kind] = receivers.as_slice() else { return Type::Graph(receiver); };
        let ground = match receiver_kind {
            super::MethodReceiver::Str => Type::Str,
            super::MethodReceiver::Bytes => Type::Bytes,
            super::MethodReceiver::Path => Type::Path,
            super::MethodReceiver::Int => Type::Int,
            super::MethodReceiver::Float => Type::Float,
            super::MethodReceiver::Status => Type::Status,
            super::MethodReceiver::Digest => Type::Digest,
            super::MethodReceiver::Regex => Type::Regex,
            super::MethodReceiver::EnvPathList => Type::EnvPathList,
            super::MethodReceiver::ProcessHandle => Type::ProcessHandle,
            super::MethodReceiver::NetJob => Type::NetJob,
            super::MethodReceiver::FsRoot => Type::FsRoot,
            _ => return Type::Graph(receiver),
        };
        self.graph_expect(&ground, &Type::Graph(receiver), span);
        self.graph_view(receiver)
    }

    pub(super) fn register_graph_declaration(&mut self, arena: &ArenaProgram, id: FunctionDefId, kind: CallableKind) {
        let def = arena.arena.function_def(id);
        // Default-only headers retain the legacy concrete default boundary.
        // Required holes and ordinary value tails need declaration relationships.
        let default_only_holes = !def.return_ty_defaulted
            && arena.arena.params(def.params).iter().any(|param| param.ty_defaulted)
            && arena.arena.params(def.params).iter().all(|param| !param.ty_defaulted || (param.default.is_some() && !param.rest));
        if def.test_declaration || kind == CallableKind::Stream
            || default_only_holes || body_uses_legacy_collection_inference(arena, def.body) {
            return;
        }
        let span = arena.arena.span(arena.arena.block(def.body).span);
        let identity = DeclarationIdentity { source: span.source_id, namespace: self.current_namespace, declaration: id };
        if self.generic.borrow().pending.contains_key(&identity) { return; }
        if !self.graph_generation { return; }
        let result = (|| {
            let mut params = Vec::new();
            let mut arguments = Vec::new();
            for param in arena.arena.params(def.params) {
                let origin = arena.arena.span(param.span);
                let ty = if param.ty_defaulted {
                    let mut state = self.generic.borrow_mut();
                    let variable = state.facts.graph.fresh(1, origin)?;
                    if param.rest { state.facts.graph.list(variable)? } else { variable }
                } else {
                    let ground = self.type_from_arena(arena, param.ty);
                    self.graph_type(&ground, origin)?
                };
                params.push(ty);
                arguments.push(Parameter { label: param.name, ty, defaulted: param.default.is_some(), rest: param.rest });
            }
            let return_ty = if def.return_ty_defaulted {
                self.generic.borrow_mut().facts.graph.fresh(1, span)?
            } else {
                let ground = self.type_from_arena(arena, def.return_ty);
                self.graph_type(&ground, span)?
            };
            let signature = self.generic.borrow_mut().facts.graph.arrow(Arrow {
                kind, params: arguments, result: return_ty,
                effects: if kind == CallableKind::Pure { EffectSummary::Closed(EffectSet::EMPTY) } else { EffectSummary::Unknown },
            })?;
            Ok::<_, InferenceError>(GenericDeclaration { signature, params, result: return_ty, kind, requirements: Vec::new(), completions: Vec::new(), ambiguous_result_completion: false })
        })();
        match result {
            Ok(declaration) => {
                let mut state = self.generic.borrow_mut();
                state.pending.insert(identity, declaration);
                state.bodies.insert(def.body, identity);
                state.names.insert((self.current_namespace, def.name), identity);
            }
            Err(error) => self.graph_error(span, error),
        }
    }

    pub(super) fn graph_expect(&mut self, expected: &Type, actual: &Type, span: Span) -> bool {
        if self.current_generic.is_none() && !expected.contains_graph() && !actual.contains_graph() { return false; }
        if !self.graph_generation { return true; }
        let result = (|| {
            let expected = self.graph_type(expected, span)?;
            let actual = self.graph_type(actual, span)?;
            let mut state = self.generic.borrow_mut();
            let reason = state.facts.graph.reason(span, None)?;
            state.facts.graph.assignable(expected, actual, reason)
        })();
        if let Err(error) = result { self.graph_error(span, error); }
        true
    }

    pub(super) fn record_graph_expression(&mut self, arena: &ArenaProgram, id: ExprId, ty: &Type) {
        if !self.graph_generation { return; }
        if self.current_generic.is_none() && ty.contains_inference()
            && self.type_constraints.resolve(ty).is_ok_and(|ty| ty.contains_inference()) {
            return;
        }
        if self.current_generic.is_none() && self.graph_argument_depth == 0 && !ty.contains_graph() {
            if matches!(arena.arena.expr(id).kind, ArenaExprKind::Record(_)) {
                // Closed literal constructors keep their physical row shape even
                // when a later call receives them through a local binding.
                // Child types are projections of the already checked literal,
                // never the narrower row promised by a callable parameter.
                let mut pending = vec![(id, ty.clone())];
                while let Some((id, ty)) = pending.pop() {
                    let ArenaExprKind::Record(fields) = arena.arena.expr(id).kind else { continue; };
                    let Type::Record(record) = &ty else { continue; };
                    let result = self.graph_type(&ty, arena.arena.expr(id).span);
                    if let Ok(result) = result {
                        let mut state = self.generic.borrow_mut();
                        if state.facts.graph.export_type(result).is_ok() {
                            let identity = ExpressionIdentity { source: arena.arena.expr(id).span.source_id, namespace: self.current_namespace, expression: id };
                            state.facts.expressions.insert(identity, result);
                        }
                    }
                    for field in arena.arena.record_fields(fields) {
                        match field.kind {
                            crate::syntax::arena::ArenaRecordFieldKind::Named { name, value, .. } => {
                                if let Some(ty) = record.get(&name) { pending.push((value, ty.clone())); }
                            }
                            crate::syntax::arena::ArenaRecordFieldKind::Path { path, value, .. } => {
                                let mut field_ty = Some(&ty);
                                for name in arena.arena.names(path) {
                                    field_ty = field_ty.and_then(|ty| match ty { Type::Record(fields) => fields.get(&name), _ => None });
                                }
                                if let Some(ty) = field_ty { pending.push((value, ty.clone())); }
                            }
                            _ => {}
                        }
                    }
                }
            }
            return;
        }
        match self.graph_type(ty, arena.arena.expr(id).span) {
            Ok(ty) => {
                let key = self.expression_identity(arena, id);
                let mut state = self.generic.borrow_mut();
                state.facts.expressions.insert(key, ty);
                if let Some(owner) = self.current_generic { state.facts.expression_owners.insert(key, owner); }
            }
            Err(error) => self.graph_error(arena.arena.expr(id).span, error),
        }
    }

    pub(super) fn graph_projection(&mut self, arena: &ArenaProgram, id: ExprId, receiver: TypeId, field: Name) -> Type {
        let key = self.expression_identity(arena, id);
        if !self.graph_generation {
            return self.generic.borrow().facts.projections.get(&key).map(|fact| self.graph_view(fact.result)).unwrap_or(Type::Invalid);
        }
        let span = arena.arena.expr(id).span;
        let level = if self.current_generic.is_some() { 1 } else { 0 };
        let result = (|| {
            let mut state = self.generic.borrow_mut();
            let reason = state.facts.graph.reason(span, None)?;
            let result = state.facts.graph.require_field(receiver, field, level, reason)?;
            state.facts.projections.insert(key, SolvedProjection { receiver, field, result });
            Ok::<_, InferenceError>(result)
        })();
        match result { Ok(ty) => self.graph_view(ty), Err(error) => { self.graph_error(span, error); Type::Invalid } }
    }

    pub(super) fn graph_add(&mut self, arena: &ArenaProgram, id: ExprId, left: &Type, right: &Type) -> Type {
        let key = self.expression_identity(arena, id);
        let span = arena.arena.expr(id).span;
        if !self.graph_generation {
            return self.generic.borrow().facts.expressions.get(&key).map(|ty| self.graph_view(*ty)).unwrap_or(Type::Invalid);
        }
        let outcome = (|| {
            let left = self.graph_type(left, span)?;
            let right = self.graph_type(right, span)?;
            let mut state = self.generic.borrow_mut();
            let result = state.facts.graph.fresh(1, span)?;
            let reason = state.facts.graph.reason(span, None)?;
            let requirement = state.facts.graph.require_add(left, right, result, reason)?;
            state.facts.graph.solve()?;
            state.facts.additions.insert(key, requirement);
            if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).unwrap().requirements.push(requirement); }
            Ok::<_, InferenceError>(result)
        })();
        match outcome { Ok(ty) => self.graph_view(ty), Err(error) => { self.graph_error(span, error); Type::Invalid } }
    }
}
