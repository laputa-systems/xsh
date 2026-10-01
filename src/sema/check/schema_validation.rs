use super::{DeclarationIdentity, ExpressionIdentity, QualifiedNominalIdentity};
use crate::sema::constants::SchemaComponent;
use crate::sema::inference::TypeId;
use std::collections::BTreeMap;
use crate::sema::inference::{Atom, InferenceContext, InferenceError, ScopedRoot, TypeNode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchemaValidationMode { Explicit, Contextual }

/// An application's declaration and arguments survive structural equality.
/// Arguments absent from record fields still distinguish schema applications.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolvedSchemaApplication {
    pub declaration: QualifiedNominalIdentity,
    pub arguments: Vec<TypeId>,
}

/// Application provenance follows the same field and container boundaries as
/// the independently selected schema, including aliases and private owners.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SolvedSchemaExpectation {
    pub applications: Vec<SolvedSchemaApplication>,
    pub children: BTreeMap<SchemaComponent, SolvedSchemaExpectation>,
}

/// Validation consumes the original source value and independently selected
/// target. Assignability alone never certifies that the input meets the schema.
#[derive(Clone, Debug)]
pub struct SolvedSchemaValidation {
    pub input_source: ExpressionIdentity,
    pub input: TypeId,
    pub target: TypeId,
    pub result: TypeId,
    pub caller: Option<DeclarationIdentity>,
    pub mode: SchemaValidationMode,
    pub expectation: SolvedSchemaExpectation,
}

impl super::Checker {
    pub(super) fn retain_schema_expectation(&mut self, arena: &crate::syntax::arena::ArenaProgram, context: &crate::sema::constants::SchemaExpectation, span: crate::source::Span, depth: usize, reifiable: bool) -> Result<SolvedSchemaExpectation, InferenceError> {
        if depth > self.generic.borrow().facts.graph.limits().structural_depth { return Err(InferenceError::Limit("schema context depth")); }
        {
            let mut state = self.generic.borrow_mut();
            state.facts.graph.charge_source_fact_nodes(1 + context.instances.len() as u64)?;
            state.facts.graph.charge_source_fact_edges(context.children.len() as u64 + context.instances.iter().map(|instance| instance.arguments.len() as u64).sum::<u64>())?;
        }
        let mut applications = Vec::with_capacity(context.instances.len());
        for instance in &context.instances {
            let source = self.record_constructors.source_id(instance.definition).ok_or(InferenceError::InvalidScheme)?;
            let namespace = self.record_constructors.namespace(instance.definition).or(arena.root_nominal_namespace);
            let declaration = QualifiedNominalIdentity::Source { source, namespace, declaration: super::NominalDeclaration::Type(instance.definition), member: None };
            let arguments = instance.arguments.iter().map(|argument| self.graph_type(argument, span)).collect::<Result<Vec<_>, _>>()?;
            if reifiable { for &argument in &arguments { self.generic.borrow().facts.graph.export_type(argument)?; } }
            applications.push(SolvedSchemaApplication { declaration, arguments });
        }
        let mut children = BTreeMap::new();
        for (&component, context) in &context.children { children.insert(component, self.retain_schema_expectation(arena, context, span, depth + 1, reifiable)?); }
        Ok(SolvedSchemaExpectation { applications, children })
    }

    pub(super) fn record_graph_schema_validation(&mut self, arena: &crate::syntax::arena::ArenaProgram, expression: crate::syntax::arena::ExprId, value: crate::syntax::arena::ExprId, target: &super::RequirementTarget, mode: SchemaValidationMode) -> Option<super::Type> {
        if !self.graph_generation { return None; }
        let identity = self.expression_identity(arena, expression);
        let input_source = self.expression_identity(arena, value);
        let span = arena.arena.expr(expression).span;
        if let Some(boundary) = self.generic.borrow().facts.schema_validations.get(&identity) { return Some(self.graph_view(boundary.result)); }
        let outcome = (|| {
            let registry_result = self.generic.borrow().facts.registry_boundaries.get(&identity).map(|boundary| boundary.result);
            let target_type = if let Some(result) = registry_result {
                let state = self.generic.borrow();
                let TypeNode::Result(target, _) = state.facts.graph.node(state.facts.graph.resolved(result)?)? else { return Err(InferenceError::InvalidScheme); };
                *target
            } else { self.graph_type(&target.ty, span)? };
            self.generic.borrow().facts.graph.export_type(target_type)?;
            let expectation = self.retain_schema_expectation(arena, &target.context, span, 0, true)?;
            let mut state = self.generic.borrow_mut();
            let graph = &mut state.facts.graph;
            graph.charge_source_fact_nodes(1)?;
            graph.charge_source_fact_edges(4)?;
            let error = graph.atom(Atom::Error)?;
            let result = if let Some(result) = registry_result { result } else { graph.result(target_type, error)? };
            let input = *state.facts.expressions.get(&input_source).ok_or(InferenceError::Boundary("schema validation input has no checked source type"))?;
            state.facts.schema_validations.insert(identity, SolvedSchemaValidation { input_source, input, target: target_type, result, caller: self.current_generic, mode, expectation });
            state.facts.expressions.insert(identity, result);
            if let Some(owner) = self.current_generic { state.facts.expression_owners.insert(identity, owner); }
            drop(state);
            Ok(self.graph_view(result))
        })();
        Some(match outcome { Ok(result) => result, Err(error) => { self.graph_error(span, error); super::Type::Invalid } })
    }
}

