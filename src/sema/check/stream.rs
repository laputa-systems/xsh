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
            match stream_type_from_input(input_ty) {
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
                {
                    if !self.check_stream_callable_module_overload(
                        checked_argument,
                        overloads,
                        item_span,
                    ) {
                        return Type::Unknown;
                    }
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
                if matches!(
                    item_ty.unvalidated(),
                    Type::Int | Type::Str | Type::Bool | Type::Path | Type::Unknown
                ) || is_sortable_record_key_type(&item_ty)
                {
                    Type::Stream(Box::new(item_ty))
                } else {
                    self.error(
                        stage_span,
                        "sort items must be Int, Str, Bool, Path, or a record of supported items",
                        DiagnosticCode::CheckStreamSort,
                    );
                    Type::Stream(Box::new(item_ty))
                }
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
                let other_item = match stream_type_from_input(other_ty) {
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
                    if key_ty == Type::Any {
                        self.reject_dynamic_use("a count key", None, stage_span);
                    } else if !matches!(
                        key_ty.unvalidated(),
                        Type::Str
                            | Type::Int
                            | Type::UInt
                            | Type::Bool
                            | Type::Unknown
                            | Type::Invalid
                    ) {
                        self.error(
                            stage_span,
                            "count keys must be Str, Int, or Bool",
                            DiagnosticCode::CheckStreamCountKey,
                        );
                    }
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
                let expected = Type::Record(btree_map(vec![
                    ("key".to_string(), Type::Any),
                    ("value".to_string(), Type::Any),
                ]).into());
                self.expect_type(&expected, &block_ty, stage_span);
                let value_ty = match &block_ty {
                    Type::Record(fields) => fields
                        .get(&Name::intern("value"))
                        .cloned()
                        .unwrap_or(Type::Unknown),
                    _ => Type::Unknown,
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
                if callback_ty.contains_any() {
                    self.reject_dynamic_use("a sort key", None, stage_span);
                } else if !is_sortable_key_type(&callback_ty) {
                    self.error(
                        stage_span,
                        "sort-by keys must be Int, Str, Bool, Path, or a record of supported keys",
                        DiagnosticCode::CheckStreamSort,
                    );
                }
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
                                if exports.get(&name).is_some_and(|export| matches!(export, crate::sema::types::ModuleExportType::Pure { .. } | crate::sema::types::ModuleExportType::Proc { .. }))))
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
            } else if !item_ty.can_be_argv_item() && !matches!(item_ty, Type::Unknown) {
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
        if !matches!(item_ty, Type::Record(_) | Type::Unknown) {
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
            super::super::FunctionSig {
                effect_declaration: None,
                inferred_effects: false,
                explicit_return: true,
                is_alias: false,
                definition: None,
                definition_target: None,
                params: Vec::new(),
                return_ty: Type::Int,
                return_schema: None,
                effects: Some(Vec::new()),
            },
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

// Keep a rejected source's type so opacity has its own diagnostic instead
// of being confused with a concrete value that is not iterable.
fn stream_type_from_input(ty: Type) -> Result<Type, Type> {
    match ty.into_unvalidated() {
        ty @ Type::Stream(_) => Ok(ty),
        Type::List(item) | Type::Set(item) => Ok(Type::Stream(item)),
        Type::Result(ok, _) => stream_type_from_input(*ok),
        Type::Unknown => Ok(Type::Stream(Box::new(Type::Unknown))),
        other => Err(other),
    }
}

/// Whether a projected `sort-by` key or `sort` item type has a defined
/// ordering. Records are orderable when every field is itself orderable; the
/// runtime comparator in `lowered_ops.rs` implements the same surface so a
/// checked program and an unchecked `xsh` run agree on what can sort. An
/// `Any` key is a dynamic boundary, rejected before this check.
fn is_sortable_key_type(ty: &Type) -> bool {
    match ty.unvalidated() {
        Type::Int | Type::Str | Type::Bool | Type::Path | Type::Unknown => true,
        Type::Record(fields) => fields.values().all(is_sortable_key_type),
        _ => false,
    }
}

fn is_sortable_record_key_type(ty: &Type) -> bool {
    matches!(ty, Type::Record(fields) if fields.values().all(is_sortable_key_type))
}
