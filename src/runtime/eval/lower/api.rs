use super::{
    Arc, ArenaCallArg, ArenaCallArgKind, BuildExprId, CallArgument, CallableParamType, CheckedApiArguments,
    CheckedApiCall, CompactLowerConstructProbe, ExprId, LoweredArchiveTarCreateArgs,
    LoweredFsFilesArgs, LoweredFsListArgs, LoweredHashVerifyFileArgs, LoweredModuleCallArgs,
    LoweredPathMkdirArgs, LoweredPathRemoveArgs, LoweredPathWriteArgs,
    LoweredProcessCommandArgvArgs, LoweredType, ModuleFnSig, Name, RuntimeOp, SlotScope, Type,
    lowered_checked_type, lowered_method_name,
};

pub(super) fn positional_call_args(args: &[ArenaCallArg]) -> Option<Vec<ExprId>> {
    let mut positional = Vec::with_capacity(args.len());
    for arg in args {
        let ArenaCallArgKind::Positional(expr) = arg.kind else {
            return None;
        };
        positional.push(expr);
    }
    Some(positional)
}

pub(super) fn call_positional_arguments(args: &[CallArgument]) -> Option<Vec<CallArgument>> {
    args.iter()
        .all(|arg| arg.name.is_none() && !arg.splice)
        .then(|| args.to_vec())
}

pub(super) fn single_positional_arena_call_arg(args: &[ArenaCallArg]) -> Option<ExprId> {
    let [arg] = args else {
        return None;
    };
    let ArenaCallArgKind::Positional(value) = arg.kind else {
        return None;
    };
    Some(value)
}

pub(super) fn lowered_str_byte_op(name: &str, args: &[BuildExprId]) -> bool {
    match name {
        "byte_len" => args.is_empty(),
        "byte_at" => args.len() == 1,
        _ => false,
    }
}

/// The checker's public-overload binding applies to a script implementation
/// that shares its parameters. A specialized form such as `hash.verify_file`
/// does not and lowers through its own path.
pub(super) fn script_argument_slots<'p>(
    plan: &'p CheckedApiCall,
    params: &[CallableParamType],
) -> Option<&'p [usize]> {
    let public = crate::sema::builtin_templates::callable_parameters(plan.sig);
    (public.len() == params.len()
        && public
            .iter()
            .zip(params)
            .all(|(public, param)| public.name == param.name))
    .then_some(plan.argument_slots.as_slice())
}

pub(super) fn lower_process_command_argv_args(
    args: &CheckedApiArguments,
) -> Option<LoweredProcessCommandArgvArgs> {
    Some(LoweredProcessCommandArgvArgs {
        target: args.get("target")?,
        argv: args.get("argv")?,
        cwd: args.get("cwd"),
        env: args.get("env"),
        stdin: args.get("stdin"),
        stdout: args.get("stdout"),
        stderr: args.get("stderr"),
        stdout_append: args.get("stdout_append"),
        stderr_append: args.get("stderr_append"),
        timeout: args.get("timeout"),
        detach: args.get("detach"),
        new_session: args.get("new_session"),
        ignore_hup: args.get("ignore_hup"),
        cpu_max: args.get("cpu_max"),
        accept: args.get("accept"),
        same_group: args.get("same_group"),
    })
}

pub(super) fn lowered_module_call_args(
    module: Name,
    name: Name,
    args: &[CallArgument],
    plan: Option<&CheckedApiCall>,
) -> Option<LoweredModuleCallArgs> {
    let plan = plan?;
    let sig = plan.sig;
    let mut lowered_args = vec![None; sig.params.len()];
    for (arg, &slot) in args.iter().zip(&plan.argument_slots) {
        lowered_args[slot] = Some(arg.clone());
    }
    while lowered_args.last().is_some_and(Option::is_none) {
        lowered_args.pop();
    }
    let op = match (module.as_str().as_str(), name.as_str().as_str()) {
        ("fs", "hardlink") => RuntimeOp::FsHardlink,
        ("mime", "lookup_ext") => RuntimeOp::MimeLookupExt,
        ("mime", "lookup_path") => RuntimeOp::MimeLookupPath,
        _ if sig.script_impl().is_some() || lowered_module_sig_type(sig).is_none() => return None,
        _ => sig.op,
    };
    Some(LoweredModuleCallArgs {
        semantic_rule: sig.semantic_rule,
        op,
        args: lowered_args,
    })
}

