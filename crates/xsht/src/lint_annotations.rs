use super::{
    ArenaBindingTargetKind, ArenaExpr, ArenaExprKind, ArenaExprOrRun, ArenaStmtKind,
    ArenaTypeExprKind, ArenaTypeExprTag, BinaryOp, BindingTargetId, BlockId, Diagnostic,
    DiagnosticCode, Effect, ExprId, FixHint, FunctionDefId, FxHashSet, Label, Linter, Name,
    Severity, Span, StmtId, Symbol, Type, TypeExprId, checked_return_removal_facts,
    effects_annotation, effects_covers_any, expr_child_exprs, inferred_proc_return,
    is_map_empty_call, ok_call_arg, return_value_is_ok_unit, scan_after_type, scan_back_space,
    scan_before_arrow, scan_before_colon, scan_effect_list_span, scan_return_stmt_span,
    span_end_after_following_newlines, tail_type_matches_lint_expected, type_expr_kind,
    type_has_contextual_collection_domain, type_mentions_path,
};

impl<'a> Linter<'a> {
    pub(super) fn lint_inferred_pure_return(&mut self, id: FunctionDefId, exported: bool) {
        let def = self.arena.function_def(id);
        if !self.prefer_inferred_pure_returns || exported || def.return_ty_defaulted {
            return;
        }
        let ty_span = self.arena.type_expr_span(def.return_ty);
        let mut types = vec![def.return_ty];
        while let Some(id) = types.pop() {
            let data = self.arena.type_expr_data[id.index()];
            match self.arena.type_expr_tags[id.index()] {
                ArenaTypeExprTag::Named => {
                    let name = Name::from_symbol(Symbol::from_raw(data.lhs));
                    if Type::builtin_from_name(&name.as_str()).is_none() {
                        return;
                    }
                }
                // A callable type's data words are not a child type id, and
                // its parts may name user types; it is never provably builtin.
                ArenaTypeExprTag::Qualified | ArenaTypeExprTag::Callable => return,
                ArenaTypeExprTag::Result => {
                    types.push(TypeExprId::from_index(data.lhs as usize));
                    if let Some(error) = TypeExprId::from_optional_raw(data.rhs) {
                        types.push(error);
                    }
                }
                ArenaTypeExprTag::Map => {
                    types.push(TypeExprId::from_index(data.lhs as usize));
                    if let Some(key) = TypeExprId::from_optional_raw(data.rhs) {
                        types.push(key);
                    }
                }
                // `lhs` is not a child here: the members and arguments live
                // in the side table.
                ArenaTypeExprTag::Union => types.extend(self.arena.union_type_members(id)),
                ArenaTypeExprTag::Applied => {
                    types.push(TypeExprId::from_index(data.lhs as usize));
                    types.extend(self.arena.applied_type_arguments(id));
                }
                ArenaTypeExprTag::List
                | ArenaTypeExprTag::Stream
                | ArenaTypeExprTag::Module
                | ArenaTypeExprTag::Optional
                | ArenaTypeExprTag::NonEmpty
                | ArenaTypeExprTag::Set => types.push(TypeExprId::from_index(data.lhs as usize)),
            }
        }
        let start = scan_before_arrow(self.source, ty_span.start());
        let deletion = Span::new(ty_span.source_id, start, ty_span.end());
        let Some(annotation) = self.source.get(start..ty_span.end()) else {
            return;
        };
        if annotation.contains('#') {
            return;
        }
        if self.return_removal_before.is_none() {
            self.return_removal_before = Some(checked_return_removal_facts(
                self.source,
                ty_span.source_id,
                None,
            ));
        }
        let Some(before) = self.return_removal_before.as_ref().unwrap().as_ref() else {
            return;
        };
        let mut rewritten = self.source.to_string();
        rewritten.replace_range(start..ty_span.end(), "");
        let after = checked_return_removal_facts(
            &rewritten,
            ty_span.source_id,
            Some((start, ty_span.end() - start)),
        );
        if Some(before) != after.as_ref() {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "private pure return type can be inferred exactly",
            )
            .with_code(DiagnosticCode::LintPreferInferredPureReturn)
            .with_label(Label::secondary(
                ty_span,
                "definition and caller types remain identical without this annotation",
            ))
            .with_fix_hint(FixHint::deletion(deletion, "infer the private pure return")),
        );
    }

    pub(super) fn lint_default_parameter_annotation(
        &mut self,
        param: &xsh::frontend::syntax::arena::ArenaParam,
    ) {
        if param.ty_defaulted
            || param.default.is_none()
            || param.rest
            || self.annotation_refs_user_type(param.ty)
        {
            return;
        }
        let span = self.arena.type_expr_span(param.ty);
        let param_span = self.arena.span(param.span);
        let Some(prefix) = self.source.get(param_span.start()..span.start()) else {
            return;
        };
        let Some(colon) = prefix.rfind(':') else {
            return;
        };
        let start = param_span.start() + colon;
        let Some(annotation) = self.source.get(start..span.end()) else {
            return;
        };
        if annotation.contains('#') {
            return;
        }
        if self.return_removal_before.is_none() {
            self.return_removal_before = Some(checked_return_removal_facts(
                self.source,
                span.source_id,
                None,
            ));
        }
        let Some(before) = self.return_removal_before.as_ref().unwrap().as_ref() else {
            return;
        };
        let mut rewritten = self.source.to_string();
        rewritten.replace_range(start..span.end(), "");
        let after = checked_return_removal_facts(
            &rewritten,
            span.source_id,
            Some((start, span.end() - start)),
        );
        if Some(before) != after.as_ref() {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "default establishes exactly the declared parameter type",
            )
            .with_code(DiagnosticCode::LintDefaultParamType)
            .with_label(Label::secondary(
                span,
                "checked signatures, expression types, effects and conversions remain identical",
            ))
            .with_fix_hint(FixHint::deletion(
                Span::new(span.source_id, start, span.end()),
                "infer the parameter type from its default",
            )),
        );
    }

    pub(super) fn checked_effect_fact(&self, body: Span) -> Option<&xsh::frontend::check::FunctionEffectFact> {
        let mut matches = self
            .checked_effects
            .iter()
            .filter_map(|(id, fact)| (id.body == body).then_some(fact));
        let fact = matches.next()?;
        matches.next().is_none().then_some(fact)
    }

    /// A private proc or stream whose clause equals the set it would infer
    /// gains nothing from the clause. Exports keep theirs because a clause there
    /// may be a deliberate API contract, and `main` and tests are entry points
    /// whose clause bounds the whole program or test. The checker's solver
    /// proves the inferred set is the declared one across module boundaries;
    /// when the file also checks on its own, every other checked fact must be
    /// unchanged by the deletion too.
    pub(super) fn lint_inferred_proc_effects(
        &mut self,
        definition: FunctionDefId,
        exported: bool,
        entrypoint: bool,
        statement_span: Span,
    ) {
        if !self.prefer_inferred_private_effects || exported || entrypoint {
            return;
        }
        let def = self.arena.function_def(definition);
        if def.effects.is_none() || def.test_declaration {
            return;
        }
        let body = self.arena.span(self.arena.block(def.body).span);
        if !self
            .checked_effect_fact(body)
            .is_some_and(|fact| fact.redundant_clause)
        {
            return;
        }
        let Some(clause) = scan_effect_list_span(self.arena, def, statement_span, self.source)
        else {
            return;
        };
        // Delete the space before the clause too, leaving `) -> T` formatted.
        let start = self.source[..clause.start()].trim_end_matches(' ').len();
        let span = Span::new(clause.source_id, start, clause.end());
        if self.return_removal_before.is_none() {
            self.return_removal_before = Some(checked_return_removal_facts(
                self.source,
                span.source_id,
                None,
            ));
        }
        if let Some(before) = self.return_removal_before.as_ref().unwrap() {
            let mut rewritten = self.source.to_string();
            rewritten.replace_range(span.start()..span.end(), "");
            let after = checked_return_removal_facts(
                &rewritten,
                span.source_id,
                Some((span.start(), span.end() - span.start())),
            );
            if after.as_ref() != Some(before) {
                return;
            }
        }
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "private effect clause names exactly the inferred effects",
            )
            .with_code(DiagnosticCode::LintPreferInferredPrivateEffects)
            .with_label(Label::secondary(
                clause,
                "checked body, caller contracts, and statement purposes remain equivalent",
            ))
            .with_fix_hint(FixHint::deletion(span, "infer the private effects")),
        );
    }

    pub(super) fn lint_proc_function(
        &mut self,
        def_id: FunctionDefId,
        exported: bool,
        entrypoint: bool,
        statement_span: Span,
    ) {
        self.lint_inferred_proc_effects(def_id, exported, entrypoint, statement_span);
        self.note_proc_return_candidate(def_id, exported);
        let def = self.arena.function_def(def_id).clone();
        // Without the annotation, a complete `if`/`match` tail may infer a value.
        let branching_tail = self
            .arena
            .stmt_ids(self.arena.block(def.body).statements)
            .last()
            .is_some_and(|tail| {
                matches!(
                    self.arena.stmt(tail).kind,
                    ArenaStmtKind::Match { .. }
                        | ArenaStmtKind::If {
                            else_block: Some(_),
                            ..
                        }
                )
            });
        if !def.return_ty_defaulted
            && !exported
            && !branching_tail
            // An error family in the annotation is a contract the body may
            // rely on (`Err(.Variant(...))`); only the broad form is implied.
            && inferred_proc_return::broad_result_unit(self.arena, def.return_ty)
        {
            let ty_span = self.arena.type_expr_span(def.return_ty);
            let deletion_start = scan_before_arrow(self.source, ty_span.start());
            let deletion_span = Span::new(ty_span.source_id, deletion_start, ty_span.end());
            self.diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    "redundant `Result[Unit]` return annotation",
                )
                .with_code(DiagnosticCode::LintRedundantResultUnit)
                .with_label(Label::secondary(
                    ty_span,
                    "a proc without a value tail infers `Result[Unit]`",
                ))
                .with_fix_hint(FixHint::deletion(
                    deletion_span,
                    "remove return type annotation",
                )),
            );
        }
        self.lint_function(def_id);
    }

    pub(super) fn lint_effect_annotation(&mut self, def_id: FunctionDefId, stmt_span: Span) {
        let def = self.arena.function_def(def_id).clone();
        let body_span = self.arena.span(self.arena.block(def.body).span);
        let Some(fact) = self.checked_effect_fact(body_span) else {
            return;
        };
        if fact.inferred {
            return;
        }
        let Some(required) = &fact.required else {
            return;
        };
        let mut effects: FxHashSet<_> = required.iter().cloned().collect();
        if self.assertion_effect_spans.iter().any(|span| {
            span.source_id == body_span.source_id
                && span.start() >= body_span.start()
                && span.end() <= body_span.end()
        }) {
            effects.insert(Effect::Error);
        }
        if effects.is_empty() {
            return;
        }
        // Every declaration without a clause infers its effects, so only an
        // incomplete clause needs a suggestion.
        let Some(declared_range) = def.effects else {
            return;
        };
        let declared: Vec<Effect> = self.arena.effects(declared_range).collect();
        let missing = effects
            .iter()
            .filter(|effect| !effects_covers_any(&declared, effect))
            .cloned()
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return;
        }
        let mut union = FxHashSet::default();
        for effect in &declared {
            union.insert(effect.clone());
        }
        for effect in missing {
            union.insert(effect);
        }
        let annotation = effects_annotation(&union);
        let Some(effect_span) = scan_effect_list_span(self.arena, &def, stmt_span, self.source)
        else {
            return;
        };
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!("proc `{}` is missing declared effects", def.name),
            )
            .with_code(DiagnosticCode::LintMissingEffects)
            .with_label(Label::secondary(
                stmt_span,
                format!("suggest [{annotation}]"),
            ))
            .with_fix_hint(FixHint::replacement(
                effect_span,
                format!("replace effect annotation with `[{annotation}]`"),
                format!("[{annotation}]"),
            )),
        );
    }

    pub(super) fn lint_redundant_bare_return(&mut self, body: BlockId) {
        let stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(body).statements)
            .collect();
        let Some(&last_id) = stmts.last() else {
            return;
        };
        let last_stmt = self.arena.stmt(last_id);
        if matches!(last_stmt.kind, ArenaStmtKind::Return(None)) {
            let deletion_span = scan_return_stmt_span(self.source, last_stmt.span);
            self.diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    "redundant `return` at end of `Result[Unit]` function",
                )
                .with_code(DiagnosticCode::LintRedundantBareReturn)
                .with_label(Label::secondary(
                    last_stmt.span,
                    "falling off the end also returns `Ok()`",
                ))
                .with_fix_hint(FixHint::deletion(deletion_span, "remove trailing `return`")),
            );
        }
    }

    pub(super) fn lint_tail_path_parse_roundtrip(&mut self, body: BlockId) {
        let stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(body).statements)
            .collect();
        let Some(&last_id) = stmts.last() else {
            return;
        };
        let ArenaStmtKind::Expr(expr) = self.arena.stmt(last_id).kind else {
            return;
        };
        self.lint_result_path_parse_roundtrip(expr);
    }

    pub(super) fn lint_tail_redundant_require(&mut self, body: BlockId) {
        let stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(body).statements)
            .collect();
        let Some(&last_id) = stmts.last() else {
            return;
        };
        let ArenaStmtKind::Expr(expr) = self.arena.stmt(last_id).kind else {
            return;
        };
        self.lint_result_redundant_require(expr);
    }

    pub(super) fn lint_return_path_parse_roundtrip(&mut self, value: &ArenaExprOrRun) {
        if !self.result_path_functions.last().copied().unwrap_or(false) {
            return;
        }
        let ArenaExprOrRun::Expr(expr) = value else {
            return;
        };
        self.lint_result_path_parse_roundtrip(*expr);
    }

    pub(super) fn lint_return_redundant_require(&mut self, value: &ArenaExprOrRun) {
        let ArenaExprOrRun::Expr(expr) = value else {
            return;
        };
        self.lint_result_redundant_require(*expr);
    }

    pub(super) fn lint_needless_annotation(
        &mut self,
        target: BindingTargetId,
        mutable: bool,
        ty: TypeExprId,
        initializer: &ArenaExprOrRun,
        exported: bool,
        binding_span: Span,
    ) {
        if self.lint_solved_local_annotation(
            target,
            mutable,
            ty,
            initializer,
            exported,
            binding_span,
        ) {
            return;
        }
        if mutable
            && let ArenaBindingTargetKind::Name(name) = self.arena.binding_target(target).kind
            && self.assigned_names.contains(&name)
        {
            return;
        }
        let annotation_ty = Type::from_arena(self.arena, ty);
        if matches!(
            annotation_ty,
            Type::Any | Type::Unknown | Type::Invalid | Type::Optional(_)
        ) {
            return;
        }
        let ArenaExprOrRun::Expr(init_expr_id) = initializer else {
            return;
        };
        // Empty Map factories acquire their key and value types from this
        // annotation. Removing it can also turn a later `{}` fix into a Record.
        if matches!(annotation_ty, Type::Map(_, _)) && is_map_empty_call(self.arena, *init_expr_id)
        {
            return;
        }
        if !self.annotation_is_needless(&annotation_ty, *init_expr_id) {
            return;
        }
        if self.annotation_refs_user_type(ty) {
            return;
        }
        let ty_span = self.arena.type_expr_span(ty);
        let deletion_start = scan_before_colon(self.source, ty_span.start());
        let deletion_end = scan_after_type(self.source, ty_span.end());
        if deletion_start >= deletion_end {
            return;
        }
        // Don't fix across comments
        if self.source[deletion_start..deletion_end].contains('#') {
            return;
        }
        let deletion_span = Span::new(ty_span.source_id, deletion_start, deletion_end);
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, "needless type annotation")
                .with_code(DiagnosticCode::LintNeedlessAnnotation)
                .with_label(Label::secondary(
                    ty_span,
                    "this type annotation is redundant with the initializer",
                ))
                .with_fix_hint(FixHint::deletion(
                    deletion_span,
                    "remove needless annotation",
                )),
        );
    }

    pub(super) fn lint_solved_local_annotation(
        &mut self,
        target: BindingTargetId,
        mutable: bool,
        annotation: TypeExprId,
        initializer: &ArenaExprOrRun,
        exported: bool,
        binding_span: Span,
    ) -> bool {
        if exported
            || self.function_return_types.is_empty()
            || self.annotation_refs_user_type(annotation)
            || !matches!(self.arena.binding_target(target).kind, ArenaBindingTargetKind::Name(name) if name != "_")
        {
            return false;
        }
        let ArenaExprOrRun::Expr(initializer) = *initializer else {
            return false;
        };
        let candidate = match self.arena.expr(initializer).kind {
            ArenaExprKind::List(items) | ArenaExprKind::Set(items) => items.is_empty(),
            ArenaExprKind::Null => mutable,
            _ => false,
        };
        if !candidate {
            return false;
        }
        let annotation_span = self.arena.type_expr_span(annotation);
        let deletion = Span::new(
            annotation_span.source_id,
            scan_before_colon(self.source, annotation_span.start()),
            scan_after_type(self.source, annotation_span.end()),
        );
        if deletion.start() >= deletion.end() || self.source[deletion.range()].contains('#') {
            return true;
        }
        if !self.local_annotation_removal_preserves_contract(deletion, binding_span) {
            return true;
        }
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "local type is determined by consistent checked constraints",
            )
            .with_code(DiagnosticCode::LintNeedlessAnnotation)
            .with_label(Label::secondary(
                annotation_span,
                "the whole local binding retains this fixed type without the annotation",
            ))
            .with_fix_hint(FixHint::deletion(
                deletion,
                "remove the redundant local annotation",
            )),
        );
        true
    }

    pub(super) fn annotation_is_needless(&self, annotation: &Type, initializer: ExprId) -> bool {
        let init = self.arena.expr(initializer);
        // Branches and record fields can acquire their types from the binding.
        // Their checked type alone cannot prove that removing context is safe.
        if matches!(
            init.kind,
            ArenaExprKind::If { .. }
                | ArenaExprKind::Match { .. }
                | ArenaExprKind::PatternTest { .. }
                | ArenaExprKind::PatternCondition { .. }
                | ArenaExprKind::Record(_)
        ) {
            return false;
        }
        if matches!(annotation, Type::List(inner) if matches!(inner.as_ref(), Type::Record(_))) {
            return false;
        }
        if self.is_empty_collection(&init) {
            return false;
        }
        if self.expression_depends_on_expected_type(initializer, true) {
            return false;
        }
        // A function's name is a callable with a checked signature. It is
        // the dynamic `Proc` or `Pure` only because the annotation asks for
        // one, so without it a later `.call` is checked against the
        // signature and the binding no longer takes another function.
        if matches!(annotation, Type::Proc | Type::Pure)
            && matches!(
                init.kind,
                ArenaExprKind::Ident(_) | ArenaExprKind::Field { .. }
            )
        {
            return false;
        }
        let Some(actual) = self.expr_types.get(&init.span) else {
            return false;
        };
        if matches!(actual, Type::Any | Type::Unknown | Type::Invalid) {
            return false;
        }
        *actual == *annotation
    }

    // Checked expression types include conversions and inference supplied by
    // the enclosing contract. Moving an expression out of that context must
    // retain collection element domains and independent validation targets.
    pub(super) fn expression_depends_on_expected_type(&self, expr: ExprId, removing_annotation: bool) -> bool {
        let expression = self.arena.expr(expr);
        match expression.kind {
            ArenaExprKind::Int(_) => self.expr_types.get(&expression.span) == Some(&Type::UInt),
            // A string literal is a Path only because a Path was expected,
            // and a validated path only because that type was.
            ArenaExprKind::Str(_) => self
                .expr_types
                .get(&expression.span)
                .is_some_and(|ty| *ty.unvalidated() == Type::Path),
            ArenaExprKind::PathStr(_) | ArenaExprKind::PathFmtString(_) => self
                .expr_types
                .get(&expression.span)
                .is_some_and(|ty| ty.validated().is_some()),
            ArenaExprKind::List(_)
            | ArenaExprKind::ListComp { .. }
            | ArenaExprKind::SetComp { .. }
            | ArenaExprKind::MapComp { .. } => {
                (removing_annotation && self.is_empty_collection(&expression))
                    || self
                        .expr_types
                        .get(&expression.span)
                        .is_some_and(type_has_contextual_collection_domain)
                    // A literal has a validated type only because one was
                    // expected, and an element of a validated type is read
                    // as its base only because the base was expected.
                    || self
                        .expr_types
                        .get(&expression.span)
                        .is_some_and(|ty| ty.validated().is_some())
                    || expr_child_exprs(self.arena, expr).into_iter().any(|child| {
                        self.expr_types
                            .get(&self.arena.expr(child).span)
                            .is_some_and(Type::holds_validated)
                    })
                    // A constant's elements carry no checked type of their
                    // own; in a collection of paths a string literal element
                    // is a Path only through the collection's declared type.
                    || (self
                        .expr_types
                        .get(&expression.span)
                        .is_some_and(type_mentions_path)
                        && expr_child_exprs(self.arena, expr).into_iter().any(|child| {
                            matches!(self.arena.expr(child).kind, ArenaExprKind::Str(_))
                        }))
                    || expr_child_exprs(self.arena, expr).into_iter().any(|child| {
                        self.expression_depends_on_expected_type(child, removing_annotation)
                    })
            }
            ArenaExprKind::Record(_)
            | ArenaExprKind::If { .. }
            | ArenaExprKind::Match { .. }
            | ArenaExprKind::PatternTest { .. }
            | ArenaExprKind::PatternCondition { .. }
            | ArenaExprKind::ValueBlock(_)
            | ArenaExprKind::Capture(_)
            | ArenaExprKind::Retry { .. }
            | ArenaExprKind::Loop { .. }
            | ArenaExprKind::Collect { .. }
            | ArenaExprKind::ErrorContext { .. }
            | ArenaExprKind::ContextScope { .. }
            | ArenaExprKind::TempDirScope { .. }
            | ArenaExprKind::ResourceScope { .. } => true,
            ArenaExprKind::Require { schema, .. } => {
                schema.is_none()
                    || (removing_annotation
                        && self
                            .requirement_targets
                            .get(&expression.span)
                            .is_some_and(|target| {
                                self.requirement_expected_targets.get(&expression.span)
                                    == Some(target)
                            }))
                    || expr_child_exprs(self.arena, expr).into_iter().any(|child| {
                        self.expression_depends_on_expected_type(child, removing_annotation)
                    })
            }
            ArenaExprKind::Call { callee, .. } => {
                // Err has no success value from which to infer its domain; Ok
                // obtains a nondefault error domain only from its boundary.
                matches!(self.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "Err"
                    || (name == "Ok" && matches!(self.expr_types.get(&expression.span), Some(Type::Result(_, error)) if error.as_ref() != &Type::Error)))
                    || expr_child_exprs(self.arena, expr).into_iter().any(|child| {
                        self.expression_depends_on_expected_type(child, removing_annotation)
                    })
            }
            _ => expr_child_exprs(self.arena, expr)
                .into_iter()
                .any(|child| self.expression_depends_on_expected_type(child, removing_annotation)),
        }
    }

    pub(super) fn annotation_refs_user_type(&self, ty: TypeExprId) -> bool {
        self.type_expr_refs_user_type(ty)
    }

    pub(super) fn type_expr_refs_user_type(&self, ty: TypeExprId) -> bool {
        match type_expr_kind(self.arena, ty) {
            ArenaTypeExprKind::Named(name) => self.user_type_names.contains(name.as_str().as_str()),
            ArenaTypeExprKind::List(inner)
            | ArenaTypeExprKind::Stream(inner)
            | ArenaTypeExprKind::Module(inner)
            | ArenaTypeExprKind::Optional(inner) => self.type_expr_refs_user_type(inner),
            ArenaTypeExprKind::Map(key, value) => {
                key.is_some_and(|key| self.type_expr_refs_user_type(key))
                    || self.type_expr_refs_user_type(value)
            }
            ArenaTypeExprKind::Result { ok, err } => {
                self.type_expr_refs_user_type(ok)
                    || err.is_some_and(|err| self.type_expr_refs_user_type(err))
            }
            // Qualified and applied schemas retain a named declaring domain
            // that cannot be established by comparing their structural shape.
            ArenaTypeExprKind::Qualified => true,
        }
    }

    pub(super) fn is_empty_collection(&self, init: &ArenaExpr) -> bool {
        matches!(&init.kind, ArenaExprKind::List(items) if items.is_empty())
            || matches!(&init.kind, ArenaExprKind::Record(fields) if fields.is_empty())
    }

    pub(super) fn lint_redundant_tail_return(&mut self, body: BlockId, expected: &Type) {
        if expected == &Type::Unit || expected.is_result_unit() {
            return;
        }
        let Some(tail) = self
            .arena
            .stmt_ids(self.arena.block(body).statements)
            .last()
        else {
            return;
        };
        let stmt = self.arena.stmt(tail);
        match stmt.kind {
            ArenaStmtKind::If {
                branches,
                else_block: Some(other),
            } => {
                for branch in self.arena.if_branches(branches).to_vec() {
                    self.lint_redundant_tail_return(branch.block, expected);
                }
                self.lint_redundant_tail_return(other, expected);
            }
            ArenaStmtKind::Match { arms, .. } => {
                for arm in self.arena.match_arms(arms).to_vec() {
                    self.lint_redundant_tail_return(arm.block, expected);
                }
            }
            ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(value))) => {
                if self.statement_positions.get(&stmt.span)
                    != Some(&xsh::frontend::check::StatementPosition::Value)
                {
                    return;
                }
                let value_span = self.arena.expr(value).span;
                if !self
                    .expr_types
                    .get(&value_span)
                    .is_some_and(|ty| tail_type_matches_lint_expected(expected, ty))
                {
                    return;
                }
                let start = stmt.span.start();
                let Some(text) = self.source.get(start..value_span.start()) else {
                    return;
                };
                let Some(after_return) = text.strip_prefix("return") else {
                    return;
                };
                let whitespace = after_return.len() - after_return.trim_start().len();
                let prefix = Span::new(
                    stmt.span.source_id,
                    start,
                    start + "return".len() + whitespace,
                );
                if self
                    .diagnostics
                    .iter()
                    .flat_map(|diagnostic| &diagnostic.fix_hints)
                    .filter_map(|fix| fix.span)
                    .any(|span| {
                        span.source_id == prefix.source_id
                            && span.start() < prefix.end()
                            && prefix.start() < span.end()
                    })
                {
                    return;
                }
                if self
                    .source
                    .get(prefix.range())
                    .is_none_or(|text| text.contains('#'))
                {
                    return;
                }
                let (replacement_span, replacement) =
                    if matches!(self.arena.expr(value).kind, ArenaExprKind::Record(_)) {
                        // A bare opening brace after a match arm introduces a body.
                        // Group literal records so removing return preserves an expression.
                        (
                            Span::new(stmt.span.source_id, start, value_span.end()),
                            format!("({})", &self.source[value_span.range()]),
                        )
                    } else {
                        (prefix, String::new())
                    };
                self.diagnostics.push(
                    Diagnostic::warning("tail return can supply its value implicitly")
                        .with_code(DiagnosticCode::LintRedundantTailReturn)
                        .with_label(Label::primary(prefix, "remove the tail return"))
                        .with_fix_hint(FixHint::replacement(
                            replacement_span,
                            "use the tail value",
                            replacement,
                        )),
                );
            }
            _ => {}
        }
    }

    pub(super) fn lint_redundant_tail_return_binding(&mut self, body: BlockId) {
        let stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(body).statements)
            .collect();
        if stmts.len() < 2 {
            return;
        }
        let len = stmts.len();
        let binding_stmt = self.arena.stmt(stmts[len - 2]);
        let return_stmt = self.arena.stmt(stmts[len - 1]);
        let (target, ty, initializer) = match binding_stmt.kind {
            ArenaStmtKind::Let {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(initializer),
            }
            | ArenaStmtKind::Const {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(initializer),
            }
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(initializer),
            } => (target, ty, initializer),
            _ => return,
        };
        let ArenaBindingTargetKind::Name(binding_name) = self.arena.binding_target(target).kind
        else {
            return;
        };
        let ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(returned))) = return_stmt.kind else {
            return;
        };
        if !matches!(self.arena.expr(returned).kind, ArenaExprKind::Ident(name) if name == binding_name)
        {
            return;
        }
        if self.annotation_is_record_type(ty) {
            return;
        }
        // A bare conditional at statement position is parsed as control flow,
        // so replacing an explicit return would lose the returned value.
        if matches!(
            self.arena.expr(initializer).kind,
            ArenaExprKind::If { .. }
                | ArenaExprKind::Match { .. }
                | ArenaExprKind::PatternTest { .. }
                | ArenaExprKind::PatternCondition { .. }
                | ArenaExprKind::Binary {
                    op: BinaryOp::ResultFallback,
                    ..
                }
        ) {
            return;
        }
        let initializer_span = self.arena.expr(initializer).span;
        let replacement = match self.source.get(initializer_span.range()) {
            Some(source) => format!("{source}\n"),
            None => return,
        };
        let replacement_span = Span::new(
            binding_stmt.span.source_id,
            binding_stmt.span.start(),
            span_end_after_following_newlines(self.source, return_stmt.span.end()),
        );
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            format!("tail binding `{binding_name}` can be returned implicitly"),
        )
        .with_code(DiagnosticCode::LintRedundantTailReturnBinding)
        .with_label(Label::secondary(
            return_stmt.span,
            "make the initializer the final expression",
        ));
        let between = &self.source[binding_stmt.span.end()..return_stmt.span.start()];
        if !between.contains('#') && self.tail_return_binding_autofix_safe(ty, initializer) {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                replacement_span,
                "replace binding and return with tail expression",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    pub(super) fn annotation_is_record_type(&self, annotation: Option<TypeExprId>) -> bool {
        let Some(annotation) = annotation else {
            return false;
        };
        match type_expr_kind(self.arena, annotation) {
            ArenaTypeExprKind::Named(name) => {
                self.record_type_names.contains(name.as_str().as_str())
            }
            _ => false,
        }
    }

    pub(super) fn tail_return_binding_autofix_safe(
        &self,
        annotation: Option<TypeExprId>,
        initializer: ExprId,
    ) -> bool {
        let Some(annotation) = annotation else {
            return true;
        };
        let expected = Type::from_arena(self.arena, annotation);
        if matches!(expected, Type::Any | Type::Unknown | Type::Invalid) {
            return false;
        }
        if self.expr_is_source_empty_list(initializer)
            && matches!(expected, Type::List(_))
            && self
                .function_return_types
                .last()
                .is_some_and(|return_ty| tail_type_matches_lint_expected(return_ty, &expected))
        {
            return true;
        }
        let initializer_span = self.arena.expr(initializer).span;
        let Some(actual) = self.expr_types.get(&initializer_span) else {
            return false;
        };
        actual.matches_expected(&expected) && expected.matches_expected(actual)
    }

    pub(super) fn expr_is_source_empty_list(&self, expr: ExprId) -> bool {
        let arena_expr = self.arena.expr(expr);
        matches!(&arena_expr.kind, ArenaExprKind::List(items) if items.is_empty())
            || self
                .source
                .get(arena_expr.span.range())
                .is_some_and(|source| source.trim() == "[]")
    }

    pub(super) fn lint_return_value(&mut self, value: &ArenaExprOrRun) {
        if !self.result_unit_functions.last().copied().unwrap_or(false)
            || !return_value_is_ok_unit(self.arena, value)
        {
            return;
        }
        let val_span = self.expr_or_run_span(value);
        // Include the whitespace before `Ok()` so deletion leaves a clean `return`.
        let deletion_start = scan_back_space(self.source, val_span.start());
        let deletion_span = Span::new(val_span.source_id, deletion_start, val_span.end());
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "redundant `return Ok()` in `Result[Unit]` function",
            )
            .with_code(DiagnosticCode::LintRedundantOkReturn)
            .with_label(Label::secondary(
                val_span,
                "use bare `return`, or omit the final return",
            ))
            .with_fix_hint(FixHint::deletion(deletion_span, "remove `Ok()`")),
        );
    }

    pub(super) fn lint_tail_redundant_ok_return(&mut self, body: BlockId, expected_ok: Option<&Type>) {
        let stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(body).statements)
            .collect();
        let Some(&last_id) = stmts.last() else {
            return;
        };
        let last_stmt = self.arena.stmt(last_id);
        let ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(expr))) = last_stmt.kind else {
            return;
        };
        let expr_span = self.arena.expr(expr).span;
        let Some(ok_expr) = ok_call_arg(self.arena, expr) else {
            return;
        };
        if matches!(
            self.arena.expr(ok_expr).kind,
            ArenaExprKind::If { .. }
                | ArenaExprKind::Match { .. }
                | ArenaExprKind::PatternTest { .. }
                | ArenaExprKind::PatternCondition { .. }
                | ArenaExprKind::Binary {
                    op: BinaryOp::ResultFallback,
                    ..
                }
        ) {
            return;
        }
        // Get the full source text of the return value by slicing from
        // `return Ok(` to the matching `)` at the end, then stripping the
        // `return ` prefix for the tail expression. Using the enclosing
        // statement span keeps grouping parens that the expression AST
        // drops (e.g. `Ok((n * 3) + 1)` → `(n * 3) + 1`).
        let replacement = match self.source.get(last_stmt.span.range()) {
            Some(stmt_source) => {
                let inner = stmt_source
                    .strip_prefix("return Ok(")
                    .and_then(|rest| rest.strip_suffix(")\n").or_else(|| rest.strip_suffix(")")));
                match inner {
                    Some(inner) => format!("{inner}\n"),
                    None => return,
                }
            }
            None => return,
        };
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "redundant `return Ok(...)` at function tail",
        )
        .with_code(DiagnosticCode::LintRedundantOkTail)
        .with_label(Label::secondary(
            expr_span,
            "plain tail values are wrapped in `Ok(...)` automatically",
        ));
        if self.tail_ok_return_autofix_safe(expected_ok, ok_expr) {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                Span::new(
                    last_stmt.span.source_id,
                    last_stmt.span.start(),
                    span_end_after_following_newlines(self.source, last_stmt.span.end()),
                ),
                "use the plain tail value",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    pub(super) fn tail_ok_return_autofix_safe(&self, expected_ok: Option<&Type>, ok_expr: ExprId) -> bool {
        let Some(expected_ok) = expected_ok else {
            return false;
        };
        let ok_span = self.arena.expr(ok_expr).span;
        let Some(actual) = self.expr_types.get(&ok_span) else {
            return false;
        };
        actual.matches_expected(expected_ok) && expected_ok.matches_expected(actual)
    }
}
