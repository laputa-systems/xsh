use super::*;
use crate::sema::inference::{Eligibility, ScopedRequirementRoot};
use super::super::super::generic::OriginalScopedEligibility;

impl FullBuilder {
    pub(super) fn prepare_original_eligibility(&mut self, solved: &SolvedTypes, scheme: SchemeId, scope: SchemeScopeId) -> Result<(), IrBuildError> {
        let original = solved.graph.scheme(scheme).map_err(|_| problem("generic_eligibility_scheme"))?;
        for (index, template) in original.requirements.iter().enumerate() {
            let RequirementTemplate::Eligibility { predicate: Eligibility::Display, ty } = *template else { continue; };
            let requirement = *original.requirement_origins.get(index).ok_or_else(|| problem("generic_eligibility_original"))?;
            let root = ScopedRequirementRoot { requirement, scope: Some(scheme) };
            solved.graph.validate_requirement_scoped(root).map_err(|_| problem("generic_eligibility_source_scope"))?;
            let RequirementTemplate::Eligibility { predicate: source_predicate, ty: source_ty } =
                solved.graph.requirement_template(requirement).map_err(|_| problem("generic_eligibility_original"))?
            else { return Err(problem("generic_eligibility_original_template")); };
            let ty = self.reference(&solved.graph, scheme, ty)?;
            // Generalization retains the source requirement while replacing
            // its type handle with the declaration's rigid binder.
            if source_predicate != Eligibility::Display || self.reference(&solved.graph, scheme, source_ty)? != ty {
                return Err(problem("generic_eligibility_original_template"));
            }
            self.generic_evidence_mut().add_original_eligibility(OriginalScopedEligibility {
                scope, requirement: u32::try_from(index).map_err(|_| problem("generic_eligibility_index"))?, original: root,
                predicate: Eligibility::Display, ty,
            }).map_err(|_| problem("generic_eligibility_allocation"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;

    // Only host proof mutation can express a forged requirement or a borrowed
    // witness after the checked source and frontend facts have been discarded.
    fn fixture(source: &str) -> FullProgram {
        let name = "scoped-display-witnesses.xsh";
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            name, crate::loader::entry_source_from_text(name, source.to_owned()), Vec::new());
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let source_id = SourceMap::files(&sources).first().unwrap().id();
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let program = FullBuilder::build_compact(&parsed.arena, &declarations, &bodies, source, Arc::new(sources), source_id).unwrap();
        drop(bodies); drop(declarations); drop(parsed);
        FullVerifier::verify(&program).unwrap();
        program
    }

    fn rejected(program: &FullProgram) {
        assert!(FullVerifier::verify_generic_evidence(&program.store).is_err());
        assert!(FullVerifier::verify(program).is_err());
    }

    #[test]
    fn generic_display_witnesses_reject_wrong_types_missing_and_coforged_obligations() {
        std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
            let original = fixture(r#"
pure rendered(value) -> Path { fp"entry-${value}" }
let number = rendered(7)
let word = rendered("leaf")
"#);
            let symbols = original.symbol_owner().clone();
            let _symbols = symbols.enter();
            let generic = original.generic_evidence().unwrap();
            let instances = generic.instances().filter_map(|(id, instance)| {
                let (index, witness) = instance.requirements.iter().copied().enumerate()
                    .find(|(_, witness)| matches!(witness, RequirementWitness::Eligibility { .. }))?;
                Some((id, instance.scope, index, witness))
            }).collect::<Vec<_>>();
            assert_eq!(instances.len(), 2);
            assert_ne!(instances[0].3, instances[1].3);

            let mut wrong = original.clone();
            wrong.store.generic.as_deref_mut().unwrap().test_eligibility_instance_mut(instances[0].0).unwrap()
                .requirements[instances[0].2] = instances[1].3;
            rejected(&wrong);

            let mut missing = original.clone();
            missing.store.generic.as_deref_mut().unwrap().test_eligibility_instance_mut(instances[0].0).unwrap()
                .requirements = Box::new([]);
            rejected(&missing);

            let RequirementWitness::Eligibility { predicate, ty } = instances[1].3 else { unreachable!() };
            let mut coforged = original.clone();
            let generic = coforged.store.generic.as_deref_mut().unwrap();
            generic.test_scope_mut(instances[0].1).unwrap().requirements[instances[0].2] =
                Requirement::Eligibility { predicate, ty: TypeRef::Ground(ty) };
            for &(id, _, index, _) in &instances {
                generic.test_eligibility_instance_mut(id).unwrap().requirements[index] = instances[1].3;
            }
            rejected(&coforged);

            let mut no_original = original.clone();
            no_original.store.generic.as_deref_mut().unwrap().test_remove_eligibility_sources();
            rejected(&no_original);
        }).unwrap().join().unwrap();
    }

    #[test]
    fn unused_generic_display_forwarding_rejects_another_callers_binder() {
        std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
            let mut program = fixture(r#"
pure rendered(value) -> Path { fp"entry-${value}" }
pure unused(first, second) {
    let one = rendered(first)
    rendered(second)
}
"#);
            let symbols = program.symbol_owner().clone();
            let _symbols = symbols.enter();
            let generic = program.generic_evidence().unwrap();
            assert_eq!(generic.instances().count(), 0);
            let plan = generic.calls().iter().find_map(|call| match call.evidence {
                CallEvidence::Forwarded(plan) => Some(plan), _ => None,
            }).unwrap();
            let forwarding = generic.forwarding(plan).unwrap();
            let ForwardedRequirement::Caller(index) = forwarding.requirements[0] else { unreachable!() };
            let caller = generic.scope(forwarding.caller).unwrap();
            let wrong = caller.requirements.iter().enumerate().find(|(other, requirement)| {
                *other != index as usize && matches!(requirement, Requirement::Eligibility { .. })
                    && *requirement != &caller.requirements[index as usize]
            }).unwrap().0 as u32;
            program.store.generic.as_deref_mut().unwrap().test_forwarding_mut(plan).unwrap()
                .requirements[0] = ForwardedRequirement::Caller(wrong);
            rejected(&program);
        }).unwrap().join().unwrap();
    }
}
