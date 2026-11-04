use super::super::*;
use super::{build_fixture, check_fixture, lean_definition, lean_document_from, parse_hex_u32, parse_hex_u64, parse_object_fields, parse_wire_document,
    take_array, take_string, wire_object, wire_string, WireValue};
use crate::runtime::eval::indexed::semantic::{RawSemanticPools, SemanticPools};

macro_rules! raw_full_rows {
    ($($field:ident: $ty:ty),* $(,)?) => {
        #[derive(Clone, Debug)]
        struct RawFullRows {
            $($field: $ty,)*
            semantic: RawSemanticPools,
        }

        impl RawFullRows {
            fn capture(program: &FullProgram) -> Self {
                Self {
                    $($field: program.store.$field.clone(),)*
                    semantic: program.store.semantic.export_raw(&program.symbols),
                }
            }

            fn into_store(self) -> (FullStore, crate::symbol::SymbolOwner) {
                let Self { $($field,)* semantic } = self;
                let (semantic, symbols) = SemanticPools::from_raw(semantic);
                let store = FullStore {
                    $($field,)*
                    semantic,
                    in_place: std::sync::OnceLock::new(),
                };
                (store, symbols)
            }
        }
    };
}

raw_full_rows! {
    source_id: SourceId,
    tags: Vec<FullTag>,
    resource_transfers: Vec<bool>,
    checked_call_target_required: Vec<bool>,
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
    driver_writers: Vec<FullDriverWriter>,
    driver_sync: Vec<FullDriverSync>,
    driver_regions: Vec<FullDriverRegion>,
    driver_programs: Vec<FullDriverProgram>,
    binding_identities: Vec<FullBindingIdentity>,
    driver_root: u32,
    functions: Vec<FullFunction>,
    slot_type_facts: Vec<u32>,
    checked_expr_types: Vec<FullCheckedExprType>,
    checked_call_targets: Vec<FullCheckedCallTarget>,
    checked_pattern_variants: BTreeMap<CheckedPatternVariantKey, FullPatternVariantSchema>,
    declared_pattern_variants: BTreeMap<CheckedPatternVariantKey, FullPatternVariantSchema>,
    function_instruction_starts: Vec<u32>,
    function_metadata: Vec<FullFunctionMetadata>,
    function_return_schemas: Vec<Option<schemes::FullSchemeSchemaExpectationId>>,
    schemes: Vec<schemes::FullScheme>,
    scheme_types: Vec<FullSchemeTypeNode>,
    scheme_callable_monotypes: Vec<schemes::FullSchemeCallableMonotype>,
    scheme_callable_values: Vec<scheme_values::FullSchemeCallableValue>,
    scheme_callable_targets: Vec<scheme_values::FullSchemeCallableTarget>,
    scheme_callable_binders: Vec<u32>,
    scheme_callable_calls: Vec<scheme_values::FullSchemeCallableCall>,
    value_materializations: Vec<value_materialization::FullValueMaterialization>,
    materialization_nodes: Vec<value_materialization::FullMaterializationNode>,
    materialization_edges: Vec<value_materialization::FullMaterializationEdge>,
    materialization_leaves: Vec<value_materialization::FullMaterializationLeaf>,
    scheme_callable_parameters: Vec<schemes::FullSchemeCallableParameter>,
    scheme_fields: Vec<schemes::FullSchemeField>,
    scheme_children: Vec<SchemeTypeId>,
    scheme_type_parameters: Vec<schemes::FullSchemeTypeParameter>,
    scheme_row_parameters: Vec<schemes::FullSchemeRowParameter>,
    scheme_row_lacks: Vec<u32>,
    scheme_params: Vec<schemes::FullSchemeParam>,
    scheme_validations: Vec<schemes::FullSchemeValidation>,
    scheme_effects: Vec<Effect>,
    scheme_effect_parameters: Vec<schemes::FullEffectParameter>,
    scheme_effect_terms: Vec<schemes::FullEffectTerm>,
    scheme_effect_predicates: Vec<schemes::FullEffectPredicate>,
    scheme_applications: Vec<schemes::FullSchemeApplication>,
    scheme_operations: Vec<schemes::FullSealedOperation>,
    scheme_methods: Vec<schemes::FullSchemeMethod>,
    call_guard_bindings: Vec<guard_schemes::FullGuardBinding>,
    scheme_guard_reads: Vec<guard_schemes::FullGuardRead>,
    scheme_guards: Vec<guard_schemes::FullGuardScope>,
    scheme_guard_calls: Vec<guard_schemes::FullGuardCall>,
    scheme_method_slots: Vec<u32>,
    scheme_method_arguments: Vec<SchemeTypeId>,
    call_operation_bindings: Vec<schemes::FullOperationBinding>,
    call_method_bindings: Vec<schemes::FullMethodBinding>,
    call_wrapper_bindings: Vec<schemes::FullWrapperBinding>,
    wrapper_contributions: Vec<schemes::FullWrapperContribution>,
    wrapper_captures: Vec<schemes::FullWrapperCapture>,
    wrapper_completions: Vec<schemes::FullWrapperCompletion>,
    wrapper_handlers: Vec<schemes::FullWrapperHandler>,
    wrapper_fields: Vec<schemes::FullWrapperField>,
    wrapper_key_validations: Vec<schemes::FullWrapperKeyValidation>,
    scheme_wrappers: Vec<schemes::FullWrapper>,
    scheme_application_arguments: Vec<schemes::FullApplicationArgument>,
    scheme_proc_returns: Vec<schemes::FullProcReturnEnvelope>,
    call_application_bindings: Vec<schemes::FullApplicationBinding>,
    call_application_slots: Vec<u32>,
    scheme_application_calls: Vec<schemes::FullSchemeApplicationCall>,
    scheme_application_returns: Vec<schemes::FullSchemeApplicationReturn>,
    call_effect_arguments: Vec<schemes::FullEffectArgument>,
    scheme_schema_expectations: Vec<schemes::FullSchemeSchemaExpectation>,
    scheme_schema_instances: Vec<schemes::FullSchemeSchemaInstance>,
    scheme_schema_definitions: Vec<schemes::FullSchemeSchemaDefinition>,
    scheme_schema_arguments: Vec<SchemeTypeId>,
    scheme_schema_children: Vec<schemes::FullSchemeSchemaChild>,
    call_instantiations: Vec<schemes::FullCallInstantiation>,
    call_type_arguments: Vec<schemes::FullTypeArgument>,
    call_row_arguments: Vec<schemes::FullRowArgument>,
    call_row_fields: Vec<schemes::FullRowEvidenceField>,
    scheme_body_facts: Vec<schemes::FullSchemeBodyFact>,
    params: Vec<FullParam>,
    param_cold: Vec<FullParamCold>,
    captures: Vec<FullCapture>,
    validations: Vec<FullValidation>,
    strings: Vec<IrRange>,
    string_bytes: Vec<u8>,
    bytes: Vec<IrRange>,
    byte_data: Vec<u8>,
    prepared_regexes: Vec<RegexValue>,
    prepared_constants: Vec<PreparedConstantValue<TypeId>>,
    wire_enums: Vec<Arc<crate::sema::wire_enums::WireEnumMapping>>,
    prepared_schemas: Vec<Arc<crate::runtime::eval::require::PreparedSchema>>,
    prepared_module_call_plans: Vec<Arc<crate::runtime::eval::ModuleCallPlan>>,
    registered_calls: BTreeMap<u32, Arc<crate::sema::schemes::RegisteredCallPlan>>,
    locations: Vec<IrLocation>,
    location_sources: Vec<SourceId>,
    runtime_ops: Vec<RuntimeOp>,
    assign_ops: Vec<AssignOp>,
    binary_ops: Vec<BinaryOp>,
    run_kinds: Vec<RunKind>,
    redirection_kinds: Vec<RedirectionKind>,
}

