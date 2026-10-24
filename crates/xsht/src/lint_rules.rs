use super::{
    ArenaCallArgKind, ArenaExprKind, ArenaExprOrRun, ArenaPatternKind, ArenaRecordFieldKind,
    ArenaStmtKind,
    AstArena, BinaryOp, Binding, BlockId, Diagnostic, DiagnosticCode, ExprId, FixHint, Label, Linter,
    LintOptions, Name,
    PatternArm, PatternId, Severity, Span, StmtId, Type, command_name, is_module_call, is_predeclared_script_args, list_splice_element_type_is_precise,
    lint_code_selected, pipeline_argument_expr, same_ordering_operand, span_may_contain_comment,
    widen_over_grouping,
};
use super::walk::{BindingOrigin, CallableVisibility};
use xsh::frontend::check::CheckBundleView;
use xsh::frontend::syntax::grammar::RunOption;

#[cfg(test)]
use super::nested_pipeline_index_tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RuleOption { InferredPrivateEffects, EnvString, ItemShorthand, TempdirScope, InferredPureReturns, InferredProcReturns, TextPattern, RelPath }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RuleSetting { Always, AlwaysWithOptInAdvice, Configured(RuleOption), OptIn(RuleOption), DeadCode, Runless, NativeTestFile, Migration }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CheckedFact { ExpressionTypes, FunctionReturns, FunctionEffects, StatementPositions, ParameterTypes, ResolvedBindings, ProvenNonNullFallbacks, RequirementTargets, MessagePayloadConstructors, DefinitelyExitingBlocks, StandardCallAndMembership, StandardCallIdentities, MembershipMigrations, PropagatingStatements, RedundantConditionPropagations, DiscardableBindings, UnvalidatedCommandVectors, RedundantVariantQualifiers, RecordRequireMigrations, CheckerDiagnostics, PreparedConstants, LocalBindingTypes, CallableEffects, CallableSignatures, ReturnProofProbe }
impl CheckedFact { fn needs_check_output(self) -> bool { !matches!(self, Self::ReturnProofProbe) } }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FixPolicy { ProducerProof, NoFix, Migration }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SequenceSite { Root, Block }
type SequenceHook = for<'source> fn(&mut Linter<'source>, &[StmtId], Option<BlockId>);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StatementSite { PropagationContext, TempdirScopeCandidate, BindingInitializerType, Propagation, RedundantDiscard, CoreAssert, NeedlessAnnotation, MissingEffects, InferredPureReturn, AtomicallyNeverReplaces, RecordConstructor, EmptyMapInitializer, ListCompoundAssignment, ListPattern, IfAsGuard, LexicalBlock, AdjacentPatternArms, PatternConditional, CatchAllMatch, StringlyMatch, InteractiveCommand, StreamProducerCandidates, FailCandidates, SetLikeBindings, ListAnyBindings, TypedCallableParameters, SizeProductInitializer, RedundantOkReturn, ReturnPathParse, ReturnRequire, WholeValuePipeline, DeferBlockHelperCall }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StatementSequenceSite { ContextScopeScaffolds, RecordDestructuring, FreshMapInitializations, ListElementReconstruction, NegativeIfGuard, OptionalBinding, WriteMode, TestExpect, EmptySentinel }
type StatementSequenceHook = for<'source> fn(&mut Linter<'source>, &[StmtId]);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LoopSite { ByteIteration, MapEntryIteration, YieldDelegation, FileLines, CountedLoop }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExpressionAnalysisSite { PropagationContexts, RedundantCloseUses, StreamProducers, FailCandidates, SetLikeBindings, EmptyFallbacks, RedundantConditionPropagation, RedundantCapturePropagation, SizeProducts, RunArgv, CatchAllArms, NegatedCalls, NameReferenceUses, TempdirScopeUses }
const EXPRESSION_ANALYSIS_ORDER: [ExpressionAnalysisSite; 14] = [
    ExpressionAnalysisSite::PropagationContexts,
    ExpressionAnalysisSite::RedundantCloseUses,
    ExpressionAnalysisSite::StreamProducers,
    ExpressionAnalysisSite::FailCandidates,
    ExpressionAnalysisSite::SetLikeBindings,
    ExpressionAnalysisSite::EmptyFallbacks,
    ExpressionAnalysisSite::RedundantConditionPropagation,
    ExpressionAnalysisSite::RedundantCapturePropagation,
    ExpressionAnalysisSite::NegatedCalls,
    ExpressionAnalysisSite::SizeProducts,
    ExpressionAnalysisSite::RunArgv,
    ExpressionAnalysisSite::CatchAllArms,
    ExpressionAnalysisSite::NameReferenceUses,
    ExpressionAnalysisSite::TempdirScopeUses,
];
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FunctionSite { BareReturn, TailPathParse, TailRequire, TailOkReturn, TailReturnBinding, TailReturn }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProcedureSite { InferredEffects, RedundantResultUnit }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ExpressionSite { PathDisplay }
type ExpressionSiteHook = for<'source> fn(&mut Linter<'source>, ExprId, super::PathDisplaySite);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CommandArgumentHook { RedundantInterpolation, FormatWrapper, CommandFormat, CommandValue }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ExpressionGroupSite { PathMigrations, CallStyle, SingleInterpolation }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RunCheckSite { CapturePropagation, SegmentChecks }
type RunCheckHook = for<'source> fn(&mut Linter<'source>, super::RunFormId);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SequenceExitSite { Root, Block }
type SequenceExitHook = for<'source> fn(&mut Linter<'source>, &[StmtId], usize);
type ExpressionHook = for<'source> fn(&mut Linter<'source>, ExprId);
type LoopHook = for<'source> fn(&mut Linter<'source>, StmtId, super::BindingTargetId, ExprId, BlockId, Span);
type ExpressionAnalysisHook = for<'source> fn(&mut Linter<'source>, ExprId);
type FunctionHook = for<'source, 'types> fn(&mut Linter<'source>, BlockId, bool, bool, Option<&'types Type>, &'types Type);
type ProcedureHook = for<'source> fn(&mut Linter<'source>, super::FunctionDefId, bool, bool, Span);
#[derive(Clone, Copy)]
enum RuleHook {
    Pattern(for<'source> fn(&mut Linter<'source>, PatternId)),
    ScopeExit(for<'source> fn(&mut Linter<'source>, &[(String, Binding)])),
    Run(for<'source> fn(&mut Linter<'source>, super::RunFormId)),
    Root(RootStage, for<'source, 'view> fn(&mut Linter<'source>, &'view CheckBundleView<'source>, &[StmtId])),
    Statement(StatementSite, for<'source> fn(&mut Linter<'source>, StmtId, bool)),
    StatementSequence(StatementSequenceSite, StatementSequenceHook),
    StatementPair(for<'source> fn(&mut Linter<'source>, StmtId, StmtId, &[StmtId])),
    RecordField(for<'source> fn(&mut Linter<'source>, &xsh::frontend::syntax::arena::ArenaRecordField)),
    Loop(LoopSite, LoopHook),
    ScalarIteration(ExpressionHook),
    ExpressionAnalysis(ExpressionAnalysisSite, ExpressionAnalysisHook),
    Parameter(for<'source> fn(&mut Linter<'source>, &xsh::frontend::syntax::arena::ArenaParam)),
    Expression(ExpressionHook),
    EveryExpression(ExpressionHook),
    ExpressionSite(ExpressionSite, ExpressionSiteHook),
    CommandArgument(CommandArgumentHook),
    CallArgument(for<'source> fn(&mut Linter<'source>, &xsh::frontend::syntax::arena::ArenaCallArg)),
    StreamStage(for<'source> fn(&mut Linter<'source>, &super::ArenaStreamStage)),
    StructuredPipeline(for<'source> fn(&mut Linter<'source>, ExprId, super::ArenaRange)),
    Function(FunctionSite, FunctionHook),
    Procedure(ProcedureSite, ProcedureHook),
    BeforeStatement(for<'source> fn(&mut Linter<'source>, StmtId)),
    Binding(for<'source> fn(&mut Linter<'source>, &str, Span, BindingOrigin)),
    ExpressionGroup(ExpressionGroupSite, ExpressionHook),
    RunCheck(RunCheckSite, RunCheckHook),
    SequenceExit(SequenceExitSite, SequenceExitHook),
    Sequence(SequenceSite, SequenceHook),
}
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum RootStage {
    BeforeProgram,
    AfterProgram,
    SignatureCli,
    TryCapture,
    DeclarationReachability,
    CallableAlias,
    RedundantUseAlias,
    InferredProcReturns,
    SortItemShorthands,
    SortCatchAllArms,
    ListAnyUnion,
    SetLike,
    FailCandidates,
    ImplicitMessages,
    TypedCallable,
    ImportBlock,
    TopLevelConstOrder,
    PreparedConstants,
    ImplicitMain,
    UnusedTypes,
    StreamProducerFinalizer,
    DeferBlockHelpers,
}
// Every registered lint inspects source syntax and reports source spans.
struct SourceLintRule { code: DiagnosticCode, setting: RuleSetting, facts: &'static [CheckedFact], fix: FixPolicy, hooks: &'static [RuleHook] }
macro_rules! lint_rules { ($( $code:ident: ($setting:expr, [$($fact:ident),* $(,)?], $fix:expr, [$($hook:expr),* $(,)?]); )+) => { const LINT_RULES: &[SourceLintRule] = &[$(SourceLintRule { code: DiagnosticCode::$code, setting: $setting, facts: &[$(CheckedFact::$fact),*], fix: $fix, hooks: &[$($hook),*] },)*]; }; }
lint_rules! {
    LintRedundantOptionalFallback: (RuleSetting::Always, [ExpressionTypes, ProvenNonNullFallbacks], FixPolicy::ProducerProof, [RuleHook::EveryExpression(|linter, expr| linter.lint_proven_nonnull_fallback(expr))]);
    LintPreferNestedRecordUpdate: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::EveryExpression(|linter, expr| linter.lint_nested_record_update(expr))]);
    LintPreferValuePipeline: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::WholeValuePipeline, lint_statement_whole_value_pipeline), RuleHook::StatementPair(|linter, previous, current, statements| linter.lint_linear_value_pipeline(previous, current, statements)), RuleHook::ExpressionAnalysis(ExpressionAnalysisSite::NameReferenceUses, |linter, expr| linter.observe_name_reference(expr)), RuleHook::RecordField(|linter, field| { if let ArenaRecordFieldKind::Shorthand { name, span } = &field.kind { linter.observe_pipeline_shorthand_field(*name, linter.arena.span(*span)); } }), RuleHook::EveryExpression(|linter, expr| linter.lint_nested_value_pipeline(expr))]);
    LintPreferBlockString: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::EveryExpression(|linter, expr| linter.lint_block_string_concatenation(expr))]);
    LintPreferListSplicing: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::EveryExpression(|linter, expr| linter.lint_list_splicing(expr))]);
    LintPreferMapLiteral: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::EveryExpression(|linter, expr| linter.lint_map_literal_chain(expr)), RuleHook::StatementSequence(StatementSequenceSite::FreshMapInitializations, |linter, statements| linter.lint_fresh_map_initializations(statements))]);
    LintPreferRegexLiteral: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::EveryExpression(|linter, expr| linter.lint_prepared_regex(expr))]);
    LintPreferEnvString: (RuleSetting::Configured(RuleOption::EnvString), [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::EveryExpression(|linter, expr| linter.lint_env_string(expr))]);
    LintPreferComparisonChain: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::EveryExpression(|linter, expr| linter.lint_comparison_chain(expr))]);
    LintLookupAbsence: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::EveryExpression(|linter, expr| linter.lint_lookup_sentinel(expr))]);
    LintPreferOptionalPostfix: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::EveryExpression(|linter, expr| linter.lint_optional_postfix(expr))]);
    LintIdenticalMatchArms: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::AdjacentPatternArms, lint_statement_adjacent_pattern_arms), RuleHook::EveryExpression(lint_expression_adjacent_pattern_arms)]);
    LintBooleanPatternTest: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::EveryExpression(|linter, expr| linter.lint_boolean_match(expr))]);
    LintPatternConditional: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::PatternConditional, |linter, statement, _| { if let ArenaStmtKind::Match { value, arms } = linter.arena.stmt(statement).kind { linter.lint_pattern_conditional_stmt(value, arms, linter.arena.stmt(statement).span); } }), RuleHook::EveryExpression(|linter, expr| linter.lint_pattern_conditional_expr(expr))]);
    LintErrorFallbackBlock: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::EveryExpression(|linter, expr| linter.lint_error_fallback_block(expr))]);
    LintRedundantPathParse: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::ReturnPathParse, |linter, statement, _| { if let ArenaStmtKind::Return(Some(value)) = linter.arena.stmt(statement).kind { linter.lint_return_path_parse_roundtrip(&value); } }), RuleHook::Function(FunctionSite::TailPathParse, |linter, body, _, result_path, _, _| if result_path { linter.lint_tail_path_parse_roundtrip(body) }), RuleHook::EveryExpression(|linter, expr| linter.lint_path_roundtrip(expr))]);
    LintRedundantRequire: (RuleSetting::Always, [ExpressionTypes, RequirementTargets], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::ReturnRequire, |linter, statement, _| { if let ArenaStmtKind::Return(Some(value)) = linter.arena.stmt(statement).kind { linter.lint_return_redundant_require(&value); } }), RuleHook::Function(FunctionSite::TailRequire, |linter, body, _, _, result_ok, _| if result_ok.is_some() { linter.lint_tail_redundant_require(body) }), RuleHook::EveryExpression(|linter, expr| linter.lint_redundant_require(expr))]);
    LintPreferKnownFieldAccess: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::EveryExpression(|linter, expr| linter.lint_known_field_access(expr))]);
    LintInferredRequireTarget: (RuleSetting::Always, [RequirementTargets], FixPolicy::ProducerProof, [RuleHook::EveryExpression(|linter, expr| linter.lint_inferred_require_target(expr))]);
    LintPreferListComp: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::Sequence(SequenceSite::Root, list_comprehension_sequence), RuleHook::Sequence(SequenceSite::Block, list_comprehension_sequence)]);
    LintPreferMapComp: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::Sequence(SequenceSite::Root, list_comprehension_sequence), RuleHook::Sequence(SequenceSite::Block, list_comprehension_sequence)]);
    LintPreferStreamProducer: (RuleSetting::Always, [], FixPolicy::NoFix, [RuleHook::Statement(StatementSite::StreamProducerCandidates, |linter, statement, _| linter.stream_producers.observe_stmt(linter.arena, statement)), RuleHook::ExpressionAnalysis(ExpressionAnalysisSite::StreamProducers, |linter, expr| linter.stream_producers.observe_expr(linter.arena, expr)), RuleHook::Root(RootStage::StreamProducerFinalizer, run_stream_producer_finalizer_hook), RuleHook::Sequence(SequenceSite::Root, |linter, statements, _| linter.lint_stream_producer_suggestions(statements))]);
    LintPreferTempdir: (RuleSetting::Always, [ExpressionTypes, FunctionReturns, StatementPositions], FixPolicy::ProducerProof, [RuleHook::Sequence(SequenceSite::Root, super::prefer_tempdir::lint_scratch_directories), RuleHook::Sequence(SequenceSite::Block, super::prefer_tempdir::lint_scratch_directories)]);
    LintPreferAtomicallyReplace: (RuleSetting::Always, [ExpressionTypes, DefinitelyExitingBlocks, PropagatingStatements], FixPolicy::ProducerProof, [RuleHook::Sequence(SequenceSite::Root, super::prefer_atomically::lint_published_files), RuleHook::Sequence(SequenceSite::Block, super::prefer_atomically::lint_published_files)]);
    LintPreferForIndex: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::Sequence(SequenceSite::Block, super::lint_prefer_for_index::lint_counter_loops)]);
    LintPreferTextPattern: (RuleSetting::OptIn(RuleOption::TextPattern), [ExpressionTypes], FixPolicy::NoFix, [RuleHook::Sequence(SequenceSite::Block, |linter, statements, _| super::lint_prefer_text_pattern::lint_positional_text(linter, statements))]);
    LintPreferWithin: (RuleSetting::Always, [], FixPolicy::NoFix, [RuleHook::Sequence(SequenceSite::Root, |linter, statements, _| super::prefer_within::lint_repeated_timeouts(linter, statements)), RuleHook::Sequence(SequenceSite::Block, |linter, statements, _| super::prefer_within::lint_repeated_timeouts(linter, statements))]);
    LintPreferCollect: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::Sequence(SequenceSite::Root, |linter, statements, _| super::prefer_collect::lint_built_lists(linter, statements)), RuleHook::Sequence(SequenceSite::Block, |linter, statements, _| super::prefer_collect::lint_built_lists(linter, statements))]);
    LintPreferWaitUntil: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::Sequence(SequenceSite::Root, |linter, statements, _| super::prefer_wait_until::lint_polling_loops(linter, statements)), RuleHook::Sequence(SequenceSite::Block, |linter, statements, _| super::prefer_wait_until::lint_polling_loops(linter, statements))]);
    LintAtomicallyNeverReplaces: (RuleSetting::Always, [DefinitelyExitingBlocks], FixPolicy::NoFix, [RuleHook::Statement(StatementSite::AtomicallyNeverReplaces, lint_statement_atomically_never_replaces)]);
    LintBlockHeader: (RuleSetting::Migration, [], FixPolicy::Migration, []);
    LintBooleanGuard: (RuleSetting::Always, [ExpressionTypes, DefinitelyExitingBlocks], FixPolicy::ProducerProof, [RuleHook::StatementSequence(StatementSequenceSite::NegativeIfGuard, |linter, statements| { if let Some(&first) = statements.first() { linter.lint_negative_if_as_boolean_guard(first); } })]);
    LintCommandValue: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::CommandArgument(CommandArgumentHook::CommandValue)]);
    LintCompatibilityVocabulary: (RuleSetting::Migration, [CheckerDiagnostics], FixPolicy::Migration, []);
    LintCoreAssert: (RuleSetting::Always, [ExpressionTypes, StatementPositions, StandardCallAndMembership, StandardCallIdentities], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::CoreAssert, |linter, statement, _| { let stmt = linter.arena.stmt(statement); if let ArenaStmtKind::Expr(expression) = stmt.kind { linter.lint_core_assert(stmt.span, expression); } }), RuleHook::ExpressionGroup(ExpressionGroupSite::CallStyle, lint_call_style_expression)]);
    LintDeadCode: (RuleSetting::DeadCode, [], FixPolicy::NoFix, [RuleHook::BeforeStatement(|linter, statement| linter.warning(linter.arena.stmt(statement).span, "unreachable code", DiagnosticCode::LintDeadCode, "this statement can never execute"))]);
    LintDefaultParamType: (RuleSetting::Always, [CheckerDiagnostics, ExpressionTypes, FunctionReturns, FunctionEffects, StatementPositions, ParameterTypes, ReturnProofProbe], FixPolicy::ProducerProof, [RuleHook::Parameter(run_default_parameter_annotation)]);
    LintDollarInExpressionString: (RuleSetting::Always, [], FixPolicy::NoFix, [RuleHook::Expression(run_dollar_in_expression_string)]);
    LintDurationArithmetic: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::CallStyle, lint_call_style_expression)]);
    LintEnumDeclaration: (RuleSetting::Migration, [], FixPolicy::Migration, []);
    LintEnvScope: (RuleSetting::Migration, [], FixPolicy::Migration, []);
    LintFsRootReceiver: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::CallStyle, lint_call_style_expression)]);
    LintInteractiveCommand: (RuleSetting::Always, [], FixPolicy::NoFix, [RuleHook::Statement(StatementSite::InteractiveCommand, lint_statement_interactive_command)]);
    LintJsonRoundtrip: (RuleSetting::Always, [], FixPolicy::NoFix, [RuleHook::Expression(|linter, expr| linter.lint_json_encode_decode_roundtrip(expr))]);
    LintLegacyTestProc: (RuleSetting::NativeTestFile, [FunctionReturns], FixPolicy::ProducerProof, [RuleHook::Root(RootStage::BeforeProgram, run_legacy_test_proc_hook)]);
    LintLexicalBlock: (RuleSetting::Always, [StatementPositions], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::LexicalBlock, |linter, statement, _| { if let ArenaStmtKind::If { branches, else_block } = linter.arena.stmt(statement).kind { let span = linter.arena.stmt(statement).span; linter.lint_lexical_block(branches, else_block, span); } })]);
    LintLookupFallback: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::CallStyle, lint_call_style_expression)]);
    LintMissingEffects: (RuleSetting::Always, [FunctionEffects, StandardCallAndMembership], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::MissingEffects, lint_statement_missing_effects)]);
    LintMissingFPrefix: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::Expression(run_missing_f_prefix)]);
    LintNeedlessAnnotation: (RuleSetting::Always, [CheckerDiagnostics, ExpressionTypes, LocalBindingTypes, RequirementTargets], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::NeedlessAnnotation, lint_statement_needless_annotation)]);
    LintOrganizeTopLevelConsts: (RuleSetting::Always, [], FixPolicy::NoFix, [RuleHook::Root(RootStage::TopLevelConstOrder, run_top_level_const_order_hook)]);
    LintPathConstructor: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::CallStyle, lint_call_style_expression)]);
    LintPathDisplayEquality: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::PathMigrations, lint_path_migrations_expression)]);
    LintPathDisplaySink: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::PathMigrations, lint_path_migrations_expression)]);
    LintPathTextQuery: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::PathMigrations, lint_path_migrations_expression)]);
    LintPositionalErrorArguments: (RuleSetting::Migration, [CheckerDiagnostics], FixPolicy::Migration, []);
    LintPreferBareFieldLabel: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::Expression(run_quoted_field_labels)]);
    LintPreferCallableAlias: (RuleSetting::Always, [CheckerDiagnostics, PreparedConstants, ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::Root(RootStage::CallableAlias, run_callable_alias_hook)]);
    LintPreferConst: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::Root(RootStage::PreparedConstants, run_prepared_constants_hook)]);
    LintPreferContextScopeValue: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::StatementSequence(StatementSequenceSite::ContextScopeScaffolds, |linter, statements| linter.lint_context_scope_scaffolds(statements))]);
    LintPreferDeferBlock: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::DeferBlockHelperCall, lint_statement_defer_block_helper_call), RuleHook::ExpressionAnalysis(ExpressionAnalysisSite::NameReferenceUses, |linter, expr| linter.observe_name_reference(expr)), RuleHook::Root(RootStage::DeferBlockHelpers, run_defer_block_helpers_hook)]);
    LintPreferEmptyMapLiteral: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::EmptyMapInitializer, |linter, statement, _| { let stmt = linter.arena.stmt(statement).clone(); let (ty, initializer) = match stmt.kind { ArenaStmtKind::Let { ty, initializer, .. } | ArenaStmtKind::Const { ty, initializer, .. } | ArenaStmtKind::Var { ty, initializer, .. } => (ty, initializer), _ => return }; linter.lint_empty_map_initializer(ty, &initializer); })]);
    LintPreferFail: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::FailCandidates, |linter, statement, _| linter.fail_candidates.visit_stmt(linter.arena, statement)), RuleHook::ExpressionAnalysis(ExpressionAnalysisSite::FailCandidates, |linter, expr| linter.fail_candidates.visit_expr(linter.arena, expr)), RuleHook::Root(RootStage::FailCandidates, run_fail_candidates_finalizer)]);
    LintPreferFileLines: (RuleSetting::Always, [ExpressionTypes], FixPolicy::NoFix, [RuleHook::Loop(LoopSite::FileLines, |linter, _, _, iter, _, _| linter.lint_prefer_file_lines(iter))]);
    LintPreferFsFiles: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::StructuredPipeline(|linter, input, stages| linter.lint_prefer_fs_files(input, stages))]);
    LintPreferGenericRecordConstructor: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::RecordConstructor, |linter, statement, _| { let stmt = linter.arena.stmt(statement).clone(); let (ty, initializer) = match stmt.kind { ArenaStmtKind::Let { ty, initializer, .. } | ArenaStmtKind::Const { ty, initializer, .. } | ArenaStmtKind::Var { ty, initializer, .. } => (ty, initializer), _ => return }; linter.lint_record_constructor(ty, &initializer); })]);
    LintPreferGuard: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::IfAsGuard, |linter, statement, _| { if let ArenaStmtKind::If { branches, else_block } = linter.arena.stmt(statement).kind { let span = linter.arena.stmt(statement).span; linter.lint_if_as_guard(branches, else_block, span); } })]);
    LintPreferImplicitMessage: (RuleSetting::Always, [CheckerDiagnostics, MessagePayloadConstructors], FixPolicy::ProducerProof, [RuleHook::Root(RootStage::ImplicitMessages, run_implicit_message_finalizer)]);
    LintPreferIn: (RuleSetting::Always, [ExpressionTypes, StandardCallAndMembership, StandardCallIdentities, MembershipMigrations], FixPolicy::ProducerProof, [RuleHook::ExpressionAnalysis(ExpressionAnalysisSite::NegatedCalls, |linter, expr| { if let ArenaExprKind::Unary { op: super::UnaryOp::Not, expr: inner } = linter.arena.expr(expr).kind && matches!(linter.arena.expr(inner).kind, ArenaExprKind::Call { .. }) { linter.negated_call_spans.insert(linter.arena.expr(inner).span, linter.arena.expr(expr).span); } }), RuleHook::ExpressionGroup(ExpressionGroupSite::CallStyle, lint_call_style_expression)]);
    LintPreferInferredPrivateEffects: (RuleSetting::Configured(RuleOption::InferredPrivateEffects), [CheckerDiagnostics, ExpressionTypes, FunctionReturns, FunctionEffects, StatementPositions, ParameterTypes, StandardCallAndMembership, ReturnProofProbe], FixPolicy::ProducerProof, [RuleHook::Procedure(ProcedureSite::InferredEffects, |linter, definition, exported, entrypoint, span| linter.lint_inferred_proc_effects(definition, exported, entrypoint, span))]);
    LintPreferInferredPureReturn: (RuleSetting::OptIn(RuleOption::InferredPureReturns), [CheckerDiagnostics, ExpressionTypes, FunctionReturns, FunctionEffects, StatementPositions, ParameterTypes, ReturnProofProbe], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::InferredPureReturn, |linter, statement, exported| if let ArenaStmtKind::PureDef(def) = linter.arena.stmt(statement).kind { linter.lint_inferred_pure_return(def, exported); })]);
    LintPreferInferredVariant: (RuleSetting::Always, [RedundantVariantQualifiers], FixPolicy::ProducerProof, [RuleHook::Pattern(run_inferred_variant_pattern_hook), RuleHook::Expression(|linter, expr| linter.lint_inferred_variant(expr))]);
    LintPreferItemShorthand: (RuleSetting::Configured(RuleOption::ItemShorthand), [], FixPolicy::ProducerProof, [RuleHook::StreamStage(|linter, stage| linter.item_shorthands.extend(super::item_shorthand::stage_report(linter.arena, linter.source, stage, linter.format_line_width))), RuleHook::Root(RootStage::SortItemShorthands, run_item_shorthand_finalizer), RuleHook::EveryExpression(run_item_shorthand_expression)]);
    LintPreferListCompoundAssignment: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::ListCompoundAssignment, |linter, statement, _| { if let ArenaStmtKind::Assign { target, op: super::AssignOp::Set, value } = linter.arena.stmt(statement).kind { linter.lint_list_compound_assignment(target, value, linter.arena.stmt(statement).span); } })]);
    LintPreferListElementAssignment: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::StatementSequence(StatementSequenceSite::ListElementReconstruction, |linter, statements| linter.lint_list_element_reconstruction(statements))]);
    LintPreferListPattern: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::ListPattern, |linter, statement, _| { if let ArenaStmtKind::If { branches, else_block } = linter.arena.stmt(statement).kind { let span = linter.arena.stmt(statement).span; linter.lint_list_pattern(branches, else_block, span); } })]);
    LintPreferMapEntryIteration: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::Loop(LoopSite::MapEntryIteration, |linter, statement, target, iter, block, _| linter.lint_map_entry_iteration(statement, target, iter, block))]);
    LintPreferMatchElse: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::ExpressionAnalysis(ExpressionAnalysisSite::CatchAllArms, |linter, expr| { if let ArenaExprKind::Match { arms, .. } = linter.arena.expr(expr).kind && let Some(arm) = linter.arena.match_expr_arms(arms).last() { let findings = super::lint_prefer_match_else::catch_all_arm_report(linter.arena, linter.source, arm.pattern, arm.guard, arm.spelling); linter.catch_all_arms.extend(findings); } }), RuleHook::Statement(StatementSite::CatchAllMatch, lint_statement_catch_all_match), RuleHook::Root(RootStage::SortCatchAllArms, run_catch_all_finalizer)]);
    LintPreferMethod: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::CallStyle, lint_call_style_expression)]);
    LintPreferNamedArgumentPun: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::CallArgument(|linter, arg| linter.lint_named_argument_pun(arg))]);
    LintPreferNamedArgumentSpread: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::CallStyle, lint_call_style_expression)]);
    LintPreferOptionalBinding: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::StatementSequence(StatementSequenceSite::OptionalBinding, super::lint_optional_binding::lint_null_test_then_binding)]);
    LintPreferPositionalConstructor: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::Expression(|linter, expr| linter.lint_positional_constructor(expr))]);
    LintPreferPropagation: (RuleSetting::Always, [ExpressionTypes, FunctionEffects, StatementPositions, PropagatingStatements, RedundantConditionPropagations], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::Propagation, |linter, statement, _| linter.lint_propagation(statement))]);
    LintPreferReadLines: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::PathMigrations, lint_path_migrations_expression)]);
    LintPreferRecordConstructor: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::RecordConstructor, |linter, statement, _| { let stmt = linter.arena.stmt(statement).clone(); let (ty, initializer) = match stmt.kind { ArenaStmtKind::Let { ty, initializer, .. } | ArenaStmtKind::Const { ty, initializer, .. } | ArenaStmtKind::Var { ty, initializer, .. } => (ty, initializer), _ => return }; linter.lint_record_constructor(ty, &initializer); })]);
    LintPreferRecordDestructuring: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::StatementSequence(StatementSequenceSite::RecordDestructuring, |linter, statements| linter.lint_record_destructuring(statements))]);
    LintPreferRepeat: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::Loop(LoopSite::CountedLoop, |linter, _, target, iter, block, span| super::prefer_repeat::lint_counted_loop(linter, span, target, iter, block))]);
    LintPreferScalarIteration: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::BindingInitializerType, |linter, statement, _| linter.observe_binding_initializer_type(statement)), RuleHook::ScalarIteration(|linter, expr| linter.lint_scalar_split_iteration(expr)), RuleHook::Loop(LoopSite::ByteIteration, |linter, statement, target, iter, block, _| linter.lint_byte_iteration(statement, target, iter, block))]);
    LintPreferRunArgv: (RuleSetting::Always, [StandardCallIdentities], FixPolicy::ProducerProof, [RuleHook::ExpressionAnalysis(ExpressionAnalysisSite::RunArgv, |linter, expr| { if let Some(finding) = super::lint_run_argv::run_argv_diagnostic(linter.arena, linter.source, &linter.standard_call_spans, expr) { linter.diagnostics.push(finding); } })]);
    LintPreferSignatureCli: (RuleSetting::Always, [CheckerDiagnostics, ExpressionTypes, StatementPositions, FunctionReturns], FixPolicy::ProducerProof, [RuleHook::Root(RootStage::SignatureCli, run_signature_cli_hook)]);
    LintPreferSizeLiteral: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::SizeProductInitializer, lint_statement_size_product_initializer), RuleHook::ExpressionAnalysis(ExpressionAnalysisSite::SizeProducts, |linter, expr| { let builtin_range = matches!(linter.arena.expr(expr).kind, ArenaExprKind::Call { callee, .. } if matches!(linter.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "range" && linter.binding_definition(name).is_none())); if let Some(finding) = linter.size_products.visit(linter.arena, linter.source, expr, builtin_range) { linter.diagnostics.push(finding); } })]);
    LintPreferSlice: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::CallStyle, lint_call_style_expression)]);
    LintPreferStringConcat: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::CallStyle, lint_call_style_expression)]);
    LintPreferTempdirScope: (RuleSetting::Configured(RuleOption::TempdirScope), [CheckerDiagnostics, ExpressionTypes, FunctionReturns, StatementPositions], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::TempdirScopeCandidate, |linter, statement, _| linter.collect_tempdir_scope_candidates(statement)), RuleHook::ExpressionAnalysis(ExpressionAnalysisSite::TempdirScopeUses, |linter, expr| { let node = linter.arena.expr(expr); match node.kind { ArenaExprKind::Try(_) => linter.tempdir_scope_try_expressions.push(expr), ArenaExprKind::Ident(name) => linter.tempdir_scope_identifiers.push((name, node.span)), _ => {} } }), RuleHook::Root(RootStage::AfterProgram, run_tempdir_scope_hook)]);
    LintPreferTryCapture: (RuleSetting::Always, [CheckerDiagnostics, ExpressionTypes, FunctionEffects, StatementPositions, LocalBindingTypes, CallableEffects], FixPolicy::ProducerProof, [RuleHook::Root(RootStage::TryCapture, run_try_capture_hook)]);
    LintPreferWriteLines: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::PathMigrations, lint_path_migrations_expression)]);
    LintPreferYieldDelegation: (RuleSetting::Always, [ExpressionTypes, FunctionReturns], FixPolicy::ProducerProof, [RuleHook::Loop(LoopSite::YieldDelegation, |linter, _, target, iter, block, span| linter.lint_yield_delegation(span, target, iter, block))]);
    LintPublicResultError: (RuleSetting::Always, [CheckerDiagnostics], FixPolicy::ProducerProof, []);
    LintRedundantBareReturn: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::Function(FunctionSite::BareReturn, |linter, body, result_unit, _, _, _| if result_unit { linter.lint_redundant_bare_return(body) })]);
    LintRedundantClose: (RuleSetting::Always, [CheckerDiagnostics, ExpressionTypes, LocalBindingTypes, ResolvedBindings, StandardCallIdentities], FixPolicy::ProducerProof, [RuleHook::ExpressionAnalysis(ExpressionAnalysisSite::RedundantCloseUses, super::redundant_close::record_binding_use), RuleHook::SequenceExit(SequenceExitSite::Root, run_redundant_close_sequence_exit), RuleHook::SequenceExit(SequenceExitSite::Block, run_redundant_close_sequence_exit)]);
    LintRedundantCommandFmt: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::CommandArgument(CommandArgumentHook::CommandFormat)]);
    LintRedundantCommandInterpolation: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::CommandArgument(CommandArgumentHook::RedundantInterpolation)]);
    LintRedundantDefault: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::CallStyle, lint_call_style_expression)]);
    LintRedundantDisplayParse: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::Expression(|linter, expr| linter.lint_scalar_display_parse_roundtrip(expr))]);
    LintRedundantFmtWrapper: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::CommandArgument(CommandArgumentHook::FormatWrapper)]);
    LintRedundantMainCall: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::Root(RootStage::ImplicitMain, run_implicit_main_hook)]);
    LintRedundantNewlineTripleString: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::Expression(run_redundant_newline_triple_string)]);
    LintRedundantOkReturn: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::RedundantOkReturn, |linter, statement, _| { if let ArenaStmtKind::Return(Some(value)) = linter.arena.stmt(statement).kind { linter.lint_return_value(&value); } })]);
    LintRedundantOkTail: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::Function(FunctionSite::TailOkReturn, |linter, body, _, _, result_ok, _| if result_ok.is_some() { linter.lint_tail_redundant_ok_return(body, result_ok) })]);
    LintRedundantPathDisplay: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::ExpressionSite(ExpressionSite::PathDisplay, |linter, expr, site| linter.lint_redundant_path_display(expr, site))]);
    LintRedundantPathInterpolation: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::SingleInterpolation, lint_redundant_single_interpolation_expression)]);
    LintRedundantPipelineStage: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::StructuredPipeline(|linter, _, stages| linter.lint_redundant_stream_stages(stages))]);
    LintRedundantPropagation: (RuleSetting::Always, [ExpressionTypes, StatementPositions, PropagatingStatements, RedundantConditionPropagations], FixPolicy::ProducerProof, [RuleHook::ExpressionAnalysis(ExpressionAnalysisSite::PropagationContexts, |linter, expr| linter.observe_expression_positions(expr)), RuleHook::ExpressionAnalysis(ExpressionAnalysisSite::RedundantConditionPropagation, |linter, expr| { if let Some(finding) = super::lint_redundant_propagation::redundant_condition_propagation(linter.arena, linter.source, &linter.redundant_condition_propagations, expr) { linter.diagnostics.push(finding); } }), RuleHook::ExpressionAnalysis(ExpressionAnalysisSite::RedundantCapturePropagation, |linter, expr| { if let Some(finding) = super::lint_redundant_propagation::redundant_capture_try(linter.arena, linter.source, linter.expr_positions.as_ref().expect("the expression walk records position facts before rule dispatch"), expr) { linter.diagnostics.push(finding); } }), RuleHook::Statement(StatementSite::PropagationContext, |linter, statement, _| linter.observe_statement_positions(statement)), RuleHook::Statement(StatementSite::Propagation, |linter, statement, _| linter.lint_propagation(statement)), RuleHook::RunCheck(RunCheckSite::CapturePropagation, run_capture_propagation_check)]);
    LintRedundantResultUnit: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::Procedure(ProcedureSite::RedundantResultUnit, |linter, definition, exported, _, _| linter.lint_redundant_result_unit(definition, exported))]);
    LintRedundantScopePropagation: (RuleSetting::Always, [ExpressionTypes, StatementPositions, PropagatingStatements], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::Propagation, |linter, statement, _| linter.lint_propagation(statement))]);
    LintRedundantStringInterpolation: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::SingleInterpolation, lint_redundant_single_interpolation_expression)]);
    LintRedundantTailReturnBinding: (RuleSetting::Always, [ExpressionTypes, FunctionReturns], FixPolicy::ProducerProof, [RuleHook::Function(FunctionSite::TailReturnBinding, |linter, body, _, _, _, _| linter.lint_redundant_tail_return_binding(body))]);
    LintRedundantTailReturn: (RuleSetting::Always, [ExpressionTypes, FunctionReturns, StatementPositions], FixPolicy::ProducerProof, [RuleHook::Function(FunctionSite::TailReturn, |linter, body, _, _, _, return_type| linter.lint_redundant_tail_return(body, return_type))]);
    LintRemovedRecordRequire: (RuleSetting::Migration, [CheckerDiagnostics, RecordRequireMigrations], FixPolicy::Migration, []);
    LintRunStatus: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::RunCheck(RunCheckSite::SegmentChecks, run_segment_checks)]);
    LintRunless: (RuleSetting::Runless, [], FixPolicy::NoFix, [RuleHook::Run(run_runless_hook)]);
    LintShadowing: (RuleSetting::Always, [], FixPolicy::NoFix, [RuleHook::Binding(run_shadowing_hook)]);
    LintStageCallable: (RuleSetting::Always, [CallableSignatures], FixPolicy::ProducerProof, [RuleHook::StreamStage(|linter, stage| linter.lint_stage_callable_wrapper(stage))]);
    LintStreamOptions: (RuleSetting::Migration, [], FixPolicy::Migration, []);
    LintStringlyTypedMatch: (RuleSetting::Always, [], FixPolicy::NoFix, [RuleHook::Statement(StatementSite::StringlyMatch, lint_statement_stringly_match)]);
    LintUnsortedImports: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::Root(RootStage::ImportBlock, run_import_block_hook)]);
    LintUnusedCallable: (RuleSetting::DeadCode, [], FixPolicy::NoFix, [RuleHook::Root(RootStage::DeclarationReachability, run_unused_callable_hook)]);
    LintUnusedLocal: (RuleSetting::Always, [LocalBindingTypes], FixPolicy::NoFix, [RuleHook::ScopeExit(run_unused_local_hook)]);
    LintUnusedType: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::Root(RootStage::UnusedTypes, run_unused_types_hook)]);
    LintPreferInferredProcReturn: (RuleSetting::OptIn(RuleOption::InferredProcReturns), [CheckerDiagnostics, ExpressionTypes, FunctionReturns, FunctionEffects, StatementPositions, ParameterTypes, ReturnProofProbe], FixPolicy::ProducerProof, [RuleHook::Root(RootStage::InferredProcReturns, run_inferred_proc_return_hook)]);
    LintRedundantUseAlias: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::Root(RootStage::RedundantUseAlias, run_redundant_use_alias_hook)]);
    LintListAnyUnion: (RuleSetting::Always, [ExpressionTypes], FixPolicy::NoFix, [RuleHook::Statement(StatementSite::ListAnyBindings, lint_statement_list_any_bindings), RuleHook::Root(RootStage::ListAnyUnion, run_list_any_finalizer)]);
    LintPreferEnvPathList: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::PathMigrations, lint_path_migrations_expression)]);
    LintPreferWriteMode: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::StatementSequence(StatementSequenceSite::WriteMode, super::lint_write_mode::lint_write_then_chmod)]);
    LintPreferPathKind: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::PathMigrations, lint_path_migrations_expression)]);
    LintPreferTypedCallable: (RuleSetting::Always, [CallableSignatures, FunctionReturns, FunctionEffects], FixPolicy::NoFix, [RuleHook::Statement(StatementSite::TypedCallableParameters, lint_statement_typed_callable_parameters), RuleHook::Root(RootStage::TypedCallable, run_typed_callable_finalizer)]);
    LintPreferTestExpect: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::StatementSequence(StatementSequenceSite::TestExpect, super::lint_prefer_test_expect::lint_script_runs)]);
    LintPreferIsEmpty: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::PathMigrations, lint_path_migrations_expression)]);
    LintPreferNegativeIndex: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::PathMigrations, lint_path_migrations_expression)]);
    LintPreferNonEmptyArgv: (RuleSetting::Always, [UnvalidatedCommandVectors], FixPolicy::NoFix, [RuleHook::RunCheck(RunCheckSite::SegmentChecks, run_segment_checks)]);
    LintPreferAsConversion: (RuleSetting::Always, [ExpressionTypes], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::PathMigrations, lint_path_migrations_expression)]);
    LintPreferRelPath: (RuleSetting::OptIn(RuleOption::RelPath), [ExpressionTypes], FixPolicy::NoFix, [RuleHook::ExpressionGroup(ExpressionGroupSite::PathMigrations, lint_path_migrations_expression)]);
    LintPreferArgumentLabel: (RuleSetting::Always, [], FixPolicy::ProducerProof, [RuleHook::ExpressionGroup(ExpressionGroupSite::PathMigrations, lint_path_migrations_expression)]);
    LintEmptySentinel: (RuleSetting::Always, [ExpressionTypes], FixPolicy::NoFix, [RuleHook::ExpressionAnalysis(ExpressionAnalysisSite::EmptyFallbacks, |linter, expr| { let findings = linter.empty_fallbacks.visit_expr(linter.arena, linter.source, expr); linter.diagnostics.extend(findings); }), RuleHook::StatementSequence(StatementSequenceSite::EmptySentinel, super::lint_empty_sentinel::lint_empty_fallback_then_test)]);
    LintPreferSet: (RuleSetting::AlwaysWithOptInAdvice, [], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::SetLikeBindings, |linter, statement, _| linter.set_like_bindings.visit_stmt(linter.arena, linter.source, statement)), RuleHook::ExpressionAnalysis(ExpressionAnalysisSite::SetLikeBindings, |linter, expr| linter.set_like_bindings.visit_expr(linter.arena, linter.source, expr)), RuleHook::Root(RootStage::SetLike, run_set_like_finalizer)]);
    LintRedundantDiscard: (RuleSetting::Always, [DiscardableBindings], FixPolicy::ProducerProof, [RuleHook::Statement(StatementSite::RedundantDiscard, |linter, statement, _| super::lint_redundant_discard::lint_redundant_discard(linter, statement))]);
}

