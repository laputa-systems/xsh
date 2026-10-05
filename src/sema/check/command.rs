#![allow(clippy::single_call_fn)]

use super::{
    BTreeMap, Checker, CoreCommand, Diagnostic, Effect, FixHint, FxHashSet, Label, ModuleFnSig,
    Name, RunKind, Span, Type, UnaryOp, api_spec,
};
use crate::diagnostic::DiagnosticCode;
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

pub(super) fn run_capture_result_type_arena(arena: &ArenaProgram, run: RunFormId) -> Option<Type> {
    let run = arena.arena.run_form(run);
    let segment = arena.arena.run_segments(run.segments).first()?;
    let ok = match segment.kind {
        RunKind::CaptureText => Type::Str,
        RunKind::CaptureBytes => Type::Bytes,
        RunKind::CaptureTextRecord | RunKind::CaptureBytesRecord => {
            let output = if segment.kind == RunKind::CaptureTextRecord {
                Type::Str
            } else {
                Type::Bytes
            };
            Type::Record(btree_map(vec![
                ("status", Type::Status),
                ("stdout", output.clone()),
                ("stderr", output),
            ]))
        }
        RunKind::StreamText => Type::Stream(Box::new(Type::Str)),
        RunKind::StreamBytes => Type::Stream(Box::new(Type::Bytes)),
        RunKind::Plain | RunKind::Status => return None,
    };
    Some(Type::Result(Box::new(ok), Box::new(Type::ProcessError)))
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
                    self.error(
                        span,
                        "unknown Status field",
                        DiagnosticCode::CheckUnknownField,
                    );
                    Type::Unknown
                }
            },
            Type::Any => Type::Any,
            Type::Unknown => Type::Unknown,
            _ => {
                self.error(
                    span,
                    "field access requires a record-like value",
                    DiagnosticCode::CheckFieldAccess,
                );
                Type::Unknown
            }
        }
    }

    pub(super) fn index_type_for_value(&mut self, base_ty: Type, span: Span) -> Type {
        match base_ty {
            Type::List(item) => *item,
            Type::Any => Type::Any,
            Type::Record(_) | Type::Unknown => Type::Unknown,
            _ => {
                self.error(
                    span,
                    "indexing requires List or Record",
                    DiagnosticCode::CheckIndexType,
                );
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

    /// Command words, argv items, and printed values convert only known
    /// scalars; an `Any` must be validated first. Reports and returns `true`
    /// for an `Any`.
    pub(super) fn reject_dynamic_word(&mut self, ty: &Type, span: Span) -> bool {
        let dynamic = *ty == Type::Any || matches!(ty, Type::List(item) if **item == Type::Any);
        if dynamic {
            self.reject_dynamic_use("a command word or printed value", None, span);
        }
        dynamic
    }

    pub(super) fn check_external_splice_type(&mut self, ty: &Type, span: Span) {
        if self.reject_dynamic_word(ty, span) {
            return;
        }
        match ty {
            Type::List(item) if item.can_be_argv_item() => {}
            Type::List(_) => self.error(
                span,
                "splice item cannot convert to argv",
                DiagnosticCode::CheckArgvConversion,
            ),
            Type::Unknown => {}
            _ => self.error(
                span,
                "`@` splices require List values",
                DiagnosticCode::CheckSpliceTarget,
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
        if let ArenaCommand::Proc { name, args } = stmt.command
            && args.is_empty()
            && !self.procs.contains_key(&name)
            && !self
                .options
                .interactive_commands
                .is_some_and(|is_command| is_command(name.as_str().as_str()))
            && self
                .lookup(name)
                .is_some_and(|binding| binding.ty == Type::Bool)
        {
            self.reject_bool_statement(source, &Type::Bool, span);
            return;
        }
        if self.in_pure {
            self.error(
                span,
                "commands are not allowed in pure functions",
                DiagnosticCode::CheckPureCommand,
            );
        }
        let ty = self.check_command_arena(arena, source, &stmt.command, span);
        if command_stmt_asserts_success_arena(arena, &stmt.command) {
            self.record_statement_error(
                &Type::Result(Box::new(Type::Unit), Box::new(Type::ProcessError)),
                span,
            );
            return;
        }
        let value_ty = if stmt.propagate || command_ty_auto_propagates(&ty) {
            self.check_propagation(&ty, span)
        } else {
            ty
        };
        // `run.status` discards its status in statement position. Only a run
        // form is also valid as a `let` initializer, so only it gets the
        // mechanical discard.
        if value_ty != Type::Status {
            self.reject_discarded_value(
                &value_ty,
                span,
                span,
                matches!(stmt.command, ArenaCommand::Run(_)),
                None,
            );
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
                self.record_required_effect(Effect::Process);
                if let Some(effs) = &self.current_effects
                    && !Self::effects_covers(effs, &Effect::Process)
                {
                    let run_span = arena.arena.span(arena.arena.run_form(*run_id).span);
                    self.error(
                        run_span,
                        "`run` requires the `process` effect",
                        DiagnosticCode::CheckEffectViolation,
                    );
                }
                self.check_effect_not_excluded(
                    &Effect::Process,
                    arena.arena.span(arena.arena.run_form(*run_id).span),
                    "`run`",
                );
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
        if name == "_" {
            self.error(
                span,
                "`_` is only a whole argument placeholder in an immediate value pipeline call",
                DiagnosticCode::CheckPipelineHole,
            );
            return Type::Invalid;
        }

        if let Some(expr) = self.prepared_constants.tail_bindings.get(&span)
            && let Some(ty) = self.prepared_constants.types.get(expr)
        {
            let ty = ty.clone();
            self.record_expr_type(span, ty.clone());
            return ty;
        }
        if self.procs.contains_key(&name) {
            if self.in_pure {
                self.error(
                    span,
                    "commands are not allowed in pure functions",
                    DiagnosticCode::CheckPureCommand,
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
        if self
            .tag_variants
            .get(&name)
            .is_some_and(|info| info.field_count == 0)
        {
            return self.lookup_expr_ident(name, span);
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
                DiagnosticCode::CheckCommandPure,
            );
            return Type::Unknown;
        }
        if self.procs.contains_key(&interned) {
            self.error(
                span,
                "procs must be called with expression-call syntax",
                DiagnosticCode::CheckProcCommandSyntax,
            );
            return Type::Unknown;
        }
        self.report_unresolved_proc_command(name, span);
        Type::Unknown
    }

    /// A command statement names a core command, a standard API, or `run`.
    /// Shell builtins are the usual unresolved names, so they get the XSH
    /// spelling; anything else is most likely an external program. The
    /// `print` fix is not auto-applied, since `echo` flags differ.
    fn report_unresolved_proc_command(&mut self, name: &str, span: Span) {
        let name_span = Span::new(
            span.source_id,
            span.start(),
            (span.start() + name.len()).min(span.end()),
        );
        let mut diagnostic = Diagnostic::error(format!("unresolved proc command `{name}`"))
            .with_code(DiagnosticCode::CheckUnresolvedProcCommand)
            .with_label(Label::primary(span, "unresolved proc command"));
        diagnostic = match name {
            "echo" | "printf" => diagnostic
                .with_note(format!("print text with `print`, or run the external program with `run {name} ...`"))
                .with_fix_hint(FixHint::replacement(name_span, "use `print`", "print").dangerous()),
            "source" | "." => diagnostic
                .with_note("XSH does not source shell scripts; import an XSH module with `use`, or run a shell script with `run sh FILE`"),
            "set" | "unset" | "declare" | "typeset" | "readonly" | "local" => diagnostic
                .with_note("declare variables with `let` or `var`; set a child's environment with `env NAME=value { ... }`"),
            "env" => diagnostic
                .with_note("an `env` scope needs a block: `env NAME=value { ... }`; for one command write `run NAME=value COMMAND`"),
            _ => diagnostic.with_note(format!("XSH never looks up commands on PATH implicitly; run an external program with `run {name} ...`")),
        };
        self.diagnostics.push(diagnostic);
    }

    /// Conversion errors name the type that cannot convert. A Unit value is
    /// almost always a statement-like call interpolated by mistake.
    pub(super) fn report_conversion(
        &mut self,
        span: Span,
        ty: &Type,
        failure: &str,
        code: DiagnosticCode,
    ) {
        let mut diagnostic = Diagnostic::error(format!("value of type `{ty}` {failure}"))
            .with_code(code)
            .with_label(Label::primary(span, format!("this value is `{ty}`")));
        if *ty == Type::Unit {
            diagnostic = diagnostic
                .with_note("this expression produces no value; run it as its own statement");
        }
        self.diagnostics.push(diagnostic);
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
        if let Some(required) = api_spec().module_required_effect(module, name) {
            self.require_effect(required, span, &format!("`{module}.{name}`"));
        }
        let Some(module_sig) = api_spec().module(module) else {
            self.error(span, "unknown module", DiagnosticCode::CheckUnknownModule);
            return Type::Unknown;
        };
        let Some(overloads) = module_sig.function_overloads(name) else {
            self.report_unknown_module_api(module, name, span);
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
                DiagnosticCode::CheckModuleCommandValue,
            );
            return Type::Unknown;
        }
        if self.in_pure {
            self.error(
                span,
                "effectful module API is not allowed in pure functions",
                DiagnosticCode::CheckPureEffect,
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
                                    .with_code(DiagnosticCode::CheckBarePrintIdent)
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
                                    .with_code(DiagnosticCode::CheckBarePrintIdent)
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
                                    .with_code(DiagnosticCode::CheckBarePrintIdent)
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
                self.require_effect(Effect::Env, span, "`cd`");
                if args.len() != 1 {
                    self.error(
                        span,
                        "`cd` expects one path argument",
                        DiagnosticCode::CheckCoreCdArity,
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
                self.require_effect(Effect::Env, span, "`env`");
                if !args.is_empty() {
                    self.error(
                        span,
                        "`env` accepts assignments",
                        DiagnosticCode::CheckCoreEnvArity,
                    );
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
        if self.reject_dynamic_word(&ty, arena.arena.span(arg.span)) {
        } else if !ty.can_display() && !matches!(ty, Type::Unknown | Type::Invalid) {
            let arg_span = arena.arena.span(arg.span);
            self.report_conversion(
                arg_span,
                &ty,
                "cannot be displayed by print",
                DiagnosticCode::CheckDisplayConversion,
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
        if segments.iter().skip(1).any(|segment| {
            arena
                .arena
                .redirections(segment.redirections)
                .iter()
                .any(|item| {
                    matches!(
                        item.kind,
                        crate::syntax::node::RedirectionKind::StdinRead
                            | crate::syntax::node::RedirectionKind::StdinDup
                    )
                })
        }) {
            self.error(
                run_span,
                "stdin redirection is only valid on the first byte pipeline segment",
                DiagnosticCode::CheckPipelineStdin,
            );
        }
        if is_pipeline {
            // The head segment chooses the form for the whole pipeline: a
            // capture head captures the last segment's stdout, so every later
            // segment is plain `run`.
            let head = segments[0].kind;
            let message = if matches!(head, RunKind::StreamText | RunKind::StreamBytes) {
                Some("byte pipelines cannot use `run.stream`")
            } else if matches!(head, RunKind::Plain | RunKind::Status) {
                segments
                    .iter()
                    .skip(1)
                    .any(|segment| !matches!(segment.kind, RunKind::Plain | RunKind::Status))
                    .then_some("only the first byte pipeline segment may choose a capture form")
            } else {
                segments
                    .iter()
                    .skip(1)
                    .any(|segment| segment.kind != RunKind::Plain)
                    .then_some("segments after a capturing pipeline head must be plain `run`")
            };
            if let Some(message) = message {
                self.error(run_span, message, DiagnosticCode::CheckPipelineCapture);
            }
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
                DiagnosticCode::CheckPipelineCpumax,
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
            RunKind::CaptureText
            | RunKind::CaptureBytes
            | RunKind::CaptureTextRecord
            | RunKind::CaptureBytesRecord
            | RunKind::StreamText
            | RunKind::StreamBytes => {
                let result =
                    run_capture_result_type_arena(arena, run_id).expect("capture run kind");
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
                DiagnosticCode::CheckCpumax,
            );
        }
        if let Some(accept) = segment.accept {
            let expected = Type::List(Box::new(Type::Int));
            let actual = self.check_expr_arena(arena, source, accept, Some(&expected));
            let span = arena.arena.expr(accept).span;
            self.expect_type(&expected, &actual, span);
            self.require_effect(
                Effect::Error,
                span,
                "explicit process completion validation",
            );
            self.record_statement_error(
                &Type::Result(Box::new(Type::Unit), Box::new(Type::ProcessError)),
                span,
            );
            self.check_static_accepted_exit_codes(arena, accept);
        }
        // A spliced target is the whole command vector, and its first
        // element is the program. An empty list literal can never name one;
        // a computed list is checked when the command runs.
        if let ArenaCommandArgKind::SpliceExpr(list) = segment.target.kind
            && let ArenaExprKind::List(items) = arena.arena.expr(list).kind
            && items.is_empty()
        {
            self.error(
                arena.arena.span(segment.target.span),
                "spliced command is empty: its first element names the program to run",
                DiagnosticCode::CheckRunTarget,
            );
        } else {
            self.check_external_arg_arena(arena, source, &segment.target);
        }
        for assignment in arena.arena.env_assignments(segment.env) {
            self.check_env_assignment_arena(arena, source, assignment);
        }
        for arg in arena.arena.command_args(segment.args) {
            self.check_external_arg_arena(arena, source, arg);
        }
        let redirections = arena.arena.redirections(segment.redirections);
        let stdin_sources = redirections
            .iter()
            .filter(|item| {
                matches!(
                    item.kind,
                    crate::syntax::node::RedirectionKind::StdinRead
                        | crate::syntax::node::RedirectionKind::StdinDup
                )
            })
            .count();
        let mut bytes_input = false;
        for redirection in redirections {
            bytes_input |= self.check_redirection_arena(arena, source, redirection);
        }
        if bytes_input && stdin_sources > 1 {
            self.error(
                arena.arena.span(segment.span),
                "Bytes input cannot compete with another stdin source",
                DiagnosticCode::CheckStdinSource,
            );
        }
    }

    /// Validates bounded literal policies. Dynamic values use the earlier
    /// run-option conversion boundary before a child starts.
    pub(super) fn check_static_accepted_exit_codes(&mut self, arena: &ArenaProgram, expr: ExprId) {
        let Some(crate::sema::constants::LiteralConstant::List(items)) =
            crate::sema::constants::LiteralConstant::analyze(
                &arena.arena,
                expr,
                &rustc_hash::FxHashMap::default(),
            )
        else {
            return;
        };
        let codes = items
            .iter()
            .map(|item| match item {
                crate::sema::constants::LiteralConstant::Int(value) => Some(*value),
                _ => None,
            })
            .collect::<Option<Vec<_>>>();
        if let Some(codes) = codes
            && let Err(error) = crate::runtime::process::AcceptedExitCodes::new(&codes)
        {
            self.error(
                arena.arena.expr(expr).span,
                &error.message,
                DiagnosticCode::CheckAcceptPolicy,
            );
        }
    }

    fn check_static_positive_int_arena(
        &mut self,
        arena: &ArenaProgram,
        expr_id: ExprId,
        message: &str,
        code: DiagnosticCode,
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
                DiagnosticCode::CheckEnvName,
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
                        DiagnosticCode::CheckEnvValue,
                    );
                    return;
                }
                self.check_external_arg_arena(arena, source, arg);
            }
            ArenaEnvAssignmentValue::Expr(expr_id) => {
                let ty = self.check_expr_arena(arena, source, *expr_id, None);
                if self.reject_dynamic_word(&ty, arena.arena.expr(*expr_id).span) {
                } else if !ty.can_be_argv_item() && !matches!(ty, Type::Unknown) {
                    let expr_span = arena.arena.expr(*expr_id).span;
                    self.error(
                        expr_span,
                        "environment value cannot convert to one value",
                        DiagnosticCode::CheckEnvValue,
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
    ) -> bool {
        match &redirection.target {
            ArenaRedirectionTarget::Path(arg) => {
                let ty = self.check_command_arg_arena(arena, source, arg, None);
                let arg_span = arena.arena.span(arg.span);
                if redirection.kind != crate::syntax::node::RedirectionKind::StdinRead
                    || ty != Type::Bytes
                {
                    self.expect_command_value_conversion(&Type::Path, &ty, arg_span);
                }
                ty == Type::Bytes
                    && redirection.kind == crate::syntax::node::RedirectionKind::StdinRead
            }
            ArenaRedirectionTarget::Fd(arg) => {
                let ty = self.check_command_arg_arena(arena, source, arg, None);
                let arg_span = arena.arena.span(arg.span);
                self.expect_command_value_conversion(&Type::Int, &ty, arg_span);
                false
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
                    // A `$name.field` word takes no `.require(T)?` suffix.
                    if matches!(word_list.as_slice(), [ArenaWordPart::Shorthand(_)]) {
                        self.dynamic_require_receivers.remove(&expr_span);
                    }
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
                        if self.reject_dynamic_word(&ty, arena.arena.expr(*expr_id).span) {
                        } else if !ty.can_display() && !matches!(ty, Type::Unknown | Type::Invalid)
                        {
                            let expr_span = arena.arena.expr(*expr_id).span;
                            self.report_conversion(
                                expr_span,
                                &ty,
                                "cannot convert to one command word",
                                DiagnosticCode::CheckArgvConversion,
                            );
                        }
                    }
                }
                if let Some(expected) = expected {
                    if !expected.can_word_convert_to() {
                        self.error(
                            arg_span,
                            "command word cannot convert to declared parameter type",
                            DiagnosticCode::CheckCommandWordConversion,
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
            // An unresolved splice name used to type as Unknown and fail
            // preparation; resolve it like any other name.
            ArenaCommandArgKind::SpliceName(name) => {
                self.lookup_expr_ident(*name, arena.arena.span(arg.span))
            }
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
                        if self.reject_dynamic_word(&ty, arena.arena.expr(*expr_id).span) {
                            continue;
                        }
                        let valid = if standalone_interpolation {
                            ty.can_be_argv_item()
                                || matches!(&ty, Type::List(item) if item.can_be_argv_item())
                        } else {
                            ty.can_display()
                        };
                        if !valid && !matches!(ty, Type::Unknown) {
                            let expr_span = arena.arena.expr(*expr_id).span;
                            self.report_conversion(
                                expr_span,
                                &ty,
                                "cannot be a command argument",
                                DiagnosticCode::CheckArgvConversion,
                            );
                        }
                    }
                }
            }
            ArenaCommandArgKind::Typed(expr_id) => {
                let ty = self.check_expr_arena(arena, source, *expr_id, None);
                if self.reject_dynamic_word(&ty, arena.arena.expr(*expr_id).span) {
                } else if !ty.can_be_argv_item() && !matches!(ty, Type::Unknown) {
                    let expr_span = arena.arena.expr(*expr_id).span;
                    self.report_conversion(
                        expr_span,
                        &ty,
                        "cannot be a command argument",
                        DiagnosticCode::CheckArgvConversion,
                    );
                }
            }
            ArenaCommandArgKind::SpliceName(name) => {
                let arg_span = arena.arena.span(arg.span);
                let ty = self.lookup_expr_ident(*name, arg_span);
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
