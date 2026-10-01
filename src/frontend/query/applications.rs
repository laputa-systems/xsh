//! Source application views retain arguments independently of record layouts.

use super::*;
use crate::sema::check::{ConstructorAuthority, ConstructorDefaultIdentity, ConstructorValueSource, QualifiedNominalIdentity, SchemaValidationMode, SolvedSchemaApplication, SolvedSchemaExpectation};
use crate::sema::constants::{LiteralConstant, SchemaComponent};
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedAppliedType { pub ty: Arc<NormalizedShape>, pub scope: Option<Arc<NormalizedScheme>> }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedSchemaApplication { pub declaration: NominalIdentity, pub arguments: Vec<NormalizedAppliedType> }

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum NormalizedSchemaComponent { Field(String), Item, Value, Key, Optional, Success, Error }

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NormalizedSchemaExpectation {
    pub applications: Vec<NormalizedSchemaApplication>,
    pub children: std::collections::BTreeMap<NormalizedSchemaComponent, Self>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalizedSchemaValidationMode { Explicit, Contextual }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedSchemaValidation {
    pub owner: crate::sema::inference::GraphOwner,
    pub source: NormalizedExpressionIdentity,
    pub input_source: NormalizedExpressionIdentity,
    pub input: NormalizedAppliedType,
    pub target: Arc<NormalizedShape>,
    pub result: NormalizedAppliedType,
    pub caller: Option<NormalizedDeclarationIdentity>,
    pub mode: NormalizedSchemaValidationMode,
    pub expectation: NormalizedSchemaExpectation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NormalizedConstructorAuthority {
    Record { application: NormalizedSchemaApplication, origin: NominalIdentity },
    Nominal(NominalIdentity),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NormalizedConstructorValueSource {
    Expression(NormalizedExpressionIdentity),
    RecordField { record: NormalizedExpressionIdentity, field: String },
}

/// These indexes locate certificates in this exact graph. They are provenance
/// metadata and are excluded from comparisons of applied contracts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedConstructorProof { pub owner: crate::sema::inference::GraphOwner, pub assignability: usize, pub projection: Option<usize> }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedConstructorArgument { pub source: NormalizedConstructorValueSource, pub actual: NormalizedAppliedType, pub slot: usize, pub proof: NormalizedConstructorProof }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedConstructorDefaultIdentity { pub owner: NominalIdentity, pub field: String }

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NormalizedLiteral {
    Null, Bool(bool), Int(i64), Float(u64), Duration(u64), Str(Arc<str>), Path(Arc<str>), Bytes(Arc<[u8]>),
    Regex { pattern: Arc<str>, source_text: Arc<str> },
    Tag { family: String, variant: String, fields: Vec<Self> },
    EmptyMap, Map(Vec<(crate::map_key::MapKey, Self)>), List(Vec<Self>), Record(std::collections::BTreeMap<String, Self>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedConstructorDefault { pub identity: NormalizedConstructorDefaultIdentity, pub source: NormalizedExpressionIdentity, pub slot: usize, pub value: NormalizedLiteral }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedConstructorParameter { pub label: Option<String>, pub ty: NormalizedAppliedType, pub default: Option<Arc<NormalizedConstructorDefault>> }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedConstructorApplication {
    pub owner: crate::sema::inference::GraphOwner,
    pub source: NormalizedExpressionIdentity,
    pub caller: Option<NormalizedDeclarationIdentity>,
    pub authority: NormalizedConstructorAuthority,
    pub result: NormalizedAppliedType,
    pub parameters: Vec<NormalizedConstructorParameter>,
    pub supplied: Vec<NormalizedConstructorArgument>,
    pub default_slots: Vec<usize>,
    pub expectation: NormalizedSchemaExpectation,
    pub requirement: Option<NormalizedRequirement>,
}

fn parity_applied(ty: &NormalizedAppliedType, owner: &mut Option<crate::sema::inference::GraphOwner>) -> Result<(), ParityError> {
    if let Some(scope) = &ty.scope { parity_scheme(scope, owner)?; }
    parity_shape(&ty.ty, ty.scope.as_ref().map(|scope| scope.quantifiers.as_slice()).unwrap_or(&[]), ty.scope.as_ref().map(|scope| scope.effect_quantifiers.len()).unwrap_or(0), owner)
}

fn parity_application(application: &NormalizedSchemaApplication, owner: crate::sema::inference::GraphOwner) -> Result<(), ParityError> {
    if application.declaration.owner() != owner { return Err(ParityError::ForeignNominalOwner); }
    let mut graph = Some(owner);
    for argument in &application.arguments { parity_applied(argument, &mut graph)?; }
    Ok(())
}

fn parity_expectation(expectation: &NormalizedSchemaExpectation, owner: crate::sema::inference::GraphOwner) -> Result<(), ParityError> {
    for application in &expectation.applications { parity_application(application, owner)?; }
    for child in expectation.children.values() { parity_expectation(child, owner)?; }
    Ok(())
}

impl NormalizedSchemaValidation {
    pub fn semantic_parity(&self, other: &Self) -> Result<bool, ParityError> {
        if self.owner != other.owner { return Err(ParityError::ForeignNominalOwner); }
        for answer in [self, other] {
            let mut owner = Some(answer.owner);
            parity_applied(&answer.input, &mut owner)?;
            parity_applied(&answer.result, &mut owner)?;
            parity_shape(&answer.target, &[], 0, &mut owner)?;
            parity_expectation(&answer.expectation, answer.owner)?;
        }
        Ok(self == other)
    }
    /// Compare the selected application contract, independently of the source
    /// expression that supplied the input or its evaluation.
    pub fn application_parity(&self, other: &Self) -> Result<bool, ParityError> {
        if self.owner != other.owner { return Err(ParityError::ForeignNominalOwner); }
        for answer in [self, other] { parity_expectation(&answer.expectation, answer.owner)?; }
        Ok(self.target == other.target && self.mode == other.mode && self.expectation == other.expectation)
    }
}

impl NormalizedConstructorApplication {
    pub fn semantic_parity(&self, other: &Self) -> Result<bool, ParityError> {
        if !self.application_parity(other)? { return Ok(false); }
        for answer in [self, other] {
            let mut owner = Some(answer.owner);
            parity_applied(&answer.result, &mut owner)?;
            for parameter in &answer.parameters {
                parity_applied(&parameter.ty, &mut owner)?;
                if parameter.default.as_ref().is_some_and(|default| default.identity.owner.owner() != answer.owner) { return Err(ParityError::ForeignNominalOwner); }
            }
            for argument in &answer.supplied {
                if argument.proof.owner != answer.owner { return Err(ParityError::ForeignOperationOwner); }
                parity_applied(&argument.actual, &mut owner)?;
            }
            if let Some(requirement) = &answer.requirement {
                let scope = answer.result.scope.as_ref();
                parity_requirement(requirement, scope.map(|scope| scope.quantifiers.as_slice()).unwrap_or(&[]), scope.map(|scope| scope.effect_quantifiers.len()).unwrap_or(0), &mut owner)?;
            }
        }
        let supplied_equal = self.supplied.iter().map(|argument| (&argument.source, &argument.actual, argument.slot)).eq(other.supplied.iter().map(|argument| (&argument.source, &argument.actual, argument.slot)));
        Ok(self.source == other.source && self.caller == other.caller && self.result == other.result && self.parameters == other.parameters && self.default_slots == other.default_slots && self.requirement == other.requirement && supplied_equal)
    }
    /// Certificate indexes and supplied values do not establish equivalence
    /// between constructor authorities or their applied type arguments.
    pub fn application_parity(&self, other: &Self) -> Result<bool, ParityError> {
        if self.owner != other.owner { return Err(ParityError::ForeignNominalOwner); }
        for answer in [self, other] {
            match &answer.authority {
                NormalizedConstructorAuthority::Record { application, origin } => { parity_application(application, answer.owner)?; if origin.owner() != answer.owner { return Err(ParityError::ForeignNominalOwner); } }
                NormalizedConstructorAuthority::Nominal(identity) if identity.owner() != answer.owner => return Err(ParityError::ForeignNominalOwner),
                NormalizedConstructorAuthority::Nominal(_) => {}
            }
            parity_expectation(&answer.expectation, answer.owner)?;
        }
        Ok(self.authority == other.authority && self.expectation == other.expectation)
    }
}

/// Every scope has its own alpha map while the complete projection shares one
/// resource budget and one normalized copy of each scope and default.
struct ApplicationView<'a, 'q> {
    query: &'a SolvedQuery<'q>, views: rustc_hash::FxHashMap<Option<SchemeId>, View<'a, 'q>>,
    scopes: rustc_hash::FxHashMap<SchemeId, Arc<NormalizedScheme>>,
    types: rustc_hash::FxHashMap<(TypeId, Option<SchemeId>), NormalizedAppliedType>,
    defaults: std::collections::BTreeMap<ConstructorDefaultIdentity, Arc<NormalizedConstructorDefault>>,
    nodes: usize, bytes: usize,
}

impl<'a, 'q> ApplicationView<'a, 'q> {
    fn new(query: &'a SolvedQuery<'q>) -> Self { Self { query, views: Default::default(), scopes: Default::default(), types: Default::default(), defaults: Default::default(), nodes: 0, bytes: 0 } }
    fn run<T>(&mut self, scope: Option<SchemeId>, action: impl FnOnce(&mut View<'a, 'q>) -> Result<T, QueryError>) -> Result<T, QueryError> {
        let view = self.views.entry(scope).or_insert_with(|| View::new(self.query, scope));
        view.nodes = self.nodes; view.text_bytes.set(self.bytes);
        let result = action(view); self.nodes = view.nodes; self.bytes = view.text_bytes.get(); result
    }
    fn entries(&mut self, count: usize, depth: usize) -> Result<(), QueryError> {
        if count > self.query.limits.nodes.saturating_sub(self.nodes) { return Err(QueryError::Limit); }
        self.run(None, |view| { for _ in 0..count { view.visit(depth)?; } Ok(()) })
    }
    fn ty(&mut self, ty: TypeId, scope: Option<SchemeId>) -> Result<NormalizedAppliedType, QueryError> {
        if let Some(ty) = self.types.get(&(ty, scope)) { return Ok(ty.clone()); }
        let contract = match scope {
            Some(scope) => {
                if !self.scopes.contains_key(&scope) {
                    let root = self.query.solved.graph.scheme(scope)?.body;
                    let scheme = self.run(Some(scope), |view| view.scheme(root, Some(scope)))?;
                    self.scopes.insert(scope, Arc::new(scheme));
                }
                self.scopes.get(&scope).cloned()
            }
            None => None,
        };
        let answer = NormalizedAppliedType { ty: Arc::new(self.run(scope, |view| view.graph(ty, 0))?), scope: contract };
        self.types.insert((ty, scope), answer.clone()); Ok(answer)
    }
    fn nominal(&mut self, identity: QualifiedNominalIdentity) -> Result<NominalIdentity, QueryError> { self.run(None, |view| { view.visit(0)?; view.nominal_identity(identity) }) }
    fn expression(&mut self, identity: ExpressionIdentity) -> Result<NormalizedExpressionIdentity, QueryError> { self.run(None, |view| { view.visit(0)?; view.expression_identity(identity) }) }
    fn application(&mut self, application: &SolvedSchemaApplication, scope: Option<SchemeId>) -> Result<NormalizedSchemaApplication, QueryError> {
        self.entries(application.arguments.len(), 0)?;
        Ok(NormalizedSchemaApplication { declaration: self.nominal(application.declaration)?, arguments: application.arguments.iter().map(|&ty| self.ty(ty, scope)).collect::<Result<_, _>>()? })
    }
    fn expectation(&mut self, expectation: &SolvedSchemaExpectation, scope: Option<SchemeId>, depth: usize) -> Result<NormalizedSchemaExpectation, QueryError> {
        self.entries(1 + expectation.applications.len() + expectation.children.len(), depth)?;
        let applications = expectation.applications.iter().map(|application| self.application(application, scope)).collect::<Result<_, _>>()?;
        let mut children = std::collections::BTreeMap::new();
        for (&component, child) in &expectation.children {
            let component = match component { SchemaComponent::Field(name) => NormalizedSchemaComponent::Field(self.run(None, |view| view.name(name))?), SchemaComponent::Item => NormalizedSchemaComponent::Item, SchemaComponent::Value => NormalizedSchemaComponent::Value, SchemaComponent::Key => NormalizedSchemaComponent::Key, SchemaComponent::Optional => NormalizedSchemaComponent::Optional, SchemaComponent::Success => NormalizedSchemaComponent::Success, SchemaComponent::Error => NormalizedSchemaComponent::Error };
            children.insert(component, self.expectation(child, scope, depth + 1)?);
        }
        Ok(NormalizedSchemaExpectation { applications, children })
    }
    fn certify_expectation(&mut self, identity: ExpressionIdentity, expectation: &SolvedSchemaExpectation, scope: Option<SchemeId>) -> Result<(), QueryError> {
        let mut roots = Vec::new();
        self.certify_context(identity, expectation, scope, &mut Vec::new(), &mut roots)?;
        self.query.solved.graph.validate_application_source_roots(identity.source, identity.namespace, identity.expression, &roots)?;
        Ok(())
    }
    fn certify_context(&mut self, identity: ExpressionIdentity, expectation: &SolvedSchemaExpectation, scope: Option<SchemeId>, path: &mut Vec<crate::sema::inference::ApplicationPathComponent>, roots: &mut Vec<crate::sema::inference::ScopedApplicationRoot>) -> Result<(), QueryError> {
        use crate::sema::inference::{ApplicationCertificate, ApplicationDeclaration, ApplicationPathComponent, ApplicationSource, ScopedApplicationRoot};
        self.entries(1 + expectation.applications.len() + expectation.children.len(), path.len())?;
        for (ordinal, application) in expectation.applications.iter().enumerate() {
            self.entries(path.len() + application.arguments.len(), path.len())?;
            let QualifiedNominalIdentity::Source { source, namespace, declaration: crate::sema::check::NominalDeclaration::Type(declaration), member: None } = application.declaration else { return Err(QueryError::Recovery); };
            let root = ScopedApplicationRoot {
                certificate: ApplicationCertificate {
                    source: ApplicationSource { source: identity.source, namespace: identity.namespace, expression: identity.expression, path: path.clone(), ordinal: ordinal.try_into().map_err(|_| QueryError::Limit)? },
                    declaration: ApplicationDeclaration { source, namespace, declaration }, arguments: application.arguments.clone(),
                }, scope,
            };
            self.query.solved.graph.validate_application_root(&root)?;
            roots.push(root);
        }
        for (&component, child) in &expectation.children {
            let component = match component { SchemaComponent::Field(name) => ApplicationPathComponent::Field(name), SchemaComponent::Item => ApplicationPathComponent::Item, SchemaComponent::Value => ApplicationPathComponent::Value, SchemaComponent::Key => ApplicationPathComponent::Key, SchemaComponent::Optional => ApplicationPathComponent::Optional, SchemaComponent::Success => ApplicationPathComponent::Success, SchemaComponent::Error => ApplicationPathComponent::Error };
            path.push(component);
            self.certify_context(identity, child, scope, path, roots)?;
            path.pop();
        }
        Ok(())
    }
    fn literal(&mut self, value: &LiteralConstant, depth: usize) -> Result<NormalizedLiteral, QueryError> {
        self.entries(1, depth)?;
        Ok(match value {
            LiteralConstant::Null => NormalizedLiteral::Null, LiteralConstant::Bool(value) => NormalizedLiteral::Bool(*value), LiteralConstant::Int(value) => NormalizedLiteral::Int(*value), LiteralConstant::Float(value) => NormalizedLiteral::Float(*value), LiteralConstant::Duration(value) => NormalizedLiteral::Duration(*value),
            LiteralConstant::Str(value) => { self.run(None, |view| view.text(value.len()))?; NormalizedLiteral::Str(value.clone()) },
            LiteralConstant::Path(value) => { self.run(None, |view| view.text(value.len()))?; NormalizedLiteral::Path(value.clone()) },
            LiteralConstant::Bytes(value) => { self.run(None, |view| view.text(value.len()))?; NormalizedLiteral::Bytes(value.clone()) },
            LiteralConstant::Regex(value) => { self.run(None, |view| view.text(value.pattern.len() + value.source_text.len()))?; NormalizedLiteral::Regex { pattern: value.pattern.clone(), source_text: value.source_text.clone() } },
            LiteralConstant::Tag { family, variant, fields } => { self.entries(fields.len(), depth)?; NormalizedLiteral::Tag { family: self.run(None, |view| view.name(*family))?, variant: self.run(None, |view| view.name(*variant))?, fields: fields.iter().map(|value| self.literal(value, depth + 1)).collect::<Result<_, _>>()? } },
            LiteralConstant::EmptyMap => NormalizedLiteral::EmptyMap,
            LiteralConstant::Map(values) => { self.entries(values.len(), depth)?; let mut entries = Vec::with_capacity(values.len()); for (key, value) in values.iter() { self.run(None, |view| view.text(match key { crate::map_key::MapKey::Str(value) => value.len(), crate::map_key::MapKey::Bytes(value) | crate::map_key::MapKey::Path(value) => value.len(), _ => 0 }))?; entries.push((key.clone(), self.literal(value, depth + 1)?)); } NormalizedLiteral::Map(entries) },
            LiteralConstant::List(values) => { self.entries(values.len(), depth)?; NormalizedLiteral::List(values.iter().map(|value| self.literal(value, depth + 1)).collect::<Result<_, _>>()?) },
            LiteralConstant::Record(values) => { self.entries(values.len(), depth)?; let mut fields = std::collections::BTreeMap::new(); for (&name, value) in values.iter() { fields.insert(self.run(None, |view| view.name(name))?, self.literal(value, depth + 1)?); } NormalizedLiteral::Record(fields) },
        })
    }
    fn default(&mut self, identity: ConstructorDefaultIdentity) -> Result<Arc<NormalizedConstructorDefault>, QueryError> {
        if let Some(default) = self.defaults.get(&identity) { return Ok(default.clone()); }
        let fact = self.query.solved.constructor_defaults.get(&identity).ok_or(QueryError::Recovery)?;
        let default = Arc::new(NormalizedConstructorDefault { identity: NormalizedConstructorDefaultIdentity { owner: self.nominal(identity.owner)?, field: self.run(None, |view| view.name(identity.field))? }, source: self.expression(fact.source)?, slot: fact.slot, value: self.literal(&fact.value, 0)? });
        self.defaults.insert(identity, default.clone()); Ok(default)
    }
}

impl SolvedQuery<'_> {
    pub fn schema_validation(&self, identity: ExpressionIdentity) -> Result<NormalizedSchemaValidation, QueryError> {
        self.validate_owner()?;
        if !self.solved.expressions.contains_key(&identity) { return Err(QueryError::MissingExpression); }
        let fact = self.solved.schema_validations.get(&identity).ok_or(QueryError::MissingSchemaValidation)?;
        let mut view = ApplicationView::new(self);
        let scope = self.solved.expression_scope(identity, fact.caller)?;
        let input_scope = self.solved.expression_scope(fact.input_source, fact.caller)?;
        view.certify_expectation(identity, &fact.expectation, scope)?;
        Ok(NormalizedSchemaValidation { owner: self.solved.owner, source: view.expression(identity)?, input_source: view.expression(fact.input_source)?, input: view.ty(fact.input, input_scope)?, target: view.ty(fact.target, None)?.ty, result: view.ty(fact.result, scope)?, caller: fact.caller.map(|caller| view.run(None, |view| view.declaration_identity(caller))).transpose()?, mode: match fact.mode { SchemaValidationMode::Explicit => NormalizedSchemaValidationMode::Explicit, SchemaValidationMode::Contextual => NormalizedSchemaValidationMode::Contextual }, expectation: view.expectation(&fact.expectation, None, 0)? })
    }
    pub fn constructor_application(&self, identity: ExpressionIdentity) -> Result<NormalizedConstructorApplication, QueryError> {
        self.validate_owner()?;
        if !self.solved.expressions.contains_key(&identity) { return Err(QueryError::MissingExpression); }
        let fact = self.solved.constructor_applications.get(&identity).ok_or(QueryError::MissingConstructorApplication)?;
        let scope = self.solved.expression_scope(identity, fact.caller)?;
        let mut view = ApplicationView::new(self);
        if let ConstructorAuthority::Record { application, origin } = &fact.authority {
            view.certify_expectation(identity, &fact.expectation, scope)?;
            if fact.expectation.applications.first() != Some(application) || !fact.expectation.applications.iter().any(|application| application.declaration == *origin) { return Err(QueryError::Recovery); }
        }
        view.entries(fact.parameters.len() + fact.supplied.len() + fact.default_slots.len(), 0)?;
        let authority = match &fact.authority { ConstructorAuthority::Record { application, origin } => NormalizedConstructorAuthority::Record { application: view.application(application, scope)?, origin: view.nominal(*origin)? }, ConstructorAuthority::Nominal(identity) => NormalizedConstructorAuthority::Nominal(view.nominal(*identity)?) };
        let mut parameters = Vec::with_capacity(fact.parameters.len());
        for parameter in &fact.parameters { parameters.push(NormalizedConstructorParameter { label: parameter.label.map(|name| view.run(None, |view| view.name(name))).transpose()?, ty: view.ty(parameter.ty, scope)?, default: parameter.default.map(|identity| view.default(identity)).transpose()? }); }
        let mut supplied = Vec::with_capacity(fact.supplied.len());
        for argument in &fact.supplied {
            let (source, input) = match argument.value { ConstructorValueSource::Expression(expression) => (NormalizedConstructorValueSource::Expression(view.expression(expression)?), expression), ConstructorValueSource::RecordField { record, field } => (NormalizedConstructorValueSource::RecordField { record: view.expression(record)?, field: view.run(None, |view| view.name(field))? }, record) };
            supplied.push(NormalizedConstructorArgument { source, actual: view.ty(argument.actual, self.solved.expression_scope(input, fact.caller)?)?, slot: argument.slot, proof: NormalizedConstructorProof { owner: self.solved.owner, assignability: argument.assignability, projection: argument.projection } });
        }
        Ok(NormalizedConstructorApplication { owner: self.solved.owner, source: view.expression(identity)?, caller: fact.caller.map(|caller| view.run(None, |view| view.declaration_identity(caller))).transpose()?, authority, result: view.ty(fact.result, scope)?, parameters, supplied, default_slots: fact.default_slots.clone(), expectation: view.expectation(&fact.expectation, scope, 0)?, requirement: fact.requirement.map(|requirement| view.run(scope, |view| view.requirement(self.solved.graph.requirement_template(requirement)?))).transpose()? })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn checked(source: &str) -> crate::sema::check::CheckOutput {
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(62), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = crate::sema::check::Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        drop(parsed);
        checked.solved.validate().unwrap();
        checked
    }
    #[test]
    fn solved_query_schema_application_keeps_phantom_arguments_after_ast_drop() {
        let checked = checked("type Marker[T] = {name: Str}\npure count(raw: Any) -> Result[Marker[Int]] { raw.require()? }\npure text(raw: Any) -> Result[Marker[Str]] { raw.require()? }\n");
        let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
        let answers = checked.solved.schema_validations.keys().map(|&identity| query.schema_validation(identity).unwrap()).collect::<Vec<_>>();
        assert_eq!(answers.len(), 2);
        assert_eq!(answers[0].target, answers[1].target);
        assert_ne!(answers[0].expectation.applications, answers[1].expectation.applications);
        assert_eq!(answers[0].application_parity(&answers[1]), Ok(false));
        assert_eq!(answers[0].semantic_parity(&answers[1]), Ok(false));
        assert!(answers.iter().all(|answer| answer.mode == NormalizedSchemaValidationMode::Contextual));
    }
    #[test]
    fn solved_query_constructor_application_keeps_phantom_arguments_after_ast_drop() {
        let checked = checked("type Marker[T] = {name: Str}\npure count() -> Marker[Int] { Marker(name: \"count\") }\npure text() -> Marker[Str] { Marker(name: \"text\") }\n");
        let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
        let answers = checked.solved.constructor_applications.keys().map(|&identity| query.constructor_application(identity).unwrap()).collect::<Vec<_>>();
        assert_eq!(answers.len(), 2);
        assert_eq!(answers[0].result, answers[1].result);
        assert_ne!(answers[0].authority, answers[1].authority);
        assert_eq!(answers[0].application_parity(&answers[1]), Ok(false));
        assert_eq!(answers[0].semantic_parity(&answers[1]), Ok(false));
    }

    #[test]
    fn solved_query_constructor_application_shares_scopes_and_enforces_one_projection_budget() {
        let checked = checked("type Pair[T] = {left: T, right: T, items: List[T] = []}\npure make(value) { Pair(left: value, right: value) }\nlet count = make(7)\nlet text = make(\"word\")\n");
        let identity = *checked.solved.constructor_applications.keys().next().unwrap();
        let counters = checked.solved.graph.counters().clone();
        let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
        let answer = query.constructor_application(identity).unwrap();
        let left = answer.parameters.iter().find(|parameter| parameter.label.as_deref() == Some("left")).unwrap();
        let right = answer.parameters.iter().find(|parameter| parameter.label.as_deref() == Some("right")).unwrap();
        assert!(Arc::ptr_eq(left.ty.scope.as_ref().unwrap(), right.ty.scope.as_ref().unwrap()));
        assert!(Arc::ptr_eq(&left.ty.ty, &right.ty.ty));
        let items = answer.parameters.iter().position(|parameter| parameter.label.as_deref() == Some("items")).unwrap();
        assert_eq!(answer.default_slots, vec![items]);
        assert_eq!(answer.parameters[items].default.as_ref().unwrap().value, NormalizedLiteral::List(Vec::new()));
        assert_eq!(answer.semantic_parity(&answer), Ok(true));
        let mut proof_reindexed = answer.clone();
        proof_reindexed.supplied[0].proof.assignability += 1;
        assert_eq!(answer.semantic_parity(&proof_reindexed), Ok(true), "local certificate indexes do not define application semantics");
        let source = match &answer.supplied[0].source { NormalizedConstructorValueSource::Expression(source) => source.clone(), _ => unreachable!() };
        let original = ExpressionIdentity { source: source.source, namespace: None, expression: source.expression };
        assert_eq!(query.constructor_application(original), Err(QueryError::MissingConstructorApplication));
        let limited = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner()).with_limits(QueryLimits { nodes: 1, ..QueryLimits::default() });
        assert_eq!(limited.constructor_application(identity), Err(QueryError::Limit));
        assert_eq!(checked.solved.graph.counters().attempted_constraints, counters.attempted_constraints);
        assert_eq!(checked.solved.graph.counters().unifications, counters.unifications);
        assert_eq!(checked.solved.graph.counters().instantiations, counters.instantiations);
    }

    #[test]
    fn solved_query_applications_refuse_changed_or_deleted_phantom_source_certificates() {
        for constructor in [false, true] {
            let source = if constructor { "type Marker[T] = {name: Str}\npure count() -> Marker[Int] { Marker(name: \"count\") }\npure text() -> Marker[Str] { Marker(name: \"text\") }\n" } else { "type Marker[T] = {name: Str}\npure count(raw: Any) -> Result[Marker[Int]] { raw.require()? }\npure text(raw: Any) -> Result[Marker[Str]] { raw.require()? }\n" };
            let checked = checked(source);
            let mut solved = Arc::try_unwrap(checked.solved).unwrap();
            if constructor {
                let keys = solved.constructor_applications.keys().copied().collect::<Vec<_>>();
                let replacement = solved.constructor_applications[&keys[1]].expectation.applications[0].arguments[0];
                let original = solved.constructor_applications[&keys[0]].clone();
                let changed = solved.constructor_applications.get_mut(&keys[0]).unwrap();
                let ConstructorAuthority::Record { application, .. } = &mut changed.authority else { unreachable!() };
                application.arguments[0] = replacement;
                changed.expectation.applications[0].arguments[0] = replacement;
                let query = SolvedQuery::new(&solved, solved.symbol_owner());
                assert!(query.constructor_application(keys[0]).is_err(), "structural equality cannot replace a constructor's original phantom application");
                assert!(query.constructor_application(keys[1]).is_ok(), "validation is limited to the queried source, without a whole-bundle scan");
                solved.constructor_applications.insert(keys[0], original);
                solved.constructor_applications.get_mut(&keys[0]).unwrap().expectation.applications.clear();
                assert!(SolvedQuery::new(&solved, solved.symbol_owner()).constructor_application(keys[0]).is_err(), "empty metadata cannot delete the recorded application");
            } else {
                let keys = solved.schema_validations.keys().copied().collect::<Vec<_>>();
                let replacement = solved.schema_validations[&keys[1]].expectation.applications[0].arguments[0];
                let original = solved.schema_validations[&keys[0]].clone();
                solved.schema_validations.get_mut(&keys[0]).unwrap().expectation.applications[0].arguments[0] = replacement;
                let query = SolvedQuery::new(&solved, solved.symbol_owner());
                assert!(query.schema_validation(keys[0]).is_err(), "structural equality cannot replace the independently chosen schema application");
                assert!(query.schema_validation(keys[1]).is_ok(), "an unrelated source keeps its original certificate");
                solved.schema_validations.insert(keys[0], original);
                solved.schema_validations.get_mut(&keys[0]).unwrap().expectation.applications.clear();
                assert!(SolvedQuery::new(&solved, solved.symbol_owner()).schema_validation(keys[0]).is_err(), "full per-source deletion must fail too");
            }
        }
    }

    #[test]
    fn solved_query_error_join_preserves_independent_inputs_and_completion_bound() {
        use crate::sema::check::SolvedCallable;
        use crate::sema::inference::{Arrow, ErrorJoin, Generalization, InferenceContext, Parameter};
        use crate::syntax::arena::{BlockId, FunctionDefId};
        let symbols = SymbolOwner::new();
        let solved = symbols.with_current(|| {
            let mut facts = SolvedTypes::<InferenceContext>::default();
            let graph = &mut facts.graph;
            let span = crate::source::Span::new(crate::source::SourceId::new(63), 0, 0);
            let first = graph.fresh(1, span).unwrap();
            let second = graph.fresh(1, span).unwrap();
            let joined = graph.fresh(1, span).unwrap();
            let bound = graph.atom(Atom::Error).unwrap();
            let reason = graph.reason(span, None).unwrap();
            let requirement = graph.require_error_join(ErrorJoin { inputs: vec![first, second], result: joined, bound: Some(bound) }, reason).unwrap();
            let empty = EffectSummary::Closed(EffectSet::EMPTY);
            let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: symbols.intern("first"), ty: first, defaulted: false, rest: false }, Parameter { label: symbols.intern("second"), ty: second, defaulted: false, rest: false }], result: joined, effects: empty }).unwrap();
            let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[requirement]).unwrap();
            let declaration = DeclarationIdentity { source: span.source_id, namespace: None, declaration: FunctionDefId::from_index(0) };
            facts.declarations.insert(declaration, SolvedCallable { source_requirements: vec![requirement], parameter_producer_flows: Vec::new(), return_producer_flow: None, parameter_producers: vec![Default::default(); 2], return_producers: Default::default(), scheme, signature, body: BlockId::from_index(0), kind: CallableKind::Pure, return_elaboration: ReturnElaboration::Value, effective_effects: empty, required_effects: empty });
            facts.freeze_fixture().unwrap()
        });
        let counters = solved.graph.counters().clone();
        let identity = *solved.declarations.keys().next().unwrap();
        let answer = SolvedQuery::new(&solved, &symbols).declaration(identity).unwrap();
        let [NormalizedRequirement::ErrorJoin { inputs, result, bound }] = answer.scheme.requirements.as_slice() else { panic!("retained independent join"); };
        assert_ne!(inputs[0], inputs[1]);
        assert_eq!(inputs.len(), 2);
        assert_ne!(result, &inputs[0]);
        assert_eq!(bound, &Some(NormalizedShape::Atom("Error".into())));
        assert!(answer.to_string().contains("ErrorJoin(T0, T1) -> T2 within Error"));
        let mut changed = answer.clone();
        let NormalizedRequirement::ErrorJoin { bound, .. } = &mut changed.scheme.requirements[0] else { unreachable!() };
        *bound = None;
        assert_eq!(answer.semantic_parity(&changed), Ok(false));
        assert_eq!(solved.graph.counters().attempted_constraints, counters.attempted_constraints);
        assert_eq!(solved.graph.counters().unifications, counters.unifications);
        assert_eq!(solved.graph.counters().instantiations, counters.instantiations);
    }
}
