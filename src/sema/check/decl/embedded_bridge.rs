use super::*;
use crate::modules::signature::RuntimeOp;
use crate::sema::check::{DeclarationIdentity, ExpressionIdentity, ProducerFlowKind, ProducerFlowSource, ReturnElaboration, SolvedCallable, SolvedCall, SolvedArgumentSource, SolvedTypes};
use crate::sema::inference::{CallableKind, ComponentMember, EffectSet, EffectSummary, Generalization, GeneralizationRoots, InferenceError, SchemeId, TypeId};
use crate::source::SourceId;
use crate::syntax::arena::{ArenaExprKind, ArenaExprOrRun, BlockId};

/// A catalog bridge owns a native implementation and an exact source signature.
/// Its placeholder body is retained only to identify the original declaration;
/// it supplies no checked expression, statement, or completion authority.
#[derive(Clone, Debug)]
pub(crate) struct CanonicalNativeBridge {
    declaration: DeclarationIdentity,
    catalog_identity: &'static str,
    function: &'static str,
    op: RuntimeOp,
    body: BlockId,
    signature: TypeId,
    scheme: SchemeId,
    graph_owner: crate::sema::inference::GraphOwner,
}

impl CanonicalNativeBridge {
    pub(crate) fn declaration(&self) -> DeclarationIdentity { self.declaration }
    pub(crate) fn catalog_identity(&self) -> &'static str { self.catalog_identity }
    pub(crate) fn function(&self) -> &'static str { self.function }
    pub(crate) fn op(&self) -> RuntimeOp { self.op }
    pub(crate) fn body(&self) -> BlockId { self.body }
    pub(crate) fn signature(&self) -> TypeId { self.signature }
    pub(crate) fn scheme(&self) -> SchemeId { self.scheme }
    pub(crate) fn graph_owner(&self) -> crate::sema::inference::GraphOwner { self.graph_owner }
    pub(crate) fn retained_bytes(&self) -> usize { std::mem::size_of::<Self>() }
}

/// The original checked invocation binds its declaration, caller, supplied
/// operand and source recipe before mutable consumer maps can be changed.
#[derive(Clone, Debug)]
pub(crate) struct NativeBridgeInvocation {
    declaration: Arc<CanonicalNativeBridge>,
    origin: ExpressionIdentity,
    call: SolvedCall,
    recipes: Box<[SolvedArgumentSource]>,
    result: TypeId,
}

impl NativeBridgeInvocation {
    pub(crate) fn declaration(&self) -> &Arc<CanonicalNativeBridge> { &self.declaration }
    pub(crate) fn origin(&self) -> ExpressionIdentity { self.origin }
    pub(crate) fn call(&self) -> &SolvedCall { &self.call }
    pub(crate) fn recipes(&self) -> &[SolvedArgumentSource] { &self.recipes }
    pub(crate) fn result(&self) -> TypeId { self.result }
    pub(crate) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        let profiles = self.call.argument_producers.iter().chain(std::iter::once(&self.call.result_producers)).map(|profile| {
            profile.len() * size_of::<(crate::sema::check::ProducerPath, crate::sema::check::ProducerEffects)>()
                + profile.keys().map(|path| path.0.capacity() * size_of::<crate::sema::check::ProducerPathComponent>()).sum::<usize>()
        }).sum::<usize>();
        size_of::<Self>() + self.recipes.len() * size_of::<SolvedArgumentSource>()
            + self.call.requirements.capacity() * size_of::<crate::sema::inference::RequirementId>()
            + self.call.requirement_origins.capacity() * size_of::<(crate::sema::inference::RequirementId, crate::sema::inference::RequirementId)>()
            + (self.call.substitutions.capacity() + self.call.actual_arguments.capacity()) * size_of::<TypeId>()
            + self.call.effect_substitutions.capacity() * size_of::<EffectSummary>()
            + self.call.argument_producers.capacity() * size_of::<crate::sema::check::ProducerProfile>()
            + (self.call.binding.supplied_slots.capacity() + self.call.binding.default_slots.capacity()) * size_of::<usize>() + profiles
    }
    pub(crate) fn matches_call(&self, call: &SolvedCall) -> bool {
        self.call.signature == call.signature && self.call.declaration == call.declaration && self.call.caller == call.caller
            && self.call.requirements == call.requirements && self.call.requirement_origins == call.requirement_origins
            && self.call.substitutions == call.substitutions && self.call.effect_substitutions == call.effect_substitutions
            && self.call.actual_arguments == call.actual_arguments && self.call.argument_producers == call.argument_producers
            && self.call.result_producers == call.result_producers && self.call.result_producer_flow == call.result_producer_flow
            && self.call.binding.supplied_slots == call.binding.supplied_slots && self.call.binding.default_slots == call.binding.default_slots
            && self.call.binding.rest_slot == call.binding.rest_slot && self.call.binding.dynamic.is_none() == call.binding.dynamic.is_none()
    }
}

