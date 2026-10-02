use std::collections::BTreeMap;
use crate::source::SourceId;
use crate::symbol::Name;
use crate::syntax::arena::{BindingTargetId, BlockId, ExprId, FunctionDefId, PatternId, StmtId, TypeDefId, ErrorDefId};
use crate::sema::inference::{CallableKind, EffectSummary, GraphOwner, InferenceContext, InferenceError, RequirementId, SchemeId, TypeId, ScopedRoot, ScopedEffectRoot, ScopedRequirementRoot, SolvedGraph};

#[cfg(test)]
#[path = "solved/argument_source_tests.rs"]
mod argument_source_tests;

#[path = "solved/pattern_plan.rs"]
mod pattern_plan;
#[path = "solved/refinement_plan.rs"]
mod refinement_plan;
pub use refinement_plan::{SolvedRefinedRead, SolvedRefinementAlias, SolvedRefinementWrite};
#[path = "solved/nominal_member_plan.rs"]
mod nominal_member_plan;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum NominalDeclaration {
    Type(TypeDefId),
    Error(ErrorDefId),
}

/// Nominal declarations are qualified by the retained graph owner, not by the
/// spelling of an import alias or a display name.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum QualifiedNominalIdentity {
    Source { source: SourceId, namespace: Option<Name>, declaration: NominalDeclaration, member: Option<Name> },
    Builtin { family: Name, member: Option<Name> },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NominalMemberKind { Tag, Error }

/// Checked declaration members retain their original field order independently
/// of constructor applications and of the patterns that select them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolvedNominalMember {
    pub kind: NominalMemberKind,
    pub family: Name,
    pub member: Name,
    pub tested: TypeId,
    pub fields: Vec<(Option<Name>, TypeId)>,
    pub facets: Vec<Name>,
    pub scope: Option<SchemeId>,
}

/// Arena identities retain the source and namespace that resolved the declaration.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ComprehensionIdentity {
    pub expression: ExpressionIdentity,
    pub qualifier: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct BindingIdentity {
    pub source: SourceId,
    pub namespace: Option<Name>,
    pub target: BindingTargetId,
}

/// A sequential `with` binder is named by its authored statement and ordinal;
/// it has no binding-target or pattern node in the syntax arena.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct WithBindingIdentity {
    pub statement: StatementIdentity,
    pub ordinal: u32,
}

/// A Guard failure parameter belongs to its authored statement; it has no
/// binding-target or pattern node and is visible only inside the handler.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct GuardErrorBindingIdentity {
    pub statement: StatementIdentity,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
