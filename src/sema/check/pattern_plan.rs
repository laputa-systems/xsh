use super::{Checker, Name, PatternCaptureIdentity, PatternIdentity, SolvedPattern,
    SolvedPatternCapture, SolvedPatternDecision, SolvedPatternShape, Span, Type};
use crate::sema::inference::{InferenceError, TypeId, TypeNode};
use crate::syntax::arena::{ArenaPatternKind, ArenaProgram, PatternId};
use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
#[path = "pattern_plan/tests.rs"]
mod tests;

impl Checker {
    // A subject can own absent-value binders independently of its lexical
    // callable. Preserve its checked expression scope on the original pattern
    // topology instead of deriving ownership from the pattern's type handles.
    pub(super) fn record_checked_pattern_value_scope(&mut self, arena: &ArenaProgram, pattern: PatternId, subject: crate::syntax::arena::ExprId) {
        if !self.graph_generation { return; }
        let subject = self.expression_identity(arena, subject);
        let scope = {
            let state = self.generic.borrow();
            state.facts.expression_schemes.get(&subject).or_else(|| state.facts.expression_value_scopes.get(&subject)).copied()
        };
        let Some(scope) = scope else { return; };
        let span = arena.arena.span(arena.arena.pattern(pattern).span);
        let outcome = (|| {
            let mut state = self.generic.borrow_mut();
            let mut pending = vec![(self.pattern_identity(arena, pattern), 0usize)];
            while let Some((identity, depth)) = pending.pop() {
                state.facts.graph.charge_source_fact_work(1)?;
                if depth > state.facts.graph.limits().structural_depth { return Err(InferenceError::Limit("pattern value scope depth")); }
                if (identity.source, identity.namespace) != (subject.source, subject.namespace) { return Err(InferenceError::InvalidScheme); }
                if !state.facts.publish_pattern_value_scope(identity, scope)? { continue; }
                let count = state.facts.patterns[&identity].children.len();
                state.facts.graph.charge_source_fact_work(count as u64)?;
                pending.extend(state.facts.patterns[&identity].children.iter().rev().map(|child| (*child, depth + 1)));
            }
            Ok::<_, InferenceError>(())
        })();
        if let Err(error) = outcome { self.graph_error(span, error); }
    }

    pub(super) fn checked_pattern_literal(&mut self, arena: &ArenaProgram, pattern: PatternId, expression: crate::syntax::arena::ExprId) {
        use crate::sema::constants::LiteralConstant;
        use crate::syntax::arena::ArenaExprKind;
        if !self.graph_generation { return; }
        let identity = self.pattern_identity(arena, pattern);
        if self.generic.borrow().facts.pattern_authority_is_published(identity) { return; }
        if !self.generic.borrow().facts.patterns.contains_key(&identity) { return; }
        let value = match &arena.arena.expr(expression).kind {
            ArenaExprKind::Null => Some(LiteralConstant::Null),
            ArenaExprKind::Bool(value) => Some(LiteralConstant::Bool(*value)),
            ArenaExprKind::Int(value) => arena.arena.int_literal(*value).value().map(LiteralConstant::Int),
            ArenaExprKind::Float(value) => arena.arena.float_literal(*value).value().map(|value| LiteralConstant::Float(value.to_bits())),
            ArenaExprKind::Duration(value) => arena.arena.duration_literal(*value).millis().map(LiteralConstant::Duration),
            ArenaExprKind::Str(value) => Some(LiteralConstant::Str(arena.arena.string_literal(*value).clone())),
            ArenaExprKind::PathStr(value) => Some(LiteralConstant::Path(arena.arena.string_literal(*value).clone())),
            ArenaExprKind::Bytes(value) => Some(LiteralConstant::Bytes(arena.arena.bytes_literal(*value).clone())),
            ArenaExprKind::Regex(value) => Some(LiteralConstant::Regex(arena.arena.regex_literal(*value).clone())),
            _ => self.prepared_constants.values.get(&expression).cloned(),
        };
        let mut state = self.generic.borrow_mut();
        if let Some(plan) = state.facts.patterns.get_mut(&identity)
            && let SolvedPatternShape::Literal { value: original, .. } = &mut std::sync::Arc::make_mut(plan).shape {
            *original = value;
        }
    }

