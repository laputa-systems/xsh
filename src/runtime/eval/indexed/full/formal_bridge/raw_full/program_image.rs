use super::*;
use crate::runtime::eval::indexed::semantic::{RawSemanticTables, TypeTag};
use crate::runtime::eval::{CompactIndexedDriverStepPlan, CompactIndexedRunPlan, Evaluator, PreparedConstantOrigin, RuntimeErrorFamily};
use crate::symbol::SymbolOwner;
use std::num::NonZeroU32;

#[derive(Debug, Eq, PartialEq)]
pub(super) enum ImageError {
    Malformed(String),
    Unsupported(String),
}

pub(super) type ImageResult<T> = Result<T, ImageError>;

fn malformed<T>(message: impl Into<String>) -> ImageResult<T> {
    Err(ImageError::Malformed(message.into()))
}

fn unsupported<T>(message: impl Into<String>) -> ImageResult<T> {
    Err(ImageError::Unsupported(message.into()))
}

#[derive(Default)]
pub(super) struct ImageWriter {
    bytes: Vec<u8>,
    type_ids: BTreeMap<u64, u64>,
    effect_ids: BTreeMap<u64, u64>,
    owners: BTreeMap<u64, u64>,
}

impl ImageWriter {
    pub(super) fn position(&self) -> usize {
        self.bytes.len()
    }

    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn len(&mut self, len: usize) -> ImageResult<()> {
        let Ok(len) = u32::try_from(len) else { return unsupported("a pool is longer than u32") };
        self.u32(len);
        Ok(())
    }

    fn raw(&mut self, bytes: &[u8]) -> ImageResult<()> {
        self.len(bytes.len())?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
}

pub(super) struct ImageReader<'a> {
    input: &'a [u8],
    depth: u32,
    type_ids: BTreeMap<u64, crate::sema::constraints::TypeVariableId>,
    effect_ids: BTreeMap<u64, crate::sema::constraints::EffectVariableId>,
    owners: BTreeMap<u64, crate::sema::scheme_callable::SchemeCallableOwner>,
}

impl<'a> ImageReader<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self { input, depth: 0, type_ids: BTreeMap::new(), effect_ids: BTreeMap::new(), owners: BTreeMap::new() }
    }

    fn array<const N: usize>(&mut self) -> ImageResult<[u8; N]> {
        let Some((head, rest)) = self.input.split_first_chunk::<N>() else { return malformed("image ends early") };
        self.input = rest;
        Ok(*head)
    }

    fn u8(&mut self) -> ImageResult<u8> {
        Ok(self.array::<1>()?[0])
    }

    fn u16(&mut self) -> ImageResult<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    fn u32(&mut self) -> ImageResult<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn u64(&mut self) -> ImageResult<u64> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    fn len(&mut self) -> ImageResult<usize> {
        let len = self.u32()? as usize;
        if len > self.input.len() {
            return malformed("a pool is longer than the rest of the image");
        }
        Ok(len)
    }

    fn raw(&mut self) -> ImageResult<&'a [u8]> {
        let len = self.len()?;
        let (head, rest) = self.input.split_at(len);
        self.input = rest;
        Ok(head)
    }

    fn nested<T>(&mut self, read: impl FnOnce(&mut Self) -> ImageResult<T>) -> ImageResult<T> {
        if self.depth >= 64 {
            return malformed("image values nest too deeply");
        }
        self.depth += 1;
        let value = read(self);
        self.depth -= 1;
        value
    }

    fn finish(self) -> ImageResult<()> {
        if self.input.is_empty() { Ok(()) } else { malformed("image has trailing bytes") }
    }
}

pub(super) trait ImageCodec: Sized {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()>;
    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self>;
}

impl ImageCodec for u8 {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        output.u8(*self);
        Ok(())
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        input.u8()
    }
}

impl ImageCodec for u32 {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        output.u32(*self);
        Ok(())
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        input.u32()
    }
}

impl ImageCodec for u64 {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        output.u64(*self);
        Ok(())
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        input.u64()
    }
}

impl ImageCodec for i64 {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        output.bytes.extend_from_slice(&self.to_le_bytes());
        Ok(())
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        Ok(i64::from_le_bytes(input.array()?))
    }
}

impl ImageCodec for usize {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        let Ok(value) = u64::try_from(*self) else { return unsupported("a size exceeds u64") };
        output.u64(value);
        Ok(())
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        usize::try_from(input.u64()?).or_else(|_| malformed("a size exceeds usize"))
    }
}

impl ImageCodec for bool {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        output.u8(u8::from(*self));
        Ok(())
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        match input.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => malformed("a flag is not 0 or 1"),
        }
    }
}

impl ImageCodec for NonZeroU32 {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        output.u32(self.get());
        Ok(())
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        NonZeroU32::new(input.u32()?).map_or_else(|| malformed("a nonzero word is zero"), Ok)
    }
}

impl<T: ImageCodec + Copy + Default, const N: usize> ImageCodec for [T; N] {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        self.iter().try_for_each(|value| value.put(output))
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        let mut values = [T::default(); N];
        for value in &mut values {
            *value = T::take(input)?;
        }
        Ok(values)
    }
}

impl<T: ImageCodec> ImageCodec for Option<T> {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        match self {
            None => {
                output.u8(0);
                Ok(())
            }
            Some(value) => {
                output.u8(1);
                value.put(output)
            }
        }
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        match input.u8()? {
            0 => Ok(None),
            1 => Ok(Some(T::take(input)?)),
            _ => malformed("an option tag is not 0 or 1"),
        }
    }
}

impl<T: ImageCodec> ImageCodec for Vec<T> {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        output.len(self.len())?;
        self.iter().try_for_each(|value| value.put(output))
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        let len = input.len()?;
        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            values.push(T::take(input)?);
        }
        Ok(values)
    }
}

impl<T: ImageCodec> ImageCodec for Box<T> {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        self.as_ref().put(output)
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        Ok(Box::new(T::take(input)?))
    }
}

impl<T: ImageCodec> ImageCodec for Arc<T> {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        self.as_ref().put(output)
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        Ok(Arc::new(T::take(input)?))
    }
}

impl<T: ImageCodec> ImageCodec for Arc<[T]> {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        output.len(self.len())?;
        self.iter().try_for_each(|value| value.put(output))
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        Ok(Vec::<T>::take(input)?.into())
    }
}

impl ImageCodec for String {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        output.raw(self.as_bytes())
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        std::str::from_utf8(input.raw()?).map(str::to_owned).or_else(|_| malformed("text is not UTF-8"))
    }
}

impl ImageCodec for Arc<str> {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        output.raw(self.as_bytes())
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        std::str::from_utf8(input.raw()?).map(Arc::from).or_else(|_| malformed("text is not UTF-8"))
    }
}

impl<A: ImageCodec, B: ImageCodec> ImageCodec for (A, B) {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        self.0.put(output)?;
        self.1.put(output)
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        Ok((A::take(input)?, B::take(input)?))
    }
}

impl<K: ImageCodec + Ord, V: ImageCodec> ImageCodec for BTreeMap<K, V> {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        output.len(self.len())?;
        self.iter().try_for_each(|(key, value)| {
            key.put(output)?;
            value.put(output)
        })
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        let len = input.len()?;
        let mut values = BTreeMap::new();
        for _ in 0..len {
            let key = K::take(input)?;
            if values.insert(key, V::take(input)?).is_some() {
                return malformed("a map repeats a key");
            }
        }
        Ok(values)
    }
}

impl<K: ImageCodec + Ord> ImageCodec for BTreeSet<K> {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        output.len(self.len())?;
        self.iter().try_for_each(|key| key.put(output))
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        let len = input.len()?;
        let mut values = BTreeSet::new();
        for _ in 0..len {
            if !values.insert(K::take(input)?) {
                return malformed("a set repeats a key");
            }
        }
        Ok(values)
    }
}

impl ImageCodec for Name {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        output.u32(self.symbol().raw());
        Ok(())
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        Ok(Name::from_symbol(Symbol::from_raw(input.u32()?)))
    }
}

impl ImageCodec for FunctionName {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        if let Some(name) = self.as_name() {
            output.u8(0);
            name.put(output)
        } else if let Some(name) = self.as_qualified() {
            output.u8(1);
            name.namespace.put(output)?;
            name.member.put(output)
        } else {
            unsupported("function name has no spelling")
        }
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        match input.u8()? {
            0 => Ok(FunctionName::name(Name::take(input)?)),
            1 => Ok(FunctionName::qualified(QualifiedName::new(Name::take(input)?, Name::take(input)?))),
            _ => malformed("function name tag is invalid"),
        }
    }
}

impl ImageCodec for SourceId {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        let Ok(raw) = u32::try_from(self.raw()) else { return unsupported("source id exceeds u32") };
        output.u32(raw);
        Ok(())
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        Ok(SourceId::new(input.u32()? as usize))
    }
}

impl ImageCodec for Span {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        self.source_id.put(output)?;
        self.start().put(output)?;
        self.end().put(output)
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        let source = SourceId::take(input)?;
        let start = usize::take(input)?;
        let end = usize::take(input)?;
        if end < start {
            return malformed("span ends before it starts");
        }
        Ok(Span::new(source, start, end))
    }
}

