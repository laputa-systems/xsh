use super::{
    Binding, Checker, Name,

};
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
        for argument in arena.arena.call_args(stage.args) {
            if let ArenaCallArgKind::NamedSpread { value, .. } = argument.kind {
                self.check_expr_arena(arena, source, value, None);
            }
        }
        match crate::sema::stage_arguments::stage_callable_argument(arena, stage, |expr| self.expr_types.get(&arena.arena.expr(expr).span).cloned()) {
            Ok(Some((callee, arguments))) => {
                if !self.stage_callable_is_static(arena, callee) {
                    self.error(arena.arena.expr(callee).span, "stage callable must be a statically resolved named function or proc", "check.stream-callable");
                    return Type::Unknown;
                }
                if let ArenaExprKind::Field { base, name } = arena.arena.expr(callee).kind
                    && let ArenaExprKind::Ident(namespace) = arena.arena.expr(base).kind
                    && let Some(overloads) = super::api_spec().module_overloads(&namespace.as_str(), &name.as_str())
                {
                    let matches = overloads.iter().filter(|signature| super::args::module_sig_accepts_arity(1, signature)
                        && signature.params.first().is_some_and(|parameter| item_ty.matches_expected(&parameter.ty))).count();
                    if matches != 1 {
                        self.error(arena.arena.expr(callee).span, "stage callable requires a unique checked one-item signature", "check.stream-callable-signature");
                        return Type::Unknown;
                    }
                }
                let mut temporary = arena.clone();
                let mut normalized = stage.clone();
                normalized.args = temporary.arena.append_call_arguments(&arguments);
                normalized.block = Some(temporary.arena.append_stage_callable_block(callee, arena.arena.expr(callee).span).0);
                return self.check_stream_stage_arena(&temporary, source, &normalized, Type::Stream(Box::new(item_ty)));
            }
            Err((span, message)) => { self.error(span, &message, "check.stream-callable"); return Type::Unknown; }
            Ok(None) => {}
        }
        let arguments = self.check_stage_arguments_arena(arena, source, stage);
        match stage.kind {
            StreamStageKind::Where => {
                self.check_stage_no_args_arena(arena, stage);
                let actual = self.check_required_stream_block_arena(arena, source, stage, &item_ty);
                let predicate_ty = result_ok_or_self(&actual);
                self.expect_type(&Type::Bool, &predicate_ty, stage_span);
                Type::Stream(Box::new(item_ty))
            }
            StreamStageKind::Map => {
                self.check_stage_no_args_arena(arena, stage);
                let actual = self.check_required_stream_block_arena(arena, source, stage, &item_ty);
                let output_ty = actual;
                if output_ty == Type::Unit {
                    self.error(stage_span, "map requires a tail value", "check.map-tail");
                }
                Type::Stream(Box::new(output_ty))
            }
            StreamStageKind::ParMap => {
                self.check_stage_no_args_arena(arena, stage);
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
                self.check_batch_stage_arena(arena, source, stage, &item_ty, &arguments);
                Type::Stream(Box::new(Type::List(Box::new(item_ty))))
            }
            StreamStageKind::Sort => {
                if stage.block.is_some() {
                    self.error(stage_span, "sort does not accept a block", "check.stream-stage-block");
                }
                if matches!(
                    item_ty,
                    Type::Int | Type::Str | Type::Bool | Type::Path | Type::Unknown
                ) || is_sortable_record_key_type(&item_ty)
                {
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
                if !stage.args.is_empty() || stage.block.is_some() {
                    self.error(stage_span, "stage accepts no arguments", "check.arity");
                }
                Type::Result(Box::new(item_ty), Box::new(Type::Error))
            }
            StreamStageKind::UniqueBy => {
                self.check_stage_no_args_arena(arena, stage);
                let _ = self.check_required_stream_block_arena(arena, source, stage, &item_ty);
                Type::Stream(Box::new(item_ty))
            }
            StreamStageKind::Enumerate => {
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
                let other_ty = arguments.first().and_then(Option::as_ref).map(|argument| argument.ty.clone()).unwrap_or(Type::Unknown);
                let other_item = match stream_type_from_input(other_ty) {
                    Some(Type::Stream(item)) => *item,
                    _ => {
                        let span = arguments.first().and_then(Option::as_ref).map(|argument| argument.span).unwrap_or(stage_span);
                        self.error(span, "zip other must be a Stream or List", "check.type-mismatch");
                        Type::Unknown
                    }
                };
                Type::Stream(Box::new(Type::Record(btree_map(vec![
                    ("left".to_string(), item_ty),
                    ("right".to_string(), other_item),
                ]))))
            }
            StreamStageKind::Range => {
                if stage.block.is_some() { self.error(stage_span, "range does not accept a block", "check.stream-stage-block"); }
                Type::Stream(Box::new(Type::Int))
            }
            StreamStageKind::Repeat => {
                if stage.block.is_some() { self.error(stage_span, "repeat does not accept a block", "check.stream-stage-block"); }
                Type::Stream(Box::new(item_ty))
            }
            StreamStageKind::Tee => {
                self.check_stage_no_args_arena(arena, stage);
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
                if !stage.args.is_empty() || stage.block.is_some() {
                    self.error(stage_span, "sum() accepts no arguments", "check.arity");
                }
                self.expect_type(&Type::Int, &item_ty, stage_span);
                Type::Int
            }
            StreamStageKind::Min | StreamStageKind::Max => {
                if !stage.args.is_empty() || stage.block.is_some() {
                    self.error(stage_span, "min/max accept no arguments", "check.arity");
                }
                Type::Result(Box::new(item_ty), Box::new(Type::Error))
            }
            StreamStageKind::GroupBy => {
                self.check_stage_no_args_arena(arena, stage);
                let key_ty = result_ok_or_self(
                    &self.check_required_stream_block_arena(arena, source, stage, &item_ty),
                );
                Type::Stream(Box::new(Type::Record(btree_map(vec![
                    ("key".to_string(), key_ty),
                    ("items".to_string(), Type::List(Box::new(item_ty))),
                ]))))
            }
            StreamStageKind::Fold | StreamStageKind::Reduce => {
                let acc_ty = arguments.first().and_then(Option::as_ref).map(|argument| argument.ty.clone()).unwrap_or(Type::Unknown);
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
                let actual = result_ok_or_self(
                    &self.check_required_stream_block_arena(arena, source, stage, &item_ty),
                );
                self.expect_type(&Type::Bool, &actual, stage_span);
                Type::Bool
            }
            StreamStageKind::Shuffle => {
                if stage.block.is_some() { self.error(stage_span, "shuffle does not accept a block", "check.stream-stage-block"); }
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
                if !stage.args.is_empty() {
                    self.error(stage_span, "count does not accept arguments", "check.arity");
                }
                if stage.block.is_some() {
                    let _key_ty = result_ok_or_self(
                        &self.check_required_stream_block_arena(arena, source, stage, &item_ty),
                    );
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
                        "check.arity",
                    );
                }
                Type::List(Box::new(item_ty))
            }
            StreamStageKind::ReduceBy => {
                self.check_stage_no_args_arena(arena, stage);
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
                Type::Map(Box::new(Type::Str), Box::new(value_ty))
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
            std::slice::from_ref(item_ty),
            1,
            item_ty,
            matches!(stage.kind, StreamStageKind::Each | StreamStageKind::Tee).then_some(&Type::Unit),
        )
    }

    /// A descriptor retains a checked declaration or immutable alias signature.
    /// Lexical values with erased callable types cannot supply that contract.
    pub(super) fn stage_callable_is_static(&self, arena: &ArenaProgram, callee: ExprId) -> bool {
        if self.resolve_callable_alias_call(arena, callee).is_some() { return true; }
        match arena.arena.expr(callee).kind {
            ArenaExprKind::Ident(name) => self.lookup(name).is_none()
                && (self.pures.contains_key(&name) || self.procs.contains_key(&name)),
            ArenaExprKind::Field { base, name } => {
                let ArenaExprKind::Ident(namespace) = arena.arena.expr(base).kind else { return false; };
                if self.scopes.iter().skip(1).any(|scope| scope.contains_key(&namespace)) { return false; }
                let qualified = crate::symbol::QualifiedName::new(namespace, name);
                self.qualified_pures.contains_key(&qualified) || self.qualified_procs.contains_key(&qualified)
                    || crate::sema::stage_arguments::stage_namespace_is_imported(arena, namespace, self.current_namespace)
                    && matches!(self.lookup(namespace).map(|binding| &binding.ty), Some(Type::Module(exports))
                        if exports.get(&name).is_some_and(|export| matches!(export, crate::sema::types::ModuleExportType::Pure { .. } | crate::sema::types::ModuleExportType::Proc { .. })))
                    || self.lookup(namespace).is_none() && super::api_spec().module(&namespace.as_str())
                        .is_some_and(|module| module.functions.iter().any(|function| function.name == name.as_str().as_str()))
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
                "check.stream-block-params",
            );
        }
        self.push_deferred_capture_scope();
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
                tail_ty = self.check_tail_stmt_arena(arena, source, *stmt_id, expected);
            } else {
                self.check_stmt_arena(arena, source, *stmt_id);
            }
        }
        self.stream_item_types.pop();
        self.pop_scope();
        tail_ty
    }

    fn check_stage_arguments_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        stage: &ArenaStreamStage,
    ) -> Vec<Option<crate::sema::arguments::ExpandedArgument>> {
        use crate::sema::arguments::{ArgumentValueSource, bind_static_arguments, expand_named_arguments};
        use xsh_registry::stream_parameters::{StageParameterValidation, stage_parameters};
        let contract = stage_parameters(stage.kind.as_str());
        let params = crate::sema::stage_arguments::stage_argument_params(stage.kind.as_str());
        if params.is_empty() { return Vec::new(); }
        let args = arena.arena.call_args(stage.args);
        let mut positional = 0;
        for arg in args {
            let expected = match arg.kind {
                ArenaCallArgKind::Named { name, .. } => params.iter().find(|param| param.name == name).map(|param| &param.ty),
                ArenaCallArgKind::Positional(_) => { let expected = params.get(positional).map(|param| &param.ty); positional += 1; expected }
                _ => None,
            };
            if let ArenaCallArgKind::NamedSpread { value, .. } = arg.kind {
                self.check_expr_arena(arena, source, value, None);
            } else {
                self.check_call_arg_arena(arena, source, &arg.kind, expected);
            }
        }
        let expanded = match expand_named_arguments(arena, args, |expr| self.expr_types.get(&arena.arena.expr(expr).span).cloned()) {
            Ok(expanded) => expanded,
            Err(error) => { self.error(error.span, &error.message, "check.named-spread"); return vec![None; params.len()]; }
        };
        let binding = match bind_static_arguments(&params, &expanded) {
            Ok(binding) => binding,
            Err(error) => {
                let span = if args.is_empty() { arena.arena.span(stage.span) } else { error.span };
                self.error(span, &error.message, "check.named-arg");
                return vec![None; params.len()];
            }
        };
        let mut values = vec![None; params.len()];
        for (argument, slot) in expanded.into_iter().zip(binding.argument_slots) {
            if argument.name.is_none() && !contract[slot].positional {
                self.error(argument.span, "stage configuration parameters must be named", "check.named-arg");
            }
            self.expect_type(&params[slot].ty, &argument.ty, argument.span);
            if contract[slot].validation == StageParameterValidation::Positive
                && let ArgumentValueSource::Expression(value) = argument.value
            {
                let code = match contract[slot].name { "jobs" => "check.stream-jobs", "size" => "check.bytes-chunks", _ => "check.stream-batch" };
                self.check_static_positive_value_arena(arena, value, code);
            }
            values[slot] = Some(argument);
        }
        let literal_bool = |argument: &crate::sema::arguments::ExpandedArgument| match argument.value {
            ArgumentValueSource::Expression(expr) => match arena.arena.expr(expr).kind { ArenaExprKind::Bool(value) => Some(value), _ => None },
            _ => None,
        };
        if stage.kind == StreamStageKind::ReduceBy {
            let modes = &values[..3];
            let enabled = modes.iter().flatten().filter(|argument| literal_bool(argument) == Some(true)).count();
            let dynamic = modes.iter().flatten().any(|argument| literal_bool(argument).is_none());
            if enabled > 1 || !dynamic && enabled != 1 {
                self.error(arena.arena.span(stage.span), "reduce-by requires exactly one enabled reduction mode", "check.stream-reduce-mode");
            }
        }
        if stage.kind == StreamStageKind::Batch
            && values[0].is_none() && values[1].is_none()
            && values[2].as_ref().is_none_or(|argument| literal_bool(argument) == Some(false))
        {
            self.error(arena.arena.span(stage.span), "batch requires an enabled count or byte limit", "check.stream-batch");
        }
        values
    }

    fn check_stage_no_args_arena(&mut self, arena: &ArenaProgram, stage: &ArenaStreamStage) {
        if !stage.args.is_empty() && xsh_registry::stream_parameters::stage_parameters(stage.kind.as_str()).is_empty() {
            self.error(
                arena.arena.span(stage.span),
                "stream stage does not accept call arguments",
                "check.arity",
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
                "check.stream-stage-block",
            );
        }
        for (slot, argument) in arguments.iter().enumerate().skip(1).filter_map(|(slot, argument)| argument.as_ref().map(|argument| (slot, argument))) {
            if slot == 2 && let crate::sema::arguments::ArgumentValueSource::Expression(expr) = argument.value
                && matches!(arena.arena.expr(expr).kind, ArenaExprKind::Bool(false)) { continue; }
            if !item_ty.can_be_argv_item() && !matches!(item_ty, Type::Unknown) {
                self.error(argument.span, "byte-bounded batches require argv-compatible items", "check.stream-batch");
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
