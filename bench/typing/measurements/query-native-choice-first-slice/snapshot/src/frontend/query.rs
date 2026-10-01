//! Owned tooling views of one immutable checked bundle.

pub use crate::sema::check::{BindingIdentity, ComprehensionIdentity, DeclarationIdentity, ExpressionIdentity, ReturnElaboration, StageIdentity, StatementIdentity};
use crate::sema::check::{ProducerEffects, ProducerFlowId, ProducerFlowKind, ProducerFlowSource, ProducerPath, ProducerPathComponent, ProducerProfile, SolvedTypes};
use crate::sema::inference::{Atom, CallableKind, EffectSet, EffectSummary, Eligibility, InferenceError, RequirementTemplate, SchemeId, TypeId, TypeNode, VariableKind};
use crate::sema::types::{ModuleExportType, Type};
use crate::symbol::{Name, SymbolOwner};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryError {
    MissingDeclaration,
    MissingExpression,
    MissingStatement,
    MissingStage,
    MissingComprehension,
    MissingCallable,
    MissingCall,
    MissingOperation,
    MissingBinding,
    MissingProducerProfile,
    MissingProducerFlow,
    ForeignGraph,
    ForeignSymbol,
    Unresolved,
    Recovery,
    ScopeEscape,
    Limit,
}

