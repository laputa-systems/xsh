//! AST -> lowered-IR lowering pass, split out of `eval.rs`.

use crate::modules::{ModuleFnSig, RuntimeOp, api_spec};
use crate::runtime::value::{DurationValue, PathValue, RecordMap, RegexValue, RuntimeError, Value};
use crate::sema::check::{
    CheckedApiCall, CompactBodyFacts, CompactDeclOutput, CompactTypeDefInfo, Conversion,
};
use crate::sema::records::standard_record_type;
use crate::sema::types::{CallableParamType, ModuleExportType, Type};
use crate::source::{SourceMap, Span};
use crate::symbol::{Name, QualifiedName, Symbol};
use crate::syntax::arena::DeferTrigger;
use crate::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaBuilderEntryKind, ArenaCallArg,
    ArenaCallArgKind, ArenaCommand, ArenaCommandArgKind, ArenaExprKind, ArenaExprOrRun,
    ArenaFmtPart, ArenaPatternKind, ArenaProgram, ArenaRecordFieldKind, ArenaSpawnTarget,
    ArenaStmtKind, ArenaStreamStage, ArenaTypeExprTag, ArenaWordPart, AstArena, BindingTargetId,
    BlockId, ExprId, FunctionDefId, PatternId, StmtId, TypeExprId,
};
use crate::syntax::node::{
    AssignOp, BinaryOp, CommandWordRefSegment, CoreCommand, RunKind, StreamStageKind, UnaryOp,
    parse_command_word_reference,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;
use xsh_registry::types::BuiltinTypeName;

use super::lowered_ops::{
    lowered_binary_op, lowered_value_from_runtime_any, lowered_value_matches,
};
use super::{
    LoweredProcessCommandArgv, LoweredProcessCommandBuilderEntry, LoweredRecordEntry,
    LoweredRecordUpdates, LoweredRunCapture, LoweredSpawnRun, lowered_record_vec_get,
    lowered_record_vec_get_mut, lowered_record_vec_insert,
};

/// Name -> dense slot-index map used while lowering a function or top-level
/// region. Slots are allocated densely and never reused; `high_water` is the
/// runtime slot-array size, so retired names do not need synthetic keys.
#[derive(Clone, Default)]
pub(super) struct SlotScope {
    pattern_slots: Option<FxHashMap<Name, usize>>,
    // Slots declared for pattern captures; slots are never reused.
    pattern_capture_slots: FxHashSet<usize>,
    indices: FxHashMap<Name, usize>,
    // Guarded receivers and explicit pipeline inputs bind once before their
    // selected operation; exact expression IDs reuse that retained value.
    postfix_receivers: FxHashMap<ExprId, BuildExprId>,
    guarded_postfixes: FxHashSet<ExprId>,
    // The condition operand being lowered under the propagation the checker
    // decided for it, so the operand itself is lowered once, unwrapped.
    propagated_condition: Option<ExprId>,
    /// The `?` written on a deferred call. `defer f()?` is `defer f()`, so its
    /// failure is placed at the call, as the bare form's is.
    deferred_propagation: Option<ExprId>,
    bound_call_entries: FxHashSet<ExprId>,
    types: FxHashMap<Name, Type>,
    captures: FxHashSet<Name>,
    // (name, previous slot) for each block-local declaration, so `exit` can
    // restore a shadowed outer binding (or drop a freshly-introduced one).
    declared: Vec<(Name, Option<usize>, Option<Type>, bool)>,
    // Index into `declared` where the innermost scope began: what follows
    // belongs to the scope being lowered now, and anything older is an
    // enclosing scope this one may shadow.
    level_start: usize,
    high_water: usize,
}

/// Snapshot of a `SlotScope` taken on entering a nested block (see `enter`/`exit`).
pub(super) struct SlotSnapshot {
    declared_len: usize,
    level_start: usize,
    high_water: usize,
}

/// Source-ordered bindings and one lowered value for each static expansion entry.
pub(super) struct LoweredArgumentValues {
    pub(super) values: Vec<BuildExprId>,
    pub(super) bindings: Vec<(BuildExprId, usize)>,
}

struct LoweredFsFilesArgs {
    root: ExprId,
    gitignore: Option<ExprId>,
    stat: Option<ExprId>,
    hidden: Option<ExprId>,
    exts: Option<ExprId>,
}

struct LoweredFsListArgs {
    path: ExprId,
    stat: Option<ExprId>,
    ordered: Option<ExprId>,
}

struct LoweredArchiveTarCreateArgs {
    path: ExprId,
    root: ExprId,
    entries: ExprId,
    compression: Option<ExprId>,
    overwrite: Option<ExprId>,
}

struct LoweredPathMkdirArgs {
    parents: Option<ExprId>,
}

struct LoweredPathRemoveArgs {
    missing_ok: Option<ExprId>,
}

struct LoweredPathWriteArgs {
    data: ExprId,
}

/// The checker's binding of a registered call's source entries to the
/// parameters of its selected overload.
struct CheckedApiArguments {
    sig: &'static ModuleFnSig,
    values: Vec<Option<ExprId>>,
}

impl CheckedApiArguments {
    fn get(&self, name: &str) -> Option<ExprId> {
        self.values[self
            .sig
            .params
            .iter()
            .position(|param| param.name == name)?]
    }

    /// Parameter-ordered values with absent trailing defaults dropped.
    fn ordered(&self) -> Vec<Option<ExprId>> {
        let mut values = self.values.clone();
        while values.last().is_some_and(Option::is_none) {
            values.pop();
        }
        values
    }
}

struct LoweredModuleCallArgs {
    semantic_rule: crate::modules::signature::SemanticRule,
    op: RuntimeOp,
    args: Vec<Option<ExprId>>,
}

struct LoweredHashVerifyFileArgs {
    path: ExprId,
    /// The algorithm the checksum argument's name selected.
    algorithm: &'static str,
    expected: ExprId,
}

struct LoweredProcessCommandArgvArgs {
    target: ExprId,
    argv: ExprId,
    cwd: Option<ExprId>,
    env: Option<ExprId>,
    stdin: Option<ExprId>,
    stdout: Option<ExprId>,
    stderr: Option<ExprId>,
    stdout_append: Option<ExprId>,
    stderr_append: Option<ExprId>,
    timeout: Option<ExprId>,
    detach: Option<ExprId>,
    new_session: Option<ExprId>,
    ignore_hup: Option<ExprId>,
    cpu_max: Option<ExprId>,
    accept: Option<ExprId>,
    same_group: Option<ExprId>,
}

fn build_expr(scratch: &Rc<RefCell<BuildScratch>>, row: BuildExprRow) -> BuildExprId {
    scratch.borrow_mut().expr(row)
}

macro_rules! push_build_row {
    ($self:expr, expr, $row:expr) => {{
        let row = $row;
        $self.scratch.borrow_mut().expr(row)
    }};
    ($self:expr, stmt, $row:expr) => {{
        let row = $row;
        $self.scratch.borrow_mut().stmt(row)
    }};
    ($self:expr, pattern, $row:expr) => {{
        let row = $row;
        $self.scratch.borrow_mut().pattern(row)
    }};
    ($self:expr, int, $row:expr) => {{
        let row = $row;
        $self.scratch.borrow_mut().int(row)
    }};
    ($self:expr, bool, $row:expr) => {{
        let row = $row;
        $self.scratch.borrow_mut().bool(row)
    }};
}

mod api;
mod arguments;
mod calls;
mod commands;
mod control;
mod names;
mod patterns;
mod pipeline;
mod scalar;
mod types;
mod values;

use api::{
    positional_call_args, single_positional_arena_call_arg, lowered_str_byte_op,
    script_argument_slots, lower_process_command_argv_args, lowered_module_call_args,
    lower_hash_verify_file_args, compact_call_arg_expr, lower_archive_tar_create_args,
    lower_path_mkdir_args, lower_path_remove_args, lower_path_write_args, lower_fs_files_args,
    lower_fs_list_args,
};

use commands::{
    lower_command_word_reference, lowered_arena_run_capture_type,
};

use types::{
    lowered_arena_type_inner, stream_item_type, compact_pattern_test_type,
    compact_runtime_type_in_namespace, compact_type_check, lowered_type_needs_static_check,
    compact_type_expr_name, guard_bound_type, record_binding_types, simple_binding_target,
    is_discard_name, lowered_checked_type, type_for_lowered_type,
};

#[cfg(debug_assertions)]
use types::checked_fact_is_resolved;

use values::{
    lower_const_param_default,
};


impl SlotScope {
    /// Build a scope from ordered binding names (function params, top-level slots).
    pub(super) fn from_names<I: IntoIterator<Item = Name>>(names: I) -> Self {
        let indices = names
            .into_iter()
            .enumerate()
            .map(|(slot, name)| (name, slot))
            .collect::<FxHashMap<_, _>>();
        let high_water = indices.len();
        Self {
            indices,
            pattern_slots: None,
            pattern_capture_slots: FxHashSet::default(),
            postfix_receivers: FxHashMap::default(),
            guarded_postfixes: FxHashSet::default(),
            propagated_condition: None,
            deferred_propagation: None,
            bound_call_entries: FxHashSet::default(),
            types: FxHashMap::default(),
            captures: FxHashSet::default(),
            declared: Vec::new(),
            level_start: 0,
            high_water,
        }
    }

    /// High-water slot total: the size the runtime slot array must have.
    pub(super) fn count(&self) -> usize {
        self.high_water
    }

    pub(super) fn resolve(&self, name: Name) -> Option<usize> {
        self.indices.get(&name).copied()
    }

    pub(super) fn is_bound(&self, name: Name) -> bool {
        self.indices.contains_key(&name)
    }

    fn is_bound_non_capture(&self, name: Name) -> bool {
        self.is_bound(name) && !self.captures.contains(&name)
    }

    fn can_bind_pattern(&self, name: Name) -> bool {
        // Retired sibling-arm captures may be reused. A capture may shadow an
        // outer name or a declaration of this scope, which resolves again when
        // the capture retires, but not another live capture.
        self.pattern_slots
            .as_ref()
            .is_some_and(|slots| slots.contains_key(&name))
            || !self.is_bound_non_capture(name)
            || !self.is_declared_here(name)
            || self
                .resolve(name)
                .is_some_and(|slot| !self.pattern_capture_slots.contains(&slot))
    }

    fn declare_pattern_binding(&mut self, name: Name) -> usize {
        self.pattern_slots
            .as_ref()
            .and_then(|slots| slots.get(&name))
            .copied()
            .unwrap_or_else(|| self.declare_pattern_capture(name))
    }

    fn declare_pattern_capture(&mut self, name: Name) -> usize {
        let slot = self.declare(name);
        self.pattern_capture_slots.insert(slot);
        slot
    }

    /// Whether the innermost scope already declared `name`.
    fn is_declared_here(&self, name: Name) -> bool {
        self.declared[self.level_start..]
            .iter()
            .any(|(declared, ..)| *declared == name)
    }

    fn binding_type(&self, name: Name) -> Option<&Type> {
        self.types.get(&name)
    }

    /// Allocate the next dense slot and bind `name` to it.
    pub(super) fn declare(&mut self, name: Name) -> usize {
        self.declare_with_type(name, None)
    }

    fn declare_with_type(&mut self, name: Name, ty: Option<Type>) -> usize {
        let slot = self.high_water;
        self.high_water += 1;
        let previous = self.indices.insert(name, slot);
        let previous_capture = self.captures.remove(&name);
        let previous_ty = match ty {
            Some(ty) => self.types.insert(name, ty),
            None => self.types.remove(&name),
        };
        self.declared
            .push((name, previous, previous_ty, previous_capture));
        slot
    }

    fn declare_capture(&mut self, name: Name) -> usize {
        // A captured top-level binding is visible to the function but was not
        // declared in its body, so a local declaration may shadow it.
        let slot = self.high_water;
        self.high_water += 1;
        self.indices.insert(name, slot);
        self.captures.insert(name);
        slot
    }

    /// Allocate the next dense slot without binding a source-visible name.
    pub(super) fn reserve(&mut self, _tag: &str) -> usize {
        let slot = self.high_water;
        self.high_water += 1;
        slot
    }

    /// Drop `name` from resolution while keeping its slot reserved by `high_water`.
    ///
    /// A retired pattern capture also leaves the scope's declarations, so a
    /// later `let` of the same name is not mistaken for a redeclaration, and
    /// any binding it shadowed resolves again.
    pub(super) fn retire(&mut self, name: Name, slot: usize, _tag: &str) {
        let position = self
            .declared
            .iter()
            .rposition(|(declared, ..)| *declared == name);
        match position {
            Some(position)
                if position >= self.level_start && self.indices.get(&name) == Some(&slot) =>
            {
                let (_, previous, previous_ty, previous_capture) = self.declared.remove(position);
                match previous {
                    Some(previous) => self.indices.insert(name, previous),
                    None => self.indices.remove(&name),
                };
                match previous_ty {
                    Some(ty) => self.types.insert(name, ty),
                    None => self.types.remove(&name),
                };
                if previous_capture {
                    self.captures.insert(name);
                }
            }
            _ => {
                self.indices.remove(&name);
            }
        }
    }

    /// Snapshot bindings on entering a nested block scope.
    ///
    /// The snapshot restores the state `enter` observed; the scope it opens
    /// owns every declaration made from here until `exit`.
    pub(super) fn enter(&mut self) -> SlotSnapshot {
        let snapshot = SlotSnapshot {
            declared_len: self.declared.len(),
            level_start: self.level_start,
            high_water: self.high_water,
        };
        self.level_start = self.declared.len();
        snapshot
    }

    /// Restore name resolution to the block-entry snapshot, dropping block-local
    /// bindings while keeping every slot index allocated inside the block.
    /// A block-local declaration that shadowed an outer binding restores the
    /// outer slot; a freshly-introduced one is dropped.
    pub(super) fn exit(&mut self, snapshot: SlotSnapshot) {
        self.level_start = snapshot.level_start;
        for (name, previous, previous_ty, previous_capture) in
            self.declared[snapshot.declared_len..].iter().rev()
        {
            match previous {
                Some(slot) => {
                    self.indices.insert(*name, *slot);
                }
                None => {
                    self.indices.remove(name);
                }
            }
            match previous_ty {
                Some(ty) => {
                    self.types.insert(*name, ty.clone());
                }
                None => {
                    self.types.remove(name);
                }
            }
            if *previous_capture {
                self.captures.insert(*name);
            } else {
                self.captures.remove(name);
            }
        }
        self.declared.truncate(snapshot.declared_len);
        self.high_water = self.high_water.max(snapshot.high_water);
    }

    /// Consume the scope, yielding `(name, slot)` entries (top-level slot metadata).
    pub(super) fn into_entries(self) -> impl Iterator<Item = (Name, usize)> {
        self.indices.into_iter()
    }
}
use super::{
    BuildBoolId, BuildBoolRow, BuildExprId, BuildExprRow, BuildIntId, BuildIntRow, BuildPatternId,
    BuildPatternIdSlots, BuildPatternRow, BuildScratch, BuildStmtId, BuildStmtRow, BuildTopKind,
    BuildTopStmtId, BuildTopStmtRow, COMPACT_CALL_BLOCKER_KIND_COUNT,
    COMPACT_COMMAND_BLOCKER_KIND_COUNT, COMPACT_EXPR_KIND_COUNT, COMPACT_STMT_KIND_COUNT,
    COMPACT_TYPE_EXPR_TAG_COUNT, CompactLowerConstructProbeOutput, Flow, FunctionBuild,
    LowerableFunctions, LoweredAssignPath, LoweredAssignStep, LoweredCallArg, LoweredCompFields,
    LoweredCompTarget, LoweredErrorExpr, LoweredErrorPatternFields, LoweredFmtPart,
    LoweredFunctionBlocker, LoweredFunctionKey, LoweredFunctionKind, LoweredFunctionUnit,
    LoweredModuleExport, LoweredModuleExportKind, LoweredParamChecks, LoweredParamDefaults,
    LoweredParamKinds, LoweredParamNames, LoweredParamRest, LoweredPipelineStage,
    LoweredReturnKind, LoweredRunArg, LoweredRunArgKind, LoweredRunEnv, LoweredRunPipelineSegment,
    LoweredRunRedirection, LoweredStrPredicate, LoweredTopLevelBinding, LoweredTopLevelSlot,
    LoweredTopLevelSlots, LoweredType, LoweredTypeCheck, LoweredValue, ProgramBuild, ReduceByOp,
    ScanBytes, ScanCheck, ScanCondition, StmtFlow, lowered_method_name,
};

pub(super) fn lowered_arena_type(
    arena: &AstArena,
    ty: TypeExprId,
    declarations: &CompactDeclOutput,
) -> Option<LoweredType> {
    lowered_arena_type_inner(arena, ty, declarations, 0)
}

pub(super) fn probe_compact_lower_constructed_bodies(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    source: &str,
) -> CompactLowerConstructProbeOutput {
    let mut probe = CompactLowerConstructProbe {
        program,
        declarations,
        bodies: &declarations.bodies,
        source,
        sources: None,
        current_namespace: None,
        functions: None,
        top_level_known: FxHashMap::default(),
        output: CompactLowerConstructProbeOutput {
            expr_type_facts: declarations.bodies.expr_types.len(),
            ..CompactLowerConstructProbeOutput::default()
        },
        last_blocker_detail: None,
        stdlib_linkage: StdlibLowerLinkage::Local,
        function_defs: Rc::new(RefCell::new(None)),
        scratch: Rc::new(RefCell::new(BuildScratch::default())),
        spread_programs: Rc::default(),
    };
    probe.probe_program();
    probe.output
}

/// Where a program's standard-library implementation calls resolve.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum StdlibLowerLinkage {
    /// The program contains the embedded implementation modules it calls.
    Local,
    /// A loading program already prepared them; this program lowers its
    /// standard calls to functions resolved through the runtime's dynamic
    /// function table instead of reparsing embedded source.
    External,
}

pub(super) fn lower_compact_function_units_into(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    source: &str,
    sources: &SourceMap,
    stdlib_linkage: StdlibLowerLinkage,
    mut emit: impl FnMut(LoweredFunctionUnit) -> Result<(), super::indexed::IrBuildError>,
) -> Result<(), super::indexed::IrBuildError> {
    // Every function's body must be walked to find its call edges, and finding
    // them once per function would make preparation quadratic in program size.
    // One index answers the whole loop: each unit's dependencies and its
    // component metadata come from the same walk of every body.
    let index = Rc::new(CompactFunctionIndex::new(program));
    let candidates = index
        .defs
        .iter()
        .map(|function| function.key)
        .collect::<Vec<_>>();
    let function_defs = Rc::new(RefCell::new(Some(Rc::clone(&index))));
    let spread_programs = Rc::default();
    let empty_pures = FxHashSet::default();
    let empty_procs = FxHashSet::default();
    let empty_qualified_pures = FxHashSet::default();
    let empty_qualified_procs = FxHashSet::default();
    let functions = LowerableFunctions::all_with_candidates(
        &empty_pures,
        &empty_procs,
        &empty_qualified_pures,
        &empty_qualified_procs,
        &candidates,
    );
    // `top_level_known` is a prefix scan of a module's statements, and the
    // functions of one module come out of the index in statement order, so one
    // cursor per module produces every function's prefix in a single pass. A
    // fresh scan per function would make preparation quadratic in a module's
    // size.
    let recorder = CompactLowerConstructProbe {
        program,
        declarations,
        bodies: &declarations.bodies,
        source,
        sources: Some(sources),
        current_namespace: None,
        functions: Some(&functions),
        top_level_known: FxHashMap::default(),
        output: CompactLowerConstructProbeOutput::default(),
        last_blocker_detail: None,
        stdlib_linkage,
        function_defs: Rc::clone(&function_defs),
        scratch: Rc::new(RefCell::new(BuildScratch::default())),
        spread_programs: Rc::clone(&spread_programs),
    };
    let mut prefixes: FxHashMap<Option<Name>, CompactTopLevelPrefix> = FxHashMap::default();
    for function in index.defs.iter().copied() {
        let prefix = prefixes
            .entry(function.namespace)
            .or_insert_with(|| CompactTopLevelPrefix::for_namespace(program, function.namespace));
        prefix.advance_to_recording(&recorder, function.id);
        let top_level_known = prefix.known.clone();
        let mut probe = CompactLowerConstructProbe {
            program,
            declarations,
            bodies: &declarations.bodies,
            source,
            sources: Some(sources),
            current_namespace: function.namespace,
            functions: Some(&functions),
            top_level_known,
            output: CompactLowerConstructProbeOutput::default(),
            last_blocker_detail: None,
            stdlib_linkage,
            function_defs: Rc::clone(&function_defs),
            scratch: Rc::new(RefCell::new(BuildScratch::default())),
            spread_programs: Rc::clone(&spread_programs),
        };
        let (scc_member_count, scc_group) = index.scc_metadata(&function);
        let unit = probe.lower_function_unit(
            function,
            compact_function_dependency_keys(&index, program, &function),
            scc_member_count,
            scc_group,
        );
        emit(unit)?;
    }
    Ok(())
}

pub(super) fn lower_compact_top_level_program_with_probe(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    source: &str,
    sources: &SourceMap,
    functions: &LowerableFunctions<'_>,
) -> (ProgramBuild, CompactLowerConstructProbeOutput) {
    let mut probe = CompactLowerConstructProbe {
        program,
        declarations,
        bodies: &declarations.bodies,
        source,
        sources: Some(sources),
        current_namespace: None,
        functions: Some(functions),
        top_level_known: compact_top_level_known(
            program,
            declarations,
            source,
            Some(sources),
            None,
            Some(functions),
        ),
        output: CompactLowerConstructProbeOutput::default(),
        last_blocker_detail: None,
        stdlib_linkage: StdlibLowerLinkage::Local,
        function_defs: Rc::new(RefCell::new(None)),
        scratch: Rc::new(RefCell::new(BuildScratch::default())),
        spread_programs: Rc::default(),
    };
    let root = program.statement_ids().collect::<Vec<_>>();
    let lowered = probe.lower_program_statements(&root);
    (lowered, probe.output)
}

fn compact_top_level_known(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    source: &str,
    sources: Option<&SourceMap>,
    namespace: Option<Name>,
    functions: Option<&LowerableFunctions<'_>>,
) -> FxHashMap<Name, LoweredTopLevelBinding> {
    let statements = match namespace {
        Some(namespace) => program
            .modules
            .iter()
            .find(|module| module.name == namespace)
            .map(|module| program.module_statements(module).collect::<Vec<_>>())
            .unwrap_or_default(),
        None => program.statement_ids().collect::<Vec<_>>(),
    };
    let probe = CompactLowerConstructProbe {
        program,
        declarations,
        bodies: &declarations.bodies,
        source,
        sources,
        current_namespace: namespace,
        functions,
        top_level_known: FxHashMap::default(),
        output: CompactLowerConstructProbeOutput::default(),
        last_blocker_detail: None,
        stdlib_linkage: StdlibLowerLinkage::Local,
        function_defs: Rc::new(RefCell::new(None)),
        scratch: Rc::new(RefCell::new(BuildScratch::default())),
        spread_programs: Rc::default(),
    };
    probe.collect_top_level_known(&statements)
}

/// The bindings visible to one function definition.
///
/// Used by the construct probe, which walks a module's statements itself and so
/// cannot share the cursor `lower_compact_function_units_into` keeps.
fn compact_function_top_level_known(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    source: &str,
    sources: Option<&SourceMap>,
    namespace: Option<Name>,
    function_id: FunctionDefId,
    functions: Option<&LowerableFunctions<'_>>,
) -> FxHashMap<Name, LoweredTopLevelBinding> {
    let probe = CompactLowerConstructProbe {
        program,
        declarations,
        bodies: &declarations.bodies,
        source,
        sources,
        current_namespace: namespace,
        functions,
        top_level_known: FxHashMap::default(),
        output: CompactLowerConstructProbeOutput::default(),
        last_blocker_detail: None,
        stdlib_linkage: StdlibLowerLinkage::Local,
        function_defs: Rc::new(RefCell::new(None)),
        scratch: Rc::new(RefCell::new(BuildScratch::default())),
        spread_programs: Rc::default(),
    };
    let mut prefix = CompactTopLevelPrefix::for_namespace(program, namespace);
    prefix.advance_to_recording(&probe, function_id);
    prefix.known
}

/// One module's `top_level_known` prefix, advanced as its functions are lowered.
///
/// The bindings visible to a function are those of the statements before its own
/// definition, so the functions of a module share one scan: the cursor stops at
/// the definition statement it was asked about and every later function resumes
/// from there.
struct CompactTopLevelPrefix {
    statements: Vec<StmtId>,
    known: FxHashMap<Name, LoweredTopLevelBinding>,
    position: usize,
}

impl CompactTopLevelPrefix {
    fn for_namespace(program: &ArenaProgram, namespace: Option<Name>) -> Self {
        let statements = match namespace {
            Some(namespace) => program
                .modules
                .iter()
                .find(|module| module.name == namespace)
                .map(|module| program.module_statements(module).collect::<Vec<_>>())
                .unwrap_or_default(),
            None => program.statement_ids().collect::<Vec<_>>(),
        };
        Self {
            statements,
            known: top_level_known_with_runtime_bindings(),
            position: 0,
        }
    }

    /// Record every statement before `function_id`'s own definition.
    fn advance_to_recording(
        &mut self,
        recorder: &CompactLowerConstructProbe<'_, '_>,
        function_id: FunctionDefId,
    ) {
        while let Some(stmt) = self.statements.get(self.position).copied() {
            if compact_stmt_contains_function_def(recorder.program, stmt, function_id) {
                break;
            }
            recorder.record_top_level_binding(stmt, &mut self.known);
            self.position += 1;
        }
    }
}

fn compact_stmt_contains_function_def(
    program: &ArenaProgram,
    stmt: StmtId,
    function_id: FunctionDefId,
) -> bool {
    match program.arena.stmt(stmt).kind {
        ArenaStmtKind::Export(inner) => {
            compact_stmt_contains_function_def(program, inner, function_id)
        }
        ArenaStmtKind::PureDef(id)
        | ArenaStmtKind::ProcDef(id)
        | ArenaStmtKind::CliMain(id)
        | ArenaStmtKind::StreamDef(id) => id == function_id,
        _ => false,
    }
}

/// Every function definition in a program, with its key index and the strongly
/// connected component metadata for each purity class.
///
/// Building any of this walks function bodies, so it is done once per lowering
/// pass rather than once per function.
struct CompactFunctionIndex {
    defs: Vec<CompactFunctionDef>,
    index_of: FxHashMap<LoweredFunctionKey, usize>,
    /// `[pure, proc]` component metadata keyed by function.
    scc: [FxHashMap<LoweredFunctionKey, (usize, Option<usize>)>; 2],
}

