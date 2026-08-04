use super::{
    Binding, Checker, Name, call_arg_expr_id_arena, call_arg_span_arena,
    command_stmt_asserts_success_arena, command_ty_auto_propagates,
};
use crate::sema::types::Type;
use crate::syntax::arena::{
    ArenaCallArgKind, ArenaExprKind, ArenaProgram, ArenaStreamStage, ArenaStreamStageOption, ExprId,
};
use crate::syntax::node::{StreamStageKind, UnaryOp};
use std::collections::BTreeMap;

fn btree_map<K: Into<Name>, V>(entries: Vec<(K, V)>) -> BTreeMap<Name, V> {
    entries
        .into_iter()
        .map(|(name, value)| (name.into(), value))
        .collect()
}

#[allow(dead_code)]
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
            self.check_adapter_stage_arena(arena, source, first, input_ty)
        } else {
            match stream_type_from_input(input_ty) {
                Some(ty) => self.check_stream_stage_arena(arena, source, first, ty),
                None => {
                    self.error(
                        arena.arena.expr(input).span,
                        "structured pipelines require Stream or List input",
                        "check.stream-input",
                    );
                    Type::Stream(Box::new(Type::Unknown))
                }
            }
        };

        for stage in rest {
            current = self.check_stream_stage_arena(arena, source, stage, current);
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
                "check.stream-terminal-stage",
            );
            return Type::Unknown;
        };
        let item_ty = *item_ty;
        match stage.kind {
            StreamStageKind::Where => {
                self.check_stage_no_args_arena(arena, stage);
                self.check_stage_options_arena(arena, source, stage, &[]);
                let actual = self.check_required_stream_block_arena(arena, source, stage, &item_ty);
                let predicate_ty = result_ok_or_self(&actual);
                self.expect_type(&Type::Bool, &predicate_ty, stage_span);
                Type::Stream(Box::new(item_ty))
            }
            StreamStageKind::Map => {
                self.check_stage_no_args_arena(arena, stage);
                self.check_stage_options_arena(arena, source, stage, &[]);
                let actual = self.check_required_stream_block_arena(arena, source, stage, &item_ty);
                let output_ty = result_ok_or_self(&actual);
                if output_ty == Type::Unit {
                    self.error(stage_span, "map requires a tail value", "check.map-tail");
                }
                Type::Stream(Box::new(output_ty))
            }
            StreamStageKind::ParMap => {
                self.check_stage_no_args_arena(arena, stage);
                self.check_stage_options_arena(arena, source, stage, &["jobs"]);
                let actual = self.check_required_stream_block_arena(arena, source, stage, &item_ty);
                let output_ty = result_ok_or_self(&actual);
                if output_ty == Type::Unit {
                    self.error(
                        stage_span,
                        "par-map requires a tail value",
                        "check.map-tail",
                    );
                }
                Type::Stream(Box::new(output_ty))
            }
            StreamStageKind::Each => {
                self.check_stage_no_args_arena(arena, stage);
                self.check_stage_options_arena(arena, source, stage, &["jobs"]);
                let actual = self.check_required_stream_block_arena(arena, source, stage, &item_ty);
                match actual {
                    Type::Unit => {}
                    Type::Result(ok, _) if *ok == Type::Unit => {}
                    Type::Unknown => {}
                    _ => self.error(
                        stage_span,
                        "each blocks must produce Unit or Result[Unit]",
                        "check.each-tail",
                    ),
                }
                Type::Unit
            }
            StreamStageKind::Batch => {
                self.check_batch_stage_arena(arena, source, stage, &item_ty);
                Type::Stream(Box::new(Type::List(Box::new(item_ty))))
            }
            StreamStageKind::Sort => {
                self.check_stage_no_options_arena(arena, source, stage);
                if !stage.args.is_empty() || stage.block.is_some() {
                    self.error(stage_span, "sort accepts no arguments", "check.arity");
                }
                if matches!(
                    item_ty,
                    Type::Int | Type::Str | Type::Bool | Type::Path | Type::Unknown
                ) || is_sortable_record_key_type(&item_ty) {
                    Type::Stream(Box::new(item_ty))
                } else {
                    self.error(
                        stage_span,
                        "sort items must be Int, Str, Bool, Path, or a record of supported items",
                        "check.stream-sort",
                    );
                    Type::Stream(Box::new(item_ty))
                }
            }
            StreamStageKind::SortBy => {
                self.check_stage_no_args_arena(arena, stage);
                for option in arena.arena.stream_options(stage.options) {
                    let option_span = arena.arena.span(option.span);
                    match option.name.as_str().as_str() {
                        "desc" => {
                            if let Some(value) = option.value {
                                let actual =
                                    self.check_expr_arena(arena, source, value, Some(&Type::Bool));
                                let value_span = arena.arena.expr(value).span;
                                self.expect_type(&Type::Bool, &actual, value_span);
                            }
                        }
                        _ => self.error(
                            option_span,
                            "unsupported stream stage option",
                            "check.stream-stage-option",
                        ),
                    }
                }
                let key_ty = self.check_required_stream_block_arena(arena, source, stage, &item_ty);
                let key_ty = result_ok_or_self(&key_ty);
                if !is_sortable_key_type(&key_ty) {
                    self.error(
                        stage_span,
                        "sort-by keys must be Int, Str, Bool, Path, or a record of supported keys",
                        "check.stream-sort",
                    );
                }
                Type::Stream(Box::new(item_ty))
            }
            StreamStageKind::Take | StreamStageKind::Drop => {
                self.check_stage_no_options_arena(arena, source, stage);
                let args = arena.arena.call_args(stage.args);
                if args.len() != 1 {
                    self.error(stage_span, "stage expects a count", "check.arity");
                }
                if let Some(arg) = args.first() {
                    let actual =
                        self.check_call_arg_arena(arena, source, &arg.kind, Some(&Type::Int));
                    self.expect_type(&Type::Int, &actual, call_arg_span_arena(arena, &arg.kind));
                }
                if stage.block.is_some() {
                    self.error(
                        stage_span,
                        "stage does not accept a block",
                        "check.stream-stage-block",
                    );
                }
                Type::Stream(Box::new(item_ty))
            }
            StreamStageKind::First | StreamStageKind::Last => {
                self.check_stage_no_options_arena(arena, source, stage);
                if !stage.args.is_empty() || stage.block.is_some() {
                    self.error(stage_span, "stage accepts no arguments", "check.arity");
                }
                Type::Result(Box::new(item_ty), Box::new(Type::Error))
            }
            StreamStageKind::UniqueBy => {
                self.check_stage_no_args_arena(arena, stage);
                self.check_stage_options_arena(arena, source, stage, &[]);
                let _ = self.check_required_stream_block_arena(arena, source, stage, &item_ty);
                Type::Stream(Box::new(item_ty))
            }
            StreamStageKind::Enumerate => {
                self.check_stage_no_options_arena(arena, source, stage);
                if !stage.args.is_empty() || stage.block.is_some() {
                    self.error(
                        stage_span,
                        "enumerate() accepts no arguments",
                        "check.arity",
                    );
                }
                Type::Stream(Box::new(Type::Record(btree_map(vec![
                    ("index".to_string(), Type::Int),
                    ("value".to_string(), item_ty),
                ]))))
            }
            StreamStageKind::Zip => {
                self.check_stage_no_options_arena(arena, source, stage);
                let args = arena.arena.call_args(stage.args);
                if args.len() != 1 {
                    self.error(stage_span, "zip expects one stream or list", "check.arity");
                }
                let other_ty = args
                    .first()
                    .map(|arg| self.check_call_arg_arena(arena, source, &arg.kind, None))
                    .unwrap_or(Type::Unknown);
                let other_item = match stream_type_from_input(other_ty) {
                    Some(Type::Stream(item)) => *item,
                    _ => Type::Unknown,
                };
                Type::Stream(Box::new(Type::Record(btree_map(vec![
                    ("left".to_string(), item_ty),
                    ("right".to_string(), other_item),
                ]))))
            }
            StreamStageKind::Range => {
                self.check_stage_no_options_arena(arena, source, stage);
                let args = arena.arena.call_args(stage.args);
                if args.len() != 2 || stage.block.is_some() {
                    self.error(stage_span, "range expects start and end", "check.arity");
                }
                for arg in args {
                    let actual =
                        self.check_call_arg_arena(arena, source, &arg.kind, Some(&Type::Int));
                    self.expect_type(&Type::Int, &actual, call_arg_span_arena(arena, &arg.kind));
                }
                Type::Stream(Box::new(Type::Int))
            }
            StreamStageKind::Repeat => {
                self.check_stage_no_options_arena(arena, source, stage);
                let args = arena.arena.call_args(stage.args);
                if args.len() != 1 || stage.block.is_some() {
                    self.error(stage_span, "repeat expects a count", "check.arity");
                }
                if let Some(arg) = args.first() {
                    let actual =
                        self.check_call_arg_arena(arena, source, &arg.kind, Some(&Type::Int));
                    self.expect_type(&Type::Int, &actual, call_arg_span_arena(arena, &arg.kind));
                }
                Type::Stream(Box::new(item_ty))
            }
            StreamStageKind::Tee => {
                self.check_stage_no_args_arena(arena, stage);
                self.check_stage_options_arena(arena, source, stage, &[]);
                let actual = self.check_required_stream_block_arena(arena, source, stage, &item_ty);
                match actual {
                    Type::Unit | Type::Unknown => {}
                    Type::Result(ok, _) if *ok == Type::Unit => {}
                    _ => self.error(
                        stage_span,
                        "tee blocks must produce Unit",
                        "check.each-tail",
                    ),
                }
                Type::Stream(Box::new(item_ty))
            }
            StreamStageKind::Sum => {
                self.check_stage_no_options_arena(arena, source, stage);
                if !stage.args.is_empty() || stage.block.is_some() {
                    self.error(stage_span, "sum() accepts no arguments", "check.arity");
                }
                self.expect_type(&Type::Int, &item_ty, stage_span);
                Type::Int
            }
            StreamStageKind::Min | StreamStageKind::Max => {
                self.check_stage_no_options_arena(arena, source, stage);
                if !stage.args.is_empty() || stage.block.is_some() {
                    self.error(stage_span, "min/max accept no arguments", "check.arity");
                }
                Type::Result(Box::new(item_ty), Box::new(Type::Error))
            }
            StreamStageKind::GroupBy => {
                self.check_stage_no_args_arena(arena, stage);
                self.check_stage_options_arena(arena, source, stage, &["jobs"]);
                let key_ty = result_ok_or_self(
                    &self.check_required_stream_block_arena(arena, source, stage, &item_ty),
                );
                Type::Stream(Box::new(Type::Record(btree_map(vec![
                    ("key".to_string(), key_ty),
                    ("items".to_string(), Type::List(Box::new(item_ty))),
                ]))))
            }
            StreamStageKind::Fold | StreamStageKind::Reduce => {
                self.check_stage_no_options_arena(arena, source, stage);
                let args = arena.arena.call_args(stage.args);
                if args.len() != 1 {
                    self.error(
                        stage_span,
                        "fold/reduce expects an initial value",
                        "check.arity",
                    );
                }
                let acc_ty = args
                    .first()
                    .map(|arg| self.check_call_arg_arena(arena, source, &arg.kind, None))
                    .unwrap_or(Type::Unknown);
                // A `fold`/`reduce` block binds the accumulator (typed by the
                // initial value) before the stream item, so it accepts up to
                // two parameters: `|acc, item| ...`. The tail must produce the
                // accumulator type.
                let actual =
                    self.check_fold_stream_block_arena(arena, source, stage, &acc_ty, &item_ty);
                let actual = result_ok_or_self(&actual);
                self.expect_type(&acc_ty, &actual, stage_span);
                acc_ty
            }
            StreamStageKind::FlatMap => {
                self.check_stage_no_args_arena(arena, stage);
                self.check_stage_options_arena(arena, source, stage, &[]);
                let actual = result_ok_or_self(
                    &self.check_required_stream_block_arena(arena, source, stage, &item_ty),
                );
                match actual {
                    Type::List(item) | Type::Stream(item) => Type::Stream(item),
                    Type::Unknown => Type::Stream(Box::new(Type::Unknown)),
                    _ => {
                        self.error(
                            stage_span,
                            "flat-map blocks must produce List or Stream",
                            "check.flat-map",
                        );
                        Type::Stream(Box::new(Type::Unknown))
                    }
                }
            }
            StreamStageKind::Any | StreamStageKind::All => {
                self.check_stage_no_args_arena(arena, stage);
                self.check_stage_options_arena(arena, source, stage, &[]);
                let actual = result_ok_or_self(
                    &self.check_required_stream_block_arena(arena, source, stage, &item_ty),
                );
                self.expect_type(&Type::Bool, &actual, stage_span);
                Type::Bool
            }
            StreamStageKind::Shuffle => {
                self.check_stage_no_options_arena(arena, source, stage);
                let args = arena.arena.call_args(stage.args);
                if args.len() > 1 || stage.block.is_some() {
                    self.error(
                        stage_span,
                        "shuffle accepts an optional seed",
                        "check.arity",
                    );
                }
                if let Some(arg) = args.first() {
                    let actual =
                        self.check_call_arg_arena(arena, source, &arg.kind, Some(&Type::Int));
                    self.expect_type(&Type::Int, &actual, call_arg_span_arena(arena, &arg.kind));
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
                    "check.stream-adapter",
                );
                Type::Unknown
            }
            StreamStageKind::Count => {
                self.check_stage_options_arena(arena, source, stage, &["jobs"]);
                if !stage.args.is_empty() {
                    self.error(stage_span, "count does not accept arguments", "check.arity");
                }
                if stage.block.is_some() {
                    let _key_ty = result_ok_or_self(
                        &self.check_required_stream_block_arena(arena, source, stage, &item_ty),
                    );
                    Type::Map(Box::new(Type::Int))
                } else {
                    Type::Int
                }
            }
            StreamStageKind::Collect => {
                self.check_stage_no_options_arena(arena, source, stage);
                if !stage.args.is_empty() || stage.block.is_some() {
                    self.error(
                        stage_span,
                        "collect() accepts no arguments or block",
                        "check.arity",
                    );
                }
                Type::List(Box::new(item_ty))
            }
            StreamStageKind::ReduceBy => {
                self.check_stage_no_args_arena(arena, stage);
                for option in arena.arena.stream_options(stage.options) {
                    let option_span = arena.arena.span(option.span);
                    match option.name.as_str().as_str() {
                        "sum" | "min" | "max" => {}
                        "jobs" => {
                            if let Some(value) = option.value {
                                let actual =
                                    self.check_expr_arena(arena, source, value, Some(&Type::Int));
                                let value_span = arena.arena.expr(value).span;
                                self.expect_type(&Type::Int, &actual, value_span);
                            }
                        }
                        _ => self.error(
                            option_span,
                            "reduce-by options are --sum, --min, --max, --jobs",
                            "check.stream-stage-option",
                        ),
                    }
                }
                let block_ty = result_ok_or_self(
                    &self.check_required_stream_block_arena(arena, source, stage, &item_ty),
                );
                let value_ty = match &block_ty {
                    Type::Record(fields) => fields
                        .get(&Name::intern("value"))
                        .cloned()
                        .unwrap_or(Type::Unknown),
                    _ => Type::Unknown,
                };
                Type::Map(Box::new(value_ty))
            }
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
                "check.stream-stage-block",
            );
            return Type::Unknown;
        };
        self.check_stream_block_params_arena(
            arena,
            source,
            block,
            &[item_ty.clone()],
            1,
            item_ty,
        )
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
                "check.stream-stage-block",
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
                "check.stream-block-params",
            );
        }
        self.push_scope();
        for (index, param) in params.iter().take(max_params).enumerate() {
            let ty = param_tys.get(index).cloned().unwrap_or(Type::Unknown);
            self.define(
                param.name,
                Binding::new(ty, false),
                arena.arena.span(param.span),
            );
        }
        self.stream_item_types.push(item_ty.clone());
        let mut tail_ty = Type::Unit;
        let stmt_ids: Vec<_> = arena.arena.stmt_ids(block.statements).collect();
        for (index, stmt_id) in stmt_ids.iter().enumerate() {
            if index + 1 == stmt_ids.len() {
                tail_ty = self.check_stream_tail_stmt_arena(arena, source, *stmt_id);
            } else {
                self.check_stmt_arena(arena, source, *stmt_id);
            }
        }
        self.stream_item_types.pop();
        self.pop_scope();
        tail_ty
    }

    fn check_stream_tail_stmt_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        stmt_id: crate::syntax::arena::StmtId,
    ) -> Type {
        let stmt = arena.arena.stmt(stmt_id);
        match stmt.kind {
            crate::syntax::arena::ArenaStmtKind::Expr(expr_id) => {
                self.check_expr_arena(arena, source, expr_id, None)
            }
            crate::syntax::arena::ArenaStmtKind::TailBareIdent(name) => {
                self.check_tail_bare_ident_arena(arena, source, name, stmt.span)
            }
            crate::syntax::arena::ArenaStmtKind::Command(command_id) => {
                if self.in_pure {
                    self.error(
                        stmt.span,
                        "commands are not allowed in pure functions",
                        "check.pure-command",
                    );
                }
                let command = arena.arena.command_stmt(command_id);
                let ty = self.check_command_arena(arena, source, &command.command, stmt.span);
                if command_stmt_asserts_success_arena(arena, &command.command) {
                    return Type::Unit;
                }
                if command.propagate || command_ty_auto_propagates(&ty) {
                    self.check_propagation(&ty, stmt.span)
                } else {
                    ty
                }
            }
            _ => {
                self.check_stmt_arena(arena, source, stmt_id);
                Type::Unit
            }
        }
    }

    fn check_stage_no_args_arena(&mut self, arena: &ArenaProgram, stage: &ArenaStreamStage) {
        if !stage.args.is_empty() {
            self.error(
                arena.arena.span(stage.span),
                "stream stage does not accept call arguments",
                "check.arity",
            );
        }
    }

    fn check_stage_no_options_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        stage: &ArenaStreamStage,
    ) {
        self.check_stage_options_arena(arena, source, stage, &[]);
    }

    fn check_stage_options_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        stage: &ArenaStreamStage,
        allowed: &[&str],
    ) {
        for option in arena.arena.stream_options(stage.options) {
            if !allowed.contains(&option.name.as_str().as_str()) {
                self.error(
                    arena.arena.span(option.span),
                    "unsupported stream stage option",
                    "check.stream-stage-option",
                );
            }
            let Some(value) = option.value else {
                self.error(
                    arena.arena.span(option.span),
                    "stream stage option requires a value",
                    "check.stream-stage-option",
                );
                continue;
            };
            let actual = self.check_expr_arena(arena, source, value, Some(&Type::Int));
            let value_span = arena.arena.expr(value).span;
            self.expect_type(&Type::Int, &actual, value_span);
            if option.name == "jobs" {
                self.check_static_positive_value_arena(arena, value, "check.stream-jobs");
            }
        }
    }

    fn check_batch_stage_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        stage: &ArenaStreamStage,
        item_ty: &Type,
    ) {
        self.check_stage_no_args_arena(arena, stage);
        if stage.block.is_some() {
            self.error(
                arena.arena.span(stage.span),
                "batch does not accept a block",
                "check.stream-stage-block",
            );
        }
        let mut has_limit = false;
        for option in arena.arena.stream_options(stage.options) {
            match option.name.as_str().as_str() {
                "count" | "max-bytes" => {
                    has_limit = true;
                    let Some(value) = self.check_required_option_value_arena(arena, option) else {
                        continue;
                    };
                    let actual = self.check_expr_arena(arena, source, value, Some(&Type::Int));
                    let value_span = arena.arena.expr(value).span;
                    self.expect_type(&Type::Int, &actual, value_span);
                    self.check_static_positive_value_arena(arena, value, "check.stream-batch");
                    if option.name == "max-bytes"
                        && !item_ty.can_be_argv_item()
                        && !matches!(item_ty, Type::Unknown)
                    {
                        self.error(
                            arena.arena.span(option.span),
                            "batch --max-bytes requires argv-compatible stream items",
                            "check.stream-batch",
                        );
                    }
                }
                "max-argv" => {
                    has_limit = true;
                    if let Some(value) = option.value {
                        let actual = self.check_expr_arena(arena, source, value, Some(&Type::Bool));
                        let value_span = arena.arena.expr(value).span;
                        self.expect_type(&Type::Bool, &actual, value_span);
                    }
                    if !item_ty.can_be_argv_item() && !matches!(item_ty, Type::Unknown) {
                        self.error(
                            arena.arena.span(option.span),
                            "batch --max-argv requires argv-compatible stream items",
                            "check.stream-batch",
                        );
                    }
                }
                _ => self.error(
                    arena.arena.span(option.span),
                    "unsupported stream stage option",
                    "check.stream-stage-option",
                ),
            }
        }
        if !has_limit {
            self.error(
                arena.arena.span(stage.span),
                "batch requires --count=N, --max-bytes=N, or --max-argv",
                "check.stream-batch",
            );
        }
    }

    fn check_table_print_stage_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        stage: &ArenaStreamStage,
        item_ty: &Type,
    ) {
        self.check_stage_no_options_arena(arena, source, stage);
        let stage_span = arena.arena.span(stage.span);
        if stage.block.is_some() {
            self.error(
                stage_span,
                "table.print does not accept a block",
                "check.stream-stage-block",
            );
        }
        if !matches!(item_ty, Type::Record(_) | Type::Unknown) {
            self.error(
                stage_span,
                "table.print requires record stream items",
                "check.table-print",
            );
        }
        let args = arena.arena.call_args(stage.args);
        if args.len() > 1 {
            self.error(stage_span, "incorrect function arity", "check.arity");
        }
        if let Some(arg) = args.first() {
            if let ArenaCallArgKind::Named { name, .. } = &arg.kind
                && name != "columns"
            {
                self.error(
                    call_arg_span_arena(arena, &arg.kind),
                    "unexpected named parameter",
                    "check.named-arg",
                );
            }
            let actual = self.check_call_arg_arena(
                arena,
                source,
                &arg.kind,
                Some(&Type::List(Box::new(Type::Str))),
            );
            self.expect_type(
                &Type::List(Box::new(Type::Str)),
                &actual,
                call_arg_span_arena(arena, &arg.kind),
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
        self.check_stage_no_options_arena(arena, source, stage);
        let stage_span = arena.arena.span(stage.span);
        if stage.block.is_some() {
            self.error(
                stage_span,
                "adapter stages do not accept blocks",
                "check.stream-stage-block",
            );
        }
        match stage.kind {
            StreamStageKind::TextStreamLines => {
                self.check_stage_no_args_arena(arena, stage);
                self.expect_type(&Type::Str, &input_ty, stage_span);
                Type::Stream(Box::new(Type::Str))
            }
            StreamStageKind::BytesChunks => {
                let args = arena.arena.call_args(stage.args);
                if args.len() != 1 {
                    self.error(stage_span, "bytes.chunks expects a size", "check.arity");
                }
                if let Some(arg) = args.first() {
                    let actual =
                        self.check_call_arg_arena(arena, source, &arg.kind, Some(&Type::Int));
                    self.expect_type(&Type::Int, &actual, call_arg_span_arena(arena, &arg.kind));
                    self.check_static_positive_value_arena(
                        arena,
                        call_arg_expr_id_arena(&arg.kind),
                        "check.bytes-chunks",
                    );
                }
                self.expect_type(&Type::Bytes, &input_ty, stage_span);
                Type::Stream(Box::new(Type::Bytes))
            }
            StreamStageKind::JsonLines | StreamStageKind::JsonStream => {
                self.check_stage_no_args_arena(arena, stage);
                self.expect_type(&Type::Str, &input_ty, stage_span);
                Type::Stream(Box::new(Type::Unknown))
            }
            _ => unreachable!("adapter stage"),
        }
    }

    fn check_required_option_value_arena(
        &mut self,
        arena: &ArenaProgram,
        option: &ArenaStreamStageOption,
    ) -> Option<ExprId> {
        let Some(value) = option.value else {
            self.error(
                arena.arena.span(option.span),
                "batch option requires a value",
                "check.stream-stage-option",
            );
            return None;
        };
        Some(value)
    }

    fn check_static_positive_value_arena(
        &mut self,
        arena: &ArenaProgram,
        expr_id: ExprId,
        code: &str,
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

fn stream_type_from_input(ty: Type) -> Option<Type> {
    match ty {
        Type::Stream(_) => Some(ty),
        Type::List(item) => Some(Type::Stream(item)),
        Type::Result(ok, _) => stream_type_from_input(*ok),
        Type::Any => Some(Type::Stream(Box::new(Type::Any))),
        Type::Unknown => Some(Type::Stream(Box::new(Type::Unknown))),
        _ => None,
    }
}

fn result_ok_or_self(ty: &Type) -> Type {
    match ty {
        Type::Result(ok, _) => (**ok).clone(),
        _ => ty.clone(),
    }
}

/// Whether a projected `sort-by` key or `sort` item type has a defined
/// ordering. Records are orderable when every field is itself orderable; the
/// runtime comparator in `lowered_ops.rs` implements the same surface so a
/// checked program and an unchecked `xsh` run agree on what can sort.
///
/// `Unknown` and `Any` are accepted to match the runtime: an `Any`-typed key
/// (for example a record field produced by `Map.get(key, fallback)`) is the
/// static view of a value that is a supported scalar (Int, Str, Bool, Path) at
/// runtime. The runtime `lowered_sort_key_orderable` still fails loudly when the
/// actual value is not orderable, so the checker and the runtime agree on every
/// program that runs correctly.
fn is_sortable_key_type(ty: &Type) -> bool {
    match ty {
        Type::Int | Type::Str | Type::Bool | Type::Path | Type::Unknown | Type::Any => true,
        Type::Record(fields) => fields.values().all(is_sortable_key_type),
        _ => false,
    }
}

fn is_sortable_record_key_type(ty: &Type) -> bool {
    matches!(ty, Type::Record(fields) if fields.values().all(is_sortable_key_type))
}