impl SolvedTypes<crate::sema::inference::InferenceContext> {
    pub(in crate::sema::check) fn seal_embedded_bridge_calls(&mut self) -> Result<(), InferenceError> {
        for (&origin, call) in &self.calls {
            let Some(declaration) = call.declaration.and_then(|identity| self.embedded_bridge(identity)).cloned() else { continue; };
            let recipes = self.argument_sources.get(&origin).ok_or(InferenceError::InvalidScheme)?;
            let result = *self.expressions.get(&origin).ok_or(InferenceError::InvalidScheme)?;
            if call.caller.is_none_or(|owner| owner.namespace != declaration.declaration.namespace || owner.source != declaration.declaration.source)
                || origin.namespace != declaration.declaration.namespace || origin.source != declaration.declaration.source || recipes.len() != 1 || call.actual_arguments.len() != 1
                || call.binding.supplied_slots != [0] || !call.binding.default_slots.is_empty() || call.binding.rest_slot.is_some() || call.binding.dynamic.is_some()
                || !call.requirements.is_empty() || !call.substitutions.is_empty() || !call.effect_substitutions.is_empty() { return Err(InferenceError::InvalidScheme); }
            self.graph.charge_source_fact_nodes(1)?;
            self.graph.charge_source_fact_edges(4)?;
            self.graph.charge_source_fact_work(2)?;
            self.embedded_bridge_calls.insert(origin, Arc::new(NativeBridgeInvocation { declaration, origin, call: call.clone(), recipes: recipes.clone().into_boxed_slice(), result }));
        }
        Ok(())
    }
}

impl SolvedTypes {
    pub(crate) fn validate_embedded_bridges(&self) -> Result<(), InferenceError> {
        use crate::sema::inference::{ScopedRoot, TypeNode};
        for (&identity, bridge) in &self.embedded_bridges {
            let declaration = self.declarations.get(&identity).ok_or(InferenceError::InvalidScheme)?;
            let catalog = crate::stdlib::find(bridge.catalog_identity).ok_or(InferenceError::InvalidScheme)?;
            if identity != bridge.declaration || bridge.graph_owner != self.owner || identity.namespace.is_none_or(|namespace| crate::stdlib::find_by_namespace(&namespace.as_str()).is_none_or(|module| !std::ptr::eq(module, catalog)))
                || crate::stdlib::bridge_op(catalog, bridge.function) != Some(bridge.op) || bridge.op != RuntimeOp::BridgeTypeName
                || declaration.signature != bridge.signature || declaration.scheme != bridge.scheme || declaration.body != bridge.body
                || declaration.kind != CallableKind::Pure || declaration.return_elaboration != ReturnElaboration::Value
                || declaration.effective_effects != EffectSummary::Closed(EffectSet::EMPTY) || declaration.required_effects != declaration.effective_effects
                || !declaration.source_requirements.is_empty() || !declaration.return_producers.is_empty() { return Err(InferenceError::InvalidScheme); }
            self.graph.validate_scoped(ScopedRoot { ty: bridge.signature, scope: Some(bridge.scheme) })?;
            let TypeNode::Arrow(arrow) = self.graph.node(self.graph.resolved(bridge.signature)?)? else { return Err(InferenceError::InvalidScheme); };
            if arrow.kind != CallableKind::Pure || arrow.effects != EffectSummary::Closed(EffectSet::EMPTY) || arrow.params.len() != 1
                || arrow.params[0].label != "value" || arrow.params[0].defaulted || arrow.params[0].rest
                || self.graph.export_type(arrow.params[0].ty)? != Type::Any || self.graph.export_type(arrow.result)? != Type::Str { return Err(InferenceError::InvalidScheme); }
            if self.expression_owners.values().any(|owner| *owner == identity) || self.statement_owners.values().any(|owner| *owner == identity) { return Err(InferenceError::InvalidScheme); }
        }
        for (&origin, original) in &self.embedded_bridge_calls {
            let call = self.calls.get(&origin).ok_or(InferenceError::InvalidScheme)?;
            if original.origin != origin || !original.matches_call(call) || self.argument_sources.get(&origin).map(Vec::as_slice) != Some(original.recipes())
                || self.expressions.get(&origin) != Some(&original.result) || self.expression_owners.get(&origin).copied() != call.caller
                || self.embedded_bridge(original.declaration.declaration).is_none_or(|declaration| !Arc::ptr_eq(declaration, &original.declaration)) { return Err(InferenceError::InvalidScheme); }
        }
        for (origin, call) in &self.calls {
            if call.declaration.is_some_and(|identity| self.embedded_bridges.contains_key(&identity)) && !self.embedded_bridge_calls.contains_key(origin) { return Err(InferenceError::InvalidScheme); }
        }
        Ok(())
    }
}