#[test]
fn a_well_typed_raw_store_can_change_the_source_result() {
    use crate::runtime::eval::{Evaluator, Flow};
    use crate::runtime::value::{RuntimeError, Value};

    let source = "pure choose(value) { var current = value; current = 1; current }\nassert choose(7) == 1\n";
    let (_, _, program) = build_fixture(source).unwrap();
    let run = |program: FullProgram| -> Result<(), RuntimeError> {
        let span = Span::new(program.store.source_id, 0, 0);
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), (*program.sources).clone());
        evaluator.indexed_program = Some(Arc::new(program.clone()));
        for step in 0..program.driver_step_count().unwrap() {
            match evaluator.eval_indexed_driver_step(step, span).unwrap()? {
                None | Some(Flow::Continue(_)) => {}
                Some(Flow::Propagate(propagation)) => match propagation.error {
                    Value::Error(error) => return Err(*error),
                    other => panic!("step {step} propagated a non-error value: {other:?}"),
                },
                Some(flow) => panic!("step {step} ended with top-level control flow: {flow:?}"),
            }
        }
        Ok(())
    };
    run(program.clone()).unwrap();

    let mut changed = RawFullProgram::capture(&program);
    let assignment = changed.rows.tags.iter().position(|tag| *tag == FullTag::StmtAssignPath).unwrap();
    let assignment_words = changed.rows.data[assignment].lhs as usize;
    let wrapper = changed.rows.extra[assignment_words + 3] as usize;
    assert_eq!(changed.rows.tags[wrapper], FullTag::ExprValueBlock);
    let wrapper_words = changed.rows.data[wrapper].lhs as usize;
    let block = changed.rows.extra[wrapper_words] as usize;
    let statements = changed.rows.blocks[block - 1].instructions.start as usize;
    assert_eq!(changed.rows.extra[statements], 2);
    let temporary = changed.rows.extra[statements + 1] as usize;
    assert_eq!(changed.rows.tags[temporary], FullTag::StmtLet);
    let tail = changed.rows.extra[statements + 2] as usize;
    assert_eq!(changed.rows.tags[tail], FullTag::StmtValue);
    let temporary_words = changed.rows.data[temporary].lhs as usize;
    let temporary_slot = changed.rows.extra[temporary_words];
    let tail_read = changed.rows.extra[changed.rows.data[tail].lhs as usize] as usize;
    assert_eq!(changed.rows.tags[tail_read], FullTag::ExprParam);
    assert_eq!(changed.rows.extra[changed.rows.data[tail_read].lhs as usize], temporary_slot);
    let replacement = changed.rows.extra[temporary_words + 1] as usize;
    assert_eq!(changed.rows.tags[replacement], FullTag::ExprInt);
    let literal_words = changed.rows.data[replacement].lhs as usize;
    assert_eq!(&changed.rows.extra[literal_words..literal_words + 2], &[1, 0]);
    changed.rows.extra[literal_words] = 2;
    let changed = changed.into_program();
    FullVerifier::verify(&changed).unwrap();
    let error = run(changed).unwrap_err();
    assert_eq!(error.family_name, Type::assertion_error_family());
    assert_eq!(error.variant, "Failed");
}

#[derive(Clone, Debug)]
struct RawFullProgram {
    rows: RawFullRows,
    sources: Arc<SourceMap>,
    binding_namespace: u64,
    function_definition_spans: Vec<Span>,
}

impl RawFullProgram {
    fn capture(program: &FullProgram) -> Self {
        Self {
            rows: RawFullRows::capture(program),
            sources: Arc::clone(&program.sources),
            binding_namespace: program.binding_namespace,
            function_definition_spans: program.function_definition_spans.clone(),
        }
    }

