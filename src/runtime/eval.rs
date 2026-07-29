#![allow(clippy::single_call_fn)]

use crate::diagnostic::Diagnostic;
use crate::modules::RuntimeOp;
use crate::modules::api_spec;
use crate::modules::net::{NetAgent, NetAgentKey, NetPoolOptions};
use crate::runtime::process::{
    CancellationDecision, CancellationPolicy, ManagedChild, ProcessGroup, ProcessSegmentStatus,
    ProcessSegmentStatusKind, ProcessStatus, ProcessStatusKind, SignalHandlerGuard, cancel_managed,
    install_hook_signal_handler, path_bytes, release_to_reaper, signal_snapshot,
};
use crate::runtime::signal::{
    HookSignal, hook_signal_from_number, normalize_hook_signal, signal_rejection_message,
};
use crate::runtime::value::{
    AbortSignal, CommandPlan, DigestValue, DurationValue, ErrorContext, FloatValue, FunctionName,
    PathValue, ProcessHandleValue, RecordMap, RegexValue, ResultValue, RuntimeError, StreamValue,
    Value,
};
use crate::sema::check::{Checker, CompactBodyProbeOutput, CompactDeclOutput};
use crate::sema::types::{CallableType, Type};
use crate::source::{SourceId, SourceMap, Span};
use crate::symbol::{Name, NameText, QualifiedName};
use crate::syntax::arena::{
    ArenaCallArg, ArenaCallArgKind, ArenaExprKind, ArenaProgram, ArenaStmtKind, ExprId, StmtId,
};
use crate::syntax::node::{AssignOp, BinaryOp, FormatSpec, RedirectionKind, RunKind};
use crate::trace::{
    TraceArg, TraceEnv, TraceError, TraceEvent, TraceKind, TracePayload, TraceStatus,
    TraceStatusKind, TraceTiming, Traceback, TracebackFrame,
};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod indexed;
mod lower;
use indexed::full::{FullBuilder, FullProgram};
mod lowered_ops;
use lowered_ops::{lowered_value_from_runtime, lowered_value_from_runtime_any};
mod lowered_run;
mod modules;
mod process_handle;
mod stream;

#[derive(Clone, Debug, Default)]
pub struct EvalOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub trace_events: Vec<TraceEvent>,
    pub diagnostics: Vec<Diagnostic>,
    pub traceback: Option<Traceback>,
    pub sources: Arc<SourceMap>,
    pub status: u8,
    pub cwd: PathBuf,
    pub env: BTreeMap<Vec<u8>, Vec<u8>>,
    pub last_status: Option<ProcessStatus>,
}

#[cfg(feature = "native-tests")]
#[derive(Clone, Debug, Default)]
pub struct TestEvalOutput {
    pub output: EvalOutput,
    pub result: Option<Value>,
}

#[cfg(feature = "native-tests")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeTestRunKind {
    Xsh,
    XshtTrace,
}

#[cfg(feature = "native-tests")]
#[derive(Clone, Debug)]
pub struct NativeTestRunRequest {
    pub kind: NativeTestRunKind,
    pub script_path: PathValue,
    pub source: String,
    pub tool_args: Vec<String>,
    pub script_args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub stdin: Vec<u8>,
    pub span: Span,
}

#[cfg(feature = "native-tests")]
pub type NativeTestHost =
    Arc<dyn Fn(NativeTestRunRequest) -> Result<Value, RuntimeError> + Send + Sync>;

#[cfg(feature = "native-tests")]
pub struct PreparedTestProgram {
    plan: CompactIndexedRunPlan,
    script_span: Span,
    symbols: crate::symbol::SymbolOwner,
    shared: Arc<LoweredSharedState>,
    setup_shared: Arc<LoweredSharedState>,
    setup_failure: Option<TestEvalOutput>,
}

#[derive(Clone)]
pub(crate) struct CompactIndexedRunPlan {
    statements: Vec<CompactIndexedDriverStepPlan>,
    script_span: Span,
    auto_main_required: bool,
    compact_auto_main_args: Vec<Value>,
}

#[derive(Clone)]
struct CompactIndexedDriverStepPlan {
    span: Span,
    skip_auto_main: bool,
}

#[derive(Clone, Debug, Default)]
pub struct CompactLowerBodyProbeOutput {
    pub functions: usize,
    pub lowerable_functions: usize,
    pub top_level_statements: usize,
    pub lowerable_top_level_statements: usize,
    pub statements: usize,
    pub lowerable_statements: usize,
    pub expressions: usize,
    pub lowerable_expressions: usize,
    pub patterns: usize,
    pub lowerable_patterns: usize,
    pub unsupported_statements: usize,
    pub unsupported_expressions: usize,
    pub unsupported_patterns: usize,
    pub expr_type_facts: usize,
}

