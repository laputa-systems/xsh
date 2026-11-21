use super::*;
use crate::runtime::eval::indexed::full::wire_shape::{PayloadShape, WordRole};

pub(super) const DOCUMENT_SCHEMA: &str = "xsh-raw-ir-v1";
pub(super) const DOCUMENT_SCOPE: &str = "indexed-store-structure-v1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Field {
    Text(String),
    Texts(Vec<String>),
    Word(u32),
    Words(Vec<u32>),
    Rows(usize, Vec<Vec<u32>>),
    Tags(Vec<(String, u32)>),
    Bytes(Vec<u8>),
}

pub(super) type Document = Vec<(&'static str, Field)>;

fn range(range: IrRange) -> [u32; 2] {
    [range.start, range.len]
}

fn rows<T, const N: usize>(values: &[T], row: impl Fn(&T) -> [u32; N]) -> Field {
    Field::Rows(N, values.iter().map(|value| row(value).to_vec()).collect())
}

fn tags<T: Copy>(values: &[T], name: impl Fn(T) -> String, ordinal: impl Fn(T) -> u32) -> Field {
    Field::Tags(values.iter().map(|value| (name(*value), ordinal(*value))).collect())
}

fn count(len: usize) -> Result<Field, String> {
    u32::try_from(len).map(Field::Word).map_err(|_| "a pool is longer than u32".to_string())
}

fn byte_words(bytes: [u8; 3]) -> [u32; 3] {
    bytes.map(u32::from)
}

pub(super) fn project(store: &RawFullRows, source_count: usize, symbol_base: u32, symbols: Vec<String>) -> Result<Document, String> {
    let source = u32::try_from(store.source_id.raw()).map_err(|_| "source id exceeds u32".to_string())?;
    let flag_words = |marks: &[bool]| Field::Words(marks.iter().map(|marked| u32::from(*marked)).collect());
    Ok(vec![
        ("schema", Field::Text(DOCUMENT_SCHEMA.into())),
        ("scope", Field::Text(DOCUMENT_SCOPE.into())),
        ("symbolBase", Field::Word(symbol_base)),
        ("symbols", Field::Texts(symbols)),
        ("sourceId", Field::Word(source)),
        ("sourceCount", count(source_count)?),
        ("tags", tags(&store.tags, |tag| tag.name().into(), |tag| tag as u32)),
        ("data", rows(&store.data, |data| [data.lhs, data.rhs])),
        ("resourceTransfers", flag_words(&store.resource_transfers)),
        ("checkedCallTargetRequired", flag_words(&store.checked_call_target_required)),
        ("extra", Field::Words(store.extra.clone())),
        ("patterns", tags(&store.patterns, |tag| format!("{tag:?}"), |tag| tag as u32)),
        ("patternData", rows(&store.pattern_data, |data| [data.lhs, data.rhs])),
        ("stages", tags(&store.stages, |tag| format!("{tag:?}"), |tag| tag as u32)),
        ("stageData", rows(&store.stage_data, |data| [data.lhs, data.rhs])),
        ("values", tags(&store.values, |tag| format!("{tag:?}"), |tag| tag as u32)),
        ("valueData", rows(&store.value_data, |data| [data.lhs, data.rhs])),
        ("blocks", rows(&store.blocks, |block| {
            let [start, len] = range(block.instructions);
            let [r0, r1, r2] = byte_words(block.reserved);
            [start, len, block.result, block.owner, u32::from(block.flags), r0, r1, r2]
        })),
        ("driverStepTags", tags(&store.driver_steps, |step| format!("{:?}", step.tag), |step| step.tag as u32)),
        ("driverSteps", rows(&store.driver_steps, |step| {
            let [r0, r1, r2] = byte_words(step.reserved);
            [step.data.lhs, step.data.rhs, step.slots.start, step.slots.len, step.writers.start, step.writers.len,
                step.instruction_start, step.slot_count, step.location, step.effects, step.registered_module_type, r0, r1, r2]
        })),
        ("driverSlots", rows(&store.driver_slots, |slot| {
            let [r0, r1, r2] = byte_words(slot.reserved);
            [slot.name, slot.type_id.raw(), slot.slot, slot.binding_identity, u32::from(slot.flags), r0, r1, r2]
        })),
        ("driverWriters", rows(&store.driver_writers, |writer| [writer.binding_identity, writer.slot])),
        ("driverSync", rows(&store.driver_sync, |sync| {
            let [r0, r1, r2] = byte_words(sync.reserved);
            [sync.name, sync.type_id.raw(), u32::from(sync.flags), r0, r1, r2]
        })),
        ("driverRegions", rows(&store.driver_regions, |region| {
            let [steps, steps_len] = range(region.steps);
            let [sync, sync_len] = range(region.sync);
            [steps, steps_len, sync, sync_len, region.effects]
        })),
        ("driverPrograms", rows(&store.driver_programs, |program| {
            let [steps, steps_len] = range(program.steps);
            let [regions, regions_len] = range(program.regions);
            [steps, steps_len, regions, regions_len, program.effects, program.owner]
        })),
        ("bindingIdentities", rows(&store.binding_identities, |identity| {
            let [r0, r1, r2] = byte_words(identity.reserved);
            [identity.name, identity.location, identity.owner, u32::from(identity.flags), r0, r1, r2]
        })),
        ("driverRoot", Field::Word(store.driver_root)),
        ("functions", rows(&store.functions, |function| {
            let (kind, contract) = match function.contract {
                FullCallableContract::Ground(id) => (0, id.raw()),
                FullCallableContract::Scheme(id) => (1, id.index() as u32),
            };
            let [params, params_len] = range(function.params);
            let [captures, captures_len] = range(function.captures);
            let [facts, facts_len] = range(function.slot_type_facts);
            [function.name, kind, contract, params, params_len, captures, captures_len, function.body,
                function.slot_count, facts, facts_len, function.checked_return_type]
        })),
        ("functionInstructionStarts", Field::Words(store.function_instruction_starts.clone())),
        ("functionMetadata", rows(&store.function_metadata, |metadata| {
            [metadata.owner, u32::from(metadata.flags), metadata.callable_kind as u32,
                u32::from(metadata.reserved[0]), u32::from(metadata.reserved[1])]
        })),
        ("params", rows(&store.params, |param| {
            let [r0, r1, r2] = byte_words(param.reserved);
            [param.name, u32::from(param.flags), r0, r1, r2]
        })),
        ("paramCold", rows(&store.param_cold, |cold| [cold.param, cold.default, cold.validation])),
        ("captures", rows(&store.captures, |capture| {
            [capture.name, capture.type_id.raw(), capture.slot_and_flags, capture.binding_identity, capture.owner, capture.use_location]
        })),
        ("validations", rows(&store.validations, |validation| [validation.type_id.raw(), validation.name])),
        ("slotTypeFacts", Field::Words(store.slot_type_facts.clone())),
        ("strings", rows(&store.strings, |string| range(*string))),
        ("stringBytes", Field::Bytes(store.string_bytes.clone())),
        ("bytes", rows(&store.bytes, |bytes| range(*bytes))),
        ("byteData", Field::Bytes(store.byte_data.clone())),
        ("locations", rows(&store.locations, |location| [location.start, location.len])),
        ("locationSources", Field::Words(store.location_sources.iter()
            .map(|source| u32::try_from(source.raw()).map_err(|_| "location source exceeds u32".to_string()))
            .collect::<Result<_, _>>()?)),
        ("runtimeOps", Field::Words(store.runtime_ops.iter().map(|op| u32::from(*op as u16)).collect())),
        ("assignOps", Field::Words(store.assign_ops.iter().map(|op| *op as u32).collect())),
        ("binaryOps", Field::Words(store.binary_ops.iter().map(|op| *op as u32).collect())),
        ("runKinds", Field::Words(store.run_kinds.iter().map(|kind| *kind as u32).collect())),
        ("redirectionKinds", Field::Words(store.redirection_kinds.iter().map(|kind| *kind as u32).collect())),
        ("poolCounts", Field::Words([
            store.semantic.tables.type_tags.len(), store.semantic.tables.signature_data.len(), store.schemes.len(),
            store.prepared_regexes.len(), store.prepared_constants.len(), store.wire_enums.len(), store.prepared_schemas.len(),
            store.prepared_module_call_plans.len(), store.value_materializations.len(), store.scheme_schema_expectations.len(),
        ].into_iter().map(|len| u32::try_from(len).map_err(|_| "a pool is longer than u32".to_string())).collect::<Result<_, _>>()?)),
    ])
}

fn field_wire(field: &Field) -> WireValue {
    match field {
        Field::Text(text) => wire_string(text.as_str()),
        Field::Texts(texts) => WireValue::Array(texts.iter().map(|text| wire_string(text.as_str())).collect()),
        Field::Word(word) => wire_word(*word),
        Field::Words(words) => WireValue::Array(words.iter().map(|word| wire_word(*word)).collect()),
        Field::Rows(_, rows) => WireValue::Array(rows.iter().map(|row| WireValue::Array(row.iter().map(|word| wire_word(*word)).collect())).collect()),
        Field::Tags(tags) => WireValue::Array(tags.iter().map(|(name, ordinal)| wire_object([
            ("name", wire_string(name.as_str())),
            ("ordinal", wire_word(*ordinal)),
        ])).collect()),
        Field::Bytes(bytes) => wire_string(super::super::hex_bytes(bytes)),
    }
}

pub(super) fn encode(document: &Document) -> Result<String, String> {
    let wire = WireValue::Object(document.iter().map(|(name, field)| ((*name).to_owned(), field_wire(field))).collect());
    deser_json::to_string(&wire).map_err(|error| error.to_string())
}

fn hex_word(value: WireValue) -> Result<u32, String> {
    match value {
        WireValue::String(text) => parse_hex_u32(&text),
        _ => Err("a document word is not a string".into()),
    }
}

fn decode_field(template: &Field, value: WireValue) -> Result<Field, String> {
    Ok(match template {
        Field::Text(expected) => {
            let WireValue::String(text) = value else { return Err("a document text field is not a string".into()) };
            if text != *expected {
                return Err(format!("document field holds {text:?} where {expected:?} is required"));
            }
            Field::Text(text)
        }
        Field::Texts(_) => {
            let WireValue::Array(texts) = value else { return Err("a document text table is not an array".into()) };
            Field::Texts(texts.into_iter().map(|text| match text {
                WireValue::String(text) => Ok(text),
                _ => Err("a document text entry is not a string".to_string()),
            }).collect::<Result<_, _>>()?)
        }
        Field::Word(_) => Field::Word(hex_word(value)?),
        Field::Words(_) => {
            let WireValue::Array(words) = value else { return Err("a document word table is not an array".into()) };
            Field::Words(words.into_iter().map(hex_word).collect::<Result<_, _>>()?)
        }
        Field::Rows(arity, _) => {
            let WireValue::Array(rows) = value else { return Err("a document row table is not an array".into()) };
            Field::Rows(*arity, rows.into_iter().map(|row| {
                let WireValue::Array(words) = row else { return Err("a document row is not an array".to_string()) };
                if words.len() != *arity {
                    return Err(format!("a document row has {} words where {arity} are required", words.len()));
                }
                words.into_iter().map(hex_word).collect()
            }).collect::<Result<_, _>>()?)
        }
        Field::Tags(_) => {
            let WireValue::Array(tags) = value else { return Err("a document tag table is not an array".into()) };
            Field::Tags(tags.into_iter().map(|tag| {
                let mut fields = parse_object_fields(tag, &["name", "ordinal"])?;
                Ok((take_string(&mut fields, "name")?, parse_hex_u32(&take_string(&mut fields, "ordinal")?)?))
            }).collect::<Result<_, String>>()?)
        }
        Field::Bytes(_) => {
            let WireValue::String(text) = value else { return Err("a document byte field is not a string".into()) };
            Field::Bytes(super::super::parse_hex_bytes(&text)?)
        }
    })
}

pub(super) fn decode(input: &str, template: &Document) -> Result<Document, String> {
    let names = template.iter().map(|(name, _)| *name).collect::<Vec<_>>();
    let mut fields = parse_object_fields(parse_wire_value(input)?, &names)?;
    template.iter().map(|(name, field)| {
        let value = fields.remove(*name).ok_or_else(|| format!("missing required wire field {name:?}"))?;
        Ok((*name, decode_field(field, value)?))
    }).collect()
}

fn lean_role(role: WordRole) -> &'static str {
    match role {
        WordRole::Raw => "raw", WordRole::Symbol => "symbol", WordRole::Slot => "slot", WordRole::String => "stringId",
        WordRole::Bytes => "bytesId", WordRole::Location => "location", WordRole::Type => "typeId",
        WordRole::LoweredType => "loweredType", WordRole::Expr => "expr", WordRole::Stmt => "stmt", WordRole::Int => "int",
        WordRole::Bool => "bool", WordRole::Pattern => "pattern", WordRole::Stage => "stage", WordRole::Value => "value",
        WordRole::Function => "function", WordRole::Regex => "regex", WordRole::Constant => "constant",
        WordRole::ModulePlan => "modulePlan", WordRole::WireEnum => "wireEnum", WordRole::Schema => "schema",
        WordRole::RuntimeOp => "runtimeOp", WordRole::AssignOp => "assignOp", WordRole::BinaryOp => "binaryOp",
        WordRole::RunKind => "runKind", WordRole::RedirectionKind => "redirectionKind",
        WordRole::BindingIdentity => "bindingIdentity", WordRole::ChildProgram => "childProgram",
        WordRole::DeferKind => "deferKind", WordRole::EndDistance => "endDistance",
        WordRole::Materialization => "materialization", WordRole::SchemaExpectation => "schemaExpectation",
        WordRole::TextHole => "textHole",
    }
}

