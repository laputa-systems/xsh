use super::{
    Arc, ArenaBuilderEntryKind, ArenaCommand, ArenaCommandArgKind, ArenaExprKind, ArenaExprOrRun,
    ArenaSpawnTarget, ArenaStmtKind, ArenaWordPart, AstArena, BuildExprId, BuildExprRow,
    BuildScratch, BuildStmtId, BuildStmtRow, CommandWordRefSegment, CompactLowerConstructProbe,
    CoreCommand, ExprId, LoweredFmtPart, LoweredProcessCommandBuilderEntry, LoweredRunArg,
    LoweredRunArgKind, LoweredRunCapture, LoweredRunEnv, LoweredRunPipelineSegment,
    LoweredRunRedirection, LoweredSpawnRun, LoweredType, Name, Rc, RefCell, RunKind, RuntimeOp,
    SlotScope, Span, StmtId, api_spec, build_expr, lower_literal_constant,
    parse_command_word_reference, print_flush_arg,
};

pub(super) fn lower_command_word_reference(
    text: &str,
    slots: &SlotScope,
    span: Span,
    scratch: &Rc<RefCell<BuildScratch>>,
    constants: &crate::sema::constants::PreparedConstants,
    wire_enums: &crate::sema::wire_enums::PreparedWireEnums,
    namespace: Option<Name>,
) -> Option<BuildExprId> {
    let (root, segments) = parse_command_word_reference(text)?;
    let mut value = if let Some(slot) = slots.resolve(Name::intern(root)) {
        build_expr(scratch, BuildExprRow::Param(slot))
    } else if let Some(initializer) = constants
        .global_bindings
        .get(&(namespace, Name::intern(root)))
    {
        let origin = constants
            .origins
            .get(initializer)
            .copied()
            .unwrap_or(*initializer);
        let cached = scratch.borrow().prepared_constants.get(&origin).cloned();
        let value = if let Some(value) = cached {
            value
        } else {
            let value =
                lower_literal_constant(constants.values.get(initializer)?, Some(wire_enums))?;
            scratch
                .borrow_mut()
                .prepared_constants
                .insert(origin, value.clone());
            value
        };
        build_expr(
            scratch,
            BuildExprRow::PreparedConstant(super::super::PreparedConstantValue(value)),
        )
    } else if root == "env" && !segments.is_empty() {
        return lower_env_command_word_reference(&segments, span, scratch);
    } else {
        return None;
    };
    for segment in segments {
        value = match segment {
            CommandWordRefSegment::Field(name) => build_expr(
                scratch,
                BuildExprRow::Field {
                    base: value,
                    name: name.as_str(),
                    span,
                },
            ),
            CommandWordRefSegment::Index(index) => build_expr(
                scratch,
                BuildExprRow::Index {
                    base: value,
                    index: build_expr(scratch, BuildExprRow::Int(index)),
                    span,
                },
            ),
        };
    }
    Some(value)
}

fn lower_env_command_word_reference(
    segments: &[CommandWordRefSegment],
    span: Span,
    scratch: &Rc<RefCell<BuildScratch>>,
) -> Option<BuildExprId> {
    match segments {
        [CommandWordRefSegment::Field(name)] => {
            if *name == "PATH" {
                let name = build_expr(scratch, BuildExprRow::Str(name.to_string().into()));
                Some(build_expr(
                    scratch,
                    BuildExprRow::ModuleCall {
                        cli_plan: None,
                        op: RuntimeOp::EnvPathList,
                        args: vec![Some(name)],
                        span,
                    },
                ))
            } else {
                let name = build_expr(scratch, BuildExprRow::Str(name.to_string().into()));
                Some(build_expr(
                    scratch,
                    BuildExprRow::ModuleCall {
                        cli_plan: None,
                        op: RuntimeOp::EnvGet,
                        args: vec![Some(name)],
                        span,
                    },
                ))
            }
        }
        [
            CommandWordRefSegment::Field(type_name),
            CommandWordRefSegment::Field(var_name),
        ] => {
            let op = if *type_name == "Path" {
                RuntimeOp::EnvPath
            } else {
                RuntimeOp::EnvGet
            };
            let name = build_expr(scratch, BuildExprRow::Str(var_name.to_string().into()));
            Some(build_expr(
                scratch,
                BuildExprRow::ModuleCall {
                    cli_plan: None,
                    op,
                    args: vec![Some(name)],
                    span,
                },
            ))
        }
        _ => None,
    }
}