    fn into_program(self) -> FullProgram {
        let (store, symbols) = self.rows.into_store();
        let headers = (0..store.functions.len()).map(|_| std::sync::OnceLock::new()).collect();
        FullProgram {
            store,
            sources: self.sources,
            symbols,
            binding_namespace: self.binding_namespace,
            function_definition_spans: self.function_definition_spans,
            headers,
            identities: std::sync::OnceLock::new(),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum CandidateDecodeError {
    Malformed(String),
    UnsupportedTag(String),
    UnsupportedFeature(String),
}

impl From<String> for CandidateDecodeError {
    fn from(message: String) -> Self {
        Self::Malformed(message)
    }
}

const CONNECTED_TAGS: &[FullTag] = &[
    FullTag::IntInt, FullTag::IntSlot, FullTag::IntBinary,
    FullTag::BoolBool, FullTag::BoolSlot, FullTag::BoolNot, FullTag::BoolAnd,
    FullTag::BoolOr, FullTag::BoolIntCompare, FullTag::ExprUnit, FullTag::ExprInt, FullTag::ExprBool,
    FullTag::ExprStr, FullTag::ExprFunctionRef, FullTag::ExprParam, FullTag::ExprBinary, FullTag::ExprRecord,
    FullTag::ExprField, FullTag::ExprRequire, FullTag::ExprCheckedValue, FullTag::ExprValueBlock,
    FullTag::ExprModuleCall, FullTag::ExprIf, FullTag::ExprOk,
    FullTag::ExprErr, FullTag::ExprTry, FullTag::ExprCall, FullTag::ExprDirectPureCall,
    FullTag::StmtLet, FullTag::StmtLetInt, FullTag::StmtLetBool, FullTag::StmtAssign,
    FullTag::StmtAssignInt, FullTag::StmtAssignBool, FullTag::StmtAssignPath, FullTag::StmtValue,
    FullTag::StmtReturn, FullTag::StmtAssert, FullTag::StmtExpr, FullTag::StmtIf,
    FullTag::StmtIfBool,
];

#[derive(Debug)]
struct RawInstructions {
    tags: Vec<FullTag>,
    resource_transfers: Vec<bool>,
    checked_call_target_required: Vec<bool>,
    data: Vec<IrData>,
    extra: Vec<u32>,
    assign_ops: Vec<AssignOp>,
    binary_ops: Vec<BinaryOp>,
    locations: Vec<IrLocation>,
    location_sources: Vec<SourceId>,
    strings: Vec<IrRange>,
    string_bytes: Vec<u8>,
}

fn assign_op_name(op: AssignOp) -> &'static str {
    match op {
        AssignOp::Set => "set", AssignOp::Add => "add", AssignOp::Sub => "sub",
        AssignOp::Mul => "mul", AssignOp::Div => "div", AssignOp::Rem => "rem",
    }
}

fn decode_assign_op(value: WireValue) -> Result<AssignOp, CandidateDecodeError> {
    let WireValue::String(name) = value else {
        return Err(CandidateDecodeError::Malformed("assignment operation is not a string".to_string()));
    };
    match name.as_str() {
        "set" => Ok(AssignOp::Set), "add" => Ok(AssignOp::Add), "sub" => Ok(AssignOp::Sub),
        "mul" => Ok(AssignOp::Mul), "div" => Ok(AssignOp::Div), "rem" => Ok(AssignOp::Rem),
        _ => Err(CandidateDecodeError::UnsupportedFeature(format!("assignment operation {name}"))),
    }
}

fn binary_op_name(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::ResultFallback => "resultFallback", BinaryOp::Or => "or", BinaryOp::And => "and",
        BinaryOp::Eq => "eq", BinaryOp::Ne => "ne", BinaryOp::Lt => "lt", BinaryOp::Le => "le",
        BinaryOp::Gt => "gt", BinaryOp::Ge => "ge", BinaryOp::In => "in", BinaryOp::NotIn => "notIn",
        BinaryOp::Add => "add", BinaryOp::Sub => "sub", BinaryOp::Mul => "mul", BinaryOp::Div => "div",
        BinaryOp::Rem => "rem", BinaryOp::Union => "union", BinaryOp::Intersect => "intersect",
    }
}

fn decode_binary_op(value: WireValue) -> Result<BinaryOp, CandidateDecodeError> {
    let WireValue::String(name) = value else {
        return Err(CandidateDecodeError::Malformed("binary operation is not a string".to_string()));
    };
    match name.as_str() {
        "resultFallback" => Ok(BinaryOp::ResultFallback), "or" => Ok(BinaryOp::Or), "and" => Ok(BinaryOp::And),
        "eq" => Ok(BinaryOp::Eq), "ne" => Ok(BinaryOp::Ne), "lt" => Ok(BinaryOp::Lt), "le" => Ok(BinaryOp::Le),
        "gt" => Ok(BinaryOp::Gt), "ge" => Ok(BinaryOp::Ge), "in" => Ok(BinaryOp::In), "notIn" => Ok(BinaryOp::NotIn),
        "add" => Ok(BinaryOp::Add), "sub" => Ok(BinaryOp::Sub), "mul" => Ok(BinaryOp::Mul), "div" => Ok(BinaryOp::Div),
        "rem" => Ok(BinaryOp::Rem), "union" => Ok(BinaryOp::Union), "intersect" => Ok(BinaryOp::Intersect),
        _ => Err(CandidateDecodeError::UnsupportedFeature(format!("binary operation {name}"))),
    }
}

fn wire_flag(value: bool) -> WireValue {
    wire_string(if value { "1" } else { "0" })
}

fn encode_instructions(rows: &RawFullRows) -> Result<String, CandidateDecodeError> {
    let tags = rows.tags.iter().map(|tag| {
        if !CONNECTED_TAGS.contains(tag) {
            return Err(CandidateDecodeError::UnsupportedTag(tag.name().to_string()));
        }
        Ok(wire_object([
            ("name", wire_string(tag.name())),
            ("ordinal", wire_string(format!("{:08x}", *tag as u32))),
        ]))
    }).collect::<Result<Vec<_>, _>>()?;
    let data = rows.data.iter().map(|data| wire_object([
        ("lhs", wire_string(format!("{:08x}", data.lhs))),
        ("rhs", wire_string(format!("{:08x}", data.rhs))),
    ])).collect();
    let wire = wire_object([
        ("tags", WireValue::Array(tags)),
        ("resourceTransfers", WireValue::Array(rows.resource_transfers.iter().copied().map(wire_flag).collect())),
        ("checkedCallTargetRequired", WireValue::Array(rows.checked_call_target_required.iter().copied().map(wire_flag).collect())),
        ("data", WireValue::Array(data)),
        ("extra", WireValue::Array(rows.extra.iter().map(|word| wire_string(format!("{word:08x}"))).collect())),
        ("assignOps", WireValue::Array(rows.assign_ops.iter().copied().map(|op| wire_string(assign_op_name(op))).collect())),
        ("binaryOps", WireValue::Array(rows.binary_ops.iter().copied().map(|op| wire_string(binary_op_name(op))).collect())),
        ("locations", WireValue::Array(rows.locations.iter().map(|location| wire_object([
            ("start", wire_string(format!("{:08x}", location.start))),
            ("length", wire_string(format!("{:08x}", location.len))),
        ])).collect())),
        ("locationSources", WireValue::Array(rows.location_sources.iter().map(|source| wire_string(format!("{:08x}", source.raw()))).collect())),
        ("strings", WireValue::Array(rows.strings.iter().copied().map(wire_range).collect())),
        ("stringBytes", WireValue::Array(rows.string_bytes.iter().map(|value| wire_word(u32::from(*value))).collect())),
    ]);
    deser_json::to_string(&wire).map_err(|error| CandidateDecodeError::Malformed(error.to_string()))
}

fn decode_flag(value: WireValue) -> Result<bool, CandidateDecodeError> {
    match value {
        WireValue::String(value) if value == "0" => Ok(false),
        WireValue::String(value) if value == "1" => Ok(true),
        _ => Err(CandidateDecodeError::Malformed("instruction mark is not 0 or 1".to_string())),
    }
}

fn decode_instructions(input: &str) -> Result<RawInstructions, CandidateDecodeError> {
    let mut object = parse_object_fields(parse_wire_document(input)?,
        &["tags", "resourceTransfers", "checkedCallTargetRequired", "data", "extra", "assignOps", "binaryOps", "locations", "locationSources", "strings", "stringBytes"])?;
    let tags = take_array(&mut object, "tags")?.into_iter().map(|value| {
        let mut row = parse_object_fields(value, &["name", "ordinal"])?;
        let name = take_string(&mut row, "name")?;
        let ordinal = parse_hex_u32(&take_string(&mut row, "ordinal")?)?;
        let tag = CONNECTED_TAGS.iter().copied().find(|tag| tag.name() == name)
            .ok_or_else(|| CandidateDecodeError::UnsupportedTag(name.clone()))?;
        if tag as u32 != ordinal {
            return Err(CandidateDecodeError::Malformed("instruction tag ordinal differs from its name".to_string()));
        }
        Ok(tag)
    }).collect::<Result<Vec<_>, CandidateDecodeError>>()?;
    let resource_transfers: Vec<bool> = take_array(&mut object, "resourceTransfers")?.into_iter().map(decode_flag).collect::<Result<_, _>>()?;
    if resource_transfers.iter().any(|marked| *marked) {
        return Err(CandidateDecodeError::UnsupportedFeature("resource transfers".to_string()));
    }
    let checked_call_target_required = take_array(&mut object, "checkedCallTargetRequired")?.into_iter().map(decode_flag).collect::<Result<_, _>>()?;
    let data = take_array(&mut object, "data")?.into_iter().map(|value| {
        let mut row = parse_object_fields(value, &["lhs", "rhs"])?;
        Ok(IrData {
            lhs: parse_hex_u32(&take_string(&mut row, "lhs")?)?,
            rhs: parse_hex_u32(&take_string(&mut row, "rhs")?)?,
        })
    }).collect::<Result<Vec<_>, CandidateDecodeError>>()?;
    let extra = take_array(&mut object, "extra")?.into_iter().map(|value| match value {
        WireValue::String(text) => parse_hex_u32(&text).map_err(CandidateDecodeError::from),
        _ => Err(CandidateDecodeError::Malformed("instruction extra word is not a string".to_string())),
    }).collect::<Result<Vec<_>, _>>()?;
    let assign_ops = take_array(&mut object, "assignOps")?.into_iter().map(decode_assign_op).collect::<Result<Vec<_>, _>>()?;
    let binary_ops = take_array(&mut object, "binaryOps")?.into_iter().map(decode_binary_op).collect::<Result<Vec<_>, _>>()?;
    let locations = take_array(&mut object, "locations")?.into_iter().map(|value| {
        let mut row = parse_object_fields(value, &["start", "length"])?;
        Ok(IrLocation { start: parse_hex_u32(&take_string(&mut row, "start")?)?,
            len: parse_hex_u32(&take_string(&mut row, "length")?)? })
    }).collect::<Result<Vec<_>, CandidateDecodeError>>()?;
    let location_sources = take_array(&mut object, "locationSources")?.into_iter().map(|value| match value {
        WireValue::String(text) => Ok(SourceId::new(parse_hex_u32(&text)? as usize)),
        _ => Err(CandidateDecodeError::Malformed("location source is not a string".to_string())),
    }).collect::<Result<Vec<_>, CandidateDecodeError>>()?;
    let strings = take_array(&mut object, "strings")?.into_iter().map(decode_range).collect::<Result<Vec<_>, _>>()?;
    let string_bytes = take_array(&mut object, "stringBytes")?.into_iter().map(|value| match value {
        WireValue::String(text) => u8::try_from(parse_hex_u32(&text)?)
            .map_err(|_| CandidateDecodeError::Malformed("string byte is out of range".to_string())),
        _ => Err(CandidateDecodeError::Malformed("string byte is not a string".to_string())),
    }).collect::<Result<Vec<_>, CandidateDecodeError>>()?;
    Ok(RawInstructions { tags, resource_transfers, checked_call_target_required, data, extra, assign_ops, binary_ops, locations, location_sources, strings, string_bytes })
}


#[derive(Debug)]
struct RawFunctions {
    blocks: Vec<FullBlock>,
    functions: Vec<FullFunction>,
}

fn wire_word(value: u32) -> WireValue {
    wire_string(format!("{value:08x}"))
}

fn wire_range(value: IrRange) -> WireValue {
    wire_object([("start", wire_word(value.start)), ("length", wire_word(value.len))])
}

fn decode_word(fields: &mut BTreeMap<String, WireValue>, name: &str) -> Result<u32, CandidateDecodeError> {
    Ok(parse_hex_u32(&take_string(fields, name)?)?)
}

fn decode_range(value: WireValue) -> Result<IrRange, CandidateDecodeError> {
    let mut fields = parse_object_fields(value, &["start", "length"])?;
    Ok(IrRange { start: decode_word(&mut fields, "start")?, len: decode_word(&mut fields, "length")? })
}

fn take_range(fields: &mut BTreeMap<String, WireValue>, name: &str) -> Result<IrRange, CandidateDecodeError> {
    let value = fields.remove(name).ok_or_else(|| CandidateDecodeError::Malformed(format!("missing {name}")))?;
    decode_range(value)
}

fn decode_byte(fields: &mut BTreeMap<String, WireValue>, name: &str) -> Result<u8, CandidateDecodeError> {
    u8::try_from(decode_word(fields, name)?)
        .map_err(|_| CandidateDecodeError::Malformed(format!("{name} does not fit a byte")))
}

fn encode_functions(rows: &RawFullRows) -> Result<String, CandidateDecodeError> {
    let blocks = rows.blocks.iter().map(|block| wire_object([
        ("instructions", wire_range(block.instructions)), ("result", wire_word(block.result)),
        ("owner", wire_word(block.owner)), ("flags", wire_word(u32::from(block.flags))),
        ("reserved0", wire_word(u32::from(block.reserved[0]))),
        ("reserved1", wire_word(u32::from(block.reserved[1]))),
        ("reserved2", wire_word(u32::from(block.reserved[2]))),
    ])).collect();
    let functions = rows.functions.iter().map(|function| {
        let (kind, contract) = match function.contract {
            FullCallableContract::Ground(id) => ("ground", id.raw()),
            FullCallableContract::Scheme(id) => ("scheme", id.index() as u32),
        };
        wire_object([
            ("name", wire_word(function.name)), ("contractKind", wire_string(kind)),
            ("contract", wire_word(contract)), ("params", wire_range(function.params)),
            ("captures", wire_range(function.captures)), ("body", wire_word(function.body)),
            ("slotCount", wire_word(function.slot_count)),
            ("slotTypeFacts", wire_range(function.slot_type_facts)),
            ("checkedReturnType", wire_word(function.checked_return_type)),
        ])
    }).collect();
    deser_json::to_string(&wire_object([
        ("blocks", WireValue::Array(blocks)), ("functions", WireValue::Array(functions)),
    ])).map_err(|error| CandidateDecodeError::Malformed(error.to_string()))
}

fn decode_functions(input: &str) -> Result<RawFunctions, CandidateDecodeError> {
    let mut object = parse_object_fields(parse_wire_document(input)?, &["blocks", "functions"])?;
    let blocks = take_array(&mut object, "blocks")?.into_iter().map(|value| {
        let mut row = parse_object_fields(value, &["instructions", "result", "owner", "flags", "reserved0", "reserved1", "reserved2"])?;
        Ok(FullBlock {
            instructions: take_range(&mut row, "instructions")?, result: decode_word(&mut row, "result")?,
            owner: decode_word(&mut row, "owner")?, flags: decode_byte(&mut row, "flags")?,
            reserved: [decode_byte(&mut row, "reserved0")?, decode_byte(&mut row, "reserved1")?, decode_byte(&mut row, "reserved2")?],
        })
    }).collect::<Result<Vec<_>, CandidateDecodeError>>()?;
    let functions = take_array(&mut object, "functions")?.into_iter().map(|value| {
        let mut row = parse_object_fields(value, &["name", "contractKind", "contract", "params", "captures", "body", "slotCount", "slotTypeFacts", "checkedReturnType"])?;
        let name = decode_word(&mut row, "name")?;
        let kind = take_string(&mut row, "contractKind")?;
        let raw = decode_word(&mut row, "contract")?;
        let contract = match kind.as_str() {
            "ground" => FullCallableContract::Ground(SignatureId::from_raw(raw)
                .ok_or_else(|| CandidateDecodeError::Malformed("invalid signature ID".to_string()))?),
            "scheme" => FullCallableContract::Scheme(FullSchemeId::from_raw(raw)
                .map_err(|_| CandidateDecodeError::Malformed("invalid scheme ID".to_string()))?),
            _ => return Err(CandidateDecodeError::UnsupportedTag(kind)),
        };
        Ok(FullFunction {
            name, contract, params: take_range(&mut row, "params")?, captures: take_range(&mut row, "captures")?,
            body: decode_word(&mut row, "body")?, slot_count: decode_word(&mut row, "slotCount")?,
            slot_type_facts: take_range(&mut row, "slotTypeFacts")?, checked_return_type: decode_word(&mut row, "checkedReturnType")?,
        })
    }).collect::<Result<Vec<_>, CandidateDecodeError>>()?;
    Ok(RawFunctions { blocks, functions })
}


fn export_resource_transfers(
    arena: &crate::syntax::arena::ArenaProgram,
    published: &super::facts::PublishedFacts,
) -> Result<String, String> {
    use crate::sema::check::ResourceTransferSite;
    use crate::syntax::arena::{ArenaExprOrRun, ArenaStmtKind};
    let mut facts = published.declarations.bodies.resource_transfers.iter().map(|(site, transfers)| {
        let (role, span) = match *site {
            ResourceTransferSite::BlockValue(block) => ("block", arena.arena.span(arena.arena.block(block).span)),
            ResourceTransferSite::Return(statement) => ("return", arena.arena.stmt(statement).span),
            ResourceTransferSite::Assignment(statement) => ("assignment", arena.arena.stmt(statement).span),
            ResourceTransferSite::Break(statement) => ("break", arena.arena.stmt(statement).span),
            ResourceTransferSite::Yield(statement) => ("yield", arena.arena.stmt(statement).span),
        };
        let checked_override = match *site {
            ResourceTransferSite::BlockValue(block) => arena.arena.stmt_ids(arena.arena.block(block).statements).last()
                .and_then(|statement| {
                    let node = arena.arena.stmt(statement);
                    let tail = match node.kind {
                        ArenaStmtKind::Expr(expression) => Some(arena.arena.expr(expression).span),
                        ArenaStmtKind::TailBareIdent(_) => Some(node.span),
                        _ => None,
                    }?;
                    published.checker_expr_types.get(&tail).map(|ty| ty.may_hold_resource())
                }),
            ResourceTransferSite::Assignment(statement) => match arena.arena.stmt(statement).kind {
                ArenaStmtKind::Assign { value: ArenaExprOrRun::Expr(expression), .. } => {
                    let span = arena.arena.expr(expression).span;
                    published.checker_expr_types.get(&span).map(|ty| ty.may_hold_resource())
                }
                _ => None,
            },
            _ => None,
        };
        (span.source_id.raw(), span.start(), span.end() - span.start(), role, *transfers, checked_override)
    }).collect::<Vec<_>>();
    facts.sort_unstable();
    let facts = facts.into_iter().map(|(source, start, length, role, transfers, checked_override)| wire_object([
        ("role", wire_string(role)),
        ("site", wire_object([
            ("source", wire_string(format!("{source:08x}"))),
            ("start", wire_string(format!("{start:08x}"))),
            ("length", wire_string(format!("{length:08x}"))),
        ])),
        ("transfers", wire_string(if transfers { "true" } else { "false" })),
        ("checkedOverride", WireValue::Array(checked_override.into_iter()
            .map(|value| wire_string(if value { "true" } else { "false" })).collect())),
    ])).collect();
    let value = wire_object([
        ("schema", wire_string("xsh-connected-resource-transfers-v1")),
        ("facts", WireValue::Array(facts)),
    ]);
    deser_json::to_string(&value).map_err(|error| error.to_string())
}

#[derive(Clone, Debug)]
struct RawFullOwner {
    sources: Arc<SourceMap>,
    source_id: SourceId,
    binding_namespace: u64,
    function_definition_spans: Vec<Span>,
}

fn encode_owner(program: &RawFullProgram) -> Result<String, String> {
    let sources = program.sources.files().iter().map(|source| wire_object([
        ("id", wire_string(format!("{:08x}", source.id().raw()))),
        ("name", wire_string(source.name())),
        ("text", wire_string(source.text())),
    ])).collect();
    let spans = program.function_definition_spans.iter().map(|span| wire_object([
        ("source", wire_string(format!("{:08x}", span.source_id.raw()))),
        ("start", wire_string(format!("{:08x}", span.start()))),
        ("length", wire_string(format!("{:08x}", span.end() - span.start()))),
    ])).collect();
    let wire = wire_object([
        ("sources", WireValue::Array(sources)),
        ("sourceId", wire_string(format!("{:08x}", program.rows.source_id.raw()))),
        ("bindingNamespace", wire_string(format!("{:016x}", program.binding_namespace))),
        ("definitionSpans", WireValue::Array(spans)),
    ]);
    deser_json::to_string(&wire).map_err(|error| error.to_string())
}

fn decode_owner(input: &str, expected: &SourceMap) -> Result<RawFullOwner, String> {
    let mut object = parse_object_fields(parse_wire_document(input)?, &["sources", "sourceId", "bindingNamespace", "definitionSpans"])?;
    let mut sources = SourceMap::new();
    for (index, value) in take_array(&mut object, "sources")?.into_iter().enumerate() {
        let mut source = parse_object_fields(value, &["id", "name", "text"])?;
        let id = parse_hex_u32(&take_string(&mut source, "id")?)? as usize;
        if id != index {
            return Err("source IDs do not follow source table order".to_string());
        }
        let name = take_string(&mut source, "name")?;
        let text = take_string(&mut source, "text")?;
        let added = sources.add_file(name.clone(), text);
        if sources.get(added).map(|file| file.name()) != Some(name.as_str()) {
            return Err("source name changes during source-map construction".to_string());
        }
    }
    if sources.files().len() != expected.files().len() || sources.files().iter().zip(expected.files()).any(|(actual, wanted)|
        actual.id() != wanted.id() || actual.name() != wanted.name() || actual.text() != wanted.text()) {
        return Err("source document does not match expected source".to_string());
    }
    let source_id = SourceId::new(parse_hex_u32(&take_string(&mut object, "sourceId")?)? as usize);
    let binding_namespace = parse_hex_u64(&take_string(&mut object, "bindingNamespace")?)?;
    let mut function_definition_spans = Vec::new();
    for value in take_array(&mut object, "definitionSpans")? {
        let mut span = parse_object_fields(value, &["source", "start", "length"])?;
        let source = parse_hex_u32(&take_string(&mut span, "source")?)? as usize;
        let start = parse_hex_u32(&take_string(&mut span, "start")?)? as usize;
        let length = parse_hex_u32(&take_string(&mut span, "length")?)? as usize;
        let end = start.checked_add(length).ok_or_else(|| "definition span overflows".to_string())?;
        function_definition_spans.push(Span::new(SourceId::new(source), start, end));
    }
    Ok(RawFullOwner { sources: Arc::new(sources), source_id, binding_namespace, function_definition_spans })
}

#[test]
fn scoped_instruction_exchange_retains_assignment_operations() {
    let source = "pure scoped(value) { var current = value; without time { current = value }; current }\nassert scoped(7) == 7\n";
    let (_, _, program) = build_fixture(source).unwrap();
    let raw = RawFullProgram::capture(&program);
    assert!(raw.rows.tags.contains(&FullTag::StmtAssignPath));
    assert!(raw.rows.tags.contains(&FullTag::ExprUnit));
    assert!(raw.rows.resource_transfers.iter().any(|marked| *marked));
    let actual = encode_instructions(&raw.rows).unwrap();
    assert!(matches!(decode_instructions(&actual), Err(CandidateDecodeError::UnsupportedFeature(_))));
    let mut unmarked = raw.rows.clone();
    unmarked.resource_transfers.fill(false);
    let encoded = encode_instructions(&unmarked).unwrap();
    let decoded = decode_instructions(&encoded).unwrap();
    assert_eq!(decoded.tags, unmarked.tags);
    assert_eq!(decoded.data, unmarked.data);
    assert_eq!(decoded.extra, unmarked.extra);
    assert_eq!(decoded.assign_ops, unmarked.assign_ops);
    let mut changed = parse_wire_document(&encoded).unwrap();
    let WireValue::Object(fields) = &mut changed else { unreachable!() };
    let (_, WireValue::Array(operations)) = fields.iter_mut().find(|(name, _)| name == "assignOps").unwrap() else { unreachable!() };
    let index = unmarked.assign_ops.iter().position(|op| *op == AssignOp::Set).unwrap();
    operations[index] = wire_string("add");
    let changed = deser_json::to_string(&changed).unwrap();
    assert_ne!(decode_instructions(&changed).unwrap().assign_ops, decoded.assign_ops);
}

#[test]
fn scoped_assignment_transfer_exports_checked_type_override() {
    use crate::sema::check::ResourceTransferSite;
    use crate::syntax::arena::{ArenaExprOrRun, ArenaStmtKind, StmtId};
    let source = "pure scoped(value) { var current = value; without time { current = value }; current }\nassert scoped(7) == 7\n";
    let (_, _, parsed, mut published) = check_fixture(source).unwrap();
    let assignment = (0..parsed.arena.arena.stmt_tags.len()).map(StmtId::from_index).find(|&id|
        matches!(parsed.arena.arena.stmt(id).kind, ArenaStmtKind::Assign { .. })).unwrap();
    assert_eq!(published.declarations.bodies.resource_transfer(ResourceTransferSite::Assignment(assignment)), Some(true));
    let ArenaStmtKind::Assign { value: ArenaExprOrRun::Expr(value), .. } = parsed.arena.arena.stmt(assignment).kind else {
        panic!("scoped assignment must have an expression value");
    };
    let span = parsed.arena.arena.expr(value).span;
    let before = export_resource_transfers(&parsed.arena, &published).unwrap();
    let lean_documents = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"),
        "/formal/Xsh/Conformance/Connected/ScopedTransferDocuments.lean")).unwrap();
    assert_eq!(lean_document_from(&lean_documents, "scopedTransferFactsCodes").as_deref(), Some(before.as_str()));
    published.checker_expr_types.insert(span, crate::sema::types::Type::Int);
    let changed = export_resource_transfers(&parsed.arena, &published).unwrap();
    assert_ne!(before, changed);
}

