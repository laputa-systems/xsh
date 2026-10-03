use super::*;
use crate::modules::RuntimeOp;
use crate::sema::types::Type;

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedNativeRecordGet {
    pub receiver: u32,
    pub key: u32,
    pub key_source: u32,
    pub field: Name,
    pub registry_result: TypeRef,
    pub producer_result: TypeRef,
    pub field_type: TypeRef,
}

impl NativeCallSource {
    pub(in crate::runtime::eval) fn verify_record_get_refinement(&self, pools: &SemanticPools) -> Result<bool, IrVerifyError> {
        let Some(refinement) = &self.result_refinement else { return Ok(false); };
        let contract = &self.expected;
        if contract.registry_owner != crate::sema::registry_graph::RegistryOwner::Method(crate::modules::signature::MethodReceiver::Record)
            || !matches!(contract.authority, PreparedOperationAuthority::Registry {
                operation: RuntimeOp::RecordGet, binding: crate::modules::signature::ImplBinding::Native,
                semantic_rule: crate::modules::signature::SemanticRule::ConstantKeyProjection, .. })
            || refinement.registry_result != contract.result || contract.arguments.len() != 1
            || contract.arguments[0].instruction != refinement.key
            || pools.signature_param_count(contract.signature)? != 2 {
            return Err(failure("record get refinement changes its original selected method contract"));
        }
        let receiver = contract.receiver.as_ref().ok_or_else(|| failure("record get refinement loses its original receiver"))?;
        let (TypeRef::Ground(receiver_type), TypeRef::Ground(registry_result), TypeRef::Ground(producer_result), TypeRef::Ground(field_type)) =
            (receiver.ty, refinement.registry_result, refinement.producer_result, refinement.field_type) else {
            return Err(failure("record get refinement requires closed source types"));
        };
        let Type::Record(fields) = pools.to_type(receiver_type)? else { return Err(failure("record get refinement receiver has no visible record row")); };
        let field_type = pools.to_type(field_type)?;
        if receiver.instruction != refinement.receiver || receiver.method_name != Name::intern("get")
            || receiver.ty != receiver.source_type || fields.get(&refinement.field) != Some(&field_type)
            || pools.to_type(pools.signature_param(contract.signature, 0)?.1)? != Type::ErasedRecord
            || pools.to_type(pools.signature_param(contract.signature, 1)?.1)? != Type::Str
            || contract.argument_relations.first() != Some(&crate::sema::inference::ArgumentRelation::DeclaredErasure) {
            return Err(failure("record get refinement changes its original field or receiver erasure"));
        }
        let Type::Result(success, error) = pools.to_type(registry_result)? else { return Err(failure("record get selected result loses its Result carrier")); };
        if *success != Type::Any || pools.to_type(producer_result)? != Type::Result(Box::new(field_type), error)
            || pools.signature_return_type(contract.signature)? != registry_result {
            return Err(failure("record get refinement changes its original producer relationship"));
        }
        Ok(true)
    }
}
