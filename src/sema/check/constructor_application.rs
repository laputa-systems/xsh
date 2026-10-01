use super::{DeclarationIdentity, ExpressionIdentity, QualifiedNominalIdentity, SolvedSchemaApplication, SolvedSchemaExpectation};
use crate::sema::arguments::{ArgumentValueSource, ExpandedArgument};
use crate::sema::constants::{LiteralConstant, SchemaInstance};
use crate::sema::inference::{Arrow, CallableKind, ConstraintRelation, EffectSet, EffectSummary, Generalization, InferenceContext, InferenceError, OperationCall, Parameter, RequirementId, ScopedRequirementRoot, ScopedRoot, TypeId, TypeNode};
use crate::source::Span;
use crate::symbol::Name;
use crate::syntax::arena::{ArenaProgram, ArenaTypeDefBody, TypeDefId};
use std::collections::{BTreeMap, BTreeSet};

/// Applied records retain the called alias independently of the declaration
/// that owns field defaults. Nominal constructors retain their exact member.
#[derive(Clone, Debug)]
pub enum ConstructorAuthority {
    Record { application: SolvedSchemaApplication, origin: QualifiedNominalIdentity },
    Nominal(QualifiedNominalIdentity),
}

/// Spread projections refer to the checked source record; they never invent
/// expression identities for fields absent from the original syntax.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstructorValueSource {
    Expression(ExpressionIdentity),
    RecordField { record: ExpressionIdentity, field: Name },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ConstructorDefaultIdentity {
    pub owner: QualifiedNominalIdentity,
    pub field: Name,
}

#[derive(Clone, Debug)]
pub struct SolvedConstructorDefault {
    pub owner: QualifiedNominalIdentity,
    pub source: ExpressionIdentity,
    pub slot: usize,
    pub value: LiteralConstant,
}

#[derive(Clone, Debug)]
pub struct SolvedConstructorParameter {
    pub label: Option<Name>,
    pub ty: TypeId,
    pub default: Option<ConstructorDefaultIdentity>,
}

#[derive(Clone, Debug)]
pub struct SolvedConstructorArgument {
    pub value: ConstructorValueSource,
    pub actual: TypeId,
    pub slot: usize,
    pub assignability: usize,
    pub projection: Option<usize>,
}

/// Supplied values and omitted defaults bind to fixed destination slots. The
/// plan carries source ownership and type arguments after syntax is discarded.
#[derive(Clone, Debug)]
pub struct SolvedConstructorApplication {
    pub authority: ConstructorAuthority,
    pub result: TypeId,
    pub caller: Option<DeclarationIdentity>,
    pub parameters: Vec<SolvedConstructorParameter>,
    pub supplied: Vec<SolvedConstructorArgument>,
    pub default_slots: Vec<usize>,
    pub expectation: SolvedSchemaExpectation,
    /// Nominal members retain their selected immutable declaration candidate;
    /// records use their independently retained schema application instead.
    pub requirement: Option<RequirementId>,
}

#[derive(Clone, Debug)]
pub(super) struct PendingRecordConstructor {
    pub span: Span,
    pub expression: ExpressionIdentity,
    pub origin: TypeDefId,
    pub instance: SchemaInstance,
    pub arguments: Vec<ExpandedArgument>,
    pub caller: Option<DeclarationIdentity>,
    pub valid: bool,
}

fn charge_constructor_literal(graph: &mut InferenceContext, value: &LiteralConstant) -> Result<(), InferenceError> {
    let mut pending = vec![(value, 0)];
    while let Some((value, depth)) = pending.pop() {
        if depth > graph.limits().structural_depth { return Err(InferenceError::Limit("constructor default depth")); }
        graph.charge_source_fact_nodes(1)?;
        match value {
            LiteralConstant::List(values) | LiteralConstant::Tag { fields: values, .. } => {
                graph.charge_source_fact_edges(values.len() as u64)?;
                pending.extend(values.iter().map(|value| (value, depth + 1)));
            }
            LiteralConstant::Record(values) => {
                graph.charge_source_fact_edges(values.len() as u64)?;
                pending.extend(values.values().map(|value| (value, depth + 1)));
            }
            LiteralConstant::Map(values) => {
                graph.charge_source_fact_edges(2 * values.len() as u64)?;
                for key in values.keys() { let bytes = match key { crate::map_key::MapKey::Str(value) => value.len(), crate::map_key::MapKey::Bytes(value) | crate::map_key::MapKey::Path(value) => value.len(), _ => 0 }; graph.charge_source_fact_work(bytes as u64)?; }
                pending.extend(values.values().map(|value| (value, depth + 1)));
            }
            LiteralConstant::Str(value) | LiteralConstant::Path(value) => graph.charge_source_fact_work(value.len() as u64)?,
            LiteralConstant::Bytes(value) => graph.charge_source_fact_work(value.len() as u64)?,
            _ => {}
        }
    }
    Ok(())
}

fn nominal_constructor_identity(authority: QualifiedNominalIdentity) -> Name {
    Name::intern(&format!("{authority:?}"))
}

impl super::Checker {
    pub(super) fn record_graph_zero_field_tag(&mut self, arena: &ArenaProgram, expression: crate::syntax::arena::ExprId, ty: &super::Type) {
        let super::Type::Tag(family) = ty else { return; };
        use crate::syntax::arena::ArenaExprKind;
        let (key, member) = match arena.arena.expr(expression).kind {
            ArenaExprKind::Ident(member) if self.lookup(member).is_none() => (member, member),
            ArenaExprKind::Field { base, name } => match arena.arena.expr(base).kind { ArenaExprKind::Ident(namespace) if self.lookup(namespace).is_none() => (Name::intern(format!("{namespace}.{name}")), name), _ => return },
            _ => return,
        };
        if !self.tag_variants.get(&key).is_some_and(|info| info.type_name == *family && info.field_count == 0) { return; }
        self.record_graph_nominal_constructor(arena, self.expression_identity(arena, expression), ty, &[], &[], &[], &[], member, arena.arena.expr(expression).span);
    }

    pub(super) fn finish_constructor_expression_facts(&mut self, arena: &ArenaProgram) {
        let pending = std::mem::take(&mut self.pending_constructor_expressions);
        for (identity, (ty, caller)) in pending {
            let span = arena.arena.expr(identity.expression).span;
            let resolved = self.type_constraints.resolve(&ty).map_err(|_| InferenceError::Boundary("constructor source type cannot be resolved"))
                .and_then(|ty| if ty.contains_inference() { Err(InferenceError::Boundary("constructor source type remains unresolved")) } else { Ok(ty) });
            match resolved.and_then(|ty| self.graph_type(&ty, span).map(|graph| (ty, graph))) {
                Ok((ty, graph)) => {
                    let mut state = self.generic.borrow_mut();
                    let graph = if let Some(original) = state.facts.expressions.get(&identity).copied() {
                        let reason = state.facts.graph.reason(span, None);
                        let relation = reason.and_then(|reason| state.facts.graph.unify(original, graph, reason));
                        if let Err(error) = relation { drop(state); self.graph_error(span, error); continue; }
                        original
                    } else { graph };
                    state.facts.expressions.insert(identity, graph);
                    if let Some(owner) = caller { state.facts.expression_owners.insert(identity, owner); }
                    drop(state);
                    self.expr_types.insert(span, ty);
                }
                Err(error) => self.graph_error(span, error),
            }
        }
    }

