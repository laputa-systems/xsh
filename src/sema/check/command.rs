#![allow(clippy::single_call_fn)]

use super::{
    BTreeMap, Checker, CoreCommand, Diagnostic, Effect, FixHint, FxHashSet, Label, ModuleFnSig,
    Name, RunKind, Span, Type, UnaryOp, api_spec,
};
use crate::syntax::arena::{
    ArenaCommand, ArenaCommandArg, ArenaCommandArgKind, ArenaEnvAssignment,
    ArenaEnvAssignmentValue, ArenaExprKind, ArenaProgram, ArenaRange, ArenaRedirection,
    ArenaRedirectionTarget, ArenaRunSegment, ArenaWordPart, BlockId, CommandStmtId, ExprId,
    RunFormId,
};
use crate::syntax::node::{CommandWordRefSegment, parse_command_word_reference};

fn btree_map<K: Into<Name>, V>(entries: Vec<(K, V)>) -> BTreeMap<Name, V> {
    entries
        .into_iter()
        .map(|(name, value)| (name.into(), value))
        .collect()
}

pub(super) fn command_ty_auto_propagates(ty: &Type) -> bool {
    ty.is_result_unit()
}

pub(super) fn standard_module_command_name(name: &str) -> Option<(&str, &str)> {
    let (module, api) = name.split_once('.')?;
    api_spec()
        .is_standard_module(module)
        .then_some((module, api))
}

pub(super) fn module_sig_is_command_callable(sig: &ModuleFnSig) -> bool {
    sig.command
}