    fn pattern_identity(&self, arena: &ArenaProgram, pattern: PatternId) -> PatternIdentity {
        PatternIdentity { source: arena.arena.span(arena.arena.pattern(pattern).span).source_id,
            namespace: self.current_namespace, pattern }
    }

    fn checked_pattern_type(&mut self, ty: &Type, span: Span) -> Result<Option<TypeId>, InferenceError> {
        if ty.is_recovery() { return Ok(None); }
        let input = match self.graph_type(ty, span) {
            Ok(input) => input,
            Err(InferenceError::Boundary(_) | InferenceError::Unresolved(_) | InferenceError::Recovery(_)) => return Ok(None),
            Err(error) => return Err(error),
        };
        let mut state = self.generic.borrow_mut();
        let graph = &mut state.facts.graph;
        let mut pending = vec![(input, 0usize)];
        let mut seen = BTreeSet::new();
        while let Some((ty, depth)) = pending.pop() {
            graph.charge_source_fact_work(1)?;
            if depth > graph.limits().structural_depth {
                return Err(InferenceError::Limit("checked pattern type depth"));
            }
            let ty = graph.resolved(ty)?;
            if !seen.insert(ty) { continue; }
            let next = depth + 1;
            match graph.node(ty)? {
                TypeNode::Poison | TypeNode::NonCompletion => return Ok(None),
                TypeNode::Optional(item) | TypeNode::List(item) | TypeNode::Stream(item) => pending.push((*item, next)),
                TypeNode::Map(left, right) | TypeNode::Result(left, right) => pending.extend([(*left, next), (*right, next)]),
                TypeNode::Arrow(arrow) => {
                    pending.extend(arrow.params.iter().map(|parameter| (parameter.ty, next)));
                    pending.push((arrow.result, next));
                }
                TypeNode::CallableChoice(signatures) => pending.extend(signatures.iter().map(|ty| (*ty, next))),
                TypeNode::FiniteDomain(alternatives) => pending.extend(alternatives.iter().map(|alternative| (alternative.ty, next))),
                TypeNode::NativeCallable(callable) => pending.push((callable.signature, next)),
                TypeNode::Module(fields) => pending.extend(fields.iter().map(|field| (field.ty, next))),
                TypeNode::Record(row) | TypeNode::Row(row) => {
                    let row = graph.row_data(*row)?;
                    pending.extend(row.fields.iter().map(|field| (field.ty, next)));
                    pending.extend(row.tail.map(|tail| (tail, next)));
                }
                TypeNode::Atom(_) | TypeNode::Meta(_) | TypeNode::Rigid { .. } => {}
            }
        }
        Ok(Some(input))
    }