impl<Graph> super::SolvedTypes<Graph> {
    pub(super) fn source_application_roots(&self, graph: &InferenceContext) -> Result<Vec<crate::sema::inference::ScopedApplicationRoot>, InferenceError> {
        use crate::sema::inference::{ApplicationCertificate, ApplicationDeclaration, ApplicationPathComponent, ApplicationSource, ScopedApplicationRoot};
        let contexts = self.schema_validations.iter().map(|(&identity, validation)| (identity, validation.caller, &validation.expectation))
            .chain(self.constructor_applications.iter().filter_map(|(&identity, plan)| matches!(plan.authority, super::ConstructorAuthority::Record { .. }).then_some((identity, plan.caller, &plan.expectation))));
        let mut roots = Vec::new();
        for (identity, caller, expectation) in contexts {
            let scope = self.expression_scope(identity, caller)?;
            let mut pending = vec![(expectation, Vec::new())];
            while let Some((context, path)) = pending.pop() {
                if path.len() > graph.limits().structural_depth { return Err(InferenceError::Limit("schema application path depth")); }
                for (ordinal, application) in context.applications.iter().enumerate() {
                    let QualifiedNominalIdentity::Source { source: declaration_source, namespace, declaration: super::NominalDeclaration::Type(declaration), member: None } = application.declaration else { return Err(InferenceError::InvalidScheme); };
                    let source = ApplicationSource { source: identity.source, namespace: identity.namespace, expression: identity.expression, path: path.clone(), ordinal: u32::try_from(ordinal).map_err(|_| InferenceError::Limit("schema application ordinal"))? };
                    roots.push(ScopedApplicationRoot { certificate: ApplicationCertificate { source, declaration: ApplicationDeclaration { source: declaration_source, namespace, declaration }, arguments: application.arguments.clone() }, scope });
                }
                for (&component, child) in &context.children {
                    let component = match component {
                        SchemaComponent::Field(field) => ApplicationPathComponent::Field(field),
                        SchemaComponent::Item => ApplicationPathComponent::Item,
                        SchemaComponent::Value => ApplicationPathComponent::Value,
                        SchemaComponent::Key => ApplicationPathComponent::Key,
                        SchemaComponent::Optional => ApplicationPathComponent::Optional,
                        SchemaComponent::Success => ApplicationPathComponent::Success,
                        SchemaComponent::Error => ApplicationPathComponent::Error,
                    };
                    let mut child_path = path.clone();
                    child_path.push(component);
                    pending.push((child, child_path));
                }
            }
        }
        Ok(roots)
    }

    pub(super) fn schema_validation_payload_bytes(&self) -> usize {
        let mut bytes = 0;
        let mut pending = self.schema_validations.values().map(|boundary| &boundary.expectation).collect::<Vec<_>>();
        while let Some(context) = pending.pop() {
            bytes += context.applications.capacity() * std::mem::size_of::<SolvedSchemaApplication>();
            bytes += context.children.len() * (std::mem::size_of::<SchemaComponent>() + std::mem::size_of::<SolvedSchemaExpectation>() + 3 * std::mem::size_of::<usize>());
            bytes += context.applications.iter().map(|application| application.arguments.capacity() * std::mem::size_of::<TypeId>()).sum::<usize>();
            pending.extend(context.children.values());
        }
        bytes
    }