#[derive(Default)]
pub(super) struct LeanShapes {
    pub(super) recursive: BTreeMap<&'static str, String>,
    expanding: Vec<&'static str>,
}

impl LeanShapes {
    pub(super) fn render(&mut self, shape: &PayloadShape) -> String {
        match shape {
            PayloadShape::Word(role) => format!(".word .{}", lean_role(*role)),
            PayloadShape::Fields(items) => format!(".seq [{}]", items.iter().map(|item| self.render(item)).collect::<Vec<_>>().join(", ")),
            PayloadShape::Choice(variants) => {
                let (tags, shapes) = self.variants(variants);
                format!(".choice {tags} {shapes}")
            }
            PayloadShape::Repeat(item) => format!(".many ({})", self.render(item)),
            PayloadShape::Block(flags, item) => format!(".block {} ({})", if *flags == BLOCK_STATEMENTS { "true" } else { "false" }, self.render(item)),
            PayloadShape::TextPattern => ".textPattern".into(),
            PayloadShape::Named(name, definition) => match definition() {
                PayloadShape::Choice(variants) => {
                    if !self.recursive.contains_key(name) && !self.expanding.contains(name) {
                        self.expanding.push(name);
                        let (tags, shapes) = self.variants(&variants);
                        self.expanding.pop();
                        self.recursive.insert(name, format!("{tags}, {shapes}"));
                    }
                    format!(".recur \"{name}\"")
                }
                fields => {
                    assert!(!self.expanding.contains(name), "record shape {name} contains itself without a tag");
                    self.expanding.push(name);
                    let rendered = self.render(&fields);
                    self.expanding.pop();
                    rendered
                }
            },
        }
    }

