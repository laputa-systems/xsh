use super::serial_pipeline::{
    IndexedLiveSerialStage, IndexedSerialPipeline, IndexedSerialStep, IndexedStageEngine,
    IndexedStageStep,
    indexed_for_pipeline_input, indexed_value_pipeline_input,
};
use super::super::LoweredRetryDelays;
use super::{
    Arc, AssignOp, BTreeMap, BinaryOp, CommandPlan, ControlFlow,
    Evaluator,
    FormatSpec, FrameStrategy, FullExecution, FullFunctionView, FullPayload, FullProgram, FullTag,
    FunctionHeader,
    IndexedFmt, LoweredFunctionKey, LoweredFunctionKind,
    LoweredReturnKind,
    LoweredStrPredicate, LoweredType,
    LoweredTypeCheck, LoweredValue, Name, RegisteredDefer, ResolvedAssignStep, RuntimeError, Span,
    RedirectionKind, RunArg, RunKind, StmtFlow, StreamValue, TraceArg, TraceKind, TracePayload,
    TracebackFrame, TracebackFrameKind,
    append_call_argument, append_lowered_list_element, append_lowered_map_literal,
    apply_indexed_assignment, apply_indexed_path_assignment, assign_lowered_bytes_view,
    assign_lowered_str_view,
    bind_indexed_comp_target, bind_lowered_comp_target, capture_checked_error, check_env_name,
    checked_indexed_assignment, comparison_link_holds,
    comparison_pair_at, decode_comparison_chain, decode_match_expr,
    decode_record_updates, match_expr_arm_at, record_update_at,
    ComparisonChainPosition, MatchExprPosition, RecordUpdatePosition,
    decode_fmt, finish_record_entries, indexed_assignment_operand, indexed_callable_identity,
    indexed_error, indexed_value, lowered_binary_value, lowered_bytes_parts,
    lowered_comp_iterable, lowered_condition_bool, lowered_err_with_cause, lowered_fallback_value,
    lowered_map_literal_key, lowered_match_no_arm, lowered_path_like_arg,
    lowered_result_err_value, lowered_result_ok, lowered_str_key, lowered_str_parts,
    lowered_str_predicate_text,
    lowered_tag_key, lowered_trim_is_empty_value, lowered_trim_str_predicate_value,
    lowered_value_from_runtime_any, push_lowered_display,
    resolve_assign_index,
};
use crate::runtime::eval::indexed::full::ProgramOwner;
use super::{LoweredMapCursor, LoweredScalarCursor};
use crate::map_key::MapKey;
use crate::runtime::eval::indexed::IrBlockId;
use crate::runtime::eval::indexed::full::schema::{BoolView, ExprView, ExprViewKind, IntView};
use crate::runtime::eval::indexed::full::schema::StmtView;
use crate::runtime::eval::indexed::full::decode::{
    AssignStepView, CallArgView, CompQualifierView, CompTargetView, FmtPartView, ListPosition,
    ListView,
    ProcessCommandBuilderEntryView, RecordEntryView, RunArgKindView, RunArgView, RunEnvView,
    ScanConditionView,
};
use crate::runtime::eval::{
    BuildBoolId, BuildExprId, BuildPatternId, BuildStmtId, LoweredCallArg, LoweredAssignStep,
    LoweredCompQualifier, LoweredCompTarget, LoweredFmtPart, LoweredProcessCommandBuilderEntry,
    LoweredRecordEntry, LoweredRunArg, LoweredRunEnv, LoweredRunPipelineSegment,
    LoweredRunRedirection,
    visit_retained_lowered_value, visit_retained_lowered_values,
};
use crate::runtime::eval::lowered_ops::lowered_record_update_batch;
use crate::runtime::eval::lowered_run::validate_parameter_default;
use crate::runtime::eval::route_witness::{
    RouteWitnessMode, frame_route,
};
use crate::runtime::value::{DurationValue, Value};
use crate::trace::Traceback;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Forced-frame evaluation reports a tag whose only remaining route is recursive.
fn frame_strategy_error(tag: FullTag, span: Span) -> RuntimeError {
    RuntimeError::new(
        "frame-scheduler",
        format!(
            "frame-scheduler: instruction tag `{}` has no explicit frame handler",
            tag.name()
        ),
    )
    .with_span(span)
}

fn indexed_run_arg(kind: RunArgKindView<'_>, span: Span) -> super::RunArg {
    let (mode, value) = match kind {
        RunArgKindView::Single(value) => (0, value.raw()),
        RunArgKindView::SingleOrSplice(value) => (1, value.raw()),
        RunArgKindView::Splice(value) => (2, value.raw()),
    };
    super::RunArg { mode, value, span }
}

/// A statement block that has passed its eager list validation.
#[derive(Clone, Copy)]
pub(super) struct StatementBlock(IrBlockId);

/// The next statement to run without owning copies of the block's rows.
pub(super) enum StatementCursor {
    Block {
        block: StatementBlock,
        position: ListPosition<BuildStmtId>,
    },
    Single(Option<u32>),
    Empty,
}

impl StatementBlock {
    fn from_view(
        mut statements: ListView<'_, BuildStmtId>,
        span: Span,
    ) -> Result<(Self, ListPosition<BuildStmtId>), RuntimeError> {
        statements
            .preflight()
            .map_err(|error| indexed_error(error, span))?;
        Ok((Self(statements.block_identity()), statements.position()))
    }

    fn open(
        execution: &FullExecution<'_>,
        raw: u32,
        span: Span,
    ) -> Result<(Self, ListPosition<BuildStmtId>), RuntimeError> {
        let statements = execution
            .statement_block_view(raw)
            .map_err(|error| indexed_error(error, span))?;
        Self::from_view(statements, span)
    }

    fn open_id(
        execution: &FullExecution<'_>,
        id: IrBlockId,
        span: Span,
    ) -> Result<(Self, ListPosition<BuildStmtId>), RuntimeError> {
        let statements = execution
            .statement_block_view_id(id)
            .map_err(|error| indexed_error(error, span))?;
        Self::from_view(statements, span)
    }
}

impl StatementCursor {
    pub(super) fn block(
        execution: &FullExecution<'_>,
        raw: u32,
        span: Span,
    ) -> Result<Self, RuntimeError> {
        let (block, position) = StatementBlock::open(execution, raw, span)?;
        Ok(Self::Block { block, position })
    }

    pub(super) fn function_body(
        execution: &FullExecution<'_>,
        id: IrBlockId,
        span: Span,
    ) -> Result<Self, RuntimeError> {
        let (block, position) = StatementBlock::open_id(execution, id, span)?;
        Ok(Self::Block { block, position })
    }

    fn single(statement: u32) -> Self {
        Self::Single(Some(statement))
    }

    fn empty() -> Self {
        Self::Empty
    }

    fn next(
        &mut self,
        execution: &FullExecution<'_>,
        span: Span,
    ) -> Result<Option<u32>, RuntimeError> {
        match self {
            Self::Block { block, position } => {
                let mut statements = execution
                    .statement_block_view_id(block.0)
                    .map_err(|error| indexed_error(error, span))?;
                statements
                    .resume(*position)
                    .map_err(|error| indexed_error(error, span))?;
                let statement = statements
                    .next()
                    .transpose()
                    .map_err(|error| indexed_error(error, span))?;
                *position = statements.position();
                Ok(statement.map(|statement| statement.raw()))
            }
            Self::Single(statement) => Ok(statement.take()),
            Self::Empty => Ok(None),
        }
    }
}

/// Reopens one borrowed statement-list row at its saved position.
fn statement_print_argument(
    execution: &FullExecution<'_>,
    statement: u32,
    position: ListPosition<BuildExprId>,
    span: Span,
) -> Result<(u32, ListPosition<BuildExprId>), RuntimeError> {
    let (tag, payload) = execution
        .instruction_id(statement)
        .map_err(|error| indexed_error(error, span))?;
    let StmtView::Print { mut args, .. } = StmtView::decode(execution, tag, payload)
        .map_err(|error| indexed_error(error, span))?
    else {
        unreachable!("statement ID and borrowed print row agree")
    };
    args.resume(position)
        .map_err(|error| indexed_error(error, span))?;
    let argument = args
        .next()
        .transpose()
        .map_err(|error| indexed_error(error, span))?
        .ok_or_else(|| {
            RuntimeError::new("indexed-ir", "print argument cursor ended early").with_span(span)
        })?;
    Ok((argument.raw(), args.position()))
}

type StatementWithBindingPosition = ListPosition<(usize, BuildExprId)>;
type CallArgumentPosition = ListPosition<LoweredCallArg>;
type MethodArgumentPosition = ListPosition<BuildExprId>;
type ModuleArgumentPosition = ListPosition<Option<BuildExprId>>;
type StatementIfExprPosition = ListPosition<(BuildExprId, Vec<BuildStmtId>)>;
type StatementIfBoolPosition = ListPosition<(BuildBoolId, Vec<BuildStmtId>)>;
type StatementPatternIfPosition =
    ListPosition<(BuildExprId, Vec<BuildStmtId>, Vec<usize>)>;
type StatementMatchPosition =
    ListPosition<(BuildPatternId, Option<BuildExprId>, Vec<BuildStmtId>)>;

#[derive(Clone, Copy)]
enum CallArgumentMode {
    Single,
    Splice,
}

enum CallArgumentOperand {
    Default(usize),
    Expression {
        instruction: u32,
        mode: CallArgumentMode,
    },
}

impl CallArgumentMode {
    fn runtime_kind(self) -> u32 {
        match self {
            Self::Single => 0,
            Self::Splice => 1,
        }
    }
}

#[derive(Clone, Copy)]
enum StatementIfPosition {
    Expr(StatementIfExprPosition),
    Bool(StatementIfBoolPosition),
}

fn statement_with_binding(
    execution: &FullExecution<'_>,
    statement: u32,
    position: StatementWithBindingPosition,
    span: Span,
) -> Result<(Option<(usize, u32)>, StatementWithBindingPosition), RuntimeError> {
    let (tag, payload) = execution
        .instruction_id(statement)
        .map_err(|error| indexed_error(error, span))?;
    let StmtView::With { mut bindings, .. } = StmtView::decode(execution, tag, payload)
        .map_err(|error| indexed_error(error, span))?
    else {
        unreachable!("statement ID and borrowed with row agree")
    };
    bindings
        .resume(position)
        .map_err(|error| indexed_error(error, span))?;
    let binding = bindings
        .next()
        .transpose()
        .map_err(|error| indexed_error(error, span))?
        .map(|(slot, value)| (slot, value.raw()));
    Ok((binding, bindings.position()))
}

fn statement_if_branch(
    execution: &FullExecution<'_>,
    statement: u32,
    position: StatementIfPosition,
    span: Span,
) -> Result<(Option<(u32, u32)>, StatementIfPosition), RuntimeError> {
    let (tag, payload) = execution
        .instruction_id(statement)
        .map_err(|error| indexed_error(error, span))?;
    let view = StmtView::decode(execution, tag, payload)
        .map_err(|error| indexed_error(error, span))?;
    match (view, position) {
        (StmtView::If { mut branches, .. }, StatementIfPosition::Expr(position)) => {
            branches
                .resume(position)
                .map_err(|error| indexed_error(error, span))?;
            let branch = branches
                .next()
                .transpose()
                .map_err(|error| indexed_error(error, span))?
                .map(|(condition, body)| (condition.raw(), body.block_id()));
            Ok((branch, StatementIfPosition::Expr(branches.position())))
        }
        (StmtView::IfBool { mut branches, .. }, StatementIfPosition::Bool(position)) => {
            branches
                .resume(position)
                .map_err(|error| indexed_error(error, span))?;
            let branch = branches
                .next()
                .transpose()
                .map_err(|error| indexed_error(error, span))?
                .map(|(condition, body)| (condition.raw(), body.block_id()));
            Ok((branch, StatementIfPosition::Bool(branches.position())))
        }
        _ => unreachable!("if continuation keeps the row's branch-list type"),
    }
}

fn statement_pattern_if_branch(
    execution: &FullExecution<'_>,
    statement: u32,
    position: StatementPatternIfPosition,
    span: Span,
) -> Result<
    (
        Option<(u32, u32, Vec<usize>)>,
        StatementPatternIfPosition,
    ),
    RuntimeError,
> {
    let (tag, payload) = execution
        .instruction_id(statement)
        .map_err(|error| indexed_error(error, span))?;
    let StmtView::PatternIf { mut branches, .. } = StmtView::decode(execution, tag, payload)
        .map_err(|error| indexed_error(error, span))?
    else {
        unreachable!("statement ID and borrowed pattern-if row agree")
    };
    branches
        .resume(position)
        .map_err(|error| indexed_error(error, span))?;
    let branch = branches
        .next()
        .transpose()
        .map_err(|error| indexed_error(error, span))?
        .map(|(condition, body, captures)| {
            captures
                .collect::<Result<Vec<_>, _>>()
                .map(|captures| (condition.raw(), body.block_id(), captures))
        })
        .transpose()
        .map_err(|error| indexed_error(error, span))?;
    Ok((branch, branches.position()))
}

fn statement_match_arm(
    execution: &FullExecution<'_>,
    statement: u32,
    position: StatementMatchPosition,
    span: Span,
) -> Result<(Option<(u32, Option<u32>, u32)>, StatementMatchPosition), RuntimeError> {
    let (tag, payload) = execution
        .instruction_id(statement)
        .map_err(|error| indexed_error(error, span))?;
    let StmtView::Match { mut arms, .. } = StmtView::decode(execution, tag, payload)
        .map_err(|error| indexed_error(error, span))?
    else {
        unreachable!("statement ID and borrowed match row agree")
    };
    arms.resume(position)
        .map_err(|error| indexed_error(error, span))?;
    let arm = arms
        .next()
        .transpose()
        .map_err(|error| indexed_error(error, span))?
        .map(|(pattern, guard, body)| {
            (pattern.raw(), guard.map(|guard| guard.raw()), body.block_id())
        });
    Ok((arm, arms.position()))
}

fn statement_env_assignment<'e>(
    execution: &'e FullExecution<'_>,
    statement: u32,
    position: ListPosition<LoweredRunEnv>,
    span: Span,
) -> Result<(RunEnvView<'e>, ListPosition<LoweredRunEnv>), RuntimeError> {
    let (tag, payload) = execution
        .instruction_id(statement)
        .map_err(|error| indexed_error(error, span))?;
    let StmtView::Env { mut env, .. } = StmtView::decode(execution, tag, payload)
        .map_err(|error| indexed_error(error, span))?
    else {
        unreachable!("statement ID and borrowed environment row agree")
    };
    env.resume(position)
        .map_err(|error| indexed_error(error, span))?;
    let assignment = env
        .next()
        .transpose()
        .map_err(|error| indexed_error(error, span))?
        .ok_or_else(|| {
            RuntimeError::new("indexed-ir", "environment cursor ended early").with_span(span)
        })?;
    Ok((assignment, env.position()))
}

fn indexed_str_tag_match_arm(
    view: super::ExprView<'_>,
    key: Option<&str>,
    span: Span,
) -> Result<u32, RuntimeError> {
    let (arms, fallback, match_span) = match view {
        super::ExprView::StrMatchExpr { arms, fallback, span, .. }
        | super::ExprView::TagMatchExpr { arms, fallback, span, .. } => {
            (arms, fallback, span)
        }
        _ => unreachable!("string/tag match tag has a matching borrowed view"),
    };
    if let Some(key) = key {
        for arm in arms {
            let (candidate, value) = arm.map_err(|error| indexed_error(error, span))?;
            if candidate == key {
                return Ok(value.raw());
            }
        }
    }
    fallback
        .map(|value| value.raw())
        .ok_or_else(|| lowered_match_no_arm(match_span))
}

fn indexed_str_tag_statement_match_arm(
    view: StmtView<'_>,
    key: Option<&str>,
    span: Span,
) -> Result<u32, RuntimeError> {
    let (arms, fallback, match_span) = match view {
        StmtView::StrMatch { arms, fallback, span, .. }
        | StmtView::TagMatch { arms, fallback, span, .. } => (arms, fallback, span),
        _ => unreachable!("string/tag statement match tag has a matching borrowed view"),
    };
    if let Some(key) = key {
        for arm in arms {
            let (candidate, body) = arm.map_err(|error| indexed_error(error, span))?;
            if candidate == key {
                return Ok(body.block_id());
            }
        }
    }
    fallback
        .map(|body| body.block_id())
        .ok_or_else(|| lowered_match_no_arm(match_span))
}

fn expr_pattern_if_view<'e>(
    execution: &'e FullExecution<'_>,
    instruction: u32,
    span: Span,
) -> Result<super::ExprView<'e>, RuntimeError> {
    let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
    super::ExprView::decode(execution, tag, payload).map_err(|error| indexed_error(error, span))
}

fn expr_if_branches<'e>(
    execution: &'e FullExecution<'_>,
    instruction: u32,
    span: Span,
) -> Result<ListView<'e, (BuildExprId, BuildExprId)>, RuntimeError> {
    let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
    match super::ExprView::decode(execution, tag, payload)
        .map_err(|error| indexed_error(error, span))?
    {
        super::ExprView::IfExpr { branches, .. } => Ok(branches),
        _ => unreachable!("if-expression instruction has an if-expression view"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::eval::route_witness::{RouteWitnessRoute, RouteWitnessSink};
    use crate::runtime::value::{ScriptStream, ScriptStreamState, ScriptStreamStep, Value};
    use crate::source::SourceMap;
    use crate::syntax::parser::Parser;

    fn full_program(source: &str) -> (Arc<FullProgram>, crate::source::SourceId) {
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("explicit-frame-test.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = crate::sema::check::Checker::check_arena_with_sources_and_options(
            &parsed.arena,
            &sources,
            source_id,
            crate::sema::check::CheckOptions {
                reveal_types: true,
                embedded_bodies: true,
                ..crate::sema::check::CheckOptions::default()
            },
        );
        let declarations =
            crate::sema::check::Checker::compact_declarations(&parsed.arena, checked);
        assert!(
            declarations.diagnostics.is_empty(),
            "{:?}",
            declarations.diagnostics
        );
        let program = crate::runtime::eval::FullBuilder::build_compact(
            &parsed.arena,
            &declarations,
            source,
            Arc::new(sources),
            source_id,
        )
        .unwrap();
        (Arc::new(program), source_id)
    }

    fn function_instruction(
        program: &FullProgram,
        execution: &FullExecution<'_>,
        tag: FullTag,
    ) -> u32 {
        let matches = (0..program.instruction_count())
            .filter_map(|instruction| {
                let actual = execution.instruction_id(instruction as u32).ok()?.0;
                (actual == tag).then_some(instruction as u32)
            })
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "expected one {tag:?} instruction");
        matches[0]
    }

    #[test]
    fn empty_map_witnesses_capture_the_selected_dispatch_route() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure empty() -> Map[Int] { return map.empty() }\n";
            let (program, source_id) = full_program(source);
            let function = program
                .symbol_owner()
                .with_current(|| Name::intern("empty"));
            let view = program
                .function_view(
                    LoweredFunctionKey::Name(function),
                    LoweredFunctionKind::Pure,
                )
                .unwrap()
                .expect("empty map function is present");
            let execution = view.execution().unwrap();
            let instruction = function_instruction(&program, &execution, FullTag::ExprEmptyMap);
            let span = Span::new(source_id, 0, 0);

            for (forced, expected_route, expected_mode) in [
                (
                    false,
                    RouteWitnessRoute::ExplicitFrameApplyPath,
                    RouteWitnessMode::Normal,
                ),
                (
                    true,
                    RouteWitnessRoute::GenericOperandApplyPath,
                    RouteWitnessMode::Forced,
                ),
            ] {
                let sink = RouteWitnessSink::default();
                let mut evaluator = Evaluator::new(Vec::new());
                evaluator.indexed_program = Some(Arc::clone(&program));
                evaluator.force_frames = forced;
                evaluator.install_route_witness_sink(sink.clone());
                let mut slots = vec![LoweredValue::Unit; view.slot_count()];

                let result = evaluator
                    .eval_indexed_expr(&execution, instruction, &mut slots, span)
                    .expect("empty map evaluation succeeds");
                assert!(matches!(
                    result,
                    ControlFlow::Continue(LoweredValue::Map(ref value)) if value.is_empty()
                ));
                assert!(evaluator.stdout.is_empty());
                assert!(evaluator.stderr.is_empty());

                let records = sink.take_records();
                assert_eq!(records.len(), 1);
                assert_eq!(records[0].tag.name, "ExprEmptyMap");
                assert_eq!(records[0].tag.ordinal, FullTag::ExprEmptyMap as u32);
                assert_eq!(records[0].route, expected_route);
                assert_eq!(records[0].mode, expected_mode);
                assert_eq!(records[0].route.name(), expected_route.name());
                assert_eq!(records[0].mode.name(), expected_mode.name());
            }
        });
    }

    #[test]
    fn forced_frames_run_an_expression_loop() {
        crate::runtime::eval::run_eval(|| {
            let source = "proc recursive_only() -> Int { return loop { break 0 } }\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("frame-scheduler.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let function = parsed
                .arena
                .symbol_owner()
                .with_current(|| Name::intern("recursive_only"));
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
            assert!(evaluator.prepare_compact_indexed_only(&parsed.arena, source_id).is_some());
            evaluator.force_frames = true;
            let value = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(function),
                    LoweredFunctionKind::Proc,
                    &[],
                    Span::new(source_id, 0, 0),
                )
                .expect("prepared recursive procedure")
                .expect("forced frames run expression loops");
            assert!(matches!(value, Value::Int(0)));
        });
    }

    #[test]
    fn complete_pipeline_decode_preserves_the_stage_count() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure pipeline() -> List[Int] { return [1] |> map { |item| item } |> sort |> map { |item| item } |> collect() }\n";
            let (program, source_id) = full_program(source);
            let function = program.symbol_owner().with_current(|| {
                program
                    .function_view(
                        LoweredFunctionKey::Name(Name::intern("pipeline")),
                        LoweredFunctionKind::Pure,
                    )
                    .unwrap()
                    .expect("pipeline function is present")
            });
            let execution = function.execution().unwrap();
            let instruction = function_instruction(&program, &execution, FullTag::ExprPipeline);
            let span = Span::new(source_id, 0, 0);
            let (_, _, stages) = indexed_value_pipeline_input(&execution, instruction, span)
                .unwrap();

            assert_eq!(stages.len(), 4);
        });
    }

    struct RetainedFrameStream(ProducerFrameState);

    impl ScriptStream for RetainedFrameStream {
        fn finished(&self) -> bool { false }

        fn poll(
            &mut self,
            _evaluator: &mut Evaluator,
            _span: Span,
        ) -> Result<ScriptStreamStep, RuntimeError> {
            Ok(ScriptStreamStep::Finished)
        }

        fn validate_item(&self, _value: &Value, _span: Span) -> Result<(), RuntimeError> {
            Ok(())
        }

        fn visit_retained_values(
            &self,
            visitor: &mut dyn for<'a> FnMut(crate::runtime::eval::RetainedValueRef<'a>),
        ) {
            self.0.visit_retained_values(visitor);
        }

        fn delegated_finished(&mut self) {}

        fn take_delegated(
            &mut self,
        ) -> (
            Option<ScriptStreamState>,
            Vec<u64>,
            Option<crate::runtime::eval::ScopedProducerContext>,
        ) {
            (None, Vec::new(), None)
        }

        fn cancel(&mut self, _evaluator: &mut Evaluator, _span: Span) -> Result<(), RuntimeError> {
            Ok(())
        }
    }

    #[test]
    fn continuation_retained_root_transfers_with_its_producer() {
        crate::runtime::eval::run_eval(|| {
            let mut evaluator = Evaluator::new(Vec::new());
            let root_scope = evaluator.current_scope_id();
            let child_scope = evaluator.enter_owned_host_scope();
            let temp = tempfile::tempdir().unwrap();
            let root = xsh_root::Root::open(temp.path()).unwrap();
            let root_id = evaluator.push_fs_capability(crate::runtime::eval::FsCapability::Root(
                Box::new(super::super::super::FsRootHandle::TempDir {
                    root,
                    _temp: temp,
                }),
            ));
            let apply_temp = tempfile::tempdir().unwrap();
            let apply_root = xsh_root::Root::open(apply_temp.path()).unwrap();
            let apply_root_id =
                evaluator.push_fs_capability(crate::runtime::eval::FsCapability::Root(Box::new(
                    super::super::super::FsRootHandle::TempDir {
                        root: apply_root,
                        _temp: apply_temp,
                    },
                )));
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("retained-frame.xsh", "");
            let span = Span::new(source_id, 0, 0);
            let retained_root = LoweredValue::FsRoot(crate::runtime::value::FsRootValue {
                id: root_id,
                owner: Arc::clone(&evaluator.resource_owner),
            });
            let apply_retained_root =
                LoweredValue::FsRoot(crate::runtime::value::FsRootValue {
                    id: apply_root_id,
                    owner: Arc::clone(&evaluator.resource_owner),
                });
            let frame = ProducerFrameState {
                work: vec![
                    FrameWork::Expr {
                        instruction: 0,
                        span,
                        next: FrameContinuation::BinaryRight {
                            op: BinaryOp::Add,
                            left: retained_root.clone(),
                            span,
                            next: Box::new(FrameContinuation::Discard(span)),
                        },
                    },
                    FrameWork::Expr {
                        instruction: 0,
                        span,
                        next: FrameContinuation::ApplyOperands {
                            instruction: 0,
                            tag: FullTag::ExprRange,
                            position: 1,
                            tag_position: None,
                            error_field_position: None,
                            values: vec![super::super::FrameApplyValue::Value(apply_retained_root)],
                            span,
                            next: Box::new(FrameContinuation::Discard(span)),
                        },
                    },
                ],
                slots: Vec::new(),
                slot_scopes: Vec::new(),
                defers: Vec::new(),
                block_scopes: Vec::new(),
                block_defer_offsets: Vec::new(),
                scope_id: child_scope,
            };
            let state = ScriptStreamState::new(RetainedFrameStream(frame));
            let mut stream = StreamValue::from_script(state);
            evaluator.register_returned_stream(&mut stream, span).unwrap();

            evaluator.transfer_owned_host_resources_in_stream(&stream, root_scope);

            assert_eq!(
                evaluator.fs_capabilities[0].as_ref().unwrap().owner_scope,
                root_scope
            );
            assert_eq!(
                evaluator.fs_capabilities[1].as_ref().unwrap().owner_scope,
                root_scope
            );
            evaluator.exit_owned_host_scope(child_scope).unwrap();
        });
    }

    #[test]
    fn continuation_transfers_materialized_fold_root_with_its_producer() {
        crate::runtime::eval::run_eval(|| {
            let source = "pure folded() -> Int { [1] |> fold(0) { |acc, item| acc + item } }\n";
            let (program, source_id) = full_program(source);
            let function = program
                .symbol_owner()
                .with_current(|| Name::intern("folded"));
            let function_view = program
                .function_view(LoweredFunctionKey::Name(function), LoweredFunctionKind::Pure)
                .unwrap()
                .expect("folded function is present");
            let execution = function_view.execution().unwrap();
            let pipeline_instruction =
                function_instruction(&program, &execution, FullTag::ExprPipeline);
            let span = Span::new(source_id, 0, 0);
            let (_, span, stages) = super::super::serial_pipeline::indexed_value_pipeline_input(
                &execution,
                pipeline_instruction,
                span,
            )
            .unwrap();
            assert_eq!(stages.len(), 1);

            let mut evaluator = Evaluator::new(Vec::new());
            let root_scope = evaluator.current_scope_id();
            let child_scope = evaluator.enter_owned_host_scope();
            let stage_temp = tempfile::tempdir().unwrap();
            let stage_root = xsh_root::Root::open(stage_temp.path()).unwrap();
            let stage_root_id = evaluator.push_fs_capability(
                crate::runtime::eval::FsCapability::Root(Box::new(
                    super::super::super::FsRootHandle::TempDir {
                        root: stage_root,
                        _temp: stage_temp,
                    },
                )),
            );
            let continuation_temp = tempfile::tempdir().unwrap();
            let continuation_root = xsh_root::Root::open(continuation_temp.path()).unwrap();
            let continuation_root_id = evaluator.push_fs_capability(
                crate::runtime::eval::FsCapability::Root(Box::new(
                    super::super::super::FsRootHandle::TempDir {
                        root: continuation_root,
                        _temp: continuation_temp,
                    },
                )),
            );
            let stage_value = LoweredValue::FsRoot(crate::runtime::value::FsRootValue {
                id: stage_root_id,
                owner: Arc::clone(&evaluator.resource_owner),
            });
            let continuation_value = LoweredValue::FsRoot(crate::runtime::value::FsRootValue {
                id: continuation_root_id,
                owner: Arc::clone(&evaluator.resource_owner),
            });

            let mut pipeline = IndexedStageEngine::new_frame(
                LoweredValue::List(vec![LoweredValue::Int(1)]),
                stages,
                span,
            )
            .unwrap();
            let mut slots = vec![LoweredValue::Unit; function_view.slot_count()];
            assert!(matches!(
                pipeline.next_frame(&mut evaluator, &execution, &mut slots),
                Ok(IndexedStageStep::Expr(_))
            ));
            pipeline.resume(ControlFlow::Continue(stage_value));
            assert!(matches!(
                pipeline.next_frame(&mut evaluator, &execution, &mut slots),
                Ok(IndexedStageStep::Block(_))
            ));
            // The callback receives a copy in its local slots; isolate the
            // accumulator's suspended retention path from that ordinary copy.
            slots.fill(LoweredValue::Unit);

            let frame = ProducerFrameState {
                work: vec![FrameWork::StageValue {
                    pipeline: Box::new(pipeline),
                    span,
                    next: FrameContinuation::BinaryRight {
                        op: BinaryOp::Add,
                        left: continuation_value,
                        span,
                        next: Box::new(FrameContinuation::Discard(span)),
                    },
                }],
                slots,
                slot_scopes: Vec::new(),
                defers: Vec::new(),
                block_scopes: Vec::new(),
                block_defer_offsets: Vec::new(),
                scope_id: child_scope,
            };
            let state = ScriptStreamState::new(RetainedFrameStream(frame));
            let mut stream = StreamValue::from_script(state);
            evaluator.register_returned_stream(&mut stream, span).unwrap();

            evaluator.transfer_owned_host_resources_in_stream(&stream, root_scope);

            for root_id in [stage_root_id, continuation_root_id] {
                assert_eq!(
                    evaluator.fs_capabilities[root_id as usize - 1]
                        .as_ref()
                        .unwrap()
                        .owner_scope,
                    root_scope
                );
            }
            evaluator.exit_owned_host_scope(child_scope).unwrap();
            assert!(evaluator.fs_capabilities[stage_root_id as usize - 1].is_some());
            assert!(evaluator.fs_capabilities[continuation_root_id as usize - 1].is_some());
        });
    }

    #[test]
    fn range_start_type_error_precedes_later_operand_effects() {
        crate::runtime::eval::run_eval(|| {
            let source = r#"
proc later(divisor: Int) [] -> Int {
  print "later"
  1 / divisor
}
proc range_case(start: Int, divisor: Int) [] -> Any {
  range(start, later(divisor))
}
"#;
            let (program, source_id) = full_program(source);
            let function = program
                .symbol_owner()
                .with_current(|| Name::intern("range_case"));
            let function_view = program
                .function_view(LoweredFunctionKey::Name(function), LoweredFunctionKind::Proc)
                .unwrap()
                .expect("range_case function is present");
            let execution = function_view.execution().unwrap();
            let range_instruction = function_instruction(&program, &execution, FullTag::ExprRange);
            let span = Span::new(source_id, 0, 0);
            let mut evaluator = Evaluator::new(Vec::new());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let mut slots = vec![LoweredValue::Unit; function_view.slot_count()];
            slots[0] = LoweredValue::Str(Arc::from("bad start"));
            slots[1] = LoweredValue::Int(0);

            let error = evaluator
                .eval_indexed_expr_with_frames(
                    &execution,
                    range_instruction,
                    &mut slots,
                    span,
                )
                .expect_err("invalid start bound fails before the later call");

            assert_eq!(error.kind, "type-error");
            assert!(error.message.contains("range start expected Int, found Str"));
            assert!(evaluator.stdout.is_empty(), "later operand ran: {:?}", evaluator.stdout);
        });
    }

    // Native scripts terminate on a runtime fault. Reusing that same evaluator
    // after the fault is a host lifecycle that only the library caller can test.
    #[test]
    fn nested_machine_depth_is_restored_after_an_execution_error() {
        crate::runtime::eval::run_eval(|| {
            let source = r#"proc descend(n: Int) -> Int {
  let values = [n] |> map { |item|
    let next = item + 1
    descend(next)
  }
  values[0]
}
pure increment(n: Int) -> Int { n + 1 }
"#;
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("nested-machine-depth.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
            assert!(evaluator.prepare_compact_indexed_only(&parsed.arena, source_id).is_some());
            let (descend, increment) = parsed.arena.symbol_owner().with_current(|| {
                (Name::intern("descend"), Name::intern("increment"))
            });
            let span = Span::new(source_id, 0, 0);
            let error = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(descend),
                    LoweredFunctionKind::Proc,
                    &[Value::Int(0)],
                    span,
                )
                .expect("prepared recursive function")
                .expect_err("nested machines have a bounded depth");
            assert_eq!(error.kind, "stack-overflow");
            assert_eq!(evaluator.frame_machine_depth.load(Ordering::Relaxed), 0);
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(increment),
                    LoweredFunctionKind::Pure,
                    &[Value::Int(41)],
                    span,
                )
                .expect("prepared ordinary function")
                .expect("the failed call leaves the evaluator reusable");
            assert_eq!(result, Value::Int(42));
            assert_eq!(evaluator.frame_machine_depth.load(Ordering::Relaxed), 0);
        });
    }
}

enum FrameValue {
    Value(LoweredValue),
    Break(LoweredValue),
}

// Pending projections and the work stack share producer ownership so error and
// return unwinding can cancel every active producer from innermost to outermost.
// The mutex keeps frame state movable between evaluator threads; only the
// active evaluator pulls the streams.
type CompStreams = Arc<std::sync::Mutex<Vec<Option<(StreamValue, Span)>>>>;

enum CompIterator {
    Scalars {
        cursor: LoweredScalarCursor,
        active: CompIteratorState,
    },
    Map {
        cursor: LoweredMapCursor,
        active: CompIteratorState,
    },
    Items {
        items: std::vec::IntoIter<LoweredValue>,
        active: CompIteratorState,
    },
    Stream {
        stream: usize,
        active: CompIteratorState,
    },
}

struct CompIteratorState {
    instruction: u32,
    target_position: ListPosition<LoweredCompQualifier>,
    span: Span,
    next_qualifier: ListPosition<LoweredCompQualifier>,
}

struct ListCompState {
    map: bool,
    key: Option<u32>,
    value: u32,
    instruction: u32,
    position: ListPosition<LoweredCompQualifier>,
    iterators: Vec<CompIterator>,
    streams: CompStreams,
    values: Vec<LoweredValue>,
    map_values: BTreeMap<MapKey, LoweredValue>,
    span: Span,
}

impl ListCompState {
    fn visit_retained_values(
        &self,
        visitor: &mut dyn for<'a> FnMut(crate::runtime::eval::RetainedValueRef<'a>),
    ) {
        for iterator in &self.iterators {
            match iterator {
                CompIterator::Scalars { .. } | CompIterator::Stream { .. } => {}
                CompIterator::Map { cursor, .. } => cursor.visit_retained_values(visitor),
                CompIterator::Items { items, .. } => {
                    visit_retained_lowered_values(items.as_slice(), visitor);
                }
            }
        }
        for stream in self
            .streams
            .lock()
            .expect("comprehension stream state poisoned")
            .iter()
            .flatten()
        {
            visitor(crate::runtime::eval::RetainedValueRef::Stream(&stream.0));
        }
        visit_retained_lowered_values(&self.values, visitor);
        visit_retained_lowered_values(self.map_values.values(), visitor);
    }
}

fn visit_retained_runtime(
    value: &crate::runtime::value::Value,
    visitor: &mut dyn for<'a> FnMut(crate::runtime::eval::RetainedValueRef<'a>),
) {
    visitor(crate::runtime::eval::RetainedValueRef::Runtime(value));
}

fn visit_retained_runtime_error(
    error: &RuntimeError,
    visitor: &mut dyn for<'a> FnMut(crate::runtime::eval::RetainedValueRef<'a>),
) {
    for (_, value) in error.payload.iter() {
        visit_retained_runtime(value, visitor);
    }
    if let Some(cause) = &error.cause {
        visit_retained_runtime(cause.as_value(), visitor);
    }
    if let Some(cause) = error
        .propagated_run_error
        .as_ref()
        .and_then(|error| error.cause.as_ref())
    {
        visit_retained_runtime(cause.as_value(), visitor);
    }
}

struct RetryFrameState {
    instruction: u32,
    delay_position: ListPosition<BuildExprId>,
    pattern: Option<u32>,
    body: u32,
    span: Span,
    schedule_kind: crate::syntax::arena::RetrySchedule,
    delay_values: Vec<DurationValue>,
    delays: Option<LoweredRetryDelays>,
    attempt_index: usize,
    max_attempts: usize,
    last_error: Option<Value>,
    last_traceback: Option<Traceback>,
}

impl RetryFrameState {
    fn visit_retained_values(
        &self,
        visitor: &mut dyn for<'a> FnMut(crate::runtime::eval::RetainedValueRef<'a>),
    ) {
        if let Some(error) = &self.last_error {
            visit_retained_runtime(error, visitor);
        }
    }
}

struct FmtState {
    instruction: u32,
    position: ListPosition<LoweredFmtPart>,
    fmt: IndexedFmt,
    span: Span,
}

type IndexedMapLiteralEntry = (Option<BuildExprId>, BuildExprId, Span);

enum IndexedListLiteralPosition {
    List(ListPosition<BuildExprId>),
    Build(ListPosition<(bool, BuildExprId, Span)>),
}

struct ListItemsState {
    instruction: u32,
    position: IndexedListLiteralPosition,
    splice: bool,
    item_span: Span,
    values: Vec<LoweredValue>,
    span: Span,
}

struct MapLiteralState {
    instruction: u32,
    position: ListPosition<IndexedMapLiteralEntry>,
    fields: BTreeMap<MapKey, LoweredValue>,
    key: Option<MapKey>,
    reading_key: bool,
    span: Span,
}

struct RecordItemsState {
    instruction: u32,
    position: ListPosition<LoweredRecordEntry>,
    fields: Vec<(Name, LoweredValue)>,
    span: Span,
}

enum IndexedListLiteralView<'a> {
    List(ListView<'a, BuildExprId>),
    Build(ListView<'a, (bool, BuildExprId, Span)>),
}

impl IndexedListLiteralView<'_> {
    fn len(&self) -> usize {
        match self {
            Self::List(items) => items.len(),
            Self::Build(items) => items.len(),
        }
    }

    fn preflight(&mut self, span: Span) -> Result<(), RuntimeError> {
        match self {
            Self::List(items) => items.preflight(),
            Self::Build(items) => items.preflight(),
        }
        .map_err(|error| indexed_error(error, span))
    }

    fn position(&self) -> IndexedListLiteralPosition {
        match self {
            Self::List(items) => IndexedListLiteralPosition::List(items.position()),
            Self::Build(items) => IndexedListLiteralPosition::Build(items.position()),
        }
    }

    fn resume(
        &mut self,
        position: IndexedListLiteralPosition,
        span: Span,
    ) -> Result<(), RuntimeError> {
        match (self, position) {
            (Self::List(items), IndexedListLiteralPosition::List(position)) => {
                items.resume(position)
            }
            (Self::Build(items), IndexedListLiteralPosition::Build(position)) => {
                items.resume(position)
            }
            _ => return Err(RuntimeError::new("indexed-ir", "list cursor is invalid").with_span(span)),
        }
        .map_err(|error| indexed_error(error, span))
    }

    fn next_operand(&mut self, span: Span) -> Result<Option<(u32, bool, Span)>, RuntimeError> {
        let operand = match self {
            Self::List(items) => items
                .next()
                .transpose()
                .map_err(|error| indexed_error(error, span))?
                .map(|instruction| (instruction.raw(), false, span)),
            Self::Build(items) => items
                .next()
                .transpose()
                .map_err(|error| indexed_error(error, span))?
                .map(|(splice, instruction, span)| (instruction.raw(), splice, span)),
        };
        Ok(operand)
    }
}

fn indexed_list_literal_view<'e>(
    execution: &'e FullExecution<'_>,
    tag: FullTag,
    payload: FullPayload<'e>,
    span: Span,
) -> Result<IndexedListLiteralView<'e>, RuntimeError> {
    let view = ExprView::decode(execution, tag, payload)
        .map_err(|error| indexed_error(error, span))?;
    match view {
        ExprView::List(items) => Ok(IndexedListLiteralView::List(items)),
        ExprView::ListBuild(items) => Ok(IndexedListLiteralView::Build(items)),
        _ => Err(RuntimeError::new("indexed-ir", "list literal view is invalid").with_span(span)),
    }
}

fn indexed_list_literal_view_at<'e>(
    execution: &'e FullExecution<'_>,
    instruction: u32,
    span: Span,
) -> Result<IndexedListLiteralView<'e>, RuntimeError> {
    let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
    indexed_list_literal_view(execution, tag, payload, span)
}

fn indexed_map_literal_view<'e>(
    execution: &'e FullExecution<'_>,
    tag: FullTag,
    payload: FullPayload<'e>,
    span: Span,
) -> Result<ListView<'e, IndexedMapLiteralEntry>, RuntimeError> {
    match ExprView::decode(execution, tag, payload)
        .map_err(|error| indexed_error(error, span))?
    {
        ExprView::MapLiteral(entries) => Ok(entries),
        _ => Err(RuntimeError::new("indexed-ir", "map literal view is invalid").with_span(span)),
    }
}

fn indexed_map_literal_view_at<'e>(
    execution: &'e FullExecution<'_>,
    instruction: u32,
    span: Span,
) -> Result<ListView<'e, IndexedMapLiteralEntry>, RuntimeError> {
    let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
    indexed_map_literal_view(execution, tag, payload, span)
}

fn indexed_record_literal_view<'e>(
    execution: &'e FullExecution<'_>,
    tag: FullTag,
    payload: FullPayload<'e>,
    span: Span,
) -> Result<ListView<'e, LoweredRecordEntry>, RuntimeError> {
    match ExprView::decode(execution, tag, payload)
        .map_err(|error| indexed_error(error, span))?
    {
        ExprView::Record(entries) => Ok(entries),
        _ => Err(RuntimeError::new("indexed-ir", "record literal view is invalid").with_span(span)),
    }
}

fn indexed_record_literal_view_at<'e>(
    execution: &'e FullExecution<'_>,
    instruction: u32,
    span: Span,
) -> Result<ListView<'e, LoweredRecordEntry>, RuntimeError> {
    let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
    indexed_record_literal_view(execution, tag, payload, span)
}

fn indexed_fmt_parts_view_at<'e>(
    execution: &'e FullExecution<'_>,
    instruction: u32,
    span: Span,
) -> Result<ListView<'e, LoweredFmtPart>, RuntimeError> {
    let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
    match ExprView::decode(execution, tag, payload)
        .map_err(|error| indexed_error(error, span))?
    {
        ExprView::FmtString(parts) | ExprView::PathFmtString { parts, .. } => Ok(parts),
        _ => Err(RuntimeError::new("indexed-ir", "format string view is invalid").with_span(span)),
    }
}

fn indexed_assign_path_view_at<'e>(
    execution: &'e FullExecution<'_>,
    statement: u32,
    span: Span,
) -> Result<ListView<'e, LoweredAssignStep>, RuntimeError> {
    let (tag, payload) = indexed_value(execution.instruction_id(statement), span)?;
    match StmtView::decode(execution, tag, payload)
        .map_err(|error| indexed_error(error, span))?
    {
        StmtView::AssignPath { path, .. } => Ok(path),
        _ => Err(RuntimeError::new("indexed-ir", "assignment path view is invalid")
            .with_span(span)),
    }
}

fn indexed_comp_qualifiers_view_at<'e>(
    execution: &'e FullExecution<'_>,
    instruction: u32,
    span: Span,
) -> Result<ListView<'e, LoweredCompQualifier>, RuntimeError> {
    let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
    match ExprView::decode(execution, tag, payload)
        .map_err(|error| indexed_error(error, span))?
    {
        ExprView::ListComp { qualifiers, .. } | ExprView::MapComp { qualifiers, .. } => {
            Ok(qualifiers)
        }
        _ => Err(RuntimeError::new("indexed-ir", "comprehension qualifier view is invalid")
            .with_span(span)),
    }
}

fn indexed_comp_target_view_at<'e>(
    execution: &'e FullExecution<'_>,
    instruction: u32,
    position: ListPosition<LoweredCompQualifier>,
    span: Span,
) -> Result<CompTargetView<'e>, RuntimeError> {
    let mut qualifiers = indexed_comp_qualifiers_view_at(execution, instruction, span)?;
    qualifiers
        .resume(position)
        .map_err(|error| indexed_error(error, span))?;
    match qualifiers
        .next()
        .transpose()
        .map_err(|error| indexed_error(error, span))?
    {
        Some(CompQualifierView::For { target, .. }) => Ok(target),
        _ => Err(RuntimeError::new(
            "indexed-ir",
            "comprehension target cursor is invalid",
        )
        .with_span(span)),
    }
}

struct FrameAssertionState {
    condition: u32,
    message: Option<u32>,
    span: Span,
    work: Vec<super::AssertionWork>,
    passed: bool,
    reached: Option<String>,
    failure: Option<super::AssertionFailure>,
}

impl FrameAssertionState {
    fn visit_retained_values(
        &self,
        visitor: &mut dyn for<'a> FnMut(crate::runtime::eval::RetainedValueRef<'a>),
    ) {
        match &self.failure {
            Some(super::AssertionFailure::Operands(left, right)) => {
                visit_retained_lowered_value(left, visitor);
                visit_retained_lowered_value(right, visitor);
            }
            None
            | Some(super::AssertionFailure::Reached(_))
            | Some(super::AssertionFailure::False) => {}
        }
    }
}

struct AssignPathState {
    slot: usize,
    path: AssignPathCursor,
    selectors: Vec<ResolvedAssignStep>,
    op: AssignOp,
    value: u32,
    singleton: bool,
    check: Option<LoweredTypeCheck>,
    span: Span,
    transfers_resource: bool,
}

#[derive(Clone, Copy)]
enum AssignPathCursor {
    DirectField(Name),
    Encoded {
        statement: u32,
        position: ListPosition<LoweredAssignStep>,
    },
    Exhausted,
}

struct FramePrintState {
    statement: u32,
    position: ListPosition<BuildExprId>,
    stderr: bool,
    flush: bool,
    propagate_result: bool,
    span: Span,
    line: String,
    argv: Vec<TraceArg>,
}

struct FrameEnvState {
    statement: u32,
    position: ListPosition<LoweredRunEnv>,
    overlay: BTreeMap<Vec<u8>, Vec<u8>>,
    body: u32,
    span: Span,
}

#[derive(Clone, Copy)]
enum ExprPatternIfStep {
    Condition,
    ExpressionValue,
}

type ExprPatternIfBranch = (BuildExprId, BuildExprId, Vec<usize>);
type ExprPatternIfPosition = ListPosition<ExprPatternIfBranch>;

type IfExprPosition = ListPosition<(BuildExprId, BuildExprId)>;

struct ProcessCommandRunState {
    args: ListPosition<LoweredRunArg>,
    env_assignments: ListPosition<LoweredRunEnv>,
}

#[derive(Clone, Copy)]
enum ProcessExecutionMode {
    Capture,
    Spawn,
    Pipeline { propagate: bool },
}

#[derive(Clone, Copy)]
enum ProcessExecutionPhase {
    Target,
    Arguments,
    Environment,
    Redirections,
    Timeout,
    CpuMax,
    Accept,
    Finish,
}

enum ProcessExecutionOperand {
    Target { mode: u32, span: Span },
    Argument { mode: u32, span: Span },
    Environment { name: Name, mode: u32, span: Span },
    Redirection {
        kind: RedirectionKind,
        mode: u32,
        target_span: Span,
        span: Span,
    },
    Timeout(Span),
    CpuMax(Span),
    Accept(Span),
}

struct ProcessInvocationBuilder {
    target: Option<Vec<u8>>,
    argv: Vec<Vec<u8>>,
    env_overlay: BTreeMap<Vec<u8>, Vec<u8>>,
    redirections: Vec<crate::runtime::process::ProcessRedirection>,
    timeout: Option<std::time::Duration>,
    cpu_max: Option<i64>,
    accepted_exit_codes: Option<crate::runtime::process::AcceptedExitCodes>,
}

impl ProcessInvocationBuilder {
    fn new() -> Self {
        Self {
            target: None,
            argv: Vec::new(),
            env_overlay: BTreeMap::new(),
            redirections: Vec::new(),
            timeout: None,
            cpu_max: None,
            accepted_exit_codes: None,
        }
    }

    fn finish(self, evaluator: &mut Evaluator) -> crate::runtime::process::ProcessInvocation {
        let mut env = evaluator.env.snapshot_clone();
        env.extend(self.env_overlay.clone());
        crate::runtime::process::ProcessInvocation {
            target: self.target.expect("run target is converted before invocation"),
            argv: self.argv,
            cwd: evaluator.cwd.clone(),
            env,
            env_overlay: self.env_overlay,
            redirections: self.redirections,
            timeout: self.timeout,
            cpu_max: self.cpu_max,
            accepted_exit_codes: self.accepted_exit_codes,
            namespaces: None,
        }
    }
}

struct ProcessExecutionState {
    instruction: u32,
    tag: FullTag,
    span: Span,
    mode: ProcessExecutionMode,
    phase: ProcessExecutionPhase,
    pending: Option<ProcessExecutionOperand>,
    segment_cursor: Option<ListPosition<LoweredRunPipelineSegment>>,
    segment_position: Option<ListPosition<LoweredRunPipelineSegment>>,
    args_position: Option<ListPosition<LoweredRunArg>>,
    env_position: Option<ListPosition<LoweredRunEnv>>,
    redirections_position: Option<ListPosition<LoweredRunRedirection>>,
    invocation: ProcessInvocationBuilder,
    invocations: Vec<crate::runtime::process::ProcessInvocation>,
    pipeline_head: Option<RunKind>,
}

struct ProcessRunOperands<'a> {
    kind: RunKind,
    target: RunArgView<'a>,
    args: ListView<'a, LoweredRunArg>,
    env: ListView<'a, LoweredRunEnv>,
    redirections: ListView<'a, LoweredRunRedirection>,
    timeout: Option<u32>,
    cpu_max: Option<u32>,
    accept: Option<u32>,
    span: Span,
}

fn run_arg_parts(value: RunArgView<'_>) -> (u32, u32, Span) {
    let span = value.span;
    let (mode, instruction) = match value.kind {
        RunArgKindView::Single(value) => (0, value.raw()),
        RunArgKindView::SingleOrSplice(value) => (1, value.raw()),
        RunArgKindView::Splice(value) => (2, value.raw()),
    };
    (mode, instruction, span)
}

fn process_run_operands<'e>(
    execution: &'e FullExecution<'_>,
    instruction: u32,
    tag: FullTag,
    segment: Option<ListPosition<LoweredRunPipelineSegment>>,
    span: Span,
) -> Result<ProcessRunOperands<'e>, RuntimeError> {
    let (decoded_tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
    let view = super::ExprView::decode(execution, decoded_tag, payload)
        .map_err(|error| indexed_error(error, span))?;
    match (tag, view) {
        (FullTag::ExprRunCapture, super::ExprView::RunCapture(value)) => {
            Ok(ProcessRunOperands {
                kind: value.kind,
                target: value.target,
                args: value.args,
                env: value.env,
                redirections: value.redirections,
                timeout: value.timeout.map(|id| id.raw()),
                cpu_max: value.cpu_max.map(|id| id.raw()),
                accept: value.accept.map(|id| id.raw()),
                span: value.span,
            })
        }
        (FullTag::ExprSpawnRun, super::ExprView::SpawnRun(value)) => {
            Ok(ProcessRunOperands {
                kind: RunKind::Plain,
                target: value.target,
                args: value.args,
                env: value.env,
                redirections: value.redirections,
                timeout: value.timeout.map(|id| id.raw()),
                cpu_max: value.cpu_max.map(|id| id.raw()),
                accept: value.accept.map(|id| id.raw()),
                span: value.span,
            })
        }
        (FullTag::ExprRunPipeline, super::ExprView::RunPipeline { segments, .. }) => {
            let Some(segment) = segment else {
                return Err(RuntimeError::new(
                    "indexed-ir",
                    "run pipeline segment cursor is missing",
                )
                .with_span(span));
            };
            let mut segments = segments;
            segments
                .resume(segment)
                .map_err(|error| indexed_error(error, span))?;
            let segment = segments
                .next()
                .transpose()
                .map_err(|error| indexed_error(error, span))?
                .ok_or_else(|| {
                    RuntimeError::new("indexed-ir", "run pipeline segment cursor ended early")
                        .with_span(span)
                })?;
            Ok(ProcessRunOperands {
                kind: segment.kind,
                target: segment.target,
                args: segment.args,
                env: segment.env,
                redirections: segment.redirections,
                timeout: segment.timeout.map(|id| id.raw()),
                cpu_max: segment.cpu_max.map(|id| id.raw()),
                accept: segment.accept.map(|id| id.raw()),
                span,
            })
        }
        _ => unreachable!("process execution tag has its typed run view"),
    }
}

struct ProcessCommandBuilderState {
    call_span: Span,
    span: Span,
    plan: Option<CommandPlan>,
    cwd: Option<crate::runtime::value::PathValue>,
    env: BTreeMap<String, Vec<u8>>,
    stdin: Option<LoweredValue>,
    stdout: Option<LoweredValue>,
    stderr: Option<LoweredValue>,
    stdout_append: bool,
    stderr_append: bool,
    timeout: Option<crate::runtime::value::DurationValue>,
    cpu_max: Option<i64>,
    accepted_exit_codes: Option<crate::runtime::process::AcceptedExitCodes>,
    detach: Option<bool>,
    new_session: Option<bool>,
    ignore_hup: Option<bool>,
    same_group: Option<bool>,
    run: Option<ProcessCommandRunState>,
}

impl ProcessCommandBuilderState {
    fn new(call_span: Span, span: Span) -> Self {
        Self {
            call_span,
            span,
            plan: None,
            cwd: None,
            env: BTreeMap::new(),
            stdin: None,
            stdout: None,
            stderr: None,
            stdout_append: false,
            stderr_append: false,
            timeout: None,
            cpu_max: None,
            accepted_exit_codes: None,
            detach: None,
            new_session: None,
            ignore_hup: None,
            same_group: None,
            run: None,
        }
    }

    fn visit_retained_values(
        &self,
        visitor: &mut dyn for<'a> FnMut(crate::runtime::eval::RetainedValueRef<'a>),
    ) {
        for value in [&self.stdin, &self.stdout, &self.stderr].into_iter().flatten() {
            visit_retained_lowered_value(value, visitor);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProcessCommandRunPhase {
    Target,
    Argument,
    Environment,
    Timeout,
    CpuMax,
    Accept,
}

#[derive(Clone, Copy)]
enum ProcessCommandBuilderPhase {
    Field,
    Run(ProcessCommandRunPhase),
}

fn process_command_builder_view<'e>(
    execution: &'e FullExecution<'_>,
    instruction: u32,
    span: Span,
) -> Result<(ListView<'e, LoweredProcessCommandBuilderEntry>, Span), RuntimeError> {
    let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
    match super::ExprView::decode(execution, tag, payload)
        .map_err(|error| indexed_error(error, span))?
    {
        super::ExprView::ProcessCommandBuilder { entries, span } => Ok((entries, span)),
        _ => unreachable!("process command builder has a matching typed view"),
    }
}

fn process_command_builder_entry<'e>(
    execution: &'e FullExecution<'_>,
    instruction: u32,
    position: ListPosition<LoweredProcessCommandBuilderEntry>,
    span: Span,
) -> Result<(
    Option<ProcessCommandBuilderEntryView<'e>>,
    ListPosition<LoweredProcessCommandBuilderEntry>,
    Span,
), RuntimeError> {
    let (mut entries, builder_span) = process_command_builder_view(execution, instruction, span)?;
    entries
        .resume(position)
        .map_err(|error| indexed_error(error, builder_span))?;
    let entry = entries
        .next()
        .transpose()
        .map_err(|error| indexed_error(error, builder_span))?;
    Ok((entry, entries.position(), builder_span))
}

fn run_arg_from_view(value: RunArgView<'_>) -> RunArg {
    let span = value.span;
    let (mode, value) = match value.kind {
        RunArgKindView::Single(value) => (0, value.raw()),
        RunArgKindView::SingleOrSplice(value) => (1, value.raw()),
        RunArgKindView::Splice(value) => (2, value.raw()),
    };
    RunArg {
        mode,
        value,
        span,
    }
}

enum FrameContinuation {
    WithBinding {
        instruction: u32,
        position: StatementWithBindingPosition,
        slot: usize,
        body: u32,
        else_param_slot: Option<usize>,
        else_body: u32,
        span: Span,
    },
    GuardInput {
        target: LoweredCompTarget,
        else_param_slot: Option<usize>,
        else_body: u32,
        span: Span,
        /// The subject is an optional, so `null` fails and no `Result` is unwrapped.
        optional: bool,
    },
    Store(usize),
    StoreTarget {
        target: LoweredCompTarget,
        span: Span,
    },
    ParameterDefault {
        slot: usize,
        kind: LoweredType,
        check: Option<LoweredTypeCheck>,
        span: Span,
    },
    Assign {
        slot: usize,
        op: AssignOp,
        singleton: bool,
        check: Option<LoweredTypeCheck>,
        span: Span,
        transfers_resource: bool,
    },
    AssignSelector(AssignPathState),
    AssignPath(AssignPathState),
    Return { transfers_resource: bool },
    BlockValue { transfers_resource: bool },
    ErrorContextEntry {
        body: u32,
        span: Span,
        next: Box<FrameContinuation>,
    },
    ContextScopeEntry {
        kind: crate::syntax::arena::ContextScopeKind,
        body: u32,
        span: Span,
        next: Box<FrameContinuation>,
    },
    CleanupAction,
    StatementScopeEnd,
    EnvValue(FrameEnvState),
    PrintArgument(FramePrintState),
    Discard(Span),
    ComparisonLeft {
        instruction: u32,
        pairs: ComparisonChainPosition,
        next: Box<FrameContinuation>,
    },
    ComparisonRight {
        instruction: u32,
        pairs: ComparisonChainPosition,
        op: BinaryOp,
        left: LoweredValue,
        span: Span,
        next: Box<FrameContinuation>,
    },
    Field {
        name: String,
        span: Span,
        next: Box<FrameContinuation>,
    },
    IndexBase {
        instruction: u32,
        span: Span,
        next: Box<FrameContinuation>,
    },
    IndexValue {
        base: LoweredValue,
        span: Span,
        next: Box<FrameContinuation>,
    },
    IndexFromEnd {
        distance: usize,
        span: Span,
        next: Box<FrameContinuation>,
    },
    ModuleArguments {
        op: super::RuntimeOp,
        call_plan: Option<Arc<crate::runtime::eval::call_plan::ModuleCallPlan>>,
        instruction: u32,
        position: ModuleArgumentPosition,
        values: Vec<Option<LoweredValue>>,
        span: Span,
        next: Box<FrameContinuation>,
    },
    UtilsCacheResult {
        key: String,
        span: Span,
        next: Box<FrameContinuation>,
    },
    BinaryLeft {
        op: BinaryOp,
        right: u32,
        span: Span,
        next: Box<FrameContinuation>,
    },
    BinaryRight {
        op: BinaryOp,
        left: LoweredValue,
        span: Span,
        next: Box<FrameContinuation>,
    },
    BoolBinaryRight {
        next: Box<FrameContinuation>,
        span: Span,
    },
    ApplyOperands {
        instruction: u32,
        tag: FullTag,
        position: usize,
        tag_position: Option<ListPosition<BuildExprId>>,
        error_field_position: Option<ListPosition<(Arc<str>, BuildExprId)>>,
        values: Vec<super::FrameApplyValue>,
        span: Span,
        next: Box<FrameContinuation>,
    },
    ProcessCommandBuilder {
        instruction: u32,
        entry: ListPosition<LoweredProcessCommandBuilderEntry>,
        phase: ProcessCommandBuilderPhase,
        state: Box<ProcessCommandBuilderState>,
        next: Box<FrameContinuation>,
    },
    ProcessRun {
        state: Box<ProcessExecutionState>,
        next: Box<FrameContinuation>,
    },
    SpawnCommand {
        span: Span,
        next: Box<FrameContinuation>,
    },
    Wait {
        span: Span,
        next: Box<FrameContinuation>,
    },
    StatementRun {
        propagate_result: bool,
        call_span: Span,
    },
    TypedBoolNot {
        span: Span,
        next: Box<FrameContinuation>,
    },
    TypedIntByteAtIndex {
        slot: usize,
        default: Option<u32>,
        span: Span,
        next: Box<FrameContinuation>,
    },
    TypedIntByteAtFallback {
        span: Span,
        next: Box<FrameContinuation>,
    },
    AssertionValue(FrameAssertionState),
    AssertionComparisonLeft {
        state: FrameAssertionState,
        instruction: u32,
        op: BinaryOp,
        right: u32,
        span: Span,
    },
    AssertionComparisonRight {
        state: FrameAssertionState,
        instruction: u32,
        op: BinaryOp,
        left: LoweredValue,
        span: Span,
    },
    AssertionChainLeft {
        state: Box<FrameAssertionState>,
        instruction: u32,
        pairs: ComparisonChainPosition,
    },
    AssertionChainRight {
        state: Box<FrameAssertionState>,
        instruction: u32,
        pairs: ComparisonChainPosition,
        op: BinaryOp,
        left: LoweredValue,
        span: Span,
    },
    AssertionMessage {
        failure: super::AssertionFailure,
        span: Span,
    },
    IfExpr {
        instruction: u32,
        position: IfExprPosition,
        current_body: u32,
        else_value: u32,
        span: Span,
        next: Box<FrameContinuation>,
    },
    StatementIf {
        instruction: u32,
        position: StatementIfPosition,
        body: u32,
        else_body: Option<u32>,
        span: Span,
    },
    StatementStrTagMatch {
        instruction: u32,
        span: Span,
    },
    PatternIf {
        instruction: u32,
        position: StatementPatternIfPosition,
        body: u32,
        else_body: Option<u32>,
        span: Span,
    },
    /// Reborrow branch descriptors from the original instruction as evaluation advances.
    ExprPatternIf {
        instruction: u32,
        position: ExprPatternIfPosition,
        span: Span,
        next: Box<FrameContinuation>,
        step: ExprPatternIfStep,
    },
    StrTagMatch {
        instruction: u32,
        span: Span,
        next: Box<FrameContinuation>,
    },
    PatternWhile {
        body: u32,
        span: Span,
    },
    ForItems {
        target: LoweredCompTarget,
        body: u32,
        span: Span,
    },
    ForPipelineInput {
        slot: usize,
        body: u32,
        span: Span,
        stages: smallvec::SmallVec<[IndexedLiveSerialStage; 4]>,
    },
    StageValueInput {
        span: Span,
        stages: smallvec::SmallVec<[IndexedLiveSerialStage; 4]>,
        next: Box<FrameContinuation>,
    },
    StageResult,
    ForStrLines {
        slot: usize,
        body: u32,
        span: Span,
    },
    While {
        condition: u32,
        body: u32,
        span: Span,
    },
    MatchValue {
        instruction: u32,
        position: StatementMatchPosition,
        span: Span,
    },
    MatchGuard {
        instruction: u32,
        position: StatementMatchPosition,
        body: u32,
        value: LoweredValue,
        span: Span,
    },
    MatchExprValue {
        instruction: u32,
        arms: MatchExprPosition,
        span: Span,
        next: Box<FrameContinuation>,
    },
    MatchExprGuard {
        instruction: u32,
        arms: MatchExprPosition,
        value: LoweredValue,
        body: u32,
        span: Span,
        next: Box<FrameContinuation>,
    },
    BreakLoop { transfers_resource: bool },
    RetryDelay(Box<RetryFrameState>, Box<FrameContinuation>),
    /// A `yield` statement's value: the frame suspends here and hands the value
    /// to whoever pulled the producer.
    Yield { transfers_resource: bool },
    YieldDelegate {
        span: Span,
        transfers_resource: bool,
    },
    DynamicCallee {
        instruction: u32,
        position: CallArgumentPosition,
        values: Vec<LoweredValue>,
        /// The kind a typed call's callee must have; `None` for a dynamic call.
        typed_pure: Option<bool>,
        span: Span,
        next: Box<FrameContinuation>,
    },
    DynamicArguments {
        callee: LoweredValue,
        instruction: u32,
        position: CallArgumentPosition,
        mode: CallArgumentMode,
        values: Vec<LoweredValue>,
        span: Span,
        next: Box<FrameContinuation>,
    },
    CallArguments {
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        external: bool,
        instruction: u32,
        position: CallArgumentPosition,
        mode: CallArgumentMode,
        values: Vec<LoweredValue>,
        span: Span,
        next: Box<FrameContinuation>,
    },
    WrapOk(Box<FrameContinuation>),
    ProcCallResult(Box<FrameContinuation>),
    WrapErr {
        cause: Option<u32>,
        span: Span,
        next: Box<FrameContinuation>,
    },
    AttachErrCause {
        error: crate::runtime::value::Value,
        span: Span,
        next: Box<FrameContinuation>,
    },
    Try {
        span: Span,
        next: Box<FrameContinuation>,
    },
    CheckedValue {
        check: LoweredTypeCheck,
        span: Span,
        next: Box<FrameContinuation>,
    },
    Require {
        check: LoweredTypeCheck,
        span: Span,
        next: Box<FrameContinuation>,
    },
    MethodReceiver {
        instruction: u32,
        position: MethodArgumentPosition,
        name: Arc<str>,
        span: Span,
        // The slot this call's result overwrites, when the call is the value of
        // a plain assignment to the same slot the receiver is read from. The
        // call may then take the value out of that slot instead of copying it.
        consume: Option<usize>,
        next: Box<FrameContinuation>,
    },
    MethodArg {
        instruction: u32,
        position: MethodArgumentPosition,
        name: Arc<str>,
        receiver: LoweredValue,
        consume: Option<usize>,
        values: Vec<LoweredValue>,
        span: Span,
        next: Box<FrameContinuation>,
    },
    FmtValue {
        state: FmtState,
        span: Span,
        spec: Option<FormatSpec>,
        next: Box<FrameContinuation>,
    },
    ResultFallback {
        right: u32,
        span: Span,
        next: Box<FrameContinuation>,
    },
    ListItems {
        state: ListItemsState,
        next: Box<FrameContinuation>,
    },
    MapLiteralItems {
        state: MapLiteralState,
        next: Box<FrameContinuation>,
    },
    RecordUpdateBase {
        instruction: u32,
        position: RecordUpdatePosition,
        span: Span,
        next: Box<FrameContinuation>,
    },
    RecordUpdateItems {
        instruction: u32,
        position: RecordUpdatePosition,
        path: Vec<Name>,
        field_span: Span,
        base: LoweredValue,
        values: Vec<(Vec<Name>, LoweredValue, Span)>,
        span: Span,
        next: Box<FrameContinuation>,
    },
    RecordItems {
        state: RecordItemsState,
        next: Box<FrameContinuation>,
    },
    ListCompIter {
        state: Box<ListCompState>,
        active: CompIteratorState,
        next: Box<FrameContinuation>,
    },
    ListCompCondition {
        state: Box<ListCompState>,
        span: Span,
        next: Box<FrameContinuation>,
    },
    ListCompKey {
        state: Box<ListCompState>,
        next: Box<FrameContinuation>,
    },
    ListCompValue {
        state: Box<ListCompState>,
        key: Option<MapKey>,
        next: Box<FrameContinuation>,
    },
}

impl FrameContinuation {
    fn visit_retained_values(
        &self,
        visitor: &mut dyn for<'a> FnMut(crate::runtime::eval::RetainedValueRef<'a>),
    ) {
        use FrameContinuation as C;
        match self {
            C::WithBinding { .. }
            | C::GuardInput { .. }
            | C::Store(_)
            | C::StoreTarget { .. }
            | C::ParameterDefault { .. }
            | C::Assign { .. }
            | C::AssignSelector(_)
            | C::AssignPath(_)
            | C::Return { .. }
            | C::BlockValue { .. }
            | C::Discard(_)
            | C::StatementIf { .. }
            | C::StatementStrTagMatch { .. }
            | C::PatternIf { .. }
            | C::PatternWhile { .. }
            | C::ForItems { .. }
            | C::ForPipelineInput { .. }
            | C::StageValueInput { .. }
            | C::StageResult
            | C::ForStrLines { .. }
            | C::While { .. }
            | C::MatchValue { .. }
            | C::BreakLoop { .. }
            | C::Yield { .. }
            | C::YieldDelegate { .. }
    | C::CleanupAction
            | C::StatementScopeEnd
            | C::EnvValue(_)
            | C::PrintArgument(_)
            | C::StatementRun { .. } => {}
            C::ExprPatternIf { next, .. } => next.visit_retained_values(visitor),
            C::RetryDelay(state, next) => {
                state.visit_retained_values(visitor);
                next.visit_retained_values(visitor);
            }
            C::TypedBoolNot { next, .. }
            | C::TypedIntByteAtIndex { next, .. }
            | C::TypedIntByteAtFallback { next, .. } => next.visit_retained_values(visitor),
            C::AssertionValue(state) | C::AssertionComparisonLeft { state, .. } => {
                state.visit_retained_values(visitor);
            }
            C::AssertionChainLeft { state, .. } => state.visit_retained_values(visitor),
            C::AssertionComparisonRight { state, left, .. } => {
                state.visit_retained_values(visitor);
                visit_retained_lowered_value(left, visitor);
            }
            C::AssertionChainRight { state, left, .. } => {
                state.visit_retained_values(visitor);
                visit_retained_lowered_value(left, visitor);
            }
            C::AssertionMessage { failure, .. } => match failure {
                super::AssertionFailure::Operands(left, right) => {
                    visit_retained_lowered_value(left, visitor);
                    visit_retained_lowered_value(right, visitor);
                }
                super::AssertionFailure::Reached(_) | super::AssertionFailure::False => {}
            },
            C::ErrorContextEntry { next, .. }
            | C::ContextScopeEntry { next, .. }
            | C::ComparisonLeft { next, .. }
            | C::Field { next, .. }
            | C::IndexBase { next, .. }
            | C::IndexFromEnd { next, .. }
            | C::BinaryLeft { next, .. }
            | C::BoolBinaryRight { next, .. }
            | C::IfExpr { next, .. }
            | C::MatchExprValue { next, .. }
            | C::StrTagMatch { next, .. }
            | C::DynamicCallee { next, .. }
            | C::WrapOk(next)
            | C::ProcCallResult(next)
            | C::WrapErr { next, .. }
            | C::Try { next, .. }
            | C::CheckedValue { next, .. }
            | C::Require { next, .. }
            | C::MethodReceiver { next, .. }
            | C::FmtValue { next, .. }
            | C::ResultFallback { next, .. }
            | C::RecordUpdateBase { next, .. }
            | C::UtilsCacheResult { next, .. } => next.visit_retained_values(visitor),
            C::ApplyOperands { values, next, .. } => {
                for value in values {
                    if let super::FrameApplyValue::Value(value) = value {
                        visit_retained_lowered_value(value, visitor);
                    }
                }
                next.visit_retained_values(visitor);
            }
            C::ProcessCommandBuilder { state, next, .. } => {
                state.visit_retained_values(visitor);
                next.visit_retained_values(visitor);
            }
            C::ProcessRun { next, .. }
            | C::SpawnCommand { next, .. }
            | C::Wait { next, .. } => next.visit_retained_values(visitor),
            C::ComparisonRight { left, next, .. } | C::BinaryRight { left, next, .. } => {
                visit_retained_lowered_value(left, visitor);
                next.visit_retained_values(visitor);
            }
            C::IndexValue { base, next, .. } => {
                visit_retained_lowered_value(base, visitor);
                next.visit_retained_values(visitor);
            }
            C::ModuleArguments { values, next, .. } => {
                visit_retained_lowered_values(values.iter().flatten(), visitor);
                next.visit_retained_values(visitor);
            }
            C::MatchGuard { value, .. } => visit_retained_lowered_value(value, visitor),
            C::MatchExprGuard { value, next, .. } => {
                visit_retained_lowered_value(value, visitor);
                next.visit_retained_values(visitor);
            }
            C::DynamicArguments { callee, values, next, .. } => {
                visit_retained_lowered_value(callee, visitor);
                visit_retained_lowered_values(values, visitor);
                next.visit_retained_values(visitor);
            }
            C::CallArguments { values, next, .. } => {
                visit_retained_lowered_values(values, visitor);
                next.visit_retained_values(visitor);
            }
            C::AttachErrCause { error, next, .. } => {
                visit_retained_runtime(error, visitor);
                next.visit_retained_values(visitor);
            }
            C::MethodArg { receiver, values, next, .. } => {
                visit_retained_lowered_value(receiver, visitor);
                visit_retained_lowered_values(values, visitor);
                next.visit_retained_values(visitor);
            }
            C::ListItems { state, next } => {
                visit_retained_lowered_values(&state.values, visitor);
                next.visit_retained_values(visitor);
            }
            C::MapLiteralItems { state, next } => {
                visit_retained_lowered_values(state.fields.values(), visitor);
                next.visit_retained_values(visitor);
            }
            C::RecordUpdateItems { base, values, next, .. } => {
                visit_retained_lowered_value(base, visitor);
                visit_retained_lowered_values(values.iter().map(|(_, value, _)| value), visitor);
                next.visit_retained_values(visitor);
            }
            C::RecordItems { state, next } => {
                visit_retained_lowered_values(
                    state.fields.iter().map(|(_, value)| value),
                    visitor,
                );
                next.visit_retained_values(visitor);
            }
            C::ListCompIter { state, next, .. }
            | C::ListCompCondition { state, next, .. }
            | C::ListCompKey { state, next }
            | C::ListCompValue { state, next, .. } => {
                state.visit_retained_values(visitor);
                next.visit_retained_values(visitor);
            }
        }
    }
}

// Propagated initializer errors stop compound evaluation at the existing With
// handler. Explicit returns and loop transfers retain their separate targets.
fn with_initializer_handler(
    mut continuation: &FrameContinuation,
) -> Option<(Option<usize>, u32, Span)> {
    loop {
        continuation = match continuation {
            FrameContinuation::WithBinding {
                else_param_slot,
                else_body,
                span,
                ..
            } => return Some((*else_param_slot, *else_body, *span)),
            FrameContinuation::Field { next, .. }
            | FrameContinuation::IndexBase { next, .. }
            | FrameContinuation::IndexValue { next, .. }
            | FrameContinuation::IndexFromEnd { next, .. }
            | FrameContinuation::ModuleArguments { next, .. }
            | FrameContinuation::UtilsCacheResult { next, .. }
            | FrameContinuation::ComparisonLeft { next, .. }
            | FrameContinuation::ComparisonRight { next, .. }
            | FrameContinuation::BinaryLeft { next, .. }
            | FrameContinuation::BinaryRight { next, .. }
            | FrameContinuation::BoolBinaryRight { next, .. }
            | FrameContinuation::ApplyOperands { next, .. }
            | FrameContinuation::ProcessCommandBuilder { next, .. }
            | FrameContinuation::ProcessRun { next, .. }
            | FrameContinuation::SpawnCommand { next, .. }
            | FrameContinuation::Wait { next, .. }
            | FrameContinuation::TypedBoolNot { next, .. }
            | FrameContinuation::TypedIntByteAtIndex { next, .. }
            | FrameContinuation::TypedIntByteAtFallback { next, .. }
            | FrameContinuation::IfExpr { next, .. }
            | FrameContinuation::MatchExprValue { next, .. }
            | FrameContinuation::StrTagMatch { next, .. }
            | FrameContinuation::MatchExprGuard { next, .. }
            | FrameContinuation::CallArguments { next, .. }
            | FrameContinuation::DynamicCallee { next, .. }
            | FrameContinuation::DynamicArguments { next, .. }
            | FrameContinuation::Try { next, .. }
            | FrameContinuation::Require { next, .. }
            | FrameContinuation::CheckedValue { next, .. }
            | FrameContinuation::MethodReceiver { next, .. }
            | FrameContinuation::MethodArg { next, .. }
            | FrameContinuation::FmtValue { next, .. }
            | FrameContinuation::ResultFallback { next, .. }
            | FrameContinuation::ListItems { next, .. }
            | FrameContinuation::MapLiteralItems { next, .. }
            | FrameContinuation::RecordItems { next, .. }
            | FrameContinuation::ListCompIter { next, .. }
            | FrameContinuation::ListCompCondition { next, .. }
            | FrameContinuation::ListCompKey { next, .. }
            | FrameContinuation::ListCompValue { next, .. }
            | FrameContinuation::RetryDelay(_, next)
            | FrameContinuation::WrapErr { next, .. }
            | FrameContinuation::AttachErrCause { next, .. }
            | FrameContinuation::WrapOk(next)
            | FrameContinuation::ProcCallResult(next) => next,
            FrameContinuation::ExprPatternIf { next, .. } => next,
            _ => return None,
        };
    }
}

// Expression boundaries own completion and failure routing within one lexical
// call frame, so producers can suspend without creating recursive evaluators.
enum ExpressionBoundaryPolicy {
    Value,
    Context(crate::runtime::value::ErrorContext),
    Capture,
    Retry(Box<RetryFrameState>),
    Scope(super::ContextScopeRestore),
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CleanupFailureResources {
    Release,
    Retain,
    RejectContextEscape,
}

enum ScanLinesSource {
    Text(Arc<str>),
    Bytes(Arc<[u8]>),
}

impl ScanLinesSource {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Text(text) => text.as_bytes(),
            Self::Bytes(bytes) => bytes,
        }
    }
}

/// Keeps cleanup-only evaluator state balanced while a deferred expression suspends.
struct CleanupActionGuard {
    shielded_deadlines: Vec<crate::runtime::eval::WithinDeadline>,
    previous_cleanup_contexts: Vec<crate::runtime::value::ErrorContext>,
}

enum CleanupGroupResume {
    Call(StmtFlow),
    ErrorCall,
    Block {
        scope_id: u64,
        disposition: CleanupFailureResources,
        during_discard: bool,
        include_work_contexts: bool,
    },
}

/// Retains one lexical cleanup sequence on its owner frame until every action ends.
/// The primary exit and the first cleanup failure stay separate so later `errdefer`
/// actions and secondary-error reporting use the scope's current exit state.
struct CleanupGroup {
    defers: Vec<RegisteredDefer>,
    next_defer: usize,
    leaves_with_error: bool,
    first_error: Option<RuntimeError>,
    first_traceback: Option<Traceback>,
    primary_error: Option<RuntimeError>,
    primary_traceback: Option<Traceback>,
    released: Option<RuntimeError>,
    cleanup_contexts: Vec<crate::runtime::value::ErrorContext>,
    call_span: Span,
    resume: CleanupGroupResume,
}

enum DiscardResume {
    Continue,
    ExpressionValue {
        boundary: usize,
        value: LoweredValue,
    },
    CapturedPropagation {
        boundary: usize,
        flow: StmtFlow,
        contexts: Vec<crate::runtime::value::ErrorContext>,
    },
    CleanupFlow {
        boundary: usize,
        flow: StmtFlow,
        contexts: Vec<crate::runtime::value::ErrorContext>,
    },
    CompleteCall {
        flow: StmtFlow,
        contexts: Vec<crate::runtime::value::ErrorContext>,
    },
    UnwindError { boundary: Option<usize> },
}

/// Owns an in-flight exit while lexical work is discarded and block cleanup suspends.
struct DiscardState {
    keep: usize,
    next_to_discard: usize,
    primary_failed: bool,
    primary_is_error: bool,
    context_scopes: usize,
    primary_error: Option<RuntimeError>,
    primary_traceback: Option<Traceback>,
    first_error: Option<RuntimeError>,
    first_traceback: Option<Traceback>,
    previous_cleanup_contexts: Vec<crate::runtime::value::ErrorContext>,
    resume: DiscardResume,
}

fn visit_retained_flow(
    flow: &StmtFlow,
    visitor: &mut dyn for<'a> FnMut(crate::runtime::eval::RetainedValueRef<'a>),
) {
    match flow {
        StmtFlow::Value(value) | StmtFlow::Return(value) | StmtFlow::Propagate(value) => {
            visit_retained_lowered_value(value, visitor);
        }
        StmtFlow::Break(Some(value)) => visit_retained_lowered_value(value, visitor),
        StmtFlow::None | StmtFlow::Break(None) | StmtFlow::Continue => {}
    }
}

impl CleanupGroup {
    fn visit_retained_values(
        &self,
        visitor: &mut dyn for<'a> FnMut(crate::runtime::eval::RetainedValueRef<'a>),
    ) {
        if let Some(error) = &self.first_error {
            visit_retained_runtime_error(error, visitor);
        }
        if let Some(error) = &self.primary_error {
            visit_retained_runtime_error(error, visitor);
        }
        if let Some(error) = &self.released {
            visit_retained_runtime_error(error, visitor);
        }
        if let CleanupGroupResume::Call(flow) = &self.resume {
            visit_retained_flow(flow, visitor);
        }
    }
}

impl DiscardState {
    fn visit_retained_values(
        &self,
        visitor: &mut dyn for<'a> FnMut(crate::runtime::eval::RetainedValueRef<'a>),
    ) {
        if let Some(error) = &self.primary_error {
            visit_retained_runtime_error(error, visitor);
        }
        if let Some(error) = &self.first_error {
            visit_retained_runtime_error(error, visitor);
        }
        match &self.resume {
            DiscardResume::ExpressionValue { value, .. } => {
                visit_retained_lowered_value(value, visitor);
            }
            DiscardResume::CapturedPropagation { flow, .. }
            | DiscardResume::CleanupFlow { flow, .. }
            | DiscardResume::CompleteCall { flow, .. } => visit_retained_flow(flow, visitor),
            DiscardResume::Continue | DiscardResume::UnwindError { .. } => {}
        }
    }
}

enum FrameWork {
    ClearSlots(Vec<usize>),
    GuardFailureEnd(Span),
    CleanupGroup(Box<CleanupGroup>),
    CleanupBoundary(CleanupActionGuard),
    Discard(Box<DiscardState>),
    // A lexical expression retains its destination across producer suspension.
    // Its statement scope owns defers; this boundary consumes only the body value.
    ExpressionBoundary {
        policy: ExpressionBoundaryPolicy,
        next: FrameContinuation,
    },
    CompCleanup(CompStreams),
    Statements {
        cursor: StatementCursor,
        complete_call: bool,
        scope_id: Option<u64>,
    },
    Expr {
        instruction: u32,
        span: Span,
        next: FrameContinuation,
    },
    Assertion(FrameAssertionState),
    Value {
        value: FrameValue,
        next: FrameContinuation,
    },
    ForScalars {
        target: LoweredCompTarget,
        cursor: LoweredScalarCursor,
        body: u32,
        span: Span,
    },
    ForMap {
        target: LoweredCompTarget,
        cursor: LoweredMapCursor,
        body: u32,
        span: Span,
    },
    ForItems {
        target: LoweredCompTarget,
        items: Vec<LoweredValue>,
        index: usize,
        body: u32,
        span: Span,
    },
    /// A loop over a script producer: each step pulls one item and re-arms
    /// itself, so the loop never holds the whole stream.
    ForStream {
        target: LoweredCompTarget,
        stream: StreamValue,
        body: u32,
        span: Span,
    },
    ForPipeline {
        slot: usize,
        pipeline: Box<IndexedSerialPipeline>,
        body: u32,
        span: Span,
    },
    StageValue {
        pipeline: Box<IndexedStageEngine>,
        span: Span,
        next: FrameContinuation,
    },
    ForStrLines {
        slot: usize,
        text: LoweredValue,
        cursor: usize,
        line_count: u32,
        body: u32,
        span: Span,
    },
    While {
        condition: u32,
        body: u32,
        span: Span,
    },
    Loop {
        body: u32,
        span: Span,
        completion: Option<Box<FrameContinuation>>,
    },
    PatternWhile {
        condition: u32,
        body: u32,
        captures: Vec<usize>,
        span: Span,
    },
    Finish(StmtFlow),
    FinishError,
}

impl FrameWork {
    fn is_context_scope(&self) -> bool {
        matches!(
            self,
            Self::ExpressionBoundary {
                policy: ExpressionBoundaryPolicy::Scope(_),
                ..
            }
        )
    }
}

#[derive(Clone, Copy)]
pub(super) enum FrameOwner {
    Function(LoweredFunctionKey, LoweredFunctionKind),
    Block {
        owns_scope: bool,
        outer_scope: u64,
    },
}

/// A function frame owns its slots. A block frame borrows the recursive
/// evaluator's, so their contents and address never move, and state keyed by
/// that address (root publication, context-scope locals) stays valid.
pub(super) enum FrameSlots<'p> {
    Owned(Vec<LoweredValue>),
    Lent(&'p mut [LoweredValue]),
}

impl std::ops::Deref for FrameSlots<'_> {
    type Target = [LoweredValue];

    fn deref(&self) -> &[LoweredValue] {
        match self {
            Self::Owned(slots) => slots,
            Self::Lent(slots) => slots,
        }
    }
}

impl std::ops::DerefMut for FrameSlots<'_> {
    fn deref_mut(&mut self) -> &mut [LoweredValue] {
        match self {
            Self::Owned(slots) => slots,
            Self::Lent(slots) => slots,
        }
    }
}

/// How many function calls may be open at once.
///
/// Call frames live on the heap, so recursion that never ends would
/// otherwise run until memory does. An open call of a small function holds
/// about 2.4 KiB, so the limit bounds a runaway recursion at roughly a
/// quarter of a gibibyte, and reaching it takes about a second in an
/// unoptimized build. It is five times the deepest recursion over data the
/// small-stack tests make (20,000 calls); raising it costs memory in
/// proportion and nothing else, because a call's cost does not depend on
/// how many are open.
const MAX_CALL_DEPTH: usize = 100_000;

// Each nested machine adds native stack frames; ordinary script calls stay
// on the machine's heap stack and retain their separate call-depth limit.
const MAX_FRAME_MACHINE_DEPTH: usize = 32;

// Owning the counter leaves the evaluator available to the machine and
// restores its depth even when execution fails or unwinds a panic.
pub(super) struct FrameMachineGuard(Arc<AtomicUsize>);

impl Drop for FrameMachineGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// How many of the open calls a `stack-overflow` error names before the call
/// that was refused.
const STACK_OVERFLOW_PATH_CALLS: usize = 5;

pub(super) struct CallFrame<'p> {
    pub(super) owner: FrameOwner,
    /// Whether this frame is a stream producer, whose body ends by falling off
    /// the end of its statements rather than by returning.
    pub(super) producer: bool,
    /// Whether the body was discarded for cancellation. A cancelled frame
    /// replays only cleanup: retained expression boundaries restore their
    /// context scopes and deliver no value to the continuations, which are
    /// body computation rather than cleanup.
    cancelled: bool,
    pub(super) scope_id: u64,
    pub(super) execution: FullExecution<'p>,
    /// Keeps a cross-program frame's interned names current through cleanup.
    _symbol_owner: Option<crate::symbol::SymbolOwnerGuard>,
    pub(super) slots: FrameSlots<'p>,
    pub(super) slot_scopes: Vec<u64>,
    pub(super) call_span: Span,
    pub(super) definition_span: Span,
    work: Vec<FrameWork>,
    pub(super) defers: Vec<RegisteredDefer>,
    pub(super) block_scopes: Vec<u64>,
    block_defer_offsets: Vec<usize>,
    return_to: Option<FrameContinuation>,
}

enum ProducerSuspension {
    Yielded(LoweredValue),
    Delegated { value: LoweredValue, span: Span },
}

pub(super) struct ExplicitFrames<'a, 'p> {
    evaluator: &'a mut Evaluator,
    program: &'p FullProgram,
    /// Inline room for one frame: a block run for the recursive evaluator
    /// usually never calls, so it does not allocate a call stack.
    calls: smallvec::SmallVec<[CallFrame<'p>; 1]>,
    result: Option<Result<LoweredValue, RuntimeError>>,
    pending_error: Option<RuntimeError>,
    /// The yielded item or delegation source, with the remaining work saved
    /// on the frame's stack until its consumer requests another item.
    suspended: Option<ProducerSuspension>,
    /// A finished block frame's statement flow.
    block_flow: Option<StmtFlow>,
    /// An upper bound on the `ctx` boundaries open in `calls`, and zero when
    /// there are none. Cleanup collects the open contexts each time a frame
    /// discards work, which every `return` does; without this bound it would
    /// read the work of every open call to find them, so a call would cost
    /// time in proportion to the calls already open. Counted where a
    /// boundary enters a frame, lowered where one is seen to leave, and
    /// corrected by any collection that finds fewer.
    open_context_boundaries: usize,
}

struct ResolvedCall<'p> {
    program: ProgramOwner<'p>,
    function: LoweredFunctionKey,
    kind: LoweredFunctionKind,
    index: usize,
}

/// How many `ctx` boundaries a frame's work holds.
fn context_boundaries(work: &[FrameWork]) -> usize {
    work.iter()
        .filter(|work| {
            matches!(
                work,
                FrameWork::ExpressionBoundary {
                    policy: ExpressionBoundaryPolicy::Context(_),
                    ..
                }
            )
        })
        .count()
}

impl Evaluator {
    pub(super) fn enter_frame_machine(&self, span: Span) -> Result<FrameMachineGuard, RuntimeError> {
        if self.frame_machine_depth.fetch_add(1, Ordering::Relaxed) >= MAX_FRAME_MACHINE_DEPTH {
            self.frame_machine_depth.fetch_sub(1, Ordering::Relaxed);
            return Err(RuntimeError::new(
                "stack-overflow",
                format!("more than {MAX_FRAME_MACHINE_DEPTH} frame machines are open"),
            )
            .with_span(span));
        }
        Ok(FrameMachineGuard(Arc::clone(&self.frame_machine_depth)))
    }

    pub(super) fn indexed_frames_supported(
        &self,
        view: FullFunctionView<'_>,
        span: Span,
    ) -> Result<bool, RuntimeError> {
        let header = view.header().map_err(|error| indexed_error(error, span))?;
        Ok(!matches!(
            header.return_kind,
            LoweredReturnKind::Plain(LoweredType::Stream)
        ))
    }

    pub(super) fn eval_indexed_with_frames(
        &mut self,
        program: &FullProgram,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        values: &[LoweredValue],
        call_span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let _machine = self.enter_frame_machine(call_span)?;
        let mut frames = ExplicitFrames::new(self, program);
        frames.push_call(function, kind, values.to_vec(), call_span, None)?;
        frames.run()
    }

    pub(super) fn eval_indexed_with_frame_slots(
        &mut self,
        program: &FullProgram,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        slots: Vec<LoweredValue>,
        call_span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let _machine = self.enter_frame_machine(call_span)?;
        let mut frames = ExplicitFrames::new(self, program);
        frames.push_call_with_slots(function, kind, slots, call_span, None)?;
        frames.run()
    }

    /// Runs a statement block for the recursive evaluator, in a scope of its own.
    pub(super) fn eval_indexed_statement_block(
        &mut self,
        execution: &FullExecution<'_>,
        block: u32,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<StmtFlow, RuntimeError> {
        let cursor = StatementCursor::block(execution, block, span)?;
        let work = FrameWork::Statements {
            cursor,
            complete_call: true,
            scope_id: None,
        };
        self.eval_indexed_work_with_frames(execution, work, true, slots, span)
    }

    /// Runs one top-level statement in the script's scope.
    pub(super) fn eval_indexed_top_level_statement(
        &mut self,
        execution: &FullExecution<'_>,
        statement: u32,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<StmtFlow, RuntimeError> {
        let work = FrameWork::Statements {
            cursor: StatementCursor::single(statement),
            complete_call: true,
            scope_id: None,
        };
        self.eval_indexed_work_with_frames(execution, work, false, slots, span)
    }

    /// Evaluates one expression for the recursive evaluator, in its scope.
    pub(super) fn eval_indexed_expr_with_frames(
        &mut self,
        execution: &FullExecution<'_>,
        instruction: u32,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<ControlFlow<LoweredValue, LoweredValue>, RuntimeError> {
        let work = FrameWork::Expr {
            instruction,
            span,
            next: FrameContinuation::BlockValue {
                transfers_resource: false,
            },
        };
        Ok(
            match self.eval_indexed_work_with_frames(execution, work, false, slots, span)? {
                StmtFlow::Value(value) => ControlFlow::Continue(value),
                flow => self.preserve_lexical_expression_flow(flow),
            },
        )
    }

    /// Statements have one implementation, on the frames, whoever runs them.
    fn eval_indexed_work_with_frames(
        &mut self,
        execution: &FullExecution<'_>,
        first: FrameWork,
        owns_scope: bool,
        slots: &mut [LoweredValue],
        span: Span,
    ) -> Result<StmtFlow, RuntimeError> {
        let _machine = self.enter_frame_machine(span)?;
        let Some(program) = self.indexed_program.clone() else {
            return Err(
                RuntimeError::new("indexed-ir", "statement block has no indexed program")
                    .with_span(span),
            );
        };
        let outer_scope = self.current_scope_id();
        let scope_id = if owns_scope {
            self.enter_owned_host_scope()
        } else {
            outer_scope
        };
        let slot_scopes = self.frame_scratch.take_slot_scopes(0, outer_scope);
        let mut work = self.frame_scratch.take_work();
        work.push(first);
        let open_context_boundaries = context_boundaries(&work);
        let mut frames = ExplicitFrames::new(self, &program);
        frames.open_context_boundaries = open_context_boundaries;
        frames.calls.push(CallFrame {
            owner: FrameOwner::Block {
                owns_scope,
                outer_scope,
            },
            producer: false,
            cancelled: false,
            scope_id,
            execution: execution.thread_local(),
            _symbol_owner: None,
            slots: FrameSlots::Lent(slots),
            slot_scopes,
            call_span: span,
            definition_span: span,
            work,
            defers: Vec::new(),
            block_scopes: Vec::new(),
            block_defer_offsets: Vec::new(),
            return_to: None,
        });
        let result = frames.run();
        let flow = frames.block_flow.take();
        result.map(|_| flow.expect("a finished block frame reports its flow"))
    }
}

fn context_assignment_escape(span: Span) -> RuntimeError {
    RuntimeError::new(
        "context-scope-escape",
        "a live producer or host handle cannot escape through an outer assignment",
    )
    .with_span(span)
}

/// The slot a receiver instruction reads, when it reads exactly one.
fn indexed_slot_read(
    execution: &FullExecution<'_>,
    instruction: u32,
    span: Span,
) -> Result<Option<usize>, RuntimeError> {
    let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
    if tag != FullTag::ExprParam {
        return Ok(None);
    }
    let ExprView::Param(slot) =
        ExprView::decode(execution, tag, payload).map_err(|error| indexed_error(error, span))?
    else {
        unreachable!("parameter tag has a typed parameter view")
    };
    Ok(Some(slot))
}

fn indexed_call_arguments<'e>(
    execution: &'e FullExecution<'_>,
    instruction: u32,
    span: Span,
) -> Result<ListView<'e, LoweredCallArg>, RuntimeError> {
    let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
    let view = ExprView::decode(execution, tag, payload)
        .map_err(|error| indexed_error(error, span))?;
    let args = match view {
        ExprView::Call { args, .. }
        | ExprView::DirectPureCall { args, .. }
        | ExprView::ExternalCall { args, .. }
        | ExprView::DynamicCall { args, .. }
        | ExprView::SelfCall { args, .. }
        | ExprView::TypedCall { args, .. } => args,
        _ => unreachable!("call instruction has a typed call argument list"),
    };
    Ok(args)
}

fn call_argument_at<'e>(
    execution: &'e FullExecution<'_>,
    instruction: u32,
    position: CallArgumentPosition,
    span: Span,
) -> Result<Option<(CallArgumentOperand, CallArgumentPosition)>, RuntimeError> {
    let mut args = indexed_call_arguments(execution, instruction, span)?;
    args.resume(position)
        .map_err(|error| indexed_error(error, span))?;
    let argument = args
        .next()
        .transpose()
        .map_err(|error| indexed_error(error, span))?;
    let position = args.position();
    Ok(argument.map(|argument| {
        let operand = match argument {
            CallArgView::Default(slot) => CallArgumentOperand::Default(slot as usize),
            CallArgView::Single(instruction) => CallArgumentOperand::Expression {
                instruction: instruction.raw(),
                mode: CallArgumentMode::Single,
            },
            CallArgView::Splice(instruction) => CallArgumentOperand::Expression {
                instruction: instruction.raw(),
                mode: CallArgumentMode::Splice,
            },
        };
        (operand, position)
    }))
}

fn indexed_method_arguments<'e>(
    execution: &'e FullExecution<'_>,
    instruction: u32,
    span: Span,
) -> Result<ListView<'e, BuildExprId>, RuntimeError> {
    let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
    let ExprView::Method { args, .. } = ExprView::decode(execution, tag, payload)
        .map_err(|error| indexed_error(error, span))?
    else {
        unreachable!("method instruction has a typed argument list")
    };
    Ok(args)
}

fn method_argument_at(
    execution: &FullExecution<'_>,
    instruction: u32,
    position: MethodArgumentPosition,
    span: Span,
) -> Result<Option<(u32, MethodArgumentPosition)>, RuntimeError> {
    let mut args = indexed_method_arguments(execution, instruction, span)?;
    args.resume(position)
        .map_err(|error| indexed_error(error, span))?;
    let argument = args
        .next()
        .transpose()
        .map_err(|error| indexed_error(error, span))?;
    let position = args.position();
    Ok(argument.map(|argument| (argument.raw(), position)))
}

fn indexed_module_arguments<'e>(
    execution: &'e FullExecution<'_>,
    instruction: u32,
    span: Span,
) -> Result<ListView<'e, Option<BuildExprId>>, RuntimeError> {
    let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
    let ExprView::ModuleCall { args, .. } = ExprView::decode(execution, tag, payload)
        .map_err(|error| indexed_error(error, span))?
    else {
        unreachable!("module call has a typed argument list")
    };
    Ok(args)
}

fn module_argument_at(
    execution: &FullExecution<'_>,
    instruction: u32,
    position: ModuleArgumentPosition,
    span: Span,
) -> Result<Option<(Option<u32>, ModuleArgumentPosition)>, RuntimeError> {
    let mut args = indexed_module_arguments(execution, instruction, span)?;
    args.resume(position)
        .map_err(|error| indexed_error(error, span))?;
    let argument = args
        .next()
        .transpose()
        .map_err(|error| indexed_error(error, span))?;
    let position = args.position();
    Ok(argument.map(|argument| (argument.map(|id| id.raw()), position)))
}

/// Take a receiver out of the slot it was read from, when that slot still holds
/// the same container and nothing else has replaced it.
///
/// The receiver was evaluated before the arguments, and an argument may have
/// assigned to that slot in the meantime; the slot's current value is what the
/// statement stores, so the take only happens while the slot still holds the
/// very value the receiver came from.
fn take_consumed_receiver(
    slots: &mut [LoweredValue],
    consume: Option<usize>,
    receiver: LoweredValue,
) -> LoweredValue {
    let Some(slot) = consume else {
        return receiver;
    };
    let Some(current) = slots.get_mut(slot) else {
        return receiver;
    };
    if super::lowered_shares_backing(current, &receiver) {
        std::mem::replace(current, LoweredValue::Unit)
    } else {
        receiver
    }
}

/// What one pull of a producer did.
pub(super) enum ProducerStep {
    /// The body reached a `yield`; the value is the pulled item and the frame
    /// state is the continuation.
    Yielded {
        value: LoweredValue,
        state: ProducerFrameState,
    },
    /// The parent pauses while this source supplies elements to its consumer.
    Delegated {
        value: LoweredValue,
        span: Span,
        state: ProducerFrameState,
    },
    /// The body ended, propagated an error, or returned a stream.
    Finished(Result<LoweredValue, RuntimeError>),
}

/// Reusable vectors for frame work and scoped frame state.
#[derive(Default)]
pub(in crate::runtime::eval) struct FrameScratch {
    work: Vec<Vec<FrameWork>>,
    slot_scopes: Vec<Vec<u64>>,
    /// A frame's block scopes and their defer offsets, taken when the frame
    /// opens its first block.
    block_stacks: Vec<(Vec<u64>, Vec<usize>)>,
}

impl FrameScratch {
    const POOL_CAP: usize = 32;

    fn take_work(&mut self) -> Vec<FrameWork> {
        self.work.pop().unwrap_or_default()
    }

    fn take_slot_scopes(&mut self, len: usize, fill: u64) -> Vec<u64> {
        let mut scopes = self.slot_scopes.pop().unwrap_or_default();
        scopes.clear();
        scopes.resize(len, fill);
        scopes
    }

    fn take_block_stacks(&mut self) -> (Vec<u64>, Vec<usize>) {
        self.block_stacks.pop().unwrap_or_default()
    }

    /// Returns a finished frame's vectors to the pools, cleared for reuse.
    pub(super) fn recycle(&mut self, call: &mut CallFrame<'_>) {
        call.work.clear();
        if self.work.len() < Self::POOL_CAP {
            self.work.push(std::mem::take(&mut call.work));
        }
        call.slot_scopes.clear();
        if self.slot_scopes.len() < Self::POOL_CAP {
            self.slot_scopes.push(std::mem::take(&mut call.slot_scopes));
        }
    }

    /// Returns a frame's block stacks once its block scopes have exited.
    fn recycle_block_stacks(&mut self, call: &mut CallFrame<'_>) {
        if call.block_scopes.capacity() == 0 || self.block_stacks.len() >= Self::POOL_CAP {
            return;
        }
        call.block_scopes.clear();
        call.block_defer_offsets.clear();
        self.block_stacks.push((
            std::mem::take(&mut call.block_scopes),
            std::mem::take(&mut call.block_defer_offsets),
        ));
    }
}

/// A suspended producer frame, without the borrow that ties it to a program.
///
/// The fields stay private to the frame engine: the producer module starts a
/// body's scope, discards a body's remaining work, and moves the state between
/// pulls, but never reaches into the machine's own bookkeeping.
pub(super) struct ProducerFrameState {
    work: Vec<FrameWork>,
    slots: Vec<LoweredValue>,
    slot_scopes: Vec<u64>,
    defers: Vec<RegisteredDefer>,
    block_scopes: Vec<u64>,
    block_defer_offsets: Vec<usize>,
    scope_id: u64,
}

impl ProducerFrameState {
    /// The state of a producer whose body has not run yet.
    pub(super) fn begin_body(cursor: StatementCursor, slots: Vec<LoweredValue>) -> Self {
        Self {
            work: vec![FrameWork::Statements {
                cursor,
                complete_call: true,
                scope_id: None,
            }],
            slots,
            slot_scopes: Vec::new(),
            defers: Vec::new(),
            block_scopes: Vec::new(),
            block_defer_offsets: Vec::new(),
            scope_id: 0,
        }
    }

    /// The scopes the body has open: its call scope, then its live blocks.
    ///
    /// A suspended producer's scopes are detached from the evaluator's stack
    /// while the consumer runs and reattached for each pull, which keeps the
    /// stack in the order the frame engine expects: a block may only be closed
    /// while it is innermost.
    pub(super) fn open_scopes(&self) -> Vec<u64> {
        let mut scopes = Vec::with_capacity(1 + self.block_scopes.len());
        scopes.push(self.scope_id);
        scopes.extend(self.block_scopes.iter().copied());
        scopes
    }

    pub(super) fn has_context_scope(&self) -> bool {
        self.work.iter().any(FrameWork::is_context_scope)
    }

    pub(super) fn owned_scope_id(&self) -> u64 { self.scope_id }

    /// Visit every value retained between pulls so escaping producers keep
    /// ownership of resources in both slots and unfinished work.
    pub(super) fn visit_retained_values(
        &self,
        visitor: &mut dyn for<'a> FnMut(crate::runtime::eval::RetainedValueRef<'a>),
    ) {
        for slot in &self.slots {
            visit_retained_lowered_value(slot, visitor);
        }
        for work in &self.work {
            match work {
                FrameWork::ClearSlots(_)
                | FrameWork::GuardFailureEnd(_)
                | FrameWork::CleanupBoundary(_) => {}
                FrameWork::CleanupGroup(group) => group.visit_retained_values(visitor),
                FrameWork::Discard(state) => state.visit_retained_values(visitor),
                FrameWork::ExpressionBoundary { policy, next } => {
                    if let ExpressionBoundaryPolicy::Retry(state) = policy {
                        state.visit_retained_values(visitor);
                    }
                    next.visit_retained_values(visitor);
                }
                FrameWork::Expr { next, .. } => {
                    next.visit_retained_values(visitor);
                }
                FrameWork::CompCleanup(streams) => {
                    for stream in streams
                        .lock()
                        .expect("comprehension stream state poisoned")
                        .iter()
                        .flatten()
                    {
                        visitor(crate::runtime::eval::RetainedValueRef::Stream(&stream.0));
                    }
                }
                FrameWork::Statements { .. }
                | FrameWork::ForScalars { .. }
                | FrameWork::While { .. }
                | FrameWork::PatternWhile { .. }
                | FrameWork::FinishError => {}
                FrameWork::Loop { completion, .. } => {
                    if let Some(next) = completion {
                        next.visit_retained_values(visitor);
                    }
                }
                FrameWork::Assertion(state) => state.visit_retained_values(visitor),
                FrameWork::Value { value, next } => {
                    match value {
                        FrameValue::Value(value) | FrameValue::Break(value) => {
                            visit_retained_lowered_value(value, visitor);
                        }
                    }
                    next.visit_retained_values(visitor);
                }
                FrameWork::ForMap { cursor, .. } => cursor.visit_retained_values(visitor),
                FrameWork::ForItems { items, .. } => {
                    visit_retained_lowered_values(items, visitor);
                }
                FrameWork::ForStream { stream, .. } => {
                    visitor(crate::runtime::eval::RetainedValueRef::Stream(stream));
                }
                FrameWork::ForPipeline { pipeline, .. } => {
                    pipeline.visit_retained_values(visitor);
                }
                FrameWork::StageValue {
                    pipeline,
                    next,
                    ..
                } => {
                    pipeline.visit_retained_values(visitor);
                    next.visit_retained_values(visitor);
                }
                FrameWork::ForStrLines { text, .. } => {
                    visit_retained_lowered_value(text, visitor);
                }
                FrameWork::Finish(flow) => visit_retained_flow(flow, visitor),
            }
        }
    }

    /// Enters the body's call scope, so its slots belong to a live scope.
    pub(super) fn start(&mut self, scope_id: u64) {
        self.scope_id = scope_id;
        self.slot_scopes = vec![scope_id; self.slots.len()];
    }
}

impl<'p> CallFrame<'p> {
    fn owns_scope(&self) -> bool {
        !matches!(
            self.owner,
            FrameOwner::Block {
                owns_scope: false,
                ..
            }
        )
    }

    /// The scope that owns a slot's value. A block frame records only the
    /// slots it declares; the rest belong to the scope that lent them.
    fn slot_scope(&self, slot: usize) -> u64 {
        match (self.slot_scopes.get(slot), self.owner) {
            (Some(scope), _) => *scope,
            (None, FrameOwner::Block { outer_scope, .. }) => outer_scope,
            (None, FrameOwner::Function(..)) => self.scope_id,
        }
    }

    /// Drops the body's remaining work, keeping its registered defers.
    pub(super) fn discard_body(&mut self) {
        self.cancelled = true;
        self.work.retain(|work| {
            matches!(
                work,
                FrameWork::Statements {
                    scope_id: Some(_),
                    ..
                } | FrameWork::ExpressionBoundary { .. }
            )
        });
        for work in &mut self.work {
            if let FrameWork::Statements { cursor, .. } = work {
                *cursor = StatementCursor::empty();
            }
        }
        self.work.insert(
            0,
            FrameWork::Statements {
                cursor: StatementCursor::empty(),
                complete_call: true,
                scope_id: None,
            },
        );
    }

    pub(super) fn into_state(self) -> ProducerFrameState {
        ProducerFrameState {
            work: self.work,
            slots: match self.slots {
                FrameSlots::Owned(slots) => slots,
                FrameSlots::Lent(_) => unreachable!("a producer frame owns its slots"),
            },
            slot_scopes: self.slot_scopes,
            defers: self.defers,
            block_scopes: self.block_scopes,
            block_defer_offsets: self.block_defer_offsets,
            scope_id: self.scope_id,
        }
    }
}

/// The frame state a producer resumes from.
impl<'p> CallFrame<'p> {
    /// Rebuilds a frame from the state a previous pull suspended.
    pub(super) fn from_state(
        program: Arc<super::FullProgram>,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        call_span: Span,
        definition_span: Span,
        state: ProducerFrameState,
    ) -> Result<Option<Self>, super::IrVerifyError> {
        let Some(view) = program.function_view(function, kind)? else {
            return Ok(None);
        };
        let symbol_owner = Some(program.symbol_owner().enter());
        let execution = FullExecution::for_function(
            ProgramOwner::Shared(Arc::clone(&program)),
            view.index(),
        )?;
        Ok(Some(CallFrame {
            owner: FrameOwner::Function(function, kind),
            producer: true,
            cancelled: false,
            scope_id: state.scope_id,
            execution,
            _symbol_owner: symbol_owner,
            slots: FrameSlots::Owned(state.slots),
            slot_scopes: state.slot_scopes,
            call_span,
            definition_span,
            work: state.work,
            defers: state.defers,
            block_scopes: state.block_scopes,
            block_defer_offsets: state.block_defer_offsets,
            return_to: None,
        }))
    }
}

impl<'a, 'p> ExplicitFrames<'a, 'p> {
    /// A machine over one program, with no frames of its own yet.
    pub(super) fn new(evaluator: &'a mut Evaluator, program: &'p FullProgram) -> Self {
        Self {
            evaluator,
            program,
            calls: smallvec::SmallVec::new(),
            result: None,
            pending_error: None,
            suspended: None,
            block_flow: None,
            open_context_boundaries: 0,
        }
    }

    fn run(&mut self) -> Result<LoweredValue, RuntimeError> {
        while self.result.is_none() {
            if self.suspended.take().is_some() {
                // Only a producer frame may suspend, and producers run through
                // `run_producer`; reaching this in an ordinary call means a
                // `yield` executed outside a producer.
                let span = self
                    .calls
                    .last()
                    .map(|call| call.call_span)
                    .unwrap_or_else(crate::runtime::eval::zero_span);
                return Err(
                    RuntimeError::new("control-flow", "yield outside stream producer")
                        .with_span(span),
                );
            }
            let index = self
                .calls
                .len()
                .checked_sub(1)
                .expect("active indexed frame");
            if let Err(error) = self.step(index) {
                self.begin_error_unwind(error);
            }
        }
        self.result.take().expect("indexed frame result")
    }

    /// Runs a producer frame until it yields or finishes.
    pub(super) fn run_producer(&mut self, call: CallFrame<'p>) -> ProducerStep {
        // A resumed producer brings the boundaries it was suspended inside.
        self.open_context_boundaries += context_boundaries(&call.work);
        self.calls.push(call);
        while self.result.is_none() && self.suspended.is_none() {
            let index = self
                .calls
                .len()
                .checked_sub(1)
                .expect("active indexed frame");
            if let Err(error) = self.step(index) {
                self.begin_error_unwind(error);
            }
        }
        if let Some(suspension) = self.suspended.take() {
            let state = self
                .calls
                .pop()
                .expect("suspended producer frame")
                .into_state();
            return match suspension {
                ProducerSuspension::Yielded(value) => ProducerStep::Yielded { value, state },
                ProducerSuspension::Delegated { value, span } => {
                    ProducerStep::Delegated { value, span, state }
                }
            };
        }
        ProducerStep::Finished(self.result.take().expect("indexed frame result"))
    }

    fn step_print(&mut self, index: usize, mut state: FramePrintState) -> Result<(), RuntimeError> {
        if !state.position.is_exhausted() {
            let (instruction, position) = statement_print_argument(
                &self.calls[index].execution,
                state.statement,
                state.position,
                state.span,
            )?;
            state.position = position;
            self.push_expr(
                index,
                instruction,
                state.span,
                FrameContinuation::PrintArgument(state),
            );
            return Ok(());
        }

        let trace_name = if state.stderr { "eprint" } else { "print" };
        self.evaluator.trace_enter(
            TraceKind::CoreCall,
            Some(state.span),
            Some(trace_name),
            TracePayload::Core { argv: state.argv },
        );
        if state.stderr && state.flush {
            self.evaluator.flush_stderr_line(&state.line);
        } else if state.stderr {
            self.evaluator.write_stderr_line(&state.line);
        } else if state.flush {
            self.evaluator.flush_stdout_line(&state.line);
        } else {
            self.evaluator.write_stdout_line(&state.line);
        }
        self.evaluator.trace_exit(
            TraceKind::CoreResult,
            Some(state.span),
            Some(trace_name),
            TracePayload::None,
        );
        if state.propagate_result {
            match self.evaluator.last_status.as_ref().and_then(|status| status.code) {
                Some(0) | None => Ok(()),
                Some(code) => {
                    self.complete_call(index, StmtFlow::Propagate(LoweredValue::Int(i64::from(code))))
                }
            }
        } else {
            Ok(())
        }
    }

    fn step_env(&mut self, index: usize, state: FrameEnvState) -> Result<(), RuntimeError> {
        if !state.position.is_exhausted() {
            let (assignment, _) = statement_env_assignment(
                &self.calls[index].execution,
                state.statement,
                state.position,
                state.span,
            )?;
            let RunEnvView { value, .. } = assignment;
            let RunArgView { kind, span, .. } = value;
            let argument = indexed_run_arg(kind, span);
            let instruction = argument.value;
            let span = argument.span;
            self.push_expr(
                index,
                instruction,
                span,
                FrameContinuation::EnvValue(state),
            );
            return Ok(());
        }

        let restore = self
            .evaluator
            .enter_indexed_env_scope(state.overlay);
        self.calls[index].work.push(FrameWork::ExpressionBoundary {
            policy: ExpressionBoundaryPolicy::Scope(restore),
            next: FrameContinuation::StatementScopeEnd,
        });
        self.push_statement_block(index, state.body, state.span)
    }

    fn start_process_run(
        &mut self,
        index: usize,
        instruction: u32,
        tag: FullTag,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        let (mode, segment_cursor, span) = match tag {
            FullTag::ExprRunCapture => {
                let execution = &self.calls[index].execution;
                let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
                let super::ExprView::RunCapture(value) =
                    super::ExprView::decode(execution, tag, payload)
                        .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("run capture tag has a capture view")
                };
                (ProcessExecutionMode::Capture, None, value.span)
            }
            FullTag::ExprSpawnRun => {
                let execution = &self.calls[index].execution;
                let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
                let super::ExprView::SpawnRun(value) =
                    super::ExprView::decode(execution, tag, payload)
                        .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("spawn-run tag has a spawn view")
                };
                (ProcessExecutionMode::Spawn, None, value.span)
            }
            FullTag::ExprRunPipeline => {
                let execution = &self.calls[index].execution;
                let (tag, payload) = indexed_value(execution.instruction_id(instruction), span)?;
                let super::ExprView::RunPipeline {
                    segments,
                    propagate,
                    span: pipeline_span,
                } = super::ExprView::decode(execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("pipeline tag has a pipeline view")
                };
                (
                    ProcessExecutionMode::Pipeline { propagate },
                    Some(segments.position()),
                    pipeline_span,
                )
            }
            _ => unreachable!("process runner starts only run expressions"),
        };
        self.step_process_run(
            index,
            Box::new(ProcessExecutionState {
                instruction,
                tag,
                span,
                mode,
                phase: ProcessExecutionPhase::Target,
                pending: None,
                segment_cursor,
                segment_position: None,
                args_position: None,
                env_position: None,
                redirections_position: None,
                invocation: ProcessInvocationBuilder::new(),
                invocations: Vec::new(),
                pipeline_head: None,
            }),
            next,
        )
    }

    fn begin_next_process_segment(
        &self,
        index: usize,
        state: &mut ProcessExecutionState,
    ) -> Result<bool, RuntimeError> {
        let position = state
            .segment_cursor
            .expect("pipeline runner retains its next segment cursor");
        let execution = &self.calls[index].execution;
        let (tag, payload) = indexed_value(
            execution.instruction_id(state.instruction),
            state.span,
        )?;
        let super::ExprView::RunPipeline { mut segments, .. } =
            super::ExprView::decode(execution, tag, payload)
                .map_err(|error| indexed_error(error, state.span))?
        else {
            unreachable!("pipeline tag has a pipeline view")
        };
        segments
            .resume(position)
            .map_err(|error| indexed_error(error, state.span))?;
        let Some(segment) = segments
            .next()
            .transpose()
            .map_err(|error| indexed_error(error, state.span))?
        else {
            state.segment_cursor = Some(segments.position());
            return Ok(false);
        };
        state.segment_position = Some(position);
        state.segment_cursor = Some(segments.position());
        state.args_position = Some(segment.args.position());
        state.env_position = Some(segment.env.position());
        state.redirections_position = Some(segment.redirections.position());
        state.phase = ProcessExecutionPhase::Target;
        Ok(true)
    }

    fn step_process_run(
        &mut self,
        index: usize,
        mut state: Box<ProcessExecutionState>,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        loop {
            if matches!(state.mode, ProcessExecutionMode::Pipeline { .. })
                && state.segment_position.is_none()
            {
                if self.begin_next_process_segment(index, &mut state)? {
                    continue;
                }
                let ProcessExecutionMode::Pipeline { propagate } = state.mode else {
                    unreachable!()
                };
                let head = state
                    .pipeline_head
                    .unwrap_or(RunKind::Plain);
                let flow = self.evaluator.apply_indexed_run_pipeline(
                    head,
                    &state.invocations,
                    propagate,
                    state.span,
                )?;
                return self.push_process_flow(index, flow, state.span, next);
            }

            let mut operands = process_run_operands(
                &self.calls[index].execution,
                state.instruction,
                state.tag,
                state.segment_position,
                state.span,
            )?;
            if state.args_position.is_none() {
                state.args_position = Some(operands.args.position());
                state.env_position = Some(operands.env.position());
                state.redirections_position = Some(operands.redirections.position());
            }
            match state.phase {
                ProcessExecutionPhase::Target => {
                    let (mode, child, target_span) = run_arg_parts(operands.target);
                    state.phase = ProcessExecutionPhase::Arguments;
                    state.pending = Some(ProcessExecutionOperand::Target {
                        mode,
                        span: target_span,
                    });
                    self.push_expr(
                        index,
                        child,
                        operands.span,
                        FrameContinuation::ProcessRun {
                            state,
                            next: Box::new(next),
                        },
                    );
                    return Ok(());
                }
                ProcessExecutionPhase::Arguments => {
                    operands
                        .args
                        .resume(state.args_position.expect("run argument cursor initialized"))
                        .map_err(|error| indexed_error(error, operands.span))?;
                    if let Some(argument) = operands
                        .args
                        .next()
                        .transpose()
                        .map_err(|error| indexed_error(error, operands.span))?
                    {
                        state.args_position = Some(operands.args.position());
                        let (mode, child, arg_span) = run_arg_parts(argument);
                        state.pending = Some(ProcessExecutionOperand::Argument {
                            mode,
                            span: arg_span,
                        });
                        self.push_expr(
                            index,
                            child,
                            operands.span,
                            FrameContinuation::ProcessRun {
                                state,
                                next: Box::new(next),
                            },
                        );
                        return Ok(());
                    }
                    state.phase = ProcessExecutionPhase::Environment;
                }
                ProcessExecutionPhase::Environment => {
                    operands
                        .env
                        .resume(state.env_position.expect("run environment cursor initialized"))
                        .map_err(|error| indexed_error(error, operands.span))?;
                    if let Some(assignment) = operands
                        .env
                        .next()
                        .transpose()
                        .map_err(|error| indexed_error(error, operands.span))?
                    {
                        state.env_position = Some(operands.env.position());
                        let RunEnvView { name, value, .. } = assignment;
                        let (mode, child, value_span) = run_arg_parts(value);
                        state.pending = Some(ProcessExecutionOperand::Environment {
                            name,
                            mode,
                            span: value_span,
                        });
                        self.push_expr(
                            index,
                            child,
                            operands.span,
                            FrameContinuation::ProcessRun {
                                state,
                                next: Box::new(next),
                            },
                        );
                        return Ok(());
                    }
                    state.phase = ProcessExecutionPhase::Redirections;
                }
                ProcessExecutionPhase::Redirections => {
                    operands
                        .redirections
                        .resume(
                            state
                                .redirections_position
                                .expect("run redirection cursor initialized"),
                        )
                        .map_err(|error| indexed_error(error, operands.span))?;
                    if let Some(redirection) = operands
                        .redirections
                        .next()
                        .transpose()
                        .map_err(|error| indexed_error(error, operands.span))?
                    {
                        state.redirections_position = Some(operands.redirections.position());
                        let (mode, child, target_span) = run_arg_parts(redirection.target);
                        state.pending = Some(ProcessExecutionOperand::Redirection {
                            kind: redirection.kind,
                            mode,
                            target_span,
                            span: redirection.span,
                        });
                        self.push_expr(
                            index,
                            child,
                            redirection.span,
                            FrameContinuation::ProcessRun {
                                state,
                                next: Box::new(next),
                            },
                        );
                        return Ok(());
                    }
                    state.phase = ProcessExecutionPhase::Timeout;
                }
                ProcessExecutionPhase::Timeout => {
                    state.phase = ProcessExecutionPhase::CpuMax;
                    if let Some(child) = operands.timeout {
                        state.pending = Some(ProcessExecutionOperand::Timeout(operands.span));
                        self.push_expr(
                            index,
                            child,
                            operands.span,
                            FrameContinuation::ProcessRun {
                                state,
                                next: Box::new(next),
                            },
                        );
                        return Ok(());
                    }
                }
                ProcessExecutionPhase::CpuMax => {
                    state.phase = ProcessExecutionPhase::Accept;
                    if let Some(child) = operands.cpu_max {
                        state.pending = Some(ProcessExecutionOperand::CpuMax(operands.span));
                        self.push_expr(
                            index,
                            child,
                            operands.span,
                            FrameContinuation::ProcessRun {
                                state,
                                next: Box::new(next),
                            },
                        );
                        return Ok(());
                    }
                }
                ProcessExecutionPhase::Accept => {
                    state.phase = ProcessExecutionPhase::Finish;
                    if let Some(child) = operands.accept {
                        state.pending = Some(ProcessExecutionOperand::Accept(operands.span));
                        self.push_expr(
                            index,
                            child,
                            operands.span,
                            FrameContinuation::ProcessRun {
                                state,
                                next: Box::new(next),
                            },
                        );
                        return Ok(());
                    }
                }
                ProcessExecutionPhase::Finish => {
                    let invocation = std::mem::replace(
                        &mut state.invocation,
                        ProcessInvocationBuilder::new(),
                    )
                    .finish(&mut self.evaluator);
                    if matches!(state.mode, ProcessExecutionMode::Pipeline { .. }) {
                        state.pipeline_head.get_or_insert(operands.kind);
                        state.invocations.push(invocation);
                        state.segment_position = None;
                        state.args_position = None;
                        state.env_position = None;
                        state.redirections_position = None;
                        state.phase = ProcessExecutionPhase::Target;
                        continue;
                    }
                    let flow = match state.mode {
                        ProcessExecutionMode::Capture => {
                            let (tag, payload) = indexed_value(
                                self.calls[index]
                                    .execution
                                    .instruction_id(state.instruction),
                                state.span,
                            )?;
                            let super::ExprView::RunCapture(capture) =
                                super::ExprView::decode(
                                    &self.calls[index].execution,
                                    tag,
                                    payload,
                                )
                                .map_err(|error| indexed_error(error, state.span))?
                            else {
                                unreachable!("run capture tag has a capture view")
                            };
                            self.evaluator.apply_indexed_run_capture(
                                capture.kind,
                                invocation,
                                capture.propagate,
                                capture.assert_success,
                                capture.span,
                            )?
                        }
                        ProcessExecutionMode::Spawn => self.evaluator.eval_lowered_spawn_invocation(
                            invocation,
                            super::SpawnOptions::default(),
                            state.span,
                        )?,
                        ProcessExecutionMode::Pipeline { .. } => unreachable!(),
                    };
                    return self.push_process_flow(index, flow, state.span, next);
                }
            }
        }
    }

    fn continue_process_run(
        &mut self,
        index: usize,
        mut state: Box<ProcessExecutionState>,
        value: LoweredValue,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        let operand = state
            .pending
            .take()
            .expect("run expression child has a conversion descriptor");
        match operand {
            ProcessExecutionOperand::Target { mode, span } => {
                let items = Evaluator::run_arg_items_for(mode, span, value.into_value())?;
                let (target, leading) = super::run_target_and_leading_argv(mode, span, items)?;
                state.invocation.target = Some(target);
                state.invocation.argv.extend(leading);
            }
            ProcessExecutionOperand::Argument { mode, span } => {
                state
                    .invocation
                    .argv
                    .extend(Evaluator::run_arg_items_for(mode, span, value.into_value())?);
            }
            ProcessExecutionOperand::Environment { name, mode, span } => {
                let value = Evaluator::run_env_value_for(mode, span, value.into_value())?;
                state
                    .invocation
                    .env_overlay
                    .insert(name.as_str().as_bytes().to_vec(), value);
            }
            ProcessExecutionOperand::Redirection {
                kind,
                mode,
                target_span,
                span,
            } => state.invocation.redirections.push(
                self.evaluator
                    .run_redirection_value(kind, mode, target_span, span, value)?,
            ),
            ProcessExecutionOperand::Timeout(span) => {
                state.invocation.timeout = Some(super::process::run_timeout_value(value, span)?);
            }
            ProcessExecutionOperand::CpuMax(span) => {
                state.invocation.cpu_max = Some(super::process::run_cpu_max_value(value, span)?);
            }
            ProcessExecutionOperand::Accept(span) => {
                state.invocation.accepted_exit_codes = Some(
                    super::super::lowered_accepted_exit_codes(value, span)?,
                );
            }
        }
        self.step_process_run(index, state, next)
    }

    fn push_process_flow(
        &mut self,
        index: usize,
        mut flow: ControlFlow<LoweredValue, LoweredValue>,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        self.evaluator
            .register_returned_lowered_streams(&mut flow, span)?;
        self.push_value(
            index,
            match flow {
                ControlFlow::Continue(value) => FrameValue::Value(value),
                ControlFlow::Break(value) => FrameValue::Break(value),
            },
            next,
        );
        Ok(())
    }

    fn begin_error_unwind(&mut self, mut error: RuntimeError) {
        if error.abort.as_ref().is_some_and(|signal| signal.force) {
            self.discard_calls();
            self.result = Some(Err(error));
            return;
        }
        let mut stage_error_was_wrapped =
            self.evaluator.take_pending_stream_item_error_unwind();
        // A failure that starts to unwind is a `within` checkpoint: what
        // fails after the deadline, such as the child the deadline stopped,
        // leaves as the scope's timeout.
        if error.abort.is_none() && error.within.is_none() && self.pending_error.is_none() {
            let span = self
                .calls
                .last()
                .map(|call| call.call_span)
                .unwrap_or_else(crate::runtime::eval::zero_span);
            if let Err(timeout) = self.evaluator.check_within_deadline(span) {
                error = timeout;
                stage_error_was_wrapped = false;
            }
        }
        if !stage_error_was_wrapped && error.abort.is_none() && self.pending_error.is_none() {
            // A block callback leaves statement and function frames above the
            // pipeline continuation while an item error unwinds.
            let stage_item = self.calls.iter().rev().find_map(|call| {
                call.work.iter().rev().find_map(|work| match work {
                    FrameWork::ForPipeline { pipeline, .. } => pipeline
                        .pending_map_error_item()
                        .map(|item_index| ("map", item_index)),
                    FrameWork::StageValue { pipeline, .. } => pipeline.pending_item_error(),
                    _ => None,
                })
            });
            if let Some((stage, item_index)) = stage_item {
                error = self
                    .evaluator
                    .stream_item_runtime_error(stage, item_index, error);
            }
        }
        if self.pending_error.is_some() {
            self.evaluator.report_cleanup_error(
                &error,
                self.calls
                    .last()
                    .map(|call| call.call_span)
                    .unwrap_or_else(crate::runtime::eval::zero_span),
            );
        } else {
            self.pending_error = Some(error);
        }
        let Some(index) = self.calls.len().checked_sub(1) else {
            self.result = Some(Err(self
                .pending_error
                .take()
                .expect("pending indexed frame error")));
            return;
        };
        if let Err(error) = self.unwind_error_frame(index) {
            self.begin_error_unwind(error);
        }
    }

    fn capture_boundary(&self, index: usize) -> Option<usize> {
        self.calls[index].work.iter().rposition(|work| {
            matches!(
                work,
                FrameWork::ExpressionBoundary {
                    policy: ExpressionBoundaryPolicy::Capture
                        | ExpressionBoundaryPolicy::Retry(_),
                    ..
                }
            )
        })
    }

    fn cleanup_action_boundary(&self, index: usize) -> Option<usize> {
        self.calls[index]
            .work
            .iter()
            .rposition(|work| matches!(work, FrameWork::CleanupBoundary(_)))
    }

    fn cleanup_action_scope(&self, index: usize, boundary: usize) -> u64 {
        match boundary.checked_sub(1).and_then(|group| self.calls[index].work.get(group)) {
            Some(FrameWork::CleanupGroup(group)) => match &group.resume {
                CleanupGroupResume::Block { scope_id, .. } => *scope_id,
                CleanupGroupResume::Call(_) | CleanupGroupResume::ErrorCall => {
                    self.calls[index].scope_id
                }
            },
            _ => unreachable!("cleanup action boundary has its group directly below it"),
        }
    }

    fn cleanup_flow_boundary(&self, index: usize, flow: &StmtFlow) -> Option<usize> {
        let cleanup = self.cleanup_action_boundary(index)?;
        match flow {
            StmtFlow::Propagate(_) => {
                if self
                    .capture_boundary(index)
                    .is_some_and(|capture| capture > cleanup)
                {
                    return None;
                }
            }
            StmtFlow::Break(_) | StmtFlow::Continue => {
                if self
                    .innermost_loop(index)
                    .is_some_and(|loop_boundary| loop_boundary > cleanup)
                {
                    return None;
                }
            }
            _ => {}
        }
        Some(cleanup)
    }

    fn schedule_cleanup_flow(
        &mut self,
        index: usize,
        boundary: usize,
        flow: StmtFlow,
    ) -> Result<(), RuntimeError> {
        if let StmtFlow::Propagate(value) = &flow {
            if self.crosses_context_scope(index, boundary + 1)
                && Evaluator::context_scope_value_escapes(value)
            {
                self.evaluator.pending_traceback = None;
                return Err(RuntimeError::new(
                    "context-scope-escape",
                    "a live producer or host handle cannot escape a restored context",
                )
                .with_span(self.calls[index].call_span));
            }
            let survivor = self.cleanup_action_scope(index, boundary);
            self.evaluator
                .transfer_owned_host_resources_in_lowered_value(value, survivor);
        }
        let contexts = self.calls[index].work[boundary + 1..]
            .iter()
            .rev()
            .filter_map(|work| match work {
                FrameWork::ExpressionBoundary {
                    policy: ExpressionBoundaryPolicy::Context(context),
                    ..
                } => Some(context.clone()),
                _ => None,
            })
            .collect();
        self.schedule_discard(
            index,
            boundary + 1,
            flow_leaves_with_error(&flow),
            DiscardResume::CleanupFlow {
                boundary,
                flow,
                contexts,
            },
        );
        Ok(())
    }

    /// The boundary of the `within` scope `id` in this frame.
    fn within_boundary(&self, index: usize, id: u64) -> Option<usize> {
        self.calls[index].work.iter().rposition(|work| {
            matches!(
                work,
                FrameWork::ExpressionBoundary {
                    policy: ExpressionBoundaryPolicy::Scope(super::ContextScopeRestore::Within {
                        id: scope,
                    }),
                    ..
                } if *scope == id
            )
        })
    }

    fn boundary_survivor_scope(&self, index: usize, boundary: usize) -> u64 {
        self.calls[index].work[..boundary]
            .iter()
            .rev()
            .find_map(|work| match work {
                FrameWork::Statements {
                    scope_id: Some(scope),
                    ..
                } => Some(*scope),
                _ => None,
            })
            .unwrap_or(self.calls[index].scope_id)
    }

    fn crosses_context_scope(&self, index: usize, keep: usize) -> bool {
        self.calls[index].work[keep..]
            .iter()
            .any(FrameWork::is_context_scope)
    }

    // Callee cleanup completes before searching its caller for a local capture.
    // The selected boundary remains live while its inner lexical scopes unwind.
    fn unwind_error_frame(&mut self, index: usize) -> Result<(), RuntimeError> {
        // A `within` timeout stops at its own scope and nowhere before it.
        let timeout = self.pending_error.as_ref().and_then(|error| error.within);
        let mut boundary = match timeout {
            Some(id) => self.within_boundary(index, id),
            None => self
                .pending_error
                .as_ref()
                .filter(|error| error.abort.is_none() && error.propagated)
                .and_then(|_| self.capture_boundary(index)),
        };
        if !self
            .pending_error
            .as_ref()
            .is_some_and(|error| error.abort.as_ref().is_some_and(|signal| signal.force))
            && let Some(cleanup) = self.cleanup_action_boundary(index)
            && boundary.is_none_or(|boundary| cleanup > boundary)
        {
            boundary = Some(cleanup);
        }
        let mut keep = boundary.map_or(0, |boundary| boundary + 1);
        if self.crosses_context_scope(index, keep)
            && self
                .pending_error
                .as_ref()
                .is_some_and(Evaluator::context_scope_runtime_error_escapes)
        {
            self.evaluator.pending_traceback = None;
            self.pending_error = Some(
                RuntimeError::new(
                    "context-scope-escape",
                    "a live producer or host handle cannot escape a restored context",
                )
                .with_span(self.calls[index].call_span),
            );
            boundary = None;
            keep = 0;
        }
        let survivor = boundary.map_or(self.calls[index].scope_id, |boundary| {
            if matches!(
                self.calls[index].work.get(boundary),
                Some(FrameWork::CleanupBoundary(_))
            ) {
                self.cleanup_action_scope(index, boundary)
            } else {
                self.boundary_survivor_scope(index, boundary)
            }
        });
        if let Some(error) = self.pending_error.as_ref()
            && error.abort.is_none()
            && error.propagated
        {
            self.evaluator
                .transfer_owned_host_resources_in_runtime_error(error, survivor);
        }
        self.schedule_discard(
            index,
            keep,
            true,
            DiscardResume::UnwindError { boundary },
        );
        Ok(())
    }

    fn discard_calls(&mut self) {
        while let Some(mut call) = self.calls.pop() {
            if let FrameOwner::Function(function, kind) = call.owner
                && let Ok(header) = Self::function_header(
                    call.execution.program(),
                    function,
                    kind,
                    call.call_span,
                )
            {
                let _ = self.evaluator.write_back_lowered_captures(
                    &header,
                    &call.slots,
                    call.call_span,
                );
            }
            // Forced discard skips user defers but still closes inner owned
            // scopes before restoring each evaluator context in lexical order.
            for work in call.work.drain(..).rev() {
                match work {
                    FrameWork::Statements {
                        scope_id: Some(scope_id),
                        ..
                    } => {
                        if call.block_scopes.last() == Some(&scope_id) {
                            call.block_scopes.pop();
                            let _ = self.evaluator.exit_owned_host_scope(scope_id);
                        }
                    }
                    FrameWork::ExpressionBoundary {
                        policy: ExpressionBoundaryPolicy::Scope(restore),
                        ..
                    } => self.evaluator.restore_indexed_context_scope(restore),
                    FrameWork::CleanupBoundary(guard) => {
                        self.restore_cleanup_action_guard(guard);
                    }
                    FrameWork::Discard(state) => {
                        self.evaluator.cleanup_error_contexts = state.previous_cleanup_contexts;
                    }
                    _ => {}
                }
            }
            let _ = cleanup_call_scopes(self.evaluator, &mut call);
            let FrameOwner::Function(function, kind) = call.owner else {
                continue;
            };
            self.release_slots(call.slots);
            self.evaluator.call_stack.pop();
            let exit_kind = match kind {
                LoweredFunctionKind::Pure => TraceKind::PureExit,
                LoweredFunctionKind::Proc => TraceKind::ProcExit,
            };
            let name = function.display_name();
            self.evaluator.trace_exit_with_definition(
                exit_kind,
                Some(call.call_span),
                Some(call.definition_span),
                Some(&name),
                TracePayload::None,
            );
            if let Some(FrameContinuation::UtilsCacheResult { span, .. }) =
                call.return_to.as_ref()
            {
                self.evaluator.trace_lowered_utils_cache_result(*span);
            }
        }
    }

    fn resolve_active_call(
        &self,
        index: usize,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        allow_dynamic: bool,
        span: Span,
    ) -> Result<ResolvedCall<'p>, RuntimeError> {
        let caller = self.calls[index].execution.program_owner();
        if let Some(function_index) = caller
            .program()
            .function_view(function, kind)
            .map_err(|error| indexed_error(error, span))?
            .map(|view| view.index())
        {
            return Ok(ResolvedCall {
                program: caller,
                function,
                kind,
                index: function_index,
            });
        }
        if allow_dynamic && let LoweredFunctionKey::Qualified(qualified) = function {
            if let Some(dynamic) = self.evaluator.indexed_dynamic_functions.get(&qualified) {
                if dynamic.kind != kind {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!("callable {} has a different function kind", function.display_name()),
                    )
                    .with_span(span));
                }
                let view = dynamic
                    .program
                    .function_view(dynamic.function, dynamic.kind)
                    .map_err(|error| indexed_error(error, span))?
                    .ok_or_else(|| {
                        RuntimeError::new("unresolved-lowered-call", function.display_name())
                            .with_span(span)
                    })?;
                return Ok(ResolvedCall {
                    program: ProgramOwner::Shared(Arc::clone(&dynamic.program)),
                    function: dynamic.function,
                    kind: dynamic.kind,
                    index: view.index(),
                });
            }
        }
        Err(RuntimeError::new("unresolved-lowered-call", function.display_name()).with_span(span))
    }

    fn resolve_active_external(
        &self,
        index: usize,
        qualified: crate::symbol::QualifiedName,
        span: Span,
    ) -> Result<ResolvedCall<'p>, RuntimeError> {
        let caller = self.calls[index].execution.program_owner();
        let function = LoweredFunctionKey::Qualified(qualified);
        for kind in [LoweredFunctionKind::Pure, LoweredFunctionKind::Proc] {
            if let Some(function_index) = caller
                .program()
                .function_view(function, kind)
                .map_err(|error| indexed_error(error, span))?
                .map(|view| view.index())
            {
                return Ok(ResolvedCall {
                    program: caller,
                    function,
                    kind,
                    index: function_index,
                });
            }
        }
        let dynamic = self
            .evaluator
            .indexed_dynamic_functions
            .get(&qualified)
            .ok_or_else(|| {
                RuntimeError::new("unresolved-lowered-call", qualified.to_string())
                    .with_span(span)
            })?;
        let view = dynamic
            .program
            .function_view(dynamic.function, dynamic.kind)
            .map_err(|error| indexed_error(error, span))?
            .ok_or_else(|| {
                RuntimeError::new("unresolved-lowered-call", qualified.to_string())
                    .with_span(span)
            })?;
        Ok(ResolvedCall {
            program: ProgramOwner::Shared(Arc::clone(&dynamic.program)),
            function: dynamic.function,
            kind: dynamic.kind,
            index: view.index(),
        })
    }

    fn argument_default_for_active(
        &self,
        index: usize,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        slot: usize,
        span: Span,
    ) -> Result<LoweredValue, RuntimeError> {
        let caller = self.calls[index].execution.program_owner();
        let view = if let Some(view) = caller
            .program()
            .function_view(function, kind)
            .map_err(|error| indexed_error(error, span))?
        {
            view
        } else if let LoweredFunctionKey::Qualified(qualified) = function {
            let dynamic = self
                .evaluator
                .indexed_dynamic_functions
                .get(&qualified)
                .ok_or_else(|| RuntimeError::new("unresolved-call", "argument default callable is not prepared").with_span(span))?;
            if dynamic.kind != kind {
                return Err(RuntimeError::new(
                    "type-error",
                    format!("callable {} has a different function kind", function.display_name()),
                )
                .with_span(span));
            }
            dynamic
                .program
                .function_view(dynamic.function, dynamic.kind)
                .map_err(|error| indexed_error(error, span))?
                .ok_or_else(|| RuntimeError::new("unresolved-call", "argument default callable is not prepared").with_span(span))?
        } else {
            return Err(RuntimeError::new("unresolved-call", "argument default callable is not prepared").with_span(span));
        };
        view.header()
            .map_err(|error| indexed_error(error, span))?
            .param_defaults
            .get(slot)
            .and_then(Clone::clone)
            .ok_or_else(|| RuntimeError::new("indexed-ir", "checked omitted argument has no prepared default").with_span(span))
    }

    /// Resolves a fully evaluated call: a stream producer runs to completion
    /// and hands its stream back, and every other callee becomes a frame.
    ///
    /// The frame engine reaches this decision from its argument-walking
    /// continuation and, for a call with no arguments, directly; both go
    /// through here so a producer is never pushed as an ordinary frame, which
    /// would run its body with nowhere for `yield` to report and then fail the
    /// call as a function that did not return.
    fn push_resolved_call(
        &mut self,
        index: usize,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        values: Vec<LoweredValue>,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        let resolved = self.resolve_active_call(index, function, kind, true, span)?;
        self.push_resolved_target(index, resolved, values, span, next)
    }

    fn push_resolved_external(
        &mut self,
        index: usize,
        qualified: crate::symbol::QualifiedName,
        values: Vec<LoweredValue>,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        let resolved = self.resolve_active_external(index, qualified, span)?;
        self.push_resolved_target(index, resolved, values, span, next)
    }

    fn push_resolved_target(
        &mut self,
        index: usize,
        resolved: ResolvedCall<'p>,
        values: Vec<LoweredValue>,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        let target = resolved.program.program();
        let stream_call = match target.function_view(resolved.function, resolved.kind)
            .map_err(|error| indexed_error(error, span))? {
            Some(view) => matches!(
                view.header()
                    .map_err(|error| indexed_error(error, span))?
                    .return_kind,
                LoweredReturnKind::Plain(LoweredType::Stream)
            ),
            None => false,
        };
        if stream_call {
            let program = resolved
                .program
                .dispatch_arc(self.evaluator.indexed_program.as_ref())
                .ok_or_else(|| {
                    RuntimeError::new(
                        "indexed-ir",
                        "stream call lost its executable program owner",
                    )
                    .with_span(span)
                })?;
            let value = self.evaluator.eval_indexed_call_at(
                &program,
                resolved.function,
                resolved.kind,
                resolved.index,
                &values,
                span,
            )?;
            self.push_value(index, FrameValue::Value(value), next);
            return Ok(());
        }
        self.push_call_in_owner(index, resolved, values, span, Some(next))
    }

    fn push_method_arguments(
        &mut self,
        index: usize,
        instruction: u32,
        position: MethodArgumentPosition,
        name: Arc<str>,
        receiver: LoweredValue,
        consume: Option<usize>,
        values: Vec<LoweredValue>,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        if let Some((argument, position)) = method_argument_at(
            &self.calls[index].execution,
            instruction,
            position,
            span,
        )? {
            self.push_expr(
                index,
                argument,
                span,
                FrameContinuation::MethodArg {
                    instruction,
                    position,
                    name,
                    receiver,
                    consume,
                    values,
                    span,
                    next: Box::new(next),
                },
            );
        } else {
            let receiver =
                take_consumed_receiver(&mut self.calls[index].slots, consume, receiver);
            self.push_method_result(index, receiver, name, values, span, next)?;
        }
        Ok(())
    }

    /// Walks a named call's borrowed arguments, filling omitted defaults at their encoded positions.
    fn push_static_arguments(
        &mut self,
        index: usize,
        instruction: u32,
        mut position: CallArgumentPosition,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        external: bool,
        mut values: Vec<LoweredValue>,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        while let Some((argument, next_position)) = call_argument_at(
            &self.calls[index].execution,
            instruction,
            position,
            span,
        )? {
            match argument {
                CallArgumentOperand::Default(slot) => {
                    values.push(self.argument_default_for_active(
                        index, function, kind, slot, span,
                    )?);
                    position = next_position;
                }
                CallArgumentOperand::Expression {
                    instruction: argument,
                    mode,
                } => {
                    self.push_expr(
                        index,
                        argument,
                        span,
                        FrameContinuation::CallArguments {
                            function,
                            kind,
                            external,
                            instruction,
                            position: next_position,
                            mode,
                            values,
                            span,
                            next: Box::new(next),
                        },
                    );
                    return Ok(());
                }
            }
        }
        if external {
            let LoweredFunctionKey::Qualified(qualified) = function else {
                unreachable!("external calls have qualified names")
            };
            self.push_resolved_external(index, qualified, values, span, next)
        } else {
            self.push_resolved_call(index, function, kind, values, span, next)
        }
    }

    fn push_dynamic_arguments(
        &mut self,
        index: usize,
        callee: LoweredValue,
        instruction: u32,
        mut position: CallArgumentPosition,
        mut values: Vec<LoweredValue>,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        while let Some((argument, next_position)) = call_argument_at(
            &self.calls[index].execution,
            instruction,
            position,
            span,
        )? {
            match argument {
                CallArgumentOperand::Default(slot) => {
                    let (function, function_kind) = indexed_callable_identity(&callee, span)?;
                    values.push(self.argument_default_for_active(
                        index,
                        function,
                        function_kind,
                        slot,
                        span,
                    )?);
                    position = next_position;
                }
                CallArgumentOperand::Expression {
                    instruction: argument,
                    mode,
                } => {
                    self.push_expr(
                        index,
                        argument,
                        span,
                        FrameContinuation::DynamicArguments {
                            callee,
                            instruction,
                            position: next_position,
                            mode,
                            values,
                            span,
                            next: Box::new(next),
                        },
                    );
                    return Ok(());
                }
            }
        }
        let (function, kind) = indexed_callable_identity(&callee, span)?;
        self.push_resolved_call(index, function, kind, values, span, next)
    }

    fn push_call(
        &mut self,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        values: Vec<LoweredValue>,
        call_span: Span,
        return_to: Option<FrameContinuation>,
    ) -> Result<(), RuntimeError> {
        let view = self
            .program
            .function_view(function, kind)
            .map_err(|error| indexed_error(error, call_span))?
            .ok_or_else(|| {
                RuntimeError::new("unresolved-lowered-call", function.display_name())
                    .with_span(call_span)
            })?;
        let header = view
            .header()
            .map_err(|error| indexed_error(error, call_span))?;
        let slots = self
            .evaluator
            .bind_lowered_values_owned(&header, values, call_span)?;
        let execution = view
            .execution()
            .map_err(|error| indexed_error(error, call_span))?;
        self.push_call_with_header(function, kind, view, execution, None, header, slots, call_span, return_to)
    }

    fn push_call_in_owner(
        &mut self,
        caller_index: usize,
        resolved: ResolvedCall<'p>,
        values: Vec<LoweredValue>,
        call_span: Span,
        return_to: Option<FrameContinuation>,
    ) -> Result<(), RuntimeError> {
        let is_cross_program = !std::ptr::eq(
            self.calls[caller_index].execution.program(),
            resolved.program.program(),
        );
        let symbol_owner = is_cross_program
            .then(|| resolved.program.program().symbol_owner().enter());
        let view = resolved
            .program
            .program()
            .function_view(resolved.function, resolved.kind)
            .map_err(|error| indexed_error(error, call_span))?
            .ok_or_else(|| RuntimeError::new("unresolved-lowered-call", resolved.function.display_name()).with_span(call_span))?;
        let header = view.header().map_err(|error| indexed_error(error, call_span))?;
        let slots = self.evaluator.bind_lowered_values_owned(&header, values, call_span)?;
        let execution = FullExecution::for_function(resolved.program.clone(), resolved.index)
            .map_err(|error| indexed_error(error, call_span))?;
        self.push_call_with_header(
            resolved.function,
            resolved.kind,
            view,
            execution,
            symbol_owner,
            header,
            slots,
            call_span,
            return_to,
        )
    }

    fn push_call_with_slots(
        &mut self,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        slots: Vec<LoweredValue>,
        call_span: Span,
        return_to: Option<FrameContinuation>,
    ) -> Result<(), RuntimeError> {
        let view = self
            .program
            .function_view(function, kind)
            .map_err(|error| indexed_error(error, call_span))?
            .ok_or_else(|| {
                RuntimeError::new("unresolved-lowered-call", function.display_name())
                    .with_span(call_span)
            })?;
        let header = view
            .header()
            .map_err(|error| indexed_error(error, call_span))?;
        let execution = view
            .execution()
            .map_err(|error| indexed_error(error, call_span))?;
        self.push_call_with_header(function, kind, view, execution, None, header, slots, call_span, return_to)
    }

    fn push_call_with_header(
        &mut self,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        view: FullFunctionView<'_>,
        execution: FullExecution<'p>,
        symbol_owner: Option<crate::symbol::SymbolOwnerGuard>,
        header: Arc<FunctionHeader>,
        mut slots: Vec<LoweredValue>,
        call_span: Span,
        return_to: Option<FrameContinuation>,
    ) -> Result<(), RuntimeError> {
        if self.evaluator.call_stack.len() >= MAX_CALL_DEPTH {
            // The innermost calls name the cycle; the whole path would be
            // ten thousand lines of it.
            let open = &self.evaluator.call_stack;
            let mut path = open[open.len() - STACK_OVERFLOW_PATH_CALLS..]
                .iter()
                .map(|frame| frame.name.text())
                .collect::<Vec<_>>();
            path.push(function.display_name());
            return Err(RuntimeError::new(
                "stack-overflow",
                format!(
                    "more than {MAX_CALL_DEPTH} calls are open; the innermost are {}",
                    path.join(" -> ")
                ),
            )
            .with_span(call_span));
        }
        self.evaluator
            .hydrate_lowered_captures(&header, &mut slots, call_span)?;
        let (body, _) = view
            .body(&execution)
            .map_err(|error| indexed_error(error, call_span))?;
        let cursor = StatementCursor::function_body(&execution, body, call_span)?;
        let (frame_kind, enter_kind) = match kind {
            LoweredFunctionKind::Pure => (TracebackFrameKind::Pure, TraceKind::PureEnter),
            LoweredFunctionKind::Proc => (TracebackFrameKind::Proc, TraceKind::ProcEnter),
        };
        let definition_span = view
            .definition_span()
            .map_err(|error| indexed_error(error, call_span))?;
        // Tracing is off unless a tool turned it on, and rendering a function's
        // display name allocates; only do it when an event will use it.
        if self.evaluator.trace_enabled {
            let name = function.display_name();
            self.evaluator.trace_enter_with_definition(
                enter_kind,
                Some(call_span),
                Some(definition_span),
                Some(&name),
                TracePayload::None,
            );
        }
        // A call starts only once any earlier `Err` was handled.
        self.evaluator.pending_traceback = None;
        self.evaluator.call_stack.push(TracebackFrame {
            kind: frame_kind,
            name: function.traceback_name(),
            definition_span: Some(definition_span),
            call_span: Some(call_span),
        });
        let scope_id = self.evaluator.enter_owned_host_scope();
        let slot_scopes = self
            .evaluator
            .frame_scratch
            .take_slot_scopes(slots.len(), scope_id);
        self.calls.push(CallFrame {
            owner: FrameOwner::Function(function, kind),
            producer: false,
            cancelled: false,
            scope_id,
            execution,
            _symbol_owner: symbol_owner,
            slots: FrameSlots::Owned(slots),
            slot_scopes,
            call_span,
            definition_span,
            work: {
                let mut work = self.evaluator.frame_scratch.take_work();
                work.push(FrameWork::Statements {
                    cursor,
                    complete_call: true,
                    scope_id: None,
                });
                work
            },
            defers: Vec::new(),
            block_scopes: Vec::new(),
            block_defer_offsets: Vec::new(),
            return_to,
        });
        Ok(())
    }

    fn step(&mut self, index: usize) -> Result<(), RuntimeError> {
        if let Some(FrameWork::Statements { .. }) = self.calls[index].work.last() {
            return self.step_statements(index);
        }
        if matches!(self.calls[index].work.last(), Some(FrameWork::Discard(_))) {
            return self.step_discard(index);
        }
        if matches!(
            self.calls[index].work.last(),
            Some(FrameWork::CleanupGroup(_))
        ) {
            return self.step_cleanup_group(index);
        }
        let work = self.calls[index].work.pop().expect("indexed frame work");
        match work {
            FrameWork::ClearSlots(slots) => {
                for slot in slots {
                    self.calls[index].slots[slot] = LoweredValue::Unit;
                }
                Ok(())
            }
            FrameWork::CleanupGroup(_) | FrameWork::CleanupBoundary(_) | FrameWork::Discard(_) => {
                unreachable!("cleanup and discard work is stepped in place")
            }
            FrameWork::GuardFailureEnd(span) => {
                Err(RuntimeError::new("guard", "guard else block must diverge").with_span(span))
            }
            FrameWork::ExpressionBoundary { policy, next } => {
                if matches!(policy, ExpressionBoundaryPolicy::Context(_)) {
                    self.open_context_boundaries = self.open_context_boundaries.saturating_sub(1);
                }
                if self.calls[index].cancelled {
                    // A cancelled body replays only cleanup. Restore the scope
                    // this boundary opened and skip its continuation: feeding a
                    // placeholder value into body computation (selecting match
                    // arms, for one) can fail on a value the body never produced.
                    if let ExpressionBoundaryPolicy::Scope(restore) = policy {
                        self.evaluator.restore_indexed_context_scope(restore);
                    }
                    return Ok(());
                }
                let statement_scope = matches!(&next, FrameContinuation::StatementScopeEnd);
                let value = match policy {
                    ExpressionBoundaryPolicy::Capture => {
                        LoweredValue::ResultOk(Box::new(LoweredValue::Unit))
                    }
                    ExpressionBoundaryPolicy::Scope(restore) => {
                        self.evaluator.restore_indexed_context_scope(restore);
                        if statement_scope {
                            LoweredValue::Unit
                        } else {
                            lowered_result_ok(LoweredValue::Unit)
                        }
                    }
                    ExpressionBoundaryPolicy::Retry(state) => {
                        return self.finish_retry_attempt(
                            index,
                            state,
                            LoweredValue::Unit,
                            next,
                        );
                    }
                    _ => LoweredValue::Unit,
                };
                self.push_value(index, FrameValue::Value(value), next);
                Ok(())
            }
            FrameWork::CompCleanup(streams) => self.cleanup_comp_streams(streams),
            FrameWork::Statements { .. } => unreachable!("statement lists step in place"),
            FrameWork::Expr {
                instruction,
                span,
                next,
            } => self.eval_expr(index, instruction, span, next),
            FrameWork::Assertion(state) => self.step_assertion(index, state),
            FrameWork::Value { value, next } => self.continue_value(index, value, next),
            FrameWork::ForMap {
                target,
                mut cursor,
                body,
                span,
            } => {
                self.evaluator.service_pending_signal(span)?;
                if self.evaluator.shutting_down() {
                    return Ok(());
                }
                if let Some(item) = cursor.next() {
                    bind_lowered_comp_target(&target, item, &mut self.calls[index].slots, span)?;
                    self.calls[index].work.push(FrameWork::ForMap {
                        target,
                        cursor,
                        body,
                        span,
                    });
                    self.push_statement_block(index, body, span)?;
                }
                Ok(())
            }
            FrameWork::ForScalars {
                target,
                mut cursor,
                body,
                span,
            } => {
                self.evaluator.service_pending_signal(span)?;
                if self.evaluator.shutting_down() {
                    return Ok(());
                }
                if let Some(item) = cursor.next() {
                    bind_lowered_comp_target(&target, item, &mut self.calls[index].slots, span)?;
                    self.calls[index].work.push(FrameWork::ForScalars {
                        target,
                        cursor,
                        body,
                        span,
                    });
                    self.push_statement_block(index, body, span)?;
                }
                Ok(())
            }
            FrameWork::ForItems {
                target,
                items,
                index: item_index,
                body,
                span,
            } => self.step_for_items(index, target, items, item_index, body, span),
            FrameWork::ForStream {
                target,
                stream,
                body,
                span,
            } => self.step_for_stream(index, target, stream, body, span),
            FrameWork::ForPipeline {
                slot,
                pipeline,
                body,
                span,
            } => self.step_for_pipeline(index, slot, pipeline, body, span),
            FrameWork::StageValue {
                pipeline,
                span,
                next,
            } => self.step_stage_value(index, pipeline, span, next),
            FrameWork::ForStrLines {
                slot,
                text,
                cursor,
                line_count,
                body,
                span,
            } => self.step_for_str_lines(index, slot, text, cursor, line_count, body, span),
            FrameWork::While {
                condition,
                body,
                span,
            } => self.step_while(index, condition, body, span),
            FrameWork::Loop {
                body,
                span,
                completion,
            } => {
                if completion.is_some() {
                    self.evaluator.service_pending_signal(span)?;
                    if self.evaluator.shutting_down() {
                        self.push_value(
                            index,
                            FrameValue::Value(LoweredValue::Unit),
                            *completion.expect("expression loop has a continuation"),
                        );
                        return Ok(());
                    }
                }
                self.calls[index].work.push(FrameWork::Loop {
                    body,
                    span,
                    completion,
                });
                self.push_statement_block(index, body, span)
            }
            FrameWork::PatternWhile {
                condition,
                body,
                captures,
                span,
            } => {
                self.evaluator.service_pending_signal(span)?;
                if self.evaluator.shutting_down() {
                    return Ok(());
                }
                self.calls[index].work.push(FrameWork::PatternWhile {
                    condition,
                    body,
                    captures: captures.clone(),
                    span,
                });
                self.open_pattern_scope(index, captures);
                self.push_expr(
                    index,
                    condition,
                    span,
                    FrameContinuation::PatternWhile { body, span },
                );
                Ok(())
            }
            FrameWork::Finish(flow) => self.finish_deferred_call(index, flow),
            FrameWork::FinishError => self.finish_error_deferred_call(index),
        }
    }

    /// Runs the next statement of the list on top of the work stack. The list
    /// stays in place and comes off the stack once it is exhausted.
    fn step_statements(&mut self, index: usize) -> Result<(), RuntimeError> {
        if self.pending_error.is_none() {
            self.evaluator.rearm_swallowed_interruptions();
        }
        let signal = self
            .evaluator
            .service_pending_signal(self.calls[index].call_span);
        if signal.is_err() || self.evaluator.shutting_down() {
            // The block stays on the work stack so error unwinding closes its
            // owned scope before running function defers.
            signal?;
            return Err(RuntimeError::abort(
                self.evaluator.signal_state.shutdown_status.unwrap_or(3),
                self.evaluator.signal_state.shutdown_force,
            )
            .with_span(self.calls[index].call_span));
        }
        let statement = {
            let call = &mut self.calls[index];
            let Some(FrameWork::Statements { cursor, .. }) = call.work.last_mut() else {
                unreachable!("step_statements runs a statement list");
            };
            cursor.next(&call.execution, call.call_span)?
        };
        let Some(statement) = statement else {
            let Some(FrameWork::Statements {
                complete_call,
                scope_id,
                ..
            }) = self.calls[index].work.pop()
            else {
                unreachable!("step_statements runs a statement list");
            };
            return if complete_call {
                self.complete_call(index, StmtFlow::None)
            } else {
                if let Some(scope_id) = scope_id {
                    self.exit_block_scope(
                        index,
                        scope_id,
                        false,
                        true,
                        CleanupFailureResources::Retain,
                    )?;
                }
                Ok(())
            };
        };
        // A block frame running top-level statements publishes script
        // bindings between them, as the statements' own reads expect.
        if let FrameOwner::Block { .. } = self.calls[index].owner {
            let call = &mut self.calls[index];
            self.evaluator
                .sync_indexed_root_slots(&mut call.slots, call.call_span)?;
        }
        self.eval_statement(index, statement)
    }

    fn step_scan_lines(
        &mut self,
        index: usize,
        tag: FullTag,
        payload: FullPayload<'_>,
    ) -> Result<(), RuntimeError> {
        let call_span = self.calls[index].call_span;
        let call = &mut self.calls[index];
        let StmtView::ScanLines {
            text_slot,
            line_slot,
            mut checks,
            span,
        } = StmtView::decode(&call.execution, tag, payload)
            .map_err(|error| indexed_error(error, call_span))?
        else {
            unreachable!("statement tag and borrowed row agree")
        };
        let check_start = checks.position();
        while let Some(check) = checks.next() {
            check.map_err(|error| indexed_error(error, span))?;
        }
        checks
            .resume(check_start)
            .map_err(|error| indexed_error(error, span))?;

        let (source, start, end) = if let Some((text, start, end)) =
            lowered_str_parts(&call.slots[text_slot])
        {
            (ScanLinesSource::Text(text), start, end)
        } else if let Some((bytes, start, end)) = lowered_bytes_parts(&call.slots[text_slot]) {
            (ScanLinesSource::Bytes(bytes), start, end)
        } else {
            return Err(RuntimeError::new("type-error", "ScanLines expected Str or Bytes")
                .with_span(span));
        };
        let bytes = source.bytes();
        let mut cursor = start;
        let mut line_count = 0u32;
        while cursor < end {
            let newline = memchr::memchr(b'\n', &bytes[cursor..end]).map(|offset| cursor + offset);
            let line_end = newline.unwrap_or(end);
            let view_end = if line_end > cursor && bytes[line_end - 1] == b'\r' {
                line_end - 1
            } else {
                line_end
            };
            line_count = line_count.wrapping_add(1);
            if line_count & 63 == 0 {
                self.evaluator.service_pending_signal(span)?;
                if self.evaluator.shutting_down() {
                    return Ok(());
                }
            }
            match &source {
                ScanLinesSource::Text(text) => {
                    assign_lowered_str_view(&mut call.slots[line_slot], text, cursor, view_end);
                }
                ScanLinesSource::Bytes(bytes) => {
                    assign_lowered_bytes_view(&mut call.slots[line_slot], bytes, cursor, view_end);
                }
            }
            checks
                .resume(check_start)
                .map_err(|error| indexed_error(error, span))?;
            while let Some(check) = checks.next() {
                let check = check.map_err(|error| indexed_error(error, span))?;
                let matches = match check.condition {
                    ScanConditionView::TrimEmpty => {
                        lowered_trim_is_empty_value(&call.slots[line_slot], span)?
                    }
                    ScanConditionView::TrimStartsWith(needle) => lowered_trim_str_predicate_value(
                        &call.slots[line_slot],
                        LoweredStrPredicate::StartsWith,
                        needle,
                        span,
                    )?,
                    ScanConditionView::StartsWith(needle) => lowered_str_predicate_text(
                        &call.slots[line_slot],
                        LoweredStrPredicate::StartsWith,
                        needle,
                        span,
                    )?,
                };
                if matches {
                    if let LoweredValue::Int(ref mut value) = call.slots[check.counter_slot] {
                        *value += 1;
                    }
                    break;
                }
            }
            let Some(newline) = newline else {
                break;
            };
            cursor = newline + 1;
        }
        call.slots[line_slot] = LoweredValue::Unit;
        Ok(())
    }

    fn step_scan_bytes(
        &mut self,
        index: usize,
        tag: FullTag,
        payload: FullPayload<'_>,
    ) -> Result<(), RuntimeError> {
        let call_span = self.calls[index].call_span;
        let call = &mut self.calls[index];
        let StmtView::ScanBytes { config } = StmtView::decode(&call.execution, tag, payload)
            .map_err(|error| indexed_error(error, call_span))?
        else {
            unreachable!("statement tag and borrowed row agree")
        };
        let Some((line, start, end)) = lowered_bytes_parts(&call.slots[config.line_slot]) else {
            return Err(RuntimeError::new("type-error", "ScanBytes expected Bytes")
                .with_span(config.span));
        };
        let bytes = &line[start..end];
        let mut block_depth = match call.slots[config.block_depth_slot] {
            LoweredValue::Int(value) => value,
            _ => {
                return Err(RuntimeError::new(
                    "type-error",
                    "ScanBytes block depth expected Int",
                )
                .with_span(config.span));
            }
        };
        let mut index = 0usize;
        let mut code_seen = false;
        let mut comment_seen = false;
        let mut in_string = false;
        let mut string_delim = -1i64;
        let mut escaped = false;
        while index < bytes.len() {
            if index & 4095 == 0 {
                self.evaluator.service_pending_signal(config.span)?;
                if self.evaluator.shutting_down() {
                    return Ok(());
                }
            }
            let byte = i64::from(bytes[index]);
            let next_byte = bytes.get(index + 1).copied().map(i64::from).unwrap_or(-1);
            if block_depth > 0 {
                comment_seen = true;
                if config.nested && byte == 47 && next_byte == 42 {
                    block_depth += 1;
                    index += 2;
                } else if byte == 42 && next_byte == 47 {
                    block_depth -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
            } else if in_string {
                code_seen = true;
                if escaped {
                    escaped = false;
                } else if byte == 92 {
                    escaped = true;
                } else if byte == string_delim {
                    in_string = false;
                }
                index += 1;
            } else if byte == 34 || byte == 39 || byte == 96 {
                code_seen = true;
                in_string = true;
                string_delim = byte;
                index += 1;
            } else if byte == 47 && next_byte == 47 {
                comment_seen = true;
                index = bytes.len();
            } else if byte == 47 && next_byte == 42 {
                comment_seen = true;
                block_depth = 1;
                index += 2;
            } else {
                if byte != 32 && byte != 9 {
                    code_seen = true;
                }
                index += 1;
            }
        }
        call.slots[config.block_depth_slot] = LoweredValue::Int(block_depth);
        call.slots[config.code_seen_slot] = LoweredValue::Bool(code_seen);
        call.slots[config.comment_seen_slot] = LoweredValue::Bool(comment_seen);
        call.slots[config.in_string_slot] = LoweredValue::Bool(in_string);
        call.slots[config.string_delim_slot] = LoweredValue::Int(string_delim);
        call.slots[config.escaped_slot] = LoweredValue::Bool(escaped);
        Ok(())
    }

    fn eval_statement(&mut self, index: usize, instruction: u32) -> Result<(), RuntimeError> {
        // A statement starts only once any earlier `Err` was handled.
        self.evaluator.pending_traceback = None;
        let execution = self.calls[index].execution.thread_local();
        let transfers_resource = execution.transfers_resource(instruction);
        let span = self.calls[index].call_span;
        let (tag, payload) = indexed_value(
            execution.instruction_id(instruction),
            span,
        )?;
        match tag {
            FullTag::StmtPrint => {
                let StmtView::Print {
                    args,
                    stderr,
                    flush,
                    propagate_result,
                    span,
                } = StmtView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("statement tag and borrowed row agree")
                };
                let position = args.position();
                let argv = if self.evaluator.trace_enabled {
                    Vec::with_capacity(args.len())
                } else {
                    Vec::new()
                };
                self.step_print(
                    index,
                    FramePrintState {
                        statement: instruction,
                        position,
                        stderr,
                        flush,
                        propagate_result,
                        span,
                        line: String::new(),
                        argv,
                    },
                )
            }
            FullTag::StmtCd => {
                let StmtView::Cd { target, body, span } =
                    StmtView::decode(&execution, tag, payload)
                        .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("statement tag and borrowed row agree")
                };
                self.push_expr(
                    index,
                    target.raw(),
                    span,
                    FrameContinuation::ContextScopeEntry {
                        kind: crate::syntax::arena::ContextScopeKind::Cwd,
                        body: body.block_id(),
                        span,
                        next: Box::new(FrameContinuation::StatementScopeEnd),
                    },
                );
                Ok(())
            }
            FullTag::StmtEnv => {
                let StmtView::Env { env, body } =
                    StmtView::decode(&execution, tag, payload)
                        .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("statement tag and borrowed row agree")
                };
                let position = env.position();
                for assignment in env {
                    let RunEnvView { name, .. } =
                        assignment.map_err(|error| indexed_error(error, span))?;
                    check_env_name(&name.as_str(), span)?;
                }
                self.step_env(
                    index,
                    FrameEnvState {
                        statement: instruction,
                        position,
                        overlay: BTreeMap::new(),
                        body: body.block_id(),
                        span,
                    },
                )
            }
            FullTag::StmtRun => {
                let StmtView::Run {
                    value,
                    propagate_result,
                } = StmtView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("run statement tag has a run view")
                };
                self.push_expr(
                    index,
                    value.raw(),
                    span,
                    FrameContinuation::StatementRun {
                        propagate_result,
                        call_span: span,
                    },
                );
                Ok(())
            }
            FullTag::StmtAssert => {
                let StmtView::Assert {
                    value,
                    message,
                    span: assertion_span,
                } = StmtView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("assert statement tag has an assertion view")
                };
                let condition = value.raw();
                let message = message.map(|value| value.raw());
                self.calls[index].work.push(FrameWork::Assertion(FrameAssertionState {
                    condition,
                    message,
                    span: assertion_span,
                    work: vec![super::AssertionWork::Expr(condition)],
                    passed: true,
                    reached: None,
                    failure: None,
                }));
                Ok(())
            }
            FullTag::StmtDefaultParameter => {
                let StmtView::DefaultParameter {
                    slot,
                    value,
                    kind,
                    check,
                    span: value_span,
                } = StmtView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("default-parameter tag has a typed view")
                };
                let kind = kind
                    .materialize_lowered()
                    .map_err(|error| indexed_error(error, value_span))?;
                let check = check
                    .map(|check| super::indexed_type_check(check, value_span))
                    .transpose()?;
                if matches!(self.calls[index].slots[slot], LoweredValue::OmittedArgument) {
                    self.push_expr(
                        index,
                        value.raw(),
                        value_span,
                        FrameContinuation::ParameterDefault {
                            slot,
                            kind,
                            check,
                            span: value_span,
                        },
                    );
                }
                Ok(())
            }
            FullTag::StmtLet => {
                let StmtView::Let { slot, value } = StmtView::decode(
                    &execution,
                    tag,
                    payload,
                )
                .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("let statement tag has a let view")
                };
                self.push_expr(index, value.raw(), span, FrameContinuation::Store(slot));
                Ok(())
            }
            FullTag::StmtLetRecord => {
                let StmtView::LetRecord {
                    source,
                    target,
                    span: value_span,
                } = StmtView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("record binding tag has a typed view")
                };
                let target = target
                    .materialize()
                    .map_err(|error| indexed_error(error, value_span))?;
                self.declare_target(index, &target);
                self.push_expr(
                    index,
                    source.raw(),
                    span,
                    FrameContinuation::StoreTarget {
                        target,
                        span: value_span,
                    },
                );
                Ok(())
            }
            FullTag::StmtWith => {
                let StmtView::With {
                    mut bindings,
                    body,
                    else_param_slot,
                    else_body,
                    captures,
                    span,
                } = StmtView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("with statement tag has a with view")
                };
                bindings
                    .preflight()
                    .map_err(|error| indexed_error(error, span))?;
                let captures = captures
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| indexed_error(error, span))?;
                let (first_binding, binding_position) = statement_with_binding(
                    &execution,
                    instruction,
                    bindings.position(),
                    span,
                )?;
                let body = body.block_id();
                let else_body = else_body.block_id();
                let scope_id = self.evaluator.enter_owned_host_scope();
                let defer_offset = self.calls[index].defers.len();
                self.calls[index].block_scopes.push(scope_id);
                self.calls[index].block_defer_offsets.push(defer_offset);
                self.calls[index].work.push(FrameWork::Statements {
                    cursor: StatementCursor::empty(),
                    complete_call: false,
                    scope_id: Some(scope_id),
                });
                self.calls[index].work.push(FrameWork::ClearSlots(captures));
                if let Some((slot, value)) = first_binding {
                    self.push_expr(
                        index,
                        value,
                        span,
                        FrameContinuation::WithBinding {
                            instruction,
                            position: binding_position,
                            slot,
                            body,
                            else_param_slot,
                            else_body,
                            span,
                        },
                    );
                    Ok(())
                } else {
                    self.push_statement_block(index, body, span)
                }
            }
            FullTag::StmtGuard => {
                let StmtView::Guard {
                    target,
                    value,
                    else_param_slot,
                    else_body,
                    span,
                    optional,
                } = StmtView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("guard statement tag has a guard view")
                };
                let target = target
                    .materialize()
                    .map_err(|error| indexed_error(error, span))?;
                self.push_expr(
                    index,
                    value.raw(),
                    span,
                    FrameContinuation::GuardInput {
                        target,
                        else_param_slot,
                        else_body: else_body.block_id(),
                        span,
                        optional,
                    },
                );
                Ok(())
            }

            FullTag::StmtLetInt => {
                let StmtView::LetInt { slot, value } = StmtView::decode(
                    &execution,
                    tag,
                    payload,
                )
                .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("integer let statement tag has an integer let view")
                };
                self.push_expr(index, value.raw(), span, FrameContinuation::Store(slot));
                Ok(())
            }
            FullTag::StmtLetBool => {
                let StmtView::LetBool { slot, value } = StmtView::decode(
                    &execution,
                    tag,
                    payload,
                )
                .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("boolean let statement tag has a boolean let view")
                };
                self.push_expr(index, value.raw(), span, FrameContinuation::Store(slot));
                Ok(())
            }
            FullTag::StmtAssign => {
                let StmtView::Assign {
                    slot,
                    op,
                    value,
                    check,
                    span: value_span,
                } = StmtView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("assignment statement tag has an assignment view")
                };
                let check = check
                    .map(|check| {
                        Ok(LoweredTypeCheck {
                            ty: check.ty.materialize()?,
                            name: Arc::from(check.name),
                            schema: check.schema.map(Arc::clone),
                        })
                    })
                    .transpose()
                    .map_err(|error| indexed_error(error, span))?;
                let value = value.raw();
                let (value, singleton) = indexed_assignment_operand(
                    &execution,
                    value,
                    op,
                    value_span,
                )?;
                self.push_expr(
                    index,
                    value,
                    value_span,
                    FrameContinuation::Assign {
                        slot,
                        op,
                        singleton,
                        check,
                        span: value_span,
                        transfers_resource,
                    },
                );
                Ok(())
            }
            FullTag::StmtAssignField | FullTag::StmtAssignFieldInt => {
                let execution = &execution;
                let view = StmtView::decode(execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?;
                let (slot, field, op, value, value_span) = match view {
                    StmtView::AssignField {
                        slot,
                        field,
                        op,
                        value,
                        span,
                    } => (slot, field, op, value.raw(), span),
                    StmtView::AssignFieldInt {
                        slot,
                        field,
                        op,
                        value,
                        span,
                    } => (slot, field, op, value.raw(), span),
                    _ => unreachable!("field assignment tag has a field assignment view"),
                };
                let (value, singleton) = indexed_assignment_operand(execution, value, op, value_span)?;
                self.advance_assign_path(index, AssignPathState {
                    slot,
                    path: AssignPathCursor::DirectField(Name::intern(field)),
                    selectors: Vec::new(),
                    op,
                    value,
                    singleton,
                    check: None,
                    span: value_span,
                    transfers_resource,
                })?;
                Ok(())
            }
            FullTag::StmtAssignInt => {
                let StmtView::AssignInt {
                    slot,
                    op,
                    value,
                    span: value_span,
                } = StmtView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("integer assignment tag has an integer assignment view")
                };
                self.push_expr(
                    index,
                    value.raw(),
                    value_span,
                    FrameContinuation::Assign {
                        slot,
                        op,
                        singleton: false,
                        check: None,
                        span: value_span,
                        transfers_resource,
                    },
                );
                Ok(())
            }
            FullTag::StmtAssignBool => {
                let StmtView::AssignBool { slot, value } = StmtView::decode(
                    &execution,
                    tag,
                    payload,
                )
                .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("boolean assignment tag has a boolean assignment view")
                };
                self.push_expr(
                    index,
                    value.raw(),
                    span,
                    FrameContinuation::Assign {
                        slot,
                        op: AssignOp::Set,
                        singleton: false,
                        check: None,
                        span,
                        transfers_resource,
                    },
                );
                Ok(())
            }
            FullTag::StmtAssignPath => {
                let view = StmtView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?;
                let (slot, mut path, op, value, check, value_span) = match view {
                    StmtView::AssignPath {
                        slot,
                        path,
                        op,
                        value,
                        check,
                        span,
                    } => (slot, path, op, value.raw(), check, span),
                    _ => unreachable!("assignment path tag has an assignment path view"),
                };
                path.preflight()
                    .map_err(|error| indexed_error(error, value_span))?;
                let position = path.position();
                let check = check
                    .map(|check| {
                        Ok(LoweredTypeCheck {
                            ty: check.ty.materialize()?,
                            name: Arc::from(check.name),
                            schema: check.schema.map(Arc::clone),
                        })
                    })
                    .transpose()
                    .map_err(|error| indexed_error(error, value_span))?;
                let (value, singleton) =
                    indexed_assignment_operand(
                        &execution,
                        value,
                        op,
                        value_span,
                    )?;
                self.advance_assign_path(
                    index,
                    AssignPathState {
                        slot,
                        path: AssignPathCursor::Encoded {
                            statement: instruction,
                            position,
                        },
                        selectors: Vec::new(),
                        op,
                        value,
                        singleton,
                        check,
                        span: value_span,
                        transfers_resource,
                    },
                )?;
                Ok(())
            }
            FullTag::StmtExpr => {
                let StmtView::Expr { value, span: value_span } = StmtView::decode(
                    &execution,
                    tag,
                    payload,
                )
                .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("expression statement tag has an expression statement view")
                };
                self.push_expr(
                    index,
                    value.raw(),
                    value_span,
                    FrameContinuation::Discard(value_span),
                );
                Ok(())
            }
            FullTag::StmtIf | FullTag::StmtIfBool => {
                let view = StmtView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?;
                let (position, else_body) = match view {
                    StmtView::If {
                        mut branches,
                        else_body,
                    } => {
                        branches
                            .preflight()
                            .map_err(|error| indexed_error(error, span))?;
                        (
                            StatementIfPosition::Expr(branches.position()),
                            else_body.map(|body| body.block_id()),
                        )
                    }
                    StmtView::IfBool {
                        mut branches,
                        else_body,
                    } => {
                        branches
                            .preflight()
                            .map_err(|error| indexed_error(error, span))?;
                        (
                            StatementIfPosition::Bool(branches.position()),
                            else_body.map(|body| body.block_id()),
                        )
                    }
                    _ => unreachable!("if statement tags have an if view"),
                };
                let (branch, position) = statement_if_branch(
                    &execution,
                    instruction,
                    position,
                    span,
                )?;
                if let Some((condition, body)) = branch {
                    self.push_expr(
                        index,
                        condition,
                        span,
                        FrameContinuation::StatementIf {
                            instruction,
                            position,
                            body,
                            else_body,
                            span,
                        },
                    );
                } else {
                    if let Some(body) = else_body {
                        self.push_statement_block(index, body, span)?;
                    }
                }
                Ok(())
            }
            // Pattern conditionals run on the frame, like `if`, so a `yield`
            // in a branch suspends the producer that lexically contains it.
            FullTag::StmtPatternIf => {
                let StmtView::PatternIf {
                    mut branches,
                    else_body,
                    span,
                } = StmtView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("pattern-if statement tag has a pattern-if view")
                };
                branches
                    .preflight()
                    .map_err(|error| indexed_error(error, span))?;
                self.start_pattern_branch(
                    index,
                    instruction,
                    branches.position(),
                    else_body.map(|body| body.block_id()),
                    span,
                )
            }
            FullTag::StmtPatternWhile => {
                let StmtView::PatternWhile {
                    condition,
                    body,
                    captures,
                    span,
                } = StmtView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("pattern-while statement tag has a pattern-while view")
                };
                let captures = captures
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| indexed_error(error, span))?;
                self.calls[index].work.push(FrameWork::PatternWhile {
                    condition: condition.raw(),
                    body: body.block_id(),
                    captures,
                    span,
                });
                Ok(())
            }
            FullTag::StmtScanLines => self.step_scan_lines(index, tag, payload),
            FullTag::StmtScanBytes => self.step_scan_bytes(index, tag, payload),
            FullTag::StmtFor | FullTag::StmtForRecord => {
                let view = StmtView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?;
                let (target, iter, body, value_span) = match view {
                    StmtView::For {
                        slot,
                        iter,
                        body,
                        span,
                    } => (LoweredCompTarget::Slot(slot), iter.raw(), body.block_id(), span),
                    StmtView::ForRecord {
                        target,
                        iter,
                        body,
                        span,
                    } => (
                        target
                            .materialize()
                            .map_err(|error| indexed_error(error, span))?,
                        iter.raw(),
                        body.block_id(),
                        span,
                    ),
                    _ => unreachable!("for statement tags have a for view"),
                };
                self.declare_target(index, &target);
                // Only a single loop variable pulls a serial pipeline row by row.
                if let LoweredCompTarget::Slot(slot) = target
                    && let Some((input, pipeline_span, stages)) =
                        indexed_for_pipeline_input(&execution, iter, value_span)?
                {
                    self.push_expr(
                        index,
                        input,
                        pipeline_span,
                        FrameContinuation::ForPipelineInput {
                            slot,
                            body,
                            span: pipeline_span,
                            stages,
                        },
                    );
                    return Ok(());
                }
                self.push_expr(
                    index,
                    iter,
                    value_span,
                    FrameContinuation::ForItems {
                        target,
                        body,
                        span: value_span,
                    },
                );
                Ok(())
            }
            FullTag::StmtForStrLines => {
                let StmtView::ForStrLines {
                    slot,
                    text,
                    body,
                    span: value_span,
                } = StmtView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("for-lines statement tag has a for-lines view")
                };
                self.push_expr(
                    index,
                    text.raw(),
                    value_span,
                    FrameContinuation::ForStrLines {
                        slot,
                        body: body.block_id(),
                        span: value_span,
                    },
                );
                Ok(())
            }
            FullTag::StmtWhile | FullTag::StmtWhileBool => {
                let (condition, body) = match StmtView::decode(
                    &execution,
                    tag,
                    payload,
                )
                .map_err(|error| indexed_error(error, span))?
                {
                    StmtView::While { condition, body } => {
                        (condition.raw(), body.block_id())
                    }
                    StmtView::WhileBool { condition, body } => {
                        (condition.raw(), body.block_id())
                    }
                    _ => unreachable!("while statement tags have a while view"),
                };
                self.calls[index].work.push(FrameWork::While {
                    condition,
                    body,
                    span,
                });
                Ok(())
            }
            FullTag::StmtMatch => {
                let StmtView::Match {
                    value,
                    mut arms,
                    span: value_span,
                } = StmtView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("match statement tag has a match view")
                };
                arms.preflight()
                    .map_err(|error| indexed_error(error, value_span))?;
                self.push_expr(
                    index,
                    value.raw(),
                    value_span,
                    FrameContinuation::MatchValue {
                        instruction,
                        position: arms.position(),
                        span: value_span,
                    },
                );
                Ok(())
            }
            FullTag::StmtStrMatch | FullTag::StmtTagMatch => {
                let view = StmtView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?;
                let (value, match_span) = match view {
                    StmtView::StrMatch { value, span, .. }
                    | StmtView::TagMatch { value, span, .. } => (value.raw(), span),
                    _ => unreachable!("string/tag statement match tag has a matching borrowed view"),
                };
                self.push_expr(
                    index,
                    value,
                    match_span,
                    FrameContinuation::StatementStrTagMatch {
                        instruction,
                        span: match_span,
                    },
                );
                Ok(())
            }
            FullTag::StmtLoop => {
                let StmtView::Loop { body } = StmtView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("loop statement tag has a typed view")
                };
                self.calls[index].work.push(FrameWork::Loop {
                    body: body.block_id(),
                    span,
                    completion: None,
                });
                Ok(())
            }
            FullTag::StmtBreak => {
                let StmtView::Break = StmtView::decode(
                    &execution,
                    tag,
                    payload,
                )
                .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("break statement tag has a break view")
                };
                self.break_loop(index, None, false)
            }
            FullTag::StmtBreakValue => {
                let StmtView::BreakValue { value } = StmtView::decode(
                    &execution,
                    tag,
                    payload,
                )
                .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("break-value statement tag has a break-value view")
                };
                self.push_expr(
                    index,
                    value.raw(),
                    span,
                    FrameContinuation::BreakLoop { transfers_resource },
                );
                Ok(())
            }
            FullTag::StmtContinue => {
                let StmtView::Continue = StmtView::decode(
                    &execution,
                    tag,
                    payload,
                )
                .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("continue statement tag has a continue view")
                };
                self.continue_loop(index)
            }
            FullTag::StmtYield | FullTag::StmtYieldDelegate => {
                let view = StmtView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?;
                let (value, yield_span, delegate) = match view {
                    StmtView::Yield { value } => (value.raw(), span, false),
                    StmtView::YieldDelegate { value, span } => (value.raw(), span, true),
                    _ => unreachable!("yield statement tag has a yield view"),
                };
                if !self.calls[index].producer {
                    return Err(
                        RuntimeError::new("control-flow", "yield outside stream producer")
                            .with_span(yield_span),
                    );
                }
                let continuation = if delegate {
                    FrameContinuation::YieldDelegate {
                        span: yield_span,
                        transfers_resource,
                    }
                } else {
                    FrameContinuation::Yield { transfers_resource }
                };
                self.push_expr(index, value, yield_span, continuation);
                Ok(())
            }
            FullTag::StmtDefer => {
                let StmtView::Defer { value, on_error } = StmtView::decode(
                    &execution,
                    tag,
                    payload,
                )
                .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("defer tag has a defer view")
                };
                self.calls[index]
                    .defers
                    .push(RegisteredDefer::new(value.raw(), on_error));
                Ok(())
            }
            FullTag::StmtValue => {
                let StmtView::Value { value } = StmtView::decode(
                    &execution,
                    tag,
                    payload,
                )
                .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("value statement tag has a value view")
                };
                self.push_expr(
                    index,
                    value.raw(),
                    span,
                    FrameContinuation::BlockValue { transfers_resource },
                );
                Ok(())
            }
            FullTag::StmtReturn => {
                let StmtView::Return { value } = StmtView::decode(
                    &execution,
                    tag,
                    payload,
                )
                .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("return statement tag has a return view")
                };
                self.push_expr(
                    index,
                    value.raw(),
                    span,
                    FrameContinuation::Return { transfers_resource },
                );
                Ok(())
            }
            _ => Err(frame_strategy_error(tag, span)),
        }
    }

    fn push_apply_operands(
        &mut self,
        index: usize,
        instruction: u32,
        tag: FullTag,
        first: Option<super::FrameApplyChild>,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        if let Some(first) = first {
            self.push_expr(
                index,
                first.id,
                first.span,
                FrameContinuation::ApplyOperands {
                    instruction,
                    tag,
                    position: 0,
                    tag_position: first.tag_position,
                    error_field_position: first.error_field_position,
                    values: Vec::new(),
                    span: first.span,
                    next: Box::new(next),
                },
            );
        } else {
            let execution = self.calls[index].execution.thread_local();
            let value = super::indexed_frame_apply_value(
                &mut self.evaluator,
                &execution,
                instruction,
                Vec::new(),
                span,
            )?;
            self.push_value(index, FrameValue::Value(value), next);
        }
        Ok(())
    }

    fn eval_expr(
        &mut self,
        index: usize,
        instruction: u32,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        let execution = self.calls[index].execution.thread_local();
        let in_place = !self.evaluator.force_frames
            && execution.is_in_place(instruction);
        let (tag, payload) = indexed_value(
            execution.instruction_id(instruction),
            span,
        )?;
        if in_place {
            let flow = {
                let call = &mut self.calls[index];
                self.evaluator.eval_indexed_expr(
                    &call.execution,
                    instruction,
                    &mut call.slots,
                    span,
                )?
            };
            match flow {
                ControlFlow::Continue(value) => {
                    self.push_value(index, FrameValue::Value(value), next);
                }
                ControlFlow::Break(value)
                    if matches!(&value, LoweredValue::ResultErr(_))
                        && matches!(self.evaluator.pending_value_block_flow, None | Some(StmtFlow::Propagate(_)))
                        && with_initializer_handler(&next).is_some() =>
                {
                    // In-place propagation must still reach the active With initializer handler.
                    self.push_value(index, FrameValue::Break(value), next);
                }
                ControlFlow::Break(value) => self.complete_expression_escape(index, value)?,
            }
            return Ok(());
        }
        if let Some(sink) = &self.evaluator.route_witness_sink
            && let Some(route) = frame_route(tag, self.evaluator.force_frames)
        {
            sink.record(
                tag,
                route,
                if self.evaluator.force_frames {
                    RouteWitnessMode::Forced
                } else {
                    RouteWitnessMode::Normal
                },
            );
        }
        let strategy = tag.frame_strategy();
        if tag.is_eager_atom() {
            let call = &mut self.calls[index];
            let value = super::indexed_eager_atom_value(
                &call.execution,
                tag,
                payload,
                &mut call.slots,
                span,
            )?;
            self.push_value(index, FrameValue::Value(value), next);
            return Ok(());
        }
        if strategy == FrameStrategy::FrameApply
            && (super::is_typed_int_tag(tag) || super::is_typed_bool_tag(tag))
        {
            return self.eval_typed_scalar_expr(
                index,
                instruction,
                tag,
                &execution,
                payload,
                span,
                next,
            );
        }
        if super::indexed_frame_apply_tag(tag) {
            let mut view = super::ExprView::decode(&execution, tag, payload)
                .map_err(|error| indexed_error(error, span))?;
            let first = super::indexed_frame_apply_child(tag, &mut view, 0, None, None, span)?;
            let apply_span = first.as_ref().map_or(span, |child| child.span);
            self.push_apply_operands(index, instruction, tag, first, apply_span, next)?;
            return Ok(());
        }
        let view_kind = tag
            .expr_view_kind()
            .ok_or_else(|| frame_strategy_error(tag, span))?;
        self.eval_expr_view(index, instruction, tag, view_kind, &execution, payload, span, next)
    }

    fn eval_expr_view<'e>(
        &mut self,
        index: usize,
        instruction: u32,
        tag: FullTag,
        view_kind: ExprViewKind,
        execution: &'e FullExecution<'_>,
        payload: FullPayload<'e>,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        match view_kind {
            ExprViewKind::RunCapture | ExprViewKind::SpawnRun | ExprViewKind::RunPipeline => {
                self.start_process_run(index, instruction, tag, span, next)?;
            }
            ExprViewKind::SpawnCommand => {
                let super::ExprView::SpawnCommand { command, span } =
                    super::ExprView::decode(&execution, tag, payload)
                        .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("spawn-command tag has a command view")
                };
                self.push_expr(
                    index,
                    command.raw(),
                    span,
                    FrameContinuation::SpawnCommand {
                        span,
                        next: Box::new(next),
                    },
                );
            }
            ExprViewKind::Wait => {
                let super::ExprView::Wait { target, span } =
                    super::ExprView::decode(&execution, tag, payload)
                        .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("wait tag has a wait view")
                };
                self.push_expr(
                    index,
                    target.raw(),
                    span,
                    FrameContinuation::Wait {
                        span,
                        next: Box::new(next),
                    },
                );
            }
            ExprViewKind::ProcessCommandBuilder => {
                let (entries, builder_span) = process_command_builder_view(
                    &execution,
                    instruction,
                    span,
                )?;
                self.evaluator.trace_enter(
                    TraceKind::ModuleCall,
                    Some(builder_span),
                    Some("process.command"),
                    TracePayload::None,
                );
                let state = Box::new(ProcessCommandBuilderState::new(span, builder_span));
                self.step_process_command_builder(
                    index,
                    instruction,
                    entries.position(),
                    state,
                    next,
                )?;
            }
            ExprViewKind::ListPipeline => {
                let (input, pipeline_span, stages) = indexed_value_pipeline_input(
                    &execution,
                    instruction,
                    span,
                )?;
                self.push_expr(
                    index,
                    input,
                    pipeline_span,
                    FrameContinuation::StageValueInput {
                        span: pipeline_span,
                        stages,
                        next: Box::new(next),
                    },
                );
            }
            ExprViewKind::ValueBlock => {
                let super::ExprView::ValueBlock { body, span } =
                    super::ExprView::decode(&execution, tag, payload)
                        .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("value-block tag has a value-block view")
                };
                self.calls[index].work.push(FrameWork::ExpressionBoundary {
                    policy: ExpressionBoundaryPolicy::Value,
                    next,
                });
                self.push_statement_block(index, body.block_id(), span)?;
            }
            ExprViewKind::Loop => {
                let super::ExprView::Loop {
                    body,
                    span: loop_span,
                } = super::ExprView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("loop tag has a loop view")
                };
                self.calls[index].work.push(FrameWork::Loop {
                    body: body.block_id(),
                    span: loop_span,
                    completion: Some(Box::new(next)),
                });
            }
            ExprViewKind::Retry => {
                let super::ExprView::Retry {
                    delays,
                    pattern,
                    body,
                    span: retry_span,
                    schedule,
                } = super::ExprView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("retry tag has a typed retry view")
                };
                let delay_position = delays.position();
                let delay_count = delays.len();
                self.eval_retry_delay(
                    index,
                    Box::new(RetryFrameState {
                        instruction,
                        delay_position,
                        pattern: pattern.map(|pattern| pattern.raw()),
                        body: body.block_id(),
                        span: retry_span,
                        schedule_kind: schedule,
                        delay_values: Vec::with_capacity(delay_count),
                        delays: None,
                        attempt_index: 0,
                        max_attempts: 0,
                        last_error: None,
                        last_traceback: None,
                    }),
                    Box::new(next),
                )?;
            }
            ExprViewKind::Capture => {
                let super::ExprView::Capture { body, span } =
                    super::ExprView::decode(&execution, tag, payload)
                        .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("capture tag has a typed capture view")
                };
                self.calls[index].work.push(FrameWork::ExpressionBoundary {
                    policy: ExpressionBoundaryPolicy::Capture,
                    next,
                });
                self.push_statement_block(index, body.block_id(), span)?;
            }
            ExprViewKind::ContextScope => {
                let super::ExprView::ContextScope {
                    kind,
                    input,
                    body,
                    span: context_span,
                } = super::ExprView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("context-scope tag has a typed view")
                };
                self.push_expr(
                    index,
                    input.raw(),
                    context_span,
                    FrameContinuation::ContextScopeEntry {
                        kind,
                        body: body.block_id(),
                        span: context_span,
                        next: Box::new(next),
                    },
                );
            }
            ExprViewKind::ErrorContext => {
                let super::ExprView::ErrorContext {
                    message,
                    body,
                    span: context_span,
                } = super::ExprView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("error-context tag has an error-context view")
                };
                self.push_expr(
                    index,
                    message.raw(),
                    context_span,
                    FrameContinuation::ErrorContextEntry {
                        body: body.block_id(),
                        span: context_span,
                        next: Box::new(next),
                    },
                );
            }
            ExprViewKind::ComparisonChain => {
                let (first, pairs) = decode_comparison_chain(
                    &execution,
                    tag,
                    payload,
                    span,
                )?;
                self.push_expr(
                    index,
                    first,
                    span,
                    FrameContinuation::ComparisonLeft {
                        instruction,
                        pairs: pairs.position(),
                        next: Box::new(next),
                    },
                );
            }
            ExprViewKind::Binary => {
                let super::ExprView::Binary {
                    op,
                    left,
                    right,
                    span: value_span,
                } = super::ExprView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("binary tag has a binary view")
                };
                self.push_expr(
                    index,
                    left.raw(),
                    value_span,
                    FrameContinuation::BinaryLeft {
                        op,
                        right: right.raw(),
                        span: value_span,
                        next: Box::new(next),
                    },
                );
            }
            ExprViewKind::IfExpr => {
                let super::ExprView::IfExpr {
                    branches,
                    else_value,
                    span: value_span,
                } = super::ExprView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("if-expression tag has an if-expression view")
                };
                self.start_if_expr(
                    index,
                    instruction,
                    branches.position(),
                    else_value.raw(),
                    value_span,
                    span,
                    Box::new(next),
                )?;
            }
            ExprViewKind::MatchExpr => {
                let (value, arms, value_span) = decode_match_expr(
                    &execution,
                    tag,
                    payload,
                    span,
                )?;
                self.push_expr(
                    index,
                    value,
                    value_span,
                    FrameContinuation::MatchExprValue {
                        instruction,
                        arms: arms.position(),
                        span: value_span,
                        next: Box::new(next),
                    },
                );
            }
            ExprViewKind::PatternIf => {
                let view = super::ExprView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?;
                let (position, value_span) = match view {
                    super::ExprView::PatternIf {
                        branches,
                        span,
                        ..
                    } => {
                        (branches.position(), span)
                    }
                    _ => unreachable!("pattern-if tag has a matching borrowed expression view"),
                };
                self.start_expr_pattern_branch(
                    index,
                    instruction,
                    position,
                    value_span,
                    Box::new(next),
                )?;
            }
            ExprViewKind::StrMatchExpr | ExprViewKind::TagMatchExpr => {
                let view = super::ExprView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?;
                let value = match view {
                    super::ExprView::StrMatchExpr { value, .. }
                    | super::ExprView::TagMatchExpr { value, .. } => value.raw(),
                    _ => unreachable!("string/tag match tag has a matching borrowed view"),
                };
                self.push_expr(
                    index,
                    value,
                    span,
                    FrameContinuation::StrTagMatch {
                        instruction,
                        span,
                        next: Box::new(next),
                    },
                );
            }
            // Compound expressions and formatted strings must stay in the active heap-backed frame machine.
            // Falling back to the recursive evaluator here would nest another explicit runner for each Result
            // call in the expression and consume native stack even when the outer loop itself is iterative.
            ExprViewKind::List | ExprViewKind::ListBuild => {
                let mut items = indexed_list_literal_view(
                    &execution,
                    tag,
                    payload,
                    span,
                )?;
                items.preflight(span)?;
                let capacity = items.len();
                if let Some((operand, splice, item_span)) = items.next_operand(span)? {
                    let position = items.position();
                    let values = Vec::with_capacity(capacity);
                    self.push_expr(
                        index,
                        operand,
                        item_span,
                        FrameContinuation::ListItems {
                            state: ListItemsState {
                                instruction,
                                position,
                                splice,
                                item_span,
                                values,
                                span,
                            },
                            next: Box::new(next),
                        },
                    );
                } else {
                    self.push_value(
                        index,
                        FrameValue::Value(LoweredValue::List(Vec::new())),
                        next,
                    );
                }
            }
            ExprViewKind::MapLiteral => {
                let mut entries = indexed_map_literal_view(
                    &execution,
                    tag,
                    payload,
                    span,
                )?;
                entries
                    .preflight()
                    .map_err(|error| indexed_error(error, span))?;
                let position = entries.position();
                self.step_map_literal(
                    index,
                    MapLiteralState {
                        instruction,
                        position,
                        fields: BTreeMap::new(),
                        key: None,
                        reading_key: false,
                        span,
                    },
                    Box::new(next),
                )?;
            }
            ExprViewKind::RecordUpdate => {
                let (base, updates, update_span) = decode_record_updates(
                    &execution,
                    tag,
                    payload,
                    span,
                )?;
                self.push_expr(
                    index,
                    base,
                    update_span,
                    FrameContinuation::RecordUpdateBase {
                        instruction,
                        position: updates.position(),
                        span: update_span,
                        next: Box::new(next),
                    },
                );
            }
            ExprViewKind::Record => {
                let mut entries = indexed_record_literal_view(
                    &execution,
                    tag,
                    payload,
                    span,
                )?;
                entries
                    .preflight()
                    .map_err(|error| indexed_error(error, span))?;
                let capacity = entries.len();
                let position = entries.position();
                self.step_record_items(
                    index,
                    RecordItemsState {
                        instruction,
                        position,
                        fields: Vec::with_capacity(capacity),
                        span,
                    },
                    Box::new(next),
                )?;
            }
            ExprViewKind::ListComp | ExprViewKind::MapComp => {
                let view = ExprView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?;
                let (map, key, value, mut qualifiers, value_span) = match view {
                    ExprView::ListComp {
                        value,
                        qualifiers,
                        span,
                    } => (false, None, value.raw(), qualifiers, span),
                    ExprView::MapComp {
                        key,
                        value,
                        qualifiers,
                        span,
                    } => (true, Some(key.raw()), value.raw(), qualifiers, span),
                    _ => unreachable!("comprehension tag has a comprehension view"),
                };
                qualifiers
                    .preflight()
                    .map_err(|error| indexed_error(error, span))?;
                let position = qualifiers.position();
                let first = qualifiers
                    .next()
                    .transpose()
                    .map_err(|error| indexed_error(error, span))?;
                if !matches!(first, Some(CompQualifierView::For { .. })) {
                    return Err(RuntimeError::new(
                        "indexed-ir",
                        "comprehension qualifiers must start with for",
                    )
                    .with_span(span));
                }
                qualifiers
                    .resume(position)
                    .map_err(|error| indexed_error(error, span))?;
                let streams = Arc::new(std::sync::Mutex::new(Vec::new()));
                self.calls[index]
                    .work
                    .push(FrameWork::CompCleanup(streams.clone()));
                let state = ListCompState {
                    map,
                    key,
                    value,
                    instruction,
                    position,
                    iterators: Vec::new(),
                    streams,
                    values: Vec::new(),
                    map_values: BTreeMap::new(),
                    span: value_span,
                };
                self.step_comp_qualifier(index, Box::new(state), Box::new(next))?;
            }
            ExprViewKind::FmtString | ExprViewKind::PathFmtString => {
                let (mut parts, fmt) = decode_fmt(
                    &execution,
                    tag,
                    payload,
                    span,
                )?;
                parts
                    .preflight()
                    .map_err(|error| indexed_error(error, span))?;
                let position = parts.position();
                self.step_fmt(
                    index,
                    FmtState {
                        instruction,
                        position,
                        fmt,
                        span,
                    },
                    Box::new(next),
                )?;
            }
            ExprViewKind::ResultFallback => {
                let super::ExprView::ResultFallback { left, right } =
                    super::ExprView::decode(&execution, tag, payload)
                        .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("result-fallback tag has a result-fallback view")
                };
                self.push_expr(
                    index,
                    left.raw(),
                    span,
                    FrameContinuation::ResultFallback {
                        right: right.raw(),
                        span,
                        next: Box::new(next),
                    },
                );
            }
            ExprViewKind::Ok => {
                let super::ExprView::Ok(value) =
                    super::ExprView::decode(&execution, tag, payload)
                        .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("ok tag has an ok view")
                };
                self.push_expr(
                    index,
                    value.raw(),
                    span,
                    FrameContinuation::WrapOk(Box::new(next)),
                );
            }
            ExprViewKind::ProcCallResult => {
                let super::ExprView::ProcCallResult(value) =
                    super::ExprView::decode(&execution, tag, payload)
                        .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("proc-call-result tag has a proc-call-result view")
                };
                self.push_expr(
                    index,
                    value.raw(),
                    span,
                    FrameContinuation::ProcCallResult(Box::new(next)),
                );
            }
            ExprViewKind::Err => {
                let super::ExprView::Err { value, cause } =
                    super::ExprView::decode(&execution, tag, payload)
                        .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("err tag has an err view")
                };
                self.push_expr(
                    index,
                    value.raw(),
                    span,
                    FrameContinuation::WrapErr {
                        cause: cause.map(|cause| cause.raw()),
                        span,
                        next: Box::new(next),
                    },
                );
            }
            ExprViewKind::Try => {
                let super::ExprView::Try {
                    value,
                    span: propagation,
                } = super::ExprView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("try tag has a try view")
                };
                // The propagation's own place: a traceback that starts here
                // names it, not the call that is running.
                // The `?` reuses only a traceback that its operand records.
                self.evaluator.pending_traceback = None;
                self.push_expr(
                    index,
                    value.raw(),
                    span,
                    FrameContinuation::Try {
                        span: propagation,
                        next: Box::new(next),
                    },
                );
            }
            ExprViewKind::CheckedValue => {
                let super::ExprView::CheckedValue {
                    value,
                    check,
                    span: value_span,
                } =
                    super::ExprView::decode(&execution, tag, payload)
                        .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("checked-value tag has a checked-value view")
                };
                let check = super::indexed_type_check(check, span)?;
                self.push_expr(
                    index,
                    value.raw(),
                    value_span,
                    FrameContinuation::CheckedValue {
                        check,
                        span: value_span,
                        next: Box::new(next),
                    },
                );
            }
            ExprViewKind::Require => {
                let super::ExprView::Require {
                    value,
                    check,
                    span: value_span,
                } = super::ExprView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("require tag has a require view")
                };
                let check = super::indexed_type_check(check, span)?;
                self.push_expr(
                    index,
                    value.raw(),
                    value_span,
                    FrameContinuation::Require {
                        check,
                        span: value_span,
                        next: Box::new(next),
                    },
                );
            }
            ExprViewKind::Method => {
                let ExprView::Method {
                    receiver,
                    name,
                    mut args,
                    span: value_span,
                } = ExprView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("method tag has a typed method view")
                };
                args.preflight()
                    .map_err(|error| indexed_error(error, value_span))?;
                let position = args.position();
                let receiver = receiver.raw();
                let name: Arc<str> = Arc::from(name);
                // `x = x.set(..)` and its shape: the call's result overwrites
                // the very slot the receiver reads, so the receiver can be
                // taken out of the slot rather than copied. Anything else — a
                // nested call, a different destination — keeps the plain copy.
                let consume = match &next {
                    FrameContinuation::Assign {
                        slot, op, check, ..
                    } if *op == AssignOp::Set
                        && check.is_none()
                        && !self
                            .evaluator
                            .indexed_root_binds(&self.calls[index].slots, *slot)
                        && indexed_slot_read(
                            &execution,
                            receiver,
                            value_span,
                        )? == Some(*slot) =>
                    {
                        Some(*slot)
                    }
                    _ => None,
                };
                self.push_expr(
                    index,
                    receiver,
                    value_span,
                    FrameContinuation::MethodReceiver {
                        instruction,
                        position,
                        name,
                        span: value_span,
                        consume,
                        next: Box::new(next),
                    },
                );
            }
            ExprViewKind::DynamicCall | ExprViewKind::TypedCall => {
                let view = ExprView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?;
                let (callee, typed_pure, mut args, value_span) = match view {
                    ExprView::DynamicCall {
                        callee,
                        args,
                        span,
                    } => (callee.raw(), None, args, span),
                    ExprView::TypedCall {
                        callee,
                        pure,
                        args,
                        span,
                        ..
                    } => (callee.raw(), Some(pure), args, span),
                    _ => unreachable!("dynamic call tag has a typed call view"),
                };
                let capacity = args.len();
                args.preflight()
                    .map_err(|error| indexed_error(error, value_span))?;
                let position = args.position();
                self.push_expr(
                    index,
                    callee,
                    value_span,
                    FrameContinuation::DynamicCallee {
                        instruction,
                        position,
                        values: Vec::with_capacity(capacity),
                        typed_pure,
                        span: value_span,
                        next: Box::new(next),
                    },
                );
            }
            ExprViewKind::Field => {
                let super::ExprView::Field {
                    base,
                    name,
                    span: value_span,
                } = super::ExprView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("field tag has a field view")
                };
                let base = base.raw();
                let name = name.to_string();
                if !self.evaluator.force_frames {
                    if let Some(base) = Evaluator::indexed_field_chain_ref(
                        &execution,
                        base,
                        &self.calls[index].slots,
                        value_span,
                    )? && let Some(value) = super::lowered_record_field_value(base, &name)
                    {
                        self.push_value(index, FrameValue::Value(value), next);
                        return Ok(());
                    }
                }
                self.push_expr(
                    index,
                    base,
                    value_span,
                    FrameContinuation::Field {
                        name,
                        span: value_span,
                        next: Box::new(next),
                    },
                );
            }
            ExprViewKind::Index => {
                let super::ExprView::Index {
                    base,
                    index: instruction,
                    span: value_span,
                } = super::ExprView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("index tag has an index view")
                };
                self.push_expr(
                    index,
                    base.raw(),
                    value_span,
                    FrameContinuation::IndexBase {
                        instruction: instruction.raw(),
                        span: value_span,
                        next: Box::new(next),
                    },
                );
            }
            ExprViewKind::IndexFromEnd => {
                let super::ExprView::IndexFromEnd {
                    base,
                    distance,
                    span: value_span,
                } = super::ExprView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("index-from-end tag has an index-from-end view")
                };
                self.push_expr(
                    index,
                    base.raw(),
                    value_span,
                    FrameContinuation::IndexFromEnd {
                        distance: distance.get(),
                        span: value_span,
                        next: Box::new(next),
                    },
                );
            }
            ExprViewKind::ModuleCall => {
                let ExprView::ModuleCall {
                    op,
                    call_plan,
                    mut args,
                    span: value_span,
                } = ExprView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("module call tag has a typed call view")
                };
                let capacity = args.len();
                args.preflight()
                    .map_err(|error| indexed_error(error, value_span))?;
                let position = args.position();
                self.step_module_arguments(
                    index,
                    instruction,
                    position,
                    op,
                    call_plan.cloned(),
                    Vec::with_capacity(capacity),
                    value_span,
                    next,
                )?;
            }
            ExprViewKind::ExternalCall => {
                let ExprView::ExternalCall {
                    function: qualified,
                    mut args,
                    span: value_span,
                } = ExprView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?
                else {
                    unreachable!("external call tag has a typed call view")
                };
                let capacity = args.len();
                args.preflight()
                    .map_err(|error| indexed_error(error, value_span))?;
                let position = args.position();
                let kind = self.external_kind(index, qualified, span)?;
                self.push_static_arguments(
                    index,
                    instruction,
                    position,
                    LoweredFunctionKey::Qualified(qualified),
                    kind,
                    true,
                    Vec::with_capacity(capacity),
                    value_span,
                    next,
                )?;
            }
            ExprViewKind::Call | ExprViewKind::SelfCall | ExprViewKind::DirectPureCall => {
                let view = ExprView::decode(&execution, tag, payload)
                    .map_err(|error| indexed_error(error, span))?;
                let (function, mut args, value_span) = match view {
                    ExprView::Call {
                        function,
                        args,
                        span,
                    }
                    | ExprView::DirectPureCall {
                        function,
                        args,
                        span,
                    } => (
                        function
                            .key(&execution)
                            .map_err(|error| indexed_error(error, span))?,
                        args,
                        span,
                    ),
                    ExprView::SelfCall { args, span } => (
                        self.calls[index]
                            .execution
                            .function_identity()
                            .map_err(|error| indexed_error(error, span))?
                            .0,
                        args,
                        span,
                    ),
                    _ => unreachable!("static call tag has a typed call view"),
                };
                args.preflight()
                    .map_err(|error| indexed_error(error, value_span))?;
                let position = args.position();
                let kind = self.function_kind(index, function, span)?;
                self.push_static_arguments(
                    index,
                    instruction,
                    position,
                    function,
                    kind,
                    false,
                    Vec::new(),
                    value_span,
                    next,
                )?;
            }
            ExprViewKind::Null |
            ExprViewKind::Unit |
            ExprViewKind::Int |
            ExprViewKind::Float |
            ExprViewKind::Duration |
            ExprViewKind::Bool |
            ExprViewKind::Str |
            ExprViewKind::Bytes |
            ExprViewKind::PreparedRegex |
            ExprViewKind::PreparedConstant |
            ExprViewKind::Path |
            ExprViewKind::FunctionRef |
            ExprViewKind::PathFrom |
            ExprViewKind::Param |
            ExprViewKind::Glob |
            ExprViewKind::LastStatus |
            ExprViewKind::EmptyMap |
            ExprViewKind::Range |
            ExprViewKind::Tag |
            ExprViewKind::Slice |
            ExprViewKind::StrByteLen |
            ExprViewKind::StrByteAt |
            ExprViewKind::StrPredicate |
            ExprViewKind::FsFiles |
            ExprViewKind::FsWalk |
            ExprViewKind::FsList |
            ExprViewKind::FsTempDir |
            ExprViewKind::FsMkdir |
            ExprViewKind::FsRemove |
            ExprViewKind::PathReadText |
            ExprViewKind::PathReadBytes |
            ExprViewKind::PathExists |
            ExprViewKind::PathExecutable |
            ExprViewKind::PathDu |
            ExprViewKind::PathMetadata |
            ExprViewKind::PathReadlink |
            ExprViewKind::PathResolve |
            ExprViewKind::PathWrite |
            ExprViewKind::PathMkdir |
            ExprViewKind::PathRemove |
            ExprViewKind::ArchiveTarCreate |
            ExprViewKind::ProcessCommandArgv |
            ExprViewKind::Abort |
            ExprViewKind::Fail |
            ExprViewKind::Error => {
                return Err(frame_strategy_error(tag, span));
            }
        }
        Ok(())
    }


    fn step_assertion(
        &mut self,
        index: usize,
        mut state: FrameAssertionState,
    ) -> Result<(), RuntimeError> {
        let execution = self.calls[index].execution.thread_local();
        while let Some(item) = state.work.pop() {
            match item {
                super::AssertionWork::Left { op, right } => {
                    if (op == BinaryOp::And && !state.passed)
                        || (op == BinaryOp::Or && state.passed)
                    {
                        if !state.passed {
                            let failure = state
                                .reached
                                .get_or_insert_with(|| "boolean assertion failed".into());
                            *failure = super::values::bounded_assertion_text(
                                &format!("{failure} (right operand skipped)"),
                                1024,
                            );
                        }
                    } else {
                        state.work.push(super::AssertionWork::Right {
                            op,
                            left_failure: state.reached.take(),
                        });
                        state.work.push(super::AssertionWork::Expr(right));
                    }
                }
                super::AssertionWork::Right { op, left_failure } => {
                    if op == BinaryOp::Or
                        && !state.passed
                        && let Some(left_failure) = left_failure
                    {
                        let right = state
                            .reached
                            .take()
                            .unwrap_or_else(|| "boolean assertion failed".into());
                        state.reached = Some(super::values::bounded_assertion_text(
                            &format!("{left_failure}; {right}"),
                            1024,
                        ));
                    }
                }
                super::AssertionWork::Expr(instruction) => {
                    let (tag, payload) = indexed_value(
                        execution.instruction_id(instruction),
                        state.span,
                    )?;
                    if tag == FullTag::ExprBinary {
                        let super::ExprView::Binary {
                            op,
                            left,
                            right,
                            span: operand_span,
                        } = super::ExprView::decode(
                            &execution,
                            tag,
                            payload,
                        )
                        .map_err(|error| indexed_error(error, state.span))?
                        else {
                            unreachable!("binary tag has a binary view")
                        };
                        if matches!(op, BinaryOp::And | BinaryOp::Or) {
                            state.work.push(super::AssertionWork::Left {
                                op,
                                right: right.raw(),
                            });
                            state
                                .work
                                .push(super::AssertionWork::Expr(left.raw()));
                            continue;
                        }
                        if super::values::assertion_comparison_op(op) {
                            self.push_expr(
                                index,
                                left.raw(),
                                operand_span,
                                FrameContinuation::AssertionComparisonLeft {
                                    state,
                                    instruction,
                                    op,
                                    right: right.raw(),
                                    span: operand_span,
                                },
                            );
                            return Ok(());
                        }
                    } else if tag == FullTag::ExprComparisonChain {
                        let (first, pairs) = decode_comparison_chain(
                            &execution,
                            tag,
                            payload,
                            state.span,
                        )?;
                        self.push_expr(
                            index,
                            first,
                            state.span,
                            FrameContinuation::AssertionChainLeft {
                                state: Box::new(state),
                                instruction,
                                pairs: pairs.position(),
                            },
                        );
                        return Ok(());
                    }
                    self.push_expr(
                        index,
                        instruction,
                        state.span,
                        FrameContinuation::AssertionValue(state),
                    );
                    return Ok(());
                }
            }
        }
        if state.passed {
            return Ok(());
        }
        let failure = state.failure.take().unwrap_or_else(|| {
            state
                .reached
                .take()
                .map(super::AssertionFailure::Reached)
                .unwrap_or(super::AssertionFailure::False)
        });
        if let Some(message) = state.message {
            self.push_expr(
                index,
                message,
                state.span,
                FrameContinuation::AssertionMessage {
                    failure,
                    span: state.span,
                },
            );
            Ok(())
        } else {
            self.finish_assertion_failure(index, failure, None, state.span)
        }
    }

    fn finish_assertion_failure(
        &mut self,
        index: usize,
        failure: super::AssertionFailure,
        context: Option<String>,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let value = self.evaluator.indexed_assertion_failed(
            failure,
            context.as_deref(),
            span,
        )?;
        self.complete_call(index, StmtFlow::Propagate(value))
    }

    fn eval_typed_scalar_expr<'e>(
        &mut self,
        index: usize,
        instruction: u32,
        tag: FullTag,
        execution: &'e FullExecution<'_>,
        payload: FullPayload<'e>,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        enum FrameOperation {
            Binary(BinaryOp, u32, u32), IntByteAt(usize, u32, Option<u32>, Span),
            BoolNot(u32),
        }

        let operation = {
            match tag {
                FullTag::IntBinary | FullTag::IntStrByteAtSlot => match IntView::decode(
                    execution, tag, payload,
                )
                    .map_err(|error| indexed_error(error, span))?
                {
                    IntView::Binary { op, left, right } => {
                        Some(FrameOperation::Binary(op, left.raw(), right.raw()))
                    }
                    IntView::StrByteAtSlot { slot, index, default, span: value_span } => Some(
                        FrameOperation::IntByteAt(
                            slot, index.raw(), default.map(|id| id.raw()), value_span,
                        ),
                    ),
                    _ => return Err(frame_strategy_error(tag, span)),
                },
                FullTag::BoolNot
                    | FullTag::BoolAnd
                    | FullTag::BoolOr
                    | FullTag::BoolIntCompare => match BoolView::decode(execution, tag, payload)
                        .map_err(|error| indexed_error(error, span))?
                    {
                        BoolView::Not(value) => Some(FrameOperation::BoolNot(value.raw())),
                        BoolView::And(left, right) | BoolView::Or(left, right) => {
                            let op = if tag == FullTag::BoolAnd {
                                BinaryOp::And
                            } else {
                                BinaryOp::Or
                            };
                            Some(FrameOperation::Binary(op, left.raw(), right.raw()))
                        }
                        BoolView::IntCompare { op, left, right } => {
                            Some(FrameOperation::Binary(op, left.raw(), right.raw()))
                        }
                        _ => return Err(frame_strategy_error(tag, span)),
                    },
                tag if super::is_typed_int_tag(tag) || super::is_typed_bool_tag(tag) => None,
                _ => return Err(frame_strategy_error(tag, span)),
            }
        };

        match operation {
            Some(FrameOperation::Binary(op, left, right)) => {
                self.push_expr(
                    index,
                    left,
                    span,
                    FrameContinuation::BinaryLeft {
                        op,
                        right,
                        span,
                        next: Box::new(next),
                    },
                );
            }
            Some(FrameOperation::IntByteAt(slot, index_expr, default, value_span)) => {
                self.push_expr(
                    index,
                    index_expr,
                    span,
                    FrameContinuation::TypedIntByteAtIndex {
                        slot,
                        default,
                        span: value_span,
                        next: Box::new(next),
                    },
                );
            }
            Some(FrameOperation::BoolNot(value)) => {
                self.push_expr(
                    index,
                    value,
                    span,
                    FrameContinuation::TypedBoolNot {
                        span,
                        next: Box::new(next),
                    },
                );
            }
            None => {
                // Inlined scalar leaves must carry the verifier's bounded-subtree proof.
                let value = {
                    let call = &mut self.calls[index];
                    if !call.execution.is_in_place(instruction) {
                        return Err(frame_strategy_error(tag, span));
                    }
                    if super::is_typed_int_tag(tag) {
                        match self.evaluator.eval_indexed_typed_int(
                            &call.execution,
                            instruction,
                            &mut call.slots,
                            span,
                        )? {
                            ControlFlow::Continue(value) => FrameValue::Value(LoweredValue::Int(value)),
                            ControlFlow::Break(value) => FrameValue::Break(value),
                        }
                    } else {
                        match self.evaluator.eval_indexed_typed_bool(
                            &call.execution,
                            instruction,
                            &mut call.slots,
                            span,
                        )? {
                            ControlFlow::Continue(value) => FrameValue::Value(LoweredValue::Bool(value)),
                            ControlFlow::Break(value) => FrameValue::Break(value),
                        }
                    }
                };
                match value {
                    FrameValue::Value(value) => self.push_value(index, FrameValue::Value(value), next),
                    FrameValue::Break(value) => self.complete_expression_escape(index, value)?,
                }
            }
        }
        Ok(())
    }

    fn continue_value(
        &mut self,
        index: usize,
        value: FrameValue,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        if matches!(&next, FrameContinuation::CleanupAction) {
            return self.continue_cleanup_action(index, value);
        }
        if matches!(value, FrameValue::Break(LoweredValue::ResultErr(_)))
            && matches!(
                self.evaluator.pending_value_block_flow,
                None | Some(StmtFlow::Propagate(_))
            )
            && let Some((else_param_slot, else_body, span)) = with_initializer_handler(&next)
        {
            let FrameValue::Break(LoweredValue::ResultErr(error)) = value else {
                unreachable!("checked propagated Result error")
            };
            self.evaluator.pending_value_block_flow = None;
            self.evaluator.pending_traceback = None;
            if let Some(slot) = else_param_slot {
                self.calls[index].slots[slot] = lowered_value_from_runtime_any(error.as_ref())
                    .ok_or_else(|| {
                        RuntimeError::new(
                            "type-error",
                            format!(
                                "with error payload cannot be lowered from {}",
                                error.type_name()
                            ),
                        )
                        .with_span(span)
                    })?;
            }
            return self.push_statement_block(index, else_body, span);
        }
        if let Some(flow) = self.evaluator.pending_value_block_flow.take() {
            if let Some(boundary) = self.cleanup_flow_boundary(index, &flow) {
                return self.schedule_cleanup_flow(index, boundary, flow);
            }
            return match flow {
                StmtFlow::Break(value) => self.break_loop(index, value, false),
                StmtFlow::Continue => self.continue_loop(index),
                flow => self.complete_call(index, flow),
            };
        }
        match next {
            FrameContinuation::Field { name, span, next } => match value {
                FrameValue::Value(base) => {
                    let value = self.evaluator.indexed_field_value(base, &name, span)?;
                    self.push_value(index, FrameValue::Value(value), *next);
                }
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::IndexBase {
                instruction,
                span,
                next,
            } => match value {
                FrameValue::Value(base) => self.push_expr(
                    index,
                    instruction,
                    span,
                    FrameContinuation::IndexValue { base, span, next },
                ),
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::IndexFromEnd {
                distance,
                span,
                next,
            } => match value {
                FrameValue::Value(base) => {
                    let value = super::lowered_index_from_end_value(base, distance, span)?;
                    self.push_value(index, FrameValue::Value(value), *next);
                }
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::IndexValue { base, span, next } => match value {
                FrameValue::Value(value) => {
                    let value = super::lowered_index_value(base, value, span)?;
                    self.push_value(index, FrameValue::Value(value), *next);
                }
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::ModuleArguments {
                op,
                call_plan,
                instruction,
                position,
                mut values,
                span,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    values.push(Some(value));
                    self.step_module_arguments(
                        index,
                        instruction,
                        position,
                        op,
                        call_plan,
                        values,
                        span,
                        *next,
                    )?;
                }
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::UtilsCacheResult { key, span, next } => {
                let value = match value {
                    FrameValue::Value(value) => {
                        let result = self
                            .evaluator
                            .finish_lowered_utils_cache_frame(key, value, span);
                        self.evaluator.trace_lowered_utils_cache_result(span);
                        FrameValue::Value(result?)
                    }
                    FrameValue::Break(value) => {
                        self.evaluator.trace_lowered_utils_cache_result(span);
                        FrameValue::Break(value)
                    }
                };
                self.push_value(index, value, *next);
            }
            FrameContinuation::WithBinding {
                instruction,
                position,
                slot,
                body,
                else_param_slot,
                else_body,
                span,
            } => {
                let value = match value {
                    FrameValue::Value(value) => value,
                    FrameValue::Break(value) => {
                        return self.complete_call(index, StmtFlow::Propagate(value));
                    }
                };
                let value = match value {
                    LoweredValue::ResultErr(error) => {
                        self.evaluator.pending_traceback = None;
                        if let Some(slot) = else_param_slot {
                            self.calls[index].slots[slot] =
                                lowered_value_from_runtime_any(error.as_ref()).ok_or_else(|| {
                                    RuntimeError::new(
                                        "type-error",
                                        format!(
                                            "with error payload cannot be lowered from {}",
                                            error.type_name()
                                        ),
                                    )
                                    .with_span(span)
                                })?;
                        }
                        return self.push_statement_block(index, else_body, span);
                    }
                    LoweredValue::ResultOk(value) => *value,
                    value => value,
                };
                self.calls[index].slots[slot] = value;
                self.declare_slot(index, slot);
                let (binding, position) = statement_with_binding(
                    &self.calls[index].execution,
                    instruction,
                    position,
                    span,
                )?;
                if let Some((slot, value)) = binding {
                    self.push_expr(
                        index,
                        value,
                        span,
                        FrameContinuation::WithBinding {
                            instruction,
                            position,
                            slot,
                            body,
                            else_param_slot,
                            else_body,
                            span,
                        },
                    );
                } else {
                    self.push_statement_block(index, body, span)?;
                }
            }
            FrameContinuation::GuardInput {
                target,
                else_param_slot,
                else_body,
                span,
                optional,
            } => match value {
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
                // An optional guard reads its subject as it is: `null` is the
                // failure, with no error to bind, and anything else (a
                // `Result` included) is the value the target names.
                FrameValue::Value(LoweredValue::Null) if optional => {
                    self.calls[index]
                        .work
                        .push(FrameWork::GuardFailureEnd(span));
                    self.push_statement_block(index, else_body, span)?;
                }
                FrameValue::Value(value) if optional => {
                    self.declare_target(index, &target);
                    bind_lowered_comp_target(&target, value, &mut self.calls[index].slots, span)?;
                }
                FrameValue::Value(LoweredValue::ResultOk(value)) => {
                    self.declare_target(index, &target);
                    bind_lowered_comp_target(&target, *value, &mut self.calls[index].slots, span)?;
                }
                FrameValue::Value(LoweredValue::ResultErr(error)) => {
                    if let Some(slot) = else_param_slot {
                        self.calls[index].slots[slot] =
                            lowered_value_from_runtime_any(error.as_ref()).ok_or_else(|| {
                                RuntimeError::new(
                                    "type-error",
                                    format!(
                                        "guard error payload cannot be lowered from {}",
                                        error.type_name()
                                    ),
                                )
                                .with_span(span)
                            })?;
                    }
                    self.calls[index]
                        .work
                        .push(FrameWork::GuardFailureEnd(span));
                    self.calls[index]
                        .work
                        .push(FrameWork::ClearSlots(else_param_slot.into_iter().collect()));
                    self.push_statement_block(index, else_body, span)?;
                }
                FrameValue::Value(other) => {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!("guard expected Result, found {}", other.type_name()),
                    )
                    .with_span(span));
                }
            },
            FrameContinuation::ParameterDefault {
                slot,
                kind,
                check,
                span,
            } => match value {
                FrameValue::Value(value) => {
                    validate_parameter_default(&value, kind, check.as_ref(), span)?;
                    self.calls[index].slots[slot] = value;
                    self.declare_slot(index, slot);
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::Store(slot) => match value {
                FrameValue::Value(value) => {
                    self.calls[index].slots[slot] = value;
                    self.declare_slot(index, slot);
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::StoreTarget { target, span } => match value {
                FrameValue::Value(value) => {
                    bind_lowered_comp_target(&target, value, &mut self.calls[index].slots, span)?
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::Assign {
                slot,
                op,
                singleton,
                check,
                span,
                transfers_resource,
            } => match value {
                FrameValue::Value(value) => {
                    self.check_context_assignment(index, slot, &value, span)?;
                    let owner_scope = self.calls[index].slot_scope(slot);
                    let source_scope = self.evaluator.current_scope_id();
                    // The target's existing contents already belong to its
                    // scope; only the right side can bring resources in, so a
                    // compound assignment transfers that rather than the whole
                    // accumulated value.
                    let incoming = (transfers_resource
                        && op != AssignOp::Set
                        && owner_scope != source_scope)
                        .then(|| value.clone());
                    let value = if let Some(check) = check.as_ref() {
                        checked_indexed_assignment(
                            &self.calls[index].slots[slot],
                            op,
                            value,
                            singleton,
                            check,
                            span,
                        )?
                    } else {
                        apply_indexed_assignment(
                            &mut self.calls[index].slots[slot],
                            op,
                            value,
                            singleton,
                            span,
                        )?
                    };
                    if transfers_resource {
                        self.evaluator
                            .transfer_owned_host_resources_in_lowered_value(
                                incoming.as_ref().unwrap_or(&value),
                                owner_scope,
                            );
                    }
                    self.calls[index].slots[slot] = value;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::AssignSelector(mut state) => match value {
                FrameValue::Value(value) => {
                    state
                        .selectors
                        .push(resolve_assign_index(value, state.span)?);
                    self.advance_assign_path(index, state)?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::AssignPath(state) => match value {
                FrameValue::Value(value) => {
                    self.check_context_assignment(index, state.slot, &value, state.span)?;
                    let (source_scope, owner_scope) = (
                        self.evaluator.current_scope_id(),
                        self.calls[index].slot_scope(state.slot),
                    );
                    let incoming = (state.transfers_resource && owner_scope != source_scope)
                        .then(|| value.clone());
                    apply_indexed_path_assignment(
                        &mut self.calls[index].slots[state.slot],
                        &state.selectors,
                        state.op,
                        value,
                        state.singleton,
                        state.check.as_ref(),
                        state.span,
                    )?;
                    if let Some(incoming) = incoming {
                        self.evaluator
                            .transfer_owned_host_resources_in_lowered_value(
                                &incoming,
                                owner_scope,
                            );
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::Return { transfers_resource } => {
                let flow = match value {
                    FrameValue::Value(value) => {
                        // `fail` lowers to a synthetic `return Err(...)`, which
                        // has no source return-transfer fact. Keep resource
                        // payloads alive across that checked-error boundary.
                        let error_payload = matches!(&value, LoweredValue::ResultErr(_));
                        if transfers_resource || error_payload {
                            self.check_context_scope_value_escape(index, &value)?;
                            let call_scope = self.calls[index].scope_id;
                            // A frame that owns its scope closes it on exit, so
                            // the payload moves to the caller's scope. A block
                            // frame that borrows its scope leaves it open, and
                            // that scope can be a worker's root with no
                            // parent; the payload only has to outlive the
                            // nested block scopes this return unwinds.
                            let target_scope = if self.calls[index].owns_scope() {
                                self.evaluator
                                    .owned_scope_parents
                                    .get(&call_scope)
                                    .copied()
                                    .expect("checked function scope has an ownership parent")
                            } else {
                                call_scope
                            };
                            self.evaluator
                                .transfer_owned_host_resources_in_lowered_value(
                                    &value,
                                    target_scope,
                                );
                        }
                        StmtFlow::Return(value)
                    }
                    FrameValue::Break(value) => StmtFlow::Propagate(value),
                };
                return self.complete_call(index, flow);
            }
            FrameContinuation::BlockValue { transfers_resource } => {
                return match value {
                    FrameValue::Value(value) => {
                        self.complete_expression_value(index, value, transfers_resource)
                    }
                    FrameValue::Break(value) => {
                        self.complete_call(index, StmtFlow::Propagate(value))
                    }
                };
            }
            FrameContinuation::StatementScopeEnd => match value {
                FrameValue::Value(_) => {}
                FrameValue::Break(value) => {
                    self.complete_call(index, StmtFlow::Propagate(value))?
                }
            },
            FrameContinuation::EnvValue(mut state) => match value {
                FrameValue::Value(value) => {
                    let (assignment, position) = statement_env_assignment(
                        &self.calls[index].execution,
                        state.statement,
                        state.position,
                        state.span,
                    )?;
                    let RunEnvView { name, value: run_arg, .. } = assignment;
                    let RunArgView { kind, span, .. } = run_arg;
                    let argument = indexed_run_arg(kind, span);
                    let value = Evaluator::run_env_value(&argument, value.into_value())?;
                    state
                        .overlay
                        .insert(name.as_str().as_bytes().to_vec(), value);
                    state.position = position;
                    self.step_env(index, state)?;
                }
                FrameValue::Break(value) => {
                    self.complete_call(index, StmtFlow::Propagate(value))?
                }
            },
            FrameContinuation::PrintArgument(mut state) => match value {
                FrameValue::Value(value) => {
                    let start = state.line.len();
                    push_lowered_display(&mut state.line, &value, state.span)?;
                    if self.evaluator.trace_enabled {
                        state.argv.push(TraceArg::text(&state.line[start..]));
                    }
                    if !state.position.is_exhausted() {
                        state.line.push(' ');
                    }
                    self.step_print(index, state)?;
                }
                FrameValue::Break(value) => {
                    self.complete_call(index, StmtFlow::Return(value))?
                }
            },
            FrameContinuation::ContextScopeEntry {
                kind,
                body,
                span,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    let statement = matches!(next.as_ref(), FrameContinuation::StatementScopeEnd);
                    let entry = if statement {
                        let target = lowered_path_like_arg(value, "cd", span)?;
                        self.evaluator.enter_indexed_cwd_scope(target, span)
                    } else {
                        self.evaluator.enter_indexed_context_scope(kind, value, span)
                    };
                    match entry {
                        Ok(restore) => {
                            self.calls[index].work.push(FrameWork::ExpressionBoundary {
                                policy: ExpressionBoundaryPolicy::Scope(restore),
                                next: *next,
                            });
                            self.push_statement_block(index, body, span)?;
                        }
                        Err(error) if statement => self.complete_call(
                            index,
                            StmtFlow::Propagate(lowered_result_err_value(error)),
                        )?,
                        Err(error) => self.push_value(
                            index,
                            FrameValue::Value(lowered_result_err_value(error)),
                            *next,
                        ),
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::ErrorContextEntry { body, span, next } => match value {
                FrameValue::Value(value) => {
                    let description =
                        super::lowered_str_arg_owned(Some(value), "", "ctx description", span)?;
                    let context = crate::runtime::value::ErrorContext {
                        kind: "ctx".to_string(),
                        message: Some(description),
                        span: Some(span),
                    };
                    self.open_context_boundaries += 1;
                    self.calls[index].work.push(FrameWork::ExpressionBoundary {
                        policy: ExpressionBoundaryPolicy::Context(context),
                        next: *next,
                    });
                    self.push_statement_block(index, body, span)?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::Discard(span) => match value {
                FrameValue::Value(value @ LoweredValue::ResultErr(_)) => {
                    let value = self
                        .evaluator
                        .lowered_question_propagation_value(value, span)?;
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
                FrameValue::Value(_) => {}
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::ComparisonLeft {
                instruction,
                pairs,
                next,
            } => match value {
                FrameValue::Value(left) => {
                    let (op, right, span, pairs) = comparison_pair_at(
                        &self.calls[index].execution,
                        instruction,
                        pairs,
                        self.calls[index].call_span,
                    )?
                    .ok_or_else(|| {
                        RuntimeError::new("indexed-ir", "comparison chain pair ended early")
                            .with_span(self.calls[index].call_span)
                    })?;
                    self.push_expr(
                        index,
                        right,
                        span,
                        FrameContinuation::ComparisonRight {
                            instruction,
                            pairs,
                            op,
                            left,
                            span,
                            next,
                        },
                    );
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::ComparisonRight {
                instruction,
                pairs,
                op,
                left,
                span,
                next,
            } => match value {
                FrameValue::Value(right) => {
                    let holds = comparison_link_holds(op, &left, &right, span)?;
                    if !holds || pairs.is_exhausted() {
                        self.push_value(index, FrameValue::Value(LoweredValue::Bool(holds)), *next);
                    } else {
                        let (op, operand, span, pairs) = comparison_pair_at(
                            &self.calls[index].execution,
                            instruction,
                            pairs,
                            self.calls[index].call_span,
                        )?
                        .ok_or_else(|| {
                            RuntimeError::new(
                                "indexed-ir",
                                "comparison chain pair ended early",
                            )
                            .with_span(self.calls[index].call_span)
                        })?;
                        self.push_expr(
                            index,
                            operand,
                            span,
                            FrameContinuation::ComparisonRight {
                                instruction,
                                pairs,
                                op,
                                left: right,
                                span,
                                next,
                            },
                        );
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::BinaryLeft {
                op,
                right,
                span,
                next,
            } => match value {
                FrameValue::Value(left) if op == BinaryOp::And || op == BinaryOp::Or => {
                    let left = lowered_condition_bool(left, span)?;
                    if (op == BinaryOp::And && !left) || (op == BinaryOp::Or && left) {
                        self.push_value(index, FrameValue::Value(LoweredValue::Bool(left)), *next);
                    } else {
                        self.push_expr(
                            index,
                            right,
                            span,
                            FrameContinuation::BoolBinaryRight { next, span },
                        );
                    }
                }
                FrameValue::Value(left) => self.push_expr(
                    index,
                    right,
                    span,
                    FrameContinuation::BinaryRight {
                        op,
                        left,
                        span,
                        next,
                    },
                ),
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::BinaryRight {
                op,
                left,
                span,
                next,
            } => match value {
                FrameValue::Value(right) => self.push_value(
                    index,
                    FrameValue::Value(lowered_binary_value(op, left, right, span)?),
                    *next,
                ),
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::BoolBinaryRight { next, span } => match value {
                FrameValue::Value(value) => self.push_value(
                    index,
                    FrameValue::Value(LoweredValue::Bool(lowered_condition_bool(value, span)?)),
                    *next,
                ),
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::ApplyOperands {
                instruction,
                tag,
                position,
                tag_position,
                error_field_position,
                mut values,
                span,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    match (tag, position) {
                        (FullTag::ExprRange, 0) => {
                            super::indexed_range_bound(&value, "start", span)?;
                        }
                        (FullTag::ExprRange, 1) => {
                            super::indexed_range_bound(&value, "end", span)?;
                        }
                        (FullTag::ExprStrByteAt, 1) => {
                            super::indexed_byte_at_index(&value, span)?;
                        }
                        _ => {}
                    }
                    let (operand, child) = {
                        let execution = &self.calls[index].execution;
                        let (decoded_tag, payload) =
                            indexed_value(execution.instruction_id(instruction), span)?;
                        let mut view = super::ExprView::decode(execution, decoded_tag, payload)
                            .map_err(|error| indexed_error(error, span))?;
                        let current = super::indexed_frame_apply_child(
                            tag,
                            &mut view,
                            position,
                            tag_position,
                            error_field_position,
                            span,
                        )?
                        .expect("scheduled frame apply child remains in its expression");
                        let operand = current.operand;
                        let child = super::indexed_frame_apply_child(
                            tag,
                            &mut view,
                            position + 1,
                            current.next_tag_position,
                            current.next_error_field_position,
                            span,
                        )?;
                        (operand, child)
                    };
                    values.push(super::indexed_frame_apply_operand(value, operand, span)?);
                    if let Some(child) = child {
                        self.push_expr(
                            index,
                            child.id,
                            child.span,
                            FrameContinuation::ApplyOperands {
                                instruction,
                                tag,
                                position: position + 1,
                                tag_position: child.tag_position,
                                error_field_position: child.error_field_position,
                                values,
                                span: child.span,
                                next,
                            },
                        );
                    } else {
                        let evaluator = &mut self.evaluator;
                        let execution = &self.calls[index].execution;
                        let value = super::indexed_frame_apply_value(
                            evaluator,
                            execution,
                            instruction,
                            values,
                            span,
                        )?;
                        self.push_value(index, FrameValue::Value(value), *next);
                    }
                }
                FrameValue::Break(value) => {
                    self.push_value(index, FrameValue::Break(value), *next);
                }
            },
            FrameContinuation::ProcessCommandBuilder {
                instruction,
                entry,
                phase,
                state,
                next,
            } => match value {
                FrameValue::Value(value) => self.continue_process_command_builder(
                    index,
                    instruction,
                    entry,
                    phase,
                    state,
                    value,
                    *next,
                )?,
                FrameValue::Break(value) => {
                    self.push_value(index, FrameValue::Break(value), *next);
                }
            },
            FrameContinuation::ProcessRun { state, next } => match value {
                FrameValue::Value(value) => {
                    self.continue_process_run(index, state, value, *next)?;
                }
                FrameValue::Break(value) => {
                    self.push_value(index, FrameValue::Break(value), *next);
                }
            },
            FrameContinuation::SpawnCommand { span, next } => match value {
                FrameValue::Value(LoweredValue::Command(plan)) => {
                    let options = super::SpawnOptions {
                        detach: plan.detach,
                        new_session: plan.new_session,
                        ignore_hup: plan.ignore_hup,
                        same_group: plan.same_group,
                    };
                    let invocation = self
                        .evaluator
                        .invocation_from_command_plan(&plan, span)?;
                    let flow = self
                        .evaluator
                        .eval_lowered_spawn_invocation(invocation, options, span)?;
                    self.push_process_flow(index, flow, span, *next)?;
                }
                FrameValue::Value(value) => {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!("spawn expected Command, found {}", value.type_name()),
                    )
                    .with_span(span));
                }
                FrameValue::Break(value) => {
                    self.push_value(index, FrameValue::Break(value), *next);
                }
            },
            FrameContinuation::Wait { span, next } => match value {
                FrameValue::Value(target) => {
                    let target = target.into_value();
                    let value = match target {
                        Value::ProcessHandle(handle) => {
                            self.evaluator.wait_one_process_handle(*handle, span)?
                        }
                        Value::List(items) => {
                            self.evaluator.wait_process_handle_list(items, span)?
                        }
                        value => super::process_handle::process_handle_error(
                            super::RunError::new(
                                "unknown",
                                format!(
                                    "wait expected ProcessHandle or List[ProcessHandle], found {}",
                                    value.type_name()
                                ),
                            )
                            .with_span(span),
                        ),
                    };
                    let value = lowered_value_from_runtime_any(&value).ok_or_else(|| {
                        RuntimeError::new(
                            "type-error",
                            format!("wait produced unsupported {}", value.type_name()),
                        )
                        .with_span(span)
                    })?;
                    self.push_value(index, FrameValue::Value(value), *next);
                }
                FrameValue::Break(value) => {
                    self.push_value(index, FrameValue::Break(value), *next);
                }
            },
            FrameContinuation::StatementRun {
                propagate_result,
                call_span,
            } => match value {
                FrameValue::Value(value) if !propagate_result => {
                    let _ = value;
                }
                FrameValue::Value(value) => match value {
                    value @ LoweredValue::ResultOk(_) => {
                        let _ = value;
                    }
                    value @ LoweredValue::ResultErr(_) => {
                        let span = match &value {
                            LoweredValue::ResultErr(error) => match error.as_ref() {
                                Value::RunError(error) => error.span,
                                Value::Error(error) => error.span,
                                _ => None,
                            },
                            _ => None,
                        }
                        .unwrap_or(call_span);
                        let value = self
                            .evaluator
                            .lowered_question_propagation_value(value, span)?;
                        self.complete_call(index, StmtFlow::Propagate(value))?;
                    }
                    other => {
                        return Err(RuntimeError::new(
                            "type-error",
                            format!("`?` expected Result, found {}", other.type_name()),
                        )
                        .with_span(call_span));
                    }
                },
                FrameValue::Break(value) => {
                    self.complete_call(index, StmtFlow::Propagate(value))?;
                }
            },
            FrameContinuation::TypedBoolNot { span, next } => match value {
                FrameValue::Value(value) => self.push_value(
                    index,
                    FrameValue::Value(LoweredValue::Bool(!lowered_condition_bool(value, span)?)),
                    *next,
                ),
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::TypedIntByteAtIndex {
                slot,
                default,
                span,
                next,
            } => match value {
                FrameValue::Value(LoweredValue::Int(index_value)) => {
                    let value = super::lowered_str_byte_at_value(
                        &self.calls[index].slots[slot],
                        index_value,
                        -1,
                        span,
                    )?;
                    if value == -1 {
                        if let Some(default) = default {
                            self.push_expr(
                                index,
                                default,
                                span,
                                FrameContinuation::TypedIntByteAtFallback { span, next },
                            );
                        } else {
                            self.push_value(
                                index,
                                FrameValue::Value(LoweredValue::Int(-1)),
                                *next,
                            );
                        }
                    } else {
                        self.push_value(
                            index,
                            FrameValue::Value(LoweredValue::Int(value)),
                            *next,
                        );
                    }
                }
                FrameValue::Value(_) => {
                    return Err(RuntimeError::new(
                        "type-error",
                        "lowered expression expected Int",
                    )
                    .with_span(span));
                }
                FrameValue::Break(value) => {
                    self.push_value(index, FrameValue::Break(value), *next);
                }
            },
            FrameContinuation::TypedIntByteAtFallback { span, next } => match value {
                FrameValue::Value(LoweredValue::Int(value)) => self.push_value(
                    index,
                    FrameValue::Value(LoweredValue::Int(value)),
                    *next,
                ),
                FrameValue::Value(_) => {
                    return Err(RuntimeError::new(
                        "type-error",
                        "lowered expression expected Int",
                    )
                    .with_span(span));
                }
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::AssertionValue(mut state) => match value {
                FrameValue::Value(LoweredValue::Bool(passed)) => {
                    state.passed = passed;
                    state.reached = None;
                    state.failure = None;
                    self.calls[index].work.push(FrameWork::Assertion(state));
                }
                FrameValue::Value(_) => {
                    return Err(RuntimeError::new(
                        "type-error",
                        "assert condition requires Bool",
                    )
                    .with_span(state.span));
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::AssertionComparisonLeft {
                state,
                instruction,
                op,
                right,
                span,
            } => match value {
                FrameValue::Value(left) => self.push_expr(
                    index,
                    right,
                    span,
                    FrameContinuation::AssertionComparisonRight {
                        state,
                        instruction,
                        op,
                        left,
                        span,
                    },
                ),
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::AssertionComparisonRight {
                mut state,
                instruction,
                op,
                left,
                span,
            } => match value {
                FrameValue::Value(right) => {
                    let passed = crate::runtime::eval::lowered_ops::lowered_assertion_comparison(
                        op, &left, &right, span,
                    )?;
                    state.passed = passed;
                    state.reached = None;
                    state.failure = if !passed && instruction == state.condition {
                        Some(super::AssertionFailure::Operands(left, right))
                    } else if passed {
                        None
                    } else {
                        state.reached = Some(super::values::comparison_failure_text(
                            op, &left, &right, span,
                        ));
                        None
                    };
                    self.calls[index].work.push(FrameWork::Assertion(state));
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::AssertionChainLeft {
                state,
                instruction,
                pairs,
            } => match value {
                FrameValue::Value(left) => {
                    let (op, right, span, pairs) = comparison_pair_at(
                        &self.calls[index].execution,
                        instruction,
                        pairs,
                        state.span,
                    )?
                    .ok_or_else(|| {
                        RuntimeError::new("indexed-ir", "comparison chain pair ended early")
                            .with_span(state.span)
                    })?;
                    self.push_expr(
                        index,
                        right,
                        span,
                        FrameContinuation::AssertionChainRight {
                            state,
                            instruction,
                            pairs,
                            op,
                            left,
                            span,
                        },
                    );
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::AssertionChainRight {
                mut state,
                instruction,
                pairs,
                op,
                left,
                span,
            } => match value {
                FrameValue::Value(right) => {
                    let passed = crate::runtime::eval::lowered_ops::lowered_assertion_comparison(
                        op, &left, &right, span,
                    )?;
                    if !passed {
                        let mut failure = super::values::comparison_failure_text(
                            op, &left, &right, span,
                        );
                        if !pairs.is_exhausted() {
                            failure.push_str(" (later operands skipped)");
                        }
                        state.passed = false;
                        state.reached = Some(failure);
                        state.failure = None;
                        self.calls[index].work.push(FrameWork::Assertion(*state));
                    } else if !pairs.is_exhausted() {
                        let (op, next_right, span, pairs) = comparison_pair_at(
                            &self.calls[index].execution,
                            instruction,
                            pairs,
                            state.span,
                        )?
                        .ok_or_else(|| {
                            RuntimeError::new(
                                "indexed-ir",
                                "comparison chain pair ended early",
                            )
                            .with_span(state.span)
                        })?;
                        self.push_expr(
                            index,
                            next_right,
                            span,
                            FrameContinuation::AssertionChainRight {
                                state,
                                instruction,
                                pairs,
                                op,
                                left: right,
                                span,
                            },
                        );
                    } else {
                        state.passed = true;
                        state.reached = None;
                        state.failure = None;
                        self.calls[index].work.push(FrameWork::Assertion(*state));
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::AssertionMessage { failure, span } => match value {
                FrameValue::Value(LoweredValue::Str(text)) => self.finish_assertion_failure(
                    index,
                    failure,
                    Some(super::values::bounded_assertion_text(&text, 1024)),
                    span,
                )?,
                FrameValue::Value(LoweredValue::StrView(text)) => self.finish_assertion_failure(
                    index,
                    failure,
                    Some(super::values::bounded_assertion_text(text.as_str(), 1024)),
                    span,
                )?,
                FrameValue::Value(_) => {
                    return Err(RuntimeError::new(
                        "type-error",
                        "assert message requires Str",
                    )
                    .with_span(span));
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::IfExpr {
                instruction,
                position,
                current_body,
                else_value,
                span,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    if lowered_condition_bool(value, span)? {
                        self.push_expr(index, current_body, span, *next);
                    } else {
                        self.if_expr_condition_step(
                            index,
                            instruction,
                            position,
                            else_value,
                            span,
                            next,
                        )?;
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::StatementIf {
                instruction,
                position,
                body,
                else_body,
                span,
            } => match value {
                FrameValue::Value(value) => {
                    if lowered_condition_bool(value, span)? {
                        self.push_statement_block(index, body, span)?;
                    } else {
                        let (branch, position) = statement_if_branch(
                            &self.calls[index].execution,
                            instruction,
                            position,
                            span,
                        )?;
                        if let Some((condition, body)) = branch {
                            self.push_expr(
                                index,
                                condition,
                                span,
                                FrameContinuation::StatementIf {
                                    instruction,
                                    position,
                                    body,
                                    else_body,
                                    span,
                                },
                            );
                        } else if let Some(body) = else_body {
                            self.push_statement_block(index, body, span)?;
                        }
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::PatternIf {
                instruction,
                position,
                body,
                else_body,
                span,
            } => match value {
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
                FrameValue::Value(value) => {
                    if lowered_condition_bool(value, span)? {
                        self.push_statement_block(index, body, span)?;
                    } else {
                        self.close_pattern_scope(index)?;
                        self.start_pattern_branch(
                            index,
                            instruction,
                            position,
                            else_body,
                            span,
                        )?;
                    }
                }
            },
            FrameContinuation::ExprPatternIf {
                instruction,
                position,
                span,
                next,
                step,
            } => match value {
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
                FrameValue::Value(value) => match step {
                    ExprPatternIfStep::Condition => {
                        if lowered_condition_bool(value, span)? {
                            let (body, _) = self.expr_pattern_if_branch_step(
                                index,
                                instruction,
                                position,
                                span,
                            )?;
                            self.push_expr(
                                index,
                                body,
                                span,
                                FrameContinuation::ExprPatternIf {
                                    instruction,
                                    position,
                                    span,
                                    next,
                                    step: ExprPatternIfStep::ExpressionValue,
                                },
                            );
                        } else {
                            self.close_pattern_scope(index)?;
                            let (_, position) = self.expr_pattern_if_branch_step(
                                index,
                                instruction,
                                position,
                                span,
                            )?;
                            self.start_expr_pattern_branch(
                                index,
                                instruction,
                                position,
                                span,
                                next,
                            )?;
                        }
                    }
                    ExprPatternIfStep::ExpressionValue => {
                        let parent_scope = self.evaluator.parent_owned_host_scope();
                        self.evaluator
                            .transfer_owned_host_resources_in_lowered_value(&value, parent_scope);
                        self.close_pattern_scope(index)?;
                        self.push_value(index, FrameValue::Value(value), *next);
                    }
                },
            },
            FrameContinuation::PatternWhile { body, span } => match value {
                FrameValue::Value(value) => {
                    if lowered_condition_bool(value, span)? {
                        self.push_statement_block(index, body, span)?;
                    } else {
                        self.close_pattern_scope(index)?;
                        let rearmed = self.calls[index].work.pop();
                        debug_assert!(matches!(rearmed, Some(FrameWork::PatternWhile { .. })));
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::ForItems { target, body, span } => match value {
                FrameValue::Value(value) => {
                    let value = match value {
                        LoweredValue::Stream(stream)
                            if stream.source.is_some() || stream.script().is_some() =>
                        {
                            self.calls[index].work.push(FrameWork::ForStream {
                                target,
                                stream: *stream,
                                body,
                                span,
                            });
                            return Ok(());
                        }
                        value => value,
                    };
                    let value = match LoweredScalarCursor::try_new(value) {
                        Ok(cursor) => {
                            self.calls[index].work.push(FrameWork::ForScalars {
                                target,
                                cursor,
                                body,
                                span,
                            });
                            return Ok(());
                        }
                        Err(value) => value,
                    };
                    if let LoweredValue::Map(entries) = value {
                        self.calls[index].work.push(FrameWork::ForMap {
                            target,
                            cursor: LoweredMapCursor::new(entries),
                            body,
                            span,
                        });
                        return Ok(());
                    }
                    let items = self.evaluator.lowered_list_items(
                        value,
                        span,
                        "lowered for expected List",
                    )?;
                    self.calls[index].work.push(FrameWork::ForItems {
                        target,
                        items,
                        index: 0,
                        body,
                        span,
                    });
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::ForPipelineInput {
                slot,
                body,
                span,
                stages,
            } => match value {
                FrameValue::Value(input) => {
                    let pipeline = IndexedSerialPipeline::new_frame(
                        input,
                        stages,
                        span,
                    )?;
                    match pipeline {
                        ControlFlow::Continue(pipeline) => {
                            self.calls[index].work.push(FrameWork::ForPipeline {
                                slot,
                                pipeline: Box::new(pipeline),
                                body,
                                span,
                            });
                        }
                        ControlFlow::Break(value) => {
                            return self.complete_expression_escape(index, value);
                        }
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::StageValueInput { span, stages, next } => match value {
                FrameValue::Value(input) => {
                    let pipeline = IndexedStageEngine::new_frame(
                        input, stages, span,
                    )?;
                    self.calls[index].work.push(FrameWork::StageValue {
                        pipeline: Box::new(pipeline),
                        span,
                        next: *next,
                    });
                }
                FrameValue::Break(value) => {
                    self.push_value(index, FrameValue::Break(value), *next);
                }
            },
            FrameContinuation::StageResult => {
                let result = match value {
                    FrameValue::Value(value) => ControlFlow::Continue(value),
                    FrameValue::Break(value) => ControlFlow::Break(value),
                };
                self.resume_stage_result(index, result);
            }
            FrameContinuation::ForStrLines { slot, body, span } => match value {
                FrameValue::Value(value) => {
                    let start = if let Some((_, start, _)) = lowered_str_parts(&value) {
                        start
                    } else if let Some((_, start, _)) = lowered_bytes_parts(&value) {
                        start
                    } else {
                        return Err(RuntimeError::new(
                            "type-error",
                            "lowered for lines expected Str or Bytes",
                        )
                        .with_span(span));
                    };
                    self.calls[index].work.push(FrameWork::ForStrLines {
                        slot,
                        text: value,
                        cursor: start,
                        line_count: 0,
                        body,
                        span,
                    });
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::While {
                condition,
                body,
                span,
            } => match value {
                FrameValue::Value(value) => {
                    if lowered_condition_bool(value, span)? {
                        self.calls[index].work.push(FrameWork::While {
                            condition,
                            body,
                            span,
                        });
                        self.push_statement_block(index, body, span)?;
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::MatchValue {
                instruction,
                position,
                span,
            } => match value {
                FrameValue::Value(value) => {
                    self.select_match_arm(index, instruction, position, value, span)?
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::StatementStrTagMatch { instruction, span } => match value {
                FrameValue::Value(value) => {
                    let (actual_tag, payload) = indexed_value(
                        self.calls[index].execution.instruction_id(instruction),
                        span,
                    )?;
                    let view = StmtView::decode(&self.calls[index].execution, actual_tag, payload)
                        .map_err(|error| indexed_error(error, span))?;
                    let key = if actual_tag == FullTag::StmtStrMatch {
                        lowered_str_key(&value)
                    } else {
                        lowered_tag_key(&value)
                    };
                    let body = indexed_str_tag_statement_match_arm(view, key, span)?;
                    self.push_statement_block(index, body, span)?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::MatchGuard {
                instruction,
                position,
                body,
                value: match_value,
                span,
            } => match value {
                FrameValue::Value(value) => {
                    if lowered_condition_bool(value, span)? {
                        self.push_statement_block(index, body, span)?;
                    } else {
                        self.select_match_arm(
                            index,
                            instruction,
                            position,
                            match_value,
                            span,
                        )?;
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::MatchExprValue {
                instruction,
                arms,
                next,
                span,
            } => match value {
                FrameValue::Value(value) => {
                    // A `match` on an `Err` handles it: whatever an arm
                    // produces, even an equal error, is not the failure the
                    // subject's traceback describes.
                    if matches!(value, LoweredValue::ResultErr(_)) {
                        self.evaluator.pending_traceback = None;
                    }
                    self.select_expr_match_arm(index, instruction, arms, value, span, *next)?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::StrTagMatch {
                instruction,
                span,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    let (actual_tag, payload) = indexed_value(
                        self.calls[index].execution.instruction_id(instruction),
                        span,
                    )?;
                    let view = super::ExprView::decode(&self.calls[index].execution, actual_tag, payload)
                        .map_err(|error| indexed_error(error, span))?;
                    let key = if actual_tag == FullTag::ExprStrMatch {
                        lowered_str_key(&value)
                    } else {
                        lowered_tag_key(&value)
                    };
                    let arm = indexed_str_tag_match_arm(view, key, span)?;
                    self.push_expr(index, arm, span, *next);
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::MatchExprGuard {
                instruction,
                arms,
                value: match_value,
                body,
                span,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    if matches!(value, LoweredValue::Bool(true)) {
                        self.push_expr(index, body, span, *next);
                    } else {
                        self.select_expr_match_arm(
                            index,
                            instruction,
                            arms,
                            match_value,
                            span,
                            *next,
                        )?;
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::BreakLoop { transfers_resource } => match value {
                FrameValue::Value(value) => {
                    return self.break_loop(index, Some(value), transfers_resource);
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::RetryDelay(mut state, next) => match value {
                FrameValue::Value(LoweredValue::Duration(delay)) => {
                    state.delay_values.push(delay);
                    self.eval_retry_delay(index, state, next)?;
                }
                FrameValue::Value(value) => {
                    return Err(RuntimeError::new(
                        "type-error",
                        format!(
                            "retry delay expected Duration, found {}",
                            value.type_name()
                        ),
                    )
                    .with_span(state.span));
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::DynamicCallee {
                instruction,
                position,
                values,
                typed_pure,
                span,
                next,
            } => match value {
                FrameValue::Value(callee) => {
                    if let Some(pure) = typed_pure {
                        super::indexed_typed_callee_kind(&callee, pure, span)?;
                    }
                    self.push_dynamic_arguments(
                        index,
                        callee,
                        instruction,
                        position,
                        values,
                        span,
                        *next,
                    )?
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::DynamicArguments {
                callee,
                instruction,
                position,
                mode,
                mut values,
                span,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    append_call_argument(&mut values, mode.runtime_kind(), value, span)?;
                    self.push_dynamic_arguments(
                        index,
                        callee,
                        instruction,
                        position,
                        values,
                        span,
                        *next,
                    )?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::CallArguments {
                function,
                kind,
                external,
                instruction,
                position,
                mode,
                mut values,
                span,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    append_call_argument(&mut values, mode.runtime_kind(), value, span)?;
                    self.push_static_arguments(
                        index,
                        instruction,
                        position,
                        function,
                        kind,
                        external,
                        values,
                        span,
                        *next,
                    )?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::WrapOk(next) => match value {
                FrameValue::Value(value) => self.push_value(
                    index,
                    FrameValue::Value(lowered_result_ok(value)),
                    *next,
                ),
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::ProcCallResult(next) => match value {
                FrameValue::Value(value) => self.push_value(
                    index,
                    FrameValue::Value(super::proc_call_result(value)),
                    *next,
                ),
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::WrapErr { cause, span, next } => match value {
                FrameValue::Value(value) => {
                    let error = value.into_value();
                    if let Some(cause) = cause {
                        self.push_expr(
                            index,
                            cause,
                            span,
                            FrameContinuation::AttachErrCause { error, span, next },
                        );
                    } else {
                        self.push_value(
                            index,
                            FrameValue::Value(lowered_err_with_cause(error, None, span)?),
                            *next,
                        );
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::AttachErrCause { error, span, next } => match value {
                FrameValue::Value(cause) => self.push_value(
                    index,
                    FrameValue::Value(lowered_err_with_cause(error, Some(cause), span)?),
                    *next,
                ),
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::Try { span, next } => match value {
                FrameValue::Value(value) => {
                    match self.evaluator.indexed_question_value(value, span)? {
                        Ok(value) => self.push_value(index, FrameValue::Value(value), *next),
                        Err(value) => {
                            let ControlFlow::Break(value) = self
                                .evaluator
                                .preserve_lexical_expression_flow::<()>(StmtFlow::Propagate(value))
                            else {
                                unreachable!("propagation crosses an expression boundary")
                            };
                            self.push_value(index, FrameValue::Break(value), *next);
                        }
                    }
                }
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::CheckedValue { check, span, next } => match value {
                FrameValue::Value(value) => {
                    super::super::checked_unsigned_value(&value, &check, span)?;
                    self.push_value(index, FrameValue::Value(value), *next);
                }
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::Require { check, span, next } => match value {
                FrameValue::Value(value) => {
                    let value = super::super::super::require::require_value(
                        self.evaluator,
                        value,
                        &check,
                        span,
                    );
                    self.push_value(index, FrameValue::Value(value), *next);
                }
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::MethodReceiver {
                instruction,
                position,
                name,
                span,
                consume,
                next,
                ..
            } => match value {
                FrameValue::Value(receiver) => {
                    self.push_method_arguments(
                        index,
                        instruction,
                        position,
                        name,
                        receiver,
                        consume,
                        Vec::new(),
                        span,
                        *next,
                    )?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::MethodArg {
                instruction,
                position,
                name,
                receiver,
                consume,
                mut values,
                span,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    values.push(value);
                    self.push_method_arguments(
                        index,
                        instruction,
                        position,
                        name,
                        receiver,
                        consume,
                        values,
                        span,
                        *next,
                    )?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::FmtValue {
                mut state,
                span,
                spec,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    state.fmt.push_value(&value, span, spec.as_ref())?;
                    self.step_fmt(index, state, next)?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::ResultFallback { right, span, next } => match value {
                FrameValue::Value(value) => match lowered_fallback_value(value) {
                    Some(value) => self.push_value(index, FrameValue::Value(value), *next),
                    None => {
                        // Taking the fallback handles the left side's `Err`:
                        // its traceback describes nothing the fallback
                        // produces, even an equal error.
                        self.evaluator.pending_traceback = None;
                        self.push_expr(index, right, span, *next)
                    }
                },
                FrameValue::Break(value) => self.push_value(index, FrameValue::Break(value), *next),
            },
            FrameContinuation::ListItems { mut state, next } => match value {
                FrameValue::Value(value) => {
                    append_lowered_list_element(
                        &mut state.values,
                        value,
                        state.splice,
                        state.item_span,
                    )?;
                    self.step_list_literal(index, state, next)?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::MapLiteralItems { mut state, next } => match value {
                FrameValue::Value(value) => {
                    let mut entries = indexed_map_literal_view_at(
                        &self.calls[index].execution,
                        state.instruction,
                        state.span,
                    )?;
                    entries
                        .resume(state.position)
                        .map_err(|error| indexed_error(error, state.span))?;
                    let (_, value_instruction, entry_span) = entries
                        .next()
                        .transpose()
                        .map_err(|error| indexed_error(error, state.span))?
                        .ok_or_else(|| {
                            RuntimeError::new("indexed-ir", "map literal cursor ended early")
                                .with_span(state.span)
                        })?;
                    if state.reading_key {
                        state.key = Some(lowered_map_literal_key(&value, entry_span)?);
                        state.reading_key = false;
                        self.push_expr(
                            index,
                            value_instruction.raw(),
                            entry_span,
                            FrameContinuation::MapLiteralItems {
                                state,
                                next,
                            },
                        );
                    } else {
                        let position = entries.position();
                        append_lowered_map_literal(
                            &mut state.fields,
                            state.key.take(),
                            value,
                            entry_span,
                        )?;
                        state.position = position;
                        self.step_map_literal(
                            index,
                            state,
                            next,
                        )?;
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::RecordUpdateBase {
                instruction,
                position,
                span,
                next,
            } => match value {
                FrameValue::Value(base) => {
                    if let Some((path, operand, field_span, position)) = record_update_at(
                        &self.calls[index].execution,
                        instruction,
                        position,
                        span,
                    )? {
                        self.push_expr(
                            index,
                            operand,
                            field_span,
                            FrameContinuation::RecordUpdateItems {
                                instruction,
                                position,
                                path,
                                field_span,
                                base,
                                values: Vec::new(),
                                span,
                                next,
                            },
                        );
                    } else {
                        self.push_value(index, FrameValue::Value(base), *next);
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::RecordUpdateItems {
                instruction,
                position,
                path,
                field_span,
                base,
                mut values,
                span,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    values.push((path, value, field_span));
                    if let Some((path, operand, field_span, position)) = record_update_at(
                        &self.calls[index].execution,
                        instruction,
                        position,
                        span,
                    )? {
                        self.push_expr(
                            index,
                            operand,
                            field_span,
                            FrameContinuation::RecordUpdateItems {
                                instruction,
                                position,
                                path,
                                field_span,
                                base,
                                values,
                                span,
                                next,
                            },
                        );
                    } else {
                        self.push_value(
                            index,
                            FrameValue::Value(lowered_record_update_batch(base, values, span)?),
                            *next,
                        );
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::RecordItems { mut state, next } => match value {
                FrameValue::Value(value) => {
                    let mut entries = indexed_record_literal_view_at(
                        &self.calls[index].execution,
                        state.instruction,
                        state.span,
                    )?;
                    entries
                        .resume(state.position)
                        .map_err(|error| indexed_error(error, state.span))?;
                    let entry = entries
                        .next()
                        .transpose()
                        .map_err(|error| indexed_error(error, state.span))?
                        .ok_or_else(|| {
                            RuntimeError::new("indexed-ir", "record literal cursor ended early")
                                .with_span(state.span)
                        })?;
                    let position = entries.position();
                    super::append_indexed_record_entry(
                        entry,
                        &mut state.fields,
                        value,
                        state.span,
                    )?;
                    state.position = position;
                    self.step_record_items(
                        index,
                        state,
                        next,
                    )?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::ListCompIter {
                mut state,
                active,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    let iterable = lowered_comp_iterable(value, active.span)?;
                    let iterable = match LoweredScalarCursor::try_new(iterable) {
                        Ok(cursor) => {
                            state
                                .iterators
                                .push(CompIterator::Scalars { cursor, active });
                            return self.step_list_comp(index, state, next);
                        }
                        Err(iterable) => iterable,
                    };
                    if let LoweredValue::Stream(stream) = iterable {
                        if stream.source.is_some() || stream.script().is_some() {
                            let stream_index = {
                                let mut streams = state
                                    .streams
                                    .lock()
                                    .expect("comprehension stream state poisoned");
                                let slot = streams
                                    .iter()
                                    .position(Option::is_none)
                                    .unwrap_or(streams.len());
                                if slot == streams.len() {
                                    streams.push(None);
                                }
                                streams[slot] = Some((*stream, active.span));
                                slot
                            };
                            state.iterators.push(CompIterator::Stream {
                                stream: stream_index,
                                active,
                            });
                        } else {
                            let items = self.evaluator.lowered_list_items(
                                LoweredValue::Stream(stream),
                                active.span,
                                "comprehension expected List or Stream",
                            )?;
                            state.iterators.push(CompIterator::Items {
                                items: items.into_iter(),
                                active,
                            });
                        }
                    } else if let LoweredValue::Map(entries) = iterable {
                        state.iterators.push(CompIterator::Map {
                            cursor: LoweredMapCursor::new(entries),
                            active,
                        });
                    } else {
                        let items = self.evaluator.lowered_list_items(
                            iterable,
                            active.span,
                            "comprehension expected List or Stream",
                        )?;
                        state.iterators.push(CompIterator::Items {
                            items: items.into_iter(),
                            active,
                        });
                    }
                    self.step_list_comp(index, state, next)?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::ListCompCondition { state, span, next } => match value {
                FrameValue::Value(value) => {
                    if lowered_condition_bool(value, span)? {
                        self.step_comp_qualifier(index, state, next)?;
                    } else {
                        self.step_list_comp(index, state, next)?;
                    }
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::ListCompKey { state, next } => match value {
                FrameValue::Value(value) => {
                    let key = lowered_map_literal_key(&value, state.span)?;
                    self.push_expr(
                        index,
                        state.value,
                        state.span,
                        FrameContinuation::ListCompValue {
                            state,
                            key: Some(key),
                            next,
                        },
                    );
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::YieldDelegate {
                span,
                transfers_resource,
            } => match value {
                FrameValue::Value(value) => {
                    if transfers_resource {
                        let producer_scope = self.calls[index].scope_id;
                        let consumer_scope = self
                            .evaluator
                            .owned_scope_parents
                            .get(&producer_scope)
                            .copied()
                            .expect("checked producer scope has a consumer parent");
                        self.evaluator
                            .transfer_owned_host_resources_in_lowered_value(
                                &value,
                                consumer_scope,
                            );
                    }
                    self.suspended = Some(ProducerSuspension::Delegated { value, span });
                    return Ok(());
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::Yield { transfers_resource } => match value {
                FrameValue::Value(value) => {
                    self.check_context_scope_value_escape(index, &value)?;
                    if transfers_resource {
                        let producer_scope = self.calls[index].scope_id;
                        let consumer_scope = self
                            .evaluator
                            .owned_scope_parents
                            .get(&producer_scope)
                            .copied()
                            .expect("checked producer scope has a consumer parent");
                        self.evaluator
                            .transfer_owned_host_resources_in_lowered_value(
                                &value,
                                consumer_scope,
                            );
                    }
                    // The frame keeps everything after this statement on its
                    // work stack; the puller receives the value.
                    self.suspended = Some(ProducerSuspension::Yielded(value));
                    return Ok(());
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::ListCompValue {
                mut state,
                key,
                next,
            } => match value {
                FrameValue::Value(value) => {
                    if state.map {
                        state
                            .map_values
                            .insert(key.expect("map comprehension key was evaluated"), value);
                    } else {
                        state.values.push(value);
                    }
                    self.step_list_comp(index, state, next)?;
                }
                FrameValue::Break(value) => {
                    return self.complete_call(index, StmtFlow::Propagate(value));
                }
            },
            FrameContinuation::CleanupAction => {
                unreachable!("cleanup actions are consumed before ordinary continuations")
            }
        }
        Ok(())
    }

    fn complete_expression_escape(
        &mut self,
        index: usize,
        value: LoweredValue,
    ) -> Result<(), RuntimeError> {
        match self
            .evaluator
            .pending_value_block_flow
            .take()
            .unwrap_or(StmtFlow::Propagate(value))
        {
            StmtFlow::Value(value) => self.complete_expression_value(index, value, false),
            StmtFlow::Break(value) => self.break_loop(index, value, false),
            StmtFlow::Continue => self.continue_loop(index),
            flow => self.complete_call(index, flow),
        }
    }

    fn check_context_assignment(
        &self,
        index: usize,
        slot: usize,
        value: &LoweredValue,
        span: Span,
    ) -> Result<(), RuntimeError> {
        if !Evaluator::context_scope_value_escapes(value) {
            return Ok(());
        }
        let call = &self.calls[index];
        let owner = call.slot_scope(slot);
        let Some(boundary) = call.work.iter().rposition(FrameWork::is_context_scope) else {
            return Ok(());
        };
        if call.work[boundary + 1..].iter().any(|work| {
            matches!(work,
            FrameWork::Statements { scope_id: Some(scope), .. } if *scope == owner)
        }) {
            return Ok(());
        }
        Err(context_assignment_escape(span))
    }

    /// Records a slot declared in the current scope.
    fn declare_slot(&mut self, index: usize, slot: usize) {
        let scope = self.evaluator.current_scope_id();
        let call = &mut self.calls[index];
        if slot >= call.slot_scopes.len() {
            let fill = call.slot_scope(slot);
            call.slot_scopes.resize(slot + 1, fill);
        }
        call.slot_scopes[slot] = scope;
    }

    fn declare_target(&mut self, index: usize, target: &LoweredCompTarget) {
        match target {
            LoweredCompTarget::Slot(slot) => self.declare_slot(index, *slot),
            LoweredCompTarget::Record { fields } => {
                for (_, target, _) in fields {
                    self.declare_target(index, target);
                }
            }
            LoweredCompTarget::Discard => {}
        }
    }

    fn complete_expression_value(
        &mut self,
        index: usize,
        value: LoweredValue,
        transfers_resource: bool,
    ) -> Result<(), RuntimeError> {
        let Some(boundary) = self.calls[index]
            .work
            .iter()
            .rposition(|work| {
                matches!(
                    work,
                    FrameWork::ExpressionBoundary {
                        policy: ExpressionBoundaryPolicy::Value
                            | ExpressionBoundaryPolicy::Context(_)
                            | ExpressionBoundaryPolicy::Capture
                            | ExpressionBoundaryPolicy::Retry(_)
                            | ExpressionBoundaryPolicy::Scope(_),
                        ..
                    }
                )
            })
        else {
            if transfers_resource {
                self.evaluator
                    .transfer_owned_host_resources_in_lowered_value(
                        &value,
                        self.evaluator.parent_owned_host_scope(),
                    );
            }
            return self.complete_call(index, StmtFlow::Value(value));
        };
        if matches!(
            &self.calls[index].work[boundary],
            FrameWork::ExpressionBoundary {
                policy: ExpressionBoundaryPolicy::Scope(_),
                ..
            }
        ) && Evaluator::context_scope_value_escapes(&value)
        {
            return Err(RuntimeError::new(
                "context-scope-escape",
                "a live producer or host handle cannot escape a restored context",
            )
            .with_span(self.calls[index].call_span));
        }
        if transfers_resource {
            self.evaluator
                .transfer_owned_host_resources_in_lowered_value(
                    &value,
                    self.evaluator.parent_owned_host_scope(),
                );
        }
        self.schedule_discard(
            index,
            boundary + 1,
            false,
            DiscardResume::ExpressionValue {
                boundary,
                value,
            },
        );
        Ok(())
    }

    fn check_context_scope_value_escape(
        &mut self,
        index: usize,
        value: &LoweredValue,
    ) -> Result<(), RuntimeError> {
        if self.calls[index]
            .work
            .iter()
            .any(FrameWork::is_context_scope)
            && Evaluator::context_scope_value_escapes(value)
        {
            self.evaluator.pending_traceback = None;
            return Err(RuntimeError::new(
                "context-scope-escape",
                "a live producer or host handle cannot escape a restored context",
            )
            .with_span(self.calls[index].call_span));
        }
        Ok(())
    }

    fn complete_call(&mut self, index: usize, flow: StmtFlow) -> Result<(), RuntimeError> {
        // A failure that starts to propagate is a `within` checkpoint, as one
        // that starts to unwind is.
        if matches!(flow, StmtFlow::Propagate(_)) {
            if self.pending_error.is_none() {
                self.evaluator.rearm_swallowed_interruptions();
            }
            self.evaluator
                .check_within_deadline(self.calls[index].call_span)?;
        }
        if let Some(boundary) = self.cleanup_flow_boundary(index, &flow) {
            return self.schedule_cleanup_flow(index, boundary, flow);
        }
        // A body that ran to its end, or returned with nothing left on its work
        // stack, has no boundaries, contexts, or block scopes to unwind.
        if self.calls[index].work.is_empty() && !matches!(flow, StmtFlow::Propagate(_)) {
            if self.calls[index].defers.is_empty() {
                return self.finish_call(index, flow);
            }
            self.calls[index].work.push(FrameWork::Finish(flow));
            return Ok(());
        }
        // A successful pure return may leave only the function's root statement
        // list beneath it. Retire those unused statements synchronously; the
        // normal finish path still transfers resources and writes back captures.
        let can_finish_after_discarding_root_statements = matches!(
            &flow,
            StmtFlow::Value(_) | StmtFlow::Return(_)
        ) && matches!(
            self.calls[index].owner,
            FrameOwner::Function(_, LoweredFunctionKind::Pure)
        ) && self.calls[index].defers.is_empty()
            && self.calls[index].block_scopes.is_empty()
            && !self.calls[index].work.is_empty()
            && self.calls[index].work.iter().all(|work| {
                matches!(
                    work,
                    FrameWork::Statements {
                        complete_call: true,
                        scope_id: None,
                        ..
                    }
                )
            });
        if can_finish_after_discarding_root_statements {
            while let Some(work) = self.calls[index].work.pop() {
                let FrameWork::Statements { .. } = work else {
                    unreachable!("synchronous pure return only retains root statements");
                };
            }
            return self.finish_call(index, flow);
        }
        if let StmtFlow::Propagate(value) = &flow
            && let Some(boundary) = self.capture_boundary(index)
        {
            if self.crosses_context_scope(index, boundary + 1)
                && Evaluator::context_scope_value_escapes(value)
            {
                self.evaluator.pending_traceback = None;
                return Err(RuntimeError::new(
                    "context-scope-escape",
                    "a live producer or host handle cannot escape a restored context",
                )
                .with_span(self.calls[index].call_span));
            }
            let survivor = self.boundary_survivor_scope(index, boundary);
            self.evaluator
                .transfer_owned_host_resources_in_lowered_value(value, survivor);
            let contexts = self.calls[index].work[boundary + 1..]
                .iter()
                .rev()
                .filter_map(|work| match work {
                    FrameWork::ExpressionBoundary {
                        policy: ExpressionBoundaryPolicy::Context(context),
                        ..
                    } => Some(context.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            self.schedule_discard(
                index,
                boundary + 1,
                true,
                DiscardResume::CapturedPropagation {
                    boundary,
                    flow,
                    contexts,
                },
            );
            return Ok(());
        }
        let contexts: Vec<_> = self.calls[index]
            .work
            .iter()
            .rev()
            .filter_map(|work| match work {
                FrameWork::ExpressionBoundary {
                    policy: ExpressionBoundaryPolicy::Context(context),
                    ..
                } => Some(context.clone()),
                _ => None,
            })
            .collect();
        if let StmtFlow::Value(value)
        | StmtFlow::Return(value)
        | StmtFlow::Propagate(value)
        | StmtFlow::Break(Some(value)) = &flow
        {
            self.check_context_scope_value_escape(index, value)?;
        }
        if let StmtFlow::Propagate(value) = &flow {
            let function_scope = self.calls[index].scope_id;
            self.evaluator
                .transfer_owned_host_resources_in_lowered_value(value, function_scope);
        }
        // Lexical exits and checked failures retain resources from every
        // discarded block before cleanup runs in the registering scopes.
        self.schedule_discard(
            index,
            0,
            flow_leaves_with_error(&flow),
            DiscardResume::CompleteCall { flow, contexts },
        );
        Ok(())
    }

    fn cleanup_comp_streams(&mut self, streams: CompStreams) -> Result<(), RuntimeError> {
        let mut first_error = None;
        for stream in streams
            .lock()
            .expect("comprehension stream state poisoned")
            .iter_mut()
            .rev()
        {
            if let Some((mut stream, span)) = stream.take()
                && let Err(error) = self.evaluator.stream_cancel(&mut stream, span)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn step_comp_qualifier(
        &mut self,
        index: usize,
        mut state: Box<ListCompState>,
        next: Box<FrameContinuation>,
    ) -> Result<(), RuntimeError> {
        let mut qualifiers = indexed_comp_qualifiers_view_at(
            &self.calls[index].execution,
            state.instruction,
            state.span,
        )?;
        qualifiers
            .resume(state.position)
            .map_err(|error| indexed_error(error, state.span))?;
        let qualifier_position = qualifiers.position();
        let qualifier = qualifiers
            .next()
            .transpose()
            .map_err(|error| indexed_error(error, state.span))?;
        state.position = qualifiers.position();
        match qualifier {
            Some(CompQualifierView::For { target: _, iter, span }) => {
                let active = CompIteratorState {
                    instruction: state.instruction,
                    target_position: qualifier_position,
                    span,
                    next_qualifier: state.position,
                };
                self.push_expr(
                    index,
                    iter.raw(),
                    span,
                    FrameContinuation::ListCompIter {
                        state,
                        active,
                        next,
                    },
                );
            }
            Some(CompQualifierView::If { condition, span }) => self.push_expr(
                index,
                condition.raw(),
                span,
                FrameContinuation::ListCompCondition { state, span, next },
            ),
            None => return self.push_list_comp_projection(index, state, next),
        }
        Ok(())
    }

    fn step_list_comp(
        &mut self,
        index: usize,
        mut state: Box<ListCompState>,
        next: Box<FrameContinuation>,
    ) -> Result<(), RuntimeError> {
        while !state.iterators.is_empty() {
            let item = match state.iterators.last_mut().expect("active comprehension iterator") {
                CompIterator::Items { items, .. } => items.next(),
                CompIterator::Map { cursor, .. } => cursor.next(),
                CompIterator::Scalars { cursor, active } => {
                    self.evaluator
                        .service_pending_signal(active.span)?;
                    if self.evaluator.shutting_down() {
                        None
                    } else {
                        cursor.next()
                    }
                }
                CompIterator::Stream { stream, active } => {
                    self.evaluator
                        .service_pending_signal(active.span)?;
                    let mut streams = state
                        .streams
                        .lock()
                        .expect("comprehension stream state poisoned");
                    let (producer, span) = streams[*stream]
                        .as_mut()
                        .expect("active comprehension stream");
                    let item = self.evaluator.stream_next(producer, *span)?;
                    let item = item
                        .map(|item| {
                            lowered_value_from_runtime_any(&item).ok_or_else(|| {
                                RuntimeError::new(
                                    "type-error",
                                    "stream produced unsupported comprehension item",
                                )
                                .with_span(*span)
                            })
                        })
                        .transpose()?;
                    if item.is_none() {
                        streams[*stream] = None;
                    }
                    item
                }
            };
            if let Some(item) = item {
                let active = match state
                    .iterators
                    .last()
                    .expect("active comprehension iterator")
                {
                    CompIterator::Scalars { active, .. }
                    | CompIterator::Map { active, .. }
                    | CompIterator::Items { active, .. }
                    | CompIterator::Stream { active, .. } => active,
                };
                let call = &mut self.calls[index];
                let target = indexed_comp_target_view_at(
                    &call.execution,
                    active.instruction,
                    active.target_position,
                    active.span,
                )?;
                bind_indexed_comp_target(
                    target,
                    item,
                    &mut call.slots,
                    active.span,
                    indexed_error,
                )?;
                state.position = active.next_qualifier;
                return self.step_comp_qualifier(index, state, next);
            }
            state.iterators.pop();
        }
        // The comprehension's cleanup sits directly below its own work, so it
        // retires here instead of lingering under the consumer's
        // continuation, where it would break work-stack shapes that pattern
        // conditions and other consumers pop back to.
        let Some(FrameWork::CompCleanup(streams)) = self.calls[index].work.pop() else {
            unreachable!("finished comprehension owns its stream cleanup");
        };
        debug_assert!(Arc::ptr_eq(&streams, &state.streams));
        self.cleanup_comp_streams(streams)?;
        let value = if state.map {
            LoweredValue::Map(Arc::new(state.map_values))
        } else {
            LoweredValue::List(state.values)
        };
        self.push_value(index, FrameValue::Value(value), *next);
        Ok(())
    }

    fn push_list_comp_projection(
        &mut self,
        index: usize,
        state: Box<ListCompState>,
        next: Box<FrameContinuation>,
    ) -> Result<(), RuntimeError> {
        if state.map {
            self.push_expr(
                index,
                state.key.expect("map comprehension key"),
                state.span,
                FrameContinuation::ListCompKey {
                    state,
                    next,
                },
            );
        } else {
            self.push_expr(
                index,
                state.value,
                state.span,
                FrameContinuation::ListCompValue {
                    state,
                    key: None,
                    next,
                },
            );
        }
        Ok(())
    }

    // Argument holes preserve omitted native defaults; present operands run once
    // in source order without retaining recursive evaluator frames across calls.
    fn step_module_arguments(
        &mut self,
        index: usize,
        instruction: u32,
        mut position: ModuleArgumentPosition,
        op: super::RuntimeOp,
        call_plan: Option<Arc<crate::runtime::eval::call_plan::ModuleCallPlan>>,
        mut values: Vec<Option<LoweredValue>>,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        while let Some((argument, next_position)) = module_argument_at(
            &self.calls[index].execution,
            instruction,
            position,
            span,
        )? {
            if let Some(argument) = argument {
                self.push_expr(
                    index,
                    argument,
                    span,
                    FrameContinuation::ModuleArguments {
                        op,
                        call_plan,
                        instruction,
                        position: next_position,
                        values,
                        span,
                        next: Box::new(next),
                    },
                );
                return Ok(());
            }
            values.push(None);
            position = next_position;
        }
        if op == super::RuntimeOp::UtilsCache {
            return self.step_utils_cache(
                index,
                super::super::NativeArgumentValues::new(values),
                span,
                next,
            );
        }
        let flow = self.evaluator.eval_indexed_module_call_values(
            op,
            super::super::NativeArgumentValues::new(values),
            span,
            call_plan.as_deref(),
        )?;
        self.push_value(
            index,
            match flow {
                ControlFlow::Continue(value) => FrameValue::Value(value),
                ControlFlow::Break(value) => FrameValue::Break(value),
            },
            next,
        );
        Ok(())
    }

    fn step_utils_cache(
        &mut self,
        index: usize,
        values: super::super::NativeArgumentValues,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        self.evaluator.trace_lowered_utils_cache_call(span);
        match self.step_utils_cache_after_trace(index, values, span, next) {
            Ok(Some((value, next))) => {
                self.evaluator.trace_lowered_utils_cache_result(span);
                self.push_value(index, FrameValue::Value(value), next);
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(error) => {
                self.evaluator.trace_lowered_utils_cache_result(span);
                Err(error)
            }
        }
    }

    fn step_utils_cache_after_trace(
        &mut self,
        index: usize,
        values: super::super::NativeArgumentValues,
        span: Span,
        next: FrameContinuation,
    ) -> Result<Option<(LoweredValue, FrameContinuation)>, RuntimeError> {
        use super::super::module_utils::LoweredUtilsCacheLookup;

        let lookup = self
            .evaluator
            .prepare_lowered_utils_cache_values(values, span)?;
        let miss = match lookup {
            LoweredUtilsCacheLookup::Hit(value) => return Ok(Some((value, next))),
            LoweredUtilsCacheLookup::Miss(miss) => miss,
        };
        let resolved = self.resolve_active_call(index, miss.function, miss.kind, true, span)?;
        let display_name = miss.function.display_name();
        let call_values = miss
            .args
            .iter()
            .map(|value| {
                lowered_value_from_runtime_any(value).ok_or_else(|| {
                    RuntimeError::new(
                        "unresolved-call",
                        format!("utils.cache target {display_name} could not be lowered"),
                    )
                    .with_span(span)
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.push_resolved_target(
            index,
            resolved,
            call_values,
            span,
            FrameContinuation::UtilsCacheResult {
                key: miss.key,
                span,
                next: Box::new(next),
            },
        )?;
        Ok(None)
    }

    fn push_method_result(
        &mut self,
        index: usize,
        receiver: LoweredValue,
        name: Arc<str>,
        values: Vec<LoweredValue>,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        let result = match self.evaluator.eval_indexed_method_dispatch(
            receiver,
            name.as_ref(),
            values,
            span,
        )? {
            ControlFlow::Continue(value) => FrameValue::Value(value),
            ControlFlow::Break(value) => FrameValue::Break(value),
        };
        self.push_value(index, result, next);
        Ok(())
    }

    fn step_list_literal(
        &mut self,
        index: usize,
        mut state: ListItemsState,
        next: Box<FrameContinuation>,
    ) -> Result<(), RuntimeError> {
        let mut items = indexed_list_literal_view_at(
            &self.calls[index].execution,
            state.instruction,
            state.span,
        )?;
        items.resume(state.position, state.span)?;
        if let Some((operand, splice, item_span)) = items.next_operand(state.span)? {
            state.position = items.position();
            state.splice = splice;
            state.item_span = item_span;
            self.push_expr(
                index,
                operand,
                item_span,
                FrameContinuation::ListItems {
                    state,
                    next,
                },
            );
        } else {
            let ListItemsState { values, .. } = state;
            self.push_value(index, FrameValue::Value(LoweredValue::List(values)), *next);
        }
        Ok(())
    }

    fn step_map_literal(
        &mut self,
        index: usize,
        mut state: MapLiteralState,
        next: Box<FrameContinuation>,
    ) -> Result<(), RuntimeError> {
        let mut entries = indexed_map_literal_view_at(
            &self.calls[index].execution,
            state.instruction,
            state.span,
        )?;
        entries
            .resume(state.position)
            .map_err(|error| indexed_error(error, state.span))?;
        let entry_position = entries.position();
        if let Some((key, value, entry_span)) = entries
            .next()
            .transpose()
            .map_err(|error| indexed_error(error, state.span))?
        {
            let reading_key = key.is_some();
            let operand = key.unwrap_or(value);
            state.position = entry_position;
            state.key = None;
            state.reading_key = reading_key;
            self.push_expr(
                index,
                operand.raw(),
                entry_span,
                FrameContinuation::MapLiteralItems {
                    state,
                    next,
                },
            );
        } else {
            let MapLiteralState { fields, .. } = state;
            self.push_value(
                index,
                FrameValue::Value(LoweredValue::Map(Arc::new(fields))),
                *next,
            );
        }
        Ok(())
    }

    fn step_record_items(
        &mut self,
        index: usize,
        mut state: RecordItemsState,
        next: Box<FrameContinuation>,
    ) -> Result<(), RuntimeError> {
        let mut entries = indexed_record_literal_view_at(
            &self.calls[index].execution,
            state.instruction,
            state.span,
        )?;
        entries
            .resume(state.position)
            .map_err(|error| indexed_error(error, state.span))?;
        let entry_position = entries.position();
        if let Some(entry) = entries
            .next()
            .transpose()
            .map_err(|error| indexed_error(error, state.span))?
        {
            let operand = match &entry {
                RecordEntryView::Field(_, value) | RecordEntryView::Spread(value) => *value,
            };
            state.position = entry_position;
            self.push_expr(
                index,
                operand.raw(),
                state.span,
                FrameContinuation::RecordItems {
                    state,
                    next,
                },
            );
        } else {
            let RecordItemsState { fields, .. } = state;
            self.push_value(
                index,
                FrameValue::Value(finish_record_entries(fields)),
                *next,
            );
        }
        Ok(())
    }

    fn step_fmt(
        &mut self,
        index: usize,
        mut state: FmtState,
        next: Box<FrameContinuation>,
    ) -> Result<(), RuntimeError> {
        let mut parts = indexed_fmt_parts_view_at(
            &self.calls[index].execution,
            state.instruction,
            state.span,
        )?;
        parts
            .resume(state.position)
            .map_err(|error| indexed_error(error, state.span))?;
        while let Some(part) = parts.next() {
            let part = part.map_err(|error| indexed_error(error, state.span))?;
            state.position = parts.position();
            match part {
                FmtPartView::Text(text) => state.fmt.push_text(text.as_ref()),
                FmtPartView::Expr(instruction, span, spec) => {
                    self.push_expr(
                        index,
                        instruction.raw(),
                        span,
                        FrameContinuation::FmtValue {
                            state,
                            span,
                            spec,
                            next,
                        },
                    );
                    return Ok(());
                }
            }
        }
        self.push_value(index, FrameValue::Value(state.fmt.finish()?), *next);
        Ok(())
    }

    fn finish_deferred_call(&mut self, index: usize, flow: StmtFlow) -> Result<(), RuntimeError> {
        let defers = std::mem::take(&mut self.calls[index].defers);
        let leaves_with_error = flow_leaves_with_error(&flow);
        let released = match &flow {
            StmtFlow::Propagate(value) => {
                self.release_call_host_resources(index, Some(value))
            }
            _ => self.release_call_host_resources(index, None),
        };
        let cleanup_contexts = self.collect_cleanup_contexts();
        let group = CleanupGroup {
            next_defer: defers.len(),
            defers,
            leaves_with_error,
            first_error: None,
            first_traceback: None,
            primary_error: None,
            primary_traceback: self.evaluator.pending_traceback.take(),
            released: released.err(),
            cleanup_contexts,
            call_span: self.calls[index].call_span,
            resume: CleanupGroupResume::Call(flow),
        };
        self.calls[index]
            .work
            .push(FrameWork::CleanupGroup(Box::new(group)));
        Ok(())
    }

    fn finish_error_deferred_call(&mut self, index: usize) -> Result<(), RuntimeError> {
        let defers = std::mem::take(&mut self.calls[index].defers);
        let leaves_with_error = !self.unwinding_successful_exit();
        if let Some(error) = self.pending_error.as_ref()
            && error.abort.is_none()
            && error.propagated
            && self.calls[index].owns_scope()
            && self.calls[index].block_scopes.is_empty()
        {
            let parent = self.evaluator.parent_owned_host_scope();
            self.evaluator
                .transfer_owned_host_resources_in_runtime_error(
                    error,
                    parent,
                );
        }
        let released = self.release_call_host_resources(index, None);
        let cleanup_contexts = self.collect_cleanup_contexts();
        let primary_error = self
            .pending_error
            .take()
            .expect("error cleanup starts with a pending error");
        let primary_traceback = self.evaluator.pending_traceback.take();
        if defers.is_empty() {
            if let Err(error) = released {
                self.report_cleanup_error_with_contexts(
                    &error,
                    self.calls[index].call_span,
                    &cleanup_contexts,
                );
            }
            self.pending_error = Some(primary_error);
            self.evaluator.pending_traceback = primary_traceback;
            return self.finish_error_call(index, &cleanup_contexts);
        }
        let group = CleanupGroup {
            next_defer: defers.len(),
            defers,
            leaves_with_error,
            first_error: None,
            first_traceback: None,
            primary_error: Some(primary_error),
            primary_traceback,
            released: released.err(),
            cleanup_contexts,
            call_span: self.calls[index].call_span,
            resume: CleanupGroupResume::ErrorCall,
        };
        self.calls[index]
            .work
            .push(FrameWork::CleanupGroup(Box::new(group)));
        Ok(())
    }

    /// Releases what the call's own scope still owns before deferred actions
    /// run. A propagating error keeps its resource payload in the caller's
    /// scope; successful values have already crossed their checked sink.
    fn release_call_host_resources(
        &mut self,
        index: usize,
        leaving: Option<&LoweredValue>,
    ) -> Result<(), RuntimeError> {
        let call = &self.calls[index];
        if !call.owns_scope() || !call.block_scopes.is_empty() {
            return Ok(());
        }
        let scope_id = call.scope_id;
        if let Some(value) = leaving {
            let parent = self.evaluator.parent_owned_host_scope();
            self.evaluator
                .transfer_owned_host_resources_in_lowered_value(value, parent);
        }
        self.evaluator.release_owned_host_resources(scope_id)
    }

    fn collect_cleanup_contexts(&mut self) -> Vec<crate::runtime::value::ErrorContext> {
        let expected = self.open_context_boundaries;
        if expected == 0 {
            return self.evaluator.cleanup_error_contexts.clone();
        }
        let mut active = Vec::with_capacity(expected);
        'calls: for call in &self.calls {
            for work in &call.work {
                if let FrameWork::ExpressionBoundary {
                    policy: ExpressionBoundaryPolicy::Context(context),
                    ..
                } = work
                {
                    active.push(context.clone());
                    if active.len() == expected {
                        break 'calls;
                    }
                }
            }
        }
        self.open_context_boundaries = active.len();
        let installed = &self.evaluator.cleanup_error_contexts;
        if active.starts_with(installed) {
            active
        } else {
            let mut contexts = installed.clone();
            contexts.extend(active);
            contexts
        }
    }

    fn report_cleanup_error_with_contexts(
        &mut self,
        error: &RuntimeError,
        fallback: Span,
        contexts: &[crate::runtime::value::ErrorContext],
    ) {
        let previous = std::mem::replace(
            &mut self.evaluator.cleanup_error_contexts,
            contexts.to_vec(),
        );
        self.evaluator.report_cleanup_error(error, fallback);
        self.evaluator.cleanup_error_contexts = previous;
    }

    fn restore_cleanup_action_guard(
        &mut self,
        guard: CleanupActionGuard,
    ) -> Vec<crate::runtime::value::ErrorContext> {
        self.evaluator.cleanup_depth = self
            .evaluator
            .cleanup_depth
            .checked_sub(1)
            .expect("deferred action cleanup depth is balanced");
        self.evaluator
            .unshield_within_deadlines(guard.shielded_deadlines);
        std::mem::replace(
            &mut self.evaluator.cleanup_error_contexts,
            guard.previous_cleanup_contexts,
        )
    }

    fn continue_cleanup_action(
        &mut self,
        index: usize,
        value: FrameValue,
    ) -> Result<(), RuntimeError> {
        let span = match self.calls[index].work.iter().rev().nth(1) {
            Some(FrameWork::CleanupGroup(group)) => group.call_span,
            _ => unreachable!("a cleanup action has a cleanup group below it"),
        };
        let value = match value {
            FrameValue::Value(value) => ControlFlow::Continue(value),
            FrameValue::Break(value) => ControlFlow::Break(value),
        };
        let pending = self.evaluator.pending_value_block_flow.take();
        self.evaluator
            .normalize_indexed_deferred_result(value, pending, span)?;

        let Some(FrameWork::CleanupBoundary(guard)) = self.calls[index].work.pop() else {
            unreachable!("successful cleanup action owns its boundary");
        };
        let cleanup_contexts = self.restore_cleanup_action_guard(guard);
        let Some(FrameWork::CleanupGroup(group)) = self.calls[index].work.pop() else {
            unreachable!("successful cleanup action owns its group");
        };
        let mut group = group;
        group.cleanup_contexts = cleanup_contexts;
        self.evaluator.pending_traceback = None;
        self.calls[index].work.push(FrameWork::CleanupGroup(group));
        Ok(())
    }

    fn record_cleanup_action_failure(
        &mut self,
        mut group: Box<CleanupGroup>,
        error: RuntimeError,
        traceback: Option<Traceback>,
    ) {
        group.leaves_with_error = true;
        if group.first_error.is_none() {
            group.first_error = Some(error);
            group.first_traceback = traceback;
        } else {
            self.report_cleanup_error_with_contexts(
                &error,
                group.call_span,
                &group.cleanup_contexts,
            );
        }
        self.evaluator.pending_traceback = None;
        self.calls
            .last_mut()
            .expect("cleanup action owner remains active")
            .work
            .push(FrameWork::CleanupGroup(group));
    }

    fn step_cleanup_group(&mut self, index: usize) -> Result<(), RuntimeError> {
        let Some(FrameWork::CleanupGroup(mut group)) = self.calls[index].work.pop() else {
            unreachable!("cleanup group is stepped in place");
        };
        while group.next_defer > 0 {
            group.next_defer -= 1;
            let deferred = group.defers[group.next_defer];
            if deferred.on_error() && !group.leaves_with_error {
                continue;
            }
            let guard = CleanupActionGuard {
                shielded_deadlines: self.evaluator.shield_within_deadlines(),
                previous_cleanup_contexts: std::mem::replace(
                    &mut self.evaluator.cleanup_error_contexts,
                    std::mem::take(&mut group.cleanup_contexts),
                ),
            };
            self.evaluator.cleanup_depth += 1;
            self.calls[index].cancelled = false;
            self.calls[index]
                .work
                .push(FrameWork::CleanupGroup(group));
            self.calls[index]
                .work
                .push(FrameWork::CleanupBoundary(guard));
            self.push_expr(
                index,
                deferred.value(),
                self.calls[index].call_span,
                FrameContinuation::CleanupAction,
            );
            return Ok(());
        }
        self.finish_cleanup_group(index, *group)
    }

    fn finish_cleanup_group(
        &mut self,
        index: usize,
        mut group: CleanupGroup,
    ) -> Result<(), RuntimeError> {
        if let Some(released) = group.released.take() {
            if group.first_error.is_some() {
                self.report_cleanup_error_with_contexts(
                    &released,
                    group.call_span,
                    &group.cleanup_contexts,
                );
            } else {
                group.first_error = Some(released);
            }
        }

        match group.resume {
            CleanupGroupResume::Call(flow) => {
                let cleanup_error = group.first_error.take();
                self.evaluator.pending_traceback =
                    group.primary_traceback.take().or(group.first_traceback.take());
                if let Some(error) = cleanup_error {
                    if error.abort.as_ref().is_some_and(|signal| signal.force) {
                        return Err(error);
                    }
                    if flow_leaves_with_error(&flow) {
                        self.report_cleanup_error_with_contexts(
                            &error,
                            group.call_span,
                            &group.cleanup_contexts,
                        );
                    } else {
                        let mut error = error;
                        if error.propagated {
                            error.scope_cleanup = error.abort.is_none();
                        }
                        return Err(error);
                    }
                }
                self.finish_call_with_cleanup_contexts(
                    index,
                    flow,
                    &group.cleanup_contexts,
                )
            }
            CleanupGroupResume::ErrorCall => {
                if let Some(error) = group.first_error.take() {
                    if error.abort.as_ref().is_some_and(|signal| signal.force) {
                        return Err(error);
                    }
                    self.report_cleanup_error_with_contexts(
                        &error,
                        group.call_span,
                        &group.cleanup_contexts,
                    );
                }
                self.pending_error = Some(
                    group
                        .primary_error
                        .take()
                        .expect("error cleanup retains its primary"),
                );
                self.evaluator.pending_traceback =
                    group.primary_traceback.take().or(group.first_traceback.take());
                self.finish_error_call(index, &group.cleanup_contexts)
            }
            CleanupGroupResume::Block {
                scope_id,
                mut disposition,
                during_discard,
                include_work_contexts,
            } => {
                let mut cleanup = group.first_error.take();
                if include_work_contexts
                    && disposition == CleanupFailureResources::Retain
                    && cleanup.is_some()
                {
                    let keep = self
                        .capture_boundary(index)
                        .map_or(0, |boundary| boundary + 1);
                    if self.crosses_context_scope(index, keep) {
                        disposition = CleanupFailureResources::RejectContextEscape;
                    }
                }
                if let Some(error) = &cleanup
                    && error.abort.is_none()
                    && error.propagated
                {
                    if disposition == CleanupFailureResources::RejectContextEscape
                        && Evaluator::context_scope_runtime_error_escapes(error)
                    {
                        self.evaluator.pending_traceback = None;
                        cleanup = Some(
                            RuntimeError::new(
                                "context-scope-escape",
                                "a live producer or host handle cannot escape a restored context",
                            )
                            .with_span(self.calls[index].call_span),
                        );
                    } else if disposition == CleanupFailureResources::Retain {
                        let parent = self.evaluator.parent_owned_host_scope();
                        self.evaluator
                            .transfer_owned_host_resources_in_runtime_error(error, parent);
                    } else {
                        self.evaluator
                            .transfer_owned_host_resources_in_runtime_error(error, scope_id);
                    }
                }
                let popped = self.calls[index].block_scopes.pop();
                debug_assert_eq!(popped, Some(scope_id));
                let host_cleanup = self.evaluator.exit_owned_host_scope(scope_id);
                if let Err(secondary) = host_cleanup {
                    if secondary.abort.as_ref().is_some_and(|signal| signal.force) {
                        return Err(secondary);
                    }
                    if cleanup.is_some() {
                        self.report_cleanup_error_with_contexts(
                            &secondary,
                            self.calls[index].call_span,
                            &group.cleanup_contexts,
                        );
                    } else {
                        cleanup = Some(secondary);
                    }
                }
                self.evaluator.pending_traceback =
                    group.primary_traceback.take().or(group.first_traceback.take());
                if during_discard {
                    let Some(FrameWork::Discard(mut state)) = self.calls[index].work.pop() else {
                        unreachable!("discarded block cleanup resumes its discard cursor");
                    };
                    if let Some(error) = cleanup {
                        let traceback = self.evaluator.pending_traceback.take();
                        self.record_discard_error(&mut state, error, traceback);
                    }
                    self.calls[index].work.push(FrameWork::Discard(state));
                    Ok(())
                } else if let Some(error) = cleanup {
                    let mut error = error;
                    if error.propagated {
                        error.scope_cleanup = error.abort.is_none();
                    }
                    Err(error)
                } else {
                    Ok(())
                }
            }
        }
    }

    fn finish_error_call(
        &mut self,
        index: usize,
        cleanup_contexts: &[crate::runtime::value::ErrorContext],
    ) -> Result<(), RuntimeError> {
        debug_assert_eq!(index, self.calls.len() - 1);
        let mut call = self.calls.pop().expect("active indexed frame");
        let is_function = matches!(call.owner, FrameOwner::Function(..));
        let mut returns_result = false;
        if let FrameOwner::Function(function, kind) = call.owner
            && let Ok(header) = Self::function_header(
                call.execution.program(),
                function,
                kind,
                call.call_span,
            )
        {
            returns_result = matches!(header.return_kind, LoweredReturnKind::Result(_));
            let _ =
                self.evaluator
                    .write_back_lowered_captures(&header, &call.slots, call.call_span);
        }
        if let Some(error) = self.pending_error.as_ref()
            && error.abort.is_none()
            && error.propagated
            && call.owns_scope()
        {
            let parent = self.evaluator.parent_owned_host_scope();
            self.evaluator
                .transfer_owned_host_resources_in_runtime_error(error, parent);
        }
        // An active error remains primary; capability release can still add a
        // secondary cleanup failure while the call's scope is closed.
        if let Err(error) = cleanup_call_scopes(self.evaluator, &mut call) {
            self.report_cleanup_error_with_contexts(
                &error,
                call.call_span,
                cleanup_contexts,
            );
        }
        if let FrameOwner::Function(function, kind) = call.owner {
            self.release_slots(call.slots);
            self.evaluator.call_stack.pop();
            let exit_kind = match kind {
                LoweredFunctionKind::Pure => TraceKind::PureExit,
                LoweredFunctionKind::Proc => TraceKind::ProcExit,
            };
            if self.evaluator.trace_enabled {
                let name = function.display_name();
                self.evaluator.trace_exit_with_definition(
                    exit_kind,
                    Some(call.call_span),
                    Some(call.definition_span),
                    Some(&name),
                    TracePayload::None,
                );
            }
        }
        if returns_result
            && let Some(error) = self.pending_error.take_if(|error| error.scope_cleanup)
        {
            // The body left without a failure and a deferred action then
            // failed: that failure is what the function returns, so the
            // caller handles it like any other `Err` of this call.
            let value = capture_checked_error(error)?;
            match call.return_to {
                Some(next) => {
                    let parent = self.calls.len() - 1;
                    if let FrameOwner::Block { .. } = self.calls[parent].owner {
                        let call = &mut self.calls[parent];
                        self.evaluator
                            .sync_indexed_root_slots(&mut call.slots, call.call_span)?;
                    }
                    self.push_value(parent, FrameValue::Value(value), next);
                }
                None => self.result = Some(Ok(value)),
            }
            return Ok(());
        }
        if let Some(FrameContinuation::UtilsCacheResult { span, .. }) =
            call.return_to.as_ref()
        {
            self.evaluator.trace_lowered_utils_cache_result(*span);
        }
        if is_function && let Some(error) = self.pending_error.as_mut() {
            error.scope_cleanup = false;
        }
        if let Some(parent) = self.calls.len().checked_sub(1) {
            self.unwind_error_frame(parent)?;
        } else {
            self.result = Some(Err(self
                .pending_error
                .take()
                .expect("pending indexed frame error")));
        }
        Ok(())
    }

    fn finish_call(&mut self, index: usize, flow: StmtFlow) -> Result<(), RuntimeError> {
        self.finish_call_with_cleanup_contexts(index, flow, &[])
    }

    fn finish_call_with_cleanup_contexts(
        &mut self,
        index: usize,
        flow: StmtFlow,
        cleanup_contexts: &[crate::runtime::value::ErrorContext],
    ) -> Result<(), RuntimeError> {
        debug_assert_eq!(index, self.calls.len() - 1);
        let FrameOwner::Function(function, kind) = self.calls[index].owner else {
            return self.finish_block(index, flow);
        };
        let mut call = self.calls.pop().expect("active indexed frame");
        // The frame's own vectors are done with; the returned value has already
        // been taken out of `slots`.
        self.evaluator.frame_scratch.recycle(&mut call);
        let header = Self::function_header(
            call.execution.program(),
            function,
            kind,
            call.call_span,
        )?;
        // Producer failures leave the stream boundary as runtime errors. Keep
        // the original propagation location when converting the Result value.
        let return_span = if call.producer {
            self.evaluator
                .pending_traceback
                .as_ref()
                .and_then(|traceback| traceback.failing_span)
                .unwrap_or(call.call_span)
        } else {
            call.call_span
        };
        let value = match flow {
            StmtFlow::Return(LoweredValue::Unit) if call.producer => Ok(LoweredValue::Unit),
            StmtFlow::Value(value) | StmtFlow::Return(value) | StmtFlow::Propagate(value) => {
                super::super::checked_lowered_return_value(&header, value, return_span)
            }
            // A producer ends by running out of statements; that is the end of
            // the stream, not a function that failed to return.
            StmtFlow::None if call.producer => Ok(LoweredValue::Unit),
            StmtFlow::None => Err(
                RuntimeError::new("return", "lowered function did not return")
                    .with_span(call.call_span),
            ),
            StmtFlow::Break(_) => {
                Err(RuntimeError::new("control-flow", "break outside loop")
                    .with_span(call.call_span))
            }
            StmtFlow::Continue => Err(RuntimeError::new("control-flow", "continue outside loop")
                .with_span(call.call_span)),
        };
        let write_back =
            self.evaluator
                .write_back_lowered_captures(&header, &call.slots, call.call_span);
        if let Err(error) = &value
            && error.abort.is_none()
            && error.propagated
        {
            let parent = self.evaluator.parent_owned_host_scope();
            self.evaluator
                .transfer_owned_host_resources_in_runtime_error(error, parent);
        }
        let cleanup = cleanup_call_scopes(self.evaluator, &mut call);
        self.evaluator.frame_scratch.recycle_block_stacks(&mut call);
        self.release_slots(call.slots);
        let exit_kind = match kind {
            LoweredFunctionKind::Pure => TraceKind::PureExit,
            LoweredFunctionKind::Proc => TraceKind::ProcExit,
        };
        self.evaluator.call_stack.pop();
        if self.evaluator.trace_enabled {
            let name = function.display_name();
            self.evaluator.trace_exit(
                exit_kind,
                Some(call.call_span),
                Some(&name),
                TracePayload::None,
            );
        }
        let value = match value {
            Err(error) => {
                if let Err(secondary) = cleanup {
                    self.report_cleanup_error_with_contexts(
                        &secondary,
                        call.call_span,
                        cleanup_contexts,
                    );
                }
                Err(error)
            }
            Ok(value) => {
                write_back?;
                cleanup?;
                Ok(value)
            }
        };
        match (call.return_to, value) {
            (Some(next), Ok(value)) => {
                let parent = self.calls.len() - 1;
                // A callee may have written a script binding back; a block
                // frame running top-level statements reads it from its slots.
                if let FrameOwner::Block { .. } = self.calls[parent].owner {
                    let call = &mut self.calls[parent];
                    self.evaluator
                        .sync_indexed_root_slots(&mut call.slots, call.call_span)?;
                }
                self.push_value(parent, FrameValue::Value(value), next);
            }
            (Some(next), Err(error)) => {
                if let FrameContinuation::UtilsCacheResult { span, .. } = next {
                    self.evaluator.trace_lowered_utils_cache_result(span);
                }
                self.begin_error_unwind(error);
            }
            (None, Err(error)) => self.begin_error_unwind(error),
            (None, Ok(value)) => self.result = Some(Ok(value)),
        }
        Ok(())
    }

    fn release_slots(&mut self, slots: FrameSlots<'p>) {
        if let FrameSlots::Owned(slots) = slots {
            self.evaluator.recycle_lowered_slots(slots);
        }
    }

    fn function_header(
        program: &FullProgram,
        function: LoweredFunctionKey,
        kind: LoweredFunctionKind,
        span: Span,
    ) -> Result<Arc<FunctionHeader>, RuntimeError> {
        program
            .function_view(function, kind)
            .map_err(|error| indexed_error(error, span))?
            .ok_or_else(|| {
                RuntimeError::new("unresolved-lowered-call", function.display_name())
                    .with_span(span)
            })?
            .header()
            .map_err(|error| indexed_error(error, span))
    }

    /// Hands a finished block's flow back to the recursive evaluator. Outgoing
    /// values and checked failures keep their resources in the parent scope;
    /// a cleanup failure after a checked failure is reported, not raised.
    /// The frame is finished where it lies on the stack and dropped there.
    fn finish_block(&mut self, index: usize, flow: StmtFlow) -> Result<(), RuntimeError> {
        let call = &mut self.calls[index];
        self.evaluator.frame_scratch.recycle(call);
        if call.owns_scope() && let StmtFlow::Propagate(value) = &flow {
            let parent = self.evaluator.parent_owned_host_scope();
            self.evaluator
                .transfer_owned_host_resources_in_lowered_value(value, parent);
        }
        let cleanup = cleanup_call_scopes(self.evaluator, call);
        self.evaluator.frame_scratch.recycle_block_stacks(call);
        let call_span = call.call_span;
        self.calls.truncate(index);
        self.result = Some(match cleanup {
            Err(error) if error.abort.as_ref().is_some_and(|signal| signal.force) => Err(error),
            Err(error) if flow_leaves_with_error(&flow) => {
                self.evaluator.report_cleanup_error(&error, call_span);
                Ok(LoweredValue::Unit)
            }
            Err(error) => Err(error),
            Ok(()) => Ok(LoweredValue::Unit),
        });
        self.block_flow = Some(flow);
        Ok(())
    }

    fn select_match_arm(
        &mut self,
        index: usize,
        instruction: u32,
        mut position: StatementMatchPosition,
        value: LoweredValue,
        span: Span,
    ) -> Result<(), RuntimeError> {
        loop {
            let (arm, next_position) = statement_match_arm(
                &self.calls[index].execution,
                instruction,
                position,
                span,
            )?;
            let Some((pattern, guard, body)) = arm else {
                break;
            };
            position = next_position;
            let matches = {
                let call = &mut self.calls[index];
                Evaluator::indexed_pattern_matches(
                    &call.execution,
                    pattern,
                    &value,
                    &mut call.slots,
                    span,
                )?
            };
            if !matches {
                continue;
            }
            if let Some(guard) = guard {
                self.push_expr(
                    index,
                    guard,
                    span,
                    FrameContinuation::MatchGuard {
                        instruction,
                        position,
                        body,
                        value,
                        span,
                    },
                );
            } else {
                self.push_statement_block(index, body, span)?;
            }
            return Ok(());
        }
        Err(lowered_match_no_arm(span))
    }

    fn step_for_items(
        &mut self,
        index: usize,
        target: LoweredCompTarget,
        items: Vec<LoweredValue>,
        item_index: usize,
        body: u32,
        span: Span,
    ) -> Result<(), RuntimeError> {
        self.evaluator.service_pending_signal(span)?;
        if self.evaluator.shutting_down() || item_index == items.len() {
            return Ok(());
        }
        bind_lowered_comp_target(
            &target,
            items[item_index].clone(),
            &mut self.calls[index].slots,
            span,
        )?;
        self.calls[index].work.push(FrameWork::ForItems {
            target,
            items,
            index: item_index + 1,
            body,
            span,
        });
        self.push_statement_block(index, body, span)
    }

    fn step_for_str_lines(
        &mut self,
        index: usize,
        slot: usize,
        text: LoweredValue,
        cursor: usize,
        line_count: u32,
        body: u32,
        span: Span,
    ) -> Result<(), RuntimeError> {
        if let Some((bytes, _, end)) = lowered_bytes_parts(&text) {
            if cursor >= end {
                return Ok(());
            }
            let newline = memchr::memchr(b'\n', &bytes[cursor..end]).map(|offset| cursor + offset);
            let line_end = newline.unwrap_or(end);
            let view_end = if line_end > cursor && bytes[line_end - 1] == b'\r' {
                line_end - 1
            } else {
                line_end
            };
            let line_count = line_count.wrapping_add(1);
            if line_count & 63 == 0 {
                self.evaluator.service_pending_signal(span)?;
                if self.evaluator.shutting_down() {
                    return Ok(());
                }
            }
            assign_lowered_bytes_view(&mut self.calls[index].slots[slot], &bytes, cursor, view_end);
            self.calls[index].work.push(FrameWork::ForStrLines {
                slot,
                text,
                cursor: newline.map_or(end, |offset| offset + 1),
                line_count,
                body,
                span,
            });
            return self.push_statement_block(index, body, span);
        }
        let Some((text_value, _, end)) = lowered_str_parts(&text) else {
            return Err(
                RuntimeError::new("type-error", "lowered for lines expected Str or Bytes")
                    .with_span(span),
            );
        };
        if cursor >= end {
            return Ok(());
        }
        let bytes = text_value.as_bytes();
        let newline = memchr::memchr(b'\n', &bytes[cursor..end]).map(|offset| cursor + offset);
        let line_end = newline.unwrap_or(end);
        let view_end = if line_end > cursor && bytes[line_end - 1] == b'\r' {
            line_end - 1
        } else {
            line_end
        };
        let line_count = line_count.wrapping_add(1);
        if line_count & 63 == 0 {
            self.evaluator.service_pending_signal(span)?;
            if self.evaluator.shutting_down() {
                return Ok(());
            }
        }
        assign_lowered_str_view(
            &mut self.calls[index].slots[slot],
            &text_value,
            cursor,
            view_end,
        );
        self.calls[index].work.push(FrameWork::ForStrLines {
            slot,
            text,
            cursor: newline.map_or(end, |offset| offset + 1),
            line_count,
            body,
            span,
        });
        self.push_statement_block(index, body, span)
    }

    fn step_while(
        &mut self,
        index: usize,
        condition: u32,
        body: u32,
        span: Span,
    ) -> Result<(), RuntimeError> {
        self.evaluator.service_pending_signal(span)?;
        if self.evaluator.shutting_down() {
            return Ok(());
        }
        self.push_expr(
            index,
            condition,
            span,
            FrameContinuation::While {
                condition,
                body,
                span,
            },
        );
        Ok(())
    }

    fn eval_retry_delay(
        &mut self,
        index: usize,
        mut state: Box<RetryFrameState>,
        next: Box<FrameContinuation>,
    ) -> Result<(), RuntimeError> {
        let (tag, payload) = self.calls[index]
            .execution
            .instruction_id(state.instruction)
            .map_err(|error| indexed_error(error, state.span))?;
        let super::ExprView::Retry { mut delays, .. } = super::ExprView::decode(
            &self.calls[index].execution,
            tag,
            payload,
        )
        .map_err(|error| indexed_error(error, state.span))?
        else {
            unreachable!("retry instruction has a typed retry view")
        };
        delays
            .resume(state.delay_position)
            .map_err(|error| indexed_error(error, state.span))?;
        let delay = delays
            .next()
            .transpose()
            .map_err(|error| indexed_error(error, state.span))?;
        if let Some(delay) = delay {
            state.delay_position = delays.position();
            self.push_expr(
                index,
                delay.raw(),
                state.span,
                FrameContinuation::RetryDelay(state, next),
            );
            return Ok(());
        }

        let delay_values = std::mem::take(&mut state.delay_values);
        state.delays = Some(match state.schedule_kind {
            crate::syntax::arena::RetrySchedule::Delays => {
                LoweredRetryDelays::Listed(delay_values)
            }
            crate::syntax::arena::RetrySchedule::Backoff => {
                match delay_values.as_slice() {
                    [first, cap, limit] => {
                        LoweredRetryDelays::backoff(first.millis, cap.millis, limit.millis)
                    }
                    _ => {
                        return Err(RuntimeError::new(
                            "type-error",
                            "retry backoff expected a first interval, a cap, and a limit",
                        )
                        .with_span(state.span));
                    }
                }
            }
        });
        state.max_attempts = state
            .delays
            .as_ref()
            .expect("retry schedule was just built")
            .max_attempts();
        self.start_retry_attempt(index, state, next)
    }

    fn start_retry_attempt(
        &mut self,
        index: usize,
        state: Box<RetryFrameState>,
        next: Box<FrameContinuation>,
    ) -> Result<(), RuntimeError> {
        let body = state.body;
        let span = state.span;
        self.calls[index].work.push(FrameWork::ExpressionBoundary {
            policy: ExpressionBoundaryPolicy::Retry(state),
            next: *next,
        });
        self.push_statement_block(index, body, span)
    }

    fn finish_retry_attempt(
        &mut self,
        index: usize,
        state: Box<RetryFrameState>,
        value: LoweredValue,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        let traceback = self.evaluator.pending_traceback.take();
        let value = match value {
            LoweredValue::ResultOk(value) => *value,
            LoweredValue::ResultErr(error) => {
                return self.fail_retry_attempt(index, state, *error, traceback, Box::new(next));
            }
            value => value,
        };
        self.evaluator.trace_lowered_retry_attempt(
            state.span,
            state.attempt_index + 1,
            state.max_attempts,
            None,
            None,
            None,
            Some(crate::trace::RetryStopReason::Success),
        );
        self.evaluator.pending_traceback = None;
        self.push_value(
            index,
            FrameValue::Value(lowered_result_ok(value)),
            next,
        );
        Ok(())
    }

    fn fail_retry_attempt(
        &mut self,
        index: usize,
        mut state: Box<RetryFrameState>,
        error: Value,
        traceback: Option<Traceback>,
        next: Box<FrameContinuation>,
    ) -> Result<(), RuntimeError> {
        let error_value = LoweredValue::Error(Box::new(error));
        let selected = match state.pattern {
            Some(pattern) => {
                let call = &mut self.calls[index];
                Some(Evaluator::indexed_pattern_match_pass(
                    &call.execution,
                    pattern,
                    &error_value,
                    &mut call.slots,
                    state.span,
                    false,
                )?)
            }
            None => None,
        };
        let error = match error_value {
            LoweredValue::Error(error) => *error,
            _ => unreachable!("retry pattern matching wraps the attempt error"),
        };
        let next_delay = if selected == Some(false) {
            None
        } else {
            state
                .delays
                .as_mut()
                .expect("retry schedule was built before attempts")
                .next_delay(state.attempt_index)
        };
        let stop_reason = if selected == Some(false) {
            Some(crate::trace::RetryStopReason::Nonmatching)
        } else if next_delay.is_none() {
            Some(crate::trace::RetryStopReason::Exhausted)
        } else {
            None
        };
        self.evaluator.trace_lowered_retry_attempt(
            state.span,
            state.attempt_index + 1,
            state.max_attempts,
            next_delay,
            Some(super::super::lowered_trace_error_from_value(&error)),
            selected,
            stop_reason,
        );
        state.last_error = Some(error);
        state.last_traceback = traceback;
        self.evaluator.pending_traceback = None;
        let Some(millis) = next_delay else {
            self.finish_retry_error(index, state, next);
            return Ok(());
        };
        state.attempt_index += 1;
        self.evaluator
            .sleep_lowered_retry_delay(&DurationValue { millis }, state.span)?;
        if self.evaluator.shutting_down() {
            self.finish_retry_error(index, state, next);
            return Ok(());
        }
        self.start_retry_attempt(index, state, next)
    }

    fn finish_retry_error(
        &mut self,
        index: usize,
        mut state: Box<RetryFrameState>,
        next: Box<FrameContinuation>,
    ) {
        let error = state
            .last_error
            .take()
            .expect("retry exhaustion retains the final error");
        self.evaluator.pending_traceback = state.last_traceback.take();
        self.push_value(
            index,
            FrameValue::Value(LoweredValue::ResultErr(Box::new(error))),
            *next,
        );
    }

    /// The innermost loop in the frame. A block frame without one hands the
    /// loop control back to the recursive evaluator that entered it.
    fn innermost_loop(&self, index: usize) -> Option<usize> {
        self.calls[index].work.iter().rposition(|work| {
            matches!(
                work,
                FrameWork::ForScalars { .. }
                    | FrameWork::ForMap { .. }
                    | FrameWork::ForItems { .. }
                    | FrameWork::ForStream { .. }
                    | FrameWork::ForPipeline { .. }
                    | FrameWork::ForStrLines { .. }
                    | FrameWork::While { .. }
                    | FrameWork::Loop { .. }
                    | FrameWork::PatternWhile { .. }
            )
        })
    }

    fn break_loop(
        &mut self,
        index: usize,
        value: Option<LoweredValue>,
        transfers_resource: bool,
    ) -> Result<(), RuntimeError> {
        match self.innermost_loop(index) {
            Some(loop_index) => {
                if let Some(value) = value.as_ref() {
                    if self.crosses_context_scope(index, loop_index)
                        && Evaluator::context_scope_value_escapes(value)
                    {
                        self.evaluator.pending_traceback = None;
                        return Err(RuntimeError::new(
                            "context-scope-escape",
                            "a live producer or host handle cannot escape a restored context",
                        )
                        .with_span(self.calls[index].call_span));
                    }
                    if transfers_resource {
                        let body_scope = self.calls[index].work[loop_index + 1..]
                            .iter()
                            .find_map(|work| match work {
                                FrameWork::Statements {
                                    scope_id: Some(scope),
                                    ..
                                } => Some(*scope),
                                _ => None,
                            })
                            .expect("a break inside a loop has an active body scope");
                        let target_scope = self
                            .evaluator
                            .owned_scope_parents
                            .get(&body_scope)
                            .copied()
                            .expect("loop body scope has an ownership parent");
                        self.evaluator
                            .transfer_owned_host_resources_in_lowered_value(
                                value,
                                target_scope,
                            );
                    }
                }
                let completion = match &mut self.calls[index].work[loop_index] {
                    FrameWork::Loop { completion, .. } => completion.take(),
                    _ => None,
                };
                if let Some(next) = completion {
                    self.calls[index].work[loop_index] = FrameWork::Value {
                        value: FrameValue::Value(value.unwrap_or(LoweredValue::Unit)),
                        next: *next,
                    };
                    self.discard_work_from(index, loop_index + 1)
                } else {
                    self.discard_work_from(index, loop_index)
                }
            }
            None if matches!(self.calls[index].owner, FrameOwner::Block { .. }) => {
                if let Some(value) = value.as_ref() {
                    self.check_context_scope_value_escape(index, value)?;
                    if transfers_resource {
                        let target_scope = match self.calls[index].owner {
                            FrameOwner::Block { outer_scope, .. } => outer_scope,
                            FrameOwner::Function(..) => unreachable!(),
                        };
                        self.evaluator
                            .transfer_owned_host_resources_in_lowered_value(
                                value,
                                target_scope,
                            );
                    }
                }
                self.complete_call(index, StmtFlow::Break(value))
            }
            None => Err(RuntimeError::new("control-flow", "break outside loop")
                .with_span(self.calls[index].call_span)),
        }
    }

    fn step_for_stream(
        &mut self,
        index: usize,
        target: LoweredCompTarget,
        mut stream: StreamValue,
        body: u32,
        span: Span,
    ) -> Result<(), RuntimeError> {
        self.evaluator.service_pending_signal(span)?;
        if self.evaluator.shutting_down() {
            self.evaluator.stream_cancel(&mut stream, span)?;
            return Ok(());
        }
        let Some(value) = self.evaluator.stream_next(&mut stream, span)? else {
            return Ok(());
        };
        let Some(item) = lowered_value_from_runtime_any(&value) else {
            return Err(RuntimeError::new(
                "type-error",
                format!("stream produced unsupported {}", value.type_name()),
            )
            .with_span(span));
        };
        if let Err(error) =
            bind_lowered_comp_target(&target, item, &mut self.calls[index].slots, span)
        {
            self.evaluator.stream_cancel(&mut stream, span)?;
            return Err(error);
        }
        // Re-arm before running the body, so a `continue` reaches the next item
        // and a `break` discards this item and stops the producer.
        self.calls[index].work.push(FrameWork::ForStream {
            target,
            stream,
            body,
            span,
        });
        self.push_statement_block(index, body, span)?;
        Ok(())
    }

    fn step_for_pipeline(
        &mut self,
        index: usize,
        slot: usize,
        mut pipeline: Box<IndexedSerialPipeline>,
        body: u32,
        span: Span,
    ) -> Result<(), RuntimeError> {
        if let Err(error) = self.evaluator.service_pending_signal(span) {
            let _ = pipeline.finish(self.evaluator, Some(&error));
            return Err(error);
        }
        if self.evaluator.shutting_down() {
            return pipeline.finish(self.evaluator, None);
        }
        let pulled = {
            let call = &mut self.calls[index];
            pipeline.next_frame(self.evaluator, &call.execution, &mut call.slots)
        };
        match pulled {
            Ok(IndexedSerialStep::Item(Some(item))) => {
                self.calls[index].slots[slot] = item;
                self.calls[index].work.push(FrameWork::ForPipeline {
                    slot,
                    pipeline,
                    body,
                    span,
                });
                self.push_statement_block(index, body, span)
            }
            Ok(IndexedSerialStep::Item(None)) => {
                pipeline.finish(self.evaluator, None)
            }
            Ok(IndexedSerialStep::Expr(expression)) => {
                self.calls[index].work.push(FrameWork::ForPipeline {
                    slot,
                    pipeline,
                    body,
                    span,
                });
                self.push_expr(index, expression, span, FrameContinuation::StageResult);
                Ok(())
            }
            Ok(IndexedSerialStep::Block(block)) => {
                self.calls[index].work.push(FrameWork::ForPipeline {
                    slot,
                    pipeline,
                    body,
                    span,
                });
                self.push_statement_block(index, block, span)
            }
            Ok(IndexedSerialStep::Break(value)) => {
                pipeline.finish(self.evaluator, None)?;
                self.complete_call(index, StmtFlow::Return(value))
            }
            Err(error) => {
                let stage_item = pipeline
                    .pending_map_error_item()
                    .map(|item_index| ("map", item_index));
                let _ = pipeline.finish(self.evaluator, Some(&error));
                Err(match stage_item {
                    Some((stage, item_index)) => self
                        .evaluator
                        .stream_item_runtime_error_before_unwind(stage, item_index, error),
                    None => error,
                })
            }
        }
    }

    fn step_stage_value(
        &mut self,
        index: usize,
        mut pipeline: Box<IndexedStageEngine>,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        if let Err(error) = self.evaluator.service_pending_signal(span) {
            let _ = pipeline.finish(self.evaluator, Some(&error));
            return Err(error);
        }
        if self.evaluator.shutting_down() {
            return pipeline.finish(self.evaluator, None);
        }
        let pulled = {
            let call = &mut self.calls[index];
            pipeline.next_frame(self.evaluator, &call.execution, &mut call.slots)
        };
        match pulled {
            Ok(IndexedStageStep::Done(value)) => {
                pipeline.finish(self.evaluator, None)?;
                self.push_value(index, FrameValue::Value(value), next);
                Ok(())
            }
            Ok(IndexedStageStep::Expr(expression)) => {
                self.calls[index].work.push(FrameWork::StageValue {
                    pipeline,
                    span,
                    next,
                });
                self.push_expr(index, expression, span, FrameContinuation::StageResult);
                Ok(())
            }
            Ok(IndexedStageStep::Block(block)) => {
                self.calls[index].work.push(FrameWork::StageValue {
                    pipeline,
                    span,
                    next,
                });
                self.push_statement_block(index, block, span)
            }
            Ok(IndexedStageStep::Break(value)) => {
                pipeline.finish(self.evaluator, None)?;
                self.push_value(index, FrameValue::Break(value), next);
                Ok(())
            }
            Err(error) => {
                let stage_item = pipeline.pending_item_error();
                let _ = pipeline.finish(self.evaluator, Some(&error));
                Err(match stage_item {
                    Some((stage, item_index)) => self
                        .evaluator
                        .stream_item_runtime_error_before_unwind(stage, item_index, error),
                    None => error,
                })
            }
        }
    }

    fn resume_stage_result(
        &mut self,
        index: usize,
        result: ControlFlow<LoweredValue, LoweredValue>,
    ) {
        match self.calls[index].work.last_mut() {
            Some(FrameWork::ForPipeline { pipeline, .. }) => pipeline.resume(result),
            Some(FrameWork::StageValue { pipeline, .. }) => pipeline.resume(result),
            _ => unreachable!("stage expression continuation follows its frame work"),
        }
    }

    fn select_expr_match_arm(
        &mut self,
        index: usize,
        instruction: u32,
        mut arms: MatchExprPosition,
        value: LoweredValue,
        span: Span,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        loop {
            let Some(((pattern, guard, body), position)) = match_expr_arm_at(
                &self.calls[index].execution,
                instruction,
                arms,
                span,
            )? else {
                return Err(lowered_match_no_arm(span));
            };
            arms = position;
            let matches = {
                let call = &mut self.calls[index];
                Evaluator::indexed_pattern_matches(
                    &call.execution,
                    pattern,
                    &value,
                    &mut call.slots,
                    span,
                )?
            };
            if !matches {
                continue;
            }
            if let Some(guard) = guard {
                self.push_expr(
                    index,
                    guard,
                    span,
                    FrameContinuation::MatchExprGuard {
                        instruction,
                        arms,
                        value,
                        body,
                        span,
                        next: Box::new(next),
                    },
                );
            } else {
                self.push_expr(index, body, span, next);
            }
            return Ok(());
        }
    }

    fn continue_loop(&mut self, index: usize) -> Result<(), RuntimeError> {
        match self.innermost_loop(index) {
            Some(loop_index) => self.discard_work_from(index, loop_index + 1),
            None if matches!(self.calls[index].owner, FrameOwner::Block { .. }) => {
                self.complete_call(index, StmtFlow::Continue)
            }
            None => Err(RuntimeError::new("control-flow", "continue outside loop")
                .with_span(self.calls[index].call_span)),
        }
    }

    fn function_kind(
        &self,
        index: usize,
        function: LoweredFunctionKey,
        span: Span,
    ) -> Result<LoweredFunctionKind, RuntimeError> {
        let program = self.calls[index].execution.program();
        if program
            .function_view(function, LoweredFunctionKind::Pure)
            .map_err(|error| indexed_error(error, span))?
            .is_some()
        {
            Ok(LoweredFunctionKind::Pure)
        } else if program
            .function_view(function, LoweredFunctionKind::Proc)
            .map_err(|error| indexed_error(error, span))?
            .is_some()
        {
            Ok(LoweredFunctionKind::Proc)
        } else {
            Err(
                RuntimeError::new("unresolved-lowered-call", function.display_name())
                    .with_span(span),
            )
        }
    }

    fn external_kind(
        &self,
        index: usize,
        qualified: crate::symbol::QualifiedName,
        span: Span,
    ) -> Result<LoweredFunctionKind, RuntimeError> {
        let function = LoweredFunctionKey::Qualified(qualified);
        let program = self.calls[index].execution.program();
        for kind in [LoweredFunctionKind::Pure, LoweredFunctionKind::Proc] {
            if program
                .function_view(function, kind)
                .map_err(|error| indexed_error(error, span))?
                .is_some()
            {
                return Ok(kind);
            }
        }
        self.evaluator
            .indexed_dynamic_functions
            .get(&qualified)
            .map(|dynamic| dynamic.kind)
            .ok_or_else(|| {
                RuntimeError::new("unresolved-lowered-call", qualified.to_string())
                    .with_span(span)
            })
    }

    fn advance_assign_path(
        &mut self,
        index: usize,
        mut state: AssignPathState,
    ) -> Result<(), RuntimeError> {
        let next_selector = match state.path {
            AssignPathCursor::DirectField(name) => {
                state.path = AssignPathCursor::Exhausted;
                state.selectors.push(ResolvedAssignStep::Field(name));
                None
            }
            AssignPathCursor::Encoded {
                statement,
                position,
            } => {
                let mut path = indexed_assign_path_view_at(
                    &self.calls[index].execution,
                    statement,
                    state.span,
                )?;
                path.resume(position)
                    .map_err(|error| indexed_error(error, state.span))?;
                let mut next_selector = None;
                loop {
                    let step = path
                        .next()
                        .transpose()
                        .map_err(|error| indexed_error(error, state.span))?;
                    let position = path.position();
                    match step {
                        Some(AssignStepView::Field(name)) => {
                            state.selectors.push(ResolvedAssignStep::Field(name));
                            state.path = AssignPathCursor::Encoded {
                                statement,
                                position,
                            };
                        }
                        Some(AssignStepView::Index(expr)) => {
                            state.path = AssignPathCursor::Encoded {
                                statement,
                                position,
                            };
                            next_selector = Some(expr.raw());
                            break;
                        }
                        None => {
                            state.path = AssignPathCursor::Exhausted;
                            break;
                        }
                    }
                }
                next_selector
            }
            AssignPathCursor::Exhausted => None,
        };
        if let Some(selector) = next_selector {
            self.push_expr(
                index,
                selector,
                state.span,
                FrameContinuation::AssignSelector(state),
            );
        } else {
            self.push_expr(
                index,
                state.value,
                state.span,
                FrameContinuation::AssignPath(state),
            );
        }
        Ok(())
    }

    fn push_expr(&mut self, index: usize, instruction: u32, span: Span, next: FrameContinuation) {
        self.calls[index].work.push(FrameWork::Expr {
            instruction,
            span,
            next,
        });
    }

    fn push_value(&mut self, index: usize, value: FrameValue, next: FrameContinuation) {
        self.calls[index]
            .work
            .push(FrameWork::Value { value, next });
    }

    fn step_process_command_builder(
        &mut self,
        index: usize,
        instruction: u32,
        entries: ListPosition<LoweredProcessCommandBuilderEntry>,
        mut state: Box<ProcessCommandBuilderState>,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        let (entry, _, _) = process_command_builder_entry(
            &self.calls[index].execution,
            instruction,
            entries,
            state.call_span,
        )?;
        match entry {
            Some(ProcessCommandBuilderEntryView::Field { value, span, .. }) => {
                self.push_expr(
                    index,
                    value.raw(),
                    span,
                    FrameContinuation::ProcessCommandBuilder {
                        instruction,
                        entry: entries,
                        phase: ProcessCommandBuilderPhase::Field,
                        state,
                        next: Box::new(next),
                    },
                );
            }
            Some(ProcessCommandBuilderEntryView::Run {
                args,
                env,
                span,
                ..
            }) => {
                if state.plan.is_some() {
                    return Err(RuntimeError::new(
                        "builder-entry",
                        "process.command accepts one run entry",
                    )
                    .with_span(span));
                }
                state.plan = Some(super::empty_command_plan());
                state.run = Some(ProcessCommandRunState {
                    args: args.position(),
                    env_assignments: env.position(),
                });
                self.step_process_command_run(
                    index,
                    instruction,
                    entries,
                    ProcessCommandRunPhase::Target,
                    state,
                    next,
                )?;
            }
            None => self.finish_process_command_builder(index, state, next)?,
        }
        Ok(())
    }

    fn step_process_command_run(
        &mut self,
        index: usize,
        instruction: u32,
        entry: ListPosition<LoweredProcessCommandBuilderEntry>,
        mut phase: ProcessCommandRunPhase,
        mut state: Box<ProcessCommandBuilderState>,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        loop {
            let (operand, run_span, finished) = {
                let (entry, _, builder_span) = process_command_builder_entry(
                    &self.calls[index].execution,
                    instruction,
                    entry,
                    state.call_span,
                )?;
                match entry {
                    Some(ProcessCommandBuilderEntryView::Run {
                        target,
                        mut args,
                        mut env,
                        timeout,
                        cpu_max,
                        accept,
                        span,
                    }) => {
                        let finished = phase == ProcessCommandRunPhase::Accept && accept.is_none();
                        let operand = match phase {
                            ProcessCommandRunPhase::Target => {
                                Some(run_arg_from_view(target).value)
                            }
                            ProcessCommandRunPhase::Argument => {
                                let cursor = state
                                    .run
                                    .as_ref()
                                    .expect("active process command run")
                                    .args;
                                args.resume(cursor)
                                    .map_err(|error| indexed_error(error, builder_span))?;
                                args.next()
                                    .transpose()
                                    .map_err(|error| indexed_error(error, builder_span))?
                                    .map(run_arg_from_view)
                                    .map(|arg| arg.value)
                            }
                            ProcessCommandRunPhase::Environment => {
                                let cursor = state
                                    .run
                                    .as_ref()
                                    .expect("active process command run")
                                    .env_assignments;
                                env.resume(cursor)
                                    .map_err(|error| indexed_error(error, builder_span))?;
                                env.next()
                                    .transpose()
                                    .map_err(|error| indexed_error(error, builder_span))?
                                    .map(|assignment| run_arg_from_view(assignment.value))
                                    .map(|arg| arg.value)
                            }
                            ProcessCommandRunPhase::Timeout => timeout.map(|value| value.raw()),
                            ProcessCommandRunPhase::CpuMax => cpu_max.map(|value| value.raw()),
                            ProcessCommandRunPhase::Accept => accept.map(|value| value.raw()),
                        };
                        (operand, span, finished)
                    }
                    _ => unreachable!("process command run cursor has a run entry"),
                }
            };
            if let Some(operand) = operand {
                self.push_expr(
                    index,
                    operand,
                    run_span,
                    FrameContinuation::ProcessCommandBuilder {
                        instruction,
                        entry,
                        phase: ProcessCommandBuilderPhase::Run(phase),
                        state,
                        next: Box::new(next),
                    },
                );
                return Ok(());
            }
            if finished {
                state.run = None;
                let (_, entries, _) = process_command_builder_entry(
                    &self.calls[index].execution,
                    instruction,
                    entry,
                    state.call_span,
                )?;
                return self.step_process_command_builder(index, instruction, entries, state, next);
            }
            phase = match phase {
                ProcessCommandRunPhase::Target => unreachable!("run target is required"),
                ProcessCommandRunPhase::Argument => ProcessCommandRunPhase::Environment,
                ProcessCommandRunPhase::Environment => ProcessCommandRunPhase::Timeout,
                ProcessCommandRunPhase::Timeout => ProcessCommandRunPhase::CpuMax,
                ProcessCommandRunPhase::CpuMax => ProcessCommandRunPhase::Accept,
                ProcessCommandRunPhase::Accept => unreachable!("accept phase completed"),
            };
        }
    }

    fn finish_process_command_builder(
        &mut self,
        index: usize,
        state: Box<ProcessCommandBuilderState>,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        let mut plan = state.plan.ok_or_else(|| {
            RuntimeError::new("builder-check", "process.command requires a run entry")
                .with_span(state.span)
        })?;
        if state.cwd.is_some() {
            plan.cwd = state.cwd;
        }
        plan.env.extend(state.env);
        plan.redirections.extend(super::lowered_command_redirections(
            state.stdin,
            state.stdout,
            state.stderr,
            state.stdout_append,
            state.stderr_append,
            "process.command",
            state.span,
        )?);
        if state.timeout.is_some() {
            plan.timeout = state.timeout;
        }
        if state.cpu_max.is_some() {
            plan.cpu_max = state.cpu_max;
        }
        if state.accepted_exit_codes.is_some() {
            if plan.accepted_exit_codes.is_some() {
                return Err(RuntimeError::new(
                    "accept-policy",
                    "accept cannot be supplied both as a field and a run option",
                )
                .with_span(state.span));
            }
            plan.accepted_exit_codes = state.accepted_exit_codes;
        }
        if let Some(value) = state.detach {
            plan.detach = value;
        }
        if let Some(value) = state.new_session {
            plan.new_session = value;
        }
        if let Some(value) = state.ignore_hup {
            plan.ignore_hup = value;
        }
        if let Some(value) = state.same_group {
            plan.same_group = value;
        }
        self.evaluator.trace_exit(
            TraceKind::ModuleResult,
            Some(state.span),
            Some("process.command"),
            TracePayload::None,
        );
        self.push_value(
            index,
            FrameValue::Value(LoweredValue::Command(Box::new(plan))),
            next,
        );
        Ok(())
    }

    fn continue_process_command_builder(
        &mut self,
        index: usize,
        instruction: u32,
        entry: ListPosition<LoweredProcessCommandBuilderEntry>,
        phase: ProcessCommandBuilderPhase,
        mut state: Box<ProcessCommandBuilderState>,
        value: LoweredValue,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        match phase {
            ProcessCommandBuilderPhase::Field => {
                let (entry_view, remaining, _) = process_command_builder_entry(
                    &self.calls[index].execution,
                    instruction,
                    entry,
                    state.call_span,
                )?;
                let Some(ProcessCommandBuilderEntryView::Field { name, span, .. }) = entry_view else {
                    unreachable!("field phase has a field entry")
                };
                let (operand, field) = match name.as_str().as_str() {
                    "cwd" => (Some(super::FrameApplyOperand::PathLike("process.command")), "cwd"),
                    "env" => (Some(super::FrameApplyOperand::Environment("process.command")), "env"),
                    "stdin" => (None, "stdin"),
                    "stdout" => (None, "stdout"),
                    "stderr" => (None, "stderr"),
                    "stdout_append" => (Some(super::FrameApplyOperand::BuilderBool("stdout_append")), "stdout_append"),
                    "stderr_append" => (Some(super::FrameApplyOperand::BuilderBool("stderr_append")), "stderr_append"),
                    "timeout" => (Some(super::FrameApplyOperand::Duration("process.command")), "timeout"),
                    "accept" => (Some(super::FrameApplyOperand::AcceptedExitCodes), "accept"),
                    "cpu_max" => (Some(super::FrameApplyOperand::PositiveInt("process.command")), "cpu_max"),
                    "detach" => (Some(super::FrameApplyOperand::BuilderBool("detach")), "detach"),
                    "new_session" => (Some(super::FrameApplyOperand::BuilderBool("new_session")), "new_session"),
                    "ignore_hup" => (Some(super::FrameApplyOperand::BuilderBool("ignore_hup")), "ignore_hup"),
                    "same_group" => (Some(super::FrameApplyOperand::BuilderBool("same_group")), "same_group"),
                    _ => {
                        return Err(RuntimeError::new(
                            "builder-field",
                            format!("unknown process.command field `{name}`"),
                        )
                        .with_span(span));
                    }
                };
                let value = super::indexed_frame_apply_operand(value, operand, span)?;
                match (field, value) {
                    ("cwd", super::FrameApplyValue::Path(value)) => state.cwd = Some(value),
                    ("env", super::FrameApplyValue::Environment(value)) => state.env.extend(value),
                    ("stdin", super::FrameApplyValue::Value(value)) => state.stdin = Some(value),
                    ("stdout", super::FrameApplyValue::Value(value)) => state.stdout = Some(value),
                    ("stderr", super::FrameApplyValue::Value(value)) => state.stderr = Some(value),
                    ("stdout_append", super::FrameApplyValue::Bool(value)) => state.stdout_append = value,
                    ("stderr_append", super::FrameApplyValue::Bool(value)) => state.stderr_append = value,
                    ("timeout", super::FrameApplyValue::Duration(value)) => state.timeout = Some(value),
                    ("accept", super::FrameApplyValue::AcceptedExitCodes(value)) => state.accepted_exit_codes = Some(value),
                    ("cpu_max", super::FrameApplyValue::Int(value)) => state.cpu_max = Some(value),
                    ("detach", super::FrameApplyValue::Bool(value)) => state.detach = Some(value),
                    ("new_session", super::FrameApplyValue::Bool(value)) => state.new_session = Some(value),
                    ("ignore_hup", super::FrameApplyValue::Bool(value)) => state.ignore_hup = Some(value),
                    ("same_group", super::FrameApplyValue::Bool(value)) => state.same_group = Some(value),
                    _ => unreachable!("process command builder conversion matches its field"),
                }
                self.step_process_command_builder(index, instruction, remaining, state, next)
            }
            ProcessCommandBuilderPhase::Run(phase) => self.continue_process_command_run(
                index, instruction, entry, phase, state, value, next,
            ),
        }
    }

    fn continue_process_command_run(
        &mut self,
        index: usize,
        instruction: u32,
        entry: ListPosition<LoweredProcessCommandBuilderEntry>,
        phase: ProcessCommandRunPhase,
        mut state: Box<ProcessCommandBuilderState>,
        value: LoweredValue,
        next: FrameContinuation,
    ) -> Result<(), RuntimeError> {
        let (entry_view, _, builder_span) = process_command_builder_entry(
            &self.calls[index].execution,
            instruction,
            entry,
            state.call_span,
        )?;
        let Some(ProcessCommandBuilderEntryView::Run {
            target,
            mut args,
            mut env,
            timeout: _,
            cpu_max: _,
            accept: _,
            span,
        }) = entry_view
        else {
            unreachable!("run phase has a run entry")
        };
        let run = state.run.as_mut().expect("active process command run");
        let plan = state
            .plan
            .as_mut()
            .expect("process command plan exists during run");
        let next_phase = match phase {
            ProcessCommandRunPhase::Target => {
                let arg = run_arg_from_view(target);
                let items = Evaluator::run_arg_items(&arg, value.into_value())?;
                let (target, leading) =
                    super::process::run_target_and_leading_argv_for_arg(&arg, items)?;
                plan.target = target;
                plan.argv.extend(leading);
                ProcessCommandRunPhase::Argument
            }
            ProcessCommandRunPhase::Argument => {
                args.resume(run.args)
                    .map_err(|error| indexed_error(error, builder_span))?;
                let arg = args
                    .next()
                    .transpose()
                    .map_err(|error| indexed_error(error, builder_span))?
                    .map(run_arg_from_view)
                    .expect("scheduled process command argument exists");
                run.args = args.position();
                plan.argv
                    .extend(Evaluator::run_arg_items(&arg, value.into_value())?);
                ProcessCommandRunPhase::Argument
            }
            ProcessCommandRunPhase::Environment => {
                env.resume(run.env_assignments)
                    .map_err(|error| indexed_error(error, builder_span))?;
                let assignment = env
                    .next()
                    .transpose()
                    .map_err(|error| indexed_error(error, builder_span))?
                    .expect("scheduled process command environment entry exists");
                let name = assignment.name.as_str().as_str().to_owned();
                let arg = run_arg_from_view(assignment.value);
                run.env_assignments = env.position();
                plan.env
                    .insert(name, Evaluator::run_env_value(&arg, value.into_value())?);
                ProcessCommandRunPhase::Environment
            }
            ProcessCommandRunPhase::Timeout => {
                let super::FrameApplyValue::Duration(value) = super::indexed_frame_apply_operand(
                    value,
                    Some(super::FrameApplyOperand::Duration("process.command")),
                    span,
                )? else {
                    unreachable!("process command timeout conversion returns a duration")
                };
                plan.timeout = Some(value);
                ProcessCommandRunPhase::CpuMax
            }
            ProcessCommandRunPhase::CpuMax => {
                let super::FrameApplyValue::Int(value) = super::indexed_frame_apply_operand(
                    value,
                    Some(super::FrameApplyOperand::PositiveInt("process.command")),
                    span,
                )? else {
                    unreachable!("process command cpu_max conversion returns an integer")
                };
                plan.cpu_max = Some(value);
                ProcessCommandRunPhase::Accept
            }
            ProcessCommandRunPhase::Accept => {
                let super::FrameApplyValue::AcceptedExitCodes(value) =
                    super::indexed_frame_apply_operand(
                        value,
                        Some(super::FrameApplyOperand::AcceptedExitCodes),
                        span,
                    )?
                else {
                    unreachable!("process command accept conversion returns exit codes")
                };
                plan.accepted_exit_codes = Some(value);
                ProcessCommandRunPhase::Accept
            }
        };
        if phase == ProcessCommandRunPhase::Accept {
            state.run = None;
            let (_, entries, _) = process_command_builder_entry(
                &self.calls[index].execution,
                instruction,
                entry,
                state.call_span,
            )?;
            self.step_process_command_builder(index, instruction, entries, state, next)
        } else {
            self.step_process_command_run(
                index,
                instruction,
                entry,
                next_phase,
                state,
                next,
            )
        }
    }

    /// Opens the owned scope of one pattern condition and its body: the
    /// captures clear and the scope closes when the work above them finishes
    /// or unwinds.
    fn open_pattern_scope(&mut self, index: usize, captures: Vec<usize>) {
        let scope_id = self.evaluator.enter_owned_host_scope();
        let defer_offset = self.calls[index].defers.len();
        self.calls[index].block_scopes.push(scope_id);
        self.calls[index].block_defer_offsets.push(defer_offset);
        self.calls[index].work.push(FrameWork::Statements {
            cursor: StatementCursor::empty(),
            complete_call: false,
            scope_id: Some(scope_id),
        });
        self.calls[index].work.push(FrameWork::ClearSlots(captures));
    }

    /// Closes the scope `open_pattern_scope` left on top of the work stack
    /// after a condition that did not match.
    fn close_pattern_scope(&mut self, index: usize) -> Result<(), RuntimeError> {
        let Some(FrameWork::ClearSlots(captures)) = self.calls[index].work.pop() else {
            unreachable!("pattern condition owns its capture cleanup");
        };
        for slot in captures {
            self.calls[index].slots[slot] = LoweredValue::Unit;
        }
        let Some(FrameWork::Statements {
            scope_id: Some(scope_id),
            ..
        }) = self.calls[index].work.pop()
        else {
            unreachable!("pattern condition owns its scope");
        };
        self.exit_block_scope(
            index,
            scope_id,
            false,
            true,
            CleanupFailureResources::Retain,
        )
    }

    fn start_expr_pattern_branch(
        &mut self,
        index: usize,
        instruction: u32,
        position: ExprPatternIfPosition,
        span: Span,
        next: Box<FrameContinuation>,
    ) -> Result<(), RuntimeError> {
        let (condition, captures, else_value) = {
            let view = expr_pattern_if_view(&self.calls[index].execution, instruction, span)?;
            let super::ExprView::PatternIf {
                mut branches,
                else_value,
                ..
            } = view
            else {
                unreachable!("pattern-if instruction has a matching borrowed expression view");
            };
            branches
                .resume(position)
                .map_err(|error| indexed_error(error, span))?;
            let branch = branches
                .next()
                .transpose()
                .map_err(|error| indexed_error(error, span))?;
            let else_value = else_value.raw();
            match branch {
                Some((condition, _, captures)) => {
                    let captures = captures
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|error| indexed_error(error, span))?;
                    (Some(condition.raw()), captures, else_value)
                }
                None => (None, Vec::new(), else_value),
            }
        };
        let Some(condition) = condition else {
            self.push_expr(index, else_value, span, *next);
            return Ok(());
        };
        self.open_pattern_scope(index, captures);
        self.push_expr(
            index,
            condition,
            span,
            FrameContinuation::ExprPatternIf {
                instruction,
                position,
                span,
                next,
                step: ExprPatternIfStep::Condition,
            },
        );
        Ok(())
    }

    fn start_if_expr(
        &mut self,
        index: usize,
        instruction: u32,
        position: IfExprPosition,
        else_value: u32,
        span: Span,
        decode_span: Span,
        next: Box<FrameContinuation>,
    ) -> Result<(), RuntimeError> {
        let (branch, position) = {
            let mut branches =
                expr_if_branches(&self.calls[index].execution, instruction, decode_span)?;
            branches
                .resume(position)
                .map_err(|error| indexed_error(error, decode_span))?;
            let first = branches
                .next()
                .transpose()
                .map_err(|error| indexed_error(error, decode_span))?;
            let position = branches.position();
            // Validate future rows before any condition can run effects, without
            // retaining their expression IDs.
            for branch in branches {
                branch.map_err(|error| indexed_error(error, decode_span))?;
            }
            (
                first.map(|(condition, body)| (condition.raw(), body.raw())),
                position,
            )
        };
        if let Some((condition, current_body)) = branch {
            self.push_expr(
                index,
                condition,
                span,
                FrameContinuation::IfExpr {
                    instruction,
                    position,
                    current_body,
                    else_value,
                    span,
                    next,
                },
            );
        } else {
            self.push_expr(index, else_value, span, *next);
        }
        Ok(())
    }

    fn if_expr_condition_step(
        &mut self,
        index: usize,
        instruction: u32,
        position: IfExprPosition,
        else_value: u32,
        span: Span,
        next: Box<FrameContinuation>,
    ) -> Result<(), RuntimeError> {
        let branch = {
            let mut branches = expr_if_branches(&self.calls[index].execution, instruction, span)?;
            branches
                .resume(position)
                .map_err(|error| indexed_error(error, span))?;
            let branch = branches
                .next()
                .transpose()
                .map_err(|error| indexed_error(error, span))?;
            let position = branches.position();
            branch.map(|(condition, body)| (condition.raw(), body.raw(), position))
        };
        if let Some((condition, current_body, position)) = branch {
            self.push_expr(
                index,
                condition,
                span,
                FrameContinuation::IfExpr {
                    instruction,
                    position,
                    current_body,
                    else_value,
                    span,
                    next,
                },
            );
        } else {
            self.push_expr(index, else_value, span, *next);
        }
        Ok(())
    }

    fn expr_pattern_if_branch_step(
        &self,
        index: usize,
        instruction: u32,
        position: ExprPatternIfPosition,
        span: Span,
    ) -> Result<(u32, ExprPatternIfPosition), RuntimeError> {
        let view = expr_pattern_if_view(&self.calls[index].execution, instruction, span)?;
        let super::ExprView::PatternIf { mut branches, .. } = view else {
            unreachable!("pattern-if instruction has a matching borrowed expression view");
        };
        branches
            .resume(position)
            .map_err(|error| indexed_error(error, span))?;
        let (_, value, _) = branches
            .next()
            .transpose()
            .map_err(|error| indexed_error(error, span))?
            .ok_or_else(|| {
                RuntimeError::new(
                    "indexed-ir",
                    "indexed IR verification failed: pattern-if cursor is exhausted",
                )
                .with_span(span)
            })?;
        Ok((value.raw(), branches.position()))
    }

    fn start_pattern_branch(
        &mut self,
        index: usize,
        instruction: u32,
        position: StatementPatternIfPosition,
        else_body: Option<u32>,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let (branch, position) = statement_pattern_if_branch(
            &self.calls[index].execution,
            instruction,
            position,
            span,
        )?;
        let Some((condition, body, captures)) = branch else {
            return match else_body {
                Some(body) => self.push_statement_block(index, body, span),
                None => Ok(()),
            };
        };
        self.open_pattern_scope(index, captures);
        self.push_expr(
            index,
            condition,
            span,
            FrameContinuation::PatternIf {
                instruction,
                position,
                body,
                else_body,
                span,
            },
        );
        Ok(())
    }

    fn push_statement_block(
        &mut self,
        index: usize,
        body: u32,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let cursor = StatementCursor::block(&self.calls[index].execution, body, span)?;
        let scope_id = self.evaluator.enter_owned_host_scope();
        let defer_offset = self.calls[index].defers.len();
        if self.calls[index].block_scopes.capacity() == 0 {
            let call = &mut self.calls[index];
            (call.block_scopes, call.block_defer_offsets) =
                self.evaluator.frame_scratch.take_block_stacks();
        }
        self.calls[index].block_scopes.push(scope_id);
        self.calls[index].block_defer_offsets.push(defer_offset);
        self.calls[index].work.push(FrameWork::Statements {
            cursor,
            complete_call: false,
            scope_id: Some(scope_id),
        });
        Ok(())
    }

    /// Closes a block scope and runs the deferred actions it registered.
    /// `leaves_with_error` is whether control is leaving the block with an
    /// error, which is when its `errdefer` actions run.
    fn exit_block_scope(
        &mut self,
        index: usize,
        scope_id: u64,
        leaves_with_error: bool,
        include_work_contexts: bool,
        disposition: CleanupFailureResources,
    ) -> Result<(), RuntimeError> {
        let defer_offset = self.calls[index]
            .block_defer_offsets
            .pop()
            .expect("live block owns a defer boundary");
        let during_discard = matches!(
            self.calls[index].work.last(),
            Some(FrameWork::Discard(_))
        );
        if self.calls[index].defers.len() == defer_offset {
            let popped = self.calls[index].block_scopes.pop();
            debug_assert_eq!(popped, Some(scope_id));
            let result = self.evaluator.exit_owned_host_scope(scope_id);
            if during_discard {
                let Some(FrameWork::Discard(mut state)) = self.calls[index].work.pop() else {
                    unreachable!("discarded block scope resumes its discard cursor");
                };
                if let Err(error) = result {
                    if error.abort.as_ref().is_some_and(|signal| signal.force) {
                        self.calls[index].work.push(FrameWork::Discard(state));
                        return Err(error);
                    }
                    let traceback = self.evaluator.pending_traceback.take();
                    self.record_discard_error(&mut state, error, traceback);
                }
                self.calls[index].work.push(FrameWork::Discard(state));
                return Ok(());
            }
            return result;
        }
        let defers = self.calls[index].defers.split_off(defer_offset);
        // Values leaving the block moved their handles outward before this
        // exit, so what the block still owns is dropped, and is released
        // before the block's deferred actions run.
        let released = if self.evaluator.current_scope_id() == scope_id {
            self.evaluator.release_owned_host_resources(scope_id)
        } else {
            Ok(())
        };
        let cleanup_contexts = self.collect_cleanup_contexts();
        let group = CleanupGroup {
            next_defer: defers.len(),
            defers,
            leaves_with_error,
            first_error: None,
            first_traceback: None,
            primary_error: None,
            primary_traceback: self.evaluator.pending_traceback.take(),
            released: released.err(),
            cleanup_contexts,
            call_span: self.calls[index].call_span,
            resume: CleanupGroupResume::Block {
                scope_id,
                disposition,
                during_discard,
                include_work_contexts,
            },
        };
        self.calls[index]
            .work
            .push(FrameWork::CleanupGroup(Box::new(group)));
        Ok(())
    }

    /// Whether what is unwinding the frames is `exit 0`. The script is ending
    /// in success, so the scopes it unwinds do not leave with an error and
    /// their `errdefer` actions stay skipped; a nonzero `exit`, a failure, and
    /// a cancellation are errors to them.
    fn unwinding_successful_exit(&self) -> bool {
        self.pending_error
            .as_ref()
            .and_then(|error| error.abort.as_ref())
            .is_some_and(|signal| signal.status == 0)
    }

    fn discard_work_from(&mut self, index: usize, keep: usize) -> Result<(), RuntimeError> {
        self.schedule_discard(index, keep, false, DiscardResume::Continue);
        Ok(())
    }

    fn schedule_discard(
        &mut self,
        index: usize,
        keep: usize,
        primary_failed: bool,
        resume: DiscardResume,
    ) {
        if self.calls[index].work.len() <= keep && matches!(resume, DiscardResume::Continue) {
            return;
        }
        let primary_is_error = primary_failed && !self.unwinding_successful_exit();
        let primary_error = if matches!(resume, DiscardResume::UnwindError { .. }) {
            self.pending_error.take()
        } else {
            None
        };
        let primary_traceback = self.evaluator.pending_traceback.take();
        let contexts = self.collect_cleanup_contexts();
        let previous_cleanup_contexts = std::mem::replace(
            &mut self.evaluator.cleanup_error_contexts,
            contexts,
        );
        let context_scopes = self.calls[index].work[keep..]
            .iter()
            .filter(|work| work.is_context_scope())
            .count();
        let next_to_discard = self.calls[index].work.len();
        self.calls[index].work.push(FrameWork::Discard(Box::new(DiscardState {
            keep,
            next_to_discard,
            primary_failed,
            primary_is_error,
            context_scopes,
            primary_error,
            primary_traceback,
            first_error: None,
            first_traceback: None,
            previous_cleanup_contexts,
            resume,
        })));
    }

    fn record_discard_error(
        &mut self,
        state: &mut DiscardState,
        error: RuntimeError,
        traceback: Option<Traceback>,
    ) {
        if state.first_error.is_none() {
            state.first_error = Some(error);
            state.first_traceback = traceback;
        } else {
            self.evaluator.report_cleanup_error(&error, self.calls.last().map_or_else(
                crate::runtime::eval::zero_span,
                |call| call.call_span,
            ));
        }
        self.evaluator.pending_traceback = None;
    }

    fn step_discard(&mut self, index: usize) -> Result<(), RuntimeError> {
        let Some(FrameWork::Discard(state)) = self.calls[index].work.pop() else {
            unreachable!("discard cursor is stepped in place");
        };
        let mut state = *state;
        state.next_to_discard = self.calls[index].work.len();
        if state.next_to_discard <= state.keep {
            return self.finish_discard(index, state);
        }
        if !state.primary_failed
            && state.context_scopes > 0
            && state
                .first_error
                .as_ref()
                .is_some_and(Evaluator::context_scope_runtime_error_escapes)
        {
            self.evaluator.pending_traceback = None;
            state.first_error = Some(
                RuntimeError::new(
                    "context-scope-escape",
                    "a live producer or host handle cannot escape a restored context",
                )
                .with_span(self.calls[index].call_span),
            );
            state.first_traceback = None;
        }
        let work = self.calls[index]
            .work
            .pop()
            .expect("discard cursor has work above its boundary");
        state.next_to_discard = self.calls[index].work.len();
        match work {
            FrameWork::ClearSlots(slots) => {
                for slot in slots {
                    self.calls[index].slots[slot] = LoweredValue::Unit;
                }
                self.calls[index].work.push(FrameWork::Discard(Box::new(state)));
            }
            FrameWork::ExpressionBoundary {
                policy: ExpressionBoundaryPolicy::Context(context),
                ..
            } => {
                self.open_context_boundaries = self.open_context_boundaries.saturating_sub(1);
                self.evaluator.cleanup_error_contexts.pop();
                if let Some(error) = state.primary_error.take() {
                    state.primary_error = Some(contextualize_runtime_error(error, context.clone()));
                }
                if let Some(error) = state.first_error.take() {
                    state.first_error = Some(contextualize_runtime_error(error, context.clone()));
                }
                if let Some(error) = state.primary_error.as_ref()
                    && let Some(traceback) = &mut state.primary_traceback
                {
                    traceback.error = crate::trace::TraceError::from_runtime_error(error);
                }
                if let Some(error) = state.first_error.as_ref()
                    && let Some(traceback) = &mut state.first_traceback
                {
                    traceback.error = crate::trace::TraceError::from_runtime_error(error);
                }
                self.calls[index].work.push(FrameWork::Discard(Box::new(state)));
            }
            FrameWork::ExpressionBoundary {
                policy: ExpressionBoundaryPolicy::Scope(restore),
                ..
            } => {
                state.context_scopes = state.context_scopes.saturating_sub(1);
                self.evaluator.restore_indexed_context_scope(restore);
                self.calls[index].work.push(FrameWork::Discard(Box::new(state)));
            }
            FrameWork::CompCleanup(streams) => {
                self.calls[index].work.push(FrameWork::Discard(Box::new(state)));
                if let Err(error) = self.cleanup_comp_streams(streams) {
                    if error.abort.as_ref().is_some_and(|signal| signal.force) {
                        return Err(error);
                    }
                    let traceback = self.evaluator.pending_traceback.take();
                    let Some(FrameWork::Discard(mut state)) = self.calls[index].work.pop() else {
                        unreachable!("discard cursor follows comprehension cleanup");
                    };
                    self.record_discard_error(&mut state, error, traceback);
                    self.calls[index].work.push(FrameWork::Discard(state));
                }
            }
            FrameWork::Statements {
                scope_id,
                ..
            } => {
                let Some(scope_id) = scope_id else {
                    self.calls[index].work.push(FrameWork::Discard(Box::new(state)));
                    return Ok(());
                };
                if !state.primary_failed
                    && let Some(error) = &state.first_error
                    && error.abort.is_none()
                    && error.propagated
                {
                    let parent = self.evaluator.parent_owned_host_scope();
                    self.evaluator
                        .transfer_owned_host_resources_in_runtime_error(error, parent);
                }
                let disposition = if state.primary_failed || state.first_error.is_some() {
                    CleanupFailureResources::Release
                } else if state.context_scopes > 0 {
                    CleanupFailureResources::RejectContextEscape
                } else {
                    CleanupFailureResources::Retain
                };
                self.calls[index].work.push(FrameWork::Discard(Box::new(state)));
                self.exit_block_scope(
                    index,
                    scope_id,
                    self.calls[index]
                        .work
                        .iter()
                        .rev()
                        .find_map(|work| match work {
                            FrameWork::Discard(state) => Some(
                                state.primary_is_error || state.first_error.is_some(),
                            ),
                            _ => None,
                        })
                        .unwrap_or(false),
                    false,
                    disposition,
                )?;
            }
            FrameWork::ForStream {
                mut stream, span, ..
            } => {
                self.calls[index].work.push(FrameWork::Discard(Box::new(state)));
                if let Err(error) = self.evaluator.stream_cancel(&mut stream, span) {
                    if error.abort.as_ref().is_some_and(|signal| signal.force) {
                        return Err(error);
                    }
                    let traceback = self.evaluator.pending_traceback.take();
                    let Some(FrameWork::Discard(mut state)) = self.calls[index].work.pop() else {
                        unreachable!("discard cursor follows stream cancellation");
                    };
                    self.record_discard_error(&mut state, error, traceback);
                    self.calls[index].work.push(FrameWork::Discard(state));
                }
            }
            FrameWork::ForPipeline { mut pipeline, .. } => {
                let pending = state.primary_error.as_ref().or(self.pending_error.as_ref());
                let result = pipeline.finish(self.evaluator, pending);
                self.calls[index].work.push(FrameWork::Discard(Box::new(state)));
                if let Err(error) = result {
                    if error.abort.as_ref().is_some_and(|signal| signal.force) {
                        return Err(error);
                    }
                    let traceback = self.evaluator.pending_traceback.take();
                    let Some(FrameWork::Discard(mut state)) = self.calls[index].work.pop() else {
                        unreachable!("discard cursor follows pipeline cleanup");
                    };
                    self.record_discard_error(&mut state, error, traceback);
                    self.calls[index].work.push(FrameWork::Discard(state));
                }
            }
            FrameWork::StageValue {
                mut pipeline, ..
            } => {
                let pending = state.primary_error.as_ref().or(self.pending_error.as_ref());
                let result = pipeline.finish(self.evaluator, pending);
                self.calls[index].work.push(FrameWork::Discard(Box::new(state)));
                if let Err(error) = result {
                    if error.abort.as_ref().is_some_and(|signal| signal.force) {
                        return Err(error);
                    }
                    let traceback = self.evaluator.pending_traceback.take();
                    let Some(FrameWork::Discard(mut state)) = self.calls[index].work.pop() else {
                        unreachable!("discard cursor follows stage cleanup");
                    };
                    self.record_discard_error(&mut state, error, traceback);
                    self.calls[index].work.push(FrameWork::Discard(state));
                }
            }
            FrameWork::CleanupBoundary(guard) => {
                let cleanup_contexts = self.restore_cleanup_action_guard(guard);
                if let Some(FrameWork::CleanupGroup(group)) = self.calls[index].work.last_mut() {
                    group.cleanup_contexts = cleanup_contexts;
                }
                self.calls[index].work.push(FrameWork::Discard(Box::new(state)));
            }
            FrameWork::CleanupGroup(_) => {
                self.calls[index].work.push(FrameWork::Discard(Box::new(state)));
                return Err(RuntimeError::new(
                    "internal-cleanup",
                    "discard reached an active cleanup group",
                )
                .with_span(self.calls[index].call_span));
            }
            other => {
                drop(other);
                self.calls[index].work.push(FrameWork::Discard(Box::new(state)));
            }
        }
        Ok(())
    }

    fn finish_discard(&mut self, index: usize, mut state: DiscardState) -> Result<(), RuntimeError> {
        let report_contexts = self.evaluator.cleanup_error_contexts.clone();
        let report_span = self.calls[index].call_span;
        if let Some(error) = state.first_error.take() {
            if error.abort.as_ref().is_some_and(|signal| signal.force) {
                self.evaluator.cleanup_error_contexts = state.previous_cleanup_contexts;
                self.evaluator.pending_traceback = state.first_traceback.take();
                return Err(error);
            }
            match &state.resume {
                DiscardResume::CapturedPropagation { .. }
                | DiscardResume::CleanupFlow { .. }
                | DiscardResume::CompleteCall { .. }
                    if state.primary_failed =>
                {
                    self.report_cleanup_error_with_contexts(
                        &error,
                        report_span,
                        &report_contexts,
                    );
                }
                DiscardResume::UnwindError { .. } => {
                    self.report_cleanup_error_with_contexts(
                        &error,
                        report_span,
                        &report_contexts,
                    );
                }
                DiscardResume::CleanupFlow { .. } => {
                    self.report_cleanup_error_with_contexts(
                        &error,
                        report_span,
                        &report_contexts,
                    );
                }
                DiscardResume::CompleteCall { flow, .. } if flow_leaves_with_error(flow) => {
                    self.report_cleanup_error_with_contexts(
                        &error,
                        report_span,
                        &report_contexts,
                    );
                }
                _ => {
                    let traceback = state.first_traceback.take();
                    self.evaluator.cleanup_error_contexts = state.previous_cleanup_contexts;
                    self.evaluator.pending_traceback = traceback;
                    let mut error = error;
                    if error.propagated {
                        error.scope_cleanup = error.abort.is_none();
                    }
                    return Err(error);
                }
            }
        }
        let first_traceback = state.first_traceback.take();
        self.evaluator.cleanup_error_contexts = state.previous_cleanup_contexts;
        match state.resume {
            DiscardResume::Continue => {
                self.evaluator.pending_traceback = first_traceback;
                Ok(())
            }
            DiscardResume::ExpressionValue { boundary, value } => {
                debug_assert_eq!(self.calls[index].work.len(), boundary + 1);
                let Some(FrameWork::ExpressionBoundary { policy, next }) =
                    self.calls[index].work.pop()
                else {
                    unreachable!("expression discard retains its boundary");
                };
                let value = match policy {
                    ExpressionBoundaryPolicy::Capture => {
                        LoweredValue::ResultOk(Box::new(value))
                    }
                    ExpressionBoundaryPolicy::Scope(restore) => {
                        self.evaluator.restore_indexed_context_scope(restore);
                        lowered_result_ok(value)
                    }
                    ExpressionBoundaryPolicy::Retry(retry) => {
                        return self.finish_retry_attempt(index, retry, value, next);
                    }
                    _ => value,
                };
                self.evaluator.pending_traceback = None;
                self.push_value(index, FrameValue::Value(value), next);
                Ok(())
            }
            DiscardResume::CapturedPropagation {
                boundary,
                flow,
                contexts,
            } => {
                debug_assert_eq!(self.calls[index].work.len(), boundary + 1);
                let Some(FrameWork::ExpressionBoundary { policy, next }) =
                    self.calls[index].work.pop()
                else {
                    unreachable!("captured propagation retains its boundary");
                };
                let StmtFlow::Propagate(value) = flow else {
                    unreachable!("captured propagation retains an error flow");
                };
                let value = contexts
                    .into_iter()
                    .fold(value, contextualize_propagation);
                if let ExpressionBoundaryPolicy::Retry(retry) = policy {
                    let error = match value {
                        LoweredValue::ResultErr(error) => *error,
                        other => other.into_value(),
                    };
                    let mut traceback = state.primary_traceback.take();
                    if let Some(traceback) = traceback.as_mut() {
                        traceback.error = crate::trace::TraceError::from_value(&error);
                    }
                    return self.fail_retry_attempt(
                        index,
                        retry,
                        error,
                        traceback,
                        Box::new(next),
                    );
                }
                self.evaluator.pending_traceback = None;
                self.push_value(index, FrameValue::Value(value), next);
                Ok(())
            }
            DiscardResume::CleanupFlow {
                boundary,
                flow,
                contexts,
            } => {
                debug_assert_eq!(self.calls[index].work.len(), boundary + 1);
                let Some(FrameWork::CleanupBoundary(guard)) = self.calls[index].work.pop() else {
                    unreachable!("deferred flow retains its action boundary");
                };
                let Some(FrameWork::CleanupGroup(group)) = self.calls[index].work.pop() else {
                    unreachable!("deferred flow boundary retains its cleanup group");
                };
                let cleanup_contexts = self.restore_cleanup_action_guard(guard);
                let mut group = group;
                group.cleanup_contexts = cleanup_contexts;
                self.evaluator.pending_traceback = state.primary_traceback.take();
                let flow = match flow {
                    StmtFlow::Propagate(value) => {
                        let value = contexts
                            .into_iter()
                            .fold(value, contextualize_propagation);
                        update_context_traceback(self.evaluator, &value);
                        StmtFlow::Propagate(value)
                    }
                    flow => flow,
                };
                self.evaluator.pending_value_block_flow.take();
                let span = group.call_span;
                match self.evaluator.normalize_indexed_deferred_result(
                    ControlFlow::Continue(LoweredValue::Unit),
                    Some(flow),
                    span,
                ) {
                    Ok(()) => {
                        self.evaluator.pending_traceback = None;
                        self.calls[index].work.push(FrameWork::CleanupGroup(group));
                    }
                    Err(error) => {
                        let traceback = self.evaluator.pending_traceback.take();
                        self.record_cleanup_action_failure(group, error, traceback);
                    }
                }
                Ok(())
            }
            DiscardResume::CompleteCall { mut flow, contexts } => {
                if let StmtFlow::Propagate(value) = flow {
                    flow = StmtFlow::Propagate(
                        contexts.into_iter().fold(value, contextualize_propagation),
                    );
                }
                self.evaluator.pending_traceback = state
                    .primary_traceback
                    .take()
                    .or(first_traceback);
                if let StmtFlow::Propagate(value) = &flow {
                    update_context_traceback(self.evaluator, value);
                }
                if self.calls[index].defers.is_empty() {
                    self.finish_call_with_cleanup_contexts(index, flow, &[])
                } else {
                    self.calls[index].work.push(FrameWork::Finish(flow));
                    Ok(())
                }
            }
            DiscardResume::UnwindError { boundary } => {
                let error = state
                    .primary_error
                    .take()
                    .expect("error discard retains its pending error");
                self.pending_error = Some(error);
                self.evaluator.pending_traceback = state
                    .primary_traceback
                    .take()
                    .or(first_traceback);
                if let Some(boundary) = boundary {
                    if matches!(
                        self.calls[index].work.get(boundary),
                        Some(FrameWork::CleanupBoundary(_))
                    ) {
                        let Some(FrameWork::CleanupBoundary(guard)) =
                            self.calls[index].work.pop()
                        else {
                            unreachable!("cleanup failure retains its action boundary");
                        };
                        let cleanup_contexts = self.restore_cleanup_action_guard(guard);
                        let error = self.pending_error.take().expect("callback error");
                        let traceback = self.evaluator.pending_traceback.take();
                        self.evaluator.pending_value_block_flow.take();
                        let Some(FrameWork::CleanupGroup(group)) =
                            self.calls[index].work.pop()
                        else {
                            unreachable!("cleanup action boundary retains its group");
                        };
                        let mut group = group;
                        group.cleanup_contexts = cleanup_contexts;
                        self.record_cleanup_action_failure(group, error, traceback);
                        return Ok(());
                    }
                    let Some(FrameWork::ExpressionBoundary { policy, next }) =
                        self.calls[index].work.pop()
                    else {
                        unreachable!("error discard retains its expression boundary");
                    };
                    let error = self.pending_error.take().expect("checked indexed frame failure");
                    let value = match policy {
                        ExpressionBoundaryPolicy::Scope(
                            super::ContextScopeRestore::Within { id },
                        ) => lowered_result_err_value(self.evaluator.finish_within_timeout(id)),
                        ExpressionBoundaryPolicy::Retry(retry) => {
                            return self.finish_retry_attempt(
                                index,
                                retry,
                                capture_checked_error(error)?,
                                next,
                            );
                        }
                        _ => capture_checked_error(error)?,
                    };
                    self.evaluator.pending_traceback = None;
                    self.push_value(index, FrameValue::Value(value), next);
                    Ok(())
                } else {
                    self.calls[index].work.push(FrameWork::FinishError);
                    Ok(())
                }
            }
        }
    }

}

/// Whether control that leaves a scope with `flow` leaves it with an error: a
/// propagating failure, or a `return` whose value is an `Err`. A cleanup
/// failure is then secondary, and `errdefer` actions run.
fn flow_leaves_with_error(flow: &StmtFlow) -> bool {
    matches!(
        flow,
        StmtFlow::Propagate(_) | StmtFlow::Return(LoweredValue::ResultErr(_))
    )
}

fn cleanup_call_scopes(
    evaluator: &mut Evaluator,
    call: &mut CallFrame<'_>,
) -> Result<(), RuntimeError> {
    let mut first_error = None;
    while let Some(scope_id) = call.block_scopes.pop() {
        if let Err(error) = evaluator.exit_owned_host_scope(scope_id)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }
    if call.owns_scope()
        && let Err(error) = evaluator.exit_owned_host_scope(call.scope_id)
        && first_error.is_none()
    {
        first_error = Some(error);
    }
    first_error.map_or(Ok(()), Err)
}

fn contextualize_propagation(
    value: LoweredValue,
    context: crate::runtime::value::ErrorContext,
) -> LoweredValue {
    match value {
        LoweredValue::ResultErr(error) => LoweredValue::ResultErr(Box::new(
            crate::runtime::eval::add_error_context(*error, context),
        )),
        other => LoweredValue::Error(Box::new(crate::runtime::eval::add_error_context(
            other.into_value(),
            context,
        ))),
    }
}

fn contextualize_runtime_error(
    error: RuntimeError,
    context: crate::runtime::value::ErrorContext,
) -> RuntimeError {
    if error.abort.is_some() {
        return error;
    }
    let crate::runtime::value::Value::Error(error) = crate::runtime::eval::add_error_context(
        crate::runtime::value::Value::Error(Box::new(error)),
        context,
    ) else {
        unreachable!()
    };
    *error
}

fn update_context_traceback(evaluator: &mut Evaluator, value: &LoweredValue) {
    if let Some(traceback) = &mut evaluator.pending_traceback {
        let error = match value {
            LoweredValue::ResultErr(error) | LoweredValue::Error(error) => error.as_ref(),
            _ => return,
        };
        traceback.error = crate::trace::TraceError::from_value(error);
    }
}