    pub(super) fn begin_checked_pattern(&mut self, arena: &ArenaProgram, pattern: PatternId, input: &Type) {
        if !self.graph_generation { return; }
        let identity = self.pattern_identity(arena, pattern);
        // Conditions visit a pattern again when installing success-branch
        // bindings. That lexical visit reuses the original checked receipt.
        if self.generic.borrow().facts.pattern_authority_is_published(identity) { return; }
        let span = arena.arena.span(arena.arena.pattern(pattern).span);
        let outcome = (|| {
            let Some(input) = self.checked_pattern_type(input, span)? else { return Ok(None); };
            let child_count = match &arena.arena.pattern(pattern).kind {
                ArenaPatternKind::Group(_) | ArenaPatternKind::Alias { .. } => 1,
                ArenaPatternKind::List { elements, rest } => elements.len as usize + usize::from(rest.is_some()),
                ArenaPatternKind::Record { fields, .. } | ArenaPatternKind::ErrorVariant { fields, .. } => fields.len as usize,
                ArenaPatternKind::Alternation(patterns) | ArenaPatternKind::Tuple(patterns) => patterns.len as usize,
                ArenaPatternKind::Constructor { arg, .. } => usize::from(arg.is_some()),
                ArenaPatternKind::Wildcard | ArenaPatternKind::Binding(_) | ArenaPatternKind::Type { .. }
                    | ArenaPatternKind::TestName { .. } | ArenaPatternKind::Literal(_) | ArenaPatternKind::Facet(_) => 0,
            };
            {
                let mut state = self.generic.borrow_mut();
                let shape_edges = match &arena.arena.pattern(pattern).kind {
                    ArenaPatternKind::Record { fields, .. } | ArenaPatternKind::ErrorVariant { fields, .. } => fields.len as u64,
                    ArenaPatternKind::Literal(_) | ArenaPatternKind::Alias { .. } => 1,
                    _ => 0,
                };
                state.facts.graph.charge_source_fact_nodes(1)?;
                state.facts.graph.charge_source_fact_edges(1 + child_count as u64 + shape_edges)?;
            }
            let children = match &arena.arena.pattern(pattern).kind {
                ArenaPatternKind::Group(child) | ArenaPatternKind::Alias { pattern: child, .. } => vec![*child],
                ArenaPatternKind::List { elements, rest } => arena.arena.pattern_ids(*elements).chain(rest.iter().copied()).collect(),
                ArenaPatternKind::Record { fields, .. } | ArenaPatternKind::ErrorVariant { fields, .. } => arena.arena.pattern_fields(*fields).iter().map(|field| field.pattern).collect(),
                ArenaPatternKind::Alternation(patterns) | ArenaPatternKind::Tuple(patterns) => arena.arena.pattern_ids(*patterns).collect(),
                ArenaPatternKind::Constructor { arg, .. } => arg.iter().copied().collect(),
                ArenaPatternKind::Wildcard | ArenaPatternKind::Binding(_) | ArenaPatternKind::Type { .. }
                    | ArenaPatternKind::TestName { .. } | ArenaPatternKind::Literal(_) | ArenaPatternKind::Facet(_) => Vec::new(),
            };
            let children = children.into_iter().map(|child| self.pattern_identity(arena, child)).collect::<Vec<_>>();
            let shape = match &arena.arena.pattern(pattern).kind {
                ArenaPatternKind::Wildcard => SolvedPatternShape::Wildcard,
                ArenaPatternKind::Binding(_) => SolvedPatternShape::Binding,
                ArenaPatternKind::Literal(expr) => SolvedPatternShape::Literal {
                    expression: super::ExpressionIdentity { source: arena.arena.expr(*expr).span.source_id, namespace: self.current_namespace, expression: *expr },
                    value: self.prepared_constants.values.get(expr).filter(|value| matches!(value,
                        crate::sema::constants::LiteralConstant::Null | crate::sema::constants::LiteralConstant::Bool(_)
                        | crate::sema::constants::LiteralConstant::Int(_) | crate::sema::constants::LiteralConstant::Float(_)
                        | crate::sema::constants::LiteralConstant::Duration(_) | crate::sema::constants::LiteralConstant::Str(_)
                        | crate::sema::constants::LiteralConstant::Path(_) | crate::sema::constants::LiteralConstant::Bytes(_)
                        | crate::sema::constants::LiteralConstant::Regex(_))).cloned(),
                },
                ArenaPatternKind::Group(_) => SolvedPatternShape::Group,
                ArenaPatternKind::Alias { name, .. } => SolvedPatternShape::Alias { name: *name },
                ArenaPatternKind::List { elements, rest } => SolvedPatternShape::List { elements: elements.len, has_rest: rest.is_some() },
                ArenaPatternKind::Record { fields, .. } => SolvedPatternShape::Record { fields: arena.arena.pattern_fields(*fields).iter().map(|field| field.name).collect() },
                ArenaPatternKind::Alternation(_) => SolvedPatternShape::Alternation,
                ArenaPatternKind::Type { .. } => SolvedPatternShape::Type,
                ArenaPatternKind::TestName { .. } => SolvedPatternShape::TestName,
                ArenaPatternKind::Constructor { .. } => SolvedPatternShape::Constructor,
                ArenaPatternKind::ErrorVariant { fields, .. } => SolvedPatternShape::ErrorVariant { fields: arena.arena.pattern_fields(*fields).iter().map(|field| field.name).collect() },
                ArenaPatternKind::Facet(_) => SolvedPatternShape::Facet,
                ArenaPatternKind::Tuple(_) => SolvedPatternShape::Tuple,
            };
            Ok::<_, InferenceError>(Some(SolvedPattern { shape, input, caller: self.current_generic,
                tested: None, decision: SolvedPatternDecision::Structural, children, captures: Vec::new() }))
        })();
        match outcome {
            Ok(Some(plan)) => { self.generic.borrow_mut().facts.patterns.insert(identity, std::sync::Arc::new(plan)); }
            Ok(None) => { self.generic.borrow_mut().facts.patterns.remove(&identity); }
            Err(error) => { self.generic.borrow_mut().facts.patterns.remove(&identity); self.graph_error(span, error); }
        }
    }

