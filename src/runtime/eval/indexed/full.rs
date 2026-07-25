use super::semantic::{SemanticPoolBuilder, SemanticPools};
use super::{
    IR_NONE, IrBlockId, IrBuildError, IrData, IrFunctionId, IrLocation, IrLocationId, IrRange,
    IrStringId, IrVerifyError, SignatureId, TypeId,
};
use crate::modules::RuntimeOp;
use crate::modules::hash::HashAlgorithm;
use crate::runtime::eval::{
    LoweredBoolExpr, LoweredCallArg, LoweredCompTarget, LoweredErrorExpr, LoweredExpr,
    LoweredFmtPart, LoweredFunctionKey, LoweredFunctionKind, LoweredFunctionUnit, LoweredIntExpr,
    LoweredPattern, LoweredPipelineStage, LoweredProcessCommandArgv,
    LoweredProcessCommandBuilderEntry, LoweredPureFunction, LoweredRecordEntry, LoweredReturnKind,
    LoweredRunArg, LoweredRunArgKind, LoweredRunCapture, LoweredRunEnv,
    LoweredRunPipelineSegment, LoweredRunRedirection, LoweredSpawnRun, LoweredStmt,
    LoweredStatsValue, LoweredStrPredicate, LoweredTagValue, LoweredType, LoweredTypeCheck,
    LoweredModuleExport, LoweredModuleExportKind, LoweredProgram, LoweredTopLevelKind,
    LoweredTopLevelSlot, LoweredTopLevelStmt, LoweredValue, ReduceByOp, ScanCheck, ScanCondition,
};
use crate::runtime::value::{DurationValue, FloatValue, FunctionName, PathValue};
use crate::sema::check::{CompactBodyProbeOutput, CompactDeclOutput};
use crate::sema::types::{CallableParamType, CallableType, ModuleExportType, Type};
use crate::source::{SourceId, SourceMap, Span};
use crate::symbol::{Name, QualifiedName, Symbol};
use crate::syntax::arena::{ArenaProgram, StmtId};
use crate::syntax::node::{
    AssignOp, BinaryOp, FormatSpec, FormatSpecKind, RedirectionKind, RunKind,
};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::mem::size_of;
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
enum FullTag {
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
    ExprOk,
    ExprErr,
    ExprError,
    ExprTry,
    ExprCall,
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
enum FullPatternTag {
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
enum FullStageTag {
    TextLines,
    JsonLines,
    Where,
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
    All,
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
enum FullValueTag {
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
enum FullDriverTag {
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
    fn payload(&self, range: IrRange) -> Result<&[u32], IrVerifyError> {
        let bounds = range
            .bounds(self.extra.len())
            .ok_or_else(|| IrVerifyError::new("full IR extra range is out of bounds"))?;
        Ok(&self.extra[bounds])
    }

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
            .unwrap_or_else(|| self.tags.len() as u32);
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
}

impl FullProgram {
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

