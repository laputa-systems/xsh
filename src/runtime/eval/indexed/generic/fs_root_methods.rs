use super::*;
use crate::sema::types::Type;

pub(in crate::runtime::eval) fn is_fs_root_method_owner(owner: crate::sema::registry_graph::RegistryOwner) -> bool {
    owner == crate::sema::registry_graph::RegistryOwner::Method(crate::modules::signature::MethodReceiver::FsRoot)
}

impl GroundNativeCallContract {
    /// The hidden receiver remains an opaque capability at both the source and
    /// formal boundaries. A record layout or an erased input cannot replace it.
    pub(in crate::runtime::eval) fn verify_fs_root_method(&self, pools: &SemanticPools) -> Result<bool, IrVerifyError> {
        if !is_fs_root_method_owner(self.registry_owner) { return Ok(false); }
        let receiver = self.receiver.as_ref().ok_or_else(|| failure("filesystem root method has no capability receiver"))?;
        let TypeRef::Ground(actual) = receiver.ty else { return Err(failure("filesystem root capability receiver is not closed")); };
        let TypeRef::Ground(source) = receiver.source_type else { return Err(failure("filesystem root original receiver source is not closed")); };
        let propagation = receiver.source_wrappers.first().is_some_and(|wrapper| wrapper.kind == ValueInitializerWrapperKind::FsRootReceiverTry);
        let source_type = pools.to_type(source)?;
        if propagation {
            if !matches!(source_type, Type::Result(inner, error) if *inner == Type::FsRoot && *error == Type::Error) {
                return Err(failure("filesystem root Result receiver changes its original source carrier"));
            }
        } else if matches!(&source_type, Type::Optional(inner) if inner.as_ref() == &Type::FsRoot) {
            if receiver.saved.as_ref().is_none_or(|saved| saved.guarded_read.is_none()) {
                return Err(failure("filesystem root optional receiver lacks its original present guard"));
            }
        } else if source_type != Type::FsRoot { return Err(failure("filesystem root method changes its original opaque source")); }
        let (label, formal, _) = pools.signature_param(self.signature, 0)?;
        if label != Name::intern("<receiver>") || pools.to_type(formal)? != Type::FsRoot
            || pools.to_type(actual)? != Type::FsRoot || self.argument_sources.first() != Some(&Some(receiver.instruction))
            || receiver.saved.is_none() || self.binding.supplied_slots.contains(&0) || self.binding.default_slots.contains(&0) {
            return Err(failure("filesystem root method changes its opaque receiver or hidden slot"));
        }
        Ok(true)
    }
}
