use super::arithmetic::ProjectedOp;
use super::*;
use crate::sema::check::{
    BindingId, CheckOptions, Checker, CompactDeclOutput, EffectDeclarationId,
};
use crate::sema::types::Type;
use crate::syntax::arena::{
    ArenaExprKind, ArenaExprOrRun, ArenaStmtKind, BindingTargetId, ExprId, FunctionDefId,
};
use crate::syntax::node::BinaryOp;

const FACTS_SCHEMA: &str = "xsh-checked-facts-v1";
const FACTS_SCOPE: &str = "integer-calls-conditional-facts-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FactType {
    Int,
    Bool,
}

impl FactType {
    const ALL: [Self; 2] = [Self::Int, Self::Bool];

    fn from_type(ty: &Type) -> Result<Self, String> {
        match ty {
            Type::Int => Ok(Self::Int),
            Type::Bool => Ok(Self::Bool),
            other => Err(format!("checked type {other:?} is outside the fact projection")),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Int => "Int",
            Self::Bool => "Bool",
        }
    }

    fn from_name(name: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|fact_type| fact_type.name() == name)
            .ok_or_else(|| format!("unknown fact type {name:?}"))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Resolution {
    Parameter(u32),
    LetBinding(u32),
}

impl Resolution {
    fn kind(self) -> &'static str {
        match self {
            Self::Parameter(_) => "parameter",
            Self::LetBinding(_) => "letBinding",
        }
    }

    fn index(self) -> u32 {
        match self {
            Self::Parameter(index) | Self::LetBinding(index) => index,
        }
    }