macro_rules! image_ir_ids {
    ($($ty:ty),* $(,)?) => {$(
        impl ImageCodec for $ty {
            fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
                output.u32(self.raw());
                Ok(())
            }

            fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
                <$ty>::from_raw(input.u32()?).map_or_else(|| malformed(concat!(stringify!($ty), " is invalid")), Ok)
            }
        }
    )*};
}

image_ir_ids!(TypeId, SignatureId, IrFunctionId);

macro_rules! image_scheme_ids {
    ($($ty:ty => $raw:ident),* $(,)?) => {$(
        impl ImageCodec for $ty {
            fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
                let Some(raw) = u32::try_from(self.$raw()).ok() else { return unsupported(concat!(stringify!($ty), " exceeds u32")) };
                output.u32(raw);
                Ok(())
            }

            fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
                <$ty>::from_raw(input.u32()?).or_else(|error| malformed(error.message.clone()))
            }
        }
    )*};
}

image_scheme_ids!(
    SchemeTypeId => raw,
    FullSchemeId => index,
    schemes::FullSchemeSchemaExpectationId => raw,
);

macro_rules! image_unit_enums {
    ($($ty:ty { $($variant:ident),* $(,)? })*) => {$(
        impl ImageCodec for $ty {
            fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
                type This = $ty;
                output.u8(match self { $(This::$variant => This::$variant as u8,)* });
                Ok(())
            }

            fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
                type This = $ty;
                let raw = input.u8()?;
                $(
                    if raw == This::$variant as u8 {
                        return Ok(This::$variant);
                    }
                )*
                malformed(concat!(stringify!($ty), " tag is invalid"))
            }
        }
    )*};
}

image_unit_enums! {
    FullValueTag { Null, Unit, Int, Float, Duration, Bool, Str, Bytes, Regex, Path, Record, RecordVec, Stats, StatsBlob, Module, List, Map, Tag, ResultOk, Set }
    FullStageTag {
        TextLines, JsonLines, Where, WhereBlock, Map, MapBlock, FlatMap, FlatMapBlock, BytesChunks, BatchCount, BatchMaxArgv,
        BatchMaxBytes, BatchLimits, Shuffle, Fold, ReduceBy, ReduceByConfigured, ParMap, ParMapBlock, ParMapFlatMapReduceBy, Tee,
        Each, TablePrint, TablePrintConfigured, Enumerate, Zip, Sort, SortBy, GroupBy, CountBy, Any, AnyBlock, All, AllBlock,
        UniqueBy, Count, Sum, Collect, First, Last, Min, Max, Take, Drop, Repeat, Range,
    }
    FullDriverTag { Skip, Use, Let, LetRecord, Assign, Discard, Stmt, Expr, Defer, SignalHook }
    TypeTag {
        Any, Null, Bool, Int, Float, Duration, Str, Bytes, Digest, Regex, Path, List, Map, Stream, Record, Module, Result, Status,
        EnvPathList, Error, ErrorFamily, ErrorVariant, ErrorFacet, ProcessError, Function, Command, ProcessHandle, NetJob, FsRoot,
        Unit, Tag, Optional, UInt, ErasedRecord, DynamicModule, ExactModule, Union, Callable, Validated, Nominal, Set, Bounded,
        FsLock, SchemeCallable, SshSession,
    }
    crate::runtime::eval::indexed::full::schema::FullSchemeTypeTag {
        Ground, SchemeCallable, Parameter, Callable, List, Map, Stream, Set, Result, Optional, Record, Union, Validated,
    }
    crate::runtime::eval::indexed::full::schema::FullSchemeValidationTag { NonEmpty, RelPath, Nominal, Range }
    crate::runtime::eval::indexed::full::schema::FullSchemeSchemaComponentTag { Field, Item, Value, Key, Optional, Success, Error }
    value_materialization::FullMaterializationKind { Keep, Callable, Record, List, MapValues, Optional, Result }
    CallableKind { Pure, Proc, Stream }
    crate::sema::callable_application::CallableKindRequirement { Pure, Effectful }
    crate::sema::schemes::IterationSourceKind { Direct, DirectOrResult }
    crate::sema::schemes::InstantiationRole { Call, Value }
    crate::sema::sealed_operation::TypeDomain { SortKey, CountKey, CollectItem, MinMaxItem, OrderingOperand, NegationOperand }
    crate::sema::sealed_operation::SliceBounds { Unbounded, Start, End, Both }
    xsh_registry::signature::ArithmeticOperator { Add, Subtract, Multiply, Divide, Remainder }
    xsh_registry::signature::ArithmeticSignature { Integer, Float, StringConcatenation, ListConcatenation, SetDifference, DurationPair, DurationScale, IntegerScale, DurationRatio }
    crate::sema::sealed_operation::MembershipDomain { List, Set, Map, UnsignedMap, Record, Str, Bytes, Path, EnvPath }
    crate::sema::wrapper_operation::WrapperSelection { Optional, Result, Capture, KeyValidation, ProcCompletion }
    crate::syntax::arena::UnknownFields { Preserve, Reject, Strip }
    xsh_registry::types::BuiltinTypeParameter { Receiver, Element, Key, Value, Error }
    crate::modules::MethodReceiver { PathConstructor, Result, EnvPathList, Path, Int, Float, List, Map, Record, Stream, Str, Bytes, Status, Digest, Regex, ProcessHandle, NetJob, FsRoot, NonEmpty, RelPath, Set }
    CheckedPatternVariantKind { Tag, Error }
    Effect { Fs, Net, Process, Env, Time, Error, Io }
    AssignOp { Set, Add, Sub, Mul, Div, Rem }
    BinaryOp { ResultFallback, Or, And, Eq, Ne, Lt, Le, Gt, Ge, In, NotIn, Add, Sub, Mul, Div, Rem, Union, Intersect }
    RunKind { Plain, Status, CaptureText, CaptureBytes, CaptureTextRecord, CaptureBytesRecord, StreamText, StreamBytes }
    RedirectionKind { StdoutWrite, StdoutAppend, StdinRead, StderrWrite, StderrAppend, StdoutDup, StdinDup }
}

impl ImageCodec for FullTag {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        output.u8(*self as u8);
        Ok(())
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        static TAGS: std::sync::OnceLock<[Option<FullTag>; 256]> = std::sync::OnceLock::new();
        let tags = TAGS.get_or_init(|| {
            let mut tags = [None; 256];
            for tag in schema::INSTRUCTION_TAGS {
                tags[*tag as usize] = Some(*tag);
            }
            tags
        });
        tags[usize::from(input.u8()?)].map_or_else(|| malformed("instruction tag is invalid"), Ok)
    }
}

impl ImageCodec for FullPatternTag {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        output.u8(*self as u8);
        Ok(())
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        let raw = input.u8()?;
        schema::PATTERN_TAGS.iter().copied().find(|tag| *tag as u8 == raw).map_or_else(|| malformed("pattern tag is invalid"), Ok)
    }
}

fn registered_runtime_ops() -> &'static FxHashMap<u16, RuntimeOp> {
    static OPS: std::sync::OnceLock<FxHashMap<u16, RuntimeOp>> = std::sync::OnceLock::new();
    OPS.get_or_init(|| {
        let spec = crate::modules::api_spec();
        let functions = spec.module_entries().flat_map(|(_, module)| module.functions.iter())
            .flat_map(|function| function.overloads.iter().map(|overload| overload.op));
        let methods = spec.method_entries().flat_map(|(_, methods)| methods.iter())
            .flat_map(|method| method.overloads.iter().map(|overload| overload.sig.op));
        functions.chain(methods).map(|op| (op as u16, op)).collect()
    })
}

impl ImageCodec for RuntimeOp {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        let raw = *self as u16;
        if registered_runtime_ops().get(&raw) != Some(self) {
            return unsupported(format!("runtime operation {self:?} has no registered signature"));
        }
        output.u16(raw);
        Ok(())
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        registered_runtime_ops().get(&input.u16()?).copied().map_or_else(|| malformed("runtime operation is not registered"), Ok)
    }
}

impl ImageCodec for crate::sema::schemes::MethodRequirementKind {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        match self {
            Self::Family(family) => {
                output.u8(0);
                output.u8(family.raw());
            }
            Self::Named(named) => {
                output.u8(1);
                output.u16(named.raw());
            }
            Self::Function(function) => {
                output.u8(2);
                output.u16(function.raw());
            }
        }
        Ok(())
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        match input.u8()? {
            0 => xsh_registry::signature::MethodFamilyId::from_raw(input.u8()?).map(Self::Family).map_or_else(|| malformed("method family is invalid"), Ok),
            1 => Ok(Self::Named(xsh_registry::signature::NamedMethodId::from_raw(input.u16()?))),
            2 => Ok(Self::Function(xsh_registry::signature::OverloadedFunctionId::from_raw(input.u16()?))),
            _ => malformed("method requirement tag is invalid"),
        }
    }
}

macro_rules! image_structs {
    ($($ty:ty { $($field:ident),* $(,)? })*) => {$(
        impl ImageCodec for $ty {
            fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
                type This = $ty;
                let This { $($field),* } = self;
                $(ImageCodec::put($field, output)?;)*
                Ok(())
            }

            fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
                type This = $ty;
                Ok(This { $($field: ImageCodec::take(input)?),* })
            }
        }
    )*};
}

macro_rules! image_field {
    ($field:ident, $input:ident) => {
        ImageCodec::take($input)?
    };
}