impl CompactFunctionIndex {
    fn new(program: &ArenaProgram) -> Self {
        let defs = compact_function_defs(program);
        let index_of = defs
            .iter()
            .enumerate()
            .map(|(index, function)| (function.key, index))
            .collect::<FxHashMap<_, _>>();
        let scc = [
            compact_scc_groups(program, &defs, true),
            compact_scc_groups(program, &defs, false),
        ];
        Self {
            defs,
            index_of,
            scc,
        }
    }

    fn scc_metadata(&self, function: &CompactFunctionDef) -> (usize, Option<usize>) {
        let class = usize::from(!function.pure);
        self.scc[class]
            .get(&function.key)
            .copied()
            .unwrap_or((1, None))
    }

    fn definition(&self, key: LoweredFunctionKey) -> Option<&CompactFunctionDef> {
        self.index_of
            .get(&key)
            .and_then(|index| self.defs.get(*index))
    }
}

/// Component metadata for every function of one purity.
///
/// A recursive group is reported with its member count so the runtime can size
/// the frame stack for it; a solitary function reports `(1, None)`.
fn compact_scc_groups(
    program: &ArenaProgram,
    defs: &[CompactFunctionDef],
    pure: bool,
) -> FxHashMap<LoweredFunctionKey, (usize, Option<usize>)> {
    let selected = defs
        .iter()
        .filter(|candidate| candidate.pure == pure)
        .collect::<Vec<_>>();
    let index_of = selected
        .iter()
        .enumerate()
        .map(|(index, function)| (function.key, index))
        .collect::<FxHashMap<_, _>>();
    let adjacency = selected
        .iter()
        .map(|function| {
            compact_function_call_edges(program, function.id, function.namespace, &index_of)
        })
        .collect::<Vec<_>>();
    let mut groups = FxHashMap::default();
    for (group, scc) in compact_tarjan_sccs(adjacency).into_iter().enumerate() {
        let metadata = if scc.len() > 1 {
            (scc.len(), Some(group))
        } else {
            (1, None)
        };
        for member in scc {
            groups.insert(selected[member].key, metadata);
        }
    }
    groups
}

#[derive(Clone, Copy)]
struct CompactFunctionDef {
    key: LoweredFunctionKey,
    id: FunctionDefId,
    pure: bool,
    namespace: Option<Name>,
    definition_span: Span,
}

fn compact_function_call_edges(
    program: &ArenaProgram,
    id: FunctionDefId,
    namespace: Option<Name>,
    index_of: &FxHashMap<LoweredFunctionKey, usize>,
) -> Vec<usize> {
    let mut edges = Vec::new();
    compact_collect_block_call_edges(
        program,
        program.arena.function_def(id).body,
        namespace,
        index_of,
        &mut edges,
    );
    edges.sort_unstable();
    edges.dedup();
    edges
}

fn compact_function_dependency_keys(
    functions: &CompactFunctionIndex,
    program: &ArenaProgram,
    function: &CompactFunctionDef,
) -> Vec<LoweredFunctionKey> {
    compact_function_call_edges(
        program,
        function.id,
        function.namespace,
        &functions.index_of,
    )
    .into_iter()
    .map(|index| functions.defs[index].key)
    .collect()
}

fn compact_collect_block_call_edges(
    program: &ArenaProgram,
    block: BlockId,
    namespace: Option<Name>,
    index_of: &FxHashMap<LoweredFunctionKey, usize>,
    edges: &mut Vec<usize>,
) {
    for stmt in program
        .arena
        .stmt_ids(program.arena.block(block).statements)
    {
        compact_collect_stmt_call_edges(program, stmt, namespace, index_of, edges);
    }
}

fn compact_collect_expr_or_run_call_edges(
    program: &ArenaProgram,
    value: ArenaExprOrRun,
    namespace: Option<Name>,
    index_of: &FxHashMap<LoweredFunctionKey, usize>,
    edges: &mut Vec<usize>,
) {
    if let ArenaExprOrRun::Expr(expr) = value {
        compact_collect_expr_call_edges(program, expr, namespace, index_of, edges);
    }
}

fn compact_collect_stmt_call_edges(
    program: &ArenaProgram,
    id: StmtId,
    namespace: Option<Name>,
    index_of: &FxHashMap<LoweredFunctionKey, usize>,
    edges: &mut Vec<usize>,
) {
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Export(inner)
        | ArenaStmtKind::Sugar {
            expansion: inner, ..
        } => compact_collect_stmt_call_edges(program, inner, namespace, index_of, edges),
        ArenaStmtKind::Let { initializer, .. }
        | ArenaStmtKind::Const { initializer, .. }
        | ArenaStmtKind::Var { initializer, .. }
        | ArenaStmtKind::Defer(initializer, _)
        | ArenaStmtKind::Yield(initializer) => {
            compact_collect_expr_or_run_call_edges(
                program,
                initializer,
                namespace,
                index_of,
                edges,
            );
        }
        ArenaStmtKind::Assign { value, .. } => {
            compact_collect_expr_or_run_call_edges(program, value, namespace, index_of, edges);
        }
        ArenaStmtKind::Return(Some(value)) => {
            compact_collect_expr_or_run_call_edges(program, value, namespace, index_of, edges);
        }
        ArenaStmtKind::If {
            branches,
            else_block,
        } => {
            for branch in program.arena.if_branches(branches) {
                compact_collect_expr_call_edges(
                    program,
                    branch.condition,
                    namespace,
                    index_of,
                    edges,
                );
                compact_collect_block_call_edges(program, branch.block, namespace, index_of, edges);
            }
            if let Some(block) = else_block {
                compact_collect_block_call_edges(program, block, namespace, index_of, edges);
            }
        }
        ArenaStmtKind::While { condition, block } => {
            compact_collect_expr_call_edges(program, condition, namespace, index_of, edges);
            compact_collect_block_call_edges(program, block, namespace, index_of, edges);
        }
        ArenaStmtKind::For { iter, block, .. } => {
            compact_collect_expr_call_edges(program, iter, namespace, index_of, edges);
            compact_collect_block_call_edges(program, block, namespace, index_of, edges);
        }
        ArenaStmtKind::Loop { block } => {
            compact_collect_block_call_edges(program, block, namespace, index_of, edges);
        }
        ArenaStmtKind::With {
            bindings,
            body,
            else_block,
        } => {
            for binding in program.arena.with_bindings(bindings) {
                compact_collect_expr_call_edges(
                    program,
                    binding.initializer,
                    namespace,
                    index_of,
                    edges,
                );
            }
            compact_collect_block_call_edges(program, body, namespace, index_of, edges);
            compact_collect_block_call_edges(program, else_block, namespace, index_of, edges);
        }
        ArenaStmtKind::Guard {
            initializer,
            else_block,
            ..
        } => {
            compact_collect_expr_or_run_call_edges(
                program,
                initializer,
                namespace,
                index_of,
                edges,
            );
            compact_collect_block_call_edges(program, else_block, namespace, index_of, edges);
        }
        ArenaStmtKind::Assert { condition, message } => {
            compact_collect_expr_call_edges(program, condition, namespace, index_of, edges);
            if let Some(message) = message {
                compact_collect_expr_call_edges(program, message, namespace, index_of, edges);
            }
        }
        ArenaStmtKind::Break { value: Some(value) }
        | ArenaStmtKind::Expr(value)
        | ArenaStmtKind::Exit(value)
        | ArenaStmtKind::YieldDelegate(value) => {
            compact_collect_expr_call_edges(program, value, namespace, index_of, edges);
        }
        ArenaStmtKind::Match { value, arms } => {
            compact_collect_expr_call_edges(program, value, namespace, index_of, edges);
            for arm in program.arena.match_arms(arms) {
                if let Some(guard) = arm.guard {
                    compact_collect_expr_call_edges(program, guard, namespace, index_of, edges);
                }
                compact_collect_block_call_edges(program, arm.block, namespace, index_of, edges);
            }
        }
        _ => {}
    }
}

fn compact_collect_expr_call_edges(
    program: &ArenaProgram,
    id: ExprId,
    namespace: Option<Name>,
    index_of: &FxHashMap<LoweredFunctionKey, usize>,
    edges: &mut Vec<usize>,
) {
    match program.arena.expr(id).kind {
        ArenaExprKind::Call { callee, args } => {
            if let ArenaExprKind::Ident(name) = program.arena.expr(callee).kind
                && let Some(index) = index_of.get(&compact_function_key(namespace, name))
            {
                edges.push(*index);
            }
            compact_collect_expr_call_edges(program, callee, namespace, index_of, edges);
            for arg in program.arena.call_args(args) {
                match arg.kind {
                    ArenaCallArgKind::Positional(expr)
                    | ArenaCallArgKind::Splice { value: expr, .. }
                    | ArenaCallArgKind::NamedSpread { value: expr, .. }
                    | ArenaCallArgKind::Named { value: expr, .. } => {
                        compact_collect_expr_call_edges(program, expr, namespace, index_of, edges);
                    }
                }
            }
        }
        ArenaExprKind::List(items) => {
            for item in program.arena.list_element_exprs(items) {
                compact_collect_expr_call_edges(program, item, namespace, index_of, edges);
            }
        }
        ArenaExprKind::Record(fields) => {
            for field in program.arena.record_fields(fields) {
                match field.kind {
                    ArenaRecordFieldKind::Computed { key, value, .. } => {
                        compact_collect_expr_call_edges(program, key, namespace, index_of, edges);
                        compact_collect_expr_call_edges(program, value, namespace, index_of, edges);
                    }
                    ArenaRecordFieldKind::Path { value, .. }
                    | ArenaRecordFieldKind::Named { value, .. }
                    | ArenaRecordFieldKind::Spread { expr: value, .. } => {
                        compact_collect_expr_call_edges(program, value, namespace, index_of, edges);
                    }
                    ArenaRecordFieldKind::Shorthand { .. } => {}
                }
            }
        }
        ArenaExprKind::If {
            branches,
            else_value,
        } => {
            for branch in program.arena.if_expr_branches(branches) {
                compact_collect_expr_call_edges(
                    program,
                    branch.condition,
                    namespace,
                    index_of,
                    edges,
                );
                compact_collect_expr_call_edges(program, branch.value, namespace, index_of, edges);
            }
            compact_collect_expr_call_edges(program, else_value, namespace, index_of, edges);
        }
        ArenaExprKind::Match { value, arms }
        | ArenaExprKind::PatternTest { value, arms }
        | ArenaExprKind::PatternCondition { value, arms } => {
            compact_collect_expr_call_edges(program, value, namespace, index_of, edges);
            for arm in program.arena.match_expr_arms(arms) {
                if let Some(guard) = arm.guard {
                    compact_collect_expr_call_edges(program, guard, namespace, index_of, edges);
                }
                compact_collect_expr_call_edges(program, arm.value, namespace, index_of, edges);
            }
        }
        ArenaExprKind::Unary { expr, .. }
        | ArenaExprKind::Try(expr)
        | ArenaExprKind::Require { value: expr, .. }
        | ArenaExprKind::Convert { value: expr, .. }
        | ArenaExprKind::Field { base: expr, .. }
        | ArenaExprKind::NullSafeField { base: expr, .. } => {
            compact_collect_expr_call_edges(program, expr, namespace, index_of, edges);
        }
        ArenaExprKind::ValuePipelineCall { input, call, .. } => {
            compact_collect_expr_call_edges(program, input, namespace, index_of, edges);
            compact_collect_expr_call_edges(program, call, namespace, index_of, edges);
        }
        ArenaExprKind::ComparisonChain(pairs) => {
            for pair in program.arena.comparison_chain_operands(pairs) {
                compact_collect_expr_call_edges(program, pair, namespace, index_of, edges);
            }
        }
        ArenaExprKind::Binary { left, right, .. }
        | ArenaExprKind::Index {
            base: left,
            index: right,
            ..
        } => {
            compact_collect_expr_call_edges(program, left, namespace, index_of, edges);
            compact_collect_expr_call_edges(program, right, namespace, index_of, edges);
        }
        ArenaExprKind::Slice {
            base, start, end, ..
        } => {
            compact_collect_expr_call_edges(program, base, namespace, index_of, edges);
            if let Some(start) = start {
                compact_collect_expr_call_edges(program, start, namespace, index_of, edges);
            }
            if let Some(end) = end {
                compact_collect_expr_call_edges(program, end, namespace, index_of, edges);
            }
        }
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
            for part in program.arena.fmt_parts(parts) {
                if let ArenaFmtPart::Expr(expr, _) = part {
                    compact_collect_expr_call_edges(program, expr, namespace, index_of, edges);
                }
            }
        }
        ArenaExprKind::Set(items) => {
            for item in program.arena.list_element_exprs(items) {
                compact_collect_expr_call_edges(program, item, namespace, index_of, edges);
            }
        }
        ArenaExprKind::ListComp { expr, qualifiers }
        | ArenaExprKind::SetComp { expr, qualifiers } => {
            for qualifier in program.arena.comp_qualifiers(qualifiers) {
                compact_collect_expr_call_edges(
                    program,
                    qualifier.expr(),
                    namespace,
                    index_of,
                    edges,
                );
            }
            compact_collect_expr_call_edges(program, expr, namespace, index_of, edges);
        }
        ArenaExprKind::MapComp {
            key,
            value,
            qualifiers,
        } => {
            for qualifier in program.arena.comp_qualifiers(qualifiers) {
                compact_collect_expr_call_edges(
                    program,
                    qualifier.expr(),
                    namespace,
                    index_of,
                    edges,
                );
            }
            compact_collect_expr_call_edges(program, key, namespace, index_of, edges);
            compact_collect_expr_call_edges(program, value, namespace, index_of, edges);
        }
        ArenaExprKind::ErrorContext { message, block }
        | ArenaExprKind::ContextScope {
            input: message,
            block,
            ..
        } => {
            compact_collect_expr_call_edges(program, message, namespace, index_of, edges);
            compact_collect_block_call_edges(program, block, namespace, index_of, edges);
        }
        ArenaExprKind::Capture(block)
        | ArenaExprKind::ValueBlock(block)
        | ArenaExprKind::Loop { block }
        | ArenaExprKind::Collect { block }
        | ArenaExprKind::Retry { block, .. }
        | ArenaExprKind::TempDirScope {
            path: None, block, ..
        } => {
            compact_collect_block_call_edges(program, block, namespace, index_of, edges);
        }
        ArenaExprKind::TempDirScope {
            path: Some(path),
            block,
            ..
        } => {
            compact_collect_expr_call_edges(program, path, namespace, index_of, edges);
            compact_collect_block_call_edges(program, block, namespace, index_of, edges);
        }
        ArenaExprKind::BuilderCall { call, .. } => {
            compact_collect_expr_call_edges(program, call, namespace, index_of, edges);
        }
        ArenaExprKind::ResourceScope {
            bindings, block, ..
        } => {
            for binding in program.arena.with_bindings(bindings) {
                compact_collect_expr_call_edges(
                    program,
                    binding.initializer,
                    namespace,
                    index_of,
                    edges,
                );
            }
            compact_collect_block_call_edges(program, block, namespace, index_of, edges);
        }
        _ => {}
    }
}

fn compact_tarjan_sccs(adjacency: Vec<Vec<usize>>) -> Vec<Vec<usize>> {
    struct Tarjan {
        adjacency: Vec<Vec<usize>>,
        index: Vec<Option<usize>>,
        lowlink: Vec<usize>,
        on_stack: Vec<bool>,
        stack: Vec<usize>,
        next_index: usize,
        sccs: Vec<Vec<usize>>,
    }
    impl Tarjan {
        fn strongconnect(&mut self, v: usize) {
            self.index[v] = Some(self.next_index);
            self.lowlink[v] = self.next_index;
            self.next_index += 1;
            self.stack.push(v);
            self.on_stack[v] = true;
            for i in 0..self.adjacency[v].len() {
                let w = self.adjacency[v][i];
                match self.index[w] {
                    None => {
                        self.strongconnect(w);
                        self.lowlink[v] = self.lowlink[v].min(self.lowlink[w]);
                    }
                    Some(w_index) if self.on_stack[w] => {
                        self.lowlink[v] = self.lowlink[v].min(w_index);
                    }
                    Some(_) => {}
                }
            }
            if self.lowlink[v] == self.index[v].expect("index set above") {
                let mut scc = Vec::new();
                loop {
                    let w = self.stack.pop().expect("stack non-empty");
                    self.on_stack[w] = false;
                    scc.push(w);
                    if w == v {
                        break;
                    }
                }
                self.sccs.push(scc);
            }
        }
    }
    let mut tarjan = Tarjan {
        index: vec![None; adjacency.len()],
        lowlink: vec![0; adjacency.len()],
        on_stack: vec![false; adjacency.len()],
        stack: Vec::new(),
        next_index: 0,
        sccs: Vec::new(),
        adjacency,
    };
    for v in 0..tarjan.adjacency.len() {
        if tarjan.index[v].is_none() {
            tarjan.strongconnect(v);
        }
    }
    tarjan.sccs
}

fn compact_function_key(namespace: Option<Name>, name: Name) -> LoweredFunctionKey {
    match namespace {
        Some(namespace) => LoweredFunctionKey::Qualified(QualifiedName::new(namespace, name)),
        None => LoweredFunctionKey::Name(name),
    }
}

fn compact_function_defs(program: &ArenaProgram) -> Vec<CompactFunctionDef> {
    let mut functions = Vec::new();
    for stmt in program.statement_ids() {
        collect_compact_function_def(program, stmt, None, &mut functions);
    }
    for module in &program.modules {
        for stmt in program.module_statements(module) {
            collect_compact_function_def(program, stmt, Some(module.name), &mut functions);
        }
    }
    functions
}

pub(super) fn compact_function_keys(program: &ArenaProgram) -> Vec<LoweredFunctionKey> {
    compact_function_defs(program)
        .into_iter()
        .map(|function| function.key)
        .collect()
}

fn collect_compact_function_def(
    program: &ArenaProgram,
    id: StmtId,
    namespace: Option<Name>,
    functions: &mut Vec<CompactFunctionDef>,
) {
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Export(inner) => {
            collect_compact_function_def(program, inner, namespace, functions);
        }
        ArenaStmtKind::PureDef(def) => {
            let name = program.arena.function_def(def).name;
            functions.push(CompactFunctionDef {
                key: compact_function_key(namespace, name),
                id: def,
                pure: true,
                namespace,
                definition_span: program.arena.stmt(id).span,
            });
        }
        ArenaStmtKind::ProcDef(def) | ArenaStmtKind::CliMain(def) => {
            let name = program.arena.function_def(def).name;
            functions.push(CompactFunctionDef {
                key: compact_function_key(namespace, name),
                id: def,
                pure: false,
                namespace,
                definition_span: program.arena.stmt(id).span,
            });
        }
        ArenaStmtKind::StreamDef(def) => {
            let name = program.arena.function_def(def).name;
            functions.push(CompactFunctionDef {
                key: compact_function_key(namespace, name),
                id: def,
                pure: true,
                namespace,
                definition_span: program.arena.stmt(id).span,
            });
        }
        _ => {}
    }
}

struct CompactLowerConstructProbe<'a, 'defs> {
    program: &'a ArenaProgram,
    declarations: &'a CompactDeclOutput,
    bodies: &'a CompactBodyFacts,
    source: &'a str,
    sources: Option<&'a SourceMap>,
    current_namespace: Option<Name>,
    functions: Option<&'a LowerableFunctions<'defs>>,
    top_level_known: FxHashMap<Name, LoweredTopLevelBinding>,
    output: CompactLowerConstructProbeOutput,
    last_blocker_detail: Option<(Span, String)>,
    /// Where standard-library implementation calls in this program resolve.
    stdlib_linkage: StdlibLowerLinkage,
    /// The program's function definitions and key index, built once and shared
    /// by every unit lowered in one pass.
    ///
    /// `compact_function_defs` walks every statement of every module, so
    /// rebuilding it per function makes preparation quadratic in program size.
    function_defs: Rc<RefCell<Option<Rc<CompactFunctionIndex>>>>,
    scratch: Rc<RefCell<BuildScratch>>,
    /// A copy of `program` and `bodies` that named-spread calls append their
    /// argument projections to, shared by every probe over the same two.
    ///
    /// The projections of a call are synthetic nodes, which need a program
    /// that can grow. Copying the program for each call is work proportional
    /// to the whole program per call; this copy is made for the first call of
    /// a pass and extended by the later ones.
    spread_programs: Rc<RefCell<Option<Box<SpreadPrograms>>>>,
}

/// A program and its body facts with the argument projections of
/// named-spread calls appended. Nothing is removed or renumbered, so every
/// node of the original has the same identity here.
struct SpreadPrograms {
    program: ArenaProgram,
    bodies: CompactBodyFacts,
}