    fn decode_functions(
        &self,
    ) -> Result<Vec<(LoweredFunctionKey, LoweredFunctionKind, Arc<LoweredPureFunction>)>, IrVerifyError>
    {
        let mut decoded = Vec::with_capacity(self.store.functions.len());
        for (function_index, function) in self.store.functions.iter().enumerate() {
            let instructions = self.store.function_instruction_range(function_index)?;
            let instruction_len = instructions.len();
            let decoder = FullDecoder {
                store: &self.store,
                owner: IrFunctionId::new(function_index)
                    .map_err(|_| IrVerifyError::new("function id is invalid"))?
                    .raw(),
                instruction_range: instructions,
                instruction_states: RefCell::new(vec![0; instruction_len]),
                block_states: RefCell::new(vec![0; self.store.blocks.len()]),
                slot_count: function.slot_count,
            };
            let body_words = [function.body];
            let mut body_words = FullCursor::new(&body_words);
            let body = Vec::<LoweredStmt>::decode(&decoder, &mut body_words)?;
            body_words.finish()?;
            decoder.finish_function()?;
            let params = function
                .params
                .bounds(self.store.params.len())
                .ok_or_else(|| IrVerifyError::new("function parameter range is invalid"))?;
            let captures = function
                .captures
                .bounds(self.store.captures.len())
                .ok_or_else(|| IrVerifyError::new("function capture range is invalid"))?;
            let mut param_names = SmallVec::new();
            let mut param_kinds = SmallVec::new();
            let mut param_checks = SmallVec::new();
            let mut param_rest = SmallVec::new();
            let mut param_defaults = SmallVec::new();
            for (offset, param) in self.store.params[params.clone()].iter().enumerate() {
                let param_index = params.start + offset;
                let cold = self
                    .store
                    .param_cold
                    .binary_search_by_key(&(param_index as u32), |cold| cold.param)
                    .ok()
                    .map(|index| self.store.param_cold[index]);
                param_names.push(Name::intern(self.store.string(param.name)?));
                param_kinds.push(lowered_type_from_type(
                    &self.store.semantic.to_type(param.type_id)?,
                )?);
                param_checks.push(if cold.is_none_or(|cold| cold.validation == IR_NONE) {
                    None
                } else {
                    let validation_id = cold.expect("checked above").validation;
                    let validation = self
                        .store
                        .validations
                        .get(validation_id as usize)
                        .ok_or_else(|| IrVerifyError::new("validation id is out of bounds"))?;
                    Some(LoweredTypeCheck {
                        ty: self.store.semantic.to_type(validation.type_id)?,
                        name: Arc::from(self.store.string(validation.name)?),
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
            for capture in &self.store.captures[captures] {
                decoded_captures.push(LoweredTopLevelSlot {
                    name: Name::intern(self.store.string(capture.name)?),
                    slot: (capture.slot_and_flags & !(1 << 31)) as usize,
                    kind: lowered_type_from_type(
                        &self.store.semantic.to_type(capture.type_id)?,
                    )?,
                    mutable: capture.slot_and_flags & (1 << 31) != 0,
                });
            }
            let return_type = self
                .store
                .semantic
                .to_type(self.store.semantic.signature_return_type(function.signature)?)?;
            let return_kind = match return_type {
                Type::Result(ok, _) => {
                    LoweredReturnKind::Result(lowered_type_from_type(&ok)?)
                }
                ty => LoweredReturnKind::Plain(lowered_type_from_type(&ty)?),
            };
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
            decoded.push((
                key,
                if metadata.flags & 1 == 0 {
                    LoweredFunctionKind::Pure
                } else {
                    LoweredFunctionKind::Proc
                },
                Arc::new(LoweredPureFunction {
                    params: param_names,
                    param_kinds,
                    param_checks,
                    param_rest,
                    param_defaults,
                    captures: decoded_captures,
                    return_kind,
                    slot_count: function.slot_count as usize,
                    body,
                    has_defers: metadata.flags & 2 != 0,
                }),
            ));
        }
        Ok(decoded)
    }

    fn decode_driver(&self) -> Result<Option<LoweredProgram>, IrVerifyError> {
        if self.store.driver_root == IR_NONE {
            return Ok(None);
        }
        let mut program_states = vec![0; self.store.driver_programs.len()];
        let mut step_states = vec![0; self.store.driver_steps.len()];
        let rows = self.decode_driver_program_rows(
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
        Ok(Some(LoweredProgram {
            statements: rows
                .into_iter()
                .map(|(_, statement)| statement)
                .collect(),
        }))
    }

    fn decode_driver_program_rows(
        &self,
        raw: u32,
        program_states: &mut [u8],
        step_states: &mut [u8],
    ) -> Result<Vec<(Span, Option<LoweredTopLevelStmt>)>, IrVerifyError> {
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
        let program = self.store.driver_programs[index];
        let steps = program
            .steps
            .bounds(self.store.driver_steps.len())
            .ok_or_else(|| IrVerifyError::new("driver program step range is invalid"))?;
        let mut rows = Vec::with_capacity(steps.len());
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
            rows.push(self.decode_driver_step(
                step_index,
                program_states,
                step_states,
            )?);
            step_states[step_index] = 2;
        }
        program_states[index] = 2;
        Ok(rows)
    }

    fn decode_driver_step(
        &self,
        step_index: usize,
        program_states: &mut [u8],
        step_states: &mut [u8],
    ) -> Result<(Span, Option<LoweredTopLevelStmt>), IrVerifyError> {
        let step = self.store.driver_steps[step_index];
        let instruction_range = self.store.driver_instruction_range(step_index)?;
        let decoder = FullDecoder {
            store: &self.store,
            owner: driver_owner(step_index)
                .map_err(|_| IrVerifyError::new("driver owner is invalid"))?,
            instruction_states: RefCell::new(vec![0; instruction_range.len()]),
            instruction_range,
            block_states: RefCell::new(vec![0; self.store.blocks.len()]),
            slot_count: step.slot_count,
        };
        let location_words = [step.location];
        let mut location = FullCursor::new(&location_words);
        let source_span = Span::decode(&decoder, &mut location)?;
        location.finish()?;
        let slots_range = step
            .slots
            .bounds(self.store.driver_slots.len())
            .ok_or_else(|| IrVerifyError::new("driver slot range is invalid"))?;
        let mut slots = SmallVec::new();
        for slot in &self.store.driver_slots[slots_range] {
            if slot.flags & !(DRIVER_SLOT_READ | DRIVER_SLOT_WRITE | DRIVER_SLOT_MUTABLE) != 0
                || slot.flags & DRIVER_SLOT_READ == 0
                || slot.slot >= step.slot_count
            {
                return Err(IrVerifyError::new("driver slot metadata is invalid"));
            }
            self.store.string(slot.name)?;
            let kind = lowered_type_from_type(&self.store.semantic.to_type(slot.type_id)?)?;
            slots.push(LoweredTopLevelSlot {
                name: Name::intern(self.store.string(slot.name)?),
                slot: slot.slot as usize,
                kind,
                mutable: slot.flags & DRIVER_SLOT_MUTABLE != 0,
            });
        }
        let mut payload = FullCursor::new(self.store.payload(step.data.range())?);
        let kind = match step.tag {
            FullDriverTag::Skip => {
                payload.finish()?;
                decoder.finish_function()?;
                return Ok((source_span, None));
            }
            FullDriverTag::Use => {
                let key = Arc::<str>::decode(&decoder, &mut payload)?;
                let alias = Option::<Name>::decode(&decoder, &mut payload)?;
                let path = Vec::<Name>::decode(&decoder, &mut payload)?;
                let namespace = Name::decode(&decoder, &mut payload)?;
                let exports = Vec::<LoweredModuleExport>::decode(&decoder, &mut payload)?;
                let child = payload.raw()?;
                let span = Span::decode(&decoder, &mut payload)?;
                let module_statements = self
                    .decode_driver_program_rows(child, program_states, step_states)?
                    .into_iter()
                    .filter_map(|(span, statement)| Some((span, statement?)))
                    .collect();
                LoweredTopLevelKind::Use {
                    key,
                    alias,
                    path,
                    namespace,
                    exports,
                    module_statements,
                    span,
                }
            }
            FullDriverTag::Let => LoweredTopLevelKind::Let {
                target: Name::decode(&decoder, &mut payload)?,
                ty: Option::<LoweredType>::decode(&decoder, &mut payload)?,
                validation: Option::<LoweredTypeCheck>::decode(&decoder, &mut payload)?,
                mutable: bool::decode(&decoder, &mut payload)?,
                value: LoweredExpr::decode(&decoder, &mut payload)?,
                value_span: Span::decode(&decoder, &mut payload)?,
            },
            FullDriverTag::LetRecord => LoweredTopLevelKind::LetRecord {
                source: LoweredExpr::decode(&decoder, &mut payload)?,
                fields: Vec::<Name>::decode(&decoder, &mut payload)?,
                mutable: bool::decode(&decoder, &mut payload)?,
                span: Span::decode(&decoder, &mut payload)?,
            },
            FullDriverTag::Assign => LoweredTopLevelKind::Assign {
                target: Name::decode(&decoder, &mut payload)?,
                op: AssignOp::decode(&decoder, &mut payload)?,
                value: LoweredExpr::decode(&decoder, &mut payload)?,
                span: Span::decode(&decoder, &mut payload)?,
            },
            FullDriverTag::Discard => LoweredTopLevelKind::Discard {
                value: LoweredExpr::decode(&decoder, &mut payload)?,
                span: Span::decode(&decoder, &mut payload)?,
            },
            FullDriverTag::Stmt => {
                LoweredTopLevelKind::Stmt(LoweredStmt::decode(&decoder, &mut payload)?)
            }
            FullDriverTag::Expr => {
                LoweredTopLevelKind::Expr(LoweredExpr::decode(&decoder, &mut payload)?)
            }
            FullDriverTag::Defer => LoweredTopLevelKind::Defer {
                value: LoweredExpr::decode(&decoder, &mut payload)?,
                span: Span::decode(&decoder, &mut payload)?,
            },
            FullDriverTag::SignalHook => LoweredTopLevelKind::SignalHook {
                signal: Name::decode(&decoder, &mut payload)?,
                pre_cancel: Option::<String>::decode(&decoder, &mut payload)?,
                body: Vec::<LoweredStmt>::decode(&decoder, &mut payload)?,
                slots: Vec::<LoweredTopLevelSlot>::decode(&decoder, &mut payload)?,
                slot_count: payload.raw()? as usize,
                span: Span::decode(&decoder, &mut payload)?,
            },
        };
        payload.finish()?;
        decoder.finish_function()?;
        Ok((
            source_span,
            Some(LoweredTopLevelStmt {
                kind,
                slots,
                slot_count: step.slot_count as usize,
            }),
        ))
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
struct FullBuilder {
    store: FullStore,
    semantic: SemanticPoolBuilder,
    strings: BTreeMap<String, IrStringId>,
    bytes: BTreeMap<Vec<u8>, super::IrBytesId>,
    locations: BTreeMap<(SourceId, u32, u32), IrLocationId>,
    function_ids: BTreeMap<LoweredFunctionKey, IrFunctionId>,
    current_owner: Option<u32>,
    current_slot_count: u32,
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

    pub(in crate::runtime::eval) fn build(
        units: &[LoweredFunctionUnit],
        sources: Arc<SourceMap>,
        source_id: SourceId,
    ) -> Result<FullProgram, IrBuildError> {
        Self::build_with_driver(units, None, sources, source_id)
    }

    fn build_with_driver(
        units: &[LoweredFunctionUnit],
        driver: Option<(&LoweredProgram, &[StmtId], &ArenaProgram)>,
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
            if let Err(mut error) = builder.encode_driver_root(driver, source_statements, arena) {
                error.attempted_instructions =
                    builder.store.tags.len().saturating_sub(checkpoint.tags);
                builder.rewind(checkpoint);
                error.committed_instructions = builder.store.tags.len();
                return Err(error);
            }
        }
        builder.store.shrink_to_fit();
        let program = FullProgram {
            store: builder.store,
            sources,
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
        let units = super::super::lower::probe_compact_lower_function_units_with_sources(
            program,
            declarations,
            bodies,
            source,
            &sources,
        );
        let mut pures = FxHashMap::default();
        let mut procs = FxHashMap::default();
        let mut qualified_pures = FxHashMap::default();
        let mut qualified_procs = FxHashMap::default();
        for unit in &units {
            let Some(body) = unit.lowered_body() else {
                continue;
            };
            match (unit.key(), unit.kind()) {
                (LoweredFunctionKey::Name(name), LoweredFunctionKind::Pure) => {
                    pures.insert(name, body);
                }
                (LoweredFunctionKey::Name(name), LoweredFunctionKind::Proc) => {
                    procs.insert(name, body);
                }
                (LoweredFunctionKey::Qualified(name), LoweredFunctionKind::Pure) => {
                    qualified_pures.insert(name, body);
                }
                (LoweredFunctionKey::Qualified(name), LoweredFunctionKind::Proc) => {
                    qualified_procs.insert(name, body);
                }
            }
        }
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
        Self::build_with_driver(
            &units,
            Some((&driver, &source_statements, program)),
            sources,
            source_id,
        )
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
            let function_id = IrFunctionId::new(self.store.functions.len())?;
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
                .map(|name| self.intern_string(name.as_str()))
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
                let name_id = self.intern_string(name.as_str())?.raw();
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
                let name = self.intern_string(capture.name.as_str())?.raw();
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
        body: &LoweredPureFunction,
    ) -> Result<(), IrBuildError> {
        let instruction_start = self.store.tags.len();
        let mut words = Vec::new();
        body.body.encode(self, &mut words)?;
        let [body_id] = words.as_slice() else {
            return Err(IrBuildError::format(
                "function_body_block",
                None,
                0,
                self.store.tags.len(),
            ));
        };
        let block = IrBlockId::from_raw(*body_id).ok_or_else(|| {
            IrBuildError::format("function_body_block", None, 0, self.store.tags.len())
        })?;
        self.store.blocks[block.index()].flags |= BLOCK_FUNCTION_BODY;
        self.store.functions[function.index()].body = *body_id;
        self.store.function_instruction_starts[function.index()] =
            u32::try_from(instruction_start)
                .map_err(|_| IrBuildError::format("instruction_overflow", None, 0, 0))?;
        Ok(())
    }

    fn encode_driver_root(
        &mut self,
        program: &LoweredProgram,
        source_statements: &[StmtId],
        arena: &ArenaProgram,
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
                    false,
                )
            {
                return Err(IrBuildError::format(
                    "top_level_boundary_blocker",
                    Some(span),
                    0,
                    self.store.tags.len(),
                ));
            }
            statements.push((span, lowered.as_ref()));
        }
        for (_, statement) in &statements {
            if let Some(statement) = statement {
                Self::validate_driver_imports(arena, statement)?;
            }
        }
        self.store.driver_root = self.encode_driver_program(&statements, arena)?;
        Ok(())
    }

    fn validate_driver_imports(
        arena: &ArenaProgram,
        statement: &LoweredTopLevelStmt,
    ) -> Result<(), IrBuildError> {
        let LoweredTopLevelKind::Use {
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
            .ok_or_else(|| {
                IrBuildError::format("driver_import_module", Some(*span), 0, 0)
            })?;
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
        if lowered_spans.len() != module_statements.len()
            || !lowered_spans.is_subset(&source_spans)
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
                false,
            ) {
                continue;
            }
            let source_span = arena.arena.stmt(source_statement).span;
            let key = (source_span.source_id, source_span.start(), source_span.end());
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
            Self::validate_driver_imports(arena, module_statement)?;
        }
        Ok(())
    }

    fn encode_driver_program(
        &mut self,
        statements: &[(Span, Option<&LoweredTopLevelStmt>)],
        arena: &ArenaProgram,
    ) -> Result<u32, IrBuildError> {
        let mut child_programs = Vec::with_capacity(statements.len());
        for (_, statement) in statements {
            let child = match statement.map(|statement| &statement.kind) {
                Some(LoweredTopLevelKind::Use {
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
                            (
                                (span.source_id, span.start(), span.end()),
                                statement,
                            )
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
        statement: Option<&LoweredTopLevelStmt>,
        child_program: Option<u32>,
    ) -> Result<(), IrBuildError> {
        let step_index = self.store.driver_steps.len();
        let owner = driver_owner(step_index)?;
        let instruction_start = u32::try_from(self.store.tags.len())
            .map_err(|_| IrBuildError::format("instruction_overflow", None, 0, 0))?;
        let slots_start = self.store.driver_slots.len();
        let slot_count = statement.map_or(0, |statement| statement.slot_count);
        let write_slots = statement.is_some_and(|statement| {
            matches!(statement.kind, LoweredTopLevelKind::Stmt(_))
        });
        if let Some(statement) = statement {
            for slot in &statement.slots {
                let slot_index = u32::try_from(slot.slot).map_err(|_| {
                    IrBuildError::format("driver_slot_overflow", None, 0, 0)
                })?;
                let type_id = self.intern_lowered_type(slot.kind)?;
                let name = self.intern_string(slot.name.as_str())?.raw();
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
                        | if slot.mutable {
                            DRIVER_SLOT_MUTABLE
                        } else {
                            0
                        },
                    reserved: [0; 3],
                });
            }
        }
        let slots = table_range(slots_start, self.store.driver_slots.len())?;
        self.current_owner = Some(owner);
        self.current_slot_count = u32::try_from(slot_count)
            .map_err(|_| IrBuildError::format("slot_overflow", None, 0, 0))?;
        let mut payload = Vec::new();
        let tag = match statement.map(|statement| &statement.kind) {
            None => FullDriverTag::Skip,
            Some(LoweredTopLevelKind::Use {
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
            Some(LoweredTopLevelKind::Let {
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
            Some(LoweredTopLevelKind::LetRecord {
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
            Some(LoweredTopLevelKind::Assign {
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
            Some(LoweredTopLevelKind::Discard { value, span }) => {
                value.encode(self, &mut payload)?;
                span.encode(self, &mut payload)?;
                FullDriverTag::Discard
            }
            Some(LoweredTopLevelKind::Stmt(statement)) => {
                statement.encode(self, &mut payload)?;
                FullDriverTag::Stmt
            }
            Some(LoweredTopLevelKind::Expr(value)) => {
                value.encode(self, &mut payload)?;
                FullDriverTag::Expr
            }
            Some(LoweredTopLevelKind::Defer { value, span }) => {
                value.encode(self, &mut payload)?;
                span.encode(self, &mut payload)?;
                FullDriverTag::Defer
            }
            Some(LoweredTopLevelKind::SignalHook {
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
        let data = self.push_extra(&payload)?;
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
        effects |= instruction_effects(
            &self.store.tags[instruction_start as usize..],
        );
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

    fn push_instruction(
        &mut self,
        tag: FullTag,
        payload: &[u32],
    ) -> Result<u32, IrBuildError> {
        let function = self
            .current_owner
            .ok_or_else(|| IrBuildError::format("missing_instruction_owner", None, 0, 0))?;
        let id = u32::try_from(self.store.tags.len())
            .map_err(|_| IrBuildError::format("instruction_overflow", None, 0, 0))?;
        let range = self.push_extra(payload)?;
        self.store.tags.push(tag);
        self.store
            .data
            .push(IrData::new(range.start, range.len));
        debug_assert_eq!(
            function,
            self.current_owner.expect("instruction owner remains set")
        );
        Ok(id)
    }

    fn push_pattern(
        &mut self,
        tag: FullPatternTag,
        payload: &[u32],
    ) -> Result<u32, IrBuildError> {
        let id = u32::try_from(self.store.patterns.len())
            .map_err(|_| IrBuildError::format("pattern_overflow", None, 0, 0))?;
        let range = self.push_extra(payload)?;
        self.store.patterns.push(tag);
        self.store
            .pattern_data
            .push(IrData::new(range.start, range.len));
        Ok(id)
    }

    fn push_stage(
        &mut self,
        tag: FullStageTag,
        payload: &[u32],
    ) -> Result<u32, IrBuildError> {
        let id = u32::try_from(self.store.stages.len())
            .map_err(|_| IrBuildError::format("stage_overflow", None, 0, 0))?;
        let range = self.push_extra(payload)?;
        self.store.stages.push(tag);
        self.store
            .stage_data
            .push(IrData::new(range.start, range.len));
        Ok(id)
    }

    fn push_value(
        &mut self,
        tag: FullValueTag,
        payload: &[u32],
    ) -> Result<u32, IrBuildError> {
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
            LoweredFunctionKey::Name(name) => Ok(self.intern_string(name.as_str())?.raw()),
            LoweredFunctionKey::Qualified(name) => Ok(self.intern_string(name.member.as_str())?.raw()),
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
            return u32::try_from(index)
                .map_err(|_| IrBuildError::format(construct, None, 0, 0));
        }
        let id = u32::try_from(values.len())
            .map_err(|_| IrBuildError::format(construct, None, 0, 0))?;
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

    fn encode_value_id(&mut self, value: &LoweredValue) -> Result<u32, IrBuildError> {
        let mut words = Vec::new();
        value.encode(self, &mut words)?;
        debug_assert_eq!(words.len(), 1);
        Ok(words[0])
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
        self.store.driver_regions.truncate(checkpoint.driver_regions);
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
        FullDriverTag::SignalHook => {
            EFFECT_SIGNAL | EFFECT_CANCELLATION | EFFECT_TRACE
        }
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
        FullDriverTag::Use
            | FullDriverTag::Let
            | FullDriverTag::LetRecord
            | FullDriverTag::Assign
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
                FullTag::ExprAbort => {
                    EFFECT_SIGNAL | EFFECT_CANCELLATION | EFFECT_PROPAGATE | EFFECT_TRACE
                }
                FullTag::ExprDynamicCall => EFFECT_DYNAMIC_CALL | EFFECT_TRACE,
                FullTag::ExprCall | FullTag::ExprSelfCall => EFFECT_TRACE,
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
                | FullTag::StmtScanLines => EFFECT_CANCELLATION,
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
        Type::Error
        | Type::ErrorFamily(_)
        | Type::ErrorVariant { .. }
        | Type::ErrorFacet(_) => LoweredType::Error,
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

trait FullCodec: Sized {
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError>;

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError>;
}

struct FullCursor<'a> {
    words: &'a [u32],
    index: usize,
}

impl<'a> FullCursor<'a> {
    fn new(words: &'a [u32]) -> Self {
        Self { words, index: 0 }
    }

    fn raw(&mut self) -> Result<u32, IrVerifyError> {
        let value = self
            .words
            .get(self.index)
            .copied()
            .ok_or_else(|| IrVerifyError::new("full IR payload ended early"))?;
        self.index += 1;
        Ok(value)
    }

    fn finish(self) -> Result<(), IrVerifyError> {
        if self.index == self.words.len() {
            Ok(())
        } else {
            Err(IrVerifyError::new("full IR payload has trailing words"))
        }
    }
}

struct FullDecoder<'a> {
    store: &'a FullStore,
    owner: u32,
    instruction_range: std::ops::Range<usize>,
    instruction_states: RefCell<Vec<u8>>,
    block_states: RefCell<Vec<u8>>,
    slot_count: u32,
}

impl<'a> FullDecoder<'a> {
    fn block(
        &self,
        input: &mut FullCursor<'_>,
        expected_flags: u8,
    ) -> Result<(IrBlockId, FullCursor<'a>), IrVerifyError> {
        let raw = input.raw()?;
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
        if block.owner != IR_NONE {
            let state = self.block_states.borrow()[id.index()];
            match state {
                0 => self.block_states.borrow_mut()[id.index()] = 1,
                1 => {
                    return Err(IrVerifyError::new(
                        "full IR block graph contains a cycle",
                    ));
                }
                2 => {
                    return Err(IrVerifyError::new(
                        "full IR block is owned by multiple parents",
                    ));
                }
                _ => unreachable!("block verifier state is bounded"),
            }
        }
        Ok((
            id,
            FullCursor::new(self.store.payload(block.instructions)?),
        ))
    }

    fn finish_block(&self, id: IrBlockId) {
        if self.store.blocks[id.index()].owner != IR_NONE {
            self.block_states.borrow_mut()[id.index()] = 2;
        }
    }

    fn instruction(
        &self,
        input: &mut FullCursor<'_>,
    ) -> Result<(usize, FullTag, FullCursor<'a>), IrVerifyError> {
        let index = input.raw()? as usize;
        if !self.instruction_range.contains(&index) {
            return Err(IrVerifyError::new(
                "full IR instruction belongs to another function",
            ));
        }
        let local = index - self.instruction_range.start;
        let state = self.instruction_states.borrow()[local];
        match state {
            0 => self.instruction_states.borrow_mut()[local] = 1,
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
        let tag = self
            .store
            .tags
            .get(index)
            .copied()
            .ok_or_else(|| IrVerifyError::new("full IR instruction is out of bounds"))?;
        let data = self.store.data[index];
        Ok((
            index,
            tag,
            FullCursor::new(self.store.payload(data.range())?),
        ))
    }

    fn finish_instruction(&self, index: usize) {
        self.instruction_states.borrow_mut()[index - self.instruction_range.start] = 2;
    }

    fn finish_function(&self) -> Result<(), IrVerifyError> {
        let instructions_complete = self
            .instruction_states
            .borrow()
            .iter()
            .all(|state| *state == 2);
        let blocks_complete = self
            .store
            .blocks
            .iter()
            .zip(self.block_states.borrow().iter())
            .all(|(block, state)| block.owner != self.owner || *state == 2);
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

impl FullVerifier {
    fn verify(program: &FullProgram) -> Result<(), IrVerifyError> {
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
            let raw = u32::try_from(index + 1)
                .map_err(|_| IrVerifyError::new("string id overflows"))?;
            store.string(raw)?;
        }
        for index in 0..store.bytes.len() {
            let raw = u32::try_from(index + 1)
                .map_err(|_| IrVerifyError::new("bytes id overflows"))?;
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
            if cold.validation != IR_NONE
                && cold.validation as usize >= store.validations.len()
            {
                return Err(IrVerifyError::new(
                    "parameter validation is out of bounds",
                ));
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
            if store
                .semantic
                .signature_param_count(function.signature)?
                != params.len()
            {
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
                instruction_states: RefCell::new(vec![0; instruction_len]),
                block_states: RefCell::new(vec![0; store.blocks.len()]),
                slot_count: function.slot_count,
            };
            let body_id = IrBlockId::from_raw(function.body)
                .ok_or_else(|| IrVerifyError::new("function body block is invalid"))?;
            let body_block = store
                .blocks
                .get(body_id.index())
                .ok_or_else(|| IrVerifyError::new("function body block is out of bounds"))?;
            if body_block.flags != (BLOCK_STATEMENTS | BLOCK_FUNCTION_BODY) {
                return Err(IrVerifyError::new(
                    "function body block flags are invalid",
                ));
            }
            let body = [function.body];
            let mut cursor = FullCursor::new(&body);
            let statements = Vec::<LoweredStmt>::decode(&decoder, &mut cursor)?;
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
                    LoweredValue::decode(&decoder, &mut cursor)?;
                    cursor.finish()?;
                }
            }
            decoder.finish_function()?;
            if statements.is_empty() {
                return Err(IrVerifyError::new(format!(
                    "function {index} has an empty body"
                )));
            }
            let return_type = store
                .semantic
                .to_type(store.semantic.signature_return_type(function.signature)?)?;
            if !matches!(return_type, Type::Stream(_))
                && !super::super::lower::lowered_body_can_return(&statements)
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
                        return Err(IrVerifyError::new(
                            "driver slot is owned by multiple steps",
                        ));
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
                        return Err(IrVerifyError::new(
                            "driver step slots are not unique",
                        ));
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
                            if let Some((type_id, existing)) =
                                expected_sync.get_mut(&slot.name)
                            {
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
                return Err(IrVerifyError::new("driver plan contains an unreachable region"));
            }
            if covered_sync.iter().any(|covered| !covered) {
                return Err(IrVerifyError::new(
                    "driver plan contains an unreachable sync row",
                ));
            }
            program.decode_driver()?;
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
                ($decode)(input.raw()?)
            }
        }
    };
}

impl_word_codec!(
    u32,
    |value: &u32| Ok(*value),
    |raw| Ok::<u32, IrVerifyError>(raw)
);
impl FullCodec for usize {
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
        if builder.current_owner.is_none() {
            return Err(IrBuildError::format("slot_without_owner", None, 0, 0));
        }
        if *self >= builder.current_slot_count as usize {
            return Err(IrBuildError::format("slot_out_of_bounds", None, 0, 0));
        }
        output.push(
            u32::try_from(*self)
                .map_err(|_| IrBuildError::format("slot_overflow", None, 0, 0))?,
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
impl_word_codec!(
    bool,
    |value: &bool| Ok(u32::from(*value)),
    |raw| match raw {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(IrVerifyError::new("boolean payload is invalid")),
    }
);

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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
        let id = builder.function_ids.get(self).copied().ok_or_else(|| {
            IrBuildError::format("unresolved_function_identity", None, 0, 0)
        })?;
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
        output.push(builder.intern_string(self)?.raw());
        Ok(())
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Arc::from(decoder.store.string(input.raw()?)?))
    }
}

impl FullCodec for String {
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
        output.push(builder.intern_string(self)?.raw());
        Ok(())
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(decoder.store.string(input.raw()?)?.to_string())
    }
}

impl FullCodec for &'static str {
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
        output.push(builder.intern_string(self)?.raw());
        Ok(())
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Name::intern(decoder.store.string(input.raw()?)?).as_str())
    }
}

impl FullCodec for Arc<[u8]> {
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
        output.push(builder.intern_bytes(self)?.raw());
        Ok(())
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Arc::from(decoder.store.bytes(input.raw()?)?))
    }
}

impl FullCodec for Vec<u8> {
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
        output.push(builder.intern_bytes(self)?.raw());
        Ok(())
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(decoder.store.bytes(input.raw()?)?.to_vec())
    }
}

impl FullCodec for PathValue {
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
}

impl<T: FullCodec> FullCodec for Box<T> {
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
        self.as_ref().encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok(Box::new(T::decode(decoder, input)?))
    }
}

impl<A: FullCodec, B: FullCodec> FullCodec for (A, B) {
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
        self.0.encode(builder, output)?;
        self.1.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Ok((A::decode(decoder, input)?, B::decode(decoder, input)?))
    }
}

impl<A: FullCodec, B: FullCodec, C: FullCodec> FullCodec for (A, B, C) {
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
impl_copy_pool_codec!(
    RedirectionKind,
    redirection_kinds,
    "redirection kind"
);

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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
        let mut payload = Vec::new();
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
        output.push(builder.push_value(tag, &payload)?);
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
            FullValueTag::Duration => {
                Self::Duration(DurationValue::decode(decoder, &mut payload)?)
            }
            FullValueTag::Bool => Self::Bool(bool::decode(decoder, &mut payload)?),
            FullValueTag::Str => Self::Str(Arc::<str>::decode(decoder, &mut payload)?),
            FullValueTag::Bytes => Self::Bytes(Arc::<[u8]>::decode(decoder, &mut payload)?),
            FullValueTag::Path => Self::Path(PathValue::decode(decoder, &mut payload)?),
            FullValueTag::Record => {
                Self::Record(BTreeMap::<Arc<str>, LoweredValue>::decode(
                    decoder,
                    &mut payload,
                )?)
            }
            FullValueTag::RecordVec => {
                Self::RecordVec(Vec::<(Name, LoweredValue)>::decode(
                    decoder,
                    &mut payload,
                )?)
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
            FullValueTag::Module => {
                Self::Module(BTreeMap::<Arc<str>, LoweredValue>::decode(
                    decoder,
                    &mut payload,
                )?)
            }
            FullValueTag::List => {
                Self::List(Vec::<LoweredValue>::decode(decoder, &mut payload)?)
            }
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
                            let mut payload = Vec::new();
                            $(
                                $field.encode(builder, &mut payload)?;
                            )*
                            (FullTag::$tag, payload)
                        }
                    ),*
                };
                output.push(builder.push_instruction(tag, &payload)?);
                Ok(())
            }

            fn decode(
                decoder: &FullDecoder<'_>,
                input: &mut FullCursor<'_>,
            ) -> Result<Self, IrVerifyError> {
                let (instruction, tag, mut payload) = decoder.instruction(input)?;
                let value = match tag {
                    $(
                        FullTag::$tag => {
                            $(
                                let $field = <$field_ty>::decode(decoder, &mut payload)?;
                            )*
                            $construct
                        }
                    ),*
                    _ => return Err(IrVerifyError::new("full IR instruction tag has the wrong category")),
                };
                payload.finish()?;
                decoder.finish_instruction(instruction);
                Ok(value)
            }
        }
    };
}

impl_node_codec! {
    LoweredIntExpr {
        LoweredIntExpr::Int(value) => IntInt { value: i64 } => LoweredIntExpr::Int(value),
        LoweredIntExpr::Slot(slot) => IntSlot { slot: usize } => LoweredIntExpr::Slot(slot),
        LoweredIntExpr::Binary { op, left, right } => IntBinary {
            op: BinaryOp,
            left: Box<LoweredIntExpr>,
            right: Box<LoweredIntExpr>,
        } => LoweredIntExpr::Binary { op, left, right },
        LoweredIntExpr::StrByteLenSlot { slot, span } => IntStrByteLenSlot {
            slot: usize,
            span: Span,
        } => LoweredIntExpr::StrByteLenSlot { slot, span },
        LoweredIntExpr::StrCountLinesSlot { slot, span } => IntStrCountLinesSlot {
            slot: usize,
            span: Span,
        } => LoweredIntExpr::StrCountLinesSlot { slot, span },
        LoweredIntExpr::StrByteAtSlot {
            slot,
            index,
            default,
            span,
        } => IntStrByteAtSlot {
            slot: usize,
            index: Box<LoweredIntExpr>,
            default: Option<Box<LoweredIntExpr>>,
            span: Span,
        } => LoweredIntExpr::StrByteAtSlot {
            slot,
            index,
            default,
            span,
        },
    }
}

impl_node_codec! {
    LoweredBoolExpr {
        LoweredBoolExpr::Bool(value) => BoolBool { value: bool } => LoweredBoolExpr::Bool(value),
        LoweredBoolExpr::Slot(slot) => BoolSlot { slot: usize } => LoweredBoolExpr::Slot(slot),
        LoweredBoolExpr::Not(value) => BoolNot {
            value: Box<LoweredBoolExpr>,
        } => LoweredBoolExpr::Not(value),
        LoweredBoolExpr::And(left, right) => BoolAnd {
            left: Box<LoweredBoolExpr>,
            right: Box<LoweredBoolExpr>,
        } => LoweredBoolExpr::And(left, right),
        LoweredBoolExpr::Or(left, right) => BoolOr {
            left: Box<LoweredBoolExpr>,
            right: Box<LoweredBoolExpr>,
        } => LoweredBoolExpr::Or(left, right),
        LoweredBoolExpr::IntCompare { op, left, right } => BoolIntCompare {
            op: BinaryOp,
            left: Box<LoweredIntExpr>,
            right: Box<LoweredIntExpr>,
        } => LoweredBoolExpr::IntCompare { op, left, right },
        LoweredBoolExpr::StrPredicateSlot {
            slot,
            predicate,
            needle,
            span,
        } => BoolStrPredicateSlot {
            slot: usize,
            predicate: LoweredStrPredicate,
            needle: Arc<[u8]>,
            span: Span,
        } => LoweredBoolExpr::StrPredicateSlot {
            slot,
            predicate,
            needle,
            span,
        },
        LoweredBoolExpr::ContainsSlot { slot, needle, span } => BoolContainsSlot {
            slot: usize,
            needle: LoweredValue,
            span: Span,
        } => LoweredBoolExpr::ContainsSlot { slot, needle, span },
        LoweredBoolExpr::StrContainsSlot { slot, needle, span } => BoolStrContainsSlot {
            slot: usize,
            needle: Arc<str>,
            span: Span,
        } => LoweredBoolExpr::StrContainsSlot { slot, needle, span },
        LoweredBoolExpr::TrimEmptySlot { slot, span } => BoolTrimEmptySlot {
            slot: usize,
            span: Span,
        } => LoweredBoolExpr::TrimEmptySlot { slot, span },
        LoweredBoolExpr::TrimStrPredicateSlot {
            slot,
            predicate,
            needle,
            span,
        } => BoolTrimStrPredicateSlot {
            slot: usize,
            predicate: LoweredStrPredicate,
            needle: Arc<[u8]>,
            span: Span,
        } => LoweredBoolExpr::TrimStrPredicateSlot {
            slot,
            predicate,
            needle,
            span,
        },
        LoweredBoolExpr::LiteralCompareSlot { op, slot, value } => BoolLiteralCompareSlot {
            op: BinaryOp,
            slot: usize,
            value: LoweredValue,
        } => LoweredBoolExpr::LiteralCompareSlot { op, slot, value },
    }
}

const BLOCK_LIST: u8 = 0;
const BLOCK_STATEMENTS: u8 = 1;
const BLOCK_FUNCTION_BODY: u8 = 1 << 1;
const BLOCK_SEQUENCE_KIND_MASK: u8 = 1;

impl_vec_codec!(LoweredStmt, BLOCK_STATEMENTS);
impl_vec_codec!(LoweredExpr, BLOCK_LIST);
impl_vec_codec!(LoweredPattern, BLOCK_LIST);
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
impl_vec_codec!((Arc<str>, LoweredExpr), BLOCK_LIST);
impl_vec_codec!((Arc<str>, Vec<LoweredStmt>), BLOCK_LIST);
impl_vec_codec!((LoweredExpr, LoweredExpr), BLOCK_LIST);
impl_vec_codec!((LoweredExpr, Vec<LoweredStmt>), BLOCK_LIST);
impl_vec_codec!((LoweredBoolExpr, Vec<LoweredStmt>), BLOCK_LIST);
impl_vec_codec!((LoweredPattern, Option<LoweredExpr>, LoweredExpr), BLOCK_LIST);
impl_vec_codec!(
    (LoweredPattern, Option<LoweredExpr>, Vec<LoweredStmt>),
    BLOCK_LIST
);

impl<A> FullCodec for SmallVec<A>
where
    A: smallvec::Array,
    A::Item: FullCodec,
{
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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

impl_fx_map_codec!(LoweredExpr);
impl_fx_map_codec!(Vec<LoweredStmt>);

impl FullCodec for LoweredCompTarget {
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
                LoweredExpr::decode(decoder, input)?,
            )),
            1 => Ok(Self::Spread(LoweredExpr::decode(decoder, input)?)),
            _ => Err(IrVerifyError::new("record entry tag is invalid")),
        }
    }
}

impl FullCodec for LoweredCallArg {
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
            0 => Ok(Self::Single(LoweredExpr::decode(decoder, input)?)),
            1 => Ok(Self::Splice(LoweredExpr::decode(decoder, input)?)),
            _ => Err(IrVerifyError::new("call argument tag is invalid")),
        }
    }
}

impl FullCodec for LoweredFmtPart {
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
                LoweredExpr::decode(decoder, input)?,
                Span::decode(decoder, input)?,
                Option::<FormatSpec>::decode(decoder, input)?,
            )),
            _ => Err(IrVerifyError::new("format part tag is invalid")),
        }
    }
}

