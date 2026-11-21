use super::*;
use crate::runtime::eval::indexed::full::wire_shape::{CustomValueShape, PayloadShape, PayloadWords, WordRole};
use crate::runtime::eval::indexed::semantic::RawSemanticTables;

fn word(role: WordRole) -> PayloadShape {
    PayloadShape::Word(role)
}

fn closed(count: u32) -> PayloadShape {
    PayloadShape::Choice((0..count).map(|tag| (tag, PayloadShape::Fields(Vec::new()))).collect())
}

pub(super) fn codec_width<T: FullCodec>(operands: &[u32]) -> u32 {
    let store = FullStore::default();
    let decoder = stage_view_decoder(&store);
    let width = (0u32..).take_while(|tag| {
        let words = std::iter::once(*tag).chain(operands.iter().copied()).collect::<Vec<_>>();
        T::decode(&decoder, &mut FullCursor::new(&words)).is_ok()
    }).count();
    u32::try_from(width).expect("a closed codec decodes fewer than u32::MAX tags")
}

fn shape<T: PayloadWords>() -> PayloadShape {
    T::shape()
}

macro_rules! word_shapes {
    ($($ty:ty => $role:ident),* $(,)?) => {$(
        impl PayloadWords for $ty {
            fn shape() -> PayloadShape { word(WordRole::$role) }
        }
    )*};
}

word_shapes! {
    u32 => Raw,
    usize => Slot,
    Name => Symbol,
    LoweredFunctionKey => Function,
    Span => Location,
    Arc<str> => String,
    NameText => String,
    String => String,
    Arc<[u8]> => Bytes,
    Vec<u8> => Bytes,
    PathValue => Bytes,
    PreparedConstantValue => Constant,
    Arc<crate::runtime::eval::ModuleCallPlan> => ModulePlan,
    Arc<crate::sema::wire_enums::WireEnumMapping> => WireEnum,
    RegexValue => Regex,
    Type => Type,
    LoweredType => LoweredType,
    Arc<crate::runtime::eval::require::PreparedSchema> => Schema,
    crate::map_key::MapKey => Value,
    LoweredValue => Value,
    LoweredPipelineStage => Stage,
    BuildExprId => Expr,
    BuildStmtId => Stmt,
    BuildIntId => Int,
    BuildBoolId => Bool,
    BuildPatternId => Pattern,
    crate::runtime::eval::EndDistance => EndDistance,
    crate::runtime::eval::DeferKind => DeferKind,
    RuntimeOp => RuntimeOp,
    AssignOp => AssignOp,
    BinaryOp => BinaryOp,
    RunKind => RunKind,
    RedirectionKind => RedirectionKind,
}

macro_rules! double_word_shapes {
    ($($ty:ty),* $(,)?) => {$(
        impl PayloadWords for $ty {
            fn shape() -> PayloadShape { PayloadShape::Fields(vec![word(WordRole::Raw), word(WordRole::Raw)]) }
        }
    )*};
}

double_word_shapes!(i64, u64, FloatValue, DurationValue);

macro_rules! closed_shapes {
    ($($ty:ty),* $(,)?) => {$(
        impl PayloadWords for $ty {
            fn shape() -> PayloadShape {
                static WIDTH: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
                closed(*WIDTH.get_or_init(|| codec_width::<$ty>(&[])))
            }
        }
    )*};
}

closed_shapes! {
    bool,
    LoweredStrPredicate,
    ReduceByOp,
    HashAlgorithm,
    crate::syntax::arena::UnknownFields,
    crate::syntax::arena::ContextScopeKind,
    crate::syntax::arena::RetrySchedule,
    CallableKind,
}

impl PayloadWords for QualifiedName {
    fn shape() -> PayloadShape {
        PayloadShape::Fields(vec![shape::<Name>(), shape::<Name>()])
    }
}

impl PayloadWords for FunctionName {
    fn shape() -> PayloadShape {
        PayloadShape::Choice(vec![
            (0, PayloadShape::Fields(vec![shape::<Name>()])),
            (1, shape::<QualifiedName>()),
        ])
    }
}