    pub(super) fn schema_validation_roots(&self, graph: &InferenceContext) -> Result<Vec<ScopedRoot>, InferenceError> {
        let mut roots = Vec::new();
        for (identity, boundary) in &self.schema_validations {
            let scope = self.expression_scope(*identity, boundary.caller)?;
            let input_scope = self.expression_scope(boundary.input_source, boundary.caller)?;
            roots.extend([ScopedRoot { ty: boundary.input, scope: input_scope }, ScopedRoot { ty: boundary.target, scope: None }, ScopedRoot { ty: boundary.result, scope }]);
            let mut pending = vec![(&boundary.expectation, 0)];
            while let Some((context, depth)) = pending.pop() {
                if depth > graph.limits().structural_depth { return Err(InferenceError::Limit("schema context depth")); }
                for application in &context.applications { roots.extend(application.arguments.iter().map(|&ty| ScopedRoot { ty, scope: None })); }
                pending.extend(context.children.values().map(|child| (child, depth + 1)));
            }
        }
        Ok(roots)
    }

    pub(super) fn validate_schema_validations(&self, graph: &InferenceContext) -> Result<(), InferenceError> {
        for (identity, boundary) in &self.schema_validations {
            if identity.source != boundary.input_source.source || identity.namespace != boundary.input_source.namespace || *identity == boundary.input_source
                || self.expression_owners.get(identity).copied() != boundary.caller { return Err(InferenceError::InvalidScheme); }
            self.expression_scope(*identity, boundary.caller)?;
            self.expression_scope(boundary.input_source, boundary.caller)?;
            let original = *self.expressions.get(&boundary.input_source).ok_or(InferenceError::InvalidScheme)?;
            let result = *self.expressions.get(identity).ok_or(InferenceError::InvalidScheme)?;
            if graph.resolved(original)? != graph.resolved(boundary.input)? || graph.resolved(result)? != graph.resolved(boundary.result)? { return Err(InferenceError::InvalidScheme); }
            let TypeNode::Result(target, error) = graph.node(graph.resolved(boundary.result)?)? else { return Err(InferenceError::InvalidScheme); };
            if graph.resolved(*target)? != graph.resolved(boundary.target)? || graph.node(graph.resolved(*error)?)? != &TypeNode::Atom(Atom::Error) { return Err(InferenceError::InvalidScheme); }
            graph.export_type(boundary.target)?;
            let mut pending = vec![(&boundary.expectation, 0)];
            while let Some((context, depth)) = pending.pop() {
                if depth > graph.limits().structural_depth { return Err(InferenceError::Limit("schema context depth")); }
                for application in &context.applications {
                    if !matches!(application.declaration, QualifiedNominalIdentity::Source { declaration: super::NominalDeclaration::Type(_), member: None, .. }) { return Err(InferenceError::InvalidScheme); }
                    for &argument in &application.arguments { graph.export_type(argument)?; }
                }
                pending.extend(context.children.values().map(|child| (child, depth + 1)));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::query::{NominalIdentity, NormalizedSchemaValidationMode, NormalizedShape, SolvedQuery};
    use crate::sema::check::{Checker, Type};
    use crate::source::SourceId;
    use crate::symbol::Name;
    use crate::syntax::parser::Parser;

    #[test]
    fn source_schema_validation_retains_composite_phantom_uint_and_wire_contracts() {
        let source = "enum State: Str { Ready = \"ready\", Missing = \"missing\" }\ntype Marker[T] = {amount: UInt, state: State}\ntype TextMarker = Marker[Str]\npure explicit(raw: Any) -> Result[TextMarker] { raw.require(TextMarker) }\npure contextual(raw: Any) -> Result[Marker[Int]] { raw.require()? }\npure composite(raw: Any) -> Result[List[Marker[Bool]?]] { raw.require(List[Marker[Bool]?]) }\npure resource(raw: Any) -> Result[FsRoot] { raw.require(FsRoot) }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(32), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.schema_validations.len(), 4);
        let mut phantom_arguments = std::collections::BTreeSet::new();
        let mut modes = Vec::new();
        for (identity, validation) in &checked.solved.schema_validations {
            assert_eq!(identity.source, SourceId::new(32));
            assert_eq!(checked.solved.graph.export_type(validation.input).unwrap(), Type::Any);
            modes.push(validation.mode);
            let mut pending = vec![&validation.expectation];
            while let Some(context) = pending.pop() {
                for application in &context.applications {
                    assert!(matches!(application.declaration, QualifiedNominalIdentity::Source { source, namespace: None, .. } if source == SourceId::new(32)));
                    for &argument in &application.arguments { phantom_arguments.insert(checked.solved.graph.export_type(argument).unwrap().to_string()); }
                }
                pending.extend(context.children.values());
            }
        }
        assert_eq!(phantom_arguments, ["Bool".to_string(), "Int".to_string(), "Str".to_string()].into());
        assert_eq!(modes.iter().filter(|&&mode| mode == SchemaValidationMode::Contextual).count(), 1);
        assert!(checked.solved.nominals.values().any(|identity| matches!(identity, QualifiedNominalIdentity::Source { source, declaration: super::super::NominalDeclaration::Type(_), .. } if *source == SourceId::new(32))));
        let types = checked.solved.schema_validations.values().map(|validation| checked.solved.graph.export_type(validation.target).unwrap()).collect::<Vec<_>>();
        let (amount, state) = checked.solved.symbol_owner().with_current(|| (Name::intern("amount"), Name::intern("state")));
        assert!(types.contains(&Type::FsRoot));
        for ty in types {
            let record = match ty { Type::Record(fields) => Some(fields), Type::List(item) => match *item { Type::Optional(item) => match *item { Type::Record(fields) => Some(fields), _ => None }, _ => None }, _ => None };
            if let Some(fields) = record { assert_eq!(fields[&amount], Type::UInt); assert!(matches!(fields[&state], Type::Tag(_))); }
        }
        drop(parsed);
        checked.solved.validate().unwrap();
        let counters = checked.solved.graph.counters().clone();
        let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
        let mut public_phantoms = std::collections::BTreeSet::new();
        for (&identity, validation) in &checked.solved.schema_validations {
            let answer = query.schema_validation(identity).unwrap();
            assert_eq!(answer.source.source, identity.source);
            assert_eq!(answer.source.expression, identity.expression);
            assert_eq!(answer.input.ty.as_ref(), &NormalizedShape::Atom("Any".to_string()));
            assert_eq!(answer.target.as_ref(), query.type_view(&Type::Graph(validation.target), None).unwrap().shape());
            assert_eq!(answer.mode, match validation.mode { SchemaValidationMode::Explicit => NormalizedSchemaValidationMode::Explicit, SchemaValidationMode::Contextual => NormalizedSchemaValidationMode::Contextual });
            let mut pending = vec![&answer.expectation];
            while let Some(context) = pending.pop() {
                for application in &context.applications {
                    assert!(matches!(&application.declaration, NominalIdentity::Source { owner, source, namespace: None, .. } if *owner == checked.solved.owner && *source == SourceId::new(32)));
                    for argument in &application.arguments {
                        let NormalizedShape::Atom(name) = argument.ty.as_ref() else { panic!("the applied schema retains concrete phantom arguments") };
                        assert!(argument.scope.is_none());
                        public_phantoms.insert(name.clone());
                    }
                }
                pending.extend(context.children.values());
            }
        }
        assert_eq!(public_phantoms, phantom_arguments);
        assert_eq!(checked.solved.graph.counters(), &counters);
    }

    #[test]
    fn source_schema_validation_preserves_generic_input_without_training_from_callers() {
        let definitions = "type Row = {name: Str}\npure validated(raw) { raw.require(Row) }\npure forwarded(raw) { validated(raw) }\n";
        for calls in ["let first = forwarded(7)\nlet second = forwarded({name: \"valid\"})\n", "let second = forwarded({name: \"valid\"})\nlet first = forwarded(7)\n"] {
            let source = format!("{definitions}{calls}");
            let parsed = Parser::parse_source_arena_only(SourceId::new(33), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            assert_eq!(checked.solved.schema_validations.len(), 1);
            let validation = checked.solved.schema_validations.values().next().unwrap();
            let declaration = &checked.solved.declarations[&validation.caller.unwrap()];
            assert!(matches!(checked.solved.graph.node(checked.solved.graph.resolved(validation.input).unwrap()).unwrap(), TypeNode::Rigid { scope, .. } if *scope == declaration.scheme));
            assert!(checked.solved.graph.export_type(validation.target).is_ok());
            assert_ne!(checked.solved.graph.resolved(validation.input).unwrap(), checked.solved.graph.resolved(validation.target).unwrap());
            drop(parsed);
            checked.solved.validate().unwrap();
        }
        for source in ["pure invalid(raw: Any) { raw.require()?.name }\n", "pure invalid(raw: Any) -> Result[Any] { raw.require()? }\n"] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(33), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.require-target")), "{:?}", checked.diagnostics);
            assert!(checked.solved.schema_validations.is_empty());
            drop(parsed);
            checked.solved.validate().unwrap();
        }
    }

    #[test]
    fn source_schema_validation_qualifies_imported_private_application_owners() {
        use crate::syntax::arena::ArenaProgramBuilder;
        let source = "use model as m\ntype Private = {count: Int}\npure wrapped(raw: Any) -> Result[m.Box[Int]] { raw.require(m.Box[Int]) }\n";
        let module_source = "##! Schema declarations with private owners.\ntype Private = {count: UInt}\n## A parameterized public schema.\nexport type Box[T] = {value: T, owner: Private}\n## Validate against the declaring schema.\nexport pure validated(raw: Any) -> Result[Box[Int]] { raw.require()? }\n";
        let mut builder = ArenaProgramBuilder::with_token_capacity(128);
        let entry = Parser::parse_source_into_arena_builder(SourceId::new(34), source, &mut builder);
        let module = Parser::parse_source_into_arena_builder(SourceId::new(35), module_source, &mut builder);
        assert!(entry.diagnostics.is_empty() && module.diagnostics.is_empty());
        let namespace = builder.symbol_owner().with_current(|| Name::intern("schema-model"));
        for statement in builder.statement_ids(entry.statements) {
            if let Some((import, _, _)) = builder.use_stmt_for_statement(statement) { builder.set_use_resolved(import, std::sync::Arc::from("schema-model")); }
        }
        builder.push_arena_module("schema-model".to_string(), namespace, module.statements);
        let program = builder.finish_with_statements(entry.statements);
        let checked = Checker::check_arena(&program, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.schema_validations.len(), 2);
        let (owner_name, count) = checked.solved.symbol_owner().with_current(|| (Name::intern("owner"), Name::intern("count")));
        for validation in checked.solved.schema_validations.values() {
            let mut pending = vec![&validation.expectation];
            while let Some(context) = pending.pop() {
                for application in &context.applications { assert!(matches!(application.declaration, QualifiedNominalIdentity::Source { source, namespace: Some(owner), .. } if source == SourceId::new(35) && owner == namespace)); }
                pending.extend(context.children.values());
            }
            let Type::Record(fields) = checked.solved.graph.export_type(validation.target).unwrap() else { panic!("schema target must retain its record fields") };
            let Type::Record(owner) = &fields[&owner_name] else { panic!("private owner remains a checked nested record") };
            assert_eq!(owner[&count], Type::UInt);
        }
        drop(program);
        checked.solved.validate().unwrap();
        let counters = checked.solved.graph.counters().clone();
        let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
        for (&identity, validation) in &checked.solved.schema_validations {
            let answer = query.schema_validation(identity).unwrap();
            assert_eq!(answer.source.source, identity.source);
            assert_eq!(answer.input_source.expression, validation.input_source.expression);
            let mut pending = vec![&answer.expectation];
            while let Some(context) = pending.pop() {
                for application in &context.applications {
                    assert!(matches!(&application.declaration, NominalIdentity::Source { owner, source, namespace, .. } if *owner == checked.solved.owner && *source == SourceId::new(35) && namespace.as_deref() == Some("schema-model")));
                }
                pending.extend(context.children.values());
            }
        }
        assert_eq!(checked.solved.graph.counters(), &counters);
    }

    #[test]
    fn frozen_schema_validation_rejects_changed_input_target_and_foreign_phantom_arguments() {
        let source = "type Marker[T] = {name: Str}\npure validated(raw: Any) -> Result[Marker[Int]] { raw.require()? }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(36), source);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        drop(parsed);
        let mut solved = std::sync::Arc::try_unwrap(checked.solved).unwrap();
        let identity = *solved.schema_validations.keys().next().unwrap();
        let original = solved.schema_validations[&identity].clone();
        solved.schema_validations.get_mut(&identity).unwrap().input = original.target;
        assert!(matches!(solved.validate(), Err(InferenceError::InvalidScheme)));
        solved.schema_validations.insert(identity, original.clone());
        solved.schema_validations.get_mut(&identity).unwrap().target = original.input;
        assert!(matches!(solved.validate(), Err(InferenceError::InvalidScheme)));
        solved.schema_validations.insert(identity, original.clone());
        let mut foreign = InferenceContext::default();
        let argument = foreign.atom(Atom::Int).unwrap();
        solved.schema_validations.get_mut(&identity).unwrap().expectation.applications[0].arguments[0] = argument;
        assert!(matches!(solved.validate(), Err(InferenceError::ForeignHandle)));
        solved.schema_validations.insert(identity, original);
        solved.validate().unwrap();
    }

    #[test]
    fn frozen_schema_validation_rejects_same_bundle_phantom_application_substitution() {
        let source = "type Marker[T] = {name: Str}\npure count(raw: Any) -> Result[Marker[Int]] { raw.require()? }\npure text(raw: Any) -> Result[Marker[Str]] { raw.require()? }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(52), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        drop(parsed);
        let mut solved = std::sync::Arc::try_unwrap(checked.solved).unwrap();
        solved.validate().unwrap();
        let applications = solved.schema_validations.iter().map(|(&identity, validation)|
            (identity, validation.target, validation.expectation.applications[0].arguments[0])).collect::<Vec<_>>();
        assert_eq!(applications.len(), 2);
        assert_eq!(solved.graph.export_type(applications[0].1).unwrap(), solved.graph.export_type(applications[1].1).unwrap());
        assert_ne!(solved.graph.export_type(applications[0].2).unwrap(), solved.graph.export_type(applications[1].2).unwrap());
        let original = solved.schema_validations[&applications[0].0].clone();
        solved.schema_validations.get_mut(&applications[0].0).unwrap().expectation.applications[0].arguments[0] = applications[1].2;
        assert!(solved.validate().is_err(), "equal record layouts cannot replace the checked phantom schema application");
        solved.schema_validations.insert(applications[0].0, original.clone());
        solved.schema_validations.get_mut(&applications[0].0).unwrap().expectation.applications.clear();
        assert!(solved.validate().is_err(), "the original schema application cannot disappear from its retained expectation");
        solved.schema_validations.insert(applications[0].0, original.clone());
        solved.schema_validations.remove(&applications[0].0);
        assert!(solved.validate().is_err(), "the immutable application ledger retains its original source expression");
        solved.schema_validations.insert(applications[0].0, original);
        solved.validate().unwrap();
    }

    #[test]
    fn source_schema_validation_keeps_erased_empty_input_fields_independent() {
        let erased = "let wrong: Record = {item: {value: \"wrong\"}, items: [], maybe: null}\n";
        let baseline = Parser::parse_source_arena_only(SourceId::new(51), erased);
        let checked = Checker::check_arena(&baseline.arena, erased);
        assert!(checked.diagnostics.is_empty(), "erased input before validation: {:?}", checked.diagnostics);
        drop(baseline);
        checked.solved.validate().unwrap();
        let source = "type Box[T] = {value: T}\ntype Envelope[T] = {item: Box[T], items: List[Box[T]], maybe: Box[T]?}\ntype CountEnvelope = Envelope[Int]\nlet raw: Record = {item: {value: 5}, items: [{value: 7}], maybe: null}\nlet checked = raw.require(CountEnvelope)?\nlet wrong: Record = {item: {value: \"wrong\"}, items: [], maybe: null}\nmatch wrong.require(CountEnvelope) { Err(_) => print \"rejected\"; Ok(_) => print \"unexpected\" }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(51), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.schema_validations.len(), 2);
        for validation in checked.solved.schema_validations.values() {
            assert_eq!(checked.solved.graph.export_type(validation.input).unwrap(), Type::ErasedRecord);
        }
        drop(parsed);
        checked.solved.validate().unwrap();
    }

    #[test]
    fn retained_schema_phantom_argument_capacity_is_accounted() {
        let source = "type Marker[T] = {name: Str}\npure validated(raw: Any) -> Result[Marker[Int]] { raw.require()? }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(37), source);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        drop(parsed);
        let mut solved = std::sync::Arc::try_unwrap(checked.solved).unwrap();
        let before = solved.retained_source_bytes();
        let arguments = &mut solved.schema_validations.values_mut().next().unwrap().expectation.applications[0].arguments;
        let old_capacity = arguments.capacity();
        arguments.reserve_exact(17);
        let added_capacity = arguments.capacity() - old_capacity;
        assert_eq!(solved.retained_source_bytes() - before, added_capacity * std::mem::size_of::<TypeId>());
        solved.validate().unwrap();
    }
}