impl CompactLowerBodyProbeOutput {
    /// Derive body-probe lowerability counts from the Construct probe's real
    /// lowering output. This replaces the former hand-synced `can_lower_*` gate
    /// (`CompactLowerBodyProbe`) with the single source of truth: the real
    /// lowering, which records `blocker_events` for any sub-node it cannot lower
    /// and refuses to commit functions whose bodies produced blockers.
    pub fn from_construct(constructed: &CompactLowerConstructProbeOutput) -> Self {
        Self {
            functions: constructed.functions,
            lowerable_functions: constructed.constructed_functions,
            top_level_statements: constructed.top_level_statements,
            lowerable_top_level_statements: constructed.constructed_top_level_statements,
            statements: constructed.statements,
            lowerable_statements: constructed.constructed_statements,
            expressions: constructed.expressions,
            lowerable_expressions: constructed.constructed_expressions,
            patterns: constructed.patterns,
            lowerable_patterns: constructed.constructed_patterns,
            unsupported_statements: constructed
                .statements
                .saturating_sub(constructed.constructed_statements),
            unsupported_expressions: constructed
                .expressions
                .saturating_sub(constructed.constructed_expressions),
            unsupported_patterns: constructed
                .patterns
                .saturating_sub(constructed.constructed_patterns),
            expr_type_facts: constructed.expr_type_facts,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CompactLowerConstructProbeOutput {
    pub functions: usize,
    pub constructed_functions: usize,
    pub constructed_auto_main_functions: usize,
    pub function_blockers: [usize; COMPACT_FUNCTION_BLOCKER_KIND_COUNT],
    pub function_return_type_tags: [u32; COMPACT_TYPE_EXPR_TAG_COUNT],
    pub function_param_type_tags: [u32; COMPACT_TYPE_EXPR_TAG_COUNT],
    pub function_body_tail_stmt_kinds: [u32; COMPACT_STMT_KIND_COUNT],
    pub function_body_tail_command_kinds: [u32; COMPACT_COMMAND_BLOCKER_KIND_COUNT],
    pub function_body_tail_call_callees: BTreeMap<String, u32>,
    pub top_level_statements: usize,
    pub constructed_top_level_statements: usize,
    pub top_level_blockers: [usize; COMPACT_TOP_LEVEL_BLOCKER_KIND_COUNT],
    pub top_level_blocker_sample_spans: BTreeMap<String, Vec<Span>>,
    pub top_level_blocker_stmt_kinds: [u32; COMPACT_STMT_KIND_COUNT],
    pub top_level_binding_type_annotation_tags: [u32; COMPACT_TYPE_EXPR_TAG_COUNT],
    pub top_level_binding_type_expr_kinds: [u32; COMPACT_EXPR_KIND_COUNT],
    pub top_level_binding_type_call_blockers: [u32; COMPACT_CALL_BLOCKER_KIND_COUNT],
    pub top_level_binding_type_call_callees: BTreeMap<String, u32>,
    pub top_level_binding_expression_expr_kinds: [u32; COMPACT_EXPR_KIND_COUNT],
    pub top_level_binding_expression_call_blockers: [u32; COMPACT_CALL_BLOCKER_KIND_COUNT],
    pub top_level_binding_expression_call_callees: BTreeMap<String, u32>,
    pub top_level_expression_expr_kinds: [u32; COMPACT_EXPR_KIND_COUNT],
    pub top_level_expression_call_blockers: [u32; COMPACT_CALL_BLOCKER_KIND_COUNT],
    pub top_level_expression_call_callees: BTreeMap<String, u32>,
    pub top_level_command_kinds: [u32; COMPACT_COMMAND_BLOCKER_KIND_COUNT],
    pub statements: usize,
    pub constructed_statements: usize,
    pub statement_blockers: [u32; COMPACT_STMT_KIND_COUNT],
    pub statement_blocker_sample_spans: BTreeMap<String, Vec<Span>>,
    pub expressions: usize,
    pub constructed_expressions: usize,
    pub expression_blockers: [u32; COMPACT_EXPR_KIND_COUNT],
    pub call_blockers: [u32; COMPACT_CALL_BLOCKER_KIND_COUNT],
    pub call_blocker_callees: BTreeMap<String, u32>,
    pub call_blocker_sample_spans: BTreeMap<String, Vec<Span>>,
    pub patterns: usize,
    pub constructed_patterns: usize,
    pub expr_type_facts: usize,
    /// Count of expressions/statements that could not be lowered and were
    /// replaced with a `Unit` placeholder during the permissive measurement
    /// traversal. A function whose body increments this is NOT fully lowerable
    /// and must not be committed (see `lower_function_with_blocker`), so the
    /// fixpoint can retry it once its dependencies lower, or fall back honestly.
    pub blocker_events: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FrontendLoweredStats {
    pub function_count: usize,
    pub constructed_functions: usize,
    pub statement_count: usize,
    pub expression_count: usize,
    pub pattern_count: usize,
    pub blocker_events: u64,
    pub retained_estimate_bytes: usize,
}

impl Default for CompactLowerConstructProbeOutput {
    fn default() -> Self {
        Self {
            functions: 0,
            constructed_functions: 0,
            constructed_auto_main_functions: 0,
            function_blockers: [0; COMPACT_FUNCTION_BLOCKER_KIND_COUNT],
            function_return_type_tags: [0; COMPACT_TYPE_EXPR_TAG_COUNT],
            function_param_type_tags: [0; COMPACT_TYPE_EXPR_TAG_COUNT],
            function_body_tail_stmt_kinds: [0; COMPACT_STMT_KIND_COUNT],
            function_body_tail_command_kinds: [0; COMPACT_COMMAND_BLOCKER_KIND_COUNT],
            function_body_tail_call_callees: BTreeMap::new(),
            top_level_statements: 0,
            constructed_top_level_statements: 0,
            top_level_blockers: [0; COMPACT_TOP_LEVEL_BLOCKER_KIND_COUNT],
            top_level_blocker_sample_spans: BTreeMap::new(),
            top_level_blocker_stmt_kinds: [0; COMPACT_STMT_KIND_COUNT],
            top_level_binding_type_annotation_tags: [0; COMPACT_TYPE_EXPR_TAG_COUNT],
            top_level_binding_type_expr_kinds: [0; COMPACT_EXPR_KIND_COUNT],
            top_level_binding_type_call_blockers: [0; COMPACT_CALL_BLOCKER_KIND_COUNT],
            top_level_binding_type_call_callees: BTreeMap::new(),
            top_level_binding_expression_expr_kinds: [0; COMPACT_EXPR_KIND_COUNT],
            top_level_binding_expression_call_blockers: [0; COMPACT_CALL_BLOCKER_KIND_COUNT],
            top_level_binding_expression_call_callees: BTreeMap::new(),
            top_level_expression_expr_kinds: [0; COMPACT_EXPR_KIND_COUNT],
            top_level_expression_call_blockers: [0; COMPACT_CALL_BLOCKER_KIND_COUNT],
            top_level_expression_call_callees: BTreeMap::new(),
            top_level_command_kinds: [0; COMPACT_COMMAND_BLOCKER_KIND_COUNT],
            statements: 0,
            constructed_statements: 0,
            statement_blockers: [0; COMPACT_STMT_KIND_COUNT],
            statement_blocker_sample_spans: BTreeMap::new(),
            expressions: 0,
            constructed_expressions: 0,
            expression_blockers: [0; COMPACT_EXPR_KIND_COUNT],
            call_blockers: [0; COMPACT_CALL_BLOCKER_KIND_COUNT],
            call_blocker_callees: BTreeMap::new(),
            call_blocker_sample_spans: BTreeMap::new(),
            patterns: 0,
            constructed_patterns: 0,
            expr_type_facts: 0,
            blocker_events: 0,
        }
    }
}

pub const COMPACT_TOP_LEVEL_BLOCKER_KIND_COUNT: usize = 11;
pub const COMPACT_FUNCTION_BLOCKER_KIND_COUNT: usize = 6;
pub const COMPACT_TYPE_EXPR_TAG_COUNT: usize = 8;
pub const COMPACT_STMT_KIND_COUNT: usize = 27;
pub const COMPACT_EXPR_KIND_COUNT: usize = 39;
pub const COMPACT_CALL_BLOCKER_KIND_COUNT: usize = 6;
pub const COMPACT_COMMAND_BLOCKER_KIND_COUNT: usize = 6;

#[derive(Clone, Debug, Default)]
pub struct CompactRuntimeDeclProbeOutput {
    pub type_defs: usize,
    pub tag_arities: usize,
    pub error_families: usize,
    pub error_variants: usize,
    pub error_fields: usize,
    pub error_facets: usize,
    pub procs: usize,
    pub pures: usize,
    pub streams: usize,
}

pub fn probe_compact_lower_constructed_bodies(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    bodies: &CompactBodyProbeOutput,
    source: &str,
) -> CompactLowerConstructProbeOutput {
    program.symbol_owner().with_current(|| {
        lower::probe_compact_lower_constructed_bodies(program, declarations, bodies, source)
    })
}

pub fn probe_compact_runtime_declarations(
    declarations: &CompactDeclOutput,
) -> CompactRuntimeDeclProbeOutput {
    let tag_arities = compact_runtime_tag_arities(declarations);
    let (error_families, error_variants, error_fields, error_facets) =
        compact_runtime_error_families(declarations);
    CompactRuntimeDeclProbeOutput {
        type_defs: declarations.types.len(),
        tag_arities: tag_arities.len(),
        error_families: error_families.len(),
        error_variants,
        error_fields,
        error_facets,
        procs: declarations.procs.len(),
        pures: declarations.pures.len(),
        streams: declarations.streams.len(),
    }
}

fn compact_runtime_tag_arities(declarations: &CompactDeclOutput) -> FxHashMap<Name, usize> {
    declarations
        .tag_variants_by_name
        .iter()
        .map(|(name, variant)| (*name, variant.field_count))
        .collect()
}

fn compact_runtime_error_families(
    declarations: &CompactDeclOutput,
) -> (FxHashMap<Name, RuntimeErrorFamily>, usize, usize, usize) {
    let mut error_variants = 0usize;
    let mut error_fields = 0usize;
    let mut error_facets = 0usize;
    let error_families = declarations
        .error_families_by_name
        .iter()
        .map(|(name, family)| {
            for variant in family.variants.values() {
                error_variants += 1;
                error_fields += variant.fields.len();
                error_facets += variant.facets.len();
            }
            (*name, RuntimeErrorFamily {})
        })
        .collect();
    (error_families, error_variants, error_fields, error_facets)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvalFlow {
    Value(Value),
    Return(Value),
    Propagate(Propagation),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Propagation {
    pub error: Value,
    pub traceback: Traceback,
}

#[derive(Clone, Debug)]
struct Binding {
    value: Value,
    mutable: bool,
}

#[derive(Clone, Debug)]
pub(super) struct RuntimeErrorFamily {}

#[derive(Clone, Debug)]
struct RegisteredSignalHook {
    signal: HookSignal,
    pre_cancel: DurationValue,
    indexed_body: RegisteredIndexedSignalBody,
    lowered_slots: Vec<LoweredTopLevelSlot>,
    scope: FxHashMap<Name, Binding>,
    span: Span,
    ignore_pending_primary: bool,
}

#[derive(Clone, Debug)]
struct RegisteredIndexedSignalBody {
    program: Arc<indexed::full::FullProgram>,
    driver_step: usize,
    body: u32,
    slot_count: usize,
}

#[derive(Clone)]
struct DynamicFunction {
    program: Arc<FullProgram>,
    function: LoweredFunctionKey,
    kind: LoweredFunctionKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveProcessGroup {
    group: ProcessGroup,
    hook_owned: bool,
}

struct LiveProcessHandle {
    owner_scope: u64,
    child: ManagedChild,
    span: Span,
}

#[derive(Clone, Debug, Default)]
struct EvaluatorSignalState {
    received_traced: bool,
    hook_started: bool,
    hook_running: bool,
    primary_forwarded: bool,
    escalated_traced: bool,
    shutdown_status: Option<u8>,
    shutdown_force: bool,
    shutdown_complete: bool,
    pre_cancel_deadline: Option<Instant>,
    hook_span: Option<Span>,
}

#[cfg(feature = "native-tests")]
#[derive(Clone, Debug)]
pub(super) struct TestMock {
    pub matcher: RecordMap,
    pub result: Value,
    pub remaining: i64,
}

#[cfg(feature = "native-tests")]
#[derive(Clone, Debug)]
pub(super) struct TestCall {
    pub op: String,
    pub args: RecordMap,
}

/// Upper bound on recycled scope maps held in `scope_pool` (deep recursion
/// shouldn't let the pool grow without bound).

macro_rules! build_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[repr(transparent)]
        struct $name(u32);

        impl $name {
            fn new(index: usize) -> Self {
                Self(u32::try_from(index).expect("construction scratch exceeds u32"))
            }

            fn index(self) -> usize {
                self.0 as usize
            }
        }
    };
}

build_id!(BuildExprId);
build_id!(BuildStmtId);
build_id!(BuildPatternId);
build_id!(BuildIntId);
build_id!(BuildBoolId);
build_id!(BuildTopStmtId);

#[derive(Clone, Debug, Default)]
struct BuildScratch {
    expressions: Vec<BuildExprRow>,
    statements: Vec<BuildStmtRow>,
    patterns: Vec<BuildPatternRow>,
    ints: Vec<BuildIntRow>,
    bools: Vec<BuildBoolRow>,
    top_statements: Vec<BuildTopStmtRow>,
}

impl BuildScratch {
    fn expr(&mut self, row: BuildExprRow) -> BuildExprId {
        let id = BuildExprId::new(self.expressions.len());
        self.expressions.push(row);
        id
    }

    fn stmt(&mut self, row: BuildStmtRow) -> BuildStmtId {
        let id = BuildStmtId::new(self.statements.len());
        self.statements.push(row);
        id
    }

    fn pattern(&mut self, row: BuildPatternRow) -> BuildPatternId {
        let id = BuildPatternId::new(self.patterns.len());
        self.patterns.push(row);
        id
    }

    fn int(&mut self, row: BuildIntRow) -> BuildIntId {
        let id = BuildIntId::new(self.ints.len());
        self.ints.push(row);
        id
    }

    fn bool(&mut self, row: BuildBoolRow) -> BuildBoolId {
        let id = BuildBoolId::new(self.bools.len());
        self.bools.push(row);
        id
    }

    fn top_stmt(&mut self, row: BuildTopStmtRow) -> BuildTopStmtId {
        let id = BuildTopStmtId::new(self.top_statements.len());
        self.top_statements.push(row);
        id
    }
}

#[derive(Clone, Debug)]
struct FunctionBuild {
    params: LoweredParamNames,
    param_kinds: LoweredParamKinds,
    param_checks: LoweredParamChecks,
    param_rest: LoweredParamRest,
    param_defaults: LoweredParamDefaults,
    captures: LoweredTopLevelSlots,
    return_kind: LoweredReturnKind,
    slot_count: usize,
    body: Vec<BuildStmtId>,
    has_defers: bool,
    scratch: Rc<RefCell<BuildScratch>>,
}

#[derive(Clone, Debug)]
struct FunctionHeader {
    params: LoweredParamNames,
    param_kinds: LoweredParamKinds,
    param_checks: LoweredParamChecks,
    param_rest: LoweredParamRest,
    param_defaults: LoweredParamDefaults,
    captures: LoweredTopLevelSlots,
    return_kind: LoweredReturnKind,
    slot_count: usize,
}

#[derive(Clone, Debug, Default)]
struct ProgramBuild {
    statements: Vec<Option<BuildTopStmtId>>,
    scratch: Rc<RefCell<BuildScratch>>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LoweredFunctionKey {
    Name(Name),
    Qualified(QualifiedName),
}

impl LoweredFunctionKey {
    pub fn display_name(self) -> String {
        match self {
            Self::Name(name) => name.to_string(),
            Self::Qualified(name) => name.to_string(),
        }
    }

    pub fn is_qualified(self) -> bool {
        matches!(self, Self::Qualified(_))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LoweredFunctionKind {
    Pure,
    Proc,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoweredFunctionBlocker {
    ReturnType,
    ParamDefault,
    ParamType,
    BlockParams,
    Body,
    NoReturn,
}

impl LoweredFunctionBlocker {
    pub fn index(self) -> usize {
        match self {
            Self::ReturnType => 0,
            Self::ParamDefault => 1,
            Self::ParamType => 2,
            Self::BlockParams => 3,
            Self::Body => 4,
            Self::NoReturn => 5,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ReturnType => "return_type",
            Self::ParamDefault => "param_default",
            Self::ParamType => "param_type",
            Self::BlockParams => "block_params",
            Self::Body => "body",
            Self::NoReturn => "no_return",
        }
    }
}

#[derive(Clone, Debug)]
pub struct LoweredFunctionUnit {
    key: LoweredFunctionKey,
    kind: LoweredFunctionKind,
    source_span: Span,
    owner: Option<Name>,
    param_count: usize,
    capture_count: usize,
    slot_count: usize,
    dependency_edges: Vec<LoweredFunctionKey>,
    body: Option<FunctionBuild>,
    blocker: Option<LoweredFunctionBlocker>,
    blocker_detail: Option<(Span, String)>,
    scc_member_count: usize,
    scc_group: Option<usize>,
}

impl LoweredFunctionUnit {
    pub fn key(&self) -> LoweredFunctionKey {
        self.key
    }

    pub fn kind(&self) -> LoweredFunctionKind {
        self.kind
    }

    pub fn source_span(&self) -> Span {
        self.source_span
    }

    pub fn owner(&self) -> Option<Name> {
        self.owner
    }

    pub fn param_count(&self) -> usize {
        self.param_count
    }

    pub fn capture_count(&self) -> usize {
        self.capture_count
    }

    pub fn slot_count(&self) -> usize {
        self.slot_count
    }

    pub fn dependency_edges(&self) -> &[LoweredFunctionKey] {
        &self.dependency_edges
    }

    pub fn is_lowered(&self) -> bool {
        self.body.is_some()
    }

    pub fn blocker(&self) -> Option<LoweredFunctionBlocker> {
        self.blocker
    }

    pub fn blocker_detail(&self) -> Option<&(Span, String)> {
        self.blocker_detail.as_ref()
    }

    pub fn scc_member_count(&self) -> usize {
        self.scc_member_count
    }

    pub fn scc_group(&self) -> Option<usize> {
        self.scc_group
    }

    pub fn is_scc_member(&self) -> bool {
        self.scc_member_count > 1
    }

    fn lowered_body(&self) -> Option<FunctionBuild> {
        self.body.clone()
    }

    fn take_lowered_body(&mut self) -> Option<FunctionBuild> {
        self.body.take()
    }
}

struct LowerableFunctions<'a> {
    pures: Option<&'a FxHashSet<Name>>,
    procs: Option<&'a FxHashSet<Name>>,
    qualified_pures: Option<&'a FxHashSet<QualifiedName>>,
    qualified_procs: Option<&'a FxHashSet<QualifiedName>>,
    // In-flight candidates not yet committed to the lowered sets. A single key
    // for self-recursion, or a whole strongly-connected component for
    // mutually-recursive co-lowering. Membership only — function-body call
    // lowering needs `contains`, not the callee's return kind (which is resolved
    // from the indexed function headers).
    candidates: &'a [LoweredFunctionKey],
}

impl<'a> LowerableFunctions<'a> {
    fn all_with_candidates(
        pures: &'a FxHashSet<Name>,
        procs: &'a FxHashSet<Name>,
        qualified_pures: &'a FxHashSet<QualifiedName>,
        qualified_procs: &'a FxHashSet<QualifiedName>,
        candidates: &'a [LoweredFunctionKey],
    ) -> Self {
        Self {
            pures: Some(pures),
            procs: Some(procs),
            qualified_pures: Some(qualified_pures),
            qualified_procs: Some(qualified_procs),
            candidates,
        }
    }

    fn all(
        pures: &'a FxHashSet<Name>,
        procs: &'a FxHashSet<Name>,
        qualified_pures: &'a FxHashSet<QualifiedName>,
        qualified_procs: &'a FxHashSet<QualifiedName>,
    ) -> Self {
        Self {
            pures: Some(pures),
            procs: Some(procs),
            qualified_pures: Some(qualified_pures),
            qualified_procs: Some(qualified_procs),
            candidates: &[],
        }
    }

    fn contains(&self, key: LoweredFunctionKey) -> bool {
        self.candidates.contains(&key)
            || match key {
                LoweredFunctionKey::Name(name) => {
                    self.pures.is_some_and(|pures| pures.contains(&name))
                        || self.procs.is_some_and(|procs| procs.contains(&name))
                }
                LoweredFunctionKey::Qualified(name) => {
                    self.qualified_pures
                        .is_some_and(|pures| pures.contains(&name))
                        || self
                            .qualified_procs
                            .is_some_and(|procs| procs.contains(&name))
                }
            }
    }

    fn pure_contains(&self, key: LoweredFunctionKey) -> bool {
        self.candidates.contains(&key)
            || match key {
                LoweredFunctionKey::Name(name) => {
                    self.pures.is_some_and(|pures| pures.contains(&name))
                }
                LoweredFunctionKey::Qualified(name) => self
                    .qualified_pures
                    .is_some_and(|pures| pures.contains(&name)),
            }
    }
}

#[derive(Clone, Debug)]
struct BuildTopStmtRow {
    kind: BuildTopKind,
    slots: LoweredTopLevelSlots,
    slot_count: usize,
}

#[derive(Clone, Debug)]
enum BuildTopKind {
    Use {
        key: Arc<str>,
        alias: Option<Name>,
        path: Vec<Name>,
        namespace: Name,
        exports: Vec<LoweredModuleExport>,
        module_statements: Vec<(Span, BuildTopStmtId)>,
        span: Span,
    },
    Let {
        target: Name,
        ty: Option<LoweredType>,
        validation: Option<LoweredTypeCheck>,
        mutable: bool,
        value: BuildExprId,
        value_span: Span,
    },
    // `let {a, b, ..} = source` / `var {…}` at top level: define one named
    // binding per field (field name == binding name) from the source record.
    LetRecord {
        source: BuildExprId,
        fields: Vec<Name>,
        mutable: bool,
        span: Span,
    },
    Assign {
        target: Name,
        op: AssignOp,
        value: BuildExprId,
        span: Span,
    },
    Discard {
        value: BuildExprId,
        span: Span,
    },
    Stmt(BuildStmtId),
    Expr(BuildExprId),
    Defer {
        value: BuildExprId,
        span: Span,
    },
    SignalHook {
        signal: Name,
        pre_cancel: Option<String>,
        body: Vec<BuildStmtId>,
        slots: Vec<LoweredTopLevelSlot>,
        slot_count: usize,
        span: Span,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LoweredModuleExportKind {
    Value,
    Pure,
    Proc,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LoweredModuleExport {
    name: Name,
    kind: LoweredModuleExportKind,
    function_namespace: Option<Name>,
}

#[derive(Clone, Debug)]
struct LoweredTopLevelSlot {
    name: Name,
    slot: usize,
    kind: LoweredType,
    mutable: bool,
}

#[derive(Clone, Debug)]
struct LoweredTopLevelBinding {
    kind: LoweredType,
    result_ok: Option<LoweredType>,
    checked: Option<Type>,
    mutable: bool,
    slot: bool,
}

#[derive(Clone, Debug)]
struct LoweredTypeCheck {
    ty: Type,
    name: Arc<str>,
}

#[derive(Clone, Copy, Debug)]
enum LoweredReturnKind {
    Plain(LoweredType),
    Result(LoweredType),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LoweredType {
    Any,
    Unit,
    Int,
    Float,
    Duration,
    Bool,
    Str,
    Bytes,
    Digest,
    Regex,
    Status,
    Path,
    Command,
    ProcessHandle,
    Stream,
    Pure,
    Proc,
    Error,
    Record,
    Module,
    List,
    Map,
    Tag,
    Result,
}

type LoweredParamNames = SmallVec<[Name; 4]>;
type LoweredParamKinds = SmallVec<[LoweredType; 4]>;
type LoweredParamChecks = SmallVec<[Option<LoweredTypeCheck>; 4]>;
type LoweredParamRest = SmallVec<[bool; 4]>;
type LoweredParamDefaults = SmallVec<[Option<LoweredValue>; 4]>;
type LoweredTopLevelSlots = SmallVec<[LoweredTopLevelSlot; 4]>;
type BuildPatternIdSlots = SmallVec<[Option<usize>; 2]>;
type LoweredCompFields = SmallVec<[(Name, usize, Span); 4]>;
type LoweredErrorPatternFields = SmallVec<[(Name, Option<usize>); 4]>;

#[derive(Clone, Debug)]
enum LoweredCompTarget {
    Slot(usize),
    Record { fields: LoweredCompFields },
}

#[derive(Clone, Debug)]
enum LoweredRecordEntry {
    Field(Name, BuildExprId),
    Spread(BuildExprId),
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
enum LoweredProcessCommandBuilderEntry {
    Field {
        name: Name,
        value: BuildExprId,
        span: Span,
    },
    Run {
        target: LoweredRunArg,
        args: Vec<LoweredRunArg>,
        env: Vec<LoweredRunEnv>,
        timeout: Option<BuildExprId>,
        cpu_max: Option<BuildExprId>,
        span: Span,
    },
}

#[derive(Clone, Debug)]
struct LoweredProcessCommandArgv {
    target: BuildExprId,
    argv: BuildExprId,
    cwd: Option<BuildExprId>,
    env: Option<BuildExprId>,
    stdin: Option<BuildExprId>,
    stdout: Option<BuildExprId>,
    stderr: Option<BuildExprId>,
    stdout_append: Option<BuildExprId>,
    stderr_append: Option<BuildExprId>,
    timeout: Option<BuildExprId>,
    detach: Option<BuildExprId>,
    new_session: Option<BuildExprId>,
    ignore_hup: Option<BuildExprId>,
    cpu_max: Option<BuildExprId>,
    span: Span,
}

#[derive(Clone, Debug)]
struct LoweredRunCapture {
    kind: RunKind,
    target: Box<LoweredRunArg>,
    args: Vec<LoweredRunArg>,
    env: Vec<LoweredRunEnv>,
    redirections: Vec<LoweredRunRedirection>,
    timeout: Option<BuildExprId>,
    cpu_max: Option<BuildExprId>,
    // For Plain/Status run *values* with `?`, propagation is handled inside
    // eval_lowered_run_capture (Break on RunError, pass Status through),
    // because a Plain run yields a bare Status on success — not a Result the
    // external `Try` wrapper could unwrap. Capture kinds keep the external
    // Try and leave this false.
    propagate: bool,
    assert_success: bool,
    span: Span,
}

#[derive(Clone, Debug)]
struct LoweredSpawnRun {
    target: Box<LoweredRunArg>,
    args: Vec<LoweredRunArg>,
    env: Vec<LoweredRunEnv>,
    redirections: Vec<LoweredRunRedirection>,
    timeout: Option<BuildExprId>,
    cpu_max: Option<BuildExprId>,
    span: Span,
}

#[derive(Clone, Debug)]
enum BuildStmtRow {
    Let {
        slot: usize,
        value: BuildExprId,
    },
    /// `guard let slot = value else |else_param| { else_body }`: evaluate
    /// `value` (a `Result`); on `Ok`, bind its inner value to `slot` and
    /// continue; on `Err`, bind the error to `else_param_slot` (if present) and
    /// run `else_body`, which must diverge.
    Guard {
        slot: usize,
        value: BuildExprId,
        else_param_slot: Option<usize>,
        else_body: Vec<BuildStmtId>,
        span: Span,
    },
    LetInt {
        slot: usize,
        value: BuildIntId,
    },
    LetBool {
        slot: usize,
        value: BuildBoolId,
    },
    Assign {
        slot: usize,
        op: AssignOp,
        value: BuildExprId,
        span: Span,
    },
    AssignField {
        slot: usize,
        field: Arc<str>,
        op: AssignOp,
        value: BuildExprId,
        span: Span,
    },
    AssignFieldInt {
        slot: usize,
        field: Arc<str>,
        op: AssignOp,
        value: BuildIntId,
        span: Span,
    },
    AssignIndex {
        slot: usize,
        // Boxed: this is the only `BuildStmtId` variant with two inline
        // `BuildExprId`s, which made it (at ~2x the enum's other variants) the
        // size driver for every statement in the lowered IR.
        index: BuildExprId,
        op: AssignOp,
        value: BuildExprId,
        span: Span,
    },
    AssignInt {
        slot: usize,
        op: AssignOp,
        value: BuildIntId,
        span: Span,
    },
    AssignBool {
        slot: usize,
        value: BuildBoolId,
    },
    Expr {
        value: BuildExprId,
        span: Span,
    },
    If {
        branches: Vec<(BuildExprId, Vec<BuildStmtId>)>,
        else_body: Option<Vec<BuildStmtId>>,
    },
    IfBool {
        branches: Vec<(BuildBoolId, Vec<BuildStmtId>)>,
        else_body: Option<Vec<BuildStmtId>>,
    },
    While {
        condition: BuildExprId,
        body: Vec<BuildStmtId>,
    },
    WhileBool {
        condition: BuildBoolId,
        body: Vec<BuildStmtId>,
    },
    Match {
        value: BuildExprId,
        arms: Vec<(BuildPatternId, Option<BuildExprId>, Vec<BuildStmtId>)>,
        span: Span,
    },
    StrMatch {
        value: BuildExprId,
        arms: FxHashMap<Arc<str>, Vec<BuildStmtId>>,
        fallback: Option<Vec<BuildStmtId>>,
        span: Span,
    },
    TagMatch {
        value: BuildExprId,
        arms: FxHashMap<Arc<str>, Vec<BuildStmtId>>,
        fallback: Option<Vec<BuildStmtId>>,
        span: Span,
    },
    For {
        slot: usize,
        iter: BuildExprId,
        body: Vec<BuildStmtId>,
        span: Span,
    },
    // `let {a, b, ..} = source` / `var {…} = source`: destructure a record into
    // one slot per field (field name == binding name).
    LetRecord {
        source: BuildExprId,
        fields: Vec<(Name, usize)>,
        span: Span,
    },
    // `for {a, b, ..} in iter { … }`: per item (a record), bind each field slot.
    ForRecord {
        fields: Vec<(Name, usize)>,
        iter: BuildExprId,
        body: Vec<BuildStmtId>,
        span: Span,
    },
    ForStrLines {
        slot: usize,
        text: BuildExprId,
        body: Vec<BuildStmtId>,
        span: Span,
    },
    ScanLines {
        text_slot: usize,
        line_slot: usize,
        checks: Vec<ScanCheck>,
        span: Span,
    },
    ScanBytes {
        config: ScanBytes,
    },
    Print {
        args: Vec<BuildExprId>,
        stderr: bool,
        flush: bool,
        propagate_result: bool,
        span: Span,
    },
    Cd {
        target: BuildExprId,
        body: Vec<BuildStmtId>,
        span: Span,
    },
    Env {
        env: Vec<LoweredRunEnv>,
        body: Vec<BuildStmtId>,
    },
    Proc {
        // The registry identity is stable; resolve it once while lowering
        // instead of retaining two names and looking it up on every execution.
        op: RuntimeOp,
        args: Vec<BuildExprId>,
        propagate_result: bool,
        span: Span,
    },
    Run {
        value: BuildExprId,
        propagate_result: bool,
    },
    Loop {
        body: Vec<BuildStmtId>,
    },
    Return {
        value: BuildExprId,
    },
    Yield {
        value: BuildExprId,
    },
    Break,
    BreakValue {
        value: BuildExprId,
    },
    Continue,
    Defer {
        value: BuildExprId,
    },
}

#[derive(Clone, Debug)]
enum BuildIntRow {
    Int(i64),
    Slot(usize),
    Binary {
        op: BinaryOp,
        left: BuildIntId,
        right: BuildIntId,
    },
    StrByteLenSlot {
        slot: usize,
        span: Span,
    },
    StrCountLinesSlot {
        slot: usize,
        span: Span,
    },
    StrByteAtSlot {
        slot: usize,
        index: BuildIntId,
        default: Option<BuildIntId>,
        span: Span,
    },
}

#[derive(Clone, Debug)]
enum BuildBoolRow {
    Bool(bool),
    Slot(usize),
    Not(BuildBoolId),
    And(BuildBoolId, BuildBoolId),
    Or(BuildBoolId, BuildBoolId),
    IntCompare {
        op: BinaryOp,
        left: BuildIntId,
        right: BuildIntId,
    },
    StrPredicateSlot {
        slot: usize,
        predicate: LoweredStrPredicate,
        needle: Arc<[u8]>,
        span: Span,
    },
    ContainsSlot {
        slot: usize,
        needle: LoweredValue,
        span: Span,
    },
    StrContainsSlot {
        slot: usize,
        needle: Arc<str>,
        span: Span,
    },
    TrimEmptySlot {
        slot: usize,
        span: Span,
    },
    TrimStrPredicateSlot {
        slot: usize,
        predicate: LoweredStrPredicate,
        needle: Arc<[u8]>,
        span: Span,
    },
    LiteralCompareSlot {
        op: BinaryOp,
        slot: usize,
        value: LoweredValue,
    },
}

enum StmtFlow {
    None,
    Return(LoweredValue),
    Propagate(LoweredValue),
    Break(Option<LoweredValue>),
    Continue,
}

#[derive(Clone, Debug)]
enum BuildExprRow {
    Null,
    Unit,
    Int(i64),
    Float(FloatValue),
    Duration(DurationValue),
    Bool(bool),
    Str(Arc<str>),
    Bytes(Arc<[u8]>),
    Path(PathValue),
    FunctionRef {
        function: FunctionName,
        pure: bool,
    },
    PathFrom {
        value: BuildExprId,
        span: Span,
    },
    Param(usize),
    Binary {
        op: BinaryOp,
        left: BuildExprId,
        right: BuildExprId,
        span: Span,
    },
    IfExpr {
        branches: Vec<(BuildExprId, BuildExprId)>,
        else_value: BuildExprId,
        span: Span,
    },
    MatchExpr {
        value: BuildExprId,
        arms: Vec<(BuildPatternId, Option<BuildExprId>, BuildExprId)>,
        span: Span,
    },
    StrMatchExpr {
        value: BuildExprId,
        arms: FxHashMap<Arc<str>, BuildExprId>,
        fallback: Option<BuildExprId>,
        span: Span,
    },
    TagMatchExpr {
        value: BuildExprId,
        arms: FxHashMap<Arc<str>, BuildExprId>,
        fallback: Option<BuildExprId>,
        span: Span,
    },
    ResultFallback {
        left: BuildExprId,
        right: BuildExprId,
    },
    FmtString(Vec<LoweredFmtPart>),
    PathFmtString {
        parts: Vec<LoweredFmtPart>,
        span: Span,
    },
    // A `g"..."` glob literal. Expanded against the current cwd at eval time into
    // a `List[Path]` (mirrors the recursive evaluator's `eval_glob`).
    Glob {
        pattern: Arc<str>,
        span: Span,
    },
    // The `$?` last-process-status expression. Reads the evaluator's tracked
    // `last_status` at eval time, erroring if no process has run yet.
    LastStatus {
        span: Span,
    },
    Record(Vec<LoweredRecordEntry>),
    List(Vec<BuildExprId>),
    // The `map.empty()` builtin constructor (empty list literals already lower via `List`).
    EmptyMap,
    // The `bytes.concat(<List[Bytes]>)` builtin constructor.
    BytesConcat {
        arg: BuildExprId,
        span: Span,
    },
    Range {
        start: BuildExprId,
        end: BuildExprId,
        span: Span,
    },
    Tag {
        name: Arc<str>,
        fields: Vec<BuildExprId>,
    },
    ListComp {
        value: BuildExprId,
        // Boxed because `LoweredCompTarget::Record` inlines a 4-element
        // `SmallVec` (~176 bytes) that would otherwise size every `BuildExprId`
        // variant, not just the rare destructuring-comprehension case.
        target: Box<LoweredCompTarget>,
        iter: BuildExprId,
        condition: Option<BuildExprId>,
        span: Span,
    },
    MapComp {
        key: BuildExprId,
        value: BuildExprId,
        target: Box<LoweredCompTarget>,
        iter: BuildExprId,
        condition: Option<BuildExprId>,
        span: Span,
    },
    ListPipeline {
        input: BuildExprId,
        stages: Vec<LoweredPipelineStage>,
        span: Span,
    },
    Field {
        base: BuildExprId,
        // Scratch rows preserve generated static text without allocation while
        // retaining an owned spelling for dynamic names.
        name: NameText,
        span: Span,
    },
    Index {
        base: BuildExprId,
        index: BuildExprId,
        span: Span,
    },
    Slice {
        base: BuildExprId,
        start: Option<BuildExprId>,
        end: Option<BuildExprId>,
        span: Span,
    },
    Method {
        receiver: BuildExprId,
        name: NameText,
        args: Vec<BuildExprId>,
        span: Span,
    },
    StrByteLen {
        receiver: BuildExprId,
        span: Span,
    },
    StrByteAt {
        receiver: BuildExprId,
        index: BuildExprId,
        default: Option<BuildExprId>,
        span: Span,
    },
    StrPredicate {
        receiver: BuildExprId,
        predicate: LoweredStrPredicate,
        needle: BuildExprId,
        span: Span,
    },
    Contains {
        receiver: BuildExprId,
        needle: BuildExprId,
        span: Span,
    },
    RegexCompile {
        pattern: BuildExprId,
        span: Span,
    },
    Require {
        value: BuildExprId,
        check: LoweredTypeCheck,
        span: Span,
    },
    // Boxed because captured and spawned runs are cold, unusually wide
    // payloads that would otherwise determine the size of every expression.
    RunCapture(Box<LoweredRunCapture>),
    RunPipeline {
        segments: Vec<LoweredRunPipelineSegment>,
        propagate: bool,
        span: Span,
    },
    SpawnRun(Box<LoweredSpawnRun>),
    SpawnCommand {
        command: BuildExprId,
        span: Span,
    },
    Wait {
        target: BuildExprId,
        span: Span,
    },
    Loop {
        body: Vec<BuildStmtId>,
        span: Span,
    },
    Retry {
        delays: Vec<BuildExprId>,
        body: Vec<BuildStmtId>,
        span: Span,
    },
    FsFiles {
        root: BuildExprId,
        gitignore: bool,
        stat: bool,
        hidden: bool,
        exts: Option<BuildExprId>,
        result_wrapped: bool,
        span: Span,
    },
    FsWalk {
        root: BuildExprId,
        gitignore: bool,
        stat: bool,
        hidden: bool,
        exts: Option<BuildExprId>,
        result_wrapped: bool,
        span: Span,
    },
    FsList {
        op: RuntimeOp,
        path: BuildExprId,
        stat: Option<BuildExprId>,
        ordered: Option<BuildExprId>,
        span: Span,
    },
    FsTempDir {
        span: Span,
    },
    FsWrite {
        path: BuildExprId,
        data: BuildExprId,
        span: Span,
    },
    FsMkdir {
        path: BuildExprId,
        parents: Option<BuildExprId>,
        span: Span,
    },
    FsRemove {
        path: BuildExprId,
        missing_ok: Option<BuildExprId>,
        span: Span,
    },
    FsCloseRoot {
        root: BuildExprId,
        span: Span,
    },
    FsRootPath {
        root: BuildExprId,
        span: Span,
    },
    PathReadText {
        path: BuildExprId,
        span: Span,
    },
    PathReadBytes {
        path: BuildExprId,
        span: Span,
    },
    PathExists {
        path: BuildExprId,
        span: Span,
    },
    PathExecutable {
        path: BuildExprId,
        span: Span,
    },
    PathDu {
        path: BuildExprId,
        span: Span,
    },
    PathMetadata {
        path: BuildExprId,
        span: Span,
    },
    PathReadlink {
        path: BuildExprId,
        span: Span,
    },
    PathResolve {
        path: BuildExprId,
        span: Span,
    },
    PathWrite {
        path: BuildExprId,
        data: BuildExprId,
        atomic: bool,
        span: Span,
    },
    PathMkdir {
        path: BuildExprId,
        parents: Option<BuildExprId>,
        span: Span,
    },
    PathRemove {
        path: BuildExprId,
        missing_ok: Option<BuildExprId>,
        span: Span,
    },
    JsonEncode {
        value: BuildExprId,
        span: Span,
    },
    ArchiveTarCreate {
        path: BuildExprId,
        root: BuildExprId,
        entries: BuildExprId,
        compression: Option<BuildExprId>,
        overwrite: Option<BuildExprId>,
        span: Span,
    },
    ArchiveTarList {
        path: BuildExprId,
        span: Span,
    },
    ArchiveTarExtract {
        path: BuildExprId,
        dest: BuildExprId,
        span: Span,
    },
    HashVerifyFile {
        path: BuildExprId,
        algorithm: crate::modules::hash::HashAlgorithm,
        expected: BuildExprId,
        span: Span,
    },
    ModuleCall {
        op: RuntimeOp,
        args: Vec<BuildExprId>,
        span: Span,
    },
    // Boxed because this cold command-construction payload otherwise makes
    // every `BuildExprId` 16 bytes larger, including expression-heavy scripts.
    ProcessCommandArgv(Box<LoweredProcessCommandArgv>),
    ProcessCommandBuilder {
        entries: Vec<LoweredProcessCommandBuilderEntry>,
        span: Span,
    },
    Abort {
        status: BuildExprId,
        force: Option<BuildExprId>,
        span: Span,
    },
    Ok(BuildExprId),
    Err(BuildExprId),
    // Boxed: `LoweredErrorExpr::Structured` inlines two `String`s plus two
    // `Vec`s (~96 bytes) that would otherwise size every `BuildExprId` variant
    // for the sake of the comparatively rare structured-error-literal case.
    Error(Box<LoweredErrorExpr>),
    Try(BuildExprId),
    Call {
        function: LoweredFunctionKey,
        args: Vec<LoweredCallArg>,
        span: Span,
    },
    DirectPureCall {
        function: LoweredFunctionKey,
        args: Vec<LoweredCallArg>,
        span: Span,
    },
    DynamicCall {
        callee: BuildExprId,
        args: Vec<LoweredCallArg>,
        span: Span,
    },
    SelfCall {
        args: Vec<LoweredCallArg>,
        span: Span,
    },
}

#[derive(Clone, Debug)]
enum LoweredCallArg {
    Single(BuildExprId),
    Splice(BuildExprId),
}

#[derive(Clone, Debug)]
struct LoweredRunArg {
    kind: LoweredRunArgKind,
    span: Span,
}

#[derive(Clone, Debug)]
enum LoweredRunArgKind {
    Single(BuildExprId),
    SingleOrSplice(BuildExprId),
    Splice(BuildExprId),
}

#[derive(Clone, Debug)]
struct LoweredRunPipelineSegment {
    #[allow(dead_code)]
    kind: RunKind,
    target: LoweredRunArg,
    args: Vec<LoweredRunArg>,
    env: Vec<LoweredRunEnv>,
    redirections: Vec<LoweredRunRedirection>,
    timeout: Option<BuildExprId>,
    cpu_max: Option<BuildExprId>,
}

#[derive(Clone, Debug)]
struct LoweredRunEnv {
    name: Name,
    value: LoweredRunArg,
}

#[derive(Clone, Debug)]
struct LoweredRunRedirection {
    kind: RedirectionKind,
    target: LoweredRunArg,
    span: Span,
}

#[derive(Clone, Debug)]
enum LoweredFmtPart {
    Text(Arc<str>),
    Expr(BuildExprId, Span, Option<FormatSpec>),
}

#[derive(Clone, Debug)]
enum BuildPatternRow {
    Wildcard,
    // `name => …`: always matches, binds the scrutinee to `slot`.
    Bind {
        slot: usize,
    },
    // `name is T => …` / `_ is T => …`: matches when the value's runtime type
    // satisfies `ty`, binding `slot` when present.
    Type {
        ty: crate::sema::types::Type,
        slot: Option<usize>,
    },
    Literal(LoweredValue),
    ResultOk {
        slot: Option<usize>,
        unit_only: bool,
    },
    ResultErr {
        slot: Option<usize>,
        unit_only: bool,
    },
    ErrorVariant {
        family: Name,
        variant: Name,
        // Structured error patterns are rare; keep their multi-field payload
        // from setting the size of every pattern in every match arm.
        fields: Box<LoweredErrorPatternFields>,
        result_wrapped: bool,
    },
    // `is Facet => …`: matches when the error value carries `facet`.
    // `result_wrapped` distinguishes `Err(is Facet)` (scrutinee is a Result)
    // from a standalone `is Facet` (scrutinee is the error value itself).
    Facet {
        facet: Name,
        result_wrapped: bool,
    },
    Tag {
        name: Name,
        slots: BuildPatternIdSlots,
    },
}

/// The aggregation a `reduce-by --sum|--min|--max` applies to the per-key
/// `value` produced by its block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReduceByOp {
    Sum,
    Min,
    Max,
}

#[derive(Clone, Debug)]
enum LoweredPipelineStage {
    TextLines,
    JsonLines,
    Where {
        slot: usize,
        predicate: BuildExprId,
    },
    Map {
        slot: usize,
        value: BuildExprId,
    },
    MapBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    },
    FlatMap {
        slot: usize,
        value: BuildExprId,
    },
    FlatMapBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    },
    BytesChunks {
        size: BuildExprId,
    },
    BatchCount {
        count: BuildExprId,
    },
    BatchMaxArgv {
        max_argv: Option<BuildExprId>,
    },
    BatchMaxBytes {
        max_bytes: BuildExprId,
    },
    Shuffle {
        seed: Option<BuildExprId>,
    },
    Fold {
        acc_slot: usize,
        item_slot: usize,
        initial: BuildExprId,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    },
    ReduceBy {
        item_slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
        op: ReduceByOp,
    },
    ParMap {
        slot: usize,
        jobs: Option<BuildExprId>,
        value: BuildExprId,
    },
    ParMapBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        jobs: Option<BuildExprId>,
        value: BuildExprId,
    },
    ParMapFlatMapReduceBy {
        slot: usize,
        body: Option<Vec<BuildStmtId>>,
        jobs: Option<BuildExprId>,
        value: BuildExprId,
        flatten: bool,
        reduce_item_slot: usize,
        reduce_body: Vec<BuildStmtId>,
        reduce_value: BuildExprId,
        op: ReduceByOp,
    },
    Tee {
        slot: usize,
        body: Vec<BuildStmtId>,
    },
    Each {
        slot: usize,
        body: Vec<BuildStmtId>,
        parallel: bool,
    },
    TablePrint {
        columns: Option<Vec<String>>,
    },
    Enumerate,
    Zip {
        other: BuildExprId,
    },
    Sort {
        descending: Option<BuildExprId>,
    },
    SortBy {
        slot: usize,
        key: BuildExprId,
        descending: Option<BuildExprId>,
    },
    GroupBy {
        slot: usize,
        key: BuildExprId,
    },
    CountBy {
        slot: usize,
        key: BuildExprId,
    },
    Any {
        slot: usize,
        predicate: BuildExprId,
    },
    All {
        slot: usize,
        predicate: BuildExprId,
    },
    UniqueBy {
        slot: usize,
        key: BuildExprId,
    },
    Count,
    Sum,
    Collect,
    First,
    Last,
    Min,
    Max,
    Take(BuildExprId),
    Drop(BuildExprId),
    Repeat {
        count: BuildExprId,
    },
    Range {
        start: BuildExprId,
        end: BuildExprId,
    },
}

#[derive(Clone, Copy, Debug)]
enum LoweredStrPredicate {
    StartsWith,
    EndsWith,
}

#[derive(Clone, Debug)]
enum ScanCondition {
    TrimEmpty,
    TrimStartsWith(Vec<u8>),
    StartsWith(Vec<u8>),
}

#[derive(Clone, Debug)]
struct ScanCheck {
    condition: ScanCondition,
    counter_slot: usize,
}

#[derive(Clone, Debug)]
struct ScanBytes {
    line_slot: usize,
    block_depth_slot: usize,
    code_seen_slot: usize,
    comment_seen_slot: usize,
    in_string_slot: usize,
    string_delim_slot: usize,
    escaped_slot: usize,
    nested: bool,
    span: Span,
}

#[derive(Clone, Debug)]
enum LoweredErrorExpr {
    Simple {
        kind: String,
        message: String,
    },
    Structured {
        family: String,
        variant: String,
        fields: Vec<(Arc<str>, BuildExprId)>,
        facets: Vec<Name>,
    },
}

#[derive(Clone, Debug)]
struct LoweredStrView {
    text: Arc<str>,
    start: u32,
    end: u32,
}

impl LoweredStrView {
    fn try_new(text: Arc<str>, start: usize, end: usize) -> Option<Self> {
        debug_assert!(start <= end);
        debug_assert!(text.is_char_boundary(start));
        debug_assert!(text.is_char_boundary(end));
        let start = u32::try_from(start).ok()?;
        let end = u32::try_from(end).ok()?;
        Some(Self { text, start, end })
    }

    fn start(&self) -> usize {
        self.start as usize
    }

    fn end(&self) -> usize {
        self.end as usize
    }

    fn as_str(&self) -> &str {
        &self.text[self.start()..self.end()]
    }

    fn into_arc(self) -> Arc<str> {
        self.as_str().into()
    }
}

fn lowered_str_view_value(text: Arc<str>, start: usize, end: usize) -> LoweredValue {
    match LoweredStrView::try_new(text.clone(), start, end) {
        Some(view) => LoweredValue::StrView(view),
        None => LoweredValue::Str(Arc::from(&text[start..end])),
    }
}

fn assign_lowered_str_view(slot: &mut LoweredValue, text: &Arc<str>, start: usize, end: usize) {
    match slot {
        LoweredValue::StrView(view)
            if Arc::ptr_eq(&view.text, text)
                && let (Ok(start), Ok(end)) = (u32::try_from(start), u32::try_from(end)) =>
        {
            debug_assert!(start <= end);
            debug_assert!(text.is_char_boundary(start as usize));
            debug_assert!(text.is_char_boundary(end as usize));
            view.start = start;
            view.end = end;
        }
        _ => {
            *slot = lowered_str_view_value(text.clone(), start, end);
        }
    }
}

#[derive(Clone, Debug)]
struct LoweredBytesView {
    bytes: Arc<[u8]>,
    start: u32,
    end: u32,
}

impl LoweredBytesView {
    fn try_new(bytes: Arc<[u8]>, start: usize, end: usize) -> Option<Self> {
        debug_assert!(start <= end);
        debug_assert!(end <= bytes.len());
        let start = u32::try_from(start).ok()?;
        let end = u32::try_from(end).ok()?;
        Some(Self { bytes, start, end })
    }

    fn start(&self) -> usize {
        self.start as usize
    }

    fn end(&self) -> usize {
        self.end as usize
    }

    fn as_slice(&self) -> &[u8] {
        &self.bytes[self.start()..self.end()]
    }
}

fn lowered_bytes_view_value(bytes: Arc<[u8]>, start: usize, end: usize) -> LoweredValue {
    match LoweredBytesView::try_new(bytes.clone(), start, end) {
        Some(view) => LoweredValue::BytesView(view),
        None => LoweredValue::Bytes(Arc::from(&bytes[start..end])),
    }
}

fn assign_lowered_bytes_view(slot: &mut LoweredValue, bytes: &Arc<[u8]>, start: usize, end: usize) {
    match slot {
        LoweredValue::BytesView(view)
            if Arc::ptr_eq(&view.bytes, bytes)
                && let (Ok(start), Ok(end)) = (u32::try_from(start), u32::try_from(end)) =>
        {
            debug_assert!(start <= end);
            debug_assert!((end as usize) <= bytes.len());
            view.start = start;
            view.end = end;
        }
        _ => {
            *slot = lowered_bytes_view_value(bytes.clone(), start, end);
        }
    }
}

#[derive(Clone, Debug)]
enum LoweredValue {
    Null,
    Unit,
    Int(i64),
    Float(FloatValue),
    Duration(DurationValue),
    Bool(bool),
    Str(Arc<str>),
    StrView(LoweredStrView),
    Bytes(Arc<[u8]>),
    BytesView(LoweredBytesView),
    Digest(Box<DigestValue>),
    Regex(Box<RegexValue>),
    Status(Box<ProcessStatus>),
    Path(PathValue),
    FsEntry(crate::runtime::value::FsEntryValue),
    Command(Box<CommandPlan>),
    ProcessHandle(Box<ProcessHandleValue>),
    Stream(Box<StreamValue>),
    Pure(FunctionName),
    Proc(FunctionName),
    Error(Box<Value>),
    Record(BTreeMap<Arc<str>, LoweredValue>),
    RecordVec(Vec<(Name, LoweredValue)>),
    Stats {
        blanks: i64,
        code: i64,
        comments: i64,
    },
    StatsBlob(Box<LoweredStatsValue>),
    Module(BTreeMap<Arc<str>, LoweredValue>),
    List(Vec<LoweredValue>),
    SharedList(Arc<Vec<LoweredValue>>),
    Map(BTreeMap<String, LoweredValue>),
    Tag(Box<LoweredTagValue>),
    ResultOk(Box<LoweredValue>),
    ResultErr(Box<Value>),
}

#[derive(Clone, Debug, PartialEq)]
struct LoweredTagValue {
    name: Arc<str>,
    fields: Vec<LoweredValue>,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::runtime::eval) struct LoweredStatsValue {
    pub(in crate::runtime::eval) blanks: i64,
    pub(in crate::runtime::eval) blobs: BTreeMap<String, LoweredValue>,
    pub(in crate::runtime::eval) code: i64,
    pub(in crate::runtime::eval) comments: i64,
}

impl LoweredStatsValue {
    fn field_value(&self, field: &str) -> Option<LoweredValue> {
        Some(match field {
            "blanks" => LoweredValue::Int(self.blanks),
            "blobs" => LoweredValue::Map(self.blobs.clone()),
            "code" => LoweredValue::Int(self.code),
            "comments" => LoweredValue::Int(self.comments),
            _ => return None,
        })
    }

    pub(in crate::runtime::eval) fn to_record_vec(&self) -> Vec<(Name, LoweredValue)> {
        vec![
            (Name::intern("blanks"), LoweredValue::Int(self.blanks)),
            (Name::intern("blobs"), LoweredValue::Map(self.blobs.clone())),
            (Name::intern("code"), LoweredValue::Int(self.code)),
            (Name::intern("comments"), LoweredValue::Int(self.comments)),
        ]
    }

    pub(in crate::runtime::eval) fn to_record_map(&self) -> RecordMap {
        RecordMap::from([
            (Arc::from("blanks"), Value::Int(self.blanks)),
            (
                Arc::from("blobs"),
                Value::Map(
                    self.blobs
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone().into_value()))
                        .collect(),
                ),
            ),
            (Arc::from("code"), Value::Int(self.code)),
            (Arc::from("comments"), Value::Int(self.comments)),
        ])
    }
}

pub(in crate::runtime::eval) fn lowered_stats_field_value(
    stats: &LoweredStatsValue,
    field: &str,
) -> Option<LoweredValue> {
    stats.field_value(field)
}

pub(in crate::runtime::eval) fn lowered_inline_stats_field_value(
    blanks: i64,
    code: i64,
    comments: i64,
    field: &str,
) -> Option<LoweredValue> {
    Some(match field {
        "blanks" => LoweredValue::Int(blanks),
        "blobs" => LoweredValue::Map(BTreeMap::new()),
        "code" => LoweredValue::Int(code),
        "comments" => LoweredValue::Int(comments),
        _ => return None,
    })
}

pub(in crate::runtime::eval) fn lowered_inline_stats_to_record_vec(
    blanks: i64,
    code: i64,
    comments: i64,
) -> Vec<(Name, LoweredValue)> {
    vec![
        (Name::intern("blanks"), LoweredValue::Int(blanks)),
        (Name::intern("blobs"), LoweredValue::Map(BTreeMap::new())),
        (Name::intern("code"), LoweredValue::Int(code)),
        (Name::intern("comments"), LoweredValue::Int(comments)),
    ]
}

pub(in crate::runtime::eval) fn lowered_inline_stats_to_record_map(
    blanks: i64,
    code: i64,
    comments: i64,
) -> RecordMap {
    RecordMap::from([
        (Arc::from("blanks"), Value::Int(blanks)),
        (Arc::from("blobs"), Value::Map(BTreeMap::new())),
        (Arc::from("code"), Value::Int(code)),
        (Arc::from("comments"), Value::Int(comments)),
    ])
}

pub(in crate::runtime::eval) fn lowered_record_vec_or_stats(
    record: Vec<(Name, LoweredValue)>,
) -> LoweredValue {
    if let Some(stats) = lowered_stats_from_record_vec(&record) {
        if stats.blobs.is_empty() {
            return LoweredValue::Stats {
                blanks: stats.blanks,
                code: stats.code,
                comments: stats.comments,
            };
        }
        return LoweredValue::StatsBlob(Box::new(stats));
    }
    LoweredValue::RecordVec(record)
}

fn lowered_stats_from_record_vec(record: &[(Name, LoweredValue)]) -> Option<LoweredStatsValue> {
    if record.len() != 4
        || record[0].0 != "blanks"
        || record[1].0 != "blobs"
        || record[2].0 != "code"
        || record[3].0 != "comments"
    {
        return None;
    }
    let LoweredValue::Int(blanks) = record[0].1 else {
        return None;
    };
    let LoweredValue::Map(blobs) = &record[1].1 else {
        return None;
    };
    let LoweredValue::Int(code) = record[2].1 else {
        return None;
    };
    let LoweredValue::Int(comments) = record[3].1 else {
        return None;
    };
    Some(LoweredStatsValue {
        blanks,
        blobs: blobs.clone(),
        code,
        comments,
    })
}

pub(in crate::runtime::eval) fn lowered_record_vec_get<'a>(
    record: &'a [(Name, LoweredValue)],
    field: &str,
) -> Option<&'a LoweredValue> {
    record
        .iter()
        .find_map(|(key, value)| (key.as_str() == field).then_some(value))
}

pub(in crate::runtime::eval) fn lowered_record_vec_get_mut<'a>(
    record: &'a mut [(Name, LoweredValue)],
    field: &str,
) -> Option<&'a mut LoweredValue> {
    record
        .iter_mut()
        .find_map(|(key, value)| (key.as_str() == field).then_some(value))
}

pub(in crate::runtime::eval) fn lowered_record_vec_insert(
    record: &mut Vec<(Name, LoweredValue)>,
    field: Name,
    value: LoweredValue,
) {
    if let Some((_, slot)) = record.iter_mut().find(|(key, _)| *key == field) {
        *slot = value;
    } else {
        record.push((field, value));
        record.sort_unstable_by_key(|left| left.0);
    }
}

fn lowered_record_map_eq_vec(
    map: &BTreeMap<Arc<str>, LoweredValue>,
    vec: &[(Name, LoweredValue)],
) -> bool {
    map.len() == vec.len()
        && vec.iter().all(|(key, value)| {
            let key_text = key.as_str();
            map.get::<str>(key_text.as_str())
                .is_some_and(|left| left == value)
        })
}

fn lowered_record_map_eq_stats_value(
    map: &BTreeMap<Arc<str>, LoweredValue>,
    stats: &LoweredValue,
) -> bool {
    map.len() == 4
        && map.get("blanks").is_some_and(|value| {
            lowered_stats_value_field(stats, "blanks").is_some_and(|field| value == &field)
        })
        && map.get("blobs").is_some_and(|value| {
            lowered_stats_value_field(stats, "blobs").is_some_and(|field| value == &field)
        })
        && map.get("code").is_some_and(|value| {
            lowered_stats_value_field(stats, "code").is_some_and(|field| value == &field)
        })
        && map.get("comments").is_some_and(|value| {
            lowered_stats_value_field(stats, "comments").is_some_and(|field| value == &field)
        })
}

fn lowered_record_vec_eq_stats_value(
    record: &[(Name, LoweredValue)],
    stats: &LoweredValue,
) -> bool {
    record.len() == 4
        && lowered_record_vec_get(record, "blanks").is_some_and(|value| {
            lowered_stats_value_field(stats, "blanks").is_some_and(|field| value == &field)
        })
        && lowered_record_vec_get(record, "blobs").is_some_and(|value| {
            lowered_stats_value_field(stats, "blobs").is_some_and(|field| value == &field)
        })
        && lowered_record_vec_get(record, "code").is_some_and(|value| {
            lowered_stats_value_field(stats, "code").is_some_and(|field| value == &field)
        })
        && lowered_record_vec_get(record, "comments").is_some_and(|value| {
            lowered_stats_value_field(stats, "comments").is_some_and(|field| value == &field)
        })
}

fn lowered_stats_value_field(stats: &LoweredValue, field: &str) -> Option<LoweredValue> {
    match stats {
        LoweredValue::Stats {
            blanks,
            code,
            comments,
        } => lowered_inline_stats_field_value(*blanks, *code, *comments, field),
        LoweredValue::StatsBlob(stats) => lowered_stats_field_value(stats, field),
        _ => None,
    }
}

impl PartialEq for LoweredValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Unit, Self::Unit) => true,
            (Self::Int(left), Self::Int(right)) => left == right,
            (Self::Float(left), Self::Float(right)) => left == right,
            (Self::Duration(left), Self::Duration(right)) => left == right,
            (Self::Bool(left), Self::Bool(right)) => left == right,
            (Self::Str(left), Self::Str(right)) => left == right,
            (Self::Str(left), Self::StrView(right)) => left.as_ref() == right.as_str(),
            (Self::StrView(left), Self::Str(right)) => left.as_str() == right.as_ref(),
            (Self::StrView(left), Self::StrView(right)) => left.as_str() == right.as_str(),
            (Self::Bytes(left), Self::Bytes(right)) => left == right,
            (Self::Bytes(left), Self::BytesView(right)) => left.as_ref() == right.as_slice(),
            (Self::BytesView(left), Self::Bytes(right)) => left.as_slice() == right.as_ref(),
            (Self::BytesView(left), Self::BytesView(right)) => left.as_slice() == right.as_slice(),
            (Self::Digest(left), Self::Digest(right)) => left == right,
            (Self::Regex(left), Self::Regex(right)) => left == right,
            (Self::Status(left), Self::Status(right)) => left == right,
            (Self::Path(left), Self::Path(right)) => left == right,
            (Self::FsEntry(left), Self::FsEntry(right)) => left == right,
            (Self::Command(left), Self::Command(right)) => left == right,
            (Self::ProcessHandle(left), Self::ProcessHandle(right)) => left == right,
            (Self::Stream(left), Self::Stream(right)) => left == right,
            (Self::Pure(left), Self::Pure(right)) => left == right,
            (Self::Proc(left), Self::Proc(right)) => left == right,
            (Self::Error(left), Self::Error(right)) => left == right,
            (Self::Record(left), Self::Record(right)) => left == right,
            (Self::RecordVec(left), Self::RecordVec(right)) => left == right,
            (
                Self::Stats {
                    blanks: left_blanks,
                    code: left_code,
                    comments: left_comments,
                },
                Self::Stats {
                    blanks: right_blanks,
                    code: right_code,
                    comments: right_comments,
                },
            ) => {
                left_blanks == right_blanks
                    && left_code == right_code
                    && left_comments == right_comments
            }
            (Self::StatsBlob(left), Self::StatsBlob(right)) => left == right,
            (Self::Record(left), Self::RecordVec(right)) => lowered_record_map_eq_vec(left, right),
            (Self::RecordVec(left), Self::Record(right)) => lowered_record_map_eq_vec(right, left),
            (Self::Record(left), right @ (Self::Stats { .. } | Self::StatsBlob(_))) => {
                lowered_record_map_eq_stats_value(left, right)
            }
            (left @ (Self::Stats { .. } | Self::StatsBlob(_)), Self::Record(right)) => {
                lowered_record_map_eq_stats_value(right, left)
            }
            (Self::RecordVec(left), right @ (Self::Stats { .. } | Self::StatsBlob(_))) => {
                lowered_record_vec_eq_stats_value(left, right)
            }
            (left @ (Self::Stats { .. } | Self::StatsBlob(_)), Self::RecordVec(right)) => {
                lowered_record_vec_eq_stats_value(right, left)
            }
            (Self::Module(left), Self::Module(right)) => left == right,
            (Self::List(left), Self::List(right)) => left == right,
            (Self::List(left), Self::SharedList(right)) => left == right.as_ref(),
            (Self::SharedList(left), Self::List(right)) => left.as_ref() == right,
            (Self::SharedList(left), Self::SharedList(right)) => left == right,
            (Self::Map(left), Self::Map(right)) => left == right,
            (Self::Tag(left), Self::Tag(right)) => left == right,
            (Self::ResultOk(left), Self::ResultOk(right)) => left == right,
            (Self::ResultErr(left), Self::ResultErr(right)) => left == right,
            _ => false,
        }
    }
}

