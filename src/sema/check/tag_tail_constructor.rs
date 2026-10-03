use super::{Checker, ConstructorAuthority, DeclarationIdentity, NominalDeclaration, NominalMemberKind, QualifiedNominalIdentity, SolvedConstructorApplication, SolvedSchemaExpectation, SolvedTypes, StatementIdentity, StatementPosition, Type};
use crate::sema::inference::{Arrow, CallableKind, EffectSet, EffectSummary, Generalization, InferenceContext, InferenceError, OperationCall, ScopedRequirementRoot, ScopedRoot};
use crate::symbol::Name;
use crate::syntax::arena::{ArenaProgram, StmtId};
use std::sync::Arc;

/// A bare nominal tail is an authored statement, with no expression identity.
#[derive(Clone, Debug)]
pub struct SolvedTagTailConstructor {
    pub statement: StatementIdentity,
    pub name: Name,
    pub application: SolvedConstructorApplication,
}

impl Checker {
    pub(super) fn record_graph_tag_tail_constructor(&mut self, arena: &ArenaProgram, statement: StmtId, name: Name, ty: &Type) {
        let Type::Tag(family) = ty else { return; };
        if !self.graph_generation || self.lookup(name).is_some() { return; }
        let Some(member) = self.tag_variants.get(&name).filter(|member| member.type_name == *family && member.field_count == 0).map(|member| member.canonical_name) else { return; };
        let span = arena.arena.stmt(statement).span;
        let identity = StatementIdentity { source: span.source_id, namespace: self.current_namespace, statement };
        if self.generic.borrow().facts.tag_tail_constructors.contains_key(&identity) { return; }
        let outcome = (|| {
            let result = self.graph_type(ty, span)?;
            let mut state = self.generic.borrow_mut();
            let QualifiedNominalIdentity::Source { source, namespace, declaration: declaration @ NominalDeclaration::Type(_), member: None } = *state.facts.nominals.get(&result).ok_or(InferenceError::InvalidScheme)? else { return Err(InferenceError::InvalidScheme); };
            let authority = QualifiedNominalIdentity::Source { source, namespace, declaration, member: Some(member) };
            let original = state.facts.checked_nominal_member(authority)?;
            if original.kind != NominalMemberKind::Tag || original.family != *family || original.member != member || !original.fields.is_empty() { return Err(InferenceError::InvalidScheme); }
            if state.facts.constructor_nominals.get(&authority).is_some_and(|fields| !fields.is_empty()) { return Err(InferenceError::InvalidScheme); }
            if !state.facts.constructor_nominals.contains_key(&authority) {
                state.facts.graph.charge_source_fact_nodes(1)?;
                state.facts.constructor_nominals.insert(authority, Vec::new());
            }
            let signature = state.facts.graph.arrow(Arrow { kind: CallableKind::Pure, params: Vec::new(), result, effects: EffectSummary::Closed(EffectSet::EMPTY) })?;
            let scheme = state.facts.graph.generalize(signature, 0, Generalization::Monomorphic, &[])?;
            let operation = {
                let super::generic::GenericState { facts, language_operations, .. } = &mut *state;
                language_operations.declaration_family(&mut facts.graph, "language.constructor.tag", Name::intern(&format!("{authority:?}")), scheme)?
            };
            let reason = state.facts.graph.reason(span, None)?;
            let requirement = state.facts.graph.require_operation(operation, OperationCall { binding: crate::sema::inference::OperationBinding::Slots,
                effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, receiver: None, arguments: Vec::new(), result,
                effects: EffectSummary::Closed(EffectSet::EMPTY), effect_bindings: Vec::new(), output_effect_bindings: Vec::new(), declared_error_bound: None }, reason)?;
            state.facts.graph.solve()?;
            if let Some(caller) = self.current_generic { state.pending.get_mut(&caller).ok_or(InferenceError::InvalidScheme)?.requirements.push(requirement); }
            state.facts.graph.charge_source_fact_nodes(2)?;
            state.facts.graph.charge_source_fact_edges(3)?;
            let source = Arc::new(SolvedTagTailConstructor { statement: identity, name, application: SolvedConstructorApplication {
                authority: ConstructorAuthority::Nominal(authority), result, caller: self.current_generic, parameters: Vec::new(), supplied: Vec::new(),
                default_slots: Vec::new(), expectation: SolvedSchemaExpectation::default(), requirement: Some(requirement),
            } });
            state.facts.tag_tail_constructors.insert(identity, Arc::clone(&source));
            state.facts.original_tag_tail_constructors.insert(identity, source);
            state.facts.statements.entry(identity).or_insert(StatementPosition::Statement);
            if let Some(caller) = self.current_generic { state.facts.statement_owners.insert(identity, caller); }
            Ok::<_, InferenceError>(())
        })();
        if let Err(error) = outcome { self.graph_error(span, error); }
    }
}

impl<Graph> SolvedTypes<Graph> {
    pub(crate) fn tag_tail_constructor_originally_checked(&self, statement: StatementIdentity) -> bool {
        self.original_tag_tail_constructors.contains_key(&statement)
    }