#[cfg(test)]
thread_local! {
    /// How many times this thread copied a whole program for a named-spread
    /// call.
    static SPREAD_PROGRAM_COPIES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[derive(Clone, Copy, Debug)]
enum CompactTopLevelBlocker {
    Use,
    BindingTarget,
    BindingType,
    BindingExpression,
    AssignTarget,
    AssignExpression,
    Control,
    Command,
    Expression,
    Defer,
    Other,
}

impl CompactTopLevelBlocker {
    fn index(self) -> usize {
        match self {
            Self::Use => 0,
            Self::BindingTarget => 1,
            Self::BindingType => 2,
            Self::BindingExpression => 3,
            Self::AssignTarget => 4,
            Self::AssignExpression => 5,
            Self::Control => 6,
            Self::Command => 7,
            Self::Expression => 8,
            Self::Defer => 9,
            Self::Other => 10,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Use => "use",
            Self::BindingTarget => "binding_target",
            Self::BindingType => "binding_type",
            Self::BindingExpression => "binding_expression",
            Self::AssignTarget => "assign_target",
            Self::AssignExpression => "assign_expression",
            Self::Control => "control",
            Self::Command => "command",
            Self::Expression => "expression",
            Self::Defer => "defer",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum CompactFunctionBlocker {
    ReturnType,
    ParamDefault,
    ParamType,
    BlockParams,
    Body,
    NoReturn,
}

impl CompactFunctionBlocker {
    fn index(self) -> usize {
        match self {
            Self::ReturnType => 0,
            Self::ParamDefault => 1,
            Self::ParamType => 2,
            Self::BlockParams => 3,
            Self::Body => 4,
            Self::NoReturn => 5,
        }
    }
}

impl From<CompactFunctionBlocker> for LoweredFunctionBlocker {
    fn from(value: CompactFunctionBlocker) -> Self {
        match value {
            CompactFunctionBlocker::ReturnType => Self::ReturnType,
            CompactFunctionBlocker::ParamDefault => Self::ParamDefault,
            CompactFunctionBlocker::ParamType => Self::ParamType,
            CompactFunctionBlocker::BlockParams => Self::BlockParams,
            CompactFunctionBlocker::Body => Self::Body,
            CompactFunctionBlocker::NoReturn => Self::NoReturn,
        }
    }
}

impl From<LoweredFunctionBlocker> for CompactFunctionBlocker {
    fn from(value: LoweredFunctionBlocker) -> Self {
        match value {
            LoweredFunctionBlocker::ReturnType => Self::ReturnType,
            LoweredFunctionBlocker::ParamDefault => Self::ParamDefault,
            LoweredFunctionBlocker::ParamType => Self::ParamType,
            LoweredFunctionBlocker::BlockParams => Self::BlockParams,
            LoweredFunctionBlocker::Body => Self::Body,
            LoweredFunctionBlocker::NoReturn => Self::NoReturn,
        }
    }
}

fn compact_type_expr_tag_index(tag: ArenaTypeExprTag) -> usize {
    match tag {
        ArenaTypeExprTag::Applied => 8,
        ArenaTypeExprTag::Named => 0,
        ArenaTypeExprTag::Qualified => 1,
        ArenaTypeExprTag::List => 2,
        ArenaTypeExprTag::Map => 3,
        ArenaTypeExprTag::Stream => 4,
        ArenaTypeExprTag::Module => 5,
        ArenaTypeExprTag::Result => 6,
        ArenaTypeExprTag::Optional => 7,
        ArenaTypeExprTag::Union => 9,
        ArenaTypeExprTag::Callable => 10,
        ArenaTypeExprTag::NonEmpty => 11,
        ArenaTypeExprTag::Set => 12,
    }
}

// Indexes 20 and 27 are unassigned: they counted the guarded statement and
// the boolean guard, which now lower as the `if` they expand to.
fn compact_stmt_kind_index(kind: ArenaStmtKind) -> usize {
    match kind {
        ArenaStmtKind::Use(_) => 0,
        ArenaStmtKind::Export(_) => 1,
        ArenaStmtKind::TypeDef(_) => 2,
        ArenaStmtKind::ErrorDef(_) => 3,
        ArenaStmtKind::Let { .. } | ArenaStmtKind::Const { .. } => 4,
        ArenaStmtKind::Var { .. } => 5,
        ArenaStmtKind::Assign { .. } => 6,
        ArenaStmtKind::ProcDef(_) => 7,
        ArenaStmtKind::CliMain(_) => 29,
        ArenaStmtKind::PureDef(_) => 8,
        ArenaStmtKind::StreamDef(_) => 9,
        ArenaStmtKind::SignalHook(_) => 10,
        ArenaStmtKind::Return(_) => 11,
        ArenaStmtKind::YieldDelegate(_) => 12,
        ArenaStmtKind::Yield(_) => 12,
        ArenaStmtKind::Defer(..) => 13,
        ArenaStmtKind::If { .. } => 14,
        ArenaStmtKind::While { .. } => 15,
        ArenaStmtKind::For { .. } => 16,
        ArenaStmtKind::With { .. } => 17,
        ArenaStmtKind::Loop { .. } => 18,
        ArenaStmtKind::Guard { .. } => 19,
        ArenaStmtKind::Break { .. } => 21,
        ArenaStmtKind::Continue => 22,
        ArenaStmtKind::Match { .. } => 23,
        ArenaStmtKind::Command(_) => 24,
        ArenaStmtKind::TailBareIdent(_) => 25,
        ArenaStmtKind::Expr(_) | ArenaStmtKind::Exit(_) => 26,
        ArenaStmtKind::Sugar { .. } => {
            unreachable!("a sugar statement is classified by its expansion")
        }
        ArenaStmtKind::Assert { .. } => 28,
    }
}

fn compact_stmt_kind_label(kind: ArenaStmtKind) -> &'static str {
    match kind {
        ArenaStmtKind::Use(_) => "use",
        ArenaStmtKind::Export(_) => "export",
        ArenaStmtKind::TypeDef(_) => "type_def",
        ArenaStmtKind::ErrorDef(_) => "error_def",
        ArenaStmtKind::Let { .. } | ArenaStmtKind::Const { .. } => "let",
        ArenaStmtKind::Var { .. } => "var",
        ArenaStmtKind::Assign { .. } => "assign",
        ArenaStmtKind::ProcDef(_) => "proc_def",
        ArenaStmtKind::CliMain(_) => "cli_main",
        ArenaStmtKind::PureDef(_) => "pure_def",
        ArenaStmtKind::StreamDef(_) => "stream_def",
        ArenaStmtKind::SignalHook(_) => "signal_hook",
        ArenaStmtKind::Return(_) => "return",
        ArenaStmtKind::YieldDelegate(_) => "yield-delegate",
        ArenaStmtKind::Yield(_) => "yield",
        ArenaStmtKind::Defer(..) => "defer",
        ArenaStmtKind::If { .. } => "if",
        ArenaStmtKind::While { .. } => "while",
        ArenaStmtKind::For { .. } => "for",
        ArenaStmtKind::With { .. } => "with",
        ArenaStmtKind::Loop { .. } => "loop",
        ArenaStmtKind::Guard { .. } => "guard",
        ArenaStmtKind::Break { .. } => "break",
        ArenaStmtKind::Continue => "continue",
        ArenaStmtKind::Match { .. } => "match",
        ArenaStmtKind::Command(_) => "command",
        ArenaStmtKind::TailBareIdent(_) => "tail_bare_ident",
        ArenaStmtKind::Expr(_) | ArenaStmtKind::Exit(_) => "expr",
        ArenaStmtKind::Sugar { .. } => {
            unreachable!("a sugar statement is classified by its expansion")
        }
        ArenaStmtKind::Assert { .. } => "assert",
    }
}

fn compact_stmt_blocker_label(program: &ArenaProgram, stmt: StmtId) -> String {
    match program.arena.stmt(stmt).kind {
        ArenaStmtKind::Sugar { expansion, .. } => compact_stmt_blocker_label(program, expansion),
        ArenaStmtKind::Command(command) => {
            format!(
                "command:{}",
                compact_command_blocker_label(compact_command_blocker_index(program, command))
            )
        }
        kind => compact_stmt_kind_label(kind).to_string(),
    }
}

fn compact_expr_kind_index(kind: ArenaExprKind) -> usize {
    match kind {
        ArenaExprKind::Null => 0,
        ArenaExprKind::Bool(_) => 1,
        ArenaExprKind::Int(_) => 2,
        ArenaExprKind::Float(_) => 3,
        ArenaExprKind::Duration(_) => 4,
        ArenaExprKind::Str(_) => 5,
        ArenaExprKind::PathStr(_) => 6,
        ArenaExprKind::GlobStr(_) => 7,
        ArenaExprKind::FmtString(_) => 8,
        ArenaExprKind::PathFmtString(_) => 9,
        ArenaExprKind::Bytes(_) => 10,
        ArenaExprKind::Ident(_) => 11,
        ArenaExprKind::Item => 12,
        ArenaExprKind::LastStatus => 13,
        ArenaExprKind::List(_) => 14,
        ArenaExprKind::ListComp { .. } => 15,
        ArenaExprKind::MapComp { .. } => 16,
        ArenaExprKind::Record(_) => 17,
        ArenaExprKind::If { .. } => 18,
        ArenaExprKind::Match { .. }
        | ArenaExprKind::PatternTest { .. }
        | ArenaExprKind::PatternCondition { .. } => 19,
        ArenaExprKind::Unary { .. } => 20,
        ArenaExprKind::ComparisonChain(_) => 40,
        ArenaExprKind::Binary { .. } => 21,
        ArenaExprKind::Call { .. } => 22,
        ArenaExprKind::Field { .. } => 23,
        ArenaExprKind::NullSafeField { .. } => 24,
        ArenaExprKind::Index { .. } => 25,
        ArenaExprKind::Slice { .. } => 26,
        ArenaExprKind::EnvString(_) => 27,
        ArenaExprKind::EnvPathList => 28,
        ArenaExprKind::Pipeline { .. } => 29,
        ArenaExprKind::StructuredPipeline { .. } => 30,
        ArenaExprKind::Run(_) => 31,
        ArenaExprKind::Spawn(_) => 32,
        ArenaExprKind::Wait(_) => 33,
        ArenaExprKind::BuilderCall { .. } => 34,
        ArenaExprKind::Try(_) => 35,
        ArenaExprKind::Require { .. } => 36,
        ArenaExprKind::Loop { .. } => 37,
        ArenaExprKind::Capture(_) => 42,
        ArenaExprKind::Retry { .. } => 38,
        ArenaExprKind::ValueBlock(_) => 39,
        ArenaExprKind::ErrorContext { .. } => 44,
        ArenaExprKind::Regex(_) => 41,
        ArenaExprKind::ValuePipelineCall { .. } => 43,
        ArenaExprKind::ContextScope { .. } => 45,
        ArenaExprKind::TempDirScope { .. } => 46,
        ArenaExprKind::Convert { .. } => 47,
        ArenaExprKind::Set(_) => 48,
        ArenaExprKind::SetComp { .. } => 49,
        ArenaExprKind::Collect { .. } => 50,
        ArenaExprKind::ResourceScope { .. } => 51,
    }
}

/// The local that holds the list of one `collect` expression. No identifier
/// can spell it, and each expression has its own, so a nested `collect`
/// neither sees nor shadows the list around it.
fn collect_local(collect: ExprId) -> Name {
    Name::intern(&format!("%collect{}", collect.index()))
}

fn compact_expr_kind_label(kind: ArenaExprKind) -> &'static str {
    match kind {
        ArenaExprKind::Null => "null",
        ArenaExprKind::Bool(_) => "bool",
        ArenaExprKind::Int(_) => "int",
        ArenaExprKind::Float(_) => "float",
        ArenaExprKind::Duration(_) => "duration",
        ArenaExprKind::Str(_) => "str",
        ArenaExprKind::PathStr(_) => "path_str",
        ArenaExprKind::GlobStr(_) => "glob_str",
        ArenaExprKind::FmtString(_) => "fmt_string",
        ArenaExprKind::PathFmtString(_) => "path_fmt_string",
        ArenaExprKind::Bytes(_) => "bytes",
        ArenaExprKind::Ident(_) => "ident",
        ArenaExprKind::Item => "item",
        ArenaExprKind::LastStatus => "last_status",
        ArenaExprKind::List(_) => "list",
        ArenaExprKind::ListComp { .. } => "list_comp",
        ArenaExprKind::MapComp { .. } => "map_comp",
        ArenaExprKind::Set(_) => "set",
        ArenaExprKind::SetComp { .. } => "set_comp",
        ArenaExprKind::Record(_) => "record",
        ArenaExprKind::If { .. } => "if",
        ArenaExprKind::Match { .. } => "match",
        ArenaExprKind::PatternTest { .. } => "pattern_test",
        ArenaExprKind::PatternCondition { .. } => "pattern_condition",
        ArenaExprKind::Unary { .. } => "unary",
        ArenaExprKind::ComparisonChain(_) => "comparison-chain",
        ArenaExprKind::Binary { .. } => "binary",
        ArenaExprKind::Call { .. } => "call",
        ArenaExprKind::Field { .. } => "field",
        ArenaExprKind::NullSafeField { .. } => "null_safe_field",
        ArenaExprKind::Index { .. } => "index",
        ArenaExprKind::Slice { .. } => "slice",
        ArenaExprKind::EnvString(_) => "env_string",
        ArenaExprKind::EnvPathList => "env_path_list",
        ArenaExprKind::Pipeline { .. } => "pipeline",
        ArenaExprKind::StructuredPipeline { .. } => "structured_pipeline",
        ArenaExprKind::Run(_) => "run",
        ArenaExprKind::Spawn(_) => "spawn",
        ArenaExprKind::Wait(_) => "wait",
        ArenaExprKind::BuilderCall { .. } => "builder_call",
        ArenaExprKind::Try(_) => "try",
        ArenaExprKind::Require { .. } => "require",
        ArenaExprKind::Loop { .. } => "loop",
        ArenaExprKind::Capture(_) => "try",
        ArenaExprKind::Retry { .. } => "retry",
        ArenaExprKind::ValueBlock(_) => "value_block",
        ArenaExprKind::ErrorContext { .. } => "error_context",
        ArenaExprKind::Regex(_) => "regex_literal",
        ArenaExprKind::ValuePipelineCall { .. } => "value_pipeline_call",
        ArenaExprKind::ContextScope { .. } => "context_scope",
        ArenaExprKind::TempDirScope { .. } => "tempdir_scope",
        ArenaExprKind::Convert { .. } => "convert",
        ArenaExprKind::Collect { .. } => "collect",
        ArenaExprKind::ResourceScope { .. } => "resource_scope",
    }
}

fn compact_checked_type_is_concrete(ty: &Type) -> bool {
    !matches!(ty, Type::Any | Type::Unknown | Type::Invalid)
        && !ty.contains_any()
        && !ty.contains_inference()
}

/// Disagreements between a representation lowering derives itself and the
/// checker's published type for the same binding, recorded by debug builds.
#[cfg(debug_assertions)]
pub(crate) static LOWERING_DRIFT: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Lowering agrees with the checker up to representation erasure: lowering may
/// keep `Any`, a nullable value erases to its present kind, `UInt` shares the
/// `Int` representation, and an unresolved checker fact makes no claim.
#[cfg(debug_assertions)]
fn note_lowering_drift(
    span: Span,
    kind: Option<LoweredType>,
    lowered: &Type,
    checked: Option<&Type>,
) {
    let Some(checked) = checked.filter(|ty| checked_fact_is_resolved(ty)) else {
        return;
    };
    let present = checked.optional_inner().unwrap_or(checked);
    let kind_agrees = match (kind, lowered_checked_type(present)) {
        (None | Some(LoweredType::Any), _) | (_, None) => true,
        (Some(kind), Some(expected)) => kind == expected,
    };
    if !kind_agrees || lowered != checked {
        LOWERING_DRIFT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(format!(
                "{span:?}: lowering {kind:?} {lowered}, checker {checked}"
            ));
    }
}

fn concrete_checked_fact(ty: &Type) -> bool {
    compact_checked_type_is_concrete(ty)
        && !matches!(
            ty.result_ok(),
            Some(Type::Any | Type::Unknown | Type::Invalid)
        )
}

fn compact_call_blocker_index(program: &ArenaProgram, callee: ExprId) -> usize {
    match program.arena.expr(callee).kind {
        ArenaExprKind::Ident(_) => 0,
        ArenaExprKind::Field { base, .. } => {
            if matches!(program.arena.expr(base).kind, ArenaExprKind::Ident(_)) {
                1
            } else {
                2
            }
        }
        ArenaExprKind::NullSafeField { base, .. } => {
            if matches!(program.arena.expr(base).kind, ArenaExprKind::Ident(_)) {
                3
            } else {
                4
            }
        }
        _ => 5,
    }
}

fn compact_call_blocker_label(program: &ArenaProgram, callee: ExprId) -> Option<String> {
    match program.arena.expr(callee).kind {
        ArenaExprKind::Ident(name) => Some(name.as_str().to_string()),
        ArenaExprKind::Field { base, name } => match program.arena.expr(base).kind {
            ArenaExprKind::Ident(module) => Some(format!("{}.{}", module.as_str(), name.as_str())),
            _ => Some(format!("<field>.{}", name.as_str())),
        },
        ArenaExprKind::NullSafeField { base, name } => match program.arena.expr(base).kind {
            ArenaExprKind::Ident(module) => Some(format!("{}?.{}", module.as_str(), name.as_str())),
            _ => Some(format!("<null-safe-field>.{}", name.as_str())),
        },
        _ => None,
    }
}

fn record_compact_call_blocker_label(
    counts: &mut BTreeMap<String, u32>,
    program: &ArenaProgram,
    callee: ExprId,
) {
    if let Some(label) = compact_call_blocker_label(program, callee) {
        *counts.entry(label).or_insert(0) += 1;
    }
}

fn record_compact_call_blocker_span(
    samples: &mut BTreeMap<String, Vec<Span>>,
    program: &ArenaProgram,
    callee: ExprId,
) {
    let Some(label) = compact_call_blocker_label(program, callee) else {
        return;
    };
    let samples = samples.entry(label).or_default();
    if samples.len() < 8 {
        samples.push(program.arena.expr(callee).span);
    }
}

fn record_compact_stmt_blocker_span(
    samples: &mut BTreeMap<String, Vec<Span>>,
    program: &ArenaProgram,
    stmt: StmtId,
) {
    let label = compact_stmt_blocker_label(program, stmt);
    let samples = samples.entry(label).or_default();
    if samples.len() < 8 {
        samples.push(program.arena.stmt(stmt).span);
    }
}

fn compact_error_family_name(program: &ArenaProgram, id: ExprId) -> Option<Name> {
    match program.arena.expr(id).kind {
        ArenaExprKind::Ident(name) => Some(name),
        ArenaExprKind::Field { base, name } => {
            let base = compact_error_family_name(program, base)?;
            Some(Name::intern(format!("{base}.{name}")))
        }
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum CompactErrorFamilyKey {
    Local(Name),
    Qualified(QualifiedName),
}

fn compact_error_family_key(program: &ArenaProgram, id: ExprId) -> Option<CompactErrorFamilyKey> {
    match program.arena.expr(id).kind {
        ArenaExprKind::Ident(name) => Some(CompactErrorFamilyKey::Local(name)),
        ArenaExprKind::Field { base, name } => match program.arena.expr(base).kind {
            ArenaExprKind::Ident(namespace) => Some(CompactErrorFamilyKey::Qualified(
                QualifiedName::new(namespace, name),
            )),
            _ => compact_error_family_name(program, id).map(CompactErrorFamilyKey::Local),
        },
        _ => None,
    }
}

fn compact_error_family_display(key: CompactErrorFamilyKey) -> String {
    match key {
        CompactErrorFamilyKey::Local(name) => name.to_string(),
        // An error carries its family's declared name wherever it is built.
        CompactErrorFamilyKey::Qualified(name) => name.member.to_string(),
    }
}

fn compact_error_family_info(
    declarations: &crate::sema::check::CompactDeclOutput,
    key: CompactErrorFamilyKey,
) -> Option<&crate::sema::check::ErrorFamilyInfo> {
    match key {
        CompactErrorFamilyKey::Local(name) => declarations.error_families_by_name.get(&name),
        CompactErrorFamilyKey::Qualified(name) => declarations.qualified_error_families.get(&name),
    }
}

fn compact_expr_call_blocker_callee(program: &ArenaProgram, expr: ExprId) -> Option<ExprId> {
    match program.arena.expr(expr).kind {
        ArenaExprKind::Call { callee, .. } => Some(callee),
        ArenaExprKind::Try(inner) => match program.arena.expr(inner).kind {
            ArenaExprKind::Call { callee, .. } => Some(callee),
            _ => None,
        },
        _ => None,
    }
}

fn print_flush_arg(
    program: &ArenaProgram,
    source: &str,
    sources: Option<&SourceMap>,
    arg: &crate::syntax::arena::ArenaCommandArg,
) -> bool {
    let arg_span = program.arena.span(arg.span);
    if sources
        .and_then(|sources| sources.span_text(arg_span))
        .is_some_and(|value| value == "--flush")
    {
        return true;
    }

    let ArenaCommandArgKind::Word(parts) = &arg.kind else {
        return false;
    };
    let parts = program.arena.word_parts(*parts).collect::<Vec<_>>();
    let [ArenaWordPart::Bare(text)] = parts.as_slice() else {
        return false;
    };
    let value = program.arena.text_value(text, source);

    value == Some("--flush")
}

fn compact_command_blocker_index(
    program: &ArenaProgram,
    command: crate::syntax::arena::CommandStmtId,
) -> usize {
    match program.arena.command_stmt(command).command {
        ArenaCommand::Proc { .. } => 0,
        ArenaCommand::Core {
            name: CoreCommand::Print,
            ..
        } => 1,
        ArenaCommand::Core {
            name: CoreCommand::Eprint,
            ..
        } => 2,
        ArenaCommand::Core {
            name: CoreCommand::Cd,
            ..
        } => 3,
        ArenaCommand::Core {
            name: CoreCommand::Env,
            ..
        } => 4,
        ArenaCommand::Run(_) => 5,
    }
}

fn compact_command_blocker_label(index: usize) -> &'static str {
    match index {
        0 => "proc",
        1 => "core_print",
        2 => "core_eprint",
        3 => "core_cd",
        4 => "core_env",
        5 => "run",
        _ => "unknown",
    }
}

fn compact_use_import_namespace(
    program: &ArenaProgram,
    use_id: crate::syntax::arena::UseStmtId,
) -> Option<Name> {
    let use_stmt = program.arena.use_stmt(use_id);
    use_stmt
        .alias
        .or_else(|| program.arena.names(use_stmt.path).last())
}

fn compact_module_exports_for_use(
    program: &ArenaProgram,
    key: &str,
    _namespace: Name,
    _functions: Option<&LowerableFunctions<'_>>,
) -> Option<Vec<LoweredModuleExport>> {
    let module = program
        .modules
        .iter()
        .find(|module| module.key.as_str() == key)?;
    let function_namespace = module.name;
    let mut exports = Vec::new();
    for stmt in program.module_statements(module) {
        let ArenaStmtKind::Export(inner) = program.arena.stmt(stmt).kind else {
            continue;
        };
        match program.arena.stmt(inner).kind {
            ArenaStmtKind::Let { target, .. }
            | ArenaStmtKind::Const { target, .. }
            | ArenaStmtKind::Var { target, .. } => {
                let ArenaBindingTargetKind::Name(name) = program.arena.binding_target(target).kind
                else {
                    return None;
                };
                exports.push(LoweredModuleExport {
                    name,
                    kind: LoweredModuleExportKind::Value,
                    function_namespace: None,
                });
            }
            ArenaStmtKind::ProcDef(def) => {
                let name = program.arena.function_def(def).name;
                exports.push(LoweredModuleExport {
                    name,
                    kind: LoweredModuleExportKind::Proc,
                    function_namespace: Some(function_namespace),
                });
            }
            ArenaStmtKind::PureDef(def) => {
                let name = program.arena.function_def(def).name;
                exports.push(LoweredModuleExport {
                    name,
                    kind: LoweredModuleExportKind::Pure,
                    function_namespace: Some(function_namespace),
                });
            }
            ArenaStmtKind::StreamDef(_)
            | ArenaStmtKind::TypeDef(_)
            | ArenaStmtKind::ErrorDef(_) => {}
            _ => return None,
        }
    }
    Some(exports)
}

pub(super) fn lower_literal_constant(
    value: &crate::sema::constants::LiteralConstant,
    enums: Option<&crate::sema::wire_enums::PreparedWireEnums>,
) -> Option<LoweredValue> {
    use crate::sema::constants::LiteralConstant as C;
    Some(match value {
        C::Regex(literal) => LoweredValue::Regex(Box::new(RegexValue {
            pattern: literal.pattern.to_string(),
            regex: literal.prepared.get()?.as_ref().ok()?.clone(),
        })),
        C::Tag {
            family,
            variant,
            fields,
        } => LoweredValue::Tag(Box::new(super::LoweredTagValue {
            type_name: *family,
            wire: enums.and_then(|enums| enums.mappings.get(family)).cloned(),
            name: Arc::from(variant.as_str().as_str()),
            fields: fields
                .iter()
                .map(|value| lower_literal_constant(value, enums))
                .collect::<Option<Vec<_>>>()?,
        })),
        C::Null => LoweredValue::Null,
        C::Bool(value) => LoweredValue::Bool(*value),
        C::Int(value) => LoweredValue::Int(*value),
        C::Float(value) => LoweredValue::Float(crate::runtime::value::FloatValue::new(
            f64::from_bits(*value),
        )),
        C::Duration(millis) => LoweredValue::Duration(DurationValue { millis: *millis }),
        C::Str(value) => LoweredValue::Str(value.clone()),
        C::Bytes(value) => LoweredValue::Bytes(value.clone()),
        C::Path(value) => LoweredValue::Path(PathValue::from_text(value).ok()?),
        C::EmptyMap => LoweredValue::Map(Arc::new(BTreeMap::new())),
        C::Map(values) => LoweredValue::Map(Arc::new(
            values
                .iter()
                .map(|(key, value)| Some((key.clone(), lower_literal_constant(value, enums)?)))
                .collect::<Option<BTreeMap<_, _>>>()?,
        )),
        C::List(values) => LoweredValue::SharedList(Arc::new(
            values
                .iter()
                .map(|value| lower_literal_constant(value, enums))
                .collect::<Option<Vec<_>>>()?,
        )),
        C::Set(values) => LoweredValue::Set(values.clone()),
        C::Record(values) => LoweredValue::Record(Arc::new(
            values
                .iter()
                .map(|(name, value)| {
                    Some((
                        Arc::<str>::from(name.as_str().as_str()),
                        lower_literal_constant(value, enums)?,
                    ))
                })
                .collect::<Option<BTreeMap<_, _>>>()?,
        )),
    })
}

fn compact_body_tail_stmt_kind(program: &ArenaProgram, block: BlockId) -> usize {
    program
        .arena
        .stmt_ids(program.arena.block(block).statements)
        .last()
        .map(|stmt| {
            compact_stmt_kind_index(program.arena.stmt(program.arena.core_stmt_id(stmt)).kind)
        })
        .unwrap_or(COMPACT_STMT_KIND_COUNT - 1)
}

fn compact_body_tail_call_blocker_callee(program: &ArenaProgram, block: BlockId) -> Option<ExprId> {
    let stmt = program
        .arena
        .stmt_ids(program.arena.block(block).statements)
        .last()?;
    match program.arena.stmt(program.arena.core_stmt_id(stmt)).kind {
        ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(expr))) | ArenaStmtKind::Expr(expr) => {
            compact_expr_call_blocker_callee(program, expr)
        }
        ArenaStmtKind::Let {
            initializer: ArenaExprOrRun::Expr(expr),
            ..
        }
        | ArenaStmtKind::Const {
            initializer: ArenaExprOrRun::Expr(expr),
            ..
        }
        | ArenaStmtKind::Var {
            initializer: ArenaExprOrRun::Expr(expr),
            ..
        } => compact_expr_call_blocker_callee(program, expr),
        _ => None,
    }
}

fn compact_body_tail_command_blocker(
    program: &ArenaProgram,
    block: BlockId,
) -> Option<crate::syntax::arena::CommandStmtId> {
    let stmt = program
        .arena
        .stmt_ids(program.arena.block(block).statements)
        .last()?;
    match program.arena.stmt(program.arena.core_stmt_id(stmt)).kind {
        ArenaStmtKind::Command(command) => Some(command),
        _ => None,
    }
}

const _: [(); COMPACT_TYPE_EXPR_TAG_COUNT] = [(); 13];
const _: [(); COMPACT_STMT_KIND_COUNT] = [(); 30];
const _: [(); COMPACT_EXPR_KIND_COUNT] = [(); 52];
const _: [(); COMPACT_CALL_BLOCKER_KIND_COUNT] = [(); 6];
const _: [(); COMPACT_COMMAND_BLOCKER_KIND_COUNT] = [(); 6];

impl<'p> CompactLowerConstructProbe<'p, '_> {
    fn text_value_in_span<'a>(
        &'a self,
        text: &'a crate::syntax::arena::ArenaText,
        _context: Span,
    ) -> Option<&'a str> {
        match text {
            crate::syntax::arena::ArenaText::Source(text_source) => {
                let start = text_source.bytes.start as usize;
                let span = Span::new(
                    text_source.source_id,
                    start,
                    start + text_source.bytes.len as usize,
                );
                self.program
                    .arena
                    .text_value(text, self.source)
                    .or_else(|| self.sources.and_then(|sources| sources.span_text(span)))
            }
            crate::syntax::arena::ArenaText::Cooked(value) => Some(value.as_ref()),
        }
    }

    fn bare_text_value_in_span<'a>(
        &'a self,
        text: &'a crate::syntax::arena::ArenaText,
        context: Span,
    ) -> Option<&'a str> {
        let value = self.text_value_in_span(text, context)?;
        if value.is_empty() {
            self.sources
                .and_then(|sources| sources.span_text(context))
                .filter(|text| !text.is_empty())
                .or(Some(value))
        } else {
            Some(value)
        }
    }

    fn text_value<'a>(&'a self, text: &'a crate::syntax::arena::ArenaText) -> Option<&'a str> {
        match text {
            crate::syntax::arena::ArenaText::Source(text_source) => {
                let start = text_source.bytes.start as usize;
                let span = Span::new(
                    text_source.source_id,
                    start,
                    start + text_source.bytes.len as usize,
                );
                self.program
                    .arena
                    .text_value(text, self.source)
                    .or_else(|| self.sources.and_then(|sources| sources.span_text(span)))
            }
            crate::syntax::arena::ArenaText::Cooked(value) => Some(value.as_ref()),
        }
    }

    fn probe_program(&mut self) {
        let root = self.program.statement_ids().collect::<Vec<_>>();
        self.top_level_known = self.collect_top_level_known(&root);
        self.lower_top_level_program(&root);
        for stmt in root {
            self.probe_function_stmt(stmt);
        }
        for module in &self.program.modules {
            let statements = self.program.module_statements(module).collect::<Vec<_>>();
            self.current_namespace = Some(module.name);
            self.top_level_known = self.collect_top_level_known(&statements);
            self.lower_top_level_program(&statements);
            for stmt in statements {
                self.probe_function_stmt(stmt);
            }
            self.current_namespace = None;
        }
    }

    fn collect_top_level_known(
        &self,
        statements: &[StmtId],
    ) -> FxHashMap<Name, LoweredTopLevelBinding> {
        let mut known = top_level_known_with_runtime_bindings();
        for stmt in statements {
            self.record_top_level_binding(*stmt, &mut known);
        }
        known
    }

    fn append_immutable_top_level_captures(&self, slots: &mut SlotScope) -> LoweredTopLevelSlots {
        let mut bindings = self
            .top_level_known
            .iter()
            .filter(|(name, binding)| {
                binding.slot
                    && slots.resolve(**name).is_none()
                    && !self
                        .declarations
                        .prepared_constants
                        .global_bindings
                        .contains_key(&(self.current_namespace, **name))
            })
            .map(|(name, binding)| (*name, binding.kind, binding.mutable))
            .collect::<Vec<_>>();
        bindings.sort_unstable_by_key(|(name, _, _)| *name);

        let mut captures: LoweredTopLevelSlots = Default::default();
        for (name, kind, mutable) in bindings {
            let slot = slots.declare_capture(name);
            captures.push(LoweredTopLevelSlot {
                name,
                slot,
                kind,
                mutable,
            });
        }
        captures
    }

    fn lower_top_level_program(&mut self, statements: &[StmtId]) {
        self.lower_program_statements(statements);
    }

    fn lower_program_statements(&mut self, statements: &[StmtId]) -> ProgramBuild {
        let mut known = top_level_known_with_runtime_bindings();
        let mut lowered = ProgramBuild {
            statements: Vec::with_capacity(statements.len()),
            scratch: self.scratch.clone(),
        };
        for stmt in statements {
            self.output.top_level_statements += 1;
            let blockers_before = self.output.blocker_events;
            let mut item = self.lower_top_level_stmt(*stmt, &known);
            if self.output.blocker_events != blockers_before {
                item = None;
            }
            if item.is_some() {
                self.output.constructed_top_level_statements += 1;
            } else if !construct_top_level_stmt_is_skippable(self.program, *stmt) {
                let blocker = self.top_level_blocker_kind(*stmt);
                self.output.top_level_blockers[blocker.index()] += 1;
                self.record_top_level_blocker_detail(*stmt, blocker);
            }
            lowered.statements.push(item);
            self.record_top_level_binding(*stmt, &mut known);
        }
        lowered
    }

    fn probe_function_stmt(&mut self, id: StmtId) {
        match self.program.arena.stmt(id).kind {
            ArenaStmtKind::Export(inner) => self.probe_function_stmt(inner),
            ArenaStmtKind::PureDef(def) => {
                self.output.functions += 1;
                let previous_known = std::mem::replace(
                    &mut self.top_level_known,
                    compact_function_top_level_known(
                        self.program,
                        self.declarations,
                        self.source,
                        self.sources,
                        self.current_namespace,
                        def,
                        self.functions,
                    ),
                );
                let function = CompactFunctionDef {
                    key: compact_function_key(
                        self.current_namespace,
                        self.program.arena.function_def(def).name,
                    ),
                    id: def,
                    pure: true,
                    namespace: self.current_namespace,
                    definition_span: self.program.arena.stmt(id).span,
                };
                let definitions = self.function_index();
                let dependencies =
                    compact_function_dependency_keys(&definitions, self.program, &function);
                let (scc_member_count, scc_group) = definitions.scc_metadata(&function);
                let unit =
                    self.lower_function_unit(function, dependencies, scc_member_count, scc_group);
                if unit.is_lowered() {
                    self.output.constructed_functions += 1;
                } else if let Some(blocker) = unit.blocker() {
                    let blocker = CompactFunctionBlocker::from(blocker);
                    self.output.function_blockers[blocker.index()] += 1;
                    self.record_function_blocker_detail(def, blocker);
                }
                self.top_level_known = previous_known;
            }
            ArenaStmtKind::ProcDef(def) | ArenaStmtKind::CliMain(def) => {
                self.output.functions += 1;
                let previous_known = std::mem::replace(
                    &mut self.top_level_known,
                    compact_function_top_level_known(
                        self.program,
                        self.declarations,
                        self.source,
                        self.sources,
                        self.current_namespace,
                        def,
                        self.functions,
                    ),
                );
                let function = CompactFunctionDef {
                    key: compact_function_key(
                        self.current_namespace,
                        self.program.arena.function_def(def).name,
                    ),
                    id: def,
                    pure: false,
                    namespace: self.current_namespace,
                    definition_span: self.program.arena.stmt(id).span,
                };
                let definitions = self.function_index();
                let dependencies =
                    compact_function_dependency_keys(&definitions, self.program, &function);
                let (scc_member_count, scc_group) = definitions.scc_metadata(&function);
                let unit =
                    self.lower_function_unit(function, dependencies, scc_member_count, scc_group);
                if unit.is_lowered() {
                    self.output.constructed_functions += 1;
                    if !self.program.arena.function_def(def).test_declaration
                        && self.program.arena.function_def(def).name == Name::intern("main")
                    {
                        self.output.constructed_auto_main_functions += 1;
                    }
                } else if let Some(blocker) = unit.blocker() {
                    let blocker = CompactFunctionBlocker::from(blocker);
                    self.output.function_blockers[blocker.index()] += 1;
                    self.record_function_blocker_detail(def, blocker);
                }
                self.top_level_known = previous_known;
            }
            ArenaStmtKind::StreamDef(_def) => {
                self.output.functions += 1;
                self.output.constructed_functions += 1;
            }
            _ => {}
        }
    }

    fn lower_function_unit(
        &mut self,
        function: CompactFunctionDef,
        dependency_edges: Vec<LoweredFunctionKey>,
        scc_member_count: usize,
        scc_group: Option<usize>,
    ) -> LoweredFunctionUnit {
        let def = self.program.arena.function_def(function.id);
        let source_span = self
            .program
            .arena
            .span(self.program.arena.block(def.body).span);
        let definition_span = function.definition_span;
        match self.lower_function_with_blocker(function.id, function.pure) {
            Ok(body) => {
                let param_count = body.params.len();
                let capture_count = body.captures.len();
                let slot_count = body.slot_count;
                LoweredFunctionUnit {
                    key: function.key,
                    kind: if function.pure {
                        LoweredFunctionKind::Pure
                    } else {
                        LoweredFunctionKind::Proc
                    },
                    source_span,
                    definition_span,
                    owner: function.namespace,
                    param_count,
                    capture_count,
                    slot_count,
                    dependency_edges,
                    body: Some(body),
                    blocker: None,
                    blocker_detail: None,
                    scc_member_count,
                    scc_group,
                }
            }
            Err(blocker) => {
                let blocker_detail = self.last_blocker_detail.clone();
                LoweredFunctionUnit {
                    key: function.key,
                    kind: if function.pure {
                        LoweredFunctionKind::Pure
                    } else {
                        LoweredFunctionKind::Proc
                    },
                    source_span,
                    definition_span,
                    owner: function.namespace,
                    param_count: self.program.arena.params(def.params).len(),
                    capture_count: 0,
                    slot_count: 0,
                    dependency_edges,
                    body: None,
                    blocker: Some(blocker.into()),
                    blocker_detail,
                    scc_member_count,
                    scc_group,
                }
            }
        }
    }

    fn record_function_blocker_detail(
        &mut self,
        id: crate::syntax::arena::FunctionDefId,
        blocker: CompactFunctionBlocker,
    ) {
        let def = self.program.arena.function_def(id);
        match blocker {
            CompactFunctionBlocker::ReturnType => {
                let tag = self.program.arena.type_expr_tags[def.return_ty.index()];
                self.output.function_return_type_tags[compact_type_expr_tag_index(tag)] += 1;
            }
            CompactFunctionBlocker::ParamType => {
                for param in self.program.arena.params(def.params) {
                    if lowered_arena_type(&self.program.arena, param.ty, self.declarations)
                        .is_none()
                    {
                        let tag = self.program.arena.type_expr_tags[param.ty.index()];
                        self.output.function_param_type_tags[compact_type_expr_tag_index(tag)] += 1;
                    }
                }
            }
            CompactFunctionBlocker::Body => {
                let index = compact_body_tail_stmt_kind(self.program, def.body);
                self.output.function_body_tail_stmt_kinds[index] += 1;
                if let Some(command) = compact_body_tail_command_blocker(self.program, def.body) {
                    let index = compact_command_blocker_index(self.program, command);
                    self.output.function_body_tail_command_kinds[index] += 1;
                }
                if let Some(callee) = compact_body_tail_call_blocker_callee(self.program, def.body)
                {
                    record_compact_call_blocker_label(
                        &mut self.output.function_body_tail_call_callees,
                        self.program,
                        callee,
                    );
                }
            }
            CompactFunctionBlocker::ParamDefault
            | CompactFunctionBlocker::BlockParams
            | CompactFunctionBlocker::NoReturn => {}
        }
    }

    fn lower_function_with_blocker(
        &mut self,
        id: crate::syntax::arena::FunctionDefId,
        _pure: bool,
    ) -> Result<FunctionBuild, CompactFunctionBlocker> {
        let def = self.program.arena.function_def(id);
        let body_span = self
            .program
            .arena
            .span(self.program.arena.block(def.body).span);
        let inferred_kind = self
            .declarations
            .function_return_types
            .get(&body_span)
            .and_then(|ty| {
                let storage_kind = |ty: &Type| match ty {
                    Type::Null | Type::Optional(_) => Some(LoweredType::Any),
                    _ => lowered_checked_type(ty),
                };
                if let Type::Result(ok, _) = ty {
                    storage_kind(ok).map(LoweredReturnKind::Result)
                } else if matches!(ty, Type::Optional(inner) if matches!(**inner, Type::Result(_, _)))
                {
                    Some(LoweredReturnKind::OptionalResult)
                } else {
                    storage_kind(ty).map(LoweredReturnKind::Plain)
                }
            });
        let return_kind = match inferred_kind.or_else(|| self.lowered_return_kind(def.return_ty)) {
            Some(kind) => kind,
            None => {
                self.last_blocker_detail = Some((
                    self.program.arena.type_expr_span(def.return_ty),
                    "unsupported return type annotation".to_string(),
                ));
                return Err(CompactFunctionBlocker::ReturnType);
            }
        };
        let mut param_kinds: LoweredParamKinds = Default::default();
        let mut param_checks: LoweredParamChecks = Default::default();
        let mut param_rest: LoweredParamRest = Default::default();
        let mut param_defaults: LoweredParamDefaults = Default::default();
        let mut params: LoweredParamNames = Default::default();
        let mut checked_params = Vec::new();
        let mut expression_defaults = Vec::new();
        for (index, param) in self.program.arena.params(def.params).iter().enumerate() {
            let expected = self
                .declarations
                .parameter_types
                .get(&self.program.arena.span(param.span))
                .cloned()
                .unwrap_or_else(|| {
                    compact_runtime_type_in_namespace(
                        &self.program.arena,
                        param.ty,
                        self.declarations,
                        self.current_namespace,
                    )
                });
            let kind = match &expected {
                Type::Optional(_) | Type::Null => Some(LoweredType::Any),
                _ => lowered_checked_type(&expected),
            }
            .ok_or(CompactFunctionBlocker::ParamType)?;
            // A parameter of a callable type records the checked type, so
            // the verifier can tie a typed call through the parameter to it.
            let check = if matches!(expected, Type::Callable(_)) {
                Some(LoweredTypeCheck {
                    schema: None,
                    ty: expected.clone(),
                    name: Arc::from(expected.to_string()),
                })
            } else if param.ty_defaulted {
                (lowered_type_needs_static_check(kind) || expected.validated().is_some()).then(
                    || LoweredTypeCheck {
                        schema: None,
                        ty: expected.clone(),
                        name: Arc::from(expected.to_string()),
                    },
                )
            } else {
                compact_type_check(
                    kind,
                    &self.program.arena,
                    param.ty,
                    self.declarations,
                    self.current_namespace,
                )
            };
            let default = if let Some(expr) = param.default {
                let prepared = self
                    .declarations
                    .prepared_constants
                    .analyze_expression(&self.program.arena, expr)
                    .map(|constant| constant.in_type(&expected))
                    .and_then(|constant| {
                        lower_literal_constant(&constant, Some(&self.declarations.wire_enums))
                    })
                    .filter(|value| lowered_value_matches(kind, value))
                    .or_else(|| {
                        lower_const_param_default(&self.program.arena, expr, kind, Some(&expected))
                    });
                if let Some(value) = prepared {
                    Some(value)
                } else {
                    expression_defaults.push((index, expr, kind, check.clone()));
                    Some(LoweredValue::OmittedArgument)
                }
            } else {
                None
            };
            param_kinds.push(kind);
            param_checks.push(check);
            param_rest.push(param.rest);
            param_defaults.push(default);
            params.push(param.name);
            checked_params.push(expected);
        }
        if !def.test_declaration && !self.program.arena.block(def.body).params.is_empty() {
            self.last_blocker_detail = Some((
                self.program
                    .arena
                    .span(self.program.arena.block(def.body).span),
                "function body block parameters are not lowerable".to_string(),
            ));
            return Err(CompactFunctionBlocker::BlockParams);
        }
        // NOTE: nested loops are supported by the lowered runtime (break/continue
        // use StmtFlow which correctly scopes to the innermost loop).
        // The check is removed — it was an early indexed-lowering safety measure that is no longer needed.
        // Parameter slots exist at entry, but their names are hidden while
        // lowering defaults so every default resolves in the outer environment.
        let mut slots = SlotScope::from_names([]);
        for _ in &params {
            slots.reserve("parameter");
        }
        let captures = self.append_immutable_top_level_captures(&mut slots);
        let blockers_before = self.output.blocker_events;
        let mut default_prefix = Vec::new();
        for (slot, expr, kind, check) in expression_defaults {
            let value = self
                .lower_expr(expr, &mut slots, Some(def.name), None)
                .ok_or(CompactFunctionBlocker::ParamDefault)?;
            default_prefix.push(push_build_row!(
                self,
                stmt,
                BuildStmtRow::DefaultParameter {
                    slot,
                    value,
                    kind,
                    check,
                    span: self.program.arena.expr(expr).span
                }
            ));
        }
        for (slot, (name, ty)) in params.iter().copied().zip(checked_params).enumerate() {
            slots.indices.insert(name, slot);
            slots.types.insert(name, ty);
            slots.captures.remove(&name);
        }
        let mut body = self
            .lower_tail_block(def.body, &mut slots, Some(def.name), None)
            .ok_or_else(|| {
                if self.last_blocker_detail.is_none() {
                    self.last_blocker_detail = Some((
                        self.program
                            .arena
                            .span(self.program.arena.block(def.body).span),
                        "unsupported statement in body".to_string(),
                    ));
                }
                CompactFunctionBlocker::Body
            })?;
        default_prefix.append(&mut body);
        body = default_prefix;
        // The construct probe is permissive: it substitutes `Unit` for any
        // sub-expression/statement it cannot lower so it can finish traversing
        // and tally blockers. That placeholder must never be committed as real
        // code. If lowering this body produced any blocker (e.g. a forward
        // reference to a not-yet-lowered function), refuse to commit so the
        // fixpoint retries once dependencies are available, or the function
        // falls back honestly.
        if self.output.blocker_events != blockers_before {
            if self.last_blocker_detail.is_none() {
                self.last_blocker_detail = Some((
                    self.program
                        .arena
                        .span(self.program.arena.block(def.body).span),
                    "unsupported statement in body".to_string(),
                ));
            }
            return Err(CompactFunctionBlocker::Body);
        }
        let can_return = {
            let scratch = self.scratch.borrow();
            lowered_body_can_return(&scratch, &body)
        };
        if !can_return {
            if matches!(return_kind, LoweredReturnKind::Plain(LoweredType::Stream)) {
            } else if lowered_return_kind_accepts_unit_fallthrough(return_kind) {
                body.push(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Return {
                        value: push_build_row!(self, expr, BuildExprRow::Unit),
                    }
                ));
            } else {
                self.last_blocker_detail = Some((
                    self.program
                        .arena
                        .span(self.program.arena.block(def.body).span),
                    "function body may fall through without returning".to_string(),
                ));
                return Err(CompactFunctionBlocker::NoReturn);
            }
        }
        let has_defers = {
            let scratch = self.scratch.borrow();
            lowered_body_has_defers(&scratch, &body)
        };
        Ok(FunctionBuild {
            params,
            param_kinds,
            param_checks,
            param_rest,
            param_defaults,
            captures,
            return_kind,
            return_check: self
                .declarations
                .function_return_types
                .get(&body_span)
                .cloned()
                .or_else(|| {
                    Some(compact_runtime_type_in_namespace(
                        &self.program.arena,
                        def.return_ty,
                        self.declarations,
                        self.current_namespace,
                    ))
                })
                .filter(Type::has_unsigned_constraint)
                .map(|ty| LoweredTypeCheck {
                    name: Arc::from(ty.to_string()),
                    ty,
                    schema: None,
                }),
            slot_count: slots.count(),
            body,
            has_defers,
            scratch: self.scratch.clone(),
        })
    }

    fn record_top_level_blocker_detail(&mut self, id: StmtId, blocker: CompactTopLevelBlocker) {
        if let ArenaStmtKind::Export(inner)
        | ArenaStmtKind::Sugar {
            expansion: inner, ..
        } = self.program.arena.stmt(id).kind
        {
            self.record_top_level_blocker_detail(inner, blocker);
            return;
        }
        let label = blocker.label().to_string();
        let samples = self
            .output
            .top_level_blocker_sample_spans
            .entry(label)
            .or_default();
        if samples.len() < 8 {
            samples.push(self.program.arena.stmt(id).span);
        }
        let kind = self.program.arena.stmt(id).kind;
        let index = compact_stmt_kind_index(kind);
        self.output.top_level_blocker_stmt_kinds[index] += 1;
        match self.program.arena.stmt(id).kind {
            ArenaStmtKind::Let {
                ty,
                initializer: ArenaExprOrRun::Expr(value),
                ..
            }
            | ArenaStmtKind::Const {
                ty,
                initializer: ArenaExprOrRun::Expr(value),
                ..
            }
            | ArenaStmtKind::Var {
                ty,
                initializer: ArenaExprOrRun::Expr(value),
                ..
            } => match blocker {
                CompactTopLevelBlocker::BindingType => {
                    if let Some(ty) = ty {
                        let tag = self.program.arena.type_expr_tags[ty.index()];
                        self.output.top_level_binding_type_annotation_tags
                            [compact_type_expr_tag_index(tag)] += 1;
                    } else {
                        let kind = self.program.arena.expr(value).kind;
                        self.output.top_level_binding_type_expr_kinds
                            [compact_expr_kind_index(kind)] += 1;
                    }
                    if let Some(callee) = compact_expr_call_blocker_callee(self.program, value) {
                        self.output.top_level_binding_type_call_blockers
                            [compact_call_blocker_index(self.program, callee)] += 1;
                        record_compact_call_blocker_label(
                            &mut self.output.top_level_binding_type_call_callees,
                            self.program,
                            callee,
                        );
                    }
                }
                CompactTopLevelBlocker::BindingExpression => {
                    let kind = self.program.arena.expr(value).kind;
                    self.output.top_level_binding_expression_expr_kinds
                        [compact_expr_kind_index(kind)] += 1;
                    if let Some(callee) = compact_expr_call_blocker_callee(self.program, value) {
                        self.output.top_level_binding_expression_call_blockers
                            [compact_call_blocker_index(self.program, callee)] += 1;
                        record_compact_call_blocker_label(
                            &mut self.output.top_level_binding_expression_call_callees,
                            self.program,
                            callee,
                        );
                    }
                }
                _ => {}
            },
            ArenaStmtKind::Expr(value) if matches!(blocker, CompactTopLevelBlocker::Expression) => {
                let kind = self.program.arena.expr(value).kind;
                self.output.top_level_expression_expr_kinds[compact_expr_kind_index(kind)] += 1;
                if let Some(callee) = compact_expr_call_blocker_callee(self.program, value) {
                    self.output.top_level_expression_call_blockers
                        [compact_call_blocker_index(self.program, callee)] += 1;
                    record_compact_call_blocker_label(
                        &mut self.output.top_level_expression_call_callees,
                        self.program,
                        callee,
                    );
                }
            }
            ArenaStmtKind::Command(command)
                if matches!(blocker, CompactTopLevelBlocker::Command) =>
            {
                let index = compact_command_blocker_index(self.program, command);
                self.output.top_level_command_kinds[index] += 1;
            }
            _ => {}
        }
    }

    fn top_level_blocker_kind(&self, id: StmtId) -> CompactTopLevelBlocker {
        match self.program.arena.stmt(id).kind {
            ArenaStmtKind::Export(inner)
            | ArenaStmtKind::Sugar {
                expansion: inner, ..
            } => self.top_level_blocker_kind(inner),
            ArenaStmtKind::Use(_) => CompactTopLevelBlocker::Use,
            ArenaStmtKind::Let { target, ty, .. }
            | ArenaStmtKind::Const { target, ty, .. }
            | ArenaStmtKind::Var { target, ty, .. } => {
                if simple_binding_target(self.program, target).is_none() {
                    return CompactTopLevelBlocker::BindingTarget;
                }
                if ty.is_some_and(|ty| {
                    lowered_arena_type(&self.program.arena, ty, self.declarations).is_none()
                }) {
                    return CompactTopLevelBlocker::BindingType;
                }
                CompactTopLevelBlocker::BindingExpression
            }
            ArenaStmtKind::Assign {
                target,
                value: ArenaExprOrRun::Expr(_),
                ..
            } => {
                if !matches!(
                    self.program.arena.assign_target(target).kind,
                    ArenaAssignTargetKind::Name(_)
                ) {
                    return CompactTopLevelBlocker::AssignTarget;
                }
                CompactTopLevelBlocker::AssignExpression
            }
            ArenaStmtKind::Assign { .. } => CompactTopLevelBlocker::AssignExpression,
            ArenaStmtKind::Assert { .. }
            | ArenaStmtKind::If { .. }
            | ArenaStmtKind::While { .. }
            | ArenaStmtKind::For { .. }
            | ArenaStmtKind::Match { .. } => CompactTopLevelBlocker::Control,
            ArenaStmtKind::Command(_) => CompactTopLevelBlocker::Command,
            ArenaStmtKind::Expr(_) | ArenaStmtKind::Exit(_) => CompactTopLevelBlocker::Expression,
            ArenaStmtKind::Defer(..) => CompactTopLevelBlocker::Defer,
            ArenaStmtKind::SignalHook(_) => CompactTopLevelBlocker::Other,
            _ => CompactTopLevelBlocker::Other,
        }
    }

    fn lower_top_level_stmt(
        &mut self,
        id: StmtId,
        known: &FxHashMap<Name, LoweredTopLevelBinding>,
    ) -> Option<BuildTopStmtId> {
        match self.program.arena.stmt(id).kind {
            ArenaStmtKind::Export(inner)
            | ArenaStmtKind::Sugar {
                expansion: inner, ..
            } => self.lower_top_level_stmt(inner, known),
            ArenaStmtKind::Use(use_id) => {
                let use_stmt = self.program.arena.use_stmt(use_id);
                let key = use_stmt.resolved.clone()?;
                let path = self.program.arena.names(use_stmt.path).collect::<Vec<_>>();
                let namespace = compact_use_import_namespace(self.program, use_id)?;
                let exports = compact_module_exports_for_use(
                    self.program,
                    key.as_ref(),
                    namespace,
                    self.functions,
                )?;
                let module = self
                    .program
                    .modules
                    .iter()
                    .find(|module| module.key.as_str() == key.as_ref())?;
                let module_statement_ids =
                    self.program.module_statements(module).collect::<Vec<_>>();
                let module_lowered = {
                    let mut probe = CompactLowerConstructProbe {
                        program: self.program,
                        declarations: self.declarations,
                        bodies: self.bodies,
                        source: self.source,
                        sources: self.sources,
                        current_namespace: Some(module.name),
                        functions: self.functions,
                        top_level_known: compact_top_level_known(
                            self.program,
                            self.declarations,
                            self.source,
                            self.sources,
                            Some(module.name),
                            self.functions,
                        ),
                        output: CompactLowerConstructProbeOutput::default(),
                        last_blocker_detail: None,
                        stdlib_linkage: StdlibLowerLinkage::Local,
                        function_defs: Rc::new(RefCell::new(None)),
                        scratch: self.scratch.clone(),
                        spread_programs: Rc::clone(&self.spread_programs),
                    };
                    probe.lower_program_statements(&module_statement_ids)
                };
                let module_statements = module_statement_ids
                    .into_iter()
                    .zip(module_lowered.statements)
                    .filter_map(|(stmt, lowered)| {
                        Some((self.program.arena.stmt(stmt).span, lowered?))
                    })
                    .collect();
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::Use {
                        key,
                        alias: use_stmt.alias,
                        path,
                        namespace,
                        exports,
                        module_statements,
                        span: self.program.arena.stmt(id).span,
                    },
                    known,
                    SlotScope::default(),
                ))
            }
            ArenaStmtKind::Let {
                target,
                ty,
                initializer: initializer @ ArenaExprOrRun::Expr(value),
            }
            | ArenaStmtKind::Const {
                target,
                ty,
                initializer: initializer @ ArenaExprOrRun::Expr(value),
            }
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer: initializer @ ArenaExprOrRun::Expr(value),
            } => {
                if let ArenaBindingTargetKind::Record { .. } =
                    self.program.arena.binding_target(target).kind
                {
                    let mut slots = top_level_slots(known);
                    let checked = self.top_level_binding_checked_type(ty, value);
                    let source = self.lower_binding_expr_value(
                        ty,
                        checked.as_ref(),
                        value,
                        self.program.arena.stmt(id).span,
                        &mut slots,
                        None,
                        None,
                    )?;
                    let target =
                        self.lower_comp_target_typed(target, &mut slots, checked.as_ref())?;
                    let field_names = slots
                        .indices
                        .iter()
                        .filter_map(|(name, slot)| {
                            if known.contains_key(name) {
                                None
                            } else {
                                Some((*name, *slot))
                            }
                        })
                        .collect();
                    return Some(lowered_top_level(
                        &self.scratch,
                        BuildTopKind::LetRecord {
                            source,
                            fields: field_names,
                            target,
                            mutable: matches!(
                                self.program.arena.stmt(id).kind,
                                ArenaStmtKind::Var { .. }
                            ),
                            span: self.program.arena.stmt(id).span,
                        },
                        known,
                        slots,
                    ));
                }
                let target = simple_binding_target(self.program, target)?;
                if is_discard_name(target) {
                    let mut slots = top_level_slots(known);
                    let value = self.lower_expr(value, &mut slots, None, None)?;
                    return Some(lowered_top_level(
                        &self.scratch,
                        BuildTopKind::Discard {
                            value,
                            span: match initializer {
                                ArenaExprOrRun::Expr(value) => self.program.arena.expr(value).span,
                                ArenaExprOrRun::Run(_) => unreachable!("expr initializer matched"),
                            },
                        },
                        known,
                        slots,
                    ));
                }
                let annotation = ty;
                let (ty, validation) = match ty {
                    Some(ty) => {
                        let lowered =
                            lowered_arena_type(&self.program.arena, ty, self.declarations)?;
                        (
                            Some(lowered),
                            compact_type_check(
                                lowered,
                                &self.program.arena,
                                ty,
                                self.declarations,
                                self.current_namespace,
                            ),
                        )
                    }
                    None => (None, None),
                };
                let mut slots = top_level_slots(known);
                let checked = self.lower_binding_checked_type(annotation, value);
                let value = if self.is_empty_record_in_map_context(value, annotation) {
                    push_build_row!(self, expr, BuildExprRow::EmptyMap)
                } else {
                    self.lower_expr(value, &mut slots, None, None)?
                };
                let value = if annotation.is_none() {
                    match checked {
                        Some(ty) => self.checked_unsigned_value(
                            value,
                            &ty,
                            self.program.arena.stmt(id).span,
                        ),
                        None => value,
                    }
                } else {
                    value
                };
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::Let {
                        target,
                        ty,
                        validation,
                        mutable: matches!(
                            self.program.arena.stmt(id).kind,
                            ArenaStmtKind::Var { .. }
                        ),
                        value,
                        value_span: match initializer {
                            ArenaExprOrRun::Expr(value) => self.program.arena.expr(value).span,
                            ArenaExprOrRun::Run(_) => unreachable!("expr initializer matched"),
                        },
                    },
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::Let {
                target,
                ty,
                initializer: ArenaExprOrRun::Run(run),
            }
            | ArenaStmtKind::Const {
                target,
                ty,
                initializer: ArenaExprOrRun::Run(run),
            }
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer: ArenaExprOrRun::Run(run),
            } => {
                let target = simple_binding_target(self.program, target)?;
                if is_discard_name(target) {
                    let mut slots = top_level_slots(known);
                    let value = self.lower_run_binding_value(run, &mut slots, None, None)?;
                    return Some(lowered_top_level(
                        &self.scratch,
                        BuildTopKind::Discard {
                            value,
                            span: self
                                .program
                                .arena
                                .span(self.program.arena.run_form(run).span),
                        },
                        known,
                        slots,
                    ));
                }
                let (ty, validation) = match ty {
                    Some(ty) => {
                        let lowered =
                            lowered_arena_type(&self.program.arena, ty, self.declarations)?;
                        (
                            Some(lowered),
                            compact_type_check(
                                lowered,
                                &self.program.arena,
                                ty,
                                self.declarations,
                                self.current_namespace,
                            ),
                        )
                    }
                    None => (None, None),
                };
                let mut slots = top_level_slots(known);
                let value = self.lower_run_binding_value(run, &mut slots, None, None)?;
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::Let {
                        target,
                        ty,
                        validation,
                        mutable: matches!(
                            self.program.arena.stmt(id).kind,
                            ArenaStmtKind::Var { .. }
                        ),
                        value,
                        value_span: self
                            .program
                            .arena
                            .span(self.program.arena.run_form(run).span),
                    },
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::Assign { target, op, value } => {
                let ArenaAssignTargetKind::Name(target) =
                    self.program.arena.assign_target(target).kind
                else {
                    // An environment assignment writes no binding.
                    if let Some(root) = self.assign_target_root_name(target)
                        && !known.get(&root).is_some_and(|binding| binding.mutable)
                    {
                        return None;
                    }
                    let mut slots = top_level_slots(known);
                    let lowered = self.lower_stmt_with_blocker_guard(id, &mut slots, None, None)?;
                    return Some(lowered_top_level(
                        &self.scratch,
                        BuildTopKind::Stmt(lowered),
                        known,
                        slots,
                    ));
                };
                if known.get(&target).is_some_and(|binding| {
                    binding
                        .checked
                        .as_ref()
                        .is_some_and(Type::has_unsigned_constraint)
                }) {
                    let mut slots = top_level_slots(known);
                    let lowered = self.lower_stmt_with_blocker_guard(id, &mut slots, None, None)?;
                    return Some(lowered_top_level(
                        &self.scratch,
                        BuildTopKind::Stmt(lowered),
                        known,
                        slots,
                    ));
                }
                let mut slots = top_level_slots(known);
                let value = match value {
                    ArenaExprOrRun::Expr(expr) => self.lower_expr(expr, &mut slots, None, None)?,
                    ArenaExprOrRun::Run(run) => {
                        self.lower_run_binding_value(run, &mut slots, None, None)?
                    }
                };
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::Assign {
                        target,
                        op,
                        value,
                        span: self.program.arena.stmt(id).span,
                    },
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::Guard { .. } => {
                // The success bindings outlive the statement, so the guard is
                // the source of a binding step that publishes its new slots.
                let mut slots = top_level_slots(known);
                let outer = slots.indices.clone();
                let span = self.program.arena.stmt(id).span;
                let guard = self.lower_stmt_with_blocker_guard(id, &mut slots, None, None)?;
                let fields = slots
                    .indices
                    .iter()
                    .filter(|(name, slot)| outer.get(*name) != Some(*slot))
                    .map(|(name, slot)| (*name, *slot))
                    .collect();
                let source = push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ValueBlock {
                        body: vec![guard],
                        span
                    }
                );
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::LetRecord {
                        source,
                        fields,
                        target: LoweredCompTarget::Discard,
                        mutable: false,
                        span,
                    },
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::Assert { .. }
            | ArenaStmtKind::If { .. }
            | ArenaStmtKind::While { .. }
            | ArenaStmtKind::For { .. }
            | ArenaStmtKind::Match { .. }
            | ArenaStmtKind::Loop { .. }
            | ArenaStmtKind::With { .. } => {
                let mut slots = top_level_slots(known);
                let lowered = self.lower_stmt_with_blocker_guard(id, &mut slots, None, None)?;
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::Stmt(lowered),
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::Command(command) => {
                let mut slots = top_level_slots(known);
                let lowered = self
                    .lower_print_stmt(command, &mut slots, None, None)
                    .or_else(|| self.lower_cd_stmt(command, &mut slots, None, None))
                    .or_else(|| self.lower_env_stmt(command, &mut slots, None, None))
                    .or_else(|| self.lower_run_stmt(command, &mut slots, None, None))
                    .or_else(|| self.lower_proc_stmt(command, &mut slots, None, None))?;
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::Stmt(lowered),
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::Expr(value) => {
                let mut slots = top_level_slots(known);
                let value = self.lower_expr(value, &mut slots, None, None)?;
                let kind = BuildTopKind::Expr(value);
                Some(lowered_top_level(&self.scratch, kind, known, slots))
            }
            ArenaStmtKind::Exit(status) => {
                let mut slots = top_level_slots(known);
                let span = self.program.arena.stmt(id).span;
                let value = self.lower_exit(status, span, &mut slots, None, None)?;
                let kind = BuildTopKind::Expr(value);
                Some(lowered_top_level(&self.scratch, kind, known, slots))
            }
            ArenaStmtKind::Defer(ArenaExprOrRun::Expr(value), trigger) => {
                let mut slots = top_level_slots(known);
                let value = self.lower_deferred_expr(value, &mut slots, None, None)?;
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::Defer {
                        value,
                        span: self.program.arena.stmt(id).span,
                        on_error: trigger == DeferTrigger::Error,
                    },
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::Defer(ArenaExprOrRun::Run(run), trigger) => {
                let mut slots = top_level_slots(known);
                let value = self.lower_run_binding_value(run, &mut slots, None, None)?;
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::Defer {
                        value,
                        span: self.program.arena.stmt(id).span,
                        on_error: trigger == DeferTrigger::Error,
                    },
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::SignalHook(hook_id) => {
                let hook = self.program.arena.signal_hook(hook_id);
                let mut slot_scope = top_level_slots(known);
                let body = self.lower_block(hook.body, &mut slot_scope, None, None)?;
                let slot_count = slot_scope.count();
                let hook_slots: Vec<LoweredTopLevelSlot> = known
                    .iter()
                    .filter_map(|(&name, binding)| {
                        slot_scope.resolve(name).map(|slot| LoweredTopLevelSlot {
                            name,
                            slot,
                            kind: binding.kind,
                            mutable: binding.mutable,
                        })
                    })
                    .collect();
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::SignalHook {
                        signal: hook.signal,
                        pre_cancel: hook.options.pre_cancel.clone(),
                        body,
                        slots: hook_slots,
                        slot_count,
                        span: self.program.arena.stmt(id).span,
                    },
                    known,
                    slot_scope,
                ))
            }
            _ => None,
        }
    }

    fn record_top_level_binding(
        &self,
        id: StmtId,
        known: &mut FxHashMap<Name, LoweredTopLevelBinding>,
    ) {
        let stmt = self.program.arena.stmt(id);
        match stmt.kind {
            // `export let X = …` inside a module makes `X` a module-scope binding
            // that the module's functions may capture; record the inner binding.
            ArenaStmtKind::Export(inner) => self.record_top_level_binding(inner, known),
            ArenaStmtKind::Use(use_id) => {
                let use_stmt = self.program.arena.use_stmt(use_id);
                let Some(resolved) = use_stmt.resolved.as_ref() else {
                    return;
                };
                let Some(namespace) = compact_use_import_namespace(self.program, use_id) else {
                    return;
                };
                if use_stmt.alias.is_none()
                    && let Some(module) = self
                        .program
                        .modules
                        .iter()
                        .find(|module| module.key.as_str() == resolved.as_ref())
                {
                    for stmt in self.program.module_statements(module) {
                        let ArenaStmtKind::Export(inner) = self.program.arena.stmt(stmt).kind
                        else {
                            continue;
                        };
                        match self.program.arena.stmt(inner).kind {
                            ArenaStmtKind::Let {
                                target,
                                ty,
                                initializer: ArenaExprOrRun::Expr(_),
                            }
                            | ArenaStmtKind::Const {
                                target,
                                ty,
                                initializer: ArenaExprOrRun::Expr(_),
                            }
                            | ArenaStmtKind::Var {
                                target,
                                ty,
                                initializer: ArenaExprOrRun::Expr(_),
                            } => {
                                let Some(name) = simple_binding_target(self.program, target) else {
                                    continue;
                                };
                                if is_discard_name(name) {
                                    continue;
                                }
                                known.insert(
                                    name,
                                    LoweredTopLevelBinding {
                                        kind: self.top_level_binding(inner, ty, false).kind,
                                        checked: None,
                                        mutable: false,
                                        slot: true,
                                    },
                                );
                            }
                            ArenaStmtKind::ProcDef(def) => {
                                let name = self.program.arena.function_def(def).name;
                                known.insert(
                                    name,
                                    LoweredTopLevelBinding {
                                        kind: LoweredType::Proc,
                                        checked: None,
                                        mutable: false,
                                        slot: false,
                                    },
                                );
                            }
                            ArenaStmtKind::PureDef(def) => {
                                let name = self.program.arena.function_def(def).name;
                                known.insert(
                                    name,
                                    LoweredTopLevelBinding {
                                        kind: LoweredType::Pure,
                                        checked: None,
                                        mutable: false,
                                        slot: false,
                                    },
                                );
                            }
                            _ => {}
                        }
                    }
                }
                known.insert(
                    namespace,
                    LoweredTopLevelBinding {
                        kind: LoweredType::Module,
                        checked: None,
                        mutable: false,
                        slot: true,
                    },
                );
            }
            ArenaStmtKind::Let {
                target,
                ty,
                initializer,
            }
            | ArenaStmtKind::Const {
                target,
                ty,
                initializer,
            }
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer,
            } => {
                let mutable = matches!(stmt.kind, ArenaStmtKind::Var { .. });
                if let (ArenaBindingTargetKind::Record { .. }, ArenaExprOrRun::Expr(value)) =
                    (&self.program.arena.binding_target(target).kind, initializer)
                {
                    let checked = self.top_level_binding_checked_type(ty, value);
                    for (name, checked) in
                        record_binding_types(self.program, target, checked.as_ref())
                    {
                        let kind = checked
                            .as_ref()
                            .and_then(lowered_checked_type)
                            .unwrap_or(LoweredType::Any);
                        known.insert(
                            name,
                            LoweredTopLevelBinding {
                                kind,
                                checked,
                                mutable,
                                slot: true,
                            },
                        );
                    }
                    return;
                }
                let Some(name) = simple_binding_target(self.program, target) else {
                    return;
                };
                if !is_discard_name(name) {
                    known.insert(name, self.top_level_binding(id, ty, mutable));
                }
            }
            ArenaStmtKind::Guard {
                target,
                initializer,
                ..
            } => {
                let checked = match initializer {
                    ArenaExprOrRun::Expr(value) => self.concrete_checked_type(value),
                    ArenaExprOrRun::Run(_) => None,
                };
                let optional = self.bodies.optional_binding_guards.contains(&id);
                for (name, checked) in record_binding_types(
                    self.program,
                    target,
                    checked
                        .as_ref()
                        .and_then(|ty| guard_bound_type(ty, optional)),
                ) {
                    let kind = checked
                        .as_ref()
                        .and_then(lowered_checked_type)
                        .unwrap_or(LoweredType::Any);
                    known.insert(
                        name,
                        LoweredTopLevelBinding {
                            kind,
                            checked,
                            mutable: false,
                            slot: true,
                        },
                    );
                }
            }
            _ => {}
        }
    }

    /// A top-level binding's representation comes from its annotation, else
    /// from the checker's published binding type.
    fn top_level_binding(
        &self,
        id: StmtId,
        ty: Option<TypeExprId>,
        mutable: bool,
    ) -> LoweredTopLevelBinding {
        if let Some(ty) = ty {
            #[cfg(debug_assertions)]
            self.note_annotation_drift(id, ty);
            return LoweredTopLevelBinding {
                kind: lowered_arena_type(&self.program.arena, ty, self.declarations)
                    .unwrap_or(LoweredType::Any),
                checked: Some(compact_runtime_type_in_namespace(
                    &self.program.arena,
                    ty,
                    self.declarations,
                    self.current_namespace,
                )),
                mutable,
                slot: true,
            };
        }
        let checked = self
            .declarations
            .local_binding_types
            .get(&self.program.arena.stmt(id).span);
        LoweredTopLevelBinding {
            kind: checked
                .and_then(lowered_checked_type)
                .unwrap_or(LoweredType::Any),
            checked: checked.filter(|ty| concrete_checked_fact(ty)).cloned(),
            mutable,
            slot: true,
        }
    }

    /// Lowering interprets binding annotations itself; the checker publishes
    /// its own reading of the same annotation as the binding type.
    #[cfg(debug_assertions)]
    fn note_annotation_drift(&self, id: StmtId, ty: TypeExprId) {
        let span = self.program.arena.stmt(id).span;
        let kind = lowered_arena_type(&self.program.arena, ty, self.declarations);
        let annotated = compact_runtime_type_in_namespace(
            &self.program.arena,
            ty,
            self.declarations,
            self.current_namespace,
        );
        note_lowering_drift(
            span,
            kind,
            &annotated,
            self.declarations.local_binding_types.get(&span),
        );
    }

    fn top_level_binding_checked_type(
        &self,
        ty: Option<TypeExprId>,
        value: ExprId,
    ) -> Option<Type> {
        ty.map(|ty| {
            compact_runtime_type_in_namespace(
                &self.program.arena,
                ty,
                self.declarations,
                self.current_namespace,
            )
        })
        .or_else(|| self.concrete_checked_type(value))
    }

    fn lower_stmt_with_blocker_guard(
        &mut self,
        id: StmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let blockers_before = self.output.blocker_events;
        match self.lower_stmt(id, slots, current_function, item_slot) {
            Some(stmt) => Some(stmt),
            None => {
                if self.output.blocker_events == blockers_before {
                    self.record_lower_stmt_blocker(id);
                    self.output.constructed_statements += 1;
                    self.output.blocker_events += 1;
                }
                None
            }
        }
    }

    fn lower_stmt(
        &mut self,
        id: StmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        if let ArenaStmtKind::Sugar { expansion, .. } = self.program.arena.stmt(id).kind {
            return self.lower_stmt(expansion, slots, current_function, item_slot);
        }
        self.output.statements += 1;
        let lowered = match self.program.arena.stmt(id).kind {
            ArenaStmtKind::Sugar { .. } => unreachable!("lowered through its expansion above"),
            ArenaStmtKind::Let {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(value),
            }
            | ArenaStmtKind::Const {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(value),
            }
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(value),
            } => {
                if let ArenaBindingTargetKind::Record { .. } =
                    self.program.arena.binding_target(target).kind
                {
                    let checked = self.lower_binding_checked_type(ty, value);
                    let source = self.lower_binding_expr_value(
                        ty,
                        checked.as_ref(),
                        value,
                        self.program.arena.stmt(id).span,
                        slots,
                        current_function,
                        item_slot,
                    )?;
                    let target = self.lower_comp_target_typed(target, slots, checked.as_ref())?;
                    return Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::LetRecord {
                            source,
                            target,
                            span: self.program.arena.stmt(id).span,
                        }
                    ));
                }
                let name = simple_binding_target(self.program, target)?;
                if is_discard_name(name) {
                    return Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::Expr {
                            value: self.lower_expr(value, slots, current_function, item_slot)?,
                            span: self.program.arena.stmt(id).span,
                        }
                    ));
                }
                // A declaration may shadow a binding an enclosing scope made:
                // the inner scope resolves the new slot and the outer binding
                // comes back when it ends. Only a name this same scope already
                // declared is refused, which the checker reports before
                // lowering ever runs.
                if slots.is_declared_here(name) {
                    {
                        self.record_lower_stmt_blocker(id);
                        self.output.constructed_statements += 1;
                        return Some(push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::Expr {
                                value: push_build_row!(self, expr, BuildExprRow::Unit),
                                span: self.program.arena.stmt(id).span,
                            }
                        ));
                    }
                }
                #[cfg(debug_assertions)]
                if let Some(ty) = ty {
                    self.note_annotation_drift(id, ty);
                }
                let binding_ty = self
                    .declarations
                    .local_binding_types
                    .get(&self.program.arena.stmt(id).span)
                    .cloned()
                    .or_else(|| self.lower_binding_checked_type(ty, value));
                if let Some(ty) = ty
                    && lowered_arena_type(&self.program.arena, ty, self.declarations).is_none()
                    && !matches!(binding_ty, Some(ref ty) if !matches!(ty, Type::Unknown | Type::Invalid))
                {
                    return None;
                }
                let value = if self.is_empty_record_in_map_context(value, ty) {
                    push_build_row!(self, expr, BuildExprRow::EmptyMap)
                } else {
                    self.lower_binding_expr_value(
                        ty,
                        binding_ty.as_ref(),
                        value,
                        self.program.arena.stmt(id).span,
                        slots,
                        current_function,
                        item_slot,
                    )?
                };
                let binding_is_int = matches!(binding_ty, Some(Type::Int));
                let slot = slots.declare_with_type(name, binding_ty);
                if binding_is_int
                    && let Some(value) = self.lower_int_expr_candidate(&value)
                    && !self.lowered_int_expr_needs_type_context(&value)
                {
                    Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::LetInt { slot, value }
                    ))
                } else if let Some(value) = self.lower_bool_expr_candidate(&value)
                    && !self.lowered_bool_expr_needs_type_context(&value)
                {
                    Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::LetBool { slot, value }
                    ))
                } else {
                    Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::Let { slot, value }
                    ))
                }
            }
            ArenaStmtKind::Let {
                target,
                ty,
                initializer: ArenaExprOrRun::Run(run),
            }
            | ArenaStmtKind::Const {
                target,
                ty,
                initializer: ArenaExprOrRun::Run(run),
            }
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer: ArenaExprOrRun::Run(run),
            } => {
                let name = simple_binding_target(self.program, target)?;
                if is_discard_name(name) {
                    return Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::Expr {
                            value: self.lower_run_binding_value(
                                run,
                                slots,
                                current_function,
                                item_slot,
                            )?,
                            span: self.program.arena.stmt(id).span,
                        }
                    ));
                }
                // A declaration may shadow a binding an enclosing scope made:
                // the inner scope resolves the new slot and the outer binding
                // comes back when it ends. Only a name this same scope already
                // declared is refused, which the checker reports before
                // lowering ever runs.
                if slots.is_declared_here(name) {
                    {
                        self.record_lower_stmt_blocker(id);
                        self.output.constructed_statements += 1;
                        return Some(push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::Expr {
                                value: push_build_row!(self, expr, BuildExprRow::Unit),
                                span: self.program.arena.stmt(id).span,
                            }
                        ));
                    }
                }
                if let Some(ty) = ty {
                    lowered_arena_type(&self.program.arena, ty, self.declarations)?;
                }
                let value =
                    self.lower_run_binding_value(run, slots, current_function, item_slot)?;
                let binding_ty = ty
                    .map(|ty| {
                        compact_runtime_type_in_namespace(
                            &self.program.arena,
                            ty,
                            self.declarations,
                            self.current_namespace,
                        )
                    })
                    .or_else(|| {
                        self.declarations
                            .local_binding_types
                            .get(&self.program.arena.stmt(id).span)
                            .cloned()
                    });
                let slot = slots.declare_with_type(name, binding_ty);
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Let { slot, value }
                ))
            }
            ArenaStmtKind::Assign { target, value, .. }
                if let ArenaAssignTargetKind::Env(name) =
                    self.program.arena.assign_target(target).kind =>
            {
                self.lower_env_assignment(id, name, value, slots, current_function, item_slot)
            }
            ArenaStmtKind::Assign { target, op, value } => {
                let root_name = self.assign_target_root_name(target)?;
                let slot = if let Some(slot) = slots.resolve(root_name) {
                    slot
                } else if op == AssignOp::Set
                    && matches!(
                        self.program.arena.assign_target(target).kind,
                        ArenaAssignTargetKind::Name(_)
                    )
                {
                    slots.declare(root_name)
                } else {
                    {
                        self.record_lower_stmt_blocker(id);
                        self.output.constructed_statements += 1;
                        return Some(push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::Expr {
                                value: push_build_row!(self, expr, BuildExprRow::Unit),
                                span: self.program.arena.stmt(id).span,
                            }
                        ));
                    }
                };
                let check = self
                    .assign_target_checked_type(target, slots)
                    .filter(Type::has_unsigned_constraint)
                    .map(|ty| LoweredTypeCheck {
                        name: Arc::from(ty.to_string()),
                        ty,
                        schema: None,
                    });
                let value_is_int = match value {
                    ArenaExprOrRun::Expr(expr) => {
                        matches!(self.checked_expr_type(expr), Some(Type::Int))
                    }
                    ArenaExprOrRun::Run(_) => false,
                };
                let value = match value {
                    ArenaExprOrRun::Expr(expr) => {
                        self.lower_expr(expr, slots, current_function, item_slot)?
                    }
                    ArenaExprOrRun::Run(run) => {
                        self.lower_run_binding_value(run, slots, current_function, item_slot)?
                    }
                };
                if check.is_some() {
                    let span = self.program.arena.stmt(id).span;
                    return Some(match self.program.arena.assign_target(target).kind {
                        ArenaAssignTargetKind::Name(_) => push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::Assign {
                                slot,
                                op,
                                value,
                                check,
                                span
                            }
                        ),
                        _ => {
                            let path =
                                self.lower_assign_path(target, slots, current_function, item_slot)?;
                            push_build_row!(
                                self,
                                stmt,
                                BuildStmtRow::AssignPath {
                                    slot,
                                    path: LoweredAssignPath(path),
                                    op,
                                    value,
                                    check,
                                    span
                                }
                            )
                        }
                    });
                }
                match self.program.arena.assign_target(target).kind {
                    ArenaAssignTargetKind::Name(_) if op == AssignOp::Set => {
                        if let Some(value) = self.lower_bool_expr_candidate(&value)
                            && !self.lowered_bool_expr_needs_type_context(&value)
                        {
                            Some(push_build_row!(
                                self,
                                stmt,
                                BuildStmtRow::AssignBool { slot, value }
                            ))
                        } else if value_is_int
                            && let Some(value) = self.lower_int_expr_candidate(&value)
                            && !self.lowered_int_expr_needs_type_context(&value)
                        {
                            Some(push_build_row!(
                                self,
                                stmt,
                                BuildStmtRow::AssignInt {
                                    slot,
                                    op,
                                    value,
                                    span: self.program.arena.stmt(id).span,
                                }
                            ))
                        } else {
                            Some(push_build_row!(
                                self,
                                stmt,
                                BuildStmtRow::Assign {
                                    slot,
                                    op,
                                    value,
                                    check: None,
                                    span: self.program.arena.stmt(id).span,
                                }
                            ))
                        }
                    }
                    ArenaAssignTargetKind::Name(_) => Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::Assign {
                            slot,
                            op,
                            value,
                            check: None,
                            span: self.program.arena.stmt(id).span,
                        }
                    )),
                    ArenaAssignTargetKind::Field { base, name }
                        if matches!(
                            self.program.arena.assign_target(base).kind,
                            ArenaAssignTargetKind::Name(_)
                        ) =>
                    {
                        if value_is_int
                            && let Some(value) = self.lower_int_expr_candidate(&value)
                            && !self.lowered_int_expr_needs_type_context(&value)
                        {
                            Some(push_build_row!(
                                self,
                                stmt,
                                BuildStmtRow::AssignFieldInt {
                                    slot,
                                    field: Arc::<str>::from(name.as_str().as_str()),
                                    op,
                                    value,
                                    span: self.program.arena.stmt(id).span,
                                }
                            ))
                        } else {
                            Some(push_build_row!(
                                self,
                                stmt,
                                BuildStmtRow::AssignField {
                                    slot,
                                    field: Arc::<str>::from(name.as_str().as_str()),
                                    op,
                                    value,
                                    span: self.program.arena.stmt(id).span,
                                }
                            ))
                        }
                    }
                    _ => {
                        let path =
                            self.lower_assign_path(target, slots, current_function, item_slot)?;
                        Some(push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::AssignPath {
                                slot,
                                path: LoweredAssignPath(path),
                                op,
                                value,
                                check: None,
                                span: self.program.arena.stmt(id).span,
                            }
                        ))
                    }
                }
            }
            ArenaStmtKind::If {
                branches,
                else_block,
            } => {
                let branches = self.program.arena.if_branches(branches).to_vec();
                let has_pattern = branches.iter().any(|branch| {
                    matches!(
                        self.program.arena.expr(branch.condition).kind,
                        ArenaExprKind::PatternCondition { .. }
                    )
                });
                let mut lowered = Vec::with_capacity(branches.len());
                for branch in branches {
                    let saved = slots.enter();
                    let condition = self.lower_pattern_condition_parts(
                        branch.condition,
                        slots,
                        current_function,
                        item_slot,
                    );
                    let body = self.lower_block(branch.block, slots, current_function, item_slot);
                    slots.exit(saved);
                    let (condition, captures) = condition?;
                    lowered.push((condition, body?, captures));
                }
                let else_body = match else_block {
                    Some(block) => {
                        Some(self.lower_block(block, slots, current_function, item_slot)?)
                    }
                    None => None,
                };
                if lowered.iter().any(|(_, _, captures)| !captures.is_empty()) || has_pattern {
                    return Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::PatternIf {
                            branches: lowered,
                            else_body,
                            span: self.program.arena.stmt(id).span
                        }
                    ));
                }
                let lowered = lowered
                    .into_iter()
                    .map(|(condition, body, _)| (condition, body))
                    .collect::<Vec<_>>();
                let mut bool_branches = Vec::with_capacity(lowered.len());
                for (condition, body) in &lowered {
                    let Some(condition) = self.lower_bool_expr_candidate(condition) else {
                        return Some(push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::If {
                                branches: lowered,
                                else_body,
                            }
                        ));
                    };
                    bool_branches.push((condition, body.clone()));
                }
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::IfBool {
                        branches: bool_branches,
                        else_body,
                    }
                ))
            }
            ArenaStmtKind::While { condition, block } => {
                let has_pattern = matches!(
                    self.program.arena.expr(condition).kind,
                    ArenaExprKind::PatternCondition { .. }
                );
                let saved = slots.enter();
                let condition = self.lower_pattern_condition_parts(
                    condition,
                    slots,
                    current_function,
                    item_slot,
                );
                let body = self.lower_block(block, slots, current_function, item_slot);
                slots.exit(saved);
                let (condition, captures) = condition?;
                let body = body?;
                if has_pattern {
                    return Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::PatternWhile {
                            condition,
                            body,
                            captures,
                            span: self.program.arena.stmt(id).span
                        }
                    ));
                }
                if let Some(condition) = self.lower_bool_expr_candidate(&condition) {
                    Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::WhileBool { condition, body }
                    ))
                } else {
                    Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::While { condition, body }
                    ))
                }
            }
            ArenaStmtKind::For {
                target,
                iter,
                block,
            } => {
                if let ArenaBindingTargetKind::Record { .. } =
                    self.program.arena.binding_target(target).kind
                {
                    if !self.program.arena.block(block).params.is_empty() {
                        return None;
                    }
                    let item_ty = self.loop_item_checked_type(iter);
                    let iter =
                        self.lower_direct_iterable(iter, slots, current_function, item_slot)?;
                    let saved = slots.enter();
                    let target = self.lower_comp_target_typed(target, slots, item_ty.as_ref())?;
                    let body =
                        self.lower_block_in_current_scope(block, slots, current_function, None)?;
                    slots.exit(saved);
                    return Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::ForRecord {
                            target,
                            iter,
                            body,
                            span: self.program.arena.stmt(id).span,
                        }
                    ));
                }
                let name = simple_binding_target(self.program, target)?;
                if !self.program.arena.block(block).params.is_empty() {
                    {
                        self.record_lower_stmt_blocker(id);
                        self.output.constructed_statements += 1;
                        return Some(push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::Expr {
                                value: push_build_row!(self, expr, BuildExprRow::Unit),
                                span: self.program.arena.stmt(id).span,
                            }
                        ));
                    }
                }
                // Recognize `for line in <text>.lines()` and lower it to the
                // streaming ForStrLines node (avoids materializing the line list).
                let str_lines_base = if let ArenaExprKind::Call { callee, args } =
                    self.program.arena.expr(iter).kind
                {
                    if self.program.arena.call_args(args).is_empty() {
                        if let ArenaExprKind::Field { base, name } =
                            self.program.arena.expr(callee).kind
                        {
                            (name.as_str() == "lines").then_some(base)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
                // The text/iter is evaluated once, before the loop scope opens.
                let text_or_iter = self.lower_direct_iterable(
                    str_lines_base.unwrap_or(iter),
                    slots,
                    current_function,
                    item_slot,
                )?;
                let item_ty = if let Some(base) = str_lines_base {
                    self.checked_expr_type(base)
                        .or_else(|| self.concrete_checked_type(base))
                        .and_then(|ty| match ty {
                            Type::Bytes => Some(Type::Bytes),
                            Type::Str => Some(Type::Str),
                            _ => None,
                        })
                        .or(Some(Type::Str))
                } else {
                    self.loop_item_checked_type(iter)
                };
                // The loop variable is declared in the loop's own scope, so it may
                // shadow an outer binding; `exit` restores the outer slot.
                let saved = slots.enter();
                let slot = slots.declare_with_type(name, item_ty);
                let body =
                    self.lower_block_in_current_scope(block, slots, current_function, Some(slot))?;
                slots.exit(saved);
                let span = self.program.arena.stmt(id).span;
                if str_lines_base.is_some() {
                    let body = self.try_lower_scan_bytes(slot, &body, span).unwrap_or(body);
                    if let Some(scan) = self.try_lower_scan_lines(&text_or_iter, slot, &body, span)
                    {
                        Some(scan)
                    } else {
                        Some(push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::ForStrLines {
                                slot,
                                text: text_or_iter,
                                body,
                                span,
                            }
                        ))
                    }
                } else {
                    Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::For {
                            slot,
                            iter: text_or_iter,
                            body,
                            span,
                        }
                    ))
                }
            }
            ArenaStmtKind::Match { value, arms } => {
                if let Some(stmt) = self.lower_str_match_stmt(
                    value,
                    arms,
                    self.program.arena.stmt(id).span,
                    slots,
                    current_function,
                    item_slot,
                ) {
                    return Some(stmt);
                }
                if let Some(stmt) = self.lower_tag_match_stmt(
                    value,
                    arms,
                    self.program.arena.stmt(id).span,
                    slots,
                    current_function,
                    item_slot,
                ) {
                    return Some(stmt);
                }
                let (ok_binding_ty, err_binding_ty) =
                    self.compact_match_scrutinee_result_types(value);
                let value = self.lower_expr(value, slots, current_function, item_slot)?;
                let arms = self.program.arena.match_arms(arms).to_vec();
                let mut lowered_arms = Vec::with_capacity(arms.len());
                for arm in arms {
                    if !self.program.arena.block(arm.block).params.is_empty() {
                        {
                            self.record_lower_stmt_blocker(id);
                            self.output.constructed_statements += 1;
                            return Some(push_build_row!(
                                self,
                                stmt,
                                BuildStmtRow::Expr {
                                    value: push_build_row!(self, expr, BuildExprRow::Unit),
                                    span: self.program.arena.stmt(id).span,
                                }
                            ));
                        }
                    }
                    let (pattern, cleanup) = self.lower_pattern(
                        arm.pattern,
                        slots,
                        ok_binding_ty.as_ref(),
                        err_binding_ty.as_ref(),
                    )?;
                    let body = match self.lower_block(arm.block, slots, current_function, item_slot)
                    {
                        Some(body) => body,
                        None => {
                            cleanup_lowered_pattern_slots(slots, cleanup);
                            {
                                self.record_lower_stmt_blocker(id);
                                self.output.constructed_statements += 1;
                                return Some(push_build_row!(
                                    self,
                                    stmt,
                                    BuildStmtRow::Expr {
                                        value: push_build_row!(self, expr, BuildExprRow::Unit),
                                        span: self.program.arena.stmt(id).span,
                                    }
                                ));
                            }
                        }
                    };
                    let guard = match arm.guard {
                        Some(guard_expr) => {
                            Some(self.lower_expr(guard_expr, slots, current_function, item_slot)?)
                        }
                        None => None,
                    };
                    cleanup_lowered_pattern_slots(slots, cleanup);
                    lowered_arms.push((pattern, guard, body));
                }
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Match {
                        value,
                        arms: lowered_arms,
                        span: self.program.arena.stmt(id).span,
                    }
                ))
            }
            ArenaStmtKind::With {
                bindings,
                body,
                else_block,
            } => {
                let saved = slots.enter();
                let result = (|| {
                    let mut lowered_bindings = Vec::new();
                    let mut captures = Vec::new();
                    let mut error_ty = None;
                    for binding in self.program.arena.with_bindings(bindings).to_vec() {
                        let checked = self.lower_binding_checked_type(None, binding.initializer);
                        let input = match self.program.arena.expr(binding.initializer).kind {
                            ArenaExprKind::Try(input) => input,
                            _ => binding.initializer,
                        };
                        if let Some(Type::Result(_, error)) =
                            self.lower_binding_checked_type(None, input)
                        {
                            error_ty = Some(match error_ty {
                                None => *error,
                                Some(previous) if previous == *error => previous,
                                Some(_) => Type::Error,
                            });
                        }
                        let value = self.lower_expr(
                            binding.initializer,
                            slots,
                            current_function,
                            item_slot,
                        )?;
                        let ty = checked.map(|ty| match ty {
                            Type::Result(ok, _) => *ok,
                            other => other,
                        });
                        let slot = if binding.name.as_str() == "_" {
                            slots.reserve("with discard")
                        } else {
                            slots.declare_with_type(binding.name, ty)
                        };
                        captures.push(slot);
                        lowered_bindings.push((slot, value));
                    }
                    let body = self.lower_block(body, slots, current_function, item_slot)?;
                    Some((lowered_bindings, body, captures, error_ty))
                })();
                slots.exit(saved);
                let (bindings, body, mut captures, error_ty) = result?;
                let error_ty = self
                    .bodies
                    .handler_input_types
                    .get(&else_block)
                    .cloned()
                    .or(error_ty);
                let saved = slots.enter();
                let else_param_slot = self
                    .program
                    .arena
                    .block_params(self.program.arena.block(else_block).params)
                    .first()
                    .filter(|param| param.name.as_str() != "_")
                    .map(|param| slots.declare_with_type(param.name, error_ty));
                captures.extend(else_param_slot);
                let else_body = self.lower_block_in_current_scope(
                    else_block,
                    slots,
                    current_function,
                    item_slot,
                );
                slots.exit(saved);
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::With {
                        bindings,
                        body,
                        else_param_slot,
                        else_body: else_body?,
                        captures,
                        span: self.program.arena.stmt(id).span
                    }
                ))
            }
            ArenaStmtKind::Guard {
                target,
                initializer,
                else_block,
                ..
            } => {
                let checked = match initializer {
                    ArenaExprOrRun::Expr(expr) => self.lower_binding_checked_type(None, expr),
                    ArenaExprOrRun::Run(_) => None,
                };
                // The checker decided whether this guard unwraps a `Result` or
                // tests an optional for `null`.
                let optional = self.bodies.optional_binding_guards.contains(&id);
                let success_ty = checked
                    .as_ref()
                    .and_then(|ty| guard_bound_type(ty, optional));
                let value = match initializer {
                    ArenaExprOrRun::Expr(expr) => {
                        self.lower_expr(expr, slots, current_function, item_slot)?
                    }
                    ArenaExprOrRun::Run(run) => {
                        self.lower_run_binding_value(run, slots, current_function, item_slot)?
                    }
                };
                // The else block runs in its own scope with the error param
                // bound; the success binding lives in the enclosing scope.
                let saved = slots.enter();
                let else_param_slot = self
                    .program
                    .arena
                    .block_params(self.program.arena.block(else_block).params)
                    .first()
                    // A null optional carries no error to bind.
                    .filter(|param| !optional && param.name.as_str() != "_")
                    .map(|param| {
                        slots.declare_with_type(
                            param.name,
                            checked.as_ref().and_then(|ty| match ty {
                                Type::Result(_, error) => Some((**error).clone()),
                                _ => None,
                            }),
                        )
                    });
                let else_body = self.lower_block_in_current_scope(
                    else_block,
                    slots,
                    current_function,
                    item_slot,
                );
                slots.exit(saved);
                let else_body = else_body?;
                let target = self.lower_comp_target_typed(target, slots, success_ty)?;
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Guard {
                        target,
                        value,
                        else_param_slot,
                        else_body,
                        span: self.program.arena.stmt(id).span,
                        optional,
                    }
                ))
            }
            ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(value))) => Some(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Return {
                    value: self.lower_expr(value, slots, current_function, item_slot)?,
                }
            )),
            ArenaStmtKind::Return(Some(ArenaExprOrRun::Run(run))) => Some(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Return {
                    value: self.lower_run_binding_value(run, slots, current_function, item_slot)?,
                }
            )),
            ArenaStmtKind::Return(None) => Some(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Return {
                    value: push_build_row!(self, expr, BuildExprRow::Unit),
                }
            )),
            // The checker decided that this yield appends to a `collect`
            // expression: it adds to that expression's local.
            ArenaStmtKind::YieldDelegate(value) if self.bodies.collect_yields.contains_key(&id) => {
                let items = self.lower_expr(value, slots, current_function, item_slot)?;
                self.lower_collect_yield(id, items, slots)
            }
            ArenaStmtKind::Yield(value) if self.bodies.collect_yields.contains_key(&id) => {
                let item = match value {
                    ArenaExprOrRun::Expr(value) => {
                        self.lower_expr(value, slots, current_function, item_slot)?
                    }
                    ArenaExprOrRun::Run(run) => {
                        self.lower_run_binding_value(run, slots, current_function, item_slot)?
                    }
                };
                let items = push_build_row!(self, expr, BuildExprRow::List(vec![item]));
                self.lower_collect_yield(id, items, slots)
            }
            ArenaStmtKind::YieldDelegate(value) => Some(push_build_row!(
                self,
                stmt,
                BuildStmtRow::YieldDelegate {
                    value: self.lower_expr(value, slots, current_function, item_slot)?,
                    span: self.program.arena.stmt(id).span,
                }
            )),
            ArenaStmtKind::Yield(ArenaExprOrRun::Expr(value)) => Some(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Yield {
                    value: self.lower_expr(value, slots, current_function, item_slot)?,
                }
            )),
            ArenaStmtKind::Yield(ArenaExprOrRun::Run(run)) => Some(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Yield {
                    value: self.lower_run_binding_value(run, slots, current_function, item_slot)?,
                }
            )),
            ArenaStmtKind::Loop { block } => {
                let body = self.lower_block(block, slots, current_function, item_slot)?;
                Some(push_build_row!(self, stmt, BuildStmtRow::Loop { body }))
            }
            ArenaStmtKind::Break { value: None } => {
                Some(push_build_row!(self, stmt, BuildStmtRow::Break))
            }
            ArenaStmtKind::Break { value: Some(value) } => Some(push_build_row!(
                self,
                stmt,
                BuildStmtRow::BreakValue {
                    value: self.lower_expr(value, slots, current_function, item_slot)?,
                }
            )),
            ArenaStmtKind::Continue => Some(push_build_row!(self, stmt, BuildStmtRow::Continue)),
            ArenaStmtKind::Command(command) => self
                .lower_print_stmt(command, slots, current_function, item_slot)
                .or_else(|| self.lower_cd_stmt(command, slots, current_function, item_slot))
                .or_else(|| self.lower_env_stmt(command, slots, current_function, item_slot))
                .or_else(|| self.lower_run_stmt(command, slots, current_function, item_slot))
                .or_else(|| self.lower_proc_stmt(command, slots, current_function, item_slot)),
            ArenaStmtKind::Assert { condition, message } => {
                let span = self.program.arena.expr(condition).span;
                let value = self.lower_expr(condition, slots, current_function, item_slot)?;
                self.mark_comparison_chain_assertion(value);
                let message = match message {
                    Some(message) => {
                        Some(self.lower_expr(message, slots, current_function, item_slot)?)
                    }
                    None => None,
                };
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Assert {
                        value,
                        message,
                        span
                    }
                ))
            }
            ArenaStmtKind::Expr(value) => {
                let span = self.program.arena.stmt(id).span;
                let value = self.lower_expr(value, slots, current_function, item_slot)?;
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Expr { value, span }
                ))
            }
            ArenaStmtKind::Exit(status) => {
                let span = self.program.arena.stmt(id).span;
                let value = self.lower_exit(status, span, slots, current_function, item_slot)?;
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Expr { value, span }
                ))
            }
            ArenaStmtKind::Defer(ArenaExprOrRun::Expr(value), trigger) => {
                let value = self.lower_deferred_expr(value, slots, current_function, item_slot)?;
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Defer {
                        value,
                        on_error: trigger == DeferTrigger::Error,
                    }
                ))
            }
            ArenaStmtKind::Defer(ArenaExprOrRun::Run(run), trigger) => {
                let value =
                    self.lower_run_binding_value(run, slots, current_function, item_slot)?;
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Defer {
                        value,
                        on_error: trigger == DeferTrigger::Error,
                    }
                ))
            }
            ArenaStmtKind::TailBareIdent(name) => {
                let span = self.program.arena.stmt(id).span;
                let value = self.lower_bare_ident_stmt(id, name, slots)?;
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Expr { value, span }
                ))
            }
            _ => None,
        };
        match lowered {
            Some(lowered) => {
                self.output.constructed_statements += 1;
                Some(lowered)
            }
            None => {
                self.record_lower_stmt_blocker(id);
                self.output.constructed_statements += 1;
                self.output.blocker_events += 1;
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Expr {
                        value: push_build_row!(self, expr, BuildExprRow::Unit),
                        span: self.program.arena.stmt(id).span,
                    }
                ))
            }
        }
    }

    fn record_lower_stmt_blocker(&mut self, id: StmtId) -> Option<BuildStmtId> {
        let id = self.program.arena.core_stmt_id(id);
        let kind = self.program.arena.stmt(id).kind;
        let stmt_span = self.program.arena.stmt(id).span;
        self.output.statement_blockers[compact_stmt_kind_index(kind)] += 1;
        let label = compact_stmt_blocker_label(self.program, id);
        let keep_nested = self.last_blocker_detail.as_ref().is_some_and(|(span, _)| {
            span.source_id == stmt_span.source_id
                && span.start() >= stmt_span.start()
                && span.end() <= stmt_span.end()
        });
        if !keep_nested {
            self.last_blocker_detail = Some((stmt_span, format!("statement `{label}`")));
        }
        record_compact_stmt_blocker_span(
            &mut self.output.statement_blocker_sample_spans,
            self.program,
            id,
        );
        None
    }

    fn lower_expr(
        &mut self,
        id: ExprId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        // The checker decided that this condition operand's `Result[Bool]`
        // propagates. It is the operand under a `?`, and nothing here decides
        // that again.
        if slots.propagated_condition != Some(id)
            && self.bodies.propagating_conditions.contains(&id)
        {
            let outer = slots.propagated_condition.replace(id);
            let operand = self.lower_expr(id, slots, current_function, item_slot);
            slots.propagated_condition = outer;
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Try {
                    value: operand?,
                    span: self.program.arena.expr(id).span
                }
            ));
        }
        if let Some(value) = self.declarations.prepared_constants.values.get(&id) {
            let origin = self
                .declarations
                .prepared_constants
                .origins
                .get(&id)
                .copied()
                .unwrap_or(id);
            let cached = self
                .scratch
                .borrow()
                .prepared_constants
                .get(&origin)
                .cloned();
            let value = if let Some(value) = cached {
                value
            } else {
                let value = lower_literal_constant(value, Some(&self.declarations.wire_enums))?;
                self.scratch
                    .borrow_mut()
                    .prepared_constants
                    .insert(origin, value.clone());
                value
            };
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::PreparedConstant(super::PreparedConstantValue(value))
            ));
        }
        if let Some(receiver) = slots.postfix_receivers.get(&id) {
            return Some(*receiver);
        }
        if !slots.guarded_postfixes.contains(&id) {
            let base = match self.program.arena.expr(id).kind {
                ArenaExprKind::NullSafeField { base, .. }
                | ArenaExprKind::Index {
                    base,
                    guarded: true,
                    ..
                }
                | ArenaExprKind::Slice {
                    base,
                    guarded: true,
                    ..
                } => Some(base),
                ArenaExprKind::Call { callee, .. } => match self.program.arena.expr(callee).kind {
                    ArenaExprKind::NullSafeField { base, .. } => Some(base),
                    _ => None,
                },
                _ => None,
            };
            if let Some(base) = base {
                let ty = self
                    .checked_expr_type(base)
                    .or_else(|| {
                        self.bodies
                            .expr_types
                            .get(&base)
                            .filter(|ty| compact_checked_type_is_concrete(ty))
                            .cloned()
                    })
                    .or_else(|| self.concrete_checked_type(base));
                if matches!(ty, Some(Type::Optional(_))) {
                    return self.lower_optional_postfix(
                        id,
                        base,
                        slots,
                        current_function,
                        item_slot,
                    );
                }
            }
        }
        self.output.expressions += 1;
        let span = self.program.arena.expr(id).span;
        let checked_creation = matches!(
            self.program.arena.expr(id).kind,
            ArenaExprKind::List(_)
                | ArenaExprKind::ListComp { .. }
                | ArenaExprKind::MapComp { .. }
                | ArenaExprKind::Record(_)
                | ArenaExprKind::If { .. }
                | ArenaExprKind::Match { .. }
                | ArenaExprKind::Binary {
                    op: BinaryOp::ResultFallback,
                    ..
                }
                | ArenaExprKind::Call { .. }
        )
        .then(|| self.bodies.expr_types.get(&id).cloned())
        .flatten()
        .filter(Type::has_unsigned_constraint);
        let lowered = match self.program.arena.expr(id).kind {
            ArenaExprKind::Field { .. } | ArenaExprKind::Call { .. }
                if self.bodies.inferred_variants.contains_key(&id) =>
            {
                self.lower_inferred_variant(id, slots, current_function, item_slot)
            }
            ArenaExprKind::Null => Some(push_build_row!(self, expr, BuildExprRow::Null)),
            ArenaExprKind::Int(value) => self
                .program
                .arena
                .int_literal(value)
                .value()
                .map(|value| push_build_row!(self, expr, BuildExprRow::Int(value))),
            ArenaExprKind::Float(value) => self
                .program
                .arena
                .float_literal(value)
                .value()
                .map(crate::runtime::value::FloatValue::new)
                .map(|value| push_build_row!(self, expr, BuildExprRow::Float(value))),
            ArenaExprKind::Duration(value) => self
                .program
                .arena
                .duration_literal(value)
                .millis()
                .map(|millis| {
                    push_build_row!(self, expr, BuildExprRow::Duration(DurationValue { millis }))
                }),
            ArenaExprKind::Bool(value) => {
                Some(push_build_row!(self, expr, BuildExprRow::Bool(value)))
            }
            // A literal the checker typed as a Path is a Path constant, the
            // same value `p"..."` builds.
            ArenaExprKind::Str(value) if self.bodies.path_literals.contains(&id) => {
                let path = self.program.arena.string_literal(value);
                PathValue::from_text(path.as_ref())
                    .ok()
                    .map(|value| push_build_row!(self, expr, BuildExprRow::Path(value)))
            }
            ArenaExprKind::Str(value) => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Str(self.program.arena.string_literal(value).clone(),)
            )),
            ArenaExprKind::PathStr(value) => {
                let path = self.program.arena.string_literal(value);
                PathValue::from_text(path.as_ref())
                    .ok()
                    .map(|value| push_build_row!(self, expr, BuildExprRow::Path(value)))
            }
            ArenaExprKind::Regex(value) => {
                let literal = self.program.arena.regex_literal(value);
                let regex = literal.prepared.get()?.as_ref().ok()?.clone();
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::PreparedRegex(RegexValue {
                        pattern: literal.pattern.to_string(),
                        regex,
                    })
                ))
            }
            ArenaExprKind::Bytes(value) => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Bytes(self.program.arena.bytes_literal(value).clone(),)
            )),
            ArenaExprKind::Ident(name) => self.lower_bare_ident(name, slots),
            ArenaExprKind::Item => {
                item_slot.map(|slot| push_build_row!(self, expr, BuildExprRow::Param(slot)))
            }
            ArenaExprKind::FmtString(parts) => {
                self.lower_fmt_string(parts, slots, current_function, item_slot)
            }
            ArenaExprKind::PathFmtString(parts) => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::PathFmtString {
                    parts: self.lower_fmt_parts(parts, slots, current_function, item_slot)?,
                    span,
                }
            )),
            ArenaExprKind::GlobStr(value) => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Glob {
                    pattern: self.program.arena.string_literal(value).clone(),
                    span,
                }
            )),
            ArenaExprKind::LastStatus => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::LastStatus { span }
            )),
            ArenaExprKind::Run(run) => {
                self.lower_run_binding_value(run, slots, current_function, item_slot)
            }
            ArenaExprKind::Spawn(form) => {
                self.lower_spawn_expr(form.target, span, slots, current_function, item_slot)
            }
            ArenaExprKind::Wait(wait) => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Wait {
                    target: self.lower_expr(wait.target, slots, current_function, item_slot,)?,
                    span,
                }
            )),
            ArenaExprKind::Set(items) => {
                let items = self.program.arena.list_elements(items).collect::<Vec<_>>();
                let mut lowered = Vec::with_capacity(items.len());
                for item in items {
                    lowered.push(self.lower_expr(
                        item.value,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
                Some(self.list_as_set(lowered, span))
            }
            ArenaExprKind::SetComp {
                expr: body,
                qualifiers,
            } => {
                let saved = slots.enter();
                let qualifiers =
                    self.lower_comp_qualifiers(qualifiers, slots, current_function, item_slot)?;
                let value = self.lower_expr(body, slots, current_function, item_slot)?;
                slots.exit(saved);
                let list = push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ListComp {
                        value,
                        qualifiers,
                        span
                    }
                );
                Some(self.set_of_list(list, span))
            }
            // Braces of bare names that the checker read as a set.
            ArenaExprKind::Record(fields)
                if matches!(self.bodies.expr_types.get(&id), Some(Type::Set(_))) =>
            {
                let mut lowered = Vec::new();
                for field in self.program.arena.record_fields(fields).to_vec() {
                    let ArenaRecordFieldKind::Shorthand { name, .. } = field.kind else {
                        return None;
                    };
                    lowered.push(self.lower_bare_ident(name, slots)?);
                }
                Some(self.list_as_set(lowered, span))
            }
            ArenaExprKind::Record(fields) => {
                if self
                    .program
                    .arena
                    .record_fields(fields)
                    .iter()
                    .any(|field| matches!(field.kind, ArenaRecordFieldKind::Path { .. }))
                {
                    self.lower_record(fields, slots, current_function, item_slot)
                } else if self
                    .bodies
                    .expr_types
                    .get(&id)
                    .is_some_and(|ty| matches!(ty, Type::Map(_, _)))
                    || self
                        .program
                        .arena
                        .record_fields(fields)
                        .iter()
                        .any(|field| matches!(field.kind, ArenaRecordFieldKind::Computed { .. }))
                {
                    self.lower_map_literal(id, fields, slots, current_function, item_slot)
                } else {
                    self.lower_record(fields, slots, current_function, item_slot)
                }
            }
            ArenaExprKind::List(items) => {
                let items = self.program.arena.list_elements(items).collect::<Vec<_>>();
                if items.iter().any(|item| item.splice_span.is_some()) {
                    let mut lowered = Vec::with_capacity(items.len());
                    for item in items {
                        let item_span = item
                            .splice_span
                            .map(|span| self.program.arena.span(span))
                            .unwrap_or(self.program.arena.expr(item.value).span);
                        lowered.push((
                            item.splice_span.is_some(),
                            self.lower_expr(item.value, slots, current_function, item_slot)?,
                            item_span,
                        ));
                    }
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ListBuild(lowered)
                    ))
                } else {
                    let mut lowered = Vec::with_capacity(items.len());
                    for item in items {
                        lowered.push(self.lower_expr(
                            item.value,
                            slots,
                            current_function,
                            item_slot,
                        )?);
                    }
                    Some(push_build_row!(self, expr, BuildExprRow::List(lowered)))
                }
            }
            ArenaExprKind::ListComp {
                expr: body,
                qualifiers,
            } => {
                let saved = slots.enter();
                let qualifiers =
                    self.lower_comp_qualifiers(qualifiers, slots, current_function, item_slot)?;
                let value = self.lower_expr(body, slots, current_function, item_slot)?;
                slots.exit(saved);
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ListComp {
                        value,
                        qualifiers,
                        span
                    }
                ))
            }
            ArenaExprKind::MapComp {
                key,
                value,
                qualifiers,
            } => {
                let saved = slots.enter();
                let qualifiers =
                    self.lower_comp_qualifiers(qualifiers, slots, current_function, item_slot)?;
                let key_span = self.program.arena.expr(key).span;
                let key = self.lower_expr(key, slots, current_function, item_slot)?;
                let key = if matches!(self.bodies.expr_types.get(&id), Some(Type::Map(key_ty, _)) if **key_ty == Type::UInt)
                {
                    self.require_uint_key(key, key_span)
                } else {
                    key
                };
                let value = self.lower_expr(value, slots, current_function, item_slot)?;
                slots.exit(saved);
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::MapComp {
                        key,
                        value,
                        qualifiers,
                        span
                    }
                ))
            }
            ArenaExprKind::StructuredPipeline { input, stages } => {
                let stages = self.program.arena.stream_stages(stages).to_vec();
                let mut lowered_stages = Vec::with_capacity(stages.len());
                for stage in &stages {
                    let input_ty = self
                        .declarations
                        .stream_stage_types
                        .get(&(self.current_namespace, self.program.arena.span(stage.span)))?
                        .input
                        .clone();
                    lowered_stages.push(self.lower_pipeline_stage(
                        stage,
                        slots,
                        current_function,
                        stream_item_type(&input_ty),
                    )?);
                }
                self.fuse_par_map_flat_map_reduce_by(&mut lowered_stages);
                let lowered_input = self.lower_expr(input, slots, current_function, item_slot)?;
                let lowered_input =
                    if matches!(self.bodies.expr_types.get(&input), Some(Type::Set(_))) {
                        let input_span = self.program.arena.expr(input).span;
                        self.set_as_list(lowered_input, input_span)
                    } else {
                        lowered_input
                    };
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ListPipeline {
                        input: lowered_input,
                        stages: lowered_stages,
                        span,
                    }
                ))
            }
            // Each conversion is an operation the language already has,
            // under `?`. The checker chose which; an expression it published
            // no conversion for was rejected and is not lowered.
            ArenaExprKind::Convert { value, target } => {
                let conversion = *self.bodies.conversions.get(&id)?;
                let value = self.lower_expr(value, slots, current_function, item_slot)?;
                let method = |name: &str| BuildExprRow::Method {
                    receiver: value,
                    name: Name::intern(name).as_str(),
                    args: Vec::new(),
                    span,
                };
                let path_from_bytes = |bytes| BuildExprRow::ModuleCall {
                    cli_plan: None,
                    op: RuntimeOp::PathParseBytes,
                    args: vec![Some(bytes)],
                    span,
                };
                let operation = match conversion {
                    Conversion::TextToInt => method("parse_int"),
                    Conversion::TextToUInt => method("parse_uint"),
                    Conversion::TextToFloat => method("parse_float"),
                    Conversion::BytesToText => method("utf8"),
                    Conversion::BytesToPath => path_from_bytes(value),
                    Conversion::TextToPath => path_from_bytes(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ModuleCall {
                            cli_plan: None,
                            op: RuntimeOp::BytesFromText,
                            args: vec![Some(value)],
                            span,
                        }
                    )),
                    Conversion::IntToUInt => BuildExprRow::Require {
                        value,
                        check: LoweredTypeCheck {
                            schema: Some(self.prepared_schema(Type::UInt)),
                            ty: Type::UInt,
                            name: Arc::from("UInt"),
                        },
                        span,
                    },
                    Conversion::IntToBounded => {
                        let ty = compact_runtime_type_in_namespace(
                            &self.program.arena,
                            target,
                            self.declarations,
                            self.current_namespace,
                        );
                        BuildExprRow::Require {
                            value,
                            check: LoweredTypeCheck {
                                schema: Some(self.prepared_schema(ty.clone())),
                                ty,
                                name: compact_type_expr_name(&self.program.arena, target),
                            },
                            span,
                        }
                    }
                };
                let operation = push_build_row!(self, expr, operation);
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Try {
                        value: operation,
                        span
                    }
                ))
            }
            ArenaExprKind::Require { value, schema } => {
                let (ty, name) = if let Some(schema) = schema {
                    lowered_arena_type(&self.program.arena, schema, self.declarations)?;
                    (
                        compact_runtime_type_in_namespace(
                            &self.program.arena,
                            schema,
                            self.declarations,
                            self.current_namespace,
                        ),
                        compact_type_expr_name(&self.program.arena, schema),
                    )
                } else {
                    let target = self.bodies.requirement_targets.get(&id)?;
                    (target.ty.clone(), target.name.clone())
                };
                let prepared = self.prepared_schema(ty.clone());
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Require {
                        value: self.lower_expr(value, slots, current_function, item_slot)?,
                        check: LoweredTypeCheck {
                            schema: Some(prepared),
                            ty,
                            name
                        },
                        span,
                    }
                ))
            }
            // `0 - x` is only right for Int: a Float operand took the Int
            // fast path inside loops ("lowered expression expected Int") and
            // `0.0 - 0.0` loses the sign of `-0.0`, so Floats scale by -1.0.
            ArenaExprKind::Unary {
                op: UnaryOp::Neg,
                expr,
            } if self.checked_expr_type(expr) == Some(Type::Float) => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Binary {
                    op: BinaryOp::Mul,
                    left: self.lower_expr(expr, slots, current_function, item_slot)?,
                    right: push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Float(crate::runtime::value::FloatValue::new(-1.0))
                    ),
                    span,
                }
            )),
            ArenaExprKind::Unary {
                op: UnaryOp::Neg,
                expr,
            } => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Binary {
                    op: BinaryOp::Sub,
                    left: push_build_row!(self, expr, BuildExprRow::Int(0)),
                    right: self.lower_expr(expr, slots, current_function, item_slot)?,
                    span,
                }
            )),
            ArenaExprKind::Unary {
                op: UnaryOp::Not,
                expr,
            } => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::IfExpr {
                    branches: vec![(
                        self.lower_expr(expr, slots, current_function, item_slot)?,
                        push_build_row!(self, expr, BuildExprRow::Bool(false)),
                    )],
                    else_value: push_build_row!(self, expr, BuildExprRow::Bool(true)),
                    span,
                }
            )),
            ArenaExprKind::ComparisonChain(pairs) => {
                let pairs = self.program.arena.expr_ids(pairs).collect::<Vec<_>>();
                let pairs = pairs
                    .into_iter()
                    .map(|pair| self.lower_expr(pair, slots, current_function, item_slot))
                    .collect::<Option<Vec<_>>>()?;
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ComparisonChain {
                        pairs,
                        assertion: false
                    }
                ))
            }
            ArenaExprKind::Binary { op, left, right } if lowered_binary_op(op) => {
                let uint_key = matches!(op, BinaryOp::In | BinaryOp::NotIn)
                    && matches!(self.checked_expr_type(right), Some(Type::Map(key, _)) if *key == Type::UInt);
                let lowered_left = self.lower_expr(left, slots, current_function, item_slot)?;
                let lowered_left = if uint_key {
                    self.require_uint_key(lowered_left, self.program.arena.expr(left).span)
                } else {
                    lowered_left
                };
                // Only the operands' checked types tell `s + s < s + s` on Str
                // from Int arithmetic; the int fast path compared Str slots as
                // Ints at runtime.
                let non_int = |operand: ExprId| {
                    self.bodies
                        .expr_types
                        .get(&operand)
                        .or(self.checked_expr_type(operand).as_ref())
                        .is_some_and(|ty| !matches!(ty, Type::Int | Type::UInt))
                };
                let non_int_operands = non_int(left)
                    || non_int(right)
                    || self.checked_expr_type(left) == Some(Type::Duration)
                    || self.checked_expr_type(right) == Some(Type::Duration);
                let value = push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Binary {
                        op,
                        left: lowered_left,
                        right: self.lower_expr(right, slots, current_function, item_slot)?,
                        span,
                    }
                );
                if non_int_operands {
                    self.scratch
                        .borrow_mut()
                        .non_int_binary_expressions
                        .insert(value.index());
                }
                let ty = if op == BinaryOp::Add {
                    self.checked_expr_type(id)
                } else {
                    None
                };
                Some(match ty {
                    Some(ty) => self.checked_unsigned_value(value, &ty, span),
                    None => value,
                })
            }
            ArenaExprKind::Binary {
                op: BinaryOp::ResultFallback,
                left,
                right,
            } => {
                if self
                    .bodies
                    .proven_nonnull_fallback_receivers
                    .contains(&left)
                {
                    return self.lower_expr(left, slots, current_function, item_slot);
                }
                if let ArenaExprKind::ValueBlock(block) = self.program.arena.expr(right).kind {
                    // A handler without `|name|` reads the error as its item `.`.
                    let parameter = match self
                        .program
                        .arena
                        .block_params(self.program.arena.block(block).params)
                    {
                        [] => None,
                        [parameter] => Some(parameter.name),
                        _ => return None,
                    };
                    let result_ty = self
                        .bodies
                        .expr_types
                        .get(&left)
                        .filter(|ty| compact_checked_type_is_concrete(ty))
                        .cloned()
                        .or_else(|| self.checked_expr_type(left))?;
                    let Type::Result(_, error_ty) = result_ty else {
                        return None;
                    };
                    let left = self.lower_expr(left, slots, current_function, item_slot)?;
                    let saved = slots.enter();
                    let (error_slot, handler_item_slot) = match parameter {
                        Some(parameter) => (
                            slots.declare_with_type(parameter, Some(*error_ty)),
                            item_slot,
                        ),
                        None => {
                            let slot = slots.reserve("fallback.error");
                            (slot, Some(slot))
                        }
                    };
                    let handler = self.lower_block_value_expr(
                        block,
                        slots,
                        current_function,
                        handler_item_slot,
                    );
                    slots.exit(saved);
                    let handler = handler?;
                    let success_slot = slots.reserve("fallback.success");
                    let success = push_build_row!(self, expr, BuildExprRow::Param(success_slot));
                    let error_pattern = push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::ResultErr {
                            slot: Some(error_slot),
                            unit_only: false
                        }
                    );
                    let success_pattern = push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::ResultOk {
                            slot: Some(success_slot),
                            unit_only: false
                        }
                    );
                    let value = push_build_row!(
                        self,
                        expr,
                        BuildExprRow::MatchExpr {
                            value: left,
                            arms: vec![
                                (success_pattern, None, success),
                                (error_pattern, None, handler)
                            ],
                            span,
                        }
                    );
                    return Some(match checked_creation {
                        Some(ty) => self.checked_unsigned_value(value, &ty, span),
                        None => value,
                    });
                }
                let left_ty = self
                    .checked_expr_type(left)
                    .or_else(|| {
                        self.bodies
                            .expr_types
                            .get(&left)
                            .filter(|ty| compact_checked_type_is_concrete(ty))
                            .cloned()
                    })
                    .or_else(|| self.concrete_checked_type(left));
                let left = self.lower_expr(left, slots, current_function, item_slot)?;
                let right = self.lower_expr(right, slots, current_function, item_slot)?;
                if matches!(left_ty, Some(Type::Optional(_))) {
                    let slot = slots.reserve("optional fallback");
                    let null_pattern = push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Literal(LoweredValue::Null)
                    );
                    let present_pattern =
                        push_build_row!(self, pattern, BuildPatternRow::Bind { slot });
                    let present = push_build_row!(self, expr, BuildExprRow::Param(slot));
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::MatchExpr {
                            value: left,
                            arms: vec![
                                (null_pattern, None, right),
                                (present_pattern, None, present)
                            ],
                            span,
                        }
                    ))
                } else {
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ResultFallback { left, right }
                    ))
                }
            }
            ArenaExprKind::If {
                branches,
                else_value,
            } => {
                let branches = self.program.arena.if_expr_branches(branches).to_vec();
                let has_pattern = branches.iter().any(|branch| {
                    matches!(
                        self.program.arena.expr(branch.condition).kind,
                        ArenaExprKind::PatternCondition { .. }
                    )
                });
                let mut lowered = Vec::with_capacity(branches.len());
                for branch in branches {
                    let saved = slots.enter();
                    let condition = self.lower_pattern_condition_parts(
                        branch.condition,
                        slots,
                        current_function,
                        item_slot,
                    );
                    let value = self.lower_expr(branch.value, slots, current_function, item_slot);
                    slots.exit(saved);
                    let (condition, captures) = condition?;
                    lowered.push((condition, value?, captures));
                }
                let else_value = self.lower_expr(else_value, slots, current_function, item_slot)?;
                if has_pattern {
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PatternIf {
                            branches: lowered,
                            else_value,
                            span
                        }
                    ))
                } else {
                    let branches = lowered
                        .into_iter()
                        .map(|(condition, value, _)| (condition, value))
                        .collect();
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::IfExpr {
                            branches,
                            else_value,
                            span
                        }
                    ))
                }
            }
            ArenaExprKind::PatternCondition { .. } => self
                .lower_pattern_condition_parts(id, slots, current_function, item_slot)
                .map(|(condition, _)| condition),
            ArenaExprKind::Match { value, arms } | ArenaExprKind::PatternTest { value, arms } => {
                if let Some(expr) =
                    self.lower_str_match_expr(value, arms, span, slots, current_function, item_slot)
                {
                    return Some(match checked_creation {
                        Some(ty) => self.checked_unsigned_value(expr, &ty, span),
                        None => expr,
                    });
                }
                if let Some(expr) =
                    self.lower_tag_match_expr(value, arms, span, slots, current_function, item_slot)
                {
                    return Some(match checked_creation {
                        Some(ty) => self.checked_unsigned_value(expr, &ty, span),
                        None => expr,
                    });
                }
                let arms = self.program.arena.match_expr_arms(arms).to_vec();
                let (ok_binding_ty, err_binding_ty) =
                    self.compact_match_scrutinee_result_types(value);
                let mut lowered_arms = Vec::with_capacity(arms.len());
                for arm in arms {
                    let (pattern, cleanup) = self.lower_pattern(
                        arm.pattern,
                        slots,
                        ok_binding_ty.as_ref(),
                        err_binding_ty.as_ref(),
                    )?;
                    let value = self
                        .lower_expr(arm.value, slots, current_function, item_slot)
                        .unwrap_or(push_build_row!(self, expr, BuildExprRow::Unit));
                    let guard = match arm.guard {
                        Some(guard_expr) => {
                            Some(self.lower_expr(guard_expr, slots, current_function, item_slot)?)
                        }
                        None => None,
                    };
                    cleanup_lowered_pattern_slots(slots, cleanup);
                    lowered_arms.push((pattern, guard, value));
                }
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::MatchExpr {
                        value: self.lower_expr(value, slots, current_function, item_slot)?,
                        arms: lowered_arms,
                        span,
                    }
                ))
            }
            ArenaExprKind::Field { base, name } => {
                if let Some(env_expr) = self.lower_env_field(base, name, span) {
                    return Some(env_expr);
                }
                if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind
                    && self.compact_qualified_tag_variant_arity(module, name) == Some(0)
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Tag {
                            type_name: self
                                .compact_tag_type_name(Name::intern(format!("{module}.{name}")))?,
                            wire: self.compact_tag_wire(Name::intern(format!("{module}.{name}"))),
                            name: Arc::<str>::from(name.as_str().as_str()),
                            fields: Default::default(),
                        }
                    ));
                }
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Field {
                        base: self.lower_expr(base, slots, current_function, item_slot)?,
                        name: name.as_str(),
                        span,
                    }
                ))
            }
            ArenaExprKind::NullSafeField { base, name } => {
                let base = self.lower_postfix_receiver(base, slots, current_function, item_slot)?;
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Field {
                        base,
                        name: name.as_str(),
                        span
                    }
                ))
            }
            ArenaExprKind::Index {
                base,
                index,
                guarded,
            } => {
                let uint_key = matches!(self.checked_expr_type(base), Some(Type::Map(key, _)) if *key == Type::UInt);
                let lowered_base = if guarded {
                    self.lower_postfix_receiver(base, slots, current_function, item_slot)?
                } else {
                    self.lower_expr(base, slots, current_function, item_slot)?
                };
                // The checker decided that this index counts from the end;
                // its literal is not evaluated.
                if let Some(distance) = self
                    .bodies
                    .from_end_indexes
                    .get(&id)
                    .copied()
                    .and_then(super::EndDistance::new)
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::IndexFromEnd {
                            base: lowered_base,
                            distance,
                            span
                        }
                    ));
                }
                let lowered_index = self.lower_expr(index, slots, current_function, item_slot)?;
                let lowered_index = if uint_key {
                    self.require_uint_key(lowered_index, self.program.arena.expr(index).span)
                } else {
                    lowered_index
                };
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Index {
                        base: lowered_base,
                        index: lowered_index,
                        span
                    }
                ))
            }
            ArenaExprKind::Slice {
                base,
                start,
                end,
                guarded,
            } => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Slice {
                    base: if guarded {
                        self.lower_postfix_receiver(base, slots, current_function, item_slot)?
                    } else {
                        self.lower_expr(base, slots, current_function, item_slot)?
                    },
                    start: match start {
                        Some(start) =>
                            Some(self.lower_expr(start, slots, current_function, item_slot,)?),
                        None => None,
                    },
                    end: match end {
                        Some(end) =>
                            Some(self.lower_expr(end, slots, current_function, item_slot,)?),
                        None => None,
                    },
                    span,
                }
            )),
            ArenaExprKind::Try(expr) => {
                if let ArenaExprKind::Call { args, .. } = self.program.arena.expr(expr).kind
                    && self.program.arena.call_args(args).iter().any(|arg| {
                        matches!(
                            arg.kind,
                            ArenaCallArgKind::Named { .. } | ArenaCallArgKind::NamedSpread { .. }
                        )
                    })
                {
                    let value = self.lower_expr(expr, slots, current_function, item_slot)?;
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Try {
                            value,
                            span: self.propagation_span(id, expr, slots)
                        }
                    ));
                }
                let expr_span = self.program.arena.expr(expr).span;
                if let ArenaExprKind::Call { callee, args } = self.program.arena.expr(expr).kind {
                    let args_vec = self.program.arena.call_args(args).to_vec();
                    if let ArenaExprKind::Field { base, name } =
                        self.program.arena.expr(callee).kind
                        && let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind
                    {
                        if module == "fs" && name == "files" {
                            let options =
                                lower_fs_files_args(&self.checked_api_arguments(expr, &args_vec)?)?;
                            return Some(push_build_row!(
                                self,
                                expr,
                                BuildExprRow::Try {
                                    value: push_build_row!(
                                        self,
                                        expr,
                                        BuildExprRow::FsFiles {
                                            root: self.lower_expr(
                                                options.root,
                                                slots,
                                                current_function,
                                                item_slot,
                                            )?,
                                            gitignore: self.lower_optional_expr(
                                                options.gitignore,
                                                slots,
                                                current_function,
                                                item_slot,
                                            )?,
                                            stat: self.lower_optional_expr(
                                                options.stat,
                                                slots,
                                                current_function,
                                                item_slot,
                                            )?,
                                            hidden: self.lower_optional_expr(
                                                options.hidden,
                                                slots,
                                                current_function,
                                                item_slot,
                                            )?,
                                            exts: match options.exts {
                                                Some(exts) => Some(self.lower_expr(
                                                    exts,
                                                    slots,
                                                    current_function,
                                                    item_slot,
                                                )?),
                                                None => None,
                                            },
                                            result_wrapped: true,
                                            span: expr_span,
                                        }
                                    ),
                                    span: self.propagation_span(id, expr, slots)
                                }
                            ));
                        }
                        if module == "fs" && name == "walk" {
                            let options =
                                lower_fs_files_args(&self.checked_api_arguments(expr, &args_vec)?)?;
                            return Some(push_build_row!(
                                self,
                                expr,
                                BuildExprRow::Try {
                                    value: push_build_row!(
                                        self,
                                        expr,
                                        BuildExprRow::FsWalk {
                                            root: self.lower_expr(
                                                options.root,
                                                slots,
                                                current_function,
                                                item_slot,
                                            )?,
                                            gitignore: self.lower_optional_expr(
                                                options.gitignore,
                                                slots,
                                                current_function,
                                                item_slot,
                                            )?,
                                            stat: self.lower_optional_expr(
                                                options.stat,
                                                slots,
                                                current_function,
                                                item_slot,
                                            )?,
                                            hidden: self.lower_optional_expr(
                                                options.hidden,
                                                slots,
                                                current_function,
                                                item_slot,
                                            )?,
                                            exts: match options.exts {
                                                Some(exts) => Some(self.lower_expr(
                                                    exts,
                                                    slots,
                                                    current_function,
                                                    item_slot,
                                                )?),
                                                None => None,
                                            },
                                            result_wrapped: true,
                                            span: expr_span,
                                        }
                                    ),
                                    span: self.propagation_span(id, expr, slots)
                                }
                            ));
                        }
                    }
                }
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Try {
                        value: self.lower_expr(expr, slots, current_function, item_slot)?,
                        span: self.propagation_span(id, expr, slots)
                    }
                ))
            }
            ArenaExprKind::Call { callee, args } => {
                self.lower_call(id, callee, args, slots, current_function, item_slot)
            }
            ArenaExprKind::BuilderCall { call, block } => self.lower_process_command_builder(
                call,
                block,
                slots,
                current_function,
                item_slot,
                span,
            ),
            ArenaExprKind::Capture(block) => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Capture {
                    body: self.lower_retry_block(block, slots, current_function, item_slot)?,
                    span,
                }
            )),
            ArenaExprKind::ValuePipelineCall { input, call, hole } => {
                let input = self.lower_expr(input, slots, current_function, item_slot)?;
                let slot = slots.reserve("value pipeline input");
                let bound = push_build_row!(self, expr, BuildExprRow::Param(slot));
                let previous = slots.postfix_receivers.insert(hole, bound);
                let selected = self.lower_expr(call, slots, current_function, item_slot);
                match previous {
                    Some(previous) => {
                        slots.postfix_receivers.insert(hole, previous);
                    }
                    None => {
                        slots.postfix_receivers.remove(&hole);
                    }
                }
                let selected = selected?;
                let pattern = push_build_row!(self, pattern, BuildPatternRow::Bind { slot });
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::MatchExpr {
                        value: input,
                        arms: vec![(pattern, None, selected)],
                        span
                    }
                ))
            }
            ArenaExprKind::ErrorContext { message, block } => {
                let message = self.lower_expr(message, slots, current_function, item_slot)?;
                let tail = self
                    .program
                    .arena
                    .stmt_ids(self.program.arena.block(block).statements)
                    .last();
                let statement_body = tail.is_some_and(|tail| {
                    self.bodies.statement_positions.get(&tail)
                        == Some(&crate::sema::check::StatementPosition::Statement)
                });
                let body = if statement_body {
                    self.lower_block(block, slots, current_function, item_slot)?
                } else {
                    self.lower_retry_block(block, slots, current_function, item_slot)?
                };
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ErrorContext {
                        message,
                        body,
                        span
                    }
                ))
            }
            ArenaExprKind::ContextScope {
                kind, input, block, ..
            } => {
                let input = self.lower_expr(input, slots, current_function, item_slot)?;
                let body = self.lower_retry_block(block, slots, current_function, item_slot)?;
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ContextScope {
                        kind,
                        input,
                        body,
                        span
                    }
                ))
            }
            ArenaExprKind::TempDirScope { path, block, .. } => {
                self.lower_tempdir_scope(path, block, span, slots, current_function, item_slot)
            }
            ArenaExprKind::ResourceScope {
                bindings, block, ..
            } => self.lower_resource_scope(
                id,
                bindings,
                block,
                span,
                slots,
                current_function,
                item_slot,
            ),
            ArenaExprKind::ValueBlock(block) => {
                self.lower_block_value_expr(block, slots, current_function, item_slot)
            }
            ArenaExprKind::Loop { block } => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Loop {
                    body: self.lower_block(block, slots, current_function, item_slot)?,
                    span,
                }
            )),
            // A block that binds an empty list to a local of its own, runs
            // the body as an inner block, and has the local as its value. The
            // body's yields append to that local (`lower_collect_yield`).
            ArenaExprKind::Collect { block } => {
                let saved = slots.enter();
                let slot = slots.declare(collect_local(id));
                let lowered = (|| {
                    let empty = push_build_row!(self, expr, BuildExprRow::List(Vec::new()));
                    let bind =
                        push_build_row!(self, stmt, BuildStmtRow::Let { slot, value: empty });
                    let body = self.lower_block(block, slots, current_function, item_slot)?;
                    let body = push_build_row!(self, expr, BuildExprRow::ValueBlock { body, span });
                    let run = push_build_row!(self, stmt, BuildStmtRow::Expr { value: body, span });
                    let list = push_build_row!(self, expr, BuildExprRow::Param(slot));
                    let value = push_build_row!(self, stmt, BuildStmtRow::Value { value: list });
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ValueBlock {
                            body: vec![bind, run, value],
                            span,
                        }
                    ))
                })();
                slots.exit(saved);
                lowered
            }
            ArenaExprKind::Retry {
                schedule,
                delays,
                pattern,
                block,
            } => {
                let delays = self.program.arena.expr_ids(delays).collect::<Vec<_>>();
                let mut lowered_delays = Vec::with_capacity(delays.len());
                for delay in delays {
                    lowered_delays.push(self.lower_expr(
                        delay,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Retry {
                        delays: lowered_delays,
                        pattern: match pattern {
                            Some(pattern) =>
                                Some(self.lower_pattern(pattern, slots, None, None)?.0),
                            None => None,
                        },
                        body: self.lower_retry_block(block, slots, current_function, item_slot)?,
                        span,
                        schedule,
                    }
                ))
            }
            // `e"NAME"` is exactly `env.get("NAME")`.
            ArenaExprKind::EnvString(name) => {
                let name = push_build_row!(self, expr, BuildExprRow::Str(name.to_string().into()));
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ModuleCall {
                        cli_plan: None,
                        op: RuntimeOp::EnvGet,
                        args: vec![Some(name)],
                        span,
                    }
                ))
            }
            ArenaExprKind::EnvPathList => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::ModuleCall {
                    cli_plan: None,
                    op: RuntimeOp::EnvPathList,
                    args: Vec::new(),
                    span,
                }
            )),
            _ => None,
        };
        match lowered {
            Some(lowered) => {
                self.output.constructed_expressions += 1;
                Some(match checked_creation {
                    Some(ty) => self.checked_unsigned_value(lowered, &ty, span),
                    None => lowered,
                })
            }
            None => {
                let kind = self.program.arena.expr(id).kind;
                let blocker_label = compact_expr_kind_label(kind.clone());
                let blocker_index = compact_expr_kind_index(kind);
                let mut blocker_detail = (
                    self.program.arena.expr(id).span,
                    format!("expression `{blocker_label}`"),
                );
                if blocker_index == 22
                    && let ArenaExprKind::Call { callee, .. } = self.program.arena.expr(id).kind
                {
                    self.output.call_blockers[compact_call_blocker_index(self.program, callee)] +=
                        1;
                    record_compact_call_blocker_label(
                        &mut self.output.call_blocker_callees,
                        self.program,
                        callee,
                    );
                    record_compact_call_blocker_span(
                        &mut self.output.call_blocker_sample_spans,
                        self.program,
                        callee,
                    );
                    if let Some(label) = compact_call_blocker_label(self.program, callee) {
                        blocker_detail = (
                            self.program.arena.expr(callee).span,
                            format!("call `{label}`"),
                        );
                    }
                }
                self.last_blocker_detail = Some(blocker_detail);
                self.output.expression_blockers[blocker_index] += 1;
                self.output.constructed_expressions += 1;
                self.output.blocker_events += 1;
                Some(push_build_row!(self, expr, BuildExprRow::Unit))
            }
        }
    }

}

