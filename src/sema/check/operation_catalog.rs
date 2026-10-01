use crate::sema::inference::{CandidateId, GraphOwner, InferenceContext, InferenceError, SchemeId, ScopedRoot};
use crate::sema::operation_graph::OperationCandidate;
use crate::sema::registry_graph::{RegistryCandidate, RegistryErrorVariant, RegistrySchema};
use crate::sema::stage_graph::StageCandidate;
use crate::symbol::Name;
use std::collections::BTreeMap;

/// Operation authority survives checker disposal without retaining a mutable
/// family lookup cache or rediscovering a signature from a public name.
#[derive(Clone, Debug)]
pub(crate) enum SolvedOperationAuthority {
    Registry(RegistryCandidate),
    Language(OperationCandidate),
    Stage(StageCandidate),
}

impl SolvedOperationAuthority {
    fn identity_and_scheme(&self) -> (Name, SchemeId) {
        match self {
            Self::Registry(candidate) => (candidate.identity, candidate.scheme),
            Self::Language(candidate) => (candidate.identity, candidate.scheme),
            Self::Stage(candidate) => (candidate.identity, candidate.scheme),
        }
    }

    fn retained_bytes(&self) -> usize {
        match self {
            Self::Registry(candidate) => candidate.retained_bytes(),
            Self::Language(_) => 0,
            Self::Stage(candidate) => (candidate.effects.output_pull.len() + candidate.effects.output_close.len()) * std::mem::size_of::<crate::sema::inference::EffectRole>(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct SolvedOperationCatalog {
    owner: GraphOwner,
    candidates: BTreeMap<CandidateId, SolvedOperationAuthority>,
    schemas: BTreeMap<Name, RegistrySchema>,
    errors: BTreeMap<Name, RegistryErrorVariant>,
}

impl SolvedOperationCatalog {
    pub(super) fn new(owner: GraphOwner) -> Self {
        Self { owner, candidates: BTreeMap::new(), schemas: BTreeMap::new(), errors: BTreeMap::new() }
    }

    pub(super) fn insert(&mut self, candidate: CandidateId, authority: SolvedOperationAuthority) -> Result<(), InferenceError> {
        if self.candidates.contains_key(&candidate) { return Err(InferenceError::InvalidScheme); }
        self.candidates.insert(candidate, authority);
        Ok(())
    }

    pub(super) fn insert_schema(&mut self, schema: RegistrySchema) -> Result<(), InferenceError> {
        if self.schemas.insert(schema.authority_id, schema).is_some() { return Err(InferenceError::InvalidScheme); }
        Ok(())
    }

    pub(super) fn insert_error(&mut self, error: RegistryErrorVariant) -> Result<(), InferenceError> {
        if self.errors.insert(error.authority_id, error).is_some() { return Err(InferenceError::InvalidScheme); }
        Ok(())
    }

    pub(crate) fn candidate(&self, graph: &InferenceContext, id: CandidateId) -> Result<&SolvedOperationAuthority, InferenceError> {
        if self.owner != graph.owner() { return Err(InferenceError::ForeignHandle); }
        let candidate = graph.candidate(id)?;
        let authority = self.candidates.get(&id).ok_or(InferenceError::InvalidScheme)?;
        let (identity, scheme) = authority.identity_and_scheme();
        if candidate.identity != identity || candidate.scheme != scheme { return Err(InferenceError::InvalidScheme); }
        if let SolvedOperationAuthority::Registry(metadata) = authority {
            if candidate.public_label != metadata.public_label || candidate.has_receiver != matches!(metadata.owner, crate::sema::registry_graph::RegistryOwner::Method(_)) { return Err(InferenceError::InvalidScheme); }
        }
        if let SolvedOperationAuthority::Stage(_) = authority {
            if !candidate.has_receiver { return Err(InferenceError::InvalidScheme); }
        }
        if let SolvedOperationAuthority::Language(metadata) = authority {
            let receiver = metadata.argument_order == crate::sema::operation_graph::OperationArgumentOrder::ReceiverThenNeedle;
            if candidate.has_receiver != receiver { return Err(InferenceError::InvalidScheme); }
        }
        Ok(authority)
    }

    pub(crate) fn schema(&self, graph: &InferenceContext, authority: Name) -> Result<&RegistrySchema, InferenceError> {
        if self.owner != graph.owner() { return Err(InferenceError::ForeignHandle); }
        let schema = self.schemas.get(&authority).ok_or(InferenceError::InvalidScheme)?;
        graph.node(schema.shape)?;
        Ok(schema)
    }

    pub(crate) fn error(&self, graph: &InferenceContext, authority: Name) -> Result<&RegistryErrorVariant, InferenceError> {
        if self.owner != graph.owner() { return Err(InferenceError::ForeignHandle); }
        let error = self.errors.get(&authority).ok_or(InferenceError::InvalidScheme)?;
        graph.node(error.result)?;
        for parameter in &error.parameters { graph.node(parameter.ty)?; }
        Ok(error)
    }

    pub(super) fn validate(&self, graph: &InferenceContext) -> Result<(), InferenceError> {
        if self.owner != graph.owner() { return Err(InferenceError::ForeignHandle); }
        for &id in self.candidates.keys() { self.candidate(graph, id)?; }
        for root in self.scoped_roots(graph)? { graph.node(root.ty)?; }
        Ok(())
    }

    pub(super) fn scoped_roots(&self, graph: &InferenceContext) -> Result<Vec<ScopedRoot>, InferenceError> {
        let mut roots = Vec::new();
        for authority in self.candidates.values() {
            let (_, scheme) = authority.identity_and_scheme();
            roots.push(ScopedRoot { ty: graph.scheme(scheme)?.body, scope: Some(scheme) });
        }
        roots.extend(self.schemas.values().map(|schema| ScopedRoot { ty: schema.shape, scope: None }));
        for error in self.errors.values() {
            roots.push(ScopedRoot { ty: error.result, scope: None });
            roots.extend(error.parameters.iter().map(|parameter| ScopedRoot { ty: parameter.ty, scope: None }));
        }
        Ok(roots)
    }

    pub(super) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.candidates.len() * size_of::<(CandidateId, SolvedOperationAuthority)>()
            + self.candidates.values().map(SolvedOperationAuthority::retained_bytes).sum::<usize>()
            + self.schemas.len() * size_of::<(Name, RegistrySchema)>()
            + self.errors.len() * size_of::<(Name, RegistryErrorVariant)>()
            + self.errors.values().map(|error| error.parameters.len() * size_of::<crate::sema::registry_graph::RegistryErrorField>() + error.facets.len() * size_of::<Name>()).sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::{Checker, SolvedTypes};
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;
    use std::sync::Arc;

    fn checked(source: &str) -> Arc<SolvedTypes> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        checked.solved.validate().unwrap();
        Arc::clone(&checked.solved)
    }

    #[test]
    fn source_operation_authorities_survive_frontend_disposal_with_exact_owner() {
        let source = "pure subtract(left, right) { left - right }\nlet difference: Int = subtract(3, 1)\nlet length: Int = [1, 2].len()\nlet mapped: List[Int] = [1, 2] |> map { |item| item }\n";
        let solved = checked(source);
        let foreign = checked(source);
        solved.symbol_owner().with_current(|| {
            let catalog = &solved.operation_catalog;
            let mut kinds = std::collections::BTreeSet::new();
            for &id in catalog.candidates.keys() {
                match catalog.candidate(&solved.graph, id).unwrap() {
                    SolvedOperationAuthority::Registry(_) => { kinds.insert("registry"); }
                    SolvedOperationAuthority::Language(_) => { kinds.insert("language"); }
                    SolvedOperationAuthority::Stage(_) => { kinds.insert("stage"); }
                }
                assert!(matches!(catalog.candidate(&foreign.graph, id), Err(InferenceError::ForeignHandle)));
            }
            assert_eq!(kinds, std::collections::BTreeSet::from(["registry", "language", "stage"]));
            assert!(catalog.retained_bytes() > catalog.candidates.len() * std::mem::size_of::<(CandidateId, SolvedOperationAuthority)>());
        });
    }

    #[test]
    fn source_catalog_refuses_metadata_from_another_candidate() {
        let source = "pure subtract(left, right) { left - right }\nlet difference: Int = subtract(3, 1)\n";
        let solved = checked(source);
        let candidates: Vec<_> = solved.operation_catalog.candidates.keys().copied().collect();
        assert!(candidates.len() >= 2);
        let mut corrupt = SolvedOperationCatalog::new(solved.owner);
        corrupt.insert(candidates[0], solved.operation_catalog.candidates[&candidates[1]].clone()).unwrap();
        assert!(matches!(corrupt.candidate(&solved.graph, candidates[0]), Err(InferenceError::InvalidScheme)));
    }
}
