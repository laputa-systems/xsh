#![allow(dead_code)]

use super::{
    LoweredBoolExpr, LoweredCallArg, LoweredErrorExpr, LoweredExpr, LoweredFunctionKey,
    LoweredFunctionKind, LoweredFunctionUnit, LoweredIntExpr, LoweredPattern,
    LoweredPipelineStage, LoweredPureFunction, LoweredRecordEntry, LoweredReturnKind, LoweredStmt,
    LoweredType, LoweredValue,
};
use crate::modules::RuntimeOp;
use crate::sema::types::Type;
use crate::source::{SourceId, SourceMap, Span};
use crate::syntax::node::{AssignOp, BinaryOp};
use std::collections::BTreeMap;
use std::fmt::Write;
use std::mem::size_of;
use std::num::NonZeroU32;
use std::sync::Arc;

const IR_NONE: u32 = u32::MAX;

macro_rules! ir_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        #[repr(transparent)]
        pub struct $name(NonZeroU32);

        impl $name {
            fn new(index: usize) -> Result<Self, IrBuildError> {
                let raw = u32::try_from(index + 1).map_err(|_| {
                    IrBuildError::format("id_overflow", None, 0, 0)
                })?;
                Ok(Self(NonZeroU32::new(raw).expect("IR ids are one-based")))
            }

            fn index(self) -> usize {
                self.0.get() as usize - 1
            }

            fn raw(self) -> u32 {
                self.0.get()
            }

            fn from_raw(raw: u32) -> Option<Self> {
                NonZeroU32::new(raw).map(Self)
            }
        }
    };
}

ir_id!(IrInstId);
ir_id!(IrBlockId);
ir_id!(IrFunctionId);
ir_id!(IrPatternId);
ir_id!(IrStringId);
ir_id!(IrBytesId);
ir_id!(IrLocationId);

trait IrExtraId: Copy {
    fn extra_raw(self) -> u32;
    fn extra_from_raw(raw: u32) -> Option<Self>;
}

macro_rules! impl_extra_id {
    ($($name:ident),+ $(,)?) => {
        $(
            impl IrExtraId for $name {
                fn extra_raw(self) -> u32 {
                    self.raw()
                }

                fn extra_from_raw(raw: u32) -> Option<Self> {
                    Self::from_raw(raw)
                }
            }
        )+
    };
}

impl_extra_id!(
    IrInstId,
    IrBlockId,
    IrFunctionId,
    IrPatternId,
    IrStringId,
    IrBytesId,
    IrLocationId,
);

#[derive(Default)]
struct IrExtraWriter {
    words: Vec<u32>,
}

impl IrExtraWriter {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            words: Vec::with_capacity(capacity),
        }
    }

    fn count(&mut self, count: usize) -> Result<(), IrBuildError> {
        self.words.push(
            u32::try_from(count)
                .map_err(|_| IrBuildError::format("payload_count_overflow", None, 0, 0))?,
        );
        Ok(())
    }

    fn id<T: IrExtraId>(&mut self, id: T) {
        self.words.push(id.extra_raw());
    }

    fn optional_id<T: IrExtraId>(&mut self, id: Option<T>) {
        self.words
            .push(id.map_or(IR_NONE, IrExtraId::extra_raw));
    }

    fn slot(&mut self, slot: usize) -> Result<(), IrBuildError> {
        self.words.push(IrBuilder::slot(slot)?);
        Ok(())
    }

    fn optional_slot(&mut self, slot: Option<usize>) -> Result<(), IrBuildError> {
        self.words
            .push(slot.map_or(Ok(IR_NONE), IrBuilder::slot)?);
        Ok(())
    }

    fn raw(&mut self, value: u32) {
        self.words.push(value);
    }

    fn finish(self, builder: &mut IrBuilder) -> Result<IrRange, IrBuildError> {
        builder.push_extra(&self.words)
    }
}

struct IrExtraReader<'a> {
    words: &'a [u32],
    index: usize,
}

impl<'a> IrExtraReader<'a> {
    fn new(words: &'a [u32]) -> Self {
        Self { words, index: 0 }
    }

    fn count(&mut self) -> Result<usize, IrVerifyError> {
        Ok(self.raw()? as usize)
    }

    fn id<T: IrExtraId>(&mut self, label: &str) -> Result<T, IrVerifyError> {
        T::extra_from_raw(self.raw()?)
            .ok_or_else(|| IrVerifyError::new(format!("{label} id is invalid")))
    }

    fn optional_id<T: IrExtraId>(&mut self, label: &str) -> Result<Option<T>, IrVerifyError> {
        let raw = self.raw()?;
        if raw == IR_NONE {
            return Ok(None);
        }
        T::extra_from_raw(raw)
            .map(Some)
            .ok_or_else(|| IrVerifyError::new(format!("optional {label} id is invalid")))
    }

    fn slot(&mut self) -> Result<u32, IrVerifyError> {
        self.raw()
    }

    fn optional_slot(&mut self) -> Result<Option<u32>, IrVerifyError> {
        let raw = self.raw()?;
        Ok((raw != IR_NONE).then_some(raw))
    }

    fn raw(&mut self) -> Result<u32, IrVerifyError> {
        let value = self
            .words
            .get(self.index)
            .copied()
            .ok_or_else(|| IrVerifyError::new("payload ended early"))?;
        self.index += 1;
        Ok(value)
    }

