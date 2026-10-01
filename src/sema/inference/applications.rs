use super::{InferenceContext, InferenceError, SchemeId, TypeId};
use crate::source::SourceId;
use crate::symbol::Name;
use crate::syntax::arena::{ExprId, TypeDefId};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApplicationPathComponent {
    Field(Name), Item, Value, Key, Optional, Success, Error,
}

/// Equal structural layouts do not identify the schema selected at a source
/// expression. Paths and ordinals distinguish nested applications and aliases.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ApplicationSource {
    pub source: SourceId,
    pub namespace: Option<Name>,
    pub expression: ExprId,
    pub path: Vec<ApplicationPathComponent>,
    pub ordinal: u32,
}

/// Schema applications name type declarations, never enum payload members.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ApplicationDeclaration {
    pub source: SourceId,
    pub namespace: Option<Name>,
    pub declaration: TypeDefId,
}

/// Original argument handles retain phantom parameters even when no field in
/// the record's structural type depends on them.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ApplicationCertificate {
    pub source: ApplicationSource,
    pub declaration: ApplicationDeclaration,
    pub arguments: Vec<TypeId>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ScopedApplicationRoot {
    pub certificate: ApplicationCertificate,
    pub scope: Option<SchemeId>,
}

impl InferenceContext {
    pub(super) fn validate_application_root_shape(&self, root: &ScopedApplicationRoot) -> Result<(), InferenceError> {
        if root.certificate.source.path.len() > self.limits.structural_depth {
            return Err(InferenceError::Limit("schema application path depth"));
        }
        if let Some(scope) = root.scope { self.scheme(scope)?; }
        for &argument in &root.certificate.arguments { self.node(argument)?; }
        Ok(())
    }
}

pub(super) fn application_certificate_work(root: &ScopedApplicationRoot) -> usize {
    root.certificate.source.path.len().saturating_add(root.certificate.arguments.len()).saturating_add(1)
}

