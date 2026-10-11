use super::{
    Arc, ArenaStmtKind, BindingId, BuildExprId, BuildExprRow, CompactFunctionIndex,
    CompactLowerConstructProbe, LoweredFunctionKey, LoweredType, Name, QualifiedName, Rc,
    SlotScope, Span, StmtId, lower_command_word_reference, lower_literal_constant,
};

impl<'p> CompactLowerConstructProbe<'p, '_> {
    /// The program's function index, built once per lowering pass.
    pub(super) fn function_index(&self) -> Rc<CompactFunctionIndex> {
        let uninitialised = self.function_defs.borrow().is_none();
        if uninitialised {
            let index = Rc::new(CompactFunctionIndex::new(self.program));
            *self.function_defs.borrow_mut() = Some(index);
        }
        Rc::clone(
            self.function_defs
                .borrow()
                .as_ref()
                .expect("function index was just built"),
        )
    }

    fn compact_function_available(&self, name: Name) -> bool {
        self.functions.map_or_else(
            || {
                self.declarations.pures.contains_key(&name)
                    || self.declarations.procs.contains_key(&name)
                    || self.declarations.streams.contains_key(&name)
            },
            |functions| functions.contains(LoweredFunctionKey::Name(name)),
        )
    }

    pub(super) fn compact_unqualified_function_key(&self, name: Name) -> Option<LoweredFunctionKey> {
        if let Some(namespace) = self.current_namespace {
            let qualified = QualifiedName::new(namespace, name);
            if self.compact_qualified_function_available(qualified) {
                return Some(LoweredFunctionKey::Qualified(qualified));
            }
            return self.compact_imported_unqualified_function_key(name);
        }
        if self.compact_function_available(name) {
            return Some(LoweredFunctionKey::Name(name));
        }
        self.compact_imported_unqualified_function_key(name)
    }

    pub(super) fn compact_unqualified_function_sig(
        &self,
        name: Name,
    ) -> Option<&crate::sema::check::CompactFunctionSig> {
        if let Some(namespace) = self.current_namespace {
            let qualified = QualifiedName::new(namespace, name);
            return self
                .declarations
                .qualified_pures
                .get(&qualified)
                .or_else(|| self.declarations.qualified_procs.get(&qualified))
                .or_else(|| self.declarations.qualified_streams.get(&qualified))
                .or_else(|| self.compact_imported_unqualified_function_sig(name));
        }
        self.declarations
            .pures
            .get(&name)
            .or_else(|| self.declarations.procs.get(&name))
            .or_else(|| self.declarations.streams.get(&name))
            .or_else(|| self.compact_imported_unqualified_function_sig(name))
    }

    pub(super) fn compact_direct_pure_call_candidate(&self, key: LoweredFunctionKey) -> bool {
        let definitions = self.function_index();
        let Some(function) = definitions.definition(key) else {
            return false;
        };
        if !function.pure {
            return false;
        }
        let def = self.program.arena.function_def(function.id);
        let span = self
            .program
            .arena
            .span(self.program.arena.block(def.body).span);
        span.end().saturating_sub(span.start()) <= 12 * 1024
    }

    fn compact_imported_unqualified_function_key(&self, name: Name) -> Option<LoweredFunctionKey> {
        if !matches!(
            self.top_level_known.get(&name).map(|binding| binding.kind),
            Some(LoweredType::Pure | LoweredType::Proc | LoweredType::Stream)
        ) {
            return None;
        }
        self.compact_unique_qualified_function(name)
            .map(LoweredFunctionKey::Qualified)
    }

    fn compact_imported_unqualified_function_sig(
        &self,
        name: Name,
    ) -> Option<&crate::sema::check::CompactFunctionSig> {
        let qualified = self.compact_unique_qualified_function(name)?;
        self.declarations
            .qualified_pures
            .get(&qualified)
            .or_else(|| self.declarations.qualified_procs.get(&qualified))
            .or_else(|| self.declarations.qualified_streams.get(&qualified))
    }

    fn compact_unique_qualified_function(&self, name: Name) -> Option<QualifiedName> {
        let mut found = None;
        for qualified in self
            .declarations
            .qualified_pures
            .keys()
            .chain(self.declarations.qualified_procs.keys())
            .chain(self.declarations.qualified_streams.keys())
            .copied()
            .filter(|qualified| qualified.member == name)
        {
            if !self.compact_qualified_function_available(qualified) {
                continue;
            }
            if found.replace(qualified).is_some() {
                return None;
            }
        }
        found
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
            self.program
                .modules
                .iter()
                .find(|module| module.key.as_str() == key)
                .map(|module| module.name)
        };

        if let Some(namespace) = self.current_namespace {
            let module = self
                .program
                .modules
                .iter()
                .find(|module| module.name == namespace)?;
            return self
                .program
                .module_statements(module)
                .find_map(|statement| match self.program.arena.stmt(statement).kind {
                    ArenaStmtKind::Use(use_id) => module_owner(use_id),
                    _ => None,
                });
        }

        self.program.statement_ids().find_map(|statement| {
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
        self.lower_named_binding(self.bodies.resolved_bindings.tails.get(&statement).copied(), name, slots)
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
        self.lower_named_binding(self.expression_binding(expression), name, slots)
    }

    pub(super) fn lower_bare_ident_at(&self, span: Span, name: Name, slots: &SlotScope) -> Option<BuildExprId> {
        self.lower_named_binding(self.bodies.resolved_bindings.at(span, name), name, slots)
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
                    let value = lower_literal_constant(prepared.values.get(initializer)?, Some(&self.declarations.wire_enums))?;
                    self.scratch.borrow_mut().prepared_constants.insert(origin, value.clone());
                    value
                };
                return Some(push_build_row!(self, expr, BuildExprRow::PreparedConstant(super::super::PreparedConstantValue(value))));
            }
            return Some(push_build_row!(self, expr, BuildExprRow::Param(slots.resolve_binding(binding)?)));
        }
        if let Some(key) = self.compact_unqualified_function_key(name) {
            // Function bodies lower with every function as an in-flight
            // candidate, which `pure_contains` counts as pure; the
            // definition decides, so a proc alias called in a body
            // resolves as a proc instead of failing at runtime.
            let pure = self.function_index().definition(key).map_or_else(
                || {
                    self.functions
                        .is_none_or(|functions| functions.pure_contains(key))
                },
                |function| function.pure,
            );
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::FunctionRef {
                    function: match key {
                        LoweredFunctionKey::Name(name) => name.into(),
                        LoweredFunctionKey::Qualified(name) => name.into(),
                    },
                    pure,
                }
            ));
        }
        if self.compact_tag_variant_arity(name) != Some(0) {
            return None;
        }
        Some({
            push_build_row!(
                self,
                expr,
                BuildExprRow::Tag {
                    type_name: self.compact_tag_type_name(name)?,
                    wire: self.compact_tag_wire(name),
                    name: Arc::<str>::from(name.as_str().as_str()),
                    fields: Default::default(),
                }
            )
        })
    }
}