fn run_inferred_variant_pattern_hook(linter: &mut Linter<'_>, pattern: PatternId) {
    let span = linter.arena.span(linter.arena.pattern(pattern).span);
    linter.lint_inferred_variant_pattern(span);
}

fn run_dollar_in_expression_string(linter: &mut Linter<'_>, expr: ExprId) {
    if matches!(linter.arena.expr(expr).kind, ArenaExprKind::Str(_)) {
        linter.lint_dollar_in_expression_string(expr);
    }
}

fn run_missing_f_prefix(linter: &mut Linter<'_>, expr: ExprId) {
    if matches!(
        linter.arena.expr(expr).kind,
        ArenaExprKind::Str(_) | ArenaExprKind::PathStr(_)
    ) {
        linter.lint_missing_f_prefix(expr);
    }
}

fn run_quoted_field_labels(linter: &mut Linter<'_>, expr: ExprId) {
    if let ArenaExprKind::Record(fields) = linter.arena.expr(expr).kind {
        linter.lint_quoted_field_labels(fields);
    }
}

fn run_redundant_newline_triple_string(linter: &mut Linter<'_>, expr: ExprId) {
    if matches!(linter.arena.expr(expr).kind, ArenaExprKind::Str(_)) {
        linter.lint_redundant_newline_triple_string(expr);
    }
}