impl PayloadWords for FormatSpec {
    fn shape() -> PayloadShape {
        static WIDTH: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
        PayloadShape::Fields(vec![closed(*WIDTH.get_or_init(|| codec_width::<FormatSpec>(&[0]))), word(WordRole::Raw)])
    }
}

impl PayloadWords for LoweredModuleExport {
    fn shape() -> PayloadShape {
        PayloadShape::Fields(vec![shape::<Name>(), PayloadShape::Choice(vec![
            (0, PayloadShape::Fields(vec![word(WordRole::BindingIdentity)])),
            (3, PayloadShape::Fields(vec![shape::<CallableKind>(), shape::<Name>()])),
            (4, PayloadShape::Fields(vec![shape::<CallableKind>(), shape::<Name>()])),
        ])])
    }
}

impl PayloadWords for crate::runtime::eval::IndexOperand<BuildExprId> {
    fn shape() -> PayloadShape {
        PayloadShape::Choice(vec![
            (0, PayloadShape::Fields(vec![shape::<BuildExprId>()])),
            (1, PayloadShape::Fields(vec![shape::<crate::runtime::eval::EndDistance>()])),
        ])
    }
}

impl PayloadWords for LoweredValueCheck {
    fn shape() -> PayloadShape {
        PayloadShape::Choice(vec![
            (0, shape::<LoweredTypeCheck>()),
            (1, PayloadShape::Fields(vec![word(WordRole::Materialization)])),
            (2, PayloadShape::Fields(vec![word(WordRole::SchemaExpectation)])),
            (3, PayloadShape::Fields(vec![word(WordRole::Raw)])),
            (4, PayloadShape::Fields(vec![word(WordRole::Raw)])),
            (5, PayloadShape::Fields(Vec::new())),
        ])
    }
}

impl<T: PayloadWords> PayloadWords for Option<T> {
    fn shape() -> PayloadShape {
        PayloadShape::Choice(vec![(0, PayloadShape::Fields(Vec::new())), (1, T::shape())])
    }
}

impl<T: PayloadWords> PayloadWords for Box<T> {
    fn shape() -> PayloadShape {
        T::shape()
    }
}

impl<A: PayloadWords, B: PayloadWords> PayloadWords for (A, B) {
    fn shape() -> PayloadShape {
        PayloadShape::Fields(vec![A::shape(), B::shape()])
    }
}

impl<A: PayloadWords, B: PayloadWords, C: PayloadWords> PayloadWords for (A, B, C) {
    fn shape() -> PayloadShape {
        PayloadShape::Fields(vec![A::shape(), B::shape(), C::shape()])
    }
}

impl<A: smallvec::Array> PayloadWords for SmallVec<A> where A::Item: PayloadWords {
    fn shape() -> PayloadShape {
        PayloadShape::Repeat(Box::new(A::Item::shape()))
    }
}

impl<K: PayloadWords, V: PayloadWords> PayloadWords for BTreeMap<K, V> {
    fn shape() -> PayloadShape {
        PayloadShape::Repeat(Box::new(PayloadShape::Fields(vec![K::shape(), V::shape()])))
    }
}

impl<V: PayloadWords> PayloadWords for FxHashMap<Arc<str>, V> {
    fn shape() -> PayloadShape {
        PayloadShape::Repeat(Box::new(PayloadShape::Fields(vec![shape::<Arc<str>>(), V::shape()])))
    }
}

impl PayloadWords for LoweredRecordUpdates {
    fn shape() -> PayloadShape {
        shape::<Vec<(Vec<Name>, BuildExprId, Span)>>()
    }
}

impl PayloadWords for LoweredAssignPath {
    fn shape() -> PayloadShape {
        shape::<Vec<LoweredAssignStep>>()
    }
}

impl PayloadWords for LoweredCompQualifiers {
    fn shape() -> PayloadShape {
        shape::<Vec<LoweredCompQualifier>>()
    }
}

impl CustomValueShape for FullValueTag {
    fn custom_payload_shape(self) -> Vec<PayloadShape> {
        match self {
            FullValueTag::Set => vec![PayloadShape::Repeat(Box::new(shape::<crate::map_key::MapKey>()))],
            other => panic!("value tag {other:?} has a declared payload, not a custom one"),
        }
    }
}