    fn remaining(&self) -> &'a [u32] {
        &self.words[self.index..]
    }

    fn finish(self) -> Result<(), IrVerifyError> {
        if self.index == self.words.len() {
            Ok(())
        } else {
            Err(IrVerifyError::new("payload has trailing words"))
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct IrOptionalId(u32);

impl IrOptionalId {
    const NONE: Self = Self(IR_NONE);

    fn some(raw: u32) -> Self {
        Self(raw)
    }

    fn raw(self) -> Option<u32> {
        (self.0 != IR_NONE).then_some(self.0)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct IrData {
    pub lhs: u32,
    pub rhs: u32,
}

impl IrData {
    const ZERO: Self = Self { lhs: 0, rhs: 0 };

    fn new(lhs: u32, rhs: u32) -> Self {
        Self { lhs, rhs }
    }

    fn from_range(range: IrRange) -> Self {
        Self::new(range.start, range.len)
    }

    fn range(self) -> IrRange {
        IrRange::new(self.lhs, self.rhs)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct IrRange {
    pub start: u32,
    pub len: u32,
}

impl IrRange {
    const EMPTY: Self = Self { start: 0, len: 0 };

    const fn new(start: u32, len: u32) -> Self {
        Self { start, len }
    }

    fn bounds(self, total: usize) -> Option<std::ops::Range<usize>> {
        let start = self.start as usize;
        let end = start.checked_add(self.len as usize)?;
        (end <= total).then_some(start..end)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct IrLocation {
    pub start: u32,
    pub len: u32,
}

impl IrLocation {
    const ZERO: Self = Self { start: 0, len: 0 };

    fn from_span(span: Span) -> Result<Self, IrBuildError> {
        Ok(Self {
            start: u32::try_from(span.start())
                .map_err(|_| IrBuildError::format("location_overflow", Some(span), 0, 0))?,
            len: u32::try_from(span.end().saturating_sub(span.start()))
                .map_err(|_| IrBuildError::format("location_overflow", Some(span), 0, 0))?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum IrTag {
    Unit,
    Int,
    Bool,
    Str,
    Bytes,
    Slot,
    Add,
    Sub,
    Mul,
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
    List,
    ListMap,
    Index,
    Call,
    SelfCall,
    Ok,
    Err,
    Error,
    Record,
    Field,
    RequireRecord,
    Try,
    Match,
    BytesUnpackBe,
    Let,
    AssignAdd,
    If,
    Loop,
    Guard,
    Print,
    Return,
    Break,
    Continue,
}

impl IrTag {
    fn is_statement(self) -> bool {
        matches!(
            self,
            Self::Let
                | Self::AssignAdd
                | Self::If
                | Self::Loop
                | Self::Guard
                | Self::Print
                | Self::Return
                | Self::Break
                | Self::Continue
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum IrPatternTag {
    Wildcard,
    Bind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum IrFunctionKind {
    Pure,
    Proc,
}

impl From<LoweredFunctionKind> for IrFunctionKind {
    fn from(kind: LoweredFunctionKind) -> Self {
        match kind {
            LoweredFunctionKind::Pure => Self::Pure,
            LoweredFunctionKind::Proc => Self::Proc,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum IrValueType {
    Any,
    Unit,
    Int,
    Bool,
    Str,
    Bytes,
    Record,
    List,
    Result,
}

impl IrValueType {
    fn from_lowered(ty: LoweredType) -> Option<Self> {
        Some(match ty {
            LoweredType::Any => Self::Any,
            LoweredType::Unit => Self::Unit,
            LoweredType::Int => Self::Int,
            LoweredType::Bool => Self::Bool,
            LoweredType::Str => Self::Str,
            LoweredType::Bytes => Self::Bytes,
            LoweredType::Record => Self::Record,
            LoweredType::List => Self::List,
            LoweredType::Result => Self::Result,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum IrReturnKind {
    PlainUnit,
    PlainInt,
    PlainBool,
    ResultUnit,
    ResultInt,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct IrBlock {
    pub instructions: IrRange,
    pub owner: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct IrFunction {
    pub name: u32,
    pub params: IrRange,
    pub captures: IrRange,
    pub body: IrOptionalId,
    pub slot_count: u32,
    pub kind: IrFunctionKind,
    pub return_kind: IrReturnKind,
    pub reserved: [u8; 2],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct IrParam {
    pub name: u32,
    pub slot: u32,
    pub ty: IrValueType,
    pub flags: u8,
    pub reserved: [u8; 2],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct IrCapture {
    pub name: u32,
    pub slot: u32,
    pub ty: IrValueType,
    pub mutable: u8,
    pub reserved: [u8; 2],
}

#[derive(Clone, Debug)]
pub struct IrStore {
    source_id: SourceId,
    tags: Vec<IrTag>,
    data: Vec<IrData>,
    instruction_locations: Vec<IrOptionalId>,
    extra: Vec<u32>,
    blocks: Vec<IrBlock>,
    functions: Vec<IrFunction>,
    params: Vec<IrParam>,
    captures: Vec<IrCapture>,
    pattern_tags: Vec<IrPatternTag>,
    pattern_data: Vec<IrData>,
    strings: Vec<IrRange>,
    string_bytes: Vec<u8>,
    bytes: Vec<IrRange>,
    byte_data: Vec<u8>,
    locations: Vec<IrLocation>,
}

impl Default for IrStore {
    fn default() -> Self {
        Self {
            source_id: SourceId::new(0),
            tags: Vec::new(),
            data: Vec::new(),
            instruction_locations: Vec::new(),
            extra: Vec::new(),
            blocks: Vec::new(),
            functions: Vec::new(),
            params: Vec::new(),
            captures: Vec::new(),
            pattern_tags: Vec::new(),
            pattern_data: Vec::new(),
            strings: Vec::new(),
            string_bytes: Vec::new(),
            bytes: Vec::new(),
            byte_data: Vec::new(),
            locations: Vec::new(),
        }
    }
}

impl IrStore {
    fn common_instruction_row_bytes() -> usize {
        size_of::<IrTag>() + size_of::<IrData>() + size_of::<IrOptionalId>()
    }

    fn instruction_count(&self) -> usize {
        self.tags.len()
    }

    fn payload(&self, range: IrRange) -> Result<&[u32], IrVerifyError> {
        let bounds = range
            .bounds(self.extra.len())
            .ok_or_else(|| IrVerifyError::new("extra range is out of bounds"))?;
        Ok(&self.extra[bounds])
    }

    fn string(&self, id: IrStringId) -> Result<&str, IrVerifyError> {
        let range = *self
            .strings
            .get(id.index())
            .ok_or_else(|| IrVerifyError::new("string id is out of bounds"))?;
        let bounds = range
            .bounds(self.string_bytes.len())
            .ok_or_else(|| IrVerifyError::new("string range is out of bounds"))?;
        std::str::from_utf8(&self.string_bytes[bounds])
            .map_err(|_| IrVerifyError::new("string blob contains invalid UTF-8"))
    }

    fn bytes(&self, id: IrBytesId) -> Result<&[u8], IrVerifyError> {
        let range = *self
            .bytes
            .get(id.index())
            .ok_or_else(|| IrVerifyError::new("bytes id is out of bounds"))?;
        let bounds = range
            .bounds(self.byte_data.len())
            .ok_or_else(|| IrVerifyError::new("bytes range is out of bounds"))?;
        Ok(&self.byte_data[bounds])
    }

    fn location(&self, id: IrOptionalId) -> Result<Option<IrLocation>, IrVerifyError> {
        let Some(raw) = id.raw() else {
            return Ok(None);
        };
        let id = IrLocationId::from_raw(raw)
            .ok_or_else(|| IrVerifyError::new("location sentinel is invalid"))?;
        self.locations
            .get(id.index())
            .copied()
            .map(Some)
            .ok_or_else(|| IrVerifyError::new("location id is out of bounds"))
    }

    fn shrink_to_fit(&mut self) {
        self.tags.shrink_to_fit();
        self.data.shrink_to_fit();
        self.instruction_locations.shrink_to_fit();
        self.extra.shrink_to_fit();
        self.blocks.shrink_to_fit();
        self.functions.shrink_to_fit();
        self.params.shrink_to_fit();
        self.captures.shrink_to_fit();
        self.pattern_tags.shrink_to_fit();
        self.pattern_data.shrink_to_fit();
        self.strings.shrink_to_fit();
        self.string_bytes.shrink_to_fit();
        self.bytes.shrink_to_fit();
        self.byte_data.shrink_to_fit();
        self.locations.shrink_to_fit();
    }

    fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            + self.tags.capacity() * size_of::<IrTag>()
            + self.data.capacity() * size_of::<IrData>()
            + self.instruction_locations.capacity() * size_of::<IrOptionalId>()
            + self.extra.capacity() * size_of::<u32>()
            + self.blocks.capacity() * size_of::<IrBlock>()
            + self.functions.capacity() * size_of::<IrFunction>()
            + self.params.capacity() * size_of::<IrParam>()
            + self.captures.capacity() * size_of::<IrCapture>()
            + self.pattern_tags.capacity() * size_of::<IrPatternTag>()
            + self.pattern_data.capacity() * size_of::<IrData>()
            + self.strings.capacity() * size_of::<IrRange>()
            + self.string_bytes.capacity()
            + self.bytes.capacity() * size_of::<IrRange>()
            + self.byte_data.capacity()
            + self.locations.capacity() * size_of::<IrLocation>()
    }

    fn extra_bytes_per_instruction(&self) -> f64 {
        if self.tags.is_empty() {
            return 0.0;
        }
        (self.extra.len() * size_of::<u32>()) as f64 / self.tags.len() as f64
    }
}

#[derive(Clone, Debug)]
pub struct IrProgram {
    store: IrStore,
    sources: Arc<SourceMap>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IrStorageEstimate {
    pub functions_seen: usize,
    pub functions_built: usize,
    pub instructions: usize,
    pub extra_words: usize,
}

impl IrStorageEstimate {
    fn add(&mut self, other: Self) {
        self.functions_seen += other.functions_seen;
        self.functions_built += other.functions_built;
        self.instructions += other.instructions;
        self.extra_words += other.extra_words;
    }

    fn extra_bytes_per_instruction(self) -> f64 {
        if self.instructions == 0 {
            return 0.0;
        }
        (self.extra_words * size_of::<u32>()) as f64 / self.instructions as f64
    }
}

impl IrProgram {
    fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            + self.store.retained_bytes()
            + 2 * size_of::<usize>()
            + self.sources.retained_bytes()
    }

    fn dump(&self) -> Result<String, IrVerifyError> {
        let mut output = String::new();
        writeln!(output, "source_id={}", self.store.source_id.raw())
            .expect("writing to String cannot fail");
        for (index, function) in self.store.functions.iter().enumerate() {
            let name = self.store.string(
                IrStringId::from_raw(function.name)
                    .ok_or_else(|| IrVerifyError::new("function name id is invalid"))?,
            )?;
            writeln!(
                output,
                "fn {index} {name} kind={:?} return={:?} slots={} body={:?}",
                function.kind, function.return_kind, function.slot_count, function.body
            )
            .expect("writing to String cannot fail");
        }
        for (index, block) in self.store.blocks.iter().enumerate() {
            writeln!(
                output,
                "block {index} owner={} instructions={}+{}",
                block.owner, block.instructions.start, block.instructions.len
            )
            .expect("writing to String cannot fail");
        }
        for index in 0..self.store.tags.len() {
            let location = self.store.location(self.store.instruction_locations[index])?;
            writeln!(
                output,
                "inst {index} {:?} {} {} loc={location:?}",
                self.store.tags[index], self.store.data[index].lhs, self.store.data[index].rhs
            )
            .expect("writing to String cannot fail");
        }
        Ok(output)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrBuildError {
    pub construct: &'static str,
    pub location: Option<IrLocation>,
    pub attempted_instructions: usize,
    pub committed_instructions: usize,
}

impl IrBuildError {
    fn format(
        construct: &'static str,
        span: Option<Span>,
        attempted_instructions: usize,
        committed_instructions: usize,
    ) -> Self {
        Self {
            construct,
            location: span.and_then(|span| IrLocation::from_span(span).ok()),
            attempted_instructions,
            committed_instructions,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrVerifyError {
    pub message: String,
}

impl IrVerifyError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct IrCheckpoint {
    instructions: usize,
    extra: usize,
    blocks: usize,
    params: usize,
    captures: usize,
    patterns: usize,
    strings: usize,
    string_bytes: usize,
    bytes: usize,
    byte_data: usize,
    locations: usize,
}

#[derive(Default)]
struct IrBuilder {
    store: IrStore,
    strings: BTreeMap<String, IrStringId>,
    locations: BTreeMap<(u32, u32), IrLocationId>,
    functions: BTreeMap<LoweredFunctionKey, IrFunctionId>,
    current_function: Option<IrFunctionId>,
}

impl IrBuilder {
    fn new(source_id: SourceId) -> Self {
        Self {
            store: IrStore {
                source_id,
                ..IrStore::default()
            },
            strings: BTreeMap::new(),
            locations: BTreeMap::new(),
            functions: BTreeMap::new(),
            current_function: None,
        }
    }

    fn build_from_units(
        units: &[LoweredFunctionUnit],
        sources: Arc<SourceMap>,
        source_id: SourceId,
    ) -> Result<IrProgram, IrBuildError> {
        let mut builder = Self::new(source_id);
        let mut ordered = units.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|unit| (unit.source_span().start(), unit.key().display_name()));
        builder.predeclare_functions(&ordered)?;
        for unit in ordered {
            let Some(body) = unit.lowered_body() else {
                return Err(IrBuildError::format(
                    "lowered_function_blocker",
                    Some(unit.source_span()),
                    0,
                    0,
                ));
            };
            let function_id = builder.functions[&unit.key()];
            builder.build_function_transaction(function_id, &body)?;
        }
        builder
            .finish(sources)
            .map_err(|_| IrBuildError::format("verification_failed", None, 0, 0))
    }

    fn estimate_supported_units(
        units: &[LoweredFunctionUnit],
        source_id: SourceId,
    ) -> Result<IrStorageEstimate, IrBuildError> {
        let mut builder = Self::new(source_id);
        let mut ordered = units.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|unit| (unit.source_span().start(), unit.key().display_name()));
        builder.predeclare_functions(&ordered)?;
        let mut estimate = IrStorageEstimate {
            functions_seen: ordered.len(),
            ..IrStorageEstimate::default()
        };
        for unit in ordered {
            let Some(body) = unit.lowered_body() else {
                continue;
            };
            let function_id = builder.functions[&unit.key()];
            if builder
                .build_function_transaction(function_id, &body)
                .is_ok()
            {
                estimate.functions_built += 1;
            }
        }
        estimate.instructions = builder.store.tags.len();
        estimate.extra_words = builder.store.extra.len();
        Ok(estimate)
    }

    fn predeclare_functions(
        &mut self,
        units: &[&LoweredFunctionUnit],
    ) -> Result<(), IrBuildError> {
        for unit in units {
            let function_id = IrFunctionId::new(self.store.functions.len())?;
            let name = match unit.key() {
                LoweredFunctionKey::Name(name) => self.intern_string(name.as_str())?,
                LoweredFunctionKey::Qualified(_) => {
                    return Err(IrBuildError::format(
                        "qualified_function",
                        Some(unit.source_span()),
                        0,
                        0,
                    ));
                }
            };
            self.functions.insert(unit.key(), function_id);
            self.store.functions.push(IrFunction {
                name: name.raw(),
                params: IrRange::EMPTY,
                captures: IrRange::EMPTY,
                body: IrOptionalId::NONE,
                slot_count: 0,
                kind: unit.kind().into(),
                return_kind: IrReturnKind::PlainUnit,
                reserved: [0; 2],
            });
        }
        Ok(())
    }

    fn build_function_transaction(
        &mut self,
        function_id: IrFunctionId,
        function: &LoweredPureFunction,
    ) -> Result<(), IrBuildError> {
        let checkpoint = self.checkpoint();
        self.current_function = Some(function_id);
        let result = self.build_function(function_id, function);
        self.current_function = None;
        if let Err(mut error) = result {
            error.attempted_instructions = self.store.tags.len() - checkpoint.instructions;
            self.rewind(checkpoint);
            error.committed_instructions = self.store.tags.len() - checkpoint.instructions;
            return Err(error);
        }
        Ok(())
    }

    fn build_function(
        &mut self,
        function_id: IrFunctionId,
        function: &LoweredPureFunction,
    ) -> Result<(), IrBuildError> {
        if function.has_defers {
            return Err(IrBuildError::format("function_defers", None, 0, 0));
        }
        let params_start = self.store.params.len();
        for (slot, ((name, kind), rest)) in function
            .params
            .iter()
            .zip(function.param_kinds.iter())
            .zip(function.param_rest.iter())
            .enumerate()
        {
            if *rest || function.param_defaults.get(slot).is_some_and(Option::is_some) {
                return Err(IrBuildError::format("parameter_shape", None, 0, 0));
            }
            let ty = IrValueType::from_lowered(*kind)
                .ok_or_else(|| IrBuildError::format("parameter_type", None, 0, 0))?;
            let name = self.intern_string(name.as_str())?.raw();
            self.store.params.push(IrParam {
                name,
                slot: u32::try_from(slot)
                    .map_err(|_| IrBuildError::format("slot_overflow", None, 0, 0))?,
                ty,
                flags: 0,
                reserved: [0; 2],
            });
        }
        let captures_start = self.store.captures.len();
        for capture in &function.captures {
            let ty = IrValueType::from_lowered(capture.kind)
                .ok_or_else(|| IrBuildError::format("capture_type", None, 0, 0))?;
            let name = self.intern_string(capture.name.as_str())?.raw();
            self.store.captures.push(IrCapture {
                name,
                slot: u32::try_from(capture.slot)
                    .map_err(|_| IrBuildError::format("slot_overflow", None, 0, 0))?,
                ty,
                mutable: u8::from(capture.mutable),
                reserved: [0; 2],
            });
        }
        let body = self.emit_block(&function.body)?;
        let params = Self::table_range(params_start, self.store.params.len())?;
        let captures = Self::table_range(captures_start, self.store.captures.len())?;
        let slot_count = u32::try_from(function.slot_count)
            .map_err(|_| IrBuildError::format("slot_overflow", None, 0, 0))?;
        let return_kind = Self::return_kind(function.return_kind)?;
        let stored = &mut self.store.functions[function_id.index()];
        stored.params = params;
        stored.captures = captures;
        stored.body = IrOptionalId::some(body.raw());
        stored.slot_count = slot_count;
        stored.return_kind = return_kind;
        Ok(())
    }

    fn table_range(start: usize, end: usize) -> Result<IrRange, IrBuildError> {
        Ok(IrRange::new(
            u32::try_from(start)
                .map_err(|_| IrBuildError::format("table_overflow", None, 0, 0))?,
            u32::try_from(end.saturating_sub(start))
                .map_err(|_| IrBuildError::format("table_overflow", None, 0, 0))?,
        ))
    }

    fn return_kind(kind: LoweredReturnKind) -> Result<IrReturnKind, IrBuildError> {
        match kind {
            LoweredReturnKind::Plain(LoweredType::Unit) => Ok(IrReturnKind::PlainUnit),
            LoweredReturnKind::Plain(LoweredType::Int) => Ok(IrReturnKind::PlainInt),
            LoweredReturnKind::Plain(LoweredType::Bool) => Ok(IrReturnKind::PlainBool),
            LoweredReturnKind::Result(LoweredType::Unit) => Ok(IrReturnKind::ResultUnit),
            LoweredReturnKind::Result(LoweredType::Int) => Ok(IrReturnKind::ResultInt),
            _ => Err(IrBuildError::format("return_type", None, 0, 0)),
        }
    }

    fn emit_block(&mut self, statements: &[LoweredStmt]) -> Result<IrBlockId, IrBuildError> {
        let owner = self
            .current_function
            .ok_or_else(|| IrBuildError::format("missing_function_owner", None, 0, 0))?;
        let mut instructions = Vec::with_capacity(statements.len());
        for statement in statements {
            instructions.push(self.emit_statement(statement)?.raw());
        }
        let range = self.push_extra(&instructions)?;
        let id = IrBlockId::new(self.store.blocks.len())?;
        self.store.blocks.push(IrBlock {
            instructions: range,
            owner: owner.raw(),
        });
        Ok(id)
    }

    fn emit_statement(&mut self, statement: &LoweredStmt) -> Result<IrInstId, IrBuildError> {
        match statement {
            LoweredStmt::Let { slot, value } => {
                let value = self.emit_expression(value)?;
                self.push_instruction(
                    IrTag::Let,
                    IrData::new(Self::slot(*slot)?, value.raw()),
                    None,
                )
            }
            LoweredStmt::LetInt { slot, value } => {
                let value = self.emit_int_expression(value)?;
                self.push_instruction(
                    IrTag::Let,
                    IrData::new(Self::slot(*slot)?, value.raw()),
                    None,
                )
            }
            LoweredStmt::Assign {
                slot,
                op: AssignOp::Add,
                value,
                span,
            } => {
                let value = self.emit_expression(value)?;
                self.push_instruction(
                    IrTag::AssignAdd,
                    IrData::new(Self::slot(*slot)?, value.raw()),
                    Some(*span),
                )
            }
            LoweredStmt::If {
                branches,
                else_body,
            } => {
                let mut lowered = Vec::with_capacity(branches.len());
                for (condition, body) in branches {
                    lowered.push((self.emit_expression(condition)?, self.emit_block(body)?));
                }
                let else_body = match else_body {
                    Some(body) => Some(self.emit_block(body)?),
                    None => None,
                };
                self.emit_if(lowered, else_body)
            }
            LoweredStmt::IfBool {
                branches,
                else_body,
            } => {
                let mut lowered = Vec::with_capacity(branches.len());
                for (condition, body) in branches {
                    lowered.push((self.emit_bool_expression(condition)?, self.emit_block(body)?));
                }
                let else_body = match else_body {
                    Some(body) => Some(self.emit_block(body)?),
                    None => None,
                };
                self.emit_if(lowered, else_body)
            }
            LoweredStmt::Loop { body } => {
                let body = self.emit_block(body)?;
                self.push_instruction(IrTag::Loop, IrData::new(body.raw(), 0), None)
            }
            LoweredStmt::Guard {
                slot,
                value,
                else_param_slot,
                else_body,
                span,
            } => {
                let value = self.emit_expression(value)?;
                let else_body = self.emit_block(else_body)?;
                let mut payload = IrExtraWriter::with_capacity(4);
                payload.slot(*slot)?;
                payload.id(value);
                payload.optional_slot(*else_param_slot)?;
                payload.id(else_body);
                let payload = payload.finish(self)?;
                self.push_instruction(IrTag::Guard, IrData::from_range(payload), Some(*span))
            }
            LoweredStmt::Print {
                args,
                stderr,
                flush,
                propagate_result,
                span,
            } if !stderr && !flush && !propagate_result => {
                let args = args
                    .iter()
                    .map(|arg| self.emit_expression(arg).map(IrInstId::raw))
                    .collect::<Result<Vec<_>, _>>()?;
                let payload = self.push_extra(&args)?;
                self.push_instruction(IrTag::Print, IrData::from_range(payload), Some(*span))
            }
            LoweredStmt::Return { value } => {
                let value = self.emit_expression(value)?;
                self.push_instruction(IrTag::Return, IrData::new(value.raw(), 0), None)
            }
            LoweredStmt::Break => self.push_instruction(IrTag::Break, IrData::ZERO, None),
            LoweredStmt::Continue => self.push_instruction(IrTag::Continue, IrData::ZERO, None),
            _ => Err(IrBuildError::format(
                "unsupported_statement",
                Self::statement_span(statement),
                0,
                0,
            )),
        }
    }

    fn emit_if(
        &mut self,
        branches: Vec<(IrInstId, IrBlockId)>,
        else_body: Option<IrBlockId>,
    ) -> Result<IrInstId, IrBuildError> {
        let mut payload = IrExtraWriter::with_capacity(2 + branches.len() * 2);
        payload.count(branches.len())?;
        for (condition, body) in branches {
            payload.id(condition);
            payload.id(body);
        }
        payload.optional_id(else_body);
        let payload = payload.finish(self)?;
        self.push_instruction(IrTag::If, IrData::from_range(payload), None)
    }

    fn emit_expression(&mut self, expression: &LoweredExpr) -> Result<IrInstId, IrBuildError> {
        match expression {
            LoweredExpr::Unit => self.push_instruction(IrTag::Unit, IrData::ZERO, None),
            LoweredExpr::Int(value) => self.emit_int(*value),
            LoweredExpr::Bool(value) => self.push_instruction(
                IrTag::Bool,
                IrData::new(u32::from(*value), 0),
                None,
            ),
            LoweredExpr::Str(value) => {
                let value = self.intern_string(value)?.raw();
                self.push_instruction(IrTag::Str, IrData::new(value, 0), None)
            }
            LoweredExpr::Bytes(value) => {
                let value = self.push_bytes(value)?.raw();
                self.push_instruction(IrTag::Bytes, IrData::new(value, 0), None)
            }
            LoweredExpr::Param(slot) => self.push_instruction(
                IrTag::Slot,
                IrData::new(Self::slot(*slot)?, 0),
                None,
            ),
            LoweredExpr::Binary {
                op,
                left,
                right,
                span,
            } => {
                let left = self.emit_expression(left)?;
                let right = self.emit_expression(right)?;
                let tag = Self::binary_tag(*op)?;
                self.push_instruction(tag, IrData::new(left.raw(), right.raw()), Some(*span))
            }
            LoweredExpr::List(values) => {
                let values = values
                    .iter()
                    .map(|value| self.emit_expression(value).map(IrInstId::raw))
                    .collect::<Result<Vec<_>, _>>()?;
                let payload = self.push_extra(&values)?;
                self.push_instruction(IrTag::List, IrData::from_range(payload), None)
            }
            LoweredExpr::ListPipeline {
                input,
                stages,
                span,
            } => {
                let [LoweredPipelineStage::Map { slot, value }, LoweredPipelineStage::Collect] =
                    stages.as_slice()
                else {
                    return Err(IrBuildError::format(
                        "pipeline_shape",
                        Some(*span),
                        0,
                        0,
                    ));
                };
                let input = self.emit_expression(input)?;
                let value = self.emit_expression(value)?;
                let payload =
                    self.push_extra(&[input.raw(), Self::slot(*slot)?, value.raw()])?;
                self.push_instruction(IrTag::ListMap, IrData::from_range(payload), Some(*span))
            }
            LoweredExpr::Index { base, index, span } => {
                let base = self.emit_expression(base)?;
                let index = self.emit_expression(index)?;
                self.push_instruction(
                    IrTag::Index,
                    IrData::new(base.raw(), index.raw()),
                    Some(*span),
                )
            }
            LoweredExpr::Field { base, name, span } => {
                let base = self.emit_expression(base)?;
                let name = self.intern_string(name)?;
                self.push_instruction(
                    IrTag::Field,
                    IrData::new(base.raw(), name.raw()),
                    Some(*span),
                )
            }
            LoweredExpr::Call {
                function,
                args,
                span,
            } => self.emit_call(*function, args, *span, false),
            LoweredExpr::SelfCall { args, span } => {
                let function = self.current_function.ok_or_else(|| {
                    IrBuildError::format("missing_self_function", Some(*span), 0, 0)
                })?;
                self.emit_call_id(function, args, *span, true)
            }
            LoweredExpr::Ok(value) => {
                let value = self.emit_expression(value)?;
                self.push_instruction(IrTag::Ok, IrData::new(value.raw(), 0), None)
            }
            LoweredExpr::Err(value) => {
                let value = self.emit_expression(value)?;
                self.push_instruction(IrTag::Err, IrData::new(value.raw(), 0), None)
            }
            LoweredExpr::Error(error) => self.emit_error(error),
            LoweredExpr::Try(value) => {
                let value = self.emit_expression(value)?;
                self.push_instruction(IrTag::Try, IrData::new(value.raw(), 0), None)
            }
            LoweredExpr::Record(fields) => self.emit_record(fields),
            LoweredExpr::Require { value, check, span } => {
                self.emit_record_requirement(value, &check.ty, &check.name, *span)
            }
            LoweredExpr::MatchExpr {
                value,
                arms,
                span,
            } => self.emit_match(value, arms, *span),
            LoweredExpr::ModuleCall {
                op: RuntimeOp::BytesUnpackBe,
                args,
                span,
            } if args.len() == 3 => {
                let args = args
                    .iter()
                    .map(|arg| self.emit_expression(arg).map(IrInstId::raw))
                    .collect::<Result<Vec<_>, _>>()?;
                let payload = self.push_extra(&args)?;
                self.push_instruction(
                    IrTag::BytesUnpackBe,
                    IrData::from_range(payload),
                    Some(*span),
                )
            }
            _ => Err(IrBuildError::format(
                "unsupported_expression",
                Self::expression_span(expression),
                0,
                0,
            )),
        }
    }

    fn emit_int_expression(
        &mut self,
        expression: &LoweredIntExpr,
    ) -> Result<IrInstId, IrBuildError> {
        match expression {
            LoweredIntExpr::Int(value) => self.emit_int(*value),
            LoweredIntExpr::Slot(slot) => self.push_instruction(
                IrTag::Slot,
                IrData::new(Self::slot(*slot)?, 0),
                None,
            ),
            LoweredIntExpr::Binary { op, left, right } => {
                let left = self.emit_int_expression(left)?;
                let right = self.emit_int_expression(right)?;
                self.push_instruction(
                    Self::binary_tag(*op)?,
                    IrData::new(left.raw(), right.raw()),
                    None,
                )
            }
            _ => Err(IrBuildError::format("integer_expression", None, 0, 0)),
        }
    }

    fn emit_bool_expression(
        &mut self,
        expression: &LoweredBoolExpr,
    ) -> Result<IrInstId, IrBuildError> {
        match expression {
            LoweredBoolExpr::Bool(value) => self.push_instruction(
                IrTag::Bool,
                IrData::new(u32::from(*value), 0),
                None,
            ),
            LoweredBoolExpr::Slot(slot) => self.push_instruction(
                IrTag::Slot,
                IrData::new(Self::slot(*slot)?, 0),
                None,
            ),
            LoweredBoolExpr::IntCompare { op, left, right } => {
                let left = self.emit_int_expression(left)?;
                let right = self.emit_int_expression(right)?;
                self.push_instruction(
                    Self::binary_tag(*op)?,
                    IrData::new(left.raw(), right.raw()),
                    None,
                )
            }
            LoweredBoolExpr::LiteralCompareSlot { op, slot, value } => {
                let left = self.push_instruction(
                    IrTag::Slot,
                    IrData::new(Self::slot(*slot)?, 0),
                    None,
                )?;
                let right = self.emit_lowered_value(value)?;
                self.push_instruction(
                    Self::binary_tag(*op)?,
                    IrData::new(left.raw(), right.raw()),
                    None,
                )
            }
            _ => Err(IrBuildError::format("boolean_expression", None, 0, 0)),
        }
    }

    fn emit_lowered_value(&mut self, value: &LoweredValue) -> Result<IrInstId, IrBuildError> {
        match value {
            LoweredValue::Unit => self.push_instruction(IrTag::Unit, IrData::ZERO, None),
            LoweredValue::Int(value) => self.emit_int(*value),
            LoweredValue::Bool(value) => self.push_instruction(
                IrTag::Bool,
                IrData::new(u32::from(*value), 0),
                None,
            ),
            LoweredValue::Str(value) => {
                let value = self.intern_string(value)?.raw();
                self.push_instruction(IrTag::Str, IrData::new(value, 0), None)
            }
            _ => Err(IrBuildError::format("lowered_literal", None, 0, 0)),
        }
    }

    fn emit_int(&mut self, value: i64) -> Result<IrInstId, IrBuildError> {
        let bits = value as u64;
        self.push_instruction(
            IrTag::Int,
            IrData::new(bits as u32, (bits >> 32) as u32),
            None,
        )
    }

    fn emit_call(
        &mut self,
        key: LoweredFunctionKey,
        args: &[LoweredCallArg],
        span: Span,
        self_call: bool,
    ) -> Result<IrInstId, IrBuildError> {
        let function = self.functions.get(&key).copied().ok_or_else(|| {
            IrBuildError::format("unknown_function", Some(span), 0, 0)
        })?;
        self.emit_call_id(function, args, span, self_call)
    }

    fn emit_call_id(
        &mut self,
        function: IrFunctionId,
        args: &[LoweredCallArg],
        span: Span,
        self_call: bool,
    ) -> Result<IrInstId, IrBuildError> {
        let mut words = IrExtraWriter::with_capacity(args.len() + 1);
        words.id(function);
        for arg in args {
            let LoweredCallArg::Single(arg) = arg else {
                return Err(IrBuildError::format("spliced_call", Some(span), 0, 0));
            };
            words.id(self.emit_expression(arg)?);
        }
        let payload = words.finish(self)?;
        self.push_instruction(
            if self_call {
                IrTag::SelfCall
            } else {
                IrTag::Call
            },
            IrData::from_range(payload),
            Some(span),
        )
    }

    fn emit_error(&mut self, error: &LoweredErrorExpr) -> Result<IrInstId, IrBuildError> {
        let LoweredErrorExpr::Structured {
            family,
            variant,
            fields,
            facets,
        } = error
        else {
            return Err(IrBuildError::format("simple_error", None, 0, 0));
        };
        if !facets.is_empty() {
            return Err(IrBuildError::format("error_facets", None, 0, 0));
        }
        let mut words = Vec::with_capacity(3 + fields.len() * 2);
        words.push(self.intern_string(family)?.raw());
        words.push(self.intern_string(variant)?.raw());
        words.push(u32::try_from(fields.len()).map_err(|_| {
            IrBuildError::format("field_overflow", None, 0, 0)
        })?);
        for (name, value) in fields {
            words.push(self.intern_string(name)?.raw());
            words.push(self.emit_expression(value)?.raw());
        }
        let payload = self.push_extra(&words)?;
        self.push_instruction(IrTag::Error, IrData::from_range(payload), None)
    }

    fn emit_record(&mut self, fields: &[LoweredRecordEntry]) -> Result<IrInstId, IrBuildError> {
        let mut words = Vec::with_capacity(fields.len() * 2);
        for field in fields {
            let LoweredRecordEntry::Field(name, value) = field else {
                return Err(IrBuildError::format("record_spread", None, 0, 0));
            };
            words.push(self.intern_string(name.as_str())?.raw());
            words.push(self.emit_expression(value)?.raw());
        }
        let payload = self.push_extra(&words)?;
        self.push_instruction(IrTag::Record, IrData::from_range(payload), None)
    }

    fn emit_record_requirement(
        &mut self,
        value: &LoweredExpr,
        ty: &Type,
        name: &str,
        span: Span,
    ) -> Result<IrInstId, IrBuildError> {
        let Type::Record(fields) = ty else {
            return Err(IrBuildError::format("require_type", Some(span), 0, 0));
        };
        let mut words = Vec::with_capacity(2 + fields.len() * 2);
        words.push(self.emit_expression(value)?.raw());
        words.push(self.intern_string(name)?.raw());
        for (field, ty) in fields {
            let ty = match ty {
                Type::Int => IrValueType::Int,
                Type::Str => IrValueType::Str,
                _ => {
                    return Err(IrBuildError::format(
                        "require_field_type",
                        Some(span),
                        0,
                        0,
                    ));
                }
            };
            words.push(self.intern_string(field.as_str())?.raw());
            words.push(ty as u32);
        }
        let payload = self.push_extra(&words)?;
        self.push_instruction(
            IrTag::RequireRecord,
            IrData::from_range(payload),
            Some(span),
        )
    }

    fn emit_match(
        &mut self,
        value: &LoweredExpr,
        arms: &[(LoweredPattern, Option<LoweredExpr>, LoweredExpr)],
        span: Span,
    ) -> Result<IrInstId, IrBuildError> {
        let mut words = Vec::with_capacity(2 + arms.len() * 3);
        words.push(self.emit_expression(value)?.raw());
        words.push(u32::try_from(arms.len()).map_err(|_| {
            IrBuildError::format("match_arm_overflow", Some(span), 0, 0)
        })?);
        for (pattern, guard, value) in arms {
            words.push(self.emit_pattern(pattern)?.raw());
            words.push(
                guard
                    .as_ref()
                    .map(|guard| self.emit_expression(guard).map(IrInstId::raw))
                    .transpose()?
                    .unwrap_or(IR_NONE),
            );
            words.push(self.emit_expression(value)?.raw());
        }
        let payload = self.push_extra(&words)?;
        self.push_instruction(IrTag::Match, IrData::from_range(payload), Some(span))
    }

    fn emit_pattern(&mut self, pattern: &LoweredPattern) -> Result<IrPatternId, IrBuildError> {
        let id = IrPatternId::new(self.store.pattern_tags.len())?;
        match pattern {
            LoweredPattern::Wildcard => {
                self.store.pattern_tags.push(IrPatternTag::Wildcard);
                self.store.pattern_data.push(IrData::ZERO);
            }
            LoweredPattern::Bind { slot } => {
                self.store.pattern_tags.push(IrPatternTag::Bind);
                self.store
                    .pattern_data
                    .push(IrData::new(Self::slot(*slot)?, 0));
            }
            _ => return Err(IrBuildError::format("pattern", None, 0, 0)),
        }
        Ok(id)
    }

    fn binary_tag(op: BinaryOp) -> Result<IrTag, IrBuildError> {
        Ok(match op {
            BinaryOp::Add => IrTag::Add,
            BinaryOp::Sub => IrTag::Sub,
            BinaryOp::Mul => IrTag::Mul,
            BinaryOp::Eq => IrTag::Eq,
            BinaryOp::Lt => IrTag::Lt,
            BinaryOp::Le => IrTag::Le,
            BinaryOp::Gt => IrTag::Gt,
            BinaryOp::Ge => IrTag::Ge,
            _ => return Err(IrBuildError::format("binary_operator", None, 0, 0)),
        })
    }

    fn slot(slot: usize) -> Result<u32, IrBuildError> {
        u32::try_from(slot).map_err(|_| IrBuildError::format("slot_overflow", None, 0, 0))
    }

    fn statement_span(statement: &LoweredStmt) -> Option<Span> {
        match statement {
            LoweredStmt::Assign { span, .. }
            | LoweredStmt::Guard { span, .. }
            | LoweredStmt::Print { span, .. } => Some(*span),
            _ => None,
        }
    }

    fn expression_span(expression: &LoweredExpr) -> Option<Span> {
        match expression {
            LoweredExpr::Binary { span, .. }
            | LoweredExpr::ListPipeline { span, .. }
            | LoweredExpr::Field { span, .. }
            | LoweredExpr::Index { span, .. }
            | LoweredExpr::Call { span, .. }
            | LoweredExpr::SelfCall { span, .. }
            | LoweredExpr::Require { span, .. }
            | LoweredExpr::MatchExpr { span, .. }
            | LoweredExpr::ModuleCall { span, .. } => Some(*span),
            _ => None,
        }
    }

    fn checkpoint(&self) -> IrCheckpoint {
        IrCheckpoint {
            instructions: self.store.tags.len(),
            extra: self.store.extra.len(),
            blocks: self.store.blocks.len(),
            params: self.store.params.len(),
            captures: self.store.captures.len(),
            patterns: self.store.pattern_tags.len(),
            strings: self.store.strings.len(),
            string_bytes: self.store.string_bytes.len(),
            bytes: self.store.bytes.len(),
            byte_data: self.store.byte_data.len(),
            locations: self.store.locations.len(),
        }
    }

    fn rewind(&mut self, checkpoint: IrCheckpoint) {
        self.store.tags.truncate(checkpoint.instructions);
        self.store.data.truncate(checkpoint.instructions);
        self.store
            .instruction_locations
            .truncate(checkpoint.instructions);
        self.store.extra.truncate(checkpoint.extra);
        self.store.blocks.truncate(checkpoint.blocks);
        self.store.params.truncate(checkpoint.params);
        self.store.captures.truncate(checkpoint.captures);
        self.store.pattern_tags.truncate(checkpoint.patterns);
        self.store.pattern_data.truncate(checkpoint.patterns);
        self.store.strings.truncate(checkpoint.strings);
        self.store.string_bytes.truncate(checkpoint.string_bytes);
        self.store.bytes.truncate(checkpoint.bytes);
        self.store.byte_data.truncate(checkpoint.byte_data);
        self.store.locations.truncate(checkpoint.locations);
        self.strings.retain(|_, id| id.index() < checkpoint.strings);
        self.locations
            .retain(|_, id| id.index() < checkpoint.locations);
    }

    fn push_instruction(
        &mut self,
        tag: IrTag,
        data: IrData,
        span: Option<Span>,
    ) -> Result<IrInstId, IrBuildError> {
        let id = IrInstId::new(self.store.tags.len())?;
        let location = match span {
            Some(span) => IrOptionalId::some(self.intern_location(span)?.raw()),
            None => IrOptionalId::NONE,
        };
        self.store.tags.push(tag);
        self.store.data.push(data);
        self.store.instruction_locations.push(location);
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

    fn push_bytes(&mut self, value: &[u8]) -> Result<IrBytesId, IrBuildError> {
        let id = IrBytesId::new(self.store.bytes.len())?;
        let start = u32::try_from(self.store.byte_data.len())
            .map_err(|_| IrBuildError::format("bytes_overflow", None, 0, 0))?;
        let len = u32::try_from(value.len())
            .map_err(|_| IrBuildError::format("bytes_overflow", None, 0, 0))?;
        self.store.byte_data.extend_from_slice(value);
        self.store.bytes.push(IrRange::new(start, len));
        Ok(id)
    }

    fn intern_location(&mut self, span: Span) -> Result<IrLocationId, IrBuildError> {
        if span.source_id != self.store.source_id {
            return Err(IrBuildError::format("cross_source_location", Some(span), 0, 0));
        }
        let location = IrLocation::from_span(span)?;
        let key = (location.start, location.len);
        if let Some(id) = self.locations.get(&key) {
            return Ok(*id);
        }
        let id = IrLocationId::new(self.store.locations.len())?;
        self.store.locations.push(location);
        self.locations.insert(key, id);
        Ok(id)
    }

    fn finish(mut self, sources: Arc<SourceMap>) -> Result<IrProgram, IrVerifyError> {
        self.store.shrink_to_fit();
        let program = IrProgram {
            store: self.store,
            sources,
        };
        IrVerifier::verify(&program)?;
        Ok(program)
    }
}

struct IrVerifier;

impl IrVerifier {
    fn verify(program: &IrProgram) -> Result<(), IrVerifyError> {
        let store = &program.store;
        if store.tags.len() != store.data.len()
            || store.tags.len() != store.instruction_locations.len()
        {
            return Err(IrVerifyError::new(
                "instruction tag, data, and location columns have different lengths",
            ));
        }
        if store.pattern_tags.len() != store.pattern_data.len() {
            return Err(IrVerifyError::new(
                "pattern tag and data columns have different lengths",
            ));
        }
        let source = program
            .sources
            .get(store.source_id)
            .ok_or_else(|| IrVerifyError::new("program source id is missing"))?;
        for location in &store.locations {
            let end = (location.start as usize)
                .checked_add(location.len as usize)
                .ok_or_else(|| IrVerifyError::new("source location overflows"))?;
            if end > source.len() {
                return Err(IrVerifyError::new("source location is out of bounds"));
            }
        }
        for range in &store.strings {
            let bounds = range
                .bounds(store.string_bytes.len())
                .ok_or_else(|| IrVerifyError::new("string range is out of bounds"))?;
            std::str::from_utf8(&store.string_bytes[bounds])
                .map_err(|_| IrVerifyError::new("string blob contains invalid UTF-8"))?;
        }
        for range in &store.bytes {
            range
                .bounds(store.byte_data.len())
                .ok_or_else(|| IrVerifyError::new("bytes range is out of bounds"))?;
        }
        for location in &store.instruction_locations {
            store.location(*location)?;
        }
        for block in &store.blocks {
            if IrFunctionId::from_raw(block.owner)
                .is_none_or(|id| id.index() >= store.functions.len())
            {
                return Err(IrVerifyError::new("block owner is out of bounds"));
            }
            let instructions = store.payload(block.instructions)?;
            for raw in instructions {
                let id = IrInstId::from_raw(*raw)
                    .ok_or_else(|| IrVerifyError::new("block instruction id is invalid"))?;
                if id.index() >= store.tags.len() || !store.tags[id.index()].is_statement() {
                    return Err(IrVerifyError::new(
                        "block contains an invalid or non-statement instruction",
                    ));
                }
            }
        }
        for param in &store.params {
            Self::verify_string(store, param.name)?;
        }
        for capture in &store.captures {
            Self::verify_string(store, capture.name)?;
        }
        for function in &store.functions {
            Self::verify_string(store, function.name)?;
            function
                .params
                .bounds(store.params.len())
                .ok_or_else(|| IrVerifyError::new("function parameter range is out of bounds"))?;
            function.captures.bounds(store.captures.len()).ok_or_else(|| {
                IrVerifyError::new("function capture range is out of bounds")
            })?;
            let body = function
                .body
                .raw()
                .and_then(IrBlockId::from_raw)
                .ok_or_else(|| IrVerifyError::new("function body is absent or invalid"))?;
            if body.index() >= store.blocks.len() {
                return Err(IrVerifyError::new("function body is out of bounds"));
            }
            for param in &store.params[function.params.bounds(store.params.len()).unwrap()] {
                if param.slot >= function.slot_count {
                    return Err(IrVerifyError::new("parameter slot is out of bounds"));
                }
            }
            for capture in &store.captures[function.captures.bounds(store.captures.len()).unwrap()] {
                if capture.slot >= function.slot_count {
                    return Err(IrVerifyError::new("capture slot is out of bounds"));
                }
            }
        }

        let mut owners = vec![None; store.tags.len()];
        let mut states = vec![0u8; store.tags.len()];
        for (index, function) in store.functions.iter().enumerate() {
            let function_id = IrFunctionId::new(index)
                .map_err(|_| IrVerifyError::new("function id overflow"))?;
            let body = IrBlockId::from_raw(function.body.raw().unwrap()).unwrap();
            Self::verify_block(program, function_id, body, &mut owners, &mut states)?;
        }
        if owners.iter().any(Option::is_none) {
            return Err(IrVerifyError::new(
                "store contains instructions not owned by a function",
            ));
        }
        Ok(())
    }

    fn verify_string(store: &IrStore, raw: u32) -> Result<(), IrVerifyError> {
        let id = IrStringId::from_raw(raw)
            .ok_or_else(|| IrVerifyError::new("string id is invalid"))?;
        store.string(id).map(|_| ())
    }

    fn verify_block(
        program: &IrProgram,
        function_id: IrFunctionId,
        block_id: IrBlockId,
        owners: &mut [Option<IrFunctionId>],
        states: &mut [u8],
    ) -> Result<(), IrVerifyError> {
        let store = &program.store;
        let block = store
            .blocks
            .get(block_id.index())
            .ok_or_else(|| IrVerifyError::new("block id is out of bounds"))?;
        if block.owner != function_id.raw() {
            return Err(IrVerifyError::new("block belongs to a different function"));
        }
        for raw in store.payload(block.instructions)? {
            let instruction = IrInstId::from_raw(*raw)
                .ok_or_else(|| IrVerifyError::new("block instruction id is invalid"))?;
            Self::verify_instruction(program, function_id, instruction, owners, states)?;
        }
        Ok(())
    }

    fn verify_instruction(
        program: &IrProgram,
        function_id: IrFunctionId,
        id: IrInstId,
        owners: &mut [Option<IrFunctionId>],
        states: &mut [u8],
    ) -> Result<(), IrVerifyError> {
        let store = &program.store;
        let index = id.index();
        let tag = *store
            .tags
            .get(index)
            .ok_or_else(|| IrVerifyError::new("instruction id is out of bounds"))?;
        if let Some(owner) = owners[index] {
            if owner != function_id {
                return Err(IrVerifyError::new(
                    "instruction is shared by different functions",
                ));
            }
            if states[index] == 1 {
                return Err(IrVerifyError::new("instruction graph contains a cycle"));
            }
            return Ok(());
        }
        owners[index] = Some(function_id);
        states[index] = 1;
        let data = store.data[index];
        let function = &store.functions[function_id.index()];
        let verify_inst = |raw: u32,
                           owners: &mut [Option<IrFunctionId>],
                           states: &mut [u8]|
         -> Result<(), IrVerifyError> {
            let child = IrInstId::from_raw(raw)
                .ok_or_else(|| IrVerifyError::new("child instruction id is invalid"))?;
            Self::verify_instruction(program, function_id, child, owners, states)
        };
        let verify_block = |raw: u32,
                            owners: &mut [Option<IrFunctionId>],
                            states: &mut [u8]|
         -> Result<(), IrVerifyError> {
            let block = IrBlockId::from_raw(raw)
                .ok_or_else(|| IrVerifyError::new("nested block id is invalid"))?;
            Self::verify_block(program, function_id, block, owners, states)
        };

        match tag {
            IrTag::Unit | IrTag::Int | IrTag::Bool | IrTag::Break | IrTag::Continue => {}
            IrTag::Str => Self::verify_string(store, data.lhs)?,
            IrTag::Field => {
                verify_inst(data.lhs, owners, states)?;
                Self::verify_string(store, data.rhs)?;
            }
            IrTag::Bytes => {
                let bytes = IrBytesId::from_raw(data.lhs)
                    .ok_or_else(|| IrVerifyError::new("bytes id is invalid"))?;
                store.bytes(bytes)?;
            }
            IrTag::Slot => {
                if data.lhs >= function.slot_count {
                    return Err(IrVerifyError::new("slot is out of bounds"));
                }
            }
            IrTag::Add
            | IrTag::Sub
            | IrTag::Mul
            | IrTag::Eq
            | IrTag::Lt
            | IrTag::Le
            | IrTag::Gt
            | IrTag::Ge
            | IrTag::Index => {
                verify_inst(data.lhs, owners, states)?;
                verify_inst(data.rhs, owners, states)?;
            }
            IrTag::List | IrTag::Print => {
                for raw in store.payload(data.range())? {
                    verify_inst(*raw, owners, states)?;
                }
            }
            IrTag::ListMap => {
                let payload = Self::payload_len(store, data.range(), 3, "list-map")?;
                verify_inst(payload[0], owners, states)?;
                if payload[1] >= function.slot_count {
                    return Err(IrVerifyError::new("list-map slot is out of bounds"));
                }
                verify_inst(payload[2], owners, states)?;
            }
            IrTag::Call | IrTag::SelfCall => {
                let payload = store.payload(data.range())?;
                if payload.is_empty() {
                    return Err(IrVerifyError::new("call payload is empty"));
                }
                let target = IrFunctionId::from_raw(payload[0])
                    .ok_or_else(|| IrVerifyError::new("call target is invalid"))?;
                if target.index() >= store.functions.len() {
                    return Err(IrVerifyError::new("call target is out of bounds"));
                }
                let target_params = store.functions[target.index()]
                    .params
                    .bounds(store.params.len())
                    .ok_or_else(|| IrVerifyError::new("call target params are invalid"))?;
                if payload.len() - 1 != target_params.len() {
                    return Err(IrVerifyError::new("call argument count does not match target"));
                }
                if tag == IrTag::SelfCall && target != function_id {
                    return Err(IrVerifyError::new("self-call target is not the owner"));
                }
                for raw in &payload[1..] {
                    verify_inst(*raw, owners, states)?;
                }
            }
            IrTag::Ok | IrTag::Err | IrTag::Try | IrTag::Return => {
                verify_inst(data.lhs, owners, states)?;
            }
            IrTag::Error => {
                let payload = store.payload(data.range())?;
                if payload.len() < 3 || (payload.len() - 3) % 2 != 0 {
                    return Err(IrVerifyError::new("error payload schema is invalid"));
                }
                Self::verify_string(store, payload[0])?;
                Self::verify_string(store, payload[1])?;
                if payload[2] as usize != (payload.len() - 3) / 2 {
                    return Err(IrVerifyError::new("error field count does not match payload"));
                }
                for field in payload[3..].chunks_exact(2) {
                    Self::verify_string(store, field[0])?;
                    verify_inst(field[1], owners, states)?;
                }
            }
            IrTag::Record => {
                let payload = store.payload(data.range())?;
                if payload.len() % 2 != 0 {
                    return Err(IrVerifyError::new("record payload schema is invalid"));
                }
                for field in payload.chunks_exact(2) {
                    Self::verify_string(store, field[0])?;
                    verify_inst(field[1], owners, states)?;
                }
            }
            IrTag::RequireRecord => {
                let payload = store.payload(data.range())?;
                if payload.len() < 2 || (payload.len() - 2) % 2 != 0 {
                    return Err(IrVerifyError::new("record requirement schema is invalid"));
                }
                verify_inst(payload[0], owners, states)?;
                Self::verify_string(store, payload[1])?;
                for field in payload[2..].chunks_exact(2) {
                    Self::verify_string(store, field[0])?;
                    if field[1] > IrValueType::Result as u32 {
                        return Err(IrVerifyError::new("record field type is invalid"));
                    }
                }
            }
            IrTag::Match => {
                let payload = store.payload(data.range())?;
                if payload.len() < 2 || (payload.len() - 2) % 3 != 0 {
                    return Err(IrVerifyError::new("match payload schema is invalid"));
                }
                verify_inst(payload[0], owners, states)?;
                if payload[1] as usize != (payload.len() - 2) / 3 {
                    return Err(IrVerifyError::new("match arm count does not match payload"));
                }
                for arm in payload[2..].chunks_exact(3) {
                    let pattern = IrPatternId::from_raw(arm[0])
                        .ok_or_else(|| IrVerifyError::new("pattern id is invalid"))?;
                    if pattern.index() >= store.pattern_tags.len() {
                        return Err(IrVerifyError::new("pattern id is out of bounds"));
                    }
                    if store.pattern_tags[pattern.index()] == IrPatternTag::Bind
                        && store.pattern_data[pattern.index()].lhs >= function.slot_count
                    {
                        return Err(IrVerifyError::new("pattern binding slot is out of bounds"));
                    }
                    let guard = IrOptionalId(arm[1]);
                    if let Some(raw) = guard.raw() {
                        verify_inst(raw, owners, states)?;
                    }
                    verify_inst(arm[2], owners, states)?;
                }
            }
            IrTag::BytesUnpackBe => {
                let payload = Self::payload_len(store, data.range(), 3, "bytes.unpack_be")?;
                for raw in payload {
                    verify_inst(*raw, owners, states)?;
                }
            }
            IrTag::Let | IrTag::AssignAdd => {
                if data.lhs >= function.slot_count {
                    return Err(IrVerifyError::new("statement slot is out of bounds"));
                }
                verify_inst(data.rhs, owners, states)?;
            }
            IrTag::If => {
                let payload = store.payload(data.range())?;
                if payload.len() < 2 || (payload.len() - 2) % 2 != 0 {
                    return Err(IrVerifyError::new("if payload schema is invalid"));
                }
                if payload[0] as usize != (payload.len() - 2) / 2 {
                    return Err(IrVerifyError::new("if branch count does not match payload"));
                }
                for branch in payload[1..payload.len() - 1].chunks_exact(2) {
                    verify_inst(branch[0], owners, states)?;
                    verify_block(branch[1], owners, states)?;
                }
                let else_block = IrOptionalId(*payload.last().unwrap());
                if let Some(raw) = else_block.raw() {
                    verify_block(raw, owners, states)?;
                }
            }
            IrTag::Loop => verify_block(data.lhs, owners, states)?,
            IrTag::Guard => {
                let payload = Self::payload_len(store, data.range(), 4, "guard")?;
                if payload[0] >= function.slot_count {
                    return Err(IrVerifyError::new("guard success slot is out of bounds"));
                }
                verify_inst(payload[1], owners, states)?;
                if let Some(slot) = IrOptionalId(payload[2]).raw()
                    && slot >= function.slot_count
                {
                    return Err(IrVerifyError::new("guard error slot is out of bounds"));
                }
                verify_block(payload[3], owners, states)?;
            }
        }
        states[index] = 2;
        Ok(())
    }

    fn payload_len<'a>(
        store: &'a IrStore,
        range: IrRange,
        len: usize,
        name: &str,
    ) -> Result<&'a [u32], IrVerifyError> {
        let payload = store.payload(range)?;
        if payload.len() != len {
            return Err(IrVerifyError::new(format!(
                "{name} payload has {}, expected {len}",
                payload.len()
            )));
        }
        Ok(payload)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrErrorValue {
    pub family: String,
    pub variant: String,
    pub kind: String,
    pub message: String,
    pub fields: BTreeMap<String, IrValue>,
    pub location: Option<IrLocation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IrValue {
    Unit,
    Int(i64),
    Bool(bool),
    Str(String),
    Bytes(Vec<u8>),
    List(Vec<IrValue>),
    Record(BTreeMap<String, IrValue>),
    ResultOk(Box<IrValue>),
    ResultErr(Box<IrValue>),
    Error(Box<IrErrorValue>),
}

impl IrValue {
    fn display(&self) -> String {
        match self {
            Self::Unit => String::new(),
            Self::Int(value) => value.to_string(),
            Self::Bool(value) => value.to_string(),
            Self::Str(value) => value.clone(),
            Self::Bytes(value) => String::from_utf8_lossy(value).into_owned(),
            Self::List(values) => format!("{values:?}"),
            Self::Record(fields) => format!("{fields:?}"),
            Self::ResultOk(value) => value.display(),
            Self::ResultErr(value) => value.display(),
            Self::Error(error) => error.message.clone(),
        }
    }

    fn matches_type(&self, ty: IrValueType) -> bool {
        match ty {
            IrValueType::Any => true,
            IrValueType::Unit => matches!(self, Self::Unit),
            IrValueType::Int => matches!(self, Self::Int(_)),
            IrValueType::Bool => matches!(self, Self::Bool(_)),
            IrValueType::Str => matches!(self, Self::Str(_)),
            IrValueType::Bytes => matches!(self, Self::Bytes(_)),
            IrValueType::Record => matches!(self, Self::Record(_)),
            IrValueType::List => matches!(self, Self::List(_)),
            IrValueType::Result => matches!(self, Self::ResultOk(_) | Self::ResultErr(_)),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrTraceEvent {
    pub kind: &'static str,
    pub name: Option<String>,
    pub location: Option<IrLocation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrExecution {
    pub value: IrValue,
    pub stdout: Vec<u8>,
    pub trace: Vec<IrTraceEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrExecError {
    pub message: String,
}

impl IrExecError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

enum IrEvalSignal {
    Propagate(IrValue),
    Fault(IrExecError),
}

type IrEvalResult<T> = Result<T, IrEvalSignal>;

enum IrFlow {
    Next,
    Return(IrValue),
    Propagate(IrValue),
    Break,
    Continue,
}

pub struct IrExecutor<'a> {
    program: &'a IrProgram,
    stdout: Vec<u8>,
    trace: Vec<IrTraceEvent>,
}

impl<'a> IrExecutor<'a> {
    fn new(program: &'a IrProgram) -> Result<Self, IrVerifyError> {
        IrVerifier::verify(program)?;
        Ok(Self {
            program,
            stdout: Vec::new(),
            trace: Vec::new(),
        })
    }

    fn execute(mut self, name: &str, args: Vec<IrValue>) -> Result<IrExecution, IrExecError> {
        let function = self.function_by_name(name)?;
        let value = self.call_function(function, args, IrLocation::ZERO, true)?;
        Ok(IrExecution {
            value,
            stdout: self.stdout,
            trace: self.trace,
        })
    }

    fn function_by_name(&self, name: &str) -> Result<IrFunctionId, IrExecError> {
        for (index, function) in self.program.store.functions.iter().enumerate() {
            let id = IrStringId::from_raw(function.name)
                .ok_or_else(|| IrExecError::new("function name id is invalid"))?;
            if self
                .program
                .store
                .string(id)
                .map_err(|error| IrExecError::new(error.message))?
                == name
            {
                return IrFunctionId::new(index)
                    .map_err(|_| IrExecError::new("function id overflow"));
            }
        }
        Err(IrExecError::new(format!("unknown function `{name}`")))
    }

    fn call_function(
        &mut self,
        function_id: IrFunctionId,
        args: Vec<IrValue>,
        call_location: IrLocation,
        trace_call: bool,
    ) -> Result<IrValue, IrExecError> {
        let function = *self
            .program
            .store
            .functions
            .get(function_id.index())
            .ok_or_else(|| IrExecError::new("function id is out of bounds"))?;
        let name = self
            .program
            .store
            .string(
                IrStringId::from_raw(function.name)
                    .ok_or_else(|| IrExecError::new("function name is invalid"))?,
            )
            .map_err(|error| IrExecError::new(error.message))?
            .to_string();
        let params = function
            .params
            .bounds(self.program.store.params.len())
            .ok_or_else(|| IrExecError::new("parameter range is out of bounds"))?;
        if args.len() != params.len() {
            return Err(IrExecError::new(format!(
                "function `{name}` expected {} args, got {}",
                params.len(),
                args.len()
            )));
        }
        if trace_call {
            self.trace.push(IrTraceEvent {
                kind: match function.kind {
                    IrFunctionKind::Pure => "pure.enter",
                    IrFunctionKind::Proc => "proc.enter",
                },
                name: Some(name.clone()),
                location: Some(call_location),
            });
        }
        let mut slots = vec![IrValue::Unit; function.slot_count as usize];
        for (param, value) in self.program.store.params[params].iter().zip(args) {
            slots[param.slot as usize] = value;
        }
        let body = IrBlockId::from_raw(
            function
                .body
                .raw()
                .ok_or_else(|| IrExecError::new("function body is absent"))?,
        )
        .ok_or_else(|| IrExecError::new("function body id is invalid"))?;
        let flow = self.execute_block(function_id, body, &mut slots, call_location)?;
        let mut value = match flow {
            IrFlow::Return(value) => value,
            IrFlow::Propagate(error) => IrValue::ResultErr(Box::new(error)),
            IrFlow::Next => IrValue::Unit,
            IrFlow::Break | IrFlow::Continue => {
                return Err(IrExecError::new("loop control escaped function body"));
            }
        };
        if matches!(function.return_kind, IrReturnKind::ResultUnit | IrReturnKind::ResultInt)
            && !matches!(value, IrValue::ResultOk(_) | IrValue::ResultErr(_))
        {
            value = IrValue::ResultOk(Box::new(value));
        }
        if trace_call {
            self.trace.push(IrTraceEvent {
                kind: match function.kind {
                    IrFunctionKind::Pure => "pure.exit",
                    IrFunctionKind::Proc => "proc.exit",
                },
                name: Some(name),
                location: Some(call_location),
            });
        }
        Ok(value)
    }

    fn execute_block(
        &mut self,
        function_id: IrFunctionId,
        block_id: IrBlockId,
        slots: &mut [IrValue],
        call_location: IrLocation,
    ) -> Result<IrFlow, IrExecError> {
        let block = *self
            .program
            .store
            .blocks
            .get(block_id.index())
            .ok_or_else(|| IrExecError::new("block id is out of bounds"))?;
        let instructions = self
            .program
            .store
            .payload(block.instructions)
            .map_err(|error| IrExecError::new(error.message))?
            .to_vec();
        for raw in instructions {
            let instruction = IrInstId::from_raw(raw)
                .ok_or_else(|| IrExecError::new("statement id is invalid"))?;
            let flow = self.execute_statement(function_id, instruction, slots, call_location)?;
            if !matches!(flow, IrFlow::Next) {
                return Ok(flow);
            }
        }
        Ok(IrFlow::Next)
    }

    fn execute_statement(
        &mut self,
        function_id: IrFunctionId,
        instruction: IrInstId,
        slots: &mut [IrValue],
        call_location: IrLocation,
    ) -> Result<IrFlow, IrExecError> {
        let store = &self.program.store;
        let tag = store.tags[instruction.index()];
        let data = store.data[instruction.index()];
        let location = store
            .location(store.instruction_locations[instruction.index()])
            .map_err(|error| IrExecError::new(error.message))?;
        let value_or_propagate = |result: IrEvalResult<IrValue>| match result {
            Ok(value) => Ok(value),
            Err(IrEvalSignal::Propagate(error)) => Err(IrFlow::Propagate(error)),
            Err(IrEvalSignal::Fault(error)) => Err(IrFlow::Return(IrValue::Error(Box::new(
                IrErrorValue {
                    family: "Error".to_string(),
                    variant: "indexed-exec".to_string(),
                    kind: "indexed-exec".to_string(),
                    message: error.message,
                    fields: BTreeMap::new(),
                    location,
                },
            )))),
        };
        match tag {
            IrTag::Let => {
                let value = match value_or_propagate(self.evaluate_expression(
                    function_id,
                    IrInstId::from_raw(data.rhs).unwrap(),
                    slots,
                    call_location,
                )) {
                    Ok(value) => value,
                    Err(flow) => return Ok(flow),
                };
                slots[data.lhs as usize] = value;
                Ok(IrFlow::Next)
            }
            IrTag::AssignAdd => {
                let value = match value_or_propagate(self.evaluate_expression(
                    function_id,
                    IrInstId::from_raw(data.rhs).unwrap(),
                    slots,
                    call_location,
                )) {
                    Ok(value) => value,
                    Err(flow) => return Ok(flow),
                };
                let IrValue::Int(rhs) = value else {
                    return Err(IrExecError::new("assignment rhs is not Int"));
                };
                let IrValue::Int(lhs) = &mut slots[data.lhs as usize] else {
                    return Err(IrExecError::new("assignment target is not Int"));
                };
                *lhs += rhs;
                Ok(IrFlow::Next)
            }
            IrTag::If => {
                let payload = store
                    .payload(data.range())
                    .map_err(|error| IrExecError::new(error.message))?
                    .to_vec();
                let branch_count = payload[0] as usize;
                for branch in payload[1..1 + branch_count * 2].chunks_exact(2) {
                    let condition = match value_or_propagate(self.evaluate_expression(
                        function_id,
                        IrInstId::from_raw(branch[0]).unwrap(),
                        slots,
                        call_location,
                    )) {
                        Ok(value) => value,
                        Err(flow) => return Ok(flow),
                    };
                    if matches!(condition, IrValue::Bool(true)) {
                        return self.execute_block(
                            function_id,
                            IrBlockId::from_raw(branch[1]).unwrap(),
                            slots,
                            call_location,
                        );
                    }
                }
                if let Some(raw) = IrOptionalId(*payload.last().unwrap()).raw() {
                    return self.execute_block(
                        function_id,
                        IrBlockId::from_raw(raw).unwrap(),
                        slots,
                        call_location,
                    );
                }
                Ok(IrFlow::Next)
            }
            IrTag::Loop => loop {
                match self.execute_block(
                    function_id,
                    IrBlockId::from_raw(data.lhs).unwrap(),
                    slots,
                    call_location,
                )? {
                    IrFlow::Next | IrFlow::Continue => continue,
                    IrFlow::Break => return Ok(IrFlow::Next),
                    flow @ (IrFlow::Return(_) | IrFlow::Propagate(_)) => return Ok(flow),
                }
            },
            IrTag::Guard => {
                let payload = store
                    .payload(data.range())
                    .map_err(|error| IrExecError::new(error.message))?
                    .to_vec();
                let value = match value_or_propagate(self.evaluate_expression(
                    function_id,
                    IrInstId::from_raw(payload[1]).unwrap(),
                    slots,
                    call_location,
                )) {
                    Ok(value) => value,
                    Err(flow) => return Ok(flow),
                };
                match value {
                    IrValue::ResultOk(value) => {
                        slots[payload[0] as usize] = *value;
                        Ok(IrFlow::Next)
                    }
                    IrValue::ResultErr(error) => {
                        if let Some(slot) = IrOptionalId(payload[2]).raw() {
                            slots[slot as usize] = *error;
                        }
                        self.execute_block(
                            function_id,
                            IrBlockId::from_raw(payload[3]).unwrap(),
                            slots,
                            call_location,
                        )
                    }
                    _ => Err(IrExecError::new("guard value is not Result")),
                }
            }
            IrTag::Print => {
                let args = store
                    .payload(data.range())
                    .map_err(|error| IrExecError::new(error.message))?
                    .to_vec();
                let mut rendered = Vec::with_capacity(args.len());
                for raw in args {
                    let value = match value_or_propagate(self.evaluate_expression(
                        function_id,
                        IrInstId::from_raw(raw).unwrap(),
                        slots,
                        call_location,
                    )) {
                        Ok(value) => value,
                        Err(flow) => return Ok(flow),
                    };
                    rendered.push(value.display());
                }
                self.trace.push(IrTraceEvent {
                    kind: "core.call",
                    name: Some("print".to_string()),
                    location,
                });
                self.stdout.extend_from_slice(rendered.join(" ").as_bytes());
                self.stdout.push(b'\n');
                self.trace.push(IrTraceEvent {
                    kind: "core.result",
                    name: Some("print".to_string()),
                    location,
                });
                Ok(IrFlow::Next)
            }
            IrTag::Return => match self.evaluate_expression(
                function_id,
                IrInstId::from_raw(data.lhs).unwrap(),
                slots,
                call_location,
            ) {
                Ok(value) => Ok(IrFlow::Return(value)),
                Err(IrEvalSignal::Propagate(error)) => Ok(IrFlow::Propagate(error)),
                Err(IrEvalSignal::Fault(error)) => Err(error),
            },
            IrTag::Break => Ok(IrFlow::Break),
            IrTag::Continue => Ok(IrFlow::Continue),
            _ => Err(IrExecError::new("expression instruction used as statement")),
        }
    }

    fn evaluate_expression(
        &mut self,
        function_id: IrFunctionId,
        instruction: IrInstId,
        slots: &mut [IrValue],
        call_location: IrLocation,
    ) -> IrEvalResult<IrValue> {
        let store = &self.program.store;
        let tag = store.tags[instruction.index()];
        let data = store.data[instruction.index()];
        let location = store
            .location(store.instruction_locations[instruction.index()])
            .map_err(|error| IrEvalSignal::Fault(IrExecError::new(error.message)))?;
        let child = |raw: u32| {
            IrInstId::from_raw(raw).ok_or_else(|| {
                IrEvalSignal::Fault(IrExecError::new("child instruction id is invalid"))
            })
        };
        match tag {
            IrTag::Unit => Ok(IrValue::Unit),
            IrTag::Int => {
                let bits = data.lhs as u64 | ((data.rhs as u64) << 32);
                Ok(IrValue::Int(bits as i64))
            }
            IrTag::Bool => Ok(IrValue::Bool(data.lhs != 0)),
            IrTag::Str => {
                let id = IrStringId::from_raw(data.lhs).ok_or_else(|| {
                    IrEvalSignal::Fault(IrExecError::new("string id is invalid"))
                })?;
                Ok(IrValue::Str(
                    store
                        .string(id)
                        .map_err(|error| IrEvalSignal::Fault(IrExecError::new(error.message)))?
                        .to_string(),
                ))
            }
            IrTag::Bytes => {
                let id = IrBytesId::from_raw(data.lhs).ok_or_else(|| {
                    IrEvalSignal::Fault(IrExecError::new("bytes id is invalid"))
                })?;
                Ok(IrValue::Bytes(
                    store
                        .bytes(id)
                        .map_err(|error| IrEvalSignal::Fault(IrExecError::new(error.message)))?
                        .to_vec(),
                ))
            }
            IrTag::Slot => Ok(slots[data.lhs as usize].clone()),
            IrTag::Add
            | IrTag::Sub
            | IrTag::Mul
            | IrTag::Eq
            | IrTag::Lt
            | IrTag::Le
            | IrTag::Gt
            | IrTag::Ge => {
                let left = self.evaluate_expression(function_id, child(data.lhs)?, slots, call_location)?;
                let right =
                    self.evaluate_expression(function_id, child(data.rhs)?, slots, call_location)?;
                Self::binary_value(tag, left, right).map_err(IrEvalSignal::Fault)
            }
            IrTag::List => {
                let ids = store
                    .payload(data.range())
                    .map_err(|error| IrEvalSignal::Fault(IrExecError::new(error.message)))?
                    .to_vec();
                let mut values = Vec::with_capacity(ids.len());
                for raw in ids {
                    values.push(self.evaluate_expression(
                        function_id,
                        child(raw)?,
                        slots,
                        call_location,
                    )?);
                }
                Ok(IrValue::List(values))
            }
            IrTag::ListMap => {
                let payload = store
                    .payload(data.range())
                    .map_err(|error| IrEvalSignal::Fault(IrExecError::new(error.message)))?
                    .to_vec();
                let input = self.evaluate_expression(
                    function_id,
                    child(payload[0])?,
                    slots,
                    call_location,
                )?;
                let IrValue::List(input) = input else {
                    return Err(IrEvalSignal::Fault(IrExecError::new(
                        "list-map input is not List",
                    )));
                };
                self.trace.push(IrTraceEvent {
                    kind: "stream.stage.enter",
                    name: Some("map".to_string()),
                    location,
                });
                let mut output = Vec::with_capacity(input.len());
                for item in input {
                    slots[payload[1] as usize] = item;
                    output.push(self.evaluate_expression(
                        function_id,
                        child(payload[2])?,
                        slots,
                        call_location,
                    )?);
                }
                self.trace.push(IrTraceEvent {
                    kind: "stream.stage.exit",
                    name: Some("map".to_string()),
                    location,
                });
                self.trace.push(IrTraceEvent {
                    kind: "stream.stage.enter",
                    name: Some("collect".to_string()),
                    location,
                });
                self.trace.push(IrTraceEvent {
                    kind: "stream.stage.exit",
                    name: Some("collect".to_string()),
                    location,
                });
                Ok(IrValue::List(output))
            }
            IrTag::Index => {
                let base = self.evaluate_expression(function_id, child(data.lhs)?, slots, call_location)?;
                let index =
                    self.evaluate_expression(function_id, child(data.rhs)?, slots, call_location)?;
                let (IrValue::List(values), IrValue::Int(index)) = (base, index) else {
                    return Err(IrEvalSignal::Fault(IrExecError::new(
                        "index operands have invalid types",
                    )));
                };
                values.get(index as usize).cloned().ok_or_else(|| {
                    IrEvalSignal::Fault(IrExecError::new("list index is out of bounds"))
                })
            }
            IrTag::Call | IrTag::SelfCall => {
                let payload = store
                    .payload(data.range())
                    .map_err(|error| IrEvalSignal::Fault(IrExecError::new(error.message)))?
                    .to_vec();
                let target = IrFunctionId::from_raw(payload[0]).ok_or_else(|| {
                    IrEvalSignal::Fault(IrExecError::new("call target is invalid"))
                })?;
                let mut args = Vec::with_capacity(payload.len() - 1);
                for raw in &payload[1..] {
                    args.push(self.evaluate_expression(
                        function_id,
                        child(*raw)?,
                        slots,
                        call_location,
                    )?);
                }
                self.call_function(
                    target,
                    args,
                    location.unwrap_or(IrLocation::ZERO),
                    tag == IrTag::Call,
                )
                .map_err(|error| IrEvalSignal::Fault(error))
            }
            IrTag::Ok => Ok(IrValue::ResultOk(Box::new(self.evaluate_expression(
                function_id,
                child(data.lhs)?,
                slots,
                call_location,
            )?))),
            IrTag::Err => Ok(IrValue::ResultErr(Box::new(self.evaluate_expression(
                function_id,
                child(data.lhs)?,
                slots,
                call_location,
            )?))),
            IrTag::Error => self.evaluate_error(data.range(), function_id, slots, call_location),
            IrTag::Record => {
                let payload = store
                    .payload(data.range())
                    .map_err(|error| IrEvalSignal::Fault(IrExecError::new(error.message)))?
                    .to_vec();
                let mut fields = BTreeMap::new();
                for field in payload.chunks_exact(2) {
                    let name = store
                        .string(IrStringId::from_raw(field[0]).unwrap())
                        .map_err(|error| IrEvalSignal::Fault(IrExecError::new(error.message)))?
                        .to_string();
                    let value = self.evaluate_expression(
                        function_id,
                        child(field[1])?,
                        slots,
                        call_location,
                    )?;
                    fields.insert(name, value);
                }
                Ok(IrValue::Record(fields))
            }
            IrTag::Field => {
                let base = self.evaluate_expression(function_id, child(data.lhs)?, slots, call_location)?;
                let name = store
                    .string(IrStringId::from_raw(data.rhs).unwrap())
                    .map_err(|error| IrEvalSignal::Fault(IrExecError::new(error.message)))?;
                match base {
                    IrValue::Record(fields) => fields.get(name).cloned().ok_or_else(|| {
                        IrEvalSignal::Fault(IrExecError::new("record field is missing"))
                    }),
                    IrValue::Error(error) if name == "message" => {
                        Ok(IrValue::Str(error.message.clone()))
                    }
                    _ => Err(IrEvalSignal::Fault(IrExecError::new(
                        "field base has invalid type",
                    ))),
                }
            }
            IrTag::RequireRecord => {
                let payload = store
                    .payload(data.range())
                    .map_err(|error| IrEvalSignal::Fault(IrExecError::new(error.message)))?
                    .to_vec();
                let value = self.evaluate_expression(
                    function_id,
                    child(payload[0])?,
                    slots,
                    call_location,
                )?;
                let IrValue::Record(fields) = &value else {
                    return Ok(Self::type_error("expected record", location));
                };
                for field in payload[2..].chunks_exact(2) {
                    let name = store
                        .string(IrStringId::from_raw(field[0]).unwrap())
                        .map_err(|error| IrEvalSignal::Fault(IrExecError::new(error.message)))?;
                    let Some(value) = fields.get(name) else {
                        return Ok(Self::type_error("missing record field", location));
                    };
                    let ty = Self::value_type(field[1])?;
                    if !value.matches_type(ty) {
                        return Ok(Self::type_error("record field type mismatch", location));
                    }
                }
                Ok(IrValue::ResultOk(Box::new(value)))
            }
            IrTag::Try => match self.evaluate_expression(
                function_id,
                child(data.lhs)?,
                slots,
                call_location,
            )? {
                IrValue::ResultOk(value) => Ok(*value),
                IrValue::ResultErr(error) => {
                    self.trace.push(IrTraceEvent {
                        kind: "result.propagate",
                        name: None,
                        location: Some(call_location),
                    });
                    Err(IrEvalSignal::Propagate(*error))
                }
                _ => Err(IrEvalSignal::Fault(IrExecError::new(
                    "try operand is not Result",
                ))),
            },
            IrTag::Match => {
                let payload = store
                    .payload(data.range())
                    .map_err(|error| IrEvalSignal::Fault(IrExecError::new(error.message)))?
                    .to_vec();
                let matched = self.evaluate_expression(
                    function_id,
                    child(payload[0])?,
                    slots,
                    call_location,
                )?;
                for arm in payload[2..].chunks_exact(3) {
                    if self.pattern_matches(
                        IrPatternId::from_raw(arm[0]).unwrap(),
                        &matched,
                        slots,
                    )? {
                        if let Some(guard) = IrOptionalId(arm[1]).raw() {
                            let guard = self.evaluate_expression(
                                function_id,
                                child(guard)?,
                                slots,
                                call_location,
                            )?;
                            if !matches!(guard, IrValue::Bool(true)) {
                                continue;
                            }
                        }
                        return self.evaluate_expression(
                            function_id,
                            child(arm[2])?,
                            slots,
                            call_location,
                        );
                    }
                }
                Err(IrEvalSignal::Fault(IrExecError::new(
                    "match has no matching arm",
                )))
            }
            IrTag::BytesUnpackBe => {
                let payload = store
                    .payload(data.range())
                    .map_err(|error| IrEvalSignal::Fault(IrExecError::new(error.message)))?
                    .to_vec();
                self.trace.push(IrTraceEvent {
                    kind: "module.call",
                    name: Some("bytes.unpack_be".to_string()),
                    location,
                });
                let bytes = self.evaluate_expression(
                    function_id,
                    child(payload[0])?,
                    slots,
                    call_location,
                )?;
                let width = self.evaluate_expression(
                    function_id,
                    child(payload[1])?,
                    slots,
                    call_location,
                )?;
                let offset = self.evaluate_expression(
                    function_id,
                    child(payload[2])?,
                    slots,
                    call_location,
                )?;
                let result = match (bytes, width, offset) {
                    (IrValue::Bytes(bytes), IrValue::Int(width), IrValue::Int(offset))
                        if width >= 0 && offset >= 0 =>
                    {
                        let width = width as usize;
                        let offset = offset as usize;
                        if width > 8 || offset.checked_add(width).is_none_or(|end| end > bytes.len())
                        {
                            Self::bytes_unpack_error(location)
                        } else {
                            let mut value = 0i64;
                            for byte in &bytes[offset..offset + width] {
                                value = (value << 8) | i64::from(*byte);
                            }
                            IrValue::ResultOk(Box::new(IrValue::Int(value)))
                        }
                    }
                    _ => Self::bytes_unpack_error(location),
                };
                self.trace.push(IrTraceEvent {
                    kind: "module.result",
                    name: Some("bytes.unpack_be".to_string()),
                    location,
                });
                Ok(result)
            }
            _ => Err(IrEvalSignal::Fault(IrExecError::new(
                "statement instruction used as expression",
            ))),
        }
    }

    fn evaluate_error(
        &mut self,
        range: IrRange,
        function_id: IrFunctionId,
        slots: &mut [IrValue],
        call_location: IrLocation,
    ) -> IrEvalResult<IrValue> {
        let store = &self.program.store;
        let payload = store
            .payload(range)
            .map_err(|error| IrEvalSignal::Fault(IrExecError::new(error.message)))?
            .to_vec();
        let family = store
            .string(IrStringId::from_raw(payload[0]).unwrap())
            .map_err(|error| IrEvalSignal::Fault(IrExecError::new(error.message)))?
            .to_string();
        let variant = store
            .string(IrStringId::from_raw(payload[1]).unwrap())
            .map_err(|error| IrEvalSignal::Fault(IrExecError::new(error.message)))?
            .to_string();
        let mut fields = BTreeMap::new();
        for field in payload[3..].chunks_exact(2) {
            let name = store
                .string(IrStringId::from_raw(field[0]).unwrap())
                .map_err(|error| IrEvalSignal::Fault(IrExecError::new(error.message)))?
                .to_string();
            let value = self.evaluate_expression(
                function_id,
                IrInstId::from_raw(field[1]).unwrap(),
                slots,
                call_location,
            )?;
            fields.insert(name, value);
        }
        let message = fields
            .get("message")
            .and_then(|value| match value {
                IrValue::Str(message) => Some(message.clone()),
                _ => None,
            })
            .unwrap_or_else(|| format!("{family}.{variant}"));
        Ok(IrValue::Error(Box::new(IrErrorValue {
            kind: format!("{family}.{variant}"),
            family,
            variant,
            message,
            fields,
            location: None,
        })))
    }

    fn pattern_matches(
        &self,
        pattern: IrPatternId,
        value: &IrValue,
        slots: &mut [IrValue],
    ) -> IrEvalResult<bool> {
        let tag = self.program.store.pattern_tags[pattern.index()];
        let data = self.program.store.pattern_data[pattern.index()];
        match tag {
            IrPatternTag::Wildcard => Ok(true),
            IrPatternTag::Bind => {
                slots[data.lhs as usize] = value.clone();
                Ok(true)
            }
        }
    }

    fn binary_value(
        tag: IrTag,
        left: IrValue,
        right: IrValue,
    ) -> Result<IrValue, IrExecError> {
        match (tag, left, right) {
            (IrTag::Add, IrValue::Int(left), IrValue::Int(right)) => {
                Ok(IrValue::Int(left + right))
            }
            (IrTag::Sub, IrValue::Int(left), IrValue::Int(right)) => {
                Ok(IrValue::Int(left - right))
            }
            (IrTag::Mul, IrValue::Int(left), IrValue::Int(right)) => {
                Ok(IrValue::Int(left * right))
            }
            (IrTag::Eq, left, right) => Ok(IrValue::Bool(left == right)),
            (IrTag::Lt, IrValue::Int(left), IrValue::Int(right)) => {
                Ok(IrValue::Bool(left < right))
            }
            (IrTag::Le, IrValue::Int(left), IrValue::Int(right)) => {
                Ok(IrValue::Bool(left <= right))
            }
            (IrTag::Gt, IrValue::Int(left), IrValue::Int(right)) => {
                Ok(IrValue::Bool(left > right))
            }
            (IrTag::Ge, IrValue::Int(left), IrValue::Int(right)) => {
                Ok(IrValue::Bool(left >= right))
            }
            _ => Err(IrExecError::new("binary operands have invalid types")),
        }
    }

    fn value_type(raw: u32) -> IrEvalResult<IrValueType> {
        match raw {
            value if value == IrValueType::Any as u32 => Ok(IrValueType::Any),
            value if value == IrValueType::Unit as u32 => Ok(IrValueType::Unit),
            value if value == IrValueType::Int as u32 => Ok(IrValueType::Int),
            value if value == IrValueType::Bool as u32 => Ok(IrValueType::Bool),
            value if value == IrValueType::Str as u32 => Ok(IrValueType::Str),
            value if value == IrValueType::Bytes as u32 => Ok(IrValueType::Bytes),
            value if value == IrValueType::Record as u32 => Ok(IrValueType::Record),
            value if value == IrValueType::List as u32 => Ok(IrValueType::List),
            value if value == IrValueType::Result as u32 => Ok(IrValueType::Result),
            _ => Err(IrEvalSignal::Fault(IrExecError::new(
                "value type tag is invalid",
            ))),
        }
    }

    fn type_error(message: &str, location: Option<IrLocation>) -> IrValue {
        IrValue::ResultErr(Box::new(IrValue::Error(Box::new(IrErrorValue {
            family: "Error".to_string(),
            variant: "type-mismatch".to_string(),
            kind: "type-mismatch".to_string(),
            message: message.to_string(),
            fields: BTreeMap::new(),
            location,
        }))))
    }

    fn bytes_unpack_error(location: Option<IrLocation>) -> IrValue {
        IrValue::ResultErr(Box::new(IrValue::Error(Box::new(IrErrorValue {
            family: "Error".to_string(),
            variant: "bytes-unpack".to_string(),
            kind: "bytes-unpack".to_string(),
            message: "requested integer extends past end of byte data".to_string(),
            fields: BTreeMap::new(),
            location,
        }))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::eval::Evaluator;
    use crate::runtime::value::{ResultValue, Value};
    use crate::sema::check::Checker;
    use crate::symbol::Name;
    use crate::syntax::parser::Parser;
    use crate::trace::TraceEvent;

    const VERTICAL_SLICE: &str =
        include_str!("../../../tests/fixtures/frontend-campaign/vertical-slice.xsh");
    const UNSUPPORTED_SLICE: &str =
        include_str!("../../../tests/fixtures/frontend-campaign/vertical-slice-unsupported.xsh");

    fn run_with_large_stack(f: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(f)
            .expect("spawn indexed IR test")
            .join()
            .unwrap_or_else(|payload| std::panic::resume_unwind(payload));
    }

    fn lowered_fixture(
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
        let units = super::super::probe_compact_lower_function_units(
            &parsed.arena,
            &declarations,
            &bodies,
            source,
        );
        drop(bodies);
        drop(declarations);
        drop(parsed);
        (Arc::new(sources), source_id, units)
    }

    fn build_fixture(name: &str, source: &str) -> Result<IrProgram, IrBuildError> {
        let (sources, source_id, units) = lowered_fixture(name, source);
        let program = IrBuilder::build_from_units(&units, sources, source_id);
        drop(units);
        program
    }

    fn oracle(name: &str, source: &str, function: &str) -> IrExecution {
        let mut sources = SourceMap::new();
        let source_id = sources.add_file(name, source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources).with_tracing();
        assert!(
            evaluator
                .install_compact_lowered_program(&parsed.arena, source_id)
                .is_empty()
        );
        let result = evaluator
            .call_lowered_proc(
                Name::intern(function),
                &[],
                Span::new(source_id, 0, 0),
            )
            .unwrap_or_else(|| panic!("oracle function `{function}` is not lowered"))
            .unwrap_or_else(|error| panic!("oracle function failed: {error:?}"));
        IrExecution {
            value: normalize_value(&result),
            stdout: evaluator.stdout,
            trace: evaluator.trace_events.iter().map(normalize_trace).collect(),
        }
    }

    fn normalize_value(value: &Value) -> IrValue {
        match value {
            Value::Unit => IrValue::Unit,
            Value::Int(value) => IrValue::Int(*value),
            Value::Bool(value) => IrValue::Bool(*value),
            Value::Str(value) => IrValue::Str(value.to_string()),
            Value::Bytes(value) => IrValue::Bytes(value.clone()),
            Value::List(values) => IrValue::List(values.iter().map(normalize_value).collect()),
            Value::Record(fields) => IrValue::Record(
                fields
                    .iter()
                    .map(|(name, value)| (name.to_string(), normalize_value(value)))
                    .collect(),
            ),
            Value::Result(ResultValue::Ok(value)) => {
                IrValue::ResultOk(Box::new(normalize_value(value)))
            }
            Value::Result(ResultValue::Err(value)) => {
                IrValue::ResultErr(Box::new(normalize_value(value)))
            }
            Value::Error(error) => IrValue::Error(Box::new(IrErrorValue {
                family: error.family.clone(),
                variant: error.variant.clone(),
                kind: error.kind.clone(),
                message: error.message.clone(),
                fields: error
                    .payload
                    .iter()
                    .map(|(name, value)| (name.to_string(), normalize_value(value)))
                    .collect(),
                location: error.span.map(|span| IrLocation {
                    start: span.start() as u32,
                    len: span.end().saturating_sub(span.start()) as u32,
                }),
            })),
            other => panic!("unsupported oracle value: {other:?}"),
        }
    }

    fn normalize_trace(event: &TraceEvent) -> IrTraceEvent {
        IrTraceEvent {
            kind: event.kind.as_str(),
            name: event.name.clone(),
            location: event.source_span.map(|span| IrLocation {
                start: span.start() as u32,
                len: span.end().saturating_sub(span.start()) as u32,
            }),
        }
    }

    #[test]
    fn vertical_slice_executes_after_frontend_state_is_dropped() {
        run_with_large_stack(|| {
            let expected = oracle("vertical-slice.xsh", VERTICAL_SLICE, "main");
            let program = build_fixture("vertical-slice.xsh", VERTICAL_SLICE).unwrap();
            let actual = IrExecutor::new(&program)
                .unwrap()
                .execute("main", Vec::new())
                .unwrap();

            assert_eq!(actual, expected);
        });
    }

    #[test]
    fn traceable_runtime_error_matches_oracle_location_and_trace() {
        run_with_large_stack(|| {
            let expected = oracle("vertical-slice.xsh", VERTICAL_SLICE, "exact_error_site");
            let program = build_fixture("vertical-slice.xsh", VERTICAL_SLICE).unwrap();
            let actual = IrExecutor::new(&program)
                .unwrap()
                .execute("exact_error_site", Vec::new())
                .unwrap();

            assert_eq!(actual, expected);
        });
    }

    #[test]
    fn unsupported_fixture_commits_no_instructions() {
        let error = build_fixture("vertical-slice-unsupported.xsh", UNSUPPORTED_SLICE)
            .expect_err("unsupported slice must not finalize");

        assert_eq!(error.construct, "lowered_function_blocker");
        assert_eq!(error.attempted_instructions, 0);
        assert_eq!(error.committed_instructions, 0);
    }

    #[test]
    fn partially_emitted_function_rewinds_every_store_column() {
        let (_sources, source_id, units) = lowered_fixture("vertical-slice.xsh", VERTICAL_SLICE);
        let mut ordered = units.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|unit| (unit.source_span().start(), unit.key().display_name()));
        let mut builder = IrBuilder::new(source_id);
        builder.predeclare_functions(&ordered).unwrap();
        let main = units
            .iter()
            .find(|unit| unit.key() == LoweredFunctionKey::Name(Name::intern("main")))
            .unwrap();
        let mut body = (*main.lowered_body().unwrap()).clone();
        body.body.push(LoweredStmt::Defer {
            value: LoweredExpr::Unit,
        });
        let checkpoint = builder.checkpoint();
        let error = builder
            .build_function_transaction(builder.functions[&main.key()], &body)
            .expect_err("defer is outside the vertical slice");

        assert!(error.attempted_instructions > 0);
        assert_eq!(error.committed_instructions, 0);
        assert_eq!(builder.checkpoint(), checkpoint);
    }

    #[test]
    fn malformed_stores_are_rejected_before_execution() {
        let program = build_fixture("vertical-slice.xsh", VERTICAL_SLICE).unwrap();

        let mut bad_slot = program.clone();
        let slot = bad_slot
            .store
            .tags
            .iter()
            .position(|tag| *tag == IrTag::Slot)
            .unwrap();
        bad_slot.store.data[slot].lhs = u32::MAX;
        assert!(IrExecutor::new(&bad_slot).is_err());

        let mut bad_payload = program.clone();
        let call = bad_payload
            .store
            .tags
            .iter()
            .position(|tag| *tag == IrTag::Call)
            .unwrap();
        bad_payload.store.data[call].rhs = u32::MAX;
        assert!(IrExecutor::new(&bad_payload).is_err());

        let mut bad_optional = program;
        bad_optional.store.functions[0].body = IrOptionalId(0);
        assert!(IrExecutor::new(&bad_optional).is_err());
    }

    #[test]
    fn dumps_layouts_and_payload_budgets_are_stable() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<IrData>();
        assert_copy::<IrRange>();
        assert_copy::<IrBlock>();
        assert_copy::<IrFunction>();
        assert_copy::<IrParam>();
        assert_copy::<IrCapture>();

        assert_eq!(size_of::<IrInstId>(), 4);
        assert_eq!(size_of::<IrFunctionId>(), 4);
        assert_eq!(size_of::<IrOptionalId>(), 4);
        assert_eq!(size_of::<IrTag>(), 1);
        assert_eq!(size_of::<IrPatternTag>(), 1);
        assert_eq!(size_of::<IrData>(), 8);
        assert_eq!(size_of::<IrRange>(), 8);
        assert_eq!(size_of::<IrLocation>(), 8);
        assert_eq!(size_of::<IrBlock>(), 12);
        assert_eq!(size_of::<IrFunction>(), 32);
        assert_eq!(size_of::<IrParam>(), 12);
        assert_eq!(size_of::<IrCapture>(), 12);
        assert_eq!(IrStore::common_instruction_row_bytes(), 13);

        let first = build_fixture("vertical-slice.xsh", VERTICAL_SLICE).unwrap();
        let second = build_fixture("vertical-slice.xsh", VERTICAL_SLICE).unwrap();
        assert_eq!(first.dump().unwrap(), second.dump().unwrap());
        assert!(first.store.extra_bytes_per_instruction() <= 24.0);
        assert!(first.retained_bytes() > first.store.retained_bytes());
        println!(
            "phase1 vertical functions={} blocks={} instructions={} common_row_bytes={} extra_bytes_per_instruction={:.3} store_retained_bytes={} program_retained_bytes={}",
            first.store.functions.len(),
            first.store.blocks.len(),
            first.store.instruction_count(),
            IrStore::common_instruction_row_bytes(),
            first.store.extra_bytes_per_instruction(),
            first.store.retained_bytes(),
            first.retained_bytes(),
        );
        println!("{}", first.dump().unwrap());
    }

    #[test]
    fn typed_extra_payloads_roundtrip_ids_slots_and_sentinels() {
        let mut builder = IrBuilder::new(SourceId::new(0));
        let instruction = IrInstId::new(0).unwrap();
        let block = IrBlockId::new(0).unwrap();
        let function = IrFunctionId::new(0).unwrap();
        let mut writer = IrExtraWriter::with_capacity(6);
        writer.count(2).unwrap();
        writer.id(instruction);
        writer.id(block);
        writer.optional_id(Some(function));
        writer.optional_id::<IrFunctionId>(None);
        writer.slot(7).unwrap();
        let range = writer.finish(&mut builder).unwrap();
        let payload = builder.store.payload(range).unwrap();
        let mut reader = IrExtraReader::new(payload);
        assert_eq!(reader.count().unwrap(), 2);
        assert_eq!(reader.id::<IrInstId>("instruction").unwrap(), instruction);
        assert_eq!(reader.id::<IrBlockId>("block").unwrap(), block);
        assert_eq!(
            reader.optional_id::<IrFunctionId>("function").unwrap(),
            Some(function)
        );
        assert_eq!(
            reader.optional_id::<IrFunctionId>("function").unwrap(),
            None
        );
        assert_eq!(reader.slot().unwrap(), 7);
        assert!(reader.remaining().is_empty());
        reader.finish().unwrap();
    }

    #[test]
    #[ignore = "Phase 1 evidence scans the full checked-in XSH corpus"]
    fn corpus_weighted_extra_payload_estimate() {
        run_with_large_stack(|| {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            let mut paths = Vec::new();
            for relative in crate::frontend_stats::DEFAULT_ROOTS {
                collect_xsh_paths(&root.join(relative), &mut paths);
            }
            paths.sort();
            paths.dedup();
            let mut aggregate = IrStorageEstimate::default();
            let mut files_measured = 0usize;
            for path in paths {
                let Ok(source) = std::fs::read_to_string(&path) else {
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
                let units = super::super::probe_compact_lower_function_units(
                    &parsed.arena,
                    &declarations,
                    &bodies,
                    &source,
                );
                let Ok(estimate) = IrBuilder::estimate_supported_units(&units, source_id) else {
                    continue;
                };
                aggregate.add(estimate);
                files_measured += 1;
            }
            assert!(files_measured > 0);
            assert!(aggregate.functions_built > 0);
            assert!(aggregate.instructions > 0);
            assert!(aggregate.extra_bytes_per_instruction() <= 24.0);
            println!(
                "phase1 corpus files={} functions_seen={} functions_built={} instructions={} extra_words={} common_row_bytes={} extra_bytes_per_instruction={:.3}",
                files_measured,
                aggregate.functions_seen,
                aggregate.functions_built,
                aggregate.instructions,
                aggregate.extra_words,
                IrStore::common_instruction_row_bytes(),
                aggregate.extra_bytes_per_instruction(),
            );
        });
    }

    fn collect_xsh_paths(path: &std::path::Path, paths: &mut Vec<std::path::PathBuf>) {
        if path.is_file() {
            if path.extension().is_some_and(|extension| extension == "xsh") {
                paths.push(path.to_path_buf());
            }
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            collect_xsh_paths(&entry.path(), paths);
        }
    }
}