pub struct PatternIdentity {
    pub source: SourceId,
    pub namespace: Option<Name>,
    pub pattern: PatternId,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
pub struct PatternCaptureIdentity {
    pub pattern: PatternIdentity,
    pub name: Name,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatternTypePosition { Input, Tested }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolvedPatternCapture {
    pub identity: PatternCaptureIdentity,
    pub ty: TypeId,
    pub branches: Vec<PatternCaptureIdentity>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SolvedPatternDecision {
    Structural,
    Alternation,
    Binding,
    Type,
    Result { success: bool, payload: Option<TypeId> },
    TagConstructor { type_name: Name, constructor: Name, identity: QualifiedNominalIdentity, fields: Vec<TypeId> },
    TagFields { fields: Vec<TypeId> },
    ErrorVariant { family: Name, variant: Name, identity: QualifiedNominalIdentity, fields: Vec<(Name, TypeId)> },
    Facet { facet: Name },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SolvedPatternShape {
    Wildcard,
    Binding,
    Literal { expression: ExpressionIdentity, value: Option<crate::sema::constants::LiteralConstant> },
    Group,
    Alias { name: Name },
    List { elements: u32, has_rest: bool },
    Record { fields: Vec<Name> },
    Alternation,
    Type,
    TestName,
    Constructor,
    ErrorVariant { fields: Vec<Name> },
    Facet,
    Tuple,
}

/// Captures are local definitions; structural parents retain only child
/// identities. Alternative joins retain each contributing definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolvedPattern {
    pub shape: SolvedPatternShape,
    pub input: TypeId,
    pub caller: Option<DeclarationIdentity>,
    pub tested: Option<TypeId>,
    pub decision: SolvedPatternDecision,
    pub children: Vec<PatternIdentity>,
    pub captures: Vec<SolvedPatternCapture>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ProducerPathComponent {
    RecordField(Name), ListItem, MapKey, MapValue, OptionalPayload,
    ResultSuccess, ResultError, CallableParameter(u32), CallableResult,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct ProducerPath(pub Vec<ProducerPathComponent>);

/// Pull and cleanup permissions belong to the retained producer handle. The
/// enclosing value's Stream item type does not establish either permission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProducerEffects {
    pub pull: EffectSummary,
    pub close: EffectSummary,
}

pub type ProducerProfile = BTreeMap<ProducerPath, ProducerEffects>;

fn retain_profile_effect_roots(profile: &ProducerProfile, scope: Option<SchemeId>, roots: &mut Vec<ScopedEffectRoot>) {
    for effects in profile.values() {
        roots.push(ScopedEffectRoot { effect: effects.pull, scope });
        roots.push(ScopedEffectRoot { effect: effects.close, scope });
    }
}

fn normalize_profile_effects(profile: &mut ProducerProfile, graph: &SolvedGraph) -> Result<(), InferenceError> {
    for effects in profile.values_mut() {
        effects.pull = graph.closed_effect_summary(effects.pull)?;
        effects.close = graph.closed_effect_summary(effects.close)?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct SolvedBinding {
    pub ty: TypeId,
    pub scheme: Option<SchemeId>,
    pub owner: Option<DeclarationIdentity>,
    pub mutable: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SolvedWithBinding {
    pub initializer: ExpressionIdentity,
    pub initializer_type: TypeId,
    pub binding_type: TypeId,
    pub owner: Option<DeclarationIdentity>,
}

#[derive(Clone, Debug)]
pub struct SolvedGuardErrorBinding {
    pub initializer: ExpressionIdentity,
    pub initializer_type: TypeId,
    pub binding_type: TypeId,
    pub owner: Option<DeclarationIdentity>,
    pub block: BlockId,
    pub name: Name,
}

/// A callable contract can remain precise without naming one declaration.
#[derive(Clone, Debug)]
pub struct SolvedExpressionCallable {
    pub signature: TypeId,
    pub scheme: Option<SchemeId>,
    pub declaration: Option<DeclarationIdentity>,
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
    pub source_requirements: Vec<RequirementId>,
    pub scheme: SchemeId,
    pub signature: TypeId,
    pub body: BlockId,
    pub kind: CallableKind,
    pub return_elaboration: ReturnElaboration,
    pub effective_effects: EffectSummary,
    pub required_effects: EffectSummary,
    pub parameter_producers: Vec<ProducerProfile>,
    pub return_producers: ProducerProfile,
    pub parameter_producer_flows: Vec<super::ProducerFlowId>,
    pub return_producer_flow: Option<super::ProducerFlowId>,
}

/// Static arguments name exact semantic slots. Unknown-length argument ranges
/// retain their potential slots and runtime guards without invented expansion.
/// Supplied entries retain their checked expansion in authored evaluation order.
/// Several fields from one finite spread share the original entry index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolvedArgumentSource {
    pub entry_index: usize,
    pub name: Option<Name>,
    pub value: crate::sema::arguments::ArgumentValueSource,
    pub span: crate::source::Span,
}

/// Defaults remain ordered independently of supplied source order.
#[derive(Clone, Debug)]
pub struct CallBinding {
    pub supplied_slots: Vec<usize>,
    pub default_slots: Vec<usize>,
    pub rest_slot: Option<usize>,
    pub dynamic: Option<crate::sema::inference::DynamicInvocationBinding>,
}

#[derive(Clone, Debug)]
pub struct SolvedCall {
    pub signature: TypeId,
    pub declaration: Option<DeclarationIdentity>,
    pub caller: Option<DeclarationIdentity>,
    pub requirements: Vec<RequirementId>,
    pub requirement_origins: Vec<(RequirementId, RequirementId)>,
    pub substitutions: Vec<TypeId>,
    pub effect_substitutions: Vec<EffectSummary>,
    pub actual_arguments: Vec<TypeId>,
    pub argument_producers: Vec<ProducerProfile>,
    pub result_producers: ProducerProfile,
    pub result_producer_flow: Option<super::ProducerFlowId>,
    pub binding: CallBinding,
}

/// The callable obligation owns its supplied argument modes and exact output
/// summary. A residual invocation does not invent a signature or binding plan.
#[derive(Clone, Debug)]
pub struct SolvedInvocation {
    pub requirement: RequirementId,
    pub caller: Option<DeclarationIdentity>,
}

fn validate_call_requirement_origins(graph: &InferenceContext, calls: &BTreeMap<ExpressionIdentity, SolvedCall>) -> Result<(), InferenceError> {
    for call in calls.values() {
        for &(source, instance) in &call.requirement_origins {
            if graph.requirement_source(instance)? != source { return Err(InferenceError::InvalidScheme); }
            graph.requirement_template(source)?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct StageIdentity {
    pub pipeline: ExpressionIdentity,
    pub index: u32,
}

#[derive(Clone, Debug)]
pub enum StageCallback {
    Block(BlockId),
    Callable { expression: ExprId, requirement: RequirementId, declaration: Option<DeclarationIdentity>, instance: Option<Box<crate::sema::inference::InstanceCertificate>> },
    /// Selection resolves this original callback through the stage operation.
    Protocol { expression: ExprId, operation: RequirementId, formal_slot: usize, declaration: Option<DeclarationIdentity> },
}

#[derive(Clone, Debug)]
pub struct SolvedStage {
    pub operation: SolvedOperation,
    pub callback: Option<StageCallback>,
    pub input_producer_flow: Option<super::ProducerFlowId>,
    pub result_producer_flow: Option<super::ProducerFlowId>,
}

#[derive(Clone, Debug)]
pub struct SolvedComprehensionClause {
    pub operation: SolvedOperation,
    pub input_producer_flow: super::ProducerFlowId,
    pub item_producer_flow: super::ProducerFlowId,
}

#[derive(Clone, Debug)]
pub struct SolvedOperation {
    pub requirement: RequirementId,
    pub result: TypeId,
    pub effects: EffectSummary,
    pub receiver: Option<TypeId>,
    pub actual_arguments: Vec<TypeId>,
    pub argument_coercions: Vec<(usize, RegistryArgumentCoercion)>,
    pub binding: CallBinding,
    pub caller: Option<DeclarationIdentity>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryArgumentCoercion {
    PathLikeToPath,
}

#[derive(Clone, Debug)]
pub struct SolvedProjection {
    pub receiver: TypeId,
    pub field: Name,
    pub result: TypeId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordUpdateValueSource {
    Expression(ExpressionIdentity),
    Binding { identity: BindingIdentity, version: u32 },
    Parameter { declaration: DeclarationIdentity, index: u32 },
}

#[derive(Clone, Debug)]
pub struct SolvedRecordUpdateReplacement {
    pub assignability: usize,
    pub path: Vec<Name>,
    pub projections: Vec<SolvedProjection>,
    pub value: TypeId,
    pub source: RecordUpdateValueSource,
    pub producer_flow: super::ProducerFlowId,
}

#[derive(Clone, Debug)]
pub struct SolvedRecordUpdate {
    pub base: ExpressionIdentity,
    pub receiver: TypeId,
    pub result: TypeId,
    pub replacements: Vec<SolvedRecordUpdateReplacement>,
    pub caller: Option<DeclarationIdentity>,
}

/// The graph and all of its consumer handles share one retained owner. Source
/// spans are projected diagnostics and never select a semantic fact.
#[derive(Debug)]
pub struct SolvedTypes<Graph = SolvedGraph> {
    pub owner: GraphOwner,
    symbols: crate::symbol::SymbolOwner,
    pub graph: Graph,
    pub producer_flows: super::ProducerFlowGraph,
    pub declarations: BTreeMap<DeclarationIdentity, SolvedCallable>,
    pub(crate) embedded_bridges: BTreeMap<DeclarationIdentity, std::sync::Arc<super::CanonicalNativeBridge>>,
    pub(in crate::sema::check) embedded_bridge_calls: BTreeMap<ExpressionIdentity, std::sync::Arc<super::NativeBridgeInvocation>>,
    pub nominals: BTreeMap<TypeId, QualifiedNominalIdentity>,
    pub nominal_members: BTreeMap<QualifiedNominalIdentity, std::sync::Arc<SolvedNominalMember>>,
    original_nominal_members: BTreeMap<QualifiedNominalIdentity, std::sync::Arc<SolvedNominalMember>>,
    pub expressions: BTreeMap<ExpressionIdentity, TypeId>,
    pub expression_schemes: BTreeMap<ExpressionIdentity, SchemeId>,
    pub non_completing_expressions: std::collections::BTreeSet<ExpressionIdentity>,
    /// Descendant instructions retain the initializer scheme that owns their
    /// generalized variables; this does not make each descendant a principal value.
    pub expression_value_scopes: BTreeMap<ExpressionIdentity, SchemeId>,
    pub expression_callables: BTreeMap<ExpressionIdentity, SolvedExpressionCallable>,
    pub expression_producers: BTreeMap<ExpressionIdentity, ProducerProfile>,
    pub expression_producer_flows: BTreeMap<ExpressionIdentity, super::ProducerFlowId>,
    pub statement_producer_flows: BTreeMap<StatementIdentity, super::ProducerFlowId>,
    pub statement_owners: BTreeMap<StatementIdentity, DeclarationIdentity>,
    pub bindings: BTreeMap<BindingIdentity, SolvedBinding>,
    pub with_bindings: BTreeMap<WithBindingIdentity, SolvedWithBinding>,
    pub guard_error_bindings: BTreeMap<GuardErrorBindingIdentity, SolvedGuardErrorBinding>,
    pub refined_reads: BTreeMap<ExpressionIdentity, std::sync::Arc<SolvedRefinedRead>>,
    original_refined_reads: BTreeMap<ExpressionIdentity, std::sync::Arc<SolvedRefinedRead>>,
    pub patterns: BTreeMap<PatternIdentity, std::sync::Arc<SolvedPattern>>,
    original_patterns: BTreeMap<PatternIdentity, (std::sync::Arc<SolvedPattern>, Option<SchemeId>)>,
    original_pattern_nominals: BTreeMap<PatternIdentity, (pattern_plan::PatternNominalReceipt, pattern_plan::PatternNominalReceipt)>,
    /// Original subject expression scopes inherited by the checked child
    /// topology; each entry is authenticated by its immutable pattern receipt.
    pub pattern_value_scopes: BTreeMap<PatternIdentity, SchemeId>,
    pub binding_producers: BTreeMap<BindingIdentity, ProducerProfile>,
    pub binding_producer_flows: BTreeMap<(BindingIdentity, u32), super::ProducerFlowId>,
    pub argument_sources: BTreeMap<ExpressionIdentity, Vec<SolvedArgumentSource>>,
    pub stage_argument_sources: BTreeMap<StageIdentity, Vec<SolvedArgumentSource>>,
    pub calls: BTreeMap<ExpressionIdentity, SolvedCall>,
    pub invocations: BTreeMap<ExpressionIdentity, SolvedInvocation>,
    pub operations: BTreeMap<ExpressionIdentity, SolvedOperation>,
    pub statement_operations: BTreeMap<StatementIdentity, SolvedOperation>,
    pub comprehension_operations: BTreeMap<ComprehensionIdentity, SolvedComprehensionClause>,
    pub stage_operations: BTreeMap<StageIdentity, SolvedStage>,
    pub(crate) operation_catalog: super::SolvedOperationCatalog,
    pub(crate) registry_boundaries: BTreeMap<ExpressionIdentity, super::registry_boundaries::SolvedRegistryBoundary>,
    pub(crate) registry_references: BTreeMap<ExpressionIdentity, super::registry_boundaries::SolvedRegistryReference>,
    pub(crate) run_operations: BTreeMap<super::run_operation::RunIdentity, super::run_operation::SolvedRun>,
    pub(crate) spawn_operations: BTreeMap<ExpressionIdentity, super::run_operation::SolvedSpawn>,
    pub projections: BTreeMap<ExpressionIdentity, SolvedProjection>,
    pub record_updates: BTreeMap<ExpressionIdentity, SolvedRecordUpdate>,
    pub module_projections: BTreeMap<ExpressionIdentity, super::SolvedModuleProjection>,
    pub schema_validations: BTreeMap<ExpressionIdentity, super::SolvedSchemaValidation>,
    pub constructor_applications: BTreeMap<ExpressionIdentity, super::SolvedConstructorApplication>,
    pub constructor_defaults: BTreeMap<super::ConstructorDefaultIdentity, super::SolvedConstructorDefault>,
    pub constructor_nominals: BTreeMap<QualifiedNominalIdentity, Vec<(Option<Name>, TypeId)>>,
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
    fn validate_argument_sources(&self) -> Result<(), InferenceError> {
        use crate::sema::arguments::ArgumentValueSource;
        for (identity, arguments, contiguous) in self.argument_sources.iter().map(|(identity, arguments)| (*identity, arguments, true))
            .chain(self.stage_argument_sources.iter().map(|(identity, arguments)| (identity.pipeline, arguments, false))) {
            if !self.expressions.contains_key(&identity) { return Err(InferenceError::InvalidScheme); }
            let mut previous: Option<&SolvedArgumentSource> = None;
            let mut fields = std::collections::BTreeSet::new();
            for argument in arguments {
                if argument.span.source_id != identity.source { return Err(InferenceError::InvalidScheme); }
                let expression = match argument.value {
                    ArgumentValueSource::Expression(expression) => expression,
                    ArgumentValueSource::PositionalSplice(expression) => {
                        if argument.name.is_some() { return Err(InferenceError::InvalidScheme); }
                        expression
                    }
                    ArgumentValueSource::RecordField { record, field } => {
                        if argument.name != Some(field) { return Err(InferenceError::InvalidScheme); }
                        record
                    }
                };
                let source = ExpressionIdentity { expression, ..identity };
                if !self.expressions.contains_key(&source) { return Err(InferenceError::InvalidScheme); }
                match previous {
                    None if contiguous && argument.entry_index != 0 => return Err(InferenceError::InvalidScheme),
                    Some(previous) if argument.entry_index == previous.entry_index => {
                        let (ArgumentValueSource::RecordField { record: before, .. }, ArgumentValueSource::RecordField { record: after, .. }) = (previous.value, argument.value) else { return Err(InferenceError::InvalidScheme); };
                        if before != after || previous.span != argument.span { return Err(InferenceError::InvalidScheme); }
                    }
                    Some(previous) => {
                        if previous.entry_index >= argument.entry_index || (contiguous && previous.entry_index.checked_add(1) != Some(argument.entry_index)) { return Err(InferenceError::InvalidScheme); }
                        fields.clear();
                    }
                    None => {}
                }
                if let ArgumentValueSource::RecordField { field, .. } = argument.value {
                    if !fields.insert(field) { return Err(InferenceError::InvalidScheme); }
                }
                previous = Some(argument);
            }
        }
        if self.stage_argument_sources.keys().any(|identity| !self.stage_operations.contains_key(identity)) { return Err(InferenceError::InvalidScheme); }
        Ok(())
    }

    fn validate_argument_bindings(&self, graph: &InferenceContext) -> Result<(), InferenceError> {
        use crate::sema::arguments::ArgumentValueSource;
        use crate::sema::inference::{InvocationArgumentKind, RequirementTemplate};
        for (identity, call) in &self.calls {
            let arguments = self.argument_sources.get(identity).ok_or(InferenceError::InvalidScheme)?;
            if arguments.len() != call.actual_arguments.len() { return Err(InferenceError::InvalidScheme); }
        }
        for (identity, invocation) in &self.invocations {
            let arguments = self.argument_sources.get(identity).ok_or(InferenceError::InvalidScheme)?;
            let RequirementTemplate::CallableInvocation { call } = graph.requirement_template(invocation.requirement)? else { return Err(InferenceError::InvalidScheme); };
            let call = graph.invocation_call(call)?;
            if arguments.len() != call.arguments.len() { return Err(InferenceError::InvalidScheme); }
            for (source, checked) in arguments.iter().zip(&call.arguments) {
                let kind = if matches!(source.value, ArgumentValueSource::PositionalSplice(_)) {
                    InvocationArgumentKind::PositionalSplice
                } else if let Some(name) = source.name {
                    InvocationArgumentKind::Named(name)
                } else {
                    InvocationArgumentKind::Positional
                };
                if checked.kind != kind { return Err(InferenceError::InvalidScheme); }
            }
        }
        Ok(())
    }

    fn with_graph(owner: GraphOwner, graph: Graph) -> Self {
        Self {
            owner, graph, symbols: crate::symbol::SymbolOwner::current().unwrap_or_default(),
            producer_flows: super::ProducerFlowGraph::new(owner),
            declarations: BTreeMap::new(), embedded_bridges: BTreeMap::new(), embedded_bridge_calls: BTreeMap::new(), nominals: BTreeMap::new(), nominal_members: BTreeMap::new(), original_nominal_members: BTreeMap::new(), expressions: BTreeMap::new(), expression_schemes: BTreeMap::new(), non_completing_expressions: std::collections::BTreeSet::new(), expression_value_scopes: BTreeMap::new(),
            expression_callables: BTreeMap::new(), expression_producers: BTreeMap::new(), bindings: BTreeMap::new(), with_bindings: BTreeMap::new(), guard_error_bindings: BTreeMap::new(), refined_reads: BTreeMap::new(), original_refined_reads: BTreeMap::new(), binding_producers: BTreeMap::new(), patterns: BTreeMap::new(), original_patterns: BTreeMap::new(), original_pattern_nominals: BTreeMap::new(), pattern_value_scopes: BTreeMap::new(),
            expression_producer_flows: BTreeMap::new(), statement_producer_flows: BTreeMap::new(), statement_owners: BTreeMap::new(), binding_producer_flows: BTreeMap::new(),
            argument_sources: BTreeMap::new(), stage_argument_sources: BTreeMap::new(), calls: BTreeMap::new(), invocations: BTreeMap::new(), operations: BTreeMap::new(), statement_operations: BTreeMap::new(), comprehension_operations: BTreeMap::new(), stage_operations: BTreeMap::new(), projections: BTreeMap::new(), record_updates: BTreeMap::new(),
            operation_catalog: super::SolvedOperationCatalog::new(owner), registry_boundaries: BTreeMap::new(), additions: BTreeMap::new(), statements: BTreeMap::new(),
            module_projections: BTreeMap::new(), schema_validations: BTreeMap::new(), constructor_applications: BTreeMap::new(), constructor_defaults: BTreeMap::new(), constructor_nominals: BTreeMap::new(), registry_references: BTreeMap::new(), run_operations: BTreeMap::new(), spawn_operations: BTreeMap::new(),
            expression_owners: BTreeMap::new(),
            result_wrappings: BTreeMap::new(), result_statement_wrappings: BTreeMap::new(),
        }
    }

    pub(crate) fn embedded_bridge(&self, identity: DeclarationIdentity) -> Option<&std::sync::Arc<super::CanonicalNativeBridge>> { self.embedded_bridges.get(&identity) }
    pub(crate) fn embedded_bridge_call(&self, identity: ExpressionIdentity) -> Option<&std::sync::Arc<super::NativeBridgeInvocation>> { self.embedded_bridge_calls.get(&identity) }

    pub fn symbol_owner(&self) -> &crate::symbol::SymbolOwner { &self.symbols }

    /// Owned source-fact payload and vector capacities exclude the shared type
    /// graph and symbol arena. Map entry bytes exclude allocator bookkeeping.
    pub fn retained_source_bytes(&self) -> usize {
        use std::mem::size_of;
        fn map_bytes<K, V>(map: &BTreeMap<K, V>) -> usize { map.len() * size_of::<(K, V)>() }
        fn profile_bytes(profile: &ProducerProfile) -> usize {
            map_bytes(profile) + profile.keys().map(|path| path.0.capacity() * size_of::<ProducerPathComponent>()).sum::<usize>()
        }
        fn binding_bytes(binding: &CallBinding) -> usize {
            let dynamic = binding.dynamic.as_ref().map(|dynamic| {
                dynamic.segments.capacity() * size_of::<crate::sema::inference::InvocationArgumentSegment>()
                    + (dynamic.conditional_default_slots.capacity() + dynamic.required_slots.capacity()) * size_of::<usize>()
                    + dynamic.segments.iter().map(|segment| match segment {
                        crate::sema::inference::InvocationArgumentSegment::DynamicRange { fixed_slots, .. } => fixed_slots.capacity() * size_of::<usize>(),
                        crate::sema::inference::InvocationArgumentSegment::StaticSlot { .. } => 0,
                    }).sum::<usize>()
            }).unwrap_or(0);
            (binding.supplied_slots.capacity() + binding.default_slots.capacity()) * size_of::<usize>() + dynamic
        }
        let maps = map_bytes(&self.embedded_bridges) + map_bytes(&self.embedded_bridge_calls) + map_bytes(&self.declarations) + map_bytes(&self.nominals) + map_bytes(&self.expressions)
            + map_bytes(&self.nominal_members) + map_bytes(&self.original_nominal_members)
            + map_bytes(&self.expression_schemes) + map_bytes(&self.expression_value_scopes) + map_bytes(&self.expression_callables)
            + map_bytes(&self.expression_producers) + map_bytes(&self.expression_producer_flows)
            + map_bytes(&self.statement_producer_flows) + map_bytes(&self.statement_owners)
            + map_bytes(&self.bindings) + map_bytes(&self.with_bindings) + map_bytes(&self.guard_error_bindings) + map_bytes(&self.refined_reads) + map_bytes(&self.original_refined_reads) + self.refined_read_payload_bytes() + map_bytes(&self.binding_producers) + map_bytes(&self.binding_producer_flows) + map_bytes(&self.patterns) + map_bytes(&self.original_patterns) + map_bytes(&self.original_pattern_nominals) + map_bytes(&self.pattern_value_scopes)
            + map_bytes(&self.argument_sources) + map_bytes(&self.stage_argument_sources) + map_bytes(&self.calls) + map_bytes(&self.invocations) + map_bytes(&self.operations) + map_bytes(&self.statement_operations) + map_bytes(&self.comprehension_operations) + map_bytes(&self.stage_operations) + map_bytes(&self.projections) + map_bytes(&self.record_updates) + map_bytes(&self.additions)
            + map_bytes(&self.statements) + map_bytes(&self.expression_owners) + map_bytes(&self.result_wrappings)
            + map_bytes(&self.result_statement_wrappings) + map_bytes(&self.registry_boundaries) + map_bytes(&self.module_projections) + map_bytes(&self.schema_validations) + map_bytes(&self.constructor_applications) + map_bytes(&self.constructor_defaults) + map_bytes(&self.constructor_nominals) + map_bytes(&self.registry_references) + map_bytes(&self.run_operations) + map_bytes(&self.spawn_operations);
        let declarations = self.declarations.values().map(|declaration| {
            declaration.source_requirements.capacity() * size_of::<RequirementId>()
                + declaration.parameter_producers.capacity() * size_of::<ProducerProfile>()
                + declaration.parameter_producer_flows.capacity() * size_of::<super::ProducerFlowId>()
                + declaration.parameter_producers.iter().chain(std::iter::once(&declaration.return_producers)).map(profile_bytes).sum::<usize>()
        }).sum::<usize>();
        let calls = self.calls.values().map(|call| {
            call.requirements.capacity() * size_of::<RequirementId>()
                + (call.substitutions.capacity() + call.actual_arguments.capacity()) * size_of::<TypeId>()
                + call.effect_substitutions.capacity() * size_of::<EffectSummary>()
                + call.requirement_origins.capacity() * size_of::<(RequirementId, RequirementId)>()
                + call.argument_producers.capacity() * size_of::<ProducerProfile>()
                + call.argument_producers.iter().chain(std::iter::once(&call.result_producers)).map(profile_bytes).sum::<usize>()
                + binding_bytes(&call.binding)
        }).sum::<usize>();
        let operations = self.operations.values().chain(self.statement_operations.values()).chain(self.comprehension_operations.values().map(|clause| &clause.operation)).chain(self.stage_operations.values().map(|stage| &stage.operation)).map(|operation| operation.actual_arguments.capacity() * size_of::<TypeId>() + operation.argument_coercions.capacity() * size_of::<(usize, RegistryArgumentCoercion)>() + binding_bytes(&operation.binding)).sum::<usize>();
        let mut descriptor_plans = std::collections::BTreeSet::new();
        let boundaries = self.registry_boundaries.values().map(|boundary| {
            boundary.retained_bytes() + boundary.shared_descriptor().filter(|plan| descriptor_plans.insert(std::sync::Arc::as_ptr(plan) as usize)).map_or(0, |plan| plan.retained_bytes())
        }).sum::<usize>();
        let updates = self.record_updates.values().map(|update| {
            update.replacements.capacity() * size_of::<SolvedRecordUpdateReplacement>()
                + update.replacements.iter().map(|replacement| replacement.path.capacity() * size_of::<Name>() + replacement.projections.capacity() * size_of::<SolvedProjection>()).sum::<usize>()
        }).sum::<usize>();
        let run_arguments = self.run_operations.values().map(|run| &run.arguments).chain(self.spawn_operations.values().map(|spawn| &spawn.arguments)).map(|arguments| arguments.capacity() * size_of::<super::run_operation::RunArgumentGuard>()).sum::<usize>();
        maps + declarations + calls + operations + boundaries + updates + run_arguments + self.stage_callback_certificate_bytes() + self.pattern_payload_bytes()
            + self.nominal_member_payload_bytes()
            + self.argument_sources.values().chain(self.stage_argument_sources.values()).map(|arguments| arguments.capacity() * size_of::<SolvedArgumentSource>()).sum::<usize>() + self.registry_references.values().map(|reference| reference.retained_bytes()).sum::<usize>() + self.schema_validation_payload_bytes() + self.constructor_application_payload_bytes() + self.producer_flows.retained_bytes() + self.operation_catalog.retained_bytes()
            + self.non_completing_expressions.len() * (size_of::<ExpressionIdentity>() + 3 * size_of::<usize>())
            + self.expression_producers.values().chain(self.binding_producers.values()).map(profile_bytes).sum::<usize>()
            + self.embedded_bridges.values().map(|bridge| bridge.retained_bytes()).sum::<usize>()
            + self.embedded_bridge_calls.values().map(|call| call.retained_bytes()).sum::<usize>()
    }

    fn source_operations(&self) -> impl Iterator<Item = (super::ProducerFlowSource, &SolvedOperation)> {
        self.operations.iter().map(|(identity, operation)| (super::ProducerFlowSource::Expression(*identity), operation))
            .chain(self.statement_operations.iter().map(|(identity, operation)| (super::ProducerFlowSource::Statement(*identity), operation)))
            .chain(self.comprehension_operations.iter().map(|(identity, clause)| (super::ProducerFlowSource::Comprehension(*identity), &clause.operation)))
            .chain(self.stage_operations.iter().map(|(identity, stage)| (super::ProducerFlowSource::Stage(*identity), &stage.operation)))
            .chain(self.run_operations.values().map(|run| (run.parent, &run.operation)))
    }

    pub(crate) fn expression_scope(&self, identity: ExpressionIdentity, caller: Option<DeclarationIdentity>) -> Result<Option<SchemeId>, InferenceError> {
        let lexical = caller.map(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
        Ok(self.expression_schemes.get(&identity).copied().or_else(|| self.expression_value_scopes.get(&identity).copied()).or(lexical))
    }

    fn record_update_value_scope(&self, source: RecordUpdateValueSource) -> Result<Option<SchemeId>, InferenceError> {
        match source {
            RecordUpdateValueSource::Expression(identity) => {
                if !self.expressions.contains_key(&identity) { return Err(InferenceError::InvalidScheme); }
                self.expression_scope(identity, self.expression_owners.get(&identity).copied())
            }
            RecordUpdateValueSource::Binding { identity, version } => {
                let binding = self.bindings.get(&identity).ok_or(InferenceError::InvalidScheme)?;
                if !self.binding_producer_flows.contains_key(&(identity, version)) { return Err(InferenceError::InvalidScheme); }
                let lexical = binding.owner.map(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
                Ok(binding.scheme.or(lexical))
            }
            RecordUpdateValueSource::Parameter { declaration, index } => {
                let declaration = self.declarations.get(&declaration).ok_or(InferenceError::InvalidScheme)?;
                if index as usize >= declaration.parameter_producer_flows.len() { return Err(InferenceError::InvalidScheme); }
                Ok(Some(declaration.scheme))
            }
        }
    }

    fn validate_record_updates(&self, graph: &InferenceContext) -> Result<(), InferenceError> {
        for (identity, update) in &self.record_updates {
            let actual = *self.expressions.get(identity).ok_or(InferenceError::InvalidScheme)?;
            let base = *self.expressions.get(&update.base).ok_or(InferenceError::InvalidScheme)?;
            if graph.resolved(actual)? != graph.resolved(update.result)? || graph.resolved(base)? != graph.resolved(update.receiver)? || graph.resolved(update.result)? != graph.resolved(update.receiver)? { return Err(InferenceError::InvalidScheme); }
            for replacement in &update.replacements {
                self.record_update_value_scope(replacement.source)?;
                if replacement.path.is_empty() || replacement.path.len() != replacement.projections.len() { return Err(InferenceError::InvalidScheme); }
                let mut receiver = update.receiver;
                for (&field, projection) in replacement.path.iter().zip(&replacement.projections) {
                    if projection.field != field || graph.resolved(receiver)? != graph.resolved(projection.receiver)? { return Err(InferenceError::InvalidScheme); }
                    let (target, _) = record_update_field(graph, receiver, field)?;
                    if graph.resolved(target)? != graph.resolved(projection.result)? { return Err(InferenceError::InvalidScheme); }
                    receiver = projection.result;
                }
                graph.node(graph.resolved(replacement.value)?)?;
                let origin = graph.constraint_origins().get(replacement.assignability).ok_or(InferenceError::InvalidScheme)?;
                let crate::sema::inference::ConstraintRelation::Assignable { expected, actual } = origin.relation else { return Err(InferenceError::InvalidScheme); };
                if graph.resolved(expected)? != graph.resolved(receiver)? || graph.resolved(actual)? != graph.resolved(replacement.value)? { return Err(InferenceError::InvalidScheme); }
                let expected_flow = match replacement.source {
                    RecordUpdateValueSource::Expression(identity) => self.expression_producer_flows.get(&identity).copied(),
                    RecordUpdateValueSource::Binding { identity, version } => self.binding_producer_flows.get(&(identity, version)).copied(),
                    RecordUpdateValueSource::Parameter { declaration, index } => self.declarations.get(&declaration).and_then(|declaration| declaration.parameter_producer_flows.get(index as usize)).copied(),
                };
                if expected_flow != Some(replacement.producer_flow) { return Err(InferenceError::InvalidScheme); }
                self.producer_flows.node(replacement.producer_flow)?;
            }
        }
        Ok(())
    }

    fn validate_run_operations(&self, graph: &InferenceContext) -> Result<(), InferenceError> {
        use crate::sema::inference::{EffectSet, EffectSummary, RequirementTemplate};
        for (identity, run) in &self.run_operations {
            let parent = match run.parent { super::ProducerFlowSource::Expression(parent) => (parent.source, parent.namespace), super::ProducerFlowSource::Statement(parent) => (parent.source, parent.namespace), _ => return Err(InferenceError::InvalidScheme) };
            if parent != (identity.source, identity.namespace) { return Err(InferenceError::InvalidScheme); }
            if run.usage == super::run_operation::RunUse::Command && !matches!(run.parent, super::ProducerFlowSource::Statement(_)) { return Err(InferenceError::InvalidScheme); }
            let RequirementTemplate::Operation { call, .. } = graph.requirement_template(run.operation.requirement)? else { return Err(InferenceError::InvalidScheme); };
            let evidence = graph.candidate_evidence(run.operation.requirement)?.ok_or(InferenceError::InvalidScheme)?;
            let super::SolvedOperationAuthority::Language(metadata) = self.operation_catalog.candidate(graph, evidence.candidate)? else { return Err(InferenceError::InvalidScheme); };
            if metadata.operation != (crate::sema::operation_graph::PreparedLanguageOperation::Run { kind: run.kind, policy: run.policy, propagate: run.propagate })
                || graph.resolved(graph.operation_call(call)?.result)? != graph.resolved(run.operation.result)? { return Err(InferenceError::InvalidScheme); }
            self.operation_scope(run.parent, &run.operation)?;
            self.validate_run_arguments(graph, &run.arguments, identity.source, identity.namespace)?;
            let node = self.producer_flows.node(run.producer_flow)?;
            if node.source != run.parent { return Err(InferenceError::InvalidScheme); }
            if matches!(run.kind, crate::syntax::node::RunKind::StreamText | crate::syntax::node::RunKind::StreamBytes) {
                let super::ProducerFlowKind::Known(profile) = &node.kind else { return Err(InferenceError::InvalidScheme); };
                let path = if run.propagate { ProducerPath::default() } else { ProducerPath(vec![ProducerPathComponent::ResultSuccess]) };
                let expected = ProducerEffects { pull: EffectSummary::Closed(if run.policy { EffectSet(EffectSet::PROCESS.0 | EffectSet::ERROR.0) } else { EffectSet::EMPTY }), close: EffectSummary::Closed(if run.policy { EffectSet::PROCESS } else { EffectSet::EMPTY }) };
                if profile.len() != 1 || profile.get(&path) != Some(&expected) { return Err(InferenceError::InvalidScheme); }
            } else if !matches!(node.kind, super::ProducerFlowKind::Empty) { return Err(InferenceError::InvalidScheme); }
        }
        Ok(())
    }

    fn validate_spawn_operations(&self, graph: &InferenceContext) -> Result<(), InferenceError> {
        for (identity, spawn) in &self.spawn_operations {
            let operation = self.operations.get(identity).ok_or(InferenceError::InvalidScheme)?;
            if operation.requirement != spawn.requirement || operation.effects != crate::sema::inference::EffectSummary::Closed(crate::sema::inference::EffectSet::PROCESS) { return Err(InferenceError::InvalidScheme); }
            let command = match spawn.target {
                super::run_operation::SpawnTarget::Run(run) => {
                    if run.source != identity.source || run.namespace != identity.namespace || !operation.actual_arguments.is_empty() { return Err(InferenceError::InvalidScheme); }
                    false
                }
                super::run_operation::SpawnTarget::Command(target) => {
                    let actual = self.expressions.get(&target).ok_or(InferenceError::InvalidScheme)?;
                    if target.source != identity.source || target.namespace != identity.namespace || operation.actual_arguments.len() != 1 || graph.resolved(operation.actual_arguments[0])? != graph.resolved(*actual)? { return Err(InferenceError::InvalidScheme); }
                    self.expression_scope(target, self.expression_owners.get(&target).copied())?;
                    true
                }
            };
            let evidence = graph.candidate_evidence(spawn.requirement)?.ok_or(InferenceError::InvalidScheme)?;
            let super::SolvedOperationAuthority::Language(metadata) = self.operation_catalog.candidate(graph, evidence.candidate)? else { return Err(InferenceError::InvalidScheme); };
            if metadata.operation != (crate::sema::operation_graph::PreparedLanguageOperation::Spawn { command }) { return Err(InferenceError::InvalidScheme); }
            self.validate_run_arguments(graph, &spawn.arguments, identity.source, identity.namespace)?;
        }
        Ok(())
    }

    fn validate_run_arguments(&self, graph: &InferenceContext, arguments: &[super::run_operation::RunArgumentGuard], source: SourceId, namespace: Option<Name>) -> Result<(), InferenceError> {
        use super::run_operation::{RunArgumentMode, RunArgumentSource};
        for argument in arguments {
            match argument.source {
                RunArgumentSource::Expression(identity) => {
                    let actual = self.expressions.get(&identity).ok_or(InferenceError::InvalidScheme)?;
                    if (identity.source, identity.namespace) != (source, namespace) || graph.resolved(*actual)? != graph.resolved(argument.actual)? { return Err(InferenceError::InvalidScheme); }
                }
                RunArgumentSource::NamedSplice { span, .. } => {
                    let reason = graph.reason_data(graph.requirement_reason(argument.requirement)?)?;
                    if span.source_id != source || span != reason.span || argument.mode != RunArgumentMode::Splice { return Err(InferenceError::InvalidScheme); }
                }
            }
            let expected = match argument.mode {
                RunArgumentMode::Single | RunArgumentMode::Environment | RunArgumentMode::Splice => crate::sema::inference::Eligibility::ArgvItem,
                RunArgumentMode::Expansion => crate::sema::inference::Eligibility::ArgvExpansion,
                RunArgumentMode::Display => crate::sema::inference::Eligibility::Display,
            };
            let crate::sema::inference::RequirementTemplate::Eligibility { predicate, ty } = graph.requirement_template(argument.requirement)? else { return Err(InferenceError::InvalidScheme); };
            if predicate != expected || graph.resolved(ty)? != graph.resolved(argument.operand)? { return Err(InferenceError::InvalidScheme); }
            if argument.mode == RunArgumentMode::Splice {
                let crate::sema::inference::TypeNode::List(item) = graph.node(graph.resolved(argument.actual)?)? else { return Err(InferenceError::InvalidScheme); };
                if graph.resolved(*item)? != graph.resolved(argument.operand)? { return Err(InferenceError::InvalidScheme); }
            } else if graph.resolved(argument.actual)? != graph.resolved(argument.operand)? { return Err(InferenceError::InvalidScheme); }
        }
        Ok(())
    }

    fn run_argument_roots(&self) -> Result<Vec<(super::run_operation::RunArgumentGuard, Option<SchemeId>)>, InferenceError> {
        let mut roots = Vec::new();
        for run in self.run_operations.values() {
            let lexical = self.operation_scope(run.parent, &run.operation)?;
            for &argument in &run.arguments {
                let scope = match argument.source { super::run_operation::RunArgumentSource::Expression(identity) => self.expression_scope(identity, run.operation.caller)?, _ => lexical };
                roots.push((argument, scope));
            }
        }
        for (identity, spawn) in &self.spawn_operations {
            let operation = self.operations.get(identity).ok_or(InferenceError::InvalidScheme)?;
            let lexical = self.expression_scope(*identity, operation.caller)?;
            for &argument in &spawn.arguments {
                let scope = match argument.source { super::run_operation::RunArgumentSource::Expression(identity) => self.expression_scope(identity, operation.caller)?, _ => lexical };
                roots.push((argument, scope));
            }
        }
        Ok(roots)
    }

    fn registry_reference_roots(&self, graph: &InferenceContext) -> Result<Vec<crate::sema::inference::ScopedInstanceRoot>, InferenceError> {
        use super::registry_boundaries::RegistryReferenceContract;
        use crate::sema::inference::{CallableAuthority, NativeAuthority, TypeNode};
        let mut instances = Vec::with_capacity(self.registry_references.len());
        for (identity, reference) in &self.registry_references {
            if self.expression_owners.get(identity).copied() != reference.caller
                || self.expressions.get(identity).copied() != Some(reference.value_type())
                || self.expression_callables.get(identity).map(|callable| callable.signature) != Some(reference.value_type()) { return Err(InferenceError::InvalidScheme); }
            if let RegistryReferenceContract::Native { callable, authority } = reference.contract {
                    let TypeNode::NativeCallable(wrapper) = graph.node(callable)? else { return Err(InferenceError::InvalidScheme); };
                    if wrapper.signature != graph.native_authority_signature(authority)? || wrapper.alternatives.as_slice() != [CallableAuthority::Native { authority }] { return Err(InferenceError::InvalidScheme); }
                    let flow = self.expression_producer_flows.get(identity).ok_or(InferenceError::InvalidScheme)?;
                    if !matches!(self.producer_flows.node(*flow)?.kind, super::ProducerFlowKind::NativeCallable { authority: found } if found == authority) { return Err(InferenceError::InvalidScheme); }
            }
            let candidates = reference.candidates(graph)?;
            let certificates = reference.certificates(graph)?;
            if candidates.len() != certificates.len() || candidates.is_empty() { return Err(InferenceError::InvalidScheme); }
            for (candidate, certificate) in candidates.into_iter().zip(certificates) {
                let super::SolvedOperationAuthority::Registry(metadata) = self.operation_catalog.candidate(graph, candidate)? else { return Err(InferenceError::InvalidScheme); };
                let template = graph.candidate(candidate)?;
                if metadata.scheme != certificate.scheme || template.scheme != certificate.scheme || template.has_receiver { return Err(InferenceError::InvalidScheme); }
                match &reference.contract {
                    RegistryReferenceContract::Arrow(arrow) => {
                    if metadata.reference_protocol(template) != crate::sema::registry_graph::RegistryReferenceProtocol::Arrow { return Err(InferenceError::InvalidScheme); }
                        if arrow.instantiated_requirements.len() != certificate.requirement_origins.len()
                            || arrow.instantiated_requirements.iter().copied().ne(certificate.requirement_origins.iter().map(|pair| pair.1)) { return Err(InferenceError::InvalidScheme); }
                    }
                    RegistryReferenceContract::Native { authority: NativeAuthority::Single(contract), .. } => {
                        if metadata.reference_protocol(template) != crate::sema::registry_graph::RegistryReferenceProtocol::Native
                            || graph.family(graph.native_contract(*contract)?.family)? != [candidate] { return Err(InferenceError::InvalidScheme); }
                    }
                    RegistryReferenceContract::Native { authority: NativeAuthority::Family(family), .. } => {
                        if !metadata.reference_family_member_supported(template) { return Err(InferenceError::InvalidScheme); }
                        let member = graph.native_family_member(*family, candidate)?;
                        if graph.native_contract(member)?.family != graph.native_family_contract(*family)?.family { return Err(InferenceError::InvalidScheme); }
                    }
                }
            let declaration = graph.scheme(certificate.scheme)?;
            if declaration.requirement_origins.len() != certificate.requirement_origins.len()
                { return Err(InferenceError::InvalidScheme); }
            for (&origin, &(source, instance)) in declaration.requirement_origins.iter().zip(&certificate.requirement_origins) {
                if source != origin || graph.requirement_origin(source)? != graph.requirement_origin(instance)? { return Err(InferenceError::InvalidScheme); }
            }
            instances.push(crate::sema::inference::ScopedInstanceRoot { certificate, scope: self.expression_scope(*identity, reference.caller)? });
            }
        }
        instances.extend(self.stage_callback_instance_roots(graph)?);
        Ok(instances)
    }

    fn validate_registry_boundaries(&self, graph: &InferenceContext) -> Result<(), InferenceError> {
        self.validate_json_literal_boundaries(graph)?;
        for (identity, boundary) in &self.registry_boundaries {
            let expression = self.expressions.get(identity).ok_or(InferenceError::InvalidScheme)?;
            if graph.resolved(*expression)? != graph.resolved(boundary.result)? { return Err(InferenceError::InvalidScheme); }
            if let Some(requirement) = boundary.requirement {
                let crate::sema::inference::RequirementTemplate::Operation { call, .. } = graph.requirement_template(requirement)? else { return Err(InferenceError::InvalidScheme); };
                if graph.resolved(graph.operation_call(call)?.result)? != graph.resolved(boundary.input)? { return Err(InferenceError::InvalidScheme); }
            }
            if let super::registry_boundaries::RegistryBoundaryKind::SchemaValidation { schema, .. } = &boundary.kind {
                let retained = self.operation_catalog.schema(graph, schema.authority_id)?;
                if retained.label != schema.label || graph.resolved(retained.shape)? != graph.resolved(schema.shape)? { return Err(InferenceError::InvalidScheme); }
            }
            self.validate_json_arguments(*identity, boundary, graph)?;
            if let super::registry_boundaries::RegistryBoundaryKind::CommandArguments { argv_children } = &boundary.kind {
                let requirement = boundary.requirement.ok_or(InferenceError::InvalidScheme)?;
                let crate::sema::inference::RequirementTemplate::Operation { family, call } = graph.requirement_template(requirement)? else { return Err(InferenceError::InvalidScheme); };
                if let Some(operation) = self.operations.get(identity) {
                    if operation.requirement != requirement || operation.caller != boundary.caller { return Err(InferenceError::InvalidScheme); }
                } else {
                    let invocation = self.invocations.get(identity).ok_or(InferenceError::InvalidScheme)?;
                    if invocation.caller != boundary.caller { return Err(InferenceError::InvalidScheme); }
                    let alternative = graph.native_invocation_children(invocation.requirement)?.iter().find(|alternative| alternative.operation == requirement).ok_or(InferenceError::InvalidScheme)?;
                    if graph.operation_call(call)?.mono_authority != Some(alternative.authority) { return Err(InferenceError::InvalidScheme); }
                    let crate::sema::inference::RequirementTemplate::CallableInvocation { call: original } = graph.requirement_template(invocation.requirement)? else { return Err(InferenceError::InvalidScheme); };
                    let child = graph.operation_call(call)?;
                    match child.binding {
                        crate::sema::inference::OperationBinding::Invocation(source) if source == original => {},
                        crate::sema::inference::OperationBinding::Slots => {
                            let original = graph.invocation_call(original)?;
                            let signature = graph.native_authority_signature(alternative.authority)?;
                            let crate::sema::inference::TypeNode::Arrow(arrow) = graph.node(graph.resolved(signature)?)? else { return Err(InferenceError::InvalidScheme); };
                            if arrow.params.len() != child.arguments.len() || arrow.params.iter().any(|parameter| parameter.rest) { return Err(InferenceError::InvalidScheme); }
                            // The original static binding is valid before a
                            // native overload has enough information to select.
                            let mut supplied = vec![None; child.arguments.len()];
                            let mut cursor = 0;
                            for argument in &original.arguments {
                                let slot = match argument.kind {
                                    crate::sema::inference::InvocationArgumentKind::Positional => {
                                        while supplied.get(cursor).is_some_and(Option::is_some) { cursor += 1; }
                                        let slot = cursor; cursor += 1; slot
                                    },
                                    crate::sema::inference::InvocationArgumentKind::Named(label) => {
                                        let mut matches = arrow.params.iter().enumerate().filter(|(_, parameter)| parameter.label == label);
                                        let slot = matches.next().ok_or(InferenceError::InvalidScheme)?.0;
                                        if matches.next().is_some() { return Err(InferenceError::InvalidScheme); }
                                        slot
                                    },
                                    crate::sema::inference::InvocationArgumentKind::PositionalSplice => return Err(InferenceError::InvalidScheme),
                                };
                                let entry = supplied.get_mut(slot).ok_or(InferenceError::InvalidScheme)?;
                                if entry.replace(argument.ty).is_some() { return Err(InferenceError::InvalidScheme); }
                            }
                            for ((original, retained), parameter) in supplied.into_iter().zip(&child.arguments).zip(&arrow.params) {
                                match (original, retained) {
                                    (None, None) if parameter.defaulted => {},
                                    (Some(original), Some(retained)) if graph.resolved(original)? == graph.resolved(*retained)? => {},
                                    _ => return Err(InferenceError::InvalidScheme),
                                }
                            }
                        },
                        _ => return Err(InferenceError::InvalidScheme),
                    }
                }
                for &candidate in graph.family(family)? {
                    let super::SolvedOperationAuthority::Registry(authority) = self.operation_catalog.candidate(graph, candidate)? else { return Err(InferenceError::InvalidScheme); };
                    if authority.operation != crate::modules::RuntimeOp::ProcessCommandArgv { return Err(InferenceError::InvalidScheme); }
                    let relations = &graph.candidate(candidate)?.argument_relations;
                    if !matches!(relations.first(), Some(crate::sema::inference::ArgumentRelation::CommandTarget { .. }))
                        || !matches!(relations.get(1), Some(crate::sema::inference::ArgumentRelation::CommandArgv { .. })) { return Err(InferenceError::InvalidScheme); }
                }
                let mut sources = std::collections::BTreeSet::new();
                for child in argv_children {
                    if !sources.insert(child.source) { return Err(InferenceError::InvalidScheme); }
                    let original = self.expressions.get(&child.source).ok_or(InferenceError::InvalidScheme)?;
                    let crate::sema::inference::RequirementTemplate::Eligibility { predicate, ty } = graph.requirement_template(child.requirement)? else { return Err(InferenceError::InvalidScheme); };
                    let expected = if child.splice { crate::sema::inference::Eligibility::CommandArgv } else { crate::sema::inference::Eligibility::CommandTarget };
                    if predicate != expected || graph.resolved(ty)? != graph.resolved(child.actual)? || graph.resolved(*original)? != graph.resolved(child.actual)? { return Err(InferenceError::InvalidScheme); }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn operation_scope(&self, source: super::ProducerFlowSource, operation: &SolvedOperation) -> Result<Option<SchemeId>, InferenceError> {
        let lexical = operation.caller.map(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
        match source {
            super::ProducerFlowSource::Expression(identity) => self.expression_scope(identity, operation.caller),
            super::ProducerFlowSource::Stage(identity) => self.expression_scope(identity.pipeline, operation.caller),
            super::ProducerFlowSource::Comprehension(identity) => {
                if !self.comprehension_operations.contains_key(&identity) || !self.expressions.contains_key(&identity.expression) { return Err(InferenceError::InvalidScheme); }
                self.expression_scope(identity.expression, operation.caller)
            }
            super::ProducerFlowSource::Statement(identity) => {
                if !self.statements.contains_key(&identity) { return Err(InferenceError::InvalidScheme); }
                Ok(lexical)
            }
            _ => Err(InferenceError::InvalidScheme),
        }
    }

    pub(super) fn producer_flow_scope(&self, source: super::ProducerFlowSource) -> Result<Option<SchemeId>, InferenceError> {
        match source {
            super::ProducerFlowSource::Expression(identity) => {
                if !self.expressions.contains_key(&identity) { return Err(InferenceError::InvalidScheme); }
                let lexical_scope = self.expression_owners.get(&identity).map(|owner| self.declarations.get(owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
                Ok(self.expression_schemes.get(&identity).copied().or_else(|| self.expression_value_scopes.get(&identity).copied()).or(lexical_scope))
            }
            super::ProducerFlowSource::Stage(identity) => {
                let stage = self.stage_operations.get(&identity).ok_or(InferenceError::InvalidScheme)?;
                let lexical = stage.operation.caller.map(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
                Ok(self.expression_schemes.get(&identity.pipeline).copied().or_else(|| self.expression_value_scopes.get(&identity.pipeline).copied()).or(lexical))
            }
            super::ProducerFlowSource::Comprehension(identity) => {
                let clause = self.comprehension_operations.get(&identity).ok_or(InferenceError::InvalidScheme)?;
                if !self.expressions.contains_key(&identity.expression) { return Err(InferenceError::InvalidScheme); }
                self.expression_scope(identity.expression, clause.operation.caller)
            }
            super::ProducerFlowSource::Statement(identity) => {
                if !self.statements.contains_key(&identity) { return Err(InferenceError::InvalidScheme); }
                self.statement_owners.get(&identity).map(|owner| self.declarations.get(owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()
            }
            super::ProducerFlowSource::Binding { identity, .. } => {
                let binding = self.bindings.get(&identity).ok_or(InferenceError::InvalidScheme)?;
                let lexical_scope = binding.owner.map(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
                Ok(binding.scheme.or(lexical_scope))
            }
            super::ProducerFlowSource::Parameter { declaration, index } => {
                let declaration = self.declarations.get(&declaration).ok_or(InferenceError::InvalidScheme)?;
                if index as usize >= declaration.parameter_producers.len() { return Err(InferenceError::InvalidScheme); }
                Ok(Some(declaration.scheme))
            }
            super::ProducerFlowSource::DeclarationResult(declaration) => Ok(Some(self.declarations.get(&declaration).ok_or(InferenceError::InvalidScheme)?.scheme)),
        }
    }

    fn validate_native_producer_flows(&self, graph: &InferenceContext) -> Result<(), InferenceError> {
        for node in self.producer_flows.nodes() {
            let super::ProducerFlowKind::NativeCallable { authority } = node.kind else { continue; };
            let super::ProducerFlowSource::Expression(identity) = node.source else { return Err(InferenceError::InvalidScheme); };
            let reference = self.registry_references.get(&identity).ok_or(InferenceError::InvalidScheme)?;
            if reference.native_authority() != Some(authority) { return Err(InferenceError::InvalidScheme); }
            let wrapper = reference.value_type();
            let crate::sema::inference::TypeNode::NativeCallable(callable) = graph.node(graph.resolved(wrapper)?)? else { return Err(InferenceError::InvalidScheme); };
            if !callable.alternatives.contains(&crate::sema::inference::CallableAuthority::Native { authority }) { return Err(InferenceError::InvalidScheme); }
            graph.native_authority_signature(authority)?;
            self.expression_scope(identity, reference.caller)?;
        }
        Ok(())
    }

    fn validate_producer_flow_roots(&self) -> Result<(), InferenceError> {
        if self.non_completing_expressions.iter().any(|identity| !self.expressions.contains_key(identity)) { return Err(InferenceError::InvalidScheme); }
        use super::{ProducerFlowKind, ProducerFlowSource};
        if self.producer_flows.owner() != self.owner { return Err(InferenceError::ForeignHandle); }
        for node in self.producer_flows.nodes() {
            self.producer_flow_scope(node.source)?;
            match node.kind {
                ProducerFlowKind::RecordUpdate { base, ref replacements } => {
                    let ProducerFlowSource::Expression(identity) = node.source else { return Err(InferenceError::InvalidScheme); };
                    let update = self.record_updates.get(&identity).ok_or(InferenceError::InvalidScheme)?;
                    if self.expression_producer_flows.get(&update.base).copied() != Some(base) || replacements.len() != update.replacements.len() { return Err(InferenceError::InvalidScheme); }
                    for (flow, replacement) in replacements.iter().zip(&update.replacements) {
                        if flow.input != replacement.producer_flow || flow.path.0.len() != replacement.path.len()
                            || flow.path.0.iter().zip(&replacement.path).any(|(component, field)| *component != ProducerPathComponent::RecordField(*field)) { return Err(InferenceError::InvalidScheme); }
                    }
                }
                ProducerFlowKind::Parameter { declaration, index } => { self.producer_flow_scope(ProducerFlowSource::Parameter { declaration, index })?; }
                ProducerFlowKind::CapturedBinding { identity, version, .. } => {
                    if !self.binding_producer_flows.contains_key(&(identity, version)) { return Err(InferenceError::InvalidScheme); }
                }
                ProducerFlowKind::Callable { declaration, origin } => {
                    if !self.declarations.contains_key(&declaration) { return Err(InferenceError::InvalidScheme); }
                    if let Some(origin) = origin {
                        let ProducerFlowSource::Expression(expression) = node.source else { return Err(InferenceError::InvalidScheme); };
                        if self.expressions.get(&expression).copied() != Some(origin) { return Err(InferenceError::InvalidScheme); }
                    } else if let ProducerFlowSource::Expression(expression) = node.source
                        && self.expression_callables.get(&expression).is_some_and(|callable| callable.declaration == Some(declaration)) {
                        return Err(InferenceError::InvalidScheme);
                    }
                }
                _ => {}
            }
        }
        for (&identity, &flow) in &self.expression_producer_flows {
            if self.producer_flows.node(flow)?.source != ProducerFlowSource::Expression(identity) { return Err(InferenceError::InvalidScheme); }
        }
        for (&identity, &flow) in &self.statement_producer_flows {
            if self.producer_flows.node(flow)?.source != ProducerFlowSource::Statement(identity) { return Err(InferenceError::InvalidScheme); }
        }
        for (&(identity, version), &flow) in &self.binding_producer_flows {
            if self.producer_flows.node(flow)?.source != (ProducerFlowSource::Binding { identity, version }) { return Err(InferenceError::InvalidScheme); }
        }
        for (&identity, declaration) in &self.declarations {
            if !declaration.parameter_producer_flows.is_empty() && declaration.parameter_producer_flows.len() != declaration.parameter_producers.len() { return Err(InferenceError::InvalidScheme); }
            for (index, &flow) in declaration.parameter_producer_flows.iter().enumerate() {
                if self.producer_flows.node(flow)?.source != (ProducerFlowSource::Parameter { declaration: identity, index: index as u32 }) { return Err(InferenceError::InvalidScheme); }
            }
            if let Some(flow) = declaration.return_producer_flow {
                if self.producer_flows.node(flow)?.source != ProducerFlowSource::DeclarationResult(identity) { return Err(InferenceError::InvalidScheme); }
            }
        }
        for (&identity, stage) in &self.stage_operations {
            if let Some(flow) = stage.input_producer_flow { self.producer_flows.node(flow)?; }
            if let Some(flow) = stage.result_producer_flow {
                if self.producer_flows.node(flow)?.source != ProducerFlowSource::Stage(identity) { return Err(InferenceError::InvalidScheme); }
            }
        }
        for (&identity, clause) in &self.comprehension_operations {
            self.producer_flows.node(clause.input_producer_flow)?;
            if self.producer_flows.node(clause.item_producer_flow)?.source != ProducerFlowSource::Comprehension(identity) { return Err(InferenceError::InvalidScheme); }
        }
        for (&identity, call) in &self.calls {
            if let Some(flow) = call.result_producer_flow {
                if self.producer_flows.node(flow)?.source != ProducerFlowSource::Expression(identity) { return Err(InferenceError::InvalidScheme); }
            }
        }
        Ok(())
    }

    fn scoped_requirement_roots(&self, graph: &InferenceContext) -> Result<Vec<ScopedRequirementRoot>, InferenceError> {
        let mut roots = std::collections::BTreeSet::new();
        roots.extend(self.constructor_application_requirement_roots()?.into_iter().map(|root| (root.requirement, root.scope)));
        roots.extend(self.run_argument_roots()?.into_iter().map(|(argument, scope)| (argument.requirement, scope)));
        for (identity, reference) in &self.registry_references {
            let scope = self.expression_scope(*identity, reference.caller)?;
            roots.extend(reference.requirements(graph)?.iter().copied().map(|requirement| (requirement, scope)));
        }
        for declaration in self.declarations.values() {
            for &requirement in &declaration.source_requirements { roots.insert((requirement, Some(declaration.scheme))); }
        }
        for call in self.calls.values() {
            let scope = call.caller.map(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
            roots.extend(call.requirements.iter().copied().map(|requirement| (requirement, scope)));
        }
        for (identity, invocation) in &self.invocations {
            let lexical_scope = invocation.caller.map(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
            roots.insert((invocation.requirement, self.expression_schemes.get(identity).copied().or_else(|| self.expression_value_scopes.get(identity).copied()).or(lexical_scope)));
        }
        for (source, operation) in self.source_operations() {
            roots.insert((operation.requirement, self.operation_scope(source, operation)?));
        }
        for (identity, boundary) in &self.registry_boundaries {
            if let Some(requirement) = boundary.requirement { roots.insert((requirement, self.expression_scope(*identity, boundary.caller)?)); }
            if let super::registry_boundaries::RegistryBoundaryKind::CommandArguments { argv_children } = &boundary.kind {
                for child in argv_children { roots.insert((child.requirement, self.expression_scope(child.source, boundary.caller)?)); }
            }
            if let super::registry_boundaries::RegistryBoundaryKind::JsonArguments { children } = &boundary.kind {
                for child in children { roots.insert((child.requirement, self.expression_scope(child.source, boundary.caller)?)); }
            }
        }
        for (identity, stage) in &self.stage_operations {
            match stage.callback {
                Some(StageCallback::Callable { requirement, .. }) => {
                    roots.insert((requirement, self.producer_flow_scope(super::ProducerFlowSource::Stage(*identity))?));
                }
                Some(StageCallback::Protocol { expression, operation, formal_slot, .. }) => {
                    if operation != stage.operation.requirement { return Err(InferenceError::InvalidScheme); }
                    let crate::sema::inference::RequirementTemplate::Operation { family, call } = graph.requirement_template(operation)? else { return Err(InferenceError::InvalidScheme); };
                    let call = graph.operation_call(call)?;
                    let actual = call.receiver.into_iter().map(Some).chain(call.arguments.iter().copied()).nth(formal_slot).flatten().ok_or(InferenceError::InvalidScheme)?;
                    let callback = ExpressionIdentity { expression, ..identity.pipeline };
                    let original = self.expressions.get(&callback).ok_or(InferenceError::InvalidScheme)?;
                    if graph.resolved(actual)? != graph.resolved(*original)? { return Err(InferenceError::InvalidScheme); }
                    let candidates = graph.family(family)?;
                    if !candidates.iter().any(|&candidate| graph.candidate(candidate).is_ok_and(|candidate| candidate.argument_relations.get(formal_slot) == Some(&crate::sema::inference::ArgumentRelation::InvocationProtocol))) {
                        return Err(InferenceError::InvalidScheme);
                    }
                    roots.insert((operation, self.producer_flow_scope(super::ProducerFlowSource::Stage(*identity))?));
                }
                _ => {}
            }
        }
        for (identity, &requirement) in &self.additions {
            let lexical_scope = self.expression_owners.get(identity).map(|owner| self.declarations.get(owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
            roots.insert((requirement, self.expression_schemes.get(identity).copied().or_else(|| self.expression_value_scopes.get(identity).copied()).or(lexical_scope)));
        }
        Ok(roots.into_iter().map(|(requirement, scope)| ScopedRequirementRoot { requirement, scope }).collect())
    }

    fn scoped_roots(&self, graph: &InferenceContext) -> Result<Vec<ScopedRoot>, InferenceError> {
        let mut roots = self.nominals.keys().map(|&ty| ScopedRoot { ty, scope: None }).collect::<Vec<_>>();
        roots.extend(self.pattern_roots()?);
        roots.extend(self.refined_reads.values().flat_map(|read| [read.invariant, read.narrowed]));
        roots.extend(self.nominal_member_roots());
        for (argument, scope) in self.run_argument_roots()? { roots.extend([ScopedRoot { ty: argument.actual, scope }, ScopedRoot { ty: argument.operand, scope }]); }
        for (identity, reference) in &self.registry_references {
            let scope = self.expression_scope(*identity, reference.caller)?;
            roots.push(ScopedRoot { ty: reference.value_type(), scope });
            for certificate in reference.certificates(graph)? {
                roots.push(ScopedRoot { ty: certificate.signature, scope });
                roots.extend(certificate.substitutions.iter().map(|&ty| ScopedRoot { ty, scope }));
            }
        }
        for declaration in self.declarations.values() {
            roots.push(ScopedRoot { ty: declaration.signature, scope: Some(declaration.scheme) });
        }
        for (identity, &ty) in &self.expressions {
            let scope = self.expression_owners.get(identity).map(|owner| self.declarations.get(owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
            let scope = self.expression_schemes.get(identity).copied().or_else(|| self.expression_value_scopes.get(identity).copied()).or(scope);
            roots.push(ScopedRoot { ty, scope });
        }
        for (identity, callable) in &self.expression_callables {
            let owner = self.expression_owners.get(identity).copied();
            roots.push(ScopedRoot { ty: callable.signature, scope: callable.scheme.or(self.expression_scope(*identity, owner)?) });
        }
        for binding in self.bindings.values() {
            let lexical_scope = binding.owner.map(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
            roots.push(ScopedRoot { ty: binding.ty, scope: binding.scheme.or(lexical_scope) });
        }
        for (identity, binding) in &self.with_bindings {
            if identity.statement.source != binding.initializer.source || identity.statement.namespace != binding.initializer.namespace
                || self.expressions.get(&binding.initializer) != Some(&binding.initializer_type)
                || self.expression_owners.get(&binding.initializer).copied() != binding.owner { return Err(InferenceError::InvalidScheme); }
            let scope = binding.owner.map(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
            roots.push(ScopedRoot { ty: binding.binding_type, scope });
            roots.push(ScopedRoot { ty: binding.initializer_type, scope: self.expression_scope(binding.initializer, binding.owner)? });
        }
        for (identity, binding) in &self.guard_error_bindings {
            if identity.statement.source != binding.initializer.source || identity.statement.namespace != binding.initializer.namespace
                || binding.name.as_str() == "_" || self.expressions.get(&binding.initializer) != Some(&binding.initializer_type)
                || self.expression_owners.get(&binding.initializer).copied() != binding.owner { return Err(InferenceError::InvalidScheme); }
            let scope = binding.owner.map(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
            roots.push(ScopedRoot { ty: binding.binding_type, scope });
            roots.push(ScopedRoot { ty: binding.initializer_type, scope: self.expression_scope(binding.initializer, binding.owner)? });
        }
        for call in self.calls.values() {
            let scope = call.caller.map(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
            roots.push(ScopedRoot { ty: call.signature, scope });
            roots.extend(call.actual_arguments.iter().chain(&call.substitutions).map(|&ty| ScopedRoot { ty, scope }));
        }
        for (source, operation) in self.source_operations() {
            let scope = self.operation_scope(source, operation)?;
            roots.push(ScopedRoot { ty: operation.result, scope });
            roots.extend(operation.receiver.iter().chain(&operation.actual_arguments).map(|&ty| ScopedRoot { ty, scope }));
        }
        for (identity, boundary) in &self.registry_boundaries {
            if !self.expressions.contains_key(identity) { return Err(InferenceError::InvalidScheme); }
            let scope = self.expression_scope(*identity, boundary.caller)?;
            roots.extend([ScopedRoot { ty: boundary.input, scope }, ScopedRoot { ty: boundary.result, scope }]);
            if let super::registry_boundaries::RegistryBoundaryKind::SchemaValidation { schema, .. } = &boundary.kind { roots.push(ScopedRoot { ty: schema.shape, scope: None }); }
            if let super::registry_boundaries::RegistryBoundaryKind::CommandArguments { argv_children } = &boundary.kind {
                for child in argv_children { roots.push(ScopedRoot { ty: child.actual, scope: self.expression_scope(child.source, boundary.caller)? }); }
            }
            if let super::registry_boundaries::RegistryBoundaryKind::JsonArguments { children } = &boundary.kind {
                for child in children { roots.push(ScopedRoot { ty: child.actual, scope: self.expression_scope(child.source, boundary.caller)? }); }
            }
        }
        for (identity, projection) in &self.projections {
            let scope = self.expression_owners.get(identity).map(|owner| self.declarations.get(owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
            for ty in [projection.receiver, projection.result] { roots.push(ScopedRoot { ty, scope }); }
        }
        for (identity, update) in &self.record_updates {
            let scope = self.expression_scope(*identity, update.caller)?;
            roots.extend([update.receiver, update.result].into_iter().map(|ty| ScopedRoot { ty, scope }));
            for replacement in &update.replacements {
                let value_scope = self.record_update_value_scope(replacement.source)?;
                roots.push(ScopedRoot { ty: replacement.value, scope: value_scope });
                for projection in &replacement.projections {
                    roots.extend([projection.receiver, projection.result].into_iter().map(|ty| ScopedRoot { ty, scope }));
                }
            }
        }
        roots.extend(self.result_wrappings.values().chain(self.result_statement_wrappings.values()).map(|&ty| ScopedRoot { ty, scope: None }));
        Ok(roots)
    }
}

impl SolvedTypes {
    pub(crate) fn from_graph(graph: SolvedGraph, symbols: crate::symbol::SymbolOwner) -> Self {
        let mut solved = Self::with_graph(graph.owner(), graph);
        solved.symbols = symbols;
        solved
    }

    pub fn validate(&self) -> Result<(), InferenceError> {
        for &read in self.refined_reads.keys() { self.checked_refined_read(read)?; }
        if self.refined_reads.len() != self.original_refined_reads.len() { return Err(InferenceError::InvalidScheme); }
        self.validate_patterns(&self.graph)?;
        self.validate_nominal_members(&self.graph)?;
        self.validate_argument_sources()?;
        self.validate_argument_bindings(&self.graph)?;
        self.validate_run_operations(&self.graph)?;
        self.validate_spawn_operations(&self.graph)?;
        for instance in self.registry_reference_roots(&self.graph)? { self.graph.validate_instance_scoped(&instance)?; }
        self.validate_registry_boundaries(&self.graph)?;
        self.validate_record_updates(&self.graph)?;
        self.validate_module_projections(&self.graph)?;
        self.validate_schema_validations(&self.graph)?;
        self.validate_constructor_applications(&self.graph)?;
        self.graph.validate_application_roots(&self.source_application_roots(&self.graph)?)?;
        self.operation_catalog.validate(&self.graph)?;
        for root in self.operation_catalog.scoped_roots(&self.graph)? { self.graph.validate_scoped(root)?; }
        validate_call_requirement_origins(&self.graph, &self.calls)?;
        self.validate_embedded_bridges()?;
        self.validate_producer_flow_roots()?;
        self.validate_native_producer_flows(&self.graph)?;
        for root in self.scoped_requirement_roots(&self.graph)? { self.graph.validate_requirement_scoped(root)?; }
        for root in self.scoped_roots(&self.graph)? { self.graph.validate_scoped(root)?; }
        for root in self.module_projection_roots()? { self.graph.validate_scoped(root)?; }
        for root in self.schema_validation_roots(&self.graph)? { self.graph.validate_scoped(root)?; }
        for root in self.constructor_application_roots(&self.graph)? { self.graph.validate_scoped(root)?; }
        Ok(())
    }
}

impl SolvedTypes<InferenceContext> {
    pub(super) fn retain_operation_catalog(&mut self, registry: &crate::sema::registry_graph::RegistryGraph, language: &crate::sema::operation_graph::OperationGraph, stages: &crate::sema::stage_graph::StageGraph) -> Result<(), InferenceError> {
        let mut candidates = std::collections::BTreeSet::new();
        for (_, operation) in self.source_operations() {
            let crate::sema::inference::RequirementTemplate::Operation { family, .. } = self.graph.requirement_template(operation.requirement)? else { return Err(InferenceError::InvalidScheme); };
            candidates.extend(self.graph.family(family)?.iter().copied());
        }
        for requirement in self.constructor_applications.values().filter_map(|application| application.requirement) {
            self.graph.charge_source_fact_work(1)?;
            let crate::sema::inference::RequirementTemplate::Operation { family, .. } = self.graph.requirement_template(requirement)? else { return Err(InferenceError::InvalidScheme); };
            self.graph.charge_source_fact_work(self.graph.family(family)?.len() as u64)?;
            candidates.extend(self.graph.family(family)?.iter().copied());
        }
        for reference in self.registry_references.values() {
            let members = reference.candidates(&self.graph)?;
            self.graph.charge_source_fact_work(members.len() as u64 + 1)?;
            candidates.extend(members);
        }
        self.graph.charge_source_fact_nodes(candidates.len() as u64)?;
        let candidates = candidates.into_iter().collect::<Vec<_>>();
        for (candidate, metadata) in registry.retained_candidates(&self.graph, &candidates)? { self.operation_catalog.insert(candidate, super::SolvedOperationAuthority::Registry(metadata))?; }
        for (candidate, metadata) in language.retained_candidates(&self.graph, &candidates)? { self.operation_catalog.insert(candidate, super::SolvedOperationAuthority::Language(metadata))?; }
        for (candidate, metadata) in stages.retained_candidates(&self.graph, &candidates)? { self.operation_catalog.insert(candidate, super::SolvedOperationAuthority::Stage(metadata))?; }
        for &candidate in &candidates { self.operation_catalog.candidate(&self.graph, candidate)?; }
        for schema in registry.retained_schemas(&self.graph)? { self.operation_catalog.insert_schema(schema)?; }
        for error in registry.retained_errors(&self.graph)? { self.operation_catalog.insert_error(error)?; }
        self.operation_catalog.validate(&self.graph)
    }

    #[cfg(test)]
    pub(crate) fn freeze_fixture(self) -> Result<SolvedTypes, InferenceError> { self.freeze() }

    pub(super) fn freeze(mut self) -> Result<SolvedTypes, InferenceError> {
        self.seal_refined_reads()?;
        self.seal_embedded_bridge_calls()?;
        self.capture_pattern_nominals()?;
        self.graph.charge_source_fact_work(self.nominal_member_source_work())?;
        self.validate_nominal_members(&self.graph)?;
        let pattern_work = self.pattern_source_edges();
        self.graph.charge_source_fact_edges(pattern_work)?;
        self.graph.charge_source_fact_work(pattern_work)?;
        let compared = self.validate_patterns(&self.graph)?;
        self.graph.charge_source_fact_work(compared)?;
        let argument_work = self.argument_sources.values().chain(self.stage_argument_sources.values()).map(|arguments| arguments.len() as u64 + 1).sum::<u64>()
            + self.calls.len() as u64
            + self.invocations.keys().filter_map(|identity| self.argument_sources.get(identity)).map(|arguments| arguments.len() as u64 + 1).sum::<u64>();
        self.graph.charge_source_fact_work(argument_work)?;
        self.validate_argument_sources()?;
        self.validate_argument_bindings(&self.graph)?;
        let requirement_edges = self.declarations.values().map(|declaration| declaration.source_requirements.len() as u64).sum::<u64>()
            + self.calls.values().map(|call| (call.requirements.len() + call.requirement_origins.len() * 2) as u64).sum::<u64>()
            + self.registry_references.values().map(|reference| reference.source_edges() as u64).sum::<u64>()
            + self.argument_sources.values().chain(self.stage_argument_sources.values()).map(|arguments| arguments.len() as u64 + 1).sum::<u64>()
            + self.stage_callback_certificate_edges()
            + self.run_operations.len() as u64 * 3
            + self.spawn_operations.len() as u64 * 2
            + self.constructor_applications.values().filter(|application| application.requirement.is_some()).count() as u64
            + self.run_operations.values().map(|run| run.arguments.len() as u64 * 3).sum::<u64>() + self.spawn_operations.values().map(|spawn| spawn.arguments.len() as u64 * 3).sum::<u64>()
            + self.operations.len() as u64 + self.statement_operations.len() as u64 + self.comprehension_operations.len() as u64 + self.stage_operations.len() as u64 + self.stage_operations.values().map(|stage| match stage.callback { Some(StageCallback::Callable { .. }) => 1, Some(StageCallback::Protocol { .. }) => 2, _ => 0 }).sum::<u64>() + self.invocations.len() as u64 + self.additions.len() as u64;
        self.graph.charge_source_fact_edges(requirement_edges)?;
        self.graph.charge_source_fact_work(self.producer_flows.nodes().len() as u64
            + self.expression_producer_flows.len() as u64 + self.binding_producer_flows.len() as u64
            + self.declarations.len() as u64 + self.calls.len() as u64 + requirement_edges)?;
        let update_edges = self.record_updates.values().map(|update| 3 + update.replacements.iter().map(|replacement| 2 + replacement.path.len() * 3).sum::<usize>()).sum::<usize>();
        self.graph.charge_source_fact_edges(update_edges as u64)?;
        let update_work = self.record_updates.values().flat_map(|update| &update.replacements).flat_map(|replacement| &replacement.projections).try_fold(0_u64, |work, projection| {
            let (_, field_work) = record_update_field(&self.graph, projection.receiver, projection.field)?;
            Ok(work + field_work)
        })?;
        self.graph.charge_source_fact_work(update_work + update_edges as u64)?;
        self.validate_record_updates(&self.graph)?;
        self.validate_module_projections(&self.graph)?;
        self.validate_schema_validations(&self.graph)?;
        self.validate_constructor_applications(&self.graph)?;
        validate_call_requirement_origins(&self.graph, &self.calls)?;
        self.validate_producer_flow_roots()?;
        self.validate_native_producer_flows(&self.graph)?;
        let mut roots = self.scoped_roots(&self.graph)?;
        roots.extend(self.module_projection_roots()?);
        roots.extend(self.schema_validation_roots(&self.graph)?);
        roots.extend(self.constructor_application_roots(&self.graph)?);
        let mut effects = Vec::new();
        for (identity, reference) in &self.registry_references {
            let scope = self.expression_scope(*identity, reference.caller)?;
            for certificate in reference.certificates(&self.graph)? {
            effects.extend(certificate.effect_substitutions.iter().map(|&effect| ScopedEffectRoot { effect: EffectSummary::Variable(effect), scope }));
            effects.extend(certificate.effect_roots.iter().map(|&effect| ScopedEffectRoot { effect, scope }));
            }
        }
        for node in self.producer_flows.nodes() {
            if let super::ProducerFlowKind::Known(profile) = &node.kind {
                retain_profile_effect_roots(profile, self.producer_flow_scope(node.source)?, &mut effects);
            }
            if let super::ProducerFlowKind::Operation { outputs, .. } = &node.kind {
                let scope = self.producer_flow_scope(node.source)?;
                effects.extend([outputs.pull, outputs.close].into_iter().map(|effect| ScopedEffectRoot { effect, scope }));
            }
        }
        for declaration in self.declarations.values() {
            for effect in [declaration.effective_effects, declaration.required_effects] {
                effects.push(ScopedEffectRoot { effect, scope: Some(declaration.scheme) });
            }
            for profile in declaration.parameter_producers.iter().chain(std::iter::once(&declaration.return_producers)) {
                retain_profile_effect_roots(profile, Some(declaration.scheme), &mut effects);
            }
        }
        for (identity, profile) in &self.expression_producers {
            if !self.expressions.contains_key(identity) { return Err(InferenceError::InvalidScheme); }
            let lexical_scope = self.expression_owners.get(identity).map(|owner| self.declarations.get(owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
            let scope = self.expression_schemes.get(identity).copied().or_else(|| self.expression_value_scopes.get(identity).copied()).or(lexical_scope);
            retain_profile_effect_roots(profile, scope, &mut effects);
        }
        for (identity, profile) in &self.binding_producers {
            let binding = self.bindings.get(identity).ok_or(InferenceError::InvalidScheme)?;
            let lexical_scope = binding.owner.map(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
            retain_profile_effect_roots(profile, binding.scheme.or(lexical_scope), &mut effects);
        }
        for call in self.calls.values() {
            let scope = call.caller.map(|owner| self.declarations.get(&owner).map(|declaration| declaration.scheme).ok_or(InferenceError::InvalidScheme)).transpose()?;
            effects.extend(call.effect_substitutions.iter().map(|&effect| ScopedEffectRoot { effect, scope }));
            for profile in call.argument_producers.iter().chain(std::iter::once(&call.result_producers)) {
                retain_profile_effect_roots(profile, scope, &mut effects);
            }
        }
        for (source, operation) in self.source_operations() {
            let scope = self.operation_scope(source, operation)?;
            effects.push(ScopedEffectRoot { effect: operation.effects, scope });
        }
        let requirements = self.scoped_requirement_roots(&self.graph)?;
        roots.extend(self.operation_catalog.scoped_roots(&self.graph)?);
        self.operation_catalog.validate(&self.graph)?;
        self.validate_registry_boundaries(&self.graph)?;
        self.validate_run_operations(&self.graph)?;
        self.validate_spawn_operations(&self.graph)?;
        let instances = self.registry_reference_roots(&self.graph)?;
        let applications = self.source_application_roots(&self.graph)?;
        let graph = self.graph.freeze_scoped_with_applications(&roots, &effects, &requirements, &instances, &applications)?;
        self.producer_flows.normalize_effects(&graph)?;
        for declaration in self.declarations.values_mut() {
            declaration.effective_effects = graph.closed_effect_summary(declaration.effective_effects)?;
            declaration.required_effects = graph.closed_effect_summary(declaration.required_effects)?;
            for profile in declaration.parameter_producers.iter_mut().chain(std::iter::once(&mut declaration.return_producers)) { normalize_profile_effects(profile, &graph)?; }
        }
        for call in self.calls.values_mut() {
            for effect in &mut call.effect_substitutions { *effect = graph.closed_effect_summary(*effect)?; }
            for profile in call.argument_producers.iter_mut().chain(std::iter::once(&mut call.result_producers)) { normalize_profile_effects(profile, &graph)?; }
        }
        for profile in self.expression_producers.values_mut().chain(self.binding_producers.values_mut()) { normalize_profile_effects(profile, &graph)?; }
        for operation in self.operations.values_mut().chain(self.statement_operations.values_mut()).chain(self.comprehension_operations.values_mut().map(|clause| &mut clause.operation)).chain(self.stage_operations.values_mut().map(|stage| &mut stage.operation)) { operation.effects = graph.closed_effect_summary(operation.effects)?; }
        Ok(SolvedTypes {
            owner: self.owner, graph, symbols: self.symbols, operation_catalog: self.operation_catalog, registry_boundaries: self.registry_boundaries, registry_references: self.registry_references, run_operations: self.run_operations, spawn_operations: self.spawn_operations, module_projections: self.module_projections, schema_validations: self.schema_validations, constructor_applications: self.constructor_applications, constructor_defaults: self.constructor_defaults, constructor_nominals: self.constructor_nominals,
            producer_flows: self.producer_flows,
            embedded_bridges: self.embedded_bridges, embedded_bridge_calls: self.embedded_bridge_calls,
            declarations: self.declarations, nominals: self.nominals, nominal_members: self.nominal_members, original_nominal_members: self.original_nominal_members, expressions: self.expressions, expression_schemes: self.expression_schemes, non_completing_expressions: self.non_completing_expressions, expression_value_scopes: self.expression_value_scopes,
            expression_callables: self.expression_callables, expression_producers: self.expression_producers, bindings: self.bindings, with_bindings: self.with_bindings, guard_error_bindings: self.guard_error_bindings, refined_reads: self.refined_reads, original_refined_reads: self.original_refined_reads, binding_producers: self.binding_producers, patterns: self.patterns, original_patterns: self.original_patterns, original_pattern_nominals: self.original_pattern_nominals, pattern_value_scopes: self.pattern_value_scopes,
            expression_producer_flows: self.expression_producer_flows, statement_producer_flows: self.statement_producer_flows, statement_owners: self.statement_owners, binding_producer_flows: self.binding_producer_flows,
            argument_sources: self.argument_sources, stage_argument_sources: self.stage_argument_sources, calls: self.calls, invocations: self.invocations, operations: self.operations, statement_operations: self.statement_operations, comprehension_operations: self.comprehension_operations, stage_operations: self.stage_operations, projections: self.projections, record_updates: self.record_updates,
            additions: self.additions, statements: self.statements,
            expression_owners: self.expression_owners,
            result_wrappings: self.result_wrappings,
            result_statement_wrappings: self.result_statement_wrappings,
        })
    }
}

fn record_update_field(graph: &InferenceContext, receiver: TypeId, field: Name) -> Result<(TypeId, u64), InferenceError> {
    let crate::sema::inference::TypeNode::Record(row) = graph.node(graph.resolved(receiver)?)? else { return Err(InferenceError::InvalidScheme); };
    let mut row = *row;
    let mut visited = std::collections::BTreeSet::new();
    let mut work = 0;
    for _ in 0..graph.limits().structural_depth {
        if !visited.insert(row) { return Err(InferenceError::InvalidScheme); }
        let data = graph.row_data(row)?;
        work += data.fields.len() as u64 + 1;
        if let Some(target) = data.fields.iter().find(|target| target.label == field) { return Ok((target.ty, work)); }
        let tail = data.tail.ok_or(InferenceError::InvalidScheme)?;
        let crate::sema::inference::TypeNode::Row(next) = graph.node(graph.resolved(tail)?)? else { return Err(InferenceError::InvalidScheme); };
        row = *next;
    }
    Err(InferenceError::Limit("record update projection depth"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::inference::{Atom, EffectSet};

    #[test]
    fn producer_profile_publication_rejects_foreign_effect_handles() {
        let mut facts = SolvedTypes::<InferenceContext>::default();
        let identity = ExpressionIdentity { source: SourceId::new(0), namespace: None, expression: ExprId::from_index(0) };
        let item = facts.graph.atom(Atom::Int).unwrap();
        let value = facts.graph.stream(item).unwrap();
        facts.expressions.insert(identity, value);
        let mut foreign = InferenceContext::default();
        let pull = EffectSummary::Variable(foreign.fresh_execution_effect(None).unwrap());
        facts.expression_producers.insert(identity, [(ProducerPath::default(), ProducerEffects { pull, close: EffectSummary::Closed(EffectSet::EMPTY) })].into());
        assert!(matches!(facts.freeze(), Err(InferenceError::ForeignHandle)), "producer roles retain the same graph owner as their source value");
    }

    #[test]
    fn producer_flow_publication_rejects_missing_source_facts() {
        let mut facts = SolvedTypes::<InferenceContext>::default();
        let identity = ExpressionIdentity { source: SourceId::new(0), namespace: None, expression: ExprId::from_index(0) };
        let flow = facts.producer_flows.push(&mut facts.graph, super::super::ProducerFlowSource::Expression(identity), super::super::ProducerFlowKind::Empty).unwrap();
        facts.expression_producer_flows.insert(identity, flow);
        assert!(matches!(facts.freeze(), Err(InferenceError::InvalidScheme)), "a flow root must retain its actual checked source value");
    }
}