fn construct_top_level_stmt_is_skippable(program: &ArenaProgram, id: StmtId) -> bool {
    if construct_is_main_at_args_call(program, id) {
        return true;
    }
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Export(inner)
        | ArenaStmtKind::Sugar {
            expansion: inner, ..
        } => construct_top_level_stmt_is_skippable(program, inner),
        ArenaStmtKind::Use(use_id) => construct_use_stmt_is_skippable(program, use_id),
        ArenaStmtKind::Expr(expr) if construct_expr_is_reveal_type_call(program, expr) => true,
        ArenaStmtKind::TypeDef(_)
        | ArenaStmtKind::ErrorDef(_)
        | ArenaStmtKind::ProcDef(_)
        | ArenaStmtKind::CliMain(_)
        | ArenaStmtKind::PureDef(_)
        | ArenaStmtKind::StreamDef(_) => true,
        _ => false,
    }
}

fn construct_use_stmt_is_skippable(
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
    path.next().is_none() && api_spec().is_standard_module(&name.as_str())
}

fn construct_expr_is_reveal_type_call(program: &ArenaProgram, expr: ExprId) -> bool {
    let ArenaExprKind::Call { callee, .. } = program.arena.expr(expr).kind else {
        return false;
    };
    matches!(program.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "reveal_type")
}

