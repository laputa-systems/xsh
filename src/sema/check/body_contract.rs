use super::{CheckOutput, CheckedApiCall, CheckedCommandArgument, CheckedErrorConstructor, CheckedFunctionKind,
    CheckedProjection, Conversion, EffectDeclarationId, FunctionEffectFact, InferredVariant, InferredVariantPattern,
    RequirementTarget, ResourceTransferSite, StatementPosition, TextPattern};
use super::callable_contract::{CallableInstanceEvidence, SchemaIdentity, same_bound_operand, same_bound_effects, same_schema, schema_identities};
use crate::modules::signature::{MethodReceiver, ModuleFnSig};
use crate::modules::ManagedResource;
use crate::sema::builtin_templates::registered_argument_check;
use crate::sema::constants::CheckedRecordConstructor;
use crate::sema::schemes::{RegisteredArgumentCheck, RegisteredCallPlan, SchemeType, TypeScheme};
use crate::sema::types::{CallableType, ModuleExportType, Type, TypedCallable};
use crate::source::{SourceId, SourceMap, Span};
use crate::symbol::{Name, SymbolOwner};
use crate::syntax::arena::{ArenaChild, ArenaExprKind, ArenaExprOrRun, ArenaPipeStageKind, ArenaProgram, ArenaRecordFieldKind, ArenaStmtKind, BlockId, ExprId, StmtId, SugarView, TypeDefId};
use crate::syntax::node::{BinaryOp, StreamStageKind};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BodyContractOwner { Entry, Declaration(Name) }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BodyContractError {
    MissingSource(SourceId),
    UnmappedFact(Span),
    DuplicateOwner(Span),
    MissingReceiver(Span),
    MissingEffectFact(Span),
    UnsupportedSelection(Span),
    AmbiguousSite(Span),
}