#[test]
fn raw_full_rows_reconstruct_candidate_and_preserve_verifier_failures() {
    let generic_source = "pure project(value) { value.amount }\npure use_project() -> Int { project({amount: 7, enabled: true}) }\nassert use_project() == 7\n";
    let (_, _, parsed, mut published) = check_fixture(generic_source).unwrap();
    let project_body = parsed.arena.arena.function_defs[0].body;
    let transfer_site = crate::sema::check::ResourceTransferSite::BlockValue(project_body);
    assert_eq!(published.declarations.bodies.resource_transfer(transfer_site), Some(true));
    let transfer_bytes = export_resource_transfers(&parsed.arena, &published).unwrap();
    published.declarations.bodies.resource_transfers.insert(transfer_site, false);
    let forged_transfer_bytes = export_resource_transfers(&parsed.arena, &published).unwrap();
    assert_ne!(transfer_bytes, forged_transfer_bytes);
    let tail = parsed.arena.arena.stmt_ids(parsed.arena.arena.block(project_body).statements).last().unwrap();
    let crate::syntax::arena::ArenaStmtKind::Expr(expression) = parsed.arena.arena.stmt(tail).kind else {
        panic!("projection body has no expression tail");
    };
    let tail_span = parsed.arena.arena.expr(expression).span;
    published.declarations.bodies.resource_transfers.insert(transfer_site, true);
    let original_tail_type = published.checker_expr_types.insert(tail_span, crate::sema::types::Type::Int);
    let forged_override_bytes = export_resource_transfers(&parsed.arena, &published).unwrap();
    assert_ne!(forged_transfer_bytes, forged_override_bytes);
    match original_tail_type {
        Some(ty) => { published.checker_expr_types.insert(tail_span, ty); }
        None => { published.checker_expr_types.remove(&tail_span); }
    }
    published.declarations.bodies.resource_transfers.insert(transfer_site, true);
    let caller_body = parsed.arena.arena.function_defs[1].body;
    let caller_tail = parsed.arena.arena.stmt_ids(parsed.arena.arena.block(caller_body).statements).last().unwrap();
    let crate::syntax::arena::ArenaStmtKind::Expr(caller_expression) = parsed.arena.arena.stmt(caller_tail).kind else {
        panic!("caller body has no expression tail");
    };
    let caller_span = parsed.arena.arena.expr(caller_expression).span;
    assert!(published.checker_expr_types.remove(&caller_span).is_some());
    let missing_override_bytes = export_resource_transfers(&parsed.arena, &published).unwrap();
    assert_ne!(transfer_bytes, missing_override_bytes);
    let lean_documents = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"),
        "/formal/Xsh/Conformance/Connected/ResourceDocuments.lean")).unwrap();
    assert_eq!(lean_document_from(&lean_documents, "resourceFactsCodes").as_deref(), Some(transfer_bytes.as_str()));
    assert_eq!(lean_document_from(&lean_documents, "resourceBadBitCodes").as_deref(), Some(forged_transfer_bytes.as_str()));
    assert_eq!(lean_document_from(&lean_documents, "resourceBadOverrideCodes").as_deref(), Some(forged_override_bytes.as_str()));
    assert_eq!(lean_document_from(&lean_documents, "resourceMissingOverrideCodes").as_deref(), Some(missing_override_bytes.as_str()));
    let (_, _, generic_program) = build_fixture(generic_source).unwrap();
    let generic_raw = RawFullProgram::capture(&generic_program);
    FullVerifier::verify(&generic_raw.clone().into_program()).unwrap();
    let generic_bytes = encode_instructions(&generic_raw.rows).unwrap();
    assert!(matches!(decode_instructions(&generic_bytes), Err(CandidateDecodeError::UnsupportedFeature(_))));

    let source = "pure number(value: Int) -> Int { value + 1 }\nassert number(7) == 8\n";
    let (_, _, program) = build_fixture(source).unwrap();
    let raw = RawFullProgram::capture(&program);
    let owner_bytes = encode_owner(&raw).unwrap();
    let owner = decode_owner(&owner_bytes, &program.sources).unwrap();
    let instruction_bytes = encode_instructions(&raw.rows).unwrap();
    let instructions = decode_instructions(&instruction_bytes).unwrap();
    assert_eq!(instructions.assign_ops, raw.rows.assign_ops);
    assert_eq!(instructions.binary_ops, raw.rows.binary_ops);
    assert_eq!(instructions.locations, raw.rows.locations);
    assert_eq!(instructions.location_sources, raw.rows.location_sources);
    assert_eq!(instructions.strings, raw.rows.strings);
    assert_eq!(instructions.string_bytes, raw.rows.string_bytes);
    let mut changed_ops = parse_wire_document(&instruction_bytes).unwrap();
    let WireValue::Object(fields) = &mut changed_ops else { unreachable!() };
    let (_, WireValue::Array(ops)) = fields.iter_mut().find(|(name, _)| name == "binaryOps").unwrap() else { unreachable!() };
    let addition = raw.rows.binary_ops.iter().position(|op| *op == BinaryOp::Add).unwrap();
    ops[addition] = wire_string("sub");
    let changed_bytes = deser_json::to_string(&changed_ops).unwrap();
    let changed_instructions = decode_instructions(&changed_bytes).unwrap();
    assert_eq!(changed_instructions.binary_ops[addition], BinaryOp::Sub);
    assert_ne!(changed_instructions.binary_ops, instructions.binary_ops);
    let mut missing_location_source = parse_wire_document(&instruction_bytes).unwrap();
    let WireValue::Object(fields) = &mut missing_location_source else { unreachable!() };
    let (_, WireValue::Array(sources)) = fields.iter_mut().find(|(name, _)| name == "locationSources").unwrap() else { unreachable!() };
    sources.pop().unwrap();
    let missing_location_bytes = deser_json::to_string(&missing_location_source).unwrap();
    let missing_location = decode_instructions(&missing_location_bytes).unwrap();
    let mut candidate = raw.clone();
    candidate.rows.locations = missing_location.locations;
    candidate.rows.location_sources = missing_location.location_sources;
    assert!(FullVerifier::verify(&candidate.into_program()).is_err());
    let mut bad_string = parse_wire_document(&instruction_bytes).unwrap();
    let WireValue::Object(fields) = &mut bad_string else { unreachable!() };
    let (_, WireValue::Array(strings)) = fields.iter_mut().find(|(name, _)| name == "strings").unwrap() else { unreachable!() };
    let WireValue::Object(range) = strings.first_mut().unwrap() else { unreachable!() };
    let (_, length) = range.iter_mut().find(|(name, _)| name == "length").unwrap();
    *length = wire_word(IR_NONE);
    let bad_string_bytes = deser_json::to_string(&bad_string).unwrap();
    let bad_string = decode_instructions(&bad_string_bytes).unwrap();
    let mut candidate = raw.clone();
    candidate.rows.strings = bad_string.strings;
    candidate.rows.string_bytes = bad_string.string_bytes;
    assert!(FullVerifier::verify(&candidate.into_program()).is_err());
    let function_bytes = encode_functions(&raw.rows).unwrap();
    let function_rows = decode_functions(&function_bytes).unwrap();
    let mut foreign_owner = raw.clone();
    foreign_owner.sources = owner.sources;
    foreign_owner.rows.source_id = owner.source_id;
    foreign_owner.binding_namespace = owner.binding_namespace;
    foreign_owner.function_definition_spans = owner.function_definition_spans;
    foreign_owner.rows.tags = instructions.tags;
    foreign_owner.rows.resource_transfers = instructions.resource_transfers;
    foreign_owner.rows.checked_call_target_required = instructions.checked_call_target_required;
    foreign_owner.rows.data = instructions.data;
    foreign_owner.rows.extra = instructions.extra;
    foreign_owner.rows.assign_ops = instructions.assign_ops;
    foreign_owner.rows.binary_ops = instructions.binary_ops;
    foreign_owner.rows.locations = instructions.locations;
    foreign_owner.rows.location_sources = instructions.location_sources;
    foreign_owner.rows.strings = instructions.strings;
    foreign_owner.rows.string_bytes = instructions.string_bytes;
    foreign_owner.rows.blocks = function_rows.blocks;
    foreign_owner.rows.functions = function_rows.functions;
    FullVerifier::verify(&foreign_owner.into_program()).unwrap();
    let mut bad_rows = parse_wire_document(&instruction_bytes).unwrap();
    let WireValue::Object(fields) = &mut bad_rows else { unreachable!() };
    let (_, WireValue::Array(data)) = fields.iter_mut().find(|(name, _)| name == "data").unwrap() else { unreachable!() };
    data.pop().unwrap();
    let bad_bytes = deser_json::to_string(&bad_rows).unwrap();
    let bad_instructions = decode_instructions(&bad_bytes).unwrap();
    let mut bad_candidate = raw.clone();
    bad_candidate.rows.tags = bad_instructions.tags;
    bad_candidate.rows.resource_transfers = bad_instructions.resource_transfers;
    bad_candidate.rows.checked_call_target_required = bad_instructions.checked_call_target_required;
    bad_candidate.rows.data = bad_instructions.data;
    bad_candidate.rows.extra = bad_instructions.extra;
    bad_candidate.rows.assign_ops = bad_instructions.assign_ops;
    bad_candidate.rows.binary_ops = bad_instructions.binary_ops;
    bad_candidate.rows.locations = bad_instructions.locations;
    bad_candidate.rows.location_sources = bad_instructions.location_sources;
    bad_candidate.rows.strings = bad_instructions.strings;
    bad_candidate.rows.string_bytes = bad_instructions.string_bytes;
    assert!(FullVerifier::verify(&bad_candidate.into_program()).is_err());

    let mut bad_function = parse_wire_document(&function_bytes).unwrap();
    let WireValue::Object(fields) = &mut bad_function else { unreachable!() };
    let (_, WireValue::Array(functions)) = fields.iter_mut().find(|(name, _)| name == "functions").unwrap() else { unreachable!() };
    let WireValue::Object(function) = functions.first_mut().unwrap() else { unreachable!() };
    let (_, body) = function.iter_mut().find(|(name, _)| name == "body").unwrap();
    *body = wire_word(IR_NONE);
    let bad_function_bytes = deser_json::to_string(&bad_function).unwrap();
    let bad_function_rows = decode_functions(&bad_function_bytes).unwrap();
    let mut bad_function_candidate = raw.clone();
    bad_function_candidate.rows.blocks = bad_function_rows.blocks;
    bad_function_candidate.rows.functions = bad_function_rows.functions;
    assert!(FullVerifier::verify(&bad_function_candidate.into_program()).is_err());

    let mut forged_transfer = parse_wire_document(&instruction_bytes).unwrap();
    let WireValue::Object(fields) = &mut forged_transfer else { unreachable!() };
    let (_, WireValue::Array(marks)) = fields.iter_mut().find(|(name, _)| name == "resourceTransfers").unwrap() else { unreachable!() };
    let return_site = raw.rows.tags.iter().position(|tag| *tag == FullTag::StmtReturn).unwrap();
    marks[return_site] = wire_flag(true);
    let forged_transfer_bytes = deser_json::to_string(&forged_transfer).unwrap();
    assert!(matches!(decode_instructions(&forged_transfer_bytes), Err(CandidateDecodeError::UnsupportedFeature(_))));

    let mut unsupported = parse_wire_document(&instruction_bytes).unwrap();
    let WireValue::Object(fields) = &mut unsupported else { unreachable!() };
    let (_, WireValue::Array(tags)) = fields.iter_mut().find(|(name, _)| name == "tags").unwrap() else { unreachable!() };
    let WireValue::Object(tag) = tags.first_mut().unwrap() else { unreachable!() };
    let (_, name) = tag.iter_mut().find(|(key, _)| key == "name").unwrap();
    *name = wire_string("ForeignOpcode");
    let unsupported_bytes = deser_json::to_string(&unsupported).unwrap();
    assert!(matches!(decode_instructions(&unsupported_bytes), Err(CandidateDecodeError::UnsupportedTag(_))));
    let mut other_source = SourceMap::new();
    for source in program.sources.files() {
        other_source.add_file(source.name(), format!("{} ", source.text()));
    }
    assert!(decode_owner(&owner_bytes, &other_source).is_err());
    let candidate = raw.clone().into_program();
    assert_eq!(candidate.binding_namespace, program.binding_namespace);
    assert_eq!(candidate.function_definition_spans, program.function_definition_spans);
    assert_eq!(candidate.store.tags, program.store.tags);
    assert_eq!(candidate.store.data, program.store.data);
    assert_eq!(candidate.store.extra, program.store.extra);
    assert_eq!(candidate.store.functions, program.store.functions);
    assert_eq!(candidate.store.semantic.export_raw(&candidate.symbols).tables, raw.rows.semantic.tables);
    FullVerifier::verify(&candidate).unwrap();

    let mut missing_data = raw.clone();
    missing_data.rows.data.pop().unwrap();
    assert!(FullVerifier::verify(&missing_data.into_program()).is_err());

    let mut missing_span = raw.clone();
    missing_span.function_definition_spans.pop().unwrap();
    assert!(FullVerifier::verify(&missing_span.into_program()).is_err());

    let mut missing_semantic_data = raw;
    missing_semantic_data.rows.semantic.tables.type_data.pop().unwrap();
    assert!(FullVerifier::verify(&missing_semantic_data.into_program()).is_err());
}

