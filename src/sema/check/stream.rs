use super::{Binding, Checker, FxHashSet, Name};
use crate::diagnostic::DiagnosticCode;
use crate::source::Span;
use crate::sema::types::Type;
use crate::syntax::arena::{
    ArenaCallArgKind, ArenaExprKind, ArenaProgram, ArenaStreamStage, ExprId,
};
use crate::syntax::node::{StreamStageKind, UnaryOp};
use std::collections::BTreeMap;

fn btree_map<K: Into<Name>, V>(entries: Vec<(K, V)>) -> BTreeMap<Name, V> {
    entries
        .into_iter()
        .map(|(name, value)| (name.into(), value))
        .collect()
}

impl Checker {
    pub(super) fn check_structured_pipeline_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        input: ExprId,
        stages: crate::syntax::arena::ArenaRange,
    ) -> Type {
        let input_ty = self.check_expr_arena(arena, source, input, None);
        let Some((first, rest)) = arena.arena.stream_stages(stages).split_first() else {
            return input_ty;
        };
        let mut current = if first.kind.is_adapter() {
            let output = self.check_adapter_stage_arena(arena, source, first, input_ty.clone());
            self.stream_stage_types.insert(
                (self.current_namespace, arena.arena.span(first.span)),
                super::CheckedStreamStage {
                    input: input_ty,
                    output: output.clone(),
                },
            );
            output
        } else {
            match self.check_pipeline_input_type(input_ty, arena.arena.expr(input).span) {
                Err(Type::Any) => {
                    self.reject_dynamic_use("a pipeline source", None, arena.arena.expr(input).span);
                    Type::Stream(Box::new(Type::Unknown))
                }
                Ok(ty) => {
                    let output = self.check_stream_stage_arena(arena, source, first, ty.clone());
                    self.stream_stage_types.insert(
                        (self.current_namespace, arena.arena.span(first.span)),
                        super::CheckedStreamStage {
                            input: ty,
                            output: output.clone(),
                        },
                    );
                    output
                }
                Err(_) => {
                    self.error(
                        arena.arena.expr(input).span,
                        "structured pipelines require Stream or List input",
                        DiagnosticCode::CheckStreamInput,
                    );
                    Type::Stream(Box::new(Type::Unknown))
                }
            }
        };

        for stage in rest {
            let input = current;
            current = self.check_stream_stage_arena(arena, source, stage, input.clone());
            self.stream_stage_types.insert(
                (self.current_namespace, arena.arena.span(stage.span)),
                super::CheckedStreamStage {
                    input,
                    output: current.clone(),
                },
            );
        }
        match current {
            Type::Stream(item) => Type::List(item),
            other => other,
        }
    }

    fn check_pipeline_input_type(&mut self, ty: Type, span: Span) -> Result<Type, Type> {
        let ty = match self.type_constraints.resolve(&ty) {
            Ok(ty) => ty,
            Err(error) => {
                self.error(span, &format!("pipeline source type cannot be resolved: {error:?}"), DiagnosticCode::CheckLocalInference);
                return Err(Type::Invalid);
            }
        };
        let source = match ty.unvalidated() { Type::Result(ok, _) => ok.unvalidated(), source => source };
        if !matches!(source, Type::Inference(_)) { return stream_type_from_input(ty); }
        let item = self.type_constraints.fresh_at_level(span, self.callable_solver_level());
        let required = crate::sema::schemes::IterationRequirement {
            item: item.clone(),
            policy: crate::sema::schemes::IterationPolicy::PipelineTransport,
            source_kind: crate::sema::schemes::IterationSourceKind::DirectOrResult,
        };
        match self.type_constraints.require_iteration(&ty, &required, span) {
            Ok(()) => Ok(Type::Stream(Box::new(item))),
            Err(conflict) => {
                self.report_inferred_type_conflict(conflict);
                Err(Type::Invalid)
            }
        }
    }

    fn check_stream_stage_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        stage: &ArenaStreamStage,
        current: Type,
    ) -> Type {
        let stage_span = arena.arena.span(stage.span);
        let Type::Stream(item_ty) = current else {
            self.error(
                stage_span,
                "stream stages cannot follow a terminal stage",
                DiagnosticCode::CheckStreamTerminalStage,
            );
            return Type::Unknown;
        };
        let item_ty = *item_ty;
        if matches!(stage.kind, StreamStageKind::Range | StreamStageKind::Repeat) {
            self.fallible_pipeline_stage_spans.insert(stage_span);
        }
        for argument in arena.arena.call_args(stage.args) {
            if let ArenaCallArgKind::NamedSpread { value, .. } = argument.kind {
                self.check_expr_arena(arena, source, value, None);
            }
        }
        let mut named_callback_ty = None;
        let mut named_stage_arguments = None;
        match crate::sema::stage_arguments::stage_callable_argument(arena, stage, |expr| {
            self.expr_types.get(&arena.arena.expr(expr).span).cloned()
        }) {
            Ok(Some((entry, callee))) => {
                if !self.stage_callable_is_static(arena, callee) {
                    self.error(
                        arena.arena.expr(callee).span,
                        "stage callable must be a statically resolved named function or proc",
                        DiagnosticCode::CheckStreamCallable,
                    );
                    return Type::Unknown;
                }
                let item_span = arena.arena.expr(callee).span;
                let checked_argument = super::args::CheckedCallArgument {
                    ty: &item_ty,
                    span: item_span,
                };
                if let ArenaExprKind::Field { base, name } = arena.arena.expr(callee).kind
                    && let ArenaExprKind::Ident(namespace) = arena.arena.expr(base).kind
                    && let Some(overloads) =
                        super::api_spec().module_overloads(&namespace.as_str(), &name.as_str())
                    && !self.check_stream_callable_module_overload(
                        checked_argument,
                        overloads,
                        item_span,
                    )
                {
                    return Type::Unknown;
                }
                if let Some(argument) = arena
                    .arena
                    .call_args(stage.args)
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| *index != entry)
                    .map(|(_, argument)| argument)
                    .find(|argument| {
                        matches!(argument.kind, ArenaCallArgKind::Positional(_))
                            || matches!(argument.kind, ArenaCallArgKind::Named { name, .. } if name == "block")
                    })
                {
                    self.error(
                        super::args::call_arg_span_arena(arena, &argument.kind),
                        "stage accepts either a block or a named callable",
                        DiagnosticCode::CheckStreamCallable,
                    );
                    return Type::Unknown;
                }
                self.argument_bindings
                    .entry(arena.arena.span(stage.span))
                    .or_default()
                    .callable_entry = Some(entry);
                named_stage_arguments = Some(self.check_stage_arguments_excluding_callable_arena(
                    arena,
                    source,
                    stage,
                    Some(entry),
                ));
                self.push_deferred_capture_scope();
                let enclosing_boundary = self.enter_callback_boundary();
                self.define(
                    super::item_binding(),
                    Binding::new(item_ty.clone(), false),
                    item_span,
                );
                self.boundary.item_frames.push(super::ItemFrame::Implicit {
                    ty: item_ty.clone(),
                    used: true,
                    stage: true,
                });
                let call_diagnostics_before = self.diagnostics.len();
                let mut callback_ty = self.check_call_with_checked_argument_arena(
                    arena,
                    source,
                    callee,
                    checked_argument,
                    item_span,
                    None,
                );
                self.record_expr_type(item_span, callback_ty.clone());
                self.statement_positions
                    .insert(item_span, super::StatementPosition::Value);
                if self.diagnostics.len() == call_diagnostics_before {
                    self.statically_resolved_call_spans.insert(item_span);
                }
                let callback_returns_result_unit =
                    matches!(stage.kind, StreamStageKind::Each | StreamStageKind::Tee)
                        && callback_ty.is_result_unit();
                if matches!(stage.kind, StreamStageKind::Each | StreamStageKind::Tee) {
                    self.propagating_statements.insert(item_span);
                    callback_ty = self.check_unit_statement_value_tail(
                        None,
                        &callback_ty,
                        item_span,
                        item_span,
                        None,
                        false,
                    );
                }
                if callback_returns_result_unit
                    || self.propagating_error_spans.contains(&item_span)
                {
                    self.propagating_stage_callback_spans.insert(stage_span);
                }
                self.boundary.item_frames.pop();
                self.leave_callback_boundary(enclosing_boundary);
                self.pop_scope();
                named_callback_ty = Some(callback_ty);
            }
            Err((span, message)) => {
                self.error(span, &message, DiagnosticCode::CheckStreamCallable);
                return Type::Unknown;
            }
            Ok(None) => {}
        }
        let arguments = named_stage_arguments
            .unwrap_or_else(|| self.check_stage_arguments_arena(arena, source, stage));
        match stage.kind {
            StreamStageKind::Where => {
                self.check_stage_no_args_arena(arena, stage);
                let callback_ty = self.check_stream_stage_callback_type(
                    arena,
                    source,
                    stage,
                    &item_ty,
                    named_callback_ty.as_ref(),
                );
                self.check_stream_stage_callback_result(
                    stage.kind,
                    &item_ty,
                    callback_ty,
                    stage_span,
                )
            }
            StreamStageKind::Map => {
                self.check_stage_no_args_arena(arena, stage);
                let callback_ty = self.check_stream_stage_callback_type(
                    arena,
                    source,
                    stage,
                    &item_ty,
                    named_callback_ty.as_ref(),
                );
                self.check_stream_stage_callback_result(
                    stage.kind,
                    &item_ty,
                    callback_ty,
                    stage_span,
                )
            }
            StreamStageKind::ParMap => {
                self.check_stage_no_args_arena(arena, stage);
                let actual = self.check_required_stream_block_arena(arena, source, stage, &item_ty);
                let output_ty = actual;
                if output_ty == Type::Unit {
                    self.error(
                        stage_span,
                        "par-map requires a tail value",
                        DiagnosticCode::CheckMapTail,
                    );
                }
                Type::Stream(Box::new(output_ty))
            }
            StreamStageKind::Each => {
                self.check_stage_no_args_arena(arena, stage);
                let callback_ty = self.check_stream_stage_callback_type(
                    arena,
                    source,
                    stage,
                    &item_ty,
                    named_callback_ty.as_ref(),
                );
                self.check_stream_stage_callback_result(
                    stage.kind,
                    &item_ty,
                    callback_ty,
                    stage_span,
                )
            }
            StreamStageKind::Batch => {
                self.check_batch_stage_arena(arena, source, stage, &item_ty, &arguments);
                Type::Stream(Box::new(Type::List(Box::new(item_ty))))
            }
            StreamStageKind::Sort => {
                if stage.block.is_some() {
                    self.error(
                        stage_span,
                        "sort does not accept a block",
                        DiagnosticCode::CheckStreamStageBlock,
                    );
                }
                self.require_pipeline_key_domain(crate::sema::sealed_operation::TypeDomain::SortKey, &item_ty, stage_span);
                Type::Stream(Box::new(item_ty))
            }
            StreamStageKind::SortBy => {
                self.check_stage_no_args_arena(arena, stage);
                let callback_ty = self.check_stream_stage_callback_type(
                    arena,
                    source,
                    stage,
                    &item_ty,
                    named_callback_ty.as_ref(),
                );
                self.check_stream_stage_callback_result(
                    stage.kind,
                    &item_ty,
                    callback_ty,
                    stage_span,
                )
            }
            StreamStageKind::Take | StreamStageKind::Drop => {
                if stage.block.is_some() {
                    self.error(
                        stage_span,
                        "stage does not accept a block",
                        DiagnosticCode::CheckStreamStageBlock,
                    );
                }
                Type::Stream(Box::new(item_ty))
            }
            StreamStageKind::First | StreamStageKind::Last => {
                if !stage.args.is_empty() || stage.block.is_some() {
                    self.error(
                        stage_span,
                        "stage accepts no arguments",
                        DiagnosticCode::CheckArity,
                    );
                }
                Type::Result(Box::new(item_ty), Box::new(Type::Error))
            }
            StreamStageKind::UniqueBy => {
                self.check_stage_no_args_arena(arena, stage);
                let callback_ty = self.check_stream_stage_callback_type(
                    arena,
                    source,
                    stage,
                    &item_ty,
                    named_callback_ty.as_ref(),
                );
                self.check_stream_stage_callback_result(
                    stage.kind,
                    &item_ty,
                    callback_ty,
                    stage_span,
                )
            }
            StreamStageKind::Enumerate => {
                if !stage.args.is_empty() || stage.block.is_some() {
                    self.error(
                        stage_span,
                        "enumerate() accepts no arguments",
                        DiagnosticCode::CheckArity,
                    );
                }
                Type::Stream(Box::new(Type::Record(btree_map(vec![
                    ("index".to_string(), Type::Int),
                    ("value".to_string(), item_ty),
                ]).into())))
            }
            StreamStageKind::Zip => {
                let other_ty = arguments
                    .first()
                    .and_then(Option::as_ref)
                    .map(|argument| argument.ty.clone())
                    .unwrap_or(Type::Unknown);
                let other_item = match self.check_pipeline_input_type(other_ty, stage_span) {
                    Ok(Type::Stream(item)) => *item,
                    Err(Type::Any) => {
                        self.reject_dynamic_use("a zip source", None, arguments.first().and_then(Option::as_ref).map(|argument| argument.span).unwrap_or(stage_span));
                        Type::Unknown
                    }
                    _ => {
                        let span = arguments
                            .first()
                            .and_then(Option::as_ref)
                            .map(|argument| argument.span)
                            .unwrap_or(stage_span);
                        self.error(
                            span,
                            "zip other must be a Stream or List",
                            DiagnosticCode::CheckTypeMismatch,
                        );
                        Type::Unknown
                    }
                };
                Type::Stream(Box::new(Type::Record(btree_map(vec![
                    ("left".to_string(), item_ty),
                    ("right".to_string(), other_item),
                ]).into())))
            }
            StreamStageKind::Range => {
                if stage.block.is_some() {
                    self.error(
                        stage_span,
                        "range does not accept a block",
                        DiagnosticCode::CheckStreamStageBlock,
                    );
                }
                Type::Stream(Box::new(Type::Int))
            }
            StreamStageKind::Repeat => {
                if stage.block.is_some() {
                    self.error(
                        stage_span,
                        "repeat does not accept a block",
                        DiagnosticCode::CheckStreamStageBlock,
                    );
                }
                Type::Stream(Box::new(item_ty))
            }
            StreamStageKind::Tee => {
                self.check_stage_no_args_arena(arena, stage);
                let callback_ty = self.check_stream_stage_callback_type(
                    arena,
                    source,
                    stage,
                    &item_ty,
                    named_callback_ty.as_ref(),
                );
                self.check_stream_stage_callback_result(
                    stage.kind,
                    &item_ty,
                    callback_ty,
                    stage_span,
                )
            }
            StreamStageKind::Sum => {
                if !stage.args.is_empty() || stage.block.is_some() {
                    self.error(
                        stage_span,
                        "sum() accepts no arguments",
                        DiagnosticCode::CheckArity,
                    );
                }
                self.expect_type(&Type::Int, &item_ty, stage_span);
                Type::Int
            }
            StreamStageKind::Min | StreamStageKind::Max => {
                if !stage.args.is_empty() || stage.block.is_some() {
                    self.error(
                        stage_span,
                        "min/max accept no arguments",
                        DiagnosticCode::CheckArity,
                    );
                }
                if item_ty == Type::Any {
                    self.reject_dynamic_use("an ordering operand", None, stage_span);
                }
                Type::Result(Box::new(item_ty), Box::new(Type::Error))
            }
            StreamStageKind::GroupBy => {
                self.check_stage_no_args_arena(arena, stage);
                let callback_ty = self.check_stream_stage_callback_type(
                    arena,
                    source,
                    stage,
                    &item_ty,
                    named_callback_ty.as_ref(),
                );
                self.check_stream_stage_callback_result(
                    stage.kind,
                    &item_ty,
                    callback_ty,
                    stage_span,
                )
            }
            StreamStageKind::Fold | StreamStageKind::Reduce => {
                let acc_ty = arguments
                    .first()
                    .and_then(Option::as_ref)
                    .map(|argument| argument.ty.clone())
                    .unwrap_or(Type::Unknown);
                // A `fold`/`reduce` block binds the accumulator (typed by the
                // initial value) before the stream item, so it accepts up to
                // two parameters: `|acc, item| ...`. The tail must produce the
                // accumulator type.
                let actual =
                    self.check_fold_stream_block_arena(arena, source, stage, &acc_ty, &item_ty);
                self.expect_type(&acc_ty, &actual, stage_span);
                acc_ty
            }
            StreamStageKind::FlatMap => {
                self.check_stage_no_args_arena(arena, stage);
                let callback_ty = self.check_stream_stage_callback_type(
                    arena,
                    source,
                    stage,
                    &item_ty,
                    named_callback_ty.as_ref(),
                );
                self.check_stream_stage_callback_result(
                    stage.kind,
                    &item_ty,
                    callback_ty,
                    stage_span,
                )
            }
            StreamStageKind::Any | StreamStageKind::All => {
                self.check_stage_no_args_arena(arena, stage);
                let callback_ty = self.check_stream_stage_callback_type(
                    arena,
                    source,
                    stage,
                    &item_ty,
                    named_callback_ty.as_ref(),
                );
                self.check_stream_stage_callback_result(
                    stage.kind,
                    &item_ty,
                    callback_ty,
                    stage_span,
                )
            }
            StreamStageKind::Shuffle => {
                if stage.block.is_some() {
                    self.error(
                        stage_span,
                        "shuffle does not accept a block",
                        DiagnosticCode::CheckStreamStageBlock,
                    );
                }
                Type::Stream(Box::new(item_ty))
            }
            StreamStageKind::TablePrint => {
                self.check_table_print_stage_arena(arena, source, stage, &item_ty);
                Type::Unit
            }
            StreamStageKind::TextStreamLines
            | StreamStageKind::BytesChunks
            | StreamStageKind::JsonLines
            | StreamStageKind::JsonStream => {
                self.error(
                    stage_span,
                    "adapter stages are valid only as the first structured pipeline stage",
                    DiagnosticCode::CheckStreamAdapter,
                );
                Type::Unknown
            }
            StreamStageKind::Count => {
                if !stage.args.is_empty() {
                    self.error(
                        stage_span,
                        "count does not accept arguments",
                        DiagnosticCode::CheckArity,
                    );
                }
                if stage.block.is_some() {
                    let key_ty =
                        self.check_required_stream_block_arena(arena, source, stage, &item_ty);
                    self.require_pipeline_key_domain(crate::sema::sealed_operation::TypeDomain::CountKey, &key_ty, stage_span);
                    Type::Map(Box::new(Type::Str), Box::new(Type::Int))
                } else {
                    Type::Int
                }
            }
            StreamStageKind::Collect => {
                if !stage.args.is_empty() || stage.block.is_some() {
                    self.error(
                        stage_span,
                        "collect() accepts no arguments or block",
                        DiagnosticCode::CheckArity,
                    );
                }
                Type::List(Box::new(item_ty))
            }
            StreamStageKind::ReduceBy => {
                self.check_stage_no_args_arena(arena, stage);
                let block_ty =
                    self.check_required_stream_block_arena(arena, source, stage, &item_ty);
                let block_ty = match self.type_constraints.resolve(&block_ty) {
                    Ok(ty) => ty,
                    Err(error) => {
                        self.error(stage_span, &format!("reduction callback type cannot be resolved: {error:?}"), DiagnosticCode::CheckLocalInference);
                        return Type::Invalid;
                    }
                };
                if let Err(conflict) = self.type_constraints.project_record_field(block_ty.unvalidated(), Name::intern("key"), self.callable_solver_level(), stage_span) {
                    self.report_inferred_type_conflict(conflict);
                    return Type::Invalid;
                }
                let value_ty = match self.type_constraints.project_record_field(block_ty.unvalidated(), Name::intern("value"), self.callable_solver_level(), stage_span) {
                    Ok(ty) => ty,
                    Err(conflict) => {
                        self.report_inferred_type_conflict(conflict);
                        return Type::Invalid;
                    }
                };
                Type::Map(Box::new(Type::Str), Box::new(value_ty))
            }
        }
    }

    fn check_stream_stage_callback_type(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        stage: &ArenaStreamStage,
        item_ty: &Type,
        checked_callback_ty: Option<&Type>,
    ) -> Type {
        checked_callback_ty.cloned().unwrap_or_else(|| {
            self.check_required_stream_block_arena(arena, source, stage, item_ty)
        })
    }

    fn check_stream_stage_callback_result(
        &mut self,
        kind: StreamStageKind,
        item_ty: &Type,
        callback_ty: Type,
        stage_span: Span,
    ) -> Type {
        match kind {
            StreamStageKind::Where => {
                self.expect_type(&Type::Bool, &callback_ty, stage_span);
                Type::Stream(Box::new(item_ty.clone()))
            }
            StreamStageKind::Map => {
                if callback_ty == Type::Unit {
                    self.error(
                        stage_span,
                        "map requires a tail value",
                        DiagnosticCode::CheckMapTail,
                    );
                }
                Type::Stream(Box::new(callback_ty))
            }
            StreamStageKind::FlatMap => match callback_ty.into_unvalidated() {
                Type::List(item) | Type::Stream(item) => Type::Stream(item),
                Type::Unknown => Type::Stream(Box::new(Type::Unknown)),
                _ => {
                    self.error(
                        stage_span,
                        "flat-map blocks must produce List or Stream",
                        DiagnosticCode::CheckFlatMap,
                    );
                    Type::Stream(Box::new(Type::Unknown))
                }
            },
            StreamStageKind::Each => Type::Unit,
            StreamStageKind::Tee | StreamStageKind::UniqueBy => {
                Type::Stream(Box::new(item_ty.clone()))
            }
            StreamStageKind::SortBy => {
                self.require_pipeline_key_domain(crate::sema::sealed_operation::TypeDomain::SortKey, &callback_ty, stage_span);
                Type::Stream(Box::new(item_ty.clone()))
            }
            StreamStageKind::GroupBy => Type::Stream(Box::new(Type::Record(btree_map(vec![
                ("key".to_string(), callback_ty),
                ("items".to_string(), Type::List(Box::new(item_ty.clone()))),
            ]).into()))),
            StreamStageKind::Any | StreamStageKind::All => {
                self.expect_type(&Type::Bool, &callback_ty, stage_span);
                Type::Bool
            }
            _ => unreachable!("only unary callback stages have named callable descriptors"),
        }
    }

    fn require_pipeline_key_domain(&mut self, domain: crate::sema::sealed_operation::TypeDomain, key: &Type, span: Span) {
        if !self.defer_pipeline_key_domain(domain, key, span) { self.validate_pipeline_key_domain(domain, key, span); }
    }

    pub(super) fn validate_pipeline_key_domain(&mut self, domain: crate::sema::sealed_operation::TypeDomain, key: &Type, span: Span) {
        let key = match self.type_constraints.resolve(key) {
            Ok(key) => key,
            Err(error) => { self.error(span, &format!("pipeline key cannot be resolved: {error:?}"), DiagnosticCode::CheckLocalInference); return; }
        };
        let code = match domain { crate::sema::sealed_operation::TypeDomain::SortKey => DiagnosticCode::CheckStreamSort, crate::sema::sealed_operation::TypeDomain::CountKey => DiagnosticCode::CheckStreamCountKey, crate::sema::sealed_operation::TypeDomain::CollectItem => DiagnosticCode::CheckYieldStream };
        if key.contains_any() {
            self.reject_dynamic_use(match domain { crate::sema::sealed_operation::TypeDomain::SortKey => "a sort key", crate::sema::sealed_operation::TypeDomain::CountKey => "a count key", crate::sema::sealed_operation::TypeDomain::CollectItem => "a collected item" }, None, span);
            return;
        }
        if !domain.may_accept(&key) {
            self.error(span, &format!("{domain:?} does not support {key}"), code);
            return;
        }
        if !key.contains_inference() { return; }
        let owner = self.stream_origin_owner.map_or(crate::sema::constraints::CallableApplicationScope::Invocation, crate::sema::constraints::CallableApplicationScope::Declaration);
        if let Err(error) = self.type_constraints.require_type_predicate(owner, domain, &key, span) {
            self.error(span, &error.to_string(), code);
        }
    }

    fn check_required_stream_block_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        stage: &ArenaStreamStage,
        item_ty: &Type,
    ) -> Type {
        let Some(block) = stage.block else {
            self.error(
                arena.arena.span(stage.span),
                "stream stage requires a block",
                DiagnosticCode::CheckStreamStageBlock,
            );
            return Type::Unknown;
        };
        self.check_stream_block_params_arena(
            arena,
            source,
            block,
            std::slice::from_ref(item_ty),
            1,
            item_ty,
            matches!(stage.kind, StreamStageKind::Each | StreamStageKind::Tee)
                .then_some(&Type::Unit),
        )
    }

    pub(super) fn stage_callable_is_static(&self, arena: &ArenaProgram, callee: ExprId) -> bool {
        if self.resolve_callable_alias_call(arena, callee).is_some() {
            return true;
        }
        match arena.arena.expr(callee).kind {
            ArenaExprKind::Ident(name) => {
                matches!(
                    self.lookup_value_binding_for_expr(arena, callee),
                    super::ValueBindingLookup::NoSite
                )
                    && (self.pures.contains_key(&name) || self.procs.contains_key(&name))
            }
            ArenaExprKind::Field { base, name } => {
                let ArenaExprKind::Ident(namespace) = arena.arena.expr(base).kind else {
                    return false;
                };
                let namespace_binding = self.lookup_value_binding_for_expr(arena, base);
                if matches!(namespace_binding, super::ValueBindingLookup::Missing) {
                    return false;
                }
                let nested_namespace_binding = match namespace_binding {
                    super::ValueBindingLookup::Found(selected) => self
                        .binding_scope(namespace, selected)
                        .is_none_or(|index| index > 0),
                    super::ValueBindingLookup::NoSite => self
                        .scopes
                        .iter()
                        .skip(1)
                        .any(|scope| scope.contains_key(&namespace)),
                    super::ValueBindingLookup::Missing => true,
                };
                if nested_namespace_binding {
                    return false;
                }
                let qualified = crate::symbol::QualifiedName::new(namespace, name);
                let static_namespace = matches!(
                    namespace_binding,
                    super::ValueBindingLookup::Found(binding)
                        if binding.static_namespace
                ) || matches!(namespace_binding, super::ValueBindingLookup::NoSite);
                static_namespace
                    && (self.qualified_pures.contains_key(&qualified)
                        || self.qualified_procs.contains_key(&qualified))
                    || crate::sema::stage_arguments::stage_namespace_is_imported(
                        arena,
                        namespace,
                        self.current_namespace,
                        self.bundle_root_statements,
                        self.bundle_module_keys.as_deref(),
                    ) && matches!(namespace_binding,
                        super::ValueBindingLookup::Found(binding)
                            if matches!(&binding.ty, Type::Module(exports)
                                if exports.get(&name).is_some_and(|export| matches!(export, crate::sema::types::ModuleExportType::Callable { .. }))))
                    || matches!(namespace_binding, super::ValueBindingLookup::NoSite)
                        && super::api_spec()
                            .module(&namespace.as_str())
                            .is_some_and(|module| {
                                module
                                    .functions
                                    .iter()
                                    .any(|function| function.name == name.as_str().as_str())
                            })
            }
            _ => false,
        }
    }

    /// `fold`/`reduce` blocks bind the accumulator (typed by the stage's
    /// initial value) before the stream item, so the block may take up to two
    /// parameters: `fold(init) { |acc, item| ... }`. The tail must still
    /// produce the accumulator type.
    fn check_fold_stream_block_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        stage: &ArenaStreamStage,
        acc_ty: &Type,
        item_ty: &Type,
    ) -> Type {
        let Some(block) = stage.block else {
            self.error(
                arena.arena.span(stage.span),
                "stream stage requires a block",
                DiagnosticCode::CheckStreamStageBlock,
            );
            return Type::Unknown;
        };
        self.check_stream_block_params_arena(
            arena,
            source,
            block,
            &[acc_ty.clone(), item_ty.clone()],
            2,
            item_ty,
            Some(acc_ty),
        )
    }

    fn check_stream_block_params_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block_id: crate::syntax::arena::BlockId,
        param_tys: &[Type],
        max_params: usize,
        item_ty: &Type,
        expected: Option<&Type>,
    ) -> Type {
        let block = arena.arena.block(block_id);
        let params = arena.arena.block_params(block.params);
        if params.len() > max_params {
            self.error(
                arena.arena.span(params[max_params].span),
                if max_params == 1 {
                    "stream stage blocks accept at most one parameter"
                } else {
                    "fold/reduce blocks accept at most two parameters (accumulator, item)"
                },
                DiagnosticCode::CheckStreamBlockParams,
            );
        }
        self.push_deferred_capture_scope();
        let enclosing_boundary = self.enter_callback_boundary();
        let mut names = FxHashSet::default();
        for (index, param) in params.iter().take(max_params).enumerate() {
            if !names.insert(param.name) {
                self.error(
                    arena.arena.span(param.span),
                    "duplicate name in scope",
                    DiagnosticCode::CheckDuplicateName,
                );
            }
            let ty = param_tys.get(index).cloned().unwrap_or(Type::Unknown);
            self.define(
                param.name,
                Binding::new(ty, false),
                arena.arena.span(param.span),
            );
        }
        // `.` is the item only where it is the block's sole, unnamed parameter.
        if max_params == 1 && params.is_empty() {
            self.define(
                super::item_binding(),
                Binding::new(item_ty.clone(), false),
                arena.arena.span(block.span),
            );
        }
        self.boundary.item_frames.push(if max_params > 1 {
            super::ItemFrame::Named {
                param: super::NamedItem::Accumulated,
                stage: true,
            }
        } else if let Some(param) = params.first() {
            super::ItemFrame::Named {
                param: if param.name == "_" {
                    super::NamedItem::Discarded
                } else {
                    super::NamedItem::Param(param.name)
                },
                stage: true,
            }
        } else {
            super::ItemFrame::Implicit {
                ty: item_ty.clone(),
                used: false,
                stage: true,
            }
        });
        let mut tail_ty = Type::Unit;
        let stmt_ids: Vec<_> = arena.arena.stmt_ids(block.statements).collect();
        for (index, stmt_id) in stmt_ids.iter().enumerate() {
            if index + 1 == stmt_ids.len() {
                tail_ty = self.check_tail_stmt_arena(arena, source, *stmt_id, expected);
            } else {
                self.check_stmt_arena(arena, source, *stmt_id);
            }
        }
        self.boundary.item_frames.pop();
        self.leave_callback_boundary(enclosing_boundary);
        self.pop_scope();
        tail_ty
    }

    fn check_stage_arguments_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        stage: &ArenaStreamStage,
    ) -> Vec<Option<crate::sema::arguments::ExpandedArgument>> {
        self.check_stage_arguments_excluding_callable_arena(arena, source, stage, None)
    }

    fn check_stage_arguments_excluding_callable_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        stage: &ArenaStreamStage,
        callable_entry: Option<usize>,
    ) -> Vec<Option<crate::sema::arguments::ExpandedArgument>> {
        use crate::sema::arguments::{
            ArgumentValueSource, bind_static_arguments, expand_named_arguments,
        };
        use xsh_registry::stream_parameters::{StageParameterValidation, stage_parameters};
        let contract = stage_parameters(stage.kind.as_str());
        let params = crate::sema::stage_arguments::stage_argument_params(stage.kind.as_str());
        if params.is_empty() {
            return Vec::new();
        }
        let args = arena
            .arena
            .call_args(stage.args)
            .iter()
            .enumerate()
            .filter(|(index, _)| Some(*index) != callable_entry)
            .map(|(_, argument)| argument.clone())
            .collect::<Vec<_>>();
        let mut positional = 0;
        for arg in &args {
            let expected = match arg.kind {
                ArenaCallArgKind::Named { name, .. } => params
                    .iter()
                    .find(|param| param.name == name)
                    .map(|param| &param.ty),
                ArenaCallArgKind::Positional(_) => {
                    let expected = params.get(positional).map(|param| &param.ty);
                    positional += 1;
                    expected
                }
                _ => None,
            };
            // A Value or Sequence parameter has no expected type; passing its
            // Unknown placeholder made branch expressions (`fold(if ...)`)
            // adopt Unknown instead of their own type.
            let expected = expected.filter(|ty| **ty != Type::Unknown);
            if let ArenaCallArgKind::NamedSpread { value, .. } = arg.kind {
                self.check_expr_arena(arena, source, value, None);
            } else {
                self.check_call_arg_arena(arena, source, &arg.kind, expected);
            }
        }
        let expanded = match expand_named_arguments(arena, &args, |expr| {
            self.expr_types.get(&arena.arena.expr(expr).span).cloned()
        }) {
            Ok(expanded) => expanded,
            Err(error) => {
                self.error(error.span, &error.message, DiagnosticCode::CheckNamedSpread);
                return vec![None; params.len()];
            }
        };
        let binding = match bind_static_arguments(&params, &expanded) {
            Ok(binding) => binding,
            Err(error) => {
                let span = if args.is_empty() {
                    arena.arena.span(stage.span)
                } else {
                    error.span
                };
                self.error(span, &error.message, DiagnosticCode::CheckNamedArg);
                return vec![None; params.len()];
            }
        };
        self.argument_bindings
            .entry(arena.arena.span(stage.span))
            .or_default()
            .argument_slots = binding.argument_slots.clone();
        let mut values = vec![None; params.len()];
        for (argument, slot) in expanded.into_iter().zip(binding.argument_slots) {
            if argument.name.is_none() && !contract[slot].positional {
                self.error(
                    argument.span,
                    "stage configuration parameters must be named",
                    DiagnosticCode::CheckNamedArg,
                );
            }
            self.expect_type(&params[slot].ty, &argument.ty, argument.span);
            if contract[slot].validation == StageParameterValidation::Positive
                && let ArgumentValueSource::Expression(value) = argument.value
            {
                let code = match contract[slot].name {
                    "jobs" => DiagnosticCode::CheckStreamJobs,
                    "size" => DiagnosticCode::CheckBytesChunks,
                    _ => DiagnosticCode::CheckStreamBatch,
                };
                self.check_static_positive_value_arena(arena, value, code);
            }
            values[slot] = Some(argument);
        }
        let literal_bool =
            |argument: &crate::sema::arguments::ExpandedArgument| match argument.value {
                ArgumentValueSource::Expression(expr) => match arena.arena.expr(expr).kind {
                    ArenaExprKind::Bool(value) => Some(value),
                    _ => None,
                },
                _ => None,
            };
        if stage.kind == StreamStageKind::ReduceBy {
            let modes = &values[..3];
            let enabled = modes
                .iter()
                .flatten()
                .filter(|argument| literal_bool(argument) == Some(true))
                .count();
            let dynamic = modes
                .iter()
                .flatten()
                .any(|argument| literal_bool(argument).is_none());
            if enabled > 1 || !dynamic && enabled != 1 {
                self.error(
                    arena.arena.span(stage.span),
                    "reduce-by requires exactly one enabled reduction mode",
                    DiagnosticCode::CheckStreamReduceMode,
                );
            }
        }
        if stage.kind == StreamStageKind::Batch
            && values[0].is_none()
            && values[1].is_none()
            && values[2]
                .as_ref()
                .is_none_or(|argument| literal_bool(argument) == Some(false))
        {
            self.error(
                arena.arena.span(stage.span),
                "batch requires an enabled count or byte limit",
                DiagnosticCode::CheckStreamBatch,
            );
        }
        values
    }

    fn check_stage_no_args_arena(&mut self, arena: &ArenaProgram, stage: &ArenaStreamStage) {
        let callable_entry = self
            .argument_bindings
            .get(&arena.arena.span(stage.span))
            .and_then(|binding| binding.callable_entry);
        self.check_stage_no_args_excluding_callable_arena(arena, stage, callable_entry);
    }

    fn check_stage_no_args_excluding_callable_arena(
        &mut self,
        arena: &ArenaProgram,
        stage: &ArenaStreamStage,
        callable_entry: Option<usize>,
    ) {
        if arena
            .arena
            .call_args(stage.args)
            .iter()
            .enumerate()
            .any(|(index, _)| Some(index) != callable_entry)
            && xsh_registry::stream_parameters::stage_parameters(stage.kind.as_str()).is_empty()
        {
            self.error(
                arena.arena.span(stage.span),
                "stream stage does not accept call arguments",
                DiagnosticCode::CheckArity,
            );
        }
    }

    fn check_batch_stage_arena(
        &mut self,
        arena: &ArenaProgram,
        _source: &str,
        stage: &ArenaStreamStage,
        item_ty: &Type,
        arguments: &[Option<crate::sema::arguments::ExpandedArgument>],
    ) {
        self.check_stage_no_args_arena(arena, stage);
        if stage.block.is_some() {
            self.error(
                arena.arena.span(stage.span),
                "batch does not accept a block",
                DiagnosticCode::CheckStreamStageBlock,
            );
        }
        for (slot, argument) in arguments
            .iter()
            .enumerate()
            .skip(1)
            .filter_map(|(slot, argument)| argument.as_ref().map(|argument| (slot, argument)))
        {
            if slot == 2
                && let crate::sema::arguments::ArgumentValueSource::Expression(expr) =
                    argument.value
                && matches!(arena.arena.expr(expr).kind, ArenaExprKind::Bool(false))
            {
                continue;
            }
            if *item_ty == Type::Any {
                self.reject_dynamic_use("a byte-bounded batch item", None, argument.span);
            } else if !self.require_value_conversion(item_ty, crate::sema::schemes::ValueConversion::Argv, argument.span) {
                self.error(
                    argument.span,
                    "byte-bounded batches require argv-compatible items",
                    DiagnosticCode::CheckStreamBatch,
                );
            }
        }
    }

    fn check_table_print_stage_arena(
        &mut self,
        arena: &ArenaProgram,
        _source: &str,
        stage: &ArenaStreamStage,
        item_ty: &Type,
    ) {
        let stage_span = arena.arena.span(stage.span);
        if stage.block.is_some() {
            self.error(
                stage_span,
                "table.print does not accept a block",
                DiagnosticCode::CheckStreamStageBlock,
            );
        }
        let item_ty = match self.type_constraints.resolve(item_ty) {
            Ok(ty) => ty,
            Err(error) => {
                self.error(stage_span, &format!("table row type cannot be resolved: {error:?}"), DiagnosticCode::CheckLocalInference);
                return;
            }
        };
        if matches!(item_ty.unvalidated(), Type::Inference(_)) {
            if let Err(conflict) = self.type_constraints.require_record_head(item_ty.unvalidated(), self.callable_solver_level(), stage_span) {
                self.report_inferred_type_conflict(conflict);
            }
        } else if !matches!(item_ty.unvalidated(), Type::Record(_) | Type::Unknown) {
            self.error(
                stage_span,
                "table.print requires record stream items",
                DiagnosticCode::CheckTablePrint,
            );
        }
    }

    fn check_adapter_stage_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        stage: &ArenaStreamStage,
        input_ty: Type,
    ) -> Type {
        let stage_span = arena.arena.span(stage.span);
        let _arguments = self.check_stage_arguments_arena(arena, source, stage);
        if stage.block.is_some() {
            self.error(
                stage_span,
                "adapter stages do not accept blocks",
                DiagnosticCode::CheckStreamStageBlock,
            );
        }
        match stage.kind {
            StreamStageKind::TextStreamLines => {
                self.check_stage_no_args_arena(arena, stage);
                self.expect_type(&Type::Str, &input_ty, stage_span);
                Type::Stream(Box::new(Type::Str))
            }
            StreamStageKind::BytesChunks => {
                self.expect_type(&Type::Bytes, &input_ty, stage_span);
                Type::Stream(Box::new(Type::Bytes))
            }
            StreamStageKind::JsonLines | StreamStageKind::JsonStream => {
                self.check_stage_no_args_arena(arena, stage);
                self.expect_type(&Type::Str, &input_ty, stage_span);
                Type::Stream(Box::new(Type::Any))
            }
            _ => unreachable!("adapter stage"),
        }
    }

    fn check_static_positive_value_arena(
        &mut self,
        arena: &ArenaProgram,
        expr_id: ExprId,
        code: DiagnosticCode,
    ) {
        let expr = arena.arena.expr(expr_id);
        match &expr.kind {
            ArenaExprKind::Int(value)
                if arena
                    .arena
                    .int_literal(*value)
                    .value()
                    .is_some_and(|value| value <= 0) =>
            {
                self.error(expr.span, "stream option must be positive", code);
            }
            ArenaExprKind::Unary {
                op: UnaryOp::Neg,
                expr: inner,
            } if matches!(arena.arena.expr(*inner).kind, ArenaExprKind::Int(_)) => {
                self.error(expr.span, "stream option must be positive", code);
            }
            _ => {}
        }
    }
}