fn is_bare_ident(text: &str) -> bool {
    if matches!(text, "true" | "false" | "null") {
        return false;
    }
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

pub(super) fn valid_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[allow(dead_code)]
impl Checker {
    pub(super) fn check_command_word_reference(&mut self, text: &str, span: Span) -> Option<Type> {
        let (root, segments) = parse_command_word_reference(text)?;
        let mut ty = self.lookup(Name::intern(root))?.ty.clone();
        for segment in segments {
            ty = match segment {
                CommandWordRefSegment::Field(name) => {
                    self.field_type_for_value(ty, &name.as_str(), span)
                }
                CommandWordRefSegment::Index(_) => self.index_type_for_value(ty, span),
            };
        }
        Some(ty)
    }

    pub(super) fn field_type_for_value(&mut self, base_ty: Type, name: &str, span: Span) -> Type {
        match base_ty {
            Type::Record(fields) => fields
                .get(&Name::intern(name))
                .cloned()
                .unwrap_or(Type::Unknown),
            Type::Status => match name {
                "ok" | "success" => Type::Bool,
                "kind" => Type::Str,
                "code" | "exit_code" => Type::Int,
                "signal" => Type::Optional(Box::new(Type::Str)),
                "message" => Type::Str,
                _ => {
                    self.error(span, "unknown Status field", "check.unknown-field");
                    Type::Unknown
                }
            },
            Type::Any | Type::Unknown => Type::Unknown,
            _ => {
                self.error(
                    span,
                    "field access requires a record-like value",
                    "check.field-access",
                );
                Type::Unknown
            }
        }
    }

    pub(super) fn index_type_for_value(&mut self, base_ty: Type, span: Span) -> Type {
        match base_ty {
            Type::List(item) => *item,
            Type::Record(_) | Type::Unknown => Type::Unknown,
            _ => {
                self.error(span, "indexing requires List or Record", "check.index-type");
                Type::Unknown
            }
        }
    }

    pub(super) fn expect_command_value_conversion(
        &mut self,
        expected: &Type,
        actual: &Type,
        span: Span,
    ) {
        if actual.matches_expected(expected) {
            return;
        }
        if matches!(actual, Type::Str)
            && matches!(expected, Type::Path | Type::Int | Type::Bool | Type::Str)
        {
            return;
        }
        self.expect_type(expected, actual, span);
    }

    pub(super) fn check_external_splice_type(&mut self, ty: &Type, span: Span) {
        match ty {
            Type::List(item) if item.can_be_argv_item() => {}
            Type::List(_) => self.error(
                span,
                "splice item cannot convert to argv",
                "check.argv-conversion",
            ),
            Type::Unknown => {}
            _ => self.error(
                span,
                "`@` splices require List values",
                "check.splice-target",
            ),
        }
    }
}

/// Arena-native mirror of every function above, operating on the arena's
/// command representation (`ArenaCommand`/`ArenaCommandArg`/`ArenaRunForm`)
/// instead of the old recursive AST's. `field_type_for_value`/
/// `index_type_for_value`/`expect_command_value_conversion` are pure
/// `Type`-level and reused unchanged; `command_ty_auto_propagates`/
/// `module_sig_is_command_callable`/`standard_module_command_name`/
/// `is_bare_ident`/`valid_env_name` are pure `Type`/`str`-level and reused
/// unchanged too.
#[allow(dead_code)]
impl Checker {
    pub(super) fn check_command_stmt_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        id: CommandStmtId,
    ) {
        let stmt = arena.arena.command_stmt(id);
        let span = arena.arena.span(stmt.span);
        if self.in_pure {
            self.error(
                span,
                "commands are not allowed in pure functions",
                "check.pure-command",
            );
        }
        let ty = self.check_command_arena(arena, source, &stmt.command, span);
        if command_stmt_asserts_success_arena(arena, &stmt.command) {
            return;
        }
        if stmt.propagate || command_ty_auto_propagates(&ty) {
            self.check_propagation(&ty, span);
        } else {
            self.reject_ignored_result(&ty, span);
        }
    }

    pub(super) fn check_command_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        command: &ArenaCommand,
        span: Span,
    ) -> Type {
        match command {
            ArenaCommand::Proc { name, args } => {
                self.check_proc_command_arena(arena, source, &name.as_str(), *args, span)
            }
            ArenaCommand::Core {
                name,
                args,
                env,
                block,
            } => self.check_core_command_arena(arena, source, *name, *args, *env, *block, span),
            ArenaCommand::Run(run_id) => {
                if let Some(effs) = &self.current_effects
                    && !Self::effects_covers(effs, &Effect::Process)
                {
                    let run_span = arena.arena.span(arena.arena.run_form(*run_id).span);
                    self.error(
                        run_span,
                        "`run` requires the `process` effect",
                        "check.effect-violation",
                    );
                }
                self.check_run_arena(arena, source, *run_id)
            }
        }
    }

    pub(super) fn check_tail_bare_ident_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        name: Name,
        span: Span,
    ) -> Type {
        if self.procs.contains_key(&name) {
            if self.in_pure {
                self.error(
                    span,
                    "commands are not allowed in pure functions",
                    "check.pure-command",
                );
            }
            return self.check_proc_command_arena(
                arena,
                source,
                &name.as_str(),
                ArenaRange::default(),
                span,
            );
        }
        if let Some(binding) = self.lookup(name) {
            return binding.ty.clone();
        }
        self.check_proc_command_arena(arena, source, &name.as_str(), ArenaRange::default(), span)
    }

    pub(super) fn check_proc_command_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        name: &str,
        args: ArenaRange,
        span: Span,
    ) -> Type {
        if let Some((module, api)) = standard_module_command_name(name) {
            return self.check_module_command_arena(arena, source, module, api, args, span);
        }
        if self
            .options
            .interactive_commands
            .is_some_and(|is_command| is_command(name))
        {
            for arg in arena.arena.command_args(args) {
                self.check_command_arg_arena(arena, source, arg, None);
            }
            self.last_status_available = true;
            return Type::Int;
        }
        let interned = Name::intern(name);
        if self.pures.contains_key(&interned) {
            self.error(
                span,
                "pure functions cannot be called with command syntax",
                "check.command-pure",
            );
            return Type::Unknown;
        }
        if self.procs.contains_key(&interned) {
            self.error(
                span,
                "procs must be called with expression-call syntax",
                "check.proc-command-syntax",
            );
            return Type::Unknown;
        }
        self.error(
            span,
            "unresolved proc command",
            "check.unresolved-proc-command",
        );
        Type::Unknown
    }

    pub(super) fn check_module_command_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        module: &str,
        name: &str,
        args: ArenaRange,
        span: Span,
    ) -> Type {
        let Some(module_sig) = api_spec().module(module) else {
            self.error(span, "unknown module", "check.unknown-module");
            return Type::Unknown;
        };
        let Some(overloads) = module_sig.function_overloads(name) else {
            self.error(span, "unknown module API", "check.unknown-module-api");
            return Type::Unknown;
        };
        let command_overloads = overloads
            .iter()
            .filter(|&x| module_sig_is_command_callable(x))
            .cloned()
            .collect::<Vec<_>>();
        if command_overloads.is_empty() {
            self.error(
                span,
                "module command syntax is only for effectful Result[Unit] APIs",
                "check.module-command-value",
            );
            return Type::Unknown;
        }
        if self.in_pure {
            self.error(
                span,
                "effectful module API is not allowed in pure functions",
                "check.pure-effect",
            );
        }
        let command_args = arena.arena.command_args(args);
        let sig = choose_module_command_sig_arena(arena, source, command_args, &command_overloads)
            .unwrap_or(&command_overloads[0]);
        self.check_module_command_args_arena(arena, source, command_args, sig, span);
        sig.return_ty.clone()
    }

    pub(super) fn check_core_command_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        name: CoreCommand,
        args: ArenaRange,
        env: ArenaRange,
        block: Option<BlockId>,
        span: Span,
    ) -> Type {
        match name {
            CoreCommand::Print | CoreCommand::Eprint => {
                for arg in arena.arena.command_args(args) {
                    if let ArenaCommandArgKind::Word(parts) = &arg.kind {
                        let word_list: Vec<ArenaWordPart> =
                            arena.arena.word_parts(*parts).collect();
                        if word_list
                            .iter()
                            .any(|p| !matches!(p, ArenaWordPart::Bare(_)))
                        {
                            self.check_command_arg_arena_print_tail(arena, source, arg);
                            continue;
                        }
                        let word_text = word_parts_text_arena(arena, source, &word_list);
                        let arg_span = arena.arena.span(arg.span);
                        if word_text.is_empty() {
                            self.check_command_arg_arena_print_tail(arena, source, arg);
                            continue;
                        }
                        let name_resolves = is_bare_ident(&word_text)
                            && self.lookup(Name::intern(&word_text)).is_some();
                        let is_hyphenated = word_text.contains('-')
                            && word_text.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                            && word_text
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
                        let mut skip = !is_bare_ident(&word_text)
                            && !word_text.contains('.')
                            && !word_text.contains('[')
                            && !is_hyphenated;
                        if matches!(word_text.as_str(), "true" | "false" | "null") {
                            skip = true;
                        }
                        if !skip {
                            if name_resolves {
                                self.diagnostics.push(
                                    Diagnostic::error(
                                        "bare identifiers in print are ambiguous; use `$ident` to dereference or `\"text\"` for a literal",
                                    )
                                    .with_code("check.bare-print-ident")
                                    .with_label(Label::primary(
                                        arg_span,
                                        "bare identifiers in print are ambiguous; use `$ident` to dereference or `\"text\"` for a literal",
                                    ))
                                    .with_fix_hint(FixHint::replacement(
                                        arg_span,
                                        "use `$` shorthand",
                                        format!("${word_text}"),
                                    )),
                                );
                            } else if is_hyphenated {
                                self.diagnostics.push(
                                    Diagnostic::error(
                                        "bare words in print should be quoted string literals",
                                    )
                                    .with_code("check.bare-print-ident")
                                    .with_label(Label::primary(
                                        arg_span,
                                        "hyphenated bare words in print are ambiguous; use `\"text\"` for a literal",
                                    ))
                                    .with_fix_hint(FixHint::replacement(
                                        arg_span,
                                        "quote as string literal",
                                        format!("\"{word_text}\""),
                                    )),
                                );
                            } else if word_text.contains('.') || word_text.contains('[') {
                                let fix = if word_text.contains('[') {
                                    format!("${{{word_text}}}")
                                } else {
                                    format!("${word_text}")
                                };
                                self.diagnostics.push(
                                    Diagnostic::error(
                                        "field access and indexing in print require `$`; use `$ident.field` or `${expr}`",
                                    )
                                    .with_code("check.bare-print-ident")
                                    .with_label(Label::primary(
                                        arg_span,
                                        "field access and indexing in print require `$`; use `$ident.field` or `${expr}`",
                                    ))
                                    .with_fix_hint(FixHint::replacement(
                                        arg_span,
                                        "use `$` shorthand",
                                        fix,
                                    )),
                                );
                            }
                        }
                        self.check_command_arg_arena_print_tail(arena, source, arg);
                        continue;
                    }
                    self.check_command_arg_arena_print_tail(arena, source, arg);
                }
                Type::Unit
            }
            CoreCommand::Cd => {
                if args.len() != 1 {
                    self.error(
                        span,
                        "`cd` expects one path argument",
                        "check.core-cd-arity",
                    );
                }
                if let Some(arg) = arena.arena.command_args(args).first() {
                    self.check_command_arg_arena(arena, source, arg, Some(&Type::Path));
                }
                if let Some(block) = block {
                    self.check_block_arena(arena, source, block);
                }
                Type::Result(Box::new(Type::Unit), Box::new(Type::Error))
            }
            CoreCommand::Env => {
                if !args.is_empty() {
                    self.error(span, "`env` accepts assignments", "check.core-env-arity");
                }
                for assignment in arena.arena.env_assignments(env) {
                    self.check_env_assignment_arena(arena, source, assignment);
                }
                if let Some(block) = block {
                    self.check_block_arena(arena, source, block);
                }
                Type::Result(Box::new(Type::Unit), Box::new(Type::Error))
            }
        }
    }

    /// Runs the plain "check the arg, flag non-displayable types" tail
    /// shared by every branch of the `Print`/`Eprint` arm above.
    fn check_command_arg_arena_print_tail(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        arg: &ArenaCommandArg,
    ) {
        let ty = self.check_command_arg_arena(arena, source, arg, None);
        if !ty.can_display() && !matches!(ty, Type::Unknown) {
            let arg_span = arena.arena.span(arg.span);
            self.error(
                arg_span,
                "value cannot be displayed by print",
                "check.display-conversion",
            );
        }
    }

    pub(super) fn check_run_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        run_id: RunFormId,
    ) -> Type {
        let run = arena.arena.run_form(run_id);
        let run_span = arena.arena.span(run.span);
        let segments = arena.arena.run_segments(run.segments);
        if segments.is_empty() {
            return Type::Unknown;
        }
        for segment in segments {
            self.check_run_segment_arena(arena, source, segment);
        }
        let is_pipeline = segments.len() > 1;
        if is_pipeline
            && segments
                .iter()
                .any(|segment| !matches!(segment.kind, RunKind::Plain | RunKind::Status))
        {
            self.error(
                run_span,
                "byte pipelines cannot use capture run forms",
                "check.pipeline-capture",
            );
        }
        if is_pipeline
            && segments
                .iter()
                .skip(1)
                .any(|segment| segment.cpu_max.is_some())
        {
            self.error(
                run_span,
                "`--cpumax` is only valid on the first byte pipeline segment",
                "check.pipeline-cpumax",
            );
        }
        self.last_status_available = true;

        match segments[0].kind {
            RunKind::Plain | RunKind::Status => {
                if run.propagate {
                    self.check_propagation(
                        &Type::Result(Box::new(Type::Status), Box::new(Type::ProcessError)),
                        run_span,
                    )
                } else {
                    Type::Status
                }
            }
            RunKind::CaptureText => {
                let result = Type::Result(Box::new(Type::Str), Box::new(Type::ProcessError));
                if run.propagate {
                    self.check_propagation(&result, run_span)
                } else {
                    result
                }
            }
            RunKind::CaptureBytes => {
                let result = Type::Result(Box::new(Type::Bytes), Box::new(Type::ProcessError));
                if run.propagate {
                    self.check_propagation(&result, run_span)
                } else {
                    result
                }
            }
            RunKind::CaptureTextRecord | RunKind::CaptureBytesRecord => {
                let output_ty = if matches!(segments[0].kind, RunKind::CaptureTextRecord) {
                    Type::Str
                } else {
                    Type::Bytes
                };
                let result = Type::Result(
                    Box::new(Type::Record(btree_map(vec![
                        ("status", Type::Status),
                        ("stdout", output_ty.clone()),
                        ("stderr", output_ty),
                    ]))),
                    Box::new(Type::ProcessError),
                );
                if run.propagate {
                    self.check_propagation(&result, run_span)
                } else {
                    result
                }
            }
            RunKind::StreamText => {
                let result = Type::Result(
                    Box::new(Type::Stream(Box::new(Type::Str))),
                    Box::new(Type::ProcessError),
                );
                if run.propagate {
                    self.check_propagation(&result, run_span)
                } else {
                    result
                }
            }
            RunKind::StreamBytes => {
                let result = Type::Result(
                    Box::new(Type::Stream(Box::new(Type::Bytes))),
                    Box::new(Type::ProcessError),
                );
                if run.propagate {
                    self.check_propagation(&result, run_span)
                } else {
                    result
                }
            }
        }
    }

    pub(super) fn check_run_segment_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        segment: &ArenaRunSegment,
    ) {
        if let Some(timeout) = segment.timeout {
            let ty = self.check_expr_arena(arena, source, timeout, Some(&Type::Duration));
            let timeout_span = arena.arena.expr(timeout).span;
            self.expect_type(&Type::Duration, &ty, timeout_span);
        }
        if let Some(cpu_max) = segment.cpu_max {
            let ty = self.check_expr_arena(arena, source, cpu_max, Some(&Type::Int));
            let cpu_max_span = arena.arena.expr(cpu_max).span;
            self.expect_type(&Type::Int, &ty, cpu_max_span);
            self.check_static_positive_int_arena(
                arena,
                cpu_max,
                "`--cpumax` must be positive",
                "check.cpumax",
            );
        }
        if matches!(
            segment.target.kind,
            ArenaCommandArgKind::SpliceName(_) | ArenaCommandArgKind::SpliceExpr(_)
        ) {
            let target_span = arena.arena.span(segment.target.span);
            self.error(
                target_span,
                "run target must be one argv item",
                "check.run-target",
            );
        }
        self.check_external_arg_arena(arena, source, &segment.target);
        for assignment in arena.arena.env_assignments(segment.env) {
            self.check_env_assignment_arena(arena, source, assignment);
        }
        for arg in arena.arena.command_args(segment.args) {
            self.check_external_arg_arena(arena, source, arg);
        }
        for redirection in arena.arena.redirections(segment.redirections) {
            self.check_redirection_arena(arena, source, redirection);
        }
    }

    fn check_static_positive_int_arena(
        &mut self,
        arena: &ArenaProgram,
        expr_id: ExprId,
        message: &str,
        code: &str,
    ) {
        let expr = arena.arena.expr(expr_id);
        match &expr.kind {
            ArenaExprKind::Int(value_id)
                if arena
                    .arena
                    .int_literal(*value_id)
                    .value()
                    .is_some_and(|value| value <= 0) =>
            {
                self.error(expr.span, message, code);
            }
            ArenaExprKind::Unary {
                op: UnaryOp::Neg,
                expr: inner,
            } if matches!(arena.arena.expr(*inner).kind, ArenaExprKind::Int(_)) => {
                self.error(expr.span, message, code);
            }
            _ => {}
        }
    }

    pub(super) fn check_env_assignment_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        assignment: &ArenaEnvAssignment,
    ) {
        let assignment_span = arena.arena.span(assignment.span);
        if !valid_env_name(&assignment.name.as_str()) {
            self.error(
                assignment_span,
                "environment names must be identifiers",
                "check.env-name",
            );
        }
        match &assignment.value {
            ArenaEnvAssignmentValue::CommandArg(arg) => {
                if matches!(
                    arg.kind,
                    ArenaCommandArgKind::SpliceName(_) | ArenaCommandArgKind::SpliceExpr(_)
                ) {
                    let arg_span = arena.arena.span(arg.span);
                    self.error(
                        arg_span,
                        "environment values must be one value",
                        "check.env-value",
                    );
                    return;
                }
                self.check_external_arg_arena(arena, source, arg);
            }
            ArenaEnvAssignmentValue::Expr(expr_id) => {
                let ty = self.check_expr_arena(arena, source, *expr_id, None);
                if !ty.can_be_argv_item() && !matches!(ty, Type::Unknown) {
                    let expr_span = arena.arena.expr(*expr_id).span;
                    self.error(
                        expr_span,
                        "environment value cannot convert to one value",
                        "check.env-value",
                    );
                }
            }
        }
    }

    pub(super) fn check_redirection_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        redirection: &ArenaRedirection,
    ) {
        match &redirection.target {
            ArenaRedirectionTarget::Path(arg) => {
                let ty = self.check_command_arg_arena(arena, source, arg, None);
                let arg_span = arena.arena.span(arg.span);
                self.expect_command_value_conversion(&Type::Path, &ty, arg_span);
            }
            ArenaRedirectionTarget::Fd(arg) => {
                let ty = self.check_command_arg_arena(arena, source, arg, None);
                let arg_span = arena.arena.span(arg.span);
                self.expect_command_value_conversion(&Type::Int, &ty, arg_span);
            }
        }
    }

    pub(super) fn check_command_arg_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        arg: &ArenaCommandArg,
        expected: Option<&Type>,
    ) -> Type {
        let arg_span = arena.arena.span(arg.span);
        match &arg.kind {
            ArenaCommandArgKind::Word(parts) => {
                let word_list: Vec<ArenaWordPart> = arena.arena.word_parts(*parts).collect();
                if let Some(expected) = expected
                    && let Some(text) = bare_command_word_parts_arena(arena, source, &word_list)
                    && let Some(ty) = self.check_command_word_reference(&text, arg_span)
                {
                    self.expect_command_value_conversion(expected, &ty, arg_span);
                    return expected.clone();
                }
                if let [ArenaWordPart::Interpolation(expr_id) | ArenaWordPart::Shorthand(expr_id)] =
                    word_list.as_slice()
                {
                    let ty = self.check_expr_arena(arena, source, *expr_id, expected);
                    let expr_span = arena.arena.expr(*expr_id).span;
                    if let Some(expected) = expected {
                        self.expect_command_value_conversion(expected, &ty, expr_span);
                        return expected.clone();
                    }
                    return ty;
                }
                for part in &word_list {
                    if let ArenaWordPart::Interpolation(expr_id)
                    | ArenaWordPart::Shorthand(expr_id) = part
                    {
                        let ty = self.check_expr_arena(arena, source, *expr_id, None);
                        if !ty.can_display() && !matches!(ty, Type::Unknown) {
                            let expr_span = arena.arena.expr(*expr_id).span;
                            self.error(
                                expr_span,
                                "interpolation cannot convert to one command word",
                                "check.argv-conversion",
                            );
                        }
                    }
                }
                if let Some(expected) = expected {
                    if !expected.can_word_convert_to() {
                        self.error(
                            arg_span,
                            "command word cannot convert to declared parameter type",
                            "check.command-word-conversion",
                        );
                    }
                    expected.clone()
                } else {
                    Type::Str
                }
            }
            ArenaCommandArgKind::Typed(expr_id) => {
                let ty = self.check_expr_arena(arena, source, *expr_id, expected);
                if let Some(expected) = expected {
                    let expr_span = arena.arena.expr(*expr_id).span;
                    self.expect_type(expected, &ty, expr_span);
                }
                ty
            }
            ArenaCommandArgKind::SpliceName(name) => self
                .lookup(*name)
                .map(|binding| binding.ty.clone())
                .unwrap_or(Type::Unknown),
            ArenaCommandArgKind::SpliceExpr(expr_id) => {
                self.check_expr_arena(arena, source, *expr_id, None)
            }
        }
    }

    pub(super) fn check_external_arg_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        arg: &ArenaCommandArg,
    ) {
        match &arg.kind {
            ArenaCommandArgKind::Word(parts) => {
                let word_list: Vec<ArenaWordPart> = arena.arena.word_parts(*parts).collect();
                let standalone_interpolation = matches!(
                    word_list.as_slice(),
                    [ArenaWordPart::Interpolation(_) | ArenaWordPart::Shorthand(_)]
                );
                for part in &word_list {
                    if let ArenaWordPart::Interpolation(expr_id)
                    | ArenaWordPart::Shorthand(expr_id) = part
                    {
                        let ty = self.check_expr_arena(arena, source, *expr_id, None);
                        let valid = if standalone_interpolation {
                            ty.can_be_argv_item()
                                || matches!(&ty, Type::List(item) if item.can_be_argv_item())
                        } else {
                            ty.can_display()
                        };
                        if !valid && !matches!(ty, Type::Unknown) {
                            let expr_span = arena.arena.expr(*expr_id).span;
                            self.error(
                                expr_span,
                                "invalid argv interpolation conversion",
                                "check.argv-conversion",
                            );
                        }
                    }
                }
            }
            ArenaCommandArgKind::Typed(expr_id) => {
                let ty = self.check_expr_arena(arena, source, *expr_id, None);
                if !ty.can_be_argv_item() && !matches!(ty, Type::Unknown) {
                    let expr_span = arena.arena.expr(*expr_id).span;
                    self.error(
                        expr_span,
                        "value cannot convert to argv item",
                        "check.argv-conversion",
                    );
                }
            }
            ArenaCommandArgKind::SpliceName(name) => {
                let ty = self
                    .lookup(*name)
                    .map(|binding| binding.ty.clone())
                    .unwrap_or(Type::Unknown);
                let arg_span = arena.arena.span(arg.span);
                self.check_external_splice_type(&ty, arg_span);
            }
            ArenaCommandArgKind::SpliceExpr(expr_id) => {
                let ty = self.check_expr_arena(arena, source, *expr_id, None);
                let arg_span = arena.arena.span(arg.span);
                self.check_external_splice_type(&ty, arg_span);
            }
        }
    }
}