macro_rules! image_enums {
    ($($ty:ty {
        $($variant:ident $(($($tuple:ident),*))? $({$($named:ident),*})? = $index:literal),* $(,)?
    })*) => {$(
        impl ImageCodec for $ty {
            fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
                type This = $ty;
                match self {
                    $(This::$variant $(($($tuple),*))? $({$($named),*})? => {
                        output.u8($index);
                        $($(ImageCodec::put($tuple, output)?;)*)?
                        $($(ImageCodec::put($named, output)?;)*)?
                    })*
                }
                Ok(())
            }

            fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
                type This = $ty;
                match input.u8()? {
                    $($index => Ok(This::$variant $(($(image_field!($tuple, input)),*))? $({$($named: ImageCodec::take(input)?),*})?),)*
                    _ => malformed(concat!(stringify!($ty), " tag is invalid")),
                }
            }
        }
    )*};
}

image_structs! {
    crate::sema::types::TypedCallable { kind, sig }
    crate::sema::types::CallableType { params, return_ty, effects }
    crate::sema::types::CallableParamType { name, ty, defaulted, rest }
    crate::sema::types::ModuleType { exports, exact }
    IrData { lhs, rhs }
    IrRange { start, len }
    IrLocation { start, len }
    FullBlock { instructions, result, owner, flags, reserved }
    FullDriverStep { data, slots, writers, instruction_start, slot_count, location, effects, registered_module_type, tag, reserved }
    FullDriverSlot { name, type_id, slot, binding_identity, flags, reserved }
    FullDriverWriter { binding_identity, slot }
    FullDriverSync { name, type_id, flags, reserved }
    FullDriverRegion { steps, sync, effects }
    FullDriverProgram { steps, regions, effects, owner }
    FullBindingIdentity { name, location, owner, flags, reserved }
    FullFunction { name, contract, params, captures, body, slot_count, slot_type_facts, checked_return_type }
    FullCheckedExprType { instruction, type_id, purpose }
    FullCheckedCallTarget { instruction, function }
    CheckedPatternVariantKey { kind, owner, variant }
    FullPatternVariantSchema { fields }
    FullPatternFieldType { name, type_id }
    FullFunctionMetadata { owner, flags, callable_kind, reserved }
    FullParam { name, flags, reserved }
    FullParamCold { param, default, validation }
    FullCapture { name, type_id, slot_and_flags, binding_identity, owner, use_location }
    FullValidation { type_id, name }
    FullSchemeTypeNode { first, second, children, tag, reserved }
    schemes::FullScheme {
        type_parameters, row_parameters, effect_parameters, params, return_type, return_schema, effects, effect_predicates,
        applications, operations, methods, wrappers, guard_calls, body_facts, guards, callable_kind, explicit_return,
        inferred_effects, reserved,
    }
    schemes::FullEffectContract { direct, terms, known, reserved }
    schemes::FullEffectParameter { id, binder }
    schemes::FullEffectTerm { binder, mask, reserved }
    schemes::FullEffectArgument { binder, expression }
    schemes::FullSchemeInvocation { callee, result, effects, arguments, proc_returns, kind }
    schemes::FullApplicationArgument { ty, kind }
    schemes::FullProcReturnEnvelope { result, propagated_errors }
    schemes::FullSchemeApplicationReturn { instruction, function, application, envelope }
    schemes::FullSchemeTypeParameter { id, binder, iteration_item, iteration_error, map_key, conversions, iteration_source_kind, iteration_policy }
    schemes::FullSchemeRowParameter { id, binder, lacks }
    schemes::FullSchemeParam { ty, schema }
    schemes::FullSchemeCallableMonotype { params, return_type, return_schema, effects, kind, reserved }
    schemes::FullSchemeCallableParameter { name, ty, schema, flags, reserved }
    schemes::FullSchemeField { name, ty }
    schemes::FullSchemeValidation { name, low, high, tag, reserved }
    schemes::FullSchemeSchemaExpectation { instances, children, constructor_instruction, ground_return_function, constructor_template, constructor_defaults }
    schemes::FullSchemeSchemaDefinition { definition, parameters, result, context, defaults }
    schemes::FullSchemeSchemaInstance { definition, arguments }
    schemes::FullSchemeSchemaChild { component, reserved, name, expectation }
    schemes::FullCallInstantiation { instruction, arguments }
    schemes::FullInstantiation {
        type_arguments, row_arguments, effect_arguments, application_bindings, operation_bindings, method_bindings,
        wrapper_bindings, guard_bindings, guard_call_bindings,
    }
    schemes::FullWrapperField { name, boundary, effects }
    schemes::FullWrapperKeyValidation { boundary }
    schemes::FullWrapperHandler { error, handler }
    schemes::FullWrapperCapture { contributions, expected, reserved }
    schemes::FullWrapperCompletion { contributions }
    schemes::FullSchemeMethod { kind, receiver, argument_slots, arguments, result }
    schemes::FullSchemeApplicationCall { instruction, application }
    schemes::FullTypeArgument { binder, ty }
    schemes::FullRowArgument { binder, fields, tail }
    schemes::FullRowEvidenceField { name, ty }
    schemes::FullSchemeBodyFact { instruction, function, value }
    scheme_values::FullSchemeCallableValue { canonical, targets }
    scheme_values::FullSchemeCallableTarget { function, types, rows, effects }
    scheme_values::FullSchemeCallableCall { instruction, relationship }
    value_materialization::FullValueMaterialization { instruction, source, destination, root }
    value_materialization::FullMaterializationNode { kind, reserved, children, leaf }
    value_materialization::FullMaterializationEdge { name, node }
    value_materialization::FullMaterializationLeaf { relationship, instance }
    guard_schemes::FullGuardScope {
        id, parent, binding_ordinal, binding_starts, source, view, source_instruction, comparison_instruction, reads,
        type_parameters, row_parameters, effect_parameters, effect_predicates, applications, operations, methods, wrappers,
        guard_calls, effects, output,
    }
    guard_schemes::FullGuardOutput { outward, present, absent, instruction, nonnull_when }
    guard_schemes::FullGuardEffects { outward, local }
    guard_schemes::FullGuardCall { instruction, function, leaf, role, target }
    guard_schemes::FullGuardRead { source, view, instruction }
    crate::sema::wire_enums::WireEnumMapping { type_name, variants }
    LoweredTagValue { type_name, name, fields, wire }
    DurationValue { millis }
    PathValue { bytes }
}

image_enums! {
    crate::sema::types::ModuleExportType { Value { ty, optional } = 0, Callable { kind, sig, optional } = 1, GenericCallable { kind, optional } = 2 }
    crate::sema::validated::Validation { NonEmpty = 0, RelPath = 1, Nominal(name) = 2, Range(range) = 3 }
    crate::sema::schemes::RegisteredArgumentCheck { Unsigned(ty) = 0, UIntMapKey = 1 }
    schemes::FullOperationBinding { Selected(signature) = 0, Forwarded(ordinal) = 1 }
    schemes::FullMethodBinding { Selected(id) = 0, SelectedFunction(id) = 1, Forwarded(ordinal) = 2 }
    schemes::FullWrapperBinding { Selected(selection) = 0, Forwarded(ordinal) = 1 }
    crate::sema::sealed_operation::SealedOperationSignature {
        TypePredicate(domain) = 0, Binary(signature) = 1, Field = 2, IndexList = 3, IndexMap = 4,
        IndexRecord = 5, IndexModule = 6, StoreValue = 7, SliceList = 8, SliceStr = 9,
        SliceBytes = 10, RecordOverride = 11, TargetVariant = 12, Equality = 13,
        Membership(domain) = 14, MapKey = 15,
    }
    FullCallableContract { Ground(signature) = 0, Scheme(scheme) = 1 }
    FullCheckedExprPurpose { CheckedContract { function_ref } = 0, ProducerContext(owner) = 1, CapturedErrorContract(owner) = 2 }
    ExecutableOwner { Function(function) = 0, Driver(step) = 1 }
    schemes::FullEffectPredicate { Equal(left, right) = 0, Included(left, right) = 1, Covered(left, right) = 2, Excluded(left, mask) = 3 }
    schemes::FullSchemeApplication { Invocation(invocation) = 0, KnownArguments { parameters, arguments } = 1 }
    schemes::FullApplicationArgumentKind { Positional = 0, Named(name) = 1, Splice = 2 }
    schemes::FullApplicationBinding { Selected { argument_slots, omitted_slots } = 0, Forwarded(slot) = 1 }
    schemes::FullWrapper {
        Field { source, result, detail } = 0,
        Fallback { source, result, backup } = 1,
        Handler { source, result, detail } = 2,
        Capture { result, detail } = 3,
        KeyValidation { operation, detail } = 4,
        ProcCompletion { returned, group, detail } = 5,
        Propagation { source, returned } = 6,
    }
    schemes::FullWrapperBoundary {
        Captured = 0, Return(ty) = 1, ErrorReturn(ty) = 2, ProcCompletion { returned, group } = 3, Producer = 4, TopLevel = 5,
    }
    schemes::FullWrapperContribution {
        Explicit(ty) = 0, Source(ty) = 1, KeyValidation(index) = 2, KeyValidationBoundary(index) = 3, PropagatedError(ty) = 4,
        ReturnedResult(ty) = 5, FieldSource(ty) = 6, ApplicationSource(ty) = 7,
    }
    schemes::FullSealedOperation {
        TargetVariant { subject, result, name, families } = 0,
        TypePredicate { domain, subject } = 1,
        Field { receiver, name, result } = 2,
        Binary { left, right, result, operator } = 3,
        Index { receiver, key, result, syntax, constant_key } = 4,
        IndexStore { receiver, key, result } = 5,
        StoreValue { storage, replacement } = 6,
        Slice { receiver, result, bounds } = 7,
        RecordOverride { receiver, result, overrides } = 8,
        Equality { left, right } = 9,
        Membership { needle, container } = 10,
        MapKey { receiver, key } = 11,
    }
    schemes::FullSchemeBodyFactValue {
        Operation(index) = 0, Method(index) = 1, Wrapper(index) = 2, CompletionLiftedValue(index) = 3,
        CompletionReturnedResult(index) = 4, CompletionAuthoredOk(index) = 5, CompletionAuthoredErr(index) = 6,
        Expression(ty) = 7, Tail(ty) = 8, LocalImmutable(ty) = 9, LocalMutable(ty) = 10, Assignment(ty) = 11,
        EmptyList = 12, EmptyMap = 13, RecordProducer = 14, NoCompletion = 15, GuardScope(index) = 16,
    }
    guard_schemes::FullGuardBinding { NonNull(instance) = 0, Optional(instance) = 1, Null = 2, Call(index) = 3, Recursive = 4 }
    guard_schemes::FullGuardCallTarget { Scope(index) = 0, Call(index) = 1 }
    crate::sema::sealed_operation::IndexSyntax { Ordinary = 0, NegativeLiteral(index) = 1 }
    crate::map_key::MapKey { Str(text) = 0, Int(value) = 1, Bool(value) = 2, Bytes(bytes) = 3, Path(bytes) = 4, Duration(millis) = 5 }
}

