//! Private wire-format probes for the Rust verifier-checked scalar-literal fixture.

use super::{FullProgram, FullTag, FullVerifier, IR_NONE};
use crate::runtime::value::{DurationValue, FloatValue, PathValue};
use crate::source::{SourceId, SourceMap, Span};
use crate::symbol::Name;
use crate::syntax::arena::{ArenaFunctionDef, ArenaProgram};
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;
use std::sync::Arc;

const SOURCE_SCHEMA: &str = "xsh-source-v1";
const RAW_IR_SCHEMA: &str = "xsh-raw-ir-v1";
const RAW_IR_SCOPE: &str = "scalar-literal-boundary-fixture-v1";
const SERIALIZATION_ONLY_NAN: u64 = 0x7ff8_0000_0000_0042;
const MAX_TRANSPORT_DOCUMENT_BYTES: usize = 64 * 1024;
const MAX_JSON_CONTAINER_DEPTH: usize = 64;

/// Exchanged table IDs are one-based and leave the maximum word for absence.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct IrId(NonZeroU32);

impl IrId {
    fn from_raw(raw: u32) -> Result<Self, String> {
        NonZeroU32::new(raw)
            .filter(|raw| raw.get() != IR_NONE)
            .map(Self)
            .ok_or_else(|| "IR ID is zero or reserved for IR_NONE".to_string())
    }

    fn from_index(index: usize) -> Result<Self, String> {
        let raw = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(1))
            .ok_or_else(|| "IR ID exceeds the one-based u32 domain".to_string())?;
        Self::from_raw(raw)
    }

    fn raw(self) -> u32 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OptionalIrId(Option<IrId>);

impl OptionalIrId {
    fn from_raw(raw: u32) -> Result<Self, String> {
        if raw == IR_NONE {
            Ok(Self(None))
        } else {
            IrId::from_raw(raw).map(|id| Self(Some(id)))
        }
    }

    fn raw(self) -> u32 {
        self.0.map_or(IR_NONE, IrId::raw)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SourceDocument {
    id: IrId,
    text: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BinderRole {
    Function,
    Parameter,
}

impl BinderRole {
    fn name(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Parameter => "parameter",
        }
    }

    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "function" => Ok(Self::Function),
            "parameter" => Ok(Self::Parameter),
            _ => Err(format!("unknown binder role {name:?}")),
        }
    }
}

/// Syntactic function and parameter names get stable source-order IDs; these
/// are textual projections, not resolved name identities from the parser.
#[derive(Clone, Debug, Eq, PartialEq)]
struct BinderProjection {
    id: IrId,
    owner: OptionalIrId,
    role: BinderRole,
    text: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LiteralTag {
    Int,
    UInt,
    Float,
    Duration,
}

impl LiteralTag {
    fn from_full_tag(tag: FullTag) -> Option<Self> {
        match tag {
            FullTag::ExprInt => Some(Self::Int),
            FullTag::ExprUInt => Some(Self::UInt),
            FullTag::ExprFloat => Some(Self::Float),
            FullTag::ExprDuration => Some(Self::Duration),
            _ => None,
        }
    }

    fn from_name(name: &str) -> Result<Self, String> {
        match name {
            "ExprInt" => Ok(Self::Int),
            "ExprUInt" => Ok(Self::UInt),
            "ExprFloat" => Ok(Self::Float),
            "ExprDuration" => Ok(Self::Duration),
            _ => Err(format!("unknown raw-IR literal tag {name:?}")),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Int => "ExprInt",
            Self::UInt => "ExprUInt",
            Self::Float => "ExprFloat",
            Self::Duration => "ExprDuration",
        }
    }

