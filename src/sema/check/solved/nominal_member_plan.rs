use super::*;
use crate::sema::inference::{Atom, TypeNode};
use std::collections::BTreeSet;

impl<Graph> SolvedTypes<Graph> {
    pub(crate) fn checked_nominal_family_members(&self, family: QualifiedNominalIdentity) -> Result<Vec<(QualifiedNominalIdentity, std::sync::Arc<SolvedNominalMember>)>, InferenceError> {
        let QualifiedNominalIdentity::Source { declaration: NominalDeclaration::Type(_), member: None, .. } = family else { return Err(InferenceError::InvalidScheme); };
        let mut members = Vec::new();
        for (&identity, original) in &self.original_nominal_members {
            let QualifiedNominalIdentity::Source { source, namespace, declaration, member: Some(_) } = identity else { continue; };
            if (QualifiedNominalIdentity::Source { source, namespace, declaration, member: None }) != family { continue; }
            self.checked_nominal_member(identity)?;
            members.push((identity, std::sync::Arc::clone(original)));
        }
        if members.is_empty() { return Err(InferenceError::InvalidScheme); }
        Ok(members)
    }

    pub(crate) fn checked_nominal_member(&self, identity: QualifiedNominalIdentity) -> Result<&SolvedNominalMember, InferenceError> {
        let original = self.original_nominal_members.get(&identity).ok_or(InferenceError::InvalidScheme)?;
        let current = self.nominal_members.get(&identity).ok_or(InferenceError::InvalidScheme)?;
        if (!std::sync::Arc::ptr_eq(original, current) && original.as_ref() != current.as_ref())
            || !match (&original.wire, &current.wire) { (None, None) => true, (Some(original), Some(current)) => std::sync::Arc::ptr_eq(original, current), _ => false } { return Err(InferenceError::InvalidScheme); }
        Ok(original.as_ref())
    }

    pub(super) fn nominal_member_roots(&self) -> Vec<ScopedRoot> {
        self.nominal_members.values().flat_map(|member| std::iter::once(member.tested).chain(member.fields.iter().map(|(_, ty)| *ty)).map(|ty| ScopedRoot { ty, scope: member.scope })).collect()
    }

    pub(super) fn nominal_member_payload_bytes(&self) -> usize {
        let mut allocations = BTreeSet::new();
        self.nominal_members.values().chain(self.original_nominal_members.values()).filter(|member| allocations.insert(std::sync::Arc::as_ptr(member) as usize)).map(|member| {
            std::mem::size_of::<SolvedNominalMember>() + 2 * std::mem::size_of::<usize>()
                + member.fields.capacity() * std::mem::size_of::<(Option<Name>, TypeId)>()
                + member.facets.capacity() * std::mem::size_of::<Name>()
        }).sum::<usize>() + {
            let mut mappings = BTreeSet::new();
            self.nominal_members.values().chain(self.original_nominal_members.values()).filter_map(|member| member.wire.as_ref()).filter(|wire| mappings.insert(std::sync::Arc::as_ptr(wire) as usize)).map(|wire| {
                std::mem::size_of::<crate::sema::wire_enums::WireEnumMapping>() + 2 * std::mem::size_of::<usize>()
                    + wire.variants.len() * std::mem::size_of::<(Name, std::sync::Arc<str>)>()
                    + wire.variants.values().map(|value| value.len() + 2 * std::mem::size_of::<usize>()).sum::<usize>()
            }).sum::<usize>()
        }
    }

    pub(super) fn nominal_member_source_work(&self) -> u64 {
        self.nominal_members.values().map(|member| 2 + member.fields.len() + member.facets.len() + member.wire.as_ref().map_or(0, |wire| wire.variants.len() + wire.variants.values().map(|value| value.len()).sum::<usize>())).sum::<usize>() as u64
    }