macro_rules! registered_id_codec {
    ($($ty:ty),* $(,)?) => {$(
        impl ImageCodec for $ty {
            fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
                self.raw().put(output)
            }

            fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
                Ok(Self::from_raw(u32::take(input)?))
            }
        }
    )*};
}

registered_id_codec! {
    xsh_registry::signature::RegisteredMethodId,
    xsh_registry::signature::RegisteredFunctionId,
}

#[test]
fn binding_images_round_trip_and_reject_invalid_tags() {
    fn check<T: ImageCodec + std::fmt::Debug + PartialEq>(value: T) {
        let mut output = ImageWriter::default();
        value.put(&mut output).unwrap();
        let mut input = ImageReader::new(&output.bytes);
        assert_eq!(T::take(&mut input).unwrap(), value);
        input.finish().unwrap();
        assert!(T::take(&mut ImageReader::new(&[255])).is_err());
    }

    use crate::sema::sealed_operation::{MembershipDomain, SealedOperationSignature};
    use crate::sema::wrapper_operation::WrapperSelection;
    check(schemes::FullOperationBinding::Selected(SealedOperationSignature::Membership(MembershipDomain::EnvPath)));
    check(schemes::FullOperationBinding::Forwarded(u32::MAX));
    check(schemes::FullMethodBinding::Selected(xsh_registry::signature::RegisteredMethodId::from_raw(37)));
    check(schemes::FullMethodBinding::SelectedFunction(xsh_registry::signature::RegisteredFunctionId::from_raw(42)));
    check(schemes::FullMethodBinding::Forwarded(u32::MAX));
    check(schemes::FullWrapperBinding::Selected(WrapperSelection::ProcCompletion));
    check(schemes::FullWrapperBinding::Forwarded(u32::MAX));
}

impl ImageCodec for FloatValue {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        output.u64(self.0.to_bits());
        Ok(())
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        Ok(FloatValue(f64::from_bits(input.u64()?)))
    }
}

impl ImageCodec for RegexValue {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        self.pattern.put(output)
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        let pattern = String::take(input)?;
        let regex = regex_lite::Regex::new(&pattern).or_else(|error| malformed(error.to_string()))?;
        Ok(RegexValue { pattern, regex: Arc::new(regex) })
    }
}

impl ImageCodec for LoweredValue {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        match self {
            LoweredValue::Null => output.u8(0),
            LoweredValue::Bool(value) => {
                output.u8(1);
                value.put(output)?;
            }
            LoweredValue::Int(value) => {
                output.u8(2);
                value.put(output)?;
            }
            LoweredValue::Float(value) => {
                output.u8(3);
                value.put(output)?;
            }
            LoweredValue::Duration(value) => {
                output.u8(4);
                value.put(output)?;
            }
            LoweredValue::Str(value) => {
                output.u8(5);
                value.put(output)?;
            }
            LoweredValue::Bytes(value) => {
                output.u8(6);
                value.put(output)?;
            }
            LoweredValue::Path(value) => {
                output.u8(7);
                value.put(output)?;
            }
            LoweredValue::Regex(value) => {
                output.u8(8);
                value.put(output)?;
            }
            LoweredValue::Set(values) => {
                output.u8(9);
                values.put(output)?;
            }
            LoweredValue::List(values) => {
                output.u8(10);
                values.put(output)?;
            }
            LoweredValue::SharedList(values) => {
                output.u8(11);
                values.put(output)?;
            }
            LoweredValue::Record(values) => {
                output.u8(12);
                values.put(output)?;
            }
            LoweredValue::Map(values) => {
                output.u8(13);
                values.put(output)?;
            }
            LoweredValue::Tag(value) => {
                output.u8(14);
                value.put(output)?;
            }
            _ => return unsupported("prepared constant holds a runtime value"),
        }
        Ok(())
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        input.nested(|input| Ok(match input.u8()? {
            0 => LoweredValue::Null,
            1 => LoweredValue::Bool(ImageCodec::take(input)?),
            2 => LoweredValue::Int(ImageCodec::take(input)?),
            3 => LoweredValue::Float(ImageCodec::take(input)?),
            4 => LoweredValue::Duration(ImageCodec::take(input)?),
            5 => LoweredValue::Str(ImageCodec::take(input)?),
            6 => LoweredValue::Bytes(ImageCodec::take(input)?),
            7 => LoweredValue::Path(ImageCodec::take(input)?),
            8 => LoweredValue::Regex(ImageCodec::take(input)?),
            9 => LoweredValue::Set(ImageCodec::take(input)?),
            10 => LoweredValue::List(ImageCodec::take(input)?),
            11 => LoweredValue::SharedList(ImageCodec::take(input)?),
            12 => LoweredValue::Record(ImageCodec::take(input)?),
            13 => LoweredValue::Map(ImageCodec::take(input)?),
            14 => LoweredValue::Tag(ImageCodec::take(input)?),
            _ => return malformed("prepared constant tag is invalid"),
        }))
    }
}

impl ImageCodec for PreparedConstantOrigin<TypeId> {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        match self {
            Self::Record { constructor, fields } => {
                output.u8(0);
                constructor.put(output)?;
                fields.put(output)
            }
            Self::List(children) => {
                output.u8(1);
                children.put(output)
            }
            Self::Map(children) => {
                output.u8(2);
                children.put(output)
            }
            Self::Tag(children) => {
                output.u8(3);
                children.put(output)
            }
        }
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        input.nested(|input| match input.u8()? {
            0 => Ok(Self::Record { constructor: ImageCodec::take(input)?, fields: ImageCodec::take(input)? }),
            1 => Ok(Self::List(ImageCodec::take(input)?)),
            2 => Ok(Self::Map(ImageCodec::take(input)?)),
            3 => Ok(Self::Tag(ImageCodec::take(input)?)),
            _ => malformed("prepared constant origin tag is invalid"),
        })
    }
}

impl ImageCodec for PreparedConstantValue<TypeId> {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        self.0.put(output)?;
        self.1.put(output)
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        Ok(PreparedConstantValue(ImageCodec::take(input)?, ImageCodec::take(input)?))
    }
}

macro_rules! image_unsupported {
    ($($ty:ty => $pool:literal),* $(,)?) => {$(
        impl ImageCodec for $ty {
            fn put(&self, _output: &mut ImageWriter) -> ImageResult<()> {
                unsupported(concat!("the image does not encode ", $pool))
            }

            fn take(_input: &mut ImageReader<'_>) -> ImageResult<Self> {
                unsupported(concat!("the image does not decode ", $pool))
            }
        }
    )*};
}

image_unsupported! {
    crate::runtime::eval::ModuleCallPlan => "prepared module call plans",
}

fn registered_signatures() -> &'static [&'static crate::modules::ModuleFnSig] {
    static SIGNATURES: std::sync::OnceLock<Vec<&'static crate::modules::ModuleFnSig>> = std::sync::OnceLock::new();
    SIGNATURES.get_or_init(|| {
        let spec = crate::modules::api_spec();
        let mut signatures: Vec<&'static crate::modules::ModuleFnSig> = spec.module_entries()
            .flat_map(|(_, module)| module.functions.iter()).flat_map(|function| function.overloads.iter())
            .chain(spec.method_entries().flat_map(|(_, methods)| methods.iter())
                .flat_map(|method| method.overloads.iter().map(|overload| &overload.sig)))
            .collect();
        let mut operations = registered_runtime_ops().iter().collect::<Vec<_>>();
        operations.sort_unstable_by_key(|(raw, _)| **raw);
        for (_, &op) in operations {
            for signature in crate::modules::signature::native_static_operation_signatures(op) {
                if !signatures.iter().any(|existing| std::ptr::eq(*existing, signature)) {
                    signatures.push(signature);
                }
            }
        }
        signatures
    })
}

