use super::schemes::{FullInstantiation, SchemeTypeId};
use super::{IrFunctionId, IrRange};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullValueMaterialization {
    pub(super) instruction: u32,
    pub(super) source: SchemeTypeId,
    pub(super) destination: SchemeTypeId,
    pub(super) root: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(super) enum FullMaterializationKind {
    Keep,
    Callable,
    Record,
    List,
    MapValues,
    Optional,
    Result,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullMaterializationNode {
    pub(super) kind: FullMaterializationKind,
    pub(super) reserved: [u8; 3],
    pub(super) children: IrRange,
    pub(super) leaf: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullMaterializationEdge {
    pub(super) name: u32,
    pub(super) node: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C, u8)]
pub(super) enum FullValueLeafSource {
    DirectTarget(IrFunctionId),
    Relationship(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullMaterializationLeaf {
    pub(super) source: FullValueLeafSource,
    pub(super) instance: FullInstantiation,
}