    pub(super) fn finish_checked_pattern(&mut self, arena: &ArenaProgram, pattern: PatternId, diagnostics_before: usize) {
        if !self.graph_generation { return; }
        let identity = self.pattern_identity(arena, pattern);
        if self.generic.borrow().facts.pattern_authority_is_published(identity) { return; }
        let has_error = self.diagnostics[diagnostics_before..].iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error);
        let result = {
            let mut state = self.generic.borrow_mut();
            let incomplete = state.facts.patterns.get(&identity).is_some_and(|plan| plan.children.iter().any(|child| !state.facts.patterns.contains_key(child)));
            if has_error || incomplete {
                state.facts.remove_pattern_authority(identity);
                Ok(())
            } else if state.facts.patterns.contains_key(&identity) {
                state.facts.publish_pattern_authority(identity)
            } else { Ok(()) }
        };
        if let Err(error) = result {
            self.generic.borrow_mut().facts.remove_pattern_authority(identity);
            self.graph_error(arena.arena.span(arena.arena.pattern(pattern).span), error);
        }
    }

    pub(super) fn unavailable_checked_pattern(&mut self, arena: &ArenaProgram, pattern: PatternId) {
        if !self.graph_generation { return; }
        let identity = self.pattern_identity(arena, pattern);
        if self.generic.borrow().facts.pattern_authority_is_published(identity) { return; }
        let mut state = self.generic.borrow_mut();
        state.facts.remove_pattern_authority(identity);
    }

    pub(super) fn checked_tag_pattern(&mut self, arena: &ArenaProgram, pattern: PatternId,
        _constructor: Name, info: &super::TagVariantInfo,
    ) {
        if let Some(fields) = self.checked_pattern_fields(arena, pattern, &info.field_types) {
            self.checked_pattern_decision(arena, pattern, Some(&Type::Tag(info.type_name)),
                SolvedPatternDecision::TagConstructor { type_name: info.type_name, constructor: info.canonical_name, identity: info.identity, fields });
        } else { self.unavailable_checked_pattern(arena, pattern); }
    }

    pub(super) fn checked_pattern_decision(&mut self, arena: &ArenaProgram, pattern: PatternId,
        tested: Option<&Type>, decision: SolvedPatternDecision,
    ) {
        if !self.graph_generation { return; }
        let identity = self.pattern_identity(arena, pattern);
        if self.generic.borrow().facts.pattern_authority_is_published(identity) { return; }
        if !self.generic.borrow().facts.patterns.contains_key(&identity) { return; }
        let span = arena.arena.span(arena.arena.pattern(pattern).span);
        let outcome = (|| {
            let tested = match tested {
                Some(tested) => { let Some(tested) = self.checked_pattern_type(tested, span)? else { return Ok(None); }; Some(tested) }
                None => None,
            };
            let fields = match &decision {
                SolvedPatternDecision::Result { payload, .. } => usize::from(payload.is_some()),
                SolvedPatternDecision::TagConstructor { fields, .. } | SolvedPatternDecision::TagFields { fields } => fields.len(),
                SolvedPatternDecision::ErrorVariant { fields, .. } => fields.len(),
                _ => 0,
            };
            self.generic.borrow_mut().facts.graph.charge_source_fact_edges(fields as u64 + u64::from(tested.is_some()))?;
            Ok::<_, InferenceError>(Some(tested))
        })();
        match outcome {
            Ok(Some(tested)) => {
                let mut state = self.generic.borrow_mut();
                let plan = std::sync::Arc::make_mut(state.facts.patterns.get_mut(&identity).unwrap());
                plan.tested = tested;
                plan.decision = decision;
            }
            Ok(None) => { self.generic.borrow_mut().facts.patterns.remove(&identity); }
            Err(error) => { self.generic.borrow_mut().facts.patterns.remove(&identity); self.graph_error(span, error); }
        }
    }

    pub(super) fn checked_pattern_fields(&mut self, arena: &ArenaProgram, pattern: PatternId, fields: &[Type]) -> Option<Vec<TypeId>> {
        if !self.graph_generation || !self.generic.borrow().facts.patterns.contains_key(&self.pattern_identity(arena, pattern)) { return None; }
        if self.generic.borrow().facts.pattern_authority_is_published(self.pattern_identity(arena, pattern)) { return None; }
        let span = arena.arena.span(arena.arena.pattern(pattern).span);
        let outcome = fields.iter().map(|field| self.checked_pattern_type(field, span)).collect::<Result<Option<Vec<_>>, _>>();
        match outcome {
            Ok(fields) => fields,
            Err(error) => { self.graph_error(span, error); None }
        }
    }

    pub(super) fn checked_pattern_capture(&mut self, arena: &ArenaProgram, pattern: PatternId, name: Name, branches: Vec<PatternCaptureIdentity>) {
        if !self.graph_generation { return; }
        let identity = self.pattern_identity(arena, pattern);
        if self.generic.borrow().facts.pattern_authority_is_published(identity) { return; }
        let span = arena.arena.span(arena.arena.pattern(pattern).span);
        let outcome = (|| {
            let mut state = self.generic.borrow_mut();
            let Some(plan) = state.facts.patterns.get(&identity) else { return Ok(()); };
            let ty = if let Some(branch) = branches.first() {
                state.facts.patterns.get(&branch.pattern).and_then(|plan| plan.captures.iter().find(|capture| capture.identity == *branch)).map(|capture| capture.ty)
                    .ok_or(InferenceError::InvalidScheme)?
            } else if matches!(plan.decision, SolvedPatternDecision::Type) {
                plan.tested.ok_or(InferenceError::InvalidScheme)?
            } else { plan.input };
            state.facts.graph.charge_source_fact_nodes(1)?;
            state.facts.graph.charge_source_fact_edges(1 + branches.len() as u64)?;
            std::sync::Arc::make_mut(state.facts.patterns.get_mut(&identity).unwrap()).captures.push(SolvedPatternCapture {
                identity: PatternCaptureIdentity { pattern: identity, name }, ty, branches,
            });
            Ok::<_, InferenceError>(())
        })();
        if let Err(error) = outcome { self.graph_error(span, error); }
    }

    pub(super) fn checked_alternative_capture_origins(&mut self, arena: &ArenaProgram, branch: PatternId) -> Option<BTreeMap<Name, PatternCaptureIdentity>> {
        if !self.graph_generation { return None; }
        let span = arena.arena.span(arena.arena.pattern(branch).span);
        let outcome = (|| {
            let mut state = self.generic.borrow_mut();
            let mut pending = vec![(self.pattern_identity(arena, branch), 0usize)];
            let mut origins = BTreeMap::new();
            while let Some((identity, depth)) = pending.pop() {
                state.facts.graph.charge_source_fact_work(1)?;
                if depth > state.facts.graph.limits().structural_depth { return Err(InferenceError::Limit("pattern capture ancestry depth")); }
                let Some(plan) = state.facts.patterns.get(&identity) else { return Ok(None); };
                let work = plan.captures.len() as u64 + plan.children.len() as u64;
                state.facts.graph.charge_source_fact_work(work)?;
                let Some(plan) = state.facts.patterns.get(&identity) else { return Ok(None); };
                for capture in &plan.captures { origins.entry(capture.identity.name).or_insert(capture.identity); }
                // A nested alternative already owns the checked join of its
                // branches. Its leaf definitions cannot replace that join.
                if matches!(plan.decision, SolvedPatternDecision::Alternation) { continue; }
                pending.extend(plan.children.iter().rev().map(|child| (*child, depth + 1)));
            }
            Ok::<_, InferenceError>(Some(origins))
        })();
        match outcome { Ok(origin) => origin, Err(error) => { self.graph_error(span, error); None } }
    }
}