    pub(super) fn record_graph_prepared_constructor(&mut self, arena: &ArenaProgram, expression: crate::syntax::arena::ExprId) {
        if !self.graph_generation { return; }
        let mut pending = vec![expression];
        let mut visited = BTreeSet::new();
        while let Some(expression) = pending.pop() {
            if !visited.insert(expression) { continue; }
            use crate::syntax::arena::{ArenaExprKind, ArenaRecordFieldKind};
            let kind = arena.arena.expr(expression).kind;
            let work = match kind { ArenaExprKind::List(values) => values.len(), ArenaExprKind::Record(fields) => fields.len(), ArenaExprKind::Call { args, .. } => args.len(), ArenaExprKind::Binary { .. } | ArenaExprKind::Index { .. } => 2, _ => 1 };
            let charged = self.generic.borrow_mut().facts.graph.charge_source_fact_work(1 + work as u64);
            if let Err(error) = charged { self.graph_error(arena.arena.expr(expression).span, error); return; }
            let mut children = Vec::new();
            match kind {
                ArenaExprKind::List(values) => children.extend(arena.arena.list_element_exprs(values)),
                ArenaExprKind::Record(fields) => for field in arena.arena.record_fields(fields) { match field.kind { ArenaRecordFieldKind::Named { value, .. } | ArenaRecordFieldKind::Path { value, .. } | ArenaRecordFieldKind::Spread { expr: value, .. } => children.push(value), _ => {} } },
                ArenaExprKind::Call { args, .. } => children.extend(arena.arena.call_args(args).iter().map(|argument| super::call_arg_expr_id_arena(&argument.kind))),
                ArenaExprKind::Binary { left, right, .. } => children.extend([left, right]),
                ArenaExprKind::Unary { expr, .. } => children.push(expr),
                ArenaExprKind::Field { base, .. } => children.push(base),
                ArenaExprKind::Index { base, index, .. } => children.extend([base, index]),
                _ => {}
            }
            pending.extend(children);
            let fact = self.prepared_constants.record_constructor_instances.get(&expression).cloned();
            let (callee, args) = match kind {
                ArenaExprKind::Call { callee, args } => (callee, Some(args)),
                ArenaExprKind::Ident(_) | ArenaExprKind::Field { .. } => (expression, None),
                _ => continue,
            };
            let (key, member) = match arena.arena.expr(callee).kind {
                ArenaExprKind::Ident(member) => (member, member),
                ArenaExprKind::Field { base, name } => match arena.arena.expr(base).kind { ArenaExprKind::Ident(namespace) => (Name::intern(format!("{namespace}.{name}")), name), _ => continue },
                _ => continue,
            };
            let nominal = self.tag_variants.get(&key).cloned().filter(|info| self.prepared_constants.types.get(&expression) == Some(&super::Type::Tag(info.type_name)));
            if fact.is_none() && nominal.is_none() { continue; }
            let arguments = crate::sema::arguments::expand_named_arguments(arena, args.map_or(&[], |args| arena.arena.call_args(args)), |expression| self.prepared_constants.types.get(&expression).cloned());
            let Ok(arguments) = arguments else { continue; };
            for argument in &arguments {
                let source = match argument.value { ArgumentValueSource::Expression(source) | ArgumentValueSource::RecordField { record: source, .. } | ArgumentValueSource::PositionalSplice(source) => source };
                if let Some(ty) = self.prepared_constants.types.get(&source).cloned() {
                    let identity = self.expression_identity(arena, source);
                    if !self.generic.borrow().facts.expressions.contains_key(&identity) {
                        match self.graph_type(&ty, arena.arena.expr(source).span) {
                            Ok(ty) => { let mut state = self.generic.borrow_mut(); state.facts.expressions.insert(identity, ty); if let Some(owner) = self.current_generic { state.facts.expression_owners.insert(identity, owner); } }
                            Err(error) => self.graph_error(arena.arena.expr(source).span, error),
                        }
                    }
                }
            }
            if let Some(fact) = fact {
                let Some(origin) = self.record_constructors.resolve_call(&arena.arena, callee, self.current_namespace) else { continue; };
                let source = PendingRecordConstructor { span: arena.arena.expr(expression).span, expression: self.expression_identity(arena, expression), origin, instance: fact.instance.clone(), arguments, caller: self.current_generic, valid: true };
                self.record_graph_record_constructor(arena, &source, &fact);
            } else if let Some(info) = nominal {
                let identity = self.expression_identity(arena, expression);
                self.record_graph_nominal_constructor(arena, identity, &super::Type::Tag(info.type_name), &vec![None; info.field_count], &info.field_types, &arguments, &(0..arguments.len()).collect::<Vec<_>>(), member, arena.arena.expr(expression).span);
            }
        }
    }

    fn constructor_arguments(&mut self, arena: &ArenaProgram, arguments: &[ExpandedArgument], slots: &[usize], parameters: &[SolvedConstructorParameter], caller: Option<DeclarationIdentity>) -> Result<Vec<SolvedConstructorArgument>, InferenceError> {
        if arguments.len() != slots.len() { return Err(InferenceError::InvalidScheme); }
        let mut supplied = Vec::with_capacity(arguments.len());
        for (argument, &slot) in arguments.iter().zip(slots) {
            let parameter = parameters.get(slot).ok_or(InferenceError::InvalidScheme)?;
            let (value, actual, projection) = match argument.value {
                ArgumentValueSource::Expression(expression) => {
                    let identity = self.expression_identity(arena, expression);
                    let existing = self.generic.borrow().facts.expressions.get(&identity).copied();
                    let actual = if let Some(actual) = existing { actual } else {
                        let actual = self.graph_type(&argument.ty, argument.span)?;
                        let mut state = self.generic.borrow_mut();
                        state.facts.expressions.insert(identity, actual);
                        if let Some(owner) = caller { state.facts.expression_owners.insert(identity, owner); }
                        actual
                    };
                    (ConstructorValueSource::Expression(identity), actual, None)
                }
                ArgumentValueSource::RecordField { record, field } => {
                    let identity = self.expression_identity(arena, record);
                    let receiver = *self.generic.borrow().facts.expressions.get(&identity).ok_or(InferenceError::Boundary("constructor spread has no original checked record"))?;
                    let mut state = self.generic.borrow_mut();
                    let graph = &mut state.facts.graph;
                    let reason = graph.reason(argument.span, None)?;
                    let projection = graph.constraint_origins().len();
                    let actual = graph.require_field(receiver, field, 0, reason)?;
                    (ConstructorValueSource::RecordField { record: identity, field }, actual, Some(projection))
                }
                ArgumentValueSource::PositionalSplice(_) => return Err(InferenceError::Boundary("constructor argument splice has no fixed destination")),
            };
            let mut state = self.generic.borrow_mut();
            let graph = &mut state.facts.graph;
            let reason = graph.reason(argument.span, None)?;
            let assignability = graph.constraint_origins().len();
            graph.assignable(parameter.ty, actual, reason)?;
            supplied.push(SolvedConstructorArgument { value, actual, slot, assignability, projection });
        }
        Ok(supplied)
    }

    pub(super) fn record_graph_record_constructor(&mut self, arena: &ArenaProgram, pending: &PendingRecordConstructor, fact: &crate::sema::constants::CheckedRecordConstructor) {
        if !self.graph_generation || self.generic.borrow().facts.constructor_applications.contains_key(&pending.expression) { return; }
        let outcome = (|| {
            let finished = self.graph_type(&fact.ty, pending.span)?;
            let original = self.generic.borrow().facts.expressions.get(&pending.expression).copied();
            let result = if let Some(original) = original {
                let mut state = self.generic.borrow_mut();
                let reason = state.facts.graph.reason(pending.span, None)?;
                state.facts.graph.unify(original, finished, reason)?;
                original
            } else { finished };
            let context = self.record_constructors.instance_expectation(&arena.arena, fact.instance.definition, &fact.instance.arguments).map_err(|_| InferenceError::InvalidScheme)?;
            let expectation = self.retain_schema_expectation(arena, &context, pending.span, 0, false)?;
            let application = expectation.applications.first().cloned().ok_or(InferenceError::InvalidScheme)?;
            let source = self.record_constructors.source_id(pending.origin).ok_or(InferenceError::InvalidScheme)?;
            let namespace = self.record_constructors.namespace(pending.origin).or(arena.root_nominal_namespace);
            let origin = QualifiedNominalIdentity::Source { source, namespace, declaration: super::NominalDeclaration::Type(pending.origin), member: None };
            let ArenaTypeDefBody::RecordSchema(fields_range) = arena.arena.type_def(pending.origin).body else { return Err(InferenceError::InvalidScheme); };
            let fields = arena.arena.schema_fields(fields_range);
            let defaults = self.record_constructors.defaults(pending.origin).cloned().unwrap_or_default();
            let row_fields = {
                let state = self.generic.borrow();
                let TypeNode::Record(row) = state.facts.graph.node(state.facts.graph.resolved(result)?)? else { return Err(InferenceError::InvalidScheme); };
                state.facts.graph.row_data(*row)?.fields.clone()
            };
            let mut parameters = Vec::with_capacity(row_fields.len());
            for (slot, field) in row_fields.into_iter().enumerate() {
                let declaration = fields.iter().find(|declaration| declaration.name == field.label).ok_or(InferenceError::InvalidScheme)?;
                let default = if let Some(expression) = declaration.default {
                    let value = defaults.get(&field.label).ok_or(InferenceError::InvalidScheme)?.clone();
                    let key = ConstructorDefaultIdentity { owner: origin, field: field.label };
                    let source = ExpressionIdentity { source: arena.arena.expr(expression).span.source_id, namespace, expression };
                    let mut state = self.generic.borrow_mut();
                    if !state.facts.constructor_defaults.contains_key(&key) {
                        charge_constructor_literal(&mut state.facts.graph, &value)?;
                        state.facts.constructor_defaults.insert(key, SolvedConstructorDefault { owner: origin, source, slot, value });
                    }
                    Some(key)
                } else { None };
                parameters.push(SolvedConstructorParameter { label: Some(field.label), ty: field.ty, default });
            }
            let slots = pending.arguments.iter().map(|argument| parameters.iter().position(|parameter| parameter.label == argument.name).ok_or(InferenceError::InvalidScheme)).collect::<Result<Vec<_>, _>>()?;
            let occupied = slots.iter().copied().collect::<BTreeSet<_>>();
            if occupied.len() != slots.len() { return Err(InferenceError::InvalidScheme); }
            let default_slots = (0..parameters.len()).filter(|slot| !occupied.contains(slot)).collect::<Vec<_>>();
            if default_slots.iter().any(|&slot| parameters[slot].default.is_none()) { return Err(InferenceError::InvalidScheme); }
            let supplied = self.constructor_arguments(arena, &pending.arguments, &slots, &parameters, pending.caller)?;
            let mut state = self.generic.borrow_mut();
            state.facts.graph.charge_source_fact_nodes(1 + parameters.len() as u64 + supplied.len() as u64)?;
            state.facts.graph.charge_source_fact_edges(parameters.len() as u64 + supplied.len() as u64 + default_slots.len() as u64)?;
            state.facts.expressions.insert(pending.expression, result);
            if let Some(owner) = pending.caller { state.facts.expression_owners.insert(pending.expression, owner); }
            state.facts.constructor_applications.insert(pending.expression, SolvedConstructorApplication { authority: ConstructorAuthority::Record { application, origin }, result, caller: pending.caller, parameters, supplied, default_slots, expectation, requirement: None });
            drop(state);
            self.record_expression_producer_flow(arena, pending.expression.expression, &self.graph_view(result));
            Ok::<_, InferenceError>(())
        })();
        if let Err(error) = outcome { self.graph_error(pending.span, error); }
    }