    pub(super) fn validate_nominal_members(&self, graph: &InferenceContext) -> Result<(), InferenceError> {
        if self.nominal_members.len() != self.original_nominal_members.len() { return Err(InferenceError::InvalidScheme); }
        for (&identity, member) in &self.nominal_members {
            self.checked_nominal_member(identity)?;
            if member.scope.is_some() { return Err(InferenceError::Boundary("scoped nominal declaration members are unavailable")); }
            let selected = match (member.kind, identity) {
                (NominalMemberKind::Tag, QualifiedNominalIdentity::Source { declaration: NominalDeclaration::Type(_), member, .. }) => member,
                (NominalMemberKind::Error, QualifiedNominalIdentity::Source { declaration: NominalDeclaration::Error(_), member, .. }) => member,
                (NominalMemberKind::Error, QualifiedNominalIdentity::Builtin { family, member: selected }) if family == member.family => selected,
                _ => return Err(InferenceError::InvalidScheme),
            };
            if selected != Some(member.member) { return Err(InferenceError::InvalidScheme); }
            if let Some(wire) = &member.wire {
                if member.kind != NominalMemberKind::Tag || !member.fields.is_empty() || wire.type_name != member.family || !wire.variants.contains_key(&member.member) { return Err(InferenceError::InvalidScheme); }
                let mut values = BTreeSet::new();
                for (&name, value) in &wire.variants { if self.symbols.resolve(name).is_none() || !values.insert(value.as_ref()) { return Err(InferenceError::InvalidScheme); } }
            }
            let expected = match (member.kind, identity) {
                (NominalMemberKind::Tag, QualifiedNominalIdentity::Source { source, namespace, declaration, .. }) => QualifiedNominalIdentity::Source { source, namespace, declaration, member: None },
                _ => identity,
            };
            let tested = graph.resolved(member.tested)?;
            if self.nominals.get(&tested) != Some(&expected) { return Err(InferenceError::InvalidScheme); }
            let mut labels = BTreeSet::new();
            for &(label, ty) in &member.fields {
                match (member.kind, label) {
                    (NominalMemberKind::Tag, None) => {},
                    (NominalMemberKind::Error, Some(label)) if labels.insert(label) => {},
                    _ => return Err(InferenceError::InvalidScheme),
                }
                graph.export_type(ty)?;
            }
            let namespace = match identity { QualifiedNominalIdentity::Source { namespace, .. } => namespace, _ => None };
            let names = std::iter::once(member.family).chain(std::iter::once(member.member)).chain(namespace).chain(member.fields.iter().filter_map(|(label, _)| *label)).chain(member.facets.iter().copied());
            if names.into_iter().any(|name| self.symbols.resolve(name).is_none()) { return Err(InferenceError::ForeignHandle); }
            match (member.kind, graph.node(tested)?) {
                (NominalMemberKind::Tag, TypeNode::Atom(Atom::Tag(family))) if *family == member.family && member.facets.is_empty() => {},
                (NominalMemberKind::Error, TypeNode::Atom(Atom::ErrorVariant { family, variant })) if (*family, *variant) == (member.family, member.member) => {},
                _ => return Err(InferenceError::InvalidScheme),
            }
        }
        Ok(())
    }
}

impl SolvedTypes<InferenceContext> {
    pub(in crate::sema::check) fn publish_nominal_member(&mut self, identity: QualifiedNominalIdentity, member: SolvedNominalMember) -> Result<(), InferenceError> {
        if let Some(original) = self.original_nominal_members.get(&identity) {
            return if original.as_ref() == &member && match (&original.wire, &member.wire) { (None, None) => true, (Some(original), Some(current)) => std::sync::Arc::ptr_eq(original, current), _ => false } { Ok(()) } else { Err(InferenceError::InvalidScheme) };
        }
        self.graph.charge_source_fact_nodes(2)?;
        self.graph.charge_source_fact_edges((3 + member.fields.len() + member.facets.len() + usize::from(member.wire.is_some())) as u64)?;
        let member = std::sync::Arc::new(member);
        self.nominal_members.insert(identity, std::sync::Arc::clone(&member));
        self.original_nominal_members.insert(identity, member);
        Ok(())
    }
}