impl ImageCodec for crate::sema::schemes::RegisteredCallPlan {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        let Some(index) = registered_signatures().iter().position(|signature| std::ptr::eq(*signature, self.signature)) else {
            return unsupported("call plan signature is not registered");
        };
        let Ok(index) = u32::try_from(index) else { return unsupported("registered signature index exceeds u32") };
        self.receiver.put(output)?;
        self.name.put(output)?;
        index.put(output)?;
        self.argument_slots.put(output)?;
        self.argument_checks.put(output)
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        let receiver = ImageCodec::take(input)?;
        let name = ImageCodec::take(input)?;
        let index = u32::take(input)? as usize;
        let Some(&signature) = registered_signatures().get(index) else { return malformed("registered signature index is invalid") };
        Ok(Self { receiver, name, signature, argument_slots: ImageCodec::take(input)?, argument_checks: ImageCodec::take(input)? })
    }
}

#[test]
fn registered_call_plan_image_preserves_signature_identity() {
    use crate::sema::schemes::{RegisteredArgumentCheck, RegisteredCallPlan};
    use crate::sema::types::Type;
    let spec = crate::modules::api_spec();
    let function = &spec.module_overloads("json", "decode").unwrap()[0];
    let method = &spec.method_overloads(crate::modules::MethodReceiver::List, "push").unwrap()[0].sig;
    for (receiver, name, signature) in [
        (None, "decode", function),
        (Some(crate::modules::MethodReceiver::List), "push", method),
    ] {
        let plan = RegisteredCallPlan {
            receiver,
            name: Arc::from(name),
            signature,
            argument_slots: Arc::from([0usize]),
            argument_checks: Arc::from([Some(RegisteredArgumentCheck::Unsigned(Type::UInt)), Some(RegisteredArgumentCheck::UIntMapKey)]),
        };
        let mut output = ImageWriter::default();
        plan.put(&mut output).unwrap();
        let mut input = ImageReader::new(&output.bytes);
        let decoded = RegisteredCallPlan::take(&mut input).unwrap();
        input.finish().unwrap();
        assert_eq!(decoded, plan);
        let mut reencoded = ImageWriter::default();
        decoded.put(&mut reencoded).unwrap();
        assert_eq!(reencoded.bytes, output.bytes);
        assert!(RegisteredCallPlan::take(&mut ImageReader::new(&output.bytes[..output.bytes.len() - 1])).is_err());
    }
}

macro_rules! image_identity {
    ($ty:ty, $field:ident) => {
        impl ImageCodec for $ty {
            fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
                let next = output.$field.len() as u64 + 1;
                let canonical = *output.$field.entry(self.image_raw()).or_insert(next);
                canonical.put(output)
            }
            fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
                let canonical = u64::take(input)?;
                if canonical == 0 || (!input.$field.contains_key(&canonical) && canonical != input.$field.len() as u64 + 1) {
                    return malformed("identity is not canonical");
                }
                Ok(*input.$field.entry(canonical).or_insert_with(Self::fresh_image_id))
            }
        }
    };
}

image_identity!(crate::sema::constraints::TypeVariableId, type_ids);
image_identity!(crate::sema::constraints::EffectVariableId, effect_ids);

fn put_owner(owner: u64, output: &mut ImageWriter) -> ImageResult<()> {
    let next = output.owners.len() as u64 + 1;
    let canonical = *output.owners.entry(owner).or_insert(next);
    canonical.put(output)
}

fn take_owner(input: &mut ImageReader<'_>) -> ImageResult<crate::sema::scheme_callable::SchemeCallableOwner> {
    let canonical = u64::take(input)?;
    if canonical == 0 || (!input.owners.contains_key(&canonical) && canonical != input.owners.len() as u64 + 1) {
        return malformed("scheme callable owner is not canonical");
    }
    Ok(*input.owners.entry(canonical).or_insert_with(crate::sema::scheme_callable::SchemeCallableOwner::fresh))
}

impl ImageCodec for crate::sema::types::SchemeCallableId {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        let (owner, index) = self.image_raw();
        put_owner(owner, output)?;
        index.put(output)
    }
    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        let owner = take_owner(input)?;
        Ok(owner.id(u32::take(input)?))
    }
}

impl ImageCodec for crate::sema::validated::IntRange {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> { self.low().put(output)?; self.high().put(output) }
    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        Self::new(i64::take(input)?, i64::take(input)?).map_or_else(|| malformed("integer range is empty"), Ok)
    }
}

impl ImageCodec for crate::sema::validated::ValidatedType {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> { self.validation().put(output)?; self.base().put(output) }
    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        Self::new(ImageCodec::take(input)?, ImageCodec::take(input)?).map_err(ImageError::Malformed)
    }
}

impl ImageCodec for crate::sema::effects::CallableEffects {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        use crate::sema::effects::EffectContract;
        match self.contract() {
            EffectContract::Unknown => 0u8.put(output),
            EffectContract::Known(expression) => {
                1u8.put(output)?;
                expression.direct().bits().put(output)?;
                let mut variables = Vec::new();
                for (id, mask) in expression.variables() {
                    let next = output.effect_ids.len() as u64 + 1;
                    let canonical = *output.effect_ids.entry(id.image_raw()).or_insert(next);
                    variables.push((canonical, mask.bits()));
                }
                variables.sort_unstable_by_key(|entry| entry.0);
                variables.put(output)
            }
        }
    }
    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        use crate::sema::effects::{EffectContract, EffectExpr, EffectSet};
        match u8::take(input)? {
            0 => Ok(Self::unknown()),
            1 => {
                let direct = EffectSet::from_bits(u8::take(input)?).ok_or_else(|| ImageError::Malformed("invalid effect bits".into()))?;
                let variables = BTreeMap::<crate::sema::constraints::EffectVariableId, u8>::take(input)?;
                let mut expression = EffectExpr::finite(direct);
                for (variable, bits) in variables {
                    let mask = EffectSet::from_bits(bits).filter(|mask| !mask.is_empty()).ok_or_else(|| ImageError::Malformed("invalid effect mask".into()))?;
                    expression = expression.union(EffectExpr::variable(variable).masked(mask));
                }
                Ok(Self::from_contract(EffectContract::Known(expression)))
            }
            _ => malformed("callable effects tag is invalid"),
        }
    }
}

macro_rules! image_type {
    ($($variant:ident $(($($tuple:ident),*))? $({$($named:ident),*})? = $index:literal),* $(,)?) => {
        impl ImageCodec for crate::sema::types::Type {
            fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
                use crate::sema::types::Type as T;
                match self {
                    $(T::$variant $(($($tuple),*))? $({$($named),*})? => {
                        output.u8($index);
                        $($(ImageCodec::put($tuple, output)?;)*)?
                        $($(ImageCodec::put($named, output)?;)*)?
                    })*
                }
                Ok(())
            }
            fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
                use crate::sema::types::Type as T;
                input.nested(|input| match input.u8()? {
                    $($index => Ok(T::$variant $(($(image_field!($tuple, input)),*))? $({$($named: ImageCodec::take(input)?),*})?),)*
                    _ => malformed("semantic type tag is invalid"),
                })
            }
        }
    };
}

image_type! {
    BuiltinParameter(parameter) = 0, Inference(variable) = 1, Any = 2, Unknown = 3, Invalid = 4,
    Null = 5, Bool = 6, Int = 7, UInt = 8, Float = 9, Duration = 10, Str = 11, Bytes = 12,
    Digest = 13, Regex = 14, Path = 15, List(item) = 16, Map(key, value) = 17, Stream(item) = 18,
    Set(item) = 19, ErasedRecord = 20, Record(fields) = 21, Module(module) = 22, DynamicModule = 23,
    Result(value, error) = 24, Status = 25, EnvPathList = 26, Error = 27, ErrorFamily(name) = 28,
    ErrorVariant { family, variant } = 29, ErrorFacet(name) = 30, ProcessError = 31,
    Function(kind) = 32, Command = 33, ProcessHandle = 34, NetJob = 35, FsRoot = 36, FsLock = 37,
    SshSession = 38, Unit = 39, Tag(name) = 40, Optional(item) = 41, Union(members) = 42,
    Callable(callable) = 43, SchemeCallable(id) = 44, Validated(validated) = 45,
}

