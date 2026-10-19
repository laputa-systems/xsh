//! AST -> lowered-IR lowering pass, split out of `eval.rs`.

use crate::modules::{ModuleFnSig, RuntimeOp, api_spec};
use crate::runtime::value::{DurationValue, PathValue, RegexValue, RuntimeError, Value};
use crate::sema::check::{
    BindingId, BindingKind, CheckBundleView, CheckedApiCall, CheckedFunctionTarget,
    CompactBodyFacts, CompactDeclOutput, Conversion,
    ResourceTransferSite, block_callable_body_cannot_fall_through_arena,
};
use crate::sema::arguments::ArgumentValueSource;
use crate::sema::types::{CallableParamType, CallableType, ModuleExportType, Type};
use crate::source::{SourceId, SourceMap, Span};
use crate::symbol::{Name, QualifiedName, Symbol};
use crate::syntax::arena::DeferTrigger;
use crate::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaBuilderEntryKind, ArenaCallArg, ArenaChild,
    ArenaCallArgKind, ArenaCommand, ArenaCommandArgKind, ArenaExprKind, ArenaExprOrRun,
    ArenaFmtPart, ArenaPatternKind, ArenaProgram, ArenaRecordFieldKind, ArenaSpawnTarget,
    ArenaStmtKind, ArenaStreamStage, ArenaTypeExprTag, ArenaUserModule, ArenaWordPart, AstArena,
    ArenaRange, BindingTargetId, BlockId, ExprId, FunctionDefId, PatternId, StmtId, SugarView,
    TypeExprId,
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

use super::lowered_ops::{
    lowered_binary_op, lowered_value_matches,
};
use super::{
    CheckedPatternField, CheckedPatternVariantKey, CheckedPatternVariantKind,
    LoweredErrorExpr, LoweredProcessCommandArgv, LoweredProcessCommandBuilderEntry,
    LoweredRecordEntry,
    LoweredRecordUpdates, LoweredRunCapture, LoweredSpawnRun,
    driver_annotation_requires_validation, lowered_record_vec_get,
    lowered_record_vec_get_mut, lowered_record_vec_insert,
};

/// Checked source bindings map to dense runtime slots. Names describe the
/// runtime layout; only declaration identities select a source value.
/// Slots are never reused, so the high-water total sizes the runtime array.
#[derive(Clone, Default)]
pub(super) struct SlotScope {
    bindings: FxHashMap<BindingId, usize>,
    types: FxHashMap<BindingId, Type>,
    checked_slot_types: FxHashMap<usize, Type>,
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
    collect_slots: FxHashMap<ExprId, usize>,
    // Layout entries restore the enclosing runtime names when a block ends.
    declared: Vec<(Name, Option<usize>)>,
    high_water: usize,
}

/// Runtime layout snapshot on entering a nested block.
pub(super) struct SlotSnapshot {
    declared_len: usize,
    high_water: usize,
}

/// Source-ordered bindings and one lowered value for each static expansion entry.
pub(super) struct LoweredArgumentValues {
    pub(super) arguments: Vec<CallArgument>,
    pub(super) bindings: Vec<(BuildExprId, usize)>,
}

/// One call argument after static named-spread expansion. Source arguments
/// retain their real source identity for constant planning; record fields use
/// the checked projection source and the value already lowered from it.
#[derive(Clone)]
pub(super) struct CallArgument {
    pub(super) name: Option<Name>,
    pub(super) splice: bool,
    pub(super) source: ArgumentValueSource,
    pub(super) ty: Type,
    pub(super) span: Span,
    pub(super) lowered: Option<BuildExprId>,
}

struct LoweredFsFilesArgs {
    root: CallArgument,
    gitignore: Option<CallArgument>,
    stat: Option<CallArgument>,
    hidden: Option<CallArgument>,
    exts: Option<CallArgument>,
}

struct LoweredFsListArgs {
    path: CallArgument,
    stat: Option<CallArgument>,
    ordered: Option<CallArgument>,
}

struct LoweredArchiveTarCreateArgs {
    path: CallArgument,
    root: CallArgument,
    entries: CallArgument,
    compression: Option<CallArgument>,
    overwrite: Option<CallArgument>,
}

struct LoweredPathMkdirArgs {
    parents: Option<CallArgument>,
}

struct LoweredPathRemoveArgs {
    missing_ok: Option<CallArgument>,
}

struct LoweredPathWriteArgs {
    data: CallArgument,
}

/// The checker's binding of a registered call's source entries to the
/// parameters of its selected overload.
struct CheckedApiArguments {
    sig: &'static ModuleFnSig,
    values: Vec<Option<CallArgument>>,
}

impl CheckedApiArguments {
    fn get(&self, name: &str) -> Option<CallArgument> {
        self.values[self
            .sig
            .params
            .iter()
            .position(|param| param.name == name)?]
            .clone()
    }

    /// Parameter-ordered values with absent trailing defaults dropped.
    fn ordered(&self) -> Vec<Option<CallArgument>> {
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
    args: Vec<Option<CallArgument>>,
}

struct LoweredHashVerifyFileArgs {
    path: CallArgument,
    /// The algorithm the checksum argument's name selected.
    algorithm: &'static str,
    expected: CallArgument,
}

struct LoweredProcessCommandArgvArgs {
    target: CallArgument,
    argv: CallArgument,
    cwd: Option<CallArgument>,
    env: Option<CallArgument>,
    stdin: Option<CallArgument>,
    stdout: Option<CallArgument>,
    stderr: Option<CallArgument>,
    stdout_append: Option<CallArgument>,
    stderr_append: Option<CallArgument>,
    timeout: Option<CallArgument>,
    detach: Option<CallArgument>,
    new_session: Option<CallArgument>,
    ignore_hup: Option<CallArgument>,
    cpu_max: Option<CallArgument>,
    accept: Option<CallArgument>,
    same_group: Option<CallArgument>,
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
    call_positional_arguments, positional_call_args, single_positional_arena_call_arg, lowered_str_byte_op,
    script_argument_slots, lower_process_command_argv_args, lowered_module_call_args,
    lower_hash_verify_file_args, compact_call_arg_expr, lower_archive_tar_create_args,
    lower_path_mkdir_args, lower_path_remove_args, lower_path_write_args, lower_fs_files_args,
    lower_fs_list_args,
};

use commands::{
    lower_command_word_reference, lowered_arena_run_capture_type,
};

use types::{
    checked_type_annotation, stream_item_type, compact_type_check,
    compact_type_expr_name, guard_bound_type, record_binding_types, simple_binding_target,
    is_discard_name, lowered_checked_type, type_for_lowered_type,
};

#[cfg(all(test, debug_assertions))]
use types::checked_fact_is_resolved;

use values::{
    lower_const_param_default,
};


impl SlotScope {
    /// High-water slot total: the size the runtime slot array must have.
    pub(super) fn count(&self) -> usize {
        self.high_water
    }

    pub(super) fn resolve_binding(&self, binding: BindingId) -> Option<usize> {
        self.bindings.get(&binding).copied()
    }

    /// Retain a checked type only when every source identity in a local dense
    /// slot shares it; parameters and captures use their headers.
    pub(super) fn type_facts_by_slot(&self, first_local_slot: usize) -> Vec<Option<Type>> {
        assert!(
            first_local_slot <= self.high_water,
            "parameter and capture slots exceed their high-water total"
        );
        let mut slot_types = vec![None; self.high_water];
        let mut seen = vec![false; self.high_water];
        let mut ambiguous = vec![false; self.high_water];
        for (slot, ty) in &self.checked_slot_types {
            assert!(*slot < self.high_water, "checked slot exceeds its high-water total");
            if *slot < first_local_slot {
                continue;
            }
            slot_types[*slot] = Some(ty.clone());
            seen[*slot] = true;
        }
        for (binding, slot) in &self.bindings {
            assert!(*slot < self.high_water, "binding slot exceeds its high-water total");
            if *slot < first_local_slot {
                continue;
            }
            if ambiguous[*slot] {
                continue;
            }
            match self.binding_type(*binding) {
                Some(ty) if !matches!(ty, Type::Any | Type::Unknown | Type::Invalid) => {
                    match &slot_types[*slot] {
                    None if !seen[*slot] => {
                        slot_types[*slot] = Some(ty.clone());
                        seen[*slot] = true;
                    }
                    Some(previous) if previous == ty => {}
                    _ => {
                        slot_types[*slot] = None;
                        ambiguous[*slot] = true;
                    }
                    }
                }
                _ => {
                    slot_types[*slot] = None;
                    seen[*slot] = true;
                    ambiguous[*slot] = true;
                }
            }
        }
        slot_types
    }

    pub(super) fn record_checked_slot_type(&mut self, slot: usize, ty: Type) -> bool {
        if slot >= self.high_water || matches!(ty, Type::Any | Type::Unknown | Type::Invalid) {
            return false;
        }
        match self.checked_slot_types.get(&slot) {
            Some(existing) => *existing == ty,
            None => {
                self.checked_slot_types.insert(slot, ty);
                true
            }
        }
    }

    /// A use-site checker fact may differ from the binding's inferred source
    /// type. Keep the source type in the slot; the verifier checks the exact
    /// expression fact against it with the checker compatibility rule.
    pub(super) fn record_checked_binding_type(
        &mut self,
        binding: BindingId,
        slot: usize,
        ty: Type,
    ) -> bool {
        if self.resolve_binding(binding) != Some(slot) {
            return false;
        }

        let binding_type = self.types.get(&binding);
        let slot_type = self.checked_slot_types.get(&slot);
        if binding_type.zip(slot_type).is_some_and(|(binding, slot)| binding != slot) {
            return false;
        }
        let source_type = binding_type
            .or(slot_type)
            .cloned()
            .unwrap_or(ty);
        if !self.record_checked_slot_type(slot, source_type.clone()) {
            return false;
        }
        self.types.entry(binding).or_insert(source_type);
        true
    }

    fn binding_type(&self, binding: BindingId) -> Option<&Type> {
        self.types.get(&binding)
    }

    /// A physical declaration owns one slot, including the shared captures
    /// of pattern alternatives. Its spelling only records the runtime layout.
    pub(super) fn declare_source(&mut self, binding: BindingId, name: Name, ty: Option<Type>) -> usize {
        if let Some(slot) = self.resolve_binding(binding) {
            if let Some(ty) = ty {
                self.types.insert(binding, ty);
            }
            return slot;
        }
        let slot = self.reserve("source binding");
        let previous = self.indices.insert(name, slot);
        self.declared.push((name, previous));
        self.bindings.insert(binding, slot);
        if let Some(ty) = ty {
            self.types.insert(binding, ty);
        }
        slot
    }

    /// Parameters and captured globals already have their runtime positions.
    pub(super) fn bind_source_at(&mut self, binding: BindingId, name: Name, slot: usize, ty: Option<Type>) {
        self.bindings.insert(binding, slot);
        self.indices.insert(name, slot);
        if let Some(ty) = ty {
            self.types.insert(binding, ty);
        }
        self.high_water = self.high_water.max(slot + 1);
    }

    /// Allocate the next dense slot without binding a source-visible name.
    pub(super) fn reserve(&mut self, _tag: &str) -> usize {
        let slot = self.high_water;
        self.high_water += 1;
        slot
    }

    /// A retired capture restores its enclosing layout entry; its source
    /// identity remains unique and its slot remains allocated.
    pub(super) fn retire(&mut self, name: Name, slot: usize, _tag: &str) {
        if let Some(position) = self.declared.iter().rposition(|(declared, _)| *declared == name)
            && self.indices.get(&name) == Some(&slot)
        {
            let (_, previous) = self.declared.remove(position);
            match previous {
                Some(previous) => self.indices.insert(name, previous),
                None => self.indices.remove(&name),
            };
        }
    }

    /// Source identities already distinguish nested declarations. The snapshot
    /// restores only names used to publish the top-level runtime layout.
    pub(super) fn enter(&mut self) -> SlotSnapshot {
        SlotSnapshot {
            declared_len: self.declared.len(),
            high_water: self.high_water,
        }
    }

    pub(super) fn exit(&mut self, snapshot: SlotSnapshot) {
        for (name, previous) in self.declared[snapshot.declared_len..].iter().rev() {
            match previous {
                Some(slot) => self.indices.insert(*name, *slot),
                None => self.indices.remove(name),
            };
        }
        self.declared.truncate(snapshot.declared_len);
        self.high_water = self.high_water.max(snapshot.high_water);
    }

    /// Consume the scope, yielding runtime layout entries.
    pub(super) fn into_entries(self) -> impl Iterator<Item = (Name, usize)> {
        self.indices.into_iter()
    }
}

use super::{
    BuildBoolId, BuildBoolRow, BuildExprId, BuildExprRow, BuildIntId, BuildIntRow, BuildPatternId,
    BuildPatternIdSlots, BuildPatternRow, BuildScratch, BuildStmtId, BuildStmtRow, BuildTopKind,
    BuildTopStmtId, BuildTopStmtRow, COMPACT_CALL_BLOCKER_KIND_COUNT,
    COMPACT_COMMAND_BLOCKER_KIND_COUNT, COMPACT_STMT_KIND_COUNT,
    COMPACT_TYPE_EXPR_TAG_COUNT, CompactLowerConstructProbeOutput, Flow, FunctionBuild,
    LowerableFunctions, LoweredAssignPath, LoweredAssignStep, LoweredCallArg, LoweredCompFields,
    LoweredBindingIdentity, LoweredCompTarget, LoweredFmtPart,
    LoweredFunctionBlocker, LoweredFunctionKey, LoweredFunctionKind, LoweredFunctionUnit,
    LoweredModuleExport, LoweredModuleExportBinding, LoweredParamChecks, LoweredParamDefaults,
    LoweredParamKind, LoweredParamKinds, LoweredParamNames, LoweredParamRest, LoweredPipelineStage,
    LoweredReturnKind, LoweredRunArg, LoweredRunArgKind, LoweredRunEnv, LoweredRunOption,
    LoweredRunOperands, LoweredRunPipelineSegment, LoweredRunRedirection,
    LoweredRunTailOperand, LoweredStrPredicate, LoweredTopLevelBinding, LoweredTopLevelSlot,
    LoweredTopLevelSlots, LoweredType, LoweredTypeCheck, LoweredValue, ProgramBuild, ReduceByOp,
    ScanBytes, ScanCheck, ScanCondition, SchemeBodyFacts, SchemeType, StmtFlow, lowered_method_name,
};

pub(super) fn lowered_arena_type(
    ty: TypeExprId,
    declarations: &CompactDeclOutput,
    namespace: Option<Name>,
) -> Option<LoweredType> {
    checked_type_annotation(declarations, namespace, ty).and_then(lowered_checked_type)
}

fn lowered_scheme_parameter_kind(ty: &SchemeType) -> Option<LoweredParamKind> {
    match ty {
        SchemeType::Parameter(_) => Some(LoweredParamKind::Erased),
        SchemeType::Ground(Type::Any) => Some(LoweredParamKind::Typed(LoweredType::Any)),
        ty => lowered_scheme_physical_type(ty).map(LoweredParamKind::Typed),
    }
}

fn lowered_scheme_physical_type(ty: &SchemeType) -> Option<LoweredType> {
    match ty {
        SchemeType::Ground(ty) => lowered_checked_type(ty).filter(|kind| *kind != LoweredType::Any),
        SchemeType::Callable { kind, .. } => match kind {
            crate::sema::types::CallableKind::Pure => Some(LoweredType::Pure),
            crate::sema::types::CallableKind::Proc => Some(LoweredType::Proc),
            crate::sema::types::CallableKind::Stream => None,
        },
        SchemeType::List(_) => Some(LoweredType::List),
        SchemeType::Map(_, _) => Some(LoweredType::Map),
        SchemeType::Stream(_) => Some(LoweredType::Stream),
        SchemeType::Set(_) => Some(LoweredType::Set),
        SchemeType::Record { .. } => Some(LoweredType::Record),
        SchemeType::Result(_, _) => Some(LoweredType::Result),
        SchemeType::Parameter(_)
        | SchemeType::Optional(_)
        | SchemeType::Union(_)
        | SchemeType::Validated(_, _) => None,
    }
}

fn lowered_scheme_return_kind(ty: &SchemeType) -> Option<LoweredReturnKind> {
    match ty {
        SchemeType::Ground(ty) => match ty.unvalidated() {
            Type::Result(ok, _) => {
                lowered_scheme_physical_type(&SchemeType::Ground((**ok).clone()))
                    .map(LoweredReturnKind::Result)
            }
            Type::Optional(inner) if matches!(inner.unvalidated(), Type::Result(_, _)) => {
                Some(LoweredReturnKind::OptionalResult)
            }
            _ => lowered_scheme_physical_type(&SchemeType::Ground(ty.clone()))
                .map(LoweredReturnKind::Plain),
        },
        SchemeType::Parameter(_) => Some(LoweredReturnKind::Erased),
        SchemeType::Callable { .. } => lowered_scheme_physical_type(ty).map(LoweredReturnKind::Plain),
        SchemeType::Result(ok, _) => {
            Some(lowered_scheme_physical_type(ok).map_or(LoweredReturnKind::QuantifiedResult, LoweredReturnKind::Result))
        }
        SchemeType::Optional(inner) if matches!(inner.as_ref(), SchemeType::Result(_, _)) => {
            Some(LoweredReturnKind::OptionalResult)
        }
        SchemeType::Record { .. } => Some(LoweredReturnKind::Plain(LoweredType::Record)),
        SchemeType::List(_) => Some(LoweredReturnKind::Plain(LoweredType::List)),
        SchemeType::Map(_, _) => Some(LoweredReturnKind::Plain(LoweredType::Map)),
        SchemeType::Stream(_) => Some(LoweredReturnKind::Plain(LoweredType::Stream)),
        SchemeType::Set(_) => Some(LoweredReturnKind::Plain(LoweredType::Set)),
        SchemeType::Optional(_)
        | SchemeType::Union(_)
        | SchemeType::Validated(_, _) => None,
    }
}

#[cfg(test)]
mod generic_parameter_kind_tests {
    use super::{LoweredParamKind, LoweredType, lowered_scheme_parameter_kind};
    use crate::sema::schemes::SchemeType;
    use crate::sema::types::Type;

    #[test]
    fn explicit_any_scheme_parameter_keeps_its_dynamic_kind() {
        assert_eq!(
            lowered_scheme_parameter_kind(&SchemeType::Ground(Type::Any)),
            Some(LoweredParamKind::Typed(LoweredType::Any))
        );
    }
}

pub(super) fn probe_compact_lower_constructed_bodies(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    source: &str,
) -> CompactLowerConstructProbeOutput {
    let scope = CompactRootScope::whole_program(program);
    probe_compact_lower_constructed_bodies_in_scope(program, declarations, source, &scope)
}

pub(in crate::runtime::eval) fn probe_compact_lower_constructed_bodies_in_scope(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    source: &str,
    scope: &CompactRootScope<'_>,
) -> CompactLowerConstructProbeOutput {
    let scope = Rc::new(scope.clone());
    let mut probe = CompactLowerConstructProbe {
        program,
        scope: Rc::clone(&scope),
        declarations,
        qualified_schemes: &declarations.function_schemes,
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
        scratch: Rc::new(RefCell::new(BuildScratch { producer_demand: Some(super::ProducerDemand::Absent), ..BuildScratch::default() })),
        with_error_source_exprs: FxHashMap::default(),
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

/// A borrowed entry root and the resolved module closure prepared for it.
#[derive(Clone)]
pub(in crate::runtime::eval) struct CompactRootScope<'a> {
    root_statements: ArenaRange,
    modules: Vec<&'a ArenaUserModule>,
}

impl<'a> CompactRootScope<'a> {
    pub(in crate::runtime::eval) fn whole_program(program: &'a ArenaProgram) -> Self {
        Self {
            root_statements: program.statements,
            modules: program.modules.iter().collect(),
        }
    }

    pub(in crate::runtime::eval) fn from_view(view: &CheckBundleView<'a>) -> Self {
        Self {
            root_statements: view.root_statements(),
            modules: view.modules().collect(),
        }
    }

    pub(in crate::runtime::eval) fn root_statements(&self) -> ArenaRange {
        self.root_statements
    }

    pub(in crate::runtime::eval) fn source_ids(
        &self,
        program: &ArenaProgram,
    ) -> FxHashSet<SourceId> {
        let mut source_ids = program
            .arena
            .stmt_ids(self.root_statements)
            .map(|id| program.arena.stmt(id).span.source_id)
            .collect::<FxHashSet<_>>();
        for module in &self.modules {
            source_ids.extend(
                program
                    .module_statements(module)
                    .map(|id| program.arena.stmt(id).span.source_id),
            );
        }
        source_ids
    }

    fn module_named(&self, name: Name) -> Option<&'a ArenaUserModule> {
        self.modules.iter().copied().find(|module| module.name == name)
    }

    fn module_with_key(&self, key: &str) -> Option<&'a ArenaUserModule> {
        self.modules.iter().copied().find(|module| module.key == key)
    }
}