pub(super) struct PayloadScan<'a> {
    extra: &'a [u32],
    blocks: &'a [FullBlock],
    symbols: BTreeSet<usize>,
}

impl<'a> PayloadScan<'a> {
    fn new(extra: &'a [u32], blocks: &'a [FullBlock]) -> Self {
        Self { extra, blocks, symbols: BTreeSet::new() }
    }

    fn take(&self, cursor: &mut usize, end: usize) -> Result<u32, String> {
        if *cursor >= end {
            return Err("payload ends before its shape".into());
        }
        let word = self.extra[*cursor];
        *cursor += 1;
        Ok(word)
    }

    fn block(&self, raw: u32, flags: u8) -> Result<(usize, usize), String> {
        let block = raw.checked_sub(1).and_then(|index| self.blocks.get(index as usize))
            .ok_or_else(|| format!("block {raw} is not in the block table"))?;
        if block.flags & 1 != flags {
            return Err(format!("block {raw} has sequence kind {} where {flags} is required", block.flags & 1));
        }
        let start = block.instructions.start as usize;
        let end = start.checked_add(block.instructions.len as usize).filter(|end| *end <= self.extra.len())
            .ok_or_else(|| format!("block {raw} leaves the word table"))?;
        Ok((start, end))
    }

    fn exact(&mut self, shape: &PayloadShape, start: usize, end: usize) -> Result<(), String> {
        let mut cursor = start;
        self.shape(shape, &mut cursor, end)?;
        if cursor != end {
            return Err(format!("words {cursor}..{end} follow a complete payload"));
        }
        Ok(())
    }

    fn shape(&mut self, shape: &PayloadShape, cursor: &mut usize, end: usize) -> Result<(), String> {
        match shape {
            PayloadShape::Word(role) => {
                let at = *cursor;
                self.take(cursor, end)?;
                if *role == WordRole::Symbol {
                    self.symbols.insert(at);
                }
            }
            PayloadShape::Fields(fields) => {
                for field in fields {
                    self.shape(field, cursor, end)?;
                }
            }
            PayloadShape::Choice(variants) => {
                let tag = self.take(cursor, end)?;
                let (_, variant) = variants.iter().find(|(candidate, _)| *candidate == tag)
                    .ok_or_else(|| format!("choice tag {tag} is not declared"))?;
                self.shape(variant, cursor, end)?;
            }
            PayloadShape::Repeat(element) => {
                let count = self.take(cursor, end)? as usize;
                if count > end - *cursor {
                    return Err(format!("{count} repeated elements cannot fit {} words", end - *cursor));
                }
                for _ in 0..count {
                    self.shape(element, cursor, end)?;
                }
            }
            PayloadShape::Block(flags, element) => {
                let raw = self.take(cursor, end)?;
                let (start, block_end) = self.block(raw, *flags)?;
                self.exact(element, start, block_end)?;
            }
            PayloadShape::Named(name, definition) => self.shape(&definition(), cursor, end).map_err(|error| format!("{name}: {error}"))?,
            PayloadShape::TextPattern => {
                let holes = PayloadShape::Block(BLOCK_LIST, Box::new(PayloadShape::Repeat(Box::new(word(WordRole::Pattern)))));
                let raw = self.extra.get(*cursor).copied().filter(|_| *cursor < end)
                    .ok_or_else(|| "text pattern has no hole list".to_string())?;
                let (start, block_end) = self.block(raw, BLOCK_LIST)?;
                if start == block_end {
                    return Err("text pattern hole list has no count".into());
                }
                let count = self.extra[start] as usize;
                self.shape(&holes, cursor, end)?;
                let tail = PayloadShape::Fields(std::iter::repeat_n(word(WordRole::TextHole), count)
                    .chain(std::iter::repeat_n(word(WordRole::String), count + 1)).collect());
                self.shape(&tail, cursor, end)?;
            }
        }
        Ok(())
    }