impl FullCodec for LoweredRunArgKind {
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
        let value = LoweredExpr::decode(decoder, input)?;
        match tag {
            0 => Ok(Self::Single(value)),
            1 => Ok(Self::SingleOrSplice(value)),
            2 => Ok(Self::Splice(value)),
            _ => Err(IrVerifyError::new("run argument tag is invalid")),
        }
    }
}

impl FullCodec for LoweredRunArg {
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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

impl FullCodec for LoweredProcessCommandArgv {
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
            target: Box::decode(decoder, input)?,
            argv: Box::decode(decoder, input)?,
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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
                value: LoweredExpr::decode(decoder, input)?,
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
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
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

impl FullCodec for LoweredPattern {
    fn encode(
        &self,
        builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
        let mut payload = Vec::new();
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
        output.push(builder.push_pattern(tag, &payload)?);
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
            FullPatternTag::Literal => {
                Self::Literal(LoweredValue::decode(decoder, &mut payload)?)
            }
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
                            let mut payload = Vec::new();
                            $(
                                $field.encode(builder, &mut payload)?;
                            )*
                            (FullStageTag::$tag, payload)
                        }
                    ),*
                };
                output.push(builder.push_stage(tag, &payload)?);
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
        }
    };
}

impl_stage_codec! {
    LoweredPipelineStage::TextLines => TextLines {} => LoweredPipelineStage::TextLines,
    LoweredPipelineStage::JsonLines => JsonLines {} => LoweredPipelineStage::JsonLines,
    LoweredPipelineStage::Where { slot, predicate } => Where {
        slot: usize,
        predicate: LoweredExpr,
    } => LoweredPipelineStage::Where { slot, predicate },
    LoweredPipelineStage::Map { slot, value } => Map {
        slot: usize,
        value: LoweredExpr,
    } => LoweredPipelineStage::Map { slot, value },
    LoweredPipelineStage::MapBlock { slot, body, value } => MapBlock {
        slot: usize,
        body: Vec<LoweredStmt>,
        value: LoweredExpr,
    } => LoweredPipelineStage::MapBlock { slot, body, value },
    LoweredPipelineStage::FlatMap { slot, value } => FlatMap {
        slot: usize,
        value: LoweredExpr,
    } => LoweredPipelineStage::FlatMap { slot, value },
    LoweredPipelineStage::FlatMapBlock { slot, body, value } => FlatMapBlock {
        slot: usize,
        body: Vec<LoweredStmt>,
        value: LoweredExpr,
    } => LoweredPipelineStage::FlatMapBlock { slot, body, value },
    LoweredPipelineStage::BytesChunks { size } => BytesChunks {
        size: LoweredExpr,
    } => LoweredPipelineStage::BytesChunks { size },
    LoweredPipelineStage::BatchCount { count } => BatchCount {
        count: LoweredExpr,
    } => LoweredPipelineStage::BatchCount { count },
    LoweredPipelineStage::BatchMaxArgv { max_argv } => BatchMaxArgv {
        max_argv: Option<LoweredExpr>,
    } => LoweredPipelineStage::BatchMaxArgv { max_argv },
    LoweredPipelineStage::BatchMaxBytes { max_bytes } => BatchMaxBytes {
        max_bytes: LoweredExpr,
    } => LoweredPipelineStage::BatchMaxBytes { max_bytes },
    LoweredPipelineStage::Shuffle { seed } => Shuffle {
        seed: Option<LoweredExpr>,
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
        initial: LoweredExpr,
        body: Vec<LoweredStmt>,
        value: LoweredExpr,
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
        body: Vec<LoweredStmt>,
        value: LoweredExpr,
        op: ReduceByOp,
    } => LoweredPipelineStage::ReduceBy {
        item_slot,
        body,
        value,
        op,
    },
    LoweredPipelineStage::ParMap { slot, value } => ParMap {
        slot: usize,
        value: LoweredExpr,
    } => LoweredPipelineStage::ParMap { slot, value },
    LoweredPipelineStage::ParMapBlock { slot, body, value } => ParMapBlock {
        slot: usize,
        body: Vec<LoweredStmt>,
        value: LoweredExpr,
    } => LoweredPipelineStage::ParMapBlock { slot, body, value },
    LoweredPipelineStage::Tee { slot, body } => Tee {
        slot: usize,
        body: Vec<LoweredStmt>,
    } => LoweredPipelineStage::Tee { slot, body },
    LoweredPipelineStage::Each {
        slot,
        body,
        parallel,
    } => Each {
        slot: usize,
        body: Vec<LoweredStmt>,
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
        other: LoweredExpr,
    } => LoweredPipelineStage::Zip { other },
    LoweredPipelineStage::Sort { descending } => Sort {
        descending: Option<LoweredExpr>,
    } => LoweredPipelineStage::Sort { descending },
    LoweredPipelineStage::SortBy {
        slot,
        key,
        descending,
    } => SortBy {
        slot: usize,
        key: LoweredExpr,
        descending: Option<LoweredExpr>,
    } => LoweredPipelineStage::SortBy {
        slot,
        key,
        descending,
    },
    LoweredPipelineStage::GroupBy { slot, key } => GroupBy {
        slot: usize,
        key: LoweredExpr,
    } => LoweredPipelineStage::GroupBy { slot, key },
    LoweredPipelineStage::CountBy { slot, key } => CountBy {
        slot: usize,
        key: LoweredExpr,
    } => LoweredPipelineStage::CountBy { slot, key },
    LoweredPipelineStage::Any { slot, predicate } => Any {
        slot: usize,
        predicate: LoweredExpr,
    } => LoweredPipelineStage::Any { slot, predicate },
    LoweredPipelineStage::All { slot, predicate } => All {
        slot: usize,
        predicate: LoweredExpr,
    } => LoweredPipelineStage::All { slot, predicate },
    LoweredPipelineStage::UniqueBy { slot, key } => UniqueBy {
        slot: usize,
        key: LoweredExpr,
    } => LoweredPipelineStage::UniqueBy { slot, key },
    LoweredPipelineStage::Count => Count {} => LoweredPipelineStage::Count,
    LoweredPipelineStage::Sum => Sum {} => LoweredPipelineStage::Sum,
    LoweredPipelineStage::Collect => Collect {} => LoweredPipelineStage::Collect,
    LoweredPipelineStage::First => First {} => LoweredPipelineStage::First,
    LoweredPipelineStage::Last => Last {} => LoweredPipelineStage::Last,
    LoweredPipelineStage::Min => Min {} => LoweredPipelineStage::Min,
    LoweredPipelineStage::Max => Max {} => LoweredPipelineStage::Max,
    LoweredPipelineStage::Take(value) => Take {
        value: LoweredExpr,
    } => LoweredPipelineStage::Take(value),
    LoweredPipelineStage::Drop(value) => Drop {
        value: LoweredExpr,
    } => LoweredPipelineStage::Drop(value),
    LoweredPipelineStage::Repeat { count } => Repeat {
        count: LoweredExpr,
    } => LoweredPipelineStage::Repeat { count },
    LoweredPipelineStage::Range { start, end } => Range {
        start: LoweredExpr,
        end: LoweredExpr,
    } => LoweredPipelineStage::Range { start, end },
}

