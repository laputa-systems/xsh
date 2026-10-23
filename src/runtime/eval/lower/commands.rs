use super::{
    Arc, ArenaBuilderEntryKind, ArenaCommand, ArenaCommandArgKind, ArenaExprKind, ArenaExprOrRun,
    ArenaSpawnTarget, ArenaStmtKind, ArenaWordPart, AstArena, BuildExprId, BuildExprRow,
    BuildScratch, BuildStmtId, BuildStmtRow, CommandWordRefSegment, CompactLowerConstructProbe,
    CoreCommand, ExprId, LoweredFmtPart, LoweredProcessCommandBuilderEntry, LoweredRunArg,
    LoweredRunArgKind, LoweredRunCapture, LoweredRunEnv, LoweredRunPipelineSegment,
    LoweredRunOption, LoweredRunOperands, LoweredRunRedirection, LoweredRunTailOperand,
    LoweredSpawnRun, LoweredType, Name, Rc, RefCell, RunKind, RuntimeOp, SlotScope, Span,
    StmtId, build_expr, SchemeType, Type,
    parse_command_word_reference, print_flush_arg,
};

pub(super) fn lower_command_word_reference(
    text: &str,
    slots: &SlotScope,
    span: Span,
    scratch: &Rc<RefCell<BuildScratch>>,
    constants: &crate::sema::constants::PreparedConstants,
    wire_enums: &crate::sema::wire_enums::PreparedWireEnums,
    bindings: &crate::sema::check::ResolvedBindings,
) -> Option<BuildExprId> {
    lower_command_word_reference_in_domain(text, slots, span, scratch, constants, wire_enums, bindings, &mut CommandLoweringDomain::Ordinary)
}

pub(super) fn lower_command_word_reference_in_domain(
    text: &str,
    slots: &SlotScope,
    span: Span,
    scratch: &Rc<RefCell<BuildScratch>>,
    constants: &crate::sema::constants::PreparedConstants,
    wire_enums: &crate::sema::wire_enums::PreparedWireEnums,
    bindings: &crate::sema::check::ResolvedBindings,
    domain: &mut CommandLoweringDomain<'_>,
) -> Option<BuildExprId> {
    let (root, segments) = parse_command_word_reference(text)?;
    let binding = bindings.at(span, Name::intern(root));
    let mut current_type = None;
    let mut value = if let Some(initializer) = binding.and_then(|binding| constants.binding_values.get(&binding)) {
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
                super::lower_prepared_literal(constants.values.get(initializer)?, Some(wire_enums))?;
            scratch
                .borrow_mut()
                .prepared_constants
                .insert(origin, value.clone());
            value
        };
        if matches!(domain, CommandLoweringDomain::Generalized { .. }) { current_type = Some(SchemeType::Ground(constants.types.get(initializer)?.clone())); }
        build_expr(scratch, BuildExprRow::PreparedConstant(value))
    } else if let Some(binding) = binding {
        if matches!(domain, CommandLoweringDomain::Generalized { .. }) { current_type = Some(slots.types.get(&binding)?.clone()); }
        build_expr(scratch, BuildExprRow::Param(slots.resolve_binding(binding)?))
    } else if root == "env" && !segments.is_empty() {
        return lower_env_command_word_reference(&segments, span, scratch, domain);
    } else {
        return None;
    };
    if let Some(ty) = &current_type { domain.record(value, ty.clone()); }
    for segment in segments {
        value = match segment {
            CommandWordRefSegment::Field(name) => {
                if let Some(ty) = &current_type { current_type = Some(CompactLowerConstructProbe::scheme_record_field(ty, name)?); }
                build_expr(
                scratch,
                BuildExprRow::Field {
                    base: value,
                    name: name.as_str(),
                    span,
                },
            ) },
            CommandWordRefSegment::Index(index) => {
                if let Some(ty) = &current_type {
                    current_type = Some(match ty {
                        SchemeType::List(item) => (**item).clone(),
                        SchemeType::Ground(Type::List(item)) => SchemeType::Ground((**item).clone()),
                        _ => return None,
                    });
                }
                let key = build_expr(scratch, BuildExprRow::Int(index));
                domain.record(key, SchemeType::Ground(Type::Int));
                build_expr(
                scratch,
                BuildExprRow::Index {
                    base: value,
                    index: super::super::IndexOperand::Ordinary(key),
                    span,
                },
            ) },
        };
        if let Some(ty) = &current_type { domain.record(value, ty.clone()); }
    }
    Some(value)
}

