use super::{
    ArenaCallArgKind, ArenaExprKind, ArenaExprOrRun, ArenaRecordFieldKind, ArenaStmtKind,
    AstArena, BinaryOp, Diagnostic, DiagnosticCode, ExprId, FixHint, Label, Linter, Name,
    Severity, Span, StmtId, Type, is_module_call, list_splice_element_type_is_precise,
    pipeline_argument_expr, same_ordering_operand, span_may_contain_comment,
    widen_over_grouping,
};

#[cfg(test)]
use super::nested_pipeline_index_tests;

struct ExpressionRule {
    code: DiagnosticCode,
    enabled: fn(&Linter<'_>) -> bool,
    // The source arena lifetime is independent of each hook's mutable borrow.
    apply: for<'source> fn(&mut Linter<'source>, ExprId),
}

fn always_enabled(_: &Linter<'_>) -> bool { true }
fn env_string_enabled(linter: &Linter<'_>) -> bool { linter.prefer_env_string }

// The order is the expression's established reporting order. Rules that
// resolve overlapping rewrites can read findings from preceding hooks.
const EXPRESSION_RULES: &[ExpressionRule] = &[
    ExpressionRule { code: DiagnosticCode::LintRedundantOptionalFallback, enabled: always_enabled, apply: |linter, expr| linter.lint_proven_nonnull_fallback(expr) },
    ExpressionRule { code: DiagnosticCode::LintPreferNestedRecordUpdate, enabled: always_enabled, apply: |linter, expr| linter.lint_nested_record_update(expr) },
    ExpressionRule { code: DiagnosticCode::LintPreferValuePipeline, enabled: always_enabled, apply: |linter, expr| linter.lint_nested_value_pipeline(expr) },
    ExpressionRule { code: DiagnosticCode::LintPreferBlockString, enabled: always_enabled, apply: |linter, expr| linter.lint_block_string_concatenation(expr) },
    ExpressionRule { code: DiagnosticCode::LintPreferListSplicing, enabled: always_enabled, apply: |linter, expr| linter.lint_list_splicing(expr) },
    ExpressionRule { code: DiagnosticCode::LintPreferMapLiteral, enabled: always_enabled, apply: |linter, expr| linter.lint_map_literal_chain(expr) },
    ExpressionRule { code: DiagnosticCode::LintPreferRegexLiteral, enabled: always_enabled, apply: |linter, expr| linter.lint_prepared_regex(expr) },
    ExpressionRule { code: DiagnosticCode::LintPreferEnvString, enabled: env_string_enabled, apply: |linter, expr| linter.lint_env_string(expr) },
    ExpressionRule { code: DiagnosticCode::LintPreferComparisonChain, enabled: always_enabled, apply: |linter, expr| linter.lint_comparison_chain(expr) },
    ExpressionRule { code: DiagnosticCode::LintLookupAbsence, enabled: always_enabled, apply: |linter, expr| linter.lint_lookup_sentinel(expr) },
    ExpressionRule { code: DiagnosticCode::LintPreferOptionalPostfix, enabled: always_enabled, apply: |linter, expr| linter.lint_optional_postfix(expr) },
];

pub(super) fn expression(linter: &mut Linter<'_>, expr: ExprId) {
    for rule in EXPRESSION_RULES {
        if !(rule.enabled)(linter) { continue; }
        let first = linter.diagnostics.len();
        (rule.apply)(linter, expr);
        debug_assert!(linter.diagnostics[first..].iter().all(|finding| finding.code == Some(rule.code)),
            "expression rule emitted another rule's code: {}", rule.code.name());
    }
}

#[cfg(test)]
mod tests {
    use super::EXPRESSION_RULES;
    use xsh::diagnostic::DiagnosticFamily;

    #[test]
    fn expression_declarations_have_unique_lint_codes() {
        for (index, rule) in EXPRESSION_RULES.iter().enumerate() {
            assert_eq!(rule.code.family(), DiagnosticFamily::Lint);
            assert!(EXPRESSION_RULES[..index].iter().all(|previous| previous.code != rule.code),
                "duplicate expression rule: {}", rule.code.name());
        }
    }
}

impl<'a> Linter<'a> {
    pub(super) fn lint_block_string_concatenation(&mut self, expr: ExprId) {
        fn collect(arena: &AstArena, expr: ExprId, output: &mut String, links: &mut usize) -> bool {
            match arena.expr(expr).kind {
                ArenaExprKind::Str(text) => {
                    output.push_str(arena.string_literal(text));
                    true
                }
                ArenaExprKind::Binary {
                    op: BinaryOp::Add,
                    left,
                    right,
                } => {
                    *links += 1;
                    collect(arena, left, output, links) && collect(arena, right, output, links)
                }
                _ => false,
            }
        }
        let span = self.arena.expr(expr).span;
        let mut value = String::new();
        let mut links = 0;
        if !collect(self.arena, expr, &mut value, &mut links)
            || links == 0
            || !value.contains('\n')
            || value.contains('\r')
        {
            return;
        }
        // Trailing spaces or tabs on a line would become invisible trailing
        // whitespace in a block string, which editors and formatters strip.
        if value.split('\n').any(|line| line.ends_with([' ', '\t'])) {
            return;
        }
        // Report the outermost literal chain once; its operands are part of the same rewrite.
        if self.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(DiagnosticCode::LintPreferBlockString)
                && diagnostic.labels.iter().any(|label| {
                    label.span.source_id == span.source_id
                        && label.span.start() <= span.start()
                        && span.end() <= label.span.end()
                })
        }) {
            return;
        }
        let diagnostic = Diagnostic::new(
            Severity::Warning,
            "constant multiline concatenation can use a block string",
        )
        .with_code(DiagnosticCode::LintPreferBlockString)
        .with_label(Label::secondary(
            span,
            "all pieces are literal text in source order",
        ));
        // Comments and a closing delimiter that cannot stand alone are layout
        // facts: they decide only whether the rewrite is offered.
        let span = widen_over_grouping(self.source, span);
        let suffix = self.source[span.end()..]
            .split(['\r', '\n'])
            .next()
            .unwrap_or("");
        if span_may_contain_comment(self.source, span)
            || !suffix.bytes().all(|byte| matches!(byte, b' ' | b'\t'))
        {
            self.diagnostics.push(diagnostic.with_note(
                "comments, trailing code, and expression consumers need a manual rewrite",
            ));
            return;
        }
        let line_start = self.source[..span.start()]
            .rfind(['\r', '\n'])
            .map_or(0, |offset| offset + 1);
        let indent: String = self.source[line_start..span.start()]
            .chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .collect();
        let margin = format!("{indent}  ");
        let escaped = value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('\0', "\\0");
        let content = escaped
            .split('\n')
            .map(|line| format!("{margin}{line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let replacement = format!("\"\"\"\n{content}\n{margin}\"\"\"");
        let witness = format!("let block_value = {replacement}\n");
        let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
            xsh::frontend::source::SourceId::new(0),
            &witness,
        );
        let reproduced = parsed.diagnostics.is_empty() && parsed.arena.statement_ids().next().is_some_and(|statement| matches!(
            parsed.arena.arena.stmt(statement).kind,
            ArenaStmtKind::Let { initializer: ArenaExprOrRun::Expr(candidate), .. }
                if matches!(parsed.arena.arena.expr(candidate).kind, ArenaExprKind::Str(text) if parsed.arena.arena.string_literal(text).as_ref() == value)
        ));
        self.diagnostics.push(if reproduced {
            diagnostic.with_fix_hint(FixHint::replacement(
                span,
                "preserve the exact decoded text",
                replacement,
            ))
        } else {
            diagnostic.with_note(
                "a block literal at this indentation would not reproduce the decoded text",
            )
        });
    }

    pub(super) fn lint_comparison_chain(&mut self, expr: ExprId) {
        fn ordering(op: BinaryOp) -> bool {
            matches!(
                op,
                BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
            )
        }
        fn ladder(
            arena: &AstArena,
            expr: ExprId,
            out: &mut Vec<(BinaryOp, ExprId, ExprId)>,
        ) -> bool {
            match arena.expr(expr).kind {
                ArenaExprKind::Binary {
                    op: BinaryOp::And,
                    left,
                    right,
                } => ladder(arena, left, out) && ladder(arena, right, out),
                ArenaExprKind::Binary { op, left, right } if ordering(op) => {
                    out.push((op, left, right));
                    true
                }
                _ => false,
            }
        }
        let span = self.arena.expr(expr).span;
        if self.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(DiagnosticCode::LintPreferComparisonChain)
                && diagnostic.labels.iter().any(|label| {
                    label.span.source_id == span.source_id
                        && label.span.start() <= span.start()
                        && label.span.end() >= span.end()
                })
        }) {
            return;
        }
        if !matches!(
            self.arena.expr(expr).kind,
            ArenaExprKind::Binary {
                op: BinaryOp::And,
                ..
            }
        ) {
            return;
        }
        let mut pairs = Vec::new();
        if !ladder(self.arena, expr, &mut pairs) || pairs.len() < 2 {
            return;
        }
        let stable = |id| match self.arena.expr(id).kind {
            ArenaExprKind::Ident(name) => {
                self.scopes
                    .iter()
                    .rev()
                    .find_map(|scope| scope.get(name.as_str().as_str()))
                    .is_some_and(|binding| binding.comparison_stable)
                    && !self.assigned_names.contains(&name)
            }
            ArenaExprKind::Int(_) | ArenaExprKind::Float(_) | ArenaExprKind::Str(_) => true,
            _ => false,
        };
        for adjacent in pairs.windows(2) {
            let shared = adjacent[0].2;
            let repeated = adjacent[1].1;
            if !stable(shared)
                || !stable(repeated)
                || !same_ordering_operand(self.arena, shared, repeated)
            {
                return;
            }
        }
        let Some(original) = self.source.get(span.range()) else {
            return;
        };
        if original.contains('#') {
            return;
        }
        let operand_text = |id| {
            let expr = self.arena.expr(id);
            let text = self.source.get(expr.span.range())?;
            let needs_grouping = matches!(
                expr.kind,
                ArenaExprKind::Binary {
                    op: BinaryOp::ResultFallback
                        | BinaryOp::Or
                        | BinaryOp::And
                        | BinaryOp::Eq
                        | BinaryOp::Ne
                        | BinaryOp::Lt
                        | BinaryOp::Le
                        | BinaryOp::Gt
                        | BinaryOp::Ge
                        | BinaryOp::In
                        | BinaryOp::NotIn,
                    ..
                } | ArenaExprKind::ComparisonChain(_)
                    | ArenaExprKind::If { .. }
                    | ArenaExprKind::Match { .. }
                    | ArenaExprKind::Pipeline { .. }
                    | ArenaExprKind::StructuredPipeline { .. }
            );
            Some(if needs_grouping {
                format!("({text})")
            } else {
                text.to_string()
            })
        };
        let Some(mut replacement) = operand_text(pairs[0].1) else {
            return;
        };
        for (op, _, right) in pairs {
            let Some(text) = operand_text(right) else {
                return;
            };
            replacement.push_str(match op {
                BinaryOp::Lt => " < ",
                BinaryOp::Le => " <= ",
                BinaryOp::Gt => " > ",
                BinaryOp::Ge => " >= ",
                _ => unreachable!(),
            });
            replacement.push_str(&text);
        }
        self.diagnostics.push(
            Diagnostic::new(Severity::Warning, "prefer an ordering comparison chain")
                .with_code(DiagnosticCode::LintPreferComparisonChain)
                .with_label(Label::secondary(
                    span,
                    "the repeated adjacent operand is stable",
                ))
                .with_fix_hint(FixHint::replacement(
                    span,
                    "compare adjacent operands once",
                    replacement,
                )),
        );
    }

    pub(super) fn lint_nested_value_pipeline(&mut self, value: ExprId) {
        let Some((_, args)) = self.pipeline_ordinary_call(value) else {
            return;
        };
        // Whole statement values need no new parentheses or precedence rules.
        let whole_values = self.whole_statement_values.get_or_init(|| {
            (0..self.arena.stmt_tags.len())
                .filter_map(|raw| {
                    #[cfg(test)]
                    nested_pipeline_index_tests::record_statement();
                    match self.arena.stmt(StmtId::from_index(raw)).kind {
                        ArenaStmtKind::Let {
                            initializer: ArenaExprOrRun::Expr(expr),
                            ..
                        }
                        | ArenaStmtKind::Var {
                            initializer: ArenaExprOrRun::Expr(expr),
                            ..
                        }
                        | ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(expr)))
                        | ArenaStmtKind::Expr(expr) => Some(expr),
                        _ => None,
                    }
                })
                .collect()
        });
        if !whole_values.contains(&value) {
            return;
        }
        let args = self.arena.call_args(args);
        let inputs = args
            .iter()
            .enumerate()
            .filter_map(|(index, arg)| {
                let expr = pipeline_argument_expr(arg).unwrap();
                self.pipeline_ordinary_call(expr).map(|_| (index, expr))
            })
            .collect::<Vec<_>>();
        let [(index, input)] = inputs.as_slice() else {
            return;
        };
        if args[..*index]
            .iter()
            .any(|arg| !self.pipeline_argument_stable(pipeline_argument_expr(arg).unwrap()))
        {
            return;
        }
        let span = self.arena.expr(value).span;
        let input_span = self.arena.expr(*input).span;
        let Some(original) = self.source.get(span.range()) else {
            return;
        };
        let Some(input_text) = self.source.get(input_span.range()) else {
            return;
        };
        let mut stage = original.to_string();
        stage.replace_range(
            input_span.start() - span.start()..input_span.end() - span.start(),
            "_",
        );
        let replacement = format!("{input_text} |> {stage}");
        if !self.pipeline_rewrite_preserves_types(
            span,
            &replacement,
            value,
            *input,
            span.start(),
            span.start(),
        ) {
            return;
        }
        let mut diagnostic =
            Diagnostic::new(Severity::Warning, "nested calls form a value pipeline")
                .with_code(DiagnosticCode::LintPreferValuePipeline)
                .with_label(Label::secondary(
                    span,
                    "place the retained input at an explicit argument hole",
                ));
        if !original.contains('#') {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                span,
                "use an explicit value pipeline",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    pub(super) fn lint_nested_record_update(&mut self, expr: ExprId) {
        fn selected_path(arena: &AstArena, expr: ExprId, root: Name) -> Option<Vec<Name>> {
            match arena.expr(expr).kind {
                ArenaExprKind::Ident(name) if name == root => Some(Vec::new()),
                ArenaExprKind::Field { base, name } => {
                    let mut path = selected_path(arena, base, root)?;
                    path.push(name);
                    Some(path)
                }
                _ => None,
            }
        }
        fn replacements(
            arena: &AstArena,
            expr: ExprId,
            root: Name,
            prefix: &[Name],
            output: &mut Vec<(Vec<Name>, Option<ExprId>)>,
        ) -> Option<()> {
            let ArenaExprKind::Record(fields) = arena.expr(expr).kind else {
                return None;
            };
            let fields = arena.record_fields(fields);
            let ArenaRecordFieldKind::Spread { expr: spread, .. } = fields.first()?.kind else {
                return None;
            };
            if selected_path(arena, spread, root)?.as_slice() != prefix {
                return None;
            }
            for field in fields.iter().skip(1) {
                let (name, value) = match field.kind {
                    ArenaRecordFieldKind::Named { name, value, .. } => (name, Some(value)),
                    ArenaRecordFieldKind::Shorthand { name, .. } => (name, None),
                    _ => return None,
                };
                let mut path = prefix.to_vec();
                path.push(name);
                if let Some(value) = value
                    && matches!(arena.expr(value).kind, ArenaExprKind::Record(inner) if matches!(arena.record_fields(inner).first().map(|f| &f.kind), Some(ArenaRecordFieldKind::Spread { .. })))
                {
                    replacements(arena, value, root, &path, output)?;
                } else {
                    output.push((path, value));
                }
            }
            Some(())
        }
        let expression = self.arena.expr(expr);
        let ArenaExprKind::Record(fields) = expression.kind else {
            return;
        };
        let Some(ArenaRecordFieldKind::Spread { expr: base, .. }) = self
            .arena
            .record_fields(fields)
            .first()
            .map(|field| &field.kind)
        else {
            return;
        };
        let ArenaExprKind::Ident(root) = self.arena.expr(*base).kind else {
            return;
        };
        let stable = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(root.as_str().as_str()))
            .is_some_and(|binding| !binding.mutable);
        if !stable || self.assigned_names.contains(&root) {
            return;
        }
        let Some(base_ty @ Type::Record(shape)) = self.expr_types.get(&self.arena.expr(*base).span)
        else {
            return;
        };
        if shape.is_empty() {
            return;
        }
        let original = &self.source[expression.span.range()];
        if original.contains('#') {
            return;
        }
        let mut updates = Vec::new();
        if replacements(self.arena, expr, root, &[], &mut updates).is_none()
            || !updates.iter().any(|(path, _)| path.len() > 1)
        {
            return;
        }
        let mut entries = Vec::new();
        for (index, (path, value)) in updates.iter().enumerate() {
            if updates[..index]
                .iter()
                .any(|(prior, _)| path.starts_with(prior) || prior.starts_with(path))
            {
                return;
            }
            let mut selected = base_ty;
            for name in path {
                let label = name.as_str();
                if !label
                    .chars()
                    .next()
                    .is_some_and(|c| c == '_' || c.is_alphabetic())
                    || !label.chars().all(|c| c == '_' || c.is_alphanumeric())
                {
                    return;
                }
                let Type::Record(fields) = selected else {
                    return;
                };
                let Some(field_ty) = fields.get(name) else {
                    return;
                };
                selected = field_ty;
            }
            if let Some(value) = value {
                let Some(actual) = self.expr_types.get(&self.arena.expr(*value).span) else {
                    return;
                };
                if actual.contains_any()
                    || (actual.is_dynamic() && !selected.is_dynamic())
                    || !actual.matches_expected(selected)
                {
                    return;
                }
            }
            let path_text = path
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(".");
            let value_text = value
                .map(|value| &self.source[self.arena.expr(value).span.range()])
                .map(str::to_string)
                .unwrap_or_else(|| path.last().unwrap().to_string());
            entries.push(format!("{path_text}: {value_text}"));
        }
        let replacement = format!("{{...{root}, {}}}", entries.join(", "));
        self.diagnostics.push(
            Diagnostic::warning("nested record spreads can use disjoint update paths")
                .with_code(DiagnosticCode::LintPreferNestedRecordUpdate)
                .with_label(Label::secondary(
                    expression.span,
                    "the repeated record reads are stable and statically known",
                ))
                .with_fix_hint(FixHint::replacement(
                    expression.span,
                    "replace nested spreads with static field paths",
                    replacement,
                )),
        );
    }

    pub(super) fn lint_lookup_sentinel(&mut self, expr: ExprId) {
        let node = self.arena.expr(expr);
        let ArenaExprKind::Binary {
            op: BinaryOp::Eq | BinaryOp::Ne,
            left,
            right,
        } = node.kind
        else {
            return;
        };
        let sentinel = if self.proven_absence_lookup(left) && self.is_negative_one_literal(right) {
            right
        } else if self.proven_absence_lookup(right) && self.is_negative_one_literal(left) {
            left
        } else {
            return;
        };
        if self.source[node.span.range()].contains('#') {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "lookup absence is null rather than a numeric sentinel",
            )
            .with_code(DiagnosticCode::LintLookupAbsence)
            .with_label(Label::secondary(
                node.span,
                "compare the proved lookup result with null",
            ))
            .with_fix_hint(FixHint::replacement(
                self.arena.expr(sentinel).span,
                "use absence",
                "null",
            )),
        );
    }

    pub(super) fn lint_prepared_regex(&mut self, expr: ExprId) {
        if self.regex_recovery_context {
            return;
        }
        let outer = self.arena.expr(expr);
        let ArenaExprKind::Try(inner) = outer.kind else {
            return;
        };
        let call = self.arena.expr(inner);
        let ArenaExprKind::Call { callee, args } = call.kind else {
            return;
        };
        if !is_module_call(self.arena, callee, "regex", "compile")
            || self.expr_types.get(&outer.span) != Some(&Type::Regex)
            || args.len() != 1
        {
            return;
        }
        let argument = match self.arena.call_args(args)[0].kind {
            ArenaCallArgKind::Positional(value) => value,
            ArenaCallArgKind::Named { name, value, .. } if name == "pattern" => value,
            _ => return,
        };
        let value = self.arena.expr(argument);
        let ArenaExprKind::Str(text) = value.kind else {
            return;
        };
        let pattern = self.arena.string_literal(text);
        // Reparse the proposed raw spelling to prove delimiter and decoded-text identity.
        let replacement = if !pattern.contains('"') && !pattern.contains(['\n', '\r']) {
            format!("rx\"{pattern}\"")
        } else {
            format!("rx\"\"\"{pattern}\"\"\"")
        };
        let candidate = format!("let prepared = {replacement}\n");
        let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
            outer.span.source_id,
            &candidate,
        );
        if !parsed.diagnostics.is_empty()
            || parsed.arena.arena.regex_literals.len() != 1
            || parsed.arena.arena.regex_literals[0].pattern.as_ref() != pattern.as_ref()
            || !xsh::frontend::check::Checker::check_arena(&parsed.arena, &candidate)
                .diagnostics
                .is_empty()
        {
            return;
        }
        let comments = self.source[outer.span.start()..value.span.start()].contains('#')
            || self.source[value.span.end()..outer.span.end()].contains('#');
        let diagnostic = Diagnostic::new(
            Severity::Warning,
            "prepare static regex patterns with a literal",
        )
        .with_code(DiagnosticCode::LintPreferRegexLiteral)
        .with_label(Label::secondary(
            outer.span,
            "this validated pattern is directly propagated",
        ));
        self.diagnostics.push(if comments {
            diagnostic.with_note("no automatic fix: the call contains comments")
        } else {
            diagnostic.with_fix_hint(FixHint::replacement(
                outer.span,
                "use a prepared regex literal",
                replacement,
            ))
        });
    }

    /// `env.get("NAME")` and `env.Str.NAME` read exactly what `e"NAME"`
    /// reads: the same lookup, the same `Result[Str]`, the same failures. Only
    /// literal identifier names are rewritten. `env.get_or` is left alone:
    /// it fails on a value that is not UTF-8, while `e"NAME" ?? fallback`
    /// would fall back, and it returns a `Result` where `??` returns `Str`.
    pub(super) fn lint_env_string(&mut self, expr: ExprId) {
        if !self.prefer_env_string {
            return;
        }
        let outer = self.arena.expr(expr);
        let name = match outer.kind {
            ArenaExprKind::Call { callee, args }
                if is_module_call(self.arena, callee, "env", "get") && args.len() == 1 =>
            {
                let argument = match self.arena.call_args(args)[0].kind {
                    ArenaCallArgKind::Positional(value) => value,
                    ArenaCallArgKind::Named { name, value, .. } if name == "name" => value,
                    _ => return,
                };
                let ArenaExprKind::Str(text) = self.arena.expr(argument).kind else {
                    return;
                };
                self.arena.string_literal(text).to_string()
            }
            ArenaExprKind::Field { base, name } => {
                let ArenaExprKind::Field {
                    base: module,
                    name: kind,
                } = self.arena.expr(base).kind
                else {
                    return;
                };
                if kind != "Str"
                    || !matches!(self.arena.expr(module).kind, ArenaExprKind::Ident(module) if module == "env")
                {
                    return;
                }
                name.as_str().to_string()
            }
            _ => return,
        };
        // The checked type proves `env` is the module, and the spelling
        // check keeps `$env.Str.NAME` command words and comments untouched.
        let text = &self.source[outer.span.range()];
        if !xsh::frontend::syntax::literal::is_env_string_name(&name)
            || self.scopes.iter().any(|scope| scope.contains_key("env"))
            || self.expr_types.get(&outer.span)
                != Some(&Type::Result(Box::new(Type::Str), Box::new(Type::Error)))
            || !text.starts_with("env.")
            || text.contains('#')
            || self.source[..outer.span.start()].ends_with('$')
        {
            return;
        }
        let replacement = format!("e\"{name}\"");
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "read an environment variable with a literal name as an e-string",
            )
            .with_code(DiagnosticCode::LintPreferEnvString)
            .with_label(Label::secondary(
                outer.span,
                "this reads one named variable",
            ))
            .with_fix_hint(FixHint::replacement(
                outer.span,
                format!("write `{replacement}`"),
                replacement,
            )),
        );
    }

    pub(super) fn map_literal_diagnostic(&mut self, span: Span, replacement: String) {
        let Some(source) = self.source.get(span.range()) else {
            return;
        };
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "prefer a Map literal for fresh Map construction",
        )
        .with_code(DiagnosticCode::LintPreferMapLiteral)
        .with_label(Label::secondary(span, "construct the entries in one Map"));
        if source.contains('#') {
            diagnostic =
                diagnostic.with_note("comments in the initialization require a manual rewrite");
        } else {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                span,
                "construct one Map literal",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    pub(super) fn lint_map_literal_chain(&mut self, expr: ExprId) {
        let Some(Type::Map(key_ty, element)) =
            self.expr_types.get(&self.arena.expr(expr).span).cloned()
        else {
            return;
        };
        if !list_splice_element_type_is_precise(&element) {
            return;
        }
        let mut base = expr;
        let mut entries = Vec::new();
        while let Some((receiver, key, value)) = self.map_set_parts(base, &element) {
            entries.push((key, value));
            base = receiver;
        }
        if entries.is_empty() {
            return;
        }
        entries.reverse();
        let Some(mut replacement) = self.map_literal_replacement(&entries) else {
            return;
        };
        if !self.is_empty_map_literal_source(base) {
            let ArenaExprKind::Record(fields) = self.arena.expr(base).kind else {
                return;
            };
            if self.expr_types.get(&self.arena.expr(base).span)
                != Some(&Type::Map(key_ty.clone(), element.clone()))
            {
                return;
            }
            let mut originals = Vec::new();
            for field in self.arena.record_fields(fields) {
                let (value, span) = match field.kind {
                    ArenaRecordFieldKind::Computed { key, value, span } => {
                        if self.expr_types.get(&self.arena.expr(key).span) != Some(key_ty.as_ref())
                        {
                            return;
                        }
                        (value, span)
                    }
                    ArenaRecordFieldKind::Named { value, span, .. } => (value, span),
                    _ => return,
                };
                if self.expr_types.get(&self.arena.expr(value).span) != Some(element.as_ref()) {
                    return;
                }
                let Some(text) = self.source.get(self.arena.span(span).range()) else {
                    return;
                };
                originals.push(text);
            }
            if !originals.is_empty() {
                replacement = format!(
                    "{{{}, {}}}",
                    originals.join(", "),
                    &replacement[1..replacement.len() - 1]
                );
            }
        }
        self.map_literal_diagnostic(self.arena.expr(expr).span, replacement);
    }

    pub(super) fn lint_list_splicing(&mut self, expr: ExprId) {
        let node = self.arena.expr(expr);
        if !matches!(
            node.kind,
            ArenaExprKind::Binary {
                op: BinaryOp::Add,
                ..
            } | ArenaExprKind::Call { .. }
        ) {
            return;
        }
        let Some(expected @ Type::List(element)) = self.expr_types.get(&node.span) else {
            return;
        };
        if !list_splice_element_type_is_precise(element) {
            return;
        }
        let mut parts = Vec::new();
        let mut links = 0;
        let mut literal = false;
        if self
            .collect_list_splice_parts(expr, expected, &mut parts, &mut links, &mut literal)
            .is_none()
            || links == 0
            || (links == 1 && !literal)
        {
            return;
        }
        // A simple receiver update already has a compound-assignment spelling.
        if links == 1 && matches!(node.kind, ArenaExprKind::Call { .. }) {
            return;
        }
        let Some(source) = self.source.get(node.span.range()) else {
            return;
        };
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "prefer one list literal with explicit splices for list construction",
        )
        .with_code(DiagnosticCode::LintPreferListSplicing)
        .with_label(Label::secondary(
            node.span,
            "build the elements in one literal",
        ));
        if source.contains('#') {
            diagnostic =
                diagnostic.with_note("comments in the construction require a manual rewrite");
        } else {
            let mut entries = Vec::with_capacity(parts.len());
            for (splice, value) in parts {
                let Some(source) = self.source.get(self.arena.expr(value).span.range()) else {
                    return;
                };
                entries.push(if splice {
                    format!("@{source}")
                } else {
                    source.to_string()
                });
            }
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                node.span,
                "build one spliced list literal",
                format!("[{}]", entries.join(", ")),
            ));
        }
        self.diagnostics.push(diagnostic);
    }
}
