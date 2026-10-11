use super::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaBuilderEntryKind, ArenaCallArg,
    ArenaCallArgKind, ArenaCommand, ArenaCommandArg, ArenaCommandArgKind, ArenaCompQualifier,
    ArenaEnvAssignment, ArenaEnvAssignmentValue, ArenaExprKind, ArenaExprOrRun, ArenaFmtPart,
    ArenaMatchExprArm, ArenaModuleContractEntryKind, ArenaPatternKind, ArenaPipeStage,
    ArenaPipeStageKind, ArenaRange, ArenaRecordField, ArenaRecordFieldKind, ArenaRedirection,
    ArenaRedirectionTarget, ArenaSpawnTarget, ArenaStmtKind, ArenaStreamStage, ArenaSugar,
    ArenaSugarOperand, ArenaTypeDefBody, ArenaTypeExprKind, ArenaTypeExprTag, ArenaWordPart,
    AssignOp, AssignTargetId, BinaryOp, Binding, BindingTargetId, BlockId, BuilderBlockId,
    CommandStmtId, Diagnostic, DiagnosticCode, ExprId, FixHint, FunctionDefId, FxHashMap, Label,
    Linter, Name, PathDisplaySite, PatternId, RunFormId, RunKind, Severity, Span, StmtId, SugarForm,
    Type, TypeExprId, UnaryOp, bare_command_word_parts, command_name, command_value_replacement,
    expects_nonzero_status, insertion_sort_by, is_predeclared_script_args, item_shorthand,
    lint_inferred_variant_pattern, lint_list_any_union, lint_prefer_for_index,
    lint_prefer_match_else, lint_prefer_non_empty_argv, lint_prefer_propagation,
    lint_prefer_text_pattern, lint_redundant_discard, lint_redundant_propagation, lint_run_argv,
    literal_command_word, parse_command_word_reference, prefer_atomically, prefer_collect,
    prefer_repeat, prefer_tempdir, prefer_wait_until, prefer_with_scope, prefer_within,
    result_ok_type_expr, result_path_type_expr, result_unit_type_expr,
    scan_run_propagate_deletion_span, simple_command_value_expr, type_expr_kind,
};

pub(super) struct LintExprVisitor<'a, 'b> {
    linter: &'a mut Linter<'b>,
    suppress_expr_autofixes: bool,
}

