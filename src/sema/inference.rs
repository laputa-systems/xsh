use crate::source::Span;
use crate::sema::constraints::TypeVariableId;
use crate::symbol::Name;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

mod unify;
mod schemes;
mod components;
mod connectivity;
mod operations;
mod invocations;
mod native;
mod eligibility;
mod requirements;
mod views;
mod finalized;
mod applications;
mod error_joins;
pub use applications::{ApplicationPathComponent, ApplicationSource, ApplicationDeclaration, ApplicationCertificate, ScopedApplicationRoot};
pub use finalized::{InstanceCertificate, RetainedStorage, ScopedEffectRoot, ScopedInstanceRoot, ScopedRequirementRoot, ScopedRoot, SolvedGraph};
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
handle!(CandidateId);
handle!(OperationFamilyId);
handle!(OperationCallId);
handle!(InvocationCallId);
handle!(ErrorJoinId);
handle!(NativeContractId);
handle!(NativeFamilyContractId);

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
    Arrow(Arrow), CallableChoice(Vec<TypeId>), FiniteDomain(Vec<DomainAlternative>), NativeCallable(NativeCallable), Module(Vec<ModuleField>), Record(RowId), Row(RowId), Poison, NonCompletion,
}

/// The callable signature belongs to one value instance. Native authorities
/// keep their original contracts, while a join shares parameters and result.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct NativeCallable { pub signature: TypeId, pub alternatives: Vec<CallableAuthority> }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// User origins identify the checked source reference through replacement;
/// signatures carry its current instance and are the semantic type roots.
pub enum CallableAuthority { User { signature: TypeId, origin: TypeId }, Native { authority: NativeAuthority } }

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NativeAuthority { Single(NativeContractId), Family(NativeFamilyContractId) }

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DomainAlternative { pub ty: TypeId, pub relation: ArgumentRelation }

#[derive(Clone, Debug)]
pub struct NativeFamilyContract { pub origin: NativeFamilyContractId, pub family: OperationFamilyId, pub signature: TypeId, pub members: Vec<NativeContractId> }

