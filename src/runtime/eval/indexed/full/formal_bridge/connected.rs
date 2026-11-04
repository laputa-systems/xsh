use super::*;

use crate::sema::check::{BindingId, BindingKind, CompactDeclOutput};
use crate::sema::schemes::{CallableRequirements, InstantiationRole, SchemeType, SealedOperationBinding};
use crate::sema::sealed_operation::{SealedOperation, SealedOperationSignature};
use crate::sema::types::Type;
use crate::sema::value_materialization::InstantiationEvidence;
use crate::syntax::arena::{ArenaExprKind, ArenaProgram, ExprId};

#[derive(Clone, Debug, Eq, PartialEq)]
enum CheckedType {
    Parameter(u32), Int, Bool, Str, Error, Unit,
    Record(BTreeMap<String, CheckedType>),
    Result(Box<CheckedType>, Box<CheckedType>),
}

fn ground_type(ty: &Type) -> Result<CheckedType, String> {
    match ty {
        Type::Int => Ok(CheckedType::Int),
        Type::Bool => Ok(CheckedType::Bool),
        Type::Str => Ok(CheckedType::Str),
        Type::Error => Ok(CheckedType::Error),
        Type::Unit => Ok(CheckedType::Unit),
        Type::Record(fields) => fields.iter().map(|(name, value)|
            Ok((name_text(*name), ground_type(value)?))).collect::<Result<_, String>>().map(CheckedType::Record),
        Type::Result(value, error) => Ok(CheckedType::Result(Box::new(ground_type(value)?), Box::new(ground_type(error)?))),
        _ => Err(format!("unsupported ground type {ty:?}")),
    }
}