impl From<InferenceError> for QueryError {
    fn from(error: InferenceError) -> Self {
        match error {
            InferenceError::ForeignHandle => Self::ForeignGraph,
            InferenceError::Unresolved(_) => Self::Unresolved,
            InferenceError::Recovery(_) => Self::Recovery,
            InferenceError::ScopeEscape | InferenceError::InvalidScheme | InferenceError::KindMismatch => Self::ScopeEscape,
            InferenceError::Limit(_) => Self::Limit,
            _ => Self::Recovery,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinderKind { Type, Row }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallableForm { Pure, Proc, Stream }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NominalKind { Tag, ErrorFamily, ErrorVariant, ErrorFacet }

/// A spelling is display metadata, never evidence that nominal types agree.
/// An identity is present only for an actual source or registry declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NominalType {
    pub kind: NominalKind,
    pub spelling: String,
    pub identity: Option<NominalIdentity>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NominalIdentity {
    Source {
        owner: crate::sema::inference::GraphOwner,
        source: crate::source::SourceId,
        namespace: Option<String>,
        declaration: NominalDeclaration,
        member: Option<String>,
    },
    Builtin {
        owner: crate::sema::inference::GraphOwner,
        family: String,
        member: Option<String>,
    },
}

impl NominalIdentity {
    pub fn owner(&self) -> crate::sema::inference::GraphOwner {
        match self { Self::Source { owner, .. } | Self::Builtin { owner, .. } => *owner }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NominalDeclaration {
    Type(crate::syntax::arena::TypeDefId),
    Error(crate::syntax::arena::ErrorDefId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParityError {
    UnidentifiedNominal,
    ForeignNominalOwner,
    ForeignProducerOwner,
    ForeignOperationOwner,
    ForeignNativeOwner,
    UnscopedBinder,
    CapturedRelationship,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum NormalizedEffect {
    Closed(Vec<String>),
    Binder(u32),
    Capture(u32),
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedParameter {
    pub label: String,
    pub ty: NormalizedShape,
    pub defaulted: bool,
    pub rest: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedArrow {
    pub kind: CallableForm,
    pub parameters: Vec<NormalizedParameter>,
    pub result: Box<NormalizedShape>,
    pub effects: NormalizedEffect,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedField {
    pub label: String,
    pub ty: NormalizedShape,
    pub optional: bool,
}

/// Equality compares published metadata; it does not decide assignability,
/// nominal equality, row width, or compatibility of callable effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NormalizedShape {
    Atom(String),
    Nominal(NominalType),
    Binder { index: u32, kind: BinderKind },
    Capture { index: u32, kind: BinderKind },
    BuiltinParameter(String),
    Optional(Box<Self>),
    List(Box<Self>),
    Stream(Box<Self>),
    Map(Box<Self>, Box<Self>),
    Result(Box<Self>, Box<Self>),
    Record { fields: Vec<NormalizedField>, tail: Option<Box<Self>> },
    Row { fields: Vec<NormalizedField>, tail: Option<Box<Self>> },
    Module(Vec<NormalizedField>),
    Arrow(NormalizedArrow),
    CallableChoice(Vec<NormalizedShape>),
    FiniteDomain(Vec<NormalizedDomainAlternative>),
    NativeCallable { signature: Box<Self>, alternatives: Vec<NormalizedCallableAuthority> },
}

impl NormalizedShape {
    /// Borrowing a single signature leaves authority on the complete shape.
    /// A complete choice returns None; inspecting all signatures is separate.
    pub fn callable_signature(&self) -> Option<&NormalizedArrow> {
        match self { Self::Arrow(arrow) => Some(arrow), Self::NativeCallable { signature, .. } => signature.callable_signature(), _ => None }
    }

    /// A choice has no single signature. These borrowed views retain every
    /// member tuple without choosing one or discarding the surrounding authority.
    pub fn callable_signatures(&self) -> Option<Vec<&NormalizedArrow>> {
        match self {
            Self::Arrow(arrow) => Some(vec![arrow]),
            Self::CallableChoice(signatures) if !signatures.is_empty() => signatures.iter().map(|signature| match signature { Self::Arrow(arrow) => Some(arrow), _ => None }).collect(),
            Self::NativeCallable { signature, .. } => signature.callable_signatures(),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NormalizedCallableAuthority { User { signature: Box<NormalizedShape> }, Native(NormalizedNativeAuthority) }

/// A family chooses one retained member; callable alternatives require every
/// possible authority to accept an invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NormalizedNativeAuthority { Single(std::sync::Arc<NormalizedNativeContract>), Family(std::sync::Arc<NormalizedNativeFamilyContract>) }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedNativeFamilyContract {
    pub owner: crate::sema::inference::GraphOwner,
    pub signature: Box<NormalizedShape>,
    pub members: Vec<std::sync::Arc<NormalizedNativeContract>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedDomainAlternative { pub ty: NormalizedShape, pub relation: NormalizedArgumentRelation }

impl NormalizedNativeAuthority {
    fn key(&self) -> &str { match self { Self::Single(contract) => &contract.candidate.identity, Self::Family(family) => family.members.first().map_or("", |contract| &contract.candidate.identity) } }
    fn public_label(&self) -> &str { match self { Self::Single(contract) => &contract.candidate.public_label, Self::Family(family) => family.members.first().map_or("", |contract| &contract.candidate.public_label) } }
}

/// The prototype owns its quantified scope; the receipt is one monomorphic
/// instance in the surrounding source scope, never an implicit new forall.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedNativeContract {
    pub owner: crate::sema::inference::GraphOwner,
    pub candidate: NormalizedCandidate,
    pub prototype: Box<NormalizedScheme>,
    pub signature: Box<NormalizedShape>,
    pub substitutions: Vec<NormalizedShape>,
    pub effect_substitutions: Vec<NormalizedEffect>,
    pub effect_roots: Vec<NormalizedEffect>,
    pub requirements: Vec<NormalizedRequirement>,
    pub actual_eligibility: Vec<(usize, EligibilityPredicate)>,
    pub argument_relations: Vec<NormalizedArgumentRelation>,
    pub has_receiver: bool,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum NormalizedArgumentRelation { Assignable, Exact, DeclaredErasure, EqualityCompatible, InvocationProtocol, CommandTarget { domain: NormalizedCommandTextDomain }, CommandArgv { element: NormalizedCommandTextDomain } }

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum NormalizedCommandTextDomain { Str, Path }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedType {
    shape: NormalizedShape,
    annotation: Option<String>,
}

impl NormalizedType {
    pub fn shape(&self) -> &NormalizedShape { &self.shape }
    pub fn annotation_source(&self) -> Option<&str> { self.annotation.as_deref() }

    pub fn semantic_parity(&self, other: &Self) -> Result<bool, ParityError> {
        let mut owner = None;
        parity_shape(&self.shape, &[], 0, &mut owner)?;
        parity_shape(&other.shape, &[], 0, &mut owner)?;
        Ok(self == other)
    }
}

impl fmt::Display for NormalizedType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result { self.shape.fmt(formatter) }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedQuantifier { pub kind: BinderKind, pub lacks: Vec<String> }

/// Derived summaries represent the minimal calculation of published inputs;
/// ordinary quantifiers are unconstrained inputs within their bounds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedEffectQuantifier { pub lower: Vec<String>, pub upper: Option<Vec<String>>, pub derived: bool }

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NormalizedRequirement {
    Add { left: NormalizedShape, right: NormalizedShape, result: NormalizedShape },
    EqualityCompatible { left: NormalizedShape, right: NormalizedShape },
    EffectInclusion { actual: NormalizedEffect, expected: NormalizedEffect, excluded: Vec<String> },
    Eligibility { predicate: EligibilityPredicate, ty: NormalizedShape },
    CallableInvocation { callable: NormalizedShape, arguments: Vec<NormalizedInvocationArgument>, result: NormalizedShape, effects: NormalizedEffect, domain: NormalizedCallableDomain },
    Operation { binding: NormalizedOperationBinding, effect_mode: NormalizedOperationEffectMode, mono_authority: Option<NormalizedNativeAuthority>, declared_error_bound: Option<NormalizedShape>, candidates: Vec<NormalizedCandidate>, receiver: Option<NormalizedShape>, arguments: Vec<Option<NormalizedShape>>, result: NormalizedShape, effects: NormalizedEffect, effect_bindings: Vec<(NormalizedEffectRole, NormalizedEffect)>, output_effect_bindings: Vec<(NormalizedProducerRole, NormalizedEffect)> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedInvocationCall {
    pub callable: NormalizedShape,
    pub arguments: Vec<NormalizedInvocationArgument>,
    pub result: NormalizedShape,
    pub effects: NormalizedEffect,
    pub domain: NormalizedCallableDomain,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NormalizedOperationBinding {
    Slots,
    /// Member-dependent labels and arity are resolved by the published child
    /// certificate, while this payload retains the original ordered arguments.
    Invocation(Box<NormalizedInvocationCall>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalizedOperationEffectMode { AvailableBudget, ComputedCreation }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalizedCallableDomain { Pure, AnyCallable, Exact(CallableForm) }

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NormalizedInvocationArgumentKind { Positional, Named(String), PositionalSplice }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedInvocationArgument { pub kind: NormalizedInvocationArgumentKind, pub ty: NormalizedShape }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EligibilityPredicate { MapKey, JsonCompatible, NonUnit, Sortable, SortableKey, ArgvItem, CountKey, Record, YieldItem, CommandTarget, CommandArgv, Error, Display, ArgvExpansion }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedCandidate {
    pub failure_projection: Option<NormalizedOperationFailureProjection>,
    pub identity: String,
    pub public_label: String,
    pub effect_quantifiers: Vec<NormalizedEffectQuantifier>,
    pub effect_roles: Vec<(NormalizedEffectRole, NormalizedEffectRoleReference)>,
    pub output_effect_roots: Vec<NormalizedEffect>,
    pub output_effect_roles: Vec<(NormalizedProducerRole, u32)>,
}

/// Failure sources refer to the existing receiver or canonical argument vector;
/// they neither add an argument nor replace the enclosing completion bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalizedOperationFailureProjection {
    ReceiverResultError,
    ArgumentResultError { argument: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalizedArgumentCoercion { PathLikeToPath }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedCallBinding {
    pub supplied_slots: Vec<usize>,
    pub default_slots: Vec<usize>,
    pub rest_slot: Option<usize>,
    pub dynamic: Option<NormalizedDynamicInvocationBinding>,
}

/// Source segments retain their original argument positions. Unknown lengths
/// require the published runtime guards before fixed slots can be filled.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NormalizedInvocationArgumentSegment {
    StaticSlot { argument: usize, slot: usize },
    DynamicRange { argument: usize, fixed_slots: Vec<usize>, rest_slot: Option<usize> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedDynamicInvocationBinding {
    pub segments: Vec<NormalizedInvocationArgumentSegment>,
    pub conditional_default_slots: Vec<usize>,
    pub required_slots: Vec<usize>,
    pub runtime_arity_guard: bool,
    pub runtime_duplicate_guard: bool,
}

/// An operation's binders belong to its published source scope. Its binding
/// preserves supplied source order, independently of omitted default slots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedOperation {
    pub owner: crate::sema::inference::GraphOwner,
    pub caller: Option<NormalizedDeclarationIdentity>,
    pub enclosing_scheme: Option<NormalizedScheme>,
    pub requirement: NormalizedRequirement,
    pub result: NormalizedShape,
    pub effects: NormalizedEffect,
    pub receiver: Option<NormalizedShape>,
    pub actual_arguments: Vec<NormalizedShape>,
    pub argument_coercions: Vec<(usize, NormalizedArgumentCoercion)>,
    pub binding: NormalizedCallBinding,
}

impl NormalizedOperation {
    pub fn semantic_parity(&self, other: &Self) -> Result<bool, ParityError> {
        if self.owner != other.owner { return Err(ParityError::ForeignOperationOwner); }
        let mut owner = Some(self.owner);
        for operation in [self, other] {
            if let Some(scheme) = &operation.enclosing_scheme { parity_scheme(scheme, &mut owner)?; }
            let binders = operation.enclosing_scheme.as_ref().map(|scheme| scheme.quantifiers.as_slice()).unwrap_or(&[]);
            let effects = operation.enclosing_scheme.as_ref().map(|scheme| scheme.effect_quantifiers.len()).unwrap_or(0);
            parity_requirement(&operation.requirement, binders, effects, &mut owner)?;
            for shape in operation.receiver.iter().chain(operation.actual_arguments.iter()).chain(std::iter::once(&operation.result)) { parity_shape(shape, binders, effects, &mut owner)?; }
            parity_effect(&operation.effects, effects)?;
        }
        Ok(self == other)
    }
}

/// Binder ordinals refer to this candidate's formal quantifier list, separately
/// from the caller's normalized effect bindings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NormalizedEffectRoleReference { Binder(u32), Fixed(Vec<String>) }

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum NormalizedEffectRole {
    Creation, Callback, Pull { source: u32 }, Close { source: u32 },
    PullProjection { source: u32, projection: NormalizedEffectProjection },
    CloseProjection { source: u32, projection: NormalizedEffectProjection },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum NormalizedEffectProjection { ResultSuccess }

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum NormalizedProducerRole { Pull, Close }

fn producer_role(role: crate::sema::inference::ProducerRole) -> NormalizedProducerRole {
    match role { crate::sema::inference::ProducerRole::Pull => NormalizedProducerRole::Pull, crate::sema::inference::ProducerRole::Close => NormalizedProducerRole::Close }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedScheme {
    pub ty: NormalizedType,
    pub quantifiers: Vec<NormalizedQuantifier>,
    pub effect_quantifiers: Vec<NormalizedEffectQuantifier>,
    pub effect_roots: Vec<NormalizedEffect>,
    pub effect_inclusions: Vec<(NormalizedEffect, NormalizedEffect)>,
    pub requirements: Vec<NormalizedRequirement>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum NormalizedProducerPathComponent {
    RecordField(String), ListItem, MapKey, MapValue, OptionalPayload,
    ResultSuccess, ResultError, CallableParameter(u32), CallableResult,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NormalizedProducerPath(pub Vec<NormalizedProducerPathComponent>);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedProducerEffects { pub pull: NormalizedEffect, pub close: NormalizedEffect }

pub type NormalizedProducerProfile = std::collections::BTreeMap<NormalizedProducerPath, NormalizedProducerEffects>;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NormalizedDeclarationIdentity {
    pub source: crate::source::SourceId,
    pub namespace: Option<String>,
    pub declaration: crate::syntax::arena::FunctionDefId,
}


#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NormalizedBindingIdentity {
    pub source: crate::source::SourceId,
    pub namespace: Option<String>,
    pub target: crate::syntax::arena::BindingTargetId,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NormalizedExpressionIdentity {
    pub source: crate::source::SourceId,
    pub namespace: Option<String>,
    pub expression: crate::syntax::arena::ExprId,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NormalizedStatementIdentity {
    pub source: crate::source::SourceId,
    pub namespace: Option<String>,
    pub statement: crate::syntax::arena::StmtId,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NormalizedStageIdentity { pub pipeline: NormalizedExpressionIdentity, pub index: u32 }

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct NormalizedComprehensionIdentity { pub expression: NormalizedExpressionIdentity, pub qualifier: u32 }

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum NormalizedProducerFlowSource {
    Expression(NormalizedExpressionIdentity),
    Statement(NormalizedStatementIdentity),
    Stage(NormalizedStageIdentity),
    Comprehension(NormalizedComprehensionIdentity),
    Binding { identity: NormalizedBindingIdentity, version: u32 },
    Parameter { declaration: NormalizedDeclarationIdentity, index: u32 },
    DeclarationResult(NormalizedDeclarationIdentity),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedProducerFlowField { pub path: NormalizedProducerPath, pub input: u32 }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedProducerFlowOperationTransfer {
    pub input: u32,
    pub input_path: NormalizedProducerPath,
    pub output_path: NormalizedProducerPath,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedProducerFlowOperationAlternative {
    pub identity: String,
    pub public_label: String,
    pub path: Option<NormalizedProducerPath>,
    pub transfers: Vec<NormalizedProducerFlowOperationTransfer>,
    pub opaque: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NormalizedProducerFlowKind {
    Empty,
    Opaque,
    Known(NormalizedProducerProfile),
    Addition { requirement: Box<NormalizedRequirement>, left: u32, right: u32 },
    Operation { requirement: Box<NormalizedRequirement>, alternatives: Vec<NormalizedProducerFlowOperationAlternative>, outputs: NormalizedProducerEffects },
    Parameter { declaration: NormalizedDeclarationIdentity, index: u32 },
    CapturedBinding { identity: NormalizedBindingIdentity, version: u32, input: u32 },
    Project { input: u32, path: NormalizedProducerPath },
    Aggregate { entries: Vec<NormalizedProducerFlowField> },
    /// Replacements overlay matching base paths in authored order; unmodified
    /// base paths remain reachable through the separate base reference.
    RecordUpdate { base: u32, replacements: Vec<NormalizedProducerFlowField> },
    Join { inputs: Vec<u32> },
    Callable { declaration: NormalizedDeclarationIdentity },
    NativeCallable { authority: NormalizedNativeAuthority },
    Apply { call: NormalizedExpressionIdentity, callee: u32, arguments: Vec<u32> },
    StageApply { stage: NormalizedStageIdentity, callee: u32, arguments: Vec<u32> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedProducerFlowNode {
    pub source: NormalizedProducerFlowSource,
    pub scope: Option<u32>,
    pub kind: NormalizedProducerFlowKind,
}

/// Local node and scope references preserve sharing within this exact owner.
/// Declaration call edges remain symbolic and never expand recursive bodies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedProducerFlowGraph {
    pub owner: crate::sema::inference::GraphOwner,
    pub root: u32,
    pub scopes: Vec<NormalizedScheme>,
    pub nodes: Vec<NormalizedProducerFlowNode>,
}

impl NormalizedProducerFlowGraph {
    pub fn semantic_parity(&self, other: &Self) -> Result<bool, ParityError> {
        if self.owner != other.owner { return Err(ParityError::ForeignProducerOwner); }
        for graph in [self, other] {
            let mut owner = None;
            for scheme in &graph.scopes { parity_scheme(scheme, &mut owner)?; }
            for node in &graph.nodes {
                let scope = node.scope.map(|scope| graph.scopes.get(scope as usize).ok_or(ParityError::UnscopedBinder)).transpose()?;
                let effects = scope.map_or(0, |scheme| scheme.effect_quantifiers.len());
                match &node.kind {
                    NormalizedProducerFlowKind::NativeCallable { authority } => parity_authority(authority, scope.map_or(&[], |scheme| scheme.quantifiers.as_slice()), effects, &mut owner)?,
                    NormalizedProducerFlowKind::Known(profile) => {
                        for producer in profile.values() { parity_effect(&producer.pull, effects)?; parity_effect(&producer.close, effects)?; }
                    }
                    NormalizedProducerFlowKind::Addition { requirement, .. } => parity_requirement(requirement, scope.map_or(&[], |scheme| scheme.quantifiers.as_slice()), effects, &mut owner)?,
                    NormalizedProducerFlowKind::Operation { requirement, outputs, .. } => {
                        parity_requirement(requirement, scope.map_or(&[], |scheme| scheme.quantifiers.as_slice()), effects, &mut owner)?;
                        parity_effect(&outputs.pull, effects)?; parity_effect(&outputs.close, effects)?;
                    }
                    _ => {}
                }
            }
        }
        Ok(self == other)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedCallable {
    pub scheme: NormalizedScheme,
    /// An unselected choice has no single summary. Its exact member effects
    /// remain on each signature; absence does not promise an empty effect set.
    pub effective_effects: Option<NormalizedEffect>,
    pub required_effects: Option<NormalizedEffect>,
    pub return_elaboration: Option<ReturnElaboration>,
    pub parameter_producers: Option<Vec<NormalizedProducerProfile>>,
    pub return_producers: Option<NormalizedProducerProfile>,
}

impl fmt::Display for NormalizedCallable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.scheme.fmt(formatter)?;
        let has_profiles = self.parameter_producers.iter().flatten().any(|profile| !profile.is_empty()) || self.return_producers.as_ref().is_some_and(|profile| !profile.is_empty());
        if !has_profiles { return Ok(()); }
        let arrow = self.scheme.ty.shape().callable_signature().ok_or(fmt::Error)?;
        write!(formatter, "; creation {}", EffectDisplay(&arrow.effects))?;
        if let Some(profiles) = &self.parameter_producers {
            if profiles.len() != arrow.parameters.len() { return Err(fmt::Error); }
            for (parameter, profile) in arrow.parameters.iter().zip(profiles) {
                write_profile(formatter, &format!("parameter {}", diagnostic_label(&parameter.label)), profile)?;
            }
        }
        if let Some(profile) = &self.return_producers { write_profile(formatter, "result", profile)?; }
        Ok(())
    }
}

fn diagnostic_label(label: &str) -> String {
    let mut characters = label.chars();
    if characters.next().is_some_and(|character| character.is_alphabetic() || character == '_') && characters.all(|character| character.is_alphanumeric() || character == '_') { label.to_string() }
    else { format!("{label:?}") }
}

fn write_profile(formatter: &mut fmt::Formatter<'_>, prefix: &str, profile: &NormalizedProducerProfile) -> fmt::Result {
    for (path, effects) in profile {
        let mut label = prefix.to_string();
        for component in &path.0 {
            use std::fmt::Write;
            match component {
                NormalizedProducerPathComponent::RecordField(name) => write!(&mut label, ".field({name:?})")?,
                NormalizedProducerPathComponent::ListItem => label.push_str(".item"),
                NormalizedProducerPathComponent::MapKey => label.push_str(".key"),
                NormalizedProducerPathComponent::MapValue => label.push_str(".value"),
                NormalizedProducerPathComponent::OptionalPayload => label.push_str(".optional"),
                NormalizedProducerPathComponent::ResultSuccess => label.push_str(".ok"),
                NormalizedProducerPathComponent::ResultError => label.push_str(".error"),
                NormalizedProducerPathComponent::CallableParameter(index) => write!(&mut label, ".parameter({index})")?,
                NormalizedProducerPathComponent::CallableResult => label.push_str(".result"),
            }
        }
        write!(formatter, "; {label}.pull {}; {label}.close {}", EffectDisplay(&effects.pull), EffectDisplay(&effects.close))?;
    }
    Ok(())
}

impl NormalizedCallable {
    /// Metadata equality alone cannot certify opaque nominal spellings or free
    /// captured relationships. Consumers must pass this check before comparing
    /// independently projected contracts as semantic answers.
    pub fn semantic_parity(&self, other: &Self) -> Result<bool, ParityError> {
        let mut owner = None;
        for callable in [self, other] {
            parity_scheme(&callable.scheme, &mut owner)?;
            let effects = callable.scheme.effect_quantifiers.len();
            if let Some(effective) = &callable.effective_effects { parity_effect(effective, effects)?; }
            if let Some(required) = &callable.required_effects { parity_effect(required, effects)?; }
            for profile in callable.parameter_producers.iter().flatten().chain(callable.return_producers.iter()) {
                for producer in profile.values() { parity_effect(&producer.pull, effects)?; parity_effect(&producer.close, effects)?; }
            }
        }
        Ok(self == other)
    }
}

impl NormalizedScheme {
    pub fn semantic_parity(&self, other: &Self) -> Result<bool, ParityError> {
        let mut owner = None;
        parity_scheme(self, &mut owner)?;
        parity_scheme(other, &mut owner)?;
        Ok(self == other)
    }
}

fn parity_effect(effect: &NormalizedEffect, binders: usize) -> Result<(), ParityError> {
    match effect {
        NormalizedEffect::Binder(index) if *index as usize >= binders => Err(ParityError::UnscopedBinder),
        NormalizedEffect::Capture(_) => Err(ParityError::CapturedRelationship),
        _ => Ok(()),
    }
}

fn parity_shape(shape: &NormalizedShape, binders: &[NormalizedQuantifier], effects: usize, owner: &mut Option<crate::sema::inference::GraphOwner>) -> Result<(), ParityError> {
    let mut pending = vec![shape];
    while let Some(shape) = pending.pop() {
        match shape {
            NormalizedShape::Nominal(nominal) => {
                let identity = nominal.identity.as_ref().ok_or(ParityError::UnidentifiedNominal)?;
                if owner.is_some_and(|owner| owner != identity.owner()) { return Err(ParityError::ForeignNominalOwner); }
                *owner = Some(identity.owner());
            }
            NormalizedShape::Binder { index, kind } => {
                if binders.get(*index as usize).is_none_or(|binder| binder.kind != *kind) { return Err(ParityError::UnscopedBinder); }
            }
            NormalizedShape::Capture { .. } => return Err(ParityError::CapturedRelationship),
            NormalizedShape::Optional(item) | NormalizedShape::List(item) | NormalizedShape::Stream(item) => pending.push(item),
            NormalizedShape::Map(key, value) | NormalizedShape::Result(key, value) => { pending.push(key); pending.push(value); }
            NormalizedShape::Record { fields, tail } | NormalizedShape::Row { fields, tail } => {
                pending.extend(fields.iter().map(|field| &field.ty));
                if let Some(tail) = tail { pending.push(tail); }
            }
            NormalizedShape::Module(fields) => pending.extend(fields.iter().map(|field| &field.ty)),
            NormalizedShape::CallableChoice(signatures) => { if signatures.is_empty() || signatures.iter().any(|signature| !matches!(signature, NormalizedShape::Arrow(_))) { return Err(ParityError::UnscopedBinder); } pending.extend(signatures); },
            NormalizedShape::FiniteDomain(alternatives) => pending.extend(alternatives.iter().map(|alternative| &alternative.ty)),
            NormalizedShape::NativeCallable { signature, alternatives } => {
                pending.push(signature);
                for authority in alternatives { match authority {
                    NormalizedCallableAuthority::User { signature } => pending.push(signature),
                    NormalizedCallableAuthority::Native(authority) => parity_authority(authority, binders, effects, owner)?,
                } }
            }
            NormalizedShape::Arrow(arrow) => {
                pending.extend(arrow.parameters.iter().map(|parameter| &parameter.ty));
                pending.push(&arrow.result); parity_effect(&arrow.effects, effects)?;
            }
            NormalizedShape::BuiltinParameter(_) => return Err(ParityError::UnscopedBinder),
            NormalizedShape::Atom(_) => {}
        }
    }
    Ok(())
}

fn parity_candidate(candidate: &NormalizedCandidate) -> Result<(), ParityError> {
    for (_, reference) in &candidate.effect_roles {
        if let NormalizedEffectRoleReference::Binder(index) = reference {
            if *index as usize >= candidate.effect_quantifiers.len() { return Err(ParityError::UnscopedBinder); }
        }
    }
    for root in &candidate.output_effect_roots { parity_effect(root, candidate.effect_quantifiers.len())?; }
    if candidate.output_effect_roles.iter().any(|(_, index)| *index as usize >= candidate.output_effect_roots.len()) { return Err(ParityError::UnscopedBinder); }
    Ok(())
}

fn parity_authority(authority: &NormalizedNativeAuthority, binders: &[NormalizedQuantifier], effects: usize, owner: &mut Option<crate::sema::inference::GraphOwner>) -> Result<(), ParityError> {
    match authority {
        NormalizedNativeAuthority::Single(contract) => parity_native(contract, binders, effects, owner),
        NormalizedNativeAuthority::Family(family) => {
            if owner.is_some_and(|owner| owner != family.owner) { return Err(ParityError::ForeignNativeOwner); }
            *owner = Some(family.owner);
            if family.members.is_empty() { return Err(ParityError::UnscopedBinder); }
            parity_shape(&family.signature, binders, effects, owner)?;
            for member in &family.members { parity_native(member, binders, effects, owner)?; }
            Ok(())
        }
    }
}

fn parity_native(contract: &NormalizedNativeContract, binders: &[NormalizedQuantifier], effects: usize, owner: &mut Option<crate::sema::inference::GraphOwner>) -> Result<(), ParityError> {
    if owner.is_some_and(|owner| owner != contract.owner) { return Err(ParityError::ForeignNativeOwner); }
    *owner = Some(contract.owner);
    parity_candidate(&contract.candidate)?;
    parity_scheme(&contract.prototype, owner)?;
    parity_shape(&contract.signature, binders, effects, owner)?;
    for ty in &contract.substitutions { parity_shape(ty, binders, effects, owner)?; }
    for summary in contract.effect_substitutions.iter().chain(&contract.effect_roots) { parity_effect(summary, effects)?; }
    for requirement in &contract.requirements { parity_requirement(requirement, binders, effects, owner)?; }
    Ok(())
}

fn parity_scheme(scheme: &NormalizedScheme, owner: &mut Option<crate::sema::inference::GraphOwner>) -> Result<(), ParityError> {
    let binders = &scheme.quantifiers;
    let effects = scheme.effect_quantifiers.len();
    parity_shape(&scheme.ty.shape, binders, effects, owner)?;
    for requirement in &scheme.requirements { parity_requirement(requirement, binders, effects, owner)?; }
    for (actual, expected) in &scheme.effect_inclusions { parity_effect(actual, effects)?; parity_effect(expected, effects)?; }
    for effect in &scheme.effect_roots { parity_effect(effect, effects)?; }
    Ok(())
}

fn parity_invocation_call(call: &NormalizedInvocationCall, binders: &[NormalizedQuantifier], effects: usize, owner: &mut Option<crate::sema::inference::GraphOwner>) -> Result<(), ParityError> {
    for ty in std::iter::once(&call.callable).chain(call.arguments.iter().map(|argument| &argument.ty)).chain(std::iter::once(&call.result)) { parity_shape(ty, binders, effects, owner)?; }
    parity_effect(&call.effects, effects)
}

fn parity_requirement(requirement: &NormalizedRequirement, binders: &[NormalizedQuantifier], effects: usize, owner: &mut Option<crate::sema::inference::GraphOwner>) -> Result<(), ParityError> {
    match requirement {
        NormalizedRequirement::Add { left, right, result } => { for ty in [left, right, result] { parity_shape(ty, binders, effects, owner)?; } }
        NormalizedRequirement::EqualityCompatible { left, right } => { for ty in [left, right] { parity_shape(ty, binders, effects, owner)?; } }
        NormalizedRequirement::EffectInclusion { actual, expected, .. } => { parity_effect(actual, effects)?; parity_effect(expected, effects)?; }
        NormalizedRequirement::Eligibility { ty, .. } => parity_shape(ty, binders, effects, owner)?,
        NormalizedRequirement::CallableInvocation { callable, arguments, result, effects: summary, .. } => {
            for ty in std::iter::once(callable).chain(arguments.iter().map(|argument| &argument.ty)).chain(std::iter::once(result)) { parity_shape(ty, binders, effects, owner)?; }
            parity_effect(summary, effects)?;
        }
        NormalizedRequirement::Operation { binding, effect_mode: _, mono_authority, declared_error_bound, candidates, receiver, arguments, result, effects: summary, effect_bindings, output_effect_bindings } => {
            if let NormalizedOperationBinding::Invocation(call) = binding { parity_invocation_call(call, binders, effects, owner)?; }
            if let Some(authority) = mono_authority { parity_authority(authority, binders, effects, owner)?; }
            for ty in receiver.iter().chain(arguments.iter().flatten()).chain(std::iter::once(result)).chain(declared_error_bound.iter()) { parity_shape(ty, binders, effects, owner)?; }
            parity_effect(summary, effects)?;
            for (_, summary) in effect_bindings { parity_effect(summary, effects)?; }
            for (_, summary) in output_effect_bindings { parity_effect(summary, effects)?; }
            for candidate in candidates { parity_candidate(candidate)?; }
        }
    }
    Ok(())
}


#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedBinding {
    pub scheme: NormalizedScheme,
    pub mutable: bool,
    pub producers: Option<NormalizedProducerProfile>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalizedInvocationDefaultTiming { AtCall, AtPull }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedNativeInvocationAlternative {
    pub origin: NormalizedExpressionIdentity,
    pub authority: NormalizedNativeAuthority,
    pub selected_member: Option<std::sync::Arc<NormalizedNativeContract>>,
    pub selected_binding: Option<NormalizedCallBinding>,
    pub selected_actual_arguments: Vec<Option<NormalizedShape>>,
    pub operation: NormalizedRequirement,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedInvocationEvidence {
    pub callable: NormalizedShape,
    pub signature: NormalizedShape,
    pub binding: NormalizedCallBinding,
    pub default_timing: NormalizedInvocationDefaultTiming,
    pub result: NormalizedShape,
    pub effects: NormalizedEffect,
    pub native_alternatives: Vec<NormalizedNativeInvocationAlternative>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedInvocation {
    pub owner: crate::sema::inference::GraphOwner,
    pub requirement: NormalizedRequirement,
    pub enclosing_scheme: Option<NormalizedScheme>,
    pub evidence: Option<NormalizedInvocationEvidence>,
}

impl NormalizedInvocation {
    pub fn semantic_parity(&self, other: &Self) -> Result<bool, ParityError> {
        if self.owner != other.owner { return Err(ParityError::ForeignNativeOwner); }
        for invocation in [self, other] {
            let mut owner = Some(invocation.owner);
            let scope = invocation.enclosing_scheme.as_ref();
            if let Some(scope) = scope { parity_scheme(scope, &mut owner)?; }
            let binders: &[NormalizedQuantifier] = scope.map_or(&[], |scope| scope.quantifiers.as_slice());
            let effects = scope.map_or(0, |scope| scope.effect_quantifiers.len());
            parity_requirement(&invocation.requirement, binders, effects, &mut owner)?;
            if let Some(evidence) = &invocation.evidence {
                for shape in [&evidence.callable, &evidence.signature, &evidence.result] { parity_shape(shape, binders, effects, &mut owner)?; }
                parity_effect(&evidence.effects, effects)?;
                for alternative in &evidence.native_alternatives { parity_authority(&alternative.authority, binders, effects, &mut owner)?; if let Some(member) = &alternative.selected_member { parity_native(member, binders, effects, &mut owner)?; } for actual in alternative.selected_actual_arguments.iter().flatten() { parity_shape(actual, binders, effects, &mut owner)?; } parity_requirement(&alternative.operation, binders, effects, &mut owner)?; }
            }
        }
        Ok(self == other)
    }
}

/// Display expansion has its own bounds because shared graph constructors can
/// expand into a much larger tree without adding inference nodes.
#[derive(Clone, Copy, Debug)]
pub struct QueryLimits { pub depth: usize, pub nodes: usize, pub text_bytes: usize }

impl Default for QueryLimits {
    fn default() -> Self { Self { depth: 512, nodes: 100_000, text_bytes: 4 * 1024 * 1024 } }
}

pub struct SolvedQuery<'a> {
    solved: &'a SolvedTypes,
    symbols: &'a SymbolOwner,
    limits: QueryLimits,
}

impl<'a> SolvedQuery<'a> {
    pub fn new(solved: &'a SolvedTypes, symbols: &'a SymbolOwner) -> Self {
        Self { solved, symbols, limits: QueryLimits::default() }
    }

    pub fn with_limits(mut self, limits: QueryLimits) -> Self { self.limits = limits; self }

    pub fn declaration(&self, identity: DeclarationIdentity) -> Result<NormalizedCallable, QueryError> {
        self.validate_owner()?;
        let fact = self.solved.declarations.get(&identity).ok_or(QueryError::MissingDeclaration)?;
        let mut view = View::new(self, Some(fact.scheme));
        view.seed_profiles(fact.parameter_producers.iter().chain(std::iter::once(&fact.return_producers)))?;
        let scheme = view.scheme(fact.signature, Some(fact.scheme))?;
        let arrow = scheme.ty.shape().callable_signature().ok_or(QueryError::Unresolved)?;
        if fact.parameter_producers.len() != arrow.parameters.len() { return Err(QueryError::Recovery); }
        Ok(NormalizedCallable {
            scheme,
            effective_effects: Some(view.effect(fact.effective_effects)?),
            required_effects: Some(view.effect(fact.required_effects)?),
            return_elaboration: Some(fact.return_elaboration),
            parameter_producers: Some(fact.parameter_producers.iter().map(|profile| view.profile(profile)).collect::<Result<Vec<_>, _>>()?),
            return_producers: Some(view.profile(&fact.return_producers)?),
        })
    }

    pub fn expression(&self, identity: ExpressionIdentity) -> Result<NormalizedType, QueryError> {
        self.validate_owner()?;
        let ty = *self.solved.expressions.get(&identity).ok_or(QueryError::MissingExpression)?;
        let scope = self.expression_scope(identity)?;
        let mut view = View::new(self, scope);
        view.seed_profiles(self.solved.expression_producers.get(&identity))?;
        view.ty(ty)
    }

    /// Published callable and value schemes retain their principal relationship.
    /// Other expressions reveal their exact checked instantiated type.
    pub fn reveal(&self, identity: ExpressionIdentity) -> Result<String, QueryError> {
        if self.solved.expression_callables.contains_key(&identity) { return Ok(self.expression_callable(identity)?.to_string()); }
        Ok(self.expression_scheme(identity)?.to_string())
    }

    pub fn expression_scheme(&self, identity: ExpressionIdentity) -> Result<NormalizedScheme, QueryError> {
        self.validate_owner()?;
        if self.solved.expression_callables.contains_key(&identity) {
            return Ok(self.expression_callable(identity)?.scheme);
        }
        let ty = *self.solved.expressions.get(&identity).ok_or(QueryError::MissingExpression)?;
        let scheme = self.solved.expression_schemes.get(&identity).copied();
        let mut view = View::new(self, self.expression_scope(identity)?);
        view.seed_profiles(self.solved.expression_producers.get(&identity))?;
        view.scheme(ty, scheme)
    }

    pub fn expression_callable(&self, identity: ExpressionIdentity) -> Result<NormalizedCallable, QueryError> {
        self.validate_owner()?;
        let fact = self.solved.expression_callables.get(&identity).ok_or(QueryError::MissingCallable)?;
        let scope = fact.scheme.or(self.expression_scope(identity)?);
        let declaration = fact.declaration.and_then(|identity| self.solved.declarations.get(&identity));
        let principal = declaration.filter(|declaration| fact.scheme == Some(declaration.scheme));
        let mut view = View::new(self, scope);
        if let Some(declaration) = principal { view.seed_profiles(declaration.parameter_producers.iter().chain(std::iter::once(&declaration.return_producers)))?; }
        else { view.seed_profiles(self.solved.expression_producers.get(&identity))?; }
        let scheme = view.scheme(fact.signature, fact.scheme)?;
        if scheme.ty.shape().callable_signatures().is_none() { return Err(QueryError::Unresolved); }
        let effective_effects = principal.map(|declaration| view.effect(declaration.effective_effects)).transpose()?.or_else(|| scheme.ty.shape().callable_signature().map(|arrow| arrow.effects.clone()));
        // A declaration target is navigation metadata. A monomorphic value's
        // signature and quantified scope still come from its own checked fact.
        Ok(NormalizedCallable {
            scheme, effective_effects,
            required_effects: declaration.map(|declaration| view.effect(declaration.required_effects)).transpose()?,
            return_elaboration: declaration.map(|declaration| declaration.return_elaboration),
            parameter_producers: principal.map(|declaration| declaration.parameter_producers.iter().map(|profile| view.profile(profile)).collect::<Result<Vec<_>, _>>()).transpose()?,
            return_producers: principal.map(|declaration| view.profile(&declaration.return_producers)).transpose()?,
        })
    }

    pub fn binding(&self, identity: BindingIdentity) -> Result<NormalizedBinding, QueryError> {
        self.validate_owner()?;
        let fact = self.solved.bindings.get(&identity).ok_or(QueryError::MissingBinding)?;
        let scope = fact.scheme.or(fact.owner.map(|identity| self.declaration_scope(identity)).transpose()?);
        let mut view = View::new(self, scope);
        let profile = self.solved.binding_producers.get(&identity);
        view.seed_profiles(profile)?;
        let scheme = view.scheme(fact.ty, fact.scheme)?;
        Ok(NormalizedBinding { scheme, mutable: fact.mutable, producers: profile.map(|profile| view.profile(profile)).transpose()? })
    }

    pub fn call_binding(&self, identity: ExpressionIdentity) -> Result<NormalizedCallBinding, QueryError> {
        self.validate_owner()?;
        if !self.solved.expressions.contains_key(&identity) { return Err(QueryError::MissingExpression); }
        let fact = self.solved.calls.get(&identity).ok_or(QueryError::MissingCall)?;
        let scope = fact.caller.map(|caller| self.declaration_scope(caller)).transpose()?;
        View::new(self, scope).call_binding(&fact.binding)
    }

    /// Residual obligations and selected invocation receipts are distinct facts.
    /// An absent receipt does not invent labels, argument slots or native authority.
    pub fn invocation(&self, identity: ExpressionIdentity) -> Result<NormalizedInvocation, QueryError> {
        self.validate_owner()?;
        if !self.solved.expressions.contains_key(&identity) { return Err(QueryError::MissingExpression); }
        let fact = self.solved.invocations.get(&identity).ok_or(QueryError::MissingCall)?;
        let scope = self.expression_scope(identity)?.or(fact.caller.map(|caller| self.declaration_scope(caller)).transpose()?);
        let graph = &self.solved.graph;
        let template = graph.requirement_template(fact.requirement)?;
        if !matches!(template, RequirementTemplate::CallableInvocation { .. }) { return Err(QueryError::Recovery); }
        let mut view = View::new(self, scope);
        let requirement = view.requirement(template)?;
        let evidence = graph.invocation_evidence(fact.requirement)?.map(|evidence| {
            if evidence.native_alternatives.len() > self.limits.nodes.saturating_sub(view.nodes) { return Err(QueryError::Limit); }
            let binding = view.binding_parts(&evidence.supplied_slots, &evidence.default_slots, evidence.rest_slot, evidence.dynamic.as_ref())?;
            let mut native_alternatives = evidence.native_alternatives.iter().map(|alternative| {
                view.visit(0)?;
                let origin = graph.native_authority_origin(alternative.authority)?;
                let mut source = None;
                for (&identity, reference) in &self.solved.registry_references {
                    view.visit(0)?;
                    if reference.native_authority() == Some(origin) {
                        if source.replace(identity).is_some() { return Err(QueryError::Recovery); }
                    }
                }
                let source = source.ok_or(QueryError::MissingCallable)?;
                let operation = graph.requirement_template(alternative.operation)?;
                if !matches!(operation, RequirementTemplate::Operation { .. }) { return Err(QueryError::Recovery); }
                let selected_member = graph.candidate_evidence(alternative.operation)?.map(|evidence| {
                    let member = graph.native_authority_member(alternative.authority, evidence.candidate)?;
                    view.native_contract(member, 0)
                }).transpose()?;
                let candidate = graph.candidate_evidence(alternative.operation)?;
                let selected_binding = candidate.and_then(|evidence| evidence.binding.as_ref()).map(|binding| view.binding_parts(&binding.supplied_slots, &binding.default_slots, binding.rest_slot, binding.dynamic.as_ref())).transpose()?;
                let selected_actual_arguments = if let Some(candidate) = candidate {
                    if candidate.actual_arguments.len() > self.limits.nodes.saturating_sub(view.nodes) { return Err(QueryError::Limit); }
                    candidate.actual_arguments.iter().map(|ty| { view.visit(0)?; ty.map(|ty| view.graph(ty, 0)).transpose() }).collect::<Result<Vec<_>, QueryError>>()?
                } else { Vec::new() };
                Ok(NormalizedNativeInvocationAlternative { origin: view.expression_identity(source)?, authority: view.native_authority(alternative.authority, 0)?, selected_member, selected_binding, selected_actual_arguments, operation: view.requirement(operation)? })
            }).collect::<Result<Vec<_>, QueryError>>()?;
            native_alternatives.sort_by(|left, right| left.authority.key().cmp(right.authority.key()));
            Ok(NormalizedInvocationEvidence {
                callable: view.graph(evidence.callable, 0)?, signature: view.graph(evidence.signature, 0)?, binding,
                default_timing: match evidence.default_timing { crate::sema::inference::InvocationDefaultTiming::AtCall => NormalizedInvocationDefaultTiming::AtCall, crate::sema::inference::InvocationDefaultTiming::AtPull => NormalizedInvocationDefaultTiming::AtPull },
                result: view.graph(evidence.result, 0)?, effects: view.effect(evidence.effects)?, native_alternatives,
            })
        }).transpose()?;
        let enclosing_scheme = scope.map(|scope| view.scheme(graph.scheme(scope)?.body, Some(scope))).transpose()?;
        Ok(NormalizedInvocation { owner: self.solved.owner, requirement, enclosing_scheme, evidence })
    }

    pub fn language_operation(&self, identity: ExpressionIdentity) -> Result<NormalizedOperation, QueryError> {
        self.validate_owner()?;
        if !self.solved.expressions.contains_key(&identity) { return Err(QueryError::MissingExpression); }
        self.operation(ProducerFlowSource::Expression(identity), self.solved.operations.get(&identity).ok_or(QueryError::MissingOperation)?)
    }

    pub fn statement_operation(&self, identity: StatementIdentity) -> Result<NormalizedOperation, QueryError> {
        self.validate_owner()?;
        if !self.solved.statements.contains_key(&identity) { return Err(QueryError::MissingStatement); }
        self.operation(ProducerFlowSource::Statement(identity), self.solved.statement_operations.get(&identity).ok_or(QueryError::MissingOperation)?)
    }

    pub fn comprehension_operation(&self, identity: ComprehensionIdentity) -> Result<NormalizedOperation, QueryError> {
        self.validate_owner()?;
        self.comprehension_scope(identity)?;
        self.operation(ProducerFlowSource::Comprehension(identity), &self.solved.comprehension_operations.get(&identity).ok_or(QueryError::MissingComprehension)?.operation)
    }

    pub fn comprehension_producer_flow(&self, identity: ComprehensionIdentity) -> Result<NormalizedProducerFlowGraph, QueryError> {
        self.validate_owner()?;
        self.comprehension_scope(identity)?;
        self.producer_flow(self.solved.comprehension_operations.get(&identity).ok_or(QueryError::MissingComprehension)?.item_producer_flow)
    }

    fn comprehension_scope(&self, identity: ComprehensionIdentity) -> Result<Option<SchemeId>, QueryError> {
        let clause = self.solved.comprehension_operations.get(&identity).ok_or(QueryError::MissingComprehension)?;
        if !self.solved.expressions.contains_key(&identity.expression) { return Err(QueryError::MissingExpression); }
        Ok(self.solved.operation_scope(ProducerFlowSource::Comprehension(identity), &clause.operation)?)
    }

    fn operation(&self, source: ProducerFlowSource, fact: &crate::sema::check::SolvedOperation) -> Result<NormalizedOperation, QueryError> {
        let entries = fact.actual_arguments.len().checked_add(fact.argument_coercions.len()).and_then(|count| count.checked_add(fact.binding.supplied_slots.len())).and_then(|count| count.checked_add(fact.binding.default_slots.len())).ok_or(QueryError::Limit)?;
        if entries > self.limits.nodes { return Err(QueryError::Limit); }
        let declaration = fact.caller.map(|identity| self.solved.declarations.get(&identity).ok_or(QueryError::MissingDeclaration)).transpose()?;
        let scope = self.solved.operation_scope(source, fact)?;
        let mut view = View::new(self, scope);
        let enclosing_scheme = scope.map(|scope| {
            if let Some(declaration) = declaration.filter(|declaration| declaration.scheme == scope) {
                view.seed_profiles(declaration.parameter_producers.iter().chain(std::iter::once(&declaration.return_producers)))?;
            } else {
                let expression = match source {
                    ProducerFlowSource::Expression(identity) => Some(identity),
                    ProducerFlowSource::Comprehension(identity) => Some(identity.expression),
                    ProducerFlowSource::Stage(identity) => Some(identity.pipeline),
                    ProducerFlowSource::Statement(_) | ProducerFlowSource::Binding { .. } | ProducerFlowSource::Parameter { .. } | ProducerFlowSource::DeclarationResult(_) => None,
                };
                view.seed_profiles(expression.and_then(|identity| self.solved.expression_producers.get(&identity)))?;
            }
            view.scheme(self.solved.graph.scheme(scope)?.body, Some(scope))
        }).transpose()?;
        view.prepare_scope()?;
        let requirement = view.requirement(self.solved.graph.requirement_template(fact.requirement)?)?;
        let actual_arguments = fact.actual_arguments.iter().map(|&ty| { view.visit(0)?; view.graph(ty, 0) }).collect::<Result<Vec<_>, QueryError>>()?;
        let argument_coercions = fact.argument_coercions.iter().map(|&(slot, coercion)| {
            view.visit(0)?;
            Ok((slot, match coercion { crate::sema::check::RegistryArgumentCoercion::PathLikeToPath => NormalizedArgumentCoercion::PathLikeToPath }))
        }).collect::<Result<Vec<_>, QueryError>>()?;
        let binding = view.call_binding(&fact.binding)?;
        Ok(NormalizedOperation {
            owner: self.solved.owner,
            caller: fact.caller.map(|identity| view.declaration_identity(identity)).transpose()?,
            enclosing_scheme, requirement, result: view.graph(fact.result, 0)?, effects: view.effect(fact.effects)?,
            receiver: fact.receiver.map(|ty| view.graph(ty, 0)).transpose()?, actual_arguments, argument_coercions,
            binding,
        })
    }

    pub fn declaration_producer_flow(&self, identity: DeclarationIdentity) -> Result<NormalizedProducerFlowGraph, QueryError> {
        self.validate_owner()?;
        let declaration = self.solved.declarations.get(&identity).ok_or(QueryError::MissingDeclaration)?;
        self.producer_flow(declaration.return_producer_flow.ok_or(QueryError::MissingProducerFlow)?)
    }

    pub fn parameter_producer_flow(&self, identity: DeclarationIdentity, parameter: usize) -> Result<NormalizedProducerFlowGraph, QueryError> {
        self.validate_owner()?;
        let declaration = self.solved.declarations.get(&identity).ok_or(QueryError::MissingDeclaration)?;
        self.producer_flow(*declaration.parameter_producer_flows.get(parameter).ok_or(QueryError::MissingProducerFlow)?)
    }

    pub fn expression_producer_flow(&self, identity: ExpressionIdentity) -> Result<NormalizedProducerFlowGraph, QueryError> {
        self.validate_owner()?;
        if !self.solved.expressions.contains_key(&identity) { return Err(QueryError::MissingExpression); }
        self.producer_flow(*self.solved.expression_producer_flows.get(&identity).ok_or(QueryError::MissingProducerFlow)?)
    }

    pub fn statement_producer_flow(&self, identity: StatementIdentity) -> Result<NormalizedProducerFlowGraph, QueryError> {
        self.validate_owner()?;
        if !self.solved.statements.contains_key(&identity) { return Err(QueryError::MissingStatement); }
        self.producer_flow(*self.solved.statement_producer_flows.get(&identity).ok_or(QueryError::MissingProducerFlow)?)
    }

    pub fn binding_producer_flow(&self, identity: BindingIdentity, version: u32) -> Result<NormalizedProducerFlowGraph, QueryError> {
        self.validate_owner()?;
        if !self.solved.bindings.contains_key(&identity) { return Err(QueryError::MissingBinding); }
        self.producer_flow(*self.solved.binding_producer_flows.get(&(identity, version)).ok_or(QueryError::MissingProducerFlow)?)
    }

    pub fn stage_producer_flow(&self, identity: StageIdentity) -> Result<NormalizedProducerFlowGraph, QueryError> {
        self.validate_owner()?;
        self.stage_scope(identity)?;
        self.producer_flow(self.solved.stage_operations.get(&identity).ok_or(QueryError::MissingStage)?.result_producer_flow.ok_or(QueryError::MissingProducerFlow)?)
    }

    fn stage_scope(&self, identity: StageIdentity) -> Result<Option<SchemeId>, QueryError> {
        let stage = self.solved.stage_operations.get(&identity).ok_or(QueryError::MissingStage)?;
        if !self.solved.expressions.contains_key(&identity.pipeline) { return Err(QueryError::MissingExpression); }
        let parent = self.expression_scope(identity.pipeline)?;
        if parent.is_some() { return Ok(parent); }
        stage.operation.caller.map(|owner| self.declaration_scope(owner)).transpose()
    }

    fn flow_scope(&self, source: ProducerFlowSource) -> Result<Option<SchemeId>, QueryError> {
        Ok(match source {
            ProducerFlowSource::Expression(identity) => {
                if !self.solved.expressions.contains_key(&identity) { return Err(QueryError::MissingExpression); }
                self.expression_scope(identity)?
            }
            ProducerFlowSource::Stage(identity) => self.stage_scope(identity)?,
            ProducerFlowSource::Comprehension(identity) => self.comprehension_scope(identity)?,
            ProducerFlowSource::Statement(identity) => {
                if !self.solved.statements.contains_key(&identity) { return Err(QueryError::MissingStatement); }
                self.solved.statement_owners.get(&identity).map(|owner| self.declaration_scope(*owner)).transpose()?
            }
            ProducerFlowSource::Binding { identity, .. } => {
                let binding = self.solved.bindings.get(&identity).ok_or(QueryError::MissingBinding)?;
                binding.scheme.or(binding.owner.map(|owner| self.declaration_scope(owner)).transpose()?)
            }
            ProducerFlowSource::Parameter { declaration, .. } | ProducerFlowSource::DeclarationResult(declaration) => Some(self.declaration_scope(declaration)?),
        })
    }

    fn producer_flow(&self, root: ProducerFlowId) -> Result<NormalizedProducerFlowGraph, QueryError> {
        self.validate_owner()?;
        if self.solved.producer_flows.owner() != self.solved.owner || root.owner() != self.solved.owner { return Err(QueryError::ForeignGraph); }
        let mut identity_view = View::new(self, None);
        let mut references = rustc_hash::FxHashMap::default();
        let mut raw = Vec::new();
        let mut pending = vec![(root, 0usize)];
        while let Some((id, depth)) = pending.pop() {
            if depth > self.limits.depth { return Err(QueryError::Limit); }
            if references.contains_key(&id) { continue; }
            identity_view.visit(depth)?;
            let index = u32::try_from(raw.len()).map_err(|_| QueryError::Limit)?;
            references.insert(id, index);
            let node = self.solved.producer_flows.node(id)?;
            raw.push(node);
            let input_count = match &node.kind {
                ProducerFlowKind::CapturedBinding { .. } | ProducerFlowKind::Project { .. } => 1,
                ProducerFlowKind::Addition { .. } => 2,
                ProducerFlowKind::Operation { alternatives, .. } => alternatives.iter().try_fold(0usize, |count, alternative| count.checked_add(alternative.transfers.len()).ok_or(QueryError::Limit))?,
                ProducerFlowKind::Aggregate { entries } => entries.len(),
                ProducerFlowKind::RecordUpdate { replacements, .. } => replacements.len().checked_add(1).ok_or(QueryError::Limit)?,
                ProducerFlowKind::Join { inputs } => inputs.len(),
                ProducerFlowKind::Apply { arguments, .. } | ProducerFlowKind::StageApply { arguments, .. } => arguments.len().checked_add(1).ok_or(QueryError::Limit)?,
                _ => 0,
            };
            if input_count > self.limits.nodes.saturating_sub(pending.len()) { return Err(QueryError::Limit); }
            for _ in 0..input_count { identity_view.visit(depth + 1)?; }
            let inputs = match &node.kind {
                ProducerFlowKind::CapturedBinding { input, .. } | ProducerFlowKind::Project { input, .. } => vec![*input],
                ProducerFlowKind::Addition { left, right, .. } => vec![*left, *right],
                ProducerFlowKind::Aggregate { entries } => {
                    let mut fields = entries.iter().map(|entry| Ok((identity_view.producer_path(&entry.path)?, entry.input))).collect::<Result<Vec<_>, QueryError>>()?;
                    fields.sort_by(|left, right| left.0.cmp(&right.0));
                    // Multiple list or map entries can carry producers at the
                    // same path. Stable sorting retains their source order.
                    fields.into_iter().map(|(_, input)| input).collect()
                }
                ProducerFlowKind::RecordUpdate { base, replacements } => {
                    let mut inputs = Vec::with_capacity(replacements.len() + 1);
                    inputs.push(*base);
                    for replacement in replacements {
                        identity_view.producer_path(&replacement.path)?;
                        inputs.push(replacement.input);
                    }
                    inputs
                }
                ProducerFlowKind::Join { inputs } => inputs.clone(),
                ProducerFlowKind::Operation { alternatives, .. } => identity_view.operation_transfer_inputs(alternatives)?,
                ProducerFlowKind::Apply { callee, arguments, .. } | ProducerFlowKind::StageApply { callee, arguments, .. } => std::iter::once(*callee).chain(arguments.iter().copied()).collect(),
                ProducerFlowKind::Empty | ProducerFlowKind::Opaque | ProducerFlowKind::Known(_) | ProducerFlowKind::Parameter { .. } | ProducerFlowKind::Callable { .. } | ProducerFlowKind::NativeCallable { .. } => Vec::new(),
            };
            if pending.len() + inputs.len() > self.limits.nodes { return Err(QueryError::Limit); }
            pending.extend(inputs.into_iter().rev().map(|input| (input, depth + 1)));
        }
        let mut group_keys = Vec::new();
        let mut group_lookup = std::collections::BTreeMap::new();
        let mut groups = Vec::<View<'_, '_>>::new();
        let mut public_scopes = Vec::new();
        let mut node_groups = Vec::new();
        let mut scope_count = 0;
        for node in &raw {
            let scope = self.flow_scope(node.source)?;
            let group = if let Some(&group) = group_lookup.get(&scope) { group }
            else {
                let mut view = View::new(self, scope);
                if let Some(scope) = scope {
                    if let Some(declaration) = self.solved.declarations.values().find(|declaration| declaration.scheme == scope) {
                        view.seed_profiles(declaration.parameter_producers.iter().chain(std::iter::once(&declaration.return_producers)))?;
                    }
                    public_scopes.push(Some(scope_count)); scope_count += 1;
                } else { public_scopes.push(None); }
                groups.push(view); group_keys.push(scope);
                let group = groups.len() - 1; group_lookup.insert(scope, group); group
            };
            match &node.kind {
                ProducerFlowKind::Known(profile) => groups[group].seed_profiles(std::iter::once(profile))?,
                ProducerFlowKind::Operation { outputs, .. } => {
                    if groups[group].producer_roots.len() + 2 > self.limits.nodes { return Err(QueryError::Limit); }
                    groups[group].producer_roots.extend([outputs.pull, outputs.close]);
                }
                _ => {}
            }
            node_groups.push(group);
            flow_budget(&identity_view, &groups, self.limits)?;
        }
        let mut scopes = Vec::new();
        for (view, scope) in groups.iter_mut().zip(&group_keys) {
            if let Some(scope) = scope {
                scopes.push(view.scheme(self.solved.graph.scheme(*scope)?.body, Some(*scope))?);
            }
        }
        flow_budget(&identity_view, &groups, self.limits)?;
        let mut nodes = Vec::with_capacity(raw.len());
        for (node, &group) in raw.iter().zip(&node_groups) {
            let view = &mut groups[group];
            view.visit(0)?;
            let source = view.flow_source(node.source)?;
            let reference = |id: ProducerFlowId| references.get(&id).copied().ok_or(QueryError::ForeignGraph);
            let application_inputs = |callee: ProducerFlowId, arguments: &[ProducerFlowId]| -> Result<(u32, Vec<u32>), QueryError> {
                Ok((reference(callee)?, arguments.iter().copied().map(reference).collect::<Result<Vec<_>, QueryError>>()?))
            };
            let kind = match &node.kind {
                ProducerFlowKind::Empty => NormalizedProducerFlowKind::Empty,
                ProducerFlowKind::Opaque => NormalizedProducerFlowKind::Opaque,
                ProducerFlowKind::NativeCallable { authority } => NormalizedProducerFlowKind::NativeCallable { authority: view.native_authority(*authority, 0)? },
                ProducerFlowKind::Known(profile) => NormalizedProducerFlowKind::Known(view.profile(profile)?),
                ProducerFlowKind::Addition { requirement, left, right } => {
                    let template = self.solved.graph.requirement_template(*requirement)?;
                    if !matches!(template, RequirementTemplate::Add { .. }) { return Err(QueryError::Recovery); }
                    NormalizedProducerFlowKind::Addition { requirement: Box::new(view.requirement(template)?), left: reference(*left)?, right: reference(*right)? }
                }
                ProducerFlowKind::Operation { requirement, alternatives, outputs } => {
                    let template = self.solved.graph.requirement_template(*requirement)?;
                    let RequirementTemplate::Operation { family, .. } = template else { return Err(QueryError::Recovery); };
                    let family = self.solved.graph.family(family)?;
                    let mut alternatives = alternatives.iter().map(|alternative| {
                        view.visit(0)?;
                        if !family.contains(&alternative.candidate) { return Err(QueryError::Recovery); }
                        let candidate = self.solved.graph.candidate(alternative.candidate)?;
                        let mut transfers = alternative.transfers.iter().map(|transfer| {
                            view.visit(0)?;
                            Ok(NormalizedProducerFlowOperationTransfer { input: reference(transfer.input)?, input_path: view.producer_path(&transfer.input_path)?, output_path: view.producer_path(&transfer.output_path)? })
                        }).collect::<Result<Vec<_>, QueryError>>()?;
                        transfers.sort_by(|left, right| left.output_path.cmp(&right.output_path).then_with(|| left.input_path.cmp(&right.input_path)).then_with(|| left.input.cmp(&right.input)));
                        Ok(NormalizedProducerFlowOperationAlternative { identity: view.name(candidate.identity)?, public_label: view.name(candidate.public_label)?, path: alternative.path.as_ref().map(|path| view.producer_path(path)).transpose()?, transfers, opaque: alternative.opaque })
                    }).collect::<Result<Vec<_>, QueryError>>()?;
                    alternatives.sort_by(|left, right| left.identity.cmp(&right.identity).then_with(|| left.path.cmp(&right.path)));
                    if alternatives.windows(2).any(|pair| pair[0].identity == pair[1].identity) { return Err(QueryError::Recovery); }
                    NormalizedProducerFlowKind::Operation { requirement: Box::new(view.requirement(template)?), alternatives, outputs: NormalizedProducerEffects { pull: view.effect(outputs.pull)?, close: view.effect(outputs.close)? } }
                },
                ProducerFlowKind::Parameter { declaration, index } => NormalizedProducerFlowKind::Parameter { declaration: view.declaration_identity(*declaration)?, index: *index },
                ProducerFlowKind::CapturedBinding { identity, version, input } => NormalizedProducerFlowKind::CapturedBinding { identity: view.binding_identity(*identity)?, version: *version, input: reference(*input)? },
                ProducerFlowKind::Project { input, path } => NormalizedProducerFlowKind::Project { input: reference(*input)?, path: view.producer_path(path)? },
                ProducerFlowKind::Aggregate { entries } => {
                    let mut entries = entries.iter().map(|entry| Ok(NormalizedProducerFlowField { path: view.producer_path(&entry.path)?, input: reference(entry.input)? })).collect::<Result<Vec<_>, QueryError>>()?;
                    entries.sort_by(|left, right| left.path.cmp(&right.path));
                    NormalizedProducerFlowKind::Aggregate { entries }
                }
                ProducerFlowKind::RecordUpdate { base, replacements } => {
                    let replacements = replacements.iter().map(|replacement| Ok(NormalizedProducerFlowField { path: view.producer_path(&replacement.path)?, input: reference(replacement.input)? })).collect::<Result<Vec<_>, QueryError>>()?;
                    NormalizedProducerFlowKind::RecordUpdate { base: reference(*base)?, replacements }
                }
                ProducerFlowKind::Join { inputs } => NormalizedProducerFlowKind::Join { inputs: inputs.iter().copied().map(reference).collect::<Result<Vec<_>, QueryError>>()? },
                ProducerFlowKind::Callable { declaration } => NormalizedProducerFlowKind::Callable { declaration: view.declaration_identity(*declaration)? },
                ProducerFlowKind::Apply { call, callee, arguments } => {
                    let (callee, arguments) = application_inputs(*callee, arguments)?;
                    NormalizedProducerFlowKind::Apply { call: view.expression_identity(*call)?, callee, arguments }
                }
                ProducerFlowKind::StageApply { stage, callee, arguments } => {
                    let (callee, arguments) = application_inputs(*callee, arguments)?;
                    NormalizedProducerFlowKind::StageApply { stage: view.stage_identity(*stage)?, callee, arguments }
                }
            };
            nodes.push(NormalizedProducerFlowNode { source, scope: public_scopes[group], kind });
            flow_budget(&identity_view, &groups, self.limits)?;
        }
        Ok(NormalizedProducerFlowGraph { owner: self.solved.owner, root: 0, scopes, nodes })
    }

    pub fn expression_producers(&self, identity: ExpressionIdentity) -> Result<NormalizedProducerProfile, QueryError> {
        self.validate_owner()?;
        if !self.solved.expressions.contains_key(&identity) { return Err(QueryError::MissingExpression); }
        let profile = self.solved.expression_producers.get(&identity).ok_or(QueryError::MissingProducerProfile)?;
        let mut view = View::new(self, self.expression_scope(identity)?);
        view.seed_profiles(std::iter::once(profile))?;
        view.profile(profile)
    }

    /// A graph handle is meaningful only together with this query's owner.
    pub fn type_view(&self, ty: &Type, scope: Option<DeclarationIdentity>) -> Result<NormalizedType, QueryError> {
        self.validate_owner()?;
        let scope = scope.map(|identity| self.declaration_scope(identity)).transpose()?;
        let mut view = View::new(self, scope);
        if let Type::Graph(id) = ty { return view.ty(*id); }
        let shape = view.tree(ty, 0)?;
        let annotation = tree_annotation(ty, self.solved);
        Ok(NormalizedType { shape, annotation })
    }

    fn declaration_scope(&self, identity: DeclarationIdentity) -> Result<SchemeId, QueryError> {
        self.solved.declarations.get(&identity).map(|fact| fact.scheme).ok_or(QueryError::MissingDeclaration)
    }

    fn validate_owner(&self) -> Result<(), QueryError> {
        if self.solved.owner != self.solved.graph.owner() { return Err(QueryError::ForeignGraph); }
        if !self.symbols.shares_storage_with(self.solved.symbol_owner()) { return Err(QueryError::ForeignSymbol); }
        Ok(())
    }

    fn expression_scope(&self, identity: ExpressionIdentity) -> Result<Option<SchemeId>, QueryError> {
        if let Some(&scheme) = self.solved.expression_schemes.get(&identity) { return Ok(Some(scheme)); }
        if let Some(&scheme) = self.solved.expression_value_scopes.get(&identity) { return Ok(Some(scheme)); }
        self.solved.expression_owners.get(&identity).copied().map(|owner| self.declaration_scope(owner)).transpose()
    }
}

/// Registry templates have named builtin parameters and no source graph owner.
/// Source handles must use a SolvedQuery instead of entering this boundary.
pub fn registry_type(ty: &Type) -> Result<NormalizedType, QueryError> {
    static EMPTY: std::sync::LazyLock<SolvedTypes> = std::sync::LazyLock::new(|| {
        let graph = crate::sema::inference::InferenceContext::default().freeze_scoped(&[]).expect("empty registry graph is solved");
        SolvedTypes::from_graph(graph, SymbolOwner::new())
    });
    let query = SolvedQuery::new(&EMPTY, EMPTY.symbol_owner());
    let mut view = View::new(&query, None);
    view.registry = true;
    let shape = view.tree(ty, 0)?;
    Ok(NormalizedType { shape, annotation: closed_annotation(ty) })
}

fn flow_budget(identity: &View<'_, '_>, groups: &[View<'_, '_>], limits: QueryLimits) -> Result<(), QueryError> {
    let mut nodes = identity.nodes;
    let mut bytes = identity.text_bytes.get();
    for group in groups {
        nodes = nodes.checked_add(group.nodes).ok_or(QueryError::Limit)?;
        bytes = bytes.checked_add(group.text_bytes.get()).ok_or(QueryError::Limit)?;
    }
    if nodes > limits.nodes || bytes > limits.text_bytes { return Err(QueryError::Limit); }
    Ok(())
}

struct View<'a, 'q> {
    query: &'a SolvedQuery<'q>, scope: Option<SchemeId>, nodes: usize,
    text_bytes: std::cell::Cell<usize>, registry: bool, scope_ready: bool,
    type_binders: std::collections::BTreeMap<TypeId, u32>,
    effect_binders: rustc_hash::FxHashMap<EffectSummary, u32>,
    type_order: Vec<usize>, effect_order: Vec<usize>, producer_roots: Vec<EffectSummary>,
    native_contracts: rustc_hash::FxHashMap<crate::sema::inference::NativeContractId, std::sync::Arc<NormalizedNativeContract>>,
    active_native_contracts: rustc_hash::FxHashSet<crate::sema::inference::NativeContractId>,
    native_families: rustc_hash::FxHashMap<crate::sema::inference::NativeFamilyContractId, std::sync::Arc<NormalizedNativeFamilyContract>>,
    active_native_families: rustc_hash::FxHashSet<crate::sema::inference::NativeFamilyContractId>,
}

impl<'a, 'q> View<'a, 'q> {
    fn new(query: &'a SolvedQuery<'q>, scope: Option<SchemeId>) -> Self {
        Self { query, scope, nodes: 0, text_bytes: std::cell::Cell::new(0), registry: false, scope_ready: false,
            type_binders: std::collections::BTreeMap::new(), effect_binders: rustc_hash::FxHashMap::default(), type_order: Vec::new(), effect_order: Vec::new(), producer_roots: Vec::new(), native_contracts: rustc_hash::FxHashMap::default(), active_native_contracts: rustc_hash::FxHashSet::default(), native_families: rustc_hash::FxHashMap::default(), active_native_families: rustc_hash::FxHashSet::default() }
    }
    fn call_binding(&mut self, binding: &crate::sema::check::CallBinding) -> Result<NormalizedCallBinding, QueryError> {
        self.binding_parts(&binding.supplied_slots, &binding.default_slots, binding.rest_slot, binding.dynamic.as_ref())
    }
    fn binding_parts(&mut self, supplied_slots: &[usize], default_slots: &[usize], rest_slot: Option<usize>, dynamic: Option<&crate::sema::inference::DynamicInvocationBinding>) -> Result<NormalizedCallBinding, QueryError> {
        self.query.validate_owner()?;
        let remaining_nodes = self.query.limits.nodes.checked_sub(self.nodes).ok_or(QueryError::Limit)?;
        let remaining_bytes = self.query.limits.text_bytes.checked_sub(self.text_bytes.get()).ok_or(QueryError::Limit)?;
        let limit = remaining_nodes.min(remaining_bytes / 64);
        let mut entries = 0usize;
        let mut charge = |count: usize| -> Result<(), QueryError> {
            entries = entries.checked_add(count).ok_or(QueryError::Limit)?;
            if entries > limit { return Err(QueryError::Limit); }
            Ok(())
        };
        charge(1)?;
        charge(supplied_slots.len())?;
        charge(default_slots.len())?;
        charge(usize::from(rest_slot.is_some()))?;
        if let Some(dynamic) = dynamic {
            if self.query.limits.depth < 1 { return Err(QueryError::Limit); }
            charge(3)?;
            charge(dynamic.conditional_default_slots.len())?;
            charge(dynamic.required_slots.len())?;
            charge(dynamic.segments.len())?;
            for segment in &dynamic.segments {
                if self.query.limits.depth < 2 { return Err(QueryError::Limit); }
                match segment {
                    crate::sema::inference::InvocationArgumentSegment::StaticSlot { .. } => charge(2)?,
                    crate::sema::inference::InvocationArgumentSegment::DynamicRange { fixed_slots, rest_slot, .. } => {
                        charge(1)?;
                        charge(fixed_slots.len())?;
                        charge(usize::from(rest_slot.is_some()))?;
                    }
                }
            }
        }
        // Preflight every nested vector before allocation. Empty supplied slots
        // in a dynamic plan never exempt its segments or guards from the budget.
        self.nodes = self.nodes.checked_add(entries).ok_or(QueryError::Limit)?;
        self.text(entries.checked_mul(64).ok_or(QueryError::Limit)?)?;
        let dynamic = dynamic.map(|dynamic| NormalizedDynamicInvocationBinding {
            segments: dynamic.segments.iter().map(|segment| match segment {
                crate::sema::inference::InvocationArgumentSegment::StaticSlot { argument, slot } => NormalizedInvocationArgumentSegment::StaticSlot { argument: *argument, slot: *slot },
                crate::sema::inference::InvocationArgumentSegment::DynamicRange { argument, fixed_slots, rest_slot } => NormalizedInvocationArgumentSegment::DynamicRange { argument: *argument, fixed_slots: fixed_slots.clone(), rest_slot: *rest_slot },
            }).collect(),
            conditional_default_slots: dynamic.conditional_default_slots.to_vec(),
            required_slots: dynamic.required_slots.clone(),
            runtime_arity_guard: dynamic.runtime_arity_guard,
            runtime_duplicate_guard: dynamic.runtime_duplicate_guard,
        });
        Ok(NormalizedCallBinding { supplied_slots: supplied_slots.to_vec(), default_slots: default_slots.to_vec(), rest_slot: rest_slot, dynamic })
    }

    fn declaration_identity(&self, identity: DeclarationIdentity) -> Result<NormalizedDeclarationIdentity, QueryError> {
        Ok(NormalizedDeclarationIdentity { source: identity.source, namespace: identity.namespace.map(|name| self.name(name)).transpose()?, declaration: identity.declaration })
    }
    fn binding_identity(&self, identity: BindingIdentity) -> Result<NormalizedBindingIdentity, QueryError> {
        Ok(NormalizedBindingIdentity { source: identity.source, namespace: identity.namespace.map(|name| self.name(name)).transpose()?, target: identity.target })
    }
    fn expression_identity(&self, identity: ExpressionIdentity) -> Result<NormalizedExpressionIdentity, QueryError> {
        Ok(NormalizedExpressionIdentity { source: identity.source, namespace: identity.namespace.map(|name| self.name(name)).transpose()?, expression: identity.expression })
    }
    fn stage_identity(&self, identity: StageIdentity) -> Result<NormalizedStageIdentity, QueryError> {
        self.query.stage_scope(identity)?;
        Ok(NormalizedStageIdentity { pipeline: self.expression_identity(identity.pipeline)?, index: identity.index })
    }
    fn comprehension_identity(&self, identity: ComprehensionIdentity) -> Result<NormalizedComprehensionIdentity, QueryError> {
        self.query.comprehension_scope(identity)?;
        Ok(NormalizedComprehensionIdentity { expression: self.expression_identity(identity.expression)?, qualifier: identity.qualifier })
    }
    fn flow_source(&self, source: ProducerFlowSource) -> Result<NormalizedProducerFlowSource, QueryError> {
        Ok(match source {
            ProducerFlowSource::Expression(identity) => NormalizedProducerFlowSource::Expression(self.expression_identity(identity)?),
            ProducerFlowSource::Stage(identity) => NormalizedProducerFlowSource::Stage(self.stage_identity(identity)?),
            ProducerFlowSource::Comprehension(identity) => NormalizedProducerFlowSource::Comprehension(self.comprehension_identity(identity)?),
            ProducerFlowSource::Statement(identity) => NormalizedProducerFlowSource::Statement(NormalizedStatementIdentity { source: identity.source, namespace: identity.namespace.map(|name| self.name(name)).transpose()?, statement: identity.statement }),
            ProducerFlowSource::Binding { identity, version } => NormalizedProducerFlowSource::Binding { identity: self.binding_identity(identity)?, version },
            ProducerFlowSource::Parameter { declaration, index } => NormalizedProducerFlowSource::Parameter { declaration: self.declaration_identity(declaration)?, index },
            ProducerFlowSource::DeclarationResult(declaration) => NormalizedProducerFlowSource::DeclarationResult(self.declaration_identity(declaration)?),
        })
    }
    fn operation_transfer_inputs(&mut self, alternatives: &[crate::sema::check::ProducerFlowOperationAlternative]) -> Result<Vec<ProducerFlowId>, QueryError> {
        let mut ordered = alternatives.iter().map(|alternative| {
            self.visit(0)?;
            Ok((self.name(self.query.solved.graph.candidate(alternative.candidate)?.identity)?, alternative))
        }).collect::<Result<Vec<_>, QueryError>>()?;
        ordered.sort_by(|left, right| left.0.cmp(&right.0));
        let mut inputs = Vec::new();
        for (_, alternative) in ordered {
            let mut transfers = alternative.transfers.iter().map(|transfer| {
                self.visit(0)?;
                let source = self.query.solved.producer_flows.node(transfer.input)?.source;
                Ok((self.producer_path(&transfer.output_path)?, self.producer_path(&transfer.input_path)?, self.flow_source(source)?, transfer.input))
            }).collect::<Result<Vec<_>, QueryError>>()?;
            transfers.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)).then_with(|| left.2.cmp(&right.2)));
            inputs.extend(transfers.into_iter().map(|(_, _, _, input)| input));
        }
        Ok(inputs)
    }
    fn producer_path(&mut self, path: &ProducerPath) -> Result<NormalizedProducerPath, QueryError> {
        if path.0.len() > self.query.limits.depth { return Err(QueryError::Limit); }
        let mut components = Vec::with_capacity(path.0.len());
        for component in &path.0 {
            self.visit(0)?;
            components.push(match component {
                ProducerPathComponent::RecordField(name) => NormalizedProducerPathComponent::RecordField(self.name(*name)?),
                ProducerPathComponent::ListItem => NormalizedProducerPathComponent::ListItem,
                ProducerPathComponent::MapKey => NormalizedProducerPathComponent::MapKey,
                ProducerPathComponent::MapValue => NormalizedProducerPathComponent::MapValue,
                ProducerPathComponent::OptionalPayload => NormalizedProducerPathComponent::OptionalPayload,
                ProducerPathComponent::ResultSuccess => NormalizedProducerPathComponent::ResultSuccess,
                ProducerPathComponent::ResultError => NormalizedProducerPathComponent::ResultError,
                ProducerPathComponent::CallableParameter(index) => NormalizedProducerPathComponent::CallableParameter(*index),
                ProducerPathComponent::CallableResult => NormalizedProducerPathComponent::CallableResult,
            });
        }
        Ok(NormalizedProducerPath(components))
    }
    fn ordered_profile(&mut self, profile: &ProducerProfile) -> Result<std::collections::BTreeMap<NormalizedProducerPath, ProducerEffects>, QueryError> {
        let mut ordered = std::collections::BTreeMap::new();
        for (path, &effects) in profile {
            self.visit(0)?;
            if ordered.insert(self.producer_path(path)?, effects).is_some() { return Err(QueryError::Recovery); }
        }
        Ok(ordered)
    }
    fn seed_profiles<'p>(&mut self, profiles: impl IntoIterator<Item = &'p ProducerProfile>) -> Result<(), QueryError> {
        for profile in profiles {
            for effects in self.ordered_profile(profile)?.values() {
                if self.producer_roots.len() + 2 > self.query.limits.nodes { return Err(QueryError::Limit); }
                self.producer_roots.extend([effects.pull, effects.close]);
            }
        }
        Ok(())
    }
    fn profile(&mut self, profile: &ProducerProfile) -> Result<NormalizedProducerProfile, QueryError> {
        self.prepare_scope()?;
        self.ordered_profile(profile)?.into_iter().map(|(path, effects)| Ok((path, NormalizedProducerEffects { pull: self.effect(effects.pull)?, close: self.effect(effects.close)? }))).collect()
    }
    fn prepare_scope(&mut self) -> Result<(), QueryError> {
        if self.scope_ready { return Ok(()); }
        let Some(scope) = self.scope else { self.scope_ready = true; return Ok(()); };
        let graph = &self.query.solved.graph;
        let scheme = graph.scheme(scope)?;
        if scheme.binders.len() + scheme.effect_binders.len() > self.query.limits.nodes { return Err(QueryError::Limit); }
        let original_types = scheme.binders.iter().enumerate().map(|(index, &ty)| Ok((graph.resolved(ty)?, index))).collect::<Result<std::collections::BTreeMap<_, _>, QueryError>>()?;
        let original_effects = scheme.effect_binders.iter().enumerate().map(|(index, &summary)| Ok((graph.resolved_effect_summary(summary)?, index))).collect::<Result<rustc_hash::FxHashMap<_, _>, QueryError>>()?;
        enum Part { Type(TypeId, usize), Effect(EffectSummary) }
        let mut roots = vec![Part::Type(scheme.body, 0)];
        roots.extend(self.producer_roots.iter().copied().map(Part::Effect));
        for requirement in &scheme.requirements {
            match *requirement {
                RequirementTemplate::Add { left, right, result } => roots.extend([Part::Type(left, 0), Part::Type(right, 0), Part::Type(result, 0)]),
                RequirementTemplate::EqualityCompatible { left, right } => roots.extend([Part::Type(left, 0), Part::Type(right, 0)]),
                RequirementTemplate::EffectInclusion { actual, expected, .. } => roots.extend([Part::Effect(actual), Part::Effect(expected)]),
                RequirementTemplate::Eligibility { ty, .. } => roots.push(Part::Type(ty, 0)),
                RequirementTemplate::CallableInvocation { call } => {
                    let call = graph.invocation_call(call)?;
                    roots.push(Part::Type(call.callable, 0));
                    roots.extend(call.arguments.iter().map(|argument| Part::Type(argument.ty, 0)));
                    roots.push(Part::Type(call.result, 0));
                    roots.push(Part::Effect(call.effects));
                }
                RequirementTemplate::Operation { call, .. } => {
                    let call = graph.operation_call(call)?;
                    if let crate::sema::inference::OperationBinding::Invocation(invocation) = call.binding {
                        let invocation = graph.invocation_call(invocation)?;
                        if invocation.arguments.len() > self.query.limits.nodes.saturating_sub(roots.len()) { return Err(QueryError::Limit); }
                        roots.push(Part::Type(invocation.callable, 0));
                        roots.extend(invocation.arguments.iter().map(|argument| Part::Type(argument.ty, 0)));
                        roots.push(Part::Type(invocation.result, 0)); roots.push(Part::Effect(invocation.effects));
                    }
                    if let Some(authority) = call.mono_authority {
                        roots.push(Part::Type(graph.native_authority_signature(authority)?, 0));
                        let members: &[crate::sema::inference::NativeContractId] = match &authority { crate::sema::inference::NativeAuthority::Single(id) => std::slice::from_ref(id), crate::sema::inference::NativeAuthority::Family(id) => &graph.native_family_contract(*id)?.members };
                        if members.len() > self.query.limits.nodes.saturating_sub(roots.len()) { return Err(QueryError::Limit); }
                        let mut members = members.iter().map(|&id| Ok((self.name(graph.candidate(graph.native_contract(id)?.candidate)?.identity)?, id))).collect::<Result<Vec<_>, QueryError>>()?;
                        members.sort_by(|left, right| left.0.cmp(&right.0));
                        for (_, member) in members {
                            let instance = &graph.native_contract(member)?.instance;
                            let count = instance.substitutions.len().checked_add(instance.effect_substitutions.len()).and_then(|count| count.checked_add(instance.effect_roots.len())).and_then(|count| count.checked_add(1)).ok_or(QueryError::Limit)?;
                            if count > self.query.limits.nodes.saturating_sub(roots.len()) { return Err(QueryError::Limit); }
                            roots.push(Part::Type(instance.ty, 0));
                            roots.extend(instance.substitutions.iter().copied().map(|ty| Part::Type(ty, 0)));
                            roots.extend(instance.effect_substitutions.iter().map(|&effect| Part::Effect(EffectSummary::Variable(effect))));
                            roots.extend(instance.effect_roots.iter().copied().map(Part::Effect));
                        }
                    }
                    roots.extend(call.receiver.iter().chain(call.arguments.iter().flatten()).copied().map(|ty| Part::Type(ty, 0)));
                    roots.push(Part::Type(call.result, 0));
                    roots.extend(call.declared_error_bound.into_iter().map(|ty| Part::Type(ty, 0)));
                    roots.push(Part::Effect(call.effects));
                    let mut roles = call.effect_bindings.clone();
                    roles.sort_by_key(|(role, _)| *role);
                    if roles.windows(2).any(|pair| pair[0].0 == pair[1].0) { return Err(QueryError::Recovery); }
                    roots.extend(roles.into_iter().map(|(_, summary)| Part::Effect(summary)));
                    let mut outputs = call.output_effect_bindings.clone();
                    outputs.sort_by_key(|(role, _)| *role);
                    if outputs.windows(2).any(|pair| pair[0].0 == pair[1].0) { return Err(QueryError::Recovery); }
                    roots.extend(outputs.into_iter().map(|(_, summary)| Part::Effect(summary)));
                }
            }
        }
        roots.extend(scheme.effect_roots.iter().copied().map(Part::Effect));
        if roots.len() > self.query.limits.nodes { return Err(QueryError::Limit); }
        let mut pending = roots.into_iter().rev().collect::<Vec<_>>();
        let mut seen = rustc_hash::FxHashSet::default();
        while let Some(part) = pending.pop() {
            match part {
                Part::Effect(summary) => {
                    let summary = graph.resolved_effect_summary(summary)?;
                    if let Some(&original) = original_effects.get(&summary) {
                        if !self.effect_binders.contains_key(&summary) {
                            self.effect_binders.insert(summary, self.effect_order.len() as u32);
                            self.effect_order.push(original);
                        }
                    }
                }
                Part::Type(ty, depth) => {
                    if depth > self.query.limits.depth { return Err(QueryError::Limit); }
                    let ty = graph.resolved(ty)?;
                    if !seen.insert(ty) { continue; }
                    if seen.len() > self.query.limits.nodes { return Err(QueryError::Limit); }
                    if let Some(&original) = original_types.get(&ty) {
                        self.type_binders.insert(ty, self.type_order.len() as u32);
                        self.type_order.push(original);
                    }
                    let mut children = Vec::new();
                    match graph.node(ty)? {
                        TypeNode::CallableChoice(signatures) => {
                            if signatures.len() > self.query.limits.nodes.saturating_sub(self.nodes) { return Err(QueryError::Limit); }
                            children.extend(signatures.iter().copied());
                        }
                        TypeNode::FiniteDomain(alternatives) => {
                            let mut ordered = alternatives.iter().map(|alternative| Ok((self.domain_key(alternative.ty, depth + 1)?, argument_relation(alternative.relation), alternative.ty))).collect::<Result<Vec<_>, QueryError>>()?;
                            ordered.sort_by(|left, right| (&left.0, &left.1).cmp(&(&right.0, &right.1)));
                            children.extend(ordered.into_iter().map(|(_, _, ty)| ty));
                        }
                        TypeNode::NativeCallable(callable) => {
                            children.push(callable.signature);
                            let mut alternatives = Vec::new();
                            for authority in &callable.alternatives { match authority {
                                crate::sema::inference::CallableAuthority::User { signature } => alternatives.push((String::new(), *signature, None)),
                                crate::sema::inference::CallableAuthority::Native { authority } => {
                                    self.visit(depth + 1)?;
                                    let members: &[crate::sema::inference::NativeContractId] = match authority { crate::sema::inference::NativeAuthority::Single(id) => std::slice::from_ref(id), crate::sema::inference::NativeAuthority::Family(id) => { let family = graph.native_family_contract(*id)?; children.push(family.signature); &family.members } };
                                    if members.len() > self.query.limits.nodes.saturating_sub(self.nodes) { return Err(QueryError::Limit); }
                                    for &id in members { self.visit(depth + 1)?; let contract = graph.native_contract(id)?; alternatives.push((self.name(graph.candidate(contract.candidate)?.identity)?, contract.instance.ty, Some(contract))); }
                                }
                            } }
                            alternatives.sort_by(|left, right| left.0.cmp(&right.0));
                            for (_, signature, contract) in alternatives {
                                children.push(signature);
                                if let Some(contract) = contract {
                                    children.extend(contract.instance.substitutions.iter().copied());
                                    pending.extend(contract.instance.effect_substitutions.iter().map(|&effect| Part::Effect(EffectSummary::Variable(effect))));
                                    pending.extend(contract.instance.effect_roots.iter().copied().map(Part::Effect));
                                }
                            }
                        }
                        TypeNode::Arrow(arrow) => {
                            pending.push(Part::Effect(arrow.effects));
                            children.extend(arrow.params.iter().map(|parameter| parameter.ty));
                            children.push(arrow.result);
                        }
                        TypeNode::Optional(item) | TypeNode::List(item) | TypeNode::Stream(item) => children.push(*item),
                        TypeNode::Map(key, value) | TypeNode::Result(key, value) => { children.push(*key); children.push(*value); }
                        TypeNode::Record(row) | TypeNode::Row(row) => {
                            let mut row = *row;
                            let mut row_depth = depth;
                            let mut fields = Vec::new();
                            let tail = loop {
                                if row_depth > self.query.limits.depth { return Err(QueryError::Limit); }
                                let data = graph.row_data(row)?;
                                if fields.len() + data.fields.len() > self.query.limits.nodes { return Err(QueryError::Limit); }
                                fields.extend(data.fields.iter().map(|field| Ok((self.name(field.label)?, field.ty))).collect::<Result<Vec<_>, QueryError>>()?);
                                let Some(tail) = data.tail else { break None; };
                                let tail = graph.resolved(tail)?;
                                if let TypeNode::Row(next) = graph.node(tail)? { row = *next; row_depth += 1; }
                                else { break Some(tail); }
                            };
                            fields.sort_by(|left, right| left.0.cmp(&right.0));
                            children.extend(fields.into_iter().map(|(_, ty)| ty));
                            children.extend(tail);
                        }
                        TypeNode::Module(fields) => {
                            let mut fields = fields.iter().map(|field| Ok((self.name(field.label)?, field.ty))).collect::<Result<Vec<_>, QueryError>>()?;
                            fields.sort_by(|left, right| left.0.cmp(&right.0));
                            children.extend(fields.into_iter().map(|(_, ty)| ty));
                        }
                        _ => {}
                    }
                    pending.extend(children.into_iter().rev().map(|child| Part::Type(child, depth + 1)));
                }
            }
        }
        let inclusions = scheme.effect_inclusions.iter().map(|&(actual, expected)| Ok((graph.resolved_effect_summary(actual)?, graph.resolved_effect_summary(expected)?))).collect::<Result<Vec<_>, QueryError>>()?;
        // Inclusion-only binders follow their already named signature or latent
        // role roots before unused storage ordinals can affect the public view.
        let mut anchor = 0;
        while anchor < self.effect_order.len() {
            let summary = graph.resolved_effect_summary(scheme.effect_binders[self.effect_order[anchor]])?;
            let mut neighbors = inclusions.iter().filter_map(|&(actual, expected)| {
                if actual == summary { Some(expected) } else if expected == summary { Some(actual) } else { None }
            }).filter_map(|summary| original_effects.get(&summary).copied().map(|original| (summary, original))).collect::<Vec<_>>();
            neighbors.sort_by_key(|(_, original)| {
                let quantifier = &scheme.effect_quantifiers[*original];
                (effects(quantifier.lower), quantifier.upper.map(effects), quantifier.derived)
            });
            for (summary, original) in neighbors {
                if !self.effect_binders.contains_key(&summary) {
                    self.effect_binders.insert(summary, self.effect_order.len() as u32);
                    self.effect_order.push(original);
                }
            }
            anchor += 1;
        }
        // Signature, producer, and requirement relationships receive ordinals
        // first. Remaining scope-owned binders retain their stored order without
        // acquiring relationships from later caller values.
        for (index, &ty) in scheme.binders.iter().enumerate() {
            let ty = graph.resolved(ty)?;
            if !self.type_binders.contains_key(&ty) { self.type_binders.insert(ty, self.type_order.len() as u32); self.type_order.push(index); }
        }
        for (index, &summary) in scheme.effect_binders.iter().enumerate() {
            let summary = graph.resolved_effect_summary(summary)?;
            if !self.effect_binders.contains_key(&summary) { self.effect_binders.insert(summary, self.effect_order.len() as u32); self.effect_order.push(index); }
        }
        self.scope_ready = true; Ok(())
    }
    fn visit(&mut self, depth: usize) -> Result<(), QueryError> {
        self.query.validate_owner()?;
        self.nodes += 1;
        self.text(64)?;
        if depth > self.query.limits.depth || self.nodes > self.query.limits.nodes { Err(QueryError::Limit) } else { Ok(()) }
    }
    fn text(&self, bytes: usize) -> Result<(), QueryError> {
        let total = self.text_bytes.get().checked_add(bytes).ok_or(QueryError::Limit)?;
        if total > self.query.limits.text_bytes { return Err(QueryError::Limit); }
        self.text_bytes.set(total); Ok(())
    }
    fn name(&self, name: Name) -> Result<String, QueryError> {
        let text = self.query.symbols.resolve(name).ok_or(QueryError::ForeignSymbol)?;
        self.text(text.len())?;
        Ok(text.to_string())
    }
    fn ty(&mut self, ty: TypeId) -> Result<NormalizedType, QueryError> {
        let shape = self.graph(ty, 0)?;
        let annotation = match self.query.solved.graph.export_type(ty) {
            Ok(ty) => closed_annotation(&ty),
            Err(InferenceError::Unresolved(_)) | Err(InferenceError::Boundary(_)) => None,
            Err(error) => return Err(error.into()),
        };
        Ok(NormalizedType { shape, annotation })
    }
    fn scheme(&mut self, ty: TypeId, scheme: Option<SchemeId>) -> Result<NormalizedScheme, QueryError> {
        let ty = self.ty(ty)?;
        let mut normalized = NormalizedScheme { ty, quantifiers: Vec::new(), effect_quantifiers: Vec::new(), effect_roots: Vec::new(), effect_inclusions: Vec::new(), requirements: Vec::new() };
        if let Some(id) = scheme {
            let scheme = self.query.solved.graph.scheme(id)?;
            for &index in &self.type_order.clone() {
                self.visit(0)?;
                let quantifier = scheme.quantifiers.get(index).ok_or(QueryError::ScopeEscape)?;
                let mut lacks = quantifier.lacks.iter().map(|&name| self.name(name)).collect::<Result<Vec<_>, _>>()?;
                lacks.sort();
                normalized.quantifiers.push(NormalizedQuantifier { kind: binder_kind(quantifier.kind), lacks });
            }
            for &index in &self.effect_order.clone() {
                self.visit(0)?;
                let quantifier = scheme.effect_quantifiers.get(index).ok_or(QueryError::ScopeEscape)?;
                normalized.effect_quantifiers.push(NormalizedEffectQuantifier { lower: effects(quantifier.lower), upper: quantifier.upper.map(effects), derived: quantifier.derived });
            }
            for &(actual, expected) in &scheme.effect_inclusions { self.visit(0)?; normalized.effect_inclusions.push((self.effect(actual)?, self.effect(expected)?)); }
            normalized.effect_inclusions.sort(); normalized.effect_inclusions.dedup();
            for &effect in &scheme.effect_roots { self.visit(0)?; normalized.effect_roots.push(self.effect(effect)?); }
            normalized.effect_roots.sort(); normalized.effect_roots.dedup();
            for requirement in &scheme.requirements { normalized.requirements.push(self.requirement(*requirement)?); }
        }
        Ok(normalized)
    }
    fn requirement(&mut self, requirement: RequirementTemplate) -> Result<NormalizedRequirement, QueryError> {
        self.visit(0)?;
        self.prepare_scope()?;
        match requirement {
            RequirementTemplate::Add { left, right, result } => Ok(NormalizedRequirement::Add { left: self.graph(left, 0)?, right: self.graph(right, 0)?, result: self.graph(result, 0)? }),
            RequirementTemplate::EqualityCompatible { left, right } => Ok(NormalizedRequirement::EqualityCompatible { left: self.graph(left, 0)?, right: self.graph(right, 0)? }),
            RequirementTemplate::EffectInclusion { actual, expected, excluded } => Ok(NormalizedRequirement::EffectInclusion { actual: self.effect(actual)?, expected: self.effect(expected)?, excluded: effects(excluded) }),
            RequirementTemplate::Eligibility { predicate, ty } => Ok(NormalizedRequirement::Eligibility { predicate: eligibility_predicate(predicate), ty: self.graph(ty, 0)? }),
            RequirementTemplate::CallableInvocation { call } => {
                let NormalizedInvocationCall { callable, arguments, result, effects, domain } = self.invocation_call(call)?;
                Ok(NormalizedRequirement::CallableInvocation { callable, arguments, result, effects, domain })
            }
            RequirementTemplate::Operation { family, call } => {
                let graph = &self.query.solved.graph;
                let mut candidates = graph.family(family)?.iter().map(|&candidate| {
                    self.candidate(candidate)
                }).collect::<Result<Vec<_>, QueryError>>()?;
                candidates.sort_by(|left, right| left.identity.cmp(&right.identity));
                let call = graph.operation_call(call)?;
                let mut effect_bindings = call.effect_bindings.iter().map(|&(role, summary)| Ok((effect_role(role), self.effect(summary)?))).collect::<Result<Vec<_>, QueryError>>()?;
                effect_bindings.sort_by_key(|(role, _)| *role);
                if effect_bindings.windows(2).any(|pair| pair[0].0 == pair[1].0) { return Err(QueryError::Recovery); }
                let mut output_effect_bindings = call.output_effect_bindings.iter().map(|&(role, summary)| Ok((producer_role(role), self.effect(summary)?))).collect::<Result<Vec<_>, QueryError>>()?;
                output_effect_bindings.sort_by_key(|(role, _)| *role);
                if output_effect_bindings.windows(2).any(|pair| pair[0].0 == pair[1].0) { return Err(QueryError::Recovery); }
                Ok(NormalizedRequirement::Operation {
                    binding: match call.binding { crate::sema::inference::OperationBinding::Slots => NormalizedOperationBinding::Slots, crate::sema::inference::OperationBinding::Invocation(invocation) => NormalizedOperationBinding::Invocation(Box::new(self.invocation_call(invocation)?)) },
                    effect_mode: match call.effect_mode { crate::sema::inference::OperationEffectMode::AvailableBudget => NormalizedOperationEffectMode::AvailableBudget, crate::sema::inference::OperationEffectMode::ComputedCreation => NormalizedOperationEffectMode::ComputedCreation },
                    mono_authority: call.mono_authority.map(|authority| self.native_authority(authority, 0)).transpose()?,
                    declared_error_bound: call.declared_error_bound.map(|ty| self.graph(ty, 0)).transpose()?, candidates, receiver: call.receiver.map(|ty| self.graph(ty, 0)).transpose()?,
                    arguments: call.arguments.iter().map(|ty| ty.map(|ty| self.graph(ty, 0)).transpose()).collect::<Result<Vec<_>, QueryError>>()?,
                    result: self.graph(call.result, 0)?, effects: self.effect(call.effects)?,
                    effect_bindings, output_effect_bindings,
                })
            }
        }
    }
    fn invocation_call(&mut self, id: crate::sema::inference::InvocationCallId) -> Result<NormalizedInvocationCall, QueryError> {
        self.visit(0)?;
        let call = self.query.solved.graph.invocation_call(id)?;
        if call.arguments.len() > self.query.limits.nodes.saturating_sub(self.nodes) { return Err(QueryError::Limit); }
        let arguments = call.arguments.iter().map(|argument| {
            self.visit(0)?;
            let kind = match argument.kind {
                crate::sema::inference::InvocationArgumentKind::Positional => NormalizedInvocationArgumentKind::Positional,
                crate::sema::inference::InvocationArgumentKind::Named(name) => NormalizedInvocationArgumentKind::Named(self.name(name)?),
                crate::sema::inference::InvocationArgumentKind::PositionalSplice => NormalizedInvocationArgumentKind::PositionalSplice,
            };
            Ok(NormalizedInvocationArgument { kind, ty: self.graph(argument.ty, 0)? })
        }).collect::<Result<Vec<_>, QueryError>>()?;
        Ok(NormalizedInvocationCall { callable: self.graph(call.callable, 0)?, arguments, result: self.graph(call.result, 0)?, effects: self.effect(call.effects)?, domain: match call.domain { crate::sema::inference::CallableDomain::Pure => NormalizedCallableDomain::Pure, crate::sema::inference::CallableDomain::AnyCallable => NormalizedCallableDomain::AnyCallable, crate::sema::inference::CallableDomain::Exact(kind) => NormalizedCallableDomain::Exact(callable_form(kind)) } })
    }
    fn native_authority(&mut self, authority: crate::sema::inference::NativeAuthority, depth: usize) -> Result<NormalizedNativeAuthority, QueryError> {
        use crate::sema::inference::NativeAuthority;
        match authority {
            NativeAuthority::Single(id) => Ok(NormalizedNativeAuthority::Single(self.native_contract(id, depth)?)),
            NativeAuthority::Family(id) => {
                self.visit(depth)?;
                if let Some(family) = self.native_families.get(&id) { return Ok(NormalizedNativeAuthority::Family(family.clone())); }
                if self.active_native_families.len() >= self.query.limits.depth || !self.active_native_families.insert(id) { return Err(QueryError::Limit); }
                let result = (|| {
                    self.prepare_scope()?;
                    let graph = &self.query.solved.graph;
                    graph.validate_native_family_contract_scoped(id, self.scope)?;
                    let family = graph.native_family_contract(id)?;
                    if family.members.is_empty() || family.members.len() != graph.family(family.family)?.len() { return Err(QueryError::Recovery); }
                    if family.members.len() > self.query.limits.nodes.saturating_sub(self.nodes) { return Err(QueryError::Limit); }
                    let signature = Box::new(self.graph(family.signature, depth + 1)?);
                    let mut members = family.members.iter().map(|&member| {
                        let contract = graph.native_contract(member)?;
                        if contract.family != family.family || !graph.family(family.family)?.contains(&contract.candidate) { return Err(QueryError::Recovery); }
                        self.native_contract(member, depth + 1)
                    }).collect::<Result<Vec<_>, QueryError>>()?;
                    members.sort_by(|left, right| left.candidate.identity.cmp(&right.candidate.identity));
                    if members.windows(2).any(|pair| pair[0].candidate.identity == pair[1].candidate.identity) { return Err(QueryError::Recovery); }
                    Ok(std::sync::Arc::new(NormalizedNativeFamilyContract { owner: self.query.solved.owner, signature, members }))
                })();
                self.active_native_families.remove(&id);
                if let Ok(family) = &result { self.native_families.insert(id, family.clone()); }
                result.map(NormalizedNativeAuthority::Family)
            }
        }
    }
    fn domain_key(&mut self, ty: TypeId, depth: usize) -> Result<String, QueryError> {
        self.visit(depth)?;
        let graph = &self.query.solved.graph;
        let ty = graph.resolved(ty)?;
        let (head, children) = match graph.node(ty)? {
            TypeNode::Atom(atom) => return match self.atom(*atom, Some(ty))? { NormalizedShape::Atom(name) => Ok(name), NormalizedShape::Nominal(nominal) => Ok(nominal.spelling), _ => Err(QueryError::Recovery) },
            TypeNode::List(item) => ("List", vec![self.domain_key(*item, depth + 1)?]),
            TypeNode::Stream(item) => ("Stream", vec![self.domain_key(*item, depth + 1)?]),
            TypeNode::Optional(item) => ("Optional", vec![self.domain_key(*item, depth + 1)?]),
            TypeNode::Map(key, value) => ("Map", vec![self.domain_key(*key, depth + 1)?, self.domain_key(*value, depth + 1)?]),
            TypeNode::Result(ok, error) => ("Result", vec![self.domain_key(*ok, depth + 1)?, self.domain_key(*error, depth + 1)?]),
            _ => return Err(QueryError::Unresolved),
        };
        let bytes = children.iter().try_fold(head.len() + 2, |bytes, child| bytes.checked_add(child.len() + 1)).ok_or(QueryError::Limit)?;
        self.text(bytes)?;
        Ok(format!("{head}[{}]", children.join(",")))
    }
    fn native_contract(&mut self, id: crate::sema::inference::NativeContractId, depth: usize) -> Result<std::sync::Arc<NormalizedNativeContract>, QueryError> {
        self.visit(depth)?;
        if let Some(contract) = self.native_contracts.get(&id) { return Ok(contract.clone()); }
        if self.active_native_contracts.len() >= self.query.limits.depth || !self.active_native_contracts.insert(id) { return Err(QueryError::Limit); }
        let result = self.normalize_native_contract(id, depth).map(std::sync::Arc::new);
        self.active_native_contracts.remove(&id);
        if let Ok(contract) = &result { self.native_contracts.insert(id, contract.clone()); }
        result
    }
    fn normalize_native_contract(&mut self, id: crate::sema::inference::NativeContractId, depth: usize) -> Result<NormalizedNativeContract, QueryError> {
        self.prepare_scope()?;
        let graph = &self.query.solved.graph;
        let contract = graph.native_contract(id)?;
        let candidate = graph.candidate(contract.candidate)?;
        if candidate.scheme != contract.scheme || !graph.family(contract.family)?.contains(&contract.candidate) { return Err(QueryError::Recovery); }
        let entries = contract.instance.substitutions.len().checked_add(contract.instance.effect_substitutions.len()).and_then(|count| count.checked_add(contract.instance.effect_roots.len())).and_then(|count| count.checked_add(contract.instance.requirements.len())).and_then(|count| count.checked_add(contract.instance.requirement_origins.len())).ok_or(QueryError::Limit)?;
        if entries > self.query.limits.nodes.saturating_sub(self.nodes) || entries > self.query.limits.text_bytes.saturating_sub(self.text_bytes.get()) / 64 { return Err(QueryError::Limit); }
        graph.validate_native_contract_scoped(id, self.scope)?;
        let remaining = QueryLimits { depth: self.query.limits.depth.checked_sub(depth + 1).ok_or(QueryError::Limit)?, nodes: self.query.limits.nodes.checked_sub(self.nodes).ok_or(QueryError::Limit)?, text_bytes: self.query.limits.text_bytes.checked_sub(self.text_bytes.get()).ok_or(QueryError::Limit)? };
        let prototype_query = SolvedQuery::new(self.query.solved, self.query.symbols).with_limits(remaining);
        let mut prototype_view = View::new(&prototype_query, Some(contract.scheme));
        let prototype = Box::new(prototype_view.scheme(graph.scheme(contract.scheme)?.body, Some(contract.scheme))?);
        self.nodes = self.nodes.checked_add(prototype_view.nodes).ok_or(QueryError::Limit)?;
        self.text(prototype_view.text_bytes.get())?;
        let actual_eligibility = candidate.actual_eligibility.iter().map(|&(index, predicate)| { self.visit(depth + 1)?; Ok((index, eligibility_predicate(predicate))) }).collect::<Result<Vec<_>, QueryError>>()?;
        let argument_relations = candidate.argument_relations.iter().map(|relation| { self.visit(depth + 1)?; Ok(argument_relation(*relation)) }).collect::<Result<Vec<_>, QueryError>>()?;
        Ok(NormalizedNativeContract {
            owner: self.query.solved.owner, candidate: self.candidate(contract.candidate)?, prototype,
            signature: Box::new(self.graph(contract.instance.ty, depth + 1)?),
            substitutions: contract.instance.substitutions.iter().map(|&ty| self.graph(ty, depth + 1)).collect::<Result<Vec<_>, QueryError>>()?,
            effect_substitutions: contract.instance.effect_substitutions.iter().map(|&effect| self.effect(EffectSummary::Variable(effect))).collect::<Result<Vec<_>, QueryError>>()?,
            effect_roots: contract.instance.effect_roots.iter().map(|&effect| self.effect(effect)).collect::<Result<Vec<_>, QueryError>>()?,
            requirements: contract.instance.requirements.iter().map(|&requirement| self.requirement(graph.requirement_template(requirement)?)).collect::<Result<Vec<_>, QueryError>>()?,
            actual_eligibility, argument_relations, has_receiver: candidate.has_receiver,
        })
    }
    fn candidate(&mut self, candidate: crate::sema::inference::CandidateId) -> Result<NormalizedCandidate, QueryError> {
        let graph = &self.query.solved.graph;
        self.visit(0)?;
        let candidate = graph.candidate(candidate)?;
        let formal = graph.scheme(candidate.scheme)?;
        let effect_quantifiers = formal.effect_quantifiers.iter().map(|quantifier| {
            self.visit(0)?;
            Ok(NormalizedEffectQuantifier { lower: effects(quantifier.lower), upper: quantifier.upper.map(effects), derived: quantifier.derived })
        }).collect::<Result<Vec<_>, QueryError>>()?;
        let mut effect_roles = candidate.effect_roles.iter().map(|&(role, reference)| {
            self.visit(0)?;
            Ok((effect_role(role), match reference {
            crate::sema::inference::EffectRoleReference::Binder(index) => {
                if index as usize >= effect_quantifiers.len() { return Err(QueryError::ScopeEscape); }
                NormalizedEffectRoleReference::Binder(index)
            }
            crate::sema::inference::EffectRoleReference::Fixed(bits) => NormalizedEffectRoleReference::Fixed(effects(bits)),
        })) }).collect::<Result<Vec<_>, QueryError>>()?;
        effect_roles.sort_by_key(|(role, _)| *role);
        if effect_roles.windows(2).any(|pair| pair[0].0 == pair[1].0) { return Err(QueryError::Recovery); }
        let output_effect_roots = formal.effect_roots.iter().map(|&summary| { self.visit(0)?; self.formal_effect(summary, candidate.scheme) }).collect::<Result<Vec<_>, QueryError>>()?;
        let mut output_effect_roles = candidate.output_effect_roles.iter().map(|&(role, index)| {
            self.visit(0)?;
            if index as usize >= output_effect_roots.len() { return Err(QueryError::ScopeEscape); }
            Ok((producer_role(role), index))
        }).collect::<Result<Vec<_>, QueryError>>()?;
        output_effect_roles.sort_by_key(|(role, _)| *role);
        if output_effect_roles.windows(2).any(|pair| pair[0].0 == pair[1].0) { return Err(QueryError::Recovery); }
        let failure_projection = candidate.failure_projection.map(|projection| {
            self.visit(0)?;
            Ok::<_, QueryError>(match projection {
                crate::sema::inference::OperationFailureProjection::ReceiverResultError => NormalizedOperationFailureProjection::ReceiverResultError,
                crate::sema::inference::OperationFailureProjection::ArgumentResultError { argument } => NormalizedOperationFailureProjection::ArgumentResultError { argument },
            })
        }).transpose()?;
        Ok(NormalizedCandidate { failure_projection, identity: self.name(candidate.identity)?, public_label: self.name(candidate.public_label)?, effect_quantifiers, effect_roles, output_effect_roots, output_effect_roles })
    }
    fn formal_effect(&self, effect: EffectSummary, scope: SchemeId) -> Result<NormalizedEffect, QueryError> {
        let graph = &self.query.solved.graph;
        let effect = graph.resolved_effect_summary(effect)?;
        match effect {
            EffectSummary::Closed(bits) => Ok(NormalizedEffect::Closed(effects(bits))),
            EffectSummary::Unknown => Ok(NormalizedEffect::Unknown),
            EffectSummary::Variable(_) => Err(QueryError::Unresolved),
            EffectSummary::Rigid { .. } => {
                if let Some(index) = graph.scheme_effect_binder_index(scope, effect)? { return Ok(NormalizedEffect::Binder(u32::try_from(index).map_err(|_| QueryError::Limit)?)); }
                for (index, &capture) in graph.scheme(scope)?.effect_captures.iter().enumerate() {
                    if graph.resolved_effect_summary(capture)? == effect { return Ok(NormalizedEffect::Capture(index as u32)); }
                }
                Err(QueryError::ScopeEscape)
            }
        }
    }
    fn effect(&self, effect: EffectSummary) -> Result<NormalizedEffect, QueryError> {
        let graph = &self.query.solved.graph;
        Ok(match graph.resolved_effect_summary(effect)? {
            EffectSummary::Closed(bits) => NormalizedEffect::Closed(effects(bits)),
            EffectSummary::Unknown => NormalizedEffect::Unknown,
            EffectSummary::Variable(_) => return Err(QueryError::Unresolved),
            rigid @ EffectSummary::Rigid { .. } => {
                let scope = self.scope.ok_or(QueryError::ScopeEscape)?;
                let scheme = graph.scheme(scope)?;
                if let Some(&index) = self.effect_binders.get(&rigid) { NormalizedEffect::Binder(index) }
                else {
                    let captures = scheme.effect_captures.iter().map(|&capture| graph.resolved_effect_summary(capture)).collect::<Result<Vec<_>, _>>()?;
                    if let Some(index) = captures.iter().position(|&capture| capture == rigid) { NormalizedEffect::Capture(index as u32) }
                    else { return Err(QueryError::ScopeEscape); }
                }
            }
        })
    }
    fn graph(&mut self, ty: TypeId, depth: usize) -> Result<NormalizedShape, QueryError> {
        self.visit(depth)?;
        self.prepare_scope()?;
        let graph = &self.query.solved.graph;
        let ty = graph.resolved(ty)?;
        Ok(match graph.node(ty)? {
            TypeNode::Atom(atom) => self.atom(*atom, Some(ty))?,
            TypeNode::Meta(_) => return Err(QueryError::Unresolved),
            TypeNode::Poison | TypeNode::NonCompletion => return Err(QueryError::Recovery),
            TypeNode::Rigid { kind, .. } => {
                let scope = self.scope.ok_or(QueryError::ScopeEscape)?;
                if let Some(&index) = self.type_binders.get(&ty) { NormalizedShape::Binder { index, kind: binder_kind(*kind) } }
                else {
                    let captures = graph.scheme(scope)?.captures.iter().map(|&capture| graph.resolved(capture)).collect::<Result<Vec<_>, _>>()?;
                    if let Some(index) = captures.iter().position(|&capture| capture == ty) { NormalizedShape::Capture { index: index as u32, kind: binder_kind(*kind) } }
                    else { return Err(QueryError::ScopeEscape); }
                }
            }
            TypeNode::Optional(item) => NormalizedShape::Optional(Box::new(self.graph(*item, depth + 1)?)),
            TypeNode::List(item) => NormalizedShape::List(Box::new(self.graph(*item, depth + 1)?)),
            TypeNode::Stream(item) => NormalizedShape::Stream(Box::new(self.graph(*item, depth + 1)?)),
            TypeNode::Map(key, value) => NormalizedShape::Map(Box::new(self.graph(*key, depth + 1)?), Box::new(self.graph(*value, depth + 1)?)),
            TypeNode::Result(ok, error) => NormalizedShape::Result(Box::new(self.graph(*ok, depth + 1)?), Box::new(self.graph(*error, depth + 1)?)),
            TypeNode::Record(row) | TypeNode::Row(row) => {
                let (fields, tail) = self.row(*row, depth + 1)?;
                if matches!(graph.node(ty)?, TypeNode::Record(_)) { NormalizedShape::Record { fields, tail } } else { NormalizedShape::Row { fields, tail } }
            }
            TypeNode::Module(fields) => {
                let mut fields = fields.iter().map(|field| Ok(NormalizedField { label: self.name(field.label)?, ty: self.graph(field.ty, depth + 1)?, optional: field.optional })).collect::<Result<Vec<_>, QueryError>>()?;
                fields.sort_by(|left, right| left.label.cmp(&right.label));
                NormalizedShape::Module(fields)
            }
            TypeNode::CallableChoice(signatures) => {
                if signatures.is_empty() || signatures.len() > self.query.limits.nodes.saturating_sub(self.nodes) { return Err(QueryError::Limit); }
                let signatures = signatures.iter().map(|&signature| { self.visit(depth + 1)?; let signature = self.graph(signature, depth + 1)?; if !matches!(signature, NormalizedShape::Arrow(_)) { return Err(QueryError::Recovery); } Ok(signature) }).collect::<Result<Vec<_>, QueryError>>()?;
                NormalizedShape::CallableChoice(signatures)
            }
            TypeNode::FiniteDomain(domains) => {
                if domains.len() > self.query.limits.nodes.saturating_sub(self.nodes) { return Err(QueryError::Limit); }
                let mut ordered = domains.iter().map(|alternative| Ok((self.domain_key(alternative.ty, depth + 1)?, argument_relation(alternative.relation), alternative.ty))).collect::<Result<Vec<_>, QueryError>>()?;
                ordered.sort_by(|left, right| (&left.0, left.1).cmp(&(&right.0, right.1)));
                let alternatives = ordered.into_iter().map(|(_, relation, ty)| Ok(NormalizedDomainAlternative { ty: self.graph(ty, depth + 1)?, relation })).collect::<Result<Vec<_>, QueryError>>()?;
                NormalizedShape::FiniteDomain(alternatives)
            }
            TypeNode::NativeCallable(callable) => {
                if callable.alternatives.len() > self.query.limits.nodes.saturating_sub(self.nodes) { return Err(QueryError::Limit); }
                let signature = Box::new(self.graph(callable.signature, depth + 1)?);
                if signature.callable_signatures().is_none() { return Err(QueryError::Recovery); }
                let mut alternatives = callable.alternatives.iter().map(|authority| {
                    self.visit(depth + 1)?;
                    Ok(match authority {
                        crate::sema::inference::CallableAuthority::User { signature } => NormalizedCallableAuthority::User { signature: Box::new(self.graph(*signature, depth + 1)?) },
                        crate::sema::inference::CallableAuthority::Native { authority } => NormalizedCallableAuthority::Native(self.native_authority(*authority, depth + 1)?),
                    })
                }).collect::<Result<Vec<_>, QueryError>>()?;
                alternatives.sort_by(|left, right| authority_key(left).cmp(authority_key(right)));
                NormalizedShape::NativeCallable { signature, alternatives }
            }
            TypeNode::Arrow(arrow) => {
                let parameters = arrow.params.iter().map(|param| Ok(NormalizedParameter { label: self.name(param.label)?, ty: self.graph(param.ty, depth + 1)?, defaulted: param.defaulted, rest: param.rest })).collect::<Result<Vec<_>, QueryError>>()?;
                NormalizedShape::Arrow(NormalizedArrow { kind: callable_form(arrow.kind), parameters, result: Box::new(self.graph(arrow.result, depth + 1)?), effects: self.effect(arrow.effects)? })
            }
        })
    }
    fn row(&mut self, row: crate::sema::inference::RowId, depth: usize) -> Result<(Vec<NormalizedField>, Option<Box<NormalizedShape>>), QueryError> {
        self.visit(depth)?;
        let graph = &self.query.solved.graph;
        let row = graph.row_data(row)?;
        let mut fields = row.fields.iter().map(|field| Ok(NormalizedField { label: self.name(field.label)?, ty: self.graph(field.ty, depth + 1)?, optional: false })).collect::<Result<Vec<_>, QueryError>>()?;
        let mut tail = None;
        if let Some(ty) = row.tail {
            let ty = graph.resolved(ty)?;
            if let TypeNode::Row(row) = graph.node(ty)? {
                let (mut more, rest) = self.row(*row, depth + 1)?;
                fields.append(&mut more); tail = rest;
            } else { tail = Some(Box::new(self.graph(ty, depth + 1)?)); }
        }
        fields.sort_by(|left, right| left.label.cmp(&right.label));
        if fields.windows(2).any(|pair| pair[0].label == pair[1].label) { return Err(QueryError::Recovery); }
        Ok((fields, tail))
    }
    fn atom(&self, atom: Atom, ty: Option<TypeId>) -> Result<NormalizedShape, QueryError> {
        let (kind, spelling) = match atom {
            Atom::Tag(name) => (NominalKind::Tag, self.name(name)?),
            Atom::ErrorFamily(name) => (NominalKind::ErrorFamily, self.name(name)?),
            Atom::ErrorFacet(name) => (NominalKind::ErrorFacet, self.name(name)?),
            Atom::ErrorVariant { family, variant } => (NominalKind::ErrorVariant, format!("{}.{}", self.name(family)?, self.name(variant)?)),
            atom => return Ok(NormalizedShape::Atom(atom_name(atom).to_string())),
        };
        let identity = ty.and_then(|ty| self.query.solved.nominals.get(&ty)).map(|identity| -> Result<NominalIdentity, QueryError> {
            Ok(match identity {
                crate::sema::check::QualifiedNominalIdentity::Source { source, namespace, declaration, member } => NominalIdentity::Source {
                    owner: self.query.solved.owner, source: *source,
                    namespace: namespace.map(|name| self.name(name)).transpose()?,
                    declaration: match declaration {
                        crate::sema::check::NominalDeclaration::Type(id) => NominalDeclaration::Type(*id),
                        crate::sema::check::NominalDeclaration::Error(id) => NominalDeclaration::Error(*id),
                    },
                    member: member.map(|name| self.name(name)).transpose()?,
                },
                crate::sema::check::QualifiedNominalIdentity::Builtin { family, member } => NominalIdentity::Builtin {
                    owner: self.query.solved.owner, family: self.name(*family)?,
                    member: member.map(|name| self.name(name)).transpose()?,
                },
            })
        }).transpose()?;
        Ok(NormalizedShape::Nominal(NominalType { kind, spelling, identity }))
    }
    fn tree(&mut self, ty: &Type, depth: usize) -> Result<NormalizedShape, QueryError> {
        self.visit(depth)?;
        Ok(match ty {
            Type::Graph(_) if self.registry => return Err(QueryError::ForeignGraph),
            Type::Graph(id) => self.graph(*id, depth + 1)?,
            Type::Inference(_) | Type::Unknown => return Err(QueryError::Unresolved),
            Type::Invalid => return Err(QueryError::Recovery),
            Type::BuiltinParameter(parameter) => NormalizedShape::BuiltinParameter(parameter.label().to_string()),
            Type::Optional(item) => NormalizedShape::Optional(Box::new(self.tree(item, depth + 1)?)),
            Type::List(item) => NormalizedShape::List(Box::new(self.tree(item, depth + 1)?)),
            Type::Stream(item) => NormalizedShape::Stream(Box::new(self.tree(item, depth + 1)?)),
            // The registry's unspecialized Map carrier predates its named
            // builtin parameters. This view preserves that template spelling;
            // an ordinary source Unknown/Any pair never establishes binders.
            Type::Map(key, value) if self.registry && matches!(key.as_ref(), Type::Unknown) && matches!(value.as_ref(), Type::Any) => NormalizedShape::Map(Box::new(NormalizedShape::BuiltinParameter("K".to_string())), Box::new(NormalizedShape::BuiltinParameter("V".to_string()))),
            Type::Map(key, value) => NormalizedShape::Map(Box::new(self.tree(key, depth + 1)?), Box::new(self.tree(value, depth + 1)?)),
            Type::Result(ok, error) => NormalizedShape::Result(Box::new(self.tree(ok, depth + 1)?), Box::new(self.tree(error, depth + 1)?)),
            Type::Record(fields) => {
                let mut fields = fields.iter().map(|(&label, ty)| Ok(NormalizedField { label: self.name(label)?, ty: self.tree(ty, depth + 1)?, optional: false })).collect::<Result<Vec<_>, QueryError>>()?;
                fields.sort_by(|left, right| left.label.cmp(&right.label));
                NormalizedShape::Record { fields, tail: None }
            }
            Type::Module(fields) => {
                let mut fields = fields.iter().map(|(&label, field)| {
                    let ty = match field {
                        ModuleExportType::Value { ty, .. } => self.tree(ty, depth + 1)?,
                        ModuleExportType::Pure { sig, .. } | ModuleExportType::Proc { sig, .. } => {
                            let parameters = sig.params.iter().map(|param| Ok(NormalizedParameter { label: self.name(param.name)?, ty: self.tree(&param.ty, depth + 1)?, defaulted: param.defaulted, rest: param.rest })).collect::<Result<Vec<_>, QueryError>>()?;
                            NormalizedShape::Arrow(NormalizedArrow { kind: if matches!(field, ModuleExportType::Pure { .. }) { CallableForm::Pure } else { CallableForm::Proc }, parameters, result: Box::new(self.tree(&sig.return_ty, depth + 1)?), effects: sig.effects.as_ref().map(|effects| NormalizedEffect::Closed(effects.iter().map(|effect| effect.as_str().to_string()).collect())).unwrap_or(NormalizedEffect::Unknown) })
                        }
                    };
                    Ok(NormalizedField { label: self.name(label)?, ty, optional: field.optional() })
                }).collect::<Result<Vec<_>, QueryError>>()?;
                fields.sort_by(|left, right| left.label.cmp(&right.label));
                NormalizedShape::Module(fields)
            }
            Type::Tag(name) => self.atom(Atom::Tag(*name), None)?,
            Type::ErrorFamily(name) => self.atom(Atom::ErrorFamily(*name), None)?,
            Type::ErrorFacet(name) => self.atom(Atom::ErrorFacet(*name), None)?,
            Type::ErrorVariant { family, variant } => self.atom(Atom::ErrorVariant { family: *family, variant: *variant }, None)?,
            ty => NormalizedShape::Atom(ty.to_string()),
        })
    }
}

fn eligibility_predicate(predicate: Eligibility) -> EligibilityPredicate { match predicate { Eligibility::MapKey => EligibilityPredicate::MapKey, Eligibility::JsonCompatible => EligibilityPredicate::JsonCompatible, Eligibility::NonUnit => EligibilityPredicate::NonUnit, Eligibility::Sortable => EligibilityPredicate::Sortable, Eligibility::SortableKey => EligibilityPredicate::SortableKey, Eligibility::ArgvItem => EligibilityPredicate::ArgvItem, Eligibility::CountKey => EligibilityPredicate::CountKey, Eligibility::Record => EligibilityPredicate::Record, Eligibility::YieldItem => EligibilityPredicate::YieldItem, Eligibility::CommandTarget => EligibilityPredicate::CommandTarget, Eligibility::CommandArgv => EligibilityPredicate::CommandArgv, Eligibility::Error => EligibilityPredicate::Error, Eligibility::Display => EligibilityPredicate::Display, Eligibility::ArgvExpansion => EligibilityPredicate::ArgvExpansion } }

fn command_text_domain(domain: crate::sema::inference::CommandTextDomain) -> NormalizedCommandTextDomain { match domain { crate::sema::inference::CommandTextDomain::Str => NormalizedCommandTextDomain::Str, crate::sema::inference::CommandTextDomain::Path => NormalizedCommandTextDomain::Path } }

fn argument_relation(relation: crate::sema::inference::ArgumentRelation) -> NormalizedArgumentRelation {
    use crate::sema::inference::ArgumentRelation;
    match relation {
        ArgumentRelation::Assignable => NormalizedArgumentRelation::Assignable,
        ArgumentRelation::Exact => NormalizedArgumentRelation::Exact,
        ArgumentRelation::DeclaredErasure => NormalizedArgumentRelation::DeclaredErasure,
        ArgumentRelation::EqualityCompatible => NormalizedArgumentRelation::EqualityCompatible,
        ArgumentRelation::InvocationProtocol => NormalizedArgumentRelation::InvocationProtocol,
        ArgumentRelation::CommandTarget { domain } => NormalizedArgumentRelation::CommandTarget { domain: command_text_domain(domain) },
        ArgumentRelation::CommandArgv { element } => NormalizedArgumentRelation::CommandArgv { element: command_text_domain(element) },
    }
}

fn authority_key(authority: &NormalizedCallableAuthority) -> &str { match authority { NormalizedCallableAuthority::User { .. } => "", NormalizedCallableAuthority::Native(authority) => authority.key() } }

fn binder_kind(kind: VariableKind) -> BinderKind { match kind { VariableKind::Type => BinderKind::Type, VariableKind::Row => BinderKind::Row } }
fn effect_role(role: crate::sema::inference::EffectRole) -> NormalizedEffectRole {
    match role {
        crate::sema::inference::EffectRole::Creation => NormalizedEffectRole::Creation,
        crate::sema::inference::EffectRole::Callback => NormalizedEffectRole::Callback,
        crate::sema::inference::EffectRole::Pull { source } => NormalizedEffectRole::Pull { source },
        crate::sema::inference::EffectRole::Close { source } => NormalizedEffectRole::Close { source },
        crate::sema::inference::EffectRole::PullProjection { source, projection } => NormalizedEffectRole::PullProjection { source, projection: effect_projection(projection) },
        crate::sema::inference::EffectRole::CloseProjection { source, projection } => NormalizedEffectRole::CloseProjection { source, projection: effect_projection(projection) },
    }
}

fn effect_projection(projection: crate::sema::inference::EffectProjection) -> NormalizedEffectProjection {
    match projection { crate::sema::inference::EffectProjection::ResultSuccess => NormalizedEffectProjection::ResultSuccess }
}
fn callable_form(kind: CallableKind) -> CallableForm { match kind { CallableKind::Pure => CallableForm::Pure, CallableKind::Proc => CallableForm::Proc, CallableKind::Stream => CallableForm::Stream } }

fn effects(bits: EffectSet) -> Vec<String> {
    [(EffectSet::FS, "fs"), (EffectSet::NET, "net"), (EffectSet::PROCESS, "process"), (EffectSet::ENV, "env"), (EffectSet::TIME, "time"), (EffectSet::ERROR, "error"), (EffectSet::IO, "io")].into_iter().filter_map(|(bit, name)| (bits.0 & bit.0 != 0).then(|| name.to_string())).collect()
}

fn atom_name(atom: Atom) -> &'static str {
    match atom {
        Atom::Any => "Any", Atom::ErasedRecord => "Record", Atom::DynamicModule => "Module", Atom::Pure => "Pure", Atom::Proc => "Proc", Atom::Command => "Command",
        Atom::Null => "Null", Atom::Bool => "Bool", Atom::Int => "Int", Atom::UInt => "UInt", Atom::Float => "Float", Atom::Duration => "Duration", Atom::Str => "Str", Atom::Bytes => "Bytes", Atom::Digest => "Digest", Atom::Regex => "Regex", Atom::Path => "Path",
        Atom::Unit => "Unit", Atom::Status => "Status", Atom::EnvPathList => "EnvPathList", Atom::Error => "Error", Atom::ProcessError => "ProcessError", Atom::ProcessHandle => "ProcessHandle", Atom::NetJob => "NetJob", Atom::FsRoot => "FsRoot",
        Atom::Tag(_) | Atom::ErrorFamily(_) | Atom::ErrorVariant { .. } | Atom::ErrorFacet(_) => unreachable!("nominal atoms retain their checked names"),
    }
}

fn closed_annotation(ty: &Type) -> Option<String> {
    fn closed(ty: &Type) -> bool {
        match ty {
            Type::BuiltinParameter(_) | Type::Graph(_) | Type::Inference(_) | Type::Any | Type::Unknown | Type::Invalid => false,
            Type::List(item) | Type::Optional(item) | Type::Stream(item) => closed(item),
            Type::Map(key, value) | Type::Result(key, value) => closed(key) && closed(value),
            Type::Record(_) | Type::Module(_) => false,
            _ => true,
        }
    }
    closed(ty).then(|| ty.annotation_source()).flatten()
}

fn tree_annotation(ty: &Type, solved: &SolvedTypes) -> Option<String> {
    fn materialize(ty: &Type, solved: &SolvedTypes) -> Option<Type> {
        Some(match ty {
            Type::Graph(id) => solved.graph.export_type(*id).ok()?,
            Type::Optional(item) => Type::Optional(Box::new(materialize(item, solved)?)),
            Type::List(item) => Type::List(Box::new(materialize(item, solved)?)),
            Type::Stream(item) => Type::Stream(Box::new(materialize(item, solved)?)),
            Type::Map(key, value) => Type::Map(Box::new(materialize(key, solved)?), Box::new(materialize(value, solved)?)),
            Type::Result(ok, error) => Type::Result(Box::new(materialize(ok, solved)?), Box::new(materialize(error, solved)?)),
            Type::Record(_) | Type::Module(_) | Type::BuiltinParameter(_) | Type::Inference(_) | Type::Any | Type::Unknown | Type::Invalid => return None,
            ty => ty.clone(),
        })
    }
    closed_annotation(&materialize(ty, solved)?)
}

impl fmt::Display for NormalizedShape {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Atom(name) | Self::BuiltinParameter(name) => formatter.write_str(name),
            Self::Nominal(nominal) => formatter.write_str(&nominal.spelling),
            Self::Binder { index, kind } => write!(formatter, "{}{index}", if *kind == BinderKind::Type { "T" } else { "R" }),
            Self::Capture { index, kind } => write!(formatter, "captured {}{index}", if *kind == BinderKind::Type { "T" } else { "R" }),
            Self::Optional(item) => write!(formatter, "{item}?"),
            Self::List(item) => write!(formatter, "List[{item}]"),
            Self::Stream(item) => write!(formatter, "Stream[{item}]"),
            Self::Map(key, value) if matches!(key.as_ref(), Self::Atom(name) if name == "Str") => write!(formatter, "Map[{value}]"),
            Self::Map(key, value) => write!(formatter, "Map[{key}, {value}]"),
            Self::Result(ok, error) => write!(formatter, "Result[{ok}, {error}]"),
            Self::Record { fields, tail } | Self::Row { fields, tail } => {
                formatter.write_str("{")?;
                for (index, field) in fields.iter().enumerate() {
                    if index > 0 { formatter.write_str(", ")?; }
                    write!(formatter, "{}: {}", field.label, field.ty)?;
                }
                if let Some(tail) = tail { if !fields.is_empty() { formatter.write_str(" | ")?; } write!(formatter, "{tail}")?; }
                formatter.write_str("}")
            }
            Self::Module(fields) => {
                formatter.write_str("Module{")?;
                for (index, field) in fields.iter().enumerate() {
                    if index > 0 { formatter.write_str(", ")?; }
                    write!(formatter, "{}{}: {}", field.label, if field.optional { "?" } else { "" }, field.ty)?;
                }
                formatter.write_str("}")
            }
            Self::CallableChoice(signatures) => {
                formatter.write_str("choice {")?;
                for (index, signature) in signatures.iter().enumerate() { if index > 0 { formatter.write_str(" | ")?; } signature.fmt(formatter)?; }
                formatter.write_str("}")
            }
            Self::FiniteDomain(alternatives) => {
                formatter.write_str("domain {")?;
                for (index, alternative) in alternatives.iter().enumerate() { if index > 0 { formatter.write_str(" | ")?; } write!(formatter, "{} ({:?})", alternative.ty, alternative.relation)?; }
                formatter.write_str("}")
            }
            Self::NativeCallable { signature, alternatives } => {
                write!(formatter, "{signature}; authority {{")?;
                for (index, authority) in alternatives.iter().enumerate() {
                    if index > 0 { formatter.write_str(" | ")?; }
                    match authority { NormalizedCallableAuthority::User { .. } => formatter.write_str("user")?, NormalizedCallableAuthority::Native(authority) => formatter.write_str(authority.public_label())? }
                }
                formatter.write_str("}")
            }
            Self::Arrow(arrow) => {
                write!(formatter, "{}(", match arrow.kind { CallableForm::Pure => "pure", CallableForm::Proc => "proc", CallableForm::Stream => "stream" })?;
                for (index, param) in arrow.parameters.iter().enumerate() {
                    if index > 0 { formatter.write_str(", ")?; }
                    write!(formatter, "{}{}: {}{}", if param.rest { "..." } else { "" }, param.label, param.ty, if param.defaulted { " = default" } else { "" })?;
                }
                write!(formatter, ") {} -> {}", EffectDisplay(&arrow.effects), arrow.result)
            }
        }
    }
}

struct EffectDisplay<'a>(&'a NormalizedEffect);
struct EffectRoleDisplay(NormalizedEffectRole);
impl fmt::Display for EffectRoleDisplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            NormalizedEffectRole::Creation => formatter.write_str("creation"),
            NormalizedEffectRole::Callback => formatter.write_str("callback"),
            NormalizedEffectRole::Pull { source } => write!(formatter, "pull[{source}]"),
            NormalizedEffectRole::Close { source } => write!(formatter, "close[{source}]"),
            NormalizedEffectRole::PullProjection { source, projection: NormalizedEffectProjection::ResultSuccess } => write!(formatter, "pull[{source}].ok"),
            NormalizedEffectRole::CloseProjection { source, projection: NormalizedEffectProjection::ResultSuccess } => write!(formatter, "close[{source}].ok"),
        }
    }
}
impl fmt::Display for EffectDisplay<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            NormalizedEffect::Closed(effects) => write!(formatter, "[{}]", effects.join(", ")),
            NormalizedEffect::Binder(index) => write!(formatter, "E{index}"),
            NormalizedEffect::Capture(index) => write!(formatter, "captured E{index}"),
            NormalizedEffect::Unknown => formatter.write_str("[unknown effects]"),
        }
    }
}

impl fmt::Display for NormalizedScheme {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.quantifiers.is_empty() || !self.effect_quantifiers.is_empty() {
            formatter.write_str("forall ")?;
            let mut names = self.quantifiers.iter().enumerate().map(|(index, quantifier)| format!("{}{index}", if quantifier.kind == BinderKind::Type { "T" } else { "R" })).collect::<Vec<_>>();
            names.extend((0..self.effect_quantifiers.len()).map(|index| format!("E{index}")));
            write!(formatter, "{}. ", names.join(", "))?;
        }
        write!(formatter, "{}", self.ty)?;
        let mut conditions = Vec::new();
        for (index, quantifier) in self.quantifiers.iter().enumerate() {
            for label in &quantifier.lacks { conditions.push(format!("{}{index} lacks {label}", if quantifier.kind == BinderKind::Type { "T" } else { "R" })); }
        }
        for requirement in &self.requirements {
            match requirement {
                NormalizedRequirement::Add { left, right, result } => conditions.push(format!("Add({left}, {right}) -> {result}")),
                NormalizedRequirement::EqualityCompatible { left, right } => conditions.push(format!("EqCompatible({left}, {right})")),
                NormalizedRequirement::EffectInclusion { actual, expected, excluded } => conditions.push(format!("{} without [{}] <= {}", EffectDisplay(actual), excluded.join(", "), EffectDisplay(expected))),
                NormalizedRequirement::Eligibility { predicate, ty } => conditions.push(format!("{}({ty})", match predicate { EligibilityPredicate::MapKey => "MapKey", EligibilityPredicate::JsonCompatible => "JsonCompatible", EligibilityPredicate::NonUnit => "NonUnit", EligibilityPredicate::Sortable => "Sortable", EligibilityPredicate::SortableKey => "SortableKey", EligibilityPredicate::ArgvItem => "ArgvItem", EligibilityPredicate::CountKey => "CountKey", EligibilityPredicate::Record => "Record", EligibilityPredicate::YieldItem => "YieldItem", EligibilityPredicate::CommandTarget => "CommandTarget", EligibilityPredicate::CommandArgv => "CommandArgv", EligibilityPredicate::Error => "Error", EligibilityPredicate::Display => "Display", EligibilityPredicate::ArgvExpansion => "ArgvExpansion" })),
                NormalizedRequirement::CallableInvocation { callable, arguments, result, effects, domain } => {
                    let arguments = arguments.iter().map(|argument| match &argument.kind {
                        NormalizedInvocationArgumentKind::Positional => argument.ty.to_string(),
                        NormalizedInvocationArgumentKind::Named(label) => format!("{}: {}", diagnostic_label(label), argument.ty),
                        NormalizedInvocationArgumentKind::PositionalSplice => format!("...{}", argument.ty),
                    }).collect::<Vec<_>>().join(", ");
                    let domain = match domain { NormalizedCallableDomain::Pure => "pure", NormalizedCallableDomain::AnyCallable => "pure/proc/stream", NormalizedCallableDomain::Exact(CallableForm::Pure) => "exact pure", NormalizedCallableDomain::Exact(CallableForm::Proc) => "exact proc", NormalizedCallableDomain::Exact(CallableForm::Stream) => "exact stream" };
                    conditions.push(format!("Invoke[{domain}]({callable}; {arguments}) {} -> {result}", EffectDisplay(effects)));
                }
                NormalizedRequirement::Operation { binding, effect_mode, mono_authority, declared_error_bound, candidates, receiver, arguments, result, effects, effect_bindings, output_effect_bindings } => {
                    let arguments = arguments.iter().map(|ty| ty.as_ref().map(ToString::to_string).unwrap_or_else(|| "default".to_string())).collect::<Vec<_>>().join(", ");
                    let receiver = receiver.as_ref().map(|ty| format!(" on {ty}")).unwrap_or_default();
                    let labels = candidates.iter().map(|candidate| match candidate.failure_projection {
                        None => candidate.public_label.clone(),
                        Some(NormalizedOperationFailureProjection::ReceiverResultError) => format!("{} (failure receiver.error)", candidate.public_label),
                        Some(NormalizedOperationFailureProjection::ArgumentResultError { argument }) => format!("{} (failure argument[{argument}].error)", candidate.public_label),
                    }).collect::<Vec<_>>().join(" | ");
                    let roles = effect_bindings.iter().map(|(role, effect)| format!("{} {}", EffectRoleDisplay(*role), EffectDisplay(effect))).collect::<Vec<_>>();
                    let roles = if roles.is_empty() { String::new() } else { format!(" with {}", roles.join(", ")) };
                    let outputs = output_effect_bindings.iter().map(|(role, effect)| format!("{} {}", match role { NormalizedProducerRole::Pull => "pull", NormalizedProducerRole::Close => "close" }, EffectDisplay(effect))).collect::<Vec<_>>();
                    let outputs = if outputs.is_empty() { String::new() } else { format!(" produces {}", outputs.join(", ")) };
                    let mono = mono_authority.as_ref().map(|authority| format!("; native {}", authority.public_label())).unwrap_or_default();
                    let bound = declared_error_bound.as_ref().map(|bound| format!("; error bound {bound}")).unwrap_or_default();
                    let binding = match binding { NormalizedOperationBinding::Slots => String::new(), NormalizedOperationBinding::Invocation(call) => {
                        let arguments = call.arguments.iter().map(|argument| match &argument.kind { NormalizedInvocationArgumentKind::Positional => argument.ty.to_string(), NormalizedInvocationArgumentKind::Named(label) => format!("{}: {}", diagnostic_label(label), argument.ty), NormalizedInvocationArgumentKind::PositionalSplice => format!("...{}", argument.ty) }).collect::<Vec<_>>().join(", ");
                        format!("; invocation {}({arguments})", call.callable)
                    } };
                    let effect_mode = match effect_mode { NormalizedOperationEffectMode::AvailableBudget => "budget", NormalizedOperationEffectMode::ComputedCreation => "creation" };
                    conditions.push(format!("operation {{{labels}}}{receiver}({arguments}) {effect_mode} {} -> {result}{roles}{outputs}{bound}{mono}{binding}", EffectDisplay(effects)));
                }
            }
        }
        for (index, quantifier) in self.effect_quantifiers.iter().enumerate() {
            if !quantifier.lower.is_empty() { conditions.push(format!("[{}] <= E{index}", quantifier.lower.join(", "))); }
            if let Some(upper) = &quantifier.upper { conditions.push(format!("E{index} <= [{}]", upper.join(", "))); }
        }
        for (actual, expected) in &self.effect_inclusions { conditions.push(format!("{} <= {}", EffectDisplay(actual), EffectDisplay(expected))); }
        if !conditions.is_empty() { write!(formatter, " where {}", conditions.join(", "))?; }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    #[test]
    fn solved_query_identity_exposes_one_principal_relationship_without_rechecking() {
        let source = "pure identity(value) { value }\nlet integer: Int = identity(7)\nlet text: Str = identity(\"word\")\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let identity = *checked.solved.declarations.keys().next().unwrap();
        let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
        let before = checked.solved.graph.counters().instantiations;
        assert_eq!(query.declaration(identity).unwrap().to_string(), "forall T0. pure(value: T0) [] -> T0");
        assert_eq!(checked.solved.graph.counters().instantiations, before);
        let signature = query.declaration(identity).unwrap();
        assert_eq!(signature.semantic_parity(&signature), Ok(true));
    }

    #[test]
    fn solved_query_call_result_is_concrete_without_specializing_the_declaration() {
        let source = "pure identity(value) { value }\nlet integer: Int = identity(7)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
        let identity = *checked.solved.calls.keys().next().unwrap();
        assert_eq!(query.expression(identity).unwrap().to_string(), "Int");
    }

    #[test]
    fn solved_query_rows_and_additions_normalize_independent_declaration_order() {
        let first = "pure field(value) { value.name }\n";
        let second = "pure plus(left, right) { left + right }\n";
        let mut answers = Vec::new();
        for source in [format!("{first}{second}"), format!("{second}{first}")] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
            let mut signatures = checked.solved.declarations.keys().map(|&identity| {
                let name = parsed.arena.arena.function_def(identity.declaration).name.to_string();
                (name, query.declaration(identity).unwrap())
            }).collect::<Vec<_>>();
            signatures.sort_by(|left, right| left.0.cmp(&right.0));
            let row = &signatures[0].1;
            assert!(row.to_string().contains("name:"), "{row}");
            assert!(row.scheme.quantifiers.iter().any(|quantifier| quantifier.kind == BinderKind::Row && quantifier.lacks == ["name"]));
            assert!(matches!(signatures[1].1.scheme.requirements.as_slice(), [NormalizedRequirement::Add { .. }]));
            assert!(signatures.iter().all(|(_, signature)| signature.scheme.ty.annotation_source().is_none()));
            answers.push(signatures);
        }
        assert_eq!(answers[0], answers[1]);
    }

    #[test]
    fn solved_query_preserves_default_rest_labels_kind_and_effect_bounds() {
        let source = "proc collect(first: Int = 4, ...items: List[Int]) [io] -> List[Int] { [first, @items] }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
        let signature = query.declaration(*checked.solved.declarations.keys().next().unwrap()).unwrap();
        let NormalizedShape::Arrow(arrow) = &signature.scheme.ty.shape else { panic!("complete callable query retains arrow") };
        assert_eq!(arrow.kind, CallableForm::Proc);
        assert_eq!(arrow.parameters[0].label, "first");
        assert!(arrow.parameters[0].defaulted);
        assert!(!arrow.parameters[0].rest);
        assert_eq!(arrow.parameters[1].label, "items");
        assert!(arrow.parameters[1].rest);
        assert!(!arrow.parameters[1].defaulted);
        assert_eq!(signature.effective_effects, Some(NormalizedEffect::Closed(vec!["io".to_string()])));
        assert_eq!(signature.required_effects, Some(NormalizedEffect::Closed(Vec::new())));
        assert!(signature.to_string().contains("...items: List[Int]"));
    }

    #[test]
    fn solved_query_closed_annotation_parses_and_rechecks_without_generic_syntax() {
        let source = "pure fixed(value: Int) -> List[Result[Int]] { [Ok(value)] }\nlet result = fixed(7)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
        let ty = query.expression(*checked.solved.calls.keys().last().unwrap()).unwrap();
        let annotation = ty.annotation_source().expect("closed named result has source syntax");
        assert_eq!(annotation, "List[Result[Int]]");
        let rewritten = format!("pure fixed(value: Int) -> List[Result[Int]] {{ [Ok(value)] }}\nlet result: {annotation} = fixed(7)\n");
        let reparsed = Parser::parse_source_arena_only(SourceId::new(0), &rewritten);
        assert!(reparsed.diagnostics.is_empty(), "{:?}", reparsed.diagnostics);
        let rechecked = Checker::check_arena(&reparsed.arena, &rewritten);
        assert!(rechecked.diagnostics.is_empty(), "{:?}", rechecked.diagnostics);
    }

    #[test]
    fn solved_query_missing_foreign_and_unresolved_are_distinct() {
        use crate::sema::inference::InferenceContext;
        use crate::syntax::arena::ExprId;
        let source = "pure identity(distinct_dynamic_query_label) { distinct_dynamic_query_label }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        let checked = Checker::check_arena(&parsed.arena, source);
        let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
        let declaration = *checked.solved.declarations.keys().next().unwrap();
        assert_eq!(query.declaration(DeclarationIdentity { source: SourceId::new(99), ..declaration }), Err(QueryError::MissingDeclaration));
        assert_eq!(query.expression(ExpressionIdentity { source: SourceId::new(99), namespace: None, expression: ExprId::from_index(0) }), Err(QueryError::MissingExpression));
        let foreign = SymbolOwner::new();
        assert_eq!(SolvedQuery::new(&checked.solved, &foreign).declaration(declaration), Err(QueryError::ForeignSymbol));
        let mut raw = InferenceContext::default();
        let ground = raw.atom(Atom::Int).unwrap();
        assert_eq!(query.type_view(&Type::Graph(ground), None), Err(QueryError::ForeignGraph));
        let hole = raw.fresh(0, crate::source::Span::new(SourceId::new(0), 0, 0)).unwrap();
        let graph = raw.freeze(&[]).unwrap();
        let solved = SolvedTypes::from_graph(graph, foreign.clone());
        assert_eq!(SolvedQuery::new(&solved, &foreign).type_view(&Type::Graph(hole), None), Err(QueryError::Unresolved));
    }

    #[test]
    fn solved_query_expansion_is_bounded_and_does_not_mutate_solver_counters() {
        let source = "pure identity(value) { value }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        let checked = Checker::check_arena(&parsed.arena, source);
        let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner()).with_limits(QueryLimits { depth: 1, nodes: 1, ..QueryLimits::default() });
        let before = checked.solved.graph.counters().work_units;
        assert_eq!(query.declaration(*checked.solved.declarations.keys().next().unwrap()), Err(QueryError::Limit));
        assert_eq!(checked.solved.graph.counters().work_units, before);
    }

    #[test]
    fn solved_query_nested_graph_leaves_offer_only_closed_source_annotations() {
        let source = "pure identity(value) { value }\nlet integer: Int = identity(7)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        let checked = Checker::check_arena(&parsed.arena, source);
        let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
        let call = *checked.solved.calls.keys().next().unwrap();
        let ground = checked.solved.expressions[&call];
        let view = query.type_view(&Type::List(Box::new(Type::Graph(ground))), None).unwrap();
        assert_eq!(view.to_string(), "List[Int]");
        assert_eq!(view.annotation_source(), Some("List[Int]"));
        let (&identity, declaration) = checked.solved.declarations.iter().next().unwrap();
        let TypeNode::Arrow(arrow) = checked.solved.graph.node(declaration.signature).unwrap() else { panic!("identity retains full arrow") };
        let generic = query.type_view(&Type::List(Box::new(Type::Graph(arrow.params[0].ty))), Some(identity)).unwrap();
        assert_eq!(generic.to_string(), "List[T0]");
        assert_eq!(generic.annotation_source(), None);
    }

    #[test]
    fn solved_query_binding_retains_fixed_collection_type_and_mutability() {
        let source = "pure gather(value: Int) { var entries = []; let before = entries; entries += [value]; before }\nlet result: List[Int] = gather(7)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
        let mutable = checked.solved.bindings.iter().find(|(_, fact)| fact.mutable).map(|(&identity, _)| query.binding(identity).unwrap()).expect("mutable collection has source-owned binding fact");
        assert!(mutable.mutable);
        assert_eq!(mutable.scheme.ty.to_string(), "List[Int]");
        assert!(mutable.scheme.quantifiers.is_empty());
        assert_eq!(mutable.scheme.ty.annotation_source(), Some("List[Int]"));
        let snapshots = checked.solved.bindings.iter().filter(|(_, fact)| !fact.mutable).map(|(&identity, _)| query.binding(identity).unwrap()).collect::<Vec<_>>();
        assert!(snapshots.iter().any(|binding| binding.scheme.ty.to_string() == "List[Int]"));
    }

    #[test]
    fn solved_query_owned_names_survive_source_and_graph_disposal() {
        let signature = {
            let source = "pure identity(owned_dynamic_query_label) { owned_dynamic_query_label }\n";
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            let checked = Checker::check_arena(&parsed.arena, source);
            let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
            query.declaration(*checked.solved.declarations.keys().next().unwrap()).unwrap()
        };
        assert_eq!(signature.to_string(), "forall T0. pure(owned_dynamic_query_label: T0) [] -> T0");
    }

    #[test]
    fn solved_query_nominal_spelling_is_not_semantic_parity_evidence() {
        let source = "error QueryFailure = Failed(message: Str)\npure keep(value: QueryFailure) -> QueryFailure { value }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let mut opaque = std::sync::Arc::try_unwrap(checked.solved).unwrap();
        opaque.nominals.clear();
        let query = SolvedQuery::new(&opaque, parsed.arena.symbol_owner());
        let signature = query.declaration(*opaque.declarations.keys().next().unwrap()).unwrap();
        assert_eq!(signature, signature.clone());
        assert!(signature.to_string().contains("QueryFailure"));
        assert_eq!(signature.semantic_parity(&signature), Err(ParityError::UnidentifiedNominal));
    }

    #[test]
    fn solved_query_nominal_source_identity_compares_only_within_its_bundle() {
        let source = "error QueryFailure = Failed(message: Str)\npure keep(value: QueryFailure) -> QueryFailure { value }\n";
        let mut signatures = Vec::new();
        for _ in 0..2 {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            assert!(!checked.solved.nominals.is_empty(), "source nominal registration retains actual declaration identity");
            let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
            let signature = query.declaration(*checked.solved.declarations.keys().next().unwrap()).unwrap();
            assert_eq!(signature.semantic_parity(&signature), Ok(true));
            signatures.push(signature);
        }
        assert_eq!(signatures[0].semantic_parity(&signatures[1]), Err(ParityError::ForeignNominalOwner));
    }

    #[test]
    fn solved_query_builtin_error_identity_comes_from_registered_family() {
        let source = "pure keep(value: Result[Str, AssertionError]) -> Result[Str, AssertionError] { value }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
        let signature = query.declaration(*checked.solved.declarations.keys().next().unwrap()).unwrap();
        let NormalizedShape::Arrow(arrow) = signature.scheme.ty.shape() else { panic!("complete callable") };
        let NormalizedShape::Result(_, error) = arrow.parameters[0].ty.clone() else { panic!("Result parameter") };
        let NormalizedShape::Nominal(nominal) = error.as_ref() else { panic!("registered error family") };
        assert!(matches!(&nominal.identity, Some(NominalIdentity::Builtin { family, member: None, .. }) if family == "AssertionError"));
        assert_eq!(signature.semantic_parity(&signature), Ok(true));
    }

    #[test]
    fn solved_query_reveal_uses_frozen_principal_and_instantiated_source_facts() {
        let source = "pure identity(value) { value }\nlet alias = identity\npure name(value) { value.name }\nproc emit(value: Str)[io] -> Unit { print $value }\nproc inferred() -> Int { time.now() }\nreveal_type(alias)\nreveal_type(identity([1, 2]))\nreveal_type(name)\nreveal_type(emit)\nreveal_type(inferred)\nreveal_type(7)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions { reveal_types: true, ..Default::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let messages = checked.reveal_types.iter().map(|diagnostic| diagnostic.message.as_str()).collect::<Vec<_>>();
        assert_eq!(messages.len(), 6, "{messages:?}");
        assert_eq!(messages[0], "revealed type: forall T0. pure(value: T0) [] -> T0");
        assert_eq!(messages[1], "revealed type: List[Int]");
        assert_eq!(messages[2], "revealed type: forall T0, R1. pure(value: {name: T0 | R1}) [] -> T0 where R1 lacks name");
        assert_eq!(messages[3], "revealed type: proc(value: Str) [io] -> Unit");
        assert_eq!(messages[4], "revealed type: proc() [time] -> Int", "{:?}", checked.callable_effects);
        assert_eq!(messages[5], "revealed type: Int");
    }

    #[test]
    fn solved_query_unscoped_binders_and_captures_are_not_parity_evidence() {
        let generic = NormalizedType { shape: NormalizedShape::Binder { index: 0, kind: BinderKind::Type }, annotation: None };
        assert_eq!(generic.semantic_parity(&generic), Err(ParityError::UnscopedBinder));
        let captured = NormalizedType { shape: NormalizedShape::Capture { index: 0, kind: BinderKind::Type }, annotation: None };
        assert_eq!(captured.semantic_parity(&captured), Err(ParityError::CapturedRelationship));
        let template = NormalizedType { shape: NormalizedShape::BuiltinParameter("T".to_string()), annotation: None };
        assert_eq!(template.semantic_parity(&template), Err(ParityError::UnscopedBinder));
        let concrete = NormalizedType { shape: NormalizedShape::Atom("Int".to_string()), annotation: Some("Int".to_string()) };
        assert_eq!(concrete.semantic_parity(&concrete), Ok(true));
    }

    #[test]
    fn solved_query_source_callable_alias_keeps_its_original_scheme() {
        let source = "pure identity(value) { value }\nlet alias = identity\nlet chained = alias\nlet integer: Int = alias(7)\nlet text: Str = chained(\"word\")\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
        let principal = query.declaration(*checked.solved.declarations.keys().next().unwrap()).unwrap();
        let aliases = checked.solved.expression_callables.iter().filter(|(_, fact)| fact.scheme.is_some()).map(|(&identity, _)| query.expression_callable(identity).unwrap()).collect::<Vec<_>>();
        assert!(aliases.len() >= 2, "source aliases retain callable facts");
        for alias in aliases {
            assert_eq!(alias.scheme, principal.scheme);
            assert_eq!(alias.semantic_parity(&principal), Ok(true));
        }
        let bindings = checked.solved.bindings.iter().filter(|(_, fact)| fact.scheme.is_some()).map(|(&identity, _)| query.binding(identity).unwrap()).collect::<Vec<_>>();
        assert!(bindings.len() >= 2, "source aliases retain binding schemes");
        assert!(bindings.iter().all(|binding| !binding.mutable && binding.scheme == principal.scheme));
    }

    #[test]
    fn solved_query_generalized_value_descendant_keeps_context_without_new_forall() {
        let source = "type Stats = {blobs: Map[Any]}\npure with_blobs(stats: Stats, blobs: Map[Any]) -> Stats { {blobs} }\npure count() -> Stats { let stats = {blobs: map.empty()}; let blobs: Map[Any] = map.empty(); with_blobs(stats, blobs) }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
        let identity = *checked.solved.expression_value_scopes.keys().find(|identity| checked.solved.operations.contains_key(identity) && !checked.solved.expression_schemes.contains_key(identity)).unwrap();
        let answer = query.expression_scheme(identity).unwrap();
        assert!(answer.quantifiers.is_empty(), "a nested instruction does not inherit its initializer's forall");
        assert!(!query.reveal(identity).unwrap().starts_with("forall"));
        assert!(matches!(answer.ty.shape(), NormalizedShape::Map(_, _)));
        assert_eq!(answer.ty.annotation_source(), None);
        assert_eq!(answer.ty.semantic_parity(&answer.ty), Err(ParityError::UnscopedBinder), "a contextual view cannot escape as an independent closed contract");
    }

    #[test]
    fn solved_query_safe_value_schemes_override_the_lexical_function_scope() {
        let source = "pure paths(values: List[Path]) -> Int { values.len() }\npure integers(values: List[Int]) -> Int { values.len() }\npure inspect() -> Int { let entries = []; let alias = entries; paths(entries) + integers(alias) }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
        assert!(!checked.solved.expression_schemes.is_empty(), "value generalization publishes its source scope");
        let value_principals = checked.solved.expression_schemes.keys().copied().filter(|identity| !checked.solved.expression_callables.contains_key(identity)).collect::<Vec<_>>();
        assert_eq!(value_principals.len(), 2, "the empty initializer and its immutable alias publish their own principal values");
        for identity in value_principals {
            let principal = query.expression_scheme(identity).unwrap();
            assert_eq!(principal.to_string(), "forall T0. List[T0]");
            assert_eq!(query.reveal(identity).unwrap(), principal.to_string());
            assert!(principal.ty.annotation_source().is_none());
            assert_eq!(principal.semantic_parity(&principal), Ok(true));
        }
        let generalized = checked.solved.bindings.keys().copied().filter_map(|identity| query.binding(identity).ok()).filter(|binding| !binding.scheme.quantifiers.is_empty()).collect::<Vec<_>>();
        assert_eq!(generalized.len(), 2, "the original and immutable alias retain independent uses");
        assert_eq!(generalized[0].scheme.semantic_parity(&generalized[1].scheme), Ok(true));
    }

    #[test]
    fn solved_query_foreign_owner_cannot_reinterpret_names_after_arena_drop() {
        let (solved, declaration, label) = {
            let source = "pure query_lifetime_identity(query_lifetime_parameter) { query_lifetime_parameter }\n";
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let (&identity, declaration) = checked.solved.declarations.iter().next().unwrap();
            let TypeNode::Arrow(arrow) = checked.solved.graph.node(declaration.signature).unwrap() else { panic!("complete callable retains parameter identity") };
            let label = arrow.params[0].label;
            (checked.solved, identity, label)
        };
        let foreign = SymbolOwner::new();
        let mut reused = false;
        for index in 0..256 {
            let name = foreign.intern(&format!("replacement_query_symbol_{index}"));
            if name.symbol() == label.symbol() { reused = true; break; }
        }
        let result = SolvedQuery::new(&solved, &foreign).declaration(declaration);
        if result.is_ok() { assert!(reused, "foreign normalization accepted only after raw symbol reuse"); }
        assert_eq!(result, Err(QueryError::ForeignSymbol));
        let retained = SolvedQuery::new(&solved, solved.symbol_owner()).declaration(declaration).unwrap();
        assert_eq!(retained.to_string(), "forall T0. pure(query_lifetime_parameter: T0) [] -> T0");
    }

    #[test]
    fn solved_query_recursive_components_normalize_binders_by_relationship() {
        let first = "pure first(left, right) { if true { second(right, left) } else { left } }\n";
        let second = "pure second(left, right) { if true { first(right, left) } else { right } }\n";
        let mut answers = Vec::new();
        for source in [format!("{first}{second}"), format!("{second}{first}")] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
            let mut declarations = checked.solved.declarations.keys().map(|&identity| (parsed.arena.arena.function_def(identity.declaration).name.to_string(), query.declaration(identity).unwrap())).collect::<Vec<_>>();
            declarations.sort_by(|left, right| left.0.cmp(&right.0));
            answers.push(declarations);
        }
        for (left, right) in answers[0].iter().zip(&answers[1]) {
            assert_eq!(left.0, right.0);
            assert_eq!(left.1.semantic_parity(&right.1), Ok(true), "{} vs {}", left.1, right.1);
        }
    }

    #[test]
    fn solved_query_row_binder_order_uses_flattened_semantic_fields() {
        use crate::sema::check::SolvedCallable;
        use crate::sema::inference::{Arrow, Generalization, InferenceContext, Parameter, RowField, ScopedRoot};
        use crate::syntax::arena::{BlockId, FunctionDefId};
        let mut answers = Vec::new();
        for split in [false, true] {
            let symbols = SymbolOwner::new();
            let mut graph = InferenceContext::default();
            let span = crate::source::Span::new(SourceId::new(0), 0, 0);
            let a = graph.fresh(1, span).unwrap();
            let z = graph.fresh(1, span).unwrap();
            let a_field = RowField { label: symbols.intern("a"), ty: a };
            let z_field = RowField { label: symbols.intern("z"), ty: z };
            let row = if split {
                let tail = graph.fresh_row(1, span).unwrap();
                let split = graph.row(vec![z_field], Some(tail)).unwrap();
                let complete = graph.row(vec![a_field, z_field], None).unwrap();
                let split_record = graph.record(split).unwrap();
                let complete_record = graph.record(complete).unwrap();
                let reason = graph.reason(span, None).unwrap();
                graph.unify(split_record, complete_record, reason).unwrap();
                split
            } else { graph.row(vec![a_field, z_field], None).unwrap() };
            let record = graph.record(row).unwrap();
            let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: symbols.intern("value"), ty: record, defaulted: false, rest: false }], result: a, effects: EffectSummary::Closed(EffectSet::default()) }).unwrap();
            let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap();
            let graph = graph.freeze_scoped(&[ScopedRoot { ty: signature, scope: Some(scheme) }]).unwrap();
            let mut solved = SolvedTypes::from_graph(graph, symbols.clone());
            let declaration = DeclarationIdentity { source: SourceId::new(0), namespace: None, declaration: FunctionDefId::from_index(0) };
            solved.declarations.insert(declaration, SolvedCallable { source_requirements: Vec::new(), parameter_producer_flows: Vec::new(), return_producer_flow: None, parameter_producers: vec![Default::default(); 1], return_producers: Default::default(), scheme, signature, body: BlockId::from_index(0), kind: CallableKind::Pure, return_elaboration: ReturnElaboration::Value, effective_effects: EffectSummary::Closed(EffectSet::default()), required_effects: EffectSummary::Closed(EffectSet::default()) });
            answers.push(SolvedQuery::new(&solved, &symbols).declaration(declaration).unwrap());
        }
        assert_eq!(answers[0].semantic_parity(&answers[1]), Ok(true), "{} vs {}", answers[0], answers[1]);
    }

    #[test]
    fn solved_query_operation_failure_projection_keeps_declared_error_binder() {
        use crate::sema::inference::{Arrow, CandidateTemplate, Generalization, InferenceContext, OperationCall, OperationFailureProjection, Parameter, ScopedRoot};
        let symbols = SymbolOwner::new();
        symbols.with_current(|| {
            for projection in [OperationFailureProjection::ReceiverResultError, OperationFailureProjection::ArgumentResultError { argument: 0 }] {
                let mut graph = InferenceContext::default();
                let span = crate::source::Span::new(SourceId::new(0), 0, 0);
                let effects = EffectSummary::Closed(EffectSet::ERROR);
                let errors = graph.atom(Atom::ProcessError).unwrap();
                let integer = graph.atom(Atom::Int).unwrap();
                let mut candidates = Vec::new();
                for (label, payload) in [("consume_int_result", integer), ("consume_str_result", graph.atom(Atom::Str).unwrap())] {
                    let parameter = graph.result(payload, errors).unwrap();
                    let signature = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: symbols.intern("value"), ty: parameter, defaulted: false, rest: false }], result: integer, effects }).unwrap();
                    let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap();
                    candidates.push(graph.register_candidate(CandidateTemplate { failure_projection: Some(projection), identity: symbols.intern(label), public_label: symbols.intern(label), effect_roles: Vec::new(), output_effect_roles: Vec::new(), scheme, has_receiver: matches!(projection, OperationFailureProjection::ReceiverResultError), actual_eligibility: Vec::new(), argument_relations: Vec::new() }).unwrap());
                }
                let family = graph.register_family(&candidates).unwrap();
                let input = graph.fresh(1, span).unwrap();
                let bound = graph.fresh(1, span).unwrap();
                let receiver = matches!(projection, OperationFailureProjection::ReceiverResultError).then_some(input);
                let arguments = if receiver.is_some() { Vec::new() } else { vec![Some(input)] };
                let reason = graph.reason(span, None).unwrap();
                let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: Some(bound), receiver, arguments, result: integer, effects, effect_bindings: Vec::new(), output_effect_bindings: Vec::new() }, reason).unwrap();
                let result = graph.result(integer, bound).unwrap();
                let signature = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: symbols.intern("input"), ty: input, defaulted: false, rest: false }], result, effects }).unwrap();
                let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[requirement]).unwrap();
                let graph = graph.freeze_scoped(&[ScopedRoot { ty: signature, scope: Some(scheme) }]).unwrap();
                let solved = SolvedTypes::from_graph(graph, symbols.clone());
                let query = SolvedQuery::new(&solved, &symbols);
                let before = solved.graph.counters().instantiations;
                let mut view = View::new(&query, Some(scheme));
                let normalized = view.requirement(solved.graph.requirement_template(requirement).unwrap()).unwrap();
                let NormalizedRequirement::Operation { candidates, declared_error_bound, .. } = &normalized else { panic!("exact finite operation family is retained") };
                assert_eq!(*declared_error_bound, Some(NormalizedShape::Binder { index: 1, kind: BinderKind::Type }));
                let expected = match projection { OperationFailureProjection::ReceiverResultError => NormalizedOperationFailureProjection::ReceiverResultError, OperationFailureProjection::ArgumentResultError { argument } => NormalizedOperationFailureProjection::ArgumentResultError { argument } };
                assert!(candidates.iter().all(|candidate| candidate.failure_projection == Some(expected)));
                let principal = view.scheme(signature, Some(scheme)).unwrap();
                assert_eq!(principal.requirements, vec![normalized.clone()], "the declared completion bound survives principal publication");
                let rendered = principal.to_string();
                let origin = match projection { OperationFailureProjection::ReceiverResultError => "failure receiver.error", OperationFailureProjection::ArgumentResultError { .. } => "failure argument[0].error" };
                assert!(rendered.contains(origin) && rendered.contains("error bound T1"), "{rendered}");
                assert_eq!(principal.semantic_parity(&principal), Ok(true));
                let mut missing = principal.clone();
                let [NormalizedRequirement::Operation { declared_error_bound, .. }] = missing.requirements.as_mut_slice() else { unreachable!() };
                *declared_error_bound = None;
                assert_eq!(principal.semantic_parity(&missing), Ok(false), "dropping the declared completion bound changes the contract");
                let mut escaped = principal.clone();
                let [NormalizedRequirement::Operation { declared_error_bound, .. }] = escaped.requirements.as_mut_slice() else { unreachable!() };
                *declared_error_bound = Some(NormalizedShape::Binder { index: 99, kind: BinderKind::Type });
                assert_eq!(escaped.semantic_parity(&escaped), Err(ParityError::UnscopedBinder), "completion bounds use the same exact callable binder scope");
                let mut changed = principal.clone();
                let [NormalizedRequirement::Operation { candidates, .. }] = changed.requirements.as_mut_slice() else { unreachable!() };
                candidates[0].failure_projection = None;
                assert_eq!(principal.semantic_parity(&changed), Ok(false), "a candidate failure source is part of its authority");
                assert_eq!(solved.graph.counters().instantiations, before);
            }
        });
    }

    #[test]
    fn solved_query_canonical_eligibility_keeps_residual_item_and_command_domains() {
        use crate::sema::inference::{Arrow, Generalization, InferenceContext, Parameter, ScopedRoot};
        use crate::syntax::arena::ExprId;
        let symbols = SymbolOwner::new();
        symbols.with_current(|| {
            let cases = [
                (Eligibility::YieldItem, EligibilityPredicate::YieldItem, "YieldItem"),
                (Eligibility::CommandTarget, EligibilityPredicate::CommandTarget, "CommandTarget"),
                (Eligibility::CommandArgv, EligibilityPredicate::CommandArgv, "CommandArgv"),
                (Eligibility::Error, EligibilityPredicate::Error, "Error"),
                (Eligibility::Display, EligibilityPredicate::Display, "Display"),
                (Eligibility::ArgvExpansion, EligibilityPredicate::ArgvExpansion, "ArgvExpansion"),
            ];
            let mut answers = Vec::new();
            for (predicate, expected, spelling) in cases {
                let mut graph = InferenceContext::default();
                let span = crate::source::Span::new(SourceId::new(0), 0, 0);
                let item = graph.fresh(1, span).unwrap();
                let reason = graph.reason(span, None).unwrap();
                let requirement = graph.require_eligibility(predicate, item, reason).unwrap();
                let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: symbols.intern("value"), ty: item, defaulted: false, rest: false }], result: item, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
                let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[requirement]).unwrap();
                let graph = graph.freeze_scoped(&[ScopedRoot { ty: signature, scope: Some(scheme) }]).unwrap();
                let mut solved = SolvedTypes::from_graph(graph, symbols.clone());
                let expression = ExpressionIdentity { source: SourceId::new(0), namespace: None, expression: ExprId::from_index(0) };
                solved.expressions.insert(expression, signature);
                solved.expression_schemes.insert(expression, scheme);
                let before = solved.graph.counters().instantiations;
                let answer = SolvedQuery::new(&solved, &symbols).expression_scheme(expression).unwrap();
                assert_eq!(answer.requirements, vec![NormalizedRequirement::Eligibility { predicate: expected, ty: NormalizedShape::Binder { index: 0, kind: BinderKind::Type } }]);
                assert!(answer.to_string().contains(&format!("{spelling}(T0)")), "{answer}");
                assert_eq!(answer.quantifiers.len(), 1, "eligibility is a residual domain, not a guessed concrete type");
                assert_eq!(answer.semantic_parity(&answer), Ok(true));
                assert_eq!(solved.graph.counters().instantiations, before);
                answers.push(answer);
            }
            for (index, left) in answers.iter().enumerate() {
                for right in &answers[index + 1..] { assert_eq!(left.semantic_parity(right), Ok(false), "distinct operand domains never collapse in normalized parity"); }
            }
        });
    }

    #[test]
    fn solved_query_residual_registry_operations_display_public_labels() {
        use crate::sema::check::SolvedCallable;
        use crate::sema::inference::{Arrow, Generalization, InferenceContext, OperationCall, Parameter, ScopedRoot};
        use crate::sema::registry_graph::RegistryGraph;
        use crate::syntax::arena::{BlockId, FunctionDefId};
        let symbols = SymbolOwner::new();
        symbols.with_current(|| {
            let mut graph = InferenceContext::default();
            let span = crate::source::Span::new(SourceId::new(0), 0, 0);
            let mut registry = RegistryGraph::default();
            let family = registry.method_family(&mut graph, "len", span).unwrap();
            let receiver = graph.fresh(1, span).unwrap();
            let result = graph.atom(Atom::Int).unwrap();
            let reason = graph.reason(span, None).unwrap();
            let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings: Vec::new(), receiver: Some(receiver), arguments: Vec::new(), result, effects: EffectSummary::Closed(EffectSet::EMPTY), effect_bindings: Vec::new() }, reason).unwrap();
            let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: symbols.intern("value"), ty: receiver, defaulted: false, rest: false }], result, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
            let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[requirement]).unwrap();
            let graph = graph.freeze_scoped(&[ScopedRoot { ty: signature, scope: Some(scheme) }]).unwrap();
            let mut solved = SolvedTypes::from_graph(graph, symbols.clone());
            let declaration = DeclarationIdentity { source: SourceId::new(0), namespace: None, declaration: FunctionDefId::from_index(0) };
            solved.declarations.insert(declaration, SolvedCallable { source_requirements: vec![requirement], parameter_producer_flows: Vec::new(), return_producer_flow: None, parameter_producers: vec![Default::default(); 1], return_producers: Default::default(), scheme, signature, body: BlockId::from_index(0), kind: CallableKind::Pure, return_elaboration: ReturnElaboration::Value, effective_effects: EffectSummary::Closed(EffectSet::EMPTY), required_effects: EffectSummary::Closed(EffectSet::EMPTY) });
            let normalized = SolvedQuery::new(&solved, &symbols).declaration(declaration).unwrap();
            let rendered = normalized.to_string();
            assert!(rendered.contains("List.len"), "{rendered}");
            assert!(!rendered.contains("registry:"), "canonical implementation descriptors stay in metadata");
            let [NormalizedRequirement::Operation { candidates, .. }] = normalized.scheme.requirements.as_slice() else { panic!("residual operation family remains in principal metadata") };
            assert!(!candidates.is_empty());
            assert!(candidates.iter().all(|candidate| candidate.identity.starts_with("registry:")), "sealed canonical identities remain available for parity");
        });
    }

    #[test]
    fn solved_query_source_stream_reveal_separates_creation_pull_and_cleanup() {
        let source = "stream delayed() [time, env] -> Stream[Int] {\n  defer { let _ = env.get(\"UNREAD_SETTING\") }\n  let _ = time.now()\n  yield 1\n}\nreveal_type(delayed)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions { reveal_types: true, ..Default::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
        let declaration = *checked.solved.declarations.keys().next().unwrap();
        let principal = query.declaration(declaration).unwrap();
        let NormalizedShape::Arrow(arrow) = principal.scheme.ty.shape() else { panic!("stream factory has a checked callable contract") };
        assert_eq!(arrow.effects, NormalizedEffect::Closed(Vec::new()));
        assert_eq!(principal.effective_effects, Some(NormalizedEffect::Closed(vec!["env".to_string(), "time".to_string()])), "written stream bound remains latent rather than becoming creation effects");
        let suffix = "; creation []; result.pull [time]; result.close [env]";
        assert!(principal.to_string().ends_with(suffix), "{principal}");
        let named = *checked.solved.expression_callables.iter().find(|(_, fact)| fact.declaration == Some(declaration)).unwrap().0;
        assert_eq!(query.expression_callable(named).unwrap().semantic_parity(&principal), Ok(true));
        assert!(query.reveal(named).unwrap().ends_with(suffix));
        assert!(checked.reveal_types.iter().any(|diagnostic| diagnostic.message.ends_with(suffix)), "{:?}", checked.reveal_types);
        assert!(principal.scheme.ty.annotation_source().is_none(), "diagnostic producer metadata is never source annotation syntax");
    }

    #[test]
    fn solved_query_source_addition_keeps_distinct_value_inputs_and_requirement() {
        let source = "pure combine(left, right) { left + right }\nlet combined: List[Int] = combine([1], [2])\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let declaration = *checked.solved.declarations.keys().next().unwrap();
        let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
        let flow = query.declaration_producer_flow(declaration).unwrap();
        let additions = flow.nodes.iter().filter_map(|node| match &node.kind { NormalizedProducerFlowKind::Addition { requirement, left, right } => Some((requirement.as_ref(), *left, *right)), _ => None }).collect::<Vec<_>>();
        assert_eq!(additions.len(), 1, "source addition remains a retained value relationship");
        let (requirement, left, right) = additions[0];
        assert!(matches!(requirement, NormalizedRequirement::Add { .. }));
        assert_ne!(left, right, "equal inferred types never collapse the two source values");
        let parameter = |mut input: u32| {
            for _ in 0..flow.nodes.len() {
                match &flow.nodes[input as usize].kind {
                    NormalizedProducerFlowKind::Join { inputs } if inputs.len() == 1 => input = inputs[0],
                    NormalizedProducerFlowKind::Parameter { index, .. } => return *index,
                    _ => panic!("addition operand must retain its exact parameter lineage: {flow:?}"),
                }
            }
            panic!("parameter lineage must terminate: {flow:?}");
        };
        assert_eq!(parameter(left), 0);
        assert_eq!(parameter(right), 1);
        assert_eq!(flow.semantic_parity(&flow), Ok(true));
    }

    #[test]
    fn solved_query_source_dynamic_splice_keeps_segments_defaults_and_guards_after_ast_drop() {
        let source = "pure pair(first: Int, second: Int = 2) -> Int { first + second }\nlet parts = [7]\nlet first: Int = pair(@parts)\nlet second: Int = pair(@parts, second: 9)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let calls = checked.solved.calls.iter().filter(|(_, call)| call.binding.dynamic.is_some()).map(|(&identity, call)| (identity, call.actual_arguments.len())).collect::<Vec<_>>();
        assert_eq!(calls.len(), 2);
        let noncall = *checked.solved.expressions.keys().find(|identity| !checked.solved.calls.contains_key(identity)).unwrap();
        let symbols = parsed.arena.symbol_owner().clone();
        drop(parsed);
        let query = SolvedQuery::new(&checked.solved, &symbols);
        let before = checked.solved.graph.counters().instantiations;
        for (identity, arguments) in &calls {
            let binding = query.call_binding(*identity).unwrap();
            assert!(binding.supplied_slots.is_empty(), "unknown-length segments are not empty argument lists");
            assert!(binding.default_slots.is_empty());
            assert_eq!(binding.rest_slot, None);
            let dynamic = binding.dynamic.as_ref().unwrap();
            let mut segments = vec![NormalizedInvocationArgumentSegment::DynamicRange { argument: 0, fixed_slots: vec![0, 1], rest_slot: None }];
            if *arguments == 2 { segments.push(NormalizedInvocationArgumentSegment::StaticSlot { argument: 1, slot: 1 }); }
            assert_eq!(dynamic.segments, segments, "source argument order stays independent of formal slot order");
            assert_eq!(dynamic.conditional_default_slots, if *arguments == 1 { vec![1] } else { Vec::new() });
            assert_eq!(dynamic.required_slots, vec![0]);
            assert!(dynamic.runtime_arity_guard && dynamic.runtime_duplicate_guard);
            let original = query.expression(*identity).unwrap();
            assert_eq!(original.shape(), &NormalizedShape::Atom("Int".to_string()));
            assert_eq!(SolvedQuery::new(&checked.solved, &symbols).with_limits(QueryLimits { nodes: 4, ..Default::default() }).call_binding(*identity), Err(QueryError::Limit));
        }
        assert_ne!(query.call_binding(calls[0].0).unwrap(), query.call_binding(calls[1].0).unwrap(), "empty supplied slots do not equate distinct dynamic source plans");
        for limits in [QueryLimits { depth: 1, ..Default::default() }, QueryLimits { text_bytes: 256, ..Default::default() }] {
            assert_eq!(SolvedQuery::new(&checked.solved, &symbols).with_limits(limits).call_binding(calls[0].0), Err(QueryError::Limit));
        }
        assert_eq!(query.call_binding(noncall), Err(QueryError::MissingCall));
        let missing = ExpressionIdentity { source: SourceId::new(u32::MAX as usize), ..calls[0].0 };
        assert_eq!(query.call_binding(missing), Err(QueryError::MissingExpression));
        assert_eq!(checked.solved.graph.counters().instantiations, before);
    }

    #[test]
    fn solved_query_source_comprehension_preserves_generator_qualifier_ordinals_and_inputs() {
        let source = "pure selected(left: List[Int], right: List[Int]) -> List[Int] { [outer + inner for outer in left if outer > 0 for inner in right] }\nlet result: List[Int] = selected([1], [2])\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let clauses = checked.solved.comprehension_operations.keys().copied().collect::<Vec<_>>();
        assert_eq!(clauses.len(), 2, "each generator publishes its own source operation");
        assert_eq!(clauses[0].expression, clauses[1].expression);
        assert_eq!(clauses.iter().map(|clause| clause.qualifier).collect::<Vec<_>>(), vec![0, 2], "the filter keeps its original qualifier position");
        let symbols = parsed.arena.symbol_owner().clone();
        drop(parsed);
        let query = SolvedQuery::new(&checked.solved, &symbols);
        let before = checked.solved.graph.counters().instantiations;
        let mut inputs = Vec::new();
        for clause in &clauses {
            let fact = &checked.solved.comprehension_operations[clause];
            assert!(!checked.solved.operations.values().any(|operation| operation.requirement == fact.operation.requirement), "an iteration relation does not overwrite its iterable expression's operation");
            inputs.push(fact.input_producer_flow);
            let operation = query.comprehension_operation(*clause).unwrap();
            assert_eq!(operation.result, NormalizedShape::Atom("Int".to_string()));
            assert_eq!(operation.actual_arguments, vec![NormalizedShape::List(Box::new(NormalizedShape::Atom("Int".to_string())))]);
            assert_eq!(operation.binding, NormalizedCallBinding { supplied_slots: vec![0], default_slots: Vec::new(), rest_slot: None, dynamic: None });
            assert_eq!(operation.semantic_parity(&operation), Ok(true));
            let flow = query.comprehension_producer_flow(*clause).unwrap();
            let NormalizedProducerFlowSource::Comprehension(identity) = &flow.nodes[flow.root as usize].source else { panic!("item flow preserves its exact comprehension source identity") };
            assert_eq!(identity.expression.source, clause.expression.source);
            assert_eq!(identity.expression.expression, clause.expression.expression);
            assert_eq!(identity.qualifier, clause.qualifier);
            assert_eq!(flow.semantic_parity(&flow), Ok(true));
        }
        assert_ne!(inputs[0], inputs[1], "equal iterable types do not merge distinct generator values");
        let guard = ComprehensionIdentity { qualifier: 1, ..clauses[0] };
        assert_eq!(query.comprehension_operation(guard), Err(QueryError::MissingComprehension));
        assert_eq!(query.comprehension_producer_flow(guard), Err(QueryError::MissingComprehension));
        assert_eq!(checked.solved.graph.counters().instantiations, before);
    }

    #[test]
    fn solved_query_source_for_statement_preserves_iterable_binding_after_ast_drop() {
        let source = "proc visit(values: List[Int]) [] -> Int { for item in values { let _ = item }; 1 }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let statement = *checked.solved.statement_operations.keys().find(|identity| matches!(parsed.arena.arena.stmt(identity.statement).kind, crate::syntax::arena::ArenaStmtKind::For { .. })).unwrap();
        let symbols = parsed.arena.symbol_owner().clone();
        drop(parsed);
        let query = SolvedQuery::new(&checked.solved, &symbols);
        let before = checked.solved.graph.counters().instantiations;
        let answer = query.statement_operation(statement).unwrap();
        assert_eq!(answer.actual_arguments, vec![NormalizedShape::List(Box::new(NormalizedShape::Atom("Int".to_string())))]);
        assert_eq!(answer.result, NormalizedShape::Atom("Int".to_string()));
        assert_eq!(answer.receiver, None);
        assert_eq!(answer.binding, NormalizedCallBinding { supplied_slots: vec![0], default_slots: Vec::new(), rest_slot: None, dynamic: None });
        assert_eq!(answer.effects, NormalizedEffect::Closed(Vec::new()));
        let NormalizedRequirement::Operation { candidates, effect_bindings, .. } = &answer.requirement else { panic!("For retains its canonical iterable relationship") };
        assert!(candidates.iter().any(|candidate| candidate.public_label == "language.iteration.List"));
        assert!(effect_bindings.iter().any(|(role, summary)| *role == NormalizedEffectRole::PullProjection { source: 0, projection: NormalizedEffectProjection::ResultSuccess } && *summary == NormalizedEffect::Closed(Vec::new())));
        assert_eq!(answer.semantic_parity(&answer), Ok(true));
        assert_eq!(checked.solved.graph.counters().instantiations, before);
    }

    #[test]
    fn solved_query_root_value_operation_retains_its_generalized_initializer_scope() {
        let source = "let values = map.empty()\nlet numbers: Map[Int] = values\nlet words: Map[Str] = values\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
        let (&identity, fact) = checked.solved.operations.iter().next().unwrap();
        assert!(fact.caller.is_none(), "a root value has no lexical callable declaration");
        let before = checked.solved.graph.counters().instantiations;
        let answer = query.language_operation(identity).unwrap();
        assert!(answer.caller.is_none());
        let enclosing = answer.enclosing_scheme.as_ref().unwrap();
        assert_eq!(enclosing.quantifiers.len(), 2);
        assert_eq!(enclosing.ty.shape(), &answer.result);
        assert!(matches!(&answer.result, NormalizedShape::Map(key, value) if **key == NormalizedShape::Binder { index: 0, kind: BinderKind::Type } && **value == NormalizedShape::Binder { index: 1, kind: BinderKind::Type }));
        assert!(enclosing.requirements.contains(&answer.requirement), "the principal value retains the original constructor relation");
        let RequirementTemplate::Operation { family, .. } = checked.solved.graph.requirement_template(fact.requirement).unwrap() else { unreachable!() };
        for &candidate in checked.solved.graph.family(family).unwrap() {
            let formal = checked.solved.graph.scheme(checked.solved.graph.candidate(candidate).unwrap().scheme).unwrap();
            assert!(formal.requirements.iter().any(|requirement| matches!(requirement, RequirementTemplate::Eligibility { predicate: Eligibility::MapKey, .. })), "the retained constructor authority owns its key eligibility constraint");
        }
        assert_eq!(answer.semantic_parity(&answer), Ok(true));
        assert_eq!(checked.solved.graph.counters().instantiations, before);
    }

    #[test]
    fn solved_query_generic_operation_keeps_definition_scope_after_independent_calls() {
        let source = "pure length(value) { value.len() }\nlet text_length: Int = length([\"word\"])\nlet list_length: Int = length([1, 2])\n";
        let mut answers = Vec::new();
        for _ in 0..2 {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
            let identity = *checked.solved.operations.keys().next().unwrap();
            let before = checked.solved.graph.counters().instantiations;
            let answer = query.language_operation(identity).unwrap();
            assert_eq!(answer.receiver, Some(NormalizedShape::Binder { index: 0, kind: BinderKind::Type }));
            assert!(answer.actual_arguments.is_empty());
            let enclosing = answer.enclosing_scheme.as_ref().unwrap();
            let NormalizedShape::Arrow(arrow) = enclosing.ty.shape() else { panic!("operation caller retains its principal arrow") };
            assert_eq!(arrow.result.as_ref(), &answer.result, "the result retains the declaration's exact relationship instead of a caller's instantiated type");
            let NormalizedRequirement::Operation { receiver, result, .. } = &answer.requirement else { panic!("generic receiver retains its canonical operation relation") };
            assert_eq!(receiver, &answer.receiver);
            assert_eq!(result, &answer.result);
            assert_eq!(enclosing.quantifiers.len(), 2);
            assert_eq!(answer.semantic_parity(&answer), Ok(true));
            assert_eq!(query.with_limits(QueryLimits { nodes: 1, ..Default::default() }).language_operation(identity), Err(QueryError::Limit));
            assert_eq!(checked.solved.graph.counters().instantiations, before);
            answers.push(answer);
        }
        assert_eq!(answers[0].semantic_parity(&answers[1]), Err(ParityError::ForeignOperationOwner), "matching declaration and expression ordinals do not establish a shared source owner");
    }

    #[test]
    fn solved_query_source_statement_and_expression_operations_preserve_binding_and_scope() {
        let source = "pure selected(value: Int) -> Int { var current = value; current += 2; current * 3 }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.statement_operations.len(), 1);
        assert_eq!(checked.solved.operations.len(), 1);
        let symbols = parsed.arena.symbol_owner().clone();
        drop(parsed);
        let query = SolvedQuery::new(&checked.solved, &symbols);
        let before = checked.solved.graph.counters().instantiations;
        let (&statement, statement_fact) = checked.solved.statement_operations.iter().next().unwrap();
        let (&expression, expression_fact) = checked.solved.operations.iter().next().unwrap();
        let statement_answer = query.statement_operation(statement).unwrap();
        let expression_answer = query.language_operation(expression).unwrap();
        for (answer, fact) in [(&statement_answer, statement_fact), (&expression_answer, expression_fact)] {
            assert_eq!(answer.owner, checked.solved.owner);
            assert_eq!(answer.caller.as_ref().map(|identity| identity.declaration), fact.caller.map(|identity| identity.declaration));
            assert!(answer.enclosing_scheme.is_some());
            assert_eq!(answer.result, NormalizedShape::Atom("Int".to_string()));
            assert_eq!(answer.actual_arguments, vec![NormalizedShape::Atom("Int".to_string()); 2]);
            assert_eq!(answer.effects, NormalizedEffect::Closed(Vec::new()));
            assert_eq!(answer.binding, NormalizedCallBinding { supplied_slots: vec![0, 1], default_slots: Vec::new(), rest_slot: None, dynamic: None });
            assert!(answer.argument_coercions.is_empty());
            assert_eq!(answer.semantic_parity(answer), Ok(true));
            let NormalizedRequirement::Operation { candidates, .. } = &answer.requirement else { panic!("operation retains its exact finite family") };
            assert!(!candidates.is_empty());
        }
        assert_ne!(statement_answer.requirement, expression_answer.requirement, "compound assignment and multiplication retain distinct authorities");
        assert_eq!(statement_answer.semantic_parity(&expression_answer), Ok(false));
        assert_eq!(query.statement_operation(StatementIdentity { statement: crate::syntax::arena::StmtId::from_index(10000), ..statement }), Err(QueryError::MissingStatement));
        assert_eq!(query.language_operation(ExpressionIdentity { expression: crate::syntax::arena::ExprId::from_index(10000), ..expression }), Err(QueryError::MissingExpression));
        let ordinary_expression = *checked.solved.expressions.keys().find(|identity| !checked.solved.operations.contains_key(identity)).unwrap();
        assert_eq!(query.language_operation(ordinary_expression), Err(QueryError::MissingOperation));
        let ordinary_statement = *checked.solved.statements.keys().find(|identity| !checked.solved.statement_operations.contains_key(identity)).unwrap();
        assert_eq!(query.statement_operation(ordinary_statement), Err(QueryError::MissingOperation));
        assert_eq!(checked.solved.graph.counters().instantiations, before, "querying source operations never freshens a scheme");
    }

    #[test]
    fn solved_query_source_stages_keep_pipeline_identity_and_actual_ordinals() {
        let source = "pure increment(value: Int) -> Int { value + 1 }\npure positive(value: Int) -> Bool { value > 0 }\npure selected() -> List[Int] { [1, 2] |> map(increment) |> where(positive) }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let stages = checked.solved.stage_operations.keys().copied().collect::<Vec<_>>();
        assert_eq!(stages.len(), 2, "each actual source stage publishes its own operation");
        assert_eq!(stages[0].pipeline, stages[1].pipeline);
        assert_eq!(stages.iter().map(|stage| stage.index).collect::<Vec<_>>(), vec![0, 1]);
        let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
        let before = checked.solved.graph.counters().instantiations;
        for stage in &stages {
            let flow = query.stage_producer_flow(*stage).unwrap();
            let NormalizedProducerFlowSource::Stage(identity) = &flow.nodes[0].source else { panic!("stage flow retains the source stage identity") };
            assert_eq!(identity.pipeline.source, stage.pipeline.source);
            assert_eq!(identity.pipeline.expression, stage.pipeline.expression);
            assert_eq!(identity.index, stage.index);
            let applications = flow.nodes.iter().filter_map(|node| match &node.kind { NormalizedProducerFlowKind::StageApply { stage, .. } => Some(stage), _ => None }).collect::<Vec<_>>();
            assert!(!applications.is_empty(), "callable stage application remains distinct from an expression call");
            assert_eq!(applications.iter().map(|application| application.index).collect::<Vec<_>>(), vec![0], "mapping contributes its callback result; filtering preserves input items rather than predicate-result provenance");
            assert!(checked.solved.producer_flows.nodes().any(|node| matches!(&node.kind, ProducerFlowKind::StageApply { stage: application, .. } if application == stage)), "each actual callback application remains in the retained source graph");
            let Some(crate::sema::check::StageCallback::Callable { requirement, .. }) = checked.solved.stage_operations[stage].callback else { panic!("source callback retains its exact invocation fact") };
            assert!(matches!(checked.solved.graph.requirement_template(requirement).unwrap(), RequirementTemplate::CallableInvocation { .. }));
            assert!(applications.iter().all(|application| application.pipeline.expression == stage.pipeline.expression && application.index <= stage.index), "earlier pipeline application edges retain their own ordinals");
            let repeated_items = flow.nodes.iter().filter_map(|node| match &node.kind { NormalizedProducerFlowKind::Aggregate { entries } if entries.len() == 2 && entries.iter().all(|entry| entry.path.0 == [NormalizedProducerPathComponent::ListItem]) => Some(entries), _ => None }).collect::<Vec<_>>();
            assert!(!repeated_items.is_empty(), "both original list elements retain their distinct producer inputs at the shared item path");
            assert!(repeated_items.iter().all(|entries| entries[0].input != entries[1].input));
            assert_eq!(flow.semantic_parity(&flow), Ok(true));
        }
        assert_eq!(query.stage_producer_flow(StageIdentity { index: 2, ..stages[0] }), Err(QueryError::MissingStage));
        assert_eq!(checked.solved.graph.counters().instantiations, before, "stage normalization never creates a fresh call instantiation");
    }

    #[test]
    fn solved_query_source_flow_keeps_distinct_formals_after_type_unification() {
        let source = "pure identity(value) { value }\npure choose(select: Bool, left, right) { if select { left } else { right } }\nlet number: Int = identity(7)\nlet chosen: Int = choose(false, 1, 2)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let query = SolvedQuery::new(&checked.solved, parsed.arena.symbol_owner());
        let choice = *checked.solved.declarations.keys().find(|identity| parsed.arena.arena.function_def(identity.declaration).name == "choose").unwrap();
        let answer = query.declaration_producer_flow(choice).unwrap();
        assert!(answer.nodes.iter().any(|node| matches!(node.kind, NormalizedProducerFlowKind::Join { .. })), "branch value relationships remain explicit");
        for index in [1, 2] {
            assert!(answer.nodes.iter().any(|node| matches!(node.kind, NormalizedProducerFlowKind::Parameter { index: found, .. } if found == index)));
        }
        let left = query.parameter_producer_flow(choice, 1).unwrap();
        let right = query.parameter_producer_flow(choice, 2).unwrap();
        assert_eq!(left.scopes, right.scopes, "equal checked types share a normalized signature");
        assert_eq!(left.semantic_parity(&right), Ok(false), "equal types do not equate two formal producer values");
        for &identity in checked.solved.calls.keys() {
            let call = query.expression_producer_flow(identity).unwrap();
            assert!(matches!(call.nodes[0].kind, NormalizedProducerFlowKind::Apply { .. }));
        }
    }

    #[test]
    fn solved_query_symbolic_flow_alpha_normalizes_each_source_scope() {
        use crate::sema::check::{ProducerFlowKind, ProducerFlowSource, SolvedCallable, SolvedExpressionCallable};
        use crate::sema::inference::{Arrow, Generalization, InferenceContext, Parameter};
        use crate::syntax::arena::{BlockId, ExprId, FunctionDefId};
        let symbols = SymbolOwner::new();
        let solved = symbols.with_current(|| {
            let mut facts = SolvedTypes::<InferenceContext>::default();
            let span = crate::source::Span::new(SourceId::new(0), 0, 0);
            let mut declarations = Vec::new();
            for index in 0..2 {
                let item = facts.graph.fresh(1, span).unwrap();
                let stream = facts.graph.stream(item).unwrap();
                let pull = EffectSummary::Variable(facts.graph.fresh_effect_at(1, None).unwrap());
                let empty = EffectSummary::Closed(EffectSet::EMPTY);
                let profile = [(ProducerPath::default(), ProducerEffects { pull, close: empty })].into_iter().collect::<ProducerProfile>();
                let signature = facts.graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: symbols.intern("producer"), ty: stream, defaulted: false, rest: false }], result: stream, effects: empty }).unwrap();
                let scheme = facts.graph.generalize_with_effect_roots(signature, 0, Generalization::Allowed, &[], &[pull]).unwrap();
                let declaration = DeclarationIdentity { source: SourceId::new(0), namespace: None, declaration: FunctionDefId::from_index(index) };
                let parameter = facts.producer_flows.push_fixture(&mut facts.graph, ProducerFlowSource::Parameter { declaration, index: 0 }, ProducerFlowKind::Known(profile.clone())).unwrap();
                facts.declarations.insert(declaration, SolvedCallable { source_requirements: Vec::new(), parameter_producer_flows: vec![parameter], return_producer_flow: None, parameter_producers: vec![profile.clone()], return_producers: profile, scheme, signature, body: BlockId::from_index(index), kind: CallableKind::Pure, return_elaboration: ReturnElaboration::Value, effective_effects: empty, required_effects: empty });
                declarations.push((declaration, scheme, signature, parameter));
            }
            let (callee_declaration, callee_scheme, callee_signature, _) = declarations[0];
            let callee_expression = ExpressionIdentity { source: SourceId::new(0), namespace: None, expression: ExprId::from_index(0) };
            facts.expressions.insert(callee_expression, callee_signature);
            facts.expression_callables.insert(callee_expression, SolvedExpressionCallable { signature: callee_signature, scheme: Some(callee_scheme), declaration: Some(callee_declaration) });
            facts.expression_schemes.insert(callee_expression, callee_scheme);
            let callee = facts.producer_flows.push_fixture(&mut facts.graph, ProducerFlowSource::Expression(callee_expression), ProducerFlowKind::Callable { declaration: callee_declaration }).unwrap();
            facts.expression_producer_flows.insert(callee_expression, callee);
            let (caller, caller_scheme, caller_signature, parameter) = declarations[1];
            let call = ExpressionIdentity { source: SourceId::new(0), namespace: None, expression: ExprId::from_index(1) };
            let crate::sema::inference::TypeNode::Arrow(arrow) = facts.graph.node(caller_signature).unwrap() else { panic!("caller retains its solved signature") };
            let argument = arrow.params[0].ty;
            let result = arrow.result;
            facts.expressions.insert(call, result);
            facts.expression_owners.insert(call, caller);
            let root = facts.producer_flows.push_fixture(&mut facts.graph, ProducerFlowSource::DeclarationResult(caller), ProducerFlowKind::Apply { call, callee, arguments: vec![parameter] }).unwrap();
            facts.declarations.get_mut(&caller).unwrap().return_producer_flow = Some(root);
            let profile = facts.declarations[&caller].return_producers.clone();
            facts.calls.insert(call, crate::sema::check::SolvedCall { signature: caller_signature, declaration: Some(callee_declaration), caller: Some(caller), requirements: Vec::new(), requirement_origins: Vec::new(), substitutions: facts.graph.scheme_type_binders(caller_scheme).unwrap(), effect_substitutions: facts.graph.scheme_effect_binders(caller_scheme).unwrap(), actual_arguments: vec![argument], binding: crate::sema::check::CallBinding { supplied_slots: vec![0], default_slots: Vec::new(), rest_slot: None, dynamic: None }, argument_producers: vec![profile.clone()], result_producers: profile, result_producer_flow: None });
            facts.freeze_fixture().unwrap()
        });
        let caller = DeclarationIdentity { source: SourceId::new(0), namespace: None, declaration: FunctionDefId::from_index(1) };
        let answer = SolvedQuery::new(&solved, &symbols).declaration_producer_flow(caller).unwrap();
        assert_eq!(answer.scopes.len(), 2, "separate declaration scopes remain separate despite equivalent signatures");
        assert_eq!(answer.scopes[0], answer.scopes[1], "allocation SchemeIds do not appear in normalized contracts");
        assert_eq!(answer.scopes[0].quantifiers.len(), 1);
        assert_eq!(answer.scopes[0].effect_quantifiers.len(), 1);
        let NormalizedProducerFlowKind::Apply { callee, arguments, .. } = &answer.nodes[0].kind else { panic!("forwarded producer remains an Apply relationship") };
        assert_ne!(answer.nodes[*callee as usize].scope, answer.nodes[arguments[0] as usize].scope);
        let NormalizedProducerFlowKind::Known(profile) = &answer.nodes[arguments[0] as usize].kind else { panic!("actual argument producer belongs to its caller scope") };
        assert_eq!(profile[&NormalizedProducerPath(Vec::new())].pull, NormalizedEffect::Binder(0));
        assert_eq!(answer.semantic_parity(&answer), Ok(true));
        let query = SolvedQuery::new(&solved, &symbols);
        let suffix = "; creation []; parameter producer.pull E0; parameter producer.close []; result.pull E0; result.close []";
        assert!(query.declaration(caller).unwrap().to_string().ends_with(suffix));
        let named = *solved.expression_callables.keys().next().unwrap();
        assert!(query.reveal(named).unwrap().ends_with(suffix), "named principal reveals preserve published latent profiles");
    }

    #[test]
    fn solved_query_symbolic_binding_snapshot_preserves_versions_and_owner() {
        use crate::sema::check::{ProducerFlowKind, ProducerFlowSource, SolvedBinding};
        use crate::sema::inference::InferenceContext;
        use crate::syntax::arena::{BindingTargetId, ExprId};
        fn fixture() -> (SolvedTypes, SymbolOwner, BindingIdentity, ExpressionIdentity) {
            let symbols = SymbolOwner::new();
            let binding = BindingIdentity { source: SourceId::new(0), namespace: None, target: BindingTargetId::from_index(0) };
            let expression = ExpressionIdentity { source: SourceId::new(0), namespace: None, expression: ExprId::from_index(0) };
            let solved = symbols.with_current(|| {
                let mut facts = SolvedTypes::<InferenceContext>::default();
                let item = facts.graph.atom(Atom::Int).unwrap();
                let stream = facts.graph.stream(item).unwrap();
                facts.bindings.insert(binding, SolvedBinding { ty: stream, scheme: None, owner: None, mutable: true });
                facts.expressions.insert(expression, stream);
                let mut first = None;
                for (version, bits) in [(0, EffectSet::TIME), (1, EffectSet::ENV)] {
                    let profile = [(ProducerPath::default(), ProducerEffects { pull: EffectSummary::Closed(bits), close: EffectSummary::Closed(EffectSet::EMPTY) })].into_iter().collect();
                    let flow = facts.producer_flows.push_fixture(&mut facts.graph, ProducerFlowSource::Binding { identity: binding, version }, ProducerFlowKind::Known(profile)).unwrap();
                    facts.binding_producer_flows.insert((binding, version), flow);
                    if version == 0 { first = Some(flow); }
                }
                let opaque = facts.producer_flows.push_fixture(&mut facts.graph, ProducerFlowSource::Binding { identity: binding, version: 2 }, ProducerFlowKind::Opaque).unwrap();
                facts.binding_producer_flows.insert((binding, 2), opaque);
                let root = facts.producer_flows.push_fixture(&mut facts.graph, ProducerFlowSource::Expression(expression), ProducerFlowKind::CapturedBinding { identity: binding, version: 0, input: first.unwrap() }).unwrap();
                facts.expression_producer_flows.insert(expression, root);
                facts.freeze_fixture().unwrap()
            });
            (solved, symbols, binding, expression)
        }
        let (solved, symbols, binding, expression) = fixture();
        let query = SolvedQuery::new(&solved, &symbols);
        let before = solved.graph.counters().instantiations;
        let captured = query.expression_producer_flow(expression).unwrap();
        assert!(matches!(captured.nodes[0].kind, NormalizedProducerFlowKind::CapturedBinding { version: 0, input: 1, .. }));
        let NormalizedProducerFlowKind::Known(profile) = &captured.nodes[1].kind else { panic!("snapshot retains the original producer permissions") };
        assert_eq!(profile[&NormalizedProducerPath(Vec::new())].pull, NormalizedEffect::Closed(vec!["time".to_string()]));
        let current = query.binding_producer_flow(binding, 1).unwrap();
        let NormalizedProducerFlowKind::Known(profile) = &current.nodes[0].kind else { panic!("later binding version has its own permissions") };
        assert_eq!(profile[&NormalizedProducerPath(Vec::new())].pull, NormalizedEffect::Closed(vec!["env".to_string()]));
        let opaque = query.binding_producer_flow(binding, 2).unwrap();
        assert!(matches!(opaque.nodes[0].kind, NormalizedProducerFlowKind::Opaque), "unknown permissions never become an empty producer flow");
        let mut empty = opaque.clone();
        empty.nodes[0].kind = NormalizedProducerFlowKind::Empty;
        assert_eq!(opaque.semantic_parity(&empty), Ok(false));
        assert_eq!(query.binding_producer_flow(binding, 3), Err(QueryError::MissingProducerFlow));
        assert_eq!(SolvedQuery::new(&solved, &symbols).with_limits(QueryLimits { depth: 0, ..Default::default() }).expression_producer_flow(expression), Err(QueryError::Limit));
        assert_eq!(solved.graph.counters().instantiations, before, "flow queries do not retrain the graph");
        let (other, other_symbols, _, other_expression) = fixture();
        let other = SolvedQuery::new(&other, &other_symbols).expression_producer_flow(other_expression).unwrap();
        assert_eq!(captured.semantic_parity(&other), Err(ParityError::ForeignProducerOwner), "matching numeric source identities from another graph cannot certify a snapshot");
    }

    #[test]
    fn solved_query_source_record_update_preserves_base_ordered_paths_and_shared_values() {
        let source = "pure changed(base, value) { {...base, z.rows: value, a.rows: value} }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let identity = *checked.solved.record_updates.keys().next().unwrap();
        let symbols = parsed.arena.symbol_owner().clone();
        drop(parsed);
        let before = checked.solved.graph.counters().instantiations;
        let query = SolvedQuery::new(&checked.solved, &symbols);
        let flow = query.expression_producer_flow(identity).unwrap();
        let root = &flow.nodes[flow.root as usize];
        let NormalizedProducerFlowSource::Expression(found) = &root.source else { panic!("overlay retains its exact source expression") };
        assert_eq!(found.expression, identity.expression);
        assert_eq!(found.source, identity.source);
        let NormalizedProducerFlowKind::RecordUpdate { base, replacements } = &root.kind else { panic!("record replacement is an overlay over an existing base, not an aggregate") };
        assert_eq!(replacements.iter().map(|replacement| &replacement.path).collect::<Vec<_>>(), vec![
            &NormalizedProducerPath(vec![NormalizedProducerPathComponent::RecordField("z".to_string()), NormalizedProducerPathComponent::RecordField("rows".to_string())]),
            &NormalizedProducerPath(vec![NormalizedProducerPathComponent::RecordField("a".to_string()), NormalizedProducerPathComponent::RecordField("rows".to_string())]),
        ], "authored replacement order stays distinct from record field sorting");
        let parameter = |mut input: u32| {
            for _ in 0..flow.nodes.len() {
                match &flow.nodes[input as usize].kind {
                    NormalizedProducerFlowKind::Join { inputs } if inputs.len() == 1 => input = inputs[0],
                    NormalizedProducerFlowKind::Parameter { index, .. } => return (input, *index),
                    _ => panic!("overlay input retains exact parameter ancestry: {flow:?}"),
                }
            }
            panic!("parameter ancestry must terminate");
        };
        assert_eq!(parameter(*base).1, 0);
        assert_eq!(parameter(replacements[0].input).1, 1);
        assert_eq!(parameter(replacements[0].input), parameter(replacements[1].input), "two authored values share their original parameter without cloning it");
        assert_eq!(flow.semantic_parity(&flow), Ok(true));
        let mut aggregate = flow.clone();
        aggregate.nodes[flow.root as usize].kind = NormalizedProducerFlowKind::Aggregate { entries: replacements.clone() };
        assert_eq!(flow.semantic_parity(&aggregate), Ok(false), "an aggregate cannot stand in for base-path replacement");
        let mut reordered = flow.clone();
        let NormalizedProducerFlowKind::RecordUpdate { replacements, .. } = &mut reordered.nodes[flow.root as usize].kind else { unreachable!() };
        replacements.reverse();
        assert_eq!(flow.semantic_parity(&reordered), Ok(false), "replacement order is retained plan metadata");
        assert_eq!(SolvedQuery::new(&checked.solved, &symbols).with_limits(QueryLimits { depth: 0, ..Default::default() }).expression_producer_flow(identity), Err(QueryError::Limit));
        assert_eq!(checked.solved.graph.counters().instantiations, before);
    }

    #[test]
    fn solved_query_symbolic_flow_preserves_parameter_identity_and_shared_edges() {
        use crate::sema::check::{ProducerFlowField, ProducerFlowKind, ProducerFlowSource, SolvedCallable};
        use crate::sema::inference::{Arrow, Generalization, InferenceContext, Parameter, RowField};
        use crate::syntax::arena::{BlockId, FunctionDefId};
        let symbols = SymbolOwner::new();
        let solved = symbols.with_current(|| {
            let mut facts = SolvedTypes::<InferenceContext>::default();
            let item = facts.graph.atom(Atom::Int).unwrap();
            let stream = facts.graph.stream(item).unwrap();
            let a = symbols.intern("a");
            let z = symbols.intern("z");
            let row = facts.graph.row(vec![RowField { label: z, ty: stream }, RowField { label: a, ty: stream }], None).unwrap();
            let result = facts.graph.record(row).unwrap();
            let signature = facts.graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: symbols.intern("first"), ty: stream, defaulted: false, rest: false }, Parameter { label: symbols.intern("second"), ty: stream, defaulted: false, rest: false }], result, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
            let scheme = facts.graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap();
            let declaration = DeclarationIdentity { source: SourceId::new(0), namespace: None, declaration: FunctionDefId::from_index(0) };
            let first = facts.producer_flows.push_fixture(&mut facts.graph, ProducerFlowSource::Parameter { declaration, index: 0 }, ProducerFlowKind::Parameter { declaration, index: 0 }).unwrap();
            let second = facts.producer_flows.push_fixture(&mut facts.graph, ProducerFlowSource::Parameter { declaration, index: 1 }, ProducerFlowKind::Parameter { declaration, index: 1 }).unwrap();
            let root = facts.producer_flows.push_fixture(&mut facts.graph, ProducerFlowSource::DeclarationResult(declaration), ProducerFlowKind::Aggregate { entries: vec![ProducerFlowField { path: ProducerPath(vec![ProducerPathComponent::RecordField(z)]), input: first }, ProducerFlowField { path: ProducerPath(vec![ProducerPathComponent::RecordField(a)]), input: first }] }).unwrap();
            facts.declarations.insert(declaration, SolvedCallable { source_requirements: Vec::new(), parameter_producer_flows: vec![first, second], return_producer_flow: Some(root), parameter_producers: vec![Default::default(); 2], return_producers: Default::default(), scheme, signature, body: BlockId::from_index(0), kind: CallableKind::Pure, return_elaboration: ReturnElaboration::Value, effective_effects: EffectSummary::Closed(EffectSet::EMPTY), required_effects: EffectSummary::Closed(EffectSet::EMPTY) });
            facts.freeze_fixture().unwrap()
        });
        let declaration = *solved.declarations.keys().next().unwrap();
        let answer = SolvedQuery::new(&solved, &symbols).declaration_producer_flow(declaration);
        assert!(answer.is_ok(), "a published symbolic result flow must remain queryable: {answer:?}");
        let answer = answer.unwrap();
        assert_eq!(answer.nodes.len(), 2, "one shared parameter node is retained once");
        let NormalizedProducerFlowKind::Aggregate { entries } = &answer.nodes[0].kind else { panic!("result aggregate stays explicit") };
        assert_eq!(entries[0].path, NormalizedProducerPath(vec![NormalizedProducerPathComponent::RecordField("a".to_string())]));
        assert_eq!(entries[0].input, entries[1].input, "equal types do not manufacture another parameter edge");
        assert!(matches!(answer.nodes[entries[0].input as usize].kind, NormalizedProducerFlowKind::Parameter { index: 0, .. }));
        assert_eq!(answer.semantic_parity(&answer), Ok(true));
    }

    #[test]
    fn solved_query_producer_field_names_precede_latent_binder_numbering() {
        use crate::sema::check::{ProducerEffects, ProducerPath, ProducerPathComponent, SolvedCallable};
        use crate::sema::inference::{Arrow, Generalization, InferenceContext, RowField, ScopedRoot};
        use crate::syntax::arena::{BlockId, FunctionDefId};
        let mut answers = Vec::new();
        for reversed in [false, true] {
            let symbols = SymbolOwner::new();
            let first = symbols.intern(if reversed { "z" } else { "a" });
            let second = symbols.intern(if reversed { "a" } else { "z" });
            let (a, z) = if reversed { (second, first) } else { (first, second) };
            let mut graph = InferenceContext::default();
            let item = graph.atom(Atom::Int).unwrap();
            let stream = graph.stream(item).unwrap();
            let row = graph.row(vec![RowField { label: z, ty: stream }, RowField { label: a, ty: stream }], None).unwrap();
            let result = graph.record(row).unwrap();
            let a_pull = EffectSummary::Variable(graph.fresh_effect_at(1, Some(EffectSet::TIME)).unwrap());
            let z_pull = EffectSummary::Variable(graph.fresh_effect_at(1, Some(EffectSet::FS)).unwrap());
            let profile = [(ProducerPath(vec![ProducerPathComponent::RecordField(a)]), ProducerEffects { pull: a_pull, close: EffectSummary::Closed(EffectSet::EMPTY) }), (ProducerPath(vec![ProducerPathComponent::RecordField(z)]), ProducerEffects { pull: z_pull, close: EffectSummary::Closed(EffectSet::EMPTY) })].into_iter().collect::<ProducerProfile>();
            let roots = profile.values().map(|effects| effects.pull).collect::<Vec<_>>();
            let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: Vec::new(), result, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
            let scheme = graph.generalize_with_effect_roots(signature, 0, Generalization::Allowed, &[], &roots).unwrap();
            let graph = graph.freeze_scoped(&[ScopedRoot { ty: signature, scope: Some(scheme) }]).unwrap();
            let mut solved = SolvedTypes::from_graph(graph, symbols.clone());
            let declaration = DeclarationIdentity { source: SourceId::new(0), namespace: None, declaration: FunctionDefId::from_index(0) };
            solved.declarations.insert(declaration, SolvedCallable { source_requirements: Vec::new(), parameter_producer_flows: Vec::new(), return_producer_flow: None, parameter_producers: Vec::new(), return_producers: profile, scheme, signature, body: BlockId::from_index(0), kind: CallableKind::Pure, return_elaboration: ReturnElaboration::Value, effective_effects: EffectSummary::Closed(EffectSet::EMPTY), required_effects: EffectSummary::Closed(EffectSet::EMPTY) });
            answers.push(SolvedQuery::new(&solved, &symbols).declaration(declaration).unwrap());
        }
        assert_eq!(answers[0].semantic_parity(&answers[1]), Ok(true), "field spelling order determines latent binder relationships");
        let profile = answers[0].return_producers.as_ref().unwrap();
        assert_eq!(profile[&NormalizedProducerPath(vec![NormalizedProducerPathComponent::RecordField("a".to_string())])].pull, NormalizedEffect::Binder(0));
        assert_eq!(profile[&NormalizedProducerPath(vec![NormalizedProducerPathComponent::RecordField("z".to_string())])].pull, NormalizedEffect::Binder(1));
        assert_eq!(answers[0].scheme.effect_quantifiers[0].upper, Some(vec!["time".to_string()]));
        assert_eq!(answers[0].scheme.effect_quantifiers[1].upper, Some(vec!["fs".to_string()]));
    }

    #[test]
    fn solved_query_producer_profiles_preserve_paths_and_latent_effects() {
        use crate::sema::check::{ProducerEffects, ProducerPath, ProducerPathComponent, SolvedCallable};
        use crate::sema::inference::{Arrow, Generalization, InferenceContext, RowField, ScopedRoot};
        use crate::syntax::arena::{BlockId, FunctionDefId};
        for nested in [false, true] {
            let mut answers = Vec::new();
            for reversed in [false, true] {
                let symbols = SymbolOwner::new();
                let mut graph = InferenceContext::default();
                let item = graph.atom(Atom::Int).unwrap();
                let stream = graph.stream(item).unwrap();
                let optional = graph.optional(stream).unwrap();
                let retained = symbols.intern("retained");
                let row = graph.row(vec![RowField { label: retained, ty: optional }], None).unwrap();
                let record = graph.record(row).unwrap();
                let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: Vec::new(), result: if nested { record } else { stream }, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
                let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap();
                let graph = graph.freeze_scoped(&[ScopedRoot { ty: signature, scope: Some(scheme) }]).unwrap();
                let mut solved = SolvedTypes::from_graph(graph, symbols.clone());
                let declaration = DeclarationIdentity { source: SourceId::new(0), namespace: None, declaration: FunctionDefId::from_index(0) };
                let cleanup = if nested { EffectSet::FS } else { EffectSet::ENV };
                let (pull, close) = if reversed { (cleanup, EffectSet::TIME) } else { (EffectSet::TIME, cleanup) };
                let path = if nested { vec![ProducerPathComponent::RecordField(retained), ProducerPathComponent::OptionalPayload] } else { Vec::new() };
                let profile = [(ProducerPath(path), ProducerEffects { pull: EffectSummary::Closed(pull), close: EffectSummary::Closed(close) })].into_iter().collect();
                solved.declarations.insert(declaration, SolvedCallable { source_requirements: Vec::new(), parameter_producer_flows: Vec::new(), return_producer_flow: None, parameter_producers: Vec::new(), return_producers: profile, scheme, signature, body: BlockId::from_index(0), kind: CallableKind::Pure, return_elaboration: ReturnElaboration::Value, effective_effects: EffectSummary::Closed(EffectSet::EMPTY), required_effects: EffectSummary::Closed(EffectSet::EMPTY) });
                answers.push(SolvedQuery::new(&solved, &symbols).declaration(declaration).unwrap());
            }
            assert_eq!(answers[0].scheme.to_string(), answers[1].scheme.to_string(), "creation signatures remain separate from latent handle permissions");
            let suffix = if nested { "; creation []; result.field(\"retained\").optional.pull [time]; result.field(\"retained\").optional.close [fs]" } else { "; creation []; result.pull [time]; result.close [env]" };
            assert!(answers[0].to_string().ends_with(suffix), "{}", answers[0]);
            assert_eq!(answers[0].semantic_parity(&answers[1]), Ok(false), "pull and close profiles are part of the returned producer contract");
        }
    }

    #[test]
    fn solved_query_effect_quantifier_preserves_derived_calculation_provenance() {
        use crate::sema::check::SolvedCallable;
        use crate::sema::inference::{Arrow, Generalization, InferenceContext, Parameter, ScopedRoot};
        use crate::syntax::arena::{BlockId, FunctionDefId};
        let mut answers = Vec::new();
        for derived in [false, true] {
            let symbols = SymbolOwner::new();
            let mut graph = InferenceContext::default();
            let span = crate::source::Span::new(SourceId::new(0), 0, 0);
            let unit = graph.atom(Atom::Unit).unwrap();
            let input = EffectSummary::Variable(graph.fresh_effect_at(1, None).unwrap());
            let output = EffectSummary::Variable(if derived { graph.fresh_derived_effect_at(1, None).unwrap() } else { graph.fresh_effect_at(1, None).unwrap() });
            let reason = graph.reason(span, None).unwrap();
            graph.include_effects(input, output, reason).unwrap();
            let callback = graph.arrow(Arrow { kind: CallableKind::Proc, params: Vec::new(), result: unit, effects: input }).unwrap();
            let signature = graph.arrow(Arrow { kind: CallableKind::Proc, params: vec![Parameter { label: symbols.intern("callback"), ty: callback, defaulted: false, rest: false }], result: unit, effects: output }).unwrap();
            let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap();
            let graph = graph.freeze_scoped(&[ScopedRoot { ty: signature, scope: Some(scheme) }]).unwrap();
            let mut solved = SolvedTypes::from_graph(graph, symbols.clone());
            let declaration = DeclarationIdentity { source: SourceId::new(0), namespace: None, declaration: FunctionDefId::from_index(0) };
            solved.declarations.insert(declaration, SolvedCallable { source_requirements: Vec::new(), parameter_producer_flows: Vec::new(), return_producer_flow: None, parameter_producers: vec![Default::default(); 1], return_producers: Default::default(), scheme, signature, body: BlockId::from_index(0), kind: CallableKind::Proc, return_elaboration: ReturnElaboration::Value, effective_effects: output, required_effects: output });
            answers.push(SolvedQuery::new(&solved, &symbols).declaration(declaration).unwrap());
        }
        assert_eq!(answers[0].scheme.effect_quantifiers.iter().map(|quantifier| quantifier.derived).collect::<Vec<_>>(), vec![false, false]);
        assert_eq!(answers[1].scheme.effect_quantifiers.iter().map(|quantifier| quantifier.derived).collect::<Vec<_>>(), vec![false, true]);
        assert_eq!(answers[0].to_string(), answers[1].to_string(), "display syntax can remain equal while calculation contracts differ");
        assert_eq!(answers[0].semantic_parity(&answers[1]), Ok(false), "a minimally derived output differs from an unconstrained effect input");
    }

    #[test]
    fn solved_query_masked_effect_inclusion_keeps_permissions_and_binder_relationships() {
        use crate::sema::check::SolvedCallable;
        use crate::sema::inference::{Arrow, Generalization, InferenceContext, Parameter};
        use crate::syntax::arena::{BlockId, FunctionDefId};
        let mut answers = Vec::new();
        for reversed in [false, true] {
            let symbols = SymbolOwner::new();
            let solved = symbols.with_current(|| {
                let mut facts = SolvedTypes::<InferenceContext>::default();
                let graph = &mut facts.graph;
                let span = crate::source::Span::new(SourceId::new(0), 0, 0);
                let upper = EffectSet(EffectSet::TIME.0 | EffectSet::ERROR.0);
                let first = EffectSummary::Variable(graph.fresh_effect_at(1, Some(if reversed { EffectSet::TIME } else { upper })).unwrap());
                let second = EffectSummary::Variable(graph.fresh_effect_at(1, Some(if reversed { upper } else { EffectSet::TIME })).unwrap());
                let (actual, expected) = if reversed { (second, first) } else { (first, second) };
                let unit = graph.atom(Atom::Unit).unwrap();
                let empty = EffectSummary::Closed(EffectSet::EMPTY);
                let callback = graph.arrow(Arrow { kind: CallableKind::Proc, params: Vec::new(), result: unit, effects: actual }).unwrap();
                let receiver = graph.arrow(Arrow { kind: CallableKind::Proc, params: Vec::new(), result: unit, effects: expected }).unwrap();
                let reason = graph.reason(span, None).unwrap();
                let requirement = graph.include_effects_masked(actual, expected, EffectSet::ERROR, reason).unwrap();
                let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: symbols.intern("callback"), ty: callback, defaulted: false, rest: false }, Parameter { label: symbols.intern("receiver"), ty: receiver, defaulted: false, rest: false }], result: unit, effects: empty }).unwrap();
                let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[requirement]).unwrap();
                let declaration = DeclarationIdentity { source: SourceId::new(0), namespace: None, declaration: FunctionDefId::from_index(0) };
                facts.declarations.insert(declaration, SolvedCallable { source_requirements: vec![requirement], parameter_producer_flows: Vec::new(), return_producer_flow: None, parameter_producers: vec![Default::default(); 2], return_producers: Default::default(), scheme, signature, body: BlockId::from_index(0), kind: CallableKind::Pure, return_elaboration: ReturnElaboration::Value, effective_effects: empty, required_effects: empty });
                facts.freeze_fixture().unwrap()
            });
            let declaration = *solved.declarations.keys().next().unwrap();
            answers.push(SolvedQuery::new(&solved, &symbols).declaration(declaration).unwrap());
        }
        assert_eq!(answers[0].semantic_parity(&answers[1]), Ok(true));
        let [NormalizedRequirement::EffectInclusion { actual, expected, excluded }] = answers[0].scheme.requirements.as_slice() else { panic!("masked inclusion remains a quantified relationship") };
        assert_eq!(actual, &NormalizedEffect::Binder(0));
        assert_eq!(expected, &NormalizedEffect::Binder(1));
        assert_eq!(excluded, &["error".to_string()]);
        assert!(answers[0].to_string().contains("E0 without [error] <= E1"));
        let mut changed = answers[0].clone();
        let NormalizedRequirement::EffectInclusion { actual, .. } = &mut changed.scheme.requirements[0] else { unreachable!() };
        *actual = NormalizedEffect::Unknown;
        assert_eq!(answers[0].semantic_parity(&changed), Ok(false), "unknown permissions remain distinct from the quantified input");
        let NormalizedRequirement::EffectInclusion { excluded, .. } = &mut changed.scheme.requirements[0] else { unreachable!() };
        excluded.clear();
        assert!(changed.to_string().contains("[unknown effects] without [] <= E1"));
    }

    #[test]
    fn solved_query_callable_invocation_preserves_actual_modes_without_formal_labels() {
        use crate::sema::check::SolvedCallable;
        use crate::sema::inference::{Arrow, CallableDomain, Generalization, InferenceContext, InvocationArgument, InvocationArgumentKind, InvocationCall, Parameter};
        use crate::syntax::arena::{BlockId, FunctionDefId};
        let mut answers = Vec::new();
        for reversed in [false, true] {
            let symbols = SymbolOwner::new();
            let solved = symbols.with_current(|| {
                let mut facts = SolvedTypes::<InferenceContext>::default();
                let graph = &mut facts.graph;
                let span = crate::source::Span::new(SourceId::new(0), 0, 0);
                let first = graph.fresh(1, span).unwrap();
                let second = graph.fresh(1, span).unwrap();
                let (callable, value) = if reversed { (second, first) } else { (first, second) };
                let result = graph.fresh(1, span).unwrap();
                let items = graph.list(value).unwrap();
                let empty = EffectSummary::Closed(EffectSet::EMPTY);
                let reason = graph.reason(span, None).unwrap();
                let requirement = graph.require_callable_invocation(InvocationCall { callable, arguments: vec![InvocationArgument { kind: InvocationArgumentKind::Positional, ty: value }, InvocationArgument { kind: InvocationArgumentKind::Named(symbols.intern("payload")), ty: value }, InvocationArgument { kind: InvocationArgumentKind::PositionalSplice, ty: items }], result, effects: empty, domain: CallableDomain::AnyCallable }, reason).unwrap();
                let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: symbols.intern("callback"), ty: callable, defaulted: false, rest: false }, Parameter { label: symbols.intern("value"), ty: value, defaulted: false, rest: false }], result, effects: empty }).unwrap();
                let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[requirement]).unwrap();
                let declaration = DeclarationIdentity { source: SourceId::new(0), namespace: None, declaration: FunctionDefId::from_index(0) };
                facts.declarations.insert(declaration, SolvedCallable { source_requirements: vec![requirement], parameter_producer_flows: Vec::new(), return_producer_flow: None, parameter_producers: vec![Default::default(); 2], return_producers: Default::default(), scheme, signature, body: BlockId::from_index(0), kind: CallableKind::Pure, return_elaboration: ReturnElaboration::Value, effective_effects: empty, required_effects: empty });
                facts.freeze_fixture().unwrap()
            });
            let declaration = *solved.declarations.keys().next().unwrap();
            answers.push(SolvedQuery::new(&solved, &symbols).declaration(declaration).unwrap());
        }
        assert_eq!(answers[0].semantic_parity(&answers[1]), Ok(true));
        let [NormalizedRequirement::CallableInvocation { callable, arguments, result, effects, domain }] = answers[0].scheme.requirements.as_slice() else { panic!("unknown callable retains its invocation relationship") };
        assert_eq!(callable, &NormalizedShape::Binder { index: 0, kind: BinderKind::Type });
        assert_eq!(result, &NormalizedShape::Binder { index: 2, kind: BinderKind::Type });
        assert_eq!(*domain, NormalizedCallableDomain::AnyCallable);
        assert_eq!(effects, &NormalizedEffect::Closed(Vec::new()));
        assert_eq!(arguments.iter().map(|argument| argument.kind.clone()).collect::<Vec<_>>(), vec![NormalizedInvocationArgumentKind::Positional, NormalizedInvocationArgumentKind::Named("payload".to_string()), NormalizedInvocationArgumentKind::PositionalSplice]);
        assert_eq!(arguments[0].ty, NormalizedShape::Binder { index: 1, kind: BinderKind::Type });
        assert_eq!(arguments[2].ty, NormalizedShape::List(Box::new(arguments[0].ty.clone())));
        assert!(answers[0].to_string().contains("Invoke[pure/proc/stream](T0; T1, payload: T1, ...List[T1]) [] -> T2"));
        assert!(answers[0].scheme.ty.annotation_source().is_none());
        let mut changed = answers[0].clone();
        let NormalizedRequirement::CallableInvocation { domain, .. } = &mut changed.scheme.requirements[0] else { unreachable!() };
        *domain = NormalizedCallableDomain::Pure;
        assert_eq!(answers[0].semantic_parity(&changed), Ok(false), "invocation domains remain semantic metadata");
        let NormalizedRequirement::CallableInvocation { domain, arguments, .. } = &mut changed.scheme.requirements[0] else { unreachable!() };
        *domain = NormalizedCallableDomain::AnyCallable;
        arguments.swap(0, 1);
        assert_eq!(answers[0].semantic_parity(&changed), Ok(false), "source argument order and modes do not become formal parameter names");
    }

    #[test]
    fn solved_query_operation_transfers_keep_paths_shared_inputs_and_uncertainty() {
        use crate::sema::check::{ProducerFlowKind, ProducerFlowOperationAlternative, ProducerFlowOperationTransfer, ProducerFlowSource, SolvedCallable};
        use crate::sema::inference::{ArgumentRelation, Arrow, CandidateTemplate, Generalization, InferenceContext, OperationCall, Parameter, RowField};
        use crate::syntax::arena::{BlockId, FunctionDefId};
        let symbols = SymbolOwner::new();
        let solved = symbols.with_current(|| {
            let mut facts = SolvedTypes::<InferenceContext>::default();
            let graph = &mut facts.graph;
            let span = crate::source::Span::new(SourceId::new(0), 0, 0);
            let z = symbols.intern("z");
            let a = symbols.intern("a");
            let item = graph.atom(Atom::Int).unwrap();
            let stream = graph.stream(item).unwrap();
            let row = graph.row(vec![RowField { label: z, ty: stream }, RowField { label: a, ty: stream }], None).unwrap();
            let record = graph.record(row).unwrap();
            let empty = EffectSummary::Closed(EffectSet::EMPTY);
            let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: symbols.intern("source"), ty: record, defaulted: false, rest: false }], result: record, effects: empty }).unwrap();
            let formal = graph.generalize(signature, 0, Generalization::Allowed, &[]).unwrap();
            let candidate = graph.register_candidate(CandidateTemplate { failure_projection: None, identity: symbols.intern("record-transfer-authority"), public_label: symbols.intern("record.forward"), scheme: formal, has_receiver: false, actual_eligibility: Vec::new(), argument_relations: vec![ArgumentRelation::Assignable], effect_roles: Vec::new(), output_effect_roles: Vec::new() }).unwrap();
            let family = graph.register_family(&[candidate]).unwrap();
            let reason = graph.reason(span, None).unwrap();
            let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![Some(record)], result: record, effects: empty, effect_bindings: Vec::new(), output_effect_bindings: Vec::new() }, reason).unwrap();
            let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[requirement]).unwrap();
            let declaration = DeclarationIdentity { source: SourceId::new(0), namespace: None, declaration: FunctionDefId::from_index(0) };
            let input = facts.producer_flows.push_fixture(&mut facts.graph, ProducerFlowSource::Parameter { declaration, index: 0 }, ProducerFlowKind::Parameter { declaration, index: 0 }).unwrap();
            let transfers = [z, a].into_iter().map(|field| ProducerFlowOperationTransfer { input, input_path: ProducerPath(vec![ProducerPathComponent::RecordField(field)]), output_path: ProducerPath(vec![ProducerPathComponent::RecordField(field)]) }).collect();
            let root = facts.producer_flows.push_fixture(&mut facts.graph, ProducerFlowSource::DeclarationResult(declaration), ProducerFlowKind::Operation { requirement, alternatives: vec![ProducerFlowOperationAlternative { candidate, path: None, transfers, opaque: true }], outputs: ProducerEffects { pull: empty, close: empty } }).unwrap();
            let profile = [(ProducerPath(vec![ProducerPathComponent::RecordField(a)]), ProducerEffects { pull: EffectSummary::Closed(EffectSet::TIME), close: empty }), (ProducerPath(vec![ProducerPathComponent::RecordField(z)]), ProducerEffects { pull: EffectSummary::Closed(EffectSet::ENV), close: empty })].into_iter().collect();
            facts.declarations.insert(declaration, SolvedCallable { source_requirements: vec![requirement], parameter_producer_flows: vec![input], return_producer_flow: Some(root), parameter_producers: vec![profile], return_producers: Default::default(), scheme, signature, body: BlockId::from_index(0), kind: CallableKind::Pure, return_elaboration: ReturnElaboration::Value, effective_effects: empty, required_effects: empty });
            facts.freeze_fixture().unwrap()
        });
        let declaration = *solved.declarations.keys().next().unwrap();
        let query = SolvedQuery::new(&solved, &symbols);
        let flow = query.declaration_producer_flow(declaration).unwrap();
        assert_eq!(flow.nodes.len(), 2, "operation transfer references retain their shared source node once");
        assert!(matches!(flow.nodes[1].kind, NormalizedProducerFlowKind::Parameter { index: 0, .. }));
        let NormalizedProducerFlowKind::Operation { alternatives, .. } = &flow.nodes[0].kind else { panic!("transfers stay attached to their exact operation alternative") };
        assert!(alternatives[0].opaque, "uncertainty is not inferred from an empty role set");
        assert_eq!(alternatives[0].transfers.len(), 2);
        assert_eq!(alternatives[0].transfers[0].output_path, NormalizedProducerPath(vec![NormalizedProducerPathComponent::RecordField("a".to_string())]));
        assert_eq!(alternatives[0].transfers[1].output_path, NormalizedProducerPath(vec![NormalizedProducerPathComponent::RecordField("z".to_string())]));
        assert!(alternatives[0].transfers.iter().all(|transfer| transfer.input == 1 && transfer.input_path == transfer.output_path), "shared input edges and owned field paths remain exact");
        assert_eq!(flow.semantic_parity(&flow), Ok(true));
        let mut changed = flow.clone();
        let NormalizedProducerFlowKind::Operation { alternatives, .. } = &mut changed.nodes[0].kind else { unreachable!() };
        alternatives[0].opaque = false;
        assert_eq!(flow.semantic_parity(&changed), Ok(false));
    }

    #[test]
    fn solved_query_operation_roles_keep_formal_inputs_and_derived_outputs_separate() {
        use crate::sema::check::{ProducerEffects, ProducerFlowKind, ProducerFlowOperationAlternative, ProducerFlowSource, SolvedCallable};
        use crate::sema::inference::{ArgumentRelation, Arrow, CandidateTemplate, EffectRole, EffectRoleReference, Generalization, InferenceContext, OperationCall, Parameter, ProducerRole};
        use crate::syntax::arena::{BlockId, FunctionDefId};
        let symbols = SymbolOwner::new();
        symbols.with_current(|| {
            let mut facts = SolvedTypes::<InferenceContext>::default();
            let graph = &mut facts.graph;
            let span = crate::source::Span::new(SourceId::new(0), 0, 0);
            let unit = graph.atom(Atom::Unit).unwrap();
            let item = graph.atom(Atom::Int).unwrap();
            let stream = graph.stream(item).unwrap();
            let empty = EffectSummary::Closed(EffectSet::EMPTY);
            let formal_pull = EffectSummary::Variable(graph.fresh_effect_at(1, Some(EffectSet::TIME)).unwrap());
            let formal_callback = graph.arrow(Arrow { kind: CallableKind::Proc, params: Vec::new(), result: unit, effects: formal_pull }).unwrap();
            let formal = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: symbols.intern("callback"), ty: formal_callback, defaulted: false, rest: false }], result: stream, effects: empty }).unwrap();
            let scheme = graph.generalize_with_effect_roots(formal, 0, Generalization::Allowed, &[], &[formal_pull, empty]).unwrap();
            let callback_slot = graph.scheme_effect_binder_index(scheme, formal_pull).unwrap().unwrap() as u32;
            let candidate = graph.register_candidate(CandidateTemplate { failure_projection: None, identity: symbols.intern("producer-output-authority"), public_label: symbols.intern("stage.producer"), scheme, has_receiver: false, actual_eligibility: Vec::new(), argument_relations: vec![ArgumentRelation::Assignable], effect_roles: vec![(EffectRole::Callback, EffectRoleReference::Binder(callback_slot)), (EffectRole::Close { source: 0 }, EffectRoleReference::Fixed(EffectSet::EMPTY))], output_effect_roles: vec![(ProducerRole::Pull, 0), (ProducerRole::Close, 1)] }).unwrap();
            let family = graph.register_family(&[candidate]).unwrap();
            let input = EffectSummary::Variable(graph.fresh_effect_at(1, Some(EffectSet::TIME)).unwrap());
            let pull = EffectSummary::Variable(graph.fresh_derived_effect_at(1, Some(EffectSet::TIME)).unwrap());
            let close = EffectSummary::Variable(graph.fresh_derived_effect_at(1, Some(EffectSet::EMPTY)).unwrap());
            let callback = graph.arrow(Arrow { kind: CallableKind::Proc, params: Vec::new(), result: unit, effects: input }).unwrap();
            let reason = graph.reason(span, None).unwrap();
            let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![Some(callback)], result: stream, effects: empty, effect_bindings: vec![(EffectRole::Callback, input), (EffectRole::Close { source: 0 }, empty)], output_effect_bindings: vec![(ProducerRole::Close, close), (ProducerRole::Pull, pull)] }, reason).unwrap();
            let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: symbols.intern("callback"), ty: callback, defaulted: false, rest: false }], result: stream, effects: empty }).unwrap();
            let scheme = graph.generalize_with_effect_roots(signature, 0, Generalization::Allowed, &[requirement], &[input, pull, close]).unwrap();

            let declaration = DeclarationIdentity { source: SourceId::new(0), namespace: None, declaration: FunctionDefId::from_index(0) };
            facts.declarations.insert(declaration, SolvedCallable { source_requirements: vec![requirement], parameter_producer_flows: Vec::new(), return_producer_flow: None, parameter_producers: vec![Default::default(); 1], return_producers: Default::default(), scheme, signature, body: BlockId::from_index(0), kind: CallableKind::Pure, return_elaboration: ReturnElaboration::Value, effective_effects: empty, required_effects: empty });
            let root = facts.producer_flows.push_fixture(&mut facts.graph, ProducerFlowSource::DeclarationResult(declaration), ProducerFlowKind::Operation { requirement, alternatives: vec![ProducerFlowOperationAlternative { candidate, path: None, transfers: Vec::new(), opaque: false }], outputs: ProducerEffects { pull, close } }).unwrap();
            facts.declarations.get_mut(&declaration).unwrap().return_producer_flow = Some(root);
            let solved = facts.freeze_fixture().unwrap();
            let query = SolvedQuery::new(&solved, &symbols);
            let answer = query.declaration(declaration).unwrap();
            let [NormalizedRequirement::Operation { candidates, output_effect_bindings, .. }] = answer.scheme.requirements.as_slice() else { panic!("producer operation remains a principal obligation") };
            assert_eq!(candidates[0].effect_roles, vec![(NormalizedEffectRole::Callback, NormalizedEffectRoleReference::Binder(0)), (NormalizedEffectRole::Close { source: 0 }, NormalizedEffectRoleReference::Fixed(Vec::new()))]);
            assert_eq!(candidates[0].output_effect_roots, vec![NormalizedEffect::Binder(0), NormalizedEffect::Closed(Vec::new())]);
            assert_eq!(candidates[0].output_effect_roles, vec![(NormalizedProducerRole::Pull, 0), (NormalizedProducerRole::Close, 1)]);
            assert_eq!(output_effect_bindings[1], (NormalizedProducerRole::Close, NormalizedEffect::Closed(Vec::new())));
            let (NormalizedProducerRole::Pull, NormalizedEffect::Binder(output)) = output_effect_bindings[0] else { panic!("latent input keeps pull output symbolic") };
            let callback = &answer.scheme.ty.shape().callable_signature().unwrap().parameters[0].ty;
            let input = &callback.callable_signature().unwrap().effects;
            assert_eq!(input, &NormalizedEffect::Binder(output), "the exact output equation retains its ordinary latent input");
            assert!(!answer.scheme.effect_quantifiers[output as usize].derived, "identity with an unconstrained input does not invent a separate calculation binder");
            assert_eq!(answer.effective_effects, Some(NormalizedEffect::Closed(Vec::new())), "latent outputs do not become creation effects");
            assert!(answer.to_string().contains("produces pull E"));
            let flow = query.declaration_producer_flow(declaration).unwrap();
            let NormalizedProducerFlowKind::Operation { requirement, alternatives, outputs } = &flow.nodes[0].kind else { panic!("pending producer operation remains explicit") };
            assert_eq!(requirement.as_ref(), &answer.scheme.requirements[0]);
            assert_eq!(alternatives, &[NormalizedProducerFlowOperationAlternative { identity: "producer-output-authority".to_string(), public_label: "stage.producer".to_string(), path: None, transfers: Vec::new(), opaque: false }], "an unselected candidate does not invent a carrier path");
            assert_eq!(outputs.pull, NormalizedEffect::Binder(output));
            assert_eq!(outputs.close, NormalizedEffect::Closed(Vec::new()));
            assert_eq!(flow.semantic_parity(&flow), Ok(true));
        });
    }

    #[test]
    fn solved_query_latent_roles_normalize_effect_binders_and_bounds() {
        use crate::sema::check::SolvedCallable;
        use crate::sema::inference::{Arrow, CandidateTemplate, EffectRole, Generalization, InferenceContext, OperationCall, ScopedRoot};
        use crate::syntax::arena::{BlockId, FunctionDefId};
        let mut answers = Vec::new();
        for reversed in [false, true] {
            let symbols = SymbolOwner::new();
            let mut graph = InferenceContext::default();
            let span = crate::source::Span::new(SourceId::new(0), 0, 0);
            let item = graph.atom(Atom::Int).unwrap();
            let stream = graph.stream(item).unwrap();
            let formal_pull = EffectSummary::Variable(graph.fresh_effect_at(1, Some(EffectSet::TIME)).unwrap());
            let formal_close = EffectSummary::Variable(graph.fresh_effect_at(1, Some(EffectSet::FS)).unwrap());
            let formal = graph.arrow(Arrow { kind: CallableKind::Pure, params: Vec::new(), result: stream, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
            let scheme = graph.generalize_with_effect_roots(formal, 0, Generalization::Allowed, &[], &[formal_pull, formal_close]).unwrap();
            let pull_slot = graph.scheme_effect_binder_index(scheme, formal_pull).unwrap().unwrap() as u32;
            let close_slot = graph.scheme_effect_binder_index(scheme, formal_close).unwrap().unwrap() as u32;
            let projection = crate::sema::inference::EffectProjection::ResultSuccess;
            let candidate = graph.register_candidate(CandidateTemplate { failure_projection: None, output_effect_roles: Vec::new(), identity: symbols.intern("latent-stage-authority"), public_label: symbols.intern("stage.buffer"), scheme, has_receiver: false, actual_eligibility: Vec::new(), argument_relations: Vec::new(), effect_roles: vec![(EffectRole::Pull { source: 0 }, crate::sema::inference::EffectRoleReference::Binder(pull_slot)), (EffectRole::Close { source: 0 }, crate::sema::inference::EffectRoleReference::Binder(close_slot)), (EffectRole::PullProjection { source: 0, projection }, crate::sema::inference::EffectRoleReference::Binder(pull_slot)), (EffectRole::CloseProjection { source: 0, projection }, crate::sema::inference::EffectRoleReference::Binder(close_slot))] }).unwrap();
            let family = graph.register_family(&[candidate]).unwrap();
            let first = EffectSummary::Variable(graph.fresh_effect_at(1, Some(if reversed { EffectSet::FS } else { EffectSet::TIME })).unwrap());
            let second = EffectSummary::Variable(graph.fresh_effect_at(1, Some(if reversed { EffectSet::TIME } else { EffectSet::FS })).unwrap());
            let (pull, close) = if reversed { (second, first) } else { (first, second) };
            let reason = graph.reason(span, None).unwrap();
            let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings: Vec::new(), receiver: None, arguments: Vec::new(), result: stream, effects: EffectSummary::Closed(EffectSet::EMPTY), effect_bindings: vec![(EffectRole::CloseProjection { source: 0, projection }, close), (EffectRole::Close { source: 0 }, close), (EffectRole::PullProjection { source: 0, projection }, pull), (EffectRole::Pull { source: 0 }, pull)] }, reason).unwrap();
            let signature = graph.arrow(Arrow { kind: CallableKind::Pure, params: Vec::new(), result: stream, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
            let scheme = graph.generalize_with_effect_roots(signature, 0, Generalization::Allowed, &[requirement], &[pull, close]).unwrap();
            let graph = graph.freeze_scoped(&[ScopedRoot { ty: signature, scope: Some(scheme) }]).unwrap();
            let mut solved = SolvedTypes::from_graph(graph, symbols.clone());
            let declaration = DeclarationIdentity { source: SourceId::new(0), namespace: None, declaration: FunctionDefId::from_index(0) };
            solved.declarations.insert(declaration, SolvedCallable { source_requirements: vec![requirement], parameter_producer_flows: Vec::new(), return_producer_flow: None, parameter_producers: vec![Default::default(); 0], return_producers: Default::default(), scheme, signature, body: BlockId::from_index(0), kind: CallableKind::Pure, return_elaboration: ReturnElaboration::Value, effective_effects: EffectSummary::Closed(EffectSet::EMPTY), required_effects: EffectSummary::Closed(EffectSet::EMPTY) });
            answers.push(SolvedQuery::new(&solved, &symbols).declaration(declaration).unwrap());
        }
        assert_eq!(answers[0].semantic_parity(&answers[1]), Ok(true), "{} vs {}", answers[0], answers[1]);
        let [NormalizedRequirement::Operation { effect_bindings, .. }] = answers[0].scheme.requirements.as_slice() else { panic!("latent roles remain explicit requirements") };
        assert_eq!(effect_bindings, &[(NormalizedEffectRole::Pull { source: 0 }, NormalizedEffect::Binder(0)), (NormalizedEffectRole::Close { source: 0 }, NormalizedEffect::Binder(1)), (NormalizedEffectRole::PullProjection { source: 0, projection: NormalizedEffectProjection::ResultSuccess }, NormalizedEffect::Binder(0)), (NormalizedEffectRole::CloseProjection { source: 0, projection: NormalizedEffectProjection::ResultSuccess }, NormalizedEffect::Binder(1))]);
        assert!(answers[0].to_string().contains("pull[0].ok E0"));
        assert!(answers[0].to_string().contains("close[0].ok E1"));
        let mut changed = answers[0].clone();
        let [NormalizedRequirement::Operation { effect_bindings, .. }] = changed.scheme.requirements.as_mut_slice() else { unreachable!() };
        effect_bindings[2].0 = NormalizedEffectRole::PullProjection { source: 1, projection: NormalizedEffectProjection::ResultSuccess };
        assert_eq!(answers[0].semantic_parity(&changed), Ok(false), "a projected payload keeps its exact source slot");
        assert_eq!(answers[0].scheme.effect_quantifiers[0].upper, Some(vec!["time".to_string()]));
        assert_eq!(answers[0].scheme.effect_quantifiers[1].upper, Some(vec!["fs".to_string()]));
    }

    #[test]
    fn solved_query_resolves_captured_type_and_effect_handles_without_generalizing_them() {
        use crate::sema::check::SolvedCallable;
        use crate::sema::inference::{Arrow, ComponentMember, Generalization, InferenceContext, Parameter, ScopedRoot};
        use crate::syntax::arena::{BlockId, FunctionDefId};
        let symbols = SymbolOwner::new();
        let mut graph = InferenceContext::default();
        let span = crate::source::Span::new(SourceId::new(0), 0, 0);
        let outer_variable = graph.fresh(1, span).unwrap();
        let inner_variable = graph.fresh(2, span).unwrap();
        let latent = EffectSummary::Variable(graph.fresh_effect_at(1, None).unwrap());
        let parameter = symbols.intern("captured_query_parameter");
        let outer = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: parameter, ty: outer_variable, defaulted: false, rest: false }], result: outer_variable, effects: latent }).unwrap();
        let inner = graph.arrow(Arrow { kind: CallableKind::Pure, params: vec![Parameter { label: parameter, ty: inner_variable, defaulted: false, rest: false }], result: outer_variable, effects: latent }).unwrap();
        let inner_scheme = graph.generalize_component(&[ComponentMember { root: inner, requirements: Vec::new(), policy: Generalization::Allowed }], 1, None).unwrap()[0];
        let outer_scheme = graph.generalize(outer, 0, Generalization::Allowed, &[]).unwrap();
        let graph = graph.freeze_scoped(&[ScopedRoot { ty: inner, scope: Some(inner_scheme) }, ScopedRoot { ty: outer, scope: Some(outer_scheme) }]).unwrap();
        let mut solved = SolvedTypes::from_graph(graph, symbols.clone());
        let identity = DeclarationIdentity { source: SourceId::new(0), namespace: None, declaration: FunctionDefId::from_index(0) };
        solved.declarations.insert(identity, SolvedCallable { source_requirements: Vec::new(), parameter_producer_flows: Vec::new(), return_producer_flow: None, parameter_producers: vec![Default::default(); 1], return_producers: Default::default(), scheme: inner_scheme, signature: inner, body: BlockId::from_index(0), kind: CallableKind::Pure, return_elaboration: ReturnElaboration::Value, effective_effects: latent, required_effects: latent });
        let query = SolvedQuery::new(&solved, &symbols);
        let normalized = query.declaration(identity).expect("captured handles resolve in their defining scope");
        let NormalizedShape::Arrow(arrow) = normalized.scheme.ty.shape() else { panic!("captured callable retains complete arrow") };
        assert_eq!(arrow.result.as_ref(), &NormalizedShape::Capture { index: 0, kind: BinderKind::Type });
        assert_eq!(arrow.effects, NormalizedEffect::Capture(0));
        assert_eq!(normalized.scheme.quantifiers.len(), 1);
        assert!(normalized.scheme.effect_quantifiers.is_empty());
        assert_eq!(normalized.semantic_parity(&normalized), Err(ParityError::CapturedRelationship));
    }
}

#[cfg(test)]
#[path = "query/native_tests.rs"]
mod native_tests;