#[allow(dead_code)]
pub(super) fn command_stmt_asserts_success_arena(
    arena: &ArenaProgram,
    command: &ArenaCommand,
) -> bool {
    let ArenaCommand::Run(run_id) = command else {
        return false;
    };
    let run = arena.arena.run_form(*run_id);
    run.propagate || run_statement_asserts_success_by_default_arena(arena, *run_id)
}

#[allow(dead_code)]
pub(super) fn run_statement_asserts_success_by_default_arena(
    arena: &ArenaProgram,
    run_id: RunFormId,
) -> bool {
    let run = arena.arena.run_form(run_id);
    let segments = arena.arena.run_segments(run.segments);
    matches!(segments[0].kind, RunKind::Plain)
}

#[allow(dead_code)]
pub(super) fn choose_module_command_sig_arena<'a>(
    arena: &ArenaProgram,
    source: &str,
    args: &[ArenaCommandArg],
    overloads: &'a [ModuleFnSig],
) -> Option<&'a ModuleFnSig> {
    overloads
        .iter()
        .find(|sig| module_command_shape_matches_arena(arena, source, args, sig))
}

#[allow(dead_code)]
pub(super) fn module_command_shape_matches_arena(
    arena: &ArenaProgram,
    source: &str,
    args: &[ArenaCommandArg],
    sig: &ModuleFnSig,
) -> bool {
    let mut positional_index = 0usize;
    let mut flags = FxHashSet::default();
    for arg in args {
        if let Some(flag) = command_bool_flag_name_arena(arena, source, arg) {
            let Some(param) = sig.params.iter().find(|param| param.name == flag) else {
                return false;
            };
            if !(param.defaulted && param.ty == Type::Bool) || !flags.insert(flag.to_string()) {
                return false;
            }
            continue;
        }
        let Some(param) = sig
            .params
            .iter()
            .filter(|param| !flags.contains(param.name))
            .nth(positional_index)
        else {
            return false;
        };
        if !command_arg_can_match_module_param_arena(arena, arg, &param.ty) {
            return false;
        }
        positional_index += 1;
    }
    let required = sig.params.iter().filter(|param| !param.defaulted).count();
    positional_index >= required && positional_index <= sig.params.len().saturating_sub(flags.len())
}