fn checked_type(ty: &SchemeType) -> Result<CheckedType, String> {
    match ty {
        SchemeType::Ground(value) => ground_type(value),
        SchemeType::Parameter(id) => Ok(CheckedType::Parameter(id.0)),
        SchemeType::Result(value, error) => Ok(CheckedType::Result(Box::new(checked_type(value)?), Box::new(checked_type(error)?))),
        _ => Err(format!("unsupported scheme type {ty:?}")),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum EffectFact { Unknown, Known(Vec<String>) }

#[derive(Clone, Debug, Eq, PartialEq)]
struct FieldFact { receiver: CheckedType, name: String, result: CheckedType, span: Span }

#[derive(Clone, Debug, Eq, PartialEq)]
enum OperationFact {
    Field(FieldFact),
    StoreValue { storage: CheckedType, replacement: CheckedType, span: Span },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReadBinding {
    name: String,
    span: Span,
    namespace: Option<String>,
    global: bool,
    exported: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BodyFacts {
    span: Span,
    kind: crate::sema::types::CallableKind,
    type_parameters: Vec<u32>,
    parameters: Vec<(String, CheckedType)>,
    result: CheckedType,
    explicit_return: bool,
    effects: EffectFact,
    inferred_effects: bool,
    operations: Vec<OperationFact>,
    expression_types: BTreeMap<usize, CheckedType>,
    expression_sites: BTreeMap<usize, Span>,
    tail_types: BTreeMap<usize, CheckedType>,
    statement_sites: BTreeMap<usize, Span>,
    read_bindings: Vec<ReadBinding>,
    resolved_reads: BTreeMap<usize, ReadBinding>,
    resolved_tails: BTreeMap<usize, ReadBinding>,
    operation_sites: Vec<(usize, u32)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum OperationBinding { Field, StoreValue }

#[derive(Clone, Debug, Eq, PartialEq)]
struct FunctionTarget {
    definition: usize,
    namespace: Option<String>,
    name: String,
    body: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CallFacts {
    expression: usize,
    span: Span,
    role: InstantiationRole,
    target: FunctionTarget,
    type_arguments: BTreeMap<u32, CheckedType>,
    operation_bindings: Vec<OperationBinding>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ConnectedFacts { functions: Vec<FunctionTarget>, bodies: Vec<BodyFacts>, calls: Vec<CallFacts> }

fn export_connected_facts(arena: &ArenaProgram, declarations: &CompactDeclOutput) -> Result<ConnectedFacts, String> {
    if !arena.modules.is_empty() || arena.root_nominal_namespace.is_some() {
        return Err("module-qualified source identity is unsupported".into());
    }
    let functions = arena.arena.function_defs.iter().enumerate().map(|(index, definition)| FunctionTarget {
        definition: index, namespace: None, name: name_text(definition.name),
        body: arena.arena.span(arena.arena.block(definition.body).span),
    }).collect::<Vec<_>>();
    if !declarations.function_schemes.is_empty() {
        return Err("qualified function schemes require qualified source identity".into());
    }
    let mut bodies = Vec::new();
    for (&span, facts) in &declarations.scheme_bodies {
        let scheme = facts.scheme();
        if !scheme.row_parameters().is_empty() || scheme.return_schema().is_some() ||
            facts.guard_scopes() != &Default::default() || !facts.noncompleting_expressions().is_empty() ||
            !facts.constructor_producers().is_empty() || !facts.constructor_instances().is_empty() ||
            !facts.registered_calls().is_empty() || facts.proc_return_site_count() != 0 {
            return Err("unsupported scheme body facts".into());
        }
        let mut rest = scheme.callable().requirements().clone();
        let operations = std::mem::take(&mut rest.operations);
        if rest != CallableRequirements::default() {
            return Err("unsupported callable requirements".into());
        }
        let operations = operations.into_iter().map(|operation| match operation {
            SealedOperation::Field { receiver, name, result, span } => Ok(OperationFact::Field(FieldFact {
                receiver: checked_type(&receiver)?, name: name_text(name), result: checked_type(&result)?, span,
            })),
            SealedOperation::StoreValue { storage, replacement, span } => Ok(OperationFact::StoreValue {
                storage: checked_type(&storage)?, replacement: checked_type(&replacement)?, span,
            }),
            _ => Err("unsupported sealed operation".to_string()),
        }).collect::<Result<Vec<_>, _>>()?;
        let parameters = scheme.params().iter().map(|parameter| {
            if parameter.defaulted || parameter.rest || parameter.schema_expectation.is_some() {
                return Err("unsupported parameter contract".to_string());
            }
            Ok((name_text(parameter.name), checked_type(&parameter.ty)?))
        }).collect::<Result<Vec<_>, String>>()?;
        let effects = match scheme.effects().concrete().map_err(|_| "symbolic effect contract")? {
            None => EffectFact::Unknown,
            Some(effects) => EffectFact::Known(effects.iter().map(|effect| effect.as_str().to_string()).collect()),
        };
        let expression_sites = facts.expression_types().keys().map(|id| {
            if id.index() >= arena.arena.expr_tags.len() {
                return Err("scheme expression id is outside the arena".to_string());
            }
            Ok((id.index(), arena.arena.expr(*id).span))
        }).collect::<Result<BTreeMap<_, _>, String>>()?;
        let statement_sites = facts.tail_types().keys().map(|id| {
            if id.index() >= arena.arena.stmt_tags.len() {
                return Err("scheme statement id is outside the arena".to_string());
            }
            Ok((id.index(), arena.arena.stmt(*id).span))
        }).collect::<Result<BTreeMap<_, _>, String>>()?;
        let mut operation_sites = Vec::new();
        for index in 0..arena.arena.expr_tags.len() {
            let expression = ExprId::from_index(index);
            if let Some(operation) = facts.operation_site(expression) {
                let Some(OperationFact::Field(field)) = operations.get(operation as usize) else {
                    return Err("body operation site names no scheme requirement".into());
                };
                if field.span != arena.arena.expr(expression).span {
                    return Err("body operation site span disagrees with expression".into());
                }
                match arena.arena.expr(expression).kind {
                    ArenaExprKind::Field { name, .. } if name_text(name) == field.name => {}
                    _ => return Err("body field requirement disagrees with source expression".into()),
                }
                operation_sites.push((index, operation));
            }
            if let Some(operation) = facts.store_value_site(expression) {
                let Some(OperationFact::StoreValue { span, .. }) = operations.get(operation as usize) else {
                    return Err("store site names no store requirement".into());
                };
                if *span != arena.arena.expr(expression).span {
                    return Err("store site disagrees with expression".into());
                }
                operation_sites.push((index, operation));
            }
            if facts.compound_site(expression).is_some() || facts.index_store_site(expression).is_some() ||
                facts.method_site(expression).is_some() ||
                facts.wrapper_site(expression).is_some() ||
                facts.map_key_site(arena.arena.expr(expression).span).is_some() {
                return Err("unsupported body operation site".into());
            }
        }
        let read_binding = |id: &BindingId| {
            let declaration = declarations.bodies.resolved_bindings.declarations.get(id)
                .ok_or_else(|| "read binding has no declaration".to_string())?;
            if declaration.kind != BindingKind::Runtime {
                return Err("unsupported read binding kind".to_string());
            }
            Ok(ReadBinding {
                name: name_text(declaration.name), span: declaration.span,
                namespace: declaration.namespace.map(name_text), global: declaration.global,
                exported: declaration.exported,
            })
        };
        let mut read_bindings = facts.read_bindings().iter().map(&read_binding)
            .collect::<Result<Vec<_>, String>>()?;
        let resolved_reads = facts.expression_types().keys().filter_map(|expression|
            declarations.bodies.resolved_bindings.expressions.get(expression)
                .map(|binding| (expression, binding)))
            .map(|(expression, binding)| Ok((expression.index(), read_binding(binding)?)))
            .collect::<Result<BTreeMap<_, _>, String>>()?;
        let resolved_tails = facts.tail_types().keys().filter_map(|statement|
            declarations.bodies.resolved_bindings.tails.get(statement)
                .map(|binding| (statement, binding)))
            .map(|(statement, binding)| Ok((statement.index(), read_binding(binding)?)))
            .collect::<Result<BTreeMap<_, _>, String>>()?;
        read_bindings.sort_by_key(|binding| (binding.span, binding.name.clone(), binding.namespace.clone()));
        if read_bindings.windows(2).any(|pair| pair[0].span == pair[1].span &&
            pair[0].name == pair[1].name && pair[0].namespace == pair[1].namespace) {
            return Err("read binding identities are not unique".into());
        }
        bodies.push(BodyFacts {
            span, kind: scheme.kind(), type_parameters: scheme.type_parameters().iter().map(|id| id.0).collect(),
            parameters, result: checked_type(scheme.return_type())?, explicit_return: scheme.explicit_return(),
            effects, inferred_effects: scheme.inferred_effects(), operations,
            expression_types: facts.expression_types().iter().map(|(id, ty)| Ok((id.index(), checked_type(ty)?))).collect::<Result<_, String>>()?,
            expression_sites, tail_types: facts.tail_types().iter().map(|(id, ty)| Ok((id.index(), checked_type(ty)?))).collect::<Result<_, String>>()?,
            statement_sites, read_bindings, resolved_reads, resolved_tails, operation_sites,
        });
    }
    let mut calls = Vec::new();
    for (site, evidence) in &declarations.bodies.call_instantiations {
        if site.source.index() >= arena.arena.expr_tags.len() {
            return Err("instantiation expression id is outside the arena".into());
        }
        let InstantiationEvidence::Invocation(instance) = evidence else {
            return Err("value materialization is unsupported".into());
        };
        if !instance.row_arguments().is_empty() || !instance.effect_arguments().is_empty() ||
            !instance.application_bindings().is_empty() || !instance.method_bindings().is_empty() ||
            !instance.wrapper_bindings().is_empty() || !instance.guard_bindings().is_empty() ||
            !instance.guard_call_bindings().is_empty() {
            return Err("unsupported call instantiation evidence".into());
        }
        if declarations.bodies.scheme_callable_calls.contains_key(site) {
            return Err("scheme callable relationships are unsupported".into());
        }
        let target = declarations.bodies.selected_function_calls.get(&site.source)
            .ok_or_else(|| "instantiation has no selected function target".to_string())?;
        if target.namespace.is_some() || target.definition.index() >= arena.arena.function_defs.len() {
            return Err("qualified or foreign function target is unsupported".into());
        }
        let definition = arena.arena.function_def(target.definition);
        if definition.name != target.name {
            return Err("selected function identity disagrees with source declaration".into());
        }
        let body = arena.arena.span(arena.arena.block(definition.body).span);
        if !declarations.scheme_bodies.contains_key(&body) {
            return Err("selected generic function has no body scheme".into());
        }
        let target = FunctionTarget {
            definition: target.definition.index(), namespace: target.namespace.map(name_text),
            name: name_text(target.name), body,
        };
        let operation_bindings = instance.operation_bindings().iter().map(|binding| match binding {
            SealedOperationBinding::Selected(SealedOperationSignature::Field) => Ok(OperationBinding::Field),
            SealedOperationBinding::Selected(SealedOperationSignature::StoreValue) => Ok(OperationBinding::StoreValue),
            _ => Err("unsupported operation binding".to_string()),
        }).collect::<Result<_, _>>()?;
        calls.push(CallFacts {
            expression: site.source.index(), span: arena.arena.expr(site.source).span, role: site.role, target,
            type_arguments: instance.type_arguments().iter().map(|(id, ty)| Ok((id.0, checked_type(ty)?))).collect::<Result<_, String>>()?,
            operation_bindings,
        });
    }
    calls.sort_by_key(|call| (call.expression, call.role));
    Ok(ConnectedFacts { functions, bodies, calls })
}

fn wire_index(index: usize) -> Result<WireValue, String> {
    let index = u32::try_from(index).map_err(|_| "wire index exceeds u32".to_string())?;
    Ok(wire_string(hex_u32(index)))
}

fn wire_span(span: Span) -> Result<WireValue, String> {
    Ok(wire_object([
        ("source", wire_index(span.source_id.raw())?),
        ("start", wire_index(span.start())?),
        ("length", wire_index(span.end() - span.start())?),
    ]))
}

fn wire_checked_type(ty: &CheckedType) -> WireValue {
    match ty {
        CheckedType::Parameter(id) => wire_object([("kind", wire_string("parameter")), ("id", wire_string(hex_u32(*id)))]),
        CheckedType::Int => wire_object([("kind", wire_string("int"))]),
        CheckedType::Bool => wire_object([("kind", wire_string("bool"))]),
        CheckedType::Str => wire_object([("kind", wire_string("str"))]),
        CheckedType::Error => wire_object([("kind", wire_string("error"))]),
        CheckedType::Unit => wire_object([("kind", wire_string("unit"))]),
        CheckedType::Record(fields) => wire_object([("kind", wire_string("record")), ("fields", WireValue::Array(
            fields.iter().map(|(name, ty)| wire_object([("name", wire_string(name)), ("type", wire_checked_type(ty))])).collect()))]),
        CheckedType::Result(value, error) => wire_object([("kind", wire_string("result")),
            ("value", wire_checked_type(value)), ("error", wire_checked_type(error))]),
    }
}

fn wire_read(binding: &ReadBinding) -> Result<WireValue, String> {
    Ok(wire_object([
        ("name", wire_string(&binding.name)), ("span", wire_span(binding.span)?),
        ("namespace", WireValue::Array(binding.namespace.iter().map(wire_string).collect())),
        ("global", wire_string(binding.global.to_string())), ("exported", wire_string(binding.exported.to_string())),
    ]))
}

fn wire_target(target: &FunctionTarget) -> Result<WireValue, String> {
    Ok(wire_object([
        ("definition", wire_index(target.definition)?), ("name", wire_string(&target.name)),
        ("namespace", WireValue::Array(target.namespace.iter().map(wire_string).collect())),
        ("body", wire_span(target.body)?),
    ]))
}

fn wire_effect_contract(effects: &crate::sema::effects::CheckedEffects) -> WireValue {
    use crate::sema::effects::EffectContract;
    match effects.contract() {
        EffectContract::Unknown => wire_object([
            ("kind", wire_string("unknown")),
            ("direct", WireValue::Array(vec![])),
            ("variables", WireValue::Array(vec![])),
        ]),
        EffectContract::Known(expression) => wire_object([
            ("kind", wire_string("known")),
            ("direct", WireValue::Array(expression.direct().iter().map(|effect| wire_string(effect.as_str())).collect())),
            ("variables", WireValue::Array(expression.variables().map(|(id, mask)| wire_object([
                ("id", wire_string(hex_u32(id.0))),
                ("mask", WireValue::Array(mask.iter().map(|effect| wire_string(effect.as_str())).collect())),
            ])).collect())),
        ]),
    }
}

fn wire_effect_identity(identity: crate::sema::check::EffectDeclarationId) -> Result<WireValue, String> {
    Ok(wire_object([
        ("namespace", WireValue::Array(identity.namespace.into_iter().map(|name| wire_string(name_text(name))).collect())),
        ("body", wire_span(identity.body)?),
    ]))
}

fn wire_declaration_effects(
    facts: &BTreeMap<crate::sema::check::EffectDeclarationId, crate::sema::check::FunctionEffectFact>,
    source: SourceId,
) -> Result<WireValue, String> {
    let declarations = facts.iter().filter(|(identity, _)| identity.body.source_id == source)
        .map(|(identity, fact)| {
            let owner = fact.scope.owner().map(wire_effect_identity).transpose()?.into_iter().collect();
            Ok(wire_object([
                ("identity", wire_effect_identity(*identity)?),
                ("effective", wire_effect_contract(&fact.effective)),
                ("required", wire_effect_contract(&fact.required)),
                ("owner", WireValue::Array(owner)),
                ("parameters", WireValue::Array(fact.scope.parameters().iter().map(|id| wire_string(hex_u32(id.0))).collect())),
                ("inferred", wire_string(fact.inferred.to_string())),
                ("inferenceAllowed", wire_string(fact.inference_allowed.to_string())),
                ("redundantClause", wire_string(fact.redundant_clause.to_string())),
                ("unknownChain", WireValue::Array(fact.unknown_chain.iter().map(wire_string).collect())),
            ]))
        }).collect::<Result<Vec<_>, String>>()?;
    Ok(wire_object([
        ("schema", wire_string("xsh-connected-declaration-effects-v1")),
        ("declarations", WireValue::Array(declarations)),
    ]))
}

fn connected_wire(facts: &ConnectedFacts) -> Result<WireValue, String> {
    let functions = facts.functions.iter().map(wire_target).collect::<Result<Vec<_>, _>>()?;
    let bodies = facts.bodies.iter().map(|body| {
        let effects = match &body.effects {
            EffectFact::Unknown => wire_object([("kind", wire_string("unknown")), ("effects", WireValue::Array(vec![]))]),
            EffectFact::Known(effects) => wire_object([("kind", wire_string("known")),
                ("effects", WireValue::Array(effects.iter().map(wire_string).collect()))]),
        };
        let operations = body.operations.iter().map(|operation| match operation {
            OperationFact::Field(field) => Ok(wire_object([
                ("kind", wire_string("field")), ("receiver", wire_checked_type(&field.receiver)),
                ("name", wire_string(&field.name)), ("result", wire_checked_type(&field.result)),
                ("span", wire_span(field.span)?),
            ])),
            OperationFact::StoreValue { storage, replacement, span } => Ok(wire_object([
                ("kind", wire_string("storeValue")), ("storage", wire_checked_type(storage)),
                ("replacement", wire_checked_type(replacement)), ("span", wire_span(*span)?),
            ])),
        }).collect::<Result<Vec<_>, String>>()?;
        let expressions = body.expression_types.iter().map(|(id, ty)| Ok(wire_object([
            ("id", wire_index(*id)?), ("span", wire_span(*body.expression_sites.get(id).ok_or("expression span missing")?)?),
            ("type", wire_checked_type(ty)),
        ]))).collect::<Result<Vec<_>, String>>()?;
        let tails = body.tail_types.iter().map(|(id, ty)| Ok(wire_object([
            ("id", wire_index(*id)?), ("span", wire_span(*body.statement_sites.get(id).ok_or("statement span missing")?)?),
            ("type", wire_checked_type(ty)),
        ]))).collect::<Result<Vec<_>, String>>()?;
        let reads = body.resolved_reads.iter().map(|(id, binding)| Ok(wire_object([
            ("expression", wire_index(*id)?), ("binding", wire_read(binding)?),
        ]))).collect::<Result<Vec<_>, String>>()?;
        let tails_resolved = body.resolved_tails.iter().map(|(id, binding)| Ok(wire_object([
            ("statement", wire_index(*id)?), ("binding", wire_read(binding)?),
        ]))).collect::<Result<Vec<_>, String>>()?;
        let sites = body.operation_sites.iter().map(|(id, ordinal)| Ok(wire_object([
            ("expression", wire_index(*id)?), ("operation", wire_string(hex_u32(*ordinal))),
        ]))).collect::<Result<Vec<_>, String>>()?;
        let kind = match body.kind {
            crate::sema::types::CallableKind::Pure => "pure",
            crate::sema::types::CallableKind::Proc => "proc",
            crate::sema::types::CallableKind::Stream => "stream",
        };
        Ok(wire_object([
            ("span", wire_span(body.span)?), ("kind", wire_string(kind)),
            ("typeParameters", WireValue::Array(body.type_parameters.iter().map(|id| wire_string(hex_u32(*id))).collect())),
            ("parameters", WireValue::Array(body.parameters.iter().map(|(name, ty)|
                wire_object([("name", wire_string(name)), ("type", wire_checked_type(ty))])).collect())),
            ("result", wire_checked_type(&body.result)), ("explicitReturn", wire_string(body.explicit_return.to_string())),
            ("effects", effects), ("inferredEffects", wire_string(body.inferred_effects.to_string())),
            ("operations", WireValue::Array(operations)), ("expressionTypes", WireValue::Array(expressions)),
            ("tailTypes", WireValue::Array(tails)),
            ("readBindings", WireValue::Array(body.read_bindings.iter().map(wire_read).collect::<Result<_, _>>()?)),
            ("resolvedReads", WireValue::Array(reads)), ("resolvedTails", WireValue::Array(tails_resolved)),
            ("operationSites", WireValue::Array(sites)),
        ]))
    }).collect::<Result<Vec<_>, String>>()?;
    let calls = facts.calls.iter().map(|call| {
        let role = match call.role { InstantiationRole::Call => "call", InstantiationRole::Value => "value" };
        Ok(wire_object([
            ("expression", wire_index(call.expression)?), ("span", wire_span(call.span)?), ("role", wire_string(role)),
            ("target", wire_target(&call.target)?),
            ("typeArguments", WireValue::Array(call.type_arguments.iter().map(|(id, ty)| wire_object([
                ("parameter", wire_string(hex_u32(*id))), ("type", wire_checked_type(ty)),
            ])).collect())),
            ("operationBindings", WireValue::Array(call.operation_bindings.iter().map(|binding| match binding {
                OperationBinding::Field => wire_string("field"),
                OperationBinding::StoreValue => wire_string("storeValue"),
            }).collect())),
        ]))
    }).collect::<Result<Vec<_>, String>>()?;
    Ok(wire_object([
        ("schema", wire_string("xsh-connected-field-facts-v1")),
        ("functions", WireValue::Array(functions)),
        ("bodies", WireValue::Array(bodies)), ("calls", WireValue::Array(calls)),
    ]))
}

fn encode_connected_facts(facts: &ConnectedFacts) -> Result<String, String> {
    let document = json_text(&connected_wire(facts)?);
    if document.len() > MAX_TRANSPORT_DOCUMENT_BYTES {
        return Err("connected facts exceed the private transport byte limit".into());
    }
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::schemes::InstantiationRole;

    const SOURCE: &str = "pure project(value) { value.amount }\npure use_project() -> Int { project({amount: 7, enabled: true}) }\nassert use_project() == 7\n";

    #[test]
    fn polymorphic_tail_read_retains_statement_binding_and_call_types() {
        let source = "pure identity(value) { value }\npure one() -> Int { identity(1) }\npure truth() -> Bool { identity(true) }\nassert one() == 1\nassert truth()\n";
        let (_, _, parsed, published) = check_fixture(source).unwrap();
        let facts = export_connected_facts(&parsed.arena, &published.declarations).unwrap();
        println!("CONNECTED_IDENTITY_EFFECT_FACTS {:?}", published.declarations.function_effect_facts);
        let document = encode_connected_facts(&facts).unwrap();
        println!("CONNECTED_IDENTITY_FACTS {document}");
        let lean_documents = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"),
            "/formal/Xsh/Conformance/Connected/Documents.lean")).unwrap();
        assert_eq!(lean_document_from(&lean_documents, "identityFactsCodes").as_deref(), Some(document.as_str()));
        let [body] = facts.bodies.as_slice() else { panic!("one generic body") };
        assert!(body.expression_types.is_empty());
        assert_eq!(body.tail_types.len(), 1);
        assert_eq!(body.resolved_tails.len(), 1);
        assert_eq!(body.resolved_tails.values().next().unwrap(), &body.read_bindings[0]);
        let actual = facts.calls.iter().map(|call| call.type_arguments.get(&0)).collect::<Vec<_>>();
        assert_eq!(actual, [Some(&CheckedType::Int), Some(&CheckedType::Bool)]);
    }

    #[test]
    fn declaration_effect_export_retains_checked_contracts() {
        let source = "proc clock() [time] -> Int { time.now() }\n";
        let (_, source_id, _, published) = check_fixture(source).unwrap();
        let facts = &published.declarations.function_effect_facts;
        let declaration = facts.iter().find(|(identity, _)| identity.body.source_id == source_id).unwrap();
        assert_eq!(declaration.1.effective.concrete(), Ok(Some(vec![crate::syntax::node::Effect::Time])));
        assert_eq!(declaration.1.required.concrete(), Ok(Some(vec![crate::syntax::node::Effect::Time])));
        let wire = wire_declaration_effects(facts, source_id).unwrap();
        let document = json_text(&wire);
        println!("CONNECTED_CLOCK_EFFECTS {document}");
        let lean_documents = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"),
            "/formal/Xsh/Conformance/Connected/Documents.lean")).unwrap();
        assert_eq!(lean_document_from(&lean_documents, "effectFactsCodes").as_deref(), Some(document.as_str()));
        let mut changed = facts.clone();
        changed.get_mut(declaration.0).unwrap().required = crate::sema::effects::CheckedEffects::finite([]);
        assert_ne!(wire, wire_declaration_effects(&changed, source_id).unwrap());
    }

    #[test]
    fn changing_published_field_sites_and_call_roles_changes_export() {
        let (_, _, parsed, published) = check_fixture(SOURCE).unwrap();
        let baseline = export_connected_facts(&parsed.arena, &published.declarations).unwrap();
        let mut changed = published.declarations.clone();
        let (&span, body) = changed.scheme_bodies.iter().next().unwrap();
        let replaced = body.clone().with_operation_sites(BTreeMap::new()).unwrap();
        changed.scheme_bodies.insert(span, replaced);
        let missing_site = export_connected_facts(&parsed.arena, &changed).unwrap();
        assert_ne!(baseline, missing_site);
        assert_ne!(encode_connected_facts(&baseline).unwrap(), encode_connected_facts(&missing_site).unwrap());

        let mut changed = published.declarations.clone();
        let (&span, body) = changed.scheme_bodies.iter().next().unwrap();
        let field = (0..parsed.arena.arena.expr_tags.len()).map(ExprId::from_index)
            .find(|id| matches!(parsed.arena.arena.expr(*id).kind, ArenaExprKind::Field { .. })).unwrap();
        let mut types = body.expression_types().clone();
        types.insert(field, SchemeType::Parameter(crate::sema::schemes::SchemeTypeParameterId(0)));
        let replacement = crate::sema::schemes::SchemeBodyFacts::new(
            body.scheme().clone(), types, body.tail_types().clone()).unwrap()
            .with_read_bindings(body.read_bindings().clone())
            .with_operation_sites(BTreeMap::from([(field, 0)])).unwrap();
        changed.scheme_bodies.insert(span, replacement);
        let wrong_expression_type = export_connected_facts(&parsed.arena, &changed).unwrap();
        assert_ne!(baseline, wrong_expression_type);
        assert_ne!(encode_connected_facts(&baseline).unwrap(), encode_connected_facts(&wrong_expression_type).unwrap());

        let mut changed = published.declarations.clone();
        let (site, evidence) = changed.bodies.call_instantiations.iter()
            .next().map(|(site, evidence)| (*site, evidence.clone())).unwrap();
        changed.bodies.call_instantiations.remove(&site);
        changed.bodies.call_instantiations.insert(
            crate::sema::schemes::InstantiationSite { source: site.source, role: InstantiationRole::Value }, evidence);
        let changed_role = export_connected_facts(&parsed.arena, &changed).unwrap();
        assert_ne!(baseline, changed_role);
        assert_ne!(encode_connected_facts(&baseline).unwrap(), encode_connected_facts(&changed_role).unwrap());

        let mut changed = published.declarations.clone();
        let target = changed.bodies.selected_function_calls.get_mut(&site.source).unwrap();
        target.name = parsed.arena.arena.function_def(
            crate::syntax::arena::FunctionDefId::from_index(target.definition.index() + 1)).name;
        assert!(export_connected_facts(&parsed.arena, &changed).is_err());
    }

    #[test]
    fn scoped_mutable_binding_facts_preserve_source_sites() {
        let source = "pure scoped(value) { var current = value; without time { current = value }; current }\nassert scoped(7) == 7\n";
        let (_, _, parsed, published) = check_fixture(source).unwrap();
        let facts = export_connected_facts(&parsed.arena, &published.declarations).unwrap();
        let document = encode_connected_facts(&facts).unwrap();
        println!("CONNECTED_SCOPED_FACTS {document}");
        let [body] = facts.bodies.as_slice() else { panic!("one generic body") };
        let binding = body.read_bindings.iter().find(|binding| binding.name == "current").unwrap();
        assert_eq!(&source[binding.span.start()..binding.span.end()], "var current = value;");
        assert!(body.resolved_tails.values().any(|read| read == binding));
        assert!(body.expression_types.keys().any(|id| {
            matches!(parsed.arena.arena.expr(ExprId::from_index(*id)).kind, ArenaExprKind::ValueBlock(_))
        }));
        let [OperationFact::StoreValue { storage, replacement, span }] = body.operations.as_slice() else {
            panic!("one store requirement");
        };
        assert_eq!((storage, replacement), (&CheckedType::Parameter(0), &CheckedType::Parameter(0)));
        assert_eq!(&source[span.start()..span.end()], "value");
        assert_eq!(body.operation_sites.len(), 1);
        assert_eq!(facts.calls[0].operation_bindings, [OperationBinding::StoreValue]);
        let mut changed = published.declarations.clone();
        let (&site, body) = changed.scheme_bodies.iter().next().unwrap();
        changed.scheme_bodies.insert(site, body.clone().with_store_value_sites(BTreeMap::new()).unwrap());
        let missing = export_connected_facts(&parsed.arena, &changed).unwrap();
        assert_ne!(document, encode_connected_facts(&missing).unwrap());
    }

    #[test]
    fn field_and_store_requirements_share_a_source_expression() {
        let source = "pure scoped(value, record) { var current = value; current = record.amount; current }\nassert scoped(1, {amount: 2}) == 2\n";
        let (_, _, parsed, published) = check_fixture(source).unwrap();
        let facts = export_connected_facts(&parsed.arena, &published.declarations).unwrap();
        let [body] = facts.bodies.as_slice() else { panic!("one generic body") };
        let [first, second] = body.operation_sites.as_slice() else { panic!("field and store sites") };
        assert_eq!(first.0, second.0);
        assert_ne!(first.1, second.1);
        assert!(body.operations.iter().any(|operation| matches!(operation, OperationFact::Field(_))));
        assert!(body.operations.iter().any(|operation| matches!(operation, OperationFact::StoreValue { .. })));
        println!("CONNECTED_FIELD_STORE_FACTS {}", encode_connected_facts(&facts).unwrap());
    }

    #[test]
    fn field_scheme_export_retains_body_sites_and_call_evidence() {
        let (_, _, parsed, published) = check_fixture(SOURCE).unwrap();
        let facts = export_connected_facts(&parsed.arena, &published.declarations).unwrap();
        println!("CONNECTED_EFFECT_FACTS {:?}", published.declarations.function_effect_facts);
        let document = encode_connected_facts(&facts).unwrap();
        println!("CONNECTED_FACTS {document}");
        let lean_documents = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"),
            "/formal/Xsh/Conformance/Connected/Documents.lean")).unwrap();
        assert_eq!(lean_document_from(&lean_documents, "connectedFactsCodes").as_deref(), Some(document.as_str()));
        assert_eq!(parse_wire_document(&document).unwrap(), connected_wire(&facts).unwrap());
        assert_eq!(facts.functions.len(), 2);
        assert_eq!(facts.functions[0].name, "project");
        assert_eq!(facts.functions[1].name, "use_project");
        let [body] = facts.bodies.as_slice() else { panic!("one generic body") };
        assert_eq!(body.span.start(), 20);
        assert_eq!(body.span.end(), 36);
        assert_eq!(body.effects, EffectFact::Unknown);
        assert_eq!(body.type_parameters, [0, 1]);
        assert_eq!(body.read_bindings.len(), 1);
        assert_eq!(body.read_bindings[0].name, "value");
        assert_eq!(body.resolved_reads.len(), 1);
        assert_eq!(body.resolved_reads.values().next().unwrap(), &body.read_bindings[0]);
        let [OperationFact::Field(operation)] = body.operations.as_slice() else { panic!("one field requirement") };
        assert_eq!(operation.receiver, CheckedType::Parameter(0));
        assert_eq!(operation.name, "amount");
        assert_eq!(operation.result, CheckedType::Parameter(1));
        assert_eq!(operation.span.start(), 22);
        assert_eq!(operation.span.end(), 34);
        let field = (0..parsed.arena.arena.expr_tags.len())
            .map(ExprId::from_index)
            .find(|id| matches!(parsed.arena.arena.expr(*id).kind, ArenaExprKind::Field { .. }))
            .unwrap();
        assert_eq!(body.expression_types.get(&field.index()), Some(&CheckedType::Parameter(1)));
        assert!(body.operation_sites.contains(&(field.index(), 0)));
        assert_eq!(body.expression_sites.get(&field.index()), Some(&operation.span));
        let [call] = facts.calls.as_slice() else { panic!("one generic call") };
        assert_eq!(call.role, InstantiationRole::Call);
        assert_eq!(call.target.name, "project");
        assert_eq!(call.target, facts.functions[0]);
        assert_eq!(call.target.body, body.span);
        assert_eq!(call.operation_bindings, [OperationBinding::Field]);
        assert_eq!(call.type_arguments.get(&1), Some(&CheckedType::Int));
        let Some(CheckedType::Record(fields)) = call.type_arguments.get(&0) else { panic!("record argument") };
        assert_eq!(fields.get("amount"), Some(&CheckedType::Int));
        assert_eq!(fields.get("enabled"), Some(&CheckedType::Bool));
    }
}