    fn ordinal(self) -> u32 {
        match self {
            Self::Int => FullTag::ExprInt as u32,
            Self::UInt => FullTag::ExprUInt as u32,
            Self::Float => FullTag::ExprFloat as u32,
            Self::Duration => FullTag::ExprDuration as u32,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ScalarInstruction {
    id: IrId,
    tag: LiteralTag,
    words: [u32; 2],
    // The typed view is checked against low-then-high words during decoding.
    value_bits: u64,
}

/// This fixture carries scalar literal rows plus explicit runtime boundary
/// samples. It is not a complete FullStore document: any instruction outside
/// its declared projection is an error in the exporter.
#[derive(Clone, Debug, Eq, PartialEq)]
struct RawIrDocument {
    source_id: IrId,
    instruction_count: u32,
    driver_program_count: u32,
    // FullStore stores driver-program references one-based and uses IR_NONE for no root.
    driver_root: OptionalIrId,
    binders: Vec<BinderProjection>,
    instructions: Vec<ScalarInstruction>,
    // These are empirical results from the fixture's pure calls, never checker facts.
    integer_output_bits: Vec<u64>,
    float_output_bits: Vec<u64>,
    native_path_inputs: Vec<Vec<u8>>,
    // These NaN samples test encoding only; they have no scalar instruction row.
    serialization_only_float_bits: Vec<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum WireValue {
    String(String),
    Array(Vec<WireValue>),
    Object(Vec<(String, WireValue)>),
}

fn parse_wire_document(input: &str) -> Result<WireValue, String> {
    if input.len() > MAX_TRANSPORT_DOCUMENT_BYTES {
        return Err("wire document exceeds the private transport byte limit".to_string());
    }
    let mut context = deser::Context::with(deser::de::Limits::builder()
        .max_depth(MAX_JSON_CONTAINER_DEPTH).build());
    context.set(deser::de::DuplicateKeys::Error);
    let value = deser_json::DeserializerConfig::builder().context(context).build()
        .from_str::<deser_value::Value>(input).map_err(|error| error.to_string())?;
    WireValue::from_host(value)
}

impl WireValue {
    fn from_host(value: deser_value::Value) -> Result<Self, String> {
        use deser_value::Kind;
        match value.into_kind() {
            Kind::Str(text) | Kind::Lexical(text) => Ok(Self::String(text)),
            Kind::Seq(values) => values.into_iter().map(Self::from_host)
                .collect::<Result<Vec<_>, _>>().map(Self::Array),
            Kind::Map(fields) => fields.into_iter().map(|(key, value)| {
                let key = match key.into_kind() {
                    Kind::Str(text) | Kind::Lexical(text) => text,
                    _ => return Err("wire object key is not a string".to_string()),
                };
                Ok((key, Self::from_host(value)?))
            }).collect::<Result<Vec<_>, _>>().map(Self::Object),
            _ => Err("wire documents contain only strings, arrays, and objects".to_string()),
        }
    }
}

impl deser::Serialize for WireValue {
    fn serialize<'a>(value: &'a Self, state: &mut deser::State) -> Result<deser::ser::Emit<'a>, deser::Error> {
        match value {
            Self::String(text) => <String as deser::Serialize>::serialize(text, state),
            Self::Array(values) => <Vec<WireValue> as deser::Serialize>::serialize(values, state),
            Self::Object(fields) => Ok(deser::ser::Emit::structure(WireFields(fields.iter()), state)),
        }
    }
}

struct WireFields<'a>(std::slice::Iter<'a, (String, WireValue)>);

impl deser::ser::StructEmitter for WireFields<'_> {
    fn next(&mut self, _: &mut deser::State) -> Result<Option<(std::borrow::Cow<'_, str>, deser::ser::SerializeHandle<'_>)>, deser::Error> {
        Ok(self.0.next().map(|(key, value)| (std::borrow::Cow::Borrowed(key.as_str()), deser::ser::SerializeHandle::to(value))))
    }
}

fn wire_object<const N: usize>(fields: [(&str, WireValue); N]) -> WireValue {
    WireValue::Object(fields.into_iter().map(|(key, value)| (key.to_string(), value)).collect())
}

fn wire_string(text: impl Into<String>) -> WireValue {
    WireValue::String(text.into())
}

fn parse_object_fields(value: WireValue, allowed: &[&str]) -> Result<BTreeMap<String, WireValue>, String> {
    let WireValue::Object(fields) = value else {
        return Err("wire record is not a JSON object".to_string());
    };
    let mut result = BTreeMap::new();
    for (name, value) in fields {
        if !allowed.contains(&name.as_str()) {
            return Err(format!("unknown wire field {name:?}"));
        }
        if result.insert(name.clone(), value).is_some() {
            return Err(format!("duplicate wire field {name:?}"));
        }
    }
    for name in allowed {
        if !result.contains_key(*name) {
            return Err(format!("missing required wire field {name:?}"));
        }
    }
    Ok(result)
}

fn take_string(fields: &mut BTreeMap<String, WireValue>, name: &str) -> Result<String, String> {
    match fields.remove(name) {
        Some(WireValue::String(value)) => Ok(value),
        Some(_) => Err(format!("wire field {name:?} is not a string")),
        None => Err(format!("missing required wire field {name:?}")),
    }
}

fn take_array(fields: &mut BTreeMap<String, WireValue>, name: &str) -> Result<Vec<WireValue>, String> {
    match fields.remove(name) {
        Some(WireValue::Array(value)) => Ok(value),
        Some(_) => Err(format!("wire field {name:?} is not an array")),
        None => Err(format!("missing required wire field {name:?}")),
    }
}

fn parse_hex_u32(text: &str) -> Result<u32, String> {
    if text.len() != 8 || !text.bytes().all(is_lower_hex) {
        return Err("u32 wire words and IDs require eight lowercase hex digits".to_string());
    }
    u32::from_str_radix(text, 16).map_err(|_| "invalid u32 hex word".to_string())
}

fn parse_hex_u64(text: &str) -> Result<u64, String> {
    if text.len() != 16 || !text.bytes().all(is_lower_hex) {
        return Err("u64 wire values require sixteen lowercase hex digits".to_string());
    }
    u64::from_str_radix(text, 16).map_err(|_| "invalid u64 hex value".to_string())
}

fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

fn hex_u32(raw: u32) -> String {
    format!("{raw:08x}")
}

fn hex_u64(raw: u64) -> String {
    format!("{raw:016x}")
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0xf) as usize] as char);
    }
    output
}

fn parse_hex_bytes(text: &str) -> Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(2) || !text.bytes().all(is_lower_hex) {
        return Err("byte strings require an even count of lowercase hex digits".to_string());
    }
    text.as_bytes()
        .as_chunks::<2>().0.iter()
        .map(|pair| {
            let digits = std::str::from_utf8(pair).expect("hex digits are ASCII");
            u8::from_str_radix(digits, 16).map_err(|_| "invalid byte hex".to_string())
        })
        .collect()
}

fn encode_source_document(document: &SourceDocument) -> String {
    json_text(&wire_object([
        ("schema", wire_string(SOURCE_SCHEMA)),
        ("id", wire_string(hex_u32(document.id.raw()))),
        ("text", wire_string(&document.text)),
    ]))
}

fn decode_source_document(input: &str) -> Result<SourceDocument, String> {
    let mut fields = parse_object_fields(
        parse_wire_document(input)?,
        &["schema", "id", "text"],
    )?;
    if take_string(&mut fields, "schema")? != SOURCE_SCHEMA {
        return Err("unknown source document version".to_string());
    }
    let id = IrId::from_raw(parse_hex_u32(&take_string(&mut fields, "id")?)?)?;
    let text = take_string(&mut fields, "text")?;
    Ok(SourceDocument { id, text })
}

fn validate_raw_document(document: &RawIrDocument) -> Result<(), String> {
    let mut binder_ids = BTreeSet::new();
    let mut current_function = None;
    // The projection emits each function row immediately before its parameters.
    for (index, binder) in document.binders.iter().enumerate() {
        let expected = IrId::from_index(index)?;
        if binder.id != expected || !binder_ids.insert(binder.id) {
            return Err("binder IDs must be unique and follow source table order".to_string());
        }
        match (binder.role, binder.owner.0) {
            (BinderRole::Function, None) => {
                if binder.text.is_empty() {
                    return Err("function binder projection is invalid".to_string());
                }
                current_function = Some(binder.id);
            }
            (BinderRole::Parameter, Some(owner)) => {
                if binder.text.is_empty() || current_function != Some(owner) {
                    return Err("parameter binder does not belong to the current function".to_string());
                }
            }
            _ => return Err("binder role and owner do not form a valid state".to_string()),
        }
    }

    let mut previous = None;
    for instruction in &document.instructions {
        if instruction.id.raw() > document.instruction_count {
            return Err("literal instruction ID is outside the instruction table".to_string());
        }
        if previous.is_some_and(|id: IrId| id >= instruction.id) {
            return Err("literal instruction IDs must preserve table order".to_string());
        }
        previous = Some(instruction.id);
        if u64::from(instruction.words[0]) | (u64::from(instruction.words[1]) << 32)
            != instruction.value_bits
        {
            return Err("typed scalar bits disagree with low/high raw words".to_string());
        }
    }

    match document.driver_root.0 {
        Some(root) if root.raw() <= document.driver_program_count => {}
        Some(_) => return Err("driver root ID is outside the driver program table".to_string()),
        None if document.driver_program_count == 0 => {}
        None => return Err("driver program table has no root".to_string()),
    }

    for bytes in &document.native_path_inputs {
        PathValue::new(bytes.clone())
            .map_err(|_| "native path input contains a forbidden NUL byte".to_string())?;
    }
    Ok(())
}

