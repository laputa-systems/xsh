use super::{
    LoweredRunArg, LoweredRunArgKind, LoweredRunEnv, LoweredRunRedirection,
    LoweredRunPipelineSegment, LoweredProcessCommandArgv, LoweredRunCapture, LoweredSpawnRun,
    LoweredTypeCheck, ScanBytes, LoweredCallArg, LoweredFmtPart,
    LoweredRecordEntry, LoweredAssignStep, LoweredCompTarget, LoweredCompQualifier,
    LoweredProcessCommandBuilderEntry, LoweredErrorExpr,
};

use super::{
    Arc, AssignOp, BinaryOp, BuildBoolId, BuildExprId, BuildIntId, BuildPatternId, BuildStmtId,
    Cell, DurationValue, FloatValue, FormatSpec, FullCodec, FullCursor, FullDecoder,
    FullExecution, FullStore, FunctionName, HashAlgorithm, IrFunctionId,
    IrVerifyError, LoweredFunctionKey, LoweredStrPredicate, LoweredType, LoweredValue, Name,
    NameText, PathValue, PreparedConstantValue, QualifiedName, RedirectionKind, RegexValue,
    ReduceByOp, RunKind, RuntimeOp, Span, Type, TypeId,
};
use super::schema::InstructionKind;
use std::marker::PhantomData;

/// The verified store and owner of a borrowed field. No verifier traversal
/// state is retained or rebuilt while execution reads its operands.
#[derive(Clone, Copy)]
pub(in crate::runtime::eval) struct DecodeContext<'a> {
    store: &'a FullStore,
    owner: u32,
    start: usize,
    end: usize,
    slots: u32,
    verified: bool,
}

impl<'a> DecodeContext<'a> {
    pub(super) fn new(execution: &FullExecution<'a>) -> Self {
        let decoder = &execution.decoder;
        Self {
            store: decoder.store,
            owner: decoder.owner,
            start: decoder.instruction_range.start,
            end: decoder.instruction_range.end,
            slots: decoder.slot_count,
            verified: decoder.verified,
        }
    }

    fn decoder(self) -> FullDecoder<'a> {
        FullDecoder {
            store: self.store,
            owner: self.owner,
            instruction_range: self.start..self.end,
            instruction_states: None,
            instruction_proof: None,
            block_states: None,
            slot_count: self.slots,
            pattern_ceiling: Cell::new(usize::MAX),
            verified: self.verified,
        }
    }

    /// Opens a tagged stage row without materializing its stored payload.
    pub(super) fn stage_payload(
        self,
        raw: u32,
    ) -> Result<(super::FullStageTag, FullCursor<'a>), IrVerifyError> {
        let index = raw as usize;
        let tag = self.store.stages.get(index).copied()
            .ok_or_else(|| IrVerifyError::new("pipeline stage id is out of bounds"))?;
        let data = self.store.stage_data.get(index).copied()
            .ok_or_else(|| IrVerifyError::new("pipeline stage id is out of bounds"))?;
        Ok((tag, FullCursor::new(self.store.payload(data.range())?)))
    }
}

pub(in crate::runtime::eval) trait BorrowCodec: FullCodec {
    type View<'a>;
    fn borrow<'a>(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
    -> Result<Self::View<'a>, IrVerifyError>;
}

/// Executable instruction identities carry their operand category and borrow
/// the program that owns the referenced instruction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct InstructionId<'a, const KIND: u8> {
    raw: u32,
    program: PhantomData<&'a FullStore>,
}

impl<const KIND: u8> InstructionId<'_, KIND> {
    pub(in crate::runtime::eval) fn raw(self) -> u32 { self.raw }
}

pub(in crate::runtime::eval) type ExprInstructionId<'a> =
    InstructionId<'a, { InstructionKind::Expr as u8 }>;
pub(in crate::runtime::eval) type StmtInstructionId<'a> =
    InstructionId<'a, { InstructionKind::Stmt as u8 }>;
pub(in crate::runtime::eval) type IntInstructionId<'a> =
    InstructionId<'a, { InstructionKind::Int as u8 }>;