pub(super) fn lower_hash_verify_file_args(args: &[CallArgument]) -> Option<LoweredHashVerifyFileArgs> {
    let [first, second] = args else { return None };
    let binds_path = |arg: &CallArgument| arg.name.is_none() || arg.name == Some(Name::intern("path"));
    let (path, checksum) = match (binds_path(first), binds_path(second)) {
        (true, false) => (first, second),
        (false, true) => (second, first),
        _ => return None,
    };
    if path.splice || checksum.splice {
        return None;
    }
    let name = checksum.name?;
    // The name is carried as text so the embedded implementation selects the
    // algorithm; the digest primitive it then calls is chosen by that name.
    let algorithm = match name.as_str().as_str() {
        "md5" => "md5",
        "sha1" => "sha1",
        "sha256" => "sha256",
        "sha512" => "sha512",
        _ => return None,
    };
    Some(LoweredHashVerifyFileArgs {
        path: path.clone(),
        algorithm,
        expected: checksum.clone(),
    })
}

pub(super) fn compact_call_arg_expr(arg: &ArenaCallArg) -> Option<ExprId> {
    match arg.kind {
        ArenaCallArgKind::Positional(expr) | ArenaCallArgKind::Named { value: expr, .. } => {
            Some(expr)
        }
        ArenaCallArgKind::Splice { .. } | ArenaCallArgKind::NamedSpread { .. } => None,
    }
}

fn lowered_module_sig_type(sig: &ModuleFnSig) -> Option<LoweredType> {
    if matches!(sig.op, RuntimeOp::TimeSleep | RuntimeOp::TimeMeasure) {
        return lowered_module_op_supported(sig.op).then_some(LoweredType::Result);
    }
    lowered_module_op_supported(sig.op).then(|| lowered_checked_type(&sig.return_ty))?
}

fn lowered_module_op_supported(op: RuntimeOp) -> bool {
    super::super::Evaluator::lowered_module_op_supported(op)
}

pub(super) fn lower_archive_tar_create_args(
    args: &CheckedApiArguments,
) -> Option<LoweredArchiveTarCreateArgs> {
    Some(LoweredArchiveTarCreateArgs {
        path: args.get("path")?,
        root: args.get("root")?,
        entries: args.get("entries")?,
        compression: args.get("compression"),
        overwrite: args.get("overwrite"),
    })
}

pub(super) fn lower_path_mkdir_args(args: &CheckedApiArguments) -> LoweredPathMkdirArgs {
    LoweredPathMkdirArgs {
        parents: args.get("parents"),
    }
}

pub(super) fn lower_path_remove_args(args: &CheckedApiArguments) -> LoweredPathRemoveArgs {
    LoweredPathRemoveArgs {
        missing_ok: args.get("missing_ok"),
    }
}

pub(super) fn lower_path_write_args(args: &CheckedApiArguments) -> Option<LoweredPathWriteArgs> {
    Some(LoweredPathWriteArgs {
        data: args.get("data")?,
    })
}

pub(super) fn lower_fs_files_args(args: &CheckedApiArguments) -> Option<LoweredFsFilesArgs> {
    Some(LoweredFsFilesArgs {
        root: args.get("path")?,
        gitignore: args.get("gitignore"),
        stat: args.get("stat"),
        hidden: args.get("hidden"),
        exts: args.get("exts"),
    })
}

pub(super) fn lower_fs_list_args(args: &CheckedApiArguments) -> Option<LoweredFsListArgs> {
    Some(LoweredFsListArgs {
        path: args.get("path")?,
        stat: args.get("stat"),
        ordered: args.get("ordered"),
    })
}