impl LoweredValue {
    fn into_value(self) -> Value {
        match self {
            Self::Null => Value::Null,
            Self::Unit => Value::Unit,
            Self::Int(value) => Value::Int(value),
            Self::Float(value) => Value::Float(value),
            Self::Duration(value) => Value::Duration(value),
            Self::Bool(value) => Value::Bool(value),
            Self::Str(value) => Value::Str(value),
            Self::StrView(value) => Value::Str(value.into_arc()),
            Self::Bytes(value) => Value::Bytes(value.to_vec()),
            Self::BytesView(value) => Value::Bytes(value.as_slice().to_vec()),
            Self::Digest(value) => Value::Digest(value),
            Self::Regex(value) => Value::Regex(*value),
            Self::Status(value) => Value::Status(*value),
            Self::Path(value) => Value::Path(value),
            Self::FsEntry(value) => Value::FsEntry(value),
            Self::Command(value) => Value::Command(value),
            Self::ProcessHandle(value) => Value::ProcessHandle(value),
            Self::Stream(value) => Value::Stream(value),
            Self::Pure(value) => Value::Pure(value),
            Self::Proc(value) => Value::Proc(value),
            Self::Error(value) => *value,
            Self::Record(value) => Value::Record(RecordMap::from_name_values(
                value
                    .into_iter()
                    .map(|(key, value)| (Name::intern(key.as_ref()), value.into_value()))
                    .collect(),
            )),
            Self::RecordVec(value) => Value::Record(RecordMap::from_name_values(
                value
                    .into_iter()
                    .map(|(key, value)| (key, value.into_value()))
                    .collect(),
            )),
            Self::Stats {
                blanks,
                code,
                comments,
            } => Value::Record(lowered_inline_stats_to_record_map(blanks, code, comments)),
            Self::StatsBlob(value) => Value::Record(value.to_record_map()),
            Self::Module(value) => Value::Module(RecordMap::from_name_values(
                value
                    .into_iter()
                    .map(|(key, value)| (Name::intern(key.as_ref()), value.into_value()))
                    .collect(),
            )),
            Self::List(value) => {
                Value::List(value.into_iter().map(LoweredValue::into_value).collect())
            }
            Self::SharedList(value) => Value::List(
                value
                    .iter()
                    .cloned()
                    .map(LoweredValue::into_value)
                    .collect(),
            ),
            Self::Map(value) => {
                let mut map = BTreeMap::new();
                for (key, value) in value {
                    map.insert(key, value.into_value());
                }
                Value::Map(map)
            }
            Self::Tag(value) => Value::Tag {
                name: value.name,
                fields: value
                    .fields
                    .into_iter()
                    .map(LoweredValue::into_value)
                    .collect(),
            },
            Self::ResultOk(value) => Value::ok(value.into_value()),
            Self::ResultErr(value) => Value::err(*value),
        }
    }

    fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "Null",
            Self::Unit => "Unit",
            Self::Int(_) => "Int",
            Self::Float(_) => "Float",
            Self::Duration(_) => "Duration",
            Self::Bool(_) => "Bool",
            Self::Str(_) | Self::StrView(_) => "Str",
            Self::Bytes(_) | Self::BytesView(_) => "Bytes",
            Self::Digest(_) => "Digest",
            Self::Regex(_) => "Regex",
            Self::Status(_) => "Status",
            Self::Path(_) => "Path",
            Self::FsEntry(_) => "Record",
            Self::Command(_) => "Command",
            Self::ProcessHandle(_) => "ProcessHandle",
            Self::Stream(_) => "Stream",
            Self::Pure(_) => "Pure",
            Self::Proc(_) => "Proc",
            Self::Error(_) => "Error",
            Self::Record(_) | Self::RecordVec(_) | Self::Stats { .. } | Self::StatsBlob(_) => {
                "Record"
            }
            Self::Module(_) => "Module",
            Self::List(_) | Self::SharedList(_) => "List",
            Self::Map(_) => "Map",
            Self::Tag(_) => "Tag",
            Self::ResultOk(_) | Self::ResultErr(_) => "Result",
        }
    }
}

/// Method names the lowering pass will emit (as `BuildExprRow::Method` or a
/// dedicated node). Kept as data — not a `matches!` — so it is the single source
/// of truth shared by `lowered_method_name`, the consistency test, and
/// `tools/xsh-ir-coverage.xsh`, which parses this declaration directly.
const LOWERED_METHOD_NAMES: &[&str] = &[
    "count_lines",
    "count_words",
    "count_chars",
    "count_bytes",
    "byte_len",
    "byte_at",
    "byte_slice",
    "slice",
    "utf8",
    "find",
    "trim",
    "lower",
    "upper",
    "reverse",
    "lines",
    "bytes_lines",
    "words",
    "fields",
    "split",
    "replace",
    "wrap",
    "translate",
    "delete",
    "squeeze",
    "matches",
    "captures",
    "parse_int",
    "parse_float",
    "float",
    "floor",
    "ceil",
    "round",
    "format",
    "sqrt",
    "pow",
    "exp",
    "ln",
    "log",
    "sin",
    "cos",
    "tan",
    "abs",
    "exited",
    "signaled",
    "exited_with",
    "exit_code",
    "signal_number",
    "display",
    "name",
    "ext",
    "with_ext",
    "normalize",
    "parent",
    "strip_prefix",
    "relative_to",
    "copy",
    "rename",
    "remove_dir",
    "touch",
    "touch_from",
    "truncate",
    "chmod",
    "hardlink",
    "unlink",
    "md5",
    "sha1",
    "sha256",
    "sha512",
    "compare",
    "dump",
    "strings",
    "chunks",
    "hex",
    "base64",
    "base32",
    "base64_decode",
    "base32_decode",
    "starts_with",
    "ends_with",
    "contains",
    "cancel",
    "context",
    "collect",
    "has",
    "get",
    "keys",
    "values",
    "len",
    "join",
    "push",
    "extend",
    "set",
    "remove",
];

fn lowered_method_name(name: &str) -> bool {
    LOWERED_METHOD_NAMES.contains(&name)
}

#[derive(Clone, Debug)]
enum RuntimeEnv {
    Inherited,
    Snapshot(BTreeMap<Vec<u8>, Vec<u8>>),
}

impl RuntimeEnv {
    fn inherited() -> Self {
        Self::Inherited
    }

    fn from_snapshot(env: BTreeMap<Vec<u8>, Vec<u8>>) -> Self {
        Self::Snapshot(env)
    }

    fn snapshot(&mut self) -> &BTreeMap<Vec<u8>, Vec<u8>> {
        self.snapshot_mut()
    }

    fn snapshot_mut(&mut self) -> &mut BTreeMap<Vec<u8>, Vec<u8>> {
        if matches!(self, Self::Inherited) {
            *self = Self::Snapshot(initial_env());
        }
        match self {
            Self::Inherited => unreachable!("runtime env snapshot was initialized"),
            Self::Snapshot(env) => env,
        }
    }

    fn snapshot_clone(&mut self) -> BTreeMap<Vec<u8>, Vec<u8>> {
        self.snapshot().clone()
    }

    fn get_owned(&self, key: &[u8]) -> Option<Vec<u8>> {
        match self {
            Self::Inherited => std::env::var_os(OsStr::from_bytes(key))
                .map(|value| value.as_os_str().as_bytes().to_vec()),
            Self::Snapshot(env) => env.get(key).cloned(),
        }
    }

    fn insert(&mut self, name: Vec<u8>, value: Vec<u8>) {
        self.snapshot_mut().insert(name, value);
    }

    fn extend(&mut self, overlay: BTreeMap<Vec<u8>, Vec<u8>>) {
        self.snapshot_mut().extend(overlay);
    }

    fn into_snapshot(self) -> BTreeMap<Vec<u8>, Vec<u8>> {
        match self {
            Self::Inherited => initial_env(),
            Self::Snapshot(env) => env,
        }
    }
}

/// Signature of a function exported by a dynamically loaded module, captured
/// from the compact declaration probe so `module.require` can validate
/// `export proc`/`export pure` contract fields.
#[derive(Clone, Debug)]
pub(super) struct ModuleExportSignature {
    pub(super) pure: bool,
    pub(super) sig: CallableType,
}

pub struct Evaluator {
    sources: Arc<SourceMap>,
    command_name: String,
    exe_path: String,
    scopes: Vec<FxHashMap<Name, Binding>>,
    // Signatures of functions exported by dynamically loaded modules
    // (`module.load`), keyed by the export's `FunctionName`. Captured from the
    // compact declaration probe at load time so `module.require` can validate
    // `export proc`/`export pure` contract fields without the old recursive AST.
    module_export_signatures:
        Arc<FxHashMap<crate::runtime::value::FunctionName, ModuleExportSignature>>,
    indexed_program: Option<Arc<FullProgram>>,
    indexed_function_cache: FxHashMap<(LoweredFunctionKey, LoweredFunctionKind), usize>,
    indexed_dynamic_functions: Arc<FxHashMap<QualifiedName, DynamicFunction>>,
    lowered_slot_pool: Vec<Vec<LoweredValue>>,
    tag_variants: FxHashMap<Name, usize>,
    error_families: FxHashMap<Name, RuntimeErrorFamily>,
    module_value_cache: Arc<FxHashMap<String, RecordMap>>,
    function_modules: Arc<FxHashMap<Name, String>>,
    qualified_function_modules: Arc<FxHashMap<QualifiedName, String>>,
    active_modules: Vec<String>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    cwd: PathBuf,
    env: RuntimeEnv,
    interactive: bool,
    interactive_command_dispatcher: Option<InteractiveCommandDispatcher>,
    last_status: Option<ProcessStatus>,
    trace_enabled: bool,
    trace_events: Vec<TraceEvent>,
    event_stack: Vec<TraceFrame>,
    call_stack: Vec<TracebackFrame>,
    pending_traceback: Option<Traceback>,
    stream_items: Vec<Value>,
    unix_next_pid: i64,
    fs_locks: Vec<Option<std::fs::File>>,
    fs_roots: Vec<Option<FsRootHandle>>,
    net_agents: FxHashMap<NetAgentKey, NetAgent>,
    net_pool_options: FxHashMap<String, NetPoolOptions>,
    utils_cache: FxHashMap<String, Value>,
    signal_hooks: FxHashMap<String, RegisteredSignalHook>,
    signal_handler_guards: Vec<SignalHandlerGuard>,
    active_process_groups: Vec<ActiveProcessGroup>,
    next_process_handle_id: u64,
    process_handles: BTreeMap<u64, LiveProcessHandle>,
    scope_ids: Vec<u64>,
    signal_state: EvaluatorSignalState,
    #[cfg(feature = "native-tests")]
    pub(super) test_mocks: FxHashMap<String, Vec<TestMock>>,
    #[cfg(feature = "native-tests")]
    pub(super) test_calls: Vec<TestCall>,
    #[cfg(feature = "native-tests")]
    pub(super) native_test_host: Option<NativeTestHost>,
    #[cfg(feature = "native-tests")]
    test_temp_counter: u64,
}

struct LoweredSharedState {
    sources: Arc<SourceMap>,
    command_name: String,
    exe_path: String,
    scopes: Vec<FxHashMap<Name, Binding>>,
    module_export_signatures:
        Arc<FxHashMap<crate::runtime::value::FunctionName, ModuleExportSignature>>,
    indexed_program: Option<Arc<FullProgram>>,
    indexed_dynamic_functions: Arc<FxHashMap<QualifiedName, DynamicFunction>>,
    tag_variants: FxHashMap<Name, usize>,
    error_families: FxHashMap<Name, RuntimeErrorFamily>,
    module_value_cache: Arc<FxHashMap<String, RecordMap>>,
    function_modules: Arc<FxHashMap<Name, String>>,
    qualified_function_modules: Arc<FxHashMap<QualifiedName, String>>,
    active_modules: Vec<String>,
    cwd: PathBuf,
    env: RuntimeEnv,
    #[cfg(feature = "native-tests")]
    native_test_host: Option<NativeTestHost>,
}

#[cfg(feature = "native-tests")]
impl PreparedTestProgram {
    pub fn eval_test(
        &self,
        test_name: &str,
        ctx: Value,
        trace_enabled: bool,
        env_overlay: Vec<(Vec<u8>, Vec<u8>)>,
    ) -> TestEvalOutput {
        let symbols = self.symbols.clone();
        run_eval(move || {
            symbols.with_current(|| {
                if !trace_enabled && let Some(failure) = &self.setup_failure {
                    return failure.clone();
                }
                let shared = if trace_enabled {
                    &self.shared
                } else {
                    &self.setup_shared
                };
                let mut evaluator = Evaluator::new_lowered_worker(shared);
                for (name, value) in env_overlay {
                    evaluator = evaluator.with_env_var(name, value);
                }
                if trace_enabled {
                    evaluator = evaluator.with_tracing();
                    evaluator.eval_installed_indexed_test_inner(
                        &self.plan,
                        self.script_span,
                        test_name,
                        ctx,
                    )
                } else {
                    evaluator.eval_installed_test_call_inner(self.script_span, test_name, ctx)
                }
            })
        })
    }
}

pub(in crate::runtime::eval) enum FsRootHandle {
    Dir(cap_std::fs::Dir),
    TempDir(cap_tempfile::TempDir),
}

impl FsRootHandle {
    pub(in crate::runtime::eval) fn dir(&self) -> &cap_std::fs::Dir {
        match self {
            Self::Dir(dir) => dir,
            Self::TempDir(dir) => dir,
        }
    }
}

pub struct InteractiveCommandContext<'a> {
    pub stdout: &'a mut Vec<u8>,
    pub stderr: &'a mut Vec<u8>,
    pub cwd: &'a std::path::Path,
    pub env: &'a BTreeMap<Vec<u8>, Vec<u8>>,
}