fn export_local_declarations(program: &FullProgram, function: u32) -> Result<String, String> {
    use crate::runtime::eval::declaration_trace;
    let (lowered, emitted, _) = declaration_trace::recorded();
    let mut rows = BTreeMap::new();
    for source in &lowered {
        let mut matches = emitted.iter().filter(|candidate| candidate.statement == source.statement);
        let (Some(declaration), None) = (matches.next(), matches.next()) else {
            return Err(format!("lowered declaration {} is not emitted exactly once", source.statement));
        };
        if declaration.function != function {
            continue;
        }
        let row = declaration.instruction;
        if program.store.tags.get(row) != Some(&FullTag::StmtLet) {
            return Err(format!("declaration row {row} is not a StmtLet"));
        }
        let payload = program.store.data[row].lhs as usize;
        if program.store.extra[payload] as usize != source.slot {
            return Err(format!("declaration row {row} stores another slot than lowering assigned"));
        }
        if rows.insert(row, source.span).is_some() {
            return Err(format!("declaration row {row} is claimed twice"));
        }
    }
    let locals = rows.iter().map(|(row, span)| wire_object([
        ("site", wire_object([
            ("source", wire_string(format!("{:08x}", span.source_id.raw()))),
            ("start", wire_string(format!("{:08x}", span.start()))),
            ("length", wire_string(format!("{:08x}", span.end() - span.start()))),
        ])),
        ("row", wire_string(format!("{row:08x}"))),
    ])).collect();
    let facts = program.store.scheme_body_facts.iter().filter(|fact| fact.function.raw() == function)
        .filter_map(|fact| match fact.value {
            schemes::FullSchemeBodyFactValue::LocalImmutable(_) => Some((fact.instruction, "immutable")),
            schemes::FullSchemeBodyFactValue::LocalMutable(_) => Some((fact.instruction, "mutable")),
            _ => None,
        })
        .map(|(instruction, kind)| wire_object([
            ("instruction", wire_string(format!("{instruction:08x}"))),
            ("kind", wire_string(kind)),
        ])).collect();
    let value = wire_object([
        ("schema", wire_string("xsh-connected-raw-locals-v1")),
        ("function", wire_string(format!("{function:08x}"))),
        ("locals", WireValue::Array(locals)),
        ("facts", WireValue::Array(facts)),
    ]);
    deser_json::to_string(&value).map_err(|error| error.to_string())
}