    pub(super) fn record_graph_nominal_constructor(&mut self, arena: &ArenaProgram, expression: ExpressionIdentity, result_type: &super::Type, labels: &[Option<Name>], field_types: &[super::Type], arguments: &[ExpandedArgument], slots: &[usize], member: Name, span: Span) {
        if !self.graph_generation || self.generic.borrow().facts.constructor_applications.contains_key(&expression) { return; }
        let outcome = (|| {
            let result = self.graph_type(result_type, span)?;
            let mut authority = *self.generic.borrow().facts.nominals.get(&result).ok_or(InferenceError::Boundary("nominal constructor has no source declaration"))?;
            if matches!(result_type, super::Type::Tag(_)) { match &mut authority { QualifiedNominalIdentity::Source { member: selected, .. } | QualifiedNominalIdentity::Builtin { member: selected, .. } => *selected = Some(member) } }
            match authority {
                QualifiedNominalIdentity::Source { declaration: super::NominalDeclaration::Type(definition), member: Some(member), .. } => {
                    let ArenaTypeDefBody::TagUnion(variants) = arena.arena.type_def(definition).body else { return Err(InferenceError::InvalidScheme); };
                    if !arena.arena.tag_variants(variants).iter().any(|variant| variant.name == member && variant.fields.len as usize == field_types.len()) { return Err(InferenceError::InvalidScheme); }
                }
                QualifiedNominalIdentity::Source { declaration: super::NominalDeclaration::Error(definition), member: Some(member), .. } => {
                    if !arena.arena.error_variants(arena.arena.error_def(definition).variants).iter().any(|variant| variant.name == member && variant.fields.len as usize == field_types.len()) { return Err(InferenceError::InvalidScheme); }
                }
                _ => return Err(InferenceError::InvalidScheme),
            }
            let mut parameters = Vec::with_capacity(labels.len());
            let retained = self.generic.borrow().facts.constructor_nominals.get(&authority).cloned();
            for (slot, (&label, ty)) in labels.iter().zip(field_types).enumerate() {
                let ty = if let Some(retained) = &retained {
                    let &(declared_label, declared) = retained.get(slot).ok_or(InferenceError::InvalidScheme)?;
                    if declared_label != label || self.generic.borrow().facts.graph.export_type(declared)? != *ty { return Err(InferenceError::InvalidScheme); }
                    declared
                } else { self.graph_type(ty, span)? };
                self.generic.borrow().facts.graph.export_type(ty)?;
                parameters.push(SolvedConstructorParameter { label, ty, default: None });
            }
            let declaration_parameters = parameters.iter().map(|parameter| (parameter.label, parameter.ty)).collect::<Vec<_>>();
            {
                let mut state = self.generic.borrow_mut();
                if let Some(previous) = state.facts.constructor_nominals.get(&authority) {
                    if previous.len() != declaration_parameters.len() || previous.iter().zip(&declaration_parameters).any(|((label, ty), (actual_label, actual))| label != actual_label || state.facts.graph.resolved(*ty).ok() != state.facts.graph.resolved(*actual).ok()) { return Err(InferenceError::InvalidScheme); }
                } else {
                    state.facts.graph.charge_source_fact_nodes(1)?;
                    state.facts.graph.charge_source_fact_edges(declaration_parameters.len() as u64)?;
                    state.facts.constructor_nominals.insert(authority, declaration_parameters);
                }
            }
            let supplied = self.constructor_arguments(arena, arguments, slots, &parameters, self.current_generic)?;
            let occupied = slots.iter().copied().collect::<BTreeSet<_>>();
            if occupied.len() != parameters.len() || occupied.len() != slots.len() { return Err(InferenceError::InvalidScheme); }
            let mut state = self.generic.borrow_mut();
            state.facts.graph.charge_source_fact_nodes(1 + parameters.len() as u64 + supplied.len() as u64)?;
            state.facts.graph.charge_source_fact_edges(parameters.len() as u64 + supplied.len() as u64)?;
            let authority_id = match authority { QualifiedNominalIdentity::Source { declaration: super::NominalDeclaration::Type(_), .. } => "language.constructor.tag", _ => "language.constructor.error_variant" };
            let signature = state.facts.graph.arrow(Arrow { kind: CallableKind::Pure, params: parameters.iter().enumerate().map(|(slot, parameter)| Parameter { label: parameter.label.unwrap_or_else(|| Name::intern(&format!("operand{slot}"))), ty: parameter.ty, defaulted: false, rest: false }).collect(), result, effects: EffectSummary::Closed(EffectSet::EMPTY) })?;
            let scheme = state.facts.graph.generalize(signature, 0, Generalization::Monomorphic, &[])?;
            let family = {
                let super::generic::GenericState { facts, language_operations, .. } = &mut *state;
                language_operations.declaration_family(&mut facts.graph, authority_id, nominal_constructor_identity(authority), scheme)?
            };
            let mut bound = vec![None; parameters.len()];
            for argument in &supplied { bound[argument.slot] = Some(argument.actual); }
            let reason = state.facts.graph.reason(span, None)?;
            let requirement = state.facts.graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, receiver: None, arguments: bound, result, effects: EffectSummary::Closed(EffectSet::EMPTY), effect_bindings: Vec::new(), output_effect_bindings: Vec::new(), declared_error_bound: None }, reason)?;
            state.facts.graph.solve()?;
            if let Some(caller) = self.current_generic { state.pending.get_mut(&caller).ok_or(InferenceError::InvalidScheme)?.requirements.push(requirement); }
            state.facts.expressions.insert(expression, result);
            if let Some(owner) = self.current_generic { state.facts.expression_owners.insert(expression, owner); }
            state.facts.constructor_applications.insert(expression, SolvedConstructorApplication { authority: ConstructorAuthority::Nominal(authority), result, caller: self.current_generic, parameters, supplied, default_slots: Vec::new(), expectation: SolvedSchemaExpectation::default(), requirement: Some(requirement) });
            Ok::<_, InferenceError>(())
        })();
        if let Err(error) = outcome { self.graph_error(span, error); }
    }
}

impl<Graph> super::SolvedTypes<Graph> {
    pub(super) fn constructor_application_requirement_roots(&self) -> Result<Vec<ScopedRequirementRoot>, InferenceError> {
        self.constructor_applications.iter().filter_map(|(identity, plan)| plan.requirement.map(|requirement| (*identity, plan, requirement)))
            .map(|(identity, plan, requirement)| self.expression_scope(identity, plan.caller).map(|scope| ScopedRequirementRoot { requirement, scope })).collect()
    }

