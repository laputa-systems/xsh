use super::{
    ArenaCallArgKind, ArenaExprKind, ArenaStreamStage, BuildExprId, BuildExprRow, BuildStmtId, BuildStmtRow,
    CompactLowerConstructProbe, LoweredPipelineStage, LoweredRecordEntry, Name, ReduceByOp,
    SlotScope, StreamStageKind, Type, cleanup_pipeline_stage_item_slot,
    SchemeBodyFacts, SchemeType, ExprId, FxHashSet, BTreeMap, CallArgument,
    compact_call_arg_expr,
};

impl<'p> CompactLowerConstructProbe<'p, '_> {
    pub(super) fn lower_scheme_pipeline_shape_stage(
        &mut self, expression: ExprId, stage: &ArenaStreamStage, item: SchemeType, slots: &mut SlotScope,
        current_function: Name, facts: &SchemeBodyFacts, visited: &mut FxHashSet<ExprId>,
        lowered_types: &mut BTreeMap<BuildExprId, SchemeType>,
    ) -> Option<(LoweredPipelineStage, SchemeType)> {
        if stage.block.is_some() { return None; }
        let mut configuration = Vec::new();
        let arguments = self.lower_scheme_call_arguments(expression, stage.args, None, slots, current_function, facts, visited, lowered_types, &mut configuration)?;
        let parameters = crate::sema::stage_arguments::stage_argument_params(stage.kind.as_str());
        let binding = self.bodies.argument_bindings.get(&self.program.arena.span(stage.span))?;
        if arguments.len() != binding.argument_slots.len() || binding.callable_entry.is_some() { return None; }
        let mut values = vec![None; parameters.len()];
        for (argument, &slot) in arguments.iter().zip(&binding.argument_slots) {
            if values.get_mut(slot)?.replace(argument.lowered?).is_some() { return None; }
        }
        let span = self.program.arena.span(stage.span);
        let wrap = |this: &mut Self, value, lowered_types: &mut BTreeMap<BuildExprId, SchemeType>| {
            let ty = lowered_types.get(&value)?.clone();
            let mut body = configuration.clone();
            let tail = push_build_row!(this, stmt, super::BuildStmtRow::Value { value });
            if ty.may_hold_resource() { this.scratch.borrow_mut().mark_resource_transfer(tail); }
            body.push(tail);
            let wrapped = push_build_row!(this, expr, BuildExprRow::ValueBlock { body, span });
            lowered_types.insert(wrapped, ty);
            Some(wrapped)
        };
        let output = match stage.kind {
            StreamStageKind::Batch => SchemeType::List(Box::new(item)),
            StreamStageKind::Zip => {
                let right = Self::scheme_pipeline_item(lowered_types.get(&values[0]?)?, facts)?;
                SchemeType::Record { fields: BTreeMap::from([(Name::intern("left"), item), (Name::intern("right"), right)]), tail: crate::sema::schemes::SchemeRowTail::Closed }
            }
            StreamStageKind::Range => SchemeType::Ground(Type::Int),
            StreamStageKind::TablePrint => SchemeType::Ground(Type::Unit),
            _ => item,
        };
        let lowered = match stage.kind {
            StreamStageKind::Batch => match values.as_slice() {
                [Some(count), None, None] => LoweredPipelineStage::BatchCount { count: wrap(self, *count, lowered_types)? },
                [None, Some(max_bytes), None] => LoweredPipelineStage::BatchMaxBytes { max_bytes: wrap(self, *max_bytes, lowered_types)? },
                _ => {
                    let fields = parameters.iter().zip(&values).filter_map(|(parameter, value)| value.map(|value| super::LoweredRecordEntry::Field(parameter.name, value))).collect();
                    let record = push_build_row!(self, expr, BuildExprRow::Record(fields));
                    let ty = SchemeType::Record { fields: parameters.iter().zip(&values).filter_map(|(parameter, value)| value.map(|value| Some((parameter.name, lowered_types.get(&value)?.clone())))).collect::<Option<BTreeMap<_, _>>>()?, tail: crate::sema::schemes::SchemeRowTail::Closed };
                    lowered_types.insert(record, ty);
                    LoweredPipelineStage::BatchLimits { configuration: wrap(self, record, lowered_types)? }
                }
            },
            StreamStageKind::Zip => LoweredPipelineStage::Zip { other: wrap(self, values[0]?, lowered_types)? },
            StreamStageKind::Repeat => LoweredPipelineStage::Repeat { count: wrap(self, values[0]?, lowered_types)? },
            StreamStageKind::Range => LoweredPipelineStage::Range { start: wrap(self, values[0]?, lowered_types)?, end: values[1]? },
            StreamStageKind::BytesChunks => LoweredPipelineStage::BytesChunks { size: wrap(self, values[0]?, lowered_types)? },
            StreamStageKind::TablePrint => match values[0] {
                Some(value) => LoweredPipelineStage::TablePrintConfigured { columns: wrap(self, value, lowered_types)? },
                None => LoweredPipelineStage::TablePrint { columns: None },
            },
            StreamStageKind::Shuffle => LoweredPipelineStage::Shuffle { seed: match values[0] { Some(value) => Some(wrap(self, value, lowered_types)?), None => None } },
            _ => return None,
        };
        Some((lowered, output))
    }

    pub(super) fn scheme_pipeline_item(ty: &SchemeType, facts: &SchemeBodyFacts) -> Option<SchemeType> {
        match ty {
            SchemeType::List(item) | SchemeType::Stream(item) | SchemeType::Set(item) => Some((**item).clone()),
            SchemeType::Ground(ty) => match ty.unvalidated() {
                Type::List(item) | Type::Stream(item) | Type::Set(item) => Some(SchemeType::Ground((**item).clone())),
                Type::Result(ok, _) => Self::scheme_pipeline_item(&SchemeType::Ground((**ok).clone()), facts),
                _ => None,
            },
            SchemeType::Validated(_, ty) | SchemeType::Result(ty, _) => Self::scheme_pipeline_item(ty, facts),
            SchemeType::Parameter(parameter) => facts.scheme().callable().requirements().iteration_parameters.get(parameter).map(|requirement| requirement.item.clone()),
            _ => None,
        }
    }

    pub(super) fn lower_scheme_pipeline_reduce_by(
        &mut self, expression: ExprId, stage: &ArenaStreamStage, item_type: &SchemeType, slots: &mut SlotScope,
        current_function: Name, facts: &SchemeBodyFacts, visited: &mut FxHashSet<ExprId>,
        lowered_types: &mut BTreeMap<BuildExprId, SchemeType>,
    ) -> Option<LoweredPipelineStage> {
        let (item_slot, value) = self.lower_scheme_pipeline_callback(stage, item_type, slots, current_function, facts, visited, lowered_types)?;
        let mut configuration = Vec::new();
        let arguments = self.lower_scheme_call_arguments(expression, stage.args, None, slots, current_function, facts, visited, lowered_types, &mut configuration)?;
        let parameters = crate::sema::stage_arguments::stage_argument_params(stage.kind.as_str());
        let binding = self.bodies.argument_bindings.get(&self.program.arena.span(stage.span))?;
        if arguments.len() != binding.argument_slots.len() || binding.callable_entry.is_some() { return None; }
        let mut fields = BTreeMap::new();
        let mut entries = Vec::new();
        for (argument, &slot) in arguments.iter().zip(&binding.argument_slots) {
            let name = parameters.get(slot)?.name;
            let value = argument.lowered?;
            if fields.insert(name, lowered_types.get(&value)?.clone()).is_some() { return None; }
            entries.push(LoweredRecordEntry::Field(name, value));
        }
        let record = push_build_row!(self, expr, BuildExprRow::Record(entries));
        let ty = SchemeType::Record { fields, tail: crate::sema::schemes::SchemeRowTail::Closed };
        lowered_types.insert(record, ty.clone());
        configuration.push(push_build_row!(self, stmt, BuildStmtRow::Value { value: record }));
        let configuration = push_build_row!(self, expr, BuildExprRow::ValueBlock { body: configuration, span: self.program.arena.span(stage.span) });
        lowered_types.insert(configuration, ty);
        Some(LoweredPipelineStage::ReduceByConfigured { item_slot, body: Vec::new(), value, configuration })
    }

    pub(super) fn lower_scheme_pipeline_observer(
        &mut self, stage: &ArenaStreamStage, item_type: &SchemeType, slots: &mut SlotScope,
        current_function: Name, facts: &SchemeBodyFacts, visited: &mut FxHashSet<ExprId>,
        lowered_types: &mut BTreeMap<BuildExprId, SchemeType>,
    ) -> Option<(usize, Vec<BuildStmtId>)> {
        let Some(block) = stage.block else {
            let (slot, value) = self.lower_scheme_pipeline_callback(stage, item_type, slots, current_function, facts, visited, lowered_types)?;
            let actual = lowered_types.get(&value)?;
            let unit = match actual {
                SchemeType::Result(ok, _) => matches!(ok.as_ref(), SchemeType::Ground(Type::Unit)),
                SchemeType::Ground(ty) => *ty == Type::Unit || ty.is_result_unit(),
                _ => false,
            };
            if !unit { return None; }
            return Some((slot, vec![push_build_row!(self, stmt, BuildStmtRow::Expr { value, span: self.program.arena.span(stage.span) })]));
        };
        let saved = slots.enter();
        let observer = (|| {
            let definition = self.program.arena.block(block);
            let (span, name) = match self.program.arena.block_params(definition.params) {
                [] => (self.program.arena.span(definition.span), crate::sema::check::item_binding()),
                [parameter] => (self.program.arena.span(parameter.span), parameter.name),
                _ => return None,
            };
            let slot = slots.declare_scheme_source(self.declaration_binding(span, name)?, name, item_type.clone());
            let body = self.lower_scheme_statement_block(block, slots, current_function, facts, visited, lowered_types)?;
            Some((slot, body))
        })();
        slots.exit(saved);
        observer
    }

    pub(super) fn lower_scheme_pipeline_callback(
        &mut self, stage: &ArenaStreamStage, item_type: &SchemeType, slots: &mut SlotScope,
        current_function: super::Name, facts: &SchemeBodyFacts, visited: &mut FxHashSet<ExprId>,
        lowered_types: &mut BTreeMap<BuildExprId, SchemeType>,
    ) -> Option<(usize, BuildExprId)> {
        let saved = slots.enter();
        let callback = (|| {
            if let Some(block) = stage.block {
                let definition = self.program.arena.block(block);
                let (span, name) = match self.program.arena.block_params(definition.params) {
                    [] => (self.program.arena.span(definition.span), crate::sema::check::item_binding()),
                    [parameter] => (self.program.arena.span(parameter.span), parameter.name),
                    _ => return None,
                };
                let slot = slots.declare_scheme_source(self.declaration_binding(span, name)?, name, item_type.clone());
                let value = self.lower_scheme_value_block(block, slots, current_function, facts, visited, lowered_types)?;
                return Some((slot, value));
            }
            let entry = self.bodies.argument_bindings.get(&self.program.arena.span(stage.span))?.callable_entry?;
            let callee = super::compact_call_arg_expr(self.program.arena.call_args(stage.args).get(entry)?)?;
            if !visited.insert(callee) { return None; }
            let name = crate::sema::check::item_binding();
            let span = self.program.arena.expr(callee).span;
            let slot = slots.declare_scheme_source(self.declaration_binding(span, name)?, name, item_type.clone());
            let item = push_build_row!(self, expr, BuildExprRow::Param(slot));
            lowered_types.insert(item, item_type.clone());
            let result = facts.expression_types().get(&callee)?.clone();
            let namespace = matches!(self.program.arena.expr(callee).kind, ArenaExprKind::Field { base, .. }
                if self.expression_binding(base).and_then(|binding| self.bodies.resolved_bindings.declarations.get(&binding)).is_some_and(|declaration| declaration.global && matches!(declaration.kind, super::BindingKind::Module(_))));
            let value = if namespace || self.declarations.static_callable_aliases.contains_key(&span)
                || self.bodies.scheme_callable_calls.contains_key(&crate::sema::schemes::InstantiationSite::call(callee))
            {
                self.lower_scheme_stage_value_call(callee, CallArgument { name: None, splice: false, source: crate::sema::arguments::ArgumentValueSource::CheckedType, ty: item_type.clone(), span, lowered: Some(item) }, slots, current_function, facts, visited, lowered_types)?
            } else if let Some(target) = self.bodies.selected_function_calls.get(&callee).copied() {
                let function = self.function_index().checked_target_key(target)?;
                let arguments = [CallArgument { name: None, splice: false, source: crate::sema::arguments::ArgumentValueSource::CheckedType, ty: item_type.clone(), span, lowered: Some(item) }];
                let arguments = self.lower_checked_function_arguments(callee, &arguments, function, slots, Some(current_function), None, SchemeType::closed_ground_type)?;
                self.record_scheme_call_argument_checks(&arguments, lowered_types);
                let value = self.lower_checked_function_call_values(function, arguments, span, Some(current_function))?;
                if let Some(instantiation) = self.checked_invocation(&crate::sema::schemes::InstantiationSite::call(callee)).cloned() {
                    let result_type = result.closed_ground_type().map(|ty| self.canonicalize_checked_type(ty));
                    let mut scratch = self.scratch.borrow_mut();
                    if !scratch.record_call_instantiation(value, instantiation)
                        || result_type.is_some_and(|ty| !scratch.record_checked_call_result_type(value, ty))
                    { return None; }
                }
                value
            } else {
                let plan = facts.registered_calls().get(&callee).cloned()?;
                if plan.receiver.is_some() { return None; }
                let [argument_slot] = plan.argument_slots.as_ref() else { return None; };
                let mut body = Vec::new();
                let item = match plan.argument_checks.get(*argument_slot).and_then(Option::as_ref) {
                    Some(check) => self.lower_scheme_registered_argument(item, check, span, slots, lowered_types, &mut body),
                    None => item,
                };
                let mut args = vec![None; plan.signature.params.len()];
                *args.get_mut(*argument_slot)? = Some(item);
                while args.last() == Some(&None) { args.pop(); }
                let value = push_build_row!(self, expr, BuildExprRow::ModuleCall { call_plan: None, op: plan.signature.op, args, span });
                self.scratch.borrow_mut().registered_calls[value.index()] = Some(plan);
                lowered_types.insert(value, result.clone());
                if body.is_empty() { value } else {
                    body.push(push_build_row!(self, stmt, BuildStmtRow::Value { value }));
                    push_build_row!(self, expr, BuildExprRow::ValueBlock { body, span })
                }
            };
            lowered_types.insert(value, result);
            Some((slot, value))
        })();
        slots.exit(saved);
        callback
    }

    pub(super) fn lower_scheme_static_callable_projection(
        &mut self, expression: ExprId, slots: &SlotScope, facts: &SchemeBodyFacts, visited: &mut FxHashSet<ExprId>, lowered_types: &mut BTreeMap<BuildExprId, SchemeType>,
    ) -> Option<BuildExprId> {
        let ArenaExprKind::Field { base, name } = self.program.arena.expr(expression).kind else { return None; };
        let binding = self.expression_binding(base)?;
        let declaration = self.bodies.resolved_bindings.declarations.get(&binding)?;
        if !declaration.global || !matches!(declaration.kind, super::BindingKind::Module(_)) { return None; }
        let source = slots.types.get(&binding)?.clone();
        let Some(Type::Module(module)) = source.closed_ground_type() else { return None; };
        if !module.exact { return None; }
        let result = match module.get(&name)? {
            super::ModuleExportType::GenericCallable { kind, optional: false } => {
                let relationship = self.bodies.scheme_callable_calls.get(&crate::sema::schemes::InstantiationSite::call(expression)).copied()
                    .or_else(|| match facts.expression_types().get(&expression)?.closed_ground_type()? { Type::SchemeCallable(id) => Some(id), _ => None })?;
                let contract = self.declarations.scheme_callables.relationship(relationship).ok()?;
                let target = self.bodies.selected_function_calls.get(&expression)?;
                if contract.canonical().scheme().kind() != *kind || contract.targets().len() != 1 || contract.targets()[0].target() != *target { return None; }
                Type::SchemeCallable(relationship)
            }
            export => export.static_callable_value_type()?,
        };
        let ArenaExprKind::Ident(namespace) = self.program.arena.expr(base).kind else { return None; };
        if facts.expression_types().contains_key(&base) && !visited.insert(base) { return None; }
        let base = self.lower_bare_ident(base, namespace, slots)?;
        lowered_types.insert(base, source);
        let value = push_build_row!(self, expr, BuildExprRow::Field { base, name: name.as_str(), span: self.program.arena.expr(expression).span });
        lowered_types.insert(value, SchemeType::Ground(result));
        Some(value)
    }

    pub(super) fn lower_scheme_pipeline_fold(
        &mut self, stage: &ArenaStreamStage, item_type: &SchemeType, initial: BuildExprId,
        slots: &mut SlotScope, current_function: Name, facts: &SchemeBodyFacts,
        visited: &mut FxHashSet<ExprId>, lowered_types: &mut BTreeMap<BuildExprId, SchemeType>,
    ) -> Option<LoweredPipelineStage> {
        let accumulator = lowered_types.get(&initial)?.clone();
        let block = stage.block?;
        let params = self.program.arena.block_params(self.program.arena.block(block).params);
        if params.len() > 2 { return None; }
        let saved = slots.enter();
        let stage = (|| {
            let acc_slot = match params.first() {
                Some(parameter) => slots.declare_scheme_source(self.declaration_binding(self.program.arena.span(parameter.span), parameter.name)?, parameter.name, accumulator),
                None => slots.reserve("pipeline.acc"),
            };
            let item_slot = match params.get(1) {
                Some(parameter) => slots.declare_scheme_source(self.declaration_binding(self.program.arena.span(parameter.span), parameter.name)?, parameter.name, item_type.clone()),
                None => slots.reserve("pipeline.item"),
            };
            let value = self.lower_scheme_value_block(block, slots, current_function, facts, visited, lowered_types)?;
            Some(LoweredPipelineStage::Fold { acc_slot, item_slot, initial, body: Vec::new(), value })
        })();
        slots.exit(saved);
        stage
    }

    pub(super) fn fuse_par_map_flat_map_reduce_by(&self, stages: &mut Vec<LoweredPipelineStage>) {
        let mut fused = Vec::with_capacity(stages.len());
        let mut index = 0;
        while index < stages.len() {
            if let Some((slot, body, jobs, value)) = Self::lowered_par_map_parts(&stages[index]) {
                if index + 2 < stages.len()
                    && self.lowered_flat_map_is_identity(&stages[index + 1])
                    && let Some((reduce_item_slot, reduce_body, reduce_value, op)) =
                        Self::lowered_reduce_by_parts(&stages[index + 2])
                {
                    fused.push(LoweredPipelineStage::ParMapFlatMapReduceBy {
                        slot,
                        body,
                        jobs,
                        value,
                        flatten: true,
                        reduce_item_slot,
                        reduce_body,
                        reduce_value,
                        op,
                    });
                    index += 3;
                    continue;
                }
                if index + 1 < stages.len()
                    && let Some((reduce_item_slot, reduce_body, reduce_value, op)) =
                        Self::lowered_reduce_by_parts(&stages[index + 1])
                {
                    fused.push(LoweredPipelineStage::ParMapFlatMapReduceBy {
                        slot,
                        body,
                        jobs,
                        value,
                        flatten: false,
                        reduce_item_slot,
                        reduce_body,
                        reduce_value,
                        op,
                    });
                    index += 2;
                    continue;
                }
            }
            fused.push(stages[index].clone());
            index += 1;
        }
        *stages = fused;
    }

    fn lowered_par_map_parts(
        stage: &LoweredPipelineStage,
    ) -> Option<(
        usize,
        Option<Vec<BuildStmtId>>,
        Option<BuildExprId>,
        BuildExprId,
    )> {
        match stage {
            LoweredPipelineStage::ParMap { slot, jobs, value } => {
                Some((*slot, None, *jobs, *value))
            }
            LoweredPipelineStage::ParMapBlock {
                slot,
                body,
                jobs,
                value,
            } => Some((*slot, Some(body.clone()), *jobs, *value)),
            _ => None,
        }
    }

    fn lowered_flat_map_is_identity(&self, stage: &LoweredPipelineStage) -> bool {
        let (slot, body, value) = match stage {
            LoweredPipelineStage::FlatMap { slot, value } => (*slot, true, *value),
            LoweredPipelineStage::FlatMapBlock { slot, body, value } => {
                (*slot, body.is_empty(), *value)
            }
            _ => return false,
        };
        if !body {
            return false;
        }
        matches!(
            self.scratch.borrow().expressions.get(value.index()),
            Some(BuildExprRow::Param(param)) if *param == slot
        )
    }

    fn lowered_reduce_by_parts(
        stage: &LoweredPipelineStage,
    ) -> Option<(usize, Vec<BuildStmtId>, BuildExprId, ReduceByOp)> {
        match stage {
            LoweredPipelineStage::ReduceBy {
                item_slot,
                body,
                value,
                op,
                jobs: None,
            } => Some((*item_slot, body.clone(), *value, *op)),
            _ => None,
        }
    }

    pub(super) fn lower_pipeline_stage(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
    ) -> Option<LoweredPipelineStage> {
        if stage.block.is_none()
            && let Some(entry) = self
                .bodies
                .argument_bindings
                .get(&self.program.arena.span(stage.span))
                .and_then(|binding| binding.callable_entry)
        {
            let callee =
                compact_call_arg_expr(self.program.arena.call_args(stage.args).get(entry)?)?;
            let slot = slots.reserve("pipeline.item");
            let item = push_build_row!(self, expr, BuildExprRow::Param(slot));
            let value = self.lower_stage_callable_call(callee, item, slots, current_function)?;
            return Some(match stage.kind {
                StreamStageKind::Map => LoweredPipelineStage::Map { slot, value },
                StreamStageKind::Where => LoweredPipelineStage::Where { slot, predicate: value },
                StreamStageKind::FlatMap => LoweredPipelineStage::FlatMap { slot, value },
                StreamStageKind::UniqueBy => LoweredPipelineStage::UniqueBy { slot, key: value },
                StreamStageKind::GroupBy => LoweredPipelineStage::GroupBy { slot, key: value },
                StreamStageKind::Any => LoweredPipelineStage::Any { slot, predicate: value },
                StreamStageKind::All => LoweredPipelineStage::All { slot, predicate: value },
                StreamStageKind::SortBy => return self.lower_configured_pipeline_stage(
                    stage, slots, current_function, item_ty, Some((slot, value)),
                ),
                StreamStageKind::Each | StreamStageKind::Tee => {
                    let span = self.program.arena.expr(callee).span;
                    let body = vec![push_build_row!(self, stmt, BuildStmtRow::Expr { value, span })];
                    if stage.kind == StreamStageKind::Each { LoweredPipelineStage::Each { slot, body } }
                    else { LoweredPipelineStage::Tee { slot, body } }
                }
                StreamStageKind::TextStreamLines | StreamStageKind::BytesChunks
                | StreamStageKind::JsonLines | StreamStageKind::JsonStream
                | StreamStageKind::Enumerate | StreamStageKind::Zip | StreamStageKind::Sort
                | StreamStageKind::Sum | StreamStageKind::Collect | StreamStageKind::First
                | StreamStageKind::Last | StreamStageKind::Min | StreamStageKind::Max
                | StreamStageKind::Count | StreamStageKind::Take | StreamStageKind::Drop
                | StreamStageKind::Repeat | StreamStageKind::Range | StreamStageKind::Batch
                | StreamStageKind::ParMap | StreamStageKind::TablePrint | StreamStageKind::ReduceBy
                | StreamStageKind::Shuffle | StreamStageKind::Fold | StreamStageKind::Reduce => return None,
            });
        }
        if !xsh_registry::stream_parameters::stage_parameters(stage.kind.as_str()).is_empty() {
            return self.lower_configured_pipeline_stage(stage, slots, current_function, item_ty, None);
        }
        match stage.kind {
            StreamStageKind::TextStreamLines => {
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::TextLines)
            }
            StreamStageKind::JsonLines | StreamStageKind::JsonStream => {
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::JsonLines)
            }
            StreamStageKind::Enumerate => {
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Enumerate)
            }
            StreamStageKind::Zip => None,
            StreamStageKind::Sort => None,
            StreamStageKind::Sum => {
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Sum)
            }
            StreamStageKind::Collect => {
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Collect)
            }
            StreamStageKind::First => {
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::First)
            }
            StreamStageKind::Last => {
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Last)
            }
            StreamStageKind::Min => {
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Min)
            }
            StreamStageKind::Max => {
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Max)
            }
            StreamStageKind::SortBy => None,
            StreamStageKind::UniqueBy => {
                if let Some((slot, key)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::UniqueBy { slot, key });
                }
                let (slot, key) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::UniqueBy { slot, key })
            }
            StreamStageKind::GroupBy => {
                if let Some((slot, key)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::GroupBy { slot, key });
                }
                let (slot, key) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::GroupBy { slot, key })
            }
            StreamStageKind::Count => {
                if !stage.args.is_empty() {
                    if stage.block.is_some() {
                        return None;
                    }
                    if let Some((slot, key)) = self.try_lower_pipeline_stage_shorthand(
                        stage,
                        slots,
                        current_function,
                        item_ty,
                    ) {
                        return Some(LoweredPipelineStage::CountBy { slot, key });
                    }
                    return None;
                }
                if stage.block.is_none() {
                    return Some(LoweredPipelineStage::Count);
                }
                let (slot, key) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::CountBy { slot, key })
            }
            StreamStageKind::Where => {
                if let Some((slot, predicate)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::Where { slot, predicate });
                }
                if let Some((slot, predicate)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::Where { slot, predicate });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::WhereBlock { slot, body, value })
            }
            StreamStageKind::Map => {
                if !stage.args.is_empty() {
                    if stage.block.is_some() {
                        return None;
                    }
                    let args = self.program.arena.call_args(stage.args);
                    let [arg] = args else {
                        return None;
                    };
                    let ArenaCallArgKind::Positional(expr) = arg.kind else {
                        return None;
                    };
                    let (slot, _cleanup) =
                        self.lower_pipeline_stage_item_slot(stage, slots, item_ty)?;
                    let value = self.lower_expr(expr, slots, current_function, Some(slot))?;
                    cleanup_pipeline_stage_item_slot(slots, _cleanup, slot);
                    return Some(LoweredPipelineStage::Map { slot, value });
                }
                if let Some((slot, value)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::Map { slot, value });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::MapBlock { slot, body, value })
            }
            StreamStageKind::FlatMap => {
                if let Some((slot, value)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::FlatMap { slot, value });
                }
                if let Some((slot, value)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::FlatMap { slot, value });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::FlatMapBlock { slot, body, value })
            }
            StreamStageKind::Any => {
                if let Some((slot, predicate)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::Any { slot, predicate });
                }
                if let Some((slot, predicate)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::Any { slot, predicate });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::AnyBlock { slot, body, value })
            }
            StreamStageKind::All => {
                if let Some((slot, predicate)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::All { slot, predicate });
                }
                if let Some((slot, predicate)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::All { slot, predicate });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::AllBlock { slot, body, value })
            }
            StreamStageKind::Take | StreamStageKind::Drop => None,
            StreamStageKind::Repeat => None,
            StreamStageKind::Range => None,
            StreamStageKind::BytesChunks => None,
            StreamStageKind::Batch => None,
            StreamStageKind::ParMap => None,
            StreamStageKind::Each => {
                let block = stage.block?;
                let (slot, cleanup) = self.lower_pipeline_stage_item_slot(stage, slots, item_ty)?;
                let saved = slots.enter();
                let body =
                    self.lower_block_in_current_scope(block, slots, current_function, Some(slot))?;
                slots.exit(saved);
                cleanup_pipeline_stage_item_slot(slots, cleanup, slot);
                Some(LoweredPipelineStage::Each { slot, body })
            }
            StreamStageKind::Tee => {
                let block = stage.block?;
                let (slot, cleanup) = self.lower_pipeline_stage_item_slot(stage, slots, item_ty)?;
                let saved = slots.enter();
                let body =
                    self.lower_block_in_current_scope(block, slots, current_function, Some(slot))?;
                slots.exit(saved);
                cleanup_pipeline_stage_item_slot(slots, cleanup, slot);
                Some(LoweredPipelineStage::Tee { slot, body })
            }
            StreamStageKind::TablePrint => None,
            StreamStageKind::ReduceBy => None,
            StreamStageKind::Shuffle => None,
            StreamStageKind::Fold | StreamStageKind::Reduce => None,
        }
    }

    // The first consumed configuration value initializes source-ordered checked
    // temporaries. Later configuration fields read those slots at the same stage
    // boundary; record spreads and effectful expressions are never repeated.
    fn lower_configured_pipeline_stage(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
        callable: Option<(usize, BuildExprId)>,
    ) -> Option<LoweredPipelineStage> {
        use crate::sema::arguments::{ArgumentValueSource, expand_named_arguments};
        let params = crate::sema::stage_arguments::stage_argument_params(stage.kind.as_str());
        let span = self.program.arena.span(stage.span);
        let argument_slots = &self.bodies.argument_bindings.get(&span)?.argument_slots;
        let callable_entry = self.bodies.argument_bindings.get(&span)?.callable_entry;
        let arguments = self.program.arena.call_args(stage.args).iter().enumerate()
            .filter(|(entry, _)| Some(*entry) != callable_entry)
            .map(|(_, argument)| argument.clone()).collect::<Vec<_>>();
        let expanded = expand_named_arguments(
            self.program,
            &arguments,
            |expr| {
                self.bodies
                    .expr_types
                    .get(&expr)
                    .cloned()
                    .or_else(|| self.checked_expr_type(expr))
            },
        )
        .ok()?;
        let lowered =
            self.lower_expanded_argument_values(&expanded, None, slots, current_function, None)?;
        let mut values = vec![None; params.len()];
        let mut types = vec![None; params.len()];
        let mut booleans = vec![Some(false); params.len()];
        for ((argument, &slot), lowered_argument) in expanded
            .iter()
            .zip(argument_slots)
            .zip(&lowered.arguments)
        {
            values[slot] = Some(lowered_argument.lowered?);
            types[slot] = Some(argument.ty.clone());
            booleans[slot] = match argument.value {
                ArgumentValueSource::Expression(expr) => match self.program.arena.expr(expr).kind {
                    ArenaExprKind::Bool(value) => Some(value),
                    _ => None,
                },
                _ => None,
            };
        }
        let wrap = |this: &mut Self, value| {
            this.wrap_argument_bindings(value, lowered.bindings.clone(), span)
        };
        let record = |this: &mut Self| {
            let fields = params
                .iter()
                .zip(&values)
                .filter_map(|(parameter, value)| {
                    value.map(|value| LoweredRecordEntry::Field(parameter.name, value))
                })
                .collect();
            let record = push_build_row!(this, expr, BuildExprRow::Record(fields));
            wrap(this, record)
        };
        match stage.kind {
            StreamStageKind::ParMap => {
                let jobs = values[0].map(|value| wrap(self, value));
                if let Some((slot, value)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::ParMap { slot, jobs, value });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::ParMapBlock {
                    slot,
                    body,
                    jobs,
                    value,
                })
            }
            StreamStageKind::Sort => Some(LoweredPipelineStage::Sort {
                descending: values[0].map(|value| wrap(self, value)),
            }),
            StreamStageKind::SortBy => {
                let (slot, key) = callable.or_else(||
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty))?;
                Some(LoweredPipelineStage::SortBy {
                    slot,
                    key,
                    descending: values[0].map(|value| wrap(self, value)),
                })
            }
            StreamStageKind::Batch => match values.as_slice() {
                [Some(count), None, None] => Some(LoweredPipelineStage::BatchCount {
                    count: wrap(self, *count),
                }),
                [None, Some(max_bytes), None] => Some(LoweredPipelineStage::BatchMaxBytes {
                    max_bytes: wrap(self, *max_bytes),
                }),
                [None, None, Some(_)] if booleans[2] == Some(true) => {
                    Some(LoweredPipelineStage::BatchMaxArgv { max_argv: None })
                }
                _ => Some(LoweredPipelineStage::BatchLimits {
                    configuration: record(self),
                }),
            },
            StreamStageKind::ReduceBy => {
                let (item_slot, value) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)?;
                let body = Vec::new();
                if booleans[..3].iter().all(Option::is_some) {
                    let op = match booleans[..3]
                        .iter()
                        .position(|value| *value == Some(true))?
                    {
                        0 => ReduceByOp::Sum,
                        1 => ReduceByOp::Min,
                        _ => ReduceByOp::Max,
                    };
                    let jobs = values[3].map(|value| wrap(self, value));
                    Some(LoweredPipelineStage::ReduceBy {
                        item_slot,
                        body,
                        value,
                        op,
                        jobs,
                    })
                } else {
                    Some(LoweredPipelineStage::ReduceByConfigured {
                        item_slot,
                        body,
                        value,
                        configuration: record(self),
                    })
                }
            }
            StreamStageKind::Take => Some(LoweredPipelineStage::Take(wrap(self, values[0]?))),
            StreamStageKind::Drop => Some(LoweredPipelineStage::Drop(wrap(self, values[0]?))),
            StreamStageKind::Repeat => Some(LoweredPipelineStage::Repeat {
                count: wrap(self, values[0]?),
            }),
            StreamStageKind::Range => Some(LoweredPipelineStage::Range {
                start: wrap(self, values[0]?),
                end: values[1]?,
            }),
            StreamStageKind::BytesChunks => Some(LoweredPipelineStage::BytesChunks {
                size: wrap(self, values[0]?),
            }),
            StreamStageKind::Zip => Some(LoweredPipelineStage::Zip {
                other: wrap(self, values[0]?),
            }),
            StreamStageKind::Fold | StreamStageKind::Reduce => {
                let initial = wrap(self, values[0]?);
                self.lower_pipeline_stage_fold(
                    stage,
                    slots,
                    current_function,
                    item_ty,
                    initial,
                    types[0].clone(),
                )
            }
            StreamStageKind::Shuffle => Some(LoweredPipelineStage::Shuffle {
                seed: values[0].map(|value| wrap(self, value)),
            }),
            StreamStageKind::TablePrint => Some(match values[0] {
                Some(value) => LoweredPipelineStage::TablePrintConfigured {
                    columns: wrap(self, value),
                },
                None => LoweredPipelineStage::TablePrint { columns: None },
            }),
            _ => None,
        }
    }

    fn try_lower_pipeline_stage_shorthand(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        _item_ty: Option<&Type>,
    ) -> Option<(usize, BuildExprId)> {
        if stage.block.is_some() || stage.args.is_empty() {
            return None;
        }
        let args = self.program.arena.call_args(stage.args);
        let [arg] = args else {
            return None;
        };
        let ArenaCallArgKind::Positional(expr) = arg.kind else {
            return None;
        };
        let slot = slots.reserve("pipeline.item");
        let value = self.lower_expr(expr, slots, current_function, Some(slot))?;
        Some((slot, value))
    }

    // The bound initializer type stays on the accumulator slot. Nested pipeline
    // tails resolve their field and stage argument types from that slot, so
    // retaining the checked type is necessary for valid compositions to lower.
    // Callback parameters have their own lexical scope and may shadow outer names.
    fn lower_pipeline_stage_fold(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
        initial: BuildExprId,
        acc_ty: Option<Type>,
    ) -> Option<LoweredPipelineStage> {
        let block = stage.block?;
        let saved = slots.enter();
        let params = self
            .program
            .arena
            .block_params(self.program.arena.block(block).params);
        let acc_slot = match params {
            [] => slots.reserve("pipeline.acc"),
            [acc] | [acc, _] => match self.declare_source_at(
                self.program.arena.span(acc.span),
                acc.name,
                acc_ty,
                slots,
            ) {
                Some(slot) => slot,
                None => {
                    slots.exit(saved);
                    return None;
                }
            },
            _ => {
                slots.exit(saved);
                return None;
            }
        };
        let item_slot = match params {
            [_, item] => match self.declare_source_at(
                self.program.arena.span(item.span),
                item.name,
                item_ty.cloned(),
                slots,
            ) {
                Some(slot) => slot,
                None => {
                    slots.exit(saved);
                    return None;
                }
            },
            _ => slots.reserve("pipeline.item"),
        };
        let body = Vec::new();
        let value =
            match self.lower_block_value_expr(block, slots, current_function, Some(item_slot)) {
                Some(value) => value,
                None => {
                    slots.exit(saved);
                    return None;
                }
            };
        slots.exit(saved);
        Some(LoweredPipelineStage::Fold {
            acc_slot,
            item_slot,
            initial,
            body,
            value,
        })
    }

    fn lower_pipeline_stage_expr(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
    ) -> Option<(usize, BuildExprId)> {
        let block = stage.block?;
        let statements = self.program.arena.block(block).statements;
        let statements = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
        let (slot, cleanup) = self.lower_pipeline_stage_item_slot(stage, slots, item_ty)?;
        let lowered = match statements.as_slice() {
            [stmt] => self
                .lower_tail_stmt_as_expr(*stmt, slots, current_function, Some(slot))
                .or_else(|| {
                    self.lower_block_value_expr(block, slots, current_function, Some(slot))
                }),
            _ => self.lower_block_value_expr(block, slots, current_function, Some(slot)),
        };
        let expr = match lowered {
            Some(expr) => expr,
            None => {
                cleanup_pipeline_stage_item_slot(slots, cleanup, slot);
                return None;
            }
        };
        cleanup_pipeline_stage_item_slot(slots, cleanup, slot);
        Some((slot, expr))
    }

    fn lower_pipeline_stage_block(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
    ) -> Option<(usize, Vec<BuildStmtId>, BuildExprId)> {
        let block = stage.block?;
        let statements = self.program.arena.block(block).statements;
        let ids = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
        let (&tail, prefix) = ids.split_last()?;
        let saved = slots.enter();
        let (slot, _cleanup) = match self.lower_pipeline_stage_item_slot(stage, slots, item_ty) {
            Some(value) => value,
            None => {
                slots.exit(saved);
                return None;
            }
        };
        let mut body = Vec::with_capacity(prefix.len());
        for stmt in prefix {
            let Some(lowered) =
                self.lower_stmt_with_blocker_guard(*stmt, slots, current_function, Some(slot))
            else {
                slots.exit(saved);
                return None;
            };
            body.push(lowered);
        }
        let value = match self.lower_tail_stmt_as_expr(tail, slots, current_function, Some(slot)) {
            Some(value) => value,
            None => {
                slots.exit(saved);
                return None;
            }
        };
        slots.exit(saved);
        Some((slot, body, value))
    }

    fn lower_pipeline_stage_item_slot(
        &self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        item_ty: Option<&Type>,
    ) -> Option<(usize, Option<Name>)> {
        let block = stage.block?;
        let params = self
            .program
            .arena
            .block_params(self.program.arena.block(block).params);
        match params {
            [] => Some((slots.reserve("pipeline.item"), None)),
            [param] => {
                // The callback's source parameter keeps its checked identity;
                // its spelling only describes the temporary runtime layout.
                Some((
                    self.declare_source_at(
                        self.program.arena.span(param.span),
                        param.name,
                        item_ty.cloned(),
                        slots,
                    )?,
                    Some(param.name),
                ))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed_fold_restores_layout(missing_parameter: usize) {
        let source = "let result = [1, 2] |> fold(0) { |acc, item| acc + item }\n";
        let entry_source = crate::loader::entry_source_from_text("pipeline-fold-binding.xsh", source.to_string());
        let source_id = entry_source.source_id;
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            "pipeline-fold-binding.xsh",
            entry_source,
            Vec::new(),
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let mut declarations = crate::sema::check::Checker::check_compact_declarations(
            &parsed.arena,
            &sources,
            source_id,
        );
        assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
        let stage = parsed
            .arena
            .statement_ids()
            .find_map(|statement| {
                let crate::syntax::arena::ArenaStmtKind::Let {
                    initializer: crate::syntax::arena::ArenaExprOrRun::Expr(expression),
                    ..
                } = parsed.arena.arena.stmt(statement).kind else {
                    return None;
                };
                match parsed.arena.arena.expr(expression).kind {
                    crate::syntax::arena::ArenaExprKind::Pipeline { stages, .. } => parsed
                        .arena
                        .arena
                        .pipe_stages(stages)
                        .iter()
                        .find_map(|stage| match &stage.kind {
                            crate::syntax::arena::ArenaPipeStageKind::Stream(stage)
                                if stage.kind == StreamStageKind::Fold => Some(stage.clone()),
                            _ => None,
                        }),
                    crate::syntax::arena::ArenaExprKind::StructuredPipeline { stages, .. } => parsed
                        .arena
                        .arena
                        .stream_stages(stages)
                        .iter()
                        .find(|stage| stage.kind == StreamStageKind::Fold)
                        .cloned(),
                    _ => None,
                }
            })
            .expect("the fixture contains a fold stage");
        let block = stage.block.expect("fold carries its callback block");
        let params = parsed
            .arena
            .arena
            .block_params(parsed.arena.arena.block(block).params);
        let param = params
            .get(missing_parameter)
            .expect("the requested fold parameter exists");
        let outer_name = params[0].name;
        let removed = std::sync::Arc::make_mut(&mut declarations.bodies.resolved_bindings)
            .declaration_sites
            .remove(&(parsed.arena.arena.span(param.span), param.name));
        assert!(removed.is_some(), "the checker published the parameter binding");

        let mut probe = CompactLowerConstructProbe {
            program: &parsed.arena,
            scope: std::rc::Rc::new(super::super::CompactRootScope::whole_program(
                &parsed.arena,
            )),
            declarations: &declarations,
            qualified_schemes: &declarations.function_schemes,
            bodies: &declarations.bodies,
            source,
            sources: Some(&sources),
            current_namespace: None,
            functions: None,
            top_level_known: crate::runtime::eval::lower::FxHashMap::default(),
            output: super::super::CompactLowerConstructProbeOutput::default(),
            last_blocker_detail: None,
            stdlib_linkage: super::super::StdlibLowerLinkage::Local,
            function_defs: std::rc::Rc::new(std::cell::RefCell::new(None)),
            scratch: std::rc::Rc::new(std::cell::RefCell::new(super::super::BuildScratch::default())),
            with_error_source_exprs: crate::runtime::eval::lower::FxHashMap::default(),
        };
        let mut slots = SlotScope::default();
        let outer_binding = crate::sema::check::BindingId::args();
        slots.bind_source_at(outer_binding, outer_name, 0, Some(Type::Str));
        let outer_layout = slots.indices.clone();
        let declared_before = slots.declared.len();
        let initial = super::super::build_expr(&probe.scratch, BuildExprRow::Unit);

        assert!(probe
            .lower_pipeline_stage_fold(&stage, &mut slots, None, None, initial, Some(Type::Int))
            .is_none());
        assert_eq!(slots.indices, outer_layout);
        assert_eq!(slots.declared.len(), declared_before);
        assert_eq!(slots.binding_type(outer_binding), Some(&Type::Str));
    }

    #[test]
    fn missing_fold_parameter_binding_restores_outer_layout() {
        failed_fold_restores_layout(0);
        failed_fold_restores_layout(1);
    }
}