pub(in crate::runtime::eval) type BoolInstructionId<'a> =
    InstructionId<'a, { InstructionKind::Bool as u8 }>;

macro_rules! instruction_id {
    ($build:ty, $kind:ident, $view:ident) => {
        impl BorrowCodec for $build {
            type View<'a> = $view<'a>;
            fn borrow<'a>(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
            -> Result<Self::View<'a>, IrVerifyError> {
                let raw = input.raw()?;
                let index = raw as usize;
                if !(context.start..context.end).contains(&index) {
                    return Err(IrVerifyError::new("full IR instruction belongs to another function"));
                }
                let tag = context.store.tags.get(index)
                    .ok_or_else(|| IrVerifyError::new("full IR instruction is out of bounds"))?;
                if tag.kind() != InstructionKind::$kind {
                    return Err(IrVerifyError::new("full IR instruction tag has the wrong category"));
                }
                Ok(InstructionId { raw, program: PhantomData })
            }
        }
    };
}
instruction_id!(BuildExprId, Expr, ExprInstructionId);
instruction_id!(BuildStmtId, Stmt, StmtInstructionId);
instruction_id!(BuildIntId, Int, IntInstructionId);
instruction_id!(BuildBoolId, Bool, BoolInstructionId);

/// A pattern identity is distinct from an instruction or a literal identity.
#[derive(Clone, Copy)]
pub(in crate::runtime::eval) struct PatternId<'a> {
    raw: u32,
    store: &'a FullStore,
}
impl PatternId<'_> {
    pub(in crate::runtime::eval) fn raw(self) -> u32 { self.raw }

    pub(super) fn raw_for_store(self, store: &FullStore) -> Result<u32, IrVerifyError> {
        if !std::ptr::eq(self.store, store) {
            return Err(IrVerifyError::new("pattern identity belongs to another program"));
        }
        Ok(self.raw)
    }
}
impl BorrowCodec for BuildPatternId {
    type View<'a> = PatternId<'a>;
    fn borrow<'a>(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
    -> Result<Self::View<'a>, IrVerifyError> {
        let raw = input.raw()?;
        context.store.patterns.get(raw as usize)
            .ok_or_else(|| IrVerifyError::new("pattern id is out of bounds"))?;
        Ok(PatternId { raw, store: context.store })
    }
}

macro_rules! scalar_field {
    ($($ty:ty),* $(,)?) => {$(
        impl BorrowCodec for $ty {
            type View<'a> = Self;
            fn borrow<'a>(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
            -> Result<Self, IrVerifyError> {
                fn scalar<T: FullCodec>(context: DecodeContext<'_>, input: &mut FullCursor<'_>)
                -> Result<T, IrVerifyError> { T::decode(&context.decoder(), input) }
                scalar(context, input)
            }
        }
    )*};
}
scalar_field!(
    bool, usize, i64, u32, u64, FloatValue, DurationValue, Name, QualifiedName, Span,
    BinaryOp, AssignOp, RuntimeOp, LoweredStrPredicate, RedirectionKind, RunKind,
    HashAlgorithm, FormatSpec, ReduceByOp, crate::runtime::eval::EndDistance,
    crate::syntax::arena::ContextScopeKind, crate::syntax::arena::RetrySchedule,
);

macro_rules! text_field {
    ($($ty:ty),* $(,)?) => {$(
        impl BorrowCodec for $ty {
            type View<'a> = &'a str;
            fn borrow<'a>(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
            -> Result<Self::View<'a>, IrVerifyError> {
                context.store.string(input.raw()?)
            }
        }
    )*};
}
text_field!(Arc<str>, NameText, String);

macro_rules! bytes_field {
    ($($ty:ty),* $(,)?) => {$(
        impl BorrowCodec for $ty {
            type View<'a> = &'a [u8];
            fn borrow<'a>(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
            -> Result<Self::View<'a>, IrVerifyError> {
                context.store.bytes(input.raw()?)
            }
        }
    )*};
}
bytes_field!(Arc<[u8]>, Vec<u8>, PathValue);