pub type InteractiveCommandDispatcher =
    fn(&str, Vec<Vec<u8>>, &mut InteractiveCommandContext<'_>) -> u8;

impl Evaluator {
    pub fn new(argv: Vec<String>) -> Self {
        Self::new_with_sources(argv, SourceMap::new())
    }

    pub fn new_with_sources(argv: Vec<String>, sources: SourceMap) -> Self {
        Self::new_with_sources_and_command(argv, sources, "command".to_string())
    }

    pub fn frontend_lowered_stats(&self) -> FrontendLoweredStats {
        if let Some(indexed) = &self.indexed_program {
            return FrontendLoweredStats {
                function_count: indexed.function_count(),
                constructed_functions: indexed.function_count(),
                statement_count: 0,
                expression_count: indexed.instruction_count(),
                pattern_count: 0,
                blocker_events: 0,
                retained_estimate_bytes: indexed.store_retained_bytes(),
            };
        }
        FrontendLoweredStats::default()
    }

    pub fn new_with_shared_sources(argv: Vec<String>, sources: Arc<SourceMap>) -> Self {
        Self::new_with_shared_sources_and_command(argv, sources, "command".to_string())
    }

    pub fn new_with_sources_and_command(
        argv: Vec<String>,
        sources: SourceMap,
        command_name: String,
    ) -> Self {
        Self::new_with_sources_and_command_inner(argv, Arc::new(sources), command_name, None)
    }

    pub fn new_with_shared_sources_and_command(
        argv: Vec<String>,
        sources: Arc<SourceMap>,
        command_name: String,
    ) -> Self {
        Self::new_with_sources_and_command_inner(argv, sources, command_name, None)
    }

    fn new_with_sources_and_command_inner(
        argv: Vec<String>,
        sources: Arc<SourceMap>,
        command_name: String,
        cwd: Option<PathBuf>,
    ) -> Self {
        let cwd =
            cwd.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let mut evaluator = Self {
            sources,
            command_name,
            exe_path: std::env::current_exe()
                .ok()
                .and_then(|p| p.to_str().map(String::from))
                .unwrap_or_default(),
            scopes: vec![FxHashMap::default()],
            module_export_signatures: Arc::new(FxHashMap::default()),
            indexed_program: None,
            indexed_function_cache: FxHashMap::default(),
            indexed_dynamic_functions: Arc::new(FxHashMap::default()),
            lowered_slot_pool: Vec::new(),
            tag_variants: FxHashMap::default(),
            error_families: FxHashMap::default(),
            module_value_cache: Arc::new(FxHashMap::default()),
            function_modules: Arc::new(FxHashMap::default()),
            qualified_function_modules: Arc::new(FxHashMap::default()),
            active_modules: Vec::new(),
            stdout: Vec::new(),
            stderr: Vec::new(),
            cwd,
            env: RuntimeEnv::inherited(),
            interactive: false,
            interactive_command_dispatcher: None,
            last_status: None,
            trace_enabled: false,
            trace_events: Vec::new(),
            event_stack: Vec::new(),
            call_stack: Vec::new(),
            pending_traceback: None,
            stream_items: Vec::new(),
            unix_next_pid: 1000,
            fs_locks: Vec::new(),
            fs_roots: Vec::new(),
            net_agents: FxHashMap::default(),
            net_pool_options: FxHashMap::default(),
            utils_cache: FxHashMap::default(),
            signal_hooks: FxHashMap::default(),
            signal_handler_guards: Vec::new(),
            active_process_groups: Vec::new(),
            next_process_handle_id: 1,
            process_handles: BTreeMap::new(),
            scope_ids: vec![0],
            signal_state: EvaluatorSignalState::default(),
            #[cfg(feature = "native-tests")]
            test_mocks: FxHashMap::default(),
            #[cfg(feature = "native-tests")]
            test_calls: Vec::new(),
            #[cfg(feature = "native-tests")]
            native_test_host: None,
            #[cfg(feature = "native-tests")]
            test_temp_counter: 0,
        };
        let argv = Value::List(argv.into_iter().map(|s| Value::Str(s.into())).collect());
        evaluator.define(
            "args",
            Binding {
                value: argv.clone(),
                mutable: false,
            },
        );
        evaluator.define(
            "ARGV",
            Binding {
                value: argv,
                mutable: false,
            },
        );
        evaluator
    }

    pub fn into_sources(self) -> SourceMap {
        Arc::try_unwrap(self.sources).unwrap_or_else(|sources| (*sources).clone())
    }

    pub fn with_tracing(mut self) -> Self {
        self.trace_enabled = true;
        self
    }

    #[cfg(feature = "native-tests")]
    pub fn with_native_test_host(mut self, host: NativeTestHost) -> Self {
        self.native_test_host = Some(host);
        self
    }

    fn register_indexed_signal_hook(
        &mut self,
        signal: &str,
        pre_cancel: Option<&str>,
        program: Arc<indexed::full::FullProgram>,
        driver_step: usize,
        body: u32,
        hook_slots: Vec<LoweredTopLevelSlot>,
        slot_count: usize,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let signal = normalize_hook_signal(signal, span).map_err(|rejection| {
            RuntimeError::new("signal-hook", signal_rejection_message(signal, rejection))
                .with_span(span)
        })?;
        let pre_cancel = match pre_cancel {
            Some(literal) => DurationValue::from_literal(literal).ok_or_else(|| {
                RuntimeError::new("signal-hook", "`--pre-cancel` expects a duration literal")
                    .with_span(span)
            })?,
            None => DurationValue { millis: 150 },
        };
        let guard = install_hook_signal_handler(signal.number)
            .map_err(|error| RuntimeError::new("signal-hook", error.to_string()).with_span(span))?;
        self.signal_handler_guards.push(guard);
        let ignore_pending_primary = signal_snapshot().primary == Some(signal.number);
        self.signal_hooks.insert(
            signal.name.clone(),
            RegisteredSignalHook {
                signal,
                pre_cancel,
                indexed_body: RegisteredIndexedSignalBody {
                    program,
                    driver_step,
                    body,
                    slot_count,
                },
                lowered_slots: hook_slots,
                scope: self.scopes.first().cloned().unwrap_or_default(),
                span,
                ignore_pending_primary,
            },
        );
        Ok(())
    }

    fn lowered_shared_state(&self) -> Arc<LoweredSharedState> {
        Arc::new(LoweredSharedState {
            sources: self.sources.clone(),
            command_name: self.command_name.clone(),
            exe_path: self.exe_path.clone(),
            scopes: self.scopes.clone(),
            module_export_signatures: self.module_export_signatures.clone(),
            indexed_program: self.indexed_program.clone(),
            indexed_dynamic_functions: self.indexed_dynamic_functions.clone(),
            tag_variants: self.tag_variants.clone(),
            error_families: self.error_families.clone(),
            module_value_cache: self.module_value_cache.clone(),
            function_modules: self.function_modules.clone(),
            qualified_function_modules: self.qualified_function_modules.clone(),
            active_modules: self.active_modules.clone(),
            cwd: self.cwd.clone(),
            env: self.env.clone(),
            #[cfg(feature = "native-tests")]
            native_test_host: self.native_test_host.clone(),
        })
    }

    fn new_lowered_worker(shared: &LoweredSharedState) -> Self {
        Self {
            sources: shared.sources.clone(),
            command_name: shared.command_name.clone(),
            exe_path: shared.exe_path.clone(),
            scopes: shared.scopes.clone(),
            module_export_signatures: shared.module_export_signatures.clone(),
            indexed_program: shared.indexed_program.clone(),
            indexed_function_cache: FxHashMap::default(),
            indexed_dynamic_functions: shared.indexed_dynamic_functions.clone(),
            lowered_slot_pool: Vec::new(),
            tag_variants: shared.tag_variants.clone(),
            error_families: shared.error_families.clone(),
            module_value_cache: shared.module_value_cache.clone(),
            function_modules: shared.function_modules.clone(),
            qualified_function_modules: shared.qualified_function_modules.clone(),
            active_modules: shared.active_modules.clone(),
            stdout: Vec::new(),
            stderr: Vec::new(),
            cwd: shared.cwd.clone(),
            env: shared.env.clone(),
            #[cfg(feature = "native-tests")]
            native_test_host: shared.native_test_host.clone(),
            interactive: false,
            interactive_command_dispatcher: None,
            last_status: None,
            trace_enabled: false,
            trace_events: Vec::new(),
            event_stack: Vec::new(),
            call_stack: Vec::new(),
            pending_traceback: None,
            stream_items: Vec::new(),
            unix_next_pid: 1000,
            fs_locks: Vec::new(),
            fs_roots: Vec::new(),
            net_agents: FxHashMap::default(),
            net_pool_options: FxHashMap::default(),
            utils_cache: FxHashMap::default(),
            signal_hooks: FxHashMap::default(),
            signal_handler_guards: Vec::new(),
            active_process_groups: Vec::new(),
            next_process_handle_id: 1,
            process_handles: BTreeMap::new(),
            scope_ids: (0..shared.scopes.len() as u64).collect(),
            signal_state: EvaluatorSignalState::default(),
            #[cfg(feature = "native-tests")]
            test_mocks: FxHashMap::default(),
            #[cfg(feature = "native-tests")]
            test_calls: Vec::new(),
            #[cfg(feature = "native-tests")]
            test_temp_counter: 0,
        }
    }

    pub(super) fn service_pending_signal(&mut self, span: Span) -> Result<(), RuntimeError> {
        if self.signal_hooks.is_empty()
            && self.process_handles.is_empty()
            && !self.signal_state.hook_running
            && !self.signal_state.hook_started
            && !self.signal_state.shutdown_complete
        {
            return Ok(());
        }
        let snapshot = signal_snapshot();
        let Some(primary_number) = snapshot.primary else {
            return Ok(());
        };
        let primary = hook_signal_from_number(primary_number);
        if let Some(escalation_number) = snapshot.escalation {
            let escalation = hook_signal_from_number(escalation_number);
            self.trace_signal_escalate(&primary, &escalation, span);
            self.kill_active_process_groups();
            self.signal_state.shutdown_status = Some(default_signal_status(&primary));
            self.signal_state.shutdown_complete = true;
            return Ok(());
        }

        if self.signal_state.hook_running {
            if let Some(deadline) = self.signal_state.pre_cancel_deadline
                && Instant::now() >= deadline
            {
                self.forward_primary_to_active(&primary, span);
            }
            return Ok(());
        }
        if self.signal_state.hook_started || self.signal_state.shutdown_complete {
            return Ok(());
        }
        let Some(hook) = self.signal_hooks.get(&primary.name).cloned() else {
            if !self.process_handles.is_empty() {
                self.cancel_process_handles_for_signal(primary_number, span)?;
                self.signal_state.shutdown_complete = true;
                self.signal_state.shutdown_status = Some(default_signal_status(&primary));
                return Err(RuntimeError::new(
                    "canceled",
                    format!("process work was canceled by signal {primary_number}"),
                )
                .with_span(span));
            }
            return Ok(());
        };
        if hook.ignore_pending_primary {
            return Ok(());
        }

        self.signal_state.hook_started = true;
        self.signal_state.hook_span = Some(hook.span);
        self.signal_state.pre_cancel_deadline =
            Some(Instant::now() + Duration::from_millis(hook.pre_cancel.millis));
        self.trace_signal_received(&primary, &hook, span);
        self.trace_leaf(
            TraceKind::SignalHookEnter,
            Some(hook.span),
            Some(&hook.signal.name),
            self.signal_payload(&primary, "hook", true, None),
        );
        self.signal_state.hook_running = true;
        let hook_result = self.execute_signal_hook(&hook);
        self.signal_state.hook_running = false;
        let hook_error = signal_hook_error(&hook_result);
        self.trace_leaf(
            TraceKind::SignalHookExit,
            Some(hook.span),
            Some(&hook.signal.name),
            self.signal_payload(&primary, "hook", true, hook_error.clone()),
        );
        self.forward_primary_to_active(&primary, span);
        if !self.process_handles.is_empty() {
            self.cancel_process_handles_for_signal(primary_number, span)?;
        }

        match hook_result {
            Ok(Flow::Continue(Value::Result(ResultValue::Err(error)))) => {
                let error_value = *error;
                let error = runtime_error_from_value(error_value.clone(), hook.span);
                self.signal_state.shutdown_complete = true;
                self.pending_traceback =
                    Some(self.traceback_for_value(hook.span, "signal.hook", &error_value));
                Err(error)
            }
            Ok(Flow::Propagate(propagation)) => {
                self.signal_state.shutdown_complete = true;
                self.pending_traceback = Some(propagation.traceback);
                Err(runtime_error_from_value(propagation.error, hook.span))
            }
            Ok(Flow::Return(_) | Flow::Break(_) | Flow::ContinueLoop) => {
                self.signal_state.shutdown_complete = true;
                Err(
                    RuntimeError::new("signal-hook", "signal hook produced invalid control flow")
                        .with_span(hook.span),
                )
            }
            Ok(Flow::Continue(_)) => {
                self.signal_state.shutdown_status = Some(default_signal_status(&primary));
                self.signal_state.shutdown_complete = true;
                Ok(())
            }
            Err(error) => {
                if let Some(abort) = error.abort {
                    self.signal_state.shutdown_status = Some(abort.status);
                    self.signal_state.shutdown_force = abort.force;
                    self.signal_state.shutdown_complete = true;
                    Ok(())
                } else {
                    self.signal_state.shutdown_complete = true;
                    Err(error)
                }
            }
        }
    }

    fn execute_signal_hook(&mut self, hook: &RegisteredSignalHook) -> Result<Flow, RuntimeError> {
        let saved_scopes = std::mem::replace(&mut self.scopes, vec![hook.scope.clone()]);
        let call_span = hook.span;
        let slot_count = hook.indexed_body.slot_count;
        let mut slots = vec![LoweredValue::Unit; slot_count];
        for slot_info in &hook.lowered_slots {
            if let Some(binding) = self.lookup(slot_info.name)
                && let Some(value) = lowered_value_from_runtime(&binding.value, slot_info.kind)
            {
                slots[slot_info.slot] = value;
            }
        }
        let indexed_body = &hook.indexed_body;
        let program = Arc::clone(&indexed_body.program);
        let view = program
            .driver_step_view_absolute(indexed_body.driver_step)
            .map_err(|error| RuntimeError::new("indexed-ir", error.message).with_span(call_span))?;
        let result =
            self.eval_indexed_body_as_signal_hook(view, indexed_body.body, &mut slots, call_span);
        self.scopes = saved_scopes;
        result
    }

    fn track_process_group(&mut self, group: ProcessGroup) {
        let hook_owned = self.signal_state.hook_running;
        if self
            .active_process_groups
            .iter()
            .any(|active| active.group == group && active.hook_owned == hook_owned)
        {
            return;
        }
        self.active_process_groups
            .push(ActiveProcessGroup { group, hook_owned });
    }

    fn untrack_process_group(&mut self, group: ProcessGroup) {
        self.active_process_groups
            .retain(|active| active.group != group);
    }

    fn forward_primary_to_active(&mut self, primary: &HookSignal, span: Span) {
        if self.signal_state.primary_forwarded {
            return;
        }
        for active in &self.active_process_groups {
            if !active.hook_owned {
                active.group.signal(primary.number);
            }
        }
        self.signal_state.primary_forwarded = true;
        self.trace_leaf(
            TraceKind::SignalForward,
            Some(span),
            Some(&primary.name),
            self.signal_payload(primary, "forward", true, None),
        );
    }

    fn kill_active_process_groups(&mut self) {
        for active in &self.active_process_groups {
            active.group.kill();
        }
    }

    fn trace_signal_received(
        &mut self,
        primary: &HookSignal,
        hook: &RegisteredSignalHook,
        span: Span,
    ) {
        if self.signal_state.received_traced {
            return;
        }
        self.signal_state.received_traced = true;
        self.trace_leaf(
            TraceKind::SignalReceived,
            Some(span),
            Some(&primary.name),
            self.signal_payload(primary, "pending", true, None),
        );
        if hook.signal.name != primary.name {
            self.trace_leaf(
                TraceKind::SignalReceived,
                Some(hook.span),
                Some(&hook.signal.name),
                self.signal_payload(&hook.signal, "registered", true, None),
            );
        }
    }

    fn trace_signal_escalate(&mut self, primary: &HookSignal, escalation: &HookSignal, span: Span) {
        if self.signal_state.escalated_traced {
            return;
        }
        self.signal_state.escalated_traced = true;
        self.trace_leaf(
            TraceKind::SignalEscalate,
            Some(span),
            Some(&primary.name),
            self.signal_payload_with_escalation(primary, "escalated", escalation),
        );
    }

    fn signal_payload(
        &self,
        signal: &HookSignal,
        phase: &str,
        matching_hook: bool,
        hook_error: Option<TraceError>,
    ) -> TracePayload {
        TracePayload::Signal {
            signal_name: signal.name.clone(),
            signal_number: signal.number,
            phase: phase.to_string(),
            matching_hook,
            forwarded: self.signal_state.primary_forwarded,
            pre_cancel_ms: self.signal_state.pre_cancel_deadline.map(|_| {
                self.signal_hooks
                    .get(&signal.name)
                    .map_or(150, |hook| hook.pre_cancel.millis)
            }),
            escalation_signal_name: None,
            escalation_signal_number: None,
            hook_error,
        }
    }

    fn signal_payload_with_escalation(
        &self,
        signal: &HookSignal,
        phase: &str,
        escalation: &HookSignal,
    ) -> TracePayload {
        TracePayload::Signal {
            signal_name: signal.name.clone(),
            signal_number: signal.number,
            phase: phase.to_string(),
            matching_hook: self.signal_hooks.contains_key(&signal.name),
            forwarded: self.signal_state.primary_forwarded,
            pre_cancel_ms: self
                .signal_hooks
                .get(&signal.name)
                .map(|hook| hook.pre_cancel.millis),
            escalation_signal_name: Some(escalation.name.clone()),
            escalation_signal_number: Some(escalation.number),
            hook_error: None,
        }
    }

    pub fn with_interactive_command_dispatcher(
        mut self,
        dispatcher: InteractiveCommandDispatcher,
    ) -> Self {
        self.interactive_command_dispatcher = Some(dispatcher);
        self
    }

    pub fn new_interactive_with_sources(argv: Vec<String>, sources: SourceMap) -> Self {
        let mut evaluator = Self::new_with_sources(argv, sources);
        evaluator.interactive = true;
        evaluator
    }

    pub fn new_interactive_session_with_sources(
        argv: Vec<String>,
        sources: SourceMap,
        cwd: PathBuf,
        env: BTreeMap<Vec<u8>, Vec<u8>>,
        last_status: Option<ProcessStatus>,
    ) -> Self {
        let mut evaluator = Self::new_interactive_with_sources(argv, sources);
        evaluator.cwd = cwd;
        evaluator.env = RuntimeEnv::from_snapshot(env);
        evaluator.last_status = last_status;
        evaluator
    }

    pub fn with_env_var(mut self, name: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>) -> Self {
        self.env.insert(name.into(), value.into());
        self
    }

    /// Build, verify, and execute the indexed program. The arena is used only
    /// during construction and is never consulted by the installed evaluator.
    pub fn eval(mut self, program: &ArenaProgram, source_id: SourceId) -> EvalOutput {
        let symbols = program.symbol_owner().clone();
        run_eval(move || {
            symbols.with_current(|| {
                let plan = match self
                    .prepare_compact_indexed_only_or_diagnostic(program, source_id, false)
                {
                    Ok(plan) => plan,
                    Err(diagnostic) => {
                        return EvalOutput {
                            stdout: self.stdout,
                            stderr: self.stderr,
                            trace_events: self.trace_events,
                            diagnostics: vec![diagnostic],
                            traceback: None,
                            sources: self.sources,
                            status: 1,
                            cwd: self.cwd,
                            env: self.env.into_snapshot(),
                            last_status: self.last_status,
                        };
                    }
                };
                self.try_eval_installed_compact_indexed_only_inner(plan)
                    .unwrap_or_else(|_| unreachable!("verified indexed program remains installed"))
            })
        })
    }

    pub(super) fn write_stdout_line(&mut self, line: &str) {
        self.stdout.extend_from_slice(line.as_bytes());
        self.stdout.push(b'\n');
    }

    pub(super) fn write_stderr_line(&mut self, line: &str) {
        self.stderr.extend_from_slice(line.as_bytes());
        self.stderr.push(b'\n');
    }

    pub(super) fn flush_stdout_line(&mut self, line: &str) {
        use std::io::Write;

        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(line.as_bytes());
        let _ = stdout.write_all(b"\n");
        let _ = stdout.flush();
    }

    pub(super) fn flush_stderr_line(&mut self, line: &str) {
        use std::io::Write;

        let mut stderr = std::io::stderr().lock();
        let _ = stderr.write_all(line.as_bytes());
        let _ = stderr.write_all(b"\n");
        let _ = stderr.flush();
    }

    pub(crate) fn prepare_compact_indexed_only(
        &mut self,
        program: &ArenaProgram,
        source_id: SourceId,
    ) -> Option<CompactIndexedRunPlan> {
        program.symbol_owner().with_current(|| {
            self.prepare_compact_indexed_only_or_diagnostic(program, source_id, false)
                .ok()
        })
    }

    fn prepare_compact_indexed_only_or_diagnostic(
        &mut self,
        program: &ArenaProgram,
        source_id: SourceId,
        allow_checker_only: bool,
    ) -> Result<CompactIndexedRunPlan, Diagnostic> {
        let mut declarations = Checker::check_compact_declarations(program);
        if !declarations.diagnostics.is_empty() {
            return Err(declarations.diagnostics.remove(0));
        }
        let mut bodies = Checker::probe_compact_bodies(program, &declarations);
        if !bodies.diagnostics.is_empty() {
            return Err(bodies.diagnostics.remove(0));
        }
        self.prepare_compact_indexed_only_or_diagnostic_with_parts(
            program,
            source_id,
            allow_checker_only,
            declarations,
            bodies,
        )
    }

    fn prepare_compact_indexed_only_or_diagnostic_with_parts(
        &mut self,
        program: &ArenaProgram,
        source_id: SourceId,
        allow_checker_only: bool,
        declarations: CompactDeclOutput,
        bodies: CompactBodyProbeOutput,
    ) -> Result<CompactIndexedRunPlan, Diagnostic> {
        self.install_compact_runtime_declarations(&declarations);
        let source_id = program.source_text_source_id().unwrap_or(source_id);
        let Some(source) = self.sources.get(source_id).map(|source| source.text()) else {
            return Err(compact_lowerability_diagnostic(
                zero_span(),
                "source text was unavailable while building indexed IR",
                "compact.indexed-source",
            ));
        };
        let indexed = FullBuilder::build_compact_with_options(
            program,
            &declarations,
            &bodies,
            source,
            Arc::clone(&self.sources),
            source_id,
            allow_checker_only,
        )
        .map_err(|error| {
            let span = error
                .location
                .map(|location| {
                    Span::new(
                        source_id,
                        location.start as usize,
                        location.start.saturating_add(location.len) as usize,
                    )
                })
                .unwrap_or_else(zero_span);
            compact_lowerability_diagnostic(
                span,
                &format!("indexed IR could not encode `{}`", error.construct),
                "compact.indexed-build",
            )
        })?;
        let root = program.statement_ids().collect::<Vec<_>>();
        let driver_steps = indexed.driver_step_count().map_err(|error| {
            compact_lowerability_diagnostic(
                root.first()
                    .map(|stmt| program.arena.stmt(*stmt).span)
                    .unwrap_or_else(zero_span),
                &format!("indexed driver verification failed: {}", error.message),
                "compact.indexed-driver",
            )
        })?;
        if driver_steps != root.len() {
            return Err(compact_lowerability_diagnostic(
                root.first()
                    .map(|stmt| program.arena.stmt(*stmt).span)
                    .unwrap_or_else(zero_span),
                "indexed driver statement count did not match the source program",
                "compact.statement-count",
            ));
        }
        let auto_main_required =
            compact_root_proc_main_requires_auto_call_indexed(program, &root, &indexed)?;
        if auto_main_required
            && !indexed.contains_function(
                LoweredFunctionKey::Name(Name::intern("main")),
                LoweredFunctionKind::Proc,
            )
        {
            let span = root
                .iter()
                .copied()
                .find_map(|stmt| compact_root_proc_main_span(program, stmt))
                .unwrap_or_else(zero_span);
            return Err(compact_lowerability_diagnostic(
                span,
                "proc main could not be encoded in indexed IR",
                "compact.unlowered-main",
            ));
        }
        let indexed = Arc::new(indexed);
        self.indexed_program = Some(Arc::clone(&indexed));
        let compact_auto_main_args = if auto_main_required {
            self.compact_auto_main_args().ok_or_else(|| {
                compact_lowerability_diagnostic(
                    zero_span(),
                    "script arguments could not be converted for compact main dispatch",
                    "compact.main-args",
                )
            })?
        } else {
            Vec::new()
        };
        let mut statements = Vec::with_capacity(root.len());
        for (index, stmt) in root.iter().copied().enumerate() {
            let skip_auto_main =
                compact_should_skip_auto_main_stmt(program, &root, index, auto_main_required);
            let encoded = !indexed.driver_step_is_skip(index).map_err(|error| {
                compact_lowerability_diagnostic(
                    program.arena.stmt(stmt).span,
                    &format!("indexed driver verification failed: {}", error.message),
                    "compact.indexed-driver",
                )
            })?;
            if !encoded
                && !skip_auto_main
                && !compact_top_level_stmt_is_skippable(program, stmt, allow_checker_only)
            {
                return Err(compact_lowerability_diagnostic(
                    program.arena.stmt(stmt).span,
                    "top-level statement could not be encoded in indexed IR",
                    "compact.unlowered-statement",
                ));
            }
            statements.push(CompactIndexedDriverStepPlan {
                span: program.arena.stmt(stmt).span,
                skip_auto_main,
            });
        }
        Ok(CompactIndexedRunPlan {
            script_span: statements
                .first()
                .map(|statement| statement.span)
                .unwrap_or_else(zero_span),
            statements,
            auto_main_required,
            compact_auto_main_args,
        })
    }

    pub fn compact_indexed_diagnostics(
        program: &ArenaProgram,
        source_id: SourceId,
        sources: SourceMap,
        argv: Vec<String>,
        command_name: String,
    ) -> Vec<Diagnostic> {
        program.symbol_owner().with_current(|| {
            let mut evaluator = Self::new_with_sources_and_command(argv, sources, command_name);
            match evaluator.prepare_compact_indexed_only_or_diagnostic(program, source_id, false) {
                Ok(_) => Vec::new(),
                Err(diagnostic) => vec![diagnostic],
            }
        })
    }

    pub fn compact_lowerability_diagnostics(
        program: &ArenaProgram,
        source_id: SourceId,
        sources: SourceMap,
        argv: Vec<String>,
        command_name: String,
    ) -> Vec<Diagnostic> {
        program.symbol_owner().with_current(|| {
            let mut evaluator = Self::new_with_sources_and_command(argv, sources, command_name);
            match evaluator.prepare_compact_indexed_only_or_diagnostic(program, source_id, true) {
                Ok(_) => Vec::new(),
                Err(diagnostic) => vec![diagnostic],
            }
        })
    }

    pub fn compact_lowerability_diagnostics_with_parts(
        program: &ArenaProgram,
        source_id: SourceId,
        sources: SourceMap,
        declarations: CompactDeclOutput,
        bodies: CompactBodyProbeOutput,
        argv: Vec<String>,
        command_name: String,
    ) -> Vec<Diagnostic> {
        program.symbol_owner().with_current(|| {
            let mut evaluator = Self::new_with_sources_and_command(argv, sources, command_name);
            if let Some(diagnostic) = declarations.diagnostics.first() {
                return vec![diagnostic.clone()];
            }
            if let Some(diagnostic) = bodies.diagnostics.first() {
                return vec![diagnostic.clone()];
            }
            match evaluator.prepare_compact_indexed_only_or_diagnostic_with_parts(
                program,
                source_id,
                true,
                declarations,
                bodies,
            ) {
                Ok(_) => Vec::new(),
                Err(diagnostic) => vec![diagnostic],
            }
        })
    }

    pub(crate) fn eval_installed_compact_indexed_only(
        self,
        plan: CompactIndexedRunPlan,
    ) -> Result<EvalOutput, Self> {
        let symbols = self
            .indexed_program
            .as_ref()
            .map(|program| program.symbol_owner().clone());
        run_eval(move || match symbols {
            Some(symbols) => {
                symbols.with_current(|| self.try_eval_installed_compact_indexed_only_inner(plan))
            }
            None => self.try_eval_installed_compact_indexed_only_inner(plan),
        })
    }

    fn try_eval_installed_compact_indexed_only_inner(
        mut self,
        plan: CompactIndexedRunPlan,
    ) -> Result<EvalOutput, Self> {
        let statement_count = self
            .indexed_program
            .as_ref()
            .and_then(|indexed| indexed.driver_step_count().ok())
            .unwrap_or(usize::MAX);
        if statement_count != plan.statements.len() {
            let span = plan.script_span;
            let message = "compact lowered statement count did not match the source program";
            let diagnostics = vec![runtime_diagnostic(
                span,
                message,
                "runtime.compact-statement-count",
            )];
            let traceback = Some(self.traceback_for_value(
                span,
                "runtime.error",
                &Value::Error(Box::new(RuntimeError::new(
                    "compact-statement-count",
                    message,
                ))),
            ));
            return Ok(EvalOutput {
                stdout: self.stdout,
                stderr: self.stderr,
                trace_events: self.trace_events,
                diagnostics,
                traceback,
                sources: self.sources,
                status: 0,
                cwd: self.cwd,
                env: self.env.into_snapshot(),
                last_status: self.last_status,
            });
        }
        crate::runtime::process::clear_cancellation_request();
        let script_span = plan.script_span;
        self.trace_enter(
            TraceKind::ScriptEnter,
            Some(script_span),
            Some("script"),
            TracePayload::None,
        );

        let mut traceback = None;
        let mut status = 0;
        let mut abort = None;
        let mut stopped = false;
        let mut last_value = Value::Unit;
        let mut diagnostics = Vec::new();
        let mut compact_indexed_defers = Vec::new();
        for (index, stmt) in plan.statements.iter().enumerate() {
            let span = stmt.span;
            if let Err(error) = self.service_pending_signal(span) {
                let pending_traceback = self.pending_traceback.take();
                diagnostics.push(runtime_diagnostic(
                    error.span.unwrap_or(span),
                    &error.message,
                    "runtime.error",
                ));
                traceback = Some(pending_traceback.unwrap_or_else(|| {
                    self.traceback_for_value(
                        error.span.unwrap_or(span),
                        "signal.hook",
                        &Value::Error(Box::new(error)),
                    )
                }));
                break;
            }
            if self.signal_state.shutdown_complete {
                break;
            }
            if stmt.skip_auto_main {
                continue;
            }
            let indexed = self
                .indexed_program
                .as_ref()
                .expect("verified indexed program remains installed");
            match indexed.driver_step_is_defer(index) {
                Ok(true) => {
                    compact_indexed_defers.push(index);
                    continue;
                }
                Ok(false) => {}
                Err(error) => {
                    let error = RuntimeError::new(
                        "indexed-driver",
                        format!("indexed driver verification failed: {}", error.message),
                    )
                    .with_span(span);
                    diagnostics.push(runtime_diagnostic(
                        span,
                        &error.message,
                        "runtime.indexed-driver",
                    ));
                    traceback = Some(self.traceback_for_value(
                        span,
                        "runtime.error",
                        &Value::Error(Box::new(error)),
                    ));
                    break;
                }
            }
            let evaluated = self
                .eval_indexed_driver_step(index, span)
                .unwrap_or_else(|| {
                    Err(RuntimeError::new(
                        "indexed-driver",
                        "verified indexed driver step has no direct executor",
                    )
                    .with_span(span))
                });
            match evaluated {
                Ok(Some(Flow::Continue(value))) => last_value = value,
                Ok(Some(Flow::Return(value))) => {
                    diagnostics.push(runtime_diagnostic(
                        span,
                        "return outside function",
                        "runtime.return-outside-function",
                    ));
                    traceback = Some(self.traceback_for_value(
                        span,
                        "return",
                        &Value::Error(Box::new(RuntimeError::new(
                            "return-outside-function",
                            format!("unexpected {}", value.type_name()),
                        ))),
                    ));
                    break;
                }
                Ok(Some(Flow::Break(_) | Flow::ContinueLoop)) => {
                    diagnostics.push(runtime_diagnostic(
                        span,
                        "loop control outside loop",
                        "runtime.loop-control",
                    ));
                    traceback = Some(self.traceback_for_value(
                        span,
                        "loop-control",
                        &Value::Error(Box::new(RuntimeError::new(
                            "loop-control",
                            "loop control outside loop",
                        ))),
                    ));
                    break;
                }
                Ok(Some(Flow::Propagate(propagation))) => {
                    if let Some(stop_status) = self.handle_cli_parse_stop(&propagation.error) {
                        status = stop_status;
                        stopped = true;
                    } else {
                        traceback = Some(propagation.traceback);
                    }
                    break;
                }
                Ok(None) => {
                    let message = "statement could not run in the compact runtime";
                    diagnostics.push(runtime_diagnostic(
                        span,
                        message,
                        "runtime.compact-unsupported-statement",
                    ));
                    traceback = Some(self.traceback_for_value(
                        span,
                        "runtime.error",
                        &Value::Error(Box::new(RuntimeError::new(
                            "compact-unsupported-statement",
                            message,
                        ))),
                    ));
                    break;
                }
                Err(error) => {
                    if let Some(signal) = error.abort {
                        abort = Some(signal);
                        status = signal.status;
                        break;
                    }
                    let pending_traceback = self.pending_traceback.take();
                    self.trace_leaf(
                        TraceKind::RuntimeError,
                        Some(error.span.unwrap_or(span)),
                        None,
                        TracePayload::RuntimeError {
                            error: TraceError::new(&error.kind, &error.message),
                        },
                    );
                    diagnostics.push(runtime_diagnostic(
                        error.span.unwrap_or(span),
                        &error.message,
                        "runtime.error",
                    ));
                    traceback = Some(pending_traceback.unwrap_or_else(|| {
                        self.traceback_for_value(
                            error.span.unwrap_or(span),
                            "runtime.error",
                            &Value::Error(Box::new(error)),
                        )
                    }));
                    break;
                }
            }
            if let Err(error) = self.service_pending_signal(span) {
                let pending_traceback = self.pending_traceback.take();
                diagnostics.push(runtime_diagnostic(
                    error.span.unwrap_or(span),
                    &error.message,
                    "runtime.error",
                ));
                traceback = Some(pending_traceback.unwrap_or_else(|| {
                    self.traceback_for_value(
                        error.span.unwrap_or(span),
                        "signal.hook",
                        &Value::Error(Box::new(error)),
                    )
                }));
                break;
            }
            if self.signal_state.shutdown_complete {
                break;
            }
        }

        // Final signal check after the statement loop.  A child process (e.g.
        // `sh -c "kill -USR1 \$PPID"`) may send a signal and exit on CPU A
        // while this process's `waitpid(NOHANG)` returns on CPU B.  The
        // child's `exit` changes its zombie state atomically, so `waitpid`
        // sees it immediately.  But the child's `kill` sets TIF_SIGPENDING on
        // our task_struct — the cross-CPU IPI may not arrive until after
        // `waitpid` returns to userspace.  Signal delivery only happens at
        // the next kernel re-entry.  If the script has no more syscalls
        // (e.g. the last statement was a fast `run`), TIF_SIGPENDING stays
        // unobserved, the handler never writes PRIMARY_SIGNAL, and
        // service_pending_signal never sees the hook signal.
        // yield_now() forces sched_yield, a real syscall, so the kernel
        // checks pending signals before returning and delivers any that
        // raced in after the last poll.  Handle the result inline to
        // propagate shutdown status from the hook's abort(0).
        std::thread::yield_now();
        if let Err(error) = self.service_pending_signal(script_span) {
            diagnostics.push(runtime_diagnostic(
                error.span.unwrap_or(script_span),
                &error.message,
                "runtime.error",
            ));
            traceback = Some(self.pending_traceback.take().unwrap_or_else(|| {
                self.traceback_for_value(
                    error.span.unwrap_or(script_span),
                    "signal.hook",
                    &Value::Error(Box::new(error)),
                )
            }));
        }
        if self.signal_state.shutdown_complete
            && traceback.is_none()
            && abort.is_none()
            && !stopped
            && let Some(shutdown_status) = self.signal_state.shutdown_status
        {
            status = shutdown_status;
        }

        if plan.auto_main_required && traceback.is_none() && abort.is_none() && !stopped {
            let zero = zero_span();
            let call_result = self.call_indexed_direct(
                LoweredFunctionKey::Name(Name::intern("main")),
                LoweredFunctionKind::Proc,
                &plan.compact_auto_main_args,
                zero,
            );
            if let Some(call_result) = call_result {
                match call_result {
                    Ok(Value::Result(ResultValue::Err(error))) => {
                        if let Some(stop_status) = self.handle_cli_parse_stop(error.as_ref()) {
                            status = stop_status;
                            stopped = true;
                            self.pending_traceback = None;
                        } else {
                            traceback = Some(self.pending_traceback.take().unwrap_or_else(|| {
                                self.traceback_for_value(zero, "main", error.as_ref())
                            }));
                        }
                    }
                    Ok(value) => last_value = value,
                    Err(error) => {
                        if let Some(signal) = error.abort {
                            abort = Some(signal);
                            status = signal.status;
                        } else {
                            let pending_traceback = self.pending_traceback.take();
                            diagnostics.push(runtime_diagnostic(
                                error.span.unwrap_or(zero),
                                &error.message,
                                "runtime.error",
                            ));
                            traceback = Some(pending_traceback.unwrap_or_else(|| {
                                self.traceback_for_value(
                                    error.span.unwrap_or(zero),
                                    "runtime.error",
                                    &Value::Error(Box::new(error)),
                                )
                            }));
                        }
                    }
                }
            } else {
                let span = plan.script_span;
                let message = "proc main could not run in the compact runtime";
                diagnostics.push(runtime_diagnostic(
                    span,
                    message,
                    "runtime.compact-unsupported-main",
                ));
                traceback = Some(self.traceback_for_value(
                    span,
                    "main",
                    &Value::Error(Box::new(RuntimeError::new(
                        "compact-unsupported-main",
                        message,
                    ))),
                ));
                stopped = true;
            }
        }

        let cleanup_result = if abort.is_some_and(|signal: AbortSignal| signal.force)
            || self.signal_state.shutdown_force
        {
            Ok(Flow::Continue(Value::Unit))
        } else {
            let mut cleanup = self.cleanup_scope_process_handles(
                self.current_scope_id(),
                Ok(Flow::Continue(Value::Unit)),
            );
            for index in compact_indexed_defers.into_iter().rev() {
                if cleanup.is_err() || matches!(cleanup, Ok(Flow::Propagate(_))) {
                    break;
                }
                cleanup = self
                    .eval_indexed_driver_step(index, script_span)
                    .unwrap_or_else(|| {
                        Err(RuntimeError::new(
                            "indexed-driver",
                            "verified indexed defer has no direct executor",
                        )
                        .with_span(script_span))
                    })
                    .map(|flow| flow.unwrap_or(Flow::Continue(Value::Unit)));
            }
            cleanup
        };
        if traceback.is_none() && abort.is_none() {
            match cleanup_result {
                Ok(Flow::Continue(_)) => {}
                Ok(Flow::Propagate(propagation)) => traceback = Some(propagation.traceback),
                Ok(Flow::Return(_) | Flow::Break(_) | Flow::ContinueLoop) => {
                    diagnostics.push(runtime_diagnostic(
                        script_span,
                        "deferred cleanup produced invalid control flow",
                        "runtime.defer-control-flow",
                    ));
                    traceback = Some(self.traceback_for_value(
                        script_span,
                        "defer",
                        &Value::Error(Box::new(RuntimeError::new(
                            "defer-control-flow",
                            "deferred cleanup produced invalid control flow",
                        ))),
                    ));
                }
                Err(error) => {
                    if let Some(signal) = error.abort {
                        status = signal.status;
                        abort = Some(signal);
                    } else {
                        diagnostics.push(runtime_diagnostic(
                            error.span.unwrap_or(script_span),
                            &error.message,
                            "runtime.error",
                        ));
                        traceback = Some(self.traceback_for_value(
                            error.span.unwrap_or(script_span),
                            "defer",
                            &Value::Error(Box::new(error)),
                        ));
                    }
                }
            }
        }
        if traceback.is_none()
            && abort.is_none()
            && !stopped
            && let Some(shutdown_status) = self.signal_state.shutdown_status
        {
            status = shutdown_status;
        } else if traceback.is_none() && abort.is_none() && !stopped {
            match script_status_from_value(&last_value, script_span) {
                Ok(script_status) => status = script_status,
                Err(error) => {
                    diagnostics.push(runtime_diagnostic(
                        error.span.unwrap_or(script_span),
                        &error.message,
                        "runtime.exit-status",
                    ));
                    traceback = Some(self.traceback_for_value(
                        error.span.unwrap_or(script_span),
                        "script.exit",
                        &Value::Error(Box::new(error)),
                    ));
                }
            }
        }
        self.trace_exit(
            TraceKind::ScriptExit,
            Some(script_span),
            Some("script"),
            TracePayload::None,
        );

        Ok(EvalOutput {
            stdout: self.stdout,
            stderr: self.stderr,
            trace_events: self.trace_events,
            diagnostics,
            traceback,
            sources: self.sources,
            status,
            cwd: self.cwd,
            env: self.env.into_snapshot(),
            last_status: self.last_status,
        })
    }

    fn compact_auto_main_args(&self) -> Option<Vec<Value>> {
        let Value::List(args) = &self.lookup(Name::intern("args"))?.value else {
            return None;
        };
        let mut args = args.clone();
        // CLI args arrive as `Str`; coerce each positional arg to its declared
        // `main` param type where a lossless widening applies (notably
        // `Str` → `Path`, e.g. `proc main(root: Path = p".", …)`). The lowered
        // call binder requires the arg's runtime type to match the param, and
        // CLI strings are the documented way to pass paths into `main`. A rest
        // param's kind is `List`, so it (and any variadic extras past the fixed
        // params) is left as `Str`.
        let main_name = Name::intern("main");
        let indexed_param_kinds = self.indexed_program.as_ref().and_then(|program| {
            program
                .function_param_kinds(
                    LoweredFunctionKey::Name(main_name),
                    LoweredFunctionKind::Proc,
                )
                .ok()
                .flatten()
        });
        if let Some(param_kinds) = indexed_param_kinds {
            for (index, arg) in args.iter_mut().enumerate() {
                if param_kinds.get(index).copied() == Some(LoweredType::Path)
                    && let Value::Str(text) = arg
                    && let Ok(path) = PathValue::from_text(&text)
                {
                    *arg = Value::Path(path);
                }
            }
        }
        Some(args)
    }

    #[cfg(feature = "native-tests")]
    pub fn eval_test(
        self,
        program: &ArenaProgram,
        source_id: SourceId,
        test_name: &str,
        ctx: Value,
    ) -> TestEvalOutput {
        let symbols = program.symbol_owner().clone();
        run_eval(move || {
            symbols.with_current(|| self.eval_test_inner(program, source_id, test_name, ctx))
        })
    }

    #[cfg(feature = "native-tests")]
    pub fn prepare_test_program(
        mut self,
        program: Arc<ArenaProgram>,
        source_id: SourceId,
    ) -> PreparedTestProgram {
        let plan = self
            .prepare_compact_indexed_only(&program, source_id)
            .expect("checked native-test programs must encode as indexed IR");
        let script_span = plan.script_span;
        let shared = self.lowered_shared_state();
        let (diagnostics, traceback) = self.eval_installed_indexed_test_setup(&plan);
        let (setup_shared, setup_failure) = if traceback.is_some() {
            (
                Arc::clone(&shared),
                Some(self.finish_test_output(diagnostics, traceback, None)),
            )
        } else {
            (self.lowered_shared_state(), None)
        };
        PreparedTestProgram {
            plan,
            script_span,
            symbols: program.symbol_owner().clone(),
            shared,
            setup_shared,
            setup_failure,
        }
    }

    #[cfg(feature = "native-tests")]
    fn eval_test_inner(
        mut self,
        program: &ArenaProgram,
        source_id: SourceId,
        test_name: &str,
        ctx: Value,
    ) -> TestEvalOutput {
        let Some(plan) = self.prepare_compact_indexed_only(program, source_id) else {
            let span = program
                .statement_ids()
                .next()
                .map(|stmt| program.arena.stmt(stmt).span)
                .unwrap_or_else(zero_span);
            return self.finish_test_output(
                vec![runtime_diagnostic(
                    span,
                    "native-test program could not be encoded in indexed IR",
                    "runtime.test-setup",
                )],
                None,
                None,
            );
        };
        self.eval_installed_indexed_test_inner(&plan, plan.script_span, test_name, ctx)
    }

    #[cfg(feature = "native-tests")]
    fn eval_installed_indexed_test_inner(
        mut self,
        plan: &CompactIndexedRunPlan,
        script_span: Span,
        test_name: &str,
        ctx: Value,
    ) -> TestEvalOutput {
        self.trace_enter(
            TraceKind::ScriptEnter,
            Some(script_span),
            Some("test"),
            TracePayload::None,
        );

        let mut result = None;
        let (mut diagnostics, mut traceback) = self.eval_installed_indexed_test_setup(plan);

        if traceback.is_none() {
            match self.call_installed_test_proc(script_span, test_name, ctx) {
                Ok(value) => result = value,
                Err((diagnostic, call_traceback, value)) => {
                    diagnostics.push(diagnostic);
                    traceback = Some(call_traceback);
                    result = value;
                }
            }
        }

        self.trace_exit(
            TraceKind::ScriptExit,
            Some(script_span),
            Some("test"),
            TracePayload::None,
        );
        self.finish_test_output(diagnostics, traceback, result)
    }

    #[cfg(feature = "native-tests")]
    fn eval_installed_indexed_test_setup(
        &mut self,
        plan: &CompactIndexedRunPlan,
    ) -> (Vec<Diagnostic>, Option<Traceback>) {
        let mut diagnostics = Vec::new();
        let mut traceback = None;
        for (index, statement) in plan.statements.iter().enumerate() {
            let span = statement.span;
            let Some(program) = self.indexed_program.as_ref() else {
                diagnostics.push(runtime_diagnostic(
                    span,
                    "indexed native-test program is not installed",
                    "runtime.test-setup",
                ));
                break;
            };
            if program.driver_step_is_skip(index).unwrap_or(false)
                || program.driver_step_is_defer(index).unwrap_or(false)
            {
                continue;
            }
            match self.eval_indexed_driver_step(index, span) {
                Some(Ok(Some(Flow::Continue(_)) | None)) => {}
                Some(Ok(Some(Flow::Propagate(propagation)))) => {
                    traceback = Some(propagation.traceback);
                    break;
                }
                Some(Ok(Some(Flow::Return(_) | Flow::Break(_) | Flow::ContinueLoop))) => {
                    diagnostics.push(runtime_diagnostic(
                        span,
                        "invalid top-level control flow in test setup",
                        "runtime.test-setup",
                    ));
                    traceback = Some(self.traceback_for_value(
                        span,
                        "test.setup",
                        &Value::Error(Box::new(RuntimeError::new(
                            "test-setup",
                            "invalid top-level control flow in test setup",
                        ))),
                    ));
                    break;
                }
                Some(Err(error)) => {
                    let pending_traceback = self.pending_traceback.take();
                    self.trace_leaf(
                        TraceKind::RuntimeError,
                        Some(error.span.unwrap_or(span)),
                        None,
                        TracePayload::RuntimeError {
                            error: TraceError::new(&error.kind, &error.message),
                        },
                    );
                    diagnostics.push(runtime_diagnostic(
                        error.span.unwrap_or(span),
                        &error.message,
                        "runtime.error",
                    ));
                    traceback = Some(pending_traceback.unwrap_or_else(|| {
                        self.traceback_for_value(
                            error.span.unwrap_or(span),
                            "test.setup",
                            &Value::Error(Box::new(error)),
                        )
                    }));
                    break;
                }
                None => {
                    diagnostics.push(runtime_diagnostic(
                        span,
                        "indexed native-test driver step is not available",
                        "runtime.test-setup",
                    ));
                    break;
                }
            }
        }
        (diagnostics, traceback)
    }

    #[cfg(feature = "native-tests")]
    fn eval_installed_test_call_inner(
        mut self,
        script_span: Span,
        test_name: &str,
        ctx: Value,
    ) -> TestEvalOutput {
        self.trace_enter(
            TraceKind::ScriptEnter,
            Some(script_span),
            Some("test"),
            TracePayload::None,
        );
        let mut diagnostics = Vec::new();
        let mut traceback = None;
        let result = match self.call_installed_test_proc(script_span, test_name, ctx) {
            Ok(value) => value,
            Err((diagnostic, call_traceback, value)) => {
                diagnostics.push(diagnostic);
                traceback = Some(call_traceback);
                value
            }
        };
        self.trace_exit(
            TraceKind::ScriptExit,
            Some(script_span),
            Some("test"),
            TracePayload::None,
        );
        self.finish_test_output(diagnostics, traceback, result)
    }

    #[cfg(feature = "native-tests")]
    fn call_installed_test_proc(
        &mut self,
        script_span: Span,
        test_name: &str,
        ctx: Value,
    ) -> Result<Option<Value>, (Diagnostic, Traceback, Option<Value>)> {
        let name = Name::intern(test_name);
        let key = LoweredFunctionKey::Name(name);
        let args = match self
            .indexed_program
            .as_ref()
            .and_then(|program| {
                program
                    .function_view(key, LoweredFunctionKind::Proc)
                    .ok()
                    .flatten()
            })
            .and_then(|view| view.header().ok())
        {
            Some(header) if header.params.is_empty() => Vec::new(),
            Some(_) => vec![ctx],
            None => Vec::new(),
        };
        match self.call_indexed_direct(key, LoweredFunctionKind::Proc, &args, script_span) {
            Some(Ok(value)) => Ok(Some(value)),
            Some(Err(error)) => {
                let span = error.span.unwrap_or(script_span);
                let pending_traceback = self.pending_traceback.take();
                self.trace_leaf(
                    TraceKind::RuntimeError,
                    Some(span),
                    None,
                    TracePayload::RuntimeError {
                        error: TraceError::new(&error.kind, &error.message),
                    },
                );
                let diagnostic = runtime_diagnostic(span, &error.message, "runtime.error");
                let traceback = pending_traceback.unwrap_or_else(|| {
                    self.traceback_for_value(span, "test.call", &Value::Error(Box::new(error)))
                });
                Err((diagnostic, traceback, None))
            }
            None => {
                let diagnostic = runtime_diagnostic(
                    script_span,
                    "test proc was not found",
                    "runtime.test-missing",
                );
                let traceback = self.traceback_for_value(
                    script_span,
                    "test.call",
                    &Value::Error(Box::new(RuntimeError::new("test-missing", test_name))),
                );
                let result = Some(Value::err(Value::Error(Box::new(RuntimeError::new(
                    "test-missing",
                    test_name,
                )))));
                Err((diagnostic, traceback, result))
            }
        }
    }

    #[cfg(feature = "native-tests")]
    fn finish_test_output(
        self,
        diagnostics: Vec<Diagnostic>,
        traceback: Option<Traceback>,
        result: Option<Value>,
    ) -> TestEvalOutput {
        let status = if traceback.is_some() { 3 } else { 0 };
        TestEvalOutput {
            output: EvalOutput {
                stdout: self.stdout,
                stderr: self.stderr,
                trace_events: self.trace_events,
                diagnostics,
                traceback,
                sources: self.sources,
                status,
                cwd: self.cwd,
                env: self.env.into_snapshot(),
                last_status: self.last_status,
            },
            result,
        }
    }

    fn install_compact_runtime_declarations(&mut self, declarations: &CompactDeclOutput) {
        self.tag_variants
            .extend(compact_runtime_tag_arities(declarations));
        let (error_families, _, _, _) = compact_runtime_error_families(declarations);
        self.error_families.extend(error_families);
    }
}

impl Evaluator {
    fn question_flow(&mut self, value: Value, span: Span) -> Flow {
        match value {
            Value::Result(ResultValue::Ok(value)) => Flow::Continue(*value),
            Value::Result(ResultValue::Err(error)) => {
                let error = *error;
                let kind = error.error_kind().unwrap_or("error").to_string();
                let message = error
                    .error_message()
                    .unwrap_or("propagated error")
                    .to_string();
                self.trace_leaf(
                    TraceKind::ResultPropagate,
                    Some(span),
                    None,
                    TracePayload::ResultPropagate {
                        error_kind: kind.clone(),
                    },
                );
                let traceback = self.pending_traceback.take().unwrap_or_else(|| Traceback {
                    failing_span: Some(span),
                    exe_path: self.exe_path_for_traceback(),
                    operation_kind: "result.propagate".to_string(),
                    error: TraceError {
                        kind: kind.clone(),
                        message: message.clone(),
                    },
                    frames: self.call_stack.clone(),
                });
                Flow::Propagate(Propagation { error, traceback })
            }
            other => Flow::Propagate(Propagation {
                error: Value::Error(Box::new(
                    RuntimeError::new(
                        "type-error",
                        format!("`?` expected Result, found {}", other.type_name()),
                    )
                    .with_span(span),
                )),
                traceback: Traceback {
                    failing_span: Some(span),
                    exe_path: self.exe_path_for_traceback(),
                    operation_kind: "result.propagate".to_string(),
                    error: TraceError::new("type-error", "`?` expected Result"),
                    frames: self.call_stack.clone(),
                },
            }),
        }
    }

    fn exe_path_for_traceback(&self) -> String {
        self.exe_path.clone()
    }

    fn traceback_for_value(&self, span: Span, operation: &str, value: &Value) -> Traceback {
        Traceback {
            failing_span: Some(span),
            exe_path: self.exe_path_for_traceback(),
            operation_kind: operation.to_string(),
            error: TraceError::new(
                value.error_kind().unwrap_or("runtime-error"),
                value.error_message().unwrap_or("runtime error"),
            ),
            frames: self.call_stack.clone(),
        }
    }

    fn handle_cli_parse_stop(&mut self, value: &Value) -> Option<u8> {
        let Value::Error(error) = value else {
            return None;
        };
        if error.kind == "cli-help" {
            self.write_stdout_line(&error.message);
            return Some(0);
        }
        if error.kind == "cli-parse"
            && matches!(error.payload.get("cli_usage"), Some(Value::Bool(true)))
        {
            self.write_stderr_line(&error.message);
            return Some(2);
        }
        None
    }

    fn trace_enter(
        &mut self,
        kind: TraceKind,
        span: Option<Span>,
        name: Option<&str>,
        payload: TracePayload,
    ) {
        if !self.trace_enabled {
            return;
        }
        let event_id = next_event_id(&self.trace_events);
        let start_time_us = trace_epoch_us();
        let started_at = Instant::now();
        let event = self
            .make_trace_event(
                event_id,
                kind,
                span,
                name,
                payload,
                TraceNesting {
                    parent_event_id: self.event_stack.last().map(|frame| frame.event_id),
                    depth: self.event_stack.len() as u32,
                },
            )
            .with_timing(TraceTiming::new(Some(start_time_us), None));
        self.event_stack.push(TraceFrame {
            event_id,
            start_time_us,
            started_at,
        });
        self.trace_events.push(event);
    }

    fn trace_exit(
        &mut self,
        kind: TraceKind,
        span: Option<Span>,
        name: Option<&str>,
        payload: TracePayload,
    ) {
        if !self.trace_enabled {
            return;
        }
        let frame = self.event_stack.pop();
        let event_id = next_event_id(&self.trace_events);
        let (parent_event_id, timing) = frame.map_or_else(
            || (None, TraceTiming::new(Some(trace_epoch_us()), None)),
            |frame| {
                (
                    Some(frame.event_id),
                    TraceTiming::new(
                        Some(frame.start_time_us),
                        Some(elapsed_micros(frame.started_at)),
                    ),
                )
            },
        );
        let event = self
            .make_trace_event(
                event_id,
                kind,
                span,
                name,
                payload,
                TraceNesting {
                    parent_event_id,
                    depth: self.event_stack.len() as u32,
                },
            )
            .with_timing(timing);
        self.trace_events.push(event);
    }

    pub(super) fn trace_leaf(
        &mut self,
        kind: TraceKind,
        span: Option<Span>,
        name: Option<&str>,
        payload: TracePayload,
    ) {
        if !self.trace_enabled {
            return;
        }
        let event_id = next_event_id(&self.trace_events);
        let event = self
            .make_trace_event(
                event_id,
                kind,
                span,
                name,
                payload,
                TraceNesting {
                    parent_event_id: self.event_stack.last().map(|frame| frame.event_id),
                    depth: self.event_stack.len() as u32,
                },
            )
            .with_timing(TraceTiming::new(Some(trace_epoch_us()), None));
        self.trace_events.push(event);
    }

    fn make_trace_event(
        &self,
        event_id: u64,
        kind: TraceKind,
        span: Option<Span>,
        name: Option<&str>,
        payload: TracePayload,
        nesting: TraceNesting,
    ) -> TraceEvent {
        let mut event = TraceEvent::new(event_id, kind);
        event.depth = nesting.depth;
        event.parent_event_id = nesting.parent_event_id;
        if let Some(span) = span {
            event = event.with_span(span);
        }
        if let Some(name) = name {
            event = event.with_name(name);
        }
        if let Some(api_id) = trace_api_id(kind, name, &payload) {
            event = event.with_api_id(api_id);
        }
        event.with_payload(payload)
    }

    fn lookup<N: Into<Name>>(&self, name: N) -> Option<&Binding> {
        let name = name.into();
        self.scopes.iter().rev().find_map(|scope| scope.get(&name))
    }

    fn define<N: Into<Name>>(&mut self, name: N, binding: Binding) {
        self.scopes
            .last_mut()
            .expect("evaluator has a scope")
            .insert(name.into(), binding);
    }

    fn assign(&mut self, name: &str, value: Value, span: Span) -> Result<(), RuntimeError> {
        let interned = Name::intern(name);
        for index in (0..self.scopes.len()).rev() {
            if self.scopes[index].contains_key(&interned) {
                let target_scope = self.scope_ids[index];
                let source_scope = self.current_scope_id();
                self.transfer_process_handles_in_value(&value, source_scope, target_scope);
                let binding = self.scopes[index]
                    .get_mut(&interned)
                    .expect("binding existence checked");
                if !binding.mutable {
                    return Err(RuntimeError::new(
                        "immutable-binding",
                        "cannot assign to immutable binding",
                    )
                    .with_span(span));
                }
                binding.value = value;
                return Ok(());
            }
        }
        Err(RuntimeError::new("unresolved-name", name).with_span(span))
    }

    fn current_scope_id(&self) -> u64 {
        *self.scope_ids.last().expect("evaluator has a scope id")
    }

    fn transfer_process_handles_in_value(
        &mut self,
        value: &Value,
        source_scope: u64,
        target_scope: u64,
    ) {
        if source_scope == target_scope {
            return;
        }
        match value {
            Value::ProcessHandle(handle) => {
                if let Some(live) = self.process_handles.get_mut(&handle.id)
                    && live.owner_scope == source_scope
                {
                    live.owner_scope = target_scope;
                }
            }
            Value::List(values) => {
                for value in values {
                    self.transfer_process_handles_in_value(value, source_scope, target_scope);
                }
            }
            Value::Map(values) => {
                for value in values.values() {
                    self.transfer_process_handles_in_value(value, source_scope, target_scope);
                }
            }
            Value::Record(fields) | Value::Module(fields) => {
                for (_, value) in fields {
                    self.transfer_process_handles_in_value(value, source_scope, target_scope);
                }
            }
            Value::Result(ResultValue::Ok(value)) | Value::Result(ResultValue::Err(value)) => {
                self.transfer_process_handles_in_value(value, source_scope, target_scope);
            }
            Value::Tag { fields, .. } => {
                for value in fields {
                    self.transfer_process_handles_in_value(value, source_scope, target_scope);
                }
            }
            _ => {}
        }
    }

    fn cleanup_scope_process_handles(
        &mut self,
        scope_id: u64,
        primary: Result<Flow, RuntimeError>,
    ) -> Result<Flow, RuntimeError> {
        if self.signal_state.shutdown_force {
            return primary;
        }
        let mut primary_failed = matches!(primary, Err(_) | Ok(Flow::Propagate(_)));
        let mut result = primary;
        let ids = self
            .process_handles
            .iter()
            .filter_map(|(id, live)| (live.owner_scope == scope_id).then_some(*id))
            .collect::<Vec<_>>();
        for id in ids {
            let Some(live) = self.process_handles.remove(&id) else {
                continue;
            };
            let cleanup = if live.child.detached {
                release_to_reaper(live.child);
                Ok(())
            } else {
                cancel_managed(live.child, libc::SIGTERM, Duration::from_millis(150)).map(|_| ())
            };
            if let Err(error) = cleanup
                && !primary_failed
            {
                result = Err(RuntimeError::new(error.kind, error.message).with_span(live.span));
                primary_failed = true;
            }
        }
        result
    }

    fn cancel_process_handles_for_signal(
        &mut self,
        signal: i32,
        span: Span,
    ) -> Result<(), RuntimeError> {
        let ids = self.process_handles.keys().copied().collect::<Vec<_>>();
        let mut first_error = None;
        for id in ids {
            let Some(live) = self.process_handles.remove(&id) else {
                continue;
            };
            let cleanup = if live.child.detached {
                release_to_reaper(live.child);
                Ok(())
            } else {
                cancel_managed(live.child, signal, Duration::from_millis(150)).map(|_| ())
            };
            if let Err(error) = cleanup
                && first_error.is_none()
            {
                first_error = Some(RuntimeError::new(error.kind, error.message).with_span(span));
            }
        }
        if let Some(error) = first_error {
            Err(error)
        } else {
            Ok(())
        }
    }
}

impl CancellationPolicy for Evaluator {
    fn check_process_group(&mut self, group: ProcessGroup) -> CancellationDecision {
        self.track_process_group(group);
        let snapshot = signal_snapshot();
        if let Some(escalation) = snapshot.escalation {
            let primary = hook_signal_from_number(snapshot.primary.unwrap_or(escalation));
            let escalation = hook_signal_from_number(escalation);
            self.trace_signal_escalate(
                &primary,
                &escalation,
                self.signal_state.hook_span.unwrap_or_else(zero_span),
            );
            self.kill_active_process_groups();
            return CancellationDecision::Escalate(primary.number);
        }
        let Some(primary_number) = snapshot.primary else {
            return CancellationDecision::Continue;
        };
        let primary = hook_signal_from_number(primary_number);
        if self.signal_state.hook_running {
            if let Some(deadline) = self.signal_state.pre_cancel_deadline
                && Instant::now() >= deadline
            {
                self.forward_primary_to_active(
                    &primary,
                    self.signal_state.hook_span.unwrap_or_else(zero_span),
                );
            }
            return CancellationDecision::Continue;
        }
        if self.signal_hooks.contains_key(&primary.name) {
            if let Err(error) = self.service_pending_signal(zero_span()) {
                self.pending_traceback = Some(self.traceback_for_value(
                    error.span.unwrap_or_else(zero_span),
                    "signal.hook",
                    &Value::Error(Box::new(error.clone())),
                ));
            }
            if self.signal_state.primary_forwarded {
                return CancellationDecision::Forward(primary.number);
            }
            return CancellationDecision::Continue;
        }
        CancellationDecision::Forward(primary.number)
    }

    fn process_group_finished(&mut self, group: ProcessGroup) {
        self.untrack_process_group(group);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Flow {
    Continue(Value),
    Return(Value),
    Break(Option<Value>),
    ContinueLoop,
    Propagate(Propagation),
}

#[derive(Clone, Copy)]
struct TraceNesting {
    parent_event_id: Option<u64>,
    depth: u32,
}

#[derive(Clone, Copy)]
struct TraceFrame {
    event_id: u64,
    start_time_us: u64,
    started_at: Instant,
}

fn trace_api_id(kind: TraceKind, name: Option<&str>, payload: &TracePayload) -> Option<String> {
    match kind {
        TraceKind::CoreCall | TraceKind::CoreResult => name.map(|name| format!("core.{name}")),
        TraceKind::ModuleCall | TraceKind::ModuleResult => {
            name.map(|name| format!("module.{name}"))
        }
        TraceKind::MethodCall | TraceKind::MethodResult => {
            name.map(|name| format!("method.{name}"))
        }
        TraceKind::RunStart | TraceKind::RunEnd => Some("run".to_string()),
        TraceKind::SpawnStart | TraceKind::SpawnReady => Some("spawn".to_string()),
        TraceKind::WaitStart | TraceKind::WaitEnd => Some("wait".to_string()),
        TraceKind::SpawnCancel => Some("spawn.cancel".to_string()),
        TraceKind::PipelineEnter
        | TraceKind::PipelineExit
        | TraceKind::PipelineSegmentStart
        | TraceKind::PipelineSegmentEnd => Some("run.pipeline".to_string()),
        TraceKind::StreamStageEnter | TraceKind::StreamStageExit => match payload {
            TracePayload::StreamStage { stage, .. } => Some(format!("stream.{stage}")),
            _ => name.map(|name| format!("stream.{name}")),
        },
        TraceKind::RetryAttempt => Some("retry.attempt".to_string()),
        TraceKind::CwdEnter | TraceKind::CwdExit => Some("core.cd".to_string()),
        _ => None,
    }
}

fn default_signal_status(signal: &HookSignal) -> u8 {
    if matches!(signal.number, libc::SIGINT | libc::SIGTERM) {
        3
    } else {
        (128 + signal.number).clamp(0, 255) as u8
    }
}

fn signal_hook_error(result: &Result<Flow, RuntimeError>) -> Option<TraceError> {
    match result {
        Err(error) if error.abort.is_none() => Some(TraceError::new(&error.kind, &error.message)),
        Ok(Flow::Continue(Value::Result(ResultValue::Err(error)))) => Some(TraceError::new(
            error.error_kind().unwrap_or("runtime-error"),
            error.error_message().unwrap_or("runtime error"),
        )),
        Ok(Flow::Propagate(propagation)) => Some(TraceError::new(
            propagation.error.error_kind().unwrap_or("runtime-error"),
            propagation.error.error_message().unwrap_or("runtime error"),
        )),
        Ok(Flow::Return(_) | Flow::Break(_) | Flow::ContinueLoop) => {
            Some(TraceError::new("signal-hook", "invalid control flow"))
        }
        _ => None,
    }
}

pub fn apply_question(
    value: Value,
    question_span: Span,
    exe_path: String,
    frames: Vec<TracebackFrame>,
    trace_events: &mut Vec<TraceEvent>,
) -> EvalFlow {
    match value {
        Value::Result(ResultValue::Ok(value)) => EvalFlow::Value(*value),
        Value::Result(ResultValue::Err(error)) => {
            let error = *error;
            let kind = error.error_kind().unwrap_or("error").to_string();
            let message = error
                .error_message()
                .unwrap_or("propagated error")
                .to_string();
            trace_events.push(
                TraceEvent::new(next_event_id(trace_events), TraceKind::ResultPropagate)
                    .with_span(question_span)
                    .with_timing(TraceTiming::new(Some(trace_epoch_us()), None))
                    .with_payload(TracePayload::ResultPropagate {
                        error_kind: kind.clone(),
                    }),
            );
            EvalFlow::Propagate(Propagation {
                error,
                traceback: Traceback {
                    failing_span: Some(question_span),
                    exe_path,
                    operation_kind: "result.propagate".to_string(),
                    error: TraceError { kind, message },
                    frames,
                },
            })
        }
        other => EvalFlow::Propagate(Propagation {
            error: Value::Error(Box::new(
                RuntimeError::new(
                    "type-error",
                    format!("`?` expected Result, found {}", other.type_name()),
                )
                .with_span(question_span),
            )),
            traceback: Traceback {
                failing_span: Some(question_span),
                exe_path,
                operation_kind: "result.propagate".to_string(),
                error: TraceError::new("type-error", "`?` expected Result"),
                frames,
            },
        }),
    }
}

#[cfg(feature = "native-tests")]
fn display_value(value: &Value, span: Span) -> Result<String, RuntimeError> {
    match value {
        Value::Str(value) => Ok(value.to_string()),
        Value::Int(value) => Ok(value.to_string()),
        Value::Float(value) => Ok(value.format()),
        Value::Duration(value) => Ok(format_duration(value.millis)),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Path(path) => Ok(path.display()),
        value => Err(RuntimeError::new(
            "display-conversion",
            format!("cannot display {}", value.type_name()),
        )
        .with_span(span)),
    }
}

fn format_duration(millis: u64) -> String {
    if millis.is_multiple_of(3_600_000) {
        format!("{}h", millis / 3_600_000)
    } else if millis.is_multiple_of(60_000) {
        format!("{}m", millis / 60_000)
    } else if millis.is_multiple_of(1_000) {
        format!("{}s", millis / 1_000)
    } else {
        format!("{millis}ms")
    }
}

fn script_status_from_value(value: &Value, span: Span) -> Result<u8, RuntimeError> {
    match value {
        Value::Unit => Ok(0),
        Value::Int(status) => exit_status(*status, span),
        _ => Ok(0),
    }
}

fn exit_status(status: i64, span: Span) -> Result<u8, RuntimeError> {
    u8::try_from(status).map_err(|_| {
        RuntimeError::new(
            "exit-status",
            "script exit status must be an integer from 0 to 255",
        )
        .with_span(span)
    })
}

fn trace_epoch_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_micros().min(u64::MAX as u128) as u64)
        .unwrap_or(0)
}

fn elapsed_micros(started_at: Instant) -> u64 {
    started_at.elapsed().as_micros().min(u64::MAX as u128) as u64
}

fn add_error_context(error: Value, context: ErrorContext) -> Value {
    let suffix = context.message.as_ref().map_or_else(
        || context.kind.clone(),
        |message| format!("{}: {message}", context.kind),
    );
    match error {
        Value::Error(mut error) => {
            if error.message.is_empty() {
                error.message = suffix;
            } else {
                error.message = format!("{} ({suffix})", error.message);
            }
            error.contexts.push(context);
            Value::Error(error)
        }
        Value::RunError(mut error) => {
            if error.message.is_empty() {
                error.message = suffix;
            } else {
                error.message = format!("{} ({suffix})", error.message);
            }
            error.contexts.push(context);
            Value::RunError(error)
        }
        other => other,
    }
}

fn trace_status(status: &ProcessStatus) -> TraceStatus {
    TraceStatus {
        success: status.success,
        kind: match status.kind {
            ProcessStatusKind::Exit => TraceStatusKind::Exit,
            ProcessStatusKind::Signal => TraceStatusKind::Signal,
            ProcessStatusKind::Exec => TraceStatusKind::Exec,
        },
        code: status.code,
    }
}

fn trace_env_overlay(env: &BTreeMap<Vec<u8>, Vec<u8>>) -> Vec<TraceEnv> {
    env.iter()
        .map(|(name, value)| TraceEnv {
            name: TraceArg::bytes(name.clone()),
            value: TraceArg::bytes(value.clone()),
        })
        .collect()
}

pub(crate) use crate::runtime::text_bytes::{
    contains_bytes as bytes_contains, find_bytes as bytes_find,
};

fn trace_segment_status(status: &ProcessSegmentStatus) -> TraceStatus {
    TraceStatus {
        success: status.success,
        kind: match status.kind {
            ProcessSegmentStatusKind::Exit => TraceStatusKind::Exit,
            ProcessSegmentStatusKind::Signal => TraceStatusKind::Signal,
            ProcessSegmentStatusKind::Exec => TraceStatusKind::Exec,
        },
        code: status.code,
    }
}

fn module_error(kind: &str, message: &str, span: Span) -> Value {
    Value::err(Value::Error(Box::new(
        RuntimeError::new(kind, message).with_span(span),
    )))
}

fn module_io_error(kind: &str, error: std::io::Error, span: Span) -> Value {
    module_error(kind, &error.to_string(), span)
}

fn runtime_error_from_value(value: Value, span: Span) -> RuntimeError {
    match value {
        Value::Error(mut error) => {
            if error.span.is_none() {
                error.span = Some(span);
            }
            *error
        }
        Value::RunError(error) => {
            let variant = error.variant_name().to_string();
            let symbols = crate::symbol::SymbolOwner::current().unwrap_or_default();
            let variant_name = symbols.intern(&variant);
            let facets = error.facets();
            RuntimeError {
                family: "ProcessError".to_string(),
                variant,
                kind: error.kind,
                message: error.message,
                payload: RecordMap::new(),
                facets,
                span: error.span.or(Some(span)),
                contexts: error.contexts,
                abort: None,
                family_name: Name::PROCESS_ERROR,
                variant_name,
                _symbols: symbols,
            }
        }
        value => RuntimeError::new(
            "stream-stage-error",
            format!("stream stage failed with {}", value.type_name()),
        )
        .with_span(span),
    }
}

fn pathbuf_from_path_value(path: &PathValue) -> PathBuf {
    PathBuf::from(OsString::from_vec(path.bytes.clone()))
}

impl Evaluator {
    /// Look up a captured signature for a function exported by a dynamically
    /// loaded module. Module exports are always qualified by the module key.
    pub(super) fn lookup_module_export_signature(
        &self,
        function: crate::runtime::value::FunctionName,
    ) -> Option<&ModuleExportSignature> {
        self.module_export_signatures.get(&function)
    }

    /// Record a module export's signature (from the compact declaration probe)
    /// so contract validation can compare against it later.
    pub(super) fn record_module_export_signature(
        &mut self,
        function: crate::runtime::value::FunctionName,
        pure: bool,
        sig: &crate::sema::check::CompactFunctionSig,
    ) {
        let sig = CallableType {
            params: sig.params.clone(),
            return_ty: Box::new(sig.return_ty.clone()),
            effects: sig.effects.clone(),
        };
        Arc::make_mut(&mut self.module_export_signatures)
            .insert(function, ModuleExportSignature { pure, sig });
    }

    /// Resolve a `PathValue` to a host filesystem path, anchoring relative
    /// paths at the evaluator's current working directory.
    pub(super) fn host_path(&self, path: &PathValue) -> PathBuf {
        let path = pathbuf_from_path_value(path);
        if path.is_absolute() {
            path
        } else {
            self.cwd.join(path)
        }
    }
}

fn path_parent(path: &PathValue) -> Result<PathValue, RuntimeError> {
    let pathbuf = pathbuf_from_path_value(path);
    let parent = pathbuf.parent().unwrap_or_else(|| {
        if path.bytes.starts_with(b"/") {
            std::path::Path::new("/")
        } else {
            std::path::Path::new(".")
        }
    });
    if parent.as_os_str().is_empty() {
        PathValue::from_text(".")
    } else {
        path_value_from_pathbuf(parent.to_path_buf())
    }
}

fn path_text_field(path: &PathValue, name: &str) -> Result<String, RuntimeError> {
    let pathbuf = pathbuf_from_path_value(path);
    Ok(match name {
        "name" => pathbuf
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        "ext" => pathbuf
            .extension()
            .map(|extension| extension.to_string_lossy().into_owned())
            .unwrap_or_default(),
        _ => unreachable!("path text field"),
    })
}

fn path_with_ext(path: &PathValue, ext: &str) -> Result<PathValue, RuntimeError> {
    let mut pathbuf = pathbuf_from_path_value(path);
    pathbuf.set_extension(ext);
    path_value_from_pathbuf(pathbuf)
}

fn initial_env() -> BTreeMap<Vec<u8>, Vec<u8>> {
    let mut env = BTreeMap::new();
    for (name, value) in std::env::vars_os() {
        env.insert(
            name.as_os_str().as_bytes().to_vec(),
            value.as_os_str().as_bytes().to_vec(),
        );
    }
    env
}

fn check_env_name(name: &str, span: Span) -> Result<(), RuntimeError> {
    if valid_env_name(name) {
        Ok(())
    } else {
        Err(RuntimeError::new("env-name", "environment names must be identifiers").with_span(span))
    }
}

fn valid_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn path_value_from_pathbuf(path: PathBuf) -> Result<PathValue, RuntimeError> {
    PathValue::new(path_bytes(&path))
}

fn path_absolute_value(cwd: &std::path::Path, path: &PathValue) -> Result<PathValue, RuntimeError> {
    if path.bytes.starts_with(b"/") {
        return normalize_path_value(path);
    }
    path_value_from_pathbuf(cwd.to_path_buf())?
        .join_path(path)
        .and_then(|joined| normalize_path_value(&joined))
}

fn normalize_path_value(path: &PathValue) -> Result<PathValue, RuntimeError> {
    let absolute = path.bytes.starts_with(b"/");
    let mut parts: Vec<Vec<u8>> = Vec::new();
    for part in path.bytes.split(|byte| *byte == b'/') {
        match part {
            b"" | b"." => {}
            b".." => {
                if parts.last().is_some_and(|part| part != b"..") {
                    parts.pop();
                } else if !absolute {
                    parts.push(part.to_vec());
                }
            }
            _ => parts.push(part.to_vec()),
        }
    }

    let mut bytes = Vec::new();
    if absolute {
        bytes.push(b'/');
    }
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            bytes.push(b'/');
        }
        bytes.extend_from_slice(part);
    }
    if bytes.is_empty() {
        bytes.extend_from_slice(b".");
    }
    PathValue::new(bytes)
}