#[test]
fn semantic_type_images_preserve_recovery_and_foreign_identities() {
    use crate::sema::types::{CallableKind, CallableParamType, CallableType, ModuleExportType, ModuleType, Type, TypedCallable};
    use crate::sema::effects::{CallableEffects, EffectContract, EffectExpr, EffectSet};
    let symbols = SymbolOwner::new();
    symbols.with_current(|| {
        let name = Name::intern("image_recovery");
        let variable = crate::sema::constraints::TypeVariableId::from_image_raw(412);
        let effect = crate::sema::constraints::EffectVariableId::from_image_raw(501);
        let identity = crate::sema::types::SchemeCallableId::from_image_raw(619, 3).unwrap();
        let effects = CallableEffects::from_contract(EffectContract::Known(
            EffectExpr::variable(effect).masked(EffectSet::singleton(Effect::Net))));
        let callable = Type::Callable(Arc::new(TypedCallable { kind: CallableKind::Proc, sig: CallableType {
            params: vec![CallableParamType { name, ty: Type::Inference(variable), defaulted: true, rest: false }],
            return_ty: Box::new(Type::SchemeCallable(identity)), effects,
        } }));
        let module = Type::Module(Arc::new(ModuleType::exact(BTreeMap::from([
            (name, ModuleExportType::Value { ty: callable, optional: false }),
        ]))));
        let cases = [Type::Unknown, Type::Invalid, Type::BuiltinParameter(xsh_registry::types::BuiltinTypeParameter::Value),
            Type::Inference(variable), Type::SchemeCallable(identity), Type::List(Box::new(module))];
        for ty in cases {
            let mut output = ImageWriter::default();
            ty.put(&mut output).unwrap();
            let mut input = ImageReader::new(&output.bytes);
            let restored = Type::take(&mut input).unwrap();
            input.finish().unwrap();
            if !matches!(ty, Type::Inference(_) | Type::SchemeCallable(_) | Type::List(_)) { assert_eq!(restored, ty); }
            let mut reencoded = ImageWriter::default();
            restored.put(&mut reencoded).unwrap();
            assert_eq!(reencoded.bytes, output.bytes);
            assert!(Type::take(&mut ImageReader::new(&output.bytes[..output.bytes.len() - 1])).is_err());
        }
        assert!(Type::take(&mut ImageReader::new(&[44, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])).is_err());
        assert!(Type::take(&mut ImageReader::new(&[16; 65])).is_err());
    });
}

#[test]
fn decoded_semantic_identities_do_not_alias_allocated_ids() {
    use crate::sema::constraints::{EffectVariableId, TypeVariableId};
    use crate::sema::scheme_callable::SchemeCallableOwner;
    let original = (TypeVariableId::fresh_image_id(), EffectVariableId::fresh_image_id(), SchemeCallableOwner::fresh().id(3));
    let mut output = ImageWriter::default();
    let original_pair = (original.0, (original.1, original.2));
    (original_pair, original_pair).put(&mut output).unwrap();
    let allocated = (TypeVariableId::fresh_image_id(), EffectVariableId::fresh_image_id(), SchemeCallableOwner::fresh().id(3));
    let mut input = ImageReader::new(&output.bytes);
    type Ids = (TypeVariableId, (EffectVariableId, crate::sema::types::SchemeCallableId));
    let (first, second): (Ids, Ids) = ImageCodec::take(&mut input).unwrap();
    assert_eq!(first, second);
    let decoded = (first.0, (first.1).0, (first.1).1);
    input.finish().unwrap();
    assert_ne!(decoded.0, original.0);
    assert_ne!(decoded.0, allocated.0);
    assert_ne!(decoded.1, original.1);
    assert_ne!(decoded.1, allocated.1);
    assert_ne!(decoded.2, original.2);
    assert_ne!(decoded.2, allocated.2);
    let mut reencoded = ImageWriter::default();
    (first, second).put(&mut reencoded).unwrap();
    assert_eq!(reencoded.bytes, output.bytes);
    assert!(TypeVariableId::take(&mut ImageReader::new(&2u64.to_le_bytes())).is_err());
}

#[test]
fn effect_images_order_previously_seen_identities_canonically() {
    use crate::sema::constraints::EffectVariableId;
    use crate::sema::effects::{CallableEffects, EffectContract, EffectExpr};
    let earlier = EffectVariableId::fresh_image_id();
    let later = EffectVariableId::fresh_image_id();
    let effects = CallableEffects::from_contract(EffectContract::Known(EffectExpr::variable(earlier).union(EffectExpr::variable(later))));
    let original = (later, effects);
    let mut output = ImageWriter::default();
    original.put(&mut output).unwrap();
    let mut input = ImageReader::new(&output.bytes);
    let decoded: (EffectVariableId, CallableEffects) = ImageCodec::take(&mut input).unwrap();
    input.finish().unwrap();
    let mut reencoded = ImageWriter::default();
    decoded.put(&mut reencoded).unwrap();
    assert_eq!(reencoded.bytes, output.bytes);
}

#[test]
fn semantic_pool_image_preserves_scheme_callable_owner() {
    use crate::runtime::eval::indexed::semantic::{SemanticPoolBuilder, SemanticPools};
    use crate::sema::types::Type;
    let symbols = SymbolOwner::new();
    let mut pools = SemanticPools::default();
    let identity = pools.register_scheme_callable().unwrap();
    let id = SemanticPoolBuilder::default().intern_type(&mut pools, &Type::SchemeCallable(identity)).unwrap();
    let mut output = ImageWriter::default();
    put_semantic(&pools.export_raw(&symbols), &mut output).unwrap();
    let mut input = ImageReader::new(&output.bytes);
    let raw = take_semantic(&mut input, symbols).unwrap();
    input.finish().unwrap();
    let (decoded, symbols) = SemanticPools::from_raw(raw);
    decoded.verify().unwrap();
    let Type::SchemeCallable(restored) = decoded.to_type(id).unwrap() else { panic!("expected scheme callable") };
    assert_ne!(restored, identity);
    assert!(decoded.scheme_callable_index(identity).is_err());
    assert_eq!(decoded.scheme_callable_index(restored).unwrap(), 0);
    let mut reencoded = ImageWriter::default();
    put_semantic(&decoded.export_raw(&symbols), &mut reencoded).unwrap();
    assert_eq!(reencoded.bytes, output.bytes);
}

impl ImageCodec for crate::runtime::eval::require::PreparedSchema {
    fn put(&self, output: &mut ImageWriter) -> ImageResult<()> {
        use crate::runtime::eval::require::PreparedSchema as S;
        match self {
            S::UnknownFields(policy, schema) => { 0u8.put(output)?; policy.put(output)?; schema.put(output) }
            S::Validate(ty) => { 1u8.put(output)?; ty.put(output) }
            S::Record(fields) => { 2u8.put(output)?; fields.put(output) }
            S::List(schema) => { 3u8.put(output)?; schema.put(output) }
            S::Map(key, schema) => { 4u8.put(output)?; key.put(output)?; schema.put(output) }
            S::Optional(schema) => { 5u8.put(output)?; schema.put(output) }
            S::WireEnum(mapping) => { 6u8.put(output)?; mapping.put(output) }
            S::Union(ty, members) => { 7u8.put(output)?; ty.put(output)?; members.put(output) }
            S::Validated(ty, schema) => { 8u8.put(output)?; ty.put(output)?; schema.put(output) }
            S::Set(ty) => { 9u8.put(output)?; ty.put(output) }
        }
    }

    fn take(input: &mut ImageReader<'_>) -> ImageResult<Self> {
        use crate::runtime::eval::require::PreparedSchema as S;
        input.nested(|input| Ok(match input.u8()? {
            0 => S::UnknownFields(ImageCodec::take(input)?, ImageCodec::take(input)?),
            1 => S::Validate(ImageCodec::take(input)?),
            2 => S::Record(ImageCodec::take(input)?),
            3 => S::List(ImageCodec::take(input)?),
            4 => S::Map(ImageCodec::take(input)?, ImageCodec::take(input)?),
            5 => S::Optional(ImageCodec::take(input)?),
            6 => S::WireEnum(ImageCodec::take(input)?),
            7 => S::Union(ImageCodec::take(input)?, ImageCodec::take(input)?),
            8 => S::Validated(ImageCodec::take(input)?, ImageCodec::take(input)?),
            9 => S::Set(ImageCodec::take(input)?),
            _ => return malformed("prepared schema tag is invalid"),
        }))
    }
}

#[test]
fn prepared_schema_image_round_trip_and_malformed_inputs() {
    use crate::runtime::eval::require::PreparedSchema as S;
    use crate::sema::types::Type;
    use crate::sema::wire_enums::WireEnumMapping;
    use crate::syntax::arena::UnknownFields;

    let symbols = SymbolOwner::new();
    symbols.with_current(|| {
        let name = Name::intern("schema_image_field");
        let leaf = Arc::new(S::Validate(Type::Int));
        let mapping = Arc::new(WireEnumMapping { type_name: name, variants: BTreeMap::from([(name, Arc::from("value"))]) });
        let schemas = [
            S::UnknownFields(UnknownFields::Reject, Arc::clone(&leaf)),
            S::Validate(Type::Str),
            S::Record(vec![(name, Arc::clone(&leaf))]),
            S::List(Arc::clone(&leaf)),
            S::Map(Type::Str, Arc::clone(&leaf)),
            S::Optional(Arc::clone(&leaf)),
            S::WireEnum(mapping),
            S::Union(Type::Union(vec![Type::Int, Type::Str]), vec![Arc::clone(&leaf)]),
            S::Validated(Type::non_empty(Type::Int), Arc::clone(&leaf)),
            S::Set(Type::Str),
        ];
        for schema in schemas {
            let mut output = ImageWriter::default();
            schema.put(&mut output).unwrap();
            let mut input = ImageReader::new(&output.bytes);
            let decoded = S::take(&mut input).unwrap();
            input.finish().unwrap();
            assert_eq!(decoded, schema);
            let mut reencoded = ImageWriter::default();
            decoded.put(&mut reencoded).unwrap();
            assert_eq!(reencoded.bytes, output.bytes);
            assert!(S::take(&mut ImageReader::new(&output.bytes[..output.bytes.len() - 1])).is_err());
        }
        assert!(S::take(&mut ImageReader::new(&[255])).is_err());
        assert!(S::take(&mut ImageReader::new(&[3; 65])).is_err());
    });
}