macro_rules! pool_field {
    ($ty:ty, $field:ident, $value:ty, $message:literal) => {
        impl BorrowCodec for $ty {
            type View<'a> = &'a $value;
            fn borrow<'a>(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
            -> Result<Self::View<'a>, IrVerifyError> {
                context.store.$field.get(input.raw()? as usize)
                    .ok_or_else(|| IrVerifyError::new($message))
            }
        }
    };
}
pool_field!(RegexValue, prepared_regexes, RegexValue, "prepared regex is out of bounds");
pool_field!(PreparedConstantValue, prepared_constants, PreparedConstantValue, "prepared constant is out of bounds");
pool_field!(Arc<crate::runtime::eval::ModuleCallPlan>, prepared_module_call_plans, Arc<crate::runtime::eval::ModuleCallPlan>, "prepared module call plan is out of bounds");
pool_field!(Arc<crate::sema::wire_enums::WireEnumMapping>, wire_enums, Arc<crate::sema::wire_enums::WireEnumMapping>, "wire enum mapping is out of bounds");
pool_field!(Arc<super::super::super::require::PreparedSchema>, prepared_schemas, Arc<super::super::super::require::PreparedSchema>, "prepared schema is out of bounds");

/// A semantic type is read from its indexed pool when its consumer needs the
/// type's behavior; decoding its identity does not reconstruct a `Type` tree.
#[derive(Clone, Copy)]
pub(in crate::runtime::eval) struct TypeRef<'a> {
    pub(in crate::runtime::eval) id: TypeId,
    store: &'a FullStore,
}
impl TypeRef<'_> {
    pub(in crate::runtime::eval) fn materialize(self) -> Result<Type, IrVerifyError> {
        self.store.semantic.to_type(self.id)
    }
}
macro_rules! type_field {
    ($($ty:ty),* $(,)?) => {$(
        impl BorrowCodec for $ty {
            type View<'a> = TypeRef<'a>;
            fn borrow<'a>(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
            -> Result<Self::View<'a>, IrVerifyError> {
                let id = TypeId::from_raw(input.raw()?)
                    .ok_or_else(|| IrVerifyError::new("semantic type id is invalid"))?;
                context.store.semantic.type_tag(id)?;
                Ok(TypeRef { id, store: context.store })
            }
        }
    )*};
}
type_field!(Type, LoweredType);

#[derive(Clone, Copy)]
pub(in crate::runtime::eval) struct ValueRef<'a> {
    raw: u32,
    context: DecodeContext<'a>,
}
impl ValueRef<'_> {
    pub(in crate::runtime::eval) fn materialize(self) -> Result<LoweredValue, IrVerifyError> {
        LoweredValue::decode(&self.context.decoder(), &mut FullCursor::new(&[self.raw]))
    }
}
impl BorrowCodec for LoweredValue {
    type View<'a> = ValueRef<'a>;
    fn borrow<'a>(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
    -> Result<Self::View<'a>, IrVerifyError> {
        let raw = input.raw()?;
        context.store.values.get(raw as usize)
            .ok_or_else(|| IrVerifyError::new("value id is out of bounds"))?;
        Ok(ValueRef { raw, context })
    }
}

impl<T: BorrowCodec> BorrowCodec for Option<T> {
    type View<'a> = Option<T::View<'a>>;
    fn borrow<'a>(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
    -> Result<Self::View<'a>, IrVerifyError> {
        match input.raw()? {
            0 => Ok(None),
            1 => T::borrow(context, input).map(Some),
            _ => Err(IrVerifyError::new("optional payload tag is invalid")),
        }
    }
}
impl<T: BorrowCodec> BorrowCodec for Box<T> {
    type View<'a> = T::View<'a>;
    fn borrow<'a>(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
    -> Result<Self::View<'a>, IrVerifyError> { T::borrow(context, input) }
}

pub(in crate::runtime::eval) trait VectorElement: FullCodec { const BLOCK_FLAGS: u8; }

/// A list view keeps its encoded entries borrowed. Traversal reads one typed
/// entry at a time, so opening a list never materializes its children.
pub(in crate::runtime::eval) struct ListView<'a, T> {
    context: DecodeContext<'a>,
    block: super::IrBlockId,
    input: FullCursor<'a>,
    remaining: usize,
    element: PhantomData<T>,
}