fn construct_is_main_at_args_call(program: &ArenaProgram, id: StmtId) -> bool {
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Expr(expr) => construct_is_main_at_args_expr(program, expr),
        _ => false,
    }
}

fn construct_is_main_at_args_expr(program: &ArenaProgram, id: ExprId) -> bool {
    match program.arena.expr(id).kind {
        ArenaExprKind::Try(inner) => construct_is_main_at_args_expr(program, inner),
        ArenaExprKind::Call { callee, args } => {
            matches!(program.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == Name::intern("main"))
                && matches!(
                    program.arena.call_args(args),
                    [arg] if construct_is_args_call_arg(program, arg)
                )
        }
        _ => false,
    }
}

fn construct_is_args_call_arg(program: &ArenaProgram, arg: &ArenaCallArg) -> bool {
    let ArenaCallArgKind::Splice { value, .. } = arg.kind else {
        return false;
    };
    matches!(program.arena.expr(value).kind, ArenaExprKind::Ident(name) if name == Name::intern("args"))
}

fn is_env_module_expr(
    arena: &crate::syntax::arena::AstArena,
    expr: crate::syntax::arena::ExprId,
) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::Ident(name) => name == "env",
        ArenaExprKind::Field { base, .. } => is_env_module_expr(arena, base),
        _ => false,
    }
}