impl LintExprVisitor<'_, '_> {
    fn visit_comp_qualifiers(&mut self, range: ArenaRange) -> usize {
        let mut scopes = 0;
        for qualifier in self.linter.arena.comp_qualifiers(range).to_vec() {
            if let ArenaCompQualifier::For { iter, .. } = qualifier {
                self.linter.lint_scalar_split_iteration(iter);
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
        self.linter
            .fail_candidates
            .visit_expr(self.linter.arena, expr);
        self.linter
            .set_like_bindings
            .visit_expr(self.linter.arena, self.linter.source, expr);
        let tested_fallbacks =
            self.linter
                .empty_fallbacks
                .visit_expr(self.linter.arena, self.linter.source, expr);
        self.linter.diagnostics.extend(tested_fallbacks);
        if let Some(diagnostic) = lint_redundant_propagation::redundant_condition_propagation(
            self.linter.arena,
            self.linter.source,
            &self.linter.redundant_condition_propagations,
            expr,
        ) {
            self.linter.diagnostics.push(diagnostic);
        }
        if let Some(diagnostic) = lint_redundant_propagation::redundant_capture_try(
            self.linter.arena,
            self.linter.source,
            || {
                self.linter.expr_positions.get_or_init(|| {
                    lint_redundant_propagation::ExprPositions::new(
                        self.linter.arena,
                        self.linter.paren_groups,
                    )
                })
            },
            expr,
        ) {
            self.linter.diagnostics.push(diagnostic);
        }
        if let ArenaExprKind::Unary {
            op: UnaryOp::Not,
            expr: inner,
        } = self.linter.arena.expr(expr).kind
            && matches!(
                self.linter.arena.expr(inner).kind,
                ArenaExprKind::Call { .. }
            )
        {
            self.linter.negated_call_spans.insert(
                self.linter.arena.expr(inner).span,
                self.linter.arena.expr(expr).span,
            );
        }

        if let Some(diagnostic) =
            self.linter
                .size_products
                .visit(self.linter.arena, self.linter.source, expr)
        {
            self.linter.diagnostics.push(diagnostic);
        }
        if let Some(diagnostic) = lint_run_argv::run_argv_diagnostic(
            self.linter.arena,
            self.linter.source,
            &self.linter.standard_call_spans,
            expr,
        ) {
            self.linter.diagnostics.push(diagnostic);
        }
        if let ArenaExprKind::Match { arms, .. } = self.linter.arena.expr(expr).kind
            && let Some(arm) = self.linter.arena.match_expr_arms(arms).last()
        {
            let report = lint_prefer_match_else::catch_all_arm_report(
                self.linter.arena,
                self.linter.source,
                arm.pattern,
                arm.guard,
                arm.spelling,
            );
            self.linter.catch_all_arms.extend(report);
        }
        if !self.suppress_expr_autofixes {
            self.linter.lint_proven_nonnull_fallback(expr);
        }
        if !self.suppress_expr_autofixes {
            self.linter.lint_nested_record_update(expr);
            self.linter.lint_nested_value_pipeline(expr);

            self.linter.lint_block_string_concatenation(expr);
            self.linter.lint_list_splicing(expr);
            self.linter.lint_map_literal_chain(expr);
            self.linter.lint_prepared_regex(expr);
            self.linter.lint_env_string(expr);
            self.linter.lint_comparison_chain(expr);
            self.linter.lint_lookup_sentinel(expr);

            self.linter.lint_optional_postfix(expr);
            if let ArenaExprKind::Match { arms, .. } = self.linter.arena.expr(expr).kind {
                self.linter.lint_adjacent_pattern_arms(
                    self.linter
                        .arena
                        .match_expr_arms(arms)
                        .iter()
                        .map(|arm| {
                            (
                                arm.pattern,
                                arm.guard,
                                Err(arm.value),
                                self.linter.arena.span(arm.span),
                            )
                        })
                        .collect(),
                );
            }
            self.linter.lint_boolean_match(expr);
            self.linter.lint_pattern_conditional_expr(expr);
            self.linter.lint_error_fallback_block(expr);
            self.linter.lint_path_roundtrip(expr);
            self.linter.lint_redundant_require(expr);
            self.linter.lint_known_field_access(expr);
            self.linter.lint_inferred_require_target(expr);
            self.linter.lint_redundant_single_interpolation(expr);
            self.linter.lint_scalar_display_parse_roundtrip(expr);
            self.linter.lint_json_encode_decode_roundtrip(expr);
            self.linter.lint_inferred_variant(expr);
            self.linter.lint_positional_constructor(expr);
            self.linter.lint_path_migrations(expr);
        }
        let arena_expr = self.linter.arena.expr(expr);
        match arena_expr.kind {
            ArenaExprKind::Ident(name) => {
                self.linter.mark_used(name.as_str().as_str());
                let hidden = self.linter.local_hides(name);
                self.linter.callable_parameters.value(expr, name, hidden);
            }
            ArenaExprKind::Call { callee, args } => {
                let mut parameters = std::mem::take(&mut self.linter.callable_parameters);
                parameters.call(self.linter.arena, callee, args, &|name| {
                    self.linter.local_hides(name)
                });
                self.linter.callable_parameters = parameters;
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
                self.linter.lint_call_style(callee, args, arena_expr.span);
            }
            _ => self.walk_expr(expr),
        }
    }

    fn walk_expr(&mut self, expr: ExprId) {
        let arena = self.linter.arena;
        match arena.expr(expr).kind {
            ArenaExprKind::ValuePipelineCall { input, call, .. } => {
                self.visit_expr(input);
                self.visit_expr(call);
            }

            ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
                for part in arena.fmt_parts(parts).collect::<Vec<_>>() {
                    if let ArenaFmtPart::Expr(e, spec) = part {
                        if spec.is_none() {
                            self.linter
                                .lint_redundant_path_display(e, PathDisplaySite::Interpolation);
                        }
                        self.visit_expr(e);
                    }
                }
            }
            ArenaExprKind::List(items) | ArenaExprKind::Set(items) => {
                for item in arena.list_element_exprs(items).collect::<Vec<_>>() {
                    self.visit_expr(item);
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
                    self.linter.lint_quoted_field_labels(fields);
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
            ArenaExprKind::Unary { expr, .. } | ArenaExprKind::Try(expr) => self.visit_expr(expr),
            ArenaExprKind::ComparisonChain(pairs) => {
                for operand in arena.comparison_chain_operands(pairs).collect::<Vec<_>>() {
                    self.visit_expr(operand);
                }
            }
            ArenaExprKind::Binary { op, left, right } => {
                if op == BinaryOp::ResultFallback && self.linter.prefer_item_shorthand {
                    let report = item_shorthand::handler_report(arena, self.linter.source, right);
                    self.linter.item_shorthands.extend(report);
                }
                let old = self.linter.regex_recovery_context;
                self.linter.regex_recovery_context |= op == BinaryOp::ResultFallback;
                self.visit_expr(left);
                self.visit_expr(right);
                self.linter.regex_recovery_context = old;
            }
            ArenaExprKind::Call { callee, args } => {
                self.visit_expr(callee);
                for arg in arena.call_args(args).to_vec() {
                    self.visit_call_arg(&arg);
                }
            }
            ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
                self.visit_expr(base)
            }
            ArenaExprKind::Index { base, index, .. } => {
                self.visit_expr(base);
                self.visit_expr(index);
            }
            ArenaExprKind::Slice {
                base, start, end, ..
            } => {
                self.visit_expr(base);
                if let Some(start) = start {
                    self.visit_expr(start);
                }
                if let Some(end) = end {
                    self.visit_expr(end);
                }
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
                self.linter.lint_prefer_fs_files(input, stages);
                self.linter.lint_redundant_stream_stages(stages);
            }
            ArenaExprKind::Run(run) => self.visit_run_form(run),
            ArenaExprKind::Spawn(form) => match form.target {
                ArenaSpawnTarget::Run(run) => self.visit_run_form(run),
                ArenaSpawnTarget::Command(expr) => self.visit_expr(expr),
            },
            ArenaExprKind::Wait(form) => self.visit_expr(form.target),
            ArenaExprKind::BuilderCall { call, block } => {
                self.visit_expr(call);
                self.visit_builder_block(block);
            }
            ArenaExprKind::Require { value, schema } => {
                self.visit_expr(value);
                if let Some(schema) = schema {
                    self.linter.collect_type_expr_refs(schema);
                }
            }
            ArenaExprKind::Convert { value, target } => {
                self.visit_expr(value);
                self.linter.collect_type_expr_refs(target);
            }
            ArenaExprKind::ErrorContext { message, block }
            | ArenaExprKind::ContextScope {
                input: message,
                block,
                ..
            } => {
                self.visit_expr(message);
                self.linter.lint_block(block);
            }
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
            ArenaExprKind::Loop { block } | ArenaExprKind::Collect { block } => {
                self.linter.lint_block(block)
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
            ArenaExprKind::ResourceScope {
                bindings, block, ..
            } => {
                for binding in arena.with_bindings(bindings).to_vec() {
                    self.visit_expr(binding.initializer);
                }
                self.linter.lint_block(block);
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
                self.linter.lint_dollar_in_expression_string(expr);
                self.linter.lint_missing_f_prefix(expr);
                if !self.suppress_expr_autofixes {
                    self.linter.lint_redundant_newline_triple_string(expr);
                }
            }
            ArenaExprKind::PathStr(_) => self.linter.lint_missing_f_prefix(expr),
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
        match field.kind {
            ArenaRecordFieldKind::Shorthand { name, .. } => {
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
            self.linter.lint_named_argument_pun(arg);
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
            ArenaCommandArgKind::SpliceName(name) => self.linter.mark_used(name.as_str().as_str()),
            ArenaCommandArgKind::Word(parts) => {
                self.linter.lint_redundant_command_arg_interpolation(arg);
                let part_list: Vec<ArenaWordPart> = self.linter.arena.word_parts(parts).collect();
                if allow_bare_refs
                    && let Some(text) =
                        bare_command_word_parts(self.linter.arena, self.linter.source, &part_list)
                    && let Some((root, _)) = parse_command_word_reference(&text)
                {
                    self.linter.mark_used(root);
                }
                // ${f"..."} is a Word with a single Interpolation whose expr is an FmtString.
                // Since f"..." is now accepted directly as a typed command arg, the wrapper is
                // redundant.
                if let [ArenaWordPart::Interpolation(expr)] = part_list.as_slice()
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
                        self.linter
                            .lint_redundant_path_display(expr, PathDisplaySite::Interpolation);
                        if matches!(part, ArenaWordPart::Interpolation(_)) {
                            self.visit_command_delimited_expr(expr);
                        } else {
                            self.visit_command_embedded_expr(expr);
                        }
                    }
                }
            }
            ArenaCommandArgKind::SpliceExpr(expr) => {
                self.linter
                    .lint_redundant_path_display(expr, PathDisplaySite::Splice);
                self.visit_command_delimited_expr(expr);
            }
            ArenaCommandArgKind::Typed(expr) => {
                let expr_span = self.linter.arena.expr(expr).span;
                self.linter
                    .lint_redundant_path_display(expr, PathDisplaySite::CommandWord(arg_span));
                // (f"...") → f"..." : f-strings don't need paren wrapping
                if matches!(
                    self.linter.arena.expr(expr).kind,
                    ArenaExprKind::FmtString(_) | ArenaExprKind::PathFmtString(_)
                ) && self.linter.source.as_bytes().get(arg_span.start()) == Some(&b'(')
                {
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
                } else if let Some(replacement) = self.linter.command_single_fmt_replacement(expr) {
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
                } else if simple_command_value_expr(self.linter.arena, expr) {
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
        if let Some(diagnostic) = lint_redundant_propagation::redundant_capture_propagation(
            arena,
            self.linter.source,
            run,
        ) {
            self.linter.diagnostics.push(diagnostic);
        }
        for segment in arena.run_segments(run_form.segments).to_vec() {
            if let Some(diagnostic) = lint_prefer_non_empty_argv::prefer_non_empty_argv(
                arena,
                &self.linter.unvalidated_command_vectors,
                &segment.target,
            ) {
                self.linter.diagnostics.push(diagnostic);
            }
            let seg_span = arena.span(segment.span);
            if segment.kind == RunKind::Plain
                && segment.accept.is_none()
                && run_form.propagation_written
                && let Some(target) =
                    literal_command_word(arena, self.linter.source, &segment.target)
                && expects_nonzero_status(&target)
            {
                let deletion_span =
                    scan_run_propagate_deletion_span(self.linter.source, arena.span(run_form.span));
                self.linter.diagnostics.push(
                    Diagnostic::new(
                        Severity::Warning,
                        "remove `?` when inspecting an expected nonzero status",
                    )
                    .with_code(DiagnosticCode::LintRunStatus)
                    .with_label(Label::secondary(
                        seg_span,
                        "nonzero status is expected for this command",
                    ))
                    .with_fix_hint(FixHint::deletion(
                        deletion_span,
                        "remove status propagation",
                    )),
                );
            }
        }
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
    pub(super) fn collect_assigned_names(&mut self, statements: &[StmtId]) {
        for &stmt in statements {
            self.collect_assigned_names_stmt(stmt);
        }
    }

    pub(super) fn collect_assigned_names_block(&mut self, block: BlockId) {
        let statements = self.arena.block(block).statements;
        for stmt in self.arena.stmt_ids(statements).collect::<Vec<_>>() {
            self.collect_assigned_names_stmt(stmt);
        }
    }

    pub(super) fn collect_assigned_names_stmt(&mut self, stmt_id: StmtId) {
        match self.arena.stmt(stmt_id).kind {
            ArenaStmtKind::Export(inner) => self.collect_assigned_names_stmt(inner),
            ArenaStmtKind::Assign { target, .. } => self.collect_assigned_names_target(target),
            ArenaStmtKind::ProcDef(def)
            | ArenaStmtKind::CliMain(def)
            | ArenaStmtKind::PureDef(def)
            | ArenaStmtKind::StreamDef(def) => {
                self.collect_assigned_names_block(self.arena.function_def(def).body);
            }
            ArenaStmtKind::SignalHook(hook) => {
                self.collect_assigned_names_block(self.arena.signal_hook(hook).body);
            }
            ArenaStmtKind::If {
                branches,
                else_block,
            } => {
                for branch in self.arena.if_branches(branches).to_vec() {
                    self.collect_assigned_names_block(branch.block);
                }
                if let Some(block) = else_block {
                    self.collect_assigned_names_block(block);
                }
            }
            ArenaStmtKind::While { block, .. }
            | ArenaStmtKind::For { block, .. }
            | ArenaStmtKind::Loop { block } => self.collect_assigned_names_block(block),
            ArenaStmtKind::Sugar { operands, .. } => {
                for operand in self.arena.sugar_operands(operands).to_vec() {
                    match operand {
                        ArenaSugarOperand::Block(block) => self.collect_assigned_names_block(block),
                        ArenaSugarOperand::Stmt(stmt) => self.collect_assigned_names_stmt(stmt),
                        _ => {}
                    }
                }
            }
            ArenaStmtKind::With {
                body, else_block, ..
            } => {
                self.collect_assigned_names_block(body);
                self.collect_assigned_names_block(else_block);
            }
            ArenaStmtKind::Guard { else_block, .. } => {
                self.collect_assigned_names_block(else_block);
            }
            ArenaStmtKind::Match { arms, .. } => {
                for arm in self.arena.match_arms(arms).to_vec() {
                    self.collect_assigned_names_block(arm.block);
                }
            }
            ArenaStmtKind::Use(_)
            | ArenaStmtKind::TypeDef(_)
            | ArenaStmtKind::ErrorDef(_)
            | ArenaStmtKind::Let { .. }
            | ArenaStmtKind::Const { .. }
            | ArenaStmtKind::Var { .. }
            | ArenaStmtKind::Return(_)
            | ArenaStmtKind::YieldDelegate(_)
            | ArenaStmtKind::Exit(_)
            | ArenaStmtKind::Yield(_)
            | ArenaStmtKind::Defer(..)
            | ArenaStmtKind::Break { .. }
            | ArenaStmtKind::Continue
            | ArenaStmtKind::Command(_)
            | ArenaStmtKind::TailBareIdent(_)
            | ArenaStmtKind::Assert { .. }
            | ArenaStmtKind::Expr(_) => {}
        }
    }

    pub(super) fn collect_assigned_names_target(&mut self, target: AssignTargetId) {
        match self.arena.assign_target(target).kind {
            ArenaAssignTargetKind::Name(name) => {
                self.assigned_names.insert(name);
            }
            ArenaAssignTargetKind::Env(_) => {}
            ArenaAssignTargetKind::Field { base, .. }
            | ArenaAssignTargetKind::Index { base, .. } => {
                self.collect_assigned_names_target(base);
            }
        }
    }

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
        let stmt = self.arena.stmt(stmt_id);
        self.lint_propagation(stmt_id);
        lint_redundant_discard::lint_redundant_discard(self, stmt_id);
        self.fail_candidates.visit_stmt(self.arena, stmt_id);
        self.set_like_bindings
            .visit_stmt(self.arena, self.source, stmt_id);
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
                self.lint_record_constructor(ty, &initializer);
                self.lint_empty_map_initializer(ty, &initializer);
                if let (Some(_), ArenaExprOrRun::Expr(value)) = (ty, &initializer) {
                    self.size_products.typed_initializer(self.arena, *value);
                }
                if let Some(type_expr) = ty {
                    self.collect_type_expr_refs(type_expr);
                    self.lint_needless_annotation(
                        target,
                        false,
                        type_expr,
                        &initializer,
                        exported,
                        stmt.span,
                    );
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
                self.declare_list_any_binding(target, ty, &initializer);
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
                self.lint_record_constructor(ty, &initializer);
                self.lint_empty_map_initializer(ty, &initializer);
                if let (Some(_), ArenaExprOrRun::Expr(value)) = (ty, &initializer) {
                    self.size_products.typed_initializer(self.arena, *value);
                }
                if let Some(type_expr) = ty {
                    self.collect_type_expr_refs(type_expr);
                    self.lint_needless_annotation(
                        target,
                        true,
                        type_expr,
                        &initializer,
                        exported,
                        stmt.span,
                    );
                }
                self.lint_expr_or_run(&initializer);
                self.define_binding_target(target, stmt.span, true);
                self.declare_list_any_binding(target, ty, &initializer);
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
                    self.lint_list_compound_assignment(target, value, stmt.span);
                }
                if let Some(definition) = lint_list_any_union::assigned_root(self.arena, target)
                    .and_then(|root| self.binding_definition(root))
                {
                    let mut bindings = std::mem::take(&mut self.list_any_bindings);
                    bindings.assign(
                        self.arena,
                        definition,
                        target,
                        op,
                        &value,
                        &self.expr_types,
                        &|name| self.binding_definition(name),
                    );
                    self.list_any_bindings = bindings;
                }
                self.lint_assign_target(target);
                self.lint_expr_or_run(&value);
            }
            ArenaStmtKind::ProcDef(def) | ArenaStmtKind::CliMain(def) => {
                let function = self.arena.function_def(def);
                let entrypoint = matches!(stmt.kind, ArenaStmtKind::CliMain(_))
                    || function.test_declaration
                    || (!exported && self.scopes.len() == 1 && function.name == "main");
                if self.scopes.len() == 1 {
                    self.callable_parameters.define(
                        function.name,
                        def,
                        false,
                        !exported && !entrypoint,
                    );
                }
                if matches!(stmt.kind, ArenaStmtKind::ProcDef(_)) {
                    self.lint_propagating_function(def, false, |linter| {
                        linter.lint_proc_function(def, exported, entrypoint, stmt.span);
                    });
                } else {
                    self.lint_proc_function(def, exported, entrypoint, stmt.span);
                }
                // Test declarations and program entrypoints have no restricted
                // callers, so a clause there is a bound the author chose.
                if !entrypoint {
                    self.lint_effect_annotation(def, stmt.span);
                }
            }
            ArenaStmtKind::PureDef(def) => {
                if self.scopes.len() == 1 {
                    let name = self.arena.function_def(def).name;
                    self.callable_parameters.define(name, def, true, !exported);
                }
                self.lint_inferred_pure_return(def, exported);
                self.lint_propagating_function(def, true, |linter| linter.lint_function(def));
            }
            ArenaStmtKind::StreamDef(def) => {
                self.lint_proc_function(def, exported, false, stmt.span);
                self.lint_effect_annotation(def, stmt.span);
            }
            ArenaStmtKind::SignalHook(hook_id) => {
                let body = self.arena.signal_hook(hook_id).body;
                self.lint_block(body);
            }
            ArenaStmtKind::Return(value) => {
                if let Some(value) = value {
                    self.lint_return_value(&value);
                    self.lint_return_path_parse_roundtrip(&value);
                    self.lint_return_redundant_require(&value);
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
                self.lint_list_pattern(branches, else_block, stmt.span);
                self.lint_if_as_guard(branches, else_block, stmt.span);
                self.lint_lexical_block(branches, else_block, stmt.span);
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
                self.lint_scalar_split_iteration(iter);
                self.lint_byte_iteration(stmt_id, target, iter, block);
                self.lint_map_entry_iteration(stmt_id, target, iter, block);
                self.lint_yield_delegation(stmt.span, target, iter, block);
                self.lint_prefer_file_lines(iter);
                prefer_repeat::lint_counted_loop(self, stmt.span, target, iter, block);
                self.lint_expr(iter);
                self.push_scope();
                self.define_binding_target(target, stmt.span, true);
                self.lint_block_statements(block);
                self.pop_scope();
            }
            ArenaStmtKind::Break { value } => {
                if let Some(expr) = value {
                    self.lint_expr(expr);
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
                    prefer_atomically::lint_body_that_never_finishes(self, stmt.span, body);
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
                self.define_binding_target(target, stmt.span, true);
                self.lint_block(else_block);
            }
            ArenaStmtKind::Assert { condition, message } => {
                self.lint_expr(condition);
                if let Some(message) = message {
                    self.lint_expr(message);
                }
            }
            ArenaStmtKind::Match { value, arms } => {
                self.lint_adjacent_pattern_arms(
                    self.arena
                        .match_arms(arms)
                        .iter()
                        .map(|arm| {
                            (
                                arm.pattern,
                                arm.guard,
                                Ok(arm.block),
                                self.arena.span(arm.span),
                            )
                        })
                        .collect(),
                );
                if let Some(arm) = self.arena.match_arms(arms).last() {
                    let report = lint_prefer_match_else::catch_all_arm_report(
                        self.arena,
                        self.source,
                        arm.pattern,
                        arm.guard,
                        arm.spelling,
                    );
                    self.catch_all_arms.extend(report);
                }
                let old_regex_context = self.regex_recovery_context;
                self.regex_recovery_context = true;
                self.lint_pattern_conditional_stmt(value, arms, stmt.span);
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
                let str_literal_arms = self
                    .arena
                    .match_arms(arms)
                    .iter()
                    .filter(|arm| {
                        matches!(
                            &self.arena.pattern(arm.pattern).kind,
                            ArenaPatternKind::Literal(expr) if matches!(self.arena.expr(*expr).kind, ArenaExprKind::Str(_))
                        )
                    })
                    .count();
                let has_catch_all = self.arena.match_arms(arms).iter().any(|arm| {
                    matches!(
                        self.arena.pattern(arm.pattern).kind,
                        ArenaPatternKind::Wildcard | ArenaPatternKind::Binding(_)
                    )
                });
                if str_literal_arms >= 3 && !has_catch_all {
                    self.warning(
                        stmt.span,
                        "3+ string-literal match arms — consider defining a tag union type",
                        DiagnosticCode::LintStringlyTypedMatch,
                        "tag unions are safer and exhaustiveness-checked",
                    );
                }
            }
            ArenaStmtKind::Command(command) => self.lint_command_stmt(command),
            ArenaStmtKind::TailBareIdent(name) => {
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
                self.lint_core_assert(stmt.span, expr);
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
                for binding in self.arena.with_bindings(bindings).to_vec() {
                    self.lint_expr_or_run(&ArenaExprOrRun::Expr(binding.initializer));
                }
                self.propagation_boundary_depth += 1;
                self.lint_block(body);
                self.lint_block(else_block);
                self.propagation_boundary_depth -= 1;
                self.regex_recovery_context = old_regex_context;
            }
        }
    }

    pub(super) fn lint_function(&mut self, def_id: FunctionDefId) {
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
        self.function_return_types.push(return_ty);
        self.function_bodies.push(def.body);
        if result_unit {
            self.lint_redundant_bare_return(def.body);
        }
        if result_path {
            self.lint_tail_path_parse_roundtrip(def.body);
        }
        if result_ok.is_some() {
            self.lint_tail_redundant_require(def.body);
            self.lint_tail_redundant_ok_return(def.body, result_ok.as_ref());
        }
        self.lint_redundant_tail_return_binding(def.body);
        self.lint_redundant_tail_return(
            def.body,
            self.function_return_types.last().cloned().as_ref().unwrap(),
        );
        if !def.return_ty_defaulted {
            self.collect_type_expr_refs(def.return_ty);
        }
        for param in self.arena.params(def.params).to_vec() {
            self.lint_default_parameter_annotation(&param);
            if !param.ty_defaulted {
                self.collect_type_expr_refs(param.ty);
            }
            if let Some(default) = param.default {
                self.lint_expr(default);
            }
            self.define(
                param.name.as_str().as_str(),
                self.arena.span(param.span),
                true,
            );
        }
        self.lint_block_statements(def.body);
        self.result_unit_functions.pop();
        self.result_path_functions.pop();
        self.result_return_ok_types.pop();
        self.function_return_types.pop();
        self.function_bodies.pop();
        self.pop_scope();
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
        self.lint_inferred_variant_pattern(span);
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
        self.lint_list_comp_suggestions(&stmts);
        prefer_tempdir::lint_scratch_directories(self, &stmts, Some(block));
        prefer_atomically::lint_published_files(self, &stmts, Some(block));
        lint_prefer_for_index::lint_counter_loops(self, &stmts, Some(block));
        if self.prefer_text_pattern {
            lint_prefer_text_pattern::lint_positional_text(self, &stmts);
        }
        prefer_within::lint_repeated_timeouts(self, &stmts);
        prefer_collect::lint_built_lists(self, &stmts);
        prefer_wait_until::lint_polling_loops(self, &stmts);
        if self.prefer_with_scope {
            prefer_with_scope::lint_deferred_releases(self, &stmts);
        }
        self.lint_statement_sequence(&stmts);
    }

    pub(super) fn lint_command_stmt(&mut self, stmt_id: CommandStmtId) {
        let stmt = self.arena.command_stmt(stmt_id).clone();
        let span = self.arena.span(stmt.span);
        match stmt.command {
            ArenaCommand::Proc { name, args } => {
                self.lint_interactive_command(name.as_str().as_str(), span);
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
        let run = self.arena.run_form(run_id).clone();
        if self.runless {
            for segment in self.arena.run_segments(run.segments).to_vec() {
                let seg_span = self.arena.span(segment.span);
                let name = command_name(self.arena, self.source, &segment.target);
                let exempt = name
                    .as_deref()
                    .is_some_and(|n| self.runless_except.iter().any(|e| e == n));
                if !exempt {
                    let label = match &name {
                        Some(n) => format!("`{n}` is an external command"),
                        None => "external command not allowed in runless mode".to_string(),
                    };
                    self.diagnostics.push(
                        Diagnostic::new(
                            Severity::Error,
                            "external command not permitted (--runless)",
                        )
                        .with_code(DiagnosticCode::LintRunless)
                        .with_label(Label::secondary(seg_span, label)),
                    );
                }
            }
        }
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
        .visit_command_arg(arg, false);
    }

    pub(super) fn lint_proc_command_arg(&mut self, arg: &ArenaCommandArg) {
        LintExprVisitor {
            linter: self,
            suppress_expr_autofixes: true,
        }
        .visit_command_arg(arg, true);
    }

    pub(super) fn lint_expr_or_run(&mut self, value: &ArenaExprOrRun) {
        match value {
            ArenaExprOrRun::Expr(expr) => self.lint_expr(*expr),
            ArenaExprOrRun::Run(run) => self.lint_run(*run),
        }
    }

    pub(super) fn lint_expr(&mut self, expr: ExprId) {
        LintExprVisitor {
            linter: self,
            suppress_expr_autofixes: false,
        }
        .visit_expr(expr);
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
        if self.prefer_item_shorthand {
            self.item_shorthands.extend(item_shorthand::stage_report(
                self.arena,
                self.source,
                stage,
            ));
        }
        self.lint_stage_callable_wrapper(stage);
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
        if self.is_defined_in_outer_scope(name) && !is_predeclared_script_args(name) && name != "_"
        {
            self.diagnostics.push(
                Diagnostic::error("binding shadows an outer name")
                    .with_code(DiagnosticCode::LintShadowing)
                    .with_label(Label::secondary(span, "shadowed binding starts here")),
            );
        }
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
                    report_unused: report_unused && name != "_",
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
        self.scopes.push(FxHashMap::default());
    }

    pub(super) fn pop_scope(&mut self) {
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
        for (name, binding) in unused {
            self.warning(
                binding.span,
                format!("unused local variable `{name}`"),
                DiagnosticCode::LintUnusedLocal,
                "binding is never read",
            );
        }
    }

    pub(super) fn expr_or_run_span(&self, value: &ArenaExprOrRun) -> Span {
        match value {
            ArenaExprOrRun::Expr(expr) => self.arena.expr(*expr).span,
            ArenaExprOrRun::Run(run) => self.arena.span(self.arena.run_form(*run).span),
        }
    }
}