fn encode_raw_ir_document(document: &RawIrDocument) -> Result<String, String> {
    validate_raw_document(document)?;
    let value = wire_object([
        ("schema", wire_string(RAW_IR_SCHEMA)),
        ("sourceId", wire_string(hex_u32(document.source_id.raw()))),
        ("instructionCount", wire_string(hex_u32(document.instruction_count))),
        ("driverProgramCount", wire_string(hex_u32(document.driver_program_count))),
        ("scope", wire_string(RAW_IR_SCOPE)),
        ("driverRoot", wire_string(hex_u32(document.driver_root.raw()))),
        ("binderProjection", WireValue::Array(document.binders.iter().map(|binder| wire_object([
            ("id", wire_string(hex_u32(binder.id.raw()))),
            ("owner", wire_string(hex_u32(binder.owner.raw()))),
            ("role", wire_string(binder.role.name())),
            ("text", wire_string(&binder.text)),
        ])).collect())),
        ("instructions", WireValue::Array(document.instructions.iter().map(|instruction| wire_object([
            ("id", wire_string(hex_u32(instruction.id.raw()))),
            ("tag", wire_string(instruction.tag.name())),
            ("ordinal", wire_string(hex_u32(instruction.tag.ordinal()))),
            ("words", WireValue::Array(instruction.words.iter().map(|word| wire_string(hex_u32(*word))).collect())),
            ("value", wire_string(hex_u64(instruction.value_bits))),
        ])).collect())),
        ("nativePathInputs", WireValue::Array(document.native_path_inputs.iter().map(|path| wire_string(hex_bytes(path))).collect())),
        ("integerOutputBits", WireValue::Array(document.integer_output_bits.iter().map(|bits| wire_string(hex_u64(*bits))).collect())),
        ("floatOutputBits", WireValue::Array(document.float_output_bits.iter().map(|bits| wire_string(hex_u64(*bits))).collect())),
        ("serializationOnlyFloatBits", WireValue::Array(document.serialization_only_float_bits.iter().map(|bits| wire_string(hex_u64(*bits))).collect())),
    ]);
    deser_json::to_string(&value).map_err(|error| error.to_string())
}

fn decode_optional_ir_id(text: &str) -> Result<OptionalIrId, String> {
    OptionalIrId::from_raw(parse_hex_u32(text)?)
}