fn splice_to_argv(value: Value, span: Span) -> Result<Vec<Vec<u8>>, RuntimeError> {
    match value {
        Value::List(items) => items
            .into_iter()
            .map(|item| value_to_argv_bytes(item, span))
            .collect(),
        value => Err(RuntimeError::new(
            "splice-target",
            format!("`@` expected List, found {}", value.type_name()),
        )
        .with_span(span)),
    }
}

fn value_to_argv_bytes(value: Value, span: Span) -> Result<Vec<u8>, RuntimeError> {
    let bytes = match value {
        Value::Str(value) => value.to_string().into_bytes(),
        Value::Path(path) => path.bytes,
        Value::Int(value) => value.to_string().into_bytes(),
        Value::Duration(value) => format_duration(value.millis).into_bytes(),
        Value::Bool(value) => value.to_string().into_bytes(),
        Value::Bytes(_) => {
            return Err(RuntimeError::new(
                "argv-conversion",
                "Bytes cannot convert to argv without explicit encoding",
            )
            .with_span(span));
        }
        Value::List(_) => {
            return Err(RuntimeError::new(
                "argv-conversion",
                "List cannot convert to argv without `@`",
            )
            .with_span(span));
        }
        value => {
            return Err(RuntimeError::new(
                "argv-conversion",
                format!("{} cannot convert to argv", value.type_name()),
            )
            .with_span(span));
        }
    };
    reject_nul(
        &bytes,
        "nul-argv",
        "argv items cannot contain NUL bytes",
        span,
    )?;
    Ok(bytes)
}