fn run_item_shorthand_expression(linter: &mut Linter<'_>, expr: ExprId) {
    if let ArenaExprKind::Binary { op: super::BinaryOp::ResultFallback, right, .. } =
        linter.arena.expr(expr).kind
    {
        linter.item_shorthands.extend(super::item_shorthand::handler_report(
            linter.arena,
            linter.source,
            right,
        linter.format_line_width,
        ));
    }
}

fn run_unused_local_hook(linter: &mut Linter<'_>, unused: &[(String, Binding)]) {
    for (name, binding) in unused {
        linter.warning(
            binding.span,
            format!("unused local variable `{name}`"),
            DiagnosticCode::LintUnusedLocal,
            "binding is never read",
        );
    }
}

// Exported parameter names are part of named calls, so matching a module
// callable is intentional.
fn run_shadowing_hook(
    linter: &mut Linter<'_>,
    name: &str,
    span: Span,
    origin: BindingOrigin,
) {
    let public_callable_label = matches!(
        origin,
        BindingOrigin::FunctionParameter {
            visibility: CallableVisibility::Exported
        }
    ) && linter.module_callable_names.contains(name);
    if linter.is_defined_in_outer_scope(name)
        && !is_predeclared_script_args(name)
        && name != "_"
        && !public_callable_label
    {
        linter.diagnostics.push(
            Diagnostic::error("binding shadows an outer name")
                .with_code(DiagnosticCode::LintShadowing)
                .with_label(Label::secondary(span, "shadowed binding starts here")),
        );
    }
}

fn run_runless_hook(linter: &mut Linter<'_>, run_id: super::RunFormId) {
    let run = linter.arena.run_form(run_id).clone();
    for segment in linter.arena.run_segments(run.segments).to_vec() {
        let seg_span = linter.arena.span(segment.span);
        let name = command_name(linter.arena, linter.source, &segment.target);
        let exempt = name
            .as_deref()
            .is_some_and(|name| linter.runless_except.iter().any(|item| item == name));
        if exempt {
            continue;
        }
        let label = match name {
            Some(name) => format!("`{name}` is an external command"),
            None => "external command not allowed in runless mode".to_string(),
        };
        linter.diagnostics.push(
            Diagnostic::new(Severity::Error, "external command not permitted (--runless)")
                .with_code(DiagnosticCode::LintRunless)
                .with_label(Label::secondary(seg_span, label)),
        );
    }
}

fn run_legacy_test_proc_hook(
    linter: &mut Linter<'_>,
    _view: &CheckBundleView<'_>,
    statements: &[StmtId],
) {
    linter.lint_legacy_test_declarations(statements);
}

fn run_tempdir_scope_hook(
    linter: &mut Linter<'_>,
    view: &CheckBundleView<'_>,
    _statements: &[StmtId],
) {
    let functions = std::mem::take(&mut linter.tempdir_scope_functions);
    let loops = std::mem::take(&mut linter.tempdir_scope_loops);
    let mut try_expressions = std::mem::take(&mut linter.tempdir_scope_try_expressions);
    try_expressions.sort_unstable_by_key(|expr| expr.index());
    let identifiers = std::mem::take(&mut linter.tempdir_scope_identifiers);
    super::tempdir_scope::lint_tempdir_scopes(
        linter,
        view,
        &functions,
        &loops,
        &try_expressions,
        &identifiers,
    );
}

fn run_signature_cli_hook(
    linter: &mut Linter<'_>,
    view: &CheckBundleView<'_>,
    _statements: &[StmtId],
) {
    let diagnostics = super::cli_entry::signature_cli_migration(
        view,
        linter.source,
        linter.checked_output.clone(),
    );
    linter.diagnostics.extend(diagnostics);
}

fn run_try_capture_hook(
    linter: &mut Linter<'_>,
    view: &CheckBundleView<'_>,
    _statements: &[StmtId],
) {
    let diagnostics = super::lint_try_capture::lint_try_capture_helpers(
        view,
        linter.source,
        linter.checked_output.clone(),
    );
    linter.diagnostics.extend(diagnostics);
}

fn run_unused_callable_hook<'source>(
    linter: &mut Linter<'source>,
    view: &CheckBundleView<'source>,
    _statements: &[StmtId],
) {
    linter.lint_declaration_reachability(view);
}

fn run_callable_alias_hook(
    linter: &mut Linter<'_>,
    view: &CheckBundleView<'_>,
    _statements: &[StmtId],
) {
    let diagnostics = super::lint_callable_alias::lint_callable_aliases(
        view,
        linter.source,
        linter.checked_output.clone(),
    );
    linter.diagnostics.extend(diagnostics);
}

fn run_redundant_use_alias_hook(
    linter: &mut Linter<'_>,
    view: &CheckBundleView<'_>,
    _statements: &[StmtId],
) {
    linter
        .diagnostics
        .extend(super::redundant_use_alias::lint_redundant_use_aliases(
            view,
            linter.source,
        ));
}

fn run_inferred_proc_return_hook(
    linter: &mut Linter<'_>,
    _view: &CheckBundleView<'_>,
    _statements: &[StmtId],
) {
    linter.lint_inferred_proc_returns();
}

fn run_item_shorthand_finalizer(
    linter: &mut Linter<'_>,
    _view: &CheckBundleView<'_>,
    _statements: &[StmtId],
) {
    let mut findings = std::mem::take(&mut linter.item_shorthands);
    findings.sort_by_key(|diagnostic| diagnostic.labels[0].span.start());
    linter.diagnostics.extend(findings);
}