fn top_level_known_with_runtime_bindings() -> FxHashMap<Name, LoweredTopLevelBinding> {
    let mut known = FxHashMap::default();
    let args = LoweredTopLevelBinding {
        kind: LoweredType::List,
        checked: None,
        mutable: false,
        slot: true,
    };
    known.insert(Name::intern("args"), args);
    known
}

pub(super) fn top_level_slots(known: &FxHashMap<Name, LoweredTopLevelBinding>) -> SlotScope {
    let mut names = known
        .iter()
        .filter_map(|(name, binding)| binding.slot.then_some(*name))
        .collect::<Vec<_>>();
    names.sort_unstable();
    let mut slots = SlotScope::from_names(names.iter().copied());
    for name in names {
        let Some(binding) = known.get(&name) else {
            continue;
        };
        if let Some(ty) = binding
            .checked
            .clone()
            .or_else(|| type_for_lowered_type(binding.kind))
        {
            slots.types.insert(name, ty);
        }
    }
    slots
}

pub(super) fn lowered_top_level(
    scratch: &Rc<RefCell<BuildScratch>>,
    kind: BuildTopKind,
    known: &FxHashMap<Name, LoweredTopLevelBinding>,
    slot_indexes: SlotScope,
) -> BuildTopStmtId {
    let slot_count = slot_indexes.count();
    let mut slots: LoweredTopLevelSlots = slot_indexes
        .into_entries()
        .filter_map(|(name, slot)| {
            let binding = known.get(&name)?;
            if !binding.slot {
                return None;
            }
            Some(LoweredTopLevelSlot {
                name,
                slot,
                kind: binding.kind,
                mutable: binding.mutable,
            })
        })
        .collect();
    slots.sort_unstable_by_key(|slot| slot.slot);
    scratch.borrow_mut().top_stmt(BuildTopStmtRow {
        kind,
        slots,
        slot_count,
    })
}