fn reject_nul(bytes: &[u8], kind: &str, message: &str, span: Span) -> Result<(), RuntimeError> {
    if bytes.contains(&0) {
        Err(RuntimeError::new(kind, message).with_span(span))
    } else {
        Ok(())
    }
}

fn compound_assignment_value(
    op: AssignOp,
    left: Value,
    right: Value,
    span: Span,
) -> Result<Value, RuntimeError> {
    match (op, left, right) {
        (AssignOp::Add, Value::Int(left), Value::Int(right)) => Ok(Value::Int(left + right)),
        (AssignOp::Sub, Value::Int(left), Value::Int(right)) => Ok(Value::Int(left - right)),
        (AssignOp::Mul, Value::Int(left), Value::Int(right)) => Ok(Value::Int(left * right)),
        (AssignOp::Div, Value::Int(left), Value::Int(right)) => Ok(Value::Int(left / right)),
        (AssignOp::Rem, Value::Int(left), Value::Int(right)) => Ok(Value::Int(left % right)),
        (AssignOp::Add, Value::Float(left), Value::Float(right)) => Ok(Value::Float(
            crate::runtime::value::FloatValue::new(left.0 + right.0),
        )),
        (AssignOp::Sub, Value::Float(left), Value::Float(right)) => Ok(Value::Float(
            crate::runtime::value::FloatValue::new(left.0 - right.0),
        )),
        (AssignOp::Mul, Value::Float(left), Value::Float(right)) => Ok(Value::Float(
            crate::runtime::value::FloatValue::new(left.0 * right.0),
        )),
        (AssignOp::Div, Value::Float(left), Value::Float(right)) => Ok(Value::Float(
            crate::runtime::value::FloatValue::new(left.0 / right.0),
        )),
        (op, left, right) => Err(RuntimeError::new(
            "type-error",
            format!(
                "{} assignment cannot combine {} and {}",
                assign_op_runtime_text(op),
                left.type_name(),
                right.type_name()
            ),
        )
        .with_span(span)),
    }
}

fn assign_op_runtime_text(op: AssignOp) -> &'static str {
    match op {
        AssignOp::Set => "=",
        AssignOp::Add => "+=",
        AssignOp::Sub => "-=",
        AssignOp::Mul => "*=",
        AssignOp::Div => "/=",
        AssignOp::Rem => "%=",
    }
}