fn lowered_run_capture_type(kind: RunKind) -> Option<LoweredType> {
    match kind {
        RunKind::CaptureText => Some(LoweredType::Str),
        RunKind::CaptureBytes => Some(LoweredType::Bytes),
        // `run.capture --text/--bytes` yields a {status, stdout, stderr} record,
        // not a bare Str/Bytes, so field access on the binding lowers.
        RunKind::CaptureTextRecord | RunKind::CaptureBytesRecord => Some(LoweredType::Record),
        RunKind::StreamText | RunKind::StreamBytes => Some(LoweredType::Stream),
        _ => None,
    }
}

fn lowered_run_status_type(kind: RunKind) -> Option<LoweredType> {
    match kind {
        RunKind::Plain | RunKind::Status => Some(LoweredType::Status),
        _ => None,
    }
}

fn lowered_run_binding_type(kind: RunKind) -> Option<LoweredType> {
    lowered_run_capture_type(kind).or(match kind {
        RunKind::Plain | RunKind::Status => Some(LoweredType::Status),
        _ => None,
    })
}

pub(super) fn lowered_arena_run_capture_type(
    arena: &AstArena,
    id: crate::syntax::arena::RunFormId,
) -> Option<LoweredType> {
    let run = arena.run_form(id);
    let segment = arena.run_segments(run.segments).first()?;
    lowered_run_capture_type(segment.kind)
}

fn lowered_arena_run_status_type(
    arena: &AstArena,
    id: crate::syntax::arena::RunFormId,
) -> Option<LoweredType> {
    let run = arena.run_form(id);
    let segment = arena.run_segments(run.segments).first()?;
    lowered_run_status_type(segment.kind)
}

fn compact_run_command_asserts_success(
    arena: &AstArena,
    id: crate::syntax::arena::RunFormId,
) -> bool {
    let run = arena.run_form(id);
    run.propagate
        || matches!(
            arena
                .run_segments(run.segments)
                .first()
                .map(|segment| segment.kind),
            Some(RunKind::Plain)
        )
}

