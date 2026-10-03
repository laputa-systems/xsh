use super::{Checker, Name, NominalMemberKind, QualifiedNominalIdentity, SolvedNominalMember, Span, Type};
use crate::sema::inference::InferenceError;

#[cfg(test)]
#[path = "nominal_member_plan/tests.rs"]
mod tests;

impl Checker {
    pub(super) fn record_imported_error_nominals(&mut self, surface: Name, family: &super::ErrorFamilyInfo, span: Span) {
        if !self.graph_generation { return; }
        let result = (|| {
            let mut state = self.generic.borrow_mut();
            state.facts.graph.charge_source_fact_nodes((1 + family.variants.len()) as u64)?;
            state.nominal_declarations.insert((self.current_namespace, super::generic::ResolvedNominal::ErrorFamily(surface)), family.identity);
            for (&variant, info) in &family.variants {
                state.nominal_declarations.insert((self.current_namespace, super::generic::ResolvedNominal::ErrorVariant { family: surface, variant }), info.identity);
            }
            Ok::<_, InferenceError>(())
        })();
        if let Err(error) = result { self.graph_error(span, error); }
    }

    // Member payloads belong to their original declaration even when no
    // constructor expression or pattern ever refers to that member.
    pub(super) fn record_checked_nominal_member(&mut self, identity: QualifiedNominalIdentity,
        kind: NominalMemberKind, family: Name, member: Name,
        fields: &[(Option<Name>, Type)], facets: &[Name], span: Span,
    ) {
        if !self.graph_generation { return; }
        let outcome = (|| {
            if fields.iter().any(|(_, ty)| ty.is_recovery()) { return Ok(()); }
            let wire = if kind == NominalMemberKind::Tag { self.wire_enums.mappings.get(&family).cloned() } else { None };
            {
                let mut state = self.generic.borrow_mut();
                state.facts.graph.charge_source_fact_work((fields.len() + facets.len() + 1) as u64)?;
                if let Some(original) = state.facts.nominal_members.get(&identity) {
                    if original.kind != kind || original.family != family || original.member != member
                        || original.facets != facets || original.fields.len() != fields.len()
                        || !match (&original.wire, &wire) { (None, None) => true, (Some(original), Some(current)) => std::sync::Arc::ptr_eq(original, current), _ => false } {
                        return Err(InferenceError::InvalidScheme);
                    }
                    for ((label, ty), (original_label, original_ty)) in fields.iter().zip(&original.fields) {
                        let checked = match ty { Type::Graph(ty) => state.facts.graph.export_type(*ty)?, _ => ty.clone() };
                        if label != original_label || state.facts.graph.export_type(*original_ty)? != checked {
                            return Err(InferenceError::InvalidScheme);
                        }
                    }
                    return Ok(());
                }
            }
            let mut checked_fields = Vec::with_capacity(fields.len());
            for (label, ty) in fields {
                let ty = match self.graph_type(ty, span) {
                    Ok(ty) => ty,
                    Err(InferenceError::Boundary(_) | InferenceError::Unresolved(_) | InferenceError::Recovery(_)) => return Ok(()),
                    Err(error) => return Err(error),
                };
                match self.generic.borrow().facts.graph.export_type(ty) {
                    Ok(_) => checked_fields.push((*label, ty)),
                    Err(InferenceError::Boundary(_) | InferenceError::Unresolved(_) | InferenceError::Recovery(_)) => return Ok(()),
                    Err(error) => return Err(error),
                }
            }
            let tested = match kind {
                NominalMemberKind::Tag => Type::Tag(family),
                NominalMemberKind::Error => Type::ErrorVariant { family, variant: member },
            };
            let tested = self.graph_type(&tested, span)?;
            let mut state = self.generic.borrow_mut();
            if let Some(previous) = state.facts.nominals.get(&tested) {
                let expected = match (kind, identity) {
                    (NominalMemberKind::Tag, QualifiedNominalIdentity::Source { source, namespace, declaration, .. }) => QualifiedNominalIdentity::Source { source, namespace, declaration, member: None },
                    _ => identity,
                };
                if *previous != expected { return Err(InferenceError::ScopeEscape); }
            } else if kind == NominalMemberKind::Error {
                state.facts.nominals.insert(tested, identity);
            }
            state.facts.publish_nominal_member(identity, SolvedNominalMember {
                kind, family, member, tested, fields: checked_fields, facets: facets.to_vec(), wire, scope: None,
            })
        })();
        if let Err(error) = outcome { self.graph_error(span, error); }
    }
}