pub(super) fn put_semantic(semantic: &RawSemanticPools, output: &mut ImageWriter) -> ImageResult<()> {
    let RawSemanticTables {
        scheme_callable_owner,
        scheme_callable_count,
        type_tags,
        type_data,
        type_extra,
        signature_data,
        signature_extra,
        shapes,
        shape_fields,
    } = &semantic.tables;
    put_owner(scheme_callable_owner.image_raw(), output)?;
    scheme_callable_count.put(output)?;
    type_tags.put(output)?;
    type_data.put(output)?;
    type_extra.put(output)?;
    signature_data.put(output)?;
    signature_extra.put(output)?;
    shapes.put(output)?;
    shape_fields.put(output)
}

pub(super) fn take_semantic(input: &mut ImageReader<'_>, symbols: SymbolOwner) -> ImageResult<RawSemanticPools> {
    Ok(RawSemanticPools {
        symbols,
        tables: RawSemanticTables {
            scheme_callable_owner: take_owner(input)?,
            scheme_callable_count: ImageCodec::take(input)?,
            type_tags: ImageCodec::take(input)?,
            type_data: ImageCodec::take(input)?,
            type_extra: ImageCodec::take(input)?,
            signature_data: ImageCodec::take(input)?,
            signature_extra: ImageCodec::take(input)?,
            shapes: ImageCodec::take(input)?,
            shape_fields: ImageCodec::take(input)?,
        },
    })
}

const IMAGE_MAGIC: &[u8; 8] = b"XSHIMG01";
const INVALID_SYMBOL_TEXT: &str = "<invalid-symbol>";
const SYMBOL_PROBE: &str = "\u{0}xsh-program-image-probe";