    pub(crate) fn checked_tag_tail_constructor(&self, statement: StatementIdentity) -> Result<&SolvedTagTailConstructor, InferenceError> {
        let original = self.original_tag_tail_constructors.get(&statement).ok_or(InferenceError::InvalidScheme)?;
        let current = self.tag_tail_constructors.get(&statement).ok_or(InferenceError::InvalidScheme)?;
        if !Arc::ptr_eq(original, current) { return Err(InferenceError::InvalidScheme); }
        Ok(original)
    }

    pub(crate) fn tag_tail_constructor_scope(&self, caller: Option<DeclarationIdentity>) -> Result<Option<crate::sema::inference::SchemeId>, InferenceError> {
        caller.map(|caller| self.declarations.get(&caller).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()
    }

    pub(super) fn tag_tail_constructor_requirement_roots(&self) -> Result<Vec<ScopedRequirementRoot>, InferenceError> {
        self.tag_tail_constructors.values().map(|source| Ok(ScopedRequirementRoot {
            requirement: source.application.requirement.ok_or(InferenceError::InvalidScheme)?, scope: self.tag_tail_constructor_scope(source.application.caller)?,
        })).collect()
    }

    pub(super) fn tag_tail_constructor_roots(&self, _graph: &InferenceContext) -> Result<Vec<ScopedRoot>, InferenceError> {
        self.tag_tail_constructors.values().map(|source| Ok(ScopedRoot { ty: source.application.result, scope: self.tag_tail_constructor_scope(source.application.caller)? })).collect()
    }

    pub(super) fn tag_tail_constructor_payload_bytes(&self) -> usize {
        self.original_tag_tail_constructors.len() * (std::mem::size_of::<SolvedTagTailConstructor>() + 2 * std::mem::size_of::<usize>())
    }

    pub(super) fn tag_tail_constructor_source_work(&self) -> u64 { self.tag_tail_constructors.len() as u64 * 3 }

    pub(super) fn validate_tag_tail_constructors(&self, graph: &InferenceContext) -> Result<(), InferenceError> {
        if self.tag_tail_constructors.len() != self.original_tag_tail_constructors.len() { return Err(InferenceError::InvalidScheme); }
        for &statement in self.tag_tail_constructors.keys() {
            let source = self.checked_tag_tail_constructor(statement)?;
            let application = &source.application;
            if source.statement != statement || !self.statements.contains_key(&statement) || self.statement_owners.get(&statement).copied() != application.caller
                || !application.parameters.is_empty() || !application.supplied.is_empty() || !application.default_slots.is_empty() { return Err(InferenceError::InvalidScheme); }
            let ConstructorAuthority::Nominal(authority @ QualifiedNominalIdentity::Source { source, namespace, declaration: declaration @ NominalDeclaration::Type(_), member: Some(_) }) = application.authority else { return Err(InferenceError::InvalidScheme); };
            let member = self.checked_nominal_member(authority)?;
            if member.kind != NominalMemberKind::Tag || !member.fields.is_empty() || self.nominals.get(&graph.resolved(application.result)?) != Some(&QualifiedNominalIdentity::Source { source, namespace, declaration, member: None }) { return Err(InferenceError::InvalidScheme); }
            let scope = self.tag_tail_constructor_scope(application.caller)?;
            graph.validate_scoped(ScopedRoot { ty: application.result, scope })?;
            let requirement = application.requirement.ok_or(InferenceError::InvalidScheme)?;
            let crate::sema::inference::RequirementTemplate::Operation { call, .. } = graph.requirement_template(requirement)? else { return Err(InferenceError::InvalidScheme); };
            let call = graph.operation_call(call)?;
            if graph.resolved(call.result)? != graph.resolved(application.result)? || call.receiver.is_some() || !call.arguments.is_empty()
                || call.effects != EffectSummary::Closed(EffectSet::EMPTY) { return Err(InferenceError::InvalidScheme); }
            let selected = graph.candidate_evidence(requirement)?.ok_or(InferenceError::InvalidScheme)?;
            let super::SolvedOperationAuthority::Language(metadata) = self.operation_catalog.candidate(graph, selected.candidate)? else { return Err(InferenceError::InvalidScheme); };
            if metadata.authority != "language.constructor.tag" || !matches!(metadata.operation, crate::sema::operation_graph::PreparedLanguageOperation::Declaration { authority: "language.constructor.tag", identity } if &*identity.as_str() == format!("{authority:?}")) { return Err(InferenceError::InvalidScheme); }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    #[test]
    fn original_tag_tail_constructor_refuses_erased_and_replaced_checked_statement_sources() {
        let source = "enum Choice { Empty }\npure selected() -> Choice { Empty }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(91), source);
        let mut checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let solved = Arc::get_mut(&mut checked.solved).unwrap();
        let statement = *solved.tag_tail_constructors.keys().next().unwrap();
        let original = solved.tag_tail_constructors.remove(&statement).unwrap();
        assert!(solved.tag_tail_constructor_originally_checked(statement));
        assert!(solved.checked_tag_tail_constructor(statement).is_err());
        assert!(solved.validate().is_err());
        solved.tag_tail_constructors.insert(statement, Arc::clone(&original));
        solved.validate().unwrap();
        solved.tag_tail_constructors.insert(statement, Arc::new((*original).clone()));
        assert!(solved.checked_tag_tail_constructor(statement).is_err());
        assert!(solved.validate().is_err());
    }
}