fn stream_type_from_input(ty: Type) -> Result<Type, Type> {
    let ty = ty.into_unvalidated();
    let source = match &ty { Type::Result(ok, _) => ok.unvalidated(), source => source };
    match source {
        Type::Unknown => Ok(Type::Stream(Box::new(Type::Unknown))),
        Type::Any => Err(Type::Any),
        Type::List(_) | Type::Set(_) | Type::Stream(_) => ty.iteration_item_type()
            .map(|item| Type::Stream(Box::new(item))).ok_or(ty),
        _ => Err(ty),
    }
}

#[cfg(test)]
mod inferred_sort_key_tests {
    use crate::diagnostic::{DiagnosticCode, Severity};
    use crate::sema::check::Checker;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    #[test]
    fn accumulated_nullable_record_fields_preserve_sortable_names() {
        for annotation in [": List[Row]", ""] {
            let source = format!("type Row = {{name: Str, revision: Str?}}\nproc ordered() -> Result[List[Row], Error] {{ var rows{annotation} = []; for name in [\"b\", \"a\"] {{ rows += [{{name, revision: null}}] }}; rows |> sort-by .name }}\nlet rows = ordered()?\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == Severity::Error).collect::<Vec<_>>();
            assert!(errors.is_empty(), "{source}\n{errors:?}");
            let start = source.find("sort-by .name").unwrap() + 8;
            assert_eq!(checked.expr_types.get(&crate::source::Span::new(SourceId::new(0), start, start + 5)), Some(&super::Type::Str));
        }
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn inferred_sort_key_native_behavior_runs_in_both_evaluator_routes() {
        let source = include_str!("../../../tests/xsh/inferred-sort-keys.xsh");
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            "inferred-sort-keys.xsh",
            crate::loader::entry_source_from_text("inferred-sort-keys.xsh", source.to_owned()),
            Vec::new(),
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources)
            .prepare_test_program(std::sync::Arc::new(parsed.arena), source_id, checked)
            .unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("inferred_sort_keys_wait_for_the_body_result_context", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(matches!(&result.result, Some(crate::runtime::value::Value::Unit))
                || matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
        }
    }

    #[test]
    fn deferred_sort_key_domains_reject_unsupported_and_dynamic_siblings() {
        for (key_type, key_value, expected) in [
            ("Float", "1.5", DiagnosticCode::CheckStreamSort),
            ("List[Str]", "[\"a\"]", DiagnosticCode::CheckStreamSort),
            ("Str?", "null", DiagnosticCode::CheckStreamSort),
            ("Any", "key", DiagnosticCode::CheckDynamicBoundary),
        ] {
            let source = format!("type ValidRow = {{name: Str, revision: Str?}}\nproc valid() -> Result[List[ValidRow], Error] {{ var rows = []; rows += [{{name: \"a\", revision: null}}]; rows |> sort-by .name }}\ntype InvalidRow = {{name: {key_type}, revision: Str?}}\npure invalid(key: Any) -> List[InvalidRow] {{ var rows = []; rows += [{{name: {key_value}, revision: null}}]; rows |> sort-by .name }}\nlet rows = valid()?\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == Severity::Error).collect::<Vec<_>>();
            assert_eq!(errors.len(), 1, "{source}\n{errors:?}");
            assert_eq!(errors[0].code, Some(expected), "{source}\n{errors:?}");
            assert!(errors[0].labels.iter().all(|label| label.span.start() > source.find("pure invalid").unwrap()), "{errors:?}");
        }
    }

    #[test]
    fn generic_sort_keys_preserve_independent_supported_domains() {
        let source = "pure ordered(item) { [item] |> sort-by . }\nlet numbers = ordered(7)\nlet words = ordered(\"kept\")\nlet flags = ordered(true)\nlet paths = ordered(p\"kept\")\nlet records = ordered({name: \"kept\", nested: {flag: true}})\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error), "{:?}", checked.diagnostics);
    }

    #[test]
    fn generic_key_predicates_reject_distinct_invalid_domains_and_row_tails() {
        for (body, rejected) in [
            ("[value] |> sort", "Float"),
            ("[value] |> sort", "UInt"),
            ("[value] |> sort", "Bytes"),
            ("[value] |> sort", "List[Int]"),
            ("[value] |> sort", "Int?"),
            ("[value] |> sort", "Any"),
            ("[value] |> sort", "Action"),
            ("[value] |> sort", "FsLock"),
            ("[value] |> count { |item| item }", "Path"),
            ("[value] |> count { |item| item }", "RecordKey"),
            ("[value] |> count { |item| item }", "Any"),
            ("[value] |> count { |item| item }", "Action"),
            ("[value] |> count { |item| item }", "FsLock"),
        ] {
            let source = format!("type Action = pure(value: Int) -> Int\ntype RecordKey = {{name: Str}}\npure keyed(value) {{ {body} }}\npure rejected(value: {rejected}) {{ keyed(value) }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error), "{source}\n{:?}", checked.diagnostics);
        }
        for extra in ["List[Int]", "Any", "FsLock", "Action"] {
            let source = format!("type Action = pure(value: Int) -> Int\ntype Row = {{name: Str, extra: {extra}}}\npure ordered(row) {{ let _ = row.name; [row] |> sort }}\npure rejected(row: Row) {{ ordered(row) }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error), "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn generic_key_domain_intersections_preserve_all_caller_requirements() {
        for (value_type, valid) in [("Int", true), ("Str", true), ("Bool", true), ("UInt", false), ("Path", false), ("RecordKey", false)] {
            let source = format!("type RecordKey = {{name: Str}}\npure both(value) {{ let counts = [value] |> count {{ |item| item }}; let ordered = [value] |> sort; {{counts, ordered}} }}\npure observed(value: {value_type}) {{ both(value) }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert_eq!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error), valid, "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn generic_key_predicates_intersect_with_the_existing_map_key_bound() {
        use crate::sema::sealed_operation::TypeDomain;
        use crate::sema::types::Type;
        for (domain, actual, valid) in [
            (TypeDomain::SortKey, Type::Path, true),
            (TypeDomain::SortKey, Type::Bool, true),
            (TypeDomain::SortKey, Type::UInt, false),
            (TypeDomain::SortKey, Type::Bytes, false),
            (TypeDomain::SortKey, Type::Duration, false),
            (TypeDomain::CountKey, Type::UInt, true),
            (TypeDomain::CountKey, Type::Bool, true),
            (TypeDomain::CountKey, Type::Path, false),
            (TypeDomain::CountKey, Type::Bytes, false),
            (TypeDomain::CountKey, Type::Duration, false),
        ] {
            let mut constraints = crate::sema::constraints::TypeConstraints::default();
            let span = crate::source::Span::at(SourceId::new(0), 0);
            let key = constraints.fresh(span);
            constraints.require_map_key(&key, span).unwrap();
            constraints.require_type_predicate(crate::sema::constraints::CallableApplicationScope::Invocation, domain, &key, span).unwrap();
            let accepted = constraints.constrain(&key, &actual, span).is_ok() && constraints.solve_sealed_operations().is_ok();
            assert_eq!(accepted, valid, "{domain:?} and MapKey with {actual}");
        }
    }

    #[test]
    fn generic_key_predicates_reject_impossible_known_shapes_before_calls() {
        for body in [
            "[value] |> sort-by { |item| {bad: [item]} }",
            "[value] |> count { |item| [item] }",
            "[value] |> count { |item| {key: item} }",
        ] {
            let source = format!("pure rejected(value) {{ {body} }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error), "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn generic_key_domains_execute_after_disposal_on_both_routes() {
        let source = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime/generic-pipeline-key-domains.xsh")).unwrap();
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("generic-pipeline-key-domains.xsh", crate::loader::entry_source_from_text("generic-pipeline-key-domains.xsh", source), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.source_text_source_id().unwrap();
        let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(std::sync::Arc::new(parsed.arena), source_id, checked).unwrap();
        for force_frames in [false, true] {
            let result = prepared.eval_test("generic_key_domains", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
            assert_eq!(result.output.status, 0, "force_frames={force_frames}: {:?} {:?}", result.output.diagnostics, result.output.traceback);
            assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
            assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
            assert!(matches!(&result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)), "force_frames={force_frames}: {:?}", result.result);
        }
    }
}

#[cfg(test)]
mod static_namespace_identity_tests {
    use super::{Checker, Type};
    use crate::sema::check::{CheckOptions, ResolvedBindings};
    use crate::source::SourceId;
    use crate::symbol::{Name, QualifiedName};
    use crate::syntax::arena::{ArenaExprKind, ArenaExprOrRun, ArenaStmtKind};
    use crate::syntax::parser::Parser;
    use std::sync::Arc;

    fn checker(program: &crate::syntax::arena::ArenaProgram) -> Checker {
        let mut checker = Checker::new(CheckOptions::default());
        checker.resolved_bindings = Arc::new(ResolvedBindings::collect(program));
        checker
    }

    fn namespace_field(
        program: &crate::syntax::arena::ArenaProgram,
        statement: crate::syntax::arena::StmtId,
    ) -> (crate::syntax::arena::ExprId, crate::syntax::arena::ExprId) {
        let ArenaStmtKind::Let {
            initializer: ArenaExprOrRun::Expr(expression),
            ..
        } = program.arena.stmt(statement).kind
        else {
            panic!("expected let initializer expression");
        };
        let ArenaExprKind::Field { base, .. } = program.arena.expr(expression).kind else {
            panic!("expected field expression");
        };
        (expression, base)
    }

    fn install_static_stage(checker: &mut Checker, namespace: Name) {
        checker
            .current_scope_mut()
            .get_mut(&namespace)
            .unwrap()
            .static_namespace = true;
        Arc::make_mut(&mut checker.qualified_pures).insert(
            QualifiedName::new(namespace, Name::intern("stage")),
            super::super::FunctionSig::ground(
                crate::sema::types::CallableKind::Pure,
                None,
                false,
                None,
                None,
                Vec::new(),
                Type::Int,
                None,
                Some(Vec::new()),
                false,
                true,
            )
            .expect("a static stage has a ground signature"),
        );
    }

    #[test]
    fn root_static_namespace_ignores_different_id_shadow() {
        let source = "let namespace = 1\nlet result = namespace.stage\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let program = &parsed.arena;
        let statements = program.arena.stmt_ids(program.statements).collect::<Vec<_>>();

        program.symbol_owner().with_current(|| {
            let mut checker = checker(program);
            checker.check_stmt_arena(program, source, statements[0]);
            let namespace = Name::intern("namespace");
            install_static_stage(&mut checker, namespace);
            let (field, base) = namespace_field(program, statements[1]);

            assert!(checker.stage_callable_is_static(program, field));
            let decoy = super::super::Binding::new(Type::Int, false);
            checker.push_scope();
            checker.current_scope_mut().insert(namespace, decoy);

            assert!(matches!(
                checker.lookup_value_binding_for_expr(program, base),
                super::super::ValueBindingLookup::Found(binding)
                    if binding.static_namespace
            ));
            assert!(checker.stage_callable_is_static(program, field));
        });
    }

    #[test]
    fn selected_inner_static_namespace_remains_nonstatic() {
        let source = "let namespace = 1\nlet result = namespace.stage\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let program = &parsed.arena;
        let statements = program.arena.stmt_ids(program.statements).collect::<Vec<_>>();

        program.symbol_owner().with_current(|| {
            let mut checker = checker(program);
            checker.check_stmt_arena(program, source, statements[0]);
            let namespace = Name::intern("namespace");
            install_static_stage(&mut checker, namespace);
            let (field, _) = namespace_field(program, statements[1]);
            let selected = checker.current_scope_mut().remove(&namespace).unwrap();
            checker.push_scope();
            checker.current_scope_mut().insert(namespace, selected);

            assert!(!checker.stage_callable_is_static(program, field));
        });
    }
}

#[cfg(test)]
mod pipeline_input_inference_tests {
    fn accepts_collect(definition: &str, argument: &str) {
        let source = format!("{definition}\nlet observed: List[Int] = collect_values({argument})\nassert observed == [1, 2]\n");
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = crate::sema::check::Checker::check_arena_with_options(&parsed.arena, &source, crate::sema::check::CheckOptions::for_execution());
        let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error).collect::<Vec<_>>();
        assert!(errors.is_empty(), "{source}\n{errors:?}");
    }

    #[test]
    fn typed_list_pipeline_input_preserves_collected_items() {
        accepts_collect("pure collect_values(values: List[Int]) -> List[Int] { values |> collect }", "[1, 2]");
    }

    #[test]
    fn inferred_pipeline_input_preserves_collected_items() {
        accepts_collect("pure collect_values(values) { values |> collect }", "[1, 2]");
    }

    #[test]
    fn known_typed_list_pipeline_source_preserves_record_items() {
        let source = r#"pure shorthand(value: Int) -> List[Int] { [{payload: value}] |> map .payload }
let integers: List[Int] = shorthand(7)
assert integers == [7]
"#;
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = crate::sema::check::Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions::for_execution());
        let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error).collect::<Vec<_>>();
        assert!(errors.is_empty(), "{source}\n{errors:?}");
    }

    #[test]
    fn known_list_pipeline_source_preserves_record_item_binders() {
        let source = r#"pure shorthand(value) { [{payload: value}] |> map .payload }
let integers: List[Int] = shorthand(7)
let words: List[Str] = shorthand("kept")
"#;
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = crate::sema::check::Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions::for_execution());
        let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error).collect::<Vec<_>>();
        assert!(errors.is_empty(), "{source}\n{errors:?}");
    }

    #[test]
    fn inferred_pipeline_input_rejects_non_pipeline_iteration_domains() {
        for source_type in ["Map[Str, Int]", "Str", "Bytes", "Any"] {
            let source = format!("pure collect_values(values) {{ values |> collect }}\npure rejected(values: {source_type}) {{ collect_values(values) }}\n");
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = crate::sema::check::Checker::check_arena_with_options(&parsed.arena, &source, crate::sema::check::CheckOptions::for_execution());
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error), "{source}");
        }
    }

    #[test]
    fn inferred_pipeline_input_preserves_result_error_payload_domains() {
        accepts_collect("pure collect_values(values) { values |> collect }\npure source_values() -> Result[List[Int], Str] { Ok([1, 2]) }", "source_values()");
    }

    #[test]
    fn typed_pipeline_input_rejects_nested_result_source_views() {
        for payload in ["List[Int]", "Stream[Int]"] {
            let source = format!(r#"pure collect_values(values: Result[Result[{payload}]]) -> List[Int] {{ values |> collect }}
"#);
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = crate::sema::check::Checker::check_arena_with_options(&parsed.arena, &source, crate::sema::check::CheckOptions::for_execution());
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckStreamInput)), "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn typed_pipeline_input_keeps_result_items_opaque_after_one_source_view() {
        let source = r#"pure collect_values(values: Result[List[Result[Int, Str]]]) -> List[Result[Int, Str]] { values |> collect }
let values: Result[List[Result[Int, Str]]] = Ok([Err("data")])
let observed: List[Result[Int, Str]] = collect_values(values)
assert match observed[0] { Ok(_) => false, Err(message) => message == "data" }
"#;
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = crate::sema::check::Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions::for_execution());
        let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error).collect::<Vec<_>>();
        assert!(errors.is_empty(), "{source}\n{errors:?}");
    }

    #[test]
    fn typed_set_pipeline_input_keeps_its_existing_domain() {
        accepts_collect("pure collect_values(values: Set[Int]) -> List[Int] { values |> collect }", "{1, 2}");
    }

    #[test]
    fn typed_result_list_pipeline_input_keeps_its_existing_domain() {
        accepts_collect("pure collect_values(values: Result[List[Int]]) -> List[Int] { values |> collect }", "Ok([1, 2])");
    }

    #[test]
    fn typed_stream_pipeline_input_keeps_its_existing_domain() {
        accepts_collect("stream source_values() [] -> Stream[Int] { yield 1; yield 2 }\npure collect_values(values: Stream[Int]) -> List[Int] { values |> collect }", "source_values()");
    }

    #[test]
    fn typed_result_stream_pipeline_input_keeps_its_existing_domain() {
        accepts_collect("stream source_values() [] -> Stream[Int] { yield 1; yield 2 }\npure collect_values(values: Result[Stream[Int]]) -> List[Int] { values |> collect }", "Ok(source_values())");
    }
}