struct SymbolKeeper(#[expect(dead_code)] SymbolOwner);

fn retain_live_symbols() -> (Vec<(u32, String)>, SymbolKeeper) {
    let keeper = SymbolOwner::new();
    let (live, _) = crate::symbol::dynamic_symbol_stats();
    let first = (0..u32::MAX).find(|raw| !Name::from_symbol(Symbol::from_raw(*raw)).is_preloaded()).unwrap();
    let mut symbols = Vec::with_capacity(live);
    let mut vacant = 0usize;
    for raw in first..u32::MAX {
        if vacant >= 1 << 20 || (symbols.len() >= live && vacant >= 1 << 12) {
            break;
        }
        let text = Name::from_symbol(Symbol::from_raw(raw)).as_str();
        if text.as_str() == INVALID_SYMBOL_TEXT {
            vacant += 1;
            continue;
        }
        vacant = 0;
        let retained = keeper.intern(text.as_str()).symbol().raw();
        symbols.push((retained, text.as_str().to_owned()));
    }
    symbols.sort_unstable();
    symbols.dedup_by_key(|(raw, _)| *raw);
    (symbols, SymbolKeeper(keeper))
}

fn restore_symbols(symbols: &[(u32, String)], owner: &SymbolOwner) -> ImageResult<()> {
    let fillers = SymbolOwner::new();
    for (raw, text) in symbols {
        loop {
            let probe = SymbolOwner::new();
            let next = probe.intern(SYMBOL_PROBE).symbol().raw();
            drop(probe);
            if next >= *raw {
                break;
            }
            fillers.intern(&format!("{SYMBOL_PROBE}-{next}"));
        }
        if owner.intern(text).symbol().raw() != *raw {
            return malformed(format!("symbol `{text}` cannot take its recorded identity {raw}"));
        }
    }
    Ok(())
}

pub(super) struct EncodedImage {
    pub(super) bytes: Vec<u8>,
    pub(super) rows: std::ops::Range<usize>,
    pub(super) sizes: Vec<(&'static str, usize)>,
}

struct ImageRunState {
    tag_variants: Vec<(Name, usize)>,
    error_families: Vec<Name>,
}

fn source_digests(sources: &SourceMap) -> Vec<(String, [u8; 32])> {
    sources.files().iter().map(|file| (file.name().to_owned(), *blake3::hash(file.text().as_bytes()).as_bytes())).collect()
}

fn encode_image(
    program: &FullProgram,
    evaluator: &Evaluator,
    plan: &CompactIndexedRunPlan,
    byte_arguments: bool,
    symbols: &[(u32, String)],
) -> ImageResult<EncodedImage> {
    if byte_arguments {
        return unsupported("byte script arguments");
    }
    if plan.signature_cli.is_some() {
        return unsupported("signature CLI entries");
    }
    let mut output = ImageWriter::default();
    output.bytes.extend_from_slice(IMAGE_MAGIC);
    let mut sizes = Vec::new();
    let start = output.position();
    source_digests(&program.sources).put(&mut output)?;
    sizes.push(("source digests", output.position() - start));
    let start = output.position();
    symbols.to_vec().put(&mut output)?;
    sizes.push(("symbol table", output.position() - start));
    let start = output.position();
    program.function_definition_spans.put(&mut output)?;
    sizes.push(("function definition spans", output.position() - start));
    let rows_start = output.position();
    sizes.extend(RawFullRows::capture(program).put_image(&mut output)?);
    let rows = rows_start..output.position();
    let mut state = ImageRunState {
        tag_variants: evaluator.tag_variants.iter().map(|(name, arity)| (*name, *arity)).collect(),
        error_families: evaluator.error_families.keys().copied().collect(),
    };
    state.tag_variants.sort_unstable_by_key(|(name, _)| name.symbol().raw());
    state.error_families.sort_unstable_by_key(|name| name.symbol().raw());
    let start = output.position();
    state.tag_variants.put(&mut output)?;
    state.error_families.put(&mut output)?;
    sizes.push(("runtime declarations", output.position() - start));
    Ok(EncodedImage { bytes: output.bytes, rows, sizes })
}

fn decode_image(bytes: &[u8], sources: Arc<SourceMap>) -> ImageResult<(FullProgram, ImageRunState)> {
    let mut input = ImageReader::new(bytes);
    if &input.array::<8>()? != IMAGE_MAGIC {
        return malformed("image magic differs");
    }
    if Vec::<(String, [u8; 32])>::take(&mut input)? != source_digests(&sources) {
        return malformed("image sources differ from the loaded closure");
    }
    let table = Vec::<(u32, String)>::take(&mut input)?;
    if !table.is_sorted_by(|left, right| left.0 < right.0) {
        return malformed("image symbol table is not strictly ordered");
    }
    let symbols = SymbolOwner::new();
    restore_symbols(&table, &symbols)?;
    let function_definition_spans = ImageCodec::take(&mut input)?;
    let rows = RawFullRows::take_image(&mut input, symbols)?;
    let state = ImageRunState { tag_variants: ImageCodec::take(&mut input)?, error_families: ImageCodec::take(&mut input)? };
    input.finish()?;
    let program = RawFullProgram {
        rows,
        sources,
        binding_namespace: FullProgram::fresh_binding_namespace(),
        function_definition_spans,
    }.into_program();
    Ok((program, state))
}

const AWK_SCRIPT: &str = "core/awk.xsh";
const AWK_PROGRAM: &str = "BEGIN{print 1}";
const IMAGE_PATH_ENV: &str = "XSH_PROGRAM_IMAGE";

struct LoadedEntry {
    sources: SourceMap,
    arena: ArenaProgram,
    source_id: SourceId,
    module_roots: Vec<std::path::PathBuf>,
}

fn load_entry(path: &str) -> LoadedEntry {
    let entry = crate::loader::entry_source_from_bytes(path, std::fs::read(path).unwrap());
    assert!(entry.diagnostics.is_empty(), "{:?}", entry.diagnostics);
    let source_id = entry.source_id;
    let module_roots = crate::project::project_module_roots(std::path::Path::new(path)).unwrap();
    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(path, entry, module_roots.clone());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let crate::syntax::parser::ArenaParseOutput { arena, cst, diagnostics: _ } = parsed;
    drop(cst);
    LoadedEntry { sources, arena, source_id, module_roots }
}

fn script_evaluator(path: &str, args: &[&str], sources: Arc<SourceMap>, module_roots: Vec<std::path::PathBuf>) -> Evaluator {
    let args = args.iter().map(|arg| (*arg).to_owned()).collect();
    Evaluator::new_with_shared_sources_and_command(args, sources, crate::runner::script_command_name(path))
        .with_module_roots(module_roots)
        .without_harness_control_env()
}

struct CheckedEntry {
    evaluator: Evaluator,
    plan: CompactIndexedRunPlan,
    byte_arguments: bool,
}

fn prepare_checked(path: &str, args: &[&str]) -> CheckedEntry {
    let LoadedEntry { sources, arena, source_id, module_roots } = load_entry(path);
    let arena = Arc::new(arena);
    let check = crate::sema::check::Checker::check_arena_with_sources_options_and_type_program(
        &arena, &sources, source_id, crate::sema::check::CheckOptions::for_execution(), Arc::clone(&arena));
    assert!(check.diagnostics.iter().all(|diagnostic| diagnostic.severity != crate::diagnostic::Severity::Error), "{:?}", check.diagnostics);
    let byte_arguments = crate::sema::check::byte_argument_main(&arena, &check.parameter_types);
    let mut evaluator = script_evaluator(path, args, Arc::new(sources), module_roots);
    let plan = arena.symbol_owner().with_current(|| {
        let declarations = crate::sema::check::Checker::compact_declarations(&arena, check);
        evaluator.prepare_checked_compact_indexed_only(&arena, source_id, false, declarations)
    }).unwrap_or_else(|diagnostic| panic!("{diagnostic:?}"));
    drop(arena);
    CheckedEntry { evaluator, plan, byte_arguments }
}

fn install_image_program(
    evaluator: &mut Evaluator,
    arena: &ArenaProgram,
    program: FullProgram,
    state: ImageRunState,
) -> ImageResult<CompactIndexedRunPlan> {
    use crate::runtime::eval as eval;
    let scope = eval::lower::CompactRootScope::whole_program(arena);
    if eval::compact_reveal_type_call_span(arena, &scope).is_some() {
        return malformed("`reveal_type` is available only through `xsht check`");
    }
    evaluator.tag_variants.extend(state.tag_variants);
    evaluator.error_families.extend(state.error_families.into_iter().map(|name| (name, RuntimeErrorFamily {})));
    let root = arena.arena.stmt_ids(scope.root_statements()).collect::<Vec<_>>();
    if program.driver_step_count().or_else(|error| malformed(error.message.clone()))? != root.len() {
        return malformed("decoded driver statement count differs from the loaded source");
    }
    let auto_main_required = eval::compact_root_proc_main_requires_auto_call_indexed(arena, &root, &program)
        .or_else(|diagnostic| malformed(diagnostic.message))?;
    if auto_main_required && !program.contains_function(LoweredFunctionKey::Name(Name::intern("main")), CallableKind::Proc) {
        return malformed("proc main is missing from the decoded program");
    }
    let program = Arc::new(program);
    evaluator.indexed_program = Some(Arc::clone(&program));
    evaluator.prune_binding_values();
    let compact_auto_main_args = if auto_main_required {
        evaluator.compact_auto_main_args().map_or_else(|| malformed("script arguments could not be converted for main"), Ok)?
    } else {
        Vec::new()
    };
    let mut statements = Vec::with_capacity(root.len());
    for (index, statement) in root.iter().copied().enumerate() {
        let skip_auto_main = eval::compact_should_skip_auto_main_stmt(arena, &root, index, auto_main_required);
        let encoded = !program.driver_step_is_skip(index).or_else(|error| malformed(error.message.clone()))?;
        if !encoded && !skip_auto_main && !eval::compact_top_level_stmt_is_skippable(arena, statement, false) {
            return malformed("a top-level statement is missing from the decoded program");
        }
        statements.push(CompactIndexedDriverStepPlan { span: arena.arena.stmt(statement).span, skip_auto_main });
    }
    Ok(CompactIndexedRunPlan {
        script_span: statements.first().map_or_else(eval::zero_span, |statement| statement.span),
        statements,
        auto_main_required,
        compact_auto_main_args,
        signature_cli: None,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ImageStage {
    Load,
    Decode,
    Verify,
    Install,
}

fn prepare_from_image(
    path: &str,
    args: &[&str],
    image: &[u8],
    through: ImageStage,
) -> ImageResult<Option<(Evaluator, CompactIndexedRunPlan)>> {
    let LoadedEntry { sources, arena, source_id: _, module_roots } = load_entry(path);
    if through == ImageStage::Load {
        return Ok(None);
    }
    let sources = Arc::new(sources);
    let (program, state) = decode_image(image, Arc::clone(&sources))?;
    if through == ImageStage::Decode {
        return Ok(None);
    }
    FullVerifier::verify(&program).or_else(|error| malformed(format!("decoded program failed verification: {}", error.message)))?;
    if through == ImageStage::Verify {
        return Ok(None);
    }
    let mut evaluator = script_evaluator(path, args, sources, module_roots);
    let plan = arena.symbol_owner().with_current(|| install_image_program(&mut evaluator, &arena, program, state))?;
    drop(arena);
    Ok(Some((evaluator, plan)))
}

fn installed_from_image(image: &[u8]) -> ImageResult<(Evaluator, CompactIndexedRunPlan)> {
    Ok(prepare_from_image(AWK_SCRIPT, &[AWK_PROGRAM], image, ImageStage::Install)?.expect("installation returns the prepared run"))
}

fn encode_checked(entry: &CheckedEntry, symbols: &[(u32, String)]) -> EncodedImage {
    let program = entry.evaluator.indexed_program.as_ref().unwrap();
    FullVerifier::verify(program).unwrap();
    encode_image(program, &entry.evaluator, &entry.plan, entry.byte_arguments, symbols).unwrap()
}

fn transfer_marks(evaluator: &Evaluator) -> usize {
    evaluator.indexed_program.as_ref().unwrap().store.resource_transfers.iter().filter(|marked| **marked).count()
}

fn awk_options() -> crate::execution::script::RunOptions {
    crate::execution::script::RunOptions {
        script: AWK_SCRIPT.to_owned(),
        args: vec![AWK_PROGRAM.into()],
        coverage_trace_dir: None,
    }
}

#[test]
fn awk_program_image_round_trip_matches_the_checked_run() {
    super::super::super::tests::run_with_large_stack(|| {
        let (image, transfers, keeper) = {
            let entry = prepare_checked(AWK_SCRIPT, &[AWK_PROGRAM]);
            let (symbols, keeper) = retain_live_symbols();
            let image = encode_checked(&entry, &symbols);
            (image, transfer_marks(&entry.evaluator), keeper)
        };
        assert!(transfers > 0);
        let (evaluator, plan) = installed_from_image(&image.bytes).unwrap();
        assert_eq!(transfer_marks(&evaluator), transfers);
        let mut reencoded = ImageWriter::default();
        RawFullRows::capture(evaluator.indexed_program.as_ref().unwrap()).put_image(&mut reencoded).unwrap();
        assert!(reencoded.bytes == image.bytes[image.rows.clone()], "decoded rows encode differently");
        let actual = evaluator.eval_installed_compact_indexed_only(plan).unwrap_or_else(|_| panic!("decoded AWK program failed to execute"));
        let expected = crate::runner::run_script(awk_options());
        assert_eq!(actual.status, expected.status);
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
        assert_eq!(actual.stdout, b"1\n");
        let mut truncated = image.bytes.clone();
        truncated.pop();
        assert!(matches!(installed_from_image(&truncated), Err(ImageError::Malformed(_))));
        drop(keeper);
    });
}

#[test]
#[ignore = "writes the AWK program image named by XSH_PROGRAM_IMAGE for the startup measurement"]
fn awk_program_image_write() {
    let path = std::env::var_os(IMAGE_PATH_ENV).expect("XSH_PROGRAM_IMAGE names the image to write");
    super::super::super::tests::run_with_large_stack(move || {
        let entry = prepare_checked(AWK_SCRIPT, &[AWK_PROGRAM]);
        let (symbols, _keeper) = retain_live_symbols();
        let image = encode_checked(&entry, &symbols);
        std::fs::write(&path, &image.bytes).unwrap();
        for (pool, size) in image.sizes.iter().filter(|(_, size)| *size > 4) {
            eprintln!("PROGRAM_IMAGE pool {pool} {size}");
        }
        eprintln!("PROGRAM_IMAGE bytes={} symbols={} transfers={}", image.bytes.len(), symbols.len(), transfer_marks(&entry.evaluator));
    });
}

#[test]
#[ignore = "runs the ordinary checked AWK start for the program image measurement"]
fn awk_program_image_checked_start() {
    super::super::super::tests::run_with_large_stack(|| {
        let output = crate::runner::run_script(awk_options());
        assert_eq!((output.status, output.stdout.as_slice(), output.stderr.as_slice()), (0, b"1\n".as_slice(), b"".as_slice()));
        print!("{}", String::from_utf8_lossy(&output.stdout));
    });
}

fn cached_start(through: ImageStage) {
    let path = std::env::var_os(IMAGE_PATH_ENV).expect("XSH_PROGRAM_IMAGE names the image to read");
    super::super::super::tests::run_with_large_stack(move || {
        let image = std::fs::read(&path).unwrap();
        let Some((evaluator, plan)) = prepare_from_image(AWK_SCRIPT, &[AWK_PROGRAM], &image, through).unwrap() else { return };
        let output = evaluator.eval_installed_compact_indexed_only(plan).unwrap_or_else(|_| panic!("decoded AWK program failed to execute"));
        assert_eq!((output.status, output.stdout.as_slice(), output.stderr.as_slice()), (0, b"1\n".as_slice(), b"".as_slice()));
        print!("{}", String::from_utf8_lossy(&output.stdout));
    });
}

#[test]
#[ignore = "runs the AWK start from the image named by XSH_PROGRAM_IMAGE for the program image measurement"]
fn awk_program_image_cached_start() {
    cached_start(ImageStage::Install);
}

#[test]
#[ignore = "loads the AWK closure for the program image phase measurement"]
fn awk_program_image_cached_load() {
    cached_start(ImageStage::Load);
}

#[test]
#[ignore = "loads and decodes the AWK image for the program image phase measurement"]
fn awk_program_image_cached_decode() {
    cached_start(ImageStage::Decode);
}

#[test]
#[ignore = "loads, decodes and verifies the AWK image for the program image phase measurement"]
fn awk_program_image_cached_verify() {
    cached_start(ImageStage::Verify);
}
