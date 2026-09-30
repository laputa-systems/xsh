use std::collections::BTreeMap;
use crate::source::SourceId;
use crate::symbol::Name;
use crate::syntax::arena::{BlockId, ExprId, FunctionDefId, StmtId};
use crate::sema::inference::{CallableKind, GraphOwner, InferenceContext, InferenceError, RequirementId, SchemeId, TypeId, ScopedRoot, SolvedGraph};

/// Arena identities retain the source and namespace that resolved the declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DeclarationIdentity {
    pub source: SourceId,
    pub namespace: Option<Name>,
    pub declaration: FunctionDefId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ExpressionIdentity {
    pub source: SourceId,
    pub namespace: Option<Name>,
    pub expression: ExprId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct StatementIdentity {
    pub source: SourceId,
    pub namespace: Option<Name>,
    pub statement: StmtId,
}

/// This choice belongs to the declaration; substituting a Result or Bool payload
/// never changes its statement consumption or adds/removes a result wrapper.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReturnElaboration {
    Value,
    ImplicitResult,
    UnitConsuming,
}

#[derive(Clone, Debug)]
pub struct SolvedCallable {
    pub scheme: SchemeId,
    pub signature: TypeId,
    pub body: BlockId,
    pub kind: CallableKind,
    pub return_elaboration: ReturnElaboration,
}

/// Each source argument names its semantic parameter slot. Omitted defaults
/// remain ordered by parameter index independently of supplied source order.
#[derive(Clone, Debug)]
pub struct CallBinding {
    pub supplied_slots: Vec<usize>,
    pub default_slots: Vec<usize>,
    pub rest_slot: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct SolvedCall {
    pub signature: TypeId,
    pub declaration: Option<DeclarationIdentity>,
    pub caller: Option<DeclarationIdentity>,
    pub requirements: Vec<RequirementId>,
    pub substitutions: Vec<TypeId>,
    pub actual_arguments: Vec<TypeId>,
    pub binding: CallBinding,
}

#[derive(Clone, Debug)]
pub struct SolvedProjection {
    pub receiver: TypeId,
    pub field: Name,
    pub result: TypeId,
}

/// The graph and all of its consumer handles share one retained owner. Source
/// spans are projected diagnostics and never select a semantic fact.
#[derive(Debug)]
pub struct SolvedTypes<Graph = SolvedGraph> {
    pub owner: GraphOwner,
    pub graph: Graph,
    pub declarations: BTreeMap<DeclarationIdentity, SolvedCallable>,
    pub expressions: BTreeMap<ExpressionIdentity, TypeId>,
    pub calls: BTreeMap<ExpressionIdentity, SolvedCall>,
    pub projections: BTreeMap<ExpressionIdentity, SolvedProjection>,
    pub additions: BTreeMap<ExpressionIdentity, RequirementId>,
    pub statements: BTreeMap<StatementIdentity, super::StatementPosition>,
    pub expression_owners: BTreeMap<ExpressionIdentity, DeclarationIdentity>,
    /// Written Result boundaries wrap only the known payload completion paths.
    /// The stored type is the synthetic wrapper's result; the original
    /// expression fact still describes the instruction that produces its payload.
    pub result_wrappings: BTreeMap<ExpressionIdentity, TypeId>,
    pub result_statement_wrappings: BTreeMap<StatementIdentity, TypeId>,
}

impl Default for SolvedTypes<InferenceContext> {
    fn default() -> Self {
        let graph = InferenceContext::default();
        let owner = graph.owner();
        Self::with_graph(owner, graph)
    }
}

impl Default for SolvedTypes {
    fn default() -> Self {
        let graph = InferenceContext::default().freeze_scoped(&[]).expect("empty graph is solved");
        Self::with_graph(graph.owner(), graph)
    }
}

impl<Graph> SolvedTypes<Graph> {
    fn with_graph(owner: GraphOwner, graph: Graph) -> Self {
        Self {
            owner, graph,
            declarations: BTreeMap::new(), expressions: BTreeMap::new(),
            calls: BTreeMap::new(), projections: BTreeMap::new(),
            additions: BTreeMap::new(), statements: BTreeMap::new(),
            expression_owners: BTreeMap::new(),
            result_wrappings: BTreeMap::new(), result_statement_wrappings: BTreeMap::new(),
        }
    }

    fn scoped_roots(&self) -> Result<Vec<ScopedRoot>, InferenceError> {
        let mut roots = Vec::new();
        for declaration in self.declarations.values() {
            roots.push(ScopedRoot { ty: declaration.signature, scope: Some(declaration.scheme) });
        }
        for (identity, &ty) in &self.expressions {
            let scope = self.expression_owners.get(identity).map(|owner| self.declarations.get(owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
            roots.push(ScopedRoot { ty, scope });
        }
        for call in self.calls.values() {
            let scope = call.caller.map(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
            roots.push(ScopedRoot { ty: call.signature, scope });
            roots.extend(call.actual_arguments.iter().chain(&call.substitutions).map(|&ty| ScopedRoot { ty, scope }));
        }
        for (identity, projection) in &self.projections {
            let scope = self.expression_owners.get(identity).map(|owner| self.declarations.get(owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
            for ty in [projection.receiver, projection.result] { roots.push(ScopedRoot { ty, scope }); }
        }
        roots.extend(self.result_wrappings.values().chain(self.result_statement_wrappings.values()).map(|&ty| ScopedRoot { ty, scope: None }));
        Ok(roots)
    }
}

impl SolvedTypes {
    pub fn validate(&self) -> Result<(), InferenceError> {
        for root in self.scoped_roots()? { self.graph.validate_scoped(root)?; }
        Ok(())
    }
}

impl SolvedTypes<InferenceContext> {
    pub(super) fn freeze(self) -> Result<SolvedTypes, InferenceError> {
        let roots = self.scoped_roots()?;
        let graph = self.graph.freeze_scoped(&roots)?;
        Ok(SolvedTypes {
            owner: self.owner, graph,
            declarations: self.declarations, expressions: self.expressions,
            calls: self.calls, projections: self.projections,
            additions: self.additions, statements: self.statements,
            expression_owners: self.expression_owners,
            result_wrappings: self.result_wrappings,
            result_statement_wrappings: self.result_statement_wrappings,
        })
    }
}