#[derive(Clone, Debug)]
pub struct NativeContract { pub origin: NativeContractId, pub candidate: CandidateId, pub scheme: SchemeId, pub family: OperationFamilyId, pub instance: Instantiation }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeInvocationAlternative { pub authority: NativeAuthority, pub operation: RequirementId }

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
    MaskedEffectInclusion { actual: EffectSummary, expected: EffectSummary, excluded: EffectSet },
    CallableInvocation { call: InvocationCallId },
    Equality { left: TypeId, right: TypeId },
    Assignable { expected: TypeId, actual: TypeId },
    Projection { record: TypeId, label: Name, result: TypeId },
    ModuleProjection { module: TypeId, label: Name, result: TypeId, optional: bool },
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
pub struct EffectQuantifier { pub lower: EffectSet, pub upper: Option<EffectSet>, pub derived: bool }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RequirementTemplate {
    ErrorJoin { join: ErrorJoinId },
    Add { left: TypeId, right: TypeId, result: TypeId },
    Eligibility { predicate: Eligibility, ty: TypeId },
    EqualityCompatible { left: TypeId, right: TypeId },
    Operation { family: OperationFamilyId, call: OperationCallId },
    CallableInvocation { call: InvocationCallId },
    EffectInclusion { actual: EffectSummary, expected: EffectSummary, excluded: EffectSet },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CallableDomain { Pure, AnyCallable, Exact(CallableKind) }

impl CallableDomain {
    pub fn admits(self, kind: CallableKind) -> bool { match self { Self::Pure => kind == CallableKind::Pure, Self::AnyCallable => true, Self::Exact(expected) => kind == expected } }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InvocationArgumentKind { Positional, Named(Name), PositionalSplice }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct InvocationArgument { pub kind: InvocationArgumentKind, pub ty: TypeId }

/// Invocation arguments keep their supplied shape. Parameter labels, defaults,
/// and rest slots come from the callable that eventually proves the obligation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvocationCall { pub callable: TypeId, pub arguments: Vec<InvocationArgument>, pub result: TypeId, pub effects: EffectSummary, pub domain: CallableDomain }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorJoin { pub inputs: Vec<TypeId>, pub result: TypeId, pub bound: Option<TypeId> }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvocationEvidence { pub callable: TypeId, pub native_alternatives: Vec<NativeInvocationAlternative>, pub plan: InvocationPlan, pub result: TypeId, pub effects: EffectSummary }

/// Each runtime alternative binds the original arguments independently. A
/// default in one branch cannot supply a missing parameter in another branch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvocationPlan {
    Unique { signature: TypeId, binding: InvocationBinding, timing: InvocationDefaultTiming },
    All { branches: Vec<InvocationBranchEvidence> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvocationBranchEvidence { pub authority: CallableAuthority, pub signature: TypeId, pub binding: InvocationBinding, pub timing: InvocationDefaultTiming, pub effects: EffectSummary }

impl InvocationEvidence {
    pub fn unique_plan(&self) -> Option<(TypeId, &InvocationBinding, InvocationDefaultTiming)> {
        match &self.plan { InvocationPlan::Unique { signature, binding, timing } => Some((*signature, binding, *timing)), InvocationPlan::All { .. } => None }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvocationBinding { pub supplied_slots: Vec<usize>, pub default_slots: Vec<usize>, pub rest_slot: Option<usize>, pub dynamic: Option<DynamicInvocationBinding> }

/// Unknown-length segments preserve source order and every reachable destination.
/// Runtime guards decide cardinality and conditional named-slot collisions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicInvocationBinding { pub segments: Vec<InvocationArgumentSegment>, pub conditional_default_slots: Vec<usize>, pub required_slots: Vec<usize>, pub runtime_arity_guard: bool, pub runtime_duplicate_guard: bool }

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvocationArgumentSegment { StaticSlot { argument: usize, slot: usize }, DynamicRange { argument: usize, fixed_slots: Vec<usize>, rest_slot: Option<usize> } }

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvocationPlanError { Graph(InferenceError), Binding(InvocationProblem) }

impl From<InferenceError> for InvocationPlanError { fn from(error: InferenceError) -> Self { Self::Graph(error) } }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvocationDefaultTiming { AtCall, AtPull }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvocationProblem { NotCallable, CallableKind, UnknownLabel(Name), DuplicateArgument(Name), MissingArgument(Name), TooManyArguments, InvalidSplice, InvalidRest }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Eligibility { MapKey, JsonCompatible, NonUnit, Sortable, SortableKey, ArgvItem, ArgvExpansion, CountKey, Record, YieldItem, CommandTarget, CommandArgv, Error, Display }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateTemplate { pub failure_projection: Option<OperationFailureProjection>, pub identity: Name, pub public_label: Name, pub effect_roles: Vec<(EffectRole, EffectRoleReference)>, pub output_effect_roles: Vec<(ProducerRole, u32)>, pub scheme: SchemeId, pub has_receiver: bool, pub actual_eligibility: Vec<(usize, Eligibility)>, pub argument_relations: Vec<ArgumentRelation> }

/// A declared failure channel belongs to the candidate signature. The source
/// supplies its fixed lexical bound independently of argument positions.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OperationFailureProjection { ReceiverResultError, ArgumentResultError { argument: usize } }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ArgumentRelation { Assignable, Exact, DeclaredErasure, EqualityCompatible, InvocationProtocol, CommandTarget { domain: CommandTextDomain }, CommandArgv { element: CommandTextDomain } }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CommandTextDomain { Str, Path }

/// A nested producer location within the same supplied source value. Projection
/// preserves the operand identity instead of inventing another source operand.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EffectProjection { ResultSuccess }

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EffectRole {
    Creation, Callback, Pull { source: u32 }, Close { source: u32 },
    PullProjection { source: u32, projection: EffectProjection },
    CloseProjection { source: u32, projection: EffectProjection },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EffectRoleReference { Binder(u32), Fixed(EffectSet) }

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProducerRole { Pull, Close }

#[derive(Clone, Debug, Default)]
pub struct GeneralizationRoots { pub captured_types: Vec<TypeId>, pub effects: Vec<EffectSummary> }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OperationBinding { Slots, Invocation(InvocationCallId) }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OperationEffectMode { AvailableBudget, ComputedCreation }

#[derive(Clone, Debug)]
pub struct OperationCall { pub binding: OperationBinding, pub effect_mode: OperationEffectMode, pub mono_authority: Option<NativeAuthority>, pub declared_error_bound: Option<TypeId>, pub receiver: Option<TypeId>, pub arguments: Vec<Option<TypeId>>, pub result: TypeId, pub effects: EffectSummary, pub effect_bindings: Vec<(EffectRole, EffectSummary)>, pub output_effect_bindings: Vec<(ProducerRole, EffectSummary)> }

#[derive(Clone, Debug)]
pub struct CandidateEvidence {
    pub callback_invocations: Vec<CallableProtocolEvidence>,
    pub binding: Option<InvocationBinding>, pub actual_arguments: Vec<Option<TypeId>>,
    pub candidate: CandidateId, pub failure_assignability: Option<usize>, pub signature: TypeId, pub substitutions: Vec<TypeId>, pub effect_substitutions: Vec<EffectId>,
    pub result: TypeId, pub effects: EffectSummary, pub effect_roots: Vec<EffectSummary>, pub dependencies: Vec<RequirementId>,
}

/// A callback protocol proves invocation of the original value, not equality
/// with the synthetic positional signature used by its enclosing operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CallableProtocolEvidence { pub slot: usize, pub invocation: RequirementId }

#[derive(Clone, Debug)]
pub struct ComponentMember { pub root: TypeId, pub requirements: Vec<RequirementId>, pub policy: Generalization }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchemeRole { Value, ComponentScope }

#[derive(Clone, Debug)]
pub struct Scheme {
    pub scope_owner: SchemeId, pub role: SchemeRole,
    pub effect_roots: Vec<EffectSummary>, pub captures: Vec<TypeId>, pub effect_captures: Vec<EffectSummary>, pub effect_binders: Vec<EffectSummary>,
    pub body: TypeId, pub quantifiers: Vec<Quantifier>, pub binders: Vec<TypeId>,
    pub effect_quantifiers: Vec<EffectQuantifier>, pub effect_inclusions: Vec<(EffectSummary, EffectSummary)>,
    /// Immediate checked-body obligations, parallel to the retained templates.
    /// Separate calls of one generic body keep distinct IDs here.
    pub requirement_origins: Vec<RequirementId>, pub requirements: Vec<RequirementTemplate>, pub scope_level: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct VariableView<'a> { pub id: MetaId, pub kind: VariableKind, pub level: u32, pub origin: Span, pub lacks: &'a [Name] }

#[derive(Default)]
pub(super) struct ReplacementMemo {
    types: FxHashMap<TypeId, TypeId>,
    contracts: FxHashMap<NativeContractId, NativeContractId>,
    families: FxHashMap<NativeFamilyContractId, NativeFamilyContractId>,
    requirements: FxHashMap<RequirementId, RequirementId>,
    invocations: FxHashMap<InvocationCallId, InvocationCallId>,
    error_joins: FxHashMap<ErrorJoinId, ErrorJoinId>,
    sources: FxHashSet<RequirementId>,
}

#[derive(Clone, Debug)]
pub struct Instantiation {
    /// Immediate body obligation and its fresh instance, in template order.
    pub requirement_origins: Vec<(RequirementId, RequirementId)>,
    pub effect_roots: Vec<EffectSummary>, pub ty: TypeId, pub requirements: Vec<RequirementId>, pub substitutions: Vec<TypeId>, pub effect_substitutions: Vec<EffectId>,
}

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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Counters {
    pub attempted_nodes: u64, pub attempted_variables: u64, pub attempted_constraints: u64,
    pub attempted_reasons: u64, pub work_units: u64, pub wakeups: u64, pub probes: u64,
    pub rollbacks: u64, pub unifications: u64, pub instantiations: u64,
    pub occurs_steps: u64, pub level_lowerings: u64, pub row_steps: u64, pub queue_pushes: u64,
    pub attempted_schemes: u64, pub rigid_variables: u64, pub attempted_reason_edges: u64, pub candidate_trials: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InferenceError {
    ForeignHandle, Limit(&'static str), KindMismatch, TypeMismatch { left: TypeId, right: TypeId },
    InvalidInvocation { call: InvocationCallId, problem: InvocationProblem },
    Occurs { variable: MetaId, within: TypeId }, DuplicateLabel(Name), MissingField(Name), Lacks(Name),
    ScopeEscape, Unresolved(TypeId), Recovery(TypeId), UnsupportedOperation(RequirementId),
    DisconnectedRequirement(RequirementId), EffectViolation,
    /// Candidate operand relationships were admissible, but an execution or
    /// producer effect bound failed. Required bits are the known lower bound;
    /// available bits are the finite cap. An opaque or unbounded side is None.
    OperationEffectViolation { requirement: RequirementId, required: Option<EffectSet>, available: Option<EffectSet> },
    /// A candidate admitted its supplied operands, but its declared failure
    /// channel did not fit the source's fixed lexical error boundary. Handles
    /// refer to the original source obligation, never to a rolled-back trial.
    OperationErrorBoundViolation { requirement: RequirementId, bound: TypeId, projection: OperationFailureProjection },
    InvalidScheme, Boundary(&'static str),
}

#[derive(Clone, Debug)]
struct Slot<T> { generation: u32, value: T }

#[derive(Clone, Debug)]
struct Meta {
    kind: VariableKind, level: u32, origin: Span, parent: MetaId, rank: u8,
    binding: Option<TypeId>, ty: TypeId, lacks: Vec<Name>, watchers: Vec<RequirementId>,
}

#[derive(Clone, Debug)]
struct Requirement {
    // The immediate source composes through nested call frames; the original
    // source identifies the checked body that first introduced the obligation.
    source: RequirementId, origin: RequirementId,
    template: RequirementTemplate, reason: ReasonId, evidence: Option<OperationEvidence>, candidate: Option<CandidateEvidence>, invocation: Option<InvocationEvidence>, native_children: Vec<NativeInvocationAlternative>, error_join_assignability: Option<usize>, error_join_output_assignability: Option<usize>, eligibility: bool, queued: bool,
}

#[derive(Clone, Copy, Debug)]
struct LegacyVariable { ty: TypeId, level: u32 }

#[derive(Clone, Debug)]
/// Calculated summaries retain quantified outer sources separately from their
/// finite lower bound, so an inclusion does not alias the entire calculation.
struct EffectVariable { rigid_inputs: Vec<EffectSummary>, derived: bool, watchers: Vec<RequirementId>, level: u32, bits: EffectSet, upper: Option<EffectSet>, outgoing: Vec<EffectId>, incoming: Vec<EffectId>, binding: Option<EffectSummary> }

#[derive(Clone, Debug)]
enum Trail {
    Meta(MetaId, Meta), Requirement(RequirementId, Requirement), Effect(EffectId, EffectVariable),
    ArrowEffects(TypeId, EffectSummary),
    WatcherInsert(MetaId, usize),
    EffectWatcherInsert(EffectId, usize),
    RigidEffectInputInsert(EffectId, usize),
    QueuePush, QueuePop(RequirementId),
    LegacyVariable(TypeVariableId, Option<LegacyVariable>),
    WireTag(Name),
}

#[derive(Clone, Copy, Debug)]
struct Checkpoint { nodes: usize, rows: usize, metas: usize, schemes: usize, reasons: usize, requirements: usize, effects: usize, origins: usize, candidates: usize, families: usize, operation_calls: usize, invocation_calls: usize, error_joins: usize, native_contracts: usize, native_families: usize, trail: usize }

/// Mutable identities and constraints belong to exactly one checked bundle.
/// Immutable constructors can be shared; metavariables are never interning keys.
#[derive(Debug)]
pub struct InferenceContext {
    owner: GraphOwner, limits: Limits, counters: Counters,
    nodes: Vec<Slot<TypeNode>>, rows: Vec<Slot<Row>>, metas: Vec<Slot<Meta>>,
    schemes: Vec<Slot<Scheme>>, reasons: Vec<Slot<Reason>>, requirements: Vec<Slot<Requirement>>,
    effects: Vec<Slot<EffectVariable>>, atoms: FxHashMap<Atom, TypeId>,
    queue: VecDeque<RequirementId>, trail: Vec<Trail>, transactions: usize,
    candidates: Vec<Slot<CandidateTemplate>>, families: Vec<Slot<Vec<CandidateId>>>, operation_calls: Vec<Slot<OperationCall>>, wire_tags: FxHashSet<Name>,
    invocation_calls: Vec<Slot<InvocationCall>>, error_joins: Vec<Slot<ErrorJoin>>, native_contracts: Vec<Slot<NativeContract>>, native_families: Vec<Slot<NativeFamilyContract>>,
    origins: Vec<ConstraintOrigin>, legacy_variables: FxHashMap<TypeVariableId, LegacyVariable>,
}

impl Default for InferenceContext { fn default() -> Self { Self::new(Limits::default()) } }

impl InferenceContext {
    pub fn new(limits: Limits) -> Self {
        Self { owner: GraphOwner(NEXT_OWNER.fetch_add(1, Ordering::Relaxed)), limits, counters: Counters::default(), nodes: Vec::new(), rows: Vec::new(), metas: Vec::new(), schemes: Vec::new(), reasons: Vec::new(), requirements: Vec::new(), effects: Vec::new(), atoms: FxHashMap::default(), queue: VecDeque::new(), trail: Vec::new(), transactions: 0, candidates: Vec::new(), families: Vec::new(), operation_calls: Vec::new(), invocation_calls: Vec::new(), error_joins: Vec::new(), native_contracts: Vec::new(), native_families: Vec::new(), wire_tags: FxHashSet::default(), origins: Vec::new(), legacy_variables: FxHashMap::default() }
    }
    pub fn owner(&self) -> GraphOwner { self.owner }
    pub fn counters(&self) -> &Counters { &self.counters }
    pub fn limits(&self) -> Limits { self.limits }
    pub(crate) fn charge_source_fact_work(&mut self, units: u64) -> Result<(), InferenceError> {
        self.counters.work_units = self.counters.work_units.saturating_add(units);
        if self.counters.work_units > self.limits.work_units { return Err(InferenceError::Limit("solver work")); } Ok(())
    }
    pub(crate) fn charge_source_fact_nodes(&mut self, nodes: u64) -> Result<(), InferenceError> {
        self.counters.attempted_nodes = self.counters.attempted_nodes.saturating_add(nodes);
        self.charge_source_fact_work(nodes)?;
        if self.counters.attempted_nodes > self.limits.type_row_nodes as u64 { return Err(InferenceError::Limit("type and row nodes")); } Ok(())
    }
    pub(crate) fn charge_source_fact_edges(&mut self, edges: u64) -> Result<(), InferenceError> {
        self.counters.attempted_constraints = self.counters.attempted_constraints.saturating_add(edges);
        self.charge_source_fact_work(edges)?;
        if self.counters.attempted_constraints > self.limits.constraints as u64 { return Err(InferenceError::Limit("constraints")); } Ok(())
    }
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
        let count = match self.node(id)? { TypeNode::Arrow(arrow) => arrow.params.len(), TypeNode::Module(fields) => fields.len(), TypeNode::NativeCallable(callable) => callable.alternatives.len(), TypeNode::FiniteDomain(alternatives) => alternatives.len(), TypeNode::CallableChoice(signatures) => signatures.len(), _ => 0 };
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
    pub fn optional(&mut self, item: TypeId) -> Result<TypeId, InferenceError> {
        self.work()?;self.value_type(item)?;let resolved=self.resolved(item)?;
        if matches!(self.node(resolved)?,TypeNode::Optional(_)) {Ok(resolved)}else{self.allocate(TypeNode::Optional(item))}
    }
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
    pub fn module_field(&self, module: TypeId, label: Name) -> Result<&ModuleField, InferenceError> {
        let module = self.resolved(module)?;
        let TypeNode::Module(exports) = self.node(module)? else { return Err(InferenceError::KindMismatch) };
        let index = exports.binary_search_by_key(&label, |field| field.label).map_err(|_| InferenceError::MissingField(label))?;
        Ok(&exports[index])
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
        if self.transactions > 0 {
            if let Some(evidence) = &self.requirement(id)?.candidate { self.work_many(evidence.substitutions.len() + evidence.effect_substitutions.len() + evidence.effect_roots.len() + evidence.dependencies.len())?; }
            if let Some(evidence) = &self.requirement(id)?.invocation { self.work_many(finalized::invocation_plan_work(&evidence.plan) + evidence.native_alternatives.len())?; }
            self.trail.push(Trail::Requirement(id, self.requirement(id)?.clone()));
        }
        Ok(())
    }
    fn checkpoint(&self) -> Checkpoint { Checkpoint { nodes: self.nodes.len(), rows: self.rows.len(), metas: self.metas.len(), schemes: self.schemes.len(), reasons: self.reasons.len(), requirements: self.requirements.len(), effects: self.effects.len(), origins: self.origins.len(), candidates: self.candidates.len(), families: self.families.len(), operation_calls: self.operation_calls.len(), invocation_calls: self.invocation_calls.len(), error_joins: self.error_joins.len(), native_contracts: self.native_contracts.len(), native_families: self.native_families.len(), trail: self.trail.len() } }
    fn rewind(&mut self, checkpoint: Checkpoint) {
        while self.trail.len() > checkpoint.trail {
            match self.trail.pop().unwrap() {
                Trail::Meta(id, value) => self.metas[id.index()].value = value,
                Trail::Requirement(id, value) => self.requirements[id.index()].value = value,
                Trail::Effect(id, value) => self.effects[id.index()].value = value,
                Trail::ArrowEffects(id, effects) => { if let TypeNode::Arrow(arrow) = &mut self.nodes[id.index()].value { arrow.effects = effects; } }
                Trail::RigidEffectInputInsert(id, index) => { self.effects[id.index()].value.rigid_inputs.remove(index); }
                Trail::EffectWatcherInsert(id, index) => { self.effects[id.index()].value.watchers.remove(index); }
                Trail::WatcherInsert(id, index) => {
                    let watchers = &mut self.metas[id.index()].value.watchers;
                    self.counters.work_units = self.counters.work_units.saturating_add((watchers.len() - index) as u64);
                    watchers.remove(index);
                }
                Trail::WireTag(name) => { self.wire_tags.remove(&name); }
                Trail::LegacyVariable(id, previous) => {
                    if let Some(previous) = previous { self.legacy_variables.insert(id, previous); } else { self.legacy_variables.remove(&id); }
                }
                Trail::QueuePush => { self.queue.pop_back(); }
                Trail::QueuePop(id) => self.queue.push_front(id),
            }
        }
        self.nodes.truncate(checkpoint.nodes); self.rows.truncate(checkpoint.rows); self.metas.truncate(checkpoint.metas);
        self.schemes.truncate(checkpoint.schemes); self.reasons.truncate(checkpoint.reasons); self.requirements.truncate(checkpoint.requirements); self.effects.truncate(checkpoint.effects); self.origins.truncate(checkpoint.origins); self.candidates.truncate(checkpoint.candidates); self.families.truncate(checkpoint.families); self.operation_calls.truncate(checkpoint.operation_calls);
        self.invocation_calls.truncate(checkpoint.invocation_calls); self.error_joins.truncate(checkpoint.error_joins); self.native_contracts.truncate(checkpoint.native_contracts); self.native_families.truncate(checkpoint.native_families);
        let nodes = &self.nodes;
        self.atoms.retain(|_, id| slot(nodes, id.index(), id.generation).is_ok());
    }
    /// Trials rewind on success as well as failure. Returned allocation handles
    /// are retired, so a selected candidate must be instantiated afresh before
    /// its evidence is published. Attempted work remains charged.
    pub fn trial<T>(&mut self, operation: impl FnOnce(&mut Self) -> Result<T, InferenceError>) -> Result<T, InferenceError> {
        self.counters.probes += 1;
        let checkpoint = self.checkpoint(); self.transactions += 1;
        let result = operation(self);
        self.counters.rollbacks += 1; self.rewind(checkpoint);
        self.transactions -= 1;
        if self.transactions == 0 { self.trail.clear(); }
        result
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