fn run_catch_all_finalizer(
    linter: &mut Linter<'_>,
    _view: &CheckBundleView<'_>,
    _statements: &[StmtId],
) {
    let mut findings = std::mem::take(&mut linter.catch_all_arms);
    findings.sort_by_key(|diagnostic| diagnostic.labels[0].span.start());
    linter.diagnostics.extend(findings);
}

fn run_list_any_finalizer(
    linter: &mut Linter<'_>,
    _view: &CheckBundleView<'_>,
    _statements: &[StmtId],
) {
    let findings = std::mem::take(&mut linter.list_any_bindings).finish();
    linter.diagnostics.extend(findings);
}

fn run_set_like_finalizer(
    linter: &mut Linter<'_>,
    _view: &CheckBundleView<'_>,
    _statements: &[StmtId],
) {
    let findings = std::mem::take(&mut linter.set_like_bindings)
        .finish(linter.source, linter.prefer_set_advice);
    linter.diagnostics.extend(findings);
}

fn run_fail_candidates_finalizer(
    linter: &mut Linter<'_>,
    view: &CheckBundleView<'_>,
    _statements: &[StmtId],
) {
    let findings = std::mem::take(&mut linter.fail_candidates)
        .finish(&view.program().arena, linter.source);
    linter.diagnostics.extend(findings);
}

fn run_implicit_message_finalizer(
    linter: &mut Linter<'_>,
    view: &CheckBundleView<'_>,
    _statements: &[StmtId],
) {
    let checked = linter.checked_output.as_deref().and_then(|checked| {
        checked
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity != Severity::Error)
            .then_some(&checked.message_payload_constructors)
    });
    #[cfg(test)]
    let constructors = linter.message_payload_constructors.as_ref().or(checked);
    #[cfg(not(test))]
    let constructors = checked;
    if let Some(constructors) = constructors {
        let findings = super::lint_implicit_message::lint_implicit_messages(
            view,
            linter.source,
            constructors,
            &linter.diagnostics,
        );
        linter.diagnostics.extend(findings);
    }
}

fn run_typed_callable_finalizer(
    linter: &mut Linter<'_>,
    _view: &CheckBundleView<'_>,
    _statements: &[StmtId],
) {
    let parameters = std::mem::take(&mut linter.callable_parameters);
    let findings = parameters.finish(
        linter.arena,
        linter.checked_output.as_deref(),
    );
    linter.diagnostics.extend(findings);
}

fn run_import_block_hook(
    linter: &mut Linter<'_>,
    _view: &CheckBundleView<'_>,
    statements: &[StmtId],
) {
    linter.lint_import_blocks(statements);
}

fn run_top_level_const_order_hook(
    linter: &mut Linter<'_>,
    _view: &CheckBundleView<'_>,
    statements: &[StmtId],
) {
    linter.lint_top_level_const_order(statements);
}

fn run_prepared_constants_hook(
    linter: &mut Linter<'_>,
    _view: &CheckBundleView<'_>,
    statements: &[StmtId],
) {
    linter.lint_prepared_constants(statements);
}

fn run_implicit_main_hook(
    linter: &mut Linter<'_>,
    _view: &CheckBundleView<'_>,
    statements: &[StmtId],
) {
    linter.lint_implicit_main(statements);
}

fn run_unused_types_hook(
    linter: &mut Linter<'_>,
    _view: &CheckBundleView<'_>,
    _statements: &[StmtId],
) {
    linter.lint_unused_types();
}

fn run_stream_producer_finalizer_hook(
    linter: &mut Linter<'_>,
    _view: &CheckBundleView<'_>,
    _statements: &[StmtId],
) {
    linter.finish_stream_producer_suggestions();
}

fn run_defer_block_helpers_hook(
    linter: &mut Linter<'_>,
    _view: &CheckBundleView<'_>,
    statements: &[StmtId],
) {
    linter.lint_defer_block_helpers(statements);
}

fn run_default_parameter_annotation(
    linter: &mut Linter<'_>,
    parameter: &xsh::frontend::syntax::arena::ArenaParam,
) {
    linter.lint_default_parameter_annotation(parameter);
}

fn lint_expression_adjacent_pattern_arms(linter: &mut Linter<'_>, expr: ExprId) {
    if let ArenaExprKind::Match { arms, .. } = linter.arena.expr(expr).kind {
        let arms = linter.arena.match_expr_arms(arms).iter().map(|arm| {
            (arm.pattern, arm.guard, Err(arm.value), linter.arena.span(arm.span))
        }).collect();
        linter.lint_adjacent_pattern_arms(arms);
    }
}

impl RuleSetting {
    fn enabled_for_options(self, options: &LintOptions, named: bool) -> bool {
        match self {
            Self::Always | Self::AlwaysWithOptInAdvice => true,
            Self::Configured(option) | Self::OptIn(option) => named || match option {
                RuleOption::InferredPrivateEffects => options.prefer_inferred_private_effects,
                RuleOption::EnvString => options.prefer_env_string,
                RuleOption::ItemShorthand => options.prefer_item_shorthand,
                RuleOption::TempdirScope => options.prefer_tempdir_scope,
                RuleOption::InferredPureReturns => options.prefer_inferred_pure_returns,
                RuleOption::InferredProcReturns => options.prefer_inferred_proc_returns,
                RuleOption::TextPattern => options.prefer_text_pattern,
                RuleOption::RelPath => options.prefer_rel_path,
            },
            Self::DeadCode => options.dead_code,
            Self::Runless => options.runless,
            Self::NativeTestFile => options.native_test_file,
            Self::Migration => named,
        }
    }
}
fn rule(code: DiagnosticCode) -> Option<&'static SourceLintRule> { LINT_RULES.iter().find(|rule| rule.code == code) }
pub(super) fn lint_codes() -> impl Iterator<Item = DiagnosticCode> {
    DiagnosticCode::ALL
        .iter()
        .copied()
        .filter(|code| code.family() == xsh::diagnostic::DiagnosticFamily::Lint && rule(*code).is_some())
}
pub(super) fn assert_fix_policy(diagnostics: &[Diagnostic]) {
    debug_assert!(diagnostics.iter().all(|diagnostic| diagnostic.code.and_then(rule).is_none_or(|rule| rule.fix != FixPolicy::NoFix || diagnostic.fix_hints.is_empty())));
}
impl RuleSelection {
    pub(super) fn new(options: &LintOptions) -> Self {
        let only = options.only.as_deref();
        let named = |code| {
            only.is_some_and(|only| only.contains(&code))
        };
        let mut enabled = rustc_hash::FxHashSet::default();
        let mut expression_analysis_hooks = [None; EXPRESSION_ANALYSIS_ORDER.len()];
        for rule in LINT_RULES {
            if only.is_some_and(|codes| !codes.contains(&rule.code))
                || options.excluded_rules.contains(&rule.code)
                || !rule.setting.enabled_for_options(options, named(rule.code))
                || (options.annotation_policy_writes_returns
                    && matches!(
                        rule.code,
                        DiagnosticCode::LintPreferInferredPureReturn
                            | DiagnosticCode::LintPreferInferredProcReturn
                    ))
            {
                continue;
            }
            enabled.insert(rule.code);
            for hook in rule.hooks {
                if let RuleHook::ExpressionAnalysis(site, callback) = hook {
                    let index = EXPRESSION_ANALYSIS_ORDER
                        .iter()
                        .position(|ordered| ordered == site)
                        .expect("expression analysis hook site is ordered");
                    if expression_analysis_hooks[index].is_none() {
                        expression_analysis_hooks[index] = Some(*callback);
                    }
                }
            }
        }
        Self {
            enabled,
            only: options.only.clone(),
            excluded: options.excluded_rules.clone(),
            expression_analysis_hooks,
        }
    }
    pub(super) fn enabled(&self, code: DiagnosticCode) -> bool {
        self.enabled.contains(&code)
    }
    pub(super) fn prefer_set_named(&self) -> bool {
        self.only
            .as_deref()
            .is_some_and(|only| only.contains(&DiagnosticCode::LintPreferSet))
    }
    pub(super) fn accepts(&self, code: Option<DiagnosticCode>) -> bool {
        lint_code_selected(self.only.as_deref(), code)
            && !code.is_some_and(|code| self.excluded.contains(&code))
    }
    pub(super) fn requires_check_output(&self) -> bool {
        LINT_RULES.iter().any(|rule| {
            self.enabled(rule.code)
                && rule.facts.iter().any(|fact| fact.needs_check_output())
        })
    }
}
pub(super) fn enabled(linter: &Linter<'_>, code: DiagnosticCode) -> bool {
    linter.rule_selection.enabled(code)
}
pub(super) struct RuleSelection {
    enabled: rustc_hash::FxHashSet<DiagnosticCode>,
    only: Option<Vec<DiagnosticCode>>,
    excluded: Vec<DiagnosticCode>,
    expression_analysis_hooks: [Option<ExpressionAnalysisHook>; EXPRESSION_ANALYSIS_ORDER.len()],
}
pub(super) fn record_field(
    linter: &mut Linter<'_>,
    field: &xsh::frontend::syntax::arena::ArenaRecordField,
) {
    for rule in LINT_RULES {
        if !linter.rule_selection.enabled(rule.code) {
            continue;
        }
        for hook in rule.hooks {
            if let RuleHook::RecordField(callback) = hook {
                callback(linter, field);
            }
        }
    }
}






fn lint_statement_needless_annotation(linter: &mut Linter<'_>, statement: StmtId, exported: bool) {
    let stmt = linter.arena.stmt(statement).clone();
    let (target, mutable, ty, initializer) = match stmt.kind {
        ArenaStmtKind::Let { target, ty: Some(ty), initializer }
        | ArenaStmtKind::Const { target, ty: Some(ty), initializer } =>
            (target, false, ty, initializer),
        ArenaStmtKind::Var { target, ty: Some(ty), initializer } =>
            (target, true, ty, initializer),
        _ => return,
    };
    linter.lint_needless_annotation(
        target, mutable, ty, &initializer, exported, stmt.span,
    );
}
fn lint_statement_missing_effects(linter: &mut Linter<'_>, statement: StmtId, exported: bool) {
    let stmt = linter.arena.stmt(statement);
    match stmt.kind {
        ArenaStmtKind::ProcDef(def) | ArenaStmtKind::CliMain(def) => {
            let function = linter.arena.function_def(def);
            let entrypoint = matches!(stmt.kind, ArenaStmtKind::CliMain(_))
                || function.test_declaration
                || (!exported && linter.scopes.len() == 1 && function.name == "main");
            if !entrypoint { linter.lint_effect_annotation(def, stmt.span); }
        }
        ArenaStmtKind::StreamDef(def) => linter.lint_effect_annotation(def, stmt.span),
        _ => {}
    }
}

fn lint_statement_atomically_never_replaces(linter: &mut Linter<'_>, statement: StmtId, _exported: bool) {
    let stmt = linter.arena.stmt(statement);
    if let ArenaStmtKind::Sugar {
        form: super::SugarForm::Atomically,
        operands,
        ..
    } = stmt.kind
        && let super::ArenaSugar::Atomically { body, .. } =
            linter.arena.sugar(super::SugarForm::Atomically, operands)
    {
        super::prefer_atomically::lint_body_that_never_finishes(
            linter,
            stmt.span,
            body,
        );
    }
}






fn lint_statement_adjacent_pattern_arms(linter: &mut Linter<'_>, statement: StmtId, _exported: bool) {
    if let ArenaStmtKind::Match { arms, .. } = linter.arena.stmt(statement).kind {
        let arms = linter.arena.match_arms(arms).iter().map(|arm| {
            (arm.pattern, arm.guard, Ok(arm.block), linter.arena.span(arm.span))
        }).collect();
        linter.lint_adjacent_pattern_arms(arms);
    }
}

fn lint_statement_catch_all_match(linter: &mut Linter<'_>, statement: StmtId, _exported: bool) {
    if let ArenaStmtKind::Match { arms, .. } = linter.arena.stmt(statement).kind
        && let Some(arm) = linter.arena.match_arms(arms).last()
    {
        let report = super::lint_prefer_match_else::catch_all_arm_report(
            linter.arena,
            linter.source,
            arm.pattern,
            arm.guard,
            arm.spelling,
        );
        linter.catch_all_arms.extend(report);
    }
}
fn lint_statement_stringly_match(linter: &mut Linter<'_>, statement: StmtId, _exported: bool) {
    if let ArenaStmtKind::Match { arms, .. } = linter.arena.stmt(statement).kind {
        let count = linter.arena.match_arms(arms).iter().filter(|arm| {
            matches!(
                &linter.arena.pattern(arm.pattern).kind,
                ArenaPatternKind::Literal(expr) if matches!(linter.arena.expr(*expr).kind, ArenaExprKind::Str(_))
            )
        }).count();
        let has_catch_all = linter.arena.match_arms(arms).iter().any(|arm| {
            matches!(linter.arena.pattern(arm.pattern).kind, ArenaPatternKind::Wildcard | ArenaPatternKind::Binding(_))
        });
        if count >= 3 && !has_catch_all {
            linter.warning(
                linter.arena.stmt(statement).span,
                "3+ string-literal match arms — consider defining a tag union type",
                DiagnosticCode::LintStringlyTypedMatch,
                "tag unions are safer and exhaustiveness-checked",
            );
        }
    }
}
fn lint_statement_interactive_command(linter: &mut Linter<'_>, statement: StmtId, _exported: bool) {
    if let ArenaStmtKind::Command(command) = linter.arena.stmt(statement).kind {
        let command = linter.arena.command_stmt(command).clone();
        if let super::ArenaCommand::Proc { name, .. } = command.command {
            linter.lint_interactive_command(
                name.as_str().as_str(),
                linter.arena.span(command.span),
            );
        }
    }
}



fn lint_statement_list_any_bindings(linter: &mut Linter<'_>, statement: StmtId, _exported: bool) {
    match linter.arena.stmt(statement).kind.clone() {
        ArenaStmtKind::Let { target, ty, initializer }
        | ArenaStmtKind::Const { target, ty, initializer }
        | ArenaStmtKind::Var { target, ty, initializer } => {
            linter.declare_list_any_binding(target, ty, &initializer);
        }
        ArenaStmtKind::Assign { target, op, value } => {
            if let Some(definition) = super::lint_list_any_union::assigned_root(linter.arena, target)
                .and_then(|root| linter.binding_definition(root))
            {
                let mut bindings = std::mem::take(&mut linter.list_any_bindings);
                bindings.assign(
                    linter.arena,
                    definition,
                    target,
                    op,
                    &value,
                    &linter.expr_types,
                    &|name| linter.binding_definition(name),
                );
                linter.list_any_bindings = bindings;
            }
        }
        _ => {}
    }
}
fn lint_statement_typed_callable_parameters(linter: &mut Linter<'_>, statement: StmtId, exported: bool) {
    if linter.scopes.len() == 1 {
        match linter.arena.stmt(statement).kind {
            ArenaStmtKind::ProcDef(def) | ArenaStmtKind::CliMain(def) => {
                let function = linter.arena.function_def(def);
                let entrypoint = matches!(linter.arena.stmt(statement).kind, ArenaStmtKind::CliMain(_))
                    || function.test_declaration
                    || (!exported && function.name == "main");
                linter.callable_parameters.define(
                    function.name,
                    def,
                    !exported && !entrypoint,
                );
            }
            ArenaStmtKind::PureDef(def) => {
                let name = linter.arena.function_def(def).name;
                linter.callable_parameters.define(name, def, !exported);
            }
            _ => {}
        }
    }
}
fn lint_statement_size_product_initializer(linter: &mut Linter<'_>, statement: StmtId, _exported: bool) {
    if let ArenaStmtKind::Let { ty: Some(_), initializer: ArenaExprOrRun::Expr(value), .. }
        | ArenaStmtKind::Const { ty: Some(_), initializer: ArenaExprOrRun::Expr(value), .. }
        | ArenaStmtKind::Var { ty: Some(_), initializer: ArenaExprOrRun::Expr(value), .. } =
        linter.arena.stmt(statement).kind
    {
        linter.size_products.typed_initializer(linter.arena, value);
    }
}

fn lint_statement_whole_value_pipeline(linter: &mut Linter<'_>, statement: StmtId, _exported: bool) {
    #[cfg(test)]
    nested_pipeline_index_tests::record_statement();
    let value = match linter.arena.stmt(statement).kind {
        ArenaStmtKind::Let { initializer: ArenaExprOrRun::Expr(value), .. }
        | ArenaStmtKind::Var { initializer: ArenaExprOrRun::Expr(value), .. }
        | ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(value)))
        | ArenaStmtKind::Expr(value) => Some(value),
        _ => None,
    };
    if let Some(value) = value {
        linter.whole_statement_values.insert(value);
    }
}
fn lint_statement_defer_block_helper_call(linter: &mut Linter<'_>, statement: StmtId, _exported: bool) {
    if let ArenaStmtKind::Command(command) = linter.arena.stmt(statement).kind
        && let super::ArenaCommand::Proc { name, .. } = linter.arena.command_stmt(command).command
    {
        linter.defer_block_proc_calls.insert(name);
    }
}


