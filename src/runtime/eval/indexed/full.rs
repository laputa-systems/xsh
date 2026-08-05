use super::semantic::{SemanticPoolBuilder, SemanticPools};
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
    LoweredCompTarget, LoweredErrorExpr, LoweredErrorPatternFields, LoweredFmtPart,
    LoweredFunctionKey, LoweredFunctionKind, LoweredFunctionUnit, LoweredModuleExport,
    LoweredModuleExportKind, LoweredPipelineStage, LoweredProcessCommandArgv,
    LoweredProcessCommandBuilderEntry, LoweredRecordEntry, LoweredReturnKind, LoweredRunArg,
    LoweredRunArgKind, LoweredRunCapture, LoweredRunEnv, LoweredRunPipelineSegment,
    LoweredRunRedirection, LoweredSpawnRun, LoweredStatsValue, LoweredStrPredicate,
    LoweredTagValue, LoweredTopLevelSlot, LoweredTopLevelSlots, LoweredType, LoweredTypeCheck,
    LoweredValue, ProgramBuild, ReduceByOp, ScanBytes, ScanCheck, ScanCondition,
};
use crate::runtime::value::{DurationValue, FloatValue, FunctionName, PathValue};
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
use std::cell::RefCell;
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
    ExprPath,
    ExprFunctionRef,
    ExprPathFrom,
    ExprParam,
    ExprBinary,
    ExprIf,
    ExprMatch,
    ExprStrMatch,
    ExprTagMatch,
    ExprResultFallback,
    ExprFmtString,
    ExprPathFmtString,
    ExprGlob,
    ExprLastStatus,
    ExprRecord,
    ExprList,
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
    ExprContains,
    ExprRegexCompile,
    ExprRequire,
    ExprRunCapture,
    ExprRunPipeline,
    ExprSpawnRun,
    ExprSpawnCommand,
    ExprWait,
    ExprLoop,
    ExprRetry,
    ExprFsFiles,
    ExprFsWalk,
    ExprFsList,
    ExprFsTempDir,
    ExprFsWrite,
    ExprFsMkdir,
    ExprFsRemove,
    ExprFsCloseRoot,
    ExprFsRootPath,
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
    ExprHashVerifyFile,
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
    ExprDynamicCall,
    ExprSelfCall,
    StmtLet,
    StmtGuard,
    StmtLetInt,
    StmtLetBool,
    StmtAssign,
    StmtAssignField,
    StmtAssignFieldInt,
    StmtAssignIndex,
    StmtAssignInt,
    StmtAssignBool,
    StmtExpr,
    StmtIf,
    StmtIfBool,
    StmtWhile,
    StmtWhileBool,
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
    StmtBreak,
    StmtBreakValue,
    StmtContinue,
    StmtDefer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(in crate::runtime::eval) enum FullPatternTag {
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
    Shuffle,
    Fold,
    ReduceBy,
    ParMap,
    ParMapBlock,
    ParMapFlatMapReduceBy,
    Tee,
    Each,
    TablePrint,
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
    signature: SignatureId,
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
    type_id: TypeId,
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
    locations: Vec<IrLocation>,
    location_sources: Vec<SourceId>,
    runtime_ops: Vec<RuntimeOp>,
    assign_ops: Vec<AssignOp>,
    binary_ops: Vec<BinaryOp>,
    run_kinds: Vec<RunKind>,
    redirection_kinds: Vec<RedirectionKind>,
    semantic: SemanticPools,
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
            locations: Vec::new(),
            location_sources: Vec::new(),
            runtime_ops: Vec::new(),
            assign_ops: Vec::new(),
            binary_ops: Vec::new(),
            run_kinds: Vec::new(),
            redirection_kinds: Vec::new(),
            semantic: SemanticPools::default(),
        }
    }
}

impl FullStore {
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
        self.locations.shrink_to_fit();
        self.location_sources.shrink_to_fit();
        self.runtime_ops.shrink_to_fit();
        self.assign_ops.shrink_to_fit();
        self.binary_ops.shrink_to_fit();
        self.run_kinds.shrink_to_fit();
        self.redirection_kinds.shrink_to_fit();
        self.semantic.shrink_to_fit();
    }
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct FullProgram {
    store: FullStore,
    sources: Arc<SourceMap>,
    symbols: crate::symbol::SymbolOwner,
}

