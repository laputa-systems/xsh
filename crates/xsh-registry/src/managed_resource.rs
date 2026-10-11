//! The ownership and consuming operations of built-in host resources.

use crate::RuntimeOp;
use crate::types::Type;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceKind {
    Activity,
    Capability,
}

/// Some process operations consume only the handles selected by their result.
/// Waiting on a handle and draining a stream are language operations rather
/// than registered module calls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceConsumption {
    Wait,
    Operation(RuntimeOp),
    SelectedOperation(RuntimeOp),
    Drain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceRepeat {
    Error,
    CancelIsIdempotent,
    EmptyStream,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceScopeRelease {
    CancelAndReap,
    CancelAndDrain,
    CancelStream,
    CloseRoot,
    Unlock,
}

pub struct ResourceRule {
    pub kind: ResourceKind,
    pub consuming: &'static [ResourceConsumption],
    pub repeat: ResourceRepeat,
    pub scope_release: ResourceScopeRelease,
    /// A `with` scope exposes fallible capability release. Scope ownership
    /// applies to every row independently of support for that scope form.
    pub with_release: Option<RuntimeOp>,
}

macro_rules! resource_table {
    ($($name:ident => ($ty:expr, $rule:expr)),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub enum ManagedResource { $($name),+ }

        impl ManagedResource {
            pub const ALL: [Self; resource_table!(@count $($name),+)] = [$(Self::$name),+];

            pub fn resource_type(self) -> Type {
                match self { $(Self::$name => $ty),+ }
            }

            pub const fn type_name(self) -> &'static str {
                match self { $(Self::$name => stringify!($name)),+ }
            }

            pub const fn rule(self) -> ResourceRule {
                match self { $(Self::$name => $rule),+ }
            }
        }
    };
    (@count $($name:ident),+) => { <[()]>::len(&[$(resource_table!(@one $name)),+]) };
    (@one $name:ident) => { () };
}

resource_table! {
    ProcessHandle => (Type::ProcessHandle, ResourceRule {
        kind: ResourceKind::Activity,
        consuming: &[
            ResourceConsumption::Wait,
            ResourceConsumption::Operation(RuntimeOp::ProcessHandleCancel),
            ResourceConsumption::SelectedOperation(RuntimeOp::ProcessWaitAny),
            ResourceConsumption::SelectedOperation(RuntimeOp::ProcessWaitReady),
            ResourceConsumption::SelectedOperation(RuntimeOp::ProcessWaitTimeout),
        ],
        repeat: ResourceRepeat::CancelIsIdempotent,
        scope_release: ResourceScopeRelease::CancelAndReap,
        with_release: None,
    }),
    NetJob => (Type::NetJob, ResourceRule {
        kind: ResourceKind::Activity,
        consuming: &[
            ResourceConsumption::Operation(RuntimeOp::NetJobWait),
            ResourceConsumption::Operation(RuntimeOp::NetJobCancel),
        ],
        repeat: ResourceRepeat::Error,
        scope_release: ResourceScopeRelease::CancelAndDrain,
        with_release: None,
    }),
    Stream => (Type::Stream(Box::new(Type::Any)), ResourceRule {
        kind: ResourceKind::Activity,
        consuming: &[ResourceConsumption::Drain],
        repeat: ResourceRepeat::EmptyStream,
        scope_release: ResourceScopeRelease::CancelStream,
        with_release: None,
    }),
    FsRoot => (Type::FsRoot, ResourceRule {
        kind: ResourceKind::Capability,
        consuming: &[ResourceConsumption::Operation(RuntimeOp::FsCloseRoot)],
        repeat: ResourceRepeat::Error,
        scope_release: ResourceScopeRelease::CloseRoot,
        with_release: Some(RuntimeOp::FsCloseRootIfOpen),
    }),
    FsLock => (Type::FsLock, ResourceRule {
        kind: ResourceKind::Capability,
        consuming: &[ResourceConsumption::Operation(RuntimeOp::FsUnlock)],
        repeat: ResourceRepeat::Error,
        scope_release: ResourceScopeRelease::Unlock,
        with_release: Some(RuntimeOp::FsUnlockIfHeld),
    }),
}

impl ManagedResource {
    pub fn for_type(ty: &Type) -> Option<Self> {
        if matches!(ty, Type::Stream(_)) {
            return Some(Self::Stream);
        }
        Self::ALL.into_iter().find(|kind| kind.resource_type() == *ty)
    }

    pub fn for_with_type(ty: &Type) -> Option<Self> {
        Self::for_type(ty).filter(|kind| kind.rule().with_release.is_some())
    }

    pub const fn release_op(self) -> Option<RuntimeOp> {
        self.rule().with_release
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_resource_kind_has_one_rule_and_its_own_type() {
        for kind in ManagedResource::ALL {
            assert_eq!(ManagedResource::for_type(&kind.resource_type()), Some(kind));
            let rule = kind.rule();
            assert!(!rule.consuming.is_empty());
            assert_eq!(rule.with_release.is_some(), rule.kind == ResourceKind::Capability);
        }
        assert_eq!(ManagedResource::for_type(&Type::Stream(Box::new(Type::Int))), Some(ManagedResource::Stream));
        for ty in [Type::Path, Type::Unknown, Type::Any, Type::Record(Default::default())] {
            assert_eq!(ManagedResource::for_type(&ty), None);
        }
    }

    #[test]
    fn with_accepts_only_the_two_fallible_capabilities() {
        let accepted: Vec<_> = ManagedResource::ALL.into_iter()
            .filter(|kind| kind.rule().with_release.is_some()).collect();
        assert_eq!(accepted, [ManagedResource::FsRoot, ManagedResource::FsLock]);
        assert_eq!(ManagedResource::for_with_type(&Type::FsLock), Some(ManagedResource::FsLock));
        assert_eq!(ManagedResource::for_with_type(&Type::ProcessHandle), None);
        assert_eq!(ManagedResource::FsRoot.release_op(), Some(RuntimeOp::FsCloseRootIfOpen));
        assert_eq!(ManagedResource::FsLock.release_op(), Some(RuntimeOp::FsUnlockIfHeld));
    }
}