    pub(super) fn constructor_application_payload_bytes(&self) -> usize {
        let mut bytes = self.constructor_nominals.values().map(|parameters| parameters.capacity() * std::mem::size_of::<(Option<Name>, TypeId)>()).sum::<usize>();
        let mut contexts = Vec::new();
        for plan in self.constructor_applications.values() {
            bytes += plan.parameters.capacity() * std::mem::size_of::<SolvedConstructorParameter>()
                + plan.supplied.capacity() * std::mem::size_of::<SolvedConstructorArgument>()
                + plan.default_slots.capacity() * std::mem::size_of::<usize>();
            if let ConstructorAuthority::Record { application, .. } = &plan.authority { bytes += application.arguments.capacity() * std::mem::size_of::<TypeId>(); }
            contexts.push(&plan.expectation);
        }
        while let Some(context) = contexts.pop() {
            bytes += context.applications.capacity() * std::mem::size_of::<SolvedSchemaApplication>()
                + context.children.len() * (std::mem::size_of::<crate::sema::constants::SchemaComponent>() + std::mem::size_of::<SolvedSchemaExpectation>() + 3 * std::mem::size_of::<usize>());
            bytes += context.applications.iter().map(|application| application.arguments.capacity() * std::mem::size_of::<TypeId>()).sum::<usize>();
            contexts.extend(context.children.values());
        }
        let mut allocations = BTreeSet::new();
        let mut literals = self.constructor_defaults.values().map(|default| &default.value).collect::<Vec<_>>();
        while let Some(literal) = literals.pop() {
            use std::sync::Arc;
            match literal {
                LiteralConstant::Str(value) | LiteralConstant::Path(value) => { if allocations.insert(Arc::as_ptr(value) as *const u8 as usize) { bytes += value.len() + 2 * std::mem::size_of::<usize>(); } }
                LiteralConstant::Bytes(value) => { if allocations.insert(Arc::as_ptr(value) as *const u8 as usize) { bytes += value.len() + 2 * std::mem::size_of::<usize>(); } }
                LiteralConstant::List(values) | LiteralConstant::Tag { fields: values, .. } => {
                    if allocations.insert(Arc::as_ptr(values) as usize) {
                        bytes += std::mem::size_of::<Vec<LiteralConstant>>() + values.capacity() * std::mem::size_of::<LiteralConstant>() + 2 * std::mem::size_of::<usize>();
                        literals.extend(values.iter());
                    }
                }
                LiteralConstant::Record(values) => {
                    if allocations.insert(Arc::as_ptr(values) as usize) {
                        bytes += std::mem::size_of::<BTreeMap<Name, LiteralConstant>>() + values.len() * (std::mem::size_of::<Name>() + std::mem::size_of::<LiteralConstant>() + 3 * std::mem::size_of::<usize>()) + 2 * std::mem::size_of::<usize>();
                        literals.extend(values.values());
                    }
                }
                LiteralConstant::Map(values) => {
                    if allocations.insert(Arc::as_ptr(values) as usize) {
                        bytes += std::mem::size_of::<BTreeMap<crate::map_key::MapKey, LiteralConstant>>() + values.len() * (std::mem::size_of::<crate::map_key::MapKey>() + std::mem::size_of::<LiteralConstant>() + 3 * std::mem::size_of::<usize>()) + 2 * std::mem::size_of::<usize>();
                        literals.extend(values.values());
                        for key in values.keys() { match key {
                            crate::map_key::MapKey::Str(value) => { if allocations.insert(Arc::as_ptr(value) as *const u8 as usize) { bytes += value.len() + 2 * std::mem::size_of::<usize>(); } }
                            crate::map_key::MapKey::Bytes(value) | crate::map_key::MapKey::Path(value) => { if allocations.insert(Arc::as_ptr(value) as *const u8 as usize) { bytes += value.len() + 2 * std::mem::size_of::<usize>(); } }
                            _ => {}
                        } }
                    }
                }
                _ => {}
            }
        }
        bytes
    }

    pub(super) fn constructor_application_roots(&self, graph: &InferenceContext) -> Result<Vec<ScopedRoot>, InferenceError> {
        let mut roots = Vec::new();
        for parameters in self.constructor_nominals.values() { roots.extend(parameters.iter().map(|&(_, ty)| ScopedRoot { ty, scope: None })); }
        for (identity, plan) in &self.constructor_applications {
            let scope = self.expression_scope(*identity, plan.caller)?;
            roots.push(ScopedRoot { ty: plan.result, scope });
            roots.extend(plan.parameters.iter().map(|parameter| ScopedRoot { ty: parameter.ty, scope }));
            for argument in &plan.supplied {
                let source = match argument.value { ConstructorValueSource::Expression(source) | ConstructorValueSource::RecordField { record: source, .. } => source };
                roots.push(ScopedRoot { ty: argument.actual, scope: self.expression_scope(source, plan.caller)? });
            }
            if let ConstructorAuthority::Record { application, .. } = &plan.authority { roots.extend(application.arguments.iter().map(|&ty| ScopedRoot { ty, scope })); }
            let mut contexts = vec![(&plan.expectation, 0)];
            while let Some((context, depth)) = contexts.pop() {
                if depth > graph.limits().structural_depth { return Err(InferenceError::Limit("constructor context depth")); }
                for application in &context.applications { roots.extend(application.arguments.iter().map(|&ty| ScopedRoot { ty, scope })); }
                contexts.extend(context.children.values().map(|context| (context, depth + 1)));
            }
        }
        Ok(roots)
    }