pub(super) fn statement(
    linter: &mut Linter<'_>,
    statement: StmtId,
    requested: StatementSite,
    exported: bool,
) {
    let callback = LINT_RULES
        .iter()
        .filter(|rule| linter.rule_selection.enabled(rule.code))
        .flat_map(|rule| rule.hooks)
        .find_map(|hook| match hook {
            RuleHook::Statement(site, callback) if *site == requested => Some(*callback),
            _ => None,
        });
    if let Some(callback) = callback {
        callback(linter, statement, exported);
    }
}
pub(super) fn parameter(
    linter: &mut Linter<'_>,
    parameter: &xsh::frontend::syntax::arena::ArenaParam,
) {
    for rule in LINT_RULES {
        if !linter.rule_selection.enabled(rule.code) { continue; }
        for hook in rule.hooks {
            if let RuleHook::Parameter(callback) = hook {
                callback(linter, parameter);
                return;
            }
        }
    }
}
fn list_comprehension_sequence(
    linter: &mut Linter<'_>,
    statements: &[StmtId],
    _block: Option<BlockId>,
) {
    linter.lint_list_comp_suggestions(statements);
}
fn dispatch_sequence(linter: &mut Linter<'_>, statements: &[StmtId], block: Option<BlockId>) {
    let site = if block.is_some() { SequenceSite::Block } else { SequenceSite::Root };
    let mut visited = Vec::new();
    for rule in LINT_RULES {
        if !linter.rule_selection.enabled(rule.code) {
            continue;
        }
        for hook in rule.hooks {
            let RuleHook::Sequence(candidate_site, candidate) = hook else { continue; };
            if *candidate_site != site
                || visited
                    .iter()
                    .any(|prior| std::ptr::fn_addr_eq(*prior, *candidate))
            {
                continue;
            }
            visited.push(*candidate);
            candidate(linter, statements, block);
        }
    }
}
pub(super) fn root_sequence(linter: &mut Linter<'_>, statements: &[StmtId]) { dispatch_sequence(linter, statements, None); }
pub(super) fn block_sequence(linter: &mut Linter<'_>, statements: &[StmtId], block: BlockId) { dispatch_sequence(linter, statements, Some(block)); }
pub(super) fn statement_sequence(linter: &mut Linter<'_>, statements: &[StmtId]) {
    const ORDER: [StatementSequenceSite; 9] = [
        StatementSequenceSite::ContextScopeScaffolds,
        StatementSequenceSite::RecordDestructuring,
        StatementSequenceSite::FreshMapInitializations,
        StatementSequenceSite::ListElementReconstruction,
        StatementSequenceSite::NegativeIfGuard,
        StatementSequenceSite::OptionalBinding,
        StatementSequenceSite::WriteMode,
        StatementSequenceSite::TestExpect,
        StatementSequenceSite::EmptySentinel,
    ];
    for requested in ORDER {
        let callback = LINT_RULES
            .iter()
            .filter(|rule| linter.rule_selection.enabled(rule.code))
            .flat_map(|rule| rule.hooks)
            .find_map(|hook| match hook {
                RuleHook::StatementSequence(site, callback) if *site == requested => Some(*callback),
                _ => None,
            });
        if let Some(callback) = callback {
            callback(linter, statements);
        }
    }
}
pub(super) fn statement_pair(
    linter: &mut Linter<'_>,
    previous: StmtId,
    current: StmtId,
    statements: &[StmtId],
) {
    for rule in LINT_RULES {
        if !linter.rule_selection.enabled(rule.code) {
            continue;
        }
        for hook in rule.hooks {
            if let RuleHook::StatementPair(callback) = hook {
                callback(linter, previous, current, statements);
            }
        }
    }
}
pub(super) fn statement_pair_enabled(linter: &Linter<'_>) -> bool {
    LINT_RULES.iter().any(|rule| {
        linter.rule_selection.enabled(rule.code)
            && rule.hooks.iter().any(|hook| matches!(hook, RuleHook::StatementPair(_)))
    })
}
pub(super) fn sequence_exit(
    linter: &mut Linter<'_>,
    scope: SequenceExitSite,
    statements: &[StmtId],
    diagnostic_index: usize,
) {
    let callback = LINT_RULES
        .iter()
        .filter(|rule| linter.rule_selection.enabled(rule.code))
        .flat_map(|rule| rule.hooks)
        .find_map(|hook| match hook {
            RuleHook::SequenceExit(site, callback) if *site == scope => Some(*callback),
            _ => None,
        });
    if let Some(callback) = callback {
        callback(linter, statements, diagnostic_index);
    }
}
fn run_redundant_close_sequence_exit(
    linter: &mut Linter<'_>,
    statements: &[StmtId],
    diagnostic_index: usize,
) {
    let diagnostics = super::redundant_close::lint_deferred_root_closes(linter, statements);
    linter.insert_diagnostics_at(diagnostic_index, diagnostics);
}
pub(super) fn expression(linter: &mut Linter<'_>, expr: ExprId) {
    for rule in LINT_RULES {
        if !linter.rule_selection.enabled(rule.code) {
            continue;
        }
        for hook in rule.hooks {
            let RuleHook::EveryExpression(callback) = hook else { continue; };
            let first = linter.diagnostics.len();
            callback(linter, expr);
            debug_assert!(linter.diagnostics[first..].iter().all(|finding| finding.code == Some(rule.code)), "expression rule emitted another rule's code: {}", rule.code.name());
        }
    }
}
pub(super) fn expression_analysis(linter: &mut Linter<'_>, expr: ExprId) {
    let callbacks = linter.rule_selection.expression_analysis_hooks;
    for callback in callbacks.into_iter().flatten() {
        callback(linter, expr);
    }
}
pub(super) fn expression_rule(linter: &mut Linter<'_>, code: DiagnosticCode, expr: ExprId) {
    let Some(rule) = rule(code) else { return; };
    if !linter.rule_selection.enabled(code) { return; }
    for hook in rule.hooks {
        if let RuleHook::Expression(callback) = hook {
            callback(linter, expr);
        }
    }
}
pub(super) fn expression_site(
    linter: &mut Linter<'_>,
    requested: ExpressionSite,
    expr: ExprId,
    site: super::PathDisplaySite,
) {
    let callback = LINT_RULES
        .iter()
        .filter(|rule| linter.rule_selection.enabled(rule.code))
        .flat_map(|rule| rule.hooks)
        .find_map(|hook| match hook {
            RuleHook::ExpressionSite(found, callback) if *found == requested => Some(*callback),
            _ => None,
        });
    if let Some(callback) = callback {
        callback(linter, expr, site);
    }
}
pub(super) fn command_argument_enabled(
    linter: &Linter<'_>,
    requested: CommandArgumentHook,
) -> bool {
    LINT_RULES.iter().any(|rule| {
        linter.rule_selection.enabled(rule.code)
            && rule.hooks.iter().any(|hook| matches!(hook, RuleHook::CommandArgument(found) if *found == requested))
    })
}
pub(super) fn call_argument(linter: &mut Linter<'_>, argument: &xsh::frontend::syntax::arena::ArenaCallArg) {
    for rule in LINT_RULES {
        if !linter.rule_selection.enabled(rule.code) { continue; }
        for hook in rule.hooks {
            if let RuleHook::CallArgument(callback) = hook {
                callback(linter, argument);
            }
        }
    }
}
fn lint_path_migrations_expression(linter: &mut Linter<'_>, expr: ExprId) {
    linter.lint_path_migrations(expr);
}
fn lint_redundant_single_interpolation_expression(linter: &mut Linter<'_>, expr: ExprId) {
    linter.lint_redundant_single_interpolation(expr);
}
fn lint_call_style_expression(linter: &mut Linter<'_>, expr: ExprId) {
    if let ArenaExprKind::Call { callee, args } = linter.arena.expr(expr).kind {
        let span = linter.arena.expr(expr).span;
        linter.lint_call_style(callee, args, span);
    }
}
pub(super) fn expression_group(linter: &mut Linter<'_>, group: ExpressionGroupSite, expr: ExprId) {
    let callback = LINT_RULES
        .iter()
        .filter(|rule| linter.rule_selection.enabled(rule.code))
        .flat_map(|rule| rule.hooks)
        .find_map(|hook| match hook {
            RuleHook::ExpressionGroup(site, callback) if *site == group => Some(*callback),
            _ => None,
        });
    if let Some(callback) = callback { callback(linter, expr); }
}
pub(super) fn pattern(linter: &mut Linter<'_>, pattern: PatternId) {
    for rule in LINT_RULES {
        if !linter.rule_selection.enabled(rule.code) { continue; }
        for hook in rule.hooks {
            if let RuleHook::Pattern(callback) = hook { callback(linter, pattern); }
        }
    }
}
pub(super) fn scope_exit(linter: &mut Linter<'_>, unused: &[(String, Binding)]) {
    for rule in LINT_RULES {
        if !linter.rule_selection.enabled(rule.code) { continue; }
        for hook in rule.hooks {
            if let RuleHook::ScopeExit(callback) = hook { callback(linter, unused); }
        }
    }
}
pub(super) fn run(linter: &mut Linter<'_>, run: super::RunFormId) {
    for rule in LINT_RULES {
        if !linter.rule_selection.enabled(rule.code) { continue; }
        for hook in rule.hooks {
            if let RuleHook::Run(callback) = hook { callback(linter, run); }
        }
    }
}
pub(super) fn stream_stage(linter: &mut Linter<'_>, stage: &super::ArenaStreamStage) {
    for rule in LINT_RULES {
        if !linter.rule_selection.enabled(rule.code) { continue; }
        for hook in rule.hooks {
            if let RuleHook::StreamStage(callback) = hook {
                callback(linter, stage);
            }
        }
    }
}
pub(super) fn structured_pipeline(
    linter: &mut Linter<'_>,
    input: ExprId,
    stages: super::ArenaRange,
) {
    for rule in LINT_RULES {
        if !linter.rule_selection.enabled(rule.code) { continue; }
        for hook in rule.hooks {
            if let RuleHook::StructuredPipeline(callback) = hook {
                callback(linter, input, stages);
            }
        }
    }
}
pub(super) fn function(
    linter: &mut Linter<'_>,
    body: BlockId,
    result_unit: bool,
    result_path: bool,
    result_ok: Option<&Type>,
    return_type: &Type,
) {
    const ORDER: [FunctionSite; 6] = [
        FunctionSite::BareReturn,
        FunctionSite::TailPathParse,
        FunctionSite::TailRequire,
        FunctionSite::TailOkReturn,
        FunctionSite::TailReturnBinding,
        FunctionSite::TailReturn,
    ];
    for requested in ORDER {
        for rule in LINT_RULES {
            if !linter.rule_selection.enabled(rule.code) {
                continue;
            }
            for hook in rule.hooks {
                if let RuleHook::Function(site, callback) = hook && *site == requested {
                    callback(linter, body, result_unit, result_path, result_ok, return_type);
                }
            }
        }
    }
}
pub(super) fn procedure(
    linter: &mut Linter<'_>,
    requested: ProcedureSite,
    definition: super::FunctionDefId,
    exported: bool,
    entrypoint: bool,
    statement_span: Span,
) {
    for rule in LINT_RULES {
        if !linter.rule_selection.enabled(rule.code) {
            continue;
        }
        for hook in rule.hooks {
            if let RuleHook::Procedure(site, callback) = hook && *site == requested {
                callback(linter, definition, exported, entrypoint, statement_span);
            }
        }
    }
}
pub(super) fn before_statement(linter: &mut Linter<'_>, statement: StmtId) {
    for rule in LINT_RULES {
        if !linter.rule_selection.enabled(rule.code) { continue; }
        for hook in rule.hooks {
            if let RuleHook::BeforeStatement(callback) = hook {
                callback(linter, statement);
            }
        }
    }
}
pub(super) fn binding(
    linter: &mut Linter<'_>,
    name: &str,
    span: Span,
    origin: BindingOrigin,
) {
    for rule in LINT_RULES {
        if !linter.rule_selection.enabled(rule.code) { continue; }
        for hook in rule.hooks {
            if let RuleHook::Binding(callback) = hook {
                callback(linter, name, span, origin);
            }
        }
    }
}
pub(super) fn scalar_iteration(linter: &mut Linter<'_>, expr: ExprId) {
    for rule in LINT_RULES {
        if !linter.rule_selection.enabled(rule.code) {
            continue;
        }
        for hook in rule.hooks {
            if let RuleHook::ScalarIteration(callback) = hook {
                callback(linter, expr);
            }
        }
    }
}
pub(super) fn for_loop(
    linter: &mut Linter<'_>,
    statement: StmtId,
    target: super::BindingTargetId,
    iter: ExprId,
    block: BlockId,
    span: Span,
) {
    scalar_iteration(linter, iter);
    const ORDER: [LoopSite; 5] = [
        LoopSite::ByteIteration,
        LoopSite::MapEntryIteration,
        LoopSite::YieldDelegation,
        LoopSite::FileLines,
        LoopSite::CountedLoop,
    ];
    for site in ORDER {
        for rule in LINT_RULES {
            if !linter.rule_selection.enabled(rule.code) {
                continue;
            }
            for hook in rule.hooks {
                if let RuleHook::Loop(found, callback) = hook && *found == site {
                    callback(linter, statement, target, iter, block, span);
                }
            }
        }
    }
}
fn run_capture_propagation_check(linter: &mut Linter<'_>, run: super::RunFormId) {
    if let Some(diagnostic) = super::lint_redundant_propagation::redundant_capture_propagation(
        linter.arena,
        linter.source,
        run,
    ) {
        linter.diagnostics.push(diagnostic);
    }
}
fn run_segment_checks(linter: &mut Linter<'_>, run: super::RunFormId) {
    let run_form = linter.arena.run_form(run).clone();
    for segment in linter.arena.run_segments(run_form.segments).to_vec() {
        if linter.rule_selection.enabled(DiagnosticCode::LintPreferNonEmptyArgv)
            && let Some(diagnostic) = super::lint_prefer_non_empty_argv::prefer_non_empty_argv(
                linter.arena,
                &linter.unvalidated_command_vectors,
                &segment.target,
            )
        {
            linter.diagnostics.push(diagnostic);
        }
        if linter.rule_selection.enabled(DiagnosticCode::LintRunStatus)
            && let Some(target) = super::literal_command_word(linter.arena, linter.source, &segment.target)
            && segment.kind == super::RunKind::Plain
            && !linter
                .arena
                .run_options(segment.options)
                .iter()
                .any(|option| option.kind == RunOption::Accept)
            && run_form.propagation_written
            && super::expects_nonzero_status(&target)
        {
            let deletion_span = super::scan_run_propagate_deletion_span(
                linter.source,
                linter.arena.span(run_form.span),
            );
            linter.diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    "remove `?` when inspecting an expected nonzero status",
                )
                .with_code(DiagnosticCode::LintRunStatus)
                .with_label(Label::secondary(
                    linter.arena.span(segment.span),
                    "nonzero status is expected for this command",
                ))
                .with_fix_hint(FixHint::deletion(
                    deletion_span,
                    "remove status propagation",
                )),
            );
        }
    }
}
pub(super) fn run_checks(linter: &mut Linter<'_>, run: super::RunFormId) {
    const ORDER: [RunCheckSite; 2] = [
        RunCheckSite::CapturePropagation,
        RunCheckSite::SegmentChecks,
    ];
    for requested in ORDER {
        let callback = LINT_RULES
            .iter()
            .filter(|rule| linter.rule_selection.enabled(rule.code))
            .flat_map(|rule| rule.hooks)
            .find_map(|hook| match hook {
                RuleHook::RunCheck(site, callback) if *site == requested => Some(*callback),
                _ => None,
            });
        if let Some(callback) = callback {
            callback(linter, run);
        }
    }
}
pub(super) fn root_hooks<'source>(
    linter: &mut Linter<'source>,
    stage: RootStage,
    view: &CheckBundleView<'source>,
    statements: &[StmtId],
) {
    for rule in LINT_RULES {
        if !linter.rule_selection.enabled(rule.code) { continue; }
        for hook in rule.hooks {
            if let RuleHook::Root(hook_stage, callback) = hook
                && *hook_stage == stage
            {
                callback(linter, view, statements);
            }
        }
    }
}

