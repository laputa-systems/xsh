use super::Type;
use std::collections::HashSet;

impl Type {
    /// Whether a value of this type can contain a host resource handle.
    /// Unknown or erased shapes stay conservative because their runtime
    /// contents cannot be inspected from the static type.
    pub(crate) fn may_hold_resource(&self) -> bool {
        let mut pending = vec![self];
        let mut visited = HashSet::new();

        while let Some(ty) = pending.pop() {
            let identity = std::ptr::from_ref(ty);
            if !visited.insert(identity) {
                continue;
            }

            match ty {
                Self::ProcessHandle | Self::NetJob | Self::FsRoot | Self::FsLock | Self::Stream(_) => {
                    return true;
                }
                Self::List(item) | Self::Set(item) | Self::Optional(item) => pending.push(item),
                Self::Map(key, value) | Self::Result(key, value) => {
                    pending.push(key);
                    pending.push(value);
                }
                Self::Record(fields) => pending.extend(fields.values()),
                Self::Union(members) => pending.extend(members),
                Self::Validated(validated) => pending.push(validated.base()),
                Self::Any
                | Self::Unknown
                | Self::Invalid
                | Self::ErasedRecord
                | Self::Module(_)
                | Self::DynamicModule
                | Self::Error
                | Self::ErrorFamily(_)
                | Self::ErrorVariant { .. }
                | Self::ErrorFacet(_)
                | Self::ProcessError
                | Self::Callable(_)
                | Self::BuiltinParameter(_)
                | Self::Inference(_) => return true,
                Self::Null
                | Self::Bool
                | Self::Int
                | Self::UInt
                | Self::Float
                | Self::Duration
                | Self::Str
                | Self::Bytes
                | Self::Digest
                | Self::Regex
                | Self::Path
                | Self::Status
                | Self::EnvPathList
                | Self::Pure
                | Self::Proc
                | Self::Command
                | Self::Unit
                | Self::Tag(_) => {}
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::SymbolOwner;
    use std::collections::BTreeMap;

    #[test]
    fn may_hold_resource_walks_nested_shapes_without_a_depth_limit() {
        let nested = (0..512).fold(Type::ProcessHandle, |ty, _| {
            Type::Optional(Box::new(ty))
        });
        assert!(nested.may_hold_resource());

        let nested_without_resource = (0..512).fold(Type::Int, |ty, _| {
            Type::Optional(Box::new(ty))
        });
        assert!(!nested_without_resource.may_hold_resource());
    }

    #[test]
    fn may_hold_resource_covers_exact_handles_containers_and_erased_payloads() {
        for ty in [Type::ProcessHandle, Type::NetJob, Type::FsRoot, Type::FsLock] {
            assert!(ty.may_hold_resource());
        }
        assert!(Type::Stream(Box::new(Type::Int)).may_hold_resource());
        assert!(Type::Map(Box::new(Type::ProcessHandle), Box::new(Type::Int)).may_hold_resource());
        assert!(Type::Result(Box::new(Type::Int), Box::new(Type::FsLock)).may_hold_resource());
        let owner = SymbolOwner::new();
        let record = Type::Record(BTreeMap::from([(owner.intern("child"), Type::NetJob)]));
        assert!(record.may_hold_resource());
        let validated = Type::nominal(owner.intern("Wrapper"), record).expect("record is nominal");
        assert!(validated.may_hold_resource());
        for ty in [Type::Any, Type::ErasedRecord, Type::Error, Type::Unknown] {
            assert!(ty.may_hold_resource());
        }
        for ty in [Type::Int, Type::Str, Type::Record(BTreeMap::new())] {
            assert!(!ty.may_hold_resource());
        }
    }
}