    fn row(&mut self, fields: Vec<PayloadShape>, data: IrData, row: &str) -> Result<(), String> {
        let start = data.lhs as usize;
        let end = start.checked_add(data.rhs as usize).filter(|end| *end <= self.extra.len())
            .ok_or_else(|| format!("{row} payload leaves the word table"))?;
        self.exact(&PayloadShape::Fields(fields), start, end).map_err(|error| format!("{row}: {error}"))
    }
}

pub(super) fn extra_symbol_positions(rows: &RawFullRows) -> Result<BTreeSet<usize>, String> {
    let mut scan = PayloadScan::new(&rows.extra, &rows.blocks);
    if rows.data.len() != rows.tags.len() || rows.pattern_data.len() != rows.patterns.len()
        || rows.stage_data.len() != rows.stages.len() || rows.value_data.len() != rows.values.len() {
        return Err("a row table and its payload table differ in length".into());
    }
    for (index, (tag, data)) in rows.tags.iter().zip(&rows.data).enumerate() {
        scan.row(tag.payload_shape(), *data, &format!("instruction {index} {tag:?}"))?;
    }
    for (index, (tag, data)) in rows.patterns.iter().zip(&rows.pattern_data).enumerate() {
        scan.row(tag.payload_shape(), *data, &format!("pattern {index} {tag:?}"))?;
    }
    for (index, (tag, data)) in rows.stages.iter().zip(&rows.stage_data).enumerate() {
        scan.row(tag.payload_shape(), *data, &format!("stage {index} {tag:?}"))?;
    }
    for (index, (tag, data)) in rows.values.iter().zip(&rows.value_data).enumerate() {
        scan.row(tag.payload_shape(), *data, &format!("value {index} {tag:?}"))?;
    }
    for (index, step) in rows.driver_steps.iter().enumerate() {
        scan.row(step.tag.payload_shape(), step.data, &format!("driver step {index} {:?}", step.tag))?;
    }
    Ok(scan.symbols)
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum SemanticSymbol {
    TypeLhs(usize),
    TypeRhs(usize),
    SignatureWord(usize),
}

pub(super) fn semantic_symbol_positions(tables: &RawSemanticTables) -> Result<Vec<SemanticSymbol>, String> {
    use crate::runtime::eval::indexed::semantic::TypeTag;
    if tables.type_tags.len() != tables.type_data.len() {
        return Err("semantic type tags and data differ in length".into());
    }
    let mut positions = Vec::new();
    for (index, tag) in tables.type_tags.iter().enumerate() {
        match tag {
            TypeTag::ErrorFamily | TypeTag::ErrorFacet | TypeTag::Tag => positions.push(SemanticSymbol::TypeLhs(index)),
            TypeTag::ErrorVariant => positions.extend([SemanticSymbol::TypeLhs(index), SemanticSymbol::TypeRhs(index)]),
            TypeTag::Nominal => positions.push(SemanticSymbol::TypeRhs(index)),
            _ => {}
        }
    }
    for (index, range) in tables.signature_data.iter().enumerate() {
        let start = range.lhs as usize;
        let payload = start.checked_add(range.rhs as usize).and_then(|end| tables.signature_extra.get(start..end))
            .ok_or_else(|| format!("signature {index} leaves the signature word table"))?;
        let [_, effects, params, ..] = payload else { return Err(format!("signature {index} has no header")) };
        let effects = if *effects == IR_NONE { 0 } else { *effects as usize };
        let params = *params as usize;
        if 3 + effects + 3 * params != payload.len() {
            return Err(format!("signature {index} has {} words for {effects} effects and {params} parameters", payload.len()));
        }
        positions.extend((0..params).map(|param| SemanticSymbol::SignatureWord(start + 3 + effects + 3 * param)));
    }
    Ok(positions)
}

impl SemanticSymbol {
    pub(super) fn word<'a>(self, tables: &'a mut RawSemanticTables) -> &'a mut u32 {
        match self {
            Self::TypeLhs(index) => &mut tables.type_data[index].lhs,
            Self::TypeRhs(index) => &mut tables.type_data[index].rhs,
            Self::SignatureWord(index) => &mut tables.signature_extra[index],
        }
    }
}