#[allow(dead_code)]
pub(super) fn command_arg_can_match_module_param_arena(
    arena: &ArenaProgram,
    arg: &ArenaCommandArg,
    expected: &Type,
) -> bool {
    match &arg.kind {
        ArenaCommandArgKind::Word(parts) => {
            let word_list: Vec<ArenaWordPart> = arena.arena.word_parts(*parts).collect();
            if matches!(
                word_list.as_slice(),
                [ArenaWordPart::Interpolation(_) | ArenaWordPart::Shorthand(_)]
            ) {
                return true;
            }
            expected.can_word_convert_to() || matches!(expected, Type::Unknown)
        }
        ArenaCommandArgKind::Typed(_) => true,
        ArenaCommandArgKind::SpliceName(_) | ArenaCommandArgKind::SpliceExpr(_) => false,
    }
}

#[allow(dead_code)]
pub(super) fn command_arg_can_be_path_like_arena(arg: &ArenaCommandArg, ty: &Type) -> bool {
    matches!(ty, Type::Path | Type::Str | Type::Unknown)
        || matches!(arg.kind, ArenaCommandArgKind::Word(_))
}

#[allow(dead_code)]
pub(super) fn command_bool_flag_name_arena(
    arena: &ArenaProgram,
    source: &str,
    arg: &ArenaCommandArg,
) -> Option<String> {
    let text = literal_command_word_text_arena(arena, source, arg)?;
    let flag = text.strip_prefix("--")?;
    if flag.is_empty() || flag.contains('=') {
        return None;
    }
    Some(flag.replace('-', "_"))
}