#[derive(Clone, Debug)]
pub(crate) struct SemanticBodyContract {
    source_name: Arc<str>,
    owner: BodyContractOwner,
    operations: BTreeMap<NodePath, Operation>,
    selections: BTreeMap<NodePath, NodeSelection>,
    expr_paths: BTreeMap<ExprId, NodePath>,
    span_paths: BTreeMap<Span, NodePath>,
    schemas: Arc<BTreeMap<TypeDefId, SchemaIdentity>>,
    effects: Option<FunctionEffectFact>,
    effect_owner: Option<Arc<TypeScheme>>,
    symbol_owner: SymbolOwner,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum BodyContractSite {
    Expression(Span),
    Statement(Span),
    ValueBoundary(Span),
    Parameter { declaration: Span, ordinal: usize },
    Return { declaration: Span },
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct CanonicalBodyContractSite(SitePath);

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum SitePath { Expression(NodePath), Statement(NodePath), ValueExpression(NodePath), ValueStatement(NodePath), Parameter(usize), Return }

type SiteLocation = (OwnerKey, CanonicalBodyContractSite);

pub(crate) struct PendingBodyContracts {
    contracts: Vec<SemanticBodyContract>,
    sites: BTreeMap<BodyContractSite, BTreeSet<SiteLocation>>,
    value_boundaries: BTreeSet<(OwnerKey, NodePath, NodePath)>,
    user_sources: BTreeSet<SourceId>,
}

impl PendingBodyContracts {
    pub(crate) fn canonical_site(&self, site: BodyContractSite)
        -> Result<Option<(usize, CanonicalBodyContractSite)>, BodyContractError>
    {
        let span = match site {
            BodyContractSite::Expression(span) | BodyContractSite::Statement(span) | BodyContractSite::ValueBoundary(span) => span,
            BodyContractSite::Parameter { declaration, .. } | BodyContractSite::Return { declaration } => declaration,
        };
        if !self.user_sources.contains(&span.source_id) { return Ok(None); }
        let unique = |site| -> Result<Option<&SiteLocation>, BodyContractError> {
            match self.sites.get(&site) {
                Some(locations) if locations.len() == 1 => Ok(locations.first()),
                Some(_) => Err(BodyContractError::AmbiguousSite(span)),
                None => Ok(None),
            }
        };
        let location = if let BodyContractSite::ValueBoundary(_) = site {
            let expression = unique(BodyContractSite::Expression(span))?;
            let statement = unique(BodyContractSite::Statement(span))?;
            match (expression, statement) {
                (Some((owner, CanonicalBodyContractSite(SitePath::Expression(expression)))), Some((statement_owner, CanonicalBodyContractSite(SitePath::Statement(statement))))) => {
                    if owner != statement_owner || !self.value_boundaries.contains(&(owner.clone(), statement.clone(), expression.clone())) {
                        return Err(BodyContractError::AmbiguousSite(span));
                    }
                    (owner.clone(), CanonicalBodyContractSite(SitePath::ValueExpression(expression.clone())))
                }
                (Some((owner, CanonicalBodyContractSite(SitePath::Expression(path)))), None) =>
                    (owner.clone(), CanonicalBodyContractSite(SitePath::ValueExpression(path.clone()))),
                (None, Some((owner, CanonicalBodyContractSite(SitePath::Statement(path))))) =>
                    (owner.clone(), CanonicalBodyContractSite(SitePath::ValueStatement(path.clone()))),
                _ => return Err(BodyContractError::UnmappedFact(span)),
            }
        } else {
            unique(site)?.cloned().ok_or(BodyContractError::UnmappedFact(span))?
        };
        let index = self.contracts.iter().position(|body| body.source_name == location.0.0 && body.owner == location.0.1)
            .ok_or(BodyContractError::UnmappedFact(span))?;
        Ok(Some((index, location.1)))
    }

    pub(crate) fn into_contracts(self) -> Vec<SemanticBodyContract> { self.contracts }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum PathStep { Child(usize), PipelineStage(usize), RecordField(usize), ComparisonPair(usize), RunValue }

type NodePath = Vec<PathStep>;

#[derive(Clone, Debug)]
struct SelectedOperation {
    receiver: Option<MethodReceiver>,
    signature: &'static ModuleFnSig,
    argument_slots: Vec<usize>,
    argument_checks: Vec<Option<RegisteredArgumentCheck>>,
}

impl PartialEq for SelectedOperation {
    fn eq(&self, other: &Self) -> bool {
        self.receiver == other.receiver && std::ptr::eq(self.signature, other.signature)
            && self.argument_slots == other.argument_slots && self.argument_checks == other.argument_checks
    }
}

impl Eq for SelectedOperation {}

impl From<&RegisteredCallPlan> for SelectedOperation {
    fn from(plan: &RegisteredCallPlan) -> Self {
        Self { receiver: plan.receiver, signature: plan.signature, argument_slots: plan.argument_slots.to_vec(),
            argument_checks: plan.argument_checks.to_vec() }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CommandArgument { BooleanFlag, Value, Word(Type) }

#[derive(Clone, Debug, Eq, PartialEq)]
struct Operation {
    selection: SelectedOperation,
    command_arguments: Option<Vec<CommandArgument>>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ControlFact {
    NonnullFallback, PropagatingStatement, DiscardableBinding, PropagatingCondition, RedundantConditionPropagation,
    ExitingBlock, PathLiteral, OptionalBinding, BuiltinRange, CardinalityPreserving, PropagatingError,
    LatentStreamError, FallibleStage, PropagatingStageCallback, ExhaustiveMatch,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum TransferKind { Return, BlockValue, Break, Assignment, Yield }

#[derive(Clone, Debug)]
enum OperandType {
    Ground(Type),
    Scheme { ty: SchemeType, owner: Arc<TypeScheme> },
    EmptyList(Box<Self>),
    Diverges,
    Missing,
}

impl OperandType {
    fn is_list(&self) -> bool {
        match self {
            Self::Ground(Type::List(item)) => **item == Type::Unknown
                || (!item.contains_recovery() && !item.contains_inference()),
            Self::Scheme { ty: SchemeType::List(_), .. } => true,
            _ => false,
        }
    }

    fn ground(&self) -> Option<Type> {
        match self {
            Self::Ground(ty) => Some(ty.clone()),
            Self::Scheme { ty, .. } => ty.closed_ground_type(),
            Self::EmptyList(_) | Self::Diverges | Self::Missing => None,
        }
    }

    fn preserved_by(&self, candidate: &Self, instance: Option<&CallableInstanceEvidence>,
        left_schemas: &BTreeMap<TypeDefId, SchemaIdentity>, right_schemas: &BTreeMap<TypeDefId, SchemaIdentity>) -> bool
    {
        match (self, candidate) {
            (Self::Ground(left), Self::Ground(right)) => left == right,
            (Self::Ground(left), Self::Scheme { .. }) => candidate.ground().as_ref() == Some(left)
                || instance.is_some_and(|instance| candidate.specialized(instance).as_ref() == Some(left)),
            (Self::Scheme { .. }, Self::Ground(right)) => self.ground().as_ref() == Some(right),
            (Self::Scheme { ty: left, owner: left_owner }, Self::Scheme { ty: right, owner: right_owner }) =>
                same_bound_operand(left, left_owner, right, right_owner, left_schemas, right_schemas),
            (Self::EmptyList(left), Self::EmptyList(right)) => left.empty_list_preserved_by(
                right, instance, left_schemas, right_schemas),
            (Self::Diverges, Self::Diverges) => true,
            _ => false,
        }
    }

    fn specialized(&self, instance: &CallableInstanceEvidence) -> Option<Type> {
        match self {
            Self::Ground(ty) => Some(ty.clone()),
            Self::Scheme { ty, owner } => instance.specialize(ty, owner),
            Self::EmptyList(_) | Self::Diverges | Self::Missing => None,
        }
    }

    fn empty_list_preserved_by(&self, candidate: &Self, instance: Option<&CallableInstanceEvidence>,
        left_schemas: &BTreeMap<TypeDefId, SchemaIdentity>, right_schemas: &BTreeMap<TypeDefId, SchemaIdentity>) -> bool
    {
        self.preserved_by(candidate, instance, left_schemas, right_schemas)
            || matches!(self.ground(), Some(Type::List(item)) if *item == Type::Unknown)
                && candidate.ground().or_else(|| instance.and_then(|instance| candidate.specialized(instance))).is_some_and(|ty| {
                    matches!(ty, Type::List(item) if !item.contains_recovery() && !item.contains_inference() && !item.contains_any())
                })
    }
}

#[derive(Clone, Debug)]
enum TransferOperand {
    Expression(OperandType),
    Run(OperandType),
    DelegatedExpression(OperandType),
    BareTail(OperandType),
    Empty,
    NoValue,
    SameScopeAssignment,
    Match(Vec<TransferCause>),
    Missing,
}

impl TransferOperand {
    fn same_role(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Expression(_), Self::Expression(_)) | (Self::Run(_), Self::Run(_))
            | (Self::BareTail(_), Self::BareTail(_)) | (Self::DelegatedExpression(_), Self::DelegatedExpression(_))
            | (Self::Empty, Self::Empty)
            | (Self::NoValue, Self::NoValue) | (Self::SameScopeAssignment, Self::SameScopeAssignment)
            | (Self::Missing, Self::Missing) => true,
            (Self::Match(left), Self::Match(right)) => left.len() == right.len()
                && left.iter().zip(right).all(|(left, right)| left.operand.same_role(&right.operand)),
            _ => false,
        }
    }

    fn scalar_instance(&self, candidate: &Self, instance: &CallableInstanceEvidence) -> bool {
        match (self, candidate) {
            (Self::Expression(old), Self::Expression(new)) | (Self::Run(old), Self::Run(new))
            | (Self::BareTail(old), Self::BareTail(new))
            | (Self::DelegatedExpression(old), Self::DelegatedExpression(new)) => old.ground().is_some_and(|old|
                !old.may_hold_resource() && new.specialized(instance).as_ref() == Some(&old)),
            (Self::Match(old), Self::Match(new)) => old.len() == new.len() && !old.is_empty()
                && old.iter().zip(new).all(|(old, new)| old.preserved_by(new, Some(instance))
                    && (matches!((&old.operand, &new.operand), (Self::Empty, Self::Empty) | (Self::NoValue, Self::NoValue))
                        && !old.runtime_flag && !new.runtime_flag
                        || old.operand.scalar_instance(&new.operand, instance))),
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
struct TransferCause {
    runtime_flag: bool,
    operand: TransferOperand,
}

impl TransferCause {
    fn preserved_by(&self, candidate: &Self, instance: Option<&CallableInstanceEvidence>) -> bool {
        if !self.operand.same_role(&candidate.operand) { return false; }
        if let (TransferOperand::Match(old), TransferOperand::Match(new)) = (&self.operand, &candidate.operand) {
            if self.runtime_flag != old.iter().any(|cause| cause.runtime_flag)
                || candidate.runtime_flag != new.iter().any(|cause| cause.runtime_flag)
                || !old.iter().zip(new).all(|(old, new)| old.preserved_by(new, instance))
            { return false; }
        }
        self.runtime_flag == candidate.runtime_flag
            || !self.runtime_flag && candidate.runtime_flag && instance.is_some_and(|instance|
                self.operand.scalar_instance(&candidate.operand, instance))
    }
}

#[derive(Clone, Debug)]
struct ResourceTransfer {
    kind: TransferKind,
    cause: TransferCause,
}

#[derive(Clone, Debug)]
struct OperatorElaboration {
    operator: BinaryOp,
    left: OperandType,
    right: OperandType,
    result: OperandType,
}

impl OperatorElaboration {
    fn preserved_by(&self, candidate: &Self, instance: Option<&CallableInstanceEvidence>,
        left_schemas: &BTreeMap<TypeDefId, SchemaIdentity>, right_schemas: &BTreeMap<TypeDefId, SchemaIdentity>) -> bool
    {
        self.operator == candidate.operator
            && self.left.preserved_by(&candidate.left, instance, left_schemas, right_schemas)
            && self.right.preserved_by(&candidate.right, instance, left_schemas, right_schemas)
            && self.result.preserved_by(&candidate.result, instance, left_schemas, right_schemas)
    }
}

#[derive(Clone, Debug, Default)]
struct NodeSelection {
    binary: Option<OperatorElaboration>,
    requirement: Option<RequirementTarget>,
    constructor: Option<CheckedRecordConstructor>,
    constructor_fields: Option<Vec<Name>>,
    conversion: Option<(Conversion, Type)>,
    projection: Option<CheckedProjection>,
    typed_call: Option<Arc<TypedCallable>>,
    user_call: Option<(Arc<str>, Name)>,
    alias: Option<(Name, bool, CallableType, crate::sema::types::CallableKind, Option<(Arc<str>, Name)>)>,
    arguments: Option<(Option<usize>, Vec<usize>)>,
    variant: Option<InferredVariant>,
    pattern_variant: Option<InferredVariantPattern>,
    error_constructor: Option<CheckedErrorConstructor>,
    from_end: Option<u32>,
    stage: Option<(Type, Type)>,
    stage_kind: Option<StreamStageKind>,
    handler_input: Option<Type>,
    position: Option<StatementPosition>,
    control: BTreeSet<ControlFact>,
    result_constructor: Option<super::CheckedResultConstructor>,
    collect_target: Option<Span>,
    resources: Option<Vec<ManagedResource>>,
    transfers: Vec<ResourceTransfer>,
    text_pattern: Option<TextPattern>,
}

impl SemanticBodyContract {
    /// Uses a loaded module's canonical key, or the entry's logical source name.
    pub(crate) fn source_name(&self) -> &str { &self.source_name }

    pub(crate) fn owner(&self) -> BodyContractOwner { self.symbol_owner.with_current(|| self.owner) }

    pub(crate) fn effect_fact(&self) -> Option<&FunctionEffectFact> { self.effects.as_ref() }

    /// Requires exact checked selections and effect summaries. Removing an
    /// explicit clause additionally requires the checker's redundancy proof.
    /// Unknown summaries remain unknown and never authorize clause removal.
    pub(crate) fn preserved_by(&self, candidate: &Self) -> bool {
        self.preserved_by_instance(candidate, None)
    }

    pub(crate) fn preserved_by_instance(&self, candidate: &Self, instance: Option<&CallableInstanceEvidence>) -> bool {
        self.source_name == candidate.source_name && self.owner == candidate.owner
            && self.operations == candidate.operations && self.same_effect_fact(candidate, instance)
            && self.selections.len() == candidate.selections.len()
            && self.selections.iter().all(|(path, left)| candidate.selections.get(path)
                .is_some_and(|right| self.same_selection(left, right, candidate, instance)))
    }

    fn same_selection(&self, left: &NodeSelection, right: &NodeSelection, candidate: &Self, instance: Option<&CallableInstanceEvidence>) -> bool {
        let same_requirement = match (&left.requirement, &right.requirement) {
            (None, None) => true,
            (Some(left), Some(right)) => left.ty == right.ty
                && same_schema(Some(&left.context), Some(&right.context), &self.schemas, &candidate.schemas),
            _ => false,
        };
        let same_constructor = match (&left.constructor, &right.constructor) {
            (None, None) => true,
            (Some(left), Some(right)) => left.ty == right.ty && left.instance.arguments == right.instance.arguments
                && self.schemas.get(&left.instance.definition).is_some_and(|identity| Some(identity) == candidate.schemas.get(&right.instance.definition)),
            _ => false,
        };
        let same_projection = match (&left.projection, &right.projection) {
            (None, None) => true,
            (Some(left), Some(right)) => left.field == right.field && left.value_type == right.value_type
                && left.operation == right.operation && left.callable == right.callable
                && self.expr_paths.get(&left.receiver) == candidate.expr_paths.get(&right.receiver)
                && self.expr_paths.get(&left.key) == candidate.expr_paths.get(&right.key),
            _ => false,
        };
        let same_collect = match (left.collect_target, right.collect_target) {
            (None, None) => true,
            (Some(left), Some(right)) => self.span_paths.get(&left) == candidate.span_paths.get(&right),
            _ => false,
        };
        let same_binary = match (&left.binary, &right.binary) {
            (None, None) => true,
            (Some(left), Some(right)) => left.preserved_by(right, instance, &self.schemas, &candidate.schemas),
            _ => false,
        };
        same_binary && same_requirement && same_constructor && same_projection && same_collect
            && left.constructor_fields == right.constructor_fields && left.conversion == right.conversion
            && left.typed_call == right.typed_call && left.user_call == right.user_call && left.alias == right.alias
            && left.arguments == right.arguments && left.variant == right.variant && left.pattern_variant == right.pattern_variant
            && left.error_constructor == right.error_constructor && left.from_end == right.from_end && left.stage == right.stage
            && left.stage_kind == right.stage_kind
            && left.handler_input == right.handler_input && left.position == right.position && left.control == right.control
            && left.result_constructor == right.result_constructor && left.resources == right.resources
            && left.transfers.len() == right.transfers.len()
            && left.transfers.iter().zip(&right.transfers).all(|(old, new)| old.kind == new.kind
                && old.cause.preserved_by(&new.cause, instance))
            && left.text_pattern == right.text_pattern
    }
}

impl SemanticBodyContract {
    fn same_effect_fact(&self, candidate: &Self, instance: Option<&CallableInstanceEvidence>) -> bool {
        let (left, right) = match (&self.effects, &candidate.effects) {
            (None, None) => return true,
            (Some(left), Some(right)) => (left, right),
            _ => return false,
        };
        let matches = |left_effects: &crate::sema::effects::CheckedEffects, right_effects: &crate::sema::effects::CheckedEffects| {
            match (&self.effect_owner, &candidate.effect_owner) {
                (Some(left_owner), Some(right_owner)) => match instance {
                    Some(instance) => instance.specialize_effects(right_effects, right_owner).as_ref() == Some(left_effects),
                    None => same_bound_effects(left_effects, left_owner, right_effects, right_owner, &self.schemas, &candidate.schemas),
                },
                _ => !left_effects.is_symbolic() && !right_effects.is_symbolic() && left_effects == right_effects,
            }
        };
        matches(&left.effective, &right.effective) && matches(&left.required, &right.required)
            && left.unknown_chain == right.unknown_chain && left.inference_allowed == right.inference_allowed
            && (left.inferred == right.inferred || !left.inferred && right.inferred && left.inference_allowed
                && left.redundant_clause && left.effective.is_known() && left.effective == left.required
                && left.unknown_chain.is_empty())
    }
}

type OwnerKey = (Arc<str>, BodyContractOwner);

struct Capture<'a> {
    checked: &'a CheckOutput,
    program: &'a ArenaProgram,
    sources: &'a SourceMap,
    bodies: BTreeMap<OwnerKey, SemanticBodyContract>,
    schemas: Arc<BTreeMap<TypeDefId, SchemaIdentity>>,
    registered: BTreeMap<ExprId, Arc<RegisteredCallPlan>>,
    seen_registered: BTreeSet<ExprId>,
    seen_binary: BTreeSet<ExprId>,
    seen_prepared_binary: BTreeSet<ExprId>,
    seen_api: BTreeSet<Span>,
    seen_commands: BTreeSet<Span>,
    seen_effects: BTreeSet<EffectDeclarationId>,
    seen_spans: BTreeSet<Span>,
    seen_stages: BTreeSet<(Option<Name>, Span)>,
    user_sources: BTreeSet<SourceId>,
    transfers: BTreeMap<Span, Vec<ResourceTransfer>>,
    operand_expressions: BTreeMap<ExprId, OperandType>,
    operand_tails: BTreeMap<StmtId, OperandType>,
    module_sources: BTreeMap<SourceId, Arc<str>>,
    sites: BTreeMap<BodyContractSite, BTreeSet<SiteLocation>>,
    value_boundaries: BTreeSet<(OwnerKey, NodePath, NodePath)>,
}

impl PendingBodyContracts {
    /// Captures noninternal registered calls, command treatment, validation,
    /// conversions, projections, callable targets, and published lowering
    /// selections at executable child paths, plus canonical declaration effects.
    /// Annotation children do not contribute to paths. Unmapped published facts
    /// fail capture; this operation neither checks a body nor solves evidence.
    pub(crate) fn capture(checked: &CheckOutput, program: &ArenaProgram, sources: &SourceMap)
        -> Result<Self, BodyContractError>
    {
        let mut definitions = checked.checked_declarations.types.iter().filter_map(|(&id, declaration)| {
            (checked.checked_declarations.record_constructors.definition(declaration.namespace, declaration.name) == Some(id)).then_some(id)
        }).collect::<BTreeSet<_>>();
        let mut contexts = checked.requirement_targets.values().map(|target| &target.context).collect::<Vec<_>>();
        while let Some(context) = contexts.pop() {
            definitions.extend(context.instances.iter().map(|instance| instance.definition));
            contexts.extend(context.children.values());
        }
        definitions.extend(checked.record_constructor_instances.values().map(|constructor| constructor.instance.definition));
        let schemas = schema_identities(program, &checked.checked_declarations.record_constructors, definitions);
        let mut capture = Capture { checked, program, sources, bodies: BTreeMap::new(), schemas,
            registered: BTreeMap::new(), seen_registered: BTreeSet::new(), seen_binary: BTreeSet::new(), seen_prepared_binary: BTreeSet::new(), seen_api: BTreeSet::new(),
            seen_commands: BTreeSet::new(), seen_effects: BTreeSet::new(), seen_spans: BTreeSet::new(),
            user_sources: BTreeSet::new(), transfers: BTreeMap::new(), seen_stages: BTreeSet::new(), module_sources: BTreeMap::new(),
            sites: BTreeMap::new(), value_boundaries: BTreeSet::new(),
            operand_expressions: BTreeMap::new(), operand_tails: BTreeMap::new() };
        for module in &program.modules {
            let source = program.module_statements(module).next().map(|statement| program.arena.stmt(statement).span.source_id)
                .or_else(|| program.module_doc_for(module.statements).map(|span| span.source_id));
            if let Some(source) = source {
                if capture.module_sources.insert(source, Arc::from(module.key.as_str())).is_some() {
                    return Err(BodyContractError::UnmappedFact(program.module_doc_for(module.statements)
                        .unwrap_or_else(|| program.arena.stmt(program.module_statements(module).next().unwrap()).span)));
                }
            }
        }
        for declaration in checked.checked_declarations.functions.values().filter(|declaration| !declaration.internal) {
            capture.user_sources.insert(declaration.body.source_id);
            if let Some(facts) = &declaration.scheme_body_facts {
                capture.operand_expressions.extend(facts.expression_types().iter().map(|(&id, ty)|
                    (id, OperandType::Scheme { ty: ty.clone(), owner: Arc::clone(facts.scheme()) })));
                capture.operand_tails.extend(facts.tail_types().iter().map(|(&id, ty)|
                    (id, OperandType::Scheme { ty: ty.clone(), owner: Arc::clone(facts.scheme()) })));
                for (&expression, plan) in facts.registered_calls() {
                    if capture.registered.insert(expression, Arc::clone(plan)).is_some() {
                        return Err(BodyContractError::UnmappedFact(program.arena.expr(expression).span));
                    }
                }
            }
        }
        for (&site, &value) in &checked.resource_transfers {
            let (span, kind) = match site {
                ResourceTransferSite::Return(id) => (program.arena.stmt(id).span, TransferKind::Return),
                ResourceTransferSite::BlockValue(id) => (program.arena.span(program.arena.block(id).span), TransferKind::BlockValue),
                ResourceTransferSite::Break(id) => (program.arena.stmt(id).span, TransferKind::Break),
                ResourceTransferSite::Assignment(id) => (program.arena.stmt(id).span, TransferKind::Assignment),
                ResourceTransferSite::Yield(id) => (program.arena.stmt(id).span, TransferKind::Yield),
            };
            let operand = capture.transfer_operand(site);
            capture.transfers.entry(span).or_default().push(ResourceTransfer { kind, cause: TransferCause { runtime_flag: value, operand } });
        }
        for transfers in capture.transfers.values_mut() { transfers.sort_by_key(|transfer| transfer.kind); }
        capture.roots(program.statement_ids().collect(), None)?;
        for module in program.modules.iter().filter(|module| !module.internal) {
            capture.roots(program.module_statements(module).collect(), Some(module.name))?;
        }
        let sites = std::mem::take(&mut capture.sites);
        let value_boundaries = std::mem::take(&mut capture.value_boundaries);
        let user_sources = capture.user_sources.clone();
        let contracts = capture.finish()?;
        Ok(Self { contracts, sites, value_boundaries, user_sources })
    }
}

impl Capture<'_> {
    fn expression_type(&self, expression: ExprId) -> OperandType {
        let checked_type = self.operand_expressions.get(&expression).cloned().unwrap_or_else(||
            self.span_operand(self.program.arena.expr(expression).span));
        match self.program.arena.expr(expression).kind {
            ArenaExprKind::List(items) if items.is_empty() && checked_type.is_list() => {
                OperandType::EmptyList(Box::new(checked_type))
            }
            ArenaExprKind::ValueBlock(block)
                if matches!(&checked_type, OperandType::Ground(Type::Unknown))
                    && self.checked.definitely_exiting_block_spans.contains(
                        &self.program.arena.span(self.program.arena.block(block).span)) =>
            {
                OperandType::Diverges
            }
            _ => checked_type,
        }
    }

    fn expression_operand(&self, expression: ExprId) -> TransferOperand {
        TransferOperand::Expression(self.expression_type(expression))
    }

    fn span_operand(&self, span: Span) -> OperandType {
        self.checked.expr_types.get(&span).cloned().map(OperandType::Ground).unwrap_or(OperandType::Missing)
    }

    fn expr_or_run_operand(&self, value: ArenaExprOrRun) -> TransferOperand {
        match value {
            ArenaExprOrRun::Expr(expression) => self.expression_operand(expression),
            ArenaExprOrRun::Run(_) => TransferOperand::Run(self.span_operand(super::expr::expr_or_run_span_arena(self.program, value))),
        }
    }

    fn transfer_operand(&self, site: ResourceTransferSite) -> TransferOperand {
        match site {
            ResourceTransferSite::BlockValue(block) => self.block_operand(block),
            ResourceTransferSite::Return(statement) | ResourceTransferSite::Break(statement)
            | ResourceTransferSite::Yield(statement) | ResourceTransferSite::Assignment(statement) => {
                if matches!(site, ResourceTransferSite::Assignment(_))
                    && self.checked.same_scope_resource_assignments.contains(&statement)
                { return TransferOperand::SameScopeAssignment; }
                match self.program.arena.stmt(statement).kind {
                    ArenaStmtKind::Return(Some(value)) | ArenaStmtKind::Yield(value)
                    | ArenaStmtKind::Assign { value, .. } => self.expr_or_run_operand(value),
                    ArenaStmtKind::Break { value: Some(expression) } => self.expression_operand(expression),
                    ArenaStmtKind::YieldDelegate(expression) => TransferOperand::DelegatedExpression(self.expression_type(expression)),
                    ArenaStmtKind::Return(None) | ArenaStmtKind::Break { value: None } => TransferOperand::NoValue,
                    _ => TransferOperand::Missing,
                }
            }
        }
    }

    fn block_operand(&self, block: BlockId) -> TransferOperand {
        let Some(statement) = self.program.arena.stmt_ids(self.program.arena.block(block).statements).last() else {
            return TransferOperand::Empty;
        };
        let node = self.program.arena.stmt(statement);
        match self.checked.statement_positions.get(&node.span) {
            Some(StatementPosition::Statement) => return TransferOperand::NoValue,
            Some(StatementPosition::Value) => {}
            None => return TransferOperand::Missing,
        }
        match node.kind {
            ArenaStmtKind::Expr(expression) => self.expression_operand(expression),
            ArenaStmtKind::TailBareIdent(_) => TransferOperand::BareTail(self.operand_tails.get(&statement).cloned()
                .unwrap_or_else(|| self.span_operand(node.span))),
            ArenaStmtKind::Match { arms, .. } => self.program.arena.match_arms(arms).iter().map(|arm| {
                self.checked.resource_transfers.get(&ResourceTransferSite::BlockValue(arm.block)).copied()
                    .map(|runtime_flag| TransferCause { runtime_flag, operand: self.block_operand(arm.block) })
            }).collect::<Option<Vec<_>>>().map(TransferOperand::Match).unwrap_or(TransferOperand::Missing),
            _ => TransferOperand::NoValue,
        }
    }

    fn source_name(&self, source_id: SourceId) -> Result<Arc<str>, BodyContractError> {
        if let Some(key) = self.module_sources.get(&source_id) { return Ok(Arc::clone(key)); }
        self.sources.get(source_id).map(|source| Arc::from(source.name())).ok_or(BodyContractError::MissingSource(source_id))
    }

    fn ensure_owner(&mut self, span: Span, owner: BodyContractOwner) -> Result<OwnerKey, BodyContractError> {
        let key = (self.source_name(span.source_id)?, owner);
        self.bodies.entry(key.clone()).or_insert_with(|| SemanticBodyContract {
            source_name: Arc::clone(&key.0), owner, operations: BTreeMap::new(), selections: BTreeMap::new(),
            expr_paths: BTreeMap::new(), span_paths: BTreeMap::new(), schemas: Arc::clone(&self.schemas), effects: None, effect_owner: None,
            symbol_owner: self.program.symbol_owner().clone(),
        });
        Ok(key)
    }

    fn roots(&mut self, statements: Vec<StmtId>, namespace: Option<Name>) -> Result<(), BodyContractError> {
        let mut entry_index = 0;
        for statement in statements {
            let span = self.program.arena.stmt(statement).span;
            self.user_sources.insert(span.source_id);
            let key = self.ensure_owner(span, BodyContractOwner::Entry)?;
            let mut unwrapped = statement;
            while let ArenaStmtKind::Export(inner) = self.program.arena.stmt(unwrapped).kind { unwrapped = inner; }
            let kind = self.program.arena.stmt(unwrapped).kind;
            if matches!(kind, ArenaStmtKind::Use(_) | ArenaStmtKind::TypeDef(_) | ArenaStmtKind::ErrorDef(_))
                && children(self.program, ArenaChild::Stmt(unwrapped)).is_empty()
            {
                self.seen_spans.insert(span);
                self.seen_spans.insert(self.program.arena.stmt(unwrapped).span);
                continue;
            }
            let mut pending = vec![(ArenaChild::Stmt(statement), key, vec![PathStep::Child(entry_index)])];
            if function_definition(kind).is_none() { entry_index += 1; }
            while let Some((node, mut owner, mut path)) = pending.pop() {
                if let ArenaChild::Stmt(statement) = node {
                    if let Some(definition) = function_definition(self.program.arena.stmt(statement).kind) {
                        let declaration = self.checked.checked_declarations.functions.get(&definition)
                            .filter(|declaration| declaration.namespace == namespace && !declaration.internal)
                            .ok_or(BodyContractError::UnmappedFact(self.program.arena.stmt(statement).span))?;
                        owner = (self.source_name(declaration.body.source_id)?, BodyContractOwner::Declaration(declaration.name));
                        if self.bodies.contains_key(&owner) { return Err(BodyContractError::DuplicateOwner(declaration.body)); }
                        self.ensure_owner(declaration.body, owner.1)?;
                        let identity = EffectDeclarationId { namespace: declaration.namespace, body: declaration.body };
                        let fact = self.checked.function_effect_facts.get(&identity);
                        if declaration.kind != CheckedFunctionKind::Pure && fact.is_none() {
                            return Err(BodyContractError::MissingEffectFact(declaration.body));
                        }
                        if fact.is_some() { self.seen_effects.insert(identity); }
                        if fact.is_some_and(|fact| fact.scope.owner().is_some_and(|owner| owner != identity)) {
                            return Err(BodyContractError::UnmappedFact(declaration.body));
                        }
                        let body = self.bodies.get_mut(&owner).unwrap();
                        body.effects = fact.cloned();
                        body.effect_owner = declaration.scheme.clone();
                        let definition_span = self.program.arena.stmt(statement).span;
                        for declaration_span in [definition_span, declaration.body] {
                            for ordinal in 0..self.program.arena.params(self.program.arena.function_def(definition).params).len() {
                                self.sites.entry(BodyContractSite::Parameter { declaration: declaration_span, ordinal }).or_default()
                                    .insert((owner.clone(), CanonicalBodyContractSite(SitePath::Parameter(ordinal))));
                            }
                            self.sites.entry(BodyContractSite::Return { declaration: declaration_span }).or_default()
                                .insert((owner.clone(), CanonicalBodyContractSite(SitePath::Return)));
                        }
                        path.clear();
                    }
                }
                self.node(node, &owner, &path, namespace)?;
                let children = children(self.program, node);
                for (index, child) in children.into_iter().enumerate().rev() {
                    let mut child_path = path.clone();
                    child_path.push(PathStep::Child(index));
                    pending.push((child, owner.clone(), child_path));
                }
            }
        }
        Ok(())
    }

    fn node(&mut self, node: ArenaChild, owner: &OwnerKey, path: &[PathStep], namespace: Option<Name>) -> Result<(), BodyContractError> {
        let span = node_span(self.program, node);
        if let Some(span) = span {
            let site = match node {
                ArenaChild::Expr(_) => Some((BodyContractSite::Expression(span), SitePath::Expression(path.to_vec()))),
                ArenaChild::Stmt(statement) if matches!(self.program.arena.stmt(statement).kind, ArenaStmtKind::Export(_)) => None,
                ArenaChild::Stmt(_) => Some((BodyContractSite::Statement(span), SitePath::Statement(path.to_vec()))),
                _ => None,
            };
            if let Some((site, path)) = site {
                self.sites.entry(site).or_default().insert((owner.clone(), CanonicalBodyContractSite(path)));
            }
            if let ArenaChild::Stmt(statement) = node {
                let value = match self.program.arena.stmt(statement).kind {
                    ArenaStmtKind::Expr(value) => Some(value),
                    ArenaStmtKind::Let { initializer: ArenaExprOrRun::Expr(value), .. }
                    | ArenaStmtKind::Var { initializer: ArenaExprOrRun::Expr(value), .. } => Some(value),
                    _ => None,
                };
                if let Some(value) = value.filter(|&value| self.program.arena.expr(value).span == span) {
                    for (index, child) in children(self.program, node).into_iter().enumerate() {
                        if child == ArenaChild::Expr(value) {
                            let mut value_path = path.to_vec();
                            value_path.push(PathStep::Child(index));
                            self.value_boundaries.insert((owner.clone(), path.to_vec(), value_path));
                        }
                    }
                }
            }
        }
        if let Some(span) = span {
            self.seen_spans.insert(span);
            self.bodies.get_mut(owner).unwrap().span_paths.entry(span).or_insert_with(|| path.to_vec());
            let selection = self.node_selection(span, namespace)?;
            if selection.stage.is_some() { self.seen_stages.insert((namespace, span)); }
            self.bodies.get_mut(owner).unwrap().selections.insert(path.to_vec(), selection);
        }
        if let ArenaChild::Expr(expression) = node {
            let span = self.program.arena.expr(expression).span;
            self.bodies.get_mut(owner).unwrap().expr_paths.insert(expression, path.to_vec());
            self.binary_operators(expression, owner, path)?;
            self.pipeline_stages(expression, owner, path, namespace)?;
            self.shorthand_fields(expression, owner, path, namespace)?;
            let ordinary = self.checked.api_calls.get(&span);
            let generalized = self.registered.get(&expression);
            let selection = match (ordinary, generalized) {
                (Some(_), Some(_)) => return Err(BodyContractError::UnmappedFact(span)),
                (Some(call), None) => {
                    self.seen_api.insert(span);
                    Some(self.api_selection(call, Some(expression), span)?)
                }
                (None, Some(plan)) => { self.seen_registered.insert(expression); Some(SelectedOperation::from(plan.as_ref())) }
                (None, None) => None,
            };
            if let Some(selection) = selection {
                self.bodies.get_mut(owner).unwrap().operations.insert(path.to_vec(), Operation { selection, command_arguments: None });
            }
        }
        if let ArenaChild::Stmt(statement) = node {
            self.run_value(statement, owner, path, namespace)?;
            let span = match self.program.arena.stmt(statement).kind {
                ArenaStmtKind::Command(command) => self.program.arena.span(self.program.arena.command_stmt(command).span),
                _ => self.program.arena.stmt(statement).span,
            };
            if let Some(command) = self.checked.module_commands.get(&span) {
                let selection = self.api_selection(&command.call, None, span)?;
                let command_arguments = command.arguments.iter().map(|argument| match argument {
                    CheckedCommandArgument::BooleanFlag => CommandArgument::BooleanFlag,
                    CheckedCommandArgument::Value => CommandArgument::Value,
                    CheckedCommandArgument::Word(ty) => CommandArgument::Word(ty.clone()),
                }).collect();
                self.seen_commands.insert(span);
                self.bodies.get_mut(owner).unwrap().operations.insert(path.to_vec(), Operation { selection, command_arguments: Some(command_arguments) });
            }
        }
        Ok(())
    }

    fn binary_elaboration(&mut self, expression: ExprId, operator: BinaryOp, left: ExprId, right: ExprId, result: OperandType)
        -> Result<OperatorElaboration, BodyContractError>
    {
        let span = self.program.arena.expr(expression).span;
        let ordinary = if let Some(checked) = self.checked.binary_operators.get(&expression) {
            if checked.operator != operator { return Err(BodyContractError::UnsupportedSelection(span)); }
            self.seen_binary.insert(expression);
            Some(OperatorElaboration { operator, left: self.expression_type(left), right: self.expression_type(right), result })
        } else { None };
        let prepared = if let Some(checked) = self.checked.prepared_constants.binary_operators.get(&expression) {
            if checked.operator != operator { return Err(BodyContractError::UnsupportedSelection(span)); }
            self.seen_prepared_binary.insert(expression);
            Some(OperatorElaboration { operator, left: OperandType::Ground(checked.left_type.clone()), right: OperandType::Ground(checked.right_type.clone()), result: OperandType::Ground(checked.result_type.clone()) })
        } else { None };
        if let (Some(ordinary), Some(prepared)) = (&ordinary, &prepared)
            && !ordinary.preserved_by(prepared, None, &self.schemas, &self.schemas)
        { return Err(BodyContractError::UnsupportedSelection(span)); }
        let elaboration = prepared.or(ordinary).ok_or(BodyContractError::UnmappedFact(span))?;
        for operand in [&elaboration.left, &elaboration.right, &elaboration.result] {
            if matches!(operand, OperandType::Missing)
                || operand.ground().is_some_and(|ty| ty.contains_inference() || ty.contains_recovery())
            { return Err(BodyContractError::UnsupportedSelection(span)); }
        }
        Ok(elaboration)
    }

    fn binary_operators(&mut self, expression: ExprId, owner: &OwnerKey, path: &[PathStep]) -> Result<(), BodyContractError> {
        match self.program.arena.expr(expression).kind {
            ArenaExprKind::Binary { op, left, right } => {
                let result = self.expression_type(expression);
                let elaboration = self.binary_elaboration(expression, op, left, right, result)?;
                self.bodies.get_mut(owner).unwrap().selections.get_mut(path).unwrap().binary = Some(elaboration);
            }
            ArenaExprKind::ComparisonChain(pairs) => {
                let mut previous = None;
                for (ordinal, pair) in self.program.arena.expr_ids(pairs).collect::<Vec<_>>().into_iter().enumerate() {
                    let ArenaExprKind::Binary { op, left, right } = self.program.arena.expr(pair).kind else { return Err(BodyContractError::UnsupportedSelection(self.program.arena.expr(pair).span)); };
                    let left = previous.unwrap_or(left);
                    let elaboration = self.binary_elaboration(pair, op, left, right, OperandType::Ground(Type::Bool))?;
                    let mut pair_path = path.to_vec();
                    pair_path.push(PathStep::ComparisonPair(ordinal));
                    let body = self.bodies.get_mut(owner).unwrap();
                    body.expr_paths.insert(pair, pair_path.clone());
                    if body.selections.insert(pair_path, NodeSelection { binary: Some(elaboration), ..NodeSelection::default() }).is_some() {
                        return Err(BodyContractError::UnmappedFact(self.program.arena.expr(pair).span));
                    }
                    previous = Some(right);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn run_value(&mut self, statement: StmtId, owner: &OwnerKey, path: &[PathStep], namespace: Option<Name>) -> Result<(), BodyContractError> {
        let value = match self.program.arena.stmt(statement).kind {
            ArenaStmtKind::Return(Some(value)) | ArenaStmtKind::Yield(value) | ArenaStmtKind::Defer(value, _)
            | ArenaStmtKind::Assign { value, .. } => value,
            ArenaStmtKind::Let { initializer, .. } | ArenaStmtKind::Var { initializer, .. } => initializer,
            _ => return Ok(()),
        };
        let ArenaExprOrRun::Run(_) = value else { return Ok(()); };
        let span = super::expr::expr_or_run_span_arena(self.program, value);
        let mut path = path.to_vec();
        path.push(PathStep::RunValue);
        let selection = self.node_selection(span, namespace)?;
        self.seen_spans.insert(span);
        self.sites.entry(BodyContractSite::Expression(span)).or_default()
            .insert((owner.clone(), CanonicalBodyContractSite(SitePath::Expression(path.clone()))));
        let body = self.bodies.get_mut(owner).unwrap();
        body.span_paths.entry(span).or_insert_with(|| path.clone());
        if body.selections.insert(path, selection).is_some() { return Err(BodyContractError::UnmappedFact(span)); }
        Ok(())
    }

    fn shorthand_fields(&mut self, expression: ExprId, owner: &OwnerKey, path: &[PathStep], namespace: Option<Name>)
        -> Result<(), BodyContractError>
    {
        let arena = &self.program.arena;
        let ArenaExprKind::Record(fields) = arena.expr(expression).kind else { return Ok(()); };
        for (index, field) in arena.record_fields(fields).iter().enumerate() {
            let ArenaRecordFieldKind::Shorthand { span, .. } = field.kind else { continue; };
            let span = arena.span(span);
            if !self.checked.selected_function_calls.contains_key(&span) { continue; }
            let mut field_path = path.to_vec();
            field_path.push(PathStep::RecordField(index));
            let selection = self.node_selection(span, namespace)?;
            self.seen_spans.insert(span);
            let body = self.bodies.get_mut(owner).unwrap();
            body.span_paths.entry(span).or_insert_with(|| field_path.clone());
            if body.selections.insert(field_path, selection).is_some() { return Err(BodyContractError::UnmappedFact(span)); }
        }
        Ok(())
    }

    fn pipeline_stages(&mut self, expression: ExprId, owner: &OwnerKey, path: &[PathStep], namespace: Option<Name>)
        -> Result<(), BodyContractError>
    {
        let arena = &self.program.arena;
        let stages = match arena.expr(expression).kind {
            ArenaExprKind::Pipeline { stages, .. } => arena.pipe_stages(stages).iter().map(|stage| {
                let kind = match stage.kind { ArenaPipeStageKind::Stream(ref stage) => Some(stage.kind), ArenaPipeStageKind::Expr(_) => None };
                (arena.span(stage.span), kind)
            }).collect::<Vec<_>>(),
            ArenaExprKind::StructuredPipeline { stages, .. } => arena.stream_stages(stages).iter()
                .map(|stage| (arena.span(stage.span), Some(stage.kind))).collect(),
            _ => Vec::new(),
        };
        for (index, (span, kind)) in stages.into_iter().enumerate() {
            let mut stage_path = path.to_vec();
            stage_path.push(PathStep::PipelineStage(index));
            self.seen_spans.insert(span);
            let mut selection = self.node_selection(span, namespace)?;
            selection.stage_kind = kind;
            if selection.stage.is_some() { self.seen_stages.insert((namespace, span)); }
            let body = self.bodies.get_mut(owner).unwrap();
            body.span_paths.entry(span).or_insert_with(|| stage_path.clone());
            if body.selections.insert(stage_path, selection).is_some() { return Err(BodyContractError::UnmappedFact(span)); }
        }
        Ok(())
    }

    fn api_selection(&self, call: &CheckedApiCall, expression: Option<ExprId>, span: Span) -> Result<SelectedOperation, BodyContractError> {
        let receiver = if call.receiver.is_some() {
            let expression = expression.ok_or(BodyContractError::MissingReceiver(span))?;
            let node = self.program.arena.expr(expression);
            let span = node.span;
            let callee = match node.kind { ArenaExprKind::Call { callee, .. } => self.program.arena.expr(callee), _ => node };
            let (base, null_safe) = match callee.kind {
                ArenaExprKind::Field { base, .. } => (base, false),
                ArenaExprKind::NullSafeField { base, .. } => (base, true),
                _ => return Err(BodyContractError::MissingReceiver(span)),
            };
            let checked_receiver = self.checked.expr_types.get(&self.program.arena.expr(base).span)
                .ok_or(BodyContractError::MissingReceiver(span))?;
            Some(if null_safe {
                match checked_receiver {
                    Type::Optional(inner) | Type::Result(inner, _) => inner.as_ref(),
                    _ => return Err(BodyContractError::MissingReceiver(span)),
                }
            } else { checked_receiver })
        } else { None };
        Ok(SelectedOperation { receiver: call.receiver, signature: call.sig, argument_slots: call.argument_slots.clone(),
            argument_checks: call.params.iter().enumerate().map(|(slot, parameter)| registered_argument_check(receiver, slot, parameter)).collect() })
    }

    fn declaration_target(&self, target: super::CheckedFunctionTarget, span: Span) -> Result<(Arc<str>, Name), BodyContractError> {
        let declaration = self.checked.checked_declarations.functions.get(&target.definition)
            .filter(|declaration| declaration.name == target.name && declaration.namespace == target.namespace)
            .ok_or(BodyContractError::UnmappedFact(span))?;
        Ok((self.source_name(declaration.body.source_id)?, declaration.name))
    }

    fn node_selection(&self, span: Span, namespace: Option<Name>) -> Result<NodeSelection, BodyContractError> {
        let checked = self.checked;
        let conversion = checked.conversions.get(&span).map(|conversion| checked.expr_types.get(&span)
            .cloned().map(|ty| (*conversion, ty)).ok_or(BodyContractError::UnmappedFact(span))).transpose()?;
        let user_call = checked.selected_function_calls.get(&span).map(|&target| self.declaration_target(target, span)).transpose()?;
        let alias = checked.static_callable_aliases.get(&span).map(|alias| {
            let definition = alias.definition.map(|body| {
                let mut declarations = checked.checked_declarations.functions.values().filter(|declaration| declaration.body == body);
                let declaration = declarations.next().ok_or(BodyContractError::UnmappedFact(span))?;
                if declarations.next().is_some() { return Err(BodyContractError::UnmappedFact(span)); }
                Ok((self.source_name(body.source_id)?, declaration.name))
            }).transpose()?;
            Ok::<_, BodyContractError>((alias.name, alias.method_call, alias.signature.clone(), alias.kind, definition))
        }).transpose()?;
        let facts = &checked.checked_control_flow;
        let mut control = BTreeSet::new();
        for (kind, spans) in control_sets(checked) { if spans.contains(&span) { control.insert(kind); } }
        let selection = NodeSelection {
            binary: None,
            requirement: checked.requirement_targets.get(&span).cloned(),
            constructor: checked.record_constructor_instances.get(&span).cloned(),
            constructor_fields: checked.record_constructor_fields.get(&span).cloned(), conversion,
            projection: checked.projections.get(&span).cloned(), typed_call: checked.typed_callable_calls.get(&span).cloned(), user_call, alias,
            arguments: checked.argument_bindings.get(&span).map(|binding| (binding.callable_entry, binding.argument_slots.clone())),
            variant: checked.inferred_variants.get(&span).cloned(), pattern_variant: checked.inferred_variant_patterns.get(&span).cloned(),
            error_constructor: checked.error_constructors.get(&span).cloned(), from_end: checked.from_end_indexes.get(&span).copied(),
            stage: checked.stream_stage_types.get(&(namespace, span)).map(|stage| (stage.input.clone(), stage.output.clone())),
            stage_kind: None,
            handler_input: checked.handler_input_types.get(&span).cloned(), position: checked.statement_positions.get(&span).copied(), control,
            result_constructor: facts.result_constructors.get(&span).copied(), collect_target: checked.collect_yields.get(&span).copied(),
            resources: checked.resource_scopes.get(&span).cloned(), transfers: self.transfers.get(&span).cloned().unwrap_or_default(),
            text_pattern: checked.text_patterns.get(&span).cloned(),
        };
        let mut types = selection.requirement.iter().map(|target| &target.ty)
            .chain(selection.constructor.iter().map(|constructor| &constructor.ty))
            .chain(selection.conversion.iter().map(|(_, ty)| ty))
            .chain(selection.projection.iter().map(|projection| &projection.value_type))
            .chain(selection.handler_input.iter()).collect::<Vec<_>>();
        let mut contexts = selection.requirement.iter().map(|target| &target.context).collect::<Vec<_>>();
        while let Some(context) = contexts.pop() {
            for instance in &context.instances { types.extend(&instance.arguments); }
            contexts.extend(context.children.values());
        }
        if let Some(constructor) = &selection.constructor { types.extend(&constructor.instance.arguments); }
        if let Some(call) = &selection.typed_call {
            types.extend(call.sig.params.iter().map(|parameter| &parameter.ty));
            types.push(&call.sig.return_ty);
        }
        if let Some((_, _, signature, _, _)) = &selection.alias {
            types.extend(signature.params.iter().map(|parameter| &parameter.ty));
            types.push(&signature.return_ty);
        }
        if let Some((input, output)) = &selection.stage { types.extend([input, output]); }
        if types.into_iter().any(|ty| ty.contains_inference() || ty.contains_recovery())
            || selection.projection.as_ref().is_some_and(|projection| matches!(projection.callable, Some(ModuleExportType::GenericCallable { .. })))
        {
            return Err(BodyContractError::UnsupportedSelection(span));
        }
        Ok(selection)
    }

    fn finish(self) -> Result<Vec<SemanticBodyContract>, BodyContractError> {
        let relevant = |span: &Span| self.user_sources.contains(&span.source_id);
        for span in self.checked.api_calls.keys().filter(|span| relevant(span)) {
            if !self.seen_api.contains(span) { return Err(BodyContractError::UnmappedFact(*span)); }
        }
        for span in self.checked.module_commands.keys().filter(|span| relevant(span)) {
            if !self.seen_commands.contains(span) { return Err(BodyContractError::UnmappedFact(*span)); }
        }
        for expression in self.checked.prepared_constants.binary_operators.keys().filter(|expression| relevant(&self.program.arena.expr(**expression).span)) {
            if !self.seen_prepared_binary.contains(expression) { return Err(BodyContractError::UnmappedFact(self.program.arena.expr(*expression).span)); }
        }
        for expression in self.checked.binary_operators.keys().filter(|expression| relevant(&self.program.arena.expr(**expression).span)) {
            if !self.seen_binary.contains(expression) { return Err(BodyContractError::UnmappedFact(self.program.arena.expr(*expression).span)); }
        }
        for expression in self.registered.keys() {
            if !self.seen_registered.contains(expression) { return Err(BodyContractError::UnmappedFact(self.program.arena.expr(*expression).span)); }
        }
        for identity in self.checked.function_effect_facts.keys().filter(|identity| relevant(&identity.body)) {
            if !self.seen_effects.contains(identity) { return Err(BodyContractError::UnmappedFact(identity.body)); }
        }
        for stage in self.checked.stream_stage_types.keys().filter(|(_, span)| relevant(span)) {
            if !self.seen_stages.contains(stage) { return Err(BodyContractError::UnmappedFact(stage.1)); }
        }
        let checked = self.checked;
        let spans = checked.requirement_targets.keys().chain(checked.record_constructor_instances.keys())
            .chain(checked.record_constructor_fields.keys()).chain(checked.conversions.keys()).chain(checked.projections.keys())
            .chain(checked.typed_callable_calls.keys()).chain(checked.selected_function_calls.keys()).chain(checked.static_callable_aliases.keys())
            .chain(checked.argument_bindings.keys()).chain(checked.inferred_variants.keys()).chain(checked.inferred_variant_patterns.keys())
            .chain(checked.error_constructors.keys()).chain(checked.from_end_indexes.keys()).chain(checked.handler_input_types.keys())
            .chain(checked.statement_positions.keys()).chain(checked.collect_yields.keys()).chain(checked.resource_scopes.keys())
            .chain(self.transfers.keys()).chain(checked.text_patterns.keys()).chain(checked.path_literals.iter())
            .chain(checked.optional_binding_spans.iter()).chain(checked.propagating_statements.iter()).chain(checked.propagating_conditions.iter())
            .chain(checked.proven_nonnull_fallback_receivers.iter()).chain(checked.definitely_exiting_block_spans.iter());
        for span in spans.filter(|span| relevant(span)) {
            if !self.seen_spans.contains(span) { return Err(BodyContractError::UnmappedFact(*span)); }
        }
        for span in control_sets(checked).into_iter().flat_map(|(_, spans)| spans).chain(checked.checked_control_flow.result_constructors.keys())
            .filter(|span| relevant(span))
        {
            if !self.seen_spans.contains(span) { return Err(BodyContractError::UnmappedFact(*span)); }
        }
        for body in self.bodies.values() {
            for selection in body.selections.values() {
                if let Some(projection) = &selection.projection {
                    if !body.expr_paths.contains_key(&projection.receiver) || !body.expr_paths.contains_key(&projection.key) {
                        return Err(BodyContractError::UnmappedFact(self.program.arena.expr(projection.receiver).span));
                    }
                }
                if let Some(span) = selection.collect_target {
                    if !body.span_paths.contains_key(&span) { return Err(BodyContractError::UnmappedFact(span)); }
                }
            }
        }
        Ok(self.bodies.into_values().collect())
    }
}

fn control_sets(checked: &CheckOutput) -> [(ControlFact, &BTreeSet<Span>); 15] {
    let facts = &checked.checked_control_flow;
    [
            (ControlFact::NonnullFallback, &checked.proven_nonnull_fallback_receivers),
            (ControlFact::PropagatingStatement, &checked.propagating_statements),
            (ControlFact::DiscardableBinding, &checked.discardable_bindings),
            (ControlFact::PropagatingCondition, &checked.propagating_conditions),
            (ControlFact::RedundantConditionPropagation, &checked.redundant_condition_propagations),
            (ControlFact::ExitingBlock, &checked.definitely_exiting_block_spans),
            (ControlFact::PathLiteral, &checked.path_literals),
            (ControlFact::OptionalBinding, &checked.optional_binding_spans),
            (ControlFact::BuiltinRange, &facts.builtin_range_calls),
            (ControlFact::CardinalityPreserving, &facts.cardinality_preserving_collection_calls),
            (ControlFact::PropagatingError, &facts.propagating_error_spans),
            (ControlFact::LatentStreamError, &facts.latent_stream_error_spans),
            (ControlFact::FallibleStage, &facts.fallible_pipeline_stage_spans),
            (ControlFact::PropagatingStageCallback, &facts.propagating_stage_callback_spans),
            (ControlFact::ExhaustiveMatch, &facts.exhaustive_match_spans),
        ]
}

fn function_definition(kind: ArenaStmtKind) -> Option<crate::syntax::arena::FunctionDefId> {
    match kind {
        ArenaStmtKind::ProcDef(id) | ArenaStmtKind::PureDef(id) | ArenaStmtKind::StreamDef(id) | ArenaStmtKind::CliMain(id) => Some(id),
        _ => None,
    }
}

fn children(program: &ArenaProgram, node: ArenaChild) -> Vec<ArenaChild> {
    let arena = &program.arena;
    let mut children = Vec::new();
    let mut visit = |child| { if !matches!(child, ArenaChild::TypeExpr(_)) { children.push(child); } };
    match node {
        ArenaChild::Stmt(id) => arena.for_each_stmt_child(id, SugarView::Core, &mut visit),
        ArenaChild::Expr(id) => arena.for_each_expr_child(id, &mut visit),
        ArenaChild::Pattern(id) => arena.for_each_pattern_child(id, &mut visit),
        ArenaChild::Block(id) => arena.for_each_block_child(id, &mut visit),
        ArenaChild::BindingTarget(id) => arena.for_each_binding_target_child(id, &mut visit),
        ArenaChild::AssignTarget(id) => arena.for_each_assign_target_child(id, &mut visit),
        ArenaChild::BuilderBlock(id) => arena.for_each_builder_child(id, &mut visit),
        ArenaChild::TypeExpr(_) => {}
    }
    children
}

fn node_span(program: &ArenaProgram, node: ArenaChild) -> Option<Span> {
    let arena = &program.arena;
    match node {
        ArenaChild::Stmt(id) => Some(arena.stmt(id).span),
        ArenaChild::Expr(id) => Some(arena.expr(id).span),
        ArenaChild::Pattern(id) => Some(arena.span(arena.pattern(id).span)),
        ArenaChild::Block(id) => Some(arena.span(arena.block(id).span)),
        ArenaChild::BindingTarget(id) => arena.binding_target(id).span.map(|span| arena.span(span)),
        ArenaChild::BuilderBlock(id) => Some(arena.span(arena.builder_block(id).span)),
        ArenaChild::AssignTarget(_) | ArenaChild::TypeExpr(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::{CheckOptions, Checker};
    use crate::syntax::parser::Parser;

    fn checked(source: &str) -> (ArenaProgram, SourceMap, CheckOutput) {
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("<body-contract>", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        (parsed.arena, sources, checked)
    }

    fn snapshots(source: &str) -> Vec<SemanticBodyContract> {
        let (program, sources, checked) = checked(source);
        PendingBodyContracts::capture(&checked, &program, &sources).map(PendingBodyContracts::into_contracts).unwrap()
    }

    fn declaration(source: &str) -> SemanticBodyContract {
        snapshots(source).into_iter().find(|body| matches!(body.owner(), BodyContractOwner::Declaration(_))).unwrap()
    }

    fn entry(source: &str) -> SemanticBodyContract {
        snapshots(source).into_iter().find(|body| body.owner() == BodyContractOwner::Entry).unwrap()
    }

    #[test]
    fn checked_body_contract_resource_specialization_requires_same_header_binders_and_operand_roles() {
        let (_, _, old_checked) = checked("pure choose(first: Int, second: Str) -> Int { first }\n");
        let (_, _, new_checked) = checked("pure choose(first, second) { first }\n");
        let old = old_checked.checked_declarations.functions.values().next().unwrap().callable_contract().unwrap();
        let declaration = new_checked.checked_declarations.functions.values().next().unwrap();
        let new = declaration.callable_contract().unwrap();
        let scheme = declaration.scheme.as_ref().unwrap();
        let instance = old.instance_evidence(&new).unwrap().unwrap();
        let operand = |ordinal: usize| OperandType::Scheme { ty: scheme.params()[ordinal].ty.clone(), owner: Arc::clone(scheme) };
        for role in [TransferOperand::Expression as fn(OperandType) -> TransferOperand, TransferOperand::Run, TransferOperand::BareTail, TransferOperand::DelegatedExpression] {
            let old = TransferCause { runtime_flag: false, operand: role(OperandType::Ground(Type::Int)) };
            let candidate = TransferCause { runtime_flag: true, operand: role(operand(0)) };
            assert!(old.preserved_by(&candidate, Some(&instance)));
            assert!(!candidate.preserved_by(&old, Some(&instance)));
            assert!(!old.preserved_by(&candidate, None));
            assert!(!old.preserved_by(&TransferCause { runtime_flag: true, operand: role(operand(1)) }, Some(&instance)));
            assert!(!old.preserved_by(&TransferCause { runtime_flag: true, operand: role(OperandType::Missing) }, Some(&instance)));
            let foreign = OperandType::Scheme { ty: scheme.params()[0].ty.clone(), owner: Arc::new((**scheme).clone()) };
            assert!(!old.preserved_by(&TransferCause { runtime_flag: true, operand: role(foreign) }, Some(&instance)));
        }
        for (old_type, new_type) in [
            (Type::List(Box::new(Type::Optional(Box::new(Type::Int)))),
             SchemeType::List(Box::new(SchemeType::Optional(Box::new(scheme.params()[0].ty.clone()))))),
            (Type::Map(Box::new(Type::Str), Box::new(Type::Set(Box::new(Type::Int)))),
             SchemeType::Map(Box::new(SchemeType::Ground(Type::Str)), Box::new(SchemeType::Set(Box::new(scheme.params()[0].ty.clone()))))),
        ] {
            let old = TransferCause { runtime_flag: false, operand: TransferOperand::Expression(OperandType::Ground(old_type)) };
            let new = TransferCause { runtime_flag: true, operand: TransferOperand::Expression(OperandType::Scheme { ty: new_type, owner: Arc::clone(scheme) }) };
            assert!(old.preserved_by(&new, Some(&instance)));
        }
        for ty in [Type::Any, Type::Record(Arc::new(BTreeMap::new())), Type::Tag(declaration.name)] {
            let old = TransferCause { runtime_flag: false, operand: TransferOperand::Expression(OperandType::Ground(ty.clone())) };
            let new = TransferCause { runtime_flag: true, operand: TransferOperand::Expression(OperandType::Ground(ty)) };
            assert!(!old.preserved_by(&new, Some(&instance)));
        }
        let old_arm = || TransferCause { runtime_flag: false, operand: TransferOperand::Expression(OperandType::Ground(Type::Int)) };
        let new_arm = || TransferCause { runtime_flag: true, operand: TransferOperand::Expression(operand(0)) };
        let old_match = TransferCause { runtime_flag: false, operand: TransferOperand::Match(vec![old_arm(), old_arm()]) };
        let new_match = TransferCause { runtime_flag: true, operand: TransferOperand::Match(vec![new_arm(), new_arm()]) };
        assert!(old_match.preserved_by(&new_match, Some(&instance)));
        let changed_match = TransferCause { runtime_flag: true, operand: TransferOperand::Match(vec![old_arm(), new_arm()]) };
        assert!(!new_match.preserved_by(&changed_match, Some(&instance)));
        for operand in [TransferOperand::Empty, TransferOperand::NoValue, TransferOperand::SameScopeAssignment, TransferOperand::Missing] {
            let original = TransferCause { runtime_flag: false, operand: operand.clone() };
            assert!(!original.preserved_by(&TransferCause { runtime_flag: true, operand }, Some(&instance)));
        }
        let original = TransferCause { runtime_flag: false, operand: TransferOperand::Expression(OperandType::Ground(Type::Int)) };
        let changed = TransferCause { runtime_flag: true, operand: TransferOperand::BareTail(operand(0)) };
        assert!(!original.preserved_by(&changed, Some(&instance)));
    }

    #[test]
    fn checked_body_contract_resource_specialization_rejects_missing_tail_authority() {
        let original = declaration("pure keep(value: Int) -> Int { value }\n");
        let (_, _, old_checked) = checked("pure keep(value: Int) -> Int { value }\n");
        let old_interface = old_checked.checked_declarations.functions.values().next().unwrap().callable_contract().unwrap();
        for missing_position in [true, false] {
            let (program, sources, mut checked) = checked("pure keep(value) { value }\n");
            let declaration = checked.checked_declarations.functions.values_mut().next().unwrap();
            let interface = declaration.callable_contract().unwrap();
            if missing_position { checked.statement_positions.clear(); }
            else {
                declaration.scheme_body_facts = Some(crate::sema::schemes::SchemeBodyFacts::new(
                    Arc::clone(declaration.scheme.as_ref().unwrap()), BTreeMap::new(), BTreeMap::new()).unwrap());
            }
            let candidate = PendingBodyContracts::capture(&checked, &program, &sources).unwrap().into_contracts().into_iter()
                .find(|body| matches!(body.owner(), BodyContractOwner::Declaration(_))).unwrap();
            assert!(candidate.selections.values().flat_map(|selection| &selection.transfers).any(|transfer|
                matches!(transfer.cause.operand, TransferOperand::Missing | TransferOperand::BareTail(OperandType::Missing))));
            let instance = old_interface.instance_evidence(&interface).unwrap().unwrap();
            assert!(!original.preserved_by_instance(&candidate, Some(&instance)));
        }
    }

    #[test]
    fn checked_body_contract_preserves_match_transfer_causes_and_disjunction() {
        let scalar = || TransferCause { runtime_flag: false, operand: TransferOperand::Expression(OperandType::Ground(Type::Int)) };
        let old = TransferCause { runtime_flag: false, operand: TransferOperand::Match(vec![scalar(), scalar()]) };
        assert!(old.preserved_by(&old, None));
        let mut changed = old.clone();
        let TransferOperand::Match(arms) = &mut changed.operand else { unreachable!() };
        arms[0].runtime_flag = true;
        assert!(!old.preserved_by(&changed, None));
        changed.runtime_flag = true;
        assert!(!old.preserved_by(&changed, None));
        let mut missing = old.clone();
        let TransferOperand::Match(arms) = &mut missing.operand else { unreachable!() };
        arms[0].operand = TransferOperand::Missing;
        assert!(!old.preserved_by(&missing, None));
    }

    #[test]
    fn checked_body_contract_captures_analyzer_assignment_roles() {
        let (program, sources, checked) = checked("proc keep(value: Int) [] -> Int { var result = value; result = value; { result = value }; result }\n");
        assert_eq!(checked.same_scope_resource_assignments.len(), 1);
        let contracts = PendingBodyContracts::capture(&checked, &program, &sources).unwrap().into_contracts();
        let assignments = contracts.iter().flat_map(|body| body.selections.values()).flat_map(|selection| &selection.transfers)
            .filter(|transfer| transfer.kind == TransferKind::Assignment).collect::<Vec<_>>();
        assert_eq!(assignments.len(), 2);
        assert!(assignments.iter().any(|transfer| matches!(transfer.cause.operand, TransferOperand::SameScopeAssignment)));
        assert!(assignments.iter().any(|transfer| matches!(transfer.cause.operand, TransferOperand::Expression(_))));
    }

    #[test]
    fn checked_body_contract_captures_checked_transfer_operand_roles() {
        let source = "pure branch(value: Int) -> Int { match true { true => value; false => value } }\npure stop(value: Int) -> Int { let result = loop { break value }; result }\nstream child(value: Int) [] -> Stream[Int] { yield value }\nstream parent(value: Int) [] -> Stream[Int] { yield @child(value) }\nproc command() [process, error] -> Result[Str] { return run.text /bin/pwd ? }\n";
        let contracts = snapshots(source);
        let transfers = contracts.iter().flat_map(|body| body.selections.values()).flat_map(|selection| &selection.transfers).collect::<Vec<_>>();
        assert!(transfers.iter().any(|transfer| transfer.kind == TransferKind::BlockValue
            && matches!(&transfer.cause.operand, TransferOperand::Match(arms) if arms.len() == 2 && arms.iter().all(|arm| !arm.runtime_flag))));
        assert!(transfers.iter().any(|transfer| transfer.kind == TransferKind::Break && matches!(transfer.cause.operand, TransferOperand::Expression(_))));
        assert!(transfers.iter().any(|transfer| transfer.kind == TransferKind::Yield && matches!(transfer.cause.operand, TransferOperand::Expression(_))));
        assert!(transfers.iter().any(|transfer| transfer.kind == TransferKind::Yield && matches!(transfer.cause.operand, TransferOperand::DelegatedExpression(_))));
        assert!(transfers.iter().any(|transfer| transfer.kind == TransferKind::Return && matches!(transfer.cause.operand, TransferOperand::Run(_))));
    }

    #[test]
    fn checked_body_contract_preserves_run_value_control_selections() {
        let source = "proc command() [process, error] -> Result[Str] { return run.text /bin/pwd ? }\n";
        let original = declaration(source);
        let shifted = declaration(&format!("type Unrelated = Int\n{source}"));
        assert!(original.preserved_by(&shifted));
        let mut changed = shifted;
        let selection = changed.selections.values_mut().find(|selection| selection.control.contains(&ControlFact::PropagatingError)).unwrap();
        selection.control.remove(&ControlFact::PropagatingError);
        assert!(!original.preserved_by(&changed));
    }

    #[test]
    fn checked_body_contract_preserves_operations_across_annotations_and_arena_shifts() {
        let first = declaration("pure accepts(value: Str) -> Bool { let prefix: Str = \"x\"; value.starts_with(prefix) }\n");
        let shifted = declaration("type Unrelated = Int\n\npure accepts(value: Str) -> Bool { let prefix = \"x\"; value.starts_with(prefix) }\n");
        assert!(!first.operations.is_empty());
        assert!(first.preserved_by(&shifted));
        assert!(shifted.preserved_by(&first));
    }

    #[test]
    fn checked_body_contract_captures_null_safe_method_receivers() {
        let source = "proc decode(value: Int) [error] -> Str { bytes.from_ints([value])?.utf8()? }\nproc lookup(value: Result[Map[UInt, Str]]) [error] -> Result[Str] { value?.get(1) }\n";
        let (program, sources, checked) = checked(source);
        let (span, _) = checked.api_calls.iter().find(|(_, call)| call.receiver == Some(MethodReceiver::Bytes)).unwrap();
        let expression = program.arena.source_rows().expr_ids(span.source_id).find(|&expression| program.arena.expr(expression).span == *span).unwrap();
        let ArenaExprKind::Call { callee, .. } = program.arena.expr(expression).kind else { panic!("checked byte method call") };
        let ArenaExprKind::NullSafeField { base, .. } = program.arena.expr(callee).kind else { panic!("checked safe byte method receiver") };
        assert!(matches!(checked.expr_types.get(&program.arena.expr(base).span), Some(Type::Result(ok, _)) if **ok == Type::Bytes));
        let contracts = PendingBodyContracts::capture(&checked, &program, &sources).unwrap().into_contracts();
        let operations = contracts.iter().flat_map(|body| body.operations.values()).map(|operation| &operation.selection).collect::<Vec<_>>();
        let bytes = operations.iter().find(|selection| selection.receiver == Some(MethodReceiver::Bytes)).unwrap();
        let map = operations.iter().find(|selection| selection.receiver == Some(MethodReceiver::Map)).unwrap();
        assert!(bytes.argument_checks.is_empty());
        assert_eq!(map.argument_checks, [Some(crate::sema::schemes::RegisteredArgumentCheck::UIntMapKey)]);

        let mut missing_receiver = checked.clone();
        missing_receiver.expr_types.remove(&program.arena.expr(base).span);
        assert_eq!(PendingBodyContracts::capture(&missing_receiver, &program, &sources)
            .map(PendingBodyContracts::into_contracts).unwrap_err(), BodyContractError::MissingReceiver(*span));
        let mut invalid_receiver = checked.clone();
        invalid_receiver.expr_types.insert(program.arena.expr(base).span, Type::Bytes);
        assert_eq!(PendingBodyContracts::capture(&invalid_receiver, &program, &sources)
            .map(PendingBodyContracts::into_contracts).unwrap_err(), BodyContractError::MissingReceiver(*span));
    }

    #[test]
    fn checked_body_contract_preserves_registered_operations_after_generalization() {
        let original = "pure keep(value: Int) -> Bool { \"x\".starts_with(\"x\") }\n";
        let candidate = "pure keep(value) { \"x\".starts_with(\"x\") }\n";
        let (program, sources, checked) = checked(candidate);
        assert!(checked.checked_declarations.functions.values().any(|declaration| declaration.scheme_body_facts.as_ref()
            .is_some_and(|facts| !facts.registered_calls().is_empty())));
        let candidate = PendingBodyContracts::capture(&checked, &program, &sources).map(PendingBodyContracts::into_contracts).unwrap().into_iter()
            .find(|body| matches!(body.owner(), BodyContractOwner::Declaration(_))).unwrap();
        assert!(declaration(original).preserved_by(&candidate));
    }

    #[test]
    fn checked_body_contract_preserves_nonmaterial_constructor_operations() {
        let original = "pure keep(payload: Int) -> Int { let _ = {items: map.empty(), payload}; payload }\n";
        let candidate = "pure keep(payload) { let _ = {items: map.empty(), payload}; payload }\n";
        let (_, _, old_checked) = checked(original);
        let old_interface = old_checked.checked_declarations.functions.values().next().unwrap().callable_contract().unwrap();
        let (program, sources, new_checked) = checked(candidate);
        let new_declaration = new_checked.checked_declarations.functions.values().next().unwrap();
        let facts = new_declaration.scheme_body_facts.as_ref().unwrap();
        assert!(facts.constructor_producers().values().any(|kind| *kind == crate::sema::schemes::NonmaterialConstructor::EmptyMap));
        let instance = old_interface.instance_evidence(&new_declaration.callable_contract().unwrap()).unwrap().unwrap();
        let candidate = PendingBodyContracts::capture(&new_checked, &program, &sources).unwrap().into_contracts().into_iter()
            .find(|body| matches!(body.owner(), BodyContractOwner::Declaration(_))).unwrap();
        let original = declaration(original);
        assert!(!original.operations.is_empty());
        assert!(original.preserved_by_instance(&candidate, Some(&instance)));
        let mut missing_operation = candidate;
        assert!(missing_operation.operations.pop_first().is_some());
        assert!(!original.preserved_by_instance(&missing_operation, Some(&instance)));
    }

    #[test]
    fn checked_body_contract_rejects_native_selection_and_argument_mutations() {
        let original = declaration("pure accepts(value: Str) -> Bool { value.starts_with(\"x\") }\n");
        let other = declaration("pure accepts(value: Str) -> Bool { value.ends_with(\"x\") }\n");
        assert!(!original.preserved_by(&other));
        let mut receiver = original.clone();
        receiver.operations.values_mut().next().unwrap().selection.receiver = None;
        assert!(!original.preserved_by(&receiver));
        let mut slots = original.clone();
        slots.operations.values_mut().next().unwrap().selection.argument_slots.clear();
        assert!(!original.preserved_by(&slots));
    }

    #[test]
    fn checked_body_contract_distinguishes_signatures_with_the_same_runtime_operation() {
        let parent = declaration("pure parent(value: Path) -> Path { value.parent() }\n");
        let dirname = declaration("pure parent(value: Path) -> Path { value.dirname() }\n");
        let parent_op = parent.operations.values().next().unwrap();
        let dirname_op = dirname.operations.values().next().unwrap();
        assert_eq!(parent_op.selection.signature.op, dirname_op.selection.signature.op);
        assert!(!parent.preserved_by(&dirname));
    }

    #[test]
    fn checked_body_contract_preserves_command_argument_treatment() {
        let original = declaration("proc unmount(target: Path) { linux.umount $target --lazy }\n");
        let mut changed = original.clone();
        let operation = changed.operations.values_mut().find(|operation| operation.command_arguments.is_some()).unwrap();
        assert!(!operation.command_arguments.as_ref().unwrap().is_empty());
        let argument = operation.command_arguments.as_mut().unwrap().iter_mut()
            .find(|argument| matches!(argument, CommandArgument::BooleanFlag)).unwrap();
        *argument = CommandArgument::Value;
        assert!(!original.preserved_by(&changed));
    }

    #[test]
    fn checked_body_contract_accepts_only_qualified_redundant_effect_removal() {
        let original = declaration("proc clock() [time] -> Int { time.now() }\n");
        let candidate = declaration("proc clock() -> Int { time.now() }\n");
        assert!(original.effect_fact().unwrap().redundant_clause, "{:?}", original.effect_fact());
        assert!(original.preserved_by(&candidate));
        assert!(!candidate.preserved_by(&original));
        let excess = declaration("proc clock() [fs, time] -> Int { time.now() }\n");
        assert!(!excess.preserved_by(&candidate));
        let mut recursive = original.clone();
        recursive.effects.as_mut().unwrap().redundant_clause = false;
        assert!(!recursive.preserved_by(&candidate));
        let mut disallowed = original.clone();
        disallowed.effects.as_mut().unwrap().inference_allowed = false;
        assert!(!disallowed.preserved_by(&candidate));
    }

    #[test]
    fn checked_body_contract_preserves_effect_sets_and_unknown_summaries() {
        let first = declaration("proc say() [io, fs] { print \"hello\" }\n");
        let reordered = declaration("proc say() [fs, io] { print \"hello\" }\n");
        assert!(first.preserved_by(&reordered));
        let mut changed = first.clone();
        changed.effects.as_mut().unwrap().required = crate::sema::effects::CheckedEffects::finite([crate::syntax::node::Effect::Fs]);
        assert!(!first.preserved_by(&changed));
        let mut unknown = first.clone();
        unknown.effects.as_mut().unwrap().required = crate::sema::effects::CheckedEffects::unknown();
        unknown.effects.as_mut().unwrap().unknown_chain = vec!["opaque callback".to_owned()];
        assert!(!first.preserved_by(&unknown));
        assert!(unknown.preserved_by(&unknown));
        let mut inferred_unknown = unknown.clone();
        inferred_unknown.effects.as_mut().unwrap().inferred = true;
        assert!(!unknown.preserved_by(&inferred_unknown));
    }

    #[test]
    fn checked_body_contract_preserves_canonical_validation_and_constructor_schemas() {
        let first = declaration("type Entry = {label: Str}\npure parse(value: Any) -> Result[Entry] { value.require(Entry) }\n");
        let shifted = declaration("type Unrelated = Int\ntype Entry = {label: Str}\npure parse(value: Any) -> Result[Entry] { value.require(Entry) }\n");
        assert!(first.selections.values().any(|selection| selection.requirement.is_some()));
        assert!(first.preserved_by(&shifted));
        let renamed = declaration("type Other = {label: Str}\npure parse(value: Any) -> Result[Other] { value.require(Other) }\n");
        assert!(!first.preserved_by(&renamed));
        let first = declaration("type Entry = {label: Str}\npure make(value: Str) -> Entry { Entry(label: value) }\n");
        let shifted = declaration("type Unrelated = Int\ntype Entry = {label: Str}\npure make(value: Str) -> Entry { Entry(label: value) }\n");
        assert!(first.selections.values().any(|selection| selection.constructor.is_some()));
        assert!(first.preserved_by(&shifted));
    }

    #[test]
    fn checked_body_contract_preserves_conversion_and_projection_choices() {
        let conversion = declaration("proc parse(value: Str) { let number = value as Int; print $number }\n");
        let mut changed = conversion.clone();
        changed.selections.values_mut().find_map(|selection| selection.conversion.as_mut()).unwrap().0 = Conversion::TextToUInt;
        assert!(!conversion.preserved_by(&changed));
        let source = "type Config = {workers: Int}\nproc read(config: Config) [error] { let _ = config.get(\"workers\")? }\n";
        let projection = declaration(source);
        let mut changed = projection.clone();
        let selected = changed.selections.values_mut().find_map(|selection| selection.projection.as_mut()).unwrap();
        selected.operation = crate::sema::projection::ProjectionOperation::Index;
        assert!(!projection.preserved_by(&changed));
    }

    #[test]
    fn checked_body_contract_preserves_typed_calls_and_selected_user_declarations() {
        let source = "pure integer(value: Int) -> Int { value }\npure apply(callback: pure(value: Int) -> Int) -> Int { callback(1) }\nlet result = apply(integer)\n";
        let snapshots = snapshots(source);
        let typed = snapshots.iter().find(|body| body.selections.values().any(|selection| selection.typed_call.is_some())).unwrap();
        let mut changed = typed.clone();
        changed.selections.values_mut().find(|selection| selection.typed_call.is_some()).unwrap().typed_call = None;
        assert!(!typed.preserved_by(&changed));
        let entry = snapshots.iter().find(|body| matches!(body.owner(), BodyContractOwner::Entry)).unwrap();
        let mut changed = entry.clone();
        changed.selections.values_mut().find(|selection| selection.user_call.is_some()).unwrap().user_call = None;
        assert!(!entry.preserved_by(&changed));
    }

    #[test]
    fn checked_body_contract_retains_recursive_and_opaque_effect_failures() {
        let recursive = declaration("proc again() [error, io] { if false { again() } }\n");
        assert!(!recursive.effect_fact().unwrap().redundant_clause);
        let inferred = declaration("proc again() { if false { again() } }\n");
        assert!(!recursive.preserved_by(&inferred));
        let opaque = declaration("proc opaque(callback: Proc) -> Int { let _ = callback.call(); 42 }\n");
        assert!(!opaque.effect_fact().unwrap().required.is_known());
        assert!(!opaque.effect_fact().unwrap().unknown_chain.is_empty());
        assert!(opaque.preserved_by(&opaque));
    }

    #[test]
    fn checked_body_contract_rejects_unmapped_canonical_facts() {
        let (program, sources, mut checked) = checked("pure accepts(value: Str) -> Bool { value.starts_with(\"x\") }\n");
        let call = checked.api_calls.values().next().unwrap().clone();
        let span = Span::at(SourceId::new(0), 0);
        checked.api_calls.insert(span, call);
        assert_eq!(PendingBodyContracts::capture(&checked, &program, &sources).map(PendingBodyContracts::into_contracts).unwrap_err(), BodyContractError::UnmappedFact(span));
    }

    #[test]
    fn checked_body_contract_maps_ordinary_pipeline_stage_roles() {
        let source = "let total: Int = [1, 2] |> count()\n";
        let (program, sources, checked) = checked(source);
        assert!(!checked.stream_stage_types.is_empty());
        let original = PendingBodyContracts::capture(&checked, &program, &sources).map(PendingBodyContracts::into_contracts).unwrap();
        let candidate = snapshots("let total = [1, 2] |> count()\n");
        assert_eq!(original.len(), candidate.len());
        assert!(original.iter().zip(&candidate).all(|(old, new)| old.preserved_by(new)));
    }

    #[test]
    fn checked_body_contract_preserves_all_owners_across_unrelated_type_declarations() {
        let original = snapshots("pure length(value: Str) -> Int { value.byte_len() }\nlet count: Int = length(\"x\")\n");
        let candidate = snapshots("type Unrelated = Int\npure length(value: Str) -> Int { value.byte_len() }\nlet count = length(\"x\")\n");
        assert_eq!(original.len(), candidate.len());
        assert!(original.iter().zip(&candidate).all(|(old, new)| old.preserved_by(new)));
    }

    #[test]
    fn checked_body_contract_preserves_selected_pipeline_stage_kinds() {
        let count = declaration("pure total(values: List[Int]) -> Int { values |> count() }\n");
        let sum = declaration("pure total(values: List[Int]) -> Int { values |> sum() }\n");
        let count_types = count.selections.values().find_map(|selection| selection.stage.as_ref()).unwrap();
        let sum_types = sum.selections.values().find_map(|selection| selection.stage.as_ref()).unwrap();
        assert_eq!(count_types, sum_types);
        assert!(!count.preserved_by(&sum));
    }

    #[test]
    fn checked_body_contract_uses_canonical_module_keys_for_imported_owners_and_calls() {
        use crate::syntax::arena::ArenaProgramBuilder;
        let imported = |source_name: &str, namespace: &str| {
            let source = "use shared as api\nlet result = api.describe(\"x\")\n";
            let module_source = "##! Descriptions.\n## Measures input.\nexport pure describe(value: Str) -> Int { value.byte_len() }\n";
            let mut sources = SourceMap::new();
            let root_id = sources.add_file("<body-contract>", source);
            let module_id = sources.add_file(source_name, module_source);
            let mut builder = ArenaProgramBuilder::with_token_capacity(128);
            let root = Parser::parse_source_into_arena_builder(root_id, source, &mut builder);
            let module = Parser::parse_source_into_arena_builder(module_id, module_source, &mut builder);
            assert!(root.diagnostics.is_empty(), "{:?}", root.diagnostics);
            assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
            let name = builder.symbol_owner().intern(namespace);
            builder.push_arena_module("/canonical/library.xsh".to_owned(), name, module.statements);
            let statement = builder.statement_ids(root.statements)[0];
            let (use_id, _, _) = builder.use_stmt_for_statement(statement).unwrap();
            builder.set_use_resolved(use_id, Arc::from("/canonical/library.xsh"));
            let program = builder.finish_with_statements(root.statements);
            let checked = Checker::check_arena_with_sources_and_options(&program, &sources, root_id,
                CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            PendingBodyContracts::capture(&checked, &program, &sources).map(PendingBodyContracts::into_contracts).unwrap()
        };
        let original = imported("/first-alias/library.xsh", "shared");
        let candidate = imported("/second-alias/library.xsh", "renamed");
        assert_eq!(original.len(), candidate.len());
        assert!(original.iter().any(|body| body.source_name() == "/canonical/library.xsh"));
        assert!(original.iter().zip(&candidate).all(|(old, new)| old.preserved_by(new)));
    }

    #[test]
    fn checked_body_contract_preserves_real_control_and_resource_selections() {
        let control = declaration("proc guarded(value: Result[Bool]) { if value { print \"yes\" } }\n");
        assert!(control.selections.values().any(|selection| selection.control.contains(&ControlFact::PropagatingCondition)));
        assert!(control.preserved_by(&control));
        let mut changed = control.clone();
        changed.selections.values_mut().find(|selection| selection.control.contains(&ControlFact::PropagatingCondition))
            .unwrap().control.remove(&ControlFact::PropagatingCondition);
        assert!(!control.preserved_by(&changed));
        let resource = declaration("proc scoped() { with root = fs.tempdir()? { let _ = root.exists(p\"item\")? } }\n");
        assert!(resource.selections.values().any(|selection| selection.resources.is_some()));
        assert!(resource.preserved_by(&resource));
        let mut changed = resource.clone();
        changed.selections.values_mut().find(|selection| selection.resources.is_some()).unwrap().resources = None;
        assert!(!resource.preserved_by(&changed));
    }

    #[test]
    fn checked_body_contract_maps_shorthand_callable_reference_roles() {
        let source = "pure number() -> Int { 1 }\nlet holder = {number}\n";
        let (program, sources, checked) = checked(source);
        assert_eq!(checked.selected_function_calls.len(), 1);
        let original = PendingBodyContracts::capture(&checked, &program, &sources).map(PendingBodyContracts::into_contracts).unwrap();
        let candidate = snapshots("type Unrelated = Int\npure number() -> Int { 1 }\nlet holder = {number}\n");
        assert_eq!(original.len(), candidate.len());
        assert!(original.iter().zip(&candidate).all(|(old, new)| old.preserved_by(new)));
        let entry = original.iter().find(|body| matches!(body.owner(), BodyContractOwner::Entry)).unwrap();
        let mut changed = entry.clone();
        changed.selections.values_mut().find(|selection| selection.user_call.is_some()).unwrap().user_call = None;
        assert!(!entry.preserved_by(&changed));
    }

    #[test]
    fn checked_body_contract_rejects_unmapped_callable_reference_facts() {
        let source = "pure number() -> Int { 1 }\nlet holder = {number}\n";
        let (program, sources, mut checked) = checked(source);
        let target = *checked.selected_function_calls.values().next().unwrap();
        let span = Span::at(SourceId::new(0), 0);
        checked.selected_function_calls.insert(span, target);
        assert_eq!(PendingBodyContracts::capture(&checked, &program, &sources).map(PendingBodyContracts::into_contracts).unwrap_err(), BodyContractError::UnmappedFact(span));
    }

    #[test]
    fn pending_body_contract_sites_survive_annotation_shifts_and_frontend_disposal() {
        fn sites(source: &str) -> Vec<(usize, CanonicalBodyContractSite)> {
            let (program, sources, checked) = checked(source);
            let pending = PendingBodyContracts::capture(&checked, &program, &sources).unwrap();
            let declarations = pending.sites.keys().filter(|site| matches!(site,
                BodyContractSite::Parameter { .. } | BodyContractSite::Return { .. })).copied().collect::<Vec<_>>();
            drop(checked);
            drop(sources);
            drop(program);
            declarations.into_iter().map(|site| pending.canonical_site(site).unwrap().unwrap()).collect::<BTreeSet<_>>().into_iter().collect()
        }
        let original = sites("pure length(value: Str) -> Int { value.byte_len() }\n");
        let candidate = sites("type Unrelated = Int\npure length(value: Str) { value.byte_len() }\n");
        assert_eq!(original.len(), 2);
        assert_eq!(original, candidate);
    }

    #[test]
    fn pending_body_contract_sites_distinguish_missing_outside_and_ambiguous_boundaries() {
        let (program, sources, checked) = checked("pure number() -> Int { 1 }\n");
        let mut pending = PendingBodyContracts::capture(&checked, &program, &sources).unwrap();
        let span = pending.sites.keys().find_map(|site| match site {
            BodyContractSite::Expression(span) if pending.sites.contains_key(&BodyContractSite::Statement(*span)) => Some(*span),
            _ => None,
        }).unwrap();
        assert!(pending.canonical_site(BodyContractSite::ValueBoundary(span)).unwrap().is_some());
        let missing = BodyContractSite::Parameter { declaration: span, ordinal: usize::MAX };
        assert_eq!(pending.canonical_site(missing), Err(BodyContractError::UnmappedFact(span)));
        assert_eq!(pending.canonical_site(BodyContractSite::Expression(Span::at(SourceId::new(999), 0))), Ok(None));
        pending.value_boundaries.clear();
        assert_eq!(pending.canonical_site(BodyContractSite::ValueBoundary(span)), Err(BodyContractError::AmbiguousSite(span)));
    }

    #[test]
    fn pending_body_contract_sites_map_exported_constant_to_its_declaration() {
        use crate::syntax::arena::ArenaProgramBuilder;
        let root_source = "use shared\nlet value = shared.MAX_COUNT\n";
        let module_source = "##! Shared constants.\n## The largest supported count.\nexport const MAX_COUNT = 9223372036854775807\n";
        let mut sources = SourceMap::new();
        let root_id = sources.add_file("entry.xsh", root_source);
        let module_id = sources.add_file("shared.xsh", module_source);
        let mut builder = ArenaProgramBuilder::with_token_capacity(64);
        let root = Parser::parse_source_into_arena_builder(root_id, root_source, &mut builder);
        let module = Parser::parse_source_into_arena_builder(module_id, module_source, &mut builder);
        assert!(root.diagnostics.is_empty(), "{:?}", root.diagnostics);
        assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
        let namespace = builder.symbol_owner().intern("shared");
        builder.push_arena_module("/canonical/shared.xsh".to_owned(), namespace, module.statements);
        let import = builder.statement_ids(root.statements)[0];
        let (use_id, _, _) = builder.use_stmt_for_statement(import).unwrap();
        builder.set_use_resolved(use_id, Arc::from("/canonical/shared.xsh"));
        let program = builder.finish_with_statements(root.statements);
        let checked = Checker::check_arena_with_sources_and_options(&program, &sources, root_id,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let pending = PendingBodyContracts::capture(&checked, &program, &sources).unwrap();
        let module = &program.modules[0];
        let exported = program.module_statements(module).next().unwrap();
        let ArenaStmtKind::Export(declaration) = program.arena.stmt(exported).kind else { panic!("export wrapper") };
        let span = program.arena.stmt(declaration).span;
        assert_eq!(span.source_id, module_id);
        assert_eq!(program.arena.stmt(exported).span, span);
        assert!(pending.canonical_site(BodyContractSite::Statement(span)).unwrap().is_some());
    }

    #[test]
    fn pending_body_contract_sites_reject_competing_nonexport_paths_for_shared_span() {
        let source = "let value = 1\n";
        let (program, sources, checked) = checked(source);
        let mut pending = PendingBodyContracts::capture(&checked, &program, &sources).unwrap();
        let span = program.arena.stmt(program.statement_ids().next().unwrap()).span;
        let site = BodyContractSite::Statement(span);
        assert!(pending.canonical_site(site).unwrap().is_some());
        let locations = pending.sites.get_mut(&site).unwrap();
        let owner = locations.first().unwrap().0.clone();
        locations.insert((owner, CanonicalBodyContractSite(SitePath::Statement(vec![PathStep::Child(usize::MAX)]))));
        assert_eq!(pending.canonical_site(site), Err(BodyContractError::AmbiguousSite(span)));
    }

    #[test]
    fn generic_operators_contract_preserves_selected_domains_after_generalization() {
        for (parameters, result, expression) in [
            ("value: Int", "Bool", "value == value"),
            ("value: Int, flag: Bool", "Int", "{ let _ = flag and flag; value }"),
            ("value: Int, number: Int", "Int", "{ let _ = number < 7; value }"),
            ("value: Int", "Int", "{ let _ = value in [value]; value }"),
            ("value: Int, left: Set[Int], right: Set[Int]", "Int", "{ let _ = left | right; value }"),
            ("value: Int, number: Int", "Int", "{ let _ = 0 <= number < 7; value }"),
        ] {
            let body = if expression.starts_with('{') { expression.to_string() } else { format!("{{ {expression} }}") };
            let original = format!("pure keep({parameters}) -> {result} {body}\n");
            let parameters = parameters.replacen("value: Int", "value", 1);
            let candidate = format!("pure keep({parameters}) {body}\n");
            let (old_program, old_sources, old_checked) = checked(&original);
            let (new_program, new_sources, new_checked) = checked(&candidate);
            let old = old_checked.checked_declarations.functions.values().next().unwrap().callable_contract().unwrap();
            let new = new_checked.checked_declarations.functions.values().next().unwrap().callable_contract().unwrap();
            let instance = old.instance_evidence(&new).unwrap().unwrap();
            let old = PendingBodyContracts::capture(&old_checked, &old_program, &old_sources).unwrap().into_contracts().into_iter().find(|body| matches!(body.owner(), BodyContractOwner::Declaration(_))).unwrap();
            let new = PendingBodyContracts::capture(&new_checked, &new_program, &new_sources).unwrap().into_contracts().into_iter().find(|body| matches!(body.owner(), BodyContractOwner::Declaration(_))).unwrap();
            assert!(old.preserved_by_instance(&new, Some(&instance)), "{expression}");
            let mut operation = new.clone();
            let selection = operation.selections.values_mut().find_map(|selection| selection.binary.as_mut()).unwrap();
            selection.operator = if selection.operator == BinaryOp::Eq { BinaryOp::Ne } else { BinaryOp::Eq };
            assert!(!old.preserved_by_instance(&operation, Some(&instance)), "{expression}");
            let mut domain = new.clone();
            domain.selections.values_mut().find_map(|selection| selection.binary.as_mut()).unwrap().right = OperandType::Ground(Type::Path);
            assert!(!old.preserved_by_instance(&domain, Some(&instance)), "{expression}");
        }
    }

    #[test]
    fn generic_operators_contract_rejects_missing_selections_and_tracks_chain_pairs() {
        let source = "pure keep(value: Int, number: Int) -> Int { let _ = 0 <= number < 7; value }\n";
        let (program, sources, checked) = checked(source);
        let contracts = PendingBodyContracts::capture(&checked, &program, &sources).unwrap().into_contracts();
        let body = contracts.iter().find(|body| matches!(body.owner(), BodyContractOwner::Declaration(_))).unwrap();
        let pairs = body.selections.iter().filter(|(path, selection)| matches!(path.last(), Some(PathStep::ComparisonPair(_))) && selection.binary.is_some()).count();
        assert_eq!(pairs, 2);
        for expression in checked.binary_operators.keys() {
            let mut missing = checked.clone();
            missing.binary_operators.remove(expression);
            assert!(matches!(PendingBodyContracts::capture(&missing, &program, &sources), Err(BodyContractError::UnmappedFact(_))));
        }
        let mut shifted = checked.clone();
        let literal = program.arena.expr_tags.iter().enumerate().find_map(|(index, _)| {
            let expression = ExprId::from_index(index);
            matches!(program.arena.expr(expression).kind, ArenaExprKind::Int(_)).then_some(expression)
        }).unwrap();
        shifted.binary_operators.insert(literal, super::super::CheckedBinaryOperator { operator: BinaryOp::Eq });
        assert!(matches!(PendingBodyContracts::capture(&shifted, &program, &sources), Err(BodyContractError::UnmappedFact(_))));
    }

    #[test]
    fn checked_body_contract_empty_list_requires_checked_ast_witness() {
        let source = "let value = 1\nlet spare = 0\nlet found = value in []\n";
        let original = entry(source);
        let operation = original.selections.values().find_map(|selection| selection.binary.as_ref()).unwrap();
        assert!(matches!(&operation.right, OperandType::EmptyList(_)));
        assert!(original.preserved_by(&entry(source)));

        let mut specialized = original.clone();
        specialized.selections.values_mut().find_map(|selection| selection.binary.as_mut()).unwrap().right =
            OperandType::EmptyList(Box::new(OperandType::Ground(Type::List(Box::new(Type::Bytes)))));
        assert!(original.preserved_by(&specialized));
        assert!(!specialized.preserved_by(&original));

        let mut changed_domain = specialized.clone();
        changed_domain.selections.values_mut().find_map(|selection| selection.binary.as_mut()).unwrap().right =
            OperandType::EmptyList(Box::new(OperandType::Ground(Type::List(Box::new(Type::Int)))));
        assert!(!specialized.preserved_by(&changed_domain));

        let mut unqualified = specialized.clone();
        unqualified.selections.values_mut().find_map(|selection| selection.binary.as_mut()).unwrap().right =
            OperandType::Ground(Type::List(Box::new(Type::Bytes)));
        assert!(!original.preserved_by(&unqualified));

        let stored_source = "let value = 1\nlet values = []\nlet found = value in values\n";
        let (program, sources, stored) = checked(stored_source);
        let stored_operation_span = program.arena.source_rows().expr_ids(SourceId::new(0)).find_map(|expression|
            matches!(program.arena.expr(expression).kind, ArenaExprKind::Binary { op: BinaryOp::In, .. })
                .then_some(program.arena.expr(expression).span)).unwrap();
        assert!(matches!(PendingBodyContracts::capture(&stored, &program, &sources),
            Err(BodyContractError::UnsupportedSelection(span)) if span == stored_operation_span));

        let (program, sources, mut intentional_any) = checked(source);
        let empty_span = program.arena.source_rows().expr_ids(SourceId::new(0)).find(|&expression|
            matches!(program.arena.expr(expression).kind, ArenaExprKind::List(items) if items.is_empty()))
            .map(|expression| program.arena.expr(expression).span).unwrap();
        intentional_any.expr_types.insert(empty_span, Type::List(Box::new(Type::Any)));
        let any_list = PendingBodyContracts::capture(&intentional_any, &program, &sources).unwrap()
            .into_contracts().into_iter().find(|body| body.owner() == BodyContractOwner::Entry).unwrap();
        assert!(matches!(&any_list.selections.values().find_map(|selection| selection.binary.as_ref()).unwrap().right,
            OperandType::EmptyList(_)));
        assert!(!original.preserved_by(&any_list));

        let (program, sources, mut wrong_type) = checked(source);
        let empty = program.arena.source_rows().expr_ids(SourceId::new(0)).find(|&expression|
            matches!(program.arena.expr(expression).kind, ArenaExprKind::List(items) if items.is_empty())).unwrap();
        let empty_span = program.arena.expr(empty).span;
        wrong_type.expr_types.insert(empty_span, Type::Int);
        let wrong = PendingBodyContracts::capture(&wrong_type, &program, &sources).unwrap().into_contracts()
            .into_iter().find(|body| body.owner() == BodyContractOwner::Entry).unwrap();
        assert!(!original.preserved_by(&wrong));

        let (program, sources, mut nested_recovery) = checked(source);
        let empty_span = program.arena.source_rows().expr_ids(SourceId::new(0)).find(|&expression|
            matches!(program.arena.expr(expression).kind, ArenaExprKind::List(items) if items.is_empty()))
            .map(|expression| program.arena.expr(expression).span).unwrap();
        let binary_span = program.arena.source_rows().expr_ids(SourceId::new(0)).find_map(|expression|
            matches!(program.arena.expr(expression).kind, ArenaExprKind::Binary { op: BinaryOp::In, .. })
                .then_some(program.arena.expr(expression).span)).unwrap();
        nested_recovery.expr_types.insert(empty_span,
            Type::List(Box::new(Type::List(Box::new(Type::Invalid)))));
        assert!(matches!(PendingBodyContracts::capture(&nested_recovery, &program, &sources),
            Err(BodyContractError::UnsupportedSelection(span)) if span == binary_span));

        let (program, sources, mut missing_type) = checked(source);
        missing_type.expr_types.remove(&empty_span);
        assert!(matches!(PendingBodyContracts::capture(&missing_type, &program, &sources),
            Err(BodyContractError::UnsupportedSelection(span)) if span == binary_span));
    }

    #[test]
    fn checked_body_contract_divergence_requires_exiting_block_witness() {
        let source = "pure choose(result: Result[Int, Error]) -> Int { result ?? { |_| return 0 } }\n";
        let original = declaration(source);
        let operation = original.selections.values().find_map(|selection| selection.binary.as_ref()).unwrap();
        assert!(matches!(&operation.right, OperandType::Diverges));

        let completing = declaration("pure choose(result: Result[Int, Error]) -> Int { result ?? 0 }\n");
        assert!(!original.preserved_by(&completing));
        let mixed = declaration("pure choose(result: Result[Int, Error], flag: Bool) -> Int { result ?? { |_| if flag { return 0 } else { 1 } } }\n");
        let mixed_operation = mixed.selections.values().find_map(|selection| selection.binary.as_ref()).unwrap();
        assert!(!matches!(&mixed_operation.right, OperandType::Diverges));
        assert!(!original.preserved_by(&mixed));

        let conditional_exit_source = "pure choose(result: Result[Int, Error], flag: Bool) -> Int { result ?? { |_| if flag { return 0 } else { return 1 } } }\n";
        let conditional_exit = declaration(conditional_exit_source);
        let exit_operation = conditional_exit.selections.values().find_map(|selection| selection.binary.as_ref()).unwrap();
        assert!(matches!(&exit_operation.right, OperandType::Diverges));

        let (program, sources, checked) = checked(source);
        let (block_expression, block_span, binary_span) = program.arena.source_rows().expr_ids(SourceId::new(0)).find_map(|expression| {
            let ArenaExprKind::Binary { op: BinaryOp::ResultFallback, right, .. } = program.arena.expr(expression).kind else { return None; };
            let ArenaExprKind::ValueBlock(block) = program.arena.expr(right).kind else { return None; };
            Some((right, program.arena.span(program.arena.block(block).span), program.arena.expr(expression).span))
        }).unwrap();

        let mut wrong_result = checked.clone();
        wrong_result.expr_types.insert(program.arena.expr(block_expression).span, Type::Int);
        let candidate = PendingBodyContracts::capture(&wrong_result, &program, &sources).unwrap()
            .into_contracts().into_iter()
            .find(|body| matches!(body.owner(), BodyContractOwner::Declaration(_))).unwrap();
        let operation = candidate.selections.values().find_map(|selection| selection.binary.as_ref()).unwrap();
        assert!(!matches!(&operation.right, OperandType::Diverges));
        assert!(!original.preserved_by(&candidate));

        let mut recovery_result = checked.clone();
        recovery_result.expr_types.insert(program.arena.expr(block_expression).span, Type::Invalid);
        assert!(matches!(PendingBodyContracts::capture(&recovery_result, &program, &sources),
            Err(BodyContractError::UnsupportedSelection(span)) if span == binary_span));

        let mut inference_result = checked.clone();
        let mut constraints = crate::sema::constraints::TypeConstraints::default();
        inference_result.expr_types.insert(program.arena.expr(block_expression).span,
            constraints.fresh(program.arena.expr(block_expression).span));
        assert!(matches!(PendingBodyContracts::capture(&inference_result, &program, &sources),
            Err(BodyContractError::UnsupportedSelection(span)) if span == binary_span));

        let mut missing_exit = checked;
        assert!(missing_exit.definitely_exiting_block_spans.remove(&block_span));
        assert!(matches!(PendingBodyContracts::capture(&missing_exit, &program, &sources),
            Err(BodyContractError::UnsupportedSelection(span)) if span == binary_span));
    }

    #[test]
    fn generic_operators_contract_preserves_checked_constant_operations() {
        let source = "const total: Int = 1 + 2\nconst numbers: List[Int] = [1 + 2, 3 * 4]\ntype Pair = {first: Int, second: Int}\nconst fields: Pair = {first: 2 + 3, second: 4 * 5}\nconst one: UInt = 1KiB\nconst total_unsigned: UInt = one + 1KiB\npure keep(value: Int) -> Int { let _ = total; value }\n";
        let (program, sources, checked) = checked(source);
        let contracts = PendingBodyContracts::capture(&checked, &program, &sources).unwrap().into_contracts();
        let entry = contracts.iter().find(|body| body.owner() == BodyContractOwner::Entry).unwrap();
        let operation = entry.selections.values().find_map(|selection| selection.binary.as_ref()).unwrap();
        assert_eq!(operation.operator, BinaryOp::Add);
        assert_eq!(operation.left.ground(), Some(Type::Int));
        assert_eq!(operation.right.ground(), Some(Type::Int));
        assert_eq!(operation.result.ground(), Some(Type::Int));
        assert_eq!(entry.selections.values().filter(|selection| selection.binary.is_some()).count(), 6);
        let (expression, unsigned) = checked.prepared_constants.binary_operators.iter().find(|(_, operation)| operation.left_type == Type::UInt && operation.right_type == Type::UInt).unwrap();
        assert_eq!(unsigned.result_type, Type::Int);
        assert_eq!(checked.prepared_constants.types.get(expression), Some(&Type::UInt));
        for expression in checked.prepared_constants.binary_operators.keys() {
            let mut missing = checked.clone();
            missing.prepared_constants.binary_operators.remove(expression);
            assert!(matches!(PendingBodyContracts::capture(&missing, &program, &sources), Err(BodyContractError::UnmappedFact(_))));
        }
    }

}