/// A typed list cursor can resume only against the store and block it came from.
pub(in crate::runtime::eval) struct ListPosition<T> {
    store: usize,
    block: super::IrBlockId,
    offset: usize,
    remaining: usize,
    element: PhantomData<fn() -> T>,
}

impl<T> Copy for ListPosition<T> {}

impl<T> Clone for ListPosition<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> ListPosition<T> {
    pub(in crate::runtime::eval) fn is_exhausted(&self) -> bool {
        self.remaining == 0
    }
}

impl<T> ListView<'_, T> {
    pub(in crate::runtime::eval) fn len(&self) -> usize { self.remaining }

    pub(in crate::runtime::eval) fn position(&self) -> ListPosition<T> {
        ListPosition {
            store: self.context.store as *const super::FullStore as usize,
            block: self.block,
            offset: self.input.index,
            remaining: self.remaining,
            element: PhantomData,
        }
    }

    pub(in crate::runtime::eval) fn resume(
        &mut self,
        position: ListPosition<T>,
    ) -> Result<(), IrVerifyError> {
        let Some(&length) = self.input.words.first() else {
            return Err(IrVerifyError::new("list cursor is invalid"));
        };
        if self.context.store as *const super::FullStore as usize != position.store
            || self.block != position.block
            || position.offset == 0
            || position.offset > self.input.words.len()
            || position.remaining > length as usize
            || (position.remaining == 0 && position.offset != self.input.words.len())
            || (position.remaining > 0 && position.offset == self.input.words.len())
        {
            return Err(IrVerifyError::new("list cursor is invalid"));
        }
        self.input.index = position.offset;
        self.remaining = position.remaining;
        Ok(())
    }
}
impl ListView<'_, BuildStmtId> {
    pub(in crate::runtime::eval) fn block_id(&self) -> u32 { self.block.raw() }
}
impl<'a, T: BorrowCodec> Iterator for ListView<'a, T> {
    type Item = Result<T::View<'a>, IrVerifyError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 { return None; }
        self.remaining -= 1;
        let value = T::borrow(self.context, &mut self.input);
        if self.remaining == 0 {
            if let Err(error) = self.input.finish() { return Some(Err(error)); }
        }
        Some(value)
    }
}
impl<T: VectorElement> BorrowCodec for Vec<T> where Vec<T>: FullCodec {
    type View<'a> = ListView<'a, T>;
    fn borrow<'a>(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
    -> Result<Self::View<'a>, IrVerifyError> {
        let (block, mut entries) = context.decoder().block(input, T::BLOCK_FLAGS)?;
        let remaining = entries.raw()? as usize;
        if remaining == 0 { entries.finish()?; }
        Ok(ListView { context, block, input: entries, remaining, element: PhantomData })
    }
}

/// Text pattern fields share one hole count and are stored inline after the
/// hole block, so their borrowed iterators read fixed-length slices in place.
pub(in crate::runtime::eval) struct TextHoleKinds<'a> {
    input: FullCursor<'a>,
    remaining: usize,
}
impl Iterator for TextHoleKinds<'_> {
    type Item = Result<crate::sema::check::TextHoleKind, IrVerifyError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 { return None; }
        self.remaining -= 1;
        let kind = self.input.raw().and_then(super::text_hole_kind);
        if self.remaining == 0 {
            if let Err(error) = self.input.finish() { return Some(Err(error)); }
        }
        Some(kind)
    }
}

pub(in crate::runtime::eval) struct TextPatternSegments<'a> {
    context: DecodeContext<'a>,
    input: FullCursor<'a>,
    remaining: usize,
}
impl<'a> Iterator for TextPatternSegments<'a> {
    type Item = Result<&'a str, IrVerifyError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 { return None; }
        self.remaining -= 1;
        let segment = self.input.raw().and_then(|id| self.context.store.string(id));
        if self.remaining == 0 {
            if let Err(error) = self.input.finish() { return Some(Err(error)); }
        }
        Some(segment)
    }
}

