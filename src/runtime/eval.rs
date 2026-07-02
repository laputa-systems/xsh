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
use crate::symbol::{Name, QualifiedName};
use crate::syntax::arena::{
    ArenaCallArg, ArenaCallArgKind, ArenaExprKind, ArenaProgram, ArenaStmtKind, ExprId, StmtId,
};
use crate::syntax::node::{AssignOp, BinaryOp, FormatSpec, RedirectionKind, RunKind};
use crate::trace::{
    TraceArg, TraceEnv, TraceError, TraceEvent, TraceKind, TracePayload, TraceStatus,
    TraceStatusKind, TraceTiming, Traceback, TracebackFrame,
};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod lower;
mod lowered_ops;
use lowered_ops::lowered_value_from_runtime;
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
    pub sources: SourceMap,
    pub status: u8,
    pub cwd: PathBuf,
    pub env: BTreeMap<Vec<u8>, Vec<u8>>,
    pub last_status: Option<ProcessStatus>,
}

#[derive(Clone, Debug, Default)]
pub struct TestEvalOutput {
    pub output: EvalOutput,
    pub result: Option<Value>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CompactInstallTimings {
    pub declarations: Duration,
    pub runtime_declarations: Duration,
    pub bodies: Duration,
    pub functions: Duration,
    pub top_level: Duration,
    pub commit: Duration,
    pub total: Duration,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct EvaluatorInitTimings {
    pub current_dir: Duration,
    pub struct_init: Duration,
    pub args_bindings: Duration,
    pub total: Duration,
}

#[derive(Clone, Debug, Default)]
pub struct CompactLowerProbeOutput {
    pub type_defs: usize,
    pub lowered_aliases: usize,
    pub lowered_records: usize,
    pub lowered_tag_unions: usize,
    pub tag_variants: usize,
    pub tag_arities: usize,
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
    retained: CompactLowerConstructRetained,
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
            retained: CompactLowerConstructRetained::default(),
        }
    }
}

impl CompactLowerConstructProbeOutput {
    pub fn function_units(&self) -> &[LoweredFunctionUnit] {
        &self.retained.functions
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
struct CompactLowerConstructRetained {
    functions: Vec<LoweredFunctionUnit>,
    programs: Vec<LoweredProgram>,
}

#[derive(Clone, Debug, Default)]
struct CompactLoweredFunctions {
    pures: FxHashMap<Name, Arc<LoweredPureFunction>>,
    procs: FxHashMap<Name, Arc<LoweredPureFunction>>,
    qualified_pures: FxHashMap<QualifiedName, Arc<LoweredPureFunction>>,
    qualified_procs: FxHashMap<QualifiedName, Arc<LoweredPureFunction>>,
}

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

pub fn probe_compact_lower_declarations(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
) -> CompactLowerProbeOutput {
    lower::probe_compact_lower_declarations(program, declarations)
}

pub fn probe_compact_lower_constructed_bodies(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    bodies: &CompactBodyProbeOutput,
    source: &str,
) -> CompactLowerConstructProbeOutput {
    lower::probe_compact_lower_constructed_bodies(program, declarations, bodies, source)
}

pub fn probe_compact_lower_function_units(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    bodies: &CompactBodyProbeOutput,
    source: &str,
) -> Vec<LoweredFunctionUnit> {
    lower::probe_compact_lower_function_units(program, declarations, bodies, source)
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
    lowered_body: Arc<LoweredPureFunction>,
    lowered_slots: Vec<LoweredTopLevelSlot>,
    scope: FxHashMap<Name, Binding>,
    span: Span,
    ignore_pending_primary: bool,
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

#[derive(Clone, Debug)]
pub(super) struct TestMock {
    pub matcher: RecordMap,
    pub result: Value,
    pub remaining: i64,
}

#[derive(Clone, Debug)]
pub(super) struct TestCall {
    pub op: String,
    pub args: RecordMap,
}

/// Upper bound on recycled scope maps held in `scope_pool` (deep recursion
/// shouldn't let the pool grow without bound).

#[derive(Clone, Debug)]
struct LoweredPureFunction {
    params: LoweredParamNames,
    param_kinds: LoweredParamKinds,
    param_checks: LoweredParamChecks,
    param_rest: LoweredParamRest,
    param_defaults: LoweredParamDefaults,
    captures: LoweredTopLevelSlots,
    return_kind: LoweredReturnKind,
    slot_count: usize,
    body: Vec<LoweredStmt>,
    has_defers: bool,
}

#[derive(Clone, Debug, Default)]
struct LoweredProgram {
    statements: Vec<Option<LoweredTopLevelStmt>>,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    body: Option<Arc<LoweredPureFunction>>,
    blocker: Option<LoweredFunctionBlocker>,
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

    pub fn scc_member_count(&self) -> usize {
        self.scc_member_count
    }

    pub fn scc_group(&self) -> Option<usize> {
        self.scc_group
    }

    pub fn is_scc_member(&self) -> bool {
        self.scc_member_count > 1
    }

    fn lowered_body(&self) -> Option<Arc<LoweredPureFunction>> {
        self.body.clone()
    }
}

struct LowerableFunctions<'a> {
    pures: Option<&'a FxHashMap<Name, Arc<LoweredPureFunction>>>,
    procs: Option<&'a FxHashMap<Name, Arc<LoweredPureFunction>>>,
    qualified_pures: Option<&'a FxHashMap<QualifiedName, Arc<LoweredPureFunction>>>,
    qualified_procs: Option<&'a FxHashMap<QualifiedName, Arc<LoweredPureFunction>>>,
    // In-flight candidates not yet committed to the lowered maps. A single key
    // for self-recursion, or a whole strongly-connected component for
    // mutually-recursive co-lowering. Membership only — function-body call
    // lowering needs `contains`, not the callee's return kind (which is resolved
    // from the lowered maps).
    candidates: &'a [LoweredFunctionKey],
}

impl<'a> LowerableFunctions<'a> {
    fn all_with_candidates(
        pures: &'a FxHashMap<Name, Arc<LoweredPureFunction>>,
        procs: &'a FxHashMap<Name, Arc<LoweredPureFunction>>,
        qualified_pures: &'a FxHashMap<QualifiedName, Arc<LoweredPureFunction>>,
        qualified_procs: &'a FxHashMap<QualifiedName, Arc<LoweredPureFunction>>,
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
        pures: &'a FxHashMap<Name, Arc<LoweredPureFunction>>,
        procs: &'a FxHashMap<Name, Arc<LoweredPureFunction>>,
        qualified_pures: &'a FxHashMap<QualifiedName, Arc<LoweredPureFunction>>,
        qualified_procs: &'a FxHashMap<QualifiedName, Arc<LoweredPureFunction>>,
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
                    self.pures.is_some_and(|pures| pures.contains_key(&name))
                        || self.procs.is_some_and(|procs| procs.contains_key(&name))
                }
                LoweredFunctionKey::Qualified(name) => {
                    self.qualified_pures
                        .is_some_and(|pures| pures.contains_key(&name))
                        || self
                            .qualified_procs
                            .is_some_and(|procs| procs.contains_key(&name))
                }
            }
    }

    fn pure_contains(&self, key: LoweredFunctionKey) -> bool {
        self.candidates.contains(&key)
            || match key {
                LoweredFunctionKey::Name(name) => {
                    self.pures.is_some_and(|pures| pures.contains_key(&name))
                }
                LoweredFunctionKey::Qualified(name) => self
                    .qualified_pures
                    .is_some_and(|pures| pures.contains_key(&name)),
            }
    }
}

#[derive(Clone, Debug)]
struct LoweredTopLevelStmt {
    kind: LoweredTopLevelKind,
    slots: LoweredTopLevelSlots,
    slot_count: usize,
}

