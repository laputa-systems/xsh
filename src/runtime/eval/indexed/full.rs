pub(in crate::runtime::eval) mod decode;
pub(in crate::runtime::eval) mod in_place;
pub(in crate::runtime::eval) mod schema;

use decode::{BorrowCodec, DecodeContext, ErrorExprView, ListView, StageRef};
#[cfg(test)]
use decode::PatternId;
pub(in crate::runtime::eval) use schema::{BuildPatternRow, FullDriverTag};
pub(in crate::runtime::eval) use schema::{
    BuildBoolRow, BuildExprRow, BuildIntRow, BuildStmtRow, FrameStrategy, FullTag,
};

use super::semantic::{SemanticPoolBuilder, SemanticPools};
use super::{
    IR_NONE, IrBlockId, IrBuildError, IrData, IrFunctionId, IrLocation, IrLocationId, IrRange,
    IrStringId, IrVerifyError, SignatureId, TypeId,
};
use crate::modules::RuntimeOp;
use crate::modules::hash::HashAlgorithm;
use crate::runtime::eval::{
    BuildBoolId, BuildExprId, BuildIntId, BuildPatternId,
    BuildPatternIdSlots, BuildScratch, BuildStmtId, BuildTopKind,
    BuildTopStmtId, BuildTopStmtRow, FunctionBuild, FunctionHeader, LoweredAssignPath,
    LoweredAssignStep, LoweredCallArg, LoweredCompQualifier, LoweredCompQualifiers,
    CheckedPatternField, LoweredCompTarget, LoweredErrorExpr, LoweredFmtPart,
    LoweredFunctionKey, LoweredFunctionKind, LoweredFunctionUnit, LoweredModuleExport,
    LoweredModuleExportKind, LoweredProcessCommandArgv,
    LoweredProcessCommandBuilderEntry, LoweredRecordEntry, LoweredRecordUpdates, LoweredReturnKind,
    LoweredRunArg, LoweredRunArgKind, LoweredRunCapture, LoweredRunEnv, LoweredRunPipelineSegment,
    LoweredRunRedirection, LoweredSpawnRun, LoweredStatsValue, LoweredStrPredicate,
    LoweredTagValue, LoweredTopLevelSlot, LoweredTopLevelSlots, LoweredType, LoweredTypeCheck,
    LoweredValue, PreparedConstantValue, ProgramBuild, ReduceByOp, ScanBytes, ScanCheck,
    ScanCondition, CheckedPatternVariantKey, CheckedPatternVariantKind,
};
use crate::runtime::value::{DurationValue, FloatValue, FunctionName, PathValue, RegexValue};
use crate::sema::check::CompactDeclOutput;
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

fn is_resource_transfer_instruction(tag: FullTag) -> bool {
    matches!(
        tag,
        FullTag::StmtReturn
            | FullTag::StmtValue
            | FullTag::StmtBreakValue
            | FullTag::StmtAssign
            | FullTag::StmtAssignField
            | FullTag::StmtAssignPath
            | FullTag::StmtYield
            | FullTag::StmtYieldDelegate
    )
}

const DRIVER_OWNER_BIT: u32 = 1 << 31;
const DRIVER_SLOT_READ: u8 = 1;
const DRIVER_SLOT_WRITE: u8 = 1 << 1;
const DRIVER_SLOT_MUTABLE: u8 = 1 << 2;
const DRIVER_SLOT_TEMPORARY: u8 = 1 << 3;

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

pub(in crate::runtime::eval) use schema::FullPatternTag;

macro_rules! value_codec_verify_field {
    ($decoder:ident, $payload:ident; $field_ty:ty) => {
        <$field_ty as FullCodec>::verify($decoder, &mut $payload)?;
    };
    ($decoder:ident, $payload:ident; $field_ty:ty => $verify_ty:ty) => {
        <$verify_ty as FullCodec>::verify($decoder, &mut $payload)?;
    };
}

/// Keeps persisted value tags and their wire field order in one declaration.
macro_rules! lowered_value_schema {
    (
        context { $builder:ident, $decoder:ident, $payload:ident }
        rows {
            $(
                $tag:ident = $ordinal:literal {
                    fields { $($field:ident: $field_ty:ty $(=> $verify_ty:ty)?),* $(,)? }
                    encode { $pattern:pat => { $($prepare:tt)* } }
                    decode $construct:expr
                }
            )*
        }
        aliases {
            $(
                $alias:pat => $alias_tag:ident {
                    fields { $($alias_field:expr),* $(,)? }
                }
            )*
        }
        custom {
            $(
                $custom_tag:ident = $custom_ordinal:literal {
                    encode { $($custom_pattern:pat => $encode:block),+ $(,)? }
                    decode $decode:block
                    verify $verify:block
                }
            )*
        }
        reject { $($rejected:pat => $reject:block),* $(,)? }
    ) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[repr(u8)]
        pub(in crate::runtime::eval) enum FullValueTag {
            $($tag = $ordinal,)*
            $($custom_tag = $custom_ordinal,)*
        }

        impl FullCodec for LoweredValue {
            fn encode(
                &self,
                $builder: &mut FullBuilder,
                output: &mut Vec<u32>,
            ) -> Result<(), IrBuildError> {
                let mut $payload = $builder.take_payload();
                let tag = match self {
                    $(
                        $pattern => {
                            $($prepare)*
                            $($field.encode($builder, &mut $payload)?;)*
                            FullValueTag::$tag
                        },
                    )*
                    $(
                        $alias => {
                            $($alias_field.encode($builder, &mut $payload)?;)*
                            FullValueTag::$alias_tag
                        },
                    )*
                    $(
                        $(
                            $custom_pattern => {
                                $encode
                                FullValueTag::$custom_tag
                            },
                        )+
                    )*
                    $($rejected => $reject,)*
                };
                let value = $builder.push_value(tag, &$payload);
                $builder.recycle_payload($payload);
                output.push(value?);
                Ok(())
            }

            fn decode(
                $decoder: &FullDecoder<'_>,
                input: &mut FullCursor<'_>,
            ) -> Result<Self, IrVerifyError> {
                let index = input.raw()? as usize;
                let tag = $decoder
                    .store
                    .values
                    .get(index)
                    .copied()
                    .ok_or_else(|| IrVerifyError::new("literal value id is out of bounds"))?;
                let data = $decoder.store.value_data[index];
                let mut $payload = FullCursor::new($decoder.store.payload(data.range())?);
                let value = match tag {
                    $(
                        FullValueTag::$tag => {
                            $(let $field = <$field_ty as FullCodec>::decode($decoder, &mut $payload)?;)*
                            $construct
                        },
                    )*
                    $(FullValueTag::$custom_tag => $decode,)*
                };
                $payload.finish()?;
                Ok(value)
            }

            fn verify(
                $decoder: &FullDecoder<'_>,
                input: &mut FullCursor<'_>,
            ) -> Result<(), IrVerifyError> {
                let index = input.raw()? as usize;
                let tag = $decoder
                    .store
                    .values
                    .get(index)
                    .copied()
                    .ok_or_else(|| IrVerifyError::new("literal value id is out of bounds"))?;
                let data = $decoder.store.value_data[index];
                let mut $payload = FullCursor::new($decoder.store.payload(data.range())?);
                match tag {
                    $(
                        FullValueTag::$tag => {
                            $(value_codec_verify_field!($decoder, $payload; $field_ty $(=> $verify_ty)?);)*
                        },
                    )*
                    $(FullValueTag::$custom_tag => $verify,)*
                }
                $payload.finish()
            }
        }
    };
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
    /// Checker module type retained for verifier provenance, or `IR_NONE`.
    registered_module_type: u32,
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
    /// Dense checked type ids for local slots; `IR_NONE` means no unique fact exists.
    slot_type_facts: IrRange,
    /// Exact checker return type for deriving call results; independent of runtime return checks.
    checked_return_type: u32,
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
struct FullCheckedExprType {
    instruction: u32,
    type_id: TypeId,
    function_ref: Option<(FunctionName, bool)>,
    with_error_source: bool,
}

#[derive(Clone, Copy)]
enum CallableAssignmentPathStep {
    Field(Name),
    Index,
}

#[derive(Clone, Copy)]
enum FunctionSignatureLookup {
    Missing,
    Unique(SignatureId),
    Ambiguous,
}

#[derive(Default)]
struct FunctionIdentitySignatures {
    pure: FunctionSignatureLookup,
    proc: FunctionSignatureLookup,
}

enum CheckedRecordEntry {
    Field(Name, usize),
    Spread(usize),
}

impl Default for FunctionSignatureLookup {
    fn default() -> Self {
        Self::Missing
    }
}

/// Shares bounded target lookup between checked references and module exports.
struct FunctionSignatureIndex {
    functions: FxHashMap<(Option<Name>, Name), FunctionIdentitySignatures>,
}

#[derive(Clone, Copy)]
enum RegisteredModuleType {
    Missing,
    Present(TypeId),
    Ambiguous,
}

impl FunctionSignatureIndex {
    fn build(store: &FullStore) -> Result<Self, IrVerifyError> {
        let mut functions: FxHashMap<(Option<Name>, Name), FunctionIdentitySignatures> =
            FxHashMap::default();
        for (index, function) in store.functions.iter().enumerate() {
            let metadata = store
                .function_metadata
                .get(index)
                .ok_or_else(|| IrVerifyError::new("function reference metadata is missing"))?;
            let name = Name::intern(store.string(function.name)?);
            let owner = if metadata.owner == IR_NONE {
                None
            } else {
                Some(Name::intern(store.string(metadata.owner)?))
            };
            let entry = functions.entry((owner, name)).or_default();
            let lookup = if metadata.flags & 1 == 0 {
                &mut entry.pure
            } else {
                &mut entry.proc
            };
            *lookup = match *lookup {
                FunctionSignatureLookup::Missing => {
                    FunctionSignatureLookup::Unique(function.signature)
                }
                FunctionSignatureLookup::Unique(_) | FunctionSignatureLookup::Ambiguous => {
                    FunctionSignatureLookup::Ambiguous
                }
            };
        }
        Ok(Self { functions })
    }

    fn module_export(
        &self,
        owner: Name,
        name: Name,
        pure: bool,
    ) -> Result<SignatureId, IrVerifyError> {
        let lookup = self
            .functions
            .get(&(Some(owner), name))
            .map(|signatures| if pure { &signatures.pure } else { &signatures.proc });
        match lookup {
            Some(FunctionSignatureLookup::Unique(signature)) => Ok(*signature),
            Some(FunctionSignatureLookup::Ambiguous) => Err(IrVerifyError::new(
                "registered module function target is ambiguous",
            )),
            Some(FunctionSignatureLookup::Missing) | None => Err(IrVerifyError::new(
                "registered module function target is missing",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
struct FullFunctionMetadata {
    owner: u32,
    flags: u8,
    reserved: [u8; 3],
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FullPatternFieldType {
    name: Option<Name>,
    type_id: TypeId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FullPatternVariantSchema {
    fields: Vec<FullPatternFieldType>,
}

#[derive(Clone, Debug)]
struct FullStore {
    source_id: SourceId,
    tags: Vec<FullTag>,
    /// Private true marks come only from checked facts emitted by the builder.
    /// They stay beside the instruction stream and never enter encoded payloads.
    resource_transfers: Vec<bool>,
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
    slot_type_facts: Vec<u32>,
    /// Sparse checker types for typed callables, structured errors, and error sources.
    checked_expr_types: Vec<FullCheckedExprType>,
    /// Exact nominal payload fields attached to the checked pattern producers.
    checked_pattern_variants:
        BTreeMap<CheckedPatternVariantKey, FullPatternVariantSchema>,
    /// Independently projected declaration fields used to authenticate each
    /// checked pattern payload schema.
    declared_pattern_variants:
        BTreeMap<CheckedPatternVariantKey, FullPatternVariantSchema>,
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
    prepared_module_call_plans: Vec<Arc<crate::runtime::eval::ModuleCallPlan>>,
    locations: Vec<IrLocation>,
    location_sources: Vec<SourceId>,
    runtime_ops: Vec<RuntimeOp>,
    assign_ops: Vec<AssignOp>,
    binary_ops: Vec<BinaryOp>,
    run_kinds: Vec<RunKind>,
    redirection_kinds: Vec<RedirectionKind>,
    semantic: SemanticPools,
    /// Derived from verified operand graphs so runtime dispatch needs only a table lookup.
    in_place: std::sync::OnceLock<Vec<bool>>,
}

impl Default for FullStore {
    fn default() -> Self {
        Self {
            source_id: SourceId::new(0),
            tags: Vec::new(),
            resource_transfers: Vec::new(),
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
            slot_type_facts: Vec::new(),
            checked_expr_types: Vec::new(),
            checked_pattern_variants: BTreeMap::new(),
            declared_pattern_variants: BTreeMap::new(),
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
            prepared_module_call_plans: Vec::new(),
            locations: Vec::new(),
            location_sources: Vec::new(),
            runtime_ops: Vec::new(),
            assign_ops: Vec::new(),
            binary_ops: Vec::new(),
            run_kinds: Vec::new(),
            redirection_kinds: Vec::new(),
            semantic: SemanticPools::default(),
            in_place: std::sync::OnceLock::new(),
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
            + self.resource_transfers.capacity() * size_of::<bool>()
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
            + self.slot_type_facts.capacity() * size_of::<u32>()
            + self.checked_expr_types.capacity() * size_of::<FullCheckedExprType>()
            + self
                .checked_pattern_variants
                .values()
                .map(|schema| {
                    size_of::<(CheckedPatternVariantKey, FullPatternVariantSchema)>()
                        + schema.fields.capacity() * size_of::<FullPatternFieldType>()
                })
                .sum::<usize>()
            + self
                .declared_pattern_variants
                .values()
                .map(|schema| {
                    size_of::<(CheckedPatternVariantKey, FullPatternVariantSchema)>()
                        + schema.fields.capacity() * size_of::<FullPatternFieldType>()
                })
                .sum::<usize>()
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
            + self.prepared_schemas.capacity()
                * size_of::<Arc<super::super::require::PreparedSchema>>()
            + self.wire_enums.capacity()
                * size_of::<Arc<crate::sema::wire_enums::WireEnumMapping>>()
            + self.prepared_module_call_plans.capacity()
                * size_of::<Arc<crate::runtime::eval::ModuleCallPlan>>()
            + self.prepared_regexes.capacity() * size_of::<RegexValue>()
            + self
                .prepared_regexes
                .iter()
                .map(|value| value.pattern.capacity())
                .sum::<usize>()
            + self.locations.capacity() * size_of::<IrLocation>()
            + self.location_sources.capacity() * size_of::<SourceId>()
            + self.runtime_ops.capacity() * size_of::<RuntimeOp>()
            + self.assign_ops.capacity() * size_of::<AssignOp>()
            + self.binary_ops.capacity() * size_of::<BinaryOp>()
            + self.run_kinds.capacity() * size_of::<RunKind>()
            + self.redirection_kinds.capacity() * size_of::<RedirectionKind>()
            + self
                .in_place
                .get()
                .map_or(0, |marks| marks.capacity() * size_of::<bool>())
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
        self.resource_transfers.shrink_to_fit();
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
        self.slot_type_facts.shrink_to_fit();
        self.checked_expr_types.shrink_to_fit();
        for schema in self.checked_pattern_variants.values_mut() {
            schema.fields.shrink_to_fit();
        }
        for schema in self.declared_pattern_variants.values_mut() {
            schema.fields.shrink_to_fit();
        }
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
        self.prepared_module_call_plans.shrink_to_fit();
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

    #[cfg(test)]
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

    #[cfg(test)]
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
        let identities = self
            .identities
            .get_or_init(|| self.resolve_identities().ok());
        if let Some(identities) = identities.as_ref() {
            return Ok(identities.get(&(key, kind)).map(|index| FullFunctionView {
                program: self,
                index: *index,
            }));
        }
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

    /// Whether a deferred driver step is an `errdefer`, which runs only when
    /// the script or module leaves its top level with an error.
    ///
    /// A deferred step's payload is its action, this flag, and its span; the
    /// verifier has checked that the flag is a boolean.
    pub(in crate::runtime::eval) fn driver_step_defers_on_error(
        &self,
        index: usize,
    ) -> Result<bool, IrVerifyError> {
        let steps = self.driver_root_steps()?;
        let step = steps
            .start
            .checked_add(index)
            .filter(|step| *step < steps.end)
            .ok_or_else(|| IrVerifyError::new("driver root step is out of bounds"))?;
        let view = self.driver_step_view_absolute(step)?;
        if view.tag() != FullDriverTag::Defer {
            return Err(IrVerifyError::new("driver step is not a deferred action"));
        }
        let execution = view.execution()?;
        let payload = view.payload()?;
        match view.decode_payload(&execution, payload)? {
            schema::DriverPayloadView::Defer { on_error, .. } => Ok(on_error),
            _ => Err(IrVerifyError::new("driver step is not a deferred action")),
        }
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

    fn verify_driver(
        &self,
        owned_blocks: &FxHashMap<u32, usize>,
        in_place_marks: &mut [bool],
        function_signature_index: Arc<FunctionSignatureIndex>,
    ) -> Result<(), IrVerifyError> {
        if self.store.driver_root == IR_NONE {
            return Ok(());
        }
        let mut program_states = vec![0; self.store.driver_programs.len()];
        let mut step_states = vec![0; self.store.driver_steps.len()];
        self.verify_driver_program(
            self.store.driver_root,
            &mut program_states,
            &mut step_states,
            owned_blocks,
            in_place_marks,
            &function_signature_index,
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
        owned_blocks: &FxHashMap<u32, usize>,
        in_place_marks: &mut [bool],
        function_signature_index: &Arc<FunctionSignatureIndex>,
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
        let mut registered_module_types = FxHashMap::default();
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
            let registered_import = self.verify_driver_step(
                step_index,
                program_states,
                step_states,
                owned_blocks,
                in_place_marks,
                function_signature_index,
                &registered_module_types,
            )?;
            if let Some((namespace, module_type)) = registered_import {
                use std::collections::hash_map::Entry;
                match registered_module_types.entry(namespace) {
                    Entry::Vacant(entry) => {
                        entry.insert(module_type.map_or(
                            RegisteredModuleType::Missing,
                            RegisteredModuleType::Present,
                        ));
                    }
                    Entry::Occupied(mut entry) => match *entry.get() {
                        RegisteredModuleType::Missing | RegisteredModuleType::Ambiguous => {}
                        RegisteredModuleType::Present(_) => {
                            *entry.get_mut() = RegisteredModuleType::Ambiguous;
                        }
                    },
                }
            }
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
        owned_blocks: &FxHashMap<u32, usize>,
        in_place_marks: &mut [bool],
        function_signature_index: &Arc<FunctionSignatureIndex>,
        registered_module_types: &FxHashMap<Name, RegisteredModuleType>,
    ) -> Result<Option<(Name, Option<TypeId>)>, IrVerifyError> {
        let step = self.store.driver_steps[step_index];
        if step.tag != FullDriverTag::Use && step.registered_module_type != IR_NONE {
            return Err(IrVerifyError::new(
                "non-import driver step has a registered module type",
            ));
        }
        let instruction_range = self.store.driver_instruction_range(step_index)?;
        let owner =
            driver_owner(step_index).map_err(|_| IrVerifyError::new("driver owner is invalid"))?;
        let decoder = FullDecoder {
            store: &self.store,
            owner,
            instruction_states: Some(RefCell::new(vec![0; instruction_range.len()])),
            instruction_proof: Some(RefCell::new(in_place::InstructionProof::new(
                instruction_range.clone(),
            ))),
            instruction_range,
            block_states: Some(OwnedBlockStates::new(owner, owned_blocks)),
            slot_count: step.slot_count,
            pattern_ceiling: Cell::new(usize::MAX),
            local_immutable_let_initializers: RefCell::default(),
            function_signature_index: Some(function_signature_index.clone()),
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
            let temporary = slot.flags & DRIVER_SLOT_TEMPORARY != 0;
            if slot.flags
                & !(DRIVER_SLOT_READ
                    | DRIVER_SLOT_WRITE
                    | DRIVER_SLOT_MUTABLE
                    | DRIVER_SLOT_TEMPORARY)
                != 0
                || slot.flags & DRIVER_SLOT_READ == 0
                || slot.slot >= step.slot_count
                || (temporary
                    && (slot.name != IR_NONE
                        || slot.flags & (DRIVER_SLOT_WRITE | DRIVER_SLOT_MUTABLE) != 0))
                || (!temporary && slot.name == IR_NONE)
            {
                return Err(IrVerifyError::new("driver slot metadata is invalid"));
            }
            if !temporary {
                self.store.string(slot.name)?;
            }
            driver_slot_runtime_kind(&self.store.semantic.to_type(slot.type_id)?)?;
        }
        let mut payload = FullCursor::new(self.store.payload(step.data.range())?);
        let verified_child_program = schema::DriverPayloadView::verify_fields(
            &decoder,
            step.tag,
            &mut payload,
            step.slot_count,
        )?;
        payload.finish()?;
        if let Some(child_program) = verified_child_program {
            self.verify_driver_program(
                child_program,
                program_states,
                step_states,
                owned_blocks,
                in_place_marks,
                function_signature_index,
            )?;
        }
        let mut registered_import = None;
        if step.tag == FullDriverTag::Use {
            let view = self.driver_step_view_absolute(step_index)?;
            let execution = view.execution()?;
            let schema::DriverPayloadView::Use {
                namespace,
                exports,
                child_program,
                ..
            } = view.decode_payload(&execution, view.payload()?)?
            else {
                return Err(IrVerifyError::new("driver payload row does not match its tag"));
            };
            if verified_child_program != Some(child_program) {
                return Err(IrVerifyError::new(
                    "driver payload child differs from its verified row",
                ));
            }
            let exports = exports.collect::<Result<Vec<_>, _>>()?;
            let module_type = if step.registered_module_type == IR_NONE {
                None
            } else {
                let module_type = TypeId::from_raw(step.registered_module_type)
                    .ok_or_else(|| IrVerifyError::new("registered module type id is invalid"))?;
                self.store.semantic.to_type(module_type)?;
                Some(module_type)
            };
            if let Some(module_type) = module_type {
                decoder.verify_registered_module_type(
                    module_type,
                    namespace,
                    &exports,
                    child_program,
                )?;
            }
            registered_import = Some((namespace, module_type));
        }
        decoder.finish_function()?;
        decoder.verify_driver_module_slots(registered_module_types)?;
        decoder.verify_checked_expression_facts()?;
        decoder.verify_temporary_driver_slot_types()?;
        decoder.verify_callable_slot_initializers()?;
        decoder.verify_typed_calls_after_structure()?;
        decoder.copy_in_place_marks_into(in_place_marks)?;
        Ok(registered_import)
    }
}

impl<'a> FullFunctionView<'a> {
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
        let execution = self.execution()?;
        let decoder = execution.decoder();
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
                    schema: None,
                    ty: self.program.store.semantic.to_type(validation.type_id)?,
                    name: Arc::from(self.program.store.string(validation.name)?),
                })
            });
            param_rest.push(param.flags & 1 != 0);
            param_defaults.push(if param.flags & 4 != 0 {
                Some(LoweredValue::OmittedArgument)
            } else if cold.is_none_or(|cold| cold.default == IR_NONE) {
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
                checked_type: None, mutable: capture.slot_and_flags & (1 << 31) != 0,
            });
        }
        let return_type = self.program.store.semantic.to_type(
            self.program
                .store
                .semantic
                .signature_return_type(function.signature)?,
        )?;
        let return_check = return_type
            .has_unsigned_constraint()
            .then(|| LoweredTypeCheck {
                name: Arc::from(return_type.to_string()),
                ty: return_type.clone(),
                schema: None,
            });
        let return_kind = match return_type {
            Type::Result(ok, _) => LoweredReturnKind::Result(lowered_type_from_type(&ok)?),
            Type::Optional(inner) if matches!(*inner, Type::Result(_, _)) => {
                LoweredReturnKind::OptionalResult
            }
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
            return_check,
            slot_count: function.slot_count as usize,
        })
    }

    pub(in crate::runtime::eval) fn execution(&self) -> Result<FullExecution<'a>, IrVerifyError> {
        FullExecution::for_function(ProgramOwner::Borrowed(self.program), self.index)
    }

    pub(in crate::runtime::eval) fn body<'e>(
        &self,
        execution: &'e FullExecution<'_>,
    ) -> Result<(IrBlockId, FullPayload<'e>), IrVerifyError> {
        execution.block_id(
            self.program.store.functions[self.index].body,
            BLOCK_STATEMENTS,
        )
    }

    #[cfg(test)]
    pub(in crate::runtime::eval) fn slot_count(&self) -> usize {
        self.program.store.functions[self.index].slot_count as usize
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
        let decoder = execution.decoder();
        let words = [step.location];
        let mut cursor = FullCursor::new(&words);
        let span = Span::decode(&decoder, &mut cursor)?;
        cursor.finish()?;
        Ok(span)
    }

    pub(in crate::runtime::eval) fn execution(&self) -> Result<FullExecution<'a>, IrVerifyError> {
        let step = self.program.store.driver_steps[self.index];
        Ok(FullExecution::new(
            ProgramOwner::Borrowed(self.program),
            driver_owner(self.index)
                .map_err(|_| IrVerifyError::new("driver owner is invalid"))?,
            self.program.store.driver_instruction_range(self.index)?,
            step.slot_count,
            true,
        ))
    }

    pub(in crate::runtime::eval) fn payload(&self) -> Result<FullPayload<'_>, IrVerifyError> {
        let step = self.program.store.driver_steps[self.index];
        Ok(FullPayload {
            store: &self.program.store,
            cursor: FullCursor::verified(self.program.store.payload(step.data.range())?),
        })
    }

    pub(in crate::runtime::eval) fn decode_payload<'e>(
        &self,
        execution: &'e FullExecution<'_>,
        payload: FullPayload<'e>,
    ) -> Result<schema::DriverPayloadView<'e>, IrVerifyError> {
        schema::DriverPayloadView::decode(execution, self.tag(), payload)
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
            if slot.flags & DRIVER_SLOT_TEMPORARY != 0 {
                continue;
            }
            slots.push(LoweredTopLevelSlot {
                name: Name::intern(self.program.store.string(slot.name)?),
                slot: slot.slot as usize,
                kind: driver_slot_runtime_kind(
                    &self.program.store.semantic.to_type(slot.type_id)?,
                )?,
                checked_type: None, mutable: slot.flags & DRIVER_SLOT_MUTABLE != 0,
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
    checked_expr_types: usize,
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
    prepared_module_call_plans: usize,
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
    function_definition_spans: Vec<Span>,
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

    fn declared_pattern_fields(
        declarations: &CompactDeclOutput,
        key: CheckedPatternVariantKey,
    ) -> Option<Vec<CheckedPatternField>> {
        match key.kind {
            CheckedPatternVariantKind::Tag => {
                let mut matches = declarations
                    .tag_variants_by_name
                    .iter()
                    .filter(|(name, variant)| {
                        **name == key.variant && variant.type_name == key.owner
                    })
                    .map(|(_, variant)| variant.field_types.as_slice())
                    .chain(
                        declarations
                            .qualified_tag_variants
                            .iter()
                            .filter(|(name, variant)| {
                                name.member == key.variant && variant.type_name == key.owner
                            })
                            .map(|(_, variant)| variant.field_types.as_slice()),
                    );
                let field_types = matches.next()?;
                if matches.any(|other| other != field_types) {
                    return None;
                }
                Some(
                    field_types
                        .iter()
                        .cloned()
                        .map(|ty| CheckedPatternField { name: None, ty })
                        .collect(),
                )
            }
            CheckedPatternVariantKind::Error => {
                let family = if let Some((namespace, member)) = key.owner.as_str().rsplit_once('.') {
                    declarations.qualified_error_families.get(&QualifiedName::new(
                        Name::intern(namespace),
                        Name::intern(member),
                    ))?
                } else {
                    declarations.error_families_by_name.get(&key.owner)?
                };
                let variant = family.variants.get(&key.variant)?;
                Some(
                    variant
                        .fields
                        .keys()
                        .zip(variant.fields.values())
                        .map(|(name, ty)| CheckedPatternField {
                            name: Some(*name),
                            ty: ty.clone(),
                        })
                        .collect(),
                )
            }
        }
    }

    fn register_checked_pattern_variants(
        &mut self,
        scratch: &BuildScratch,
        declarations: &CompactDeclOutput,
    ) -> Result<(), IrBuildError> {
        for (key, fields) in &scratch.checked_pattern_variants {
            let canonical_fields = Self::declared_pattern_fields(declarations, *key)
                .ok_or_else(|| IrBuildError::format(
                    "checked_pattern_declaration_missing",
                    None,
                    0,
                    self.store.tags.len(),
                ))?;
            if fields != &canonical_fields {
                return Err(IrBuildError::format(
                    "checked_pattern_declaration_mismatch",
                    None,
                    0,
                    self.store.tags.len(),
                ));
            }

            for (target, fields) in [
                (&mut self.store.checked_pattern_variants, fields),
                (&mut self.store.declared_pattern_variants, &canonical_fields),
            ] {
                let mut full_fields = Vec::with_capacity(fields.len());
                for field in fields {
                    full_fields.push(FullPatternFieldType {
                        name: field.name,
                        type_id: self.semantic.intern_type(
                            &mut self.store.semantic,
                            &executable_type(&field.ty),
                        )?,
                    });
                }
                let schema = FullPatternVariantSchema {
                    fields: full_fields,
                };
                match target.get(key) {
                    Some(existing) if existing == &schema => {}
                    Some(_) => {
                        return Err(IrBuildError::format(
                            "checked_pattern_variant_conflict",
                            None,
                            0,
                            self.store.tags.len(),
                        ));
                    }
                    None => {
                        target.insert(*key, schema);
                    }
                }
            }
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
                .with_detail(
                    unit.blocker_detail
                        .as_ref()
                        .map(|(_, detail)| detail.clone()),
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
        let headers = (0..self.store.functions.len())
            .map(|_| std::sync::OnceLock::new())
            .collect();
        let program = FullProgram {
            store: self.store,
            sources,
            symbols,
            function_definition_spans: self.function_definition_spans,
            headers,
            identities: std::sync::OnceLock::new(),
        };
        let verified = FullVerifier::verify(&program);
        verified.map_err(|_| IrBuildError::format("full_ir_verification", None, 0, 0))?;
        Ok(program)
    }

    /// Build a program whose standard-library calls resolve to implementations
    /// a loading program already prepared.
    pub(in crate::runtime::eval) fn build_compact_external_stdlib(
        program: &ArenaProgram,
        declarations: &CompactDeclOutput,
        source: &str,
        sources: Arc<SourceMap>,
        source_id: SourceId,
    ) -> Result<FullProgram, IrBuildError> {
        Self::build_compact_with_options(
            program,
            declarations,
            source,
            sources,
            source_id,
            false,
            super::super::lower::StdlibLowerLinkage::External,
        )
    }

    #[cfg(test)]
    pub(in crate::runtime::eval) fn build_compact(
        program: &ArenaProgram,
        declarations: &CompactDeclOutput,
        source: &str,
        sources: Arc<SourceMap>,
        source_id: SourceId,
    ) -> Result<FullProgram, IrBuildError> {
        Self::build_compact_with_options(
            program,
            declarations,
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
        source: &str,
        sources: Arc<SourceMap>,
        source_id: SourceId,
        allow_checker_only: bool,
        stdlib_linkage: super::super::lower::StdlibLowerLinkage,
    ) -> Result<FullProgram, IrBuildError> {
        let scope = super::super::lower::CompactRootScope::whole_program(program);
        Self::build_compact_with_scope(
            program,
            declarations,
            source,
            sources,
            source_id,
            allow_checker_only,
            stdlib_linkage,
            &scope,
        )
    }

    pub(in crate::runtime::eval) fn build_compact_with_scope(
        program: &ArenaProgram,
        declarations: &CompactDeclOutput,
        source: &str,
        sources: Arc<SourceMap>,
        source_id: SourceId,
        allow_checker_only: bool,
        stdlib_linkage: super::super::lower::StdlibLowerLinkage,
        scope: &super::super::lower::CompactRootScope<'_>,
    ) -> Result<FullProgram, IrBuildError> {
        let symbols = program.symbol_owner().clone();
        symbols.clone().with_current(|| {
            Self::build_compact_with_options_inner(
                program,
                declarations,
                source,
                sources,
                source_id,
                allow_checker_only,
                stdlib_linkage,
                scope,
                symbols,
            )
        })
    }

    fn build_compact_with_options_inner(
        program: &ArenaProgram,
        declarations: &CompactDeclOutput,
        source: &str,
        sources: Arc<SourceMap>,
        source_id: SourceId,
        allow_checker_only: bool,
        stdlib_linkage: super::super::lower::StdlibLowerLinkage,
        scope: &super::super::lower::CompactRootScope<'_>,
        symbols: crate::symbol::SymbolOwner,
    ) -> Result<FullProgram, IrBuildError> {
        let mut builder = Self::new(source_id);
        builder.reserve_function_keys(super::super::lower::compact_function_keys(program, scope))?;
        let mut pures = rustc_hash::FxHashSet::default();
        let mut procs = rustc_hash::FxHashSet::default();
        let mut qualified_pures = rustc_hash::FxHashSet::default();
        let mut qualified_procs = rustc_hash::FxHashSet::default();
        super::super::lower::lower_compact_function_units_into(
            program,
            declarations,
            source,
            &sources,
            scope,
            stdlib_linkage,
            |mut unit| {
                builder.predeclare(&[&unit])?;
                let body = unit.take_lowered_body().ok_or_else(|| {
                    IrBuildError::format(
                        "full_ir_function_blocker",
                        unit.blocker_detail.as_ref().map(|(span, _)| *span),
                        0,
                        builder.store.tags.len(),
                    )
                    .with_detail(
                        unit.blocker_detail
                            .as_ref()
                            .map(|(_, detail)| detail.clone()),
                    )
                })?;
                builder.register_checked_pattern_variants(&body.scratch.borrow(), declarations)?;
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
            source,
            &sources,
            &functions,
            scope,
        );
        builder.register_checked_pattern_variants(&driver.scratch.borrow(), declarations)?;
        let source_statements = program
            .arena
            .stmt_ids(scope.root_statements())
            .collect::<Vec<_>>();
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
                    unit.blocker_detail.as_ref().map(|(span, _)| *span),
                    0,
                    self.store.tags.len(),
                )
                .with_detail(
                    unit.blocker_detail
                        .as_ref()
                        .map(|(_, detail)| detail.clone()),
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
                    .filter(|value| !matches!(value, LoweredValue::OmittedArgument))
                    .map(|value| self.encode_value_id(value))
                    .transpose()?
                    .unwrap_or(IR_NONE);
                let validation = body.param_checks[index]
                    .as_ref()
                    .map(|check| self.encode_validation(check))
                    .transpose()?
                    .unwrap_or(IR_NONE);
                let flags = u8::from(body.param_rest[index])
                    | u8::from(body.param_defaults[index].is_some()) << 1
                    | u8::from(matches!(
                        body.param_defaults[index],
                        Some(LoweredValue::OmittedArgument)
                    )) << 2;
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
                signature_params.push((name, type_id, u32::from(flags & 0b11)));
            }
            for capture in &body.captures {
                let slot = u32::try_from(capture.slot)
                    .map_err(|_| IrBuildError::format("slot_overflow", None, 0, 0))?;
                let name = self.intern_string(&capture.name.as_str())?.raw();
                let type_id = match &capture.checked_type {
                    Some(ty) => self
                        .semantic
                        .intern_type(&mut self.store.semantic, &executable_type(ty))?,
                    None => self.intern_lowered_type(capture.kind)?,
                };
                self.store.captures.push(FullCapture {
                    name,
                    type_id,
                    slot_and_flags: slot | u32::from(capture.mutable) << 31,
                });
            }
            let checked_return_type = body
                .checked_return_type
                .as_ref()
                .map(|ty| {
                    self.semantic
                        .intern_type(&mut self.store.semantic, &executable_type(ty))
                })
                .transpose()?
                .map_or(IR_NONE, TypeId::raw);
            let return_type = if let Some(check) = &body.return_check {
                self.semantic
                    .intern_type(&mut self.store.semantic, &executable_type(&check.ty))?
            } else if body
                .checked_return_type
                .as_ref()
                .is_some_and(Type::contains_typed_callable)
            {
                TypeId::from_raw(checked_return_type).ok_or_else(|| {
                    IrBuildError::format("checked_callable_return_type", None, 0, 0)
                })?
            } else {
                self.intern_return_type(body.return_kind)?
            };
            let signature = if let Some(checked) = &body.checked_function_signature {
                self.semantic.intern_signature(
                    &mut self.store.semantic,
                    &executable_callable_type(checked),
                )?
            } else {
                self.semantic.intern_signature_parts(
                    &mut self.store.semantic,
                    &signature_params,
                    return_type,
                    None,
                )?
            };
            if body.slot_type_facts.len() != body.slot_count {
                return Err(IrBuildError::format(
                    "callable_slot_type_count",
                    Some(unit.source_span()),
                    0,
                    self.store.tags.len(),
                ));
            }
            let slot_type_facts_start = self.store.slot_type_facts.len();
            for ty in &body.slot_type_facts {
                let raw = match ty {
                    None => IR_NONE,
                    Some(ty) => self
                        .semantic
                        .intern_type(&mut self.store.semantic, &executable_type(ty))?
                        .raw(),
                };
                self.store.slot_type_facts.push(raw);
            }
            let slot_type_facts =
                table_range(slot_type_facts_start, self.store.slot_type_facts.len())?;
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
                slot_type_facts,
                checked_return_type,
            });
            self.store.function_instruction_starts.push(IR_NONE);
            self.store.function_metadata.push(FullFunctionMetadata {
                owner,
                flags: match unit.kind() {
                    LoweredFunctionKind::Pure => 0,
                    LoweredFunctionKind::Proc => 1,
                },
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
        if let Some(statement) = statement.as_ref() {
            if statement.slot_type_facts.len() != statement.slot_count {
                return Err(IrBuildError::format(
                    "driver_slot_type_fact_count",
                    Some(span),
                    0,
                    self.store.tags.len(),
                ));
            }
        }
        let write_slots = statement
            .as_ref()
            .is_some_and(|statement| matches!(statement.kind, BuildTopKind::Stmt(_)));
        if let Some(statement) = statement.as_ref() {
            let mut covered_slots = vec![false; statement.slot_count];
            for slot in &statement.slots {
                let slot_index = u32::try_from(slot.slot)
                    .map_err(|_| IrBuildError::format("driver_slot_overflow", None, 0, 0))?;
                let covered = covered_slots.get_mut(slot.slot).ok_or_else(|| {
                    IrBuildError::format("driver_slot_out_of_bounds", Some(span), 0, 0)
                })?;
                if std::mem::replace(covered, true) {
                    return Err(IrBuildError::format(
                        "duplicate_driver_slot",
                        Some(span),
                        0,
                        0,
                    ));
                }
                let type_id = statement
                    .slot_type_facts
                    .get(slot.slot)
                    .and_then(Option::as_ref)
                    .or(slot.checked_type.as_ref())
                    .map(|ty| {
                        self.semantic
                            .intern_type(&mut self.store.semantic, &executable_type(ty))
                    })
                    .transpose()?
                    .map_or_else(|| self.intern_lowered_type(slot.kind), Ok)?;
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
            for (slot, covered) in covered_slots.iter().enumerate() {
                if *covered {
                    continue;
                }
                let Some(ty) = statement
                    .slot_type_facts
                    .get(slot)
                    .and_then(Option::as_ref)
                    .filter(|ty| checked_type_contains_typed_callable(ty))
                else {
                    continue;
                };
                let type_id = self
                    .semantic
                    .intern_type(&mut self.store.semantic, &executable_type(ty))?;
                self.store.driver_slots.push(FullDriverSlot {
                    name: IR_NONE,
                    type_id,
                    slot: u32::try_from(slot)
                        .map_err(|_| IrBuildError::format("driver_slot_overflow", None, 0, 0))?,
                    flags: DRIVER_SLOT_READ | DRIVER_SLOT_TEMPORARY,
                    reserved: [0; 3],
                });
            }
        }
        let slots = table_range(slots_start, self.store.driver_slots.len())?;
        self.current_owner = Some(owner);
        self.current_slot_count = u32::try_from(slot_count)
            .map_err(|_| IrBuildError::format("slot_overflow", None, 0, 0))?;
        let mut registered_module_type = IR_NONE;
        let mut payload = self.take_payload();
        let row = match statement.as_ref().map(|statement| &statement.kind) {
            None => schema::DriverPayloadRowRef::Skip {},
            Some(BuildTopKind::Use {
                key, alias, path, namespace, exports, checked_type, span, ..
            }) => {
                registered_module_type = match checked_type {
                    Some(ty @ Type::Module(_)) => self
                        .semantic
                        .intern_type(&mut self.store.semantic, &executable_type(ty))?
                        .raw(),
                    Some(_) => {
                        return Err(IrBuildError::format(
                            "driver_use_checked_type",
                            Some(*span),
                            0,
                            self.store.tags.len(),
                        ));
                    }
                    None => IR_NONE,
                };
                schema::DriverPayloadRowRef::Use {
                    key, alias, path, namespace, exports,
                    child_program: &child_program,
                    span,
                }
            }
            Some(BuildTopKind::Let { target, ty, validation, mutable, value, value_span }) =>
                schema::DriverPayloadRowRef::Let {
                    target, ty, validation, mutable, value, value_span,
                },
            Some(BuildTopKind::LetRecord { source, fields, target, mutable, span }) =>
                schema::DriverPayloadRowRef::LetRecord {
                    source, fields, target, mutable, span,
                },
            Some(BuildTopKind::Assign { target, op, value, span }) =>
                schema::DriverPayloadRowRef::Assign { target, op, value, span },
            Some(BuildTopKind::Discard { value, span }) =>
                schema::DriverPayloadRowRef::Discard { value, span },
            Some(BuildTopKind::Stmt(statement)) =>
                schema::DriverPayloadRowRef::Stmt { statement },
            Some(BuildTopKind::Expr(value)) => schema::DriverPayloadRowRef::Expr { value },
            Some(BuildTopKind::Defer { value, on_error, span }) =>
                schema::DriverPayloadRowRef::Defer { value, on_error, span },
            Some(BuildTopKind::SignalHook {
                signal, pre_cancel, body, slots, slot_count, span,
            }) => schema::DriverPayloadRowRef::SignalHook {
                signal, pre_cancel, body, slots, slot_count, span,
            },
        };
        let error_span = match statement.as_ref().map(|statement| &statement.kind) {
            Some(BuildTopKind::Use { span, .. })
            | Some(BuildTopKind::SignalHook { span, .. }) => *span,
            _ => span,
        };
        let tag = row.encode(self, &mut payload, error_span)?;
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
            registered_module_type,
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
                if slot.flags & DRIVER_SLOT_TEMPORARY != 0 {
                    continue;
                }
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
        self.store.resource_transfers.push(false);
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
        let ty = lowered_type_to_type(ty);
        self.semantic.intern_type(&mut self.store.semantic, &ty)
    }

    fn intern_return_type(&mut self, kind: LoweredReturnKind) -> Result<TypeId, IrBuildError> {
        match kind {
            LoweredReturnKind::Plain(ty) => self.intern_lowered_type(ty),
            LoweredReturnKind::Result(ty) => {
                let ok = lowered_type_to_type(ty);
                self.semantic.intern_type(
                    &mut self.store.semantic,
                    &Type::Result(Box::new(ok), Box::new(Type::Error)),
                )
            }
            // The signature keeps only the shape that selects the kind again
            // when the function header is read back.
            LoweredReturnKind::OptionalResult => self.semantic.intern_type(
                &mut self.store.semantic,
                &Type::Optional(Box::new(Type::Result(
                    Box::new(Type::Any),
                    Box::new(Type::Error),
                ))),
            ),
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
            checked_expr_types: self.store.checked_expr_types.len(),
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
            prepared_module_call_plans: self.store.prepared_module_call_plans.len(),
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
        self.store.resource_transfers.truncate(checkpoint.tags);
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
        self.store
            .checked_expr_types
            .truncate(checkpoint.checked_expr_types);
        self.store.params.truncate(checkpoint.params);
        self.store.captures.truncate(checkpoint.captures);
        self.store.validations.truncate(checkpoint.validations);
        self.store.strings.truncate(checkpoint.strings);
        self.store.string_bytes.truncate(checkpoint.string_bytes);
        self.store.bytes.truncate(checkpoint.bytes);
        self.store.byte_data.truncate(checkpoint.byte_data);
        self.store
            .prepared_regexes
            .truncate(checkpoint.prepared_regexes);
        self.store
            .prepared_constants
            .truncate(checkpoint.prepared_constants);
        self.store.wire_enums.truncate(checkpoint.wire_enums);
        self.store
            .prepared_schemas
            .truncate(checkpoint.prepared_schemas);
        self.store
            .prepared_module_call_plans
            .truncate(checkpoint.prepared_module_call_plans);
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

fn checked_type_contains_typed_callable(ty: &Type) -> bool {
    match ty {
        Type::Module(module) => module.exports.values().any(|export| match export {
            ModuleExportType::Value { ty, .. } => ty.contains_typed_callable(),
            ModuleExportType::Proc { .. } | ModuleExportType::Pure { .. } => true,
        }),
        _ => ty.contains_typed_callable(),
    }
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
    tags.iter().fold(0, |effects, tag| effects | tag.effects())
}

pub(in crate::runtime::eval) fn lowered_type_to_type(ty: LoweredType) -> Type {
    match ty {
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
        LoweredType::FsLock => Type::FsLock,
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
        LoweredType::Set => Type::Set(Box::new(Type::Any)),
    }
}

fn executable_type(ty: &Type) -> Type {
    // The current runtime treats checker recovery types as wildcards. Commit
    // that executable meaning explicitly so recovery identities never enter
    // the stable semantic pool.
    match ty {
        Type::Unknown | Type::Invalid => Type::Any,
        Type::List(inner) => Type::List(Box::new(executable_type(inner))),
        Type::Map(key, inner) => Type::Map(
            Box::new(executable_type(key)),
            Box::new(executable_type(inner)),
        ),
        Type::Stream(inner) => Type::Stream(Box::new(executable_type(inner))),
        Type::Optional(inner) => Type::Optional(Box::new(executable_type(inner))),
        Type::Union(members) => Type::Union(members.iter().map(executable_type).collect()),
        Type::Validated(validated) => {
            Type::Validated(Box::new(validated.map_base(executable_type)))
        }
        Type::Callable(callable) => {
            Type::Callable(std::sync::Arc::new(crate::sema::types::TypedCallable {
                pure: callable.pure,
                sig: executable_callable_type(&callable.sig),
            }))
        }
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
        Type::Module(module) => Type::Module(std::sync::Arc::new(crate::sema::types::ModuleType {
            exact: module.exact,
            exports: module
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
                .collect::<BTreeMap<_, _>>(),
        })),
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

pub(in crate::runtime::eval) fn lowered_type_for_type(ty: &Type) -> Option<LoweredType> {
    Some(match ty {
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
        Type::FsLock => LoweredType::FsLock,
        Type::Stream(_) => LoweredType::Stream,
        Type::Set(_) => LoweredType::Set,
        Type::Pure => LoweredType::Pure,
        Type::Proc => LoweredType::Proc,
        Type::Callable(callable) if callable.pure => LoweredType::Pure,
        Type::Callable(_) => LoweredType::Proc,
        Type::Error | Type::ErrorFamily(_) | Type::ErrorVariant { .. } => {
            LoweredType::Error
        }
        Type::ErasedRecord | Type::Record(_) => LoweredType::Record,
        Type::Module(_) | Type::DynamicModule => LoweredType::Module,
        Type::List(_) => LoweredType::List,
        Type::Map(_, _) => LoweredType::Map,
        Type::Tag(_) => LoweredType::Tag,
        Type::Result(_, _) => LoweredType::Result,
        Type::Null | Type::Optional(_) | Type::Union(_) => LoweredType::Any,
        // A validated value is stored as a value of its base.
        Type::Validated(validated) => return lowered_type_for_type(validated.base()),
        // Policy-only and transient types are handled by their narrow caller
        // contracts rather than by the shared mapping.
        Type::BuiltinParameter(_)
        | Type::Inference(_)
        | Type::Invalid
        | Type::EnvPathList
        | Type::ErrorFacet(_)
        | Type::ProcessError => return None,
    })
}

fn typed_callable_purity(ty: &Type) -> Option<bool> {
    match ty {
        Type::Callable(callable) => Some(callable.pure),
        Type::Validated(validated) => typed_callable_purity(validated.base()),
        _ => None,
    }
}

/// The verifier rejects checker recovery types but accepts error facets as Error.
fn lowered_type_from_type(ty: &Type) -> Result<LoweredType, IrVerifyError> {
    match ty {
        Type::ErrorFacet(_) => Ok(LoweredType::Error),
        Type::BuiltinParameter(_)
        | Type::Inference(_)
        | Type::Invalid
        | Type::EnvPathList
        | Type::ProcessError => {
            Err(IrVerifyError::new(
                "semantic type has no lowered runtime equivalent",
            ))
        }
        ty => lowered_type_for_type(ty).ok_or_else(|| {
            IrVerifyError::new("semantic type has no lowered runtime equivalent")
        }),
    }
}

/// The lowered environment-path read materializes a path list while its checked
/// slot keeps `EnvPathList` for method resolution and checker facts.
fn driver_slot_runtime_kind(ty: &Type) -> Result<LoweredType, IrVerifyError> {
    match ty {
        Type::EnvPathList => Ok(LoweredType::List),
        ty => lowered_type_from_type(ty),
    }
}

/// The hole kind an encoded index names.
fn text_hole_kind(index: u32) -> Result<crate::sema::check::TextHoleKind, IrVerifyError> {
    crate::sema::check::TextHoleKind::from_index(index as usize)
        .ok_or_else(|| IrVerifyError::new("text pattern hole kind is out of range"))
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
    store: &'a FullStore,
    cursor: FullCursor<'a>,
}

impl<'a> FullPayload<'a> {
    #[inline(always)]
    fn check_store(&self, execution: &FullExecution<'_>) -> Result<(), IrVerifyError> {
        if std::ptr::eq(self.store, execution.store()) {
            Ok(())
        } else {
            Err(IrVerifyError::new("full IR payload belongs to another program"))
        }
    }

    fn check_store_decoder(&self, decoder: &FullDecoder<'_>) -> Result<(), IrVerifyError> {
        if std::ptr::eq(self.store, decoder.store) {
            Ok(())
        } else {
            Err(IrVerifyError::new("full IR payload belongs to another program"))
        }
    }

    #[inline(always)]
    pub(in crate::runtime::eval) fn raw(&mut self) -> Result<u32, IrVerifyError> {
        self.cursor.raw()
    }

    #[cfg(test)]
    #[inline(always)]
    pub(in crate::runtime::eval) fn borrow<'e, T: BorrowCodec>(
        &'e mut self,
        execution: &'e FullExecution<'_>,
    ) -> Result<T::View<'e>, IrVerifyError>
    where
        'a: 'e,
    {
        self.check_store(execution)?;
        let mut cursor = FullCursor {
            words: self.cursor.words,
            index: self.cursor.index,
            verified: self.cursor.verified,
        };
        let view = T::borrow(DecodeContext::new(execution), &mut cursor)?;
        self.cursor.index = cursor.index;
        Ok(view)
    }

    #[inline(always)]
    pub(in crate::runtime::eval) fn finish(self) -> Result<(), IrVerifyError> {
        self.cursor.finish()
    }
}

/// Lets schema tests decode a scratch store without exposing `FullStore` to
/// runtime modules through the executable ownership enum.
#[cfg(test)]
#[derive(Clone, Copy)]
pub(in crate::runtime::eval) struct TestStoreOwner<'p>(&'p FullStore);

#[derive(Clone)]
pub(in crate::runtime::eval) enum ProgramOwner<'p> {
    Borrowed(&'p FullProgram),
    Shared(Arc<FullProgram>),
    #[cfg(test)]
    TestStore(TestStoreOwner<'p>),
}

impl ProgramOwner<'_> {
    pub(in crate::runtime::eval) fn program(&self) -> &FullProgram {
        match self {
            Self::Borrowed(program) => program,
            Self::Shared(program) => program,
            #[cfg(test)]
            Self::TestStore(_) => unreachable!("test store has no executable program"),
        }
    }

    /// Returns the retained Arc needed by a suspended stream call. A borrowed
    /// root is dispatchable only while its evaluator still owns that exact program.
    pub(in crate::runtime::eval) fn dispatch_arc(
        &self,
        root: Option<&Arc<FullProgram>>,
    ) -> Option<Arc<FullProgram>> {
        match self {
            Self::Borrowed(program) => root
                .filter(|active| std::ptr::eq(active.as_ref(), *program))
                .cloned(),
            Self::Shared(program) => Some(Arc::clone(program)),
            #[cfg(test)]
            Self::TestStore(_) => None,
        }
    }

    fn store(&self) -> &FullStore {
        match self {
            Self::Borrowed(program) => &program.store,
            Self::Shared(program) => &program.store,
            #[cfg(test)]
            Self::TestStore(owner) => owner.0,
        }
    }
}

/// A frame keeps executable ownership and the verified range it may read.
/// Decoders borrow the owner only while a payload or view is being read.
#[derive(Clone)]
pub(in crate::runtime::eval) struct FullExecution<'p> {
    program: ProgramOwner<'p>,
    owner: u32,
    instruction_range: std::ops::Range<usize>,
    slot_count: u32,
    verified: bool,
}

impl<'p> FullExecution<'p> {
    fn new(
        program: ProgramOwner<'p>,
        owner: u32,
        instruction_range: std::ops::Range<usize>,
        slot_count: u32,
        verified: bool,
    ) -> Self {
        Self {
            program,
            owner,
            instruction_range,
            slot_count,
            verified,
        }
    }

    pub(in crate::runtime::eval) fn for_function(
        program: ProgramOwner<'p>,
        index: usize,
    ) -> Result<Self, IrVerifyError> {
        let full_program = program.program();
        let function = full_program
            .store
            .functions
            .get(index)
            .ok_or_else(|| IrVerifyError::new("function id is out of bounds"))?;
        let owner = IrFunctionId::new(index)
            .map_err(|_| IrVerifyError::new("function id is invalid"))?
            .raw();
        let instruction_range = full_program.store.function_instruction_range(index)?;
        let slot_count = function.slot_count;
        Ok(Self::new(
            program,
            owner,
            instruction_range,
            slot_count,
            true,
        ))
    }

    #[cfg(test)]
    pub(in crate::runtime::eval) fn shared_function(
        program: Arc<FullProgram>,
        index: usize,
    ) -> Result<FullExecution<'static>, IrVerifyError> {
        FullExecution::<'static>::for_function(ProgramOwner::Shared(program), index)
    }

    pub(in crate::runtime::eval) fn program(&self) -> &FullProgram {
        self.program.program()
    }

    fn store(&self) -> &FullStore {
        self.program.store()
    }

    pub(in crate::runtime::eval) fn program_owner(&self) -> ProgramOwner<'p> {
        self.program.clone()
    }

    fn decoder(&self) -> FullDecoder<'_> {
        FullDecoder {
            store: self.store(),
            owner: self.owner,
            instruction_range: self.instruction_range.clone(),
            instruction_states: None,
            instruction_proof: None,
            block_states: None,
            slot_count: self.slot_count,
            pattern_ceiling: Cell::new(usize::MAX),
            local_immutable_let_initializers: RefCell::default(),
            function_signature_index: None,
            verified: self.verified,
        }
    }

    pub(in crate::runtime::eval) fn thread_local(&self) -> Self {
        self.clone()
    }

    /// The evaluator calls this for instruction IDs from an already verified
    /// owner range, after verification has installed the derived mark table.
    pub(in crate::runtime::eval) fn is_in_place(&self, instruction: u32) -> bool {
        *self
            .store()
            .in_place
            .get()
            .expect("verified full IR has an in-place mark table")
            .get(instruction as usize)
            .expect("verified instruction ID is in bounds")
    }

    #[inline(always)]
    pub(in crate::runtime::eval) fn transfers_resource(&self, instruction: u32) -> bool {
        *self
            .store()
            .resource_transfers
            .get(instruction as usize)
            .expect("verified resource-transfer instruction ID is in bounds")
    }

    pub(in crate::runtime::eval) fn function_identity(
        &self,
    ) -> Result<(LoweredFunctionKey, LoweredFunctionKind), IrVerifyError> {
        let id = IrFunctionId::from_raw(self.owner)
            .ok_or_else(|| IrVerifyError::new("indexed execution owner is not a function"))?;
        let function = self
            .store()
            .functions
            .get(id.index())
            .ok_or_else(|| IrVerifyError::new("function id is out of bounds"))?;
        let metadata = self
            .store()
            .function_metadata
            .get(id.index())
            .copied()
            .ok_or_else(|| IrVerifyError::new("function metadata is missing"))?;
        let name = Name::intern(self.store().string(function.name)?);
        let key = if metadata.owner == IR_NONE {
            LoweredFunctionKey::Name(name)
        } else {
            LoweredFunctionKey::Qualified(QualifiedName::new(
                Name::intern(self.store().string(metadata.owner)?),
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
    ) -> Result<(FullTag, FullPayload<'_>), IrVerifyError> {
        let decoder = self.decoder();
        if decoder.verified {
            let index = raw as usize;
            let tag = unsafe { *decoder.store.tags.get_unchecked(index) };
            let data = unsafe { *decoder.store.data.get_unchecked(index) };
            let words = unsafe { decoder.store.payload_unchecked(data.range()) };
            return Ok((
                tag,
                FullPayload {
                    store: decoder.store,
                    cursor: FullCursor::verified(words),
                },
            ));
        }
        let index = raw as usize;
        if !decoder.instruction_range.contains(&index) {
            return Err(IrVerifyError::new(
                "full IR instruction belongs to another function",
            ));
        }
        let tag = decoder
            .store
            .tags
            .get(index)
            .copied()
            .ok_or_else(|| IrVerifyError::new("full IR instruction is out of bounds"))?;
        let data = decoder.store.data[index];
        Ok((
            tag,
            FullPayload {
                store: decoder.store,
                cursor: decoder
                    .cursor(decoder.store.payload(data.range())?),
            },
        ))
    }

    pub(in crate::runtime::eval) fn pattern(
        &self,
        raw: u32,
    ) -> Result<(FullPatternTag, FullPayload<'_>), IrVerifyError> {
        let decoder = self.decoder();
        let index = raw as usize;
        if decoder.verified {
            let tag = unsafe { *decoder.store.patterns.get_unchecked(index) };
            let data = unsafe { *decoder.store.pattern_data.get_unchecked(index) };
            return Ok((
                tag,
                FullPayload {
                    store: decoder.store,
                    cursor: decoder.cursor(unsafe {
                        decoder.store.payload_unchecked(data.range())
                    }),
                },
            ));
        }
        let tag = decoder
            .store
            .patterns
            .get(index)
            .copied()
            .ok_or_else(|| IrVerifyError::new("pattern id is out of bounds"))?;
        let data = decoder.store.pattern_data[index];
        Ok((
            tag,
            FullPayload {
                store: decoder.store,
                cursor: decoder
                    .cursor(decoder.store.payload(data.range())?),
            },
        ))
    }

    #[cfg(test)]
    pub(in crate::runtime::eval) fn pattern_view<'e>(
        &'e self,
        pattern: PatternId<'e>,
    ) -> Result<schema::PatternView<'e>, IrVerifyError> {
        let raw = pattern.raw_for_store(self.store())?;
        let (tag, payload) = self.pattern(raw)?;
        schema::PatternView::decode(self, tag, payload)
    }

    #[cfg(test)]
    #[inline(always)]
    pub(in crate::runtime::eval) fn block<'e>(
        &'e self,
        input: &mut FullPayload<'e>,
        expected_flags: u8,
    ) -> Result<(IrBlockId, FullPayload<'e>), IrVerifyError> {
        input.check_store(self)?;
        let decoder = self.decoder();
        let (id, block) = decoder.block(&mut input.cursor, expected_flags)?;
        Ok((id, FullPayload { store: decoder.store, cursor: block }))
    }

    #[inline(always)]
    pub(in crate::runtime::eval) fn block_id(
        &self,
        raw: u32,
        expected_flags: u8,
    ) -> Result<(IrBlockId, FullPayload<'_>), IrVerifyError> {
        let decoder = self.decoder();
        let id = IrBlockId::from_raw(raw)
            .ok_or_else(|| IrVerifyError::new("full IR block id is invalid"))?;
        if decoder.verified {
            let block = unsafe { *decoder.store.blocks.get_unchecked(id.index()) };
            debug_assert!(
                block.owner == IR_NONE || block.owner == decoder.owner,
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
                    store: decoder.store,
                    cursor: decoder.cursor(unsafe {
                        decoder.store.payload_unchecked(block.instructions)
                    }),
                },
            ));
        }
        let block = decoder
            .store
            .blocks
            .get(id.index())
            .copied()
            .ok_or_else(|| IrVerifyError::new("full IR block id is out of bounds"))?;
        if block.owner != IR_NONE && block.owner != decoder.owner {
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
                store: decoder.store,
                cursor: decoder.cursor(decoder.store.payload(block.instructions)?),
            },
        ))
    }

    pub(in crate::runtime::eval) fn stage_view<'e>(
        &'e self,
        raw: u32,
    ) -> Result<StageView<'e>, IrVerifyError> {
        StageView::decode(DecodeContext::new(self), raw)
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
    instruction_proof: Option<RefCell<in_place::InstructionProof>>,
    block_states: Option<OwnedBlockStates>,
    slot_count: u32,
    pattern_ceiling: Cell<usize>,
    local_immutable_let_initializers: RefCell<Option<FxHashMap<usize, Option<usize>>>>,
    function_signature_index: Option<Arc<FunctionSignatureIndex>>,
    verified: bool,
}

/// The blocks one owner's verification entered. Tracking only those keeps
/// verifying a body proportional to the body, not to the whole program.
struct OwnedBlockStates {
    /// 1 while a block is open and 2 once it is finished.
    states: RefCell<FxHashMap<usize, u8>>,
    /// Finished blocks that the store assigns to this owner.
    finished_owned: Cell<usize>,
    /// Blocks that the store assigns to this owner.
    owned: usize,
}

impl OwnedBlockStates {
    fn new(owner: u32, owned_blocks: &FxHashMap<u32, usize>) -> Self {
        Self {
            states: RefCell::default(),
            finished_owned: Cell::new(0),
            owned: owned_blocks.get(&owner).copied().unwrap_or(0),
        }
    }
}

/// How many blocks the store assigns to each function or driver step.
fn owned_block_counts(store: &FullStore) -> FxHashMap<u32, usize> {
    let mut counts = FxHashMap::default();
    for block in &store.blocks {
        *counts.entry(block.owner).or_default() += 1;
    }
    counts
}

impl<'a> FullDecoder<'a> {
    fn expression_view<'e>(
        &'e self,
        instruction: usize,
    ) -> Result<schema::ExprView<'e>, IrVerifyError> {
        let tag = *self
            .store
            .tags
            .get(instruction)
            .ok_or_else(|| IrVerifyError::new("checked expression instruction is out of bounds"))?;
        let payload = FullPayload {
            store: self.store,
            cursor: FullCursor::new(self.store.payload(self.store.data[instruction].range())?),
        };
        schema::ExprView::decode_from_decoder(self, tag, payload)
    }

    fn statement_view<'e>(
        &'e self,
        instruction: usize,
    ) -> Result<schema::StmtView<'e>, IrVerifyError> {
        let tag = *self
            .store
            .tags
            .get(instruction)
            .ok_or_else(|| IrVerifyError::new("checked statement instruction is out of bounds"))?;
        let payload = FullPayload {
            store: self.store,
            cursor: FullCursor::new(self.store.payload(self.store.data[instruction].range())?),
        };
        schema::StmtView::decode_from_decoder(self, tag, payload)
    }

    #[inline(always)]
    fn cursor(&self, words: &'a [u32]) -> FullCursor<'a> {
        if self.verified {
            FullCursor::verified(words)
        } else {
            FullCursor::new(words)
        }
    }

    fn pattern_list_cursor(
        &self,
        input: &mut FullCursor<'_>,
    ) -> Result<FullCursor<'a>, IrVerifyError> {
        let block = IrBlockId::from_raw(input.raw()?)
            .ok_or_else(|| IrVerifyError::new("pattern child list id is invalid"))?;
        let block = self
            .store
            .blocks
            .get(block.index())
            .ok_or_else(|| IrVerifyError::new("pattern child list id is out of bounds"))?;
        // Inspection follows structural verification and must not claim block
        // ownership a second time while reading the same capture contract.
        Ok(FullCursor::new(self.store.payload(block.instructions)?))
    }

    fn pattern_capture_slots(&self, pattern: usize) -> Result<BTreeSet<usize>, IrVerifyError> {
        let mut slots = BTreeSet::new();
        let mut pending = vec![pattern];
        while let Some(pattern) = pending.pop() {
            let tag = *self
                .store
                .patterns
                .get(pattern)
                .ok_or_else(|| IrVerifyError::new("pattern id is out of bounds"))?;
            let mut payload = FullCursor::new(
                self.store
                    .payload(self.store.pattern_data[pattern].range())?,
            );
            let mut captures = Vec::new();
            match tag {
                FullPatternTag::Bind => captures.push(usize::decode(self, &mut payload)?),
                FullPatternTag::Alias => {
                    pending.push(payload.raw()? as usize);
                    captures.push(usize::decode(self, &mut payload)?);
                }
                FullPatternTag::Type => {
                    Type::decode(self, &mut payload)?;
                    captures.extend(Option::<usize>::decode(self, &mut payload)?);
                }
                FullPatternTag::ResultOk | FullPatternTag::ResultErr => {
                    captures.extend(Option::<usize>::decode(self, &mut payload)?)
                }
                FullPatternTag::ResultTest => {
                    bool::decode(self, &mut payload)?;
                    pending.push(payload.raw()? as usize);
                }
                FullPatternTag::Tag => {
                    // Enum payloads retain both the declaring type and variant names.
                    Name::decode(self, &mut payload)?;
                    Name::decode(self, &mut payload)?;
                    captures.extend(
                        BuildPatternIdSlots::decode(self, &mut payload)?
                            .into_iter()
                            .flatten(),
                    );
                }
                FullPatternTag::List
                | FullPatternTag::TagTest
                | FullPatternTag::Alternation
                | FullPatternTag::Text => {
                    if tag == FullPatternTag::TagTest {
                        Name::decode(self, &mut payload)?;
                        Name::decode(self, &mut payload)?;
                    }
                    let mut children = self.pattern_list_cursor(&mut payload)?;
                    let count = children.raw()? as usize;
                    for index in 0..count {
                        let child = children.raw()? as usize;
                        if tag != FullPatternTag::Alternation || index == 0 {
                            pending.push(child);
                        }
                    }
                    if tag == FullPatternTag::List && bool::decode(self, &mut payload)? {
                        pending.push(payload.raw()? as usize);
                    }
                }
                FullPatternTag::RecordTest | FullPatternTag::ErrorTest => {
                    if tag == FullPatternTag::ErrorTest {
                        Name::decode(self, &mut payload)?;
                        Name::decode(self, &mut payload)?;
                    }
                    let mut children = self.pattern_list_cursor(&mut payload)?;
                    let count = children.raw()? as usize;
                    for _ in 0..count {
                        Name::decode(self, &mut children)?;
                        pending.push(children.raw()? as usize);
                    }
                }
                _ => {}
            }
            for slot in captures {
                if !slots.insert(slot) {
                    return Err(IrVerifyError::new(
                        "pattern writes the same capture slot twice",
                    ));
                }
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
            let mut states = states.states.borrow_mut();
            match states.get(&id.index()).copied().unwrap_or(0) {
                0 => {
                    states.insert(id.index(), 1);
                }
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
        let owner = self.store.blocks[id.index()].owner;
        if owner != IR_NONE
            && let Some(states) = &self.block_states
            && states.states.borrow_mut().insert(id.index(), 2) != Some(2)
            && owner == self.owner
        {
            states.finished_owned.set(states.finished_owned.get() + 1);
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
        if let Some(proof) = &self.instruction_proof {
            proof.borrow_mut().enter(index, tag);
        }
        Ok((index, tag, self.cursor(self.store.payload(data.range())?)))
    }

    #[inline(always)]
    fn verify_comparison_chain_shape(
        &self,
        mut payload: FullCursor<'_>,
    ) -> Result<(), IrVerifyError> {
        let id = IrBlockId::from_raw(payload.raw()?)
            .ok_or_else(|| IrVerifyError::new("comparison chain block id is invalid"))?;
        let block = self
            .store
            .blocks
            .get(id.index())
            .ok_or_else(|| IrVerifyError::new("comparison chain block is missing"))?;
        let mut pairs = self.cursor(self.store.payload(block.instructions)?);
        let len = pairs.raw()? as usize;
        if len < 2 {
            return Err(IrVerifyError::new(
                "comparison chain requires at least two pairs",
            ));
        }
        for _ in 0..len {
            let pair = pairs.raw()? as usize;
            if self.store.tags.get(pair) != Some(&FullTag::ExprBinary) {
                return Err(IrVerifyError::new("comparison chain requires binary pairs"));
            }
            let mut pair_payload = self.cursor(self.store.payload(self.store.data[pair].range())?);
            let op = BinaryOp::decode(self, &mut pair_payload)?;
            if !matches!(
                op,
                BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
            ) {
                return Err(IrVerifyError::new(
                    "comparison chain requires ordering operators",
                ));
            }
        }
        pairs.finish()
    }

    /// A typed call was checked against one callable type: the row names
    /// that type and its kind, and passes each parameter exactly once. The
    /// executor binds arguments by position and reads no defaults, so a row
    /// with another argument count, a splice, or an omitted slot would call
    /// the function with arguments the checker never saw.
    fn verify_typed_call_shape(&self, payload: FullCursor<'_>) -> Result<(), IrVerifyError> {
        let schema::ExprView::TypedCall {
            callee,
            pure,
            signature,
            args,
            ..
        } = schema::ExprView::decode_from_decoder(
            self,
            FullTag::ExprTypedCall,
            FullPayload {
                store: self.store,
                cursor: payload,
            },
        )?
        else {
            return Err(IrVerifyError::new(
                "typed call shape decoder returned another expression kind",
            ));
        };
        let callee = callee.raw() as usize;
        // Top-level steps have no enclosing function to hold the call to.
        if driver_owner_index(self.owner).is_none()
            && let Some(function) = IrFunctionId::from_raw(self.owner)
        {
            let function = function.index();
            let metadata = self
                .store
                .function_metadata
                .get(function)
                .ok_or_else(|| IrVerifyError::new("typed call owner metadata is missing"))?;
            // A pure function calls only pure callables, whatever a row says
            // its callee is.
            if metadata.flags & 1 == 0 && !pure {
                return Err(IrVerifyError::new(
                    "typed call to a proc inside a pure function",
                ));
            }
        }
        let ty = signature.id;
        self.verify_typed_callee_signature(callee, ty)?;
        let semantic = &self.store.semantic;
        let signature = semantic
            .typed_callable_signature(ty)
            .map_err(|_| IrVerifyError::new("typed call signature is not a callable type"))?;
        if semantic.typed_callable_is_pure(ty)? != pure {
            return Err(IrVerifyError::new(
                "typed call kind does not match its callable type",
            ));
        }
        let len = args.len();
        if len != semantic.signature_param_count(signature)? {
            return Err(IrVerifyError::new(
                "typed call argument count does not match its signature",
            ));
        }
        for argument in args {
            if !matches!(argument?, decode::CallArgView::Single(_)) {
                return Err(IrVerifyError::new(
                    "typed call argument is not a single value",
                ));
            }
        }
        Ok(())
    }

    /// Lowering checked the source callee against this signature. Verify the
    /// encoded callee independently from its exact slot, field, or producer.
    fn verify_typed_callee_signature(
        &self,
        callee: usize,
        signature: TypeId,
    ) -> Result<(), IrVerifyError> {
        let reason = match self.store.tags.get(callee) {
            Some(FullTag::ExprField) => {
                "typed call signature differs from its callee field's callable type"
            }
            Some(FullTag::ExprCall | FullTag::ExprDirectPureCall | FullTag::ExprSelfCall) => {
                "typed call signature differs from its callee call result's callable type"
            }
            Some(FullTag::ExprParam) => {
                "typed call signature differs from its callee slot's callable type"
            }
            _ => "typed call signature differs from its checked callee type",
        };
        let actual = match self.derive_typed_callee_type(callee) {
            Ok(actual) => actual,
            Err(error)
                if self.store.tags.get(callee) == Some(&FullTag::ExprParam)
                    && (error.message == "typed callee slot has no checked type fact"
                        || error
                            .message
                            .starts_with("typed callee slot has no checked type fact:")) =>
            {
                let slot = self
                    .store
                    .payload(self.store.data[callee].range())?
                    .first()
                    .copied()
                    .ok_or_else(|| IrVerifyError::new("typed callee slot is missing"))?
                    as usize;
                let expected = self.store.semantic.to_type(signature)?;
                self.local_function_ref_matches_type(slot, &expected)?;
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        if actual != signature {
            return Err(IrVerifyError::new(reason));
        }
        self.store
            .semantic
            .typed_callable_signature(actual)
            .map_err(|_| IrVerifyError::new("typed call callee is not a typed callable"))?;
        Ok(())
    }

    fn derive_typed_callee_type(&self, instruction: usize) -> Result<TypeId, IrVerifyError> {
        let tag = *self
            .store
            .tags
            .get(instruction)
            .ok_or_else(|| IrVerifyError::new("typed callee expression is out of bounds"))?;
        if !matches!(
            tag,
            FullTag::ExprField
                | FullTag::ExprParam
                | FullTag::ExprCall
                | FullTag::ExprDirectPureCall
                | FullTag::ExprSelfCall
                | FullTag::ExprTypedCall
        ) {
            return self
                .checked_expression_type_id(instruction)?
                .ok_or_else(|| IrVerifyError::new("typed callee has no checked expression type"));
        }
        let mut fields = Vec::new();
        let mut current = instruction;
        for _ in 0..self.store.tags.len() {
            if self.store.tags.get(current) == Some(&FullTag::ExprField) {
                let words = self
                    .store
                    .payload(self.store.data[current].range())?;
                let [base, name, ..] = words else {
                    return Err(IrVerifyError::new("typed callee field payload is truncated"));
                };
                fields.push(Name::intern(self.store.string(*name)?));
                current = *base as usize;
                continue;
            }
            let leaf_tag = *self
                .store
                .tags
                .get(current)
                .ok_or_else(|| IrVerifyError::new("typed callee expression is out of bounds"))?;
            let mut ty = if matches!(
                leaf_tag,
                FullTag::ExprParam
                    | FullTag::ExprCall
                    | FullTag::ExprDirectPureCall
                    | FullTag::ExprSelfCall
                    | FullTag::ExprTypedCall
            ) {
                self.derive_typed_callee_leaf_type(current)?
            } else {
                self.checked_expression_type_id(current)?
                    .ok_or_else(|| IrVerifyError::new("typed callee receiver has no checked type"))?
            };
            for field in fields.into_iter().rev() {
                let Some(field_type) = self
                    .store
                    .semantic
                    .field_type(ty, field)?
                else {
                    return Err(IrVerifyError::new(format!(
                        "typed callee field `{field}` is absent from checked receiver type {:?}",
                        self.store.semantic.to_type(ty)?
                    )));
                };
                ty = field_type;
            }
            return Ok(ty);
        }
        Err(IrVerifyError::new("typed callee field chain contains a cycle"))
    }

    fn derive_typed_callee_leaf_type(
        &self,
        instruction: usize,
    ) -> Result<TypeId, IrVerifyError> {
        let tag = *self
            .store
            .tags
            .get(instruction)
            .ok_or_else(|| IrVerifyError::new("typed callee expression is out of bounds"))?;
        let words = self
            .store
            .payload(self.store.data[instruction].range())?;
        match tag {
            FullTag::ExprParam => {
                let slot = *words
                    .first()
                    .ok_or_else(|| IrVerifyError::new("typed callee slot is missing"))?
                    as usize;
                self.checked_slot_type(slot)?.ok_or_else(|| {
                    IrVerifyError::new("typed callee slot has no checked type fact")
                })
            }
            FullTag::ExprCall | FullTag::ExprDirectPureCall => {
                let function = IrFunctionId::from_raw(*words.first().ok_or_else(|| {
                    IrVerifyError::new("typed callee call target is missing")
                })?)
                .ok_or_else(|| IrVerifyError::new("typed callee call target is invalid"))?;
                self.function_return_type(function.index())
            }
            FullTag::ExprSelfCall => {
                let function = IrFunctionId::from_raw(self.owner)
                    .ok_or_else(|| IrVerifyError::new("self call has no function owner"))?;
                self.function_return_type(function.index())
            }
            FullTag::ExprTypedCall => {
                let raw = *words
                    .get(2)
                    .ok_or_else(|| IrVerifyError::new("typed call signature is missing"))?;
                let ty = TypeId::from_raw(raw)
                    .ok_or_else(|| IrVerifyError::new("typed call signature type id is invalid"))?;
                let signature = self.store.semantic.typed_callable_signature(ty)?;
                self.store.semantic.signature_return_type(signature)
            }
            _ => Err(IrVerifyError::new(
                "typed callee type cannot be derived from its IR expression",
            )),
        }
    }

    fn checked_slot_type(&self, slot: usize) -> Result<Option<TypeId>, IrVerifyError> {
        if driver_owner_index(self.owner).is_none()
            && let Some(function) = IrFunctionId::from_raw(self.owner)
        {
            let function = self
                .store
                .functions
                .get(function.index())
                .ok_or_else(|| IrVerifyError::new("typed call owner is out of bounds"))?;
            let params = function
                .params
                .bounds(self.store.params.len())
                .ok_or_else(|| IrVerifyError::new("function parameter range is invalid"))?;
            if slot < params.len() {
                let parameter = params.start + slot;
                let validation = self
                    .store
                    .param_cold
                    .binary_search_by_key(&(parameter as u32), |cold| cold.param)
                    .ok()
                    .map(|index| self.store.param_cold[index].validation)
                    .filter(|validation| *validation != IR_NONE);
                if let Some(validation) = validation {
                    let validation = self
                        .store
                        .validations
                        .get(validation as usize)
                        .ok_or_else(|| {
                            IrVerifyError::new("typed callee validation is out of bounds")
                        })?;
                    return Ok(Some(validation.type_id));
                }
                return Ok(self.store.params.get(parameter).map(|param| param.type_id));
            }
            let captures = function
                .captures
                .bounds(self.store.captures.len())
                .ok_or_else(|| IrVerifyError::new("function capture range is invalid"))?;
            if let Some(capture) = self.store.captures[captures.clone()].iter().find(|capture| {
                (capture.slot_and_flags & !(1 << 31)) as usize == slot
            }) {
                return Ok(Some(capture.type_id));
            }
            let slot_types = function
                .slot_type_facts
                .bounds(self.store.slot_type_facts.len())
                .ok_or_else(|| IrVerifyError::new("function slot type fact range is invalid"))?;
            if slot >= function.slot_count as usize {
                return Err(IrVerifyError::new("typed callee slot is out of bounds"));
            }
            let raw = *self
                .store
                .slot_type_facts
                .get(slot_types.start + slot)
                .ok_or_else(|| IrVerifyError::new("typed callee slot type fact is missing"))?;
            if raw == IR_NONE {
                return Ok(None);
            }
            return TypeId::from_raw(raw)
                .map(Some)
                .ok_or_else(|| IrVerifyError::new("typed callee slot type fact is invalid"));
        }
        let owner = driver_owner_index(self.owner)
            .ok_or_else(|| IrVerifyError::new("typed call owner is invalid"))?;
        let step = self
            .store
            .driver_steps
            .get(owner)
            .ok_or_else(|| IrVerifyError::new("typed call driver owner is out of bounds"))?;
        let slots = step
            .slots
            .bounds(self.store.driver_slots.len())
            .ok_or_else(|| IrVerifyError::new("typed callee driver slot range is invalid"))?;
        if slot >= step.slot_count as usize {
            return Err(IrVerifyError::new(
                "typed callee driver slot is out of bounds",
            ));
        }
        let Some(binding) = self.store.driver_slots[slots]
            .iter()
            .find(|binding| binding.slot as usize == slot)
        else {
            return Ok(None);
        };
        self.store.semantic.to_type(binding.type_id)?;
        Ok(Some(binding.type_id))
    }

    // Function-body instructions share an owner range, so index writes once
    // before proving local expression types from unique `let` initializers.
    fn local_immutable_let_initializer(
        &self,
        slot: usize,
    ) -> Result<(bool, Option<usize>), IrVerifyError> {
        if self.local_immutable_let_initializers.borrow().is_none() {
            let index = self.local_immutable_let_initializers_uncached()?;
            *self.local_immutable_let_initializers.borrow_mut() = Some(index);
        }
        let index = self.local_immutable_let_initializers.borrow();
        Ok(index
            .as_ref()
            .and_then(|initializers| initializers.get(&slot))
            .map_or((false, None), |initializer| (true, *initializer)))
    }

    fn local_immutable_let_initializers_uncached(
        &self,
    ) -> Result<FxHashMap<usize, Option<usize>>, IrVerifyError> {
        let mut initializers = FxHashMap::default();
        for instruction in self.instruction_range.clone() {
            if self.store.tags[instruction].kind() != schema::InstructionKind::Stmt
            {
                continue;
            }
            let (written_slots, initializer) = self.statement_written_slots(instruction)?;
            for slot in written_slots {
                let initializer = initializer
                    .filter(|(written, _)| *written == slot)
                    .map(|(_, initializer)| initializer);
                if initializers.insert(slot, initializer).is_some() {
                    initializers.insert(slot, None);
                }
            }
        }
        Ok(initializers)
    }

    fn comp_target_written_slots(
        &self,
        target: decode::CompTargetView<'a>,
        written_slots: &mut Vec<usize>,
    ) -> Result<(), IrVerifyError> {
        match target {
            decode::CompTargetView::Discard => {}
            decode::CompTargetView::Slot(slot) => written_slots.push(slot),
            decode::CompTargetView::Record { mut fields } => {
                for field in &mut fields {
                    let (_, target, _) = field?;
                    self.comp_target_written_slots(target, written_slots)?;
                }
            }
        }
        Ok(())
    }

    fn statement_written_slots(
        &self,
        instruction: usize,
    ) -> Result<(Vec<usize>, Option<(usize, usize)>), IrVerifyError> {
        let mut written_slots = Vec::new();
        let mut initializer = None;
        match self.statement_view(instruction)? {
            schema::StmtView::Let { slot, value } => {
                written_slots.push(slot);
                initializer = Some((slot, value.raw() as usize));
            }
            schema::StmtView::LetInt { slot, .. }
            | schema::StmtView::LetBool { slot, .. }
            | schema::StmtView::Assign { slot, .. }
            | schema::StmtView::AssignInt { slot, .. }
            | schema::StmtView::AssignBool { slot, .. }
            | schema::StmtView::AssignField { slot, .. }
            | schema::StmtView::AssignFieldInt { slot, .. }
            | schema::StmtView::AssignPath { slot, .. }
            | schema::StmtView::DefaultParameter { slot, .. }
            | schema::StmtView::For { slot, .. }
            | schema::StmtView::ForStrLines { slot, .. } => written_slots.push(slot),
            schema::StmtView::Guard {
                target,
                else_param_slot,
                ..
            } => {
                self.comp_target_written_slots(target, &mut written_slots)?;
                written_slots.extend(else_param_slot);
            }
            schema::StmtView::With {
                mut bindings,
                else_param_slot,
                mut captures,
                ..
            } => {
                for binding in &mut bindings {
                    written_slots.push(binding?.0);
                }
                written_slots.extend(else_param_slot);
                for capture in &mut captures {
                    written_slots.push(capture?);
                }
            }
            schema::StmtView::PatternIf { mut branches, .. } => {
                for branch in &mut branches {
                    let (_, _, captures) = branch?;
                    for capture in captures {
                        written_slots.push(capture?);
                    }
                }
            }
            schema::StmtView::PatternWhile { mut captures, .. } => {
                for capture in &mut captures {
                    written_slots.push(capture?);
                }
            }
            schema::StmtView::Match { mut arms, .. } => {
                for arm in &mut arms {
                    let (pattern, _, _) = arm?;
                    written_slots.extend(self.pattern_capture_slots(pattern.raw() as usize)?);
                }
            }
            schema::StmtView::ForRecord { target, .. }
            | schema::StmtView::LetRecord { target, .. } => {
                self.comp_target_written_slots(target, &mut written_slots)?;
            }
            schema::StmtView::ScanLines {
                text_slot,
                line_slot,
                ..
            } => written_slots.extend([text_slot, line_slot]),
            _ => {}
        }
        written_slots.sort_unstable();
        written_slots.dedup();
        Ok((written_slots, initializer))
    }

    fn verify_driver_module_slots(
        &self,
        registered_module_types: &FxHashMap<Name, RegisteredModuleType>,
    ) -> Result<(), IrVerifyError> {
        let Some(owner) = driver_owner_index(self.owner) else {
            return Ok(());
        };
        let step = self
            .store
            .driver_steps
            .get(owner)
            .ok_or_else(|| IrVerifyError::new("driver module slot owner is out of bounds"))?;
        let slots = step
            .slots
            .bounds(self.store.driver_slots.len())
            .ok_or_else(|| IrVerifyError::new("driver module slot range is invalid"))?;
        for binding in &self.store.driver_slots[slots] {
            if binding.flags & DRIVER_SLOT_TEMPORARY != 0 {
                continue;
            }
            let checked_type = self.store.semantic.to_type(binding.type_id)?;
            let Type::Module(module) = &checked_type else {
                continue;
            };
            if !checked_type_contains_typed_callable(&checked_type) {
                continue;
            }
            let name = Name::intern(self.store.string(binding.name)?);
            let Some(registered) = registered_module_types.get(&name) else {
                continue;
            };
            let registered = match registered {
                RegisteredModuleType::Missing => {
                    return Err(IrVerifyError::new(
                        "exact module binding has no registered result type",
                    ));
                }
                RegisteredModuleType::Present(type_id) => *type_id,
                RegisteredModuleType::Ambiguous => {
                    return Err(IrVerifyError::new(
                        "module binding has multiple registered imports",
                    ));
                }
            };
            if self.store.semantic.to_type(registered)? != checked_type || !module.exact {
                return Err(IrVerifyError::new(
                    "driver module slot differs from its registered import result",
                ));
            }
        }
        Ok(())
    }

    fn verify_registered_module_type(
        &self,
        module_type_id: TypeId,
        namespace: Name,
        exports: &[LoweredModuleExport],
        child: u32,
    ) -> Result<(), IrVerifyError> {
        let Type::Module(module) = self.store.semantic.to_type(module_type_id)? else {
            return Err(IrVerifyError::new(
                "registered import result is not a module type",
            ));
        };
        if !module.exact || module.exports.len() != exports.len() {
            return Err(IrVerifyError::new(
                "registered module type differs from its exact export set",
            ));
        }
        let mut names = BTreeSet::new();
        for export in exports {
            if !names.insert(export.name) {
                return Err(IrVerifyError::new(
                    "registered module export name is duplicated",
                ));
            }
            let Some(checked) = module.exports.get(&export.name) else {
                return Err(IrVerifyError::new(
                    "registered module type omits an exported member",
                ));
            };
            match (export.kind, checked) {
                (
                    LoweredModuleExportKind::Value,
                    ModuleExportType::Value { ty, optional: false },
                ) => {
                    if ty.contains_typed_callable() {
                        self.verify_registered_module_value(child, export.name, ty)?;
                    }
                }
                (
                    LoweredModuleExportKind::Pure,
                    ModuleExportType::Pure { sig, optional: false },
                )
                | (
                    LoweredModuleExportKind::Proc,
                    ModuleExportType::Proc { sig, optional: false },
                ) => {
                    let function_namespace = export.function_namespace.unwrap_or(namespace);
                    let signature = self.registered_module_function_signature(
                        function_namespace,
                        export.name,
                        export.kind == LoweredModuleExportKind::Pure,
                    )?;
                    if !self.store.semantic.signature_matches_callable(signature, sig)? {
                        return Err(IrVerifyError::new(
                            "registered module callable type differs from its child function",
                        ));
                    }
                }
                _ => {
                    return Err(IrVerifyError::new(
                        "registered module export kind differs from its exact type",
                    ));
                }
            }
        }
        Ok(())
    }

    fn verify_registered_module_value(
        &self,
        child: u32,
        name: Name,
        expected: &Type,
    ) -> Result<(), IrVerifyError> {
        let program_index = child
            .checked_sub(1)
            .map(|index| index as usize)
            .filter(|index| *index < self.store.driver_programs.len())
            .ok_or_else(|| IrVerifyError::new("module child program is out of bounds"))?;
        let steps = self.store.driver_programs[program_index]
            .steps
            .bounds(self.store.driver_steps.len())
            .ok_or_else(|| IrVerifyError::new("module child step range is invalid"))?;
        let mut found = None;
        for step_index in steps {
            let step = self.store.driver_steps[step_index];
            if step.tag != FullDriverTag::Let {
                continue;
            }
            let instruction_range = self.store.driver_instruction_range(step_index)?;
            let decoder = FullDecoder {
                store: self.store,
                owner: driver_owner(step_index)
                    .map_err(|_| IrVerifyError::new("module child owner is invalid"))?,
                instruction_states: None,
                instruction_proof: None,
                instruction_range,
                block_states: None,
                slot_count: step.slot_count,
                pattern_ceiling: Cell::new(usize::MAX),
                local_immutable_let_initializers: RefCell::default(),
                function_signature_index: self.function_signature_index.clone(),
                verified: false,
            };
            let mut payload = decoder.cursor(self.store.payload(step.data.range())?);
            if Name::decode(&decoder, &mut payload)? != name {
                continue;
            }
            Option::<LoweredType>::decode(&decoder, &mut payload)?;
            Option::<LoweredTypeCheck>::decode(&decoder, &mut payload)?;
            bool::decode(&decoder, &mut payload)?;
            let value = payload.raw()? as usize;
            if decoder.store.tags.get(value).is_none_or(|tag| {
                tag.kind() != schema::InstructionKind::Expr
                    || !decoder.instruction_range.contains(&value)
            }) {
                return Err(IrVerifyError::new(
                    "module value export initializer belongs to another driver step",
                ));
            }
            if found.replace(()).is_some() {
                return Err(IrVerifyError::new(
                    "module value export has multiple child declarations",
                ));
            }
            decoder.checked_expression_matches_type(value, expected)?;
        }
        found.ok_or_else(|| {
            IrVerifyError::new("module value export has no child declaration")
        })
    }

    fn registered_module_function_signature(
        &self,
        namespace: Name,
        name: Name,
        pure: bool,
    ) -> Result<SignatureId, IrVerifyError> {
        self.function_signature_index
            .as_ref()
            .ok_or_else(|| IrVerifyError::new("function signature index is missing"))?
            .module_export(namespace, name, pure)
    }

    fn verify_temporary_driver_slot_types(&self) -> Result<(), IrVerifyError> {
        let Some(owner) = driver_owner_index(self.owner) else {
            return Ok(());
        };
        let step = self
            .store
            .driver_steps
            .get(owner)
            .ok_or_else(|| IrVerifyError::new("typed call driver owner is out of bounds"))?;
        let slots = step
            .slots
            .bounds(self.store.driver_slots.len())
            .ok_or_else(|| IrVerifyError::new("typed callee driver slot range is invalid"))?;
        for slot in self.store.driver_slots[slots]
            .iter()
            .filter(|slot| slot.flags & DRIVER_SLOT_TEMPORARY != 0)
        {
            let slot_index = slot.slot as usize;
            let expected = self.store.semantic.to_type(slot.type_id)?;
            let mut initializer = None;
            for instruction in self.instruction_range.clone() {
                if self.store.tags[instruction] == FullTag::ExprMatch {
                    let schema::ExprView::MatchExpr { value, mut arms, .. } =
                        self.expression_view(instruction)?
                    else {
                        unreachable!("expression tag and generated view disagree")
                    };
                    for arm in &mut arms {
                        let (pattern, guard, _) = arm?;
                        let pattern_id = pattern.raw();
                        let pattern = self.checked_pattern_view(pattern_id)?;
                        match pattern {
                            schema::PatternView::Bind { slot: bound }
                                if bound == slot_index && guard.is_none() =>
                            {
                                if initializer.replace(value.raw() as usize).is_some() {
                                    return Err(IrVerifyError::new(
                                        "temporary driver slot has multiple structural initializers",
                                    ));
                                }
                            }
                            _ if self
                                .pattern_capture_slots(pattern_id as usize)?
                                .contains(&slot_index) =>
                            {
                                return Err(IrVerifyError::new(
                                    "temporary driver slot initializer pattern is not a direct bind",
                                ));
                            }
                            _ => {}
                        }
                    }
                } else if self.store.tags[instruction].kind() == schema::InstructionKind::Stmt
                    && self.statement_writes_slot(instruction, slot_index)?
                {
                    return Err(IrVerifyError::new(
                        "temporary driver slot has a statement writer",
                    ));
                }
            }
            let initializer = initializer.ok_or_else(|| {
                IrVerifyError::new("temporary driver slot has no structural initializer")
            })?;
            let result = if matches!(expected, Type::Module(_)) {
                let actual = self
                    .expression_checked_type_id(initializer)?
                    .ok_or_else(|| {
                        IrVerifyError::new(
                            "temporary module receiver has no checked source type",
                        )
                    })?;
                if self.store.semantic.to_type(actual)? == expected {
                    Ok(())
                } else {
                    Err(IrVerifyError::new(
                        "temporary module receiver differs from its exact source type",
                    ))
                }
            } else {
                self.checked_expression_matches_type(initializer, &expected)
            };
            result.map_err(|_| {
                    IrVerifyError::new(
                        "temporary driver slot type fact disagrees with its MatchExpr initializer",
                    )
                })?;
        }
        Ok(())
    }

    fn checked_expression_type_id(
        &self,
        instruction: usize,
    ) -> Result<Option<TypeId>, IrVerifyError> {
        let instruction = u32::try_from(instruction)
            .map_err(|_| IrVerifyError::new("checked expression instruction is out of bounds"))?;
        Ok(self
            .store
            .checked_expr_types
            .binary_search_by_key(&instruction, |fact| fact.instruction)
            .ok()
            .map(|index| self.store.checked_expr_types[index].type_id))
    }

    fn instruction_parent_of(&self, instruction: usize) -> Result<Option<usize>, IrVerifyError> {
        let proof = self
            .instruction_proof
            .as_ref()
            .ok_or_else(|| IrVerifyError::new("full IR instruction parent proof is missing"))?;
        Ok(proof.borrow().parent_of(instruction))
    }

    fn checked_with_error_source_instructions(&self) -> Result<BTreeSet<usize>, IrVerifyError> {
        let mut initializer_roots = BTreeSet::new();
        for instruction in self.instruction_range.clone() {
            if self.store.tags[instruction] != FullTag::StmtWith {
                continue;
            }
            let schema::StmtView::With {
                mut bindings, ..
            } = self.statement_view(instruction)?
            else {
                unreachable!("With instruction tag and generated view disagree")
            };
            for binding in &mut bindings {
                let (_, value) = binding?;
                initializer_roots.insert(value.raw() as usize);
            }
        }
        let mut sources = initializer_roots.clone();
        for instruction in self.instruction_range.clone() {
            if self.store.tags[instruction] != FullTag::ExprTry {
                continue;
            }
            let schema::ExprView::Try { value, .. } = self.expression_view(instruction)? else {
                unreachable!("Try instruction tag and generated view disagree")
            };
            let mut current = instruction;
            loop {
                if initializer_roots.contains(&current) {
                    sources.insert(value.raw() as usize);
                    break;
                }
                let Some(parent) = self.instruction_parent_of(current)? else {
                    break;
                };
                current = parent;
            }
        }
        Ok(sources)
    }

    fn function_reference_signature(
        &self,
        function: decode::FunctionNameView,
    ) -> Result<(SignatureId, bool), IrVerifyError> {
        let (name, namespace) = match function {
            decode::FunctionNameView::Name(name) => (name, None),
            decode::FunctionNameView::Qualified(name) => (name.member, Some(name.namespace)),
        };
        let index = self
            .function_signature_index
            .as_ref()
            .ok_or_else(|| IrVerifyError::new("function signature index is missing"))?;
        let lookup = index.functions.get(&(namespace, name));
        let result = match lookup {
            Some(signatures) => match (&signatures.pure, &signatures.proc) {
                (FunctionSignatureLookup::Unique(signature), FunctionSignatureLookup::Missing) => {
                    Some((*signature, true))
                }
                (FunctionSignatureLookup::Missing, FunctionSignatureLookup::Unique(signature)) => {
                    Some((*signature, false))
                }
                (FunctionSignatureLookup::Missing, FunctionSignatureLookup::Missing) => None,
                _ => {
                    return Err(IrVerifyError::new(
                        "function reference target is ambiguous in the checked program",
                    ));
                }
            },
            None => None,
        };
        result.ok_or_else(|| {
            let target = match namespace {
                Some(namespace) => QualifiedName::new(namespace, name).to_string(),
                None => name.to_string(),
            };
            IrVerifyError::new(format!(
                "function reference `{target}` has no checked signature"
            ))
        })
    }

    fn require_checked_child_type(
        &self,
        child: usize,
        expected: &Type,
    ) -> Result<(), IrVerifyError> {
        self.checked_expression_matches_type(child, expected)
    }

    fn checked_block_tail_matches_type(
        &self,
        mut statements: ListView<'a, BuildStmtId>,
        expected: &Type,
    ) -> Result<(), IrVerifyError> {
        let value = self.checked_block_tail_expression(&mut statements)?;
        self.checked_expression_matches_type(value, expected)
    }

    fn checked_block_tail_expression(
        &self,
        statements: &mut ListView<'a, BuildStmtId>,
    ) -> Result<usize, IrVerifyError> {
        let mut tail = None;
        for statement in statements {
            tail = Some(statement?.raw() as usize);
        }
        let Some(instruction) = tail else {
            return Err(IrVerifyError::new("checked value block has no value tail"));
        };
        match self.statement_view(instruction)? {
            schema::StmtView::Value { value } => Ok(value.raw() as usize),
            _ => Err(IrVerifyError::new("checked value block has no value tail")),
        }
    }

    fn checked_record_entries(
        &self,
        entries: &mut ListView<'a, LoweredRecordEntry>,
    ) -> Result<Vec<CheckedRecordEntry>, IrVerifyError> {
        let mut checked = Vec::with_capacity(entries.len());
        for entry in entries.by_ref() {
            checked.push(match entry? {
                decode::RecordEntryView::Field(name, value) => {
                    CheckedRecordEntry::Field(name, value.raw() as usize)
                }
                decode::RecordEntryView::Spread(value) => {
                    CheckedRecordEntry::Spread(value.raw() as usize)
                }
            });
        }
        Ok(checked)
    }

    fn checked_record_update_matches_type(
        &self,
        base: usize,
        mut updates: ListView<'a, (Vec<Name>, BuildExprId, Span)>,
        expected: &Type,
    ) -> Result<(), IrVerifyError> {
        let Type::Record(_) = expected else {
            return Err(IrVerifyError::new(
                "checked record update has a non-record result type",
            ));
        };
        self.checked_expression_matches_type(base, expected)?;
        for update in &mut updates {
            let (path, value, _) = update?;
            let mut target_type = expected;
            for name in path {
                let name = name?;
                let Type::Record(target_fields) = target_type else {
                    return Err(IrVerifyError::new(
                        "checked record update path crosses a non-record field",
                    ));
                };
                target_type = target_fields.get(&name).ok_or_else(|| {
                    IrVerifyError::new(
                        "checked record update field is absent from its result type",
                    )
                })?;
            }
            if target_type.contains_typed_callable() {
                self.checked_expression_matches_type(value.raw() as usize, target_type)
                    .map_err(|_| {
                        IrVerifyError::new(
                            "record update value disagrees with its checked callable field",
                        )
                    })?;
            }
        }
        Ok(())
    }

    fn checked_pattern_view<'e>(
        &'e self,
        pattern: u32,
    ) -> Result<schema::PatternView<'e>, IrVerifyError> {
        let index = pattern as usize;
        let tag = *self
            .store
            .patterns
            .get(index)
            .ok_or_else(|| IrVerifyError::new("checked pattern is out of bounds"))?;
        let data = self
            .store
            .pattern_data
            .get(index)
            .ok_or_else(|| IrVerifyError::new("checked pattern payload is missing"))?;
        let payload = FullPayload {
            store: self.store,
            cursor: self.cursor(self.store.payload(data.range())?),
        };
        schema::PatternView::decode_from_decoder(self, tag, payload)
    }

    fn checked_match_result_matches_type(
        &self,
        subject: u32,
        mut arms: ListView<'a, (BuildPatternId, Option<BuildExprId>, BuildExprId)>,
        expected: &Type,
    ) -> Result<(), IrVerifyError> {
        let mut arm_values = Vec::new();
        let mut null_branch = None;
        let mut null_pattern_count = 0;
        let mut bound_value_count = 0;
        let mut optional_bind_shape = true;
        for arm in &mut arms {
            let (pattern, guard, value) = arm?;
            let value = value.raw() as usize;
            arm_values.push(value);
            match self.checked_pattern_view(pattern.raw())? {
                schema::PatternView::Literal(literal)
                    if literal.materialize()? == LoweredValue::Null && guard.is_none() =>
                {
                    null_pattern_count += 1;
                    null_branch = Some(value);
                }
                schema::PatternView::Bind { slot } if guard.is_none() => {
                    if matches!(
                        self.expression_view(value)?,
                        schema::ExprView::Param(value_slot) if value_slot == slot
                    ) {
                        bound_value_count += 1;
                    } else {
                        optional_bind_shape = false;
                    }
                }
                _ => optional_bind_shape = false,
            }
        }
        if optional_bind_shape
            && arm_values.len() == 2
            && null_pattern_count == 1
            && bound_value_count == 1
        {
            let subject = self
                .checked_expression_type(subject as usize)?
                .ok_or_else(|| {
                    IrVerifyError::new("checked optional binding match has no subject type")
                })?;
            if !matches!(subject, Type::Optional(inner) if *inner == *expected) {
                return Err(IrVerifyError::new(
                    "checked optional binding result disagrees with its subject type",
                ));
            }
            return self.checked_expression_matches_type(
                null_branch.expect("one null arm was counted"),
                expected,
            );
        }
        for value in arm_values {
            self.checked_expression_matches_type(value, expected)?;
        }
        Ok(())
    }

    fn comp_target_view_writes_slot(
        &self,
        target: decode::CompTargetView<'a>,
        slot: usize,
    ) -> Result<bool, IrVerifyError> {
        match target {
            decode::CompTargetView::Discard => Ok(false),
            decode::CompTargetView::Slot(written) => Ok(written == slot),
            decode::CompTargetView::Record { mut fields } => {
                for field in &mut fields {
                    let (_, target, _) = field?;
                    if self.comp_target_view_writes_slot(target, slot)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }

    fn statement_writes_slot(
        &self,
        instruction: usize,
        slot: usize,
    ) -> Result<bool, IrVerifyError> {
        Ok(match self.statement_view(instruction)? {
            schema::StmtView::Let { slot: written, .. }
            | schema::StmtView::LetInt { slot: written, .. }
            | schema::StmtView::LetBool { slot: written, .. }
            | schema::StmtView::Assign { slot: written, .. }
            | schema::StmtView::AssignInt { slot: written, .. }
            | schema::StmtView::AssignBool { slot: written, .. }
            | schema::StmtView::AssignField { slot: written, .. }
            | schema::StmtView::AssignFieldInt { slot: written, .. }
            | schema::StmtView::AssignPath { slot: written, .. }
            | schema::StmtView::DefaultParameter { slot: written, .. } => written == slot,
            schema::StmtView::Guard {
                target,
                else_param_slot,
                ..
            } => self.comp_target_view_writes_slot(target, slot)? || else_param_slot == Some(slot),
            schema::StmtView::With {
                mut bindings,
                else_param_slot,
                mut captures,
                ..
            } => {
                let mut writes = else_param_slot == Some(slot);
                for binding in &mut bindings {
                    let (written, _) = binding?;
                    writes |= written == slot;
                }
                for capture in &mut captures {
                    writes |= capture? == slot;
                }
                writes
            }
            schema::StmtView::PatternIf { mut branches, .. } => {
                let mut writes = false;
                for branch in &mut branches {
                    let (_, _, captures) = branch?;
                    for capture in captures {
                        writes |= capture? == slot;
                    }
                }
                writes
            }
            schema::StmtView::PatternWhile { mut captures, .. } => {
                let mut writes = false;
                for capture in &mut captures {
                    writes |= capture? == slot;
                }
                writes
            }
            schema::StmtView::Match { mut arms, .. } => {
                let mut writes = false;
                for arm in &mut arms {
                    let (pattern, _, _) = arm?;
                    writes |= self
                        .pattern_capture_slots(pattern.raw() as usize)?
                        .contains(&slot);
                }
                writes
            }
            schema::StmtView::For { slot: written, .. }
            | schema::StmtView::ForStrLines { slot: written, .. } => written == slot,
            schema::StmtView::ForRecord { target, .. }
            | schema::StmtView::LetRecord { target, .. } => {
                self.comp_target_view_writes_slot(target, slot)?
            }
            schema::StmtView::ScanLines {
                text_slot,
                line_slot,
                ..
            } => text_slot == slot || line_slot == slot,
            _ => false,
        })
    }

    /// Use a local handle's target only when its slot has one immutable
    /// function-reference initializer; every lowered slot write is checked.
    fn local_function_ref_initializer(
        &self,
        slot: usize,
    ) -> Result<Option<usize>, IrVerifyError> {
        let (_, initializer) = self.local_immutable_let_initializer(slot)?;
        let Some(initializer) = initializer else {
            return Ok(None);
        };
        Ok(matches!(
            self.expression_view(initializer)?,
            schema::ExprView::FunctionRef { .. }
        )
        .then_some(initializer))
    }

    fn checked_expression_matches_type(
        &self,
        instruction: usize,
        expected: &Type,
    ) -> Result<(), IrVerifyError> {
        if !expected.contains_typed_callable()
            && self.checked_expression_type_id(instruction)?.is_none()
        {
            return Ok(());
        }
        if let schema::ExprView::Param(slot) = self.expression_view(instruction)? {
            let actual = if let Some(actual) = self.checked_expression_type_id(instruction)? {
                Some(self.store.semantic.to_type(actual)?)
            } else {
                match self.checked_slot_type(slot) {
                    Ok(Some(actual)) => Some(self.store.semantic.to_type(actual)?),
                    Ok(None) => None,
                    Err(error)
                        if error
                            .message
                            .starts_with("typed callee slot has no checked type fact:") =>
                    {
                        None
                    }
                    Err(error) => return Err(error),
                }
            };
            if actual.as_ref() == Some(expected) {
                return Ok(());
            }
            if actual
                .as_ref()
                .is_some_and(|actual| self.checked_type_fits(actual, expected))
            {
                return Ok(());
            }
            if let Type::Callable(callable) = expected
                && actual.as_ref().is_none_or(|actual| {
                    matches!(
                        (actual, callable.pure),
                        (Type::Pure, true) | (Type::Proc, false)
                    )
                })
            {
                return self.local_function_ref_matches_type(slot, expected);
            }
            return Err(IrVerifyError::new(
                "checked callable expression slot disagrees with its expected type",
            ));
        }
        if let Some(actual) = self.expression_checked_type_id(instruction)? {
            let actual_type = self.store.semantic.to_type(actual)?;
            if actual_type == *expected || self.checked_type_fits(&actual_type, expected) {
                return Ok(());
            }
            if let schema::ExprView::Param(slot) = self.expression_view(instruction)?
                && let Type::Callable(callable) = expected
                && matches!(
                    (actual_type, callable.pure),
                    (Type::Pure, true) | (Type::Proc, false)
                )
                && let Some(initializer) = self.local_function_ref_initializer(slot)?
            {
                return self.checked_expression_matches_type(initializer, expected);
            }
            return Err(IrVerifyError::new(
                "checked expression child type disagrees with its producer",
            ));
        }
        match self.expression_view(instruction)? {
            schema::ExprView::Param(slot) => {
                let actual = match self.checked_slot_type(slot) {
                    Ok(Some(actual)) => Some(self.store.semantic.to_type(actual)?),
                    Ok(None) => None,
                    Err(error)
                        if error
                            .message
                            .starts_with("typed callee slot has no checked type fact:") =>
                    {
                        None
                    }
                    Err(error) => return Err(error),
                };
                if actual.as_ref() == Some(expected) {
                    Ok(())
                } else if let Type::Callable(callable) = expected
                    && actual.as_ref().is_none_or(|actual| {
                        matches!(
                            (actual, callable.pure),
                            (Type::Pure, true) | (Type::Proc, false)
                        )
                    })
                {
                    self.local_function_ref_matches_type(slot, expected)
                } else {
                    Err(IrVerifyError::new(
                        "checked callable expression slot disagrees with its expected type",
                    ))
                }
            }
            schema::ExprView::FunctionRef { function, pure } => {
                let (signature, target_pure) = self.function_reference_signature(function)?;
                let Type::Callable(callable) = expected else {
                    return Err(IrVerifyError::new(
                        "checked function reference tail has a non-callable expected type",
                    ));
                };
                if target_pure != pure
                    || callable.pure != pure
                    || !self
                        .store
                        .semantic
                        .signature_fits_callable(signature, pure, callable)?
                {
                    return Err(IrVerifyError::new(
                        "checked function reference tail disagrees with its target signature",
                    ));
                }
                Ok(())
            }
            schema::ExprView::Null => {
                if matches!(expected, Type::Optional(_)) {
                    Ok(())
                } else {
                    Err(IrVerifyError::new(
                        "checked null expression has an incompatible expected type",
                    ))
                }
            }
            schema::ExprView::CheckedValue { value, check, .. } => {
                if check.ty.materialize()? != *expected {
                    return Err(IrVerifyError::new(
                        "checked value wrapper disagrees with its checked result type",
                    ));
                }
                self.checked_expression_matches_type(value.raw() as usize, expected)
            }
            schema::ExprView::Try { value, .. } => {
                let value = value.raw() as usize;
                if let schema::ExprView::Require { value: checked, check, .. } = self.expression_view(value)?
                    && check.ty.materialize()? == *expected
                {
                    // A checked record constructor returns its value only after
                    // the runtime require has enforced the declared record type.
                    return self.checked_expression_matches_type(
                        checked.raw() as usize,
                        expected,
                    );
                }
                let Some(actual) = self.expression_checked_type_id(value)? else {
                    return Err(IrVerifyError::new(
                        "checked try producer is missing its result type",
                    ));
                };
                let actual = self.store.semantic.to_type(actual)?;
                let ok = match actual {
                    Type::Result(ok, _) => *ok,
                    Type::Validated(validated) => match validated.base() {
                        Type::Result(ok, _) => ok.as_ref().clone(),
                        _ => {
                            return Err(IrVerifyError::new(
                                "checked try producer operand is not a result",
                            ));
                        }
                    },
                    _ => {
                        return Err(IrVerifyError::new(
                            "checked try producer operand is not a result",
                        ));
                    }
                };
                if ok == *expected {
                    Ok(())
                } else {
                    Err(IrVerifyError::new(
                        "checked try result disagrees with its checked operand",
                    ))
                }
            }
            schema::ExprView::ValueBlock { body, .. } => {
                self.checked_block_tail_matches_type(body, expected)
            }
            schema::ExprView::Record(mut entries) => {
                let Type::Record(fields) = expected else {
                    return Err(IrVerifyError::new(
                        "checked record producer has a non-record expected type",
                    ));
                };
                let entries = self.checked_record_entries(&mut entries)?;
                let mut supplied = BTreeSet::new();
                let mut later_fields = BTreeSet::new();
                for entry in entries.into_iter().rev() {
                    match entry {
                        CheckedRecordEntry::Field(name, value) => {
                            if !later_fields.contains(&name)
                                && let Some(field_type) = fields.get(&name)
                                && field_type.contains_typed_callable()
                            {
                                self.checked_expression_matches_type(value, field_type)?;
                                supplied.insert(name);
                            }
                            later_fields.insert(name);
                        }
                        CheckedRecordEntry::Spread(value) => {
                            let Some(source_type) = self.expression_checked_type_id(value)? else {
                                return Err(IrVerifyError::new(
                                    "checked callable record spread is missing its type",
                                ));
                            };
                            let Type::Record(source_fields) =
                                self.store.semantic.to_type(source_type)?
                            else {
                                return Err(IrVerifyError::new(
                                    "checked callable record spread has no field facts",
                                ));
                            };
                            for (name, field_type) in source_fields {
                                if !later_fields.contains(&name) {
                                    if field_type.contains_typed_callable()
                                        && let Some(expected_field) = fields.get(&name)
                                    {
                                        if !self.checked_type_fits(&field_type, expected_field) {
                                            return Err(IrVerifyError::new(
                                                "checked record spread field disagrees with its result",
                                            ));
                                        }
                                        supplied.insert(name);
                                    }
                                }
                                later_fields.insert(name);
                            }
                        }
                    }
                }
                if fields.iter().any(|(name, field_type)| {
                    field_type.contains_typed_callable() && !supplied.contains(name)
                }) {
                    return Err(IrVerifyError::new(
                        "checked record producer has an unproven callable field",
                    ));
                }
                Ok(())
            }
            schema::ExprView::IfExpr {
                mut branches,
                else_value,
                ..
            } => {
                for branch in &mut branches {
                    let (_, value) = branch?;
                    self.checked_expression_matches_type(value.raw() as usize, expected)?;
                }
                self.checked_expression_matches_type(else_value.raw() as usize, expected)
            }
            schema::ExprView::PatternIf {
                mut branches,
                else_value,
                ..
            } => {
                for branch in &mut branches {
                    let (_, value, _) = branch?;
                    self.checked_expression_matches_type(value.raw() as usize, expected)?;
                }
                self.checked_expression_matches_type(else_value.raw() as usize, expected)
            }
            schema::ExprView::MatchExpr { value, arms, .. } => self
                .checked_match_result_matches_type(value.raw(), arms, expected),
            schema::ExprView::Loop { mut body, .. } => {
                let mut found_value = false;
                for statement in &mut body {
                    let statement = statement?.raw() as usize;
                    if let schema::StmtView::BreakValue { value } =
                        self.statement_view(statement)?
                    {
                        found_value = true;
                        self.checked_expression_matches_type(value.raw() as usize, expected)?;
                    }
                }
                if found_value {
                    Ok(())
                } else {
                    Err(IrVerifyError::new(
                        "checked callable loop has no value-producing break",
                    ))
                }
            }
            _ => {
                Err(IrVerifyError::new(
                    "checked callable expression child has no structural producer proof",
                ))
            }
        }
    }

    fn checked_type_fits(&self, actual: &Type, expected: &Type) -> bool {
        match (actual, expected) {
            (Type::Callable(actual), Type::Callable(expected)) => {
                self.store.semantic.callable_type_fits(actual, expected)
            }
            _ => actual.matches_expected(expected),
        }
    }

    fn function_reference_matches_callable(
        &self,
        instruction: usize,
        expected: &Type,
    ) -> Result<(), IrVerifyError> {
        let schema::ExprView::FunctionRef { function, pure } = self.expression_view(instruction)?
        else {
            return Err(IrVerifyError::new(
                "checked callable initializer is not a function reference",
            ));
        };
        let (signature, target_pure) = self.function_reference_signature(function)?;
        let Type::Callable(callable) = expected else {
            return Err(IrVerifyError::new(
                "checked function reference has a non-callable expected type",
            ));
        };
        if target_pure != pure
            || callable.pure != pure
            || !self
                .store
                .semantic
                .signature_fits_callable(signature, pure, callable)?
        {
            return Err(IrVerifyError::new(
                "checked function reference tail disagrees with its target signature",
            ));
        }
        Ok(())
    }

    fn local_function_ref_matches_type(
        &self,
        slot: usize,
        expected: &Type,
    ) -> Result<(), IrVerifyError> {
        let initializer = self
            .local_function_ref_initializer(slot)?
            .ok_or_else(|| {
                IrVerifyError::new(
                    "checked callable slot has no immutable function reference initializer",
                )
            })?;
        self.function_reference_matches_callable(initializer, expected)
    }

    fn checked_index_result_type(&self, base: usize) -> Result<Type, IrVerifyError> {
        let Some(base_type) = self.expression_checked_type_id(base)? else {
            return Err(IrVerifyError::new(
                "callable index producer is missing its checked base type",
            ));
        };
        let base_type = self.store.semantic.to_type(base_type)?;
        let mut current = base_type;
        let mut optional = false;
        let item = loop {
            match current {
                Type::Optional(inner) => {
                    optional = true;
                    current = *inner;
                }
                Type::Validated(validated) => {
                    current = validated.base().clone();
                }
                Type::List(item) | Type::Set(item) => break *item,
                Type::Map(_, value) => break *value,
                _ => {
                    return Err(IrVerifyError::new(
                        "callable index producer has no checked collection element type",
                    ));
                }
            }
        };
        Ok(if optional {
            Type::Optional(Box::new(item))
        } else {
            item
        })
    }

    fn expression_is_nonempty_list_literal(
        &self,
        instruction: usize,
    ) -> Result<bool, IrVerifyError> {
        match self.expression_view(instruction)? {
            schema::ExprView::List(values) => Ok(values.len() > 0),
            _ => Ok(false),
        }
    }

    fn registered_api_operation_return_type(op: RuntimeOp) -> Result<Type, IrVerifyError> {
        static RETURN_TYPES:
            std::sync::OnceLock<FxHashMap<RuntimeOp, Result<Type, ()>>> =
            std::sync::OnceLock::new();
        let return_types = RETURN_TYPES.get_or_init(|| {
            let mut return_types = FxHashMap::default();
            let mut record = |op, return_type: &Type| {
                match return_types.entry(op) {
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert(Ok(return_type.clone()));
                    }
                    std::collections::hash_map::Entry::Occupied(mut entry) => {
                        if entry
                            .get()
                            .as_ref()
                            .is_ok_and(|previous| previous != return_type)
                        {
                            drop(entry.insert(Err(())));
                        }
                    }
                }
            };
            for (_, module) in crate::modules::api_spec().module_entries() {
                for function in &module.functions {
                    for overload in &function.overloads {
                        record(overload.op, &overload.return_ty);
                    }
                }
            }
            for (_, methods) in crate::modules::api_spec().method_entries() {
                for method in methods {
                    for overload in &method.overloads {
                        record(overload.sig.op, &overload.sig.return_ty);
                    }
                }
            }
            return_types
        });
        match return_types.get(&op) {
            Some(Ok(return_type)) => Ok(return_type.clone()),
            Some(Err(())) => Err(IrVerifyError::new(
                "checked API operation has ambiguous return types",
            )),
            None => Err(IrVerifyError::new(
                "checked API operation has no registry signature",
            )),
        }
    }

    fn registered_path_method_return_type(
        name: &str,
        op: RuntimeOp,
    ) -> Result<Type, IrVerifyError> {
        let mut return_type = None;
        for overload in crate::modules::api_spec()
            .method_overloads(crate::modules::MethodReceiver::Path, name)
            .unwrap_or_default()
        {
            if overload.sig.op != op {
                continue;
            }
            match &return_type {
                Some(previous) if previous != &overload.sig.return_ty => {
                    return Err(IrVerifyError::new(
                        "checked path method has ambiguous return types",
                    ));
                }
                Some(_) => {}
                None => return_type = Some(overload.sig.return_ty.clone()),
            }
        }
        return_type.ok_or_else(|| {
            IrVerifyError::new("checked path method has no registry signature")
        })
    }

    fn registered_path_method_type(name: &str) -> Result<Type, IrVerifyError> {
        let overloads = crate::modules::api_spec()
            .method_overloads(crate::modules::MethodReceiver::Path, name)
            .ok_or_else(|| IrVerifyError::new("checked path method has no registry signature"))?;
        let mut return_type = None;
        for overload in overloads {
            match &return_type {
                Some(previous) if previous != &overload.sig.return_ty => {
                    return Err(IrVerifyError::new(
                        "checked path method has ambiguous return types",
                    ));
                }
                Some(_) => {}
                None => return_type = Some(overload.sig.return_ty.clone()),
            }
        }
        return_type.ok_or_else(|| {
            IrVerifyError::new("checked path method has no registry signature")
        })
    }

    fn derive_checked_expression_type(
        &self,
        instruction: usize,
    ) -> Result<Option<Type>, IrVerifyError> {
        if let Some(ty) = self.checked_expression_type(instruction)? {
            return Ok(Some(ty));
        }
        match self.expression_view(instruction)? {
            schema::ExprView::Param(slot) => {
                let (has_local_write, initializer) = if driver_owner_index(self.owner).is_none() {
                    self.local_immutable_let_initializer(slot)?
                } else {
                    (false, None)
                };
                if has_local_write {
                    return match initializer {
                        Some(initializer) => self.derive_checked_expression_type(initializer),
                        None => Ok(None),
                    };
                }
                let Some(type_id) = self.checked_slot_type(slot)? else {
                    return Ok(None);
                };
                self.store.semantic.to_type(type_id).map(Some)
            }
            schema::ExprView::Path(_)
            | schema::ExprView::PathFrom { .. }
            | schema::ExprView::PathFmtString { .. } => Ok(Some(Type::Path)),
            schema::ExprView::Record(_) => self.derive_checked_record_type(instruction),
            schema::ExprView::RecordUpdate {
                base,
                mut updates,
                ..
            } => {
                let Some(base_type) =
                    self.derive_checked_expression_type(base.raw() as usize)?
                else {
                    return Ok(None);
                };
                let Type::Record(mut fields) = base_type.into_unvalidated() else {
                    return Ok(None);
                };
                for update in &mut updates {
                    let (mut path, value, _) = update?;
                    let path = path.by_ref().collect::<Result<Vec<_>, _>>()?;
                    let Some(value_type) =
                        self.derive_checked_expression_type(value.raw() as usize)?
                    else {
                        return Ok(None);
                    };
                    if !Self::update_derived_record_type(&mut fields, &path, value_type) {
                        return Ok(None);
                    }
                }
                Ok(Some(Type::Record(fields)))
            }
            schema::ExprView::Require { check, .. } => Ok(Some(Type::Result(
                Box::new(check.ty.materialize()?),
                Box::new(Type::Error),
            ))),
            schema::ExprView::ValueBlock { mut body, .. } => {
                let value = self.checked_block_tail_expression(&mut body)?;
                self.derive_checked_expression_type(value)
            }
            schema::ExprView::Method { receiver, name, .. } => {
                let Some(receiver_type) =
                    self.derive_checked_expression_type(receiver.raw() as usize)?
                else {
                    return Ok(None);
                };
                if receiver_type.into_unvalidated() != Type::Path {
                    return Ok(None);
                }
                Self::registered_path_method_type(name).map(Some)
            }
            schema::ExprView::ModuleCall { op, .. } => {
                Self::registered_api_operation_return_type(op).map(Some)
            }
            schema::ExprView::FsFiles {
                result_wrapped, ..
            } => {
                let registered =
                    Self::registered_api_operation_return_type(RuntimeOp::FsFiles)?;
                let result = if result_wrapped {
                    registered
                } else {
                    match registered {
                        Type::Result(ok, _) => *ok,
                        _ => {
                            return Err(IrVerifyError::new(
                                "filesystem file scan registry signature is not a result",
                            ));
                        }
                    }
                };
                Ok(Some(result))
            }
            schema::ExprView::FsWalk {
                result_wrapped, ..
            } => {
                let registered =
                    Self::registered_api_operation_return_type(RuntimeOp::FsWalk)?;
                let result = if result_wrapped {
                    registered
                } else {
                    match registered {
                        Type::Result(ok, _) => *ok,
                        _ => {
                            return Err(IrVerifyError::new(
                                "filesystem walk registry signature is not a result",
                            ));
                        }
                    }
                };
                Ok(Some(result))
            }
            schema::ExprView::FsList { op, .. } => {
                Self::registered_api_operation_return_type(op).map(Some)
            }
            schema::ExprView::FsMkdir { .. } => {
                Self::registered_api_operation_return_type(RuntimeOp::FsMkdir).map(Some)
            }
            schema::ExprView::FsRemove { .. } => {
                Self::registered_api_operation_return_type(RuntimeOp::FsRemove).map(Some)
            }
            schema::ExprView::FsTempDir { .. } => {
                Self::registered_api_operation_return_type(RuntimeOp::FsTempDir).map(Some)
            }
            schema::ExprView::ArchiveTarCreate { .. } => {
                Self::registered_api_operation_return_type(RuntimeOp::ArchiveTarCreate).map(Some)
            }
            schema::ExprView::PathReadText { .. } => Self::registered_path_method_return_type(
                "read_text",
                RuntimeOp::FsReadText,
            )
            .map(Some),
            schema::ExprView::PathReadBytes { .. } => Self::registered_path_method_return_type(
                "read_bytes",
                RuntimeOp::FsRead,
            )
            .map(Some),
            schema::ExprView::PathExists { .. } => Self::registered_path_method_return_type(
                "exists",
                RuntimeOp::FsExists,
            )
            .map(Some),
            schema::ExprView::PathExecutable { .. } => Self::registered_path_method_return_type(
                "executable",
                RuntimeOp::FsExecutable,
            )
            .map(Some),
            schema::ExprView::PathDu { .. } => {
                Self::registered_path_method_return_type("du", RuntimeOp::FsDu).map(Some)
            }
            schema::ExprView::PathMetadata { .. } => Self::registered_path_method_return_type(
                "metadata",
                RuntimeOp::FsMetadata,
            )
            .map(Some),
            schema::ExprView::PathWrite { atomic, .. } => {
                let (name, op) = if atomic {
                    ("write_atomic", RuntimeOp::FsWriteAtomic)
                } else {
                    ("write", RuntimeOp::FsWrite)
                };
                Self::registered_path_method_return_type(name, op).map(Some)
            }
            schema::ExprView::PathMkdir { .. } => {
                Self::registered_path_method_return_type("mkdir", RuntimeOp::FsMkdir).map(Some)
            }
            schema::ExprView::PathRemove { .. } => {
                Self::registered_path_method_return_type("remove", RuntimeOp::FsRemove).map(Some)
            }
            schema::ExprView::PathReadlink { .. } => {
                Self::registered_path_method_return_type("readlink", RuntimeOp::FsReadlink)
                    .map(Some)
            }
            schema::ExprView::PathResolve { .. } => {
                Self::registered_path_method_return_type("resolve", RuntimeOp::PathResolve)
                    .map(Some)
            }
            schema::ExprView::Try { value, .. } => {
                let Some(value_type) =
                    self.derive_checked_expression_type(value.raw() as usize)?
                else {
                    return Ok(None);
                };
                match value_type.into_unvalidated() {
                    Type::Result(ok, _) => Ok(Some(*ok)),
                    _ => Ok(None),
                }
            }
            schema::ExprView::IfExpr {
                mut branches,
                else_value,
                ..
            } => {
                for branch in &mut branches {
                    let (_, value) = branch?;
                    if !self.checked_path_value(value.raw() as usize)? {
                        return Ok(None);
                    }
                }
                if self.checked_path_value(else_value.raw() as usize)? {
                    Ok(Some(Type::Path))
                } else {
                    Ok(None)
                }
            }
            schema::ExprView::PatternIf {
                mut branches,
                else_value,
                ..
            } => {
                for branch in &mut branches {
                    let (_, value, _) = branch?;
                    if !self.checked_path_value(value.raw() as usize)? {
                        return Ok(None);
                    }
                }
                if self.checked_path_value(else_value.raw() as usize)? {
                    Ok(Some(Type::Path))
                } else {
                    Ok(None)
                }
            }
            schema::ExprView::MatchExpr {
                value: subject,
                mut arms,
                ..
            } => {
                let subject_type =
                    self.derive_checked_expression_type(subject.raw() as usize)?;
                let mut checked_arms = Vec::with_capacity(arms.len());
                for arm in &mut arms {
                    let (pattern, guard, value) = arm?;
                    checked_arms.push((pattern.raw(), guard.is_some(), value.raw() as usize));
                }
                if checked_arms.is_empty() {
                    return Ok(None);
                }
                let optional_payload_slot = if checked_arms.len() == 2
                    && let Some(Type::Optional(payload)) = subject_type.as_ref()
                {
                    let mut null_arm = false;
                    let mut bind_slot = None;
                    for (pattern, has_guard, _) in &checked_arms {
                        if *has_guard {
                            null_arm = false;
                            bind_slot = None;
                            break;
                        }
                        match self.checked_pattern_view(*pattern)? {
                            schema::PatternView::Literal(value)
                                if value.materialize()? == LoweredValue::Null =>
                            {
                                null_arm = true;
                            }
                            schema::PatternView::Bind { slot } => bind_slot = Some(slot),
                            _ => {
                                null_arm = false;
                                bind_slot = None;
                                break;
                            }
                        }
                    }
                    null_arm.then(|| bind_slot.map(|slot| (slot, payload.as_ref().clone())))
                        .flatten()
                } else {
                    None
                };
                for (pattern, _, arm_value) in &checked_arms {
                    if self.checked_path_value(*arm_value)? {
                        continue;
                    }
                    let schema::ExprView::Param(slot) = self.expression_view(*arm_value)? else {
                        return Ok(None);
                    };
                    let captured_type = optional_payload_slot
                        .as_ref()
                        .filter(|(bound, _)| *bound == slot)
                        .map(|(_, ty)| ty.clone())
                        .map_or_else(
                            || {
                                self.checked_pattern_bound_type(
                                    *pattern,
                                    slot,
                                    subject_type.as_ref(),
                                )
                            },
                            |ty| Ok(Some(ty)),
                        )?;
                    if captured_type
                        .is_none_or(|ty| ty.into_unvalidated() != Type::Path)
                    {
                        return Ok(None);
                    }
                }
                Ok(Some(Type::Path))
            }
            schema::ExprView::StrMatchExpr {
                mut arms,
                fallback,
                ..
            }
            | schema::ExprView::TagMatchExpr {
                mut arms,
                fallback,
                ..
            } => {
                let mut saw_arm = false;
                for arm in &mut arms {
                    let (_, value) = arm?;
                    saw_arm = true;
                    if !self.checked_path_value(value.raw() as usize)? {
                        return Ok(None);
                    }
                }
                if let Some(fallback) = fallback {
                    saw_arm = true;
                    if !self.checked_path_value(fallback.raw() as usize)? {
                        return Ok(None);
                    }
                }
                if saw_arm {
                    Ok(Some(Type::Path))
                } else {
                    Ok(None)
                }
            }
            schema::ExprView::ResultFallback { left, right } => {
                let Some(left_type) =
                    self.derive_checked_expression_type(left.raw() as usize)?
                else {
                    return Ok(None);
                };
                let fallback_type = match left_type.into_unvalidated() {
                    Type::Optional(value) | Type::Result(value, _) => *value,
                    _ => return Ok(None),
                };
                if fallback_type.into_unvalidated() == Type::Path
                    && self.checked_path_value(right.raw() as usize)?
                {
                    Ok(Some(Type::Path))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    fn checked_path_value(&self, instruction: usize) -> Result<bool, IrVerifyError> {
        let ty = self.derive_checked_expression_type(instruction)?;
        Ok(ty.is_some_and(|ty| ty.into_unvalidated() == Type::Path))
    }

    fn checked_pattern_bound_type(
        &self,
        root: u32,
        slot: usize,
        subject_type: Option<&Type>,
    ) -> Result<Option<Type>, IrVerifyError> {
        let Some(subject_type) = subject_type else {
            return Ok(None);
        };
        let mut pending = vec![(root, subject_type.clone())];
        while let Some((pattern, subject_type)) = pending.pop() {
            match self.checked_pattern_view(pattern)? {
                schema::PatternView::Bind { slot: bound } if bound == slot => {
                    return Ok(Some(subject_type));
                }
                schema::PatternView::Alias {
                    pattern: inner,
                    slot: bound,
                } => {
                    if bound == slot {
                        return Ok(Some(subject_type.clone()));
                    }
                    pending.push((inner.raw(), subject_type));
                }
                schema::PatternView::ResultTest { ok, inner } => {
                    if let Some(payload) =
                        Self::checked_result_payload_type(Some(&subject_type), ok)
                    {
                        pending.push((inner.raw(), payload));
                    }
                }
                schema::PatternView::ResultOk {
                    slot: Some(bound),
                    ..
                } if bound == slot => {
                    return Ok(Self::checked_result_payload_type(Some(&subject_type), true));
                }
                schema::PatternView::ResultErr {
                    slot: Some(bound),
                    ..
                } if bound == slot => {
                    return Ok(Self::checked_result_payload_type(Some(&subject_type), false));
                }
                schema::PatternView::Type {
                    ty,
                    slot: Some(bound),
                } if bound == slot => return ty.materialize().map(Some),
                _ => {}
            }
        }
        Ok(None)
    }

    fn derive_checked_record_type(
        &self,
        instruction: usize,
    ) -> Result<Option<Type>, IrVerifyError> {
        let schema::ExprView::Record(mut entries) = self.expression_view(instruction)? else {
            return Err(IrVerifyError::new(
                "record instruction tag and generated view disagree",
            ));
        };
        let mut fields = BTreeMap::new();
        for entry in self.checked_record_entries(&mut entries)? {
            match entry {
                CheckedRecordEntry::Field(name, value) => {
                    let Some(field_type) = self.derive_checked_expression_type(value)? else {
                        return Ok(None);
                    };
                    fields.insert(name, field_type);
                }
                CheckedRecordEntry::Spread(value) => {
                    let Some(source_type) = self.derive_checked_expression_type(value)? else {
                        return Ok(None);
                    };
                    let Type::Record(source_fields) = source_type.into_unvalidated() else {
                        return Ok(None);
                    };
                    fields.extend(source_fields);
                }
            }
        }
        Ok(Some(Type::Record(fields)))
    }

    fn update_derived_record_type(
        fields: &mut BTreeMap<Name, Type>,
        path: &[Name],
        value_type: Type,
    ) -> bool {
        let Some((field, rest)) = path.split_first() else {
            return false;
        };
        if rest.is_empty() {
            return fields.insert(*field, value_type).is_some();
        }
        let Some(Type::Record(nested)) = fields.get_mut(field) else {
            return false;
        };
        Self::update_derived_record_type(nested, rest, value_type)
    }

    fn verify_checked_expression_producer(
        &self,
        instruction: usize,
        fact: FullCheckedExprType,
    ) -> Result<(), IrVerifyError> {
        let ty = self.store.semantic.to_type(fact.type_id)?;
        if !ty.contains_typed_callable()
            && !fact.with_error_source
            && !matches!(ty, Type::ErrorVariant { .. })
        {
            return Err(IrVerifyError::new(
                "checked expression fact is neither callable, a structured error, nor a With error source",
            ));
        }
        let exact_type = |actual: TypeId| -> Result<(), IrVerifyError> {
            if self.store.semantic.to_type(actual)? == ty {
                Ok(())
            } else {
                Err(IrVerifyError::new(
                    "checked expression fact disagrees with its structural producer",
                ))
            }
        };
        let require_child = |child: usize, expected: &Type| {
            self.require_checked_child_type(child, expected)
        };
        let require_registered_return = |expected: Type, message| {
            if expected == ty {
                Ok(())
            } else {
                Err(IrVerifyError::new(message))
            }
        };
        let view = self.expression_view(instruction)?;
        match view {
            schema::ExprView::Int(_) if matches!(ty, Type::Int | Type::UInt) => {}
            schema::ExprView::Float(_) if ty == Type::Float => {}
            schema::ExprView::Duration(_) if ty == Type::Duration => {}
            schema::ExprView::Bool(_) if ty == Type::Bool => {}
            schema::ExprView::Str(_) if ty == Type::Str => {}
            schema::ExprView::Bytes(_) if ty == Type::Bytes => {}
            schema::ExprView::Path(_) if ty == Type::Path => {}
            schema::ExprView::PreparedRegex(_) if ty == Type::Regex => {}
            schema::ExprView::Unit if ty == Type::Unit => {}
            schema::ExprView::FunctionRef { function, pure } => {
                let function_matches = fact.function_ref.is_some_and(|(expected, expected_pure)| {
                    if expected_pure != pure {
                        return false;
                    }
                    match (expected.as_name(), expected.as_qualified(), function) {
                        (Some(expected), None, decode::FunctionNameView::Name(actual)) => {
                            expected == actual
                        }
                        (None, Some(expected), decode::FunctionNameView::Qualified(actual)) => {
                            expected == actual
                        }
                        _ => false,
                    }
                });
                let (signature, target_is_pure) = self.function_reference_signature(function)?;
                let Type::Callable(checked_callable) = &ty else {
                    return Err(IrVerifyError::new(
                        "checked function reference has a non-callable fact",
                    ));
                };
                if !function_matches {
                    return Err(IrVerifyError::new(
                        "checked function reference identity disagrees with its producer: source name",
                    ));
                }
                if target_is_pure != pure {
                    return Err(IrVerifyError::new(
                        "checked function reference identity disagrees with its producer: function kind",
                    ));
                }
                if typed_callable_purity(&ty) != Some(pure) {
                    return Err(IrVerifyError::new(
                        "checked function reference identity disagrees with its producer: checked kind",
                    ));
                }
                if !self.store.semantic.signature_fits_callable(
                    signature,
                    pure,
                    checked_callable,
                )? {
                    return Err(IrVerifyError::new(
                        "checked function reference identity disagrees with its producer: checked signature",
                    ));
                }
            }
            schema::ExprView::Param(slot) => {
                let actual = match self.checked_slot_type(slot) {
                    Ok(Some(actual)) => actual,
                    Ok(None) => return self.local_function_ref_matches_type(slot, &ty),
                    Err(error)
                        if error
                            .message
                            .starts_with("typed callee slot has no checked type fact:") =>
                    {
                        return self.local_function_ref_matches_type(slot, &ty);
                    }
                    Err(error) => return Err(error),
                };
                let actual_type = self.store.semantic.to_type(actual)?;
                if actual_type == ty {
                    return Ok(());
                } else if let Type::Callable(callable) = &ty
                    && matches!(
                        (actual_type, callable.pure),
                        (Type::Pure, true) | (Type::Proc, false)
                    )
                {
                    return self.local_function_ref_matches_type(slot, &ty);
                } else {
                    return Err(IrVerifyError::new(
                        "checked expression fact disagrees with its structural producer",
                    ));
                }
            }
            schema::ExprView::Field { .. } => {
                if self.derive_checked_field_type(instruction)? != ty {
                    return Err(IrVerifyError::new(
                        "checked field fact disagrees with its structural receiver",
                    ));
                }
            }
            schema::ExprView::Error(ErrorExprView::Structured {
                family,
                family_identity,
                variant,
                ..
            }) => {
                let family_identity = Name::intern(family_identity);
                let variant = Name::intern(variant);
                let Type::ErrorVariant {
                    family: checked_family,
                    variant: checked_variant,
                } = &ty
                else {
                    return Err(IrVerifyError::new(
                        "checked structured-error fact is not an error variant",
                    ));
                };
                if *checked_family != family_identity || *checked_variant != variant {
                    return Err(IrVerifyError::new(
                        "checked structured-error identity disagrees with its constructor",
                    ));
                }
                let identity_text = family_identity.as_str();
                let display_family = identity_text
                    .as_str()
                    .rsplit_once('.')
                    .map(|(_, member)| member)
                    .unwrap_or(identity_text.as_str());
                if family != display_family {
                    return Err(IrVerifyError::new(
                        "structured-error display name disagrees with its declaration identity",
                    ));
                }
                let key = CheckedPatternVariantKey {
                    kind: CheckedPatternVariantKind::Error,
                    owner: family_identity,
                    variant,
                };
                if !self.store.checked_pattern_variants.contains_key(&key) {
                    return Err(IrVerifyError::new(
                        "structured-error constructor has no checked declaration schema",
                    ));
                }
            }
            schema::ExprView::Call { function, .. }
            | schema::ExprView::DirectPureCall { function, .. } => {
                exact_type(self.function_return_type(function.id.index())?)?;
            }
            schema::ExprView::SelfCall { .. } => {
                let function = IrFunctionId::from_raw(self.owner)
                    .ok_or_else(|| IrVerifyError::new("self call has no function owner"))?;
                exact_type(self.function_return_type(function.index())?)?;
            }
            schema::ExprView::TypedCall { signature, .. } => {
                let Type::Callable(callable) = signature.materialize()? else {
                    return Err(IrVerifyError::new(
                        "checked typed-call producer has a non-callable signature",
                    ));
                };
                if ty != *callable.sig.return_ty {
                    return Err(IrVerifyError::new(
                        "checked typed-call result disagrees with its signature",
                    ));
                }
            }
            schema::ExprView::ModuleCall { op, .. } => {
                if Self::registered_api_operation_return_type(op)? != ty {
                    return Err(IrVerifyError::new(
                        "checked module call result disagrees with its registry signature",
                    ));
                }
            }
            schema::ExprView::FsTempDir { .. } => {
                if Self::registered_api_operation_return_type(RuntimeOp::FsTempDir)? != ty {
                    return Err(IrVerifyError::new(
                        "checked temporary-directory result disagrees with its registry signature",
                    ));
                }
            }
            schema::ExprView::FsFiles {
                result_wrapped, ..
            } => {
                let registered =
                    Self::registered_api_operation_return_type(RuntimeOp::FsFiles)?;
                let expected = if result_wrapped {
                    registered
                } else {
                    match registered {
                        Type::Result(ok, _) => *ok,
                        _ => {
                            return Err(IrVerifyError::new(
                                "filesystem file scan registry signature is not a result",
                            ));
                        }
                    }
                };
                require_registered_return(
                    expected,
                    "checked filesystem file scan disagrees with its registry signature",
                )?;
            }
            schema::ExprView::FsWalk {
                result_wrapped, ..
            } => {
                let registered =
                    Self::registered_api_operation_return_type(RuntimeOp::FsWalk)?;
                let expected = if result_wrapped {
                    registered
                } else {
                    match registered {
                        Type::Result(ok, _) => *ok,
                        _ => {
                            return Err(IrVerifyError::new(
                                "filesystem walk registry signature is not a result",
                            ));
                        }
                    }
                };
                require_registered_return(
                    expected,
                    "checked filesystem walk disagrees with its registry signature",
                )?;
            }
            schema::ExprView::FsList { op, .. } => {
                require_registered_return(
                    Self::registered_api_operation_return_type(op)?,
                    "checked filesystem listing disagrees with its registry signature",
                )?;
            }
            schema::ExprView::FsMkdir { .. } => {
                require_registered_return(
                    Self::registered_api_operation_return_type(RuntimeOp::FsMkdir)?,
                    "checked filesystem directory creation disagrees with its registry signature",
                )?;
            }
            schema::ExprView::FsRemove { .. } => {
                require_registered_return(
                    Self::registered_api_operation_return_type(RuntimeOp::FsRemove)?,
                    "checked filesystem removal disagrees with its registry signature",
                )?;
            }
            schema::ExprView::PathReadText { .. } => {
                if Self::registered_path_method_return_type("read_text", RuntimeOp::FsReadText)?
                    != ty
                {
                    return Err(IrVerifyError::new(
                        "checked path read result disagrees with its registry signature",
                    ));
                }
            }
            schema::ExprView::PathReadBytes { .. } => {
                require_registered_return(
                    Self::registered_path_method_return_type("read_bytes", RuntimeOp::FsRead)?,
                    "checked path byte read disagrees with its registry signature",
                )?;
            }
            schema::ExprView::PathExists { .. } => {
                require_registered_return(
                    Self::registered_path_method_return_type("exists", RuntimeOp::FsExists)?,
                    "checked path existence result disagrees with its registry signature",
                )?;
            }
            schema::ExprView::PathExecutable { .. } => {
                require_registered_return(
                    Self::registered_path_method_return_type(
                        "executable",
                        RuntimeOp::FsExecutable,
                    )?,
                    "checked path executable result disagrees with its registry signature",
                )?;
            }
            schema::ExprView::PathDu { .. } => {
                require_registered_return(
                    Self::registered_path_method_return_type("du", RuntimeOp::FsDu)?,
                    "checked path disk usage disagrees with its registry signature",
                )?;
            }
            schema::ExprView::PathMetadata { .. } => {
                require_registered_return(
                    Self::registered_path_method_return_type("metadata", RuntimeOp::FsMetadata)?,
                    "checked path metadata disagrees with its registry signature",
                )?;
            }
            schema::ExprView::PathReadlink { .. } => {
                require_registered_return(
                    Self::registered_path_method_return_type("readlink", RuntimeOp::FsReadlink)?,
                    "checked path readlink result disagrees with its registry signature",
                )?;
            }
            schema::ExprView::PathResolve { .. } => {
                require_registered_return(
                    Self::registered_path_method_return_type("resolve", RuntimeOp::PathResolve)?,
                    "checked path resolve result disagrees with its registry signature",
                )?;
            }
            schema::ExprView::PathWrite { atomic, .. } => {
                let (name, op) = if atomic {
                    ("write_atomic", RuntimeOp::FsWriteAtomic)
                } else {
                    ("write", RuntimeOp::FsWrite)
                };
                require_registered_return(
                    Self::registered_path_method_return_type(name, op)?,
                    "checked path write disagrees with its registry signature",
                )?;
            }
            schema::ExprView::PathMkdir { .. } => {
                require_registered_return(
                    Self::registered_path_method_return_type("mkdir", RuntimeOp::FsMkdir)?,
                    "checked path directory creation disagrees with its registry signature",
                )?;
            }
            schema::ExprView::PathRemove { .. } => {
                require_registered_return(
                    Self::registered_path_method_return_type("remove", RuntimeOp::FsRemove)?,
                    "checked path removal disagrees with its registry signature",
                )?;
            }
            schema::ExprView::ArchiveTarCreate { .. } => {
                require_registered_return(
                    Self::registered_api_operation_return_type(RuntimeOp::ArchiveTarCreate)?,
                    "checked archive creation disagrees with its registry signature",
                )?;
            }
            schema::ExprView::Index { base, .. }
            | schema::ExprView::IndexFromEnd { base, .. } => {
                let actual = self.checked_index_result_type(base.raw() as usize)?;
                if actual != ty {
                    return Err(IrVerifyError::new(
                        "checked index fact disagrees with its collection producer",
                    ));
                }
            }
            schema::ExprView::Try { value, .. } => {
                let value = value.raw() as usize;
                let Some(value_type) = self.expression_checked_type_id(value)? else {
                    return Err(IrVerifyError::new(
                        "checked try producer is missing its operand type",
                    ));
                };
                let value_type = self.store.semantic.to_type(value_type)?;
                let result = match value_type {
                    Type::Result(ok, _) => *ok,
                    Type::Validated(validated) => match validated.base() {
                        Type::Result(ok, _) => ok.as_ref().clone(),
                        _ => {
                            return Err(IrVerifyError::new(
                                "checked try producer operand is not a result",
                            ));
                        }
                    },
                    _ => {
                        return Err(IrVerifyError::new(
                            "checked try producer operand is not a result",
                        ));
                    }
                };
                if result != ty {
                    return Err(IrVerifyError::new(
                        "checked try fact disagrees with its result operand",
                    ));
                }
            }
            schema::ExprView::IfExpr {
                mut branches,
                else_value,
                ..
            } => {
                for branch in &mut branches {
                    let (_, value) = branch?;
                    require_child(value.raw() as usize, &ty)?;
                }
                require_child(else_value.raw() as usize, &ty)?;
            }
            schema::ExprView::PatternIf {
                mut branches,
                else_value,
                ..
            } => {
                for branch in &mut branches {
                    let (_, value, _) = branch?;
                    require_child(value.raw() as usize, &ty)?;
                }
                require_child(else_value.raw() as usize, &ty)?;
            }
            schema::ExprView::MatchExpr { value, arms, .. } => {
                self.checked_match_result_matches_type(value.raw(), arms, &ty)?;
            }
            schema::ExprView::StrMatchExpr {
                mut arms,
                fallback,
                ..
            }
            | schema::ExprView::TagMatchExpr {
                mut arms,
                fallback,
                ..
            } => {
                for arm in &mut arms {
                    let (_, value) = arm?;
                    require_child(value.raw() as usize, &ty)?;
                }
                if let Some(fallback) = fallback {
                    require_child(fallback.raw() as usize, &ty)?;
                }
            }
            schema::ExprView::ResultFallback { left, right } => {
                let left = left.raw() as usize;
                let Some(left_type) = self.expression_checked_type_id(left)? else {
                    return Err(IrVerifyError::new(
                        "checked fallback producer is missing its left type",
                    ));
                };
                let left_type = self.store.semantic.to_type(left_type)?;
                let fallback_type = match left_type {
                    Type::Optional(value) | Type::Result(value, _) => *value,
                    Type::Validated(value) => match value.base() {
                        Type::Optional(inner) | Type::Result(inner, _) => inner.as_ref().clone(),
                        _ => {
                            return Err(IrVerifyError::new(
                                "checked fallback producer has no fallback value type",
                            ));
                        }
                    },
                    _ => {
                        return Err(IrVerifyError::new(
                            "checked fallback producer has no fallback value type",
                        ));
                    }
                };
                if fallback_type != ty {
                    return Err(IrVerifyError::new(
                        "checked fallback fact disagrees with its left producer",
                    ));
                }
                require_child(right.raw() as usize, &ty)?;
            }
            schema::ExprView::Record(mut entries) => {
                let Type::Record(fields) = &ty else {
                    return Err(IrVerifyError::new(
                        "checked record fact has a non-record result type",
                    ));
                };
                let entries = self.checked_record_entries(&mut entries)?;
                let mut supplied = BTreeSet::new();
                let mut later_fields = BTreeSet::new();
                for entry in entries.into_iter().rev() {
                    match entry {
                        CheckedRecordEntry::Field(name, value) => {
                            if !later_fields.contains(&name)
                                && let Some(field_type) = fields.get(&name)
                                && field_type.contains_typed_callable()
                            {
                                require_child(value, field_type)?;
                                supplied.insert(name);
                            }
                            later_fields.insert(name);
                        }
                        CheckedRecordEntry::Spread(value) => {
                            let Some(source_type) = self.expression_checked_type_id(value)? else {
                                return Err(IrVerifyError::new(
                                    "checked record spread is missing its type",
                                ));
                            };
                            let Type::Record(source_fields) =
                                self.store.semantic.to_type(source_type)?
                            else {
                                return Err(IrVerifyError::new(
                                    "checked callable record spread has no field facts",
                                ));
                            };
                            for (name, field_type) in source_fields {
                                if !later_fields.contains(&name) {
                                    if field_type.contains_typed_callable()
                                        && let Some(expected_field) = fields.get(&name)
                                    {
                                        if !self.checked_type_fits(&field_type, expected_field) {
                                            return Err(IrVerifyError::new(
                                                "checked record spread field disagrees with its result",
                                            ));
                                        }
                                        supplied.insert(name);
                                    }
                                }
                                later_fields.insert(name);
                            }
                        }
                    }
                }
                if fields.iter().any(|(name, field_type)| {
                    field_type.contains_typed_callable() && !supplied.contains(name)
                }) {
                    return Err(IrVerifyError::new(
                        "checked record fact has an unproven callable field",
                    ));
                }
            }
            schema::ExprView::RecordUpdate { base, updates, .. } => {
                self.checked_record_update_matches_type(
                    base.raw() as usize,
                    updates,
                    &ty,
                )?;
            }
            schema::ExprView::Ok(value) => {
                let Type::Result(ok, _) = &ty else {
                    return Err(IrVerifyError::new("checked Ok fact is not a result"));
                };
                require_child(value.raw() as usize, ok)?;
            }
            schema::ExprView::Err { value, .. } => {
                let Type::Result(_, error) = &ty else {
                    return Err(IrVerifyError::new("checked Err fact is not a result"));
                };
                require_child(value.raw() as usize, error)?;
            }
            schema::ExprView::Null => {
                if !matches!(ty, Type::Optional(_) | Type::Any) {
                    return Err(IrVerifyError::new(
                        "checked null fact has an incompatible producer type",
                    ));
                }
            }
            schema::ExprView::ValueBlock { body, .. } => {
                self.checked_block_tail_matches_type(body, &ty)?;
            }
            schema::ExprView::ContextScope { body, .. } => {
                match &ty {
                    Type::Result(ok, _) => {
                        self.checked_block_tail_matches_type(body, ok)?;
                    }
                    _ => {
                        return Err(IrVerifyError::new(
                            "checked scope fact disagrees with its value block",
                        ));
                    }
                }
            }
            schema::ExprView::Loop { mut body, .. } => {
                let mut found_value = false;
                for statement in &mut body {
                    let statement = statement?.raw() as usize;
                    if let schema::StmtView::BreakValue { value } =
                        self.statement_view(statement)?
                    {
                        found_value = true;
                        require_child(value.raw() as usize, &ty)?;
                    }
                }
                if !found_value {
                    return Err(IrVerifyError::new(
                        "checked loop fact has no value-producing break",
                    ));
                }
            }
            schema::ExprView::List(mut values) => {
                let (item, requires_nonempty) = match &ty {
                    Type::List(item) => (item.as_ref(), false),
                    Type::Validated(validated)
                        if validated.validation() == crate::sema::validated::Validation::NonEmpty =>
                    {
                        let Type::List(item) = validated.base() else {
                            return Err(IrVerifyError::new(
                                "checked non-empty list fact has a non-list base",
                            ));
                        };
                        (item.as_ref(), true)
                    }
                    _ => {
                    return Err(IrVerifyError::new("checked list fact is not a list"));
                    }
                };
                if requires_nonempty && values.len() == 0 {
                    return Err(IrVerifyError::new(
                        "checked non-empty list fact has an empty literal producer",
                    ));
                }
                for value in &mut values {
                    require_child(value?.raw() as usize, item)?;
                }
            }
            schema::ExprView::ListBuild(mut values) => {
                let (item, requires_nonempty) = match &ty {
                    Type::List(item) => (item.as_ref(), false),
                    Type::Validated(validated)
                        if validated.validation() == crate::sema::validated::Validation::NonEmpty =>
                    {
                        let Type::List(item) = validated.base() else {
                            return Err(IrVerifyError::new(
                                "checked non-empty list-build fact has a non-list base",
                            ));
                        };
                        (item.as_ref(), true)
                    }
                    _ => {
                    return Err(IrVerifyError::new("checked list-build fact is not a list"));
                    }
                };
                let mut proves_nonempty = false;
                for value in &mut values {
                    let (splice, value, _) = value?;
                    let value = value.raw() as usize;
                    if splice {
                        let Some(actual) = self.expression_checked_type_id(value)? else {
                            return Err(IrVerifyError::new(
                                "checked list splice is missing its type",
                            ));
                        };
                        let actual = self.store.semantic.to_type(actual)?;
                        let (actual_list, is_nonempty) = match &actual {
                            Type::Validated(validated)
                                if validated.validation()
                                    == crate::sema::validated::Validation::NonEmpty =>
                            {
                                (validated.base(), true)
                            }
                            actual => (actual, false),
                        };
                        let expected_list = Type::List(Box::new(item.clone()));
                        if !self.checked_type_fits(actual_list, &expected_list) {
                            return Err(IrVerifyError::new(
                                "checked list splice disagrees with its element type",
                            ));
                        }
                        if is_nonempty
                            || (!requires_nonempty
                                && self.expression_is_nonempty_list_literal(value)?)
                        {
                            proves_nonempty = true;
                        }
                    } else {
                        require_child(value, item)?;
                        proves_nonempty = true;
                    }
                }
                if requires_nonempty && !proves_nonempty {
                    return Err(IrVerifyError::new(
                        "checked non-empty list-build fact has no non-empty producer",
                    ));
                }
            }
            schema::ExprView::ListComp { value, .. } => {
                let Type::List(item) = &ty else {
                    return Err(IrVerifyError::new("checked list-comprehension fact is not a list"));
                };
                require_child(value.raw() as usize, item)?;
            }
            schema::ExprView::MapComp { key, value, .. } => {
                let Type::Map(key_type, value_type) = &ty else {
                    return Err(IrVerifyError::new("checked map-comprehension fact is not a map"));
                };
                require_child(key.raw() as usize, key_type)?;
                require_child(value.raw() as usize, value_type)?;
            }
            _ => {
                if fact.function_ref.is_some() {
                    return Err(IrVerifyError::new(
                        "checked function reference producer tag was changed",
                    ));
                }
                return Err(IrVerifyError::new(
                    "checked callable expression has no structural producer proof",
                ));
            }
        }
        Ok(())
    }

    fn verify_checked_expression_facts(&self) -> Result<(), IrVerifyError> {
        let facts = &self.store.checked_expr_types;
        let with_error_sources = self.checked_with_error_source_instructions()?;
        // Instruction order makes each owner's sparse facts one searchable range.
        let first = facts
            .partition_point(|fact| (fact.instruction as usize) < self.instruction_range.start);
        let last = facts
            .partition_point(|fact| (fact.instruction as usize) < self.instruction_range.end);
        for fact in &facts[first..last] {
            let instruction = fact.instruction as usize;
            let ty = self.store.semantic.to_type(fact.type_id)?;
            if fact.with_error_source && !with_error_sources.contains(&instruction) {
                return Err(IrVerifyError::new(
                    "checked With error source fact has no initializer producer",
                ));
            }
            if fact.with_error_source
                && !matches!(ty, Type::Result(_, _) | Type::Any | Type::Unknown)
            {
                return Err(IrVerifyError::new(
                    "checked With error source fact is not a result value",
                ));
            }
            if !ty.contains_typed_callable()
                && !fact.with_error_source
                && !matches!(ty, Type::ErrorVariant { .. })
            {
                return Err(IrVerifyError::new(
                    "checked expression fact is neither callable, a structured error, nor a With error source",
                ));
            }
            if let Err(error) = self.verify_checked_expression_producer(instruction, *fact) {
                return Err(error);
            }
        }
        for instruction in self.instruction_range.clone() {
            if self.store.tags[instruction] == FullTag::ExprError
                && self.checked_expression_type_id(instruction)?.is_none()
            {
                return Err(IrVerifyError::new(
                    "structured-error constructor has no checked declaration identity",
                ));
            }
        }
        Ok(())
    }

    fn verify_typed_calls_after_structure(&self) -> Result<(), IrVerifyError> {
        for instruction in self.instruction_range.clone() {
            if self.store.tags[instruction] != FullTag::ExprTypedCall {
                continue;
            }
            let payload = self.store.payload(self.store.data[instruction].range())?;
            self.verify_typed_call_shape(self.cursor(payload))?;
        }
        Ok(())
    }

    fn verify_callable_slot_initializer(
        &self,
        slot: usize,
        value: usize,
    ) -> Result<(), IrVerifyError> {
        let slot_type = match self.checked_slot_type(slot) {
            Ok(slot_type) => slot_type,
            Err(error)
                if error
                    .message
                    .starts_with("typed callee slot has no checked type fact:")
                    && self.checked_expression_type_id(value)?.is_none()
                    && matches!(
                        self.expression_view(value)?,
                        schema::ExprView::FunctionRef { .. }
                    ) =>
            {
                // An unannotated function reference has only broad Pure/Proc inference;
                // contextual callable uses are checked from this immutable source at use sites.
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        let Some(fact) = slot_type else {
            return Ok(());
        };
        let fact_type = self.store.semantic.to_type(fact)?;
        if !fact_type.contains_typed_callable() {
            return Ok(());
        }
        if let Some(initializer) = self.checked_expression_type_id(value)? {
            if self.store.semantic.to_type(initializer)? == fact_type {
                return Ok(());
            }
        }
        if self
            .checked_expression_matches_type(value, &fact_type)
            .is_err()
        {
            return Err(IrVerifyError::new(
                "local slot type fact disagrees with its initializer IR representation",
            ));
        }
        Ok(())
    }

    fn verify_callable_slot_without_checked_initializer(
        &self,
        slot: usize,
    ) -> Result<(), IrVerifyError> {
        let Some(slot_type) = self.checked_slot_type(slot)? else {
            return Ok(());
        };
        if self
            .store
            .semantic
            .to_type(slot_type)?
            .contains_typed_callable()
        {
            return Err(IrVerifyError::new(
                "local slot type fact disagrees with its initializer IR representation",
            ));
        }
        Ok(())
    }

    fn checked_expression_type(&self, instruction: usize) -> Result<Option<Type>, IrVerifyError> {
        match self.expression_checked_type_id(instruction) {
            Ok(Some(type_id)) => self.store.semantic.to_type(type_id).map(Some),
            Ok(None) if self.store.tags.get(instruction) == Some(&FullTag::ExprField) => self
                .checked_field_receiver_type(instruction)?
                .map_or(Ok(None), |_| self.derive_checked_field_type(instruction).map(Some)),
            Ok(None) => Ok(None),
            Err(error) if self.store.tags.get(instruction) == Some(&FullTag::ExprField) => {
                if self.checked_field_receiver_type(instruction)?.is_none() {
                    return Ok(None);
                }
                let field_type = self.derive_checked_field_type(instruction)?;
                if field_type.contains_typed_callable() {
                    Err(error)
                } else {
                    Ok(Some(field_type))
                }
            }
            Err(error) => Err(error),
        }
    }

    fn checked_field_receiver_type(
        &self,
        instruction: usize,
    ) -> Result<Option<Type>, IrVerifyError> {
        let schema::ExprView::Field { base, .. } = self.expression_view(instruction)? else {
            return Err(IrVerifyError::new(
                "field instruction tag and generated view disagree",
            ));
        };
        self.derive_checked_expression_type(base.raw() as usize)
    }

    fn derive_checked_field_type(&self, instruction: usize) -> Result<Type, IrVerifyError> {
        let payload = self.store.payload(self.store.data[instruction].range())?;
        let [base, name, ..] = payload else {
            return Err(IrVerifyError::new("checked field payload is truncated"));
        };
        let base = *base as usize;
        let field = Name::intern(self.store.string(*name)?);
        let receiver = self
            .derive_checked_expression_type(base)?
            .ok_or_else(|| IrVerifyError::new("checked field receiver has no source type"))?;
        let receiver = receiver.into_unvalidated();
        let receiver_id = if matches!(&receiver, Type::Record(_) | Type::Module(_)) {
            self.expression_checked_type_id(base)?
        } else {
            None
        };
        if let Some(receiver_id) = receiver_id
            && let Some(field_id) = self.store.semantic.field_type(receiver_id, field)?
        {
            return self.store.semantic.to_type(field_id);
        }
        match receiver {
            Type::Record(fields) => fields.get(&field).cloned().ok_or_else(|| {
                IrVerifyError::new("checked field is absent from its record receiver")
            }),
            Type::Module(exports) => exports.get(&field).map(|export| export.field_type()).ok_or_else(|| {
                IrVerifyError::new("checked field is absent from its module receiver")
            }),
            Type::ErasedRecord | Type::DynamicModule => Ok(Type::Any),
            receiver => match crate::sema::check::Checker::fixed_field_type(&receiver, field) {
                Some(Some(field_type)) => Ok(field_type),
                Some(None) => Err(IrVerifyError::new(
                    "checked field is absent from its fixed-field receiver",
                )),
                None => Err(IrVerifyError::new(
                    "checked field receiver has no structural field type",
                )),
            },
        }
    }

    fn checked_result_payload_type(subject_type: Option<&Type>, ok: bool) -> Option<Type> {
        match subject_type? {
            Type::Result(ok_type, error_type) => Some(if ok {
                ok_type.as_ref().clone()
            } else {
                error_type.as_ref().clone()
            }),
            Type::Validated(validated) => match validated.base() {
                Type::Result(ok_type, error_type) => Some(if ok {
                    ok_type.as_ref().clone()
                } else {
                    error_type.as_ref().clone()
                }),
                _ => None,
            },
            _ => None,
        }
    }

    fn verify_tag_pattern_subject(
        &self,
        subject_type: Option<&Type>,
        type_name: Name,
    ) -> Result<(), IrVerifyError> {
        if let Some(subject_type) = subject_type {
            match subject_type {
                Type::Any | Type::Unknown => {}
                Type::Tag(actual) if *actual == type_name => {}
                _ => {
                    return Err(IrVerifyError::new(
                        "tag pattern disagrees with its checked subject type",
                    ));
                }
            }
        }
        Ok(())
    }

    fn verify_error_pattern_subject(
        &self,
        subject_type: Option<&Type>,
        family: Name,
        variant: Name,
    ) -> Result<(), IrVerifyError> {
        if let Some(subject_type) = subject_type {
            match subject_type {
                Type::Any | Type::Unknown | Type::Error | Type::ProcessError => {}
                Type::ErrorFamily(actual) if *actual == family => {}
                Type::ErrorVariant {
                    family: actual_family,
                    variant: actual_variant,
                } if *actual_family == family && *actual_variant == variant => {}
                _ => {
                    return Err(IrVerifyError::new(
                        "error pattern disagrees with its checked subject type",
                    ));
                }
            }
        }
        Ok(())
    }

    fn pattern_variant_field_type(
        &self,
        key: CheckedPatternVariantKey,
        field: usize,
    ) -> Result<Option<Type>, IrVerifyError> {
        let Some(schema) = self.store.checked_pattern_variants.get(&key) else {
            return Ok(None);
        };
        let field = schema
            .fields
            .get(field)
            .ok_or_else(|| IrVerifyError::new("pattern variant field is outside its declaration"))?;
        self.store.semantic.to_type(field.type_id).map(Some)
    }

    fn verify_callable_slot_writer_type(
        &self,
        slot: usize,
        writer_type: Option<&Type>,
    ) -> Result<(), IrVerifyError> {
        let Some(slot_type) = self.checked_slot_type(slot)? else {
            return Ok(());
        };
        let slot_type = self.store.semantic.to_type(slot_type)?;
        if !slot_type.contains_typed_callable() {
            return Ok(());
        }
        let Some(writer_type) = writer_type else {
            return Err(IrVerifyError::new(
                "local callable slot writer has no checked producer type",
            ));
        };
        if writer_type == &slot_type || self.checked_type_fits(writer_type, &slot_type) {
            Ok(())
        } else {
            Err(IrVerifyError::new(
                "local callable slot type fact disagrees with its writer producer",
            ))
        }
    }

    fn callable_assignment_target_type(
        &self,
        root_slot: usize,
        path: &[CallableAssignmentPathStep],
    ) -> Result<Option<Type>, IrVerifyError> {
        let Some(root_type) = self.checked_slot_type(root_slot)? else {
            return Ok(None);
        };
        let mut target_type = self.store.semantic.to_type(root_type)?;
        for step in path {
            target_type = match step {
                CallableAssignmentPathStep::Field(name) => {
                    let Type::Record(fields) = target_type else {
                        return Ok(None);
                    };
                    fields.get(name).cloned().ok_or_else(|| {
                        IrVerifyError::new(
                            "callable assignment field is absent from its checked receiver",
                        )
                    })?
                }
                CallableAssignmentPathStep::Index => match target_type.into_unvalidated() {
                    Type::List(item) | Type::Map(_, item) => *item,
                    _ => return Ok(None),
                },
            };
        }
        Ok(Some(target_type))
    }

    fn verify_callable_assignment_value(
        &self,
        root_slot: usize,
        path: &[CallableAssignmentPathStep],
        value: usize,
    ) -> Result<(), IrVerifyError> {
        let Some(target_type) = self.callable_assignment_target_type(root_slot, path)? else {
            return Ok(());
        };
        if target_type.contains_typed_callable() {
            self.checked_expression_matches_type(value, &target_type)
                .map_err(|_| {
                    IrVerifyError::new(
                        "assignment value disagrees with its checked callable target",
                    )
                })?;
        }
        Ok(())
    }

    // Assign each propagated try to its nearest With initializer in one owner pass.
    fn checked_with_initializer_error_types(
        &self,
    ) -> Result<BTreeMap<usize, Type>, IrVerifyError> {
        let mut initializer_owners = BTreeMap::new();
        let mut error_types = BTreeMap::new();
        for instruction in self.instruction_range.clone() {
            if self.store.tags[instruction] != FullTag::StmtWith {
                continue;
            }
            let schema::StmtView::With {
                mut bindings,
                else_param_slot,
                ..
            } = self.statement_view(instruction)? else {
                unreachable!("With instruction tag and generated view disagree")
            };
            let mut error_type = None;
            for binding in &mut bindings {
                let (_, value) = binding?;
                let initializer = value.raw() as usize;
                if initializer_owners.insert(initializer, instruction).is_some() {
                    return Err(IrVerifyError::new(
                        "checked With initializer has multiple owners",
                    ));
                }
                if else_param_slot.is_some() {
                    if let Some(Type::Result(_, error)) =
                        self.checked_expression_type(initializer)?
                    {
                        Self::merge_with_error_type(&mut error_type, *error);
                    }
                }
            }
            if else_param_slot.is_some() {
                error_types.insert(instruction, error_type);
            }
        }
        for instruction in self.instruction_range.clone() {
            if self.store.tags[instruction] != FullTag::ExprTry {
                continue;
            }
            let mut current = instruction;
            let mut with_instruction = None;
            loop {
                if let Some(owner) = initializer_owners.get(&current) {
                    with_instruction = Some(*owner);
                    break;
                }
                let Some(parent) = self.instruction_parent_of(current)? else {
                    break;
                };
                if self.store.tags[parent] == FullTag::StmtDefer {
                    break;
                }
                current = parent;
            }
            let Some(with_instruction) = with_instruction else {
                continue;
            };
            let Some(error_type) = error_types.get_mut(&with_instruction) else {
                continue;
            };
            let schema::ExprView::Try { value, .. } = self.expression_view(instruction)? else {
                unreachable!("Try instruction tag and generated view disagree")
            };
            let value_instruction = value.raw() as usize;
            let operand_type = self
                .derive_checked_expression_type(value_instruction)?
                .ok_or_else(|| {
                    IrVerifyError::new(format!(
                        "checked With propagation at instruction {instruction} is missing its Result source type at instruction {value_instruction} ({:?})",
                        self.store.tags[value_instruction]
                    ))
                })?;
            let error = Self::checked_result_payload_type(Some(&operand_type), false)
                .ok_or_else(|| {
                    IrVerifyError::new("checked With propagation source is not a Result")
                })?;
            Self::merge_with_error_type(error_type, error);
        }
        Ok(error_types
            .into_iter()
            .map(|(instruction, error_type)| (instruction, error_type.unwrap_or(Type::Error)))
            .collect())
    }

    fn merge_with_error_type(error_type: &mut Option<Type>, next: Type) {
        *error_type = Some(match error_type.take() {
            None => next,
            Some(previous) if previous == next => previous,
            Some(_) => Type::Error,
        });
    }

    fn verify_comp_target_writer_type(
        &self,
        target: decode::CompTargetView<'a>,
        writer_type: Option<&Type>,
    ) -> Result<(), IrVerifyError> {
        match target {
            decode::CompTargetView::Discard => Ok(()),
            decode::CompTargetView::Slot(slot) => {
                self.verify_callable_slot_writer_type(slot, writer_type)
            }
            decode::CompTargetView::Record { mut fields } => {
                let writer_fields = writer_type.and_then(|writer_type| match writer_type {
                    Type::Record(fields) => Some(fields),
                    _ => None,
                });
                for field in &mut fields {
                    let (name, target, _) = field?;
                    let field_type = writer_fields.and_then(|fields| fields.get(&name));
                    self.verify_comp_target_writer_type(target, field_type)?;
                }
                Ok(())
            }
        }
    }

    fn verify_pattern_writer_types(
        &self,
        root: u32,
        subject_type: Option<&Type>,
    ) -> Result<(), IrVerifyError> {
        let mut pending = vec![(root, subject_type.cloned())];
        while let Some((pattern, subject_type)) = pending.pop() {
            match self.checked_pattern_view(pattern)? {
                schema::PatternView::Alias { pattern, slot } => {
                    self.verify_callable_slot_writer_type(slot, subject_type.as_ref())?;
                    pending.push((pattern.raw(), subject_type));
                }
                schema::PatternView::Alternation { mut patterns } => {
                    for pattern in &mut patterns {
                        pending.push((pattern?.raw(), subject_type.clone()));
                    }
                }
                schema::PatternView::List {
                    mut elements,
                    rest,
                } => {
                    let item_type = subject_type.as_ref().and_then(Type::iteration_item_type);
                    for pattern in &mut elements {
                        pending.push((pattern?.raw(), item_type.clone()));
                    }
                    if let Some(rest) = rest {
                        let rest_type = item_type.map(|item| Type::List(Box::new(item)));
                        pending.push((rest.raw(), rest_type));
                    }
                }
                schema::PatternView::RecordTest { mut fields } => {
                    let writer_fields = subject_type.as_ref().and_then(|subject| match subject {
                        Type::Record(fields) => Some(fields),
                        _ => None,
                    });
                    for field in &mut fields {
                        let (name, pattern) = field?;
                        let field_type = writer_fields.and_then(|fields| fields.get(&name)).cloned();
                        pending.push((pattern.raw(), field_type));
                    }
                }
                schema::PatternView::ResultTest { ok, inner } => {
                    let payload_type = Self::checked_result_payload_type(
                        subject_type.as_ref(),
                        ok,
                    );
                    pending.push((inner.raw(), payload_type));
                }
                schema::PatternView::TagTest {
                    type_name,
                    name,
                    mut fields,
                } => {
                    self.verify_tag_pattern_subject(subject_type.as_ref(), type_name)?;
                    let key = CheckedPatternVariantKey {
                        kind: CheckedPatternVariantKind::Tag,
                        owner: type_name,
                        variant: name,
                    };
                    let declared_count = self
                        .store
                        .checked_pattern_variants
                        .get(&key)
                        .map(|schema| schema.fields.len());
                    let mut field_index = 0usize;
                    for pattern in &mut fields {
                        let pattern = pattern?;
                        let field_type = self.pattern_variant_field_type(key, field_index)?;
                        pending.push((pattern.raw(), field_type));
                        field_index += 1;
                    }
                    if declared_count.is_some_and(|count| count != field_index) {
                        return Err(IrVerifyError::new(
                            "tag pattern field count disagrees with its declaration",
                        ));
                    }
                }
                schema::PatternView::ErrorTest {
                    family,
                    variant,
                    mut fields,
                } => {
                    self.verify_error_pattern_subject(
                        subject_type.as_ref(),
                        family,
                        variant,
                    )?;
                    let key = CheckedPatternVariantKey {
                        kind: CheckedPatternVariantKind::Error,
                        owner: family,
                        variant,
                    };
                    for field in &mut fields {
                        let (name, pattern) = field?;
                        let Some(schema) = self.store.checked_pattern_variants.get(&key) else {
                            pending.push((pattern.raw(), None));
                            continue;
                        };
                        let field_index = schema
                            .fields
                            .iter()
                            .position(|field| field.name == Some(name))
                            .ok_or_else(|| {
                                IrVerifyError::new(
                                    "error pattern field is absent from its declaration",
                                )
                            })?;
                        let field_type = self.pattern_variant_field_type(key, field_index)?;
                        pending.push((pattern.raw(), field_type));
                    }
                }
                schema::PatternView::Bind { slot } => {
                    self.verify_callable_slot_writer_type(slot, subject_type.as_ref())?;
                }
                schema::PatternView::Type { ty, slot } => {
                    let ty = ty.materialize()?;
                    if ty.contains_typed_callable() {
                        return Err(IrVerifyError::new(
                            "type pattern cannot establish a typed callable",
                        ));
                    }
                    if let Some(slot) = slot {
                        self.verify_callable_slot_writer_type(slot, Some(&ty))?;
                    }
                }
                schema::PatternView::ResultOk { slot, .. } => {
                    if let Some(slot) = slot {
                        let payload_type =
                            Self::checked_result_payload_type(subject_type.as_ref(), true);
                        self.verify_callable_slot_writer_type(slot, payload_type.as_ref())?;
                    }
                }
                schema::PatternView::ResultErr { slot, .. } => {
                    if let Some(slot) = slot {
                        let payload_type =
                            Self::checked_result_payload_type(subject_type.as_ref(), false);
                        self.verify_callable_slot_writer_type(slot, payload_type.as_ref())?;
                    }
                }
                schema::PatternView::Tag {
                    type_name,
                    name,
                    slots,
                } => {
                    self.verify_tag_pattern_subject(subject_type.as_ref(), type_name)?;
                    let key = CheckedPatternVariantKey {
                        kind: CheckedPatternVariantKind::Tag,
                        owner: type_name,
                        variant: name,
                    };
                    let declared_count = self
                        .store
                        .checked_pattern_variants
                        .get(&key)
                        .map(|schema| schema.fields.len());
                    let mut field_index = 0usize;
                    for slot in slots {
                        if let Some(slot) = slot? {
                            let field_type = self.pattern_variant_field_type(key, field_index)?;
                            self.verify_callable_slot_writer_type(slot, field_type.as_ref())?;
                        }
                        field_index += 1;
                    }
                    if declared_count.is_some_and(|count| count != field_index) {
                        return Err(IrVerifyError::new(
                            "tag pattern field count disagrees with its declaration",
                        ));
                    }
                }
                schema::PatternView::Text { mut holes, .. } => {
                    for hole in &mut holes {
                        pending.push((hole?.raw(), None));
                    }
                }
                schema::PatternView::Wildcard
                | schema::PatternView::TagType { .. }
                | schema::PatternView::Literal(_)
                | schema::PatternView::Facet { .. } => {}
            }
        }
        Ok(())
    }

    fn verify_pattern_condition_writers(
        &self,
        condition: usize,
        mut captures: ListView<'a, usize>,
    ) -> Result<(), IrVerifyError> {
        let mut capture_slots = Vec::new();
        for capture in &mut captures {
            capture_slots.push(capture?);
        }
        if capture_slots.is_empty() {
            return Ok(());
        }
        let schema::ExprView::MatchExpr { value, mut arms, .. } =
            self.expression_view(condition)?
        else {
            return Err(IrVerifyError::new(
                "pattern condition captures have no match producer",
            ));
        };
        let subject_type = self.checked_expression_type(value.raw() as usize)?;
        let mut pattern_captures = BTreeSet::new();
        for arm in &mut arms {
            let (pattern, _, _) = arm?;
            let pattern = pattern.raw();
            self.verify_pattern_writer_types(pattern, subject_type.as_ref())?;
            pattern_captures.extend(self.pattern_capture_slots(pattern as usize)?);
        }
        let declared_captures = capture_slots.iter().copied().collect::<BTreeSet<_>>();
        if declared_captures.len() != capture_slots.len() || pattern_captures != declared_captures {
            return Err(IrVerifyError::new(
                "pattern condition captures disagree with its match producer",
            ));
        }
        Ok(())
    }

    fn verify_callable_slot_initializers(&self) -> Result<(), IrVerifyError> {
        let with_error_types = self.checked_with_initializer_error_types()?;
        for instruction in self.instruction_range.clone() {
            let instruction_kind = self.store.tags[instruction].kind();
            if instruction_kind == schema::InstructionKind::Expr {
                match self.expression_view(instruction)? {
                    schema::ExprView::MatchExpr { value, mut arms, .. } => {
                        let subject_type = self.checked_expression_type(value.raw() as usize)?;
                        for arm in &mut arms {
                            let (pattern, _, _) = arm?;
                            self.verify_pattern_writer_types(
                                pattern.raw(),
                                subject_type.as_ref(),
                            )?;
                        }
                    }
                    schema::ExprView::PatternIf { mut branches, .. } => {
                        for branch in &mut branches {
                            let (condition, _, captures) = branch?;
                            self.verify_pattern_condition_writers(
                                condition.raw() as usize,
                                captures,
                            )?;
                        }
                    }
                    schema::ExprView::ListComp { mut qualifiers, .. }
                    | schema::ExprView::MapComp { mut qualifiers, .. } => {
                        for qualifier in &mut qualifiers {
                            if let decode::CompQualifierView::For { target, iter, .. } = qualifier? {
                                let item_type = self
                                    .checked_expression_type(iter.raw() as usize)?
                                    .and_then(|iter_type| iter_type.iteration_item_type());
                                self.verify_comp_target_writer_type(
                                    target,
                                    item_type.as_ref(),
                                )?;
                            }
                        }
                    }
                    _ => {}
                }
            }
            if instruction_kind != schema::InstructionKind::Stmt {
                continue;
            }
            match self.statement_view(instruction)? {
                schema::StmtView::Let { slot, value }
                | schema::StmtView::Assign { slot, value, .. }
                | schema::StmtView::DefaultParameter { slot, value, .. } => {
                    self.verify_callable_slot_initializer(slot, value.raw() as usize)?;
                }
                schema::StmtView::LetInt { slot, .. }
                | schema::StmtView::LetBool { slot, .. }
                | schema::StmtView::AssignInt { slot, .. }
                | schema::StmtView::AssignBool { slot, .. } => {
                    self.verify_callable_slot_without_checked_initializer(slot)?;
                }
                schema::StmtView::AssignField {
                    slot,
                    field,
                    value,
                    ..
                } => {
                    let path = [CallableAssignmentPathStep::Field(Name::intern(field))];
                    self.verify_callable_assignment_value(
                        slot,
                        &path,
                        value.raw() as usize,
                    )?;
                }
                schema::StmtView::AssignFieldInt { slot, field, .. } => {
                    let path = [CallableAssignmentPathStep::Field(Name::intern(field))];
                    if self
                        .callable_assignment_target_type(slot, &path)?
                        .is_some_and(|ty| ty.contains_typed_callable())
                    {
                        return Err(IrVerifyError::new(
                            "integer assignment cannot write a typed callable field",
                        ));
                    }
                }
                schema::StmtView::AssignPath {
                    slot,
                    mut path,
                    value,
                    ..
                } => {
                    let mut checked_path = Vec::new();
                    for step in &mut path {
                        checked_path.push(match step? {
                            decode::AssignStepView::Field(name) => {
                                CallableAssignmentPathStep::Field(name)
                            }
                            decode::AssignStepView::Index(_) => CallableAssignmentPathStep::Index,
                        });
                    }
                    self.verify_callable_assignment_value(
                        slot,
                        &checked_path,
                        value.raw() as usize,
                    )?;
                }
                schema::StmtView::LetRecord { source, target, .. } => {
                    let source_type = self.checked_expression_type(source.raw() as usize)?;
                    self.verify_comp_target_writer_type(target, source_type.as_ref())?;
                }
                schema::StmtView::For { slot, iter, .. } => {
                    let item_type = self
                        .checked_expression_type(iter.raw() as usize)?
                        .and_then(|iter_type| iter_type.iteration_item_type());
                    self.verify_callable_slot_writer_type(slot, item_type.as_ref())?;
                }
                schema::StmtView::ForStrLines { slot, text, .. } => {
                    let text_type = self.checked_expression_type(text.raw() as usize)?;
                    let item_type = text_type.map(|text_type| match text_type {
                        Type::Bytes => Type::Bytes,
                        _ => Type::Str,
                    });
                    self.verify_callable_slot_writer_type(slot, item_type.as_ref())?;
                }
                schema::StmtView::ForRecord { target, iter, .. } => {
                    let item_type = self
                        .checked_expression_type(iter.raw() as usize)?
                        .and_then(|iter_type| iter_type.iteration_item_type());
                    self.verify_comp_target_writer_type(target, item_type.as_ref())?;
                }
                schema::StmtView::ScanLines {
                    text_slot,
                    line_slot,
                    ..
                } => {
                    self.verify_callable_slot_writer_type(text_slot, None)?;
                    let text_type = self
                        .checked_slot_type(text_slot)?
                        .map(|type_id| self.store.semantic.to_type(type_id))
                        .transpose()?;
                    let line_type = text_type.and_then(|text_type| match text_type {
                        Type::Str => Some(Type::Str),
                        Type::Bytes => Some(Type::Bytes),
                        _ => None,
                    });
                    self.verify_callable_slot_writer_type(line_slot, line_type.as_ref())?;
                }
                schema::StmtView::Guard {
                    target,
                    value,
                    optional,
                    else_param_slot,
                    ..
                } => {
                    let value_type = self.checked_expression_type(value.raw() as usize)?;
                    let success_type = value_type.as_ref().and_then(|value_type| match value_type {
                        Type::Optional(inner) if optional => Some(inner.as_ref().clone()),
                        Type::Result(ok, _) if !optional => Some(ok.as_ref().clone()),
                        Type::Validated(validated) if !optional => match validated.base() {
                            Type::Result(ok, _) => Some(ok.as_ref().clone()),
                            _ => None,
                        },
                        _ => None,
                    });
                    self.verify_comp_target_writer_type(target, success_type.as_ref())?;
                    if let Some(slot) = else_param_slot {
                        let failure_type = value_type
                            .as_ref()
                            .and_then(|value_type| {
                                Self::checked_result_payload_type(Some(value_type), false)
                            });
                        self.verify_callable_slot_writer_type(slot, failure_type.as_ref())?;
                    }
                }
                schema::StmtView::With {
                    mut bindings,
                    else_param_slot,
                    ..
                } => {
                    for binding in &mut bindings {
                        let (slot, value) = binding?;
                        let value = value.raw() as usize;
                        let value_type = self.checked_expression_type(value)?;
                        let bound_type = value_type.map(|value_type| match value_type {
                            Type::Result(ok, _) => ok.as_ref().clone(),
                            other => other,
                        });
                        self.verify_callable_slot_writer_type(slot, bound_type.as_ref())?;
                    }
                    if let Some(slot) = else_param_slot {
                        let error_type = with_error_types.get(&instruction).ok_or_else(|| {
                            IrVerifyError::new("checked With error type is missing")
                        })?;
                        self.verify_callable_slot_writer_type(slot, Some(error_type))?;
                    }
                }
                schema::StmtView::Match { value, mut arms, .. } => {
                    let subject_type = self.checked_expression_type(value.raw() as usize)?;
                    for arm in &mut arms {
                        let (pattern, _, _) = arm?;
                        self.verify_pattern_writer_types(pattern.raw(), subject_type.as_ref())?;
                    }
                }
                schema::StmtView::PatternIf { mut branches, .. } => {
                    for branch in &mut branches {
                        let (condition, _, captures) = branch?;
                        self.verify_pattern_condition_writers(
                            condition.raw() as usize,
                            captures,
                        )?;
                    }
                }
                schema::StmtView::PatternWhile {
                    condition,
                    captures,
                    ..
                } => {
                    self.verify_pattern_condition_writers(condition.raw() as usize, captures)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn expression_checked_type_id(
        &self,
        instruction: usize,
    ) -> Result<Option<TypeId>, IrVerifyError> {
        if let Some(ty) = self.checked_expression_type_id(instruction)? {
            return Ok(Some(ty));
        }
        let Some(tag) = self.store.tags.get(instruction).copied() else {
            return Err(IrVerifyError::new("typed expression is out of bounds"));
        };
        let words = self.store.payload(self.store.data[instruction].range())?;
        match tag {
            FullTag::ExprParam => {
                let slot = *words
                    .first()
                    .ok_or_else(|| IrVerifyError::new("typed parameter slot is missing"))?
                    as usize;
                self.checked_slot_type(slot)
            }
            FullTag::ExprField => match self.derive_typed_callee_type(instruction) {
                Ok(type_id) => Ok(Some(type_id)),
                Err(error) => {
                    let schema::ExprView::Field { base, .. } =
                        self.expression_view(instruction)?
                    else {
                        unreachable!("field instruction tag and generated view disagree")
                    };
                    if self
                        .checked_expression_type(base.raw() as usize)?
                        .is_none()
                    {
                        return Ok(None);
                    }
                    let field_type = self.derive_checked_field_type(instruction)?;
                    if field_type.contains_typed_callable() {
                        Err(error)
                    } else {
                        Ok(None)
                    }
                }
            },
            FullTag::ExprCall | FullTag::ExprDirectPureCall => {
                let function = IrFunctionId::from_raw(*words.first().ok_or_else(|| {
                    IrVerifyError::new("typed call target is missing")
                })?)
                .ok_or_else(|| IrVerifyError::new("typed call target is invalid"))?;
                self.function_return_type(function.index()).map(Some)
            }
            FullTag::ExprSelfCall => IrFunctionId::from_raw(self.owner)
                .ok_or_else(|| IrVerifyError::new("self call has no function owner"))
                .and_then(|function| self.function_return_type(function.index()))
                .map(Some),
            FullTag::ExprTypedCall => {
                let raw = *words
                    .get(2)
                    .ok_or_else(|| IrVerifyError::new("typed call signature is missing"))?;
                let ty = TypeId::from_raw(raw)
                    .ok_or_else(|| IrVerifyError::new("typed call signature is invalid"))?;
                let signature = self.store.semantic.typed_callable_signature(ty)?;
                self.store
                    .semantic
                    .signature_return_type(signature)
                    .map(Some)
            }
            _ => Ok(None),
        }
    }

    fn function_return_type(&self, function: usize) -> Result<TypeId, IrVerifyError> {
        let function = self
            .store
            .functions
            .get(function)
            .ok_or_else(|| IrVerifyError::new("typed callee function is out of bounds"))?;
        if function.checked_return_type != IR_NONE {
            let ty = TypeId::from_raw(function.checked_return_type)
                .ok_or_else(|| IrVerifyError::new("typed callee checked return type is invalid"))?;
            self.store.semantic.type_tag(ty)?;
            return Ok(ty);
        }
        self.store
            .semantic
            .signature_return_type(function.signature)
    }

    /// A retry selects errors without binding them, and a backoff's delays
    /// are its first interval, its cap, and its limit: the executor reads
    /// exactly those three, so any other count would run a schedule nobody
    /// wrote.
    fn verify_retry_shape(&self, mut payload: FullCursor<'_>) -> Result<(), IrVerifyError> {
        // The row's own fields are verified already, which settled who owns
        // the delay block; this only reads its length.
        let delays = IrBlockId::from_raw(payload.raw()?)
            .and_then(|id| self.store.blocks.get(id.index()).copied())
            .ok_or_else(|| IrVerifyError::new("retry delay block is invalid"))?;
        let delays = self
            .cursor(self.store.payload(delays.instructions)?)
            .raw()?;
        if bool::decode(self, &mut payload)?
            && !self
                .pattern_capture_slots(payload.raw()? as usize)?
                .is_empty()
        {
            return Err(IrVerifyError::new(
                "retry selection pattern cannot bind slots",
            ));
        }
        payload.raw()?;
        Span::decode(self, &mut payload)?;
        let schedule = crate::syntax::arena::RetrySchedule::decode(self, &mut payload)?;
        if schedule == crate::syntax::arena::RetrySchedule::Backoff && delays != 3 {
            return Err(IrVerifyError::new(
                "retry backoff must have a first interval, a cap, and a limit",
            ));
        }
        Ok(())
    }

    /// An optional guard fails on `null`, which carries no error, so it has
    /// no slot to bind one to. Executing such a row would leave that slot
    /// holding whatever an earlier statement stored there.
    fn verify_guard_shape(&self, mut payload: FullCursor<'_>) -> Result<(), IrVerifyError> {
        LoweredCompTarget::decode(self, &mut payload)?;
        payload.raw()?;
        let else_param_slot = Option::<usize>::decode(self, &mut payload)?;
        payload.raw()?;
        Span::decode(self, &mut payload)?;
        if bool::decode(self, &mut payload)? && else_param_slot.is_some() {
            return Err(IrVerifyError::new(
                "optional guard cannot bind a failure parameter",
            ));
        }
        Ok(())
    }

    /// A With binding and its error parameter are distinct source bindings,
    /// even though only one branch writes each slot at runtime.
    fn verify_with_shape(&self, instruction: usize) -> Result<(), IrVerifyError> {
        let schema::StmtView::With {
            mut bindings,
            else_param_slot,
            ..
        } = self.statement_view(instruction)?
        else {
            return Err(IrVerifyError::new("With instruction tag and view disagree"));
        };
        let mut slots = BTreeSet::new();
        for binding in &mut bindings {
            let (slot, _) = binding?;
            if !slots.insert(slot) {
                return Err(IrVerifyError::new(
                    "With success bindings share a source slot",
                ));
            }
        }
        if let Some(slot) = else_param_slot
            && !slots.insert(slot)
        {
            return Err(IrVerifyError::new(
                "With success and error bindings share a source slot",
            ));
        }
        Ok(())
    }

    fn finish_instruction(&self, index: usize) {
        if let Some(proof) = &self.instruction_proof {
            proof.borrow_mut().finish(index);
        }
        if let Some(states) = &self.instruction_states {
            states.borrow_mut()[index - self.instruction_range.start] = 2;
        }
    }

    fn copy_in_place_marks_into(&self, output: &mut [bool]) -> Result<(), IrVerifyError> {
        let complete = self
            .instruction_proof
            .as_ref()
            .is_none_or(|proof| proof.borrow().copy_marks_into(output));
        if complete {
            Ok(())
        } else {
            Err(IrVerifyError::new(
                "full IR in-place proof did not cover its instruction range",
            ))
        }
    }

    fn finish_function(&self) -> Result<(), IrVerifyError> {
        let instructions_complete = self
            .instruction_states
            .as_ref()
            .is_none_or(|states| states.borrow().iter().all(|state| *state == 2));
        let blocks_complete = self
            .block_states
            .as_ref()
            .is_none_or(|states| states.finished_owned.get() == states.owned);
        let proof_complete = self
            .instruction_proof
            .as_ref()
            .is_none_or(|proof| proof.borrow().complete());
        if instructions_complete && blocks_complete && proof_complete {
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

/// The literal truth value of a lowered branch condition instruction, when it
/// is a boolean literal (`BoolBool` for bool chains, `ExprBool` for
/// expression conditions). Any other instruction is not a literal.
fn indexed_condition_bool_literal(
    store: &FullStore,
    instruction: u32,
) -> Result<Option<bool>, IrVerifyError> {
    let tag = store
        .tags
        .get(instruction as usize)
        .copied()
        .ok_or_else(|| IrVerifyError::new("return-analysis condition is out of bounds"))?;
    if !matches!(tag, FullTag::BoolBool | FullTag::ExprBool) {
        return Ok(None);
    }
    let data = *store
        .data
        .get(instruction as usize)
        .ok_or_else(|| IrVerifyError::new("return-analysis condition data is out of bounds"))?;
    match store.payload(data.range())? {
        [0] => Ok(Some(false)),
        [1] => Ok(Some(true)),
        _ => Err(IrVerifyError::new(
            "return-analysis condition literal is invalid",
        )),
    }
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
            // A literal-false condition never runs its branch; a literal-true
            // one makes every later branch and the else unreachable, so the
            // chain's completion is that branch's.
            let mut chain_can_return = true;
            let mut chain_consumed = false;
            for _ in 0..len {
                let Some((&condition, rest)) = branch_words.split_first() else {
                    return Err(IrVerifyError::new(
                        "return-analysis if condition is missing",
                    ));
                };
                let Some((&body, rest)) = rest.split_first() else {
                    return Err(IrVerifyError::new("return-analysis if body is missing"));
                };
                if !chain_consumed {
                    match indexed_condition_bool_literal(store, condition)? {
                        Some(false) => {}
                        Some(true) => {
                            chain_can_return = indexed_block_can_return(store, block(body)?)?;
                            chain_consumed = true;
                        }
                        None => {
                            if !indexed_block_can_return(store, block(body)?)? {
                                chain_can_return = false;
                            }
                        }
                    }
                }
                branch_words = if tag == FullTag::StmtPatternIf {
                    rest.get(1..)
                        .ok_or_else(|| IrVerifyError::new("return-analysis captures are missing"))?
                } else {
                    rest
                };
            }
            if !branch_words.is_empty() {
                return Err(IrVerifyError::new(
                    "return-analysis if branches have trailing data",
                ));
            }
            let else_payload = if tag == FullTag::StmtPatternIf {
                payload
                    .get(1..payload.len().saturating_sub(1))
                    .unwrap_or_default()
            } else {
                payload.get(1..).unwrap_or_default()
            };
            let else_returns = match else_payload {
                [1, body] => indexed_block_can_return(store, block(*body)?)?,
                [0] => false,
                _ => {
                    return Err(IrVerifyError::new(
                        "return-analysis else payload is invalid",
                    ));
                }
            };
            if chain_consumed {
                chain_can_return
            } else {
                chain_can_return && else_returns
            }
        }
        FullTag::StmtWith => {
            let body = *payload
                .get(1)
                .ok_or_else(|| IrVerifyError::new("return-analysis with body is missing"))?;
            let else_index = match payload.get(2) {
                Some(0) => 3,
                Some(1) => 4,
                _ => {
                    return Err(IrVerifyError::new(
                        "return-analysis with parameter is invalid",
                    ));
                }
            };
            let else_body = *payload
                .get(else_index)
                .ok_or_else(|| IrVerifyError::new("return-analysis with handler is missing"))?;
            indexed_block_can_return(store, block(body)?)?
                && indexed_block_can_return(store, block(else_body)?)?
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
    fn verify_resource_transfer_sites(store: &FullStore) -> Result<(), IrVerifyError> {
        if store.resource_transfers.len() != store.tags.len() {
            return Err(IrVerifyError::new(
                "resource-transfer metadata does not match instruction count",
            ));
        }
        for (&tag, &transfers_resource) in store.tags.iter().zip(&store.resource_transfers) {
            if transfers_resource && !is_resource_transfer_instruction(tag) {
                return Err(IrVerifyError::new(
                    "resource-transfer metadata is attached to an invalid instruction",
                ));
            }
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
        }
        Ok(())
    }

    fn verify(program: &FullProgram) -> Result<(), IrVerifyError> {
        let _symbols = program.symbol_owner().enter();
        let store = &program.store;
        Self::verify_resource_transfer_sites(store)?;
        for plan in &store.prepared_module_call_plans {
            match plan.as_ref() {
                crate::runtime::eval::ModuleCallPlan::SignatureCli(_) => {}
                crate::runtime::eval::ModuleCallPlan::CommandArguments(plan) => {
                    if !crate::modules::api_spec().module_entries().any(|(_, module)| {
                        module.functions.iter().any(|function| function.overloads.iter().any(|registered| std::ptr::eq(registered, plan.signature)))
                    }) {
                        return Err(IrVerifyError::new("module command signature is not a registered module overload"));
                    }
                }
            }
        }
        let mut wire_types = rustc_hash::FxHashSet::default();
        for mapping in &store.wire_enums {
            if mapping.variants.is_empty()
                || mapping
                    .variants
                    .values()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != mapping.variants.len()
            {
                return Err(IrVerifyError::new(
                    "wire enum mapping is empty or has duplicate strings",
                ));
            }
            if !wire_types.insert(mapping.type_name) {
                return Err(IrVerifyError::new(
                    "wire enum declaring identity has multiple mappings",
                ));
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
        for (key, schema) in &store.checked_pattern_variants {
            if key.owner.as_str().is_empty() || key.variant.as_str().is_empty() {
                return Err(IrVerifyError::new(
                    "checked nominal pattern schema has an empty identity",
                ));
            }
            let Some(declared) = store.declared_pattern_variants.get(key) else {
                return Err(IrVerifyError::new(
                    "checked nominal pattern schema has no canonical declaration",
                ));
            };
            if schema != declared {
                return Err(IrVerifyError::new(
                    "checked nominal pattern schema disagrees with its canonical declaration",
                ));
            }
            let mut error_fields = BTreeSet::new();
            for field in &schema.fields {
                match (key.kind, field.name) {
                    (CheckedPatternVariantKind::Tag, None) => {}
                    (CheckedPatternVariantKind::Error, Some(name))
                        if error_fields.insert(name) => {}
                    _ => {
                        return Err(IrVerifyError::new(
                            "checked nominal pattern field schema has an invalid name",
                        ));
                    }
                }
                store.semantic.to_type(field.type_id)?;
            }
        }
        if store.checked_pattern_variants.len() != store.declared_pattern_variants.len() {
            return Err(IrVerifyError::new(
                "canonical nominal pattern declarations do not match checked pattern schemas",
            ));
        }
        let function_signature_index = Arc::new(FunctionSignatureIndex::build(store)?);
        let mut previous_checked_expression = None;
        for fact in &store.checked_expr_types {
            let instruction = fact.instruction as usize;
            if instruction >= store.tags.len()
                || previous_checked_expression.is_some_and(|previous| previous >= instruction)
                || store.tags[instruction].kind() != schema::InstructionKind::Expr
            {
                return Err(IrVerifyError::new(
                    "checked expression type rows are not strictly ordered expression facts",
                ));
            }
            let ty = store.semantic.to_type(fact.type_id)?;
            let structured_error = matches!(ty, Type::ErrorVariant { .. });
            if structured_error != (store.tags[instruction] == FullTag::ExprError) {
                return Err(IrVerifyError::new(
                    "checked error-variant type row does not match its structured-error producer",
                ));
            }
            if !ty.contains_typed_callable() && !fact.with_error_source && !structured_error {
                return Err(IrVerifyError::new(
                    "checked expression type row is neither callable, a structured error, nor a With error source",
                ));
            }
            previous_checked_expression = Some(instruction);
        }
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
        let mut in_place_marks = vec![false; store.tags.len()];
        let owned_blocks = owned_block_counts(store);
        let mut previous_slot_type_fact_end = 0usize;
        for (index, function) in store.functions.iter().enumerate() {
            store.string(function.name)?;
            let metadata = store.function_metadata[index];
            if metadata.owner != IR_NONE {
                store.string(metadata.owner)?;
            }
            if function.checked_return_type != IR_NONE {
                let ty = TypeId::from_raw(function.checked_return_type).ok_or_else(|| {
                    IrVerifyError::new("function checked return type id is invalid")
                })?;
                let signature_return = store
                    .semantic
                    .signature_return_type(function.signature)?;
                let checked_return = store.semantic.to_type(ty)?;
                let declared_return = store.semantic.to_type(signature_return)?;
                // A body may produce a narrower value than the public return contract.
                if !checked_return.matches_expected(&declared_return) {
                    return Err(IrVerifyError::new(
                        "checked return type disagrees with the function signature",
                    ));
                }
            }
            if metadata.flags & !0b1 != 0 {
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
            let slot_type_facts = function
                .slot_type_facts
                .bounds(store.slot_type_facts.len())
                .ok_or_else(|| IrVerifyError::new("function slot type fact range is invalid"))?;
            if slot_type_facts.start != previous_slot_type_fact_end
                || slot_type_facts.len() != function.slot_count as usize
            {
                return Err(IrVerifyError::new(
                    "function slot type fact ranges are not dense and slot-complete",
                ));
            }
            previous_slot_type_fact_end = slot_type_facts.end;
            for raw in &store.slot_type_facts[slot_type_facts.clone()] {
                if *raw == IR_NONE {
                    continue;
                }
                let ty = TypeId::from_raw(*raw).ok_or_else(|| {
                    IrVerifyError::new("function slot type fact id is invalid")
                })?;
                store.semantic.to_type(ty)?;
            }
            let signature_param_count = store.semantic.signature_param_count(function.signature)?;
            if signature_param_count != params.len() {
                return Err(IrVerifyError::new(
                    "function parameters do not match its signature",
                ));
            }
            for param in &store.params[params.clone()] {
                store.string(param.name)?;
                store.semantic.type_tag(param.type_id)?;
                if param.flags & !0b111 != 0 {
                    return Err(IrVerifyError::new("parameter flags are invalid"));
                }
            }
            for capture in &store.captures[captures.clone()] {
                store.string(capture.name)?;
                store.semantic.type_tag(capture.type_id)?;
                if capture.slot_and_flags & !(1 << 31) >= function.slot_count {
                    return Err(IrVerifyError::new("capture slot is out of bounds"));
                }
            }
            let instruction_len = instructions.len();
            let owner = IrFunctionId::new(index)
                .map_err(|_| IrVerifyError::new("function id is invalid"))?
                .raw();
            let decoder = FullDecoder {
                store,
                owner,
                instruction_range: instructions.clone(),
                instruction_states: Some(RefCell::new(vec![0; instruction_len])),
                instruction_proof: Some(RefCell::new(in_place::InstructionProof::new(
                    instructions.clone(),
                ))),
                block_states: Some(OwnedBlockStates::new(owner, &owned_blocks)),
                slot_count: function.slot_count,
                pattern_ceiling: Cell::new(usize::MAX),
                local_immutable_let_initializers: RefCell::default(),
                function_signature_index: Some(function_signature_index.clone()),
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
            decoder.verify_typed_calls_after_structure()?;
            decoder.verify_checked_expression_facts()?;
            decoder.verify_callable_slot_initializers()?;
            decoder.copy_in_place_marks_into(&mut in_place_marks)?;
            let body_payload = store.payload(body_block.instructions)?;
            let mut default_slots = BTreeSet::new();
            let mut entry_open = true;
            let mut previous_default = None;
            for &instruction in body_payload.iter().skip(1) {
                let tag = store.tags[instruction as usize];
                if tag != FullTag::StmtDefaultParameter {
                    entry_open = false;
                    continue;
                }
                if !entry_open {
                    return Err(IrVerifyError::new(
                        "parameter defaults must precede the callable body",
                    ));
                }
                let words = store.payload(store.data[instruction as usize].range())?;
                let slot = words[0] as usize;
                if slot >= params.len()
                    || store.params[params.start + slot].flags & 4 == 0
                    || previous_default.is_some_and(|previous| slot <= previous)
                {
                    return Err(IrVerifyError::new(
                        "parameter default entry does not match its parameter",
                    ));
                }
                previous_default = Some(slot);
                default_slots.insert(slot);
            }
            let entry_count = store.tags[decoder.instruction_range.clone()]
                .iter()
                .filter(|tag| **tag == FullTag::StmtDefaultParameter)
                .count();
            if entry_count != default_slots.len() {
                return Err(IrVerifyError::new(
                    "parameter default entry must belong to its callable prefix",
                ));
            }
            for (slot, param) in store.params[params.clone()].iter().enumerate() {
                if param.flags & 4 != 0 {
                    if param.flags & 2 == 0
                        || param.flags & 1 != 0
                        || !default_slots.contains(&slot)
                    {
                        return Err(IrVerifyError::new(
                            "expression default metadata requires a non-rest defaulted parameter entry",
                        ));
                    }
                    if store
                        .param_cold
                        .binary_search_by_key(&((params.start + slot) as u32), |cold| cold.param)
                        .ok()
                        .is_some_and(|index| store.param_cold[index].default != IR_NONE)
                    {
                        return Err(IrVerifyError::new(
                            "expression defaults cannot also carry a prepared literal",
                        ));
                    }
                }
            }
            let return_type = store
                .semantic
                .to_type(store.semantic.signature_return_type(function.signature)?)?;
            // A stream producer's body ending ends the stream, so the empty
            // body is the degenerate empty stream rather than a missing return.
            if body_payload.first().copied() == Some(0) && !matches!(return_type, Type::Stream(_)) {
                return Err(IrVerifyError::new(format!(
                    "function {index} has an empty body"
                )));
            }
            if !matches!(return_type, Type::Stream(_)) && !indexed_block_can_return(store, body_id)?
            {
                return Err(IrVerifyError::new(format!(
                    "function {index} body does not terminate with a return"
                )));
            }
        }
        if previous_slot_type_fact_end != store.slot_type_facts.len() {
            return Err(IrVerifyError::new(
                "unowned function callable slot type rows",
            ));
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
                    let temporary = slot.flags & DRIVER_SLOT_TEMPORARY != 0;
                    if !temporary {
                        store.string(slot.name)?;
                    }
                    store.semantic.type_tag(slot.type_id)?;
                    if slot.reserved != [0; 3]
                        || slot.flags
                            & !(DRIVER_SLOT_READ
                                | DRIVER_SLOT_WRITE
                                | DRIVER_SLOT_MUTABLE
                                | DRIVER_SLOT_TEMPORARY)
                            != 0
                        || slot.flags & DRIVER_SLOT_READ == 0
                        || slot.slot >= step.slot_count
                        || (temporary
                            && (slot.name != IR_NONE
                                || slot.flags & (DRIVER_SLOT_WRITE | DRIVER_SLOT_MUTABLE) != 0))
                        || (!temporary && slot.name == IR_NONE)
                    {
                        return Err(IrVerifyError::new("driver slot is invalid"));
                    }
                    if (!temporary && !names.insert(slot.name)) || !indices.insert(slot.slot) {
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
                            if slot.flags & DRIVER_SLOT_TEMPORARY != 0 {
                                continue;
                            }
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
            program.verify_driver(
                &owned_blocks,
                &mut in_place_marks,
                Arc::clone(&function_signature_index),
            )?;
        }
        if previous_end != store.tags.len() {
            return Err(IrVerifyError::new(
                "full IR contains instructions outside executable owner ranges",
            ));
        }
        let marks = store.in_place.get_or_init(|| in_place_marks.clone());
        if marks != &in_place_marks {
            return Err(IrVerifyError::new(
                "full IR in-place mark cache does not match verifier proof",
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
impl_word_codec!(
    crate::syntax::arena::ContextScopeKind,
    |value: &crate::syntax::arena::ContextScopeKind| Ok(match value {
        crate::syntax::arena::ContextScopeKind::Cwd => 0,
        crate::syntax::arena::ContextScopeKind::Env => 1,
        crate::syntax::arena::ContextScopeKind::Within => 2,
    }),
    |raw: Result<u32, IrVerifyError>| raw.and_then(|raw| match raw {
        0 => Ok(crate::syntax::arena::ContextScopeKind::Cwd),
        1 => Ok(crate::syntax::arena::ContextScopeKind::Env),
        2 => Ok(crate::syntax::arena::ContextScopeKind::Within),
        _ => Err(IrVerifyError::new("context scope kind is invalid")),
    })
);
impl_word_codec!(
    crate::syntax::arena::RetrySchedule,
    |value: &crate::syntax::arena::RetrySchedule| Ok(match value {
        crate::syntax::arena::RetrySchedule::Delays => 0,
        crate::syntax::arena::RetrySchedule::Backoff => 1,
    }),
    |raw: Result<u32, IrVerifyError>| raw.and_then(|raw| match raw {
        0 => Ok(crate::syntax::arena::RetrySchedule::Delays),
        1 => Ok(crate::syntax::arena::RetrySchedule::Backoff),
        _ => Err(IrVerifyError::new("retry schedule is invalid")),
    })
);
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

impl FullCodec for crate::runtime::eval::EndDistance {
    fn encode(
        &self,
        _builder: &mut FullBuilder,
        output: &mut Vec<u32>,
    ) -> Result<(), IrBuildError> {
        output.push(self.get() as u32);
        Ok(())
    }

    // A distance of zero would read one past the end of every list, so a
    // program that carries one was not produced by lowering.
    fn decode(
        _decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        Self::new(input.raw()?)
            .ok_or_else(|| IrVerifyError::new("an index from the end counts from one"))
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
        impl decode::VectorElement for $ty {
            const BLOCK_FLAGS: u8 = $flags;
        }

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
    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        decoder
            .store
            .prepared_constants
            .get(input.raw()? as usize)
            .cloned()
            .ok_or_else(|| IrVerifyError::new("prepared constant is out of bounds"))
    }
    fn verify(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>) -> Result<(), IrVerifyError> {
        let value = decoder
            .store
            .prepared_constants
            .get(input.raw()? as usize)
            .ok_or_else(|| IrVerifyError::new("prepared constant is out of bounds"))?;
        if !prepared_constant_is_data(&value.0, 0) {
            return Err(IrVerifyError::new(
                "prepared constant contains a runtime value",
            ));
        }
        if !visit_value_wire_mappings(&value.0, &mut |mapping| {
            wire_mapping_matches_pool(mapping, decoder.store)
        }) {
            return Err(IrVerifyError::new(
                "prepared constant has a contradictory wire mapping",
            ));
        }
        Ok(())
    }
}

// Constructors, constants, and schema conversions share one declaring mapping.
// Verification rejects independently altered copies before they can disagree at a boundary.
fn register_wire_mapping(
    builder: &mut FullBuilder,
    mapping: &Arc<crate::sema::wire_enums::WireEnumMapping>,
) {
    if !builder
        .store
        .wire_enums
        .iter()
        .any(|stored| Arc::ptr_eq(stored, mapping))
    {
        builder.store.wire_enums.push(mapping.clone());
    }
}

fn wire_mapping_matches_pool(
    mapping: &Arc<crate::sema::wire_enums::WireEnumMapping>,
    store: &FullStore,
) -> bool {
    store
        .wire_enums
        .iter()
        .find(|stored| stored.type_name == mapping.type_name)
        .is_some_and(|stored| stored.as_ref() == mapping.as_ref())
}

fn visit_value_wire_mappings(
    value: &LoweredValue,
    visit: &mut impl FnMut(&Arc<crate::sema::wire_enums::WireEnumMapping>) -> bool,
) -> bool {
    match value {
        LoweredValue::Tag(tag) => {
            tag.wire.as_ref().is_none_or(&mut *visit)
                && tag
                    .fields
                    .iter()
                    .all(|value| visit_value_wire_mappings(value, visit))
        }
        LoweredValue::List(values) => values
            .iter()
            .all(|value| visit_value_wire_mappings(value, visit)),
        LoweredValue::SharedList(values) => values
            .iter()
            .all(|value| visit_value_wire_mappings(value, visit)),
        LoweredValue::Record(values) => values
            .values()
            .all(|value| visit_value_wire_mappings(value, visit)),
        LoweredValue::Map(values) => values
            .values()
            .all(|value| visit_value_wire_mappings(value, visit)),
        _ => true,
    }
}

impl FullCodec for Arc<crate::runtime::eval::ModuleCallPlan> {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        let index = if let Some(index) = builder
            .store
            .prepared_module_call_plans
            .iter()
            .position(|plan| plan.same_identity(self))
        {
            index
        } else {
            let index = builder.store.prepared_module_call_plans.len();
            builder.store.prepared_module_call_plans.push(Arc::clone(self));
            index
        };
        output.push(
            u32::try_from(index)
                .map_err(|_| IrBuildError::format("module call plan pool overflow", None, 0, 0))?,
        );
        Ok(())
    }
    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        decoder
            .store
            .prepared_module_call_plans
            .get(input.raw()? as usize)
            .cloned()
            .ok_or_else(|| IrVerifyError::new("prepared module call plan is out of bounds"))
    }
}

fn prepared_constant_is_data(value: &LoweredValue, depth: usize) -> bool {
    if depth > 128 {
        return false;
    }
    match value {
        LoweredValue::Null
        | LoweredValue::Bool(_)
        | LoweredValue::Int(_)
        | LoweredValue::Float(_)
        | LoweredValue::Duration(_)
        | LoweredValue::Str(_)
        | LoweredValue::Bytes(_)
        | LoweredValue::Path(_)
        | LoweredValue::Regex(_)
        | LoweredValue::Set(_) => true,
        LoweredValue::List(values) => values
            .iter()
            .all(|value| prepared_constant_is_data(value, depth + 1)),
        LoweredValue::SharedList(values) => values
            .iter()
            .all(|value| prepared_constant_is_data(value, depth + 1)),
        LoweredValue::Record(values) => values
            .values()
            .all(|value| prepared_constant_is_data(value, depth + 1)),
        LoweredValue::Map(values) => values
            .values()
            .all(|value| prepared_constant_is_data(value, depth + 1)),
        LoweredValue::Tag(value) => {
            value
                .fields
                .iter()
                .all(|value| prepared_constant_is_data(value, depth + 1))
                && value.wire.as_ref().is_none_or(|mapping| {
                    value.fields.is_empty()
                        && mapping.type_name == value.type_name
                        && mapping
                            .variants
                            .contains_key(&Name::intern(value.name.as_ref()))
                        && !mapping.variants.is_empty()
                        && mapping
                            .variants
                            .values()
                            .collect::<std::collections::BTreeSet<_>>()
                            .len()
                            == mapping.variants.len()
                })
        }
        _ => false,
    }
}

impl FullCodec for Arc<crate::sema::wire_enums::WireEnumMapping> {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        let index = if let Some(index) = builder
            .store
            .wire_enums
            .iter()
            .position(|mapping| Arc::ptr_eq(mapping, self))
        {
            index
        } else {
            let index = builder.store.wire_enums.len();
            builder.store.wire_enums.push(self.clone());
            index
        };
        output.push(
            u32::try_from(index)
                .map_err(|_| IrBuildError::format("wire enum pool overflow", None, 0, 0))?,
        );
        Ok(())
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        let mapping = decoder
            .store
            .wire_enums
            .get(input.raw()? as usize)
            .cloned()
            .ok_or_else(|| IrVerifyError::new("wire enum mapping is out of bounds"))?;
        if !decoder.verified
            && (mapping.variants.is_empty()
                || mapping
                    .variants
                    .values()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != mapping.variants.len())
        {
            return Err(IrVerifyError::new(
                "wire enum mapping is empty or has duplicate strings",
            ));
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

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        decoder
            .store
            .prepared_regexes
            .get(input.raw()? as usize)
            .cloned()
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
        lowered_type_to_type(*self).encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        lowered_type_from_type(&Type::decode(decoder, input)?)
    }
}

impl FullCodec for Arc<super::super::require::PreparedSchema> {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        let index = if let Some(index) = builder
            .store
            .prepared_schemas
            .iter()
            .position(|schema| Arc::ptr_eq(schema, self))
        {
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
        output.push(
            u32::try_from(index)
                .map_err(|_| IrBuildError::format("schema pool overflow", None, 0, 0))?,
        );
        Ok(())
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        let schema = decoder
            .store
            .prepared_schemas
            .get(input.raw()? as usize)
            .cloned()
            .ok_or_else(|| IrVerifyError::new("prepared schema is out of bounds"))?;
        if !decoder.verified
            && (!schema.valid()
                || !schema.visit_wire_mappings(&mut |mapping| {
                    wire_mapping_matches_pool(mapping, decoder.store)
                }))
        {
            return Err(IrVerifyError::new(
                "prepared schema has an invalid or contradictory wire mapping",
            ));
        }
        Ok(schema)
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
    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        let value = LoweredValue::decode(decoder, input)?;
        crate::runtime::eval::lowered_ops::lowered_map_key_ref(
            &value,
            crate::source::Span::at(crate::source::SourceId::new(0), 0),
        )
        .map(|key| key.to_owned())
        .map_err(|_| IrVerifyError::new("map payload contains an invalid scalar key"))
    }
}
impl_btree_codec!(Arc<str>, LoweredValue);

/// The elements of a constant set. Each is a scalar key, and a set holds an
/// element once, so a payload that repeats one does not describe a set.
fn decode_set_payload(
    decoder: &FullDecoder<'_>,
    payload: &mut FullCursor<'_>,
) -> Result<BTreeSet<crate::map_key::MapKey>, IrVerifyError> {
    let len = payload.raw()? as usize;
    let mut elements = BTreeSet::new();
    for _ in 0..len {
        if !elements.insert(crate::map_key::MapKey::decode(decoder, payload)?) {
            return Err(IrVerifyError::new(
                "set payload contains a duplicate element",
            ));
        }
    }
    Ok(elements)
}

lowered_value_schema! {
    context { builder, decoder, payload }
    rows {
        Null = 0 {
            fields {}
            encode { Self::Null => {} }
            decode Self::Null
        }
        Unit = 1 {
            fields {}
            encode { Self::Unit => {} }
            decode Self::Unit
        }
        Int = 2 {
            fields { value: i64 }
            encode { Self::Int(value) => {} }
            decode Self::Int(value)
        }
        Float = 3 {
            fields { value: FloatValue }
            encode { Self::Float(value) => {} }
            decode Self::Float(value)
        }
        Duration = 4 {
            fields { value: DurationValue }
            encode { Self::Duration(value) => {} }
            decode Self::Duration(value)
        }
        Bool = 5 {
            fields { value: bool }
            encode { Self::Bool(value) => {} }
            decode Self::Bool(value)
        }
        Str = 6 {
            fields { value: Arc<str> }
            encode { Self::Str(value) => {} }
            decode Self::Str(value)
        }
        Bytes = 7 {
            fields { value: Arc<[u8]> }
            encode { Self::Bytes(value) => {} }
            decode Self::Bytes(value)
        }
        Regex = 8 {
            fields { value: RegexValue }
            encode { Self::Regex(value) => { let value = value.as_ref(); } }
            decode Self::Regex(Box::new(value))
        }
        Path = 9 {
            fields { value: PathValue }
            encode { Self::Path(value) => {} }
            decode Self::Path(value)
        }
        Record = 10 {
            fields { value: BTreeMap<Arc<str>, LoweredValue> }
            encode { Self::Record(value) => { let value = value.as_ref(); } }
            decode Self::Record(Arc::new(value))
        }
        RecordVec = 11 {
            fields { value: Vec<(Name, LoweredValue)> }
            encode { Self::RecordVec(value) => { let value = value.as_ref(); } }
            decode Self::RecordVec(Arc::new(value))
        }
        Stats = 12 {
            fields { blanks: i64, code: i64, comments: i64 }
            encode {
                Self::Stats { blanks, code, comments } => {}
            }
            decode Self::Stats { blanks, code, comments }
        }
        // Owned decoding reconstructs MapKey keys; verification preserves the legacy string-key check.
        StatsBlob = 13 {
            fields {
                blanks: i64,
                blobs: BTreeMap<crate::map_key::MapKey, LoweredValue> => BTreeMap<String, LoweredValue>,
                code: i64,
                comments: i64,
            }
            encode {
                Self::StatsBlob(value) => {
                    let LoweredStatsValue { blanks, blobs, code, comments } = value.as_ref();
                }
            }
            decode Self::StatsBlob(Box::new(LoweredStatsValue { blanks, blobs, code, comments }))
        }
        Module = 14 {
            fields { value: BTreeMap<Arc<str>, LoweredValue> }
            encode { Self::Module(value) => { let value = value.as_ref(); } }
            decode Self::Module(Arc::new(value))
        }
        List = 15 {
            fields { value: Vec<LoweredValue> }
            encode { Self::List(value) => {} }
            decode Self::List(value)
        }
        Map = 16 {
            fields { value: BTreeMap<crate::map_key::MapKey, LoweredValue> }
            encode { Self::Map(value) => { let value = value.as_ref(); } }
            decode Self::Map(Arc::new(value))
        }
        Tag = 17 {
            fields {
                type_name: Name,
                wire: Option<Arc<crate::sema::wire_enums::WireEnumMapping>>,
                name: Arc<str>,
                fields: Vec<LoweredValue>,
            }
            encode {
                Self::Tag(value) => {
                    let LoweredTagValue { type_name, wire, name, fields } = value.as_ref();
                }
            }
            decode Self::Tag(Box::new(LoweredTagValue { type_name, wire, name, fields }))
        }
        ResultOk = 18 {
            fields { value: LoweredValue }
            encode { Self::ResultOk(value) => { let value = value.as_ref(); } }
            decode Self::ResultOk(Box::new(value))
        }
    }
    aliases {
        Self::StrView(value) => Str { fields { Arc::<str>::from(value.as_str()) } }
        Self::BytesView(value) => Bytes { fields { Arc::<[u8]>::from(value.as_slice()) } }
        Self::SharedList(value) => List { fields { value.as_ref() } }
    }
    custom {
        Set = 19 {
            encode {
                Self::Set(value) => {
                    payload.push(
                        u32::try_from(value.len())
                            .map_err(|_| IrBuildError::format("set_overflow", None, 0, 0))?,
                    );
                    for element in value.iter() {
                        element.encode(builder, &mut payload)?;
                    }
                }
            }
            decode { Self::Set(Arc::new(decode_set_payload(decoder, &mut payload)?)) }
            verify { decode_set_payload(decoder, &mut payload)?; }
        }
    }
    reject {
        Self::OmittedArgument => {
            return Err(IrBuildError::format("omitted_argument_literal", None, 0, 0));
        },
        Self::Digest(_)
        | Self::Status(_)
        | Self::FsEntry(_)
        | Self::Command(_)
        | Self::ProcessHandle(_)
        | Self::NetJob(_)
        | Self::FsRoot(_)
        | Self::FsLock(_)
        | Self::Stream(_)
        | Self::Pure(_)
        | Self::Proc(_)
        | Self::Error(_)
        | Self::ResultErr(_) => {
            return Err(IrBuildError::format("non_literal_persistent_value", None, 0, 0));
        }
    }
}

/// The checks of a node instruction that depend on its tag and read its
/// payload from the start, before the fields are verified one by one.
fn verify_node_metadata(
    decoder: &FullDecoder<'_>,
    tag: FullTag,
    payload: FullCursor<'_>,
) -> Result<(), IrVerifyError> {
    if tag == FullTag::ExprModuleCall {
        let mut metadata = payload;
        let op = RuntimeOp::decode(decoder, &mut metadata)?;
        let plan =
            Option::<Arc<crate::runtime::eval::ModuleCallPlan>>::decode(decoder, &mut metadata)?;
        if let Some(plan) = plan {
            if !plan.matches_operation(op) {
                return Err(IrVerifyError::new("prepared module call plan operation policy does not match its instruction"));
            }
            // Inspect the shape without entering this block in the graph walk;
            // the generated field verifier owns its traversal and ownership check.
            let arguments = IrBlockId::from_raw(metadata.raw()?)
                .ok_or_else(|| IrVerifyError::new("module call argument block id is invalid"))?;
            let arguments = decoder.store.blocks.get(arguments.index())
                .ok_or_else(|| IrVerifyError::new("module call argument block is out of bounds"))?;
            let mut arguments = FullCursor::new(decoder.store.payload(arguments.instructions)?);
            let count = arguments.raw()? as usize;
            let mut supplied = Vec::with_capacity(count);
            for _ in 0..count {
                supplied.push(match arguments.raw()? {
                    0 => false,
                    1 => { arguments.raw()?; true }
                    _ => return Err(IrVerifyError::new("optional payload tag is invalid")),
                });
            }
            arguments.finish()?;
            plan.verify_arguments(op, &supplied).map_err(IrVerifyError::new)?;
        }
    }
    if tag == FullTag::ExprComparisonChain {
        decoder.verify_comparison_chain_shape(payload)?;
    }
    if tag == FullTag::ExprFsList {
        let op = RuntimeOp::decode(decoder, &mut payload.clone())?;
        if op != RuntimeOp::FsChildren {
            return Err(IrVerifyError::new(
                "filesystem list operation is not `fs.children`",
            ));
        }
    }
    if tag == FullTag::ExprTypedCall {
        decoder.verify_typed_call_shape(payload)?;
    }
    if tag == FullTag::ExprTag {
        let mut metadata = payload;
        let type_name = Name::decode(decoder, &mut metadata)?;
        let name = Arc::<str>::decode(decoder, &mut metadata)?;
        let fields_id = IrBlockId::from_raw(metadata.raw()?)
            .ok_or_else(|| IrVerifyError::new("tag fields block id is invalid"))?;
        let fields = decoder
            .store
            .blocks
            .get(fields_id.index())
            .ok_or_else(|| IrVerifyError::new("tag fields block is out of bounds"))?;
        let field_count = decoder
            .store
            .payload(fields.instructions)?
            .first()
            .copied()
            .ok_or_else(|| IrVerifyError::new("tag fields length is missing"))?;
        let wire = Option::<Arc<crate::sema::wire_enums::WireEnumMapping>>::decode(
            decoder,
            &mut metadata,
        )?;
        if wire.as_ref().is_some_and(|mapping| {
            mapping.type_name != type_name
                || field_count != 0
                || !mapping.variants.contains_key(&Name::intern(name.as_ref()))
        }) {
            return Err(IrVerifyError::new(
                "wire enum constructor identity or payload is invalid",
            ));
        }
    }
    Ok(())
}



macro_rules! impl_build_id_codec {
    ($id:ty, $rows:ident, $row:ty, $resource_transfer:literal, $checked_expr_type:literal) => {
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
                let checked_expr_type = if $checked_expr_type {
                    scratch.checked_expr_types.get(self.index()).cloned().flatten()
                } else {
                    None
                };
                let transfers_resource = if $resource_transfer {
                    *scratch.resource_transfers.get(self.index()).ok_or_else(|| {
                        IrBuildError::format("indexed_resource_transfer_fact", None, 0, 0)
                    })?
                } else {
                    false
                };
                let instruction_start = builder.store.tags.len();
                row.encode(builder, output)?;
                if let Some(ty) = checked_expr_type {
                    let instruction = builder
                        .store
                        .tags
                        .len()
                        .checked_sub(1)
                        .filter(|instruction| *instruction >= instruction_start)
                        .ok_or_else(|| {
                            IrBuildError::format(
                                "indexed_checked_expr_instruction",
                                None,
                                0,
                                builder.store.tags.len(),
                            )
                        })?;
                    if !builder.store.tags[instruction]
                        .kind()
                        .eq(&schema::InstructionKind::Expr)
                    {
                        return Err(IrBuildError::format(
                            "indexed_checked_expr_tag",
                            None,
                            0,
                            instruction,
                        ));
                    }
                    let type_id = builder
                        .semantic
                        .intern_type(&mut builder.store.semantic, &executable_type(&ty.ty))?;
                    builder.store.checked_expr_types.push(FullCheckedExprType {
                        instruction: u32::try_from(instruction).map_err(|_| {
                            IrBuildError::format(
                                "indexed_checked_expr_instruction",
                                None,
                                0,
                                instruction,
                            )
                        })?,
                        type_id,
                        function_ref: ty.function_ref,
                        with_error_source: ty.with_error_source,
                    });
                }
                if transfers_resource {
                    let instruction = builder
                        .store
                        .tags
                        .len()
                        .checked_sub(1)
                        .filter(|instruction| *instruction >= instruction_start)
                        .ok_or_else(|| {
                            IrBuildError::format(
                                "indexed_resource_transfer_instruction",
                                None,
                                0,
                                builder.store.tags.len(),
                            )
                        })?;
                    if !is_resource_transfer_instruction(builder.store.tags[instruction]) {
                        return Err(IrBuildError::format(
                            "indexed_resource_transfer_tag",
                            None,
                            0,
                            instruction,
                        ));
                    }
                    builder.store.resource_transfers[instruction] = true;
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

impl_build_id_codec!(BuildExprId, expressions, BuildExprRow, false, true);
impl_build_id_codec!(BuildStmtId, statements, BuildStmtRow, true, false);
impl_build_id_codec!(BuildPatternId, patterns, BuildPatternRow, false, false);
impl_build_id_codec!(BuildIntId, ints, BuildIntRow, false, false);
impl_build_id_codec!(BuildBoolId, bools, BuildBoolRow, false, false);





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
    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        let updates = Vec::<(Vec<Name>, BuildExprId, Span)>::decode(decoder, input)?;
        for (index, (path, _, _)) in updates.iter().enumerate() {
            if path.is_empty()
                || updates[..index]
                    .iter()
                    .any(|(other, _, _)| path.starts_with(other) || other.starts_with(path))
            {
                return Err(IrVerifyError::new(
                    "record update paths must be nonempty and disjoint",
                ));
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



impl FullCodec for LoweredAssignPath {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.0.encode(builder, output)
    }
    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        let steps = Vec::<LoweredAssignStep>::decode(decoder, input)?;
        if steps.is_empty() {
            return Err(IrVerifyError::new("assignment path must select an element"));
        }
        Ok(Self(steps))
    }
}

impl_vec_codec!(LoweredAssignStep, BLOCK_LIST);

impl FullCodec for LoweredCompQualifiers {
    fn encode(&self, builder: &mut FullBuilder, output: &mut Vec<u32>) -> Result<(), IrBuildError> {
        self.0.encode(builder, output)
    }

    fn decode(
        decoder: &FullDecoder<'_>,
        input: &mut FullCursor<'_>,
    ) -> Result<Self, IrVerifyError> {
        let qualifiers = Vec::<LoweredCompQualifier>::decode(decoder, input)?;
        if !matches!(qualifiers.first(), Some(LoweredCompQualifier::For { .. })) {
            return Err(IrVerifyError::new(
                "comprehension qualifiers must start with for",
            ));
        }
        Ok(Self(qualifiers))
    }
}


impl_vec_codec!(LoweredCompQualifier, BLOCK_LIST);

















#[cfg(test)]
struct StageViewExpectations<'a> {
    store: &'a FullStore,
    expressions: &'a [BuildExprRow],
    statements: &'a [BuildStmtRow],
}

#[cfg(test)]
fn stage_view_decoder(store: &FullStore) -> FullDecoder<'_> {
    FullDecoder {
        store,
        owner: 0,
        instruction_range: 0..store.tags.len(),
        instruction_states: None,
        instruction_proof: None,
        block_states: None,
        slot_count: 1,
        pattern_ceiling: Cell::new(usize::MAX),
        local_immutable_let_initializers: RefCell::default(),
        function_signature_index: None,
        verified: false,
    }
}

#[cfg(test)]
fn assert_expression_instruction_matches(
    actual: decode::ExprInstructionId<'_>,
    expected: BuildExprId,
    expectations: &StageViewExpectations<'_>,
) {
    let row = expectations.expressions.get(expected.index())
        .expect("fixture expression ID refers to its owned row");
    let index = actual.raw() as usize;
    let tag = expectations.store.tags[index];
    let data = expectations.store.data[index];
    let mut payload = FullCursor::new(expectations.store.payload(data.range()).unwrap());
    let decoder = stage_view_decoder(expectations.store);
    match row {
        BuildExprRow::Int(value) => {
            assert_eq!(tag, FullTag::ExprInt);
            assert_eq!(i64::decode(&decoder, &mut payload).unwrap(), *value);
        }
        BuildExprRow::Bool(value) => {
            assert_eq!(tag, FullTag::ExprBool);
            assert_eq!(bool::decode(&decoder, &mut payload).unwrap(), *value);
        }
        _ => panic!("stage view fixture uses scalar expression rows"),
    }
    payload.finish().unwrap();
}

#[cfg(test)]
fn assert_statement_instruction_matches(
    actual: decode::StmtInstructionId<'_>,
    expected: BuildStmtId,
    expectations: &StageViewExpectations<'_>,
) {
    let row = expectations.statements.get(expected.index())
        .expect("fixture statement ID refers to its owned row");
    let index = actual.raw() as usize;
    let tag = expectations.store.tags[index];
    let data = expectations.store.data[index];
    let mut payload = FullCursor::new(expectations.store.payload(data.range()).unwrap());
    let execution = FullExecution::new(
        ProgramOwner::TestStore(TestStoreOwner(expectations.store)),
        0,
        0..expectations.store.tags.len(),
        1,
        false,
    );
    let context = DecodeContext::new(&execution);
    match row {
        BuildStmtRow::Value { value } => {
            assert_eq!(tag, FullTag::StmtValue);
            let actual = BuildExprId::borrow(context, &mut payload).unwrap();
            assert_expression_instruction_matches(actual, *value, expectations);
        }
        BuildStmtRow::Expr { value, span } => {
            assert_eq!(tag, FullTag::StmtExpr);
            let actual = BuildExprId::borrow(context, &mut payload).unwrap();
            assert_expression_instruction_matches(actual, *value, expectations);
            assert_eq!(Span::borrow(context, &mut payload).unwrap(), *span);
        }
        _ => panic!("stage view fixture uses value and expression statement rows"),
    }
    payload.finish().unwrap();
}

#[cfg(test)]
trait StageFieldMatch<T> {
    fn assert_matches(self, expected: &T, expectations: &StageViewExpectations<'_>);
}

#[cfg(test)]
macro_rules! stage_scalar_match {
    ($($ty:ty),* $(,)?) => {$(
        impl StageFieldMatch<$ty> for $ty {
            fn assert_matches(self, expected: &$ty, _expectations: &StageViewExpectations<'_>) {
                assert_eq!(self, *expected);
            }
        }
    )* };
}

#[cfg(test)]
stage_scalar_match!(usize, bool, ReduceByOp);

#[cfg(test)]
impl<'a> StageFieldMatch<BuildExprId> for decode::ExprInstructionId<'a> {
    fn assert_matches(self, expected: &BuildExprId, expectations: &StageViewExpectations<'_>) {
        assert_expression_instruction_matches(self, *expected, expectations);
    }
}

#[cfg(test)]
impl<'a> StageFieldMatch<BuildStmtId> for decode::StmtInstructionId<'a> {
    fn assert_matches(self, expected: &BuildStmtId, expectations: &StageViewExpectations<'_>) {
        assert_statement_instruction_matches(self, *expected, expectations);
    }
}

#[cfg(test)]
impl<'a> StageFieldMatch<String> for &'a str {
    fn assert_matches(self, expected: &String, _expectations: &StageViewExpectations<'_>) {
        assert_eq!(self, expected.as_str());
    }
}

#[cfg(test)]
impl<T, V> StageFieldMatch<Option<T>> for Option<V>
where
    V: StageFieldMatch<T>,
{
    fn assert_matches(self, expected: &Option<T>, expectations: &StageViewExpectations<'_>) {
        match (self, expected) {
            (Some(view), Some(expected)) => view.assert_matches(expected, expectations),
            (None, None) => {}
            _ => panic!("borrowed pipeline stage option differs"),
        }
    }
}

#[cfg(test)]
impl<'a, T> StageFieldMatch<Vec<T>> for decode::ListView<'a, T>
where
    T: BorrowCodec,
    T::View<'a>: StageFieldMatch<T>,
{
    fn assert_matches(
        mut self,
        expected: &Vec<T>,
        expectations: &StageViewExpectations<'_>,
    ) {
        assert_eq!(self.len(), expected.len());
        for expected in expected {
            self.next()
                .expect("stage list has an encoded element")
                .unwrap()
                .assert_matches(expected, expectations);
        }
        assert!(self.next().is_none());
    }
}

#[cfg(test)]
trait StageFieldsMatch<T> {
    fn assert_matches(self, expected: &T, expectations: &StageViewExpectations<'_>);
}

#[cfg(test)]
impl StageFieldsMatch<()> for () {
    fn assert_matches(self, _expected: &(), _expectations: &StageViewExpectations<'_>) {}
}

#[cfg(test)]
macro_rules! stage_fields_match {
    ($($view_ty:ident:$expected_ty:ident:$view:ident:$index:tt),+ $(,)?) => {
        impl<$($view_ty, $expected_ty),+> StageFieldsMatch<($($expected_ty,)+)>
            for ($($view_ty,)+)
        where
            $($view_ty: StageFieldMatch<$expected_ty>),+
        {
            fn assert_matches(
                self,
                expected: &($($expected_ty,)+),
                expectations: &StageViewExpectations<'_>,
            ) {
                let ($($view,)+) = self;
                $($view.assert_matches(&expected.$index, expectations);)+
            }
        }
    };
}

#[cfg(test)]
stage_fields_match!(V0:E0:view0:0);
#[cfg(test)]
stage_fields_match!(V0:E0:view0:0, V1:E1:view1:1);
#[cfg(test)]
stage_fields_match!(V0:E0:view0:0, V1:E1:view1:1, V2:E2:view2:2);
#[cfg(test)]
stage_fields_match!(V0:E0:view0:0, V1:E1:view1:1, V2:E2:view2:2, V3:E3:view3:3);
#[cfg(test)]
stage_fields_match!(V0:E0:view0:0, V1:E1:view1:1, V2:E2:view2:2, V3:E3:view3:3, V4:E4:view4:4);
#[cfg(test)]
stage_fields_match!(V0:E0:view0:0, V1:E1:view1:1, V2:E2:view2:2, V3:E3:view3:3, V4:E4:view4:4, V5:E5:view5:5);
#[cfg(test)]
stage_fields_match!(V0:E0:view0:0, V1:E1:view1:1, V2:E2:view2:2, V3:E3:view3:3, V4:E4:view4:4, V5:E5:view5:5, V6:E6:view6:6);
#[cfg(test)]
stage_fields_match!(V0:E0:view0:0, V1:E1:view1:1, V2:E2:view2:2, V3:E3:view3:3, V4:E4:view4:4, V5:E5:view5:5, V6:E6:view6:6, V7:E7:view7:7);
#[cfg(test)]
stage_fields_match!(V0:E0:view0:0, V1:E1:view1:1, V2:E2:view2:2, V3:E3:view3:3, V4:E4:view4:4, V5:E5:view5:5, V6:E6:view6:6, V7:E7:view7:7, V8:E8:view8:8);

#[cfg(test)]
fn assert_stage_fields_match<V, T>(
    view: V,
    expected: &T,
    expectations: &StageViewExpectations<'_>,
)
where
    V: StageFieldsMatch<T>,
{
    view.assert_matches(expected, expectations);
}

macro_rules! pipeline_stage_schema {
    (
        @build
        [$stage_builder:ident]
        [$stage_decoder:ident]
        [$stage_payload:ident]
        [$stage_view_context:ident] [$stage_view_input:ident]
        [$stage_view_value:ident] [$stage_view_expectations:ident]
        [$($variants:tt)*]
        [$($tags:tt)*]
        [$($encode_arms:tt)*]
        [$($decode_arms:tt)*]
        [$($verify_arms:tt)*]
        [$($view_variants:tt)*]
        [$($borrow_arms:tt)*]
        [$($view_tag_arms:tt)*]
        [$($view_match_arms:tt)*]
        ;
    ) => {
        #[derive(Clone, Debug)]
        pub(in crate::runtime::eval) enum LoweredPipelineStage {
            $($variants)*
        }

        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[repr(u8)]
        pub(in crate::runtime::eval) enum FullStageTag {
            $($tags)*
        }

        pub(in crate::runtime::eval) enum StageView<'a> {
            $($view_variants)*
        }

        impl<'a> StageView<'a> {
            pub(in crate::runtime::eval) fn decode(
                $stage_view_context: DecodeContext<'a>,
                raw: u32,
            ) -> Result<Self, IrVerifyError> {
                let (tag, mut $stage_payload) =
                    $stage_view_context.stage_payload(raw)?;
                let view = match tag {
                    $($borrow_arms)*
                };
                $stage_payload.finish()?;
                Ok(view)
            }

            pub(in crate::runtime::eval) fn tag(&self) -> FullStageTag {
                match self {
                    $($view_tag_arms)*
                }
            }
        }

        impl LoweredPipelineStage {
            pub(in crate::runtime::eval) fn borrow_with_id<'a>(
                context: DecodeContext<'a>,
                input: &mut FullCursor<'a>,
            ) -> Result<StageRef<'a>, IrVerifyError> {
                let raw = input.raw()?;
                let view = StageView::decode(context, raw)?;
                Ok(StageRef::new(raw, view))
            }
        }

        impl FullCodec for LoweredPipelineStage {
            fn encode(
                &self,
                $stage_builder: &mut FullBuilder,
                output: &mut Vec<u32>,
            ) -> Result<(), IrBuildError> {
                let (tag, $stage_payload) = match self {
                    $($encode_arms)*
                };
                let stage = $stage_builder.push_stage(tag, &$stage_payload);
                $stage_builder.recycle_payload($stage_payload);
                output.push(stage?);
                Ok(())
            }

            fn decode(
                $stage_decoder: &FullDecoder<'_>,
                input: &mut FullCursor<'_>,
            ) -> Result<Self, IrVerifyError> {
                let index = input.raw()? as usize;
                let tag = $stage_decoder
                    .store
                    .stages
                    .get(index)
                    .copied()
                    .ok_or_else(|| IrVerifyError::new("pipeline stage id is out of bounds"))?;
                let data = $stage_decoder.store.stage_data[index];
                let mut $stage_payload =
                    FullCursor::new($stage_decoder.store.payload(data.range())?);
                let stage = match tag {
                    $($decode_arms)*
                };
                $stage_payload.finish()?;
                Ok(stage)
            }

            fn verify(
                $stage_decoder: &FullDecoder<'_>,
                input: &mut FullCursor<'_>,
            ) -> Result<(), IrVerifyError> {
                let index = input.raw()? as usize;
                let tag = $stage_decoder
                    .store
                    .stages
                    .get(index)
                    .copied()
                    .ok_or_else(|| IrVerifyError::new("pipeline stage id is out of bounds"))?;
                let data = $stage_decoder.store.stage_data[index];
                let mut $stage_payload =
                    FullCursor::new($stage_decoder.store.payload(data.range())?);
                match tag {
                    $($verify_arms)*
                }
                $stage_payload.finish()
            }
        }

        // A stage view must consume its complete tagged row before it escapes.
        impl BorrowCodec for LoweredPipelineStage {
            type View<'a> = StageView<'a>;

            fn borrow<'a>(
                $stage_view_context: DecodeContext<'a>,
                $stage_view_input: &mut FullCursor<'a>,
            ) -> Result<Self::View<'a>, IrVerifyError> {
                Ok(Self::borrow_with_id($stage_view_context, $stage_view_input)?.view)
            }
        }

        #[cfg(test)]
        fn assert_stage_view_matches(
            owned: LoweredPipelineStage,
            $stage_view_value: StageView<'_>,
            $stage_view_expectations: &StageViewExpectations<'_>,
        ) {
            match owned {
                $($view_match_arms)*
            }
        }
    };
    (
        @build
        [$stage_builder:ident]
        [$stage_decoder:ident]
        [$stage_payload:ident]
        [$stage_view_context:ident] [$stage_view_input:ident]
        [$stage_view_value:ident] [$stage_view_expectations:ident]
        [$($variants:tt)*]
        [$($tags:tt)*]
        [$($encode_arms:tt)*]
        [$($decode_arms:tt)*]
        [$($verify_arms:tt)*]
        [$($view_variants:tt)*]
        [$($borrow_arms:tt)*]
        [$($view_tag_arms:tt)*]
        [$($view_match_arms:tt)*]
        ;
        $variant:ident unit {};
        $($rest:tt)*
    ) => {
        pipeline_stage_schema!(
            @build
            [$stage_builder]
            [$stage_decoder]
            [$stage_payload]
            [$stage_view_context] [$stage_view_input]
            [$stage_view_value] [$stage_view_expectations]
            [$($variants)* $variant,]
            [$($tags)* $variant,]
            [$($encode_arms)*
                LoweredPipelineStage::$variant => {
                    #[allow(unused_mut)]
                    let mut $stage_payload = $stage_builder.take_payload();
                    (FullStageTag::$variant, $stage_payload)
                },
            ]
            [$($decode_arms)*
                FullStageTag::$variant => LoweredPipelineStage::$variant,
            ]
            [$($verify_arms)* FullStageTag::$variant => {},]
            [$($view_variants)* $variant,]
            [$($borrow_arms)* FullStageTag::$variant => StageView::$variant,]
            [$($view_tag_arms)* StageView::$variant => FullStageTag::$variant,]
            [$($view_match_arms)*
                LoweredPipelineStage::$variant => match $stage_view_value {
                    StageView::$variant => {},
                    _ => panic!("borrowed pipeline stage tag differs"),
                },
            ]
            ;
            $($rest)*
        );
    };
    (
        @build
        [$stage_builder:ident]
        [$stage_decoder:ident]
        [$stage_payload:ident]
        [$stage_view_context:ident] [$stage_view_input:ident]
        [$stage_view_value:ident] [$stage_view_expectations:ident]
        [$($variants:tt)*]
        [$($tags:tt)*]
        [$($encode_arms:tt)*]
        [$($decode_arms:tt)*]
        [$($verify_arms:tt)*]
        [$($view_variants:tt)*]
        [$($borrow_arms:tt)*]
        [$($view_tag_arms:tt)*]
        [$($view_match_arms:tt)*]
        ;
        $variant:ident tuple { $($field:ident : $field_ty:ty),+ $(,)? };
        $($rest:tt)*
    ) => {
        pipeline_stage_schema!(
            @build
            [$stage_builder]
            [$stage_decoder]
            [$stage_payload]
            [$stage_view_context] [$stage_view_input]
            [$stage_view_value] [$stage_view_expectations]
            [$($variants)* $variant($($field_ty),*),]
            [$($tags)* $variant,]
            [$($encode_arms)*
                LoweredPipelineStage::$variant($($field),*) => {
                    #[allow(unused_mut)]
                    let mut $stage_payload = $stage_builder.take_payload();
                    $($field.encode($stage_builder, &mut $stage_payload)?;)*
                    (FullStageTag::$variant, $stage_payload)
                },
            ]
            [$($decode_arms)*
                FullStageTag::$variant => {
                    $(let $field = <$field_ty>::decode($stage_decoder, &mut $stage_payload)?;)*
                    LoweredPipelineStage::$variant($($field),*)
                },
            ]
            [$($verify_arms)*
                FullStageTag::$variant => {
                    $(<$field_ty>::verify($stage_decoder, &mut $stage_payload)?;)*
                },
            ]
            [$($view_variants)*
                $variant($(<$field_ty as BorrowCodec>::View<'a>),*),
            ]
            [$($borrow_arms)*
                FullStageTag::$variant => {
                    $(let $field = <$field_ty>::borrow(
                        $stage_view_context,
                        &mut $stage_payload,
                    )?;)*
                    StageView::$variant($($field),*)
                },
            ]
            [$($view_tag_arms)*
                StageView::$variant(..) => FullStageTag::$variant,
            ]
            [$($view_match_arms)*
                LoweredPipelineStage::$variant($($field),*) => {
                    let expected = ($($field,)*);
                    let actual = match $stage_view_value {
                        StageView::$variant($($field),*) => ($($field,)*),
                        _ => panic!("borrowed pipeline stage tag differs"),
                    };
                    assert_stage_fields_match(actual, &expected, $stage_view_expectations);
                },
            ]
            ;
            $($rest)*
        );
    };
    (
        @build
        [$stage_builder:ident]
        [$stage_decoder:ident]
        [$stage_payload:ident]
        [$stage_view_context:ident] [$stage_view_input:ident]
        [$stage_view_value:ident] [$stage_view_expectations:ident]
        [$($variants:tt)*]
        [$($tags:tt)*]
        [$($encode_arms:tt)*]
        [$($decode_arms:tt)*]
        [$($verify_arms:tt)*]
        [$($view_variants:tt)*]
        [$($borrow_arms:tt)*]
        [$($view_tag_arms:tt)*]
        [$($view_match_arms:tt)*]
        ;
        $variant:ident struct { $($field:ident : $field_ty:ty),+ $(,)? };
        $($rest:tt)*
    ) => {
        pipeline_stage_schema!(
            @build
            [$stage_builder]
            [$stage_decoder]
            [$stage_payload]
            [$stage_view_context] [$stage_view_input]
            [$stage_view_value] [$stage_view_expectations]
            [$($variants)* $variant { $($field: $field_ty),* },]
            [$($tags)* $variant,]
            [$($encode_arms)*
                LoweredPipelineStage::$variant { $($field),* } => {
                    #[allow(unused_mut)]
                    let mut $stage_payload = $stage_builder.take_payload();
                    $($field.encode($stage_builder, &mut $stage_payload)?;)*
                    (FullStageTag::$variant, $stage_payload)
                },
            ]
            [$($decode_arms)*
                FullStageTag::$variant => {
                    $(let $field = <$field_ty>::decode($stage_decoder, &mut $stage_payload)?;)*
                    LoweredPipelineStage::$variant { $($field),* }
                },
            ]
            [$($verify_arms)*
                FullStageTag::$variant => {
                    $(<$field_ty>::verify($stage_decoder, &mut $stage_payload)?;)*
                },
            ]
            [$($view_variants)*
                $variant { $($field: <$field_ty as BorrowCodec>::View<'a>),* },
            ]
            [$($borrow_arms)*
                FullStageTag::$variant => {
                    $(let $field = <$field_ty>::borrow(
                        $stage_view_context,
                        &mut $stage_payload,
                    )?;)*
                    StageView::$variant { $($field),* }
                },
            ]
            [$($view_tag_arms)*
                StageView::$variant { .. } => FullStageTag::$variant,
            ]
            [$($view_match_arms)*
                LoweredPipelineStage::$variant { $($field),* } => {
                    let expected = ($($field,)*);
                    let actual = match $stage_view_value {
                        StageView::$variant { $($field),* } => ($($field,)*),
                        _ => panic!("borrowed pipeline stage tag differs"),
                    };
                    assert_stage_fields_match(actual, &expected, $stage_view_expectations);
                },
            ]
            ;
            $($rest)*
        );
    };
    (
        @context
        $stage_builder:ident
        $stage_decoder:ident
        $stage_payload:ident
        $stage_view_context:ident $stage_view_input:ident
        $stage_view_value:ident $stage_view_expectations:ident;
        $($schema:tt)*
    ) => {
        pipeline_stage_schema!(
            @build
            [$stage_builder]
            [$stage_decoder]
            [$stage_payload]
            [$stage_view_context] [$stage_view_input]
            [$stage_view_value] [$stage_view_expectations]
            [] [] [] [] [] [] [] [] []
            ;
            $($schema)*
        );
    };
}

pipeline_stage_schema! {
    @context
    stage_builder
    stage_decoder
    stage_payload
    stage_view_context stage_view_input
    stage_view_value stage_view_expectations;
    TextLines unit {};
    JsonLines unit {};
    Where struct {
        slot: usize,
        predicate: BuildExprId,
    };
    WhereBlock struct {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    };
    Map struct {
        slot: usize,
        value: BuildExprId,
    };
    MapBlock struct {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    };
    FlatMap struct {
        slot: usize,
        value: BuildExprId,
    };
    FlatMapBlock struct {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    };
    BytesChunks struct {
        size: BuildExprId,
    };
    BatchCount struct {
        count: BuildExprId,
    };
    BatchMaxArgv struct {
        max_argv: Option<BuildExprId>,
    };
    BatchMaxBytes struct {
        max_bytes: BuildExprId,
    };
    BatchLimits struct {
        configuration: BuildExprId,
    };
    Shuffle struct {
        seed: Option<BuildExprId>,
    };
    Fold struct {
        acc_slot: usize,
        item_slot: usize,
        initial: BuildExprId,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    };
    ReduceBy struct {
        item_slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
        op: ReduceByOp,
        jobs: Option<BuildExprId>,
    };
    ReduceByConfigured struct {
        item_slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
        configuration: BuildExprId,
    };
    ParMap struct {
        slot: usize,
        jobs: Option<BuildExprId>,
        value: BuildExprId,
    };
    ParMapBlock struct {
        slot: usize,
        body: Vec<BuildStmtId>,
        jobs: Option<BuildExprId>,
        value: BuildExprId,
    };
    ParMapFlatMapReduceBy struct {
        slot: usize,
        body: Option<Vec<BuildStmtId>>,
        jobs: Option<BuildExprId>,
        value: BuildExprId,
        flatten: bool,
        reduce_item_slot: usize,
        reduce_body: Vec<BuildStmtId>,
        reduce_value: BuildExprId,
        op: ReduceByOp,
    };
    Tee struct {
        slot: usize,
        body: Vec<BuildStmtId>,
    };
    Each struct {
        slot: usize,
        body: Vec<BuildStmtId>,
    };
    TablePrint struct {
        columns: Option<Vec<String>>,
    };
    TablePrintConfigured struct {
        columns: BuildExprId,
    };
    Enumerate unit {};
    Zip struct {
        other: BuildExprId,
    };
    Sort struct {
        descending: Option<BuildExprId>,
    };
    SortBy struct {
        slot: usize,
        key: BuildExprId,
        descending: Option<BuildExprId>,
    };
    GroupBy struct {
        slot: usize,
        key: BuildExprId,
    };
    CountBy struct {
        slot: usize,
        key: BuildExprId,
    };
    Any struct {
        slot: usize,
        predicate: BuildExprId,
    };
    AnyBlock struct {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    };
    All struct {
        slot: usize,
        predicate: BuildExprId,
    };
    AllBlock struct {
        slot: usize,
        body: Vec<BuildStmtId>,
        value: BuildExprId,
    };
    UniqueBy struct {
        slot: usize,
        key: BuildExprId,
    };
    Count unit {};
    Sum unit {};
    Collect unit {};
    First unit {};
    Last unit {};
    Min unit {};
    Max unit {};
    Take tuple { value: BuildExprId };
    Drop tuple { value: BuildExprId };
    Repeat struct {
        count: BuildExprId,
    };
    Range struct {
        start: BuildExprId,
        end: BuildExprId,
    };
}





#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::eval::Evaluator;
    use crate::runtime::value::Value;
    use crate::sema::check::{CheckBundleView, CheckOptions, Checker};
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

    #[test]
    fn compact_lowering_uses_only_the_selected_entry_view() {
        let alpha_source = "print \"alpha\"\n";
        let sibling_source = "print ${reveal_type(1)}\n";
        let mut sources = SourceMap::new();
        let alpha_id = sources.add_file("alpha.xsh", alpha_source);
        let sibling_id = sources.add_file("sibling.xsh", sibling_source);
        let mut builder = crate::syntax::arena::ArenaProgramBuilder::with_token_capacity(32);
        let alpha = Parser::parse_source_into_arena_builder(alpha_id, alpha_source, &mut builder);
        let sibling =
            Parser::parse_source_into_arena_builder(sibling_id, sibling_source, &mut builder);
        assert!(alpha.diagnostics.is_empty(), "{:?}", alpha.diagnostics);
        assert!(sibling.diagnostics.is_empty(), "{:?}", sibling.diagnostics);
        let program = builder.finish_with_statements(alpha.statements);
        let sources = Arc::new(sources);
        let view = CheckBundleView::for_root(
            &program,
            alpha.statements,
            Some(&sources),
            Some(alpha_id),
        );
        let checked = Checker::check_bundle_view_with_type_program(
            &view,
            alpha_source,
            CheckOptions::for_execution(),
            Arc::new(program.clone()),
        );
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let declarations = Checker::compact_declarations(&program, checked);
        let diagnostics = Evaluator::compact_lowerability_diagnostics_with_view(
            &view,
            alpha_id,
            Arc::clone(&sources),
            declarations,
            Vec::new(),
            "alpha".to_string(),
        );

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(program.statements, alpha.statements);
        assert!(sibling.statements != alpha.statements);
    }

    #[test]
    fn resource_transfer_marks_require_aligned_eligible_instructions() {
        let mut store = FullStore::default();
        store.tags.push(FullTag::StmtReturn);
        store.resource_transfers.push(true);
        assert!(FullVerifier::verify_resource_transfer_sites(&store).is_ok());

        store.tags.push(FullTag::StmtContinue);
        assert!(FullVerifier::verify_resource_transfer_sites(&store).is_err());

        store.resource_transfers.push(false);
        assert!(FullVerifier::verify_resource_transfer_sites(&store).is_ok());

        store.resource_transfers[1] = true;
        assert!(FullVerifier::verify_resource_transfer_sites(&store).is_err());
    }

    #[test]
    fn compact_lowering_blocks_an_outer_assignment_without_its_checked_fact() {
        let source = "proc transfer() [fs, error] -> Result[Unit] { var escaped: FsRoot? = null; { escaped = fs.tempdir()? }; return Ok() }\n";
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("resource-transfer.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let mut declarations = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);

        let assignment = (0..parsed.arena.arena.stmt_tags.len())
            .map(crate::syntax::arena::StmtId::from_index)
            .find(|&stmt| {
                matches!(
                    parsed.arena.arena.stmt(stmt).kind,
                    crate::syntax::arena::ArenaStmtKind::Assign { .. }
                )
            })
            .expect("the fixture has one assignment");
        let site = crate::sema::check::ResourceTransferSite::Assignment(assignment);
        assert_eq!(declarations.bodies.resource_transfer(site), Some(true));
        declarations.bodies.resource_transfers.remove(&site);

        let constructed = super::super::super::lower::probe_compact_lower_constructed_bodies(
            &parsed.arena,
            &declarations,
            source,
        );
        assert!(constructed.blocker_events > 0, "{constructed:?}");
    }

    fn run_with_large_stack(f: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(f)
            .expect("spawn full indexed IR test")
            .join()
            .unwrap_or_else(|payload| std::panic::resume_unwind(payload));
    }

    fn fixture(name: &str, source: &str) -> FullProgram {
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
        let checked = Checker::check_arena_with_sources_and_options(
            &parsed.arena,
            &sources,
            source_id,
            crate::sema::check::CheckOptions {
                reveal_types: true,
                embedded_bodies: true,
                ..crate::sema::check::CheckOptions::default()
            },
        );
        let declarations = Checker::compact_declarations(&parsed.arena, checked);
        assert!(
            declarations.diagnostics.is_empty(),
            "{:?}",
            declarations.diagnostics
        );
        FullBuilder::build_compact(
            &parsed.arena,
            &declarations,
            source,
            Arc::new(sources),
            source_id,
        )
        .unwrap()
    }

    fn program_name(program: &FullProgram, text: &str) -> Name {
        program.symbol_owner().with_current(|| Name::intern(text))
    }

    fn schema_execution(store: &FullStore) -> FullExecution<'_> {
        schema_execution_with_slots(store, 1)
    }

    fn schema_execution_with_slots(store: &FullStore, slot_count: u32) -> FullExecution<'_> {
        schema_execution_with_owner(store, 0, slot_count)
    }

    fn schema_execution_with_owner(
        store: &FullStore,
        owner: u32,
        slot_count: u32,
    ) -> FullExecution<'_> {
        FullExecution::new(
            ProgramOwner::TestStore(TestStoreOwner(store)),
            owner,
            0..store.tags.len(),
            slot_count,
            false,
        )
    }

    fn stage_view_fixture() -> (
        FullBuilder,
        Vec<LoweredPipelineStage>,
        u32,
        Vec<BuildExprRow>,
        Vec<BuildStmtRow>,
    ) {
        let mut scratch = BuildScratch::default();
        let expression0 = scratch.expr(BuildExprRow::Int(17));
        let expression1 = scratch.expr(BuildExprRow::Bool(true));
        let statement0 = scratch.stmt(BuildStmtRow::Value { value: expression0 });
        let statement1 = scratch.stmt(BuildStmtRow::Expr {
            value: expression1,
            span: Span::at(SourceId::new(0), 0),
        });
        let expression_rows = scratch.expressions.clone();
        let statement_rows = scratch.statements.clone();
        let scratch = Rc::new(RefCell::new(scratch));
        let expression = BuildExprId::new;
        let body = || vec![statement0, statement1];
        let stages = vec![
            LoweredPipelineStage::TextLines,
            LoweredPipelineStage::JsonLines,
            LoweredPipelineStage::Where { slot: 2, predicate: expression(0) },
            LoweredPipelineStage::WhereBlock { slot: 3, body: body(), value: expression(1) },
            LoweredPipelineStage::Map { slot: 4, value: expression(0) },
            LoweredPipelineStage::MapBlock { slot: 5, body: body(), value: expression(1) },
            LoweredPipelineStage::FlatMap { slot: 6, value: expression(0) },
            LoweredPipelineStage::FlatMapBlock { slot: 7, body: body(), value: expression(1) },
            LoweredPipelineStage::BytesChunks { size: expression(0) },
            LoweredPipelineStage::BatchCount { count: expression(1) },
            LoweredPipelineStage::BatchMaxArgv { max_argv: Some(expression(0)) },
            LoweredPipelineStage::BatchMaxBytes { max_bytes: expression(1) },
            LoweredPipelineStage::BatchLimits { configuration: expression(0) },
            LoweredPipelineStage::Shuffle { seed: Some(expression(1)) },
            LoweredPipelineStage::Fold {
                acc_slot: 8,
                item_slot: 9,
                initial: expression(0),
                body: body(),
                value: expression(1),
            },
            LoweredPipelineStage::ReduceBy {
                item_slot: 10,
                body: body(),
                value: expression(1),
                op: ReduceByOp::Sum,
                jobs: Some(expression(0)),
            },
            LoweredPipelineStage::ReduceByConfigured {
                item_slot: 11,
                body: body(),
                value: expression(0),
                configuration: expression(1),
            },
            LoweredPipelineStage::ParMap {
                slot: 12,
                jobs: Some(expression(1)),
                value: expression(0),
            },
            LoweredPipelineStage::ParMapBlock {
                slot: 13,
                body: body(),
                jobs: None,
                value: expression(1),
            },
            LoweredPipelineStage::ParMapFlatMapReduceBy {
                slot: 14,
                body: Some(body()),
                jobs: Some(expression(0)),
                value: expression(1),
                flatten: true,
                reduce_item_slot: 15,
                reduce_body: body(),
                reduce_value: expression(0),
                op: ReduceByOp::Max,
            },
            LoweredPipelineStage::Tee { slot: 15, body: body() },
            LoweredPipelineStage::Each { slot: 16, body: body() },
            LoweredPipelineStage::TablePrint {
                columns: Some(vec!["name".to_string(), "size".to_string()]),
            },
            LoweredPipelineStage::TablePrintConfigured { columns: expression(1) },
            LoweredPipelineStage::Enumerate,
            LoweredPipelineStage::Zip { other: expression(0) },
            LoweredPipelineStage::Sort { descending: Some(expression(1)) },
            LoweredPipelineStage::SortBy { slot: 17, key: expression(0), descending: None },
            LoweredPipelineStage::GroupBy { slot: 18, key: expression(1) },
            LoweredPipelineStage::CountBy { slot: 19, key: expression(0) },
            LoweredPipelineStage::Any { slot: 20, predicate: expression(1) },
            LoweredPipelineStage::AnyBlock { slot: 21, body: body(), value: expression(0) },
            LoweredPipelineStage::All { slot: 22, predicate: expression(0) },
            LoweredPipelineStage::AllBlock { slot: 23, body: body(), value: expression(1) },
            LoweredPipelineStage::UniqueBy { slot: 24, key: expression(0) },
            LoweredPipelineStage::Count,
            LoweredPipelineStage::Sum,
            LoweredPipelineStage::Collect,
            LoweredPipelineStage::First,
            LoweredPipelineStage::Last,
            LoweredPipelineStage::Min,
            LoweredPipelineStage::Max,
            LoweredPipelineStage::Take(expression(0)),
            LoweredPipelineStage::Drop(expression(1)),
            LoweredPipelineStage::Repeat { count: expression(1) },
            LoweredPipelineStage::Range { start: expression(0), end: expression(1) },
        ];
        assert_eq!(stages.len(), 46);

        let mut builder = FullBuilder::new(SourceId::new(0));
        builder.current_owner = Some(0);
        builder.current_slot_count = 25;
        builder.active_scratch = Some(scratch);
        let mut encoded = Vec::new();
        stages.encode(&mut builder, &mut encoded).unwrap();
        assert_eq!(encoded.len(), 1);
        (builder, stages, encoded[0], expression_rows, statement_rows)
    }

    fn pattern_view_fixture() -> (FullBuilder, Vec<u32>) {
        let wildcard = BuildPatternId::new(8);
        let binding = BuildPatternId::new(9);
        let rows = [
            BuildPatternRow::Alias { pattern: wildcard, slot: 0 },
            BuildPatternRow::Alternation { patterns: vec![wildcard, wildcard] },
            BuildPatternRow::List { elements: vec![wildcard], rest: Some(binding) },
            BuildPatternRow::TagType { type_name: Name::INT, variants: vec![Name::STR] },
            BuildPatternRow::RecordTest { fields: vec![(Name::INT, wildcard)] },
            BuildPatternRow::ResultTest { ok: true, inner: wildcard },
            BuildPatternRow::TagTest { type_name: Name::INT, name: Name::STR, fields: vec![wildcard] },
            BuildPatternRow::ErrorTest { family: Name::ERROR, variant: Name::STR,
                fields: vec![(Name::INT, wildcard)] },
            BuildPatternRow::Wildcard,
            BuildPatternRow::Bind { slot: 0 },
            BuildPatternRow::Type { ty: Type::Int, slot: Some(0) },
            BuildPatternRow::Literal(LoweredValue::Null),
            BuildPatternRow::ResultOk { slot: Some(0), unit_only: false },
            BuildPatternRow::ResultErr { slot: None, unit_only: true },
            BuildPatternRow::Facet { facet: Name::ERROR, result_wrapped: false },
            BuildPatternRow::Tag { type_name: Name::INT, name: Name::STR,
                slots: SmallVec::from([Some(0), None]) },
            BuildPatternRow::Text { holes: vec![wildcard],
                kinds: vec![crate::sema::check::TextHoleKind::Decimal],
                segments: vec![Arc::from("id="), Arc::from("")] },
        ];
        let scratch = Rc::new(RefCell::new(BuildScratch::default()));
        for row in rows {
            scratch.borrow_mut().pattern(row);
        }
        let mut builder = FullBuilder::new(SourceId::new(0));
        builder.current_owner = Some(0);
        builder.current_slot_count = 1;
        builder.active_scratch = Some(scratch);
        let roots = (0..17)
            .map(|index| {
                let mut words = Vec::new();
                BuildPatternId::new(index).encode(&mut builder, &mut words).unwrap();
                words[0]
            })
            .collect();
        (builder, roots)
    }

    fn pattern_view_tag(view: schema::PatternView<'_>) -> FullPatternTag {
        match view {
            schema::PatternView::Alias { .. } => FullPatternTag::Alias,
            schema::PatternView::Alternation { .. } => FullPatternTag::Alternation,
            schema::PatternView::List { .. } => FullPatternTag::List,
            schema::PatternView::TagType { .. } => FullPatternTag::TagType,
            schema::PatternView::RecordTest { .. } => FullPatternTag::RecordTest,
            schema::PatternView::ResultTest { .. } => FullPatternTag::ResultTest,
            schema::PatternView::TagTest { .. } => FullPatternTag::TagTest,
            schema::PatternView::ErrorTest { .. } => FullPatternTag::ErrorTest,
            schema::PatternView::Wildcard => FullPatternTag::Wildcard,
            schema::PatternView::Bind { .. } => FullPatternTag::Bind,
            schema::PatternView::Type { .. } => FullPatternTag::Type,
            schema::PatternView::Literal(_) => FullPatternTag::Literal,
            schema::PatternView::ResultOk { .. } => FullPatternTag::ResultOk,
            schema::PatternView::ResultErr { .. } => FullPatternTag::ResultErr,
            schema::PatternView::Facet { .. } => FullPatternTag::Facet,
            schema::PatternView::Tag { .. } => FullPatternTag::Tag,
            schema::PatternView::Text { .. } => FullPatternTag::Text,
        }
    }

    fn function_instruction_indices(
        program: &FullProgram,
        function: &str,
        tag: FullTag,
    ) -> Vec<usize> {
        let function_index = program
            .store
            .functions
            .iter()
            .position(|row| program.store.string(row.name).is_ok_and(|name| name == function))
            .expect("fixture function exists");
        program
            .store
            .function_instruction_range(function_index)
            .expect("fixture function range exists")
            .filter(|instruction| program.store.tags[*instruction] == tag)
            .collect()
    }

    #[test]
    fn list_position_cannot_resume_a_different_statement_list() {
        let program = fixture(
            "statement-list-position.xsh",
            "proc main() [io, error] -> Result[Unit] {\n  print \"first\" \"second\"\n  print \"third\"\n  return Ok()\n}\n",
        );
        let statements = function_instruction_indices(&program, "main", FullTag::StmtPrint);
        assert_eq!(statements.len(), 2);

        program.symbol_owner().with_current(|| {
            let execution = program
                .function_view(
                    LoweredFunctionKey::Name(program_name(&program, "main")),
                    LoweredFunctionKind::Proc,
                )
                .expect("main resolves")
                .expect("main exists")
                .execution()
                .expect("main execution decodes");
            let first = execution
                .instruction_id(statements[0] as u32)
                .expect("first print instruction exists");
            let schema::StmtView::Print { args, .. } =
                schema::StmtView::decode(&execution, first.0, first.1).expect("first print decodes")
            else {
                panic!("print tag decodes to a print row");
            };
            let position = args.position();

            let second = execution
                .instruction_id(statements[1] as u32)
                .expect("second print instruction exists");
            let schema::StmtView::Print { mut args, .. } =
                schema::StmtView::decode(&execution, second.0, second.1)
                    .expect("second print decodes")
            else {
                panic!("print tag decodes to a print row");
            };
            assert!(args.resume(position).is_err());
        });
    }

    #[test]
    fn statement_block_position_rejects_a_foreign_store() {
        let program = fixture(
            "statement-block-position.xsh",
            "proc main() [io] {\n  print \"first\"\n  print \"second\"\n}\n",
        );
        program.symbol_owner().with_current(|| {
            let function = program
                .function_view(
                    LoweredFunctionKey::Name(program_name(&program, "main")),
                    LoweredFunctionKind::Proc,
                )
                .expect("main resolves")
                .expect("main exists");
            let execution = function.execution().expect("main execution decodes");
            let (body, _) = function.body(&execution).expect("main body decodes");
            let statements = execution
                .statement_block_view_id(body)
                .expect("main statements borrow");
            let position = statements.position();

            let foreign_store = program.store.clone();
            let foreign_execution =
                schema_execution_with_owner(&foreign_store, execution.owner, execution.slot_count);
            let mut foreign_statements = foreign_execution
                .statement_block_view_id(body)
                .expect("cloned store has the same block layout");
            assert!(foreign_statements.resume(position).is_err());
        });
    }

    #[test]
    fn owned_execution_keeps_list_position_store_identity() {
        let source = "proc main() { print \"first\" \"second\" }\n";
        let program = Arc::new(fixture("owned-list-position.xsh", source));
        let other_program = Arc::new(fixture("other-owned-list-position.xsh", source));
        let key = LoweredFunctionKey::Name(program_name(&program, "main"));
        let function_index = program
            .function_view(key, LoweredFunctionKind::Proc)
            .unwrap()
            .unwrap()
            .index();
        let statement = function_instruction_indices(&program, "main", FullTag::StmtPrint)[0];

        let execution = FullExecution::shared_function(Arc::clone(&program), function_index)
            .expect("shared execution resolves its function recipe");
        let (tag, payload) = execution
            .instruction_id(statement as u32)
            .expect("print instruction exists");
        let schema::StmtView::Print { mut args, .. } =
            schema::StmtView::decode(&execution, tag, payload).expect("print row decodes")
        else {
            panic!("print tag decodes to a print row");
        };
        args.next().unwrap().unwrap();
        let position = args.position();

        let same_program_execution =
            FullExecution::shared_function(Arc::clone(&program), function_index)
                .expect("another execution pins the same program allocation");
        let (tag, payload) = same_program_execution
            .instruction_id(statement as u32)
            .expect("print instruction exists in the same program");
        let schema::StmtView::Print { mut args, .. } =
            schema::StmtView::decode(&same_program_execution, tag, payload)
                .expect("same-program print row decodes")
        else {
            panic!("print tag decodes to a print row");
        };
        args.resume(position)
            .expect("a suspended list position resumes against the same store");
        assert!(args.next().unwrap().is_ok());

        let other_index = other_program
            .function_view(
                LoweredFunctionKey::Name(program_name(&other_program, "main")),
                LoweredFunctionKind::Proc,
            )
            .unwrap()
            .unwrap()
            .index();
        let other_execution =
            FullExecution::shared_function(Arc::clone(&other_program), other_index)
                .expect("other program resolves its function recipe");
        let (tag, payload) = other_execution
            .instruction_id(statement as u32)
            .expect("matching instruction index exists in other program");
        let schema::StmtView::Print { mut args, .. } =
            schema::StmtView::decode(&other_execution, tag, payload)
                .expect("other-program print row decodes")
        else {
            panic!("print tag decodes to a print row");
        };
        assert!(args.resume(position).is_err());
    }

    #[test]
    fn statement_env_validates_all_names_before_evaluating_values() {
        let source = r#"
proc mark() -> Str { print "evaluated"; return "value" }
proc main() [env, error] -> Result[Unit] {
  env XSH_FRAME_VALID=${mark()} XSH_FRAME_LATE=${mark()} { print "body" }
  return Ok()
}
"#;
        let mut program = fixture("statement-env-name-order.xsh", source);
        let statement = function_instruction_indices(&program, "main", FullTag::StmtEnv)[0];
        let payload = program.store.payload(program.store.data[statement].range()).unwrap();
        let env_block = IrBlockId::from_raw(payload[0]).unwrap();
        let range = program.store.blocks[env_block.index()].instructions;
        let second_name = program.symbol_owner().with_current(|| {
            let mut execution = schema_execution(&program.store);
            execution.owner = program.store.blocks[env_block.index()].owner;
            let mut entries = FullCursor::new(program.store.payload(range).unwrap());
            assert_eq!(entries.raw().unwrap(), 2);
            let decoder = execution.decoder();
            LoweredRunEnv::decode(&decoder, &mut entries).unwrap();
            entries.index
        });
        let invalid_name = program
            .symbol_owner()
            .with_current(|| Name::intern("BAD=NAME"));
        program.store.extra[range.start as usize + second_name] = invalid_name.symbol().raw();
        assert!(FullVerifier::verify(&program).is_ok());

        let program = Arc::new(program);
        program.symbol_owner().with_current(|| {
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            evaluator.force_frames = true;
            let original_env = evaluator.env.snapshot_clone();
            let error = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "main")),
                    LoweredFunctionKind::Proc,
                    &[],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("main exists")
                .expect_err("the corrupted later environment name is rejected");
            assert_eq!(error.kind, "env-name");
            assert!(evaluator.stdout.is_empty(), "no value or body expression ran");
            assert_eq!(evaluator.env.snapshot_clone(), original_env);
        });
    }

    fn append_string_addition_chain(program: &mut FullProgram, wrappers: usize) -> Vec<usize> {
        program.store.in_place.take();
        let function_index = program
            .store
            .functions
            .iter()
            .position(|row| program.store.string(row.name).is_ok_and(|name| name == "make_text"))
            .expect("string fixture function exists");
        let mut range = program
            .store
            .function_instruction_range(function_index)
            .expect("string fixture function range exists");
        assert_eq!(function_index + 1, program.store.functions.len());
        assert_eq!(range.end, program.store.tags.len());
        let old_instruction_end = u32::try_from(program.store.tags.len()).expect("fixture fits");
        assert!(program
            .store
            .driver_steps
            .iter()
            .all(|step| step.instruction_start == old_instruction_end));
        let template_index = range
            .clone()
            .find(|instruction| program.store.tags[*instruction] == FullTag::ExprBinary)
            .expect("string fixture contains an addition");
        let template_range = program.store.data[template_index]
            .range()
            .bounds(program.store.extra.len())
            .expect("binary payload is in bounds");
        let template = program.store.extra[template_range].to_vec();
        let execution = schema_execution(&program.store);
        let mut template_payload = FullCursor::new(&template);
        let _op = BinaryOp::decode(&execution.decoder(), &mut template_payload)
            .expect("binary operation decodes");
        let context = DecodeContext::new(&execution);
        let left_offset = template_payload.index;
        let _left = BuildExprId::borrow(context, &mut template_payload)
            .expect("binary left operand belongs to the fixture function");
        let right_offset = template_payload.index;
        let right_template = BuildExprId::borrow(context, &mut template_payload)
            .expect("binary right operand belongs to the fixture function")
            .raw() as usize;
        Span::decode(&execution.decoder(), &mut template_payload)
            .expect("binary source span decodes");
        template_payload
            .finish()
            .expect("binary payload has no trailing fields");
        assert_eq!(program.store.tags[right_template], FullTag::ExprStr);
        let literal_range = program.store.data[right_template]
            .range()
            .bounds(program.store.extra.len())
            .expect("string literal payload is in bounds");
        let literal = program.store.extra[literal_range].to_vec();
        let return_index = range
            .find(|instruction| program.store.tags[*instruction] == FullTag::StmtReturn)
            .expect("string fixture returns its addition");
        let return_payload = program.store.data[return_index]
            .range()
            .bounds(program.store.extra.len())
            .expect("return payload is in bounds");
        let mut child = template_index as u32;
        let mut appended = Vec::with_capacity(wrappers);
        for _ in 0..wrappers {
            let literal_index = program.store.tags.len();
            let literal_start = u32::try_from(program.store.extra.len()).expect("literal fits");
            let literal_len = u32::try_from(literal.len()).expect("literal payload fits");
            program.store.extra.extend_from_slice(&literal);
            program
                .store
                .data
                .push(IrData::new(literal_start, literal_len));
            program.store.tags.push(FullTag::ExprStr);
            program.store.resource_transfers.push(false);

            let instruction = program.store.tags.len();
            let mut payload = template.clone();
            payload[left_offset] = child;
            payload[right_offset] =
                u32::try_from(literal_index).expect("literal instruction ID fits");
            let start = u32::try_from(program.store.extra.len()).expect("fixture payload fits");
            let len = u32::try_from(payload.len()).expect("fixture payload fits");
            program.store.extra.extend(payload);
            program.store.data.push(IrData::new(start, len));
            program.store.tags.push(FullTag::ExprBinary);
            program.store.resource_transfers.push(false);
            appended.push(instruction);
            child = u32::try_from(instruction).expect("fixture instruction ID fits");
        }
        let new_instruction_end = u32::try_from(program.store.tags.len()).expect("fixture fits");
        for step in &mut program.store.driver_steps {
            step.instruction_start = new_instruction_end;
        }
        program.store.extra[return_payload.start] = child;
        appended
    }

    fn direct_if_fixture() -> FullProgram {
        let mut sources = SourceMap::new();
        let source_id = sources.add_file(
            "direct-if.xsh",
            "pure selected(flag: Bool) -> Int { return if flag { 1 } else { 2 } }\n",
        );
        let span = Span::new(source_id, 0, 0);
        let symbols = crate::symbol::SymbolOwner::new();
        symbols.with_current(|| {
            let scratch = Rc::new(RefCell::new(BuildScratch::default()));
            let return_statement = {
                let mut scratch = scratch.borrow_mut();
                let condition = scratch.expr(BuildExprRow::Param(0));
                let then_value = scratch.expr(BuildExprRow::Int(1));
                let else_value = scratch.expr(BuildExprRow::Int(2));
                let selection = scratch.expr(BuildExprRow::IfExpr {
                    branches: vec![(condition, then_value)],
                    else_value,
                    span,
                });
                scratch.stmt(BuildStmtRow::Return { value: selection })
            };
            let body = FunctionBuild {
                params: SmallVec::from_vec(vec![Name::intern("flag")]),
                param_kinds: SmallVec::from_vec(vec![LoweredType::Bool]),
                param_checks: SmallVec::from_vec(vec![None]),
                param_rest: SmallVec::from_vec(vec![false]),
                param_defaults: SmallVec::from_vec(vec![None]),
                captures: SmallVec::new(),
                return_kind: LoweredReturnKind::Plain(LoweredType::Int),
                return_check: None,
                checked_return_type: None,
                checked_function_signature: None,
                slot_count: 1,
                slot_type_facts: vec![None],
                body: vec![return_statement],
                scratch,
            };
            let unit = LoweredFunctionUnit {
                key: LoweredFunctionKey::Name(Name::intern("selected")),
                kind: LoweredFunctionKind::Pure,
                source_span: span,
                definition_span: span,
                owner: None,
                param_count: 1,
                capture_count: 0,
                slot_count: 1,
                dependency_edges: Vec::new(),
                body: Some(body),
                blocker: None,
                blocker_detail: None,
                scc_member_count: 1,
                scc_group: None,
            };
            let mut builder = FullBuilder::new(source_id);
            builder.predeclare(&[&unit]).unwrap();
            let body = unit.lowered_body().unwrap();
            let function = builder.function_ids[&unit.key()];
            builder.current_owner = Some(function.raw());
            builder.current_slot_count = body.slot_count as u32;
            builder.encode_body(function, &body).unwrap();
            builder.current_owner = None;
            builder.current_slot_count = 0;
            builder
                .finish(Arc::new(sources), symbols.clone())
                .expect("direct selection artifact verifies")
        })
    }

    #[test]
    fn direct_if_children_are_marked_and_selected() {
        let program = Arc::new(direct_if_fixture());
        let selections = function_instruction_indices(&program, "selected", FullTag::ExprIf);
        assert_eq!(selections.len(), 1);
        assert!(program.store.in_place.get().unwrap()[selections[0]]);

        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
        evaluator.indexed_program = Some(Arc::clone(&program));
        for (flag, expected) in [(true, 1), (false, 2)] {
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "selected")),
                    LoweredFunctionKind::Pure,
                    &[Value::Bool(flag)],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("selection function exists");
            assert_eq!(result.unwrap(), Value::Int(expected));
        }
    }

    #[test]
    fn frame_apply_child_keeps_selection_on_the_explicit_machine() {
        let source = r#"
pure selected() -> Path {
  return if true { Path("x") } else { Path("y") }
}
"#;
        let program = Arc::new(fixture("frame-apply-selection.xsh", source));
        FullVerifier::verify(&program).unwrap();
        let marks = program.store.in_place.get().expect("verification installs marks");
        let selection = function_instruction_indices(&program, "selected", FullTag::ExprIf);
        assert_eq!(selection.len(), 1);
        assert!(!marks[selection[0]]);
        let paths = function_instruction_indices(&program, "selected", FullTag::ExprPathFrom);
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().all(|path| !marks[*path]));

        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
        evaluator.indexed_program = Some(Arc::clone(&program));
        let result = evaluator
            .call_indexed_direct(
                LoweredFunctionKey::Name(program_name(&program, "selected")),
                LoweredFunctionKind::Pure,
                &[],
                Span::new(program.store.source_id, 0, 0),
            )
            .expect("selection function exists");
        assert_eq!(result.unwrap(), Value::Path(PathValue::from_text("x").unwrap()));
    }

    #[test]
    fn in_place_marks_follow_verified_operands_and_selection_runs_directly() {
        let source = r#"
pure leaf() -> Int { return 7 }
pure sum(left: Int, right: Int) -> Int { return left + right }
pure selected(flag: Bool) -> Int { return if flag { 1 } else { 2 } }
pure selected_call(flag: Bool) -> Int { return if flag { leaf() } else { 2 } }
pure aggregate() -> List[Int] { return [leaf(), 3] }
pure matched(value: Int) -> Int { return match value { 1 => 11, _ => 12 } }
"#;
        let program = Arc::new(fixture("in-place-marks.xsh", source));
        FullVerifier::verify(&program).unwrap();
        let marks = program.store.in_place.get().expect("verification installs marks");
        let selected = function_instruction_indices(&program, "selected", FullTag::ExprIf);
        assert_eq!(selected.len(), 1);
        assert!(!marks[selected[0]]);
        let selected_call =
            function_instruction_indices(&program, "selected_call", FullTag::ExprIf);
        assert_eq!(selected_call.len(), 1);
        assert!(!marks[selected_call[0]]);
        let aggregate = function_instruction_indices(&program, "aggregate", FullTag::ExprList);
        assert_eq!(aggregate.len(), 1);
        assert!(!marks[aggregate[0]]);
        assert!(function_instruction_indices(&program, "aggregate", FullTag::ExprInt)
            .into_iter()
            .any(|instruction| marks[instruction]));
        let binary = function_instruction_indices(&program, "sum", FullTag::ExprBinary);
        assert_eq!(binary.len(), 1);
        assert!(marks[binary[0]]);
        let matched = function_instruction_indices(&program, "matched", FullTag::ExprMatch);
        assert_eq!(matched.len(), 1);
        assert!(marks[matched[0]]);

        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
        evaluator.indexed_program = Some(Arc::clone(&program));
        for (name, arguments, expected) in [
            ("sum", vec![Value::Int(4), Value::Int(5)], Value::Int(9)),
            ("selected", vec![Value::Bool(true)], Value::Int(1)),
            ("selected", vec![Value::Bool(false)], Value::Int(2)),
            ("matched", vec![Value::Int(1)], Value::Int(11)),
            ("matched", vec![Value::Int(0)], Value::Int(12)),
        ] {
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, name)),
                    LoweredFunctionKind::Pure,
                    &arguments,
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("selection function exists");
            assert_eq!(result.unwrap(), expected);
        }
    }

    #[test]
    fn in_place_mark_cache_is_checked_against_independent_verification() {
        let mut program = fixture(
            "in-place-cache.xsh",
            "pure value() -> Int { return 1 }",
        );
        FullVerifier::verify(&program).unwrap();
        let marks = program.store.in_place.get_mut().unwrap();
        let mark = marks.iter_mut().find(|mark| **mark).expect("fixture has a mark");
        *mark = !*mark;
        let error = FullVerifier::verify(&program).unwrap_err();
        assert!(error.message.contains("in-place mark cache does not match"));
    }

    #[test]
    fn in_place_height_limit_marks_128_and_rejects_129_and_ancestors() {
        let mut program = fixture(
            "in-place-height.xsh",
            "pure make_text(text: Str) -> Str { return text + \"x\" }",
        );
        let appended = append_string_addition_chain(&mut program, 128);
        FullVerifier::verify(&program).unwrap();
        let marks = program.store.in_place.get().expect("verification installs marks");
        assert!(marks[appended[125]]);
        assert!(!marks[appended[126]]);
        assert!(!marks[appended[127]]);
    }

    #[test]
    fn instruction_schema_preserves_all_tag_ordinals_and_effects() {
        let expected = r#"
            IntInt
            IntSlot
            IntBinary
            IntStrByteLenSlot
            IntStrCountLinesSlot
            IntStrByteAtSlot
            BoolBool
            BoolSlot
            BoolNot
            BoolAnd
            BoolOr
            BoolIntCompare
            BoolStrPredicateSlot
            BoolContainsSlot
            BoolStrContainsSlot
            BoolTrimEmptySlot
            BoolTrimStrPredicateSlot
            BoolLiteralCompareSlot
            ExprNull
            ExprUnit
            ExprInt
            ExprFloat
            ExprDuration
            ExprBool
            ExprStr
            ExprBytes
            ExprPreparedRegex
            ExprPreparedConstant
            ExprPath
            ExprFunctionRef
            ExprPathFrom
            ExprParam
            ExprComparisonChain
            ExprBinary
            ExprIf
            ExprPatternIf
            ExprMatch
            ExprStrMatch
            ExprTagMatch
            ExprResultFallback
            ExprFmtString
            ExprPathFmtString
            ExprGlob
            ExprLastStatus
            ExprRecord
            ExprMapLiteral
            ExprRecordUpdate
            ExprList
            ExprListBuild
            ExprEmptyMap
            ExprRange
            ExprTag
            ExprListComp
            ExprMapComp
            ExprPipeline
            ExprField
            ExprIndex
            ExprIndexFromEnd
            ExprSlice
            ExprMethod
            ExprStrByteLen
            ExprStrByteAt
            ExprStrPredicate
            ExprRequire
            ExprCheckedValue
            ExprRunCapture
            ExprRunPipeline
            ExprSpawnRun
            ExprSpawnCommand
            ExprWait
            ExprCapture
            ExprValueBlock
            ExprErrorContext
            ExprContextScope
            ExprLoop
            ExprRetry
            ExprFsFiles
            ExprFsWalk
            ExprFsList
            ExprFsTempDir
            ExprFsMkdir
            ExprFsRemove
            ExprPathReadText
            ExprPathReadBytes
            ExprPathExists
            ExprPathExecutable
            ExprPathDu
            ExprPathMetadata
            ExprPathReadlink
            ExprPathResolve
            ExprPathWrite
            ExprPathMkdir
            ExprPathRemove
            ExprArchiveTarCreate
            ExprModuleCall
            ExprProcessCommandArgv
            ExprProcessCommandBuilder
            ExprAbort
            ExprFail
            ExprOk
            ExprErr
            ExprError
            ExprTry
            ExprCall
            ExprDirectPureCall
            ExprExternalCall
            ExprDynamicCall
            ExprSelfCall
            ExprTypedCall
            ExprProcCallResult
            StmtLet
            StmtGuard
            StmtWith
            StmtLetInt
            StmtLetBool
            StmtAssign
            StmtAssignField
            StmtAssignFieldInt
            StmtAssignPath
            StmtAssignInt
            StmtAssignBool
            StmtValue
            StmtAssert
            StmtExpr
            StmtIf
            StmtIfBool
            StmtWhile
            StmtWhileBool
            StmtPatternIf
            StmtPatternWhile
            StmtMatch
            StmtStrMatch
            StmtTagMatch
            StmtFor
            StmtLetRecord
            StmtForRecord
            StmtForStrLines
            StmtScanLines
            StmtScanBytes
            StmtPrint
            StmtCd
            StmtEnv
            StmtRun
            StmtLoop
            StmtReturn
            StmtYield
            StmtYieldDelegate
            StmtBreak
            StmtBreakValue
            StmtContinue
            StmtDefer
            StmtDefaultParameter
        "#;
        let names: Vec<_> = expected.split_whitespace().collect();
        let dead_ordinals = [50, 64, 95, 97, 98, 147];
        assert_eq!(schema::INSTRUCTION_TAGS.len(), 152);
        let expected_ordinals = (0usize..158).filter(|ordinal| !dead_ordinals.contains(ordinal));
        for ((tag, name), ordinal) in schema::INSTRUCTION_TAGS.iter().zip(names).zip(expected_ordinals) {
            assert_eq!(*tag as usize, ordinal);
            assert_eq!(tag.name(), name);
        }
        assert!(!schema::INSTRUCTION_TAGS.iter().any(|tag| dead_ordinals.contains(&(*tag as usize))));
        assert_eq!(*schema::INSTRUCTION_TAGS.last().unwrap() as usize, 157);
        fn previous_effects(tags: &[FullTag]) -> u32 {
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
                        FullTag::ExprAbort | FullTag::ExprFail => EFFECT_PROPAGATE | EFFECT_TRACE,
                        FullTag::ExprDynamicCall | FullTag::ExprTypedCall => {
                            EFFECT_DYNAMIC_CALL | EFFECT_TRACE
                        }
                        FullTag::ExprCall
                        | FullTag::ExprSelfCall
                        | FullTag::ExprDirectPureCall
                        | FullTag::ExprExternalCall => EFFECT_TRACE,
                        FullTag::ExprModuleCall => EFFECT_HOST | EFFECT_TRACE,
                        FullTag::StmtPrint => EFFECT_HOST | EFFECT_TRACE,
                        FullTag::ExprFsFiles
                        | FullTag::ExprFsWalk
                        | FullTag::ExprFsList
                        | FullTag::ExprFsTempDir
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
        for tag in schema::INSTRUCTION_TAGS {
            assert_eq!(tag.effects(), previous_effects(&[*tag]), "{}", tag.name());
            if tag.kind() == schema::InstructionKind::Stmt {
                assert_eq!(
                    tag.frame_strategy(),
                    FrameStrategy::FrameControl,
                    "statement {} must be scheduled on frames",
                    tag.name()
                );
            }
        }
        assert_eq!(
            FullTag::ExprExternalCall.frame_strategy(),
            FrameStrategy::FrameControl,
            "external calls are scheduled by the explicit frame machine"
        );
    }

    #[test]
    fn instruction_schema_preserves_scalar_encoding_and_borrows_storage() {
        let mut builder = FullBuilder::new(SourceId::new(0));
        builder.current_owner = Some(0);
        let mut words = Vec::new();
        for row in [
            BuildExprRow::Int(-1), BuildExprRow::Bool(true),
            BuildExprRow::Str(Arc::from("borrowed")),
            BuildExprRow::Bytes(Arc::from(b"bytes".as_slice())),
            BuildExprRow::Unit,
        ] {
            row.encode(&mut builder, &mut words).unwrap();
        }
        BuildIntRow::Int(i64::MIN).encode(&mut builder, &mut words).unwrap();
        BuildBoolRow::Bool(false).encode(&mut builder, &mut words).unwrap();
        assert_eq!(words, [0, 1, 2, 3, 4, 5, 6]);
        assert_eq!(builder.store.tags, [FullTag::ExprInt, FullTag::ExprBool, FullTag::ExprStr,
            FullTag::ExprBytes, FullTag::ExprUnit, FullTag::IntInt, FullTag::BoolBool]);
        let payloads: Vec<_> = builder.store.data.iter()
            .map(|data| builder.store.payload(data.range()).unwrap()).collect();
        assert_eq!(payloads, [vec![u32::MAX, u32::MAX], vec![1], vec![1],
            vec![1], vec![], vec![0, 0x8000_0000], vec![0]]);
        let execution = schema_execution(&builder.store);
        for (raw, expected) in [(0, -1)] {
            let (tag, payload) = execution.instruction_id(raw).unwrap();
            let schema::ExprView::Int(actual) = schema::ExprView::decode(&execution, tag, payload).unwrap()
                else { panic!("integer view") };
            assert_eq!(actual, expected);
        }
        let (tag, payload) = execution.instruction_id(2).unwrap();
        let schema::ExprView::Str(text) = schema::ExprView::decode(&execution, tag, payload).unwrap()
            else { panic!("string view") };
        assert_eq!(text, "borrowed");
        assert_eq!(text.as_ptr(), builder.store.string_bytes.as_ptr());
        let (tag, payload) = execution.instruction_id(3).unwrap();
        let schema::ExprView::Bytes(bytes) = schema::ExprView::decode(&execution, tag, payload).unwrap()
            else { panic!("bytes view") };
        assert_eq!(bytes, b"bytes");
        assert_eq!(bytes.as_ptr(), builder.store.byte_data.as_ptr());
        let (tag, payload) = execution.instruction_id(5).unwrap();
        let schema::IntView::Int(value) = schema::IntView::decode(&execution, tag, payload).unwrap()
            else { panic!("specialized integer view") };
        assert_eq!(value, i64::MIN);
        let (tag, payload) = execution.instruction_id(6).unwrap();
        assert!(matches!(schema::BoolView::decode(&execution, tag, payload).unwrap(), schema::BoolView::Bool(false)));
    }

    #[test]
    fn instruction_schema_list_views_keep_real_typed_child_ids() {
        let scratch = Rc::new(RefCell::new(BuildScratch::default()));
        let first = scratch.borrow_mut().expr(BuildExprRow::Int(7));
        let second = scratch.borrow_mut().expr(BuildExprRow::Str(Arc::from("seven")));
        let mut builder = FullBuilder::new(SourceId::new(0));
        builder.current_owner = Some(0);
        builder.active_scratch = Some(scratch);
        let mut words = Vec::new();
        BuildExprRow::List(vec![first, second]).encode(&mut builder, &mut words).unwrap();
        assert_eq!(words, [2]);
        assert_eq!(builder.store.tags, [FullTag::ExprInt, FullTag::ExprStr, FullTag::ExprList]);
        let execution = schema_execution(&builder.store);
        let (tag, payload) = execution.instruction_id(2).unwrap();
        let schema::ExprView::List(mut children) = schema::ExprView::decode(&execution, tag, payload).unwrap()
            else { panic!("list view") };
        assert_eq!(children.len(), 2);
        assert_eq!(children.next().unwrap().unwrap().raw(), 0);
        let second = children.next().unwrap().unwrap();
        assert_eq!(second.raw(), 1);
        assert!(children.next().is_none());
        let (tag, payload) = execution.instruction_id(second.raw()).unwrap();
        assert!(matches!(schema::ExprView::decode(&execution, tag, payload).unwrap(), schema::ExprView::Str("seven")));

        for malformed in [vec![2, 0], vec![2, 0, u32::MAX], vec![2, 0, 1, 99]] {
            let mut invalid = builder.store.clone();
            let start = invalid.extra.len() as u32;
            invalid.extra.extend_from_slice(&malformed);
            invalid.blocks[0].instructions = IrRange::new(start, malformed.len() as u32);
            let execution = schema_execution(&invalid);
            let (tag, payload) = execution.instruction_id(2).unwrap();
            let schema::ExprView::List(mut children) = schema::ExprView::decode(&execution, tag, payload).unwrap()
                else { panic!("list view") };
            assert_eq!(children.next().unwrap().unwrap().raw(), 0);
            assert!(children.next().unwrap().is_err());
        }
    }

    #[test]
    fn instruction_schema_views_reject_malformed_fields_and_wrong_child_categories() {
        let mut builder = FullBuilder::new(SourceId::new(0));
        builder.current_owner = Some(0);
        builder.current_slot_count = 1;
        let mut words = Vec::new();
        BuildIntRow::Int(2).encode(&mut builder, &mut words).unwrap();
        BuildBoolRow::Bool(true).encode(&mut builder, &mut words).unwrap();
        let execution = schema_execution(&builder.store);
        let decode_expr = |tag, words| schema::ExprView::decode(&execution, tag,
            FullPayload { store: execution.store(), cursor: FullCursor::new(words) });
        assert!(decode_expr(FullTag::ExprInt, &[1]).is_err());
        assert!(decode_expr(FullTag::ExprInt, &[1, 0, 9]).is_err());
        assert!(decode_expr(FullTag::ExprBool, &[2]).is_err());
        assert!(decode_expr(FullTag::ExprStr, &[u32::MAX]).is_err());
        assert!(decode_expr(FullTag::IntInt, &[1, 0]).is_err());
        let decoder = execution.decoder();
        assert!(schema::BoolView::decode(&execution, FullTag::BoolNot,
            FullPayload { store: execution.store(), cursor: FullCursor::new(&[0]) }).err().unwrap().message.contains("wrong category"));
        assert!(schema::BoolView::decode(&execution, FullTag::BoolNot,
            FullPayload { store: execution.store(), cursor: FullCursor::new(&[u32::MAX]) }).is_err());
        let mut other_owner = schema_execution(&builder.store);
        other_owner.instruction_range = 1..2;
        assert!(BuildIntId::borrow(DecodeContext::new(&other_owner),
            &mut FullCursor::new(&[0])).unwrap_err().message.contains("another function"));
        let context = DecodeContext::new(&execution);
        assert!(Option::<BuildExprId>::borrow(context, &mut FullCursor::new(&[2])).is_err());
        assert!(Option::<BuildExprId>::borrow(context, &mut FullCursor::new(&[1])).is_err());
        let decode::CallArgView::Default(slot) = LoweredCallArg::borrow(context,
            &mut FullCursor::new(&[2, 37])).unwrap() else { panic!("default argument") };
        assert_eq!(slot, 37);
        assert!(matches!(LoweredCallArg::decode(&decoder,
            &mut FullCursor::new(&[2, 37])).unwrap(), LoweredCallArg::Default(37)));
        assert_eq!(
            ScanCondition::borrow(context, &mut FullCursor::new(&[3]))
                .err()
                .unwrap()
                .message,
            "scan condition tag is invalid",
        );
        assert_eq!(
            ScanCondition::decode(&decoder, &mut FullCursor::new(&[3]))
                .err()
                .unwrap()
                .message,
            "scan condition tag is invalid",
        );
        assert_eq!(
            ScanCheck::borrow(context, &mut FullCursor::new(&[0, 1]))
                .err()
                .unwrap()
                .message,
            "slot is out of bounds",
        );
        assert_eq!(
            ScanCheck::decode(&decoder, &mut FullCursor::new(&[0, 1]))
                .err()
                .unwrap()
                .message,
            "slot is out of bounds",
        );
    }

    #[test]
    fn pattern_schema_views_cover_all_rows_and_keep_fields_borrowed() {
        let (builder, roots) = pattern_view_fixture();
        let execution = schema_execution(&builder.store);
        assert_eq!(schema::PATTERN_TAGS.len(), 17);
        let mut ids = FullCursor::new(&roots);
        let context = DecodeContext::new(&execution);
        let mut viewed_tags = Vec::with_capacity(roots.len());
        for _ in &roots {
            let pattern = BuildPatternId::borrow(context, &mut ids).unwrap();
            viewed_tags.push(pattern_view_tag(execution.pattern_view(pattern).unwrap()));
        }
        ids.finish().unwrap();
        assert_eq!(viewed_tags, schema::PATTERN_TAGS);

        let mut text_ids = FullCursor::new(&roots[16..]);
        let text_id = BuildPatternId::borrow(context, &mut text_ids).unwrap();
        let schema::PatternView::Text { mut holes, mut kinds, mut segments } =
            execution.pattern_view(text_id).unwrap() else { panic!("text pattern view") };
        assert_eq!(holes.len(), 1);
        let hole = holes.next().unwrap().unwrap();
        assert!(matches!(execution.pattern_view(hole).unwrap(), schema::PatternView::Wildcard));
        assert_eq!(kinds.next().unwrap().unwrap(), crate::sema::check::TextHoleKind::Decimal);
        let prefix = segments.next().unwrap().unwrap();
        assert_eq!(prefix, "id=");
        let pool_start = builder.store.string_bytes.as_ptr() as usize;
        let pool_end = pool_start + builder.store.string_bytes.len();
        assert!((pool_start..pool_end).contains(&(prefix.as_ptr() as usize)));
        assert_eq!(segments.next().unwrap().unwrap(), "");
        assert!(segments.next().is_none());

        let mut payload = FullPayload {
            store: execution.store(),
            cursor: FullCursor::new(&[2, 37, 99]),
        };
        let decode::CallArgView::Default(slot) = payload.borrow::<LoweredCallArg>(&execution).unwrap()
            else { panic!("borrowed call argument") };
        assert_eq!(slot, 37);
        assert_eq!(payload.raw().unwrap(), 99);
        payload.finish().unwrap();
    }

    #[test]
    fn pattern_views_reject_corruption_and_foreign_store_identities() {
        let (builder, roots) = pattern_view_fixture();
        let execution = schema_execution(&builder.store);
        let context = DecodeContext::new(&execution);
        let mut text_ids = FullCursor::new(&roots[16..]);
        let text_id = BuildPatternId::borrow(context, &mut text_ids).unwrap();

        let mut invalid = builder.store.clone();
        let text_range = invalid.pattern_data[roots[16] as usize].range();
        invalid.extra[text_range.start as usize + 1] =
            crate::sema::check::TextHoleKind::ALL.len() as u32;
        let invalid_execution = schema_execution(&invalid);
        let error = match invalid_execution.pattern_view(
            BuildPatternId::borrow(DecodeContext::new(&invalid_execution), &mut FullCursor::new(&roots[16..])).unwrap()
        ) {
            Err(error) => error,
            Ok(_) => panic!("invalid text hole kind was accepted"),
        };
        assert!(error.message.contains("text pattern hole kind is out of range"));

        let mut trailing = builder.store.clone();
        let text_index = roots[16] as usize;
        let text_data = trailing.pattern_data[text_index];
        let mut text_payload = trailing.payload(text_data.range()).unwrap().to_vec();
        text_payload.push(0);
        let start = u32::try_from(trailing.extra.len()).unwrap();
        trailing.extra.extend_from_slice(&text_payload);
        trailing.pattern_data[text_index] = IrData {
            lhs: start,
            rhs: u32::try_from(text_payload.len()).unwrap(),
        };
        let trailing_execution = schema_execution(&trailing);
        let trailing_id = BuildPatternId::borrow(DecodeContext::new(&trailing_execution),
            &mut FullCursor::new(&roots[16..])).unwrap();
        let error = match trailing_execution.pattern_view(trailing_id) {
            Err(error) => error,
            Ok(_) => panic!("trailing pattern row word was accepted"),
        };
        assert!(error.message.contains("full IR payload has trailing words"));

        let other_store = FullStore::default();
        let other_execution = schema_execution(&other_store);
        let error = match other_execution.pattern_view(text_id) {
            Err(error) => error,
            Ok(_) => panic!("foreign pattern identity was accepted"),
        };
        assert!(error.message.contains("another program"));

        let mut foreign_block_payload = FullPayload {
            store: execution.store(),
            cursor: FullCursor::new(&[0]),
        };
        let error = match other_execution.block(&mut foreign_block_payload, 0) {
            Err(error) => error,
            Ok(_) => panic!("foreign block payload was accepted"),
        };
        assert!(error.message.contains("another program"));
        assert_eq!(foreign_block_payload.raw().unwrap(), 0);

        let mut payload = FullPayload {
            store: execution.store(),
            cursor: FullCursor::new(&[2, 37]),
        };
        assert!(payload.borrow::<LoweredCallArg>(&other_execution).is_err());
    }

    #[test]
    fn module_command_plan_metadata_rejects_corrupt_interpretation() {
        run_with_large_stack(|| {
            use crate::modules::cli::ArgValueType;
            use crate::runtime::eval::call_plan::ModuleCallPlan;
            let program = fixture("command-plan-metadata.xsh", "json.write output ({value: 1}) false\njson.write output ({value: 1}) --pretty\ntime.sleep 0ms\nfs.fsync output\n");
            FullVerifier::verify(&program).unwrap();
            let (index, plan) = program.store.prepared_module_call_plans.iter().enumerate()
                .find_map(|(index, plan)| match plan.as_ref() {
                    ModuleCallPlan::CommandArguments(plan) if plan.word_conversions.len() == 2 => Some((index, plan)),
                    _ => None,
                }).expect("JSON command has Path and Bool word conversions");
            let rejects = |change: &dyn Fn(&mut crate::runtime::eval::call_plan::CommandArgumentsPlan)| {
                let mut invalid = program.clone();
                let mut plan = plan.clone();
                change(&mut plan);
                invalid.store.prepared_module_call_plans[index] = Arc::new(ModuleCallPlan::CommandArguments(plan));
                assert!(FullVerifier::verify(&invalid).is_err());
            };
            rejects(&|plan| plan.supplied_slots[0] = false);
            rejects(&|plan| plan.word_conversions.push(plan.word_conversions[0].clone()));
            rejects(&|plan| plan.word_conversions[0].slot = plan.supplied_slots.len());
            rejects(&|plan| plan.word_conversions[0].scalar = ArgValueType::Int);
            rejects(&|plan| plan.word_conversions.reverse());
            rejects(&|plan| plan.flag_slots.push(plan.word_conversions[0].slot));
            let other_signature = program.store.prepared_module_call_plans.iter().find_map(|entry| match entry.as_ref() {
                ModuleCallPlan::CommandArguments(other) if other.signature.op != plan.signature.op => Some(other.signature),
                _ => None,
            }).unwrap();
            rejects(&|plan| plan.signature = other_signature);
            rejects(&|plan| plan.signature = Box::leak(Box::new(plan.signature.clone())));

            let (flag_index, flag_plan) = program.store.prepared_module_call_plans.iter().enumerate()
                .find_map(|(index, plan)| match plan.as_ref() {
                    ModuleCallPlan::CommandArguments(plan) if !plan.flag_slots.is_empty() => Some((index, plan)),
                    _ => None,
                }).unwrap_or_else(|| panic!("Boolean flag command; published plans: {:?}",
                    program.store.prepared_module_call_plans.iter().filter_map(|plan| match plan.as_ref() {
                        ModuleCallPlan::CommandArguments(plan) => Some((plan.signature.op, &plan.flag_slots,
                            plan.word_conversions.iter().map(|word| word.slot).collect::<Vec<_>>())),
                        _ => None,
                    }).collect::<Vec<_>>()));
            for slot in [flag_plan.flag_slots[0], flag_plan.supplied_slots.len(), 0] {
                let mut invalid = program.clone();
                let mut flag_plan = flag_plan.clone();
                flag_plan.flag_slots.push(slot);
                invalid.store.prepared_module_call_plans[flag_index] = Arc::new(ModuleCallPlan::CommandArguments(flag_plan));
                assert!(FullVerifier::verify(&invalid).is_err());
            }

            let mut invalid = program.clone();
            let instruction = invalid.store.tags.iter().enumerate().find_map(|(instruction, tag)| {
                if *tag != FullTag::ExprModuleCall { return None; }
                let words = invalid.store.payload(invalid.store.data[instruction].range()).unwrap();
                (words[1] == 1 && words[2] as usize == index).then_some(instruction)
            }).unwrap();
            let words = invalid.store.payload(invalid.store.data[instruction].range()).unwrap();
            let arguments = IrBlockId::from_raw(words[3]).unwrap();
            let original = invalid.store.payload(invalid.store.blocks[arguments.index()].instructions).unwrap();
            let mut missing = vec![original[0], 0];
            missing.extend_from_slice(&original[3..]);
            let start = invalid.store.extra.len() as u32;
            invalid.store.extra.extend_from_slice(&missing);
            invalid.store.blocks[arguments.index()].instructions = IrRange::new(start, missing.len() as u32);
            let mut absent = plan.clone();
            absent.supplied_slots[0] = false;
            invalid.store.prepared_module_call_plans[index] = Arc::new(ModuleCallPlan::CommandArguments(absent));
            assert!(FullVerifier::verify(&invalid).unwrap_err().message.contains("checked signature"));
        });
    }

    #[test]
    fn builtin_templates_execute_checked_calls_after_frontend_drop_on_both_routes() {
        run_with_large_stack(|| {
            let source =
                include_str!("../../../../tests/fixtures/frontend-indexed/builtin-templates.xsh");
            let program = Arc::new(fixture("builtin-templates.xsh", source));
            FullVerifier::verify(&program).unwrap();
            for (name, expected) in [
                ("nested_templates", Value::Int(7)),
                ("fresh_templates", Value::Int(8)),
                ("absent_templates", Value::Bool(true)),
                ("materialized_templates", Value::Int(6)),
                ("discarded_templates", Value::Unit),
            ] {
                let mut evaluator =
                    Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let result = evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)),
                        LoweredFunctionKind::Pure,
                        &[],
                        Span::new(program.store.source_id, 0, 0),
                    )
                    .expect("template function exists");
                assert_eq!(result.unwrap(), expected);
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
            for (name, expected) in [("value", Value::Int(9)), ("results", Value::Bool(true))] {
                let mut evaluator =
                    Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let result = evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)),
                        LoweredFunctionKind::Pure,
                        &[],
                        Span::new(program.store.source_id, 0, 0),
                    )
                    .expect("stage callable function exists");
                assert_eq!(result.unwrap(), expected);
            }
        });
    }

    #[test]
    fn typed_cause_indexed_codec_preserves_metadata_and_verifies_optional_child() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/typed-causes.xsh");
        let program = fixture("typed-causes.xsh", source);
        FullVerifier::verify(&program).unwrap();
        let row = program
            .store
            .tags
            .iter()
            .position(|tag| *tag == FullTag::ExprErr)
            .unwrap();
        let range = program.store.data[row]
            .range()
            .bounds(program.store.extra.len())
            .unwrap();
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
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
        evaluator.indexed_program = Some(Arc::clone(&program));
        let result = evaluator
            .call_indexed_direct(
                LoweredFunctionKey::Name(program_name(&program, "translated_cause")),
                LoweredFunctionKind::Pure,
                &[],
                Span::at(program.store.source_id, 0),
            )
            .expect("typed constructor function exists")
            .unwrap();
        let Value::Result(crate::runtime::value::ResultValue::Err(error)) = result else {
            panic!("constructor returns Result data")
        };
        let Value::Error(outer) = error.as_ref() else {
            panic!("nominal outer error")
        };
        assert_eq!(outer.family, "OuterCauseError");
        let Value::Error(inner) = outer.cause.as_ref().unwrap().as_value() else {
            panic!("typed cause")
        };
        assert_eq!(inner.family, "InnerCauseError");
        assert_eq!(inner.span, None);
    }

    #[test]
    fn empty_map_fold_inference_publishes_concrete_accumulator_types() {
        let source = "let counts = [\"one\", \"two\", \"one\"] |> fold(map.empty()) { |acc, item| acc.set(item, (acc.get(item) ?? 0) + 1) }\nprint ${counts.get(\"one\") ?? 0}\n";
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("empty-map-fold.xsh", source);
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(
            checked.diagnostics.is_empty(),
            "full: {:?}",
            checked.diagnostics
        );
        let declarations = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(
            declarations.diagnostics.is_empty(),
            "decl: {:?}",
            declarations.diagnostics
        );
        assert!(
            declarations
                .bodies
                .expr_types
                .values()
                .all(|ty| !ty.contains_inference())
        );
        let _ = fixture("empty-map-fold.xsh", source);
    }

    #[test]
    fn local_collection_inference_publishes_concrete_indexed_call_and_slot_types() {
        let source =
            include_str!("../../../../tests/fixtures/frontend-indexed/local-inference.xsh");
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("local-inference.xsh", source);
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let declarations = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        parsed.arena.symbol_owner().with_current(|| {
            let function = parsed
                .arena
                .arena
                .function_defs
                .iter()
                .find(|function| function.name == "gather")
                .unwrap();
            let body_span = parsed
                .arena
                .arena
                .span(parsed.arena.arena.block(function.body).span);
            assert_eq!(
                declarations.function_return_types[&body_span],
                Type::List(Box::new(Type::Path))
            );
            assert!(
                declarations
                    .local_binding_types
                    .values()
                    .all(|ty| !ty.contains_inference())
            );
            assert!(
                declarations
                    .local_binding_types
                    .values()
                    .any(|ty| *ty == Type::List(Box::new(Type::Path)))
            );
        });
        assert!(
            declarations
                .bodies
                .expr_types
                .values()
                .all(|ty| !ty.contains_inference())
        );
        let constructed = super::super::super::lower::probe_compact_lower_constructed_bodies(
            &parsed.arena,
            &declarations,
            source,
        );
        assert_eq!(constructed.blocker_events, 0, "{constructed:?}");
        let _ = fixture("local-inference.xsh", source);
    }

    /// A resumed statement cursor runs the block body once per loop iteration.
    #[test]
    fn loop_statement_cursor_preserves_iteration_order() {
        run_with_large_stack(|| {
            let program = Arc::new(fixture(
                "indexed-list-reuse.xsh",
                "proc main() [io] {\n  var index = 0\n  while index < 200 {\n    index = index + 1\n  }\n  print f\"index={index}\"\n}\n",
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
    fn verifier_rejects_the_retired_function_defer_flag() {
        let mut program = fixture("indexed-execution.xsh", INDEXED_EXECUTION);
        assert!(program
            .store
            .function_metadata
            .iter()
            .any(|metadata| metadata.flags & 1 == 0));
        assert!(program
            .store
            .function_metadata
            .iter()
            .any(|metadata| metadata.flags & 1 != 0));
        FullVerifier::verify(&program).unwrap();

        program.store.function_metadata[0].flags |= 1 << 1;
        let error = FullVerifier::verify(&program).unwrap_err();
        assert!(
            error.message.contains("function metadata flags are invalid"),
            "{}",
            error.message
        );
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
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "selected")),
                    LoweredFunctionKind::Pure,
                    &[],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("enum payload function exists");
            assert_eq!(result.unwrap(), Value::Str("one:two:other".into()));
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
            let program = Arc::new(fixture(
                "prepared-constant.xsh",
                "const data: List[Int] = [1, 2]\nproc constant() -> List[Int] { data }\n",
            ));
            program.symbol_owner().with_current(|| {
                let header = program
                    .function_view(
                        LoweredFunctionKey::Name(Name::intern("constant")),
                        LoweredFunctionKind::Proc,
                    )
                    .unwrap()
                    .unwrap()
                    .header()
                    .unwrap();
                assert!(
                    header
                        .captures
                        .iter()
                        .all(|capture| capture.name != Name::intern("data"))
                );
            });
            let (result, _, _) = run_full(Arc::clone(&program), program_name(&program, "constant"));
            assert_eq!(
                result.unwrap(),
                Value::List(vec![Value::Int(1), Value::Int(2)])
            );
            let shared = program
                .store
                .prepared_constants
                .iter()
                .find_map(|constant| match &constant.0 {
                    LoweredValue::SharedList(values) => Some(Arc::clone(values)),
                    _ => None,
                })
                .expect("prepared list pool");
            let decoder = FullDecoder {
                store: &program.store,
                owner: 0,
                instruction_range: 0..0,
                instruction_states: None,
                instruction_proof: None,
                block_states: None,
                slot_count: 0,
                pattern_ceiling: Cell::new(usize::MAX),
                local_immutable_let_initializers: RefCell::default(),
                function_signature_index: None,
                verified: false,
            };
            let mut words = vec![0];
            let mut cursor = FullCursor::new(&words);
            let decoded = PreparedConstantValue::decode(&decoder, &mut cursor).unwrap();
            let LoweredValue::SharedList(values) = decoded.0 else {
                panic!("shared list");
            };
            assert!(Arc::ptr_eq(&shared, &values));
            let mut broken = (*program).clone();
            broken.store.prepared_constants.clear();
            assert!(FullVerifier::verify(&broken).is_err());
            let mut broken = (*program).clone();
            broken.store.prepared_constants[0] = PreparedConstantValue(LoweredValue::Unit);
            assert!(FullVerifier::verify(&broken).is_err());
            let mut builder = FullBuilder::new(SourceId::new(0));
            let checkpoint = builder.checkpoint();
            PreparedConstantValue(LoweredValue::Int(1))
                .encode(&mut builder, &mut words)
                .unwrap();
            builder.rewind(checkpoint);
            assert!(builder.store.prepared_constants.is_empty());
        });
    }

    /// A constant set is stored as its elements, and a payload that names
    /// one element twice is not a set.
    #[test]
    fn constant_set_value_round_trips_and_rejects_a_repeated_element() {
        use crate::map_key::MapKey;
        let mut builder = FullBuilder::new(SourceId::new(0));
        let elements = [MapKey::from("a"), MapKey::from("b")]
            .into_iter()
            .collect::<BTreeSet<_>>();
        let mut words = Vec::new();
        LoweredValue::Set(Arc::new(elements.clone()))
            .encode(&mut builder, &mut words)
            .unwrap();
        let index = words[0] as usize;
        assert_eq!(builder.store.values[index], FullValueTag::Set);
        let read = |store: &FullStore, verify: bool| {
            let decoder = FullDecoder {
                store,
                owner: 0,
                instruction_range: 0..0,
                instruction_states: None,
                instruction_proof: None,
                block_states: None,
                slot_count: 0,
                pattern_ceiling: Cell::new(usize::MAX),
                local_immutable_let_initializers: RefCell::default(),
                function_signature_index: None,
                verified: false,
            };
            let mut cursor = FullCursor::new(&words);
            if verify {
                LoweredValue::verify(&decoder, &mut cursor).map(|()| None)
            } else {
                LoweredValue::decode(&decoder, &mut cursor).map(Some)
            }
        };
        let Ok(Some(LoweredValue::Set(decoded))) = read(&builder.store, false) else {
            panic!("a set value");
        };
        assert_eq!(*decoded, elements);
        assert!(read(&builder.store, true).is_ok());

        // The payload is the length, then one value id per element.
        let payload = builder.store.value_data[index]
            .range()
            .bounds(builder.store.extra.len())
            .unwrap();
        assert_eq!(payload.len(), 3);
        builder.store.extra[payload.start + 2] = builder.store.extra[payload.start + 1];
        assert!(read(&builder.store, false).is_err());
        assert!(read(&builder.store, true).is_err());
    }

    #[test]
    fn prepared_cli_plan_pool_rewinds_with_builder_checkpoint() {
        run_with_large_stack(|| {
            let program = fixture(
                "cli-constant-descriptors.xsh",
                include_str!(
                    "../../../../tests/fixtures/frontend-indexed/cli-constant-descriptors.xsh"
                ),
            );
            let plan = Arc::clone(&program.store.prepared_module_call_plans[0]);
            let mut builder = FullBuilder::new(SourceId::new(0));
            let checkpoint = builder.checkpoint();
            let crate::runtime::eval::ModuleCallPlan::SignatureCli(inner) = plan.as_ref()
                else { panic!("CLI descriptor plan") };
            let separately_wrapped = Arc::new(crate::runtime::eval::ModuleCallPlan::SignatureCli(Arc::clone(inner)));
            assert!(!Arc::ptr_eq(&plan, &separately_wrapped));
            let mut words = Vec::new();
            plan.encode(&mut builder, &mut words).unwrap();
            separately_wrapped.encode(&mut builder, &mut words).unwrap();
            assert_eq!(words, [0, 0]);
            assert_eq!(builder.store.prepared_module_call_plans.len(), 1);
            builder.rewind(checkpoint);
            assert!(builder.store.prepared_module_call_plans.is_empty());
        });
    }

    #[test]
    fn prepared_regex_pool_rewinds_with_builder_checkpoint() {
        let mut builder = FullBuilder::new(SourceId::new(0));
        let checkpoint = builder.checkpoint();
        let value = RegexValue {
            pattern: "[a-z]+".to_string(),
            regex: Arc::new(
                crate::modules::regex::compile("[a-z]+", Span::new(SourceId::new(0), 0, 0))
                    .unwrap(),
            ),
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
            let program = Arc::new(fixture(
                "prepared-regex.xsh",
                "proc literal() -> Regex { rx\"[a-z]+\" }\n",
            ));
            let prepared = Arc::clone(&program.store.prepared_regexes[0].regex);
            let name = program_name(&program, "literal");
            for _ in 0..3 {
                let (result, _, _) = run_full(Arc::clone(&program), name);
                let Value::Regex(value) = result.unwrap() else {
                    panic!("expected prepared regex");
                };
                assert!(Arc::ptr_eq(&prepared, &value.regex));
                assert!(value.regex.is_match("text"));
            }
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            for _ in 0..3 {
                let result = evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(name),
                        LoweredFunctionKind::Proc,
                        &[],
                        Span::new(program.store.source_id, 0, 0),
                    )
                    .unwrap()
                    .unwrap();
                let Value::Regex(value) = result else {
                    panic!("expected prepared regex");
                };
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
                let declarations = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
                FullBuilder::build_compact(
                    &parsed.arena,
                    &declarations,
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
        assert_eq!(size_of::<FullFunction>(), 44);
        assert_eq!(size_of::<FullParam>(), 12);
        assert_eq!(size_of::<FullParamCold>(), 12);
        assert_eq!(size_of::<FullCapture>(), 12);
        assert_eq!(size_of::<FullFunctionMetadata>(), 8);
        assert_eq!(size_of::<FullValidation>(), 8);
        assert_eq!(size_of::<FullDriverStep>(), 40);
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
    fn lowered_value_tags_keep_wire_ordinals() {
        assert_eq!(
            [
                FullValueTag::Null as u8,
                FullValueTag::Unit as u8,
                FullValueTag::Int as u8,
                FullValueTag::Float as u8,
                FullValueTag::Duration as u8,
                FullValueTag::Bool as u8,
                FullValueTag::Str as u8,
                FullValueTag::Bytes as u8,
                FullValueTag::Regex as u8,
                FullValueTag::Path as u8,
                FullValueTag::Record as u8,
                FullValueTag::RecordVec as u8,
                FullValueTag::Stats as u8,
                FullValueTag::StatsBlob as u8,
                FullValueTag::Module as u8,
                FullValueTag::List as u8,
                FullValueTag::Map as u8,
                FullValueTag::Tag as u8,
                FullValueTag::ResultOk as u8,
                FullValueTag::Set as u8,
            ],
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19]
        );
    }

    #[test]
    fn pattern_tags_keep_wire_ordinals() {
        assert_eq!(
            [
                FullPatternTag::Alias as u8,
                FullPatternTag::Alternation as u8,
                FullPatternTag::List as u8,
                FullPatternTag::TagType as u8,
                FullPatternTag::RecordTest as u8,
                FullPatternTag::ResultTest as u8,
                FullPatternTag::TagTest as u8,
                FullPatternTag::ErrorTest as u8,
                FullPatternTag::Wildcard as u8,
                FullPatternTag::Bind as u8,
                FullPatternTag::Type as u8,
                FullPatternTag::Literal as u8,
                FullPatternTag::ResultOk as u8,
                FullPatternTag::ResultErr as u8,
                FullPatternTag::Facet as u8,
                FullPatternTag::Tag as u8,
                FullPatternTag::Text as u8,
            ],
            std::array::from_fn::<_, 17, _>(|index| {
                if index < 14 { index as u8 } else { index as u8 + 1 }
            }),
        );
    }

    #[test]
    fn driver_tags_keep_row_ordinals() {
        assert_eq!(
            [
                FullDriverTag::Skip as u8,
                FullDriverTag::Use as u8,
                FullDriverTag::Let as u8,
                FullDriverTag::LetRecord as u8,
                FullDriverTag::Assign as u8,
                FullDriverTag::Discard as u8,
                FullDriverTag::Stmt as u8,
                FullDriverTag::Expr as u8,
                FullDriverTag::Defer as u8,
                FullDriverTag::SignalHook as u8,
            ],
            std::array::from_fn::<_, 10, _>(|index| index as u8),
        );
    }

    #[test]
    fn stage_tags_keep_wire_ordinals() {
        assert_eq!(
            [
                FullStageTag::TextLines as u8,
                FullStageTag::JsonLines as u8,
                FullStageTag::Where as u8,
                FullStageTag::WhereBlock as u8,
                FullStageTag::Map as u8,
                FullStageTag::MapBlock as u8,
                FullStageTag::FlatMap as u8,
                FullStageTag::FlatMapBlock as u8,
                FullStageTag::BytesChunks as u8,
                FullStageTag::BatchCount as u8,
                FullStageTag::BatchMaxArgv as u8,
                FullStageTag::BatchMaxBytes as u8,
                FullStageTag::BatchLimits as u8,
                FullStageTag::Shuffle as u8,
                FullStageTag::Fold as u8,
                FullStageTag::ReduceBy as u8,
                FullStageTag::ReduceByConfigured as u8,
                FullStageTag::ParMap as u8,
                FullStageTag::ParMapBlock as u8,
                FullStageTag::ParMapFlatMapReduceBy as u8,
                FullStageTag::Tee as u8,
                FullStageTag::Each as u8,
                FullStageTag::TablePrint as u8,
                FullStageTag::TablePrintConfigured as u8,
                FullStageTag::Enumerate as u8,
                FullStageTag::Zip as u8,
                FullStageTag::Sort as u8,
                FullStageTag::SortBy as u8,
                FullStageTag::GroupBy as u8,
                FullStageTag::CountBy as u8,
                FullStageTag::Any as u8,
                FullStageTag::AnyBlock as u8,
                FullStageTag::All as u8,
                FullStageTag::AllBlock as u8,
                FullStageTag::UniqueBy as u8,
                FullStageTag::Count as u8,
                FullStageTag::Sum as u8,
                FullStageTag::Collect as u8,
                FullStageTag::First as u8,
                FullStageTag::Last as u8,
                FullStageTag::Min as u8,
                FullStageTag::Max as u8,
                FullStageTag::Take as u8,
                FullStageTag::Drop as u8,
                FullStageTag::Repeat as u8,
                FullStageTag::Range as u8,
            ],
            std::array::from_fn::<_, 46, _>(|index| index as u8),
        );
    }

    #[test]
    fn stage_schema_views_match_all_owned_fields_and_leave_trailing_fields() {
        let (builder, rows, block, expressions, statements) = stage_view_fixture();
        let execution = schema_execution_with_slots(&builder.store, 25);
        let owned_block = [block];
        let mut owned_input = FullCursor::new(&owned_block);
        let decoder = execution.decoder();
        let decoded_owned = Vec::<LoweredPipelineStage>::decode(&decoder, &mut owned_input)
            .unwrap();
        owned_input.finish().unwrap();
        assert_eq!(decoded_owned.len(), rows.len());

        let outer_payload = [block, 0x5a5a];
        let mut payload = FullPayload {
            store: &builder.store,
            cursor: FullCursor::new(&outer_payload),
        };
        let mut views = payload.borrow::<Vec<LoweredPipelineStage>>(&execution).unwrap();
        assert_eq!(views.len(), 46);
        let expectations = StageViewExpectations {
            store: &builder.store,
            expressions: &expressions,
            statements: &statements,
        };
        for (index, owned) in rows.into_iter().enumerate() {
            let stage = views.next_with_id().unwrap().unwrap();
            assert_eq!(stage.id.raw(), index as u32);
            assert_eq!(stage.view.tag() as usize, index);
            assert_stage_view_matches(owned, stage.view, &expectations);
        }
        assert!(views.next_with_id().is_none());
        assert_eq!(payload.raw().unwrap(), 0x5a5a);
        payload.finish().unwrap();
    }

    #[test]
    fn borrowed_stage_list_views_reject_trailing_words() {
        let (builder, _, block, _, _) = stage_view_fixture();
        let mut trailing = builder.store.clone();
        let block = IrBlockId::from_raw(block).unwrap();
        trailing.extra.push(0);
        trailing.blocks[block.index()].instructions.len += 1;
        let execution = schema_execution_with_slots(&trailing, 25);
        let encoded_block = [block.raw()];
        let mut payload = FullPayload {
            store: &trailing,
            cursor: FullCursor::new(&encoded_block),
        };
        let mut stages = payload.borrow::<Vec<LoweredPipelineStage>>(&execution).unwrap();
        for _ in 0..45 {
            stages.next_with_id().unwrap().unwrap();
        }
        let error = match stages.next_with_id().unwrap() {
            Err(error) => error,
            Ok(_) => panic!("stage list with trailing words was accepted"),
        };
        assert!(error.message.contains("full IR payload has trailing words"));
        payload.finish().unwrap();
    }

    #[test]
    fn borrowed_stage_body_reuses_its_validated_statement_block() {
        let (builder, _, _, _, _) = stage_view_fixture();
        let execution = schema_execution_with_slots(&builder.store, 25);
        let StageView::Tee { body, .. } = execution.stage_view(20).unwrap() else {
            panic!("stage 20 is the Tee fixture row");
        };
        let (_, mut statements) = execution
            .block_id(body.block_id(), BLOCK_STATEMENTS)
            .unwrap();
        assert_eq!(statements.raw().unwrap(), 2);
        let first = statements.raw().unwrap();
        let second = statements.raw().unwrap();
        statements.finish().unwrap();
        assert_eq!(execution.instruction_id(first).unwrap().0, FullTag::StmtValue);
        assert_eq!(execution.instruction_id(second).unwrap().0, FullTag::StmtExpr);
    }

    #[test]
    fn stage_schema_views_reject_malformed_trailing_and_foreign_payloads() {
        let (builder, _, _, _, _) = stage_view_fixture();

        let mut malformed = builder.store.clone();
        let reduce_by = malformed.stage_data[15].range();
        malformed.extra[reduce_by.start as usize + 3] = u32::MAX;
        let malformed_execution = schema_execution_with_slots(&malformed, 25);
        let mut malformed_payload = FullPayload {
            store: &malformed,
            cursor: FullCursor::new(&[15]),
        };
        let error = match malformed_payload.borrow::<LoweredPipelineStage>(&malformed_execution) {
            Err(error) => error,
            Ok(_) => panic!("invalid reduce-by operation was accepted"),
        };
        assert!(error.message.contains("reduce-by operation is invalid"));

        let mut trailing = builder.store.clone();
        let start = u32::try_from(trailing.extra.len()).unwrap();
        trailing.extra.push(0);
        trailing.stage_data[0] = IrData { lhs: start, rhs: 1 };
        let trailing_execution = schema_execution_with_slots(&trailing, 25);
        let mut trailing_payload = FullPayload {
            store: &trailing,
            cursor: FullCursor::new(&[0]),
        };
        let error = match trailing_payload.borrow::<LoweredPipelineStage>(&trailing_execution) {
            Err(error) => error,
            Ok(_) => panic!("trailing stage row word was accepted"),
        };
        assert!(error.message.contains("full IR payload has trailing words"));

        let other_store = builder.store.clone();
        let other_execution = schema_execution_with_slots(&other_store, 25);
        let mut foreign_payload = FullPayload {
            store: &builder.store,
            cursor: FullCursor::new(&[15]),
        };
        let error = match foreign_payload.borrow::<LoweredPipelineStage>(&other_execution) {
            Err(error) => error,
            Ok(_) => panic!("foreign stage payload was accepted"),
        };
        assert!(error.message.contains("another program"));
        assert_eq!(foreign_payload.raw().unwrap(), 15);
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
                let declarations = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
                assert!(
                    declarations.diagnostics.is_empty(),
                    "{:?}",
                    declarations.diagnostics
                );
                let shared_sources = Arc::new(sources.clone());
                let program = FullBuilder::build_compact(
                    &parsed.arena,
                    &declarations,
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
        let declarations = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        let program = FullBuilder::build_compact(
            &parsed.arena,
            &declarations,
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
    fn driver_verifier_rejects_invalid_signal_hook_option_tag() {
        let mut sources = SourceMap::new();
        let source_id =
            sources.add_file("top-level-driver-boundary.xsh", TOP_LEVEL_DRIVER_BOUNDARY);
        let parsed = Parser::parse_source_arena_only(source_id, TOP_LEVEL_DRIVER_BOUNDARY);
        let declarations = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        let program = FullBuilder::build_compact(
            &parsed.arena,
            &declarations,
            TOP_LEVEL_DRIVER_BOUNDARY,
            Arc::new(sources),
            source_id,
        )
        .unwrap();
        let mut malformed = program;
        let step = malformed
            .store
            .driver_steps
            .iter()
            .find(|step| step.tag == FullDriverTag::SignalHook)
            .expect("boundary fixture includes a signal hook");
        malformed.store.extra[step.data.range().start as usize + 1] = u32::MAX;

        let error = FullVerifier::verify(&malformed).unwrap_err();
        assert!(error.message.contains("optional payload tag is invalid"));
    }

    #[test]
    fn driver_propagated_process_failure_records_process_propagation_and_trace_effects() {
        let source = "run false ?\n";
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("top-level-propagate.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        let declarations = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        let program = FullBuilder::build_compact(
            &parsed.arena,
            &declarations,
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
    fn verifier_rejects_empty_record_update_path_and_bad_replacement_reference() {
        run_with_large_stack(|| {
            let program = fixture(
                "record-update.xsh",
                "pure value() -> Int { let base = {a: {b: 1, c: 0}}; return {...base, a.b: 2, a.c: 3}.a.b }\n",
            );
            let instruction = program
                .store
                .tags
                .iter()
                .position(|tag| *tag == FullTag::ExprRecordUpdate)
                .unwrap();
            let payload = program.store.data[instruction]
                .range()
                .bounds(program.store.extra.len())
                .unwrap();
            let updates = IrBlockId::from_raw(program.store.extra[payload.start + 1]).unwrap();
            let entries = program.store.blocks[updates.index()]
                .instructions
                .bounds(program.store.extra.len())
                .unwrap();
            let path = IrBlockId::from_raw(program.store.extra[entries.start + 1]).unwrap();
            let names = program.store.blocks[path.index()]
                .instructions
                .bounds(program.store.extra.len())
                .unwrap();
            let mut empty = program.clone();
            empty.store.extra[names.start] = 0;
            empty.store.blocks[path.index()].instructions.len = 1;
            let error = FullVerifier::verify(&empty).unwrap_err();
            assert!(
                error.message.contains("nonempty and disjoint"),
                "{}",
                error.message
            );
            let mut bad_child = program.clone();
            bad_child.store.extra[entries.start + 2] = u32::MAX;
            assert!(FullVerifier::verify(&bad_child).is_err());
            let mut overlapping = program.clone();
            let second_path = IrBlockId::from_raw(program.store.extra[entries.start + 4]).unwrap();
            let second_names = program.store.blocks[second_path.index()]
                .instructions
                .bounds(program.store.extra.len())
                .unwrap();
            overlapping.store.extra[second_names.start + 2] = program.store.extra[names.start + 2];
            let error = FullVerifier::verify(&overlapping).unwrap_err();
            assert!(
                error.message.contains("nonempty and disjoint"),
                "{}",
                error.message
            );
            let program = Arc::new(program);
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "value")),
                    LoweredFunctionKind::Pure,
                    &[],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("update function exists");
            assert_eq!(result.unwrap(), Value::Int(2));
        });
    }

    #[test]
    fn fs_root_methods_keep_opaque_identity_and_defaults_on_both_indexed_routes() {
        run_with_large_stack(|| {
            let source =
                include_str!("../../../../tests/fixtures/frontend-indexed/fs-root-methods.xsh");
            let program = Arc::new(fixture("fs-root-methods.xsh", source));
            FullVerifier::verify(&program).unwrap();
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "root_methods")),
                    LoweredFunctionKind::Proc,
                    &[],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("filesystem root fixture exists");
            assert_eq!(result.unwrap(), Value::ok(Value::Str(Arc::from("payload"))));
        });
    }

    #[test]
    fn cli_command_descriptor_plans_execute_both_indexed_routes() {
        run_with_large_stack(|| {
            let source = include_str!(
                "../../../../tests/fixtures/frontend-indexed/cli-constant-commands.xsh"
            );
            let program = fixture("cli-constant-commands.xsh", source);
            assert_eq!(program.store.prepared_module_call_plans.len(), 1);
            assert!(program.store.prepared_module_call_plans[0].matches_operation(RuntimeOp::CliCommands));
            assert!(!program.store.prepared_module_call_plans[0].matches_operation(RuntimeOp::CliParse));
            FullVerifier::verify(&program).unwrap();
            let program = Arc::new(program);
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "command_values")),
                    LoweredFunctionKind::Proc,
                    &[],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("command descriptor function exists");
            assert_eq!(
                result.unwrap(),
                Value::ok(Value::Str(Arc::from("workspace/build")))
            );
        });
    }

    #[test]
    fn cli_descriptor_plans_share_preparation_and_execute_both_indexed_routes() {
        run_with_large_stack(|| {
            let source = include_str!(
                "../../../../tests/fixtures/frontend-indexed/cli-constant-descriptors.xsh"
            );
            let program = fixture("cli-constant-descriptors.xsh", source);
            assert_eq!(program.store.prepared_module_call_plans.len(), 2);
            FullVerifier::verify(&program).unwrap();
            let instruction = program
                .store
                .tags
                .iter()
                .position(|tag| *tag == FullTag::ExprModuleCall)
                .unwrap();
            let payload = program.store.data[instruction]
                .range()
                .bounds(program.store.extra.len())
                .unwrap();
            let mut broken = program.clone();
            broken.store.extra[payload.start + 2] = u32::MAX;
            assert!(
                FullVerifier::verify(&broken)
                    .unwrap_err()
                    .message
                    .contains("module call plan is out of bounds")
            );
            let mut wrong_policy = program.clone();
            let applet = program
                .store
                .prepared_module_call_plans
                .iter()
                .position(|plan| plan.matches_operation(RuntimeOp::CliApplet))
                .unwrap();
            wrong_policy.store.extra[payload.start + 2] = applet as u32;
            assert!(
                FullVerifier::verify(&wrong_policy)
                    .unwrap_err()
                    .message
                    .contains("operation policy")
            );
            let program = Arc::new(program);
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "descriptor_values")),
                    LoweredFunctionKind::Proc,
                    &[],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("descriptor function exists");
            assert_eq!(result.unwrap(), Value::ok(Value::Str(Arc::from("6/4/3"))));
        });
    }

    #[test]
    fn native_path_interpolation_executes_both_indexed_routes() {
        run_with_large_stack(|| {
            let source = "pure native_path(value: Path) -> Path { return fp\"prefix/{value}/../end\" }\nproc native_plan(value: Path) [process, error] -> Command { return process.command { stdin = fp\"before/{value}\"; stdout = fp\"{value}/after\"; run true \"--target=$value\" } }\n";
            let program = Arc::new(fixture("native-path-interpolation.xsh", source));
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let argument = Value::Path(PathValue::new(b"raw\xff name".to_vec()).unwrap());
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "native_path")),
                    LoweredFunctionKind::Pure,
                    std::slice::from_ref(&argument),
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("native path function exists");
            assert_eq!(
                result.unwrap(),
                Value::Path(PathValue::new(b"prefix/raw\xff name/../end".to_vec()).unwrap())
            );
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "native_plan")),
                    LoweredFunctionKind::Proc,
                    std::slice::from_ref(&argument),
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("native command plan exists");
            let Value::Command(plan) = result.unwrap() else {
                panic!("command plan");
            };
            assert_eq!(plan.argv.last().unwrap(), b"--target=raw\xff name");
            let crate::runtime::value::CommandRedirection::File { path, .. } =
                &plan.redirections[0]
            else {
                panic!("file input redirection");
            };
            assert_eq!(path.bytes, b"before/raw\xff name");
            let crate::runtime::value::CommandRedirection::File { path, .. } =
                &plan.redirections[1]
            else {
                panic!("file output redirection");
            };
            assert_eq!(path.bytes, b"raw\xff name/after");
        });
    }

    #[test]
    fn verifier_rejects_empty_or_unknown_comprehension_qualifiers() {
        let program = fixture(
            "indexed-comprehension.xsh",
            "pure values() -> List[Int] { return [inner for outer in [1] if outer > 0 for inner in [outer]] }\n",
        );
        let instruction = program
            .store
            .tags
            .iter()
            .position(|tag| *tag == FullTag::ExprListComp)
            .unwrap();
        let payload = program.store.data[instruction]
            .range()
            .bounds(program.store.extra.len())
            .unwrap();
        let block = IrBlockId::from_raw(program.store.extra[payload.start + 1]).unwrap();
        let entries = program.store.blocks[block.index()]
            .instructions
            .bounds(program.store.extra.len())
            .unwrap();

        let mut empty = program.clone();
        empty.store.extra[entries.start] = 0;
        empty.store.blocks[block.index()].instructions.len = 1;
        let error = FullVerifier::verify(&empty).unwrap_err();
        assert!(
            error.message.contains("must start with for"),
            "{}",
            error.message
        );

        let mut unknown = program;
        unknown.store.extra[entries.start + 1] = u32::MAX;
        let error = FullVerifier::verify(&unknown).unwrap_err();
        assert!(
            error.message.contains("qualifier tag is invalid"),
            "{}",
            error.message
        );
    }

    /// A deferred action carries whether it is an `errdefer` as one boolean
    /// word, in a function body and at the top level alike. Any other value
    /// would be read as one of the two, so the verifier rejects it.
    #[test]
    fn verifier_rejects_a_deferred_action_whose_trigger_is_not_a_boolean() {
        let program = fixture(
            "indexed-errdefer.xsh",
            "proc step() -> Int {\n  errdefer { print \"undo\" }\n  defer { print \"done\" }\n  1\n}\n\nerrdefer { print \"undo\" }\ndefer { print \"done\" }\nprint $step()\n",
        );
        FullVerifier::verify(&program).unwrap();

        // In a function body the payload is the action and the trigger.
        let statement_triggers = program
            .store
            .tags
            .iter()
            .enumerate()
            .filter(|(_, tag)| **tag == FullTag::StmtDefer)
            .map(|(instruction, _)| {
                let payload = program.store.data[instruction]
                    .range()
                    .bounds(program.store.extra.len())
                    .unwrap();
                assert_eq!(payload.len(), 2);
                payload.start + 1
            })
            .collect::<Vec<_>>();
        let mut written = statement_triggers
            .iter()
            .map(|word| program.store.extra[*word])
            .collect::<Vec<_>>();
        written.sort_unstable();
        assert_eq!(written, [0, 1], "one `defer` and one `errdefer`");

        // At the top level it is the action, the trigger, and the span.
        let driver_triggers = program
            .store
            .driver_steps
            .iter()
            .filter(|step| step.tag == FullDriverTag::Defer)
            .map(|step| {
                step.data
                    .range()
                    .bounds(program.store.extra.len())
                    .unwrap()
                    .start
                    + 1
            })
            .collect::<Vec<_>>();
        assert_eq!(driver_triggers.len(), 2);
        assert!(program.driver_step_defers_on_error(0).is_err());
        assert_eq!(program.driver_step_defers_on_error(1).ok(), Some(true));
        assert_eq!(program.driver_step_defers_on_error(2).ok(), Some(false));

        for word in statement_triggers.into_iter().chain(driver_triggers) {
            let mut corrupt = program.clone();
            corrupt.store.extra[word] = 2;
            let error = FullVerifier::verify(&corrupt).unwrap_err();
            assert!(
                error.message.contains("boolean payload"),
                "{}",
                error.message
            );
        }
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
        let source =
            include_str!("../../../../tests/fixtures/frontend-indexed/pattern-aliases.xsh");
        let program = fixture("pattern-aliases.xsh", source);
        program.symbol_owner().with_current(|| {
            let view = program
                .function_view(
                    LoweredFunctionKey::Name(Name::intern("select")),
                    LoweredFunctionKind::Pure,
                )
                .unwrap()
                .unwrap();
            let execution = view.execution().unwrap();
            let mut slots = vec![LoweredValue::Unit; view.header().unwrap().slot_count];
            let pattern = program
                .store
                .patterns
                .iter()
                .position(|tag| *tag == FullPatternTag::Alias)
                .unwrap() as u32;
            let span = Span::new(crate::source::SourceId::new(0), 0, 0);
            let failed = LoweredValue::List(vec![
                LoweredValue::List(vec![LoweredValue::Int(1)]),
                LoweredValue::List(vec![LoweredValue::Int(2)]),
            ]);
            assert!(
                !Evaluator::indexed_pattern_matches(&execution, pattern, &failed, &mut slots, span)
                    .unwrap()
            );
            assert!(slots.iter().all(|slot| matches!(slot, LoweredValue::Unit)));
            let matched = LoweredValue::List(vec![
                LoweredValue::List(vec![LoweredValue::Int(99)]),
                LoweredValue::List(vec![LoweredValue::Int(7), LoweredValue::Int(8)]),
            ]);
            assert!(
                Evaluator::indexed_pattern_matches(&execution, pattern, &matched, &mut slots, span)
                    .unwrap()
            );
            assert!(slots.iter().any(|slot| *slot == LoweredValue::Int(7)));
            assert!(
                slots
                    .iter()
                    .any(|slot| *slot == LoweredValue::List(vec![LoweredValue::Int(8)]))
            );
            assert!(slots.contains(&matched));
        });
    }

    #[test]
    fn pattern_aliases_compact_facts_preserve_captured_subject_and_element_types() {
        let source =
            include_str!("../../../../tests/fixtures/frontend-indexed/pattern-aliases.xsh");
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("pattern-aliases.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty());
        let declarations = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        parsed.arena.symbol_owner().with_current(|| {
            for (id, ty) in &declarations.bodies.expr_types {
                match parsed.arena.arena.expr(*id).kind {
                    crate::syntax::arena::ArenaExprKind::Ident(name)
                        if name == "value" || name == "left" || name == "right" =>
                    {
                        assert_eq!(*ty, Type::Int)
                    }
                    crate::syntax::arena::ArenaExprKind::Ident(name) if name == "tail" => {
                        assert_eq!(*ty, Type::List(Box::new(Type::Int)))
                    }
                    crate::syntax::arena::ArenaExprKind::Ident(name) if name == "original" => {
                        assert_eq!(*ty, Type::List(Box::new(Type::List(Box::new(Type::Int)))))
                    }
                    _ => {}
                }
            }
        });
    }

    #[test]
    fn verifier_rejects_incompatible_alternative_and_alias_capture_slots() {
        let source =
            include_str!("../../../../tests/fixtures/frontend-indexed/pattern-aliases.xsh");
        let program = fixture("pattern-aliases.xsh", source);
        let alias = program
            .store
            .patterns
            .iter()
            .position(|tag| *tag == FullPatternTag::Alias)
            .unwrap();
        let alias_payload = program.store.pattern_data[alias]
            .range()
            .bounds(program.store.extra.len())
            .unwrap();
        let mut bad_alias = program.clone();
        bad_alias.store.extra[alias_payload.start + 1] = u32::MAX;
        assert!(FullVerifier::verify(&bad_alias).is_err());
        let mut alias_cycle = program.clone();
        alias_cycle.store.extra[alias_payload.start] = alias as u32;
        assert!(
            FullVerifier::verify(&alias_cycle)
                .unwrap_err()
                .message
                .contains("nested pattern")
        );
        let bind = program
            .store
            .patterns
            .iter()
            .rposition(|tag| *tag == FullPatternTag::Bind)
            .unwrap();
        let range = program.store.pattern_data[bind]
            .range()
            .bounds(program.store.extra.len())
            .unwrap();
        let mut duplicate = program.clone();
        duplicate.store.extra[range.start] = 0;
        assert!(FullVerifier::verify(&duplicate).is_err());
    }

    #[test]
    fn direct_scalar_iteration_runs_after_frontend_drop() {
        run_with_large_stack(|| {
            let source =
                include_str!("../../../../tests/fixtures/frontend-indexed/scalar-iteration.xsh");
            let program = Arc::new(fixture("scalar-iteration.xsh", source));
            let execute = |name: &str| {
                let name = program_name(&program, name);
                run_full(program.clone(), name).0.unwrap()
            };
            for (name, expected) in [("scalar_count", 261), ("scalar_comp", 3)] {
                assert_eq!(execute(name), Value::Int(expected));
            }
            let Value::Result(crate::runtime::value::ResultValue::Ok(value)) =
                execute("scalar_result")
            else {
                panic!("Result iterable")
            };
            assert_eq!(*value, Value::Int(255));
            let Value::Result(crate::runtime::value::ResultValue::Err(error)) =
                execute("scalar_failure")
            else {
                panic!("source failure")
            };
            let Value::Error(error) = *error else {
                panic!("nominal source failure")
            };
            assert_eq!(error.kind, "ScalarFailure.Missing");
            assert_eq!(
                error
                    .contexts
                    .iter()
                    .map(|context| context.message.as_deref())
                    .collect::<Vec<_>>(),
                [Some("scalar source")]
            );
        });
    }

    #[test]
    fn lexical_ctx_indexed_execution_preserves_context_order_and_region_spans() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/error-context.xsh");
        let program = Arc::new(fixture("error-context.xsh", source));
        let value = program_name(&program, "contextual_value");
        assert_eq!(run_full(program.clone(), value).0.unwrap(), Value::Int(7));
        let failure = program_name(&program, "contextual_failure");
        let Value::Result(crate::runtime::value::ResultValue::Err(error)) =
            run_full(program, failure).0.unwrap()
        else {
            panic!("expected propagated error")
        };
        let Value::Error(error) = *error else {
            panic!("expected error")
        };
        assert_eq!(
            error
                .contexts
                .iter()
                .map(|context| context.message.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("inner"), Some("outer")]
        );
        assert!(
            error
                .contexts
                .iter()
                .all(|context| context.span.is_some_and(|span| source
                    .get(span.range())
                    .is_some_and(|text| text.starts_with("ctx "))))
        );
        assert_eq!(error.kind, "validation");
    }

    #[test]
    fn list_patterns_validate_before_publishing_capture_slots() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/list-patterns.xsh");
        let program = fixture("list-patterns.xsh", source);
        program.symbol_owner().with_current(|| {
            let view = program
                .function_view(
                    LoweredFunctionKey::Name(Name::intern("nested")),
                    LoweredFunctionKind::Pure,
                )
                .unwrap()
                .unwrap();
            let execution = view.execution().unwrap();
            let count = view.header().unwrap().slot_count;
            let pattern = program
                .store
                .patterns
                .iter()
                .rposition(|tag| *tag == FullPatternTag::List)
                .unwrap() as u32;
            let span = Span::new(crate::source::SourceId::new(0), 0, 0);
            let list = |last| {
                LoweredValue::List(vec![
                    LoweredValue::List(vec![LoweredValue::Int(1), LoweredValue::Int(2)]),
                    LoweredValue::List(vec![LoweredValue::Int(last)]),
                ])
            };
            let mut slots = vec![LoweredValue::Unit; count];
            assert!(
                !Evaluator::indexed_pattern_matches(
                    &execution,
                    pattern,
                    &list(3),
                    &mut slots,
                    span
                )
                .unwrap()
            );
            assert!(
                slots
                    .iter()
                    .all(|value| matches!(value, LoweredValue::Unit))
            );
            assert!(
                Evaluator::indexed_pattern_matches(
                    &execution,
                    pattern,
                    &list(99),
                    &mut slots,
                    span
                )
                .unwrap()
            );
            assert!(slots.iter().any(|value| *value == LoweredValue::Int(1)));
            assert!(
                slots
                    .iter()
                    .any(|value| *value == LoweredValue::List(vec![LoweredValue::Int(2)]))
            );
        });
    }

    #[test]
    fn verifier_rejects_binding_and_missing_retry_selection_patterns() {
        let source =
            include_str!("../../../../tests/fixtures/frontend-indexed/selective-retry.xsh");
        let program = fixture("selective-retry.xsh", source);
        FullVerifier::verify(&program).unwrap();
        let retry = program
            .store
            .tags
            .iter()
            .position(|tag| *tag == FullTag::ExprRetry)
            .unwrap();
        let payload = program.store.data[retry]
            .range()
            .bounds(program.store.extra.len())
            .unwrap();
        let binding = program
            .store
            .patterns
            .iter()
            .position(|tag| *tag == FullPatternTag::ResultOk)
            .unwrap();
        let mut invalid = program.clone();
        invalid.store.extra[payload.start + 2] = binding as u32;
        assert!(
            FullVerifier::verify(&invalid)
                .unwrap_err()
                .message
                .contains("cannot bind")
        );
        let mut missing = program.clone();
        missing.store.extra[payload.start + 2] = u32::MAX;
        assert!(FullVerifier::verify(&missing).is_err());
    }

    /// A `collect` block lowers to a local list and an append per yield, so
    /// its yields must never reach the program as producer yields, and an
    /// append must name a slot of the function that holds the block.
    #[test]
    fn collect_yields_lower_to_appends_that_the_verifier_ties_to_a_slot() {
        let program = fixture(
            "collect.xsh",
            "let squares = collect {\n  for n in [1, 2] {\n    yield n * n\n  }\n  yield @[7, 8]\n}\n",
        );
        FullVerifier::verify(&program).unwrap();
        let tags = &program.store.tags;
        assert!(
            !tags
                .iter()
                .any(|tag| matches!(tag, FullTag::StmtYield | FullTag::StmtYieldDelegate)),
            "a collect yield lowered as a producer yield"
        );
        let appends: Vec<usize> = tags
            .iter()
            .enumerate()
            .filter(|(_, tag)| **tag == FullTag::StmtAssign)
            .map(|(index, _)| index)
            .collect();
        assert_eq!(appends.len(), 2, "one append for each yield");
        for append in appends {
            // The slot is the row's first word.
            let payload = program.store.data[append]
                .range()
                .bounds(program.store.extra.len())
                .unwrap();
            let mut invalid = program.clone();
            invalid.store.extra[payload.start] = u32::MAX - 1;
            assert!(
                FullVerifier::verify(&invalid)
                    .unwrap_err()
                    .message
                    .contains("slot"),
                "an append to a slot the function does not have verified"
            );
        }
    }

    /// The executor reads a backoff's three durations by position, so a
    /// backoff row over any other number of delays, or a row whose schedule
    /// is no schedule, must not verify.
    #[test]
    fn verifier_rejects_a_retry_backoff_without_its_three_durations() {
        let listed = fixture(
            "retry-listed.xsh",
            "let value = retry [1ms, 2ms] {\n  1\n}\n",
        );
        let backoff = fixture(
            "retry-backoff.xsh",
            "let value = retry backoff 1ms..2ms within 5ms {\n  1\n}\n",
        );
        FullVerifier::verify(&listed).unwrap();
        FullVerifier::verify(&backoff).unwrap();
        // The schedule is the row's last word, after the delays, the
        // selection's presence, the body, and the span.
        let schedule_word = |program: &FullProgram| {
            let retry = program
                .store
                .tags
                .iter()
                .position(|tag| *tag == FullTag::ExprRetry)
                .unwrap();
            let payload = program.store.data[retry]
                .range()
                .bounds(program.store.extra.len())
                .unwrap();
            payload.end - 1
        };

        // Two written delays read as a backoff.
        let mut short = listed.clone();
        let word = schedule_word(&short);
        assert_eq!(short.store.extra[word], 0);
        short.store.extra[word] = 1;
        assert!(
            FullVerifier::verify(&short)
                .unwrap_err()
                .message
                .contains("retry backoff must have a first interval, a cap, and a limit")
        );

        // A backoff's three durations are a valid written list, and no other
        // schedule exists.
        let mut relisted = backoff.clone();
        let word = schedule_word(&relisted);
        assert_eq!(relisted.store.extra[word], 1);
        relisted.store.extra[word] = 0;
        FullVerifier::verify(&relisted).unwrap();
        relisted.store.extra[word] = 2;
        assert!(
            FullVerifier::verify(&relisted)
                .unwrap_err()
                .message
                .contains("retry schedule is invalid")
        );
    }

    #[test]
    fn verifier_rejects_invalid_list_rest_patterns() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/list-patterns.xsh");
        let program = fixture("list-patterns.xsh", source);
        let parent = program
            .store
            .patterns
            .iter()
            .position(|tag| *tag == FullPatternTag::List)
            .unwrap();
        let range = program.store.pattern_data[parent]
            .range()
            .bounds(program.store.extra.len())
            .unwrap();
        let mut invalid = program.clone();
        invalid.store.extra[range.start + 2] = parent as u32;
        assert!(
            FullVerifier::verify(&invalid)
                .unwrap_err()
                .message
                .contains("list rest")
        );
        let mut missing = program.clone();
        missing.store.extra[range.start + 2] = u32::MAX;
        assert!(FullVerifier::verify(&missing).is_err());
    }

    #[test]
    fn verifier_checks_with_binding_slots_and_both_branch_returns() {
        let source =
            include_str!("../../../../tests/fixtures/frontend-indexed/block-parameters.xsh");
        let program = fixture("block-parameters.xsh", source);
        FullVerifier::verify(&program).unwrap();
        let row = program
            .store
            .tags
            .iter()
            .position(|tag| *tag == FullTag::StmtWith)
            .unwrap();
        let payload = program.store.data[row]
            .range()
            .bounds(program.store.extra.len())
            .unwrap();
        let bindings = IrBlockId::from_raw(program.store.extra[payload.start]).unwrap();
        let bindings = program.store.blocks[bindings.index()]
            .instructions
            .bounds(program.store.extra.len())
            .unwrap();
        let mut bad_slot = program.clone();
        bad_slot.store.extra[bindings.start + 1] = u32::MAX;
        assert!(
            FullVerifier::verify(&bad_slot)
                .unwrap_err()
                .message
                .contains("slot")
        );
        assert!(indexed_stmt_can_return(&program.store, row).unwrap());
    }

    #[test]
    fn verifier_checks_pattern_condition_capture_slots_and_branch_returns() {
        let source =
            include_str!("../../../../tests/fixtures/frontend-indexed/pattern-conditionals.xsh");
        let program = fixture("pattern-conditionals.xsh", source);
        FullVerifier::verify(&program).unwrap();
        let row = program
            .store
            .tags
            .iter()
            .position(|tag| *tag == FullTag::StmtPatternWhile)
            .unwrap();
        let payload = program.store.data[row]
            .range()
            .bounds(program.store.extra.len())
            .unwrap();
        let capture_block = IrBlockId::from_raw(program.store.extra[payload.start + 2]).unwrap();
        let captures = program.store.blocks[capture_block.index()]
            .instructions
            .bounds(program.store.extra.len())
            .unwrap();
        let mut bad_slot = program.clone();
        bad_slot.store.extra[captures.start + 1] = u32::MAX;
        assert!(
            FullVerifier::verify(&bad_slot)
                .unwrap_err()
                .message
                .contains("slot")
        );
    }

    #[test]
    fn verifier_rejects_nested_pattern_cycles_and_missing_children() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/pattern-tests.xsh");
        let program = fixture("pattern-tests.xsh", source);
        let parent = program
            .store
            .patterns
            .iter()
            .position(|tag| *tag == FullPatternTag::ResultTest)
            .unwrap();
        let range = program.store.pattern_data[parent]
            .range()
            .bounds(program.store.extra.len())
            .unwrap();
        let mut cycle = program.clone();
        cycle.store.extra[range.start + 1] = parent as u32;
        assert!(
            FullVerifier::verify(&cycle)
                .unwrap_err()
                .message
                .contains("nested pattern")
        );
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

    /// An index from the end carries the distance the checker read from the
    /// literal. Lowering never writes zero, which would read one past the
    /// end, so a program that holds one is rejected before it runs.
    #[test]
    fn an_index_from_the_end_counts_from_one() {
        run_with_large_stack(|| {
            let source = "pure last(items: List[Int]) -> Int {\n  items[-2]\n}\n";
            let program = fixture("index-from-end.xsh", source);
            FullVerifier::verify(&program).unwrap();
            assert!(!program.store.tags.contains(&FullTag::ExprIndex));
            let row = program
                .store
                .tags
                .iter()
                .position(|tag| *tag == FullTag::ExprIndexFromEnd)
                .expect("the negative literal lowers to an index from the end");
            let payload = program.store.data[row]
                .range()
                .bounds(program.store.extra.len())
                .unwrap();
            assert_eq!(program.store.extra[payload.start + 1], 2);
            let mut zero = program;
            zero.store.extra[payload.start + 1] = 0;
            assert!(
                FullVerifier::verify(&zero)
                    .unwrap_err()
                    .message
                    .contains("counts from one")
            );
        });
    }

    /// A propagation carries the place it is written at, after its operand.
    /// A row whose place is not a location of the program, or that has lost
    /// it, is rejected before it runs.
    #[test]
    fn a_propagation_row_carries_its_own_place() {
        run_with_large_stack(|| {
            let source = "pure width(field: Str) -> Result[Int] {\n  let value = field.parse_int()?\n  Ok(value)\n}\n";
            let program = Arc::new(fixture("propagation-place.xsh", source));
            FullVerifier::verify(&program).unwrap();
            let propagation = program
                .store
                .tags
                .iter()
                .position(|tag| *tag == FullTag::ExprTry)
                .expect("`?` lowers to a propagation");
            let words = program.store.data[propagation]
                .range()
                .bounds(program.store.extra.len())
                .unwrap();
            assert_eq!(words.len(), 2, "a propagation is its operand and its place");

            let mut misplaced = (*program).clone();
            misplaced.store.extra[words.start + 1] = u32::MAX;
            assert!(
                FullVerifier::verify(&misplaced).is_err(),
                "a propagation's place must be a location of the program"
            );

            let mut dangling = (*program).clone();
            dangling.store.extra[words.start] = u32::MAX;
            assert!(
                FullVerifier::verify(&dangling).is_err(),
                "a propagation's operand must be an instruction of the program"
            );
        });
    }

    /// A conversion lowers to the operation the checker selected, under a
    /// propagation, and to nothing else: no instruction is specific to it.
    /// Lowering reads the selection and never works it out, so a conversion
    /// with no selection is refused instead of being given a default.
    #[test]
    fn a_conversion_lowers_to_the_operation_the_checker_selected() {
        run_with_large_stack(|| {
            let source = "pure width(field: Str) -> Result[Int] {\n  Ok(field as Int)\n}\npure size(count: Int) -> Result[UInt] {\n  Ok(count as UInt)\n}\npure place(name: Str) -> Result[Path] {\n  Ok(name as Path)\n}\n";
            let program = Arc::new(fixture("conversion.xsh", source));
            FullVerifier::verify(&program).unwrap();
            let count = |tag| {
                program
                    .store
                    .tags
                    .iter()
                    .filter(|found| **found == tag)
                    .count()
            };
            assert_eq!(count(FullTag::ExprTry), 3);
            assert_eq!(count(FullTag::ExprMethod), 1);
            assert_eq!(count(FullTag::ExprRequire), 1);
            assert_eq!(count(FullTag::ExprModuleCall), 2);
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let mut call = |name: &str, argument: Value| {
                evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)),
                        LoweredFunctionKind::Pure,
                        &[argument],
                        Span::new(program.store.source_id, 0, 0),
                    )
                    .expect("the function exists")
            };
            use crate::runtime::value::ResultValue;
            assert_eq!(
                call("width", Value::Str("12".into())).unwrap(),
                Value::Result(ResultValue::Ok(Box::new(Value::Int(12))))
            );
            assert!(matches!(
                call("width", Value::Str("wide".into())).unwrap(),
                Value::Result(ResultValue::Err(_))
            ));
            assert!(matches!(
                call("size", Value::Int(-1)).unwrap(),
                Value::Result(ResultValue::Err(_))
            ));

            // The propagation's operand is the operation: a program whose
            // propagation points nowhere is rejected before it runs.
            let mut dangling = (*program).clone();
            let propagation = dangling
                .store
                .tags
                .iter()
                .position(|tag| *tag == FullTag::ExprTry)
                .unwrap();
            dangling.store.data[propagation].lhs = u32::MAX;
            dangling.store.data[propagation].rhs = u32::MAX;
            assert!(FullVerifier::verify(&dangling).is_err());

            // Without the checker's selection there is nothing to lower.
            let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                "conversion.xsh",
                crate::loader::entry_source_from_text("conversion.xsh", source.to_string()),
                Vec::new(),
            );
            let source_id = crate::source::SourceMap::files(&sources)
                .first()
                .map(crate::source::SourceFile::id)
                .expect("entry source is present");
            let mut declarations = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
            assert_eq!(declarations.bodies.conversions.len(), 3);
            declarations.bodies.conversions.clear();
            assert!(
                FullBuilder::build_compact(
                    &parsed.arena,
                    &declarations,
                    source,
                    Arc::new(sources),
                    source_id,
                )
                .is_err()
            );
        });
    }

    /// A text pattern holds one more literal segment than it has holes, with
    /// text between each two holes, and each hole is a wildcard or a binding
    /// of a known kind. Matching relies on all of it, so a program that
    /// breaks any of it is rejected before it runs.
    #[test]
    fn a_text_pattern_keeps_its_holes_apart_and_their_kinds_known() {
        run_with_large_stack(|| {
            let source = "pure port(line: Str) -> Int {\n  match line {\n    \"none\" => -1,\n    f\"{_}:{port:d}/tcp\" => port,\n    else => 0,\n  }\n}\n";
            let program = Arc::new(fixture("text-pattern.xsh", source));
            FullVerifier::verify(&program).unwrap();
            let row = program
                .store
                .patterns
                .iter()
                .position(|tag| *tag == FullPatternTag::Text)
                .expect("the f-string arm lowers to a text pattern");
            let payload = program.store.pattern_data[row]
                .range()
                .bounds(program.store.extra.len())
                .unwrap();
            // The child list, then a kind for each of two holes, then three
            // segments.
            assert_eq!(payload.len(), 6);
            assert_eq!(
                program.store.extra[payload.start + 1],
                crate::sema::check::TextHoleKind::Text as u32
            );
            assert_eq!(
                program.store.extra[payload.start + 2],
                crate::sema::check::TextHoleKind::Decimal as u32
            );

            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let mut port = |line: &str| {
                evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, "port")),
                        LoweredFunctionKind::Pure,
                        &[Value::Str(line.into())],
                        Span::new(program.store.source_id, 0, 0),
                    )
                    .expect("the function exists")
                    .unwrap()
            };
            assert_eq!(port("ssh:22/tcp"), Value::Int(22));
            assert_eq!(port("a:b:443/tcp"), Value::Int(0));
            assert_eq!(port(":80/tcp"), Value::Int(80));
            assert_eq!(port("ssh:22/udp"), Value::Int(0));
            assert_eq!(port("none"), Value::Int(-1));

            let mut unknown_kind = (*program).clone();
            unknown_kind.store.extra[payload.start + 2] =
                crate::sema::check::TextHoleKind::ALL.len() as u32;
            assert!(
                FullVerifier::verify(&unknown_kind)
                    .unwrap_err()
                    .message
                    .contains("hole kind is out of range")
            );

            // The middle segment becomes the empty first one: two holes with
            // nothing between them.
            let mut adjacent = (*program).clone();
            adjacent.store.extra[payload.start + 4] = adjacent.store.extra[payload.start + 3];
            assert!(
                FullVerifier::verify(&adjacent)
                    .unwrap_err()
                    .message
                    .contains("separated by text")
            );

            // A hole that is a literal is not a wildcard or a binding.
            let literal = program
                .store
                .patterns
                .iter()
                .position(|tag| *tag == FullPatternTag::Literal)
                .expect("the first arm is a literal pattern");
            assert!(literal < row);
            let mut literal_hole = (*program).clone();
            let holes = IrBlockId::from_raw(literal_hole.store.extra[payload.start])
                .expect("the hole list is a block");
            let holes = literal_hole.store.blocks[holes.index()]
                .instructions
                .bounds(literal_hole.store.extra.len())
                .unwrap();
            assert_eq!(literal_hole.store.extra[holes.start], 2);
            literal_hole.store.extra[holes.start + 1] = literal as u32;
            assert!(
                FullVerifier::verify(&literal_hole)
                    .unwrap_err()
                    .message
                    .contains("must be a wildcard or binding")
            );

            // A pattern cut short of its last segment does not decode.
            let mut truncated = (*program).clone();
            truncated.store.pattern_data[row] =
                IrData::from_range(IrRange::new(payload.start as u32, payload.len() as u32 - 1));
            assert!(FullVerifier::verify(&truncated).is_err());
        });
    }

    #[test]
    fn constant_key_projection_preserves_index_and_get_both_routes() {
        run_with_large_stack(|| {
            let source = "type Config = {workers: Int}\nconst field = \"workers\"\npure counts() -> Int {\n  let config: Config = {workers: 4}\n  (config.get(field) ?? 0) + config[field]\n}\n";
            let program = Arc::new(fixture("constant-key-projection.xsh", source));
            assert!(program.store.tags.contains(&FullTag::ExprIndex));
            assert!(program.store.tags.contains(&FullTag::ExprMethod));
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "counts")),
                    LoweredFunctionKind::Pure,
                    &[],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("known field function exists");
            assert_eq!(result.unwrap(), Value::Int(8));
        });
    }

    #[test]
    fn uint_mutation_checks_execute_both_indexed_routes_after_frontend_drop() {
        run_with_large_stack(|| {
            let program = Arc::new(fixture(
                "uint-mutation.xsh",
                include_str!("../../../../tests/fixtures/frontend-indexed/uint-mutation.xsh"),
            ));
            FullVerifier::verify(&program).unwrap();
            let instruction = program
                .store
                .tags
                .iter()
                .position(|tag| *tag == FullTag::ExprCheckedValue)
                .expect("constructors and method operands retain checked values");
            let words = program.store.data[instruction]
                .range()
                .bounds(program.store.extra.len())
                .unwrap();
            let mut malformed = (*program).clone();
            malformed.store.extra[words.start + 1] = u32::MAX;
            assert!(
                FullVerifier::verify(&malformed).is_err(),
                "checked value types must refer to the semantic pool"
            );
            for name in [
                "scalar_failure",
                "compound_failure",
                "record_failure",
                "list_failure",
                "map_failure",
                "append_failure",
                "valid_updates",
                "argument_failure",
                "return_failure",
                "tail_failure",
                "default_failure",
                "list_default_failure",
                "list_return_failure",
                "map_return_failure",
                "record_return_failure",
                "list_argument_failure",
                "map_argument_failure",
                "record_argument_failure",
                "map_default_failure",
                "record_default_failure",
                "result_return_failure",
                "producer_failure",
                "nested_producer_failure",
                "accept",
                "negative_return",
                "defaulted",
                "list_defaulted",
                "map_defaulted",
                "record_defaulted",
                "tag_failure",
                "error_failure",
                "method_list_failure",
                "method_map_failure",
                "method_fallback_failure",
                "method_map_push_failure",
                "inferred_if_failure",
                "inferred_match_failure",
                "builtin_creation_failure",
                "branch_creation_failure",
            ] {
                let mut evaluator =
                    Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let args = if matches!(
                    name,
                    "argument_failure"
                        | "return_failure"
                        | "list_return_failure"
                        | "map_return_failure"
                        | "record_return_failure"
                        | "list_argument_failure"
                        | "map_argument_failure"
                        | "record_argument_failure"
                        | "result_return_failure"
                        | "producer_failure"
                        | "nested_producer_failure"
                        | "accept"
                        | "negative_return"
                        | "tag_failure"
                        | "error_failure"
                        | "method_list_failure"
                        | "method_map_failure"
                        | "method_fallback_failure"
                        | "method_map_push_failure"
                        | "inferred_if_failure"
                        | "inferred_match_failure"
                        | "builtin_creation_failure"
                        | "branch_creation_failure"
                ) {
                    vec![Value::Int(-1)]
                } else {
                    Vec::new()
                };
                let kind = if matches!(name, "producer_failure" | "nested_producer_failure") {
                    LoweredFunctionKind::Proc
                } else {
                    LoweredFunctionKind::Pure
                };
                let result = evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)),
                        kind,
                        &args,
                        Span::new(program.store.source_id, 0, 0),
                    )
                    .expect("UInt function exists");
                if name == "valid_updates" {
                    assert_eq!(result.unwrap(), Value::Int(8));
                } else {
                    let error = result.unwrap_err();
                    assert_eq!(error.kind, "type-error", "{name}");
                    assert!(
                        error.message.contains("UInt") || error.message.contains("UnsignedRow"),
                        "{name}: {}",
                        error.message
                    );
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
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "counts")),
                    LoweredFunctionKind::Pure,
                    &[],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("typed Map function exists");
            assert_eq!(result.unwrap(), Value::Int(13));
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
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            for (name, text, index, expected) in [
                ("nullable", "é", 0, Value::Int(195)),
                ("nullable", "é", 2, Value::Null),
                ("nullable", "é", -1, Value::Null),
                ("sentinel", "é", 0, Value::Int(195)),
                ("sentinel", "é", 2, Value::Int(-1)),
                ("find", ":x", 0, Value::Int(0)),
                ("find", "é:x", 0, Value::Int(2)),
                ("find", "é:x", 3, Value::Null),
            ] {
                let args = [Value::Str(Arc::from(text)), Value::Int(index)];
                let result = evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)),
                        LoweredFunctionKind::Pure,
                        &args,
                        Span::new(program.store.source_id, 0, 0),
                    )
                    .expect("lookup function exists");
                assert_eq!(result.unwrap(), expected, "{name} index {index}");
            }
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "present")),
                    LoweredFunctionKind::Pure,
                    &[],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("present function exists");
            assert_eq!(result.unwrap(), Value::Null);
        });
    }

    #[test]
    fn map_literals_verify_entry_flags_and_execute_both_indexed_routes() {
        run_with_large_stack(|| {
            let source = "pure counts() -> Int {\n  let source: Map[Int] = {beta: 3}\n  let values: Map[Int] = {[\"alpha\"]: 1, alpha: 2, ...source}\n  return (values.get(\"alpha\") ?? 0) + (values.get(\"beta\") ?? 0)\n}\n";
            let program = fixture("computed-map.xsh", source);
            let instruction = program
                .store
                .tags
                .iter()
                .position(|tag| *tag == FullTag::ExprMapLiteral)
                .expect("Map literal instruction");
            let payload = program.store.data[instruction]
                .range()
                .bounds(program.store.extra.len())
                .unwrap();
            let block = IrBlockId::from_raw(program.store.extra[payload.start]).unwrap();
            let entries = program.store.blocks[block.index()]
                .instructions
                .bounds(program.store.extra.len())
                .unwrap();
            let mut bad_flag = program.clone();
            bad_flag.store.extra[entries.start + 1] = 2;
            assert!(FullVerifier::verify(&bad_flag).is_err());
            let mut bad_key = program.clone();
            bad_key.store.extra[entries.start + 2] = u32::MAX;
            assert!(FullVerifier::verify(&bad_key).is_err());
            let program = Arc::new(program);
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "counts")),
                    LoweredFunctionKind::Pure,
                    &[],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("Map function exists");
            assert_eq!(result.unwrap(), Value::Int(5));
        });
    }

    #[test]
    fn list_assignment_verifies_paths_and_executes_both_routes() {
        run_with_large_stack(|| {
            let source =
                include_str!("../../../../tests/fixtures/frontend-indexed/list-assignment.xsh");
            let program = fixture("list-assignment.xsh", source);
            let instruction = program
                .store
                .tags
                .iter()
                .position(|tag| *tag == FullTag::StmtAssignPath)
                .unwrap();
            let payload = program.store.data[instruction]
                .range()
                .bounds(program.store.extra.len())
                .unwrap();
            let block = IrBlockId::from_raw(program.store.extra[payload.start + 1]).unwrap();
            let steps = program.store.blocks[block.index()]
                .instructions
                .bounds(program.store.extra.len())
                .unwrap();
            let mut malformed = program.clone();
            malformed.store.extra[steps.start + 1] = u32::MAX;
            assert!(
                FullVerifier::verify(&malformed)
                    .unwrap_err()
                    .message
                    .contains("assignment path step")
            );
            let mut bad_index = program.clone();
            bad_index.store.extra[steps.start + 2] = u32::MAX;
            assert!(FullVerifier::verify(&bad_index).is_err());
            let mut empty = program.clone();
            empty.store.extra[steps.start] = 0;
            assert!(FullVerifier::verify(&empty).is_err());
            let program = Arc::new(program);
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "updated")),
                    LoweredFunctionKind::Pure,
                    &[],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("assignment function exists");
            assert_eq!(result.unwrap(), Value::Int(16));
        });
    }

    #[test]
    fn list_splicing_verifies_payload_and_executes_both_indexed_routes() {
        run_with_large_stack(|| {
            let source = "pure spliced() -> Int {\n  let middle = [2, 3]\n  let result = [1, @[], @middle, 4]\n  return result[0] + result[3]\n}\n";
            let program = fixture("list-splicing.xsh", source);
            let instruction = program
                .store
                .tags
                .iter()
                .position(|tag| *tag == FullTag::ExprListBuild)
                .expect("mixed list instruction");
            let payload = program.store.data[instruction]
                .range()
                .bounds(program.store.extra.len())
                .unwrap();
            let block = IrBlockId::from_raw(program.store.extra[payload.start]).unwrap();
            let elements = program.store.blocks[block.index()]
                .instructions
                .bounds(program.store.extra.len())
                .unwrap();
            let mut malformed = program.clone();
            malformed.store.extra[elements.start + 1] = 2;
            assert!(
                FullVerifier::verify(&malformed)
                    .unwrap_err()
                    .message
                    .contains("boolean payload")
            );
            let mut bad_child = program.clone();
            bad_child.store.extra[elements.start + 2] = u32::MAX;
            assert!(FullVerifier::verify(&bad_child).is_err());
            let program = Arc::new(program);
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "spliced")),
                    LoweredFunctionKind::Pure,
                    &[],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("splice function exists");
            assert_eq!(result.unwrap(), Value::Int(5));
        });
    }

    #[test]
    fn try_capture_verifies_body_ownership_and_preserves_result_data_on_both_routes() {
        run_with_large_stack(|| {
            let program = fixture(
                "try-capture.xsh",
                "proc capture() [error] -> Result[Int] {\n  let nested = try { Ok(7) }?\n  nested\n}\n",
            );
            let instruction = program
                .store
                .tags
                .iter()
                .position(|tag| *tag == FullTag::ExprCapture)
                .expect("capture instruction");
            let payload = program.store.data[instruction]
                .range()
                .bounds(program.store.extra.len())
                .unwrap();
            let mut missing_body = program.clone();
            missing_body.store.extra[payload.start] = u32::MAX;
            assert!(FullVerifier::verify(&missing_body).is_err());
            let block = IrBlockId::from_raw(program.store.extra[payload.start]).unwrap();
            let mut wrong_kind = program.clone();
            wrong_kind.store.blocks[block.index()].flags = BLOCK_LIST;
            assert!(FullVerifier::verify(&wrong_kind).is_err());
            let program = Arc::new(program);
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "capture")),
                    LoweredFunctionKind::Proc,
                    &[],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("capture function exists");
            assert_eq!(
                result.unwrap(),
                Value::Result(crate::runtime::value::ResultValue::Ok(Box::new(
                    Value::Int(7)
                )))
            );
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
            for (tag, configuration_offset) in [
                (FullStageTag::BatchLimits, 0),
                (FullStageTag::ReduceByConfigured, 3),
                (FullStageTag::TablePrintConfigured, 0),
            ] {
                let stage = program
                    .store
                    .stages
                    .iter()
                    .position(|actual| *actual == tag)
                    .expect("configured stage opcode");
                let payload = program.store.stage_data[stage]
                    .range()
                    .bounds(program.store.extra.len())
                    .unwrap();
                let mut malformed = program.clone();
                malformed.store.extra[payload.start + configuration_offset] = u32::MAX;
                assert!(FullVerifier::verify(&malformed).is_err());
            }
            let program = Arc::new(program);
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "configured")),
                    LoweredFunctionKind::Proc,
                    &[],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("configuration function exists");
            assert_eq!(result.unwrap(), Value::Int(5));
            assert!(
                String::from_utf8(evaluator.stdout.clone())
                    .unwrap()
                    .contains("row")
            );
        });
    }

    #[test]
    fn core_assert_verifier_rejects_invalid_message_presence_and_expression() {
        let source = include_str!("../../../../tests/fixtures/frontend-indexed/assert.xsh");
        let program = fixture("assert.xsh", source);
        let assertion = program
            .store
            .tags
            .iter()
            .position(|tag| *tag == FullTag::StmtAssert)
            .unwrap();
        let payload = program.store.data[assertion]
            .range()
            .bounds(program.store.extra.len())
            .unwrap();
        let mut invalid_presence = program.clone();
        invalid_presence.store.extra[payload.start + 1] = 2;
        assert!(
            FullVerifier::verify(&invalid_presence)
                .unwrap_err()
                .message
                .contains("optional payload")
        );
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
            for (name, detail) in [
                ("passes", None),
                ("fails", Some("1 == 2")),
                ("chain_fails", Some("3 < 2")),
                ("short_circuit_fails", Some("right operand skipped")),
            ] {
                let mut evaluator =
                    Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let result = evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)),
                        LoweredFunctionKind::Pure,
                        &[],
                        Span::new(program.store.source_id, 0, 0),
                    )
                    .expect("assertion function exists");
                if let Some(detail) = detail {
                    // Assertion failures propagate as ordinary Result errors.
                    let Ok(Value::Result(crate::runtime::value::ResultValue::Err(error))) = result
                    else {
                        panic!("false assertion propagates an error: {result:?}");
                    };
                    let Value::Error(error) = *error else {
                        panic!("assertion error payload: {error:?}")
                    };
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
        });
    }

    #[test]
    fn comparison_chain_verifier_rejects_short_chains_and_non_ordering_pairs() {
        let source =
            include_str!("../../../../tests/fixtures/frontend-indexed/comparison-chain.xsh");
        let program = fixture("comparison-chain.xsh", source);
        let chain = program
            .store
            .tags
            .iter()
            .position(|tag| *tag == FullTag::ExprComparisonChain)
            .unwrap();
        let payload = program.store.data[chain]
            .range()
            .bounds(program.store.extra.len())
            .unwrap();
        let block = IrBlockId::from_raw(program.store.extra[payload.start]).unwrap();
        let pairs = program.store.blocks[block.index()]
            .instructions
            .bounds(program.store.extra.len())
            .unwrap();
        let mut short = program.clone();
        short.store.extra[pairs.start] = 1;
        assert!(
            FullVerifier::verify(&short)
                .unwrap_err()
                .message
                .contains("at least two pairs")
        );
        let first_pair = program.store.extra[pairs.start + 1] as usize;
        let pair_payload = program.store.data[first_pair]
            .range()
            .bounds(program.store.extra.len())
            .unwrap();
        let mut mixed = program;
        let op = mixed.store.extra[pair_payload.start] as usize;
        mixed.store.binary_ops[op] = BinaryOp::Eq;
        assert!(
            FullVerifier::verify(&mixed)
                .unwrap_err()
                .message
                .contains("ordering operators")
        );
    }

    #[test]
    fn fs_list_verifier_requires_the_filesystem_children_operation() {
        let program = fixture("fs-list.xsh", "let children = fs.children(p\".\")?\n");
        let instruction = program
            .store
            .tags
            .iter()
            .position(|tag| *tag == FullTag::ExprFsList)
            .expect("fs.children lowers to an indexed filesystem-list expression");
        let payload = program.store.data[instruction]
            .range()
            .bounds(program.store.extra.len())
            .expect("filesystem-list expression payload is in bounds");
        let mut malformed = program;
        let op = u32::try_from(malformed.store.runtime_ops.len()).expect("operation index fits");
        malformed.store.runtime_ops.push(RuntimeOp::FsMkdir);
        malformed.store.extra[payload.start] = op;

        assert!(FullVerifier::verify(&malformed)
            .unwrap_err()
            .message
            .contains("filesystem list operation is not `fs.children`"));
    }

    #[test]
    fn record_proof_precise_types_and_unreachable_fallback_survive_frontend_drop() {
        run_with_large_stack(|| {
            let source =
                include_str!("../../../../tests/fixtures/frontend-indexed/proof-provenance.xsh");
            let program = Arc::new(fixture("proof-provenance.xsh", source));
            FullVerifier::verify(&program).unwrap();
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "verified")),
                    LoweredFunctionKind::Pure,
                    &[],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("proof function exists");
            assert_eq!(result.unwrap(), Value::Str("ready".into()));
        });
    }

    #[test]
    fn duration_arithmetic_retains_checked_operands_after_frontend_drop_on_both_routes() {
        run_with_large_stack(|| {
            let source =
                include_str!("../../../../tests/fixtures/frontend-indexed/duration-arithmetic.xsh");
            let program = Arc::new(fixture("duration-arithmetic.xsh", source));
            FullVerifier::verify(&program).unwrap();
            let duration =
                |millis| Value::Duration(crate::runtime::value::DurationValue { millis });
            for (name, args, expected) in [
                (
                    "intervals",
                    vec![duration(7000), duration(2000)],
                    Value::Int(3),
                ),
                ("pause", vec![duration(250), Value::Int(3)], duration(751)),
            ] {
                let mut evaluator =
                    Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let result = evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)),
                        LoweredFunctionKind::Pure,
                        &args,
                        Span::new(program.store.source_id, 0, 0),
                    )
                    .expect("Duration function exists");
                assert_eq!(result.unwrap(), expected);
            }
            for (name, code, expression) in [
                ("underflow", "duration-underflow", "0ms - 1ms"),
                (
                    "overflow",
                    "duration-overflow",
                    "18446744073709551615ms + 1ms",
                ),
                (
                    "count_overflow",
                    "integer-overflow",
                    "18446744073709551615ms / 1ms",
                ),
            ] {
                let mut evaluator =
                    Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let result = evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)),
                        LoweredFunctionKind::Pure,
                        &[],
                        Span::new(program.store.source_id, 0, 0),
                    )
                    .expect("Duration function exists");
                let error = result.expect_err("checked arithmetic failure");
                assert_eq!(error.kind, code);
                assert_eq!(&source[error.span.unwrap().range()], expression);
            }
        });
    }

    #[test]
    fn wire_enum_preparation_survives_frontend_drop_and_executes_both_routes() {
        run_with_large_stack(|| {
            let source = include_str!("../../../../tests/fixtures/frontend-indexed/wire-enums.xsh");
            let program = Arc::new(fixture("wire-enums.xsh", source));
            assert_eq!(
                program.store.wire_enums.len(),
                1,
                "constructors share one declaring mapping"
            );
            FullVerifier::verify(&program).unwrap();
            let mut malformed = (*program).clone();
            Arc::make_mut(&mut malformed.store.wire_enums[0])
                .variants
                .insert(program_name(&program, "Duplicate"), Arc::from("ready"));
            assert!(
                FullVerifier::verify(&malformed)
                    .unwrap_err()
                    .message
                    .contains("duplicate strings")
            );
            let mut missing = (*program).clone();
            missing.store.wire_enums.clear();
            assert!(FullVerifier::verify(&missing).is_err());
            let mut mismatched = (*program).clone();
            mismatched.store.prepared_schemas[0] = Arc::new(
                super::super::super::require::PreparedSchema::Validate(Type::Int),
            );
            assert!(
                FullVerifier::verify(&mismatched)
                    .unwrap_err()
                    .message
                    .contains("does not match")
            );
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
                    PreparedSchema::Record(fields) => fields
                        .iter_mut()
                        .any(|(_, schema)| change_mapping(Arc::make_mut(schema))),
                    PreparedSchema::List(schema)
                    | PreparedSchema::Map(_, schema)
                    | PreparedSchema::Optional(schema)
                    | PreparedSchema::Validated(_, schema) => change_mapping(Arc::make_mut(schema)),
                    PreparedSchema::Union(_, members) => members
                        .iter_mut()
                        .any(|schema| change_mapping(Arc::make_mut(schema))),
                    PreparedSchema::Validate(_) | PreparedSchema::Set(_) => false,
                }
            }
            let mut contradictory = (*program).clone();
            assert!(change_mapping(Arc::make_mut(
                &mut contradictory.store.prepared_schemas[0]
            )));
            assert!(
                FullVerifier::verify(&contradictory).is_err(),
                "schema and constructor mappings must agree"
            );
            let mut duplicate = (*program).clone();
            duplicate
                .store
                .wire_enums
                .push(duplicate.store.wire_enums[0].clone());
            assert!(
                FullVerifier::verify(&duplicate)
                    .unwrap_err()
                    .message
                    .contains("multiple mappings")
            );
            let mut changed_constant = (*program).clone();
            let tag = changed_constant
                .store
                .prepared_constants
                .iter_mut()
                .find_map(|value| match &mut value.0 {
                    LoweredValue::Tag(tag) if tag.wire.is_some() => Some(tag),
                    _ => None,
                })
                .expect("prepared enum constant");
            *Arc::make_mut(tag.wire.as_mut().unwrap())
                .variants
                .values_mut()
                .next()
                .unwrap() = Arc::from("forged");
            assert!(
                FullVerifier::verify(&changed_constant)
                    .unwrap_err()
                    .message
                    .contains("contradictory")
            );
            let raw = "{\"state\":\"ready\",\"values\":[\"\"],\"optional\":null}";
            for (name, arguments, expected) in [
                ("wire_direct", Vec::new(), "\"ready\""),
                ("wire_prepared", Vec::new(), "\"ready\""),
                ("wire_typed_map", Vec::new(), "[\"ready\",\"\"]"),
                (
                    "wire_round_trip",
                    vec![Value::Str(Arc::from(raw))],
                    "{\"optional\":null,\"state\":\"ready\",\"values\":[\"\"]}",
                ),
                (
                    "wire_nested",
                    vec![Value::Str(Arc::from(format!("{{\"packet\":{raw}}}")))],
                    "{\"packet\":{\"optional\":null,\"state\":\"ready\",\"values\":[\"\"]}}",
                ),
            ] {
                let mut evaluator =
                    Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let result = evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)),
                        LoweredFunctionKind::Pure,
                        &arguments,
                        Span::new(program.store.source_id, 0, 0),
                    )
                    .expect("wire enum function exists");
                assert_eq!(result.unwrap(), Value::ok(Value::Str(Arc::from(expected))));
            }
        });
    }

    // A slot declared as a union holds one member where a method is called on
    // it, and the checker says which: the receiver's published type. Lowering
    // reads that fact and nothing else, so a receiver whose fact is missing,
    // is still the union, or names a member without the method is refused
    // instead of being lowered on the strength of some other member.
    #[test]
    fn narrowed_union_receivers_are_lowered_from_the_checked_member() {
        use crate::sema::check::CompactDeclOutput;
        use crate::syntax::arena::{ArenaExprKind, ExprId};
        run_with_large_stack(|| {
            let source = r#"type Word = Union[Str, Path]

pure label(word: Word) -> Str {
  if word is Path {
    return word.name()
  }

  word.trim()
}
"#;
            let program = Arc::new(fixture("narrowed-union.xsh", source));
            FullVerifier::verify(&program).unwrap();
            for (argument, expected) in [
                (Value::Str(Arc::from("  padded ")), "padded"),
                (
                    Value::Path(PathValue::new(b"/srv/data.txt".to_vec()).unwrap()),
                    "data.txt",
                ),
            ] {
                let mut evaluator =
                    Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let result = evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, "label")),
                        LoweredFunctionKind::Pure,
                        &[argument],
                        Span::new(program.store.source_id, 0, 0),
                    )
                    .expect("label exists");
                assert_eq!(result.unwrap(), Value::Str(Arc::from(expected)));
            }

            // Builds the same source after `edit` has changed the checked
            // type of the `word` receiver of `.name()`.
            let build = |edit: &dyn Fn(&mut CompactDeclOutput, ExprId)| {
                let name = "narrowed-union.xsh";
                let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                    name,
                    crate::loader::entry_source_from_text(name, source.to_string()),
                    Vec::new(),
                );
                let source_id = crate::source::SourceMap::files(&sources)
                    .first()
                    .map(crate::source::SourceFile::id)
                    .expect("entry source is present");
                let mut declarations = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
                assert!(declarations.diagnostics.is_empty());
                let receivers = declarations
                    .bodies
                    .expr_types
                    .iter()
                    .filter(|(id, ty)| {
                        **ty == Type::Path
                            && matches!(
                                parsed.arena.arena.expr(**id).kind,
                                ArenaExprKind::Ident(name) if name == "word"
                            )
                    })
                    .map(|(id, _)| *id)
                    .collect::<Vec<_>>();
                assert_eq!(receivers.len(), 1, "the narrowed receiver of `.name()`");
                edit(&mut declarations, receivers[0]);
                FullBuilder::build_compact(
                    &parsed.arena,
                    &declarations,
                    source,
                    Arc::new(sources),
                    source_id,
                )
            };
            assert!(build(&|_, _| {}).is_ok());

            let word = Type::Union(vec![Type::Str, Type::Path]);
            // The fact names the member that has no `name` method.
            assert!(
                build(&|declarations, receiver| {
                    declarations.bodies.expr_types.insert(receiver, Type::Str);
                })
                .is_err()
            );
            // The fact is the declared union: nothing was narrowed.
            assert!(
                build(&|declarations, receiver| {
                    declarations
                        .bodies
                        .expr_types
                        .insert(receiver, word.clone());
                })
                .is_err()
            );
            // The fact is missing.
            assert!(
                build(&|declarations, receiver| {
                    declarations.bodies.expr_types.remove(&receiver);
                })
                .is_err()
            );
        });
    }

    // A loop item has no declared slot type, so the checker's fact is the
    // only thing that says which union member a receiver is. Without the
    // fact the method call is refused, as for a declared union slot.
    #[test]
    fn union_loop_item_receiver_without_a_fact_is_not_lowered() {
        use crate::syntax::arena::ArenaExprKind;
        run_with_large_stack(|| {
            let source = r#"type Word = Union[Str, Path]

pure names(words: List[Word]) -> Str {
  var out = ""
  for word in words {
    if word is Path {
      out = out + word.name()
    }
  }

  out
}
"#;
            let build = |keep_fact: bool| {
                let name = "union-loop-item.xsh";
                let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                    name,
                    crate::loader::entry_source_from_text(name, source.to_string()),
                    Vec::new(),
                );
                let source_id = crate::source::SourceMap::files(&sources)
                    .first()
                    .map(crate::source::SourceFile::id)
                    .expect("entry source is present");
                let mut declarations = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
                assert!(declarations.diagnostics.is_empty());
                let receivers = declarations
                    .bodies
                    .expr_types
                    .iter()
                    .filter(|(id, ty)| {
                        **ty == Type::Path
                            && matches!(
                                parsed.arena.arena.expr(**id).kind,
                                ArenaExprKind::Ident(name) if name == "word"
                            )
                    })
                    .map(|(id, _)| *id)
                    .collect::<Vec<_>>();
                assert_eq!(receivers.len(), 1, "the narrowed receiver of `.name()`");
                if !keep_fact {
                    declarations.bodies.expr_types.remove(&receivers[0]);
                }
                FullBuilder::build_compact(
                    &parsed.arena,
                    &declarations,
                    source,
                    Arc::new(sources),
                    source_id,
                )
            };
            assert!(build(true).is_ok());
            assert!(build(false).is_err());
        });
    }

    // A call through a callable type reaches the verified program as one row
    // that names the checked type, its kind, and one argument per parameter.
    // The executor binds those arguments by position and reads no signature,
    // so each way the row can disagree with the type is rejected before it
    // runs; a row that agrees with a type the value does not have stops at
    // the call.
    #[test]
    fn typed_callable_calls_reject_corruption_in_a_lowered_program() {
        run_with_large_stack(|| {
            let source = r#"type Scale = pure(n: Int) -> Int
type Shift = proc(n: Int) [error] -> Result[Int]

pure double(n: Int) -> Int {
  n * 2
}

pure triple(n: Int) -> Int {
  n * 3
}

proc add_one(n: Int) [error] -> Result[Int] {
  Ok(n + 1)
}

proc add_two(n: Int) [error] -> Result[Int] {
  Ok(n + 2)
}

pure scaled(wide: Bool, n: Int) -> Int {
  let scale: Scale = if wide { triple } else { double }
  scale(n)
}

proc shifted(far: Bool, n: Int) [error] -> Result[Int] {
  let shift: Shift = if far { add_two } else { add_one }
  shift(n: n)
}
"#;
            let program = Arc::new(fixture("typed-callables.xsh", source));
            FullVerifier::verify(&program).unwrap();

            let call = |program: &Arc<FullProgram>, name: &str, kind, flag: bool| {
                let mut evaluator =
                    Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(program));
                evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(program, name)),
                        kind,
                        &[Value::Bool(flag), Value::Int(5)],
                        Span::new(program.store.source_id, 0, 0),
                    )
                    .expect("function exists")
            };
            for (flag, expected) in [(true, 15), (false, 10)] {
                assert_eq!(
                    call(&program, "scaled", LoweredFunctionKind::Pure, flag).unwrap(),
                    Value::Int(expected)
                );
            }
            for (flag, expected) in [(true, 7), (false, 6)] {
                assert_eq!(
                    call(&program, "shifted", LoweredFunctionKind::Proc, flag).unwrap(),
                    Value::ok(Value::Int(expected))
                );
            }

            // Payload words: callee, kind, signature type, argument block.
            let typed_call = |pure: u32| {
                program
                    .store
                    .tags
                    .iter()
                    .enumerate()
                    .filter(|(_, tag)| **tag == FullTag::ExprTypedCall)
                    .map(|(index, _)| {
                        program.store.data[index]
                            .range()
                            .bounds(program.store.extra.len())
                            .unwrap()
                            .start
                    })
                    .find(|start| program.store.extra[start + 1] == pure)
                    .expect("both kinds of typed call are lowered")
            };
            let pure_call = typed_call(1);
            let proc_call = typed_call(0);
            let rejected = |corrupt: &FullProgram, reason: &str| {
                let error = FullVerifier::verify(corrupt).unwrap_err();
                assert!(
                    error.message.contains(reason),
                    "expected `{reason}`, got `{}`",
                    error.message
                );
            };

            // The row claims a kind its type does not have.
            let mut wrong_kind = (*program).clone();
            wrong_kind.store.extra[proc_call + 1] = 1;
            rejected(
                &wrong_kind,
                "typed call kind does not match its callable type",
            );

            // The row names `Int` instead of the callee slot's checked callable
            // type. Exact slot correlation rejects it before callable-shape validation.
            let semantic = &program.store.semantic;
            let scale = TypeId::from_raw(program.store.extra[pure_call + 2]).unwrap();
            let int = semantic
                .signature_return_type(semantic.typed_callable_signature(scale).unwrap())
                .unwrap();
            let mut untyped = (*program).clone();
            untyped.store.extra[pure_call + 2] = int.raw();
            rejected(&untyped, "typed call signature differs from its callee slot's callable type");

            let arguments = |program: &FullProgram| {
                let block = IrBlockId::from_raw(program.store.extra[pure_call + 3]).unwrap();
                program.store.blocks[block.index()]
                    .instructions
                    .bounds(program.store.extra.len())
                    .unwrap()
                    .start
            };
            // The list declares an extra argument, which disagrees with the
            // checked callable signature.
            let mut extra_argument = (*program).clone();
            let count = arguments(&extra_argument);
            extra_argument.store.extra[count] += 1;
            rejected(
                &extra_argument,
                "typed call argument count does not match its signature",
            );

            // Pointing the argument at an instruction that does not produce
            // one value is rejected before ownership validation.
            let mut unowned_argument = (*program).clone();
            let first = arguments(&unowned_argument) + 1;
            unowned_argument.store.extra[first] = 2;
            rejected(
                &unowned_argument,
                "typed call argument is not a single value",
            );

            // The type's signature lets a call omit a parameter.
            let mut defaulted = (*program).clone();
            assert!(
                defaulted
                    .store
                    .semantic
                    .default_first_typed_callable_param_for_test()
            );
            rejected(
                &defaulted,
                "typed callable signature has a defaulted or rest parameter",
            );

            // A row made consistent with a proc type of the same arity still
            // sits in a pure function, which makes no call to a proc.
            let mut proc_in_pure = (*program).clone();
            proc_in_pure.store.extra[pure_call + 1] = 0;
            proc_in_pure.store.extra[pure_call + 2] = program.store.extra[proc_call + 2];
            rejected(&proc_in_pure, "typed call to a proc inside a pure function");

            // `shifted` reads the local declared as `Shift` (a proc
            // signature). Relabeling its call row to the pure `Scale`
            // signature is rejected before execution.
            let mut other_kind = (*program).clone();
            other_kind.store.extra[proc_call + 1] = 1;
            other_kind.store.extra[proc_call + 2] = program.store.extra[pure_call + 2];
            let error = FullVerifier::verify(&other_kind).unwrap_err();
            assert!(
                error
                    .message
                    .contains("typed call signature differs from its callee slot's callable type"),
                "{}",
                error.message
            );
        });
    }

    #[test]
    fn typed_callable_field_facts_reject_field_and_receiver_corruption() {
        let source = r#"type Scale = pure(n: Int) -> Int
type Holder = {scale: Scale, count: Int}
type OtherHolder = {scale: Int}

pure apply_field(holder: Holder, other: OtherHolder, holder_again: Holder, n: Int) -> Int {
  let retained = other
  let retained_holder = holder_again
  holder.scale(n)
}
"#;
        let program = fixture("typed-callable-field-facts.xsh", source);
        FullVerifier::verify(&program).unwrap();
        fn payload(program: &FullProgram, instruction: usize) -> &[u32] {
            program
                .store
                .payload(program.store.data[instruction].range())
                .unwrap()
        }
        let field_callee = |program: &FullProgram| {
            program
                .store
                .tags
                .iter()
                .enumerate()
                .filter(|(_, tag)| **tag == FullTag::ExprTypedCall)
                .find_map(|(instruction, _)| {
                    let callee = *payload(program, instruction).first()? as usize;
                    (program.store.tags.get(callee) == Some(&FullTag::ExprField)).then_some(callee)
                })
                .expect("record-field typed call exists")
        };
        let reject = |program: &FullProgram| {
            let error = FullVerifier::verify(program).unwrap_err();
            assert!(
                error
                    .message
                    .contains("typed call signature differs from its callee field's callable type"),
                "{}",
                error.message
            );
        };

        let mut wrong_field = program.clone();
        let field = field_callee(&wrong_field);
        assert_eq!(wrong_field.store.string(payload(&wrong_field, field)[1]).unwrap(), "scale");
        let count_name = u32::try_from(wrong_field.store.strings.len() + 1).unwrap();
        let start = u32::try_from(wrong_field.store.string_bytes.len()).unwrap();
        wrong_field.store.string_bytes.extend_from_slice(b"count");
        wrong_field
            .store
            .strings
            .push(IrRange::new(start, 5));
        wrong_field.store.extra[wrong_field.store.data[field].range().start as usize + 1] =
            count_name;
        assert_eq!(wrong_field.store.string(payload(&wrong_field, field)[1]).unwrap(), "count");
        reject(&wrong_field);

        let mut wrong_receiver = program.clone();
        let field = field_callee(&wrong_receiver);
        let receiver = payload(&wrong_receiver, field)[0] as usize;
        assert_eq!(wrong_receiver.store.tags[receiver], FullTag::ExprParam);
        wrong_receiver.store.extra
            [wrong_receiver.store.data[receiver].range().start as usize] = 1;
        reject(&wrong_receiver);

        let other_receiver = program
            .store
            .function_instruction_range(0)
            .unwrap()
            .find(|instruction| {
                program.store.tags[*instruction] == FullTag::ExprParam
                    && payload(&program, *instruction).first() == Some(&2)
            })
            .expect("the other record parameter is already referenced");
        let mut shared_receiver = program.clone();
        let field = field_callee(&shared_receiver);
        shared_receiver.store.extra
            [shared_receiver.store.data[field].range().start as usize] =
            u32::try_from(other_receiver).unwrap();
        let error = FullVerifier::verify(&shared_receiver).unwrap_err();
        assert!(error.message.contains("multiple parents"), "{}", error.message);
    }

    #[test]
    fn typed_callable_record_return_facts_reject_field_and_receiver_corruption() {
        let source = r#"type Scale = pure(n: Int) -> Int
type Holder = {callback: Scale, count: Int}
type OtherHolder = {callback: Int}

pure double(n: Int) -> Int {
  n * 2
}

pure get_holder() -> Holder {
  Holder(callback: double, count: 0)
}

pure get_other_holder() -> OtherHolder {
  OtherHolder(callback: 0)
}

pure apply_result(n: Int) -> Int {
  get_holder().callback(n)
}
"#;
        let program = fixture("typed-callable-record-result.xsh", source);
        FullVerifier::verify(&program).unwrap();
        fn payload(program: &FullProgram, instruction: usize) -> &[u32] {
            program
                .store
                .payload(program.store.data[instruction].range())
                .unwrap()
        }
        let field_callee = program
            .store
            .tags
            .iter()
            .enumerate()
            .filter(|(_, tag)| **tag == FullTag::ExprTypedCall)
            .find_map(|(instruction, _)| {
                let callee = *payload(&program, instruction).first()? as usize;
                (program.store.tags.get(callee) == Some(&FullTag::ExprField))
                    .then_some(callee)
            })
            .expect("record-return field typed call exists");
        assert_eq!(
            program.store.string(payload(&program, field_callee)[1]).unwrap(),
            "callback"
        );
        let reject = |program: &FullProgram| {
            let error = FullVerifier::verify(program).unwrap_err();
            assert!(
                error
                    .message
                    .contains("typed call signature differs from its callee field's callable type"),
                "{}",
                error.message
            );
        };

        let mut wrong_field = program.clone();
        let count_name = u32::try_from(wrong_field.store.strings.len() + 1).unwrap();
        let start = u32::try_from(wrong_field.store.string_bytes.len()).unwrap();
        wrong_field.store.string_bytes.extend_from_slice(b"count");
        wrong_field.store.strings.push(IrRange::new(start, 5));
        wrong_field.store.extra[wrong_field.store.data[field_callee].range().start as usize + 1] =
            count_name;
        reject(&wrong_field);

        let receiver = payload(&program, field_callee)[0] as usize;
        assert!(matches!(
            program.store.tags[receiver],
            FullTag::ExprCall | FullTag::ExprDirectPureCall
        ));
        let alternate = program
            .store
            .functions
            .iter()
            .position(|function| program.store.string(function.name).unwrap() == "get_other_holder")
            .expect("alternate record producer exists");
        let mut wrong_receiver = program.clone();
        wrong_receiver.store.extra
            [wrong_receiver.store.data[receiver].range().start as usize] =
            IrFunctionId::new(alternate).unwrap().raw();
        reject(&wrong_receiver);
    }

    #[test]
    fn typed_callable_call_result_facts_reject_a_different_producer_target() {
        let source = r#"type Scale = pure(n: Int) -> Int
type Label = pure(text: Str) -> Int

pure double(n: Int) -> Int {
  n * 2
}

pure label_length(text: Str) -> Int {
  text.byte_len()
}

pure provide_scale() -> Scale {
  double
}

pure provide_label() -> Label {
  label_length
}

pure apply_result(n: Int) -> Int {
  provide_scale()(n)
}
"#;
        let program = fixture("typed-callable-call-result-facts.xsh", source);
        FullVerifier::verify(&program).unwrap();
        let producing_call = program
            .store
            .tags
            .iter()
            .enumerate()
            .filter(|(_, tag)| **tag == FullTag::ExprTypedCall)
            .find_map(|(instruction, _)| {
                let words = program
                    .store
                    .payload(program.store.data[instruction].range())
                    .ok()?;
                let callee = *words.first()? as usize;
                matches!(
                    program.store.tags.get(callee),
                    Some(FullTag::ExprCall | FullTag::ExprDirectPureCall)
                )
                .then_some(callee)
            })
            .expect("call-result typed call exists");
        let alternate = program
            .store
            .functions
            .iter()
            .position(|function| program.store.string(function.name).unwrap() == "provide_label")
            .expect("alternate callable producer exists");
        let mut wrong_result = program.clone();
        wrong_result.store.extra
            [wrong_result.store.data[producing_call].range().start as usize] =
            IrFunctionId::new(alternate).unwrap().raw();
        let error = FullVerifier::verify(&wrong_result).unwrap_err();
        assert!(
            error
                .message
                .contains("typed call signature differs from its callee call result's callable type"),
            "{}",
            error.message
        );
    }

    #[test]
    fn typed_callable_call_result_rejects_a_forged_checked_return_type() {
        let source = r#"type Scale = pure(n: Int) -> Int

pure double(n: Int) -> Int { n * 2 }
pure provide_scale() -> Scale { double }
pure provide_int() -> Int { 7 }

pure apply_result(n: Int) -> Int {
  provide_scale()(n)
}
"#;
        let program = fixture("typed-callable-forged-return-type.xsh", source);
        FullVerifier::verify(&program).unwrap();
        fn payload(program: &FullProgram, instruction: usize) -> &[u32] {
            program
                .store
                .payload(program.store.data[instruction].range())
                .unwrap()
        }
        let (typed_call, producer_call) = program
            .store
            .tags
            .iter()
            .enumerate()
            .filter(|(_, tag)| **tag == FullTag::ExprTypedCall)
            .find_map(|(instruction, _)| {
                let callee = *payload(&program, instruction).first()? as usize;
                matches!(
                    program.store.tags.get(callee),
                    Some(FullTag::ExprCall | FullTag::ExprDirectPureCall)
                )
                .then_some((instruction, callee))
            })
            .expect("call-result typed call exists");
        let signature = TypeId::from_raw(payload(&program, typed_call)[2]).unwrap();
        let alternate = program
            .store
            .functions
            .iter()
            .position(|function| program.store.string(function.name).unwrap() == "provide_int")
            .expect("integer producer exists");

        let mut forged = program.clone();
        forged.store.extra[forged.store.data[producer_call].range().start as usize] =
            IrFunctionId::new(alternate).unwrap().raw();
        forged.store.functions[alternate].checked_return_type = signature.raw();

        let error = FullVerifier::verify(&forged).unwrap_err();
        assert!(
            error.message.contains("checked return type disagrees with the function signature"),
            "{}",
            error.message
        );
    }

    #[test]
    fn with_fallible_path_results_match_their_registered_types() {
        let source = r#"test test_with_fallible_path_results_match_their_registered_types {
  let source_path = fp"source"
  with raw_content = source_path.read_bytes()? {
    let _ = raw_content
  } else { |failure| let _ = failure.message }
  with present = source_path.exists()? {
    let _ = present
  } else { |failure| let _ = failure.message }
  with executable = source_path.executable()? {
    let _ = executable
  } else { |failure| let _ = failure.message }
  with size = source_path.du()? {
    let _ = size
  } else { |failure| let _ = failure.message }
  with entry = source_path.metadata()? {
    let _ = entry
  } else { |failure| let _ = failure.message }
  with resolved_path = source_path.resolve()? {
    let _ = resolved_path.display()
  } else { |failure| let _ = failure.message }
  with target_path = source_path.readlink()? {
    let _ = target_path.display()
  } else { |failure| let _ = failure.message }
  with written = source_path.write("contents")? {
    let _ = written
  } else { |failure| let _ = failure.message }
  with atomically_written = source_path.write_atomic("contents")? {
    let _ = atomically_written
  } else { |failure| let _ = failure.message }
  with created = source_path.mkdir()? {
    let _ = created
  } else { |failure| let _ = failure.message }
  with removed = source_path.remove()? {
    let _ = removed
  } else { |failure| let _ = failure.message }
  with children = fs.children(source_path)? {
    let _ = children
  } else { |failure| let _ = failure.message }
  with files = fs.files(source_path)? {
    let _ = files
  } else { |failure| let _ = failure.message }
  with walked = fs.walk(source_path)? {
    let _ = walked
  } else { |failure| let _ = failure.message }
  with root = fs.tempdir()? {
    let _ = root
  } else { |failure| let _ = failure.message }
  with tar_result = archive.tar_create(source_path, source_path, [source_path])? {
    let _ = tar_result
  } else { |failure| let _ = failure.message }
  with contents = source_path.read_text()? {
    let _ = contents
  } else { |failure| let _ = failure.message }
}
"#;
        let program = fixture("typed-callable-path-result-producer.xsh", source);
        FullVerifier::verify(&program).unwrap();

        let read_text_fact = program
            .store
            .checked_expr_types
            .iter()
            .find(|fact| {
                program.store.tags[fact.instruction as usize] == FullTag::ExprPathReadText
            })
            .expect("read_text error source has a checked result type");
        let alternate_type = read_text_fact.type_id;
        let resolve_type = program
            .store
            .checked_expr_types
            .iter()
            .find(|fact| {
                program.store.tags[fact.instruction as usize] == FullTag::ExprPathResolve
            })
            .expect("resolve error source has a checked result type")
            .type_id;

        for (tag, forged_type, expected_message) in [
            (
                FullTag::ExprPathReadText,
                resolve_type,
                "checked path read result disagrees with its registry signature",
            ),
            (
                FullTag::ExprPathReadBytes,
                alternate_type,
                "checked path byte read disagrees with its registry signature",
            ),
            (
                FullTag::ExprPathExists,
                alternate_type,
                "checked path existence result disagrees with its registry signature",
            ),
            (
                FullTag::ExprPathExecutable,
                alternate_type,
                "checked path executable result disagrees with its registry signature",
            ),
            (
                FullTag::ExprPathDu,
                alternate_type,
                "checked path disk usage disagrees with its registry signature",
            ),
            (
                FullTag::ExprPathMetadata,
                alternate_type,
                "checked path metadata disagrees with its registry signature",
            ),
            (
                FullTag::ExprPathResolve,
                alternate_type,
                "checked path resolve result disagrees with its registry signature",
            ),
            (
                FullTag::ExprPathReadlink,
                alternate_type,
                "checked path readlink result disagrees with its registry signature",
            ),
            (
                FullTag::ExprPathWrite,
                alternate_type,
                "checked path write disagrees with its registry signature",
            ),
            (
                FullTag::ExprPathMkdir,
                alternate_type,
                "checked path directory creation disagrees with its registry signature",
            ),
            (
                FullTag::ExprPathRemove,
                alternate_type,
                "checked path removal disagrees with its registry signature",
            ),
            (
                FullTag::ExprFsList,
                alternate_type,
                "checked filesystem listing disagrees with its registry signature",
            ),
            (
                FullTag::ExprFsFiles,
                alternate_type,
                "checked filesystem file scan disagrees with its registry signature",
            ),
            (
                FullTag::ExprFsWalk,
                alternate_type,
                "checked filesystem walk disagrees with its registry signature",
            ),
            (
                FullTag::ExprFsTempDir,
                alternate_type,
                "checked temporary-directory result disagrees with its registry signature",
            ),
            (
                FullTag::ExprArchiveTarCreate,
                alternate_type,
                "checked archive creation disagrees with its registry signature",
            ),
        ] {
            let fact_indices = program
                .store
                .checked_expr_types
                .iter()
                .enumerate()
                .filter_map(|(index, fact)| {
                    (program.store.tags[fact.instruction as usize] == tag).then_some(index)
                })
                .collect::<Vec<_>>();
            assert!(!fact_indices.is_empty(), "{tag:?} has no checked source fact");
            for fact_index in fact_indices {
                let mut forged = program.clone();
                forged.store.checked_expr_types[fact_index].type_id = forged_type;

                let error = FullVerifier::verify(&forged).unwrap_err();
                assert!(error.message.contains(expected_message), "{}", error.message);
            }
        }

        for (tag, expected_message) in [
            (
                FullTag::ExprFsFiles,
                "checked filesystem file scan disagrees with its registry signature",
            ),
            (
                FullTag::ExprFsWalk,
                "checked filesystem walk disagrees with its registry signature",
            ),
        ] {
            let instruction = program
                .store
                .tags
                .iter()
                .position(|actual| *actual == tag)
                .expect("wrapped filesystem scan producer");
            let payload = program.store.data[instruction].range();
            let wrapped_flag = payload.start as usize + 5;
            assert_eq!(program.store.extra[wrapped_flag], 1);
            let mut forged = program.clone();
            forged.store.extra[wrapped_flag] = 0;

            let error = FullVerifier::verify(&forged).unwrap_err();
            assert!(error.message.contains(expected_message), "{}", error.message);
        }

        let fs_list = program
            .store
            .tags
            .iter()
            .position(|actual| *actual == FullTag::ExprFsList)
            .expect("filesystem child-list producer");
        let op_index = program.store.extra[program.store.data[fs_list].range().start as usize]
            as usize;
        let mut forged = program.clone();
        forged.store.runtime_ops[op_index] = RuntimeOp::FsDirs;
        let error = FullVerifier::verify(&forged).unwrap_err();
        assert!(
            error.message.contains("filesystem list operation is not `fs.children`"),
            "{}",
            error.message
        );
    }

    #[test]
    fn typed_callable_function_ref_fact_matches_its_target_signature() {
        let source = r#"type Scale = pure(n: Int) -> Int
type Label = pure(text: Str) -> Int

pure double(n: Int) -> Int { n * 2 }
pure label_length(text: Str) -> Int { text.byte_len() }

pure apply() -> Int {
  let callback: Scale = double
  let label: Label = label_length
  callback(7) + label("seven")
}
"#;
        let program = fixture("typed-callable-function-ref-signature.xsh", source);
        FullVerifier::verify(&program).unwrap();
        let _symbols = program.symbol_owner().enter();
        let function = program
            .store
            .functions
            .iter()
            .position(|function| program.store.string(function.name).unwrap() == "apply")
            .expect("apply function exists");
        fn fact_for(program: &FullProgram, name: &str) -> (usize, FullCheckedExprType) {
            program
                .store
                .checked_expr_types
                .iter()
                .enumerate()
                .find(|(_, fact)| {
                    fact.function_ref.is_some_and(|(function, _)| {
                        function.as_name() == Some(Name::intern(name))
                    })
                })
                .map(|(index, fact)| (index, *fact))
                .expect("function reference checked type exists")
        }
        let (double_fact_index, double_fact) = fact_for(&program, "double");
        let (_, label_fact) = fact_for(&program, "label_length");
        let double_instruction = double_fact.instruction as usize;
        let callback_slot = program
            .store
            .function_instruction_range(function)
            .unwrap()
            .find_map(|instruction| {
                if program.store.tags[instruction] != FullTag::StmtLet {
                    return None;
                }
                let words = program.store.payload(program.store.data[instruction].range()).ok()?;
                (words.get(1).copied() == Some(double_instruction as u32))
                    .then(|| words[0] as usize)
            })
            .expect("callable initializer slot exists");
        let typed_call = program
            .store
            .function_instruction_range(function)
            .unwrap()
            .find(|instruction| program.store.tags[*instruction] == FullTag::ExprTypedCall)
            .expect("typed call exists");
        let mut forged = program.clone();
        let signature = label_fact.type_id.raw();
        forged.store.checked_expr_types[double_fact_index].type_id = label_fact.type_id;
        let slot_facts = forged.store.functions[function]
            .slot_type_facts
            .bounds(forged.store.slot_type_facts.len())
            .unwrap();
        forged.store.slot_type_facts[slot_facts.start + callback_slot] = signature;
        let call_signature = forged.store.data[typed_call].range().start as usize + 2;
        forged.store.extra[call_signature] = signature;

        let error = FullVerifier::verify(&forged).unwrap_err();
        assert!(
            error.message.contains("checked function reference identity disagrees with its producer"),
            "{}",
            error.message
        );
    }

    #[test]
    fn scoped_callable_alias_checks_its_function_reference_signature() {
        let source = r#"type Scale = pure(n: Int) -> Int
type Label = pure(text: Str) -> Int

pure double(n: Int) -> Int { n * 2 }
pure label_length(text: Str) -> Int { text.byte_len() }
pure provide_label() -> Label { label_length }

test test_scoped_callable_alias {
  let scoped: Scale = within 5s {
    let result: Scale = double
    result
  }?
  let other: Label = label_length
  assert scoped(2) == 4
  assert other("two") == 3
}
"#;
        let program = fixture("typed-callable-scoped-alias.xsh", source);
        FullVerifier::verify(&program).unwrap();
        let _symbols = program.symbol_owner().enter();
        let scoped_test = program
            .store
            .functions
            .iter()
            .position(|function| {
                program.store.string(function.name).unwrap() == "test_scoped_callable_alias"
            })
            .expect("scoped test function exists");
        let alias_initializer = program
            .store
            .function_instruction_range(scoped_test)
            .unwrap()
            .find_map(|instruction| {
                if program.store.tags[instruction] != FullTag::StmtLet {
                    return None;
                }
                let words = program
                    .store
                    .payload(program.store.data[instruction].range())
                    .ok()?;
                let value = *words.get(1)? as usize;
                (program.store.tags.get(value) == Some(&FullTag::ExprFunctionRef))
                    .then_some(value)
            })
            .expect("scoped local has a function reference initializer");
        let label_reference = program
            .store
            .checked_expr_types
            .iter()
            .find(|fact| {
                fact.function_ref.is_some_and(|(function, _)| {
                    function.as_name() == Some(Name::intern("label_length"))
                })
            })
            .map(|fact| fact.instruction as usize)
            .expect("alternate function reference exists");
        let target_range = program.store.data[alias_initializer].range();
        let alternate_range = program.store.data[label_reference].range();
        assert_eq!(target_range.len, alternate_range.len);

        let mut forged = program.clone();
        let target_start = target_range.start as usize;
        let alternate_start = alternate_range.start as usize;
        let payload_len = target_range.len as usize;
        forged.store.extra[target_start..target_start + payload_len]
            .copy_from_slice(&program.store.extra[alternate_start..alternate_start + payload_len]);
        let error = FullVerifier::verify(&forged).unwrap_err();
        assert!(
            error
                .message
                .contains("checked function reference identity disagrees with its producer"),
            "{}",
            error.message
        );
    }

    #[test]
    fn typed_callable_local_fact_cannot_retype_an_integer_slot() {
        let source = r#"type Scale = pure(n: Int) -> Int

pure double(n: Int) -> Int { n * 2 }

pure apply() -> Int {
  let callback: Scale = double
  let number = 7
  callback(number)
}
"#;
        let program = fixture("typed-callable-forged-local-type.xsh", source);
        FullVerifier::verify(&program).unwrap();
        let function = program
            .store
            .functions
            .iter()
            .position(|function| program.store.string(function.name).unwrap() == "apply")
            .expect("apply function exists");
        fn payload(program: &FullProgram, instruction: usize) -> &[u32] {
            program
                .store
                .payload(program.store.data[instruction].range())
                .unwrap()
        }
        let typed_call = program
            .store
            .function_instruction_range(function)
            .unwrap()
            .find(|instruction| program.store.tags[*instruction] == FullTag::ExprTypedCall)
            .expect("typed call exists");
        let callee = *payload(&program, typed_call).first().unwrap() as usize;
        assert_eq!(program.store.tags[callee], FullTag::ExprParam);
        let int_slot = program
            .store
            .function_instruction_range(function)
            .unwrap()
            .find(|instruction| program.store.tags[*instruction] == FullTag::StmtLetInt)
            .map(|instruction| payload(&program, instruction)[0] as usize)
            .expect("integer local uses the integer let instruction");
        let fact_range = program.store.functions[function]
            .slot_type_facts
            .bounds(program.store.slot_type_facts.len())
            .unwrap();
        let signature = payload(&program, typed_call)[2];

        let mut forged = program.clone();
        forged.store.extra[forged.store.data[callee].range().start as usize] =
            u32::try_from(int_slot).unwrap();
        forged.store.slot_type_facts[fact_range.start + int_slot] = signature;

        let error = FullVerifier::verify(&forged).unwrap_err();
        assert!(
            error
                .message
                .contains("local slot type fact disagrees with its initializer IR representation"),
            "{}",
            error.message
        );
    }

    #[test]
    fn typed_callable_slot_fact_cannot_retype_an_integer_initializer() {
        let source = r#"type Scale = pure(n: Int) -> Int

pure double(n: Int) -> Int { n * 2 }

pure apply() -> Int {
  let callback: Scale = double
  let number = 7
  callback(number)
}
"#;
        let program = fixture("typed-callable-forged-slot-fact.xsh", source);
        FullVerifier::verify(&program).unwrap();
        let function = program
            .store
            .functions
            .iter()
            .position(|function| program.store.string(function.name).unwrap() == "apply")
            .expect("apply function exists");
        let int_slot = program
            .store
            .function_instruction_range(function)
            .unwrap()
            .find_map(|instruction| {
                (program.store.tags[instruction] == FullTag::StmtLetInt)
                    .then(|| program.store.payload(program.store.data[instruction].range()).unwrap()[0] as usize)
            })
            .expect("integer local uses the integer let instruction");
        let fact_range = program.store.functions[function]
            .slot_type_facts
            .bounds(program.store.slot_type_facts.len())
            .unwrap();
        let typed_call = program
            .store
            .function_instruction_range(function)
            .unwrap()
            .find(|instruction| program.store.tags[*instruction] == FullTag::ExprTypedCall)
            .expect("typed call exists");
        let signature = program
            .store
            .payload(program.store.data[typed_call].range())
            .unwrap()[2];

        let mut forged = program.clone();
        forged.store.slot_type_facts[fact_range.start + int_slot] = signature;

        let error = FullVerifier::verify(&forged).unwrap_err();
        assert!(
            error
                .message
                .contains("local slot type fact disagrees with its initializer IR representation"),
            "{}",
            error.message
        );
    }

    #[test]
    fn typed_callable_loop_slot_fact_matches_its_iteration_item_type() {
        let source = r#"type Count = pure(text: Str) -> Int
type Measure = pure(size: Int) -> Int

pure byte_count(text: Str) -> Int { text.byte_len() }
pure measure(size: Int) -> Int { size }

pure loop_call(counts: List[Count], text: Str) -> Int {
  var total = 0
  for each in counts {
    total = each(text)
  }
  total
}

pure measured(callback: Measure, size: Int) -> Int {
  callback(size)
}
"#;
        let program = fixture("typed-callable-forged-loop-slot-type.xsh", source);
        FullVerifier::verify(&program).unwrap();
        let function = program
            .store
            .functions
            .iter()
            .position(|function| program.store.string(function.name).unwrap() == "loop_call")
            .expect("loop function exists");
        let measured = program
            .store
            .functions
            .iter()
            .position(|function| program.store.string(function.name).unwrap() == "measured")
            .expect("second callable signature function exists");
        fn payload(program: &FullProgram, instruction: usize) -> &[u32] {
            program
                .store
                .payload(program.store.data[instruction].range())
                .unwrap()
        }
        let range = program.store.function_instruction_range(function).unwrap();
        let loop_slot = range
            .clone()
            .find_map(|instruction| {
                (program.store.tags[instruction] == FullTag::StmtFor)
                    .then(|| payload(&program, instruction)[0] as usize)
            })
            .expect("typed loop binding exists");
        let loop_call = range
            .clone()
            .find_map(|instruction| {
                if program.store.tags[instruction] != FullTag::ExprTypedCall {
                    return None;
                }
                let callee = payload(&program, instruction)[0] as usize;
                (program.store.tags.get(callee) == Some(&FullTag::ExprParam)
                    && payload(&program, callee).first().copied() == Some(loop_slot as u32))
                .then_some((instruction, callee))
            })
            .expect("loop binding is called through its local slot");
        let alternate_signature = program
            .store
            .function_instruction_range(measured)
            .unwrap()
            .find(|instruction| program.store.tags[*instruction] == FullTag::ExprTypedCall)
            .map(|instruction| payload(&program, instruction)[2])
            .expect("distinct typed call signature exists");
        let callee_fact = program
            .store
            .checked_expr_types
            .iter()
            .position(|fact| fact.instruction as usize == loop_call.1)
            .expect("loop callee has an exact checker fact");
        let fact_range = program.store.functions[function]
            .slot_type_facts
            .bounds(program.store.slot_type_facts.len())
            .unwrap();

        let mut forged = program.clone();
        forged.store.slot_type_facts[fact_range.start + loop_slot] = alternate_signature;
        let alternate_signature = TypeId::from_raw(alternate_signature).unwrap();
        forged.store.checked_expr_types[callee_fact].type_id = alternate_signature;
        let call_signature = forged.store.data[loop_call.0].range().start as usize + 2;
        forged.store.extra[call_signature] = alternate_signature.raw();

        let error = FullVerifier::verify(&forged).unwrap_err();
        assert!(
            error.message.contains("local callable slot type fact disagrees with its writer producer"),
            "{}",
            error.message
        );
    }

    #[test]
    fn typed_callable_guard_error_binding_matches_its_result_payload() {
        let source = r#"type Scale = pure(n: Int) -> Int
type Measure = pure(value: Int) -> Int

pure double(n: Int) -> Int {
  n * 2
}

pure measure(value: Int) -> Int {
  value
}

pure invoke_measure(callback: Measure, value: Int) -> Int {
  callback(value)
}

pure use_failure(result: Result[Int, Scale]) -> Int {
  guard let value = result else { |failure| return failure(3) }
  value
}
"#;
        let program = fixture("typed-callable-guard-error.xsh", source);
        FullVerifier::verify(&program).unwrap();

        let use_failure = program
            .store
            .functions
            .iter()
            .position(|function| program.store.string(function.name).unwrap() == "use_failure")
            .expect("guard function exists");
        let invoke_measure = program
            .store
            .functions
            .iter()
            .position(|function| {
                program.store.string(function.name).unwrap() == "invoke_measure"
            })
            .expect("alternate callable function exists");
        fn payload(program: &FullProgram, instruction: usize) -> &[u32] {
            program
                .store
                .payload(program.store.data[instruction].range())
                .unwrap()
        }
        let range = program.store.function_instruction_range(use_failure).unwrap();
        let guard = range
            .clone()
            .find(|instruction| program.store.tags[*instruction] == FullTag::StmtGuard)
            .expect("guard row exists");
        let execution = schema_execution_with_owner(
            &program.store,
            IrFunctionId::new(use_failure).unwrap().raw(),
            program.store.functions[use_failure].slot_count,
        );
        let (tag, payload_view) = execution
            .instruction_id(u32::try_from(guard).unwrap())
            .expect("guard instruction exists");
        let schema::StmtView::Guard {
            else_param_slot: Some(failure_slot),
            ..
        } = schema::StmtView::decode(&execution, tag, payload_view).unwrap()
        else {
            panic!("guard binds its Result error parameter");
        };
        let call = range
            .clone()
            .find_map(|instruction| {
                if program.store.tags[instruction] != FullTag::ExprTypedCall {
                    return None;
                }
                let callee = payload(&program, instruction)[0] as usize;
                (program.store.tags.get(callee) == Some(&FullTag::ExprParam)
                    && payload(&program, callee).first().copied()
                        == Some(failure_slot as u32))
                .then_some((instruction, callee))
            })
            .expect("the checked failure value is called");
        let alternate_signature = program
            .store
            .function_instruction_range(invoke_measure)
            .unwrap()
            .find(|instruction| program.store.tags[*instruction] == FullTag::ExprTypedCall)
            .map(|instruction| payload(&program, instruction)[2])
            .expect("alternate signature call exists");
        let callee_fact = program
            .store
            .checked_expr_types
            .iter()
            .position(|fact| fact.instruction as usize == call.1)
            .expect("failure parameter has an exact checker fact");
        let slot_facts = program.store.functions[use_failure]
            .slot_type_facts
            .bounds(program.store.slot_type_facts.len())
            .unwrap();

        let mut forged = program.clone();
        forged.store.slot_type_facts[slot_facts.start + failure_slot] = alternate_signature;
        let alternate_signature = TypeId::from_raw(alternate_signature).unwrap();
        forged.store.checked_expr_types[callee_fact].type_id = alternate_signature;
        let call_signature = forged.store.data[call.0].range().start as usize + 2;
        forged.store.extra[call_signature] = alternate_signature.raw();

        let error = FullVerifier::verify(&forged).unwrap_err();
        assert!(
            error
                .message
                .contains("local callable slot type fact disagrees with its writer producer"),
            "{}",
            error.message
        );
    }

    #[test]
    fn typed_callable_with_error_binding_matches_its_result_payload() {
        let source = r#"type Scale = pure(n: Int) -> Int
type Measure = pure(value: Int) -> Int

pure double(n: Int) -> Int { n * 2 }
pure measure(value: Int) -> Int { value }

pure use_failure(
  result: Result[Int, Scale],
  second: Result[Int, Scale],
) -> Result[Int, Scale] {
  var value = 0
  with direct = result, left = result?, right = second? {
    value = direct + left + right
  } else { |failure| return Ok(failure(4)) }
  Ok(value)
}

pure other(callback: Measure) -> Int { callback(7) }
"#;
        let program = fixture("typed-callable-with-error.xsh", source);
        FullVerifier::verify(&program).unwrap();

        fn payload(program: &FullProgram, instruction: usize) -> &[u32] {
            program
                .store
                .payload(program.store.data[instruction].range())
                .unwrap()
        }
        let use_failure = program
            .store
            .functions
            .iter()
            .position(|function| program.store.string(function.name).unwrap() == "use_failure")
            .expect("With function exists");
        let other = program
            .store
            .functions
            .iter()
            .position(|function| program.store.string(function.name).unwrap() == "other")
            .expect("alternate callable function exists");
        let range = program
            .store
            .function_instruction_range(use_failure)
            .unwrap();
        let with = range
            .clone()
            .find(|instruction| program.store.tags[*instruction] == FullTag::StmtWith)
            .expect("With row exists");
        let execution = schema_execution_with_owner(
            &program.store,
            IrFunctionId::new(use_failure).unwrap().raw(),
            program.store.functions[use_failure].slot_count,
        );
        let (tag, payload_view) = execution
            .instruction_id(u32::try_from(with).unwrap())
            .expect("With instruction exists");
        let schema::StmtView::With {
            else_param_slot: Some(failure_slot),
            ..
        } = schema::StmtView::decode(&execution, tag, payload_view).unwrap()
        else {
            panic!("With binds its failure parameter");
        };
        let call = range
            .clone()
            .find_map(|instruction| {
                if program.store.tags[instruction] != FullTag::ExprTypedCall {
                    return None;
                }
                let callee = payload(&program, instruction)[0] as usize;
                (program.store.tags.get(callee) == Some(&FullTag::ExprParam)
                    && payload(&program, callee).first().copied()
                        == Some(failure_slot as u32))
                .then_some((instruction, callee))
            })
            .expect("the With failure value is called");
        let alternate_signature = program
            .store
            .function_instruction_range(other)
            .unwrap()
            .find(|instruction| program.store.tags[*instruction] == FullTag::ExprTypedCall)
            .map(|instruction| payload(&program, instruction)[2])
            .expect("alternate signature call exists");
        let callee_fact = program
            .store
            .checked_expr_types
            .iter()
            .position(|fact| fact.instruction as usize == call.1)
            .expect("With failure parameter has an exact checker fact");
        let slot_facts = program.store.functions[use_failure]
            .slot_type_facts
            .bounds(program.store.slot_type_facts.len())
            .unwrap();

        let mut forged = program.clone();
        forged.store.slot_type_facts[slot_facts.start + failure_slot] = alternate_signature;
        let alternate_signature = TypeId::from_raw(alternate_signature).unwrap();
        forged.store.checked_expr_types[callee_fact].type_id = alternate_signature;
        let call_signature = forged.store.data[call.0].range().start as usize + 2;
        forged.store.extra[call_signature] = alternate_signature.raw();

        let error = FullVerifier::verify(&forged).unwrap_err();
        assert!(
            error
                .message
                .contains("local callable slot type fact disagrees with its writer producer"),
            "{}",
            error.message
        );

    }

    #[test]
    fn typed_callable_with_binding_cannot_be_retyped_from_an_any_initializer() {
        let source = r#"type Scale = pure(n: Int) -> Int

pure double(n: Int) -> Int { n * 2 }

pure use_callback(result: Result[Scale, Scale], dynamic: Result[Any, Scale]) -> Int {
  with callback = result, opaque = dynamic {
    return callback(4)
  } else { |failure| return failure(4) }
}
"#;
        let program = fixture("typed-callable-with-any-with-binding.xsh", source);
        FullVerifier::verify(&program).unwrap();

        let owner = program
            .store
            .functions
            .iter()
            .position(|function| program.store.string(function.name).unwrap() == "use_callback")
            .expect("With function exists");
        let mut range = program.store.function_instruction_range(owner).unwrap();
        let with = range
            .find(|instruction| program.store.tags[*instruction] == FullTag::StmtWith)
            .expect("With row exists");
        let with_payload = program.store.data[with]
            .range()
            .bounds(program.store.extra.len())
            .unwrap();
        let bindings_block = IrBlockId::from_raw(program.store.extra[with_payload.start])
            .expect("With bindings block exists");
        let bindings = program.store.blocks[bindings_block.index()]
            .instructions
            .bounds(program.store.extra.len())
            .unwrap();
        assert_eq!(program.store.extra[bindings.start], 2);
        let callback_initializer = program.store.extra[bindings.start + 2] as usize;
        let opaque_initializer = program.store.extra[bindings.start + 4] as usize;
        assert_eq!(program.store.tags[callback_initializer], FullTag::ExprParam);
        assert_eq!(program.store.tags[opaque_initializer], FullTag::ExprParam);
        assert_ne!(callback_initializer, opaque_initializer);

        let mut forged = program.clone();
        forged.store.extra[bindings.start + 2] = opaque_initializer as u32;
        forged.store.extra[bindings.start + 4] = callback_initializer as u32;

        let error = FullVerifier::verify(&forged).unwrap_err();
        assert!(
            error.message.contains("callable") || error.message.contains("checked type"),
            "{}",
            error.message
        );
    }

    #[test]
    fn typed_callable_with_error_slot_cannot_alias_a_different_success_type() {
        let source = r#"type Count = pure(text: Str) -> Int
type Measure = pure(value: Int) -> Int

pure count(text: Str) -> Int { text.byte_len() }
pure measure(value: Int) -> Int { value }

pure use_failure(result: Result[Count, Measure]) -> Int {
  var value = 0
  with candidate = result {
    value = candidate("success")
  } else { |failure| value = failure(4) }
  value
}
"#;
        let program = fixture("typed-callable-with-error-slot-alias.xsh", source);
        FullVerifier::verify(&program).unwrap();

        fn payload(program: &FullProgram, instruction: usize) -> &[u32] {
            program
                .store
                .payload(program.store.data[instruction].range())
                .unwrap()
        }
        fn call_for_slot(
            program: &FullProgram,
            range: std::ops::Range<usize>,
            slot: usize,
        ) -> (usize, usize) {
            range
                .into_iter()
                .find_map(|instruction| {
                    if program.store.tags[instruction] != FullTag::ExprTypedCall {
                        return None;
                    }
                    let callee = payload(program, instruction)[0] as usize;
                    (program.store.tags.get(callee) == Some(&FullTag::ExprParam)
                        && payload(program, callee).first().copied() == Some(slot as u32))
                    .then_some((instruction, callee))
                })
                .expect("typed call through the selected With binding exists")
        }

        let owner = program
            .store
            .functions
            .iter()
            .position(|function| program.store.string(function.name).unwrap() == "use_failure")
            .expect("With function exists");
        let range = program.store.function_instruction_range(owner).unwrap();
        let with = range
            .clone()
            .find(|instruction| program.store.tags[*instruction] == FullTag::StmtWith)
            .expect("With row exists");
        let execution = schema_execution_with_owner(
            &program.store,
            IrFunctionId::new(owner).unwrap().raw(),
            program.store.functions[owner].slot_count,
        );
        let (tag, payload_view) = execution
            .instruction_id(u32::try_from(with).unwrap())
            .expect("With instruction exists");
        let schema::StmtView::With {
            mut bindings,
            else_param_slot: Some(failure_slot),
            ..
        } = schema::StmtView::decode(&execution, tag, payload_view).unwrap()
        else {
            panic!("With binds its failure parameter");
        };
        let (success_slot, _) = bindings
            .next()
            .expect("With success binding exists")
            .unwrap();
        assert_ne!(success_slot, failure_slot);
        let (success_call, _) = call_for_slot(&program, range.clone(), success_slot);
        let (failure_call, failure_callee) = call_for_slot(&program, range.clone(), failure_slot);
        let checked_fact = program
            .store
            .checked_expr_types
            .iter()
            .position(|fact| fact.instruction as usize == failure_callee)
            .expect("failure parameter has an exact checker fact");
        let success_signature = payload(&program, success_call)[2];

        let mut forged = program.clone();
        let with_payload = forged.store.data[with]
            .range()
            .bounds(forged.store.extra.len())
            .unwrap();
        assert_eq!(forged.store.extra[with_payload.start + 2], 1);
        forged.store.extra[with_payload.start + 3] = success_slot as u32;
        let callee_payload = forged.store.data[failure_callee]
            .range()
            .bounds(forged.store.extra.len())
            .unwrap();
        forged.store.extra[callee_payload.start] = success_slot as u32;
        let signature = TypeId::from_raw(success_signature).unwrap();
        forged.store.checked_expr_types[checked_fact].type_id = signature;
        let call_signature = forged.store.data[failure_call].range().start as usize + 2;
        forged.store.extra[call_signature] = success_signature;

        let error = FullVerifier::verify(&forged).unwrap_err();
        assert!(
            error
                .message
                .contains("With success and error bindings share a source slot"),
            "{}",
            error.message
        );
    }

    #[test]
    fn typed_callable_pattern_slot_facts_match_nominal_variant_fields() {
        let source = r#"type Count = pure(text: Str) -> Int
type Measure = pure(value: Str) -> Int
enum Choice { Selected(Count), Empty }
error Failure = Selected(handler: Count)

pure count(text: Str) -> Int { text.byte_len() }
pure measure(value: Str) -> Int { value.byte_len() }

pure tag_apply(choice: Choice) -> Int {
  match choice {
    Selected(each) => each("tag"),
    Empty => 0,
  }
}

pure error_apply(failure: Error) -> Int {
  match failure {
    Failure.Selected {handler} => handler("error"),
    _ => 0,
  }
}

pure other(callback: Measure) -> Int { callback("alternate") }

pure construct_failure() -> Result[Unit, Failure] {
  return Err(Failure.Selected(handler: count))
}
"#;
        let program = fixture("typed-callable-pattern-slot-facts.xsh", source);
        FullVerifier::verify(&program).unwrap();
        fn payload(program: &FullProgram, instruction: usize) -> &[u32] {
            program
                .store
                .payload(program.store.data[instruction].range())
                .unwrap()
        }
        let other = program
            .store
            .functions
            .iter()
            .position(|function| program.store.string(function.name).unwrap() == "other")
            .expect("alternate signature function exists");
        let alternate_signature = program
            .store
            .function_instruction_range(other)
            .unwrap()
            .find(|instruction| program.store.tags[*instruction] == FullTag::ExprTypedCall)
            .map(|instruction| payload(&program, instruction)[2])
            .expect("alternate typed call signature exists");

        for name in ["tag_apply", "error_apply"] {
            let function = program
                .store
                .functions
                .iter()
                .position(|function| program.store.string(function.name).unwrap() == name)
                .expect("pattern function exists");
            let (call, callee) = program
                .store
                .function_instruction_range(function)
                .unwrap()
                .find_map(|instruction| {
                    if program.store.tags[instruction] != FullTag::ExprTypedCall {
                        return None;
                    }
                    let callee = payload(&program, instruction)[0] as usize;
                    (program.store.tags.get(callee) == Some(&FullTag::ExprParam))
                        .then_some((instruction, callee))
                })
                .expect("pattern-bound callable is called");
            let slot = payload(&program, callee)[0] as usize;
            let fact = program
                .store
                .checked_expr_types
                .iter()
                .position(|fact| fact.instruction as usize == callee)
                .expect("pattern-bound callable has an exact checker fact");
            let fact_range = program.store.functions[function]
                .slot_type_facts
                .bounds(program.store.slot_type_facts.len())
                .unwrap();

            let mut forged = program.clone();
            forged.store.slot_type_facts[fact_range.start + slot] = alternate_signature;
            let alternate_signature = TypeId::from_raw(alternate_signature).unwrap();
            forged.store.checked_expr_types[fact].type_id = alternate_signature;
            let call_signature = forged.store.data[call].range().start as usize + 2;
            forged.store.extra[call_signature] = alternate_signature.raw();

            let error = FullVerifier::verify(&forged).unwrap_err();
            assert!(
                error.message.contains("local callable slot type fact disagrees with its writer producer"),
                "{name}: {}",
                error.message
            );
        }

        let construct_failure = program
            .store
            .functions
            .iter()
            .position(|function| {
                program.store.string(function.name).unwrap() == "construct_failure"
            })
            .expect("structured error constructor function exists");
        let error_instruction = program
            .store
            .function_instruction_range(construct_failure)
            .unwrap()
            .find(|instruction| program.store.tags[*instruction] == FullTag::ExprError)
            .expect("structured error constructor is encoded");
        let error_payload = program.store.data[error_instruction]
            .range()
            .bounds(program.store.extra.len())
            .unwrap();
        assert_eq!(program.store.extra[error_payload.start], 1);
        assert_eq!(
            program
                .store
                .string(program.store.extra[error_payload.start + 1])
                .unwrap(),
            "Failure"
        );
        let error_fact = program
            .store
            .checked_expr_types
            .binary_search_by_key(
                &u32::try_from(error_instruction).unwrap(),
                |fact| fact.instruction,
            )
            .expect("structured error constructor carries its checked family identity");

        fn append_string(store: &mut FullStore, value: &str) -> u32 {
            let start = u32::try_from(store.string_bytes.len()).unwrap();
            let length = u32::try_from(value.len()).unwrap();
            store.string_bytes.extend_from_slice(value.as_bytes());
            let id = u32::try_from(store.strings.len()).unwrap();
            store.strings.push(IrRange::new(start, length));
            id
        }

        let mut wrong_identity = program.clone();
        let wrong_identity_id = append_string(&mut wrong_identity.store, "missing.Failure");
        wrong_identity.store.extra[error_payload.start + 2] = wrong_identity_id;
        let error = FullVerifier::verify(&wrong_identity).unwrap_err();
        assert!(
            error
                .message
                .contains("checked structured-error identity disagrees with its constructor"),
            "{}",
            error.message
        );

        let mut wrong_display = program.clone();
        let wrong_display_id = append_string(&mut wrong_display.store, "OtherFailure");
        wrong_display.store.extra[error_payload.start + 1] = wrong_display_id;
        let error = FullVerifier::verify(&wrong_display).unwrap_err();
        assert!(
            error.message.contains(
                "structured-error display name disagrees with its declaration identity"
            ),
            "{}",
            error.message
        );

        let mut missing_identity = program.clone();
        missing_identity.store.checked_expr_types.remove(error_fact);
        let error = FullVerifier::verify(&missing_identity).unwrap_err();
        assert!(
            error
                .message
                .contains("structured-error constructor has no checked declaration identity"),
            "{}",
            error.message
        );

        let tag_key = program
            .store
            .checked_pattern_variants
            .iter()
            .find(|(key, schema)| {
                key.kind == CheckedPatternVariantKind::Tag && !schema.fields.is_empty()
            })
            .map(|(key, _)| key)
            .copied()
            .expect("tag pattern has a declaration schema with callable fields");
        let mut forged = program.clone();
        forged.store.checked_pattern_variants.get_mut(&tag_key).unwrap().fields[0].type_id =
            TypeId::from_raw(alternate_signature).unwrap();
        let error = FullVerifier::verify(&forged).unwrap_err();
        assert!(
            error.message.contains(
                "checked nominal pattern schema disagrees with its canonical declaration"
            ),
            "{}",
            error.message
        );
    }

    #[test]
    fn verifier_rejects_typed_callable_type_patterns() {
        let callable = Type::Callable(Arc::new(crate::sema::types::TypedCallable {
            pure: true,
            sig: CallableType {
                params: Vec::new(),
                return_ty: Box::new(Type::Unit),
                effects: None,
            },
        }));
        let mut builder = FullBuilder::new(SourceId::new(0));
        builder.current_owner = Some(0);
        builder.current_slot_count = 1;
        let mut roots = Vec::new();
        BuildPatternRow::Type {
            ty: callable,
            slot: Some(0),
        }
        .encode(&mut builder, &mut roots)
        .unwrap();

        let execution = schema_execution_with_slots(&builder.store, 1);
        let error = execution
            .decoder()
            .verify_pattern_writer_types(roots[0], Some(&Type::Any))
            .unwrap_err();
        assert!(
            error
                .message
                .contains("type pattern cannot establish a typed callable"),
            "{}",
            error.message
        );
    }

    #[test]
    fn callable_field_assignments_match_the_record_field_producer_type() {
        let source = r#"type Scale = pure(n: Int) -> Int
type Label = pure(text: Str) -> Str
type Holder = {callback: Scale}
type Bundle = {nested: Holder}

pure double(n: Int) -> Int { n * 2 }
pure text_copy(text: Str) -> Str { text }

pure apply_field() -> Int {
  var holder: Holder = {callback: double}
  let other: Label = text_copy
  holder.callback = double
  assert other("x") == "x"
  holder.callback(2)
}

pure apply_path() -> Int {
  var bundle: Bundle = {nested: {callback: double}}
  let other: Label = text_copy
  bundle.nested.callback = double
  assert other("x") == "x"
  bundle.nested.callback(2)
}

pure apply_update() -> Int {
  let bundle: Bundle = {nested: {callback: double}}
  let other: Label = text_copy
  let updated = {...bundle, nested.callback: double}
  assert other("x") == "x"
  updated.nested.callback(2)
}
"#;
        let program = fixture("typed-callable-field-assignment.xsh", source);
        FullVerifier::verify(&program).unwrap();
        let _symbols = program.symbol_owner().enter();

        fn forged_assignment(
            program: &FullProgram,
            function_name: &str,
            expected_tag: FullTag,
        ) -> FullProgram {
            let function = program
                .store
                .functions
                .iter()
                .position(|function| program.store.string(function.name).unwrap() == function_name)
                .expect("assignment function exists");
            let range = program.store.function_instruction_range(function).unwrap();
            let execution = FullFunctionView { program, index: function }
                .execution()
                .unwrap();
            let statement = range
                .clone()
                .find(|instruction| program.store.tags[*instruction] == expected_tag)
                .expect("callable field assignment exists");
            let (tag, payload) = execution.instruction_id(statement as u32).unwrap();
            let value = match schema::StmtView::decode(&execution, tag, payload).unwrap() {
                schema::StmtView::AssignField { value, .. }
                | schema::StmtView::AssignPath { value, .. } => value.raw() as usize,
                _ => panic!("expected a field or path assignment"),
            };
            let (value_fact_index, _) = program
                .store
                .checked_expr_types
                .iter()
                .enumerate()
                .find(|(_, fact)| fact.instruction as usize == value)
                .expect("assignment value has a checked callable fact");
            let (alternate_instruction, alternate_fact) = program
                .store
                .checked_expr_types
                .iter()
                .find(|fact| {
                    range.contains(&(fact.instruction as usize))
                        && fact.function_ref.is_some_and(|(function, _)| {
                            function.as_name() == Some(Name::intern("text_copy"))
                        })
                })
                .map(|fact| (fact.instruction as usize, *fact))
                .expect("alternate callable initializer exists in this function");

            let target_range = program.store.data[value].range();
            let alternate_range = program.store.data[alternate_instruction].range();
            assert_eq!(target_range.len, alternate_range.len);
            let target_words = target_range.bounds(program.store.extra.len()).unwrap();
            let alternate_words = alternate_range
                .bounds(program.store.extra.len())
                .unwrap();

            let mut forged = program.clone();
            forged.store.extra[target_words].copy_from_slice(
                &program.store.extra[alternate_words],
            );
            forged.store.checked_expr_types[value_fact_index] = alternate_fact;
            forged
        }

        for (function, tag) in [
            ("apply_field", FullTag::StmtAssignField),
            ("apply_path", FullTag::StmtAssignPath),
        ] {
            let forged = forged_assignment(&program, function, tag);
            let error = FullVerifier::verify(&forged).unwrap_err();
            assert!(
                error.message.contains(
                    "assignment value disagrees with its checked callable target"
                ),
                "{function}: {}",
                error.message
            );
        }

        let function = program
            .store
            .functions
            .iter()
            .position(|function| program.store.string(function.name).unwrap() == "apply_update")
            .expect("record update function exists");
        let range = program.store.function_instruction_range(function).unwrap();
        let execution = FullFunctionView {
            program: &program,
            index: function,
        }
        .execution()
        .unwrap();
        let update = range
            .clone()
            .find(|instruction| program.store.tags[*instruction] == FullTag::ExprRecordUpdate)
            .expect("callable record update expression exists");
        let (tag, payload) = execution.instruction_id(update as u32).unwrap();
        let value = match schema::ExprView::decode(&execution, tag, payload).unwrap() {
            schema::ExprView::RecordUpdate { mut updates, .. } => updates
                .next()
                .expect("one callable field update")
                .unwrap()
                .1
                .raw() as usize,
            _ => panic!("expected a record update expression"),
        };
        let (value_fact_index, _) = program
            .store
            .checked_expr_types
            .iter()
            .enumerate()
            .find(|(_, fact)| fact.instruction as usize == value)
            .expect("record update value has a checked callable fact");
        let (alternate_instruction, alternate_fact) = program
            .store
            .checked_expr_types
            .iter()
            .find(|fact| {
                range.contains(&(fact.instruction as usize))
                    && fact.function_ref.is_some_and(|(function, _)| {
                        function.as_name() == Some(Name::intern("text_copy"))
                    })
            })
            .map(|fact| (fact.instruction as usize, *fact))
            .expect("alternate callable initializer exists in record update function");
        let target_range = program.store.data[value].range();
        let alternate_range = program.store.data[alternate_instruction].range();
        assert_eq!(target_range.len, alternate_range.len);
        let target_words = target_range.bounds(program.store.extra.len()).unwrap();
        let alternate_words = alternate_range.bounds(program.store.extra.len()).unwrap();
        let mut forged = program.clone();
        forged.store.extra[target_words]
            .copy_from_slice(&program.store.extra[alternate_words]);
        forged.store.checked_expr_types[value_fact_index] = alternate_fact;
        let error = FullVerifier::verify(&forged).unwrap_err();
        assert!(
            error.message.contains(
                "record update value disagrees with its checked callable field"
            ),
            "{}",
            error.message
        );
    }

    #[test]
    fn typed_callable_local_fact_cannot_retype_a_string_initializer() {
        let source = r#"type Scale = pure(n: Int) -> Int

pure double(n: Int) -> Int { n * 2 }

pure apply() -> Int {
  let callback: Scale = double
  let text = "ordinary"
  callback(7)
}
"#;
        let program = fixture("typed-callable-forged-string-local-type.xsh", source);
        FullVerifier::verify(&program).unwrap();
        let function = program
            .store
            .functions
            .iter()
            .position(|function| program.store.string(function.name).unwrap() == "apply")
            .expect("apply function exists");
        fn payload(program: &FullProgram, instruction: usize) -> &[u32] {
            program
                .store
                .payload(program.store.data[instruction].range())
                .unwrap()
        }
        let typed_call = program
            .store
            .function_instruction_range(function)
            .unwrap()
            .find(|instruction| program.store.tags[*instruction] == FullTag::ExprTypedCall)
            .expect("typed call exists");
        let callee = payload(&program, typed_call)[0] as usize;
        let text_slot = program
            .store
            .function_instruction_range(function)
            .unwrap()
            .find_map(|instruction| {
                let words = payload(&program, instruction);
                (program.store.tags[instruction] == FullTag::StmtLet
                    && words.get(1).is_some_and(|value| {
                        program.store.tags.get(*value as usize) == Some(&FullTag::ExprStr)
                    }))
                .then(|| words[0] as usize)
            })
            .expect("string initializer uses a general let instruction");
        let fact_range = program.store.functions[function]
            .slot_type_facts
            .bounds(program.store.slot_type_facts.len())
            .unwrap();
        let signature = payload(&program, typed_call)[2];

        let mut forged = program.clone();
        forged.store.extra[forged.store.data[callee].range().start as usize] =
            u32::try_from(text_slot).unwrap();
        forged.store.slot_type_facts[fact_range.start + text_slot] = signature;

        let error = FullVerifier::verify(&forged).unwrap_err();
        assert!(
            error
                .message
                .contains("local slot type fact disagrees with its initializer IR representation"),
            "{}",
            error.message
        );
    }

    #[test]
    fn typed_callable_local_fact_cannot_retype_a_string_branch_initializer() {
        let source = r#"type Scale = pure(n: Int) -> Int

pure double(n: Int) -> Int { n * 2 }

pure apply() -> Int {
  let callback: Scale = double
  let text = if true { "a" } else { "b" }
  callback(7)
}
"#;
        let program = fixture("typed-callable-forged-if-local-type.xsh", source);
        FullVerifier::verify(&program).unwrap();
        let function = program
            .store
            .functions
            .iter()
            .position(|function| program.store.string(function.name).unwrap() == "apply")
            .expect("apply function exists");
        fn payload(program: &FullProgram, instruction: usize) -> &[u32] {
            program
                .store
                .payload(program.store.data[instruction].range())
                .unwrap()
        }
        let typed_call = program
            .store
            .function_instruction_range(function)
            .unwrap()
            .find(|instruction| program.store.tags[*instruction] == FullTag::ExprTypedCall)
            .expect("typed call exists");
        let callee = payload(&program, typed_call)[0] as usize;
        assert_eq!(program.store.tags[callee], FullTag::ExprParam);
        let text_slot = program
            .store
            .function_instruction_range(function)
            .unwrap()
            .find_map(|instruction| {
                let words = payload(&program, instruction);
                (program.store.tags[instruction] == FullTag::StmtLet
                    && words.get(1).is_some_and(|value| {
                        program.store.tags.get(*value as usize) == Some(&FullTag::ExprIf)
                    }))
                .then(|| words[0] as usize)
            })
            .expect("conditional string initializer uses a general let instruction");
        let fact_range = program.store.functions[function]
            .slot_type_facts
            .bounds(program.store.slot_type_facts.len())
            .unwrap();
        let signature = payload(&program, typed_call)[2];

        let mut forged = program.clone();
        forged.store.extra[forged.store.data[callee].range().start as usize] =
            u32::try_from(text_slot).unwrap();
        forged.store.slot_type_facts[fact_range.start + text_slot] = signature;

        let error = FullVerifier::verify(&forged).unwrap_err();
        assert!(
            error
                .message
                .contains("local slot type fact disagrees with its initializer IR representation"),
            "{}",
            error.message
        );
    }

    #[test]
    fn checked_initializer_cycle_is_rejected_before_recursive_type_derivation() {
        let source = r#"pure identity(value: UInt) -> UInt {
  let checked = value
  checked
}
"#;
        let program = fixture("checked-initializer-cycle.xsh", source);
        FullVerifier::verify(&program).unwrap();
        let checked_value = program
            .store
            .function_instruction_range(0)
            .unwrap()
            .find(|instruction| program.store.tags[*instruction] == FullTag::ExprCheckedValue)
            .expect("unsigned let initializer has a checked value wrapper");
        let checked_value_payload = program.store.data[checked_value].range();

        let mut malformed = program.clone();
        malformed.store.extra[checked_value_payload.start as usize] =
            u32::try_from(checked_value).unwrap();

        let error = FullVerifier::verify(&malformed).unwrap_err();
        assert!(error.message.contains("cycle"), "{}", error.message);
    }

    #[test]
    fn typed_callable_driver_temporary_module_fact_matches_its_source() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("xsh-typed-call-driver-module-fact-{stamp}"));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("first.xsh"),
            "##! First module for driver receiver tests.\n\
             ## The callable contract shared by both modules.\n\
             export type Handler = pure(value: Int) -> Int\n\
             pure double(value: Int) -> Int { value * 2 }\n\
             ## The exported callable value.\n\
             export let handler: Handler = double\n",
        )
        .unwrap();
        fs::write(
            root.join("second.xsh"),
            "##! Second module for driver receiver tests.\n\
             ## The same callable contract with a distinct exact export set.\n\
             export type Handler = pure(value: Int) -> Int\n\
             pure double(value: Int) -> Int { value * 3 }\n\
             ## The exported callable value.\n\
             export let handler: Handler = double\n\
             ## An extra export distinguishes the exact module type.\n\
             export let extra: Str = \"second\"\n",
        )
        .unwrap();
        let script = root.join("main.xsh");
        fs::write(
            &script,
            "use first\nuse second\nprint first.handler(value: 1)\nprint second.handler(value: 1)\n",
        )
        .unwrap();
        let (sources, parsed) = crate::loader::parse_script(&script.to_string_lossy()).unwrap();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.arena.span_source_id.unwrap();
        let source = sources.get(source_id).unwrap().text().to_string();
        let checked = Checker::check_arena_with_sources_and_options(
            &parsed.arena,
            &sources,
            source_id,
            crate::sema::check::CheckOptions {
                reveal_types: true,
                embedded_bodies: true,
                ..crate::sema::check::CheckOptions::default()
            },
        );
        let declarations = Checker::compact_declarations(&parsed.arena, checked);
        assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
        let program = FullBuilder::build_compact(
            &parsed.arena,
            &declarations,
            &source,
            Arc::new(sources),
            source_id,
        )
        .unwrap();
        FullVerifier::verify(&program).unwrap();

        let (temporary_slot, second_type) = program
            .store
            .driver_steps
            .iter()
            .find_map(|step| {
                let slots = step.slots.bounds(program.store.driver_slots.len())?;
                let slots_start = slots.start;
                let rows = &program.store.driver_slots[slots];
                let first_type = rows
                    .iter()
                    .find(|row| {
                        row.flags & DRIVER_SLOT_TEMPORARY == 0
                            && program.store.string(row.name).ok() == Some("first")
                    })
                    .map(|row| row.type_id)?;
                let second_type = rows
                    .iter()
                    .find(|row| {
                        row.flags & DRIVER_SLOT_TEMPORARY == 0
                            && program.store.string(row.name).ok() == Some("second")
                    })
                    .map(|row| row.type_id)?;
                if first_type == second_type
                    || !rows.iter().any(|row| {
                        row.flags & DRIVER_SLOT_TEMPORARY != 0 && row.type_id == first_type
                    })
                {
                    return None;
                }
                let temp = rows.iter().position(|row| {
                    row.flags & DRIVER_SLOT_TEMPORARY != 0 && row.type_id == first_type
                })?;
                Some((slots_start + temp, second_type))
            })
            .expect("named-argument lowering emits a typed temporary module receiver");
        assert!(matches!(
            program.store.semantic.to_type(second_type),
            Ok(Type::Module(_))
        ));
        let mut forged = program.clone();
        forged.store.driver_slots[temporary_slot].type_id = second_type;
        let error = FullVerifier::verify(&forged).unwrap_err();
        assert!(
            error.message.contains("temporary driver slot type fact disagrees with its MatchExpr initializer"),
            "{}",
            error.message
        );

        let first_use = program
            .store
            .driver_steps
            .iter()
            .position(|step| {
                if step.tag != FullDriverTag::Use {
                    return false;
                }
                let Ok(payload) = program.store.payload(step.data.range()) else {
                    return false;
                };
                payload
                    .first()
                    .and_then(|key| program.store.string(*key).ok())
                    .is_some_and(|key| key.ends_with("first.xsh"))
            })
            .expect("first import registration exists");
        let use_step = program.store.driver_steps[first_use];
        assert_ne!(use_step.registered_module_type, IR_NONE);
        let mut missing = program.clone();
        missing.store.driver_steps[first_use].registered_module_type = IR_NONE;
        let error = FullVerifier::verify(&missing).unwrap_err();
        assert!(
            error
                .message
                .contains("exact module binding has no registered result type"),
            "{}",
            error.message
        );

        let non_use = program
            .store
            .driver_steps
            .iter()
            .position(|step| step.tag != FullDriverTag::Use)
            .expect("the root has a non-import driver step");
        let mut misplaced = program.clone();
        misplaced.store.driver_steps[non_use].registered_module_type = use_step.registered_module_type;
        let error = FullVerifier::verify(&misplaced).unwrap_err();
        assert!(
            error
                .message
                .contains("non-import driver step has a registered module type"),
            "{}",
            error.message
        );

        let mut forged = program.clone();
        forged.store.driver_steps[first_use].registered_module_type = second_type.raw();
        let error = FullVerifier::verify(&forged).unwrap_err();
        assert!(
            error
                .message
                .contains("registered module type differs from its exact export set"),
            "{}",
            error.message
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn typed_callable_module_receiver_facts_reject_another_import_slot() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("xsh-typed-call-module-slot-{stamp}"));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("first.xsh"),
            "##! First module for typed-call verifier tests.\n\
             ## The pure callable contract.\n\
             export type Handler = pure(value: Int) -> Int\n\
             pure double(value: Int) -> Int { value * 2 }\n\
             ## The exported callable value.\n\
             export let handler: Handler = double\n",
        )
        .unwrap();
        fs::write(
            root.join("second.xsh"),
            "##! Second module for typed-call verifier tests.\n\
             ## The effectful callable contract.\n\
             export type Handler = proc(value: Int) [error] -> Result[Str, Error]\n\
             proc label(value: Int) [error] -> Result[Str, Error] { Ok(f\"{value}\") }\n\
             ## The exported callable value.\n\
             export let handler: Handler = label\n",
        )
        .unwrap();
        let script = root.join("main.xsh");
        fs::write(
            &script,
            "use first\n\
             use second\n\
             pure apply(value: Int) -> Int {\n\
               let retained = second\n\
               first.handler(value)\n\
             }\n\
             print first.handler(1)\n",
        )
        .unwrap();

        let (sources, parsed) = crate::loader::parse_script(&script.to_string_lossy()).unwrap();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = parsed.arena.arena.span_source_id.unwrap();
        let source = sources.get(source_id).unwrap().text().to_string();
        let checked = Checker::check_arena_with_sources_and_options(
            &parsed.arena,
            &sources,
            source_id,
            crate::sema::check::CheckOptions {
                reveal_types: true,
                embedded_bodies: true,
                ..crate::sema::check::CheckOptions::default()
            },
        );
        let declarations = Checker::compact_declarations(&parsed.arena, checked);
        assert!(
            declarations.diagnostics.is_empty(),
            "{:?}",
            declarations.diagnostics
        );
        let program = FullBuilder::build_compact(
            &parsed.arena,
            &declarations,
            &source,
            Arc::new(sources),
            source_id,
        )
        .unwrap();
        FullVerifier::verify(&program).unwrap();

        let function = program
            .store
            .functions
            .iter()
            .position(|function| program.store.string(function.name).unwrap() == "apply")
            .expect("apply function exists");
        let captures = program.store.functions[function]
            .captures
            .bounds(program.store.captures.len())
            .unwrap();
        let other_module_slot = program.store.captures[captures]
            .iter()
            .find(|capture| program.store.string(capture.name).unwrap() == "second")
            .expect("second import is retained by apply")
            .slot_and_flags
            & !(1 << 31);
        let payload = |instruction: usize| {
            program
                .store
                .payload(program.store.data[instruction].range())
                .unwrap()
        };
        let receiver = program
            .store
            .function_instruction_range(function)
            .unwrap()
            .find_map(|instruction| {
                if program.store.tags[instruction] != FullTag::ExprTypedCall {
                    return None;
                }
                let callee = *payload(instruction).first()? as usize;
                if program.store.tags.get(callee) != Some(&FullTag::ExprField) {
                    return None;
                }
                let receiver = *payload(callee).first()? as usize;
                (program.store.tags.get(receiver) == Some(&FullTag::ExprParam))
                    .then_some(receiver)
            })
            .expect("imported callable field receiver is a parameter");
        let mut wrong_receiver = program.clone();
        wrong_receiver.store.extra
            [wrong_receiver.store.data[receiver].range().start as usize] = other_module_slot;
        let error = FullVerifier::verify(&wrong_receiver).unwrap_err();
        assert!(
            error
                .message
                .contains("typed call signature differs from its callee field's callable type"),
            "{}",
            error.message
        );

        let driver_step = program
            .store
            .driver_steps
            .iter()
            .enumerate()
            .find(|(index, _)| {
                program
                    .store
                    .driver_instruction_range(*index)
                    .unwrap()
                    .any(|instruction| program.store.tags[instruction] == FullTag::ExprTypedCall)
            })
            .map(|(_, step)| step)
            .expect("top-level imported callable call exists");
        let driver_slots = driver_step
            .slots
            .bounds(program.store.driver_slots.len())
            .unwrap();
        let other_driver_slot = program.store.driver_slots[driver_slots]
            .iter()
            .find(|slot| program.store.string(slot.name).unwrap() == "second")
            .expect("second import is present in the driver slot layout")
            .slot;
        let mut driver_range = program
            .store
            .driver_instruction_range(
                program
                    .store
                    .driver_steps
                    .iter()
                    .position(|step| std::ptr::eq(step, driver_step))
                    .unwrap(),
            )
            .unwrap();
        let driver_receiver = driver_range
            .find_map(|instruction| {
                if program.store.tags[instruction] != FullTag::ExprTypedCall {
                    return None;
                }
                let callee = *payload(instruction).first()? as usize;
                if program.store.tags.get(callee) != Some(&FullTag::ExprField) {
                    return None;
                }
                let receiver = *payload(callee).first()? as usize;
                (program.store.tags.get(receiver) == Some(&FullTag::ExprParam))
                    .then_some(receiver)
            })
            .expect("top-level field receiver is a parameter");
        let mut wrong_driver_receiver = program.clone();
        wrong_driver_receiver.store.extra
            [wrong_driver_receiver.store.data[driver_receiver].range().start as usize] =
            other_driver_slot;
        let error = FullVerifier::verify(&wrong_driver_receiver).unwrap_err();
        assert!(
            error
                .message
                .contains("typed call signature differs from its callee field's callable type"),
            "{}",
            error.message
        );

        let _ = fs::remove_dir_all(root);
    }

    // A typed call through a parameter was checked against the parameter's
    // declared callable type, which the function header records. A row that
    // names another callable type of the same kind and arity would bind its
    // arguments to a signature the parameter was never held to.
    #[test]
    fn typed_call_through_a_parameter_names_the_parameter_s_type() {
        run_with_large_stack(|| {
            let source = r#"type Scale = pure(n: Int) -> Int
type Measure = pure(text: Str) -> Int

pure double(n: Int) -> Int {
  n * 2
}

pure length(text: Str) -> Int {
  text.byte_len()
}

pure apply(scale: Scale, n: Int) -> Int {
  scale(n)
}

pure measure(by: Measure, text: Str) -> Int {
  by(text)
}

pure both() -> Int {
  apply(double, 4) + measure(length, "abc")
}
"#;
            let program = Arc::new(fixture("typed-callable-parameters.xsh", source));
            FullVerifier::verify(&program).unwrap();
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "both")),
                    LoweredFunctionKind::Pure,
                    &[],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("both exists");
            assert_eq!(result.unwrap(), Value::Int(11));

            let calls = program
                .store
                .tags
                .iter()
                .enumerate()
                .filter(|(_, tag)| **tag == FullTag::ExprTypedCall)
                .map(|(index, _)| {
                    program.store.data[index]
                        .range()
                        .bounds(program.store.extra.len())
                        .unwrap()
                        .start
                })
                .collect::<Vec<_>>();
            assert_eq!(calls.len(), 2, "one typed call per parameter");
            let (first, second) = (calls[0], calls[1]);
            assert_ne!(
                program.store.extra[first + 2],
                program.store.extra[second + 2],
                "the two parameters have different callable types"
            );
            let mut swapped = (*program).clone();
            swapped.store.extra[first + 2] = program.store.extra[second + 2];
            let error = FullVerifier::verify(&swapped).unwrap_err();
            assert!(
                error.message.contains(
                    "typed call signature differs from its callee slot's callable type"
                ),
                "{}",
                error.message
            );
        });
    }

    // A direct local slot with one declared callable type carries the same
    // signature proof as a function parameter. The verifier must reject a
    // typed-call row that names a different callable signature before it can
    // reach the callee's runtime parameter boundary.
    #[test]
    fn typed_call_through_a_local_names_the_slot_s_callable_type() {
        run_with_large_stack(|| {
            let source = r#"type Scale = pure(n: Int) -> Int
type Measure = pure(text: Str) -> Int
type Pair = pure(n: Int, m: Int) -> Int

pure double(n: Int) -> Int {
  n * 2
}

pure triple(n: Int) -> Int {
  n * 3
}

pure length(text: Str) -> Int {
  text.byte_len()
}

pure width(text: Str) -> Int {
  text.byte_len() + 1
}

pure sum(n: Int, m: Int) -> Int {
  n + m
}

pure product(n: Int, m: Int) -> Int {
  n * m
}

pure all(wide: Bool, n: Int, text: Str) -> Int {
  let scale: Scale = if wide { triple } else { double }
  let measure: Measure = if wide { width } else { length }
  let pair: Pair = if wide { product } else { sum }
  scale(n) + measure(text) + pair(n, n)
}
"#;
            let program = Arc::new(fixture("typed-callable-locals.xsh", source));
            FullVerifier::verify(&program).unwrap();
            let call = |program: &Arc<FullProgram>| {
                let mut evaluator =
                    Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(program));
                evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(program, "all")),
                        LoweredFunctionKind::Pure,
                        &[
                            Value::Bool(false),
                            Value::Int(5),
                            Value::Str(Arc::from("abc")),
                        ],
                        Span::new(program.store.source_id, 0, 0),
                    )
                    .expect("all exists")
            };
            assert_eq!(call(&program).unwrap(), Value::Int(23));

            // Payload words: callee, kind, signature type, argument block.
            let calls = program
                .store
                .tags
                .iter()
                .enumerate()
                .filter(|(_, tag)| **tag == FullTag::ExprTypedCall)
                .map(|(index, _)| {
                    program.store.data[index]
                        .range()
                        .bounds(program.store.extra.len())
                        .unwrap()
                        .start
                })
                .collect::<Vec<_>>();
            assert_eq!(calls.len(), 3, "one typed call per local");
            let (scale, measure, pair) = (calls[0], calls[1], calls[2]);
            // Where the callee of a call keeps the slot it reads.
            let callee_slot = |call: usize| {
                let callee = program.store.extra[call] as usize;
                program.store.data[callee]
                    .range()
                    .bounds(program.store.extra.len())
                    .unwrap()
                    .start
            };

            // `scale(n)` reads the local that holds a `Measure`; the row's
            // signature no longer agrees with the callee slot's declaration.
            let mut retyped = (*program).clone();
            retyped.store.extra[callee_slot(scale)] = program.store.extra[callee_slot(measure)];
            let error = FullVerifier::verify(&retyped).unwrap_err();
            assert!(
                error
                    .message
                    .contains("typed call signature differs from its callee slot's callable type"),
                "{}",
                error.message
            );

            // `scale(n)` reading the two-argument `Pair` slot also disagrees.
            let mut short = (*program).clone();
            short.store.extra[callee_slot(scale)] = program.store.extra[callee_slot(pair)];
            let error = FullVerifier::verify(&short).unwrap_err();
            assert!(
                error
                    .message
                    .contains("typed call signature differs from its callee slot's callable type"),
                "{}",
                error.message
            );
        });
    }

    #[test]
    fn typed_call_through_a_capture_names_the_capture_s_callable_type() {
        run_with_large_stack(|| {
            let source = r#"type Scale = pure(n: Int) -> Int
type Measure = pure(text: Str) -> Int

pure double(n: Int) -> Int { n * 2 }
pure length(text: Str) -> Int { text.byte_len() }

let scale: Scale = double
let measure: Measure = length

pure by_scale() -> Int { scale(5) }
pure by_measure() -> Int { measure("abc") }
pure outer() -> Int { by_scale() + by_measure() }
"#;
            let program = Arc::new(fixture("typed-callable-captures.xsh", source));
            FullVerifier::verify(&program).unwrap();

            let mut capture_calls = Vec::new();
            for (function, row) in program.store.functions.iter().enumerate() {
                let captures = row.captures.bounds(program.store.captures.len()).unwrap();
                if captures.is_empty() {
                    continue;
                }
                for instruction in program.store.function_instruction_range(function).unwrap() {
                    if program.store.tags[instruction] == FullTag::ExprTypedCall {
                        capture_calls.push(
                            program.store.data[instruction]
                                .range()
                                .bounds(program.store.extra.len())
                                .unwrap()
                                .start,
                        );
                    }
                }
            }
            assert_eq!(capture_calls.len(), 2);
            assert_ne!(
                program.store.extra[capture_calls[0] + 2],
                program.store.extra[capture_calls[1] + 2]
            );
            let mut swapped = (*program).clone();
            swapped.store.extra[capture_calls[0] + 2] =
                program.store.extra[capture_calls[1] + 2];
            let error = FullVerifier::verify(&swapped).unwrap_err();
            assert!(
                error.message.contains(
                    "typed call signature differs from its callee slot's callable type"
                ),
                "{}",
                error.message
            );
        });
    }

    // A union reaches the verified program as a type-pool row and, where a
    // member converts wire strings, as a prepared schema that tries the
    // member schemas in the order written. Corrupting either is rejected.
    #[test]
    fn union_types_and_schemas_reject_corruption_in_a_lowered_program() {
        use super::super::super::require::PreparedSchema;
        run_with_large_stack(|| {
            let source = r#"enum UnionMode: Str { Fast = "fast", Slow = "slow" }
type UnionTask = {tool: Str, args: List[Union[Str, Path]], level: Union[Int, UnionMode]}

pure union_task(source: Str) -> Result[Str] {
  let task = json.decode(source)?.require(UnionTask)?
  let level = match task.level {
    count is Int => f"{count}",
    _ is UnionMode => "mode",
  }
  return Ok(f"{task.tool} {level} {task.args.len()}")
}
"#;
            let program = Arc::new(fixture("union-types.xsh", source));
            FullVerifier::verify(&program).unwrap();

            for (raw, expected) in [
                (
                    "{\"tool\":\"make\",\"args\":[\"all\"],\"level\":3}",
                    "make 3 1",
                ),
                (
                    "{\"tool\":\"make\",\"args\":[],\"level\":\"fast\"}",
                    "make mode 0",
                ),
            ] {
                let mut evaluator =
                    Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let result = evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, "union_task")),
                        LoweredFunctionKind::Pure,
                        &[Value::Str(Arc::from(raw))],
                        Span::new(program.store.source_id, 0, 0),
                    )
                    .expect("union function exists");
                assert_eq!(result.unwrap(), Value::ok(Value::Str(Arc::from(expected))));
            }

            let mut one_member = (*program).clone();
            assert!(one_member.store.semantic.truncate_first_union_for_test());
            assert!(
                FullVerifier::verify(&one_member)
                    .unwrap_err()
                    .message
                    .contains("fewer than two members")
            );

            fn union_schema(schema: &mut PreparedSchema) -> Option<&mut PreparedSchema> {
                match schema {
                    PreparedSchema::Union(..) => Some(schema),
                    PreparedSchema::Record(fields) => fields
                        .iter_mut()
                        .find_map(|(_, schema)| union_schema(Arc::make_mut(schema))),
                    _ => None,
                }
            }
            let record = program
                .store
                .prepared_schemas
                .iter()
                .position(|schema| matches!(schema.as_ref(), PreparedSchema::Record(_)))
                .expect("the record schema with a converting union slot");

            // The member schemas no longer line up with the members.
            let mut reordered = (*program).clone();
            let Some(PreparedSchema::Union(_, members)) =
                union_schema(Arc::make_mut(&mut reordered.store.prepared_schemas[record]))
            else {
                panic!("the level slot is a converting union");
            };
            members.reverse();
            assert!(
                FullVerifier::verify(&reordered)
                    .unwrap_err()
                    .message
                    .contains("does not match")
            );

            // The schema claims a member order the checked type does not have.
            let mut retyped = (*program).clone();
            let Some(PreparedSchema::Union(ty, members)) =
                union_schema(Arc::make_mut(&mut retyped.store.prepared_schemas[record]))
            else {
                panic!("the level slot is a converting union");
            };
            let Type::Union(listed) = ty else {
                panic!("a union schema carries its union type");
            };
            listed.reverse();
            members.reverse();
            assert!(
                FullVerifier::verify(&retyped)
                    .unwrap_err()
                    .message
                    .contains("does not match")
            );

            let mut dropped = (*program).clone();
            let Some(PreparedSchema::Union(_, members)) =
                union_schema(Arc::make_mut(&mut dropped.store.prepared_schemas[record]))
            else {
                panic!("the level slot is a converting union");
            };
            members.pop();
            assert!(FullVerifier::verify(&dropped).is_err());
        });
    }

    // A validated type reaches the verified program as a type-pool row and
    // as a prepared schema that decodes the base and then tests the
    // validation. A program that names an unknown validation, validates the
    // wrong base, or drops the test from the schema is rejected.
    #[test]
    fn validated_types_and_schemas_reject_corruption_in_a_lowered_program() {
        use super::super::super::require::PreparedSchema;
        run_with_large_stack(|| {
            let source = r#"pure validated_first(source: Str) -> Result[Str] {
  let argv = json.decode(source)?.require(NonEmpty[Str])?
  return Ok(argv.first())
}
"#;
            let program = Arc::new(fixture("validated-types.xsh", source));
            FullVerifier::verify(&program).unwrap();

            let call = |raw: &str| {
                let mut evaluator =
                    Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, "validated_first")),
                        LoweredFunctionKind::Pure,
                        &[Value::Str(Arc::from(raw))],
                        Span::new(program.store.source_id, 0, 0),
                    )
                    .expect("validated function exists")
                    .unwrap()
            };
            assert_eq!(
                call("[\"ls\", \"-l\"]"),
                Value::ok(Value::Str(Arc::from("ls")))
            );
            // The empty list is refused by the validation, before `first()`.
            assert!(matches!(
                call("[]"),
                Value::Result(crate::runtime::value::ResultValue::Err(_))
            ));

            let schema = program
                .store
                .prepared_schemas
                .iter()
                .position(|schema| matches!(schema.as_ref(), PreparedSchema::Validated(..)))
                .expect("the validated schema");

            // The schema decodes the base and no longer tests the validation.
            let mut unvalidated = (*program).clone();
            let PreparedSchema::Validated(_, base) =
                program.store.prepared_schemas[schema].as_ref()
            else {
                panic!("the schema is the validated one");
            };
            unvalidated.store.prepared_schemas[schema] = Arc::clone(base);
            assert!(
                FullVerifier::verify(&unvalidated)
                    .unwrap_err()
                    .message
                    .contains("does not match")
            );

            // The base schema is not the schema of the validated type's base.
            let mut rebased = (*program).clone();
            let PreparedSchema::Validated(_, base) =
                Arc::make_mut(&mut rebased.store.prepared_schemas[schema])
            else {
                panic!("the schema is the validated one");
            };
            *base = Arc::new(PreparedSchema::Validate(Type::Str));
            assert!(
                FullVerifier::verify(&rebased)
                    .unwrap_err()
                    .message
                    .contains("does not match")
            );

            let mut unknown = (*program).clone();
            assert!(
                unknown
                    .store
                    .semantic
                    .corrupt_first_validated_for_test(false)
            );
            assert!(
                FullVerifier::verify(&unknown)
                    .unwrap_err()
                    .message
                    .contains("names no validation")
            );

            let mut wrong_base = (*program).clone();
            assert!(
                wrong_base
                    .store
                    .semantic
                    .corrupt_first_validated_for_test(true)
            );
            assert!(FullVerifier::verify(&wrong_base).is_err());
        });
    }

    // A bounded integer is stored as an integer, so everything that tests it
    // reads the bounds from its type: the parameter's check, and the schema
    // of the conversion, which the verifier ties to the bounded type.
    #[test]
    fn a_bounded_integer_keeps_its_bounds_in_a_lowered_program() {
        use super::super::super::require::PreparedSchema;
        run_with_large_stack(|| {
            let source = r#"type Port = Int range 1..=65535

pure following(port: Port) -> Int {
  return port + 1
}

pure checked(number: Int) -> Result[Port] {
  Ok(number as Port)
}
"#;
            let program = Arc::new(fixture("bounded-types.xsh", source));
            FullVerifier::verify(&program).unwrap();

            let call = |name: &str, value: i64| {
                let mut evaluator =
                    Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, name)),
                    LoweredFunctionKind::Pure,
                    &[Value::Int(value)],
                    Span::new(program.store.source_id, 0, 0),
                )
            };
            // Arithmetic takes the integer as it is stored.
            assert!(matches!(call("following", 80), Some(Ok(Value::Int(81)))));
            // A value the parameter refuses never reaches the body: the
            // direct call declines it or fails.
            assert!(!matches!(call("following", 0), Some(Ok(_))));
            assert!(!matches!(call("following", 65536), Some(Ok(_))));
            assert_eq!(
                call("checked", 8080).unwrap().unwrap(),
                Value::ok(Value::Int(8080))
            );
            for outside in [0, 70000, -1] {
                assert!(matches!(
                    call("checked", outside).unwrap().unwrap(),
                    Value::Result(crate::runtime::value::ResultValue::Err(_))
                ));
            }

            // The conversion's schema decodes an integer and no longer tests
            // the bounds.
            let schema = program
                .store
                .prepared_schemas
                .iter()
                .position(|schema| matches!(schema.as_ref(), PreparedSchema::Validated(..)))
                .expect("the bounded schema");
            let mut unbounded = (*program).clone();
            let PreparedSchema::Validated(_, base) =
                program.store.prepared_schemas[schema].as_ref()
            else {
                panic!("the schema is the bounded one");
            };
            unbounded.store.prepared_schemas[schema] = Arc::clone(base);
            assert!(
                FullVerifier::verify(&unbounded)
                    .unwrap_err()
                    .message
                    .contains("does not match")
            );

            let mut empty = (*program).clone();
            assert!(empty.store.semantic.corrupt_first_bounded_for_test(false));
            assert!(
                FullVerifier::verify(&empty)
                    .unwrap_err()
                    .message
                    .contains("bounded type admits no value")
            );

            let mut wrong_base = (*program).clone();
            assert!(
                wrong_base
                    .store
                    .semantic
                    .corrupt_first_bounded_for_test(true)
            );
            assert!(FullVerifier::verify(&wrong_base).is_err());
        });
    }

    // A validated parameter stored as a scalar has a storage kind that says
    // nothing of the validation, so it carries the test of its declared type
    // and a call that arrives with an unchecked value is refused by it.
    #[test]
    fn a_validated_scalar_parameter_carries_the_test_of_its_type() {
        run_with_large_stack(|| {
            let source = r#"pure beneath(rel: RelPath) -> Path {
  return rel
}
"#;
            let program = Arc::new(fixture("validated-params.xsh", source));
            FullVerifier::verify(&program).unwrap();

            let call = |program: &Arc<FullProgram>, raw: &str| {
                let mut evaluator =
                    Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(program));
                evaluator.call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(program, "beneath")),
                    LoweredFunctionKind::Pure,
                    &[Value::Path(
                        crate::runtime::value::PathValue::from_text(raw).unwrap(),
                    )],
                    Span::new(program.store.source_id, 0, 0),
                )
            };
            // A value the parameter refuses never reaches the body: the
            // direct call declines it or fails.
            assert!(matches!(call(&program, "etc/passwd"), Some(Ok(_))));
            assert!(!matches!(call(&program, "/etc/passwd"), Some(Ok(_))));
            assert!(!matches!(call(&program, "../etc/passwd"), Some(Ok(_))));

            let [cold] = program.store.param_cold.as_slice() else {
                panic!("the one parameter has a cold row");
            };
            let tested = program.store.validations[cold.validation as usize].type_id;
            assert_eq!(
                program.store.semantic.to_type(tested).unwrap(),
                Type::rel_path()
            );
        });
    }

    #[test]
    fn inferred_require_targets_survive_frontend_drop_on_both_routes() {
        run_with_large_stack(|| {
            let source =
                include_str!("../../../../tests/fixtures/frontend-indexed/inferred-require.xsh");
            let program = Arc::new(fixture("inferred-require.xsh", source));
            FullVerifier::verify(&program).unwrap();
            assert_eq!(
                program
                    .store
                    .prepared_schemas
                    .iter()
                    .filter(|schema| matches!(
                        schema.as_ref(),
                        super::super::super::require::PreparedSchema::Record(_)
                    ))
                    .count(),
                1,
                "both validation sites share the same checked record schema: {:?}",
                program.store.prepared_schemas
            );
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let raw = program.symbols.with_current(|| {
                Value::Record(BTreeMap::from([(Arc::from("jobs"), Value::Int(4))]).into())
            });
            let arguments = vec![raw];
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(
                        &program,
                        "inferred_requirement_via_parameter",
                    )),
                    LoweredFunctionKind::Pure,
                    &arguments,
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("prepared function exists");
            assert_eq!(result.unwrap(), Value::ok(Value::Int(4)));
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
        let declarations = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
        assert!(
            declarations.diagnostics.is_empty(),
            "{:?}",
            declarations.diagnostics
        );
        let program = FullBuilder::build_compact(
            &parsed.arena,
            &declarations,
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
            let source =
                include_str!("../../../../tests/fixtures/frontend-indexed/yield-delegation.xsh");
            let program = fixture("yield-delegation.xsh", source);
            let instruction = program
                .store
                .tags
                .iter()
                .position(|tag| *tag == FullTag::StmtYieldDelegate)
                .unwrap();
            let payload = program.store.data[instruction]
                .range()
                .bounds(program.store.extra.len())
                .unwrap();
            let mut malformed = program.clone();
            malformed.store.extra[payload.start] = u32::MAX;
            assert!(FullVerifier::verify(&malformed).is_err());
            let mut malformed = program.clone();
            malformed.store.extra[payload.start + 1] = u32::MAX;
            assert!(FullVerifier::verify(&malformed).is_err());
            let program = Arc::new(program);
            program.symbol_owner().with_current(|| {
                let mut evaluator =
                    Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let span = Span::new(program.store.source_id, 0, 0);
                let value = evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, "parent")),
                        LoweredFunctionKind::Pure,
                        &[],
                        span,
                    )
                    .expect("producer function")
                    .expect("producer creation");
                let Value::Stream(mut stream) = value else {
                    panic!("producer returns stream");
                };
                for expected in [1, 2, 3] {
                    assert_eq!(
                        evaluator.stream_next(&mut stream, span).unwrap(),
                        Some(Value::Int(expected))
                    );
                }
                let error = evaluator
                    .stream_next(&mut stream, span)
                    .expect_err("late delegated error");
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
            let program = fixture(
                "accept-policy-rows.xsh",
                r#"
proc checked() [process, error] {
  let text = run.text --accept=[0,1] sh -c "exit 1" ?
  let child = spawn run --accept=[0,1] sh -c "exit 1" ?
  let command = process.command_argv("sh", ["sh", "-c", "exit 1"], accept: [0,1])
  print $text $child.pid
  let status = process.run(command)?
}
"#,
            );
            FullVerifier::verify(&program).unwrap();
            for tag in [FullTag::ExprRunCapture, FullTag::ExprSpawnRun, FullTag::ExprProcessCommandArgv] {
                let instruction = program
                    .store
                    .tags
                    .iter()
                    .position(|actual| *actual == tag)
                    .expect("process row");
                let payload = program.store.data[instruction]
                    .range()
                    .bounds(program.store.extra.len())
                    .unwrap();
                let function = (0..program.function_count())
                    .map(|index| program.function_view_at(index).unwrap())
                    .find(|view| program.store.function_instruction_range(view.index()).unwrap().contains(&instruction))
                    .expect("process row belongs to a function");
                let execution = function.execution().unwrap();
                let context = DecodeContext::new(&execution);
                let cursor = FullCursor::new(&program.store.extra[payload.clone()]);
                let policy = match tag {
                    FullTag::ExprRunCapture => LoweredRunCapture::field_cursor(context, cursor, "accept"),
                    FullTag::ExprSpawnRun => LoweredSpawnRun::field_cursor(context, cursor, "accept"),
                    FullTag::ExprProcessCommandArgv => LoweredProcessCommandArgv::field_cursor(context, cursor, "accept"),
                    _ => unreachable!(),
                }.unwrap();
                let presence = payload.start + policy.index;
                assert_eq!(program.store.extra[presence], 1, "policy is present");
                let mut invalid_value = program.clone();
                invalid_value.store.extra[presence + 1] = u32::MAX;
                assert!(FullVerifier::verify(&invalid_value).is_err());
                let mut invalid_presence = program.clone();
                invalid_presence.store.extra[presence] = 2;
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
        leaf.payload = crate::runtime::value::RecordMap::from([(
            "job".into(),
            Value::NetJob(Box::new(crate::runtime::value::NetJobValue { id: 7, owner: Arc::new(()) })),
        )]);
        let mut chain = Value::Error(Box::new(leaf));
        for _ in 0..100_000 {
            chain = Value::Error(Box::new(RuntimeError::new("outer", "translation")))
                .with_error_cause(chain)
                .unwrap();
        }
        let process = Value::RunError(Box::new(crate::runtime::value::RunError::from_status(
            crate::runtime::process::ProcessStatus::exited(7),
        )))
        .with_error_cause(chain)
        .unwrap();
        let shared = Value::List(vec![process.clone(), process]);
        assert!(Evaluator::context_scope_runtime_value_escapes(&shared));
        assert!(Evaluator::context_scope_value_escapes(
            &LoweredValue::ResultErr(Box::new(shared))
        ));
    }

    #[test]
    fn context_scope_command_capture_tails_execute_after_frontend_drop_on_both_routes() {
        run_with_large_stack(|| {
            let source = include_str!(
                "../../../../tests/fixtures/frontend-indexed/context-scope-capture-tails.xsh"
            );
            let program = Arc::new(fixture("context-scope-capture-tails.xsh", source));
            FullVerifier::verify(&program).unwrap();
            for (name, expected, stdout) in [
                (
                    "text_tail",
                    Some(Value::ok(Value::Str(Arc::from("text")))),
                    b"".as_slice(),
                ),
                (
                    "bytes_tail",
                    Some(Value::ok(Value::Bytes(b"bytes".to_vec()))),
                    b"bytes\n".as_slice(),
                ),
                (
                    "record_tail",
                    Some(Value::ok(Value::Str(Arc::from("record")))),
                    b"".as_slice(),
                ),
                (
                    "nested_tail",
                    Some(Value::ok(Value::ok(Value::Str(Arc::from("nested"))))),
                    b"".as_slice(),
                ),
                (
                    "discarded_tail",
                    Some(Value::ok(Value::Unit)),
                    b"".as_slice(),
                ),
                ("failed_tail", None, b"".as_slice()),
            ] {
                let mut evaluator =
                    Evaluator::new_with_sources(Vec::new(), (*program.sources).clone())
                        .with_env_var(b"XSH_CAPTURE_TAIL".to_vec(), b"outer".to_vec());
                let original_cwd = evaluator.cwd.clone();
                let original_env = evaluator.env.snapshot_clone();
                evaluator.indexed_program = Some(Arc::clone(&program));
                let result = evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)),
                        LoweredFunctionKind::Proc,
                        &[],
                        Span::new(program.store.source_id, 0, 0),
                    )
                    .expect("capture tail fixture function exists")
                    .unwrap();
                if let Some(expected) = expected {
                    assert_eq!(result, expected, "{name}");
                } else {
                    assert!(
                        matches!(
                            result,
                            Value::Result(crate::runtime::value::ResultValue::Err(_))
                        ),
                        "{name}: {result:?}"
                    );
                }
                assert_eq!(evaluator.stdout, stdout, "{name}");
                assert_eq!(evaluator.cwd, original_cwd, "{name}");
                assert_eq!(evaluator.env.snapshot_clone(), original_env, "{name}");
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
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone())
                .with_env_var(b"XSH_SCOPE_ASSIGNMENT".to_vec(), b"outer".to_vec());
            let original_env = evaluator.env.snapshot_clone();
            evaluator.indexed_program = Some(Arc::clone(&program));
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "scoped")),
                    LoweredFunctionKind::Proc,
                    &[],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("scope function exists");
            assert_eq!(result.unwrap_err().kind, "context-scope-escape");
            assert_eq!(evaluator.env.snapshot_clone(), original_env);
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "local")),
                    LoweredFunctionKind::Proc,
                    &[],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("scope function exists");
            assert_eq!(result.unwrap(), Value::Int(7));
            assert_eq!(evaluator.env.snapshot_clone(), original_env);
        });
    }

    /// A `within` scope is a context scope of its own kind. The lowered
    /// program is valid as lowered, is rejected before it runs when its kind
    /// or its limit is corrupted, and reports its deadline as an `Err` that
    /// implements `Timeout` while leaving no deadline open behind it.
    #[test]
    fn within_scope_verifies_its_kind_and_closes_its_deadline() {
        run_with_large_stack(|| {
            let source = "proc limited(limit: Duration, pause: Duration) [time, error] -> Result[Int] {\n  within limit {\n    time.sleep(pause)\n    7\n  }\n}\n";
            let program = fixture("within-scope.xsh", source);
            FullVerifier::verify(&program).expect("the lowered scope is valid");
            let instruction = program
                .store
                .tags
                .iter()
                .position(|tag| *tag == FullTag::ExprContextScope)
                .expect("scope instruction");
            let payload = program.store.data[instruction]
                .range()
                .bounds(program.store.extra.len())
                .unwrap();
            assert_eq!(program.store.extra[payload.start], 2, "the scope's kind");
            let mut bad_kind = program.clone();
            bad_kind.store.extra[payload.start] = 3;
            assert!(FullVerifier::verify(&bad_kind).is_err());
            let mut bad_limit = program.clone();
            bad_limit.store.extra[payload.start + 1] = u32::MAX;
            assert!(FullVerifier::verify(&bad_limit).is_err());

            let program = Arc::new(program);
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
            evaluator.indexed_program = Some(Arc::clone(&program));
            let millis = |millis| Value::Duration(crate::runtime::value::DurationValue { millis });
            let mut call = |limit, pause| {
                let result = evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, "limited")),
                        LoweredFunctionKind::Proc,
                        &[millis(limit), millis(pause)],
                        Span::new(program.store.source_id, 0, 0),
                    )
                    .expect("scope function exists");
                assert!(evaluator.within_deadlines.is_empty());
                result
            };
            // The scope's `Result` is the function's value either way.
            assert_eq!(
                format!("{:?}", call(60_000, 0).unwrap()),
                "Result(Ok(Int(7)))"
            );
            let timeout = format!("{:?}", call(20, 60_000).unwrap());
            assert!(timeout.starts_with("Result(Err("), "{timeout}");
            assert!(timeout.contains("\"Timeout\""), "{timeout}");
            assert!(timeout.contains("did not finish within 20ms"), "{timeout}");
        });
    }

    #[test]
    fn context_scope_verifies_payload_and_preserves_native_environment_bytes() {
        run_with_large_stack(|| {
            let source = "proc selected(value: Path) [env, error] -> Result[Int] { env ({XSH_NATIVE_SCOPE: value}) { 7 } }\n";
            let program = fixture("context-scope-native.xsh", source);
            let instruction = program
                .store
                .tags
                .iter()
                .position(|tag| *tag == FullTag::ExprContextScope)
                .expect("scope instruction");
            let payload = program.store.data[instruction]
                .range()
                .bounds(program.store.extra.len())
                .unwrap();
            let mut bad_kind = program.clone();
            bad_kind.store.extra[payload.start] = 3;
            assert!(FullVerifier::verify(&bad_kind).is_err());
            let mut bad_input = program.clone();
            bad_input.store.extra[payload.start + 1] = u32::MAX;
            assert!(FullVerifier::verify(&bad_input).is_err());
            let program = Arc::new(program);
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone())
                .with_env_var(b"XSH_RAW_INHERITED".to_vec(), b"raw\xff bytes".to_vec());
            let original_env = evaluator.env.snapshot_clone();
            evaluator.indexed_program = Some(Arc::clone(&program));
            let argument = Value::Path(PathValue::new(b"raw\xfe name".to_vec()).unwrap());
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "selected")),
                    LoweredFunctionKind::Proc,
                    std::slice::from_ref(&argument),
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("scope function exists");
            assert!(result.is_ok(), "{result:?}");
            assert_eq!(evaluator.env.snapshot_clone(), original_env);
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
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone())
                .with_env_var(b"XSH_FATAL_SCOPE".to_vec(), b"outer".to_vec());
            let original_cwd = evaluator.cwd.clone();
            let original_env = evaluator.env.snapshot_clone();
            evaluator.indexed_program = Some(Arc::clone(&program));
            let result = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(program_name(&program, "fatal")),
                    LoweredFunctionKind::Proc,
                    &[],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("scope function exists");
            assert!(result.unwrap_err().abort.is_none());
            assert_eq!(evaluator.cwd, original_cwd);
            assert_eq!(evaluator.env.snapshot_clone(), original_env);
        });
    }

    #[test]
    fn default_parameter_calls_preserve_both_indexed_execution_routes_after_frontend_drop() {
        run_with_large_stack(|| {
            let source =
                include_str!("../../../../tests/fixtures/frontend-indexed/default-parameters.xsh");
            let program = Arc::new(fixture("default-parameters.xsh", source));
            FullVerifier::verify(&program).unwrap();
            for (name, expected) in [
                ("choose", 4),
                ("nested", 5),
                ("supplied", 9),
                ("alias_default", 4),
                ("alias_named", 11),
            ] {
                let mut evaluator =
                    Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let result = evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)),
                        LoweredFunctionKind::Pure,
                        &[],
                        Span::new(program.store.source_id, 0, 0),
                    )
                    .expect("defaulted function exists");
                assert_eq!(result.unwrap(), Value::Int(expected));
            }
            for (name, succeeds) in [("caught", false), ("skipped", true)] {
                let mut evaluator =
                    Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
                evaluator.indexed_program = Some(Arc::clone(&program));
                let result = evaluator
                    .call_indexed_direct(
                        LoweredFunctionKey::Name(program_name(&program, name)),
                        LoweredFunctionKind::Pure,
                        &[],
                        Span::new(program.store.source_id, 0, 0),
                    )
                    .expect("defaulted Result function exists");
                assert!(
                    matches!(
                        result.unwrap(),
                        Value::Result(crate::runtime::value::ResultValue::Ok(_))
                    ) == succeeds
                );
            }
        });
    }

    #[test]
    fn default_parameter_verifier_rejects_missing_or_inconsistent_entry_metadata() {
        let source =
            include_str!("../../../../tests/fixtures/frontend-indexed/default-parameters.xsh");
        let program = fixture("default-parameters.xsh", source);
        let parameter = program
            .store
            .params
            .iter()
            .position(|param| param.flags & 4 != 0)
            .unwrap();
        let mut missing_default = program.clone();
        missing_default.store.params[parameter].flags &= !2;
        assert!(
            FullVerifier::verify(&missing_default)
                .unwrap_err()
                .message
                .contains("non-rest defaulted parameter entry")
        );
        let mut inconsistent_entry = program.clone();
        inconsistent_entry.store.params[parameter].flags &= !4;
        assert!(
            FullVerifier::verify(&inconsistent_entry)
                .unwrap_err()
                .message
                .contains("entry does not match")
        );
        let mut rest_default = program;
        rest_default.store.params[parameter].flags |= 1;
        assert!(
            FullVerifier::verify(&rest_default)
                .unwrap_err()
                .message
                .contains("non-rest defaulted parameter entry")
        );
    }

    #[test]
    fn verifier_rejects_optional_guard_with_failure_parameter() {
        run_with_large_stack(|| {
            let guard_payload = |program: &FullProgram| {
                let guard = program
                    .store
                    .tags
                    .iter()
                    .position(|tag| *tag == FullTag::StmtGuard)
                    .unwrap();
                program.store.data[guard]
                    .range()
                    .bounds(program.store.extra.len())
                    .unwrap()
            };

            // The checker's optional-binding fact reaches the row.
            let optional = fixture(
                "optional-guard.xsh",
                "pure label(name: Str?) -> Str { guard let found = name else { return \"none\" }; found }\n",
            );
            FullVerifier::verify(&optional).unwrap();
            assert_eq!(optional.store.extra[guard_payload(&optional).end - 1], 1);

            // A Result guard binds its failure; marking it optional claims a
            // failure with no error for that slot.
            let result = fixture(
                "result-guard.xsh",
                "pure parse(text: Str) -> Result[Int] { text.parse_int() }\npure value(text: Str) -> Int { guard let number = parse(text) else { |failure| return failure.message.count_chars() }; number }\n",
            );
            FullVerifier::verify(&result).unwrap();
            let flag = guard_payload(&result).end - 1;
            assert_eq!(result.store.extra[flag], 0);
            let mut invalid = result.clone();
            invalid.store.extra[flag] = 1;
            let error = FullVerifier::verify(&invalid).unwrap_err();
            assert!(
                error
                    .message
                    .contains("optional guard cannot bind a failure parameter"),
                "{}",
                error.message
            );
        });
    }

    #[test]
    fn lowered_type_mappings_cover_every_storage_kind_and_policy_edge() {
        let owner = crate::symbol::SymbolOwner::new();
        let _owner = owner.enter();

        let kinds = [
            (LoweredType::Any, Type::Any),
            (LoweredType::Unit, Type::Unit),
            (LoweredType::Int, Type::Int),
            (LoweredType::Float, Type::Float),
            (LoweredType::Duration, Type::Duration),
            (LoweredType::Bool, Type::Bool),
            (LoweredType::Str, Type::Str),
            (LoweredType::Bytes, Type::Bytes),
            (LoweredType::Digest, Type::Digest),
            (LoweredType::Regex, Type::Regex),
            (LoweredType::Status, Type::Status),
            (LoweredType::Path, Type::Path),
            (LoweredType::Command, Type::Command),
            (LoweredType::ProcessHandle, Type::ProcessHandle),
            (LoweredType::NetJob, Type::NetJob),
            (LoweredType::FsRoot, Type::FsRoot),
            (LoweredType::FsLock, Type::FsLock),
            (
                LoweredType::Stream,
                Type::Stream(Box::new(Type::Any)),
            ),
            (LoweredType::Pure, Type::Pure),
            (LoweredType::Proc, Type::Proc),
            (LoweredType::Error, Type::Error),
            (LoweredType::Record, Type::ErasedRecord),
            (LoweredType::Module, Type::DynamicModule),
            (LoweredType::List, Type::List(Box::new(Type::Any))),
            (
                LoweredType::Map,
                Type::Map(Box::new(Type::Str), Box::new(Type::Any)),
            ),
            (
                LoweredType::Tag,
                Type::Tag(Name::intern("<tag>")),
            ),
            (
                LoweredType::Result,
                Type::Result(Box::new(Type::Any), Box::new(Type::Error)),
            ),
            (LoweredType::Set, Type::Set(Box::new(Type::Any))),
        ];

        for (kind, ty) in kinds {
            assert_eq!(lowered_type_to_type(kind), ty, "{kind:?}");
            assert_eq!(lowered_type_for_type(&ty), Some(kind), "{kind:?}");
        }

        let callable = |pure| {
            Type::Callable(Arc::new(crate::sema::types::TypedCallable {
                pure,
                sig: CallableType {
                    params: Vec::new(),
                    return_ty: Box::new(Type::Unit),
                    effects: None,
                },
            }))
        };
        let module = Type::Module(Arc::new(crate::sema::types::ModuleType::exact(
            std::collections::BTreeMap::new(),
        )));
        let variants = vec![
            (Type::Unknown, LoweredType::Any),
            (Type::UInt, LoweredType::Int),
            (Type::Null, LoweredType::Any),
            (Type::Optional(Box::new(Type::Int)), LoweredType::Any),
            (
                Type::Union(vec![Type::Int, Type::Str]),
                LoweredType::Any,
            ),
            (
                Type::ErrorFamily(Name::intern("IOError")),
                LoweredType::Error,
            ),
            (
                Type::ErrorVariant {
                    family: Name::intern("IOError"),
                    variant: Name::intern("NotFound"),
                },
                LoweredType::Error,
            ),
            (Type::Record(std::collections::BTreeMap::new()), LoweredType::Record),
            (module, LoweredType::Module),
            (callable(true), LoweredType::Pure),
            (callable(false), LoweredType::Proc),
            (Type::non_empty(Type::Int), LoweredType::List),
        ];
        for (ty, expected) in variants {
            assert_eq!(lowered_type_for_type(&ty), Some(expected), "{ty:?}");
            assert_eq!(lowered_type_from_type(&ty), Ok(expected), "{ty:?}");
        }

        let mut fields = std::collections::BTreeMap::new();
        fields.insert(Name::intern("field"), Type::Int);
        let nominal = Type::Validated(Box::new(
            crate::sema::validated::ValidatedType::new(
                crate::sema::validated::Validation::Nominal(Name::intern("Nominal")),
                Type::Record(fields),
            )
            .unwrap(),
        ));
        assert_eq!(lowered_type_for_type(&nominal), Some(LoweredType::Record));
        assert_eq!(lowered_type_from_type(&nominal), Ok(LoweredType::Record));

        let mut constraints = crate::sema::constraints::TypeConstraints::default();
        let inference = constraints.fresh(crate::source::Span::new(
            crate::source::SourceId::new(0),
            0,
            0,
        ));
        let unsupported = [
            Type::BuiltinParameter(crate::sema::types::BuiltinTypeParameter::Receiver),
            inference,
            Type::Invalid,
            Type::EnvPathList,
            Type::ProcessError,
        ];
        for ty in unsupported {
            assert_eq!(lowered_type_for_type(&ty), None, "{ty:?}");
            assert!(lowered_type_from_type(&ty).is_err(), "{ty:?}");
        }
        assert_eq!(
            driver_slot_runtime_kind(&Type::EnvPathList),
            Ok(LoweredType::List)
        );
        assert!(driver_slot_runtime_kind(&Type::Invalid).is_err());
        let facet = Type::ErrorFacet(Name::intern("Error"));
        assert_eq!(lowered_type_for_type(&facet), None);
        assert_eq!(lowered_type_from_type(&facet), Ok(LoweredType::Error));
        assert_eq!(
            lowered_type_from_type(&Type::Unknown),
            Ok(LoweredType::Any)
        );
    }
}