pub(in crate::runtime::eval) struct TextPatternFields<'a> {
    pub(in crate::runtime::eval) holes: ListView<'a, BuildPatternId>,
    pub(in crate::runtime::eval) kinds: TextHoleKinds<'a>,
    pub(in crate::runtime::eval) segments: TextPatternSegments<'a>,
}
impl<'a> TextPatternFields<'a> {
    pub(super) fn borrow(
        context: DecodeContext<'a>,
        input: &mut FullCursor<'a>,
    ) -> Result<Self, IrVerifyError> {
        let holes = Vec::<BuildPatternId>::borrow(context, input)?;
        let count = holes.len();

        let kinds_start = input.index;
        for _ in 0..count {
            super::text_hole_kind(input.raw()?)?;
        }
        let kinds = TextHoleKinds {
            input: FullCursor::new(&input.words[kinds_start..input.index]),
            remaining: count,
        };

        let segment_count = count
            .checked_add(1)
            .ok_or_else(|| IrVerifyError::new("text pattern segment count overflows"))?;
        let segments_start = input.index;
        for _ in 0..segment_count {
            context.store.string(input.raw()?)?;
        }
        let segments = TextPatternSegments {
            context,
            input: FullCursor::new(&input.words[segments_start..input.index]),
            remaining: segment_count,
        };

        Ok(Self { holes, kinds, segments })
    }
}

macro_rules! tuple_field {
    ($($ty:ident),+) => {
        impl<$($ty: BorrowCodec),+> BorrowCodec for ($($ty,)+) {
            type View<'a> = ($($ty::View<'a>,)+);
            fn borrow<'a>(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
            -> Result<Self::View<'a>, IrVerifyError> {
                Ok(($($ty::borrow(context, input)?,)+))
            }
        }
    };
}
tuple_field!(A, B);
tuple_field!(A, B, C);

pub(in crate::runtime::eval) struct InlineList<'a, T> {
    context: DecodeContext<'a>,
    input: FullCursor<'a>,
    remaining: usize,
    element: PhantomData<T>,
}
impl<'a, T: BorrowCodec> InlineList<'a, T> {
    fn borrow(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
    -> Result<Self, IrVerifyError> {
        let remaining = input.raw()? as usize;
        let start = input.index;
        for _ in 0..remaining { T::borrow(context, input)?; }
        Ok(Self { context, input: FullCursor::new(&input.words[start..input.index]), remaining, element: PhantomData })
    }
}
impl<'a, T: BorrowCodec> Iterator for InlineList<'a, T> {
    type Item = Result<T::View<'a>, IrVerifyError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 { return None; }
        self.remaining -= 1;
        Some(T::borrow(self.context, &mut self.input))
    }
}
impl<A: smallvec::Array> BorrowCodec for smallvec::SmallVec<A>
where A::Item: BorrowCodec {
    type View<'a> = InlineList<'a, A::Item>;
    fn borrow<'a>(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
    -> Result<Self::View<'a>, IrVerifyError> { InlineList::borrow(context, input) }
}
impl<T: BorrowCodec> BorrowCodec for rustc_hash::FxHashMap<Arc<str>, T>
where rustc_hash::FxHashMap<Arc<str>, T>: FullCodec {
    type View<'a> = InlineList<'a, (Arc<str>, T)>;
    fn borrow<'a>(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
    -> Result<Self::View<'a>, IrVerifyError> { InlineList::borrow(context, input) }
}

#[derive(Clone, Copy)]
pub(in crate::runtime::eval) enum FunctionNameView {
    Name(Name),
    Qualified(QualifiedName),
}
impl BorrowCodec for FunctionName {
    type View<'a> = FunctionNameView;
    fn borrow<'a>(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
    -> Result<Self::View<'a>, IrVerifyError> {
        match input.raw()? {
            0 => Ok(FunctionNameView::Name(Name::borrow(context, input)?)),
            1 => Ok(FunctionNameView::Qualified(QualifiedName::borrow(context, input)?)),
            _ => Err(IrVerifyError::new("function name tag is invalid")),
        }
    }
}

