use super::*;
use super::super::super::generic::PreparedNativeRecordGet;
use crate::sema::check::{ExpressionIdentity, SolvedTypes};
use crate::sema::inference::TypeId;

impl FullVerifier {
    pub(in crate::runtime::eval::indexed::full) fn verify_constant_field_key(store: &FullStore, instruction: u32, field: Name) -> Result<(), IrVerifyError> {
        let words = store.payload(store.data.get(instruction as usize).ok_or_else(|| IrVerifyError::new("constant record key source is invalid"))?.range())?;
        if words.len() != 1 { return Err(IrVerifyError::new("constant record key has another payload")); }
        let matches = match store.tags.get(instruction as usize) {
            Some(FullTag::ExprStr) => store.string(words[0])? == field.as_str().as_str(),
            Some(FullTag::ExprPreparedConstant) => matches!(store.prepared_constants.get(words[0] as usize).map(|value| &value.0), Some(LoweredValue::Str(value)) if value.as_ref() == field.as_str().as_str()),
            _ => false,
        };
        if !matches { return Err(IrVerifyError::new("constant record key no longer selects its original visible field")); }
        Ok(())
    }
}

impl FullBuilder {
    pub(in crate::runtime::eval::indexed::full) fn prepare_record_get_refinement(&mut self,
        solved: &SolvedTypes, expression: ExpressionIdentity, receiver: &Option<PreparedNativeReceiver>,
        arguments: &[PreparedInvocationArgument], registry_result: TypeId, producer_result: TypeId,
        owner: InstructionOwner,
    ) -> Result<Option<PreparedNativeRecordGet>, IrBuildError> {
        let Some(fact) = solved.record_get_projection(expression).map_err(|_| native_problem("record_get_original_refinement_changed"))? else { return Ok(None); };
        let receiver = receiver.as_ref().ok_or_else(|| native_problem("record_get_original_receiver_missing"))?;
        let [key] = arguments else { return Err(native_problem("record_get_original_key_protocol")); };
        let graph = &solved.graph;
        if fact.receiver != receiver.origin || fact.caller != solved.expression_owners.get(&expression).copied()
            || solved.expressions.get(&fact.receiver) != Some(&fact.receiver_type)
            || solved.expressions.get(&expression) != Some(&fact.producer_result)
            || graph_ground_type(graph, registry_result).map_err(|_| native_problem("record_get_registry_result_scope"))? != graph_ground_type(graph, fact.registry_result).map_err(|_| native_problem("record_get_original_registry_result_scope"))?
            || graph_ground_type(graph, producer_result).map_err(|_| native_problem("record_get_producer_result_scope"))? != graph_ground_type(graph, fact.producer_result).map_err(|_| native_problem("record_get_original_producer_result_scope"))?
            || key.original.value != crate::sema::arguments::ArgumentValueSource::Expression(fact.key.expression) {
            return Err(native_problem("record_get_original_projection_relationship_changed"));
        }
        let source_type = graph_ground_type(graph, fact.receiver_type).map_err(|_| native_problem("record_get_original_receiver_scope"))?;
        let TypeRef::Ground(receiver_type) = receiver.source_type else { return Err(native_problem("record_get_original_receiver_requires_closed_type")); };
        if self.store.semantic.to_type(receiver_type).map_err(|_| native_problem("record_get_original_receiver_type"))? != source_type {
            return Err(native_problem("record_get_original_receiver_changed"));
        }
        for ty in [fact.receiver_type, fact.field_type, fact.registry_result, fact.producer_result] {
            let scope = solved.expression_scope(expression, fact.caller).map_err(|_| native_problem("record_get_original_scope"))?;
            graph.validate_scoped(crate::sema::inference::ScopedRoot { ty, scope }).map_err(|_| native_problem("record_get_original_type_certificate"))?;
        }
        let (material, _) = self.argument_initializer_lineage(key.instruction, owner)?;
        let key_source = self.prepared_saved_argument_bindings.get(&material).map_or(material, |saved| saved.initializer_source_instruction);
        FullVerifier::verify_constant_field_key(&self.store, key_source, fact.field).map_err(|cause| IrBuildError::verification("record_get_original_key_literal_changed", cause))?;
        let mut intern = |ty| -> Result<TypeRef, IrBuildError> {
            let ty = graph_ground_type(graph, ty).map_err(|_| native_problem("record_get_original_closed_type"))?;
            Ok(TypeRef::Ground(self.intern_generic_ground_type(&ty)?))
        };
        Ok(Some(PreparedNativeRecordGet { receiver: receiver.instruction, key: key.instruction, key_source,
            field: fact.field, registry_result: intern(fact.registry_result)?,
            producer_result: intern(fact.producer_result)?, field_type: intern(fact.field_type)?,
        }))
    }
}

impl FullVerifier {
    pub(in crate::runtime::eval::indexed::full) fn verify_record_get_key(store: &FullStore,
        generic: &GenericEvidenceStore, source: &NativeCallSource,
    ) -> Result<(), IrVerifyError> {
        if !source.verify_record_get_refinement(&store.semantic)? { return Ok(()); }
        let refinement = source.result_refinement.as_ref().unwrap();
        let [lineage] = source.argument_lineages.as_ref() else { return Err(IrVerifyError::new("record get loses its original key lineage")); };
        let material = generic.original_argument_binding(lineage.source_instruction)
            .map_or(lineage.source_instruction, |saved| saved.initializer_source_instruction);
        if material != refinement.key_source { return Err(IrVerifyError::new("record get changes its original key allocation")); }
        Self::verify_constant_field_key(store, material, refinement.field)?;
        if store.tags.get(material as usize) == Some(&FullTag::ExprPreparedConstant) {
            Self::verify_constant_operand(store, generic, material, source.owner, &Type::Str)?;
        }
        Ok(())
    }
}
