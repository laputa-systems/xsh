use xsh::frontend::syntax::arena::ArenaChild;

use super::{
    rules,
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaBuilderEntryKind, ArenaCallArg,
    ArenaCallArgKind, ArenaCommand, ArenaCommandArg, ArenaCommandArgKind, ArenaCompQualifier,
    ArenaEnvAssignment, ArenaEnvAssignmentValue, ArenaExprKind, ArenaExprOrRun, ArenaFmtPart,
    ArenaMatchExprArm, ArenaModuleContractEntryKind, ArenaPatternKind, ArenaPipeStage,
    ArenaPipeStageKind, ArenaRange, ArenaRecordField, ArenaRecordFieldKind, ArenaRedirection,
    ArenaRedirectionTarget, ArenaSpawnTarget, ArenaStmtKind, ArenaStreamStage, ArenaSugar,
    ArenaSugarOperand, ArenaTypeDefBody, ArenaTypeExprKind, ArenaTypeExprTag, ArenaWordPart,
    AssignOp, AssignTargetId, BinaryOp, Binding, BindingTargetId, BlockId, BuilderBlockId,
    CallableEdgeOwner,
    CommandStmtId, Diagnostic, DiagnosticCode, ExprId, FixHint, FunctionDefId, FxHashMap, Label,
    Linter, Name, PathDisplaySite, PatternId, RunFormId, Severity, Span, StmtId, SugarForm,
    Type, TypeExprId, UnaryOp, bare_command_word_parts, command_value_replacement,
    insertion_sort_by,
    lint_inferred_variant_pattern,
    lint_prefer_propagation,
    parse_command_word_reference,
    result_ok_type_expr, result_path_type_expr, result_unit_type_expr,
    simple_command_value_expr, type_expr_kind,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CallableVisibility {
    Exported,
    Private,
}

impl CallableVisibility {
    pub(super) fn from_exported(exported: bool) -> Self {
        if exported {
            Self::Exported
        } else {
            Self::Private
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BindingOrigin {
    Local,
    FunctionParameter { visibility: CallableVisibility },
}

pub(super) struct LintExprVisitor<'a, 'b> {
    linter: &'a mut Linter<'b>,
    suppress_expr_autofixes: bool,
}

impl LintExprVisitor<'_, '_> {
    fn visit_comp_qualifiers(&mut self, range: ArenaRange) -> usize {
        let mut scopes = 0;
        for qualifier in self.linter.arena.comp_qualifiers(range).to_vec() {
            if let ArenaCompQualifier::For { iter, .. } = qualifier {
                rules::scalar_iteration(self.linter, iter);
            }
            self.visit_expr(qualifier.expr());
            if let ArenaCompQualifier::For { target, .. } = qualifier {
                self.linter.push_scope();
                scopes += 1;
                self.linter
                    .define_binding_target(target, qualifier.span(), false);
            }
        }
        scopes
    }

    fn visit_expr(&mut self, expr: ExprId) {
        self.linter.propagation_boundary_depth += 1;
        self.visit_expression(expr);
        self.linter.propagation_boundary_depth -= 1;
    }

    fn visit_expression(&mut self, expr: ExprId) {
        rules::expression_analysis(self.linter, expr);
        if !self.suppress_expr_autofixes {
            rules::expression(self.linter, expr);
            rules::expression_group(self.linter, rules::ExpressionGroupSite::SingleInterpolation, expr);
            rules::expression_rule(self.linter, DiagnosticCode::LintRedundantDisplayParse, expr);
            rules::expression_rule(self.linter, DiagnosticCode::LintJsonRoundtrip, expr);
            rules::expression_rule(self.linter, DiagnosticCode::LintPreferInferredVariant, expr);
            rules::expression_rule(self.linter, DiagnosticCode::LintPreferPositionalConstructor, expr);
            rules::expression_group(self.linter, rules::ExpressionGroupSite::PathMigrations, expr);
        }
        let arena_expr = self.linter.arena.expr(expr);
        match arena_expr.kind {
            ArenaExprKind::Ident(name) => {
                if let Some(edges) = &mut self.linter.callable_edges {
                    edges.dynamic_name(name, Some(expr));
                }
                self.linter.mark_used(name.as_str().as_str());
                if rules::enabled(self.linter, DiagnosticCode::LintPreferTypedCallable) {
                    let hidden = self.linter.local_hides(name);
                    self.linter.callable_parameters.value(expr, name, hidden);
                }
            }
            ArenaExprKind::Call { callee, args } => {
                if let Some(edges) = &mut self.linter.callable_edges {
                    match self.linter.arena.expr(callee).kind {
                        ArenaExprKind::Ident(name) => {
                            edges.direct_name(name, Some(expr));
                        }
                        ArenaExprKind::Field { base, name }
                            if let ArenaExprKind::Ident(alias) =
                                self.linter.arena.expr(base).kind =>
                        {
                            edges.direct_qualified(alias, name, expr);
                        }
                        _ => {}
                    }
                }
                if rules::enabled(self.linter, DiagnosticCode::LintPreferTypedCallable) {
                    let mut parameters = std::mem::take(&mut self.linter.callable_parameters);
                    parameters.call(self.linter.arena, callee, args, &|name| {
                        self.linter.local_hides(name)
                    });
                    self.linter.callable_parameters = parameters;
                }
                if self
                    .linter
                    .record_constructors
                    .resolve_call(self.linter.arena, callee, None)
                    .is_some()
                    && let ArenaExprKind::Ident(name) = self.linter.arena.expr(callee).kind
                {
                    self.linter
                        .used_type_names
                        .insert(name.as_str().to_string());
                }
                self.walk_expr(expr);
                rules::expression_group(self.linter, rules::ExpressionGroupSite::CallStyle, expr);
            }
            ArenaExprKind::Field { base, name } | ArenaExprKind::NullSafeField { base, name } => {
                if let ArenaExprKind::Ident(alias) = self.linter.arena.expr(base).kind
                    && let Some(edges) = &mut self.linter.callable_edges
                {
                    edges.dynamic_qualified(alias, name, base);
                }
                self.walk_expr(expr);
            }
            _ => self.walk_expr(expr),
        }
    }

    // These forms have no child-specific scope transitions. Callbacks,
    // patterns, stages, and command arguments enter their explicit contexts.
    fn walk_immediate_children(&mut self, expr: ExprId) {
        self.linter.arena.for_each_expr_child(expr, |child| match child {
            ArenaChild::Expr(child) => self.visit_expr(child),
            ArenaChild::Block(block) => self.linter.lint_block(block),
            ArenaChild::TypeExpr(ty) => self.linter.collect_type_expr_refs(ty),
            ArenaChild::BuilderBlock(block) => self.visit_builder_block(block),
            ArenaChild::Stmt(_)
            | ArenaChild::Pattern(_)
            | ArenaChild::BindingTarget(_)
            | ArenaChild::AssignTarget(_) => unreachable!("scope-free expression has a scoped child"),
        });
    }

    fn walk_expr(&mut self, expr: ExprId) {
        let arena = self.linter.arena;
        match arena.expr(expr).kind {
            ArenaExprKind::ValuePipelineCall { .. }
            | ArenaExprKind::List(_)
            | ArenaExprKind::Set(_)
            | ArenaExprKind::Unary { .. }
            | ArenaExprKind::Try(_)
            | ArenaExprKind::ComparisonChain(_)
            | ArenaExprKind::Index { .. }
            | ArenaExprKind::Slice { .. }
            | ArenaExprKind::Wait(_)
            | ArenaExprKind::BuilderCall { .. }
            | ArenaExprKind::Require { .. }
            | ArenaExprKind::Convert { .. }
            | ArenaExprKind::ErrorContext { .. }
            | ArenaExprKind::ContextScope { .. }
            | ArenaExprKind::Loop { .. }
            | ArenaExprKind::Collect { .. } => self.walk_immediate_children(expr),

            ArenaExprKind::ResourceScope {
                bindings, block, ..
            } => {
                if let Some(edges) = &mut self.linter.callable_edges {
                    edges.push_scope();
                }
                for binding in arena.with_bindings(bindings).to_vec() {
                    self.visit_expr(binding.initializer);
                    if binding.name.as_str() != "_"
                        && let Some(edges) = &mut self.linter.callable_edges
                    {
                        edges.define(binding.name);
                    }
                }
                self.linter.lint_block(block);
                if let Some(edges) = &mut self.linter.callable_edges {
                    edges.pop_scope();
                }
            }

            ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
                for part in arena.fmt_parts(parts).collect::<Vec<_>>() {
                    if let ArenaFmtPart::Expr(e, spec) = part {
                        if spec.is_none() {
                            rules::expression_site(
                                self.linter,
                                rules::ExpressionSite::PathDisplay,
                                e,
                                PathDisplaySite::Interpolation,
                            );
                        }
                        self.visit_expr(e);
                    }
                }
            }
            ArenaExprKind::ListComp { expr, qualifiers }
            | ArenaExprKind::SetComp { expr, qualifiers } => {
                let scopes = self.visit_comp_qualifiers(qualifiers);
                self.visit_expr(expr);
                for _ in 0..scopes {
                    self.linter.pop_scope();
                }
            }
            ArenaExprKind::MapComp {
                key,
                value,
                qualifiers,
            } => {
                let scopes = self.visit_comp_qualifiers(qualifiers);
                self.visit_expr(key);
                self.visit_expr(value);
                for _ in 0..scopes {
                    self.linter.pop_scope();
                }
            }
            ArenaExprKind::Record(fields) => {
                if !self.suppress_expr_autofixes {
                    rules::expression_rule(
                        self.linter,
                        DiagnosticCode::LintPreferBareFieldLabel,
                        expr,
                    );
                }
                for field in arena.record_fields(fields).to_vec() {
                    self.visit_record_field(&field);
                }
            }
            ArenaExprKind::If {
                branches,
                else_value,
            } => {
                for branch in arena.if_expr_branches(branches).to_vec() {
                    if let ArenaExprKind::PatternCondition { value, arms } =
                        arena.expr(branch.condition).kind
                    {
                        self.visit_expr(value);
                        self.linter.push_scope();
                        self.linter
                            .lint_pattern(arena.match_expr_arms(arms)[0].pattern);
                        self.visit_expr(branch.value);
                        self.linter.pop_scope();
                    } else {
                        self.visit_expr(branch.condition);
                        self.visit_expr(branch.value);
                    }
                }
                self.visit_expr(else_value);
            }
            ArenaExprKind::PatternCondition { value, .. } => self.visit_expr(value),
            ArenaExprKind::Match { value, arms } | ArenaExprKind::PatternTest { value, arms } => {
                let old = self.linter.regex_recovery_context;
                self.linter.regex_recovery_context = true;
                self.visit_expr(value);
                let is_match = matches!(arena.expr(expr).kind, ArenaExprKind::Match { .. });
                for arm in arena.match_expr_arms(arms).to_vec() {
                    if is_match {
                        self.linter.note_match_arm_head(arm.pattern);
                    }
                    self.visit_match_expr_arm(&arm);
                }
                self.linter.regex_recovery_context = old;
            }
            ArenaExprKind::Binary { op, left, right } => {
                let old = self.linter.regex_recovery_context;
                self.linter.regex_recovery_context |= op == BinaryOp::ResultFallback;
                self.visit_expr(left);
                self.visit_expr(right);
                self.linter.regex_recovery_context = old;
            }
            ArenaExprKind::Call { callee, args } => {
                let direct_shape = match arena.expr(callee).kind {
                    ArenaExprKind::Ident(_) => true,
                    ArenaExprKind::Field { base, .. } => {
                        matches!(arena.expr(base).kind, ArenaExprKind::Ident(_))
                    }
                    _ => false,
                };
                if let Some(edges) = &mut self.linter.callable_edges {
                    edges.enter_call_callee(expr, direct_shape);
                }
                self.visit_expr(callee);
                if let Some(edges) = &mut self.linter.callable_edges {
                    edges.leave_call_callee();
                }
                for arg in arena.call_args(args).to_vec() {
                    self.visit_call_arg(&arg);
                }
            }
            ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
                self.visit_expr(base);
            }
            ArenaExprKind::Pipeline { input, stages } => {
                self.visit_expr(input);
                for stage in arena.pipe_stages(stages).to_vec() {
                    self.visit_pipe_stage(&stage);
                }
            }
            ArenaExprKind::StructuredPipeline { input, stages } => {
                self.visit_expr(input);
                for stage in arena.stream_stages(stages).to_vec() {
                    self.visit_stream_stage(&stage);
                }
                // fs.walk(X) |> where .kind == "file" → fs.files(X)
                rules::structured_pipeline(self.linter, input, stages);
            }
            ArenaExprKind::Run(run) => self.visit_run_form(run),
            ArenaExprKind::Spawn(form) => match form.target {
                ArenaSpawnTarget::Run(run) => self.visit_run_form(run),
                ArenaSpawnTarget::Command(expr) => self.visit_expr(expr),
            },
            ArenaExprKind::Capture(block) | ArenaExprKind::ValueBlock(block) => {
                let capturing = matches!(arena.expr(expr).kind, ArenaExprKind::Capture(_));
                if capturing {
                    self.linter.assertion_capture_depth += 1;
                }
                self.linter.push_scope();
                for parameter in arena.block_params(arena.block(block).params) {
                    self.linter.define(
                        parameter.name.as_str().as_str(),
                        arena.span(parameter.span),
                        true,
                    );
                }
                self.linter.lint_block_statements(block);
                self.linter.pop_scope();
                if capturing {
                    self.linter.assertion_capture_depth -= 1;
                }
            }
            // The directory name is the block's parameter, in scope for the
            // body and not for the path.
            ArenaExprKind::TempDirScope { path, block, .. } => {
                if let Some(path) = path {
                    self.visit_expr(path);
                    // A body may reach a directory at a path the program
                    // chose through that path, so its name is not an unused
                    // binding.
                    self.linter.push_scope();
                    for parameter in arena.block_params(arena.block(block).params) {
                        self.linter.define(
                            parameter.name.as_str().as_str(),
                            arena.span(parameter.span),
                            false,
                        );
                    }
                    self.linter.lint_block_statements(block);
                    self.linter.pop_scope();
                } else {
                    self.linter.lint_stream_block(block);
                }
            }
            ArenaExprKind::Retry {
                schedule: _,
                delays,
                pattern,
                block,
            } => {
                if let Some(pattern) = pattern {
                    self.linter.lint_pattern(pattern);
                }
                let old = self.linter.regex_recovery_context;
                self.linter.regex_recovery_context = true;
                for delay in arena.expr_ids(delays).collect::<Vec<_>>() {
                    self.visit_expr(delay);
                }
                self.linter.assertion_capture_depth += 1;
                self.linter.lint_block(block);
                self.linter.assertion_capture_depth -= 1;
                self.linter.regex_recovery_context = old;
            }
            ArenaExprKind::Str(_) => {
                rules::expression_rule(
                    self.linter,
                    DiagnosticCode::LintDollarInExpressionString,
                    expr,
                );
                rules::expression_rule(self.linter, DiagnosticCode::LintMissingFPrefix, expr);
                if !self.suppress_expr_autofixes {
                    rules::expression_rule(
                        self.linter,
                        DiagnosticCode::LintRedundantNewlineTripleString,
                        expr,
                    );
                }
            }
            ArenaExprKind::PathStr(_) => {
                rules::expression_rule(self.linter, DiagnosticCode::LintMissingFPrefix, expr)
            }
            ArenaExprKind::Null
            | ArenaExprKind::Bool(_)
            | ArenaExprKind::Int(_)
            | ArenaExprKind::Float(_)
            | ArenaExprKind::Duration(_)
            | ArenaExprKind::GlobStr(_)
            | ArenaExprKind::Bytes(_)
            | ArenaExprKind::Regex(_)
            | ArenaExprKind::Ident(_)
            | ArenaExprKind::EnvString(_)
            | ArenaExprKind::EnvPathList
            | ArenaExprKind::Item
            | ArenaExprKind::LastStatus => {}
        }
    }

    fn visit_record_field(&mut self, field: &ArenaRecordField) {
        rules::record_field(self.linter, field);
        match field.kind {
            ArenaRecordFieldKind::Shorthand { name, .. } => {
                if let Some(edges) = &mut self.linter.callable_edges {
                    edges.dynamic_name(name, None);
                }
                self.linter.mark_used(name.as_str().as_str())
            }
            ArenaRecordFieldKind::Computed { key, value, .. } => {
                self.visit_expr(key);
                self.visit_expr(value);
            }
            ArenaRecordFieldKind::Named { value, .. }
            | ArenaRecordFieldKind::Path { value, .. } => self.visit_expr(value),
            ArenaRecordFieldKind::Spread { expr, .. } => self.visit_expr(expr),
        }
    }

    fn visit_match_expr_arm(&mut self, arm: &ArenaMatchExprArm) {
        self.linter.push_scope();
        self.linter.lint_pattern(arm.pattern);
        if let Some(guard) = arm.guard {
            self.visit_expr(guard);
        }
        self.visit_expr(arm.value);
        self.linter.pop_scope();
    }

    fn visit_call_arg(&mut self, arg: &ArenaCallArg) {
        if !self.suppress_expr_autofixes {
            rules::call_argument(self.linter, arg);
        }
        match arg.kind {
            ArenaCallArgKind::Positional(expr) | ArenaCallArgKind::Named { value: expr, .. } => {
                self.visit_expr(expr);
            }
            ArenaCallArgKind::Splice { value, .. }
            | ArenaCallArgKind::NamedSpread { value, .. } => self.visit_expr(value),
        }
    }

    fn visit_pipe_stage(&mut self, stage: &ArenaPipeStage) {
        match stage.kind {
            ArenaPipeStageKind::Expr(expr) => self.visit_expr(expr),
            ArenaPipeStageKind::Stream(ref stage) => self.visit_stream_stage(stage),
        }
    }

    fn visit_command_arg(&mut self, arg: &ArenaCommandArg, allow_bare_refs: bool) {
        let arg_span = self.linter.arena.span(arg.span);
        match arg.kind {
            ArenaCommandArgKind::SpliceName(name) => {
                if let Some(edges) = &mut self.linter.callable_edges {
                    edges.dynamic_name(name, None);
                }
                self.linter.mark_used(name.as_str().as_str());
            }
            ArenaCommandArgKind::Word(parts) => {
                if rules::command_argument_enabled(
                    self.linter,
                    rules::CommandArgumentHook::RedundantInterpolation,
                ) {
                    self.linter.lint_redundant_command_arg_interpolation(arg);
                }
                let part_list: Vec<ArenaWordPart> = self.linter.arena.word_parts(parts).collect();
                if allow_bare_refs
                    && let Some(text) =
                        bare_command_word_parts(
                            self.linter.source_id,
                            self.linter.source,
                            &part_list,
                        )
                    && let Some((root, _)) = parse_command_word_reference(&text)
                {
                    self.linter.mark_used(root);
                }
                // ${f"..."} is a Word with a single Interpolation whose expr is an FmtString.
                // Since f"..." is now accepted directly as a typed command arg, the wrapper is
                // redundant.
                if rules::command_argument_enabled(
                    self.linter,
                    rules::CommandArgumentHook::FormatWrapper,
                ) && let [ArenaWordPart::Interpolation(expr)] = part_list.as_slice()
                    && matches!(
                        self.linter.arena.expr(*expr).kind,
                        ArenaExprKind::FmtString(_)
                    )
                    && self.linter.source.as_bytes().get(arg_span.start()) == Some(&b'$')
                {
                    let expr_span = self.linter.arena.expr(*expr).span;
                    let replacement =
                        self.linter.source[expr_span.start()..expr_span.end()].to_string();
                    self.linter.diagnostics.push(
                        Diagnostic::new(Severity::Warning, "redundant `${}` around f-string")
                            .with_code(DiagnosticCode::LintRedundantFmtWrapper)
                            .with_span(arg_span)
                            .with_fix_hint(FixHint::replacement(
                                arg_span,
                                "remove the `${}` wrapper",
                                replacement,
                            )),
                    );
                }
                for part in &part_list {
                    if let ArenaWordPart::Interpolation(expr) | ArenaWordPart::Shorthand(expr) =
                        *part
                    {
                        rules::expression_site(
                            self.linter,
                            rules::ExpressionSite::PathDisplay,
                            expr,
                            PathDisplaySite::Interpolation,
                        );
                        if matches!(part, ArenaWordPart::Interpolation(_)) {
                            self.visit_command_delimited_expr(expr);
                        } else {
                            self.visit_command_embedded_expr(expr);
                        }
                    }
                }
            }
            ArenaCommandArgKind::SpliceExpr(expr) => {
                rules::expression_site(
                    self.linter,
                    rules::ExpressionSite::PathDisplay,
                    expr,
                    PathDisplaySite::Splice,
                );
                self.visit_command_delimited_expr(expr);
            }
            ArenaCommandArgKind::Typed(expr) => {
                let expr_span = self.linter.arena.expr(expr).span;
                rules::expression_site(
                    self.linter,
                    rules::ExpressionSite::PathDisplay,
                    expr,
                    PathDisplaySite::CommandWord(arg_span),
                );
                // (f"...") → f"..." : f-strings don't need paren wrapping
                if matches!(
                    self.linter.arena.expr(expr).kind,
                    ArenaExprKind::FmtString(_) | ArenaExprKind::PathFmtString(_)
                ) && self.linter.source.as_bytes().get(arg_span.start()) == Some(&b'(')
                {
                    if rules::command_argument_enabled(
                        self.linter,
                        rules::CommandArgumentHook::FormatWrapper,
                    ) {
                        let replacement =
                            self.linter.source[expr_span.start()..expr_span.end()].to_string();
                        self.linter.diagnostics.push(
                            Diagnostic::new(Severity::Warning, "redundant `()` around f-string")
                                .with_code(DiagnosticCode::LintRedundantFmtWrapper)
                                .with_span(arg_span)
                                .with_fix_hint(FixHint::replacement(
                                    arg_span,
                                    "remove the `()` wrapper",
                                    replacement,
                                )),
                        );
                    }
                } else if rules::command_argument_enabled(
                    self.linter,
                    rules::CommandArgumentHook::CommandFormat,
                ) && let Some(replacement) = self.linter.command_single_fmt_replacement(expr) {
                    self.linter.diagnostics.push(
                        Diagnostic::new(
                            Severity::Warning,
                            "redundant single-value command f-string",
                        )
                        .with_code(DiagnosticCode::LintRedundantCommandFmt)
                        .with_label(Label::secondary(
                            expr_span,
                            "use command value syntax directly",
                        ))
                        .with_fix_hint(FixHint::replacement(
                            arg_span,
                            "use command value syntax",
                            replacement,
                        )),
                    );
                } else if rules::command_argument_enabled(
                    self.linter,
                    rules::CommandArgumentHook::CommandValue,
                ) && simple_command_value_expr(self.linter.arena, expr) {
                    let replacement = command_value_replacement(self.linter.arena, expr);
                    self.linter.diagnostics.push(
                        Diagnostic::new(
                            Severity::Warning,
                            "redundant parentheses around a command value",
                        )
                        .with_code(DiagnosticCode::LintCommandValue)
                        .with_label(Label::secondary(arg_span, "a name or field path takes `$`"))
                        .with_fix_hint(FixHint::replacement(
                            arg_span,
                            "write it with `$`",
                            replacement,
                        )),
                    );
                }
                self.visit_command_delimited_expr(expr);
            }
        }
    }

    /// An expression a command argument holds: `(EXPR)`, `${EXPR}`,
    /// `@(EXPR)`, or the same expression written without the delimiters,
    /// which the formatter adds. It is linted like an expression anywhere
    /// else, and which findings it has depends only on the expression, never
    /// on how the argument is laid out.
    ///
    /// A literal is the exception: a bare `f"..."` argument and `(f"...")`
    /// are the same argument, the formatter prefers the bare one, and
    /// replacing a bare literal could turn it into a command word. Its
    /// rewrites stay off in both spellings.
    ///
    /// A rewrite is offered only where the delimiters are written, because
    /// only there can any expression stand in for the old one.
    fn visit_command_delimited_expr(&mut self, expr: ExprId) {
        let node = self.linter.arena.expr(expr);
        if matches!(
            node.kind,
            ArenaExprKind::Null
                | ArenaExprKind::Bool(_)
                | ArenaExprKind::Int(_)
                | ArenaExprKind::Float(_)
                | ArenaExprKind::Duration(_)
                | ArenaExprKind::Str(_)
                | ArenaExprKind::PathStr(_)
                | ArenaExprKind::GlobStr(_)
                | ArenaExprKind::FmtString(_)
                | ArenaExprKind::PathFmtString(_)
                | ArenaExprKind::Bytes(_)
                | ArenaExprKind::Regex(_)
        ) {
            return self.visit_command_embedded_expr(expr);
        }
        let start = node.span.start();
        let delimited = start > 0
            && matches!(
                self.linter.source.as_bytes().get(start - 1),
                Some(b'(' | b'{')
            );
        let reported = self.linter.diagnostics.len();
        let old = self.suppress_expr_autofixes;
        self.suppress_expr_autofixes = false;
        self.visit_expr(expr);
        self.suppress_expr_autofixes = old;
        if !delimited {
            for diagnostic in &mut self.linter.diagnostics[reported..] {
                diagnostic.fix_hints.clear();
            }
        }
    }

    fn visit_command_embedded_expr(&mut self, expr: ExprId) {
        let old = self.suppress_expr_autofixes;
        self.suppress_expr_autofixes = true;
        self.visit_expr(expr);
        self.suppress_expr_autofixes = old;
    }

    fn visit_run_form(&mut self, run: RunFormId) {
        let arena = self.linter.arena;
        let run_form = arena.run_form(run).clone();
        rules::run_checks(self.linter, run);
        for segment in arena.run_segments(run_form.segments).to_vec() {
            if let Some(timeout) = segment.timeout {
                self.visit_expr(timeout);
            }
            if let Some(cpu_max) = segment.cpu_max {
                self.visit_expr(cpu_max);
            }
            if let Some(accept) = segment.accept {
                self.visit_expr(accept);
            }
            for assignment in arena.env_assignments(segment.env).to_vec() {
                self.visit_env_assignment(&assignment);
            }
            self.visit_command_arg(&segment.target, false);
            for arg in arena.command_args(segment.args).to_vec() {
                self.visit_command_arg(&arg, false);
            }
            for redirection in arena.redirections(segment.redirections).to_vec() {
                self.visit_redirection(&redirection);
            }
        }
    }

    fn visit_env_assignment(&mut self, assignment: &ArenaEnvAssignment) {
        match assignment.value {
            ArenaEnvAssignmentValue::CommandArg(ref arg) => self.visit_command_arg(arg, true),
            ArenaEnvAssignmentValue::Expr(expr) => self.visit_expr(expr),
        }
    }

    fn visit_redirection(&mut self, redirection: &ArenaRedirection) {
        match redirection.target {
            ArenaRedirectionTarget::Path(ref arg) | ArenaRedirectionTarget::Fd(ref arg) => {
                self.visit_command_arg(arg, false);
            }
        }
    }

    fn visit_stream_stage(&mut self, stage: &ArenaStreamStage) {
        self.linter.lint_stream_stage(stage);
    }

    fn visit_builder_block(&mut self, block: BuilderBlockId) {
        self.linter.lint_builder_block(block);
    }
}

impl<'a> Linter<'a> {
    pub(super) fn collect_type_expr_refs(&mut self, ty: TypeExprId) {
        self.set_like_bindings
            .visit_type(self.arena, self.source, ty);
        if self.arena.type_expr_tags[ty.index()] == ArenaTypeExprTag::Applied {
            let base = TypeExprId::from_index(self.arena.type_expr_data[ty.index()].lhs as usize);
            self.collect_type_expr_refs(base);
            let arguments = self.arena.applied_type_arguments(ty).collect::<Vec<_>>();
            for argument in arguments {
                self.collect_type_expr_refs(argument);
            }
            return;
        }
        // The rules see a validated type and a set as opaque, but the
        // element type is still a reference to whatever it names.
        if matches!(
            self.arena.type_expr_tags[ty.index()],
            ArenaTypeExprTag::NonEmpty | ArenaTypeExprTag::Set
        ) {
            let inner = TypeExprId::from_index(self.arena.type_expr_data[ty.index()].lhs as usize);
            self.collect_type_expr_refs(inner);
            return;
        }
        // Likewise a union names its members and a callable type its
        // parameter and return types.
        if self.arena.type_expr_tags[ty.index()] == ArenaTypeExprTag::Union {
            let members = self.arena.union_type_members(ty).collect::<Vec<_>>();
            for member in members {
                self.collect_type_expr_refs(member);
            }
            return;
        }
        if self.arena.type_expr_tags[ty.index()] == ArenaTypeExprTag::Callable {
            let callable = self.arena.callable_type_expr(ty);
            for param in self.arena.params(callable.params).to_vec() {
                self.collect_type_expr_refs(param.ty);
            }
            self.collect_type_expr_refs(callable.return_ty);
            return;
        }
        match type_expr_kind(self.arena, ty) {
            ArenaTypeExprKind::Named(name) => {
                self.used_type_names.insert(name.to_string());
            }
            ArenaTypeExprKind::Qualified => {}
            ArenaTypeExprKind::List(inner)
            | ArenaTypeExprKind::Stream(inner)
            | ArenaTypeExprKind::Module(inner)
            | ArenaTypeExprKind::Optional(inner) => self.collect_type_expr_refs(inner),
            ArenaTypeExprKind::Map(key, value) => {
                if let Some(key) = key {
                    self.collect_type_expr_refs(key);
                }
                self.collect_type_expr_refs(value);
            }
            ArenaTypeExprKind::Result { ok, err } => {
                self.collect_type_expr_refs(ok);
                if let Some(err) = err {
                    self.collect_type_expr_refs(err);
                }
            }
        }
    }

    pub(super) fn collect_type_def_refs(&mut self, body: &ArenaTypeDefBody) {
        match body {
            ArenaTypeDefBody::Alias(ty) => self.collect_type_expr_refs(*ty),
            ArenaTypeDefBody::RecordSchema(fields) => {
                for field in self.arena.schema_fields(*fields).to_vec() {
                    self.collect_type_expr_refs(field.ty);
                    if let Some(default) = field.default {
                        self.lint_expr(default);
                    }
                }
            }
            ArenaTypeDefBody::ModuleContract { entries, .. } => {
                for entry in self.arena.module_contract_entries(*entries).to_vec() {
                    match &entry.kind {
                        ArenaModuleContractEntryKind::Value(ty) => self.collect_type_expr_refs(*ty),
                        ArenaModuleContractEntryKind::Proc {
                            params, return_ty, ..
                        }
                        | ArenaModuleContractEntryKind::Pure { params, return_ty } => {
                            for param in self.arena.params(*params).to_vec() {
                                self.collect_type_expr_refs(param.ty);
                            }
                            self.collect_type_expr_refs(*return_ty);
                        }
                    }
                }
            }
            ArenaTypeDefBody::TagUnion(variants) => {
                for variant in self.arena.tag_variants(*variants).to_vec() {
                    if let Some(value) = variant.wire_value {
                        self.lint_expr(value);
                    }
                    for ty in self.arena.extra_range(variant.fields).to_vec() {
                        self.collect_type_expr_refs(TypeExprId::from_index(ty as usize));
                    }
                }
            }
        }
    }

    /// Lints a proc or pure body as one whose statements may trade
    /// `return Err(e)` for `?`.
    pub(super) fn lint_propagating_function(
        &mut self,
        definition: FunctionDefId,
        pure: bool,
        lint: impl FnOnce(&mut Self),
    ) {
        let allowed = lint_prefer_propagation::propagation_allowed(
            self.arena,
            &self.checked_effects,
            definition,
            pure,
        );
        let function = self.propagation_function.replace(allowed);
        let depth = std::mem::take(&mut self.propagation_boundary_depth);
        lint(self);
        self.propagation_function = function;
        self.propagation_boundary_depth = depth;
    }

    pub(super) fn lint_stmt(&mut self, stmt_id: StmtId, exported: bool) {
        let previous_collection = self.assignment_collection_enabled;
        let collect_assignment = previous_collection && self.checked_output.is_none();
        let kind = self.arena.stmt(stmt_id).kind;
        self.assignment_collection_enabled = collect_assignment
            && match kind {
                ArenaStmtKind::Export(_)
                | ArenaStmtKind::Assign { .. }
                | ArenaStmtKind::ProcDef(_)
                | ArenaStmtKind::CliMain(_)
                | ArenaStmtKind::PureDef(_)
                | ArenaStmtKind::StreamDef(_)
                | ArenaStmtKind::SignalHook(_)
                | ArenaStmtKind::If { .. }
                | ArenaStmtKind::While { .. }
                | ArenaStmtKind::For { .. }
                | ArenaStmtKind::Loop { .. }
                | ArenaStmtKind::Sugar { .. }
                | ArenaStmtKind::With { .. }
                | ArenaStmtKind::Guard { .. }
                | ArenaStmtKind::Match { .. } => true,
                ArenaStmtKind::Use(_)
                | ArenaStmtKind::TypeDef(_)
                | ArenaStmtKind::ErrorDef(_)
                | ArenaStmtKind::Const { .. }
                | ArenaStmtKind::Let { .. }
                | ArenaStmtKind::Var { .. }
                | ArenaStmtKind::Return(_)
                | ArenaStmtKind::Yield(_)
                | ArenaStmtKind::YieldDelegate(_)
                | ArenaStmtKind::Defer(..)
                | ArenaStmtKind::Assert { .. }
                | ArenaStmtKind::Break { .. }
                | ArenaStmtKind::Continue
                | ArenaStmtKind::Exit(_)
                | ArenaStmtKind::Command(_)
                | ArenaStmtKind::TailBareIdent(_)
                | ArenaStmtKind::Expr(_) => false,
            };
        if collect_assignment
            && let ArenaStmtKind::Assign { target, .. } = kind
            && let Some(name) = xsh::frontend::check::assignment_target_root_name(self.program, target)
        {
            self.source_assignment_names.insert(name);
        }
        self.lint_stmt_inner(stmt_id, exported);
        self.assignment_collection_enabled = previous_collection;
    }

    fn lint_stmt_inner(&mut self, stmt_id: StmtId, exported: bool) {
        let stmt = self.arena.stmt(stmt_id);
        rules::statement(self, stmt_id, rules::StatementSite::PropagationContext, exported);
        rules::statement(self, stmt_id, rules::StatementSite::TempdirScopeCandidate, exported);
        rules::statement(self, stmt_id, rules::StatementSite::StreamProducerCandidates, exported);
        rules::statement(self, stmt_id, rules::StatementSite::WholeValuePipeline, exported);
        rules::statement(self, stmt_id, rules::StatementSite::DeferBlockHelperCall, exported);
        rules::statement(self, stmt_id, rules::StatementSite::Propagation, exported);
        rules::statement(self, stmt_id, rules::StatementSite::RedundantDiscard, exported);
        rules::statement(self, stmt_id, rules::StatementSite::FailCandidates, exported);
        rules::statement(self, stmt_id, rules::StatementSite::SetLikeBindings, exported);
        match stmt.kind {
            ArenaStmtKind::Use(_) | ArenaStmtKind::TypeDef(_) | ArenaStmtKind::ErrorDef(_) => {}
            ArenaStmtKind::Export(inner) => self.lint_stmt(inner, true),
            ArenaStmtKind::Let {
                target,
                ty,
                initializer,
            }
            | ArenaStmtKind::Const {
                target,
                ty,
                initializer,
            } => {
                rules::statement(self, stmt_id, rules::StatementSite::RecordConstructor, exported);
                rules::statement(self, stmt_id, rules::StatementSite::EmptyMapInitializer, exported);
                rules::statement(self, stmt_id, rules::StatementSite::SizeProductInitializer, exported);
                if let Some(type_expr) = ty {
                    self.collect_type_expr_refs(type_expr);
                    rules::statement(self, stmt_id, rules::StatementSite::NeedlessAnnotation, exported);
                }
                self.lint_expr_or_run(&initializer);
                let absence_lookup = match initializer {
                    ArenaExprOrRun::Expr(value) => self.proven_absence_lookup(value),
                    _ => false,
                };
                let materialized_record = match initializer {
                    ArenaExprOrRun::Expr(value) => self.proven_materialized_record(value),
                    _ => false,
                };
                let immutable_byte_length = match initializer {
                    ArenaExprOrRun::Expr(value) => self.proven_immutable_byte_length(value),
                    _ => None,
                };
                self.define_binding_target(target, stmt.span, true);
                rules::statement(self, stmt_id, rules::StatementSite::ListAnyBindings, exported);
                if let ArenaBindingTargetKind::Name(name) = self.arena.binding_target(target).kind
                    && let Some(binding) = self
                        .scopes
                        .last_mut()
                        .and_then(|scope| scope.get_mut(name.as_str().as_str()))
                {
                    binding.comparison_stable = true;
                    binding.absence_lookup = absence_lookup;
                    binding.materialized_record = materialized_record;
                    binding.immutable_byte_length = immutable_byte_length;
                }
            }
            ArenaStmtKind::Var {
                target,
                ty,
                initializer,
            } => {
                rules::statement(self, stmt_id, rules::StatementSite::RecordConstructor, exported);
                rules::statement(self, stmt_id, rules::StatementSite::EmptyMapInitializer, exported);
                rules::statement(self, stmt_id, rules::StatementSite::SizeProductInitializer, exported);
                if let Some(type_expr) = ty {
                    self.collect_type_expr_refs(type_expr);
                    rules::statement(self, stmt_id, rules::StatementSite::NeedlessAnnotation, exported);
                }
                self.lint_expr_or_run(&initializer);
                self.define_binding_target(target, stmt.span, true);
                rules::statement(self, stmt_id, rules::StatementSite::ListAnyBindings, exported);
                if let ArenaBindingTargetKind::Name(name) = self.arena.binding_target(target).kind
                    && let Some(binding) = self
                        .scopes
                        .last_mut()
                        .and_then(|scope| scope.get_mut(name.as_str().as_str()))
                {
                    binding.mutable = true;
                }
            }
            ArenaStmtKind::Assign { target, op, value } => {
                if op == AssignOp::Set {
                    rules::statement(self, stmt_id, rules::StatementSite::ListCompoundAssignment, exported);
                }
                rules::statement(self, stmt_id, rules::StatementSite::ListAnyBindings, exported);
                self.lint_assign_target(target);
                self.lint_expr_or_run(&value);
            }
            ArenaStmtKind::ProcDef(def) | ArenaStmtKind::CliMain(def) => {
                let function = self.arena.function_def(def);
                if let Some(edges) = &mut self.callable_edges {
                    let cli_main = matches!(stmt.kind, ArenaStmtKind::CliMain(_));
                    let owner = if cli_main {
                        CallableEdgeOwner::CliMain
                    } else {
                        CallableEdgeOwner::Callable(def)
                    };
                    edges.enter_owner(owner, cli_main);
                }
                let entrypoint = matches!(stmt.kind, ArenaStmtKind::CliMain(_))
                    || function.test_declaration
                    || (!exported && self.scopes.len() == 1 && function.name == "main");
                rules::statement(self, stmt_id, rules::StatementSite::TypedCallableParameters, exported);
                if matches!(stmt.kind, ArenaStmtKind::ProcDef(_)) {
                    self.lint_propagating_function(def, false, |linter| {
                        linter.lint_proc_function(def, exported, entrypoint, stmt.span);
                    });
                } else {
                    self.lint_proc_function(def, exported, entrypoint, stmt.span);
                }
                rules::statement(self, stmt_id, rules::StatementSite::MissingEffects, exported);
                if let Some(edges) = &mut self.callable_edges {
                    edges.leave_owner();
                }
            }
            ArenaStmtKind::PureDef(def) => {
                if let Some(edges) = &mut self.callable_edges {
                    edges.enter_owner(CallableEdgeOwner::Callable(def), false);
                }
                rules::statement(self, stmt_id, rules::StatementSite::TypedCallableParameters, exported);
                rules::statement(self, stmt_id, rules::StatementSite::InferredPureReturn, exported);
                let visibility = CallableVisibility::from_exported(exported);
                self.lint_propagating_function(def, true, |linter| {
                    linter.lint_function(def, visibility)
                });
                if let Some(edges) = &mut self.callable_edges {
                    edges.leave_owner();
                }
            }
            ArenaStmtKind::StreamDef(def) => {
                if let Some(edges) = &mut self.callable_edges {
                    edges.enter_owner(CallableEdgeOwner::Callable(def), false);
                }
                self.lint_proc_function(def, exported, false, stmt.span);
                rules::statement(self, stmt_id, rules::StatementSite::MissingEffects, exported);
                if let Some(edges) = &mut self.callable_edges {
                    edges.leave_owner();
                }
            }
            ArenaStmtKind::SignalHook(hook_id) => {
                let body = self.arena.signal_hook(hook_id).body;
                if let Some(edges) = &mut self.callable_edges {
                    edges.enter_owner(CallableEdgeOwner::RootSignal, false);
                }
                let active_candidate = self.stream_producers.suspend_candidate_analysis();
                self.lint_block(body);
                self.stream_producers
                    .restore_candidate_analysis(active_candidate);
                if let Some(edges) = &mut self.callable_edges {
                    edges.leave_owner();
                }
            }
            ArenaStmtKind::Return(value) => {
                if let Some(value) = value {
                    rules::statement(self, stmt_id, rules::StatementSite::RedundantOkReturn, exported);
                    rules::statement(self, stmt_id, rules::StatementSite::ReturnPathParse, exported);
                    rules::statement(self, stmt_id, rules::StatementSite::ReturnRequire, exported);
                    self.lint_expr_or_run(&value);
                }
            }
            ArenaStmtKind::Defer(value, _) => self.lint_expr_or_run(&value),
            ArenaStmtKind::YieldDelegate(value) | ArenaStmtKind::Exit(value) => {
                self.lint_expr(value)
            }
            ArenaStmtKind::Yield(value) => self.lint_expr_or_run(&value),
            ArenaStmtKind::If {
                branches,
                else_block,
            } => {
                rules::statement(self, stmt_id, rules::StatementSite::ListPattern, exported);
                rules::statement(self, stmt_id, rules::StatementSite::IfAsGuard, exported);
                rules::statement(self, stmt_id, rules::StatementSite::LexicalBlock, exported);
                for branch in self.arena.if_branches(branches).to_vec() {
                    self.lint_pattern_condition_block(branch.condition, branch.block);
                }
                if let Some(block) = else_block {
                    self.lint_block(block);
                }
            }
            ArenaStmtKind::While { condition, block } => {
                self.lint_pattern_condition_block(condition, block);
            }
            ArenaStmtKind::For {
                target,
                iter,
                block,
            } => {
                rules::for_loop(self, stmt_id, target, iter, block, stmt.span);
                self.lint_expr(iter);
                self.push_scope();
                self.define_binding_target(target, stmt.span, true);
                self.lint_block_statements(block);
                self.pop_scope();
            }
            ArenaStmtKind::Break { value } => {
                if let Some(expr) = value {
                    let observing = self.stream_producers.suspend_legacy_projection();
                    self.lint_expr(expr);
                    self.stream_producers.restore_projection(observing);
                }
            }
            ArenaStmtKind::Continue => {}
            ArenaStmtKind::Loop { block } => self.lint_block(block),
            // The name `atomically replace` binds is in scope for its body
            // only, and the form itself renames it, so it is never an unused
            // binding.
            ArenaStmtKind::Sugar {
                form: SugarForm::Atomically,
                operands,
                ..
            } => {
                if let ArenaSugar::Atomically { dest, name, body } =
                    self.arena.sugar(SugarForm::Atomically, operands)
                {
                    self.lint_expr(dest);
                    self.push_scope();
                    self.define_binding_target(name, stmt.span, false);
                    self.lint_block(body);
                    self.pop_scope();
                    rules::statement(
                        self,
                        stmt_id,
                        rules::StatementSite::AtomicallyNeverReplaces,
                        exported,
                    );
                }
            }
            // Both names an indexed `for` binds are loop bindings, in scope
            // for its body only.
            ArenaStmtKind::Sugar {
                form: SugarForm::ForIndex,
                operands,
                ..
            } => {
                if let ArenaSugar::ForIndex {
                    index,
                    item,
                    source,
                    body,
                } = self.arena.sugar(SugarForm::ForIndex, operands)
                {
                    self.lint_expr(source);
                    self.push_scope();
                    self.define_binding_target(index, stmt.span, true);
                    self.define_binding_target(item, stmt.span, true);
                    self.lint_block_statements(body);
                    self.pop_scope();
                }
            }
            ArenaStmtKind::Sugar { form, operands, .. } => {
                let guarded = matches!(form, SugarForm::When | SugarForm::Unless);
                for operand in self.arena.sugar_operands(operands).to_vec() {
                    match operand {
                        ArenaSugarOperand::Expr(expr) => self.lint_expr(expr),
                        ArenaSugarOperand::Block(block) => self.lint_block(block),
                        ArenaSugarOperand::Stmt(stmt) => {
                            self.guarded_statement_depth += usize::from(guarded);
                            self.lint_stmt(stmt, false);
                            self.guarded_statement_depth -= usize::from(guarded);
                        }
                        _ => {}
                    }
                }
            }
            ArenaStmtKind::Guard {
                target,
                initializer,
                else_block,
                ..
            } => {
                self.lint_expr_or_run(&initializer);
                if let Some(edges) = &mut self.callable_edges {
                    edges.suppress_definitions(true);
                }
                self.define_binding_target(target, stmt.span, true);
                if let Some(edges) = &mut self.callable_edges {
                    edges.suppress_definitions(false);
                }
                self.lint_block(else_block);
                self.collect_callable_edge_binding_target(target);
            }
            ArenaStmtKind::Assert { condition, message } => {
                self.lint_expr(condition);
                if let Some(message) = message {
                    self.lint_expr(message);
                }
            }
            ArenaStmtKind::Match { value, arms } => {
                rules::statement(self, stmt_id, rules::StatementSite::AdjacentPatternArms, exported);
                rules::statement(self, stmt_id, rules::StatementSite::CatchAllMatch, exported);
                let old_regex_context = self.regex_recovery_context;
                self.regex_recovery_context = true;
                rules::statement(self, stmt_id, rules::StatementSite::PatternConditional, exported);
                self.lint_expr(value);
                for arm in self.arena.match_arms(arms).to_vec() {
                    self.push_scope();
                    self.note_match_arm_head(arm.pattern);
                    self.lint_pattern(arm.pattern);
                    if let Some(guard) = arm.guard {
                        self.lint_expr(guard);
                    }
                    self.lint_block_statements(arm.block);
                    self.widen_guard_fix_to_arm_block(arm.block);
                    self.pop_scope();
                }
                self.regex_recovery_context = old_regex_context;
                rules::statement(self, stmt_id, rules::StatementSite::StringlyMatch, exported);
            }
            ArenaStmtKind::Command(command) => {
                rules::statement(self, stmt_id, rules::StatementSite::InteractiveCommand, exported);
                let active_candidate = self.stream_producers.suspend_candidate_analysis();
                self.lint_command_stmt(command);
                self.stream_producers
                    .restore_candidate_analysis(active_candidate);
            }
            ArenaStmtKind::TailBareIdent(name) => {
                if let Some(edges) = &mut self.callable_edges {
                    edges.direct_name(name, None);
                }
                self.mark_used(name.as_str().as_str());
                let hidden = self.local_hides(name);
                self.callable_parameters.tail_value(name, hidden);
            }
            ArenaStmtKind::Expr(expr) => {
                let span = self.arena.expr(expr).span;
                if self.statement_expression_spans.contains(&span) {
                    let inner = match self.arena.expr(expr).kind {
                        ArenaExprKind::Try(inner)
                        | ArenaExprKind::Unary {
                            op: UnaryOp::Not,
                            expr: inner,
                        } => inner,
                        _ => expr,
                    };
                    if matches!(self.arena.expr(inner).kind, ArenaExprKind::Call { .. }) {
                        self.statement_call_spans
                            .insert(self.arena.expr(inner).span, span);
                        if self.guarded_statement_depth == 0 {
                            self.whole_statement_call_spans
                                .insert(self.arena.expr(inner).span, span);
                        }
                    }
                }
                rules::statement(self, stmt_id, rules::StatementSite::CoreAssert, exported);
                self.lint_expr(expr);
            }
            ArenaStmtKind::With {
                bindings,
                body,
                else_block,
                ..
            } => {
                let old_regex_context = self.regex_recovery_context;
                self.regex_recovery_context = true;
                if let Some(edges) = &mut self.callable_edges {
                    edges.push_scope();
                }
                for binding in self.arena.with_bindings(bindings).to_vec() {
                    self.lint_expr_or_run(&ArenaExprOrRun::Expr(binding.initializer));
                    if binding.name.as_str() != "_"
                        && let Some(edges) = &mut self.callable_edges
                    {
                        edges.define(binding.name);
                    }
                }
                self.propagation_boundary_depth += 1;
                self.lint_block(body);
                if let Some(edges) = &mut self.callable_edges {
                    edges.pop_scope();
                    edges.push_scope();
                }
                self.lint_block(else_block);
                if let Some(edges) = &mut self.callable_edges {
                    edges.pop_scope();
                }
                self.propagation_boundary_depth -= 1;
                self.regex_recovery_context = old_regex_context;
            }
        }
    }

    pub(super) fn lint_function(
        &mut self,
        def_id: FunctionDefId,
        visibility: CallableVisibility,
    ) {
        let previous_candidate = self.stream_producers.enter_function(def_id);
        let def = self.arena.function_def(def_id).clone();
        self.push_scope();
        let result_unit = result_unit_type_expr(self.arena, def.return_ty);
        let result_path = result_path_type_expr(self.arena, def.return_ty);
        let result_ok = result_ok_type_expr(self.arena, def.return_ty);
        let body_span = self.arena.span(self.arena.block(def.body).span);
        let return_ty = self
            .checked_function_returns
            .get(&body_span)
            .cloned()
            .unwrap_or_else(|| Type::from_arena(self.arena, def.return_ty));

        self.result_unit_functions.push(result_unit);
        self.result_path_functions.push(result_path);
        self.result_return_ok_types.push(result_ok.clone());
        self.function_return_types.push(return_ty.clone());
        self.function_bodies.push(def.body);
        rules::function(self, def.body, result_unit, result_path, result_ok.as_ref(), &return_ty);
        if !def.return_ty_defaulted {
            self.collect_type_expr_refs(def.return_ty);
        }
        for param in self.arena.params(def.params).to_vec() {
            rules::parameter(self, &param);
            if !param.ty_defaulted {
                self.collect_type_expr_refs(param.ty);
            }
            if let Some(default) = param.default {
                let observing = self.stream_producers.suspend_legacy_projection();
                self.lint_expr(default);
                self.stream_producers.restore_projection(observing);
            }
            self.define_function_parameter(
                param.name.as_str().as_str(),
                self.arena.span(param.span),
                true,
                visibility,
            );
        }
        self.lint_block_statements(def.body);
        self.result_unit_functions.pop();
        self.result_path_functions.pop();
        self.result_return_ok_types.pop();
        self.function_return_types.pop();
        self.function_bodies.pop();
        self.pop_scope();
        self.stream_producers.leave_function(previous_candidate);
    }

    pub(super) fn note_match_arm_head(&mut self, pattern: PatternId) {
        let span = self.arena.span(self.arena.pattern(pattern).span);
        self.match_arm_head_starts.insert(span.start());
    }

    /// `lint.prefer-inferred-variant` for a pattern: the checker found its
    /// qualifier redundant against the matched value's type.
    pub(super) fn lint_inferred_variant_pattern(&mut self, pattern: Span) {
        let Some(qualifier) = self.redundant_variant_qualifiers.get(&pattern).copied() else {
            return;
        };
        if self.match_arm_head_starts.contains(&qualifier.start())
            || !self.inferred_variant_patterns_reported.insert(pattern)
        {
            return;
        }
        self.diagnostics
            .push(lint_inferred_variant_pattern::redundant_pattern_qualifier(
                self.source,
                qualifier,
            ));
    }

    pub(super) fn lint_pattern(&mut self, pattern: PatternId) {
        let arena_pattern = self.arena.pattern(pattern).clone();
        let span = self.arena.span(arena_pattern.span);
        rules::pattern(self, pattern);
        match arena_pattern.kind {
            ArenaPatternKind::Group(child) => self.lint_pattern(child),
            ArenaPatternKind::Alias {
                pattern,
                name,
                name_span,
            } => {
                self.lint_pattern(pattern);
                self.define(&name.as_str(), self.arena.span(name_span), true);
            }
            ArenaPatternKind::Binding(name) => {
                if !self.tag_variants.contains(name.as_str().as_str()) {
                    self.define(name.as_str().as_str(), span, true);
                }
            }
            ArenaPatternKind::Type { binding, ty } => {
                self.collect_type_expr_refs(ty);
                if let Some(name) = binding {
                    self.define(name.as_str().as_str(), span, true);
                }
            }
            ArenaPatternKind::Constructor { arg, .. } => {
                if let Some(arg) = arg {
                    self.lint_pattern(arg);
                }
            }
            ArenaPatternKind::Record { fields, .. }
            | ArenaPatternKind::ErrorVariant { fields, .. } => {
                for field in self.arena.pattern_fields(fields).to_vec() {
                    self.lint_pattern(field.pattern);
                }
            }
            ArenaPatternKind::List { elements, rest } => {
                for child in self
                    .arena
                    .pattern_ids(elements)
                    .chain(rest)
                    .collect::<Vec<_>>()
                {
                    self.lint_pattern(child);
                }
            }
            ArenaPatternKind::Alternation(patterns) => {
                let children: Vec<_> = self.arena.pattern_ids(patterns).collect();
                let saved = self.scopes.last().cloned().unwrap_or_default();
                for &child in children.iter().skip(1) {
                    if let Some(scope) = self.scopes.last_mut() {
                        *scope = saved.clone();
                    }
                    self.lint_pattern(child);
                }
                if let Some(scope) = self.scopes.last_mut() {
                    *scope = saved;
                }
                if let Some(&child) = children.first() {
                    self.lint_pattern(child);
                }
            }
            ArenaPatternKind::Tuple(patterns) | ArenaPatternKind::Text(patterns) => {
                for pat in self.arena.pattern_ids(patterns).collect::<Vec<_>>() {
                    self.lint_pattern(pat);
                }
            }
            ArenaPatternKind::TextHole { binding, .. } => {
                if let Some(name) = binding {
                    self.define(name.as_str().as_str(), span, true);
                }
            }
            ArenaPatternKind::TestName { ty, .. } => self.collect_type_expr_refs(ty),
            ArenaPatternKind::Wildcard
            | ArenaPatternKind::Literal(_)
            | ArenaPatternKind::Facet(_) => {}
        }
    }

    /// The span the innermost scope records for the binding `name` names.
    pub(super) fn binding_definition(&self, name: Name) -> Option<Span> {
        let name = name.as_str();
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name.as_str()))
            .map(|binding| binding.span)
    }

    /// Whether a binding inside a function or block hides the top-level
    /// meaning of `name` where the traversal is.
    pub(super) fn local_hides(&self, name: Name) -> bool {
        self.scopes
            .iter()
            .skip(1)
            .any(|scope| scope.contains_key(name.as_str().as_str()))
    }

    /// Tells `lint.list-any-union` about a name the traversal just defined.
    pub(super) fn declare_list_any_binding(
        &mut self,
        target: BindingTargetId,
        ty: Option<TypeExprId>,
        initializer: &ArenaExprOrRun,
    ) {
        let ArenaBindingTargetKind::Name(name) = self.arena.binding_target(target).kind else {
            return;
        };
        if let Some(definition) = self.binding_definition(name) {
            self.list_any_bindings.declare(
                self.arena,
                definition,
                ty,
                initializer,
                &self.expr_types,
            );
        }
    }

    pub(super) fn define_binding_target(&mut self, target: BindingTargetId, span: Span, report_unused: bool) {
        match self.arena.binding_target(target).kind.clone() {
            ArenaBindingTargetKind::Name(name) => {
                self.define(name.as_str().as_str(), span, report_unused)
            }
            ArenaBindingTargetKind::Record { fields, .. } => {
                for field in self.arena.destructure_fields(fields).to_vec() {
                    self.define_binding_target(
                        field.target,
                        self.arena.span(field.span),
                        report_unused,
                    );
                }
            }
        }
    }

    fn collect_callable_edge_binding_target(&mut self, target: BindingTargetId) {
        match self.arena.binding_target(target).kind.clone() {
            ArenaBindingTargetKind::Name(name) => {
                if let Some(edges) = &mut self.callable_edges {
                    edges.define(name);
                }
            }
            ArenaBindingTargetKind::Record { fields, .. } => {
                for field in self.arena.destructure_fields(fields).to_vec() {
                    self.collect_callable_edge_binding_target(field.target);
                }
            }
        }
    }

    pub(super) fn lint_pattern_condition_block(&mut self, condition: ExprId, block: BlockId) {
        if let ArenaExprKind::PatternCondition { value, arms } = self.arena.expr(condition).kind {
            self.lint_expr(value);
            self.push_scope();
            self.lint_pattern(self.arena.match_expr_arms(arms)[0].pattern);
            self.lint_block_statements(block);
            self.pop_scope();
        } else {
            self.lint_expr(condition);
            self.lint_block(block);
        }
    }

    pub(super) fn lint_block(&mut self, block: BlockId) {
        self.push_scope();
        self.lint_block_statements(block);
        self.pop_scope();
    }

    pub(super) fn lint_block_statements(&mut self, block: BlockId) {
        let stmts: Vec<StmtId> = self
            .arena
            .stmt_ids(self.arena.block(block).statements)
            .collect();
        rules::block_sequence(self, &stmts, block);
        let close_diagnostic_index = self.diagnostics.len();
        self.lint_statement_sequence(&stmts);
        rules::sequence_exit(
            self,
            rules::SequenceExitSite::Block,
            &stmts,
            close_diagnostic_index,
        );
    }

    pub(super) fn lint_command_stmt(&mut self, stmt_id: CommandStmtId) {
        let stmt = self.arena.command_stmt(stmt_id).clone();
        match stmt.command {
            ArenaCommand::Proc { name, args } => {
                if let Some(edges) = &mut self.callable_edges {
                    edges.direct_name(name, None);
                }
                self.lint_proc_command_args(args);
            }
            ArenaCommand::Core {
                name: _,
                args,
                env,
                block,
            } => {
                self.lint_command_args(args);
                for assignment in self.arena.env_assignments(env).to_vec() {
                    self.lint_env_assignment_value(&assignment.value);
                }
                if let Some(block) = block {
                    self.lint_block(block);
                }
            }
            ArenaCommand::Run(run) => self.lint_run(run),
        }
    }

    pub(super) fn lint_run(&mut self, run_id: RunFormId) {
        rules::run(self, run_id);
        LintExprVisitor {
            linter: self,
            suppress_expr_autofixes: false,
        }
        .visit_run_form(run_id);
    }

    pub(super) fn lint_command_args(&mut self, args: ArenaRange) {
        for arg in self.arena.command_args(args).to_vec() {
            self.lint_command_arg(&arg);
        }
    }

    pub(super) fn lint_proc_command_args(&mut self, args: ArenaRange) {
        for arg in self.arena.command_args(args).to_vec() {
            self.lint_proc_command_arg(&arg);
        }
    }

    pub(super) fn lint_env_assignment_value(&mut self, value: &ArenaEnvAssignmentValue) {
        match value {
            ArenaEnvAssignmentValue::CommandArg(arg) => self.lint_proc_command_arg(arg),
            ArenaEnvAssignmentValue::Expr(expr) => self.lint_expr(*expr),
        }
    }

    pub(super) fn lint_assign_target(&mut self, target: AssignTargetId) {
        match self.arena.assign_target(target).kind.clone() {
            ArenaAssignTargetKind::Name(name) => self.mark_used(name.as_str().as_str()),
            ArenaAssignTargetKind::Env(_) => {}
            ArenaAssignTargetKind::Field { base, .. } => self.lint_assign_target(base),
            ArenaAssignTargetKind::Index { base, index } => {
                self.lint_assign_target(base);
                self.lint_expr(index);
            }
        }
    }

    pub(super) fn lint_command_arg(&mut self, arg: &ArenaCommandArg) {
        LintExprVisitor {
            linter: self,
            suppress_expr_autofixes: true,
        }
        .visit_command_arg(arg, true);
    }

    pub(super) fn lint_proc_command_arg(&mut self, arg: &ArenaCommandArg) {
        LintExprVisitor {
            linter: self,
            suppress_expr_autofixes: true,
        }
        .visit_command_arg(arg, true);
    }

    pub(super) fn lint_expr_or_run(&mut self, value: &ArenaExprOrRun) {
        let previous_collection =
            std::mem::replace(&mut self.assignment_collection_enabled, false);
        match value {
            ArenaExprOrRun::Expr(expr) => self.lint_expr(*expr),
            ArenaExprOrRun::Run(run) => {
                let observing = self.stream_producers.suspend_legacy_projection();
                self.lint_run(*run);
                self.stream_producers.restore_projection(observing);
            }
        }
        self.assignment_collection_enabled = previous_collection;
    }

    pub(super) fn lint_expr(&mut self, expr: ExprId) {
        let previous_collection =
            std::mem::replace(&mut self.assignment_collection_enabled, false);
        let active_candidate = self.stream_producers.suspend_candidate_analysis();
        LintExprVisitor {
            linter: self,
            suppress_expr_autofixes: false,
        }
        .visit_expr(expr);
        self.stream_producers
            .restore_candidate_analysis(active_candidate);
        self.assignment_collection_enabled = previous_collection;
    }

    pub(super) fn observe_expression_positions(&mut self, expression: ExprId) {
        let arena = self.arena;
        let paren_groups = self.paren_groups;
        self.expr_positions
            .get_or_insert_with(|| {
                super::lint_redundant_propagation::ExprPositions::new(arena, paren_groups)
            })
            .observe_expression(arena, expression);
    }

    pub(super) fn observe_statement_positions(&mut self, statement: StmtId) {
        let arena = self.arena;
        let paren_groups = self.paren_groups;
        self.expr_positions
            .get_or_insert_with(|| {
                super::lint_redundant_propagation::ExprPositions::new(arena, paren_groups)
            })
            .observe_statement(arena, statement);
    }

    pub(super) fn collect_tempdir_scope_candidates(&mut self, statement: StmtId) {
        if !self.source.contains("fs.tempdir()") {
            return;
        }
        match self.arena.stmt(statement).kind {
            ArenaStmtKind::ProcDef(definition)
            | ArenaStmtKind::CliMain(definition)
            | ArenaStmtKind::PureDef(definition)
            | ArenaStmtKind::StreamDef(definition) => {
                self.tempdir_scope_functions.push(definition);
            }
            ArenaStmtKind::For { .. }
            | ArenaStmtKind::While { .. }
            | ArenaStmtKind::Loop { .. } => {
                self.tempdir_scope_loops.push(statement);
            }
            _ => {}
        }
    }

    pub(super) fn observe_name_reference(&mut self, expression: ExprId) {
        let ArenaExprKind::Ident(name) = self.arena.expr(expression).kind else {
            return;
        };
        if !self.name_reference_exprs.insert(expression) {
            return;
        }
        if rules::enabled(self, DiagnosticCode::LintPreferDeferBlock) {
            let count = self.defer_block_references.entry(name).or_default();
            *count += 1;
        }
        if rules::enabled(self, DiagnosticCode::LintPreferValuePipeline) {
            self.linear_pipeline_identifier_uses
                .entry(name)
                .or_default()
                .push(self.arena.expr(expression).span);
        }
    }

    pub(super) fn observe_pipeline_shorthand_field(&mut self, name: Name, span: Span) {
        self.linear_pipeline_shorthand_fields
            .entry(name)
            .or_default()
            .push(span);
    }

    pub(super) fn observe_binding_initializer_type(&mut self, statement: StmtId) {
        let initializer = match self.arena.stmt(statement).kind {
            ArenaStmtKind::Let {
                initializer: ArenaExprOrRun::Expr(value),
                ..
            }
            | ArenaStmtKind::Const {
                initializer: ArenaExprOrRun::Expr(value),
                ..
            } => value,
            _ => return,
        };
        let span = self.arena.expr(initializer).span;
        if let Some(ty) = self.expr_types.get(&span) {
            self.binding_initializer_types
                .insert(self.arena.stmt(statement).span, ty.clone());
        }
    }

    pub(super) fn lint_builder_block(&mut self, block: BuilderBlockId) {
        self.push_scope();
        let entries: Vec<_> = self
            .arena
            .builder_entries(self.arena.builder_block(block).entries)
            .to_vec();
        for entry in entries {
            match entry.kind {
                ArenaBuilderEntryKind::Field { value, .. } => self.lint_expr(value),
                ArenaBuilderEntryKind::Entry { args, block, .. } => {
                    for arg in self.arena.command_args(args).to_vec() {
                        self.lint_command_arg(&arg);
                    }
                    if let Some(block) = block {
                        self.lint_builder_block(block);
                    }
                }
                ArenaBuilderEntryKind::Task { block, .. } => self.lint_block_statements(block),
                ArenaBuilderEntryKind::Stmt(stmt) => self.lint_stmt(stmt, false),
            }
        }
        self.pop_scope();
    }

    pub(super) fn lint_stream_block(&mut self, block: BlockId) {
        self.push_scope();
        for param in self
            .arena
            .block_params(self.arena.block(block).params)
            .to_vec()
        {
            self.define(
                param.name.as_str().as_str(),
                self.arena.span(param.span),
                true,
            );
        }
        self.lint_block_statements(block);
        self.pop_scope();
    }

    pub(super) fn lint_stream_stage(&mut self, stage: &ArenaStreamStage) {
        rules::stream_stage(self, stage);
        for arg in self.arena.call_args(stage.args).to_vec() {
            self.lint_call_arg(&arg);
        }
        if let Some(block) = stage.block {
            let first_diagnostic = self.diagnostics.len();
            self.lint_stream_block(block);
            let span = self.arena.span(self.arena.block(block).span);
            if let Some(text) = self.source.get(span.range())
                && !text.trim_start().starts_with('{')
            {
                // An inline predicate beginning with parentheses is parsed as
                // stage arguments. Keep rewritten expressions inside the same
                // callback by giving its synthetic block explicit delimiters.
                let mut edits: Vec<_> = self.diagnostics[first_diagnostic..]
                    .iter()
                    .filter(|diagnostic| {
                        matches!(
                            diagnostic.code,
                            Some(DiagnosticCode::LintPreferIn | DiagnosticCode::LintCoreAssert)
                        )
                    })
                    .flat_map(|diagnostic| &diagnostic.fix_hints)
                    .filter_map(|hint| Some((hint.span?, hint.replacement.as_ref()?)))
                    .filter(|(edit, _)| edit.start() >= span.start() && edit.end() <= span.end())
                    .collect();
                edits.sort_by_key(|(edit, _)| (edit.start(), std::cmp::Reverse(edit.end())));
                let mut end = span.start();
                edits.retain(|(edit, _)| {
                    if edit.start() < end {
                        return false;
                    }
                    end = edit.end();
                    true
                });
                if !edits.is_empty() {
                    let mut replacement = text.to_string();
                    for (edit, value) in edits.iter().rev() {
                        replacement.replace_range(
                            edit.start() - span.start()..edit.end() - span.start(),
                            value,
                        );
                    }
                    let replacement = format!("{{ {replacement} }}");
                    for diagnostic in &mut self.diagnostics[first_diagnostic..] {
                        if matches!(
                            diagnostic.code,
                            Some(DiagnosticCode::LintPreferIn | DiagnosticCode::LintCoreAssert)
                        ) {
                            for hint in &mut diagnostic.fix_hints {
                                if hint.span.is_some_and(|edit| {
                                    edit.start() >= span.start() && edit.end() <= span.end()
                                }) {
                                    *hint = FixHint::replacement(
                                        span,
                                        "preserve the stream predicate callback",
                                        replacement.clone(),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub(super) fn lint_call_arg(&mut self, arg: &ArenaCallArg) {
        LintExprVisitor {
            linter: self,
            suppress_expr_autofixes: false,
        }
        .visit_call_arg(arg);
    }

    pub(super) fn define(&mut self, name: &str, span: Span, report_unused: bool) {
        self.define_with_origin(name, span, report_unused, BindingOrigin::Local);
    }

    fn define_function_parameter(
        &mut self,
        name: &str,
        span: Span,
        report_unused: bool,
        visibility: CallableVisibility,
    ) {
        self.define_with_origin(
            name,
            span,
            report_unused,
            BindingOrigin::FunctionParameter { visibility },
        );
    }

    fn define_with_origin(
        &mut self,
        name: &str,
        span: Span,
        report_unused: bool,
        origin: BindingOrigin,
    ) {
        rules::binding(self, name, span, origin);
        if let Some(edges) = &mut self.callable_edges {
            edges.define(Name::intern(name));
        }
        // Removing an unread owner would shorten its lexical lifetime.
        let resource_owned = self.checked_output.as_deref().is_some_and(|checked| {
            checked
                .local_binding_types
                .get(&span)
                .is_some_and(Type::may_hold_resource)
        });
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(
                name.to_string(),
                Binding {
                    mutable: false,
                    span,
                    used: false,
                    comparison_stable: false,
                    absence_lookup: false,
                    materialized_record: false,
                    immutable_byte_length: None,
                    report_unused: report_unused && name != "_" && !resource_owned,
                },
            );
        }
    }

    pub(super) fn mark_used(&mut self, name: &str) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(binding) = scope.get_mut(name) {
                binding.used = true;
                break;
            }
        }
    }

    pub(super) fn is_defined_in_outer_scope(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .rev()
            .skip(1)
            .any(|scope| scope.contains_key(name))
    }

    pub(super) fn push_scope(&mut self) {
        if let Some(edges) = &mut self.callable_edges {
            edges.push_scope();
        }
        self.scopes.push(FxHashMap::default());
    }

    pub(super) fn pop_scope(&mut self) {
        if let Some(edges) = &mut self.callable_edges {
            edges.pop_scope();
        }
        let Some(scope) = self.scopes.pop() else {
            return;
        };
        let mut unused: Vec<_> = scope
            .into_iter()
            .filter(|(_, binding)| binding.report_unused && !binding.used)
            .collect();
        insertion_sort_by(&mut unused, |(_, left), (_, right)| {
            left.span.start().cmp(&right.span.start())
        });
        rules::scope_exit(self, &unused);
    }

    pub(super) fn expr_or_run_span(&self, value: &ArenaExprOrRun) -> Span {
        match value {
            ArenaExprOrRun::Expr(expr) => self.arena.expr(*expr).span,
            ArenaExprOrRun::Run(run) => self.arena.span(self.arena.run_form(*run).span),
        }
    }
}

#[cfg(test)]
mod unused_local_resource_tests {
    use super::super::{LintOptions, Linter, lint_requires_check_output};
    use xsh::diagnostic::DiagnosticCode;
    use xsh::frontend::check::{Checker, Type};
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    #[test]
    fn cd_path_expression_marks_local_as_used() {
        let source = "test cd_path_expression_marks_local_as_used { |ctx|\n  let root = test.temp_dir(ctx)?\n  cd root {\n    assert true\n  }\n  let unused = 1\n}\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(1), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let mut program = parsed.arena;
        let checked = Checker::check_arena(&program, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

        // Workspace modules share one arena, whose primary source ID differs
        // from the source ID attached to a later module's command words.
        let test_statement = program.arena.stmt_ids(program.statements).next().unwrap();
        let xsh::frontend::syntax::arena::ArenaStmtKind::ProcDef(test_function) =
            program.arena.stmt(test_statement).kind
        else {
            panic!("test declaration should lower to a procedure");
        };
        let body = program.arena.function_def(test_function).body;
        let cd_argument_span = program
            .arena
            .stmt_ids(program.arena.block(body).statements)
            .find_map(|statement| {
                let xsh::frontend::syntax::arena::ArenaStmtKind::Command(command) =
                    program.arena.stmt(statement).kind
                else {
                    return None;
                };
                let xsh::frontend::syntax::arena::ArenaCommand::Core {
                    name: xsh::frontend::syntax::node::CoreCommand::Cd,
                    args,
                    ..
                } = program.arena.command_stmt(command).command.clone()
                else {
                    return None;
                };
                program.arena.command_args(args).first().map(|arg| arg.span)
            })
            .expect("the test body should have a cd path argument");
        program.arena.span_source_id = Some(SourceId::new(0));
        program
            .arena
            .stmt_span_source_overrides
            .push(xsh::frontend::syntax::arena::ArenaSpanSource {
                span: test_statement.index() as u32,
                source_id: SourceId::new(1),
            });
        program
            .arena
            .span_source_overrides
            .push(xsh::frontend::syntax::arena::ArenaSpanSource {
                span: cd_argument_span.index() as u32,
                source_id: SourceId::new(1),
            });

        let mut options = LintOptions {
            only: Some(vec![DiagnosticCode::LintUnusedLocal]),
            ..LintOptions::default()
        };
        options.checked_output = lint_requires_check_output(&options)
            .then(|| std::sync::Arc::new(checked));
        let diagnostics = Linter::lint(&program, source, options).diagnostics;
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].message, "unused local variable `unused`");
    }

    #[test]
    fn literal_cd_path_does_not_mark_an_unrelated_local_as_used() {
        let source = "test literal_cd_path_does_not_mark_an_unrelated_local_as_used {\n  let root = 1\n  cd p\".\" {\n    assert true\n  }\n}\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

        let mut options = LintOptions {
            only: Some(vec![DiagnosticCode::LintUnusedLocal]),
            ..LintOptions::default()
        };
        options.checked_output = lint_requires_check_output(&options)
            .then(|| std::sync::Arc::new(checked));
        let diagnostics = Linter::lint(&parsed.arena, source, options).diagnostics;
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].message, "unused local variable `root`");
    }

    #[test]
    fn unused_local_requests_checked_types_for_deferred_resource_owners() {
        let source = "proc hold_locks(first_path: Path, second_path: Path) [fs, error] -> Result[Unit] {\n  let lock = fs.lock(first_path)?\n  let second_lock = fs.lock(second_path)?\n  defer {\n    assert true\n  }\n  Ok()\n}\nproc hold_stream() [process, error] {\n  let owned = run.stream --text --accept=[0] printf value\n  defer {\n    let kept = owned\n    assert true\n  }\n}\npure ignore_scalar() -> Int {\n  let unused = 1\n  0\n}\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert!(checked.local_binding_types.values().any(|ty| *ty == Type::FsLock));
        assert!(checked.local_binding_types.values().any(|ty| matches!(ty, Type::Stream(item) if item.as_ref() == &Type::Str)));

        let mut options = LintOptions {
            only: Some(vec![DiagnosticCode::LintUnusedLocal]),
            ..LintOptions::default()
        };
        options.checked_output = lint_requires_check_output(&options)
            .then(|| std::sync::Arc::new(checked));
        let diagnostics = Linter::lint(&parsed.arena, source, options).diagnostics;
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(
            diagnostics[0].message,
            "unused local variable `unused`"
        );
    }
}