fn lower_env_command_word_reference(
    segments: &[CommandWordRefSegment],
    span: Span,
    scratch: &Rc<RefCell<BuildScratch>>,
    domain: &mut CommandLoweringDomain<'_>,
) -> Option<BuildExprId> {
    let mut expression = |row: BuildExprRow| -> Option<BuildExprId> {
        if matches!(domain, CommandLoweringDomain::Ordinary) { return Some(build_expr(scratch, row)); }
        let ty = match &row {
            BuildExprRow::Str(_) => Type::Str,
            BuildExprRow::ModuleCall { op, .. } => {
                let mut signatures = crate::modules::signature::native_static_operation_signatures(*op);
                let first = signatures.next()?;
                if signatures.any(|signature| signature.return_ty != first.return_ty) { return None; }
                first.return_ty.clone()
            }
            _ => return None,
        };
        let value = build_expr(scratch, row);
        domain.record(value, SchemeType::Ground(ty));
        Some(value)
    };
    match segments {
        [CommandWordRefSegment::Field(name)] => {
            if *name == "PATH" {
                let name = expression(BuildExprRow::Str(name.to_string().into()))?;
                expression(
                    BuildExprRow::ModuleCall {
                        call_plan: None,
                        op: RuntimeOp::EnvPathList,
                        args: vec![Some(name)],
                        span,
                    },
                )
            } else {
                let name = expression(BuildExprRow::Str(name.to_string().into()))?;
                expression(
                    BuildExprRow::ModuleCall {
                        call_plan: None,
                        op: RuntimeOp::EnvGet,
                        args: vec![Some(name)],
                        span,
                    },
                )
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
            let name = expression(BuildExprRow::Str(var_name.to_string().into()))?;
            expression(
                BuildExprRow::ModuleCall {
                    call_plan: None,
                    op,
                    args: vec![Some(name)],
                    span,
                },
            )
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

pub(super) enum CommandLoweringDomain<'a> {
    Ordinary,
    Generalized {
        facts: &'a crate::sema::schemes::SchemeBodyFacts,
        visited: &'a mut super::FxHashSet<ExprId>,
        types: &'a mut super::BTreeMap<BuildExprId, crate::sema::schemes::SchemeType>,
    },
}

impl CommandLoweringDomain<'_> {
    fn record(&mut self, value: BuildExprId, ty: SchemeType) {
        if let Self::Generalized { types, .. } = self { types.insert(value, ty); }
    }

    fn record_alias(&mut self, value: BuildExprId, original: BuildExprId) -> Option<()> {
        if let Self::Generalized { types, .. } = self {
            let ty = types.get(&original)?.clone();
            types.insert(value, ty);
        }
        Some(())
    }
}

impl<'p> CompactLowerConstructProbe<'p, '_> {
    fn build_command_expression(&mut self, row: BuildExprRow, ty: Type, domain: &mut CommandLoweringDomain<'_>) -> BuildExprId {
        let value = build_expr(&self.scratch, row);
        domain.record(value, SchemeType::Ground(ty));
        value
    }

    fn lower_command_expression(
        &mut self, expression: ExprId, slots: &mut SlotScope,
        current_function: Option<Name>, item_slot: Option<usize>,
        domain: &mut CommandLoweringDomain<'_>,
    ) -> Option<BuildExprId> {
        match domain {
            CommandLoweringDomain::Ordinary => self.lower_expr(expression, slots, current_function, item_slot),
            CommandLoweringDomain::Generalized { facts, visited, types } => self.lower_scheme_lazy_expression(expression, slots, current_function?, facts, visited, types),
        }
    }

    fn lower_command_block(
        &mut self, block: crate::syntax::arena::BlockId, slots: &mut SlotScope,
        current_function: Option<Name>, item_slot: Option<usize>,
        domain: &mut CommandLoweringDomain<'_>,
    ) -> Option<Vec<BuildStmtId>> {
        match domain {
            CommandLoweringDomain::Ordinary => self.lower_block(block, slots, current_function, item_slot),
            CommandLoweringDomain::Generalized { facts, visited, types } => self.lower_scheme_statement_block(block, slots, current_function?, facts, visited, types),
        }
    }

    pub(super) fn lower_generalized_command(
        &mut self, command: crate::syntax::arena::CommandStmtId, slots: &mut SlotScope,
        current_function: Name, domain: &mut CommandLoweringDomain<'_>,
    ) -> Option<BuildStmtId> {
        match self.program.arena.command_stmt(command).command {
            ArenaCommand::Core { name: CoreCommand::Print | CoreCommand::Eprint, .. } => self.lower_print_stmt_in_domain(command, slots, Some(current_function), None, domain),
            ArenaCommand::Core { name: CoreCommand::Cd, .. } => self.lower_cd_stmt_in_domain(command, slots, Some(current_function), None, domain),
            ArenaCommand::Core { name: CoreCommand::Env, .. } => self.lower_env_stmt_in_domain(command, slots, Some(current_function), None, domain),
            ArenaCommand::Run(_) => self.lower_run_stmt_in_domain(command, slots, Some(current_function), None, domain),
            ArenaCommand::Proc { .. } => self.lower_proc_stmt_in_domain(command, slots, Some(current_function), None, domain),
        }
    }

    pub(super) fn lower_print_stmt(
        &mut self,
        id: crate::syntax::arena::CommandStmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        self.lower_print_stmt_in_domain(id, slots, current_function, item_slot, &mut CommandLoweringDomain::Ordinary)
    }

    pub(super) fn lower_print_stmt_in_domain(
        &mut self,
        id: crate::syntax::arena::CommandStmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        domain: &mut CommandLoweringDomain<'_>,
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
            lowered.push(self.lower_command_arg_in_domain(arg, slots, current_function, item_slot, domain)?);
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
        self.lower_run_stmt_in_domain(id, slots, current_function, item_slot, &mut CommandLoweringDomain::Ordinary)
    }

    pub(super) fn lower_run_stmt_in_domain(
        &mut self,
        id: crate::syntax::arena::CommandStmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        domain: &mut CommandLoweringDomain<'_>,
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
                    value: self.lower_run_status_value_in_domain(
                        run,
                        assert_success,
                        slots,
                        current_function,
                        item_slot, domain)?,
                    propagate_result: assert_success,
                }
            ));
        }
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::Run {
                value: self.lower_run_binding_value_in_domain(run, slots, current_function, item_slot, domain)?,
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
        self.lower_cd_stmt_in_domain(id, slots, current_function, item_slot, &mut CommandLoweringDomain::Ordinary)
    }

    pub(super) fn lower_cd_stmt_in_domain(
        &mut self,
        id: crate::syntax::arena::CommandStmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        domain: &mut CommandLoweringDomain<'_>,
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
        let target = self.lower_command_arg_in_domain(target, slots, current_function, item_slot, domain)?;
        let body = match block {
            Some(block) => self.lower_command_block(block, slots, current_function, item_slot, domain)?,
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
        self.lower_env_stmt_in_domain(id, slots, current_function, item_slot, &mut CommandLoweringDomain::Ordinary)
    }

    pub(super) fn lower_env_stmt_in_domain(
        &mut self,
        id: crate::syntax::arena::CommandStmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        domain: &mut CommandLoweringDomain<'_>,
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
            lowered_env.push(self.lower_run_env_in_domain(assignment, slots, current_function, item_slot, domain)?);
        }
        let body = match block {
            Some(block) => self.lower_command_block(block, slots, current_function, item_slot, domain)?,
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
        self.lower_proc_stmt_in_domain(id, slots, current_function, item_slot, &mut CommandLoweringDomain::Ordinary)
    }

    pub(super) fn lower_proc_stmt_in_domain(
        &mut self,
        id: crate::syntax::arena::CommandStmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        domain: &mut CommandLoweringDomain<'_>,
    ) -> Option<BuildStmtId> {
        let stmt = self.program.arena.command_stmt(id);
        let ArenaCommand::Proc { args, .. } = stmt.command else {
            return None;
        };
        let span = self.program.arena.span(stmt.span);
        let checked = self.bodies.module_commands.get(&span)?.clone();
        let args = self.program.arena.command_args(args).to_vec();
        if args.len() != checked.call.argument_slots.len() || args.len() != checked.arguments.len() {
            return None;
        }
        let mut values = vec![None; checked.call.sig.params.len()];
        let mut bindings = Vec::new();
        let mut conversions = Vec::new();
        let mut flags = Vec::new();
        for ((arg, &slot), kind) in args.iter().zip(&checked.call.argument_slots).zip(&checked.arguments) {
            let arg_span = self.program.arena.span(arg.span);
            let value = if matches!(kind, crate::sema::check::CheckedCommandArgument::BooleanFlag) {
                flags.push(slot);
                self.build_command_expression(BuildExprRow::Bool(true), Type::Bool, domain)
            } else {
                let value = self.lower_command_arg_in_domain(arg, slots, current_function, item_slot, domain)?;
                let binding = slots.reserve("module command argument");
                bindings.push((value, binding));
                let original = value;
                let value = push_build_row!(self, expr, BuildExprRow::Param(binding));
                domain.record_alias(value, original)?;
                match kind {
                    crate::sema::check::CheckedCommandArgument::Word(ty) => {
                        if let Some(scalar) = super::super::call_plan::command_word_scalar(ty) {
                            conversions.push(super::super::call_plan::CommandWordConversion { slot, scalar, span: arg_span });
                        }
                        value
                    }
                    _ => {
                        let expected = &checked.call.sig.params.get(slot)?.ty;
                        let checked = self.checked_unsigned_value(value, expected, arg_span);
                        if checked != value { domain.record(checked, SchemeType::Ground(expected.clone())); }
                        checked
                    },
                }
            };
            if values.get_mut(slot)?.replace(value).is_some() { return None; }
        }
        let plan = super::super::call_plan::CommandArgumentsPlan {
            signature: checked.call.sig,
            supplied_slots: values.iter().map(Option::is_some).collect(),
            word_conversions: conversions,
            flag_slots: flags,
        };
        let call = push_build_row!(self, expr, BuildExprRow::ModuleCall {
            op: checked.call.sig.op,
            call_plan: Some(Arc::new(super::super::ModuleCallPlan::CommandArguments(plan))),
            args: values,
            span,
        });
        domain.record(call, SchemeType::Ground(checked.call.sig.return_ty.clone()));
        let mut call = call;
        for (subject, slot) in bindings.into_iter().rev() {
            let pattern = push_build_row!(self, pattern, super::BuildPatternRow::Bind { slot });
            let wrapped = push_build_row!(self, expr, BuildExprRow::MatchExpr { value: subject, arms: vec![(pattern, None, call)], span });
            domain.record_alias(wrapped, call)?;
            call = wrapped;
        }
        Some(push_build_row!(self, stmt, BuildStmtRow::Expr { value: call, span }))
    }

    fn lower_command_splice_name(
        &self, name: Name, span: Span, slots: &SlotScope,
        domain: &mut CommandLoweringDomain<'_>,
    ) -> Option<BuildExprId> {
        match domain {
            CommandLoweringDomain::Ordinary => self.lower_splice_name(name, span, slots),
            CommandLoweringDomain::Generalized { .. } => lower_command_word_reference_in_domain(
                name.as_str().as_str(), slots, span, &self.scratch, &self.declarations.prepared_constants,
                &self.declarations.wire_enums, &self.bodies.resolved_bindings, domain,
            ),
        }
    }

    fn lower_command_arg_in_domain(
        &mut self,
        arg: &crate::syntax::arena::ArenaCommandArg,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        domain: &mut CommandLoweringDomain<'_>,
    ) -> Option<BuildExprId> {
        match &arg.kind {
            ArenaCommandArgKind::Typed(expr) => {
                self.lower_command_expression(*expr, slots, current_function, item_slot, domain)
            }
            ArenaCommandArgKind::Word(parts) => {
                let span = self.program.arena.span(arg.span);
                let parts = self.program.arena.word_parts(*parts).collect::<Vec<_>>();
                if let [ArenaWordPart::Bare(text)] = parts.as_slice() {
                    let text = self.bare_text_value_in_span(text, span)?;
                    let reference = lower_command_word_reference_in_domain(
                        text,
                        slots,
                        span,
                        &self.scratch,
                        &self.declarations.prepared_constants,
                        &self.declarations.wire_enums,
                        &self.bodies.resolved_bindings,
                        domain,
                    );
                    if parse_command_word_reference(text).is_some_and(|(root, _)| self.bodies.resolved_bindings.at(span, Name::intern(root)).is_some()) { return reference; }
                    if reference.is_some() { return reference; }
                }
                if let [ArenaWordPart::Shorthand(expr) | ArenaWordPart::Interpolation(expr)] =
                    parts.as_slice()
                {
                    return self.lower_command_expression(*expr, slots, current_function, item_slot, domain);
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
                                self.lower_command_expression(expr, slots, current_function, item_slot, domain)?,
                                span,
                                None,
                            ));
                        }
                    }
                }
                Some(self.build_command_expression(BuildExprRow::FmtString(lowered), Type::Str, domain))
            }
            ArenaCommandArgKind::SpliceName(name) => {
                self.lower_command_splice_name(*name, self.program.arena.span(arg.span), slots, domain)
            }
            ArenaCommandArgKind::SpliceExpr(expr) => {
                self.lower_command_expression(*expr, slots, current_function, item_slot, domain)
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
                let operands = self.lower_run_operands_in_domain(
                    segment,
                    slots,
                    current_function,
                    item_slot, &mut CommandLoweringDomain::Ordinary)?;
                let run = LoweredSpawnRun {
                    operands,
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
        self.lower_run_binding_value_in_domain(id, slots, current_function, item_slot, &mut CommandLoweringDomain::Ordinary)
    }

    pub(super) fn lower_run_binding_value_in_domain(
        &mut self,
        id: crate::syntax::arena::RunFormId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        domain: &mut CommandLoweringDomain<'_>,
    ) -> Option<BuildExprId> {
        self.lower_run_value_with_propagate_in_domain(
            id,
            slots,
            current_function,
            item_slot,
            true,
            lowered_run_binding_type, domain)
    }

    fn lower_run_status_value_in_domain(
        &mut self,
        id: crate::syntax::arena::RunFormId,
        assert_success: bool,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        domain: &mut CommandLoweringDomain<'_>,
    ) -> Option<BuildExprId> {
        self.lower_run_value_with_propagate_and_assert_in_domain(
            id,
            slots,
            current_function,
            item_slot,
            true,
            assert_success,
            lowered_run_status_type, domain)
    }

    fn lower_run_value_with_propagate_in_domain(
        &mut self,
        id: crate::syntax::arena::RunFormId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        allow_propagate: bool,
        allowed_type: fn(RunKind) -> Option<LoweredType>,
        domain: &mut CommandLoweringDomain<'_>,
    ) -> Option<BuildExprId> {
        self.lower_run_value_with_propagate_and_assert_in_domain(
            id,
            slots,
            current_function,
            item_slot,
            allow_propagate,
            false,
            allowed_type, domain)
    }

    fn lower_run_value_with_propagate_and_assert_in_domain(
        &mut self,
        id: crate::syntax::arena::RunFormId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        allow_propagate: bool,
        assert_success: bool,
        allowed_type: fn(RunKind) -> Option<LoweredType>,
        domain: &mut CommandLoweringDomain<'_>,
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
            let operands = self.lower_run_operands_in_domain(
                segment,
                slots,
                current_function,
                item_slot, domain)?;
            let capture_kind = lowered_run_capture_type(segment.kind).is_some();
            let propagate_internally = run.propagate && !capture_kind;
            let capture = LoweredRunCapture {
                kind: segment.kind,
                operands,
                propagate: propagate_internally,
                assert_success,
                span: self.program.arena.span(run.span),
            };
            let capture_type = Type::run_capture_output(segment.kind, assert_success)?;
            let capture = self.build_command_expression(BuildExprRow::RunCapture(Box::new(capture)), capture_type.clone(), domain);
            if run.propagate && capture_kind {
                let success = match capture_type { Type::Result(success, _) => *success, other => other };
                Some(self.build_command_expression(BuildExprRow::Try { value: capture, span: self.program.arena.span(run.span) }, success, domain))
            } else {
                Some(capture)
            }
        } else {
            let mut lowered_segments = Vec::with_capacity(segments.len());
            for segment in &segments {
                allowed_type(segment.kind)?;
                let operands = self.lower_run_operands_in_domain(
                    segment,
                    slots,
                    current_function,
                    item_slot, domain)?;
                lowered_segments.push(LoweredRunPipelineSegment {
                    kind: segment.kind,
                    operands,
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
            let pipeline_type = Type::run_pipeline_output(segments[0].kind, !capture_kind && (run.propagate || assert_success))?;
            domain.record(pipeline, SchemeType::Ground(pipeline_type.clone()));
            if run.propagate && (capture_kind || !assert_success) {
                let success = match pipeline_type { Type::Result(success, _) => *success, other => other };
                Some(self.build_command_expression(BuildExprRow::Try { value: pipeline, span: self.program.arena.span(run.span) }, success, domain))
            } else {
                Some(pipeline)
            }
        }
    }

    fn lower_run_redirection_in_domain(
        &mut self,
        redirection: &crate::syntax::arena::ArenaRedirection,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        domain: &mut CommandLoweringDomain<'_>,
    ) -> Option<LoweredRunRedirection> {
        let target = match &redirection.target {
            crate::syntax::arena::ArenaRedirectionTarget::Path(target) => {
                self.lower_run_redirection_path_target_in_domain(target, slots, current_function, item_slot, domain)?
            }
            crate::syntax::arena::ArenaRedirectionTarget::Fd(target) => {
                self.lower_run_arg_in_domain(target, slots, current_function, item_slot, domain)?
            }
        };
        Some(LoweredRunRedirection {
            kind: redirection.kind,
            target,
            span: self.program.arena.span(redirection.span),
        })
    }

    fn lower_run_operands_in_domain(
        &mut self,
        segment: &crate::syntax::arena::ArenaRunSegment,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        domain: &mut CommandLoweringDomain<'_>,
    ) -> Option<LoweredRunOperands> {
        let mut options = Vec::with_capacity(3);
        let source_options = self.program.arena.run_options(segment.options).to_vec();
        for option in source_options {
            let value = self.lower_command_expression(option.value, slots, current_function, item_slot, domain)?;
            options.push(match option.kind {
                crate::syntax::grammar::RunOption::Timeout => LoweredRunOption::Timeout(value),
                crate::syntax::grammar::RunOption::CpuMax => LoweredRunOption::CpuMax(value),
                crate::syntax::grammar::RunOption::Accept => LoweredRunOption::Accept(value),
            });
        }
        let source_env = self.program.arena.env_assignments(segment.env).to_vec();
        let mut env = Vec::with_capacity(source_env.len());
        for assignment in &source_env {
            env.push(self.lower_run_env_in_domain(assignment, slots, current_function, item_slot, domain)?);
        }
        let source_target = segment.target.clone();
        let target = self.lower_run_arg_in_domain(&source_target, slots, current_function, item_slot, domain)?;
        let source_tail = self.program.arena.run_tail_operands(segment.tail).to_vec();
        let mut tail = Vec::with_capacity(source_tail.len());
        for operand in source_tail {
            tail.push(match operand {
                crate::syntax::arena::ArenaRunTailOperand::Argument(arg) => {
                    let arg = self.program.arena.command_arg(arg).clone();
                    LoweredRunTailOperand::Argument(self.lower_run_arg_in_domain(
                        &arg,
                        slots,
                        current_function,
                        item_slot, domain)?)
                }
                crate::syntax::arena::ArenaRunTailOperand::Redirection(redirection) => {
                    let redirection = self.program.arena.redirection(redirection).clone();
                    LoweredRunTailOperand::Redirection(self.lower_run_redirection_in_domain(
                        &redirection,
                        slots,
                        current_function,
                        item_slot, domain)?)
                }
            });
        }
        Some(LoweredRunOperands {
            options,
            env,
            target,
            tail,
        })
    }

    fn lower_run_redirection_path_target_in_domain(
        &mut self,
        target: &crate::syntax::arena::ArenaCommandArg,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        domain: &mut CommandLoweringDomain<'_>,
    ) -> Option<LoweredRunArg> {
        if let ArenaCommandArgKind::Word(parts) = &target.kind {
            let span = self.program.arena.span(target.span);
            let parts = self.program.arena.word_parts(*parts).collect::<Vec<_>>();
            if let [ArenaWordPart::Bare(text)] = parts.as_slice() {
                let text = self.bare_text_value_in_span(text, span)?;
                if let Some(binding) = self.bodies.resolved_bindings.at(span, Name::intern(text)) {
                    let slot = slots.resolve_binding(binding)?;
                    let value = push_build_row!(self, expr, BuildExprRow::Param(slot));
                    if matches!(domain, CommandLoweringDomain::Generalized { .. }) { domain.record(value, slots.types.get(&binding)?.clone()); }
                    return Some(LoweredRunArg { kind: LoweredRunArgKind::Single(value), span });
                }
            }
        }
        self.lower_run_arg_in_domain(target, slots, current_function, item_slot, domain)
    }

    fn lower_run_env_in_domain(
        &mut self,
        assignment: &crate::syntax::arena::ArenaEnvAssignment,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        domain: &mut CommandLoweringDomain<'_>,
    ) -> Option<LoweredRunEnv> {
        let value = match &assignment.value {
            crate::syntax::arena::ArenaEnvAssignmentValue::CommandArg(arg) => {
                if matches!(
                    arg.kind,
                    ArenaCommandArgKind::SpliceName(_) | ArenaCommandArgKind::SpliceExpr(_)
                ) {
                    return None;
                }
                self.lower_run_arg_in_domain(arg, slots, current_function, item_slot, domain)?
            }
            crate::syntax::arena::ArenaEnvAssignmentValue::Expr(expr) => LoweredRunArg {
                kind: LoweredRunArgKind::Single(self.lower_command_expression(
                    *expr,
                    slots,
                    current_function,
                    item_slot, domain)?),
                span: self.program.arena.expr(*expr).span,
            },
        };
        Some(LoweredRunEnv {
            name: assignment.name,
            value,
        })
    }

    fn lower_run_arg_in_domain(
        &mut self,
        arg: &crate::syntax::arena::ArenaCommandArg,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        domain: &mut CommandLoweringDomain<'_>,
    ) -> Option<LoweredRunArg> {
        let span = self.program.arena.span(arg.span);
        let kind =
            match &arg.kind {
                ArenaCommandArgKind::Typed(expr) => LoweredRunArgKind::Single(self.lower_command_expression(
                    *expr,
                    slots,
                    current_function,
                    item_slot, domain)?),
                ArenaCommandArgKind::Word(parts) => {
                    let parts = self.program.arena.word_parts(*parts).collect::<Vec<_>>();
                    if let [ArenaWordPart::Shorthand(expr) | ArenaWordPart::Interpolation(expr)] =
                        parts.as_slice()
                    {
                        LoweredRunArgKind::SingleOrSplice(self.lower_command_expression(
                            *expr,
                            slots,
                            current_function,
                            item_slot, domain)?)
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
                                        self.lower_command_expression(expr, slots, current_function, item_slot, domain)?,
                                        self.program.arena.expr(expr).span,
                                        None,
                                    ));
                                }
                            }
                        }
                        LoweredRunArgKind::Single(self.build_command_expression(BuildExprRow::PathFmtString { parts: lowered, span }, Type::Path, domain))
                    }
                }
                ArenaCommandArgKind::SpliceName(name) => LoweredRunArgKind::Splice(
                    self.lower_command_splice_name(*name, self.program.arena.span(arg.span), slots, domain)?,
                ),
                ArenaCommandArgKind::SpliceExpr(expr) => LoweredRunArgKind::Splice(
                    self.lower_command_expression(*expr, slots, current_function, item_slot, domain)?,
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
                            call_plan: None,
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
                        call_plan: None,
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
                                call_plan: None,
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
                        || self
                            .program
                            .arena
                            .run_tail_operands(segment.tail)
                            .iter()
                            .any(|operand| matches!(
                                operand,
                                crate::syntax::arena::ArenaRunTailOperand::Redirection(_)
                            ))
                    {
                        return None;
                    }
                    let operands = self.lower_run_operands_in_domain(
                        segment,
                        slots,
                        current_function,
                        item_slot, &mut CommandLoweringDomain::Ordinary)?;
                    lowered.push(LoweredProcessCommandBuilderEntry::Run {
                        operands,
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
                call_plan: None,
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