fn expand_glob_pattern(
    cwd: &std::path::Path,
    pattern: &str,
    span: Span,
) -> Result<Vec<Vec<u8>>, RuntimeError> {
    if pattern.contains('\0') {
        return Err(
            RuntimeError::new("glob-pattern", "glob patterns cannot contain NUL").with_span(span),
        );
    }
    let absolute = pattern.as_bytes().starts_with(b"/");
    let components = pattern
        .as_bytes()
        .split(|byte| *byte == b'/')
        .filter(|component| !component.is_empty())
        .map(|component| component.to_vec())
        .collect::<Vec<_>>();
    if components.is_empty() {
        return Ok(Vec::new());
    }
    let host = if absolute {
        PathBuf::from("/")
    } else {
        cwd.to_path_buf()
    };
    let output = if absolute { b"/".to_vec() } else { Vec::new() };
    let mut matches = Vec::new();
    expand_glob_components(&host, &output, &components, 0, span, &mut matches)?;
    matches.sort_unstable();
    matches.dedup();
    Ok(matches)
}

fn expand_glob_components(
    host: &std::path::Path,
    output: &[u8],
    components: &[Vec<u8>],
    index: usize,
    span: Span,
    matches: &mut Vec<Vec<u8>>,
) -> Result<(), RuntimeError> {
    if index == components.len() {
        matches.push(if output.is_empty() {
            b".".to_vec()
        } else {
            output.to_vec()
        });
        return Ok(());
    }
    let component = &components[index];
    if component == b"**" {
        expand_glob_components(host, output, components, index + 1, span, matches)?;
        for entry in read_glob_dir(host, span)? {
            if entry.is_dir {
                let next_output = append_path_component(output, &entry.name);
                expand_glob_components(
                    &entry.host_path,
                    &next_output,
                    components,
                    index,
                    span,
                    matches,
                )?;
            }
        }
        return Ok(());
    }

    if component_has_glob_meta(component) {
        let include_hidden = component.starts_with(b".");
        for entry in read_glob_dir(host, span)? {
            if !include_hidden && entry.name.starts_with(b".") {
                continue;
            }
            if !glob_component_matches(component, &entry.name) {
                continue;
            }
            let next_output = append_path_component(output, &entry.name);
            if index + 1 == components.len() {
                matches.push(next_output);
            } else if entry.is_dir {
                expand_glob_components(
                    &entry.host_path,
                    &next_output,
                    components,
                    index + 1,
                    span,
                    matches,
                )?;
            }
        }
        return Ok(());
    }

    let next_host = host.join(OsString::from_vec(component.clone()));
    let next_output = append_path_component(output, component);
    if index + 1 == components.len() {
        if next_host.exists() {
            matches.push(next_output);
        }
        return Ok(());
    }
    if std::fs::metadata(&next_host)
        .map(|metadata| metadata.file_type().is_dir())
        .unwrap_or(false)
    {
        expand_glob_components(
            &next_host,
            &next_output,
            components,
            index + 1,
            span,
            matches,
        )?;
    }
    Ok(())
}

struct GlobDirEntry {
    name: Vec<u8>,
    host_path: PathBuf,
    is_dir: bool,
}

fn read_glob_dir(host: &std::path::Path, span: Span) -> Result<Vec<GlobDirEntry>, RuntimeError> {
    let read_dir = match std::fs::read_dir(host) {
        Ok(read_dir) => read_dir,
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(Vec::new());
        }
        Err(error) => {
            return Err(RuntimeError::new("glob-read", error.to_string()).with_span(span));
        }
    };
    let mut entries = Vec::new();
    for entry in read_dir {
        let entry = entry
            .map_err(|error| RuntimeError::new("glob-read", error.to_string()).with_span(span))?;
        let host_path = entry.path();
        let metadata = match std::fs::symlink_metadata(&host_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(RuntimeError::new("glob-read", error.to_string()).with_span(span));
            }
        };
        entries.push(GlobDirEntry {
            name: entry.file_name().as_bytes().to_vec(),
            host_path,
            is_dir: metadata.file_type().is_dir(),
        });
    }
    entries.sort_unstable_by(|left, right| left.name.cmp(&right.name));
    Ok(entries)
}

fn append_path_component(prefix: &[u8], component: &[u8]) -> Vec<u8> {
    if prefix.is_empty() {
        return component.to_vec();
    }
    let mut output = prefix.to_vec();
    if !output.ends_with(b"/") {
        output.push(b'/');
    }
    output.extend_from_slice(component);
    output
}

fn component_has_glob_meta(component: &[u8]) -> bool {
    component
        .iter()
        .any(|byte| matches!(byte, b'*' | b'?' | b'['))
}

fn glob_component_matches(pattern: &[u8], name: &[u8]) -> bool {
    glob_component_matches_inner(pattern, name)
}

fn glob_component_matches_inner(pattern: &[u8], name: &[u8]) -> bool {
    if pattern.is_empty() {
        return name.is_empty();
    }
    match pattern[0] {
        b'*' => {
            glob_component_matches_inner(&pattern[1..], name)
                || (!name.is_empty() && glob_component_matches_inner(pattern, &name[1..]))
        }
        b'?' => !name.is_empty() && glob_component_matches_inner(&pattern[1..], &name[1..]),
        b'[' => {
            let Some((matched, consumed)) = glob_class_matches(pattern, name.first().copied())
            else {
                return !name.is_empty()
                    && name[0] == b'['
                    && glob_component_matches_inner(&pattern[1..], &name[1..]);
            };
            matched
                && !name.is_empty()
                && glob_component_matches_inner(&pattern[consumed..], &name[1..])
        }
        byte => {
            !name.is_empty()
                && name[0] == byte
                && glob_component_matches_inner(&pattern[1..], &name[1..])
        }
    }
}

fn glob_class_matches(pattern: &[u8], candidate: Option<u8>) -> Option<(bool, usize)> {
    if pattern.first() != Some(&b'[') {
        return None;
    }
    let mut index = 1;
    let negated = matches!(pattern.get(index), Some(b'!' | b'^'));
    if negated {
        index += 1;
    }
    let candidate = candidate?;
    let mut matched = false;
    let mut saw_item = false;
    while index < pattern.len() {
        if pattern[index] == b']' && saw_item {
            return Some((if negated { !matched } else { matched }, index + 1));
        }
        saw_item = true;
        if index + 2 < pattern.len() && pattern[index + 1] == b'-' && pattern[index + 2] != b']' {
            let start = pattern[index];
            let end = pattern[index + 2];
            let (lo, hi) = if start <= end {
                (start, end)
            } else {
                (end, start)
            };
            if candidate >= lo && candidate <= hi {
                matched = true;
            }
            index += 3;
        } else {
            if candidate == pattern[index] {
                matched = true;
            }
            index += 1;
        }
    }
    None
}

fn standard_module_command_name(name: &str) -> Option<(&str, &str)> {
    let (module, api) = name.split_once('.')?;
    api_spec()
        .is_standard_module(module)
        .then_some((module, api))
}

pub(super) fn value_matches_static_type(value: &Value, ty: &Type) -> bool {
    match ty {
        Type::Any | Type::Unknown | Type::Invalid => true,
        Type::Null => matches!(value, Value::Null),
        Type::Bool => matches!(value, Value::Bool(_)),
        Type::Int => matches!(value, Value::Int(_)),
        Type::Float => matches!(value, Value::Float(_)),
        Type::Duration => matches!(value, Value::Duration(_)),
        Type::Str => matches!(value, Value::Str(_)),
        Type::Bytes => matches!(value, Value::Bytes(_)),
        Type::Digest => matches!(value, Value::Digest(_)),
        Type::Regex => matches!(value, Value::Regex(_)),
        Type::Path => matches!(value, Value::Path(_)),
        Type::List(item_ty) => match value {
            Value::List(items) => items
                .iter()
                .all(|item| value_matches_static_type(item, item_ty)),
            _ => false,
        },
        Type::Map(item_ty) => match value {
            Value::Map(items) => items
                .values()
                .all(|item| value_matches_static_type(item, item_ty)),
            _ => false,
        },
        Type::Stream(item_ty) => match value {
            Value::Stream(stream) => stream
                .items
                .iter()
                .all(|item| value_matches_static_type(&item.value, item_ty)),
            _ => false,
        },
        Type::Record(fields) => match value {
            Value::Record(_) | Value::FsEntry(_) if fields.is_empty() => true,
            Value::Record(record) => fields.iter().all(|(field, field_ty)| {
                record
                    .get(field.as_str().as_ref())
                    .is_some_and(|value| value_matches_static_type(value, field_ty))
            }),
            Value::FsEntry(entry) => fields.iter().all(|(field, field_ty)| {
                entry
                    .field_value(&field.as_str())
                    .and_then(Result::ok)
                    .as_ref()
                    .is_some_and(|value| value_matches_static_type(value, field_ty))
            }),
            _ => false,
        },
        Type::Module(exports) => matches!(value, Value::Module(_)) && exports.is_empty(),
        Type::Result(ok_ty, err_ty) => match value {
            Value::Result(ResultValue::Ok(value)) => value_matches_static_type(value, ok_ty),
            Value::Result(ResultValue::Err(value)) => value_matches_static_type(value, err_ty),
            _ => false,
        },
        Type::Status => matches!(value, Value::Status(_)),
        Type::EnvPathList => matches!(value, Value::EnvPathList),
        Type::Error => matches!(value, Value::Error(_)),
        Type::ErrorFamily(family) => {
            matches!(value, Value::Error(error) if error.family_name() == *family)
        }
        Type::ErrorVariant { family, variant } => {
            matches!(value, Value::Error(error) if error.family_name() == *family && error.variant_name() == *variant)
        }
        Type::ErrorFacet(facet) => {
            matches!(value, Value::Error(error) if error.facets.iter().any(|value| value == facet))
        }
        Type::ProcessError => matches!(value, Value::RunError(_)),
        Type::Pure => matches!(value, Value::Pure(_)),
        Type::Proc => matches!(value, Value::Proc(_)),
        Type::Command => matches!(value, Value::Command(_)),
        Type::ProcessHandle => matches!(value, Value::ProcessHandle(_)),
        Type::Unit => matches!(value, Value::Unit),
        Type::Tag(_) => matches!(value, Value::Tag { .. }),
        Type::Optional(inner) => {
            matches!(value, Value::Null) || value_matches_static_type(value, inner)
        }
    }
}

fn lowered_value_matches_static_type(value: &LoweredValue, ty: &Type) -> bool {
    match ty {
        Type::Any | Type::Unknown | Type::Invalid => true,
        Type::Null => matches!(value, LoweredValue::Null),
        Type::Bool => matches!(value, LoweredValue::Bool(_)),
        Type::Int => matches!(value, LoweredValue::Int(_)),
        Type::Float => matches!(value, LoweredValue::Float(_)),
        Type::Duration => matches!(value, LoweredValue::Duration(_)),
        Type::Str => matches!(value, LoweredValue::Str(_) | LoweredValue::StrView(_)),
        Type::Bytes => matches!(value, LoweredValue::Bytes(_) | LoweredValue::BytesView(_)),
        Type::Digest => matches!(value, LoweredValue::Digest(_)),
        Type::Regex => matches!(value, LoweredValue::Regex(_)),
        Type::Path => matches!(value, LoweredValue::Path(_)),
        Type::List(item_ty) => match value {
            LoweredValue::List(items) => items
                .iter()
                .all(|item| lowered_value_matches_static_type(item, item_ty)),
            LoweredValue::SharedList(items) => items
                .iter()
                .all(|item| lowered_value_matches_static_type(item, item_ty)),
            _ => false,
        },
        Type::Map(item_ty) => match value {
            LoweredValue::Map(items) => items
                .values()
                .all(|item| lowered_value_matches_static_type(item, item_ty)),
            LoweredValue::Record(record) => record
                .values()
                .all(|item| lowered_value_matches_static_type(item, item_ty)),
            LoweredValue::RecordVec(record) => record
                .iter()
                .all(|(_, item)| lowered_value_matches_static_type(item, item_ty)),
            LoweredValue::FsEntry(entry) => entry.to_record_map().is_ok_and(|record| {
                record
                    .values()
                    .filter_map(lowered_value_from_runtime_any)
                    .all(|item| lowered_value_matches_static_type(&item, item_ty))
            }),
            _ => false,
        },
        Type::Stream(_) => matches!(value, LoweredValue::Stream(_)),
        Type::Record(fields) => match value {
            LoweredValue::Record(_) | LoweredValue::RecordVec(_) | LoweredValue::FsEntry(_)
                if fields.is_empty() =>
            {
                true
            }
            LoweredValue::Stats { .. } | LoweredValue::StatsBlob(_) if fields.is_empty() => true,
            LoweredValue::Record(record) => fields.iter().all(|(field, field_ty)| {
                {
                    let field_text = field.as_str();
                    record.get::<str>(field_text.as_str())
                }
                .is_some_and(|value| lowered_value_matches_static_type(value, field_ty))
            }),
            LoweredValue::RecordVec(record) => fields.iter().all(|(field, field_ty)| {
                lowered_record_vec_get(record, &field.as_str())
                    .is_some_and(|value| lowered_value_matches_static_type(value, field_ty))
            }),
            value @ (LoweredValue::Stats { .. } | LoweredValue::StatsBlob(_)) => {
                fields.iter().all(|(field, field_ty)| {
                    lowered_stats_value_field(value, &field.as_str())
                        .is_some_and(|value| lowered_value_matches_static_type(&value, field_ty))
                })
            }
            LoweredValue::FsEntry(entry) => fields.iter().all(|(field, field_ty)| {
                entry
                    .field_value(&field.as_str())
                    .and_then(Result::ok)
                    .as_ref()
                    .is_some_and(|value| value_matches_static_type(value, field_ty))
            }),
            _ => false,
        },
        Type::Module(exports) => match value {
            LoweredValue::Module(_) if exports.is_empty() => true,
            LoweredValue::Module(module) => exports.iter().all(|(field, export)| {
                let field_text = field.as_str();
                module.get::<str>(field_text.as_str()).is_some_and(|value| {
                    lowered_value_matches_static_type(value, &export.field_type())
                })
            }),
            _ => false,
        },
        Type::Result(ok_ty, err_ty) => match value {
            LoweredValue::ResultOk(value) => lowered_value_matches_static_type(value, ok_ty),
            LoweredValue::ResultErr(value) => value_matches_static_type(value, err_ty),
            _ => false,
        },
        Type::Status => matches!(value, LoweredValue::Status(_)),
        Type::EnvPathList => false,
        Type::Error => matches!(value, LoweredValue::Error(_)),
        Type::ErrorFamily(family) => {
            matches!(value, LoweredValue::Error(value) if matches!(value.as_ref(), Value::Error(error) if error.family_name() == *family))
        }
        Type::ErrorVariant { family, variant } => {
            matches!(value, LoweredValue::Error(value) if matches!(value.as_ref(), Value::Error(error) if error.family_name() == *family && error.variant_name() == *variant))
        }
        Type::ErrorFacet(facet) => {
            matches!(value, LoweredValue::Error(value) if matches!(value.as_ref(), Value::Error(error) if error.facets.iter().any(|value| value == facet)))
        }
        Type::Command => matches!(value, LoweredValue::Command(_)),
        Type::ProcessHandle => matches!(value, LoweredValue::ProcessHandle(_)),
        Type::ProcessError => false,
        Type::Pure => matches!(value, LoweredValue::Pure(_)),
        Type::Proc => matches!(value, LoweredValue::Proc(_)),
        Type::Unit => matches!(value, LoweredValue::Unit),
        Type::Tag(_) => matches!(value, LoweredValue::Tag(_)),
        Type::Optional(inner) => {
            matches!(value, LoweredValue::Null) || lowered_value_matches_static_type(value, inner)
        }
    }
}

fn runtime_diagnostic(span: Span, message: &str, code: &str) -> Diagnostic {
    crate::diagnostic::Diagnostic::error(message)
        .with_code(code)
        .with_label(crate::diagnostic::Label::primary(span, ""))
}

fn compact_lowerability_diagnostic(span: Span, message: &str, code: &str) -> Diagnostic {
    crate::diagnostic::Diagnostic::error(message)
        .with_code(code)
        .with_label(crate::diagnostic::Label::primary(span, message))
}

fn compact_top_level_stmt_is_skippable(
    program: &ArenaProgram,
    id: StmtId,
    allow_checker_only: bool,
) -> bool {
    if compact_is_main_at_args_call(program, id) {
        return true;
    }
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Export(inner) => {
            compact_top_level_stmt_is_skippable(program, inner, allow_checker_only)
        }
        ArenaStmtKind::Use(use_id) => {
            compact_use_stmt_is_skippable(program, use_id, allow_checker_only)
        }
        ArenaStmtKind::Expr(expr)
            if allow_checker_only && compact_expr_is_reveal_type_call(program, expr) =>
        {
            true
        }
        ArenaStmtKind::TypeDef(_)
        | ArenaStmtKind::ErrorDef(_)
        | ArenaStmtKind::ProcDef(_)
        | ArenaStmtKind::PureDef(_)
        | ArenaStmtKind::StreamDef(_) => true,
        _ => false,
    }
}

fn compact_use_stmt_is_skippable(
    program: &ArenaProgram,
    id: crate::syntax::arena::UseStmtId,
    allow_checker_only: bool,
) -> bool {
    if allow_checker_only {
        return true;
    }
    let use_stmt = program.arena.use_stmt(id);
    if use_stmt.alias.is_some() || use_stmt.resolved.is_some() {
        return false;
    }
    let mut path = program.arena.names(use_stmt.path);
    let Some(name) = path.next() else {
        return false;
    };
    path.next().is_none() && api_spec().is_standard_module(&name.as_str())
}

fn compact_expr_is_reveal_type_call(program: &ArenaProgram, expr: ExprId) -> bool {
    let ArenaExprKind::Call { callee, .. } = program.arena.expr(expr).kind else {
        return false;
    };
    matches!(program.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "reveal_type")
}

fn compact_root_proc_main_requires_auto_call_indexed(
    program: &ArenaProgram,
    root: &[StmtId],
    indexed: &FullProgram,
) -> Result<bool, Diagnostic> {
    if !root
        .iter()
        .copied()
        .any(|stmt| compact_root_proc_main_exists(program, stmt))
    {
        return Ok(false);
    }
    let Some((last_index, last_stmt)) = root.iter().copied().enumerate().next_back() else {
        return Ok(false);
    };
    if !compact_is_main_at_args_call(program, last_stmt) {
        return Ok(true);
    }
    if compact_is_main_spliced_args_call(program, last_stmt)
        && !compact_root_binds_name_before(program, root, last_index, Name::intern("args"))
    {
        return Ok(true);
    }
    indexed.driver_step_is_skip(last_index).map_err(|error| {
        compact_lowerability_diagnostic(
            program.arena.stmt(last_stmt).span,
            &format!("indexed driver verification failed: {}", error.message),
            "compact.indexed-driver",
        )
    })
}

fn compact_should_skip_auto_main_stmt(
    program: &ArenaProgram,
    root: &[StmtId],
    index: usize,
    auto_main_required: bool,
) -> bool {
    auto_main_required
        && root
            .get(index)
            .copied()
            .is_some_and(|stmt| compact_is_main_spliced_args_call(program, stmt))
        && root.len().checked_sub(1) == Some(index)
        && !compact_root_binds_name_before(program, root, index, Name::intern("args"))
}

fn compact_root_binds_name_before(
    program: &ArenaProgram,
    root: &[StmtId],
    index: usize,
    name: Name,
) -> bool {
    root.iter()
        .take(index)
        .copied()
        .any(|stmt| compact_stmt_binds_name(program, stmt, name))
}

fn compact_stmt_binds_name(program: &ArenaProgram, id: StmtId, name: Name) -> bool {
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Export(inner) => compact_stmt_binds_name(program, inner, name),
        ArenaStmtKind::Let { target, .. } | ArenaStmtKind::Var { target, .. } => {
            compact_binding_target_binds_name(program, target, name)
        }
        _ => false,
    }
}

fn compact_binding_target_binds_name(
    program: &ArenaProgram,
    id: crate::syntax::arena::BindingTargetId,
    name: Name,
) -> bool {
    match program.arena.binding_target(id).kind {
        crate::syntax::arena::ArenaBindingTargetKind::Name(binding) => binding == name,
        crate::syntax::arena::ArenaBindingTargetKind::Record { fields, .. } => program
            .arena
            .destructure_fields(fields)
            .iter()
            .any(|field| field.name == name),
    }
}

fn compact_root_proc_main_exists(program: &ArenaProgram, id: StmtId) -> bool {
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Export(inner) => compact_root_proc_main_exists(program, inner),
        ArenaStmtKind::ProcDef(def) => program.arena.function_def(def).name == Name::intern("main"),
        _ => false,
    }
}

fn compact_root_proc_main_span(program: &ArenaProgram, id: StmtId) -> Option<Span> {
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Export(inner) => compact_root_proc_main_span(program, inner),
        ArenaStmtKind::ProcDef(def)
            if program.arena.function_def(def).name == Name::intern("main") =>
        {
            Some(program.arena.stmt(id).span)
        }
        _ => None,
    }
}

fn compact_is_main_at_args_call(program: &ArenaProgram, id: StmtId) -> bool {
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Expr(expr) => compact_is_main_at_args_expr(program, expr),
        _ => false,
    }
}

fn compact_is_main_spliced_args_call(program: &ArenaProgram, id: StmtId) -> bool {
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Expr(expr) => compact_is_main_spliced_args_expr(program, expr),
        _ => false,
    }
}

fn compact_is_main_at_args_expr(program: &ArenaProgram, id: ExprId) -> bool {
    match program.arena.expr(id).kind {
        ArenaExprKind::Try(inner) => compact_is_main_at_args_expr(program, inner),
        ArenaExprKind::Call { callee, args } => {
            matches!(program.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == Name::intern("main"))
                && matches!(
                    program.arena.call_args(args),
                    [arg] if compact_is_args_call_arg(program, arg)
                )
        }
        _ => false,
    }
}

fn compact_is_main_spliced_args_expr(program: &ArenaProgram, id: ExprId) -> bool {
    match program.arena.expr(id).kind {
        ArenaExprKind::Try(inner) => compact_is_main_spliced_args_expr(program, inner),
        ArenaExprKind::Call { callee, args } => {
            matches!(program.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == Name::intern("main"))
                && matches!(
                    program.arena.call_args(args),
                    [arg] if compact_is_spliced_args_call_arg(program, arg)
                )
        }
        _ => false,
    }
}

fn compact_is_args_call_arg(program: &ArenaProgram, arg: &ArenaCallArg) -> bool {
    let value = match arg.kind {
        ArenaCallArgKind::Positional(value) | ArenaCallArgKind::Splice { value, .. } => value,
        ArenaCallArgKind::Named { .. } => return false,
    };
    matches!(program.arena.expr(value).kind, ArenaExprKind::Ident(name) if name == Name::intern("args"))
}

fn compact_is_spliced_args_call_arg(program: &ArenaProgram, arg: &ArenaCallArg) -> bool {
    let ArenaCallArgKind::Splice { value, .. } = arg.kind else {
        return false;
    };
    matches!(program.arena.expr(value).kind, ArenaExprKind::Ident(name) if name == Name::intern("args"))
}

fn zero_span() -> Span {
    Span::new(crate::source::SourceId::new(0), 0, 0)
}

fn run_eval<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    const EVAL_STACK_SIZE: usize = 12 * 1024 * 1024;
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(debug_test_eval_stack_size(EVAL_STACK_SIZE))
            .spawn_scoped(scope, f)
            .expect("spawn evaluation worker thread")
            .join()
            .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
    })
}

fn debug_test_eval_stack_size(default: usize) -> usize {
    if cfg!(debug_assertions) && std::env::var_os("XSH_TEST_SMALL_EVAL_STACK").is_some() {
        8 * 1024 * 1024
    } else {
        default
    }
}

fn next_event_id(events: &[TraceEvent]) -> u64 {
    events.last().map_or(1, |event| event.event_id + 1)
}
