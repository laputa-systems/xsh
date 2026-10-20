use super::{
    Arc, ArenaStmtKind, BindingId, BuildExprId, BuildExprRow, CompactFunctionIndex,
    CallableKind, CompactLowerConstructProbe, LoweredFunctionKey, Name, QualifiedName, Rc,
    SlotScope, Span, StmtId, Type, lower_command_word_reference, lower_prepared_literal,
};

impl<'p> CompactLowerConstructProbe<'p, '_> {
    /// The program's function index, built once per lowering pass.
    pub(super) fn function_index(&self) -> Rc<CompactFunctionIndex> {
        let uninitialised = self.function_defs.borrow().is_none();
        if uninitialised {
            let index = Rc::new(CompactFunctionIndex::new(self.program, &self.scope));
            *self.function_defs.borrow_mut() = Some(index);
        }
        Rc::clone(
            self.function_defs
                .borrow()
                .as_ref()
                .expect("function index was just built"),
        )
    }

    pub(super) fn compact_function_sig_for_key(
        &self,
        key: LoweredFunctionKey,
    ) -> Option<&crate::sema::check::CompactFunctionSig> {
        match key {
            LoweredFunctionKey::Name(name) => self
                .declarations
                .pures
                .get(&name)
                .or_else(|| self.declarations.procs.get(&name))
                .or_else(|| self.declarations.streams.get(&name)),
            LoweredFunctionKey::Qualified(qualified) => self
                .declarations
                .qualified_pures
                .get(&qualified)
                .or_else(|| self.declarations.qualified_procs.get(&qualified))
                .or_else(|| self.declarations.qualified_streams.get(&qualified)),
        }
    }

    pub(super) fn compact_direct_pure_call_candidate(&self, key: LoweredFunctionKey) -> bool {
        let definitions = self.function_index();
        let Some(function) = definitions.definition(key) else {
            return false;
        };
        if function.kind == CallableKind::Proc {
            return false;
        }
        let def = self.program.arena.function_def(function.id);
        let span = self
            .program
            .arena
            .span(self.program.arena.block(def.body).span);
        span.end().saturating_sub(span.start()) <= 12 * 1024
    }

    pub(super) fn compact_qualified_function_available(&self, name: QualifiedName) -> bool {
        self.functions.map_or_else(
            || {
                self.declarations.qualified_pures.contains_key(&name)
                    || self.declarations.qualified_procs.contains_key(&name)
                    || self.declarations.qualified_streams.contains_key(&name)
            },
            |functions| functions.contains(LoweredFunctionKey::Qualified(name)),
        )
    }

    pub(super) fn compact_qualified_function_sig(
        &self,
        module: Name,
        name: Name,
    ) -> Option<&crate::sema::check::CompactFunctionSig> {
        let qualified = self.compact_qualified_function_key(module, name);
        self.declarations
            .qualified_pures
            .get(&qualified)
            .or_else(|| self.declarations.qualified_procs.get(&qualified))
            .or_else(|| self.declarations.qualified_streams.get(&qualified))
    }

    pub(super) fn compact_qualified_function_key(&self, module: Name, name: Name) -> QualifiedName {
        QualifiedName::new(
            self.compact_imported_module_owner(module).unwrap_or(module),
            name,
        )
    }

    // The shared arena can contain one physical namespace under several module-root contexts.
    // Resolve import aliases only among modules reachable from this lowering root.
    pub(super) fn compact_imported_module_owner(&self, alias: Name) -> Option<Name> {
        let module_owner = |use_id| {
            let use_stmt = self.program.arena.use_stmt(use_id);
            let imported_as = use_stmt
                .alias
                .or_else(|| self.program.arena.names(use_stmt.path).last());
            if imported_as != Some(alias) {
                return None;
            }
            let key = use_stmt.resolved.as_deref()?;
            self.scope
                .module_with_key(key)
                .map(|module| module.name)
        };

        if let Some(namespace) = self.current_namespace {
            let module = self.scope.module_named(namespace)?;
            return self
                .program
                .module_statements(module)
                .find_map(|statement| match self.program.arena.stmt(statement).kind {
                    ArenaStmtKind::Use(use_id) => module_owner(use_id),
                    _ => None,
                });
        }

        self.program
            .arena
            .stmt_ids(self.scope.root_statements())
            .find_map(|statement| {
                match self.program.arena.stmt(statement).kind {
                    ArenaStmtKind::Use(use_id) => module_owner(use_id),
                    _ => None,
                }
            })
    }

    pub(super) fn declaration_binding(&self, span: Span, name: Name) -> Option<BindingId> {
        self.bodies.resolved_bindings.declaration_sites.get(&(span, name)).copied()
    }

    pub(super) fn declare_source_at(&self, span: Span, name: Name, ty: Option<super::Type>, slots: &mut SlotScope) -> Option<usize> {
        Some(slots.declare_source(self.declaration_binding(span, name)?, name, ty))
    }

    pub(super) fn expression_binding(&self, expression: super::ExprId) -> Option<BindingId> {
        self.bodies.resolved_bindings.expressions.get(&expression).copied()
    }

    pub(super) fn source_slot(&self, expression: super::ExprId, slots: &SlotScope) -> Option<usize> {
        slots.resolve_binding(self.expression_binding(expression)?)
    }

    pub(super) fn lower_bare_ident_stmt(
        &self,
        statement: StmtId,
        name: Name,
        slots: &SlotScope,
    ) -> Option<BuildExprId> {
        let span = self.program.arena.stmt(statement).span;
        self.lower_bare_function_value_at(span, name, slots)
    }

    // Command splices and word references consume the same checked source
    // binding as expressions; prepared constants do not require a slot.
    pub(super) fn lower_splice_name(&self, name: Name, span: Span, slots: &SlotScope) -> Option<BuildExprId> {
        self.lower_named_binding(self.bodies.resolved_bindings.at(span, name), name, slots).or_else(|| {
            lower_command_word_reference(
                name.as_str().as_str(),
                slots,
                span,
                &self.scratch,
                &self.declarations.prepared_constants,
                &self.declarations.wire_enums,
                &self.bodies.resolved_bindings,
            )
        })
    }

    pub(super) fn lower_bare_ident(&self, expression: super::ExprId, name: Name, slots: &SlotScope) -> Option<BuildExprId> {
        self.lower_bare_function_value(expression, name, slots)
    }

    pub(super) fn lower_bare_ident_at(&self, span: Span, name: Name, slots: &SlotScope) -> Option<BuildExprId> {
        self.lower_bare_function_value_at(span, name, slots)
    }

    fn lower_named_binding(&self, binding: Option<BindingId>, name: Name, slots: &SlotScope) -> Option<BuildExprId> {
        if let Some(binding) = binding {
            let prepared = &self.declarations.prepared_constants;
            if let Some(initializer) = prepared.binding_values.get(&binding) {
                let origin = prepared.origins.get(initializer).copied().unwrap_or(*initializer);
                let cached = self.scratch.borrow().prepared_constants.get(&origin).cloned();
                let value = if let Some(value) = cached {
                    value
                } else {
                    let value = lower_prepared_literal(prepared.values.get(initializer)?, Some(&self.declarations.wire_enums))?;
                    self.scratch.borrow_mut().prepared_constants.insert(origin, value.clone());
                    value
                };
                return self.lower_prepared_constant(value, prepared.types.get(initializer));
            }
            return Some(push_build_row!(self, expr, BuildExprRow::Param(slots.resolve_binding(binding)?)));
        }
        if self.compact_tag_variant_arity(name) != Some(0) {
            return None;
        }
        self.lower_nullary_tag_value(name)
    }

    pub(super) fn lower_bare_function_value(
        &self,
        expression: super::ExprId,
        name: Name,
        slots: &SlotScope,
    ) -> Option<BuildExprId> {
        let binding = self.expression_binding(expression);
        if binding.is_some() {
            return self.lower_named_binding(binding, name, slots);
        }
        if let Some(target) = self.bodies.selected_function_calls.get(&expression).copied() {
            let checked_type = self.bodies.expr_types.get(&expression)?;
            return self.lower_selected_function_ref(target, name, checked_type);
        }
        if self.compact_tag_variant_arity(name) == Some(0) {
            return self.lower_nullary_tag_value(name);
        }
        None
    }

    pub(super) fn lower_bare_function_value_at(
        &self,
        span: Span,
        name: Name,
        slots: &SlotScope,
    ) -> Option<BuildExprId> {
        let binding = self.bodies.resolved_bindings.at(span, name);
        if binding.is_some() {
            return self.lower_named_binding(binding, name, slots);
        }
        if let Some((target, checked_type)) =
            self.bodies.selected_function_references.get(&span).cloned()
        {
            return self.lower_selected_function_ref(target, name, checked_type.as_ref()?);
        }
        if self.compact_tag_variant_arity(name) == Some(0) {
            return self.lower_nullary_tag_value(name);
        }
        None
    }

    fn lower_selected_function_ref(
        &self,
        target: crate::sema::check::CheckedFunctionTarget,
        name: Name,
        checked_type: &Type,
    ) -> Option<BuildExprId> {
        if target.name != name || target.namespace != self.current_namespace {
            return None;
        }
        let index = self.function_index();
        let key = index.checked_target_key(target)?;
        let definition = index.definition(key)?;
        let matches_checked_type = match checked_type {
            Type::Pure => definition.kind != CallableKind::Proc,
            Type::Proc => definition.kind == CallableKind::Proc,
            Type::Callable(expected) => {
                let signature = self.compact_function_sig_for_key(key)?;
                let actual = crate::sema::types::CallableType {
                    params: signature.params.clone(),
                    return_ty: Box::new(signature.return_ty.clone()),
                    effects: signature.effects.clone(),
                };
                expected.mismatch(definition.kind != CallableKind::Proc, &actual, true).is_none()
            }
            _ => return None,
        };
        if !matches_checked_type {
            return None;
        }
        self.lower_function_ref(key)
    }

    fn lower_function_ref(&self, key: LoweredFunctionKey) -> Option<BuildExprId> {
        let pure = self.function_index().definition(key).map_or_else(
            || self.functions.is_none_or(|functions| functions.pure_contains(key)),
            |function| function.kind != CallableKind::Proc,
        );
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::FunctionRef {
                function: match key {
                    LoweredFunctionKey::Name(name) => name.into(),
                    LoweredFunctionKey::Qualified(name) => name.into(),
                },
                pure,
            }
        ))
    }

    fn lower_nullary_tag_value(&self, name: Name) -> Option<BuildExprId> {
        self.record_tag_pattern_schema(name)?;
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::Tag {
                type_name: self.compact_tag_type_name(name)?,
                wire: self.compact_tag_wire(name),
                name: Arc::<str>::from(name.as_str().as_str()),
                fields: Default::default(),
            }
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::{entry_source_from_text, parse_load_entry_source_arena_only};
    use crate::sema::check::{Checker, CompactDeclOutput};
    use crate::source::SourceMap;
    use crate::syntax::arena::{ArenaExprKind, ArenaRecordFieldKind, ExprId};
    use crate::syntax::parser::ArenaParseOutput;

    fn checked_program() -> (String, SourceMap, ArenaParseOutput, CompactDeclOutput) {
        let source = "type Renderer = pure(value: Int) -> Int\npure render() -> Int { 1 }\npure other() -> Int { 2 }\npure render_defaulted(value: Int = 1) -> Int { value }\npure factory() -> Renderer { render_defaulted }\nlet first = render\nlet second = other\nlet fields = {render, marker: 1}\n".to_string();
        let entry = entry_source_from_text("function-reference-authority.xsh", source.clone());
        let source_id = entry.source_id;
        let (sources, parsed) = parse_load_entry_source_arena_only(
            "function-reference-authority.xsh",
            entry,
            Vec::new(),
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let declarations = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
        (source, sources, parsed, declarations)
    }

    fn probe<'a>(
        parsed: &'a ArenaParseOutput,
        declarations: &'a CompactDeclOutput,
        source: &'a str,
        sources: &'a SourceMap,
    ) -> CompactLowerConstructProbe<'a, 'a> {
        CompactLowerConstructProbe {
            program: &parsed.arena,
            scope: Rc::new(super::super::CompactRootScope::whole_program(&parsed.arena)),
            declarations,
            qualified_schemes: &declarations.function_schemes,
            bodies: &declarations.bodies,
            source,
            sources: Some(sources),
            current_namespace: None,
            functions: None,
            top_level_known: super::super::FxHashMap::default(),
            output: super::super::CompactLowerConstructProbeOutput::default(),
            last_blocker_detail: None,
            stdlib_linkage: super::super::StdlibLowerLinkage::Local,
            function_defs: Rc::new(std::cell::RefCell::new(None)),
            scratch: Rc::new(std::cell::RefCell::new(super::super::BuildScratch::default())),
            with_error_source_exprs: super::super::FxHashMap::default(),
        }
    }

    fn identifier(parsed: &ArenaParseOutput, name: Name) -> ExprId {
        (0..parsed.arena.arena.expr_tags.len())
            .map(ExprId::from_index)
            .find(|&id| matches!(parsed.arena.arena.expr(id).kind, ArenaExprKind::Ident(found) if found == name))
            .expect("the fixture contains the requested identifier")
    }

    fn function_name(parsed: &ArenaParseOutput, name: &str) -> Name {
        parsed
            .arena
            .arena
            .function_defs
            .iter()
            .find(|definition| definition.name.as_str().as_str() == name)
            .map(|definition| definition.name)
            .expect("the fixture contains the requested function")
    }

    fn tail_statement(parsed: &ArenaParseOutput, name: Name) -> StmtId {
        (0..parsed.arena.arena.stmt_tags.len())
            .map(StmtId::from_index)
            .find(|&id| matches!(parsed.arena.arena.stmt(id).kind, ArenaStmtKind::TailBareIdent(found) if found == name))
            .expect("the fixture contains the requested tail")
    }

    fn function_body_span(parsed: &ArenaParseOutput, name: Name) -> Span {
        let definition = parsed
            .arena
            .arena
            .function_defs
            .iter()
            .find(|definition| definition.name == name)
            .expect("the fixture contains the requested function");
        parsed
            .arena
            .arena
            .span(parsed.arena.arena.block(definition.body).span)
    }

    fn shorthand_record(parsed: &ArenaParseOutput, name: &str) -> (ExprId, Span) {
        (0..parsed.arena.arena.expr_tags.len())
            .map(ExprId::from_index)
            .find_map(|id| {
                let ArenaExprKind::Record(fields) = parsed.arena.arena.expr(id).kind else {
                    return None;
                };
                parsed
                    .arena
                    .arena
                    .record_fields(fields)
                    .iter()
                    .find_map(|field| match field.kind {
                        ArenaRecordFieldKind::Shorthand {
                            name: found,
                            span,
                        } if found.as_str().as_str() == name => {
                            Some((id, parsed.arena.arena.span(span)))
                        }
                        _ => None,
                    })
            })
            .expect("the fixture contains the requested shorthand")
    }

    #[test]
    fn bare_function_values_publish_and_lower_the_checked_target() {
        let (source, sources, parsed, declarations) = checked_program();
        let render = function_name(&parsed, "render");
        let expression = identifier(&parsed, render);
        let target = declarations
            .bodies
            .selected_function_calls
            .get(&expression)
            .copied()
            .expect("the checker publishes the function-value target");
        assert_eq!(target.name, render);

        let probe = probe(&parsed, &declarations, &source, &sources);
        let lowered = probe
            .lower_bare_ident(expression, render, &SlotScope::default())
            .expect("the checked function target lowers");
        let scratch = probe.scratch.borrow();
        let BuildExprRow::FunctionRef { function, pure } =
            &scratch.expressions[lowered.index()]
        else {
            panic!("function values lower as function references");
        };
        assert_eq!(function.as_name(), Some(render));
        assert!(*pure);
    }

    #[test]
    fn missing_function_value_target_does_not_reselect_by_spelling() {
        let (source, sources, parsed, mut declarations) = checked_program();
        let render = function_name(&parsed, "render");
        let expression = identifier(&parsed, render);
        assert!(declarations
            .bodies
            .selected_function_calls
            .remove(&expression)
            .is_some());

        let probe = probe(&parsed, &declarations, &source, &sources);
        assert!(probe
            .lower_bare_ident(expression, render, &SlotScope::default())
            .is_none());
    }

    #[test]
    fn mismatched_function_value_target_fails_closed() {
        let (source, sources, parsed, mut declarations) = checked_program();
        let render = function_name(&parsed, "render");
        let expression = identifier(&parsed, render);
        let other = declarations
            .bodies
            .selected_function_calls
            .get(&identifier(&parsed, function_name(&parsed, "other")))
            .copied()
            .expect("the other function has a checker target");
        declarations
            .bodies
            .selected_function_calls
            .insert(expression, other);

        let probe = probe(&parsed, &declarations, &source, &sources);
        assert!(probe
            .lower_bare_ident(expression, render, &SlotScope::default())
            .is_none());
    }

    #[test]
    fn mismatched_function_value_kind_fails_closed() {
        let (source, sources, parsed, mut declarations) = checked_program();
        let render = function_name(&parsed, "render");
        let expression = identifier(&parsed, render);
        assert_eq!(declarations.bodies.expr_types.get(&expression), Some(&Type::Pure));
        declarations.bodies.expr_types.insert(expression, Type::Proc);

        let probe = probe(&parsed, &declarations, &source, &sources);
        assert!(probe
            .lower_bare_ident(expression, render, &SlotScope::default())
            .is_none());
    }

    #[test]
    fn typed_callable_tail_publishes_and_lowers_the_checked_target() {
        let (source, sources, parsed, declarations) = checked_program();
        let name = function_name(&parsed, "render_defaulted");
        let statement = tail_statement(&parsed, name);
        let span = parsed.arena.arena.stmt(statement).span;
        let (target, checked_type) = declarations
            .bodies
            .selected_function_references
            .get(&span)
            .cloned()
            .expect("the checker publishes the tail function target");
        assert_eq!(target.name, name);
        assert!(matches!(checked_type, Some(Type::Callable(_))));

        let probe = probe(&parsed, &declarations, &source, &sources);
        let lowered = probe
            .lower_bare_ident_stmt(statement, name, &SlotScope::default())
            .expect("the checked callable tail lowers");
        let scratch = probe.scratch.borrow();
        let BuildExprRow::FunctionRef { function, pure } =
            &scratch.expressions[lowered.index()]
        else {
            panic!("function-valued tails lower as function references");
        };
        assert_eq!(function.as_name(), Some(name));
        assert!(*pure);
    }

    #[test]
    fn missing_typed_callable_tail_target_does_not_reselect_by_spelling() {
        let (source, sources, parsed, mut declarations) = checked_program();
        let name = function_name(&parsed, "render_defaulted");
        let statement = tail_statement(&parsed, name);
        let span = parsed.arena.arena.stmt(statement).span;
        let factory_span = function_body_span(&parsed, function_name(&parsed, "factory"));
        let checked_return_type = declarations
            .function_return_types
            .get(&factory_span)
            .expect("the factory's checked return type remains published");
        assert!(matches!(checked_return_type, Type::Callable(_)));
        assert_eq!(
            declarations
                .function_signatures
                .get(&factory_span)
                .map(|signature| &signature.return_ty),
            Some(checked_return_type)
        );
        assert!(declarations
            .bodies
            .selected_function_references
            .remove(&span)
            .is_some());

        let probe = probe(&parsed, &declarations, &source, &sources);
        assert!(probe
            .lower_bare_ident_stmt(statement, name, &SlotScope::default())
            .is_none());
        let source_id = sources.files()[0].id();
        let error = match crate::runtime::eval::FullBuilder::build_compact(
            &parsed.arena,
            &declarations,
            &source,
            Arc::new(sources.clone()),
            source_id,
        ) {
            Err(error) => error,
            Ok(_) => panic!("a missing typed callable tail target must block full lowering"),
        };
        assert_eq!(error.construct, "full_ir_function_blocker");
    }

    #[test]
    fn shorthand_function_values_retain_the_checked_span_target() {
        let (source, sources, parsed, declarations) = checked_program();
        let render = function_name(&parsed, "render");
        let (record, span) = shorthand_record(&parsed, "render");
        assert!(matches!(
            declarations.bodies.expr_types.get(&record),
            Some(Type::Record(_))
        ));
        let matching_exprs: Vec<_> = (0..parsed.arena.arena.expr_tags.len())
            .map(ExprId::from_index)
            .filter(|&id| parsed.arena.arena.expr(id).span == span)
            .map(|id| {
                (
                    format!("{:?}", parsed.arena.arena.expr(id).kind),
                    declarations.bodies.selected_function_calls.get(&id).copied(),
                )
            })
            .collect();
        assert!(
            declarations.bodies.selected_function_references.contains_key(&span),
            "the record shorthand span must retain its checked target; matching expression kinds and targets: {matching_exprs:?}"
        );

        let probe = probe(&parsed, &declarations, &source, &sources);
        let lowered = probe
            .lower_bare_ident_at(span, render, &SlotScope::default())
            .expect("the shorthand's checked function target lowers");
        let scratch = probe.scratch.borrow();
        let BuildExprRow::FunctionRef { function, pure } =
            &scratch.expressions[lowered.index()]
        else {
            panic!("function shorthand lowers as a function reference");
        };
        assert_eq!(function.as_name(), Some(render));
        assert!(*pure);
    }

    #[test]
    fn mismatched_shorthand_function_kind_fails_closed() {
        let (source, sources, parsed, mut declarations) = checked_program();
        let render = function_name(&parsed, "render");
        let (_, span) = shorthand_record(&parsed, "render");
        let (target, checked_type) = declarations
            .bodies
            .selected_function_references
            .get(&span)
            .cloned()
            .expect("the checker publishes the shorthand target and type");
        assert_eq!(checked_type, Some(Type::Pure));
        declarations
            .bodies
            .selected_function_references
            .insert(span, (target, Some(Type::Proc)));

        let probe = probe(&parsed, &declarations, &source, &sources);
        assert!(probe
            .lower_bare_ident_at(span, render, &SlotScope::default())
            .is_none());
    }
}