    fn variants(&mut self, variants: &[(u32, PayloadShape)]) -> (String, String) {
        let tags = variants.iter().map(|(tag, _)| tag.to_string()).collect::<Vec<_>>().join(", ");
        let shapes = variants.iter().map(|(_, shape)| self.render(shape)).collect::<Vec<_>>().join(", ");
        (format!("[{tags}]"), format!("[{shapes}]"))
    }
}

fn instruction_kind(tag: FullTag) -> &'static str {
    match tag.kind() {
        schema::InstructionKind::Int => "int",
        schema::InstructionKind::Bool => "bool",
        schema::InstructionKind::Expr => "expr",
        schema::InstructionKind::Stmt => "stmt",
    }
}

fn lean_table<T: Copy>(shapes: &mut LeanShapes, definition: &str, tags: &[T], row: impl Fn(T) -> (String, u32, &'static str, Vec<PayloadShape>)) -> String {
    let rows = tags.iter().map(|tag| {
        let (name, ordinal, kind, fields) = row(*tag);
        let fields = fields.iter().map(|field| shapes.render(field)).collect::<Vec<_>>().join(", ");
        format!("  ⟨\"{name}\", {ordinal}, .{kind}, [{fields}]⟩")
    }).collect::<Vec<_>>();
    format!("def {definition} : List TagSchema := [\n{}]\n\n", rows.join(",\n"))
}

pub(super) fn lean_schema_module() -> String {
    use super::program_image::UnitVariants;
    let mut shapes = LeanShapes::default();
    let mut output = String::from("import Xsh.IR.Raw.Shape\n\nnamespace Xsh.IR.Raw\n\n");
    output.push_str(&lean_table(&mut shapes, "instructionSchema", schema::INSTRUCTION_TAGS,
        |tag| (tag.name().to_owned(), tag as u32, instruction_kind(tag), tag.payload_shape())));
    output.push_str(&lean_table(&mut shapes, "patternSchema", schema::PATTERN_TAGS,
        |tag| (format!("{tag:?}"), tag as u32, "pattern", tag.payload_shape())));
    output.push_str(&lean_table(&mut shapes, "stageSchema", FullStageTag::ALL,
        |tag| (format!("{tag:?}"), tag as u32, "stage", tag.payload_shape())));
    output.push_str(&lean_table(&mut shapes, "valueSchema", FullValueTag::ALL,
        |tag| (format!("{tag:?}"), tag as u32, "value", tag.payload_shape())));
    output.push_str(&lean_table(&mut shapes, "driverSchema", FullDriverTag::ALL,
        |tag| (format!("{tag:?}"), tag as u32, "driver", tag.payload_shape())));
    let recursive = shapes.recursive.iter().map(|(name, variants)| format!("  ⟨\"{name}\", {variants}⟩")).collect::<Vec<_>>();
    output.push_str(&format!("def recursiveShapes : List RecursiveShape := [\n{}]\n\n", recursive.join(",\n")));
    let counts = [
        ("assignOpCount", AssignOp::ALL.len()),
        ("binaryOpCount", BinaryOp::ALL.len()),
        ("runKindCount", RunKind::ALL.len()),
        ("redirectionKindCount", RedirectionKind::ALL.len()),
        ("deferKindCount", (0..).take_while(|raw| crate::runtime::eval::DeferKind::from_encoded(*raw).is_some()).count()),
        ("textHoleKindCount", crate::sema::check::TextHoleKind::ALL.len()),
        ("callableKindCount", super::shapes::codec_width::<CallableKind>(&[]) as usize),
    ];
    for (name, count) in counts {
        output.push_str(&format!("def {name} : Nat := {count}\n\n"));
    }
    let transfers = schema::INSTRUCTION_TAGS.iter().filter(|tag| is_resource_transfer_instruction(**tag))
        .map(|tag| (*tag as u32).to_string()).collect::<Vec<_>>();
    output.push_str(&format!("def resourceTransferOrdinals : List Nat := [{}]\n\n", transfers.join(", ")));
    output.push_str("end Xsh.IR.Raw\n");
    output
}
