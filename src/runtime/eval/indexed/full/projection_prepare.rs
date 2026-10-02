use super::*;
use super::super::generic::{GroundProjectionSource, PhysicalLayout, PhysicalLayoutId, PreparedGroundProjection, SolvedRecordLayout, graph_ground_type};

fn projection_problem(construct: &'static str) -> IrBuildError { IrBuildError::format(construct, None, 0, 0) }

impl FullBuilder {
    pub(super) fn prepare_ground_projections(&mut self) -> Result<(), IrBuildError> {
        let Some(solved) = self.solved.as_ref().cloned() else { return Ok(()); };
        let rows = self.generic_expression_rows.clone();
        let mut origins = FxHashMap::default();
        for &(instruction, expression, owner) in &rows {
            if origins.insert(instruction, (expression, owner)).is_some_and(|previous| previous != (expression, owner)) {
                return Err(projection_problem("ground_projection_original_instruction"));
            }
        }
        let mut constructors = FxHashMap::default();
        let mut layouts = BTreeMap::new();
        if let Some(generic) = self.generic.as_ref() {
            for constructor in generic.constructors() {
                let layout = generic.layout(constructor.layout).map_err(|_| projection_problem("ground_projection_layout_owner"))?;
                constructors.insert(constructor.instruction, constructor.layout);
                layouts.insert(layout.record_type, constructor.layout);
            }
        }
        // A fixed record is still an original producer when its declaration
        // needs no generic frame. Its layout proves every supplied field.
        for &(instruction, expression, owner) in &rows {
            if self.store.tags[instruction as usize] != FullTag::ExprRecord || constructors.contains_key(&instruction) { continue; }
            let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| projection_problem("ground_projection_record_payload"))?;
            let block = words.first().and_then(|&id| IrBlockId::from_raw(id)).and_then(|id| self.store.blocks.get(id.index()))
                .ok_or_else(|| projection_problem("ground_projection_record_fields"))?;
            let fields = self.store.payload(block.instructions).map_err(|_| projection_problem("ground_projection_record_fields"))?;
            let count = fields.first().copied().ok_or_else(|| projection_problem("ground_projection_record_fields"))? as usize;
            let entries = &fields[1..];
            let mut position = 0usize;
            let mut fixed = true;
            for _ in 0..count {
                let width = match entries.get(position) {
                    Some(0) => 3,
                    Some(1) => { fixed = false; 2 },
                    _ => return Err(projection_problem("ground_projection_record_fields")),
                };
                position = position.checked_add(width).filter(|&end| end <= entries.len()).ok_or_else(|| projection_problem("ground_projection_record_fields"))?;
            }
            if position != entries.len() { return Err(projection_problem("ground_projection_record_fields")); }
            // A spread retains its existing producer path. A projection that
            // consumes it still needs independent expanded-field evidence.
            if !fixed { continue; }
            let Some(&checked) = solved.expressions.get(&expression) else { continue; };
            let Ok(ty @ Type::Record(_)) = graph_ground_type(&solved.graph, checked) else { continue; };
            let ty = self.intern_generic_ground_type(&ty)?;
            let layout = self.ground_projection_layout(ty, &mut layouts)?;
            self.generic_evidence_mut().add_constructor(SolvedRecordLayout { instruction, owner, layout });
            constructors.insert(instruction, layout);
        }
        for (instruction, expression, owner) in rows {
            if self.generic_projection_uses.contains_key(&expression) { continue; }
            let Some(projection) = solved.projections.get(&expression) else { continue; };
            let Ok(receiver @ Type::Record(_)) = graph_ground_type(&solved.graph, projection.receiver) else { continue; };
            let Ok(result) = graph_ground_type(&solved.graph, projection.result) else { continue; };
            if self.store.tags[instruction as usize] != FullTag::ExprField { return Err(projection_problem("ground_projection_instruction_kind")); }
            let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| projection_problem("ground_projection_instruction_payload"))?;
            let receiver_instruction = *words.first().ok_or_else(|| projection_problem("ground_projection_receiver"))?;
            let field = words.get(1).copied().ok_or_else(|| projection_problem("ground_projection_field"))?;
            if self.store.string(field).map_err(|_| projection_problem("ground_projection_field"))? != projection.field.as_str().as_str() {
                return Err(projection_problem("ground_projection_original_field"));
            }
            let &(receiver_origin, receiver_owner) = origins.get(&receiver_instruction).ok_or_else(|| projection_problem("ground_projection_receiver_origin"))?;
            if receiver_owner != owner { return Err(projection_problem("ground_projection_receiver_owner")); }
            let &original_receiver = solved.expressions.get(&receiver_origin).ok_or_else(|| projection_problem("ground_projection_receiver_type"))?;
            if graph_ground_type(&solved.graph, original_receiver).map_err(|_| projection_problem("ground_projection_receiver_type"))? != receiver {
                return Err(projection_problem("ground_projection_original_receiver"));
            }
            let &original_result = solved.expressions.get(&expression).ok_or_else(|| projection_problem("ground_projection_result_type"))?;
            if graph_ground_type(&solved.graph, original_result).map_err(|_| projection_problem("ground_projection_result_type"))? != result {
                return Err(projection_problem("ground_projection_original_result"));
            }
            let receiver = self.intern_generic_ground_type(&receiver)?;
            let result = self.intern_generic_ground_type(&result)?;
            let layout = self.ground_projection_layout(receiver, &mut layouts)?;
            let physical = self.generic.as_ref().unwrap().layout(layout).map_err(|_| projection_problem("ground_projection_layout_owner"))?;
            let field_slot = physical.fields.iter().position(|&(field, _)| field == projection.field).ok_or_else(|| projection_problem("ground_projection_layout_field"))?;
            if physical.fields[field_slot].1 != result { return Err(projection_problem("ground_projection_layout_result")); }
            let field_slot = u32::try_from(field_slot).map_err(|_| projection_problem("ground_projection_slot_overflow"))?;
            let scope = solved.expression_owners.get(&expression).and_then(|declaration| self.generic_declarations.get(declaration).copied());
            let source = self.generic_evidence_mut().add_ground_projection_source(GroundProjectionSource {
                origin: expression, instruction, owner, scope, receiver_origin, receiver_instruction,
                field: projection.field, receiver, result, layout, field_slot,
            }).map_err(|_| projection_problem("ground_projection_source_allocation"))?;
            self.generic_evidence_mut().add_ground_projection(PreparedGroundProjection {
                source, receiver_instruction, receiver: TypeRef::Ground(receiver), result: TypeRef::Ground(result), layout, field_slot,
            }).map_err(|_| projection_problem("ground_projection_proof_allocation"))?;
        }
        Ok(())
    }

    fn ground_projection_layout(&mut self, ty: TypeId, layouts: &mut BTreeMap<TypeId, PhysicalLayoutId>) -> Result<PhysicalLayoutId, IrBuildError> {
        if let Some(&layout) = layouts.get(&ty) { return Ok(layout); }
        let (names, types) = self.store.semantic.record_fields(ty).map_err(|_| projection_problem("ground_projection_closed_record"))?;
        let fields = names.iter().copied().zip(types.iter().copied()).map(|(name, ty)| {
            TypeId::from_raw(ty).map(|ty| (name, ty)).ok_or_else(|| projection_problem("ground_projection_field_type"))
        }).collect::<Result<Vec<_>, _>>()?;
        let layout = self.generic_evidence_mut().add_layout(PhysicalLayout { record_type: ty, fields: fields.into_boxed_slice() })
            .map_err(|_| projection_problem("ground_projection_layout_allocation"))?;
        layouts.insert(ty, layout);
        Ok(layout)
    }
}

