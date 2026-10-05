//! The built-in resource types a `with NAME = VALUE { ... }` scope manages.

use crate::RuntimeOp;
use crate::records::{fs_lock_type, fs_root_type};
use crate::types::Type;

/// A built-in type whose values hold a host resource that is released by one
/// fallible operation. The checker accepts exactly these as the value of a
/// managed `with` binding, and lowering reads the release operation from the
/// kind the checker recorded.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ManagedResource {
    /// An `FsRoot`, from `fs.open_root`, `fs.tempdir`, or a rooted open.
    FsRoot,
    /// The lock record `fs.lock` returns.
    FsLock,
}

impl ManagedResource {
    pub const ALL: [Self; 2] = [Self::FsRoot, Self::FsLock];

    /// The type of the values this kind manages.
    pub fn resource_type(self) -> Type {
        match self {
            Self::FsRoot => fs_root_type(),
            Self::FsLock => fs_lock_type(),
        }
    }

    /// The kind that manages a value of `ty`, if the type is a resource.
    pub fn for_type(ty: &Type) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.resource_type() == *ty)
    }

    /// The name diagnostics and documentation use for the type.
    pub fn type_name(self) -> &'static str {
        match self {
            Self::FsRoot => "FsRoot",
            Self::FsLock => "FsLock",
        }
    }

    /// The operation the scope releases the value with. Unlike the method or
    /// function a program calls, it succeeds on a value that was already
    /// released, so a body may release its resource early.
    pub fn release_op(self) -> RuntimeOp {
        match self {
            Self::FsRoot => RuntimeOp::FsCloseRootIfOpen,
            Self::FsLock => RuntimeOp::FsUnlockIfHeld,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_is_found_from_its_own_type_only() {
        assert_eq!(
            ManagedResource::for_type(&Type::FsRoot),
            Some(ManagedResource::FsRoot)
        );
        assert_eq!(
            ManagedResource::for_type(&fs_lock_type()),
            Some(ManagedResource::FsLock)
        );
        for ty in [
            Type::Path,
            Type::Unknown,
            Type::Any,
            Type::Record(Default::default()),
        ] {
            assert_eq!(ManagedResource::for_type(&ty), None, "{ty:?}");
        }
        for kind in ManagedResource::ALL {
            assert_ne!(kind.release_op(), RuntimeOp::FsCloseRoot);
            assert_ne!(kind.release_op(), RuntimeOp::FsUnlock);
        }
    }
}
