mod generic_prepare;
mod scoped_callable_prepare;
mod generic_operation_prepare;
mod operation_prepare;
#[cfg(test)]
mod language_result_tests;
mod language_result_prepare;
mod result_receiver_prepare;
pub(in crate::runtime::eval) use operation_prepare::{PreparedIntegerAddition, PreparedLiteralComparisonSlot, BuildLiteralComparison, PreparedComparisonLiteral, PreparedMembershipLowering};
mod callable_prepare;
mod callable_receiver_prepare;
mod argument_prepare;
pub(in crate::runtime::eval) use argument_prepare::BuildOptionalReceiverGuard;
mod iteration_prepare;
mod record_prepare;
mod record_update_prepare;
mod conditional_prepare;
mod path_prepare;
pub(in crate::runtime::eval) use conditional_prepare::{BuildConditionalArm, BuildConditionalBody, BuildConditionalResult};
mod comprehension_prepare;
mod context_prepare;
mod stage_prepare;
mod duration_prepare;
mod range_prepare;
pub(in crate::runtime::eval) use range_prepare::range_stream;
mod cli_call_prepare;
mod capture_prepare;
mod index_prepare;
mod container_prepare;
mod native_scalar_prepare;
mod mutable_prepare;
mod constructor_prepare;
pub(in crate::runtime::eval) use native_scalar_prepare::{BuildByteAtFallbackOriginal, BuildFoldedNativeReceiver};
pub(in crate::runtime::eval) use container_prepare::{BuildContainerCreationCheck, BuildNamedMapKeyOrigin};
pub(in crate::runtime::eval) use index_prepare::BuildIndexOrigin;
pub(in crate::runtime::eval) use context_prepare::{BuildContextScopeOrigin, BuildRunProducerOrigin, BuildSpawnRunOrigin};
pub(in crate::runtime::eval) use capture_prepare::{BuildRetryCapturePolicy, BuildTryCaptureOrigin};
mod projection_prepare;
mod pattern_prepare;
mod native_prepare;
mod fs_root_prepare;
mod process_prepare;
mod constant_prepare;
mod host_binding_prepare;
mod lexical_capture_prepare;
mod mutable_path_prepare;
mod error_constructor_prepare;
mod bridge_prepare;
pub(in crate::runtime::eval) use native_prepare::BuildSavedNativeReceiverOrigin;
mod native_callable_prepare;
mod value_prepare;
#[cfg(test)]
mod evidence_audit_tests;

use super::semantic::{SemanticPoolBuilder, SemanticPools};
use super::generic::{GenericReturnPlan, TypeRef, CallEvidence, GenericCheckpoint, GenericEvidenceBuilder, GenericEvidenceStore, InstantiationId, InstructionOwner, Requirement, RequirementWitness, SchemeScopeId};
use super::{
    IR_NONE, IrBlockId, IrBuildError, IrData, IrFunctionId, IrLocation, IrLocationId, IrRange,
    IrStringId, IrVerifyError, SignatureId, TypeId,
};
use crate::modules::RuntimeOp;
use crate::modules::hash::HashAlgorithm;
use crate::runtime::eval::{
    BuildBoolId, BuildBoolRow, BuildExprId, BuildExprRow, BuildIntId, BuildIntRow, BuildPatternId,
    BuildPatternIdSlots, BuildPatternRow, BuildScratch, BuildStmtId, BuildStmtRow, BuildTopKind,
    BuildTopStmtId, BuildTopStmtRow, FunctionBuild, FunctionHeader, LoweredCallArg,
    LoweredAssignPath, LoweredAssignStep, LoweredCompQualifier, LoweredCompQualifiers, LoweredCompTarget, LoweredErrorExpr, LoweredErrorPatternFields, LoweredFmtPart,
    LoweredFunctionKey, LoweredFunctionKind, LoweredFunctionUnit, LoweredModuleExport,
    LoweredModuleExportKind, LoweredPipelineStage, LoweredProcessCommandArgv,
    LoweredProcessCommandBuilderEntry, LoweredRecordEntry, LoweredRecordUpdates, LoweredReturnKind, LoweredRunArg,
    LoweredRunArgKind, LoweredRunCapture, LoweredRunEnv, LoweredRunPipelineSegment,
    LoweredRunRedirection, LoweredSpawnRun, LoweredStatsValue, LoweredStrPredicate,
    LoweredTagValue, LoweredTopLevelSlot, LoweredTopLevelSlots, LoweredType, LoweredTypeCheck,
    LoweredValue, LoweredParamDefault, PreparedConstantValue, ProgramBuild, ReduceByOp, ScanBytes, ScanCheck, ScanCondition,
};
use crate::runtime::value::{DurationValue, FloatValue, FunctionName, PathValue, RegexValue};
use crate::sema::check::{CompactBodyProbeOutput, CompactDeclOutput};
use crate::sema::types::{CallableParamType, CallableType, ModuleExportType, Type};
use crate::source::{SourceId, SourceMap, Span};
use crate::symbol::{Name, NameText, QualifiedName, Symbol};
use crate::syntax::arena::{ArenaProgram, StmtId};
use crate::syntax::node::{
    AssignOp, BinaryOp, FormatSpec, FormatSpecKind, RedirectionKind, RunKind,
};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::mem::size_of;
use std::rc::Rc;
use std::sync::Arc;

const DRIVER_OWNER_BIT: u32 = 1 << 31;
const DRIVER_SLOT_READ: u8 = 1;
const DRIVER_SLOT_WRITE: u8 = 1 << 1;
const DRIVER_SLOT_MUTABLE: u8 = 1 << 2;

const EFFECT_IMPORT: u32 = 1 << 0;
const EFFECT_CWD: u32 = 1 << 1;
const EFFECT_ENV: u32 = 1 << 2;
const EFFECT_PROCESS: u32 = 1 << 3;
const EFFECT_SIGNAL: u32 = 1 << 4;
const EFFECT_CANCELLATION: u32 = 1 << 5;
const EFFECT_TRACE: u32 = 1 << 6;
const EFFECT_DYNAMIC_CALL: u32 = 1 << 7;
const EFFECT_DEFER: u32 = 1 << 8;
const EFFECT_PROPAGATE: u32 = 1 << 9;
const EFFECT_HOST: u32 = 1 << 10;
const EFFECT_BINDING_READ: u32 = 1 << 11;
const EFFECT_BINDING_WRITE: u32 = 1 << 12;

const EFFECT_BOUNDARY_MASK: u32 = EFFECT_IMPORT
    | EFFECT_CWD
    | EFFECT_ENV
    | EFFECT_PROCESS
    | EFFECT_SIGNAL
    | EFFECT_CANCELLATION
    | EFFECT_TRACE
    | EFFECT_DYNAMIC_CALL
    | EFFECT_DEFER
    | EFFECT_PROPAGATE
    | EFFECT_HOST;
const EFFECT_ALL: u32 = EFFECT_BOUNDARY_MASK | EFFECT_BINDING_READ | EFFECT_BINDING_WRITE;

fn driver_owner(index: usize) -> Result<u32, IrBuildError> {
    let raw = u32::try_from(index)
        .ok()
        .and_then(|index| index.checked_add(1))
        .filter(|index| *index < DRIVER_OWNER_BIT)
        .ok_or_else(|| IrBuildError::format("driver_step_overflow", None, 0, 0))?;
    Ok(DRIVER_OWNER_BIT | raw)
}

fn driver_owner_index(owner: u32) -> Option<usize> {
    if owner & DRIVER_OWNER_BIT == 0 || owner == IR_NONE {
        return None;
    }
    usize::try_from((owner & !DRIVER_OWNER_BIT).checked_sub(1)?).ok()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(in crate::runtime::eval) enum FullTag {
    // The exhaustive FullCodec implementations below are the payload schemas:
    // each arm writes and reads fields in one order and preserves the runtime
    // operation, error, location, and trace contract owned by Lowered*.
    IntInt,
    IntSlot,
    IntBinary,
    IntStrByteLenSlot,
    IntStrCountLinesSlot,
    IntStrByteAtSlot,
    BoolBool,
    BoolSlot,
    BoolNot,
    BoolAnd,
    BoolOr,
    BoolIntCompare,
    BoolStrPredicateSlot,
    BoolContainsSlot,
    BoolStrContainsSlot,
    BoolTrimEmptySlot,
    BoolTrimStrPredicateSlot,
    BoolLiteralCompareSlot,
    ExprNull,
    ExprUnit,
    ExprInt,
    ExprFloat,
    ExprDuration,
    ExprBool,
    ExprStr,
    ExprBytes,
    ExprPreparedRegex,
    ExprPreparedConstant,
    ExprPath,
    ExprFunctionRef,
    ExprNativeCallableRef,
    ExprPathFrom,
    ExprParam,
    ExprAssert,
    ExprComparisonChain,
    ExprBinary,
    ExprIf,
    ExprPatternIf,
    ExprMatch,
    ExprStrMatch,
    ExprTagMatch,
    ExprResultFallback,
    ExprFmtString,
    ExprPathFmtString,
    ExprGlob,
    ExprLastStatus,
    ExprRecord,
    ExprMapLiteral,
    ExprRecordUpdate,
    ExprList,
    ExprListBuild,
    ExprEmptyMap,
    ExprBytesConcat,
    ExprRange,
    ExprTag,
    ExprListComp,
    ExprMapComp,
    ExprPipeline,
    ExprField,
    ExprIndex,
    ExprSlice,
    ExprMethod,
    ExprStrByteLen,
    ExprStrByteAt,
    ExprStrPredicate,
    ExprRegexCompile,
    ExprRequire,
    ExprCheckedValue,
    ExprRunCapture,
    ExprRunPipeline,
    ExprSpawnRun,
    ExprSpawnCommand,
    ExprWait,
    ExprCapture,
    ExprValueBlock,
    ExprErrorContext,
    ExprContextScope,
    ExprLoop,
    ExprRetry,
    ExprFsFiles,
    ExprFsWalk,
    ExprFsList,
    ExprFsTempDir,
    ExprFsWrite,
    ExprFsMkdir,
    ExprFsRemove,
    ExprPathReadText,
    ExprPathReadBytes,
    ExprPathExists,
    ExprPathExecutable,
    ExprPathDu,
    ExprPathMetadata,
    ExprPathReadlink,
    ExprPathResolve,
    ExprPathWrite,
    ExprPathMkdir,
    ExprPathRemove,
    ExprJsonEncode,
    ExprArchiveTarCreate,
    ExprArchiveTarList,
    ExprArchiveTarExtract,
    ExprModuleCall,
    ExprProcessCommandArgv,
    ExprProcessCommandBuilder,
    ExprAbort,
    ExprFail,
    ExprOk,
    ExprErr,
    ExprError,
    ExprTry,
    ExprCall,
    ExprDirectPureCall,
    ExprExternalCall,
    ExprDynamicCall,
    ExprSelfCall,
    StmtLet,
    StmtGuard,
    StmtWith,
    StmtLetInt,
    StmtLetBool,
    StmtAssign,
    StmtAssignField,
    StmtAssignFieldInt,
    StmtAssignPath,
    StmtAssignInt,
    StmtAssignBool,
    StmtValue,
    StmtAssert,
    StmtExpr,
    StmtIf,
    StmtIfBool,
    StmtWhile,
    StmtWhileBool,
    StmtPatternIf,
    StmtPatternWhile,
    StmtMatch,
    StmtStrMatch,
    StmtTagMatch,
    StmtFor,
    StmtLetRecord,
    StmtForRecord,
    StmtForStrLines,
    StmtScanLines,
    StmtScanBytes,
    StmtPrint,
    StmtCd,
    StmtEnv,
    StmtProc,
    StmtRun,
    StmtLoop,
    StmtReturn,
    StmtYield,
    StmtYieldDelegate,
    StmtBreak,
    StmtBreakValue,
    StmtContinue,
    StmtDefer,
    StmtDefaultParameter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(in crate::runtime::eval) enum FullPatternTag {
    Alias,
    Alternation,
    List,
    TagType,
    RecordTest,
    ResultTest,
    TagTest,
    ErrorTest,
    Wildcard,
    Bind,
    Type,
    Literal,
    ResultOk,
    ResultErr,
    ErrorVariant,
    Facet,
    Tag,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(in crate::runtime::eval) enum FullStageTag {
    TextLines,
    JsonLines,
    Where,
    WhereBlock,
    Map,
    MapBlock,
    FlatMap,
    FlatMapBlock,
    BytesChunks,
    BatchCount,
    BatchMaxArgv,
    BatchMaxBytes,
    BatchLimits,
    Shuffle,
    Fold,
    ReduceBy,
    ReduceByConfigured,
    ParMap,
    ParMapBlock,
    ParMapFlatMapReduceBy,
    Tee,
    Each,
    TablePrint,
    TablePrintConfigured,
    Enumerate,
    Zip,
    Sort,
    SortBy,
    GroupBy,
    CountBy,
    Any,
    AnyBlock,
    All,
    AllBlock,
    UniqueBy,
    Count,
    Sum,
    Collect,
    First,
    Last,
    Min,
    Max,
    Take,
    Drop,
    Repeat,
    Range,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(in crate::runtime::eval) enum FullValueTag {
    Null,
    Unit,
    Int,
    Float,
    Duration,
    Bool,
    Str,
    Bytes,
    Regex,
    Path,
    Record,
    RecordVec,
    Stats,
    StatsBlob,
    Module,
    List,
    Map,
    Tag,
    ResultOk,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(in crate::runtime::eval) enum FullDriverTag {
    Skip,
    Use,
    Let,
    LetRecord,
    Assign,
    Discard,
    Stmt,
    Expr,
    Defer,
    SignalHook,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
struct FullDriverStep {
    data: IrData,
    slots: IrRange,
    instruction_start: u32,
    slot_count: u32,
    location: u32,
    effects: u32,
    tag: FullDriverTag,
    reserved: [u8; 3],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
struct FullDriverSlot {
    name: u32,
    type_id: TypeId,
    slot: u32,
    flags: u8,
    reserved: [u8; 3],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
struct FullDriverSync {
    name: u32,
    type_id: TypeId,
    flags: u8,
    reserved: [u8; 3],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
struct FullDriverRegion {
    steps: IrRange,
    sync: IrRange,
    effects: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
struct FullDriverProgram {
    steps: IrRange,
    regions: IrRange,
    effects: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
struct FullFunction {
    name: u32,
    signature: u32,
    params: IrRange,
    captures: IrRange,
    body: u32,
    slot_count: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
struct FullBlock {
    // `instructions` stores a count followed by typed IDs. The codec selecting
    // the block supplies the element schema; statement blocks have one owner.
    instructions: IrRange,
    result: u32,
    owner: u32,
    flags: u8,
    reserved: [u8; 3],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
struct FullParam {
    name: u32,
    type_id: u32,
    flags: u8,
    reserved: [u8; 3],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
struct FullParamCold {
    param: u32,
    default: u32,
    validation: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
struct FullCapture {
    name: u32,
    type_id: TypeId,
    slot_and_flags: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
struct FullValidation {
    type_id: TypeId,
    name: u32,
}

// The declaration header independently preserves completion wrapping: two
// policies can share a Result type while producing different carrier depths.
const GENERIC_RETURN_PLAN_MASK: u8 = 0b11 << 3;

const fn generic_return_plan_flags(plan: GenericReturnPlan) -> u8 {
    match plan {
        GenericReturnPlan::Value => 0,
        GenericReturnPlan::Result => 1 << 3,
        GenericReturnPlan::Unit => 2 << 3,
        GenericReturnPlan::ResultUnit => 3 << 3,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
struct FullFunctionMetadata {
    owner: u32,
    flags: u8,
    reserved: [u8; 3],
}

#[derive(Clone, Debug)]
struct FullStore {
    source_id: SourceId,
    tags: Vec<FullTag>,
    data: Vec<IrData>,
    extra: Vec<u32>,
    patterns: Vec<FullPatternTag>,
    pattern_data: Vec<IrData>,
    stages: Vec<FullStageTag>,
    stage_data: Vec<IrData>,
    values: Vec<FullValueTag>,
    value_data: Vec<IrData>,
    blocks: Vec<FullBlock>,
    driver_steps: Vec<FullDriverStep>,
    driver_slots: Vec<FullDriverSlot>,
    driver_sync: Vec<FullDriverSync>,
    driver_regions: Vec<FullDriverRegion>,
    driver_programs: Vec<FullDriverProgram>,
    driver_root: u32,
    functions: Vec<FullFunction>,
    function_instruction_starts: Vec<u32>,
    function_metadata: Vec<FullFunctionMetadata>,
    params: Vec<FullParam>,
    param_cold: Vec<FullParamCold>,
    captures: Vec<FullCapture>,
    validations: Vec<FullValidation>,
    strings: Vec<IrRange>,
    string_bytes: Vec<u8>,
    bytes: Vec<IrRange>,
    byte_data: Vec<u8>,
    prepared_regexes: Vec<RegexValue>,
    prepared_constants: Vec<PreparedConstantValue>,
    wire_enums: Vec<Arc<crate::sema::wire_enums::WireEnumMapping>>,
    prepared_schemas: Vec<Arc<super::super::require::PreparedSchema>>,
    prepared_cli_plans: Vec<Arc<crate::modules::cli::CliDescriptorPlan>>,
    locations: Vec<IrLocation>,
    location_sources: Vec<SourceId>,
    runtime_ops: Vec<RuntimeOp>,
    assign_ops: Vec<AssignOp>,
    binary_ops: Vec<BinaryOp>,
    run_kinds: Vec<RunKind>,
    redirection_kinds: Vec<RedirectionKind>,
    semantic: SemanticPools,
    generic: Option<Box<GenericEvidenceStore>>,
    original_generic_owner: Option<u64>,
}

impl Default for FullStore {
    fn default() -> Self {
        Self {
            source_id: SourceId::new(0),
            tags: Vec::new(),
            data: Vec::new(),
            extra: Vec::new(),
            patterns: Vec::new(),
            pattern_data: Vec::new(),
            stages: Vec::new(),
            stage_data: Vec::new(),
            values: Vec::new(),
            value_data: Vec::new(),
            blocks: Vec::new(),
            driver_steps: Vec::new(),
            driver_slots: Vec::new(),
            driver_sync: Vec::new(),
            driver_regions: Vec::new(),
            driver_programs: Vec::new(),
            driver_root: IR_NONE,
            functions: Vec::new(),
            function_instruction_starts: Vec::new(),
            function_metadata: Vec::new(),
            params: Vec::new(),
            param_cold: Vec::new(),
            captures: Vec::new(),
            validations: Vec::new(),
            strings: Vec::new(),
            string_bytes: Vec::new(),
            bytes: Vec::new(),
            byte_data: Vec::new(),
            prepared_regexes: Vec::new(),
            prepared_constants: Vec::new(),
            wire_enums: Vec::new(),
            prepared_schemas: Vec::new(),
            prepared_cli_plans: Vec::new(),
            locations: Vec::new(),
            location_sources: Vec::new(),
            runtime_ops: Vec::new(),
            assign_ops: Vec::new(),
            binary_ops: Vec::new(),
            run_kinds: Vec::new(),
            redirection_kinds: Vec::new(),
            semantic: SemanticPools::default(),
            generic: None,
            original_generic_owner: None,
        }
    }
}

impl FullStore {
    fn verify_generic_owner(&self) -> Result<(), IrVerifyError> {
        if let Some(original) = self.original_generic_owner
            && self.generic.as_deref().is_none_or(|generic| generic.program_owner() != original) {
            return Err(IrVerifyError::new("prepared program lacks its original evidence owner"));
        }
        Ok(())
    }
    fn generic_instruction_owners(&self) -> Result<Vec<Option<InstructionOwner>>, IrVerifyError> {
        let mut owners = vec![None; self.tags.len()];
        for index in 0..self.functions.len() {
            let owner = InstructionOwner::Function(IrFunctionId::new(index).map_err(|_| IrVerifyError::new("generic function owner overflows"))?);
            for instruction in self.function_instruction_range(index)? {
                if owners[instruction].replace(owner).is_some() { return Err(IrVerifyError::new("generic function instruction ranges overlap")); }
            }
        }
        for index in 0..self.driver_steps.len() {
            let owner = InstructionOwner::Driver(u32::try_from(index).map_err(|_| IrVerifyError::new("generic driver owner overflows"))?);
            for instruction in self.driver_instruction_range(index)? {
                if owners[instruction].replace(owner).is_some() { return Err(IrVerifyError::new("generic driver instruction ranges overlap")); }
            }
        }
        Ok(owners)
    }
    #[inline(always)]
    fn payload(&self, range: IrRange) -> Result<&[u32], IrVerifyError> {
        let bounds = range
            .bounds(self.extra.len())
            .ok_or_else(|| IrVerifyError::new("full IR extra range is out of bounds"))?;
        Ok(&self.extra[bounds])
    }

    #[inline(always)]
    unsafe fn payload_unchecked(&self, range: IrRange) -> &[u32] {
        let start = range.start as usize;
        let end = start + range.len as usize;
        unsafe { self.extra.get_unchecked(start..end) }
    }

    #[inline(always)]
    fn string(&self, raw: u32) -> Result<&str, IrVerifyError> {
        let id = IrStringId::from_raw(raw)
            .ok_or_else(|| IrVerifyError::new("full IR string id is invalid"))?;
        let range = self
            .strings
            .get(id.index())
            .copied()
            .ok_or_else(|| IrVerifyError::new("full IR string id is out of bounds"))?;
        let bounds = range
            .bounds(self.string_bytes.len())
            .ok_or_else(|| IrVerifyError::new("full IR string range is out of bounds"))?;
        std::str::from_utf8(&self.string_bytes[bounds])
            .map_err(|_| IrVerifyError::new("full IR string is not UTF-8"))
    }

    fn bytes(&self, raw: u32) -> Result<&[u8], IrVerifyError> {
        let id = super::IrBytesId::from_raw(raw)
            .ok_or_else(|| IrVerifyError::new("full IR bytes id is invalid"))?;
        let range = self
            .bytes
            .get(id.index())
            .copied()
            .ok_or_else(|| IrVerifyError::new("full IR bytes id is out of bounds"))?;
        let bounds = range
            .bounds(self.byte_data.len())
            .ok_or_else(|| IrVerifyError::new("full IR byte range is out of bounds"))?;
        Ok(&self.byte_data[bounds])
    }

    fn function_instruction_range(
        &self,
        index: usize,
    ) -> Result<std::ops::Range<usize>, IrVerifyError> {
        let start = *self
            .function_instruction_starts
            .get(index)
            .ok_or_else(|| IrVerifyError::new("function instruction start is missing"))?;
        if start == IR_NONE {
            return Err(IrVerifyError::new("function body was not committed"));
        }
        let end = self
            .function_instruction_starts
            .get(index + 1)
            .copied()
            .or_else(|| self.driver_steps.first().map(|step| step.instruction_start))
            .unwrap_or(self.tags.len() as u32);
        let range = IrRange::new(start, end.saturating_sub(start));
        range
            .bounds(self.tags.len())
            .ok_or_else(|| IrVerifyError::new("function instruction range is invalid"))
    }

    fn driver_instruction_range(
        &self,
        index: usize,
    ) -> Result<std::ops::Range<usize>, IrVerifyError> {
        let step = self
            .driver_steps
            .get(index)
            .ok_or_else(|| IrVerifyError::new("driver step is out of bounds"))?;
        let end = self
            .driver_steps
            .get(index + 1)
            .map_or(self.tags.len() as u32, |next| next.instruction_start);
        IrRange::new(
            step.instruction_start,
            end.saturating_sub(step.instruction_start),
        )
        .bounds(self.tags.len())
        .ok_or_else(|| IrVerifyError::new("driver instruction range is invalid"))
    }

    fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            + self.tags.capacity() * size_of::<FullTag>()
            + self.data.capacity() * size_of::<IrData>()
            + self.extra.capacity() * size_of::<u32>()
            + self.patterns.capacity() * size_of::<FullPatternTag>()
            + self.pattern_data.capacity() * size_of::<IrData>()
            + self.stages.capacity() * size_of::<FullStageTag>()
            + self.stage_data.capacity() * size_of::<IrData>()
            + self.values.capacity() * size_of::<FullValueTag>()
            + self.value_data.capacity() * size_of::<IrData>()
            + self.blocks.capacity() * size_of::<FullBlock>()
            + self.driver_steps.capacity() * size_of::<FullDriverStep>()
            + self.driver_slots.capacity() * size_of::<FullDriverSlot>()
            + self.driver_sync.capacity() * size_of::<FullDriverSync>()
            + self.driver_regions.capacity() * size_of::<FullDriverRegion>()
            + self.driver_programs.capacity() * size_of::<FullDriverProgram>()
            + self.functions.capacity() * size_of::<FullFunction>()
            + self.function_instruction_starts.capacity() * size_of::<u32>()
            + self.function_metadata.capacity() * size_of::<FullFunctionMetadata>()
            + self.params.capacity() * size_of::<FullParam>()
            + self.param_cold.capacity() * size_of::<FullParamCold>()
            + self.captures.capacity() * size_of::<FullCapture>()
            + self.validations.capacity() * size_of::<FullValidation>()
            + self.strings.capacity() * size_of::<IrRange>()
            + self.string_bytes.capacity()
            + self.bytes.capacity() * size_of::<IrRange>()
            + self.byte_data.capacity()
            + self.prepared_constants.capacity() * size_of::<PreparedConstantValue>()
            + self.prepared_schemas.capacity() * size_of::<Arc<super::super::require::PreparedSchema>>()
            + self.prepared_schemas.iter().map(|schema| schema.retained_bytes()).sum::<usize>()
            + self.wire_enums.capacity() * size_of::<Arc<crate::sema::wire_enums::WireEnumMapping>>()
            + self.prepared_cli_plans.capacity() * size_of::<Arc<crate::modules::cli::CliDescriptorPlan>>()
            + self.prepared_regexes.capacity() * size_of::<RegexValue>()
            + self.prepared_regexes.iter().map(|value| value.pattern.capacity()).sum::<usize>()
            + self.locations.capacity() * size_of::<IrLocation>()
            + self.location_sources.capacity() * size_of::<SourceId>()
            + self.runtime_ops.capacity() * size_of::<RuntimeOp>()
            + self.assign_ops.capacity() * size_of::<AssignOp>()
            + self.binary_ops.capacity() * size_of::<BinaryOp>()
            + self.run_kinds.capacity() * size_of::<RunKind>()
            + self.redirection_kinds.capacity() * size_of::<RedirectionKind>()
            + self
                .semantic
                .retained_bytes()
                .saturating_sub(size_of::<SemanticPools>())
            + self.generic.as_ref().map_or(0, |generic| generic.retained_bytes())
    }

    #[cfg(test)]
    fn driver_retained_bytes(&self) -> usize {
        self.driver_steps.capacity() * size_of::<FullDriverStep>()
            + self.driver_slots.capacity() * size_of::<FullDriverSlot>()
            + self.driver_sync.capacity() * size_of::<FullDriverSync>()
            + self.driver_regions.capacity() * size_of::<FullDriverRegion>()
            + self.driver_programs.capacity() * size_of::<FullDriverProgram>()
    }

    fn shrink_to_fit(&mut self) {
        self.tags.shrink_to_fit();
        self.data.shrink_to_fit();
        self.extra.shrink_to_fit();
        self.patterns.shrink_to_fit();
        self.pattern_data.shrink_to_fit();
        self.stages.shrink_to_fit();
        self.stage_data.shrink_to_fit();
        self.values.shrink_to_fit();
        self.value_data.shrink_to_fit();
        self.blocks.shrink_to_fit();
        self.driver_steps.shrink_to_fit();
        self.driver_slots.shrink_to_fit();
        self.driver_sync.shrink_to_fit();
        self.driver_regions.shrink_to_fit();
        self.driver_programs.shrink_to_fit();
        self.functions.shrink_to_fit();
        self.function_instruction_starts.shrink_to_fit();
        self.function_metadata.shrink_to_fit();
        self.params.shrink_to_fit();
        self.param_cold.shrink_to_fit();
        self.captures.shrink_to_fit();
        self.validations.shrink_to_fit();
        self.strings.shrink_to_fit();
        self.string_bytes.shrink_to_fit();
        self.bytes.shrink_to_fit();
        self.byte_data.shrink_to_fit();
        self.prepared_regexes.shrink_to_fit();
        self.prepared_constants.shrink_to_fit();
        self.wire_enums.shrink_to_fit();
        self.prepared_schemas.shrink_to_fit();
        self.prepared_cli_plans.shrink_to_fit();
        self.locations.shrink_to_fit();
        self.location_sources.shrink_to_fit();
        self.runtime_ops.shrink_to_fit();
        self.assign_ops.shrink_to_fit();
        self.binary_ops.shrink_to_fit();
        self.run_kinds.shrink_to_fit();
        self.redirection_kinds.shrink_to_fit();
        self.semantic.shrink_to_fit();
        if let Some(generic) = &mut self.generic { generic.shrink_to_fit(); }
    }
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct FullProgram {
    source_lowering_stats: super::super::FrontendLoweredStats,
    store: FullStore,
    sources: Arc<SourceMap>,
    symbols: crate::symbol::SymbolOwner,
    function_definition_spans: Vec<Span>,
    /// The decoded headers, one slot per function, filled on first call.
    ///
    /// A header is immutable once the store is verified: it is decoded from the
    /// function's parameter and capture rows, which nothing mutates afterwards.
    /// It is also several allocations wide — interned names, lowered types, and
    /// decoded default values — so decoding it on every call charged the call
    /// path for metadata preparation already resolved. The slot lives in the
    /// program, so the cache's identity and lifetime are the owning program's:
    /// it is indexed by function and dies with the program, and the names it
    /// holds stay valid because the program owns the symbol owner that
    /// interned them.
    headers: Vec<std::sync::OnceLock<Arc<FunctionHeader>>>,
    /// Function identities are interned once into this program's symbol owner.
    /// A malformed directly constructed fixture leaves this empty so the
    /// fallback scan reports its original verification error.
    identities:
        std::sync::OnceLock<Option<FxHashMap<(LoweredFunctionKey, LoweredFunctionKind), usize>>>,
}

#[derive(Clone, Copy)]
pub(in crate::runtime::eval) struct FullFunctionView<'a> {
    program: &'a FullProgram,
    index: usize,
    instantiation: Option<InstantiationId>,
}

#[derive(Clone, Copy)]
pub(in crate::runtime::eval) struct FullDriverStepView<'a> {
    program: &'a FullProgram,
    index: usize,
}

impl FullProgram {
    pub(in crate::runtime::eval) fn function_view_by_id(&self, function: IrFunctionId) -> Result<FullFunctionView<'_>, IrVerifyError> {
        if function.index() >= self.store.functions.len() { return Err(IrVerifyError::new("prepared callable function is out of bounds")); }
        Ok(FullFunctionView { program: self, index: function.index(), instantiation: None })
    }
    pub(in crate::runtime::eval) fn callable_value(&self, instruction: u32) -> Result<Option<super::generic::CallableValueId>, IrVerifyError> {
        if instruction as usize >= self.store.tags.len() { return Err(IrVerifyError::new("callable creation instruction is out of bounds")); }
        self.store.generic.as_deref().map_or(Ok(None), |store| store.callable_value_at(instruction))
    }
    pub(in crate::runtime::eval) fn invocation_plan(&self, instruction: u32) -> Result<Option<super::generic::InvocationPlanId>, IrVerifyError> {
        if instruction as usize >= self.store.tags.len() { return Err(IrVerifyError::new("invocation instruction is out of bounds")); }
        self.store.generic.as_deref().map_or(Ok(None), |store| store.invocation_plan_at(instruction))
    }
    pub(in crate::runtime::eval) fn source_lowering_stats(&self) -> super::super::FrontendLoweredStats {
        super::super::FrontendLoweredStats { retained_estimate_bytes: self.store.retained_bytes(), ..self.source_lowering_stats }
    }
    pub(in crate::runtime::eval) fn generic_evidence(&self) -> Option<&GenericEvidenceStore> { self.store.generic.as_deref() }
    pub(in crate::runtime::eval) fn symbol_owner(&self) -> &crate::symbol::SymbolOwner {
        &self.symbols
    }

    pub(in crate::runtime::eval) fn function_count(&self) -> usize {
        self.store.functions.len()
    }

    pub(in crate::runtime::eval) fn store_retained_bytes(&self) -> usize {
        self.store.retained_bytes()
    }

    pub(in crate::runtime::eval) fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            + self
                .store
                .retained_bytes()
                .saturating_sub(size_of::<FullStore>())
            + self.sources.retained_bytes()
    }

    pub(in crate::runtime::eval) fn instruction_count(&self) -> usize {
        self.store.tags.len()
    }

    pub(in crate::runtime::eval) fn extra_words(&self) -> usize {
        self.store.extra.len()
    }

    pub(in crate::runtime::eval) fn contains_function(
        &self,
        key: LoweredFunctionKey,
        kind: LoweredFunctionKind,
    ) -> bool {
        let _symbols = self.symbol_owner().enter();
        (0..self.store.functions.len()).any(|function_index| {
            self.function_identity(function_index)
                .is_ok_and(|identity| identity == (key, kind))
        })
    }

    pub(in crate::runtime::eval) fn function_param_kinds(
        &self,
        key: LoweredFunctionKey,
        kind: LoweredFunctionKind,
    ) -> Result<Option<Vec<LoweredType>>, IrVerifyError> {
        let Some(view) = self.function_view(key, kind)? else {
            return Ok(None);
        };
        let function = self.store.functions[view.index];
        let params = function
            .params
            .bounds(self.store.params.len())
            .ok_or_else(|| IrVerifyError::new("function parameter range is invalid"))?;
        self.store.params[params]
            .iter()
            .map(|param| if param.type_id == IR_NONE { Ok(LoweredType::Generic) } else { lowered_type_from_type(&self.store.semantic.to_type(TypeId::from_raw(param.type_id).ok_or_else(|| IrVerifyError::new("parameter type id is invalid"))?)?) })
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }

    pub(in crate::runtime::eval) fn function_view(
        &self,
        key: LoweredFunctionKey,
        kind: LoweredFunctionKind,
    ) -> Result<Option<FullFunctionView<'_>>, IrVerifyError> {
        let _symbols = self.symbol_owner().enter();
        let identities = self
            .identities
            .get_or_init(|| self.resolve_identities().ok());
        if let Some(identities) = identities.as_ref() {
            return Ok(identities.get(&(key, kind)).map(|index| FullFunctionView {
                program: self,
                index: *index,
                instantiation: None,
            }));
        }
        for index in 0..self.store.functions.len() {
            if self.function_identity(index)? == (key, kind) {
                return Ok(Some(FullFunctionView {
                    program: self,
                    index,
                    instantiation: None,
                }));
            }
        }
        Ok(None)
    }

    /// Keep the scan's first-match behavior for repeated identities.
    fn resolve_identities(
        &self,
    ) -> Result<FxHashMap<(LoweredFunctionKey, LoweredFunctionKind), usize>, IrVerifyError> {
        let mut identities = FxHashMap::default();
        identities.reserve(self.store.functions.len());
        for index in 0..self.store.functions.len() {
            let identity = self.function_identity(index)?;
            identities.entry(identity).or_insert(index);
        }
        Ok(identities)
    }

    pub(in crate::runtime::eval) fn function_view_at(
        &self,
        index: usize,
    ) -> Option<FullFunctionView<'_>> {
        (index < self.store.functions.len()).then_some(FullFunctionView {
            program: self,
            index,
            instantiation: None,
        })
    }

    fn function_identity(
        &self,
        function_index: usize,
    ) -> Result<(LoweredFunctionKey, LoweredFunctionKind), IrVerifyError> {
        let function = self
            .store
            .functions
            .get(function_index)
            .ok_or_else(|| IrVerifyError::new("function identity is out of bounds"))?;
        let metadata = self
            .store
            .function_metadata
            .get(function_index)
            .copied()
            .ok_or_else(|| IrVerifyError::new("function metadata is missing"))?;
        let name = Name::intern(self.store.string(function.name)?);
        let key = if metadata.owner == IR_NONE {
            LoweredFunctionKey::Name(name)
        } else {
            LoweredFunctionKey::Qualified(QualifiedName::new(
                Name::intern(self.store.string(metadata.owner)?),
                name,
            ))
        };
        let kind = if metadata.flags & 1 == 0 {
            LoweredFunctionKind::Pure
        } else {
            LoweredFunctionKind::Proc
        };
        Ok((key, kind))
    }

    fn pipeline_stage_tags(
        &self,
        instruction_range: std::ops::Range<usize>,
    ) -> Result<Vec<FullStageTag>, IrVerifyError> {
        let mut tags = Vec::new();
        for instruction in instruction_range {
            if self.store.tags[instruction] != FullTag::ExprPipeline {
                continue;
            }
            let data = self.store.data[instruction];
            let mut payload = FullCursor::new(self.store.payload(data.range())?);
            payload.raw()?;
            let block = IrBlockId::from_raw(payload.raw()?)
                .ok_or_else(|| IrVerifyError::new("pipeline stage block id is invalid"))?;
            let block = self
                .store
                .blocks
                .get(block.index())
                .copied()
                .ok_or_else(|| IrVerifyError::new("pipeline stage block id is out of bounds"))?;
            if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST {
                return Err(IrVerifyError::new("pipeline stage block kind is invalid"));
            }
            let mut stages = FullCursor::new(self.store.payload(block.instructions)?);
            let len = stages.raw()? as usize;
            tags.reserve(len);
            for _ in 0..len {
                let index = stages.raw()? as usize;
                tags.push(
                    self.store
                        .stages
                        .get(index)
                        .copied()
                        .ok_or_else(|| IrVerifyError::new("pipeline stage id is out of bounds"))?,
                );
            }
            stages.finish()?;
        }
        Ok(tags)
    }

    pub(in crate::runtime::eval) fn driver_step_count(&self) -> Result<usize, IrVerifyError> {
        Ok(self.driver_root_steps()?.len())
    }

    pub(in crate::runtime::eval) fn driver_step_view(
        &self,
        index: usize,
    ) -> Result<FullDriverStepView<'_>, IrVerifyError> {
        let steps = self.driver_root_steps()?;
        let index = steps
            .start
            .checked_add(index)
            .filter(|index| *index < steps.end)
            .ok_or_else(|| IrVerifyError::new("driver root step is out of bounds"))?;
        Ok(FullDriverStepView {
            program: self,
            index,
        })
    }

    pub(in crate::runtime::eval) fn driver_program_step_views(
        &self,
        raw: u32,
    ) -> Result<Vec<FullDriverStepView<'_>>, IrVerifyError> {
        let index = raw
            .checked_sub(1)
            .map(|index| index as usize)
            .filter(|index| *index < self.store.driver_programs.len())
            .ok_or_else(|| IrVerifyError::new("driver program id is out of bounds"))?;
        let steps = self.store.driver_programs[index]
            .steps
            .bounds(self.store.driver_steps.len())
            .ok_or_else(|| IrVerifyError::new("driver program step range is invalid"))?;
        Ok(steps
            .map(|index| FullDriverStepView {
                program: self,
                index,
            })
            .collect())
    }

    pub(in crate::runtime::eval) fn driver_step_view_absolute(
        &self,
        index: usize,
    ) -> Result<FullDriverStepView<'_>, IrVerifyError> {
        if index >= self.store.driver_steps.len() {
            return Err(IrVerifyError::new("driver step is out of bounds"));
        }
        Ok(FullDriverStepView {
            program: self,
            index,
        })
    }

    pub(in crate::runtime::eval) fn driver_step_is_skip(
        &self,
        index: usize,
    ) -> Result<bool, IrVerifyError> {
        let steps = self.driver_root_steps()?;
        let step = steps
            .start
            .checked_add(index)
            .filter(|step| *step < steps.end)
            .ok_or_else(|| IrVerifyError::new("driver root step is out of bounds"))?;
        Ok(self.store.driver_steps[step].tag == FullDriverTag::Skip)
    }

    pub(in crate::runtime::eval) fn driver_step_is_defer(
        &self,
        index: usize,
    ) -> Result<bool, IrVerifyError> {
        let steps = self.driver_root_steps()?;
        let step = steps
            .start
            .checked_add(index)
            .filter(|step| *step < steps.end)
            .ok_or_else(|| IrVerifyError::new("driver root step is out of bounds"))?;
        Ok(self.store.driver_steps[step].tag == FullDriverTag::Defer)
    }

    fn driver_root_steps(&self) -> Result<std::ops::Range<usize>, IrVerifyError> {
        let root = self
            .store
            .driver_root
            .checked_sub(1)
            .map(|index| index as usize)
            .and_then(|index| self.store.driver_programs.get(index))
            .ok_or_else(|| IrVerifyError::new("driver root is out of bounds"))?;
        root.steps
            .bounds(self.store.driver_steps.len())
            .ok_or_else(|| IrVerifyError::new("driver root step range is invalid"))
    }

    fn verify_driver(&self, pattern_tree: Option<&std::sync::Mutex<super::pattern::PatternTreeBuilder>>) -> Result<(), IrVerifyError> {
        if self.store.driver_root == IR_NONE {
            return Ok(());
        }
        let mut program_states = vec![0; self.store.driver_programs.len()];
        let mut step_states = vec![0; self.store.driver_steps.len()];
        self.verify_driver_program(
            self.store.driver_root,
            &mut program_states,
            &mut step_states,
            pattern_tree,
        )?;
        if program_states.iter().any(|state| *state != 2)
            || step_states.iter().any(|state| *state != 2)
        {
            return Err(IrVerifyError::new(
                "driver plan contains an unreachable program or step",
            ));
        }
        Ok(())
    }

    fn verify_driver_program(
        &self,
        raw: u32,
        program_states: &mut [u8],
        step_states: &mut [u8],
        pattern_tree: Option<&std::sync::Mutex<super::pattern::PatternTreeBuilder>>,
    ) -> Result<(), IrVerifyError> {
        let index = raw
            .checked_sub(1)
            .map(|index| index as usize)
            .filter(|index| *index < self.store.driver_programs.len())
            .ok_or_else(|| IrVerifyError::new("driver program id is out of bounds"))?;
        match program_states[index] {
            0 => program_states[index] = 1,
            1 => return Err(IrVerifyError::new("driver program graph contains a cycle")),
            2 => {
                return Err(IrVerifyError::new(
                    "driver program is owned by multiple import steps",
                ));
            }
            _ => unreachable!("driver program state is bounded"),
        }
        let steps = self.store.driver_programs[index]
            .steps
            .bounds(self.store.driver_steps.len())
            .ok_or_else(|| IrVerifyError::new("driver program step range is invalid"))?;
        for step_index in steps {
            match step_states[step_index] {
                0 => step_states[step_index] = 1,
                1 => return Err(IrVerifyError::new("driver step graph contains a cycle")),
                2 => {
                    return Err(IrVerifyError::new(
                        "driver step is owned by multiple programs",
                    ));
                }
                _ => unreachable!("driver step state is bounded"),
            }
            self.verify_driver_step(step_index, program_states, step_states, pattern_tree)?;
            step_states[step_index] = 2;
        }
        program_states[index] = 2;
        Ok(())
    }

    fn verify_driver_step(
        &self,
        step_index: usize,
        program_states: &mut [u8],
        step_states: &mut [u8],
        pattern_tree: Option<&std::sync::Mutex<super::pattern::PatternTreeBuilder>>,
    ) -> Result<(), IrVerifyError> {
        let step = self.store.driver_steps[step_index];
        let instruction_range = self.store.driver_instruction_range(step_index)?;
        let decoder = FullDecoder {
            store: &self.store,
            owner: driver_owner(step_index)
                .map_err(|_| IrVerifyError::new("driver owner is invalid"))?,
            instruction_states: Some(RefCell::new(vec![0; instruction_range.len()])),
            instruction_range,
            block_states: Some(RefCell::new(vec![0; self.store.blocks.len()])),
            slot_count: step.slot_count,
            pattern_tree, pattern_ceiling: Cell::new(usize::MAX),
            verified: false,
        };
        let location_words = [step.location];
        let mut location = FullCursor::new(&location_words);
        Span::verify(&decoder, &mut location)?;
        location.finish()?;
        let slots = step
            .slots
            .bounds(self.store.driver_slots.len())
            .ok_or_else(|| IrVerifyError::new("driver slot range is invalid"))?;
        for slot in &self.store.driver_slots[slots] {
            if slot.flags & !(DRIVER_SLOT_READ | DRIVER_SLOT_WRITE | DRIVER_SLOT_MUTABLE) != 0
                || slot.flags & DRIVER_SLOT_READ == 0
                || slot.slot >= step.slot_count
            {
                return Err(IrVerifyError::new("driver slot metadata is invalid"));
            }
            self.store.string(slot.name)?;
            callable_prepare::checked_storage_kind(&self.store.semantic, slot.type_id)?;
        }
        let mut payload = FullCursor::new(self.store.payload(step.data.range())?);
        match step.tag {
            FullDriverTag::Skip => {}
            FullDriverTag::Use => {
                Arc::<str>::verify(&decoder, &mut payload)?;
                Option::<Name>::verify(&decoder, &mut payload)?;
                Vec::<Name>::verify(&decoder, &mut payload)?;
                Name::verify(&decoder, &mut payload)?;
                Vec::<LoweredModuleExport>::verify(&decoder, &mut payload)?;
                let child = payload.raw()?;
                Span::verify(&decoder, &mut payload)?;
                self.verify_driver_program(child, program_states, step_states, pattern_tree)?;
            }
            FullDriverTag::Let => {
                Name::verify(&decoder, &mut payload)?;
                Option::<LoweredType>::verify(&decoder, &mut payload)?;
                Option::<LoweredTypeCheck>::verify(&decoder, &mut payload)?;
                bool::verify(&decoder, &mut payload)?;
                BuildExprRow::verify(&decoder, &mut payload)?;
                Span::verify(&decoder, &mut payload)?;
            }
            FullDriverTag::LetRecord => {
                BuildExprRow::verify(&decoder, &mut payload)?;
                Vec::<(Name, usize)>::verify(&decoder, &mut payload)?;
                LoweredCompTarget::verify(&decoder, &mut payload)?;
                bool::verify(&decoder, &mut payload)?;
                Span::verify(&decoder, &mut payload)?;
            }
            FullDriverTag::Assign => {
                Name::verify(&decoder, &mut payload)?;
                AssignOp::verify(&decoder, &mut payload)?;
                BuildExprRow::verify(&decoder, &mut payload)?;
                Span::verify(&decoder, &mut payload)?;
            }
            FullDriverTag::Discard => {
                BuildExprRow::verify(&decoder, &mut payload)?;
                Span::verify(&decoder, &mut payload)?;
            }
            FullDriverTag::Stmt => BuildStmtRow::verify(&decoder, &mut payload)?,
            FullDriverTag::Expr => BuildExprRow::verify(&decoder, &mut payload)?,
            FullDriverTag::Defer => {
                BuildExprRow::verify(&decoder, &mut payload)?;
                Span::verify(&decoder, &mut payload)?;
            }
            FullDriverTag::SignalHook => {
                Name::verify(&decoder, &mut payload)?;
                Option::<String>::verify(&decoder, &mut payload)?;
                Vec::<BuildStmtId>::verify(&decoder, &mut payload)?;
                Vec::<LoweredTopLevelSlot>::verify(&decoder, &mut payload)?;
                let slot_count = payload.raw()?;
                if slot_count != step.slot_count {
                    return Err(IrVerifyError::new(
                        "signal hook slot count does not match its driver step",
                    ));
                }
                Span::verify(&decoder, &mut payload)?;
            }
        }
        payload.finish()?;
        decoder.finish_function()
    }
}

impl<'a> FullFunctionView<'a> {
    pub(in crate::runtime::eval) fn with_instantiation(self, id: InstantiationId) -> Result<Self, IrVerifyError> {
        let store = self.generic_evidence().ok_or_else(|| IrVerifyError::new("generic function entry has no evidence store"))?;
        if Some(store.instance(id)?.scope) != self.generic_scope() { return Err(IrVerifyError::new("generic function entry proof belongs to another scope")); }
        Ok(Self { instantiation: Some(id), ..self })
    }
    pub(in crate::runtime::eval) fn generic_scope(&self) -> Option<SchemeScopeId> {
        self.program.generic_evidence()?.scope_for_function(IrFunctionId::new(self.index).ok()?)
    }
    pub(in crate::runtime::eval) fn generic_evidence(&self) -> Option<&'a GenericEvidenceStore> { self.program.generic_evidence() }
    pub(in crate::runtime::eval) fn index(&self) -> usize {
        self.index
    }

    pub(in crate::runtime::eval) fn definition_span(&self) -> Result<Span, IrVerifyError> {
        self.program
            .function_definition_spans
            .get(self.index)
            .copied()
            .ok_or_else(|| IrVerifyError::new("function definition span is missing"))
    }

    pub(in crate::runtime::eval) fn instruction_tags(
        &self,
    ) -> Result<&'a [FullTag], IrVerifyError> {
        let range = self.program.store.function_instruction_range(self.index)?;
        Ok(&self.program.store.tags[range])
    }

    pub(in crate::runtime::eval) fn pipeline_stage_tags(
        &self,
    ) -> Result<Vec<FullStageTag>, IrVerifyError> {
        self.program
            .pipeline_stage_tags(self.program.store.function_instruction_range(self.index)?)
    }

    /// The function's header, decoded once and retained with this program.
    ///
    /// The first call for a function decodes it and stores it in the program's
    /// slot; every later call — from any evaluator, including worker
    /// evaluators, which share the program — reads the same `Arc`. The store is
    /// immutable once verified, so there is nothing to invalidate.
    pub(in crate::runtime::eval) fn header(&self) -> Result<Arc<FunctionHeader>, IrVerifyError> {
        // A program built without slots — the verifier fixtures construct one
        // directly — decodes without caching rather than failing.
        let Some(slot) = self.program.headers.get(self.index) else {
            return Ok(Arc::new(self.decode_header()?));
        };
        if let Some(header) = slot.get() {
            return Ok(Arc::clone(header));
        }
        let decoded = Arc::new(self.decode_header()?);
        // A concurrent worker may have filled the slot first; either value is
        // the same header, so the stored one wins.
        match slot.set(Arc::clone(&decoded)) {
            Ok(()) => Ok(decoded),
            Err(_) => Ok(Arc::clone(
                slot.get().expect("a losing `set` means the slot is filled"),
            )),
        }
    }

    fn decode_header(&self) -> Result<FunctionHeader, IrVerifyError> {
        let function = self.program.store.functions[self.index];
        let decoder = FullDecoder { store: &self.program.store, owner: IrFunctionId::new(self.index).map_err(|_| IrVerifyError::new("function id is invalid"))?.raw(), instruction_range: self.program.store.function_instruction_range(self.index)?, instruction_states: None, block_states: None, slot_count: function.slot_count, pattern_tree: None, pattern_ceiling: Cell::new(usize::MAX), verified: true };
        let params = function
            .params
            .bounds(self.program.store.params.len())
            .ok_or_else(|| IrVerifyError::new("function parameter range is invalid"))?;
        let captures = function
            .captures
            .bounds(self.program.store.captures.len())
            .ok_or_else(|| IrVerifyError::new("function capture range is invalid"))?;
        let mut param_names = SmallVec::new();
        let mut param_kinds = SmallVec::new();
        let mut param_checks = SmallVec::new();
        let mut param_rest = SmallVec::new();
        let mut param_defaults = SmallVec::new();
        for (offset, param) in self.program.store.params[params.clone()].iter().enumerate() {
            let param_index = params.start + offset;
            let cold = self
                .program
                .store
                .param_cold
                .binary_search_by_key(&(param_index as u32), |cold| cold.param)
                .ok()
                .map(|index| self.program.store.param_cold[index]);
            param_names.push(Name::intern(self.program.store.string(param.name)?));
            param_kinds.push(if param.type_id == IR_NONE {
                if self.generic_scope().is_none() { return Err(IrVerifyError::new("generic parameter lacks a scope")); }
                LoweredType::Generic
            } else { lowered_type_from_type(&self.program.store.semantic.to_type(TypeId::from_raw(param.type_id).ok_or_else(|| IrVerifyError::new("parameter type id is invalid"))?)?)? });
            param_checks.push(if cold.is_none_or(|cold| cold.validation == IR_NONE) {
                None
            } else {
                let validation_id = cold.expect("checked above").validation;
                let validation = self
                    .program
                    .store
                    .validations
                    .get(validation_id as usize)
                    .ok_or_else(|| IrVerifyError::new("validation id is out of bounds"))?;
                Some(LoweredTypeCheck {
                    schema: None,
                    ty: self.program.store.semantic.to_type(validation.type_id)?,
                    name: Arc::from(self.program.store.string(validation.name)?),
                })
            });
            param_rest.push(param.flags & 1 != 0);
            param_defaults.push(if param.flags & 4 != 0 {
                LoweredParamDefault::Expression
            } else if cold.is_none_or(|cold| cold.default == IR_NONE) {
                LoweredParamDefault::None
            } else {
                let raw = [cold.expect("checked above").default];
                let mut value = FullCursor::new(&raw);
                LoweredParamDefault::Constant(LoweredValue::decode(&decoder, &mut value)?)
            });
        }
        let mut decoded_captures = SmallVec::new();
        for capture in &self.program.store.captures[captures] {
            decoded_captures.push(LoweredTopLevelSlot {
                lexical_binding: None,
                host_binding: self.program.generic_evidence().map(|generic| generic.host_binding_capture_for_slot(IrFunctionId::new(self.index).map_err(|_| IrVerifyError::new("host capture function id is invalid"))?, capture.slot_and_flags & !(1 << 31))).transpose()?.flatten().map(|(_, capture)| capture.binding),
                source_type: None,
                name: Name::intern(self.program.store.string(capture.name)?),
                slot: (capture.slot_and_flags & !(1 << 31)) as usize,
                kind: callable_prepare::checked_storage_kind(&self.program.store.semantic, capture.type_id)?,
                mutable: capture.slot_and_flags & (1 << 31) != 0,
            });
        }
        let (return_kind, return_check) = if function.signature == IR_NONE {
            let scope = self.generic_scope().ok_or_else(|| IrVerifyError::new("generic signature lacks a scope"))?;
            let plan = self.generic_evidence().ok_or_else(|| IrVerifyError::new("generic signature lacks evidence"))?.scope(scope)?.return_plan;
            (match plan { GenericReturnPlan::Value => LoweredReturnKind::Plain(LoweredType::Generic), GenericReturnPlan::Result | GenericReturnPlan::ResultUnit => LoweredReturnKind::Result(LoweredType::Generic), GenericReturnPlan::Unit => LoweredReturnKind::Plain(LoweredType::Unit) }, None)
        } else {
            let signature = SignatureId::from_raw(function.signature).ok_or_else(|| IrVerifyError::new("function signature id is invalid"))?;
            let return_type = self.program.store.semantic.to_type(self.program.store.semantic.signature_return_type(signature)?)?;
            let return_check = return_type.has_unsigned_constraint().then(|| LoweredTypeCheck { name: Arc::from(return_type.to_string()), ty: return_type.clone(), schema: None });
            let return_kind = match return_type { Type::Result(ok, _) => LoweredReturnKind::Result(lowered_type_from_type(&ok)?), ty => LoweredReturnKind::Plain(lowered_type_from_type(&ty)?) };
            (return_kind, return_check)
        };
        Ok(FunctionHeader {
            params: param_names,
            param_kinds,
            param_checks,
            param_rest,
            param_defaults,
            captures: decoded_captures,
            return_kind,
            return_check,
            slot_count: function.slot_count as usize,
        })
    }

    pub(in crate::runtime::eval) fn execution(&self) -> Result<FullExecution<'a>, IrVerifyError> {
        self.program.store.verify_generic_owner()?;
        if self.generic_scope().is_some() && self.instantiation.is_none() { return Err(IrVerifyError::new("generic function entry lacks prepared evidence")); }
        let function = self.program.store.functions[self.index];
        Ok(FullExecution {
            instantiation: self.instantiation,
            decoder: FullDecoder {
                store: &self.program.store,
                owner: IrFunctionId::new(self.index)
                    .map_err(|_| IrVerifyError::new("function id is invalid"))?
                    .raw(),
                instruction_range: self.program.store.function_instruction_range(self.index)?,
                instruction_states: None,
                block_states: None,
                slot_count: function.slot_count,
                pattern_tree: None, pattern_ceiling: Cell::new(usize::MAX),
                verified: true,
            },
        })
    }

    pub(in crate::runtime::eval) fn body(
        &self,
        execution: &FullExecution<'a>,
    ) -> Result<(IrBlockId, FullPayload<'a>), IrVerifyError> {
        execution.block_id(
            self.program.store.functions[self.index].body,
            BLOCK_STATEMENTS,
        )
    }

    pub(in crate::runtime::eval) fn slot_count(&self) -> usize {
        self.program.store.functions[self.index].slot_count as usize
    }

    pub(in crate::runtime::eval) fn function_id(&self) -> IrFunctionId {
        IrFunctionId::new(self.index).expect("function views retain a valid function index")
    }

    pub(in crate::runtime::eval) fn belongs_to_program(&self, program: &FullProgram) -> bool {
        std::ptr::eq(self.program, program)
    }

    pub(in crate::runtime::eval) fn has_defers(&self) -> bool {
        self.program.store.function_metadata[self.index].flags & 2 != 0
    }
}

impl<'a> FullDriverStepView<'a> {
    pub(in crate::runtime::eval) fn index(&self) -> usize {
        self.index
    }

    pub(in crate::runtime::eval) fn tag(&self) -> FullDriverTag {
        self.program.store.driver_steps[self.index].tag
    }

    pub(in crate::runtime::eval) fn source_span(&self) -> Result<Span, IrVerifyError> {
        let step = self.program.store.driver_steps[self.index];
        let execution = self.execution()?;
        let words = [step.location];
        let mut cursor = FullCursor::new(&words);
        let span = Span::decode(&execution.decoder, &mut cursor)?;
        cursor.finish()?;
        Ok(span)
    }

    pub(in crate::runtime::eval) fn execution(&self) -> Result<FullExecution<'a>, IrVerifyError> {
        self.program.store.verify_generic_owner()?;
        let step = self.program.store.driver_steps[self.index];
        Ok(FullExecution {
            instantiation: None,
            decoder: FullDecoder {
                store: &self.program.store,
                owner: driver_owner(self.index)
                    .map_err(|_| IrVerifyError::new("driver owner is invalid"))?,
                instruction_range: self.program.store.driver_instruction_range(self.index)?,
                instruction_states: None,
                block_states: None,
                slot_count: step.slot_count,
                pattern_tree: None, pattern_ceiling: Cell::new(usize::MAX),
                verified: true,
            },
        })
    }

    pub(in crate::runtime::eval) fn instruction_tags(
        &self,
    ) -> Result<&'a [FullTag], IrVerifyError> {
        let range = self.program.store.driver_instruction_range(self.index)?;
        Ok(&self.program.store.tags[range])
    }

    pub(in crate::runtime::eval) fn pipeline_stage_tags(
        &self,
    ) -> Result<Vec<FullStageTag>, IrVerifyError> {
        self.program
            .pipeline_stage_tags(self.program.store.driver_instruction_range(self.index)?)
    }

    pub(in crate::runtime::eval) fn payload(&self) -> Result<FullPayload<'a>, IrVerifyError> {
        let step = self.program.store.driver_steps[self.index];
        Ok(FullPayload {
            cursor: FullCursor::verified(self.program.store.payload(step.data.range())?),
        })
    }

    pub(in crate::runtime::eval) fn slot_count(&self) -> usize {
        self.program.store.driver_steps[self.index].slot_count as usize
    }

    pub(in crate::runtime::eval) fn slots(&self) -> Result<LoweredTopLevelSlots, IrVerifyError> {
        let step = self.program.store.driver_steps[self.index];
        let range = step
            .slots
            .bounds(self.program.store.driver_slots.len())
            .ok_or_else(|| IrVerifyError::new("driver slot range is invalid"))?;
        let mut slots = SmallVec::new();
        for slot in &self.program.store.driver_slots[range] {
            slots.push(LoweredTopLevelSlot {
                host_binding: None,
                lexical_binding: None,
                source_type: None,
                name: Name::intern(self.program.store.string(slot.name)?),
                slot: slot.slot as usize,
                kind: callable_prepare::checked_storage_kind(&self.program.store.semantic, slot.type_id)?,
                mutable: slot.flags & DRIVER_SLOT_MUTABLE != 0,
            });
        }
        Ok(slots)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FullCheckpoint {
    stage_block_callback_rows: usize,
    value_binding_rows: usize,
    value_use_rows: usize,
    iteration_binding_rows: usize,
    iteration_use_rows: usize,
    argument_binding_rows: usize,
    callable_binding_rows: usize,
    callable_use_rows: usize,
    callable_receiver_rows: usize,
    original_callable_capture_rows: usize,
    saved_native_receiver_rows: usize,
    result_receiver_rows: usize,
    host_binding_read_rows: usize,
    lexical_capture_read_rows: usize,
    conditional_result_rows: usize,
    path_source_rows: usize,
    container_creation_check_rows: usize,
    byte_at_fallback_rows: usize,
    encoded_slot_uses: usize,
    tags: usize,
    extra: usize,
    patterns: usize,
    stages: usize,
    values: usize,
    blocks: usize,
    driver_steps: usize,
    driver_slots: usize,
    driver_sync: usize,
    driver_regions: usize,
    driver_programs: usize,
    driver_root: u32,
    params: usize,
    captures: usize,
    validations: usize,
    strings: usize,
    string_bytes: usize,
    bytes: usize,
    byte_data: usize,
    prepared_regexes: usize,
    prepared_constants: usize,
    wire_enums: usize,
    prepared_schemas: usize,
    prepared_cli_plans: usize,
    locations: usize,
    runtime_ops: usize,
    assign_ops: usize,
    binary_ops: usize,
    run_kinds: usize,
    redirection_kinds: usize,
    semantic: super::semantic::SemanticCheckpoint,
    generic: Option<GenericCheckpoint>,
    generic_expression_rows: usize,
    generic_stage_call_rows: usize,
    generic_pattern_rows: usize,
    pattern_capture_slot_rows: usize,
    pattern_capture_source_rows: usize,
    pattern_use_rows: usize,
    generic_pattern_statement_use_rows: usize,
    generic_pattern_admissions: usize,
}

#[derive(Default)]
pub(in crate::runtime::eval) struct FullBuilder {
    stage_block_callback_rows: Vec<super::generic::OriginalStageBlockCallback>,
    active_pattern_admissions: FxHashMap<super::super::BuildPatternControlRow, Vec<(BuildExprId, super::super::BuildPatternAdmission)>>,
    active_pattern_statements: FxHashMap<BuildStmtId, u32>,
    generic_pattern_admissions: Vec<(u32, super::pattern::PreparedPatternAdmission, InstructionOwner, Option<Box<super::pattern::PreparedPatternConditionalResult>>)>,
    active_value_bindings: FxHashMap<BuildStmtId, (crate::sema::check::BindingIdentity, super::super::BuildValueBindingOrigin)>,
    active_with_value_bindings: FxHashMap<BuildStmtId, Vec<(crate::sema::check::WithBindingIdentity, super::super::BuildValueBindingOrigin)>>,
    active_guard_error_bindings: FxHashMap<BuildStmtId, (crate::sema::check::GuardErrorBindingIdentity, super::super::BuildValueBindingOrigin)>,
    value_binding_rows: Vec<(super::generic::ValueBindingIdentity, super::super::BuildValueBindingOrigin, u32, u32, InstructionOwner)>,
    value_use_rows: Vec<(super::generic::OperationSourceOrigin, super::generic::ValueBindingIdentity, u32, InstructionOwner)>,
    active_iteration_bindings: FxHashMap<BuildStmtId, super::super::BuildIterationBindingOrigin>,
    iteration_binding_rows: Vec<(super::super::BuildIterationBindingOrigin, u32, u32, u32, IrBlockId, InstructionOwner)>,
    iteration_use_rows: Vec<(crate::sema::check::ExpressionIdentity, crate::sema::check::BindingIdentity, u32, InstructionOwner)>,
    active_argument_wrappers: FxHashMap<BuildExprId, Vec<usize>>,
    argument_binding_rows: Vec<(super::super::BuildArgumentBindingOrigin, u32, InstructionOwner, Option<(u32, u32, u32)>)>,
    folded_native_receiver_rows: Vec<(u32, BuildFoldedNativeReceiver, InstructionOwner)>,
    literal_comparison_rows: Vec<(u32, BuildLiteralComparison, InstructionOwner)>,
    record_constructor_rows: Vec<constructor_prepare::StagedRecordConstructor>,
    constant_source_rows: Vec<(super::generic::OriginalConstantSource, u32, InstructionOwner)>,
    record_update_rows: Vec<record_update_prepare::StagedRecordUpdate>,
    conditional_result_rows: Vec<super::generic::ConditionalResultSource>,
    path_source_rows: Vec<path_prepare::PathSourceRow>,
    container_creation_check_rows: Vec<(BuildContainerCreationCheck, u32, u32, InstructionOwner)>,
    byte_at_fallback_rows: Vec<(u32, BuildByteAtFallbackOriginal, InstructionOwner)>,
    encoded_slot_uses: Vec<(u32, usize)>,
    error_constructor_rows: Vec<error_constructor_prepare::StagedErrorConstructor>,
    host_binding_read_rows: Vec<(u32, super::super::lower::host_bindings::BuildHostBindingRead, InstructionOwner)>,
    lexical_capture_read_rows: Vec<(u32, super::super::lower::lexical_captures::BuildLexicalCaptureRead, InstructionOwner)>,
    index_rows: Vec<(BuildIndexOrigin, u32, InstructionOwner)>,
    record_source_rows: Vec<(super::super::lower::record_binding::OriginalRecordSource, u32, InstructionOwner, Box<[(u32, u32)]>)>,
    compiler_argument_wrappers: FxHashMap<u32, argument_prepare::CompilerArgumentWrapper>,
    optional_receiver_guards: FxHashMap<u32, super::generic::OriginalOptionalReceiverGuard>,
    prepared_argument_origins: FxHashMap<u32, (crate::sema::check::ExpressionIdentity, InstructionOwner)>,
    prepared_saved_argument_bindings: FxHashMap<u32, super::generic::OriginalArgumentBinding>,
    active_callable_bindings: FxHashMap<BuildStmtId, (crate::sema::check::BindingIdentity, super::super::BuildCallableBindingOrigin)>,
    active_encoded_expressions: FxHashMap<BuildExprId, u32>,
    callable_binding_rows: Vec<(crate::sema::check::BindingIdentity, super::super::BuildCallableBindingOrigin, u32, u32, InstructionOwner)>,
    original_callable_capture_rows: Vec<(IrFunctionId, crate::sema::check::DeclarationIdentity, u32, LoweredTopLevelSlot)>,
    callable_use_rows: Vec<super::generic::OriginalCallableUse>,
    callable_receiver_rows: Vec<(super::super::BuildCallableReceiverOrigin, u32, InstructionOwner, Option<(u32, u32, u32)>)>,
    active_callable_receiver_wrappers: FxHashMap<BuildExprId, Vec<usize>>,
    saved_native_receiver_rows: Vec<(BuildSavedNativeReceiverOrigin, u32, InstructionOwner, Option<(u32, u32, u32, u32)>)>,
    result_receiver_rows: Vec<(super::super::lower::BuildResultReceiver, u32, InstructionOwner, u32)>,
    active_native_receiver_wrappers: FxHashMap<BuildExprId, Vec<usize>>,
    source_lowering_stats: super::super::FrontendLoweredStats,
    store: FullStore,
    semantic: SemanticPoolBuilder,
    generic: Option<GenericEvidenceBuilder>,
    solved: Option<Arc<crate::sema::check::SolvedTypes>>,
    generic_declarations: BTreeMap<crate::sema::check::DeclarationIdentity, SchemeScopeId>,
    declaration_functions: BTreeMap<crate::sema::check::DeclarationIdentity, IrFunctionId>,
    generic_schemes: FxHashMap<SchemeScopeId, crate::sema::inference::SchemeId>,
    generic_projection_uses: BTreeMap<crate::sema::check::ExpressionIdentity, (SchemeScopeId, u32)>,
    active_expression_origins: FxHashMap<BuildExprId, crate::sema::check::ExpressionIdentity>,
    generic_expression_rows: Vec<(u32, crate::sema::check::ExpressionIdentity, InstructionOwner)>,
    active_stage_call_origins: FxHashMap<BuildExprId, crate::sema::check::StageIdentity>,
    generic_stage_call_rows: Vec<(u32, crate::sema::check::StageIdentity, InstructionOwner)>,
    generic_pattern_rows: Vec<(u32, crate::sema::check::PatternIdentity, InstructionOwner)>,
    pattern_capture_slots: BTreeMap<crate::sema::check::PatternCaptureIdentity, u32>,
    pattern_capture_slot_rows: Vec<crate::sema::check::PatternCaptureIdentity>,
    pattern_capture_sources: BTreeSet<crate::sema::check::PatternIdentity>,
    pattern_capture_source_rows: Vec<crate::sema::check::PatternIdentity>,
    pattern_use_origins: BTreeMap<crate::sema::check::ExpressionIdentity, crate::sema::check::PatternCaptureIdentity>,
    pattern_use_rows: Vec<crate::sema::check::ExpressionIdentity>,
    generic_pattern_statement_use_rows: Vec<(u32, crate::sema::check::StatementIdentity, crate::sema::check::PatternCaptureIdentity, InstructionOwner)>,
    strings: BTreeMap<String, IrStringId>,
    bytes: BTreeMap<Vec<u8>, super::IrBytesId>,
    locations: BTreeMap<(SourceId, u32, u32), IrLocationId>,
    function_ids: BTreeMap<LoweredFunctionKey, IrFunctionId>,
    payload_pool: Vec<Vec<u32>>,
    function_definition_spans: Vec<Span>,
    current_owner: Option<u32>,
    current_slot_count: u32,
    active_scratch: Option<Rc<RefCell<BuildScratch>>>,
}

impl FullBuilder {
    pub(in crate::runtime::eval) fn generic_evidence_mut(&mut self) -> &mut GenericEvidenceBuilder { self.generic.get_or_insert_with(GenericEvidenceBuilder::default) }
    pub(in crate::runtime::eval) fn generic_function_id(&self, function: LoweredFunctionKey) -> Option<IrFunctionId> { self.function_ids.get(&function).copied() }
    pub(in crate::runtime::eval) fn intern_generic_ground_type(&mut self, ty: &Type) -> Result<TypeId, IrBuildError> { self.semantic.intern_type(&mut self.store.semantic, ty) }
    fn stage_pattern_origin(&mut self, pattern: u32, origin: crate::sema::check::PatternIdentity, owner: InstructionOwner, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        self.generic_pattern_rows.push((pattern, origin, owner));
        let solved = self.solved.as_ref().ok_or_else(|| IrBuildError::format("pattern_original_graph_missing", None, 0, 0))?;
        let mut pending = vec![(origin, 0usize)];
        while let Some((identity, depth)) = pending.pop() {
            if self.pattern_capture_sources.contains(&identity) { continue; }
            if depth >= 512 { return Err(IrBuildError::format("pattern_original_forest_depth", None, 0, 0)); }
            let source = solved.patterns.get(&identity).ok_or_else(|| IrBuildError::format("pattern_original_source_missing", None, 0, 0))?;
            for capture in &source.captures {
                let slot = scratch.pattern_capture_slots.get(&capture.identity).copied().ok_or_else(|| IrBuildError::format("pattern_original_capture_allocation_missing", None, 0, 0))?;
                let slot = u32::try_from(slot).map_err(|_| IrBuildError::format("pattern_capture_slot_overflow", None, 0, 0))?;
                if let Some(previous) = self.pattern_capture_slots.insert(capture.identity, slot) {
                    if previous != slot { return Err(IrBuildError::format("pattern_original_capture_allocation_changed", None, 0, 0)); }
                } else { self.pattern_capture_slot_rows.push(capture.identity); }
            }
            pending.extend(source.children.iter().map(|&child| (child, depth + 1)));
            self.pattern_capture_sources.insert(identity);
            self.pattern_capture_source_rows.push(identity);
        }
        Ok(())
    }
    fn stage_pattern_expression_use(&mut self, origin: crate::sema::check::ExpressionIdentity, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        let Some(&capture) = scratch.pattern_use_origins.get(&origin) else { return Ok(()); };
        if let Some(previous) = self.pattern_use_origins.insert(origin, capture) {
            if previous != capture { return Err(IrBuildError::format("pattern_original_capture_use_changed", None, 0, 0)); }
        } else { self.pattern_use_rows.push(origin); }
        Ok(())
    }
    fn new(source_id: SourceId) -> Self {
        Self {
            store: FullStore {
                source_id,
                ..FullStore::default()
            },
            ..Self::default()
        }
    }

    fn stage_callable_scratch(&mut self, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        self.active_value_bindings.clear();
        for (&binding, original) in &scratch.value_binding_origins {
            if self.active_value_bindings.insert(original.row, (binding, original.clone())).is_some() {
                return Err(IrBuildError::format("value_binding_allocation_ambiguous", None, 0, 0));
            }
        }
        self.active_with_value_bindings.clear();
        for (&binding, original) in &scratch.with_value_binding_origins {
            self.active_with_value_bindings.entry(original.row).or_default().push((binding, original.clone()));
        }
        self.active_guard_error_bindings.clear();
        for (&binding, original) in &scratch.guard_error_binding_origins {
            if self.active_guard_error_bindings.insert(original.row, (binding, original.clone())).is_some() {
                return Err(IrBuildError::format("guard_error_allocation_ambiguous", None, 0, 0));
            }
        }
        self.active_pattern_admissions.clear();
        self.active_pattern_statements.clear();
        for (&matcher, original) in &scratch.pattern_admissions {
            self.active_pattern_admissions.entry(original.control).or_default().push((matcher, original.clone()));
        }
        self.active_iteration_bindings.clear();
        for (&binding, original) in &scratch.iteration_binding_origins {
            if binding != original.binding || self.active_iteration_bindings.insert(original.row, original.clone()).is_some() {
                return Err(IrBuildError::format("iteration_binding_allocation_ambiguous", None, 0, 0));
            }
        }
        self.active_argument_wrappers.clear();
        self.active_callable_receiver_wrappers.clear();
        self.active_native_receiver_wrappers.clear();
        self.active_callable_bindings.clear();
        self.active_encoded_expressions.clear();
        for (&binding, original) in &scratch.callable_binding_origins {
            if self.active_callable_bindings.insert(original.row, (binding, original.clone())).is_some() {
                return Err(IrBuildError::format("callable_binding_allocation_ambiguous", None, 0, 0));
            }
        }
        Ok(())
    }

    fn stage_iteration_expression_use(&mut self, instruction: u32, origin: crate::sema::check::ExpressionIdentity, owner: InstructionOwner, scratch: &BuildScratch) -> Result<(), IrBuildError> {
        if let Some(&binding) = scratch.iteration_binding_uses.get(&origin) {
            if !scratch.iteration_binding_origins.contains_key(&binding) { return Err(IrBuildError::format("iteration_use_original_binding_missing", None, 0, 0)); }
            self.iteration_use_rows.push((origin, binding, instruction, owner));
        }
        Ok(())
    }

    fn reserve_function_keys(
        &mut self,
        keys: impl IntoIterator<Item = LoweredFunctionKey>,
    ) -> Result<(), IrBuildError> {
        for key in keys {
            let function_id = IrFunctionId::new(self.function_ids.len())?;
            if function_id.raw() & DRIVER_OWNER_BIT != 0 {
                return Err(IrBuildError::format(
                    "function_owner_overflow",
                    None,
                    0,
                    self.store.tags.len(),
                ));
            }
            self.function_ids.insert(key, function_id);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::runtime::eval) fn build(
        units: &[LoweredFunctionUnit],
        sources: Arc<SourceMap>,
        source_id: SourceId,
    ) -> Result<FullProgram, IrBuildError> {
        Self::build_with_driver(units, None, sources, source_id)
    }

    #[cfg(test)]
    fn build_with_driver(
        units: &[LoweredFunctionUnit],
        driver: Option<(&ProgramBuild, &[StmtId], &ArenaProgram)>,
        sources: Arc<SourceMap>,
        source_id: SourceId,
    ) -> Result<FullProgram, IrBuildError> {
        let mut builder = Self::new(source_id);
        let mut units = units.iter().collect::<Vec<_>>();
        units.sort_by_key(|unit| (unit.source_span().start(), unit.key().display_name()));
        builder.predeclare(&units)?;
        for unit in units {
            let body = unit.lowered_body().ok_or_else(|| {
                IrBuildError::format(
                    "full_ir_function_blocker",
                    unit.blocker_detail.as_ref().map(|(span, _)| *span),
                    0,
                    builder.store.tags.len(),
                )
            })?;
            let checkpoint = builder.checkpoint();
            let function = builder.function_ids[&unit.key()];
            builder.current_owner = Some(function.raw());
            builder.current_slot_count = body.slot_count as u32;
            let result = builder.encode_body(function, &body);
            builder.current_owner = None;
            builder.current_slot_count = 0;
            if let Err(mut error) = result {
                error.attempted_instructions =
                    builder.store.tags.len().saturating_sub(checkpoint.tags);
                builder.rewind(checkpoint);
                error.committed_instructions = builder.store.tags.len();
                return Err(error);
            }
        }
        if let Some((driver, source_statements, arena)) = driver {
            let checkpoint = builder.checkpoint();
            if let Err(mut error) =
                builder.encode_driver_root(driver, source_statements, arena, false)
            {
                error.attempted_instructions =
                    builder.store.tags.len().saturating_sub(checkpoint.tags);
                builder.rewind(checkpoint);
                error.committed_instructions = builder.store.tags.len();
                return Err(error);
            }
        }
        builder.finish(sources, crate::symbol::SymbolOwner::new())
    }

    fn finish(
        self,
        sources: Arc<SourceMap>,
        symbols: crate::symbol::SymbolOwner,
    ) -> Result<FullProgram, IrBuildError> {
        let stats = self.source_lowering_stats;
        self.finish_inner(sources, symbols).map_err(|mut error| { error.source_lowering_stats = Some(stats); error })
    }

    fn finish_inner(mut self, sources: Arc<SourceMap>, symbols: crate::symbol::SymbolOwner) -> Result<FullProgram, IrBuildError> {
        for (_, original, instruction, _, owner) in self.value_binding_rows.clone() {
            self.generic_evidence_mut().register_instruction_origin(instruction, super::generic::OperationSourceOrigin::Statement(original.statement), owner).map_err(|_| IrBuildError::format("original_value_binding_registration", None, 0, 0))?;
        }
        for (original, instruction, _, _, _, owner) in self.iteration_binding_rows.clone() {
            self.generic_evidence_mut().register_instruction_origin(instruction, super::generic::OperationSourceOrigin::Statement(original.statement), owner).map_err(|_| IrBuildError::format("original_iteration_binding_registration", None, 0, 0))?;
        }
        for (_, original, instruction, _, owner) in self.callable_binding_rows.clone() {
            self.generic_evidence_mut().register_instruction_origin(instruction, super::generic::OperationSourceOrigin::Statement(original.statement), owner).map_err(|_| IrBuildError::format("original_callable_binding_registration", None, 0, 0))?;
        }
        for (instruction, origin, owner) in self.generic_expression_rows.clone() {
            self.generic_evidence_mut().register_instruction_origin(instruction, super::generic::OperationSourceOrigin::Expression(origin), owner).map_err(|_| IrBuildError::format("original_expression_registration", None, 0, 0))?;
        }
        for (instruction, origin, owner) in self.generic_stage_call_rows.clone() {
            self.generic_evidence_mut().register_instruction_origin(instruction, super::generic::OperationSourceOrigin::Stage(origin), owner).map_err(|_| IrBuildError::format("original_stage_registration", None, 0, 0))?;
        }
        for (pattern, origin, owner) in self.generic_pattern_rows.clone() {
            self.generic_evidence_mut().register_pattern_origin(pattern, origin, owner);
        }
        for (instruction, origin, _, owner) in self.generic_pattern_statement_use_rows.clone() {
            self.generic_evidence_mut().register_instruction_origin(instruction, super::generic::OperationSourceOrigin::Statement(origin), owner).map_err(|_| IrBuildError::format("original_pattern_statement_registration", None, 0, 0))?;
        }
        self.prepare_constant_sources()?;
        self.prepare_host_bindings()?;
        self.prepare_lexical_captures()?;
        self.prepare_original_argument_bindings()?;
        self.prepare_generic_expressions()?;
        self.prepare_original_iteration_bindings()?;
        self.prepare_native_callable_values()?;
        self.prepare_callable_values()?;
        self.prepare_ground_containers()?;
        self.prepare_native_calls()?;
        self.prepare_cli_calls()?;
        self.prepare_compiler_argument_wrappers()?;
        self.prepare_record_constructors()?;
        self.prepare_error_constructors()?;
        self.prepare_embedded_bridges()?;
        self.prepare_value_bindings()?;
        self.prepare_native_scalar_sources()?;
        self.prepare_ground_projections()?;
        self.prepare_mutable_refinements()?;
        self.prepare_record_sources()?;
        self.prepare_record_updates()?;
        self.prepare_conditional_results()?;
        self.prepare_formatted_paths()?;
        self.prepare_pattern_evidence()?;
        self.prepare_source_operations()?;
        self.prepare_duration_operations()?;
        self.prepare_range_operations()?;
        self.prepare_stage_pipelines()?;
        self.prepare_original_indices()?;
        self.store.semantic.seal_original_contract();
        if let Some(generic) = self.generic.take() {
            let owners = self.store.generic_instruction_owners().map_err(|_| IrBuildError::format("generic_instruction_owners", None, 0, 0))?;
            self.store.generic = Some(Box::new(generic.finish(&self.store.semantic, self.store.functions.len(), &owners)
                .map_err(|error| IrBuildError::verification("generic_evidence_verification", error))?));
            self.store.original_generic_owner = self.store.generic.as_deref().map(GenericEvidenceStore::program_owner);
        }
        self.store.shrink_to_fit();
        let headers = (0..self.store.functions.len())
            .map(|_| std::sync::OnceLock::new())
            .collect();
        let program = FullProgram {
            source_lowering_stats: self.source_lowering_stats,
            store: self.store,
            sources,
            symbols,
            function_definition_spans: self.function_definition_spans,
            headers,
            identities: std::sync::OnceLock::new(),
        };
        FullVerifier::verify(&program)
            .map_err(|error| IrBuildError::verification("full_ir_verification", error))?;
        Ok(program)
    }

    /// Build a program whose standard-library calls resolve to implementations
    /// a loading program already prepared.
    pub(in crate::runtime::eval) fn build_compact_external_stdlib(
        program: &ArenaProgram,
        declarations: &CompactDeclOutput,
        bodies: &CompactBodyProbeOutput,
        source: &str,
        sources: Arc<SourceMap>,
        source_id: SourceId,
    ) -> Result<FullProgram, IrBuildError> {
        Self::build_compact_with_options(
            program,
            declarations,
            bodies,
            source,
            sources,
            source_id,
            false,
            super::super::lower::StdlibLowerLinkage::External,
        )
    }

    pub(in crate::runtime::eval) fn build_compact(
        program: &ArenaProgram,
        declarations: &CompactDeclOutput,
        bodies: &CompactBodyProbeOutput,
        source: &str,
        sources: Arc<SourceMap>,
        source_id: SourceId,
    ) -> Result<FullProgram, IrBuildError> {
        Self::build_compact_with_options(
            program,
            declarations,
            bodies,
            source,
            sources,
            source_id,
            false,
            super::super::lower::StdlibLowerLinkage::Local,
        )
    }

    pub(in crate::runtime::eval) fn build_compact_with_options(
        program: &ArenaProgram,
        declarations: &CompactDeclOutput,
        bodies: &CompactBodyProbeOutput,
        source: &str,
        sources: Arc<SourceMap>,
        source_id: SourceId,
        allow_checker_only: bool,
        stdlib_linkage: super::super::lower::StdlibLowerLinkage,
    ) -> Result<FullProgram, IrBuildError> {
        let symbols = program.symbol_owner().clone();
        symbols.clone().with_current(|| {
            Self::build_compact_with_options_inner(
                program,
                declarations,
                bodies,
                source,
                sources,
                source_id,
                allow_checker_only,
                stdlib_linkage,
                symbols,
            )
        })
    }

    fn build_compact_with_options_inner(
        program: &ArenaProgram,
        declarations: &CompactDeclOutput,
        bodies: &CompactBodyProbeOutput,
        source: &str,
        sources: Arc<SourceMap>,
        source_id: SourceId,
        allow_checker_only: bool,
        stdlib_linkage: super::super::lower::StdlibLowerLinkage,
        symbols: crate::symbol::SymbolOwner,
    ) -> Result<FullProgram, IrBuildError> {
        let mut builder = Self::new(source_id);
        builder.solved = Some(Arc::clone(&bodies.solved));
        builder.reserve_function_keys(super::super::lower::compact_emitted_function_keys(program, declarations))?;
        let mut pures = rustc_hash::FxHashSet::default();
        let mut procs = rustc_hash::FxHashSet::default();
        let mut qualified_pures = rustc_hash::FxHashSet::default();
        let mut qualified_procs = rustc_hash::FxHashSet::default();
        super::super::lower::lower_compact_function_units_into(
            program,
            declarations,
            bodies,
            source,
            &sources,
            stdlib_linkage,
            |mut unit| {
                builder.accumulate_source_lowering_stats(unit.source_lowering_stats());
                builder.predeclare(&[&unit])?;
                let body = unit.take_lowered_body().ok_or_else(|| {
                    IrBuildError::format(
                        "full_ir_function_blocker",
                        unit.blocker_detail.as_ref().map(|(span, _)| *span),
                        0,
                        builder.store.tags.len(),
                    )
                })?;
                let checkpoint = builder.checkpoint();
                let function = builder.function_ids[&unit.key()];
                builder.current_owner = Some(function.raw());
                builder.current_slot_count = body.slot_count as u32;
                let result = builder.encode_body(function, &body);
                builder.current_owner = None;
                builder.current_slot_count = 0;
                if let Err(mut error) = result {
                    error.attempted_instructions =
                        builder.store.tags.len().saturating_sub(checkpoint.tags);
                    builder.rewind(checkpoint);
                    error.committed_instructions = builder.store.tags.len();
                    return Err(error);
                }
                match (unit.key(), unit.kind()) {
                    (LoweredFunctionKey::Name(name), LoweredFunctionKind::Pure) => {
                        pures.insert(name);
                    }
                    (LoweredFunctionKey::Name(name), LoweredFunctionKind::Proc) => {
                        procs.insert(name);
                    }
                    (LoweredFunctionKey::Qualified(name), LoweredFunctionKind::Pure) => {
                        qualified_pures.insert(name);
                    }
                    (LoweredFunctionKey::Qualified(name), LoweredFunctionKind::Proc) => {
                        qualified_procs.insert(name);
                    }
                }
                Ok(())
            },
        ).map_err(|mut error| { error.source_lowering_stats = Some(builder.source_lowering_stats); error })?;
        let functions = super::super::LowerableFunctions::all(
            &pures,
            &procs,
            &qualified_pures,
            &qualified_procs,
        );
        let (driver, probe_output) = super::super::lower::lower_compact_top_level_program_with_probe(
            program,
            declarations,
            bodies,
            source,
            &sources,
            &functions,
        );
        builder.accumulate_source_lowering_stats(super::super::FrontendLoweredStats {
            function_count: probe_output.functions,
            constructed_functions: probe_output.constructed_functions,
            statement_count: probe_output.statements,
            expression_count: probe_output.expressions,
            pattern_count: probe_output.patterns,
            blocker_events: probe_output.blocker_events,
            retained_estimate_bytes: 0,
        });
        let source_statements = program.statement_ids().collect::<Vec<_>>();
        drop(pures);
        drop(procs);
        drop(qualified_pures);
        drop(qualified_procs);

        let checkpoint = builder.checkpoint();
        if let Err(mut error) =
            builder.encode_driver_root(&driver, &source_statements, program, allow_checker_only)
        {
            error.attempted_instructions = builder.store.tags.len().saturating_sub(checkpoint.tags);
            builder.rewind(checkpoint);
            error.committed_instructions = builder.store.tags.len();
            error.source_lowering_stats = Some(builder.source_lowering_stats);
            return Err(error);
        }
        builder.finish(sources, symbols)
    }

    fn accumulate_source_lowering_stats(&mut self, stats: super::super::FrontendLoweredStats) {
        self.source_lowering_stats.function_count += stats.function_count;
        self.source_lowering_stats.constructed_functions += stats.constructed_functions;
        self.source_lowering_stats.statement_count += stats.statement_count;
        self.source_lowering_stats.expression_count += stats.expression_count;
        self.source_lowering_stats.pattern_count += stats.pattern_count;
        self.source_lowering_stats.blocker_events += stats.blocker_events;
    }

    fn predeclare(&mut self, units: &[&LoweredFunctionUnit]) -> Result<(), IrBuildError> {
        for unit in units {
            let body = unit.lowered_body().ok_or_else(|| {
                IrBuildError::format(
                    "full_ir_function_blocker",
                    unit.blocker_detail.as_ref().map(|(span, _)| *span),
                    0,
                    self.store.tags.len(),
                )
            })?;
            let function_id = self
                .function_ids
                .get(&unit.key())
                .copied()
                .unwrap_or(IrFunctionId::new(self.store.functions.len())?);
            if function_id.index() != self.store.functions.len() {
                return Err(IrBuildError::format(
                    "function_declaration_order",
                    Some(unit.source_span()),
                    0,
                    self.store.tags.len(),
                ));
            }
            if function_id.raw() & DRIVER_OWNER_BIT != 0 {
                return Err(IrBuildError::format(
                    "function_owner_overflow",
                    Some(unit.source_span()),
                    0,
                    self.store.tags.len(),
                ));
            }
            let name = self.intern_function_key(unit.key())?;
            let owner = unit
                .owner()
                .map(|name| self.intern_string(&name.as_str()))
                .transpose()?
                .map_or(IR_NONE, IrStringId::raw);
            let generic_scope = self.prepare_generic_scope(function_id, &body)?;
            if let Some(declaration) = body.solved_declaration {
                if self.declaration_functions.insert(declaration, function_id).is_some() {
                    return Err(IrBuildError::format("duplicate_source_declaration", Some(unit.source_span()), 0, self.store.tags.len()));
                }
            }
            let generic_parameters = generic_scope.map(|scope| self.generic.as_ref().expect("scope has builder").scope(scope).expect("scope was prepared").parameters.clone());
            let grounded_signature = if generic_scope.is_none() {
                if let (Some(identity), Some(solved)) = (body.solved_declaration, self.solved.as_ref()) {
                    let declaration = solved.declarations.get(&identity).ok_or_else(|| IrBuildError::format("missing_ground_declaration", None, 0, 0))?;
                    let signature = solved.graph.resolved(declaration.signature).map_err(|_| IrBuildError::format("invalid_ground_signature", None, 0, 0))?;
                    let crate::sema::inference::TypeNode::Arrow(arrow) = solved.graph.node(signature).map_err(|_| IrBuildError::format("invalid_ground_signature", None, 0, 0))? else { return Err(IrBuildError::format("invalid_ground_signature", None, 0, 0)); };
                    let parameters = arrow.params.iter().map(|parameter| super::generic::graph_ground_type(&solved.graph, parameter.ty).map_err(|_| IrBuildError::format("unresolved_ground_parameter", None, 0, 0))).collect::<Result<Vec<_>, _>>()?;
                    let result = super::generic::graph_ground_type(&solved.graph, arrow.result).map_err(|_| IrBuildError::format("unresolved_ground_result", None, 0, 0))?;
                    Some((parameters, result))
                } else if body.solved_declaration.is_none() {
                    body.legacy_checked_signature.clone()
                } else {
                    return Err(IrBuildError::format("missing_ground_signature_owner", None, 0, 0));
                }
            } else { None };
            if let Some((parameters, _)) = &grounded_signature {
                if parameters.len() != body.params.len() || body.param_kinds.len() != body.params.len() || body.param_defaults.len() != body.params.len() || body.param_rest.len() != body.params.len() || body.param_checks.len() != body.params.len() {
                    return Err(IrBuildError::format("ground_parameter_arity", None, 0, 0));
                }
                for (index, ty) in parameters.iter().enumerate() {
                    let kind = lowered_type_from_type(&executable_type(ty)).map_err(|_| IrBuildError::format("ground_parameter_storage", None, 0, 0))?;
                    if kind != body.param_kinds[index] { return Err(IrBuildError::format("ground_parameter_storage", None, 0, 0)); }
                }
            }
            let params_start = self.store.params.len();
            let captures_start = self.store.captures.len();
            let mut signature_params = Vec::with_capacity(body.params.len());
            for (index, name) in body.params.iter().copied().enumerate() {
                let type_id = if let Some(parameters) = &generic_parameters { match parameters.get(index).copied().ok_or_else(|| IrBuildError::format("generic_parameter_arity", None, 0, 0))? { TypeRef::Ground(ty) => ty.raw(), TypeRef::Rigid(_) | TypeRef::Template(_) => IR_NONE } } else if let Some((parameters, _)) = &grounded_signature { self.semantic.intern_type(&mut self.store.semantic, parameters.get(index).ok_or_else(|| IrBuildError::format("ground_parameter_arity", None, 0, 0))?)?.raw() } else { self.intern_lowered_type(body.param_kinds[index])?.raw() };
                let default = match &body.param_defaults[index] {
                    LoweredParamDefault::Constant(value) => self.encode_value_id(value)?,
                    LoweredParamDefault::None | LoweredParamDefault::Expression => IR_NONE,
                };
                let validation = body.param_checks[index]
                    .as_ref()
                    .map(|check| self.encode_validation(check))
                    .transpose()?
                    .unwrap_or(IR_NONE);
                let flags = u8::from(body.param_rest[index])
                    | u8::from(!matches!(body.param_defaults[index], LoweredParamDefault::None)) << 1
                    | u8::from(matches!(body.param_defaults[index], LoweredParamDefault::Expression)) << 2;
                let name_id = self.intern_string(&name.as_str())?.raw();
                let param = u32::try_from(self.store.params.len())
                    .map_err(|_| IrBuildError::format("parameter_overflow", None, 0, 0))?;
                self.store.params.push(FullParam {
                    name: name_id,
                    type_id,
                    flags,
                    reserved: [0; 3],
                });
                if default != IR_NONE || validation != IR_NONE {
                    self.store.param_cold.push(FullParamCold {
                        param,
                        default,
                        validation,
                    });
                }
                if let Some(type_id) = TypeId::from_raw(type_id) {
                    let signature_flags = u32::from(flags & 2 != 0) | u32::from(flags & 1 != 0) << 1;
                    signature_params.push((name, type_id, signature_flags));
                }
            }
            for capture in &body.captures {
                let slot = u32::try_from(capture.slot)
                    .map_err(|_| IrBuildError::format("slot_overflow", None, 0, 0))?;
                let name = self.intern_string(&capture.name.as_str())?.raw();
                let type_id = if let Some(binding) = capture.host_binding { self.semantic.intern_type(&mut self.store.semantic, &binding.ty())? } else { self.intern_checked_slot_type(capture)? };
                let header_index = u32::try_from(self.store.captures.len()).map_err(|_| IrBuildError::format("host_capture_header_overflow", None, 0, 0))?;
                self.store.captures.push(FullCapture {
                    name,
                    type_id,
                    slot_and_flags: slot | u32::from(capture.mutable) << 31,
                });
                self.stage_lexical_capture(function_id, body.solved_declaration, header_index, capture)?;
                if body.solved_declaration.is_some() || capture.host_binding.is_none() {
                    self.stage_host_binding_capture(function_id, body.solved_declaration, header_index, capture)?;
                }
            }
            let signature = if generic_scope.is_some() { IR_NONE } else {
                let return_type = if let Some((_, result)) = &grounded_signature { self.semantic.intern_type(&mut self.store.semantic, result)? } else if let Some(check) = &body.return_check { self.semantic.intern_type(&mut self.store.semantic, &executable_type(&check.ty))? } else { self.intern_return_type(body.return_kind)? };
                let effects = if let (Some(declaration), Some(solved)) = (body.solved_declaration, self.solved.as_ref()) {
                    let declaration = solved.declarations.get(&declaration).ok_or_else(|| IrBuildError::format("missing_ground_effects", None, 0, 0))?;
                    let signature = solved.graph.resolved(declaration.signature).map_err(|_| IrBuildError::format("invalid_ground_effects", None, 0, 0))?;
                    let crate::sema::inference::TypeNode::Arrow(arrow) = solved.graph.node(signature).map_err(|_| IrBuildError::format("invalid_ground_effects", None, 0, 0))? else { return Err(IrBuildError::format("invalid_ground_effects", None, 0, 0)); };
                    match solved.graph.closed_effect_summary(arrow.effects).map_err(|_| IrBuildError::format("invalid_ground_effects", None, 0, 0))? {
                        crate::sema::inference::EffectSummary::Closed(effects) => Some(callable_prepare::closed_effect_names(effects)),
                        _ => None,
                    }
                } else { None };
                self.semantic.intern_signature_parts(&mut self.store.semantic, &signature_params, return_type, effects.as_deref())?.raw()
            };
            let params = table_range(params_start, self.store.params.len())?;
            let captures = table_range(captures_start, self.store.captures.len())?;
            self.function_ids.insert(unit.key(), function_id);
            self.store.functions.push(FullFunction {
                name,
                signature,
                params,
                captures,
                body: IR_NONE,
                slot_count: u32::try_from(body.slot_count)
                    .map_err(|_| IrBuildError::format("slot_overflow", None, 0, 0))?,
            });
            self.store.function_instruction_starts.push(IR_NONE);
            if generic_scope.is_none() && let Some(declaration) = body.solved_declaration {
                self.generic_evidence_mut().add_checked_function(super::generic::CheckedFunctionSource {
                    declaration, target: function_id,
                    signature: SignatureId::from_raw(signature).ok_or_else(|| IrBuildError::format("checked_function_signature", None, 0, 0))?,
                }).map_err(|_| IrBuildError::format("checked_function_capacity", None, 0, 0))?;
            }
            let return_plan_flags = generic_scope.map(|scope| generic_return_plan_flags(self.generic.as_ref().expect("scope has builder").scope(scope).expect("scope was prepared").return_plan)).unwrap_or(0);
            self.store.function_metadata.push(FullFunctionMetadata {
                owner,
                flags: match unit.kind() {
                    LoweredFunctionKind::Pure => 0,
                    LoweredFunctionKind::Proc => 1,
                } | u8::from(body.has_defers) << 1 | u8::from(generic_scope.is_some()) << 2 | return_plan_flags,
                reserved: [0; 3],
            });
            self.function_definition_spans.push(unit.definition_span());
        }
        Ok(())
    }

    fn encode_body(
        &mut self,
        function: IrFunctionId,
        body: &FunctionBuild,
    ) -> Result<(), IrBuildError> {
        let instruction_start = self.store.tags.len();
        self.active_expression_origins = body.expression_origins.clone();
        self.active_stage_call_origins = body.stage_call_origins.clone();
        self.stage_callable_scratch(&body.scratch.borrow())?;
        self.active_scratch = Some(body.scratch.clone());
        let mut words = self.take_payload();
        let encoded = body.body.encode(self, &mut words);
        self.active_scratch = None;
        if let Err(error) = encoded {
            self.recycle_payload(words);
            return Err(error);
        }
        let [body_id] = words.as_slice() else {
            self.recycle_payload(words);
            return Err(IrBuildError::format(
                "function_body_block",
                None,
                0,
                self.store.tags.len(),
            ));
        };
        let body_id = *body_id;
        self.recycle_payload(words);
        let block = IrBlockId::from_raw(body_id).ok_or_else(|| {
            IrBuildError::format("function_body_block", None, 0, self.store.tags.len())
        })?;
        let instruction_start = u32::try_from(instruction_start)
            .map_err(|_| IrBuildError::format("instruction_overflow", None, 0, 0))?;
        self.finalize_synthetic_host_captures(function, body)?;
        self.store.blocks[block.index()].flags |= BLOCK_FUNCTION_BODY;
        self.store.functions[function.index()].body = body_id;
        self.store.function_instruction_starts[function.index()] = instruction_start;
        self.encoded_slot_uses.retain(|(owner, _)| *owner != function.raw());
        Ok(())
    }

    fn encode_driver_root(
        &mut self,
        program: &ProgramBuild,
        source_statements: &[StmtId],
        arena: &ArenaProgram,
        allow_checker_only: bool,
    ) -> Result<(), IrBuildError> {
        if self.solved.is_none() { self.solved = program.solved.clone(); }
        self.active_expression_origins = program.expression_origins.clone();
        self.active_stage_call_origins = program.stage_call_origins.clone();
        self.stage_callable_scratch(&program.scratch.borrow())?;
        self.active_scratch = Some(program.scratch.clone());
        let result = self.encode_driver_root_with_scratch(
            program,
            source_statements,
            arena,
            allow_checker_only,
        );
        self.active_scratch = None;
        result
    }

    fn encode_driver_root_with_scratch(
        &mut self,
        program: &ProgramBuild,
        source_statements: &[StmtId],
        arena: &ArenaProgram,
        allow_checker_only: bool,
    ) -> Result<(), IrBuildError> {
        if program.statements.len() != source_statements.len() {
            return Err(IrBuildError::format(
                "driver_statement_count",
                None,
                0,
                self.store.tags.len(),
            ));
        }
        let mut statements = Vec::with_capacity(source_statements.len());
        for (source_stmt, lowered) in source_statements.iter().zip(&program.statements) {
            let span = arena.arena.stmt(*source_stmt).span;
            if lowered.is_none()
                && !super::super::compact_top_level_stmt_is_skippable(
                    arena,
                    *source_stmt,
                    allow_checker_only,
                )
            {
                return Err(IrBuildError::format(
                    "top_level_boundary_blocker",
                    Some(span),
                    0,
                    self.store.tags.len(),
                ));
            }
            statements.push((span, *lowered));
        }
        for (_, statement) in &statements {
            if let Some(statement) = statement {
                self.validate_driver_imports(arena, *statement, allow_checker_only)?;
            }
        }
        self.store.driver_root = self.encode_driver_program(&statements, arena)?;
        Ok(())
    }

    fn build_top_stmt(&self, id: BuildTopStmtId) -> Result<BuildTopStmtRow, IrBuildError> {
        let scratch = self
            .active_scratch
            .as_ref()
            .ok_or_else(|| IrBuildError::format("missing_indexed_build_scratch", None, 0, 0))?;
        let scratch = scratch.borrow();
        scratch
            .top_statements
            .get(id.index())
            .cloned()
            .ok_or_else(|| IrBuildError::format("indexed_top_stmt_id", None, 0, 0))
    }

    fn validate_driver_imports(
        &self,
        arena: &ArenaProgram,
        statement: BuildTopStmtId,
        allow_checker_only: bool,
    ) -> Result<(), IrBuildError> {
        let statement = self.build_top_stmt(statement)?;
        let BuildTopKind::Use {
            key,
            module_statements,
            span,
            ..
        } = &statement.kind
        else {
            return Ok(());
        };
        let module = arena
            .modules
            .iter()
            .find(|module| module.key.as_str() == key.as_ref())
            .ok_or_else(|| IrBuildError::format("driver_import_module", Some(*span), 0, 0))?;
        let lowered_spans = module_statements
            .iter()
            .map(|(span, _)| (span.source_id, span.start(), span.end()))
            .collect::<BTreeSet<_>>();
        let source_spans = arena
            .module_statements(module)
            .map(|statement| {
                let span = arena.arena.stmt(statement).span;
                (span.source_id, span.start(), span.end())
            })
            .collect::<BTreeSet<_>>();
        if lowered_spans.len() != module_statements.len() || !lowered_spans.is_subset(&source_spans)
        {
            return Err(IrBuildError::format(
                "driver_import_statement",
                Some(*span),
                0,
                0,
            ));
        }
        for source_statement in arena.module_statements(module) {
            if super::super::compact_top_level_stmt_is_skippable(
                arena,
                source_statement,
                allow_checker_only,
            ) {
                continue;
            }
            let source_span = arena.arena.stmt(source_statement).span;
            let key = (
                source_span.source_id,
                source_span.start(),
                source_span.end(),
            );
            if !lowered_spans.contains(&key) {
                return Err(IrBuildError::format(
                    "module_top_level_boundary_blocker",
                    Some(source_span),
                    0,
                    0,
                ));
            }
        }
        for (_, module_statement) in module_statements {
            self.validate_driver_imports(arena, *module_statement, allow_checker_only)?;
        }
        Ok(())
    }

    fn encode_driver_program(
        &mut self,
        statements: &[(Span, Option<BuildTopStmtId>)],
        arena: &ArenaProgram,
    ) -> Result<u32, IrBuildError> {
        let mut child_programs = Vec::with_capacity(statements.len());
        for (_, statement) in statements {
            let statement_row = statement
                .map(|statement| self.build_top_stmt(statement))
                .transpose()?;
            let child = match statement_row.as_ref().map(|statement| &statement.kind) {
                Some(BuildTopKind::Use {
                    key,
                    module_statements,
                    span,
                    ..
                }) => {
                    let module = arena
                        .modules
                        .iter()
                        .find(|module| module.key.as_str() == key.as_ref())
                        .ok_or_else(|| {
                            IrBuildError::format(
                                "driver_import_module",
                                Some(*span),
                                0,
                                self.store.tags.len(),
                            )
                        })?;
                    let lowered = module_statements
                        .iter()
                        .map(|(span, statement)| {
                            ((span.source_id, span.start(), span.end()), *statement)
                        })
                        .collect::<BTreeMap<_, _>>();
                    let child_statements = arena
                        .module_statements(module)
                        .map(|source_statement| {
                            let span = arena.arena.stmt(source_statement).span;
                            let key = (span.source_id, span.start(), span.end());
                            (span, lowered.get(&key).copied())
                        })
                        .collect::<Vec<_>>();
                    Some(self.encode_driver_program(&child_statements, arena)?)
                }
                _ => None,
            };
            child_programs.push(child);
        }

        let steps_start = self.store.driver_steps.len();
        for ((span, statement), child_program) in statements.iter().zip(child_programs) {
            self.encode_driver_step(*span, *statement, child_program)?;
        }
        let steps = table_range(steps_start, self.store.driver_steps.len())?;
        let regions_start = self.store.driver_regions.len();
        let mut cursor = steps_start;
        while cursor < self.store.driver_steps.len() {
            let first_effects = self.store.driver_steps[cursor].effects;
            let end = if first_effects & EFFECT_BOUNDARY_MASK != 0 {
                cursor + 1
            } else {
                let mut end = cursor + 1;
                while end < self.store.driver_steps.len()
                    && self.store.driver_steps[end].effects & EFFECT_BOUNDARY_MASK == 0
                {
                    end += 1;
                }
                end
            };
            self.push_driver_region(cursor, end)?;
            cursor = end;
        }
        let regions = table_range(regions_start, self.store.driver_regions.len())?;
        let effects = self.store.driver_steps[steps_start..]
            .iter()
            .fold(0, |effects, step| effects | step.effects);
        let program_id = u32::try_from(self.store.driver_programs.len())
            .ok()
            .and_then(|index| index.checked_add(1))
            .filter(|raw| *raw != IR_NONE)
            .ok_or_else(|| IrBuildError::format("driver_program_overflow", None, 0, 0))?;
        self.store.driver_programs.push(FullDriverProgram {
            steps,
            regions,
            effects,
        });
        Ok(program_id)
    }

    fn encode_driver_step(
        &mut self,
        span: Span,
        statement: Option<BuildTopStmtId>,
        child_program: Option<u32>,
    ) -> Result<(), IrBuildError> {
        let original_statement = statement;
        let statement = statement
            .map(|statement| self.build_top_stmt(statement))
            .transpose()?;
        let step_index = self.store.driver_steps.len();
        let owner = driver_owner(step_index)?;
        let instruction_start = u32::try_from(self.store.tags.len())
            .map_err(|_| IrBuildError::format("instruction_overflow", None, 0, 0))?;
        let slots_start = self.store.driver_slots.len();
        let slot_count = statement
            .as_ref()
            .map_or(0, |statement| statement.slot_count);
        let write_slots = statement
            .as_ref()
            .is_some_and(|statement| matches!(statement.kind, BuildTopKind::Stmt(_)));
        if let Some(statement) = statement.as_ref() {
            for slot in &statement.slots {
                let slot_index = u32::try_from(slot.slot)
                    .map_err(|_| IrBuildError::format("driver_slot_overflow", None, 0, 0))?;
                let type_id = if let Some(host) = slot.host_binding { self.intern_generic_ground_type(&host.ty())? } else { self.intern_checked_slot_type(slot)? };
                let name = self.intern_string(&slot.name.as_str())?.raw();
                self.store.driver_slots.push(FullDriverSlot {
                    name,
                    type_id,
                    slot: slot_index,
                    flags: DRIVER_SLOT_READ
                        | if write_slots && slot.mutable {
                            DRIVER_SLOT_WRITE
                        } else {
                            0
                        }
                        | if slot.mutable { DRIVER_SLOT_MUTABLE } else { 0 },
                    reserved: [0; 3],
                });
            }
        }
        let slots = table_range(slots_start, self.store.driver_slots.len())?;
        self.current_owner = Some(owner);
        self.current_slot_count = u32::try_from(slot_count)
            .map_err(|_| IrBuildError::format("slot_overflow", None, 0, 0))?;
        let mut payload = self.take_payload();
        let tag = match statement.as_ref().map(|statement| &statement.kind) {
            None => FullDriverTag::Skip,
            Some(BuildTopKind::Use {
                key,
                alias,
                path,
                namespace,
                exports,
                span,
                ..
            }) => {
                key.encode(self, &mut payload)?;
                alias.encode(self, &mut payload)?;
                path.encode(self, &mut payload)?;
                namespace.encode(self, &mut payload)?;
                exports.encode(self, &mut payload)?;
                payload.push(child_program.ok_or_else(|| {
                    IrBuildError::format("driver_use_program", Some(*span), 0, 0)
                })?);
                span.encode(self, &mut payload)?;
                FullDriverTag::Use
            }
            Some(BuildTopKind::Let {
                target,
                ty,
                validation,
                mutable,
                value,
                value_span,
            }) => {
                target.encode(self, &mut payload)?;
                ty.encode(self, &mut payload)?;
                validation.encode(self, &mut payload)?;
                mutable.encode(self, &mut payload)?;
                value.encode(self, &mut payload)?;
                value_span.encode(self, &mut payload)?;
                FullDriverTag::Let
            }
            Some(BuildTopKind::LetRecord {
                source,
                fields,
                target,
                mutable,
                span,
            }) => {
                source.encode(self, &mut payload)?;
                fields.encode(self, &mut payload)?;
                target.encode(self, &mut payload)?;
                mutable.encode(self, &mut payload)?;
                span.encode(self, &mut payload)?;
                FullDriverTag::LetRecord
            }
            Some(BuildTopKind::Assign {
                target,
                op,
                value,
                span,
            }) => {
                target.encode(self, &mut payload)?;
                op.encode(self, &mut payload)?;
                value.encode(self, &mut payload)?;
                span.encode(self, &mut payload)?;
                FullDriverTag::Assign
            }
            Some(BuildTopKind::Discard { value, span }) => {
                value.encode(self, &mut payload)?;
                span.encode(self, &mut payload)?;
                FullDriverTag::Discard
            }
            Some(BuildTopKind::Stmt(statement)) => {
                statement.encode(self, &mut payload)?;
                FullDriverTag::Stmt
            }
            Some(BuildTopKind::Expr(value)) => {
                value.encode(self, &mut payload)?;
                FullDriverTag::Expr
            }
            Some(BuildTopKind::Defer { value, span }) => {
                value.encode(self, &mut payload)?;
                span.encode(self, &mut payload)?;
                FullDriverTag::Defer
            }
            Some(BuildTopKind::SignalHook {
                signal,
                pre_cancel,
                body,
                slots,
                slot_count,
                span,
            }) => {
                signal.encode(self, &mut payload)?;
                pre_cancel.encode(self, &mut payload)?;
                body.encode(self, &mut payload)?;
                slots.encode(self, &mut payload)?;
                payload.push(u32::try_from(*slot_count).map_err(|_| {
                    IrBuildError::format("signal_hook_slot_overflow", Some(*span), 0, 0)
                })?);
                span.encode(self, &mut payload)?;
                FullDriverTag::SignalHook
            }
        };
        self.current_owner = None;
        self.current_slot_count = 0;
        let data = self.push_extra(&payload);
        self.recycle_payload(payload);
        let data = data?;
        let location = self.intern_location(span)?.raw();
        let mut effects = driver_tag_effects(tag);
        if slots.len != 0 {
            effects |= EFFECT_BINDING_READ;
        }
        if self.store.driver_slots[slots.bounds(self.store.driver_slots.len()).unwrap()]
            .iter()
            .any(|slot| slot.flags & DRIVER_SLOT_WRITE != 0)
            || driver_tag_writes_binding(tag)
        {
            effects |= EFFECT_BINDING_WRITE;
        }
        effects |= instruction_effects(&self.store.tags[instruction_start as usize..]);
        self.store.driver_steps.push(FullDriverStep {
            data: IrData::new(data.start, data.len),
            slots,
            instruction_start,
            slot_count: u32::try_from(slot_count)
                .map_err(|_| IrBuildError::format("slot_overflow", None, 0, 0))?,
            location,
            effects,
            tag,
            reserved: [0; 3],
        });
        if let Some(row) = original_statement {
            let scratch = self.active_scratch.clone().ok_or_else(|| IrBuildError::format("mutable_driver_build_scratch_missing", None, 0, 0))?;
            self.stage_mutable_driver_step(row, u32::try_from(step_index).map_err(|_| IrBuildError::format("driver_step_overflow", None, 0, 0))?, &scratch.borrow())?;
        }
        Ok(())
    }

    fn push_driver_region(&mut self, start: usize, end: usize) -> Result<(), IrBuildError> {
        let mut effects = 0;
        let mut sync = BTreeMap::<u32, (TypeId, u8)>::new();
        for step in &self.store.driver_steps[start..end] {
            effects |= step.effects;
            let slots = step
                .slots
                .bounds(self.store.driver_slots.len())
                .ok_or_else(|| IrBuildError::format("driver_slot_range", None, 0, 0))?;
            for slot in &self.store.driver_slots[slots] {
                let flags = slot.flags & (DRIVER_SLOT_READ | DRIVER_SLOT_WRITE);
                if let Some((type_id, existing)) = sync.get_mut(&slot.name) {
                    if *type_id != slot.type_id {
                        return Err(IrBuildError::format(
                            "driver_sync_type_conflict",
                            None,
                            0,
                            self.store.tags.len(),
                        ));
                    }
                    *existing |= flags;
                } else {
                    sync.insert(slot.name, (slot.type_id, flags));
                }
            }
        }
        let sync_start = self.store.driver_sync.len();
        for (name, (type_id, flags)) in sync {
            self.store.driver_sync.push(FullDriverSync {
                name,
                type_id,
                flags,
                reserved: [0; 3],
            });
        }
        self.store.driver_regions.push(FullDriverRegion {
            steps: table_range(start, end)?,
            sync: table_range(sync_start, self.store.driver_sync.len())?,
            effects,
        });
        Ok(())
    }

    fn push_instruction(&mut self, tag: FullTag, payload: &[u32]) -> Result<u32, IrBuildError> {
        let function = self
            .current_owner
            .ok_or_else(|| IrBuildError::format("missing_instruction_owner", None, 0, 0))?;
        let id = u32::try_from(self.store.tags.len())
            .map_err(|_| IrBuildError::format("instruction_overflow", None, 0, 0))?;
        let range = self.push_extra(payload)?;
        self.store.tags.push(tag);
        self.store.data.push(IrData::new(range.start, range.len));
        debug_assert_eq!(
            function,
            self.current_owner.expect("instruction owner remains set")
        );
        Ok(id)
    }

    fn push_pattern(&mut self, tag: FullPatternTag, payload: &[u32]) -> Result<u32, IrBuildError> {
        let id = u32::try_from(self.store.patterns.len())
            .map_err(|_| IrBuildError::format("pattern_overflow", None, 0, 0))?;
        let range = self.push_extra(payload)?;
        self.store.patterns.push(tag);
        self.store
            .pattern_data
            .push(IrData::new(range.start, range.len));
        Ok(id)
    }

    fn push_stage(&mut self, tag: FullStageTag, payload: &[u32]) -> Result<u32, IrBuildError> {
        let id = u32::try_from(self.store.stages.len())
            .map_err(|_| IrBuildError::format("stage_overflow", None, 0, 0))?;
        let range = self.push_extra(payload)?;
        self.store.stages.push(tag);
        self.store
            .stage_data
            .push(IrData::new(range.start, range.len));
        Ok(id)
    }

    fn push_value(&mut self, tag: FullValueTag, payload: &[u32]) -> Result<u32, IrBuildError> {
        let id = u32::try_from(self.store.values.len())
            .map_err(|_| IrBuildError::format("value_overflow", None, 0, 0))?;
        let range = self.push_extra(payload)?;
        self.store.values.push(tag);
        self.store
            .value_data
            .push(IrData::new(range.start, range.len));
        Ok(id)
    }

    fn push_block(&mut self, instructions: &[u32], flags: u8) -> Result<IrBlockId, IrBuildError> {
        let id = IrBlockId::new(self.store.blocks.len())?;
        let instructions = self.push_extra(instructions)?;
        self.store.blocks.push(FullBlock {
            instructions,
            result: IR_NONE,
            owner: self.current_owner.unwrap_or(IR_NONE),
            flags,
            reserved: [0; 3],
        });
        Ok(id)
    }

    fn push_extra(&mut self, words: &[u32]) -> Result<IrRange, IrBuildError> {
        let start = u32::try_from(self.store.extra.len())
            .map_err(|_| IrBuildError::format("extra_overflow", None, 0, 0))?;
        let len = u32::try_from(words.len())
            .map_err(|_| IrBuildError::format("extra_overflow", None, 0, 0))?;
        self.store.extra.extend_from_slice(words);
        Ok(IrRange::new(start, len))
    }

    fn intern_function_key(&mut self, key: LoweredFunctionKey) -> Result<u32, IrBuildError> {
        match key {
            LoweredFunctionKey::Name(name) => Ok(self.intern_string(&name.as_str())?.raw()),
            LoweredFunctionKey::Qualified(name) => {
                Ok(self.intern_string(&name.member.as_str())?.raw())
            }
        }
    }

    fn intern_string(&mut self, value: &str) -> Result<IrStringId, IrBuildError> {
        if let Some(id) = self.strings.get(value) {
            return Ok(*id);
        }
        let id = IrStringId::new(self.store.strings.len())?;
        let start = u32::try_from(self.store.string_bytes.len())
            .map_err(|_| IrBuildError::format("string_overflow", None, 0, 0))?;
        let len = u32::try_from(value.len())
            .map_err(|_| IrBuildError::format("string_overflow", None, 0, 0))?;
        self.store.string_bytes.extend_from_slice(value.as_bytes());
        self.store.strings.push(IrRange::new(start, len));
        self.strings.insert(value.to_string(), id);
        Ok(id)
    }

    fn intern_bytes(&mut self, value: &[u8]) -> Result<super::IrBytesId, IrBuildError> {
        if let Some(id) = self.bytes.get(value) {
            return Ok(*id);
        }
        let id = super::IrBytesId::new(self.store.bytes.len())?;
        let start = u32::try_from(self.store.byte_data.len())
            .map_err(|_| IrBuildError::format("bytes_overflow", None, 0, 0))?;
        let len = u32::try_from(value.len())
            .map_err(|_| IrBuildError::format("bytes_overflow", None, 0, 0))?;
        self.store.byte_data.extend_from_slice(value);
        self.store.bytes.push(IrRange::new(start, len));
        self.bytes.insert(value.to_vec(), id);
        Ok(id)
    }

    fn intern_location(&mut self, span: Span) -> Result<IrLocationId, IrBuildError> {
        let location = IrLocation::from_span(span)?;
        let key = (span.source_id, location.start, location.len);
        if let Some(id) = self.locations.get(&key) {
            return Ok(*id);
        }
        let id = IrLocationId::new(self.store.locations.len())?;
        self.store.locations.push(location);
        self.store.location_sources.push(span.source_id);
        self.locations.insert(key, id);
        Ok(id)
    }

    fn intern_copy<T: Copy + Eq>(
        values: &mut Vec<T>,
        value: T,
        construct: &'static str,
    ) -> Result<u32, IrBuildError> {
        if let Some(index) = values.iter().position(|candidate| *candidate == value) {
            return u32::try_from(index).map_err(|_| IrBuildError::format(construct, None, 0, 0));
        }
        let id =
            u32::try_from(values.len()).map_err(|_| IrBuildError::format(construct, None, 0, 0))?;
        values.push(value);
        Ok(id)
    }

    fn encode_validation(&mut self, check: &LoweredTypeCheck) -> Result<u32, IrBuildError> {
        let id = u32::try_from(self.store.validations.len())
            .map_err(|_| IrBuildError::format("validation_overflow", None, 0, 0))?;
        let type_id = self
            .semantic
            .intern_type(&mut self.store.semantic, &executable_type(&check.ty))?;
        let name = self.intern_string(&check.name)?.raw();
        self.store
            .validations
            .push(FullValidation { type_id, name });
        Ok(id)
    }

    fn intern_lowered_type(&mut self, ty: LoweredType) -> Result<TypeId, IrBuildError> {
        let ty = lowered_type_to_type(ty)?;
        self.semantic.intern_type(&mut self.store.semantic, &ty)
    }

    fn intern_return_type(&mut self, kind: LoweredReturnKind) -> Result<TypeId, IrBuildError> {
        match kind {
            LoweredReturnKind::Plain(ty) => self.intern_lowered_type(ty),
            LoweredReturnKind::Result(ty) => {
                let ok = lowered_type_to_type(ty)?;
                self.semantic.intern_type(
                    &mut self.store.semantic,
                    &Type::Result(Box::new(ok), Box::new(Type::Error)),
                )
            }
        }
    }

    fn take_payload(&mut self) -> Vec<u32> {
        self.payload_pool.pop().unwrap_or_default()
    }

    fn recycle_payload(&mut self, mut payload: Vec<u32>) {
        payload.clear();
        self.payload_pool.push(payload);
    }

    fn encode_value_id(&mut self, value: &LoweredValue) -> Result<u32, IrBuildError> {
        let mut words = self.take_payload();
        let encoded = value.encode(self, &mut words);
        if let Err(error) = encoded {
            self.recycle_payload(words);
            return Err(error);
        }
        debug_assert_eq!(words.len(), 1);
        let value = words[0];
        self.recycle_payload(words);
        Ok(value)
    }

    fn checkpoint(&self) -> FullCheckpoint {
        FullCheckpoint {
            value_binding_rows: self.value_binding_rows.len(),
            value_use_rows: self.value_use_rows.len(),
            iteration_binding_rows: self.iteration_binding_rows.len(),
            iteration_use_rows: self.iteration_use_rows.len(),
            argument_binding_rows: self.argument_binding_rows.len(),
            callable_binding_rows: self.callable_binding_rows.len(),
            callable_use_rows: self.callable_use_rows.len(),
            callable_receiver_rows: self.callable_receiver_rows.len(),
            original_callable_capture_rows: self.original_callable_capture_rows.len(),
            saved_native_receiver_rows: self.saved_native_receiver_rows.len(),
            result_receiver_rows: self.result_receiver_rows.len(),
            host_binding_read_rows: self.host_binding_read_rows.len(),
            lexical_capture_read_rows: self.lexical_capture_read_rows.len(),
            conditional_result_rows: self.conditional_result_rows.len(),
            path_source_rows: self.path_source_rows.len(),
            container_creation_check_rows: self.container_creation_check_rows.len(),
            byte_at_fallback_rows: self.byte_at_fallback_rows.len(),
            encoded_slot_uses: self.encoded_slot_uses.len(),
            tags: self.store.tags.len(),
            extra: self.store.extra.len(),
            patterns: self.store.patterns.len(),
            stages: self.store.stages.len(),
            values: self.store.values.len(),
            blocks: self.store.blocks.len(),
            driver_steps: self.store.driver_steps.len(),
            driver_slots: self.store.driver_slots.len(),
            driver_sync: self.store.driver_sync.len(),
            driver_regions: self.store.driver_regions.len(),
            driver_programs: self.store.driver_programs.len(),
            driver_root: self.store.driver_root,
            params: self.store.params.len(),
            captures: self.store.captures.len(),
            validations: self.store.validations.len(),
            strings: self.store.strings.len(),
            string_bytes: self.store.string_bytes.len(),
            bytes: self.store.bytes.len(),
            byte_data: self.store.byte_data.len(),
            prepared_regexes: self.store.prepared_regexes.len(),
            prepared_constants: self.store.prepared_constants.len(),
            wire_enums: self.store.wire_enums.len(),
            prepared_schemas: self.store.prepared_schemas.len(),
            prepared_cli_plans: self.store.prepared_cli_plans.len(),
            locations: self.store.locations.len(),
            runtime_ops: self.store.runtime_ops.len(),
            assign_ops: self.store.assign_ops.len(),
            binary_ops: self.store.binary_ops.len(),
            run_kinds: self.store.run_kinds.len(),
            redirection_kinds: self.store.redirection_kinds.len(),
            semantic: self.semantic.checkpoint(&self.store.semantic),
            generic: self.generic.as_ref().map(GenericEvidenceBuilder::checkpoint),
            generic_expression_rows: self.generic_expression_rows.len(),
            stage_block_callback_rows: self.stage_block_callback_rows.len(),
            generic_stage_call_rows: self.generic_stage_call_rows.len(),
            generic_pattern_rows: self.generic_pattern_rows.len(),
            pattern_capture_slot_rows: self.pattern_capture_slot_rows.len(),
            pattern_capture_source_rows: self.pattern_capture_source_rows.len(),
            pattern_use_rows: self.pattern_use_rows.len(),
            generic_pattern_statement_use_rows: self.generic_pattern_statement_use_rows.len(),
            generic_pattern_admissions: self.generic_pattern_admissions.len(),
        }
    }

    fn rewind(&mut self, checkpoint: FullCheckpoint) {
        self.value_binding_rows.truncate(checkpoint.value_binding_rows);
        self.value_use_rows.truncate(checkpoint.value_use_rows);
        self.iteration_binding_rows.truncate(checkpoint.iteration_binding_rows);
        self.iteration_use_rows.truncate(checkpoint.iteration_use_rows);
        self.argument_binding_rows.truncate(checkpoint.argument_binding_rows);
        self.active_argument_wrappers.retain(|_, rows| { rows.retain(|&row| row < checkpoint.argument_binding_rows); !rows.is_empty() });
        self.prepared_argument_origins.retain(|instruction, _| (*instruction as usize) < checkpoint.tags);
        self.prepared_saved_argument_bindings.retain(|instruction, _| (*instruction as usize) < checkpoint.tags);
        self.callable_binding_rows.truncate(checkpoint.callable_binding_rows);
        self.callable_use_rows.truncate(checkpoint.callable_use_rows);
        self.callable_receiver_rows.truncate(checkpoint.callable_receiver_rows);
        self.original_callable_capture_rows.truncate(checkpoint.original_callable_capture_rows);
        self.saved_native_receiver_rows.truncate(checkpoint.saved_native_receiver_rows);
        self.result_receiver_rows.truncate(checkpoint.result_receiver_rows);
        self.active_native_receiver_wrappers.retain(|_, rows| { rows.retain(|&row| row < checkpoint.saved_native_receiver_rows); !rows.is_empty() });
        self.active_callable_receiver_wrappers.retain(|_, rows| { rows.retain(|&row| row < checkpoint.callable_receiver_rows); !rows.is_empty() });
        self.active_encoded_expressions.retain(|_, instruction| (*instruction as usize) < checkpoint.tags);
        self.compiler_argument_wrappers.retain(|instruction, _| (*instruction as usize) < checkpoint.tags);
        self.optional_receiver_guards.retain(|_, guard| (guard.wrapper as usize) < checkpoint.tags);
        self.record_source_rows.retain(|(_, instruction, _, _)| (*instruction as usize) < checkpoint.tags);
        self.index_rows.retain(|(_, instruction, _)| (*instruction as usize) < checkpoint.tags);
        self.literal_comparison_rows.retain(|(instruction, _, _)| (*instruction as usize) < checkpoint.tags);
        self.folded_native_receiver_rows.retain(|(instruction, _, _)| (*instruction as usize) < checkpoint.tags);
        self.record_constructor_rows.retain(|row| (row.1 as usize) < checkpoint.tags);
        self.constant_source_rows.retain(|row| (row.1 as usize) < checkpoint.tags);
        self.record_update_rows.retain(|row| (row.1 as usize) < checkpoint.tags);
        self.error_constructor_rows.retain(|row| (row.1 as usize) < checkpoint.tags);
        self.host_binding_read_rows.truncate(checkpoint.host_binding_read_rows);
        self.lexical_capture_read_rows.truncate(checkpoint.lexical_capture_read_rows);
        self.conditional_result_rows.truncate(checkpoint.conditional_result_rows);
        self.path_source_rows.truncate(checkpoint.path_source_rows);
        self.container_creation_check_rows.truncate(checkpoint.container_creation_check_rows);
        self.byte_at_fallback_rows.truncate(checkpoint.byte_at_fallback_rows);
        self.encoded_slot_uses.truncate(checkpoint.encoded_slot_uses);
        self.generic_expression_rows.truncate(checkpoint.generic_expression_rows);
        self.stage_block_callback_rows.truncate(checkpoint.stage_block_callback_rows);
        self.generic_stage_call_rows.truncate(checkpoint.generic_stage_call_rows);
        self.generic_pattern_rows.truncate(checkpoint.generic_pattern_rows);
        for identity in self.pattern_capture_slot_rows.drain(checkpoint.pattern_capture_slot_rows..) { self.pattern_capture_slots.remove(&identity); }
        for identity in self.pattern_capture_source_rows.drain(checkpoint.pattern_capture_source_rows..) { self.pattern_capture_sources.remove(&identity); }
        for identity in self.pattern_use_rows.drain(checkpoint.pattern_use_rows..) { self.pattern_use_origins.remove(&identity); }
        self.generic_pattern_statement_use_rows.truncate(checkpoint.generic_pattern_statement_use_rows);
        self.generic_pattern_admissions.truncate(checkpoint.generic_pattern_admissions);
        self.active_pattern_statements.retain(|_, instruction| (*instruction as usize) < checkpoint.tags);
        match checkpoint.generic {
            Some(checkpoint) => self.generic.as_mut().expect("generic checkpoint retains its builder").rewind(checkpoint).expect("generic checkpoint belongs to this builder"),
            None => self.generic = None,
        }
        if let Some(generic) = &self.generic {
            self.generic_declarations.retain(|_, scope| generic.scope(*scope).is_ok());
            self.generic_schemes.retain(|scope, _| generic.scope(*scope).is_ok());
            self.generic_projection_uses.retain(|_, (scope, _)| generic.scope(*scope).is_ok());
        } else { self.generic_declarations.clear(); self.generic_schemes.clear(); self.generic_projection_uses.clear(); }
        self.store.tags.truncate(checkpoint.tags);
        self.store.data.truncate(checkpoint.tags);
        self.store.extra.truncate(checkpoint.extra);
        self.store.patterns.truncate(checkpoint.patterns);
        self.store.pattern_data.truncate(checkpoint.patterns);
        self.store.stages.truncate(checkpoint.stages);
        self.store.stage_data.truncate(checkpoint.stages);
        self.store.values.truncate(checkpoint.values);
        self.store.value_data.truncate(checkpoint.values);
        self.store.blocks.truncate(checkpoint.blocks);
        self.store.driver_steps.truncate(checkpoint.driver_steps);
        self.store.driver_slots.truncate(checkpoint.driver_slots);
        self.store.driver_sync.truncate(checkpoint.driver_sync);
        self.store
            .driver_regions
            .truncate(checkpoint.driver_regions);
        self.store
            .driver_programs
            .truncate(checkpoint.driver_programs);
        self.store.driver_root = checkpoint.driver_root;
        self.store.params.truncate(checkpoint.params);
        self.store.captures.truncate(checkpoint.captures);
        self.store.validations.truncate(checkpoint.validations);
        self.store.strings.truncate(checkpoint.strings);
        self.store.string_bytes.truncate(checkpoint.string_bytes);
        self.store.bytes.truncate(checkpoint.bytes);
        self.store.byte_data.truncate(checkpoint.byte_data);
        self.store.prepared_regexes.truncate(checkpoint.prepared_regexes);
        self.store.prepared_constants.truncate(checkpoint.prepared_constants);
        self.store.wire_enums.truncate(checkpoint.wire_enums);
        self.store.prepared_schemas.truncate(checkpoint.prepared_schemas);
        self.store.prepared_cli_plans.truncate(checkpoint.prepared_cli_plans);
        self.store.locations.truncate(checkpoint.locations);
        self.store.location_sources.truncate(checkpoint.locations);
        self.store.runtime_ops.truncate(checkpoint.runtime_ops);
        self.store.assign_ops.truncate(checkpoint.assign_ops);
        self.store.binary_ops.truncate(checkpoint.binary_ops);
        self.store.run_kinds.truncate(checkpoint.run_kinds);
        self.store
            .redirection_kinds
            .truncate(checkpoint.redirection_kinds);
        self.semantic
            .rewind(&mut self.store.semantic, checkpoint.semantic);
        self.strings.retain(|_, id| id.index() < checkpoint.strings);
        self.bytes.retain(|_, id| id.index() < checkpoint.bytes);
        self.locations
            .retain(|_, id| id.index() < checkpoint.locations);
    }
}

fn table_range(start: usize, end: usize) -> Result<IrRange, IrBuildError> {
    Ok(IrRange::new(
        u32::try_from(start).map_err(|_| IrBuildError::format("table_overflow", None, 0, 0))?,
        u32::try_from(end.saturating_sub(start))
            .map_err(|_| IrBuildError::format("table_overflow", None, 0, 0))?,
    ))
}

fn driver_tag_effects(tag: FullDriverTag) -> u32 {
    match tag {
        FullDriverTag::Use => EFFECT_IMPORT | EFFECT_DYNAMIC_CALL | EFFECT_TRACE,
        FullDriverTag::Defer => EFFECT_DEFER | EFFECT_PROPAGATE | EFFECT_TRACE,
        FullDriverTag::SignalHook => EFFECT_SIGNAL | EFFECT_CANCELLATION | EFFECT_TRACE,
        FullDriverTag::Skip
        | FullDriverTag::Let
        | FullDriverTag::LetRecord
        | FullDriverTag::Assign
        | FullDriverTag::Discard
        | FullDriverTag::Stmt
        | FullDriverTag::Expr => 0,
    }
}

fn driver_tag_writes_binding(tag: FullDriverTag) -> bool {
    matches!(
        tag,
        FullDriverTag::Use | FullDriverTag::Let | FullDriverTag::LetRecord | FullDriverTag::Assign
    )
}

fn instruction_effects(tags: &[FullTag]) -> u32 {
    tags.iter().fold(0, |effects, tag| {
        effects
            | match tag {
                FullTag::ExprContextScope => EFFECT_CWD | EFFECT_ENV | EFFECT_HOST | EFFECT_TRACE,
                FullTag::StmtCd => EFFECT_CWD | EFFECT_HOST | EFFECT_TRACE,
                FullTag::StmtEnv => EFFECT_ENV | EFFECT_HOST | EFFECT_TRACE,
                FullTag::ExprRunCapture
                | FullTag::ExprRunPipeline
                | FullTag::ExprSpawnRun
                | FullTag::ExprSpawnCommand
                | FullTag::ExprWait
                | FullTag::ExprProcessCommandArgv
                | FullTag::ExprProcessCommandBuilder
                | FullTag::StmtRun => {
                    EFFECT_PROCESS
                        | EFFECT_CANCELLATION
                        | EFFECT_PROPAGATE
                        | EFFECT_HOST
                        | EFFECT_TRACE
                }
                FullTag::ExprAssert | FullTag::ExprAbort | FullTag::ExprFail => EFFECT_PROPAGATE | EFFECT_TRACE,
                FullTag::ExprDynamicCall => EFFECT_DYNAMIC_CALL | EFFECT_TRACE,
                FullTag::ExprCall
                | FullTag::ExprSelfCall
                | FullTag::ExprDirectPureCall
                | FullTag::ExprExternalCall => EFFECT_TRACE,
                FullTag::ExprModuleCall | FullTag::StmtProc => EFFECT_HOST | EFFECT_TRACE,
                FullTag::StmtPrint => EFFECT_HOST | EFFECT_TRACE,
                FullTag::ExprFsFiles
                | FullTag::ExprFsWalk
                | FullTag::ExprFsList
                | FullTag::ExprFsTempDir
                | FullTag::ExprFsWrite
                | FullTag::ExprFsMkdir
                | FullTag::ExprFsRemove
                | FullTag::ExprPathReadText
                | FullTag::ExprPathReadBytes
                | FullTag::ExprPathExists
                | FullTag::ExprPathExecutable
                | FullTag::ExprPathDu
                | FullTag::ExprPathMetadata
                | FullTag::ExprPathReadlink
                | FullTag::ExprPathResolve
                | FullTag::ExprPathWrite
                | FullTag::ExprPathMkdir
                | FullTag::ExprPathRemove
                | FullTag::ExprArchiveTarCreate
                | FullTag::ExprArchiveTarList
                | FullTag::ExprArchiveTarExtract
                | FullTag::ExprTry
                | FullTag::StmtGuard
                | FullTag::StmtWith => EFFECT_PROPAGATE | EFFECT_TRACE,
                FullTag::StmtDefer => EFFECT_DEFER | EFFECT_PROPAGATE | EFFECT_TRACE,
                FullTag::ExprLoop
                | FullTag::StmtLoop
                | FullTag::StmtWhile
                | FullTag::StmtWhileBool
                | FullTag::StmtPatternIf
                | FullTag::StmtPatternWhile
                | FullTag::StmtFor
                | FullTag::StmtForRecord
                | FullTag::StmtForStrLines
                | FullTag::StmtScanLines
                | FullTag::StmtScanBytes => EFFECT_CANCELLATION,
                _ => 0,
            }
    })
}

fn lowered_type_to_type(ty: LoweredType) -> Result<Type, IrBuildError> {
    Ok(match ty {
        LoweredType::Generic => return Err(IrBuildError::format("generic_type_requires_scoped_metadata", None, 0, 0)),
        LoweredType::Any => Type::Any,
        LoweredType::Unit => Type::Unit,
        LoweredType::Int => Type::Int,
        LoweredType::Float => Type::Float,
        LoweredType::Duration => Type::Duration,
        LoweredType::Bool => Type::Bool,
        LoweredType::Str => Type::Str,
        LoweredType::Bytes => Type::Bytes,
        LoweredType::Digest => Type::Digest,
        LoweredType::Regex => Type::Regex,
        LoweredType::Status => Type::Status,
        LoweredType::Path => Type::Path,
        LoweredType::Command => Type::Command,
        LoweredType::ProcessHandle => Type::ProcessHandle,
        LoweredType::NetJob => Type::NetJob,
        LoweredType::FsRoot => Type::FsRoot,
        LoweredType::Stream => Type::Stream(Box::new(Type::Any)),
        LoweredType::Pure => Type::Pure,
        LoweredType::Proc => Type::Proc,
        LoweredType::Error => Type::Error,
        LoweredType::Record => Type::ErasedRecord,
        LoweredType::Module => Type::DynamicModule,
        LoweredType::List => Type::List(Box::new(Type::Any)),
        LoweredType::Map => Type::Map(Box::new(Type::Str), Box::new(Type::Any)),
        LoweredType::Result => Type::Result(Box::new(Type::Any), Box::new(Type::Error)),
        LoweredType::Tag => Type::Tag(Name::intern("<tag>")),
    })
}

fn executable_type(ty: &Type) -> Type {
    // The current runtime treats checker recovery types as wildcards. Commit
    // that executable meaning explicitly so recovery identities never enter
    // the stable semantic pool.
    match ty {
        Type::Unknown | Type::Invalid => Type::Any,
        Type::List(inner) => Type::List(Box::new(executable_type(inner))),
        Type::Map(key, inner) => Type::Map(Box::new(executable_type(key)), Box::new(executable_type(inner))),
        Type::Stream(inner) => Type::Stream(Box::new(executable_type(inner))),
        Type::Optional(inner) => Type::Optional(Box::new(executable_type(inner))),
        Type::Result(ok, error) => Type::Result(
            Box::new(executable_type(ok)),
            Box::new(executable_type(error)),
        ),
        Type::Record(fields) => Type::Record(
            fields
                .iter()
                .map(|(name, ty)| (*name, executable_type(ty)))
                .collect(),
        ),
        Type::Module(exports) => Type::Module(
            exports
                .iter()
                .map(|(name, export)| {
                    let export = match export {
                        ModuleExportType::Value { ty, optional } => ModuleExportType::Value {
                            ty: executable_type(ty),
                            optional: *optional,
                        },
                        ModuleExportType::Proc { sig, optional } => ModuleExportType::Proc {
                            sig: executable_callable_type(sig),
                            optional: *optional,
                        },
                        ModuleExportType::Pure { sig, optional } => ModuleExportType::Pure {
                            sig: executable_callable_type(sig),
                            optional: *optional,
                        },
                    };
                    (*name, export)
                })
                .collect(),
        ),
        ty => ty.clone(),
    }
}

fn executable_callable_type(signature: &CallableType) -> CallableType {
    CallableType {
        params: signature
            .params
            .iter()
            .map(|param| CallableParamType {
                name: param.name,
                ty: executable_type(&param.ty),
                defaulted: param.defaulted,
                rest: param.rest,
            })
            .collect(),
        return_ty: Box::new(executable_type(&signature.return_ty)),
        effects: signature.effects.clone(),
    }
}

fn lowered_type_from_type(ty: &Type) -> Result<LoweredType, IrVerifyError> {
    Ok(match ty {
        Type::Any | Type::Unknown => LoweredType::Any,
        Type::Unit => LoweredType::Unit,
        Type::Int | Type::UInt => LoweredType::Int,
        Type::Float => LoweredType::Float,
        Type::Duration => LoweredType::Duration,
        Type::Bool => LoweredType::Bool,
        Type::Str => LoweredType::Str,
        Type::Bytes => LoweredType::Bytes,
        Type::Digest => LoweredType::Digest,
        Type::Regex => LoweredType::Regex,
        Type::Status => LoweredType::Status,
        Type::Path => LoweredType::Path,
        Type::Command => LoweredType::Command,
        Type::ProcessHandle => LoweredType::ProcessHandle,
        Type::NetJob => LoweredType::NetJob,
        Type::FsRoot => LoweredType::FsRoot,
        Type::Stream(_) => LoweredType::Stream,
        Type::Pure => LoweredType::Pure,
        Type::Proc => LoweredType::Proc,
        Type::Error | Type::ProcessError | Type::ErrorFamily(_) | Type::ErrorVariant { .. } | Type::ErrorFacet(_) => {
            LoweredType::Error
        }
        Type::ErasedRecord | Type::Record(_) => LoweredType::Record,
        Type::Module(_) | Type::DynamicModule => LoweredType::Module,
        Type::List(_) => LoweredType::List,
        Type::Map(_, _) => LoweredType::Map,
        Type::Tag(_) => LoweredType::Tag,
        Type::Result(_, _) => LoweredType::Result,
        Type::Null | Type::Optional(_) => LoweredType::Any,
        Type::BuiltinParameter(_) | Type::Inference(_) | Type::Graph(_) | Type::Invalid | Type::EnvPathList => {
            return Err(IrVerifyError::new(
                "semantic type has no lowered runtime equivalent",
            ));
        }
    })
}

pub(in crate::runtime::eval) trait FullCodec: Sized {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError>;

    fn decode(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>)
    -> Result<Self, IrVerifyError>;

    fn verify(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<(), IrVerifyError> {
        Self::decode(decoder, input).map(drop)
    }
}

#[derive(Clone, Copy)]
pub(in crate::runtime::eval) struct FullCursor<'a> {
    words: &'a [u32],
    index: usize,
    verified: bool,
}

#[derive(Clone, Copy)]
pub(in crate::runtime::eval) struct FullPayload<'a> {
    cursor: FullCursor<'a>,
}

impl<'a> FullPayload<'a> {
    #[inline(always)]
    pub(in crate::runtime::eval) fn raw(&mut self) -> Result<u32, IrVerifyError> {
        self.cursor.raw()
    }

    #[inline(always)]
    pub(in crate::runtime::eval) fn decode<T: FullCodec>(
        &mut self,
        execution: &FullExecution<'a>,
    ) -> Result<T, IrVerifyError> {
        T::decode(&execution.decoder, &mut self.cursor)
    }

    #[inline(always)]
    pub(in crate::runtime::eval) fn finish(self) -> Result<(), IrVerifyError> {
        self.cursor.finish()
    }
}

pub(in crate::runtime::eval) struct FullExecution<'a> {
    decoder: FullDecoder<'a>,
    instantiation: Option<InstantiationId>,
}

impl<'a> FullExecution<'a> {
    pub(in crate::runtime::eval) fn ground_projection(&self, instruction: u32) -> Result<Option<(super::generic::PhysicalLayoutId, u32)>, IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("ground projection belongs to another body")); }
        let Some(store) = self.generic_evidence() else { return Ok(None); };
        let Some(id) = store.ground_projection_at(instruction)? else { return Ok(None); };
        let proof = store.ground_projection(id)?;
        let source = store.ground_projection_source(proof.source)?;
        let owner = if let Some(driver) = driver_owner_index(self.decoder.owner) { InstructionOwner::Driver(driver as u32) } else {
            InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("ground projection instruction owner is invalid"))?)
        };
        if source.owner != owner || source.instruction != instruction { return Err(IrVerifyError::new("ground projection belongs to another owner or instruction")); }
        store.layout(proof.layout)?;
        Ok(Some((proof.layout, proof.field_slot)))
    }
    pub(in crate::runtime::eval) fn ground_native_call(&self, instruction: u32, operation: RuntimeOp) -> Result<Option<&super::generic::GroundNativeCallContract>, IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("native call belongs to another body")); }
        if crate::stdlib::is_private_bridge_op(operation) {
            let generic = self.generic_evidence().ok_or_else(|| IrVerifyError::new("private native bridge lacks prepared authority"))?;
            let owner = if let Some(driver) = driver_owner_index(self.decoder.owner) { InstructionOwner::Driver(driver as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("native bridge owner is invalid"))?) };
            let original = generic.bridge_call(instruction)?.ok_or_else(|| IrVerifyError::new("private native bridge lacks its original invocation"))?;
            if original.original.declaration().op() != operation { return Err(IrVerifyError::new("native bridge operation differs from its original catalog declaration")); }
            FullVerifier::bridge_result(self.decoder.store, generic, instruction, owner)?;
            return Ok(None);
        }
        let Some(generic) = self.generic_evidence() else { return Ok(None); };
        let Some(id) = generic.ground_native_call_at(instruction)? else {
            if generic.native_call_originally_prepared(instruction) { return Err(IrVerifyError::new("native call lacks its original prepared authority")); }
            return Ok(None);
        };
        let proof = generic.ground_native_call(id)?;
        let source = generic.native_call_source(proof.source)?;
        let owner = if let Some(driver) = driver_owner_index(self.decoder.owner) { InstructionOwner::Driver(driver as u32) } else {
            InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("native call instruction owner is invalid"))?)
        };
        if source.owner != owner || source.instruction != instruction
            || !matches!(proof.contract.authority, super::generic::PreparedOperationAuthority::Registry { operation: selected, .. } if selected == operation) {
            return Err(IrVerifyError::new("native call changes its original owner, instruction, or operation"));
        }
        FullVerifier::native_call_result(self.decoder.store, generic, instruction, owner)?;
        Ok(Some(&proof.contract))
    }
    pub(in crate::runtime::eval) fn ground_native_method(&self, instruction: u32) -> Result<Option<RuntimeOp>, IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("native method belongs to another body")); }
        let Some(generic) = self.generic_evidence() else { return Ok(None); };
        if let Some(operation) = generic.scoped_native_method_operation(&self.decoder.store.semantic, instruction, self.instantiation)? {
            let source = generic.scoped_native_method_source(generic.scoped_native_method_source_at(instruction)?.ok_or_else(|| IrVerifyError::new("scoped native method lacks its original source"))?)?;
            if generic.scope(source.scope)?.owner.raw() != self.decoder.owner { return Err(IrVerifyError::new("scoped native method belongs to another execution owner")); }
            FullVerifier::verify_scoped_native_method_instruction(self.decoder.store, generic, instruction)?;
            return Ok(Some(operation));
        }
        let Some(id) = generic.ground_native_call_at(instruction)? else {
            if generic.native_call_originally_prepared(instruction) { return Err(IrVerifyError::new("native method lacks its original prepared authority")); }
            return Ok(None);
        };
        let proof = generic.ground_native_call(id)?;
        let super::generic::PreparedOperationAuthority::Registry { operation, .. } = proof.contract.authority else { return Err(IrVerifyError::new("native method lacks registry authority")); };
        if proof.contract.receiver.is_none() || !matches!(proof.contract.registry_owner, crate::sema::registry_graph::RegistryOwner::Method(_)) {
            return Err(IrVerifyError::new("native method lacks its original receiver contract"));
        }
        self.ground_native_call(instruction, operation)?;
        Ok(Some(operation))
    }
    pub(in crate::runtime::eval) fn callable_value(&self, instruction: u32) -> Result<Option<super::generic::CallableValueId>, IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("callable creation belongs to another body")); }
        self.generic_evidence().map_or(Ok(None), |store| store.callable_value_at(instruction))
    }
    pub(in crate::runtime::eval) fn native_callable_value(&self, instruction: u32) -> Result<super::generic::NativeCallableValueId, IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) {
            return Err(IrVerifyError::new("native callable creation belongs to another body"));
        }
        let evidence = self.generic_evidence().ok_or_else(|| IrVerifyError::new("native callable creation has no evidence"))?;
        let id = evidence.native_callable_value_at(instruction)?.ok_or_else(|| IrVerifyError::new("native callable creation has no original authority"))?;
        let source = &evidence.native_callable_value(id)?.source;
        let owner = if let Some(driver) = driver_owner_index(self.decoder.owner) { InstructionOwner::Driver(driver as u32) } else {
            InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("native callable owner is invalid"))?)
        };
        if source.instruction != instruction || source.owner != owner || source.scope != self.generic_scope() {
            return Err(IrVerifyError::new("native callable creation changes its original instruction, owner, or scope"));
        }
        Ok(id)
    }
    pub(in crate::runtime::eval) fn native_invocation_plan(&self, instruction: u32) -> Result<Option<super::generic::NativeInvocationPlanId>, IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) {
            return Err(IrVerifyError::new("native invocation belongs to another body"));
        }
        let Some(evidence) = self.generic_evidence() else { return Ok(None); };
        let Some(id) = evidence.native_invocation_plan_at(instruction)? else { return Ok(None); };
        let source = &evidence.native_invocation_plan(id)?.source;
        let owner = if let Some(driver) = driver_owner_index(self.decoder.owner) { InstructionOwner::Driver(driver as u32) } else {
            InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("native invocation owner is invalid"))?)
        };
        if source.instruction != instruction || source.owner != owner || source.scope != self.generic_scope() {
            return Err(IrVerifyError::new("native invocation changes its original instruction, owner, or scope"));
        }
        Ok(Some(id))
    }
    pub(in crate::runtime::eval) fn invocation_plan(&self, instruction: u32) -> Result<Option<super::generic::InvocationPlanId>, IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("invocation belongs to another body")); }
        self.generic_evidence().map_or(Ok(None), |store| store.invocation_plan_at(instruction))
    }
    pub(in crate::runtime::eval) fn user_invocation_authority(&self, instruction: u32) -> Result<Option<super::generic::UserInvocationAuthority>, IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("invocation belongs to another body")); }
        let Some(store) = self.generic_evidence() else { return Ok(None); };
        if let Some(id) = store.invocation_plan_at(instruction)? {
            let proof = store.invocation_plan(id)?;
            let source = store.invocation_source(proof.source)?;
            let owner = if let Some(driver) = driver_owner_index(self.decoder.owner) { InstructionOwner::Driver(driver as u32) } else {
                InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("invocation instruction owner is invalid"))?)
            };
            if source.instruction != instruction || source.owner != owner { return Err(IrVerifyError::new("invocation belongs to another owner or instruction")); }
            return Ok(Some(super::generic::UserInvocationAuthority::Ground(id)));
        }
        let Some(source) = store.scoped_invocation_source_at(instruction)? else { return Ok(None); };
        let source = store.scoped_invocation_source(source)?;
        if self.generic_scope() != Some(source.scope) { return Err(IrVerifyError::new("scoped invocation belongs to another body scope")); }
        let instance = self.instantiation.ok_or_else(|| IrVerifyError::new("scoped invocation has no active instance"))?;
        store.scoped_invocation_authority(instruction, instance)
    }
    pub(in crate::runtime::eval) fn prepared_result_constructor(&self, instruction: u32, expected: super::generic::ScopedOperationCode) -> Result<(), IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("constructor instruction belongs to another executable body")); }
        let Some(generic) = self.generic_evidence() else { return Ok(()); };
        let Some(source) = generic.scoped_operation_source_at(instruction)? else { return Ok(()); };
        if Some(generic.scoped_operation_source(source)?.scope) != self.generic_scope() { return Err(IrVerifyError::new("constructor proof belongs to another body scope")); }
        match generic.scoped_operation_authority(instruction, self.instantiation)? {
            Some(actual) if actual == expected => Ok(()),
            _ => Err(IrVerifyError::new("scoped constructor operation is missing or has another code")),
        }
    }
    pub(in crate::runtime::eval) fn active_instantiation(&self) -> Option<InstantiationId> { self.instantiation }
    pub(in crate::runtime::eval) fn has_declared_result_channel(&self) -> Result<bool, IrVerifyError> {
        let Some(owner) = IrFunctionId::from_raw(self.decoder.owner) else { return Ok(false); };
        if let Some(scope) = self.generic_scope() {
            let evidence = self.generic_evidence().ok_or_else(|| IrVerifyError::new("generic return channel lacks scoped evidence"))?;
            let scope = evidence.scope(scope)?;
            if matches!(scope.return_plan, GenericReturnPlan::Result | GenericReturnPlan::ResultUnit) { return Ok(true); }
            return match scope.result {
                TypeRef::Ground(ty) => Ok(self.decoder.store.semantic.type_tag(ty)? == super::semantic::TypeTag::Result),
                TypeRef::Template(id) => Ok(matches!(evidence.template(id)?, super::generic::TypeTemplate::Result { .. })),
                TypeRef::Rigid(_) => Ok(false),
            };
        }
        let function = self.decoder.store.functions.get(owner.index()).ok_or_else(|| IrVerifyError::new("return channel function is out of bounds"))?;
        let signature = SignatureId::from_raw(function.signature).ok_or_else(|| IrVerifyError::new("return channel lacks a fixed function signature"))?;
        Ok(self.decoder.store.semantic.type_tag(self.decoder.store.semantic.signature_return_type(signature)?)? == super::semantic::TypeTag::Result)
    }
    pub(in crate::runtime::eval) fn constructor_layout(&self, instruction: u32) -> Result<Option<super::generic::PhysicalLayoutId>, IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("generic constructor belongs to another body")); }
        let Some(store) = self.generic_evidence() else { return Ok(None); };
        let Some(constructor) = store.constructor(instruction) else { return Ok(None); };
        store.layout(constructor.layout)?;
        Ok(Some(constructor.layout))
    }
    pub(in crate::runtime::eval) fn requirement_witness(&self, instruction: u32) -> Result<Option<RequirementWitness>, IrVerifyError> {
        let Some(store) = self.generic_evidence() else { return Ok(None); };
        let Some(use_) = store.requirement_use(instruction) else { return Ok(None); };
        if Some(use_.scope) != self.generic_scope() { return Err(IrVerifyError::new("generic requirement use belongs to another function scope")); }
        let instance = store.instance(self.instantiation.ok_or_else(|| IrVerifyError::new("generic requirement use lacks frame evidence"))?)?;
        if instance.scope != use_.scope { return Err(IrVerifyError::new("generic requirement frame proof belongs to another scope")); }
        instance.requirements.get(use_.requirement as usize).copied().map(Some).ok_or_else(|| IrVerifyError::new("generic requirement witness is missing"))
    }
    pub(in crate::runtime::eval) fn call_instantiation(&self, instruction: u32) -> Result<Option<InstantiationId>, IrVerifyError> {
        let Some(store) = self.generic_evidence() else { return Ok(None); };
        let Some(call) = store.call(instruction) else { return Ok(None); };
        if !self.decoder.instruction_range.contains(&(instruction as usize)) { return Err(IrVerifyError::new("generic call proof lies outside its caller body")); }
        let id = match call.evidence {
            CallEvidence::Ground(instance) => instance,
            CallEvidence::Forwarded(plan) => store.forwarded_instance(plan, self.instantiation.ok_or_else(|| IrVerifyError::new("generic forwarding call lacks frame evidence"))?)?,
        };
        if store.instance(id)?.scope != call.target { return Err(IrVerifyError::new("generic call proof targets another scope")); }
        Ok(Some(id))
    }
    pub(in crate::runtime::eval) fn generic_evidence(&self) -> Option<&'a GenericEvidenceStore> { self.decoder.store.generic.as_deref() }

    pub(in crate::runtime::eval) fn prepared_byte_at_fallback(&self, instruction: u32) -> Result<(), IrVerifyError> {
        if !self.decoder.instruction_range.contains(&(instruction as usize)) {
            return Err(IrVerifyError::new("folded byte lookup belongs to another body"));
        }
        let generic = self.generic_evidence().ok_or_else(|| IrVerifyError::new("folded byte lookup lacks its original source proof"))?;
        let id = generic.native_scalar_at(instruction)?.ok_or_else(|| IrVerifyError::new("folded byte lookup lacks its original source proof"))?;
        let source = generic.native_scalar_source(id)?;
        if source.byte_at_fallback.is_none() {
            return Err(IrVerifyError::new("folded byte lookup has another scalar protocol"));
        }
        let owner = if let Some(step) = driver_owner_index(self.decoder.owner) {
            InstructionOwner::Driver(step as u32)
        } else {
            InstructionOwner::Function(IrFunctionId::from_raw(self.decoder.owner).ok_or_else(|| IrVerifyError::new("folded byte lookup owner is invalid"))?)
        };
        FullVerifier::verify_native_scalar_operand(self.decoder.store, generic, instruction, owner, &Type::Int, &mut Vec::new())
    }
    pub(in crate::runtime::eval) fn generic_scope(&self) -> Option<SchemeScopeId> {
        let owner = IrFunctionId::from_raw(self.decoder.owner)?;
        if owner.index() >= self.decoder.store.functions.len() { return None; }
        self.generic_evidence()?.scope_for_function(owner)
    }
    pub(in crate::runtime::eval) fn thread_local(&self) -> Self {
        Self {
            instantiation: self.instantiation,
            decoder: FullDecoder {
                store: self.decoder.store,
                owner: self.decoder.owner,
                instruction_range: self.decoder.instruction_range.clone(),
                instruction_states: None,
                block_states: None,
                slot_count: self.decoder.slot_count,
                pattern_tree: None, pattern_ceiling: Cell::new(usize::MAX),
                verified: self.decoder.verified,
            },
        }
    }

    pub(in crate::runtime::eval) fn string(&self, raw: u32) -> Result<&str, IrVerifyError> {
        self.decoder.store.string(raw)
    }

    pub(in crate::runtime::eval) fn function_identity(
        &self,
    ) -> Result<(LoweredFunctionKey, LoweredFunctionKind), IrVerifyError> {
        let id = IrFunctionId::from_raw(self.decoder.owner)
            .ok_or_else(|| IrVerifyError::new("indexed execution owner is not a function"))?;
        let function = self
            .decoder
            .store
            .functions
            .get(id.index())
            .ok_or_else(|| IrVerifyError::new("function id is out of bounds"))?;
        let metadata = self
            .decoder
            .store
            .function_metadata
            .get(id.index())
            .copied()
            .ok_or_else(|| IrVerifyError::new("function metadata is missing"))?;
        let name = Name::intern(self.decoder.store.string(function.name)?);
        let key = if metadata.owner == IR_NONE {
            LoweredFunctionKey::Name(name)
        } else {
            LoweredFunctionKey::Qualified(QualifiedName::new(
                Name::intern(self.decoder.store.string(metadata.owner)?),
                name,
            ))
        };
        let kind = if metadata.flags & 1 == 0 {
            LoweredFunctionKind::Pure
        } else {
            LoweredFunctionKind::Proc
        };
        Ok((key, kind))
    }

    #[inline(always)]
    pub(in crate::runtime::eval) fn instruction_id(
        &self,
        raw: u32,
    ) -> Result<(FullTag, FullPayload<'a>), IrVerifyError> {
        if self.decoder.verified {
            let index = raw as usize;
            let tag = unsafe { *self.decoder.store.tags.get_unchecked(index) };
            let data = unsafe { *self.decoder.store.data.get_unchecked(index) };
            let words = unsafe { self.decoder.store.payload_unchecked(data.range()) };
            return Ok((
                tag,
                FullPayload {
                    cursor: FullCursor::verified(words),
                },
            ));
        }
        let index = raw as usize;
        if !self.decoder.instruction_range.contains(&index) {
            return Err(IrVerifyError::new(
                "full IR instruction belongs to another function",
            ));
        }
        let tag = self
            .decoder
            .store
            .tags
            .get(index)
            .copied()
            .ok_or_else(|| IrVerifyError::new("full IR instruction is out of bounds"))?;
        let data = self.decoder.store.data[index];
        Ok((
            tag,
            FullPayload {
                cursor: self
                    .decoder
                    .cursor(self.decoder.store.payload(data.range())?),
            },
        ))
    }

    pub(in crate::runtime::eval) fn pattern(
        &self,
        raw: u32,
    ) -> Result<(FullPatternTag, FullPayload<'a>), IrVerifyError> {
        let index = raw as usize;
        if self.decoder.verified {
            let tag = unsafe { *self.decoder.store.patterns.get_unchecked(index) };
            let data = unsafe { *self.decoder.store.pattern_data.get_unchecked(index) };
            return Ok((
                tag,
                FullPayload {
                    cursor: self
                        .decoder
                        .cursor(unsafe { self.decoder.store.payload_unchecked(data.range()) }),
                },
            ));
        }
        let tag = self
            .decoder
            .store
            .patterns
            .get(index)
            .copied()
            .ok_or_else(|| IrVerifyError::new("pattern id is out of bounds"))?;
        let data = self.decoder.store.pattern_data[index];
        Ok((
            tag,
            FullPayload {
                cursor: self
                    .decoder
                    .cursor(self.decoder.store.payload(data.range())?),
            },
        ))
    }

    #[inline(always)]
    pub(in crate::runtime::eval) fn instruction(
        &self,
        input: &mut FullPayload<'a>,
    ) -> Result<(u32, FullTag, FullPayload<'a>), IrVerifyError> {
        let (index, tag, payload) = self.decoder.instruction(&mut input.cursor)?;
        Ok((
            u32::try_from(index)
                .map_err(|_| IrVerifyError::new("full IR instruction id overflows"))?,
            tag,
            FullPayload { cursor: payload },
        ))
    }

    #[inline(always)]
    pub(in crate::runtime::eval) fn block(
        &self,
        input: &mut FullPayload<'a>,
        expected_flags: u8,
    ) -> Result<(IrBlockId, FullPayload<'a>), IrVerifyError> {
        let (id, block) = self.decoder.block(&mut input.cursor, expected_flags)?;
        Ok((id, FullPayload { cursor: block }))
    }

    #[inline(always)]
    pub(in crate::runtime::eval) fn block_id(
        &self,
        raw: u32,
        expected_flags: u8,
    ) -> Result<(IrBlockId, FullPayload<'a>), IrVerifyError> {
        let id = IrBlockId::from_raw(raw)
            .ok_or_else(|| IrVerifyError::new("full IR block id is invalid"))?;
        if self.decoder.verified {
            let block = unsafe { *self.decoder.store.blocks.get_unchecked(id.index()) };
            debug_assert!(
                block.owner == IR_NONE || block.owner == self.decoder.owner,
                "verified block owner changed"
            );
            debug_assert_eq!(
                block.flags & BLOCK_SEQUENCE_KIND_MASK,
                expected_flags,
                "verified block kind changed"
            );
            debug_assert_eq!(block.result, IR_NONE, "verified block result changed");
            return Ok((
                id,
                FullPayload {
                    cursor: self.decoder.cursor(unsafe {
                        self.decoder.store.payload_unchecked(block.instructions)
                    }),
                },
            ));
        }
        let block = self
            .decoder
            .store
            .blocks
            .get(id.index())
            .copied()
            .ok_or_else(|| IrVerifyError::new("full IR block id is out of bounds"))?;
        if block.owner != IR_NONE && block.owner != self.decoder.owner {
            return Err(IrVerifyError::new(
                "full IR block belongs to another executable owner",
            ));
        }
        if block.flags & BLOCK_SEQUENCE_KIND_MASK != expected_flags {
            return Err(IrVerifyError::new("full IR block kind is invalid"));
        }
        if block.result != IR_NONE {
            return Err(IrVerifyError::new(
                "full IR statement/list block has an unexpected result",
            ));
        }
        Ok((
            id,
            FullPayload {
                cursor: self
                    .decoder
                    .cursor(self.decoder.store.payload(block.instructions)?),
            },
        ))
    }

    pub(in crate::runtime::eval) fn pattern_id(
        &self,
        raw: u32,
    ) -> Result<(FullPatternTag, FullPayload<'a>), IrVerifyError> {
        let index = raw as usize;
        if self.decoder.verified {
            let tag = unsafe { *self.decoder.store.patterns.get_unchecked(index) };
            let data = unsafe { *self.decoder.store.pattern_data.get_unchecked(index) };
            let words = unsafe { self.decoder.store.payload_unchecked(data.range()) };
            return Ok((
                tag,
                FullPayload {
                    cursor: FullCursor::verified(words),
                },
            ));
        }
        let tag = self
            .decoder
            .store
            .patterns
            .get(index)
            .copied()
            .ok_or_else(|| IrVerifyError::new("pattern id is out of bounds"))?;
        let data = self.decoder.store.pattern_data[index];
        Ok((
            tag,
            FullPayload {
                cursor: self
                    .decoder
                    .cursor(self.decoder.store.payload(data.range())?),
            },
        ))
    }

    #[inline(always)]
    pub(in crate::runtime::eval) fn stage_id(
        &self,
        raw: u32,
    ) -> Result<(FullStageTag, FullPayload<'a>), IrVerifyError> {
        let index = raw as usize;
        if self.decoder.verified {
            let tag = unsafe { *self.decoder.store.stages.get_unchecked(index) };
            let data = unsafe { *self.decoder.store.stage_data.get_unchecked(index) };
            let words = unsafe { self.decoder.store.payload_unchecked(data.range()) };
            return Ok((
                tag,
                FullPayload {
                    cursor: FullCursor::verified(words),
                },
            ));
        }
        let tag = self
            .decoder
            .store
            .stages
            .get(index)
            .copied()
            .ok_or_else(|| IrVerifyError::new("pipeline stage id is out of bounds"))?;
        let data = self.decoder.store.stage_data[index];
        Ok((
            tag,
            FullPayload {
                cursor: self
                    .decoder
                    .cursor(self.decoder.store.payload(data.range())?),
            },
        ))
    }

    pub(in crate::runtime::eval) fn value_id(
        &self,
        raw: u32,
    ) -> Result<(FullValueTag, FullPayload<'a>), IrVerifyError> {
        let index = raw as usize;
        if self.decoder.verified {
            let tag = unsafe { *self.decoder.store.values.get_unchecked(index) };
            let data = unsafe { *self.decoder.store.value_data.get_unchecked(index) };
            let words = unsafe { self.decoder.store.payload_unchecked(data.range()) };
            return Ok((
                tag,
                FullPayload {
                    cursor: FullCursor::verified(words),
                },
            ));
        }
        let tag = self
            .decoder
            .store
            .values
            .get(index)
            .copied()
            .ok_or_else(|| IrVerifyError::new("literal value id is out of bounds"))?;
        let data = self.decoder.store.value_data[index];
        Ok((
            tag,
            FullPayload {
                cursor: self
                    .decoder
                    .cursor(self.decoder.store.payload(data.range())?),
            },
        ))
    }

    #[inline(always)]
    pub(in crate::runtime::eval) fn finish_instruction(&self, instruction: u32) {
        self.decoder.finish_instruction(instruction as usize);
    }

    #[inline(always)]
    pub(in crate::runtime::eval) fn finish_block(&self, block: IrBlockId) {
        self.decoder.finish_block(block);
    }
}

impl<'a> FullCursor<'a> {
    #[inline(always)]
    fn new(words: &'a [u32]) -> Self {
        Self {
            words,
            index: 0,
            verified: false,
        }
    }

    #[inline(always)]
    fn verified(words: &'a [u32]) -> Self {
        Self {
            words,
            index: 0,
            verified: true,
        }
    }

    #[inline(always)]
    fn raw(&mut self) -> Result<u32, IrVerifyError> {
        if self.verified {
            let value = unsafe { *self.words.get_unchecked(self.index) };
            self.index += 1;
            return Ok(value);
        }
        let value = self
            .words
            .get(self.index)
            .copied()
            .ok_or_else(|| IrVerifyError::new("full IR payload ended early"))?;
        self.index += 1;
        Ok(value)
    }

    #[inline(always)]
    fn finish(self) -> Result<(), IrVerifyError> {
        if self.verified {
            return Ok(());
        }
        if self.index == self.words.len() {
            Ok(())
        } else {
            Err(IrVerifyError::new("full IR payload has trailing words"))
        }
    }
}

pub(in crate::runtime::eval) struct FullDecoder<'a> {
    store: &'a FullStore,
    owner: u32,
    instruction_range: std::ops::Range<usize>,
    instruction_states: Option<RefCell<Vec<u8>>>,
    block_states: Option<RefCell<Vec<u8>>>,
    slot_count: u32,
    pattern_ceiling: Cell<usize>,
    pattern_tree: Option<&'a std::sync::Mutex<super::pattern::PatternTreeBuilder>>,
    verified: bool,
}

impl<'a> FullDecoder<'a> {
    #[inline(always)]
    fn cursor(&self, words: &'a [u32]) -> FullCursor<'a> {
        if self.verified {
            FullCursor::verified(words)
        } else {
            FullCursor::new(words)
        }
    }

    fn pattern_list_cursor(&self, input: &mut FullCursor<'_>) -> Result<FullCursor<'a>, IrVerifyError> {
        let block = IrBlockId::from_raw(input.raw()?).ok_or_else(|| IrVerifyError::new("pattern child list id is invalid"))?;
        let block = self.store.blocks.get(block.index()).ok_or_else(|| IrVerifyError::new("pattern child list id is out of bounds"))?;
        // Inspection follows structural verification and must not claim block
        // ownership a second time while reading the same capture contract.
        Ok(FullCursor::new(self.store.payload(block.instructions)?))
    }

    fn pattern_capture_slots(&self, pattern: usize) -> Result<BTreeSet<usize>, IrVerifyError> {
        let mut slots = BTreeSet::new();
        let mut pending = vec![pattern];
        while let Some(pattern) = pending.pop() {
            let tag = *self.store.patterns.get(pattern).ok_or_else(|| IrVerifyError::new("pattern id is out of bounds"))?;
            let mut payload = FullCursor::new(self.store.payload(self.store.pattern_data[pattern].range())?);
            let mut captures = Vec::new();
            match tag {
                FullPatternTag::Bind => captures.push(usize::decode(self, &mut payload)?),
                FullPatternTag::Alias => { pending.push(payload.raw()? as usize); captures.push(usize::decode(self, &mut payload)?); }
                FullPatternTag::Type => { Type::decode(self, &mut payload)?; captures.extend(Option::<usize>::decode(self, &mut payload)?); }
                FullPatternTag::ResultOk | FullPatternTag::ResultErr => captures.extend(Option::<usize>::decode(self, &mut payload)?),
                FullPatternTag::ResultTest => { bool::decode(self, &mut payload)?; pending.push(payload.raw()? as usize); }
                FullPatternTag::Tag => {
                    // Enum payloads retain both the declaring type and variant names.
                    Name::decode(self, &mut payload)?;
                    Name::decode(self, &mut payload)?;
                    captures.extend(BuildPatternIdSlots::decode(self, &mut payload)?.into_iter().flatten());
                }
                FullPatternTag::ErrorVariant => {
                    Name::decode(self, &mut payload)?; Name::decode(self, &mut payload)?;
                    captures.extend(Box::<LoweredErrorPatternFields>::decode(self, &mut payload)?.iter().filter_map(|(_, slot)| *slot));
                }
                FullPatternTag::List | FullPatternTag::TagTest | FullPatternTag::Alternation => {
                    if tag == FullPatternTag::TagTest {
                        Name::decode(self, &mut payload)?;
                        Name::decode(self, &mut payload)?;
                    }
                    let mut children = self.pattern_list_cursor(&mut payload)?;
                    let count = children.raw()? as usize;
                    for index in 0..count {
                        let child = children.raw()? as usize;
                        if tag != FullPatternTag::Alternation || index == 0 { pending.push(child); }
                    }
                    if tag == FullPatternTag::List && bool::decode(self, &mut payload)? { pending.push(payload.raw()? as usize); }
                }
                FullPatternTag::RecordTest | FullPatternTag::ErrorTest => {
                    if tag == FullPatternTag::ErrorTest { Name::decode(self, &mut payload)?; Name::decode(self, &mut payload)?; }
                    let mut children = self.pattern_list_cursor(&mut payload)?;
                    let count = children.raw()? as usize;
                    for _ in 0..count { Name::decode(self, &mut children)?; pending.push(children.raw()? as usize); }
                }
                _ => {}
            }
            for slot in captures {
                if !slots.insert(slot) { return Err(IrVerifyError::new("pattern writes the same capture slot twice")); }
            }
        }
        Ok(slots)
    }

    #[inline(always)]
    fn block(
        &self,
        input: &mut FullCursor<'_>,
        expected_flags: u8,
    ) -> Result<(IrBlockId, FullCursor<'a>), IrVerifyError> {
        let raw = input.raw()?;
        if self.verified {
            let id = unsafe { IrBlockId::from_raw(raw).unwrap_unchecked() };
            let block = unsafe { *self.store.blocks.get_unchecked(id.index()) };
            debug_assert!(
                block.owner == IR_NONE || block.owner == self.owner,
                "verified block owner changed"
            );
            debug_assert_eq!(
                block.flags & BLOCK_SEQUENCE_KIND_MASK,
                expected_flags,
                "verified block kind changed"
            );
            debug_assert_eq!(block.result, IR_NONE, "verified block result changed");
            return Ok((
                id,
                self.cursor(unsafe { self.store.payload_unchecked(block.instructions) }),
            ));
        }
        let id = IrBlockId::from_raw(raw)
            .ok_or_else(|| IrVerifyError::new("full IR block id is invalid"))?;
        let block = self
            .store
            .blocks
            .get(id.index())
            .copied()
            .ok_or_else(|| IrVerifyError::new("full IR block id is out of bounds"))?;
        if block.owner != IR_NONE && block.owner != self.owner {
            return Err(IrVerifyError::new(
                "full IR block belongs to another executable owner",
            ));
        }
        if block.flags & BLOCK_SEQUENCE_KIND_MASK != expected_flags {
            return Err(IrVerifyError::new("full IR block kind is invalid"));
        }
        if block.result != IR_NONE {
            return Err(IrVerifyError::new(
                "full IR statement/list block has an unexpected result",
            ));
        }
        if block.owner != IR_NONE
            && let Some(states) = &self.block_states
        {
            let state = states.borrow()[id.index()];
            match state {
                0 => states.borrow_mut()[id.index()] = 1,
                1 => {
                    return Err(IrVerifyError::new("full IR block graph contains a cycle"));
                }
                2 => {
                    return Err(IrVerifyError::new(
                        "full IR block is owned by multiple parents",
                    ));
                }
                _ => unreachable!("block verifier state is bounded"),
            }
        }
        Ok((id, self.cursor(self.store.payload(block.instructions)?)))
    }

    fn finish_block(&self, id: IrBlockId) {
        if self.store.blocks[id.index()].owner != IR_NONE
            && let Some(states) = &self.block_states
        {
            states.borrow_mut()[id.index()] = 2;
        }
    }

    #[inline(always)]
    fn instruction(
        &self,
        input: &mut FullCursor<'_>,
    ) -> Result<(usize, FullTag, FullCursor<'a>), IrVerifyError> {
        let index = input.raw()? as usize;
        if self.verified {
            let tag = unsafe { *self.store.tags.get_unchecked(index) };
            let data = unsafe { *self.store.data.get_unchecked(index) };
            return Ok((
                index,
                tag,
                self.cursor(unsafe { self.store.payload_unchecked(data.range()) }),
            ));
        }
        if !self.instruction_range.contains(&index) {
            return Err(IrVerifyError::new(
                "full IR instruction belongs to another function",
            ));
        }
        let local = index - self.instruction_range.start;
        if let Some(states) = &self.instruction_states {
            let state = states.borrow()[local];
            match state {
                0 => states.borrow_mut()[local] = 1,
                1 => {
                    return Err(IrVerifyError::new(
                        "full IR instruction graph contains a cycle",
                    ));
                }
                2 => {
                    return Err(IrVerifyError::new(
                        "full IR instruction is owned by multiple parents",
                    ));
                }
                _ => unreachable!("instruction verifier state is bounded"),
            }
        }
        let tag = self
            .store
            .tags
            .get(index)
            .copied()
            .ok_or_else(|| IrVerifyError::new("full IR instruction is out of bounds"))?;
        let data = self.store.data[index];
        if let Some(tree) = self.pattern_tree { tree.lock().map_err(|_| IrVerifyError::new("pattern verifier traversal was interrupted"))?.enter(index as u32)?; }
        Ok((index, tag, self.cursor(self.store.payload(data.range())?)))
    }

    #[inline(always)]
    fn verify_comparison_chain_shape(&self, mut payload: FullCursor<'_>) -> Result<(), IrVerifyError> {
        let id = IrBlockId::from_raw(payload.raw()?).ok_or_else(|| IrVerifyError::new("comparison chain block id is invalid"))?;
        let block = self.store.blocks.get(id.index()).ok_or_else(|| IrVerifyError::new("comparison chain block is missing"))?;
        let mut pairs = self.cursor(self.store.payload(block.instructions)?);
        let len = pairs.raw()? as usize;
        if len < 2 { return Err(IrVerifyError::new("comparison chain requires at least two pairs")); }
        for _ in 0..len {
            let pair = pairs.raw()? as usize;
            if self.store.tags.get(pair) != Some(&FullTag::ExprBinary) { return Err(IrVerifyError::new("comparison chain requires binary pairs")); }
            let mut pair_payload = self.cursor(self.store.payload(self.store.data[pair].range())?);
            let op = BinaryOp::decode(self, &mut pair_payload)?;
            if !matches!(op, BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge) { return Err(IrVerifyError::new("comparison chain requires ordering operators")); }
        }
        pairs.finish()
    }

    fn verify_retry_selection(&self, mut payload: FullCursor<'_>) -> Result<(), IrVerifyError> {
        payload.raw()?;
        if bool::decode(self, &mut payload)? && !self.pattern_capture_slots(payload.raw()? as usize)?.is_empty() {
            return Err(IrVerifyError::new("retry selection pattern cannot bind slots"));
        }
        Ok(())
    }

    fn finish_instruction(&self, index: usize) {
        if let Some(tree) = self.pattern_tree { tree.lock().expect("pattern verifier traversal has exclusive temporary ownership").exit(index as u32); }
        if let Some(states) = &self.instruction_states {
            states.borrow_mut()[index - self.instruction_range.start] = 2;
        }
    }

    fn finish_function(&self) -> Result<(), IrVerifyError> {
        let instructions_complete = self
            .instruction_states
            .as_ref()
            .is_none_or(|states| states.borrow().iter().all(|state| *state == 2));
        let blocks_complete = self.block_states.as_ref().is_none_or(|states| {
            self.store
                .blocks
                .iter()
                .zip(states.borrow().iter())
                .all(|(block, state)| block.owner != self.owner || *state == 2)
        });
        if instructions_complete && blocks_complete {
            Ok(())
        } else {
            Err(IrVerifyError::new(
                "full IR contains an instruction or block not owned by the function body",
            ))
        }
    }
}

pub(super) struct FullVerifier;

#[derive(Clone, Copy)]
enum StageItemInput {
    Sequence(u32),
    GroundCallback(u32),
}

fn indexed_block_can_return(store: &FullStore, block: IrBlockId) -> Result<bool, IrVerifyError> {
    let block = store
        .blocks
        .get(block.index())
        .ok_or_else(|| IrVerifyError::new("return-analysis block is out of bounds"))?;
    let words = store.payload(block.instructions)?;
    let Some((&len, instructions)) = words.split_first() else {
        return Err(IrVerifyError::new("return-analysis block is empty"));
    };
    if instructions.len() != len as usize {
        return Err(IrVerifyError::new(
            "return-analysis block length is invalid",
        ));
    }
    for instruction in instructions {
        if indexed_stmt_can_return(store, *instruction as usize)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn indexed_stmt_can_return(store: &FullStore, instruction: usize) -> Result<bool, IrVerifyError> {
    let tag = store
        .tags
        .get(instruction)
        .copied()
        .ok_or_else(|| IrVerifyError::new("return-analysis instruction is out of bounds"))?;
    let data = *store
        .data
        .get(instruction)
        .ok_or_else(|| IrVerifyError::new("return-analysis data is out of bounds"))?;
    let payload = store.payload(data.range())?;
    let block = |raw| {
        IrBlockId::from_raw(raw)
            .ok_or_else(|| IrVerifyError::new("return-analysis block id is invalid"))
    };
    Ok(match tag {
        FullTag::StmtReturn => true,
        FullTag::StmtCd => indexed_block_can_return(
            store,
            block(
                *payload
                    .get(1)
                    .ok_or_else(|| IrVerifyError::new("return-analysis cd payload is invalid"))?,
            )?,
        )?,
        FullTag::StmtEnv => {
            indexed_block_can_return(
                store,
                block(*payload.get(1).ok_or_else(|| {
                    IrVerifyError::new("return-analysis env payload is invalid")
                })?)?,
            )?
        }
        FullTag::StmtIf | FullTag::StmtIfBool | FullTag::StmtPatternIf => {
            let branches = block(
                *payload
                    .first()
                    .ok_or_else(|| IrVerifyError::new("return-analysis if payload is invalid"))?,
            )?;
            let branch_words = store.payload(store.blocks[branches.index()].instructions)?;
            let Some((&len, mut branch_words)) = branch_words.split_first() else {
                return Err(IrVerifyError::new(
                    "return-analysis if branches are invalid",
                ));
            };
            let mut all_return = len != 0;
            for _ in 0..len {
                let Some((_, rest)) = branch_words.split_first() else {
                    return Err(IrVerifyError::new(
                        "return-analysis if condition is missing",
                    ));
                };
                let Some((&body, rest)) = rest.split_first() else {
                    return Err(IrVerifyError::new("return-analysis if body is missing"));
                };
                all_return &= indexed_block_can_return(store, block(body)?)?;
                branch_words = if tag == FullTag::StmtPatternIf {
                    rest.get(1..).ok_or_else(|| IrVerifyError::new("return-analysis captures are missing"))?
                } else { rest };
            }
            if !branch_words.is_empty() {
                return Err(IrVerifyError::new(
                    "return-analysis if branches have trailing data",
                ));
            }
            let else_payload = if tag == FullTag::StmtPatternIf {
                payload.get(1..payload.len().saturating_sub(1)).unwrap_or_default()
            } else { payload.get(1..).unwrap_or_default() };
            let else_returns = match else_payload {
                [1, body] => indexed_block_can_return(store, block(*body)?)?,
                [0] => false,
                _ => {
                    return Err(IrVerifyError::new(
                        "return-analysis else payload is invalid",
                    ));
                }
            };
            all_return && else_returns
        }
        FullTag::StmtWith => {
            let body = *payload.get(1).ok_or_else(|| IrVerifyError::new("return-analysis with body is missing"))?;
            let else_index = match payload.get(2) { Some(0) => 3, Some(1) => 4, _ => return Err(IrVerifyError::new("return-analysis with parameter is invalid")) };
            let else_body = *payload.get(else_index).ok_or_else(|| IrVerifyError::new("return-analysis with handler is missing"))?;
            indexed_block_can_return(store, block(body)?)? && indexed_block_can_return(store, block(else_body)?)?
        }
        FullTag::StmtMatch => {
            let arms =
                block(*payload.get(1).ok_or_else(|| {
                    IrVerifyError::new("return-analysis match payload is invalid")
                })?)?;
            let arm_words = store.payload(store.blocks[arms.index()].instructions)?;
            let Some((&len, mut arm_words)) = arm_words.split_first() else {
                return Err(IrVerifyError::new("return-analysis match arms are invalid"));
            };
            let mut all_return = len != 0;
            for _ in 0..len {
                let Some((_, rest)) = arm_words.split_first() else {
                    return Err(IrVerifyError::new(
                        "return-analysis match pattern is missing",
                    ));
                };
                let Some((&guard, rest)) = rest.split_first() else {
                    return Err(IrVerifyError::new("return-analysis match guard is missing"));
                };
                let rest = match guard {
                    0 => rest,
                    1 => rest.get(1..).ok_or_else(|| {
                        IrVerifyError::new("return-analysis match guard is invalid")
                    })?,
                    _ => {
                        return Err(IrVerifyError::new(
                            "return-analysis match guard tag is invalid",
                        ));
                    }
                };
                let Some((&body, rest)) = rest.split_first() else {
                    return Err(IrVerifyError::new("return-analysis match body is missing"));
                };
                all_return &= indexed_block_can_return(store, block(body)?)?;
                arm_words = rest;
            }
            if !arm_words.is_empty() {
                return Err(IrVerifyError::new(
                    "return-analysis match arms have trailing data",
                ));
            }
            all_return
        }
        FullTag::StmtStrMatch | FullTag::StmtTagMatch => {
            let Some((&len, mut words)) = payload.get(1..).and_then(|words| words.split_first())
            else {
                return Err(IrVerifyError::new(
                    "return-analysis exact match payload is invalid",
                ));
            };
            let mut all_return = len != 0;
            for _ in 0..len {
                let Some((_, rest)) = words.split_first() else {
                    return Err(IrVerifyError::new(
                        "return-analysis exact match key is missing",
                    ));
                };
                let Some((&body, rest)) = rest.split_first() else {
                    return Err(IrVerifyError::new(
                        "return-analysis exact match body is missing",
                    ));
                };
                all_return &= indexed_block_can_return(store, block(body)?)?;
                words = rest;
            }
            let fallback_returns = match words {
                [1, body, _span] => indexed_block_can_return(store, block(*body)?)?,
                [0, _span] => false,
                _ => {
                    return Err(IrVerifyError::new(
                        "return-analysis exact match fallback is invalid",
                    ));
                }
            };
            all_return && fallback_returns
        }
        _ => false,
    })
}

impl FullVerifier {
    fn verify_generic_symbolic_source(store: &FullStore, generic: &GenericEvidenceStore, source: u32, scope_id: SchemeScopeId, expected: TypeRef, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        let scope = generic.scope(scope_id)?;
        let range = store.function_instruction_range(scope.owner.index())?;
        if !range.contains(&(source as usize)) || active.len() >= 256 || active.contains(&source) { return Err(IrVerifyError::new("symbolic operation source is foreign, cyclic, or too deep")); }
        if let Some(id) = generic.lexical_capture_source_at(source)? {
            let capture = generic.lexical_capture_source(id)?;
            if !generic.reference_equals_ground(&store.semantic, scope_id, expected, store.semantic.to_type(capture.ty)?)? {
                return Err(IrVerifyError::new("symbolic capture changes its original operand type"));
            }
            Self::verify_lexical_capture_source(store, generic, capture)?;
            if capture.owner != InstructionOwner::Function(scope.owner) { return Err(IrVerifyError::new("symbolic capture changes its receiving declaration")); }
            return Ok(());
        }
        active.push(source);
        if Self::verify_scoped_native_method_symbolic_operand(store, generic, source, scope_id, expected, active)? { active.pop(); return Ok(()); }
        if Self::verify_formatted_path_symbolic_operand(store, generic, source, scope_id, expected, active)? { active.pop(); return Ok(()); }
        if let TypeRef::Ground(ty) = expected
            && Self::verify_error_field_operand(store, generic, source, InstructionOwner::Function(scope.owner), &store.semantic.to_type(ty)?, None, active)? { active.pop(); return Ok(()); }
        if Self::verify_conditional_symbolic_operand(store, generic, source, scope_id, expected, active)? { active.pop(); return Ok(()); }
        if let TypeRef::Ground(ty) = expected
            && generic.record_constructor_at(source)?.is_some() {
            Self::verify_record_constructor_operand(store, generic, source, InstructionOwner::Function(scope.owner), &store.semantic.to_type(ty)?, None, active)?;
            active.pop();
            return Ok(());
        }
        if let Some(body) = Self::original_compiler_argument_wrapper_body(store, generic, source, InstructionOwner::Function(generic.scope(scope_id)?.owner))? {
            Self::verify_generic_symbolic_source(store, generic, body, scope_id, expected, active)?;
            active.pop();
            return Ok(());
        }
        if let Some(saved) = generic.original_argument_binding(source) {
            if saved.owner != InstructionOwner::Function(scope.owner) || saved.scope != Some(scope_id)
                || !generic.references_equal(&store.semantic, scope_id, expected, saved.ty)? {
                return Err(IrVerifyError::new("symbolic saved argument changes its original scope or type"));
            }
            Self::verify_generic_symbolic_source(store, generic, saved.initializer, scope_id, expected, active)?;
            active.pop();
            return Ok(());
        }
        let tag = store.tags[source as usize];
        let words = store.payload(store.data[source as usize].range())?;
        let valid = match tag {
            FullTag::ExprParam => {
                if Self::verify_pattern_symbolic_operand(store, generic, source, scope_id, expected)? {
                    active.pop();
                    return Ok(());
                }
                let slot = *words.first().ok_or_else(|| IrVerifyError::new("symbolic parameter source slot is missing"))? as usize;
                if let Some(&ty) = scope.parameters.get(slot) { generic.references_equal(&store.semantic, scope_id, expected, ty)? }
                else {
                    for instruction in range.clone() { if matches!(store.tags[instruction], FullTag::StmtAssign | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt | FullTag::StmtAssignPath | FullTag::StmtAssignInt | FullTag::StmtAssignBool) && store.payload(store.data[instruction].range())?.first() == Some(&(slot as u32)) { return Err(IrVerifyError::new("symbolic local operand is mutable without assignment evidence")); } }
                    let function = store.functions[scope.owner.index()];
                    let block = IrBlockId::from_raw(function.body).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("symbolic operand body is invalid"))?;
                    let mut initializer = None;
                    for &instruction in store.payload(block.instructions)?.iter().skip(1) {
                        if instruction >= source || store.tags[instruction as usize] != FullTag::StmtLet { continue; }
                        let binding = store.payload(store.data[instruction as usize].range())?;
                        if binding.first() == Some(&(slot as u32)) { if initializer.replace(binding[1]).is_some() { return Err(IrVerifyError::new("symbolic operand has ambiguous local bindings")); } }
                    }
                    Self::verify_generic_symbolic_source(store, generic, initializer.ok_or_else(|| IrVerifyError::new("symbolic local operand lacks a dominating binding"))?, scope_id, expected, active)?;
                    true
                }
            }
            FullTag::ExprCheckedValue => { Self::verify_generic_symbolic_source(store, generic, *words.first().ok_or_else(|| IrVerifyError::new("symbolic checked operand is missing"))?, scope_id, expected, active)?; true }
            FullTag::ExprList => {
                let item = match expected {
                    TypeRef::Template(id) => match generic.template(id)? {
                        super::generic::TypeTemplate::List(item) => *item,
                        _ => return Err(IrVerifyError::new("symbolic List source has another expected constructor")),
                    },
                    TypeRef::Ground(ty) if store.semantic.type_tag(ty)? == super::semantic::TypeTag::List => {
                        TypeRef::Ground(store.semantic.type_children(ty)?.ok_or_else(|| IrVerifyError::new("symbolic List item type is missing"))?.0)
                    }
                    _ => return Err(IrVerifyError::new("symbolic List source has no checked List template")),
                };
                let block = words.first().copied().and_then(IrBlockId::from_raw).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("symbolic List source block is missing"))?;
                let items = store.payload(block.instructions)?;
                if block.flags & BLOCK_SEQUENCE_KIND_MASK != BLOCK_LIST || items.first().copied().map(|count| count as usize) != Some(items.len().saturating_sub(1)) {
                    return Err(IrVerifyError::new("symbolic List source item sequence is invalid"));
                }
                for &source in &items[1..] { Self::verify_generic_symbolic_source(store, generic, source, scope_id, item, active)?; }
                true
            }
            FullTag::ExprOk | FullTag::ExprErr => {
                Self::verify_scoped_operation_instruction(store, generic, source)?;
                let use_ = generic.requirement_use(source).ok_or_else(|| IrVerifyError::new("symbolic constructor lacks its original requirement"))?;
                if use_.scope != scope_id { return Err(IrVerifyError::new("symbolic constructor has another scope")); }
                let Some(Requirement::Operation(requirement)) = scope.requirements.get(use_.requirement as usize) else { return Err(IrVerifyError::new("symbolic constructor has another requirement kind")); };
                generic.references_equal(&store.semantic, scope_id, expected, requirement.result)?
            }
            FullTag::ExprField | FullTag::ExprBinary => {
                let use_ = generic.requirement_use(source).ok_or_else(|| IrVerifyError::new("symbolic computed operand lacks requirement evidence"))?;
                if use_.scope != scope_id { return Err(IrVerifyError::new("symbolic operand requirement has another scope")); }
                let result = match &scope.requirements[use_.requirement as usize] { Requirement::Add { result, .. } | Requirement::Projection { result, .. } | Requirement::Invocation { result, .. } => *result, Requirement::Operation(requirement) => requirement.result, Requirement::NativeMethod(requirement) => requirement.result, Requirement::Eligibility { .. } => return Err(IrVerifyError::new("eligibility requirement cannot produce a symbolic operation value")) };
                generic.references_equal(&store.semantic, scope_id, expected, result)?
            }
            FullTag::ExprCall | FullTag::ExprDirectPureCall | FullTag::ExprSelfCall if generic.call(source).is_some() => generic.call_result_matches(&store.semantic, scope_id, source, expected)?,
            _ => {
                let scalar = match tag { FullTag::ExprNull => Some(Type::Null), FullTag::ExprUnit => Some(Type::Unit), FullTag::ExprInt => Some(Type::Int), FullTag::ExprFloat => Some(Type::Float), FullTag::ExprDuration => Some(Type::Duration), FullTag::ExprBool => Some(Type::Bool), FullTag::ExprStr => Some(Type::Str), _ => None };
                if let Some(scalar) = scalar { generic.reference_equals_ground(&store.semantic, scope_id, expected, scalar)? } else { return Err(IrVerifyError::new(format!("symbolic operation source {source} ({tag:?}) lacks a prepared type proof"))); }
            }
        };
        active.pop();
        if valid { Ok(()) } else { Err(IrVerifyError::new("generic operation operand disagrees with its scoped requirement type")) }
    }

    fn verify_generic_default(store: &FullStore, generic: &GenericEvidenceStore, instance_id: InstantiationId, slot: usize) -> Result<(), IrVerifyError> {
        let instance = generic.instance(instance_id)?;
        let scope = generic.scope(instance.scope)?;
        let expected = store.semantic.to_type(instance.parameter_types[slot])?;
        Self::verify_function_default(store, generic, scope.owner, slot, &expected, Some(instance_id))
    }

    fn verify_function_default(store: &FullStore, generic: &GenericEvidenceStore, target: IrFunctionId, slot: usize, expected: &Type, instance: Option<InstantiationId>) -> Result<(), IrVerifyError> {
        let function = store.functions[target.index()];
        let params = function.params.bounds(store.params.len()).ok_or_else(|| IrVerifyError::new("default parameter range is invalid"))?;
        let param = *store.params.get(params.start + slot).ok_or_else(|| IrVerifyError::new("default parameter slot is invalid"))?;
        if slot >= params.len() || param.flags & 2 == 0 { return Err(IrVerifyError::new("default parameter slot is not defaulted")); }
        if param.flags & 4 != 0 {
            let body = IrBlockId::from_raw(function.body).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("generic default body is invalid"))?;
            let mut source = None;
            for &instruction in store.payload(body.instructions)?.iter().skip(1) {
                if store.tags[instruction as usize] != FullTag::StmtDefaultParameter { break; }
                let payload = store.payload(store.data[instruction as usize].range())?;
                if payload.first() == Some(&(slot as u32)) { source = payload.get(1).copied(); }
            }
            Self::verify_generic_source(store, generic, source.ok_or_else(|| IrVerifyError::new("generic lexical default lacks its source instruction"))?, InstructionOwner::Function(target), expected, instance, &mut Vec::new())
        } else {
            let cold = store.param_cold.binary_search_by_key(&((params.start + slot) as u32), |cold| cold.param).ok().map(|index| store.param_cold[index]).ok_or_else(|| IrVerifyError::new("generic literal default is missing"))?;
            Self::verify_generic_default_value(store, cold.default, expected, &mut Vec::new())
        }
    }

    fn verify_generic_default_value(store: &FullStore, value: u32, expected: &Type, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        let tag = *store.values.get(value as usize).ok_or_else(|| IrVerifyError::new("generic default value is out of bounds"))?;
        if active.len() >= 256 || active.contains(&value) { return Err(IrVerifyError::new("generic default value is cyclic or too deep")); }
        if *expected == Type::Any { return Ok(()); }
        if let Type::Optional(inner) = expected { if tag == FullValueTag::Null { return Ok(()); } return Self::verify_generic_default_value(store, value, inner, active); }
        active.push(value);
        let words = store.payload(store.value_data[value as usize].range())?;
        let scalar = match tag { FullValueTag::Null => Some(Type::Null), FullValueTag::Unit => Some(Type::Unit), FullValueTag::Int => Some(Type::Int), FullValueTag::Float => Some(Type::Float), FullValueTag::Duration => Some(Type::Duration), FullValueTag::Bool => Some(Type::Bool), FullValueTag::Str => Some(Type::Str), FullValueTag::Bytes => Some(Type::Bytes), FullValueTag::Regex => Some(Type::Regex), FullValueTag::Path => Some(Type::Path), _ => None };
        if let Some(actual) = scalar {
            let unsigned = actual == Type::Int && *expected == Type::UInt && words.len() == 2 && (((words[1] as u64) << 32 | words[0] as u64) as i64) >= 0;
            if &actual != expected && !unsigned { return Err(IrVerifyError::new("generic literal default disagrees with its ground type")); }
        } else { match (tag, expected) {
            (FullValueTag::ResultOk, Type::Result(ok, _)) => Self::verify_generic_default_value(store, *words.first().ok_or_else(|| IrVerifyError::new("generic Result default value is missing"))?, ok, active)?,
            (FullValueTag::List, Type::List(inner)) => {
                let block = words.first().copied().and_then(IrBlockId::from_raw).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("generic list default block is invalid"))?;
                for &value in store.payload(block.instructions)?.iter().skip(1) { Self::verify_generic_default_value(store, value, inner, active)?; }
            }
            (FullValueTag::RecordVec, Type::Record(fields)) => {
                let block = words.first().copied().and_then(IrBlockId::from_raw).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("generic record default block is invalid"))?;
                let values = store.payload(block.instructions)?;
                if values.first().copied() != Some(fields.len() as u32) || values.len() != 1 + fields.len() * 2 { return Err(IrVerifyError::new("generic record default has the wrong width")); }
                for (entry, (&name, ty)) in values[1..].chunks_exact(2).zip(fields) {
                    if Name::from_symbol(Symbol::from_raw(entry[0])) != name { return Err(IrVerifyError::new("generic record default lacks canonical physical field order")); }
                    Self::verify_generic_default_value(store, entry[1], ty, active)?;
                }
            }
            (FullValueTag::Record | FullValueTag::RecordVec | FullValueTag::Stats | FullValueTag::StatsBlob, Type::ErasedRecord) | (FullValueTag::Module, Type::DynamicModule) => {},
            _ => return Err(IrVerifyError::new("generic literal default lacks an independently prepared type proof")),
        } }
        active.pop();
        Ok(())
    }

    fn verify_generic_source(store: &FullStore, generic: &GenericEvidenceStore, source: u32, owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        let range = match owner { InstructionOwner::Function(function) => store.function_instruction_range(function.index())?, InstructionOwner::Driver(step) => store.driver_instruction_range(step as usize)? };
        if !range.contains(&(source as usize)) || active.len() >= 256 || active.contains(&source) { return Err(IrVerifyError::new("generic argument source is foreign, cyclic, or too deep")); }
        if *expected == Type::Any { return Ok(()); }
        if Self::verify_context_scope_source(store, generic, source, owner, expected)? { return Ok(()); }
        if Self::verify_run_producer_source(store, generic, source, owner, expected)? { return Ok(()); }
        if store.tags.get(source as usize) == Some(&FullTag::ExprProcessCommandArgv) && generic.ground_native_call_at(source)?.is_some() {
            return Self::verify_native_call_operand(store, generic, source, owner, expected, active);
        }
        if Self::verify_try_capture_operand(store, generic, source, owner, expected)? { return Ok(()); }
        if Self::verify_index_operand(store, generic, source, owner, expected, active)? { return Ok(()); }
        if Self::verify_comprehension_source(store, generic, source, owner, expected, instance, active)? { return Ok(()); }
        if generic.constant_source_at(source)?.is_some() { return Self::verify_constant_operand(store, generic, source, owner, expected); }
        if generic.record_update_source_at(source)?.is_some() { return Self::verify_record_update_operand(store, generic, source, owner, expected, instance, active); }
        if Self::verify_host_binding_operand(store, generic, source, owner, expected)? { return Ok(()); }
        if Self::verify_lexical_capture_operand(store, generic, source, owner, expected)? { return Ok(()); }
        if Self::verify_stage_block_callback_operand(store, generic, source, owner, expected)? { return Ok(()); }
        if Self::verify_error_constructor_operand(store, generic, source, owner, expected, active)? { return Ok(()); }
        if Self::verify_bridge_operand(store, generic, source, owner, expected)? { return Ok(()); }
        if generic.stage_pipeline_at(source)?.is_some() { return Self::verify_stage_pipeline_operand(store, generic, source, owner, expected, instance, active); }
        active.push(source);
        if Self::verify_formatted_path_operand(store, generic, source, owner, expected, instance, active)? { active.pop(); return Ok(()); }
        if Self::verify_optional_receiver_guard_result(store, generic, source, owner, expected, instance, active)? { active.pop(); return Ok(()); }
        if Self::verify_literal_comparison_operand(store, generic, source, owner, expected, instance, active)? { active.pop(); return Ok(()); }
        if Self::verify_null_optional_equality_operand(store, generic, source, owner, expected, instance, active)? { active.pop(); return Ok(()); }
        if Self::verify_conditional_operand(store, generic, source, owner, expected, instance, active)? { active.pop(); return Ok(()); }
        if Self::verify_uint_integer_arithmetic_operand(store, generic, source, owner, expected, instance, active)? { active.pop(); return Ok(()); }
        if Self::verify_scoped_native_method_operand(store, generic, source, owner, expected, instance, active)? { active.pop(); return Ok(()); }
        if Self::verify_duration_operand(store, generic, source, owner, expected, instance, active)? { active.pop(); return Ok(()); }
        if Self::verify_range_operand(store, generic, source, owner, expected, instance, active)? { active.pop(); return Ok(()); }
        if Self::verify_language_result_operand(store, generic, source, owner, expected, instance, active)? { active.pop(); return Ok(()); }
        if Self::verify_error_field_operand(store, generic, source, owner, expected, instance, active)? { active.pop(); return Ok(()); }
        if generic.native_scalar_at(source)?.is_some() {
            Self::verify_native_scalar_operand(store, generic, source, owner, expected, active)?;
            active.pop();
            return Ok(());
        }
        if generic.record_constructor_at(source)?.is_some() {
            Self::verify_record_constructor_operand(store, generic, source, owner, expected, instance, active)?;
            active.pop();
            return Ok(());
        }
        if generic.ground_container_at(source)?.is_some() {
            Self::verify_ground_container_operand(store, generic, source, owner, expected, instance, active)?;
            active.pop();
            return Ok(());
        }
        if let Some(body) = Self::original_compiler_argument_wrapper_body(store, generic, source, owner)? {
            Self::verify_generic_source(store, generic, body, owner, expected, instance, active)?;
            active.pop();
            return Ok(());
        }
        if Self::verify_value_scalar_source(store, generic, source, owner, expected, active)? {
            active.pop();
            return Ok(());
        }
        if Self::verify_fallback_operand(store, generic, source, owner, expected, instance, active)? {
            active.pop();
            return Ok(());
        }
        if let Some(saved) = generic.original_argument_binding(source) {
            if saved.owner != owner { return Err(IrVerifyError::new("saved argument read changes its original owner")); }
            let actual = match (saved.scope, saved.ty) {
                (None, TypeRef::Ground(ty)) => store.semantic.to_type(ty)?,
                (None, _) => return Err(IrVerifyError::new("saved argument has an unowned symbolic type")),
                (Some(scope), reference) => {
                    if generic.scope(scope)?.owner != match owner { InstructionOwner::Function(function) => function, InstructionOwner::Driver(_) => return Err(IrVerifyError::new("driver saved argument has a declaration scope")) } {
                        return Err(IrVerifyError::new("saved argument belongs to another body scope"));
                    }
                    if let Some(id) = instance {
                        let frame = generic.instance(id)?;
                        if frame.scope != scope { return Err(IrVerifyError::new("saved argument uses another declaration's instance")); }
                        generic.expand(&store.semantic, reference, &frame.substitutions, &mut FxHashMap::default())?
                    } else if let TypeRef::Ground(ty) = reference { store.semantic.to_type(ty)? }
                    else { return Err(IrVerifyError::new("saved argument has no active instance")); }
                }
            };
            if actual != *expected { return Err(IrVerifyError::new("saved argument read changes its original type")); }
            Self::verify_generic_source(store, generic, saved.initializer, owner, expected, instance, active)?;
            active.pop();
            return Ok(());
        }
        if let Some((body, _)) = Self::original_argument_wrapper_body(store, generic, source, owner)? {
            Self::verify_generic_source(store, generic, body, owner, expected, instance, active)?;
            active.pop();
            return Ok(());
        }
        let tag = store.tags[source as usize];
        let words = store.payload(store.data[source as usize].range())?;
        let child = |index: usize| words.get(index).copied().ok_or_else(|| IrVerifyError::new("generic source child is missing"));
        let scalar = match tag {
            FullTag::ExprNull => Some(Type::Null), FullTag::ExprUnit => Some(Type::Unit), FullTag::ExprInt => Some(Type::Int), FullTag::ExprFloat => Some(Type::Float), FullTag::ExprDuration => Some(Type::Duration), FullTag::ExprBool => Some(Type::Bool), FullTag::ExprStr => Some(Type::Str), FullTag::ExprBytes => Some(Type::Bytes), FullTag::ExprPreparedRegex => Some(Type::Regex), FullTag::ExprPath | FullTag::ExprPathFrom => Some(Type::Path), _ => None,
        };
        let expected = if let Type::Optional(inner) = expected && (scalar.is_some() || matches!(tag, FullTag::ExprRecord | FullTag::ExprList | FullTag::ExprOk | FullTag::ExprErr)) { if tag == FullTag::ExprNull { active.pop(); return Ok(()); } inner.as_ref() } else { expected };
        if let Some(actual) = scalar {
            let unsigned_literal = actual == Type::Int && *expected == Type::UInt && tag == FullTag::ExprInt && words.len() == 2 && (((words[1] as u64) << 32 | words[0] as u64) as i64) >= 0;
            if &actual != expected && !unsigned_literal { return Err(IrVerifyError::new("generic argument type disagrees with literal source")); }
        } else { match tag {
            FullTag::ExprTag | FullTag::ExprPreparedConstant if Self::verify_native_nominal_source(store, source, expected)? => {},
            FullTag::ExprCheckedValue => Self::verify_generic_source(store, generic, child(0)?, owner, expected, instance, active)?,
            FullTag::ExprRecord => {
                if generic.constructor(source).is_none() {
                    Self::verify_record_source_operand(store, generic, source, owner, expected, instance, active)?;
                    active.pop();
                    return Ok(());
                }
                let constructor = generic.constructor(source).ok_or_else(|| IrVerifyError::new("generic record argument lacks constructor layout evidence"))?;
                let layout = generic.layout(constructor.layout)?;
                if store.semantic.to_type(layout.record_type)? != *expected { return Err(IrVerifyError::new("generic record argument disagrees with its constructor type")); }
                let block = IrBlockId::from_raw(child(0)?).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("generic record source block is invalid"))?;
                let fields = store.payload(block.instructions)?;
                let Type::Record(types) = expected else { return Err(IrVerifyError::new("generic record argument expected a non-record")); };
                for field in fields.get(1..).ok_or_else(|| IrVerifyError::new("generic record fields are missing"))?.chunks_exact(3) {
                    if field[0] != 0 { return Err(IrVerifyError::new("generic record argument has an unprepared spread")); }
                    let name = Name::from_symbol(Symbol::from_raw(field[1]));
                    let ty = types.get(&name).ok_or_else(|| IrVerifyError::new("generic record source has an unexpected field"))?;
                    Self::verify_generic_source(store, generic, field[2], owner, ty, instance, active)?;
                }
            }
            FullTag::ExprList => {
                let Type::List(inner) = expected else { return Err(IrVerifyError::new("generic list argument expected a non-list")); };
                let block = IrBlockId::from_raw(child(0)?).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("generic list source block is invalid"))?;
                for &item in store.payload(block.instructions)?.get(1..).ok_or_else(|| IrVerifyError::new("generic list values are missing"))? { Self::verify_generic_source(store, generic, item, owner, inner, instance, active)?; }
            }
            FullTag::ExprOk => {
                let Type::Result(ok, _) = expected else { return Err(IrVerifyError::new("generic Result argument expected a non-Result")); };
                Self::verify_generic_source(store, generic, child(0)?, owner, ok, instance, active)?;
            }
            FullTag::ExprErr => {
                let Type::Result(_, error) = expected else { return Err(IrVerifyError::new("generic error argument expected a non-Result")); };
                Self::verify_generic_source(store, generic, child(0)?, owner, error, instance, active)?;
            }
            FullTag::ExprError => {
                if words.first() == Some(&1) { return Err(IrVerifyError::new("structured error lacks its original constructor authority")); }
                let family = if words.first() == Some(&1) { Some(Name::intern(store.string(child(1)?)?)) } else { None };
                let variant = if family.is_some() { Some(Name::intern(store.string(child(2)?)?)) } else { None };
                let valid = match expected {
                    Type::Error => matches!(words.first(), Some(0 | 1)),
                    Type::ErrorFamily(name) => family == Some(*name),
                    Type::ErrorVariant { family: name, variant: tag } => family == Some(*name) && variant == Some(*tag),
                    _ => false,
                };
                if !valid { return Err(IrVerifyError::new("generic error constructor disagrees with its nominal carrier")); }
            }
            FullTag::ExprRequire => {
                let Type::Result(ok, error) = expected else { return Err(IrVerifyError::new("generic validation producer expected a non-Result carrier")); };
                let ty = TypeId::from_raw(child(1)?).ok_or_else(|| IrVerifyError::new("generic validation producer type is invalid"))?;
                if store.semantic.to_type(ty)? != **ok || **error != Type::Error { return Err(IrVerifyError::new("generic validation producer disagrees with its checked success type")); }
            }
            FullTag::ExprEmptyMap => {
                if !matches!(expected, Type::Map(_, _)) { return Err(IrVerifyError::new("generic empty map source expected a non-map")); }
            }
            FullTag::ExprTry => {
                let original_producer = child(0)?;
                let mut producer = original_producer;
                let mut wrapped_call = None;
                let mut wrappers = Vec::new();
                loop {
                    let next = if let Some((body, call)) = Self::original_argument_wrapper_body(store, generic, producer, owner)? {
                        if wrapped_call.is_some_and(|expected| expected != call) {
                            return Err(IrVerifyError::new("propagated saved argument wrappers have another original call"));
                        }
                        wrapped_call = Some(call);
                        Some(body)
                    } else { Self::original_compiler_argument_wrapper_body(store, generic, producer, owner)? };
                    let Some(body) = next else { break; };
                    if wrappers.len() >= 256 || wrappers.contains(&producer) {
                        return Err(IrVerifyError::new("propagated compiler argument wrappers are cyclic or too deep"));
                    }
                    wrappers.push(producer);
                    producer = body;
                }
                if let Some(call) = wrapped_call
                    && generic.registered_instruction_origin(producer, false) != Some((super::generic::OperationSourceOrigin::Expression(call), owner)) {
                    return Err(IrVerifyError::new("saved argument wrapper body changes its original call"));
                }
                let producer_tag = *store.tags.get(producer as usize).ok_or_else(|| IrVerifyError::new("generic propagation producer is out of bounds"))?;
                let carrier = if let Some(call) = generic.call(producer) {
                    let id = match call.evidence { CallEvidence::Ground(id) => id, CallEvidence::Forwarded(plan) => generic.forwarded_instance(plan, instance.ok_or_else(|| IrVerifyError::new("generic propagated call lacks a frame proof"))?)? };
                    store.semantic.to_type(generic.instance(id)?.result_type)?
                } else if matches!(producer_tag, FullTag::ExprCall | FullTag::ExprDirectPureCall | FullTag::ExprSelfCall) {
                    let target = if producer_tag == FullTag::ExprSelfCall { match owner { InstructionOwner::Function(function) => function, _ => return Err(IrVerifyError::new("generic propagation self call has no owner")) } } else {
                        let payload = store.payload(store.data[producer as usize].range())?;
                        IrFunctionId::from_raw(*payload.first().ok_or_else(|| IrVerifyError::new("generic propagation call target is missing"))?).ok_or_else(|| IrVerifyError::new("generic propagation call target is invalid"))?
                    };
                    let callable = store.functions.get(target.index()).ok_or_else(|| IrVerifyError::new("generic propagation call target is out of bounds"))?;
                    let signature = SignatureId::from_raw(callable.signature).ok_or_else(|| IrVerifyError::new("generic propagation producer lacks scoped evidence"))?;
                    store.semantic.to_type(store.semantic.signature_return_type(signature)?)?
                } else if generic.ground_native_call_at(producer)?.is_some() {
                    Self::native_call_result(store, generic, producer, owner)?
                } else if let Some(capture) = generic.try_capture_source_at(producer)? {
                    store.semantic.to_type(generic.try_capture_source(capture)?.carrier)?
                } else if let Some(carrier) = Self::context_scope_carrier(store, generic, producer, owner)? {
                    carrier
                } else if let Some(carrier) = Self::run_producer_carrier(store, generic, producer, owner)? {
                    carrier
                } else if producer_tag == FullTag::ExprRequire {
                    let payload = store.payload(store.data[producer as usize].range())?;
                    let ty = TypeId::from_raw(*payload.get(1).ok_or_else(|| IrVerifyError::new("generic validation producer lacks a checked type"))?).ok_or_else(|| IrVerifyError::new("generic validation producer type is invalid"))?;
                    Type::Result(Box::new(store.semantic.to_type(ty)?), Box::new(Type::Error))
                } else { return Err(IrVerifyError::new(format!("generic propagation producer {producer} ({producer_tag:?}) requires a prepared carrier type"))); };
                let Type::Result(ok, _) = &carrier else { return Err(IrVerifyError::new("generic propagation producer lacks a Result carrier")); };
                if ok.as_ref() != expected { return Err(IrVerifyError::new("generic propagated success type disagrees with argument type")); }
                Self::verify_generic_source(store, generic, original_producer, owner, &carrier, instance, active)?;
            }
            FullTag::ExprModuleCall | FullTag::ExprMethod | FullTag::ExprPathReadText | FullTag::ExprPathReadBytes | FullTag::ExprStrByteAt | FullTag::ExprFsList if generic.ground_native_call_at(source)?.is_some() => {
                Self::verify_native_call_operand(store, generic, source, owner, expected, active)?;
            }
            FullTag::ExprPatternIf => {
                Self::verify_pattern_conditional_result(store, generic, source, owner, expected, instance, active)?;
            }
            FullTag::ExprParam | FullTag::IntSlot | FullTag::BoolSlot => {
                if Self::verify_iteration_operand(store, generic, source, owner, expected)? || Self::verify_pattern_operand_instantiated(store, generic, source, owner, expected, instance)? || Self::verify_value_binding_operand(store, generic, source, owner, expected, active)? || Self::verify_mutable_binding_operand(store, generic, source, owner, expected, active)? {
                    active.pop();
                    return Ok(());
                }
                let slot = child(0)? as usize;
                match owner {
                    InstructionOwner::Function(function) => {
                        let callable = &store.functions[function.index()];
                        if slot < callable.params.len as usize {
                            let actual = if let Some(scope) = generic.scope_for_function(function) {
                                if let Some(instance) = instance { let instance = generic.instance(instance)?; if instance.scope != scope { return Err(IrVerifyError::new("generic source parameter instance is foreign")); } store.semantic.to_type(instance.parameter_types[slot])? }
                                else { let TypeRef::Ground(ty) = generic.scope(scope)?.parameters[slot] else { return Err(IrVerifyError::new("generic source parameter lacks a concrete frame proof")); }; store.semantic.to_type(ty)? }
                            } else { let params = callable.params.bounds(store.params.len()).ok_or_else(|| IrVerifyError::new("generic source parameter range is invalid"))?; let ty = TypeId::from_raw(store.params[params.start + slot].type_id).ok_or_else(|| IrVerifyError::new("generic source parameter has no ground type"))?; store.semantic.to_type(ty)? };
                            if actual != *expected { return Err(IrVerifyError::new("generic argument disagrees with source parameter type")); }
                        } else {
                            for instruction in range.clone() { if matches!(store.tags[instruction], FullTag::StmtAssign | FullTag::StmtAssignField | FullTag::StmtAssignFieldInt | FullTag::StmtAssignPath | FullTag::StmtAssignInt | FullTag::StmtAssignBool) && store.payload(store.data[instruction].range())?.first() == Some(&(slot as u32)) { return Err(IrVerifyError::new("generic local source is mutable and lacks a prepared assignment proof")); } }
                            let block = IrBlockId::from_raw(callable.body).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("generic local source body is invalid"))?;
                            let mut initializer = None;
                            for &instruction in store.payload(block.instructions)?.iter().skip(1) {
                                if instruction >= source || store.tags[instruction as usize] != FullTag::StmtLet { continue; }
                                let binding = store.payload(store.data[instruction as usize].range())?;
                                if binding.first() == Some(&(slot as u32)) { if initializer.replace(binding[1]).is_some() { return Err(IrVerifyError::new("generic local source has ambiguous bindings")); } }
                            }
                            let initializer = initializer.ok_or_else(|| IrVerifyError::new(format!("generic local source {source} ({tag:?}) slot {slot} in {owner:?} lacks a dominating prepared binding; original {:?}, expected {expected:?}, pattern use {}", generic.registered_instruction_origin(source, false), generic.pattern_use(source).is_some())))?;
                            Self::verify_generic_source(store, generic, initializer, owner, expected, instance, active)?;
                        }
                    }
                    InstructionOwner::Driver(step) => {
                        let slots = store.driver_steps[step as usize].slots.bounds(store.driver_slots.len()).ok_or_else(|| IrVerifyError::new("generic driver source slots are invalid"))?;
                        let binding = store.driver_slots[slots].iter().find(|binding| binding.slot as usize == slot).ok_or_else(|| IrVerifyError::new(format!("generic driver source {source} slot {slot} in {owner:?} has no lexical binding; original {:?}, expected {expected:?}, payload {words:?}", generic.registered_instruction_origin(source, false))))?;
                        if binding.flags & DRIVER_SLOT_MUTABLE != 0 { return Err(IrVerifyError::new("generic driver source is mutable and lacks assignment proof")); }
                        let program = store.driver_programs.iter().find(|program| program.steps.bounds(store.driver_steps.len()).is_some_and(|steps| steps.contains(&(step as usize)))).ok_or_else(|| IrVerifyError::new("generic driver source has no program owner"))?;
                        let mut initializer = None;
                        for index in program.steps.start as usize..step as usize {
                            let previous = store.driver_steps[index];
                            if !matches!(previous.tag, FullDriverTag::Let | FullDriverTag::Assign) { continue; }
                            let payload = store.payload(previous.data.range())?;
                            if payload.first().copied().map(|raw| Name::from_symbol(Symbol::from_raw(raw))) != Some(Name::intern(store.string(binding.name)?)) { continue; }
                            if previous.tag == FullDriverTag::Assign || initializer.is_some() { return Err(IrVerifyError::new("generic driver source has an assignment or ambiguous binding")); }
                            let decoder = FullDecoder { store, owner: driver_owner(index).map_err(|_| IrVerifyError::new("generic driver source owner is invalid"))?, instruction_range: store.driver_instruction_range(index)?, instruction_states: None, block_states: None, slot_count: previous.slot_count, pattern_tree: None, pattern_ceiling: Cell::new(usize::MAX), verified: true };
                            let mut payload = FullCursor::new(payload);
                            Name::decode(&decoder, &mut payload)?; Option::<LoweredType>::decode(&decoder, &mut payload)?; Option::<LoweredTypeCheck>::decode(&decoder, &mut payload)?;
                            if bool::decode(&decoder, &mut payload)? { return Err(IrVerifyError::new("generic driver initializer is mutable")); }
                            initializer = Some((index, payload.raw()?));
                        }
                        let (index, initializer) = initializer.ok_or_else(|| IrVerifyError::new("generic driver source lacks a prepared initializer"))?;
                        Self::verify_generic_source(store, generic, initializer, InstructionOwner::Driver(index as u32), expected, None, active)?;
                    }
                }
            }
            FullTag::ExprCall | FullTag::ExprDirectPureCall | FullTag::ExprSelfCall => {
                let Some(call) = generic.call(source) else {
                    let target = if tag == FullTag::ExprSelfCall { match owner { InstructionOwner::Function(function) => function, InstructionOwner::Driver(_) => return Err(IrVerifyError::new("generic source self call has no function owner")) } } else { IrFunctionId::from_raw(child(0)?).ok_or_else(|| IrVerifyError::new("generic source call target is invalid"))? };
                    let function = store.functions.get(target.index()).ok_or_else(|| IrVerifyError::new("generic source call target is out of bounds"))?;
                    let signature = SignatureId::from_raw(function.signature).ok_or_else(|| IrVerifyError::new("generic source call lacks scoped evidence"))?;
                    if store.semantic.to_type(store.semantic.signature_return_type(signature)?)? != *expected { return Err(IrVerifyError::new("generic argument disagrees with fixed producer result type")); }
                    active.pop();
                    return Ok(());
                };

                if let CallEvidence::Forwarded(plan) = call.evidence && instance.is_none() {
                    let plan = generic.forwarding(plan)?;
                    if owner != InstructionOwner::Function(generic.scope(plan.caller)?.owner)
                        || !generic.call_result_equals_ground(&store.semantic, plan.caller, source, expected.clone())? {
                        return Err(IrVerifyError::new("generic unused forwarding result changes its original scope or checked relationship"));
                    }
                    active.pop();
                    return Ok(());
                }
                let instance = match call.evidence { CallEvidence::Ground(id) => id, CallEvidence::Forwarded(plan) => generic.forwarded_instance(plan, instance.ok_or_else(|| IrVerifyError::new("generic argument forwarding result lacks a frame proof"))?)? };
                if store.semantic.to_type(generic.instance(instance)?.result_type)? != *expected { return Err(IrVerifyError::new("generic argument call result disagrees with prepared type")); }
            }
            FullTag::ExprField if generic.ground_projection_at(source)?.is_some() => {
                Self::verify_ground_projection_operand(store, generic, source, owner, expected, active)?;
            }
            FullTag::ExprField | FullTag::ExprBinary => {
                if tag == FullTag::ExprBinary && generic.requirement_use(source).is_none() {
                    let operation = words.first().and_then(|index| store.binary_ops.get(*index as usize)).ok_or_else(|| IrVerifyError::new("generic arithmetic source operator is invalid"))?;
                    let supported = match expected { Type::Int => matches!(operation, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem), Type::Float => matches!(operation, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div), Type::Str | Type::List(_) => *operation == BinaryOp::Add, _ => false };
                    if !supported { return Err(IrVerifyError::new(format!("generic arithmetic source {source} ({tag:?}) lacks a supported fixed contract; original {:?}, operator {operation:?}, payload {words:?}, expected {expected:?}", generic.registered_instruction_origin(source, false)))); }
                    Self::verify_generic_source(store, generic, child(1)?, owner, expected, instance, active)?;
                    Self::verify_generic_source(store, generic, child(2)?, owner, expected, instance, active)?;
                    active.pop();
                    return Ok(());
                }
                let use_ = generic.requirement_use(source).ok_or_else(|| IrVerifyError::new(format!("generic computed argument {source} ({tag:?}) lacks solved operation evidence; original {:?}, payload {words:?}, expected {expected:?}", generic.registered_instruction_origin(source, false))))?;
                let instance = generic.instance(instance.ok_or_else(|| IrVerifyError::new("generic computed argument lacks a frame proof"))?)?;
                if instance.scope != use_.scope { return Err(IrVerifyError::new("generic computed argument proof is foreign")); }
                let result = match instance.requirements[use_.requirement as usize] { RequirementWitness::Projection { result, .. } | RequirementWitness::Add { result, .. } => result, RequirementWitness::Invocation(id) => store.semantic.signature_return_type(generic.scoped_invocation_witness(id)?.signature)?, RequirementWitness::Operation(id) => generic.scoped_operation_witness(id)?.result, RequirementWitness::NativeMethod(id) => generic.scoped_native_method_witness(id)?.result, RequirementWitness::Eligibility { .. } => return Err(IrVerifyError::new("eligibility witness cannot produce an operation value")) };
                if store.semantic.to_type(result)? != *expected { return Err(IrVerifyError::new("generic computed argument disagrees with operation result type")); }
            }
            _ => return Err(IrVerifyError::new(format!("generic argument source {source} ({tag:?}) requires an independently prepared type proof; original {:?}, payload {words:?}, expected {expected:?}", generic.registered_instruction_origin(source, false)))),
        } }
        active.pop();
        Ok(())
    }

    fn stage_call_topology(store: &FullStore, generic: &GenericEvidenceStore, owners: &[Option<InstructionOwner>]) -> Result<FxHashMap<u32, (StageItemInput, u32, InstructionOwner)>, IrVerifyError> {
        let mut calls = FxHashMap::default();
        for (pipeline, tag) in store.tags.iter().enumerate() {
            if *tag != FullTag::ExprPipeline { continue; }
            let words = store.payload(store.data[pipeline].range())?;
            let input = *words.first().ok_or_else(|| IrVerifyError::new("generic stage input is missing"))?;
            let owner = owners[pipeline].ok_or_else(|| IrVerifyError::new("generic stage pipeline has no owner"))?;
            if owners.get(input as usize) != Some(&Some(owner)) { return Err(IrVerifyError::new("generic stage input has another owner")); }
            let block = words.get(1).copied().and_then(IrBlockId::from_raw).and_then(|id| store.blocks.get(id.index()))
                .ok_or_else(|| IrVerifyError::new("generic stage sequence is invalid"))?;
            let stages = store.payload(block.instructions)?;
            let count = stages.first().copied().ok_or_else(|| IrVerifyError::new("generic stage sequence length is missing"))? as usize;
            if stages.len() != count.checked_add(1).ok_or_else(|| IrVerifyError::new("generic stage sequence length overflows"))? { return Err(IrVerifyError::new("generic stage sequence length is invalid")); }
            let mut previous = Some(StageItemInput::Sequence(input));
            let mut pipeline_origin = None;
            for (position, &stage) in stages[1..].iter().enumerate() {
                let tag = store.stages.get(stage as usize);
                if !matches!(tag, Some(FullStageTag::Map | FullStageTag::Where)) { previous = None; continue; }
                let data = store.stage_data.get(stage as usize).ok_or_else(|| IrVerifyError::new("generic Map stage payload is missing"))?;
                let words = store.payload(data.range())?;
                if words.len() != 2 { return Err(IrVerifyError::new("generic Map stage payload is invalid")); }
                let slot = words[0];
                let callback = words[1];
                let ground = generic.ground_stage_call_at(callback)?;
                if generic.call(callback).is_none() && ground.is_none() { previous = None; continue; }
                if let Some(proof) = ground {
                    let source = generic.stage_call_source(proof.source)?;
                    if proof.contract.original_position as usize != position || pipeline_origin.is_some_and(|origin| origin != source.origin.pipeline) { return Err(IrVerifyError::new("ground stage call order or original pipeline identity changed")); }
                    pipeline_origin = Some(source.origin.pipeline);
                    let super::generic::PreparedOperationAuthority::Stage { stage: expected, .. } = &proof.contract.stage_authority else { return Err(IrVerifyError::new("ground stage has no selected authority")); };
                    if !matches!((expected, tag), (crate::syntax::node::StreamStageKind::Map, Some(FullStageTag::Map)) | (crate::syntax::node::StreamStageKind::Where, Some(FullStageTag::Where))) { return Err(IrVerifyError::new("ground stage opcode disagrees with its original selected authority")); }
                } else if position != 0 || tag != Some(&FullStageTag::Map) { return Err(IrVerifyError::new("generic callback requires prepared preceding stage results")); }
                if owners.get(callback as usize) != Some(&Some(owner)) { return Err(IrVerifyError::new("generic stage callback has another owner")); }
                let source = previous.ok_or_else(|| IrVerifyError::new("stage callback requires an original preceding stage transition"))?;
                if calls.insert(callback, (source, slot, owner)).is_some() { return Err(IrVerifyError::new("generic stage callback belongs to multiple pipelines")); }
                previous = ground.map(|_| StageItemInput::GroundCallback(callback));
            }
        }
        Ok(calls)
    }

    fn verify_generic_call_argument(store: &FullStore, generic: &GenericEvidenceStore, stages: &FxHashMap<u32, (StageItemInput, u32, InstructionOwner)>, call: u32, source: u32, owner: InstructionOwner, expected: &Type, instance: Option<InstantiationId>) -> Result<(), IrVerifyError> {
        if let Some(&(input, slot, stage_owner)) = stages.get(&call) {
            if owner != stage_owner || store.tags.get(source as usize) != Some(&FullTag::ExprParam) { return Err(IrVerifyError::new("generic stage argument is not its original item parameter")); }
            if store.payload(store.data[source as usize].range())? != [slot] { return Err(IrVerifyError::new("generic stage item slot disagrees with its callback argument")); }
            match input {
                StageItemInput::Sequence(input) => {
                    let sequence = if let Some(proof) = generic.ground_stage_call_at(call)? {
                        let TypeRef::Ground(ty) = proof.contract.input_sequence else { return Err(IrVerifyError::new("stage input sequence is not ground")); };
                        store.semantic.to_type(ty)?
                    } else { Type::List(Box::new(expected.clone())) };
                    Self::verify_generic_source(store, generic, input, owner, &sequence, instance, &mut Vec::new())
                }
                StageItemInput::GroundCallback(previous) => {
                    let preceding = generic.ground_stage_call_at(previous)?.ok_or_else(|| IrVerifyError::new("preceding stage has no original result proof"))?;
                    let current = generic.ground_stage_call_at(call)?.ok_or_else(|| IrVerifyError::new("stage input lacks a prepared result relationship"))?;
                    if previous >= call || generic.stage_call_source(preceding.source)?.owner != owner { return Err(IrVerifyError::new("preceding stage result has another owner or order")); }
                    let (TypeRef::Ground(result), TypeRef::Ground(input), TypeRef::Ground(item)) = (preceding.contract.result_sequence, current.contract.input_sequence, preceding.contract.result_item) else { return Err(IrVerifyError::new("preceding stage result is not ground")); };
                    if store.semantic.to_type(result)? != store.semantic.to_type(input)? || store.semantic.to_type(item)? != *expected { return Err(IrVerifyError::new("preceding original stage result disagrees with the next item source")); }
                    Ok(())
                }
            }
        } else { Self::verify_generic_source(store, generic, source, owner, expected, instance, &mut Vec::new()) }
    }

    fn verify_ground_stage_calls(store: &FullStore, generic: &GenericEvidenceStore, stages: &FxHashMap<u32, (StageItemInput, u32, InstructionOwner)>) -> Result<(), IrVerifyError> {
        for (_, root) in generic.checked_functions() {
            let function = store.functions.get(root.target.index()).ok_or_else(|| IrVerifyError::new("checked function target is invalid"))?;
            if function.signature != root.signature.raw() || store.function_metadata[root.target.index()].flags & 4 != 0 { return Err(IrVerifyError::new("checked function signature disagrees with its original declaration")); }
        }
        for (_, proof) in generic.ground_stage_calls() {
            let source = generic.stage_call_source(proof.source)?;
            let contract = &proof.contract;
            if !stages.contains_key(&source.instruction) { return Err(IrVerifyError::new("ground stage callback lacks its original pipeline item topology")); }
            let tag = store.tags[source.instruction as usize];
            if !matches!(tag, FullTag::ExprCall | FullTag::ExprDirectPureCall)
                || (tag == FullTag::ExprDirectPureCall && (contract.kind != super::generic::CallableKind::Pure || !contract.default_slots.is_empty())) { return Err(IrVerifyError::new("ground stage callback has another call opcode or default policy")); }
            let words = store.payload(store.data[source.instruction as usize].range())?;
            if words.first() != Some(&contract.target.raw()) { return Err(IrVerifyError::new("ground stage callback target disagrees with its original declaration")); }
            let function = store.functions[contract.target.index()];
            let metadata = store.function_metadata[contract.target.index()];
            if (metadata.flags & 1 != 0) != (contract.kind == super::generic::CallableKind::Proc) { return Err(IrVerifyError::new("ground stage callback kind disagrees with its original declaration")); }
            let params = function.params.bounds(store.params.len()).ok_or_else(|| IrVerifyError::new("ground stage parameter range is invalid"))?;
            if params.len() != contract.argument_types.len() { return Err(IrVerifyError::new("ground stage parameter arity is invalid")); }
            let block = words.get(1).copied().and_then(IrBlockId::from_raw).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("ground stage call arguments are invalid"))?;
            let args = store.payload(block.instructions)?;
            let count = args.first().copied().ok_or_else(|| IrVerifyError::new("ground stage call argument count is absent"))? as usize;
            if count > params.len() || args.len() != 1 + count.checked_mul(2).ok_or_else(|| IrVerifyError::new("ground stage argument count overflows"))? { return Err(IrVerifyError::new("ground stage call argument count is invalid")); }
            for (slot, &ty) in contract.argument_types.iter().enumerate() {
                let param = store.params[params.start + slot];
                let (name, formal, flags) = store.semantic.signature_param(contract.signature, slot)?;
                let signature_flags = u32::from(param.flags & 2 != 0) | u32::from(param.flags & 1 != 0) << 1;
                if Name::intern(store.string(param.name)?) != name || param.type_id != formal.raw() || signature_flags != flags { return Err(IrVerifyError::new("ground stage parameter header disagrees with its checked signature")); }
                let (tag, value) = if slot < count { (args[1 + slot * 2], args[2 + slot * 2]) } else { (2, slot as u32) };
                let expected = store.semantic.to_type(ty)?;
                match (tag, contract.argument_sources[slot]) {
                    (0, Some(instruction)) if instruction == value => Self::verify_generic_call_argument(store, generic, stages, source.instruction, instruction, source.owner, &expected, None)?,
                    (2, None) if value == slot as u32 && param.flags & 2 != 0 => Self::verify_function_default(store, generic, contract.target, slot, &expected, None)?,
                    _ => return Err(IrVerifyError::new("ground stage argument source or default slot disagrees with its original binding")),
                }
            }
        }
        Ok(())
    }

    fn verify_local_call_default_slots(store: &FullStore) -> Result<(), IrVerifyError> {
        let owners = store.generic_instruction_owners()?;
        for (instruction, &tag) in store.tags.iter().enumerate() {
            if !matches!(tag, FullTag::ExprCall | FullTag::ExprDirectPureCall | FullTag::ExprSelfCall) { continue; }
            let words = store.payload(store.data[instruction].range())?;
            let (target, args_index) = if tag == FullTag::ExprSelfCall {
                let Some(InstructionOwner::Function(target)) = owners[instruction] else { return Err(IrVerifyError::new("self call has no function owner")); };
                (target, 0)
            } else {
                (words.first().copied().and_then(IrFunctionId::from_raw).ok_or_else(|| IrVerifyError::new("call default target is invalid"))?, 1)
            };
            let function = store.functions.get(target.index()).ok_or_else(|| IrVerifyError::new("call default target is out of bounds"))?;
            let params = function.params.bounds(store.params.len()).ok_or_else(|| IrVerifyError::new("call default parameters are invalid"))?;
            let block = words.get(args_index).copied().and_then(IrBlockId::from_raw).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("call default argument block is invalid"))?;
            let args = store.payload(block.instructions)?;
            let count = args.first().copied().ok_or_else(|| IrVerifyError::new("call default argument count is absent"))? as usize;
            if args.len() != 1 + count.checked_mul(2).ok_or_else(|| IrVerifyError::new("call argument count overflows"))? { return Err(IrVerifyError::new("call default argument count is invalid")); }
            let has_splice = args[1..].chunks_exact(2).any(|argument| argument[0] == 1);
            let mut omitted = std::collections::BTreeSet::new();
            for (position, argument) in args[1..].chunks_exact(2).enumerate() {
                if argument[0] != 2 { continue; }
                let slot = argument[1] as usize;
                if slot >= params.len() || store.params[params.start + slot].flags & 2 == 0
                    || !omitted.insert(slot) || (!has_splice && slot != position) || tag == FullTag::ExprDirectPureCall {
                    return Err(IrVerifyError::new("call omission has a duplicate, misplaced, or nondefault parameter slot"));
                }
            }
        }
        Ok(())
    }

    fn verify_generic_evidence(store: &FullStore) -> Result<(), IrVerifyError> {
        store.verify_generic_owner()?;
        Self::verify_local_call_default_slots(store)?;
        let Some(generic) = store.generic.as_deref() else { return Ok(()); };
        let owners = store.generic_instruction_owners()?;
        generic.verify(&store.semantic, store.functions.len(), &owners)?;
        Self::verify_callable_values(store, generic)?;
        Self::verify_native_callable_values(store, generic)?;
        Self::verify_native_calls(store, generic)?;
        Self::verify_ground_containers(store, generic)?;
        Self::verify_native_scalar_sources(store, generic)?;
        Self::verify_record_constructors(store, generic)?;
        Self::verify_constant_sources(store, generic)?;
        Self::verify_record_updates(store, generic)?;
        Self::verify_error_constructors(store, generic)?;
        Self::verify_embedded_bridges(store, generic)?;
        Self::verify_record_sources(store, generic)?;
        Self::verify_value_bindings(store, generic)?;
        Self::verify_ground_projections(store, generic)?;
        Self::verify_source_operations(store, generic)?;
        for (_, scope) in generic.scopes() {
            let function = &store.functions[scope.owner.index()];
            if function.params.len as usize != scope.parameters.len() { return Err(IrVerifyError::new("generic scheme parameters disagree with function header")); }
            let metadata = store.function_metadata[scope.owner.index()];
            if function.signature != IR_NONE || metadata.flags & 4 == 0 || (metadata.flags & 1 != 0) != (scope.kind == super::generic::CallableKind::Proc) { return Err(IrVerifyError::new("generic scheme kind disagrees with function header")); }
            if metadata.flags & GENERIC_RETURN_PLAN_MASK != generic_return_plan_flags(scope.return_plan) { return Err(IrVerifyError::new("generic return plan disagrees with declaration header")); }
            let params = function.params.bounds(store.params.len()).ok_or_else(|| IrVerifyError::new("generic parameter range is invalid"))?;
            for (index, param) in store.params[params].iter().enumerate() {
                if Name::intern(store.string(param.name)?) != scope.parameter_names[index] || param.flags != scope.parameter_flags[index] { return Err(IrVerifyError::new("generic scheme labels or argument modes disagree with header")); }
                let expected = match scope.parameters[index] { TypeRef::Ground(ty) => ty.raw(), TypeRef::Rigid(_) | TypeRef::Template(_) => IR_NONE };
                if param.type_id != expected { return Err(IrVerifyError::new("generic parameter storage disagrees with scoped type")); }
            }
        }
        for call in generic.calls() {
            let instruction = call.instruction as usize;
            let words = store.payload(store.data[instruction].range())?;
            let target = match store.tags[instruction] {
                FullTag::ExprCall | FullTag::ExprDirectPureCall => words.first().copied(),
                FullTag::ExprSelfCall => match call.caller { InstructionOwner::Function(owner) => Some(owner.raw()), InstructionOwner::Driver(_) => None },
                _ => return Err(IrVerifyError::new("generic call evidence is attached to a non-call instruction")),
            };
            if target != Some(generic.scope(call.target)?.owner.raw()) {
                return Err(IrVerifyError::new("generic call evidence disagrees with encoded target"));
            }
        }
        let stage_calls = Self::stage_call_topology(store, generic, &owners)?;
        Self::verify_ground_stage_calls(store, generic, &stage_calls)?;
        Self::verify_stage_pipelines(store, generic)?;
        for call in generic.calls() {
            let words = store.payload(store.data[call.instruction as usize].range())?;
            let args_index = if store.tags[call.instruction as usize] == FullTag::ExprSelfCall { 0 } else { 1 };
            let args_block = words.get(args_index).copied().and_then(IrBlockId::from_raw).and_then(|id| store.blocks.get(id.index())).ok_or_else(|| IrVerifyError::new("generic call arguments block is invalid"))?;
            let args = store.payload(args_block.instructions)?;
            let count = args.first().copied().ok_or_else(|| IrVerifyError::new("generic call argument count is missing"))? as usize;
            if args.len() != 1 + count.checked_mul(2).ok_or_else(|| IrVerifyError::new("generic argument count overflow"))? || count > generic.call_arguments(call.instruction).len() { return Err(IrVerifyError::new("generic call argument evidence disagrees with encoded arity")); }
            for (index, argument) in generic.call_arguments(call.instruction).iter().enumerate() {
                let (tag, source) = if index < count { (args[1 + index * 2], args[2 + index * 2]) } else { (2, argument.parameter) };
                match (tag, argument.source_instruction) {
                    (0, Some(instruction)) if instruction == source => {},
                    (2, None) if source == argument.parameter && generic.scope(call.target)?.parameter_flags[index] & 2 != 0 => {},
                    _ => return Err(IrVerifyError::new("generic argument evidence disagrees with encoded source")),
                }
                let Some(source) = argument.source_instruction else {
                    match call.evidence {
                        CallEvidence::Ground(instance) => Self::verify_generic_default(store, generic, instance, argument.parameter as usize)?,
                        CallEvidence::Forwarded(plan) => for &(_, instance) in &generic.forwarding(plan)?.instances { Self::verify_generic_default(store, generic, instance, argument.parameter as usize)?; },
                    }
                    continue;
                };
                match call.evidence {
                    CallEvidence::Ground(_) => {
                        let TypeRef::Ground(ty) = argument.ty else { return Err(IrVerifyError::new("ground generic argument is not grounded")); };
                        if store.semantic.callable_descriptor(ty)?.is_some() {
                            Self::verify_scoped_callable_source(store, generic, source, call.caller, ty, None, 0)?;
                            continue;
                        }
                        let expected = store.semantic.to_type(ty)?;
                        if let InstructionOwner::Function(owner) = call.caller && let Some(scope) = generic.scope_for_function(owner) {
                            let instances = generic.instances().filter(|(_, instance)| instance.scope == scope).collect::<Vec<_>>();
                            if instances.is_empty() { Self::verify_generic_call_argument(store, generic, &stage_calls, call.instruction, source, call.caller, &expected, None)?; }
                            for (instance, _) in instances { Self::verify_generic_call_argument(store, generic, &stage_calls, call.instruction, source, call.caller, &expected, Some(instance))?; }
                        } else { Self::verify_generic_call_argument(store, generic, &stage_calls, call.instruction, source, call.caller, &expected, None)?; }
                    }
                    CallEvidence::Forwarded(plan) => for &(from, to) in &generic.forwarding(plan)?.instances {
                        let ty = generic.instance(to)?.parameter_types[argument.parameter as usize];
                        if store.semantic.callable_descriptor(ty)?.is_some() { Self::verify_scoped_callable_source(store, generic, source, call.caller, ty, Some(from), 0)?; continue; }
                        let expected = generic.expand(&store.semantic, argument.ty, &generic.instance(from)?.substitutions, &mut FxHashMap::default())?;
                        Self::verify_generic_call_argument(store, generic, &stage_calls, call.instruction, source, call.caller, &expected, Some(from))?;
                    },
                }
            }
        }
        for use_ in generic.requirement_uses() {
            let instruction = use_.instruction as usize;
            let words = store.payload(store.data[instruction].range())?;
            let requirement = &generic.scope(use_.scope)?.requirements[use_.requirement as usize];
            match (store.tags[instruction], requirement) {
                (FullTag::ExprMethod, Requirement::NativeMethod(_)) => Self::verify_scoped_native_method_instruction(store, generic, use_.instruction)?,
                (FullTag::ExprOk | FullTag::ExprErr, Requirement::Operation(_)) => Self::verify_scoped_operation_instruction(store, generic, use_.instruction)?,
                (FullTag::ExprDynamicCall, Requirement::Invocation { .. }) => Self::verify_scoped_invocation_instruction(store, generic, use_.instruction)?,
                (FullTag::ExprField, Requirement::Projection { receiver_parameter, field, .. }) => {
                    let name = words.get(1).copied().ok_or_else(|| IrVerifyError::new("generic projection field is missing"))?;
                    if Name::intern(store.string(name)?) != *field { return Err(IrVerifyError::new("generic projection evidence disagrees with encoded field")); }
                    let base = words.first().copied().ok_or_else(|| IrVerifyError::new("generic projection receiver is missing"))? as usize;
                    if store.tags.get(base) != Some(&FullTag::ExprParam) { return Err(IrVerifyError::new("generic projection receiver is not its prepared parameter")); }
                    let base_words = store.payload(store.data[base].range())?;
                    if base_words.first() != Some(receiver_parameter) { return Err(IrVerifyError::new("generic projection receiver parameter disagrees with evidence")); }
                }
                (FullTag::ExprBinary, Requirement::Add { .. }) => {
                    let op = words.first().and_then(|index| store.binary_ops.get(*index as usize));
                    if op != Some(&BinaryOp::Add) { return Err(IrVerifyError::new("generic Add evidence is attached to another binary operator")); }
                    let Requirement::Add { left, right, .. } = requirement else { unreachable!() };
                    let left_source = *words.get(1).ok_or_else(|| IrVerifyError::new("generic Add left operand is missing"))?;
                    let right_source = *words.get(2).ok_or_else(|| IrVerifyError::new("generic Add right operand is missing"))?;
                    Self::verify_generic_symbolic_source(store, generic, left_source, use_.scope, *left, &mut Vec::new())?;
                    Self::verify_generic_symbolic_source(store, generic, right_source, use_.scope, *right, &mut Vec::new())?;
                    for (instance_id, instance) in generic.instances().filter(|(_, instance)| instance.scope == use_.scope) {
                        let RequirementWitness::Add { left, right, .. } = instance.requirements[use_.requirement as usize] else { return Err(IrVerifyError::new("generic Add witness has another kind")); };
                        Self::verify_generic_source(store, generic, left_source, InstructionOwner::Function(generic.scope(use_.scope)?.owner), &store.semantic.to_type(left)?, Some(instance_id), &mut Vec::new())?;
                        Self::verify_generic_source(store, generic, right_source, InstructionOwner::Function(generic.scope(use_.scope)?.owner), &store.semantic.to_type(right)?, Some(instance_id), &mut Vec::new())?;
                    }
                }
                _ => return Err(IrVerifyError::new("generic requirement evidence is attached to the wrong instruction kind")),
            }
        }
        for record in generic.constructors() {
            let instruction = record.instruction as usize;
            if store.tags[instruction] != FullTag::ExprRecord { return Err(IrVerifyError::new("generic physical layout is attached to a non-record constructor")); }
            let words = store.payload(store.data[instruction].range())?;
            let block = words.first().and_then(|raw| IrBlockId::from_raw(*raw)).and_then(|id| store.blocks.get(id.index()))
                .ok_or_else(|| IrVerifyError::new("generic record constructor entries are missing"))?;
            let entries = store.payload(block.instructions)?;
            let count = entries.first().copied().ok_or_else(|| IrVerifyError::new("generic record constructor count is missing"))? as usize;
            let fields = entries.get(1..).ok_or_else(|| IrVerifyError::new("generic record constructor entries are missing"))?;
            if fields.len() != count.checked_mul(3).ok_or_else(|| IrVerifyError::new("generic record constructor count overflows"))? { return Err(IrVerifyError::new("generic physical layout requires fixed literal record fields")); }
            let mut names = Vec::with_capacity(count);
            for entry in fields.chunks_exact(3) {
                if entry[0] != 0 { return Err(IrVerifyError::new("generic physical layout requires fixed literal record fields")); }
                names.push(Name::from_symbol(Symbol::from_raw(entry[1])));
            }
            names.sort_unstable();
            let layout = generic.layout(record.layout)?;
            if names.len() != layout.fields.len() || names.iter().copied().ne(layout.fields.iter().map(|field| field.0)) { return Err(IrVerifyError::new("generic physical layout disagrees with constructor field order")); }
        }
        Ok(())
    }
    /// Private representation operations are reachable only from the embedded
    /// implementation module that declares them.
    ///
    /// Lowering already rewrites a bridge call only inside its own module, but
    /// an executable could still carry the operation from anywhere. The owner
    /// recorded on the enclosing function is the authority: it comes from the
    /// checked implementation identity, not from a callsite span or a display
    /// label. Instructions outside every function are driver statements, where
    /// no bridge call can be legitimate.
    fn verify_bridge_ownership(store: &FullStore) -> Result<(), IrVerifyError> {
        let mut covered = 0usize;
        for (index, _function) in store.functions.iter().enumerate() {
            let instructions = store.function_instruction_range(index)?;
            covered = covered.max(instructions.end);
            let owner = store.function_metadata[index].owner;
            let module = if owner == IR_NONE {
                None
            } else {
                crate::stdlib::find_by_namespace(store.string(owner)?)
            };
            Self::verify_bridge_instructions(store, instructions.start, instructions.end, module)?;
        }
        Self::verify_bridge_instructions(store, covered, store.tags.len(), None)
    }

    /// Reject a private representation operation carried by an instruction the
    /// owner is not permitted to reach.
    fn verify_bridge_instructions(
        store: &FullStore,
        start: usize,
        end: usize,
        owner: Option<&'static crate::stdlib::StdlibModule>,
    ) -> Result<(), IrVerifyError> {
        for instruction in start..end {
            if store.tags[instruction] != FullTag::ExprModuleCall {
                continue;
            }
            let payload = store.payload(store.data[instruction].range())?;
            let Some(pool_index) = payload.first().copied() else {
                continue;
            };
            let Some(op) = store.runtime_ops.get(pool_index as usize) else {
                continue;
            };
            if !crate::stdlib::is_private_bridge_op(*op) {
                continue;
            }
            let permitted =
                owner.is_some_and(|module| crate::stdlib::declares_bridge_op(module, *op));
            if !permitted {
                return Err(IrVerifyError::new(
                    "private representation operation reached from outside its implementation module",
                ));
            }
            let generic = store.generic.as_deref().ok_or_else(|| IrVerifyError::new("private native bridge lacks prepared original authority"))?;
            let source = generic.bridge_call(instruction as u32)?.ok_or_else(|| IrVerifyError::new("private native bridge lacks its original invocation"))?;
            if source.original.declaration().op() != *op { return Err(IrVerifyError::new("private native bridge changes its original selected operation")); }

        }
        Ok(())
    }

    fn verify(program: &FullProgram) -> Result<(), IrVerifyError> {
        let _symbols = program.symbol_owner().enter();
        let store = &program.store;
        let pattern_tree = store.generic.as_deref().is_some_and(|generic| generic.has_pattern_applications() || generic.has_original_patterns() || generic.has_local_callable_bindings() || generic.original_callable_receivers().next().is_some() || generic.has_original_argument_bindings() || generic.has_iteration_bindings() || generic.has_value_bindings() || generic.has_comprehensions() || generic.has_context_producers() || generic.has_try_captures() || generic.has_native_scalars() || generic.has_mutable_bindings() || generic.has_saved_native_receivers() || generic.has_mutable_paths() || generic.has_host_bindings() || generic.has_lexical_captures() || generic.has_conditionals() || generic.has_formatted_paths() || generic.has_optional_receiver_guards() || generic.has_native_record_arguments() || generic.operations().any(|(_, operation)| operation.literal_comparison_slot.is_some())).then(|| std::sync::Mutex::new(super::pattern::PatternTreeBuilder::new(store.tags.len())));
        let mut wire_types = rustc_hash::FxHashSet::default();
        for mapping in &store.wire_enums {
            if mapping.variants.is_empty() || mapping.variants.values().collect::<std::collections::BTreeSet<_>>().len() != mapping.variants.len() {
                return Err(IrVerifyError::new("wire enum mapping is empty or has duplicate strings"));
            }
            if !wire_types.insert(mapping.type_name) {
                return Err(IrVerifyError::new("wire enum declaring identity has multiple mappings"));
            }
        }
        if store.tags.len() != store.data.len()
            || store.patterns.len() != store.pattern_data.len()
            || store.stages.len() != store.stage_data.len()
            || store.values.len() != store.value_data.len()
            || store.locations.len() != store.location_sources.len()
            || store.functions.len() != store.function_instruction_starts.len()
            || store.functions.len() != store.function_metadata.len()
            || store.functions.len() != program.function_definition_spans.len()
        {
            return Err(IrVerifyError::new("full IR tag/data columns differ"));
        }
        store.semantic.verify()?;
        program
            .sources
            .get(store.source_id)
            .ok_or_else(|| IrVerifyError::new("full IR source is missing"))?;
        for span in &program.function_definition_spans {
            let source = program
                .sources
                .get(span.source_id)
                .ok_or_else(|| IrVerifyError::new("function definition source is missing"))?;
            if span.end() > source.len() {
                return Err(IrVerifyError::new(
                    "function definition span is out of bounds",
                ));
            }
        }
        for (location, source_id) in store.locations.iter().zip(&store.location_sources) {
            let source = program
                .sources
                .get(*source_id)
                .ok_or_else(|| IrVerifyError::new("full IR location source is missing"))?;
            let end = (location.start as usize)
                .checked_add(location.len as usize)
                .ok_or_else(|| IrVerifyError::new("full IR location overflows"))?;
            if end > source.len() {
                return Err(IrVerifyError::new("full IR location is out of bounds"));
            }
        }
        for index in 0..store.strings.len() {
            let raw =
                u32::try_from(index + 1).map_err(|_| IrVerifyError::new("string id overflows"))?;
            store.string(raw)?;
        }
        for index in 0..store.bytes.len() {
            let raw =
                u32::try_from(index + 1).map_err(|_| IrVerifyError::new("bytes id overflows"))?;
            store.bytes(raw)?;
        }
        for validation in &store.validations {
            store.semantic.type_tag(validation.type_id)?;
            store.string(validation.name)?;
        }
        for block in &store.blocks {
            store.payload(block.instructions)?;
            if block.owner != IR_NONE {
                let function_owner = IrFunctionId::from_raw(block.owner)
                    .is_some_and(|id| id.index() < store.functions.len());
                let driver_owner = driver_owner_index(block.owner)
                    .is_some_and(|index| index < store.driver_steps.len());
                if !function_owner && !driver_owner {
                    return Err(IrVerifyError::new("block owner is out of bounds"));
                }
            }
            if block.flags & !(BLOCK_STATEMENTS | BLOCK_FUNCTION_BODY) != 0 {
                return Err(IrVerifyError::new("block flags are invalid"));
            }
            if block.result != IR_NONE {
                return Err(IrVerifyError::new("block result is out of bounds"));
            }
        }
        let mut previous_end = 0usize;
        let mut previous_cold_param = None;
        for cold in &store.param_cold {
            if cold.param as usize >= store.params.len()
                || previous_cold_param.is_some_and(|previous| previous >= cold.param)
            {
                return Err(IrVerifyError::new(
                    "cold parameter rows are not sorted unique in bounds",
                ));
            }
            if cold.default != IR_NONE && cold.default as usize >= store.values.len() {
                return Err(IrVerifyError::new("parameter default is out of bounds"));
            }
            if cold.validation != IR_NONE && cold.validation as usize >= store.validations.len() {
                return Err(IrVerifyError::new("parameter validation is out of bounds"));
            }
            previous_cold_param = Some(cold.param);
        }
        for (index, function) in store.functions.iter().enumerate() {
            store.string(function.name)?;
            let metadata = store.function_metadata[index];
            if metadata.owner != IR_NONE {
                store.string(metadata.owner)?;
            }
            if metadata.flags & !0b1_1111 != 0 {
                return Err(IrVerifyError::new("function metadata flags are invalid"));
            }
            let instructions = store.function_instruction_range(index)?;
            if instructions.start != previous_end {
                return Err(IrVerifyError::new(
                    "function instruction ranges are not dense and source ordered",
                ));
            }
            previous_end = instructions.end;
            let params = function
                .params
                .bounds(store.params.len())
                .ok_or_else(|| IrVerifyError::new("function parameter range is invalid"))?;
            let captures = function
                .captures
                .bounds(store.captures.len())
                .ok_or_else(|| IrVerifyError::new("function capture range is invalid"))?;
            let generic_scope = store.generic.as_deref().and_then(|evidence| evidence.scope_for_function(IrFunctionId::new(index).ok()?));
            if (metadata.flags & 4 != 0) != generic_scope.is_some() || (function.signature == IR_NONE) != generic_scope.is_some() { return Err(IrVerifyError::new("generic function header and scoped metadata disagree")); }
            if generic_scope.is_none() && metadata.flags & GENERIC_RETURN_PLAN_MASK != 0 { return Err(IrVerifyError::new("nongeneric function carries generic return-plan flags")); }
            if function.signature != IR_NONE && store.semantic.signature_param_count(SignatureId::from_raw(function.signature).ok_or_else(|| IrVerifyError::new("function signature id is invalid"))?)? != params.len() {
                return Err(IrVerifyError::new(
                    "function parameters do not match its signature",
                ));
            }
            for param in &store.params[params.clone()] {
                store.string(param.name)?;
                if param.type_id == IR_NONE { if generic_scope.is_none() { return Err(IrVerifyError::new("generic parameter lacks scoped metadata")); } } else { store.semantic.type_tag(TypeId::from_raw(param.type_id).ok_or_else(|| IrVerifyError::new("parameter type id is invalid"))?)?; }
                if param.flags & !0b111 != 0 {
                    return Err(IrVerifyError::new("parameter flags are invalid"));
                }
            }
            for capture in &store.captures[captures] {
                store.string(capture.name)?;
                store.semantic.type_tag(capture.type_id)?;
                if capture.slot_and_flags & !(1 << 31) >= function.slot_count {
                    return Err(IrVerifyError::new("capture slot is out of bounds"));
                }
            }
            let instruction_len = instructions.len();
            let decoder = FullDecoder {
                store,
                owner: IrFunctionId::new(index)
                    .map_err(|_| IrVerifyError::new("function id is invalid"))?
                    .raw(),
                instruction_range: instructions,
                instruction_states: Some(RefCell::new(vec![0; instruction_len])),
                block_states: Some(RefCell::new(vec![0; store.blocks.len()])),
                slot_count: function.slot_count,
                pattern_tree: pattern_tree.as_ref(), pattern_ceiling: Cell::new(usize::MAX),
                verified: false,
            };
            let body_id = IrBlockId::from_raw(function.body)
                .ok_or_else(|| IrVerifyError::new("function body block is invalid"))?;
            let body_block = store
                .blocks
                .get(body_id.index())
                .ok_or_else(|| IrVerifyError::new("function body block is out of bounds"))?;
            if body_block.flags != (BLOCK_STATEMENTS | BLOCK_FUNCTION_BODY) {
                return Err(IrVerifyError::new("function body block flags are invalid"));
            }
            let body = [function.body];
            let mut cursor = FullCursor::new(&body);
            Vec::<BuildStmtId>::verify(&decoder, &mut cursor)?;
            cursor.finish()?;
            for (param_index, _) in store.params[params.clone()].iter().enumerate() {
                let param_index = params.start + param_index;
                let Some(cold) = store
                    .param_cold
                    .binary_search_by_key(&(param_index as u32), |cold| cold.param)
                    .ok()
                    .map(|cold| store.param_cold[cold])
                else {
                    continue;
                };
                if cold.default != IR_NONE {
                    let default = [cold.default];
                    let mut cursor = FullCursor::new(&default);
                    LoweredValue::verify(&decoder, &mut cursor)?;
                    cursor.finish()?;
                }
            }
            decoder.finish_function()?;
            let body_payload = store.payload(body_block.instructions)?;
            let mut default_slots = BTreeSet::new();
            let mut entry_open = true;
            let mut previous_default = None;
            for &instruction in body_payload.iter().skip(1) {
                let tag = store.tags[instruction as usize];
                if tag != FullTag::StmtDefaultParameter { entry_open = false; continue; }
                if !entry_open { return Err(IrVerifyError::new("parameter defaults must precede the callable body")); }
                let words = store.payload(store.data[instruction as usize].range())?;
                let slot = words[0] as usize;
                if slot >= params.len() || store.params[params.start + slot].flags & 4 == 0
                    || previous_default.is_some_and(|previous| slot <= previous) {
                    return Err(IrVerifyError::new("parameter default entry does not match its parameter"));
                }
                previous_default = Some(slot);
                default_slots.insert(slot);
            }
            let entry_count = store.tags[decoder.instruction_range.clone()].iter()
                .filter(|tag| **tag == FullTag::StmtDefaultParameter).count();
            if entry_count != default_slots.len() { return Err(IrVerifyError::new("parameter default entry must belong to its callable prefix")); }
            for (slot, param) in store.params[params.clone()].iter().enumerate() {
                if param.flags & 4 != 0 {
                    if param.flags & 2 == 0 || param.flags & 1 != 0 || !default_slots.contains(&slot) {
                        return Err(IrVerifyError::new("expression default metadata requires a non-rest defaulted parameter entry"));
                    }
                    if store.param_cold.binary_search_by_key(&((params.start + slot) as u32), |cold| cold.param).ok()
                        .is_some_and(|index| store.param_cold[index].default != IR_NONE) {
                        return Err(IrVerifyError::new("expression defaults cannot also carry a prepared literal"));
                    }
                }
            }
            if body_payload.first().copied() == Some(0) {
                return Err(IrVerifyError::new(format!(
                    "function {index} has an empty body"
                )));
            }
            let is_stream = if function.signature == IR_NONE { false } else { matches!(store.semantic.to_type(store.semantic.signature_return_type(SignatureId::from_raw(function.signature).ok_or_else(|| IrVerifyError::new("function signature id is invalid"))?)?)?, Type::Stream(_)) };
            if !is_stream && !indexed_block_can_return(store, body_id)?
            {
                return Err(IrVerifyError::new(format!(
                    "function {index} body does not terminate with a return"
                )));
            }
        }
        Self::verify_bridge_ownership(store)?;
        if store.driver_root == IR_NONE {
            if !store.driver_steps.is_empty()
                || !store.driver_slots.is_empty()
                || !store.driver_sync.is_empty()
                || !store.driver_regions.is_empty()
                || !store.driver_programs.is_empty()
            {
                return Err(IrVerifyError::new(
                    "driver tables exist without a root program",
                ));
            }
        } else {
            if store.driver_root as usize > store.driver_programs.len() {
                return Err(IrVerifyError::new("driver root is out of bounds"));
            }
            let mut covered_slots = vec![false; store.driver_slots.len()];
            for (index, step) in store.driver_steps.iter().enumerate() {
                if step.reserved != [0; 3] || step.effects & !EFFECT_ALL != 0 {
                    return Err(IrVerifyError::new("driver step metadata is invalid"));
                }
                let instructions = store.driver_instruction_range(index)?;
                if instructions.start != previous_end
                    || step.instruction_start as usize != instructions.start
                {
                    return Err(IrVerifyError::new(
                        "driver instruction ranges are not dense and source ordered",
                    ));
                }
                previous_end = instructions.end;
                store.payload(step.data.range())?;
                let location = IrLocationId::from_raw(step.location)
                    .ok_or_else(|| IrVerifyError::new("driver location is invalid"))?;
                if location.index() >= store.locations.len() {
                    return Err(IrVerifyError::new("driver location is out of bounds"));
                }
                let slots = step
                    .slots
                    .bounds(store.driver_slots.len())
                    .ok_or_else(|| IrVerifyError::new("driver slot range is invalid"))?;
                let mut expected_effects =
                    driver_tag_effects(step.tag) | instruction_effects(&store.tags[instructions]);
                let mut names = BTreeSet::new();
                let mut indices = BTreeSet::new();
                if !slots.is_empty() {
                    expected_effects |= EFFECT_BINDING_READ;
                }
                for slot_index in slots.clone() {
                    if covered_slots[slot_index] {
                        return Err(IrVerifyError::new("driver slot is owned by multiple steps"));
                    }
                    covered_slots[slot_index] = true;
                    let slot = store.driver_slots[slot_index];
                    store.string(slot.name)?;
                    store.semantic.type_tag(slot.type_id)?;
                    if slot.reserved != [0; 3]
                        || slot.flags
                            & !(DRIVER_SLOT_READ | DRIVER_SLOT_WRITE | DRIVER_SLOT_MUTABLE)
                            != 0
                        || slot.flags & DRIVER_SLOT_READ == 0
                        || slot.slot >= step.slot_count
                    {
                        return Err(IrVerifyError::new("driver slot is invalid"));
                    }
                    if !names.insert(slot.name) || !indices.insert(slot.slot) {
                        return Err(IrVerifyError::new("driver step slots are not unique"));
                    }
                    if slot.flags & DRIVER_SLOT_WRITE != 0 {
                        expected_effects |= EFFECT_BINDING_WRITE;
                    }
                }
                if driver_tag_writes_binding(step.tag) {
                    expected_effects |= EFFECT_BINDING_WRITE;
                }
                if step.effects != expected_effects {
                    return Err(IrVerifyError::new("driver effects are not exact"));
                }
            }
            if covered_slots.iter().any(|covered| !covered) {
                return Err(IrVerifyError::new(
                    "driver plan contains an unreachable slot row",
                ));
            }
            let mut covered_regions = vec![false; store.driver_regions.len()];
            let mut covered_sync = vec![false; store.driver_sync.len()];
            for driver in &store.driver_programs {
                let steps = driver
                    .steps
                    .bounds(store.driver_steps.len())
                    .ok_or_else(|| IrVerifyError::new("driver step range is invalid"))?;
                let regions = driver
                    .regions
                    .bounds(store.driver_regions.len())
                    .ok_or_else(|| IrVerifyError::new("driver region range is invalid"))?;
                let expected_program_effects = store.driver_steps[steps.clone()]
                    .iter()
                    .fold(0, |effects, step| effects | step.effects);
                if driver.effects != expected_program_effects {
                    return Err(IrVerifyError::new("driver program effects are not exact"));
                }
                let mut region_step = steps.start;
                for region_index in regions {
                    if covered_regions[region_index] {
                        return Err(IrVerifyError::new(
                            "driver region is owned by multiple programs",
                        ));
                    }
                    covered_regions[region_index] = true;
                    let region = store.driver_regions[region_index];
                    let region_steps = region
                        .steps
                        .bounds(store.driver_steps.len())
                        .ok_or_else(|| IrVerifyError::new("driver region steps are invalid"))?;
                    if region_steps.start != region_step
                        || region_steps.end > steps.end
                        || region_steps.is_empty()
                    {
                        return Err(IrVerifyError::new(
                            "driver regions do not partition their program",
                        ));
                    }
                    region_step = region_steps.end;
                    let expected_region_effects = store.driver_steps[region_steps.clone()]
                        .iter()
                        .fold(0, |effects, step| effects | step.effects);
                    if region.effects != expected_region_effects {
                        return Err(IrVerifyError::new("driver region effects are not exact"));
                    }
                    if region_steps.len() > 1
                        && store.driver_steps[region_steps.clone()]
                            .iter()
                            .any(|step| step.effects & EFFECT_BOUNDARY_MASK != 0)
                    {
                        return Err(IrVerifyError::new(
                            "effect boundary is not isolated in its driver region",
                        ));
                    }
                    let sync = region
                        .sync
                        .bounds(store.driver_sync.len())
                        .ok_or_else(|| IrVerifyError::new("driver sync range is invalid"))?;
                    for sync_index in sync.clone() {
                        if covered_sync[sync_index] {
                            return Err(IrVerifyError::new(
                                "driver sync row is owned by multiple regions",
                            ));
                        }
                        covered_sync[sync_index] = true;
                    }
                    let mut expected_sync = BTreeMap::<u32, (TypeId, u8)>::new();
                    for step in &store.driver_steps[region_steps] {
                        let slots = step
                            .slots
                            .bounds(store.driver_slots.len())
                            .ok_or_else(|| IrVerifyError::new("driver slot range is invalid"))?;
                        for slot in &store.driver_slots[slots] {
                            let flags = slot.flags & (DRIVER_SLOT_READ | DRIVER_SLOT_WRITE);
                            if let Some((type_id, existing)) = expected_sync.get_mut(&slot.name) {
                                if *type_id != slot.type_id {
                                    return Err(IrVerifyError::new(
                                        "driver sync type identities conflict",
                                    ));
                                }
                                *existing |= flags;
                            } else {
                                expected_sync.insert(slot.name, (slot.type_id, flags));
                            }
                        }
                    }
                    if sync.len() != expected_sync.len() {
                        return Err(IrVerifyError::new("driver sync rows are incomplete"));
                    }
                    for (row, (name, (type_id, flags))) in
                        store.driver_sync[sync].iter().zip(expected_sync)
                    {
                        if row.reserved != [0; 3]
                            || row.name != name
                            || row.type_id != type_id
                            || row.flags != flags
                        {
                            return Err(IrVerifyError::new("driver sync rows are not exact"));
                        }
                    }
                }
                if region_step != steps.end {
                    return Err(IrVerifyError::new(
                        "driver regions do not cover their program",
                    ));
                }
            }
            if covered_regions.iter().any(|covered| !covered) {
                return Err(IrVerifyError::new(
                    "driver plan contains an unreachable region",
                ));
            }
            if covered_sync.iter().any(|covered| !covered) {
                return Err(IrVerifyError::new(
                    "driver plan contains an unreachable sync row",
                ));
            }
            program.verify_driver(pattern_tree.as_ref())?;
        }
        if previous_end != store.tags.len() {
            return Err(IrVerifyError::new(
                "full IR contains instructions outside executable owner ranges",
            ));
        }
        if let Some(tree) = pattern_tree {
            let tree = tree.into_inner().map_err(|_| IrVerifyError::new("pattern verifier traversal was interrupted"))?.finish()?;
            Self::verify_pattern_evidence(store, &tree)?;
            Self::verify_local_callable_dominance(store, &tree)?;
            Self::verify_value_binding_dominance(store, &tree)?;
            Self::verify_native_scalar_dominance(store, &tree)?;
            Self::verify_literal_comparison_scopes(store, &tree)?;
            Self::verify_native_receiver_scopes(store, &tree)?;
            Self::verify_optional_receiver_guard_scopes(store, &tree)?;
            Self::verify_host_bindings(store, &tree)?;
            Self::verify_lexical_captures(store, &tree)?;
            Self::verify_mutable_binding_dominance(store, &tree)?;
            Self::verify_original_argument_bindings(store, &tree)?;
            Self::verify_original_iteration_bindings(store, &tree)?;
            Self::verify_original_comprehensions(store, &tree)?;
            Self::verify_original_context_scopes(store, &tree)?;
            Self::verify_conditionals(store, &tree)?;
            if let Some(generic) = store.generic.as_deref() { Self::verify_formatted_paths(store, generic, &tree)?; }
        }
        Self::verify_compiler_argument_wrappers(store)?;
        if let Some(generic) = store.generic.as_deref() { Self::verify_try_capture_sources(store, generic)?; Self::verify_ground_containers(store, generic)?; }
        Self::verify_generic_evidence(store)?;
        Ok(())
    }
}

macro_rules! impl_word_codec {
    ($ty:ty, $encode:expr, $decode:expr) => {
        impl FullCodec for $ty {
            fn encode(
                &self,
                _builder: &mut FullBuilder,
                output: &mut Vec<u32>,
            ) -> Result<(), IrBuildError> {
                output.push(($encode)(self)?);
                Ok(())
            }

            fn decode(
                _decoder: &FullDecoder<'_>,
                input: &mut FullCursor<'_>,
            ) -> Result<Self, IrVerifyError> {
                ($decode)(input.raw())
            }
        }
    };
}

impl_word_codec!(u32, |value: &u32| Ok(*value), |raw| raw);
impl_word_codec!(crate::syntax::arena::ContextScopeKind,
    |value: &crate::syntax::arena::ContextScopeKind| Ok(match value { crate::syntax::arena::ContextScopeKind::Cwd => 0, crate::syntax::arena::ContextScopeKind::Env => 1 }),
    |raw: Result<u32, IrVerifyError>| raw.and_then(|raw| match raw {
        0 => Ok(crate::syntax::arena::ContextScopeKind::Cwd),
        1 => Ok(crate::syntax::arena::ContextScopeKind::Env),
        _ => Err(IrVerifyError::new("context scope kind is invalid")),
    }));
impl FullCodec for usize {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        if builder.current_owner.is_none() {
            return Err(IrBuildError::format("slot_without_owner", None, 0, 0));
        }
        if *self >= builder.current_slot_count as usize {
            return Err(IrBuildError::format("slot_out_of_bounds", None, 0, 0));
        }
        builder.encoded_slot_uses.push((builder.current_owner.unwrap(), *self));
        output.push(
            u32::try_from(*self).map_err(|_| IrBuildError::format("slot_overflow", None, 0, 0))?,
        );
        Ok(())
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        let slot = input.raw()? as usize;
        if slot >= decoder.slot_count as usize {
            return Err(IrVerifyError::new("slot is out of bounds"));
        }
        Ok(slot)
    }
}
impl_word_codec!(bool, |value: &bool| Ok(u32::from(*value)), |raw: Result<
    u32,
    IrVerifyError,
>| raw.and_then(
    |raw| match raw {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(IrVerifyError::new("boolean payload is invalid")),
    }
));

impl FullCodec for i64 {
    fn encode(
        &self,
        _builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
        let bits = *self as u64;
        output.push(bits as u32);
        output.push((bits >> 32) as u32);
        Ok(())
    }

    fn decode(
        _decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        let low = input.raw()? as u64;
        let high = input.raw()? as u64;
        Ok((low | high << 32) as i64)
    }
}

impl FullCodec for u64 {
    fn encode(
        &self,
        _builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
        output.push(*self as u32);
        output.push((*self >> 32) as u32);
        Ok(())
    }

    fn decode(
        _decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(input.raw()? as u64 | (input.raw()? as u64) << 32)
    }
}

impl FullCodec for FloatValue {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.0.to_bits().encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Self(f64::from_bits(u64::decode(decoder, input)?)))
    }
}

impl FullCodec for DurationValue {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.millis.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Self {
            millis: u64::decode(decoder, input)?,
        })
    }
}

impl FullCodec for Name {
    fn encode(
        &self,
        _builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
        output.push(self.symbol().raw());
        Ok(())
    }

    fn decode(
        _decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Name::from_symbol(Symbol::from_raw(input.raw()?)))
    }
}

impl FullCodec for QualifiedName {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.namespace.encode(builder, output)?;
        self.member.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Self::new(
            Name::decode(decoder, input)?,
            Name::decode(decoder, input)?,
        ))
    }
}

impl FullCodec for LoweredFunctionKey {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        let id = builder
            .function_ids
            .get(self)
            .copied()
            .ok_or_else(|| IrBuildError::format("unresolved_function_identity", None, 0, 0))?;
        output.push(id.raw());
        Ok(())
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        let id = IrFunctionId::from_raw(input.raw()?)
            .ok_or_else(|| IrVerifyError::new("function identity is invalid"))?;
        let function = decoder
            .store
            .functions
            .get(id.index())
            .ok_or_else(|| IrVerifyError::new("function identity is out of bounds"))?;
        let metadata = decoder.store.function_metadata[id.index()];
        let name = Name::intern(decoder.store.string(function.name)?);
        if metadata.owner == IR_NONE {
            Ok(Self::Name(name))
        } else {
            Ok(Self::Qualified(QualifiedName::new(
                Name::intern(decoder.store.string(metadata.owner)?),
                name,
            )))
        }
    }
}

impl FullCodec for FunctionName {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        if let Some(name) = self.as_name() {
            output.push(0);
            name.encode(builder, output)
        } else if let Some(name) = self.as_qualified() {
            output.push(1);
            name.encode(builder, output)
        } else {
            Err(IrBuildError::format("function_name", None, 0, 0))
        }
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        match input.raw()? {
            0 => Ok(Self::name(Name::decode(decoder, input)?)),
            1 => Ok(Self::qualified(QualifiedName::decode(decoder, input)?)),
            _ => Err(IrVerifyError::new("function name tag is invalid")),
        }
    }
}

impl FullCodec for Span {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        output.push(builder.intern_location(*self)?.raw());
        Ok(())
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        let id = IrLocationId::from_raw(input.raw()?)
            .ok_or_else(|| IrVerifyError::new("location id is invalid"))?;
        let location = decoder
            .store
            .locations
            .get(id.index())
            .copied()
            .ok_or_else(|| IrVerifyError::new("location id is out of bounds"))?;
        let source_id = decoder
            .store
            .location_sources
            .get(id.index())
            .copied()
            .ok_or_else(|| IrVerifyError::new("location source is out of bounds"))?;
        Ok(Span::new(
            source_id,
            location.start as usize,
            location.start as usize + location.len as usize,
        ))
    }
}

impl FullCodec for Arc<str> {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        output.push(builder.intern_string(self)?.raw());
        Ok(())
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Arc::from(decoder.store.string(input.raw()?)?))
    }

    fn verify(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<(), IrVerifyError> {
        decoder.store.string(input.raw()?).map(drop)
    }
}

impl FullCodec for NameText {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        output.push(builder.intern_string(self.as_str())?.raw());
        Ok(())
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(NameText::Dynamic(Arc::from(
            decoder.store.string(input.raw()?)?,
        )))
    }

    fn verify(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<(), IrVerifyError> {
        decoder.store.string(input.raw()?).map(drop)
    }
}

impl FullCodec for String {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        output.push(builder.intern_string(self)?.raw());
        Ok(())
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(decoder.store.string(input.raw()?)?.to_string())
    }

    fn verify(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<(), IrVerifyError> {
        decoder.store.string(input.raw()?).map(drop)
    }
}

impl FullCodec for Arc<[u8]> {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        output.push(builder.intern_bytes(self)?.raw());
        Ok(())
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Arc::from(decoder.store.bytes(input.raw()?)?))
    }

    fn verify(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<(), IrVerifyError> {
        decoder.store.bytes(input.raw()?).map(drop)
    }
}

impl FullCodec for Vec<u8> {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        output.push(builder.intern_bytes(self)?.raw());
        Ok(())
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(decoder.store.bytes(input.raw()?)?.to_vec())
    }

    fn verify(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<(), IrVerifyError> {
        decoder.store.bytes(input.raw()?).map(drop)
    }
}

impl FullCodec for PathValue {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.bytes.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        PathValue::new(Vec::<u8>::decode(decoder, input)?)
            .map_err(|error| IrVerifyError::new(error.message))
    }
}

impl<T: FullCodec> FullCodec for Option<T> {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        match self {
            Some(value) => {
                output.push(1);
                value.encode(builder, output)
            }
            None => {
                output.push(0);
                Ok(())
            }
        }
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        match input.raw()? {
            0 => Ok(None),
            1 => Ok(Some(T::decode(decoder, input)?)),
            _ => Err(IrVerifyError::new("optional payload tag is invalid")),
        }
    }

    fn verify(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<(), IrVerifyError> {
        match input.raw()? {
            0 => Ok(()),
            1 => T::verify(decoder, input),
            _ => Err(IrVerifyError::new("optional payload tag is invalid")),
        }
    }
}

impl<T: FullCodec> FullCodec for Box<T> {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.as_ref().encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Box::new(T::decode(decoder, input)?))
    }

    fn verify(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<(), IrVerifyError> {
        T::verify(decoder, input)
    }
}

impl<A: FullCodec, B: FullCodec> FullCodec for (A, B) {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.0.encode(builder, output)?;
        self.1.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok((A::decode(decoder, input)?, B::decode(decoder, input)?))
    }

    fn verify(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<(), IrVerifyError> {
        A::verify(decoder, input)?;
        B::verify(decoder, input)
    }
}

impl<A: FullCodec, B: FullCodec, C: FullCodec> FullCodec for (A, B, C) {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.0.encode(builder, output)?;
        self.1.encode(builder, output)?;
        self.2.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok((
            A::decode(decoder, input)?,
            B::decode(decoder, input)?,
            C::decode(decoder, input)?,
        ))
    }

    fn verify(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<(), IrVerifyError> {
        A::verify(decoder, input)?;
        B::verify(decoder, input)?;
        C::verify(decoder, input)
    }
}

macro_rules! impl_vec_codec {
    ($ty:ty, $flags:expr) => {
        impl FullCodec for Vec<$ty> {
            fn encode(
                &self,
                builder: &mut FullBuilder,
                output: &mut Vec<u32>,
            ) -> Result<(), IrBuildError> {
                let mut instructions = Vec::new();
                instructions.push(
                    u32::try_from(self.len())
                        .map_err(|_| IrBuildError::format("vector_overflow", None, 0, 0))?,
                );
                for value in self {
                    value.encode(builder, &mut instructions)?;
                }
                output.push(builder.push_block(&instructions, $flags)?.raw());
                Ok(())
            }

            fn decode(
                decoder: &FullDecoder<'_>,
                input: &mut FullCursor<'_>,
            ) -> Result<Self, IrVerifyError> {
                let (block_id, mut block) = decoder.block(input, $flags)?;
                let len = block.raw()? as usize;
                let mut values = Vec::with_capacity(len);
                for _ in 0..len {
                    values.push(<$ty>::decode(decoder, &mut block)?);
                }
                block.finish()?;
                decoder.finish_block(block_id);
                Ok(values)
            }

            fn verify(
                decoder: &FullDecoder<'_>,
                input: &mut FullCursor<'_>,
            ) -> Result<(), IrVerifyError> {
                let (block_id, mut block) = decoder.block(input, $flags)?;
                let len = block.raw()? as usize;
                for _ in 0..len {
                    <$ty>::verify(decoder, &mut block)?;
                }
                block.finish()?;
                decoder.finish_block(block_id);
                Ok(())
            }
        }
    };
}

macro_rules! impl_copy_pool_codec {
    ($ty:ty, $field:ident, $label:literal) => {
        impl FullCodec for $ty {
            fn encode(
                &self,
                builder: &mut FullBuilder,
                output: &mut Vec<u32>,
            ) -> Result<(), IrBuildError> {
                output.push(FullBuilder::intern_copy(
                    &mut builder.store.$field,
                    *self,
                    $label,
                )?);
                Ok(())
            }

            fn decode(
                decoder: &FullDecoder<'_>,
                input: &mut FullCursor<'_>,
            ) -> Result<Self, IrVerifyError> {
                decoder
                    .store
                    .$field
                    .get(input.raw()? as usize)
                    .copied()
                    .ok_or_else(|| IrVerifyError::new(concat!($label, " is out of bounds")))
            }
        }
    };
}

impl_copy_pool_codec!(RuntimeOp, runtime_ops, "runtime operation");
impl_copy_pool_codec!(AssignOp, assign_ops, "assignment operation");
impl_copy_pool_codec!(BinaryOp, binary_ops, "binary operation");
impl_copy_pool_codec!(RunKind, run_kinds, "run kind");
impl_copy_pool_codec!(RedirectionKind, redirection_kinds, "redirection kind");

impl FullCodec for PreparedConstantValue {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        let index = u32::try_from(builder.store.prepared_constants.len())
            .map_err(|_| IrBuildError::format("constant pool overflow", None, 0, 0))?;
        visit_value_wire_mappings(&self.0, &mut |mapping| {
            register_wire_mapping(builder, mapping);
            true
        });
        builder.store.prepared_constants.push(self.clone());
        output.push(index);
        Ok(())
    }
    fn decode(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<Self, IrVerifyError> {
        decoder.store.prepared_constants.get(input.raw()? as usize).cloned()
            .ok_or_else(|| IrVerifyError::new("prepared constant is out of bounds"))
    }
    fn verify(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<(), IrVerifyError> {
        let value = decoder.store.prepared_constants.get(input.raw()? as usize)
            .ok_or_else(|| IrVerifyError::new("prepared constant is out of bounds"))?;
        if !prepared_constant_is_data(&value.0, 0) {
            return Err(IrVerifyError::new("prepared constant contains a runtime value"));
        }
        if !visit_value_wire_mappings(&value.0, &mut |mapping| wire_mapping_matches_pool(mapping, decoder.store)) {
            return Err(IrVerifyError::new("prepared constant has a contradictory wire mapping"));
        }
        Ok(())
    }
}

// Constructors, constants, and schema conversions share one declaring mapping.
// Verification rejects independently altered copies before they can disagree at a boundary.
fn register_wire_mapping(builder: &mut FullBuilder, mapping: &Arc<crate::sema::wire_enums::WireEnumMapping>) {
    if !builder.store.wire_enums.iter().any(|stored| Arc::ptr_eq(stored, mapping)) {
        builder.store.wire_enums.push(mapping.clone());
    }
}

fn wire_mapping_matches_pool(mapping: &Arc<crate::sema::wire_enums::WireEnumMapping>, store: &FullStore) -> bool {
    store.wire_enums.iter().find(|stored| stored.type_name == mapping.type_name)
        .is_some_and(|stored| stored.as_ref() == mapping.as_ref())
}

fn visit_value_wire_mappings(value: &LoweredValue, visit: &mut impl FnMut(&Arc<crate::sema::wire_enums::WireEnumMapping>) -> bool) -> bool {
    match value {
        LoweredValue::Tag(tag) => tag.wire.as_ref().is_none_or(&mut *visit)
            && tag.fields.iter().all(|value| visit_value_wire_mappings(value, visit)),
        LoweredValue::List(values) => values.iter().all(|value| visit_value_wire_mappings(value, visit)),
        LoweredValue::SharedList(values) => values.iter().all(|value| visit_value_wire_mappings(value, visit)),
        LoweredValue::Record(values) => values.values().all(|value| visit_value_wire_mappings(value, visit)),
        LoweredValue::RecordVec(values) => values.iter().all(|(_, value)| visit_value_wire_mappings(value, visit)),
        LoweredValue::Map(values) => values.values().all(|value| visit_value_wire_mappings(value, visit)),
        _ => true,
    }
}

impl FullCodec for Arc<crate::modules::cli::CliDescriptorPlan> {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        let index = if let Some(index) = builder.store.prepared_cli_plans.iter().position(|plan| Arc::ptr_eq(plan, self)) { index }
            else { let index = builder.store.prepared_cli_plans.len(); builder.store.prepared_cli_plans.push(Arc::clone(self)); index };
        output.push(u32::try_from(index).map_err(|_| IrBuildError::format("CLI plan pool overflow", None, 0, 0))?);
        Ok(())
    }
    fn decode(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<Self, IrVerifyError> {
        decoder.store.prepared_cli_plans.get(input.raw()? as usize).cloned()
            .ok_or_else(|| IrVerifyError::new("prepared CLI descriptor plan is out of bounds"))
    }
}

fn prepared_constant_is_data(value: &LoweredValue, depth: usize) -> bool {
    if depth > 128 { return false; }
    match value {
        LoweredValue::Null | LoweredValue::Bool(_) | LoweredValue::Int(_) | LoweredValue::Float(_)
        | LoweredValue::Duration(_) | LoweredValue::Str(_) | LoweredValue::Bytes(_)
        | LoweredValue::Path(_) | LoweredValue::Regex(_) => true,
        LoweredValue::List(values) => values.iter().all(|value| prepared_constant_is_data(value, depth + 1)),
        LoweredValue::SharedList(values) => values.iter().all(|value| prepared_constant_is_data(value, depth + 1)),
        LoweredValue::Record(values) => values.values().all(|value| prepared_constant_is_data(value, depth + 1)),
        LoweredValue::RecordVec(values) => values.iter().all(|(_, value)| prepared_constant_is_data(value, depth + 1)),
        LoweredValue::Map(values) => values.values().all(|value| prepared_constant_is_data(value, depth + 1)),
        LoweredValue::Tag(value) => value.fields.iter().all(|value| prepared_constant_is_data(value, depth + 1))
            && value.wire.as_ref().is_none_or(|mapping| value.fields.is_empty()
                && mapping.type_name == value.type_name
                && mapping.variants.contains_key(&Name::intern(value.name.as_ref()))
                && !mapping.variants.is_empty()
                && mapping.variants.values().collect::<std::collections::BTreeSet<_>>().len() == mapping.variants.len()),
        _ => false,
    }
}

impl FullCodec for Arc<crate::sema::wire_enums::WireEnumMapping> {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        let index = if let Some(index) = builder.store.wire_enums.iter().position(|mapping| Arc::ptr_eq(mapping, self)) {
            index
        } else {
            let index = builder.store.wire_enums.len();
            builder.store.wire_enums.push(self.clone());
            index
        };
        output.push(u32::try_from(index).map_err(|_| IrBuildError::format("wire enum pool overflow", None, 0, 0))?);
        Ok(())
    }

    fn decode(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<Self, IrVerifyError> {
        let mapping = decoder.store.wire_enums.get(input.raw()? as usize).cloned()
            .ok_or_else(|| IrVerifyError::new("wire enum mapping is out of bounds"))?;
        if !decoder.verified && (mapping.variants.is_empty() || mapping.variants.values().collect::<std::collections::BTreeSet<_>>().len() != mapping.variants.len()) {
            return Err(IrVerifyError::new("wire enum mapping is empty or has duplicate strings"));
        }
        Ok(mapping)
    }
}

impl FullCodec for RegexValue {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        let index = u32::try_from(builder.store.prepared_regexes.len())
            .map_err(|_| IrBuildError::format("regex pool overflow", None, 0, 0))?;
        builder.store.prepared_regexes.push(self.clone());
        output.push(index);
        Ok(())
    }

    fn decode(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<Self, IrVerifyError> {
        decoder.store.prepared_regexes.get(input.raw()? as usize).cloned()
            .ok_or_else(|| IrVerifyError::new("prepared regex is out of bounds"))
    }
}

impl FullCodec for LoweredStrPredicate {
    fn encode(
        &self,
        _builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
        output.push(match self {
            Self::StartsWith => 0,
            Self::EndsWith => 1,
        });
        Ok(())
    }

    fn decode(
        _decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        match input.raw()? {
            0 => Ok(Self::StartsWith),
            1 => Ok(Self::EndsWith),
            _ => Err(IrVerifyError::new("string predicate is invalid")),
        }
    }
}

impl FullCodec for ReduceByOp {
    fn encode(
        &self,
        _builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
        output.push(match self {
            Self::Sum => 0,
            Self::Min => 1,
            Self::Max => 2,
        });
        Ok(())
    }

    fn decode(
        _decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        match input.raw()? {
            0 => Ok(Self::Sum),
            1 => Ok(Self::Min),
            2 => Ok(Self::Max),
            _ => Err(IrVerifyError::new("reduce-by operation is invalid")),
        }
    }
}

impl FullCodec for HashAlgorithm {
    fn encode(
        &self,
        _builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
        output.push(match self {
            Self::Md5 => 0,
            Self::Sha1 => 1,
            Self::Sha256 => 2,
            Self::Sha512 => 3,
        });
        Ok(())
    }

    fn decode(
        _decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        match input.raw()? {
            0 => Ok(Self::Md5),
            1 => Ok(Self::Sha1),
            2 => Ok(Self::Sha256),
            3 => Ok(Self::Sha512),
            _ => Err(IrVerifyError::new("hash algorithm is invalid")),
        }
    }
}

impl FullCodec for FormatSpec {
    fn encode(
        &self,
        _builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
        output.push(match self.kind {
            FormatSpecKind::RightAlign => 0,
            FormatSpecKind::LeftAlign => 1,
            FormatSpecKind::ZeroPad => 2,
        });
        output.push(
            u32::try_from(self.width)
                .map_err(|_| IrBuildError::format("format_width_overflow", None, 0, 0))?,
        );
        Ok(())
    }

    fn decode(
        _decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        let kind = match input.raw()? {
            0 => FormatSpecKind::RightAlign,
            1 => FormatSpecKind::LeftAlign,
            2 => FormatSpecKind::ZeroPad,
            _ => return Err(IrVerifyError::new("format specifier kind is invalid")),
        };
        Ok(Self {
            kind,
            width: input.raw()? as usize,
        })
    }
}

impl FullCodec for Type {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        output.push(
            builder
                .semantic
                .intern_type(&mut builder.store.semantic, &executable_type(self))?
                .raw(),
        );
        Ok(())
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        let id = TypeId::from_raw(input.raw()?)
            .ok_or_else(|| IrVerifyError::new("semantic type id is invalid"))?;
        decoder.store.semantic.to_type(id)
    }
}

impl FullCodec for LoweredType {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        if *self == LoweredType::Generic {
            let owner = builder.current_owner.and_then(IrFunctionId::from_raw).ok_or_else(|| IrBuildError::format("generic_storage_without_function", None, 0, 0))?;
            if builder.store.function_metadata.get(owner.index()).is_none_or(|metadata| metadata.flags & 4 == 0) { return Err(IrBuildError::format("generic_storage_without_scope", None, 0, 0)); }
            output.push(IR_NONE);
            Ok(())
        } else { lowered_type_to_type(*self)?.encode(builder, output) }
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        let raw = input.raw()?;
        if raw == IR_NONE {
            let owner = IrFunctionId::from_raw(decoder.owner).ok_or_else(|| IrVerifyError::new("generic storage belongs to a non-function owner"))?;
            if decoder.store.function_metadata.get(owner.index()).is_none_or(|metadata| metadata.flags & 4 == 0) || decoder.store.generic.as_deref().and_then(|evidence| evidence.scope_for_function(owner)).is_none() { return Err(IrVerifyError::new("generic storage lacks scoped metadata")); }
            Ok(LoweredType::Generic)
        } else { lowered_type_from_type(&decoder.store.semantic.to_type(TypeId::from_raw(raw).ok_or_else(|| IrVerifyError::new("lowered type id is invalid"))?)?) }
    }
}

impl FullCodec for Arc<super::super::require::PreparedSchema> {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        let index = if let Some(index) = builder.store.prepared_schemas.iter().position(|schema| Arc::ptr_eq(schema, self)) {
            index
        } else {
            let index = builder.store.prepared_schemas.len();
            self.visit_wire_mappings(&mut |mapping| {
                register_wire_mapping(builder, mapping);
                true
            });
            builder.store.prepared_schemas.push(self.clone());
            index
        };
        output.push(u32::try_from(index).map_err(|_| IrBuildError::format("schema pool overflow", None, 0, 0))?);
        Ok(())
    }

    fn decode(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<Self, IrVerifyError> {
        let schema = decoder.store.prepared_schemas.get(input.raw()? as usize).cloned()
            .ok_or_else(|| IrVerifyError::new("prepared schema is out of bounds"))?;
        if !decoder.verified && (!schema.valid() || !schema.visit_wire_mappings(&mut |mapping| wire_mapping_matches_pool(mapping, decoder.store))) {
            return Err(IrVerifyError::new("prepared schema has an invalid or contradictory wire mapping"));
        }
        Ok(schema)
    }
}

impl FullCodec for LoweredTypeCheck {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.ty.encode(builder, output)?;
        self.name.encode(builder, output)?;
        self.schema.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        let check = Self {
            ty: Type::decode(decoder, input)?,
            name: Arc::<str>::decode(decoder, input)?,
            schema: Option::decode(decoder, input)?,
        };
        if !decoder.verified && check.schema.as_ref().is_some_and(|schema| !schema.matches_type(&check.ty)) {
            return Err(IrVerifyError::new("prepared schema does not match its checked type"));
        }
        Ok(check)
    }
}

impl FullCodec for LoweredTopLevelSlot {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.name.encode(builder, output)?;
        self.slot.encode(builder, output)?;
        self.kind.encode(builder, output)?;
        self.mutable.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Self {
            name: Name::decode(decoder, input)?,
            slot: usize::decode(decoder, input)?,
            kind: LoweredType::decode(decoder, input)?,
            mutable: bool::decode(decoder, input)?,
            source_type: None,
            host_binding: None,
            lexical_binding: None,
        })
    }
}

impl FullCodec for LoweredModuleExport {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.name.encode(builder, output)?;
        output.push(match self.kind {
            LoweredModuleExportKind::Value => 0,
            LoweredModuleExportKind::Pure => 1,
            LoweredModuleExportKind::Proc => 2,
        });
        self.function_namespace.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        let name = Name::decode(decoder, input)?;
        let kind = match input.raw()? {
            0 => LoweredModuleExportKind::Value,
            1 => LoweredModuleExportKind::Pure,
            2 => LoweredModuleExportKind::Proc,
            _ => return Err(IrVerifyError::new("module export kind is invalid")),
        };
        Ok(Self {
            name,
            kind,
            function_namespace: Option::<Name>::decode(decoder, input)?,
        })
    }
}

macro_rules! impl_btree_codec {
    ($key:ty, $value:ty) => {
        impl FullCodec for BTreeMap<$key, $value> {
            fn encode(
                &self,
                builder: &mut FullBuilder,
                output: &mut Vec<u32>,
            ) -> Result<(), IrBuildError> {
                output.push(
                    u32::try_from(self.len())
                        .map_err(|_| IrBuildError::format("map_overflow", None, 0, 0))?,
                );
                for (key, value) in self {
                    key.encode(builder, output)?;
                    value.encode(builder, output)?;
                }
                Ok(())
            }

            fn decode(
                decoder: &FullDecoder<'_>,
                input: &mut FullCursor<'_>,
            ) -> Result<Self, IrVerifyError> {
                let len = input.raw()? as usize;
                let mut values = BTreeMap::new();
                for _ in 0..len {
                    let key = <$key>::decode(decoder, input)?;
                    let value = <$value>::decode(decoder, input)?;
                    if values.insert(key, value).is_some() {
                        return Err(IrVerifyError::new("map payload contains a duplicate key"));
                    }
                }
                Ok(values)
            }
        }
    };
}

impl_btree_codec!(String, LoweredValue);
impl_btree_codec!(crate::map_key::MapKey, LoweredValue);

impl FullCodec for crate::map_key::MapKey {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        crate::runtime::eval::lowered_ops::lowered_map_key_value(self).encode(builder, output)
    }
    fn decode(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<Self, IrVerifyError> {
        let value = LoweredValue::decode(decoder, input)?;
        crate::runtime::eval::lowered_ops::lowered_map_key_ref(&value, crate::source::Span::at(crate::source::SourceId::new(0), 0))
            .map(|key| key.to_owned()).map_err(|_| IrVerifyError::new("map payload contains an invalid scalar key"))
    }
}
impl_btree_codec!(Arc<str>, LoweredValue);

impl FullCodec for LoweredValue {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        let mut payload = builder.take_payload();
        let tag = match self {
            Self::Null => FullValueTag::Null,
            Self::Unit => FullValueTag::Unit,
            Self::Int(value) => {
                value.encode(builder, &mut payload)?;
                FullValueTag::Int
            }
            Self::Float(value) => {
                value.encode(builder, &mut payload)?;
                FullValueTag::Float
            }
            Self::Duration(value) => {
                value.encode(builder, &mut payload)?;
                FullValueTag::Duration
            }
            Self::Bool(value) => {
                value.encode(builder, &mut payload)?;
                FullValueTag::Bool
            }
            Self::Str(value) => {
                value.encode(builder, &mut payload)?;
                FullValueTag::Str
            }
            Self::StrView(value) => {
                Arc::<str>::from(value.as_str()).encode(builder, &mut payload)?;
                FullValueTag::Str
            }
            Self::Bytes(value) => {
                value.encode(builder, &mut payload)?;
                FullValueTag::Bytes
            }
            Self::BytesView(value) => {
                Arc::<[u8]>::from(value.as_slice()).encode(builder, &mut payload)?;
                FullValueTag::Bytes
            }
            Self::Path(value) => {
                value.encode(builder, &mut payload)?;
                FullValueTag::Path
            }
            Self::Record(value) => {
                value.encode(builder, &mut payload)?;
                FullValueTag::Record
            }
            Self::RecordVec(value) => {
                value.encode(builder, &mut payload)?;
                FullValueTag::RecordVec
            }
            Self::Stats {
                blanks,
                code,
                comments,
            } => {
                blanks.encode(builder, &mut payload)?;
                code.encode(builder, &mut payload)?;
                comments.encode(builder, &mut payload)?;
                FullValueTag::Stats
            }
            Self::StatsBlob(value) => {
                value.blanks.encode(builder, &mut payload)?;
                value.blobs.encode(builder, &mut payload)?;
                value.code.encode(builder, &mut payload)?;
                value.comments.encode(builder, &mut payload)?;
                FullValueTag::StatsBlob
            }
            Self::Module(value) => {
                value.encode(builder, &mut payload)?;
                FullValueTag::Module
            }
            Self::List(value) => {
                value.encode(builder, &mut payload)?;
                FullValueTag::List
            }
            Self::SharedList(value) => {
                value.as_ref().encode(builder, &mut payload)?;
                FullValueTag::List
            }
            Self::Map(value) => {
                value.encode(builder, &mut payload)?;
                FullValueTag::Map
            }
            Self::Tag(value) => {
                value.type_name.encode(builder, &mut payload)?;
                value.wire.encode(builder, &mut payload)?;
                value.name.encode(builder, &mut payload)?;
                value.fields.encode(builder, &mut payload)?;
                FullValueTag::Tag
            }
            Self::ResultOk(value) => {
                value.encode(builder, &mut payload)?;
                FullValueTag::ResultOk
            }
            Self::Regex(value) => {
                value.as_ref().encode(builder, &mut payload)?;
                FullValueTag::Regex
            }
            Self::Digest(_)
            | Self::Status(_)
            | Self::FsEntry(_)
            | Self::Command(_)
            | Self::ProcessHandle(_)
            | Self::NetJob(_)
            | Self::FsRoot(_)
            | Self::Stream(_)
            | Self::Pure(_)
            | Self::Proc(_)
            | Self::Callable(_)
            | Self::NativeCallable(_)
            | Self::Error(_)
            | Self::ResultErr(_) => {
                return Err(IrBuildError::format(
                    "non_literal_persistent_value",
                    None,
                    0,
                    0,
                ));
            }
        };
        let value = builder.push_value(tag, &payload);
        builder.recycle_payload(payload);
        output.push(value?);
        Ok(())
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        let index = input.raw()? as usize;
        let tag = decoder
            .store
            .values
            .get(index)
            .copied()
            .ok_or_else(|| IrVerifyError::new("literal value id is out of bounds"))?;
        let data = decoder.store.value_data[index];
        let mut payload = FullCursor::new(decoder.store.payload(data.range())?);
        let value = match tag {
            FullValueTag::Null => Self::Null,
            FullValueTag::Unit => Self::Unit,
            FullValueTag::Int => Self::Int(i64::decode(decoder, &mut payload)?),
            FullValueTag::Float => Self::Float(FloatValue::decode(decoder, &mut payload)?),
            FullValueTag::Duration => Self::Duration(DurationValue::decode(decoder, &mut payload)?),
            FullValueTag::Bool => Self::Bool(bool::decode(decoder, &mut payload)?),
            FullValueTag::Str => Self::Str(Arc::<str>::decode(decoder, &mut payload)?),
            FullValueTag::Bytes => Self::Bytes(Arc::<[u8]>::decode(decoder, &mut payload)?),
            FullValueTag::Regex => Self::Regex(Box::new(RegexValue::decode(decoder, &mut payload)?)),
            FullValueTag::Path => Self::Path(PathValue::decode(decoder, &mut payload)?),
            FullValueTag::Record => Self::Record(Arc::new(
                BTreeMap::<Arc<str>, LoweredValue>::decode(decoder, &mut payload)?,
            )),
            FullValueTag::RecordVec => Self::RecordVec(Arc::new(
                Vec::<(Name, LoweredValue)>::decode(decoder, &mut payload)?,
            )),
            FullValueTag::Stats => Self::Stats {
                blanks: i64::decode(decoder, &mut payload)?,
                code: i64::decode(decoder, &mut payload)?,
                comments: i64::decode(decoder, &mut payload)?,
            },
            FullValueTag::StatsBlob => Self::StatsBlob(Box::new(LoweredStatsValue {
                blanks: i64::decode(decoder, &mut payload)?,
                blobs: BTreeMap::<crate::map_key::MapKey, LoweredValue>::decode(decoder, &mut payload)?,
                code: i64::decode(decoder, &mut payload)?,
                comments: i64::decode(decoder, &mut payload)?,
            })),
            FullValueTag::Module => Self::Module(Arc::new(
                BTreeMap::<Arc<str>, LoweredValue>::decode(decoder, &mut payload)?,
            )),
            FullValueTag::List => Self::List(Vec::<LoweredValue>::decode(decoder, &mut payload)?),
            FullValueTag::Map => Self::Map(Arc::new(BTreeMap::<crate::map_key::MapKey, LoweredValue>::decode(
                decoder,
                &mut payload,
            )?)),
            FullValueTag::Tag => Self::Tag(Box::new(LoweredTagValue {
                type_name: Name::decode(decoder, &mut payload)?,
                wire: Option::decode(decoder, &mut payload)?,
                name: Arc::<str>::decode(decoder, &mut payload)?,
                fields: Vec::<LoweredValue>::decode(decoder, &mut payload)?,
            })),
            FullValueTag::ResultOk => {
                Self::ResultOk(Box::new(LoweredValue::decode(decoder, &mut payload)?))
            }
        };
        payload.finish()?;
        Ok(value)
    }

    fn verify(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<(), IrVerifyError> {
        let index = input.raw()? as usize;
        let tag = decoder
            .store
            .values
            .get(index)
            .copied()
            .ok_or_else(|| IrVerifyError::new("literal value id is out of bounds"))?;
        let data = decoder.store.value_data[index];
        let mut payload = FullCursor::new(decoder.store.payload(data.range())?);
        match tag {
            FullValueTag::Null | FullValueTag::Unit => {}
            FullValueTag::Int => i64::verify(decoder, &mut payload)?,
            FullValueTag::Float => FloatValue::verify(decoder, &mut payload)?,
            FullValueTag::Duration => DurationValue::verify(decoder, &mut payload)?,
            FullValueTag::Bool => bool::verify(decoder, &mut payload)?,
            FullValueTag::Str => Arc::<str>::verify(decoder, &mut payload)?,
            FullValueTag::Bytes => Arc::<[u8]>::verify(decoder, &mut payload)?,
            FullValueTag::Regex => RegexValue::verify(decoder, &mut payload)?,
            FullValueTag::Path => PathValue::verify(decoder, &mut payload)?,
            FullValueTag::Record => {
                BTreeMap::<Arc<str>, LoweredValue>::verify(decoder, &mut payload)?;
            }
            FullValueTag::RecordVec => {
                Vec::<(Name, LoweredValue)>::verify(decoder, &mut payload)?;
            }
            FullValueTag::Stats => {
                i64::verify(decoder, &mut payload)?;
                i64::verify(decoder, &mut payload)?;
                i64::verify(decoder, &mut payload)?;
            }
            FullValueTag::StatsBlob => {
                i64::verify(decoder, &mut payload)?;
                BTreeMap::<String, LoweredValue>::verify(decoder, &mut payload)?;
                i64::verify(decoder, &mut payload)?;
                i64::verify(decoder, &mut payload)?;
            }
            FullValueTag::Module => {
                BTreeMap::<Arc<str>, LoweredValue>::verify(decoder, &mut payload)?;
            }
            FullValueTag::List => Vec::<LoweredValue>::verify(decoder, &mut payload)?,
            FullValueTag::Map => {
                BTreeMap::<String, LoweredValue>::verify(decoder, &mut payload)?;
            }
            FullValueTag::Tag => {
                Name::verify(decoder, &mut payload)?;
                Option::<Arc<crate::sema::wire_enums::WireEnumMapping>>::verify(decoder, &mut payload)?;
                Arc::<str>::verify(decoder, &mut payload)?;
                Vec::<LoweredValue>::verify(decoder, &mut payload)?;
            }
            FullValueTag::ResultOk => LoweredValue::verify(decoder, &mut payload)?,
        }
        payload.finish()
    }
}

macro_rules! impl_node_codec {
    (
        $ty:ty {
            $(
                $pattern:pat => $tag:ident {
                    $($field:ident : $field_ty:ty),* $(,)?
                } => $construct:expr
            ),* $(,)?
        }
    ) => {
        impl FullCodec for $ty {
            fn encode(
                &self,
                builder: &mut FullBuilder,
                output: &mut Vec<u32>,
            ) -> Result<(), IrBuildError> {
                let (tag, payload) = match self {
                    $(
                        $pattern => {
                            #[allow(unused_mut)]
                            let mut payload = builder.take_payload();
                            $(
                                $field.encode(builder, &mut payload)?;
                            )*
                            (FullTag::$tag, payload)
                        }
                    ),*
                };
                let instruction = builder.push_instruction(tag, &payload);
                builder.recycle_payload(payload);
                output.push(instruction?);
                Ok(())
            }

            fn decode(
                _decoder: &FullDecoder<'_>,
                _input: &mut FullCursor<'_>,
            ) -> Result<Self, IrVerifyError> {
                Err(IrVerifyError::new(
                    "indexed construction rows cannot be decoded from executable IR",
                ))
            }

            fn verify(
                decoder: &FullDecoder<'_>,
                input: &mut FullCursor<'_>,
            ) -> Result<(), IrVerifyError> {
                let (instruction, tag, mut payload) = decoder.instruction(input)?;
                if tag == FullTag::ExprModuleCall {
                    let mut metadata = payload;
                    let op = RuntimeOp::decode(decoder, &mut metadata)?;
                    let plan = Option::<Arc<crate::modules::cli::CliDescriptorPlan>>::decode(decoder, &mut metadata)?;
                    if plan.as_ref().is_some_and(|plan| !plan.matches_operation(op)) {
                        return Err(IrVerifyError::new("prepared CLI plan operation policy does not match its instruction"));
                    }
                }
                if tag == FullTag::ExprComparisonChain { decoder.verify_comparison_chain_shape(payload)?; }
                if tag == FullTag::ExprTag {
                    let mut metadata = payload;
                    let type_name = Name::decode(decoder, &mut metadata)?;
                    let name = Arc::<str>::decode(decoder, &mut metadata)?;
                    let fields_id = IrBlockId::from_raw(metadata.raw()?).ok_or_else(|| IrVerifyError::new("tag fields block id is invalid"))?;
                    let fields = decoder.store.blocks.get(fields_id.index()).ok_or_else(|| IrVerifyError::new("tag fields block is out of bounds"))?;
                    let field_count = decoder.store.payload(fields.instructions)?.first().copied().ok_or_else(|| IrVerifyError::new("tag fields length is missing"))?;
                    let wire = Option::<Arc<crate::sema::wire_enums::WireEnumMapping>>::decode(decoder, &mut metadata)?;
                    if wire.as_ref().is_some_and(|mapping| mapping.type_name != type_name || field_count != 0 || !mapping.variants.contains_key(&Name::intern(name.as_ref()))) {
                        return Err(IrVerifyError::new("wire enum constructor identity or payload is invalid"));
                    }
                }
                match tag {
                    $(
                        FullTag::$tag => {
                            $(
                                <$field_ty>::verify(decoder, &mut payload)?;
                            )*
                        }
                    ),*
                    _ => return Err(IrVerifyError::new("full IR instruction tag has the wrong category")),
                }
                payload.finish()?;
                if tag == FullTag::ExprRetry {
                    let payload = decoder.cursor(decoder.store.payload(decoder.store.data[instruction].range())?);
                    decoder.verify_retry_selection(payload)?;
                }
                decoder.finish_instruction(instruction);
                Ok(())
            }
        }
    };
}

macro_rules! impl_build_id_codec {
    ($id:ty, $rows:ident, $row:ty) => {
        impl FullCodec for $id {
            fn encode(
                &self,
                builder: &mut FullBuilder,
                output: &mut Vec<u32>,
            ) -> Result<(), IrBuildError> {
                let scratch = builder.active_scratch.clone().ok_or_else(|| {
                    IrBuildError::format("missing_indexed_build_scratch", None, 0, 0)
                })?;
                let scratch = scratch.borrow();
                let row = scratch
                    .$rows
                    .get(self.index())
                    .ok_or_else(|| IrBuildError::format("indexed_build_id", None, 0, 0))?;
                row.encode(builder, output)?;
                if stringify!($rows) == "expressions" {
                    let expression = BuildExprId::new(self.index());
                    let instruction = *output.last().ok_or_else(|| IrBuildError::format("callable_encoded_expression_missing", None, 0, 0))?;
                    builder.active_encoded_expressions.insert(expression, instruction);
                    builder.stage_original_block_callback(expression, instruction, &scratch)?;
                    if let Some(original) = scratch.named_map_key_origins.get(&expression) {
                        let raw = builder.current_owner.ok_or_else(|| IrBuildError::format("named_map_key_owner_missing", None, 0, 0))?;
                        let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| IrBuildError::format("named_map_key_owner_invalid", None, 0, 0))?) };
                        builder.stage_named_map_key(original.clone(), instruction, owner)?;
                    }
                    if scratch.container_creation_checks.contains_key(&expression) {
                        let raw = builder.current_owner.ok_or_else(|| IrBuildError::format("container_creation_owner_missing", None, 0, 0))?;
                        let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| IrBuildError::format("container_creation_owner_invalid", None, 0, 0))?) };
                        builder.stage_original_container_creation_check(expression, instruction, owner, &scratch)?;
                    }
                    builder.stage_original_record_source(expression, instruction, &scratch)?;
                    if scratch.result_receiver_origins.contains_key(&expression) {
                        let raw = builder.current_owner.ok_or_else(|| IrBuildError::format("result_receiver_owner_missing", None, 0, 0))?;
                        let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| IrBuildError::format("result_receiver_owner_invalid", None, 0, 0))?) };
                        builder.stage_original_result_receiver(expression, instruction, owner, &scratch)?;
                    }
                    builder.stage_optional_receiver_guard(expression, instruction, &scratch)?;
                    builder.stage_formatted_path(expression, instruction, &scratch)?;
                    builder.stage_original_constant(expression, instruction, &scratch)?;
                    builder.stage_original_record_update(expression, instruction, &scratch)?;
                    builder.stage_original_error_constructor(expression, instruction, &scratch)?;
                    if let Some(&origin) = builder.active_expression_origins.get(&expression) {
                        let raw = builder.current_owner.ok_or_else(|| IrBuildError::format("conditional_owner_missing", None, 0, 0))?;
                        let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| IrBuildError::format("conditional_owner_invalid", None, 0, 0))?) };
                        builder.stage_conditional_result(instruction, origin, owner, &scratch)?;
                        builder.stage_lexical_capture_read(instruction, origin, owner, &scratch)?;
                        if scratch.host_binding_reads.contains_key(&origin) {
                            let raw = builder.current_owner.ok_or_else(|| IrBuildError::format("host_binding_owner_missing", None, 0, 0))?;
                            let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| IrBuildError::format("host_binding_owner_invalid", None, 0, 0))?) };
                            builder.stage_host_binding_read(instruction, origin, owner, &scratch)?;
                        }
                    }
                    builder.stage_original_record_constructor(expression, instruction, &scratch)?;
                    builder.stage_original_index(expression, instruction, &scratch)?;
                    builder.stage_original_comprehension(expression, instruction, &scratch)?;
                    builder.stage_original_context_scope(expression, instruction, &scratch)?;
                    builder.stage_original_run_producer(expression, instruction, &scratch)?;
                    builder.stage_try_capture(expression, instruction, &scratch)?;
                    builder.stage_compiler_argument_wrapper(expression, instruction, &scratch)?;
                    if scratch.native_receiver_origins.contains_key(&expression) || builder.active_native_receiver_wrappers.contains_key(&expression) {
                        let raw = builder.current_owner.ok_or_else(|| IrBuildError::format("native_saved_receiver_owner_missing", None, 0, 0))?;
                        let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| IrBuildError::format("native_saved_receiver_owner_invalid", None, 0, 0))?) };
                        builder.stage_saved_native_receiver(expression, instruction, owner, &scratch)?;
                    }
                    builder.stage_pattern_admission_control(super::super::BuildPatternControlRow::Expression(expression), instruction, &scratch)?;
                    if let Some(original) = scratch.argument_binding_origins.get(&expression) {
                        let raw = builder.current_owner.ok_or_else(|| IrBuildError::format("saved_argument_owner_missing", None, 0, 0))?;
                        let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| IrBuildError::format("saved_argument_owner_invalid", None, 0, 0))?) };
                        let (wrapper, _) = original.wrapper.ok_or_else(|| IrBuildError::format("saved_argument_wrapper_missing", None, 0, 0))?;
                        builder.active_argument_wrappers.entry(wrapper).or_default().push(builder.argument_binding_rows.len());
                        builder.argument_binding_rows.push((original.clone(), instruction, owner, None));
                    }
                    if let Some(rows) = builder.active_argument_wrappers.remove(&expression) {
                        let (initializer, pattern, _) = argument_prepare::saved_argument_wrapper(&builder.store, instruction).map_err(|_| IrBuildError::format("saved_argument_wrapper_changed", None, 0, 0))?;
                        for row in rows { builder.argument_binding_rows[row].3 = Some((instruction, initializer, pattern)); }
                    }
                    if let Some(original) = scratch.callable_receiver_origins.get(&expression) {
                        let raw = builder.current_owner.ok_or_else(|| IrBuildError::format("callable_receiver_owner_missing", None, 0, 0))?;
                        let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| IrBuildError::format("callable_receiver_owner_invalid", None, 0, 0))?) };
                        let (wrapper, _) = original.wrapper.ok_or_else(|| IrBuildError::format("callable_receiver_wrapper_missing", None, 0, 0))?;
                        builder.active_callable_receiver_wrappers.entry(wrapper).or_default().push(builder.callable_receiver_rows.len());
                        builder.callable_receiver_rows.push((original.clone(), instruction, owner, None));
                        if original.capture.is_none() { builder.callable_use_rows.push(super::generic::OriginalCallableUse { instruction, origin: original.origin, binding: original.binding, owner }); }
                    }
                    if let Some(rows) = builder.active_callable_receiver_wrappers.remove(&expression) {
                        let (initializer, pattern, _) = argument_prepare::saved_argument_wrapper(&builder.store, instruction).map_err(|_| IrBuildError::format("callable_receiver_wrapper_changed", None, 0, 0))?;
                        for row in rows { builder.callable_receiver_rows[row].3 = Some((instruction, initializer, pattern)); }
                    }
                    if builder.store.tags[instruction as usize] == FullTag::ExprParam
                        && let Some(&origin) = builder.active_expression_origins.get(&expression)
                        && let Some(&binding) = scratch.callable_binding_uses.get(&origin)
                        && scratch.callable_binding_origins.contains_key(&binding) {
                        let raw = builder.current_owner.ok_or_else(|| IrBuildError::format("callable_use_owner_missing", None, 0, 0))?;
                        let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| IrBuildError::format("callable_use_owner_invalid", None, 0, 0))?) };
                        builder.callable_use_rows.push(super::generic::OriginalCallableUse { instruction, origin, binding, owner });
                    }
                }
                if stringify!($rows) == "statements" && (!builder.active_pattern_admissions.is_empty() || !scratch.conditional_result_origins.is_empty()) {
                    let statement = BuildStmtId::new(self.index());
                    let instruction = *output.last().ok_or_else(|| IrBuildError::format("pattern_admission_statement_missing", None, 0, 0))?;
                    builder.active_pattern_statements.insert(statement, instruction);
                    builder.stage_pattern_admission_control(super::super::BuildPatternControlRow::Statement(statement), instruction, &scratch)?;
                }
                if stringify!($rows) == "statements"
                    && let Some((binding, original)) = builder.active_callable_bindings.get(&BuildStmtId::new(self.index())).cloned() {
                    let statement = scratch.statements.get(original.row.index()).ok_or_else(|| IrBuildError::format("callable_binding_statement_missing", None, 0, 0))?;
                    let BuildStmtRow::Let { slot, value } = statement else { return Err(IrBuildError::format("callable_binding_statement_changed", None, 0, 0)); };
                    if *slot != original.slot || *value != original.initializer { return Err(IrBuildError::format("callable_binding_allocation_changed", None, 0, 0)); }
                    let initializer = *builder.active_encoded_expressions.get(value).ok_or_else(|| IrBuildError::format("callable_binding_initializer_missing", None, 0, 0))?;
                    let instruction = *output.last().ok_or_else(|| IrBuildError::format("callable_binding_encoded_statement_missing", None, 0, 0))?;
                    let raw = builder.current_owner.ok_or_else(|| IrBuildError::format("callable_binding_owner_missing", None, 0, 0))?;
                    let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| IrBuildError::format("callable_binding_owner_invalid", None, 0, 0))?) };
                    builder.callable_binding_rows.push((binding, original, instruction, initializer, owner));
                }
                if stringify!($rows) == "expressions" {
                    let instruction = *output.last().ok_or_else(|| IrBuildError::format("value_read_encoded_expression_missing", None, 0, 0))?;
                    builder.stage_value_expression_use(instruction, BuildExprId::new(self.index()), &scratch)?;
                }
                if stringify!($rows) == "statements" {
                    let instruction = *output.last().ok_or_else(|| IrBuildError::format("value_binding_encoded_statement_missing", None, 0, 0))?;
                    builder.stage_value_statement_binding(BuildStmtId::new(self.index()), instruction, &scratch)?;
                    builder.stage_mutable_statement(BuildStmtId::new(self.index()), instruction, &scratch)?;
                    builder.stage_mutable_path_statement(BuildStmtId::new(self.index()), instruction, &scratch)?;
                }
                if stringify!($rows) == "statements"
                    && let Some(original) = builder.active_iteration_bindings.get(&BuildStmtId::new(self.index())).cloned() {
                    let statement = scratch.statements.get(original.row.index()).ok_or_else(|| IrBuildError::format("iteration_binding_statement_missing", None, 0, 0))?;
                    let BuildStmtRow::For { slot, iter, .. } = statement else { return Err(IrBuildError::format("iteration_binding_statement_changed", None, 0, 0)); };
                    if *slot != original.slot || *iter != original.iterator { return Err(IrBuildError::format("iteration_binding_operand_changed", None, 0, 0)); }
                    let instruction = *output.last().ok_or_else(|| IrBuildError::format("iteration_binding_encoded_statement_missing", None, 0, 0))?;
                    if builder.store.tags.get(instruction as usize) != Some(&FullTag::StmtFor) { return Err(IrBuildError::format("iteration_binding_encoded_statement_changed", None, 0, 0)); }
                    let words = builder.store.payload(builder.store.data[instruction as usize].range()).map_err(|_| IrBuildError::format("iteration_binding_encoded_payload_missing", None, 0, 0))?;
                    let actual_slot = u32::try_from(original.slot).map_err(|_| IrBuildError::format("iteration_binding_slot_overflow", None, 0, 0))?;
                    if words.len() != 4 || words[0] != actual_slot { return Err(IrBuildError::format("iteration_binding_encoded_operand_changed", None, 0, 0)); }
                    let iterator = words[1];
                    let body = IrBlockId::from_raw(words[2]).ok_or_else(|| IrBuildError::format("iteration_binding_encoded_body_missing", None, 0, 0))?;
                    let raw = builder.current_owner.ok_or_else(|| IrBuildError::format("iteration_binding_owner_missing", None, 0, 0))?;
                    let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| IrBuildError::format("iteration_binding_owner_invalid", None, 0, 0))?) };
                    builder.iteration_binding_rows.push((original, instruction, iterator, actual_slot, body, owner));
                }
                let use_row = match stringify!($rows) {
                    "expressions" => Some(super::super::BuildPatternUseRow::Expression(BuildExprId::new(self.index()))),
                    "ints" => Some(super::super::BuildPatternUseRow::Int(BuildIntId::new(self.index()))),
                    "bools" => Some(super::super::BuildPatternUseRow::Bool(BuildBoolId::new(self.index()))),
                    _ => None,
                };
                if let Some(use_row) = use_row {
                    if let Some(&statement) = scratch.pattern_statement_use_rows.get(&use_row) {
                        let &(original_row, capture) = scratch.pattern_statement_use_origins.get(&statement).ok_or_else(|| IrBuildError::format("pattern_statement_use_original_missing", None, 0, 0))?;
                        if original_row != use_row { return Err(IrBuildError::format("pattern_statement_use_row_changed", None, 0, 0)); }
                        let raw_owner = builder.current_owner.ok_or_else(|| IrBuildError::format("pattern_statement_use_owner_missing", None, 0, 0))?;
                        let owner = if let Some(index) = driver_owner_index(raw_owner) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw_owner).ok_or_else(|| IrBuildError::format("pattern_statement_use_owner_invalid", None, 0, 0))?) };
                        let instruction = *output.last().ok_or_else(|| IrBuildError::format("pattern_statement_use_encoded_row_missing", None, 0, 0))?;
                        builder.generic_pattern_statement_use_rows.push((instruction, statement, capture, owner));
                    }
                }
                if stringify!($rows) == "patterns" {
                    if let Some(&origin) = scratch.pattern_origins.get(&BuildPatternId::new(self.index())) {
                        let raw_owner = builder.current_owner.ok_or_else(|| IrBuildError::format("pattern_without_instruction_owner", None, 0, 0))?;
                        let owner = if let Some(index) = driver_owner_index(raw_owner) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw_owner).ok_or_else(|| IrBuildError::format("pattern_instruction_owner", None, 0, 0))?) };
                        let pattern = *output.last().ok_or_else(|| IrBuildError::format("pattern_encoded_row_missing", None, 0, 0))?;
                        builder.stage_pattern_origin(pattern, origin, owner, &scratch)?;
                    }
                }
                let typed_origin = match stringify!($rows) {
                    "ints" => scratch.int_expression_origins.get(&BuildIntId::new(self.index())).copied(),
                    "bools" => scratch.bool_expression_origins.get(&BuildBoolId::new(self.index())).copied(),
                    _ => None,
                };
                if let Some(origin) = typed_origin {
                    let raw_owner = builder.current_owner.ok_or_else(|| IrBuildError::format("solved_expression_without_owner", None, 0, 0))?;
                    let owner = if let Some(index) = driver_owner_index(raw_owner) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw_owner).ok_or_else(|| IrBuildError::format("solved_expression_owner", None, 0, 0))?) };
                    let instruction = *output.last().ok_or_else(|| IrBuildError::format("solved_expression_instruction", None, 0, 0))?;
                    builder.generic_expression_rows.push((instruction, origin, owner));
                    builder.stage_pattern_expression_use(origin, &scratch)?;
                    builder.stage_iteration_expression_use(instruction, origin, owner, &scratch)?;
                    builder.stage_value_typed_use(instruction, origin, owner, &scratch)?;
                    builder.stage_mutable_use(instruction, origin, owner, &scratch)?;
                    if stringify!($rows) == "bools" && let Some(original) = scratch.folded_literal_comparison.get(&BuildBoolId::new(self.index())) { builder.literal_comparison_rows.push((instruction, original.clone(), owner)); }
                    if stringify!($rows) == "ints" && let Some(original) = scratch.folded_native_receivers.get(&BuildIntId::new(self.index())) { builder.stage_folded_native_receiver(instruction, original.clone(), owner); }
                    if stringify!($rows) == "ints" && let Some(original) = scratch.byte_at_fallback_origins.get(&BuildIntId::new(self.index())) { builder.stage_original_byte_at_fallback(instruction, original.clone(), owner); }
                }
                let statement_use = match stringify!($rows) {
                    "expressions" => scratch.value_statement_reads.get(&BuildExprId::new(self.index())).copied(),
                    "ints" => scratch.int_value_statement_reads.get(&BuildIntId::new(self.index())).copied(),
                    "bools" => scratch.bool_value_statement_reads.get(&BuildBoolId::new(self.index())).copied(),
                    _ => None,
                };
                if let Some((statement, binding)) = statement_use {
                    let instruction = *output.last().ok_or_else(|| IrBuildError::format("value_statement_read_missing", None, 0, 0))?;
                    builder.stage_value_statement_use(instruction, statement, binding, &scratch)?;
                }
                let with_statement_use = match stringify!($rows) {
                    "expressions" => scratch.with_value_statement_reads.get(&BuildExprId::new(self.index())).copied(),
                    "ints" => scratch.int_with_value_statement_reads.get(&BuildIntId::new(self.index())).copied(),
                    "bools" => scratch.bool_with_value_statement_reads.get(&BuildBoolId::new(self.index())).copied(),
                    _ => None,
                };
                if let Some((statement, binding)) = with_statement_use {
                    let instruction = *output.last().ok_or_else(|| IrBuildError::format("with_statement_read_missing", None, 0, 0))?;
                    builder.stage_with_value_statement_use(instruction, statement, binding, &scratch)?;
                }
                let guard_error_statement_use = match stringify!($rows) {
                    "expressions" => scratch.guard_error_statement_reads.get(&BuildExprId::new(self.index())).copied(),
                    "ints" => scratch.int_guard_error_statement_reads.get(&BuildIntId::new(self.index())).copied(),
                    "bools" => scratch.bool_guard_error_statement_reads.get(&BuildBoolId::new(self.index())).copied(),
                    _ => None,
                };
                if let Some((statement, binding)) = guard_error_statement_use {
                    let instruction = *output.last().ok_or_else(|| IrBuildError::format("guard_error_statement_read_missing", None, 0, 0))?;
                    builder.stage_guard_error_statement_use(instruction, statement, binding, &scratch)?;
                }
                let mutable_statement_use = match stringify!($rows) {
                    "expressions" => scratch.mutable_statement_reads.get(&BuildExprId::new(self.index())).copied(),
                    "ints" => scratch.int_mutable_statement_reads.get(&BuildIntId::new(self.index())).copied(),
                    "bools" => scratch.bool_mutable_statement_reads.get(&BuildBoolId::new(self.index())).copied(),
                    _ => None,
                };
                if let Some((statement, binding)) = mutable_statement_use {
                    let raw = builder.current_owner.ok_or_else(|| IrBuildError::format("mutable_statement_read_owner_missing", None, 0, 0))?;
                    let owner = if let Some(index) = driver_owner_index(raw) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw).ok_or_else(|| IrBuildError::format("mutable_statement_read_owner_invalid", None, 0, 0))?) };
                    let instruction = *output.last().ok_or_else(|| IrBuildError::format("mutable_statement_read_missing", None, 0, 0))?;
                    builder.stage_mutable_read(instruction, super::generic::OperationSourceOrigin::Statement(statement), binding, owner, &scratch)?;
                }
                if stringify!($rows) == "expressions" {
                    let expr = BuildExprId::new(self.index());
                    let expression_origin = builder.active_expression_origins.get(&expr).copied();
                    let stage_origin = builder.active_stage_call_origins.get(&expr).copied();
                    if expression_origin.is_some() || stage_origin.is_some() {
                        let raw_owner = builder.current_owner.ok_or_else(|| IrBuildError::format("solved_expression_without_owner", None, 0, 0))?;
                        let owner = if let Some(index) = driver_owner_index(raw_owner) { InstructionOwner::Driver(index as u32) } else { InstructionOwner::Function(IrFunctionId::from_raw(raw_owner).ok_or_else(|| IrBuildError::format("solved_expression_owner", None, 0, 0))?) };
                        let instruction = *output.last().ok_or_else(|| IrBuildError::format("solved_expression_instruction", None, 0, 0))?;
                        if let Some(origin) = expression_origin { builder.generic_expression_rows.push((instruction, origin, owner)); builder.stage_pattern_expression_use(origin, &scratch)?; builder.stage_iteration_expression_use(instruction, origin, owner, &scratch)?; builder.stage_mutable_use(instruction, origin, owner, &scratch)?; }
                        if let Some(origin) = stage_origin { builder.generic_stage_call_rows.push((instruction, origin, owner)); }
                    }
                }
                Ok(())
            }

            fn decode(
                decoder: &FullDecoder<'_>,
                input: &mut FullCursor<'_>,
            ) -> Result<Self, IrVerifyError> {
                <$row>::verify(decoder, input)?;
                Ok(Self(0))
            }

            fn verify(
                decoder: &FullDecoder<'_>,
                input: &mut FullCursor<'_>,
            ) -> Result<(), IrVerifyError> {
                <$row>::verify(decoder, input)
            }
        }
    };
}

impl_build_id_codec!(BuildExprId, expressions, BuildExprRow);
impl_build_id_codec!(BuildStmtId, statements, BuildStmtRow);
impl_build_id_codec!(BuildPatternId, patterns, BuildPatternRow);
impl_build_id_codec!(BuildIntId, ints, BuildIntRow);
impl_build_id_codec!(BuildBoolId, bools, BuildBoolRow);

impl_node_codec! {
    BuildIntRow {
        BuildIntRow::Int(value) => IntInt { value: i64 } => BuildIntRow::Int(value),
        BuildIntRow::Slot(slot) => IntSlot { slot: usize } => BuildIntRow::Slot(slot),
        BuildIntRow::Binary { op, left, right } => IntBinary {
            op: BinaryOp,
            left: BuildIntId,
            right: BuildIntId,
        } => BuildIntRow::Binary { op, left, right },
        BuildIntRow::StrByteLenSlot { slot, span } => IntStrByteLenSlot {
            slot: usize,
            span: Span,
        } => BuildIntRow::StrByteLenSlot { slot, span },
        BuildIntRow::StrCountLinesSlot { slot, span } => IntStrCountLinesSlot {
            slot: usize,
            span: Span,
        } => BuildIntRow::StrCountLinesSlot { slot, span },
        BuildIntRow::StrByteAtSlot {
            slot,
            index,
            default,
            span,
        } => IntStrByteAtSlot {
            slot: usize,
            index: BuildIntId,
            default: Option<BuildIntId>,
            span: Span,
        } => BuildIntRow::StrByteAtSlot {
            slot,
            index,
            default,
            span,
        },
    }
}

impl_node_codec! {
    BuildBoolRow {
        BuildBoolRow::Bool(value) => BoolBool { value: bool } => BuildBoolRow::Bool(value),
        BuildBoolRow::Slot(slot) => BoolSlot { slot: usize } => BuildBoolRow::Slot(slot),
        BuildBoolRow::Not(value) => BoolNot {
            value: BuildBoolId,
        } => BuildBoolRow::Not(value),
        BuildBoolRow::And(left, right) => BoolAnd {
            left: BuildBoolId,
            right: BuildBoolId,
        } => BuildBoolRow::And(left, right),
        BuildBoolRow::Or(left, right) => BoolOr {
            left: BuildBoolId,
            right: BuildBoolId,
        } => BuildBoolRow::Or(left, right),
        BuildBoolRow::IntCompare { op, left, right } => BoolIntCompare {
            op: BinaryOp,
            left: BuildIntId,
            right: BuildIntId,
        } => BuildBoolRow::IntCompare { op, left, right },
        BuildBoolRow::StrPredicateSlot {
            slot,
            predicate,
            needle,
            span,
        } => BoolStrPredicateSlot {
            slot: usize,
            predicate: LoweredStrPredicate,
            needle: Arc<[u8]>,
            span: Span,
        } => BuildBoolRow::StrPredicateSlot {
            slot,
            predicate,
            needle,
            span,
        },
        BuildBoolRow::ContainsSlot { slot, needle, span } => BoolContainsSlot {
            slot: usize,
            needle: LoweredValue,
            span: Span,
        } => BuildBoolRow::ContainsSlot { slot, needle, span },
        BuildBoolRow::StrContainsSlot { slot, needle, span } => BoolStrContainsSlot {
            slot: usize,
            needle: Arc<str>,
            span: Span,
        } => BuildBoolRow::StrContainsSlot { slot, needle, span },
        BuildBoolRow::TrimEmptySlot { slot, span } => BoolTrimEmptySlot {
            slot: usize,
            span: Span,
        } => BuildBoolRow::TrimEmptySlot { slot, span },
        BuildBoolRow::TrimStrPredicateSlot {
            slot,
            predicate,
            needle,
            span,
        } => BoolTrimStrPredicateSlot {
            slot: usize,
            predicate: LoweredStrPredicate,
            needle: Arc<[u8]>,
            span: Span,
        } => BuildBoolRow::TrimStrPredicateSlot {
            slot,
            predicate,
            needle,
            span,
        },
        BuildBoolRow::LiteralCompareSlot { op, slot, value } => BoolLiteralCompareSlot {
            op: BinaryOp,
            slot: usize,
            value: LoweredValue,
        } => BuildBoolRow::LiteralCompareSlot { op, slot, value },
    }
}

pub(in crate::runtime::eval) const BLOCK_LIST: u8 = 0;
pub(in crate::runtime::eval) const BLOCK_STATEMENTS: u8 = 1;
const BLOCK_FUNCTION_BODY: u8 = 1 << 1;
const BLOCK_SEQUENCE_KIND_MASK: u8 = 1;

impl_vec_codec!(BuildStmtId, BLOCK_STATEMENTS);
impl_vec_codec!(BuildExprId, BLOCK_LIST);
impl_vec_codec!((usize, BuildExprId), BLOCK_LIST);
impl_vec_codec!(Option<BuildExprId>, BLOCK_LIST);
impl_vec_codec!((bool, BuildExprId, Span), BLOCK_LIST);
impl_vec_codec!(BuildPatternId, BLOCK_LIST);
impl_vec_codec!(LoweredPipelineStage, BLOCK_LIST);
impl_vec_codec!(LoweredValue, BLOCK_LIST);
impl_vec_codec!(LoweredFmtPart, BLOCK_LIST);
impl_vec_codec!(LoweredRecordEntry, BLOCK_LIST);
impl_vec_codec!((Option<BuildExprId>, BuildExprId, Span), BLOCK_LIST);
impl_vec_codec!(LoweredCallArg, BLOCK_LIST);
impl_vec_codec!(LoweredRunArg, BLOCK_LIST);
impl_vec_codec!(LoweredRunEnv, BLOCK_LIST);
impl_vec_codec!(LoweredRunRedirection, BLOCK_LIST);
impl_vec_codec!(LoweredRunPipelineSegment, BLOCK_LIST);
impl_vec_codec!(LoweredProcessCommandBuilderEntry, BLOCK_LIST);
impl_vec_codec!(ScanCheck, BLOCK_LIST);
impl_vec_codec!(LoweredModuleExport, BLOCK_LIST);
impl_vec_codec!(LoweredTopLevelSlot, BLOCK_LIST);
impl_vec_codec!(String, BLOCK_LIST);
impl_vec_codec!(Name, BLOCK_LIST);
impl_vec_codec!((Vec<Name>, BuildExprId, Span), BLOCK_LIST);

impl FullCodec for LoweredRecordUpdates {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.0.encode(builder, output)
    }
    fn decode(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<Self, IrVerifyError> {
        let updates = Vec::<(Vec<Name>, BuildExprId, Span)>::decode(decoder, input)?;
        for (index, (path, _, _)) in updates.iter().enumerate() {
            if path.is_empty() || updates[..index].iter().any(|(other, _, _)| path.starts_with(other) || other.starts_with(path)) {
                return Err(IrVerifyError::new("record update paths must be nonempty and disjoint"));
            }
        }
        Ok(Self(updates))
    }
}

impl_vec_codec!((Name, usize), BLOCK_LIST);
impl_vec_codec!((Name, BuildPatternId), BLOCK_LIST);
impl_vec_codec!((Name, LoweredValue), BLOCK_LIST);
impl_vec_codec!((Arc<str>, BuildExprId), BLOCK_LIST);
impl_vec_codec!((Arc<str>, Vec<BuildStmtId>), BLOCK_LIST);
impl_vec_codec!(usize, BLOCK_LIST);
impl_vec_codec!((BuildExprId, BuildExprId), BLOCK_LIST);
impl_vec_codec!((BuildExprId, BuildExprId, Vec<usize>), BLOCK_LIST);
impl_vec_codec!((BuildExprId, Vec<BuildStmtId>, Vec<usize>), BLOCK_LIST);
impl_vec_codec!((BuildExprId, Vec<BuildStmtId>), BLOCK_LIST);
impl_vec_codec!((BuildBoolId, Vec<BuildStmtId>), BLOCK_LIST);
impl_vec_codec!(
    (BuildPatternId, Option<BuildExprId>, BuildExprId),
    BLOCK_LIST
);
impl_vec_codec!(
    (BuildPatternId, Option<BuildExprId>, Vec<BuildStmtId>),
    BLOCK_LIST
);

impl<A> FullCodec for SmallVec<A>
where
    A: smallvec::Array,
    A::Item: FullCodec,
{
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        output.push(
            u32::try_from(self.len())
                .map_err(|_| IrBuildError::format("smallvec_overflow", None, 0, 0))?,
        );
        for value in self {
            value.encode(builder, output)?;
        }
        Ok(())
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        let len = input.raw()? as usize;
        let mut values = SmallVec::with_capacity(len);
        for _ in 0..len {
            values.push(A::Item::decode(decoder, input)?);
        }
        Ok(values)
    }

    fn verify(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<(), IrVerifyError> {
        let len = input.raw()? as usize;
        for _ in 0..len {
            A::Item::verify(decoder, input)?;
        }
        Ok(())
    }
}

macro_rules! impl_fx_map_codec {
    ($value:ty) => {
        impl FullCodec for FxHashMap<Arc<str>, $value> {
            fn encode(
                &self,
                builder: &mut FullBuilder,
                output: &mut Vec<u32>,
            ) -> Result<(), IrBuildError> {
                let mut values = self.iter().collect::<Vec<_>>();
                values.sort_by(|left, right| left.0.cmp(right.0));
                output.push(
                    u32::try_from(values.len())
                        .map_err(|_| IrBuildError::format("map_overflow", None, 0, 0))?,
                );
                for (key, value) in values {
                    key.encode(builder, output)?;
                    value.encode(builder, output)?;
                }
                Ok(())
            }

            fn decode(
                decoder: &FullDecoder<'_>,
                input: &mut FullCursor<'_>,
            ) -> Result<Self, IrVerifyError> {
                let len = input.raw()? as usize;
                let mut values = FxHashMap::default();
                for _ in 0..len {
                    let key = Arc::<str>::decode(decoder, input)?;
                    let value = <$value>::decode(decoder, input)?;
                    if values.insert(key, value).is_some() {
                        return Err(IrVerifyError::new("map payload contains a duplicate key"));
                    }
                }
                Ok(values)
            }
        }
    };
}

impl_fx_map_codec!(BuildExprId);
impl_fx_map_codec!(Vec<BuildStmtId>);

impl FullCodec for LoweredCompTarget {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        match self {
            Self::Discard => { output.push(2); Ok(()) }
            Self::Slot(slot) => {
                output.push(0);
                slot.encode(builder, output)
            }
            Self::Record { fields } => {
                output.push(1);
                fields.encode(builder, output)
            }
        }
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        match input.raw()? {
            2 => Ok(Self::Discard),
            0 => Ok(Self::Slot(usize::decode(decoder, input)?)),
            1 => Ok(Self::Record {
                fields: SmallVec::decode(decoder, input)?,
            }),
            _ => Err(IrVerifyError::new("comprehension target tag is invalid")),
        }
    }
}

impl FullCodec for LoweredAssignPath {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.0.encode(builder, output)
    }
    fn decode(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<Self, IrVerifyError> {
        let steps = Vec::<LoweredAssignStep>::decode(decoder, input)?;
        if steps.is_empty() { return Err(IrVerifyError::new("assignment path must select an element")); }
        Ok(Self(steps))
    }
}
impl FullCodec for LoweredAssignStep {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        match self {
            Self::Field(name) => { output.push(0); name.encode(builder, output) }
            Self::Index(expr) => { output.push(1); expr.encode(builder, output) }
        }
    }
    fn decode(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<Self, IrVerifyError> {
        match input.raw()? {
            0 => Ok(Self::Field(Name::decode(decoder, input)?)),
            1 => Ok(Self::Index(BuildExprId::decode(decoder, input)?)),
            _ => Err(IrVerifyError::new("assignment path step tag is invalid")),
        }
    }
}
impl_vec_codec!(LoweredAssignStep, BLOCK_LIST);

impl FullCodec for LoweredCompQualifiers {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.0.encode(builder, output)
    }

    fn decode(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<Self, IrVerifyError> {
        let qualifiers = Vec::<LoweredCompQualifier>::decode(decoder, input)?;
        if !matches!(qualifiers.first(), Some(LoweredCompQualifier::For { .. })) {
            return Err(IrVerifyError::new("comprehension qualifiers must start with for"));
        }
        Ok(Self(qualifiers))
    }
}

impl FullCodec for LoweredCompQualifier {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        match self {
            Self::For { target, iter, span } => { output.push(0); target.encode(builder, output)?; iter.encode(builder, output)?; span.encode(builder, output) }
            Self::If { condition, span } => { output.push(1); condition.encode(builder, output)?; span.encode(builder, output) }
        }
    }
    fn decode(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<Self, IrVerifyError> {
        match input.raw()? {
            0 => Ok(Self::For { target: Box::decode(decoder, input)?, iter: BuildExprId::decode(decoder, input)?, span: Span::decode(decoder, input)? }),
            1 => Ok(Self::If { condition: BuildExprId::decode(decoder, input)?, span: Span::decode(decoder, input)? }),
            _ => Err(IrVerifyError::new("comprehension qualifier tag is invalid")),
        }
    }
}
impl_vec_codec!(LoweredCompQualifier, BLOCK_LIST);

impl FullCodec for LoweredRecordEntry {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        match self {
            Self::Field(name, value) => {
                output.push(0);
                name.encode(builder, output)?;
                value.encode(builder, output)
            }
            Self::Spread(value) => {
                output.push(1);
                value.encode(builder, output)
            }
        }
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        match input.raw()? {
            0 => Ok(Self::Field(
                Name::decode(decoder, input)?,
                BuildExprId::decode(decoder, input)?,
            )),
            1 => Ok(Self::Spread(BuildExprId::decode(decoder, input)?)),
            _ => Err(IrVerifyError::new("record entry tag is invalid")),
        }
    }
}

impl FullCodec for LoweredCallArg {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        match self {
            Self::Default(slot) => {
                // This is a callable parameter index, not a caller frame slot.
                output.push(2);
                u32::try_from(*slot).map_err(|_| IrBuildError::format("argument_slot_overflow", None, 0, 0))?.encode(builder, output)
            }
            Self::Single(value) => {
                output.push(0);
                value.encode(builder, output)
            }
            Self::Splice(value) => {
                output.push(1);
                value.encode(builder, output)
            }
        }
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        match input.raw()? {
            0 => Ok(Self::Single(BuildExprId::decode(decoder, input)?)),
            1 => Ok(Self::Splice(BuildExprId::decode(decoder, input)?)),
            2 => Ok(Self::Default(u32::decode(decoder, input)? as usize)),
            _ => Err(IrVerifyError::new("call argument tag is invalid")),
        }
    }
}

impl FullCodec for LoweredFmtPart {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        match self {
            Self::Text(value) => {
                output.push(0);
                value.encode(builder, output)
            }
            Self::Expr(value, span, format) => {
                output.push(1);
                value.encode(builder, output)?;
                span.encode(builder, output)?;
                format.encode(builder, output)
            }
        }
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        match input.raw()? {
            0 => Ok(Self::Text(Arc::<str>::decode(decoder, input)?)),
            1 => Ok(Self::Expr(
                BuildExprId::decode(decoder, input)?,
                Span::decode(decoder, input)?,
                Option::<FormatSpec>::decode(decoder, input)?,
            )),
            _ => Err(IrVerifyError::new("format part tag is invalid")),
        }
    }
}

impl FullCodec for LoweredRunArgKind {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        let (tag, value) = match self {
            Self::Single(value) => (0, value),
            Self::SingleOrSplice(value) => (1, value),
            Self::Splice(value) => (2, value),
        };
        output.push(tag);
        value.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        let tag = input.raw()?;
        let value = BuildExprId::decode(decoder, input)?;
        match tag {
            0 => Ok(Self::Single(value)),
            1 => Ok(Self::SingleOrSplice(value)),
            2 => Ok(Self::Splice(value)),
            _ => Err(IrVerifyError::new("run argument tag is invalid")),
        }
    }
}

impl FullCodec for LoweredRunArg {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.kind.encode(builder, output)?;
        self.span.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Self {
            kind: LoweredRunArgKind::decode(decoder, input)?,
            span: Span::decode(decoder, input)?,
        })
    }
}

impl FullCodec for LoweredRunEnv {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.name.encode(builder, output)?;
        self.value.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Self {
            name: Name::decode(decoder, input)?,
            value: LoweredRunArg::decode(decoder, input)?,
        })
    }
}

impl FullCodec for LoweredRunRedirection {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.kind.encode(builder, output)?;
        self.target.encode(builder, output)?;
        self.span.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Self {
            kind: RedirectionKind::decode(decoder, input)?,
            target: LoweredRunArg::decode(decoder, input)?,
            span: Span::decode(decoder, input)?,
        })
    }
}

impl FullCodec for LoweredRunPipelineSegment {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.kind.encode(builder, output)?;
        self.target.encode(builder, output)?;
        self.args.encode(builder, output)?;
        self.env.encode(builder, output)?;
        self.redirections.encode(builder, output)?;
        self.timeout.encode(builder, output)?;
        self.cpu_max.encode(builder, output)?;
        self.accept.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Self {
            kind: RunKind::decode(decoder, input)?,
            target: LoweredRunArg::decode(decoder, input)?,
            args: Vec::decode(decoder, input)?,
            env: Vec::decode(decoder, input)?,
            redirections: Vec::decode(decoder, input)?,
            timeout: Option::decode(decoder, input)?,
            cpu_max: Option::decode(decoder, input)?,
            accept: Option::decode(decoder, input)?,
        })
    }
}

impl FullCodec for ScanCondition {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        match self {
            Self::TrimEmpty => {
                output.push(0);
                Ok(())
            }
            Self::TrimStartsWith(value) => {
                output.push(1);
                value.encode(builder, output)
            }
            Self::StartsWith(value) => {
                output.push(2);
                value.encode(builder, output)
            }
        }
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        match input.raw()? {
            0 => Ok(Self::TrimEmpty),
            1 => Ok(Self::TrimStartsWith(Vec::<u8>::decode(decoder, input)?)),
            2 => Ok(Self::StartsWith(Vec::<u8>::decode(decoder, input)?)),
            _ => Err(IrVerifyError::new("scan condition tag is invalid")),
        }
    }
}

impl FullCodec for ScanCheck {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.condition.encode(builder, output)?;
        self.counter_slot.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Self {
            condition: ScanCondition::decode(decoder, input)?,
            counter_slot: usize::decode(decoder, input)?,
        })
    }
}

impl FullCodec for ScanBytes {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.line_slot.encode(builder, output)?;
        self.block_depth_slot.encode(builder, output)?;
        self.code_seen_slot.encode(builder, output)?;
        self.comment_seen_slot.encode(builder, output)?;
        self.in_string_slot.encode(builder, output)?;
        self.string_delim_slot.encode(builder, output)?;
        self.escaped_slot.encode(builder, output)?;
        self.nested.encode(builder, output)?;
        self.span.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Self {
            line_slot: usize::decode(decoder, input)?,
            block_depth_slot: usize::decode(decoder, input)?,
            code_seen_slot: usize::decode(decoder, input)?,
            comment_seen_slot: usize::decode(decoder, input)?,
            in_string_slot: usize::decode(decoder, input)?,
            string_delim_slot: usize::decode(decoder, input)?,
            escaped_slot: usize::decode(decoder, input)?,
            nested: bool::decode(decoder, input)?,
            span: Span::decode(decoder, input)?,
        })
    }
}

impl FullCodec for LoweredProcessCommandArgv {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.target.encode(builder, output)?;
        self.argv.encode(builder, output)?;
        self.cwd.encode(builder, output)?;
        self.env.encode(builder, output)?;
        self.stdin.encode(builder, output)?;
        self.stdout.encode(builder, output)?;
        self.stderr.encode(builder, output)?;
        self.stdout_append.encode(builder, output)?;
        self.stderr_append.encode(builder, output)?;
        self.timeout.encode(builder, output)?;
        self.detach.encode(builder, output)?;
        self.new_session.encode(builder, output)?;
        self.ignore_hup.encode(builder, output)?;
        self.cpu_max.encode(builder, output)?;
        self.accept.encode(builder, output)?;
        self.span.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Self {
            target: BuildExprId::decode(decoder, input)?,
            argv: BuildExprId::decode(decoder, input)?,
            cwd: Option::decode(decoder, input)?,
            env: Option::decode(decoder, input)?,
            stdin: Option::decode(decoder, input)?,
            stdout: Option::decode(decoder, input)?,
            stderr: Option::decode(decoder, input)?,
            stdout_append: Option::decode(decoder, input)?,
            stderr_append: Option::decode(decoder, input)?,
            timeout: Option::decode(decoder, input)?,
            detach: Option::decode(decoder, input)?,
            new_session: Option::decode(decoder, input)?,
            ignore_hup: Option::decode(decoder, input)?,
            cpu_max: Option::decode(decoder, input)?,
            accept: Option::decode(decoder, input)?,
            span: Span::decode(decoder, input)?,
        })
    }
}

impl FullCodec for LoweredRunCapture {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.kind.encode(builder, output)?;
        self.target.encode(builder, output)?;
        self.args.encode(builder, output)?;
        self.env.encode(builder, output)?;
        self.redirections.encode(builder, output)?;
        self.timeout.encode(builder, output)?;
        self.cpu_max.encode(builder, output)?;
        self.accept.encode(builder, output)?;
        self.propagate.encode(builder, output)?;
        self.assert_success.encode(builder, output)?;
        self.span.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Self {
            kind: RunKind::decode(decoder, input)?,
            target: Box::decode(decoder, input)?,
            args: Vec::decode(decoder, input)?,
            env: Vec::decode(decoder, input)?,
            redirections: Vec::decode(decoder, input)?,
            timeout: Option::decode(decoder, input)?,
            cpu_max: Option::decode(decoder, input)?,
            accept: Option::decode(decoder, input)?,
            propagate: bool::decode(decoder, input)?,
            assert_success: bool::decode(decoder, input)?,
            span: Span::decode(decoder, input)?,
        })
    }
}

impl FullCodec for LoweredSpawnRun {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.target.encode(builder, output)?;
        self.args.encode(builder, output)?;
        self.env.encode(builder, output)?;
        self.redirections.encode(builder, output)?;
        self.timeout.encode(builder, output)?;
        self.cpu_max.encode(builder, output)?;
        self.accept.encode(builder, output)?;
        self.span.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Self {
            target: Box::decode(decoder, input)?,
            args: Vec::decode(decoder, input)?,
            env: Vec::decode(decoder, input)?,
            redirections: Vec::decode(decoder, input)?,
            timeout: Option::decode(decoder, input)?,
            cpu_max: Option::decode(decoder, input)?,
            accept: Option::decode(decoder, input)?,
            span: Span::decode(decoder, input)?,
        })
    }
}

impl FullCodec for LoweredProcessCommandBuilderEntry {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        match self {
            Self::Field { name, value, span } => {
                output.push(0);
                name.encode(builder, output)?;
                value.encode(builder, output)?;
                span.encode(builder, output)
            }
            Self::Run {
                target,
                args,
                env,
                timeout,
                cpu_max,
                accept,
                span,
            } => {
                output.push(1);
                target.encode(builder, output)?;
                args.encode(builder, output)?;
                env.encode(builder, output)?;
                timeout.encode(builder, output)?;
                cpu_max.encode(builder, output)?;
                accept.encode(builder, output)?;
                span.encode(builder, output)
            }
        }
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        match input.raw()? {
            0 => Ok(Self::Field {
                name: Name::decode(decoder, input)?,
                value: BuildExprId::decode(decoder, input)?,
                span: Span::decode(decoder, input)?,
            }),
            1 => Ok(Self::Run {
                target: LoweredRunArg::decode(decoder, input)?,
                args: Vec::decode(decoder, input)?,
                env: Vec::decode(decoder, input)?,
                timeout: Option::decode(decoder, input)?,
                cpu_max: Option::decode(decoder, input)?,
                accept: Option::decode(decoder, input)?,
                span: Span::decode(decoder, input)?,
            }),
            _ => Err(IrVerifyError::new(
                "process command builder entry tag is invalid",
            )),
        }
    }
}

impl FullCodec for LoweredErrorExpr {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        match self {
            Self::Simple { kind, message } => {
                output.push(0);
                kind.encode(builder, output)?;
                message.encode(builder, output)
            }
            Self::Structured {
                family,
                variant,
                fields,
                facets,
            } => {
                output.push(1);
                family.encode(builder, output)?;
                variant.encode(builder, output)?;
                fields.encode(builder, output)?;
                facets.encode(builder, output)
            }
        }
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        match input.raw()? {
            0 => Ok(Self::Simple {
                kind: String::decode(decoder, input)?,
                message: String::decode(decoder, input)?,
            }),
            1 => Ok(Self::Structured {
                family: String::decode(decoder, input)?,
                variant: String::decode(decoder, input)?,
                fields: Vec::decode(decoder, input)?,
                facets: Vec::decode(decoder, input)?,
            }),
            _ => Err(IrVerifyError::new("error expression tag is invalid")),
        }
    }
}

impl FullCodec for BuildPatternRow {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        let mut payload = builder.take_payload();
        let tag = match self {
            Self::TagType { type_name, variants } => { type_name.encode(builder, &mut payload)?; variants.encode(builder, &mut payload)?; FullPatternTag::TagType }
            Self::RecordTest { fields } => {
                fields.encode(builder, &mut payload)?;
                FullPatternTag::RecordTest
            }
            Self::ResultTest { ok, inner } => {
                ok.encode(builder, &mut payload)?;
                inner.encode(builder, &mut payload)?;
                FullPatternTag::ResultTest
            }
            Self::TagTest { type_name, name, fields } => {
                type_name.encode(builder, &mut payload)?;
                name.encode(builder, &mut payload)?;
                fields.encode(builder, &mut payload)?;
                FullPatternTag::TagTest
            }
            Self::ErrorTest { family, variant, fields } => {
                family.encode(builder, &mut payload)?;
                variant.encode(builder, &mut payload)?;
                fields.encode(builder, &mut payload)?;
                FullPatternTag::ErrorTest
            }
            Self::List { elements, rest } => {
                elements.encode(builder, &mut payload)?;
                rest.encode(builder, &mut payload)?;
                FullPatternTag::List
            }
            Self::Alias { pattern, slot } => {
                pattern.encode(builder, &mut payload)?;
                slot.encode(builder, &mut payload)?;
                FullPatternTag::Alias
            }
            Self::Alternation { patterns } => {
                patterns.encode(builder, &mut payload)?;
                FullPatternTag::Alternation
            }
            Self::Wildcard => FullPatternTag::Wildcard,
            Self::Bind { slot } => {
                slot.encode(builder, &mut payload)?;
                FullPatternTag::Bind
            }
            Self::Type { ty, slot } => {
                ty.encode(builder, &mut payload)?;
                slot.encode(builder, &mut payload)?;
                FullPatternTag::Type
            }
            Self::Literal(value) => {
                value.encode(builder, &mut payload)?;
                FullPatternTag::Literal
            }
            Self::ResultOk { slot, unit_only } => {
                slot.encode(builder, &mut payload)?;
                unit_only.encode(builder, &mut payload)?;
                FullPatternTag::ResultOk
            }
            Self::ResultErr { slot, unit_only } => {
                slot.encode(builder, &mut payload)?;
                unit_only.encode(builder, &mut payload)?;
                FullPatternTag::ResultErr
            }
            Self::ErrorVariant {
                family,
                variant,
                fields,
                result_wrapped,
            } => {
                family.encode(builder, &mut payload)?;
                variant.encode(builder, &mut payload)?;
                fields.encode(builder, &mut payload)?;
                result_wrapped.encode(builder, &mut payload)?;
                FullPatternTag::ErrorVariant
            }
            Self::Facet {
                facet,
                result_wrapped,
            } => {
                facet.encode(builder, &mut payload)?;
                result_wrapped.encode(builder, &mut payload)?;
                FullPatternTag::Facet
            }
            Self::Tag { type_name, name, slots } => {
                type_name.encode(builder, &mut payload)?;
                name.encode(builder, &mut payload)?;
                slots.encode(builder, &mut payload)?;
                FullPatternTag::Tag
            }
        };
        let pattern = builder.push_pattern(tag, &payload);
        builder.recycle_payload(payload);
        output.push(pattern?);
        Ok(())
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        let index = input.raw()? as usize;
        let tag = decoder
            .store
            .patterns
            .get(index)
            .copied()
            .ok_or_else(|| IrVerifyError::new("pattern id is out of bounds"))?;
        let data = decoder.store.pattern_data[index];
        let mut payload = FullCursor::new(decoder.store.payload(data.range())?);
        let pattern = match tag {
            FullPatternTag::TagType => Self::TagType { type_name: Name::decode(decoder, &mut payload)?, variants: Vec::decode(decoder, &mut payload)? },
            FullPatternTag::RecordTest => Self::RecordTest { fields: Box::decode(decoder, &mut payload)? },
            FullPatternTag::ResultTest => Self::ResultTest { ok: bool::decode(decoder, &mut payload)?, inner: BuildPatternId::decode(decoder, &mut payload)? },
            FullPatternTag::TagTest => Self::TagTest { type_name: Name::decode(decoder, &mut payload)?, name: Name::decode(decoder, &mut payload)?, fields: Vec::decode(decoder, &mut payload)? },
            FullPatternTag::ErrorTest => Self::ErrorTest { family: Name::decode(decoder, &mut payload)?, variant: Name::decode(decoder, &mut payload)?, fields: Box::decode(decoder, &mut payload)? },
            FullPatternTag::List => Self::List {
                elements: Vec::decode(decoder, &mut payload)?,
                rest: Option::decode(decoder, &mut payload)?,
            },
            FullPatternTag::Alias => Self::Alias { pattern: BuildPatternId::decode(decoder, &mut payload)?, slot: usize::decode(decoder, &mut payload)? },
            FullPatternTag::Alternation => Self::Alternation { patterns: Vec::decode(decoder, &mut payload)? },
            FullPatternTag::Wildcard => Self::Wildcard,
            FullPatternTag::Bind => Self::Bind {
                slot: usize::decode(decoder, &mut payload)?,
            },
            FullPatternTag::Type => Self::Type {
                ty: Type::decode(decoder, &mut payload)?,
                slot: Option::decode(decoder, &mut payload)?,
            },
            FullPatternTag::Literal => Self::Literal(LoweredValue::decode(decoder, &mut payload)?),
            FullPatternTag::ResultOk => Self::ResultOk {
                slot: Option::decode(decoder, &mut payload)?,
                unit_only: bool::decode(decoder, &mut payload)?,
            },
            FullPatternTag::ResultErr => Self::ResultErr {
                slot: Option::decode(decoder, &mut payload)?,
                unit_only: bool::decode(decoder, &mut payload)?,
            },
            FullPatternTag::ErrorVariant => Self::ErrorVariant {
                family: Name::decode(decoder, &mut payload)?,
                variant: Name::decode(decoder, &mut payload)?,
                fields: Box::decode(decoder, &mut payload)?,
                result_wrapped: bool::decode(decoder, &mut payload)?,
            },
            FullPatternTag::Facet => Self::Facet {
                facet: Name::decode(decoder, &mut payload)?,
                result_wrapped: bool::decode(decoder, &mut payload)?,
            },
            FullPatternTag::Tag => Self::Tag {
                type_name: Name::decode(decoder, &mut payload)?,
                name: Name::decode(decoder, &mut payload)?,
                slots: SmallVec::decode(decoder, &mut payload)?,
            },
        };
        payload.finish()?;
        Ok(pattern)
    }

    fn verify(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<(), IrVerifyError> {
        let index = input.raw()? as usize;
        let parent = decoder.pattern_ceiling.get();
        // Child patterns are committed first. Descending IDs rule out cycles
        // while allowing immutable children to be shared by multiple parents.
        if index >= parent {
            return Err(IrVerifyError::new("nested pattern must precede its parent"));
        }
        decoder.pattern_ceiling.set(index);
        let tag = decoder
            .store
            .patterns
            .get(index)
            .copied()
            .ok_or_else(|| IrVerifyError::new("pattern id is out of bounds"))?;
        let data = decoder.store.pattern_data[index];
        let mut payload = FullCursor::new(decoder.store.payload(data.range())?);
        match tag {
            FullPatternTag::TagType => { Name::verify(decoder, &mut payload)?; Vec::<Name>::verify(decoder, &mut payload)?; },
            FullPatternTag::RecordTest => Box::<Vec<(Name, BuildPatternId)>>::verify(decoder, &mut payload)?,
            FullPatternTag::ResultTest => {
                bool::verify(decoder, &mut payload)?;
                BuildPatternId::verify(decoder, &mut payload)?;
            }
            FullPatternTag::TagTest => {
                Name::verify(decoder, &mut payload)?;
                Name::verify(decoder, &mut payload)?;
                Vec::<BuildPatternId>::verify(decoder, &mut payload)?;
            }
            FullPatternTag::ErrorTest => {
                Name::verify(decoder, &mut payload)?;
                Name::verify(decoder, &mut payload)?;
                Box::<Vec<(Name, BuildPatternId)>>::verify(decoder, &mut payload)?;
            }
            FullPatternTag::List => {
                Vec::<BuildPatternId>::verify(decoder, &mut payload)?;
                let mut rest_payload = payload;
                if bool::decode(decoder, &mut rest_payload)? {
                    let rest = rest_payload.raw()? as usize;
                    if !matches!(decoder.store.patterns.get(rest), Some(FullPatternTag::Wildcard | FullPatternTag::Bind)) {
                        return Err(IrVerifyError::new("list rest must be a wildcard or binding"));
                    }
                }
                Option::<BuildPatternId>::verify(decoder, &mut payload)?;
            }
            FullPatternTag::Alias => {
                BuildPatternId::verify(decoder, &mut payload)?;
                usize::verify(decoder, &mut payload)?;
                decoder.pattern_capture_slots(index)?;
            }
            FullPatternTag::Alternation => {
                let mut alternatives = payload;
                Vec::<BuildPatternId>::verify(decoder, &mut payload)?;
                let mut children = decoder.pattern_list_cursor(&mut alternatives)?;
                let count = children.raw()? as usize;
                if count < 2 { return Err(IrVerifyError::new("alternative pattern requires at least two choices")); }
                let mut common = None;
                for _ in 0..count {
                    let child = children.raw()? as usize;
                    let captures = decoder.pattern_capture_slots(child)?;
                    if common.as_ref().is_some_and(|common| common != &captures) {
                        return Err(IrVerifyError::new("pattern alternatives must publish identical capture slots"));
                    }
                    common = Some(captures);
                }
                children.finish()?;
            }
            FullPatternTag::Wildcard => {}
            FullPatternTag::Bind => usize::verify(decoder, &mut payload)?,
            FullPatternTag::Type => {
                Type::verify(decoder, &mut payload)?;
                Option::<usize>::verify(decoder, &mut payload)?;
            }
            FullPatternTag::Literal => LoweredValue::verify(decoder, &mut payload)?,
            FullPatternTag::ResultOk | FullPatternTag::ResultErr => {
                Option::<usize>::verify(decoder, &mut payload)?;
                bool::verify(decoder, &mut payload)?;
            }
            FullPatternTag::ErrorVariant => {
                Name::verify(decoder, &mut payload)?;
                Name::verify(decoder, &mut payload)?;
                Box::<LoweredErrorPatternFields>::verify(decoder, &mut payload)?;
                bool::verify(decoder, &mut payload)?;
            }
            FullPatternTag::Facet => {
                Name::verify(decoder, &mut payload)?;
                bool::verify(decoder, &mut payload)?;
            }
            FullPatternTag::Tag => {
                Name::verify(decoder, &mut payload)?;
                Name::verify(decoder, &mut payload)?;
                BuildPatternIdSlots::verify(decoder, &mut payload)?;
            }
        }
        payload.finish()?;
        decoder.pattern_ceiling.set(parent);
        Ok(())
    }
}

macro_rules! impl_stage_codec {
    (
        $(
            $pattern:pat => $tag:ident {
                $($field:ident : $field_ty:ty),* $(,)?
            } => $construct:expr
        ),* $(,)?
    ) => {
        impl FullCodec for LoweredPipelineStage {
            fn encode(
                &self,
                builder: &mut FullBuilder,
                output: &mut Vec<u32>,
            ) -> Result<(), IrBuildError> {
                let (tag, payload) = match self {
                    $(
                        $pattern => {
                            #[allow(unused_mut)]
                            let mut payload = builder.take_payload();
                            $(
                                $field.encode(builder, &mut payload)?;
                            )*
                            (FullStageTag::$tag, payload)
                        }
                    ),*
                };
                let stage = builder.push_stage(tag, &payload);
                builder.recycle_payload(payload);
                output.push(stage?);
                Ok(())
            }

            fn decode(
                decoder: &FullDecoder<'_>,
                input: &mut FullCursor<'_>,
            ) -> Result<Self, IrVerifyError> {
                let index = input.raw()? as usize;
                let tag = decoder
                    .store
                    .stages
                    .get(index)
                    .copied()
                    .ok_or_else(|| IrVerifyError::new("pipeline stage id is out of bounds"))?;
                let data = decoder.store.stage_data[index];
                let mut payload = FullCursor::new(decoder.store.payload(data.range())?);
                let stage = match tag {
                    $(
                        FullStageTag::$tag => {
                            $(
                                let $field = <$field_ty>::decode(decoder, &mut payload)?;
                            )*
                            $construct
                        }
                    ),*
                };
                payload.finish()?;
                Ok(stage)
            }

            fn verify(
                decoder: &FullDecoder<'_>,
                input: &mut FullCursor<'_>,
            ) -> Result<(), IrVerifyError> {
                let index = input.raw()? as usize;
                let tag = decoder
                    .store
                    .stages
                    .get(index)
                    .copied()
                    .ok_or_else(|| IrVerifyError::new("pipeline stage id is out of bounds"))?;
                let data = decoder.store.stage_data[index];
                let mut payload = FullCursor::new(decoder.store.payload(data.range())?);
                match tag {
                    $(
                        FullStageTag::$tag => {
                            $(
                                <$field_ty>::verify(decoder, &mut payload)?;
                            )*
                        }
                    ),*
                }
                payload.finish()
            }
        }
    };
}

impl_stage_codec! {
    LoweredPipelineStage::TextLines => TextLines {} => LoweredPipelineStage::TextLines,
    LoweredPipelineStage::JsonLines => JsonLines {} => LoweredPipelineStage::JsonLines,
    LoweredPipelineStage::Where { slot, predicate } => Where {
        slot: usize,
        predicate: BuildExprId,
    } => LoweredPipelineStage::Where { slot, predicate },
    LoweredPipelineStage::WhereBlock { slot, body, value } => WhereBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    } => LoweredPipelineStage::WhereBlock { slot, body, value },
    LoweredPipelineStage::Map { slot, value } => Map {
        slot: usize,
        value: BuildExprId,
    } => LoweredPipelineStage::Map { slot, value },
    LoweredPipelineStage::MapBlock { slot, body, value } => MapBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    } => LoweredPipelineStage::MapBlock { slot, body, value },
    LoweredPipelineStage::FlatMap { slot, value } => FlatMap {
        slot: usize,
        value: BuildExprId,
    } => LoweredPipelineStage::FlatMap { slot, value },
    LoweredPipelineStage::FlatMapBlock { slot, body, value } => FlatMapBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    } => LoweredPipelineStage::FlatMapBlock { slot, body, value },
    LoweredPipelineStage::BytesChunks { size } => BytesChunks {
        size: BuildExprId,
    } => LoweredPipelineStage::BytesChunks { size },
    LoweredPipelineStage::BatchCount { count } => BatchCount {
        count: BuildExprId,
    } => LoweredPipelineStage::BatchCount { count },
    LoweredPipelineStage::BatchMaxArgv { max_argv } => BatchMaxArgv {
        max_argv: Option<BuildExprId>,
    } => LoweredPipelineStage::BatchMaxArgv { max_argv },
    LoweredPipelineStage::BatchMaxBytes { max_bytes } => BatchMaxBytes {
        max_bytes: BuildExprId,
    } => LoweredPipelineStage::BatchMaxBytes { max_bytes },
    LoweredPipelineStage::BatchLimits { configuration } => BatchLimits {
        configuration: BuildExprId,
    } => LoweredPipelineStage::BatchLimits { configuration },
    LoweredPipelineStage::Shuffle { seed } => Shuffle {
        seed: Option<BuildExprId>,
    } => LoweredPipelineStage::Shuffle { seed },
    LoweredPipelineStage::Fold {
        acc_slot,
        item_slot,
        initial,
        body,
        value,
    } => Fold {
        acc_slot: usize,
        item_slot: usize,
        initial: BuildExprId,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    } => LoweredPipelineStage::Fold {
        acc_slot,
        item_slot,
        initial,
        body,
        value,
    },
    LoweredPipelineStage::ReduceBy {
        item_slot,
        body,
        value,
        op,
        jobs,
    } => ReduceBy {
        item_slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
        op: ReduceByOp,
        jobs: Option<BuildExprId>,
    } => LoweredPipelineStage::ReduceBy {
        item_slot,
        body,
        value,
        op,
        jobs,
    },
    LoweredPipelineStage::ReduceByConfigured { item_slot, body, value, configuration } => ReduceByConfigured {
        item_slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
        configuration: BuildExprId,
    } => LoweredPipelineStage::ReduceByConfigured { item_slot, body, value, configuration },
    LoweredPipelineStage::ParMap { slot, jobs, value } => ParMap {
        slot: usize,
        jobs: Option<BuildExprId>,
        value: BuildExprId,
    } => LoweredPipelineStage::ParMap { slot, jobs, value },
    LoweredPipelineStage::ParMapBlock {
        slot,
        body,
        jobs,
        value,
    } => ParMapBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        jobs: Option<BuildExprId>,
        value: BuildExprId,
    } => LoweredPipelineStage::ParMapBlock {
        slot,
        body,
        jobs,
        value,
    },
    LoweredPipelineStage::ParMapFlatMapReduceBy {
        slot,
        body,
        jobs,
        value,
        flatten,
        reduce_item_slot,
        reduce_body,
        reduce_value,
        op,
    } => ParMapFlatMapReduceBy {
        slot: usize,
        body: Option<Vec<BuildStmtId>>,
        jobs: Option<BuildExprId>,
        value: BuildExprId,
        flatten: bool,
        reduce_item_slot: usize,
        reduce_body: Vec<BuildStmtId>,
        reduce_value: BuildExprId,
        op: ReduceByOp,
    } => LoweredPipelineStage::ParMapFlatMapReduceBy {
        slot,
        body,
        jobs,
        value,
        flatten,
        reduce_item_slot,
        reduce_body,
        reduce_value,
        op,
    },
    LoweredPipelineStage::Tee { slot, body } => Tee {
        slot: usize,
        body: Vec<BuildStmtId>,
    } => LoweredPipelineStage::Tee { slot, body },
    LoweredPipelineStage::Each {
        slot,
        body,
    } => Each {
        slot: usize,
        body: Vec<BuildStmtId>,
    } => LoweredPipelineStage::Each {
        slot,
        body,
    },
    LoweredPipelineStage::TablePrint { columns } => TablePrint {
        columns: Option<Vec<String>>,
    } => LoweredPipelineStage::TablePrint { columns },
    LoweredPipelineStage::TablePrintConfigured { columns } => TablePrintConfigured {
        columns: BuildExprId,
    } => LoweredPipelineStage::TablePrintConfigured { columns },
    LoweredPipelineStage::Enumerate => Enumerate {} => LoweredPipelineStage::Enumerate,
    LoweredPipelineStage::Zip { other } => Zip {
        other: BuildExprId,
    } => LoweredPipelineStage::Zip { other },
    LoweredPipelineStage::Sort { descending } => Sort {
        descending: Option<BuildExprId>,
    } => LoweredPipelineStage::Sort { descending },
    LoweredPipelineStage::SortBy {
        slot,
        key,
        descending,
    } => SortBy {
        slot: usize,
        key: BuildExprId,
        descending: Option<BuildExprId>,
    } => LoweredPipelineStage::SortBy {
        slot,
        key,
        descending,
    },
    LoweredPipelineStage::GroupBy { slot, key } => GroupBy {
        slot: usize,
        key: BuildExprId,
    } => LoweredPipelineStage::GroupBy { slot, key },
    LoweredPipelineStage::CountBy { slot, key } => CountBy {
        slot: usize,
        key: BuildExprId,
    } => LoweredPipelineStage::CountBy { slot, key },
    LoweredPipelineStage::Any { slot, predicate } => Any {
        slot: usize,
        predicate: BuildExprId,
    } => LoweredPipelineStage::Any { slot, predicate },
    LoweredPipelineStage::AnyBlock { slot, body, value } => AnyBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    } => LoweredPipelineStage::AnyBlock { slot, body, value },
    LoweredPipelineStage::All { slot, predicate } => All {
        slot: usize,
        predicate: BuildExprId,
    } => LoweredPipelineStage::All { slot, predicate },
    LoweredPipelineStage::AllBlock { slot, body, value } => AllBlock {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    } => LoweredPipelineStage::AllBlock { slot, body, value },
    LoweredPipelineStage::UniqueBy { slot, key } => UniqueBy {
        slot: usize,
        key: BuildExprId,
    } => LoweredPipelineStage::UniqueBy { slot, key },
    LoweredPipelineStage::Count => Count {} => LoweredPipelineStage::Count,
    LoweredPipelineStage::Sum => Sum {} => LoweredPipelineStage::Sum,
    LoweredPipelineStage::Collect => Collect {} => LoweredPipelineStage::Collect,
    LoweredPipelineStage::First => First {} => LoweredPipelineStage::First,
    LoweredPipelineStage::Last => Last {} => LoweredPipelineStage::Last,
    LoweredPipelineStage::Min => Min {} => LoweredPipelineStage::Min,
    LoweredPipelineStage::Max => Max {} => LoweredPipelineStage::Max,
    LoweredPipelineStage::Take(value) => Take {
        value: BuildExprId,
    } => LoweredPipelineStage::Take(value),
    LoweredPipelineStage::Drop(value) => Drop {
        value: BuildExprId,
    } => LoweredPipelineStage::Drop(value),
    LoweredPipelineStage::Repeat { count } => Repeat {
        count: BuildExprId,
    } => LoweredPipelineStage::Repeat { count },
    LoweredPipelineStage::Range { start, end } => Range {
        start: BuildExprId,
        end: BuildExprId,
    } => LoweredPipelineStage::Range { start, end },
}

impl_node_codec! {
    BuildExprRow {
        BuildExprRow::Null => ExprNull {} => BuildExprRow::Null,
        BuildExprRow::Unit => ExprUnit {} => BuildExprRow::Unit,
        BuildExprRow::Int(value) => ExprInt { value: i64 } => BuildExprRow::Int(value),
        BuildExprRow::Float(value) => ExprFloat {
            value: FloatValue,
        } => BuildExprRow::Float(value),
        BuildExprRow::Duration(value) => ExprDuration {
            value: DurationValue,
        } => BuildExprRow::Duration(value),
        BuildExprRow::Bool(value) => ExprBool { value: bool } => BuildExprRow::Bool(value),
        BuildExprRow::Str(value) => ExprStr {
            value: Arc<str>,
        } => BuildExprRow::Str(value),
        BuildExprRow::PreparedConstant(value) => ExprPreparedConstant {
            value: PreparedConstantValue,
        } => BuildExprRow::PreparedConstant(value),
        BuildExprRow::PreparedRegex(value) => ExprPreparedRegex {
            value: RegexValue,
        } => BuildExprRow::PreparedRegex(value),
        BuildExprRow::Bytes(value) => ExprBytes {
            value: Arc<[u8]>,
        } => BuildExprRow::Bytes(value),
        BuildExprRow::Path(value) => ExprPath {
            value: PathValue,
        } => BuildExprRow::Path(value),
        BuildExprRow::FunctionRef { function, pure } => ExprFunctionRef {
            function: FunctionName,
            pure: bool,
        } => BuildExprRow::FunctionRef { function, pure },
        BuildExprRow::NativeCallableRef => ExprNativeCallableRef {} => BuildExprRow::NativeCallableRef,
        BuildExprRow::PathFrom { value, span } => ExprPathFrom {
            value: BuildExprId,
            span: Span,
        } => BuildExprRow::PathFrom { value, span },
        BuildExprRow::Param(slot) => ExprParam {
            slot: usize,
        } => BuildExprRow::Param(slot),
        BuildExprRow::Assert { value, span } => ExprAssert {
            value: BuildExprId,
            span: Span,
        } => BuildExprRow::Assert { value, span },
        BuildExprRow::ComparisonChain { pairs, assertion } => ExprComparisonChain {
            pairs: Vec<BuildExprId>,
            assertion: bool,
        } => BuildExprRow::ComparisonChain { pairs, assertion },
        BuildExprRow::Binary {
            op,
            left,
            right,
            span,
        } => ExprBinary {
            op: BinaryOp,
            left: BuildExprId,
            right: BuildExprId,
            span: Span,
        } => BuildExprRow::Binary {
            op,
            left,
            right,
            span,
        },
        BuildExprRow::IfExpr {
            branches,
            else_value,
            span,
        } => ExprIf {
            branches: Vec<(BuildExprId, BuildExprId)>,
            else_value: BuildExprId,
            span: Span,
        } => BuildExprRow::IfExpr {
            branches,
            else_value,
            span,
        },
        BuildExprRow::PatternIf { branches, else_value, span } => ExprPatternIf {
            branches: Vec<(BuildExprId, BuildExprId, Vec<usize>)>,
            else_value: BuildExprId,
            span: Span,
        } => BuildExprRow::PatternIf { branches, else_value, span },
        BuildExprRow::MatchExpr { value, arms, span } => ExprMatch {
            value: BuildExprId,
            arms: Vec<(BuildPatternId, Option<BuildExprId>, BuildExprId)>,
            span: Span,
        } => BuildExprRow::MatchExpr { value, arms, span },
        BuildExprRow::StrMatchExpr {
            value,
            arms,
            fallback,
            span,
        } => ExprStrMatch {
            value: BuildExprId,
            arms: FxHashMap<Arc<str>, BuildExprId>,
            fallback: Option<BuildExprId>,
            span: Span,
        } => BuildExprRow::StrMatchExpr {
            value,
            arms,
            fallback,
            span,
        },
        BuildExprRow::TagMatchExpr {
            value,
            arms,
            fallback,
            span,
        } => ExprTagMatch {
            value: BuildExprId,
            arms: FxHashMap<Arc<str>, BuildExprId>,
            fallback: Option<BuildExprId>,
            span: Span,
        } => BuildExprRow::TagMatchExpr {
            value,
            arms,
            fallback,
            span,
        },
        BuildExprRow::ResultFallback { left, right } => ExprResultFallback {
            left: BuildExprId,
            right: BuildExprId,
        } => BuildExprRow::ResultFallback { left, right },
        BuildExprRow::FmtString(parts) => ExprFmtString {
            parts: Vec<LoweredFmtPart>,
        } => BuildExprRow::FmtString(parts),
        BuildExprRow::PathFmtString { parts, span } => ExprPathFmtString {
            parts: Vec<LoweredFmtPart>,
            span: Span,
        } => BuildExprRow::PathFmtString { parts, span },
        BuildExprRow::Glob { pattern, span } => ExprGlob {
            pattern: Arc<str>,
            span: Span,
        } => BuildExprRow::Glob { pattern, span },
        BuildExprRow::LastStatus { span } => ExprLastStatus {
            span: Span,
        } => BuildExprRow::LastStatus { span },
        BuildExprRow::MapLiteral(entries) => ExprMapLiteral {
            entries: Vec<(Option<BuildExprId>, BuildExprId, Span)>,
        } => BuildExprRow::MapLiteral(entries),
        BuildExprRow::RecordUpdate { base, updates, span } => ExprRecordUpdate {
            base: BuildExprId, updates: LoweredRecordUpdates, span: Span,
        } => BuildExprRow::RecordUpdate { base, updates, span },
        BuildExprRow::Record(entries) => ExprRecord {
            entries: Vec<LoweredRecordEntry>,
        } => BuildExprRow::Record(entries),
        BuildExprRow::List(values) => ExprList {
            values: Vec<BuildExprId>,
        } => BuildExprRow::List(values),
        BuildExprRow::ListBuild(values) => ExprListBuild {
            values: Vec<(bool, BuildExprId, Span)>,
        } => BuildExprRow::ListBuild(values),
        BuildExprRow::EmptyMap => ExprEmptyMap {} => BuildExprRow::EmptyMap,
        BuildExprRow::BytesConcat { arg, span } => ExprBytesConcat {
            arg: BuildExprId,
            span: Span,
        } => BuildExprRow::BytesConcat { arg, span },
        BuildExprRow::Range { start, end, span } => ExprRange {
            start: BuildExprId,
            end: BuildExprId,
            span: Span,
        } => BuildExprRow::Range { start, end, span },
        BuildExprRow::Tag { type_name, name, fields, wire } => ExprTag {
            type_name: Name,
            name: Arc<str>,
            fields: Vec<BuildExprId>,
            wire: Option<Arc<crate::sema::wire_enums::WireEnumMapping>>,
        } => BuildExprRow::Tag { type_name, name, fields, wire },
        BuildExprRow::ListComp { value, qualifiers, span } => ExprListComp {
            value: BuildExprId,
            qualifiers: LoweredCompQualifiers,
            span: Span,
        } => BuildExprRow::ListComp { value, qualifiers, span },
        BuildExprRow::MapComp { key, value, qualifiers, span } => ExprMapComp {
            key: BuildExprId,
            value: BuildExprId,
            qualifiers: LoweredCompQualifiers,
            span: Span,
        } => BuildExprRow::MapComp { key, value, qualifiers, span },
        BuildExprRow::ListPipeline {
            input,
            stages,
            span,
        } => ExprPipeline {
            input: BuildExprId,
            stages: Vec<LoweredPipelineStage>,
            span: Span,
        } => BuildExprRow::ListPipeline {
            input,
            stages,
            span,
        },
        BuildExprRow::Field { base, name, span } => ExprField {
            base: BuildExprId,
            name: NameText,
            span: Span,
        } => BuildExprRow::Field { base, name, span },
        BuildExprRow::Index { base, index, span } => ExprIndex {
            base: BuildExprId,
            index: BuildExprId,
            span: Span,
        } => BuildExprRow::Index { base, index, span },
        BuildExprRow::Slice {
            base,
            start,
            end,
            span,
        } => ExprSlice {
            base: BuildExprId,
            start: Option<BuildExprId>,
            end: Option<BuildExprId>,
            span: Span,
        } => BuildExprRow::Slice {
            base,
            start,
            end,
            span,
        },
        BuildExprRow::Method {
            receiver,
            name,
            args,
            span,
        } => ExprMethod {
            receiver: BuildExprId,
            name: NameText,
            args: Vec<BuildExprId>,
            span: Span,
        } => BuildExprRow::Method {
            receiver,
            name,
            args,
            span,
        },
        BuildExprRow::StrByteLen { receiver, span } => ExprStrByteLen {
            receiver: BuildExprId,
            span: Span,
        } => BuildExprRow::StrByteLen { receiver, span },
        BuildExprRow::StrByteAt {
            receiver,
            index,
            span,
        } => ExprStrByteAt {
            receiver: BuildExprId,
            index: BuildExprId,
            span: Span,
        } => BuildExprRow::StrByteAt {
            receiver,
            index,
            span,
        },
        BuildExprRow::StrPredicate {
            receiver,
            predicate,
            needle,
            span,
        } => ExprStrPredicate {
            receiver: BuildExprId,
            predicate: LoweredStrPredicate,
            needle: BuildExprId,
            span: Span,
        } => BuildExprRow::StrPredicate {
            receiver,
            predicate,
            needle,
            span,
        },
        BuildExprRow::RegexCompile { pattern, span } => ExprRegexCompile {
            pattern: BuildExprId,
            span: Span,
        } => BuildExprRow::RegexCompile { pattern, span },
        BuildExprRow::CheckedValue { value, check, span } => ExprCheckedValue {
            value: BuildExprId,
            check: LoweredTypeCheck,
            span: Span,
        } => BuildExprRow::CheckedValue { value, check, span },
        BuildExprRow::Require { value, check, span } => ExprRequire {
            value: BuildExprId,
            check: LoweredTypeCheck,
            span: Span,
        } => BuildExprRow::Require { value, check, span },
        BuildExprRow::RunCapture(value) => ExprRunCapture {
            value: Box<LoweredRunCapture>,
        } => BuildExprRow::RunCapture(value),
        BuildExprRow::RunPipeline {
            segments,
            propagate,
            span,
        } => ExprRunPipeline {
            segments: Vec<LoweredRunPipelineSegment>,
            propagate: bool,
            span: Span,
        } => BuildExprRow::RunPipeline {
            segments,
            propagate,
            span,
        },
        BuildExprRow::SpawnRun(value) => ExprSpawnRun {
            value: Box<LoweredSpawnRun>,
        } => BuildExprRow::SpawnRun(value),
        BuildExprRow::SpawnCommand { command, span } => ExprSpawnCommand {
            command: BuildExprId,
            span: Span,
        } => BuildExprRow::SpawnCommand { command, span },
        BuildExprRow::Wait { target, span } => ExprWait {
            target: BuildExprId,
            span: Span,
        } => BuildExprRow::Wait { target, span },
        BuildExprRow::Capture { body, span } => ExprCapture {
            body: Vec<BuildStmtId>, span: Span,
        } => BuildExprRow::Capture { body, span },
        BuildExprRow::ErrorContext { message, body, span } => ExprErrorContext {
            message: BuildExprId, body: Vec<BuildStmtId>, span: Span,
        } => BuildExprRow::ErrorContext { message, body, span },
        BuildExprRow::ContextScope { kind, input, body, span } => ExprContextScope {
            kind: crate::syntax::arena::ContextScopeKind, input: BuildExprId, body: Vec<BuildStmtId>, span: Span,
        } => BuildExprRow::ContextScope { kind, input, body, span },
        BuildExprRow::ValueBlock { body, span } => ExprValueBlock {
            body: Vec<BuildStmtId>, span: Span,
        } => BuildExprRow::ValueBlock { body, span },
        BuildExprRow::Loop { body, span } => ExprLoop {
            body: Vec<BuildStmtId>,
            span: Span,
        } => BuildExprRow::Loop { body, span },
        BuildExprRow::Retry { delays, pattern, body, span } => ExprRetry {
            delays: Vec<BuildExprId>,
            pattern: Option<BuildPatternId>,
            body: Vec<BuildStmtId>,
            span: Span,
        } => BuildExprRow::Retry { delays, pattern, body, span },
        BuildExprRow::FsFiles {
            root,
            gitignore,
            stat,
            hidden,
            exts,
            result_wrapped,
            span,
        } => ExprFsFiles {
            root: BuildExprId,
            gitignore: Option<BuildExprId>,
            stat: Option<BuildExprId>,
            hidden: Option<BuildExprId>,
            exts: Option<BuildExprId>,
            result_wrapped: bool,
            span: Span,
        } => BuildExprRow::FsFiles {
            root,
            gitignore,
            stat,
            hidden,
            exts,
            result_wrapped,
            span,
        },
        BuildExprRow::FsWalk {
            root,
            gitignore,
            stat,
            hidden,
            exts,
            result_wrapped,
            span,
        } => ExprFsWalk {
            root: BuildExprId,
            gitignore: Option<BuildExprId>,
            stat: Option<BuildExprId>,
            hidden: Option<BuildExprId>,
            exts: Option<BuildExprId>,
            result_wrapped: bool,
            span: Span,
        } => BuildExprRow::FsWalk {
            root,
            gitignore,
            stat,
            hidden,
            exts,
            result_wrapped,
            span,
        },
        BuildExprRow::FsList {
            op,
            path,
            stat,
            ordered,
            span,
        } => ExprFsList {
            op: RuntimeOp,
            path: BuildExprId,
            stat: Option<BuildExprId>,
            ordered: Option<BuildExprId>,
            span: Span,
        } => BuildExprRow::FsList {
            op,
            path,
            stat,
            ordered,
            span,
        },
        BuildExprRow::FsTempDir { span } => ExprFsTempDir {
            span: Span,
        } => BuildExprRow::FsTempDir { span },
        BuildExprRow::FsWrite { path, data, span } => ExprFsWrite {
            path: BuildExprId,
            data: BuildExprId,
            span: Span,
        } => BuildExprRow::FsWrite { path, data, span },
        BuildExprRow::FsMkdir {
            path,
            parents,
            span,
        } => ExprFsMkdir {
            path: BuildExprId,
            parents: Option<BuildExprId>,
            span: Span,
        } => BuildExprRow::FsMkdir {
            path,
            parents,
            span,
        },
        BuildExprRow::FsRemove {
            path,
            missing_ok,
            span,
        } => ExprFsRemove {
            path: BuildExprId,
            missing_ok: Option<BuildExprId>,
            span: Span,
        } => BuildExprRow::FsRemove {
            path,
            missing_ok,
            span,
        },
        BuildExprRow::PathReadText { path, span } => ExprPathReadText {
            path: BuildExprId,
            span: Span,
        } => BuildExprRow::PathReadText { path, span },
        BuildExprRow::PathReadBytes { path, span } => ExprPathReadBytes {
            path: BuildExprId,
            span: Span,
        } => BuildExprRow::PathReadBytes { path, span },
        BuildExprRow::PathExists { path, span } => ExprPathExists {
            path: BuildExprId,
            span: Span,
        } => BuildExprRow::PathExists { path, span },
        BuildExprRow::PathExecutable { path, span } => ExprPathExecutable {
            path: BuildExprId,
            span: Span,
        } => BuildExprRow::PathExecutable { path, span },
        BuildExprRow::PathDu { path, span } => ExprPathDu {
            path: BuildExprId,
            span: Span,
        } => BuildExprRow::PathDu { path, span },
        BuildExprRow::PathMetadata { path, span } => ExprPathMetadata {
            path: BuildExprId,
            span: Span,
        } => BuildExprRow::PathMetadata { path, span },
        BuildExprRow::PathReadlink { path, span } => ExprPathReadlink {
            path: BuildExprId,
            span: Span,
        } => BuildExprRow::PathReadlink { path, span },
        BuildExprRow::PathResolve { path, span } => ExprPathResolve {
            path: BuildExprId,
            span: Span,
        } => BuildExprRow::PathResolve { path, span },
        BuildExprRow::PathWrite {
            path,
            data,
            atomic,
            span,
        } => ExprPathWrite {
            path: BuildExprId,
            data: BuildExprId,
            atomic: bool,
            span: Span,
        } => BuildExprRow::PathWrite {
            path,
            data,
            atomic,
            span,
        },
        BuildExprRow::PathMkdir {
            path,
            parents,
            span,
        } => ExprPathMkdir {
            path: BuildExprId,
            parents: Option<BuildExprId>,
            span: Span,
        } => BuildExprRow::PathMkdir {
            path,
            parents,
            span,
        },
        BuildExprRow::PathRemove {
            path,
            missing_ok,
            span,
        } => ExprPathRemove {
            path: BuildExprId,
            missing_ok: Option<BuildExprId>,
            span: Span,
        } => BuildExprRow::PathRemove {
            path,
            missing_ok,
            span,
        },
        BuildExprRow::JsonEncode { value, span } => ExprJsonEncode {
            value: BuildExprId,
            span: Span,
        } => BuildExprRow::JsonEncode { value, span },
        BuildExprRow::ArchiveTarCreate {
            path,
            root,
            entries,
            compression,
            overwrite,
            span,
        } => ExprArchiveTarCreate {
            path: BuildExprId,
            root: BuildExprId,
            entries: BuildExprId,
            compression: Option<BuildExprId>,
            overwrite: Option<BuildExprId>,
            span: Span,
        } => BuildExprRow::ArchiveTarCreate {
            path,
            root,
            entries,
            compression,
            overwrite,
            span,
        },
        BuildExprRow::ArchiveTarList { path, span } => ExprArchiveTarList {
            path: BuildExprId,
            span: Span,
        } => BuildExprRow::ArchiveTarList { path, span },
        BuildExprRow::ArchiveTarExtract { path, dest, span } => ExprArchiveTarExtract {
            path: BuildExprId,
            dest: BuildExprId,
            span: Span,
        } => BuildExprRow::ArchiveTarExtract { path, dest, span },
        BuildExprRow::ModuleCall { op, cli_plan, args, span } => ExprModuleCall {
            op: RuntimeOp,
            cli_plan: Option<Arc<crate::modules::cli::CliDescriptorPlan>>,
            args: Vec<Option<BuildExprId>>,
            span: Span,
        } => BuildExprRow::ModuleCall { op, cli_plan, args, span },
        BuildExprRow::ProcessCommandArgv(value) => ExprProcessCommandArgv {
            value: Box<LoweredProcessCommandArgv>,
        } => BuildExprRow::ProcessCommandArgv(value),
        BuildExprRow::ProcessCommandBuilder { entries, span } => ExprProcessCommandBuilder {
            entries: Vec<LoweredProcessCommandBuilderEntry>,
            span: Span,
        } => BuildExprRow::ProcessCommandBuilder { entries, span },
        BuildExprRow::Abort {
            status,
            force,
            span,
        } => ExprAbort {
            status: BuildExprId,
            force: Option<BuildExprId>,
            span: Span,
        } => BuildExprRow::Abort {
            status,
            force,
            span,
        },
        BuildExprRow::Fail { message, span } => ExprFail {
            message: BuildExprId,
            span: Span,
        } => BuildExprRow::Fail { message, span },
        BuildExprRow::Ok(value) => ExprOk {
            value: BuildExprId,
        } => BuildExprRow::Ok(value),
        BuildExprRow::Err { value, cause } => ExprErr {
            value: BuildExprId,
            cause: Option<BuildExprId>,
        } => BuildExprRow::Err { value, cause },
        BuildExprRow::Error(value) => ExprError {
            value: Box<LoweredErrorExpr>,
        } => BuildExprRow::Error(value),
        BuildExprRow::Try(value) => ExprTry {
            value: BuildExprId,
        } => BuildExprRow::Try(value),
        BuildExprRow::Call {
            function,
            args,
            span,
        } => ExprCall {
            function: LoweredFunctionKey,
            args: Vec<LoweredCallArg>,
            span: Span,
        } => BuildExprRow::Call {
            function,
            args,
            span,
        },
        BuildExprRow::ExternalCall {
            function,
            args,
            span,
        } => ExprExternalCall {
            function: QualifiedName,
            args: Vec<LoweredCallArg>,
            span: Span,
        } => BuildExprRow::ExternalCall {
            function,
            args,
            span,
        },
        BuildExprRow::DirectPureCall {
            function,
            args,
            span,
        } => ExprDirectPureCall {
            function: LoweredFunctionKey,
            args: Vec<LoweredCallArg>,
            span: Span,
        } => BuildExprRow::DirectPureCall {
            function,
            args,
            span,
        },
        BuildExprRow::DynamicCall { callee, args, span } => ExprDynamicCall {
            callee: BuildExprId,
            args: Vec<LoweredCallArg>,
            span: Span,
        } => BuildExprRow::DynamicCall { callee, args, span },
        BuildExprRow::SelfCall { args, span } => ExprSelfCall {
            args: Vec<LoweredCallArg>,
            span: Span,
        } => BuildExprRow::SelfCall { args, span },
    }
}

impl_node_codec! {
    BuildStmtRow {
        BuildStmtRow::DefaultParameter { slot, value, kind, check, span } => StmtDefaultParameter {
            slot: usize,
            value: BuildExprId,
            kind: LoweredType,
            check: Option<LoweredTypeCheck>,
            span: Span,
        } => BuildStmtRow::DefaultParameter { slot, value, kind, check, span },
        BuildStmtRow::Let { slot, value } => StmtLet {
            slot: usize,
            value: BuildExprId,
        } => BuildStmtRow::Let { slot, value },
        BuildStmtRow::Guard {
            target,
            value,
            else_param_slot,
            else_body,
            span,
        } => StmtGuard {
            target: LoweredCompTarget,
            value: BuildExprId,
            else_param_slot: Option<usize>,
            else_body: Vec<BuildStmtId>,
            span: Span,
        } => BuildStmtRow::Guard {
            target,
            value,
            else_param_slot,
            else_body,
            span,
        },
        BuildStmtRow::With { bindings, body, else_param_slot, else_body, captures, span } => StmtWith {
            bindings: Vec<(usize, BuildExprId)>,
            body: Vec<BuildStmtId>,
            else_param_slot: Option<usize>,
            else_body: Vec<BuildStmtId>,
            captures: Vec<usize>,
            span: Span,
        } => BuildStmtRow::With { bindings, body, else_param_slot, else_body, captures, span },
        BuildStmtRow::LetInt { slot, value } => StmtLetInt {
            slot: usize,
            value: BuildIntId,
        } => BuildStmtRow::LetInt { slot, value },
        BuildStmtRow::LetBool { slot, value } => StmtLetBool {
            slot: usize,
            value: BuildBoolId,
        } => BuildStmtRow::LetBool { slot, value },
        BuildStmtRow::Assign {
            slot,
            op,
            value,
            check,
            span,
        } => StmtAssign {
            slot: usize,
            op: AssignOp,
            value: BuildExprId,
            check: Option<LoweredTypeCheck>,
            span: Span,
        } => BuildStmtRow::Assign {
            slot,
            op,
            value,
            check,
            span,
        },
        BuildStmtRow::AssignField {
            slot,
            field,
            op,
            value,
            span,
        } => StmtAssignField {
            slot: usize,
            field: Arc<str>,
            op: AssignOp,
            value: BuildExprId,
            span: Span,
        } => BuildStmtRow::AssignField {
            slot,
            field,
            op,
            value,
            span,
        },
        BuildStmtRow::AssignFieldInt {
            slot,
            field,
            op,
            value,
            span,
        } => StmtAssignFieldInt {
            slot: usize,
            field: Arc<str>,
            op: AssignOp,
            value: BuildIntId,
            span: Span,
        } => BuildStmtRow::AssignFieldInt {
            slot,
            field,
            op,
            value,
            span,
        },
        BuildStmtRow::AssignPath { slot, path, op, value, check, span } => StmtAssignPath {
            slot: usize,
            path: LoweredAssignPath,
            op: AssignOp,
            value: BuildExprId,
            check: Option<LoweredTypeCheck>,
            span: Span,
        } => BuildStmtRow::AssignPath { slot, path, op, value, check, span },
        BuildStmtRow::AssignInt {
            slot,
            op,
            value,
            span,
        } => StmtAssignInt {
            slot: usize,
            op: AssignOp,
            value: BuildIntId,
            span: Span,
        } => BuildStmtRow::AssignInt {
            slot,
            op,
            value,
            span,
        },
        BuildStmtRow::AssignBool { slot, value } => StmtAssignBool {
            slot: usize,
            value: BuildBoolId,
        } => BuildStmtRow::AssignBool { slot, value },
        BuildStmtRow::Value { value } => StmtValue { value: BuildExprId } => BuildStmtRow::Value { value },
        BuildStmtRow::Assert { value, message, span } => StmtAssert { value: BuildExprId, message: Option<BuildExprId>, span: Span } => BuildStmtRow::Assert { value, message, span },
        BuildStmtRow::Expr { value, span } => StmtExpr {
            value: BuildExprId,
            span: Span,
        } => BuildStmtRow::Expr { value, span },
        BuildStmtRow::If {
            branches,
            else_body,
        } => StmtIf {
            branches: Vec<(BuildExprId, Vec<BuildStmtId>)>,
            else_body: Option<Vec<BuildStmtId>>,
        } => BuildStmtRow::If {
            branches,
            else_body,
        },
        BuildStmtRow::IfBool {
            branches,
            else_body,
        } => StmtIfBool {
            branches: Vec<(BuildBoolId, Vec<BuildStmtId>)>,
            else_body: Option<Vec<BuildStmtId>>,
        } => BuildStmtRow::IfBool {
            branches,
            else_body,
        },
        BuildStmtRow::PatternIf { branches, else_body, span } => StmtPatternIf {
            branches: Vec<(BuildExprId, Vec<BuildStmtId>, Vec<usize>)>,
            else_body: Option<Vec<BuildStmtId>>,
            span: Span,
        } => BuildStmtRow::PatternIf { branches, else_body, span },
        BuildStmtRow::PatternWhile { condition, body, captures, span } => StmtPatternWhile {
            condition: BuildExprId,
            body: Vec<BuildStmtId>,
            captures: Vec<usize>,
            span: Span,
        } => BuildStmtRow::PatternWhile { condition, body, captures, span },
        BuildStmtRow::While { condition, body } => StmtWhile {
            condition: BuildExprId,
            body: Vec<BuildStmtId>,
        } => BuildStmtRow::While { condition, body },
        BuildStmtRow::WhileBool { condition, body } => StmtWhileBool {
            condition: BuildBoolId,
            body: Vec<BuildStmtId>,
        } => BuildStmtRow::WhileBool { condition, body },
        BuildStmtRow::Match { value, arms, span } => StmtMatch {
            value: BuildExprId,
            arms: Vec<(BuildPatternId, Option<BuildExprId>, Vec<BuildStmtId>)>,
            span: Span,
        } => BuildStmtRow::Match { value, arms, span },
        BuildStmtRow::StrMatch {
            value,
            arms,
            fallback,
            span,
        } => StmtStrMatch {
            value: BuildExprId,
            arms: FxHashMap<Arc<str>, Vec<BuildStmtId>>,
            fallback: Option<Vec<BuildStmtId>>,
            span: Span,
        } => BuildStmtRow::StrMatch {
            value,
            arms,
            fallback,
            span,
        },
        BuildStmtRow::TagMatch {
            value,
            arms,
            fallback,
            span,
        } => StmtTagMatch {
            value: BuildExprId,
            arms: FxHashMap<Arc<str>, Vec<BuildStmtId>>,
            fallback: Option<Vec<BuildStmtId>>,
            span: Span,
        } => BuildStmtRow::TagMatch {
            value,
            arms,
            fallback,
            span,
        },
        BuildStmtRow::For {
            slot,
            iter,
            body,
            span,
        } => StmtFor {
            slot: usize,
            iter: BuildExprId,
            body: Vec<BuildStmtId>,
            span: Span,
        } => BuildStmtRow::For {
            slot,
            iter,
            body,
            span,
        },
        BuildStmtRow::LetRecord {
            source,
            target,
            span,
        } => StmtLetRecord {
            source: BuildExprId,
            target: LoweredCompTarget,
            span: Span,
        } => BuildStmtRow::LetRecord {
            source,
            target,
            span,
        },
        BuildStmtRow::ForRecord {
            target,
            iter,
            body,
            span,
        } => StmtForRecord {
            target: LoweredCompTarget,
            iter: BuildExprId,
            body: Vec<BuildStmtId>,
            span: Span,
        } => BuildStmtRow::ForRecord {
            target,
            iter,
            body,
            span,
        },
        BuildStmtRow::ForStrLines {
            slot,
            text,
            body,
            span,
        } => StmtForStrLines {
            slot: usize,
            text: BuildExprId,
            body: Vec<BuildStmtId>,
            span: Span,
        } => BuildStmtRow::ForStrLines {
            slot,
            text,
            body,
            span,
        },
        BuildStmtRow::ScanLines {
            text_slot,
            line_slot,
            checks,
            span,
        } => StmtScanLines {
            text_slot: usize,
            line_slot: usize,
            checks: Vec<ScanCheck>,
            span: Span,
        } => BuildStmtRow::ScanLines {
            text_slot,
            line_slot,
            checks,
            span,
        },
        BuildStmtRow::ScanBytes { config } => StmtScanBytes {
            config: ScanBytes,
        } => BuildStmtRow::ScanBytes { config },
        BuildStmtRow::Print {
            args,
            stderr,
            flush,
            propagate_result,
            span,
        } => StmtPrint {
            args: Vec<BuildExprId>,
            stderr: bool,
            flush: bool,
            propagate_result: bool,
            span: Span,
        } => BuildStmtRow::Print {
            args,
            stderr,
            flush,
            propagate_result,
            span,
        },
        BuildStmtRow::Cd { target, body, span } => StmtCd {
            target: BuildExprId,
            body: Vec<BuildStmtId>,
            span: Span,
        } => BuildStmtRow::Cd { target, body, span },
        BuildStmtRow::Env { env, body } => StmtEnv {
            env: Vec<LoweredRunEnv>,
            body: Vec<BuildStmtId>,
        } => BuildStmtRow::Env { env, body },
        BuildStmtRow::Proc {
            op,
            args,
            propagate_result,
            span,
        } => StmtProc {
            op: RuntimeOp,
            args: Vec<BuildExprId>,
            propagate_result: bool,
            span: Span,
        } => BuildStmtRow::Proc {
            op,
            args,
            propagate_result,
            span,
        },
        BuildStmtRow::Run {
            value,
            propagate_result,
        } => StmtRun {
            value: BuildExprId,
            propagate_result: bool,
        } => BuildStmtRow::Run {
            value,
            propagate_result,
        },
        BuildStmtRow::Loop { body } => StmtLoop {
            body: Vec<BuildStmtId>,
        } => BuildStmtRow::Loop { body },
        BuildStmtRow::Return { value } => StmtReturn {
            value: BuildExprId,
        } => BuildStmtRow::Return { value },
        BuildStmtRow::YieldDelegate { value, span } => StmtYieldDelegate {
            value: BuildExprId,
            span: Span,
        } => BuildStmtRow::YieldDelegate { value, span },
        BuildStmtRow::Yield { value } => StmtYield {
            value: BuildExprId,
        } => BuildStmtRow::Yield { value },
        BuildStmtRow::Break => StmtBreak {} => BuildStmtRow::Break,
        BuildStmtRow::BreakValue { value } => StmtBreakValue {
            value: BuildExprId,
        } => BuildStmtRow::BreakValue { value },
        BuildStmtRow::Continue => StmtContinue {} => BuildStmtRow::Continue,
        BuildStmtRow::Defer { value } => StmtDefer {
            value: BuildExprId,
        } => BuildStmtRow::Defer { value },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::eval::Evaluator;
    use crate::runtime::value::Value;
    use crate::sema::check::Checker;
    use crate::syntax::parser::Parser;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    const INDEXED_EXECUTION: &str =
        include_str!("../../../../tests/fixtures/frontend-indexed/indexed-execution.xsh");
    const TOP_LEVEL_DRIVER_BOUNDARY: &str = r#"
let base: Int = 1

pure plus_base(value: Int) -> Int {
  return value + base
}

var total: Int = 0
var index: Int = 0

while index < 3 {
  total += index
  index += 1
}

env ({
  TOP_LEVEL_DRIVER_BOUNDARY: "indexed",
}) {
  print "indexed"
}

cd . {
  print ${plus_base(total)}
}

on USR1 [] {
  print "signal"
}

defer run true
run true
"#;

    pub(super) fn run_with_large_stack(f: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(f)
            .expect("spawn full indexed IR test")
            .join()
            .unwrap_or_else(|payload| std::panic::resume_unwind(payload));
    }

    pub(super) fn fixture(name: &str, source: &str) -> FullProgram {
        // Prepare through the loader so embedded standard-library
        // implementations are attached, exactly as the script runner does.
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            name,
            crate::loader::entry_source_from_text(name, source.to_string()),
            Vec::new(),
        );
        let source_id = crate::source::SourceMap::files(&sources)
            .first()
            .map(crate::source::SourceFile::id)
            .expect("entry source is present");
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        assert!(
            declarations.diagnostics.is_empty(),
            "{:?}",
            declarations.diagnostics
        );
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        FullBuilder::build_compact(
            &parsed.arena,
            &declarations,
            &bodies,
            source,
            Arc::new(sources),
            source_id,
        )
        .unwrap()
    }

    pub(super) fn program_name(program: &FullProgram, text: &str) -> Name {
        program.symbol_owner().with_current(|| Name::intern(text))
    }

    #[test]
    fn builtin_templates_execute_checked_calls_after_frontend_drop_on_both_routes() {
        run_with_large_stack(|| {
            let source = include_str!("../../../../tests/fixtures/frontend-indexed/builtin-templates.xsh");
            let program = Arc::new(fixture("builtin-templates.xsh", source));
            FullVerifier::verify(&program).unwrap();
            for recursive in [false, true] {
                for (name, expected) in [("nested_templates", Value::Int(7)), ("fresh_templates", Value::Int(8)), ("absent_templates", Value::Bool(true)), ("materialized_templates", Value::Int(6)), ("discarded_templates", Value::Unit)] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let mut call = || evaluator.call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)), LoweredFunctionKind::Pure,
                        &[], Span::new(program.store.source_id, 0, 0),
                    ).expect("template function exists");
                    let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                    assert_eq!(result.unwrap(), expected);
                }
            }
        });
    }

    #[test]
    fn stage_callable_ordinary_calls_execute_after_frontend_drop_on_both_routes() {
        run_with_large_stack(|| {
            let source = "pure increment(item: Int, amount: Int = 2) -> Int { item + amount }\npure positive(item: Int) -> Bool { item > 3 }\npure result(item: Int) -> Result[Int] { Ok(item) }\npure value() -> Int { [1, 2, 3] |> map(increment) |> where(positive) |> sum }\npure results() -> Bool { let values = [3] |> map(result); values[0] is Ok(3) }\n";
            let program = fixture("stage-callable.xsh", source);
            FullVerifier::verify(&program).unwrap();
            assert!(!program.store.tags.contains(&FullTag::ExprFunctionRef));
            let program = Arc::new(program);
            for recursive in [false, true] {
                for (name, expected) in [("value", Value::Int(9)), ("results", Value::Bool(true))] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let mut call = || evaluator.call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)), LoweredFunctionKind::Pure, &[],
                        Span::new(program.store.source_id, 0, 0),
                    ).expect("stage callable function exists");
                    let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                    assert_eq!(result.unwrap(), expected);
                }
            }
        });
    }

    #[test]
    fn typed_cause_indexed_codec_preserves_metadata_and_verifies_optional_child() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/typed-causes.xsh");
        let program = fixture("typed-causes.xsh", source);
        FullVerifier::verify(&program).unwrap();
        let row = program.store.tags.iter().position(|tag| *tag == FullTag::ExprErr).unwrap();
        let range = program.store.data[row].range().bounds(program.store.extra.len()).unwrap();
        assert_eq!(program.store.extra[range.start + 1], 1);
        for (offset, value) in [(1, 2), (2, u32::MAX)] {
            let mut invalid = program.clone();
            invalid.store.extra[range.start + offset] = value;
            assert!(FullVerifier::verify(&invalid).is_err());
        }
        let mut cycle = program.clone();
        cycle.store.extra[range.start + 2] = row as u32;
        assert!(FullVerifier::verify(&cycle).is_err());
        let program = Arc::new(program);
        for recursive in [false, true] {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let mut call = || evaluator.call_indexed_direct(
                LoweredFunctionKey::Name(program_name(&program, "translated_cause")), LoweredFunctionKind::Pure,
                &[], Span::at(program.store.source_id, 0),
            ).expect("typed constructor function exists");
            let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() }.unwrap();
            let Value::Result(crate::runtime::value::ResultValue::Err(error)) = result else { panic!("constructor returns Result data") };
            let Value::Error(outer) = error.as_ref() else { panic!("nominal outer error") };
            assert_eq!(outer.family, "OuterCauseError");
            let Value::Error(inner) = outer.cause.as_ref().unwrap().as_value() else { panic!("typed cause") };
            assert_eq!(inner.family, "InnerCauseError");
            assert_eq!(inner.span, None);
        }
    }

    #[test]
    fn empty_map_fold_inference_publishes_concrete_accumulator_types() {
        let source = "let counts = [\"one\", \"two\", \"one\"] |> fold(map.empty()) { |acc, item| acc.set(item, (acc.get(item) ?? 0) + 1) }\nprint ${counts.get(\"one\") ?? 0}\n";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "full: {:?}", checked.diagnostics);
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        assert!(declarations.diagnostics.is_empty(), "decl: {:?}", declarations.diagnostics);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "compact: {:?}", bodies.diagnostics);
        assert!(bodies.expr_types.values().all(|ty| !ty.contains_inference()));
        let _ = fixture("empty-map-fold.xsh", source);
    }

    #[test]
    fn local_collection_inference_publishes_concrete_indexed_call_and_slot_types() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/local-inference.xsh");
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        parsed.arena.symbol_owner().with_current(|| {
            let function = parsed.arena.arena.function_defs.iter().find(|function| function.name == "gather").unwrap();
            let body_span = parsed.arena.arena.span(parsed.arena.arena.block(function.body).span);
            assert_eq!(declarations.function_return_types[&body_span], Type::List(Box::new(Type::Path)));
            assert!(declarations.local_binding_types.values().all(|ty| !ty.contains_inference()));
            assert!(declarations.local_binding_types.values().any(|ty| *ty == Type::List(Box::new(Type::Path))));
        });
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.expr_types.values().all(|ty| !ty.contains_inference()));
        let constructed = super::super::super::lower::probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
        assert_eq!(constructed.blocker_events, 0, "{constructed:?}");
        let _ = fixture("local-inference.xsh", source);
    }

    /// A loop reuses its statement list rather than allocating per iteration.
    #[test]
    fn loop_iterations_reuse_their_statement_list() {
        run_with_large_stack(|| {
            let program = Arc::new(fixture(
                "indexed-list-reuse.xsh",
                "proc main() [io] {\n  var index = 0\n  while index < 200 {\n    index = index + 1\n  }\n  print f\"index=${index}\"\n}\n",
            ));
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "main")),
                    LoweredFunctionKind::Proc,
                    &[],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("main exists")
                .expect("main runs");
            assert_eq!(evaluator.stdout, b"index=200\n");
            let scratch = &evaluator.frame_scratch;
            // The first iteration fills the pool, so one of the two hundred
            // takes is the fresh one.
            assert!(
                scratch.reused_statements >= 199,
                "each iteration takes its statement list back from the pool (reused {})",
                scratch.reused_statements
            );
            assert!(
                scratch.fresh_statements <= 8,
                "a 200-iteration loop must not build a list per iteration (fresh {})",
                scratch.fresh_statements
            );
        });
    }

    /// The per-program header cache: one decode per function, shared by every
    /// later call, and a program built without slots still decodes.
    ///
    /// The header is what the call path binds arguments, hydrates captures, and
    /// checks returns against, so `Arc::ptr_eq` here is the evidence that a
    /// second call does not re-resolve preparation metadata; a call's behavior
    /// cannot show that, which is why this test compares value identity rather
    /// than timing.
    #[test]
    fn function_headers_are_decoded_once_per_program() {
        let program = fixture(
            "indexed-header-cache.xsh",
            "pure plus(value: Int) -> Int {\n  return value + 1\n}\n",
        );
        let header = |program: &FullProgram| {
            program.symbol_owner().with_current(|| {
                program
                    .function_view(
                        LoweredFunctionKey::Name(Name::intern("plus")),
                        LoweredFunctionKind::Pure,
                    )
                    .expect("function view resolves")
                    .expect("the fixture declares `plus`")
                    .header()
                    .expect("the header decodes")
            })
        };
        let first = header(&program);
        let second = header(&program);
        assert!(
            Arc::ptr_eq(&first, &second),
            "a second call must reuse the decoded header"
        );
        assert_eq!(first.params.len(), second.params.len());
        assert_eq!(first.slot_count, second.slot_count);

        // A program that carries no slots — the verifier fixtures build one
        // directly — decodes without caching instead of failing.
        let mut unslotted = program.clone();
        unslotted.headers.clear();
        let decoded = header(&unslotted);
        assert_eq!(decoded.params.len(), first.params.len());
    }

    #[test]
    fn full_indexed_program_represents_every_indexed_fixture_function() {
        let program = fixture("indexed-execution.xsh", INDEXED_EXECUTION);

        assert!(program.function_count() > 0);
        assert!(
            program
                .function_view(
                    LoweredFunctionKey::Name(program_name(&program, "main")),
                    LoweredFunctionKind::Proc,
                )
                .unwrap()
                .is_some()
        );
        assert!(program.instruction_count() > 0);
        assert!(program.extra_words() > 0);
        assert!(program.retained_bytes() > size_of::<FullProgram>());
    }

    #[test]
    fn trimmed_else_if_scanner_lowers_to_scan_lines() {
        let program = fixture(
            "trimmed-scanner.xsh",
            r##"
pure scan(text: Bytes) -> Int {
  var blanks = 0
  var comments = 0
  for line in text.lines() {
    let trimmed = line.trim()
    if trimmed == b"" {
      blanks += 1
    } else if trimmed.starts_with(b"#") {
      comments += 1
    }
  }
  return text.count_lines() - blanks - comments
}

proc main() [error] {
  print scan(b"# x\nvalue\n\n")
}
"##,
        );
        assert!(program.store.tags.contains(&FullTag::StmtScanLines));
    }

    #[test]
    fn enum_payload_alias_and_alternation_execute_after_frontend_drop_on_both_routes() {
        run_with_large_stack(|| {
            let source = r#"enum Event { Added(Str), Changed(Str), Count(Int) }
pure render(event: Event) -> Str {
  match event {
    (Added(file) | Changed(file)) as original => {
      let typed: Event = original
      if typed is Added(_) { file } else { file }
    }
    Count(_) => "other"
  }
}
pure selected() -> Str {
  render(Added("one")) + ":" + render(Changed("two")) + ":" + render(Count(3))
}
"#;
            let program = fixture("enum-payload-alias-and-alternation.xsh", source);
            let program = Arc::new(program);
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "selected")), LoweredFunctionKind::Pure, &[],
                    Span::new(program.store.source_id, 0, 0),
                ).expect("enum payload function exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert_eq!(result.unwrap(), Value::Str("one:two:other".into()));
            }
        });
    }

    #[test]
    fn tokei_showcase_contains_scan_lines_fast_paths() {
        let program = fixture(
            "showcase/tokei.xsh",
            include_str!("../../../../showcase/tokei.xsh"),
        );
        let scan_lines = program
            .store
            .tags
            .iter()
            .filter(|tag| **tag == FullTag::StmtScanLines)
            .count();
        assert!(
            scan_lines > 0,
            "Tokei showcase has no ScanLines instructions"
        );
    }

    #[test]
    fn tokei_showcase_contains_scan_bytes_fast_paths() {
        let program = fixture(
            "showcase/tokei.xsh",
            include_str!("../../../../showcase/tokei.xsh"),
        );
        let scan_bytes = program
            .store
            .tags
            .iter()
            .filter(|tag| **tag == FullTag::StmtScanBytes)
            .count();
        assert!(
            scan_bytes > 0,
            "Tokei showcase has no ScanBytes instructions"
        );
    }

    #[test]
    fn tokei_showcase_contains_direct_pure_calls() {
        let program = fixture(
            "showcase/tokei.xsh",
            include_str!("../../../../showcase/tokei.xsh"),
        );
        let direct_calls = program
            .store
            .tags
            .iter()
            .filter(|tag| **tag == FullTag::ExprDirectPureCall)
            .count();
        assert!(direct_calls > 0, "Tokei showcase has no direct pure calls");
    }

    fn normalize_traces(events: &[crate::trace::TraceEvent]) -> String {
        events
            .iter()
            .map(|event| {
                let mut payload = event.payload.clone();
                match &mut payload {
                    crate::trace::TracePayload::RunEnd { pid, .. }
                    | crate::trace::TracePayload::SpawnReady { pid, .. }
                    | crate::trace::TracePayload::WaitEnd { pid, .. }
                    | crate::trace::TracePayload::SpawnCancel { pid, .. }
                    | crate::trace::TracePayload::PipelineSegmentEnd { pid, .. } => {
                        *pid = None;
                    }
                    _ => {}
                }
                format!(
                    "{:?}",
                    (
                        event.event_id,
                        event.parent_event_id,
                        event.depth,
                        event.kind,
                        event.source_span,
                        &event.name,
                        &event.api_id,
                        payload,
                    )
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn run_full(
        program: Arc<FullProgram>,
        name: Name,
    ) -> (
        Result<Value, crate::runtime::value::RuntimeError>,
        Vec<u8>,
        String,
    ) {
        let mut evaluator =
            Evaluator::new_with_sources(Vec::new(), (*program.sources).clone()).with_tracing();
        evaluator.indexed_program = Some(Arc::clone(&program));
        let result = evaluator
            .call_indexed_direct(
                LoweredFunctionKey::Name(name),
                LoweredFunctionKind::Proc,
                &[],
                Span::new(program.store.source_id, 0, 0),
            )
            .expect("full indexed proc is installed");
        (
            result,
            std::mem::take(&mut evaluator.stdout),
            normalize_traces(&evaluator.trace_events),
        )
    }

    #[test]
    fn prepared_constant_pool_reuses_data_and_rejects_invalid_values() {
        run_with_large_stack(|| {
            let program = Arc::new(fixture("prepared-constant.xsh", "const data: List[Int] = [1, 2]\nproc constant() -> List[Int] { data }\n"));
            program.symbol_owner().with_current(|| {
                let header = program.function_view(LoweredFunctionKey::Name(Name::intern("constant")), LoweredFunctionKind::Proc).unwrap().unwrap().header().unwrap();
                assert!(header.captures.iter().all(|capture| capture.name != Name::intern("data")));
            });
            let (result, _, _) = run_full(Arc::clone(&program), program_name(&program, "constant"));
            assert_eq!(result.unwrap(), Value::List(vec![Value::Int(1), Value::Int(2)]));
            let shared = program.store.prepared_constants.iter().find_map(|constant| match &constant.0 {
                LoweredValue::SharedList(values) => Some(Arc::clone(values)), _ => None,
            }).expect("prepared list pool");
            let decoder = FullDecoder { store: &program.store, owner: 0, instruction_range: 0..0, instruction_states: None, block_states: None, slot_count: 0, pattern_tree: None, pattern_ceiling: Cell::new(usize::MAX), verified: false };
            let mut words = vec![0];
            let mut cursor = FullCursor::new(&words);
            let decoded = PreparedConstantValue::decode(&decoder, &mut cursor).unwrap();
            let LoweredValue::SharedList(values) = decoded.0 else { panic!("shared list"); };
            assert!(Arc::ptr_eq(&shared, &values));
            let mut broken = (*program).clone();
            broken.store.prepared_constants.clear();
            assert!(FullVerifier::verify(&broken).is_err());
            let mut broken = (*program).clone();
            broken.store.prepared_constants[0] = PreparedConstantValue(LoweredValue::Unit);
            assert!(FullVerifier::verify(&broken).is_err());
            let mut builder = FullBuilder::new(SourceId::new(0));
            let checkpoint = builder.checkpoint();
            PreparedConstantValue(LoweredValue::Int(1)).encode(&mut builder, &mut words).unwrap();
            builder.rewind(checkpoint);
            assert!(builder.store.prepared_constants.is_empty());
        });
    }

    #[test]
    fn prepared_cli_plan_pool_rewinds_with_builder_checkpoint() {
        run_with_large_stack(|| {
            let program = fixture("cli-constant-descriptors.xsh", include_str!("../../../../tests/fixtures/frontend-indexed/cli-constant-descriptors.xsh"));
            let plan = Arc::clone(&program.store.prepared_cli_plans[0]);
            let mut builder = FullBuilder::new(SourceId::new(0));
            let checkpoint = builder.checkpoint();
            plan.encode(&mut builder, &mut Vec::new()).unwrap();
            assert_eq!(builder.store.prepared_cli_plans.len(), 1);
            builder.rewind(checkpoint);
            assert!(builder.store.prepared_cli_plans.is_empty());
        });
    }

    #[test]
    fn prepared_regex_pool_rewinds_with_builder_checkpoint() {
        let mut builder = FullBuilder::new(SourceId::new(0));
        let checkpoint = builder.checkpoint();
        let value = RegexValue {
            pattern: "[a-z]+".to_string(),
            regex: Arc::new(crate::modules::regex::compile("[a-z]+", Span::new(SourceId::new(0), 0, 0)).unwrap()),
        };
        let mut words = Vec::new();
        value.encode(&mut builder, &mut words).unwrap();
        assert_eq!(builder.store.prepared_regexes.len(), 1);
        builder.rewind(checkpoint);
        assert!(builder.store.prepared_regexes.is_empty());
    }

    #[test]
    fn prepared_regex_pool_survives_frontend_and_evaluator_reuse() {
        run_with_large_stack(|| {
            let program = Arc::new(fixture("prepared-regex.xsh", "proc literal() -> Regex { rx\"[a-z]+\" }\n"));
            let prepared = Arc::clone(&program.store.prepared_regexes[0].regex);
            let name = program_name(&program, "literal");
            for _ in 0..3 {
                let (result, _, _) = run_full(Arc::clone(&program), name);
                let Value::Regex(value) = result.unwrap() else { panic!("expected prepared regex"); };
                assert!(Arc::ptr_eq(&prepared, &value.regex));
                assert!(value.regex.is_match("text"));
            }
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            for _ in 0..3 {
                let result = evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(name), LoweredFunctionKind::Proc, &[],
                    Span::new(program.store.source_id, 0, 0),
                ).unwrap().unwrap();
                let Value::Regex(value) = result else { panic!("expected prepared regex"); };
                assert!(Arc::ptr_eq(&prepared, &value.regex));
            }
            let mut broken = (*program).clone();
            broken.store.prepared_regexes.clear();
            assert!(FullVerifier::verify(&broken).is_err());
        });
    }

    #[test]
    fn direct_full_program_preserves_values_output_errors_and_traces() {
        run_with_large_stack(|| {
            let program = Arc::new(fixture("indexed-execution.xsh", INDEXED_EXECUTION));

            let main = program_name(&program, "main");
            let (result, stdout, traces) = run_full(Arc::clone(&program), main);
            assert_eq!(result.unwrap(), Value::ok(Value::Unit));
            assert_eq!(stdout, b"slice 13 120 true true\n");
            assert!(traces.contains("ProcEnter"));
            assert!(traces.contains("ProcExit"));

            let exact_error_site = program_name(&program, "exact_error_site");
            let (result, stdout, traces) = run_full(program, exact_error_site);
            let Value::Result(crate::runtime::value::ResultValue::Err(error)) = result.unwrap()
            else {
                panic!("exact error site must return Err");
            };
            assert_eq!(error.error_kind(), Some("bytes-unpack"));
            assert!(stdout.is_empty());
            assert!(traces.contains("ProcEnter"));
            assert!(traces.contains("ProcExit"));
        });
    }

    #[test]
    fn compact_entry_executes_after_all_frontend_and_adapter_scratch_is_dropped() {
        run_with_large_stack(|| {
            let program = {
                let mut sources = SourceMap::new();
                let source_id = sources.add_file("indexed-execution.xsh", INDEXED_EXECUTION);
                let parsed = Parser::parse_source_arena_only(source_id, INDEXED_EXECUTION);
                let declarations = Checker::check_compact_declarations(&parsed.arena);
                let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
                FullBuilder::build_compact(
                    &parsed.arena,
                    &declarations,
                    &bodies,
                    INDEXED_EXECUTION,
                    Arc::new(sources),
                    source_id,
                )
                .unwrap()
            };

            let main = program_name(&program, "main");
            let (result, stdout, _) = run_full(Arc::new(program), main);
            assert_eq!(result.unwrap(), Value::ok(Value::Unit));
            assert_eq!(stdout, b"slice 13 120 true true\n");
        });
    }

    #[test]
    fn full_indexed_layouts_keep_hot_rows_compact() {
        assert_eq!(size_of::<FullTag>(), 1);
        assert_eq!(size_of::<FullPatternTag>(), 1);
        assert_eq!(size_of::<FullStageTag>(), 1);
        assert_eq!(size_of::<FullValueTag>(), 1);
        assert_eq!(size_of::<FullBlock>(), 20);
        assert_eq!(size_of::<FullFunction>(), 32);
        assert_eq!(size_of::<FullParam>(), 12);
        assert_eq!(size_of::<FullParamCold>(), 12);
        assert_eq!(size_of::<FullCapture>(), 12);
        assert_eq!(size_of::<FullFunctionMetadata>(), 8);
        assert_eq!(size_of::<FullValidation>(), 8);
        assert_eq!(size_of::<FullDriverStep>(), 36);
        assert_eq!(size_of::<FullDriverSlot>(), 16);
        assert_eq!(size_of::<FullDriverSync>(), 12);
        assert_eq!(size_of::<FullDriverRegion>(), 20);
        assert_eq!(size_of::<FullDriverProgram>(), 20);
        assert_eq!(size_of::<BuildExprId>(), 4);
        assert_eq!(size_of::<BuildStmtId>(), 4);
        assert_eq!(size_of::<BuildPatternId>(), 4);
        assert_eq!(size_of::<BuildTopStmtId>(), 4);
        assert_eq!(
            size_of::<FullTag>() + size_of::<IrData>(),
            9,
            "full indexed instructions use one-byte tags and eight-byte data"
        );
    }

    #[test]
    fn generic_metadata_is_owned_by_the_encoded_function_and_operation() {
        use super::super::generic::{CallEvidence, ConcreteOperationId, GenericReturnPlan, Instantiation, QuantifierKind, RequirementWitness, SchemeScope, SolvedCall, SolvedRequirementUse, TypeRef};
        let mut program = fixture("generic-metadata.xsh", "pure add(left: Str, right: Str = \"z\") -> Str { left + right }\npure caller() -> Str { add(\"a\") }\n");
        assert_eq!(program.generic_evidence().unwrap().scopes().count(), 0);
        assert!(program.generic_evidence().unwrap().calls().is_empty());
        let find = |name: &str| program.store.functions.iter().position(|function| program.store.string(function.name).unwrap() == name).unwrap();
        let add = find("add"); let caller = find("caller");
        let add_owner = IrFunctionId::new(add).unwrap(); let caller_owner = IrFunctionId::new(caller).unwrap();
        let body_instruction = program.store.function_instruction_range(add).unwrap().find(|&instruction| program.store.tags[instruction] == FullTag::ExprBinary).unwrap();
        let call_instruction = program.store.function_instruction_range(caller).unwrap().find(|&instruction| matches!(program.store.tags[instruction], FullTag::ExprCall | FullTag::ExprDirectPureCall)).unwrap();
        let ty = program.store.semantic.signature_param(SignatureId::from_raw(program.store.functions[add].signature).unwrap(), 0).unwrap().1;
        let mut evidence = GenericEvidenceBuilder::default();
        let scope = evidence.add_scope(SchemeScope { owner: add_owner, quantifiers: Box::new([QuantifierKind::Type]), parameters: Box::new([TypeRef::Rigid(0), TypeRef::Rigid(0)]), parameter_names: Box::new([Name::intern("left"), Name::intern("right")]), parameter_flags: Box::new([0, 2]), kind: super::super::generic::CallableKind::Pure, result: TypeRef::Rigid(0), requirements: Box::new([Requirement::Add { left: TypeRef::Rigid(0), right: TypeRef::Rigid(0), result: TypeRef::Rigid(0) }]), return_plan: GenericReturnPlan::Value }).unwrap();
        let instance = evidence.add_instance(Instantiation { scope, substitutions: Box::new([ty]), parameter_types: Box::new([ty, ty]), result_type: ty, requirements: Box::new([RequirementWitness::Add { operation: ConcreteOperationId::AddStr, left: ty, right: ty, result: ty }]) }).unwrap();
        evidence.add_requirement_use(SolvedRequirementUse { instruction: body_instruction as u32, scope, requirement: 0 });
        evidence.add_call(SolvedCall { instruction: call_instruction as u32, caller: InstructionOwner::Function(caller_owner), target: scope, evidence: CallEvidence::Ground(instance) });
        let call_words = program.store.payload(program.store.data[call_instruction].range()).unwrap();
        let args_block = program.store.blocks[IrBlockId::from_raw(call_words[1]).unwrap().index()];
        let args = program.store.payload(args_block.instructions).unwrap();
        let argument_source = args[2] as usize;
        for parameter in 0..2 { evidence.add_argument(super::super::generic::SolvedArgument { call_instruction: call_instruction as u32, parameter, source_instruction: if args.get(1 + parameter as usize * 2) == Some(&0) { Some(args[2 + parameter as usize * 2]) } else { None }, ty: TypeRef::Ground(ty) }); }
        let owners = program.store.generic_instruction_owners().unwrap();
        program.store.generic = Some(Box::new(evidence.finish(&program.store.semantic, program.store.functions.len(), &owners).unwrap()));
        program.store.function_metadata[add].flags |= 4;
        program.store.functions[add].signature = IR_NONE;
        let params = program.store.functions[add].params.bounds(program.store.params.len()).unwrap();
        for param in &mut program.store.params[params] { param.type_id = IR_NONE; }
        FullVerifier::verify(&program).unwrap();
        let view = program.function_view_at(add).unwrap();
        assert_eq!(view.generic_scope(), Some(scope));
        assert!(view.execution().is_err());
        assert_eq!(view.with_instantiation(instance).unwrap().execution().unwrap().generic_scope(), Some(scope));
        assert_eq!(view.generic_evidence().unwrap().instance(instance).unwrap().scope, scope);
        let mut wrong_header = program.clone(); wrong_header.store.function_metadata[add].flags &= !4;
        assert!(FullVerifier::verify(&wrong_header).is_err());
        let mut wrong_nongeneric_plan = program.clone();
        wrong_nongeneric_plan.store.function_metadata[caller].flags |= generic_return_plan_flags(GenericReturnPlan::Result);
        assert!(FullVerifier::verify(&wrong_nongeneric_plan).unwrap_err().message.contains("nongeneric function carries generic return-plan flags"));
        let mut wrong_argument = program.clone();
        wrong_argument.store.tags[argument_source] = FullTag::ExprInt;
        assert!(FullVerifier::verify_generic_evidence(&wrong_argument.store).unwrap_err().message.contains("literal source"));
        let mut wrong_default = program.clone();
        let parameter = program.store.functions[add].params.start + 1;
        let default = program.store.param_cold.iter().find(|cold| cold.param == parameter).unwrap().default as usize;
        wrong_default.store.values[default] = FullValueTag::Int;
        assert!(FullVerifier::verify_generic_evidence(&wrong_default.store).unwrap_err().message.contains("literal default"));
        let mut wrong_operation = program.clone();
        let start = wrong_operation.store.data[body_instruction].lhs as usize;
        let op = wrong_operation.store.extra[start] as usize;
        wrong_operation.store.binary_ops[op] = BinaryOp::Sub;
        assert!(FullVerifier::verify_generic_evidence(&wrong_operation.store).unwrap_err().message.contains("another binary operator"));
        let mut wrong_target = program.clone();
        let start = wrong_target.store.data[call_instruction].lhs as usize;
        wrong_target.store.extra[start] = caller_owner.raw();
        assert!(FullVerifier::verify_generic_evidence(&wrong_target.store).unwrap_err().message.contains("encoded target"));
        let mut wrong_tag = program.clone(); wrong_tag.store.tags[call_instruction] = FullTag::ExprNull;
        assert!(FullVerifier::verify_generic_evidence(&wrong_tag.store).unwrap_err().message.contains("non-call"));
        let mut foreign_owner = program.clone(); foreign_owner.store.function_instruction_starts[caller] = body_instruction as u32;
        assert!(FullVerifier::verify_generic_evidence(&foreign_owner.store).is_err());
    }

    #[test]
    fn generic_builder_checkpoint_discards_new_roots_and_retires_reused_slots() {
        use super::super::generic::{GenericReturnPlan, QuantifierKind, SchemeScope, TypeRef};
        let make_scope = || SchemeScope { owner: IrFunctionId::new(0).unwrap(), quantifiers: Box::new([QuantifierKind::Type]), parameters: Box::new([TypeRef::Rigid(0)]), parameter_names: Box::new([Name::intern("value")]), parameter_flags: Box::new([0]), kind: super::super::generic::CallableKind::Pure, result: TypeRef::Rigid(0), requirements: Box::new([]), return_plan: GenericReturnPlan::Value };
        let source = "pure name(entry) { entry.name }";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let (declaration, callable) = bodies.solved.declarations.iter().next().unwrap();
        let expression = *bodies.solved.projections.keys().next().unwrap();
        let stage = crate::sema::check::StageIdentity { pipeline: expression, index: 0 };
        let mut builder = FullBuilder::new(SourceId::new(0));
        let empty = builder.checkpoint();
        let retired = builder.generic_evidence_mut().add_scope(make_scope()).unwrap();
        builder.generic_declarations.insert(*declaration, retired);
        builder.generic_schemes.insert(retired, callable.scheme);
        builder.generic_projection_uses.insert(expression, (retired, 0));
        builder.generic_expression_rows.push((0, expression, InstructionOwner::Function(IrFunctionId::new(0).unwrap())));
        builder.generic_stage_call_rows.push((0, stage, InstructionOwner::Function(IrFunctionId::new(0).unwrap())));
        builder.rewind(empty);
        assert!(builder.generic.is_none());
        assert!(builder.generic_declarations.is_empty());
        assert!(builder.generic_schemes.is_empty());
        assert!(builder.generic_projection_uses.is_empty());
        assert!(builder.generic_expression_rows.is_empty());
        assert!(builder.generic_stage_call_rows.is_empty());
        let replacement = builder.generic_evidence_mut().add_scope(make_scope()).unwrap();
        assert_ne!(retired, replacement);
        let retained = builder.checkpoint();
        let discarded = builder.generic_evidence_mut().add_scope(make_scope()).unwrap();
        builder.generic_declarations.insert(*declaration, discarded);
        builder.generic_schemes.insert(discarded, callable.scheme);
        builder.generic_projection_uses.insert(expression, (discarded, 0));
        builder.generic_expression_rows.push((0, expression, InstructionOwner::Function(IrFunctionId::new(0).unwrap())));
        builder.generic_stage_call_rows.push((0, stage, InstructionOwner::Function(IrFunctionId::new(0).unwrap())));
        builder.rewind(retained);
        let replaced = builder.generic_evidence_mut().add_scope(make_scope()).unwrap();
        assert_ne!(discarded, replaced);
        assert!(builder.generic_declarations.is_empty());
        assert!(builder.generic_schemes.is_empty());
        assert!(builder.generic_projection_uses.is_empty());
        assert!(builder.generic_expression_rows.is_empty());
        assert!(builder.generic_stage_call_rows.is_empty());
        assert!(builder.generic.as_ref().unwrap().scope(discarded).is_err());
    }

    #[test]
    fn compact_driver_executes_effects_after_arena_drop() {
        run_with_large_stack(|| {
            let (program, plan, mut evaluator) = {
                let mut sources = SourceMap::new();
                let source_id =
                    sources.add_file("top-level-driver-boundary.xsh", TOP_LEVEL_DRIVER_BOUNDARY);
                let parsed = Parser::parse_source_arena_only(source_id, TOP_LEVEL_DRIVER_BOUNDARY);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let declarations = Checker::check_compact_declarations(&parsed.arena);
                assert!(
                    declarations.diagnostics.is_empty(),
                    "{:?}",
                    declarations.diagnostics
                );
                let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
                assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
                let shared_sources = Arc::new(sources.clone());
                let program = FullBuilder::build_compact(
                    &parsed.arena,
                    &declarations,
                    &bodies,
                    TOP_LEVEL_DRIVER_BOUNDARY,
                    shared_sources,
                    source_id,
                )
                .unwrap();
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
                let plan = evaluator
                    .prepare_compact_indexed_only(&parsed.arena, source_id)
                    .expect("top-level driver boundary fixture is wholly lowerable");
                (Arc::new(program), plan, evaluator)
            };

            assert!(
                program
                    .store
                    .driver_steps
                    .iter()
                    .any(|step| { step.effects & (EFFECT_ENV | EFFECT_CWD | EFFECT_PROCESS) != 0 })
            );
            assert!(
                program
                    .store
                    .driver_steps
                    .iter()
                    .any(|step| step.effects & EFFECT_SIGNAL != 0)
            );
            assert!(
                program
                    .store
                    .driver_steps
                    .iter()
                    .any(|step| step.effects & EFFECT_DEFER != 0)
            );
            assert!(
                program
                    .store
                    .captures
                    .iter()
                    .any(|capture| { program.store.string(capture.name).ok() == Some("base") }),
                "top-level binding capture is stored by compact identity"
            );
            evaluator.indexed_program = Some(Arc::clone(&program));
            let output = match evaluator.eval_installed_compact_indexed_only(plan) {
                Ok(output) => output,
                Err(_) => panic!("indexed driver remains executable"),
            };
            assert_eq!(output.stdout, b"indexed\n4\n");
            assert!(output.stderr.is_empty());
            assert_eq!(output.status, 0);
            assert!(output.traceback.is_none());
        });
    }

    #[test]
    fn driver_verifier_rejects_effect_sync_and_owner_corruption() {
        let mut sources = SourceMap::new();
        let source_id =
            sources.add_file("top-level-driver-boundary.xsh", TOP_LEVEL_DRIVER_BOUNDARY);
        let parsed = Parser::parse_source_arena_only(source_id, TOP_LEVEL_DRIVER_BOUNDARY);
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        let program = FullBuilder::build_compact(
            &parsed.arena,
            &declarations,
            &bodies,
            TOP_LEVEL_DRIVER_BOUNDARY,
            Arc::new(sources),
            source_id,
        )
        .unwrap();

        let mut bad_effect = program.clone();
        bad_effect.store.driver_steps[0].effects ^= EFFECT_HOST;
        assert!(FullVerifier::verify(&bad_effect).is_err());

        let mut bad_sync = program.clone();
        bad_sync.store.driver_sync[0].flags ^= DRIVER_SLOT_WRITE;
        assert!(FullVerifier::verify(&bad_sync).is_err());

        let mut unreachable_slot = program.clone();
        unreachable_slot
            .store
            .driver_slots
            .push(unreachable_slot.store.driver_slots[0]);
        assert!(FullVerifier::verify(&unreachable_slot).is_err());

        let mut bad_owner = program;
        let block_index = bad_owner
            .store
            .blocks
            .iter()
            .position(|block| driver_owner_index(block.owner).is_some())
            .expect("boundary fixture owns driver blocks");
        let step = driver_owner_index(bad_owner.store.blocks[block_index].owner).unwrap();
        bad_owner.store.blocks[block_index].owner =
            driver_owner((step + 1) % bad_owner.store.driver_steps.len()).unwrap();
        assert!(FullVerifier::verify(&bad_owner).is_err());
    }

    #[test]
    fn driver_propagated_process_failure_records_process_propagation_and_trace_effects() {
        let source = "run false ?\n";
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("top-level-propagate.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        let program = FullBuilder::build_compact(
            &parsed.arena,
            &declarations,
            &bodies,
            source,
            Arc::new(sources.clone()),
            source_id,
        )
        .unwrap();
        let effects = program.store.driver_steps[0].effects;
        assert_eq!(
            effects & (EFFECT_PROCESS | EFFECT_PROPAGATE | EFFECT_TRACE),
            EFFECT_PROCESS | EFFECT_PROPAGATE | EFFECT_TRACE
        );
    }

    #[test]
    fn driver_rejects_non_skippable_unlowered_top_level_statement() {
        let source = "print \"boundary\"\n";
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("top-level-reject.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        let statements = parsed.arena.statement_ids().collect::<Vec<_>>();
        let lowered = ProgramBuild {
            statements: vec![None],
            scratch: Rc::new(RefCell::new(BuildScratch::default())),
            ..ProgramBuild::default()
        };
        let error = FullBuilder::build_with_driver(
            &[],
            Some((&lowered, &statements, &parsed.arena)),
            Arc::new(sources),
            source_id,
        )
        .unwrap_err();
        assert_eq!(error.construct, "top_level_boundary_blocker");
    }

    #[test]
    fn verifier_checks_assertion_children_locations_and_propagation_effects() {
        let program = fixture("assertion-ir.xsh", "pure value() -> Bool { false }\nproc check() [error] -> Unit { false }\nlet _ = value()\ntrue\n");
        let assertions: Vec<_> = program.store.tags.iter().enumerate().filter_map(|(index, tag)| (*tag == FullTag::ExprAssert).then_some(index)).collect();
        assert_eq!(assertions.len(), 2, "only statement consumers lower to assertions");
        assert!(program.store.driver_steps.iter().any(|step| step.effects & (EFFECT_PROPAGATE | EFFECT_TRACE) == (EFFECT_PROPAGATE | EFFECT_TRACE)));
        let mut bad_child = program.clone();
        let payload = bad_child.store.data[assertions[0]].range().bounds(bad_child.store.extra.len()).unwrap();
        bad_child.store.extra[payload.start] = u32::MAX;
        assert!(FullVerifier::verify(&bad_child).is_err());
        let mut bad_location = program;
        bad_location.store.extra[payload.start + 1] = u32::MAX;
        assert!(FullVerifier::verify(&bad_location).is_err());
    }

    #[test]
    fn verifier_rejects_empty_record_update_path_and_bad_replacement_reference() {
        run_with_large_stack(|| {
            let program = fixture("record-update.xsh", "pure value() -> Int { let base = {a: {b: 1, c: 0}}; return {...base, a.b: 2, a.c: 3}.a.b }\n");
            let instruction = program.store.tags.iter().position(|tag| *tag == FullTag::ExprRecordUpdate).unwrap();
            let payload = program.store.data[instruction].range().bounds(program.store.extra.len()).unwrap();
            let updates = IrBlockId::from_raw(program.store.extra[payload.start + 1]).unwrap();
            let entries = program.store.blocks[updates.index()].instructions.bounds(program.store.extra.len()).unwrap();
            let path = IrBlockId::from_raw(program.store.extra[entries.start + 1]).unwrap();
            let names = program.store.blocks[path.index()].instructions.bounds(program.store.extra.len()).unwrap();
            let mut empty = program.clone();
            empty.store.extra[names.start] = 0;
            empty.store.blocks[path.index()].instructions.len = 1;
            let error = FullVerifier::verify(&empty).unwrap_err();
            assert!(error.message.contains("nonempty and disjoint"), "{}", error.message);
            let mut bad_child = program.clone();
            bad_child.store.extra[entries.start + 2] = u32::MAX;
            assert!(FullVerifier::verify(&bad_child).is_err());
            let mut overlapping = program.clone();
            let second_path = IrBlockId::from_raw(program.store.extra[entries.start + 4]).unwrap();
            let second_names = program.store.blocks[second_path.index()].instructions.bounds(program.store.extra.len()).unwrap();
            overlapping.store.extra[second_names.start + 2] = program.store.extra[names.start + 2];
            let error = FullVerifier::verify(&overlapping).unwrap_err();
            assert!(error.message.contains("nonempty and disjoint"), "{}", error.message);
            let program = Arc::new(program);
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "value")),
                    LoweredFunctionKind::Pure, &[], Span::new(program.store.source_id, 0, 0),
                ).expect("update function exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert_eq!(result.unwrap(), Value::Int(2));
            }
        });
    }

    #[test]
    fn fs_root_methods_keep_opaque_identity_and_defaults_on_both_indexed_routes() {
        run_with_large_stack(|| {
            let source = include_str!("../../../../tests/fixtures/frontend-indexed/fs-root-methods.xsh");
            let program = Arc::new(fixture("fs-root-methods.xsh", source));
            FullVerifier::verify(&program).unwrap();
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "root_methods")),
                    LoweredFunctionKind::Proc, &[], Span::new(program.store.source_id, 0, 0),
                ).expect("filesystem root fixture exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert_eq!(result.unwrap(), Value::ok(Value::Str(Arc::from("payload"))));
            }
        });
    }

    #[test]
    fn cli_command_descriptor_plans_execute_both_indexed_routes() {
        run_with_large_stack(|| {
            let source = include_str!("../../../../tests/fixtures/frontend-indexed/cli-constant-commands.xsh");
            let program = fixture("cli-constant-commands.xsh", source);
            assert_eq!(program.store.prepared_cli_plans.len(), 1);
            assert!(program.store.prepared_cli_plans[0].matches_operation(RuntimeOp::CliCommands));
            assert!(!program.store.prepared_cli_plans[0].matches_operation(RuntimeOp::CliParse));
            FullVerifier::verify(&program).unwrap();
            let program = Arc::new(program);
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "command_values")),
                    LoweredFunctionKind::Proc, &[], Span::new(program.store.source_id, 0, 0),
                ).expect("command descriptor function exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert_eq!(result.unwrap(), Value::ok(Value::Str(Arc::from("workspace/build"))));
            }
        });
    }

    #[test]
    fn cli_descriptor_plans_share_preparation_and_execute_both_indexed_routes() {
        run_with_large_stack(|| {
            let source = include_str!("../../../../tests/fixtures/frontend-indexed/cli-constant-descriptors.xsh");
            let program = fixture("cli-constant-descriptors.xsh", source);
            assert_eq!(program.store.prepared_cli_plans.len(), 2);
            FullVerifier::verify(&program).unwrap();
            let instruction = program.store.tags.iter().position(|tag| *tag == FullTag::ExprModuleCall).unwrap();
            let payload = program.store.data[instruction].range().bounds(program.store.extra.len()).unwrap();
            let mut broken = program.clone();
            broken.store.extra[payload.start + 2] = u32::MAX;
            assert!(FullVerifier::verify(&broken).unwrap_err().message.contains("CLI descriptor plan is out of bounds"));
            let mut wrong_policy = program.clone();
            let applet = program.store.prepared_cli_plans.iter().position(|plan| plan.matches_operation(RuntimeOp::CliApplet)).unwrap();
            wrong_policy.store.extra[payload.start + 2] = applet as u32;
            assert!(FullVerifier::verify(&wrong_policy).unwrap_err().message.contains("operation policy"));
            let program = Arc::new(program);
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "descriptor_values")),
                    LoweredFunctionKind::Proc, &[], Span::new(program.store.source_id, 0, 0),
                ).expect("descriptor function exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert_eq!(result.unwrap(), Value::ok(Value::Str(Arc::from("6/4/3"))));
            }
        });
    }

    #[test]
    fn native_path_interpolation_executes_both_indexed_routes() {
        run_with_large_stack(|| {
            let source = "pure native_path(value: Path) -> Path { return fp\"prefix/${value}/../end\" }\nproc native_plan(value: Path) [process, error] -> Command { return process.command { stdin = fp\"before/${value}\"; stdout = fp\"${value}/after\"; run true \"--target=$value\" } }\n";
            let program = Arc::new(fixture("native-path-interpolation.xsh", source));
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let argument = Value::Path(PathValue::new(b"raw\xff name".to_vec()).unwrap());
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "native_path")),
                    LoweredFunctionKind::Pure, std::slice::from_ref(&argument), Span::new(program.store.source_id, 0, 0),
                ).expect("native path function exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert_eq!(result.unwrap(), Value::Path(PathValue::new(b"prefix/raw\xff name/../end".to_vec()).unwrap()));
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "native_plan")),
                    LoweredFunctionKind::Proc, std::slice::from_ref(&argument), Span::new(program.store.source_id, 0, 0),
                ).expect("native command plan exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                let Value::Command(plan) = result.unwrap() else { panic!("command plan"); };
                assert_eq!(plan.argv.last().unwrap(), b"--target=raw\xff name");
                let crate::runtime::value::CommandRedirection::File { path, .. } = &plan.redirections[0] else { panic!("file input redirection"); };
                assert_eq!(path.bytes, b"before/raw\xff name");
                let crate::runtime::value::CommandRedirection::File { path, .. } = &plan.redirections[1] else { panic!("file output redirection"); };
                assert_eq!(path.bytes, b"raw\xff name/after");
            }
        });
    }

    #[test]
    fn verifier_rejects_empty_or_unknown_comprehension_qualifiers() {
        let program = fixture("indexed-comprehension.xsh", "pure values() -> List[Int] { return [inner for outer in [1] if outer > 0 for inner in [outer]] }\n");
        let instruction = program.store.tags.iter().position(|tag| *tag == FullTag::ExprListComp).unwrap();
        let payload = program.store.data[instruction].range().bounds(program.store.extra.len()).unwrap();
        let block = IrBlockId::from_raw(program.store.extra[payload.start + 1]).unwrap();
        let entries = program.store.blocks[block.index()].instructions.bounds(program.store.extra.len()).unwrap();

        let mut empty = program.clone();
        empty.store.extra[entries.start] = 0;
        empty.store.blocks[block.index()].instructions.len = 1;
        let error = FullVerifier::verify(&empty).unwrap_err();
        assert!(error.message.contains("must start with for"), "{}", error.message);

        let mut unknown = program;
        unknown.store.extra[entries.start + 1] = u32::MAX;
        let error = FullVerifier::verify(&unknown).unwrap_err();
        assert!(error.message.contains("qualifier tag is invalid"), "{}", error.message);
    }

    #[test]
    fn verifier_rejects_cross_function_instruction_ownership() {
        let mut program = fixture("indexed-execution.xsh", INDEXED_EXECUTION);
        let target = program.store.function_instruction_starts[0];
        let body = IrBlockId::from_raw(program.store.functions[1].body).unwrap();
        let body_words = program.store.blocks[body.index()]
            .instructions
            .bounds(program.store.extra.len())
            .unwrap();
        assert!(body_words.len() >= 2);
        program.store.extra[body_words.start + 1] = target;

        let error = FullVerifier::verify(&program).unwrap_err();
        assert!(
            error.message.contains("another function"),
            "{}",
            error.message
        );
    }

    #[test]
    fn verifier_rejects_block_ownership_and_missing_function_terminators() {
        let program = fixture("indexed-execution.xsh", INDEXED_EXECUTION);

        let mut bad_owner = program.clone();
        let body = IrBlockId::from_raw(bad_owner.store.functions[0].body).unwrap();
        bad_owner.store.blocks[body.index()].owner = IrFunctionId::new(1).unwrap().raw();
        assert!(FullVerifier::verify(&bad_owner).is_err());

        let mut bad_terminator = program;
        let (function, return_instruction) = bad_terminator
            .store
            .functions
            .iter()
            .enumerate()
            .find_map(|(function, metadata)| {
                let block = IrBlockId::from_raw(metadata.body)?;
                let words = bad_terminator.store.blocks[block.index()]
                    .instructions
                    .bounds(bad_terminator.store.extra.len())?;
                (words.len() == 2).then(|| {
                    let instruction = bad_terminator.store.extra[words.start + 1] as usize;
                    (bad_terminator.store.tags[instruction] == FullTag::StmtReturn)
                        .then_some((function, instruction))
                })?
            })
            .expect("indexed execution fixture has a single-return function");
        bad_terminator.store.tags[return_instruction] = FullTag::StmtBreakValue;
        let error = FullVerifier::verify(&bad_terminator).unwrap_err();
        assert!(
            error.message.contains("does not terminate"),
            "function {function}: {}",
            error.message
        );
    }

    #[test]
    fn pattern_aliases_and_alternatives_publish_only_complete_capture_sets() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/pattern-aliases.xsh");
        let program = fixture("pattern-aliases.xsh", source);
        program.symbol_owner().with_current(|| {
            let view = program.function_view(LoweredFunctionKey::Name(Name::intern("select")), LoweredFunctionKind::Pure).unwrap().unwrap();
            let execution = view.execution().unwrap();
            let mut slots = vec![LoweredValue::Unit; view.header().unwrap().slot_count];
            let pattern = program.store.patterns.iter().position(|tag| *tag == FullPatternTag::Alias).unwrap() as u32;
            let span = Span::new(crate::source::SourceId::new(0), 0, 0);
            let failed = LoweredValue::List(vec![LoweredValue::List(vec![LoweredValue::Int(1)]), LoweredValue::List(vec![LoweredValue::Int(2)])]);
            assert!(!Evaluator::indexed_pattern_matches(&execution, pattern, &failed, &mut slots, span).unwrap());
            assert!(slots.iter().all(|slot| matches!(slot, LoweredValue::Unit)));
            let matched = LoweredValue::List(vec![LoweredValue::List(vec![LoweredValue::Int(99)]), LoweredValue::List(vec![LoweredValue::Int(7), LoweredValue::Int(8)])]);
            assert!(Evaluator::indexed_pattern_matches(&execution, pattern, &matched, &mut slots, span).unwrap());
            assert!(slots.iter().any(|slot| *slot == LoweredValue::Int(7)));
            assert!(slots.iter().any(|slot| *slot == LoweredValue::List(vec![LoweredValue::Int(8)])));
            assert!(slots.iter().any(|slot| *slot == matched));
        });
    }

    #[test]
    fn pattern_aliases_compact_facts_preserve_captured_subject_and_element_types() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/pattern-aliases.xsh");
        let parsed = Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty());
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        parsed.arena.symbol_owner().with_current(|| {
            for (id, ty) in &bodies.expr_types {
                match parsed.arena.arena.expr(*id).kind {
                    crate::syntax::arena::ArenaExprKind::Ident(name) if name == "value" || name == "left" || name == "right" => assert_eq!(*ty, Type::Int),
                    crate::syntax::arena::ArenaExprKind::Ident(name) if name == "tail" => assert_eq!(*ty, Type::List(Box::new(Type::Int))),
                    crate::syntax::arena::ArenaExprKind::Ident(name) if name == "original" => assert_eq!(*ty, Type::List(Box::new(Type::List(Box::new(Type::Int))))),
                    _ => {}
                }
            }
        });
    }

    #[test]
    fn verifier_rejects_incompatible_alternative_and_alias_capture_slots() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/pattern-aliases.xsh");
        let program = fixture("pattern-aliases.xsh", source);
        let alias = program.store.patterns.iter().position(|tag| *tag == FullPatternTag::Alias).unwrap();
        let alias_payload = program.store.pattern_data[alias].range().bounds(program.store.extra.len()).unwrap();
        let mut bad_alias = program.clone();
        bad_alias.store.extra[alias_payload.start + 1] = u32::MAX;
        assert!(FullVerifier::verify(&bad_alias).is_err());
        let mut alias_cycle = program.clone();
        alias_cycle.store.extra[alias_payload.start] = alias as u32;
        assert!(FullVerifier::verify(&alias_cycle).unwrap_err().message.contains("nested pattern"));
        let bind = program.store.patterns.iter().rposition(|tag| *tag == FullPatternTag::Bind).unwrap();
        let range = program.store.pattern_data[bind].range().bounds(program.store.extra.len()).unwrap();
        let mut duplicate = program.clone();
        duplicate.store.extra[range.start] = 0;
        assert!(FullVerifier::verify(&duplicate).is_err());
    }

    #[test]
    fn direct_scalar_iteration_runs_after_frontend_drop() {
        run_with_large_stack(|| {
            let source = include_str!("../../../../tests/fixtures/frontend-indexed/scalar-iteration.xsh");
            let program = Arc::new(fixture("scalar-iteration.xsh", source));
            for recursive in [false, true] {
                let execute = |name: &str| {
                    let name = program_name(&program, name);
                    let work = || run_full(program.clone(), name).0.unwrap();
                    if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(work) } else { work() }
                };
                for (name, expected) in [("scalar_count", 261), ("scalar_comp", 3)] {
                    assert_eq!(execute(name), Value::Int(expected));
                }
                let Value::Result(crate::runtime::value::ResultValue::Ok(value)) = execute("scalar_result") else { panic!("Result iterable") };
                assert_eq!(*value, Value::Int(255));
                let Value::Result(crate::runtime::value::ResultValue::Err(error)) = execute("scalar_failure") else { panic!("source failure") };
                let Value::Error(error) = *error else { panic!("nominal source failure") };
                assert_eq!(error.kind, "ScalarFailure.Missing");
                assert_eq!(error.contexts.iter().map(|context| context.message.as_deref()).collect::<Vec<_>>(), [Some("scalar source")]);
            }
        });
    }

    #[test]
    fn lexical_ctx_indexed_execution_preserves_context_order_and_region_spans() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/error-context.xsh");
        let program = Arc::new(fixture("error-context.xsh", source));
        let value = program_name(&program, "contextual_value");
        assert_eq!(run_full(program.clone(), value).0.unwrap(), Value::Int(7));
        let failure = program_name(&program, "contextual_failure");
        let Value::Result(crate::runtime::value::ResultValue::Err(error)) = run_full(program, failure).0.unwrap() else { panic!("expected propagated error") };
        let Value::Error(error) = *error else { panic!("expected error") };
        assert_eq!(error.contexts.iter().map(|context| context.message.as_deref()).collect::<Vec<_>>(), vec![Some("inner"), Some("outer")]);
        assert!(error.contexts.iter().all(|context| context.span.is_some_and(|span| source.get(span.range()).is_some_and(|text| text.starts_with("ctx ")))));
        assert_eq!(error.kind, "validation");
    }

    #[test]
    fn list_patterns_validate_before_publishing_capture_slots() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/list-patterns.xsh");
        let program = fixture("list-patterns.xsh", source);
        program.symbol_owner().with_current(|| {
            let view = program.function_view(LoweredFunctionKey::Name(Name::intern("nested")), LoweredFunctionKind::Pure).unwrap().unwrap();
            let execution = view.execution().unwrap();
            let count = view.header().unwrap().slot_count;
            let pattern = program.store.patterns.iter().rposition(|tag| *tag == FullPatternTag::List).unwrap() as u32;
            let span = Span::new(crate::source::SourceId::new(0), 0, 0);
            let list = |last| LoweredValue::List(vec![LoweredValue::List(vec![LoweredValue::Int(1), LoweredValue::Int(2)]), LoweredValue::List(vec![LoweredValue::Int(last)])]);
            let mut slots = vec![LoweredValue::Unit; count];
            assert!(!Evaluator::indexed_pattern_matches(&execution, pattern, &list(3), &mut slots, span).unwrap());
            assert!(slots.iter().all(|value| matches!(value, LoweredValue::Unit)));
            assert!(Evaluator::indexed_pattern_matches(&execution, pattern, &list(99), &mut slots, span).unwrap());
            assert!(slots.iter().any(|value| *value == LoweredValue::Int(1)));
            assert!(slots.iter().any(|value| *value == LoweredValue::List(vec![LoweredValue::Int(2)])));
        });
    }

    #[test]
    fn verifier_rejects_binding_and_missing_retry_selection_patterns() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/selective-retry.xsh");
        let program = fixture("selective-retry.xsh", source);
        FullVerifier::verify(&program).unwrap();
        let retry = program.store.tags.iter().position(|tag| *tag == FullTag::ExprRetry).unwrap();
        let payload = program.store.data[retry].range().bounds(program.store.extra.len()).unwrap();
        let binding = program.store.patterns.iter().position(|tag| *tag == FullPatternTag::ResultOk).unwrap();
        let mut invalid = program.clone();
        invalid.store.extra[payload.start + 2] = binding as u32;
        assert!(FullVerifier::verify(&invalid).unwrap_err().message.contains("cannot bind"));
        let mut missing = program.clone();
        missing.store.extra[payload.start + 2] = u32::MAX;
        assert!(FullVerifier::verify(&missing).is_err());
    }

    #[test]
    fn verifier_rejects_invalid_list_rest_patterns() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/list-patterns.xsh");
        let program = fixture("list-patterns.xsh", source);
        let parent = program.store.patterns.iter().position(|tag| *tag == FullPatternTag::List).unwrap();
        let range = program.store.pattern_data[parent].range().bounds(program.store.extra.len()).unwrap();
        let mut invalid = program.clone();
        invalid.store.extra[range.start + 2] = parent as u32;
        assert!(FullVerifier::verify(&invalid).unwrap_err().message.contains("list rest"));
        let mut missing = program.clone();
        missing.store.extra[range.start + 2] = u32::MAX;
        assert!(FullVerifier::verify(&missing).is_err());
    }

    #[test]
    fn verifier_checks_with_binding_slots_and_both_branch_returns() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/block-parameters.xsh");
        let program = fixture("block-parameters.xsh", source);
        FullVerifier::verify(&program).unwrap();
        let row = program.store.tags.iter().position(|tag| *tag == FullTag::StmtWith).unwrap();
        let payload = program.store.data[row].range().bounds(program.store.extra.len()).unwrap();
        let bindings = IrBlockId::from_raw(program.store.extra[payload.start]).unwrap();
        let bindings = program.store.blocks[bindings.index()].instructions.bounds(program.store.extra.len()).unwrap();
        let mut bad_slot = program.clone();
        bad_slot.store.extra[bindings.start + 1] = u32::MAX;
        assert!(FullVerifier::verify(&bad_slot).unwrap_err().message.contains("slot"));
        assert!(indexed_stmt_can_return(&program.store, row).unwrap());
    }

    #[test]
    fn verifier_checks_pattern_condition_capture_slots_and_branch_returns() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/pattern-conditionals.xsh");
        let program = fixture("pattern-conditionals.xsh", source);
        FullVerifier::verify(&program).unwrap();
        let row = program.store.tags.iter().position(|tag| *tag == FullTag::StmtPatternWhile).unwrap();
        let payload = program.store.data[row].range().bounds(program.store.extra.len()).unwrap();
        let capture_block = IrBlockId::from_raw(program.store.extra[payload.start + 2]).unwrap();
        let captures = program.store.blocks[capture_block.index()].instructions.bounds(program.store.extra.len()).unwrap();
        let mut bad_slot = program.clone();
        bad_slot.store.extra[captures.start + 1] = u32::MAX;
        assert!(FullVerifier::verify(&bad_slot).unwrap_err().message.contains("slot"));
    }

    #[test]
    fn verifier_rejects_nested_pattern_cycles_and_missing_children() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/pattern-tests.xsh");
        let program = fixture("pattern-tests.xsh", source);
        let parent = program.store.patterns.iter().position(|tag| *tag == FullPatternTag::ResultTest).unwrap();
        let range = program.store.pattern_data[parent].range().bounds(program.store.extra.len()).unwrap();
        let mut cycle = program.clone();
        cycle.store.extra[range.start + 1] = parent as u32;
        assert!(FullVerifier::verify(&cycle).unwrap_err().message.contains("nested pattern"));
        let mut missing = program.clone();
        missing.store.extra[range.start + 1] = u32::MAX;
        assert!(FullVerifier::verify(&missing).is_err());
    }

    #[test]
    fn verifier_rejects_slot_pattern_function_stage_and_location_bounds() {
        let program = fixture("indexed-execution.xsh", INDEXED_EXECUTION);

        let mut bad_slot = program.clone();
        let slot = bad_slot
            .store
            .tags
            .iter()
            .position(|tag| *tag == FullTag::ExprParam)
            .unwrap();
        let slot_payload = bad_slot.store.data[slot]
            .range()
            .bounds(bad_slot.store.extra.len())
            .unwrap();
        bad_slot.store.extra[slot_payload.start] = u32::MAX;
        assert!(FullVerifier::verify(&bad_slot).is_err());

        let mut bad_pattern = program.clone();
        let pattern = bad_pattern
            .store
            .patterns
            .iter()
            .position(|tag| *tag == FullPatternTag::Bind)
            .unwrap();
        let pattern_payload = bad_pattern.store.pattern_data[pattern]
            .range()
            .bounds(bad_pattern.store.extra.len())
            .unwrap();
        bad_pattern.store.extra[pattern_payload.start] = u32::MAX;
        assert!(FullVerifier::verify(&bad_pattern).is_err());

        let mut bad_function = program.clone();
        let call = bad_function
            .store
            .tags
            .iter()
            .position(|tag| matches!(tag, FullTag::ExprCall | FullTag::ExprDirectPureCall))
            .unwrap();
        let call_payload = bad_function.store.data[call]
            .range()
            .bounds(bad_function.store.extra.len())
            .unwrap();
        bad_function.store.extra[call_payload.start] = u32::MAX;
        assert!(FullVerifier::verify(&bad_function).is_err());

        let mut bad_stage = program.clone();
        let pipeline = bad_stage
            .store
            .tags
            .iter()
            .position(|tag| *tag == FullTag::ExprPipeline)
            .unwrap();
        let pipeline_payload = bad_stage.store.data[pipeline]
            .range()
            .bounds(bad_stage.store.extra.len())
            .unwrap();
        assert!(pipeline_payload.len() >= 3);
        bad_stage.store.extra[pipeline_payload.start + 2] = u32::MAX;
        assert!(FullVerifier::verify(&bad_stage).is_err());

        let mut bad_location = program;
        bad_location.store.locations[0].start = u32::MAX;
        assert!(FullVerifier::verify(&bad_location).is_err());
    }

    #[test]
    fn constant_key_projection_preserves_index_and_get_both_routes() {
        run_with_large_stack(|| {
            let source = "type Config = {workers: Int}\nconst field = \"workers\"\npure counts() -> Int {\n  let config: Config = {workers: 4}\n  (config.get(field) ?? 0) + config[field]\n}\n";
            let program = Arc::new(fixture("constant-key-projection.xsh", source));
            assert!(program.store.tags.contains(&FullTag::ExprIndex));
            assert!(program.store.tags.contains(&FullTag::ExprMethod));
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "counts")), LoweredFunctionKind::Pure, &[],
                    Span::new(program.store.source_id, 0, 0),
                ).expect("known field function exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert_eq!(result.unwrap(), Value::Int(8));
            }
        });
    }

    #[test]
    fn uint_mutation_checks_execute_both_indexed_routes_after_frontend_drop() {
        run_with_large_stack(|| {
            let program = Arc::new(fixture("uint-mutation.xsh", include_str!("../../../../tests/fixtures/frontend-indexed/uint-mutation.xsh")));
            FullVerifier::verify(&program).unwrap();
            let instruction = program.store.tags.iter().position(|tag| *tag == FullTag::ExprCheckedValue).expect("constructors and method operands retain checked values");
            let words = program.store.data[instruction].range().bounds(program.store.extra.len()).unwrap();
            let mut malformed = (*program).clone();
            malformed.store.extra[words.start + 1] = u32::MAX;
            assert!(FullVerifier::verify(&malformed).is_err(), "checked value types must refer to the semantic pool");
            for recursive in [false, true] {
                for name in ["scalar_failure", "compound_failure", "record_failure", "list_failure", "map_failure", "append_failure", "valid_updates", "argument_failure", "return_failure", "tail_failure", "default_failure", "list_default_failure", "list_return_failure", "map_return_failure", "record_return_failure", "list_argument_failure", "map_argument_failure", "record_argument_failure", "map_default_failure", "record_default_failure", "result_return_failure", "producer_failure", "nested_producer_failure", "accept", "negative_return", "defaulted", "list_defaulted", "map_defaulted", "record_defaulted", "tag_failure", "error_failure", "method_list_failure", "method_map_failure", "method_fallback_failure", "method_map_push_failure", "inferred_if_failure", "inferred_match_failure", "builtin_creation_failure", "branch_creation_failure"] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let args = if matches!(name, "argument_failure" | "return_failure" | "list_return_failure" | "map_return_failure" | "record_return_failure" | "list_argument_failure" | "map_argument_failure" | "record_argument_failure" | "result_return_failure" | "producer_failure" | "nested_producer_failure" | "accept" | "negative_return" | "tag_failure" | "error_failure" | "method_list_failure" | "method_map_failure" | "method_fallback_failure" | "method_map_push_failure" | "inferred_if_failure" | "inferred_match_failure" | "builtin_creation_failure" | "branch_creation_failure") { vec![Value::Int(-1)] } else { Vec::new() };
                    let kind = if matches!(name, "producer_failure" | "nested_producer_failure") { LoweredFunctionKind::Proc } else { LoweredFunctionKind::Pure };
                    let mut call = || evaluator.call_indexed_direct(LoweredFunctionKey::Name(program_name(&program, name)), kind, &args, Span::new(program.store.source_id, 0, 0)).expect("UInt function exists");
                    let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                    if name == "valid_updates" { assert_eq!(result.unwrap(), Value::Int(8)); }
                    else {
                        let error = result.unwrap_err();
                        assert_eq!(error.kind, "type-error", "{name}");
                        assert!(error.message.contains("UInt") || error.message.contains("UnsignedRow"), "{name}: {}", error.message);
                    }
                }
            }
        });
    }

    #[test]
    fn typed_map_keys_execute_both_indexed_routes() {
        run_with_large_stack(|| {
            let source = "pure counts() -> Int {\n  var values: Map[Int, Int] = {[20]: 2, [3]: 1}\n  let older = values\n  values[3] = 9\n  let keys: List[Int] = values.keys()\n  return keys[0] + older[3] + (values.get(3) ?? 0)\n}\n";
            let program = fixture("typed-map.xsh", source);
            let program = Arc::new(program);
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut call = || evaluator.call_indexed_direct(LoweredFunctionKey::Name(program_name(&program, "counts")), LoweredFunctionKind::Pure, &[], Span::new(program.store.source_id, 0, 0)).expect("typed Map function exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert_eq!(result.unwrap(), Value::Int(13));
            }
        });
    }

    #[test]
    fn absence_lookups_execute_nullable_and_integer_slots_after_frontend_drop() {
        run_with_large_stack(|| {
            let source = "pure nullable(text: Str, position: Int) -> Int? { let byte = text.byte_at(position); byte }\npure sentinel(text: Str, position: Int) -> Int { let byte = text.byte_at(position) ?? -1; byte }\npure find(text: Str, position: Int) -> Int? { text.find(\":\", position) }\npure present() -> Int? { let entries: Map[Int?] = {present: null}; entries.get(\"present\") ?? 7 }\n";
            let program = fixture("absence-lookups.xsh", source);
            assert!(program.store.tags.contains(&FullTag::ExprStrByteAt));
            assert!(program.store.tags.contains(&FullTag::IntStrByteAtSlot));
            let program = Arc::new(program);
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                for (name, text, index, expected) in [
                    ("nullable", "é", 0, Value::Int(195)), ("nullable", "é", 2, Value::Null),
                    ("nullable", "é", -1, Value::Null), ("sentinel", "é", 0, Value::Int(195)),
                    ("sentinel", "é", 2, Value::Int(-1)), ("find", ":x", 0, Value::Int(0)),
                    ("find", "é:x", 0, Value::Int(2)), ("find", "é:x", 3, Value::Null),
                ] {
                    let args = [Value::Str(Arc::from(text)), Value::Int(index)];
                    let mut call = || evaluator.call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)), LoweredFunctionKind::Pure, &args,
                        Span::new(program.store.source_id, 0, 0),
                    ).expect("lookup function exists");
                    let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                    assert_eq!(result.unwrap(), expected, "{name} index {index} recursive {recursive}");
                }
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "present")), LoweredFunctionKind::Pure, &[],
                    Span::new(program.store.source_id, 0, 0),
                ).expect("present function exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert_eq!(result.unwrap(), Value::Null);
            }
        });
    }

    #[test]
    fn map_literals_verify_entry_flags_and_execute_both_indexed_routes() {
        run_with_large_stack(|| {
            let source = "pure counts() -> Int {\n  let source: Map[Int] = {beta: 3}\n  let values: Map[Int] = {[\"alpha\"]: 1, alpha: 2, ...source}\n  return (values.get(\"alpha\") ?? 0) + (values.get(\"beta\") ?? 0)\n}\n";
            let program = fixture("computed-map.xsh", source);
            let instruction = program.store.tags.iter().position(|tag| *tag == FullTag::ExprMapLiteral).expect("Map literal instruction");
            let payload = program.store.data[instruction].range().bounds(program.store.extra.len()).unwrap();
            let block = IrBlockId::from_raw(program.store.extra[payload.start]).unwrap();
            let entries = program.store.blocks[block.index()].instructions.bounds(program.store.extra.len()).unwrap();
            let mut bad_flag = program.clone();
            bad_flag.store.extra[entries.start + 1] = 2;
            assert!(FullVerifier::verify(&bad_flag).is_err());
            let mut bad_key = program.clone();
            bad_key.store.extra[entries.start + 2] = u32::MAX;
            assert!(FullVerifier::verify(&bad_key).is_err());
            let program = Arc::new(program);
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "counts")), LoweredFunctionKind::Pure, &[],
                    Span::new(program.store.source_id, 0, 0),
                ).expect("Map function exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert_eq!(result.unwrap(), Value::Int(5));
            }
        });
    }

    #[test]
    fn list_assignment_verifies_paths_and_executes_both_routes() {
        run_with_large_stack(|| {
            let source = include_str!("../../../../tests/fixtures/frontend-indexed/list-assignment.xsh");
            let program = fixture("list-assignment.xsh", source);
            let instruction = program.store.tags.iter().position(|tag| *tag == FullTag::StmtAssignPath).unwrap();
            let payload = program.store.data[instruction].range().bounds(program.store.extra.len()).unwrap();
            let block = IrBlockId::from_raw(program.store.extra[payload.start + 1]).unwrap();
            let steps = program.store.blocks[block.index()].instructions.bounds(program.store.extra.len()).unwrap();
            let mut malformed = program.clone();
            malformed.store.extra[steps.start + 1] = u32::MAX;
            assert!(FullVerifier::verify(&malformed).unwrap_err().message.contains("assignment path step"));
            let mut bad_index = program.clone();
            bad_index.store.extra[steps.start + 2] = u32::MAX;
            assert!(FullVerifier::verify(&bad_index).is_err());
            let mut empty = program.clone();
            empty.store.extra[steps.start] = 0;
            assert!(FullVerifier::verify(&empty).is_err());
            let program = Arc::new(program);
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "updated")),
                    LoweredFunctionKind::Pure, &[], Span::new(program.store.source_id, 0, 0),
                ).expect("assignment function exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert_eq!(result.unwrap(), Value::Int(16));
            }
        });
    }

    #[test]
    fn list_splicing_verifies_payload_and_executes_both_indexed_routes() {
        run_with_large_stack(|| {
            let source = "pure spliced() -> Int {\n  let middle = [2, 3]\n  let result = [1, @[], @middle, 4]\n  return result[0] + result[3]\n}\n";
            let program = fixture("list-splicing.xsh", source);
            let instruction = program.store.tags.iter().position(|tag| *tag == FullTag::ExprListBuild).expect("mixed list instruction");
            let payload = program.store.data[instruction].range().bounds(program.store.extra.len()).unwrap();
            let block = IrBlockId::from_raw(program.store.extra[payload.start]).unwrap();
            let elements = program.store.blocks[block.index()].instructions.bounds(program.store.extra.len()).unwrap();
            let mut malformed = program.clone();
            malformed.store.extra[elements.start + 1] = 2;
            assert!(FullVerifier::verify(&malformed).unwrap_err().message.contains("boolean payload"));
            let mut bad_child = program.clone();
            bad_child.store.extra[elements.start + 2] = u32::MAX;
            assert!(FullVerifier::verify(&bad_child).is_err());
            let program = Arc::new(program);
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "spliced")),
                    LoweredFunctionKind::Pure, &[], Span::new(program.store.source_id, 0, 0),
                ).expect("splice function exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert_eq!(result.unwrap(), Value::Int(5));
            }
        });
    }

    #[test]
    fn try_capture_verifies_body_ownership_and_preserves_result_data_on_both_routes() {
        run_with_large_stack(|| {
            let program = fixture("try-capture.xsh", "proc capture() [error] -> Result[Int] {\n  let nested = try { Ok(7) }?\n  nested\n}\n");
            let instruction = program.store.tags.iter().position(|tag| *tag == FullTag::ExprCapture).expect("capture instruction");
            let payload = program.store.data[instruction].range().bounds(program.store.extra.len()).unwrap();
            let mut missing_body = program.clone();
            missing_body.store.extra[payload.start] = u32::MAX;
            assert!(FullVerifier::verify(&missing_body).is_err());
            let block = IrBlockId::from_raw(program.store.extra[payload.start]).unwrap();
            let mut wrong_kind = program.clone();
            wrong_kind.store.blocks[block.index()].flags = BLOCK_LIST;
            assert!(FullVerifier::verify(&wrong_kind).is_err());
            let program = Arc::new(program);
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "capture")), LoweredFunctionKind::Proc,
                    &[], Span::new(program.store.source_id, 0, 0),
                ).expect("capture function exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert_eq!(result.unwrap(), Value::Result(crate::runtime::value::ResultValue::Ok(Box::new(Value::Int(7)))));
            }
        });
    }

    #[test]
    fn stage_named_configuration_verifies_children_and_executes_both_indexed_routes() {
        run_with_large_stack(|| {
            let source = r#"
proc configured() [] -> Int {
  let limits = {count: 2, max_bytes: 4, max_argv: false}
  let batches = ["aa", "bb", "cc"] |> batch(...limits)
  let mode = true
  let totals = [1, 2] |> reduce-by(sum: mode) { |item| {key: "all", value: item} }
  let columns = ["name"]
  [{name: "row"}] |> table.print(columns:)
  return batches.len() + (totals.get("all") ?? 0)
}
"#;
            let program = fixture("stage-named-configuration.xsh", source);
            for (tag, configuration_offset) in [(FullStageTag::BatchLimits, 0), (FullStageTag::ReduceByConfigured, 3), (FullStageTag::TablePrintConfigured, 0)] {
                let stage = program.store.stages.iter().position(|actual| *actual == tag).expect("configured stage opcode");
                let payload = program.store.stage_data[stage].range().bounds(program.store.extra.len()).unwrap();
                let mut malformed = program.clone();
                malformed.store.extra[payload.start + configuration_offset] = u32::MAX;
                assert!(FullVerifier::verify(&malformed).is_err());
            }
            let program = Arc::new(program);
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "configured")),
                    LoweredFunctionKind::Proc, &[], Span::new(program.store.source_id, 0, 0),
                ).expect("configuration function exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert_eq!(result.unwrap(), Value::Int(5));
                assert!(String::from_utf8(evaluator.stdout.clone()).unwrap().contains("row"));
            }
        });
    }

    #[test]
    fn core_assert_verifier_rejects_invalid_message_presence_and_expression() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/assert.xsh");
        let program = fixture("assert.xsh", source);
        let assertion = program.store.tags.iter().position(|tag| *tag == FullTag::StmtAssert).unwrap();
        let payload = program.store.data[assertion].range().bounds(program.store.extra.len()).unwrap();
        let mut invalid_presence = program.clone();
        invalid_presence.store.extra[payload.start + 1] = 2;
        assert!(FullVerifier::verify(&invalid_presence).unwrap_err().message.contains("optional payload"));
        let mut invalid_message = program.clone();
        invalid_message.store.extra[payload.start + 2] = u32::MAX;
        assert!(FullVerifier::verify(&invalid_message).is_err());
        let mut statement_message = program;
        statement_message.store.extra[payload.start + 2] = assertion as u32;
        assert!(FullVerifier::verify(&statement_message).is_err());
    }

    #[test]
    fn core_assert_executes_lazy_context_and_preserves_diagnostics_on_both_routes() {
        run_with_large_stack(|| {
            let source = include_str!("../../../../tests/fixtures/frontend-indexed/assert.xsh");
            let program = Arc::new(fixture("assert.xsh", source));
            for recursive in [false, true] {
                for (name, detail) in [("passes", None), ("fails", Some("1 == 2")), ("chain_fails", Some("3 < 2")), ("short_circuit_fails", Some("right operand skipped"))] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let mut call = || evaluator.call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)),
                        LoweredFunctionKind::Pure, &[], Span::new(program.store.source_id, 0, 0),
                    ).expect("assertion function exists");
                    let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                    if let Some(detail) = detail {
                        let error = result.expect_err("false assertion fails");
                        assert_eq!(error.family, "AssertionError");
                        assert_eq!(error.variant, "Failed");
                        assert_eq!(error.kind, "AssertionError.Failed");
                        assert!(error.message.contains(detail), "{}", error.message);
                        assert!(error.message.contains("context"));
                        assert!(!error.message.contains("division-by-zero"));
                    } else {
                        assert_eq!(result.unwrap(), Value::ok(Value::Int(7)));
                    }
                }
            }
        });
    }

    #[test]
    fn comparison_chain_verifier_rejects_short_chains_and_non_ordering_pairs() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/comparison-chain.xsh");
        let program = fixture("comparison-chain.xsh", source);
        let chain = program.store.tags.iter().position(|tag| *tag == FullTag::ExprComparisonChain).unwrap();
        let payload = program.store.data[chain].range().bounds(program.store.extra.len()).unwrap();
        let block = IrBlockId::from_raw(program.store.extra[payload.start]).unwrap();
        let pairs = program.store.blocks[block.index()].instructions.bounds(program.store.extra.len()).unwrap();
        let mut short = program.clone();
        short.store.extra[pairs.start] = 1;
        assert!(FullVerifier::verify(&short).unwrap_err().message.contains("at least two pairs"));
        let first_pair = program.store.extra[pairs.start + 1] as usize;
        let pair_payload = program.store.data[first_pair].range().bounds(program.store.extra.len()).unwrap();
        let mut mixed = program;
        let op = mixed.store.extra[pair_payload.start] as usize;
        mixed.store.binary_ops[op] = BinaryOp::Eq;
        assert!(FullVerifier::verify(&mixed).unwrap_err().message.contains("ordering operators"));
    }

    #[test]
    fn comparison_chain_assertion_reports_only_evaluated_failed_pair_on_both_routes() {
        run_with_large_stack(|| {
            let source = include_str!("../../../../tests/fixtures/frontend-indexed/comparison-chain.xsh");
            let mut program = fixture("comparison-chain.xsh", source);
            for (index, tag) in program.store.tags.iter().enumerate() {
                if *tag == FullTag::ExprComparisonChain {
                    let payload = program.store.data[index].range().bounds(program.store.extra.len()).unwrap();
                    program.store.extra[payload.start + 1] = 1;
                }
            }
            FullVerifier::verify(&program).unwrap();
            let program = Arc::new(program);
            for recursive in [false, true] {
                for (name, message) in [("skipped", "3 < 2"), ("failed_last", "2 < 1")] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let mut call = || evaluator.call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)),
                        LoweredFunctionKind::Pure, &[], Span::new(program.store.source_id, 0, 0),
                    ).expect("chain function exists");
                    let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                    let error = result.expect_err("asserted chain fails");
                    assert_eq!(error.kind, "AssertionError.Failed");
                    assert_eq!(error.family, "AssertionError");
                    assert_eq!(error.variant, "Failed");
                    assert!(error.message.contains(message), "{}", error.message);
                    let failed_source = &source[error.span.unwrap().range()];
                    assert_eq!(failed_source, message);
                    assert!(!error.message.contains("division"));
                }
            }
        });
    }

    #[test]
    fn record_proof_precise_types_and_unreachable_fallback_survive_frontend_drop() {
        run_with_large_stack(|| {
            let source = include_str!("../../../../tests/fixtures/frontend-indexed/proof-provenance.xsh");
            let program = Arc::new(fixture("proof-provenance.xsh", source));
            FullVerifier::verify(&program).unwrap();
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "verified")),
                    LoweredFunctionKind::Pure, &[], Span::new(program.store.source_id, 0, 0),
                ).expect("proof function exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert_eq!(result.unwrap(), Value::Str("ready".into()));
            }
        });
    }

    #[test]
    fn duration_arithmetic_retains_checked_operands_after_frontend_drop_on_both_routes() {
        run_with_large_stack(|| {
            let source = include_str!("../../../../tests/fixtures/frontend-indexed/duration-arithmetic.xsh");
            let program = Arc::new(fixture("duration-arithmetic.xsh", source));
            FullVerifier::verify(&program).unwrap();
            for recursive in [false, true] {
                let duration = |millis| Value::Duration(crate::runtime::value::DurationValue { millis });
                for (name, args, expected) in [
                    ("intervals", vec![duration(7000), duration(2000)], Value::Int(3)),
                    ("pause", vec![duration(250), Value::Int(3)], duration(751)),
                ] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let mut call = || evaluator.call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)), LoweredFunctionKind::Pure,
                        &args, Span::new(program.store.source_id, 0, 0),
                    ).expect("Duration function exists");
                    let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                    assert_eq!(result.unwrap(), expected);
                }
                for (name, code, expression) in [
                    ("underflow", "duration-underflow", "0ms - 1ms"),
                    ("overflow", "duration-overflow", "18446744073709551615ms + 1ms"),
                    ("count_overflow", "integer-overflow", "18446744073709551615ms / 1ms"),
                ] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let mut call = || evaluator.call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)), LoweredFunctionKind::Pure,
                        &[], Span::new(program.store.source_id, 0, 0),
                    ).expect("Duration function exists");
                    let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                    let error = result.expect_err("checked arithmetic failure");
                    assert_eq!(error.kind, code);
                    assert_eq!(&source[error.span.unwrap().range()], expression);
                }
            }
        });
    }

    #[test]
    fn wire_enum_preparation_survives_frontend_drop_and_executes_both_routes() {
        run_with_large_stack(|| {
            let source = include_str!("../../../../tests/fixtures/frontend-indexed/wire-enums.xsh");
            let program = Arc::new(fixture("wire-enums.xsh", source));
            assert_eq!(program.store.wire_enums.len(), 1, "constructors share one declaring mapping");
            FullVerifier::verify(&program).unwrap();
            let mut malformed = (*program).clone();
            Arc::make_mut(&mut malformed.store.wire_enums[0]).variants.insert(program_name(&program, "Duplicate"), Arc::from("ready"));
            assert!(FullVerifier::verify(&malformed).unwrap_err().message.contains("duplicate strings"));
            let mut missing = (*program).clone();
            missing.store.wire_enums.clear();
            assert!(FullVerifier::verify(&missing).is_err());
            let mut mismatched = (*program).clone();
            mismatched.store.prepared_schemas[0] = Arc::new(super::super::super::require::PreparedSchema::Validate(Type::Int));
            assert!(FullVerifier::verify(&mismatched).unwrap_err().message.contains("does not match"));
            let mut missing_schema = (*program).clone();
            missing_schema.store.prepared_schemas.clear();
            assert!(FullVerifier::verify(&missing_schema).is_err());
            fn change_mapping(schema: &mut super::super::super::require::PreparedSchema) -> bool {
                use super::super::super::require::PreparedSchema;
                match schema {
                    PreparedSchema::WireEnum(mapping) => {
                        let mapping = Arc::make_mut(mapping);
                        *mapping.variants.values_mut().next().unwrap() = Arc::from("different");
                        true
                    }
                    PreparedSchema::Record(fields) => fields.iter_mut().any(|(_, schema)| change_mapping(Arc::make_mut(schema))),
                    PreparedSchema::List(schema) | PreparedSchema::Map(_, schema) | PreparedSchema::Optional(schema) => change_mapping(Arc::make_mut(schema)),
                    PreparedSchema::Validate(_) => false,
                }
            }
            let mut contradictory = (*program).clone();
            assert!(change_mapping(Arc::make_mut(&mut contradictory.store.prepared_schemas[0])));
            assert!(FullVerifier::verify(&contradictory).is_err(), "schema and constructor mappings must agree");
            let mut duplicate = (*program).clone();
            duplicate.store.wire_enums.push(duplicate.store.wire_enums[0].clone());
            assert!(FullVerifier::verify(&duplicate).unwrap_err().message.contains("multiple mappings"));
            let mut changed_constant = (*program).clone();
            let tag = changed_constant.store.prepared_constants.iter_mut().find_map(|value| match &mut value.0 {
                LoweredValue::Tag(tag) if tag.wire.is_some() => Some(tag),
                _ => None,
            }).expect("prepared enum constant");
            *Arc::make_mut(tag.wire.as_mut().unwrap()).variants.values_mut().next().unwrap() = Arc::from("forged");
            assert!(FullVerifier::verify(&changed_constant).unwrap_err().message.contains("contradictory"));
            let raw = "{\"state\":\"ready\",\"values\":[\"\"],\"optional\":null}";
            for recursive in [false, true] {
                for (name, arguments, expected) in [
                    ("wire_direct", Vec::new(), "\"ready\""),
                    ("wire_prepared", Vec::new(), "\"ready\""),
                    ("wire_typed_map", Vec::new(), "[\"ready\",\"\"]"),
                    ("wire_round_trip", vec![Value::Str(Arc::from(raw))], "{\"optional\":null,\"state\":\"ready\",\"values\":[\"\"]}"),
                    ("wire_nested", vec![Value::Str(Arc::from(format!("{{\"packet\":{raw}}}")))], "{\"packet\":{\"optional\":null,\"state\":\"ready\",\"values\":[\"\"]}}"),
                ] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let mut call = || evaluator.call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)), LoweredFunctionKind::Pure,
                        &arguments, Span::new(program.store.source_id, 0, 0),
                    ).expect("wire enum function exists");
                    let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                    assert_eq!(result.unwrap(), Value::ok(Value::Str(Arc::from(expected))));
                }
            }
        });
    }

    #[test]
    fn inferred_require_targets_survive_frontend_drop_on_both_routes() {
        run_with_large_stack(|| {
            let source = include_str!("../../../../tests/fixtures/frontend-indexed/inferred-require.xsh");
            let program = Arc::new(fixture("inferred-require.xsh", source));
            FullVerifier::verify(&program).unwrap();
            assert_eq!(program.store.prepared_schemas.iter().filter(|schema| matches!(schema.as_ref(), super::super::super::require::PreparedSchema::Record(_))).count(), 1, "both validation sites share the same checked record schema: {:?}", program.store.prepared_schemas);
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let raw = program.symbols.with_current(|| Value::Record(BTreeMap::from([(Arc::from("jobs"), Value::Int(4))]).into()));
                let arguments = vec![raw];
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "inferred_requirement_via_parameter")), LoweredFunctionKind::Pure,
                    &arguments, Span::new(program.store.source_id, 0, 0),
                ).expect("prepared function exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert_eq!(result.unwrap(), Value::ok(Value::Int(4)));
            }
        });
    }

    #[test]
    fn locations_preserve_imported_source_identity() {
        let mut sources = SourceMap::new();
        let root_id = sources.add_file("root.xsh", "use module\n");
        let module_id = sources.add_file("module.xsh", "print 1\n");
        let mut builder = FullBuilder::new(root_id);
        let root_location = builder.intern_location(Span::new(root_id, 0, 3)).unwrap();
        let module_location = builder.intern_location(Span::new(module_id, 0, 5)).unwrap();
        assert_ne!(root_location.raw(), module_location.raw());
        assert_eq!(builder.store.location_sources, vec![root_id, module_id]);

        let program = FullProgram {
            store: builder.store,
            source_lowering_stats: crate::runtime::eval::FrontendLoweredStats::default(),
            sources: Arc::new(sources),
            symbols: crate::symbol::SymbolOwner::new(),
            function_definition_spans: Vec::new(),
            headers: Vec::new(),
            identities: std::sync::OnceLock::new(),
        };
        FullVerifier::verify(&program).unwrap();

        let mut bad_source = program;
        bad_source.store.location_sources[1] = SourceId::new(2);
        assert!(FullVerifier::verify(&bad_source).is_err());
    }

    #[test]
    fn compact_driver_executes_loaded_module_programs_directly() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("xsh-full-driver-use-{stamp}"));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("compact_import.xsh"),
            "##! Imported label provider.\n\
             let suffix = \"!\"\n\
             ## Build a label with the retained suffix.\n\
             export pure label(value: Str) -> Str {\n\
               return value + suffix\n\
             }\n",
        )
        .unwrap();
        let script = root.join("main.xsh");
        fs::write(
            &script,
            "use compact_import\n\
             print \"loaded\"\n",
        )
        .unwrap();

        let script_text = script.to_string_lossy().into_owned();
        let (sources, parsed) = crate::loader::parse_script(&script_text).unwrap();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.arena.span_source_id.unwrap();
        let source = sources.get(source_id).unwrap().text().to_string();
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        assert!(
            declarations.diagnostics.is_empty(),
            "{:?}",
            declarations.diagnostics
        );
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let program = FullBuilder::build_compact(
            &parsed.arena,
            &declarations,
            &bodies,
            &source,
            Arc::new(sources),
            source_id,
        )
        .unwrap();
        let module_source = parsed
            .arena
            .module_statements(&parsed.arena.modules[0])
            .next()
            .map(|statement| parsed.arena.arena.stmt(statement).span.source_id)
            .unwrap();
        assert!(program.store.location_sources.contains(&module_source));
        let child_steps = program.store.driver_programs[0]
            .steps
            .bounds(program.store.driver_steps.len())
            .unwrap();
        assert!(
            program.store.driver_steps[child_steps].iter().any(|step| {
                step.tag == FullDriverTag::Skip
                    && IrLocationId::from_raw(step.location).is_some_and(|location| {
                        program.store.location_sources[location.index()] == module_source
                    })
            }),
            "declaration-only imported rows remain explicit compact skips"
        );

        let program = Arc::new(program);
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
        evaluator.install_compact_runtime_declarations(&declarations);
        evaluator.indexed_program = Some(Arc::clone(&program));
        for index in 0..program.driver_step_count().unwrap() {
            evaluator
                .eval_indexed_driver_step(index, Span::new(source_id, 0, 0))
                .expect("verified driver step has a direct executor")
                .unwrap();
        }
        assert_eq!(evaluator.stdout, b"loaded\n");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn yield_delegation_verifies_operands_and_preserves_nominal_error_payload() {
        run_with_large_stack(|| {
            let source = include_str!("../../../../tests/fixtures/frontend-indexed/yield-delegation.xsh");
            let program = fixture("yield-delegation.xsh", source);
            let instruction = program.store.tags.iter().position(|tag| *tag == FullTag::StmtYieldDelegate).unwrap();
            let payload = program.store.data[instruction].range().bounds(program.store.extra.len()).unwrap();
            let mut malformed = program.clone();
            malformed.store.extra[payload.start] = u32::MAX;
            assert!(FullVerifier::verify(&malformed).is_err());
            let mut malformed = program.clone();
            malformed.store.extra[payload.start + 1] = u32::MAX;
            assert!(FullVerifier::verify(&malformed).is_err());
            let program = Arc::new(program);
            program.symbol_owner().with_current(|| {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let span = Span::new(program.store.source_id, 0, 0);
                let value = evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "parent")), LoweredFunctionKind::Pure, &[], span,
                ).expect("producer function").expect("producer creation");
                let Value::Stream(mut stream) = value else { panic!("producer returns stream"); };
                for expected in [1, 2, 3] {
                    assert_eq!(evaluator.stream_next(&mut stream, span).unwrap(), Some(Value::Int(expected)));
                }
                let error = evaluator.stream_next(&mut stream, span).expect_err("late delegated error");
                assert_eq!(error.family, "RowsError");
                assert_eq!(error.variant, "Late");
                assert_eq!(error.payload.get("row"), Some(&Value::Int(4)));
                assert_eq!(source[error.span.unwrap().range()].trim(), "fail()?");
                assert_eq!(evaluator.stream_next(&mut stream, span).unwrap(), None);
            });
        });
    }

    #[test]
    fn accept_policy_operands_are_verified_on_capture_spawn_and_command_rows() {
        run_with_large_stack(|| {
            let program = fixture("accept-policy-rows.xsh", r#"
proc checked() [process, error] {
  let text = run.text --accept=[0,1] sh -c "exit 1" ?
  let child = spawn run --accept=[0,1] sh -c "exit 1" ?
  let command = process.command_argv("sh", ["sh", "-c", "exit 1"], accept: [0,1])
  print $text $child.pid
  let status = process.run(command)?
}
"#);
            FullVerifier::verify(&program).unwrap();
            for (tag, value_offset) in [(FullTag::ExprRunCapture, 4), (FullTag::ExprSpawnRun, 2), (FullTag::ExprProcessCommandArgv, 2)] {
                let instruction = program.store.tags.iter().position(|actual| *actual == tag).expect("process row");
                let payload = program.store.data[instruction].range().bounds(program.store.extra.len()).unwrap();
                assert_eq!(program.store.extra[payload.end - value_offset - 1], 1, "policy is present");
                let mut invalid_value = program.clone();
                invalid_value.store.extra[payload.end - value_offset] = u32::MAX;
                assert!(FullVerifier::verify(&invalid_value).is_err());
                let mut invalid_presence = program.clone();
                invalid_presence.store.extra[payload.end - value_offset - 1] = 2;
                assert!(FullVerifier::verify(&invalid_presence).is_err());
            }
        });
    }

    #[test]
    fn context_scope_escape_walks_long_shared_error_causes_without_a_depth_limit() {
        use crate::runtime::value::RuntimeError;

        let symbols = crate::symbol::SymbolOwner::default();
        let _symbols = symbols.enter();
        let mut leaf = RuntimeError::new("inner", "resource payload");
        leaf.payload = crate::runtime::value::RecordMap::from([("job".into(),
            Value::NetJob(Box::new(crate::runtime::value::NetJobValue { id: 7 })))]);
        let mut chain = Value::Error(Box::new(leaf));
        for _ in 0..100_000 {
            chain = Value::Error(Box::new(RuntimeError::new("outer", "translation")))
                .with_error_cause(chain).unwrap();
        }
        let process = Value::RunError(Box::new(crate::runtime::value::RunError::from_status(
            crate::runtime::process::ProcessStatus::exited(7))))
            .with_error_cause(chain).unwrap();
        let shared = Value::List(vec![process.clone(), process]);
        assert!(Evaluator::context_scope_runtime_value_escapes(&shared));
        assert!(Evaluator::context_scope_value_escapes(&LoweredValue::ResultErr(Box::new(shared))));
    }

    #[test]
    fn context_scope_command_capture_tails_execute_after_frontend_drop_on_both_routes() {
        run_with_large_stack(|| {
            let source = include_str!("../../../../tests/fixtures/frontend-indexed/context-scope-capture-tails.xsh");
            let program = Arc::new(fixture("context-scope-capture-tails.xsh", source));
            FullVerifier::verify(&program).unwrap();
            for recursive in [false, true] {
                for (name, expected, stdout) in [
                    ("text_tail", Some(Value::ok(Value::Str(Arc::from("text")))), b"".as_slice()),
                    ("bytes_tail", Some(Value::ok(Value::Bytes(b"bytes".to_vec()))), b"bytes\n".as_slice()),
                    ("record_tail", Some(Value::ok(Value::Str(Arc::from("record")))), b"".as_slice()),
                    ("nested_tail", Some(Value::ok(Value::ok(Value::Str(Arc::from("nested"))))), b"".as_slice()),
                    ("discarded_tail", Some(Value::ok(Value::Unit)), b"".as_slice()),
                    ("failed_tail", None, b"".as_slice()),
                ] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone())
                        .with_env_var(b"XSH_CAPTURE_TAIL".to_vec(), b"outer".to_vec());
                    let original_cwd = evaluator.cwd.clone();
                    let original_env = evaluator.env.snapshot_clone();
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let mut call = || evaluator.call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)), LoweredFunctionKind::Proc, &[],
                        Span::new(program.store.source_id, 0, 0),
                    ).expect("capture tail fixture function exists");
                    let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() }.unwrap();
                    if let Some(expected) = expected { assert_eq!(result, expected, "{name}"); }
                    else { assert!(matches!(result, Value::Result(crate::runtime::value::ResultValue::Err(_))), "{name}: {result:?}"); }
                    assert_eq!(evaluator.stdout, stdout, "{name}");
                    assert_eq!(evaluator.cwd, original_cwd, "{name}");
                    assert_eq!(evaluator.env.snapshot_clone(), original_env, "{name}");
                }
            }
        });
    }

    #[test]
    fn context_scope_dynamic_outer_assignment_is_rejected_on_both_routes() {
        run_with_large_stack(|| {
            let source = r#"stream rows() [] -> Stream[Int] { yield 1 }
proc hidden() [] -> Any { rows() }
proc scoped() [env, error] -> Int {
  var output: Any = null
  let ignored = env ({XSH_SCOPE_ASSIGNMENT: "inner"}) { output = hidden(); 7 }
  99
}
proc local() [env, error] -> Int {
  env ({XSH_SCOPE_ASSIGNMENT: "inner"}) { var output: Any = null; output = hidden(); 7 }?
}
"#;
            let program = Arc::new(fixture("context-scope-assignment.xsh", source));
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone())
                    .with_env_var(b"XSH_SCOPE_ASSIGNMENT".to_vec(), b"outer".to_vec());
                let original_env = evaluator.env.snapshot_clone();
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "scoped")), LoweredFunctionKind::Proc, &[],
                    Span::new(program.store.source_id, 0, 0),
                ).expect("scope function exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert_eq!(result.unwrap_err().kind, "context-scope-escape");
                assert_eq!(evaluator.env.snapshot_clone(), original_env);
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "local")), LoweredFunctionKind::Proc, &[],
                    Span::new(program.store.source_id, 0, 0),
                ).expect("scope function exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert_eq!(result.unwrap(), Value::Int(7));
                assert_eq!(evaluator.env.snapshot_clone(), original_env);
            }
        });
    }

    #[test]
    fn context_scope_force_abort_restores_evaluator_state_on_both_routes() {
        run_with_large_stack(|| {
            let source = r#"proc fail() [io, error] -> Int { abort(23, force: true); 9 }
proc scoped() [io, error] -> Int {
  let ignored = cd (p"/") {
    let ignored = env ({XSH_FORCE_SCOPE: "inner"}) { fail() }
    7
  }
  99
}
"#;
            let program = Arc::new(fixture("context-scope-force.xsh", source));
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone())
                    .with_env_var(b"XSH_FORCE_SCOPE".to_vec(), b"outer".to_vec())
                    .with_env_var(b"XSH_RAW_INHERITED".to_vec(), b"raw\xff bytes".to_vec());
                let original_cwd = evaluator.cwd.clone();
                let original_env = evaluator.env.snapshot_clone();
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "scoped")), LoweredFunctionKind::Proc, &[],
                    Span::new(program.store.source_id, 0, 0),
                ).expect("scope function exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert!(result.unwrap_err().abort.is_some_and(|signal| signal.force));
                assert_eq!(evaluator.cwd, original_cwd);
                assert_eq!(evaluator.env.snapshot_clone(), original_env);
            }
        });
    }

    #[test]
    fn context_scope_verifies_payload_and_preserves_native_environment_bytes() {
        run_with_large_stack(|| {
            let source = "proc selected(value: Path) [env, error] -> Result[Int] { env ({XSH_NATIVE_SCOPE: value}) { 7 } }\n";
            let program = fixture("context-scope-native.xsh", source);
            let instruction = program.store.tags.iter().position(|tag| *tag == FullTag::ExprContextScope).expect("scope instruction");
            let payload = program.store.data[instruction].range().bounds(program.store.extra.len()).unwrap();
            let mut bad_kind = program.clone();
            bad_kind.store.extra[payload.start] = 2;
            assert!(FullVerifier::verify(&bad_kind).is_err());
            let mut bad_input = program.clone();
            bad_input.store.extra[payload.start + 1] = u32::MAX;
            assert!(FullVerifier::verify(&bad_input).is_err());
            let program = Arc::new(program);
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone())
                    .with_env_var(b"XSH_RAW_INHERITED".to_vec(), b"raw\xff bytes".to_vec());
                let original_env = evaluator.env.snapshot_clone();
                evaluator.indexed_program = Some(Arc::clone(&program));
                let argument = Value::Path(PathValue::new(b"raw\xfe name".to_vec()).unwrap());
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "selected")), LoweredFunctionKind::Proc, &[argument.clone()],
                    Span::new(program.store.source_id, 0, 0),
                ).expect("scope function exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert!(result.is_ok(), "{result:?}");
                assert_eq!(evaluator.env.snapshot_clone(), original_env);
            }
        });
    }

    #[test]
    fn context_scope_fatal_body_restores_state_on_both_routes() {
        run_with_large_stack(|| {
            let source = r#"proc fatal() [env, error] -> Int {
  let ignored = cd (p"/") {
    let ignored = env ({XSH_FATAL_SCOPE: "inner"}) { let zero = 0; 1 / zero }
    7
  }
  99
}
"#;
            let program = Arc::new(fixture("context-scope-fatal.xsh", source));
            for recursive in [false, true] {
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone())
                    .with_env_var(b"XSH_FATAL_SCOPE".to_vec(), b"outer".to_vec());
                let original_cwd = evaluator.cwd.clone();
                let original_env = evaluator.env.snapshot_clone();
                evaluator.indexed_program = Some(Arc::clone(&program));
                let mut call = || evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "fatal")), LoweredFunctionKind::Proc, &[],
                    Span::new(program.store.source_id, 0, 0),
                ).expect("scope function exists");
                let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                assert!(result.unwrap_err().abort.is_none());
                assert_eq!(evaluator.cwd, original_cwd);
                assert_eq!(evaluator.env.snapshot_clone(), original_env);
            }
        });
    }

    #[test]
    fn default_parameter_calls_preserve_both_indexed_execution_routes_after_frontend_drop() {
        run_with_large_stack(|| {
            let source = include_str!("../../../../tests/fixtures/frontend-indexed/default-parameters.xsh");
            let program = Arc::new(fixture("default-parameters.xsh", source));
            FullVerifier::verify(&program).unwrap();
            for recursive in [false, true] {
                for (name, expected) in [("choose", 4), ("nested", 5), ("supplied", 9), ("alias_default", 4), ("alias_named", 11)] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let mut call = || evaluator.call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)), LoweredFunctionKind::Pure,
                        &[], Span::new(program.store.source_id, 0, 0),
                    ).expect("defaulted function exists");
                    let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                    assert_eq!(result.unwrap(), Value::Int(expected));
                }
                for (name, succeeds) in [("caught", false), ("skipped", true)] {
                    let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                    evaluator.indexed_program = Some(Arc::clone(&program));
                    let mut call = || evaluator.call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)), LoweredFunctionKind::Pure,
                        &[], Span::new(program.store.source_id, 0, 0),
                    ).expect("defaulted Result function exists");
                    let result = if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(call) } else { call() };
                    assert!(matches!(result.unwrap(), Value::Result(crate::runtime::value::ResultValue::Ok(_))) == succeeds);
                }
            }
        });
    }

    #[test]
    fn default_parameter_verifier_rejects_missing_or_inconsistent_entry_metadata() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/default-parameters.xsh");
        let program = fixture("default-parameters.xsh", source);
        let parameter = program.store.params.iter().position(|param| param.flags & 4 != 0).unwrap();
        let mut missing_default = program.clone();
        missing_default.store.params[parameter].flags &= !2;
        assert!(FullVerifier::verify(&missing_default).unwrap_err().message.contains("non-rest defaulted parameter entry"));
        let mut inconsistent_entry = program.clone();
        inconsistent_entry.store.params[parameter].flags &= !4;
        assert!(FullVerifier::verify(&inconsistent_entry).unwrap_err().message.contains("entry does not match"));
        let mut rest_default = program;
        rest_default.store.params[parameter].flags |= 1;
        assert!(FullVerifier::verify(&rest_default).unwrap_err().message.contains("non-rest defaulted parameter entry"));
    }

}