#[allow(dead_code)]
pub(super) fn literal_command_word_text_arena(
    arena: &ArenaProgram,
    source: &str,
    arg: &ArenaCommandArg,
) -> Option<String> {
    let ArenaCommandArgKind::Word(parts) = &arg.kind else {
        return None;
    };
    let word_list: Vec<ArenaWordPart> = arena.arena.word_parts(*parts).collect();
    literal_command_word_parts_arena(arena, source, &word_list)
}

#[allow(dead_code)]
pub(super) fn literal_command_word_parts_arena(
    arena: &ArenaProgram,
    source: &str,
    parts: &[ArenaWordPart],
) -> Option<String> {
    let mut text = String::new();
    for part in parts {
        match part {
            ArenaWordPart::Bare(value) | ArenaWordPart::Quoted(value) => {
                text.push_str(arena.arena.text_value(value, source)?);
            }
            ArenaWordPart::Interpolation(_) | ArenaWordPart::Shorthand(_) => return None,
        }
    }
    Some(text)
}

#[allow(dead_code)]
pub(super) fn bare_command_word_parts_arena(
    arena: &ArenaProgram,
    source: &str,
    parts: &[ArenaWordPart],
) -> Option<String> {
    let [ArenaWordPart::Bare(value)] = parts else {
        return None;
    };
    Some(arena.arena.text_value(value, source)?.to_string())
}

#[allow(dead_code)]
fn word_parts_text_arena(arena: &ArenaProgram, source: &str, parts: &[ArenaWordPart]) -> String {
    let mut text = String::new();
    for part in parts {
        if let ArenaWordPart::Bare(value) = part
            && let Some(resolved) = arena.arena.text_value(value, source)
        {
            text.push_str(resolved);
        }
    }
    text
}