#[derive(Clone, Copy)]
pub(in crate::runtime::eval) struct FullFunctionView<'a> {
    program: &'a FullProgram,
    index: usize,
}

#[derive(Clone, Copy)]
pub(in crate::runtime::eval) struct FullDriverStepView<'a> {
    program: &'a FullProgram,
    index: usize,
}

impl FullProgram {
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
            .map(|param| lowered_type_from_type(&self.store.semantic.to_type(param.type_id)?))
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }

    pub(in crate::runtime::eval) fn function_view(
        &self,
        key: LoweredFunctionKey,
        kind: LoweredFunctionKind,
    ) -> Result<Option<FullFunctionView<'_>>, IrVerifyError> {
        let _symbols = self.symbol_owner().enter();
        for index in 0..self.store.functions.len() {
            if self.function_identity(index)? == (key, kind) {
                return Ok(Some(FullFunctionView {
                    program: self,
                    index,
                }));
            }
        }
        Ok(None)
    }

    pub(in crate::runtime::eval) fn function_view_at(
        &self,
        index: usize,
    ) -> Option<FullFunctionView<'_>> {
        (index < self.store.functions.len()).then_some(FullFunctionView {
            program: self,
            index,
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

    fn verify_driver(&self) -> Result<(), IrVerifyError> {
        if self.store.driver_root == IR_NONE {
            return Ok(());
        }
        let mut program_states = vec![0; self.store.driver_programs.len()];
        let mut step_states = vec![0; self.store.driver_steps.len()];
        self.verify_driver_program(
            self.store.driver_root,
            &mut program_states,
            &mut step_states,
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
            self.verify_driver_step(step_index, program_states, step_states)?;
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
            lowered_type_from_type(&self.store.semantic.to_type(slot.type_id)?)?;
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
                self.verify_driver_program(child, program_states, step_states)?;
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
                Vec::<Name>::verify(&decoder, &mut payload)?;
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
    pub(in crate::runtime::eval) fn index(&self) -> usize {
        self.index
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

    pub(in crate::runtime::eval) fn header(&self) -> Result<FunctionHeader, IrVerifyError> {
        let function = self.program.store.functions[self.index];
        let decoder = self.execution()?.decoder;
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
            param_kinds.push(lowered_type_from_type(
                &self.program.store.semantic.to_type(param.type_id)?,
            )?);
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
                    ty: self.program.store.semantic.to_type(validation.type_id)?,
                    name: Arc::from(self.program.store.string(validation.name)?),
                })
            });
            param_rest.push(param.flags & 1 != 0);
            param_defaults.push(if cold.is_none_or(|cold| cold.default == IR_NONE) {
                None
            } else {
                let raw = [cold.expect("checked above").default];
                let mut value = FullCursor::new(&raw);
                Some(LoweredValue::decode(&decoder, &mut value)?)
            });
        }
        let mut decoded_captures = SmallVec::new();
        for capture in &self.program.store.captures[captures] {
            decoded_captures.push(LoweredTopLevelSlot {
                name: Name::intern(self.program.store.string(capture.name)?),
                slot: (capture.slot_and_flags & !(1 << 31)) as usize,
                kind: lowered_type_from_type(
                    &self.program.store.semantic.to_type(capture.type_id)?,
                )?,
                mutable: capture.slot_and_flags & (1 << 31) != 0,
            });
        }
        let return_type = self.program.store.semantic.to_type(
            self.program
                .store
                .semantic
                .signature_return_type(function.signature)?,
        )?;
        let return_kind = match return_type {
            Type::Result(ok, _) => LoweredReturnKind::Result(lowered_type_from_type(&ok)?),
            ty => LoweredReturnKind::Plain(lowered_type_from_type(&ty)?),
        };
        Ok(FunctionHeader {
            params: param_names,
            param_kinds,
            param_checks,
            param_rest,
            param_defaults,
            captures: decoded_captures,
            return_kind,
            slot_count: function.slot_count as usize,
        })
    }

    pub(in crate::runtime::eval) fn execution(&self) -> Result<FullExecution<'a>, IrVerifyError> {
        let function = self.program.store.functions[self.index];
        Ok(FullExecution {
            decoder: FullDecoder {
                store: &self.program.store,
                owner: IrFunctionId::new(self.index)
                    .map_err(|_| IrVerifyError::new("function id is invalid"))?
                    .raw(),
                instruction_range: self.program.store.function_instruction_range(self.index)?,
                instruction_states: None,
                block_states: None,
                slot_count: function.slot_count,
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
        let step = self.program.store.driver_steps[self.index];
        Ok(FullExecution {
            decoder: FullDecoder {
                store: &self.program.store,
                owner: driver_owner(self.index)
                    .map_err(|_| IrVerifyError::new("driver owner is invalid"))?,
                instruction_range: self.program.store.driver_instruction_range(self.index)?,
                instruction_states: None,
                block_states: None,
                slot_count: step.slot_count,
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
                name: Name::intern(self.program.store.string(slot.name)?),
                slot: slot.slot as usize,
                kind: lowered_type_from_type(&self.program.store.semantic.to_type(slot.type_id)?)?,
                mutable: slot.flags & DRIVER_SLOT_MUTABLE != 0,
            });
        }
        Ok(slots)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FullCheckpoint {
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
    locations: usize,
    runtime_ops: usize,
    assign_ops: usize,
    binary_ops: usize,
    run_kinds: usize,
    redirection_kinds: usize,
    semantic: super::semantic::SemanticCheckpoint,
}

#[derive(Default)]
pub(in crate::runtime::eval) struct FullBuilder {
    store: FullStore,
    semantic: SemanticPoolBuilder,
    strings: BTreeMap<String, IrStringId>,
    bytes: BTreeMap<Vec<u8>, super::IrBytesId>,
    locations: BTreeMap<(SourceId, u32, u32), IrLocationId>,
    function_ids: BTreeMap<LoweredFunctionKey, IrFunctionId>,
    payload_pool: Vec<Vec<u32>>,
    current_owner: Option<u32>,
    current_slot_count: u32,
    active_scratch: Option<Rc<RefCell<BuildScratch>>>,
}

impl FullBuilder {
    fn new(source_id: SourceId) -> Self {
        Self {
            store: FullStore {
                source_id,
                ..FullStore::default()
            },
            ..Self::default()
        }
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
                    Some(unit.source_span()),
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
        mut self,
        sources: Arc<SourceMap>,
        symbols: crate::symbol::SymbolOwner,
    ) -> Result<FullProgram, IrBuildError> {
        self.store.shrink_to_fit();
        let program = FullProgram {
            store: self.store,
            sources,
            symbols,
        };
        FullVerifier::verify(&program)
            .map_err(|_| IrBuildError::format("full_ir_verification", None, 0, 0))?;
        Ok(program)
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
        symbols: crate::symbol::SymbolOwner,
    ) -> Result<FullProgram, IrBuildError> {
        let mut builder = Self::new(source_id);
        builder.reserve_function_keys(super::super::lower::compact_function_keys(program))?;
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
            |mut unit| {
                builder.predeclare(&[&unit])?;
                let body = unit.take_lowered_body().ok_or_else(|| {
                    IrBuildError::format(
                        "full_ir_function_blocker",
                        Some(unit.source_span()),
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
        )?;
        let functions = super::super::LowerableFunctions::all(
            &pures,
            &procs,
            &qualified_pures,
            &qualified_procs,
        );
        let (driver, _) = super::super::lower::lower_compact_top_level_program_with_probe(
            program,
            declarations,
            bodies,
            source,
            &sources,
            &functions,
            true,
        );
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
            return Err(error);
        }
        builder.finish(sources, symbols)
    }

    fn predeclare(&mut self, units: &[&LoweredFunctionUnit]) -> Result<(), IrBuildError> {
        for unit in units {
            let body = unit.lowered_body().ok_or_else(|| {
                IrBuildError::format(
                    "full_ir_function_blocker",
                    Some(unit.source_span()),
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
            let params_start = self.store.params.len();
            let captures_start = self.store.captures.len();
            let mut signature_params = Vec::with_capacity(body.params.len());
            for (index, name) in body.params.iter().copied().enumerate() {
                let type_id = self.intern_lowered_type(body.param_kinds[index])?;
                let default = body.param_defaults[index]
                    .as_ref()
                    .map(|value| self.encode_value_id(value))
                    .transpose()?
                    .unwrap_or(IR_NONE);
                let validation = body.param_checks[index]
                    .as_ref()
                    .map(|check| self.encode_validation(check))
                    .transpose()?
                    .unwrap_or(IR_NONE);
                let flags = u8::from(body.param_rest[index])
                    | u8::from(body.param_defaults[index].is_some()) << 1;
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
                signature_params.push((name, type_id, u32::from(flags)));
            }
            for capture in &body.captures {
                let slot = u32::try_from(capture.slot)
                    .map_err(|_| IrBuildError::format("slot_overflow", None, 0, 0))?;
                let name = self.intern_string(&capture.name.as_str())?.raw();
                let type_id = self.intern_lowered_type(capture.kind)?;
                self.store.captures.push(FullCapture {
                    name,
                    type_id,
                    slot_and_flags: slot | u32::from(capture.mutable) << 31,
                });
            }
            let return_type = self.intern_return_type(body.return_kind)?;
            let signature = self.semantic.intern_signature_parts(
                &mut self.store.semantic,
                &signature_params,
                return_type,
                None,
            )?;
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
            self.store.function_metadata.push(FullFunctionMetadata {
                owner,
                flags: match unit.kind() {
                    LoweredFunctionKind::Pure => 0,
                    LoweredFunctionKind::Proc => 1,
                } | u8::from(body.has_defers) << 1,
                reserved: [0; 3],
            });
        }
        Ok(())
    }

    fn encode_body(
        &mut self,
        function: IrFunctionId,
        body: &FunctionBuild,
    ) -> Result<(), IrBuildError> {
        let instruction_start = self.store.tags.len();
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
        self.store.blocks[block.index()].flags |= BLOCK_FUNCTION_BODY;
        self.store.functions[function.index()].body = body_id;
        self.store.function_instruction_starts[function.index()] = u32::try_from(instruction_start)
            .map_err(|_| IrBuildError::format("instruction_overflow", None, 0, 0))?;
        Ok(())
    }

    fn encode_driver_root(
        &mut self,
        program: &ProgramBuild,
        source_statements: &[StmtId],
        arena: &ArenaProgram,
        allow_checker_only: bool,
    ) -> Result<(), IrBuildError> {
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
                let type_id = self.intern_lowered_type(slot.kind)?;
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
                mutable,
                span,
            }) => {
                source.encode(self, &mut payload)?;
                fields.encode(self, &mut payload)?;
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
            locations: self.store.locations.len(),
            runtime_ops: self.store.runtime_ops.len(),
            assign_ops: self.store.assign_ops.len(),
            binary_ops: self.store.binary_ops.len(),
            run_kinds: self.store.run_kinds.len(),
            redirection_kinds: self.store.redirection_kinds.len(),
            semantic: self.semantic.checkpoint(&self.store.semantic),
        }
    }

    fn rewind(&mut self, checkpoint: FullCheckpoint) {
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
                FullTag::ExprAbort | FullTag::ExprFail => EFFECT_PROPAGATE | EFFECT_TRACE,
                FullTag::ExprDynamicCall => EFFECT_DYNAMIC_CALL | EFFECT_TRACE,
                FullTag::ExprCall | FullTag::ExprSelfCall | FullTag::ExprDirectPureCall => {
                    EFFECT_TRACE
                }
                FullTag::ExprModuleCall | FullTag::StmtProc => EFFECT_HOST | EFFECT_TRACE,
                FullTag::StmtPrint => EFFECT_HOST | EFFECT_TRACE,
                FullTag::ExprFsFiles
                | FullTag::ExprFsWalk
                | FullTag::ExprFsList
                | FullTag::ExprFsTempDir
                | FullTag::ExprFsWrite
                | FullTag::ExprFsMkdir
                | FullTag::ExprFsRemove
                | FullTag::ExprFsCloseRoot
                | FullTag::ExprFsRootPath
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
                | FullTag::ExprHashVerifyFile => EFFECT_HOST | EFFECT_TRACE,
                FullTag::ExprTry | FullTag::StmtGuard => EFFECT_PROPAGATE | EFFECT_TRACE,
                FullTag::StmtDefer => EFFECT_DEFER | EFFECT_PROPAGATE | EFFECT_TRACE,
                FullTag::ExprLoop
                | FullTag::StmtLoop
                | FullTag::StmtWhile
                | FullTag::StmtWhileBool
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
        LoweredType::Stream => Type::Stream(Box::new(Type::Any)),
        LoweredType::Pure => Type::Pure,
        LoweredType::Proc => Type::Proc,
        LoweredType::Error => Type::Error,
        LoweredType::Record => Type::Record(BTreeMap::new()),
        LoweredType::Module => Type::Module(BTreeMap::new()),
        LoweredType::List => Type::List(Box::new(Type::Any)),
        LoweredType::Map => Type::Map(Box::new(Type::Any)),
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
        Type::Map(inner) => Type::Map(Box::new(executable_type(inner))),
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
        Type::Int => LoweredType::Int,
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
        Type::Stream(_) => LoweredType::Stream,
        Type::Pure => LoweredType::Pure,
        Type::Proc => LoweredType::Proc,
        Type::Error | Type::ErrorFamily(_) | Type::ErrorVariant { .. } | Type::ErrorFacet(_) => {
            LoweredType::Error
        }
        Type::Record(_) => LoweredType::Record,
        Type::Module(_) => LoweredType::Module,
        Type::List(_) => LoweredType::List,
        Type::Map(_) => LoweredType::Map,
        Type::Tag(_) => LoweredType::Tag,
        Type::Result(_, _) => LoweredType::Result,
        Type::Null | Type::Optional(_) => LoweredType::Any,
        Type::Invalid | Type::EnvPathList | Type::ProcessError => {
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

pub(in crate::runtime::eval) struct FullCursor<'a> {
    words: &'a [u32],
    index: usize,
    verified: bool,
}

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
}

impl<'a> FullExecution<'a> {
    pub(in crate::runtime::eval) fn thread_local(&self) -> Self {
        Self {
            decoder: FullDecoder {
                store: self.decoder.store,
                owner: self.decoder.owner,
                instruction_range: self.decoder.instruction_range.clone(),
                instruction_states: None,
                block_states: None,
                slot_count: self.decoder.slot_count,
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
        Ok((index, tag, self.cursor(self.store.payload(data.range())?)))
    }

    #[inline(always)]
    fn finish_instruction(&self, index: usize) {
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

struct FullVerifier;

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
        FullTag::StmtIf | FullTag::StmtIfBool => {
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
                branch_words = rest;
            }
            if !branch_words.is_empty() {
                return Err(IrVerifyError::new(
                    "return-analysis if branches have trailing data",
                ));
            }
            let else_returns = match payload.get(1..).unwrap_or_default() {
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
    fn verify(program: &FullProgram) -> Result<(), IrVerifyError> {
        let _symbols = program.symbol_owner().enter();
        let store = &program.store;
        if store.tags.len() != store.data.len()
            || store.patterns.len() != store.pattern_data.len()
            || store.stages.len() != store.stage_data.len()
            || store.values.len() != store.value_data.len()
            || store.locations.len() != store.location_sources.len()
            || store.functions.len() != store.function_instruction_starts.len()
            || store.functions.len() != store.function_metadata.len()
        {
            return Err(IrVerifyError::new("full IR tag/data columns differ"));
        }
        store.semantic.verify()?;
        program
            .sources
            .get(store.source_id)
            .ok_or_else(|| IrVerifyError::new("full IR source is missing"))?;
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
            if metadata.flags & !0b11 != 0 {
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
            if store.semantic.signature_param_count(function.signature)? != params.len() {
                return Err(IrVerifyError::new(
                    "function parameters do not match its signature",
                ));
            }
            for param in &store.params[params.clone()] {
                store.string(param.name)?;
                store.semantic.type_tag(param.type_id)?;
                if param.flags & !0b11 != 0 {
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
            if body_payload.first().copied() == Some(0) {
                return Err(IrVerifyError::new(format!(
                    "function {index} has an empty body"
                )));
            }
            let return_type = store
                .semantic
                .to_type(store.semantic.signature_return_type(function.signature)?)?;
            if !matches!(return_type, Type::Stream(_)) && !indexed_block_can_return(store, body_id)?
            {
                return Err(IrVerifyError::new(format!(
                    "function {index} body does not terminate with a return"
                )));
            }
        }
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
            program.verify_driver()?;
        }
        if previous_end != store.tags.len() {
            return Err(IrVerifyError::new(
                "full IR contains instructions outside executable owner ranges",
            ));
        }
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
impl FullCodec for usize {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        if builder.current_owner.is_none() {
            return Err(IrBuildError::format("slot_without_owner", None, 0, 0));
        }
        if *self >= builder.current_slot_count as usize {
            return Err(IrBuildError::format("slot_out_of_bounds", None, 0, 0));
        }
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
        lowered_type_to_type(*self)?.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        lowered_type_from_type(&Type::decode(decoder, input)?)
    }
}

impl FullCodec for LoweredTypeCheck {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.ty.encode(builder, output)?;
        self.name.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Self {
            ty: Type::decode(decoder, input)?,
            name: Arc::<str>::decode(decoder, input)?,
        })
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
                value.name.encode(builder, &mut payload)?;
                value.fields.encode(builder, &mut payload)?;
                FullValueTag::Tag
            }
            Self::ResultOk(value) => {
                value.encode(builder, &mut payload)?;
                FullValueTag::ResultOk
            }
            Self::Digest(_)
            | Self::Regex(_)
            | Self::Status(_)
            | Self::FsEntry(_)
            | Self::Command(_)
            | Self::ProcessHandle(_)
            | Self::Stream(_)
            | Self::Pure(_)
            | Self::Proc(_)
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
            FullValueTag::Path => Self::Path(PathValue::decode(decoder, &mut payload)?),
            FullValueTag::Record => Self::Record(BTreeMap::<Arc<str>, LoweredValue>::decode(
                decoder,
                &mut payload,
            )?),
            FullValueTag::RecordVec => {
                Self::RecordVec(Vec::<(Name, LoweredValue)>::decode(decoder, &mut payload)?)
            }
            FullValueTag::Stats => Self::Stats {
                blanks: i64::decode(decoder, &mut payload)?,
                code: i64::decode(decoder, &mut payload)?,
                comments: i64::decode(decoder, &mut payload)?,
            },
            FullValueTag::StatsBlob => Self::StatsBlob(Box::new(LoweredStatsValue {
                blanks: i64::decode(decoder, &mut payload)?,
                blobs: BTreeMap::<String, LoweredValue>::decode(decoder, &mut payload)?,
                code: i64::decode(decoder, &mut payload)?,
                comments: i64::decode(decoder, &mut payload)?,
            })),
            FullValueTag::Module => Self::Module(BTreeMap::<Arc<str>, LoweredValue>::decode(
                decoder,
                &mut payload,
            )?),
            FullValueTag::List => Self::List(Vec::<LoweredValue>::decode(decoder, &mut payload)?),
            FullValueTag::Map => Self::Map(BTreeMap::<String, LoweredValue>::decode(
                decoder,
                &mut payload,
            )?),
            FullValueTag::Tag => Self::Tag(Box::new(LoweredTagValue {
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
                row.encode(builder, output)
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
impl_vec_codec!(BuildPatternId, BLOCK_LIST);
impl_vec_codec!(LoweredPipelineStage, BLOCK_LIST);
impl_vec_codec!(LoweredValue, BLOCK_LIST);
impl_vec_codec!(LoweredFmtPart, BLOCK_LIST);
impl_vec_codec!(LoweredRecordEntry, BLOCK_LIST);
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
impl_vec_codec!((Name, usize), BLOCK_LIST);
impl_vec_codec!((Name, LoweredValue), BLOCK_LIST);
impl_vec_codec!((Arc<str>, BuildExprId), BLOCK_LIST);
impl_vec_codec!((Arc<str>, Vec<BuildStmtId>), BLOCK_LIST);
impl_vec_codec!((BuildExprId, BuildExprId), BLOCK_LIST);
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
            0 => Ok(Self::Slot(usize::decode(decoder, input)?)),
            1 => Ok(Self::Record {
                fields: SmallVec::decode(decoder, input)?,
            }),
            _ => Err(IrVerifyError::new("comprehension target tag is invalid")),
        }
    }
}

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
        self.cpu_max.encode(builder, output)
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
                span,
            } => {
                output.push(1);
                target.encode(builder, output)?;
                args.encode(builder, output)?;
                env.encode(builder, output)?;
                timeout.encode(builder, output)?;
                cpu_max.encode(builder, output)?;
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
            Self::Tag { name, slots } => {
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
                name: Name::decode(decoder, &mut payload)?,
                slots: SmallVec::decode(decoder, &mut payload)?,
            },
        };
        payload.finish()?;
        Ok(pattern)
    }

    fn verify(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<(), IrVerifyError> {
        let index = input.raw()? as usize;
        let tag = decoder
            .store
            .patterns
            .get(index)
            .copied()
            .ok_or_else(|| IrVerifyError::new("pattern id is out of bounds"))?;
        let data = decoder.store.pattern_data[index];
        let mut payload = FullCursor::new(decoder.store.payload(data.range())?);
        match tag {
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
                BuildPatternIdSlots::verify(decoder, &mut payload)?;
            }
        }
        payload.finish()
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
    } => ReduceBy {
        item_slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
        op: ReduceByOp,
    } => LoweredPipelineStage::ReduceBy {
        item_slot,
        body,
        value,
        op,
    },
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
        parallel,
    } => Each {
        slot: usize,
        body: Vec<BuildStmtId>,
        parallel: bool,
    } => LoweredPipelineStage::Each {
        slot,
        body,
        parallel,
    },
    LoweredPipelineStage::TablePrint { columns } => TablePrint {
        columns: Option<Vec<String>>,
    } => LoweredPipelineStage::TablePrint { columns },
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
        BuildExprRow::PathFrom { value, span } => ExprPathFrom {
            value: BuildExprId,
            span: Span,
        } => BuildExprRow::PathFrom { value, span },
        BuildExprRow::Param(slot) => ExprParam {
            slot: usize,
        } => BuildExprRow::Param(slot),
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
        BuildExprRow::Record(entries) => ExprRecord {
            entries: Vec<LoweredRecordEntry>,
        } => BuildExprRow::Record(entries),
        BuildExprRow::List(values) => ExprList {
            values: Vec<BuildExprId>,
        } => BuildExprRow::List(values),
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
        BuildExprRow::Tag { name, fields } => ExprTag {
            name: Arc<str>,
            fields: Vec<BuildExprId>,
        } => BuildExprRow::Tag { name, fields },
        BuildExprRow::ListComp {
            value,
            target,
            iter,
            condition,
            span,
        } => ExprListComp {
            value: BuildExprId,
            target: Box<LoweredCompTarget>,
            iter: BuildExprId,
            condition: Option<BuildExprId>,
            span: Span,
        } => BuildExprRow::ListComp {
            value,
            target,
            iter,
            condition,
            span,
        },
        BuildExprRow::MapComp {
            key,
            value,
            target,
            iter,
            condition,
            span,
        } => ExprMapComp {
            key: BuildExprId,
            value: BuildExprId,
            target: Box<LoweredCompTarget>,
            iter: BuildExprId,
            condition: Option<BuildExprId>,
            span: Span,
        } => BuildExprRow::MapComp {
            key,
            value,
            target,
            iter,
            condition,
            span,
        },
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
            default,
            span,
        } => ExprStrByteAt {
            receiver: BuildExprId,
            index: BuildExprId,
            default: Option<BuildExprId>,
            span: Span,
        } => BuildExprRow::StrByteAt {
            receiver,
            index,
            default,
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
        BuildExprRow::Contains {
            receiver,
            needle,
            span,
        } => ExprContains {
            receiver: BuildExprId,
            needle: BuildExprId,
            span: Span,
        } => BuildExprRow::Contains {
            receiver,
            needle,
            span,
        },
        BuildExprRow::RegexCompile { pattern, span } => ExprRegexCompile {
            pattern: BuildExprId,
            span: Span,
        } => BuildExprRow::RegexCompile { pattern, span },
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
        BuildExprRow::Loop { body, span } => ExprLoop {
            body: Vec<BuildStmtId>,
            span: Span,
        } => BuildExprRow::Loop { body, span },
        BuildExprRow::Retry { delays, body, span } => ExprRetry {
            delays: Vec<BuildExprId>,
            body: Vec<BuildStmtId>,
            span: Span,
        } => BuildExprRow::Retry { delays, body, span },
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
            gitignore: bool,
            stat: bool,
            hidden: bool,
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
            gitignore: bool,
            stat: bool,
            hidden: bool,
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
        BuildExprRow::FsCloseRoot { root, span } => ExprFsCloseRoot {
            root: BuildExprId,
            span: Span,
        } => BuildExprRow::FsCloseRoot { root, span },
        BuildExprRow::FsRootPath { root, span } => ExprFsRootPath {
            root: BuildExprId,
            span: Span,
        } => BuildExprRow::FsRootPath { root, span },
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
        BuildExprRow::HashVerifyFile {
            path,
            algorithm,
            expected,
            span,
        } => ExprHashVerifyFile {
            path: BuildExprId,
            algorithm: HashAlgorithm,
            expected: BuildExprId,
            span: Span,
        } => BuildExprRow::HashVerifyFile {
            path,
            algorithm,
            expected,
            span,
        },
        BuildExprRow::ModuleCall { op, args, span } => ExprModuleCall {
            op: RuntimeOp,
            args: Vec<BuildExprId>,
            span: Span,
        } => BuildExprRow::ModuleCall { op, args, span },
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
        BuildExprRow::Err(value) => ExprErr {
            value: BuildExprId,
        } => BuildExprRow::Err(value),
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
        BuildStmtRow::Let { slot, value } => StmtLet {
            slot: usize,
            value: BuildExprId,
        } => BuildStmtRow::Let { slot, value },
        BuildStmtRow::Guard {
            slot,
            value,
            else_param_slot,
            else_body,
            span,
        } => StmtGuard {
            slot: usize,
            value: BuildExprId,
            else_param_slot: Option<usize>,
            else_body: Vec<BuildStmtId>,
            span: Span,
        } => BuildStmtRow::Guard {
            slot,
            value,
            else_param_slot,
            else_body,
            span,
        },
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
            span,
        } => StmtAssign {
            slot: usize,
            op: AssignOp,
            value: BuildExprId,
            span: Span,
        } => BuildStmtRow::Assign {
            slot,
            op,
            value,
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
        BuildStmtRow::AssignIndex {
            slot,
            index,
            op,
            value,
            span,
        } => StmtAssignIndex {
            slot: usize,
            index: BuildExprId,
            op: AssignOp,
            value: BuildExprId,
            span: Span,
        } => BuildStmtRow::AssignIndex {
            slot,
            index,
            op,
            value,
            span,
        },
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
            fields,
            span,
        } => StmtLetRecord {
            source: BuildExprId,
            fields: Vec<(Name, usize)>,
            span: Span,
        } => BuildStmtRow::LetRecord {
            source,
            fields,
            span,
        },
        BuildStmtRow::ForRecord {
            fields,
            iter,
            body,
            span,
        } => StmtForRecord {
            fields: Vec<(Name, usize)>,
            iter: BuildExprId,
            body: Vec<BuildStmtId>,
            span: Span,
        } => BuildStmtRow::ForRecord {
            fields,
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

env {
  TOP_LEVEL_DRIVER_BOUNDARY = "indexed"
} {
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

    fn run_with_large_stack(f: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(f)
            .expect("spawn full indexed IR test")
            .join()
            .unwrap_or_else(|payload| std::panic::resume_unwind(payload));
    }

    fn fixture(name: &str, source: &str) -> FullProgram {
        let mut sources = SourceMap::new();
        let source_id = sources.add_file(name, source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
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

    fn program_name(program: &FullProgram, text: &str) -> Name {
        program.symbol_owner().with_current(|| Name::intern(text))
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
            evaluator.stdout,
            normalize_traces(&evaluator.trace_events),
        )
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
            sources: Arc::new(sources),
            symbols: crate::symbol::SymbolOwner::new(),
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
            "let suffix = \"!\"\n\
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
}
