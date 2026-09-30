use crate::source::Span;
use crate::symbol::Name;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

mod unify;
mod schemes;
mod requirements;
mod views;
mod finalized;
pub use finalized::{RetainedStorage, ScopedRoot, SolvedGraph};
#[cfg(test)]
mod tests;
#[cfg(test)]
mod reference;

static NEXT_HANDLE: AtomicU32 = AtomicU32::new(1);
static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);

macro_rules! handle {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name { index: u32, generation: u32 }
        impl $name {
            pub const fn index(self) -> usize { self.index as usize }
            pub const fn generation(self) -> u32 { self.generation }
        }
    };
}
handle!(TypeId);
handle!(RowId);
handle!(MetaId);
handle!(SchemeId);
handle!(ReasonId);
handle!(RequirementId);
handle!(EffectId);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GraphOwner(u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum VariableKind { Type, Row }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Atom {
    Any, ErasedRecord, DynamicModule, Pure, Proc, Command,
    Null, Bool, Int, UInt, Float, Duration, Str, Bytes, Digest, Regex, Path,
    Unit, Status, EnvPathList, Error, ProcessError, ProcessHandle, NetJob, FsRoot,
    Tag(Name), ErrorFamily(Name), ErrorVariant { family: Name, variant: Name }, ErrorFacet(Name),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CallableKind { Pure, Proc, Stream }

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct EffectSet(pub u8);

impl EffectSet {
    pub const EMPTY: Self = Self(0);
    pub const FS: Self = Self(1);
    pub const NET: Self = Self(2);
    pub const PROCESS: Self = Self(4);
    pub const ENV: Self = Self(8);
    pub const TIME: Self = Self(16);
    pub const ERROR: Self = Self(32);
    pub const IO: Self = Self(64);
    pub fn contains(self, other: Self) -> bool {
        let covered = if self.0 & Self::IO.0 != 0 { self.0 | Self::FS.0 | Self::NET.0 | Self::PROCESS.0 | Self::ENV.0 } else { self.0 };
        covered & other.0 == other.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EffectSummary { Closed(EffectSet), Variable(EffectId), Rigid { scope: SchemeId, index: u32 }, Unknown }

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Parameter { pub label: Name, pub ty: TypeId, pub defaulted: bool, pub rest: bool }

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Arrow { pub kind: CallableKind, pub params: Vec<Parameter>, pub result: TypeId, pub effects: EffectSummary }

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ModuleField { pub label: Name, pub ty: TypeId, pub optional: bool }

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum TypeNode {
    Atom(Atom), Meta(MetaId),
    Rigid { scope: SchemeId, index: u32, kind: VariableKind },
    Optional(TypeId), List(TypeId), Stream(TypeId), Map(TypeId, TypeId), Result(TypeId, TypeId),
    Arrow(Arrow), Module(Vec<ModuleField>), Record(RowId), Row(RowId), Poison, NonCompletion,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RowField { pub label: Name, pub ty: TypeId }

/// Labels describe a semantic row. Physical record order belongs to a separate
/// prepared layout and must never be reconstructed from this normalization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Row { pub fields: Vec<RowField>, pub tail: Option<TypeId> }

#[derive(Clone, Debug)]
pub struct Reason { pub span: Span, pub parent: Option<ReasonId> }

/// Constraint endpoints keep their original identities so diagnostics can follow
/// substitutions without losing the source that connected them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstraintRelation {
    Equality { left: TypeId, right: TypeId },
    Assignable { expected: TypeId, actual: TypeId },
    Projection { record: TypeId, label: Name, result: TypeId },
    Capture { ty: TypeId, level: u32 },
    Add { left: TypeId, right: TypeId, result: TypeId },
    EffectInclusion { actual: EffectSummary, expected: EffectSummary },
}

#[derive(Clone, Copy, Debug)]
pub struct ConstraintOrigin { pub relation: ConstraintRelation, pub reason: ReasonId }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Generalization { Allowed, Monomorphic }

#[derive(Clone, Debug)]
pub struct Quantifier { pub kind: VariableKind, pub lacks: Vec<Name> }

#[derive(Clone, Copy, Debug)]
pub struct EffectQuantifier { pub lower: EffectSet, pub upper: Option<EffectSet> }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RequirementTemplate { Add { left: TypeId, right: TypeId, result: TypeId } }

#[derive(Clone, Debug)]
pub struct Scheme {
    pub body: TypeId, pub quantifiers: Vec<Quantifier>, pub binders: Vec<TypeId>,
    pub effect_quantifiers: Vec<EffectQuantifier>, pub effect_inclusions: Vec<(EffectSummary, EffectSummary)>,
    pub requirements: Vec<RequirementTemplate>, pub scope_level: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct VariableView<'a> { pub id: MetaId, pub kind: VariableKind, pub level: u32, pub origin: Span, pub lacks: &'a [Name] }

#[derive(Clone, Debug)]
pub struct Instantiation { pub ty: TypeId, pub requirements: Vec<RequirementId>, pub substitutions: Vec<TypeId>, pub effect_substitutions: Vec<EffectId> }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SealedOperation { AddInt, AddFloat, AddStr, AddDuration, AddList }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OperationEvidence {
    pub requirement: RequirementId, pub operation: SealedOperation,
    pub left: TypeId, pub right: TypeId, pub result: TypeId,
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub type_row_nodes: usize, pub variables: usize, pub constraints: usize,
    pub reason_edges: usize, pub work_units: u64, pub structural_depth: usize, pub row_labels: usize,
}

impl Default for Limits {
    fn default() -> Self { Self { type_row_nodes: 2_000_000, variables: 2_000_000, constraints: 8_000_000, reason_edges: 8_000_000, work_units: 100_000_000, structural_depth: 512, row_labels: 20_000 } }
}

#[derive(Clone, Debug, Default)]
pub struct Counters {
    pub attempted_nodes: u64, pub attempted_variables: u64, pub attempted_constraints: u64,
    pub attempted_reasons: u64, pub work_units: u64, pub wakeups: u64, pub probes: u64,
    pub rollbacks: u64, pub unifications: u64, pub instantiations: u64,
    pub occurs_steps: u64, pub level_lowerings: u64, pub row_steps: u64, pub queue_pushes: u64,
    pub attempted_schemes: u64, pub rigid_variables: u64, pub attempted_reason_edges: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InferenceError {
    ForeignHandle, Limit(&'static str), KindMismatch, TypeMismatch { left: TypeId, right: TypeId },
    Occurs { variable: MetaId, within: TypeId }, DuplicateLabel(Name), MissingField(Name), Lacks(Name),
    ScopeEscape, Unresolved(TypeId), Recovery(TypeId), UnsupportedOperation(RequirementId),
    DisconnectedRequirement(RequirementId), EffectViolation, InvalidScheme, Boundary(&'static str),
}

#[derive(Clone, Debug)]
struct Slot<T> { generation: u32, value: T }

#[derive(Clone, Debug)]
struct Meta {
    kind: VariableKind, level: u32, origin: Span, parent: MetaId, rank: u8,
    binding: Option<TypeId>, ty: TypeId, lacks: Vec<Name>, watchers: Vec<RequirementId>,
}

#[derive(Clone, Debug)]
struct Requirement { template: RequirementTemplate, reason: ReasonId, evidence: Option<OperationEvidence>, queued: bool }

#[derive(Clone, Debug)]
struct EffectVariable { level: u32, bits: EffectSet, upper: Option<EffectSet>, outgoing: Vec<EffectId>, incoming: Vec<EffectId>, binding: Option<EffectSummary> }

#[derive(Clone, Debug)]
enum Trail {
    Meta(MetaId, Meta), Requirement(RequirementId, Requirement), Effect(EffectId, EffectVariable),
    ArrowEffects(TypeId, EffectSummary),
    WatcherInsert(MetaId, usize),
    QueuePush, QueuePop(RequirementId),
}

#[derive(Clone, Copy, Debug)]
struct Checkpoint { nodes: usize, rows: usize, metas: usize, schemes: usize, reasons: usize, requirements: usize, effects: usize, origins: usize, trail: usize }

/// Mutable identities and constraints belong to exactly one checked bundle.
/// Immutable constructors can be shared; metavariables are never interning keys.
#[derive(Debug)]
pub struct InferenceContext {
    owner: GraphOwner, limits: Limits, counters: Counters,
    nodes: Vec<Slot<TypeNode>>, rows: Vec<Slot<Row>>, metas: Vec<Slot<Meta>>,
    schemes: Vec<Slot<Scheme>>, reasons: Vec<Slot<Reason>>, requirements: Vec<Slot<Requirement>>,
    effects: Vec<Slot<EffectVariable>>, atoms: FxHashMap<Atom, TypeId>,
    queue: VecDeque<RequirementId>, trail: Vec<Trail>, transactions: usize,
    origins: Vec<ConstraintOrigin>,
}

impl Default for InferenceContext { fn default() -> Self { Self::new(Limits::default()) } }

impl InferenceContext {
    pub fn new(limits: Limits) -> Self {
        Self { owner: GraphOwner(NEXT_OWNER.fetch_add(1, Ordering::Relaxed)), limits, counters: Counters::default(), nodes: Vec::new(), rows: Vec::new(), metas: Vec::new(), schemes: Vec::new(), reasons: Vec::new(), requirements: Vec::new(), effects: Vec::new(), atoms: FxHashMap::default(), queue: VecDeque::new(), trail: Vec::new(), transactions: 0, origins: Vec::new() }
    }
    pub fn owner(&self) -> GraphOwner { self.owner }
    pub fn counters(&self) -> &Counters { &self.counters }
    pub fn constraint_origins(&self) -> &[ConstraintOrigin] { &self.origins }
    fn generation() -> Result<u32, InferenceError> {
        NEXT_HANDLE.try_update(Ordering::Relaxed, Ordering::Relaxed, |value| value.checked_add(1)).map_err(|_| InferenceError::Limit("handle identities"))
    }
    fn work(&mut self) -> Result<(), InferenceError> {
        self.work_many(1)
    }
    fn work_many(&mut self, units: usize) -> Result<(), InferenceError> {
        self.counters.work_units = self.counters.work_units.saturating_add(units as u64);
        if self.counters.work_units > self.limits.work_units { return Err(InferenceError::Limit("solver work")); }
        Ok(())
    }
    fn reason_edge(&mut self) -> Result<(), InferenceError> {
        self.counters.attempted_reason_edges = self.counters.attempted_reason_edges.saturating_add(1);
        self.reason_limit()
    }
    fn reason_limit(&self) -> Result<(), InferenceError> {
        if self.counters.attempted_reasons.saturating_add(self.counters.attempted_reason_edges) > self.limits.reason_edges as u64 { return Err(InferenceError::Limit("reason edges")); }
        Ok(())
    }
    fn contribute(&mut self, relation: ConstraintRelation, reason: ReasonId) -> Result<(), InferenceError> {
        self.reason_data(reason)?;
        self.reason_edge()?;
        self.origins.push(ConstraintOrigin { relation, reason });
        Ok(())
    }
    fn constraint(&mut self) -> Result<(), InferenceError> {
        self.counters.attempted_constraints += 1;
        if self.counters.attempted_constraints > self.limits.constraints as u64 { return Err(InferenceError::Limit("constraints")); }
        Ok(())
    }
    fn allocate(&mut self, node: TypeNode) -> Result<TypeId, InferenceError> {
        self.counters.attempted_nodes += 1;
        if self.counters.attempted_nodes > self.limits.type_row_nodes as u64 { return Err(InferenceError::Limit("type and row nodes")); }
        let id = TypeId { index: self.nodes.len().try_into().map_err(|_| InferenceError::Limit("type identities"))?, generation: Self::generation()? };
        self.nodes.push(Slot { generation: id.generation, value: node });
        Ok(id)
    }
    pub fn atom(&mut self, atom: Atom) -> Result<TypeId, InferenceError> {
        if let Some(id) = self.atoms.get(&atom) { return Ok(*id); }
        let id = self.allocate(TypeNode::Atom(atom))?;
        self.atoms.insert(atom, id);
        Ok(id)
    }
    pub fn poison(&mut self) -> Result<TypeId, InferenceError> { self.allocate(TypeNode::Poison) }
    pub fn non_completion(&mut self) -> Result<TypeId, InferenceError> { self.allocate(TypeNode::NonCompletion) }
    pub fn node(&self, id: TypeId) -> Result<&TypeNode, InferenceError> { slot(&self.nodes, id.index(), id.generation) }
    fn clone_node(&mut self, id: TypeId) -> Result<TypeNode, InferenceError> {
        let count = match self.node(id)? { TypeNode::Arrow(arrow) => arrow.params.len(), TypeNode::Module(fields) => fields.len(), _ => 0 };
        self.work_many(count)?;
        Ok(self.node(id)?.clone())
    }
    fn clone_row(&mut self, id: RowId) -> Result<Row, InferenceError> {
        self.work_many(self.row_data(id)?.fields.len())?;
        Ok(self.row_data(id)?.clone())
    }
    pub fn row_data(&self, id: RowId) -> Result<&Row, InferenceError> { slot(&self.rows, id.index(), id.generation) }
    pub fn scheme(&self, id: SchemeId) -> Result<&Scheme, InferenceError> { slot(&self.schemes, id.index(), id.generation) }
    pub fn reason_data(&self, id: ReasonId) -> Result<&Reason, InferenceError> { slot(&self.reasons, id.index(), id.generation) }
    fn meta(&self, id: MetaId) -> Result<&Meta, InferenceError> { slot(&self.metas, id.index(), id.generation) }
    fn requirement(&self, id: RequirementId) -> Result<&Requirement, InferenceError> { slot(&self.requirements, id.index(), id.generation) }
    pub fn reason(&mut self, span: Span, parent: Option<ReasonId>) -> Result<ReasonId, InferenceError> {
        if let Some(parent) = parent { self.reason_data(parent)?; }
        self.counters.attempted_reasons += 1;
        self.reason_limit()?;
        if parent.is_some() { self.reason_edge()?; }
        let id = ReasonId { index: self.reasons.len() as u32, generation: Self::generation()? };
        self.reasons.push(Slot { generation: id.generation, value: Reason { span, parent } });
        Ok(id)
    }
    pub fn fresh(&mut self, level: u32, origin: Span) -> Result<TypeId, InferenceError> { self.fresh_kind(VariableKind::Type, level, origin) }
    pub fn fresh_row(&mut self, level: u32, origin: Span) -> Result<TypeId, InferenceError> { self.fresh_kind(VariableKind::Row, level, origin) }
    fn fresh_kind(&mut self, kind: VariableKind, level: u32, origin: Span) -> Result<TypeId, InferenceError> {
        self.counters.attempted_variables += 1;
        if self.counters.attempted_variables > self.limits.variables as u64 { return Err(InferenceError::Limit("variables")); }
        let id = MetaId { index: self.metas.len() as u32, generation: Self::generation()? };
        let ty = self.allocate(TypeNode::Meta(id))?;
        self.metas.push(Slot { generation: id.generation, value: Meta { kind, level, origin, parent: id, rank: 0, binding: None, ty, lacks: Vec::new(), watchers: Vec::new() } });
        Ok(ty)
    }
    fn type_kind(&self, ty: TypeId) -> Result<VariableKind, InferenceError> {
        Ok(match self.node(self.resolved(ty)?)? { TypeNode::Meta(id) => self.meta(*id)?.kind, TypeNode::Rigid { kind, .. } => *kind, TypeNode::Row(_) => VariableKind::Row, _ => VariableKind::Type })
    }
    fn value_type(&self, ty: TypeId) -> Result<(), InferenceError> { if self.type_kind(ty)? == VariableKind::Type { Ok(()) } else { Err(InferenceError::KindMismatch) } }
    pub fn list(&mut self, item: TypeId) -> Result<TypeId, InferenceError> { self.value_type(item)?; self.allocate(TypeNode::List(item)) }
    pub fn optional(&mut self, item: TypeId) -> Result<TypeId, InferenceError> { self.value_type(item)?; self.allocate(TypeNode::Optional(item)) }
    pub fn stream(&mut self, item: TypeId) -> Result<TypeId, InferenceError> { self.value_type(item)?; self.allocate(TypeNode::Stream(item)) }
    pub fn map(&mut self, key: TypeId, value: TypeId) -> Result<TypeId, InferenceError> { self.value_type(key)?; self.value_type(value)?; self.allocate(TypeNode::Map(key, value)) }
    pub fn result(&mut self, success: TypeId, error: TypeId) -> Result<TypeId, InferenceError> { self.value_type(success)?; self.value_type(error)?; self.allocate(TypeNode::Result(success, error)) }
    pub fn arrow(&mut self, arrow: Arrow) -> Result<TypeId, InferenceError> {
        self.value_type(arrow.result)?;
        let mut labels = FxHashSet::default();
        for (index, parameter) in arrow.params.iter().enumerate() {
            self.value_type(parameter.ty)?;
            if !labels.insert(parameter.label) { return Err(InferenceError::DuplicateLabel(parameter.label)); }
            if parameter.rest && (index + 1 != arrow.params.len() || parameter.defaulted) { return Err(InferenceError::InvalidScheme); }
        }
        self.resolved_effect_summary(arrow.effects)?;
        self.allocate(TypeNode::Arrow(arrow))
    }
    pub fn set_arrow_effects(&mut self, ty: TypeId, effects: EffectSummary) -> Result<(), InferenceError> {
        let resolved_effects = self.resolved_effect_summary(effects)?;
        let ty = self.resolved(ty)?;
        let TypeNode::Arrow(arrow) = self.node(ty)? else { return Err(InferenceError::KindMismatch) };
        let previous = arrow.effects;
        let resolved_previous = self.resolved_effect_summary(previous)?;
        if matches!(resolved_previous, EffectSummary::Rigid { .. }) && resolved_previous != resolved_effects { return Err(InferenceError::EffectViolation); }
        if self.transactions > 0 { self.trail.push(Trail::ArrowEffects(ty, previous)); }
        if let TypeNode::Arrow(arrow) = &mut self.nodes[ty.index()].value { arrow.effects = effects; }
        Ok(())
    }
    pub fn module(&mut self, mut exports: Vec<ModuleField>) -> Result<TypeId, InferenceError> {
        let mut comparisons = 0usize;
        exports.sort_by(|left, right| { comparisons += 1; left.label.cmp(&right.label) });
        self.work_many(comparisons + exports.len())?;
        for pair in exports.windows(2) { if pair[0].label == pair[1].label { return Err(InferenceError::DuplicateLabel(pair[0].label)); } }
        for field in &exports { self.value_type(field.ty)?; }
        self.allocate(TypeNode::Module(exports))
    }
    pub fn record(&mut self, row: RowId) -> Result<TypeId, InferenceError> { self.row_data(row)?; self.allocate(TypeNode::Record(row)) }
    fn row_type(&mut self, row: RowId) -> Result<TypeId, InferenceError> { self.row_data(row)?; self.allocate(TypeNode::Row(row)) }
    fn trail_meta(&mut self, id: MetaId) -> Result<(), InferenceError> {
        if self.transactions > 0 {
            let meta = self.meta(id)?; let units = meta.lacks.len() + meta.watchers.len(); self.work_many(units)?;
            self.trail.push(Trail::Meta(id, self.meta(id)?.clone()));
        }
        Ok(())
    }
    fn trail_requirement(&mut self, id: RequirementId) -> Result<(), InferenceError> {
        if self.transactions > 0 { self.trail.push(Trail::Requirement(id, self.requirement(id)?.clone())); }
        Ok(())
    }
    fn checkpoint(&self) -> Checkpoint { Checkpoint { nodes: self.nodes.len(), rows: self.rows.len(), metas: self.metas.len(), schemes: self.schemes.len(), reasons: self.reasons.len(), requirements: self.requirements.len(), effects: self.effects.len(), origins: self.origins.len(), trail: self.trail.len() } }
    fn rewind(&mut self, checkpoint: Checkpoint) {
        while self.trail.len() > checkpoint.trail {
            match self.trail.pop().unwrap() {
                Trail::Meta(id, value) => self.metas[id.index()].value = value,
                Trail::Requirement(id, value) => self.requirements[id.index()].value = value,
                Trail::Effect(id, value) => self.effects[id.index()].value = value,
                Trail::ArrowEffects(id, effects) => { if let TypeNode::Arrow(arrow) = &mut self.nodes[id.index()].value { arrow.effects = effects; } }
                Trail::WatcherInsert(id, index) => {
                    let watchers = &mut self.metas[id.index()].value.watchers;
                    self.counters.work_units = self.counters.work_units.saturating_add((watchers.len() - index) as u64);
                    watchers.remove(index);
                }
                Trail::QueuePush => { self.queue.pop_back(); }
                Trail::QueuePop(id) => self.queue.push_front(id),
            }
        }
        self.nodes.truncate(checkpoint.nodes); self.rows.truncate(checkpoint.rows); self.metas.truncate(checkpoint.metas);
        self.schemes.truncate(checkpoint.schemes); self.reasons.truncate(checkpoint.reasons); self.requirements.truncate(checkpoint.requirements); self.effects.truncate(checkpoint.effects); self.origins.truncate(checkpoint.origins);
        let nodes = &self.nodes;
        self.atoms.retain(|_, id| slot(nodes, id.index(), id.generation).is_ok());
    }
    /// Failed probes restore substitutions, levels, watchers, pending work and
    /// allocation visibility. Attempted-work counters deliberately do not rewind.
    pub fn probe<T>(&mut self, operation: impl FnOnce(&mut Self) -> Result<T, InferenceError>) -> Result<T, InferenceError> {
        self.counters.probes += 1;
        let checkpoint = self.checkpoint(); self.transactions += 1;
        let result = operation(self);
        if result.is_err() { self.counters.rollbacks += 1; self.rewind(checkpoint); }
        self.transactions -= 1;
        if self.transactions == 0 { self.trail.clear(); }
        result
    }
}

fn slot<T>(slots: &[Slot<T>], index: usize, generation: u32) -> Result<&T, InferenceError> {
    slots.get(index).filter(|slot| slot.generation == generation).map(|slot| &slot.value).ok_or(InferenceError::ForeignHandle)
}
