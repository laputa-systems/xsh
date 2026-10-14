use super::{
    Arc, ArenaCallArgKind, ArenaExprKind, BuildExprId, BuildExprRow, CallArgument, CallableParamType,
    CompactLowerConstructProbe, ExprId, FxHashMap, FxHashSet, LoweredCallArg, LoweredFunctionKey,
    CheckedFunctionTarget, LoweredHashVerifyFileArgs, LoweredProcessCommandArgv, LoweredRecordEntry,
    LoweredStrPredicate,
    LoweredTypeCheck,
    ModuleExportType, Name, QualifiedName, RuntimeOp, SlotScope, Span, StdlibLowerLinkage, Type,
    api_spec, call_positional_arguments, is_env_module_expr, lower_archive_tar_create_args,
    lower_fs_files_args, lower_fs_list_args, lower_hash_verify_file_args, lower_path_remove_args,
    lower_path_mkdir_args, lower_path_write_args, lower_process_command_argv_args, lowered_method_name,
    lowered_module_call_args, lowered_str_byte_op, script_argument_slots,
};

impl<'p> CompactLowerConstructProbe<'p, '_> {
    fn lower_checked_function_call(
        &mut self,
        id: ExprId,
        args: &[CallArgument],
        target: CheckedFunctionTarget,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let function = self.function_index().checked_target_key(target)?;
        let params = self.compact_function_sig_for_key(function)?.params.clone();
        let lowered_args = self.lower_function_call_args(
            args,
            Some(&params),
            self.call_argument_slots(id),
            Some((function, 0)),
            slots,
            current_function,
            item_slot,
        )?;
        let span = self.program.arena.expr(id).span;
        self.lower_checked_function_call_values(
            function,
            lowered_args,
            span,
            current_function,
        )
    }

    fn lower_checked_function_call_values(
        &mut self,
        function: LoweredFunctionKey,
        lowered_args: Vec<LoweredCallArg>,
        span: Span,
        current_function: Option<Name>,
    ) -> Option<BuildExprId> {
        let self_call = current_function.is_some_and(|name| match function {
            LoweredFunctionKey::Name(target) => self.current_namespace.is_none() && target == name,
            LoweredFunctionKey::Qualified(target) => {
                self.current_namespace == Some(target.namespace) && target.member == name
            }
        });
        if let Some(op) = self.compact_bridge_op(function) {
            let args = lowered_args
                .into_iter()
                .map(|arg| match arg {
                    LoweredCallArg::Single(expr) => Some(expr),
                    LoweredCallArg::Splice(_) | LoweredCallArg::Default(_) => None,
                })
                .collect::<Option<Vec<_>>>()?;
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::ModuleCall {
                    call_plan: None,
                    op,
                    args: args.into_iter().map(Some).collect(),
                    span,
                }
            ));
        }
        let row = if self_call {
            BuildExprRow::SelfCall { args: lowered_args, span }
        } else if self.compact_direct_pure_call_candidate(function) {
            BuildExprRow::DirectPureCall { function, args: lowered_args, span }
        } else {
            BuildExprRow::Call { function, args: lowered_args, span }
        };
        let lowered = push_build_row!(self, expr, row);
        self.scratch
            .borrow_mut()
            .record_checked_call_target(lowered, function)
            .then_some(lowered)
    }

    pub(super) fn lower_stage_callable_call(
        &mut self, callee: ExprId, item: BuildExprId, slots: &mut SlotScope,
        current_function: Option<Name>,
    ) -> Option<BuildExprId> {
        let span = self.program.arena.expr(callee).span;
        let args = vec![LoweredCallArg::Single(item)];
        if let Some(alias) = self.declarations.static_callable_aliases.get(&span) {
            let callee = match self.program.arena.expr(callee).kind {
                ArenaExprKind::Field { base, .. } if alias.method_call => base,
                _ => callee,
            };
            let callee = self.lower_expr(callee, slots, current_function, None)?;
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::DynamicCall { callee, args, span }
            ));
        }
        if let Some(target) = self.bodies.selected_function_calls.get(&callee).copied() {
            let function = self.function_index().checked_target_key(target)?;
            return self.lower_checked_function_call_values(
                function,
                args,
                span,
                current_function,
            );
        }
        if let Some(plan) = self.module_call_plan(callee) {
            if let Some(script) = plan.sig.script_impl() {
                let namespace = self.internal_namespace(script.module)?;
                let function = QualifiedName::new(namespace, Name::intern(script.function));
                if !self.compact_qualified_function_available(function)
                    && self.stdlib_linkage != StdlibLowerLinkage::External { return None; }
                if self.stdlib_linkage == StdlibLowerLinkage::External {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ExternalCall { function, args, span }
                    ));
                } else {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Call {
                            function: LoweredFunctionKey::Qualified(function),
                            args,
                            span,
                        }
                    ));
                }
            } else {
                let [slot] = plan.argument_slots.as_slice() else { return None };
                let mut values = vec![None; plan.params.len()];
                values[*slot] = Some(item);
                while values.last().is_some_and(Option::is_none) { values.pop(); }
                return Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ModuleCall {
                        call_plan: None,
                        op: plan.sig.op,
                        args: values,
                        span,
                    }
                ));
            }
        }
        None
    }

    /// `env.PATH` methods act on the runtime environment overlay, so a view
    /// held in a binding is evaluated only for its effects.
    fn lower_env_path_method_call(
        &mut self,
        call: ExprId,
        callee: ExprId,
        args_vec: &[CallArgument],
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let ArenaExprKind::Field { base, .. } = self.program.arena.expr(callee).kind else {
            return None;
        };
        let plan =
            self.bodies.api_calls.get(&call).filter(|plan| {
                plan.receiver == Some(crate::modules::MethodReceiver::EnvPathList)
            })?;
        let arguments = self
            .checked_api_arguments(call, args_vec)?
            .ordered()
            .into_iter()
            .collect::<Option<Vec<_>>>()?;
        let mut bindings = Vec::new();
        if !matches!(
            self.program.arena.expr(base).kind,
            ArenaExprKind::EnvPathList
        ) && !is_env_module_expr(&self.program.arena, base)
        {
            let receiver = self.lower_expr(base, slots, current_function, item_slot)?;
            bindings.push((receiver, slots.reserve("environment path receiver")));
        }
        let args = arguments
            .iter()
            .map(|argument| {
                self.lower_call_argument(argument, slots, current_function, item_slot)
                    .map(Some)
            })
            .collect::<Option<Vec<_>>>()?;
        let value = push_build_row!(
            self,
            expr,
            BuildExprRow::ModuleCall {
                call_plan: None,
                op: plan.sig.op,
                args,
                span
            }
        );
        Some(self.wrap_argument_bindings(value, bindings, span))
    }

    /// Route a script-backed method call to its prepared implementation.
    ///
    /// The receiver becomes the implementation function's first argument, so
    /// the embedded body sees exactly the parameters the method signature
    /// declares plus the receiver it was invoked on.
    fn lower_script_method_call(
        &mut self,
        call: ExprId,
        base: ExprId,
        name: Name,
        args: &[CallArgument],
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let script = api_spec().script_method_impl(&name.as_str())?;
        let namespace = self.internal_namespace(script.module)?;
        let function = Name::intern(script.function);
        let qualified = QualifiedName::new(namespace, function);
        if !self.compact_qualified_function_available(qualified)
            && self.stdlib_linkage != StdlibLowerLinkage::External
        {
            return None;
        }
        // The method signature excludes the receiver; the implementation
        // function declares it first, so bind the declared parameters against
        // the arguments after the leading receiver slot.
        let params = self
            .compact_qualified_function_sig(namespace, function)
            .map(|sig| {
                sig.params
                    .iter()
                    .skip(1)
                    .cloned()
                    .collect::<Vec<CallableParamType>>()
            });
        let mut lowered = vec![LoweredCallArg::Single(self.lower_expr(
            base,
            slots,
            current_function,
            item_slot,
        )?)];
        lowered.extend(
            self.lower_function_call_args(
                args,
                params.as_deref(),
                self.bodies
                    .api_calls
                    .get(&call)
                    .filter(|plan| plan.receiver.is_some())
                    .and_then(|plan| script_argument_slots(plan, params.as_deref()?)),
                Some((LoweredFunctionKey::Qualified(qualified), 1)),
                slots,
                current_function,
                item_slot,
            )?,
        );
        if self.stdlib_linkage == StdlibLowerLinkage::External {
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::ExternalCall {
                    function: qualified,
                    args: lowered,
                    span,
                }
            ));
        }
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::Call {
                function: LoweredFunctionKey::Qualified(qualified),
                args: lowered,
                span,
            }
        ))
    }

    /// Lower a specialized `hash.verify_file` call to its embedded
    /// implementation.
    ///
    /// The public form carries its algorithm in the checksum argument's *name*,
    /// and a name is not a value, so the implementation function takes the
    /// algorithm as an explicit third argument rather than mirroring the public
    /// parameters. The file is hashed by the retained digest primitives the
    /// implementation selects by that name, and everything after the digest —
    /// length and hexadecimal validation, comparison, and the failure
    /// composition — is the embedded function's own work.
    fn lower_hash_verify_file_call(
        &mut self,
        options: &LoweredHashVerifyFileArgs,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        span: Span,
    ) -> Option<BuildExprId> {
        let namespace = self.internal_namespace("hash")?;
        let function = QualifiedName::new(namespace, Name::intern("verify_file"));
        let path = self.lower_call_argument(&options.path, slots, current_function, item_slot)?;
        let expected = self.lower_call_argument(&options.expected, slots, current_function, item_slot)?;
        let algorithm = push_build_row!(self, expr, BuildExprRow::Str(options.algorithm.into()));
        let args = vec![
            LoweredCallArg::Single(path),
            LoweredCallArg::Single(expected),
            LoweredCallArg::Single(algorithm),
        ];
        if self.stdlib_linkage == StdlibLowerLinkage::External {
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::ExternalCall {
                    function,
                    args,
                    span,
                }
            ));
        }
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::Call {
                function: LoweredFunctionKey::Qualified(function),
                args,
                span,
            }
        ))
    }

    /// Route a public entry whose implementation is embedded XSH to the
    /// prepared implementation function.
    ///
    /// Returns `None` for native entries, for a spelling the registry does not
    /// bind to a script, and when the implementation module was not prepared —
    /// the latter is a preparation defect that surfaces as a missing-target
    /// diagnostic rather than as a silent fallback to a deleted native body.
    fn lower_script_module_call(
        &mut self,
        call: ExprId,
        args: &[CallArgument],
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        // The checker's selected overload routes each form of a public name to
        // its own implementation function.
        let script = self.module_call_plan(call)?.sig.script_impl()?;
        let namespace = self.internal_namespace(script.module)?;
        let function = Name::intern(script.function);
        let qualified = QualifiedName::new(namespace, function);
        if !self.compact_qualified_function_available(qualified)
            && self.stdlib_linkage != StdlibLowerLinkage::External
        {
            // No prepared implementation and no loading program to provide one:
            // this is a preparation defect, so do not quietly fall back.
            return None;
        }
        let params = self
            .compact_qualified_function_sig(namespace, function)
            .map(|sig| sig.params.clone());
        let args = self
            .lower_function_call_args(
                args,
                params.as_deref(),
                self.module_call_plan(call)
                    .and_then(|plan| script_argument_slots(plan, params.as_deref()?)),
                Some((LoweredFunctionKey::Qualified(qualified), 0)),
                slots,
                current_function,
                item_slot,
            )?
            .into_iter()
            .collect();
        if self.stdlib_linkage == StdlibLowerLinkage::External {
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::ExternalCall {
                    function: qualified,
                    args,
                    span,
                }
            ));
        }
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::Call {
                function: LoweredFunctionKey::Qualified(qualified),
                args,
                span,
            }
        ))
    }

    /// The private representation operation a declared bridge function lowers
    /// to, when this call is inside that function's own implementation module.
    ///
    /// The owner test is what keeps the bridge narrow: a call is rewritten
    /// only from the module that declares the bridge, so no other embedded
    /// module and no user source can reach the operation even if a spelling
    /// collides.
    fn compact_bridge_op(&self, key: LoweredFunctionKey) -> Option<RuntimeOp> {
        let LoweredFunctionKey::Qualified(qualified) = key else {
            return None;
        };
        if self.current_namespace != Some(qualified.namespace) {
            return None;
        }
        let module = crate::stdlib::find_by_namespace(&qualified.namespace.as_str())?;
        let function = qualified.member.as_str();
        crate::stdlib::bridge_op(module, function.as_str())
    }

    /// The interned namespace of an embedded implementation module, when this
    /// program can reach it.
    ///
    /// A program that prepared the module has its functions among the checked
    /// declarations. A program linked to a loading program's prepared modules
    /// has none of its own, so the namespace is accepted whenever the
    /// implementation catalog defines it.
    fn internal_namespace(&self, identity: &str) -> Option<Name> {
        crate::stdlib::find(identity)?;
        let text = crate::stdlib::namespace_text(identity);
        let namespace = Name::intern(text);
        if self.stdlib_linkage == StdlibLowerLinkage::External {
            return Some(namespace);
        }
        self.declarations
            .qualified_pures
            .keys()
            .chain(self.declarations.qualified_procs.keys())
            .chain(self.declarations.qualified_streams.keys())
            .any(|qualified| qualified.namespace == namespace)
            .then_some(namespace)
    }

    pub(super) fn lower_call(
        &mut self,
        id: ExprId,
        callee: ExprId,
        args: crate::syntax::arena::ArenaRange,
        prepared_arguments: Option<&[CallArgument]>,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let span = self.program.arena.expr(id).span;
        let source_args = self.program.arena.call_args(args);
        if prepared_arguments.is_none()
            && source_args.iter().any(|arg| {
                matches!(
                    arg.kind,
                    ArenaCallArgKind::NamedSpread { .. } | ArenaCallArgKind::Named { .. }
                )
            })
        {
            return self.lower_named_spread_call(
                id,
                callee,
                args,
                slots,
                current_function,
                item_slot,
            );
        }
        let args_vec = match prepared_arguments {
            Some(arguments) => std::borrow::Cow::Borrowed(arguments),
            None => std::borrow::Cow::Owned(self.source_call_arguments(source_args)?),
        };
        if let Some(target) = self.bodies.selected_function_calls.get(&id).copied() {
            return self.lower_checked_function_call(
                id,
                &args_vec,
                target,
                slots,
                current_function,
                item_slot,
            );
        }
        // The checker decided this callee is a value of a callable type and
        // bound the arguments to that type's parameters. Named entries were
        // evaluated in source order above, as for a call by name, so slot
        // order is free here.
        if let Some(callable) = self.bodies.typed_callable_calls.get(&id).cloned() {
            let Some(Type::Callable(checked_callee_type)) =
                self.bodies.expr_types.get(&callee).cloned()
            else {
                return None;
            };
            if checked_callee_type.as_ref() != callable.as_ref() {
                return None;
            }
            let args = self.lower_function_call_args(
                &args_vec,
                Some(&callable.sig.params),
                self.call_argument_slots(id),
                None,
                slots,
                current_function,
                item_slot,
            );
            let args = args?;
            let callee = self.lower_expr(callee, slots, current_function, item_slot)?;
            let typed_call = push_build_row!(
                self,
                expr,
                BuildExprRow::TypedCall {
                    callee,
                    pure: callable.pure,
                    signature: Type::Callable(callable),
                    args,
                    span,
                }
            );
            return Some(typed_call);
        }
        if let Some(alias) = self
            .declarations
            .static_callable_aliases
            .get(&self.program.arena.expr(callee).span)
            .cloned()
        {
            let args = self.lower_function_call_args(
                &args_vec,
                Some(&alias.signature.params),
                self.call_argument_slots(id),
                None,
                slots,
                current_function,
                item_slot,
            )?;
            let callee = match self.program.arena.expr(callee).kind {
                ArenaExprKind::Field { base, .. } if alias.method_call => base,
                _ => callee,
            };
            let callee = self.lower_expr(callee, slots, current_function, item_slot)?;
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::DynamicCall { callee, args, span }
            ));
        }
        if let ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } = self.program.arena.expr(callee).kind
            && self.checked_expr_type(base)
                .or_else(|| self.concrete_checked_type(base)).is_some_and(|ty| {
                ty == Type::FsRoot
                    || matches!(ty, Type::Result(ref inner, _) if **inner == Type::FsRoot)
                    || matches!(ty, Type::Optional(ref inner) if **inner == Type::FsRoot && slots.postfix_receivers.contains_key(&base))
            })
        {
            let plan = self.bodies.api_calls.get(&id).filter(|plan| plan.receiver.is_some())?;
            let mut order = vec![None; plan.params.len()];
            for (index, &slot) in plan.argument_slots.iter().enumerate() { order[slot] = Some(index); }
            let receiver = if matches!(self.checked_expr_type(base), Some(Type::Result(_, _))) {
                self.lower_postfix_receiver(base, slots, current_function, item_slot)?
            } else { self.lower_expr(base, slots, current_function, item_slot)? };
            let receiver_slot = slots.reserve("filesystem root receiver");
            let mut bindings = vec![(receiver, receiver_slot)];
            let mut evaluated = Vec::with_capacity(args_vec.len());
            // Evaluate the receiver and argument entries in source order before
            // arranging host slots. Named arguments never reorder effects.
            for arg in args_vec.iter() {
                let value = self.lower_call_argument(arg, slots, current_function, item_slot)?;
                let slot = slots.reserve("filesystem root argument");
                bindings.push((value, slot));
                evaluated.push(push_build_row!(self, expr, BuildExprRow::Param(slot)));
            }
            let mut arguments = vec![Some(push_build_row!(self, expr, BuildExprRow::Param(receiver_slot)))];
            arguments.extend(order.into_iter().map(|argument| argument.map(|index| evaluated[index])));
            let call = push_build_row!(self, expr, BuildExprRow::ModuleCall { call_plan: None, op: plan.sig.op, args: arguments, span });
            return Some(self.wrap_argument_bindings(call, bindings, span));
        }
        if let Some(instance) = self
            .bodies
            .resolved_type_expressions
            .instantiations
            .get(&id)
        {
            let definition = instance.instance.definition;
            let schema_definition = match self.program.arena.type_def(definition).body {
                crate::syntax::arena::ArenaTypeDefBody::RecordSchema(_) => definition,
                crate::syntax::arena::ArenaTypeDefBody::Alias(ty) => self
                    .declarations
                    .record_constructors
                    .resolve_annotation(
                        &self.program.arena,
                        ty,
                        self.declarations.record_constructors.namespace(definition),
                    )?,
                crate::syntax::arena::ArenaTypeDefBody::ModuleContract { .. }
                | crate::syntax::arena::ArenaTypeDefBody::TagUnion(_) => return None,
            };
            let defaults = self
                .declarations
                .record_constructors
                .defaults(schema_definition)
                .cloned()
                .unwrap_or_default();
            let schema = self.bodies.expr_types.get(&id).cloned()?;
            let mut supplied = FxHashSet::default();
            let mut fields = Vec::new();
            for (index, arg) in args_vec.iter().enumerate() {
                if arg.splice {
                    return None;
                }
                let name = arg.name.or_else(|| {
                    self.bodies.record_constructor_fields.get(&id)?.get(index).copied()
                })?;
                supplied.insert(name);
                let literal = match arg.source {
                    crate::sema::arguments::ArgumentValueSource::Expression(value) => {
                        crate::sema::constants::LiteralConstant::analyze(
                            &self.program.arena,
                            value,
                            &FxHashMap::default(),
                        )
                    }
                    crate::sema::arguments::ArgumentValueSource::RecordField { .. }
                    | crate::sema::arguments::ArgumentValueSource::PositionalSplice(_)
                    | crate::sema::arguments::ArgumentValueSource::CheckedType => None,
                };
                let lowered =
                    if let (Some(literal), Type::Record(schema_fields)) = (literal, &schema) {
                        let contextual = literal.clone().in_type(schema_fields.get(&name)?);
                        if contextual != literal {
                            self.lower_record_default(&contextual)?
                        } else {
                            self.lower_call_argument(arg, slots, current_function, item_slot)?
                        }
                    } else {
                        self.lower_call_argument(arg, slots, current_function, item_slot)?
                    };
                fields.push(LoweredRecordEntry::Field(name, lowered));
            }
            for (name, value) in &defaults {
                if !supplied.contains(name) {
                    fields.push(LoweredRecordEntry::Field(
                        *name,
                        self.lower_record_default(&match &schema {
                            Type::Record(fields) => value.clone().in_type(fields.get(name)?),
                            _ => return None,
                        })?,
                    ));
                }
            }
            let value = push_build_row!(self, expr, BuildExprRow::Record(fields));
            let check = LoweredTypeCheck {
                schema: None,
                ty: schema,
                name: match self.program.arena.expr(callee).kind {
                    ArenaExprKind::Ident(name) => name.to_string(),
                    ArenaExprKind::Field { base, name } => match self.program.arena.expr(base).kind
                    {
                        ArenaExprKind::Ident(namespace) => format!("{namespace}.{name}"),
                        _ => return None,
                    },
                    _ => return None,
                }
                .into(),
            };
            let checked = push_build_row!(self, expr, BuildExprRow::Require { value, check, span });
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Try {
                    value: checked,
                    span
                }
            ));
        }
        if let Some((error, bindings)) =
            self.lower_compact_error_expr(id, callee, &args_vec, slots, current_function, item_slot)
        {
            let value = self.push_checked_structured_error(id, error, span)?;
            return Some(self.wrap_argument_bindings(value, bindings, span));
        }
        match self.program.arena.expr(callee).kind {
            ArenaExprKind::Field { base, name } => {
                if let Some(env_call) = self.lower_env_path_method_call(
                    id,
                    callee,
                    &args_vec,
                    span,
                    slots,
                    current_function,
                    item_slot,
                ) {
                    return Some(env_call);
                }
                let statically_resolved = self.bodies.statically_resolved_calls.contains(&id);
                let runtime_module = !statically_resolved
                    && !matches!(self.program.arena.expr(base).kind, ArenaExprKind::Ident(_) if self.source_slot(base, slots).is_none());
                if runtime_module
                    && let Some(Type::Module(exports)) = self.checked_expr_type(base)
                    && let Some(
                        ModuleExportType::Pure { sig, .. } | ModuleExportType::Proc { sig, .. },
                    ) = exports.get(&name)
                {
                    let params = sig.params.clone();
                    let args = self.lower_function_call_args(
                        &args_vec,
                        Some(&params),
                        self.call_argument_slots(id),
                        None,
                        slots,
                        current_function,
                        item_slot,
                    )?;
                    let callee = self.lower_expr(callee, slots, current_function, item_slot)?;
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::DynamicCall { callee, args, span }
                    ));
                }
                if name == "call" && !statically_resolved {
                    let args =
                        self.lower_call_args(&args_vec, slots, current_function, item_slot)?;
                    let call = push_build_row!(
                        self,
                        expr,
                        BuildExprRow::DynamicCall {
                            callee: self.lower_expr(base, slots, current_function, item_slot,)?,
                            args,
                            span,
                        }
                    );
                    // The checker typed `.call` on a dynamic proc handle as
                    // a Result without knowing what the proc returns.
                    if self.checked_expr_type(base) == Some(Type::Proc) {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ProcCallResult(call)
                        ));
                    }
                    return Some(call);
                }
                if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind {
                    if let Some(arity) = self.compact_qualified_tag_variant_arity(module, name) {
                        let positional = call_positional_arguments(&args_vec)?;
                        if positional.len() != arity {
                            return None;
                        }
                        let types = self.compact_tag_variant_field_types(Name::intern(format!(
                            "{module}.{name}"
                        )))?;
                        self.record_tag_pattern_schema(Name::intern(format!("{module}.{name}")))?;
                        let typed = positional.iter().zip(types).map(|(arg, ty)| (arg.clone(), ty.clone())).collect::<Vec<_>>();
                        let (fields, bindings) = self.lower_checked_call_arguments(
                            &typed,
                            slots,
                            current_function,
                            item_slot,
                        )?;
                        let value = push_build_row!(
                            self,
                            expr,
                            BuildExprRow::Tag {
                                type_name: self.compact_tag_type_name(Name::intern(format!(
                                    "{module}.{name}"
                                )))?,
                                wire: self
                                    .compact_tag_wire(Name::intern(format!("{module}.{name}"))),
                                name: Arc::<str>::from(name.as_str().as_str()),
                                fields,
                            }
                        );
                        return Some(self.wrap_argument_bindings(value, bindings, span));
                    }
                    if module.as_str() == "error" && name.as_str() == "fail" {
                        // The checker bound the sole `message` parameter.
                        let [argument] = args_vec.as_ref() else {
                            return None;
                        };
                        let message = self.lower_call_argument(argument, slots, current_function, item_slot)?;
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::Fail {
                                message,
                                span,
                            }
                        ));
                    }
                    if module.as_str() == "error" && name.as_str() == "failure" {
                        // The checker bound the sole `message` parameter.
                        let [argument] = args_vec.as_ref() else {
                            return None;
                        };
                        let message = self.lower_call_argument(argument, slots, current_function, item_slot)?;
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
                                call_plan: None,
                                op: RuntimeOp::ErrorFailure,
                                args: vec![Some(message)],
                                span,
                            }
                        ));
                    }
                    if module == "Path"
                        && name == "parse_bytes"
                        && let [argument] = args_vec.as_ref()
                    {
                        let bytes = self.lower_call_argument(argument, slots, current_function, item_slot)?;
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
                                call_plan: None,
                                op: RuntimeOp::PathParseBytes,
                                args: vec![Some(bytes)],
                                span,
                            }
                        ));
                    }
                    if module == "fs" && name == "children" {
                        let options =
                            lower_fs_list_args(&self.checked_api_arguments(id, &args_vec)?)?;
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::FsList {
                                op: RuntimeOp::FsChildren,
                                path: self.lower_call_argument(
                                    &options.path,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                stat: match options.stat {
                                    Some(stat) => Some(self.lower_call_argument(
                                        &stat,
                                        slots,
                                        current_function,
                                        item_slot,
                                    )?),
                                    None => None,
                                },
                                ordered: match options.ordered {
                                    Some(ordered) => Some(self.lower_call_argument(
                                        &ordered,
                                        slots,
                                        current_function,
                                        item_slot,
                                    )?),
                                    None => None,
                                },
                                span,
                            }
                        ));
                    }
                    if module == "fs" && name == "files" {
                        let options =
                            lower_fs_files_args(&self.checked_api_arguments(id, &args_vec)?)?;
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::FsFiles {
                                root: self.lower_call_argument(
                                    &options.root,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                gitignore: self.lower_optional_call_argument(
                                    options.gitignore.as_ref(),
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                stat: self.lower_optional_call_argument(
                                    options.stat.as_ref(),
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                hidden: self.lower_optional_call_argument(
                                    options.hidden.as_ref(),
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                exts: match options.exts {
                                    Some(exts) => Some(self.lower_call_argument(
                                        &exts,
                                        slots,
                                        current_function,
                                        item_slot,
                                    )?),
                                    None => None,
                                },
                                result_wrapped: true,
                                span,
                            }
                        ));
                    }
                    if module == "fs" && name == "walk" {
                        let options =
                            lower_fs_files_args(&self.checked_api_arguments(id, &args_vec)?)?;
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::FsWalk {
                                root: self.lower_call_argument(
                                    &options.root,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                gitignore: self.lower_optional_call_argument(
                                    options.gitignore.as_ref(),
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                stat: self.lower_optional_call_argument(
                                    options.stat.as_ref(),
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                hidden: self.lower_optional_call_argument(
                                    options.hidden.as_ref(),
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                exts: match options.exts {
                                    Some(exts) => Some(self.lower_call_argument(
                                        &exts,
                                        slots,
                                        current_function,
                                        item_slot,
                                    )?),
                                    None => None,
                                },
                                result_wrapped: true,
                                span,
                            }
                        ));
                    }
                    if module == "fs" && name == "tempdir" && args_vec.is_empty() {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::FsTempDir { span }
                        ));
                    }
                    if module == "archive" && name == "tar_create" {
                        let options = lower_archive_tar_create_args(
                            &self.checked_api_arguments(id, &args_vec)?,
                        )?;
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ArchiveTarCreate {
                                path: self.lower_call_argument(
                                    &options.path,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                root: self.lower_call_argument(
                                    &options.root,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                entries: self.lower_call_argument(
                                    &options.entries,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                compression: match options.compression {
                                    Some(expr) => Some(self.lower_call_argument(
                                        &expr,
                                        slots,
                                        current_function,
                                        item_slot,
                                    )?),
                                    None => None,
                                },
                                overwrite: match options.overwrite {
                                    Some(expr) => Some(self.lower_call_argument(
                                        &expr,
                                        slots,
                                        current_function,
                                        item_slot,
                                    )?),
                                    None => None,
                                },
                                span,
                            }
                        ));
                    }
                    if module == "process" && name == "command_argv" {
                        let options = lower_process_command_argv_args(
                            &self.checked_api_arguments(id, &args_vec)?,
                        )?;
                        let command = LoweredProcessCommandArgv {
                            target: self.lower_call_argument(
                                &options.target,
                                slots,
                                current_function,
                                item_slot,
                            )?,
                            argv: self.lower_call_argument(
                                &options.argv,
                                slots,
                                current_function,
                                item_slot,
                            )?,
                            cwd: match options.cwd {
                                Some(expr) => Some(self.lower_call_argument(
                                    &expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            env: match options.env {
                                Some(expr) => Some(self.lower_call_argument(
                                    &expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            stdin: match options.stdin {
                                Some(expr) => Some(self.lower_call_argument(
                                    &expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            stdout: match options.stdout {
                                Some(expr) => Some(self.lower_call_argument(
                                    &expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            stderr: match options.stderr {
                                Some(expr) => Some(self.lower_call_argument(
                                    &expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            stdout_append: match options.stdout_append {
                                Some(expr) => Some(self.lower_call_argument(
                                    &expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            stderr_append: match options.stderr_append {
                                Some(expr) => Some(self.lower_call_argument(
                                    &expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            timeout: match options.timeout {
                                Some(expr) => Some(self.lower_call_argument(
                                    &expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            detach: match options.detach {
                                Some(expr) => Some(self.lower_call_argument(
                                    &expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            new_session: match options.new_session {
                                Some(expr) => Some(self.lower_call_argument(
                                    &expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            ignore_hup: match options.ignore_hup {
                                Some(expr) => Some(self.lower_call_argument(
                                    &expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            cpu_max: match options.cpu_max {
                                Some(expr) => Some(self.lower_call_argument(
                                    &expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            accept: match options.accept {
                                Some(expr) => Some(self.lower_call_argument(
                                    &expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            same_group: match options.same_group {
                                Some(expr) => Some(self.lower_call_argument(
                                    &expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            span,
                        };
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ProcessCommandArgv(Box::new(command))
                        ));
                    }
                    if let Some(script_call) = self.lower_script_module_call(
                        id,
                        &args_vec,
                        span,
                        slots,
                        current_function,
                        item_slot,
                    ) {
                        return Some(script_call);
                    }
                    if let Some(module_call) =
                        lowered_module_call_args(module, name, &args_vec, self.module_call_plan(id))
                    {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
                                call_plan: self.prepared_cli_plan(&module_call, &args_vec),
                                op: module_call.op,
                                args: self.lower_module_argument_values(
                                    &module_call.args,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                span,
                            }
                        ));
                    }
                }
                if name == "read_text"
                    && args_vec.is_empty()
                    && self.checked_path_method_arguments(id, &args_vec).is_some()
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathReadText {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            span,
                        }
                    ));
                }
                if name == "read_bytes"
                    && args_vec.is_empty()
                    && self.checked_path_method_arguments(id, &args_vec).is_some()
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathReadBytes {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            span,
                        }
                    ));
                }
                if name == "exists"
                    && args_vec.is_empty()
                    && self.checked_path_method_arguments(id, &args_vec).is_some()
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathExists {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            span,
                        }
                    ));
                }
                if name == "executable"
                    && args_vec.is_empty()
                    && self.checked_path_method_arguments(id, &args_vec).is_some()
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathExecutable {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            span,
                        }
                    ));
                }
                if name == "du"
                    && args_vec.is_empty()
                    && self.checked_path_method_arguments(id, &args_vec).is_some()
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathDu {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            span,
                        }
                    ));
                }
                if name == "metadata"
                    && args_vec.is_empty()
                    && self.checked_path_method_arguments(id, &args_vec).is_some()
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathMetadata {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            span,
                        }
                    ));
                }
                if name == "readlink"
                    && args_vec.is_empty()
                    && self.checked_path_method_arguments(id, &args_vec).is_some()
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathReadlink {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            span,
                        }
                    ));
                }
                if name == "resolve"
                    && args_vec.is_empty()
                    && self.checked_path_method_arguments(id, &args_vec).is_some()
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathResolve {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            span,
                        }
                    ));
                }
                if name == "chmod"
                    && let Some(operands) = self.path_chmod_operands(id, &args_vec)
                {
                    let mut args = vec![Some(self.lower_expr(
                        base,
                        slots,
                        current_function,
                        item_slot,
                    )?)];
                    for operand in operands {
                        args.push(match operand {
                            Some(operand) => Some(self.lower_call_argument(
                                &operand,
                                slots,
                                current_function,
                                item_slot,
                            )?),
                            None => None,
                        });
                    }
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ModuleCall {
                            call_plan: None,
                            op: RuntimeOp::FsChmod,
                            args,
                            span,
                        }
                    ));
                }
                if name == "write"
                    && let Some((data, mode)) = self.path_write_mode_args(id, &args_vec)
                {
                    let args = vec![
                        Some(self.lower_expr(base, slots, current_function, item_slot)?),
                        Some(self.lower_call_argument(&data, slots, current_function, item_slot)?),
                        Some(self.lower_call_argument(&mode, slots, current_function, item_slot)?),
                    ];
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ModuleCall {
                            call_plan: None,
                            op: RuntimeOp::FsWrite,
                            args,
                            span,
                        }
                    ));
                }
                if (name == "write" || name == "write_atomic")
                    && let Some(checked) = self.checked_path_method_arguments(id, &args_vec)
                {
                    let options = lower_path_write_args(&checked)?;
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathWrite {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            data: self.lower_call_argument(
                                &options.data,
                                slots,
                                current_function,
                                item_slot,
                            )?,
                            atomic: name == "write_atomic",
                            span,
                        }
                    ));
                }
                if name == "mkdir"
                    && let Some(checked) = self.checked_path_method_arguments(id, &args_vec)
                {
                    let options = lower_path_mkdir_args(&checked);
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathMkdir {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            parents: match options.parents {
                                Some(expr) => Some(self.lower_call_argument(
                                    &expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            span,
                        }
                    ));
                }
                if name == "remove"
                    && let Some(options) = self
                        .checked_path_method_arguments(id, &args_vec)
                        .map(|args| lower_path_remove_args(&args))
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathRemove {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            missing_ok: match options.missing_ok {
                                Some(expr) => Some(self.lower_call_argument(
                                    &expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            span,
                        }
                    ));
                }
                if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind
                    && module == "hash"
                    && name == "verify_file"
                    && let Some(options) = lower_hash_verify_file_args(&args_vec)
                {
                    return self.lower_hash_verify_file_call(
                        &options,
                        slots,
                        current_function,
                        item_slot,
                        span,
                    );
                }
                if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind
                    && let Some(module_call) =
                        lowered_module_call_args(module, name, &args_vec, self.module_call_plan(id))
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ModuleCall {
                            call_plan: self.prepared_cli_plan(&module_call, &args_vec),
                            op: module_call.op,
                            args: self.lower_module_argument_values(
                                &module_call.args,
                                slots,
                                current_function,
                                item_slot,
                            )?,
                            span,
                        }
                    ));
                }
                let positional = call_positional_arguments(&args_vec);
                if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind
                    && let Some(positional) = positional.as_ref()
                {
                    if module == "Path" && name == "parse_bytes" && positional.len() == 1 {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
                                call_plan: None,
                                op: RuntimeOp::PathParseBytes,
                                args: positional
                                    .iter()
                                    .map(|arg| {
                                        self.lower_call_argument(arg, slots, current_function, item_slot)
                                            .map(Some)
                                    })
                                    .collect::<Option<Vec<_>>>()?,
                                span,
                            }
                        ));
                    }
                    if module == "map" && name == "empty" && positional.is_empty() {
                        return Some(push_build_row!(self, expr, BuildExprRow::EmptyMap));
                    }
                    // `set.empty()` and `set.from(items)` build the set the
                    // checker typed them as.
                    if module == "set"
                        && matches!(self.bodies.expr_types.get(&id), Some(Type::Set(_)))
                    {
                        if name == "empty" && positional.is_empty() {
                            return Some(self.list_as_set(Vec::new(), span));
                        }
                        if name == "from" && positional.len() == 1 {
                            let items = self.lower_call_argument(&positional[0], slots, current_function, item_slot)?;
                            return Some(self.set_of_list(items, span));
                        }
                    }
                    if module == "hash"
                        && name == "verify_file"
                        && let Some(options) = lower_hash_verify_file_args(&args_vec)
                    {
                        return self.lower_hash_verify_file_call(
                            &options,
                            slots,
                            current_function,
                            item_slot,
                            span,
                        );
                    }
                    if let Some(script_call) = self.lower_script_module_call(
                        id,
                        &args_vec,
                        span,
                        slots,
                        current_function,
                        item_slot,
                    ) {
                        return Some(script_call);
                    }
                    if let Some(module_call) =
                        lowered_module_call_args(module, name, &args_vec, self.module_call_plan(id))
                    {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
                                call_plan: self.prepared_cli_plan(&module_call, &args_vec),
                                op: module_call.op,
                                args: self.lower_module_argument_values(
                                    &module_call.args,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                span,
                            }
                        ));
                    }
                }
                if statically_resolved
                    || self.source_slot(base, slots).is_none()
                        && matches!(self.checked_expr_type(base), Some(Type::Module(_)))
                {
                    return None;
                }
                if !lowered_method_name(&name.as_str()) {
                    if name == "read_text"
                        && args_vec.is_empty()
                        && self.checked_path_method_arguments(id, &args_vec).is_some()
                    {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::PathReadText {
                                path: self.lower_expr(base, slots, current_function, item_slot,)?,
                                span,
                            }
                        ));
                    }
                    if name == "read_bytes"
                        && args_vec.is_empty()
                        && self.checked_path_method_arguments(id, &args_vec).is_some()
                    {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::PathReadBytes {
                                path: self.lower_expr(base, slots, current_function, item_slot,)?,
                                span,
                            }
                        ));
                    }
                    if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind
                        && let Some(call_args) = lowered_module_call_args(
                            module,
                            name,
                            &args_vec,
                            self.module_call_plan(id),
                        )
                    {
                        let mut lowered = Vec::with_capacity(call_args.args.len());
                        for arg in call_args.args {
                            lowered.push(match arg {
                                Some(arg) => Some(self.lower_call_argument(
                                    &arg,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            });
                        }
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
                                call_plan: None,
                                op: call_args.op,
                                args: lowered,
                                span,
                            }
                        ));
                    }
                    if let Some((op, operands)) =
                        self.path_method_module_operands(id, name, &args_vec)
                    {
                        let mut args = vec![Some(self.lower_expr(
                            base,
                            slots,
                            current_function,
                            item_slot,
                        )?)];
                        for operand in operands {
                            args.push(match operand {
                                Some(operand) => Some(self.lower_call_argument(
                                    &operand,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            });
                        }
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
                                call_plan: None,
                                op,
                                args,
                                span,
                            }
                        ));
                    }
                    // A checked `Path` method that reaches here has no route. A
                    // dynamic call would find no such method at run time, so
                    // the call is refused now.
                    if self.checked_path_method_arguments(id, &args_vec).is_some() {
                        return None;
                    }
                    let args =
                        self.lower_call_args(&args_vec, slots, current_function, item_slot)?;
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::DynamicCall {
                            callee: self.lower_expr(callee, slots, current_function, item_slot,)?,
                            args,
                            span,
                        }
                    ));
                }
                // Recognize `<text>.starts_with(n)` / `.ends_with(n)` and lower to the
                // direct StrPredicate node (the bool-condition path then specializes it
                // into StrPredicateSlot/TrimStrPredicateSlot for slot/trim receivers).
                // A Path receiver compares whole components, so it takes the method
                // call below instead of the byte predicate.
                let str_predicate = match name.as_str().as_str() {
                    _ if matches!(
                        self.checked_expr_type(base).as_ref().map(Type::unvalidated),
                        Some(Type::Path)
                    ) =>
                    {
                        None
                    }
                    "starts_with" if args_vec.len() == 1 => Some(LoweredStrPredicate::StartsWith),
                    "ends_with" if args_vec.len() == 1 => Some(LoweredStrPredicate::EndsWith),
                    _ => None,
                };
                if let Some(predicate) = str_predicate
                    && let Some(positional) = call_positional_arguments(&args_vec)
                    && positional.len() == 1
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::StrPredicate {
                            receiver: self.lower_expr(base, slots, current_function, item_slot,)?,
                            predicate,
                            needle: self.lower_call_argument(
                                &positional[0],
                                slots,
                                current_function,
                                item_slot,
                            )?,
                            span,
                        }
                    ));
                }
                if let Some(script_call) = self.lower_script_method_call(
                    id,
                    base,
                    name,
                    &args_vec,
                    span,
                    slots,
                    current_function,
                    item_slot,
                ) {
                    return Some(script_call);
                }
                let method_args = self.checked_method_call_args(id, &args_vec).or_else(|| {
                    call_positional_arguments(&args_vec)
                        .map(|args| {
                            args.into_iter()
                                .map(|arg| {
                                    let ty = if arg.ty == Type::Unknown {
                                        Type::Any
                                    } else {
                                        arg.ty.clone()
                                    };
                                    (arg, ty)
                                })
                                .collect()
                        })
                })?;
                if !self.lowered_method_supported_for_receiver(base, name, method_args.len(), slots)
                {
                    return None;
                }
                let receiver = self.lower_expr(base, slots, current_function, item_slot)?;
                let mut bindings = Vec::new();
                let uint_key = matches!(self.checked_expr_type(base), Some(Type::Map(key, _)) if *key == Type::UInt);
                let checked = method_args.iter().enumerate().any(|(position, (_, ty))| {
                    ty.has_unsigned_constraint() && !(uint_key && position == 0)
                });
                let (receiver, mut lowered_args) = if checked {
                    let slot = slots.reserve("method receiver");
                    bindings.push((receiver, slot));
                    let receiver = push_build_row!(self, expr, BuildExprRow::Param(slot));
                    let args: Vec<_> = method_args
                        .iter()
                        .enumerate()
                        .map(|(position, (arg, ty))| {
                            (arg.clone(), if uint_key && position == 0 { Type::Int } else { ty.clone() })
                        })
                        .collect();
                    let (values, argument_bindings) = self.lower_checked_call_arguments(
                        &args,
                        slots,
                        current_function,
                        item_slot,
                    )?;
                    bindings.extend(argument_bindings);
                    (receiver, values)
                } else {
                    (
                        receiver,
                            method_args.iter()
                                .map(|(arg, _)| self.lower_call_argument(arg, slots, current_function, item_slot))
                                .collect::<Option<Vec<_>>>()?,
                    )
                };
                if uint_key && let Some(value) = lowered_args.first_mut() {
                    *value = self
                        .require_uint_key(*value, method_args[0].0.span);
                }
                if lowered_str_byte_op(&name.as_str(), &lowered_args) {
                    return Some(match name.as_str().as_str() {
                        "byte_len" => {
                            push_build_row!(self, expr, BuildExprRow::StrByteLen { receiver, span })
                        }
                        "byte_at" => {
                            let mut args = lowered_args.into_iter();
                            push_build_row!(
                                self,
                                expr,
                                BuildExprRow::StrByteAt {
                                    receiver,
                                    index: args.next().unwrap(),
                                    span,
                                }
                            )
                        }
                        _ => unreachable!(),
                    });
                }
                let value = push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Method {
                        receiver,
                        name: name.as_str(),
                        args: lowered_args,
                        span
                    }
                );
                Some(self.wrap_argument_bindings(value, bindings, span))
            }
            ArenaExprKind::NullSafeField { base, name } => {
                if name == "read_text"
                    && args_vec.is_empty()
                    && self.checked_path_method_arguments(id, &args_vec).is_some()
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathReadText {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            span,
                        }
                    ));
                }
                if name == "read_bytes"
                    && args_vec.is_empty()
                    && self.checked_path_method_arguments(id, &args_vec).is_some()
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathReadBytes {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            span,
                        }
                    ));
                }
                if name == "exists"
                    && args_vec.is_empty()
                    && self.checked_path_method_arguments(id, &args_vec).is_some()
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathExists {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            span,
                        }
                    ));
                }
                if name == "executable"
                    && args_vec.is_empty()
                    && self.checked_path_method_arguments(id, &args_vec).is_some()
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathExecutable {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            span,
                        }
                    ));
                }
                if name == "du"
                    && args_vec.is_empty()
                    && self.checked_path_method_arguments(id, &args_vec).is_some()
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathDu {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            span,
                        }
                    ));
                }
                if name == "metadata"
                    && args_vec.is_empty()
                    && self.checked_path_method_arguments(id, &args_vec).is_some()
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathMetadata {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            span,
                        }
                    ));
                }
                if name == "readlink"
                    && args_vec.is_empty()
                    && self.checked_path_method_arguments(id, &args_vec).is_some()
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathReadlink {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            span,
                        }
                    ));
                }
                if name == "resolve"
                    && args_vec.is_empty()
                    && self.checked_path_method_arguments(id, &args_vec).is_some()
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathResolve {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            span,
                        }
                    ));
                }
                if name == "chmod"
                    && let Some(operands) = self.path_chmod_operands(id, &args_vec)
                {
                    let mut args = vec![Some(self.lower_postfix_receiver(
                        base,
                        slots,
                        current_function,
                        item_slot,
                    )?)];
                    for operand in operands {
                        args.push(match operand {
                            Some(operand) => Some(self.lower_call_argument(
                                &operand,
                                slots,
                                current_function,
                                item_slot,
                            )?),
                            None => None,
                        });
                    }
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ModuleCall {
                            call_plan: None,
                            op: RuntimeOp::FsChmod,
                            args,
                            span,
                        }
                    ));
                }
                if name == "write"
                    && let Some((data, mode)) = self.path_write_mode_args(id, &args_vec)
                {
                    let args = vec![
                        Some(self.lower_postfix_receiver(
                            base,
                            slots,
                            current_function,
                            item_slot,
                        )?),
                        Some(self.lower_call_argument(&data, slots, current_function, item_slot)?),
                        Some(self.lower_call_argument(&mode, slots, current_function, item_slot)?),
                    ];
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ModuleCall {
                            call_plan: None,
                            op: RuntimeOp::FsWrite,
                            args,
                            span,
                        }
                    ));
                }
                if (name == "write" || name == "write_atomic")
                    && let Some(checked) = self.checked_path_method_arguments(id, &args_vec)
                {
                    let options = lower_path_write_args(&checked)?;
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathWrite {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            data: self.lower_call_argument(
                                &options.data,
                                slots,
                                current_function,
                                item_slot,
                            )?,
                            atomic: name == "write_atomic",
                            span,
                        }
                    ));
                }
                if name == "mkdir"
                    && let Some(checked) = self.checked_path_method_arguments(id, &args_vec)
                {
                    let options = lower_path_mkdir_args(&checked);
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathMkdir {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            parents: match options.parents {
                                Some(expr) => Some(self.lower_call_argument(
                                    &expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            span,
                        }
                    ));
                }
                if name == "remove"
                    && let Some(options) = self
                        .checked_path_method_arguments(id, &args_vec)
                        .map(|args| lower_path_remove_args(&args))
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathRemove {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            missing_ok: match options.missing_ok {
                                Some(expr) => Some(self.lower_call_argument(
                                    &expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            span,
                        }
                    ));
                }
                if let Some((op, operands)) = self.path_method_module_operands(id, name, &args_vec)
                {
                    let mut args = vec![Some(self.lower_postfix_receiver(
                        base,
                        slots,
                        current_function,
                        item_slot,
                    )?)];
                    for operand in operands {
                        args.push(match operand {
                            Some(operand) => Some(self.lower_call_argument(
                                &operand,
                                slots,
                                current_function,
                                item_slot,
                            )?),
                            None => None,
                        });
                    }
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ModuleCall {
                            call_plan: None,
                            op,
                            args,
                            span,
                        }
                    ));
                }
                // A checked `Path` method without a route is refused here
                // too, instead of becoming a dynamic call that fails when run.
                if !lowered_method_name(&name.as_str())
                    && self.checked_path_method_arguments(id, &args_vec).is_some()
                {
                    return None;
                }
                let method_args = self.checked_method_call_args(id, &args_vec).or_else(|| {
                    call_positional_arguments(&args_vec)
                        .map(|args| {
                            args.into_iter()
                                .map(|arg| {
                                    let ty = if arg.ty == Type::Unknown {
                                        Type::Any
                                    } else {
                                        arg.ty.clone()
                                    };
                                    (arg, ty)
                                })
                                .collect()
                        })
                })?;
                if !lowered_method_name(&name.as_str()) {
                    if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind
                        && let Some(call_args) = lowered_module_call_args(
                            module,
                            name,
                            &args_vec,
                            self.module_call_plan(id),
                        )
                    {
                            let mut lowered = Vec::with_capacity(call_args.args.len());
                        for arg in call_args.args {
                            lowered.push(match arg {
                                    Some(arg) => Some(self.lower_call_argument(
                                        &arg,
                                        slots,
                                        current_function,
                                        item_slot,
                                    )?),
                                None => None,
                            });
                        }
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
                                call_plan: None,
                                op: call_args.op,
                                args: lowered,
                                span,
                            }
                        ));
                    }
                    let args =
                        self.lower_call_args(&args_vec, slots, current_function, item_slot)?;
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::DynamicCall {
                            callee: self.lower_expr(callee, slots, current_function, item_slot,)?,
                            args,
                            span,
                        }
                    ));
                }
                if !self.lowered_method_supported_for_receiver(base, name, method_args.len(), slots)
                {
                    return None;
                }
                let receiver =
                    self.lower_postfix_receiver(base, slots, current_function, item_slot)?;
                let uint_key = matches!(self.checked_expr_type(base), Some(Type::Map(key, _)) if *key == Type::UInt);
                let args: Vec<_> = method_args
                    .iter()
                    .enumerate()
                    .map(|(position, (arg, ty))| {
                        (arg.clone(), if uint_key && position == 0 { Type::Int } else { ty.clone() })
                    })
                    .collect();
                let checked = args.iter().any(|(_, ty)| ty.has_unsigned_constraint());
                let mut bindings = Vec::new();
                let receiver = if checked {
                    let slot = slots.reserve("method receiver");
                    bindings.push((receiver, slot));
                    push_build_row!(self, expr, BuildExprRow::Param(slot))
                } else {
                    receiver
                };
                let (mut lowered_args, argument_bindings) = self.lower_checked_call_arguments(
                    &args,
                    slots,
                    current_function,
                    item_slot,
                )?;
                bindings.extend(argument_bindings);
                if uint_key && let Some(value) = lowered_args.first_mut() {
                    *value = self
                        .require_uint_key(*value, method_args[0].0.span);
                }
                let value = push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Method {
                        receiver,
                        name: name.as_str(),
                        args: lowered_args,
                        span
                    }
                );
                Some(self.wrap_argument_bindings(value, bindings, span))
            }
            ArenaExprKind::Ident(name) => {
                let positional = call_positional_arguments(&args_vec);
                if let Some(arity) = self.compact_tag_variant_arity(name) {
                    let positional = positional.as_ref()?;
                    if positional.len() != arity {
                        return None;
                    }
                    let types = self.compact_tag_variant_field_types(name)?;
                    self.record_tag_pattern_schema(name)?;
                    let typed = positional.iter().zip(types).map(|(arg, ty)| (arg.clone(), ty.clone())).collect::<Vec<_>>();
                    let (fields, bindings) = self.lower_checked_call_arguments(
                        &typed,
                        slots,
                        current_function,
                        item_slot,
                    )?;
                    let value = push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Tag {
                            type_name: self.compact_tag_type_name(name)?,
                            wire: self.compact_tag_wire(name),
                            name: Arc::<str>::from(name.as_str().as_str()),
                            fields,
                        }
                    );
                    return Some(self.wrap_argument_bindings(value, bindings, span));
                }
                if name == "Err" {
                    let argument_slots = self.call_argument_slots(id)?;
                    if args_vec.len() == 1 && args_vec[0].name.is_none() && !args_vec[0].splice {
                        let value = self.lower_call_argument(
                            &args_vec[0],
                            slots,
                            current_function,
                            item_slot,
                        )?;
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::Err { value, cause: None }
                        ));
                    }
                    let mut value = None;
                    let mut cause = None;
                    for (argument, &slot) in args_vec.iter().zip(argument_slots) {
                        let argument = self.lower_call_argument(
                            argument,
                            slots,
                            current_function,
                            item_slot,
                        )?;
                        if slot == 0 {
                            value = Some(argument);
                        } else {
                            cause = Some(argument);
                        }
                    }
                    let result = push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Err {
                            value: value?,
                            cause
                        }
                    );
                    return Some(result);
                }
                if name == "Ok"
                    && let Some(positional) = positional.as_ref()
                {
                    let value = match positional.as_slice() {
                        [] if name == "Ok" => push_build_row!(self, expr, BuildExprRow::Unit),
                        [value] => self.lower_call_argument(value, slots, current_function, item_slot)?,
                        _ => return None,
                    };
                    return Some(push_build_row!(self, expr, BuildExprRow::Ok(value)));
                }
                if name == "range"
                    && self
                        .bodies
                        .checked_control_flow
                        .builtin_range_calls
                        .contains(&span)
                    && let Some(positional) = positional.as_ref()
                {
                    let (start, end) = match positional.as_slice() {
                        [end] => (None, end),
                        [start, end] => (Some(start), end),
                        _ => return None,
                    };
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Range {
                            start: match start {
                                Some(start) => {
                                    self.lower_call_argument(start, slots, current_function, item_slot)?
                                }
                                None => push_build_row!(self, expr, BuildExprRow::Int(0)),
                            },
                            end: self.lower_call_argument(end, slots, current_function, item_slot)?,
                            span,
                        }
                    ));
                }
                if name == "Path"
                    && let Some(positional) = positional.as_ref()
                {
                    let [value] = positional.as_slice() else {
                        return None;
                    };
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathFrom {
                            value: self.lower_call_argument(value, slots, current_function, item_slot,)?,
                            span,
                        }
                    ));
                }
                if name == "env"
                    && let Some(positional) = positional.as_ref()
                {
                    let [value] = positional.as_slice() else {
                        return None;
                    };
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ModuleCall {
                            call_plan: None,
                            op: RuntimeOp::EnvGet,
                            args: vec![Some(self.lower_call_argument(
                                value,
                                slots,
                                current_function,
                                item_slot
                            )?)],
                            span,
                        }
                    ));
                }
                if self.expression_binding(callee).is_some() {
                    if self.bodies.statically_resolved_calls.contains(&id) {
                        return None;
                    }
                    let args =
                        self.lower_call_args(&args_vec, slots, current_function, item_slot)?;
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::DynamicCall {
                            callee: self.lower_bare_ident(callee, name, slots)?,
                            args,
                            span,
                        }
                    ));
                }
                None
            }
            _ => None,
        }
    }
}
