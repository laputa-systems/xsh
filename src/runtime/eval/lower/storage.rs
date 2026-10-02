use crate::runtime::eval::LoweredType;
use crate::sema::inference::{Atom, CallableKind, InferenceError, ScopedRoot, SolvedGraph, TypeId, TypeNode, VariableKind};
use rustc_hash::FxHashMap;

#[derive(Clone, Copy, Debug)]
pub(super) struct CheckedStorageView {
    pub(super) root: ScopedRoot,
    pub(super) kind: LoweredType,
    #[cfg(test)]
    visited_nodes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum StorageViewError {
    MissingSourceType,
    Graph(InferenceError),
    UnsupportedStorage(TypeId),
}

/// The physical kind is a storage view of the original scoped semantic root.
/// Shared representations never replace UInt, nominal errors, Optional, or a
/// callable's signature with a less precise semantic type.
pub(super) fn checked_storage_view(
    graph: &SolvedGraph,
    root: Option<ScopedRoot>,
) -> Result<CheckedStorageView, StorageViewError> {
    let root = root.ok_or(StorageViewError::MissingSourceType)?;
    graph.validate_scoped(root).map_err(StorageViewError::Graph)?;
    let mut projection = StorageProjection { graph, kinds: FxHashMap::default(), visits: 0, work: 0 };
    let kind = projection.kind(root.ty, 0)?;
    Ok(CheckedStorageView {
        root,
        kind,
        #[cfg(test)]
        visited_nodes: projection.visits,
    })
}

struct StorageProjection<'a> {
    graph: &'a SolvedGraph,
    kinds: FxHashMap<TypeId, LoweredType>,
    visits: usize,
    work: u64,
}

impl StorageProjection<'_> {
    fn kind(&mut self, input: TypeId, depth: usize) -> Result<LoweredType, StorageViewError> {
        let limits = self.graph.limits();
        if depth > limits.structural_depth { return Err(StorageViewError::Graph(InferenceError::Limit("storage depth"))); }
        if self.work >= limits.work_units { return Err(StorageViewError::Graph(InferenceError::Limit("storage work"))); }
        self.work += 1;
        let ty = self.graph.resolved(input).map_err(StorageViewError::Graph)?;
        if let Some(kind) = self.kinds.get(&ty) { return Ok(*kind); }
        if self.visits >= limits.type_row_nodes { return Err(StorageViewError::Graph(InferenceError::Limit("storage nodes"))); }
        self.visits += 1;
        let graph = self.graph;
        let kind = match graph.node(ty).map_err(StorageViewError::Graph)? {
            TypeNode::Atom(atom) => match atom {
                Atom::Any | Atom::Null => LoweredType::Any,
                Atom::ErasedRecord => LoweredType::Record,
                Atom::DynamicModule => LoweredType::Module,
                Atom::Pure => LoweredType::Pure,
                Atom::Proc => LoweredType::Proc,
                Atom::Command => LoweredType::Command,
                Atom::Bool => LoweredType::Bool,
                Atom::Int | Atom::UInt => LoweredType::Int,
                Atom::Float => LoweredType::Float,
                Atom::Duration => LoweredType::Duration,
                Atom::Str => LoweredType::Str,
                Atom::Bytes => LoweredType::Bytes,
                Atom::Digest => LoweredType::Digest,
                Atom::Regex => LoweredType::Regex,
                Atom::Path => LoweredType::Path,
                Atom::Unit => LoweredType::Unit,
                Atom::Status => LoweredType::Status,
                Atom::ProcessHandle => LoweredType::ProcessHandle,
                Atom::NetJob => LoweredType::NetJob,
                Atom::FsRoot => LoweredType::FsRoot,
                Atom::Tag(_) => LoweredType::Tag,
                Atom::Error | Atom::ProcessError | Atom::ErrorFamily(_) | Atom::ErrorVariant { .. } | Atom::ErrorFacet(_) => LoweredType::Error,
                Atom::EnvPathList => return Err(StorageViewError::UnsupportedStorage(ty)),
            },
            TypeNode::Rigid { kind: VariableKind::Type, .. } => LoweredType::Generic,
            TypeNode::Optional(_) => LoweredType::Any,
            TypeNode::List(_) => LoweredType::List,
            TypeNode::Stream(_) => LoweredType::Stream,
            TypeNode::Map(_, _) => LoweredType::Map,
            TypeNode::Result(_, _) => LoweredType::Result,
            TypeNode::Record(_) => LoweredType::Record,
            TypeNode::Module(_) => LoweredType::Module,
            TypeNode::Arrow(arrow) => match arrow.kind {
                CallableKind::Pure | CallableKind::Stream => LoweredType::Pure,
                CallableKind::Proc => LoweredType::Proc,
            },
            TypeNode::NativeCallable(callable) => self.kind(callable.signature, depth + 1)?,
            TypeNode::CallableChoice(arms) => {
                // Storage must agree across every arm; the retained root still
                // owns each distinct signature and its effects.
                let mut common = None;
                for arm in arms {
                    let kind = self.kind(*arm, depth + 1)?;
                    if !matches!(kind, LoweredType::Pure | LoweredType::Proc) || common.is_some_and(|previous| previous != kind) {
                        return Err(StorageViewError::UnsupportedStorage(ty));
                    }
                    common = Some(kind);
                }
                common.ok_or(StorageViewError::UnsupportedStorage(ty))?
            }
            TypeNode::Meta(_) => return Err(StorageViewError::Graph(InferenceError::Unresolved(ty))),
            TypeNode::Poison | TypeNode::NonCompletion => return Err(StorageViewError::Graph(InferenceError::Recovery(ty))),
            TypeNode::Rigid { kind: VariableKind::Row, .. } | TypeNode::Row(_) | TypeNode::FiniteDomain(_) => return Err(StorageViewError::UnsupportedStorage(ty)),
        };
        self.kinds.insert(ty, kind);
        Ok(kind)
    }
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