/// Recursively check whether a lowered statement body contains any `Defer`
/// statements (including those nested inside `If` branches, `Retry` bodies, etc.).
pub(super) fn lowered_body_has_defers(scratch: &BuildScratch, statements: &[BuildStmtId]) -> bool {
    fn stmt_has_defers(scratch: &BuildScratch, stmt: &BuildStmtId) -> bool {
        match &scratch.statements[stmt.index()] {
            BuildStmtRow::Defer { .. } => true,
            BuildStmtRow::If {
                branches,
                else_body,
            } => {
                branches
                    .iter()
                    .any(|(_, body)| lowered_body_has_defers(scratch, body))
                    || else_body
                        .as_ref()
                        .is_some_and(|b| lowered_body_has_defers(scratch, b))
            }
            BuildStmtRow::IfBool {
                branches,
                else_body,
            } => {
                branches
                    .iter()
                    .any(|(_, body)| lowered_body_has_defers(scratch, body))
                    || else_body
                        .as_ref()
                        .is_some_and(|b| lowered_body_has_defers(scratch, b))
            }
            BuildStmtRow::While { body, .. } | BuildStmtRow::WhileBool { body, .. } => {
                lowered_body_has_defers(scratch, body)
            }
            BuildStmtRow::For { body, .. }
            | BuildStmtRow::ForRecord { body, .. }
            | BuildStmtRow::ForStrLines { body, .. } => lowered_body_has_defers(scratch, body),
            BuildStmtRow::Match { arms, .. } => arms
                .iter()
                .any(|(_, _, body)| lowered_body_has_defers(scratch, body)),
            BuildStmtRow::StrMatch { arms, fallback, .. } => {
                arms.values()
                    .any(|body| lowered_body_has_defers(scratch, body))
                    || fallback
                        .as_ref()
                        .is_some_and(|b| lowered_body_has_defers(scratch, b))
            }
            BuildStmtRow::TagMatch { arms, fallback, .. } => {
                arms.values()
                    .any(|body| lowered_body_has_defers(scratch, body))
                    || fallback
                        .as_ref()
                        .is_some_and(|b| lowered_body_has_defers(scratch, b))
            }
            BuildStmtRow::Guard { else_body, .. } => lowered_body_has_defers(scratch, else_body),
            BuildStmtRow::With {
                body, else_body, ..
            } => {
                lowered_body_has_defers(scratch, body)
                    || lowered_body_has_defers(scratch, else_body)
            }
            BuildStmtRow::Cd { body, .. } | BuildStmtRow::Env { body, .. } => {
                lowered_body_has_defers(scratch, body)
            }
            _ => false,
        }
    }
    statements.iter().any(|stmt| stmt_has_defers(scratch, stmt))
}