#[derive(Debug, Eq, PartialEq)]
struct BridgeShape {
    declaration: Span,
    body: Span,
    parameter: Span,
    parameter_type: Span,
    result_type: Span,
    returned: Span,
    list: Span,
    value: Span,
    index: Span,
}

/// Recognize the exact native type-name declaration, including original source
/// positions. A same-named authored function does not acquire native authority.
fn bridge_shape(arena: &ArenaProgram, statement: StmtId, source: SourceId) -> Option<(FunctionDefId, BridgeShape)> {
    let outer = arena.arena.stmt(statement);
    let ArenaStmtKind::Export(inner) = outer.kind else { return None; };
    let inner = arena.arena.stmt(inner);
    let ArenaStmtKind::PureDef(id) = inner.kind else { return None; };
    let def = arena.arena.function_def(id);
    if def.name != "type_name" || def.test_declaration || def.return_ty_defaulted || def.effects.is_some()
        || !arena.arena.type_expr_named(def.return_ty, "Str") { return None; }
    let [parameter] = arena.arena.params(def.params) else { return None; };
    if parameter.name != "value" || parameter.ty_defaulted || parameter.default.is_some() || parameter.rest
        || !arena.arena.type_expr_named(parameter.ty, "Any") { return None; }
    let body = arena.arena.block(def.body);
    let statements: Vec<_> = arena.arena.stmt_ids(body.statements).collect();
    let [statement] = statements.as_slice() else { return None; };
    let ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(returned))) = arena.arena.stmt(*statement).kind else { return None; };
    let ArenaExprKind::Index { base, index, guarded: false } = arena.arena.expr(returned).kind else { return None; };
    let ArenaExprKind::List(elements) = arena.arena.expr(base).kind else { return None; };
    let elements: Vec<_> = arena.arena.list_elements(elements).collect();
    let [element] = elements.as_slice() else { return None; };
    if element.splice_span.is_some() || !matches!(arena.arena.expr(element.value).kind, ArenaExprKind::Ident(name) if name == parameter.name) { return None; }
    let ArenaExprKind::Int(integer) = arena.arena.expr(index).kind else { return None; };
    if arena.arena.int_literal(integer).value() != Some(1) { return None; }
    let shape = BridgeShape {
        declaration: outer.span, body: arena.arena.span(body.span), parameter: arena.arena.span(parameter.span),
        parameter_type: arena.arena.type_expr_span(parameter.ty), result_type: arena.arena.type_expr_span(def.return_ty),
        returned: arena.arena.expr(returned).span, list: arena.arena.expr(base).span,
        value: arena.arena.expr(element.value).span, index: arena.arena.expr(index).span,
    };
    let spans = [shape.declaration, shape.body, shape.parameter, shape.parameter_type, shape.result_type, shape.returned, shape.list, shape.value, shape.index];
    spans.iter().all(|span| span.source_id == source).then_some((id, shape))
}