pub(super) fn lower_compact_function_units_into(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    qualified_schemes: &BTreeMap<QualifiedName, Arc<crate::sema::schemes::TypeScheme>>,
    source: &str,
    sources: &SourceMap,
    scope: &CompactRootScope<'_>,
    stdlib_linkage: StdlibLowerLinkage,
    producer_demand: super::ProducerDemand,
    mut emit: impl FnMut(LoweredFunctionUnit) -> Result<(), super::indexed::IrBuildError>,
) -> Result<(), super::indexed::IrBuildError> {
    // Every function's body must be walked to find its call edges, and finding
    // them once per function would make preparation quadratic in program size.
    // One index answers the whole loop: each unit's dependencies and its
    // component metadata come from the same walk of every body.
    let scope = Rc::new(scope.clone());
    let index = Rc::new(CompactFunctionIndex::new(program, &scope));
    let candidates = index
        .defs
        .iter()
        .map(|function| function.key)
        .collect::<Vec<_>>();
    let function_defs = Rc::new(RefCell::new(Some(Rc::clone(&index))));
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
    let mut recorder = CompactLowerConstructProbe {
        program,
        scope: Rc::clone(&scope),
        declarations,
        qualified_schemes,
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
        scratch: Rc::new(RefCell::new(BuildScratch { producer_demand: Some(producer_demand), ..BuildScratch::default() })),
        with_error_source_exprs: FxHashMap::default(),
    };
    let mut prefixes: FxHashMap<Option<Name>, CompactTopLevelPrefix> = FxHashMap::default();
    for function in index.defs.iter().copied() {
        recorder.current_namespace = function.namespace;
        let prefix = prefixes
            .entry(function.namespace)
            .or_insert_with(|| {
                CompactTopLevelPrefix::for_namespace(
                    program,
                    &scope,
                    declarations,
                    function.namespace,
                )
            });
        prefix.advance_to_recording(&recorder, function.id);
        let top_level_known = prefix.known.clone();
        let mut probe = CompactLowerConstructProbe {
            program,
            scope: Rc::clone(&scope),
            declarations,
            qualified_schemes,
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
            scratch: Rc::new(RefCell::new(BuildScratch { producer_demand: Some(producer_demand), ..BuildScratch::default() })),
            with_error_source_exprs: FxHashMap::default(),
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
    qualified_schemes: &BTreeMap<QualifiedName, Arc<crate::sema::schemes::TypeScheme>>,
    source: &str,
    sources: &SourceMap,
    functions: &LowerableFunctions<'_>,
    scope: &CompactRootScope<'_>,
    producer_demand: super::ProducerDemand,
) -> (ProgramBuild, CompactLowerConstructProbeOutput) {
    let scope = Rc::new(scope.clone());
    let mut probe = CompactLowerConstructProbe {
        program,
        scope: Rc::clone(&scope),
        declarations,
        qualified_schemes,
        bodies: &declarations.bodies,
        source,
        sources: Some(sources),
        current_namespace: None,
        functions: Some(functions),
        top_level_known: compact_top_level_known(
            program,
            &scope,
            declarations,
            qualified_schemes,
            source,
            Some(sources),
            None,
            Some(functions),
        ),
        output: CompactLowerConstructProbeOutput::default(),
        last_blocker_detail: None,
        stdlib_linkage: StdlibLowerLinkage::Local,
        function_defs: Rc::new(RefCell::new(None)),
        scratch: Rc::new(RefCell::new(BuildScratch { producer_demand: Some(producer_demand), ..BuildScratch::default() })),
        with_error_source_exprs: FxHashMap::default(),
    };
    let root = program.arena.stmt_ids(scope.root_statements).collect::<Vec<_>>();
    let lowered = probe.lower_program_statements(&root);
    (lowered, probe.output)
}

fn compact_top_level_known(
    program: &ArenaProgram,
    scope: &CompactRootScope<'_>,
    declarations: &CompactDeclOutput,
    qualified_schemes: &BTreeMap<QualifiedName, Arc<crate::sema::schemes::TypeScheme>>,
    source: &str,
    sources: Option<&SourceMap>,
    namespace: Option<Name>,
    functions: Option<&LowerableFunctions<'_>>,
) -> FxHashMap<Name, LoweredTopLevelBinding> {
    let statements = match namespace {
        Some(namespace) => scope
            .module_named(namespace)
            .map(|module| program.module_statements(module).collect::<Vec<_>>())
            .unwrap_or_default(),
        None => program.arena.stmt_ids(scope.root_statements).collect::<Vec<_>>(),
    };
    let probe = CompactLowerConstructProbe {
        program,
        scope: Rc::new(scope.clone()),
        declarations,
        qualified_schemes,
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
        with_error_source_exprs: FxHashMap::default(),
    };
    probe.collect_top_level_known(&statements)
}

/// The bindings visible to one function definition.
///
/// Used by the construct probe, which walks a module's statements itself and so
/// cannot share the cursor `lower_compact_function_units_into` keeps.
fn compact_function_top_level_known(
    program: &ArenaProgram,
    scope: &CompactRootScope<'_>,
    declarations: &CompactDeclOutput,
    qualified_schemes: &BTreeMap<QualifiedName, Arc<crate::sema::schemes::TypeScheme>>,
    source: &str,
    sources: Option<&SourceMap>,
    namespace: Option<Name>,
    function_id: FunctionDefId,
    functions: Option<&LowerableFunctions<'_>>,
) -> FxHashMap<Name, LoweredTopLevelBinding> {
    let probe = CompactLowerConstructProbe {
        program,
        scope: Rc::new(scope.clone()),
        declarations,
        qualified_schemes,
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
        with_error_source_exprs: FxHashMap::default(),
    };
    let mut prefix = CompactTopLevelPrefix::for_namespace(program, scope, declarations, namespace);
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
    fn for_namespace(
        program: &ArenaProgram,
        scope: &CompactRootScope<'_>,
        declarations: &CompactDeclOutput,
        namespace: Option<Name>,
    ) -> Self {
        let statements = match namespace {
            Some(namespace) => scope
                .module_named(namespace)
                .map(|module| program.module_statements(module).collect::<Vec<_>>())
                .unwrap_or_default(),
            None => program.arena.stmt_ids(scope.root_statements).collect::<Vec<_>>(),
        };
        Self {
            statements,
            known: top_level_known_with_runtime_bindings(script_arguments_type(
                program,
                scope,
                declarations,
            )),
            position: 0,
        }
    }

    /// Record every statement before `function_id`'s own definition.
    fn advance_to_recording(
        &mut self,
        recorder: &CompactLowerConstructProbe<'_, '_>,
        function_id: FunctionDefId,
    ) {
        recorder.attach_runtime_bindings(&mut self.known);
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
    fn new(program: &ArenaProgram, scope: &CompactRootScope<'_>) -> Self {
        let defs = compact_function_defs(program, scope);
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

    fn checked_target_key(
        &self,
        target: crate::sema::check::CheckedFunctionTarget,
    ) -> Option<LoweredFunctionKey> {
        self.defs.iter().find_map(|function| {
            if function.id != target.definition {
                return None;
            }
            let matches = match function.key {
                LoweredFunctionKey::Name(name) => {
                    target.namespace.is_none() && name == target.name
                }
                LoweredFunctionKey::Qualified(qualified) => {
                    target.namespace == Some(qualified.namespace)
                        && qualified.member == target.name
                }
            };
            matches.then_some(function.key)
        })
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
    exported: bool,
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

fn compact_collect_arena_child_call_edges(
    program: &ArenaProgram,
    child: ArenaChild,
    namespace: Option<Name>,
    index_of: &FxHashMap<LoweredFunctionKey, usize>,
    edges: &mut Vec<usize>,
) {
    match child {
        ArenaChild::Stmt(stmt) => {
            compact_collect_stmt_call_edges(program, stmt, namespace, index_of, edges);
        }
        ArenaChild::Expr(expr) => {
            compact_collect_expr_call_edges(program, expr, namespace, index_of, edges);
        }
        ArenaChild::Block(block) => {
            compact_collect_block_call_edges(program, block, namespace, index_of, edges);
        }
        ArenaChild::Pattern(_)
        | ArenaChild::BindingTarget(_)
        | ArenaChild::AssignTarget(_)
        | ArenaChild::TypeExpr(_)
        | ArenaChild::BuilderBlock(_) => {}
    }
}

fn compact_collect_stmt_call_edges(
    program: &ArenaProgram,
    id: StmtId,
    namespace: Option<Name>,
    index_of: &FxHashMap<LoweredFunctionKey, usize>,
    edges: &mut Vec<usize>,
) {
    if !matches!(
        program.arena.stmt(id).kind,
        ArenaStmtKind::Export(_)
            | ArenaStmtKind::Sugar { .. }
            | ArenaStmtKind::Let { .. }
            | ArenaStmtKind::Const { .. }
            | ArenaStmtKind::Var { .. }
            | ArenaStmtKind::Assign { .. }
            | ArenaStmtKind::Defer(_, _)
            | ArenaStmtKind::Yield(_)
            | ArenaStmtKind::Return(Some(_))
            | ArenaStmtKind::If { .. }
            | ArenaStmtKind::While { .. }
            | ArenaStmtKind::For { .. }
            | ArenaStmtKind::Loop { .. }
            | ArenaStmtKind::With { .. }
            | ArenaStmtKind::Guard { .. }
            | ArenaStmtKind::Assert { .. }
            | ArenaStmtKind::Break { value: Some(_) }
            | ArenaStmtKind::Exit(_)
            | ArenaStmtKind::YieldDelegate(_)
            | ArenaStmtKind::Match { .. }
            | ArenaStmtKind::Expr(_)
    ) {
        return;
    }
    if matches!(
        program.arena.stmt(id).kind,
        ArenaStmtKind::Let {
            initializer: ArenaExprOrRun::Run(_),
            ..
        }
        | ArenaStmtKind::Const {
            initializer: ArenaExprOrRun::Run(_),
            ..
        }
        | ArenaStmtKind::Var {
            initializer: ArenaExprOrRun::Run(_),
            ..
        }
        | ArenaStmtKind::Assign {
            value: ArenaExprOrRun::Run(_),
            ..
        }
        | ArenaStmtKind::Return(Some(ArenaExprOrRun::Run(_)))
        | ArenaStmtKind::Yield(ArenaExprOrRun::Run(_))
        | ArenaStmtKind::Defer(ArenaExprOrRun::Run(_), _)
        | ArenaStmtKind::Guard {
            initializer: ArenaExprOrRun::Run(_),
            ..
        }
    ) {
        return;
    }
    program
        .arena
        .for_each_stmt_child(id, SugarView::Core, |child| {
            compact_collect_arena_child_call_edges(program, child, namespace, index_of, edges);
        });
}

fn compact_collect_expr_call_edges(
    program: &ArenaProgram,
    id: ExprId,
    namespace: Option<Name>,
    index_of: &FxHashMap<LoweredFunctionKey, usize>,
    edges: &mut Vec<usize>,
) {
    if let ArenaExprKind::Call { callee, .. } = program.arena.expr(id).kind
        && let ArenaExprKind::Ident(name) = program.arena.expr(callee).kind
        && let Some(index) = index_of.get(&compact_function_key(namespace, name))
    {
        edges.push(*index);
    }
    if matches!(
        program.arena.expr(id).kind,
        ArenaExprKind::Pipeline { .. }
            | ArenaExprKind::StructuredPipeline { .. }
            | ArenaExprKind::Run(_)
            | ArenaExprKind::Spawn(_)
            | ArenaExprKind::Wait(_)
    ) {
        return;
    }
    if let ArenaExprKind::BuilderCall { call, .. } = program.arena.expr(id).kind {
        compact_collect_expr_call_edges(program, call, namespace, index_of, edges);
        return;
    }
    program
        .arena
        .for_each_expr_child(id, |child| {
            compact_collect_arena_child_call_edges(program, child, namespace, index_of, edges);
        });
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

fn compact_function_defs(
    program: &ArenaProgram,
    scope: &CompactRootScope<'_>,
) -> Vec<CompactFunctionDef> {
    let mut functions = Vec::new();
    for stmt in program.arena.stmt_ids(scope.root_statements) {
        collect_compact_function_def(program, stmt, None, false, &mut functions);
    }
    for module in scope.modules.iter().copied() {
        for stmt in program.module_statements(module) {
            collect_compact_function_def(
                program,
                stmt,
                Some(module.name),
                false,
                &mut functions,
            );
        }
    }
    functions
}

pub(super) struct CompactFunctionSelection {
    pub(super) keys: Vec<LoweredFunctionKey>,
    pub(super) producer_demand: super::ProducerDemand,
}

pub(super) fn compact_function_selection(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    scope: &CompactRootScope<'_>,
) -> CompactFunctionSelection {
    let mut producer_demand = super::ProducerDemand::Absent;
    let keys = compact_function_defs(program, scope).into_iter().map(|function| {
        let definition = program.arena.function_def(function.id);
        let span = program.arena.span(program.arena.block(definition.body).span);
        if declarations.scheme_bodies.get(&span).is_some_and(|facts| !facts.registered_calls().is_empty()) { producer_demand = super::ProducerDemand::Needed; }
        function.key
    }).collect();
    if producer_demand == super::ProducerDemand::Absent {
        let constructors = super::ProducerDemand::checked_result_constructors(program, declarations)
            .collect::<std::collections::BTreeSet<_>>();
        let contains_constructor = |statement| {
            let span = program.arena.stmt(statement).span;
            constructors.range(Span::at(span.source_id, span.start())..Span::at(span.source_id, span.end()))
                .next().is_some()
        };
        if program.arena.stmt_ids(scope.root_statements).any(contains_constructor)
            || scope.modules.iter().any(|module| program.module_statements(module).any(contains_constructor))
        {
            producer_demand = super::ProducerDemand::Needed;
        }
    }
    CompactFunctionSelection { keys, producer_demand }
}

fn collect_compact_function_def(
    program: &ArenaProgram,
    id: StmtId,
    namespace: Option<Name>,
    exported: bool,
    functions: &mut Vec<CompactFunctionDef>,
) {
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Export(inner) => {
            collect_compact_function_def(program, inner, namespace, true, functions);
        }
        ArenaStmtKind::PureDef(def) => {
            let name = program.arena.function_def(def).name;
            functions.push(CompactFunctionDef {
                key: compact_function_key(namespace, name),
                id: def,
                pure: true,
                exported,
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
                exported,
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
                exported,
                namespace,
                definition_span: program.arena.stmt(id).span,
            });
        }
        _ => {}
    }
}

struct CompactLowerConstructProbe<'a, 'defs> {
    program: &'a ArenaProgram,
    scope: Rc<CompactRootScope<'a>>,
    declarations: &'a CompactDeclOutput,
    qualified_schemes: &'a BTreeMap<QualifiedName, Arc<crate::sema::schemes::TypeScheme>>,
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
    with_error_source_exprs: FxHashMap<ExprId, usize>,
}

fn with_error_source_exprs(program: &ArenaProgram, initializer: ExprId) -> Vec<ExprId> {
    let mut sources = FxHashSet::default();
    let mut pending = vec![ArenaChild::Expr(initializer)];
    while let Some(child) = pending.pop() {
        match child {
            ArenaChild::Expr(expr) => {
                if let ArenaExprKind::Try(value) = program.arena.expr(expr).kind {
                    sources.insert(value);
                }
                program
                    .arena
                    .for_each_expr_child(expr, |child| pending.push(child));
            }
            ArenaChild::Stmt(stmt) => program
                .arena
                .for_each_stmt_child(stmt, SugarView::Source, |child| pending.push(child)),
            ArenaChild::Block(block) => program
                .arena
                .for_each_block_child(block, |child| pending.push(child)),
            ArenaChild::Pattern(pattern) => program
                .arena
                .for_each_pattern_child(pattern, |child| pending.push(child)),
            ArenaChild::BindingTarget(target) => program
                .arena
                .for_each_binding_target_child(target, |child| pending.push(child)),
            ArenaChild::AssignTarget(target) => program
                .arena
                .for_each_assign_target_child(target, |child| pending.push(child)),
            ArenaChild::TypeExpr(ty) => program
                .arena
                .for_each_type_expr_child(ty, |child| pending.push(child)),
            ArenaChild::BuilderBlock(block) => program
                .arena
                .for_each_builder_child(block, |child| pending.push(child)),
        }
    }
    sources.insert(initializer);
    let mut sources = sources.into_iter().collect::<Vec<_>>();
    sources.sort_unstable_by_key(|expr| expr.index());
    sources
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

macro_rules! compact_expr_kind_catalog {
    ($($kind:pat => ($index:literal, $label:literal);)+) => {
        const COMPACT_EXPR_KIND_CATALOG: &[(usize, &str)] = &[
            $(($index, $label),)+
        ];

        pub(super) const COMPACT_EXPR_KIND_COUNT: usize =
            compact_expr_kind_count(COMPACT_EXPR_KIND_CATALOG);

        fn compact_expr_kind_index(kind: ArenaExprKind) -> usize {
            match kind {
                $($kind => $index,)+
            }
        }

        fn compact_expr_kind_label(kind: ArenaExprKind) -> &'static str {
            match kind {
                $($kind => $label,)+
            }
        }
    };
}

const fn compact_expr_kind_count(catalog: &[(usize, &str)]) -> usize {
    let mut count = 0;
    let mut index = 0;
    while index < catalog.len() {
        let candidate = catalog[index].0 + 1;
        if candidate > count {
            count = candidate;
        }
        index += 1;
    }
    count
}

compact_expr_kind_catalog! {
    ArenaExprKind::Null => (0, "null");
    ArenaExprKind::Bool(_) => (1, "bool");
    ArenaExprKind::Int(_) => (2, "int");
    ArenaExprKind::Float(_) => (3, "float");
    ArenaExprKind::Duration(_) => (4, "duration");
    ArenaExprKind::Str(_) => (5, "str");
    ArenaExprKind::PathStr(_) => (6, "path_str");
    ArenaExprKind::GlobStr(_) => (7, "glob_str");
    ArenaExprKind::FmtString(_) => (8, "fmt_string");
    ArenaExprKind::PathFmtString(_) => (9, "path_fmt_string");
    ArenaExprKind::Bytes(_) => (10, "bytes");
    ArenaExprKind::Ident(_) => (11, "ident");
    ArenaExprKind::Item => (12, "item");
    ArenaExprKind::LastStatus => (13, "last_status");
    ArenaExprKind::List(_) => (14, "list");
    ArenaExprKind::ListComp { .. } => (15, "list_comp");
    ArenaExprKind::MapComp { .. } => (16, "map_comp");
    ArenaExprKind::Record(_) => (17, "record");
    ArenaExprKind::If { .. } => (18, "if");
    ArenaExprKind::Match { .. } => (19, "match");
    ArenaExprKind::PatternTest { .. } => (19, "pattern_test");
    ArenaExprKind::PatternCondition { .. } => (19, "pattern_condition");
    ArenaExprKind::Unary { .. } => (20, "unary");
    ArenaExprKind::Binary { .. } => (21, "binary");
    ArenaExprKind::Call { .. } => (22, "call");
    ArenaExprKind::Field { .. } => (23, "field");
    ArenaExprKind::NullSafeField { .. } => (24, "null_safe_field");
    ArenaExprKind::Index { .. } => (25, "index");
    ArenaExprKind::Slice { .. } => (26, "slice");
    ArenaExprKind::EnvString(_) => (27, "env_string");
    ArenaExprKind::EnvPathList => (28, "env_path_list");
    ArenaExprKind::Pipeline { .. } => (29, "pipeline");
    ArenaExprKind::StructuredPipeline { .. } => (30, "structured_pipeline");
    ArenaExprKind::Run(_) => (31, "run");
    ArenaExprKind::Spawn(_) => (32, "spawn");
    ArenaExprKind::Wait(_) => (33, "wait");
    ArenaExprKind::BuilderCall { .. } => (34, "builder_call");
    ArenaExprKind::Try(_) => (35, "try");
    ArenaExprKind::Require { .. } => (36, "require");
    ArenaExprKind::Loop { .. } => (37, "loop");
    ArenaExprKind::Retry { .. } => (38, "retry");
    ArenaExprKind::ValueBlock(_) => (39, "value_block");
    ArenaExprKind::ComparisonChain(_) => (40, "comparison-chain");
    ArenaExprKind::Regex(_) => (41, "regex_literal");
    ArenaExprKind::Capture(_) => (42, "try");
    ArenaExprKind::ValuePipelineCall { .. } => (43, "value_pipeline_call");
    ArenaExprKind::ErrorContext { .. } => (44, "error_context");
    ArenaExprKind::ContextScope { .. } => (45, "context_scope");
    ArenaExprKind::TempDirScope { .. } => (46, "tempdir_scope");
    ArenaExprKind::Convert { .. } => (47, "convert");
    ArenaExprKind::Set(_) => (48, "set");
    ArenaExprKind::SetComp { .. } => (49, "set_comp");
    ArenaExprKind::Collect { .. } => (50, "collect");
    ArenaExprKind::ResourceScope { .. } => (51, "resource_scope");
}

fn compact_checked_type_is_concrete(ty: &Type) -> bool {
    !matches!(ty, Type::Any | Type::Unknown | Type::Invalid)
        && !ty.contains_any()
        && !ty.contains_inference()
}

#[cfg(all(test, debug_assertions))]
static CHECKED_TYPE_FACT_MISMATCHES: std::sync::Mutex<Vec<String>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(all(test, debug_assertions))]
fn note_checked_type_fact_mismatch(
    span: Span,
    annotation_kind: Option<LoweredType>,
    annotation_type: &Type,
    binding_type: Option<&Type>,
) {
    let Some(binding_type) = binding_type.filter(|ty| checked_fact_is_resolved(ty)) else {
        return;
    };
    let present = binding_type.optional_inner().unwrap_or(binding_type);
    let kind_agrees = match (annotation_kind, lowered_checked_type(present)) {
        (None | Some(LoweredType::Any), _) | (_, None) => true,
        (Some(kind), Some(expected)) => kind == expected,
    };
    if !kind_agrees || annotation_type != binding_type {
        CHECKED_TYPE_FACT_MISMATCHES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(format!(
                "{span:?}: annotation {annotation_kind:?} {annotation_type}, binding {binding_type}"
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
        // The declared family name remains the user-facing display name.
        CompactErrorFamilyKey::Qualified(name) => name.member.to_string(),
    }
}

fn compact_error_family_identity(key: CompactErrorFamilyKey) -> String {
    match key {
        CompactErrorFamilyKey::Local(name) => name.to_string(),
        CompactErrorFamilyKey::Qualified(name) => name.to_string(),
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
    qualified_schemes: &BTreeMap<QualifiedName, Arc<crate::sema::schemes::TypeScheme>>,
    resolved_bindings: &crate::sema::check::ResolvedBindings,
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
                let id = resolved_bindings.target(target)?;
                let declaration = resolved_bindings.declarations.get(&id)?;
                if declaration.name != name || !declaration.exported {
                    return None;
                }
                let binding = LoweredBindingIdentity {
                    id,
                    name,
                    span: declaration.span,
                    owner: declaration.namespace,
                    built_in: id == BindingId::args(),
                    exported: declaration.exported,
                };
                exports.push(LoweredModuleExport::Value {
                    name,
                    binding: LoweredModuleExportBinding::Semantic(binding),
                });
            }
            ArenaStmtKind::ProcDef(def) | ArenaStmtKind::PureDef(def) => {
                let name = program.arena.function_def(def).name;
                let function_name = QualifiedName::new(function_namespace, name);
                let kind = if matches!(program.arena.stmt(inner).kind, ArenaStmtKind::ProcDef(_)) {
                    crate::sema::types::CallableKind::Proc
                } else { crate::sema::types::CallableKind::Pure };
                let generic = qualified_schemes.get(&function_name).is_some_and(|scheme| {
                    !scheme.type_parameters().is_empty() || !scheme.row_parameters().is_empty()
                });
                exports.push(if generic {
                    LoweredModuleExport::GenericCallable { kind, name, function_namespace }
                } else if kind == crate::sema::types::CallableKind::Pure {
                    LoweredModuleExport::Pure { name, function_namespace }
                } else { LoweredModuleExport::Proc { name, function_namespace } });
            }
            ArenaStmtKind::StreamDef(_)
            | ArenaStmtKind::TypeDef(_)
            | ArenaStmtKind::ErrorDef(_) => {}
            ArenaStmtKind::Use(_)
            | ArenaStmtKind::Export(_)
            | ArenaStmtKind::Assign { .. }
            | ArenaStmtKind::CliMain(_)
            | ArenaStmtKind::SignalHook(_)
            | ArenaStmtKind::Return(_)
            | ArenaStmtKind::Yield(_)
            | ArenaStmtKind::YieldDelegate(_)
            | ArenaStmtKind::Defer(..)
            | ArenaStmtKind::If { .. }
            | ArenaStmtKind::While { .. }
            | ArenaStmtKind::For { .. }
            | ArenaStmtKind::With { .. }
            | ArenaStmtKind::Loop { .. }
            | ArenaStmtKind::Guard { .. }
            | ArenaStmtKind::Sugar { .. }
            | ArenaStmtKind::Assert { .. }
            | ArenaStmtKind::Break { .. }
            | ArenaStmtKind::Continue
            | ArenaStmtKind::Exit(_)
            | ArenaStmtKind::Match { .. }
            | ArenaStmtKind::Command(_)
            | ArenaStmtKind::TailBareIdent(_)
            | ArenaStmtKind::Expr(_) => return None,
        }
    }
    Some(exports)
}

pub(super) fn lower_literal_constant(
    value: &crate::sema::constants::LiteralConstant,
    enums: Option<&crate::sema::wire_enums::PreparedWireEnums>,
) -> Option<LoweredValue> {
    Some(lower_prepared_literal_with_purpose(value, enums, PreparedLiteralPurpose::Data)?.0)
}

#[derive(Clone, Copy)]
enum PreparedLiteralPurpose { Data, ConstructorProof }

pub(super) fn lower_prepared_literal(
    value: &crate::sema::constants::LiteralConstant,
    enums: Option<&crate::sema::wire_enums::PreparedWireEnums>,
) -> Option<super::PreparedConstantValue> {
    lower_prepared_literal_with_purpose(value, enums, PreparedLiteralPurpose::ConstructorProof)
}

fn lower_prepared_literal_with_purpose(
    value: &crate::sema::constants::LiteralConstant,
    enums: Option<&crate::sema::wire_enums::PreparedWireEnums>,
    purpose: PreparedLiteralPurpose,
) -> Option<super::PreparedConstantValue> {
    use crate::sema::constants::{ConstantRecordOrigin, LiteralConstant as C};
    use super::{PreparedConstantOrigin as O, PreparedConstantValue as V};
    let mut origin = None;
    let value = match value {
        C::Regex(literal) => LoweredValue::Regex(Box::new(RegexValue {
            pattern: literal.pattern.to_string(),
            regex: literal.prepared.get()?.as_ref().ok()?.clone(),
        })),
        C::Tag { family, variant, fields } => {
            let mut values = Vec::with_capacity(fields.len());
            let mut origins = BTreeMap::new();
            for (index, value) in fields.iter().enumerate() {
                let V(value, child) = lower_prepared_literal_with_purpose(value, enums, purpose)?;
                values.push(value);
                if let Some(child) = child { origins.insert(index, child); }
            }
            if !origins.is_empty() { origin = Some(Arc::new(O::Tag(origins))); }
            LoweredValue::Tag(Box::new(super::LoweredTagValue {
                type_name: *family,
                wire: enums.and_then(|enums| enums.mappings.get(family)).cloned(),
                name: Arc::from(variant.as_str().as_str()),
                fields: values,
            }))
        }
        C::Null => LoweredValue::Null,
        C::Bool(value) => LoweredValue::Bool(*value),
        C::Int(value) => LoweredValue::Int(*value),
        C::Float(value) => LoweredValue::Float(crate::runtime::value::FloatValue::new(f64::from_bits(*value))),
        C::Duration(millis) => LoweredValue::Duration(DurationValue { millis: *millis }),
        C::Str(value) => LoweredValue::Str(value.clone()),
        C::Bytes(value) => LoweredValue::Bytes(value.clone()),
        C::Path(value) => LoweredValue::Path(PathValue::from_text(value).ok()?),
        C::EmptyMap => LoweredValue::Map(Arc::new(BTreeMap::new())),
        C::Map(entries) => {
            let mut values = BTreeMap::new();
            let mut origins = BTreeMap::new();
            for (key, value) in entries.iter() {
                let V(value, child) = lower_prepared_literal_with_purpose(value, enums, purpose)?;
                values.insert(key.clone(), value);
                if let Some(child) = child { origins.insert(key.clone(), child); }
            }
            if !origins.is_empty() { origin = Some(Arc::new(O::Map(origins))); }
            LoweredValue::Map(Arc::new(values))
        }
        C::List(entries) => {
            let mut values = Vec::with_capacity(entries.len());
            let mut origins = BTreeMap::new();
            for (index, value) in entries.iter().enumerate() {
                let V(value, child) = lower_prepared_literal_with_purpose(value, enums, purpose)?;
                values.push(value);
                if let Some(child) = child { origins.insert(index, child); }
            }
            if !origins.is_empty() { origin = Some(Arc::new(O::List(origins))); }
            LoweredValue::SharedList(Arc::new(values))
        }
        C::Set(values) => LoweredValue::Set(values.clone()),
        C::Record(entries) => {
            let constructor = match purpose {
                PreparedLiteralPurpose::Data => None,
                PreparedLiteralPurpose::ConstructorProof => match entries.constructor_origin() {
                ConstantRecordOrigin::Structural => None,
                ConstantRecordOrigin::Pending { .. } => return None,
                ConstantRecordOrigin::Checked(constructor) => {
                    if constructor.ty.contains_inference() || constructor.ty.contains_recovery() || constructor.ty.contains_any() { return None; }
                    constructor.ty.nominal_name().map(|_| Arc::new(constructor.ty.clone()))
                }
                },
            };
            let mut values = BTreeMap::new();
            let mut fields = BTreeMap::new();
            for (name, value) in entries.iter() {
                let V(value, child) = lower_prepared_literal_with_purpose(value, enums, purpose)?;
                values.insert(Arc::<str>::from(name.as_str().as_str()), value);
                if let Some(child) = child { fields.insert(*name, child); }
            }
            if constructor.is_some() || !fields.is_empty() { origin = Some(Arc::new(O::Record { constructor, fields })); }
            LoweredValue::Record(Arc::new(values))
        }
    };
    Some(V(value, origin))
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
        let root = self
            .program
            .arena
            .stmt_ids(self.scope.root_statements)
            .collect::<Vec<_>>();
        self.top_level_known = self.collect_top_level_known(&root);
        self.lower_top_level_program(&root);
        for stmt in root {
            self.probe_function_stmt(stmt, false);
        }
        let modules = self.scope.modules.clone();
        for module in modules {
            let statements = self.program.module_statements(module).collect::<Vec<_>>();
            self.current_namespace = Some(module.name);
            self.top_level_known = self.collect_top_level_known(&statements);
            self.lower_top_level_program(&statements);
            for stmt in statements {
                self.probe_function_stmt(stmt, false);
            }
            self.current_namespace = None;
        }
    }

    fn collect_top_level_known(
        &self,
        statements: &[StmtId],
    ) -> FxHashMap<Name, LoweredTopLevelBinding> {
        let mut known = top_level_known_with_runtime_bindings(self.script_arguments_type());
        self.attach_runtime_bindings(&mut known);
        for stmt in statements {
            self.record_top_level_binding(*stmt, &mut known);
        }
        known
    }

    fn attach_runtime_bindings(&self, known: &mut FxHashMap<Name, LoweredTopLevelBinding>) {
        if let Some(args) = known.get_mut(&Name::intern("args")) {
            args.binding_identity = Some(self.script_args_binding_identity());
            args.checked = self
                .binding_expression_type(BindingId::args())
                .or_else(|| args.checked.clone());
        }
    }

    fn script_arguments_type(&self) -> Type {
        script_arguments_type(self.program, &self.scope, self.declarations)
    }

    fn append_immutable_top_level_captures(&self, slots: &mut SlotScope, reads: Option<&std::collections::BTreeSet<BindingId>>) -> LoweredTopLevelSlots {
        let mut bindings = self.bodies.resolved_bindings.declarations.iter()
            .filter(|(id, declaration)| {
                reads.is_none_or(|reads| reads.contains(id)) && (**id == BindingId::args() || (declaration.global && declaration.namespace == self.current_namespace))
                    && slots.resolve_binding(**id).is_none()
                    && !self.declarations.prepared_constants.binding_values.contains_key(id)
            })
            .filter_map(|(&id, declaration)| {
                let binding = self.top_level_known.get(&declaration.name)?;
                if binding
                    .binding_identity
                    .is_none_or(|identity| identity.id != id)
                {
                    return None;
                }
                let checked_type = binding.checked.clone();
                if id == BindingId::args() && checked_type.is_none() {
                    return None;
                }
                binding.slot.then_some((
                    id,
                    declaration.name,
                    binding.kind,
                    binding.mutable,
                    self.binding_identity(id)?,
                    checked_type,
                ))
            })
            .collect::<Vec<_>>();
        bindings.sort_unstable_by_key(|(_, name, _, _, _, _)| *name);
        let mut captures: LoweredTopLevelSlots = Default::default();
        for (binding, name, kind, mutable, binding_identity, checked_type) in bindings {
            let slot = slots.reserve("captured global");
            slots.bind_source_at(binding, name, slot, checked_type.clone());
            captures.push(LoweredTopLevelSlot {
                name,
                slot,
                kind,
                checked_type,
                binding_identity: Some(binding_identity),
                runtime_identity: None,
                mutable,
            });
        }
        captures
    }

    fn lower_top_level_program(&mut self, statements: &[StmtId]) {
        self.lower_program_statements(statements);
    }

    fn lower_program_statements(&mut self, statements: &[StmtId]) -> ProgramBuild {
        let mut known = top_level_known_with_runtime_bindings(self.script_arguments_type());
        self.attach_runtime_bindings(&mut known);
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

    fn probe_function_stmt(&mut self, id: StmtId, exported: bool) {
        match self.program.arena.stmt(id).kind {
            ArenaStmtKind::Export(inner) => self.probe_function_stmt(inner, true),
            ArenaStmtKind::PureDef(def) => {
                self.output.functions += 1;
                let previous_known = std::mem::replace(
                    &mut self.top_level_known,
                    compact_function_top_level_known(
                        self.program,
                        &self.scope,
                        self.declarations,
                        self.qualified_schemes,
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
                    exported,
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
                        &self.scope,
                        self.declarations,
                        self.qualified_schemes,
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
                    exported,
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
        match self.lower_function_with_blocker(function.id) {
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
                    exported: function.exported,
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
                    exported: function.exported,
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
                    if let Some(annotation) = param.annotation
                        && lowered_arena_type(
                            annotation,
                            self.declarations,
                            self.current_namespace,
                        )
                        .is_none()
                    {
                        let tag = self.program.arena.type_expr_tags[annotation.index()];
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
    ) -> Result<FunctionBuild, CompactFunctionBlocker> {
        self.last_blocker_detail = None;
        let def = self.program.arena.function_def(id);
        let body_span = self
            .program
            .arena
            .span(self.program.arena.block(def.body).span);
        let scheme_body_facts = self.declarations.scheme_bodies.get(&body_span).cloned();
        let scheme = scheme_body_facts
            .as_ref()
            .map(|facts| Arc::clone(facts.scheme()));
        let inferred_kind = self
            .declarations
            .function_return_types
            .get(&body_span)
            .and_then(|ty| {
                if let Type::Result(ok, _) = ty {
                    lowered_checked_type(ok).map(LoweredReturnKind::Result)
                } else if matches!(ty, Type::Optional(inner) if matches!(**inner, Type::Result(_, _)))
                {
                    Some(LoweredReturnKind::OptionalResult)
                } else {
                    lowered_checked_type(ty).map(LoweredReturnKind::Plain)
                }
            });
        let inferred_return_kind = match scheme.as_deref().map(|scheme| scheme.return_type()) {
            Some(scheme_return) => lowered_scheme_return_kind(scheme_return),
            None => inferred_kind.or_else(|| self.lowered_return_kind(def.return_ty)),
        };
        let return_kind = match inferred_return_kind {
            Some(kind) => kind,
            None => {
                let span = if scheme.is_some() {
                    self.program
                        .arena
                        .span(self.program.arena.block(def.body).span)
                } else {
                    self.program.arena.type_expr_span(def.return_ty)
                };
                self.last_blocker_detail = Some((span, "unsupported return representation".to_string()));
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
        if scheme.as_ref().is_some_and(|scheme| {
            scheme.params().len() != self.program.arena.params(def.params).len()
                || scheme
                    .params()
                    .iter()
                    .zip(self.program.arena.params(def.params))
                    .any(|(scheme_param, param)| {
                        scheme_param.name != param.name
                            || scheme_param.defaulted != param.default.is_some()
                            || scheme_param.rest != param.rest
                    })
        }) {
            self.last_blocker_detail = Some((
                self.program
                    .arena
                    .span(self.program.arena.block(def.body).span),
                "generic function scheme does not match its declared parameters".to_string(),
            ));
            return Err(CompactFunctionBlocker::ParamType);
        }
        if scheme.is_some()
            && let Some(parameter) = self
                .program
                .arena
                .params(def.params)
                .iter()
                .find(|parameter| parameter.default.is_some())
        {
            self.last_blocker_detail = Some((
                self.program.arena.span(parameter.span),
                "generic function parameters with defaults are not lowerable".to_string(),
            ));
            return Err(CompactFunctionBlocker::ParamDefault);
        }
        for (index, param) in self.program.arena.params(def.params).iter().enumerate() {
            let scheme_parameter = scheme
                .as_ref()
                .and_then(|scheme| scheme.params().get(index));
            let expected = match scheme_parameter.map(|parameter| &parameter.ty) {
                Some(SchemeType::Ground(ty)) => Some(ty.clone()),
                Some(_) => None,
                None => self
                    .declarations
                    .parameter_types
                    .get(&self.program.arena.span(param.span))
                    .cloned()
                    .or_else(|| {
                        param.annotation.and_then(|annotation| {
                            checked_type_annotation(
                                self.declarations,
                                self.current_namespace,
                                annotation,
                            )
                            .cloned()
                        })
                    }),
            };
            let expected = expected.map(|ty| self.canonicalize_checked_type(ty));
            let kind = match scheme_parameter {
                Some(parameter) => {
                    let Some(kind) = lowered_scheme_parameter_kind(&parameter.ty) else {
                        self.last_blocker_detail = Some((
                            self.program.arena.span(param.span),
                            "generic parameter has no supported runtime representation".to_string(),
                        ));
                        return Err(CompactFunctionBlocker::ParamType);
                    };
                    kind
                }
                None => {
                    let Some(expected) = expected.as_ref() else {
                        return Err(CompactFunctionBlocker::ParamType);
                    };
                    LoweredParamKind::Typed(
                        lowered_checked_type(expected).ok_or(CompactFunctionBlocker::ParamType)?,
                    )
                }
            };
            let checked_kind = match kind {
                LoweredParamKind::Typed(kind) => Some(kind),
                LoweredParamKind::Erased => None,
            };
            let check = match (expected.as_ref(), checked_kind) {
                (Some(expected), Some(_)) if matches!(expected, Type::Callable(_)) => {
                    Some(LoweredTypeCheck {
                        schema: None,
                        ty: expected.clone(),
                        name: Arc::from(expected.to_string()),
                    })
                }
                (Some(expected), Some(kind)) if param.annotation.is_none() => {
                    driver_annotation_requires_validation(kind, expected).then(
                        || LoweredTypeCheck {
                            schema: None,
                            ty: expected.clone(),
                            name: Arc::from(expected.to_string()),
                        },
                    )
                }
                (Some(expected), Some(kind)) => compact_type_check(
                    kind,
                    expected,
                    compact_type_expr_name(
                        &self.program.arena,
                        param.annotation.expect("annotated parameter has a type expression"),
                    ),
                ),
                _ => None,
            };
            let default = if let Some(expr) = param.default {
                let (Some(expected), Some(kind)) = (expected.as_ref(), checked_kind) else {
                    self.last_blocker_detail = Some((
                        self.program.arena.expr(expr).span,
                        "generic parameter defaults require a ground type".to_string(),
                    ));
                    return Err(CompactFunctionBlocker::ParamDefault);
                };
                let prepared = self
                    .declarations
                    .prepared_constants
                    .analyze_expression(&self.program.arena, expr)
                    .map(|constant| constant.in_type(expected))
                    .and_then(|constant| {
                        lower_literal_constant(&constant, Some(&self.declarations.wire_enums))
                    })
                    .filter(|value| lowered_value_matches(kind, value))
                    .or_else(|| {
                        lower_const_param_default(&self.program.arena, expr, kind, Some(expected))
                    });
                if let Some(value) = prepared {
                    self.record_prepared_tag_schemas(&value).ok_or(CompactFunctionBlocker::ParamDefault)?;
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
        let mut slots = SlotScope::default();
        for _ in &params {
            slots.reserve("parameter");
        }
        let captures = self.append_immutable_top_level_captures(&mut slots, scheme_body_facts.as_ref().map(|facts| facts.read_bindings()));
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
            let parameter = &self.program.arena.params(def.params)[slot];
            let binding = self.declaration_binding(self.program.arena.span(parameter.span), name)
                .ok_or(CompactFunctionBlocker::ParamType)?;
            slots.bind_source_at(binding, name, slot, ty);
        }
        let (mut body, scheme_body_types, scheme_tail_types) =
            if let Some(facts) = scheme_body_facts.as_ref() {
                self.lower_scheme_body(def.body, &mut slots, def.name, facts)
                    .ok_or(CompactFunctionBlocker::Body)?
            } else {
                let body = self
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
                (body, BTreeMap::new(), BTreeMap::new())
            };
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
        let body_cannot_fall_through = if scheme_body_facts.is_some() {
            true
        } else {
            let tail_is_value = self
                .program
                .arena
                .stmt_ids(self.program.arena.block(def.body).statements)
                .last()
                .map(|tail| self.program.arena.core_stmt_id(tail))
                .is_some_and(|tail| {
                    self.bodies.statement_positions.get(&tail)
                        != Some(&crate::sema::check::StatementPosition::Statement)
                });
            block_callable_body_cannot_fall_through_arena(
                self.program,
                def.body,
                tail_is_value,
                &self.bodies.expr_types,
                &self.declarations.tag_variants_by_name,
                &self.bodies.conversions,
                &self.bodies.argument_bindings,
                &self.bodies.checked_control_flow,
            )
        };
        if !body_cannot_fall_through {
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
        let mut slot_type_facts = slots.type_facts_by_slot(params.len() + captures.len());
        for ty in slot_type_facts.iter_mut().flatten() {
            *ty = self.canonicalize_checked_type(ty.clone());
        }
        let checked_return_type = if scheme.is_none() {
            self.declarations
                .function_return_types
                .get(&body_span)
                .cloned()
                .or_else(|| {
                    checked_type_annotation(
                        self.declarations,
                        self.current_namespace,
                        def.return_ty,
                    )
                    .cloned()
                })
                .map(|ty| self.canonicalize_checked_type(ty))
        } else {
            None
        };
        let checked_function_signature = scheme.is_none().then(|| {
            self.declarations
                .function_signatures
                .get(&body_span)
                .map(|signature| {
                    self.canonicalize_checked_callable(CallableType {
                        params: signature.params.clone(),
                        return_ty: Box::new(signature.return_ty.clone()),
                        effects: signature.effects.clone(),
                    })
                })
        }).flatten();
        let return_check = checked_return_type
            .clone()
            .filter(Type::has_unsigned_constraint)
            .map(|ty| LoweredTypeCheck {
                name: Arc::from(ty.to_string()),
                ty,
                schema: None,
            });
        Ok(FunctionBuild {
            params,
            param_kinds,
            param_checks,
            param_rest,
            param_defaults,
            captures,
            return_kind,
            return_check,
            checked_return_type,
            checked_function_signature,
            slot_count: slots.count(),
            slot_type_facts,
            body,
            scheme,
            scheme_body_types,
            scheme_tail_types,
            scratch: self.scratch.clone(),
        })
    }

    fn lower_scheme_body(
        &mut self,
        block: crate::syntax::arena::BlockId,
        slots: &mut SlotScope,
        current_function: Name,
        facts: &SchemeBodyFacts,
    ) -> Option<(
        Vec<BuildStmtId>, BTreeMap<BuildExprId, SchemeType>, BTreeMap<BuildStmtId, SchemeType>,
    )> {
        let block_data = self.program.arena.block(block);
        if !block_data.params.is_empty() { return self.block_scheme_lowering(block); }
        let statements = self.program.arena.stmt_ids(block_data.statements).collect::<Vec<_>>();
        let (&tail, prefix) = statements.split_last()?;
        let mut body = Vec::new();
        let mut lowered_types = BTreeMap::new();
        let mut visited = FxHashSet::default();
        for &statement in prefix {
            self.lower_scheme_binding(statement, slots, current_function, facts, &mut visited, &mut lowered_types, &mut body)?;
        }
        let mut tail_types = BTreeMap::new();
        let mut value = self.lower_scheme_tail_value(tail, slots, current_function, facts, &mut visited, &mut lowered_types, &mut body)?;
        if facts.scheme().kind() == crate::sema::types::CallableKind::Proc
            && let SchemeType::Result(ok, _) = facts.scheme().return_type()
            && lowered_types.get(&value) == Some(ok.as_ref())
        {
            value = push_build_row!(self, expr, BuildExprRow::Ok(value));
            lowered_types.insert(value, facts.scheme().return_type().clone());
        }
        let tail_type = if matches!(self.program.arena.stmt(tail).kind, ArenaStmtKind::TailBareIdent(_)) {
            lowered_types.get(&value).cloned()
        } else { None };
        if visited.len() != facts.expression_types().len()
            || !lowered_types.get(&value).is_some_and(|ty| ty.matches_body_return(facts.scheme().return_type()))
        { return self.block_scheme_lowering(block); }
        let returned = push_build_row!(self, stmt, BuildStmtRow::Return { value });
        if let Some(ty) = tail_type { tail_types.insert(returned, ty); }
        body.push(returned);
        Some((body, lowered_types, tail_types))
    }

    fn lower_scheme_tail_value(
        &mut self, statement: StmtId, slots: &mut SlotScope, current_function: Name,
        facts: &SchemeBodyFacts, visited: &mut FxHashSet<ExprId>,
        lowered_types: &mut BTreeMap<BuildExprId, SchemeType>, body: &mut Vec<BuildStmtId>,
    ) -> Option<BuildExprId> {
        match self.program.arena.stmt(statement).kind {
            ArenaStmtKind::TailBareIdent(_) => {
                let ty = facts.tail_types().get(&statement)?.clone();
                let binding = self.bodies.resolved_bindings.tails.get(&statement)?;
                let value = push_build_row!(self, expr, BuildExprRow::Param(slots.resolve_binding(*binding)?));
                lowered_types.insert(value, ty);
                Some(value)
            }
            ArenaStmtKind::Expr(value) | ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(value))) =>
                self.lower_scheme_expression(value, slots, current_function, facts, visited, lowered_types, body),
            ArenaStmtKind::If { branches, else_block: Some(else_block) } => {
                let mut values = Vec::new();
                for branch in self.program.arena.if_branches(branches).to_vec() {
                    let condition = self.lower_scheme_lazy_expression(branch.condition, slots, current_function, facts, visited, lowered_types)?;
                    let value = self.lower_scheme_value_block(branch.block, slots, current_function, facts, visited, lowered_types)?;
                    values.push((condition, value));
                }
                let else_value = self.lower_scheme_value_block(else_block, slots, current_function, facts, visited, lowered_types)?;
                let ty = lowered_types.get(&else_value)?.clone();
                if values.iter().any(|(_, value)| !lowered_types.get(value).is_some_and(|value| value.matches_body_return(&ty))) { return None; }
                let span = self.program.arena.stmt(statement).span;
                let value = push_build_row!(self, expr, BuildExprRow::IfExpr { branches: values, else_value, span });
                lowered_types.insert(value, ty);
                Some(value)
            }
            _ => None,
        }
    }

    fn lower_scheme_value_block(
        &mut self, block: crate::syntax::arena::BlockId, slots: &mut SlotScope, current_function: Name,
        facts: &SchemeBodyFacts, visited: &mut FxHashSet<ExprId>,
        lowered_types: &mut BTreeMap<BuildExprId, SchemeType>,
    ) -> Option<BuildExprId> {
        let saved = slots.enter();
        let block_data = self.program.arena.block(block);
        if !block_data.params.is_empty() { return None; }
        let statements = self.program.arena.stmt_ids(block_data.statements).collect::<Vec<_>>();
        let (&tail, prefix) = statements.split_last()?;
        let mut body = Vec::new();
        for &statement in prefix {
            self.lower_scheme_binding(statement, slots, current_function, facts, visited, lowered_types, &mut body)?;
        }
        let value = self.lower_scheme_tail_value(tail, slots, current_function, facts, visited, lowered_types, &mut body)?;
        let ty = lowered_types.get(&value)?.clone();
        body.push(push_build_row!(self, stmt, BuildStmtRow::Value { value }));
        slots.exit(saved);
        let value = push_build_row!(self, expr, BuildExprRow::ValueBlock { body, span: self.program.arena.span(block_data.span) });
        lowered_types.insert(value, ty);
        Some(value)
    }

    fn lower_scheme_binding(
        &mut self, statement: StmtId, slots: &mut SlotScope, current_function: Name,
        facts: &SchemeBodyFacts, visited: &mut FxHashSet<ExprId>,
        lowered_types: &mut BTreeMap<BuildExprId, SchemeType>, body: &mut Vec<BuildStmtId>,
    ) -> Option<()> {
        let ArenaStmtKind::Let { target, ty: annotation, initializer: ArenaExprOrRun::Expr(expression) } = self.program.arena.stmt(statement).kind else { return None; };
        let ArenaBindingTargetKind::Name(name) = self.program.arena.binding_target(target).kind else { return None; };
        let value = self.lower_scheme_expression(expression, slots, current_function, facts, visited, lowered_types, body)?;
        let binding = self.declaration_binding(self.program.arena.stmt(statement).span, name)?;
        let ty = self.declarations.local_binding_types.get(&self.program.arena.stmt(statement).span).cloned();
        let kind = lowered_scheme_physical_type(lowered_types.get(&value)?);
        let checked = self.lower_binding_storage_value(annotation, ty.as_ref(), kind, value, self.program.arena.stmt(statement).span)?;
        if checked != value {
            let stored = SchemeType::Ground(ty.clone()?);
            self.record_scheme_storage_elaboration(checked, &stored, lowered_types);
        }
        let slot = slots.declare_source(binding, name, ty.clone());
        body.push(push_build_row!(self, stmt, BuildStmtRow::Let { slot, value: checked, annotation: ty }));
        Some(())
    }

    fn lower_scheme_lazy_expression(
        &mut self, expression: ExprId, slots: &mut SlotScope, current_function: Name,
        facts: &SchemeBodyFacts, visited: &mut FxHashSet<ExprId>,
        lowered_types: &mut BTreeMap<BuildExprId, SchemeType>,
    ) -> Option<BuildExprId> {
        let mut body = Vec::new();
        let value = self.lower_scheme_expression(expression, slots, current_function, facts, visited, lowered_types, &mut body)?;
        let ty = lowered_types.get(&value)?.clone();
        body.push(push_build_row!(self, stmt, BuildStmtRow::Value { value }));
        let value = push_build_row!(self, expr, BuildExprRow::ValueBlock { body, span: self.program.arena.expr(expression).span });
        lowered_types.insert(value, ty);
        Some(value)
    }

    fn record_scheme_storage_elaboration(
        &mut self,
        checked: BuildExprId,
        ty: &SchemeType,
        lowered_types: &mut BTreeMap<BuildExprId, SchemeType>,
    ) {
        if let BuildExprRow::Try { value: required, .. } = self.scratch.borrow().expressions[checked.index()] {
            lowered_types.insert(required, SchemeType::Result(Box::new(ty.clone()), Box::new(SchemeType::Ground(Type::Error))));
        }
        lowered_types.insert(checked, ty.clone());
    }

    fn lower_scheme_registered_argument(
        &mut self,
        original: BuildExprId,
        check: &crate::sema::schemes::RegisteredArgumentCheck,
        span: Span,
        slots: &mut SlotScope,
        lowered_types: &mut BTreeMap<BuildExprId, SchemeType>,
        body: &mut Vec<BuildStmtId>,
    ) -> BuildExprId {
        let (checked, ty) = match check {
            crate::sema::schemes::RegisteredArgumentCheck::Unsigned(ty) => (self.checked_unsigned_value(original, ty, span), SchemeType::Ground(ty.clone())),
            crate::sema::schemes::RegisteredArgumentCheck::UIntMapKey => (self.require_uint_key(original, span), SchemeType::Ground(Type::UInt)),
        };
        self.record_scheme_storage_elaboration(checked, &ty, lowered_types);
        let slot = slots.reserve("checked generic body argument");
        body.push(push_build_row!(self, stmt, BuildStmtRow::Let { slot, value: checked, annotation: None }));
        let read = push_build_row!(self, expr, BuildExprRow::Param(slot));
        lowered_types.insert(read, ty);
        read
    }

    fn lower_scheme_expression(
        &mut self,
        expression: ExprId,
        slots: &mut SlotScope,
        current_function: Name,
        facts: &SchemeBodyFacts,
        visited: &mut FxHashSet<ExprId>,
        lowered_types: &mut BTreeMap<BuildExprId, SchemeType>,
        body: &mut Vec<BuildStmtId>,
    ) -> Option<BuildExprId> {
        if !visited.insert(expression) { return self.block_scheme_lowering_expr(expression); }
        let ty = facts.expression_types().get(&expression)?.clone();
        let span = self.program.arena.expr(expression).span;
        let mut registered = None;
        let lowered = match self.program.arena.expr(expression).kind {
            ArenaExprKind::Ident(_) => {
                let binding = self.expression_binding(expression)?;
                let slot = slots.resolve_binding(binding)?;
                push_build_row!(self, expr, BuildExprRow::Param(slot))
            }
            ArenaExprKind::Null | ArenaExprKind::Bool(_) | ArenaExprKind::Int(_) | ArenaExprKind::Float(_) | ArenaExprKind::Str(_) | ArenaExprKind::Bytes(_) | ArenaExprKind::Duration(_) | ArenaExprKind::Regex(_) | ArenaExprKind::PathStr(_) => self.lower_expr(expression, slots, Some(current_function), None)?,
            ArenaExprKind::Field { base, name } => {
                let Some(SchemeType::Record { fields, .. }) = facts.expression_types().get(&base) else { return self.block_scheme_lowering_expr(expression) };
                if fields.get(&name) != Some(&ty) { return self.block_scheme_lowering_expr(expression); }
                let base = self.lower_scheme_expression(base, slots, current_function, facts, visited, lowered_types, body)?;
                push_build_row!(self, expr, BuildExprRow::Field { base, name: name.as_str(), span })
            }
            ArenaExprKind::List(items) => {
                let mut values = Vec::new();
                for item in self.program.arena.list_elements(items).collect::<Vec<_>>() {
                    if item.splice_span.is_some() { return self.block_scheme_lowering_expr(expression); }
                    values.push(self.lower_scheme_expression(item.value, slots, current_function, facts, visited, lowered_types, body)?);
                }
                push_build_row!(self, expr, BuildExprRow::List(values))
            }
            ArenaExprKind::Record(fields) => {
                let mut entries = Vec::new();
                for field in self.program.arena.record_fields(fields).to_vec() {
                    let ArenaRecordFieldKind::Named { name, value, .. } = field.kind else { return self.block_scheme_lowering_expr(expression); };
                    let value = self.lower_scheme_expression(value, slots, current_function, facts, visited, lowered_types, body)?;
                    entries.push(LoweredRecordEntry::Field(name, value));
                }
                push_build_row!(self, expr, BuildExprRow::Record(entries))
            }
            ArenaExprKind::If { branches, else_value } => {
                let mut values = Vec::new();
                for branch in self.program.arena.if_expr_branches(branches).to_vec() {
                    let condition = self.lower_scheme_lazy_expression(branch.condition, slots, current_function, facts, visited, lowered_types)?;
                    let value = self.lower_scheme_lazy_expression(branch.value, slots, current_function, facts, visited, lowered_types)?;
                    values.push((condition, value));
                }
                let else_value = self.lower_scheme_lazy_expression(else_value, slots, current_function, facts, visited, lowered_types)?;
                push_build_row!(self, expr, BuildExprRow::IfExpr { branches: values, else_value, span })
            }
            ArenaExprKind::ValueBlock(block) => self.lower_scheme_value_block(block, slots, current_function, facts, visited, lowered_types)?,
            ArenaExprKind::Call { callee, args } => {
                let plan = facts.registered_calls().get(&expression).cloned();
                let receiver = if plan.as_ref().is_some_and(|plan| plan.receiver.is_some()) {
                    let ArenaExprKind::Field { base, .. } = self.program.arena.expr(callee).kind else { return self.block_scheme_lowering_expr(expression) };
                    Some(self.lower_scheme_expression(base, slots, current_function, facts, visited, lowered_types, body)?)
                } else { None };
                if facts.expression_types().contains_key(&callee) { visited.insert(callee); }
                let arguments = self.program.arena.call_args(args).to_vec();
                let mut values = Vec::with_capacity(arguments.len());
                for (source_slot, argument) in arguments.into_iter().enumerate() {
                    let value = match argument.kind {
                        ArenaCallArgKind::Positional(value) | ArenaCallArgKind::Named { value, .. } => value,
                        _ => return self.block_scheme_lowering_expr(expression),
                    };
                    let mut lowered = self.lower_scheme_expression(value, slots, current_function, facts, visited, lowered_types, body)?;
                    if let Some(plan) = &plan {
                        let slot = *plan.argument_slots.get(source_slot)?;
                        if let Some(check) = plan.argument_checks.get(slot).and_then(Option::as_ref) {
                            lowered = self.lower_scheme_registered_argument(lowered, check, self.program.arena.expr(value).span, slots, lowered_types, body);
                        }
                    }
                    values.push(lowered);
                }
                if let Some(constructor) = self.bodies.checked_control_flow.result_constructors.get(&span).copied() {
                    match constructor {
                        crate::sema::check::CheckedResultConstructor::Ok => {
                            let value = match values.as_slice() {
                                [] => {
                                    let unit = push_build_row!(self, expr, BuildExprRow::Unit);
                                    lowered_types.insert(unit, SchemeType::Ground(Type::Unit));
                                    unit
                                }
                                [value] => *value,
                                _ => return self.block_scheme_lowering_expr(expression),
                            };
                            push_build_row!(self, expr, BuildExprRow::Ok(value))
                        }
                        crate::sema::check::CheckedResultConstructor::Err => {
                            let ordering = self.call_argument_slots(expression)?;
                            let value = values.get(ordering.iter().position(|slot| *slot == 0)?)?;
                            let cause = ordering.iter().position(|slot| *slot == 1).and_then(|index| values.get(index)).copied();
                            push_build_row!(self, expr, BuildExprRow::Err { value: *value, cause })
                        }
                    }
                } else if let Some(plan) = plan {
                    if values.len() != plan.argument_slots.len() { return self.block_scheme_lowering_expr(expression); }
                    let mut ordered = vec![None; plan.signature.params.len()];
                    for (value, &slot) in values.into_iter().zip(plan.argument_slots.iter()) {
                        if ordered.get_mut(slot)?.replace(value).is_some() { return self.block_scheme_lowering_expr(expression); }
                    }
                    while ordered.last() == Some(&None) { ordered.pop(); }
                    let lowered = if let Some(receiver) = receiver {
                        let args = ordered.into_iter().collect::<Option<Vec<_>>>()?;
                        push_build_row!(self, expr, BuildExprRow::Method { receiver, name: crate::symbol::NameText::Dynamic(Arc::clone(&plan.name)), args, span })
                    } else {
                        push_build_row!(self, expr, BuildExprRow::ModuleCall { call_plan: None, op: plan.signature.op, args: ordered, span })
                    };
                    registered = Some(plan);
                    lowered
                } else {
                    let target = *self.bodies.selected_function_calls.get(&expression)?;
                    let function = self.function_index().checked_target_key(target)?;
                    let ordering = self.call_argument_slots(expression).map(<[usize]>::to_vec)
                        .unwrap_or_else(|| (0..values.len()).collect());
                    if ordering.len() != values.len() { return self.block_scheme_lowering_expr(expression); }
                    let mut ordered = vec![None; values.len()];
                    for (value, slot) in values.into_iter().zip(ordering) {
                        if ordered.get_mut(slot)?.replace(LoweredCallArg::Single(value)).is_some() { return self.block_scheme_lowering_expr(expression); }
                    }
                    let lowered = self.lower_checked_function_call_values(function, ordered.into_iter().collect::<Option<Vec<_>>>()?, span, Some(current_function))?;
                    if let Some(instantiation) = self.bodies.call_instantiations.get(&expression).cloned()
                        && !self.scratch.borrow_mut().record_call_instantiation(lowered, instantiation)
                    { return self.block_scheme_lowering_expr(expression); }
                    lowered
                }
            }
            _ => return self.block_scheme_lowering_expr(expression),
        };
        if let Some(plan) = registered { self.scratch.borrow_mut().registered_calls[lowered.index()] = Some(plan); }
        lowered_types.insert(lowered, ty.clone());
        let slot = slots.reserve("generic body expression");
        body.push(push_build_row!(self, stmt, BuildStmtRow::Let { slot, value: lowered, annotation: None }));
        let read = push_build_row!(self, expr, BuildExprRow::Param(slot));
        lowered_types.insert(read, ty);
        Some(read)
    }

    fn block_scheme_lowering(
        &mut self,
        block: crate::syntax::arena::BlockId,
    ) -> Option<(
        Vec<BuildStmtId>,
        BTreeMap<BuildExprId, SchemeType>,
        BTreeMap<BuildStmtId, SchemeType>,
    )> {
        self.last_blocker_detail = Some((
            self.program
                .arena
                .span(self.program.arena.block(block).span),
            "generic body is outside the supported parameter projection path".to_string(),
        ));
        None
    }

    fn block_scheme_lowering_expr(&mut self, expression: ExprId) -> Option<BuildExprId> {
        self.last_blocker_detail = Some((
            self.program.arena.expr(expression).span,
            "generic expression is outside the supported parameter projection path".to_string(),
        ));
        None
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
                    lowered_arena_type(ty, self.declarations, self.current_namespace).is_none()
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
            ArenaStmtKind::TypeDef(_)
            | ArenaStmtKind::ErrorDef(_)
            | ArenaStmtKind::ProcDef(_)
            | ArenaStmtKind::CliMain(_)
            | ArenaStmtKind::PureDef(_)
            | ArenaStmtKind::StreamDef(_)
            | ArenaStmtKind::Return(_)
            | ArenaStmtKind::Yield(_)
            | ArenaStmtKind::YieldDelegate(_)
            | ArenaStmtKind::With { .. }
            | ArenaStmtKind::Loop { .. }
            | ArenaStmtKind::Guard { .. }
            | ArenaStmtKind::Break { .. }
            | ArenaStmtKind::Continue
            | ArenaStmtKind::TailBareIdent(_) => CompactTopLevelBlocker::Other,
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
                    self.qualified_schemes,
                    &self.bodies.resolved_bindings,
                )?;
                let checked_type =
                    self.checked_import_module_type(id, namespace, key.as_ref());
                let module = self.scope.module_with_key(key.as_ref())?;
                let module_statement_ids =
                    self.program.module_statements(module).collect::<Vec<_>>();
                let module_lowered = {
                    let mut probe = CompactLowerConstructProbe {
                        program: self.program,
                        scope: Rc::clone(&self.scope),
                        declarations: self.declarations,
                        qualified_schemes: self.qualified_schemes,
                        bodies: self.bodies,
                        source: self.source,
                        sources: self.sources,
                        current_namespace: Some(module.name),
                        functions: self.functions,
                        top_level_known: compact_top_level_known(
                            self.program,
                            &self.scope,
                            self.declarations,
                            self.qualified_schemes,
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
                        with_error_source_exprs: FxHashMap::default(),
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
                Some(self.lowered_top_level_stmt(BuildTopKind::Use {
                        key,
                        alias: use_stmt.alias,
                        path,
                        namespace,
                        exports,
                        checked_type,
                        binding_identity: self.binding_identity_at(
                            self.program.arena.stmt(id).span,
                            namespace,
                        ),
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
                    let mut slots = top_level_slots(known, &self.bodies.resolved_bindings, self.current_namespace);
                    let checked = self.top_level_binding_checked_type(ty, value);
                    let binding_identities = self
                        .binding_identities_for_target(target)
                        .into_iter()
                        .collect::<Vec<_>>();
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
                    let declared_bindings = binding_identities
                        .into_iter()
                        .map(|identity| {
                            let slot = slots.resolve_binding(identity.id)?;
                            let checked = slots.binding_type(identity.id).cloned();
                            let binding = LoweredTopLevelBinding {
                                kind: checked
                                    .as_ref()
                                    .and_then(lowered_checked_type)
                                    .unwrap_or(LoweredType::Any),
                                checked,
                                binding_identity: Some(identity),
                                mutable: matches!(
                                    self.program.arena.stmt(id).kind,
                                    ArenaStmtKind::Var { .. }
                                ),
                                slot: true,
                            };
                            Some((identity, slot, binding))
                        })
                        .collect::<Option<Vec<_>>>()?;
                    let field_names = declared_bindings
                        .iter()
                        .map(|(identity, slot, _)| (identity.name, *slot))
                        .collect();
                    let binding_identities = declared_bindings
                        .iter()
                        .map(|(identity, slot, _)| (*identity, *slot))
                        .collect();
                    let output_known = declared_bindings.into_iter().fold(
                        known.clone(),
                        |mut output_known, (identity, _, binding)| {
                            output_known.insert(identity.name, binding);
                            output_known
                        },
                    );
                    return Some(self.lowered_top_level_stmt(BuildTopKind::LetRecord {
                            source,
                            fields: field_names,
                            binding_identities,
                            target,
                            mutable: matches!(
                                self.program.arena.stmt(id).kind,
                                ArenaStmtKind::Var { .. }
                            ),
                            span: self.program.arena.stmt(id).span,
                        },
                        &output_known,
                        slots,
                    ));
                }
                let target = simple_binding_target(self.program, target)?;
                if is_discard_name(target) {
                    let mut slots = top_level_slots(known, &self.bodies.resolved_bindings, self.current_namespace);
                    let value = self.lower_expr(value, &mut slots, None, None)?;
                    return Some(self.lowered_top_level_stmt(BuildTopKind::Discard {
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
                let (ty, validation, checked_annotation) = match ty {
                    Some(ty) => {
                        let lowered =
                            lowered_arena_type(ty, self.declarations, self.current_namespace)?;
                        let checked = self.canonical_checked_type_annotation(ty)?;
                        (
                            Some(lowered),
                            compact_type_check(
                                lowered,
                                &checked,
                                compact_type_expr_name(&self.program.arena, ty),
                            ),
                            Some(checked),
                        )
                    }
                    None => (None, None, None),
                };
                let mut slots = top_level_slots(known, &self.bodies.resolved_bindings, self.current_namespace);
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
                Some(self.lowered_top_level_stmt(BuildTopKind::Let {
                        target,
                        ty,
                        validation,
                        annotation: checked_annotation,
                        binding_identity: self.binding_identity_for_simple_target(id),
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
                    let mut slots = top_level_slots(known, &self.bodies.resolved_bindings, self.current_namespace);
                    let value = self.lower_run_binding_value(run, &mut slots, None, None)?;
                    return Some(self.lowered_top_level_stmt(BuildTopKind::Discard {
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
                let (ty, validation, annotation) = match ty {
                    Some(ty) => {
                        let lowered =
                            lowered_arena_type(ty, self.declarations, self.current_namespace)?;
                        let checked = self.canonical_checked_type_annotation(ty)?;
                        (
                            Some(lowered),
                            compact_type_check(
                                lowered,
                                &checked,
                                compact_type_expr_name(&self.program.arena, ty),
                            ),
                            Some(checked),
                        )
                    }
                    None => (None, None, None),
                };
                let mut slots = top_level_slots(known, &self.bodies.resolved_bindings, self.current_namespace);
                let value = self.lower_run_binding_value(run, &mut slots, None, None)?;
                Some(self.lowered_top_level_stmt(BuildTopKind::Let {
                        target,
                        ty,
                        validation,
                        annotation,
                        binding_identity: self.binding_identity_for_simple_target(id),
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
                    let mut slots = top_level_slots(known, &self.bodies.resolved_bindings, self.current_namespace);
                    let lowered = self.lower_stmt_with_blocker_guard(id, &mut slots, None, None)?;
                    return Some(self.lowered_top_level_stmt(BuildTopKind::Stmt(lowered),
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
                    let mut slots = top_level_slots(known, &self.bodies.resolved_bindings, self.current_namespace);
                    let lowered = self.lower_stmt_with_blocker_guard(id, &mut slots, None, None)?;
                    return Some(self.lowered_top_level_stmt(BuildTopKind::Stmt(lowered),
                        known,
                        slots,
                    ));
                }
                let mut slots = top_level_slots(known, &self.bodies.resolved_bindings, self.current_namespace);
                let value = match value {
                    ArenaExprOrRun::Expr(expr) => self.lower_expr(expr, &mut slots, None, None)?,
                    ArenaExprOrRun::Run(run) => {
                        self.lower_run_binding_value(run, &mut slots, None, None)?
                    }
                };
                let span = self.program.arena.stmt(id).span;
                let binding_identity = self
                    .bodies
                    .resolved_bindings
                    .at(span, target)
                    .and_then(|binding| self.binding_identity(binding))?;
                Some(self.lowered_top_level_stmt(BuildTopKind::Assign {
                        target,
                        binding_identity,
                        op,
                        value,
                        span,
                    },
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::Guard { target, .. } => {
                let mut slots = top_level_slots(known, &self.bodies.resolved_bindings, self.current_namespace);
                let outer = slots.indices.clone();
                let span = self.program.arena.stmt(id).span;
                let guard = self.lower_stmt_with_blocker_guard(id, &mut slots, None, None)?;
                let fields = slots
                    .indices
                    .iter()
                    .filter(|(name, slot)| outer.get(*name) != Some(*slot))
                    .map(|(name, slot)| (*name, *slot))
                    .collect();
                let binding_identities: Vec<_> = self
                    .binding_identities_for_target(target)
                    .into_iter()
                    .filter_map(|identity| {
                        slots
                            .indices
                            .get(&identity.name)
                            .copied()
                            .filter(|_| !outer.contains_key(&identity.name))
                            .map(|slot| (identity, slot))
                    })
                    .collect();
                let mut output_known = known.clone();
                for (identity, _) in &binding_identities {
                    let checked = slots.binding_type(identity.id).cloned();
                    output_known.insert(identity.name, LoweredTopLevelBinding {
                        kind: checked.as_ref().and_then(lowered_checked_type)
                            .unwrap_or(LoweredType::Any),
                        checked,
                        binding_identity: Some(*identity),
                        mutable: false,
                        slot: true,
                    });
                }
                let source = push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ValueBlock {
                        body: vec![guard],
                        span
                    }
                );
                Some(self.lowered_top_level_stmt(BuildTopKind::LetRecord {
                        source,
                        fields,
                        binding_identities,
                        target: LoweredCompTarget::Discard,
                        mutable: false,
                        span,
                    },
                    &output_known,
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
                let mut slots = top_level_slots(known, &self.bodies.resolved_bindings, self.current_namespace);
                let lowered = self.lower_stmt_with_blocker_guard(id, &mut slots, None, None)?;
                Some(self.lowered_top_level_stmt(BuildTopKind::Stmt(lowered),
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::Command(command) => {
                let mut slots = top_level_slots(known, &self.bodies.resolved_bindings, self.current_namespace);
                let lowered = self
                    .lower_print_stmt(command, &mut slots, None, None)
                    .or_else(|| self.lower_cd_stmt(command, &mut slots, None, None))
                    .or_else(|| self.lower_env_stmt(command, &mut slots, None, None))
                    .or_else(|| self.lower_run_stmt(command, &mut slots, None, None))
                    .or_else(|| self.lower_proc_stmt(command, &mut slots, None, None))?;
                Some(self.lowered_top_level_stmt(BuildTopKind::Stmt(lowered),
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::Expr(value) => {
                let mut slots = top_level_slots(known, &self.bodies.resolved_bindings, self.current_namespace);
                let value = self.lower_expr(value, &mut slots, None, None)?;
                let kind = BuildTopKind::Expr(value);
                Some(self.lowered_top_level_stmt(kind, known, slots))
            }
            ArenaStmtKind::Exit(status) => {
                let mut slots = top_level_slots(known, &self.bodies.resolved_bindings, self.current_namespace);
                let span = self.program.arena.stmt(id).span;
                let value = self.lower_exit(status, span, &mut slots, None, None)?;
                let kind = BuildTopKind::Expr(value);
                Some(self.lowered_top_level_stmt(kind, known, slots))
            }
            ArenaStmtKind::Defer(ArenaExprOrRun::Expr(value), trigger) => {
                let mut slots = top_level_slots(known, &self.bodies.resolved_bindings, self.current_namespace);
                let value = self.lower_deferred_expr(value, &mut slots, None, None)?;
                Some(self.lowered_top_level_stmt(BuildTopKind::Defer {
                        value,
                        span: self.program.arena.stmt(id).span,
                        on_error: trigger == DeferTrigger::Error,
                    },
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::Defer(ArenaExprOrRun::Run(run), trigger) => {
                let mut slots = top_level_slots(known, &self.bodies.resolved_bindings, self.current_namespace);
                let value = self.lower_run_binding_value(run, &mut slots, None, None)?;
                Some(self.lowered_top_level_stmt(BuildTopKind::Defer {
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
                let mut slot_scope = top_level_slots(known, &self.bodies.resolved_bindings, self.current_namespace);
                let body = self.lower_block(hook.body, &mut slot_scope, None, None)?;
                let slot_count = slot_scope.count();
                let hook_slots: Vec<LoweredTopLevelSlot> = known
                    .iter()
                    .filter_map(|(&name, binding)| {
                        slot_scope.indices.get(&name).copied().map(|slot| LoweredTopLevelSlot {
                            name,
                            slot,
                            kind: binding.kind,
                            checked_type: binding.checked.clone(),
                            binding_identity: binding.binding_identity,
                            runtime_identity: None,
                            mutable: binding.mutable,
                        })
                    })
                    .collect();
                Some(self.lowered_top_level_stmt(BuildTopKind::SignalHook {
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
            ArenaStmtKind::TypeDef(_)
            | ArenaStmtKind::ErrorDef(_)
            | ArenaStmtKind::ProcDef(_)
            | ArenaStmtKind::CliMain(_)
            | ArenaStmtKind::PureDef(_)
            | ArenaStmtKind::StreamDef(_)
            | ArenaStmtKind::Return(_)
            | ArenaStmtKind::Yield(_)
            | ArenaStmtKind::YieldDelegate(_)
            | ArenaStmtKind::Break { .. }
            | ArenaStmtKind::Continue
            | ArenaStmtKind::TailBareIdent(_) => None,
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
                known.insert(
                    namespace,
                    LoweredTopLevelBinding {
                        kind: LoweredType::Module,
                        checked: self.checked_import_module_type(
                            id,
                            namespace,
                            resolved.as_ref(),
                        ),
                        binding_identity: self.binding_identity_at(
                            self.program.arena.stmt(id).span,
                            namespace,
                        ),
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
                                binding_identity: self
                                    .binding_identities_for_target(target)
                                    .into_iter()
                                    .find(|identity| identity.name == name),
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
                            binding_identity: self
                                .binding_identities_for_target(target)
                                .into_iter()
                                .find(|identity| identity.name == name),
                            mutable: false,
                            slot: true,
                        },
                    );
                }
            }
            _ => {}
        }
    }

    fn checked_import_module_type(
        &self,
        statement: StmtId,
        namespace: Name,
        resolved_key: &str,
    ) -> Option<Type> {
        let bindings = &self.bodies.resolved_bindings;
        let binding = bindings
            .declaration_sites
            .get(&(self.program.arena.stmt(statement).span, namespace))?;
        let declaration = bindings.declarations.get(binding)?;
        if declaration.namespace != self.current_namespace
            || !declaration.global
            || !matches!(declaration.kind, BindingKind::Module(key) if key.as_str() == resolved_key)
        {
            return None;
        }
        bindings
            .expressions
            .iter()
            .filter_map(|(expression, resolved)| {
                (*resolved == *binding)
                    .then(|| self.bodies.expr_types.get(expression))
                    .flatten()
            })
            .find(|ty| matches!(ty, Type::Module(_)))
            .cloned()
    }

    fn binding_identity_at(&self, span: Span, name: Name) -> Option<LoweredBindingIdentity> {
        let id = *self
            .bodies
            .resolved_bindings
            .declaration_sites
            .get(&(span, name))?;
        self.binding_identity(id)
    }

    fn binding_identity(&self, id: BindingId) -> Option<LoweredBindingIdentity> {
        let declaration = self.bodies.resolved_bindings.declarations.get(&id)?;
        if id == BindingId::args() {
            return Some(self.script_args_binding_identity());
        }
        Some(LoweredBindingIdentity {
            id,
            name: declaration.name,
            span: declaration.span,
            owner: declaration.namespace,
            built_in: id == BindingId::args(),
            exported: declaration.exported,
        })
    }

    fn script_args_binding_identity(&self) -> LoweredBindingIdentity {
        let id = BindingId::args();
        let span = self
            .bodies
            .resolved_bindings
            .declarations
            .get(&id)
            .map_or_else(
                || Span::at(crate::source::SourceId::new(0), 0),
                |declaration| declaration.span,
            );
        LoweredBindingIdentity {
            id,
            name: Name::intern("args"),
            span,
            owner: None,
            built_in: true,
            exported: false,
        }
    }

    fn binding_expression_type(&self, binding: BindingId) -> Option<Type> {
        self.bodies
            .resolved_bindings
            .expressions
            .iter()
            .find_map(|(expression, resolved)| {
                (*resolved == binding)
                    .then(|| self.bodies.expr_types.get(expression).cloned())
                    .flatten()
            })
    }

    fn binding_identity_for_simple_target(
        &self,
        statement: StmtId,
    ) -> Option<LoweredBindingIdentity> {
        let target = match self.program.arena.stmt(statement).kind {
            ArenaStmtKind::Let { target, .. }
            | ArenaStmtKind::Const { target, .. }
            | ArenaStmtKind::Var { target, .. } => target,
            _ => return None,
        };
        self.binding_identities_for_target(target).into_iter().next()
    }

    fn binding_identities_for_target(
        &self,
        root: crate::syntax::arena::BindingTargetId,
    ) -> Vec<LoweredBindingIdentity> {
        let mut pending = vec![root];
        let mut identities = Vec::new();
        while let Some(target) = pending.pop() {
            match self.program.arena.binding_target(target).kind {
                ArenaBindingTargetKind::Name(_) => {
                    if let Some(identity) = self
                        .bodies
                        .resolved_bindings
                        .target(target)
                        .and_then(|id| self.binding_identity(id))
                    {
                        identities.push(identity);
                    }
                }
                ArenaBindingTargetKind::Record { fields, .. } => {
                    pending.extend(
                        self.program
                            .arena
                            .destructure_fields(fields)
                            .iter()
                            .map(|field| field.target),
                    );
                }
            }
        }
        identities
    }

    /// A top-level binding's representation comes from its annotation, else
    /// from the checker's published binding type.
    fn top_level_binding(
        &self,
        id: StmtId,
        ty: Option<TypeExprId>,
        mutable: bool,
    ) -> LoweredTopLevelBinding {
        let target = match self.program.arena.stmt(id).kind {
            ArenaStmtKind::Let { target, .. }
            | ArenaStmtKind::Const { target, .. }
            | ArenaStmtKind::Var { target, .. } => Some(target),
            _ => None,
        };
        let binding_identity = target
            .and_then(|target| self.binding_identities_for_target(target).into_iter().next());
        if let Some(ty) = ty {
            #[cfg(all(test, debug_assertions))]
            self.note_checked_type_fact_mismatch(id, ty);
            return LoweredTopLevelBinding {
                kind: lowered_arena_type(ty, self.declarations, self.current_namespace)
                    .unwrap_or(LoweredType::Any),
                checked: checked_type_annotation(
                    self.declarations,
                    self.current_namespace,
                    ty,
                )
                .cloned(),
                binding_identity,
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
            binding_identity,
            mutable,
            slot: true,
        }
    }

    #[cfg(all(test, debug_assertions))]
    fn note_checked_type_fact_mismatch(&self, id: StmtId, ty: TypeExprId) {
        let span = self.program.arena.stmt(id).span;
        let kind = lowered_arena_type(ty, self.declarations, self.current_namespace);
        if let Some(annotated) =
            checked_type_annotation(self.declarations, self.current_namespace, ty)
        {
            note_checked_type_fact_mismatch(
                span,
                kind,
                annotated,
                self.declarations.local_binding_types.get(&span),
            );
        }
    }

    fn top_level_binding_checked_type(
        &self,
        ty: Option<TypeExprId>,
        value: ExprId,
    ) -> Option<Type> {
        match ty {
            Some(ty) => checked_type_annotation(self.declarations, self.current_namespace, ty)
                .cloned(),
            None => self.concrete_checked_type(value),
        }
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
        let statement_kind = self.program.arena.stmt(id).kind;
        let transfer_site = if self.program.arena.stmt_is_synthetic(id) {
            None
        } else {
            match statement_kind {
                ArenaStmtKind::Return(_) => Some(ResourceTransferSite::Return(id)),
                ArenaStmtKind::Break { .. } => Some(ResourceTransferSite::Break(id)),
                ArenaStmtKind::Yield(_) | ArenaStmtKind::YieldDelegate(_) => {
                    Some(ResourceTransferSite::Yield(id))
                }
                ArenaStmtKind::Assign { target, .. }
                    if self.assign_target_root_name(target).is_some() =>
                {
                    Some(ResourceTransferSite::Assignment(id))
                }
                _ => None,
            }
        };
        let transfer_fact = match transfer_site {
            Some(site) => Some(self.require_resource_transfer_fact(id, site)?),
            None => None,
        };
        self.output.statements += 1;
        let lowered = match statement_kind {
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
                #[cfg(all(test, debug_assertions))]
                if let Some(ty) = ty {
                    self.note_checked_type_fact_mismatch(id, ty);
                }
                let binding_ty = self
                    .declarations
                    .local_binding_types
                    .get(&self.program.arena.stmt(id).span)
                    .cloned()
                    .or_else(|| self.lower_binding_checked_type(ty, value));
                if let Some(ty) = ty
                    && lowered_arena_type(ty, self.declarations, self.current_namespace).is_none()
                    && !matches!(binding_ty, Some(ref ty) if !matches!(ty, Type::Unknown | Type::Invalid))
                {
                    return None;
                }
                let annotation = match ty {
                    Some(ty) => Some(self.canonical_checked_type_annotation(ty)?),
                    None => None,
                };
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
                let slot = slots.declare_source(self.bodies.resolved_bindings.target(target)?, name, binding_ty);
                if annotation.is_none()
                    && binding_is_int
                    && let Some(value) = self.lower_int_expr_candidate(&value)
                    && !self.lowered_int_expr_needs_type_context(&value)
                {
                    Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::LetInt { slot, value }
                    ))
                } else if annotation.is_none()
                    && let Some(value) = self.lower_bool_expr_candidate(&value)
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
                        BuildStmtRow::Let {
                            slot,
                            value,
                            annotation,
                        }
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
                if let Some(ty) = ty {
                    lowered_arena_type(ty, self.declarations, self.current_namespace)?;
                }
                let annotation = match ty {
                    Some(ty) => Some(self.canonical_checked_type_annotation(ty)?),
                    None => None,
                };
                let value =
                    self.lower_run_binding_value(run, slots, current_function, item_slot)?;
                let binding_ty = match ty {
                    Some(ty) => Some(self.canonical_checked_type_annotation(ty)?),
                    None => {
                        self.declarations
                            .local_binding_types
                            .get(&self.program.arena.stmt(id).span)
                            .cloned()
                    }
                };
                let slot = slots.declare_source(self.bodies.resolved_bindings.target(target)?, name, binding_ty);
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Let {
                        slot,
                        value,
                        annotation,
                    }
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
                let binding = self.bodies.resolved_bindings.at(self.program.arena.stmt(id).span, root_name)?;
                let slot = slots.resolve_binding(binding)?;
                let check = self
                    .assign_target_checked_type(target, self.program.arena.stmt(id).span, slots)
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
                    Some(match self.program.arena.assign_target(target).kind {
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
                                self.lower_assign_path(target, self.program.arena.stmt(id).span, slots, current_function, item_slot)?;
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
                    })
                } else {
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
                            self.lower_assign_path(target, self.program.arena.stmt(id).span, slots, current_function, item_slot)?;
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
                let slot = if name.as_str() == "_" {
                    slots.reserve("for discard")
                } else {
                    slots.declare_source(self.bodies.resolved_bindings.target(target)?, name, item_ty)
                };
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
                        let source_exprs = with_error_source_exprs(
                            self.program,
                            binding.initializer,
                        )
                        .into_iter()
                        .filter(|source_expr| {
                            matches!(
                                self.bodies.expr_types.get(source_expr),
                                Some(Type::Result(_, _) | Type::Any | Type::Unknown)
                            )
                        })
                        .collect::<Vec<_>>();
                        for source_expr in &source_exprs {
                            let Some(source_type) = self.bodies.expr_types.get(source_expr) else {
                                self.last_blocker_detail = Some((
                                    self.program.arena.expr(*source_expr).span,
                                    "missing checked type for With error source".to_string(),
                                ));
                                self.output.blocker_events += 1;
                                return None;
                            };
                            let source_error = match source_type {
                                Type::Result(_, error) => Some(error.as_ref().clone()),
                                Type::Unknown | Type::Any => Some(source_type.clone()),
                                _ => None,
                            };
                            if let Some(source_error) = source_error {
                                error_ty = Some(match error_ty {
                                    None => source_error,
                                    Some(previous) if previous == source_error => previous,
                                    Some(_) => Type::Error,
                                });
                            }
                        }
                        for source_expr in &source_exprs {
                            *self
                                .with_error_source_exprs
                                .entry(*source_expr)
                                .or_default() += 1;
                        }
                        let lowered_value = self.lower_producer_input_expr(
                            binding.initializer,
                            slots,
                            current_function,
                            item_slot,
                        );
                        for source_expr in source_exprs {
                            if let Some(count) = self.with_error_source_exprs.get_mut(&source_expr) {
                                *count -= 1;
                                if *count == 0 {
                                    self.with_error_source_exprs.remove(&source_expr);
                                }
                            }
                        }
                        let value = lowered_value?;
                        let ty = checked.map(|ty| match ty {
                            Type::Result(ok, _) => *ok,
                            other => other,
                        });
                        let slot = if binding.name.as_str() == "_" {
                            slots.reserve("with discard")
                        } else {
                            self.declare_source_at(self.program.arena.span(binding.span), binding.name, ty, slots)?
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
                let else_param_slot = if let Some(param) = self.program.arena
                    .block_params(self.program.arena.block(else_block).params).first()
                    .filter(|param| param.name.as_str() != "_")
                {
                    Some(self.declare_source_at(self.program.arena.span(param.span), param.name, error_ty, slots)?)
                } else { None };
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
                        self.lower_producer_input_expr(expr, slots, current_function, item_slot)?
                    }
                    ArenaExprOrRun::Run(run) => {
                        self.lower_run_binding_value(run, slots, current_function, item_slot)?
                    }
                };
                // The else block runs in its own scope with the error param
                // bound; the success binding lives in the enclosing scope.
                let saved = slots.enter();
                // A null optional carries no error to bind.
                let else_param_slot = if let Some(param) = self.program.arena
                    .block_params(self.program.arena.block(else_block).params).first()
                    .filter(|param| !optional && param.name.as_str() != "_")
                {
                    let error_ty = checked.as_ref().and_then(|ty| match ty {
                        Type::Result(_, error) => Some((**error).clone()),
                        _ => None,
                    });
                    Some(self.declare_source_at(self.program.arena.span(param.span), param.name, error_ty, slots)?)
                } else { None };
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
            ArenaStmtKind::Use(_)
            | ArenaStmtKind::Export(_)
            | ArenaStmtKind::TypeDef(_)
            | ArenaStmtKind::ErrorDef(_)
            | ArenaStmtKind::ProcDef(_)
            | ArenaStmtKind::CliMain(_)
            | ArenaStmtKind::PureDef(_)
            | ArenaStmtKind::StreamDef(_)
            | ArenaStmtKind::SignalHook(_) => None,
        };
        if transfer_fact == Some(true) {
            let lowered = lowered?;
            self.scratch
                .borrow_mut()
                .mark_resource_transfer(lowered);
            self.output.constructed_statements += 1;
            return Some(lowered);
        }
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

    fn require_resource_transfer_fact(
        &mut self,
        stmt: StmtId,
        site: ResourceTransferSite,
    ) -> Option<bool> {
        let Some(fact) = self.bodies.resource_transfer(site) else {
            let span = self.program.arena.stmt(stmt).span;
            self.record_lower_stmt_blocker(stmt);
            self.output.constructed_statements += 1;
            self.output.blocker_events += 1;
            self.last_blocker_detail = Some((
                span,
                format!("missing checked resource-transfer fact for {site:?}"),
            ));
            return None;
        };
        Some(fact)
    }

    fn lower_expr(
        &mut self,
        id: ExprId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let previous_detail = self.last_blocker_detail.take();
        let result = self.lower_expr_attempt(id, slots, current_function, item_slot);
        if result.is_some() && self.last_blocker_detail.is_none() {
            self.last_blocker_detail = previous_detail;
        }
        result
    }

    fn record_prepared_tag_schemas(&self, value: &LoweredValue) -> Option<()> {
        let demand = *self.scratch.borrow_mut().producer_demand.get_or_insert_with(|| super::ProducerDemand::for_declarations(self.program, self.declarations));
        if demand == super::ProducerDemand::Absent { return Some(()); }
        self.record_checked_prepared_tag_schemas(value)
    }

    fn lower_prepared_constant(&self, value: super::PreparedConstantValue, ty: Option<&Type>) -> Option<BuildExprId> {
        if ty.is_some() {
            self.record_checked_prepared_tag_schemas(&value.0)?;
        } else {
            self.record_prepared_tag_schemas(&value.0)?;
        }
        let lowered = push_build_row!(self, expr, BuildExprRow::PreparedConstant(value));
        if let Some(ty) = ty {
            let ty = self.canonicalize_checked_type(ty.clone());
            if !self.scratch.borrow_mut().record_checked_expr_type(lowered, ty, false) { return None; }
        }
        Some(lowered)
    }

    fn record_checked_prepared_tag_schemas(&self, value: &LoweredValue) -> Option<()> {
        let mut pending = vec![value];
        while let Some(value) = pending.pop() {
            match value {
                LoweredValue::Tag(value) => {
                    let key = super::CheckedPatternVariantKey { kind: super::CheckedPatternVariantKind::Tag, owner: value.type_name, variant: Name::intern(&value.name) };
                    let fields = super::indexed::full::FullBuilder::declared_pattern_fields(self.declarations, key)?;
                    if !self.scratch.borrow_mut().record_checked_pattern_variant(key, fields) { return None; }
                    pending.extend(&value.fields);
                }
                LoweredValue::Record(fields) | LoweredValue::Module(fields) => pending.extend(fields.values()),
                LoweredValue::RecordVec(fields) => pending.extend(fields.iter().map(|(_, value)| value)),
                LoweredValue::List(values) => pending.extend(values),
                LoweredValue::SharedList(values) => pending.extend(values.iter()),
                LoweredValue::Map(values) => pending.extend(values.values()),
                LoweredValue::ResultOk(value) => pending.push(value),
                _ => {}
            }
        }
        Some(())
    }

    fn lower_producer_input_expr(
        &mut self,
        id: ExprId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let lowered = self.lower_expr(id, slots, current_function, item_slot)?;
        let demand = *self.scratch.borrow_mut().producer_demand.get_or_insert_with(|| super::ProducerDemand::for_declarations(self.program, self.declarations));
        if demand == super::ProducerDemand::Needed
            && let Some(ty) = self.bodies.expr_types.get(&id).cloned()
            && (compact_checked_type_is_concrete(&ty) && !ty.contains_recovery())
            && !self.scratch.borrow_mut().record_producer_context(lowered, self.canonicalize_checked_type(ty))
        {
            return None;
        }
        Some(lowered)
    }

    fn record_lowered_producer_inputs(&mut self, id: ExprId, lowered: BuildExprId) -> Option<()> {
        let demand = *self.scratch.borrow_mut().producer_demand.get_or_insert_with(|| super::ProducerDemand::for_declarations(self.program, self.declarations));
        if demand == super::ProducerDemand::Absent { return Some(()); }
        let source = match self.program.arena.expr(id).kind {
            ArenaExprKind::Call { callee, .. } => match self.program.arena.expr(callee).kind {
                ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => Some(base),
                _ => None,
            },
            ArenaExprKind::Field { base, .. } | ArenaExprKind::Index { base, .. } | ArenaExprKind::Slice { base, .. } => Some(base),
            ArenaExprKind::Try(value) | ArenaExprKind::Match { value, .. } | ArenaExprKind::PatternTest { value, .. } => Some(value),
            ArenaExprKind::Pipeline { input, .. } | ArenaExprKind::StructuredPipeline { input, .. } => Some(input),
            ArenaExprKind::Binary { op: BinaryOp::ResultFallback, left, .. } => Some(left),
            _ => None,
        };
        let Some(source) = source else { return Some(()) };
        let input = match self.scratch.borrow().expressions.get(lowered.index())? {
            BuildExprRow::Field { base, .. } | BuildExprRow::Index { base, .. } | BuildExprRow::Slice { base, .. } | BuildExprRow::IndexFromEnd { base, .. } => Some(*base),
            BuildExprRow::Method { receiver, .. } | BuildExprRow::StrByteLen { receiver, .. } | BuildExprRow::StrByteAt { receiver, .. } | BuildExprRow::StrPredicate { receiver, .. } => Some(*receiver),
            BuildExprRow::Try { value, .. } | BuildExprRow::MatchExpr { value, .. } | BuildExprRow::StrMatchExpr { value, .. } | BuildExprRow::TagMatchExpr { value, .. } => Some(*value),
            BuildExprRow::ListPipeline { input, .. } => Some(*input),
            BuildExprRow::ResultFallback { left, .. } => Some(*left),
            _ => None,
        };
        if let Some(input) = input && let Some(ty) = self.bodies.expr_types.get(&source).cloned() && (compact_checked_type_is_concrete(&ty) && !ty.contains_recovery()) {
            let ty = self.canonicalize_checked_type(ty);
            if !self.scratch.borrow_mut().record_producer_context(input, ty) { return None; }
        }
        Some(())
    }

    fn lower_expr_attempt(
        &mut self,
        id: ExprId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let uses_postfix_receiver = slots.postfix_receivers.contains_key(&id);
        let lowered_result = self.lower_expr_untyped(id, slots, current_function, item_slot);
        let lowered = lowered_result?;
        self.record_lowered_producer_inputs(id, lowered)?;
        let with_error_source = self.with_error_source_exprs.contains_key(&id);
        let checked_uint_literal = self.bodies.expr_types.get(&id).is_some_and(|ty| {
            ty == &Type::UInt
                && matches!(
                    self.scratch.borrow().expressions.get(lowered.index()),
                    Some(BuildExprRow::UInt(_))
                )
        });
        let checked_result_constructor = self.bodies.checked_control_flow.result_constructors
            .get(&self.program.arena.expr(id).span).copied();
        let Some(ty) = self
            .bodies
            .expr_types
            .get(&id)
            .filter(|ty| {
                ty.contains_typed_callable() || with_error_source || checked_uint_literal
                    || checked_result_constructor.is_some() && !ty.contains_inference() && !ty.contains_recovery() && !ty.contains_any()
            })
            .cloned()
        else {
            return Some(lowered);
        };
        let ty = self.canonicalize_checked_type(ty);
        let contract_producer = if let Some(selected) = checked_result_constructor {
            let scratch = self.scratch.borrow();
            let mut producer = lowered;
            while let Some(BuildExprRow::CheckedValue { value, .. }) = scratch.expressions.get(producer.index()) {
                producer = *value;
            }
            if !matches!((selected, scratch.expressions.get(producer.index())),
                (crate::sema::check::CheckedResultConstructor::Ok, Some(BuildExprRow::Ok(_)))
                | (crate::sema::check::CheckedResultConstructor::Err, Some(BuildExprRow::Err { .. })))
            {
                self.last_blocker_detail = Some((self.program.arena.expr(id).span,
                    "checked Result constructor has no matching physical producer".to_string()));
                return None;
            }
            producer
        } else { lowered };
        if ty.contains_typed_callable() {
            let parameter_slot = {
                let scratch = self.scratch.borrow();
                match scratch.expressions.get(lowered.index()) {
                    Some(BuildExprRow::Param(slot)) => Some(*slot),
                    _ => None,
                }
            };
            let checked_slot_type_conflicts = parameter_slot.is_some_and(|slot| {
                if let Some(binding) = self.expression_binding(id) {
                    if self.source_slot(id, slots) == Some(slot) {
                        !slots.record_checked_binding_type(binding, slot, ty.clone())
                    } else if uses_postfix_receiver {
                        !slots.record_checked_slot_type(slot, ty.clone())
                    } else {
                        true
                    }
                } else {
                    !slots.record_checked_slot_type(slot, ty.clone())
                }
            });
            if checked_slot_type_conflicts {
                self.last_blocker_detail = Some((
                    self.program.arena.expr(id).span,
                    "checked callable expression conflicts with its lowered slot type".to_string(),
                ));
                return None;
            }
        }
        let recorded = self
            .scratch
            .borrow_mut()
            .record_checked_expr_type(contract_producer, ty.clone(), with_error_source);
        if !recorded {
            self.last_blocker_detail = Some((
                self.program.arena.expr(id).span,
                "lowered expression identity has conflicting checked callable types".to_string(),
            ));
            return None;
        }
        Some(lowered)
    }

    fn push_checked_structured_error(
        &mut self,
        call: ExprId,
        error: LoweredErrorExpr,
        span: Span,
    ) -> Option<BuildExprId> {
        let checked = self.bodies.expr_types.get(&call)?.clone();
        let checked = self.canonicalize_checked_type(checked);
        let Type::ErrorVariant {
            family: checked_family,
            variant: checked_variant,
        } = checked
        else {
            self.last_blocker_detail = Some((
                span,
                "structured error constructor has no checked variant identity".to_string(),
            ));
            return None;
        };
        let (family_identity, variant) = match &error {
            LoweredErrorExpr::Structured {
                family_identity,
                variant,
                ..
            } => (family_identity, variant),
        };
        if checked_family != Name::intern(family_identity)
            || checked_variant != Name::intern(variant)
        {
            self.last_blocker_detail = Some((
                span,
                "structured error lowering disagrees with the checked constructor identity"
                    .to_string(),
            ));
            return None;
        }
        let ty = Type::ErrorVariant {
            family: checked_family,
            variant: checked_variant,
        };
        let value = push_build_row!(self, expr, BuildExprRow::Error(Box::new(error)));
        if !self
            .scratch
            .borrow_mut()
            .record_checked_expr_type(value, ty, false)
        {
            self.last_blocker_detail = Some((
                span,
                "structured error producer has conflicting declaration identity".to_string(),
            ));
            return None;
        }
        Some(value)
    }

    fn lower_expr_untyped(
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
                let value = lower_prepared_literal(value, Some(&self.declarations.wire_enums))?;
                self.scratch
                    .borrow_mut()
                    .prepared_constants
                    .insert(origin, value.clone());
                value
            };
            return self.lower_prepared_constant(value, self.declarations.prepared_constants.types.get(&id));
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
                .map(|value| {
                    if self.bodies.expr_types.get(&id) == Some(&Type::UInt) {
                        push_build_row!(self, expr, BuildExprRow::UInt(value))
                    } else {
                        push_build_row!(self, expr, BuildExprRow::Int(value))
                    }
                }),
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
            ArenaExprKind::Ident(name) => self.lower_bare_ident(id, name, slots),
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
                    let ArenaRecordFieldKind::Shorthand { name, span } = field.kind else {
                        return None;
                    };
                    lowered.push(self.lower_bare_ident_at(self.program.arena.span(span), name, slots)?);
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
                    let demand = *self.scratch.borrow_mut().producer_demand.get_or_insert_with(|| super::ProducerDemand::for_declarations(self.program, self.declarations));
                    let start = self.scratch.borrow().expressions.len();
                    let lowered_stage = self.lower_pipeline_stage(
                        stage,
                        slots,
                        current_function,
                        stream_item_type(&input_ty),
                    )?;
                    if demand == super::ProducerDemand::Needed {
                        let output_ty = self.declarations.stream_stage_types.get(&(self.current_namespace, self.program.arena.span(stage.span)))?.output.clone();
                        let inputs = lowered_stage.producer_input_slots();
                        let candidates = self.scratch.borrow().expressions[start..].iter().enumerate().filter_map(|(offset, row)| {
                            let BuildExprRow::Param(slot) = row else { return None };
                            let (_, role) = inputs.iter().find(|(input, _)| input == slot)?;
                            let ty = match role {
                                super::indexed::full::PipelineStageSlotRole::Item => stream_item_type(&input_ty)?.clone(),
                                super::indexed::full::PipelineStageSlotRole::Accumulator => output_ty.clone(),
                                super::indexed::full::PipelineStageSlotRole::ReducedItem { .. } => return None,
                            };
                            Some((BuildExprId::new(start + offset), ty))
                        }).collect::<Vec<_>>();
                        for (value, ty) in candidates {
                            if (compact_checked_type_is_concrete(&ty) && !ty.contains_recovery()) && !self.scratch.borrow_mut().record_producer_context(value, self.canonicalize_checked_type(ty)) { return None; }
                        }
                        if let LoweredPipelineStage::Fold { initial, .. } = &lowered_stage
                            && (compact_checked_type_is_concrete(&output_ty) && !output_ty.contains_recovery())
                            && !self.scratch.borrow_mut().record_producer_context(*initial, self.canonicalize_checked_type(output_ty))
                        { return None; }
                    }
                    lowered_stages.push(lowered_stage);
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
                    call_plan: None,
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
                            call_plan: None,
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
                        let ty = checked_type_annotation(
                            self.declarations,
                            self.current_namespace,
                            target,
                        )?
                        .clone();
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
                    lowered_arena_type(schema, self.declarations, self.current_namespace)?;
                    (
                        checked_type_annotation(
                            self.declarations,
                            self.current_namespace,
                            schema,
                        )?
                        .clone(),
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
                let proven_nonnull = self
                    .bodies
                    .proven_nonnull_fallback_receivers
                    .contains(&left);
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
                            self.declare_source_at(
                                self.program.arena.span(self.program.arena.block_params(self.program.arena.block(block).params)[0].span),
                                parameter, Some(*error_ty), slots)?,
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
                if matches!(left_ty, Some(Type::Optional(_))) || proven_nonnull {
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
                        value: self.lower_producer_input_expr(value, slots, current_function, item_slot)?,
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
                    self.record_tag_pattern_schema(Name::intern(format!("{module}.{name}")))?;
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
                let lowered_base = self.lower_producer_input_expr(base, slots, current_function, item_slot)?;
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Field {
                        base: lowered_base,
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
                    self.lower_producer_input_expr(base, slots, current_function, item_slot)?
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
                        self.lower_producer_input_expr(base, slots, current_function, item_slot)?
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
                self.lower_call(id, callee, args, None, slots, current_function, item_slot)
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
                let mut transfers_resource = false;
                for (&yield_stmt, &collect_expr) in &self.bodies.collect_yields {
                    if collect_expr != id {
                        continue;
                    }
                    let Some(yield_transfer) = self
                        .bodies
                        .resource_transfer(ResourceTransferSite::Yield(yield_stmt))
                    else {
                        self.last_blocker_detail = Some((
                            self.program.arena.stmt(yield_stmt).span,
                            "missing checked resource-transfer fact for collected yield".into(),
                        ));
                        return None;
                    };
                    transfers_resource |= yield_transfer;
                }
                let saved = slots.enter();
                let slot = slots.reserve("collect list");
                slots.collect_slots.insert(id, slot);
                let lowered = (|| {
                    let empty = push_build_row!(self, expr, BuildExprRow::List(Vec::new()));
                    let bind =
                        push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::Let {
                                slot,
                                value: empty,
                                annotation: None,
                            }
                        );
                    let body = self.lower_block(block, slots, current_function, item_slot)?;
                    let body = push_build_row!(self, expr, BuildExprRow::ValueBlock { body, span });
                    let run = push_build_row!(self, stmt, BuildStmtRow::Expr { value: body, span });
                    let list = push_build_row!(self, expr, BuildExprRow::Param(slot));
                    let value = push_build_row!(self, stmt, BuildStmtRow::Value { value: list });
                    if transfers_resource {
                        self.scratch.borrow_mut().mark_resource_transfer(value);
                    }
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
            ArenaExprKind::Binary { .. } => None,
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
                        call_plan: None,
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
                    call_plan: None,
                    op: RuntimeOp::EnvPathList,
                    args: Vec::new(),
                    span,
                }
            )),
            ArenaExprKind::Pipeline { .. } => None,
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
                if self.last_blocker_detail.is_none() {
                    self.last_blocker_detail = Some(blocker_detail);
                }
                self.output.expression_blockers[blocker_index] += 1;
                self.output.constructed_expressions += 1;
                self.output.blocker_events += 1;
                Some(push_build_row!(self, expr, BuildExprRow::Unit))
            }
        }
    }

    fn canonicalize_checked_error_family(&self, family: Name) -> Name {
            let qualified = if let Some((namespace, member)) = family.as_str().rsplit_once('.') {
                self.compact_qualified_function_key(
                    Name::intern(namespace),
                    Name::intern(member),
                )
            } else if let Some(namespace) = self.current_namespace {
                self.compact_qualified_function_key(namespace, family)
            } else {
                return family;
            };
            if self
                .declarations
                .qualified_error_families
                .contains_key(&qualified)
            {
                Name::intern(qualified.to_string())
            } else {
                family
            }
    }

    fn canonicalize_checked_callable(&self, mut signature: CallableType) -> CallableType {
        for parameter in &mut signature.params {
            parameter.ty = self.canonicalize_checked_type(parameter.ty.clone());
        }
        signature.return_ty = Box::new(self.canonicalize_checked_type(*signature.return_ty));
        signature
    }

    fn canonicalize_checked_type(&self, ty: Type) -> Type {
        match ty {
            Type::ErrorFamily(family) => Type::ErrorFamily(
                self.canonicalize_checked_error_family(family),
            ),
            Type::ErrorVariant { family, variant } => Type::ErrorVariant {
                family: self.canonicalize_checked_error_family(family),
                variant,
            },
            Type::ErrorFacet(facet) if facet.as_str().contains('.') => Type::ErrorFacet(
                self.compact_qualified_pattern_facet(facet).unwrap_or(facet),
            ),
            Type::List(item) => Type::List(Box::new(self.canonicalize_checked_type(*item))),
            Type::Map(key, value) => Type::Map(
                Box::new(self.canonicalize_checked_type(*key)),
                Box::new(self.canonicalize_checked_type(*value)),
            ),
            Type::Stream(item) => Type::Stream(Box::new(self.canonicalize_checked_type(*item))),
            Type::Set(item) => Type::Set(Box::new(self.canonicalize_checked_type(*item))),
            Type::Record(fields) => Type::Record(
                fields
                    .iter()
                    .map(|(name, ty)| (*name, self.canonicalize_checked_type(ty.clone())))
                    .collect::<BTreeMap<_, _>>()
                    .into(),
            ),
            Type::Module(module) => Type::Module(Arc::new(crate::sema::types::ModuleType {
                exports: module
                    .exports
                    .iter()
                    .map(|(name, export)| {
                        let export = match export {
                            ModuleExportType::Value { ty, optional } => ModuleExportType::Value {
                                ty: self.canonicalize_checked_type(ty.clone()),
                                optional: *optional,
                            },
                            ModuleExportType::Proc { sig, optional } => ModuleExportType::Proc {
                                sig: self.canonicalize_checked_callable(sig.clone()),
                                optional: *optional,
                            },
                            ModuleExportType::Pure { sig, optional } => ModuleExportType::Pure {
                                sig: self.canonicalize_checked_callable(sig.clone()),
                                optional: *optional,
                            },
                            ModuleExportType::GenericCallable { kind, optional } => {
                                ModuleExportType::GenericCallable { kind: *kind, optional: *optional }
                            }
                        };
                        (*name, export)
                    })
                    .collect(),
                exact: module.exact,
            })),
            Type::Result(ok, error) => Type::Result(
                Box::new(self.canonicalize_checked_type(*ok)),
                Box::new(self.canonicalize_checked_type(*error)),
            ),
            Type::Optional(inner) => {
                Type::Optional(Box::new(self.canonicalize_checked_type(*inner)))
            }
            Type::Union(members) => Type::Union(
                members
                    .into_iter()
                    .map(|member| self.canonicalize_checked_type(member))
                    .collect(),
            ),
            Type::Callable(signature) => {
                let mut callable = (*signature).clone();
                callable.sig = self.canonicalize_checked_callable(callable.sig);
                Type::Callable(Arc::new(callable))
            }
            Type::Validated(validated) => Type::Validated(Box::new(
                crate::sema::validated::ValidatedType::new(
                    validated.validation(),
                    self.canonicalize_checked_type(validated.into_base()),
                )
                .expect("canonicalized validated type keeps its base shape"),
            )),
            other => other,
        }
    }

    fn lowered_top_level_stmt(
        &self,
        kind: BuildTopKind,
        known: &FxHashMap<Name, LoweredTopLevelBinding>,
        slot_indexes: SlotScope,
    ) -> BuildTopStmtId {
        let slot_count = slot_indexes.count();
        let mut slot_type_facts = slot_indexes.type_facts_by_slot(0);
        for ty in slot_type_facts.iter_mut().flatten() {
            *ty = self.canonicalize_checked_type(ty.clone());
        }
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
                    checked_type: binding
                        .checked
                        .clone()
                        .map(|ty| self.canonicalize_checked_type(ty)),
                    binding_identity: binding.binding_identity,
                    runtime_identity: None,
                    mutable: binding.mutable,
                })
            })
            .collect();
        slots.sort_unstable_by_key(|slot| slot.slot);
        self.scratch.borrow_mut().top_stmt(BuildTopStmtRow {
            kind,
            slots,
            slot_count,
            slot_type_facts,
        })
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
        ArenaStmtKind::Const { .. }
        | ArenaStmtKind::Let { .. }
        | ArenaStmtKind::Var { .. }
        | ArenaStmtKind::Assign { .. }
        | ArenaStmtKind::SignalHook(_)
        | ArenaStmtKind::Return(_)
        | ArenaStmtKind::Yield(_)
        | ArenaStmtKind::YieldDelegate(_)
        | ArenaStmtKind::Defer(..)
        | ArenaStmtKind::Expr(_)
        | ArenaStmtKind::If { .. }
        | ArenaStmtKind::While { .. }
        | ArenaStmtKind::For { .. }
        | ArenaStmtKind::With { .. }
        | ArenaStmtKind::Loop { .. }
        | ArenaStmtKind::Guard { .. }
        | ArenaStmtKind::Assert { .. }
        | ArenaStmtKind::Break { .. }
        | ArenaStmtKind::Continue
        | ArenaStmtKind::Exit(_)
        | ArenaStmtKind::Match { .. }
        | ArenaStmtKind::Command(_)
        | ArenaStmtKind::TailBareIdent(_) => false,
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

fn script_arguments_type(
    program: &ArenaProgram,
    scope: &CompactRootScope<'_>,
    declarations: &CompactDeclOutput,
) -> Type {
    let item = if crate::sema::check::byte_argument_main_for_root(
        program,
        scope.root_statements(),
        &declarations.parameter_types,
    ) {
        Type::Bytes
    } else {
        Type::Str
    };
    Type::List(Box::new(item))
}

fn top_level_known_with_runtime_bindings(args_type: Type) -> FxHashMap<Name, LoweredTopLevelBinding> {
    let mut known = FxHashMap::default();
    let args = LoweredTopLevelBinding {
        kind: LoweredType::List,
        checked: Some(args_type),
        binding_identity: None,
        mutable: false,
        slot: true,
    };
    known.insert(Name::intern("args"), args);
    known
}

pub(super) fn top_level_slots(
    known: &FxHashMap<Name, LoweredTopLevelBinding>,
    bindings: &crate::sema::check::ResolvedBindings,
    namespace: Option<Name>,
) -> SlotScope {
    let mut names = known.iter().filter_map(|(name, binding)| binding.slot.then_some(*name)).collect::<Vec<_>>();
    names.sort_unstable();
    let mut slots = SlotScope::default();
    for name in names {
        let slot = slots.reserve("top-level layout");
        slots.indices.insert(name, slot);
    }
    for (&id, declaration) in &bindings.declarations {
        if id != BindingId::args() && (!declaration.global || declaration.namespace != namespace) {
            continue;
        }
        if let Some(&slot) = slots.indices.get(&declaration.name) {
            let binding = &known[&declaration.name];
            let ty = binding.checked.clone().or_else(|| type_for_lowered_type(binding.kind));
            slots.bind_source_at(id, declaration.name, slot, ty);
        }
    }
    slots
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
mod compact_expr_kind_tests {
    use super::{COMPACT_EXPR_KIND_CATALOG, COMPACT_EXPR_KIND_COUNT};

    #[test]
    fn expression_counter_catalog_preserves_existing_indices_and_labels() {
        const EXPECTED: &[(usize, &str)] = &[
            (0, "null"),
            (1, "bool"),
            (2, "int"),
            (3, "float"),
            (4, "duration"),
            (5, "str"),
            (6, "path_str"),
            (7, "glob_str"),
            (8, "fmt_string"),
            (9, "path_fmt_string"),
            (10, "bytes"),
            (11, "ident"),
            (12, "item"),
            (13, "last_status"),
            (14, "list"),
            (15, "list_comp"),
            (16, "map_comp"),
            (17, "record"),
            (18, "if"),
            (19, "match"),
            (19, "pattern_test"),
            (19, "pattern_condition"),
            (20, "unary"),
            (21, "binary"),
            (22, "call"),
            (23, "field"),
            (24, "null_safe_field"),
            (25, "index"),
            (26, "slice"),
            (27, "env_string"),
            (28, "env_path_list"),
            (29, "pipeline"),
            (30, "structured_pipeline"),
            (31, "run"),
            (32, "spawn"),
            (33, "wait"),
            (34, "builder_call"),
            (35, "try"),
            (36, "require"),
            (37, "loop"),
            (38, "retry"),
            (39, "value_block"),
            (40, "comparison-chain"),
            (41, "regex_literal"),
            (42, "try"),
            (43, "value_pipeline_call"),
            (44, "error_context"),
            (45, "context_scope"),
            (46, "tempdir_scope"),
            (47, "convert"),
            (48, "set"),
            (49, "set_comp"),
            (50, "collect"),
            (51, "resource_scope"),
        ];

        assert_eq!(COMPACT_EXPR_KIND_COUNT, 52);
        assert_eq!(COMPACT_EXPR_KIND_CATALOG, EXPECTED);
    }
}

#[cfg(test)]
mod checked_constructor_fact_tests {
    use crate::sema::check::CompactDeclOutput;
    use crate::sema::types::Type;
    use crate::source::{SourceId, SourceMap};
    use crate::syntax::parser::ArenaParseOutput;
    use std::path::PathBuf;

    struct CheckedSource {
        source: String,
        sources: SourceMap,
        parsed: ArenaParseOutput,
        declarations: CompactDeclOutput,
        source_id: SourceId,
    }

    impl CheckedSource {
        fn new(name: &str, source: &str, module_roots: Vec<PathBuf>) -> Self {
            let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                name,
                crate::loader::entry_source_from_text(name, source.to_string()),
                module_roots,
            );
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let source_id = sources.files()[0].id();
            let declarations = crate::sema::check::Checker::check_compact_declarations(
                &parsed.arena,
                &sources,
                source_id,
            );
            assert!(
                declarations.diagnostics.is_empty(),
                "{:?}",
                declarations.diagnostics
            );
            Self {
                source: source.to_string(),
                sources,
                parsed,
                declarations,
                source_id,
            }
        }

        fn expression_containing(&self, fragment: &str) -> crate::syntax::arena::ExprId {
            let matches = (0..self.parsed.arena.arena.expr_tags.len())
                .map(crate::syntax::arena::ExprId::from_index)
                .filter(|&id| {
                    self.sources
                        .span_text(self.parsed.arena.arena.expr(id).span)
                        .is_some_and(|text| text.contains(fragment))
                })
                .collect::<Vec<_>>();
            assert_eq!(matches.len(), 1, "{fragment:?}: {matches:?}");
            matches[0]
        }

        fn build(
            &self,
            declarations: &CompactDeclOutput,
        ) -> Result<(), super::super::indexed::IrBuildError> {
            super::super::FullBuilder::build_compact(
                &self.parsed.arena,
                declarations,
                &self.source,
                std::sync::Arc::new(self.sources.clone()),
                self.source_id,
            )
            .map(|_| ())
        }

        fn assert_top_level_blocker(&self, declarations: &CompactDeclOutput) {
            let result = self.build(declarations);
            assert!(
                matches!(&result, Err(error) if error.construct == "top_level_boundary_blocker"),
                "{result:?}"
            );
        }
    }

    #[test]
    fn missing_constructor_call_type_fact_blocks_lowering() {
        use crate::syntax::arena::ArenaExprKind;

        let name = "missing-constructor-type.xsh";
        let source = "type Box[T] = {value: T}\nlet made = Box(value: 7)\n";
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            name,
            crate::loader::entry_source_from_text(name, source.to_string()),
            Vec::new(),
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = sources.files()[0].id();
        let call = (0..parsed.arena.arena.expr_tags.len())
            .map(crate::syntax::arena::ExprId::from_index)
            .find(|&id| {
                matches!(
                    parsed.arena.arena.expr(id).kind,
                    ArenaExprKind::Call { .. }
                )
            })
            .expect("constructor call");
        let statement = parsed.arena.statement_ids().last().expect("binding");
        let statement_span = parsed.arena.arena.stmt(statement).span;
        let mut declarations = crate::sema::check::Checker::check_compact_declarations(
            &parsed.arena,
            &sources,
            source_id,
        );
        assert!(
            declarations.diagnostics.is_empty(),
            "{:?}",
            declarations.diagnostics
        );
        assert!(declarations.bodies.expr_types.remove(&call).is_some());

        let error = match super::super::FullBuilder::build_compact(
            &parsed.arena,
            &declarations,
            source,
            std::sync::Arc::new(sources),
            source_id,
        ) {
            Err(error) => error,
            Ok(_) => panic!("missing constructor call type fact must block lowering"),
        };
        assert_eq!(error.construct, "top_level_boundary_blocker");
        assert_eq!(error.span, Some(statement_span));
    }

    #[test]
    fn imported_error_annotations_agree_with_local_slot_facts() {
        let work = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
        std::fs::create_dir_all(&work).expect("create repository scratch directory");
        let module_root = tempfile::tempdir_in(&work).expect("create error module fixture");
        std::fs::write(
            module_root.path().join("model.xsh"),
            "##! Error annotation fixture.\n## Keep imported error identity in local bindings.\nexport error ContextError = Left | Right(value: Int)\n",
        )
        .expect("write error module");
        for declaration in ["let", "const", "var"] {
            for (annotation, initializer, pattern) in [
                ("m.ContextError", "m.ContextError.Right(11)", "m.ContextError.Right { value }"),
                ("Result[Unit, m.ContextError]", "Err(m.ContextError.Right(11))", "Err(m.ContextError.Right { value })"),
            ] {
                let source = format!(
                    "use model as m\npure inspect() -> Bool {{ {declaration} failure: {annotation} = {initializer}; match failure {{ {pattern} => value == 11; _ => false }} }}\n",
                );
                if declaration == "const" {
                    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                        "imported-error-annotation.xsh",
                        crate::loader::entry_source_from_text("imported-error-annotation.xsh", source),
                        vec![module_root.path().to_path_buf()],
                    );
                    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                    let declarations = crate::sema::check::Checker::check_compact_declarations(
                        &parsed.arena,
                        &sources,
                        sources.files()[0].id(),
                    );
                    assert_eq!(
                        declarations.diagnostics.iter().map(|diagnostic| diagnostic.code).collect::<Vec<_>>(),
                        [Some(crate::diagnostic::DiagnosticCode::CheckConst)],
                    );
                    continue;
                }
                let checked = CheckedSource::new(
                    "imported-error-annotation.xsh",
                    &source,
                    vec![module_root.path().to_path_buf()],
                );
                let result = checked.build(&checked.declarations);
                assert!(result.is_ok(), "{declaration} {annotation}: {result:?}");
            }
        }
    }

    #[test]
    fn missing_constructor_identity_fact_blocks_lowering() {
        let source = "type Box[T] = {value: T}\nlet made = Box(value: 7)\n";
        let checked = CheckedSource::new("missing-constructor-identity.xsh", source, Vec::new());
        let call = checked.expression_containing("Box(value: 7)");
        assert!(checked.build(&checked.declarations).is_ok());
        assert!(checked.declarations.bodies.expr_types.contains_key(&call));
        let mut declarations = checked.declarations.clone();
        assert!(declarations
            .bodies
            .resolved_type_expressions
            .instantiations
            .remove(&call)
            .is_some());
        checked.assert_top_level_blocker(&declarations);
    }

    #[test]
    fn missing_qualified_constructor_identity_blocks_named_spread_lowering() {
        let work = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
        std::fs::create_dir_all(&work).expect("create repository scratch directory");
        let module_root = tempfile::tempdir_in(&work).expect("create module fixture");
        std::fs::write(
            module_root.path().join("model.xsh"),
            "##! A module for constructor identity tests.\n## An exported record schema.\nexport type Box[T] = {value: T}\n",
        )
        .expect("write model module");
        let source = "use model as m\nlet made = m.Box(...{value: 7})\n";
        let checked = CheckedSource::new(
            "missing-qualified-constructor-identity.xsh",
            source,
            vec![module_root.path().to_path_buf()],
        );
        let call = checked.expression_containing("m.Box(...{value: 7})");
        assert!(checked.build(&checked.declarations).is_ok());
        assert!(checked.declarations.bodies.expr_types.contains_key(&call));
        let mut declarations = checked.declarations.clone();
        assert!(declarations
            .bodies
            .resolved_type_expressions
            .instantiations
            .remove(&call)
            .is_some());
        checked.assert_top_level_blocker(&declarations);
    }

    #[test]
    fn missing_user_function_identity_blocks_lowering() {
        let source = "pure increment(value: Int) -> Int { return value + 1 }\nlet made = increment(7)\n";
        let checked = CheckedSource::new("missing-function-identity.xsh", source, Vec::new());
        let call = checked.expression_containing("increment(7)");
        assert!(checked.build(&checked.declarations).is_ok());
        assert!(checked.declarations.bodies.expr_types.contains_key(&call));
        let mut declarations = checked.declarations.clone();
        assert!(declarations
            .bodies
            .selected_function_calls
            .remove(&call)
            .is_some());
        checked.assert_top_level_blocker(&declarations);
    }

    #[test]
    fn missing_qualified_function_identity_blocks_lowering() {
        let work = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
        std::fs::create_dir_all(&work).expect("create repository scratch directory");
        let module_root = tempfile::tempdir_in(&work).expect("create function module fixture");
        std::fs::write(
            module_root.path().join("model.xsh"),
            "##! A module for function identity tests.\n## An exported pure function.\nexport pure increment(value: Int) -> Int { return value + 1 }\n",
        )
        .expect("write function module");
        let source = "use model as m\nlet made = m.increment(7)\n";
        let checked = CheckedSource::new(
            "missing-qualified-function-identity.xsh",
            source,
            vec![module_root.path().to_path_buf()],
        );
        let call = checked.expression_containing("m.increment(7)");
        assert!(checked.build(&checked.declarations).is_ok());
        assert!(checked
            .declarations
            .bodies
            .statically_resolved_calls
            .contains(&call));
        assert!(checked.declarations.bodies.expr_types.contains_key(&call));
        let mut declarations = checked.declarations.clone();
        assert!(declarations
            .bodies
            .selected_function_calls
            .remove(&call)
            .is_some());
        checked.assert_top_level_blocker(&declarations);
    }

    #[test]
    fn missing_qualified_named_spread_function_identity_blocks_lowering() {
        let work = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
        std::fs::create_dir_all(&work).expect("create repository scratch directory");
        let module_root = tempfile::tempdir_in(&work).expect("create function module fixture");
        std::fs::write(
            module_root.path().join("model.xsh"),
            "##! A module for function identity tests.\n## An exported pure function.\nexport pure increment(value: Int) -> Int { return value + 1 }\n",
        )
        .expect("write function module");
        let source = "use model as m\nlet made = m.increment(...{value: 7})\n";
        let checked = CheckedSource::new(
            "missing-qualified-named-spread-function-identity.xsh",
            source,
            vec![module_root.path().to_path_buf()],
        );
        let call = checked.expression_containing("m.increment(...{value: 7})");
        assert!(checked.build(&checked.declarations).is_ok());
        assert!(checked
            .declarations
            .bodies
            .statically_resolved_calls
            .contains(&call));
        let mut declarations = checked.declarations.clone();
        assert!(declarations
            .bodies
            .selected_function_calls
            .remove(&call)
            .is_some());
        checked.assert_top_level_blocker(&declarations);
    }

    #[test]
    fn missing_stage_callback_identity_blocks_named_stage_lowering() {
        let source = "pure increment(value: Int) -> Int { return value + 1 }\nlet values = [1] |> map(increment)\n";
        let checked = CheckedSource::new("missing-stage-callback-identity.xsh", source, Vec::new());
        let matches = checked
            .declarations
            .bodies
            .selected_function_calls
            .keys()
            .copied()
            .filter(|&id| {
                checked
                    .sources
                    .span_text(checked.parsed.arena.arena.expr(id).span)
                    .is_some_and(|text| text == "increment")
            })
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "{matches:?}");
        let callback = matches[0];
        assert!(checked.build(&checked.declarations).is_ok());
        let mut declarations = checked.declarations.clone();
        assert!(declarations
            .bodies
            .selected_function_calls
            .remove(&callback)
            .is_some());
        checked.assert_top_level_blocker(&declarations);
    }

    #[test]
    fn missing_qualified_stage_callback_identity_blocks_lowering() {
        let work = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
        std::fs::create_dir_all(&work).expect("create repository scratch directory");
        let module_root = tempfile::tempdir_in(&work).expect("create function module fixture");
        std::fs::write(
            module_root.path().join("model.xsh"),
            "##! A module for function identity tests.\n## An exported pure function.\nexport pure increment(value: Int) -> Int { return value + 1 }\n",
        )
        .expect("write function module");
        let source = "use model as m\nlet values = [1] |> map(m.increment)\n";
        let checked = CheckedSource::new(
            "missing-qualified-stage-callback-identity.xsh",
            source,
            vec![module_root.path().to_path_buf()],
        );
        let matches = checked
            .declarations
            .bodies
            .selected_function_calls
            .keys()
            .copied()
            .filter(|&id| {
                checked
                    .sources
                    .span_text(checked.parsed.arena.arena.expr(id).span)
                    .is_some_and(|text| text == "m.increment")
            })
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "{matches:?}");
        let callback = matches[0];
        assert!(checked.build(&checked.declarations).is_ok());
        let mut declarations = checked.declarations.clone();
        assert!(declarations
            .bodies
            .selected_function_calls
            .remove(&callback)
            .is_some());
        checked.assert_top_level_blocker(&declarations);
    }

    #[test]
    fn runtime_module_function_calls_keep_dynamic_lowering() {
        let source = "type Plugin = module { export pure increment(value: Int) -> Int }\nproc invoke(plugin: Plugin) -> Int { return plugin.increment(7) }\n";
        let checked = CheckedSource::new("runtime-module-call.xsh", source, Vec::new());
        let call = checked.expression_containing("plugin.increment(7)");
        assert!(!checked
            .declarations
            .bodies
            .statically_resolved_calls
            .contains(&call));
        assert!(!checked
            .declarations
            .bodies
            .selected_function_calls
            .contains_key(&call));
        assert!(checked.build(&checked.declarations).is_ok());
    }

    #[test]
    fn checked_set_constructor_results_keep_static_builtin_lowering() {
        let source = "let empty: Set[Int] = set.empty()\nlet values: Set[Int] = set.from([1, 2])\n";
        let checked = CheckedSource::new("checked-set-constructors.xsh", source, Vec::new());
        let expected = Type::Set(Box::new(Type::Int));
        for fragment in ["set.empty()", "set.from([1, 2])"] {
            let call = checked.expression_containing(fragment);
            assert_eq!(checked.declarations.bodies.expr_types.get(&call), Some(&expected));
            assert!(checked
                .declarations
                .bodies
                .statically_resolved_calls
                .contains(&call));
        }
        assert!(checked.build(&checked.declarations).is_ok());
    }

    #[test]
    fn unused_script_arguments_keep_the_byte_entry_type() {
        let checked = CheckedSource::new(
            "unused-byte-arguments.xsh",
            "proc main(...argv: List[Bytes]) {}\n",
            Vec::new(),
        );
        let scope = super::CompactRootScope::whole_program(&checked.parsed.arena);
        assert_eq!(
            super::script_arguments_type(
                &checked.parsed.arena,
                &scope,
                &checked.declarations,
            ),
            Type::List(Box::new(Type::Bytes))
        );
    }

    #[test]
    fn default_control_flow_facts_do_not_hide_captured_fallthrough() {
        let name = "incomplete-control-flow-facts.xsh";
        let source = "proc choose() [error] -> Int { let ignored: Result[Unit, Error] = try { let values = range(9223372036854775807).collect(); return values.len() } }\n";
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            name,
            crate::loader::entry_source_from_text(name, source.to_string()),
            Vec::new(),
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = sources.files()[0].id();
        let mut declarations = crate::sema::check::Checker::check_compact_declarations(
            &parsed.arena,
            &sources,
            source_id,
        );
        assert!(
            declarations
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code
                    == Some(crate::diagnostic::DiagnosticCode::CheckMissingReturn)),
            "{:?}",
            declarations.diagnostics
        );
        declarations.bodies.checked_control_flow = Default::default();

        let error = match super::super::FullBuilder::build_compact(
            &parsed.arena,
            &declarations,
            source,
            std::sync::Arc::new(sources),
            source_id,
        ) {
            Err(error) => error,
            Ok(_) => panic!("an incomplete checked-flow carrier cannot prove no fallthrough"),
        };
        assert_eq!(error.construct, "full_ir_function_blocker");
    }

    #[test]
    fn generic_bare_tail_identity_lowers_from_statement_fact() {
        let source = "pure identity(value) { value }\nlet answer = identity(false)\n";
        let checked = CheckedSource::new("generic-bare-tail-identity.xsh", source, Vec::new());
        assert_eq!(checked.declarations.scheme_bodies.len(), 1);
        let facts = checked
            .declarations
            .scheme_bodies
            .values()
            .next()
            .expect("identity scheme body facts");
        assert_eq!(facts.tail_types().len(), 1);
        let call = checked.expression_containing("identity(false)");
        assert!(checked
            .declarations
            .bodies
            .call_instantiations
            .contains_key(&call));
        assert_eq!(
            checked.declarations.bodies.expr_types.get(&call),
            Some(&Type::Bool)
        );
        let build = checked.build(&checked.declarations);
        assert!(build.is_ok(), "{build:?}");
    }

    #[test]
    fn generic_required_field_chain_lowers_from_expression_facts() {
        let source = concat!(
            "pure nested_name(value) { value.person.name }\n",
            "let answer = nested_name({person: {name: \"Ada\"}})\n",
        );
        let checked = CheckedSource::new("generic-required-field-chain.xsh", source, Vec::new());
        assert_eq!(checked.declarations.scheme_bodies.len(), 1);
        let facts = checked
            .declarations
            .scheme_bodies
            .values()
            .next()
            .expect("nested name scheme body facts");
        assert!(facts.expression_types().len() >= 3);
        let build = checked.build(&checked.declarations);
        assert!(build.is_ok(), "{build:?}");
    }

    #[test]
    fn generic_rest_calls_keep_checked_rest_slots() {
        let source = concat!(
            "pure head(prefix, ...items: List[Int]) { prefix }\n",
            "pure reordered_head(prefix, marker, ...items: List[Int]) { prefix }\n",
            "let tail: List[Int] = [8, 9]\n",
            "let empty: Int = head(7)\n",
            "let many: Int = head(7, 8, 9)\n",
            "let spliced: Int = head(7, @tail)\n",
            "let reordered: Int = reordered_head(marker: 1, prefix: 7, @tail)\n",
            "let spread: Int = reordered_head(...{marker: 1, prefix: 7}, @tail)\n",
        );
        let checked = CheckedSource::new("generic-rest-calls.xsh", source, Vec::new());
        let argument_slots = |fragment: &str| {
            let call = checked.expression_containing(fragment);
            let span = checked.parsed.arena.arena.expr(call).span;
            checked
                .declarations
                .bodies
                .argument_bindings
                .get(&span)
                .map(|binding| binding.argument_slots.clone())
        };
        assert_eq!(argument_slots("head(7)"), None);
        assert_eq!(argument_slots("head(7, 8, 9)"), None);
        assert_eq!(argument_slots("head(7, @tail)"), None);
        assert_eq!(
            argument_slots("reordered_head(marker: 1, prefix: 7, @tail)"),
            Some(vec![1, 0, 2])
        );
        assert_eq!(
            argument_slots("reordered_head(...{marker: 1, prefix: 7}, @tail)"),
            Some(vec![1, 0, 2])
        );
        let build = checked.build(&checked.declarations);
        assert!(build.is_ok(), "{build:?}");
    }

    #[test]
    fn generic_rest_splice_with_a_pre_rest_slot_blocks_in_lowering() {
        let source = concat!(
            "pure head(prefix, ...items: List[Int]) { prefix }\n",
            "pure caller(tail: List[Int]) -> Int { head(prefix: 7, @tail) }\n",
        );
        let checked = CheckedSource::new("generic-rest-pre-rest-splice.xsh", source, Vec::new());
        let valid_build = checked.build(&checked.declarations);
        assert!(valid_build.is_ok(), "{valid_build:?}");
        let call = checked.expression_containing("head(prefix: 7, @tail)");
        let crate::syntax::arena::ArenaExprKind::Call { args: call_args, .. } = checked.parsed.arena.arena.expr(call).kind else {
            panic!("the fixture expression is a call")
        };
        let splice_span = checked
            .parsed
            .arena
            .arena
            .call_args(call_args)
            .iter()
            .find_map(|argument| match argument.kind {
                crate::syntax::arena::ArenaCallArgKind::Splice { span, .. } => {
                    Some(checked.parsed.arena.arena.span(span))
                }
                _ => None,
            })
            .expect("the fixture call contains a positional splice");
        let span = checked.parsed.arena.arena.expr(call).span;
        let mut declarations = checked.declarations.clone();
        let argument_slots = &mut declarations
            .bodies
            .argument_bindings
            .get_mut(&span)
            .expect("checker argument slots")
            .argument_slots;
        assert_eq!(argument_slots, &[0, 1]);
        argument_slots[1] = 0;
        let build = checked.build(&declarations);
        assert!(
            matches!(
                &build,
                Err(error)
                    if error.construct == "full_ir_function_blocker"
                        && error.span == Some(splice_span)
                        && error.detail.as_deref()
                            == Some("generic positional splice must target the final rest parameter")
            ),
            "{build:?}"
        );

        let scope = super::CompactRootScope::whole_program(&checked.parsed.arena);
        let caller = checked
            .parsed
            .arena
            .arena
            .stmt_ids(scope.root_statements())
            .find_map(|statement| match checked.parsed.arena.arena.stmt(statement).kind {
                crate::syntax::arena::ArenaStmtKind::PureDef(function)
                    if checked
                        .parsed
                        .arena
                        .arena
                        .function_def(function)
                        .name
                        .as_str()
                        == "caller" =>
                {
                    Some(function)
                }
                _ => None,
            })
            .expect("the fixture contains its caller function");
        let mut probe = super::CompactLowerConstructProbe {
            program: &checked.parsed.arena,
            scope: std::rc::Rc::new(scope),
            declarations: &declarations,
            qualified_schemes: &declarations.function_schemes,
            bodies: &declarations.bodies,
            source: &checked.source,
            sources: Some(&checked.sources),
            current_namespace: None,
            functions: None,
            top_level_known: super::FxHashMap::default(),
            output: super::CompactLowerConstructProbeOutput::default(),
            last_blocker_detail: Some((splice_span, "stale unrelated detail".to_string())),
            stdlib_linkage: super::StdlibLowerLinkage::Local,
            function_defs: std::rc::Rc::new(std::cell::RefCell::new(None)),
            scratch: std::rc::Rc::new(std::cell::RefCell::new(super::BuildScratch::default())),
            with_error_source_exprs: super::FxHashMap::default(),
        };
        checked.parsed.arena.symbol_owner().with_current(|| {
            assert!(probe.lower_function_with_blocker(caller).is_err());
            assert_eq!(
                probe
                    .last_blocker_detail
                    .as_ref()
                    .map(|(_, detail)| detail.as_str()),
                Some("generic positional splice must target the final rest parameter")
            );
        });
    }

    #[test]
    fn generic_constructed_list_body_lowers_from_shared_scheme_facts() {
        let source = "pure wrap(value) { [value] }\nlet first = wrap(42)\nlet second = wrap(\"demo\")\n";
        let checked = CheckedSource::new("generic-list-body.xsh", source, Vec::new());
        assert_eq!(checked.declarations.scheme_bodies.len(), 1);
        assert!(checked.build(&checked.declarations).is_ok());
    }

    #[test]
    fn generic_constructed_result_body_lowers_from_shared_scheme_facts() {
        let source = "pure as_result(value) { Ok(value) }\nlet first = as_result(42)\nlet second = as_result(\"demo\")\n";
        let checked = CheckedSource::new("generic-result-body.xsh", source, Vec::new());
        assert_eq!(checked.declarations.scheme_bodies.len(), 1);
        assert!(checked.build(&checked.declarations).is_ok());
    }

    #[test]
    fn generic_parameter_default_remains_a_lowering_blocker() {
        let source = concat!(
            "pure defaulted(value, flag = false) { value }\n",
            "let answer = defaulted(false)\n",
        );
        let checked = CheckedSource::new("generic-default-blocker.xsh", source, Vec::new());
        assert_eq!(checked.declarations.scheme_bodies.len(), 1);
        let output = super::probe_compact_lower_constructed_bodies(
            &checked.parsed.arena,
            &checked.declarations,
            &checked.source,
        );
        assert_eq!(output.functions, 1);
        assert_eq!(output.constructed_functions, 0);
        assert_eq!(output.function_blockers[1], 1);
    }
}

#[cfg(test)]
mod checked_control_flow_fact_tests {
    use crate::sema::check::{CheckedControlFlowFacts, CompactDeclOutput};
    use crate::source::{SourceId, SourceMap, Span};
    use crate::syntax::parser::ArenaParseOutput;
    use std::collections::BTreeSet;

    #[derive(Clone, Copy, Debug)]
    enum FactCategory {
        BuiltinRange,
        CollectionCardinality,
        PropagatingError,
        LatentStreamError,
        FalliblePipelineStage,
        PropagatingStageCallback,
        ExhaustiveMatch,
    }

    impl FactCategory {
        fn spans(self, facts: &CheckedControlFlowFacts) -> &BTreeSet<Span> {
            match self {
                Self::BuiltinRange => &facts.builtin_range_calls,
                Self::CollectionCardinality => {
                    &facts.cardinality_preserving_collection_calls
                }
                Self::PropagatingError => &facts.propagating_error_spans,
                Self::LatentStreamError => &facts.latent_stream_error_spans,
                Self::FalliblePipelineStage => &facts.fallible_pipeline_stage_spans,
                Self::PropagatingStageCallback => &facts.propagating_stage_callback_spans,
                Self::ExhaustiveMatch => &facts.exhaustive_match_spans,
            }
        }

        fn spans_mut(self, facts: &mut CheckedControlFlowFacts) -> &mut BTreeSet<Span> {
            match self {
                Self::BuiltinRange => &mut facts.builtin_range_calls,
                Self::CollectionCardinality => {
                    &mut facts.cardinality_preserving_collection_calls
                }
                Self::PropagatingError => &mut facts.propagating_error_spans,
                Self::LatentStreamError => &mut facts.latent_stream_error_spans,
                Self::FalliblePipelineStage => &mut facts.fallible_pipeline_stage_spans,
                Self::PropagatingStageCallback => &mut facts.propagating_stage_callback_spans,
                Self::ExhaustiveMatch => &mut facts.exhaustive_match_spans,
            }
        }
    }

    struct CheckedSource {
        source: String,
        sources: SourceMap,
        parsed: ArenaParseOutput,
        declarations: CompactDeclOutput,
        source_id: SourceId,
    }

    impl CheckedSource {
        fn new(name: &str, source: &str) -> Self {
            let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                name,
                crate::loader::entry_source_from_text(name, source.to_string()),
                Vec::new(),
            );
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let source_id = sources.files()[0].id();
            let declarations = crate::sema::check::Checker::check_compact_declarations(
                &parsed.arena,
                &sources,
                source_id,
            );
            Self {
                source: source.to_string(),
                sources,
                parsed,
                declarations,
                source_id,
            }
        }

        fn build(
            &self,
            declarations: &CompactDeclOutput,
        ) -> Result<(), super::super::indexed::IrBuildError> {
            super::super::FullBuilder::build_compact(
                &self.parsed.arena,
                declarations,
                &self.source,
                std::sync::Arc::new(self.sources.clone()),
                self.source_id,
            )
            .map(|_| ())
        }

        fn fact_owner(&self, category: FactCategory, fragment: &str) -> Span {
            let matches = category
                .spans(&self.declarations.bodies.checked_control_flow)
                .iter()
                .copied()
                .filter(|span| {
                    self.sources
                        .span_text(*span)
                        .is_some_and(|text| text.contains(fragment))
                })
                .collect::<Vec<_>>();
            let available = category
                .spans(&self.declarations.bodies.checked_control_flow)
                .iter()
                .map(|span| (*span, self.sources.span_text(*span)))
                .collect::<Vec<_>>();
            assert_eq!(
                matches.len(),
                1,
                "{category:?} owner {fragment:?}: {matches:?}; available {available:?}"
            );
            matches[0]
        }

        fn unrelated_statement_span(&self) -> Span {
            let statement = (0..self.parsed.arena.arena.stmt_tags.len())
                .map(crate::syntax::arena::StmtId::from_index)
                .map(|id| self.parsed.arena.arena.stmt(id))
                .find(|statement| {
                    self.sources
                        .span_text(statement.span)
                        .is_some_and(|text| text.starts_with("let unrelated = 0"))
                })
                .expect("unrelated statement");
            statement.span
        }

        fn assert_fact_changes_lowering(
            &self,
            category: FactCategory,
            owner_fragment: &str,
            actual_builds: bool,
        ) {
            let owner = self.fact_owner(category, owner_fragment);
            let unrelated = self.unrelated_statement_span();
            assert_ne!(owner, unrelated);
            assert!(
                !category
                    .spans(&self.declarations.bodies.checked_control_flow)
                    .contains(&unrelated)
            );

            let actual = self.build(&self.declarations);
            let mut omitted = self.declarations.clone();
            assert!(category
                .spans_mut(&mut omitted.bodies.checked_control_flow)
                .remove(&owner));
            let mut misowned = self.declarations.clone();
            let spans = category.spans_mut(&mut misowned.bodies.checked_control_flow);
            assert!(spans.remove(&owner));
            assert!(spans.insert(unrelated));

            if actual_builds {
                assert!(actual.is_ok(), "{actual:?}");
                let omitted = self.build(&omitted);
                assert!(
                    matches!(&omitted, Err(error) if error.construct == "full_ir_function_blocker"),
                    "{omitted:?}"
                );
                let misowned = self.build(&misowned);
                assert!(
                    matches!(&misowned, Err(error) if error.construct == "full_ir_function_blocker"),
                    "{misowned:?}"
                );
            } else {
                assert!(
                    matches!(&actual, Err(error) if error.construct == "full_ir_function_blocker"),
                    "{actual:?}"
                );
                let omitted = self.build(&omitted);
                assert!(omitted.is_ok(), "{omitted:?}");
                let misowned = self.build(&misowned);
                assert!(misowned.is_ok(), "{misowned:?}");
            }
        }

        fn assert_fact_mutation_is_rejected_by_verification(
            &self,
            category: FactCategory,
            owner_fragment: &str,
        ) {
            let owner = self.fact_owner(category, owner_fragment);
            let unrelated = self.unrelated_statement_span();
            assert_ne!(owner, unrelated);
            assert!(
                !category
                    .spans(&self.declarations.bodies.checked_control_flow)
                    .contains(&unrelated)
            );

            let actual = self.build(&self.declarations);
            assert!(
                matches!(&actual, Err(error) if error.construct == "full_ir_function_blocker"),
                "{actual:?}"
            );
            let mut omitted = self.declarations.clone();
            assert!(category
                .spans_mut(&mut omitted.bodies.checked_control_flow)
                .remove(&owner));
            let mut misowned = self.declarations.clone();
            let spans = category.spans_mut(&mut misowned.bodies.checked_control_flow);
            assert!(spans.remove(&owner));
            assert!(spans.insert(unrelated));

            for mutated in [&omitted, &misowned] {
                let build = self.build(mutated);
                assert!(
                    matches!(&build, Err(error) if error.construct == "full_ir_verification"),
                    "{build:?}"
                );
            }
        }

        fn assert_no_fact_owner(&self, category: FactCategory, fragment: &str) {
            assert!(
                category
                    .spans(&self.declarations.bodies.checked_control_flow)
                    .iter()
                    .all(|span| self
                        .sources
                        .span_text(*span)
                        .is_none_or(|text| !text.contains(fragment))),
                "unexpected {category:?} owner for {fragment:?}"
            );
        }

        fn assert_missing_return(&self) {
            assert!(
                self.declarations.diagnostics.iter().any(|diagnostic| diagnostic.code
                    == Some(crate::diagnostic::DiagnosticCode::CheckMissingReturn)),
                "{:?}",
                self.declarations.diagnostics
            );
        }

        fn assert_no_errors(&self) {
            assert!(self.declarations.diagnostics.is_empty(), "{:?}", self.declarations.diagnostics);
        }
    }

    fn with_unrelated_statement(source: &str) -> String {
        format!("{source}\npure unrelated() -> Int {{ let unrelated = 0; return unrelated }}\n")
    }

    #[test]
    fn builtin_range_fact_is_owned_by_the_builtin_call() {
        let source = with_unrelated_statement(
            "pure choose() -> Int { for item in range(1) { return item } }",
        );
        let checked = CheckedSource::new("builtin-range-flow.xsh", &source);
        checked.assert_no_errors();
        checked.assert_fact_changes_lowering(FactCategory::BuiltinRange, "range(1)", true);
    }

    #[test]
    fn collection_cardinality_fact_is_owned_by_the_collect_call() {
        let source = with_unrelated_statement(
            "pure choose() -> Int { for item in range(1).collect() { return item } }",
        );
        let checked = CheckedSource::new("collection-cardinality-flow.xsh", &source);
        checked.assert_no_errors();
        checked.assert_fact_changes_lowering(
            FactCategory::CollectionCardinality,
            "range(1).collect()",
            true,
        );
    }

    #[test]
    fn propagating_error_fact_is_owned_by_the_effectful_call() {
        let source = with_unrelated_statement(
            "proc maybe_fail() [error] -> Int { return 1 }\nproc choose() [error] -> Int { with value = maybe_fail() { return value } else { |failure| let ignored = 0 } }",
        );
        let checked = CheckedSource::new("propagating-error-flow.xsh", &source);
        checked.assert_missing_return();
        checked.assert_fact_mutation_is_rejected_by_verification(
            FactCategory::PropagatingError,
            "maybe_fail()",
        );
    }

    #[test]
    fn latent_stream_error_fact_is_owned_by_the_stream_call() {
        let source = with_unrelated_statement(
            "stream failing_rows() [error] -> Stream[Int] { yield 1; let ignored = \"late\".parse_int()? }\nproc choose() [error] -> Int { let ignored: Result[Unit, Error] = try { let values = failing_rows().collect(); return values.len() } }",
        );
        let checked = CheckedSource::new("latent-stream-flow.xsh", &source);
        checked.assert_missing_return();
        checked.assert_fact_changes_lowering(
            FactCategory::LatentStreamError,
            "failing_rows()",
            false,
        );
    }

    #[test]
    fn fallible_pipeline_stage_fact_is_owned_by_the_range_stage() {
        let source = with_unrelated_statement(
            "proc choose() [error] -> Int { let ignored: Result[Unit, Error] = try { let _ = [] |> range(0, 9223372036854775807) |> map { |item| return 1 } } }",
        );
        let checked = CheckedSource::new("fallible-stage-flow.xsh", &source);
        checked.assert_missing_return();
        checked.assert_fact_changes_lowering(
            FactCategory::FalliblePipelineStage,
            "range(0, 9223372036854775807)",
            false,
        );
    }

    #[test]
    fn propagating_stage_callback_fact_is_owned_by_the_tee_stage() {
        let source = with_unrelated_statement(
            "proc maybe_fail(item: Int) -> Result[Unit, Error] { return Ok() }\nproc choose() [error] -> Int { let ignored: Result[Unit, Error] = try { let _ = [1] |> tee(maybe_fail); return 1 } }",
        );
        let checked = CheckedSource::new("stage-callback-flow.xsh", &source);
        checked.assert_missing_return();
        checked.assert_fact_changes_lowering(
            FactCategory::PropagatingStageCallback,
            "tee(maybe_fail)",
            false,
        );
    }

    #[test]
    fn exhaustive_match_fact_is_owned_by_the_match_expression() {
        let source = with_unrelated_statement(
            "pure choose(values: List[Int]) -> Int { let ignored = match values { [] => { return 1 }, [head, ..tail] => { return 2 } } }",
        );
        let checked = CheckedSource::new("exhaustive-match-flow.xsh", &source);
        checked.assert_no_errors();
        checked.assert_fact_changes_lowering(
            FactCategory::ExhaustiveMatch,
            "match values",
            true,
        );
    }

    #[test]
    fn shadowed_range_does_not_publish_the_builtin_cardinality_fact() {
        let source = with_unrelated_statement(
            "pure range(start: Int, end: Int) -> List[Int] { return [] }\npure choose() -> Int { let ignored = range(0, 1) |> map { |item| return item } }",
        );
        let checked = CheckedSource::new("shadowed-range-flow.xsh", &source);
        checked.assert_missing_return();
        checked.assert_no_fact_owner(FactCategory::BuiltinRange, "range(0, 1)");
    }

    #[test]
    fn preserving_collection_fact_keeps_an_empty_source_empty() {
        let source = with_unrelated_statement(
            "pure choose() -> Int { for item in range(0).collect() { return item } }",
        );
        let checked = CheckedSource::new("empty-collection-flow.xsh", &source);
        checked.assert_missing_return();
        checked.fact_owner(FactCategory::CollectionCardinality, "range(0).collect()");
        let build = checked.build(&checked.declarations);
        assert!(
            matches!(&build, Err(error) if error.construct == "full_ir_function_blocker"),
            "{build:?}"
        );
    }

    #[test]
    fn direct_result_return_does_not_publish_a_propagation_fact() {
        let source = with_unrelated_statement(
            "pure maybe() -> Result[Int, Error] { return Ok(1) }\npure choose() -> Int { let ignored = maybe(); return 1 }",
        );
        let checked = CheckedSource::new("direct-result-flow.xsh", &source);
        checked.assert_no_errors();
        checked.assert_no_fact_owner(FactCategory::PropagatingError, "maybe()");
    }

    #[test]
    fn safe_stream_call_does_not_publish_a_latent_error_fact() {
        let source = with_unrelated_statement(
            "stream ready() -> Stream[Int] { yield 1 }\nproc choose() [error] -> Int { let ignored: Result[Unit, Error] = try { let values = ready().collect(); return values.len() } }",
        );
        let checked = CheckedSource::new("safe-stream-flow.xsh", &source);
        checked.assert_no_errors();
        checked.assert_no_fact_owner(FactCategory::LatentStreamError, "ready()");
    }

    #[test]
    fn non_fallible_map_stage_does_not_publish_a_stage_error_fact() {
        let source = with_unrelated_statement(
            "proc choose() [error] -> Int { let ignored: Result[Unit, Error] = try { let _ = [1] |> map { |item| return item } } }",
        );
        let checked = CheckedSource::new("non-fallible-stage-flow.xsh", &source);
        checked.assert_no_errors();
        checked.assert_no_fact_owner(FactCategory::FalliblePipelineStage, "map");
    }

    #[test]
    fn map_result_callback_does_not_publish_a_propagating_callback_fact() {
        let source = with_unrelated_statement(
            "proc maybe_fail(item: Int) -> Result[Unit, Error] { return Ok() }\nproc choose() [error] -> Int { let ignored: Result[Unit, Error] = try { let _ = [1] |> map(maybe_fail); return 1 } }",
        );
        let checked = CheckedSource::new("map-result-callback-flow.xsh", &source);
        checked.assert_no_errors();
        checked.assert_no_fact_owner(
            FactCategory::PropagatingStageCallback,
            "map(maybe_fail)",
        );
    }

    #[test]
    fn incomplete_error_match_does_not_publish_an_exhaustiveness_fact() {
        let source = with_unrelated_statement(
            "pure choose(values: List[Int]) -> Int { let ignored = match values { [] => { return 1 } } }",
        );
        let checked = CheckedSource::new("incomplete-match-flow.xsh", &source);
        assert!(!checked.declarations.diagnostics.is_empty());
        checked.assert_no_fact_owner(FactCategory::ExhaustiveMatch, "match values");
    }
}

#[cfg(all(test, debug_assertions))]
mod checked_type_fact_tests {
    #[test]
    fn corpus_annotations_agree_with_binding_facts() {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(|| {
                let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
                let mut paths = Vec::new();
                for dir in ["core", "dev", "examples", "showcase", "stdlib", "tests"] {
                    let path = root.join(dir);
                    super::super::indexed::tests::collect_xsh_paths(root, &path, &mut paths)
                        .unwrap_or_else(|error| panic!("cannot collect {}: {error}", path.display()));
                }
                paths.sort();
                let mut lowered = 0;
                for path in paths {
                    let name = path.to_string_lossy();
                    let source = super::super::indexed::tests::read_xsh_source(&path)
                        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
                    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                        &name,
                        crate::loader::entry_source_from_text(&name, source.clone()),
                        Vec::new(),
                    );
                    let source_id = sources.files()[0].id();
                    let declarations = crate::sema::check::Checker::check_compact_declarations(
                        &parsed.arena,
                        &sources,
                        source_id,
                    );
                    if !parsed.diagnostics.is_empty() || !declarations.diagnostics.is_empty() {
                        continue;
                    }
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
                let mismatches = std::mem::take(
                    &mut *super::CHECKED_TYPE_FACT_MISMATCHES.lock().unwrap(),
                );
                assert!(lowered > 300, "only {lowered} corpus programs lowered");
                assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