fn decode_raw_ir_document(input: &str) -> Result<RawIrDocument, String> {
    let mut fields = parse_object_fields(
        parse_wire_document(input)?,
        &[
            "schema",
            "sourceId",
            "instructionCount",
            "driverProgramCount",
            "scope",
            "driverRoot",
            "binderProjection",
            "instructions",
            "nativePathInputs",
            "integerOutputBits",
            "floatOutputBits",
            "serializationOnlyFloatBits",
        ],
    )?;
    if take_string(&mut fields, "schema")? != RAW_IR_SCHEMA {
        return Err("unknown raw-IR document version".to_string());
    }
    let source_id = IrId::from_raw(parse_hex_u32(&take_string(&mut fields, "sourceId")?)?)?;
    let instruction_count = parse_hex_u32(&take_string(&mut fields, "instructionCount")?)?;
    let driver_program_count =
        parse_hex_u32(&take_string(&mut fields, "driverProgramCount")?)?;
    if take_string(&mut fields, "scope")? != RAW_IR_SCOPE {
        return Err("unsupported raw-IR projection scope".to_string());
    }
    let driver_root = decode_optional_ir_id(&take_string(&mut fields, "driverRoot")?)?;
    let binders = take_array(&mut fields, "binderProjection")?
        .into_iter()
        .map(decode_binder)
        .collect::<Result<Vec<_>, _>>()?;
    let instructions = take_array(&mut fields, "instructions")?
        .into_iter()
        .map(decode_instruction)
        .collect::<Result<Vec<_>, _>>()?;
    let native_path_inputs = take_array(&mut fields, "nativePathInputs")?
        .into_iter()
        .map(|value| {
            let WireValue::String(text) = value else {
                return Err("native path entry is not a hex string".to_string());
            };
            let bytes = parse_hex_bytes(&text)?;
            PathValue::new(bytes.clone())
                .map_err(|_| "native path input contains a forbidden NUL byte".to_string())?;
            Ok(bytes)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let integer_output_bits = take_array(&mut fields, "integerOutputBits")?
        .into_iter()
        .map(|value| match value {
            WireValue::String(text) => parse_hex_u64(&text),
            _ => Err("integer output bit pattern is not a hex string".to_string()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let float_output_bits = take_array(&mut fields, "floatOutputBits")?
        .into_iter()
        .map(|value| match value {
            WireValue::String(text) => parse_hex_u64(&text),
            _ => Err("Float output bit pattern is not a hex string".to_string()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let serialization_only_float_bits = take_array(&mut fields, "serializationOnlyFloatBits")?
        .into_iter()
        .map(|value| match value {
            WireValue::String(text) => parse_hex_u64(&text),
            _ => Err("serialized Float bit sample is not a hex string".to_string()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let document = RawIrDocument {
        source_id,
        instruction_count,
        driver_program_count,
        driver_root,
        binders,
        instructions,
        integer_output_bits,
        float_output_bits,
        native_path_inputs,
        serialization_only_float_bits,
    };
    validate_raw_document(&document)?;
    Ok(document)
}

/// Decode the source and raw-IR documents as one artifact and keep their IDs paired.
fn decode_source_raw_pair(
    source_input: &str,
    raw_input: &str,
) -> Result<(SourceDocument, RawIrDocument), String> {
    let source = decode_source_document(source_input)?;
    let raw = decode_raw_ir_document(raw_input)?;
    if raw.source_id != source.id {
        return Err("raw-IR document does not name its source document".to_string());
    }
    Ok((source, raw))
}

fn decode_binder(value: WireValue) -> Result<BinderProjection, String> {
    let mut fields = parse_object_fields(value, &["id", "owner", "role", "text"])?;
    let id = IrId::from_raw(parse_hex_u32(&take_string(&mut fields, "id")?)?)?;
    let owner = decode_optional_ir_id(&take_string(&mut fields, "owner")?)?;
    let role = BinderRole::parse(&take_string(&mut fields, "role")?)?;
    let text = take_string(&mut fields, "text")?;
    Ok(BinderProjection {
        id,
        owner,
        role,
        text,
    })
}

fn decode_instruction(value: WireValue) -> Result<ScalarInstruction, String> {
    let mut fields = parse_object_fields(value, &["id", "tag", "ordinal", "words", "value"])?;
    let id = IrId::from_raw(parse_hex_u32(&take_string(&mut fields, "id")?)?)?;
    let tag = LiteralTag::from_name(&take_string(&mut fields, "tag")?)?;
    let ordinal = parse_hex_u32(&take_string(&mut fields, "ordinal")?)?;
    if ordinal != tag.ordinal() {
        return Err(format!(
            "raw-IR tag {} has ordinal {ordinal:08x}, expected {:08x}",
            tag.name(),
            tag.ordinal()
        ));
    }
    let words = take_array(&mut fields, "words")?;
    if words.len() != 2 {
        return Err("64-bit scalar payload must contain low and high words".to_string());
    }
    let mut words = words.into_iter();
    let parse_word = |value| match value {
        WireValue::String(text) => parse_hex_u32(&text),
        _ => Err("raw-IR payload word is not a hex string".to_string()),
    };
    let words = [
        parse_word(words.next().expect("two payload words"))?,
        parse_word(words.next().expect("two payload words"))?,
    ];
    let value_bits = parse_hex_u64(&take_string(&mut fields, "value")?)?;
    if u64::from(words[0]) | (u64::from(words[1]) << 32) != value_bits {
        return Err("typed scalar bits disagree with low/high raw words".to_string());
    }
    Ok(ScalarInstruction {
        id,
        tag,
        words,
        value_bits,
    })
}

fn export_binder_projection(arena: &ArenaProgram) -> Result<Vec<BinderProjection>, String> {
    let mut binders = Vec::new();
    for (function_index, function) in arena.arena.function_defs.iter().enumerate() {
        let owner = IrId::from_index(binders.len())?;
        binders.push(BinderProjection {
            id: owner,
            owner: OptionalIrId(None),
            role: BinderRole::Function,
            text: name_text(function.name),
        });
        let parameter_end = (function.params.start as usize)
            .checked_add(function.params.len())
            .ok_or_else(|| "arena parameter range overflow".to_string())?;
        let params = arena
            .arena
            .params
            .get(function.params.start as usize..parameter_end)
            .ok_or_else(|| "arena parameter range is out of bounds".to_string())?;
        for param in params {
            binders.push(BinderProjection {
                id: IrId::from_index(binders.len())?,
                owner: OptionalIrId(Some(owner)),
                role: BinderRole::Parameter,
                text: name_text(param.name),
            });
        }
        if function_index >= u32::MAX as usize {
            return Err("too many arena function binders".to_string());
        }
    }
    Ok(binders)
}

fn name_text(name: Name) -> String {
    name.as_str().to_string()
}

fn export_raw_ir_document(
    program: &FullProgram,
    arena: &ArenaProgram,
    source_id: IrId,
    integer_output_bits: Vec<u64>,
    float_output_bits: Vec<u64>,
    native_path_inputs: Vec<Vec<u8>>,
) -> Result<RawIrDocument, String> {
    FullVerifier::verify(program).map_err(|error| error.message)?;
    if source_id.raw().checked_sub(1) != Some(program.store.source_id.raw() as u32) {
        return Err("raw-IR document does not name its source document".to_string());
    }

    let binders = export_binder_projection(arena)?;
    let function_names = arena
        .arena
        .function_defs
        .iter()
        .map(|function| name_text(function.name))
        .collect::<Vec<_>>();
    let lowered_function_names = program
        .store
        .functions
        .iter()
        .map(|function| program.store.string(function.name).map(str::to_string))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.message)?;
    if function_names != lowered_function_names {
        return Err("arena function names do not match the lowered function table".to_string());
    }
    for (syntax_function, lowered_function) in arena
        .arena
        .function_defs
        .iter()
        .zip(&program.store.functions)
    {
        let syntax_end = (syntax_function.params.start as usize)
            .checked_add(syntax_function.params.len())
            .ok_or_else(|| "arena parameter range overflow".to_string())?;
        let syntax_params = arena
            .arena
            .params
            .get(syntax_function.params.start as usize..syntax_end)
            .ok_or_else(|| "arena parameter range is out of bounds".to_string())?
            .iter()
            .map(|param| name_text(param.name))
            .collect::<Vec<_>>();
        let lowered_range = lowered_function
            .params
            .bounds(program.store.params.len())
            .ok_or_else(|| "lowered parameter range is out of bounds".to_string())?;
        let lowered_params = program.store.params[lowered_range]
            .iter()
            .map(|param| program.store.string(param.name).map(str::to_string))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.message)?;
        if syntax_params != lowered_params {
            return Err(
                "arena parameter names do not match the lowered parameter table".to_string(),
            );
        }
    }

    let mut instructions = Vec::new();
    for (index, tag) in program.store.tags.iter().copied().enumerate() {
        let Some(literal_tag) = LiteralTag::from_full_tag(tag) else {
            if matches!(tag, FullTag::StmtReturn | FullTag::ExprParam | FullTag::ExprBinary) {
                continue;
            }
            return Err(format!(
                "instruction tag {tag:?} is outside the scalar-literal projection"
            ));
        };
        let data = program
            .store
            .data
            .get(index)
            .ok_or_else(|| "literal instruction has no data row".to_string())?;
        let words = program
            .store
            .payload(data.range())
            .map_err(|error| error.message)?;
        let [low, high] = words else {
            return Err("64-bit literal payload does not have two raw words".to_string());
        };
        instructions.push(ScalarInstruction {
            id: IrId::from_index(index)?,
            tag: literal_tag,
            words: [*low, *high],
            value_bits: u64::from(*low) | (u64::from(*high) << 32),
        });
    }

    let document = RawIrDocument {
        source_id,
        instruction_count: u32::try_from(program.store.tags.len())
            .map_err(|_| "instruction table exceeds the u32 wire count".to_string())?,
        driver_program_count: u32::try_from(program.store.driver_programs.len())
            .map_err(|_| "driver program table exceeds the u32 wire count".to_string())?,
        driver_root: OptionalIrId::from_raw(program.store.driver_root)?,
        binders,
        instructions,
        integer_output_bits,
        float_output_bits,
        native_path_inputs,
        serialization_only_float_bits: vec![SERIALIZATION_ONLY_NAN, SERIALIZATION_ONLY_NAN],
    };
    validate_raw_document(&document)?;
    Ok(document)
}

fn build_fixture(
    source: &str,
) -> Result<(SourceMap, crate::syntax::parser::ArenaParseOutput, FullProgram), String> {
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("formal-bridge.xsh", source.to_string());
    let parsed = crate::syntax::parser::Parser::parse_source_arena_only(source_id, source);
    if !parsed.diagnostics.is_empty() {
        return Err(format!("fixture parser diagnostics: {:?}", parsed.diagnostics));
    }
    let declarations = crate::sema::check::Checker::check_compact_declarations(
        &parsed.arena,
        &sources,
        source_id,
    );
    if !declarations.diagnostics.is_empty() {
        return Err(format!(
            "fixture checker diagnostics: {:?}",
            declarations.diagnostics
        ));
    }
    let program = super::FullBuilder::build_compact(
        &parsed.arena,
        &declarations,
        source,
        Arc::new(sources.clone()),
        source_id,
    )
    .map_err(|error| format!("fixture lowerer failed: {error:?}"))?;
    FullVerifier::verify(&program).map_err(|error| error.message)?;
    Ok((sources, parsed, program))
}

fn source_document_id(source_id: SourceId) -> Result<IrId, String> {
    IrId::from_index(source_id.raw())
}

fn json_text(value: &WireValue) -> String {
    deser_json::to_string(value).expect("wire values contain only strings, arrays, and objects")
}

fn insert_field(value: &mut WireValue, name: &str, item: WireValue) {
    let WireValue::Object(fields) = value else {
        panic!("test mutation requires an object");
    };
    fields.push((name.to_string(), item));
}

fn set_field(value: &mut WireValue, name: &str, item: WireValue) {
    let WireValue::Object(fields) = value else {
        panic!("test mutation requires an object");
    };
    let field = fields
        .iter_mut()
        .find(|(field, _)| field == name)
        .expect("test mutation field exists");
    field.1 = item;
}

fn object_field<'a>(value: &'a mut WireValue, name: &str) -> &'a mut WireValue {
    let WireValue::Object(fields) = value else {
        panic!("test mutation requires an object");
    };
    &mut fields
        .iter_mut()
        .find(|(field, _)| field == name)
        .expect("test mutation field exists")
        .1
}

fn parsed_raw_mutation(
    encoded: &str,
    mutate: impl FnOnce(&mut WireValue),
) -> String {
    let mut value = parse_wire_document(encoded).expect("the encoder emits valid JSON");
    mutate(&mut value);
    json_text(&value)
}

fn fixture_binders(functions: &[ArenaFunctionDef]) -> Vec<String> {
    functions.iter().map(|function| name_text(function.name)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::eval::{LoweredFunctionKey, CallableKind};
    use crate::runtime::value::Value;

    const FIXTURE_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/formal-bridge-scalar-literals.xsh"
    ));

    fn set_nested_field(
        value: &mut WireValue,
        list_name: &str,
        row_index: usize,
        field: &str,
        item: WireValue,
    ) {
        let rows = object_field(value, list_name);
        let WireValue::Array(rows) = rows else {
            panic!("test mutation field is an array");
        };
        set_field(&mut rows[row_index], field, item);
    }

    fn function_name(program: &FullProgram, text: &str) -> Name {
        program.symbol_owner().with_current(|| Name::intern(text))
    }

    fn call_signed_min(program: &FullProgram, sources: &SourceMap) -> i64 {
        let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(
            Vec::new(),
            sources.clone(),
        );
        evaluator.indexed_program = Some(Arc::new(program.clone()));
        match evaluator
            .call_indexed_direct(
                LoweredFunctionKey::Name(function_name(program, "signed_min")),
                CallableKind::Pure,
                &[Value::Int(i64::MAX)],
                Span::new(program.store.source_id, 0, 0),
            )
            .expect("signed_min is present")
            .expect("signed_min returns a value")
        {
            Value::Int(value) => value,
            other => panic!("signed_min returned {other:?}"),
        }
    }

    fn call_negative_zero(program: &FullProgram, sources: &SourceMap) -> u64 {
        let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(
            Vec::new(),
            sources.clone(),
        );
        evaluator.indexed_program = Some(Arc::new(program.clone()));
        match evaluator
            .call_indexed_direct(
                LoweredFunctionKey::Name(function_name(program, "negative_zero")),
                CallableKind::Pure,
                &[],
                Span::new(program.store.source_id, 0, 0),
            )
            .expect("negative_zero is present")
            .expect("negative_zero returns a value")
        {
            Value::Float(value) => value.0.to_bits(),
            other => panic!("negative_zero returned {other:?}"),
        }
    }

    fn base_documents() -> (
        SourceDocument,
        RawIrDocument,
        SourceMap,
        crate::syntax::parser::ArenaParseOutput,
        FullProgram,
    ) {
        let source = SourceDocument {
            id: source_document_id(SourceId::new(0)).unwrap(),
            text: FIXTURE_SOURCE.to_string(),
        };
        let encoded_source = encode_source_document(&source);
        let source = decode_source_document(&encoded_source).unwrap();
        let (sources, parsed, program) = build_fixture(&source.text).unwrap();
        let paths = vec![b"native/raw-\xff".to_vec(), b"\x80tail".to_vec()];
        let integer_output_bits = vec![call_signed_min(&program, &sources) as u64];
        let float_output_bits = vec![call_negative_zero(&program, &sources)];
        let raw = export_raw_ir_document(
            &program,
            &parsed.arena,
            source.id,
            integer_output_bits,
            float_output_bits,
            paths,
        )
        .unwrap();
        (source, raw, sources, parsed, program)
    }

    #[test]
    fn private_scalar_projection_roundtrips_and_rejects_malformed_documents() {
        let (source, raw, sources, parsed, program) = base_documents();
        FullVerifier::verify(&program).unwrap();

        let source_encoded = encode_source_document(&source);
        assert_eq!(decode_source_document(&source_encoded).unwrap(), source);
        assert_eq!(
            encode_source_document(&decode_source_document(&source_encoded).unwrap()),
            source_encoded
        );

        let raw_encoded = encode_raw_ir_document(&raw).unwrap();
        let (paired_source, decoded) =
            decode_source_raw_pair(&source_encoded, &raw_encoded).unwrap();
        assert_eq!(paired_source, source);
        assert_eq!(decoded, raw);
        assert_eq!(encode_raw_ir_document(&decoded).unwrap(), raw_encoded);
        assert_eq!(decoded.source_id, source.id);
        assert_eq!(decoded.instruction_count, program.store.tags.len() as u32);
        assert_eq!(
            decoded.driver_program_count,
            program.store.driver_programs.len() as u32
        );
        assert_eq!(
            decoded.driver_root,
            OptionalIrId::from_raw(program.store.driver_root).unwrap()
        );
        assert!(IrId::from_raw(0).is_err());
        assert!(IrId::from_raw(IR_NONE).is_err());
        assert_eq!(
            decode_optional_ir_id("ffffffff").unwrap(),
            OptionalIrId(None)
        );
        assert_eq!(hex_u32(OptionalIrId(None).raw()), "ffffffff");
        assert_eq!(decoded.integer_output_bits, [i64::MIN as u64]);
        assert_eq!(hex_u64(decoded.integer_output_bits[0]), "8000000000000000");
        assert_eq!(decoded.float_output_bits, [1_u64 << 63]);

        let ints = decoded
            .instructions
            .iter()
            .filter(|instruction| instruction.tag == LiteralTag::Int)
            .map(|instruction| instruction.value_bits as i64)
            .collect::<Vec<_>>();
        assert_eq!(ints, [0, 1, i64::MAX]);
        let durations = decoded
            .instructions
            .iter()
            .filter(|instruction| instruction.tag == LiteralTag::Duration)
            .map(|instruction| DurationValue { millis: instruction.value_bits })
            .collect::<Vec<_>>();
        assert_eq!(durations, [DurationValue { millis: u64::MAX }]);
        assert_eq!(
            decoded
                .instructions
                .iter()
                .find(|instruction| instruction.tag == LiteralTag::Duration)
                .map(|instruction| hex_u64(instruction.value_bits)),
            Some("ffffffffffffffff".to_string())
        );
        let floats = decoded
            .instructions
            .iter()
            .filter(|instruction| instruction.tag == LiteralTag::Float)
            .map(|instruction| FloatValue(f64::from_bits(instruction.value_bits)))
            .collect::<Vec<_>>();
        assert_eq!(
            floats.iter().map(|value| value.0.to_bits()).collect::<Vec<_>>(),
            [0, 0, (-1.0_f64).to_bits()]
        );
        assert_eq!(decoded.native_path_inputs, [b"native/raw-\xff".to_vec(), b"\x80tail".to_vec()]);
        assert!(raw_encoded.contains("6e61746976652f7261772dff"));
        assert!(raw_encoded.contains("807461696c"));
        assert_eq!(
            decoded.serialization_only_float_bits,
            [SERIALIZATION_ONLY_NAN, SERIALIZATION_ONLY_NAN]
        );
        assert_eq!(hex_u64(decoded.serialization_only_float_bits[0]), "7ff8000000000042");

        let expected_binders = fixture_binders(&parsed.arena.arena.function_defs);
        let projected_function_binders = decoded
            .binders
            .iter()
            .filter(|binder| binder.role == BinderRole::Function)
            .map(|binder| binder.text.clone())
            .collect::<Vec<_>>();
        assert_eq!(projected_function_binders, expected_binders);
        assert!(decoded.binders.iter().any(|binder| {
            binder.role == BinderRole::Parameter
                && binder.text == "input"
                && binder.owner.0.is_some()
        }));

        for bytes in &decoded.native_path_inputs {
            let path = PathValue::new(bytes.clone()).unwrap();
            let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(
                Vec::new(),
                sources.clone(),
            );
            evaluator.indexed_program = Some(Arc::new(program.clone()));
            let returned = evaluator
                .call_indexed_direct(
                    LoweredFunctionKey::Name(function_name(&program, "path_echo")),
                    CallableKind::Pure,
                    &[Value::Path(path.clone())],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("path_echo is present")
                .expect("path_echo returns its input");
            assert_eq!(returned, Value::Path(path));
        }

        let unknown_version = parsed_raw_mutation(&raw_encoded, |value| {
            set_field(value, "schema", WireValue::String("xsh-raw-ir-v2".to_string()));
        });
        assert!(decode_raw_ir_document(&unknown_version).is_err());

        let unknown_field = parsed_raw_mutation(&raw_encoded, |value| {
            insert_field(value, "unrecognized", WireValue::String("rejected".to_string()));
        });
        assert!(decode_raw_ir_document(&unknown_field).is_err());

        let duplicate_field = parsed_raw_mutation(&raw_encoded, |value| {
            insert_field(value, "schema", WireValue::String(RAW_IR_SCHEMA.to_string()));
        });
        assert!(decode_raw_ir_document(&duplicate_field).is_err());

        let unknown_tag = parsed_raw_mutation(&raw_encoded, |value| {
            set_nested_field(
                value,
                "instructions",
                0,
                "tag",
                WireValue::String("ExprUnknown".to_string()),
            );
        });
        assert!(decode_raw_ir_document(&unknown_tag).is_err());

        let bad_ordinal = parsed_raw_mutation(&raw_encoded, |value| {
            set_nested_field(
                value,
                "instructions",
                0,
                "ordinal",
                WireValue::String("000000ff".to_string()),
            );
        });
        assert!(decode_raw_ir_document(&bad_ordinal).is_err());

        let short_payload = parsed_raw_mutation(&raw_encoded, |value| {
            let words = object_field(value, "instructions");
            let WireValue::Array(rows) = words else {
                panic!("test mutation field is an array");
            };
            set_field(
                &mut rows[0],
                "words",
                WireValue::Array(vec![WireValue::String("00000000".to_string())]),
            );
        });
        assert!(decode_raw_ir_document(&short_payload).is_err());

        let duplicate_id = parsed_raw_mutation(&raw_encoded, |value| {
            let instructions = object_field(value, "instructions");
            let WireValue::Array(rows) = instructions else {
                panic!("test mutation field is an array");
            };
            let first = match object_field(&mut rows[0], "id") {
                WireValue::String(id) => id.clone(),
                _ => panic!("encoded ID is a string"),
            };
            set_field(&mut rows[1], "id", WireValue::String(first));
        });
        assert!(decode_raw_ir_document(&duplicate_id).is_err());

        let ghost_instruction = parsed_raw_mutation(&raw_encoded, |value| {
            set_nested_field(
                value,
                "instructions",
                0,
                "id",
                WireValue::String(hex_u32(raw.instruction_count + 1)),
            );
        });
        assert!(decode_raw_ir_document(&ghost_instruction).is_err());

        let dangling_driver = parsed_raw_mutation(&raw_encoded, |value| {
            set_field(
                value,
                "driverRoot",
                WireValue::String(hex_u32(raw.driver_program_count + 1)),
            );
        });
        assert!(decode_raw_ir_document(&dangling_driver).is_err());

        let parameter_index = raw
            .binders
            .iter()
            .position(|binder| binder.role == BinderRole::Parameter)
            .expect("fixture has a parameter binder");
        let dangling_owner_id = IrId::from_index(raw.binders.len()).unwrap();
        let dangling_owner = parsed_raw_mutation(&raw_encoded, |value| {
            set_nested_field(
                value,
                "binderProjection",
                parameter_index,
                "owner",
                WireValue::String(hex_u32(dangling_owner_id.raw())),
            );
        });
        assert!(decode_raw_ir_document(&dangling_owner).is_err());

        let wrong_word_width = parsed_raw_mutation(&raw_encoded, |value| {
            set_nested_field(value, "instructions", 0, "value", WireValue::String("1".to_string()));
        });
        assert!(decode_raw_ir_document(&wrong_word_width).is_err());

        let mut malformed_program = program.clone();
        let int_instruction = malformed_program
            .store
            .tags
            .iter()
            .position(|tag| *tag == FullTag::ExprInt)
            .expect("the fixture contains an integer literal");
        malformed_program.store.data[int_instruction].rhs = 1;
        assert!(FullVerifier::verify(&malformed_program).is_err());
    }

    #[test]
    fn source_document_rejects_duplicate_and_unknown_fields() {
        let source = SourceDocument {
            id: IrId::from_raw(1).unwrap(),
            text: "print \"é\"\n".to_string(),
        };
        let encoded = encode_source_document(&source);
        assert_eq!(decode_source_document(&encoded).unwrap(), source);
        let duplicate = parsed_raw_mutation(&encoded, |value| {
            insert_field(value, "schema", WireValue::String(SOURCE_SCHEMA.to_string()));
        });
        assert!(decode_source_document(&duplicate).is_err());
        let unknown = parsed_raw_mutation(&encoded, |value| {
            insert_field(value, "extra", WireValue::String("rejected".to_string()));
        });
        assert!(decode_source_document(&unknown).is_err());
        let unknown_version = parsed_raw_mutation(&encoded, |value| {
            set_field(value, "schema", WireValue::String("xsh-source-v2".to_string()));
        });
        assert!(decode_source_document(&unknown_version).is_err());
        for reserved_id in [0, IR_NONE] {
            let invalid_id = parsed_raw_mutation(&encoded, |value| {
                set_field(value, "id", WireValue::String(hex_u32(reserved_id)));
            });
            assert!(decode_source_document(&invalid_id).is_err());
        }
        assert!(encoded.contains("é"), "UTF-8 source text stays readable and exact");

        let escaped_unicode =
            r#"{"schema":"xsh-source-v1","id":"00000001","text":"caf\u00e9 \ud83d\ude00"}"#;
        assert_eq!(
            decode_source_document(escaped_unicode).unwrap().text,
            "café 😀"
        );
    }

    #[test]
    fn raw_transport_rejects_reserved_ids_and_invalid_driver_roots() {
        let (source, raw, _, parsed, program) = base_documents();
        let encoded = encode_raw_ir_document(&raw).unwrap();

        for reserved_id in [0, IR_NONE] {
            let invalid_source_id = parsed_raw_mutation(&encoded, |value| {
                set_field(value, "sourceId", WireValue::String(hex_u32(reserved_id)));
            });
            assert!(decode_raw_ir_document(&invalid_source_id).is_err());

            let invalid_instruction_id = parsed_raw_mutation(&encoded, |value| {
                set_nested_field(
                    value,
                    "instructions",
                    0,
                    "id",
                    WireValue::String(hex_u32(reserved_id)),
                );
            });
            assert!(decode_raw_ir_document(&invalid_instruction_id).is_err());
        }

        let zero_driver_root = parsed_raw_mutation(&encoded, |value| {
            set_field(value, "driverRoot", WireValue::String(hex_u32(0)));
        });
        assert!(decode_raw_ir_document(&zero_driver_root).is_err());

        let missing_driver_root = parsed_raw_mutation(&encoded, |value| {
            set_field(value, "driverProgramCount", WireValue::String(hex_u32(1)));
            set_field(value, "driverRoot", WireValue::String(hex_u32(IR_NONE)));
        });
        assert!(decode_raw_ir_document(&missing_driver_root).is_err());

        let other_source_id = IrId::from_raw(source.id.raw() + 1).unwrap();
        let source_encoded = encode_source_document(&source);
        let mismatched_source_document = parsed_raw_mutation(&encoded, |value| {
            set_field(
                value,
                "sourceId",
                WireValue::String(hex_u32(other_source_id.raw())),
            );
        });
        assert!(
            decode_source_raw_pair(&source_encoded, &mismatched_source_document).is_err()
        );

        assert!(export_raw_ir_document(
            &program,
            &parsed.arena,
            other_source_id,
            raw.integer_output_bits.clone(),
            raw.float_output_bits.clone(),
            raw.native_path_inputs.clone(),
        )
        .is_err());
    }

    #[test]
    fn raw_transport_rejects_row_word_binder_and_path_corruption() {
        let (_, raw, _, _, _) = base_documents();
        let encoded = encode_raw_ir_document(&raw).unwrap();

        let reordered_instructions = parsed_raw_mutation(&encoded, |value| {
            let instructions = object_field(value, "instructions");
            let WireValue::Array(rows) = instructions else {
                panic!("test mutation field is an array");
            };
            rows.swap(0, 1);
        });
        assert!(decode_raw_ir_document(&reordered_instructions).is_err());

        let inconsistent_words = parsed_raw_mutation(&encoded, |value| {
            let replacement = raw.instructions[0].words[0] ^ 1;
            set_nested_field(
                value,
                "instructions",
                0,
                "words",
                WireValue::Array(vec![
                    WireValue::String(hex_u32(replacement)),
                    WireValue::String(hex_u32(raw.instructions[0].words[1])),
                ]),
            );
        });
        assert!(decode_raw_ir_document(&inconsistent_words).is_err());

        let inconsistent_value = parsed_raw_mutation(&encoded, |value| {
            set_nested_field(
                value,
                "instructions",
                0,
                "value",
                WireValue::String(hex_u64(raw.instructions[0].value_bits ^ 1)),
            );
        });
        assert!(decode_raw_ir_document(&inconsistent_value).is_err());

        let empty_function_name = parsed_raw_mutation(&encoded, |value| {
            set_nested_field(
                value,
                "binderProjection",
                0,
                "text",
                WireValue::String(String::new()),
            );
        });
        assert!(decode_raw_ir_document(&empty_function_name).is_err());

        let reordered_binders = parsed_raw_mutation(&encoded, |value| {
            let binders = object_field(value, "binderProjection");
            let WireValue::Array(rows) = binders else {
                panic!("test mutation field is an array");
            };
            rows.swap(0, 1);
        });
        assert!(decode_raw_ir_document(&reordered_binders).is_err());

        let parameter_index = raw
            .binders
            .iter()
            .position(|binder| binder.role == BinderRole::Parameter)
            .expect("fixture has a parameter binder");
        let later_function = raw
            .binders
            .iter()
            .skip(parameter_index + 1)
            .find(|binder| binder.role == BinderRole::Function)
            .expect("fixture has a function after its first parameter");
        let later_owner = parsed_raw_mutation(&encoded, |value| {
            set_nested_field(
                value,
                "binderProjection",
                parameter_index,
                "owner",
                WireValue::String(hex_u32(later_function.id.raw())),
            );
        });
        assert!(decode_raw_ir_document(&later_owner).is_err());

        let earlier_function = raw
            .binders
            .iter()
            .find(|binder| binder.role == BinderRole::Function)
            .expect("fixture has a function binder");
        let later_parameter_index = raw
            .binders
            .iter()
            .enumerate()
            .find(|(_, binder)| {
                binder.role == BinderRole::Parameter
                    && binder.owner.0 != Some(earlier_function.id)
            })
            .map(|(index, _)| index)
            .expect("fixture has a parameter after its first function");
        let earlier_owner = parsed_raw_mutation(&encoded, |value| {
            set_nested_field(
                value,
                "binderProjection",
                later_parameter_index,
                "owner",
                WireValue::String(hex_u32(earlier_function.id.raw())),
            );
        });
        assert!(decode_raw_ir_document(&earlier_owner).is_err());

        let nul_path = parsed_raw_mutation(&encoded, |value| {
            set_field(
                value,
                "nativePathInputs",
                WireValue::Array(vec![WireValue::String("00".to_string())]),
            );
        });
        assert!(decode_raw_ir_document(&nul_path).is_err());
    }

    #[test]
    fn json_transport_preserves_escaped_strings_and_rejects_non_protocol_atoms() {
        let input = r#"{"text":"quote: \" snow: \u2603 music: \uD834\uDD1E"}"#;
        let value = parse_wire_document(input).expect("protocol JSON");
        assert_eq!(parse_wire_document(&json_text(&value)).unwrap(), value);
        for input in ["null", "true", "42", "1.5", "[null]", r#"{"a":"one","a":"two"}"#, r#"{"a":{"x":"one","x":"two"}}"#] {
            assert!(parse_wire_document(input).is_err(), "{input}");
        }
    }

    #[test]
    fn json_transport_rejects_non_json_whitespace_and_bounds_input() {
        for whitespace in ['\u{000b}', '\u{000c}'] {
            let invalid = format!("[{whitespace}\"value\"]");
            assert!(parse_wire_document(&invalid).is_err());
        }

        let valid_whitespace = " [ \t\r\n\"value\" \t\r\n ] ";
        assert!(parse_wire_document(valid_whitespace).is_ok());

        let at_byte_limit = format!(
            "\"{}\"",
            "x".repeat(MAX_TRANSPORT_DOCUMENT_BYTES - 2)
        );
        assert_eq!(at_byte_limit.len(), MAX_TRANSPORT_DOCUMENT_BYTES);
        assert!(parse_wire_document(&at_byte_limit).is_ok());
        let over_byte_limit = format!("\"{}\"", "x".repeat(MAX_TRANSPORT_DOCUMENT_BYTES - 1));
        assert!(parse_wire_document(&over_byte_limit).is_err());
    }

    #[test]
    fn json_transport_rejects_container_nesting_beyond_the_fixed_limit() {
        let nested_arrays = |depth: usize| {
            format!(
                "{}\"value\"{}",
                "[".repeat(depth),
                "]".repeat(depth)
            )
        };
        let nested_objects = |depth: usize| {
            format!(
                "{}\"value\"{}",
                "{\"child\":".repeat(depth),
                "}".repeat(depth)
            )
        };

        assert!(parse_wire_document(&nested_arrays(MAX_JSON_CONTAINER_DEPTH)).is_ok());
        assert!(parse_wire_document(&nested_arrays(MAX_JSON_CONTAINER_DEPTH + 1)).is_err());
        assert!(parse_wire_document(&nested_objects(MAX_JSON_CONTAINER_DEPTH)).is_ok());
        assert!(parse_wire_document(&nested_objects(MAX_JSON_CONTAINER_DEPTH + 1)).is_err());
    }
}