#[derive(Clone, Copy)]
pub(in crate::runtime::eval) struct FunctionRef<'a> {
    pub(in crate::runtime::eval) id: IrFunctionId,
    program: PhantomData<&'a FullStore>,
}
impl BorrowCodec for LoweredFunctionKey {
    type View<'a> = FunctionRef<'a>;
    fn borrow<'a>(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
    -> Result<Self::View<'a>, IrVerifyError> {
        let id = IrFunctionId::from_raw(input.raw()?)
            .ok_or_else(|| IrVerifyError::new("function identity is invalid"))?;
        context.store.functions.get(id.index())
            .ok_or_else(|| IrVerifyError::new("function identity is out of bounds"))?;
        Ok(FunctionRef { id, program: PhantomData })
    }
}

macro_rules! wrapped_list {
    ($ty:ty, $items:ty) => {
        impl BorrowCodec for $ty {
            type View<'a> = <$items as BorrowCodec>::View<'a>;
            fn borrow<'a>(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
            -> Result<Self::View<'a>, IrVerifyError> { <$items>::borrow(context, input) }
        }
    };
}
wrapped_list!(super::LoweredRecordUpdates, Vec<(Vec<Name>, BuildExprId, Span)>);
wrapped_list!(super::LoweredAssignPath, Vec<super::LoweredAssignStep>);
wrapped_list!(super::LoweredCompQualifiers, Vec<super::LoweredCompQualifier>);

macro_rules! struct_schema {
    ($ty:ident => $view:ident { $($field:ident: $field_ty:ty),* $(,)? }
     $(validate |$decoder:ident, $value:ident| $validation:block)?) => {
        pub(in crate::runtime::eval) struct $view<'a> {
            $(pub(in crate::runtime::eval) $field: <$field_ty as BorrowCodec>::View<'a>,)*
            program: PhantomData<&'a FullStore>,
        }
        impl BorrowCodec for $ty {
            type View<'a> = $view<'a>;
            fn borrow<'a>(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
            -> Result<Self::View<'a>, IrVerifyError> {
                Ok($view { $($field: <$field_ty>::borrow(context, input)?,)* program: PhantomData })
            }
        }
        impl $ty {
            pub(super) fn field_cursor<'a>(
                context: DecodeContext<'a>, mut input: FullCursor<'a>, name: &str,
            ) -> Result<FullCursor<'a>, IrVerifyError> {
                $(
                    if name == stringify!($field) { return Ok(input); }
                    <$field_ty>::borrow(context, &mut input)?;
                )*
                Err(IrVerifyError::new("payload field does not exist"))
            }
        }
        impl FullCodec for $ty {
            fn encode(&self, builder: &mut super::FullBuilder, output: &mut Vec<u32>)
            -> Result<(), super::IrBuildError> {
                $(self.$field.encode(builder, output)?;)*
                Ok(())
            }
            fn decode(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>)
            -> Result<Self, IrVerifyError> {
                let value = Self { $($field: <$field_ty>::decode(decoder, input)?,)* };
                $(let $decoder = decoder; let $value = &value; $validation)?
                Ok(value)
            }
        }
    };
}

macro_rules! encode_payload {
    ([] $builder:ident, $output:ident; [$($field:ident),*]) => {
        $($field.encode($builder, $output)?;)*
    };
    ([|$slot:ident, $encode_builder:ident, $encode_output:ident| $body:block]
     $builder:ident, $output:ident; [$field:ident]) => {
        let $slot = $field;
        let $encode_builder = $builder;
        let $encode_output = $output;
        $body
    };
}

macro_rules! borrow_field {
    ($context:ident, $input:ident; $ty:ty) => { <$ty>::borrow($context, $input)? };
    ($context:ident, $input:ident; $ty:ty as $wire:ty) => {
        <$wire>::borrow($context, $input)? as $ty
    };
}

macro_rules! decode_field {
    ($decoder:ident, $input:ident; $ty:ty) => { <$ty>::decode($decoder, $input)? };
    ($decoder:ident, $input:ident; $ty:ty as $wire:ty) => {
        <$wire>::decode($decoder, $input)? as $ty
    };
}

