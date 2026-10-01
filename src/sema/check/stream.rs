use super::{Binding, Checker, Type};
use crate::syntax::arena::{ArenaCallArgKind, ArenaExprKind, ArenaProgram, ArenaStreamStage, ExprId};
use crate::syntax::node::{StreamStageKind, UnaryOp};

impl Checker {
    pub(super) fn check_structured_pipeline_arena(
        &mut self, arena: &ArenaProgram, source: &str, input: ExprId,
        stages: crate::syntax::arena::ArenaRange,
    ) -> Type {
        self.check_graph_pipeline_stages(arena, source, input, stages)
    }

    /// A descriptor retains a checked declaration or immutable alias signature.
    /// Lexical values with erased callable types cannot supply that contract.
    pub(super) fn stage_callable_is_static(&self, arena: &ArenaProgram, callee: ExprId) -> bool {
        if self.resolve_callable_alias_call(arena, callee).is_some() { return true; }
        if let Some(target) = self.graph_callable_target(arena, callee) {
            let state = self.generic.borrow();
            if matches!(state.facts.graph.resolved(target.signature).and_then(|ty| state.facts.graph.node(ty)),
                Ok(crate::sema::inference::TypeNode::NativeCallable(_))) { return true; }
        }
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

    pub(super) fn check_stream_block_params_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        block_id: crate::syntax::arena::BlockId,
        parameters: &[super::StreamItemFact],
        expected: Option<&Type>,
    ) -> Type {
        let block = arena.arena.block(block_id);
        let params = arena.arena.block_params(block.params);
        let max_params = parameters.len();
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
            let mut binding = Binding::new(parameters[index].ty.clone(), false);
            binding.producer_flow = parameters[index].producer_flow;
            self.define(param.name, binding, arena.arena.span(param.span));
        }
        self.stream_items.push(parameters.last().expect("stage callback requires its item parameter").clone());
        let mut tail_ty = Type::Unit;
        let stmt_ids: Vec<_> = arena.arena.stmt_ids(block.statements).collect();
        for (index, stmt_id) in stmt_ids.iter().enumerate() {
            if index + 1 == stmt_ids.len() {
                tail_ty = self.check_tail_stmt_arena(arena, source, *stmt_id, expected);
            } else {
                self.check_stmt_arena(arena, source, *stmt_id);
            }
        }
        self.stream_items.pop();
        self.pop_scope();
        tail_ty
    }

    pub(super) fn check_stage_arguments_arena(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        stage: &ArenaStreamStage,
        args: &[crate::syntax::arena::ArenaCallArg],
    ) -> Vec<Option<crate::sema::arguments::ExpandedArgument>> {
        use crate::sema::arguments::{ArgumentValueSource, bind_static_arguments, expand_named_arguments};
        use xsh_registry::stream_parameters::{StageParameterValidation, stage_parameters};
        let contract = stage_parameters(stage.kind.as_str());
        let params = crate::sema::stage_arguments::stage_argument_params(stage.kind.as_str());
        if params.is_empty() {
            if !args.is_empty() { self.error(arena.arena.span(stage.span), "stream stage does not accept call arguments", "check.arity"); }
            return Vec::new();
        }
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
                self.check_call_arg_arena(arena, source, &arg.kind, expected.filter(|ty| **ty != Type::Unknown));
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
            if params[slot].ty != Type::Unknown { self.expect_type(&params[slot].ty, &argument.ty, argument.span); }
            if contract[slot].validation == StageParameterValidation::Positive
                && let ArgumentValueSource::Expression(value) = argument.value
            {
                let code = match contract[slot].name { "jobs" => "check.stream-jobs", "size" => "check.bytes-chunks", _ => "check.stream-batch" };
                self.check_static_positive_value_arena(arena, value, code);
            }
            if contract[slot].validation == StageParameterValidation::Nonnegative
                && self.stage_argument_literal(arena, &argument).is_some_and(|value| matches!(value, crate::sema::constants::LiteralConstant::Int(value) if value < 0)) {
                self.error(argument.span, "stream count must be nonnegative", "check.stream-count");
            }
            values[slot] = Some(argument);
        }
        if stage.kind == StreamStageKind::ReduceBy {
            let modes = &values[..3];
            let enabled = modes.iter().flatten().filter(|argument| self.stage_argument_literal_bool(arena, argument) == Some(true)).count();
            let dynamic = modes.iter().flatten().any(|argument| self.stage_argument_literal_bool(arena, argument).is_none());
            if enabled > 1 || !dynamic && enabled != 1 {
                self.error(arena.arena.span(stage.span), "reduce-by requires exactly one enabled reduction mode", "check.stream-reduce-mode");
            }
        }
        if stage.kind == StreamStageKind::Batch
            && values[0].is_none() && values[1].is_none()
            && values[2].as_ref().is_none_or(|argument| self.stage_argument_literal_bool(arena, argument) == Some(false))
        {
            self.error(arena.arena.span(stage.span), "batch requires an enabled count or byte limit", "check.stream-batch");
        }
        values
    }

    pub(super) fn stage_argument_literal(&self, arena: &ArenaProgram, argument: &crate::sema::arguments::ExpandedArgument) -> Option<crate::sema::constants::LiteralConstant> {
        use crate::sema::arguments::ArgumentValueSource;
        match argument.value {
            ArgumentValueSource::Expression(expression) => self.prepared_constants.analyze_expression(&arena.arena, expression),
            ArgumentValueSource::RecordField { record, field } => {
                let crate::sema::constants::LiteralConstant::Record(fields) = self.prepared_constants.analyze_expression(&arena.arena, record)? else { return None; };
                fields.get(&field).cloned()
            }
            ArgumentValueSource::PositionalSplice(_) => None,
        }
    }

    pub(super) fn stage_argument_literal_bool(&self, arena: &ArenaProgram, argument: &crate::sema::arguments::ExpandedArgument) -> Option<bool> {
        if let crate::sema::constants::LiteralConstant::Bool(value) = self.stage_argument_literal(arena, argument)? { Some(value) } else { None }
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