    fn parse(kind: &str, index: u32) -> Result<Self, String> {
        match kind {
            "parameter" => Ok(Self::Parameter(index)),
            "letBinding" => Ok(Self::LetBinding(index)),
            other => Err(format!("unknown resolution kind {other:?}")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NodeFact {
    Literal(u64),
    Read(Resolution),
    Operate { op: ProjectedOp, operands: FactType, result: FactType },
    Call { callee: u32, arguments: u32 },
    Branch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FunctionFacts {
    name: String,
    parameter_types: Vec<FactType>,
    local_types: Vec<FactType>,
    result_type: FactType,
    effects: Vec<String>,
    nodes: Vec<NodeFact>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FactsDocument {
    source_id: IrId,
    functions: Vec<FunctionFacts>,
}

fn validate_facts_document(document: &FactsDocument) -> Result<(), String> {
    let mut names = BTreeSet::new();
    for (index, function) in document.functions.iter().enumerate() {
        if function.name.is_empty() || !names.insert(function.name.as_str()) {
            return Err("function fact names must be nonempty and distinct".to_string());
        }
        for node in &function.nodes {
            match *node {
                NodeFact::Read(Resolution::Parameter(position)) => {
                    if position as usize >= function.parameter_types.len() {
                        return Err("read names a parameter the scheme does not have".to_string());
                    }
                }
                NodeFact::Read(Resolution::LetBinding(position)) => {
                    if position as usize >= function.local_types.len() {
                        return Err("read names a let the function does not have".to_string());
                    }
                }
                NodeFact::Call { callee, .. } => {
                    if callee as usize >= index {
                        return Err("call names a function that is not earlier".to_string());
                    }
                }
                NodeFact::Literal(_) | NodeFact::Operate { .. } | NodeFact::Branch => {}
            }
        }
    }
    Ok(())
}

fn type_array(types: &[FactType]) -> WireValue {
    WireValue::Array(types.iter().map(|fact_type| wire_string(fact_type.name())).collect())
}

fn node_wire(node: &NodeFact) -> WireValue {
    match node {
        NodeFact::Literal(bits) => {
            wire_object([("kind", wire_string("literal")), ("value", wire_string(hex_u64(*bits)))])
        }
        NodeFact::Read(resolution) => wire_object([
            ("kind", wire_string("read")),
            ("resolution", wire_string(resolution.kind())),
            ("index", wire_string(hex_u32(resolution.index()))),
        ]),
        NodeFact::Operate { op, operands, result } => wire_object([
            ("kind", wire_string("operate")),
            ("op", wire_string(op.name())),
            ("ordinal", wire_string(hex_u32(op.ordinal()))),
            ("operands", wire_string(operands.name())),
            ("result", wire_string(result.name())),
        ]),
        NodeFact::Call { callee, arguments } => wire_object([
            ("kind", wire_string("call")),
            ("callee", wire_string(hex_u32(*callee))),
            ("arguments", wire_string(hex_u32(*arguments))),
        ]),
        NodeFact::Branch => wire_object([("kind", wire_string("branch"))]),
    }
}

fn encode_facts_document(document: &FactsDocument) -> Result<String, String> {
    validate_facts_document(document)?;
    let value = wire_object([
        ("schema", wire_string(FACTS_SCHEMA)),
        ("sourceId", wire_string(hex_u32(document.source_id.raw()))),
        ("scope", wire_string(FACTS_SCOPE)),
        ("functions", WireValue::Array(document.functions.iter().map(|function| wire_object([
            ("name", wire_string(&function.name)),
            ("parameterTypes", type_array(&function.parameter_types)),
            ("localTypes", type_array(&function.local_types)),
            ("resultType", wire_string(function.result_type.name())),
            ("effects", WireValue::Array(function.effects.iter().map(wire_string).collect())),
            ("nodes", WireValue::Array(function.nodes.iter().map(node_wire).collect())),
        ])).collect())),
    ]);
    deser_json::to_string(&value).map_err(|error| error.to_string())
}

fn decode_type_array(fields: &mut BTreeMap<String, WireValue>, name: &str) -> Result<Vec<FactType>, String> {
    take_array(fields, name)?
        .into_iter()
        .map(|value| match value {
            WireValue::String(text) => FactType::from_name(&text),
            _ => Err("fact type is not a string".to_string()),
        })
        .collect()
}

fn decode_node(value: WireValue) -> Result<NodeFact, String> {
    let WireValue::Object(entries) = &value else {
        return Err("fact node is not a JSON object".to_string());
    };
    let kind = match entries.iter().find(|(name, _)| name == "kind") {
        Some((_, WireValue::String(kind))) => kind.clone(),
        _ => return Err("fact node has no kind".to_string()),
    };
    match kind.as_str() {
        "literal" => {
            let mut fields = parse_object_fields(value, &["kind", "value"])?;
            Ok(NodeFact::Literal(parse_hex_u64(&take_string(&mut fields, "value")?)?))
        }
        "read" => {
            let mut fields = parse_object_fields(value, &["kind", "resolution", "index"])?;
            let index = parse_hex_u32(&take_string(&mut fields, "index")?)?;
            Ok(NodeFact::Read(Resolution::parse(&take_string(&mut fields, "resolution")?, index)?))
        }
        "operate" => {
            let mut fields =
                parse_object_fields(value, &["kind", "op", "ordinal", "operands", "result"])?;
            let op = ProjectedOp::from_name(&take_string(&mut fields, "op")?)?;
            let ordinal = parse_hex_u32(&take_string(&mut fields, "ordinal")?)?;
            if ordinal != op.ordinal() {
                return Err(format!(
                    "operation {} has ordinal {ordinal:08x}, expected {:08x}",
                    op.name(),
                    op.ordinal()
                ));
            }
            Ok(NodeFact::Operate {
                op,
                operands: FactType::from_name(&take_string(&mut fields, "operands")?)?,
                result: FactType::from_name(&take_string(&mut fields, "result")?)?,
            })
        }
        "call" => {
            let mut fields = parse_object_fields(value, &["kind", "callee", "arguments"])?;
            Ok(NodeFact::Call {
                callee: parse_hex_u32(&take_string(&mut fields, "callee")?)?,
                arguments: parse_hex_u32(&take_string(&mut fields, "arguments")?)?,
            })
        }
        "branch" => {
            parse_object_fields(value, &["kind"])?;
            Ok(NodeFact::Branch)
        }
        other => Err(format!("unknown fact node kind {other:?}")),
    }
}

fn decode_function_facts(value: WireValue) -> Result<FunctionFacts, String> {
    let mut fields = parse_object_fields(
        value,
        &["name", "parameterTypes", "localTypes", "resultType", "effects", "nodes"],
    )?;
    let effects = take_array(&mut fields, "effects")?
        .into_iter()
        .map(|value| match value {
            WireValue::String(text) => Ok(text),
            _ => Err("effect is not a string".to_string()),
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(FunctionFacts {
        name: take_string(&mut fields, "name")?,
        parameter_types: decode_type_array(&mut fields, "parameterTypes")?,
        local_types: decode_type_array(&mut fields, "localTypes")?,
        result_type: FactType::from_name(&take_string(&mut fields, "resultType")?)?,
        effects,
        nodes: take_array(&mut fields, "nodes")?
            .into_iter()
            .map(decode_node)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn decode_facts_document(input: &str) -> Result<FactsDocument, String> {
    let mut fields = parse_object_fields(
        parse_wire_document(input)?,
        &["schema", "sourceId", "scope", "functions"],
    )?;
    if take_string(&mut fields, "schema")? != FACTS_SCHEMA {
        return Err("unknown checked-facts document version".to_string());
    }
    let source_id = IrId::from_raw(parse_hex_u32(&take_string(&mut fields, "sourceId")?)?)?;
    if take_string(&mut fields, "scope")? != FACTS_SCOPE {
        return Err("unsupported checked-facts projection scope".to_string());
    }
    let functions = take_array(&mut fields, "functions")?
        .into_iter()
        .map(decode_function_facts)
        .collect::<Result<Vec<_>, _>>()?;
    let document = FactsDocument { source_id, functions };
    validate_facts_document(&document)?;
    Ok(document)
}

/// What the Rust checker publishes for one program: the declarations and body
/// facts lowering consumes, and the operator it recorded for each binary
/// expression, which the compact output does not carry.
#[derive(Clone, Debug)]
pub(super) struct PublishedFacts {
    pub(super) declarations: CompactDeclOutput,
    binary_operators: BTreeMap<ExprId, BinaryOp>,
}

pub(super) fn publish_facts(arena: &ArenaProgram, sources: &SourceMap, source_id: SourceId) -> PublishedFacts {
    let checked = Checker::check_arena_with_sources_and_options(
        arena,
        sources,
        source_id,
        CheckOptions::for_execution(),
    );
    let binary_operators = checked
        .binary_operators
        .iter()
        .map(|(expression, operator)| (*expression, operator.operator))
        .collect();
    PublishedFacts { declarations: Checker::compact_declarations(arena, checked), binary_operators }
}

struct FunctionWalk<'a> {
    arena: &'a ArenaProgram,
    published: &'a PublishedFacts,
    parameters: Vec<BindingId>,
    locals: Vec<BindingId>,
}

fn require_int_expression(published: &PublishedFacts, expression: ExprId) -> Result<(), String> {
    let ty = published.declarations.bodies.expr_types.get(&expression)
        .ok_or_else(|| "the checker published no type for an expression".to_string())?;
    if FactType::from_type(ty)? != FactType::Int {
        return Err("the expression is not checked as Int".to_string());
    }
    Ok(())
}

impl FunctionWalk<'_> {
    fn expression_type(&self, expression: ExprId) -> Result<FactType, String> {
        let ty = self
            .published
            .declarations
            .bodies
            .expr_types
            .get(&expression)
            .ok_or_else(|| "the checker published no type for an expression".to_string())?;
        FactType::from_type(ty)
    }

    fn resolution(&self, expression: ExprId) -> Result<Resolution, String> {
        let binding = self
            .published
            .declarations
            .bodies
            .resolved_bindings
            .expressions
            .get(&expression)
            .ok_or_else(|| "the checker resolved no binding for a name".to_string())?;
        if let Some(position) = self.parameters.iter().position(|candidate| candidate == binding) {
            return Ok(Resolution::Parameter(position as u32));
        }
        if let Some(position) = self.locals.iter().position(|candidate| candidate == binding) {
            return Ok(Resolution::LetBinding(position as u32));
        }
        Err("a name resolves to a binding outside the function".to_string())
    }

    fn nodes(&self, expression: ExprId, output: &mut Vec<NodeFact>) -> Result<(), String> {
        let ast = &self.arena.arena;
        let kind = ast.expr(expression).kind;
        match kind {
            ArenaExprKind::Int(literal) => {
                if self.expression_type(expression)? != FactType::Int {
                    return Err("an integer literal is not checked as Int".to_string());
                }
                let value = ast
                    .int_literal(literal)
                    .value()
                    .ok_or_else(|| "integer literal is outside the signed 64-bit range".to_string())?;
                let bits = u64::try_from(value)
                    .map_err(|_| "negative integer literal is outside the fact projection".to_string())?;
                output.push(NodeFact::Literal(bits));
            }
            ArenaExprKind::Ident(_) => {
                require_int_expression(self.published, expression)?;
                output.push(NodeFact::Read(self.resolution(expression)?));
            }
            ArenaExprKind::Binary { left, right, .. } => {
                self.nodes(left, output)?;
                self.nodes(right, output)?;
                let operator = self
                    .published
                    .binary_operators
                    .get(&expression)
                    .ok_or_else(|| "the checker recorded no operator for an operation".to_string())?;
                let operands = self.expression_type(left)?;
                if self.expression_type(right)? != operands {
                    return Err("the operands of an operation have different checked types".to_string());
                }
                output.push(NodeFact::Operate {
                    op: ProjectedOp::from_binary_op(*operator)?,
                    operands,
                    result: self.expression_type(expression)?,
                });
            }
            ArenaExprKind::Call { args, .. } => {
                require_int_expression(self.published, expression)?;
                let mut arguments = 0;
                for argument in ast.call_args(args) {
                    let crate::syntax::arena::ArenaCallArgKind::Positional(value) = argument.kind
                    else {
                        return Err("only positional call arguments are in the projection".to_string());
                    };
                    self.nodes(value, output)?;
                    arguments += 1;
                }
                let target = self
                    .published
                    .declarations
                    .bodies
                    .selected_function_calls
                    .get(&expression)
                    .ok_or_else(|| "the checker selected no function for a call".to_string())?;
                let callee = ast.function_def(target.definition);
                let parameters = self
                    .published
                    .declarations
                    .pures
                    .get(&callee.name)
                    .ok_or_else(|| "the checker published no signature for a callee".to_string())?
                    .params
                    .len();
                if parameters != arguments {
                    return Err("a call disagrees with its callee's published arity".to_string());
                }
                output.push(NodeFact::Call {
                    callee: target.definition.index() as u32,
                    arguments: arguments as u32,
                });
            }
            ArenaExprKind::ValueBlock(block) => {
                require_int_expression(self.published, expression)?;
                let statements = ast.stmt_ids(ast.block(block).statements).collect::<Vec<_>>();
                let [statement] = statements.as_slice() else {
                    return Err("a value block holds exactly one statement".to_string());
                };
                let ArenaStmtKind::Expr(value) = ast.stmt(*statement).kind else {
                    return Err("a value block statement is not a value".to_string());
                };
                self.nodes(value, output)?;
            }
            ArenaExprKind::If { branches, else_value } => {
                require_int_expression(self.published, expression)?;
                let [branch] = ast.if_expr_branches(branches) else {
                    return Err("only a single-branch conditional is in the projection".to_string());
                };
                self.nodes(branch.condition, output)?;
                self.nodes(branch.value, output)?;
                self.nodes(else_value, output)?;
                output.push(NodeFact::Branch);
            }
            other => return Err(format!("expression {other:?} is outside the fact projection")),
        }
        Ok(())
    }
}

fn export_function_facts(
    arena: &ArenaProgram,
    published: &PublishedFacts,
    index: usize,
) -> Result<FunctionFacts, String> {
    let ast = &arena.arena;
    let definition = ast.function_def(FunctionDefId::from_index(index));
    let declarations = &published.declarations;
    let signature = declarations
        .pures
        .get(&definition.name)
        .ok_or_else(|| "the checker published no signature for a pure function".to_string())?;
    let parameters = ast.params(definition.params);
    if signature.params.len() != parameters.len() {
        return Err("the published signature disagrees with the declared parameters".to_string());
    }
    let bindings = &declarations.bodies.resolved_bindings;
    let parameter_bindings = parameters
        .iter()
        .map(|parameter| {
            bindings
                .declaration_sites
                .get(&(ast.span(parameter.span), parameter.name))
                .copied()
                .ok_or_else(|| "the checker declared no binding for a parameter".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;

    let body = ast.block(definition.body);
    let mut effects = signature
        .effects
        .iter()
        .flatten()
        .map(|effect| effect.as_str().to_string())
        .collect::<Vec<_>>();
    let identity = EffectDeclarationId { namespace: None, body: ast.span(body.span) };
    if let Some(fact) = declarations.function_effect_facts.get(&identity) {
        effects.extend(
            fact.effective
                .concrete()
                .map_err(|error| format!("function effects are unresolved: {error:?}"))?
                .ok_or_else(|| "the function's effects are unknown".to_string())?
                .into_iter()
                .map(|effect| effect.as_str().to_string()),
        );
    }
    effects.sort();
    effects.dedup();

    let mut locals = Vec::new();
    let mut local_types = Vec::new();
    let mut returned = None;
    for statement in ast.stmt_ids(body.statements) {
        let node = ast.stmt(statement);
        match node.kind {
            ArenaStmtKind::Let { target, initializer: ArenaExprOrRun::Expr(value), .. } => {
                require_int_expression(published, value)?;
                locals.push(local_binding(bindings, target)?);
                local_types.push(FactType::from_type(
                    declarations
                        .local_binding_types
                        .get(&node.span)
                        .ok_or_else(|| "the checker published no type for a let".to_string())?,
                )?);
            }
            ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(value))) if returned.is_none() => {
                returned = Some(value);
            }
            _ => return Err("statement is outside the fact projection".to_string()),
        }
    }
    let returned = returned.ok_or_else(|| "the function has no return".to_string())?;
    let walk = FunctionWalk { arena, published, parameters: parameter_bindings, locals };
    let mut nodes = Vec::new();
    walk.nodes(returned, &mut nodes)?;
    Ok(FunctionFacts {
        name: name_text(definition.name),
        parameter_types: signature
            .params
            .iter()
            .map(|parameter| FactType::from_type(&parameter.ty))
            .collect::<Result<Vec<_>, _>>()?,
        local_types,
        result_type: FactType::from_type(&signature.return_ty)?,
        effects,
        nodes,
    })
}

fn local_binding(
    bindings: &crate::sema::check::ResolvedBindings,
    target: BindingTargetId,
) -> Result<BindingId, String> {
    bindings
        .target(target)
        .ok_or_else(|| "the checker declared no binding for a let".to_string())
}

fn export_facts_document(
    arena: &ArenaProgram,
    published: &PublishedFacts,
    source_id: IrId,
) -> Result<FactsDocument, String> {
    let functions = (0..arena.arena.function_defs.len())
        .map(|index| export_function_facts(arena, published, index))
        .collect::<Result<Vec<_>, _>>()?;
    let document = FactsDocument { source_id, functions };
    validate_facts_document(&document)?;
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAILURES_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/formal-bridge-integer-failures.xsh"
    ));

    const CALLS_SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/formal-bridge-integer-calls.xsh"
    ));

    struct Fixture {
        source: SourceDocument,
        parsed: crate::syntax::parser::ArenaParseOutput,
        published: PublishedFacts,
        document: FactsDocument,
    }

    fn fixture_of(text: &str) -> Fixture {
        let source = SourceDocument { id: source_document_id(SourceId::new(0)).unwrap(), text: text.to_string() };
        let (sources, source_id, parsed, published) = check_fixture(text).unwrap();
        let document = export_facts_document(&parsed.arena, &published, source.id).unwrap();
        assert_eq!(source_id, SourceId::new(0));
        let _ = sources;
        Fixture { source, parsed, published, document }
    }

    #[test]
    fn exported_facts_are_the_lean_certified_bytes() {
        let failures = fixture_of(FAILURES_SOURCE);
        let calls = fixture_of(CALLS_SOURCE);
        let mut documents = Vec::new();
        for (fixture, name) in [(&failures, "failuresFactsDocumentCodes"), (&calls, "callsFactsDocumentCodes")] {
            let text = encode_facts_document(&fixture.document).unwrap();
            let decoded = decode_facts_document(&text).unwrap();
            assert_eq!(decoded, fixture.document);
            assert_eq!(encode_facts_document(&decoded).unwrap(), text);
            assert_eq!(decoded.source_id, fixture.source.id);
            documents.push((name, text));
        }
        assert_lean_documents(
            &documents.iter().map(|(name, text)| (*name, text.as_str())).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn exported_facts_state_the_checkers_resolutions_selections_and_types() {
        use FactType::{Bool, Int};
        let failures = fixture_of(FAILURES_SOURCE);
        assert_eq!(
            failures.document.functions,
            [
                FunctionFacts {
                    name: "divided".to_string(),
                    parameter_types: vec![],
                    local_types: vec![Int],
                    result_type: Int,
                    effects: vec![],
                    nodes: vec![
                        NodeFact::Literal(1),
                        NodeFact::Read(Resolution::LetBinding(0)),
                        NodeFact::Operate { op: ProjectedOp::Div, operands: Int, result: Int },
                    ],
                },
                FunctionFacts {
                    name: "overflowed".to_string(),
                    parameter_types: vec![],
                    local_types: vec![Int],
                    result_type: Int,
                    effects: vec![],
                    nodes: vec![
                        NodeFact::Read(Resolution::LetBinding(0)),
                        NodeFact::Literal(1),
                        NodeFact::Operate { op: ProjectedOp::Add, operands: Int, result: Int },
                    ],
                },
            ]
        );
        let calls = fixture_of(CALLS_SOURCE);
        let [excess, surplus] = calls.document.functions.as_slice() else {
            panic!("the calls fixture has two functions");
        };
        assert_eq!(excess.parameter_types, [Int, Int]);
        assert_eq!(
            excess.nodes,
            [
                NodeFact::Read(Resolution::Parameter(0)),
                NodeFact::Read(Resolution::Parameter(1)),
                NodeFact::Operate { op: ProjectedOp::Gt, operands: Int, result: Bool },
                NodeFact::Read(Resolution::Parameter(0)),
                NodeFact::Read(Resolution::Parameter(1)),
                NodeFact::Operate { op: ProjectedOp::Sub, operands: Int, result: Int },
                NodeFact::Literal(0),
                NodeFact::Branch,
            ]
        );
        assert_eq!(
            surplus.nodes,
            [
                NodeFact::Read(Resolution::LetBinding(0)),
                NodeFact::Literal(9),
                NodeFact::Call { callee: 0, arguments: 2 },
            ]
        );
    }

    #[test]
    fn exported_facts_are_tied_to_their_source_document() {
        let fixture = fixture_of(CALLS_SOURCE);
        let foreign = IrId::from_raw(2).unwrap();
        let other = export_facts_document(&fixture.parsed.arena, &fixture.published, foreign).unwrap();
        assert_ne!(
            encode_facts_document(&other).unwrap(),
            encode_facts_document(&fixture.document).unwrap()
        );
        let changed = fixture_of(&fixture.source.text.replace("value - limit", "value * limit"));
        assert_ne!(changed.document, fixture.document);
    }

    #[test]
    fn a_wrong_checker_fact_changes_the_export() {
        let fixture = fixture_of(CALLS_SOURCE);
        let baseline = encode_facts_document(&fixture.document).unwrap();
        let ast = &fixture.parsed.arena.arena;
        let expression_of = |matches: &dyn Fn(&ArenaExprKind) -> bool, occurrence: usize| {
            (0..ast.expr_tags.len())
                .map(ExprId::from_index)
                .filter(|id| matches(&ast.expr(*id).kind))
                .nth(occurrence)
                .expect("fixture expression")
        };
        let export = |published: &PublishedFacts| {
            export_facts_document(&fixture.parsed.arena, published, fixture.source.id)
                .and_then(|document| encode_facts_document(&document))
        };
        let exported = |mutate: &dyn Fn(&mut PublishedFacts)| {
            let mut published = fixture.published.clone();
            mutate(&mut published);
            export(&published)
        };
        let changes = |name: &str, mutate: &dyn Fn(&mut PublishedFacts)| match exported(mutate) {
            Ok(text) => assert_ne!(text, baseline, "{name} must change the export"),
            Err(error) => panic!("{name} must export a different document, not fail: {error}"),
        };
        let refuses = |name: &str, reason: &str, mutate: &dyn Fn(&mut PublishedFacts)| match exported(mutate) {
            Ok(_) => panic!("{name} must be refused"),
            Err(error) => assert!(error.contains(reason), "{name} was refused for {error:?}"),
        };
        let identifier = |occurrence| expression_of(&|kind| matches!(kind, ArenaExprKind::Ident(_)), occurrence);
        let binary = |occurrence| expression_of(&|kind| matches!(kind, ArenaExprKind::Binary { .. }), occurrence);

        for (name, kind) in [
            ("a call result typed as Bool", 0),
            ("a conditional result typed as Bool", 1),
            ("a value block result typed as Bool", 2),
            ("a call argument typed as Bool", 3),
        ] {
            let expression = match kind {
                0 => expression_of(&|node| matches!(node, ArenaExprKind::Call { .. }), 0),
                1 => expression_of(&|node| matches!(node, ArenaExprKind::If { .. }), 0),
                2 => expression_of(&|node| matches!(node, ArenaExprKind::ValueBlock(_)), 0),
                _ => {
                    let call = expression_of(&|node| matches!(node, ArenaExprKind::Call { .. }), 0);
                    let ArenaExprKind::Call { args, .. } = ast.expr(call).kind else { unreachable!() };
                    let crate::syntax::arena::ArenaCallArgKind::Positional(value) = ast.call_args(args)[0].kind
                    else { unreachable!() };
                    value
                }
            };
            refuses(name, "checked as Int", &|published| {
                published.declarations.bodies.expr_types.insert(expression, Type::Bool);
            });
        }
        let body = ast.block(ast.function_def(FunctionDefId::from_index(1)).body);
        let initializer = ast.stmt_ids(body.statements).find_map(|statement| {
            match ast.stmt(statement).kind {
                ArenaStmtKind::Let { initializer: ArenaExprOrRun::Expr(value), .. } => Some(value),
                _ => None,
            }
        }).unwrap();
        refuses("a let initializer typed as Bool", "checked as Int", &|published| {
            published.declarations.bodies.expr_types.insert(initializer, Type::Bool);
        });

        changes("a parameter read resolved to the other parameter", &|published| {
            let bindings = std::sync::Arc::make_mut(&mut published.declarations.bodies.resolved_bindings);
            let other = bindings.expressions[&identifier(1)];
            bindings.expressions.insert(identifier(0), other);
        });
        refuses("a name resolved to another function's binding", "outside the function", &|published| {
            let bindings = std::sync::Arc::make_mut(&mut published.declarations.bodies.resolved_bindings);
            let parameter = bindings.expressions[&identifier(0)];
            let last = (0..ast.expr_tags.len())
                .map(ExprId::from_index)
                .filter(|id| bindings.expressions.contains_key(id))
                .last()
                .unwrap();
            bindings.expressions.insert(last, parameter);
        });
        changes("a comparison typed as Int", &|published| {
            published.declarations.bodies.expr_types.insert(binary(0), Type::Int);
        });
        refuses("an operand typed as Bool", "checked as Int", &|published| {
            published.declarations.bodies.expr_types.insert(identifier(0), Type::Bool);
        });
        changes("a subtraction recorded as an addition", &|published| {
            published.binary_operators.insert(binary(1), BinaryOp::Add);
        });
        refuses("a call selecting another function", "published arity", &|published| {
            let call = expression_of(&|kind| matches!(kind, ArenaExprKind::Call { .. }), 0);
            let target = published.declarations.bodies.selected_function_calls.get_mut(&call).unwrap();
            target.definition = FunctionDefId::from_index(1);
        });
        refuses("a callee published with another arity", "disagrees with the declared parameters", &|published| {
            let name = ast.function_def(FunctionDefId::from_index(0)).name;
            let signature = published.declarations.pures.get_mut(&name).unwrap();
            signature.params.pop();
        });
        changes("a parameter typed as Bool in the signature", &|published| {
            let name = ast.function_def(FunctionDefId::from_index(0)).name;
            published.declarations.pures.get_mut(&name).unwrap().params[0].ty = Type::Bool;
        });
        changes("a result typed as Bool in the signature", &|published| {
            let name = ast.function_def(FunctionDefId::from_index(1)).name;
            published.declarations.pures.get_mut(&name).unwrap().return_ty = Type::Bool;
        });
        changes("a let typed as Bool", &|published| {
            for ty in published.declarations.local_binding_types.values_mut() {
                *ty = Type::Bool;
            }
        });
        changes("an effect fact the function does not have", &|published| {
            let body = ast.span(ast.block(ast.function_def(FunctionDefId::from_index(0)).body).span);
            published.declarations.function_effect_facts.insert(
                EffectDeclarationId { namespace: None, body },
                crate::sema::check::FunctionEffectFact {
                    effective: crate::sema::check::CheckedEffects::finite([
                        crate::syntax::node::Effect::Io,
                    ]),
                    ..Default::default()
                },
            );
        });
        changes("a declared effect in the signature", &|published| {
            let name = ast.function_def(FunctionDefId::from_index(0)).name;
            published.declarations.pures.get_mut(&name).unwrap().effects =
                Some(vec![crate::syntax::node::Effect::Io]);
        });
        refuses("an effect fact with an unknown contract", "effects are unknown", &|published| {
            let body = ast.span(ast.block(ast.function_def(FunctionDefId::from_index(0)).body).span);
            published.declarations.function_effect_facts.insert(
                EffectDeclarationId { namespace: None, body },
                crate::sema::check::FunctionEffectFact::default(),
            );
        });
        refuses("a missing signature", "no signature", &|published| {
            published.declarations.pures.clear();
        });
    }

    #[test]
    fn effects_are_the_sorted_union_of_the_signature_and_the_effect_fact() {
        use crate::syntax::node::Effect;
        let fixture = fixture_of(CALLS_SOURCE);
        let ast = &fixture.parsed.arena.arena;
        let definition = ast.function_def(FunctionDefId::from_index(0));
        let mut published = fixture.published.clone();
        published.declarations.pures.get_mut(&definition.name).unwrap().effects =
            Some(vec![Effect::Time, Effect::Io]);
        published.declarations.function_effect_facts.insert(
            EffectDeclarationId { namespace: None, body: ast.span(ast.block(definition.body).span) },
            crate::sema::check::FunctionEffectFact {
                effective: crate::sema::check::CheckedEffects::finite([Effect::Io, Effect::Fs]),
                ..Default::default()
            },
        );
        let document = export_facts_document(&fixture.parsed.arena, &published, fixture.source.id).unwrap();
        assert_eq!(document.functions[0].effects, ["fs", "io", "time"]);
        assert!(document.functions[1].effects.is_empty());
    }

    #[test]
    fn a_wrong_exported_document_is_not_the_recorded_one() {
        let fixture = fixture_of(CALLS_SOURCE);
        let baseline = encode_facts_document(&fixture.document).unwrap();
        let change = |mutate: &dyn Fn(&mut FactsDocument)| {
            let mut document = fixture.document.clone();
            mutate(&mut document);
            encode_facts_document(&document).map_or(true, |text| text != baseline)
        };
        assert!(change(&|document| document.functions[0].nodes.swap(0, 1)));
        assert!(change(&|document| document.functions[0].nodes.pop().map(|_| ()).unwrap_or(())));
        assert!(change(&|document| document.functions[1].nodes[2] = NodeFact::Call { callee: 0, arguments: 1 }));
        assert!(change(&|document| document.functions[1].effects.push("io".to_string())));
        assert!(change(&|document| document.functions.reverse()));
    }

    fn wire_at<'a>(value: &'a mut WireValue, path: &[&str]) -> &'a mut WireValue {
        let Some((step, rest)) = path.split_first() else {
            return value;
        };
        let next = match step.parse::<usize>() {
            Ok(index) => {
                let WireValue::Array(items) = value else { panic!("test path expects an array") };
                &mut items[index]
            }
            Err(_) => object_field(value, step),
        };
        wire_at(next, rest)
    }

    #[test]
    fn wire_corruptions_are_rejected_by_the_facts_decoder() {
        let fixture = fixture_of(CALLS_SOURCE);
        let text = encode_facts_document(&fixture.document).unwrap();
        let reject = |name: &str, path: &[&str], replacement: &str| {
            let corrupted = parsed_raw_mutation(&text, |value| {
                let (field, parent) = path.split_last().unwrap();
                set_field(wire_at(value, parent), field, WireValue::String(replacement.to_string()));
            });
            assert!(decode_facts_document(&corrupted).is_err(), "{name} must be rejected");
        };
        reject("unknown schema", &["schema"], "xsh-checked-facts-v2");
        reject("unknown scope", &["scope"], "integer-calls-conditional-local-fixture-v3");
        reject("source id zero", &["sourceId"], &hex_u32(0));
        reject("unknown node kind", &["functions", "0", "nodes", "0", "kind"], "write");
        reject("read with an unknown resolution", &["functions", "0", "nodes", "0", "resolution"], "global");
        reject(
            "operation with another operation's ordinal",
            &["functions", "0", "nodes", "2", "ordinal"],
            &hex_u32(ProjectedOp::Mul.ordinal()),
        );
        reject("unknown operation", &["functions", "0", "nodes", "2", "op"], "Pow");
        reject("unknown type", &["functions", "0", "nodes", "2", "result"], "Str");
        reject("literal with a short word", &["functions", "0", "nodes", "6", "value"], "00");
        reject("call to a later function", &["functions", "1", "nodes", "2", "callee"], &hex_u32(1));
        reject("read of a parameter the function lacks", &["functions", "0", "nodes", "0", "index"], &hex_u32(5));
        reject("duplicate function name", &["functions", "1", "name"], "excess");
        reject("effects that are not a list", &["functions", "0", "effects"], "io");
        reject("uppercase hex word", &["functions", "1", "nodes", "2", "arguments"], "0000000A");

        let unknown_field = parsed_raw_mutation(&text, |value| {
            insert_field(value, "unrecognized", WireValue::String("rejected".to_string()))
        });
        assert!(decode_facts_document(&unknown_field).is_err());
        let node_with_extra_field = parsed_raw_mutation(&text, |value| {
            insert_field(wire_at(value, &["functions", "0", "nodes", "0"]), "extra", WireValue::String(hex_u32(0)))
        });
        assert!(decode_facts_document(&node_with_extra_field).is_err());
    }
}
