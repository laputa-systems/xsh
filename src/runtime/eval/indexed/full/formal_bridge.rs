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
// Keep this private fixture transport bounded without exposing parser tuning as a runtime option.
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
    Float,
    Duration,
}

impl LiteralTag {
    fn from_full_tag(tag: FullTag) -> Option<Self> {
        match tag {
            FullTag::ExprInt => Some(Self::Int),
            FullTag::ExprFloat => Some(Self::Float),
            FullTag::ExprDuration => Some(Self::Duration),
            _ => None,
        }
    }

    fn from_name(name: &str) -> Result<Self, String> {
        match name {
            "ExprInt" => Ok(Self::Int),
            "ExprFloat" => Ok(Self::Float),
            "ExprDuration" => Ok(Self::Duration),
            _ => Err(format!("unknown raw-IR literal tag {name:?}")),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Int => "ExprInt",
            Self::Float => "ExprFloat",
            Self::Duration => "ExprDuration",
        }
    }

    fn ordinal(self) -> u32 {
        match self {
            Self::Int => FullTag::ExprInt as u32,
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
enum Json {
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

struct JsonParser<'a> {
    input: &'a str,
    bytes: &'a [u8],
    index: usize,
}

impl<'a> JsonParser<'a> {
    fn parse(input: &'a str) -> Result<Json, String> {
        if input.len() > MAX_TRANSPORT_DOCUMENT_BYTES {
            return Err("wire document exceeds the private transport byte limit".to_string());
        }
        let mut parser = Self {
            input,
            bytes: input.as_bytes(),
            index: 0,
        };
        let value = parser.value(0)?;
        parser.whitespace();
        if parser.index != parser.bytes.len() {
            return Err("trailing data after JSON document".to_string());
        }
        Ok(value)
    }

    fn value(&mut self, parent_depth: usize) -> Result<Json, String> {
        self.whitespace();
        match self.bytes.get(self.index).copied() {
            Some(b'"') => self.string().map(Json::String),
            Some(b'[') => self.array(self.next_container_depth(parent_depth)?),
            Some(b'{') => self.object(self.next_container_depth(parent_depth)?),
            _ => Err("wire documents contain only strings, arrays, and objects".to_string()),
        }
    }

    fn next_container_depth(&self, parent_depth: usize) -> Result<usize, String> {
        parent_depth
            .checked_add(1)
            .filter(|depth| *depth <= MAX_JSON_CONTAINER_DEPTH)
            .ok_or_else(|| "wire document exceeds the private JSON nesting limit".to_string())
    }

    fn string(&mut self) -> Result<String, String> {
        let start = self.index;
        self.index += 1;
        while let Some(byte) = self.bytes.get(self.index).copied() {
            match byte {
                b'"' => {
                    self.index += 1;
                    return miniserde::json::from_str(&self.input[start..self.index])
                        .map_err(|_| "invalid JSON string".to_string());
                }
                b'\\' => {
                    self.index = self
                        .index
                        .checked_add(2)
                        .ok_or_else(|| "JSON string offset overflow".to_string())?;
                }
                0..=0x1f => return Err("unescaped control byte in JSON string".to_string()),
                _ => self.index += 1,
            }
        }
        Err("unterminated JSON string".to_string())
    }

    fn array(&mut self, depth: usize) -> Result<Json, String> {
        self.index += 1;
        self.whitespace();
        let mut values = Vec::new();
        if self.consume(b']') {
            return Ok(Json::Array(values));
        }
        loop {
            values.push(self.value(depth)?);
            self.whitespace();
            if self.consume(b']') {
                return Ok(Json::Array(values));
            }
            if !self.consume(b',') {
                return Err("expected comma or closing bracket in JSON array".to_string());
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, String> {
        self.index += 1;
        self.whitespace();
        let mut fields = Vec::new();
        if self.consume(b'}') {
            return Ok(Json::Object(fields));
        }
        loop {
            self.whitespace();
            if self.bytes.get(self.index) != Some(&b'"') {
                return Err("JSON object key is not a string".to_string());
            }
            let key = self.string()?;
            self.whitespace();
            if !self.consume(b':') {
                return Err("expected colon after JSON object key".to_string());
            }
            let value = self.value(depth)?;
            fields.push((key, value));
            self.whitespace();
            if self.consume(b'}') {
                return Ok(Json::Object(fields));
            }
            if !self.consume(b',') {
                return Err("expected comma or closing brace in JSON object".to_string());
            }
        }
    }

    fn whitespace(&mut self) {
        while matches!(
            self.bytes.get(self.index),
            Some(b' ' | b'\t' | b'\r' | b'\n')
        ) {
            self.index += 1;
        }
    }

    fn consume(&mut self, byte: u8) -> bool {
        if self.bytes.get(self.index) == Some(&byte) {
            self.index += 1;
            true
        } else {
            false
        }
    }
}

fn parse_object_fields(value: Json, allowed: &[&str]) -> Result<BTreeMap<String, Json>, String> {
    let Json::Object(fields) = value else {
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

fn take_string(fields: &mut BTreeMap<String, Json>, name: &str) -> Result<String, String> {
    match fields.remove(name) {
        Some(Json::String(value)) => Ok(value),
        Some(_) => Err(format!("wire field {name:?} is not a string")),
        None => Err(format!("missing required wire field {name:?}")),
    }
}

fn take_array(fields: &mut BTreeMap<String, Json>, name: &str) -> Result<Vec<Json>, String> {
    match fields.remove(name) {
        Some(Json::Array(value)) => Ok(value),
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
    if text.len() % 2 != 0 || !text.bytes().all(is_lower_hex) {
        return Err("byte strings require an even count of lowercase hex digits".to_string());
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digits = std::str::from_utf8(pair).expect("hex digits are ASCII");
            u8::from_str_radix(digits, 16).map_err(|_| "invalid byte hex".to_string())
        })
        .collect()
}

fn json_string(text: &str, output: &mut String) {
    output.push_str(&miniserde::json::to_string(text));
}

fn encode_source_document(document: &SourceDocument) -> String {
    let mut output = String::from("{\"schema\":");
    json_string(SOURCE_SCHEMA, &mut output);
    output.push_str(",\"id\":");
    json_string(&hex_u32(document.id.raw()), &mut output);
    output.push_str(",\"text\":");
    json_string(&document.text, &mut output);
    output.push('}');
    output
}

fn decode_source_document(input: &str) -> Result<SourceDocument, String> {
    let mut fields = parse_object_fields(
        JsonParser::parse(input)?,
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
    let mut function_ids = BTreeSet::new();
    for (index, binder) in document.binders.iter().enumerate() {
        let expected = IrId::from_index(index)?;
        if binder.id != expected || !binder_ids.insert(binder.id) {
            return Err("binder IDs must be unique and follow source table order".to_string());
        }
        match (binder.role, binder.owner.0) {
            (BinderRole::Function, None) => {
                if binder.text.is_empty() || !function_ids.insert(binder.id) {
                    return Err("function binder projection is invalid".to_string());
                }
            }
            (BinderRole::Parameter, Some(owner)) => {
                if binder.text.is_empty() || !function_ids.contains(&owner) {
                    return Err("parameter binder has no preceding function owner".to_string());
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
    let mut output = String::from("{\"schema\":");
    json_string(RAW_IR_SCHEMA, &mut output);
    output.push_str(",\"sourceId\":");
    json_string(&hex_u32(document.source_id.raw()), &mut output);
    output.push_str(",\"instructionCount\":");
    json_string(&hex_u32(document.instruction_count), &mut output);
    output.push_str(",\"driverProgramCount\":");
    json_string(&hex_u32(document.driver_program_count), &mut output);
    output.push_str(",\"scope\":");
    json_string(RAW_IR_SCOPE, &mut output);
    output.push_str(",\"driverRoot\":");
    json_string(&hex_u32(document.driver_root.raw()), &mut output);
    output.push_str(",\"binderProjection\":[");
    for (index, binder) in document.binders.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        output.push_str("{\"id\":");
        json_string(&hex_u32(binder.id.raw()), &mut output);
        output.push_str(",\"owner\":");
        json_string(&hex_u32(binder.owner.raw()), &mut output);
        output.push_str(",\"role\":");
        json_string(binder.role.name(), &mut output);
        output.push_str(",\"text\":");
        json_string(&binder.text, &mut output);
        output.push('}');
    }
    output.push_str("],\"instructions\":[");
    for (index, instruction) in document.instructions.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        output.push_str("{\"id\":");
        json_string(&hex_u32(instruction.id.raw()), &mut output);
        output.push_str(",\"tag\":");
        json_string(instruction.tag.name(), &mut output);
        output.push_str(",\"ordinal\":");
        json_string(&hex_u32(instruction.tag.ordinal()), &mut output);
        output.push_str(",\"words\":[");
        json_string(&hex_u32(instruction.words[0]), &mut output);
        output.push(',');
        json_string(&hex_u32(instruction.words[1]), &mut output);
        output.push_str("],\"value\":");
        json_string(&hex_u64(instruction.value_bits), &mut output);
        output.push('}');
    }
    output.push_str("],\"nativePathInputs\":[");
    for (index, path) in document.native_path_inputs.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        json_string(&hex_bytes(path), &mut output);
    }
    output.push_str("],\"integerOutputBits\":[");
    for (index, bits) in document.integer_output_bits.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        json_string(&hex_u64(*bits), &mut output);
    }
    output.push_str("],\"floatOutputBits\":[");
    for (index, bits) in document.float_output_bits.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        json_string(&hex_u64(*bits), &mut output);
    }
    output.push_str("],\"serializationOnlyFloatBits\":[");
    for (index, bits) in document.serialization_only_float_bits.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        json_string(&hex_u64(*bits), &mut output);
    }
    output.push_str("]}");
    Ok(output)
}

fn decode_optional_ir_id(text: &str) -> Result<OptionalIrId, String> {
    OptionalIrId::from_raw(parse_hex_u32(text)?)
}

fn decode_raw_ir_document(input: &str) -> Result<RawIrDocument, String> {
    let mut fields = parse_object_fields(
        JsonParser::parse(input)?,
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
            let Json::String(text) = value else {
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
            Json::String(text) => parse_hex_u64(&text),
            _ => Err("integer output bit pattern is not a hex string".to_string()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let float_output_bits = take_array(&mut fields, "floatOutputBits")?
        .into_iter()
        .map(|value| match value {
            Json::String(text) => parse_hex_u64(&text),
            _ => Err("Float output bit pattern is not a hex string".to_string()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let serialization_only_float_bits = take_array(&mut fields, "serializationOnlyFloatBits")?
        .into_iter()
        .map(|value| match value {
            Json::String(text) => parse_hex_u64(&text),
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

fn decode_binder(value: Json) -> Result<BinderProjection, String> {
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

fn decode_instruction(value: Json) -> Result<ScalarInstruction, String> {
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
        Json::String(text) => parse_hex_u32(&text),
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

fn write_json(value: &Json, output: &mut String) {
    match value {
        Json::String(text) => json_string(text, output),
        Json::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                write_json(value, output);
            }
            output.push(']');
        }
        Json::Object(fields) => {
            output.push('{');
            for (index, (key, value)) in fields.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                json_string(key, output);
                output.push(':');
                write_json(value, output);
            }
            output.push('}');
        }
    }
}

fn json_text(value: &Json) -> String {
    let mut output = String::new();
    write_json(value, &mut output);
    output
}

fn insert_field(value: &mut Json, name: &str, item: Json) {
    let Json::Object(fields) = value else {
        panic!("test mutation requires an object");
    };
    fields.push((name.to_string(), item));
}

fn set_field(value: &mut Json, name: &str, item: Json) {
    let Json::Object(fields) = value else {
        panic!("test mutation requires an object");
    };
    let field = fields
        .iter_mut()
        .find(|(field, _)| field == name)
        .expect("test mutation field exists");
    field.1 = item;
}

fn object_field<'a>(value: &'a mut Json, name: &str) -> &'a mut Json {
    let Json::Object(fields) = value else {
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
    mutate: impl FnOnce(&mut Json),
) -> String {
    let mut value = JsonParser::parse(encoded).expect("the encoder emits valid JSON");
    mutate(&mut value);
    json_text(&value)
}

fn fixture_binders(functions: &[ArenaFunctionDef]) -> Vec<String> {
    functions.iter().map(|function| name_text(function.name)).collect()
}

fn format_fixture_source(source: &str) -> String {
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".work/tmp");
    std::fs::create_dir_all(&scratch).expect("create ignored bridge scratch directory");
    let directory = tempfile::Builder::new()
        .prefix("formal-bridge-")
        .tempdir_in(scratch)
        .expect("create bridge formatter fixture directory");
    let path = directory.path().join("fixture.xsh");
    std::fs::write(&path, source).expect("write bridge formatter fixture");
    let file = path.to_string_lossy().into_owned();
    let first = xsht::format_files(std::slice::from_ref(&file), false);
    assert_eq!(
        first.status,
        0,
        "formatter failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let formatted = std::fs::read_to_string(&path).expect("read formatted bridge fixture");
    let second = xsht::format_files(std::slice::from_ref(&file), false);
    assert_eq!(
        second.status,
        0,
        "formatter failed on its own output: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("read second formatted bridge fixture"),
        formatted
    );
    formatted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::eval::{LoweredFunctionKey, LoweredFunctionKind};
    use crate::runtime::value::Value;

    const FIXTURE_SOURCE: &str = r#"pure signed_min(max: Int) -> Int {
  return -max - 1
}

pure signed_max() -> Int {
  return 9223372036854775807
}

pure duration_max() -> Duration {
  return 18446744073709551615ms
}

pure positive_zero() -> Float {
  return 0.0
}

pure negative_zero() -> Float {
  return -0.0
}

pure path_echo(input: Path) -> Path {
  return input
}
"#;

    fn set_nested_field(
        value: &mut Json,
        list_name: &str,
        row_index: usize,
        field: &str,
        item: Json,
    ) {
        let rows = object_field(value, list_name);
        let Json::Array(rows) = rows else {
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
                LoweredFunctionKind::Pure,
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
                LoweredFunctionKind::Pure,
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
        let decoded = decode_raw_ir_document(&raw_encoded).unwrap();
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

        let source_id = SourceId::new(0);
        let formatted_source = format_fixture_source(&source.text);
        let reparsed = crate::syntax::parser::Parser::parse_source_arena_only(
            source_id,
            &formatted_source,
        );
        assert!(reparsed.diagnostics.is_empty(), "{:?}", reparsed.diagnostics);
        assert_eq!(
            fixture_binders(&reparsed.arena.arena.function_defs),
            projected_function_binders
        );
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
                    LoweredFunctionKind::Pure,
                    &[Value::Path(path.clone())],
                    Span::new(program.store.source_id, 0, 0),
                )
                .expect("path_echo is present")
                .expect("path_echo returns its input");
            assert_eq!(returned, Value::Path(path));
        }

        let unknown_version = parsed_raw_mutation(&raw_encoded, |value| {
            set_field(value, "schema", Json::String("xsh-raw-ir-v2".to_string()));
        });
        assert!(decode_raw_ir_document(&unknown_version).is_err());

        let unknown_field = parsed_raw_mutation(&raw_encoded, |value| {
            insert_field(value, "unrecognized", Json::String("rejected".to_string()));
        });
        assert!(decode_raw_ir_document(&unknown_field).is_err());

        let duplicate_field = parsed_raw_mutation(&raw_encoded, |value| {
            insert_field(value, "schema", Json::String(RAW_IR_SCHEMA.to_string()));
        });
        assert!(decode_raw_ir_document(&duplicate_field).is_err());

        let unknown_tag = parsed_raw_mutation(&raw_encoded, |value| {
            set_nested_field(
                value,
                "instructions",
                0,
                "tag",
                Json::String("ExprUnknown".to_string()),
            );
        });
        assert!(decode_raw_ir_document(&unknown_tag).is_err());

        let bad_ordinal = parsed_raw_mutation(&raw_encoded, |value| {
            set_nested_field(
                value,
                "instructions",
                0,
                "ordinal",
                Json::String("000000ff".to_string()),
            );
        });
        assert!(decode_raw_ir_document(&bad_ordinal).is_err());

        let short_payload = parsed_raw_mutation(&raw_encoded, |value| {
            let words = object_field(value, "instructions");
            let Json::Array(rows) = words else {
                panic!("test mutation field is an array");
            };
            set_field(
                &mut rows[0],
                "words",
                Json::Array(vec![Json::String("00000000".to_string())]),
            );
        });
        assert!(decode_raw_ir_document(&short_payload).is_err());

        let duplicate_id = parsed_raw_mutation(&raw_encoded, |value| {
            let instructions = object_field(value, "instructions");
            let Json::Array(rows) = instructions else {
                panic!("test mutation field is an array");
            };
            let first = match object_field(&mut rows[0], "id") {
                Json::String(id) => id.clone(),
                _ => panic!("encoded ID is a string"),
            };
            set_field(&mut rows[1], "id", Json::String(first));
        });
        assert!(decode_raw_ir_document(&duplicate_id).is_err());

        let ghost_instruction = parsed_raw_mutation(&raw_encoded, |value| {
            set_nested_field(
                value,
                "instructions",
                0,
                "id",
                Json::String(hex_u32(raw.instruction_count + 1)),
            );
        });
        assert!(decode_raw_ir_document(&ghost_instruction).is_err());

        let dangling_driver = parsed_raw_mutation(&raw_encoded, |value| {
            set_field(
                value,
                "driverRoot",
                Json::String(hex_u32(raw.driver_program_count + 1)),
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
                Json::String(hex_u32(dangling_owner_id.raw())),
            );
        });
        assert!(decode_raw_ir_document(&dangling_owner).is_err());

        let wrong_word_width = parsed_raw_mutation(&raw_encoded, |value| {
            set_nested_field(value, "instructions", 0, "value", Json::String("1".to_string()));
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
            insert_field(value, "schema", Json::String(SOURCE_SCHEMA.to_string()));
        });
        assert!(decode_source_document(&duplicate).is_err());
        let unknown = parsed_raw_mutation(&encoded, |value| {
            insert_field(value, "extra", Json::String("rejected".to_string()));
        });
        assert!(decode_source_document(&unknown).is_err());
        let unknown_version = parsed_raw_mutation(&encoded, |value| {
            set_field(value, "schema", Json::String("xsh-source-v2".to_string()));
        });
        assert!(decode_source_document(&unknown_version).is_err());
        assert!(encoded.contains("é"), "UTF-8 source text stays readable and exact");

        let escaped_unicode =
            r#"{"schema":"xsh-source-v1","id":"00000001","text":"caf\u00e9 \ud83d\ude00"}"#;
        assert_eq!(
            decode_source_document(escaped_unicode).unwrap().text,
            "café 😀"
        );
    }

    #[test]
    fn json_transport_rejects_non_json_whitespace_and_bounds_input() {
        for whitespace in ['\u{000b}', '\u{000c}'] {
            let invalid = format!("[{whitespace}\"value\"]");
            assert!(JsonParser::parse(&invalid).is_err());
        }

        let valid_whitespace = " [ \t\r\n\"value\" \t\r\n ] ";
        assert!(JsonParser::parse(valid_whitespace).is_ok());

        let at_byte_limit = format!(
            "\"{}\"",
            "x".repeat(MAX_TRANSPORT_DOCUMENT_BYTES - 2)
        );
        assert_eq!(at_byte_limit.len(), MAX_TRANSPORT_DOCUMENT_BYTES);
        assert!(JsonParser::parse(&at_byte_limit).is_ok());
        let over_byte_limit = format!("\"{}\"", "x".repeat(MAX_TRANSPORT_DOCUMENT_BYTES - 1));
        assert!(JsonParser::parse(&over_byte_limit).is_err());
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

        assert!(JsonParser::parse(&nested_arrays(MAX_JSON_CONTAINER_DEPTH)).is_ok());
        assert!(JsonParser::parse(&nested_arrays(MAX_JSON_CONTAINER_DEPTH + 1)).is_err());
        assert!(JsonParser::parse(&nested_objects(MAX_JSON_CONTAINER_DEPTH)).is_ok());
        assert!(JsonParser::parse(&nested_objects(MAX_JSON_CONTAINER_DEPTH + 1)).is_err());
    }
}