#[cfg(test)]
fn canonical_shape(module: &'static crate::stdlib::StdlibModule, source: SourceId) -> Option<BridgeShape> {
    let parsed = crate::syntax::parser::Parser::parse_source_arena_only(source, module.source);
    if !parsed.diagnostics.is_empty() { return None; }
    parsed.arena.symbol_owner().with_current(|| {
        let mut shapes = parsed.arena.statement_ids().filter_map(|statement| bridge_shape(&parsed.arena, statement, source).map(|(_, shape)| shape));
        let shape = shapes.next()?;
        shapes.next().is_none().then_some(shape)
    })
}

impl Checker {
    pub(super) fn register_embedded_bridge(&mut self, arena: &ArenaProgram, id: FunctionDefId, kind: CallableKind) {
        if !self.graph_generation { return; }
        let Some(namespace) = self.current_namespace else { return; };
        let Some(module) = arena.modules.iter().find(|module| module.name == namespace) else { return; };
        let Some((catalog, source)) = module.canonical_stdlib_source() else { return; };
        if !module.internal || crate::stdlib::find_by_namespace(&namespace.as_str()).is_none_or(|selected| !std::ptr::eq(selected, catalog)) { return; }
        let def = arena.arena.function_def(id);
        let Some(original_function) = module.canonical_bridge_function(id) else { return; };
        let Some(op) = crate::stdlib::bridge_op(catalog, &original_function.as_str()) else { return; };
        let candidate = arena.module_statements(module).find_map(|statement| bridge_shape(arena, statement, source));
        let identity = DeclarationIdentity { source, namespace: Some(namespace), declaration: id };
        if self.generic.borrow().completed.contains(&identity) || self.generic.borrow().rejected.contains(&identity) { return; }
        let shape = match candidate {
            Some((original_id, shape)) if original_id == id && kind == CallableKind::Pure && op == RuntimeOp::BridgeTypeName && module.canonical_bridge_declaration_matches(&arena.arena, id) => shape,
            _ => {
                self.generic.borrow_mut().rejected.insert(identity);
                self.error(arena.arena.span(arena.arena.block(def.body).span), "catalog native bridge declaration differs from its original callable and placeholder contract", "check.native-bridge-contract");
                return;
            }
        };
        let result = (|| {
            let mut state = self.generic.borrow_mut();
            let pending = state.pending.get(&identity).ok_or(InferenceError::InvalidScheme)?;
            let signature = pending.signature;
            let effects = EffectSummary::Closed(EffectSet::EMPTY);
            if pending.kind != CallableKind::Pure || pending.required_effects != effects
                || !pending.requirements.is_empty() || pending.enclosing_owner.is_some() { return Err(InferenceError::InvalidScheme); }
            let parameters = pending.parameter_producer_flows.clone();
            let mut schemes = state.facts.graph.generalize_component_with_roots(&[ComponentMember {
                root: signature, requirements: Vec::new(), policy: Generalization::Allowed,
            }], 0, None, &[GeneralizationRoots { captured_types: Vec::new(), effects: vec![effects] }])?;
            let scheme = schemes.pop().ok_or(InferenceError::InvalidScheme)?;
            let facts = &mut state.facts;
            let flow = facts.producer_flows.push(&mut facts.graph, ProducerFlowSource::DeclarationResult(identity), ProducerFlowKind::Known(BTreeMap::new()))?;
            facts.graph.charge_source_fact_nodes(1)?;
            facts.graph.charge_source_fact_edges(3)?;
            facts.graph.charge_source_fact_work(1)?;
            facts.embedded_bridges.insert(identity, Arc::new(CanonicalNativeBridge {
                declaration: identity, catalog_identity: catalog.identity, function: "type_name", op, body: def.body, signature, scheme, graph_owner: facts.graph.owner(),
            }));
            facts.declarations.insert(identity, SolvedCallable {
                source_requirements: Vec::new(), scheme, signature, body: def.body, kind,
                return_elaboration: ReturnElaboration::Value, effective_effects: effects, required_effects: effects,
                parameter_producers: vec![BTreeMap::new()], return_producers: BTreeMap::new(),
                parameter_producer_flows: parameters, return_producer_flow: Some(flow),
            });
            let pending = state.pending.get_mut(&identity).unwrap();
            pending.return_elaboration = Some(ReturnElaboration::Value);
            pending.return_producer_flow = Some(flow);
            if let Some(inputs) = state.producer_inputs.declarations.get_mut(&identity) { inputs.result = Some(flow); }
            state.generated.insert(identity);
            state.completed.insert(identity);
            Ok::<_, InferenceError>(())
        })();
        match result {
            Ok(()) => {
                self.function_return_types.insert(shape.body, Type::Str);
                self.publish_parameter_types(arena, def);
            }
            Err(error) => self.graph_error(shape.body, error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn internal_fixture(source: &str, namespace: &str) -> ArenaProgram {
        let source_id = SourceId::new(1);
        let mut builder = crate::syntax::arena::ArenaProgramBuilder::with_token_capacity(256);
        let entry = crate::syntax::parser::Parser::parse_source_into_arena_builder(SourceId::new(0), "", &mut builder);
        let module = crate::syntax::parser::Parser::parse_source_into_arena_builder(source_id, source, &mut builder);
        assert!(entry.diagnostics.is_empty() && module.diagnostics.is_empty());
        let name = builder.name(namespace);
        builder.push_internal_arena_module("<xsh-stdlib:json>".to_owned(), name, module.statements);
        builder.finish_with_statements(entry.statements)
    }

    #[test]
    fn embedded_bridge_requires_catalog_ingress_and_retains_ordinary_any_return_checks() {
        crate::runtime::eval::run_eval(|| {
            let canonical = crate::stdlib::find("json").unwrap();
            for namespace in ["<xsh-stdlib:json>", "json", "user-json"] {
                let program = internal_fixture(canonical.source, namespace);
                let checked = Checker::check_arena(&program, "");
                assert!(checked.solved.embedded_bridges.is_empty(), "module spelling cannot mint a catalog receipt");
                assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.dynamic-boundary")), "ordinary unchecked Any returns require validation: {:?}", checked.diagnostics);
            }
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(SourceId::new(0), "pure type_name(value: Any) -> Str { return [value][1] }\n");
            assert!(parsed.diagnostics.is_empty());
            let checked = Checker::check_arena(&parsed.arena, "");
            assert!(checked.solved.embedded_bridges.is_empty());
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.dynamic-boundary")));
        });
    }

    #[test]
    fn embedded_bridge_shape_requires_original_kind_labels_types_and_placeholder() {
        crate::runtime::eval::run_eval(|| {
            let catalog = crate::stdlib::find("json").unwrap();
            let source_id = SourceId::new(1);
            let expected = canonical_shape(catalog, source_id).expect("catalog declares one native type-name bridge");
            for (before, after) in [
                ("export pure type_name", "export proc type_name"),
                ("type_name(value: Any)", "type_name(other: Any)"),
                ("type_name(value: Any)", "type_name(value: Str)"),
                ("type_name(value: Any)", "type_name(value: Any = null)"),
                ("-> Str", "-> Any"),
                ("[value][1]", "[value][0]"),
                ("[value][1]", "[null ][1]"),
            ] {
                let changed = catalog.source.replacen(before, after, 1);
                assert_ne!(changed, catalog.source);
                let parsed = crate::syntax::parser::Parser::parse_source_arena_only(source_id, &changed);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                parsed.arena.symbol_owner().with_current(|| {
                    let changed_shape = parsed.arena.statement_ids().find_map(|statement| bridge_shape(&parsed.arena, statement, source_id).map(|(_, shape)| shape));
                    assert_ne!(changed_shape.as_ref(), Some(&expected), "changed header or body cannot establish catalog authority");
                });
            }
        });
    }

    #[test]
    fn embedded_bridge_catalog_signature_has_no_placeholder_body_facts() {
        crate::runtime::eval::run_eval(|| {
            let (_, parsed) = crate::loader::parse_load_entry_source_arena_only("catalog-bridge.xsh", crate::loader::entry_source_from_text("catalog-bridge.xsh", "use json\nlet encoded = json.encode_lines([1])\n".to_owned()), Vec::new());
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_compact_declarations(&parsed.arena);
            let receipt = checked.solved.embedded_bridges.values().next().expect("catalog source establishes a native bridge receipt");
            assert_eq!(receipt.catalog_identity(), "json");
            assert_eq!(receipt.op(), RuntimeOp::BridgeTypeName);
            let declaration = &checked.solved.declarations[&receipt.declaration()];
            assert_eq!(declaration.kind, CallableKind::Pure);
            assert_eq!(declaration.signature, receipt.signature());
            assert_eq!(declaration.scheme, receipt.scheme());
            assert_eq!(declaration.body, receipt.body());
            assert_eq!(declaration.effective_effects, EffectSummary::Closed(EffectSet::EMPTY));
            let graph = &checked.solved.graph;
            let crate::sema::inference::TypeNode::Arrow(arrow) = graph.node(graph.resolved(declaration.signature).unwrap()).unwrap() else { panic!("bridge retains its original callable signature"); };
            assert_eq!(arrow.kind, CallableKind::Pure);
            assert_eq!(arrow.effects, EffectSummary::Closed(EffectSet::EMPTY));
            assert_eq!(arrow.params.len(), 1);
            assert_eq!(checked.solved.symbol_owner().with_current(|| arrow.params[0].label.as_str().to_string()), "value");
            assert!(!arrow.params[0].defaulted && !arrow.params[0].rest);
            assert_eq!(graph.export_type(arrow.params[0].ty).unwrap(), Type::Any);
            assert_eq!(graph.export_type(arrow.result).unwrap(), Type::Str);
            assert!(!checked.solved.expression_owners.values().any(|owner| *owner == receipt.declaration()));
            assert!(!checked.solved.statement_owners.values().any(|owner| *owner == receipt.declaration()));
            assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.dynamic-boundary")), "native placeholder is not an authored Any return: {:?}", checked.diagnostics);
            parsed.arena.symbol_owner().with_current(|| {
                let (statement, target, expression) = parsed.arena.statement_ids().find_map(|id| match parsed.arena.arena.stmt(id).kind {
                    ArenaStmtKind::Let { target, initializer: ArenaExprOrRun::Expr(expression), .. } => Some((id, target, expression)),
                    _ => None,
                }).unwrap();
                let source = parsed.arena.arena.stmt(statement).span.source_id;
                let origin = ExpressionIdentity { source, namespace: None, expression };
                let operation = checked.solved.operations.get(&origin).expect("public catalog call retains its original operation");
                assert!(operation.receiver.is_none(), "public catalog function uses its module-call binder");
                let selected = graph.candidate_evidence(operation.requirement).unwrap().expect("public catalog call retains its original selected candidate");
                let crate::sema::check::SolvedOperationAuthority::Registry(metadata) = checked.solved.operation_catalog.candidate(graph, selected.candidate).unwrap() else { panic!("public catalog call retains registry authority"); };
                assert_eq!(metadata.binding, crate::modules::signature::ImplBinding::Script(crate::modules::signature::ScriptImpl { module: "json", function: "encode_lines" }));
                assert_eq!(operation.binding.supplied_slots, [0]);
                let recipes = checked.solved.argument_sources.get(&origin).expect("public script call retains its original argument recipe");
                assert_eq!(recipes.len(), 1);
                let crate::sema::arguments::ArgumentValueSource::Expression(operand) = recipes[0].value else { panic!("public script fixture has its authored list operand"); };
                let operand = ExpressionIdentity { expression: operand, ..origin };
                assert!(checked.solved.expressions.contains_key(&operand), "public script list operand retains its original checked type");
                let binding = crate::sema::check::BindingIdentity { source, namespace: None, target };
                let binding = checked.solved.bindings.get(&binding).expect("public script result retains its original entry binding");
                assert!(binding.owner.is_none());
                graph.validate_scoped(crate::sema::inference::ScopedRoot { ty: binding.ty, scope: binding.scheme }).expect("public script entry binding has valid original storage scope");
                assert!(matches!(graph.node(graph.resolved(binding.ty).unwrap()).unwrap(), crate::sema::inference::TypeNode::Result(_, _)), "public script entry binding has its declared Result storage");
                let function = crate::symbol::QualifiedName::new(Name::intern(crate::stdlib::namespace_text("json")), Name::intern("encode_lines"));
                assert!(checked.qualified_pures.contains_key(&function), "public script call has its checked canonical implementation");
            });
        });
    }

    #[test]
    fn embedded_bridge_ingress_refuses_rewritten_original_header_kind_and_body() {
        crate::runtime::eval::run_eval(|| {
            let (_, parsed) = crate::loader::parse_load_entry_source_arena_only("bridge-ingress-controls.xsh", crate::loader::entry_source_from_text("bridge-ingress-controls.xsh", "use json\nlet encoded = json.encode_lines([1])\n".to_owned()), Vec::new());
            let program = parsed.arena;
            program.symbol_owner().with_current(|| {
                let module = program.modules.iter().find(|module| module.canonical_stdlib_source().is_some_and(|(catalog, _)| catalog.identity == "json")).unwrap();
                let (_, source) = module.canonical_stdlib_source().unwrap();
                let (definition, _) = program.module_statements(module).find_map(|statement| bridge_shape(&program, statement, source)).unwrap();
                let original = program.arena.function_def(definition);
                let parameter_index = original.params.start as usize;
                for control in 0..10 {
                    let mut changed = program.clone();
                    match control {
                        0 => { changed.arena.function_defs[definition.index()].return_ty = program.arena.params(original.params)[0].ty; }
                        1 => { changed.arena.params[parameter_index].ty = original.return_ty; }
                        2 => { changed.arena.params[parameter_index].ty_defaulted = true; }
                        3 => { changed.arena.function_defs[definition.index()].effects = Some(ArenaRange::default()); }
                        4 => {
                            let inner = program.module_statements(module).find_map(|statement| if let ArenaStmtKind::Export(inner) = program.arena.stmt(statement).kind {
                                (program.arena.stmt(inner).kind == ArenaStmtKind::PureDef(definition)).then_some(inner)
                            } else { None }).unwrap();
                            changed.arena.stmt_tags[inner.index()] = crate::syntax::arena::ArenaStmtTag::ProcDef;
                        }
                        5 => {
                            let statement = program.arena.stmt_ids(program.arena.block(original.body).statements).next().unwrap();
                            let ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(returned))) = program.arena.stmt(statement).kind else { unreachable!(); };
                            let ArenaExprKind::Index { index, .. } = program.arena.expr(returned).kind else { unreachable!(); };
                            let ArenaExprKind::Int(integer) = program.arena.expr(index).kind else { unreachable!(); };
                            changed.arena.int_literals[integer.index()] = crate::syntax::node::IntLiteral::from_text("0");
                        }
                        6 => { changed.arena.type_expr_data[program.arena.params(original.params)[0].ty.index()].rhs ^= 1; }
                        7 => { changed.arena.type_expr_data[original.return_ty.index()].rhs ^= 1; }
                        8 => { changed.arena.function_defs[definition.index()].name = Name::intern("renamed_type_name"); }
                        _ => {
                            let replacement = program.module_statements(module).find_map(|statement| {
                                let ArenaStmtKind::Export(inner) = program.arena.stmt(statement).kind else { return None; };
                                let ArenaStmtKind::PureDef(id) = program.arena.stmt(inner).kind else { return None; };
                                (id != definition).then(|| program.arena.stmt_ids(program.arena.block(program.arena.function_def(id).body).statements).next().unwrap())
                            }).unwrap();
                            let block = program.arena.block(original.body);
                            let returned = program.arena.stmt_ids(block.statements).next().unwrap();
                            changed.arena.stmt_tags[replacement.index()] = program.arena.stmt_tags[returned.index()];
                            changed.arena.stmt_data[replacement.index()] = program.arena.stmt_data[returned.index()];
                            changed.arena.extra[block.statements.start as usize] = replacement.index() as u32;
                            assert!(changed.module_statements(module).any(|statement| bridge_shape(&changed, statement, source).is_some()), "equivalent expression shape does not authorize a replaced body edge");
                        }
                    }
                    assert!(!changed.modules.iter().find(|candidate| candidate.name == module.name).unwrap().canonical_bridge_declaration_matches(&changed.arena, definition));
                    let checked = Checker::check_compact_declarations(&changed);
                    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.native-bridge-contract")), "changed native declaration cannot become an authored body: {:?}", checked.diagnostics);
                    let identity = DeclarationIdentity { source, namespace: Some(module.name), declaration: definition };
                    assert!(checked.solved.embedded_bridge(identity).is_none());
                    assert!(!checked.solved.expression_owners.values().any(|owner| *owner == identity));
                }
            });
        });
    }
}