const LEAN_RAW_LOCAL_DOCUMENTS: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"),
    "/formal/Xsh/Conformance/Connected/RawLocalDocuments.lean"));

#[test]
fn raw_local_declarations_bind_source_sites_to_store_rows() {
    use crate::runtime::eval::declaration_trace;
    let mut stale = Vec::new();
    for (source, prefix, parameters) in [
        ("pure scoped(value) { var current = value; without time { current = value }; current }\nassert scoped(7) == 7\n",
            "scoped", 1usize),
        ("pure pair(a, b) { var x = a; var y = b; y = x; y }\nassert pair(1, 2) == 1\n", "pair", 2usize),
    ] {
        declaration_trace::clear();
        let (_, _, program) = build_fixture(source).unwrap();
        let raw = RawFullProgram::capture(&program);
        for (name, exported) in [
            (format!("{prefix}LocalDeclarationsCodes"), export_local_declarations(&program, 1).unwrap()),
            (format!("{prefix}RawInstructionCodes"), encode_instructions(&raw.rows).unwrap()),
            (format!("{prefix}RawFunctionCodes"), encode_functions(&raw.rows).unwrap()),
        ] {
            if lean_document_from(LEAN_RAW_LOCAL_DOCUMENTS, &name).as_deref() != Some(exported.as_str()) {
                stale.push(lean_definition(&name, &exported));
            }
        }
        let (_, _, recorded) = declaration_trace::recorded();
        assert_eq!(recorded.iter().map(|parameter| parameter.slot).collect::<Vec<_>>(), (0..parameters).collect::<Vec<_>>());
        assert_eq!(program.store.functions[0].params.len as usize, parameters);
    }
    assert!(stale.is_empty(), "the Lean documents must hold the exported bytes; the current export is\n{}", stale.join("\n"));
}