impl<'p> CompactLowerConstructProbe<'p, '_> {
    pub(super) fn lower_print_stmt(
        &mut self,
        id: crate::syntax::arena::CommandStmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let stmt = self.program.arena.command_stmt(id);
        let ArenaCommand::Core {
            name,
            args,
            env,
            block,
        } = stmt.command
        else {
            return None;
        };
        if !matches!(name, CoreCommand::Print | CoreCommand::Eprint)
            || !env.is_empty()
            || block.is_some()
        {
            return None;
        }
        let args = self.program.arena.command_args(args).to_vec();
        let flush = args
            .first()
            .is_some_and(|arg| print_flush_arg(self.program, self.source, self.sources, arg));
        let print_args = if flush { &args[1..] } else { args.as_slice() };
        let mut lowered = Vec::with_capacity(print_args.len());
        for arg in print_args {
            lowered.push(self.lower_command_arg(arg, slots, current_function, item_slot)?);
        }
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::Print {
                args: lowered,
                stderr: name == CoreCommand::Eprint,
                flush,
                propagate_result: stmt.propagate,
                span: self.program.arena.span(stmt.span),
            }
        ))
    }

    pub(super) fn lower_run_stmt(
        &mut self,
        id: crate::syntax::arena::CommandStmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let stmt = self.program.arena.command_stmt(id);
        let ArenaCommand::Run(run) = stmt.command else {
            return None;
        };
        if lowered_arena_run_status_type(&self.program.arena, run).is_some() {
            let assert_success = compact_run_command_asserts_success(&self.program.arena, run);
            return Some(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Run {
                    value: self.lower_run_status_value(
                        run,
                        assert_success,
                        slots,
                        current_function,
                        item_slot,
                    )?,
                    propagate_result: assert_success,
                }
            ));
        }
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::Run {
                value: self.lower_run_binding_value(run, slots, current_function, item_slot)?,
                propagate_result: false,
            }
        ))
    }

    pub(super) fn lower_cd_stmt(
        &mut self,
        id: crate::syntax::arena::CommandStmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let stmt = self.program.arena.command_stmt(id);
        let ArenaCommand::Core {
            name: CoreCommand::Cd,
            args,
            env,
            block,
        } = stmt.command
        else {
            return None;
        };
        if !env.is_empty() {
            return None;
        }
        let args = self.program.arena.command_args(args);
        let [target] = args else {
            return None;
        };
        let target = self.lower_command_arg(target, slots, current_function, item_slot)?;
        let body = match block {
            Some(block) => self.lower_block(block, slots, current_function, item_slot)?,
            None => Vec::new(),
        };
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::Cd {
                target,
                body,
                span: self.program.arena.span(stmt.span),
            }
        ))
    }

    pub(super) fn lower_env_stmt(
        &mut self,
        id: crate::syntax::arena::CommandStmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let stmt = self.program.arena.command_stmt(id);
        let ArenaCommand::Core {
            name: CoreCommand::Env,
            args,
            env,
            block,
        } = stmt.command
        else {
            return None;
        };
        if !self.program.arena.command_args(args).is_empty() {
            return None;
        }
        let assignments = self.program.arena.env_assignments(env).to_vec();
        let mut lowered_env = Vec::with_capacity(assignments.len());
        for assignment in &assignments {
            lowered_env.push(self.lower_run_env(assignment, slots, current_function, item_slot)?);
        }
        let body = match block {
            Some(block) => self.lower_block(block, slots, current_function, item_slot)?,
            None => Vec::new(),
        };
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::Env {
                env: lowered_env,
                body,
            }
        ))
    }

    pub(super) fn lower_proc_stmt(
        &mut self,
        id: crate::syntax::arena::CommandStmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let stmt = self.program.arena.command_stmt(id);
        let ArenaCommand::Proc { name, args } = stmt.command else {
            return None;
        };
        let name_text = name.as_str();
        let (module, api) = super::super::standard_module_command_name(name_text.as_str())?;
        let op = api_spec().module_op(module, api)?;
        let args = self.program.arena.command_args(args).to_vec();
        let mut lowered = Vec::with_capacity(args.len());
        for arg in &args {
            lowered.push(self.lower_command_arg(arg, slots, current_function, item_slot)?);
        }
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::Proc {
                op,
                args: lowered,
                propagate_result: stmt.propagate,
                span: self.program.arena.span(stmt.span),
            }
        ))
    }

    fn lower_command_arg(
        &mut self,
        arg: &crate::syntax::arena::ArenaCommandArg,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        match &arg.kind {
            ArenaCommandArgKind::Typed(expr) => {
                self.lower_expr(*expr, slots, current_function, item_slot)
            }
            ArenaCommandArgKind::Word(parts) => {
                let span = self.program.arena.span(arg.span);
                let parts = self.program.arena.word_parts(*parts).collect::<Vec<_>>();
                if let [ArenaWordPart::Bare(text)] = parts.as_slice() {
                    let text = self.bare_text_value_in_span(text, span)?;
                    if let Some(value) = lower_command_word_reference(
                        text,
                        slots,
                        span,
                        &self.scratch,
                        &self.declarations.prepared_constants,
                        &self.declarations.wire_enums,
                        self.current_namespace,
                    ) {
                        return Some(value);
                    }
                }
                if let [ArenaWordPart::Shorthand(expr) | ArenaWordPart::Interpolation(expr)] =
                    parts.as_slice()
                {
                    return self.lower_expr(*expr, slots, current_function, item_slot);
                }
                let mut lowered = Vec::with_capacity(parts.len());
                for part in parts {
                    match part {
                        ArenaWordPart::Bare(text) => {
                            lowered.push(LoweredFmtPart::Text(Arc::from(
                                self.bare_text_value_in_span(&text, span)?,
                            )));
                        }
                        ArenaWordPart::Quoted(text) => {
                            lowered.push(LoweredFmtPart::Text(Arc::from(
                                self.text_value_in_span(&text, span)?,
                            )));
                        }
                        ArenaWordPart::Shorthand(expr) | ArenaWordPart::Interpolation(expr) => {
                            let span = self.program.arena.expr(expr).span;
                            lowered.push(LoweredFmtPart::Expr(
                                self.lower_expr(expr, slots, current_function, item_slot)?,
                                span,
                                None,
                            ));
                        }
                    }
                }
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::FmtString(lowered)
                ))
            }
            ArenaCommandArgKind::SpliceName(name) => {
                self.lower_splice_name(*name, self.program.arena.span(arg.span), slots)
            }
            ArenaCommandArgKind::SpliceExpr(expr) => {
                self.lower_expr(*expr, slots, current_function, item_slot)
            }
        }
    }

    pub(super) fn lower_spawn_expr(
        &mut self,
        target: ArenaSpawnTarget,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        match target {
            ArenaSpawnTarget::Command(command) => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::SpawnCommand {
                    command: self.lower_expr(command, slots, current_function, item_slot)?,
                    span,
                }
            )),
            ArenaSpawnTarget::Run(run) => {
                let form = self.program.arena.run_form(run);
                if form.propagate {
                    return None;
                }
                let [segment] = self.program.arena.run_segments(form.segments) else {
                    return None;
                };
                if !matches!(segment.kind, RunKind::Plain | RunKind::Status) || segment.grouped {
                    return None;
                }
                let target = Box::new(self.lower_run_arg(
                    &segment.target,
                    slots,
                    current_function,
                    item_slot,
                )?);
                let args = self.program.arena.command_args(segment.args).to_vec();
                let mut lowered_args = Vec::with_capacity(args.len());
                for arg in &args {
                    lowered_args.push(self.lower_run_arg(
                        arg,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
                let env = self.program.arena.env_assignments(segment.env).to_vec();
                let mut lowered_env = Vec::with_capacity(env.len());
                for assignment in &env {
                    lowered_env.push(self.lower_run_env(
                        assignment,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
                let redirections = self
                    .program
                    .arena
                    .redirections(segment.redirections)
                    .to_vec()
                    .into_iter()
                    .map(|redirection| {
                        self.lower_run_redirection(&redirection, slots, current_function, item_slot)
                    })
                    .collect::<Option<Vec<_>>>()?;
                let timeout = match segment.timeout {
                    Some(expr) => {
                        Some(self.lower_expr(expr, slots, current_function, item_slot)?)
                    }
                    None => None,
                };
                let cpu_max = match segment.cpu_max {
                    Some(expr) => {
                        Some(self.lower_expr(expr, slots, current_function, item_slot)?)
                    }
                    None => None,
                };
                let accept = match segment.accept {
                    Some(expr) => {
                        Some(self.lower_expr(expr, slots, current_function, item_slot)?)
                    }
                    None => None,
                };
                let run = LoweredSpawnRun {
                    target,
                    args: lowered_args,
                    env: lowered_env,
                    redirections,
                    timeout,
                    cpu_max,
                    accept,
                    span,
                };
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::SpawnRun(Box::new(run))
                ))
            }
        }
    }

    pub(super) fn lower_run_binding_value(
        &mut self,
        id: crate::syntax::arena::RunFormId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        self.lower_run_value_with_propagate(
            id,
            slots,
            current_function,
            item_slot,
            true,
            lowered_run_binding_type,
        )
    }

    fn lower_run_status_value(
        &mut self,
        id: crate::syntax::arena::RunFormId,
        assert_success: bool,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        self.lower_run_value_with_propagate_and_assert(
            id,
            slots,
            current_function,
            item_slot,
            true,
            assert_success,
            lowered_run_status_type,
        )
    }

    fn lower_run_value_with_propagate(
        &mut self,
        id: crate::syntax::arena::RunFormId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        allow_propagate: bool,
        allowed_type: fn(RunKind) -> Option<LoweredType>,
    ) -> Option<BuildExprId> {
        self.lower_run_value_with_propagate_and_assert(
            id,
            slots,
            current_function,
            item_slot,
            allow_propagate,
            false,
            allowed_type,
        )
    }

    fn lower_run_value_with_propagate_and_assert(
        &mut self,
        id: crate::syntax::arena::RunFormId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        allow_propagate: bool,
        assert_success: bool,
        allowed_type: fn(RunKind) -> Option<LoweredType>,
    ) -> Option<BuildExprId> {
        let run = self.program.arena.run_form(id);
        if run.propagate && !allow_propagate {
            return None;
        }
        let segments = self.program.arena.run_segments(run.segments).to_vec();
        if segments.is_empty() {
            return None;
        }
        if segments.len() == 1 {
            let segment = &segments[0];
            allowed_type(segment.kind)?;
            let target = Box::new(self.lower_run_arg(
                &segment.target,
                slots,
                current_function,
                item_slot,
            )?);
            let args = self.program.arena.command_args(segment.args).to_vec();
            let mut lowered_args = Vec::with_capacity(args.len());
            for arg in &args {
                lowered_args.push(self.lower_run_arg(arg, slots, current_function, item_slot)?);
            }
            let env = self.program.arena.env_assignments(segment.env).to_vec();
            let mut lowered_env = Vec::with_capacity(env.len());
            for assignment in &env {
                lowered_env.push(self.lower_run_env(
                    assignment,
                    slots,
                    current_function,
                    item_slot,
                )?);
            }
            let redirections = self
                .program
                .arena
                .redirections(segment.redirections)
                .to_vec()
                .into_iter()
                .map(|redirection| {
                    self.lower_run_redirection(&redirection, slots, current_function, item_slot)
                })
                .collect::<Option<Vec<_>>>()?;
            let timeout = match segment.timeout {
                Some(expr) => Some(self.lower_expr(expr, slots, current_function, item_slot)?),
                None => None,
            };
            let cpu_max = match segment.cpu_max {
                Some(expr) => Some(self.lower_expr(expr, slots, current_function, item_slot)?),
                None => None,
            };
            let accept = match segment.accept {
                Some(expr) => Some(self.lower_expr(expr, slots, current_function, item_slot)?),
                None => None,
            };
            // Capture/stream kinds return a Result and are unwrapped by an
            // external `Try`. Plain/Status return a bare Status on success, so
            // `?` propagation is handled inside eval_lowered_run_capture via the
            // `propagate` flag instead.
            let capture_kind = lowered_run_capture_type(segment.kind).is_some();
            let propagate_internally = run.propagate && !capture_kind;
            let capture = LoweredRunCapture {
                kind: segment.kind,
                target,
                args: lowered_args,
                env: lowered_env,
                redirections,
                timeout,
                cpu_max,
                accept,
                propagate: propagate_internally,
                assert_success,
                span: self.program.arena.span(run.span),
            };
            let capture = push_build_row!(self, expr, BuildExprRow::RunCapture(Box::new(capture)));
            if run.propagate && capture_kind {
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Try {
                        value: capture,
                        span: self.program.arena.span(run.span)
                    }
                ))
            } else {
                Some(capture)
            }
        } else {
            let mut lowered_segments = Vec::with_capacity(segments.len());
            for segment in &segments {
                allowed_type(segment.kind)?;
                let target =
                    self.lower_run_arg(&segment.target, slots, current_function, item_slot)?;
                let args = self.program.arena.command_args(segment.args).to_vec();
                let mut lowered_args = Vec::with_capacity(args.len());
                for arg in &args {
                    lowered_args.push(self.lower_run_arg(
                        arg,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
                let env = self.program.arena.env_assignments(segment.env).to_vec();
                let mut lowered_env = Vec::with_capacity(env.len());
                for assignment in &env {
                    lowered_env.push(self.lower_run_env(
                        assignment,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
                let redirections = self
                    .program
                    .arena
                    .redirections(segment.redirections)
                    .to_vec()
                    .into_iter()
                    .map(|redirection| {
                        self.lower_run_redirection(&redirection, slots, current_function, item_slot)
                    })
                    .collect::<Option<Vec<_>>>()?;
                let timeout = match segment.timeout {
                    Some(expr) => {
                        Some(self.lower_expr(expr, slots, current_function, item_slot)?)
                    }
                    None => None,
                };
                let cpu_max = match segment.cpu_max {
                    Some(expr) => {
                        Some(self.lower_expr(expr, slots, current_function, item_slot)?)
                    }
                    None => None,
                };
                let accept = match segment.accept {
                    Some(expr) => {
                        Some(self.lower_expr(expr, slots, current_function, item_slot)?)
                    }
                    None => None,
                };
                lowered_segments.push(LoweredRunPipelineSegment {
                    kind: segment.kind,
                    target,
                    args: lowered_args,
                    env: lowered_env,
                    redirections,
                    timeout,
                    cpu_max,
                    accept,
                });
            }
            // A capturing head yields a Result unwrapped by an external `Try`.
            // A statement-position status pipeline asserts success like a lone
            // `run`: it yields a Result the statement row propagates.
            let capture_kind = lowered_run_capture_type(segments[0].kind).is_some();
            let pipeline = push_build_row!(
                self,
                expr,
                BuildExprRow::RunPipeline {
                    segments: lowered_segments,
                    propagate: !capture_kind && (run.propagate || assert_success),
                    span: self.program.arena.span(run.span),
                }
            );
            if run.propagate && (capture_kind || !assert_success) {
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Try {
                        value: pipeline,
                        span: self.program.arena.span(run.span)
                    }
                ))
            } else {
                Some(pipeline)
            }
        }
    }

    fn lower_run_redirection(
        &mut self,
        redirection: &crate::syntax::arena::ArenaRedirection,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<LoweredRunRedirection> {
        let target = match &redirection.target {
            crate::syntax::arena::ArenaRedirectionTarget::Path(target) => {
                self.lower_run_redirection_path_target(target, slots, current_function, item_slot)?
            }
            crate::syntax::arena::ArenaRedirectionTarget::Fd(target) => {
                self.lower_run_arg(target, slots, current_function, item_slot)?
            }
        };
        Some(LoweredRunRedirection {
            kind: redirection.kind,
            target,
            span: self.program.arena.span(redirection.span),
        })
    }

    fn lower_run_redirection_path_target(
        &mut self,
        target: &crate::syntax::arena::ArenaCommandArg,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<LoweredRunArg> {
        if let ArenaCommandArgKind::Word(parts) = &target.kind {
            let span = self.program.arena.span(target.span);
            let parts = self.program.arena.word_parts(*parts).collect::<Vec<_>>();
            if let [ArenaWordPart::Bare(text)] = parts.as_slice() {
                let text = self.bare_text_value_in_span(text, span)?;
                if let Some(slot) = slots.resolve(Name::intern(text)) {
                    return Some(LoweredRunArg {
                        kind: LoweredRunArgKind::Single(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::Param(slot)
                        )),
                        span,
                    });
                }
            }
        }
        self.lower_run_arg(target, slots, current_function, item_slot)
    }

    fn lower_run_env(
        &mut self,
        assignment: &crate::syntax::arena::ArenaEnvAssignment,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<LoweredRunEnv> {
        let value = match &assignment.value {
            crate::syntax::arena::ArenaEnvAssignmentValue::CommandArg(arg) => {
                if matches!(
                    arg.kind,
                    ArenaCommandArgKind::SpliceName(_) | ArenaCommandArgKind::SpliceExpr(_)
                ) {
                    return None;
                }
                self.lower_run_arg(arg, slots, current_function, item_slot)?
            }
            crate::syntax::arena::ArenaEnvAssignmentValue::Expr(expr) => LoweredRunArg {
                kind: LoweredRunArgKind::Single(self.lower_expr(
                    *expr,
                    slots,
                    current_function,
                    item_slot,
                )?),
                span: self.program.arena.expr(*expr).span,
            },
        };
        Some(LoweredRunEnv {
            name: assignment.name,
            value,
        })
    }

    fn lower_run_arg(
        &mut self,
        arg: &crate::syntax::arena::ArenaCommandArg,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<LoweredRunArg> {
        let span = self.program.arena.span(arg.span);
        let kind =
            match &arg.kind {
                ArenaCommandArgKind::Typed(expr) => LoweredRunArgKind::Single(self.lower_expr(
                    *expr,
                    slots,
                    current_function,
                    item_slot,
                )?),
                ArenaCommandArgKind::Word(parts) => {
                    let parts = self.program.arena.word_parts(*parts).collect::<Vec<_>>();
                    if let [ArenaWordPart::Shorthand(expr) | ArenaWordPart::Interpolation(expr)] =
                        parts.as_slice()
                    {
                        LoweredRunArgKind::SingleOrSplice(self.lower_expr(
                            *expr,
                            slots,
                            current_function,
                            item_slot,
                        )?)
                    } else {
                        let mut lowered = Vec::with_capacity(parts.len());
                        for part in parts {
                            match part {
                                ArenaWordPart::Bare(text) => {
                                    lowered.push(LoweredFmtPart::Text(Arc::from(
                                        self.bare_text_value_in_span(&text, span)?,
                                    )));
                                }
                                ArenaWordPart::Quoted(text) => {
                                    lowered.push(LoweredFmtPart::Text(Arc::from(
                                        self.text_value_in_span(&text, span)?,
                                    )));
                                }
                                ArenaWordPart::Shorthand(expr)
                                | ArenaWordPart::Interpolation(expr) => {
                                    lowered.push(LoweredFmtPart::Expr(
                                        self.lower_expr(expr, slots, current_function, item_slot)?,
                                        self.program.arena.expr(expr).span,
                                        None,
                                    ));
                                }
                            }
                        }
                        LoweredRunArgKind::Single(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::PathFmtString {
                                parts: lowered,
                                span
                            }
                        ))
                    }
                }
                ArenaCommandArgKind::SpliceName(name) => LoweredRunArgKind::Splice(
                    self.lower_splice_name(*name, self.program.arena.span(arg.span), slots)?,
                ),
                ArenaCommandArgKind::SpliceExpr(expr) => LoweredRunArgKind::Splice(
                    self.lower_expr(*expr, slots, current_function, item_slot)?,
                ),
            };
        Some(LoweredRunArg { kind, span })
    }

    pub(super) fn lower_env_field(
        &mut self,
        base: crate::syntax::arena::ExprId,
        name: crate::symbol::Name,
        span: crate::source::Span,
    ) -> Option<BuildExprId> {
        let base_kind = self.program.arena.expr(base).kind;
        match base_kind {
            ArenaExprKind::Ident(base_name) if base_name == "env" => {
                if name == "PATH" {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ModuleCall {
                            cli_plan: None,
                            op: RuntimeOp::EnvPathList,
                            args: Vec::new(),
                            span,
                        }
                    ));
                }
                let name = push_build_row!(self, expr, BuildExprRow::Str(name.to_string().into()));
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ModuleCall {
                        cli_plan: None,
                        op: RuntimeOp::EnvGet,
                        args: vec![Some(name)],
                        span,
                    }
                ))
            }
            ArenaExprKind::Field {
                base: inner_base,
                name: type_name,
            } => {
                let inner_kind = self.program.arena.expr(inner_base).kind;
                if let ArenaExprKind::Ident(inner_name) = inner_kind {
                    if inner_name == "env" {
                        let arg =
                            push_build_row!(self, expr, BuildExprRow::Str(name.to_string().into()));
                        let op = match type_name.as_str().as_str() {
                            "Path" => RuntimeOp::EnvPath,
                            "PathList" => RuntimeOp::EnvPathList,
                            _ => RuntimeOp::EnvGet,
                        };
                        Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
                                cli_plan: None,
                                op,
                                args: vec![Some(arg)],
                                span,
                            }
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub(super) fn lower_process_command_builder(
        &mut self,
        call: ExprId,
        block: crate::syntax::arena::BuilderBlockId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        span: Span,
    ) -> Option<BuildExprId> {
        let (module, name, args) = match self.program.arena.expr(call).kind {
            ArenaExprKind::Field { base, name } => {
                let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind else {
                    return None;
                };
                (module, name, None)
            }
            ArenaExprKind::Call { callee, args } => {
                let ArenaExprKind::Field { base, name } = self.program.arena.expr(callee).kind
                else {
                    return None;
                };
                let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind else {
                    return None;
                };
                (module, name, Some(args))
            }
            _ => return None,
        };
        if module != "process" || name != "command" {
            return None;
        }
        if args.is_some_and(|args| !self.program.arena.call_args(args).is_empty()) {
            return None;
        }

        let entries = self
            .program
            .arena
            .builder_entries(self.program.arena.builder_block(block).entries)
            .to_vec();
        let mut lowered = Vec::with_capacity(entries.len());
        let mut run_seen = false;
        for entry in entries {
            match entry.kind {
                ArenaBuilderEntryKind::Field { name, value } => {
                    lowered.push(LoweredProcessCommandBuilderEntry::Field {
                        name,
                        value: self.lower_expr(value, slots, current_function, item_slot)?,
                        span: self.program.arena.span(entry.span),
                    });
                }
                ArenaBuilderEntryKind::Stmt(stmt) => {
                    let ArenaStmtKind::Command(command) = self.program.arena.stmt(stmt).kind else {
                        return None;
                    };
                    let command_stmt = self.program.arena.command_stmt(command);
                    let ArenaCommand::Run(run) = command_stmt.command else {
                        return None;
                    };
                    if command_stmt.propagate || run_seen {
                        return None;
                    }
                    let run_form = self.program.arena.run_form(run);
                    if run_form.propagate {
                        return None;
                    }
                    let [segment] = self.program.arena.run_segments(run_form.segments) else {
                        return None;
                    };
                    if !matches!(segment.kind, RunKind::Plain | RunKind::Status)
                        || !self
                            .program
                            .arena
                            .redirections(segment.redirections)
                            .is_empty()
                    {
                        return None;
                    }
                    let target =
                        self.lower_run_arg(&segment.target, slots, current_function, item_slot)?;
                    let args = self.program.arena.command_args(segment.args).to_vec();
                    let mut lowered_args = Vec::with_capacity(args.len());
                    for arg in &args {
                        lowered_args.push(self.lower_run_arg(
                            arg,
                            slots,
                            current_function,
                            item_slot,
                        )?);
                    }
                    let env = self.program.arena.env_assignments(segment.env).to_vec();
                    let mut lowered_env = Vec::with_capacity(env.len());
                    for assignment in &env {
                        lowered_env.push(self.lower_run_env(
                            assignment,
                            slots,
                            current_function,
                            item_slot,
                        )?);
                    }
                    lowered.push(LoweredProcessCommandBuilderEntry::Run {
                        target,
                        args: lowered_args,
                        env: lowered_env,
                        timeout: match segment.timeout {
                            Some(value) => {
                                Some(self.lower_expr(value, slots, current_function, item_slot)?)
                            }
                            None => None,
                        },
                        cpu_max: match segment.cpu_max {
                            Some(value) => {
                                Some(self.lower_expr(value, slots, current_function, item_slot)?)
                            }
                            None => None,
                        },
                        accept: match segment.accept {
                            Some(value) => {
                                Some(self.lower_expr(value, slots, current_function, item_slot)?)
                            }
                            None => None,
                        },
                        span: self.program.arena.span(command_stmt.span),
                    });
                    run_seen = true;
                }
                ArenaBuilderEntryKind::Entry { .. } | ArenaBuilderEntryKind::Task { .. } => {
                    return None;
                }
            }
        }
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::ProcessCommandBuilder {
                entries: lowered,
                span,
            }
        ))
    }

    /// `e"NAME" = value` runs as an `EnvSet` call in statement position.
    pub(super) fn lower_env_assignment(
        &mut self,
        id: StmtId,
        name: Name,
        value: ArenaExprOrRun,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let span = self.program.arena.stmt(id).span;
        let value = match value {
            ArenaExprOrRun::Expr(expr) => {
                self.lower_expr(expr, slots, current_function, item_slot)?
            }
            ArenaExprOrRun::Run(run) => {
                self.lower_run_binding_value(run, slots, current_function, item_slot)?
            }
        };
        let name = push_build_row!(self, expr, BuildExprRow::Str(name.to_string().into()));
        let call = push_build_row!(
            self,
            expr,
            BuildExprRow::ModuleCall {
                cli_plan: None,
                op: RuntimeOp::EnvSet,
                args: vec![Some(name), Some(value)],
                span,
            }
        );
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::Expr { value: call, span }
        ))
    }
}
