use super::*;
use crate::runtime::value::ResultValue;
use crate::syntax::node::BinaryOp;

const ARITHMETIC_SCOPE: &str = "integer-calls-conditional-local-fixture-v3";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectedTag {
    IntInt,
    StmtLetInt,
    ExprInt,
    ExprParam,
    ExprBinary,
    StmtReturn,
    StmtValue,
    ExprValueBlock,
    ExprIf,
    ExprDirectPureCall,
    IntSlot,
    IntBinary,
    StmtAssignInt,
    ExprOk,
    ExprErr,
    ExprTry,
    StmtLet,
}

impl ProjectedTag {
    const ALL: [Self; 17] = [
        Self::IntInt,
        Self::StmtLetInt,
        Self::ExprInt,
        Self::ExprParam,
        Self::ExprBinary,
        Self::StmtReturn,
        Self::StmtValue,
        Self::ExprValueBlock,
        Self::ExprIf,
        Self::ExprDirectPureCall,
        Self::IntSlot,
        Self::IntBinary,
        Self::StmtAssignInt,
        Self::ExprOk,
        Self::ExprErr,
        Self::ExprTry,
        Self::StmtLet,
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
            Self::StmtValue => FullTag::StmtValue,
            Self::ExprValueBlock => FullTag::ExprValueBlock,
            Self::ExprIf => FullTag::ExprIf,
            Self::ExprDirectPureCall => FullTag::ExprDirectPureCall,
            Self::IntSlot => FullTag::IntSlot,
            Self::IntBinary => FullTag::IntBinary,
            Self::StmtAssignInt => FullTag::StmtAssignInt,
            Self::ExprOk => FullTag::ExprOk,
            Self::ExprErr => FullTag::ExprErr,
            Self::ExprTry => FullTag::ExprTry,
            Self::StmtLet => FullTag::StmtLet,
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
            Self::IntInt | Self::StmtLetInt | Self::ExprInt | Self::ExprValueBlock => 2,
            Self::ExprParam | Self::StmtReturn | Self::StmtValue | Self::IntSlot => 1,
            Self::ExprIf | Self::ExprDirectPureCall | Self::IntBinary => 3,
            Self::ExprBinary | Self::StmtAssignInt => 4,
            Self::ExprOk => 1,
            Self::ExprErr => 2,
            Self::ExprTry => 2,
            Self::StmtLet => 3,
        }
    }

    fn is_expression(self) -> bool {
        matches!(
            self,
            Self::ExprInt
                | Self::ExprParam
                | Self::ExprBinary
                | Self::ExprValueBlock
                | Self::ExprIf
                | Self::ExprDirectPureCall
        )
    }

    fn is_result(self) -> bool {
        matches!(self, Self::ExprOk | Self::ExprErr)
    }

    fn is_propagation(self) -> bool {
        matches!(self, Self::ExprTry)
    }

    fn is_int_row(self) -> bool {
        matches!(self, Self::IntInt | Self::IntSlot | Self::IntBinary)
    }

    fn is_statement(self) -> bool {
        matches!(
            self,
            Self::StmtLetInt | Self::StmtReturn | Self::StmtValue | Self::StmtAssignInt
                | Self::StmtLet
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProjectedOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Gt,
    Lt,
}

impl ProjectedOp {
    const ALL: [Self; 7] = [Self::Add, Self::Sub, Self::Mul, Self::Div, Self::Rem, Self::Gt, Self::Lt];

    pub(super) fn from_binary_op(op: BinaryOp) -> Result<Self, String> {
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
            Self::Gt => BinaryOp::Gt,
            Self::Lt => BinaryOp::Lt,
        }
    }

    pub(super) fn from_name(name: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|projected| projected.name() == name)
            .ok_or_else(|| format!("unknown binary operation {name:?}"))
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Add => "Add",
            Self::Sub => "Sub",
            Self::Mul => "Mul",
            Self::Div => "Div",
            Self::Rem => "Rem",
            Self::Gt => "Gt",
            Self::Lt => "Lt",
        }
    }

    pub(super) fn ordinal(self) -> u32 {
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
enum ProjectedFailure {
    DivisionByZero,
    IntegerOverflow,
}

impl ProjectedFailure {
    const ALL: [Self; 2] = [Self::DivisionByZero, Self::IntegerOverflow];

    fn name(self) -> &'static str {
        match self {
            Self::DivisionByZero => "division-by-zero",
            Self::IntegerOverflow => "integer-overflow",
        }
    }

    fn from_name(name: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|failure| failure.name() == name)
            .ok_or_else(|| format!("unknown specified failure {name:?}"))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Outcome {
    Value(u64),
    Failure(ProjectedFailure),
    Ok(u64),
    Err(u64),
}

impl Outcome {
    fn name(self) -> &'static str {
        match self {
            Self::Value(_) => "value",
            Self::Failure(_) => "failure",
            Self::Ok(_) => "ok",
            Self::Err(_) => "err",
        }
    }

    fn detail(self) -> String {
        match self {
            Self::Value(bits) | Self::Ok(bits) | Self::Err(bits) => hex_u64(bits),
            Self::Failure(failure) => failure.name().to_string(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Observation {
    function: IrId,
    arguments: Vec<u64>,
    outcome: Outcome,
}

#[derive(Clone, Copy, Debug)]
struct ScheduledCall {
    function: &'static str,
    arguments: &'static [i64],
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BlockRole {
    FunctionBody(usize),
    ValueBlock(usize),
    Branches(usize),
    Arguments(usize),
}

fn block_index(document: &ArithmeticDocument, raw: u32) -> Result<usize, String> {
    let index = (raw as usize)
        .checked_sub(1)
        .filter(|index| *index < document.blocks.len())
        .ok_or_else(|| "row names a block outside the block table".to_string())?;
    Ok(index)
}

fn claim_block(
    document: &ArithmeticDocument,
    roles: &mut [Option<BlockRole>],
    raw: u32,
    role: BlockRole,
) -> Result<(), String> {
    let index = block_index(document, raw)?;
    if roles[index].replace(role).is_some() {
        return Err("a block is referenced by more than one user".to_string());
    }
    Ok(())
}

fn block_words<'a>(
    document: &'a ArithmeticDocument,
    block: &ProjectedBlock,
) -> Result<&'a [u32], String> {
    let end = (block.start as usize)
        .checked_add(block.len as usize)
        .ok_or_else(|| "block range overflows".to_string())?;
    document
        .extra
        .get(block.start as usize..end)
        .ok_or_else(|| "block range is outside the extra table".to_string())
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
                if !document.row_before(words[1], index)?.tag.is_int_row() {
                    return Err("StmtLetInt value is not an integer row".to_string());
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
            ProjectedTag::IntSlot => {}
            ProjectedTag::IntBinary => {
                if words[0] as usize >= document.binary_ops.len() {
                    return Err("integer operation index is outside the operation pool".to_string());
                }
                for operand in [words[1], words[2]] {
                    if !document.row_before(operand, index)?.tag.is_int_row() {
                        return Err("integer operand is not an integer row".to_string());
                    }
                }
            }
            ProjectedTag::StmtAssignInt => {
                if words[1] != 0 {
                    return Err("an integer assignment is not a plain assignment".to_string());
                }
                if !document.row_before(words[2], index)?.tag.is_int_row() {
                    return Err("assigned value is not an integer row".to_string());
                }
            }
            ProjectedTag::StmtReturn | ProjectedTag::StmtValue => {
                let operand = document.row_before(words[0], index)?.tag;
                if !operand.is_expression() && !operand.is_result() {
                    return Err("statement value is not an expression row".to_string());
                }
            }
            ProjectedTag::ExprValueBlock => {}
            ProjectedTag::ExprIf => {
                if !document.row_before(words[1], index)?.tag.is_expression() {
                    return Err("conditional else value is not an expression row".to_string());
                }
            }
            ProjectedTag::ExprDirectPureCall => {
                if (words[0] as usize).checked_sub(1).is_none_or(|callee| callee >= document.functions.len()) {
                    return Err("call names a function outside the function table".to_string());
                }
            }
            ProjectedTag::ExprOk => {
                if !document.row_before(words[0], index)?.tag.is_expression() {
                    return Err("Ok payload is not an expression row".to_string());
                }
            }
            ProjectedTag::ExprErr => {
                if !document.row_before(words[0], index)?.tag.is_expression() {
                    return Err("Err payload is not an expression row".to_string());
                }
                if words[1] != 0 {
                    return Err("Err cause word is not the absent marker".to_string());
                }
            }
            ProjectedTag::ExprTry => {
                if document.row_before(words[0], index)?.tag != ProjectedTag::ExprDirectPureCall {
                    return Err("propagated value is not a direct call".to_string());
                }
            }
            ProjectedTag::StmtLet => {
                if words[2] != 0 {
                    return Err("propagation binding carries a type annotation".to_string());
                }
                if !document.row_before(words[1], index)?.tag.is_propagation() {
                    return Err("bound value is not a propagation".to_string());
                }
            }
        }
    }

    let mut roles = vec![None; document.blocks.len()];
    for (function_index, function) in document.functions.iter().enumerate() {
        claim_block(document, &mut roles, function.body.raw(), BlockRole::FunctionBody(function_index))?;
    }
    for (index, row) in document.rows.iter().enumerate() {
        let words = document.payload(row)?;
        match row.tag {
            ProjectedTag::ExprValueBlock => {
                claim_block(document, &mut roles, words[0], BlockRole::ValueBlock(index))?
            }
            ProjectedTag::ExprIf => {
                claim_block(document, &mut roles, words[0], BlockRole::Branches(index))?
            }
            ProjectedTag::ExprDirectPureCall => {
                claim_block(document, &mut roles, words[1], BlockRole::Arguments(index))?
            }
            _ => {}
        }
    }

    for (index, block) in document.blocks.iter().enumerate() {
        if block.id != IrId::from_index(index)? {
            return Err("block IDs must follow table order".to_string());
        }
        let owner = block
            .owner
            .0
            .ok_or_else(|| "block has no owning function".to_string())?;
        let owner_index = (owner.raw() as usize)
            .checked_sub(1)
            .filter(|owner_index| *owner_index < document.functions.len())
            .ok_or_else(|| "block owner is outside the function table".to_string())?;
        let role = roles[index].ok_or_else(|| "block is referenced by no user".to_string())?;
        let words = block_words(document, block)?;
        let Some((count, entries)) = words.split_first() else {
            return Err("block range has no entry count".to_string());
        };
        let count = *count as usize;
        match role {
            BlockRole::FunctionBody(function_index) => {
                if function_index != owner_index {
                    return Err("block owner does not name the block as its body".to_string());
                }
                if count != entries.len() {
                    return Err("block statement count disagrees with its range".to_string());
                }
                for statement in entries {
                    let row = document
                        .rows
                        .get(*statement as usize)
                        .ok_or_else(|| "block statement is outside the instruction table".to_string())?;
                    if !row.tag.is_statement() {
                        return Err("block entry is not a statement row".to_string());
                    }
                }
            }
            BlockRole::ValueBlock(user) => {
                if count != 1 || entries.len() != 1 {
                    return Err("a value block holds exactly one statement".to_string());
                }
                if document.row_before(entries[0], user)?.tag != ProjectedTag::StmtValue {
                    return Err("a value block entry is not a StmtValue row".to_string());
                }
            }
            BlockRole::Branches(user) => {
                if count == 0 || entries.len() != 2 * count {
                    return Err("branch table count disagrees with its range".to_string());
                }
                for pair in entries.chunks(2) {
                    for operand in pair {
                        if !document.row_before(*operand, user)?.tag.is_expression() {
                            return Err("branch table entry is not an expression row".to_string());
                        }
                    }
                }
            }
            BlockRole::Arguments(user) => {
                if entries.len() != 2 * count {
                    return Err("argument table count disagrees with its range".to_string());
                }
                let callee = document.payload(&document.rows[user])?[0] as usize - 1;
                if count != document.functions[callee].params_len as usize {
                    return Err("call argument count disagrees with the callee parameters".to_string());
                }
                for pair in entries.chunks(2) {
                    if pair[0] != 0 {
                        return Err("call argument is not a single value".to_string());
                    }
                    if !document.row_before(pair[1], user)?.tag.is_expression() {
                        return Err("call argument is not an expression row".to_string());
                    }
                }
            }
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
        let function = (observation.function.raw() as usize)
            .checked_sub(1)
            .and_then(|index| document.functions.get(index))
            .ok_or_else(|| "observation names a function outside the function table".to_string())?;
        if observation.arguments.len() != function.params_len as usize {
            return Err("observation arguments disagree with the function parameters".to_string());
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
            ("arguments", WireValue::Array(observation.arguments.iter().map(|argument| wire_string(hex_u64(*argument))).collect())),
            ("outcome", wire_string(observation.outcome.name())),
            ("detail", wire_string(&observation.outcome.detail())),
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
            let mut row =
                parse_object_fields(value, &["function", "arguments", "outcome", "detail"])?;
            let function = take_id(&mut row, "function")?;
            let arguments = take_array(&mut row, "arguments")?
                .into_iter()
                .map(|value| match value {
                    WireValue::String(text) => parse_hex_u64(&text),
                    _ => Err("observation argument is not a hex string".to_string()),
                })
                .collect::<Result<Vec<_>, _>>()?;
            let outcome = take_string(&mut row, "outcome")?;
            let detail = take_string(&mut row, "detail")?;
            let outcome = match outcome.as_str() {
                "value" => Outcome::Value(parse_hex_u64(&detail)?),
                "ok" => Outcome::Ok(parse_hex_u64(&detail)?),
                "err" => Outcome::Err(parse_hex_u64(&detail)?),
                "failure" => Outcome::Failure(ProjectedFailure::from_name(&detail)?),
                other => return Err(format!("unknown observation outcome {other:?}")),
            };
            Ok(Observation { function, arguments, outcome })
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

fn call_projected_function(
    program: &FullProgram,
    sources: &SourceMap,
    name: &str,
    arguments: &[i64],
) -> Result<Outcome, String> {
    use crate::runtime::eval::{CallableKind, LoweredFunctionKey};
    use crate::runtime::value::Value;
    let mut evaluator =
        crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources.clone());
    evaluator.indexed_program = Some(Arc::new(program.clone()));
    let key = LoweredFunctionKey::Name(program.symbol_owner().with_current(|| Name::intern(name)));
    let arguments = arguments.iter().copied().map(Value::Int).collect::<Vec<_>>();
    let call = evaluator
        .call_indexed_direct(
            key,
            CallableKind::Pure,
            &arguments,
            Span::new(program.store.source_id, 0, 0),
        )
        .ok_or_else(|| format!("function {name:?} is absent"))?;
    match call {
        Ok(Value::Int(value)) => Ok(Outcome::Value(value as u64)),
        Ok(Value::Bool(value)) => Ok(Outcome::Value(u64::from(value))),
        Ok(Value::Result(ResultValue::Ok(value))) => match value.as_ref() {
            Value::Int(inner) => Ok(Outcome::Ok(*inner as u64)),
            other => Err(format!("function {name:?} returned Ok of {other:?}")),
        },
        Ok(Value::Result(ResultValue::Err(value))) => match value.as_ref() {
            Value::Int(inner) => Ok(Outcome::Err(*inner as u64)),
            other => Err(format!("function {name:?} returned Err of {other:?}")),
        },
        Ok(other) => Err(format!("function {name:?} returned {other:?}")),
        Err(error) => ProjectedFailure::from_name(&error.kind)
            .map(Outcome::Failure)
            .map_err(|_| format!("function {name:?} failed outside the projection: {error:?}")),
    }
}

fn execute_schedule(
    program: &FullProgram,
    sources: &SourceMap,
    schedule: &[ScheduledCall],
) -> Result<Vec<Observation>, String> {
    schedule
        .iter()
        .map(|call| {
            let index = program
                .store
                .functions
                .iter()
                .position(|function| {
                    program.store.string(function.name).is_ok_and(|text| text == call.function)
                })
                .ok_or_else(|| format!("scheduled function {:?} is absent", call.function))?;
            Ok(Observation {
                function: IrId::from_index(index)?,
                arguments: call.arguments.iter().map(|argument| *argument as u64).collect(),
                outcome: call_projected_function(program, sources, call.function, call.arguments)?,
            })
        })
        .collect()
}

fn export_arithmetic_document(
    program: &FullProgram,
    arena: &ArenaProgram,
    sources: &SourceMap,
    source_id: IrId,
    schedule: &[ScheduledCall],
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
    schedule: &[ScheduledCall],
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

    const SCHEDULE: [ScheduledCall; 7] = [
        ScheduledCall { function: "shifted", arguments: &[] },
        ScheduledCall { function: "scaled", arguments: &[] },
        ScheduledCall { function: "divided", arguments: &[] },
        ScheduledCall { function: "overflowed", arguments: &[] },
        ScheduledCall { function: "excess", arguments: &[12, 9] },
        ScheduledCall { function: "excess", arguments: &[3, 9] },
        ScheduledCall { function: "surplus", arguments: &[] },
    ];

    const LOCALS_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/formal-bridge-integer-locals.xsh"
    ));

    const FAILURES_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/formal-bridge-integer-failures.xsh"
    ));

    const FAILURES_SCHEDULE: [ScheduledCall; 2] = [
        ScheduledCall { function: "divided", arguments: &[] },
        ScheduledCall { function: "overflowed", arguments: &[] },
    ];

    const CALLS_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/formal-bridge-integer-calls.xsh"
    ));

    const ASSIGNMENTS_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/formal-bridge-integer-assignments.xsh"
    ));

    const ASSIGNMENTS_SCHEDULE: [ScheduledCall; 2] = [
        ScheduledCall { function: "scaled", arguments: &[] },
        ScheduledCall { function: "wrapped", arguments: &[] },
    ];

    const CALLS_SCHEDULE: [ScheduledCall; 3] = [
        ScheduledCall { function: "excess", arguments: &[12, 9] },
        ScheduledCall { function: "excess", arguments: &[3, 9] },
        ScheduledCall { function: "surplus", arguments: &[] },
    ];

    const BOOLEAN_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/formal-bridge-boolean-control.xsh"
    ));

    const BOOLEAN_SCHEDULE: [ScheduledCall; 4] = [
        ScheduledCall { function: "exceeds", arguments: &[12, 9] },
        ScheduledCall { function: "exceeds", arguments: &[3, 9] },
        ScheduledCall { function: "below", arguments: &[3, 9] },
        ScheduledCall { function: "below", arguments: &[12, 9] },
    ];

    const RESULT_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/formal-bridge-result-control.xsh"
    ));

    const RESULT_SCHEDULE: [ScheduledCall; 4] = [
        ScheduledCall { function: "keeps", arguments: &[7] },
        ScheduledCall { function: "stops", arguments: &[5] },
        ScheduledCall { function: "adds", arguments: &[4] },
        ScheduledCall { function: "adds", arguments: &[-1] },
    ];

    const PROPAGATION_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/formal-bridge-result-propagation.xsh"
    ));

    const LET_CALL_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/formal-bridge-integer-let-calls.xsh"
    ));

    const LET_CALL_SCHEDULE: [ScheduledCall; 3] = [
        ScheduledCall { function: "bumps", arguments: &[5] },
        ScheduledCall { function: "runs", arguments: &[5] },
        ScheduledCall { function: "runs", arguments: &[-9] },
    ];

    const COMPUTED_LET_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/formal-bridge-integer-computed-let-calls.xsh"
    ));

    const COMPUTED_LET_SCHEDULE: [ScheduledCall; 3] = [
        ScheduledCall { function: "scales", arguments: &[5] },
        ScheduledCall { function: "runs", arguments: &[5] },
        ScheduledCall { function: "runs", arguments: &[-9] },
    ];

    const SEVERAL_LET_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/formal-bridge-integer-several-let-calls.xsh"
    ));

    const SEVERAL_LET_SCHEDULE: [ScheduledCall; 3] = [
        ScheduledCall { function: "scales", arguments: &[5] },
        ScheduledCall { function: "runs", arguments: &[5] },
        ScheduledCall { function: "runs", arguments: &[-9] },
    ];

    const PROPAGATION_SCHEDULE: [ScheduledCall; 5] = [
        ScheduledCall { function: "keeps", arguments: &[7] },
        ScheduledCall { function: "stops", arguments: &[5] },
        ScheduledCall { function: "passes", arguments: &[7] },
        ScheduledCall { function: "blocks", arguments: &[5] },
        ScheduledCall { function: "passes", arguments: &[-3] },
    ];

    struct Fixture {
        source: SourceDocument,
        sources: SourceMap,
        parsed: crate::syntax::parser::ArenaParseOutput,
        program: FullProgram,
        document: ArithmeticDocument,
    }

    fn fixture_of(text: String, schedule: &[ScheduledCall]) -> Fixture {
        let source = SourceDocument { id: source_document_id(SourceId::new(0)).unwrap(), text };
        let (sources, parsed, program) = build_fixture(&source.text).unwrap();
        let document =
            export_arithmetic_document(&program, &parsed.arena, &sources, source.id, schedule)
                .unwrap();
        Fixture { source, sources, parsed, program, document }
    }

    fn fixture() -> Fixture {
        fixture_of(format!("{LOCALS_SOURCE}\n{CALLS_SOURCE}"), &SCHEDULE)
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

    fn tag_row(program: &FullProgram, tag: FullTag, occurrence: usize) -> usize {
        program
            .store
            .tags
            .iter()
            .enumerate()
            .filter(|(_, candidate)| **candidate == tag)
            .nth(occurrence)
            .expect("fixture row")
            .0
    }

    fn payload_start(program: &FullProgram, tag: FullTag, occurrence: usize) -> usize {
        program.store.data[tag_row(program, tag, occurrence)].lhs as usize
    }

    #[test]
    fn exported_documents_are_the_lean_certified_bytes() {
        let failures = fixture_of(FAILURES_SOURCE.to_string(), &FAILURES_SCHEDULE);
        let calls = fixture_of(CALLS_SOURCE.to_string(), &CALLS_SCHEDULE);
        let assignments = fixture_of(ASSIGNMENTS_SOURCE.to_string(), &ASSIGNMENTS_SCHEDULE);
        let boolean = fixture_of(BOOLEAN_SOURCE.to_string(), &BOOLEAN_SCHEDULE);
        let result = fixture_of(RESULT_SOURCE.to_string(), &RESULT_SCHEDULE);
        let propagation =
            fixture_of(PROPAGATION_SOURCE.to_string(), &PROPAGATION_SCHEDULE);
        let let_call = fixture_of(LET_CALL_SOURCE.to_string(), &LET_CALL_SCHEDULE);
        let computed_let =
            fixture_of(COMPUTED_LET_SOURCE.to_string(), &COMPUTED_LET_SCHEDULE);
        let several_let = fixture_of(SEVERAL_LET_SOURCE.to_string(), &SEVERAL_LET_SCHEDULE);
        let mut documents = Vec::new();
        for (fixture, schedule, source_name, raw_name) in [
            (&failures, &FAILURES_SCHEDULE[..], "sourceDocumentCodes", "rawIrDocumentCodes"),
            (&calls, &CALLS_SCHEDULE[..], "callsSourceDocumentCodes", "callsRawIrDocumentCodes"),
            (
                &assignments,
                &ASSIGNMENTS_SCHEDULE[..],
                "assignmentsSourceDocumentCodes",
                "assignmentsRawIrDocumentCodes",
            ),
            (
                &boolean,
                &BOOLEAN_SCHEDULE[..],
                "booleanSourceDocumentCodes",
                "booleanRawIrDocumentCodes",
            ),
            (
                &result,
                &RESULT_SCHEDULE[..],
                "resultSourceDocumentCodes",
                "resultRawIrDocumentCodes",
            ),
            (
                &propagation,
                &PROPAGATION_SCHEDULE[..],
                "propagationSourceDocumentCodes",
                "propagationRawIrDocumentCodes",
            ),
            (
                &let_call,
                &LET_CALL_SCHEDULE[..],
                "letCallSourceDocumentCodes",
                "letCallRawIrDocumentCodes",
            ),
            (
                &computed_let,
                &COMPUTED_LET_SCHEDULE[..],
                "computedLetSourceDocumentCodes",
                "computedLetRawIrDocumentCodes",
            ),
            (
                &several_let,
                &SEVERAL_LET_SCHEDULE[..],
                "severalLetSourceDocumentCodes",
                "severalLetRawIrDocumentCodes",
            ),
        ] {
            let source_text = encode_source_document(&fixture.source);
            let raw_text = encode_arithmetic_document(&fixture.document).unwrap();
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
                schedule,
            )
            .unwrap();
            documents.push((source_name, source_text));
            documents.push((raw_name, raw_text));
        }
        assert_lean_documents(
            &documents.iter().map(|(name, text)| (*name, text.as_str())).collect::<Vec<_>>(),
        );
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
        let observation = |function: u32, arguments: &[u64], outcome: Outcome| Observation {
            function: IrId::from_raw(function).unwrap(),
            arguments: arguments.to_vec(),
            outcome,
        };
        assert_eq!(
            document.observations,
            [
                observation(1, &[], Outcome::Value(5)),
                observation(2, &[], Outcome::Value(42)),
                observation(3, &[], Outcome::Failure(ProjectedFailure::DivisionByZero)),
                observation(4, &[], Outcome::Failure(ProjectedFailure::IntegerOverflow)),
                observation(5, &[12, 9], Outcome::Value(3)),
                observation(5, &[3, 9], Outcome::Value(0)),
                observation(6, &[], Outcome::Value(3)),
            ]
        );
    }

    #[test]
    fn exported_artifact_has_a_conditional_over_parameters_and_a_call_to_it() {
        let fixture = fixture();
        let document = &fixture.document;
        let function_named = |name: &str| {
            document.functions.iter().position(|function| function.name == name).unwrap()
        };
        let excess = &document.functions[function_named("excess")];
        assert_eq!((excess.params_len, excess.slot_count), (2, 3));
        let body = document.blocks[excess.body.raw() as usize - 1];
        let statements = &document.extra[body.start as usize + 1..(body.start + body.len) as usize];
        assert_eq!(statements.len(), 1);
        let returned = document.payload(&document.rows[statements[0] as usize]).unwrap()[0] as usize;
        assert_eq!(document.rows[returned].tag, ProjectedTag::ExprIf);
        let conditional = document.payload(&document.rows[returned]).unwrap();
        let branches = document.blocks[conditional[0] as usize - 1];
        let table = &document.extra[branches.start as usize..(branches.start + branches.len) as usize];
        assert_eq!(table[0], 1);
        let condition = document.rows[table[1] as usize];
        assert_eq!(condition.tag, ProjectedTag::ExprBinary);
        let operation = document.payload(&condition).unwrap()[0] as usize;
        assert_eq!(document.binary_ops[operation], ProjectedOp::Gt);
        assert_eq!(document.rows[table[2] as usize].tag, ProjectedTag::ExprValueBlock);
        assert_eq!(document.rows[conditional[1] as usize].tag, ProjectedTag::ExprValueBlock);

        let surplus = &document.functions[function_named("surplus")];
        let body = document.blocks[surplus.body.raw() as usize - 1];
        let statements = &document.extra[body.start as usize + 1..(body.start + body.len) as usize];
        let returned = document.payload(&document.rows[statements[1] as usize]).unwrap()[0] as usize;
        assert_eq!(document.rows[returned].tag, ProjectedTag::ExprDirectPureCall);
        let call = document.payload(&document.rows[returned]).unwrap();
        assert_eq!(call[0] as usize, function_named("excess") + 1);
        let arguments = document.blocks[call[1] as usize - 1];
        let table = &document.extra[arguments.start as usize..(arguments.start + arguments.len) as usize];
        assert_eq!(table[0], 2);
        assert_eq!(document.rows[table[2] as usize].tag, ProjectedTag::ExprParam);
        assert_eq!(document.rows[table[4] as usize].tag, ProjectedTag::ExprInt);
    }

    #[test]
    fn exported_boolean_rows_are_the_comparison_family() {
        let fixture = fixture_of(BOOLEAN_SOURCE.to_string(), &BOOLEAN_SCHEDULE);
        let document = &fixture.document;
        let exceeds = function_body_statements(&fixture, "exceeds");
        assert_eq!(exceeds.len(), 1);
        assert_eq!(document.rows[exceeds[0]].tag, ProjectedTag::StmtReturn);
        let returned = document.payload(&document.rows[exceeds[0]]).unwrap()[0] as usize;
        assert_eq!(document.rows[returned].tag, ProjectedTag::ExprBinary);
        let comparison = document.payload(&document.rows[returned]).unwrap();
        assert_eq!(document.binary_ops[comparison[0] as usize], ProjectedOp::Gt);
        assert_eq!(document.rows[comparison[1] as usize].tag, ProjectedTag::ExprParam);
        assert_eq!(document.rows[comparison[2] as usize].tag, ProjectedTag::ExprParam);
        let below = function_body_statements(&fixture, "below");
        assert_eq!(below.len(), 1);
        let returned = document.payload(&document.rows[below[0]]).unwrap()[0] as usize;
        assert_eq!(document.rows[returned].tag, ProjectedTag::ExprBinary);
        let comparison = document.payload(&document.rows[returned]).unwrap();
        assert_eq!(document.binary_ops[comparison[0] as usize], ProjectedOp::Lt);
        assert_eq!(
            document.observations.iter().map(|observation| observation.outcome).collect::<Vec<_>>(),
            [Outcome::Value(1), Outcome::Value(0), Outcome::Value(1), Outcome::Value(0)]
        );
    }

    #[test]
    fn mutated_boolean_programs_are_rejected_by_the_verifier_or_the_recorded_run() {
        let fixture = fixture_of(BOOLEAN_SOURCE.to_string(), &BOOLEAN_SCHEDULE);
        let check = |program: &FullProgram| {
            check_artifact_against_program(
                &fixture.document,
                program,
                &fixture.parsed.arena,
                &fixture.sources,
                &BOOLEAN_SCHEDULE,
            )
        };
        assert!(check(&fixture.program).is_ok());

        let mut reversed = fixture.program.clone();
        let greater = reversed
            .store
            .binary_ops
            .iter()
            .position(|op| *op == BinaryOp::Gt)
            .unwrap();
        reversed.store.binary_ops[greater] = BinaryOp::Lt;
        FullVerifier::verify(&reversed).expect("the reversed comparison is still well typed");
        assert_eq!(
            execute_schedule(&reversed, &fixture.sources, &BOOLEAN_SCHEDULE).unwrap()[0].outcome,
            Outcome::Value(0)
        );
        assert!(check(&reversed).is_err());

        let mut swapped = fixture.program.clone();
        let binary = payload_start(&swapped, FullTag::ExprBinary, 0);
        swapped.store.extra.swap(binary + 1, binary + 2);
        FullVerifier::verify(&swapped).expect("the swapped operands are still well typed");
        assert_eq!(
            execute_schedule(&swapped, &fixture.sources, &BOOLEAN_SCHEDULE).unwrap()[0].outcome,
            Outcome::Value(0)
        );
        assert!(check(&swapped).is_err());
    }

    #[test]
    fn exported_result_rows_are_the_constructor_family() {
        let fixture = fixture_of(RESULT_SOURCE.to_string(), &RESULT_SCHEDULE);
        let document = &fixture.document;
        let keeps = function_body_statements(&fixture, "keeps");
        assert_eq!(keeps.len(), 1);
        assert_eq!(document.rows[keeps[0]].tag, ProjectedTag::StmtReturn);
        let returned = document.payload(&document.rows[keeps[0]]).unwrap()[0] as usize;
        assert_eq!(document.rows[returned].tag, ProjectedTag::ExprOk);
        let payload = document.payload(&document.rows[returned]).unwrap();
        assert_eq!(document.rows[payload[0] as usize].tag, ProjectedTag::ExprParam);

        let stops = function_body_statements(&fixture, "stops");
        let returned = document.payload(&document.rows[stops[0]]).unwrap()[0] as usize;
        assert_eq!(document.rows[returned].tag, ProjectedTag::ExprErr);
        let failure = document.payload(&document.rows[returned]).unwrap();
        assert_eq!(document.rows[failure[0] as usize].tag, ProjectedTag::ExprParam);
        assert_eq!(failure[1], 0);

        let adds = function_body_statements(&fixture, "adds");
        let returned = document.payload(&document.rows[adds[0]]).unwrap()[0] as usize;
        assert_eq!(document.rows[returned].tag, ProjectedTag::ExprOk);
        let payload = document.payload(&document.rows[returned]).unwrap()[0] as usize;
        assert_eq!(document.rows[payload].tag, ProjectedTag::ExprBinary);
        assert_eq!(
            document.observations.iter().map(|observation| observation.outcome).collect::<Vec<_>>(),
            [Outcome::Ok(7), Outcome::Err(5), Outcome::Ok(5), Outcome::Ok(0)]
        );
    }

    #[test]
    fn mutated_result_programs_are_rejected_by_the_verifier_or_the_recorded_run() {
        let fixture = fixture_of(RESULT_SOURCE.to_string(), &RESULT_SCHEDULE);
        let check = |program: &FullProgram| {
            check_artifact_against_program(
                &fixture.document,
                program,
                &fixture.parsed.arena,
                &fixture.sources,
                &RESULT_SCHEDULE,
            )
        };
        assert!(check(&fixture.program).is_ok());

        let mut forged = fixture.program.clone();
        let constructor = forged
            .store
            .tags
            .iter()
            .position(|tag| *tag == FullTag::ExprErr)
            .unwrap();
        forged.store.tags[constructor] = FullTag::ExprOk;
        assert!(check(&forged).is_err());

        let mut literal_changed = fixture.program.clone();
        let adds_literal = payload_start(&literal_changed, FullTag::ExprInt, 0);
        literal_changed.store.extra[adds_literal] = 2;
        FullVerifier::verify(&literal_changed).expect("the changed literal is still well typed");
        assert_eq!(
            execute_schedule(&literal_changed, &fixture.sources, &RESULT_SCHEDULE).unwrap()[2].outcome,
            Outcome::Ok(6)
        );
        assert!(check(&literal_changed).is_err());
    }

    #[test]
    fn structurally_malformed_result_rows_are_rejected() {
        let fixture = fixture_of(RESULT_SOURCE.to_string(), &RESULT_SCHEDULE);
        let reject = |name: &str, mutate: &dyn Fn(&mut ArithmeticDocument)| {
            let mut document = fixture.document.clone();
            mutate(&mut document);
            assert!(validate_arithmetic_document(&document).is_err(), "{name} must be rejected");
        };
        let ok_row = fixture
            .document
            .rows
            .iter()
            .position(|row| row.tag == ProjectedTag::ExprOk)
            .unwrap();
        let err_row = fixture
            .document
            .rows
            .iter()
            .position(|row| row.tag == ProjectedTag::ExprErr)
            .unwrap();
        reject("an Ok payload that does not precede it", &|document| {
            let at = document.rows[ok_row].lhs as usize;
            document.extra[at] = ok_row as u32;
        });
        reject("an Ok payload that is a statement row", &|document| {
            let statement = document
                .rows
                .iter()
                .position(|row| row.tag == ProjectedTag::StmtReturn)
                .unwrap();
            let at = document.rows[ok_row].lhs as usize;
            document.extra[at] = statement as u32;
        });
        reject("an Err cause word that is not absent", &|document| {
            let at = document.rows[err_row].lhs as usize + 1;
            document.extra[at] = 1;
        });
        reject("an Err payload outside the table", &|document| {
            let at = document.rows[err_row].lhs as usize;
            document.extra[at] = 99;
        });
    }

    #[test]
    fn exported_assignment_rows_are_the_typed_integer_family() {
        let fixture = fixture_of(ASSIGNMENTS_SOURCE.to_string(), &ASSIGNMENTS_SCHEDULE);
        let document = &fixture.document;
        let scaled = function_body_statements(&fixture, "scaled");
        assert_eq!(scaled.len(), 3);
        assert_eq!(document.rows[scaled[0]].tag, ProjectedTag::StmtLetInt);
        assert_eq!(document.rows[scaled[1]].tag, ProjectedTag::StmtAssignInt);
        assert_eq!(document.rows[scaled[2]].tag, ProjectedTag::StmtReturn);
        let let_slot = document.payload(&document.rows[scaled[0]]).unwrap()[0];
        let assignment = document.payload(&document.rows[scaled[1]]).unwrap();
        assert_eq!((assignment[0], assignment[1]), (let_slot, 0));
        let value = document.rows[assignment[2] as usize];
        assert_eq!(value.tag, ProjectedTag::IntBinary);
        let binary = document.payload(&value).unwrap();
        assert_eq!(document.binary_ops[binary[0] as usize], ProjectedOp::Mul);
        assert_eq!(document.rows[binary[1] as usize].tag, ProjectedTag::IntSlot);
        assert_eq!(document.payload(&document.rows[binary[1] as usize]).unwrap(), [let_slot]);
        assert_eq!(document.rows[binary[2] as usize].tag, ProjectedTag::IntInt);
        let returned = document.payload(&document.rows[scaled[2]]).unwrap()[0] as usize;
        assert_eq!(document.rows[returned].tag, ProjectedTag::ExprBinary);
        assert_eq!(
            document.observations.iter().map(|observation| observation.outcome).collect::<Vec<_>>(),
            [Outcome::Value(31), Outcome::Failure(ProjectedFailure::IntegerOverflow)]
        );
    }

    #[test]
    fn assigned_local_calls_preserve_updated_values_and_unused_assignment_faults() {
        let source = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/formal/tests/fixtures/assigned-calls.xsh"
        ));
        assert_eq!(lean_document("assignedCallsSourceTextCodes").as_deref(), Some(source));
        let schedule = [
            ScheduledCall { function: "scaled", arguments: &[] },
            ScheduledCall { function: "runs", arguments: &[0] },
            ScheduledCall { function: "ignored", arguments: &[] },
            ScheduledCall { function: "fails", arguments: &[0] },
        ];
        let fixture = fixture_of(source.to_string(), &schedule);
        assert_eq!(
            fixture.document.observations.iter().map(|observation| observation.outcome)
                .collect::<Vec<_>>(),
            [Outcome::Value(31), Outcome::Value(31),
                Outcome::Failure(ProjectedFailure::DivisionByZero),
                Outcome::Failure(ProjectedFailure::DivisionByZero)]
        );
        for (caller, target) in [("runs", "scaled"), ("fails", "ignored")] {
            let statements = function_body_statements(&fixture, caller);
            assert_eq!(statements.len(), 1);
            let returned = fixture.document.payload(&fixture.document.rows[statements[0]]).unwrap()[0];
            let call = &fixture.document.rows[returned as usize];
            assert_eq!(call.tag, ProjectedTag::ExprDirectPureCall);
            let payload = fixture.document.payload(call).unwrap();
            let callee = &fixture.document.functions[payload[0] as usize - 1];
            assert_eq!(callee.name, target);
            assert_eq!(callee.params_len, 0);
        }
        let mut changed = fixture.program.clone();
        let division = changed.store.binary_ops.iter().position(|op| *op == BinaryOp::Div).unwrap();
        changed.store.binary_ops[division] = BinaryOp::Add;
        FullVerifier::verify(&changed).expect("the assigned sum remains well typed");
        let observed = execute_schedule(&changed, &fixture.sources, &schedule).unwrap();
        assert_eq!(observed[2].outcome, Outcome::Value(7));
        assert_eq!(observed[3].outcome, Outcome::Value(7));
        assert!(check_artifact_against_program(
            &fixture.document, &changed, &fixture.parsed.arena, &fixture.sources, &schedule,
        ).is_err());
    }

    #[test]
    fn structurally_malformed_assignments_are_rejected() {
        let fixture = fixture_of(ASSIGNMENTS_SOURCE.to_string(), &ASSIGNMENTS_SCHEDULE);
        let assignment_row = fixture
            .document
            .rows
            .iter()
            .position(|row| row.tag == ProjectedTag::StmtAssignInt)
            .unwrap();
        let reject = |name: &str, mutate: &dyn Fn(&mut ArithmeticDocument)| {
            let mut document = fixture.document.clone();
            mutate(&mut document);
            assert!(validate_arithmetic_document(&document).is_err(), "{name} must be rejected");
        };
        let word = |document: &ArithmeticDocument, offset: usize| {
            document.rows[assignment_row].lhs as usize + offset
        };
        reject("a compound assignment word", &|document| {
            let at = word(document, 1);
            document.extra[at] = 1;
        });
        reject("an assigned value that is an expression row", &|document| {
            let at = word(document, 2);
            document.extra[at] = document
                .rows
                .iter()
                .position(|row| row.tag == ProjectedTag::ExprInt)
                .unwrap() as u32;
        });
        reject("an assigned value that does not precede the assignment", &|document| {
            let at = word(document, 2);
            document.extra[at] = assignment_row as u32;
        });
        reject("an integer operand that is an expression row", &|document| {
            let binary = document
                .rows
                .iter()
                .position(|row| row.tag == ProjectedTag::IntBinary)
                .unwrap();
            let at = document.rows[binary].lhs as usize + 1;
            document.extra[at] = document
                .rows
                .iter()
                .position(|row| row.tag == ProjectedTag::ExprInt)
                .unwrap() as u32;
        });
        reject("an integer operation outside the pool", &|document| {
            let binary = document
                .rows
                .iter()
                .position(|row| row.tag == ProjectedTag::IntBinary)
                .unwrap();
            let at = document.rows[binary].lhs as usize;
            document.extra[at] = 99;
        });
        reject("an assignment that is not a function statement", &|document| {
            let block = document.blocks[0];
            let at = block.start as usize + 2;
            document.extra[at] = document
                .rows
                .iter()
                .position(|row| row.tag == ProjectedTag::IntSlot)
                .unwrap() as u32;
        });
    }

    #[test]
    fn mutated_assignments_are_rejected_by_the_verifier_or_the_recorded_run() {
        let fixture = fixture_of(ASSIGNMENTS_SOURCE.to_string(), &ASSIGNMENTS_SCHEDULE);
        let check = |program: &FullProgram| {
            check_artifact_against_program(
                &fixture.document,
                program,
                &fixture.parsed.arena,
                &fixture.sources,
                &ASSIGNMENTS_SCHEDULE,
            )
        };
        assert!(check(&fixture.program).is_ok());

        let mut wrong_operation = fixture.program.clone();
        let multiply = wrong_operation
            .store
            .binary_ops
            .iter()
            .position(|op| *op == BinaryOp::Mul)
            .unwrap();
        wrong_operation.store.binary_ops[multiply] = BinaryOp::Sub;
        FullVerifier::verify(&wrong_operation).expect("the subtraction is still well typed");
        assert_eq!(
            execute_schedule(&wrong_operation, &fixture.sources, &ASSIGNMENTS_SCHEDULE).unwrap()[0]
                .outcome,
            Outcome::Value(0)
        );
        assert!(check(&wrong_operation).is_err());

        let mut swapped_operands = fixture.program.clone();
        let binary = payload_start(&swapped_operands, FullTag::IntBinary, 0);
        swapped_operands.store.extra.swap(binary + 1, binary + 2);
        assert!(check(&swapped_operands).is_err());

        let mut assigned_literal = fixture.program.clone();
        let assignment = payload_start(&assigned_literal, FullTag::StmtAssignInt, 0);
        let literal_row = assigned_literal
            .store
            .tags
            .iter()
            .position(|tag| *tag == FullTag::IntInt)
            .unwrap() as u32;
        assigned_literal.store.extra[assignment + 2] = literal_row;
        assert!(FullVerifier::verify(&assigned_literal).is_err());
        assert!(check(&assigned_literal).is_err());

        let mut wrong_slot = fixture.program.clone();
        let assignment = payload_start(&wrong_slot, FullTag::StmtAssignInt, 0);
        wrong_slot.store.extra[assignment] = 0;
        assert!(check(&wrong_slot).is_err());
    }

    #[test]
    fn structurally_malformed_calls_and_conditionals_are_rejected() {
        let fixture = fixture();
        let row_of = |tag: ProjectedTag| {
            fixture.document.rows.iter().position(|row| row.tag == tag).unwrap()
        };
        let payload_at = |document: &ArithmeticDocument, tag: ProjectedTag, word: usize| {
            let row = &document.rows[document.rows.iter().position(|row| row.tag == tag).unwrap()];
            row.lhs as usize + word
        };
        let reject = |name: &str, mutate: &dyn Fn(&mut ArithmeticDocument)| {
            let mut document = fixture.document.clone();
            mutate(&mut document);
            assert!(validate_arithmetic_document(&document).is_err(), "{name} must be rejected");
        };
        reject("call to a function outside the table", &|document| {
            let word = payload_at(document, ProjectedTag::ExprDirectPureCall, 0);
            document.extra[word] = 99;
        });
        reject("call argument count disagrees with the callee", &|document| {
            document.functions[4].params_len = 3;
        });
        reject("conditional else value that does not precede it", &|document| {
            let word = payload_at(document, ProjectedTag::ExprIf, 1);
            document.extra[word] = row_of(ProjectedTag::ExprIf) as u32;
        });
        reject("value block referenced twice", &|document| {
            let first = document.rows.iter().position(|row| row.tag == ProjectedTag::ExprValueBlock).unwrap();
            let first_block = document.extra[document.rows[first].lhs as usize];
            let word = payload_at(document, ProjectedTag::ExprIf, 0);
            document.extra[word] = first_block;
        });
        reject("branch table entry that is not an expression", &|document| {
            let word = payload_at(document, ProjectedTag::ExprIf, 0);
            let table = document.blocks[document.extra[word] as usize - 1];
            document.extra[table.start as usize + 1] = row_of(ProjectedTag::StmtValue) as u32;
        });
        reject("call argument that is not a single value", &|document| {
            let word = payload_at(document, ProjectedTag::ExprDirectPureCall, 1);
            let table = document.blocks[document.extra[word] as usize - 1];
            document.extra[table.start as usize + 1] = 1;
        });
        reject("observation with a missing argument", &|document| {
            document.observations[4].arguments.pop();
        });
        reject("block owned by no function", &|document| {
            document.blocks[0].owner = OptionalIrId::from_raw(IR_NONE).unwrap();
        });
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
        reject("observation arguments that are not a list", &|value| {
            set_nested_wire(value, "observations", 4, "arguments", &hex_u64(12))
        });
        reject("unknown field", &|value| {
            insert_field(value, "unrecognized", WireValue::String("rejected".to_string()))
        });
        reject("unknown scope", &|value| {
            set_field(value, "scope", WireValue::String("scalar-literal-boundary-fixture-v1".to_string()))
        });
        reject("unknown observation outcome", &|value| {
            set_nested_wire(value, "observations", 0, "outcome", "completed")
        });
        reject("unknown specified failure", &|value| {
            set_nested_wire(value, "observations", 2, "detail", "index-out-of-range")
        });
        reject("failure outcome carrying value bits", &|value| {
            set_nested_wire(value, "observations", 2, "outcome", "value")
        });
        reject("value outcome carrying a failure name", &|value| {
            set_nested_wire(value, "observations", 0, "outcome", "failure")
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
        wrong_result.observations[0].outcome = Outcome::Value(4);
        assert!(validate_arithmetic_document(&wrong_result).is_ok());
        assert!(check(&wrong_result).is_err());

        let mut value_recorded_as_failure = fixture.document.clone();
        value_recorded_as_failure.observations[0].outcome =
            Outcome::Failure(ProjectedFailure::IntegerOverflow);
        assert!(check(&value_recorded_as_failure).is_err());

        let mut failure_recorded_as_value = fixture.document.clone();
        failure_recorded_as_value.observations[2].outcome = Outcome::Value(0);
        assert!(check(&failure_recorded_as_value).is_err());

        let mut wrong_failure = fixture.document.clone();
        wrong_failure.observations[2].outcome = Outcome::Failure(ProjectedFailure::IntegerOverflow);
        assert!(check(&wrong_failure).is_err());

        let mut wrong_branch = fixture.document.clone();
        wrong_branch.observations[4].outcome = Outcome::Value(0);
        assert!(validate_arithmetic_document(&wrong_branch).is_ok());
        assert!(check(&wrong_branch).is_err());

        let mut wrong_arguments = fixture.document.clone();
        wrong_arguments.observations[4].arguments = vec![9, 12];
        assert!(validate_arithmetic_document(&wrong_arguments).is_ok());
        assert!(check(&wrong_arguments).is_err());

        let mut duplicated = fixture.document.clone();
        duplicated.observations.push(duplicated.observations[0].clone());
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
            execute_schedule(&wrong_operation, &fixture.sources, &SCHEDULE).unwrap()[0].outcome,
            Outcome::Value(6)
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

        let mut wrong_comparison = fixture.program.clone();
        let greater = wrong_comparison
            .store
            .binary_ops
            .iter()
            .position(|op| *op == BinaryOp::Gt)
            .unwrap();
        wrong_comparison.store.binary_ops[greater] = BinaryOp::Lt;
        FullVerifier::verify(&wrong_comparison).expect("the reversed comparison is still well typed");
        let observations = execute_schedule(&wrong_comparison, &fixture.sources, &SCHEDULE).unwrap();
        assert_eq!(observations[4].outcome, Outcome::Value(0));
        assert_eq!(observations[5].outcome, Outcome::Value(-6_i64 as u64));
        assert!(check(&wrong_comparison).is_err());

        let mut swapped_arguments = fixture.program.clone();
        let call = payload_start(&swapped_arguments, FullTag::ExprDirectPureCall, 0);
        let table = swapped_arguments.store.blocks[swapped_arguments.store.extra[call + 1] as usize - 1]
            .instructions
            .start as usize;
        swapped_arguments.store.extra.swap(table + 2, table + 4);
        FullVerifier::verify(&swapped_arguments).expect("the swapped arguments are still well typed");
        let observations = execute_schedule(&swapped_arguments, &fixture.sources, &SCHEDULE).unwrap();
        assert_eq!(observations[6].outcome, Outcome::Value(0));
        assert!(check(&swapped_arguments).is_err());

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

    #[test]
    fn exported_result_propagation_rows_are_the_propagation_family() {
        let fixture =
            fixture_of(PROPAGATION_SOURCE.to_string(), &PROPAGATION_SCHEDULE);
        let document = &fixture.document;

        let keeps = function_body_statements(&fixture, "keeps");
        assert_eq!(keeps.len(), 1);
        let returned = document.payload(&document.rows[keeps[0]]).unwrap()[0] as usize;
        assert_eq!(document.rows[returned].tag, ProjectedTag::ExprOk);
        let sum = document.payload(&document.rows[returned]).unwrap()[0] as usize;
        assert_eq!(document.rows[sum].tag, ProjectedTag::ExprBinary);

        let passes = function_body_statements(&fixture, "passes");
        assert_eq!(passes.len(), 2);
        assert_eq!(document.rows[passes[0]].tag, ProjectedTag::StmtLet);
        let binding = document.payload(&document.rows[passes[0]]).unwrap();
        assert_eq!(binding[2], 0);
        let try_row = binding[1] as usize;
        assert_eq!(document.rows[try_row].tag, ProjectedTag::ExprTry);
        let propagated = document.payload(&document.rows[try_row]).unwrap();
        let call_row = propagated[0] as usize;
        assert_eq!(document.rows[call_row].tag, ProjectedTag::ExprDirectPureCall);
        let call = document.payload(&document.rows[call_row]).unwrap();
        assert_eq!(call[0] as usize, 1);
        let arguments = document.blocks[call[1] as usize - 1];
        let table = &document.extra
            [arguments.start as usize..(arguments.start + arguments.len) as usize];
        assert_eq!(table, &[1, 0, 8]);
        assert_eq!(document.rows[passes[1]].tag, ProjectedTag::StmtReturn);

        let blocks = function_body_statements(&fixture, "blocks");
        assert_eq!(blocks.len(), 2);
        assert_eq!(document.rows[blocks[0]].tag, ProjectedTag::StmtLet);
        let binding = document.payload(&document.rows[blocks[0]]).unwrap();
        let try_row = binding[1] as usize;
        assert_eq!(document.rows[try_row].tag, ProjectedTag::ExprTry);
        let call_row = document.payload(&document.rows[try_row]).unwrap()[0] as usize;
        let call = document.payload(&document.rows[call_row]).unwrap();
        assert_eq!(call[0] as usize, 2);

        assert_eq!(
            document.observations.iter().map(|observation| observation.outcome).collect::<Vec<_>>(),
            [
                Outcome::Ok(8),
                Outcome::Err(5),
                Outcome::Ok(8),
                Outcome::Err(5),
                Outcome::Ok(18446744073709551614),
            ]
        );
    }

    #[test]
    fn result_propagation_runs_continue_on_ok_and_propagate_err() {
        let fixture =
            fixture_of(PROPAGATION_SOURCE.to_string(), &PROPAGATION_SCHEDULE);
        let observations = execute_schedule(
            &fixture.program,
            &fixture.sources,
            &PROPAGATION_SCHEDULE,
        )
        .unwrap();
        assert_eq!(
            observations.iter().map(|observation| observation.outcome).collect::<Vec<_>>(),
            [
                Outcome::Ok(8),
                Outcome::Err(5),
                Outcome::Ok(8),
                Outcome::Err(5),
                Outcome::Ok(18446744073709551614),
            ]
        );
    }

    #[test]
    fn mutated_result_propagation_programs_are_rejected_by_the_verifier_or_the_recorded_run() {
        let fixture =
            fixture_of(PROPAGATION_SOURCE.to_string(), &PROPAGATION_SCHEDULE);
        let check = |document: &ArithmeticDocument, program: &FullProgram| {
            check_artifact_against_program(
                document,
                program,
                &fixture.parsed.arena,
                &fixture.sources,
                &PROPAGATION_SCHEDULE,
            )
        };
        assert!(check(&fixture.document, &fixture.program).is_ok());

        let mut wrong_slot = fixture.program.clone();
        let slot = payload_start(&wrong_slot, FullTag::ExprParam, 3);
        wrong_slot.store.extra[slot] = 0;
        FullVerifier::verify(&wrong_slot).expect("the changed slot is still well typed");
        assert_eq!(
            execute_schedule(&wrong_slot, &fixture.sources, &PROPAGATION_SCHEDULE).unwrap()[2]
                .outcome,
            Outcome::Ok(7)
        );
        assert!(check(&fixture.document, &wrong_slot).is_err());

        let mut wrong_record = fixture.document.clone();
        wrong_record.observations[2].outcome = Outcome::Err(8);
        assert!(validate_arithmetic_document(&wrong_record).is_ok());
        assert!(check(&wrong_record, &fixture.program).is_err());
    }

    #[test]
    fn structurally_malformed_result_propagation_rows_are_rejected() {
        let fixture =
            fixture_of(PROPAGATION_SOURCE.to_string(), &PROPAGATION_SCHEDULE);
        let reject = |name: &str, mutate: &dyn Fn(&mut ArithmeticDocument)| {
            let mut document = fixture.document.clone();
            mutate(&mut document);
            assert!(validate_arithmetic_document(&document).is_err(), "{name} must be rejected");
        };
        let try_row = fixture
            .document
            .rows
            .iter()
            .position(|row| row.tag == ProjectedTag::ExprTry)
            .unwrap();
        let binding_row = fixture
            .document
            .rows
            .iter()
            .position(|row| row.tag == ProjectedTag::StmtLet)
            .unwrap();
        let call_row = fixture
            .document
            .rows
            .iter()
            .position(|row| row.tag == ProjectedTag::ExprDirectPureCall)
            .unwrap();
        reject("a propagation row naming itself", &|document| {
            document.extra[document.rows[try_row].lhs as usize] = try_row as u32;
        });
        reject("a binding value that is not a propagation", &|document| {
            document.extra[document.rows[binding_row].lhs as usize + 1] = call_row as u32;
        });
        reject("a binding type annotation", &|document| {
            document.extra[document.rows[binding_row].lhs as usize + 2] = 1;
        });
        reject("a call naming the absent function", &|document| {
            document.extra[document.rows[call_row].lhs as usize] = 0;
        });
    }

    #[test]
    fn exported_integer_let_call_rows_are_the_let_call_family() {
        let fixture = fixture_of(LET_CALL_SOURCE.to_string(), &LET_CALL_SCHEDULE);
        let document = &fixture.document;

        let bumps = function_body_statements(&fixture, "bumps");
        assert_eq!(bumps.len(), 2);
        assert_eq!(document.rows[bumps[0]].tag, ProjectedTag::StmtLetInt);
        let binding = document.payload(&document.rows[bumps[0]]).unwrap();
        assert_eq!(binding[0], 2);
        assert_eq!(document.rows[binding[1] as usize].tag, ProjectedTag::IntInt);
        assert_eq!(document.rows[bumps[1]].tag, ProjectedTag::StmtReturn);
        let returned = document.payload(&document.rows[bumps[1]]).unwrap()[0] as usize;
        assert_eq!(document.rows[returned].tag, ProjectedTag::ExprBinary);

        let runs = function_body_statements(&fixture, "runs");
        assert_eq!(runs.len(), 1);
        let returned = document.payload(&document.rows[runs[0]]).unwrap()[0] as usize;
        assert_eq!(document.rows[returned].tag, ProjectedTag::ExprDirectPureCall);
        let call = document.payload(&document.rows[returned]).unwrap();
        assert_eq!(call[0], 1);
        let arguments = document.blocks[call[1] as usize - 1];
        let table = &document.extra
            [arguments.start as usize..(arguments.start + arguments.len) as usize];
        assert_eq!(table, &[1, 0, 6]);
        assert_eq!(
            document.observations.iter().map(|observation| observation.outcome).collect::<Vec<_>>(),
            [Outcome::Value(7), Outcome::Value(7), Outcome::Value(-7_i64 as u64)]
        );
    }

    #[test]
    fn integer_let_calls_run_through_the_let_binding() {
        let fixture = fixture_of(LET_CALL_SOURCE.to_string(), &LET_CALL_SCHEDULE);
        let observations =
            execute_schedule(&fixture.program, &fixture.sources, &LET_CALL_SCHEDULE).unwrap();
        assert_eq!(
            observations.iter().map(|observation| observation.outcome).collect::<Vec<_>>(),
            [Outcome::Value(7), Outcome::Value(7), Outcome::Value(-7_i64 as u64)]
        );
    }

    #[test]
    fn mutated_integer_let_call_programs_are_rejected_by_the_verifier_or_the_recorded_run() {
        let fixture = fixture_of(LET_CALL_SOURCE.to_string(), &LET_CALL_SCHEDULE);
        let check = |document: &ArithmeticDocument, program: &FullProgram| {
            check_artifact_against_program(
                document,
                program,
                &fixture.parsed.arena,
                &fixture.sources,
                &LET_CALL_SCHEDULE,
            )
        };
        assert!(check(&fixture.document, &fixture.program).is_ok());

        let mut wrong_read = fixture.program.clone();
        let slot = payload_start(&wrong_read, FullTag::ExprParam, 1);
        wrong_read.store.extra[slot] = 0;
        FullVerifier::verify(&wrong_read).expect("the changed read is still well typed");
        assert_eq!(
            execute_schedule(&wrong_read, &fixture.sources, &LET_CALL_SCHEDULE).unwrap()[0]
                .outcome,
            Outcome::Value(10)
        );
        assert!(check(&fixture.document, &wrong_read).is_err());

        let mut wrong_record = fixture.document.clone();
        wrong_record.observations[0].outcome = Outcome::Value(8);
        assert!(validate_arithmetic_document(&wrong_record).is_ok());
        assert!(check(&wrong_record, &fixture.program).is_err());
    }

    #[test]
    fn structurally_malformed_integer_let_rows_are_rejected() {
        let fixture = fixture_of(LET_CALL_SOURCE.to_string(), &LET_CALL_SCHEDULE);
        let reject = |name: &str, mutate: &dyn Fn(&mut ArithmeticDocument)| {
            let mut document = fixture.document.clone();
            mutate(&mut document);
            assert!(validate_arithmetic_document(&document).is_err(), "{name} must be rejected");
        };
        let binding_row = fixture
            .document
            .rows
            .iter()
            .position(|row| row.tag == ProjectedTag::StmtLetInt)
            .unwrap();
        reject("a let binding naming its own row as its value", &|document| {
            document.extra[document.rows[binding_row].lhs as usize + 1] = binding_row as u32;
        });
        reject("a let binding whose value is not an integer row", &|document| {
            document.extra[document.rows[binding_row].lhs as usize + 1] =
                document.rows.len() as u32;
        });
    }

    #[test]
    fn exported_integer_computed_let_rows_are_the_integer_family() {
        let fixture = fixture_of(COMPUTED_LET_SOURCE.to_string(), &COMPUTED_LET_SCHEDULE);
        let document = &fixture.document;

        let scales = function_body_statements(&fixture, "scales");
        assert_eq!(scales.len(), 2);
        assert_eq!(document.rows[scales[0]].tag, ProjectedTag::StmtLetInt);
        let binding = document.payload(&document.rows[scales[0]]).unwrap();
        assert_eq!(binding[0], 2);
        let value = document.rows[binding[1] as usize];
        assert_eq!(value.tag, ProjectedTag::IntBinary);
        let binary = document.payload(&value).unwrap();
        assert_eq!(document.binary_ops[binary[0] as usize], ProjectedOp::Add);
        assert_eq!(document.rows[binary[1] as usize].tag, ProjectedTag::IntSlot);
        assert_eq!(document.payload(&document.rows[binary[1] as usize]).unwrap(), [0]);
        assert_eq!(document.rows[binary[2] as usize].tag, ProjectedTag::IntInt);
        assert_eq!(document.rows[scales[1]].tag, ProjectedTag::StmtReturn);
        let returned = document.payload(&document.rows[scales[1]]).unwrap()[0] as usize;
        assert_eq!(document.rows[returned].tag, ProjectedTag::ExprBinary);
        let sum = document.payload(&document.rows[returned]).unwrap();
        assert_eq!(document.rows[sum[1] as usize].tag, ProjectedTag::ExprParam);
        assert_eq!(document.payload(&document.rows[sum[1] as usize]).unwrap(), [0]);
        assert_eq!(document.rows[sum[2] as usize].tag, ProjectedTag::ExprParam);
        assert_eq!(document.payload(&document.rows[sum[2] as usize]).unwrap(), [2]);

        let runs = function_body_statements(&fixture, "runs");
        assert_eq!(runs.len(), 1);
        let returned = document.payload(&document.rows[runs[0]]).unwrap()[0] as usize;
        assert_eq!(document.rows[returned].tag, ProjectedTag::ExprDirectPureCall);
        let call = document.payload(&document.rows[returned]).unwrap();
        assert_eq!(call[0], 1);
        let arguments = document.blocks[call[1] as usize - 1];
        let table = &document.extra
            [arguments.start as usize..(arguments.start + arguments.len) as usize];
        assert_eq!((table[0], table[1]), (1, 0));
        assert_eq!(document.rows[table[2] as usize].tag, ProjectedTag::ExprParam);
        assert_eq!(document.payload(&document.rows[table[2] as usize]).unwrap(), [0]);
        assert_eq!(
            document.observations.iter().map(|observation| observation.outcome).collect::<Vec<_>>(),
            [Outcome::Value(14), Outcome::Value(14), Outcome::Value(-14_i64 as u64)]
        );
    }

    #[test]
    fn integer_computed_let_calls_run_through_the_binding() {
        let fixture = fixture_of(COMPUTED_LET_SOURCE.to_string(), &COMPUTED_LET_SCHEDULE);
        let observations =
            execute_schedule(&fixture.program, &fixture.sources, &COMPUTED_LET_SCHEDULE).unwrap();
        assert_eq!(
            observations.iter().map(|observation| observation.outcome).collect::<Vec<_>>(),
            [Outcome::Value(14), Outcome::Value(14), Outcome::Value(-14_i64 as u64)]
        );
    }

    #[test]
    fn mutated_integer_computed_let_programs_are_rejected_by_the_verifier_or_the_recorded_run() {
        let fixture = fixture_of(COMPUTED_LET_SOURCE.to_string(), &COMPUTED_LET_SCHEDULE);
        let check = |document: &ArithmeticDocument, program: &FullProgram| {
            check_artifact_against_program(
                document,
                program,
                &fixture.parsed.arena,
                &fixture.sources,
                &COMPUTED_LET_SCHEDULE,
            )
        };
        assert!(check(&fixture.document, &fixture.program).is_ok());

        let mut wrong_operation = fixture.program.clone();
        let add = wrong_operation
            .store
            .binary_ops
            .iter()
            .position(|op| *op == BinaryOp::Add)
            .unwrap();
        wrong_operation.store.binary_ops[add] = BinaryOp::Sub;
        FullVerifier::verify(&wrong_operation).expect("the subtraction is still well typed");
        assert_eq!(
            execute_schedule(&wrong_operation, &fixture.sources, &COMPUTED_LET_SCHEDULE).unwrap()[0]
                .outcome,
            Outcome::Value(4)
        );
        assert!(check(&fixture.document, &wrong_operation).is_err());

        let mut wrong_record = fixture.document.clone();
        wrong_record.observations[0].outcome = Outcome::Value(15);
        assert!(validate_arithmetic_document(&wrong_record).is_ok());
        assert!(check(&wrong_record, &fixture.program).is_err());
    }

    #[test]
    fn structurally_malformed_computed_let_rows_are_rejected() {
        let fixture = fixture_of(COMPUTED_LET_SOURCE.to_string(), &COMPUTED_LET_SCHEDULE);
        let reject = |name: &str, mutate: &dyn Fn(&mut ArithmeticDocument)| {
            let mut document = fixture.document.clone();
            mutate(&mut document);
            assert!(validate_arithmetic_document(&document).is_err(), "{name} must be rejected");
        };
        let binding_row = fixture
            .document
            .rows
            .iter()
            .position(|row| row.tag == ProjectedTag::StmtLetInt)
            .unwrap();
        reject("a computed let value naming its own row", &|document| {
            document.extra[document.rows[binding_row].lhs as usize + 1] = binding_row as u32;
        });
        reject("a computed let value that is an expression row", &|document| {
            document.extra[document.rows[binding_row].lhs as usize + 1] = document
                .rows
                .iter()
                .position(|row| row.tag == ProjectedTag::ExprParam)
                .unwrap() as u32;
        });
    }

    #[test]
    fn exported_integer_several_let_rows_are_the_computed_family_in_order() {
        let fixture = fixture_of(SEVERAL_LET_SOURCE.to_string(), &SEVERAL_LET_SCHEDULE);
        let document = &fixture.document;

        let scales = function_body_statements(&fixture, "scales");
        assert_eq!(scales.len(), 3);
        assert_eq!(document.rows[scales[0]].tag, ProjectedTag::StmtLetInt);
        assert_eq!(document.rows[scales[1]].tag, ProjectedTag::StmtLetInt);
        assert_eq!(document.rows[scales[2]].tag, ProjectedTag::StmtReturn);

        let first = document.payload(&document.rows[scales[0]]).unwrap();
        assert_eq!(first[0], 2);
        let first_value = document.payload(&document.rows[first[1] as usize]).unwrap();
        assert_eq!(document.binary_ops[first_value[0] as usize], ProjectedOp::Add);
        assert_eq!(document.rows[first_value[1] as usize].tag, ProjectedTag::IntSlot);
        assert_eq!(document.payload(&document.rows[first_value[1] as usize]).unwrap(), [0]);
        assert_eq!(document.rows[first_value[2] as usize].tag, ProjectedTag::IntInt);

        let second = document.payload(&document.rows[scales[1]]).unwrap();
        assert_eq!(second[0], 3);
        let second_value = document.payload(&document.rows[second[1] as usize]).unwrap();
        assert_eq!(document.binary_ops[second_value[0] as usize], ProjectedOp::Mul);
        assert_eq!(document.rows[second_value[1] as usize].tag, ProjectedTag::IntSlot);
        assert_eq!(document.payload(&document.rows[second_value[1] as usize]).unwrap(), [2]);
        assert_eq!(document.rows[second_value[2] as usize].tag, ProjectedTag::IntInt);

        let returned = document.payload(&document.rows[scales[2]]).unwrap()[0] as usize;
        assert_eq!(document.rows[returned].tag, ProjectedTag::ExprBinary);
        let difference = document.payload(&document.rows[returned]).unwrap();
        assert_eq!(document.binary_ops[difference[0] as usize], ProjectedOp::Sub);
        assert_eq!(document.rows[difference[1] as usize].tag, ProjectedTag::ExprParam);
        assert_eq!(document.payload(&document.rows[difference[1] as usize]).unwrap(), [3]);
        assert_eq!(document.rows[difference[2] as usize].tag, ProjectedTag::ExprParam);
        assert_eq!(document.payload(&document.rows[difference[2] as usize]).unwrap(), [0]);

        let runs = function_body_statements(&fixture, "runs");
        assert_eq!(runs.len(), 1);
        let returned = document.payload(&document.rows[runs[0]]).unwrap()[0] as usize;
        assert_eq!(document.rows[returned].tag, ProjectedTag::ExprDirectPureCall);
        let call = document.payload(&document.rows[returned]).unwrap();
        assert_eq!(call[0], 1);
        let arguments = document.blocks[call[1] as usize - 1];
        let table = &document.extra
            [arguments.start as usize..(arguments.start + arguments.len) as usize];
        assert_eq!((table[0], table[1]), (1, 0));
        assert_eq!(document.rows[table[2] as usize].tag, ProjectedTag::ExprParam);
        assert_eq!(document.payload(&document.rows[table[2] as usize]).unwrap(), [0]);
        assert_eq!(
            document.observations.iter().map(|observation| observation.outcome).collect::<Vec<_>>(),
            [Outcome::Value(13), Outcome::Value(13), Outcome::Value(-1_i64 as u64)]
        );
    }

    #[test]
    fn integer_several_let_calls_run_through_both_bindings() {
        let fixture = fixture_of(SEVERAL_LET_SOURCE.to_string(), &SEVERAL_LET_SCHEDULE);
        let observations =
            execute_schedule(&fixture.program, &fixture.sources, &SEVERAL_LET_SCHEDULE).unwrap();
        assert_eq!(
            observations.iter().map(|observation| observation.outcome).collect::<Vec<_>>(),
            [Outcome::Value(13), Outcome::Value(13), Outcome::Value(-1_i64 as u64)]
        );
    }

    #[test]
    fn mutated_integer_several_let_programs_are_rejected_by_the_verifier_or_the_recorded_run() {
        let fixture = fixture_of(SEVERAL_LET_SOURCE.to_string(), &SEVERAL_LET_SCHEDULE);
        let check = |document: &ArithmeticDocument, program: &FullProgram| {
            check_artifact_against_program(
                document,
                program,
                &fixture.parsed.arena,
                &fixture.sources,
                &SEVERAL_LET_SCHEDULE,
            )
        };
        assert!(check(&fixture.document, &fixture.program).is_ok());

        let mut wrong_operation = fixture.program.clone();
        let add = wrong_operation
            .store
            .binary_ops
            .iter()
            .position(|op| *op == BinaryOp::Add)
            .unwrap();
        wrong_operation.store.binary_ops[add] = BinaryOp::Mul;
        FullVerifier::verify(&wrong_operation).expect("the multiplied binding is still well typed");
        assert_eq!(
            execute_schedule(&wrong_operation, &fixture.sources, &SEVERAL_LET_SCHEDULE).unwrap()[0]
                .outcome,
            Outcome::Value(35)
        );
        assert!(check(&fixture.document, &wrong_operation).is_err());

        let mut wrong_record = fixture.document.clone();
        wrong_record.observations[0].outcome = Outcome::Value(14);
        assert!(validate_arithmetic_document(&wrong_record).is_ok());
        assert!(check(&wrong_record, &fixture.program).is_err());
    }

    #[test]
    fn structurally_malformed_several_let_rows_are_rejected() {
        let fixture = fixture_of(SEVERAL_LET_SOURCE.to_string(), &SEVERAL_LET_SCHEDULE);
        let reject = |name: &str, mutate: &dyn Fn(&mut ArithmeticDocument)| {
            let mut document = fixture.document.clone();
            mutate(&mut document);
            assert!(validate_arithmetic_document(&document).is_err(), "{name} must be rejected");
        };
        let bindings: Vec<usize> = fixture
            .document
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.tag == ProjectedTag::StmtLetInt)
            .map(|(index, _)| index)
            .collect();
        assert_eq!(bindings.len(), 2);
        reject("a second binding naming itself as its value", &|document| {
            document.extra[document.rows[bindings[1]].lhs as usize + 1] = bindings[1] as u32;
        });
        reject("a second binding whose value is an expression row", &|document| {
            document.extra[document.rows[bindings[1]].lhs as usize + 1] = document
                .rows
                .iter()
                .position(|row| row.tag == ProjectedTag::ExprParam)
                .unwrap() as u32;
        });
        reject("a second binding whose value is a statement row", &|document| {
            document.extra[document.rows[bindings[1]].lhs as usize + 1] = bindings[0] as u32;
        });
    }

}