    pub(super) fn validate_constructor_applications(&self, graph: &InferenceContext) -> Result<(), InferenceError> {
        for (key, default) in &self.constructor_defaults {
            let QualifiedNominalIdentity::Source { source, namespace, declaration: super::NominalDeclaration::Type(_), member: None } = key.owner else { return Err(InferenceError::InvalidScheme); };
            if default.owner != key.owner || default.source.source != source || default.source.namespace != namespace { return Err(InferenceError::InvalidScheme); }
        }
        for (identity, plan) in &self.constructor_applications {
            self.expression_scope(*identity, plan.caller)?;
            if self.expression_owners.get(identity).copied() != plan.caller { return Err(InferenceError::Boundary("constructor result lost its source callable owner")); }
            if graph.resolved(*self.expressions.get(identity).ok_or(InferenceError::InvalidScheme)?)? != graph.resolved(plan.result)? { return Err(InferenceError::Boundary("constructor result lost its original source endpoint")); }
            let mut occupied = BTreeSet::new();
            for supplied in &plan.supplied {
                let parameter = plan.parameters.get(supplied.slot).ok_or(InferenceError::InvalidScheme)?;
                if !occupied.insert(supplied.slot) { return Err(InferenceError::InvalidScheme); }
                let source = match supplied.value { ConstructorValueSource::Expression(source) | ConstructorValueSource::RecordField { record: source, .. } => source };
                if source.source != identity.source || source.namespace != identity.namespace { return Err(InferenceError::InvalidScheme); }
                self.expression_scope(source, plan.caller)?;
                let checked = *self.expressions.get(&source).ok_or(InferenceError::InvalidScheme)?;
                match (supplied.value, supplied.projection) {
                    (ConstructorValueSource::Expression(_), None) => { if graph.resolved(checked)? != graph.resolved(supplied.actual)? { return Err(InferenceError::Boundary("constructor argument lost its original source endpoint")); } }
                    (ConstructorValueSource::RecordField { field, .. }, Some(origin)) => {
                        let origin = graph.constraint_origins().get(origin).ok_or(InferenceError::InvalidScheme)?;
                        let ConstraintRelation::Projection { record, label, result } = origin.relation else { return Err(InferenceError::InvalidScheme); };
                        if label != field || graph.resolved(record)? != graph.resolved(checked)? || graph.resolved(result)? != graph.resolved(supplied.actual)? { return Err(InferenceError::InvalidScheme); }
                    }
                    _ => return Err(InferenceError::InvalidScheme),
                }
                let origin = graph.constraint_origins().get(supplied.assignability).ok_or(InferenceError::InvalidScheme)?;
                let ConstraintRelation::Assignable { expected, actual } = origin.relation else { return Err(InferenceError::InvalidScheme); };
                if graph.resolved(expected)? != graph.resolved(parameter.ty)? || graph.resolved(actual)? != graph.resolved(supplied.actual)? { return Err(InferenceError::Boundary("constructor assignment endpoints differ from its fixed slot")); }
            }
            for &slot in &plan.default_slots {
                if !occupied.insert(slot) { return Err(InferenceError::InvalidScheme); }
                let parameter = plan.parameters.get(slot).ok_or(InferenceError::InvalidScheme)?;
                let key = parameter.default.ok_or(InferenceError::InvalidScheme)?;
                let default = self.constructor_defaults.get(&key).ok_or(InferenceError::InvalidScheme)?;
                if parameter.label != Some(key.field) || default.slot != slot { return Err(InferenceError::InvalidScheme); }
            }
            if occupied.len() != plan.parameters.len() { return Err(InferenceError::InvalidScheme); }
            match &plan.authority {
                ConstructorAuthority::Record { application, origin } => {
                    if plan.requirement.is_some() { return Err(InferenceError::InvalidScheme); }
                    if plan.expectation.applications.first() != Some(application) || !plan.expectation.applications.iter().any(|application| application.declaration == *origin) { return Err(InferenceError::Boundary("constructor application lost its declaration owner")); }
                    let TypeNode::Record(row) = graph.node(graph.resolved(plan.result)?)? else { return Err(InferenceError::InvalidScheme); };
                    let fields = &graph.row_data(*row)?.fields;
                    if fields.len() != plan.parameters.len() { return Err(InferenceError::InvalidScheme); }
                    for (slot, (field, parameter)) in fields.iter().zip(&plan.parameters).enumerate() {
                        if parameter.label != Some(field.label) || graph.resolved(parameter.ty)? != graph.resolved(field.ty)? { return Err(InferenceError::Boundary("constructor formal slot differs from its result field")); }
                        if let Some(key) = parameter.default {
                            let default = self.constructor_defaults.get(&key).ok_or(InferenceError::InvalidScheme)?;
                            if key.owner != *origin || key.field != field.label || default.slot != slot { return Err(InferenceError::InvalidScheme); }
                        }
                    }
                }
                ConstructorAuthority::Nominal(authority) => {
                    let requirement = plan.requirement.ok_or(InferenceError::InvalidScheme)?;
                    let crate::sema::inference::RequirementTemplate::Operation { call, .. } = graph.requirement_template(requirement)? else { return Err(InferenceError::InvalidScheme); };
                    let evidence = graph.candidate_evidence(requirement)?.ok_or(InferenceError::InvalidScheme)?;
                    let super::SolvedOperationAuthority::Language(metadata) = self.operation_catalog.candidate(graph, evidence.candidate)? else { return Err(InferenceError::InvalidScheme); };
                    let authority_id = match authority { QualifiedNominalIdentity::Source { declaration: super::NominalDeclaration::Type(_), .. } => "language.constructor.tag", _ => "language.constructor.error_variant" };
                    let crate::sema::operation_graph::PreparedLanguageOperation::Declaration { authority: selected_authority, identity: selected_identity } = metadata.operation else { return Err(InferenceError::InvalidScheme); };
                    if selected_authority != authority_id || &*selected_identity.as_str() != format!("{authority:?}") || metadata.authority != authority_id { return Err(InferenceError::InvalidScheme); }
                    let call = graph.operation_call(call)?;
                    if graph.resolved(call.result)? != graph.resolved(plan.result)? || call.receiver.is_some() || call.arguments.len() != plan.parameters.len() || call.effects != EffectSummary::Closed(EffectSet::EMPTY) { return Err(InferenceError::InvalidScheme); }
                    for supplied in &plan.supplied { if graph.resolved(call.arguments[supplied.slot].ok_or(InferenceError::InvalidScheme)?)? != graph.resolved(supplied.actual)? { return Err(InferenceError::InvalidScheme); } }
                    let declared = self.constructor_nominals.get(authority).ok_or(InferenceError::InvalidScheme)?;
                    if declared.len() != plan.parameters.len() || declared.iter().zip(&plan.parameters).any(|((label, ty), parameter)| *label != parameter.label || graph.resolved(*ty).ok() != graph.resolved(parameter.ty).ok()) { return Err(InferenceError::InvalidScheme); }
                    let nominal = *self.nominals.get(&graph.resolved(plan.result)?).ok_or(InferenceError::InvalidScheme)?;
                    let matches = match (nominal, *authority) {
                        (QualifiedNominalIdentity::Source { source, namespace, declaration, member }, QualifiedNominalIdentity::Source { source: actual_source, namespace: actual_namespace, declaration: actual_declaration, member: actual_member }) => source == actual_source && namespace == actual_namespace && declaration == actual_declaration && actual_member.is_some() && (member.is_none() || member == actual_member),
                        (QualifiedNominalIdentity::Builtin { family, member }, QualifiedNominalIdentity::Builtin { family: actual_family, member: actual_member }) => family == actual_family && member == actual_member,
                        _ => false,
                    };
                    if !matches || !plan.default_slots.is_empty() || plan.parameters.iter().any(|parameter| parameter.default.is_some()) { return Err(InferenceError::InvalidScheme); }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::query::{BinderKind, NormalizedConstructorAuthority, NormalizedConstructorValueSource, NormalizedLiteral, NormalizedRequirement, NormalizedShape, SolvedQuery};
    use crate::sema::check::Checker;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    #[test]
    fn source_constructor_application_retains_default_plan_after_syntax_drop() {
        let source = "type Box[T] = {value: T, items: List[T] = []}\npure make(value: Int) -> Box[Int] { Box(value: value) }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(38), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.constructor_applications.len(), 1, "each checked constructor retains an original-source application and default binding");
        let (identity, plan) = checked.solved.constructor_applications.iter().next().unwrap();
        assert_eq!(identity.source, SourceId::new(38));
        assert!(plan.caller.is_some());
        assert_eq!(plan.supplied.len(), 1);
        assert_eq!(plan.default_slots.len(), 1);
        assert!(plan.parameters[plan.default_slots[0]].default.is_some());
        assert!(matches!(plan.authority, ConstructorAuthority::Record { .. }));
        drop(parsed);
        checked.solved.validate().unwrap();
    }

    #[test]
    fn source_constructor_application_keeps_generic_binders_phantoms_and_caller_order() {
        let declarations = "type Box[T] = {value: T, items: List[T] = []}\ntype Marker[T] = {name: Str}\npure make(value) { Box(value: value) }\npure forwarded(value) { make(value) }\npure marker() -> Marker[UInt] { Marker(name: \"kept\") }\n";
        for calls in ["let count = forwarded(7)\nlet text = forwarded(\"word\")\n", "let text = forwarded(\"word\")\nlet count = forwarded(7)\n"] {
            let source = format!("{declarations}{calls}");
            let parsed = Parser::parse_source_arena_only(SourceId::new(39), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            assert_eq!(checked.solved.constructor_applications.len(), 2);
            let generic = checked.solved.constructor_applications.values().find(|plan| !plan.default_slots.is_empty()).unwrap();
            let ConstructorAuthority::Record { application, .. } = &generic.authority else { panic!("applied record authority") };
            assert_eq!(application.arguments.len(), 1);
            assert!(checked.solved.graph.export_type(application.arguments[0]).is_err(), "the definition retains its binder rather than either caller's ground type");
            let scope = checked.solved.declarations[&generic.caller.unwrap()].scheme;
            assert!(matches!(checked.solved.graph.node(checked.solved.graph.resolved(application.arguments[0]).unwrap()).unwrap(), TypeNode::Rigid { scope: actual, .. } if *actual == scope));
            let phantom = checked.solved.constructor_applications.values().find(|plan| plan.default_slots.is_empty()).unwrap();
            let ConstructorAuthority::Record { application, .. } = &phantom.authority else { panic!("phantom application authority") };
            assert_eq!(checked.solved.graph.export_type(application.arguments[0]).unwrap(), super::super::Type::UInt);
            drop(parsed);
            checked.solved.validate().unwrap();
            let counters = checked.solved.graph.counters().clone();
            let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
            for (&identity, plan) in &checked.solved.constructor_applications {
                let answer = query.constructor_application(identity).unwrap();
                assert_eq!(answer.source.source, identity.source);
                assert_eq!(answer.source.expression, identity.expression);
                assert_eq!(answer.default_slots, plan.default_slots);
                let NormalizedConstructorAuthority::Record { application, .. } = &answer.authority else { panic!("the consumer retains applied record authority") };
                if plan.default_slots.is_empty() {
                    assert_eq!(application.arguments[0].ty.as_ref(), &NormalizedShape::Atom("UInt".to_string()));
                } else {
                    assert_eq!(application.arguments[0].ty.as_ref(), &NormalizedShape::Binder { index: 0, kind: BinderKind::Type });
                    assert_eq!(application.arguments[0].scope.as_ref().unwrap().quantifiers.len(), 1);
                    assert_eq!(answer.supplied[0].actual.ty, application.arguments[0].ty);
                    assert!(matches!(&answer.parameters[answer.default_slots[0]].default.as_ref().unwrap().value, NormalizedLiteral::List(values) if values.is_empty()));
                }
            }
            assert_eq!(checked.solved.graph.counters(), &counters);
        }
    }

    #[test]
    fn source_constructor_application_finishes_shared_null_empty_and_nested_fields_before_publication() {
        let source = "type Inner[T] = {value: T?}\ntype Shared[T] = {inner: Inner[T], values: List[T], anchor: T}\npure make(value) { Shared(inner: Inner(value: null), values: [], anchor: value) }\npure forwarded(value) { make(value) }\nlet count = forwarded(7)\nlet text = forwarded(\"word\")\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(46), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.constructor_applications.len(), 2);
        for plan in checked.solved.constructor_applications.values() {
            let ConstructorAuthority::Record { application, .. } = &plan.authority else { panic!("shared generic application") };
            assert!(matches!(checked.solved.graph.node(checked.solved.graph.resolved(application.arguments[0]).unwrap()).unwrap(), TypeNode::Rigid { .. }));
        }
        drop(parsed);
        checked.solved.validate().unwrap();
    }

    #[test]
    fn source_constructor_application_publishes_prepared_nested_calls_and_shared_defaults() {
        let source = "type Inner[T] = {value: T, items: List[T] = []}\ntype Outer[T] = {inner: Inner[T], label: Str = \"kept\"}\nlet first = Outer(inner: Inner(value: 7))\nlet second = Outer(inner: Inner(value: \"word\"))\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(40), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.constructor_applications.len(), 4);
        assert_eq!(checked.solved.constructor_defaults.len(), 2, "default preparation is shared by declaration, independently of specialization");
        assert!(checked.solved.constructor_applications.values().all(|plan| plan.caller.is_none() && plan.default_slots.len() == 1));
        drop(parsed);
        checked.solved.validate().unwrap();
    }

    #[test]
    fn source_constructor_application_retains_original_spread_fields_and_directional_proofs() {
        let source = "type Pair[T] = {left: T, right: T, items: List[T] = []}\npure make(left: Int, right: Int) -> Pair[Int] { Pair(...{left: left}, right: right) }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(41), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.constructor_applications.len(), 1);
        let plan = checked.solved.constructor_applications.values().next().unwrap();
        assert_eq!(plan.supplied.len(), 2);
        let spread = plan.supplied.iter().find(|argument| argument.projection.is_some()).unwrap();
        let ConstructorValueSource::RecordField { record, field } = spread.value else { panic!("the original spread record is retained") };
        assert!(matches!(parsed.arena.arena.expr(record.expression).kind, crate::syntax::arena::ArenaExprKind::Record(_)));
        assert_eq!(field.as_str(), "left");
        assert_eq!(plan.parameters[spread.slot].label, Some(field));
        let relation = checked.solved.graph.constraint_origins()[spread.assignability].relation;
        assert!(matches!(relation, ConstraintRelation::Assignable { expected, actual } if expected == plan.parameters[spread.slot].ty && actual == spread.actual));
        drop(parsed);
        checked.solved.validate().unwrap();
        let counters = checked.solved.graph.counters().clone();
        let (&identity, plan) = checked.solved.constructor_applications.iter().next().unwrap();
        let answer = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner()).constructor_application(identity).unwrap();
        let spread = answer.supplied.iter().find(|argument| argument.proof.projection.is_some()).unwrap();
        assert!(matches!(&spread.source, NormalizedConstructorValueSource::RecordField { record, field } if record.source == SourceId::new(41) && field == "left"));
        assert_eq!(spread.actual.ty.as_ref(), &NormalizedShape::Atom("Int".to_string()));
        assert_eq!(answer.parameters[spread.slot].label.as_deref(), Some("left"));
        assert_eq!(spread.proof.owner, checked.solved.owner);
        assert!(plan.supplied.iter().any(|argument| argument.assignability == spread.proof.assignability && argument.projection == spread.proof.projection));
        assert_eq!(answer.default_slots, plan.default_slots);
        assert_eq!(checked.solved.graph.counters(), &counters);
    }

    #[test]
    fn source_constructor_application_retains_nominal_payload_members_and_prepared_values() {
        let source = "enum Choice { Empty, Value(Int) }\nerror Failure = Failed(code: Int, message: Str) : InvalidData\npure selected(value: Int) -> Choice { Value(value) }\npure failed(code: Int) -> Failure { Failure.Failed(...{code: code}, message: \"failed\") }\nlet literal = Value(7)\nlet empty = Empty\nlet nested = [Value(8)]\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(43), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let mut checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.constructor_applications.len(), 5, "{:?}", checked.solved.constructor_applications.keys().map(|identity| { let span = parsed.arena.arena.expr(identity.expression).span; &source[span.start()..span.end()] }).collect::<Vec<_>>());
        assert_eq!(checked.solved.constructor_nominals.len(), 3);
        for plan in checked.solved.constructor_applications.values() {
            let ConstructorAuthority::Nominal(authority) = plan.authority else { panic!("nominal constructor authority") };
            assert!(matches!(authority, QualifiedNominalIdentity::Source { source, member: Some(_), .. } if source == SourceId::new(43)));
            assert_eq!(checked.solved.constructor_nominals[&authority].len(), plan.parameters.len());
        }
        drop(parsed);
        checked.solved.validate().unwrap();
        let counters = checked.solved.graph.counters().clone();
        let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
        for (&identity, plan) in &checked.solved.constructor_applications {
            let answer = query.constructor_application(identity).unwrap();
            assert!(matches!(&answer.authority, NormalizedConstructorAuthority::Nominal(_)));
            assert!(matches!(&answer.requirement, Some(NormalizedRequirement::Operation { .. })));
            assert_eq!(answer.parameters.len(), plan.parameters.len());
            assert_eq!(answer.supplied.len(), plan.supplied.len());
            assert!(answer.default_slots.is_empty());
        }
        assert_eq!(checked.solved.graph.counters(), &counters);
        let solved = std::sync::Arc::get_mut(&mut checked.solved).unwrap();
        let identity = *solved.constructor_applications.iter().find(|(_, plan)| plan.parameters.len() == 1).unwrap().0;
        let original = solved.constructor_applications[&identity].authority.clone();
        let empty = solved.symbol_owner().with_current(|| Name::intern("Empty"));
        let ConstructorAuthority::Nominal(QualifiedNominalIdentity::Source { member, .. }) = &mut solved.constructor_applications.get_mut(&identity).unwrap().authority else { unreachable!() };
        *member = Some(empty);
        assert!(solved.validate().is_err(), "a different member cannot reuse this payload binding");
        solved.constructor_applications.get_mut(&identity).unwrap().authority = original;
        solved.validate().unwrap();
    }

    #[test]
    fn frozen_constructor_application_rejects_equal_signature_member_substitution() {
        let source = "enum Choice { First(Int), Second(Int) }\npure first(value: Int) -> Choice { First(value) }\npure second(value: Int) -> Choice { Second(value) }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(49), source);
        let mut checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.constructor_applications.len(), 2);
        drop(parsed);
        checked.solved.validate().unwrap();
        let solved = std::sync::Arc::get_mut(&mut checked.solved).unwrap();
        let (first, second) = solved.symbol_owner().with_current(|| (Name::intern("First"), Name::intern("Second")));
        let plan = solved.constructor_applications.values_mut().find(|plan| matches!(plan.authority, ConstructorAuthority::Nominal(QualifiedNominalIdentity::Source { member: Some(member), .. }) if member == first)).unwrap();
        let ConstructorAuthority::Nominal(QualifiedNominalIdentity::Source { member, .. }) = &mut plan.authority else { unreachable!() };
        *member = Some(second);
        assert!(solved.validate().is_err(), "a different same-signature member cannot reuse this source constructor proof");
    }

    #[test]
    fn source_constructor_application_preserves_imported_alias_and_default_owner() {
        use crate::syntax::arena::ArenaProgramBuilder;
        let source = "use model as m\nconst DEFAULT = \"root\"\npure made(value: UInt) -> m.Public[UInt] { m.Public(value: value) }\n";
        let module_source = "##! Constructor declarations with private default owners.\nconst DEFAULT = \"module\"\ntype Private[T] = {value: T, label: Str = DEFAULT}\n## A public applied alias.\nexport type Public[T] = Private[T]\n";
        let mut builder = ArenaProgramBuilder::with_token_capacity(128);
        let entry = Parser::parse_source_into_arena_builder(SourceId::new(44), source, &mut builder);
        let module = Parser::parse_source_into_arena_builder(SourceId::new(45), module_source, &mut builder);
        assert!(entry.diagnostics.is_empty() && module.diagnostics.is_empty());
        let namespace = builder.symbol_owner().with_current(|| Name::intern("constructor-model"));
        for statement in builder.statement_ids(entry.statements) { if let Some((import, _, _)) = builder.use_stmt_for_statement(statement) { builder.set_use_resolved(import, std::sync::Arc::from("constructor-model")); } }
        builder.push_arena_module("constructor-model".to_string(), namespace, module.statements);
        let program = builder.finish_with_statements(entry.statements);
        let checked = Checker::check_arena(&program, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.constructor_applications.len(), 1);
        let plan = checked.solved.constructor_applications.values().next().unwrap();
        let ConstructorAuthority::Record { application, origin } = &plan.authority else { panic!("alias and origin remain separate") };
        assert_ne!(application.declaration, *origin);
        assert_eq!(checked.solved.graph.export_type(application.arguments[0]).unwrap(), super::super::Type::UInt);
        for owner in [application.declaration, *origin] { assert!(matches!(owner, QualifiedNominalIdentity::Source { source, namespace: Some(owner), .. } if source == SourceId::new(45) && owner == namespace)); }
        let default = &checked.solved.constructor_defaults[&plan.parameters[plan.default_slots[0]].default.unwrap()];
        assert_eq!(default.owner, *origin);
        assert_eq!(default.source.source, SourceId::new(45));
        assert_eq!(default.value, LiteralConstant::Str(std::sync::Arc::from("module")));
        drop(program);
        checked.solved.validate().unwrap();
    }

    #[test]
    fn source_constructor_application_rejects_shared_contradictions_and_unanchored_parameters() {
        for source in [
            "type Pair[T] = {left: T, right: T}\npure make(value) { Pair(left: value, right: 1) }\nlet rejected = make(\"wrong\")\n",
            "type Pair[T] = {left: T, right: T}\npure make() { Pair(left: 1, right: 2.0) }\n",
            "type Box[T] = {value: T?}\npure make() { Box(value: null) }\n",
            "type Box[T] = {values: List[T]}\npure make() { Box(values: []) }\n",
            "type Marker[T] = {name: Str}\npure make() { Marker(name: \"missing\") }\n",
            "type Box[T] = {value: T = 1}\npure make() -> Box[Str] { Box() }\n",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(47), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(!checked.diagnostics.is_empty(), "unsupported specialization was accepted: {source}");
            drop(parsed);
            checked.solved.validate().unwrap();
        }
        let source = "type Pair[T] = {left: T, right: T}\npure make(value) { Pair(left: value, right: 1) }\nlet accepted = make(7)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(47), source);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        drop(parsed);
        checked.solved.validate().unwrap();
    }

    #[test]
    fn retained_constructor_plan_capacities_are_accounted() {
        let source = "type Box[T] = {value: T, items: List[T] = []}\npure make(value: Int) -> Box[Int] { Box(value: value) }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(48), source);
        let mut checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        drop(parsed);
        let solved = std::sync::Arc::get_mut(&mut checked.solved).unwrap();
        macro_rules! reserve_plan_capacity {
            ($field:ident, $ty:ty) => {{
                let before = solved.retained_source_bytes();
                let values = &mut solved.constructor_applications.values_mut().next().unwrap().$field;
                let capacity = values.capacity();
                values.reserve_exact(32);
                let added = values.capacity() - capacity;
                assert_eq!(solved.retained_source_bytes() - before, added * std::mem::size_of::<$ty>());
            }};
        }
        reserve_plan_capacity!(parameters, SolvedConstructorParameter);
        reserve_plan_capacity!(supplied, SolvedConstructorArgument);
        reserve_plan_capacity!(default_slots, usize);
        let before = solved.retained_source_bytes();
        let ConstructorAuthority::Record { application, .. } = &mut solved.constructor_applications.values_mut().next().unwrap().authority else { unreachable!() };
        let capacity = application.arguments.capacity();
        application.arguments.reserve_exact(32);
        let added = application.arguments.capacity() - capacity;
        assert_eq!(solved.retained_source_bytes() - before, added * std::mem::size_of::<TypeId>());
        solved.validate().unwrap();
    }

    #[test]
    fn source_tag_constructor_keeps_distinct_positional_payload_slots() {
        let source = "enum Event { Made(Int, Str) }\npure make() -> Event { Made(1, \"ready\") }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(106), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let plan = checked.solved.constructor_applications.values().next().unwrap();
        assert_eq!(plan.parameters.len(), 2);
        assert!(plan.parameters.iter().all(|parameter| parameter.label.is_none()));
        assert_eq!(plan.supplied.iter().map(|argument| argument.slot).collect::<Vec<_>>(), [0, 1]);
        let crate::sema::inference::RequirementTemplate::Operation { call, .. } = checked.solved.graph.requirement_template(plan.requirement.unwrap()).unwrap() else { panic!("tag payload slots keep their canonical member proof") };
        assert_eq!(checked.solved.graph.operation_call(call).unwrap().binding, crate::sema::inference::OperationBinding::Slots);
        drop(parsed);
        checked.solved.validate().unwrap();
        let invalid = source.replace("Made(1, \"ready\")", "Made(1, false)");
        let parsed = Parser::parse_source_arena_only(SourceId::new(106), &invalid);
        let rejected = Checker::check_arena(&parsed.arena, &invalid);
        assert!(rejected.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "{:?}", rejected.diagnostics);
    }

    #[test]
    fn source_constructor_forwarding_keeps_nominal_members_and_reached_argument_permissions() {
        let permissions = EffectSet(EffectSet::TIME.0 | EffectSet::ENV.0);
        for (declaration, constructor, result, authority) in [
            ("enum Event { Made(Int, Int) }", "Made(clock(), setting())", "Event", "language.constructor.tag"),
            ("error Failure = Made(left: Int, right: Int)", "Failure.Made(left: clock(), right: setting())", "Failure", "language.constructor.error_variant"),
        ] {
            for calls in ["let first = forwarded()\nlet second = construct()\n", "let second = construct()\nlet first = forwarded()\n"] {
                let source = format!("{declaration}\nproc clock() [time] -> Int {{ let _ = time.now(); 7 }}\nproc setting() [env] -> Int {{ let _ = env.get(\"SETTING\"); 8 }}\nproc construct() [time, env] -> {result} {{ {constructor} }}\nproc forwarded() [time, env] -> {result} {{ construct() }}\n{calls}");
                let parsed = Parser::parse_source_arena_only(SourceId::new(104), &source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let checked = Checker::check_arena(&parsed.arena, &source);
                assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
                let owners = ["construct", "forwarded"].map(|name| *checked.solved.declarations.keys().find(|identity| parsed.arena.arena.function_def(identity.declaration).name == name).unwrap());
                for owner in owners {
                    let declaration = &checked.solved.declarations[&owner];
                    assert_eq!(declaration.required_effects, EffectSummary::Closed(permissions));
                    assert_eq!(declaration.effective_effects, EffectSummary::Closed(permissions));
                    assert!(checked.solved.graph.scheme(declaration.scheme).unwrap().quantifiers.is_empty(), "explicit nominal payload contracts do not become generalized operation domains");
                }
                let (&identity, plan) = checked.solved.constructor_applications.iter().find(|(_, plan)| plan.caller == Some(owners[0])).unwrap();
                assert_eq!(&source[parsed.arena.arena.expr(identity.expression).span.range()], constructor);
                assert_eq!(plan.supplied.len(), 2);
                for argument in &plan.supplied {
                    let ConstructorValueSource::Expression(original) = argument.value else { panic!("each effectful payload retains its original expression") };
                    assert!(matches!(&source[parsed.arena.arena.expr(original.expression).span.range()], "clock()" | "setting()"));
                    assert_eq!(checked.solved.graph.resolved(argument.actual).unwrap(), checked.solved.graph.resolved(checked.solved.expressions[&original]).unwrap());
                }
                let requirement = plan.requirement.unwrap();
                let crate::sema::inference::RequirementTemplate::Operation { call, .. } = checked.solved.graph.requirement_template(requirement).unwrap() else { panic!("nominal construction retains its canonical member") };
                assert_eq!(checked.solved.graph.operation_call(call).unwrap().effects, EffectSummary::Closed(EffectSet::EMPTY));
                let candidate = checked.solved.graph.candidate_evidence(requirement).unwrap().unwrap().candidate;
                let super::super::SolvedOperationAuthority::Language(metadata) = checked.solved.operation_catalog.candidate(&checked.solved.graph, candidate).unwrap() else { panic!("language constructor authority") };
                assert_eq!(metadata.authority, authority);
                assert!(checked.solved.calls.values().any(|call| call.caller == Some(owners[1]) && call.declaration == Some(owners[0])));
                drop(parsed);
                checked.solved.validate().unwrap();
                let counters = checked.solved.graph.counters().clone();
                let answer = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner()).constructor_application(identity).unwrap();
                assert!(matches!(answer.authority, NormalizedConstructorAuthority::Nominal(_)));
                assert_eq!(answer.supplied.len(), 2);
                assert!(answer.default_slots.is_empty());
                assert_eq!(checked.solved.graph.counters(), &counters);
                for bound in ["time", "env"] {
                    for name in ["construct", "forwarded"] {
                        let invalid = source.replace(&format!("proc {name}() [time, env]"), &format!("proc {name}() [{bound}]"));
                        let parsed = Parser::parse_source_arena_only(SourceId::new(104), &invalid);
                        let rejected = Checker::check_arena(&parsed.arena, &invalid);
                        assert!(rejected.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{invalid}: {:?}", rejected.diagnostics);
                    }
                }
                let invalid = source.replace(constructor, &constructor.replace("clock()", "true"));
                let parsed = Parser::parse_source_arena_only(SourceId::new(104), &invalid);
                let rejected = Checker::check_arena(&parsed.arena, &invalid);
                assert!(rejected.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "{:?}", rejected.diagnostics);
            }
        }
    }

    #[test]
    fn source_record_constructor_defaults_add_no_permissions_and_keep_reached_arguments() {
        let permissions = EffectSet(EffectSet::TIME.0 | EffectSet::ENV.0);
        for calls in ["let first = forwarded()\nlet second = unchanged()\n", "let second = unchanged()\nlet first = forwarded()\n"] {
            let source = format!("const LABEL = \"definition\"\ntype Settings = {{count: Int, label: Str = LABEL, values: List[Int] = []}}\nproc clock() [time] -> Int {{ let _ = time.now(); 7 }}\nproc setting() [env] -> Int {{ let _ = env.get(\"SETTING\"); 8 }}\npure unchanged() -> Settings {{ Settings(count: 0) }}\nproc construct() [time, env] -> Settings {{ Settings(count: clock() + setting()) }}\nproc forwarded() [time, env] -> Settings {{ construct() }}\n{calls}");
            let parsed = Parser::parse_source_arena_only(SourceId::new(105), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            for (name, expected) in [("unchanged", EffectSet::EMPTY), ("construct", permissions), ("forwarded", permissions)] {
                let declaration = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == name).unwrap().1;
                assert_eq!(declaration.required_effects, EffectSummary::Closed(expected));
                assert_eq!(declaration.effective_effects, EffectSummary::Closed(expected));
            }
            assert_eq!(checked.solved.constructor_defaults.len(), 2);
            assert_eq!(checked.solved.constructor_applications.len(), 2);
            for plan in checked.solved.constructor_applications.values() {
                assert_eq!(plan.supplied.len(), 1);
                assert_eq!(plan.default_slots.len(), 2);
                let ConstructorAuthority::Record { origin, .. } = plan.authority else { panic!("record default owner") };
                for &slot in &plan.default_slots {
                    let key = plan.parameters[slot].default.unwrap();
                    let default = &checked.solved.constructor_defaults[&key];
                    assert_eq!(default.owner, origin);
                    assert_eq!(default.source.source, SourceId::new(105));
                    assert_eq!(default.slot, slot);
                    assert!(matches!(&default.value, LiteralConstant::Str(value) if value.as_ref() == "definition") || matches!(&default.value, LiteralConstant::List(values) if values.is_empty()));
                }
            }
            drop(parsed);
            checked.solved.validate().unwrap();
            let counters = checked.solved.graph.counters().clone();
            let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
            for &identity in checked.solved.constructor_applications.keys() {
                let answer = query.constructor_application(identity).unwrap();
                assert_eq!(answer.default_slots.len(), 2);
                assert_eq!(answer.supplied.len(), 1);
                for &slot in &answer.default_slots {
                    let default = answer.parameters[slot].default.as_ref().unwrap();
                    assert_eq!(default.source.source, SourceId::new(105));
                    assert_eq!(default.slot, slot);
                }
            }
            assert_eq!(checked.solved.graph.counters(), &counters);
            for bound in ["time", "env"] {
                let invalid = source.replace("proc construct() [time, env]", &format!("proc construct() [{bound}]"));
                let parsed = Parser::parse_source_arena_only(SourceId::new(105), &invalid);
                let rejected = Checker::check_arena(&parsed.arena, &invalid);
                assert!(rejected.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation")), "{:?}", rejected.diagnostics);
            }
            let invalid = source.replace("label: Str = LABEL", "label: Str = env.get(\"SETTING\")");
            let parsed = Parser::parse_source_arena_only(SourceId::new(105), &invalid);
            let rejected = Checker::check_arena(&parsed.arena, &invalid);
            assert!(rejected.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.record-default")), "effectful expressions cannot acquire a constant default plan: {:?}", rejected.diagnostics);
        }
    }

    #[test]
    fn frozen_constructor_application_rejects_same_bundle_phantom_application_substitution() {
        let source = "type Marker[T] = {name: Str}\npure count() -> Marker[Int] { Marker(name: \"count\") }\npure text() -> Marker[Str] { Marker(name: \"text\") }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(53), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        drop(parsed);
        let mut solved = std::sync::Arc::try_unwrap(checked.solved).unwrap();
        solved.validate().unwrap();
        let applications = solved.constructor_applications.iter().map(|(&identity, plan)| {
            let ConstructorAuthority::Record { application, .. } = &plan.authority else { panic!("record application") };
            (identity, plan.result, application.arguments[0])
        }).collect::<Vec<_>>();
        assert_eq!(applications.len(), 2);
        assert_eq!(solved.graph.export_type(applications[0].1).unwrap(), solved.graph.export_type(applications[1].1).unwrap());
        assert_ne!(solved.graph.export_type(applications[0].2).unwrap(), solved.graph.export_type(applications[1].2).unwrap());
        let original = solved.constructor_applications[&applications[0].0].clone();
        let plan = solved.constructor_applications.get_mut(&applications[0].0).unwrap();
        let ConstructorAuthority::Record { application, .. } = &mut plan.authority else { unreachable!() };
        application.arguments[0] = applications[1].2;
        plan.expectation.applications[0].arguments[0] = applications[1].2;
        assert!(solved.validate().is_err(), "a constructor's phantom application is fixed by its original source contract");
        solved.constructor_applications.insert(applications[0].0, original.clone());
        solved.constructor_applications.remove(&applications[0].0);
        assert!(solved.validate().is_err(), "removing a constructor plan cannot remove its published source application");
        solved.constructor_applications.insert(applications[0].0, original);
        solved.validate().unwrap();
    }

    #[test]
    fn frozen_constructor_application_rejects_changed_slot_default_and_argument_proof() {
        let source = "type Row = {count: Int, enabled: Bool = false}\npure make(count: Int) -> Row { Row(count: count) }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(42), source);
        let mut checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        drop(parsed);
        let solved = std::sync::Arc::get_mut(&mut checked.solved).unwrap();
        let identity = *solved.constructor_applications.keys().next().unwrap();
        let original = solved.constructor_applications[&identity].clone();
        let default_slot = original.default_slots[0];
        solved.constructor_applications.get_mut(&identity).unwrap().supplied[0].slot = default_slot;
        assert!(solved.validate().is_err());
        solved.constructor_applications.insert(identity, original.clone());
        solved.constructor_applications.get_mut(&identity).unwrap().default_slots.clear();
        assert!(solved.validate().is_err());
        solved.constructor_applications.insert(identity, original.clone());
        solved.constructor_applications.get_mut(&identity).unwrap().supplied[0].assignability = usize::MAX;
        assert!(solved.validate().is_err());
        solved.constructor_applications.insert(identity, original);
        let key = solved.constructor_applications[&identity].parameters[default_slot].default.unwrap();
        let default = solved.constructor_defaults.remove(&key).unwrap();
        assert!(solved.validate().is_err(), "an omitted slot needs its exact declaration-owned preparation");
        solved.constructor_defaults.insert(key, default);
        solved.validate().unwrap();
    }
}