impl<'p> CompactLowerConstructProbe<'p, '_> {
    pub(super) fn lower_module_argument_values(
        &mut self,
        args: &[Option<CallArgument>],
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<Option<BuildExprId>>> {
        args.iter()
            .map(|arg| match arg {
                Some(arg) => self
                    .lower_call_argument(arg, slots, current_function, item_slot)
                    .map(Some),
                None => Some(None),
            })
            .collect()
    }

    /// The checker's selected overload, with argument expressions in parameter
    /// slot order and their concrete parameter types.
    pub(super) fn checked_method_call_args(
        &self,
        call: ExprId,
        args: &[CallArgument],
    ) -> Option<Vec<(CallArgument, Type)>> {
        let plan = self
            .bodies
            .api_calls
            .get(&call)
            .filter(|plan| plan.receiver.is_some())?;
        let mut values = vec![None; plan.params.len()];
        for (arg, &slot) in args.iter().zip(&plan.argument_slots) {
            values[slot] = Some((arg.clone(), plan.params[slot].clone()));
        }
        Some(values.into_iter().flatten().collect())
    }

    pub(super) fn module_call_plan(&self, call: ExprId) -> Option<&'p CheckedApiCall> {
        self.bodies
            .api_calls
            .get(&call)
            .filter(|plan| plan.receiver.is_none())
    }

    pub(super) fn checked_path_method_arguments(
        &self,
        call: ExprId,
        args: &[CallArgument],
    ) -> Option<CheckedApiArguments> {
        self.bodies
            .api_calls
            .get(&call)
            .filter(|plan| plan.receiver == Some(crate::modules::MethodReceiver::Path))
            .and_then(|_| self.checked_api_arguments(call, args))
    }

    /// The data and mode of a checked `PATH.write(data, mode)`. A write with
    /// a mode is a separate overload and lowers as an ordinary operation
    /// call with the receiver first; only the two-operand write has its own
    /// instruction.
    pub(super) fn path_write_mode_args(
        &self,
        call: ExprId,
        args: &[CallArgument],
    ) -> Option<(CallArgument, CallArgument)> {
        let checked = self.checked_path_method_arguments(call, args)?;
        Some((checked.get("data")?, checked.get("mode")?))
    }

    /// The operands of a checked `PATH.chmod(mode)` or
    /// `PATH.chmod(mode, follow_symlinks: B)` after the receiver, in parameter
    /// order. The name is shared with the rooted method, which is dispatched
    /// by name, so the `Path` method cannot take the route of a method whose
    /// name lowering does not know and is given its operation here.
    pub(super) fn path_chmod_operands(
        &self,
        call: ExprId,
        args: &[CallArgument],
    ) -> Option<Vec<Option<CallArgument>>> {
        let checked = self.checked_path_method_arguments(call, args)?;
        (checked.sig.op == RuntimeOp::FsChmod).then(|| checked.ordered())
    }

    /// The module operation a checked `Path` method runs as when it has no
    /// lowering of its own, with its operands after the receiver in parameter
    /// order. The registry gives such a method the operation of the function
    /// that takes the path first, so a method added there needs only that
    /// operation to be executable.
    pub(super) fn path_method_module_operands(
        &self,
        call: ExprId,
        name: Name,
        args: &[CallArgument],
    ) -> Option<(RuntimeOp, Vec<Option<CallArgument>>)> {
        if lowered_method_name(&name.as_str()) {
            return None;
        }
        let checked = self.checked_path_method_arguments(call, args)?;
        let op = checked.sig.op;
        lowered_module_op_supported(op).then(|| (op, checked.ordered()))
    }

    pub(super) fn checked_api_arguments(
        &self,
        call: ExprId,
        args: &[CallArgument],
    ) -> Option<CheckedApiArguments> {
        let plan = self.bodies.api_calls.get(&call)?;
        if plan.argument_slots.len() != args.len() {
            return None;
        }
        let mut values = vec![None; plan.sig.params.len()];
        for (arg, &slot) in args.iter().zip(&plan.argument_slots) {
            if values
                .get_mut(slot)?
                .replace(arg.clone())
                .is_some()
            {
                return None;
            }
        }
        Some(CheckedApiArguments {
            sig: plan.sig,
            values,
        })
    }

    pub(super) fn prepared_cli_plan(
        &self,
        call: &LoweredModuleCallArgs,
        args: &[CallArgument],
    ) -> Option<Arc<crate::runtime::eval::call_plan::ModuleCallPlan>> {
        if call.semantic_rule == crate::modules::signature::SemanticRule::CliCommands {
            let index = if call.args.len() == 2 { 1 } else { 2 };
            let commands = call.args.get(index)?.as_ref()?.source;
            let fallback = call
                .args
                .get(3)
                .and_then(|argument| argument.as_ref())
                .map(|argument| argument.source);
            return self
                .declarations
                .prepared_constants
                .cli_commands_plan(&self.program.arena, commands, fallback)?
                .ok()
                .map(|plan| Arc::new(super::super::ModuleCallPlan::SignatureCli(plan)));
        }
        if call.semantic_rule != crate::modules::signature::SemanticRule::CliDescriptor {
            return None;
        }
        let schema = call_descriptor_argument(args)?.source;
        self.declarations
            .prepared_constants
            .cli_descriptor_source_plan(
                &self.program.arena,
                schema,
                call.op == RuntimeOp::CliApplet,
            )?
            .ok()
            .map(|plan| Arc::new(super::super::ModuleCallPlan::SignatureCli(plan)))
    }
}

fn call_descriptor_argument(args: &[CallArgument]) -> Option<CallArgument> {
    let mut occupied = [false; 4];
    let mut positional = 0;
    let mut schema = None;
    for arg in args {
        if arg.splice {
            return None;
        }
        let slot = if let Some(name) = arg.name {
            ["argv", "schema", "env", "command"]
                .iter()
                .position(|field| name.as_str().as_str() == *field)?
        } else {
            while occupied.get(positional) == Some(&true) {
                positional += 1;
            }
            let slot = positional;
            positional += 1;
            slot
        };
        if let Some(occupied) = occupied.get_mut(slot) {
            *occupied = true;
        }
        if slot == 1 {
            schema = Some(arg.clone());
        }
    }
    schema
}
