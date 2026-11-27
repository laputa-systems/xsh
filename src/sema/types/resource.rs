use super::Type;

impl Type {
    /// Whether a value of this type can contain a host resource handle.
    /// Unknown or erased shapes stay conservative because their runtime
    /// contents cannot be inspected from the static type.
    pub fn may_hold_resource(&self) -> bool {
        let mut pending = ResourceSearch::default();
        let mut current = self;
        loop {
            match current {
                Self::ProcessHandle | Self::NetJob | Self::FsRoot | Self::FsLock | Self::SshSession | Self::SqliteDb | Self::Stream(_) => {
                    return true;
                }
                Self::List(item) | Self::Set(item) | Self::Optional(item) => {
                    current = item;
                    continue;
                }
                Self::Validated(validated) => {
                    current = validated.base();
                    continue;
                }
                Self::Map(key, value) | Self::Result(key, value) => {
                    pending.push(value);
                    current = key;
                    continue;
                }
                Self::Record(_) | Self::Tag(_) => return true,
                Self::Union(members) => {
                    for member in members { pending.push(member); }
                }
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
                | Self::SchemeCallable(_)
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
                | Self::Function(_)
                | Self::Command
                | Self::Unit => {}
            }
            match pending.pop() {
                Some(next) => current = next,
                None => return false,
            }
        }
    }
}

#[derive(Default)]
struct ResourceSearch<'a> {
    inline: [Option<&'a Type>; 8],
    count: usize,
    overflow: Vec<&'a Type>,
}

impl<'a> ResourceSearch<'a> {
    fn push(&mut self, ty: &'a Type) {
        if self.count < self.inline.len() {
            self.inline[self.count] = Some(ty);
            self.count += 1;
        } else {
            self.overflow.push(ty);
        }
    }

    fn pop(&mut self) -> Option<&'a Type> {
        if let Some(ty) = self.overflow.pop() { return Some(ty); }
        if self.count == 0 { return None; }
        self.count -= 1;
        self.inline[self.count].take()
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
        let record = Type::Record(BTreeMap::from([(owner.intern("child"), Type::NetJob)]).into());
        assert!(record.may_hold_resource());
        let validated = Type::nominal(owner.intern("Wrapper"), record).expect("record is nominal");
        assert!(validated.may_hold_resource());
        for ty in [Type::Any, Type::ErasedRecord, Type::Error, Type::Unknown,
            Type::Record(BTreeMap::new().into()), Type::Tag(owner.intern("Hidden"))] {
            assert!(ty.may_hold_resource());
        }
        for ty in [Type::Int, Type::Str] {
            assert!(!ty.may_hold_resource());
        }
    }

    #[test]
    fn may_hold_resource_searches_every_pending_shape_past_the_inline_stack() {
        let mut members = vec![Type::Int; 20];
        assert!(!Type::Union(members.clone()).may_hold_resource());
        for index in [0, 7, 8, 19] {
            members[index] = Type::FsLock;
            assert!(Type::Union(members.clone()).may_hold_resource());
            members[index] = Type::Int;
        }
        let nested_values = (0..20).fold(Type::Int, |ty, _| {
            Type::Map(Box::new(Type::Str), Box::new(Type::Set(Box::new(ty))))
        });
        assert!(!nested_values.may_hold_resource());
        let nested_keys = (0..20).fold(Type::Int, |ty, _| Type::Map(Box::new(ty), Box::new(Type::Bool)));
        assert!(!nested_keys.may_hold_resource());
        let resource_at_depth = (0..20).fold(Type::SshSession, |ty, _| Type::Map(Box::new(Type::Bool), Box::new(ty)));
        assert!(resource_at_depth.may_hold_resource());
        let resource_in_first_key = (0..20).fold(Type::NetJob, |ty, _| Type::Map(Box::new(ty), Box::new(Type::Bool)));
        assert!(resource_in_first_key.may_hold_resource());
    }
}