macro_rules! enum_schema {
    ($ty:ident => $view:ident [$message:literal] {
        $(
            $variant:ident
            $(($($tuple_field:ident: $tuple_ty:ty $(as $wire_ty:ty)?),* $(,)?))?
            $({$($field:ident: $field_ty:ty),* $(,)?})?
            = $tag:literal $(encode |$slot:ident, $encode_builder:ident, $encode_output:ident| $encode:block)?;
        )*
    }) => {
        pub(in crate::runtime::eval) enum $view<'a> {
            $(
                $variant $(($(<$tuple_ty as BorrowCodec>::View<'a>),*))?
                $({$($field: <$field_ty as BorrowCodec>::View<'a>),*})?,
            )*
        }
        impl BorrowCodec for $ty {
            type View<'a> = $view<'a>;
            fn borrow<'a>(context: DecodeContext<'a>, input: &mut FullCursor<'a>)
            -> Result<Self::View<'a>, IrVerifyError> {
                Ok(match input.raw()? {
                    $(
                        $tag => {
                            $($(let $tuple_field = borrow_field!(context, input; $tuple_ty $(as $wire_ty)?);)*)?
                            $($(let $field = <$field_ty>::borrow(context, input)?;)*)?
                            $view::$variant $(($($tuple_field),*))? $({$($field),*})?
                        }
                    )*
                    _ => return Err(IrVerifyError::new($message)),
                })
            }
        }
        impl FullCodec for $ty {
            fn encode(&self, builder: &mut super::FullBuilder, output: &mut Vec<u32>)
            -> Result<(), super::IrBuildError> {
                match self {
                    $(
                        Self::$variant $(($($tuple_field),*))? $({$($field),*})? => {
                            output.push($tag);
                            encode_payload!([$(|$slot, $encode_builder, $encode_output| $encode)?]
                                builder, output; [$($($tuple_field),*)? $($($field),*)?]);
                        }
                    )*
                }
                Ok(())
            }
            fn decode(decoder: &FullDecoder<'_>, input: &mut FullCursor<'_>)
            -> Result<Self, IrVerifyError> {
                Ok(match input.raw()? {
                    $(
                        $tag => {
                            $($(let $tuple_field = decode_field!(decoder, input; $tuple_ty $(as $wire_ty)?);)*)?
                            $($(let $field = <$field_ty>::decode(decoder, input)?;)*)?
                            Self::$variant $(($($tuple_field),*))? $({$($field),*})?
                        }
                    )*
                    _ => return Err(IrVerifyError::new($message)),
                })
            }
        }
    };
}

struct_schema!(LoweredRunArg => RunArgView {
    kind: LoweredRunArgKind, span: Span,
});
struct_schema!(LoweredRunEnv => RunEnvView {
    name: Name, value: LoweredRunArg,
});
struct_schema!(LoweredRunRedirection => RunRedirectionView {
    kind: RedirectionKind, target: LoweredRunArg, span: Span,
});
struct_schema!(LoweredRunPipelineSegment => RunPipelineSegmentView {
    kind: RunKind, target: LoweredRunArg, args: Vec<LoweredRunArg>, env: Vec<LoweredRunEnv>,
    redirections: Vec<LoweredRunRedirection>, timeout: Option<BuildExprId>,
    cpu_max: Option<BuildExprId>, accept: Option<BuildExprId>,
});
struct_schema!(LoweredProcessCommandArgv => ProcessCommandArgvView {
    target: BuildExprId, argv: BuildExprId, cwd: Option<BuildExprId>, env: Option<BuildExprId>,
    stdin: Option<BuildExprId>, stdout: Option<BuildExprId>, stderr: Option<BuildExprId>,
    stdout_append: Option<BuildExprId>, stderr_append: Option<BuildExprId>,
    timeout: Option<BuildExprId>, detach: Option<BuildExprId>, new_session: Option<BuildExprId>,
    ignore_hup: Option<BuildExprId>, cpu_max: Option<BuildExprId>, accept: Option<BuildExprId>,
    same_group: Option<BuildExprId>, span: Span,
});
struct_schema!(LoweredRunCapture => RunCaptureView {
    kind: RunKind, target: Box<LoweredRunArg>, args: Vec<LoweredRunArg>, env: Vec<LoweredRunEnv>,
    redirections: Vec<LoweredRunRedirection>, timeout: Option<BuildExprId>,
    cpu_max: Option<BuildExprId>, accept: Option<BuildExprId>, propagate: bool,
    assert_success: bool, span: Span,
});
struct_schema!(LoweredSpawnRun => SpawnRunView {
    target: Box<LoweredRunArg>, args: Vec<LoweredRunArg>, env: Vec<LoweredRunEnv>,
    redirections: Vec<LoweredRunRedirection>, timeout: Option<BuildExprId>,
    cpu_max: Option<BuildExprId>, accept: Option<BuildExprId>, span: Span,
});
struct_schema!(LoweredTypeCheck => TypeCheckView {
    ty: Type, name: Arc<str>, schema: Option<Arc<super::super::super::require::PreparedSchema>>,
} validate |decoder, check| {
    if !decoder.verified && check.schema.as_ref().is_some_and(|schema| !schema.matches_type(&check.ty)) {
        return Err(IrVerifyError::new("prepared schema does not match its checked type"));
    }
});
struct_schema!(ScanBytes => ScanBytesView {
    line_slot: usize, block_depth_slot: usize, code_seen_slot: usize, comment_seen_slot: usize,
    in_string_slot: usize, string_delim_slot: usize, escaped_slot: usize, nested: bool, span: Span,
});
enum_schema!(LoweredRunArgKind => RunArgKindView ["run argument tag is invalid"] {
    Single(value: BuildExprId) = 0;
    SingleOrSplice(value: BuildExprId) = 1;
    Splice(value: BuildExprId) = 2;
});
enum_schema!(LoweredCallArg => CallArgView ["call argument tag is invalid"] {
    Default(slot: usize as u32) = 2 encode |slot, builder, output| {
        // This is a callable parameter index, not a caller frame slot.
        u32::try_from(*slot)
            .map_err(|_| super::IrBuildError::format("argument_slot_overflow", None, 0, 0))?
            .encode(builder, output)?;
    };
    Single(value: BuildExprId) = 0;
    Splice(value: BuildExprId) = 1;
});
enum_schema!(LoweredFmtPart => FmtPartView ["format part tag is invalid"] {
    Text(value: Arc<str>) = 0;
    Expr(value: BuildExprId, span: Span, spec: Option<FormatSpec>) = 1;
});
enum_schema!(LoweredRecordEntry => RecordEntryView ["record entry tag is invalid"] {
    Field(name: Name, value: BuildExprId) = 0;
    Spread(value: BuildExprId) = 1;
});
enum_schema!(LoweredAssignStep => AssignStepView ["assignment path step tag is invalid"] {
    Field(name: Name) = 0;
    Index(value: BuildExprId) = 1;
});
enum_schema!(LoweredCompTarget => CompTargetView ["comprehension target tag is invalid"] {
    Discard = 2;
    Slot(slot: usize) = 0;
    Record { fields: super::super::super::LoweredCompFields } = 1;
});
enum_schema!(LoweredCompQualifier => CompQualifierView ["comprehension qualifier tag is invalid"] {
    For { target: Box<LoweredCompTarget>, iter: BuildExprId, span: Span } = 0;
    If { condition: BuildExprId, span: Span } = 1;
});
enum_schema!(LoweredProcessCommandBuilderEntry => ProcessCommandBuilderEntryView ["process command builder entry tag is invalid"] {
    Field { name: Name, value: BuildExprId, span: Span } = 0;
    Run { target: LoweredRunArg, args: Vec<LoweredRunArg>, env: Vec<LoweredRunEnv>,
        timeout: Option<BuildExprId>, cpu_max: Option<BuildExprId>, accept: Option<BuildExprId>,
        span: Span } = 1;
});
enum_schema!(LoweredErrorExpr => ErrorExprView ["error expression tag is invalid"] {
    Structured { family: String, variant: String, fields: Vec<(Arc<str>, BuildExprId)>,
        facets: Vec<Name> } = 1;
});