impl FullVerifier {
    pub(super) fn verify_ground_projections(store: &FullStore, generic: &GenericEvidenceStore) -> Result<(), IrVerifyError> {
        for (_, projection) in generic.ground_projections() {
            let source = generic.ground_projection_source(projection.source)?;
            let TypeRef::Ground(result) = projection.result else { return Err(IrVerifyError::new("ground projection result is not closed")); };
            Self::verify_ground_projection_operand(store, generic, source.instruction, source.owner, &store.semantic.to_type(result)?, &mut Vec::new())?;
        }
        Ok(())
    }

    pub(super) fn verify_ground_projection_operand(store: &FullStore, generic: &GenericEvidenceStore, instruction: u32, owner: InstructionOwner, expected: &Type, active: &mut Vec<u32>) -> Result<(), IrVerifyError> {
        let id = generic.ground_projection_at(instruction)?.ok_or_else(|| IrVerifyError::new("original ground projection lacks its prepared proof"))?;
        let projection = generic.ground_projection(id)?;
        let source = generic.ground_projection_source(projection.source)?;
        if source.instruction != instruction || source.owner != owner { return Err(IrVerifyError::new("ground projection proof belongs to another instruction owner")); }
        if store.tags.get(instruction as usize) != Some(&FullTag::ExprField) { return Err(IrVerifyError::new("ground projection proof is attached to another opcode")); }
        if generic.registered_instruction_origin(instruction, false) != Some((super::super::generic::OperationSourceOrigin::Expression(source.origin), owner))
            || generic.registered_instruction_origin(source.receiver_instruction, false) != Some((super::super::generic::OperationSourceOrigin::Expression(source.receiver_origin), owner)) {
            return Err(IrVerifyError::new("ground projection disagrees with its original instruction or receiver"));
        }
        let words = store.payload(store.data[instruction as usize].range())?;
        if words.first() != Some(&projection.receiver_instruction) || words.get(1).copied().and_then(|field| store.string(field).ok()) != Some(source.field.as_str().as_str()) {
            return Err(IrVerifyError::new("ground projection receiver or field differs from its original source"));
        }
        if words.get(2).and_then(|&location| store.location_sources.get(location as usize)) != Some(&source.origin.source) {
            return Err(IrVerifyError::new("ground projection differs from its original source location"));
        }
        let TypeRef::Ground(receiver) = projection.receiver else { return Err(IrVerifyError::new("ground projection receiver is not closed")); };
        let TypeRef::Ground(result) = projection.result else { return Err(IrVerifyError::new("ground projection result is not closed")); };
        if store.semantic.to_type(result)? != *expected { return Err(IrVerifyError::new("ground projection result disagrees with its consumer")); }
        let already_active = active.last() == Some(&instruction);
        if active.len() >= 256 || (!already_active && active.contains(&instruction)) { return Err(IrVerifyError::new("ground projection receiver is cyclic or too deep")); }
        if !already_active { active.push(instruction); }
        Self::verify_generic_source(store, generic, projection.receiver_instruction, owner, &store.semantic.to_type(receiver)?, None, active)?;
        if !already_active { active.pop(); }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::operation_prepare::tests::source_fixture;
    use crate::sema::inference::Atom;
    use crate::sema::operation_graph::PreparedLanguageOperation;

    #[test]
    fn selected_string_equality_retains_fixed_record_field_operand_proofs_after_frontend_drop() {
        let source = "pure compare(value: Str) -> Bool { let record = {text: value}; record.text == \"yes\" }\nlet result = compare(\"yes\")\n";
        let program = source_fixture(source, PreparedLanguageOperation::Equality { op: BinaryOp::Eq });
        assert_eq!(program.generic_evidence().unwrap().ground_projections().count(), 1);
        assert_eq!(program.generic_evidence().unwrap().constructors().len(), 1);
    }

    #[test]
    fn fixed_record_field_proofs_preserve_the_original_generic_scope_for_unrelated_parameters() {
        let source = "pure compare(unused, value: Str) -> Bool { let record = {text: value}; record.text == \"yes\" }\nlet result = compare(7, \"yes\")\n";
        let program = source_fixture(source, PreparedLanguageOperation::Equality { op: BinaryOp::Eq });
        let generic = program.generic_evidence().unwrap();
        let (_, proof) = generic.ground_projections().next().unwrap();
        let source = generic.ground_projection_source(proof.source).unwrap();
        let scope = source.scope.expect("the original generic declaration scope must survive a closed projection");
        assert_eq!(source.owner, InstructionOwner::Function(generic.scope(scope).unwrap().owner));
        assert_eq!(generic.scope(scope).unwrap().quantifiers.len(), 1);
        assert!(matches!(proof.receiver, TypeRef::Ground(_)) && matches!(proof.result, TypeRef::Ground(_)));
    }

    #[test]
    fn unrelated_record_spreads_do_not_acquire_fixed_literal_layout_proofs() {
        let source = "pure compare(value: Str) -> Bool { let copied = {...{text: value}}; value == \"yes\" }\nlet result = compare(\"yes\")\n";
        let program = source_fixture(source, PreparedLanguageOperation::Equality { op: BinaryOp::Eq });
        assert_eq!(program.generic_evidence().unwrap().ground_projections().count(), 0);
    }

    #[test]
    fn fixed_record_field_proofs_reject_missing_wrong_slot_domain_and_rewritten_field() {
        let source = "pure compare(left: Str, right: Str) -> Bool { let first = {text: left, suffix: right}; let second = {text: right, suffix: left}; first.text == second.text and first.suffix == second.suffix }\nlet result = compare(\"yes\", \"no\")\n";
        let program = source_fixture(source, PreparedLanguageOperation::Equality { op: BinaryOp::Eq });
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            assert_eq!(generic.ground_projections().count(), 4);
            let (id, proof) = generic.ground_projections().find(|(_, proof)| generic.ground_projection_source(proof.source).unwrap().field == "text").unwrap();
            let original = generic.ground_projection_source(proof.source).unwrap();
            let (other_id, other) = generic.ground_projections().find(|(_, proof)| generic.ground_projection_source(proof.source).unwrap().field == "suffix").unwrap();
            let other_source = generic.ground_projection_source(other.source).unwrap();
            let mut missing = program.store.clone();
            missing.generic.as_deref_mut().unwrap().test_remove_ground_projections();
            assert!(FullVerifier::verify_generic_evidence(&missing).unwrap_err().message.contains("missing"));
            let mut wrong_slot = program.store.clone();
            wrong_slot.generic.as_deref_mut().unwrap().test_ground_projection_mut(id).unwrap().field_slot = other.field_slot;
            assert!(FullVerifier::verify_generic_evidence(&wrong_slot).is_err());
            let mut wrong_result = program.store.clone();
            let boolean = SemanticPoolBuilder::default().intern_type(&mut wrong_result.semantic, &Type::Bool).unwrap();
            wrong_result.generic.as_deref_mut().unwrap().test_ground_projection_mut(id).unwrap().result = TypeRef::Ground(boolean);
            assert!(FullVerifier::verify_generic_evidence(&wrong_result).is_err());
            let mut rewritten = program.store.clone();
            let other_words = rewritten.payload(rewritten.data[other_source.instruction as usize].range()).unwrap();
            let other_field = other_words[1];
            let range = rewritten.data[original.instruction as usize].range();
            rewritten.extra[range.start as usize + 1] = other_field;
            rewritten.generic.as_deref_mut().unwrap().test_ground_projection_mut(id).unwrap().field_slot = other.field_slot;
            assert!(FullVerifier::verify_generic_evidence(&rewritten).is_err(), "same-typed fields cannot rewrite their independent original source proof");
            let mut substituted = program.store.clone();
            substituted.generic.as_deref_mut().unwrap().test_ground_projection_mut(id).unwrap().source = other.source;
            assert!(FullVerifier::verify_generic_evidence(&substituted).is_err());
            assert_ne!(id, other_id);
        });
    }

    #[test]
    fn fixed_record_field_proofs_reject_coforged_field_slot_and_source_selection() {
        let source = "pure compare(left: Str, right: Str) -> Bool { let record = {text: left, suffix: right}; record.text == record.suffix }\nlet result = compare(\"yes\", \"no\")\n";
        let program = source_fixture(source, PreparedLanguageOperation::Equality { op: BinaryOp::Eq });
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, proof) = generic.ground_projections().find(|(_, proof)| generic.ground_projection_source(proof.source).unwrap().field == "text").unwrap();
            let original = generic.ground_projection_source(proof.source).unwrap();
            let (_, other) = generic.ground_projections().find(|(_, proof)| generic.ground_projection_source(proof.source).unwrap().field == "suffix").unwrap();
            let alternate = generic.ground_projection_source(other.source).unwrap();
            assert_eq!(proof.layout, other.layout);
            assert_eq!(proof.result, other.result);
            let mut forged = program.store.clone();
            let alternate_field = forged.payload(forged.data[alternate.instruction as usize].range()).unwrap()[1];
            let range = forged.data[original.instruction as usize].range();
            forged.extra[range.start as usize + 1] = alternate_field;
            let evidence = forged.generic.as_deref_mut().unwrap();
            evidence.test_ground_projection_mut(id).unwrap().field_slot = other.field_slot;
            let rewritten_source = evidence.test_ground_projection_source_mut(proof.source).unwrap();
            rewritten_source.field = alternate.field;
            rewritten_source.field_slot = other.field_slot;
            let failure = FullVerifier::verify_generic_evidence(&forged).unwrap_err();
            assert!(failure.message.contains("original receipt"), "same-typed fields must retain their original selection when all mutable copies agree: {}", failure.message);
        });
    }

    #[test]
    fn fixed_record_field_proofs_require_original_receiver_producers_and_owned_roots() {
        let source = "pure compare(value: Str) -> Bool { let record = {text: value}; record.text == \"yes\" }\nlet result = compare(\"yes\")\n";
        let program = source_fixture(source, PreparedLanguageOperation::Equality { op: BinaryOp::Eq });
        let foreign = source_fixture(source, PreparedLanguageOperation::Equality { op: BinaryOp::Eq });
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (id, proof) = generic.ground_projections().next().unwrap();
            let source = generic.ground_projection_source(proof.source).unwrap();
            let mut wrong_owner = program.store.clone();
            wrong_owner.generic.as_deref_mut().unwrap().test_ground_projection_source_mut(proof.source).unwrap().owner = InstructionOwner::Driver(0);
            assert!(FullVerifier::verify_generic_evidence(&wrong_owner).is_err());
            let mut wrong_origin = program.store.clone();
            wrong_origin.generic.as_deref_mut().unwrap().test_ground_projection_source_mut(proof.source).unwrap().receiver_origin.source = SourceId::new(123);
            assert!(FullVerifier::verify_generic_evidence(&wrong_origin).is_err());
            let mut wrong_opcode = program.store.clone();
            wrong_opcode.tags[source.instruction as usize] = FullTag::ExprIndex;
            assert!(FullVerifier::verify_generic_evidence(&wrong_opcode).unwrap_err().message.contains("another opcode"));
            let mut wrong_producer = program.store.clone();
            let constructor = generic.constructors()[0].instruction as usize;
            wrong_producer.tags[constructor] = FullTag::ExprList;
            assert!(FullVerifier::verify_generic_evidence(&wrong_producer).is_err(), "a cached receiver shape cannot replace its original producer proof");
            let mut foreign_layout = program.store.clone();
            let (_, foreign_proof) = foreign.generic_evidence().unwrap().ground_projections().next().unwrap();
            foreign_layout.generic.as_deref_mut().unwrap().test_ground_projection_mut(id).unwrap().layout = foreign_proof.layout;
            assert!(FullVerifier::verify_generic_evidence(&foreign_layout).is_err());
            let mut foreign_source = program.store.clone();
            foreign_source.generic.as_deref_mut().unwrap().test_ground_projection_mut(id).unwrap().source = foreign_proof.source;
            assert!(FullVerifier::verify_generic_evidence(&foreign_source).is_err());
        });
    }

    #[test]
    fn fixed_record_field_proof_lifetimes_keep_cache_roots_and_retained_storage_owned() {
        use super::super::super::generic::{GenericEvidenceBuilder, OperationSourceOrigin};
        let source = "pure compare(value: Str) -> Bool { let record = {text: value}; record.text == \"yes\" }\nlet result = compare(\"yes\")\n";
        let program = source_fixture(source, PreparedLanguageOperation::Equality { op: BinaryOp::Eq });
        program.symbol_owner().with_current(|| {
            let generic = program.generic_evidence().unwrap();
            let (original_id, original_proof) = generic.ground_projections().next().unwrap();
            let mut original = generic.ground_projection_source(original_proof.source).unwrap().clone();
            let mut builder = GenericEvidenceBuilder::default();
            assert!(builder.rewind(GenericEvidenceBuilder::default().checkpoint()).is_err());
            original.layout = builder.add_layout(generic.layout(original.layout).unwrap().clone()).unwrap();
            for (instruction, origin) in [(original.instruction, original.origin), (original.receiver_instruction, original.receiver_origin)] {
                builder.register_instruction_origin(instruction, OperationSourceOrigin::Expression(origin), original.owner).unwrap();
            }
            let checkpoint = builder.checkpoint();
            let retired_source = builder.add_ground_projection_source(original.clone()).unwrap();
            let proof = |source| PreparedGroundProjection { source, layout: original.layout, ..original_proof.clone() };
            let retired = builder.add_ground_projection(proof(retired_source)).unwrap();
            let stale_checkpoint = builder.checkpoint();
            builder.rewind(checkpoint).unwrap();
            let replacement_source = builder.add_ground_projection_source(original.clone()).unwrap();
            let replacement = builder.add_ground_projection(proof(replacement_source)).unwrap();
            assert!(builder.rewind(stale_checkpoint).is_err());
            let owners = program.store.generic_instruction_owners().unwrap();
            let mut store = builder.finish(&program.store.semantic, program.store.functions.len(), &owners).unwrap();
            assert!(store.ground_projection(retired).is_err());
            assert!(store.ground_projection_source(retired_source).is_err());
            assert!(store.ground_projection(original_id).is_err());
            assert_eq!(store.ground_projection_at(original.instruction).unwrap(), Some(replacement));
            assert_eq!(store.ground_projection(replacement).unwrap().source, replacement_source);
            let retained = store.retained_bytes();
            assert!(retained >= size_of::<GroundProjectionSource>() + size_of::<PreparedGroundProjection>() + size_of::<(u32, super::super::super::generic::GroundProjectionId)>());
            store.shrink_to_fit();
            assert!(store.retained_bytes() <= retained);
            assert_eq!(store.ground_projection_at(original.instruction).unwrap(), Some(replacement));
        });
    }

    #[test]
    fn native_comparison_sources_preserve_preparation_after_frontend_drop() {
        source_fixture(include_str!("../../../../../tests/xsh/comparison-chain.xsh"),
            PreparedLanguageOperation::Ordering { op: BinaryOp::Lt, left: Atom::Str, right: Atom::Str });
    }
}