pub(super) fn application_certificate_bytes(root: &ScopedApplicationRoot) -> usize {
    root.certificate.source.path.capacity() * std::mem::size_of::<ApplicationPathComponent>()
        + root.certificate.arguments.capacity() * std::mem::size_of::<TypeId>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{Atom, Generalization, Limits, RetainedStorage};
    use crate::source::Span;

    fn application(argument: TypeId, expression: usize) -> ScopedApplicationRoot {
        ScopedApplicationRoot {
            certificate: ApplicationCertificate {
                source: ApplicationSource { source: SourceId::new(7), namespace: None, expression: ExprId::from_index(expression), path: Vec::new(), ordinal: 0 },
                declaration: ApplicationDeclaration { source: SourceId::new(11), namespace: None, declaration: TypeDefId::from_index(0) },
                arguments: vec![argument],
            },
            scope: None,
        }
    }

    #[test]
    fn application_ledger_preserves_the_complete_original_source_certificate() {
        let mut graph = InferenceContext::default();
        let count = application(graph.atom(Atom::Int).unwrap(), 0);
        let text = application(graph.atom(Atom::Str).unwrap(), 1);
        let roots = vec![count.clone(), text.clone()];
        let solved = graph.freeze_scoped_with_applications(&[], &[], &[], &[], &roots).unwrap();
        solved.validate_application_roots(&roots).unwrap();
        solved.validate_application_roots(&[text.clone(), count.clone()]).unwrap();
        assert!(solved.validate_application_roots(&[count.clone()]).is_err());
        assert!(solved.validate_application_roots(&[count.clone(), count.clone()]).is_err());
        for changed in [
            ScopedApplicationRoot { certificate: ApplicationCertificate { arguments: text.certificate.arguments.clone(), ..count.certificate.clone() }, ..count.clone() },
            ScopedApplicationRoot { certificate: ApplicationCertificate { declaration: ApplicationDeclaration { declaration: TypeDefId::from_index(1), ..count.certificate.declaration }, ..count.certificate.clone() }, ..count.clone() },
            ScopedApplicationRoot { certificate: ApplicationCertificate { source: ApplicationSource { ordinal: 1, ..count.certificate.source.clone() }, ..count.certificate.clone() }, ..count.clone() },
            ScopedApplicationRoot { certificate: ApplicationCertificate { source: ApplicationSource { path: vec![ApplicationPathComponent::Item], ..count.certificate.source.clone() }, ..count.certificate.clone() }, ..count.clone() },
        ] {
            assert!(matches!(solved.validate_application_roots(&[changed, text.clone()]), Err(InferenceError::InvalidScheme)));
        }
        solved.validate_application_roots(&roots).unwrap();
    }

    #[test]
    fn application_ledger_requires_original_argument_graph_and_scheme_ownership() {
        let mut foreign = InferenceContext::default();
        let root = application(foreign.atom(Atom::Int).unwrap(), 0);
        assert!(matches!(InferenceContext::default().freeze_scoped_with_applications(&[], &[], &[], &[], &[root]), Err(InferenceError::ForeignHandle)));
        for scope_index in [Some(0), Some(1), None] {
            let mut graph = InferenceContext::default();
            let span = Span::at(SourceId::new(7), 0);
            let mut scopes = Vec::new();
            for _ in 0..2 {
                let ty = graph.fresh(1, span).unwrap();
                scopes.push(graph.generalize(ty, 0, Generalization::Allowed, &[]).unwrap());
            }
            let binder = graph.scheme_type_binders(scopes[0]).unwrap()[0];
            let mut root = application(binder, 0);
            root.scope = scope_index.map(|index| scopes[index]);
            let result = graph.freeze_scoped_with_applications(&[], &[], &[], &[], &[root.clone()]);
            if scope_index == Some(0) { result.unwrap().validate_application_roots(&[root]).unwrap(); }
            else { assert!(matches!(result, Err(InferenceError::ScopeEscape))); }
        }
    }

    #[test]
    fn application_ledger_publication_respects_path_depth_and_shared_work_budget() {
        for depth in [2, 3] {
            let mut graph = InferenceContext::new(Limits { structural_depth: 2, ..Limits::default() });
            let mut root = application(graph.atom(Atom::Int).unwrap(), 0);
            root.certificate.source.path = vec![ApplicationPathComponent::Optional; depth];
            let result = graph.freeze_scoped_with_applications(&[], &[], &[], &[], &[root]);
            if depth == 2 { assert!(result.is_ok()); }
            else { assert!(matches!(result, Err(InferenceError::Limit("schema application path depth")))); }
        }
        let mut graph = InferenceContext::new(Limits { work_units: 1, ..Limits::default() });
        let root = application(graph.atom(Atom::Int).unwrap(), 0);
        assert!(matches!(graph.freeze_scoped_with_applications(&[], &[], &[], &[], &[root]), Err(InferenceError::Limit("solver work"))));
    }

    #[test]
    fn application_ledger_retained_storage_accounts_both_paths_and_original_arguments() {
        fn storage(path_length: usize, argument_count: usize) -> RetainedStorage {
            let mut graph = InferenceContext::default();
            let argument = graph.atom(Atom::Int).unwrap();
            let mut root = application(argument, 0);
            root.certificate.source.path = vec![ApplicationPathComponent::Optional; path_length];
            root.certificate.arguments = vec![argument; argument_count];
            graph.freeze_scoped_with_applications(&[], &[], &[], &[], &[root]).unwrap().retained_storage()
        }
        let baseline = storage(0, 1);
        let paths = storage(5, 1);
        let arguments = storage(0, 5);
        assert_eq!(paths.validation_bytes - baseline.validation_bytes, 10 * std::mem::size_of::<ApplicationPathComponent>());
        assert_eq!(arguments.validation_bytes - baseline.validation_bytes, 4 * std::mem::size_of::<TypeId>());
        for current in [&baseline, &paths, &arguments] {
            assert_eq!(current.types, 1);
            assert_eq!(current.schemes, 0);
            assert_eq!(current.origins, 0);
            assert_eq!(current.validation_capacity, baseline.validation_capacity);
            assert!(current.validation_bytes >= std::mem::size_of::<(ApplicationSource, ScopedApplicationRoot)>());
        }
    }
}
