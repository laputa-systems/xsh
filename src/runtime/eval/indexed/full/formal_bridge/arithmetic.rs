use super::*;
use crate::syntax::node::BinaryOp;

const ARITHMETIC_SCOPE: &str = "integer-arithmetic-local-fixture-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectedTag {
    IntInt,
    StmtLetInt,
    ExprInt,
    ExprParam,
    ExprBinary,
    StmtReturn,
}

impl ProjectedTag {
    const ALL: [Self; 6] = [
        Self::IntInt,
        Self::StmtLetInt,
        Self::ExprInt,
        Self::ExprParam,
        Self::ExprBinary,
        Self::StmtReturn,
    ];

    fn from_full_tag(tag: FullTag) -> Option<Self> {
        Self::ALL.into_iter().find(|projected| projected.full_tag() == tag)
    }

    fn full_tag(self) -> FullTag {
        match self {
            Self::IntInt => FullTag::IntInt,
            Self::StmtLetInt => FullTag::StmtLetInt,
            Self::ExprInt => FullTag::ExprInt,
            Self::ExprParam => FullTag::ExprParam,
            Self::ExprBinary => FullTag::ExprBinary,
            Self::StmtReturn => FullTag::StmtReturn,
        }
    }

    fn from_name(name: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|projected| projected.name() == name)
            .ok_or_else(|| format!("unknown raw-IR tag {name:?}"))
    }

    fn name(self) -> &'static str {
        self.full_tag().name()
    }

    fn ordinal(self) -> u32 {
        self.full_tag() as u32
    }

    fn payload_words(self) -> usize {
        match self {
            Self::IntInt | Self::StmtLetInt | Self::ExprInt => 2,
            Self::ExprParam | Self::StmtReturn => 1,
            Self::ExprBinary => 4,
        }
    }

    fn is_expression(self) -> bool {
        matches!(self, Self::ExprInt | Self::ExprParam | Self::ExprBinary)
    }

    fn is_statement(self) -> bool {
        matches!(self, Self::StmtLetInt | Self::StmtReturn)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectedOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

impl ProjectedOp {
    const ALL: [Self; 5] = [Self::Add, Self::Sub, Self::Mul, Self::Div, Self::Rem];

    fn from_binary_op(op: BinaryOp) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|projected| projected.binary_op() == op)
            .ok_or_else(|| format!("binary operation {op:?} is outside the arithmetic projection"))
    }

    fn binary_op(self) -> BinaryOp {
        match self {
            Self::Add => BinaryOp::Add,
            Self::Sub => BinaryOp::Sub,
            Self::Mul => BinaryOp::Mul,
            Self::Div => BinaryOp::Div,
            Self::Rem => BinaryOp::Rem,
        }
    }

    fn from_name(name: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|projected| projected.name() == name)
            .ok_or_else(|| format!("unknown binary operation {name:?}"))
    }

    fn name(self) -> &'static str {
        match self {
            Self::Add => "Add",
            Self::Sub => "Sub",
            Self::Mul => "Mul",
            Self::Div => "Div",
            Self::Rem => "Rem",
        }
    }

    fn ordinal(self) -> u32 {
        self.binary_op() as u32
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProjectedRow {
    id: IrId,
    tag: ProjectedTag,
    lhs: u32,
    rhs: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProjectedBlock {
    id: IrId,
    start: u32,
    len: u32,
    result: OptionalIrId,
    owner: OptionalIrId,
    flags: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProjectedFunction {
    id: IrId,
    name: String,
    params_start: u32,
    params_len: u32,
    captures_start: u32,
    captures_len: u32,
    body: IrId,
    slot_count: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Observation {
    function: IrId,
    result_bits: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ArithmeticDocument {
    source_id: IrId,
    rows: Vec<ProjectedRow>,
    extra: Vec<u32>,
    binary_ops: Vec<ProjectedOp>,
    blocks: Vec<ProjectedBlock>,
    functions: Vec<ProjectedFunction>,
    observations: Vec<Observation>,
}

impl ArithmeticDocument {
    fn payload(&self, row: &ProjectedRow) -> Result<&[u32], String> {
        let end = (row.lhs as usize)
            .checked_add(row.rhs as usize)
            .ok_or_else(|| "row payload range overflows".to_string())?;
        self.extra
            .get(row.lhs as usize..end)
            .ok_or_else(|| "row payload range is outside the extra table".to_string())
    }

    fn row_before(&self, operand: u32, index: usize) -> Result<&ProjectedRow, String> {
        let operand = operand as usize;
        if operand >= index {
            return Err("operand row does not precede its user".to_string());
        }
        Ok(&self.rows[operand])
    }
}

fn validate_arithmetic_document(document: &ArithmeticDocument) -> Result<(), String> {
    for (index, row) in document.rows.iter().enumerate() {
        if row.id != IrId::from_index(index)? {
            return Err("instruction IDs must follow table order".to_string());
        }
    }
    for (index, row) in document.rows.iter().enumerate() {
        let words = document.payload(row)?;
        if words.len() != row.tag.payload_words() {
            return Err(format!("{} payload has the wrong word count", row.tag.name()));
        }
        match row.tag {
            ProjectedTag::IntInt | ProjectedTag::ExprInt | ProjectedTag::ExprParam => {}
            ProjectedTag::StmtLetInt => {
                if document.row_before(words[1], index)?.tag != ProjectedTag::IntInt {
                    return Err("StmtLetInt value is not an IntInt row".to_string());
                }
            }
            ProjectedTag::ExprBinary => {
                if words[0] as usize >= document.binary_ops.len() {
                    return Err("binary operation index is outside the operation pool".to_string());
                }
                for operand in [words[1], words[2]] {
                    if !document.row_before(operand, index)?.tag.is_expression() {
                        return Err("binary operand is not an expression row".to_string());
                    }
                }
            }
            ProjectedTag::StmtReturn => {
                if !document.row_before(words[0], index)?.tag.is_expression() {
                    return Err("return value is not an expression row".to_string());
                }
            }
        }
    }

    for (index, block) in document.blocks.iter().enumerate() {
        if block.id != IrId::from_index(index)? {
            return Err("block IDs must follow table order".to_string());
        }
        let end = (block.start as usize)
            .checked_add(block.len as usize)
            .ok_or_else(|| "block range overflows".to_string())?;
        let words = document
            .extra
            .get(block.start as usize..end)
            .ok_or_else(|| "block range is outside the extra table".to_string())?;
        let Some((count, statements)) = words.split_first() else {
            return Err("block range has no statement count".to_string());
        };
        if *count as usize != statements.len() {
            return Err("block statement count disagrees with its range".to_string());
        }
        for statement in statements {
            let row = document
                .rows
                .get(*statement as usize)
                .ok_or_else(|| "block statement is outside the instruction table".to_string())?;
            if !row.tag.is_statement() {
                return Err("block entry is not a statement row".to_string());
            }
        }
        let owner = block
            .owner
            .0
            .ok_or_else(|| "block has no owning function".to_string())?;
        let function = document
            .functions
            .get(owner.raw() as usize - 1)
            .ok_or_else(|| "block owner is outside the function table".to_string())?;
        if function.body != block.id {
            return Err("block owner does not name the block as its body".to_string());
        }
    }

    for (index, function) in document.functions.iter().enumerate() {
        if function.id != IrId::from_index(index)? {
            return Err("function IDs must follow table order".to_string());
        }
        if function.name.is_empty() {
            return Err("function name is empty".to_string());
        }
        if function.body.raw() as usize > document.blocks.len() {
            return Err("function body is outside the block table".to_string());
        }
    }

    for observation in &document.observations {
        if observation.function.raw() as usize > document.functions.len() {
            return Err("observation names a function outside the function table".to_string());
        }
    }
    Ok(())
}

fn hex_flag(flags: u8) -> String {
    hex_bytes(&[flags])
}

fn parse_flag(text: &str) -> Result<u8, String> {
    match parse_hex_bytes(text)?.as_slice() {
        [flags] => Ok(*flags),
        _ => Err("block flags require exactly one byte".to_string()),
    }
}

fn hex_array(words: &[u32]) -> WireValue {
    WireValue::Array(words.iter().map(|word| wire_string(hex_u32(*word))).collect())
}

fn encode_arithmetic_document(document: &ArithmeticDocument) -> Result<String, String> {
    validate_arithmetic_document(document)?;
    let value = wire_object([
        ("schema", wire_string(RAW_IR_SCHEMA)),
        ("sourceId", wire_string(hex_u32(document.source_id.raw()))),
        ("scope", wire_string(ARITHMETIC_SCOPE)),
        ("instructions", WireValue::Array(document.rows.iter().map(|row| wire_object([
            ("id", wire_string(hex_u32(row.id.raw()))),
            ("tag", wire_string(row.tag.name())),
            ("ordinal", wire_string(hex_u32(row.tag.ordinal()))),
            ("lhs", wire_string(hex_u32(row.lhs))),
            ("rhs", wire_string(hex_u32(row.rhs))),
        ])).collect())),
        ("extra", hex_array(&document.extra)),
        ("binaryOps", WireValue::Array(document.binary_ops.iter().map(|op| wire_object([
            ("name", wire_string(op.name())),
            ("ordinal", wire_string(hex_u32(op.ordinal()))),
        ])).collect())),
        ("blocks", WireValue::Array(document.blocks.iter().map(|block| wire_object([
            ("id", wire_string(hex_u32(block.id.raw()))),
            ("start", wire_string(hex_u32(block.start))),
            ("len", wire_string(hex_u32(block.len))),
            ("result", wire_string(hex_u32(block.result.raw()))),
            ("owner", wire_string(hex_u32(block.owner.raw()))),
            ("flags", wire_string(hex_flag(block.flags))),
        ])).collect())),
        ("functions", WireValue::Array(document.functions.iter().map(|function| wire_object([
            ("id", wire_string(hex_u32(function.id.raw()))),
            ("name", wire_string(&function.name)),
            ("paramsStart", wire_string(hex_u32(function.params_start))),
            ("paramsLen", wire_string(hex_u32(function.params_len))),
            ("capturesStart", wire_string(hex_u32(function.captures_start))),
            ("capturesLen", wire_string(hex_u32(function.captures_len))),
            ("body", wire_string(hex_u32(function.body.raw()))),
            ("slotCount", wire_string(hex_u32(function.slot_count))),
        ])).collect())),
        ("observations", WireValue::Array(document.observations.iter().map(|observation| wire_object([
            ("function", wire_string(hex_u32(observation.function.raw()))),
            ("result", wire_string(hex_u64(observation.result_bits))),
        ])).collect())),
    ]);
    deser_json::to_string(&value).map_err(|error| error.to_string())
}

fn take_hex_u32(fields: &mut BTreeMap<String, WireValue>, name: &str) -> Result<u32, String> {
    parse_hex_u32(&take_string(fields, name)?)
}

fn take_id(fields: &mut BTreeMap<String, WireValue>, name: &str) -> Result<IrId, String> {
    IrId::from_raw(take_hex_u32(fields, name)?)
}

fn take_optional_id(
    fields: &mut BTreeMap<String, WireValue>,
    name: &str,
) -> Result<OptionalIrId, String> {
    OptionalIrId::from_raw(take_hex_u32(fields, name)?)
}

fn decode_arithmetic_document(input: &str) -> Result<ArithmeticDocument, String> {
    let mut fields = parse_object_fields(
        parse_wire_document(input)?,
        &[
            "schema",
            "sourceId",
            "scope",
            "instructions",
            "extra",
            "binaryOps",
            "blocks",
            "functions",
            "observations",
        ],
    )?;
    if take_string(&mut fields, "schema")? != RAW_IR_SCHEMA {
        return Err("unknown raw-IR document version".to_string());
    }
    let source_id = take_id(&mut fields, "sourceId")?;
    if take_string(&mut fields, "scope")? != ARITHMETIC_SCOPE {
        return Err("unsupported raw-IR projection scope".to_string());
    }
    let rows = take_array(&mut fields, "instructions")?
        .into_iter()
        .map(|value| {
            let mut row = parse_object_fields(value, &["id", "tag", "ordinal", "lhs", "rhs"])?;
            let id = take_id(&mut row, "id")?;
            let tag = ProjectedTag::from_name(&take_string(&mut row, "tag")?)?;
            let ordinal = take_hex_u32(&mut row, "ordinal")?;
            if ordinal != tag.ordinal() {
                return Err(format!(
                    "raw-IR tag {} has ordinal {ordinal:08x}, expected {:08x}",
                    tag.name(),
                    tag.ordinal()
                ));
            }
            Ok(ProjectedRow {
                id,
                tag,
                lhs: take_hex_u32(&mut row, "lhs")?,
                rhs: take_hex_u32(&mut row, "rhs")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let extra = take_array(&mut fields, "extra")?
        .into_iter()
        .map(|value| match value {
            WireValue::String(text) => parse_hex_u32(&text),
            _ => Err("extra word is not a hex string".to_string()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let binary_ops = take_array(&mut fields, "binaryOps")?
        .into_iter()
        .map(|value| {
            let mut row = parse_object_fields(value, &["name", "ordinal"])?;
            let op = ProjectedOp::from_name(&take_string(&mut row, "name")?)?;
            let ordinal = take_hex_u32(&mut row, "ordinal")?;
            if ordinal != op.ordinal() {
                return Err(format!(
                    "binary operation {} has ordinal {ordinal:08x}, expected {:08x}",
                    op.name(),
                    op.ordinal()
                ));
            }
            Ok(op)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let blocks = take_array(&mut fields, "blocks")?
        .into_iter()
        .map(|value| {
            let mut row =
                parse_object_fields(value, &["id", "start", "len", "result", "owner", "flags"])?;
            Ok(ProjectedBlock {
                id: take_id(&mut row, "id")?,
                start: take_hex_u32(&mut row, "start")?,
                len: take_hex_u32(&mut row, "len")?,
                result: take_optional_id(&mut row, "result")?,
                owner: take_optional_id(&mut row, "owner")?,
                flags: parse_flag(&take_string(&mut row, "flags")?)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let functions = take_array(&mut fields, "functions")?
        .into_iter()
        .map(|value| {
            let mut row = parse_object_fields(
                value,
                &[
                    "id",
                    "name",
                    "paramsStart",
                    "paramsLen",
                    "capturesStart",
                    "capturesLen",
                    "body",
                    "slotCount",
                ],
            )?;
            Ok(ProjectedFunction {
                id: take_id(&mut row, "id")?,
                name: take_string(&mut row, "name")?,
                params_start: take_hex_u32(&mut row, "paramsStart")?,
                params_len: take_hex_u32(&mut row, "paramsLen")?,
                captures_start: take_hex_u32(&mut row, "capturesStart")?,
                captures_len: take_hex_u32(&mut row, "capturesLen")?,
                body: take_id(&mut row, "body")?,
                slot_count: take_hex_u32(&mut row, "slotCount")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let observations = take_array(&mut fields, "observations")?
        .into_iter()
        .map(|value| {
            let mut row = parse_object_fields(value, &["function", "result"])?;
            Ok(Observation {
                function: take_id(&mut row, "function")?,
                result_bits: parse_hex_u64(&take_string(&mut row, "result")?)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let document = ArithmeticDocument {
        source_id,
        rows,
        extra,
        binary_ops,
        blocks,
        functions,
        observations,
    };
    validate_arithmetic_document(&document)?;
    Ok(document)
}

fn call_integer_function(
    program: &FullProgram,
    sources: &SourceMap,
    name: &str,
) -> Result<u64, String> {
    use crate::runtime::eval::{CallableKind, LoweredFunctionKey};
    use crate::runtime::value::Value;
    let mut evaluator =
        crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources.clone());
    evaluator.indexed_program = Some(Arc::new(program.clone()));
    let key = LoweredFunctionKey::Name(program.symbol_owner().with_current(|| Name::intern(name)));
    match evaluator
        .call_indexed_direct(
            key,
            CallableKind::Pure,
            &[],
            Span::new(program.store.source_id, 0, 0),
        )
        .ok_or_else(|| format!("function {name:?} is absent"))?
        .map_err(|error| format!("function {name:?} failed: {error:?}"))?
    {
        Value::Int(value) => Ok(value as u64),
        other => Err(format!("function {name:?} returned {other:?}")),
    }
}

fn execute_schedule(
    program: &FullProgram,
    sources: &SourceMap,
    schedule: &[&str],
) -> Result<Vec<Observation>, String> {
    schedule
        .iter()
        .map(|name| {
            let index = program
                .store
                .functions
                .iter()
                .position(|function| {
                    program.store.string(function.name).is_ok_and(|text| text == *name)
                })
                .ok_or_else(|| format!("scheduled function {name:?} is absent"))?;
            Ok(Observation {
                function: IrId::from_index(index)?,
                result_bits: call_integer_function(program, sources, name)?,
            })
        })
        .collect()
}

fn export_arithmetic_document(
    program: &FullProgram,
    arena: &ArenaProgram,
    sources: &SourceMap,
    source_id: IrId,
    schedule: &[&str],
) -> Result<ArithmeticDocument, String> {
    FullVerifier::verify(program).map_err(|error| error.message.clone())?;
    if source_id.raw().checked_sub(1) != Some(program.store.source_id.raw() as u32) {
        return Err("raw-IR document does not name its source document".to_string());
    }
    let store = &program.store;
    let syntax_names = arena
        .arena
        .function_defs
        .iter()
        .map(|function| name_text(function.name))
        .collect::<Vec<_>>();
    let lowered_names = store
        .functions
        .iter()
        .map(|function| store.string(function.name).map(str::to_string))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.message.clone())?;
    if syntax_names != lowered_names {
        return Err("arena function names do not match the lowered function table".to_string());
    }

    let mut rows = Vec::new();
    for (index, tag) in store.tags.iter().copied().enumerate() {
        let tag = ProjectedTag::from_full_tag(tag).ok_or_else(|| {
            format!("instruction tag {tag:?} is outside the arithmetic projection")
        })?;
        let data = store
            .data
            .get(index)
            .ok_or_else(|| "instruction has no data row".to_string())?;
        rows.push(ProjectedRow {
            id: IrId::from_index(index)?,
            tag,
            lhs: data.lhs,
            rhs: data.rhs,
        });
    }
    let binary_ops = store
        .binary_ops
        .iter()
        .copied()
        .map(ProjectedOp::from_binary_op)
        .collect::<Result<Vec<_>, _>>()?;
    let blocks = store
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            Ok(ProjectedBlock {
                id: IrId::from_index(index)?,
                start: block.instructions.start,
                len: block.instructions.len,
                result: OptionalIrId::from_raw(block.result)?,
                owner: OptionalIrId::from_raw(block.owner)?,
                flags: block.flags,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let functions = store
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| {
            Ok(ProjectedFunction {
                id: IrId::from_index(index)?,
                name: store
                    .string(function.name)
                    .map(str::to_string)
                    .map_err(|error| error.message.clone())?,
                params_start: function.params.start,
                params_len: function.params.len,
                captures_start: function.captures.start,
                captures_len: function.captures.len,
                body: IrId::from_raw(function.body)?,
                slot_count: function.slot_count,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let document = ArithmeticDocument {
        source_id,
        rows,
        extra: store.extra.clone(),
        binary_ops,
        blocks,
        functions,
        observations: execute_schedule(program, sources, schedule)?,
    };
    validate_arithmetic_document(&document)?;
    Ok(document)
}

fn check_artifact_against_program(
    recorded: &ArithmeticDocument,
    program: &FullProgram,
    arena: &ArenaProgram,
    sources: &SourceMap,
    schedule: &[&str],
) -> Result<(), String> {
    let reproduced = export_arithmetic_document(program, arena, sources, recorded.source_id, schedule)?;
    if &reproduced != recorded {
        return Err("the program no longer reproduces the recorded artifact".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEDULE: [&str; 2] = ["shifted", "scaled"];

    const FIXTURE_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/formal-bridge-integer-locals.xsh"
    ));

    const LEAN_DOCUMENTS: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/formal/Xsh/Conformance/Vertical/Documents.lean"
    ));

    fn lean_document(definition: &str) -> String {
        let header = format!("def {definition} : List Nat :=\n");
        let start = LEAN_DOCUMENTS
            .find(&header)
            .unwrap_or_else(|| panic!("the Lean documents define {definition}"))
            + header.len();
        let mut bytes = Vec::new();
        for line in LEAN_DOCUMENTS[start..].lines() {
            let last = line.ends_with(']');
            let numbers = line.trim_start_matches(['[', ' ']).trim_end_matches([',', ']']);
            for number in numbers.split(", ") {
                bytes.push(number.parse::<u8>().unwrap_or_else(|_| {
                    panic!("malformed Lean code point {number:?} in {definition}")
                }));
            }
            if last {
                return String::from_utf8(bytes).expect("Lean documents hold UTF-8 text");
            }
        }
        panic!("the Lean code list {definition} is not terminated");
    }

    const LEAN_CODES_PER_LINE: usize = 20;

    fn lean_definition(definition: &str, text: &str) -> String {
        let rows = text.as_bytes().chunks(LEAN_CODES_PER_LINE).collect::<Vec<_>>();
        let mut output = format!("def {definition} : List Nat :=\n");
        for (index, row) in rows.iter().enumerate() {
            let opener = if index == 0 { "  [" } else { "   " };
            let closer = if index + 1 == rows.len() { "]" } else { "," };
            let numbers = row.iter().map(u8::to_string).collect::<Vec<_>>().join(", ");
            output.push_str(&format!("{opener}{numbers}{closer}\n"));
        }
        output
    }

    fn assert_lean_document(definition: &str, text: &str) {
        assert_eq!(
            lean_document(definition),
            text,
            "Lean documents must hold the exported bytes; the current export is\n{}",
            lean_definition(definition, text)
        );
    }

    struct Fixture {
        source: SourceDocument,
        sources: SourceMap,
        parsed: crate::syntax::parser::ArenaParseOutput,
        program: FullProgram,
        document: ArithmeticDocument,
    }

    fn fixture() -> Fixture {
        let source = SourceDocument {
            id: source_document_id(SourceId::new(0)).unwrap(),
            text: FIXTURE_SOURCE.to_string(),
        };
        let (sources, parsed, program) = build_fixture(&source.text).unwrap();
        let document =
            export_arithmetic_document(&program, &parsed.arena, &sources, source.id, &SCHEDULE)
                .unwrap();
        Fixture { source, sources, parsed, program, document }
    }

    fn function_body_statements(fixture: &Fixture, name: &str) -> Vec<usize> {
        let function = fixture
            .document
            .functions
            .iter()
            .find(|function| function.name == name)
            .expect("fixture function");
        let block = fixture.document.blocks[function.body.raw() as usize - 1];
        fixture.document.extra[block.start as usize + 1..(block.start + block.len) as usize]
            .iter()
            .map(|id| *id as usize)
            .collect()
    }

    fn payload_start(program: &FullProgram, tag: FullTag, occurrence: usize) -> usize {
        let row = program
            .store
            .tags
            .iter()
            .enumerate()
            .filter(|(_, candidate)| **candidate == tag)
            .nth(occurrence)
            .expect("fixture row")
            .0;
        program.store.data[row].lhs as usize
    }

    #[test]
    fn exported_documents_are_the_lean_certified_bytes() {
        let fixture = fixture();
        let source_text = encode_source_document(&fixture.source);
        let raw_text = encode_arithmetic_document(&fixture.document).unwrap();
        assert_lean_document("sourceDocumentCodes", &source_text);
        assert_lean_document("rawIrDocumentCodes", &raw_text);

        assert_eq!(decode_source_document(&source_text).unwrap(), fixture.source);
        let decoded = decode_arithmetic_document(&raw_text).unwrap();
        assert_eq!(decoded, fixture.document);
        assert_eq!(encode_arithmetic_document(&decoded).unwrap(), raw_text);
        assert_eq!(decoded.source_id, fixture.source.id);

        check_artifact_against_program(
            &decoded,
            &fixture.program,
            &fixture.parsed.arena,
            &fixture.sources,
            &SCHEDULE,
        )
        .unwrap();
    }

    #[test]
    fn exported_artifact_has_an_integer_arithmetic_row_over_an_identifier_operand() {
        let fixture = fixture();
        let document = &fixture.document;
        let shifted = function_body_statements(&fixture, "shifted");
        assert_eq!(shifted.len(), 2);
        assert_eq!(document.rows[shifted[0]].tag, ProjectedTag::StmtLetInt);
        assert_eq!(document.rows[shifted[1]].tag, ProjectedTag::StmtReturn);
        let returned = document.payload(&document.rows[shifted[1]]).unwrap()[0] as usize;
        assert_eq!(document.rows[returned].tag, ProjectedTag::ExprBinary);
        let binary = document.payload(&document.rows[returned]).unwrap();
        assert_eq!(document.binary_ops[binary[0] as usize], ProjectedOp::Add);
        let operand = document.rows[binary[1] as usize];
        assert_eq!(operand.tag, ProjectedTag::ExprParam);
        let let_slot = document.payload(&document.rows[shifted[0]]).unwrap()[0];
        assert_eq!(document.payload(&operand).unwrap(), [let_slot]);
        assert_eq!(document.rows[binary[2] as usize].tag, ProjectedTag::ExprInt);
        assert_eq!(
            document.observations,
            [
                Observation { function: IrId::from_raw(1).unwrap(), result_bits: 5 },
                Observation { function: IrId::from_raw(2).unwrap(), result_bits: 42 },
            ]
        );
    }

    #[test]
    fn source_and_artifact_association_is_explicit() {
        let fixture = fixture();
        let source_text = encode_source_document(&fixture.source);
        let raw_text = encode_arithmetic_document(&fixture.document).unwrap();
        let other_source = parsed_raw_mutation(&raw_text, |value| {
            set_field(value, "sourceId", WireValue::String(hex_u32(2)));
        });
        let decoded_source = decode_source_document(&source_text).unwrap();
        let decoded_other = decode_arithmetic_document(&other_source).unwrap();
        assert_ne!(decoded_source.id, decoded_other.source_id);
        let foreign_source = SourceDocument {
            id: IrId::from_raw(2).unwrap(),
            text: fixture.source.text.clone(),
        };
        assert!(export_arithmetic_document(
            &fixture.program,
            &fixture.parsed.arena,
            &fixture.sources,
            foreign_source.id,
            &SCHEDULE,
        )
        .is_err());
        let changed_source = fixture.source.text.replace("x + 3", "x + 4");
        let (changed_sources, changed_parsed, changed_program) =
            build_fixture(&changed_source).unwrap();
        assert!(check_artifact_against_program(
            &fixture.document,
            &changed_program,
            &changed_parsed.arena,
            &changed_sources,
            &SCHEDULE,
        )
        .is_err());
    }

    #[test]
    fn wire_corruptions_are_rejected_by_the_decoder() {
        let fixture = fixture();
        let raw_text = encode_arithmetic_document(&fixture.document).unwrap();
        let reject = |name: &str, mutate: &dyn Fn(&mut WireValue)| {
            let corrupted = parsed_raw_mutation(&raw_text, |value| mutate(value));
            assert!(
                decode_arithmetic_document(&corrupted).is_err(),
                "{name} must be rejected"
            );
        };
        let binary_index = fixture
            .document
            .rows
            .iter()
            .position(|row| row.tag == ProjectedTag::ExprBinary)
            .unwrap();
        let param_index = fixture
            .document
            .rows
            .iter()
            .position(|row| row.tag == ProjectedTag::ExprParam)
            .unwrap();
        reject("unknown tag", &|value| {
            set_nested_wire(value, "instructions", param_index, "tag", "ExprUnknown")
        });
        reject("tag with another tag's ordinal", &|value| {
            set_nested_wire(value, "instructions", param_index, "ordinal", &hex_u32(33))
        });
        reject("operation with another operation's ordinal", &|value| {
            set_nested_wire(value, "binaryOps", 0, "ordinal", &hex_u32(ProjectedOp::Mul.ordinal()))
        });
        reject("unknown operation", &|value| {
            set_nested_wire(value, "binaryOps", 0, "name", "Pow")
        });
        reject("instruction gap", &|value| {
            set_nested_wire(value, "instructions", param_index, "id", &hex_u32(99))
        });
        reject("payload range outside extra", &|value| {
            set_nested_wire(value, "instructions", binary_index, "rhs", &hex_u32(99))
        });
        reject("payload of the wrong width", &|value| {
            set_nested_wire(value, "instructions", binary_index, "rhs", &hex_u32(3))
        });
        reject("block count disagrees with range", &|value| {
            set_nested_wire(value, "blocks", 0, "len", &hex_u32(2))
        });
        reject("block owner is not the function", &|value| {
            set_nested_wire(value, "blocks", 0, "owner", &hex_u32(2))
        });
        reject("function body is outside the block table", &|value| {
            set_nested_wire(value, "functions", 0, "body", &hex_u32(9))
        });
        reject("observation names an absent function", &|value| {
            set_nested_wire(value, "observations", 0, "function", &hex_u32(9))
        });
        reject("unknown field", &|value| {
            insert_field(value, "unrecognized", WireValue::String("rejected".to_string()))
        });
        reject("unknown scope", &|value| {
            set_field(value, "scope", WireValue::String("scalar-literal-boundary-fixture-v1".to_string()))
        });
        reject("uppercase hex word", &|value| {
            set_nested_wire(value, "instructions", binary_index, "lhs", "0000000A")
        });
    }

    fn set_nested_wire(value: &mut WireValue, list: &str, row: usize, field: &str, text: &str) {
        let WireValue::Array(rows) = object_field(value, list) else {
            panic!("test mutation field is an array");
        };
        set_field(&mut rows[row], field, WireValue::String(text.to_string()));
    }

    #[test]
    fn tampered_observations_are_rejected_against_the_scheduled_run() {
        let fixture = fixture();
        let check = |document: &ArithmeticDocument| {
            check_artifact_against_program(
                document,
                &fixture.program,
                &fixture.parsed.arena,
                &fixture.sources,
                &SCHEDULE,
            )
        };
        assert!(check(&fixture.document).is_ok());

        let mut missing = fixture.document.clone();
        missing.observations.pop();
        assert!(validate_arithmetic_document(&missing).is_ok());
        assert!(check(&missing).is_err());

        let mut reordered = fixture.document.clone();
        reordered.observations.reverse();
        assert!(validate_arithmetic_document(&reordered).is_ok());
        assert!(check(&reordered).is_err());

        let mut wrong_result = fixture.document.clone();
        wrong_result.observations[0].result_bits ^= 1;
        assert!(validate_arithmetic_document(&wrong_result).is_ok());
        assert!(check(&wrong_result).is_err());

        let mut duplicated = fixture.document.clone();
        duplicated.observations.push(duplicated.observations[0]);
        assert!(check(&duplicated).is_err());
    }

    #[test]
    fn mutated_programs_are_rejected_by_the_verifier_or_the_recorded_run() {
        let fixture = fixture();
        let check = |program: &FullProgram| {
            check_artifact_against_program(
                &fixture.document,
                program,
                &fixture.parsed.arena,
                &fixture.sources,
                &SCHEDULE,
            )
        };

        let mut wrong_operation = fixture.program.clone();
        let add = wrong_operation
            .store
            .binary_ops
            .iter()
            .position(|op| *op == BinaryOp::Add)
            .unwrap();
        wrong_operation.store.binary_ops[add] = BinaryOp::Mul;
        FullVerifier::verify(&wrong_operation).expect("the multiplication is still well typed");
        assert_eq!(
            execute_schedule(&wrong_operation, &fixture.sources, &SCHEDULE).unwrap()[0].result_bits,
            6
        );
        assert!(check(&wrong_operation).is_err());

        let mut binding_outside_frame = fixture.program.clone();
        let param = payload_start(&binding_outside_frame, FullTag::ExprParam, 0);
        binding_outside_frame.store.extra[param] = binding_outside_frame.store.functions[0].slot_count;
        assert!(FullVerifier::verify(&binding_outside_frame).is_err());
        assert!(check(&binding_outside_frame).is_err());

        let mut binding_to_other_slot = fixture.program.clone();
        let param = payload_start(&binding_to_other_slot, FullTag::ExprParam, 0);
        binding_to_other_slot.store.extra[param] ^= 1;
        assert!(check(&binding_to_other_slot).is_err());

        let mut operand_outside_table = fixture.program.clone();
        let binary = payload_start(&operand_outside_table, FullTag::ExprBinary, 0);
        operand_outside_table.store.extra[binary + 1] = operand_outside_table.store.tags.len() as u32;
        assert!(FullVerifier::verify(&operand_outside_table).is_err());
        assert!(check(&operand_outside_table).is_err());

        let mut operand_to_literal = fixture.program.clone();
        let binary = payload_start(&operand_to_literal, FullTag::ExprBinary, 0);
        let literal_row = operand_to_literal
            .store
            .tags
            .iter()
            .position(|tag| *tag == FullTag::ExprInt)
            .unwrap() as u32;
        operand_to_literal.store.extra[binary + 1] = literal_row;
        assert!(check(&operand_to_literal).is_err());
    }
}