impl_node_codec! {
    LoweredExpr {
        LoweredExpr::Null => ExprNull {} => LoweredExpr::Null,
        LoweredExpr::Unit => ExprUnit {} => LoweredExpr::Unit,
        LoweredExpr::Int(value) => ExprInt { value: i64 } => LoweredExpr::Int(value),
        LoweredExpr::Float(value) => ExprFloat {
            value: FloatValue,
        } => LoweredExpr::Float(value),
        LoweredExpr::Duration(value) => ExprDuration {
            value: DurationValue,
        } => LoweredExpr::Duration(value),
        LoweredExpr::Bool(value) => ExprBool { value: bool } => LoweredExpr::Bool(value),
        LoweredExpr::Str(value) => ExprStr {
            value: Arc<str>,
        } => LoweredExpr::Str(value),
        LoweredExpr::Bytes(value) => ExprBytes {
            value: Arc<[u8]>,
        } => LoweredExpr::Bytes(value),
        LoweredExpr::Path(value) => ExprPath {
            value: PathValue,
        } => LoweredExpr::Path(value),
        LoweredExpr::FunctionRef { function, pure } => ExprFunctionRef {
            function: FunctionName,
            pure: bool,
        } => LoweredExpr::FunctionRef { function, pure },
        LoweredExpr::PathFrom { value, span } => ExprPathFrom {
            value: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::PathFrom { value, span },
        LoweredExpr::Param(slot) => ExprParam {
            slot: usize,
        } => LoweredExpr::Param(slot),
        LoweredExpr::Binary {
            op,
            left,
            right,
            span,
        } => ExprBinary {
            op: BinaryOp,
            left: Box<LoweredExpr>,
            right: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::Binary {
            op,
            left,
            right,
            span,
        },
        LoweredExpr::IfExpr {
            branches,
            else_value,
            span,
        } => ExprIf {
            branches: Vec<(LoweredExpr, LoweredExpr)>,
            else_value: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::IfExpr {
            branches,
            else_value,
            span,
        },
        LoweredExpr::MatchExpr { value, arms, span } => ExprMatch {
            value: Box<LoweredExpr>,
            arms: Vec<(LoweredPattern, Option<LoweredExpr>, LoweredExpr)>,
            span: Span,
        } => LoweredExpr::MatchExpr { value, arms, span },
        LoweredExpr::StrMatchExpr {
            value,
            arms,
            fallback,
            span,
        } => ExprStrMatch {
            value: Box<LoweredExpr>,
            arms: FxHashMap<Arc<str>, LoweredExpr>,
            fallback: Option<Box<LoweredExpr>>,
            span: Span,
        } => LoweredExpr::StrMatchExpr {
            value,
            arms,
            fallback,
            span,
        },
        LoweredExpr::TagMatchExpr {
            value,
            arms,
            fallback,
            span,
        } => ExprTagMatch {
            value: Box<LoweredExpr>,
            arms: FxHashMap<Arc<str>, LoweredExpr>,
            fallback: Option<Box<LoweredExpr>>,
            span: Span,
        } => LoweredExpr::TagMatchExpr {
            value,
            arms,
            fallback,
            span,
        },
        LoweredExpr::ResultFallback { left, right } => ExprResultFallback {
            left: Box<LoweredExpr>,
            right: Box<LoweredExpr>,
        } => LoweredExpr::ResultFallback { left, right },
        LoweredExpr::FmtString(parts) => ExprFmtString {
            parts: Vec<LoweredFmtPart>,
        } => LoweredExpr::FmtString(parts),
        LoweredExpr::PathFmtString { parts, span } => ExprPathFmtString {
            parts: Vec<LoweredFmtPart>,
            span: Span,
        } => LoweredExpr::PathFmtString { parts, span },
        LoweredExpr::Glob { pattern, span } => ExprGlob {
            pattern: Arc<str>,
            span: Span,
        } => LoweredExpr::Glob { pattern, span },
        LoweredExpr::LastStatus { span } => ExprLastStatus {
            span: Span,
        } => LoweredExpr::LastStatus { span },
        LoweredExpr::Record(entries) => ExprRecord {
            entries: Vec<LoweredRecordEntry>,
        } => LoweredExpr::Record(entries),
        LoweredExpr::List(values) => ExprList {
            values: Vec<LoweredExpr>,
        } => LoweredExpr::List(values),
        LoweredExpr::EmptyMap => ExprEmptyMap {} => LoweredExpr::EmptyMap,
        LoweredExpr::BytesConcat { arg, span } => ExprBytesConcat {
            arg: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::BytesConcat { arg, span },
        LoweredExpr::Range { start, end, span } => ExprRange {
            start: Box<LoweredExpr>,
            end: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::Range { start, end, span },
        LoweredExpr::Tag { name, fields } => ExprTag {
            name: Arc<str>,
            fields: Vec<LoweredExpr>,
        } => LoweredExpr::Tag { name, fields },
        LoweredExpr::ListComp {
            value,
            target,
            iter,
            condition,
            span,
        } => ExprListComp {
            value: Box<LoweredExpr>,
            target: Box<LoweredCompTarget>,
            iter: Box<LoweredExpr>,
            condition: Option<Box<LoweredExpr>>,
            span: Span,
        } => LoweredExpr::ListComp {
            value,
            target,
            iter,
            condition,
            span,
        },
        LoweredExpr::MapComp {
            key,
            value,
            target,
            iter,
            condition,
            span,
        } => ExprMapComp {
            key: Box<LoweredExpr>,
            value: Box<LoweredExpr>,
            target: Box<LoweredCompTarget>,
            iter: Box<LoweredExpr>,
            condition: Option<Box<LoweredExpr>>,
            span: Span,
        } => LoweredExpr::MapComp {
            key,
            value,
            target,
            iter,
            condition,
            span,
        },
        LoweredExpr::ListPipeline {
            input,
            stages,
            span,
        } => ExprPipeline {
            input: Box<LoweredExpr>,
            stages: Vec<LoweredPipelineStage>,
            span: Span,
        } => LoweredExpr::ListPipeline {
            input,
            stages,
            span,
        },
        LoweredExpr::Field { base, name, span } => ExprField {
            base: Box<LoweredExpr>,
            name: &'static str,
            span: Span,
        } => LoweredExpr::Field { base, name, span },
        LoweredExpr::Index { base, index, span } => ExprIndex {
            base: Box<LoweredExpr>,
            index: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::Index { base, index, span },
        LoweredExpr::Slice {
            base,
            start,
            end,
            span,
        } => ExprSlice {
            base: Box<LoweredExpr>,
            start: Option<Box<LoweredExpr>>,
            end: Option<Box<LoweredExpr>>,
            span: Span,
        } => LoweredExpr::Slice {
            base,
            start,
            end,
            span,
        },
        LoweredExpr::Method {
            receiver,
            name,
            args,
            span,
        } => ExprMethod {
            receiver: Box<LoweredExpr>,
            name: &'static str,
            args: Vec<LoweredExpr>,
            span: Span,
        } => LoweredExpr::Method {
            receiver,
            name,
            args,
            span,
        },
        LoweredExpr::StrByteLen { receiver, span } => ExprStrByteLen {
            receiver: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::StrByteLen { receiver, span },
        LoweredExpr::StrByteAt {
            receiver,
            index,
            default,
            span,
        } => ExprStrByteAt {
            receiver: Box<LoweredExpr>,
            index: Box<LoweredExpr>,
            default: Option<Box<LoweredExpr>>,
            span: Span,
        } => LoweredExpr::StrByteAt {
            receiver,
            index,
            default,
            span,
        },
        LoweredExpr::StrPredicate {
            receiver,
            predicate,
            needle,
            span,
        } => ExprStrPredicate {
            receiver: Box<LoweredExpr>,
            predicate: LoweredStrPredicate,
            needle: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::StrPredicate {
            receiver,
            predicate,
            needle,
            span,
        },
        LoweredExpr::Contains {
            receiver,
            needle,
            span,
        } => ExprContains {
            receiver: Box<LoweredExpr>,
            needle: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::Contains {
            receiver,
            needle,
            span,
        },
        LoweredExpr::RegexCompile { pattern, span } => ExprRegexCompile {
            pattern: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::RegexCompile { pattern, span },
        LoweredExpr::Require { value, check, span } => ExprRequire {
            value: Box<LoweredExpr>,
            check: LoweredTypeCheck,
            span: Span,
        } => LoweredExpr::Require { value, check, span },
        LoweredExpr::RunCapture(value) => ExprRunCapture {
            value: Box<LoweredRunCapture>,
        } => LoweredExpr::RunCapture(value),
        LoweredExpr::RunPipeline {
            segments,
            propagate,
            span,
        } => ExprRunPipeline {
            segments: Vec<LoweredRunPipelineSegment>,
            propagate: bool,
            span: Span,
        } => LoweredExpr::RunPipeline {
            segments,
            propagate,
            span,
        },
        LoweredExpr::SpawnRun(value) => ExprSpawnRun {
            value: Box<LoweredSpawnRun>,
        } => LoweredExpr::SpawnRun(value),
        LoweredExpr::SpawnCommand { command, span } => ExprSpawnCommand {
            command: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::SpawnCommand { command, span },
        LoweredExpr::Wait { target, span } => ExprWait {
            target: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::Wait { target, span },
        LoweredExpr::Loop { body, span } => ExprLoop {
            body: Vec<LoweredStmt>,
            span: Span,
        } => LoweredExpr::Loop { body, span },
        LoweredExpr::Retry { delays, body, span } => ExprRetry {
            delays: Vec<LoweredExpr>,
            body: Vec<LoweredStmt>,
            span: Span,
        } => LoweredExpr::Retry { delays, body, span },
        LoweredExpr::FsFiles {
            root,
            gitignore,
            stat,
            hidden,
            exts,
            result_wrapped,
            span,
        } => ExprFsFiles {
            root: Box<LoweredExpr>,
            gitignore: bool,
            stat: bool,
            hidden: bool,
            exts: Option<Box<LoweredExpr>>,
            result_wrapped: bool,
            span: Span,
        } => LoweredExpr::FsFiles {
            root,
            gitignore,
            stat,
            hidden,
            exts,
            result_wrapped,
            span,
        },
        LoweredExpr::FsWalk {
            root,
            gitignore,
            stat,
            hidden,
            exts,
            result_wrapped,
            span,
        } => ExprFsWalk {
            root: Box<LoweredExpr>,
            gitignore: bool,
            stat: bool,
            hidden: bool,
            exts: Option<Box<LoweredExpr>>,
            result_wrapped: bool,
            span: Span,
        } => LoweredExpr::FsWalk {
            root,
            gitignore,
            stat,
            hidden,
            exts,
            result_wrapped,
            span,
        },
        LoweredExpr::FsList {
            op,
            path,
            stat,
            ordered,
            span,
        } => ExprFsList {
            op: RuntimeOp,
            path: Box<LoweredExpr>,
            stat: Option<Box<LoweredExpr>>,
            ordered: Option<Box<LoweredExpr>>,
            span: Span,
        } => LoweredExpr::FsList {
            op,
            path,
            stat,
            ordered,
            span,
        },
        LoweredExpr::FsTempDir { span } => ExprFsTempDir {
            span: Span,
        } => LoweredExpr::FsTempDir { span },
        LoweredExpr::FsWrite { path, data, span } => ExprFsWrite {
            path: Box<LoweredExpr>,
            data: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::FsWrite { path, data, span },
        LoweredExpr::FsMkdir {
            path,
            parents,
            span,
        } => ExprFsMkdir {
            path: Box<LoweredExpr>,
            parents: Option<Box<LoweredExpr>>,
            span: Span,
        } => LoweredExpr::FsMkdir {
            path,
            parents,
            span,
        },
        LoweredExpr::FsRemove {
            path,
            missing_ok,
            span,
        } => ExprFsRemove {
            path: Box<LoweredExpr>,
            missing_ok: Option<Box<LoweredExpr>>,
            span: Span,
        } => LoweredExpr::FsRemove {
            path,
            missing_ok,
            span,
        },
        LoweredExpr::FsCloseRoot { root, span } => ExprFsCloseRoot {
            root: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::FsCloseRoot { root, span },
        LoweredExpr::FsRootPath { root, span } => ExprFsRootPath {
            root: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::FsRootPath { root, span },
        LoweredExpr::PathReadText { path, span } => ExprPathReadText {
            path: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::PathReadText { path, span },
        LoweredExpr::PathReadBytes { path, span } => ExprPathReadBytes {
            path: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::PathReadBytes { path, span },
        LoweredExpr::PathExists { path, span } => ExprPathExists {
            path: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::PathExists { path, span },
        LoweredExpr::PathExecutable { path, span } => ExprPathExecutable {
            path: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::PathExecutable { path, span },
        LoweredExpr::PathDu { path, span } => ExprPathDu {
            path: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::PathDu { path, span },
        LoweredExpr::PathMetadata { path, span } => ExprPathMetadata {
            path: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::PathMetadata { path, span },
        LoweredExpr::PathReadlink { path, span } => ExprPathReadlink {
            path: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::PathReadlink { path, span },
        LoweredExpr::PathResolve { path, span } => ExprPathResolve {
            path: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::PathResolve { path, span },
        LoweredExpr::PathWrite {
            path,
            data,
            atomic,
            span,
        } => ExprPathWrite {
            path: Box<LoweredExpr>,
            data: Box<LoweredExpr>,
            atomic: bool,
            span: Span,
        } => LoweredExpr::PathWrite {
            path,
            data,
            atomic,
            span,
        },
        LoweredExpr::PathMkdir {
            path,
            parents,
            span,
        } => ExprPathMkdir {
            path: Box<LoweredExpr>,
            parents: Option<Box<LoweredExpr>>,
            span: Span,
        } => LoweredExpr::PathMkdir {
            path,
            parents,
            span,
        },
        LoweredExpr::PathRemove {
            path,
            missing_ok,
            span,
        } => ExprPathRemove {
            path: Box<LoweredExpr>,
            missing_ok: Option<Box<LoweredExpr>>,
            span: Span,
        } => LoweredExpr::PathRemove {
            path,
            missing_ok,
            span,
        },
        LoweredExpr::JsonEncode { value, span } => ExprJsonEncode {
            value: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::JsonEncode { value, span },
        LoweredExpr::ArchiveTarCreate {
            path,
            root,
            entries,
            compression,
            overwrite,
            span,
        } => ExprArchiveTarCreate {
            path: Box<LoweredExpr>,
            root: Box<LoweredExpr>,
            entries: Box<LoweredExpr>,
            compression: Option<Box<LoweredExpr>>,
            overwrite: Option<Box<LoweredExpr>>,
            span: Span,
        } => LoweredExpr::ArchiveTarCreate {
            path,
            root,
            entries,
            compression,
            overwrite,
            span,
        },
        LoweredExpr::ArchiveTarList { path, span } => ExprArchiveTarList {
            path: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::ArchiveTarList { path, span },
        LoweredExpr::ArchiveTarExtract { path, dest, span } => ExprArchiveTarExtract {
            path: Box<LoweredExpr>,
            dest: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::ArchiveTarExtract { path, dest, span },
        LoweredExpr::HashVerifyFile {
            path,
            algorithm,
            expected,
            span,
        } => ExprHashVerifyFile {
            path: Box<LoweredExpr>,
            algorithm: HashAlgorithm,
            expected: Box<LoweredExpr>,
            span: Span,
        } => LoweredExpr::HashVerifyFile {
            path,
            algorithm,
            expected,
            span,
        },
        LoweredExpr::ModuleCall { op, args, span } => ExprModuleCall {
            op: RuntimeOp,
            args: Vec<LoweredExpr>,
            span: Span,
        } => LoweredExpr::ModuleCall { op, args, span },
        LoweredExpr::ProcessCommandArgv(value) => ExprProcessCommandArgv {
            value: Box<LoweredProcessCommandArgv>,
        } => LoweredExpr::ProcessCommandArgv(value),
        LoweredExpr::ProcessCommandBuilder { entries, span } => ExprProcessCommandBuilder {
            entries: Vec<LoweredProcessCommandBuilderEntry>,
            span: Span,
        } => LoweredExpr::ProcessCommandBuilder { entries, span },
        LoweredExpr::Abort {
            status,
            force,
            span,
        } => ExprAbort {
            status: Box<LoweredExpr>,
            force: Option<Box<LoweredExpr>>,
            span: Span,
        } => LoweredExpr::Abort {
            status,
            force,
            span,
        },
        LoweredExpr::Ok(value) => ExprOk {
            value: Box<LoweredExpr>,
        } => LoweredExpr::Ok(value),
        LoweredExpr::Err(value) => ExprErr {
            value: Box<LoweredExpr>,
        } => LoweredExpr::Err(value),
        LoweredExpr::Error(value) => ExprError {
            value: Box<LoweredErrorExpr>,
        } => LoweredExpr::Error(value),
        LoweredExpr::Try(value) => ExprTry {
            value: Box<LoweredExpr>,
        } => LoweredExpr::Try(value),
        LoweredExpr::Call {
            function,
            args,
            span,
        } => ExprCall {
            function: LoweredFunctionKey,
            args: Vec<LoweredCallArg>,
            span: Span,
        } => LoweredExpr::Call {
            function,
            args,
            span,
        },
        LoweredExpr::DynamicCall { callee, args, span } => ExprDynamicCall {
            callee: Box<LoweredExpr>,
            args: Vec<LoweredCallArg>,
            span: Span,
        } => LoweredExpr::DynamicCall { callee, args, span },
        LoweredExpr::SelfCall { args, span } => ExprSelfCall {
            args: Vec<LoweredCallArg>,
            span: Span,
        } => LoweredExpr::SelfCall { args, span },
    }
}

impl_node_codec! {
    LoweredStmt {
        LoweredStmt::Let { slot, value } => StmtLet {
            slot: usize,
            value: LoweredExpr,
        } => LoweredStmt::Let { slot, value },
        LoweredStmt::Guard {
            slot,
            value,
            else_param_slot,
            else_body,
            span,
        } => StmtGuard {
            slot: usize,
            value: LoweredExpr,
            else_param_slot: Option<usize>,
            else_body: Vec<LoweredStmt>,
            span: Span,
        } => LoweredStmt::Guard {
            slot,
            value,
            else_param_slot,
            else_body,
            span,
        },
        LoweredStmt::LetInt { slot, value } => StmtLetInt {
            slot: usize,
            value: LoweredIntExpr,
        } => LoweredStmt::LetInt { slot, value },
        LoweredStmt::LetBool { slot, value } => StmtLetBool {
            slot: usize,
            value: LoweredBoolExpr,
        } => LoweredStmt::LetBool { slot, value },
        LoweredStmt::Assign {
            slot,
            op,
            value,
            span,
        } => StmtAssign {
            slot: usize,
            op: AssignOp,
            value: LoweredExpr,
            span: Span,
        } => LoweredStmt::Assign {
            slot,
            op,
            value,
            span,
        },
        LoweredStmt::AssignField {
            slot,
            field,
            op,
            value,
            span,
        } => StmtAssignField {
            slot: usize,
            field: Arc<str>,
            op: AssignOp,
            value: LoweredExpr,
            span: Span,
        } => LoweredStmt::AssignField {
            slot,
            field,
            op,
            value,
            span,
        },
        LoweredStmt::AssignFieldInt {
            slot,
            field,
            op,
            value,
            span,
        } => StmtAssignFieldInt {
            slot: usize,
            field: Arc<str>,
            op: AssignOp,
            value: LoweredIntExpr,
            span: Span,
        } => LoweredStmt::AssignFieldInt {
            slot,
            field,
            op,
            value,
            span,
        },
        LoweredStmt::AssignIndex {
            slot,
            index,
            op,
            value,
            span,
        } => StmtAssignIndex {
            slot: usize,
            index: Box<LoweredExpr>,
            op: AssignOp,
            value: Box<LoweredExpr>,
            span: Span,
        } => LoweredStmt::AssignIndex {
            slot,
            index,
            op,
            value,
            span,
        },
        LoweredStmt::AssignInt {
            slot,
            op,
            value,
            span,
        } => StmtAssignInt {
            slot: usize,
            op: AssignOp,
            value: LoweredIntExpr,
            span: Span,
        } => LoweredStmt::AssignInt {
            slot,
            op,
            value,
            span,
        },
        LoweredStmt::AssignBool { slot, value } => StmtAssignBool {
            slot: usize,
            value: LoweredBoolExpr,
        } => LoweredStmt::AssignBool { slot, value },
        LoweredStmt::Expr { value, span } => StmtExpr {
            value: LoweredExpr,
            span: Span,
        } => LoweredStmt::Expr { value, span },
        LoweredStmt::If {
            branches,
            else_body,
        } => StmtIf {
            branches: Vec<(LoweredExpr, Vec<LoweredStmt>)>,
            else_body: Option<Vec<LoweredStmt>>,
        } => LoweredStmt::If {
            branches,
            else_body,
        },
        LoweredStmt::IfBool {
            branches,
            else_body,
        } => StmtIfBool {
            branches: Vec<(LoweredBoolExpr, Vec<LoweredStmt>)>,
            else_body: Option<Vec<LoweredStmt>>,
        } => LoweredStmt::IfBool {
            branches,
            else_body,
        },
        LoweredStmt::While { condition, body } => StmtWhile {
            condition: LoweredExpr,
            body: Vec<LoweredStmt>,
        } => LoweredStmt::While { condition, body },
        LoweredStmt::WhileBool { condition, body } => StmtWhileBool {
            condition: LoweredBoolExpr,
            body: Vec<LoweredStmt>,
        } => LoweredStmt::WhileBool { condition, body },
        LoweredStmt::Match { value, arms, span } => StmtMatch {
            value: LoweredExpr,
            arms: Vec<(LoweredPattern, Option<LoweredExpr>, Vec<LoweredStmt>)>,
            span: Span,
        } => LoweredStmt::Match { value, arms, span },
        LoweredStmt::StrMatch {
            value,
            arms,
            fallback,
            span,
        } => StmtStrMatch {
            value: LoweredExpr,
            arms: FxHashMap<Arc<str>, Vec<LoweredStmt>>,
            fallback: Option<Vec<LoweredStmt>>,
            span: Span,
        } => LoweredStmt::StrMatch {
            value,
            arms,
            fallback,
            span,
        },
        LoweredStmt::TagMatch {
            value,
            arms,
            fallback,
            span,
        } => StmtTagMatch {
            value: LoweredExpr,
            arms: FxHashMap<Arc<str>, Vec<LoweredStmt>>,
            fallback: Option<Vec<LoweredStmt>>,
            span: Span,
        } => LoweredStmt::TagMatch {
            value,
            arms,
            fallback,
            span,
        },
        LoweredStmt::For {
            slot,
            iter,
            body,
            span,
        } => StmtFor {
            slot: usize,
            iter: LoweredExpr,
            body: Vec<LoweredStmt>,
            span: Span,
        } => LoweredStmt::For {
            slot,
            iter,
            body,
            span,
        },
        LoweredStmt::LetRecord {
            source,
            fields,
            span,
        } => StmtLetRecord {
            source: LoweredExpr,
            fields: Vec<(Name, usize)>,
            span: Span,
        } => LoweredStmt::LetRecord {
            source,
            fields,
            span,
        },
        LoweredStmt::ForRecord {
            fields,
            iter,
            body,
            span,
        } => StmtForRecord {
            fields: Vec<(Name, usize)>,
            iter: LoweredExpr,
            body: Vec<LoweredStmt>,
            span: Span,
        } => LoweredStmt::ForRecord {
            fields,
            iter,
            body,
            span,
        },
        LoweredStmt::ForStrLines {
            slot,
            text,
            body,
            span,
        } => StmtForStrLines {
            slot: usize,
            text: LoweredExpr,
            body: Vec<LoweredStmt>,
            span: Span,
        } => LoweredStmt::ForStrLines {
            slot,
            text,
            body,
            span,
        },
        LoweredStmt::ScanLines {
            text_slot,
            line_slot,
            checks,
            span,
        } => StmtScanLines {
            text_slot: usize,
            line_slot: usize,
            checks: Vec<ScanCheck>,
            span: Span,
        } => LoweredStmt::ScanLines {
            text_slot,
            line_slot,
            checks,
            span,
        },
        LoweredStmt::Print {
            args,
            stderr,
            flush,
            propagate_result,
            span,
        } => StmtPrint {
            args: Vec<LoweredExpr>,
            stderr: bool,
            flush: bool,
            propagate_result: bool,
            span: Span,
        } => LoweredStmt::Print {
            args,
            stderr,
            flush,
            propagate_result,
            span,
        },
        LoweredStmt::Cd { target, body, span } => StmtCd {
            target: LoweredExpr,
            body: Vec<LoweredStmt>,
            span: Span,
        } => LoweredStmt::Cd { target, body, span },
        LoweredStmt::Env { env, body } => StmtEnv {
            env: Vec<LoweredRunEnv>,
            body: Vec<LoweredStmt>,
        } => LoweredStmt::Env { env, body },
        LoweredStmt::Proc {
            op,
            args,
            propagate_result,
            span,
        } => StmtProc {
            op: RuntimeOp,
            args: Vec<LoweredExpr>,
            propagate_result: bool,
            span: Span,
        } => LoweredStmt::Proc {
            op,
            args,
            propagate_result,
            span,
        },
        LoweredStmt::Run {
            value,
            propagate_result,
        } => StmtRun {
            value: LoweredExpr,
            propagate_result: bool,
        } => LoweredStmt::Run {
            value,
            propagate_result,
        },
        LoweredStmt::Loop { body } => StmtLoop {
            body: Vec<LoweredStmt>,
        } => LoweredStmt::Loop { body },
        LoweredStmt::Return { value } => StmtReturn {
            value: LoweredExpr,
        } => LoweredStmt::Return { value },
        LoweredStmt::Yield { value } => StmtYield {
            value: LoweredExpr,
        } => LoweredStmt::Yield { value },
        LoweredStmt::Break => StmtBreak {} => LoweredStmt::Break,
        LoweredStmt::BreakValue { value } => StmtBreakValue {
            value: LoweredExpr,
        } => LoweredStmt::BreakValue { value },
        LoweredStmt::Continue => StmtContinue {} => LoweredStmt::Continue,
        LoweredStmt::Defer { value } => StmtDefer {
            value: LoweredExpr,
        } => LoweredStmt::Defer { value },
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

    const VERTICAL_SLICE: &str =
        include_str!("../../../../tests/fixtures/frontend-campaign/vertical-slice.xsh");
    const PHASE5_BOUNDARY: &str = r#"
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
  PHASE5_BOUNDARY = "indexed"
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

    fn fixture(
        name: &str,
        source: &str,
    ) -> (Arc<SourceMap>, SourceId, Vec<LoweredFunctionUnit>) {
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
        let units = super::super::super::probe_compact_lower_function_units(
            &parsed.arena,
            &declarations,
            &bodies,
            source,
        );
        (Arc::new(sources), source_id, units)
    }

    #[test]
    fn full_indexed_program_roundtrips_every_vertical_function() {
        let (sources, source_id, units) = fixture("vertical-slice.xsh", VERTICAL_SLICE);
        let program = FullBuilder::build(&units, sources, source_id).unwrap();
        let decoded = program.decode_functions().unwrap();

        assert_eq!(decoded.len(), units.len());
        for (key, kind, body) in decoded {
            let unit = units.iter().find(|unit| unit.key() == key).unwrap();
            assert_eq!(kind, unit.kind());
            assert_eq!(
                format!("{body:?}"),
                format!("{:?}", unit.lowered_body().unwrap())
            );
        }
        assert!(program.instruction_count() > 0);
        assert!(program.extra_words() > 0);
        assert!(program.retained_bytes() > size_of::<FullProgram>());
    }

    fn run_original(
        name: Name,
    ) -> (
        Result<Value, crate::runtime::value::RuntimeError>,
        Vec<u8>,
        String,
    ) {
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("vertical-slice.xsh", VERTICAL_SLICE);
        let parsed = Parser::parse_source_arena_only(source_id, VERTICAL_SLICE);
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources).with_tracing();
        assert!(
            evaluator
                .install_compact_lowered_program(&parsed.arena, source_id)
                .is_empty()
        );
        let result = evaluator
            .call_lowered_proc(name, &[], Span::new(source_id, 0, 0))
            .expect("oracle proc is lowered");
        (
            result,
            evaluator.stdout,
            normalize_traces(&evaluator.trace_events),
        )
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
        program: &FullProgram,
        name: Name,
    ) -> (
        Result<Value, crate::runtime::value::RuntimeError>,
        Vec<u8>,
        String,
    ) {
        let mut evaluator =
            Evaluator::new_with_sources(Vec::new(), (*program.sources).clone()).with_tracing();
        for (key, kind, body) in program.decode_functions().unwrap() {
            match (key, kind) {
                (LoweredFunctionKey::Name(name), LoweredFunctionKind::Pure) => {
                    Arc::make_mut(&mut evaluator.lowered_pures).insert(name, body);
                }
                (LoweredFunctionKey::Name(name), LoweredFunctionKind::Proc) => {
                    Arc::make_mut(&mut evaluator.lowered_procs).insert(name, body);
                }
                (LoweredFunctionKey::Qualified(name), LoweredFunctionKind::Pure) => {
                    Arc::make_mut(&mut evaluator.lowered_qualified_pures).insert(name, body);
                }
                (LoweredFunctionKey::Qualified(name), LoweredFunctionKind::Proc) => {
                    Arc::make_mut(&mut evaluator.lowered_qualified_procs).insert(name, body);
                }
            }
        }
        let result = evaluator
            .call_lowered_proc(name, &[], Span::new(program.store.source_id, 0, 0))
            .expect("full indexed proc is installed");
        (
            result,
            evaluator.stdout,
            normalize_traces(&evaluator.trace_events),
        )
    }

    fn run_driver_direct(
        program: &LoweredProgram,
        sources: SourceMap,
        source_id: SourceId,
    ) -> (Vec<u8>, Vec<u8>, String, Vec<String>) {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources).with_tracing();
        let mut outcomes = Vec::new();
        for statement in &program.statements {
            let Some(statement) = statement else {
                continue;
            };
            let result = evaluator.eval_lowered_top_level_stmt(
                statement,
                Span::new(source_id, 0, 0),
            );
            outcomes.push(normalize_process_ids(format!("{result:?}")));
        }
        (
            evaluator.stdout,
            evaluator.stderr,
            normalize_traces(&evaluator.trace_events),
            outcomes,
        )
    }

    fn normalize_process_ids(mut value: String) -> String {
        let marker = "pid: Some(";
        let mut search_start = 0;
        while let Some(relative) = value[search_start..].find(marker) {
            let start = search_start + relative + marker.len();
            let Some(relative_end) = value[start..].find(')') else {
                break;
            };
            let end = start + relative_end;
            value.replace_range(start..end, "<pid>");
            search_start = start + "<pid>".len() + 1;
        }
        value
    }

    #[test]
    fn decoded_full_program_matches_values_output_errors_and_traces() {
        run_with_large_stack(|| {
            let (sources, source_id, units) =
                fixture("vertical-slice.xsh", VERTICAL_SLICE);
            let program = FullBuilder::build(&units, sources, source_id).unwrap();

            for name in [Name::intern("main"), Name::intern("exact_error_site")] {
                assert_eq!(run_full(&program, name), run_original(name));
            }
        });
    }

    #[test]
    fn compact_entry_executes_after_all_frontend_and_adapter_scratch_is_dropped() {
        run_with_large_stack(|| {
            let program = {
                let mut sources = SourceMap::new();
                let source_id =
                    sources.add_file("vertical-slice.xsh", VERTICAL_SLICE);
                let parsed =
                    Parser::parse_source_arena_only(source_id, VERTICAL_SLICE);
                let declarations =
                    Checker::check_compact_declarations(&parsed.arena);
                let bodies =
                    Checker::probe_compact_bodies(&parsed.arena, &declarations);
                FullBuilder::build_compact(
                    &parsed.arena,
                    &declarations,
                    &bodies,
                    VERTICAL_SLICE,
                    Arc::new(sources),
                    source_id,
                )
                .unwrap()
            };

            assert_eq!(
                run_full(&program, Name::intern("main")),
                run_original(Name::intern("main"))
            );
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
        assert_eq!(
            size_of::<FullTag>() + size_of::<IrData>(),
            9,
            "full Phase 4 instructions use one-byte tags and eight-byte data"
        );
    }

    #[test]
    fn compact_driver_roundtrips_effects_and_executes_after_arena_drop() {
        run_with_large_stack(|| {
            let (program, original, plan, mut evaluator) = {
                let mut sources = SourceMap::new();
                let source_id = sources.add_file("phase5-boundary.xsh", PHASE5_BOUNDARY);
                let parsed =
                    Parser::parse_source_arena_only(source_id, PHASE5_BOUNDARY);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let declarations =
                    Checker::check_compact_declarations(&parsed.arena);
                assert!(
                    declarations.diagnostics.is_empty(),
                    "{:?}",
                    declarations.diagnostics
                );
                let bodies =
                    Checker::probe_compact_bodies(&parsed.arena, &declarations);
                assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
                let shared_sources = Arc::new(sources.clone());
                let program = FullBuilder::build_compact(
                    &parsed.arena,
                    &declarations,
                    &bodies,
                    PHASE5_BOUNDARY,
                    shared_sources,
                    source_id,
                )
                .unwrap();
                let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
                let plan = evaluator
                    .prepare_compact_lowered_only(&parsed.arena, source_id, true)
                    .expect("phase 5 boundary fixture is wholly lowerable");
                let original = (*evaluator.lowered_program).clone();
                (program, original, plan, evaluator)
            };

            let decoded = program.decode_driver().unwrap().unwrap();
            assert_eq!(format!("{decoded:?}"), format!("{original:?}"));
            assert_eq!(
                run_driver_direct(
                    &decoded,
                    (*program.sources).clone(),
                    program.store.source_id,
                ),
                run_driver_direct(
                    &original,
                    (*program.sources).clone(),
                    program.store.source_id,
                )
            );
            assert!(
                program.store.driver_steps.iter().any(|step| {
                    step.effects & (EFFECT_ENV | EFFECT_CWD | EFFECT_PROCESS) != 0
                })
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
                program.store.captures.iter().any(|capture| {
                    program.store.string(capture.name).ok() == Some("base")
                }),
                "top-level binding capture is stored by compact identity"
            );
            evaluator.lowered_program = Arc::new(decoded);
            for (key, kind, body) in program.decode_functions().unwrap() {
                match (key, kind) {
                    (LoweredFunctionKey::Name(name), LoweredFunctionKind::Pure) => {
                        Arc::make_mut(&mut evaluator.lowered_pures).insert(name, body);
                    }
                    (LoweredFunctionKey::Name(name), LoweredFunctionKind::Proc) => {
                        Arc::make_mut(&mut evaluator.lowered_procs).insert(name, body);
                    }
                    (LoweredFunctionKey::Qualified(name), LoweredFunctionKind::Pure) => {
                        Arc::make_mut(&mut evaluator.lowered_qualified_pures)
                            .insert(name, body);
                    }
                    (LoweredFunctionKey::Qualified(name), LoweredFunctionKind::Proc) => {
                        Arc::make_mut(&mut evaluator.lowered_qualified_procs)
                            .insert(name, body);
                    }
                }
            }
            let output = match evaluator.eval_installed_compact_lowered_only(plan) {
                Ok(output) => output,
                Err(_) => panic!("decoded driver remains executable"),
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
        let source_id = sources.add_file("phase5-boundary.xsh", PHASE5_BOUNDARY);
        let parsed = Parser::parse_source_arena_only(source_id, PHASE5_BOUNDARY);
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        let program = FullBuilder::build_compact(
            &parsed.arena,
            &declarations,
            &bodies,
            PHASE5_BOUNDARY,
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
    fn driver_propagated_process_failure_preserves_error_location_and_trace() {
        let source = "run false ?\n";
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("phase5-propagate.xsh", source);
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
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources.clone());
        evaluator
            .prepare_compact_lowered_only(&parsed.arena, source_id, true)
            .expect("propagating process statement lowers");
        let original = (*evaluator.lowered_program).clone();
        let decoded = program.decode_driver().unwrap().unwrap();
        assert_eq!(
            run_driver_direct(&decoded, sources.clone(), source_id),
            run_driver_direct(&original, sources, source_id)
        );
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
        let source_id = sources.add_file("phase5-reject.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        let statements = parsed.arena.statement_ids().collect::<Vec<_>>();
        let lowered = LoweredProgram {
            statements: vec![None],
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
        let (sources, source_id, units) = fixture("vertical-slice.xsh", VERTICAL_SLICE);
        let mut program = FullBuilder::build(&units, sources, source_id).unwrap();
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
        let (sources, source_id, units) = fixture("vertical-slice.xsh", VERTICAL_SLICE);
        let program = FullBuilder::build(&units, sources, source_id).unwrap();

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
            .expect("vertical slice has a single-return function");
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
        let (sources, source_id, units) = fixture("vertical-slice.xsh", VERTICAL_SLICE);
        let program = FullBuilder::build(&units, sources, source_id).unwrap();

        let mut bad_slot = program.clone();
        let slot = bad_slot
            .store
            .tags
            .iter()
            .position(|tag| *tag == FullTag::ExprParam)
            .unwrap();
        let slot_payload = bad_slot.store.data[slot].range().bounds(bad_slot.store.extra.len()).unwrap();
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
            .position(|tag| *tag == FullTag::ExprCall)
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
        let root_location = builder
            .intern_location(Span::new(root_id, 0, 3))
            .unwrap();
        let module_location = builder
            .intern_location(Span::new(module_id, 0, 5))
            .unwrap();
        assert_ne!(root_location.raw(), module_location.raw());
        assert_eq!(
            builder.store.location_sources,
            vec![root_id, module_id]
        );

        let program = FullProgram {
            store: builder.store,
            sources: Arc::new(sources),
        };
        FullVerifier::verify(&program).unwrap();

        let mut bad_source = program;
        bad_source.store.location_sources[1] = SourceId::new(2);
        assert!(FullVerifier::verify(&bad_source).is_err());
    }

    #[test]
    fn compact_driver_roundtrips_loaded_module_programs() {
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
        let mut oracle =
            Evaluator::new_with_sources(Vec::new(), sources.clone());
        assert!(
            oracle
                .install_compact_lowered_program(&parsed.arena, source_id)
                .is_empty()
        );
        let original = (*oracle.lowered_program).clone();
        let mut incomplete = original.clone();
        let Some(LoweredTopLevelStmt {
            kind: LoweredTopLevelKind::Use {
                module_statements, ..
            },
            ..
        }) = incomplete.statements[0].as_mut()
        else {
            panic!("loaded user module should lower to a use driver step");
        };
        module_statements.clear();
        let source_statements = parsed.arena.statement_ids().collect::<Vec<_>>();
        let error = FullBuilder::build_with_driver(
            &[],
            Some((&incomplete, &source_statements, &parsed.arena)),
            Arc::new(sources.clone()),
            source_id,
        )
        .unwrap_err();
        assert_eq!(error.construct, "module_top_level_boundary_blocker");

        let program = FullBuilder::build_compact(
            &parsed.arena,
            &declarations,
            &bodies,
            &source,
            Arc::new(sources),
            source_id,
        )
        .unwrap();
        let decoded = program.decode_driver().unwrap().unwrap();
        assert_eq!(format!("{decoded:?}"), format!("{original:?}"));
        let module_source = parsed
            .arena
            .module_statements(&parsed.arena.modules[0])
            .next()
            .map(|statement| parsed.arena.arena.stmt(statement).span.source_id)
            .unwrap();
        assert!(
            program
                .store
                .location_sources
                .contains(&module_source)
        );
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

        let mut evaluator = Evaluator::new_with_sources(
            Vec::new(),
            (*program.sources).clone(),
        );
        for (key, kind, body) in program.decode_functions().unwrap() {
            match (key, kind) {
                (LoweredFunctionKey::Name(name), LoweredFunctionKind::Pure) => {
                    Arc::make_mut(&mut evaluator.lowered_pures).insert(name, body);
                }
                (LoweredFunctionKey::Name(name), LoweredFunctionKind::Proc) => {
                    Arc::make_mut(&mut evaluator.lowered_procs).insert(name, body);
                }
                (LoweredFunctionKey::Qualified(name), LoweredFunctionKind::Pure) => {
                    Arc::make_mut(&mut evaluator.lowered_qualified_pures)
                        .insert(name, body);
                }
                (LoweredFunctionKey::Qualified(name), LoweredFunctionKind::Proc) => {
                    Arc::make_mut(&mut evaluator.lowered_qualified_procs)
                        .insert(name, body);
                }
            }
        }
        for statement in decoded.statements.iter().flatten() {
            evaluator
                .eval_lowered_top_level_stmt(statement, Span::new(source_id, 0, 0))
                .unwrap();
        }
        assert_eq!(evaluator.stdout, b"loaded\n");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn failed_full_encoding_rewinds_all_graph_columns() {
        let (sources, source_id, units) = fixture("vertical-slice.xsh", VERTICAL_SLICE);
        let mut ordered = units.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|unit| (unit.source_span().start(), unit.key().display_name()));
        let mut builder = FullBuilder::new(source_id);
        builder.predeclare(&ordered).unwrap();
        let function = builder.function_ids[&ordered[0].key()];
        let mut body = (*ordered[0].lowered_body().unwrap()).clone();
        body.body.push(LoweredStmt::Expr {
            value: LoweredExpr::Param(usize::MAX),
            span: ordered[0].source_span(),
        });
        let checkpoint = builder.checkpoint();
        builder.current_owner = Some(function.raw());
        builder.current_slot_count = body.slot_count as u32;
        let result = builder.encode_body(function, &body);
        builder.current_owner = None;
        builder.current_slot_count = 0;
        assert!(result.is_err());
        builder.rewind(checkpoint);
        assert_eq!(builder.checkpoint(), checkpoint);
        drop(sources);
    }

    #[test]
    #[ignore = "Phase 5 evidence compares complete-program admission and compact driver strategies"]
    fn corpus_phase5_boundary_strategy_evidence() {
        run_with_large_stack(|| {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            let mut paths = Vec::new();
            for relative in crate::frontend_stats::DEFAULT_ROOTS {
                super::super::tests::collect_xsh_paths(&root.join(relative), &mut paths);
            }
            paths.sort();
            paths.dedup();

            let mut files = 0usize;
            let mut source_bytes = 0usize;
            let mut top_level_statements = 0usize;
            let mut lowerable_top_level_statements = 0usize;
            let mut whole_program_files = 0usize;
            let mut whole_program_statements = 0usize;
            let mut driver_steps = 0usize;
            let mut driver_regions = 0usize;
            let mut driver_sync_rows = 0usize;
            let mut driver_metadata_bytes = 0usize;
            let mut full_retained_bytes = 0usize;
            let mut arena_duplication_bytes = 0usize;
            let mut blockers = BTreeMap::<String, usize>::new();
            let mut effects = BTreeMap::<&'static str, usize>::new();

            for path in paths {
                let Ok(source) = super::super::tests::read_xsh_source(&path) else {
                    continue;
                };
                let mut sources = SourceMap::new();
                let source_id = sources.add_file(path.to_string_lossy(), source.clone());
                let parsed = Parser::parse_source_arena_only(source_id, &source);
                if !parsed.diagnostics.is_empty() {
                    continue;
                }
                let declarations = Checker::check_compact_declarations(&parsed.arena);
                if !declarations.diagnostics.is_empty() {
                    continue;
                }
                let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
                if !bodies.diagnostics.is_empty() {
                    continue;
                }
                let probe = super::super::super::probe_compact_lower_constructed_bodies(
                    &parsed.arena,
                    &declarations,
                    &bodies,
                    &source,
                );
                let units = super::super::super::probe_compact_lower_function_units(
                    &parsed.arena,
                    &declarations,
                    &bodies,
                    &source,
                );
                files += 1;
                source_bytes += source.len();
                top_level_statements += probe.top_level_statements;
                lowerable_top_level_statements += probe.constructed_top_level_statements;
                if units.iter().any(|unit| !unit.is_lowered()) {
                    *blockers.entry("function".to_string()).or_default() += 1;
                    continue;
                }
                let root_statement_count = parsed.arena.statement_ids().count();
                let built = FullBuilder::build_compact(
                    &parsed.arena,
                    &declarations,
                    &bodies,
                    &source,
                    Arc::new(sources),
                    source_id,
                );
                let program = match built {
                    Ok(program) => program,
                    Err(error) => {
                        *blockers.entry(error.construct.to_string()).or_default() += 1;
                        continue;
                    }
                };
                whole_program_files += 1;
                whole_program_statements += root_statement_count;
                driver_steps += program.store.driver_steps.len();
                driver_regions += program.store.driver_regions.len();
                driver_sync_rows += program.store.driver_sync.len();
                driver_metadata_bytes += program.store.driver_retained_bytes();
                full_retained_bytes += program.store.retained_bytes();
                arena_duplication_bytes += parsed.arena.retained_bytes();
                for step in &program.store.driver_steps {
                    for (name, flag) in [
                        ("import", EFFECT_IMPORT),
                        ("cwd", EFFECT_CWD),
                        ("env", EFFECT_ENV),
                        ("process", EFFECT_PROCESS),
                        ("signal", EFFECT_SIGNAL),
                        ("cancellation", EFFECT_CANCELLATION),
                        ("trace", EFFECT_TRACE),
                        ("dynamic", EFFECT_DYNAMIC_CALL),
                        ("defer", EFFECT_DEFER),
                        ("propagate", EFFECT_PROPAGATE),
                        ("host", EFFECT_HOST),
                        ("binding_read", EFFECT_BINDING_READ),
                        ("binding_write", EFFECT_BINDING_WRITE),
                    ] {
                        if step.effects & flag != 0 {
                            *effects.entry(name).or_default() += 1;
                        }
                    }
                }
            }

            assert!(files > 0);
            assert!(whole_program_files > 0);
            assert!(driver_steps > 0);
            assert!(driver_regions > 0);
            assert!(full_retained_bytes > driver_metadata_bytes);
            println!(
                "phase5 corpus files={files} source_bytes={source_bytes} top_level_statements={top_level_statements} lowerable_top_level_statements={lowerable_top_level_statements} region_statement_coverage_percent={:.2} whole_program_files={whole_program_files} whole_program_file_coverage_percent={:.2} whole_program_statements={whole_program_statements} driver_steps={driver_steps} coherent_regions={driver_regions} driver_sync_rows={driver_sync_rows} driver_metadata_bytes={driver_metadata_bytes} full_retained_bytes={full_retained_bytes} avoided_arena_duplication_bytes={arena_duplication_bytes}",
                100.0 * lowerable_top_level_statements as f64 / top_level_statements as f64,
                100.0 * whole_program_files as f64 / files as f64,
            );
            println!(
                "phase5 strategy=whole_program admitted_files={whole_program_files} regions={whole_program_files} silent_internal_fallback=0"
            );
            println!(
                "phase5 strategy=coherent_regions admitted_files={whole_program_files} regions={driver_regions} sync_rows={driver_sync_rows} metadata_bytes={driver_metadata_bytes} silent_internal_fallback=0 selected=true"
            );
            println!(
                "phase5 strategy=arena_orchestration admitted_files={files} retained_arena_bytes={arena_duplication_bytes} general_ast_required=true selected=false"
            );
            for (effect, count) in effects {
                println!("phase5 effect name={effect} steps={count}");
            }
            for (blocker, count) in blockers {
                println!("phase5 blocker label={blocker} files={count}");
            }
        });
    }

    #[test]
    #[ignore = "Phase 4 evidence scans every fully lowerable function corpus file"]
    fn corpus_full_indexed_store_evidence() {
        run_with_large_stack(|| {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            let mut paths = Vec::new();
            for relative in crate::frontend_stats::DEFAULT_ROOTS {
                super::super::tests::collect_xsh_paths(
                    &root.join(relative),
                    &mut paths,
                );
            }
            paths.sort();
            paths.dedup();

            let mut files = 0usize;
            let mut executable_files = 0usize;
            let mut source_bytes = 0usize;
            let mut functions_seen = 0usize;
            let mut functions_built = 0usize;
            let mut instructions = 0usize;
            let mut blocks = 0usize;
            let mut patterns = 0usize;
            let mut stages = 0usize;
            let mut extra_words = 0usize;
            let mut retained_bytes = 0usize;
            let mut recursive_row_lower_bound = 0usize;
            let mut blockers = BTreeMap::<String, usize>::new();
            let mut frequencies = BTreeMap::<String, usize>::new();

            for path in paths {
                let Ok(source) = super::super::tests::read_xsh_source(&path) else {
                    continue;
                };
                let mut sources = SourceMap::new();
                let source_id = sources.add_file(path.to_string_lossy(), source.clone());
                let parsed = Parser::parse_source_arena_only(source_id, &source);
                if !parsed.diagnostics.is_empty() {
                    continue;
                }
                let declarations = Checker::check_compact_declarations(&parsed.arena);
                if !declarations.diagnostics.is_empty() {
                    continue;
                }
                let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
                if !bodies.diagnostics.is_empty() {
                    continue;
                }
                let units = super::super::super::probe_compact_lower_function_units(
                    &parsed.arena,
                    &declarations,
                    &bodies,
                    &source,
                );
                files += 1;
                source_bytes += source.len();
                functions_seen += units.len();
                for unit in &units {
                    if let Some(blocker) = unit.blocker() {
                        *blockers.entry(blocker.label().to_string()).or_default() += 1;
                    }
                }
                if units.is_empty() || units.iter().any(|unit| !unit.is_lowered()) {
                    continue;
                }

                let program =
                    FullBuilder::build(&units, Arc::new(sources), source_id).unwrap_or_else(
                        |error| {
                            let recovery_functions = units
                                .iter()
                                .filter(|unit| {
                                    unit.lowered_body()
                                        .is_some_and(|body| format!("{body:?}").contains("Unknown"))
                                })
                                .map(|unit| unit.key().display_name())
                                .collect::<Vec<_>>();
                            panic!(
                                "{}: full indexed construction failed: {error:?}; recovery functions: {recovery_functions:?}",
                                path.display()
                            )
                        },
                    );
                let decoded = program.decode_functions().unwrap();
                assert_eq!(decoded.len(), units.len());
                for (key, kind, body) in decoded {
                    let unit = units.iter().find(|unit| unit.key() == key).unwrap();
                    assert_eq!(kind, unit.kind());
                    assert_eq!(body.slot_count, unit.lowered_body().unwrap().slot_count);
                    assert_eq!(body.params, unit.lowered_body().unwrap().params);
                    assert_eq!(body.body.len(), unit.lowered_body().unwrap().body.len());
                }
                executable_files += 1;
                functions_built += units.len();
                instructions += program.store.tags.len();
                blocks += program.store.blocks.len();
                patterns += program.store.patterns.len();
                stages += program.store.stages.len();
                extra_words += program.store.extra.len();
                retained_bytes += program.store.retained_bytes();
                for tag in &program.store.tags {
                    recursive_row_lower_bound += if (*tag as u8)
                        <= FullTag::IntStrByteAtSlot as u8
                    {
                        size_of::<LoweredIntExpr>()
                    } else if (*tag as u8) <= FullTag::BoolLiteralCompareSlot as u8 {
                        size_of::<LoweredBoolExpr>()
                    } else if (*tag as u8) <= FullTag::ExprSelfCall as u8 {
                        size_of::<LoweredExpr>()
                    } else {
                        size_of::<LoweredStmt>()
                    };
                    *frequencies.entry(format!("{tag:?}")).or_default() += 1;
                }
                recursive_row_lower_bound +=
                    program.store.patterns.len() * size_of::<LoweredPattern>()
                        + program.store.stages.len() * size_of::<LoweredPipelineStage>()
                        + program.store.values.len() * size_of::<LoweredValue>()
                        + program.store.functions.len() * size_of::<LoweredPureFunction>();
            }

            assert!(files > 0);
            assert!(executable_files > 0);
            assert!(functions_built > 0);
            assert!(instructions > 0);
            assert!(retained_bytes * 10 < recursive_row_lower_bound * 8);
            println!(
                "phase4 corpus files={files} executable_files={executable_files} source_bytes={source_bytes} functions_seen={functions_seen} functions_built={functions_built} instructions={instructions} blocks={blocks} patterns={patterns} stages={stages} extra_words={extra_words} retained_bytes={retained_bytes} recursive_row_lower_bound={recursive_row_lower_bound} reduction_percent={:.2} retained_bytes_per_instruction={:.3} extra_bytes_per_instruction={:.3}",
                100.0 * (recursive_row_lower_bound - retained_bytes) as f64
                    / recursive_row_lower_bound as f64,
                retained_bytes as f64 / instructions as f64,
                extra_words as f64 * size_of::<u32>() as f64 / instructions as f64,
            );
            for (tag, count) in frequencies {
                println!("phase4 opcode tag={tag} count={count}");
            }
            for (blocker, count) in blockers {
                println!("phase4 blocker label={blocker} count={count}");
            }
        });
    }
}