impl<'a> Linter<'a> {
    pub(super) fn lint_block_string_concatenation(&mut self, expr: ExprId) {
        fn collect(arena: &AstArena, expr: ExprId, output: &mut String, links: &mut usize) -> bool {
            match arena.expr(expr).kind {
                ArenaExprKind::Str(text) => {
                    output.push_str(arena.string_literal(text));
                    true
                }
                ArenaExprKind::Binary {
                    op: BinaryOp::Add,
                    left,
                    right,
                } => {
                    *links += 1;
                    collect(arena, left, output, links) && collect(arena, right, output, links)
                }
                _ => false,
            }
        }
        let span = self.arena.expr(expr).span;
        let mut value = String::new();
        let mut links = 0;
        if !collect(self.arena, expr, &mut value, &mut links)
            || links == 0
            || !value.contains('\n')
            || value.contains('\r')
        {
            return;
        }
        // Trailing spaces or tabs on a line would become invisible trailing
        // whitespace in a block string, which editors and formatters strip.
        if value.split('\n').any(|line| line.ends_with([' ', '\t'])) {
            return;
        }
        // Report the outermost literal chain once; its operands are part of the same rewrite.
        if self.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(DiagnosticCode::LintPreferBlockString)
                && diagnostic.labels.iter().any(|label| {
                    label.span.source_id == span.source_id
                        && label.span.start() <= span.start()
                        && span.end() <= label.span.end()
                })
        }) {
            return;
        }
        let diagnostic = Diagnostic::new(
            Severity::Warning,
            "constant multiline concatenation can use a block string",
        )
        .with_code(DiagnosticCode::LintPreferBlockString)
        .with_label(Label::secondary(
            span,
            "all pieces are literal text in source order",
        ));
        // Comments and a closing delimiter that cannot stand alone are layout
        // facts: they decide only whether the rewrite is offered.
        let span = widen_over_grouping(self.source, span);
        let suffix = self.source[span.end()..]
            .split(['\r', '\n'])
            .next()
            .unwrap_or("");
        if span_may_contain_comment(self.source, span)
            || !suffix.bytes().all(|byte| matches!(byte, b' ' | b'\t'))
        {
            self.diagnostics.push(diagnostic.with_note(
                "comments, trailing code, and expression consumers need a manual rewrite",
            ));
            return;
        }
        let line_start = self.source[..span.start()]
            .rfind(['\r', '\n'])
            .map_or(0, |offset| offset + 1);
        let indent: String = self.source[line_start..span.start()]
            .chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .collect();
        let margin = format!("{indent}  ");
        let escaped = value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('\0', "\\0");
        let content = escaped
            .split('\n')
            .map(|line| format!("{margin}{line}"))
            .collect::<Vec<_>>()
            .join("\n");
        let replacement = format!("\"\"\"\n{content}\n{margin}\"\"\"");
        let witness = format!("let block_value = {replacement}\n");
        let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(
            xsh::frontend::source::SourceId::new(0),
            &witness,
        );
        let reproduced = parsed.diagnostics.is_empty() && parsed.arena.statement_ids().next().is_some_and(|statement| matches!(
            parsed.arena.arena.stmt(statement).kind,
            ArenaStmtKind::Let { initializer: ArenaExprOrRun::Expr(candidate), .. }
                if matches!(parsed.arena.arena.expr(candidate).kind, ArenaExprKind::Str(text) if parsed.arena.arena.string_literal(text).as_ref() == value)
        ));
        self.diagnostics.push(if reproduced {
            diagnostic.with_fix_hint(FixHint::replacement(
                span,
                "preserve the exact decoded text",
                replacement,
            ))
        } else {
            diagnostic.with_note(
                "a block literal at this indentation would not reproduce the decoded text",
            )
        });
    }

    pub(super) fn lint_comparison_chain(&mut self, expr: ExprId) {
        fn ordering(op: BinaryOp) -> bool {
            matches!(
                op,
                BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
            )
        }
        fn ladder(
            arena: &AstArena,
            expr: ExprId,
            out: &mut Vec<(BinaryOp, ExprId, ExprId)>,
        ) -> bool {
            match arena.expr(expr).kind {
                ArenaExprKind::Binary {
                    op: BinaryOp::And,
                    left,
                    right,
                } => ladder(arena, left, out) && ladder(arena, right, out),
                ArenaExprKind::Binary { op, left, right } if ordering(op) => {
                    out.push((op, left, right));
                    true
                }
                _ => false,
            }
        }
        let span = self.arena.expr(expr).span;
        let Some(suppressed_by) = self.comparison_chain_suppressors(span) else {
            return;
        };
        if !matches!(
            self.arena.expr(expr).kind,
            ArenaExprKind::Binary {
                op: BinaryOp::And,
                ..
            }
        ) {
            return;
        }
        let mut pairs = Vec::new();
        if !ladder(self.arena, expr, &mut pairs) || pairs.len() < 2 {
            return;
        }
        let mut assigned_name_conditions = Vec::new();
        let mut stable = |id| match self.arena.expr(id).kind {
            ArenaExprKind::Ident(name) => {
                let stable_binding = self
                    .scopes
                    .iter()
                    .rev()
                    .find_map(|scope| scope.get(name.as_str().as_str()))
                    .is_some_and(|binding| binding.comparison_stable);
                if !stable_binding || self.assignment_contains(name) {
                    false
                } else {
                    assigned_name_conditions.push(name);
                    true
                }
            }
            ArenaExprKind::Int(_) | ArenaExprKind::Float(_) | ArenaExprKind::Str(_) => true,
            _ => false,
        };
        for adjacent in pairs.windows(2) {
            let shared = adjacent[0].2;
            let repeated = adjacent[1].1;
            if !stable(shared)
                || !stable(repeated)
                || !same_ordering_operand(self.arena, shared, repeated)
            {
                return;
            }
        }
        let Some(original) = self.source.get(span.range()) else {
            return;
        };
        if original.contains('#') {
            return;
        }
        let operand_text = |id| {
            let expr = self.arena.expr(id);
            let text = self.source.get(expr.span.range())?;
            let needs_grouping = matches!(
                expr.kind,
                ArenaExprKind::Binary {
                    op: BinaryOp::ResultFallback
                        | BinaryOp::Or
                        | BinaryOp::And
                        | BinaryOp::Eq
                        | BinaryOp::Ne
                        | BinaryOp::Lt
                        | BinaryOp::Le
                        | BinaryOp::Gt
                        | BinaryOp::Ge
                        | BinaryOp::In
                        | BinaryOp::NotIn,
                    ..
                } | ArenaExprKind::ComparisonChain(_)
                    | ArenaExprKind::If { .. }
                    | ArenaExprKind::Match { .. }
                    | ArenaExprKind::Pipeline { .. }
                    | ArenaExprKind::StructuredPipeline { .. }
            );
            Some(if needs_grouping {
                format!("({text})")
            } else {
                text.to_string()
            })
        };
        let Some(mut replacement) = operand_text(pairs[0].1) else {
            return;
        };
        for (op, _, right) in pairs {
            let Some(text) = operand_text(right) else {
                return;
            };
            replacement.push_str(match op {
                BinaryOp::Lt => " < ",
                BinaryOp::Le => " <= ",
                BinaryOp::Gt => " > ",
                BinaryOp::Ge => " >= ",
                _ => unreachable!(),
            });
            replacement.push_str(&text);
        }
        let diagnostic =
            Diagnostic::new(Severity::Warning, "prefer an ordering comparison chain")
                .with_code(DiagnosticCode::LintPreferComparisonChain)
                .with_label(Label::secondary(
                    span,
                    "the repeated adjacent operand is stable",
                ))
                .with_fix_hint(FixHint::replacement(
                    span,
                    "compare adjacent operands once",
                    replacement,
                ));
        if self.checked_output.is_some() {
            self.diagnostics.push(diagnostic);
        } else {
            self.push_assignment_candidate(
                diagnostic,
                super::AssignmentCandidateCondition::NoAssignedNames(assigned_name_conditions),
                suppressed_by,
            );
        }
    }

    pub(super) fn lint_nested_value_pipeline(&mut self, value: ExprId) {
        let Some((_, args)) = self.pipeline_ordinary_call(value) else {
            return;
        };
        // Whole statement values need no new parentheses or precedence rules.
        if !self.whole_statement_values.contains(&value) {
            return;
        }
        let args = self.arena.call_args(args);
        let inputs = args
            .iter()
            .enumerate()
            .filter_map(|(index, arg)| {
                let expr = pipeline_argument_expr(arg).unwrap();
                self.pipeline_ordinary_call(expr).map(|_| (index, expr))
            })
            .collect::<Vec<_>>();
        let [(index, input)] = inputs.as_slice() else {
            return;
        };
        if args[..*index]
            .iter()
            .any(|arg| !self.pipeline_argument_stable(pipeline_argument_expr(arg).unwrap()))
        {
            return;
        }
        let span = self.arena.expr(value).span;
        let input_span = self.arena.expr(*input).span;
        let Some(original) = self.source.get(span.range()) else {
            return;
        };
        let Some(input_text) = self.source.get(input_span.range()) else {
            return;
        };
        let mut stage = original.to_string();
        stage.replace_range(
            input_span.start() - span.start()..input_span.end() - span.start(),
            "_",
        );
        let replacement = format!("{input_text} |> {stage}");
        if !self.pipeline_rewrite_preserves_types(
            span,
            &replacement,
            value,
            *input,
            span.start(),
            span.start(),
        ) {
            return;
        }
        let mut diagnostic =
            Diagnostic::new(Severity::Warning, "nested calls form a value pipeline")
                .with_code(DiagnosticCode::LintPreferValuePipeline)
                .with_label(Label::secondary(
                    span,
                    "place the retained input at an explicit argument hole",
                ));
        if !original.contains('#') {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                span,
                "use an explicit value pipeline",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    pub(super) fn lint_nested_record_update(&mut self, expr: ExprId) {
        fn selected_path(arena: &AstArena, expr: ExprId, root: Name) -> Option<Vec<Name>> {
            match arena.expr(expr).kind {
                ArenaExprKind::Ident(name) if name == root => Some(Vec::new()),
                ArenaExprKind::Field { base, name } => {
                    let mut path = selected_path(arena, base, root)?;
                    path.push(name);
                    Some(path)
                }
                _ => None,
            }
        }
        fn replacements(
            arena: &AstArena,
            expr: ExprId,
            root: Name,
            prefix: &[Name],
            output: &mut Vec<(Vec<Name>, Option<ExprId>)>,
        ) -> Option<()> {
            let ArenaExprKind::Record(fields) = arena.expr(expr).kind else {
                return None;
            };
            let fields = arena.record_fields(fields);
            let ArenaRecordFieldKind::Spread { expr: spread, .. } = fields.first()?.kind else {
                return None;
            };
            if selected_path(arena, spread, root)?.as_slice() != prefix {
                return None;
            }
            for field in fields.iter().skip(1) {
                let (name, value) = match field.kind {
                    ArenaRecordFieldKind::Named { name, value, .. } => (name, Some(value)),
                    ArenaRecordFieldKind::Shorthand { name, .. } => (name, None),
                    _ => return None,
                };
                let mut path = prefix.to_vec();
                path.push(name);
                if let Some(value) = value
                    && matches!(arena.expr(value).kind, ArenaExprKind::Record(inner) if matches!(arena.record_fields(inner).first().map(|f| &f.kind), Some(ArenaRecordFieldKind::Spread { .. })))
                {
                    replacements(arena, value, root, &path, output)?;
                } else {
                    output.push((path, value));
                }
            }
            Some(())
        }
        let expression = self.arena.expr(expr);
        let ArenaExprKind::Record(fields) = expression.kind else {
            return;
        };
        let Some(ArenaRecordFieldKind::Spread { expr: base, .. }) = self
            .arena
            .record_fields(fields)
            .first()
            .map(|field| &field.kind)
        else {
            return;
        };
        let ArenaExprKind::Ident(root) = self.arena.expr(*base).kind else {
            return;
        };
        let stable = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(root.as_str().as_str()))
            .is_some_and(|binding| !binding.mutable);
        if !stable || self.assignment_contains(root) {
            return;
        }
        let Some(base_ty @ Type::Record(shape)) = self.expr_types.get(&self.arena.expr(*base).span)
        else {
            return;
        };
        if shape.is_empty() {
            return;
        }
        let original = &self.source[expression.span.range()];
        if original.contains('#') {
            return;
        }
        let mut updates = Vec::new();
        if replacements(self.arena, expr, root, &[], &mut updates).is_none()
            || !updates.iter().any(|(path, _)| path.len() > 1)
        {
            return;
        }
        let mut entries = Vec::new();
        for (index, (path, value)) in updates.iter().enumerate() {
            if updates[..index]
                .iter()
                .any(|(prior, _)| path.starts_with(prior) || prior.starts_with(path))
            {
                return;
            }
            let mut selected = base_ty;
            for name in path {
                let label = name.as_str();
                if !label
                    .chars()
                    .next()
                    .is_some_and(|c| c == '_' || c.is_alphabetic())
                    || !label.chars().all(|c| c == '_' || c.is_alphanumeric())
                {
                    return;
                }
                let Type::Record(fields) = selected else {
                    return;
                };
                let Some(field_ty) = fields.get(name) else {
                    return;
                };
                selected = field_ty;
            }
            if let Some(value) = value {
                let Some(actual) = self.expr_types.get(&self.arena.expr(*value).span) else {
                    return;
                };
                if actual.contains_any()
                    || (actual.is_dynamic() && !selected.is_dynamic())
                    || !actual.matches_expected(selected)
                {
                    return;
                }
            }
            let path_text = path
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(".");
            let value_text = value
                .map(|value| &self.source[self.arena.expr(value).span.range()])
                .map(str::to_string)
                .unwrap_or_else(|| path.last().unwrap().to_string());
            entries.push(format!("{path_text}: {value_text}"));
        }
        let replacement = format!("{{...{root}, {}}}", entries.join(", "));
        self.diagnostics.push(
            Diagnostic::warning("nested record spreads can use disjoint update paths")
                .with_code(DiagnosticCode::LintPreferNestedRecordUpdate)
                .with_label(Label::secondary(
                    expression.span,
                    "the repeated record reads are stable and statically known",
                ))
                .with_fix_hint(FixHint::replacement(
                    expression.span,
                    "replace nested spreads with static field paths",
                    replacement,
                )),
        );
    }

    pub(super) fn lint_lookup_sentinel(&mut self, expr: ExprId) {
        let node = self.arena.expr(expr);
        let ArenaExprKind::Binary {
            op: BinaryOp::Eq | BinaryOp::Ne,
            left,
            right,
        } = node.kind
        else {
            return;
        };
        let sentinel = if self.proven_absence_lookup(left) && self.is_negative_one_literal(right) {
            right
        } else if self.proven_absence_lookup(right) && self.is_negative_one_literal(left) {
            left
        } else {
            return;
        };
        if self.source[node.span.range()].contains('#') {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "lookup absence is null rather than a numeric sentinel",
            )
            .with_code(DiagnosticCode::LintLookupAbsence)
            .with_label(Label::secondary(
                node.span,
                "compare the proved lookup result with null",
            ))
            .with_fix_hint(FixHint::replacement(
                self.arena.expr(sentinel).span,
                "use absence",
                "null",
            )),
        );
    }

    pub(super) fn lint_prepared_regex(&mut self, expr: ExprId) {
        if self.regex_recovery_context {
            return;
        }
        let outer = self.arena.expr(expr);
        let ArenaExprKind::Try(inner) = outer.kind else {
            return;
        };
        let call = self.arena.expr(inner);
        let ArenaExprKind::Call { callee, args } = call.kind else {
            return;
        };
        if !is_module_call(self.arena, callee, "regex", "compile")
            || self.expr_types.get(&outer.span) != Some(&Type::Regex)
            || args.len() != 1
        {
            return;
        }
        let argument = match self.arena.call_args(args)[0].kind {
            ArenaCallArgKind::Positional(value) => value,
            ArenaCallArgKind::Named { name, value, .. } if name == "pattern" => value,
            _ => return,
        };
        let value = self.arena.expr(argument);
        let ArenaExprKind::Str(text) = value.kind else {
            return;
        };
        let pattern = self.arena.string_literal(text);
        // Reparse the proposed raw spelling to prove delimiter and decoded-text identity.
        let replacement = if !pattern.contains('"') && !pattern.contains(['\n', '\r']) {
            format!("rx\"{pattern}\"")
        } else {
            format!("rx\"\"\"{pattern}\"\"\"")
        };
        let candidate = format!("let prepared = {replacement}\n");
        let snapshot = super::probes::LintProbeTransport::standalone(
            outer.span.source_id,
            &candidate,
            xsh::frontend::check::CheckOptions::default(),
        );
        if !snapshot.parse_diagnostics.is_empty()
            || snapshot.program.arena.regex_literals.len() != 1
            || snapshot.program.arena.regex_literals[0].pattern.as_ref() != pattern.as_ref()
            || !snapshot
                .checked
                .as_ref()
                .is_some_and(|checked| checked.diagnostics.is_empty())
        {
            return;
        }
        let comments = self.source[outer.span.start()..value.span.start()].contains('#')
            || self.source[value.span.end()..outer.span.end()].contains('#');
        let diagnostic = Diagnostic::new(
            Severity::Warning,
            "prepare static regex patterns with a literal",
        )
        .with_code(DiagnosticCode::LintPreferRegexLiteral)
        .with_label(Label::secondary(
            outer.span,
            "this validated pattern is directly propagated",
        ));
        self.diagnostics.push(if comments {
            diagnostic.with_note("no automatic fix: the call contains comments")
        } else {
            diagnostic.with_fix_hint(FixHint::replacement(
                outer.span,
                "use a prepared regex literal",
                replacement,
            ))
        });
    }

    /// `env.get("NAME")` and `env.Str.NAME` read exactly what `e"NAME"`
    /// reads: the same lookup, the same `Result[Str]`, the same failures. Only
    /// literal identifier names are rewritten. `env.get_or` is left alone:
    /// it fails on a value that is not UTF-8, while `e"NAME" ?? fallback`
    /// would fall back, and it returns a `Result` where `??` returns `Str`.
    pub(super) fn lint_env_string(&mut self, expr: ExprId) {
        let outer = self.arena.expr(expr);
        let name = match outer.kind {
            ArenaExprKind::Call { callee, args }
                if is_module_call(self.arena, callee, "env", "get") && args.len() == 1 =>
            {
                let argument = match self.arena.call_args(args)[0].kind {
                    ArenaCallArgKind::Positional(value) => value,
                    ArenaCallArgKind::Named { name, value, .. } if name == "name" => value,
                    _ => return,
                };
                let ArenaExprKind::Str(text) = self.arena.expr(argument).kind else {
                    return;
                };
                self.arena.string_literal(text).to_string()
            }
            ArenaExprKind::Field { base, name } => {
                let ArenaExprKind::Field {
                    base: module,
                    name: kind,
                } = self.arena.expr(base).kind
                else {
                    return;
                };
                if kind != "Str"
                    || !matches!(self.arena.expr(module).kind, ArenaExprKind::Ident(module) if module == "env")
                {
                    return;
                }
                name.as_str().to_string()
            }
            _ => return,
        };
        // The checked type proves `env` is the module, and the spelling
        // check keeps `$env.Str.NAME` command words and comments untouched.
        let text = &self.source[outer.span.range()];
        if !xsh::frontend::syntax::literal::is_env_string_name(&name)
            || self.scopes.iter().any(|scope| scope.contains_key("env"))
            || self.expr_types.get(&outer.span)
                != Some(&Type::Result(Box::new(Type::Str), Box::new(Type::Error)))
            || !text.starts_with("env.")
            || text.contains('#')
            || self.source[..outer.span.start()].ends_with('$')
        {
            return;
        }
        let replacement = format!("e\"{name}\"");
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "read an environment variable with a literal name as an e-string",
            )
            .with_code(DiagnosticCode::LintPreferEnvString)
            .with_label(Label::secondary(
                outer.span,
                "this reads one named variable",
            ))
            .with_fix_hint(FixHint::replacement(
                outer.span,
                format!("write `{replacement}`"),
                replacement,
            )),
        );
    }

    pub(super) fn map_literal_diagnostic(&mut self, span: Span, replacement: String) {
        let Some(source) = self.source.get(span.range()) else {
            return;
        };
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "prefer a Map literal for fresh Map construction",
        )
        .with_code(DiagnosticCode::LintPreferMapLiteral)
        .with_label(Label::secondary(span, "construct the entries in one Map"));
        if source.contains('#') {
            diagnostic =
                diagnostic.with_note("comments in the initialization require a manual rewrite");
        } else {
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                span,
                "construct one Map literal",
                replacement,
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    pub(super) fn lint_map_literal_chain(&mut self, expr: ExprId) {
        let Some(Type::Map(key_ty, element)) =
            self.expr_types.get(&self.arena.expr(expr).span).cloned()
        else {
            return;
        };
        if !list_splice_element_type_is_precise(&element) {
            return;
        }
        let mut base = expr;
        let mut entries = Vec::new();
        while let Some((receiver, key, value)) = self.map_set_parts(base, &element) {
            entries.push((key, value));
            base = receiver;
        }
        if entries.is_empty() {
            return;
        }
        entries.reverse();
        let Some(mut replacement) = self.map_literal_replacement(&entries) else {
            return;
        };
        if !self.is_empty_map_literal_source(base) {
            let ArenaExprKind::Record(fields) = self.arena.expr(base).kind else {
                return;
            };
            if self.expr_types.get(&self.arena.expr(base).span)
                != Some(&Type::Map(key_ty.clone(), element.clone()))
            {
                return;
            }
            let mut originals = Vec::new();
            for field in self.arena.record_fields(fields) {
                let (value, span) = match field.kind {
                    ArenaRecordFieldKind::Computed { key, value, span } => {
                        if self.expr_types.get(&self.arena.expr(key).span) != Some(key_ty.as_ref())
                        {
                            return;
                        }
                        (value, span)
                    }
                    ArenaRecordFieldKind::Named { value, span, .. } => (value, span),
                    _ => return,
                };
                if self.expr_types.get(&self.arena.expr(value).span) != Some(element.as_ref()) {
                    return;
                }
                let Some(text) = self.source.get(self.arena.span(span).range()) else {
                    return;
                };
                originals.push(text);
            }
            if !originals.is_empty() {
                replacement = format!(
                    "{{{}, {}}}",
                    originals.join(", "),
                    &replacement[1..replacement.len() - 1]
                );
            }
        }
        self.map_literal_diagnostic(self.arena.expr(expr).span, replacement);
    }

    pub(super) fn lint_list_splicing(&mut self, expr: ExprId) {
        let node = self.arena.expr(expr);
        if !matches!(
            node.kind,
            ArenaExprKind::Binary {
                op: BinaryOp::Add,
                ..
            } | ArenaExprKind::Call { .. }
        ) {
            return;
        }
        let Some(expected @ Type::List(element)) = self.expr_types.get(&node.span) else {
            return;
        };
        if !list_splice_element_type_is_precise(element) {
            return;
        }
        let mut parts = Vec::new();
        let mut links = 0;
        let mut literal = false;
        if self
            .collect_list_splice_parts(expr, expected, &mut parts, &mut links, &mut literal)
            .is_none()
            || links == 0
            || (links == 1 && !literal)
        {
            return;
        }
        // A simple receiver update already has a compound-assignment spelling.
        if links == 1 && matches!(node.kind, ArenaExprKind::Call { .. }) {
            return;
        }
        let Some(source) = self.source.get(node.span.range()) else {
            return;
        };
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "prefer one list literal with explicit splices for list construction",
        )
        .with_code(DiagnosticCode::LintPreferListSplicing)
        .with_label(Label::secondary(
            node.span,
            "build the elements in one literal",
        ));
        if source.contains('#') {
            diagnostic =
                diagnostic.with_note("comments in the construction require a manual rewrite");
        } else {
            let mut entries = Vec::with_capacity(parts.len());
            for (splice, value) in parts {
                let Some(source) = self.source.get(self.arena.expr(value).span.range()) else {
                    return;
                };
                entries.push(if splice {
                    format!("@{source}")
                } else {
                    source.to_string()
                });
            }
            diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                node.span,
                "build one spliced list literal",
                format!("[{}]", entries.join(", ")),
            ));
        }
        self.diagnostics.push(diagnostic);
    }

    pub(super) fn lint_adjacent_pattern_arms(&mut self, arms: Vec<PatternArm>) {
        // Bodies compare by canonical tree, not spelling, so formatting cannot
        // change which arms merge.
        let keys: Vec<String> = arms
            .iter()
            .map(|arm| super::super::format::canonical_subtree(self.arena, self.source, arm.2))
            .collect();
        let mut start = 0;
        while start + 1 < arms.len() {
            let body_span = match arms[start].2 {
                Ok(block) => self.arena.span(self.arena.block(block).span),
                Err(expr) => self.arena.expr(expr).span,
            };
            let Some(body) = self.source.get(body_span.range()) else {
                return;
            };
            if arms[start].1.is_some() {
                start += 1;
                continue;
            }
            let mut end = start + 1;
            while end < arms.len() && arms[end].1.is_none() && keys[end] == keys[start] {
                end += 1;
            }
            if end == start + 1 {
                start = end;
                continue;
            }
            let span = Span::new(
                arms[start].3.source_id,
                arms[start].3.start(),
                arms[end - 1].3.end(),
            );
            let Some(original) = self.source.get(span.range()) else {
                return;
            };
            if original.contains('#') {
                start = end;
                continue;
            }
            let patterns: Option<Vec<_>> = arms[start..end]
                .iter()
                .map(|(pattern, _, _, _)| {
                    self.source
                        .get(self.arena.span(self.arena.pattern(*pattern).span).range())
                })
                .collect();
            let Some(patterns) = patterns else { return };
            let replacement = format!("{} => {}", patterns.join(" | "), body);
            let mut candidate = self.source.to_string();
            candidate.replace_range(span.range(), &replacement);
            let (_, cst_errors) =
                xsh::frontend::syntax::cst::SyntaxTree::parse(span.source_id, &candidate);
            let snapshot = super::probes::LintProbeTransport::standalone(
                span.source_id,
                &candidate,
                xsh::frontend::check::CheckOptions::default(),
            );
            if cst_errors.is_empty() && snapshot.parse_diagnostics.is_empty() {
                let Some(checked) = snapshot.checked.as_ref() else {
                    start = end;
                    continue;
                };
                // Rechecking the combined pattern proves equal resolved capture types,
                // including nominal constructor payloads and aliases of those types.
                if checked.diagnostics.is_empty() {
                    self.diagnostics.push(
                        Diagnostic::new(
                            Severity::Warning,
                            "adjacent match arms can share an alternative pattern",
                        )
                        .with_code(DiagnosticCode::LintIdenticalMatchArms)
                        .with_label(Label::secondary(
                            span,
                            "unguarded arms share a checked body and capture contract",
                        ))
                        .with_fix_hint(FixHint::replacement(
                            span,
                            "combine the compatible alternatives",
                            replacement,
                        )),
                    );
                }
            }
            start = end;
        }
    }

    fn lint_boolean_match(&mut self, expr: ExprId) {
        let expression = self.arena.expr(expr);
        let ArenaExprKind::Match { value, arms } = expression.kind else {
            return;
        };
        let [selected, complement] = self.arena.match_expr_arms(arms) else {
            return;
        };
        let wildcard_complement = matches!(
            self.arena.pattern(complement.pattern).kind,
            ArenaPatternKind::Wildcard
        );
        let result_complement = matches!(
            self.expr_types.get(&self.arena.expr(value).span),
            Some(Type::Result(_, _))
        ) && match (
            self.arena.pattern(selected.pattern).kind.clone(),
            self.arena.pattern(complement.pattern).kind.clone(),
        ) {
            (
                ArenaPatternKind::Constructor {
                    name: left,
                    arg: Some(left_arg),
                },
                ArenaPatternKind::Constructor {
                    name: right,
                    arg: Some(right_arg),
                },
            ) => {
                ((left == "Ok" && right == "Err") || (left == "Err" && right == "Ok"))
                    && matches!(
                        self.arena.pattern(left_arg).kind,
                        ArenaPatternKind::Wildcard
                    )
                    && matches!(
                        self.arena.pattern(right_arg).kind,
                        ArenaPatternKind::Wildcard
                    )
            }
            _ => false,
        };
        if selected.guard.is_some()
            || complement.guard.is_some()
            || !(wildcard_complement || result_complement)
        {
            return;
        }
        let (ArenaExprKind::Bool(yes), ArenaExprKind::Bool(no)) = (
            self.arena.expr(selected.value).kind,
            self.arena.expr(complement.value).kind,
        ) else {
            return;
        };
        if yes == no {
            return;
        }
        let span = expression.span;
        let mut diagnostic =
            Diagnostic::new(Severity::Warning, "boolean match can use a pattern test")
                .with_code(DiagnosticCode::LintBooleanPatternTest)
                .with_label(Label::secondary(
                    span,
                    "test the selected pattern with `is`",
                ));
        if self.pattern_test_fix_is_nonbinding(selected.pattern)
            && self
                .source
                .get(span.range())
                .is_some_and(|source| !source.contains('#'))
        {
            let subject = self.arena.expr(value).span;
            let pattern = self.arena.span(self.arena.pattern(selected.pattern).span);
            if let (Some(subject), Some(pattern)) = (
                self.source.get(subject.range()),
                self.source.get(pattern.range()),
            ) {
                let pattern = if matches!(
                    self.arena.pattern(selected.pattern).kind,
                    ArenaPatternKind::Alternation(_)
                ) {
                    format!("({pattern})")
                } else {
                    pattern.to_string()
                };
                let replacement = if yes {
                    format!("(({subject}) is {pattern})")
                } else {
                    format!("!(({subject}) is {pattern})")
                };
                diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                    span,
                    "use a non-binding pattern test",
                    replacement,
                ));
            }
        }
        self.diagnostics.push(diagnostic);
    }

    fn lint_pattern_conditional_expr(&mut self, expr: ExprId) {
        let expression = self.arena.expr(expr);
        let ArenaExprKind::Match { value, arms } = expression.kind else {
            return;
        };
        let [selected, complement] = self.arena.match_expr_arms(arms) else {
            return;
        };
        if matches!(self.arena.expr(selected.value).kind, ArenaExprKind::Bool(_))
            && matches!(
                self.arena.expr(complement.value).kind,
                ArenaExprKind::Bool(_)
            )
        {
            return;
        }
        if selected.guard.is_some()
            || complement.guard.is_some()
            || !self.pattern_condition_selected_binds(selected.pattern)
            || !self.pattern_condition_complement(value, selected.pattern, complement.pattern)
        {
            return;
        }
        let span = expression.span;
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "two-arm match can use a pattern conditional",
        )
        .with_code(DiagnosticCode::LintPatternConditional)
        .with_label(Label::secondary(
            span,
            "bind the selected pattern with `if let`",
        ));
        if self
            .source
            .get(span.range())
            .is_some_and(|source| !source.contains('#'))
        {
            let subject = self.source.get(self.arena.expr(value).span.range());
            let pattern = self.source.get(
                self.arena
                    .span(self.arena.pattern(selected.pattern).span)
                    .range(),
            );
            let branch = |value| {
                self.source
                    .get(self.arena.expr(value).span.range())
                    .map(|text| {
                        if matches!(self.arena.expr(value).kind, ArenaExprKind::ValueBlock(_)) {
                            text.to_string()
                        } else {
                            format!("{{ {text} }}")
                        }
                    })
            };
            if let (Some(subject), Some(pattern), Some(yes), Some(no)) = (
                subject,
                pattern,
                branch(selected.value),
                branch(complement.value),
            ) && let Some(replacement) = self.pattern_conditional_replacement(
                span,
                &format!("if let {pattern} = {subject} {yes} else {no}"),
                true,
            ) {
                diagnostic = diagnostic.with_fix_hint(FixHint::replacement(
                    span,
                    "use a pattern conditional",
                    replacement,
                ));
            }
        }
        self.diagnostics.push(diagnostic);
    }

    fn lint_error_fallback_block(&mut self, expr: ExprId) {
        let expression = self.arena.expr(expr);
        let ArenaExprKind::Match { value, arms } = expression.kind else {
            return;
        };
        let [first, second] = self.arena.match_expr_arms(arms) else {
            return;
        };
        if first.guard.is_some() || second.guard.is_some() {
            return;
        }
        let constructor = |pattern| match self.arena.pattern(pattern).kind {
            ArenaPatternKind::Constructor {
                name,
                arg: Some(argument),
            } => Some((name, argument)),
            _ => None,
        };
        let (Some((first_name, first_arg)), Some((second_name, second_arg))) =
            (constructor(first.pattern), constructor(second.pattern))
        else {
            return;
        };
        let (success, success_arg, handler, error_arg) =
            if first_name == "Ok" && second_name == "Err" {
                (first, first_arg, second, second_arg)
            } else if first_name == "Err" && second_name == "Ok" {
                (second, second_arg, first, first_arg)
            } else {
                return;
            };
        let ArenaPatternKind::Binding(success_name) = self.arena.pattern(success_arg).kind else {
            return;
        };
        let identity = match self.arena.expr(success.value).kind {
            ArenaExprKind::Ident(name) => name == success_name,
            ArenaExprKind::ValueBlock(block) => {
                let statements = self
                    .arena
                    .stmt_ids(self.arena.block(block).statements)
                    .collect::<Vec<_>>();
                matches!(statements.as_slice(), [stmt] if match self.arena.stmt(*stmt).kind {
                    ArenaStmtKind::TailBareIdent(name) => name == success_name,
                    ArenaStmtKind::Expr(value) => matches!(self.arena.expr(value).kind, ArenaExprKind::Ident(name) if name == success_name),
                    _ => false,
                })
            }
            _ => false,
        };
        if !identity {
            return;
        }
        let error_name = match self.arena.pattern(error_arg).kind {
            ArenaPatternKind::Binding(name)
                if !self.tag_variants.contains(name.as_str().as_str()) =>
            {
                name.to_string()
            }
            ArenaPatternKind::Wildcard => "_".to_string(),
            _ => return,
        };
        let Some(Type::Result(success_type, _)) = self.expr_types.get(&self.arena.expr(value).span)
        else {
            return;
        };
        if success_type.contains_any()
            || **success_type == Type::Unknown
            || self.expr_types.get(&expression.span) != Some(success_type.as_ref())
        {
            return;
        }
        let Some(original) = self.source.get(expression.span.range()) else {
            return;
        };
        if original.contains('#') {
            return;
        }
        let Some(subject) = self.source.get(self.arena.expr(value).span.range()) else {
            return;
        };
        let Some(handler_source) = self.source.get(self.arena.expr(handler.value).span.range())
        else {
            return;
        };
        let line_indent = |offset: usize| {
            self.source[..offset]
                .rsplit('\n')
                .next()
                .unwrap_or("")
                .chars()
                .take_while(|character| *character == ' ')
                .count()
        };
        let indent = line_indent(expression.span.start());
        let handler = if matches!(
            self.arena.expr(handler.value).kind,
            ArenaExprKind::ValueBlock(_)
        ) {
            let Some(rest) = handler_source.strip_prefix('{') else {
                return;
            };
            let removed_indent =
                line_indent(self.arena.expr(handler.value).span.start()).saturating_sub(indent);
            let mut lines = rest.split('\n');
            let mut body = lines.next().unwrap_or("").to_string();
            for line in lines {
                body.push('\n');
                let trim = line
                    .bytes()
                    .take(removed_indent)
                    .take_while(|byte| *byte == b' ')
                    .count();
                body.push_str(&line[trim..]);
            }
            format!("{{ |{error_name}|{body}")
        } else {
            // Keep continuation lines intact: formatting may wrap an expression
            // handler without changing its lazy fallback behavior.
            format!(
                "{{ |{error_name}|\n{}{handler_source}\n{}}}",
                " ".repeat(indent + 2),
                " ".repeat(indent)
            )
        };
        let subject = match self.arena.expr(value).kind {
            ArenaExprKind::Ident(_)
            | ArenaExprKind::Call { .. }
            | ArenaExprKind::Field { .. }
            | ArenaExprKind::Index { .. }
            | ArenaExprKind::Try(_) => subject.to_string(),
            _ => format!("({subject})"),
        };
        let replacement = format!("{subject} ?? {handler}");
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "identity success match can use an error fallback block",
            )
            .with_code(DiagnosticCode::LintErrorFallbackBlock)
            .with_label(Label::secondary(
                expression.span,
                "retain the lazy error handler with `??`",
            ))
            .with_fix_hint(FixHint::replacement(
                expression.span,
                "use an error fallback block",
                replacement,
            )),
        );
    }

    fn lint_path_roundtrip(&mut self, expr: ExprId) {
        let expr_span = self.arena.expr(expr).span;
        if !matches!(self.expr_types.get(&expr_span), Some(Type::Path)) {
            return;
        }
        let Some((label_span, replacement)) = self.path_roundtrip_replacement(expr) else {
            return;
        };
        self.push_path_roundtrip_diagnostic(expr_span, label_span, replacement);
    }

    fn lint_redundant_require(&mut self, expr: ExprId) {
        let arena_expr = self.arena.expr(expr);
        let ArenaExprKind::Try(inner) = arena_expr.kind else {
            return;
        };
        let Some((label_span, replacement)) = self.require_replacement(inner) else {
            return;
        };
        self.push_redundant_require_diagnostic(arena_expr.span, label_span, replacement);
    }

    fn lint_known_field_access(&mut self, expr: ExprId) {
        let outer = self.arena.expr(expr);
        let ArenaExprKind::Try(call) = outer.kind else {
            return;
        };
        let expression = self.arena.expr(call);
        let ArenaExprKind::Call { callee, args } = expression.kind else {
            return;
        };
        let ArenaExprKind::Field { base, name } = self.arena.expr(callee).kind else {
            return;
        };
        if name != "get" || !self.proven_materialized_record(base) {
            return;
        }
        let [argument] = self.arena.call_args(args) else {
            return;
        };
        let key = match argument.kind {
            ArenaCallArgKind::Positional(key) => key,
            ArenaCallArgKind::Named { name, value, .. } if name == "field" => value,
            _ => return,
        };
        let ArenaExprKind::Str(key) = self.arena.expr(key).kind else {
            return;
        };
        let field = self.arena.string_literal(key);
        if !self.bare_field_label(field) {
            return;
        }
        let Some(Type::Record(fields)) = self.expr_types.get(&self.arena.expr(base).span) else {
            return;
        };
        if fields.contains_key(&Name::intern("get")) {
            return;
        }
        let Some(expected) = fields.get(&Name::intern(field)) else {
            return;
        };
        let Some(Type::Result(success, _)) = self.expr_types.get(&expression.span) else {
            return;
        };
        if success.as_ref() != expected || self.expr_types.get(&outer.span) != Some(expected) {
            return;
        }
        let span = self.expression_source_span(outer.span);
        let receiver_span = self.expression_source_span(self.arena.expr(base).span);
        let cst = self
            .source_cst
            .get()
            .expect("expression span initializes CST");
        if cst.contains_comment(span) {
            return;
        }
        let receiver = &self.source[receiver_span.range()];
        let receiver = if matches!(self.arena.expr(base).kind, ArenaExprKind::Record(_)) {
            format!("({receiver})")
        } else {
            receiver.to_owned()
        };
        self.diagnostics.push(
            Diagnostic::warning(
                "a guaranteed field on an ordinary record can be selected directly",
            )
            .with_code(DiagnosticCode::LintPreferKnownFieldAccess)
            .with_label(Label::secondary(
                span,
                "the selected value retains its exact checked type",
            ))
            .with_fix_hint(FixHint::replacement(
                span,
                "select the known record field",
                format!("{receiver}.{field}"),
            )),
        );
    }

    fn lint_inferred_require_target(&mut self, expr: ExprId) {
        let expression = self.arena.expr(expr);
        let ArenaExprKind::Require {
            schema: Some(schema),
            ..
        } = expression.kind
        else {
            return;
        };
        let Some(actual) = self.requirement_targets.get(&expression.span) else {
            return;
        };
        let Some(expected) = self.requirement_expected_targets.get(&expression.span) else {
            return;
        };
        if actual.ty != expected.ty || actual.context != expected.context {
            return;
        }
        let span = self.arena.type_expr_span(schema);
        // Only the schema token range is removed. Comments inside that range
        // carry author intent and must survive a source rewrite.
        if self
            .source
            .get(expression.span.range())
            .is_none_or(|text| text.contains('#'))
        {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "schema target is supplied by the checked boundary",
            )
            .with_code(DiagnosticCode::LintInferredRequireTarget)
            .with_label(Label::secondary(
                span,
                "the same concrete schema is independently known here",
            ))
            .with_fix_hint(FixHint::replacement(
                span,
                "infer the validation target",
                String::new(),
            )),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{CheckedFact, LINT_RULES, LintOptions, Linter, RuleSelection, lint_codes, rule};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticFamily};

    fn shadowing_diagnostics(source: &str) -> Vec<Diagnostic> {
        use xsh::frontend::check::Checker;
        use xsh::frontend::source::SourceId;
        use xsh::frontend::syntax::parser::Parser;

        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}\n{source}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}\n{source}", checked.diagnostics);
        Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                only: Some(vec![DiagnosticCode::LintShadowing]),
                ..LintOptions::default()
            },
        )
        .diagnostics
    }

    fn exported_callable(kind: &str, name: &str, parameter: &str) -> String {
        match kind {
            "pure" => format!("export pure {name}({parameter}: Int) -> Int {{ {parameter} }}\n"),
            "proc" => format!("export proc {name}({parameter}: Int) [] {{}}\n"),
            "stream" => format!(
                "export stream {name}({parameter}: Int) [] -> Stream[Int] {{ yield {parameter} }}\n"
            ),
            _ => unreachable!(),
        }
    }

    fn named_call(kind: &str, name: &str, parameter: &str) -> String {
        match kind {
            "pure" => format!("pure invoke() -> Int {{ {name}({parameter}: 1) }}\n"),
            "proc" => format!("proc invoke() [] {{ {name}({parameter}: 1) }}\n"),
            "stream" => format!("proc invoke() [] {{ let _ = {name}({parameter}: 1) }}\n"),
            _ => unreachable!(),
        }
    }

    #[test]
    fn table_contains_every_lint_code_once() {
        let codes: Vec<_> = LINT_RULES.iter().map(|rule| rule.code).collect();
        assert_eq!(codes.len(), 147);
        for (index, code) in codes.iter().enumerate() {
            assert_eq!(code.family(), DiagnosticFamily::Lint);
            assert!(codes[..index].iter().all(|previous| previous != code), "duplicate rule: {}", code.name());
        }
        let expected: Vec<_> = DiagnosticCode::ALL.iter().copied().filter(|code| code.family() == DiagnosticFamily::Lint).collect();
        assert_eq!(lint_codes().collect::<Vec<_>>(), expected);
    }
    #[test]
    fn selected_facts_follow_only_and_exclusion() {
        let selection = |only, excluded_rules| {
            RuleSelection::new(&LintOptions {
                only,
                excluded_rules,
                ..LintOptions::default()
            })
        };
        let no_facts = selection(Some(vec![DiagnosticCode::LintRunless]), Vec::new());
        let unused_local = selection(Some(vec![DiagnosticCode::LintUnusedLocal]), Vec::new());
        let checked = selection(Some(vec![DiagnosticCode::LintPreferListSplicing]), Vec::new());
        let default_parameter = selection(Some(vec![DiagnosticCode::LintDefaultParamType]), Vec::new());
        let parser_alias = selection(Some(vec![DiagnosticCode::LintBlockHeader]), Vec::new());
        let excluded = selection(
            Some(vec![DiagnosticCode::LintPreferListSplicing]),
            vec![DiagnosticCode::LintPreferListSplicing],
        );
        assert!(!no_facts.requires_check_output());
        assert_eq!(
            rule(DiagnosticCode::LintUnusedLocal).unwrap().facts,
            &[CheckedFact::LocalBindingTypes],
        );
        assert!(unused_local.requires_check_output());
        assert!(!selection(Some(vec![DiagnosticCode::LintRunStatus]), Vec::new()).requires_check_output());
        assert!(selection(Some(vec![DiagnosticCode::LintPreferMapLiteral]), Vec::new()).requires_check_output());
        assert!(!selection(Some(vec![DiagnosticCode::LintPreferEmptyMapLiteral]), Vec::new()).requires_check_output());
        assert!(default_parameter.requires_check_output());
        assert!(parser_alias.enabled(DiagnosticCode::LintBlockHeader));
        assert!(!parser_alias.requires_check_output());
        assert!(checked.requires_check_output());
        assert!(!excluded.requires_check_output());
        assert!(checked.enabled(DiagnosticCode::LintPreferListSplicing));
        for code in [
            DiagnosticCode::LintCompatibilityVocabulary,
            DiagnosticCode::LintPositionalErrorArguments,
            DiagnosticCode::LintPublicResultError,
            DiagnosticCode::LintRemovedRecordRequire,
            DiagnosticCode::LintPreferCallableAlias,
            DiagnosticCode::LintPreferSignatureCli,
            DiagnosticCode::LintNeedlessAnnotation,
            DiagnosticCode::LintMissingEffects,
            DiagnosticCode::LintPreferInferredPrivateEffects,
            DiagnosticCode::LintPreferInferredPureReturn,
            DiagnosticCode::LintPreferInferredProcReturn,
            DiagnosticCode::LintRedundantClose,
            DiagnosticCode::LintPreferTempdir,
            DiagnosticCode::LintPreferAtomicallyReplace,
            DiagnosticCode::LintPreferForIndex,
            DiagnosticCode::LintPreferTempdirScope,
            DiagnosticCode::LintPreferWriteMode,
            DiagnosticCode::LintPreferTryCapture,
            DiagnosticCode::LintDefaultParamType,
            DiagnosticCode::LintEmptySentinel,
            DiagnosticCode::LintLookupAbsence,
            DiagnosticCode::LintPatternConditional,
            DiagnosticCode::LintRedundantOptionalFallback,
            DiagnosticCode::LintRedundantPathParse,
            DiagnosticCode::LintRedundantRequire,
            DiagnosticCode::LintPathConstructor,
            DiagnosticCode::LintPreferImplicitMessage,
            DiagnosticCode::LintPreferIn,
            DiagnosticCode::LintPreferPropagation,
            DiagnosticCode::LintPreferRecordDestructuring,
            DiagnosticCode::LintPreferRunArgv,
            DiagnosticCode::LintRedundantDefault,
            DiagnosticCode::LintRedundantDisplayParse,
            DiagnosticCode::LintRedundantPathDisplay,
            DiagnosticCode::LintRedundantPathInterpolation,
            DiagnosticCode::LintRedundantPropagation,
            DiagnosticCode::LintRedundantStringInterpolation,
            DiagnosticCode::LintStageCallable,
            DiagnosticCode::LintPreferTypedCallable,
            DiagnosticCode::LintRedundantOkTail,
            DiagnosticCode::LintRedundantTailReturnBinding,
            DiagnosticCode::LintRedundantTailReturn,
            DiagnosticCode::LintRedundantCommandFmt,
        ] {
            let checked_rule = selection(Some(vec![code]), Vec::new());
            assert!(checked_rule.enabled(code), "{}", code.name());
            assert!(checked_rule.requires_check_output(), "{}", code.name());
        }
        assert!(!excluded.enabled(DiagnosticCode::LintPreferListSplicing));

        let annotation_policy = RuleSelection::new(&LintOptions {
            only: Some(vec![DiagnosticCode::LintPreferInferredProcReturn]),
            annotation_policy_writes_returns: true,
            ..LintOptions::default()
        });
        assert!(!annotation_policy.enabled(DiagnosticCode::LintPreferInferredProcReturn));
        assert!(!annotation_policy.requires_check_output());
    }

    #[test]
    fn selecting_an_opt_in_rule_runs_its_table_hook() {
        use xsh::diagnostic::DiagnosticCode;
        use xsh::frontend::check::Checker;
        use xsh::frontend::source::SourceId;
        use xsh::frontend::syntax::parser::Parser;

        let source = "pure probe(line: Str) -> Str {\n  let parts = line.split(\"=\")\n  let key = parts[0]\n  let value = parts[1]\n  if line.starts_with(\"#define \") {\n    return line.byte_slice(8, line.byte_len() - 8)\n  }\n  key + value\n}\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let diagnostics = Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                checked_output: Some(std::sync::Arc::new(checked)),
                only: Some(vec![DiagnosticCode::LintPreferTextPattern]),
                ..LintOptions::default()
            },
        )
        .diagnostics;
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferTextPattern))
                .count(),
            2,
            "{diagnostics:?}"
        );
    }

    #[test]
    fn source_only_lint_has_no_unprovided_checked_facts() {
        use xsh::diagnostic::DiagnosticCode;
        use xsh::frontend::check::Checker;
        use xsh::frontend::source::SourceId;
        use xsh::frontend::syntax::parser::Parser;

        let source = "proc launch(argv: List[Str]) [process, error] -> Result[Status, ProcessError] {\n  let status = process.run(process.command_argv(argv[0], argv))?\n  Ok(status)\n}\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let selected = || {
            LintOptions {
                only: Some(vec![DiagnosticCode::LintPreferRunArgv]),
                ..LintOptions::default()
            }
        };
        let source_only = Linter::lint(&parsed.arena, source, selected());
        assert!(source_only.diagnostics.is_empty());

        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let with_facts = Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                checked_output: Some(std::sync::Arc::new(checked)),
                only: Some(vec![DiagnosticCode::LintPreferRunArgv]),
                ..LintOptions::default()
            },
        );
        assert_eq!(
            with_facts
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferRunArgv))
                .count(),
            1,
            "{:?}",
            with_facts.diagnostics
        );
    }

    #[test]
    fn exported_callable_labels_may_match_module_callable_names() {
        for kind in ["proc", "pure", "stream"] {
            for sibling_first in [true, false] {
                let sibling = "pure peer() -> Int { 1 }\n";
                let exported = exported_callable(kind, "target", "peer");
                let mut source = if sibling_first {
                    format!("{sibling}{exported}")
                } else {
                    format!("{exported}{sibling}")
                };
                source.push_str(&named_call(kind, "target", "peer"));
                assert!(
                    shadowing_diagnostics(&source).is_empty(),
                    "{kind}, sibling_first={sibling_first}: {source}"
                );
            }

            let self_name = format!("{kind}_self");
            let mut source = exported_callable(kind, &self_name, &self_name);
            source.push_str(&named_call(kind, &self_name, &self_name));
            assert!(
                shadowing_diagnostics(&source).is_empty(),
                "{kind} self label: {source}"
            );
        }
    }

    #[test]
    fn shadowing_still_covers_private_bindings_and_module_values() {
        let source = "pure peer() -> Int { 1 }\npure private_target(peer: Int) -> Int { peer }\nlet existing = 1\nexport pure value_target(existing: Int) -> Int { existing }\npure local_name() -> Int { 1 }\npure pattern_name() -> Int { 2 }\npure callback_name() -> Int { 3 }\npure exercise(flag: Bool, items: List[Int]) -> Int {\n  let local_name = 4\n  let matched = match flag { pattern_name => 5 }\n  let mapped = items |> map { |callback_name| callback_name }\n  local_name + matched + mapped[0]\n}\n";
        let diagnostics = shadowing_diagnostics(source);
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintShadowing))
                .count(),
            5,
            "{diagnostics:?}"
        );
    }
}