#[derive(Clone, Debug)]
enum LoweredTopLevelKind {
    Use {
        key: Arc<str>,
        alias: Option<Name>,
        path: Vec<Name>,
        namespace: Name,
        exports: Vec<LoweredModuleExport>,
        module_statements: Vec<(Span, LoweredTopLevelStmt)>,
        span: Span,
    },
    Let {
        target: Name,
        ty: Option<LoweredType>,
        validation: Option<LoweredTypeCheck>,
        mutable: bool,
        value: LoweredExpr,
        value_span: Span,
    },
    // `let {a, b, ..} = source` / `var {…}` at top level: define one named
    // binding per field (field name == binding name) from the source record.
    LetRecord {
        source: LoweredExpr,
        fields: Vec<Name>,
        mutable: bool,
        span: Span,
    },
    Assign {
        target: Name,
        op: AssignOp,
        value: LoweredExpr,
        span: Span,
    },
    Discard {
        value: LoweredExpr,
        span: Span,
    },
    Stmt(LoweredStmt),
    Expr(LoweredExpr),
    Defer {
        value: LoweredExpr,
        span: Span,
    },
    SignalHook {
        signal: Name,
        pre_cancel: Option<String>,
        body: Vec<LoweredStmt>,
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
type LoweredPatternSlots = SmallVec<[Option<usize>; 2]>;
type LoweredCompFields = SmallVec<[(Arc<str>, usize, Span); 4]>;
type LoweredErrorPatternFields = SmallVec<[(Arc<str>, Option<usize>); 4]>;

#[derive(Clone, Debug)]
enum LoweredCompTarget {
    Slot(usize),
    Record { fields: LoweredCompFields },
}

#[derive(Clone, Debug)]
enum LoweredRecordEntry {
    Field(Arc<str>, LoweredExpr),
    Spread(LoweredExpr),
}

#[derive(Clone, Debug)]
enum LoweredProcessCommandBuilderEntry {
    Field {
        name: Name,
        value: LoweredExpr,
        span: Span,
    },
    Run {
        target: LoweredRunArg,
        args: Vec<LoweredRunArg>,
        env: Vec<LoweredRunEnv>,
        timeout: Option<LoweredExpr>,
        cpu_max: Option<LoweredExpr>,
        span: Span,
    },
}

#[derive(Clone, Debug)]
enum LoweredStmt {
    Let {
        slot: usize,
        value: LoweredExpr,
    },
    /// `guard let slot = value else |else_param| { else_body }`: evaluate
    /// `value` (a `Result`); on `Ok`, bind its inner value to `slot` and
    /// continue; on `Err`, bind the error to `else_param_slot` (if present) and
    /// run `else_body`, which must diverge.
    Guard {
        slot: usize,
        value: LoweredExpr,
        else_param_slot: Option<usize>,
        else_body: Vec<LoweredStmt>,
        span: Span,
    },
    LetInt {
        slot: usize,
        value: LoweredIntExpr,
    },
    LetBool {
        slot: usize,
        value: LoweredBoolExpr,
    },
    Assign {
        slot: usize,
        op: AssignOp,
        value: LoweredExpr,
        span: Span,
    },
    AssignField {
        slot: usize,
        field: Arc<str>,
        op: AssignOp,
        value: LoweredExpr,
        span: Span,
    },
    AssignFieldInt {
        slot: usize,
        field: Arc<str>,
        op: AssignOp,
        value: LoweredIntExpr,
        span: Span,
    },
    AssignIndex {
        slot: usize,
        index: LoweredExpr,
        op: AssignOp,
        value: LoweredExpr,
        span: Span,
    },
    AssignInt {
        slot: usize,
        op: AssignOp,
        value: LoweredIntExpr,
        span: Span,
    },
    AssignBool {
        slot: usize,
        value: LoweredBoolExpr,
    },
    Expr {
        value: LoweredExpr,
        span: Span,
    },
    If {
        branches: Vec<(LoweredExpr, Vec<LoweredStmt>)>,
        else_body: Option<Vec<LoweredStmt>>,
    },
    IfBool {
        branches: Vec<(LoweredBoolExpr, Vec<LoweredStmt>)>,
        else_body: Option<Vec<LoweredStmt>>,
    },
    While {
        condition: LoweredExpr,
        body: Vec<LoweredStmt>,
    },
    WhileBool {
        condition: LoweredBoolExpr,
        body: Vec<LoweredStmt>,
    },
    Match {
        value: LoweredExpr,
        arms: Vec<(LoweredPattern, Option<LoweredExpr>, Vec<LoweredStmt>)>,
        span: Span,
    },
    StrMatch {
        value: LoweredExpr,
        arms: FxHashMap<Arc<str>, Vec<LoweredStmt>>,
        fallback: Option<Vec<LoweredStmt>>,
        span: Span,
    },
    TagMatch {
        value: LoweredExpr,
        arms: FxHashMap<Arc<str>, Vec<LoweredStmt>>,
        fallback: Option<Vec<LoweredStmt>>,
        span: Span,
    },
    For {
        slot: usize,
        iter: LoweredExpr,
        body: Vec<LoweredStmt>,
        span: Span,
    },
    // `let {a, b, ..} = source` / `var {…} = source`: destructure a record into
    // one slot per field (field name == binding name).
    LetRecord {
        source: LoweredExpr,
        fields: Vec<(Name, usize)>,
        span: Span,
    },
    // `for {a, b, ..} in iter { … }`: per item (a record), bind each field slot.
    ForRecord {
        fields: Vec<(Name, usize)>,
        iter: LoweredExpr,
        body: Vec<LoweredStmt>,
        span: Span,
    },
    ForStrLines {
        slot: usize,
        text: LoweredExpr,
        body: Vec<LoweredStmt>,
        span: Span,
    },
    Print {
        args: Vec<LoweredExpr>,
        stderr: bool,
        propagate_result: bool,
        span: Span,
    },
    Cd {
        target: LoweredExpr,
        body: Vec<LoweredStmt>,
        propagate_result: bool,
        span: Span,
    },
    Env {
        env: Vec<LoweredRunEnv>,
        body: Vec<LoweredStmt>,
    },
    Proc {
        module: Arc<str>,
        name: Arc<str>,
        args: Vec<LoweredExpr>,
        propagate_result: bool,
        span: Span,
    },
    Run {
        value: LoweredExpr,
        propagate_result: bool,
    },
    Loop {
        body: Vec<LoweredStmt>,
    },
    Return {
        value: LoweredExpr,
    },
    Yield {
        value: LoweredExpr,
    },
    Break,
    BreakValue {
        value: LoweredExpr,
    },
    Continue,
    Defer {
        value: LoweredExpr,
    },
}

#[derive(Clone, Debug)]
enum LoweredIntExpr {
    Int(i64),
    Slot(usize),
    Binary {
        op: BinaryOp,
        left: Box<LoweredIntExpr>,
        right: Box<LoweredIntExpr>,
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
        index: Box<LoweredIntExpr>,
        default: Option<Box<LoweredIntExpr>>,
        span: Span,
    },
}

#[derive(Clone, Debug)]
enum LoweredBoolExpr {
    Bool(bool),
    Slot(usize),
    Not(Box<LoweredBoolExpr>),
    And(Box<LoweredBoolExpr>, Box<LoweredBoolExpr>),
    Or(Box<LoweredBoolExpr>, Box<LoweredBoolExpr>),
    IntCompare {
        op: BinaryOp,
        left: Box<LoweredIntExpr>,
        right: Box<LoweredIntExpr>,
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

enum LoweredStmtFlow {
    None,
    Return(LoweredValue),
    Propagate(LoweredValue),
    Break(Option<LoweredValue>),
    Continue,
}

#[derive(Clone, Debug)]
enum LoweredExpr {
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
        value: Box<LoweredExpr>,
        span: Span,
    },
    Param(usize),
    Binary {
        op: BinaryOp,
        left: Box<LoweredExpr>,
        right: Box<LoweredExpr>,
        span: Span,
    },
    IfExpr {
        branches: Vec<(LoweredExpr, LoweredExpr)>,
        else_value: Box<LoweredExpr>,
        span: Span,
    },
    MatchExpr {
        value: Box<LoweredExpr>,
        arms: Vec<(LoweredPattern, Option<LoweredExpr>, LoweredExpr)>,
        span: Span,
    },
    StrMatchExpr {
        value: Box<LoweredExpr>,
        arms: FxHashMap<Arc<str>, LoweredExpr>,
        fallback: Option<Box<LoweredExpr>>,
        span: Span,
    },
    TagMatchExpr {
        value: Box<LoweredExpr>,
        arms: FxHashMap<Arc<str>, LoweredExpr>,
        fallback: Option<Box<LoweredExpr>>,
        span: Span,
    },
    ResultFallback {
        left: Box<LoweredExpr>,
        right: Box<LoweredExpr>,
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
    List(Vec<LoweredExpr>),
    // The `map.empty()` builtin constructor (empty list literals already lower via `List`).
    EmptyMap,
    // The `bytes.concat(<List[Bytes]>)` builtin constructor.
    BytesConcat {
        arg: Box<LoweredExpr>,
        span: Span,
    },
    Range {
        start: Box<LoweredExpr>,
        end: Box<LoweredExpr>,
        span: Span,
    },
    Tag {
        name: Arc<str>,
        fields: Vec<LoweredExpr>,
    },
    ListComp {
        value: Box<LoweredExpr>,
        target: LoweredCompTarget,
        iter: Box<LoweredExpr>,
        condition: Option<Box<LoweredExpr>>,
        span: Span,
    },
    MapComp {
        key: Box<LoweredExpr>,
        value: Box<LoweredExpr>,
        target: LoweredCompTarget,
        iter: Box<LoweredExpr>,
        condition: Option<Box<LoweredExpr>>,
        span: Span,
    },
    ListPipeline {
        input: Box<LoweredExpr>,
        stages: Vec<LoweredPipelineStage>,
        span: Span,
    },
    Field {
        base: Box<LoweredExpr>,
        name: String,
        span: Span,
    },
    Index {
        base: Box<LoweredExpr>,
        index: Box<LoweredExpr>,
        span: Span,
    },
    Slice {
        base: Box<LoweredExpr>,
        start: Option<Box<LoweredExpr>>,
        end: Option<Box<LoweredExpr>>,
        span: Span,
    },
    Method {
        receiver: Box<LoweredExpr>,
        name: String,
        args: Vec<LoweredExpr>,
        span: Span,
    },
    StrByteLen {
        receiver: Box<LoweredExpr>,
        span: Span,
    },
    StrByteAt {
        receiver: Box<LoweredExpr>,
        index: Box<LoweredExpr>,
        default: Option<Box<LoweredExpr>>,
        span: Span,
    },
    StrPredicate {
        receiver: Box<LoweredExpr>,
        predicate: LoweredStrPredicate,
        needle: Box<LoweredExpr>,
        span: Span,
    },
    Contains {
        receiver: Box<LoweredExpr>,
        needle: Box<LoweredExpr>,
        span: Span,
    },
    RegexCompile {
        pattern: Box<LoweredExpr>,
        span: Span,
    },
    Require {
        value: Box<LoweredExpr>,
        check: LoweredTypeCheck,
        span: Span,
    },
    RunCapture {
        kind: RunKind,
        target: Box<LoweredRunArg>,
        args: Vec<LoweredRunArg>,
        env: Vec<LoweredRunEnv>,
        redirections: Vec<LoweredRunRedirection>,
        timeout: Option<Box<LoweredExpr>>,
        cpu_max: Option<Box<LoweredExpr>>,
        // For Plain/Status run *values* with `?`, propagation is handled inside
        // eval_lowered_run_capture (Break on RunError, pass Status through),
        // because a Plain run yields a bare Status on success — not a Result the
        // external `Try` wrapper could unwrap. Capture kinds keep the external
        // Try and leave this false.
        propagate: bool,
        assert_success: bool,
        span: Span,
    },
    RunPipeline {
        segments: Vec<LoweredRunPipelineSegment>,
        propagate: bool,
        span: Span,
    },
    SpawnRun {
        target: Box<LoweredRunArg>,
        args: Vec<LoweredRunArg>,
        env: Vec<LoweredRunEnv>,
        redirections: Vec<LoweredRunRedirection>,
        timeout: Option<Box<LoweredExpr>>,
        cpu_max: Option<Box<LoweredExpr>>,
        span: Span,
    },
    SpawnCommand {
        command: Box<LoweredExpr>,
        span: Span,
    },
    Wait {
        target: Box<LoweredExpr>,
        span: Span,
    },
    Loop {
        body: Vec<LoweredStmt>,
        span: Span,
    },
    Retry {
        delays: Vec<LoweredExpr>,
        body: Vec<LoweredStmt>,
        span: Span,
    },
    FsFiles {
        root: Box<LoweredExpr>,
        gitignore: bool,
        stat: bool,
        hidden: bool,
        exts: Option<Box<LoweredExpr>>,
        result_wrapped: bool,
        span: Span,
    },
    FsWalk {
        root: Box<LoweredExpr>,
        gitignore: bool,
        stat: bool,
        hidden: bool,
        exts: Option<Box<LoweredExpr>>,
        result_wrapped: bool,
        span: Span,
    },
    FsList {
        op: RuntimeOp,
        path: Box<LoweredExpr>,
        stat: Option<Box<LoweredExpr>>,
        ordered: Option<Box<LoweredExpr>>,
        span: Span,
    },
    FsTempDir {
        span: Span,
    },
    FsWrite {
        path: Box<LoweredExpr>,
        data: Box<LoweredExpr>,
        span: Span,
    },
    FsMkdir {
        path: Box<LoweredExpr>,
        parents: Option<Box<LoweredExpr>>,
        span: Span,
    },
    FsRemove {
        path: Box<LoweredExpr>,
        missing_ok: Option<Box<LoweredExpr>>,
        span: Span,
    },
    FsCloseRoot {
        root: Box<LoweredExpr>,
        span: Span,
    },
    FsRootPath {
        root: Box<LoweredExpr>,
        span: Span,
    },
    PathReadText {
        path: Box<LoweredExpr>,
        span: Span,
    },
    PathReadBytes {
        path: Box<LoweredExpr>,
        span: Span,
    },
    PathExists {
        path: Box<LoweredExpr>,
        span: Span,
    },
    PathExecutable {
        path: Box<LoweredExpr>,
        span: Span,
    },
    PathDu {
        path: Box<LoweredExpr>,
        span: Span,
    },
    PathMetadata {
        path: Box<LoweredExpr>,
        span: Span,
    },
    PathReadlink {
        path: Box<LoweredExpr>,
        span: Span,
    },
    PathResolve {
        path: Box<LoweredExpr>,
        span: Span,
    },
    PathWrite {
        path: Box<LoweredExpr>,
        data: Box<LoweredExpr>,
        atomic: bool,
        span: Span,
    },
    PathMkdir {
        path: Box<LoweredExpr>,
        parents: Option<Box<LoweredExpr>>,
        span: Span,
    },
    PathRemove {
        path: Box<LoweredExpr>,
        missing_ok: Option<Box<LoweredExpr>>,
        span: Span,
    },
    JsonEncode {
        value: Box<LoweredExpr>,
        span: Span,
    },
    ArchiveTarCreate {
        path: Box<LoweredExpr>,
        root: Box<LoweredExpr>,
        entries: Box<LoweredExpr>,
        compression: Option<Box<LoweredExpr>>,
        overwrite: Option<Box<LoweredExpr>>,
        span: Span,
    },
    ArchiveTarList {
        path: Box<LoweredExpr>,
        span: Span,
    },
    ArchiveTarExtract {
        path: Box<LoweredExpr>,
        dest: Box<LoweredExpr>,
        span: Span,
    },
    HashVerifyFile {
        path: Box<LoweredExpr>,
        algorithm: crate::modules::hash::HashAlgorithm,
        expected: Box<LoweredExpr>,
        span: Span,
    },
    ModuleCall {
        op: RuntimeOp,
        args: Vec<LoweredExpr>,
        span: Span,
    },
    ProcessCommandArgv {
        target: Box<LoweredExpr>,
        argv: Box<LoweredExpr>,
        cwd: Option<Box<LoweredExpr>>,
        env: Option<Box<LoweredExpr>>,
        timeout: Option<Box<LoweredExpr>>,
        detach: Option<Box<LoweredExpr>>,
        new_session: Option<Box<LoweredExpr>>,
        ignore_hup: Option<Box<LoweredExpr>>,
        cpu_max: Option<Box<LoweredExpr>>,
        span: Span,
    },
    ProcessCommandBuilder {
        entries: Vec<LoweredProcessCommandBuilderEntry>,
        span: Span,
    },
    Abort {
        status: Box<LoweredExpr>,
        force: Option<Box<LoweredExpr>>,
        span: Span,
    },
    Ok(Box<LoweredExpr>),
    Err(Box<LoweredExpr>),
    Error(LoweredErrorExpr),
    Try(Box<LoweredExpr>),
    Call {
        function: LoweredFunctionKey,
        args: Vec<LoweredCallArg>,
        span: Span,
    },
    DynamicCall {
        callee: Box<LoweredExpr>,
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
    Single(LoweredExpr),
    Splice(LoweredExpr),
}

#[derive(Clone, Debug)]
struct LoweredRunArg {
    kind: LoweredRunArgKind,
    span: Span,
}

#[derive(Clone, Debug)]
enum LoweredRunArgKind {
    Single(LoweredExpr),
    SingleOrSplice(LoweredExpr),
    Splice(LoweredExpr),
}

#[derive(Clone, Debug)]
struct LoweredRunPipelineSegment {
    #[allow(dead_code)]
    kind: RunKind,
    target: LoweredRunArg,
    args: Vec<LoweredRunArg>,
    env: Vec<LoweredRunEnv>,
    redirections: Vec<LoweredRunRedirection>,
    timeout: Option<Box<LoweredExpr>>,
    cpu_max: Option<Box<LoweredExpr>>,
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
    Expr(LoweredExpr, Span, Option<FormatSpec>),
}

#[derive(Clone, Debug)]
enum LoweredPattern {
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
        family: Arc<str>,
        variant: Arc<str>,
        fields: LoweredErrorPatternFields,
        result_wrapped: bool,
    },
    // `is Facet => …`: matches when the error value carries `facet`.
    // `result_wrapped` distinguishes `Err(is Facet)` (scrutinee is a Result)
    // from a standalone `is Facet` (scrutinee is the error value itself).
    Facet {
        facet: Arc<str>,
        result_wrapped: bool,
    },
    Tag {
        name: Arc<str>,
        slots: LoweredPatternSlots,
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
        predicate: LoweredExpr,
    },
    Map {
        slot: usize,
        value: LoweredExpr,
    },
    MapBlock {
        slot: usize,
        body: Vec<LoweredStmt>,
        value: LoweredExpr,
    },
    FlatMap {
        slot: usize,
        value: LoweredExpr,
    },
    FlatMapBlock {
        slot: usize,
        body: Vec<LoweredStmt>,
        value: LoweredExpr,
    },
    BytesChunks {
        size: LoweredExpr,
    },
    BatchCount {
        count: LoweredExpr,
    },
    BatchMaxArgv {
        max_argv: Option<LoweredExpr>,
    },
    BatchMaxBytes {
        max_bytes: LoweredExpr,
    },
    Shuffle {
        seed: Option<LoweredExpr>,
    },
    Fold {
        acc_slot: usize,
        item_slot: usize,
        initial: LoweredExpr,
        body: Vec<LoweredStmt>,
        value: LoweredExpr,
    },
    ReduceBy {
        item_slot: usize,
        body: Vec<LoweredStmt>,
        value: LoweredExpr,
        op: ReduceByOp,
    },
    ParMap {
        slot: usize,
        value: LoweredExpr,
    },
    ParMapBlock {
        slot: usize,
        body: Vec<LoweredStmt>,
        value: LoweredExpr,
    },
    Tee {
        slot: usize,
        body: Vec<LoweredStmt>,
    },
    Each {
        slot: usize,
        body: Vec<LoweredStmt>,
        parallel: bool,
    },
    TablePrint {
        columns: Option<Vec<String>>,
    },
    Enumerate,
    Zip {
        other: LoweredExpr,
    },
    Sort {
        descending: Option<LoweredExpr>,
    },
    SortBy {
        slot: usize,
        key: LoweredExpr,
        descending: Option<LoweredExpr>,
    },
    GroupBy {
        slot: usize,
        key: LoweredExpr,
    },
    CountBy {
        slot: usize,
        key: LoweredExpr,
    },
    Any {
        slot: usize,
        predicate: LoweredExpr,
    },
    All {
        slot: usize,
        predicate: LoweredExpr,
    },
    UniqueBy {
        slot: usize,
        key: LoweredExpr,
    },
    Count,
    Sum,
    Collect,
    First,
    Last,
    Min,
    Max,
    Take(LoweredExpr),
    Drop(LoweredExpr),
    Repeat {
        count: LoweredExpr,
    },
    Range {
        start: LoweredExpr,
        end: LoweredExpr,
    },
}

impl LoweredPipelineStage {
    /// The source stage spelling, used as the `stream.stage` trace name/payload
    /// (mirrors `StreamStageKind::as_str`).
    fn trace_name(&self) -> &'static str {
        match self {
            Self::TextLines => "text.lines",
            Self::JsonLines => "json.lines",
            Self::Where { .. } => "where",
            Self::Map { .. } | Self::MapBlock { .. } => "map",
            Self::FlatMap { .. } | Self::FlatMapBlock { .. } => "flat-map",
            Self::BytesChunks { .. } => "bytes.chunks",
            Self::BatchCount { .. } | Self::BatchMaxArgv { .. } | Self::BatchMaxBytes { .. } => {
                "batch"
            }
            Self::Shuffle { .. } => "shuffle",
            Self::Fold { .. } => "fold",
            Self::ReduceBy { .. } => "reduce-by",
            Self::ParMap { .. } | Self::ParMapBlock { .. } => "par-map",
            Self::Tee { .. } => "tee",
            Self::Each { .. } => "each",
            Self::TablePrint { .. } => "table.print",
            Self::Enumerate => "enumerate",
            Self::Zip { .. } => "zip",
            Self::Sort { .. } => "sort",
            Self::SortBy { .. } => "sort-by",
            Self::GroupBy { .. } => "group-by",
            Self::CountBy { .. } | Self::Count => "count",
            Self::Any { .. } => "any",
            Self::All { .. } => "all",
            Self::UniqueBy { .. } => "unique-by",
            Self::Sum => "sum",
            Self::Collect => "collect",
            Self::First => "first",
            Self::Last => "last",
            Self::Min => "min",
            Self::Max => "max",
            Self::Take(_) => "take",
            Self::Drop(_) => "drop",
            Self::Repeat { .. } => "repeat",
            Self::Range { .. } => "range",
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum LoweredStrPredicate {
    StartsWith,
    EndsWith,
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
        fields: Vec<(Arc<str>, LoweredExpr)>,
        facets: Vec<Name>,
    },
}

#[derive(Clone, Debug)]
struct LoweredStrView {
    text: Arc<str>,
    start: usize,
    end: usize,
}

impl LoweredStrView {
    fn new(text: Arc<str>, start: usize, end: usize) -> Self {
        debug_assert!(start <= end);
        debug_assert!(text.is_char_boundary(start));
        debug_assert!(text.is_char_boundary(end));
        Self { text, start, end }
    }

    fn as_str(&self) -> &str {
        &self.text[self.start..self.end]
    }

    fn into_arc(self) -> Arc<str> {
        self.as_str().into()
    }
}

fn assign_lowered_str_view(slot: &mut LoweredValue, text: &Arc<str>, start: usize, end: usize) {
    match slot {
        LoweredValue::StrView(view) if Arc::ptr_eq(&view.text, text) => {
            debug_assert!(start <= end);
            debug_assert!(text.is_char_boundary(start));
            debug_assert!(text.is_char_boundary(end));
            view.start = start;
            view.end = end;
        }
        _ => {
            *slot = LoweredValue::StrView(LoweredStrView::new(text.clone(), start, end));
        }
    }
}

#[derive(Clone, Debug)]
struct LoweredBytesView {
    bytes: Arc<[u8]>,
    start: usize,
    end: usize,
}

impl LoweredBytesView {
    fn new(bytes: Arc<[u8]>, start: usize, end: usize) -> Self {
        debug_assert!(start <= end);
        debug_assert!(end <= bytes.len());
        Self { bytes, start, end }
    }

    fn as_slice(&self) -> &[u8] {
        &self.bytes[self.start..self.end]
    }
}

fn assign_lowered_bytes_view(slot: &mut LoweredValue, bytes: &Arc<[u8]>, start: usize, end: usize) {
    match slot {
        LoweredValue::BytesView(view) if Arc::ptr_eq(&view.bytes, bytes) => {
            debug_assert!(start <= end);
            debug_assert!(end <= bytes.len());
            view.start = start;
            view.end = end;
        }
        _ => {
            *slot = LoweredValue::BytesView(LoweredBytesView::new(bytes.clone(), start, end));
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
    Digest(DigestValue),
    Regex(RegexValue),
    Status(ProcessStatus),
    Path(PathValue),
    Command(CommandPlan),
    ProcessHandle(Box<ProcessHandleValue>),
    Stream(StreamValue),
    Pure(FunctionName),
    Proc(FunctionName),
    Error(Box<Value>),
    Record(BTreeMap<Arc<str>, LoweredValue>),
    Module(BTreeMap<Arc<str>, LoweredValue>),
    List(Vec<LoweredValue>),
    Map(BTreeMap<String, LoweredValue>),
    Tag {
        name: Arc<str>,
        fields: Vec<LoweredValue>,
    },
    ResultOk(Box<LoweredValue>),
    ResultErr(Box<Value>),
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
            (Self::Command(left), Self::Command(right)) => left == right,
            (Self::ProcessHandle(left), Self::ProcessHandle(right)) => left == right,
            (Self::Stream(left), Self::Stream(right)) => left == right,
            (Self::Pure(left), Self::Pure(right)) => left == right,
            (Self::Proc(left), Self::Proc(right)) => left == right,
            (Self::Error(left), Self::Error(right)) => left == right,
            (Self::Record(left), Self::Record(right)) => left == right,
            (Self::Module(left), Self::Module(right)) => left == right,
            (Self::List(left), Self::List(right)) => left == right,
            (Self::Map(left), Self::Map(right)) => left == right,
            (
                Self::Tag {
                    name: left_name,
                    fields: left_fields,
                },
                Self::Tag {
                    name: right_name,
                    fields: right_fields,
                },
            ) => left_name == right_name && left_fields == right_fields,
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
            Self::Digest(value) => Value::digest(value),
            Self::Regex(value) => Value::Regex(value),
            Self::Status(value) => Value::Status(value),
            Self::Path(value) => Value::Path(value),
            Self::Command(value) => Value::Command(Box::new(value)),
            Self::ProcessHandle(value) => Value::ProcessHandle(value),
            Self::Stream(value) => Value::Stream(Box::new(value)),
            Self::Pure(value) => Value::Pure(value),
            Self::Proc(value) => Value::Proc(value),
            Self::Error(value) => *value,
            Self::Record(value) => {
                let mut record = RecordMap::new();
                for (key, value) in value {
                    record.insert(key, value.into_value());
                }
                Value::Record(record)
            }
            Self::Module(value) => {
                let mut module = RecordMap::new();
                for (key, value) in value {
                    module.insert(key, value.into_value());
                }
                Value::Module(module)
            }
            Self::List(value) => {
                Value::List(value.into_iter().map(LoweredValue::into_value).collect())
            }
            Self::Map(value) => {
                let mut map = BTreeMap::new();
                for (key, value) in value {
                    map.insert(key, value.into_value());
                }
                Value::Map(map)
            }
            Self::Tag { name, fields } => Value::Tag {
                name,
                fields: fields.into_iter().map(LoweredValue::into_value).collect(),
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
            Self::Command(_) => "Command",
            Self::ProcessHandle(_) => "ProcessHandle",
            Self::Stream(_) => "Stream",
            Self::Pure(_) => "Pure",
            Self::Proc(_) => "Proc",
            Self::Error(_) => "Error",
            Self::Record(_) => "Record",
            Self::Module(_) => "Module",
            Self::List(_) => "List",
            Self::Map(_) => "Map",
            Self::Tag { .. } => "Tag",
            Self::ResultOk(_) | Self::ResultErr(_) => "Result",
        }
    }
}

/// Method names the lowering pass will emit (as `LoweredExpr::Method` or a
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
    sources: SourceMap,
    command_name: String,
    scopes: Vec<FxHashMap<Name, Binding>>,
    // Signatures of functions exported by dynamically loaded modules
    // (`module.load`), keyed by the export's `FunctionName`. Captured from the
    // compact declaration probe at load time so `module.require` can validate
    // `export proc`/`export pure` contract fields without the old recursive AST.
    module_export_signatures: FxHashMap<crate::runtime::value::FunctionName, ModuleExportSignature>,
    lowered_pures: FxHashMap<Name, Arc<LoweredPureFunction>>,
    lowered_procs: FxHashMap<Name, Arc<LoweredPureFunction>>,
    lowered_qualified_pures: FxHashMap<QualifiedName, Arc<LoweredPureFunction>>,
    lowered_qualified_procs: FxHashMap<QualifiedName, Arc<LoweredPureFunction>>,
    lowered_program: LoweredProgram,
    lowered_slot_pool: Vec<Vec<LoweredValue>>,
    tag_variants: FxHashMap<Name, usize>,
    error_families: FxHashMap<Name, RuntimeErrorFamily>,
    module_value_cache: FxHashMap<String, RecordMap>,
    function_modules: FxHashMap<Name, String>,
    qualified_function_modules: FxHashMap<QualifiedName, String>,
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
    pub(super) test_mocks: FxHashMap<String, Vec<TestMock>>,
    pub(super) test_calls: Vec<TestCall>,
    test_temp_counter: u64,
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

    pub(crate) fn new_with_sources_at_cwd(
        argv: Vec<String>,
        sources: SourceMap,
        cwd: PathBuf,
    ) -> Self {
        Self::new_with_sources_and_command_inner::<false>(
            argv,
            sources,
            "command".to_string(),
            Some(cwd),
            None,
        )
    }

    pub(crate) fn new_with_sources_at_cwd_profiled(
        argv: Vec<String>,
        sources: SourceMap,
        cwd: PathBuf,
    ) -> (Self, EvaluatorInitTimings) {
        Self::new_with_sources_profiled_inner(argv, sources, Some(cwd))
    }

    fn new_with_sources_profiled_inner(
        argv: Vec<String>,
        sources: SourceMap,
        cwd: Option<PathBuf>,
    ) -> (Self, EvaluatorInitTimings) {
        let mut timings = EvaluatorInitTimings::default();
        let evaluator = Self::new_with_sources_and_command_inner::<true>(
            argv,
            sources,
            "command".to_string(),
            cwd,
            Some(&mut timings),
        );
        (evaluator, timings)
    }

    pub fn new_with_sources_and_command(
        argv: Vec<String>,
        sources: SourceMap,
        command_name: String,
    ) -> Self {
        Self::new_with_sources_and_command_inner::<false>(argv, sources, command_name, None, None)
    }

    fn new_with_sources_and_command_inner<const PROFILE: bool>(
        argv: Vec<String>,
        sources: SourceMap,
        command_name: String,
        cwd: Option<PathBuf>,
        timings: Option<&mut EvaluatorInitTimings>,
    ) -> Self {
        let start = PROFILE.then(Instant::now);
        let cwd =
            cwd.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let after_current_dir = PROFILE.then(Instant::now);
        let mut evaluator = Self {
            sources,
            command_name,
            scopes: vec![FxHashMap::default()],
            module_export_signatures: FxHashMap::default(),
            lowered_pures: FxHashMap::default(),
            lowered_procs: FxHashMap::default(),
            lowered_qualified_pures: FxHashMap::default(),
            lowered_qualified_procs: FxHashMap::default(),
            lowered_program: LoweredProgram::default(),
            lowered_slot_pool: Vec::new(),
            tag_variants: FxHashMap::default(),
            error_families: FxHashMap::default(),
            module_value_cache: FxHashMap::default(),
            function_modules: FxHashMap::default(),
            qualified_function_modules: FxHashMap::default(),
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
            test_mocks: FxHashMap::default(),
            test_calls: Vec::new(),
            test_temp_counter: 0,
        };
        let after_struct_init = PROFILE.then(Instant::now);
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
        let after_args_bindings = PROFILE.then(Instant::now);
        if PROFILE
            && let (
                Some(timings),
                Some(start),
                Some(after_current_dir),
                Some(after_struct_init),
                Some(after_args_bindings),
            ) = (
                timings,
                start,
                after_current_dir,
                after_struct_init,
                after_args_bindings,
            )
        {
            *timings = EvaluatorInitTimings {
                current_dir: after_current_dir.duration_since(start),
                struct_init: after_struct_init.duration_since(after_current_dir),
                args_bindings: after_args_bindings.duration_since(after_struct_init),
                total: after_args_bindings.duration_since(start),
            };
        }
        evaluator
    }

    pub fn into_sources(self) -> SourceMap {
        self.sources
    }

    pub fn with_tracing(mut self) -> Self {
        self.trace_enabled = true;
        self
    }

    fn register_compact_signal_hook(
        &mut self,
        signal: &str,
        pre_cancel: Option<&str>,
        body: Vec<LoweredStmt>,
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
        let has_defers = lower::lowered_body_has_defers(&body);
        let lowered_function = LoweredPureFunction {
            params: Default::default(),
            param_kinds: Default::default(),
            param_checks: Default::default(),
            param_rest: Default::default(),
            param_defaults: Default::default(),
            captures: Default::default(),
            return_kind: LoweredReturnKind::Plain(LoweredType::Unit),
            slot_count,
            body,
            has_defers,
        };
        self.signal_hooks.insert(
            signal.name.clone(),
            RegisteredSignalHook {
                signal,
                pre_cancel,
                lowered_body: Arc::new(lowered_function),
                lowered_slots: hook_slots,
                scope: self.scopes.first().cloned().unwrap_or_default(),
                span,
                ignore_pending_primary,
            },
        );
        Ok(())
    }

    /// Fork a lightweight copy of this evaluator for use in a parallel `par-map`
    /// worker thread. Shares all lowered function definitions (via Arc in the
    /// maps) and gives the worker its own I/O buffers and slot pool.
    pub(super) fn fork_for_par_map(&self) -> Self {
        Self {
            sources: self.sources.clone(),
            command_name: self.command_name.clone(),
            scopes: self.scopes.clone(),
            module_export_signatures: self.module_export_signatures.clone(),
            lowered_pures: self.lowered_pures.clone(),
            lowered_procs: self.lowered_procs.clone(),
            lowered_qualified_pures: self.lowered_qualified_pures.clone(),
            lowered_qualified_procs: self.lowered_qualified_procs.clone(),
            lowered_program: self.lowered_program.clone(),
            lowered_slot_pool: Vec::new(),
            tag_variants: self.tag_variants.clone(),
            error_families: self.error_families.clone(),
            module_value_cache: self.module_value_cache.clone(),
            function_modules: self.function_modules.clone(),
            qualified_function_modules: self.qualified_function_modules.clone(),
            active_modules: self.active_modules.clone(),
            stdout: Vec::new(),
            stderr: Vec::new(),
            cwd: self.cwd.clone(),
            env: self.env.clone(),
            interactive: false,
            interactive_command_dispatcher: None,
            last_status: self.last_status.clone(),
            trace_enabled: false,
            trace_events: Vec::new(),
            event_stack: Vec::new(),
            call_stack: Vec::new(),
            pending_traceback: None,
            stream_items: Vec::new(),
            unix_next_pid: 0,
            fs_locks: Vec::new(),
            fs_roots: Vec::new(),
            net_agents: FxHashMap::default(),
            net_pool_options: self.net_pool_options.clone(),
            utils_cache: FxHashMap::default(),
            signal_hooks: FxHashMap::default(),
            signal_handler_guards: Vec::new(),
            active_process_groups: Vec::new(),
            next_process_handle_id: 0,
            process_handles: BTreeMap::new(),
            scope_ids: Vec::new(),
            signal_state: EvaluatorSignalState::default(),
            test_mocks: self.test_mocks.clone(),
            test_calls: Vec::new(),
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
        let lowered_body = &hook.lowered_body;
        let call_span = hook.span;
        let mut slots = vec![LoweredValue::Unit; lowered_body.slot_count];
        for slot_info in &hook.lowered_slots {
            if let Some(binding) = self.lookup(slot_info.name)
                && let Some(value) = lowered_value_from_runtime(&binding.value, slot_info.kind)
            {
                slots[slot_info.slot] = value;
            }
        }
        let result = self.eval_lowered_body_as_signal_hook(lowered_body, &mut slots, call_span);
        self.scopes = saved_scopes;
        result
    }

    fn eval_lowered_body_as_signal_hook(
        &mut self,
        lowered: &LoweredPureFunction,
        slots: &mut [LoweredValue],
        call_span: Span,
    ) -> Result<Flow, RuntimeError> {
        let flow = self.eval_lowered_stmts(lowered, &lowered.body, slots, call_span)?;
        match flow {
            LoweredStmtFlow::None => Ok(Flow::Continue(Value::Unit)),
            LoweredStmtFlow::Return(value) => Ok(Flow::Continue(value.into_value())),
            LoweredStmtFlow::Propagate(value) => {
                let error = match value {
                    LoweredValue::Error(error) => *error,
                    LoweredValue::ResultErr(error) => *error,
                    other => Value::Error(Box::new(
                        RuntimeError::new(
                            "signal-hook",
                            format!("propagated {}", other.type_name()),
                        )
                        .with_span(call_span),
                    )),
                };
                let kind = error.error_kind().unwrap_or("error").to_string();
                let message = error
                    .error_message()
                    .unwrap_or("signal hook error")
                    .to_string();
                let traceback = self.pending_traceback.take().unwrap_or_else(|| Traceback {
                    failing_span: Some(call_span),
                    operation_kind: "signal.hook".to_string(),
                    error: TraceError { kind, message },
                    frames: self.call_stack.clone(),
                });
                Ok(Flow::Propagate(Propagation { error, traceback }))
            }
            LoweredStmtFlow::Break(_) | LoweredStmtFlow::Continue => {
                Ok(Flow::Continue(Value::Unit))
            }
        }
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

    /// Run a program through the compact arena + lowered IR. This is the only
    /// runtime path: the program is installed via the compact lowering pipeline
    /// and every top-level statement runs through `eval_lowered_top_level_stmt`.
    /// Unlike `try_eval_compact_lowered_only`, this runs even when tracing /
    /// coverage is enabled, and an un-lowerable statement surfaces a runtime
    /// diagnostic instead of falling back to a recursive evaluator (there is no
    /// recursive evaluator any more — the compact corpus is exhaustive).
    pub fn eval(self, program: &ArenaProgram, source_id: SourceId) -> EvalOutput {
        run_eval_on_large_stack(move || self.eval_inner(program, source_id))
    }

    fn eval_inner(mut self, program: &ArenaProgram, source_id: SourceId) -> EvalOutput {
        let install_diagnostics = self.install_compact_lowered_program(program, source_id);
        let root = program.statement_ids().collect::<Vec<_>>();
        let lowered_statements = self.lowered_program.statements.clone();
        let auto_main_required =
            compact_root_proc_main_requires_auto_call(program, &root, &lowered_statements);
        let compact_auto_main_args = if auto_main_required {
            self.compact_auto_main_args().unwrap_or_default()
        } else {
            Vec::new()
        };

        crate::runtime::process::clear_cancellation_request();
        let script_span = root
            .first()
            .map(|stmt| program.arena.stmt(*stmt).span)
            .unwrap_or_else(zero_span);
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
        let mut compact_defers = Vec::new();
        if let Some(diagnostic) = install_diagnostics.first() {
            let span = diagnostic_primary_span(diagnostic).unwrap_or(script_span);
            let kind = diagnostic.code.as_deref().unwrap_or("compact-install");
            diagnostics.push(runtime_diagnostic(
                span,
                &diagnostic.message,
                "runtime.error",
            ));
            traceback = Some(self.traceback_for_value(
                span,
                "runtime.error",
                &Value::Error(Box::new(
                    RuntimeError::new(kind, diagnostic.message.clone()).with_span(span),
                )),
            ));
            self.trace_exit(
                TraceKind::ScriptExit,
                Some(script_span),
                Some("script"),
                TracePayload::None,
            );
            return EvalOutput {
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
            };
        }
        for (index, stmt) in root.iter().copied().enumerate() {
            let span = program.arena.stmt(stmt).span;
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
            let Some(lowered) = lowered_statements.get(index).cloned().flatten() else {
                if compact_should_skip_auto_main_stmt(program, &root, index, auto_main_required)
                    || compact_top_level_stmt_is_skippable(program, stmt)
                {
                    continue;
                }
                diagnostics.push(runtime_diagnostic(
                    span,
                    "statement could not be lowered to the compact runtime",
                    "runtime.unlowered-statement",
                ));
                traceback = Some(self.traceback_for_value(
                    span,
                    "runtime.error",
                    &Value::Error(Box::new(RuntimeError::new(
                        "unlowered-statement",
                        "statement could not be lowered to the compact runtime",
                    ))),
                ));
                break;
            };
            if compact_should_skip_auto_main_stmt(program, &root, index, auto_main_required) {
                continue;
            }
            if matches!(lowered.kind, LoweredTopLevelKind::Defer { .. }) {
                compact_defers.push(lowered);
                continue;
            }
            match self.eval_lowered_top_level_stmt(&lowered, span) {
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
                    diagnostics.push(runtime_diagnostic(
                        span,
                        "statement could not be lowered to the compact runtime",
                        "runtime.unlowered-statement",
                    ));
                    traceback = Some(self.traceback_for_value(
                        span,
                        "runtime.error",
                        &Value::Error(Box::new(RuntimeError::new(
                            "unlowered-statement",
                            "statement could not be lowered to the compact runtime",
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

        if auto_main_required && traceback.is_none() && abort.is_none() && !stopped {
            let zero = zero_span();
            let call_result =
                self.call_lowered_proc(Name::intern("main"), &compact_auto_main_args, zero);
            match call_result {
                Some(Ok(Value::Result(ResultValue::Err(error)))) => {
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
                Some(Ok(value)) => last_value = value,
                Some(Err(error)) => {
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
                None => {
                    diagnostics.push(runtime_diagnostic(
                        zero,
                        "main could not be lowered to the compact runtime",
                        "runtime.unlowered-statement",
                    ));
                    traceback = Some(self.traceback_for_value(
                        zero,
                        "runtime.error",
                        &Value::Error(Box::new(RuntimeError::new(
                            "unlowered-statement",
                            "main could not be lowered to the compact runtime",
                        ))),
                    ));
                }
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
            for lowered in compact_defers.into_iter().rev() {
                if cleanup.is_err() || matches!(cleanup, Ok(Flow::Propagate(_))) {
                    break;
                }
                cleanup = self
                    .eval_lowered_top_level_stmt(&lowered, script_span)
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

        EvalOutput {
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
        }
    }

    pub fn eval_compact_lowered_only(
        self,
        program: &ArenaProgram,
        source_id: SourceId,
    ) -> Option<EvalOutput> {
        self.try_eval_compact_lowered_only(program, source_id).ok()
    }

    pub fn try_eval_compact_lowered_only(
        self,
        program: &ArenaProgram,
        source_id: SourceId,
    ) -> Result<EvalOutput, Self> {
        run_eval_on_large_stack(move || {
            self.try_eval_compact_lowered_only_inner(program, source_id)
        })
    }

    fn try_eval_compact_lowered_only_inner(
        mut self,
        program: &ArenaProgram,
        source_id: SourceId,
    ) -> Result<EvalOutput, Self> {
        if self.trace_enabled {
            return Err(self);
        }
        if !self
            .install_compact_lowered_program(program, source_id)
            .is_empty()
        {
            return Err(self);
        }
        let root = program.statement_ids().collect::<Vec<_>>();
        let auto_main_required = compact_root_proc_main_requires_auto_call(
            program,
            &root,
            &self.lowered_program.statements,
        );
        if auto_main_required && !self.lowered_procs.contains_key(&Name::intern("main")) {
            return Err(self);
        }
        let compact_auto_main_args = if auto_main_required {
            let Some(args) = self.compact_auto_main_args() else {
                return Err(self);
            };
            args
        } else {
            Vec::new()
        };
        if self.lowered_program.statements.len() != root.len() {
            return Err(self);
        }
        for (index, stmt) in root.iter().copied().enumerate() {
            if self.lowered_program.statements[index].is_none()
                && !compact_should_skip_auto_main_stmt(program, &root, index, auto_main_required)
                && !compact_top_level_stmt_is_skippable(program, stmt)
            {
                return Err(self);
            }
        }
        let lowered_statements = self.lowered_program.statements.clone();

        crate::runtime::process::clear_cancellation_request();
        let script_span = root
            .first()
            .map(|stmt| program.arena.stmt(*stmt).span)
            .unwrap_or_else(zero_span);
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
        let mut compact_defers = Vec::new();
        for (index, stmt) in root.iter().copied().enumerate() {
            let span = program.arena.stmt(stmt).span;
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
            let Some(lowered) = lowered_statements[index].clone() else {
                continue;
            };
            if compact_should_skip_auto_main_stmt(program, &root, index, auto_main_required) {
                continue;
            }
            if matches!(lowered.kind, LoweredTopLevelKind::Defer { .. }) {
                compact_defers.push(lowered);
                continue;
            }
            match self.eval_lowered_top_level_stmt(&lowered, span) {
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
                    return Err(self);
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

        if auto_main_required && traceback.is_none() && abort.is_none() && !stopped {
            let zero = zero_span();
            let call_result =
                self.call_lowered_proc(Name::intern("main"), &compact_auto_main_args, zero);
            let Some(call_result) = call_result else {
                return Err(self);
            };
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
            for lowered in compact_defers.into_iter().rev() {
                if cleanup.is_err() || matches!(cleanup, Ok(Flow::Propagate(_))) {
                    break;
                }
                cleanup = self
                    .eval_lowered_top_level_stmt(&lowered, script_span)
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

        lowered_run::print_perf_counters();

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
        if let Some(main) = self.lowered_procs.get(&Name::intern("main")) {
            for (index, arg) in args.iter_mut().enumerate() {
                if main.param_kinds.get(index).copied() == Some(LoweredType::Path)
                    && let Value::Str(text) = arg
                    && let Ok(path) = PathValue::from_text(&text)
                {
                    *arg = Value::Path(path);
                }
            }
        }
        Some(args)
    }

    pub fn eval_test(
        mut self,
        program: &ArenaProgram,
        source_id: SourceId,
        test_name: &str,
        ctx: Value,
    ) -> TestEvalOutput {
        self.install_compact_lowered_program(program, source_id);
        let root = program.statement_ids().collect::<Vec<_>>();
        let lowered_statements = self.lowered_program.statements.clone();
        let script_span = root
            .first()
            .map(|stmt| program.arena.stmt(*stmt).span)
            .unwrap_or_else(zero_span);
        self.trace_enter(
            TraceKind::ScriptEnter,
            Some(script_span),
            Some("test"),
            TracePayload::None,
        );

        let mut result = None;
        let mut traceback = None;
        let mut diagnostics = Vec::new();
        for (index, stmt) in root.iter().copied().enumerate() {
            let span = program.arena.stmt(stmt).span;
            let Some(lowered) = lowered_statements.get(index).cloned().flatten() else {
                if compact_top_level_stmt_is_skippable(program, stmt) {
                    continue;
                }
                diagnostics.push(runtime_diagnostic(
                    span,
                    "statement could not be lowered to the compact runtime",
                    "runtime.unlowered-statement",
                ));
                traceback = Some(self.traceback_for_value(
                    span,
                    "test.setup",
                    &Value::Error(Box::new(RuntimeError::new(
                        "unlowered-statement",
                        "statement could not be lowered to the compact runtime",
                    ))),
                ));
                break;
            };
            if matches!(lowered.kind, LoweredTopLevelKind::Defer { .. }) {
                continue;
            }
            match self.eval_lowered_top_level_stmt(&lowered, span) {
                Ok(Some(Flow::Continue(_)) | None) => {}
                Ok(Some(Flow::Propagate(propagation))) => {
                    traceback = Some(propagation.traceback);
                    break;
                }
                Ok(Some(Flow::Return(_) | Flow::Break(_) | Flow::ContinueLoop)) => {
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
                Err(error) => {
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
            }
        }

        if traceback.is_none() {
            let name = Name::intern(test_name);
            let args = match self.lowered_procs.get(&name) {
                Some(def) if def.params.is_empty() => Vec::new(),
                Some(_) => vec![ctx],
                None => Vec::new(),
            };
            match self.call_lowered_proc(name, &args, script_span) {
                Some(Ok(value)) => result = Some(value),
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
                    diagnostics.push(runtime_diagnostic(span, &error.message, "runtime.error"));
                    traceback = Some(pending_traceback.unwrap_or_else(|| {
                        self.traceback_for_value(span, "test.call", &Value::Error(Box::new(error)))
                    }));
                }
                None => {
                    diagnostics.push(runtime_diagnostic(
                        script_span,
                        "test proc was not found",
                        "runtime.test-missing",
                    ));
                    result = Some(Value::err(Value::Error(Box::new(RuntimeError::new(
                        "test-missing",
                        test_name,
                    )))));
                }
            }
        }

        self.trace_exit(
            TraceKind::ScriptExit,
            Some(script_span),
            Some("test"),
            TracePayload::None,
        );
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

    pub fn install_compact_lowered_program(
        &mut self,
        program: &ArenaProgram,
        source_id: SourceId,
    ) -> Vec<Diagnostic> {
        self.install_compact_lowered::<false>(program, source_id, true, None, None)
    }

    pub(crate) fn install_compact_lowered_program_profiled(
        &mut self,
        program: &ArenaProgram,
        source_id: SourceId,
    ) -> (Vec<Diagnostic>, CompactInstallTimings) {
        let mut timings = CompactInstallTimings::default();
        let diagnostics = self.install_compact_lowered::<true>(
            program,
            source_id,
            true,
            None,
            Some(&mut timings),
        );
        (diagnostics, timings)
    }

    pub fn install_compact_lowered_functions(
        &mut self,
        program: &ArenaProgram,
        source_id: SourceId,
    ) -> Vec<Diagnostic> {
        self.install_compact_lowered::<false>(program, source_id, false, None, None)
    }

    pub fn install_compact_lowered_functions_with_source(
        &mut self,
        program: &ArenaProgram,
        source_id: SourceId,
        source: &str,
    ) -> Vec<Diagnostic> {
        self.install_compact_lowered::<false>(program, source_id, false, Some(source), None)
    }

    fn install_compact_lowered<const PROFILE: bool>(
        &mut self,
        program: &ArenaProgram,
        source_id: SourceId,
        install_top_level: bool,
        explicit_source: Option<&str>,
        timings: Option<&mut CompactInstallTimings>,
    ) -> Vec<Diagnostic> {
        let start = PROFILE.then(Instant::now);
        let declarations = Checker::check_compact_declarations(program);
        let after_declarations = PROFILE.then(Instant::now);
        if !declarations.diagnostics.is_empty() {
            return declarations.diagnostics;
        }
        self.install_compact_runtime_declarations(&declarations);
        let after_runtime_declarations = PROFILE.then(Instant::now);
        let bodies = Checker::probe_compact_bodies(program, &declarations);
        let after_bodies = PROFILE.then(Instant::now);
        if !bodies.diagnostics.is_empty() {
            return bodies.diagnostics;
        }
        let source_id = program.source_text_source_id().unwrap_or(source_id);
        let Some(source) =
            explicit_source.or_else(|| self.sources.get(source_id).map(|source| source.text()))
        else {
            return Vec::new();
        };
        let lowered_functions = lower::lower_compact_root_functions(
            program,
            &declarations,
            &bodies,
            source,
            &self.lowered_qualified_pures,
            &self.lowered_qualified_procs,
        );
        let after_functions = PROFILE.then(Instant::now);
        let functions = LowerableFunctions::all(
            &lowered_functions.pures,
            &lowered_functions.procs,
            &lowered_functions.qualified_pures,
            &lowered_functions.qualified_procs,
        );
        let lowered = install_top_level.then(|| {
            lower::lower_compact_top_level_program(
                program,
                &declarations,
                &bodies,
                source,
                &functions,
            )
        });
        let after_top_level = PROFILE.then(Instant::now);
        self.lowered_pures.extend(lowered_functions.pures);
        self.lowered_procs.extend(lowered_functions.procs);
        self.lowered_qualified_pures
            .extend(lowered_functions.qualified_pures);
        self.lowered_qualified_procs
            .extend(lowered_functions.qualified_procs);
        if let Some(lowered) = lowered {
            self.lowered_program = lowered;
        }
        let after_commit = PROFILE.then(Instant::now);
        if PROFILE
            && let (
                Some(timings),
                Some(start),
                Some(after_declarations),
                Some(after_runtime_declarations),
                Some(after_bodies),
                Some(after_functions),
                Some(after_top_level),
                Some(after_commit),
            ) = (
                timings,
                start,
                after_declarations,
                after_runtime_declarations,
                after_bodies,
                after_functions,
                after_top_level,
                after_commit,
            )
        {
            *timings = CompactInstallTimings {
                declarations: after_declarations.duration_since(start),
                runtime_declarations: after_runtime_declarations.duration_since(after_declarations),
                bodies: after_bodies.duration_since(after_runtime_declarations),
                functions: after_functions.duration_since(after_bodies),
                top_level: after_top_level.duration_since(after_functions),
                commit: after_commit.duration_since(after_top_level),
                total: after_commit.duration_since(start),
            };
        }
        Vec::new()
    }

    fn install_compact_runtime_declarations(&mut self, declarations: &CompactDeclOutput) {
        self.tag_variants
            .extend(compact_runtime_tag_arities(declarations));
        let (error_families, _, _, _) = compact_runtime_error_families(declarations);
        self.error_families.extend(error_families);
    }

    fn lowered_function(&self, function: LoweredFunctionKey) -> Option<Arc<LoweredPureFunction>> {
        match function {
            LoweredFunctionKey::Name(function) => self
                .lowered_pures
                .get(&function)
                .or_else(|| self.lowered_procs.get(&function))
                .cloned(),
            LoweredFunctionKey::Qualified(function) => self
                .lowered_qualified_pures
                .get(&function)
                .or_else(|| self.lowered_qualified_procs.get(&function))
                .cloned(),
        }
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
                    operation_kind: "result.propagate".to_string(),
                    error: TraceError::new("type-error", "`?` expected Result"),
                    frames: self.call_stack.clone(),
                },
            }),
        }
    }

    fn traceback_for_value(&self, span: Span, operation: &str, value: &Value) -> Traceback {
        Traceback {
            failing_span: Some(span),
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
            self.stdout.extend_from_slice(error.message.as_bytes());
            self.stdout.push(b'\n');
            return Some(0);
        }
        if error.kind == "cli-parse"
            && matches!(error.payload.get("cli_usage"), Some(Value::Bool(true)))
        {
            self.stderr.extend_from_slice(error.message.as_bytes());
            self.stderr.push(b'\n');
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
                operation_kind: "result.propagate".to_string(),
                error: TraceError::new("type-error", "`?` expected Result"),
                frames,
            },
        }),
    }
}

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
        self.module_export_signatures
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
            Value::Record(_) if fields.is_empty() => true,
            Value::Record(record) => fields.iter().all(|(field, field_ty)| {
                record
                    .get(field.as_str())
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
            matches!(value, Value::Error(error) if &error.family == family)
        }
        Type::ErrorVariant { family, variant } => {
            matches!(value, Value::Error(error) if &error.family == family && &error.variant == variant)
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
            _ => false,
        },
        Type::Map(item_ty) => match value {
            LoweredValue::Map(items) => items
                .values()
                .all(|item| lowered_value_matches_static_type(item, item_ty)),
            _ => false,
        },
        Type::Stream(_) => matches!(value, LoweredValue::Stream(_)),
        Type::Record(fields) => match value {
            LoweredValue::Record(_) if fields.is_empty() => true,
            LoweredValue::Record(record) => fields.iter().all(|(field, field_ty)| {
                record
                    .get(field.as_str())
                    .is_some_and(|value| lowered_value_matches_static_type(value, field_ty))
            }),
            _ => false,
        },
        Type::Module(exports) => match value {
            LoweredValue::Module(_) if exports.is_empty() => true,
            LoweredValue::Module(module) => exports.iter().all(|(field, export)| {
                module.get(field.as_str()).is_some_and(|value| {
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
            matches!(value, LoweredValue::Error(value) if matches!(value.as_ref(), Value::Error(error) if &error.family == family))
        }
        Type::ErrorVariant { family, variant } => {
            matches!(value, LoweredValue::Error(value) if matches!(value.as_ref(), Value::Error(error) if &error.family == family && &error.variant == variant))
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
        Type::Tag(_) => matches!(value, LoweredValue::Tag { .. }),
        Type::Optional(inner) => {
            matches!(value, LoweredValue::Null) || lowered_value_matches_static_type(value, inner)
        }
    }
}

fn runtime_diagnostic(span: Span, message: &str, code: &str) -> Diagnostic {
    crate::diagnostic::Diagnostic::error(message)
        .with_code(code)
        .with_label(crate::diagnostic::Label::primary(span, message))
}

fn diagnostic_primary_span(diagnostic: &Diagnostic) -> Option<Span> {
    diagnostic
        .span
        .or_else(|| diagnostic.labels.first().map(|label| label.span))
}

fn compact_top_level_stmt_is_skippable(program: &ArenaProgram, id: StmtId) -> bool {
    if compact_is_main_at_args_call(program, id) {
        return true;
    }
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Export(inner) => compact_top_level_stmt_is_skippable(program, inner),
        ArenaStmtKind::Use(use_id) => compact_use_stmt_is_skippable(program, use_id),
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
) -> bool {
    let use_stmt = program.arena.use_stmt(id);
    if use_stmt.alias.is_some() || use_stmt.resolved.is_some() {
        return false;
    }
    let mut path = program.arena.names(use_stmt.path);
    let Some(name) = path.next() else {
        return false;
    };
    path.next().is_none() && api_spec().is_standard_module(name.as_str())
}

fn compact_root_proc_main_requires_auto_call(
    program: &ArenaProgram,
    root: &[StmtId],
    lowered_statements: &[Option<LoweredTopLevelStmt>],
) -> bool {
    if !root
        .iter()
        .copied()
        .any(|stmt| compact_root_proc_main_exists(program, stmt))
    {
        return false;
    }
    let Some((last_index, last_stmt)) = root.iter().copied().enumerate().next_back() else {
        return false;
    };
    if !compact_is_main_at_args_call(program, last_stmt) {
        return true;
    }
    if compact_is_main_spliced_args_call(program, last_stmt)
        && !compact_root_binds_name_before(program, root, last_index, Name::intern("args"))
    {
        return true;
    }
    lowered_statements
        .get(last_index)
        .is_none_or(|lowered| lowered.is_none())
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

/// Run evaluation on a worker thread with a large stack.
///
/// The lowered evaluator recurses in Rust once per XSH call frame, and each
/// frame is large (the eval functions are giant matches over `LoweredExpr` /
/// `LoweredValue`). The default 8 MB main-thread stack overflows after only a
/// few levels of XSH recursion, so evaluation runs on a worker thread sized
/// well above any realistic native frame budget. `RUST_MIN_STACK` does not help
/// here because it governs spawned threads, not the main thread. A scoped
/// thread lets the closure borrow the arena without a `'static` bound.
fn run_eval_on_large_stack<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    const EVAL_STACK_SIZE: usize = 1 << 30;
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(EVAL_STACK_SIZE)
            .spawn_scoped(scope, f)
            .expect("spawn evaluation worker thread")
            .join()
            .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
    })
}

fn next_event_id(events: &[TraceEvent]) -> u64 {
    events.last().map_or(1, |event| event.event_id + 1)
}

#[cfg(test)]
mod tests;