/// without an `else` (or a non-exhaustive `match`) can fall through. So a body
/// "can return" only when every reachable path provably ends in a `Return`.
pub(super) fn lowered_body_can_return(scratch: &BuildScratch, statements: &[BuildStmtId]) -> bool {
    statements
        .iter()
        .any(|stmt| match &scratch.statements[stmt.index()] {
            BuildStmtRow::Return { .. } => true,
            BuildStmtRow::Defer { .. } => false,
            BuildStmtRow::Yield { .. } | BuildStmtRow::YieldDelegate { .. } => false,
            BuildStmtRow::ScanLines { .. } => false,
            BuildStmtRow::ScanBytes { .. } => false,
            BuildStmtRow::Break | BuildStmtRow::BreakValue { .. } => false,
            BuildStmtRow::Continue => false,
            BuildStmtRow::If {
                branches,
                else_body,
            } => lowered_branch_chain_can_return(
                scratch,
                branches.iter().map(|(condition, body)| {
                    (
                        lowered_expr_bool_literal(scratch, condition),
                        body.as_slice(),
                    )
                }),
                else_body.as_deref(),
            ),
            BuildStmtRow::PatternIf {
                branches,
                else_body,
                ..
            } => lowered_branch_chain_can_return(
                scratch,
                branches.iter().map(|(condition, body, _)| {
                    (
                        lowered_expr_bool_literal(scratch, condition),
                        body.as_slice(),
                    )
                }),
                else_body.as_deref(),
            ),
            BuildStmtRow::IfBool {
                branches,
                else_body,
            } => lowered_branch_chain_can_return(
                scratch,
                branches.iter().map(|(condition, body)| {
                    (
                        lowered_bool_row_literal(scratch, condition),
                        body.as_slice(),
                    )
                }),
                else_body.as_deref(),
            ),
            BuildStmtRow::While { body, .. }
            | BuildStmtRow::PatternWhile { body, .. }
            | BuildStmtRow::WhileBool { body, .. }
            | BuildStmtRow::For { body, .. }
            | BuildStmtRow::ForRecord { body, .. }
            | BuildStmtRow::ForStrLines { body, .. } => {
                let _ = body;
                false
            }
            BuildStmtRow::Cd { body, .. } | BuildStmtRow::Env { body, .. } => {
                lowered_body_can_return(scratch, body)
            }
            BuildStmtRow::Match { arms, .. } => lowered_match_body_can_return(scratch, arms),
            BuildStmtRow::StrMatch { arms, fallback, .. } => {
                !arms.is_empty()
                    && arms
                        .values()
                        .all(|body| lowered_body_can_return(scratch, body))
                    && fallback
                        .as_ref()
                        .is_some_and(|body| lowered_body_can_return(scratch, body))
            }
            BuildStmtRow::TagMatch { arms, fallback, .. } => {
                !arms.is_empty()
                    && arms
                        .values()
                        .all(|body| lowered_body_can_return(scratch, body))
                    && fallback
                        .as_ref()
                        .is_some_and(|body| lowered_body_can_return(scratch, body))
            }
            // A guard's success path falls through to later statements, so the
            // guard alone never guarantees a return.
            BuildStmtRow::Guard { .. } => false,
            BuildStmtRow::With {
                body, else_body, ..
            } => {
                lowered_body_can_return(scratch, body)
                    && lowered_body_can_return(scratch, else_body)
            }
            BuildStmtRow::DefaultParameter { .. }
            | BuildStmtRow::Let { .. }
            | BuildStmtRow::LetRecord { .. }
            | BuildStmtRow::LetInt { .. }
            | BuildStmtRow::LetBool { .. }
            | BuildStmtRow::Assign { .. }
            | BuildStmtRow::AssignInt { .. }
            | BuildStmtRow::AssignField { .. }
            | BuildStmtRow::AssignFieldInt { .. }
            | BuildStmtRow::AssignPath { .. }
            | BuildStmtRow::AssignBool { .. }
            | BuildStmtRow::Value { .. }
            | BuildStmtRow::Assert { .. }
            | BuildStmtRow::Expr { .. }
            | BuildStmtRow::Run { .. }
            | BuildStmtRow::Print { .. }
            | BuildStmtRow::Proc { .. }
            | BuildStmtRow::Loop { .. } => false,
        })
}

fn lowered_return_kind_accepts_unit_fallthrough(kind: LoweredReturnKind) -> bool {
    matches!(
        kind,
        LoweredReturnKind::Plain(LoweredType::Unit)
            | LoweredReturnKind::Result(LoweredType::Unit)
            // `Any` (and the erased optionals lowering maps onto it) accepts
            // a Unit fallthrough, which is how a command-tailed body
            // completes an `Any`- or `Any?`-returning function.
            | LoweredReturnKind::Plain(LoweredType::Any)
            | LoweredReturnKind::Result(LoweredType::Any)
            | LoweredReturnKind::OptionalResult
    )
}

fn lowered_expr_bool_literal(scratch: &BuildScratch, expr: &BuildExprId) -> Option<bool> {
    match &scratch.expressions[expr.index()] {
        BuildExprRow::Bool(value) => Some(*value),
        _ => None,
    }
}

fn lowered_bool_row_literal(scratch: &BuildScratch, expr: &BuildBoolId) -> Option<bool> {
    match &scratch.bools[expr.index()] {
        BuildBoolRow::Bool(value) => Some(*value),
        _ => None,
    }
}

/// Whether an `if`-style chain returns on every reachable path. A
/// literal-false condition is unreachable, and a literal-true one makes every
/// later branch and the else unreachable; a no-else chain can fall through.
fn lowered_branch_chain_can_return<'a>(
    scratch: &BuildScratch,
    branches: impl Iterator<Item = (Option<bool>, &'a [BuildStmtId])>,
    else_body: Option<&[BuildStmtId]>,
) -> bool {
    for (literal, body) in branches {
        match literal {
            Some(false) => continue,
            Some(true) => return lowered_body_can_return(scratch, body),
            None => {
                if !lowered_body_can_return(scratch, body) {
                    return false;
                }
            }
        }
    }
    else_body.is_some_and(|body| lowered_body_can_return(scratch, body))
}

pub(super) fn lowered_match_body_can_return(
    scratch: &BuildScratch,
    arms: &[(BuildPatternId, Option<BuildExprId>, Vec<BuildStmtId>)],
) -> bool {
    !arms.is_empty()
        && arms
            .iter()
            .all(|(_, _, body)| lowered_body_can_return(scratch, body))
}

pub(super) fn cleanup_lowered_pattern_slots(slots: &mut SlotScope, cleanup: Vec<(Name, usize)>) {
    for (name, slot) in cleanup {
        slots.retire(name, slot, "pattern");
    }
}

pub(super) fn lowered_error_value_has_facet(value: &Value, facet: &str) -> bool {
    match value {
        Value::Error(error) => error.facets.iter().any(|f| f == facet),
        Value::RunError(error) => error.facets().iter().any(|f| f == facet),
        _ => false,
    }
}

pub(super) fn lowered_error_variant_matches(
    family: &Name,
    variant: &Name,
    fields: &LoweredErrorPatternFields,
    value: &Value,
    slots: &mut [LoweredValue],
    bind: bool,
) -> bool {
    match value {
        Value::Error(error) => {
            crate::runtime::value::error_family_matches(error.family_name(), *family)
                && error.variant_name() == *variant
                && lowered_error_pattern_fields_match(&error.payload, fields, slots, bind)
        }
        Value::RunError(error) if *family == Name::PROCESS_ERROR => {
            if error.variant_name() != variant.as_str() {
                return false;
            }
            let payload = error.payload();
            lowered_error_pattern_fields_match(&payload, fields, slots, bind)
        }
        _ => false,
    }
}

fn lowered_error_pattern_fields_match(
    payload: &RecordMap,
    fields: &LoweredErrorPatternFields,
    slots: &mut [LoweredValue],
    bind: bool,
) -> bool {
    for (name, slot) in fields {
        let Some(value) = payload.get(&name.as_str()) else {
            return false;
        };
        if let Some(slot) = slot {
            let Some(value) = lowered_value_from_runtime_any(value) else {
                return false;
            };
            if bind {
                slots[*slot] = value;
            }
        }
    }
    true
}

pub(super) fn lowered_str_key(value: &LoweredValue) -> Option<&str> {
    match value {
        LoweredValue::Str(value) => Some(value.as_ref()),
        LoweredValue::StrView(value) => Some(value.as_str()),
        _ => None,
    }
}

pub(super) fn lowered_record_field<'a>(
    value: &'a LoweredValue,
    field: &str,
) -> Option<&'a LoweredValue> {
    match value {
        LoweredValue::Record(entries) | LoweredValue::Module(entries) => entries.get(field),
        LoweredValue::RecordVec(entries) => lowered_record_vec_get(entries, field),
        LoweredValue::Stats { .. } | LoweredValue::StatsBlob(_) => None,
        _ => None,
    }
}

/// Take a shared container's contents out of its `Arc`.
///
/// Reuses the storage when this handle is the only owner and copies otherwise,
/// so merging a value the caller has already given up does not copy it.
pub(super) fn take_shared<T: Clone>(shared: Arc<T>) -> T {
    Arc::try_unwrap(shared).unwrap_or_else(|still_shared| (*still_shared).clone())
}

pub(super) fn lowered_sum_records(mut acc: LoweredValue, val: LoweredValue) -> LoweredValue {
    match (&mut acc, val) {
        (LoweredValue::Record(acc_map), LoweredValue::Record(val_map)) => {
            let acc_map = Arc::make_mut(acc_map);
            for (key, value) in take_shared(val_map) {
                match acc_map.get_mut(&key) {
                    Some(acc_value) => {
                        *acc_value = lowered_sum_values(
                            std::mem::replace(acc_value, LoweredValue::Unit),
                            value,
                        );
                    }
                    None => {
                        acc_map.insert(key, value);
                    }
                }
            }
        }
        (LoweredValue::RecordVec(acc_map), LoweredValue::RecordVec(val_map)) => {
            let acc_map = Arc::make_mut(acc_map);
            for (key, value) in take_shared(val_map) {
                match lowered_record_vec_get_mut(acc_map, &key.as_str()) {
                    Some(acc_value) => {
                        *acc_value = lowered_sum_values(
                            std::mem::replace(acc_value, LoweredValue::Unit),
                            value,
                        );
                    }
                    None => {
                        lowered_record_vec_insert(acc_map, key, value);
                    }
                }
            }
        }
        (LoweredValue::Record(acc_map), LoweredValue::RecordVec(val_map)) => {
            let acc_map = Arc::make_mut(acc_map);
            for (key, value) in take_shared(val_map) {
                let key_text = key.as_str();
                match acc_map.get_mut::<str>(key_text.as_str()) {
                    Some(acc_value) => {
                        *acc_value = lowered_sum_values(
                            std::mem::replace(acc_value, LoweredValue::Unit),
                            value,
                        );
                    }
                    None => {
                        acc_map.insert(Arc::<str>::from(key_text.as_str()), value);
                    }
                }
            }
        }
        (LoweredValue::RecordVec(acc_map), LoweredValue::Record(val_map)) => {
            let acc_map = Arc::make_mut(acc_map);
            for (key, value) in take_shared(val_map) {
                match lowered_record_vec_get_mut(acc_map, key.as_ref()) {
                    Some(acc_value) => {
                        *acc_value = lowered_sum_values(
                            std::mem::replace(acc_value, LoweredValue::Unit),
                            value,
                        );
                    }
                    None => {
                        lowered_record_vec_insert(acc_map, Name::intern(key.as_ref()), value);
                    }
                }
            }
        }
        _ => {}
    }
    acc
}

pub(super) fn lowered_sum_values(acc: LoweredValue, val: LoweredValue) -> LoweredValue {
    match (acc, val) {
        (LoweredValue::Int(a), LoweredValue::Int(b)) => LoweredValue::Int(a + b),
        (LoweredValue::Float(a), LoweredValue::Float(b)) => {
            LoweredValue::Float(crate::runtime::value::FloatValue::new(a.0 + b.0))
        }
        (LoweredValue::List(mut acc), LoweredValue::List(value)) => {
            acc.extend(value);
            LoweredValue::List(acc)
        }
        (LoweredValue::List(mut acc), LoweredValue::SharedList(value)) => {
            acc.extend(value.iter().cloned());
            LoweredValue::List(acc)
        }
        (LoweredValue::SharedList(acc), LoweredValue::List(value)) => {
            let mut acc = (*acc).clone();
            acc.extend(value);
            LoweredValue::List(acc)
        }
        (LoweredValue::SharedList(acc), LoweredValue::SharedList(value)) => {
            let mut acc = (*acc).clone();
            acc.extend(value.iter().cloned());
            LoweredValue::List(acc)
        }
        (LoweredValue::Record(acc_map), LoweredValue::Record(val_map)) => {
            let mut acc_map = take_shared(acc_map);
            for (key, value) in take_shared(val_map) {
                match acc_map.get_mut(&key) {
                    Some(acc_value) => {
                        *acc_value = lowered_sum_values(
                            std::mem::replace(acc_value, LoweredValue::Unit),
                            value,
                        );
                    }
                    None => {
                        acc_map.insert(key, value);
                    }
                }
            }
            LoweredValue::Record(Arc::new(acc_map))
        }
        (acc @ LoweredValue::RecordVec(_), val @ LoweredValue::RecordVec(_))
        | (acc @ LoweredValue::Record(_), val @ LoweredValue::RecordVec(_))
        | (acc @ LoweredValue::RecordVec(_), val @ LoweredValue::Record(_)) => {
            lowered_sum_records(acc, val)
        }
        (acc, _) => acc,
    }
}

pub(super) fn lowered_tag_key(value: &LoweredValue) -> Option<&str> {
    match value {
        LoweredValue::Tag(tag) if tag.fields.is_empty() => Some(tag.name.as_ref()),
        _ => None,
    }
}

pub(super) fn lowered_match_no_arm(span: Span) -> RuntimeError {
    RuntimeError::new("match-no-arm", "match did not match any arm").with_span(span)
}

pub(super) fn lowered_stmt_flow_to_flow(flow: StmtFlow) -> Flow {
    match flow {
        StmtFlow::None => Flow::Continue(Value::Unit),
        StmtFlow::Value(value) | StmtFlow::Return(value) => Flow::Return(value.into_value()),
        StmtFlow::Propagate(_) => {
            unreachable!("lowered propagation must be handled with evaluator context")
        }
        StmtFlow::Break(value) => Flow::Break(value.map(LoweredValue::into_value)),
        StmtFlow::Continue => Flow::ContinueLoop,
    }
}

pub(super) fn cleanup_pipeline_stage_item_slot(
    slots: &mut SlotScope,
    cleanup: Option<Name>,
    slot: usize,
) {
    if let Some(name) = cleanup {
        slots.retire(name, slot, "pipeline.item");
    }
}

#[cfg(test)]
mod named_spread_tests {
    /// Builds `source` and returns how many whole-program copies its
    /// named-spread calls made.
    fn program_copies(source: &str) -> u64 {
        let name = "named-spread.xsh";
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            name,
            crate::loader::entry_source_from_text(name, source.to_string()),
            Vec::new(),
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let declarations = crate::sema::check::Checker::check_compact_declarations(&parsed.arena);
        assert!(
            declarations.diagnostics.is_empty(),
            "{:?}",
            declarations.diagnostics
        );
        let source_id = sources.files()[0].id();
        let before = super::SPREAD_PROGRAM_COPIES.with(std::cell::Cell::get);
        let built = super::super::FullBuilder::build_compact(
            &parsed.arena,
            &declarations,
            source,
            std::sync::Arc::new(sources),
            source_id,
        );
        assert!(built.is_ok(), "the program lowers");
        super::SPREAD_PROGRAM_COPIES.with(std::cell::Cell::get) - before
    }

    // The projections of a named-spread call are appended to a copy of the
    // program. Each call once made its own copy, which is work proportional
    // to the whole program per call; the calls of one pass now share one.
    #[test]
    fn named_spread_calls_share_one_program_copy_per_pass() {
        const CALLS: usize = 40;
        let mut source = String::from(
            "type Point = {x: Int, y: Int}\n\nproc sum(x: Int, y: Int) -> Int {\n  x + y\n}\n\n",
        );
        for index in 0..CALLS {
            source.push_str(&format!(
                "proc call{index}(point: Point) -> Int {{\n  sum(...point)\n}}\n\n"
            ));
        }
        source.push_str("let origin: Point = {x: 1, y: 2}\n");
        for index in 0..CALLS {
            source.push_str(&format!(
                "print f\"{{sum(...origin) + call{index}(origin)}}\"\n"
            ));
        }
        let copies = std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || program_copies(&source))
            .expect("spawn the lowering thread")
            .join()
            .expect("the program lowers");
        // The functions are one pass and the top-level statements another;
        // 80 calls once made 80 copies or more.
        assert!((1..=4).contains(&copies), "{copies} program copies");
    }
}

#[cfg(all(test, debug_assertions))]
mod drift_tests {
    /// Lower every repository program and the embedded standard library, and
    /// require each representation lowering derives itself to agree with the
    /// checker's published type.
    #[test]
    fn corpus_lowering_agrees_with_checked_types() {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(|| {
                let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
                let mut paths = Vec::new();
                for dir in ["core", "dev", "examples", "showcase", "stdlib", "tests"] {
                    super::super::indexed::tests::collect_xsh_paths(&root.join(dir), &mut paths);
                }
                paths.sort();
                let mut lowered = 0;
                for path in paths {
                    let name = path.to_string_lossy();
                    let source = std::fs::read_to_string(&path).expect("corpus source is readable");
                    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                        &name,
                        crate::loader::entry_source_from_text(&name, source.clone()),
                        Vec::new(),
                    );
                    let declarations =
                        crate::sema::check::Checker::check_compact_declarations(&parsed.arena);
                    if !parsed.diagnostics.is_empty() || !declarations.diagnostics.is_empty() {
                        continue;
                    }
                    let source_id = sources.files()[0].id();
                    lowered += usize::from(
                        super::super::FullBuilder::build_compact(
                            &parsed.arena,
                            &declarations,
                            &source,
                            std::sync::Arc::new(sources),
                            source_id,
                        )
                        .is_ok(),
                    );
                }
                let drift = std::mem::take(&mut *super::LOWERING_DRIFT.lock().unwrap());
                assert!(lowered > 300, "only {lowered} corpus programs lowered");
                assert!(drift.is_empty(), "{}", drift.join("\n"));
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
