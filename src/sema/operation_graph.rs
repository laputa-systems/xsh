use crate::sema::inference::{ArgumentRelation, Arrow, Atom, CallableKind, CandidateId, CandidateTemplate, EffectProjection, EffectRole, EffectRoleReference, EffectSet, EffectSummary, Eligibility, Generalization, GraphOwner, InferenceContext, InferenceError, OperationFailureProjection, OperationFamilyId, Parameter, RequirementId, RowField, SchemeId, TypeId};
use crate::source::Span;
use crate::symbol::Name;
use crate::syntax::node::{AssignOp, BinaryOp, RunKind, UnaryOp};
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LanguageDisposition { SealedRequirement, ParametricTemplate, FixedBoundary, DynamicBoundary, StaticIdentity }

#[derive(Clone, Copy, Debug)]
pub(crate) struct LanguageAuthority { pub id: &'static str, pub family: &'static str, pub disposition: LanguageDisposition }

pub(crate) const LANGUAGE_AUTHORITIES: &[LanguageAuthority] = &[
    LanguageAuthority { id: "language.assertion", family: "statement_boundary", disposition: LanguageDisposition::FixedBoundary },
    LanguageAuthority { id: "language.assignment.Add", family: "compound_arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.assignment.Div", family: "compound_arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.assignment.Mul", family: "compound_arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.assignment.Rem", family: "compound_arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.assignment.Set", family: "assignment", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.assignment.Sub", family: "compound_arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Add.duration", family: "arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Add.float", family: "arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Add.integer", family: "arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Add.list", family: "arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Add.text", family: "arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.And", family: "boolean_operator", disposition: LanguageDisposition::FixedBoundary },
    LanguageAuthority { id: "language.binary.Div.duration_ratio", family: "arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Div.duration_scale", family: "arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Div.float", family: "arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Div.integer", family: "arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Eq", family: "equality", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Ge", family: "ordering", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Gt", family: "ordering", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.In.Bytes", family: "membership", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.In.EnvPathList", family: "membership", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.In.List", family: "membership", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.In.Map", family: "membership", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.In.Path", family: "membership", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.In.Record", family: "membership", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.In.Str", family: "membership", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Le", family: "ordering", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Lt", family: "ordering", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Mul.duration_scale", family: "arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Mul.duration_scale_reverse", family: "arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Mul.float", family: "arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Mul.integer", family: "arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Ne", family: "equality", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.NotIn.Bytes", family: "membership", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.NotIn.EnvPathList", family: "membership", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.NotIn.List", family: "membership", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.NotIn.Map", family: "membership", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.NotIn.Path", family: "membership", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.NotIn.Record", family: "membership", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.NotIn.Str", family: "membership", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Or", family: "boolean_operator", disposition: LanguageDisposition::FixedBoundary },
    LanguageAuthority { id: "language.binary.Rem.integer", family: "arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.ResultFallback.Optional", family: "fallback", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.binary.ResultFallback.Result", family: "fallback", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.binary.Sub.duration", family: "arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Sub.float", family: "arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.binary.Sub.integer", family: "arithmetic", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.call.checked_alias", family: "callable_application", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.call.direct_named", family: "callable_application", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.call.erased_Proc", family: "callable_application", disposition: LanguageDisposition::DynamicBoundary },
    LanguageAuthority { id: "language.call.erased_Pure", family: "callable_application", disposition: LanguageDisposition::DynamicBoundary },
    LanguageAuthority { id: "language.call.module_contract", family: "callable_application", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.call.named_spread", family: "argument_binding", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.call.rest_and_splice", family: "callable_application", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.call.stage_descriptor", family: "callable_descriptor", disposition: LanguageDisposition::StaticIdentity },
    LanguageAuthority { id: "language.command.cd", family: "core_command", disposition: LanguageDisposition::FixedBoundary },
    LanguageAuthority { id: "language.command.env", family: "core_command", disposition: LanguageDisposition::FixedBoundary },
    LanguageAuthority { id: "language.command.eprint", family: "core_command", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.command.print", family: "core_command", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.comparison_chain", family: "ordering", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.comprehension.List", family: "comprehension", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.comprehension.Map", family: "comprehension", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.constructor.Err", family: "constructor", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.constructor.Ok", family: "constructor", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.constructor.Path", family: "constructor", disposition: LanguageDisposition::FixedBoundary },
    LanguageAuthority { id: "language.constructor.error_variant", family: "constructor", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.constructor.range", family: "constructor", disposition: LanguageDisposition::FixedBoundary },
    LanguageAuthority { id: "language.constructor.record", family: "constructor", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.constructor.tag", family: "constructor", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.default.callable", family: "default", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.default.record", family: "default", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.descriptor.cli", family: "constructor_descriptor", disposition: LanguageDisposition::DynamicBoundary },
    LanguageAuthority { id: "language.dynamic.membership", family: "dynamic_operation", disposition: LanguageDisposition::DynamicBoundary },
    LanguageAuthority { id: "language.dynamic.operators", family: "dynamic_operation", disposition: LanguageDisposition::DynamicBoundary },
    LanguageAuthority { id: "language.dynamic.pipeline", family: "dynamic_operation", disposition: LanguageDisposition::DynamicBoundary },
    LanguageAuthority { id: "language.dynamic.projection", family: "dynamic_operation", disposition: LanguageDisposition::DynamicBoundary },
    LanguageAuthority { id: "language.index.List", family: "collection_index", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.index.Map", family: "collection_index", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.iteration.Bytes", family: "iteration", disposition: LanguageDisposition::FixedBoundary },
    LanguageAuthority { id: "language.iteration.Bytes.Result", family: "iteration", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.iteration.List", family: "iteration", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.iteration.List.Result", family: "iteration", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.iteration.Map", family: "iteration", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.iteration.Map.Result", family: "iteration", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.iteration.Str", family: "iteration", disposition: LanguageDisposition::FixedBoundary },
    LanguageAuthority { id: "language.iteration.Str.Result", family: "iteration", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.iteration.Stream", family: "iteration", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.iteration.Stream.Result", family: "iteration", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.literal.List", family: "collection_construction", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.literal.Map", family: "collection_construction", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.literal.Record", family: "collection_construction", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.map_key.Bool", family: "map_keys", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.map_key.Bytes", family: "map_keys", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.map_key.Duration", family: "map_keys", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.map_key.Int", family: "map_keys", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.map_key.Path", family: "map_keys", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.map_key.Str", family: "map_keys", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.map_key.UInt", family: "map_keys", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.pattern.test", family: "pattern_operator", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.postfix.optional", family: "optional_operation", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.postfix.propagate", family: "result_boundary", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.projection.constant_key", family: "constant_key", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.projection.field", family: "row_projection", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.record.update", family: "row_update", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.require.contextual", family: "validation", disposition: LanguageDisposition::DynamicBoundary },
    LanguageAuthority { id: "language.require.explicit", family: "validation", disposition: LanguageDisposition::DynamicBoundary },
    LanguageAuthority { id: "language.run.CaptureBytes", family: "process_run", disposition: LanguageDisposition::FixedBoundary },
    LanguageAuthority { id: "language.run.CaptureBytesRecord", family: "process_run", disposition: LanguageDisposition::FixedBoundary },
    LanguageAuthority { id: "language.run.CaptureText", family: "process_run", disposition: LanguageDisposition::FixedBoundary },
    LanguageAuthority { id: "language.run.CaptureTextRecord", family: "process_run", disposition: LanguageDisposition::FixedBoundary },
    LanguageAuthority { id: "language.run.Plain", family: "process_run", disposition: LanguageDisposition::FixedBoundary },
    LanguageAuthority { id: "language.run.Status", family: "process_run", disposition: LanguageDisposition::FixedBoundary },
    LanguageAuthority { id: "language.run.StreamBytes", family: "process_run", disposition: LanguageDisposition::FixedBoundary },
    LanguageAuthority { id: "language.run.StreamText", family: "process_run", disposition: LanguageDisposition::FixedBoundary },
    LanguageAuthority { id: "language.slice.Bytes", family: "collection_slice", disposition: LanguageDisposition::FixedBoundary },
    LanguageAuthority { id: "language.slice.List", family: "collection_slice", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.slice.Str", family: "collection_slice", disposition: LanguageDisposition::FixedBoundary },
    LanguageAuthority { id: "language.spawn", family: "process_lifecycle", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.unary.Neg", family: "unary_operator", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.unary.Not", family: "unary_operator", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.wait", family: "process_lifecycle", disposition: LanguageDisposition::SealedRequirement },
    LanguageAuthority { id: "language.yield", family: "producer_item", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.yield_delegation.List", family: "delegation", disposition: LanguageDisposition::ParametricTemplate },
    LanguageAuthority { id: "language.yield_delegation.Stream", family: "delegation", disposition: LanguageDisposition::ParametricTemplate },
];

const ERROR_FIELD_AUTHORITY: LanguageAuthority = LanguageAuthority { id: "language.projection.error_message", family: "error_field", disposition: LanguageDisposition::FixedBoundary };

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArithmeticDomain { Integer { left: Atom, right: Atom }, Float, Text, List, DurationPair, DurationScale { duration_left: bool }, DurationRatio, PathJoin { right: Atom } }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MembershipDomain { List, Map, Str, Bytes, Record, Path { needle: Atom }, EnvPathList }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IterableDomain { List, Stream, Map, Str, Bytes }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SliceDomain { List, Str, Bytes }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ValueConstructor { Ok, Err, Path, Range }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PreparedLanguageOperation {
    Arithmetic { op: BinaryOp, domain: ArithmeticDomain },
    Compound { op: AssignOp, domain: ArithmeticDomain },
    Assignment,
    Ordering { op: BinaryOp, left: Atom, right: Atom },
    Equality { op: BinaryOp },
    Membership { negated: bool, domain: MembershipDomain },
    Boolean { op: BinaryOp },
    Fallback { result: bool },
    Unary { op: UnaryOp, operand: Atom },
    Iteration { domain: IterableDomain, outer_result: bool },
    Index { map: bool },
    ConstantKeyProjection { field: Name },
    ErrorField { receiver: Atom, field: Name },
    Slice(SliceDomain),
    Constructor { kind: ValueConstructor, arity: usize },
    Declaration { authority: &'static str, identity: Name },
    Display { stderr: bool },
    Wait { list: bool, erased: bool },
    Run { kind: RunKind, policy: bool, propagate: bool },
    Spawn { command: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OperationArgumentOrder { SourceOrder, ReceiverThenNeedle }

#[derive(Clone, Debug)]
pub(crate) struct OperationCandidate {
    pub identity: Name,
    pub scheme: SchemeId,
    pub authority: &'static str,
    pub operation: PreparedLanguageOperation,
    pub argument_order: OperationArgumentOrder,
    pub statement_result_is_unit: bool,
}

#[derive(Default)]
pub(crate) struct OperationGraph {
    owner: Option<GraphOwner>,
    families: BTreeMap<String, OperationFamilyId>,
    candidates: FxHashMap<CandidateId, OperationCandidate>,
}

impl OperationGraph {
    pub(crate) fn spawn_family(&mut self, graph: &mut InferenceContext, command: bool) -> Result<OperationFamilyId, InferenceError> {
        self.family(graph, format!("spawn.{command}"), |catalog, graph| {
            let handle = graph.atom(Atom::ProcessHandle)?;
            let error = graph.atom(Atom::ProcessError)?;
            let result = graph.result(handle, error)?;
            let parameters = if command { vec![Parameter { label: Name::intern("command"), ty: graph.atom(Atom::Command)?, defaulted: false, rest: false }] } else { Vec::new() };
            let signature = graph.arrow(Arrow { kind: CallableKind::Proc, params: parameters, result, effects: EffectSummary::Closed(EffectSet::PROCESS) })?;
            let scheme = graph.generalize(signature, 0, Generalization::Allowed, &[])?;
            let operation = PreparedLanguageOperation::Spawn { command };
            let identity = Name::intern(&format!("language.spawn:{operation:?}"));
            let candidate = graph.register_candidate(CandidateTemplate { failure_projection: None, identity, public_label: Name::intern("language.spawn"), scheme,
                has_receiver: false, actual_eligibility: Vec::new(), argument_relations: if command { vec![ArgumentRelation::Assignable] } else { Vec::new() }, effect_roles: Vec::new(), output_effect_roles: Vec::new() })?;
            catalog.candidates.insert(candidate, OperationCandidate { identity, scheme, authority: authority("language.spawn")?.id, operation, argument_order: OperationArgumentOrder::SourceOrder, statement_result_is_unit: false });
            Ok(vec![candidate])
        })
    }

    pub(crate) fn run_family(&mut self, graph: &mut InferenceContext, kind: RunKind, policy: bool, propagate: bool) -> Result<OperationFamilyId, InferenceError> {
        self.family(graph, format!("run.{kind:?}.{policy}.{propagate}"), |catalog, graph| {
            let label = format!("language.run.{kind:?}");
            let authority = authority(&label)?.id;
            let result = run_result_type(graph, kind, propagate)?;
            let effects = EffectSummary::Closed(EffectSet(EffectSet::PROCESS.0 | if propagate { EffectSet::ERROR.0 } else { 0 }));
            let arrow = graph.arrow(Arrow { kind: CallableKind::Proc, params: Vec::new(), result, effects })?;
            let producer = matches!(kind, RunKind::StreamText | RunKind::StreamBytes);
            let roots = if producer {
                vec![EffectSummary::Closed(if policy { EffectSet(EffectSet::PROCESS.0 | EffectSet::ERROR.0) } else { EffectSet::EMPTY }),
                    EffectSummary::Closed(if policy { EffectSet::PROCESS } else { EffectSet::EMPTY })]
            } else { Vec::new() };
            let scheme = graph.generalize_with_effect_roots(arrow, 0, Generalization::Allowed, &[], &roots)?;
            let operation = PreparedLanguageOperation::Run { kind, policy, propagate };
            let identity = Name::intern(&format!("{authority}:{operation:?}"));
            let candidate = graph.register_candidate(CandidateTemplate { failure_projection: None,
                identity, public_label: Name::intern(&label), scheme, has_receiver: false, actual_eligibility: Vec::new(), argument_relations: Vec::new(), effect_roles: Vec::new(),
                output_effect_roles: if producer { vec![(crate::sema::inference::ProducerRole::Pull, 0), (crate::sema::inference::ProducerRole::Close, 1)] } else { Vec::new() },
            })?;
            catalog.candidates.insert(candidate, OperationCandidate { identity, scheme, authority, operation, argument_order: OperationArgumentOrder::SourceOrder, statement_result_is_unit: false });
            Ok(vec![candidate])
        })
    }

    fn bind_owner(&mut self, graph: &InferenceContext) -> Result<(), InferenceError> {
        match self.owner { Some(owner) if owner != graph.owner() => Err(InferenceError::ForeignHandle), None => { self.owner = Some(graph.owner()); Ok(()) }, _ => Ok(()) }
    }

    fn family(&mut self, graph: &mut InferenceContext, key: String, build: impl FnOnce(&mut Self, &mut InferenceContext) -> Result<Vec<CandidateId>, InferenceError>) -> Result<OperationFamilyId, InferenceError> {
        self.bind_owner(graph)?;
        if let Some(id) = self.families.get(&key).copied() { if graph.family(id).is_ok() { return Ok(id); } }
        self.families.retain(|_, id| graph.family(*id).is_ok()); self.candidates.retain(|id, _| graph.candidate(*id).is_ok());
        let result = graph.probe(|graph| { let candidates = build(self, graph)?; let family = graph.register_family(&candidates)?; self.families.insert(key, family); Ok(family) });
        if result.is_err() { self.families.retain(|_, id| graph.family(*id).is_ok()); self.candidates.retain(|id, _| graph.candidate(*id).is_ok()); }
        result
    }

    pub(crate) fn metadata(&self, graph: &InferenceContext, id: CandidateId) -> Result<&OperationCandidate, InferenceError> {
        if self.owner != Some(graph.owner()) { return Err(InferenceError::ForeignHandle); }
        let candidate = graph.candidate(id)?; let metadata = self.candidates.get(&id).ok_or(InferenceError::ForeignHandle)?;
        if candidate.identity != metadata.identity || candidate.scheme != metadata.scheme { return Err(InferenceError::InvalidScheme); }
        Ok(metadata)
    }

    pub(crate) fn retained_candidates(&self, graph: &InferenceContext, candidates: &[CandidateId]) -> Result<Vec<(CandidateId, OperationCandidate)>, InferenceError> {
        if self.owner.is_some_and(|owner| owner != graph.owner()) { return Err(InferenceError::ForeignHandle); }
        let mut retained = Vec::new();
        for &candidate in candidates {
            graph.candidate(candidate)?;
            if self.candidates.contains_key(&candidate) { retained.push((candidate, self.metadata(graph, candidate)?.clone())); }
        }
        Ok(retained)
    }

    fn candidate(&mut self, graph: &mut InferenceContext, contract: LanguageContract) -> Result<CandidateId, InferenceError> {
        let authority = authority(&contract.authority)?;
        let identity = match contract.operation {
            PreparedLanguageOperation::ConstantKeyProjection { field } => Name::intern(&format!("{}:{field}", authority.id)),
            PreparedLanguageOperation::Display { stderr } => Name::intern(&format!("{}:Display:{stderr}:{}", authority.id, contract.parameters.len())),
            _ => Name::intern(&format!("{}:{:?}", authority.id, contract.operation)),
        };
        let arrow = graph.arrow(Arrow { kind: CallableKind::Pure, params: contract.parameters, result: contract.result, effects: contract.effects })?;
        let scheme = graph.generalize(arrow, 0, Generalization::Allowed, &contract.requirements)?;
        let id = graph.register_candidate(CandidateTemplate { failure_projection: None, output_effect_roles:Vec::new(), public_label: Name::intern(authority.id), effect_roles: Vec::new(), identity, scheme, has_receiver: contract.receiver, actual_eligibility: Vec::new(), argument_relations: contract.relations })?;
        self.candidates.insert(id, OperationCandidate { identity, scheme, authority: authority.id, operation: contract.operation, argument_order: if contract.receiver { OperationArgumentOrder::ReceiverThenNeedle } else { OperationArgumentOrder::SourceOrder }, statement_result_is_unit: contract.statement });
        Ok(id)
    }

    pub(crate) fn binary_family(&mut self, graph: &mut InferenceContext, op: BinaryOp, origin: Span) -> Result<OperationFamilyId, InferenceError> {
        self.family(graph, format!("binary.{op:?}"), |catalog, graph| {
            let mut candidates = Vec::new();
            match op {
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                    for domain in arithmetic_domains(op, false) {
                        let (left, right, result) = arithmetic_types(graph, domain, origin)?;
                        let operation = PreparedLanguageOperation::Arithmetic { op, domain };
                        candidates.push(catalog.candidate(graph, LanguageContract::new(arithmetic_authority(op, domain), operation, vec![left, right], result))?);
                    }
                }
                BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                    for (left, right) in [(Atom::Int,Atom::Int),(Atom::Int,Atom::UInt),(Atom::UInt,Atom::Int),(Atom::UInt,Atom::UInt),(Atom::Float,Atom::Float),(Atom::Duration,Atom::Duration),(Atom::Str,Atom::Str)] {
                        let parameters = vec![graph.atom(left)?,graph.atom(right)?]; let result = graph.atom(Atom::Bool)?;
                        candidates.push(catalog.candidate(graph, LanguageContract::new(format!("language.binary.{op:?}"), PreparedLanguageOperation::Ordering { op, left, right }, parameters, result))?);
                    }
                }
                BinaryOp::Eq | BinaryOp::Ne => {
                    let value = graph.fresh(1, origin)?; let result = graph.atom(Atom::Bool)?;
                    let mut contract = LanguageContract::new(format!("language.binary.{op:?}"), PreparedLanguageOperation::Equality { op }, vec![value,value],result);
                    contract.relations[1] = ArgumentRelation::EqualityCompatible;
                    candidates.push(catalog.candidate(graph, contract)?);
                }
                BinaryOp::And | BinaryOp::Or => {
                    let value = graph.atom(Atom::Bool)?;
                    candidates.push(catalog.candidate(graph, LanguageContract::new(format!("language.binary.{op:?}"), PreparedLanguageOperation::Boolean { op }, vec![value,value],value))?);
                }
                BinaryOp::In | BinaryOp::NotIn => {
                    for domain in [MembershipDomain::List,MembershipDomain::Map,MembershipDomain::Str,MembershipDomain::Bytes,MembershipDomain::Record,MembershipDomain::Path { needle:Atom::Str },MembershipDomain::Path { needle:Atom::Path },MembershipDomain::EnvPathList] {
                        let (receiver, needle, requirements) = membership_types(graph, domain, origin)?;
                        let result = graph.atom(Atom::Bool)?; let suffix = membership_name(domain);
                        let mut contract = LanguageContract::new(format!("language.binary.{op:?}.{suffix}"), PreparedLanguageOperation::Membership { negated: op == BinaryOp::NotIn, domain }, vec![receiver,needle],result);
                        contract.receiver = true; contract.parameters[0].label = Name::intern("<receiver>"); contract.requirements = requirements;
                        contract.relations[1] = ArgumentRelation::Assignable;
                        if domain == MembershipDomain::Record { contract.relations[0] = ArgumentRelation::DeclaredErasure; }
                        candidates.push(catalog.candidate(graph, contract)?);
                    }
                }
                BinaryOp::ResultFallback => {
                    for result in [false,true] {
                        let value = graph.fresh(1, origin)?;
                        let left = if result { let error = graph.fresh(1, origin)?; graph.result(value,error)? } else { graph.optional(value)? };
                        let suffix = if result { "Result" } else { "Optional" };
                        let mut contract = LanguageContract::new(format!("language.binary.ResultFallback.{suffix}"), PreparedLanguageOperation::Fallback { result },vec![left,value],value); contract.relations[1] = ArgumentRelation::Assignable;
                        candidates.push(catalog.candidate(graph, contract)?);
                    }
                }
            }
            Ok(candidates)
        })
    }

    pub(crate) fn compound_family(&mut self, graph: &mut InferenceContext, op: AssignOp, origin: Span) -> Result<OperationFamilyId, InferenceError> {
        self.family(graph, format!("assignment.{op:?}"), |catalog, graph| {
            if op == AssignOp::Set {
                let value = graph.fresh(1, origin)?; let mut contract = LanguageContract::new("language.assignment.Set".into(),PreparedLanguageOperation::Assignment,vec![value,value],value);
                contract.relations[1] = ArgumentRelation::Assignable; contract.statement = true;
                return Ok(vec![catalog.candidate(graph, contract)?]);
            }
            let binary = match op { AssignOp::Add => BinaryOp::Add, AssignOp::Sub => BinaryOp::Sub, AssignOp::Mul => BinaryOp::Mul, AssignOp::Div => BinaryOp::Div, AssignOp::Rem => BinaryOp::Rem, AssignOp::Set => unreachable!() };
            let mut candidates = Vec::new();
            for domain in arithmetic_domains(binary, true) {
                let (left,right,result) = arithmetic_types(graph, domain, origin)?;
                // A compound result describes the checked replacement stored in
                // the slot. Signed arithmetic still crosses UInt's storage check.
                let result = if matches!(domain, ArithmeticDomain::Integer { .. }) { left } else { result };
                let mut contract = LanguageContract::new(format!("language.assignment.{op:?}"),PreparedLanguageOperation::Compound { op,domain },vec![left,right],result); contract.statement = true;
                candidates.push(catalog.candidate(graph, contract)?);
            }
            Ok(candidates)
        })
    }

    pub(crate) fn unary_family(&mut self, graph: &mut InferenceContext, op: UnaryOp, _origin: Span) -> Result<OperationFamilyId, InferenceError> {
        self.family(graph, format!("unary.{op:?}"), |catalog,graph| {
            let domains: &[Atom] = match op { UnaryOp::Not => &[Atom::Bool,Atom::Status], UnaryOp::Neg => &[Atom::Int,Atom::UInt,Atom::Float] };
            domains.iter().map(|operand| { let value = graph.atom(*operand)?; let result = graph.atom(match op { UnaryOp::Not => Atom::Bool, UnaryOp::Neg if *operand == Atom::Float => Atom::Float, UnaryOp::Neg => Atom::Int })?;
                catalog.candidate(graph,LanguageContract::new(format!("language.unary.{op:?}"),PreparedLanguageOperation::Unary { op,operand:*operand },vec![value],result)) }).collect()
        })
    }

    pub(crate) fn iteration_family(&mut self, graph: &mut InferenceContext, origin: Span) -> Result<OperationFamilyId, InferenceError> {
        self.family(graph,"iteration".into(),|catalog,graph| catalog.iteration_candidates(graph, origin, &[IterableDomain::List,IterableDomain::Stream,IterableDomain::Map,IterableDomain::Str,IterableDomain::Bytes], &[false,true], "language.iteration"))
    }

    pub(crate) fn unresolved_iteration_family(&mut self, graph: &mut InferenceContext, origin: Span) -> Result<OperationFamilyId, InferenceError> {
        self.family(graph,"iteration.unresolved".into(),|catalog,graph| {
            let full = catalog.iteration_family(graph, origin)?;
            let candidates = graph.family(full)?.to_vec().into_iter().filter(|candidate| {
                !matches!(catalog.candidates[candidate].operation, PreparedLanguageOperation::Iteration { domain: IterableDomain::Map | IterableDomain::Str | IterableDomain::Bytes, outer_result: true })
            }).collect();
            Ok(candidates)
        })
    }

    pub(crate) fn delegation_family(&mut self, graph: &mut InferenceContext, origin: Span) -> Result<OperationFamilyId, InferenceError> {
        self.family(graph,"yield_delegation".into(),|catalog,graph| catalog.iteration_candidates(graph, origin, &[IterableDomain::List,IterableDomain::Stream], &[false], "language.yield_delegation"))
    }

    fn iteration_candidates(&mut self, graph: &mut InferenceContext, origin: Span, domains: &[IterableDomain], wrappers: &[bool], prefix: &str) -> Result<Vec<CandidateId>, InferenceError> {
            let mut candidates = Vec::new();
            for &domain in domains {
                for &outer_result in wrappers {
                    let (receiver,item,requirements) = iterable_types(graph,domain,origin)?;
                    let receiver = if outer_result { let error = graph.fresh(1,origin)?; graph.result(receiver,error)? } else { receiver };
                    let name = match domain { IterableDomain::List => "List",IterableDomain::Stream => "Stream",IterableDomain::Map => "Map",IterableDomain::Str => "Str",IterableDomain::Bytes => "Bytes" };
                    let authority_label = format!("{prefix}.{name}{}",if outer_result { ".Result" } else { "" });
                    let operation = PreparedLanguageOperation::Iteration { domain,outer_result };
                    let execution = EffectSummary::Variable(graph.fresh_derived_effect_at(1,None)?);
                    let reason = graph.reason(origin,None)?;
                    if outer_result { graph.include_effects(EffectSummary::Closed(EffectSet::ERROR),execution,reason)?; }
                    let roles = [EffectRole::Pull { source:0 },EffectRole::Close { source:0 },EffectRole::PullProjection { source:0,projection:EffectProjection::ResultSuccess },EffectRole::CloseProjection { source:0,projection:EffectProjection::ResultSuccess }].map(|role| {
                        let projected = matches!(role,EffectRole::PullProjection { .. } | EffectRole::CloseProjection { .. });
                        let effect = if domain == IterableDomain::Stream && projected == outer_result { EffectSummary::Variable(graph.fresh_effect_at(1,None)?) } else { EffectSummary::Closed(EffectSet::EMPTY) };
                        graph.include_effects(effect,execution,reason)?;
                        Ok((role,effect))
                    }).into_iter().collect::<Result<Vec<_>,InferenceError>>()?;
                    let arrow = graph.arrow(Arrow { kind:CallableKind::Pure,params:vec![Parameter { label:Name::intern("operand0"),ty:receiver,defaulted:false,rest:false }],result:item,effects:execution })?;
                    let roots: Vec<_> = roles.iter().map(|(_,summary)|*summary).collect();
                    let scheme = graph.generalize_with_effect_roots(arrow,0,Generalization::Allowed,&requirements,&roots)?;
                    let effect_roles = roles.iter().zip(&graph.scheme(scheme)?.effect_roots).map(|((role,_),summary)| match summary {
                        EffectSummary::Rigid { scope,index } if *scope == scheme => Ok((*role,EffectRoleReference::Binder(*index))),
                        EffectSummary::Closed(bits) => Ok((*role,EffectRoleReference::Fixed(*bits))),
                        _ => Err(InferenceError::InvalidScheme),
                    }).collect::<Result<Vec<_>,_>>()?;
                    let identity = Name::intern(&format!("{}:{operation:?}",authority(&authority_label)?.id));
                    let failure_projection = (outer_result && matches!(domain, IterableDomain::Map | IterableDomain::Str | IterableDomain::Bytes)).then_some(OperationFailureProjection::ArgumentResultError { argument: 0 });
                    let candidate = graph.register_candidate(CandidateTemplate { failure_projection, identity,public_label:Name::intern(&authority_label),effect_roles,output_effect_roles:Vec::new(),scheme,has_receiver:false,actual_eligibility:Vec::new(),argument_relations:vec![ArgumentRelation::Exact] })?;
                    self.candidates.insert(candidate,OperationCandidate { identity,scheme,authority:authority(&authority_label)?.id,operation,argument_order:OperationArgumentOrder::SourceOrder,statement_result_is_unit:false });
                    candidates.push(candidate);
                }
            }
            Ok(candidates)
    }

    pub(crate) fn index_family(&mut self, graph: &mut InferenceContext, origin: Span) -> Result<OperationFamilyId, InferenceError> {
        self.family(graph,"index".into(),|catalog,graph| {
            let mut candidates = Vec::new();
            for map in [false,true] {
                let value = graph.fresh(1,origin)?;
                let (receiver,index,requirements) = if map { let key = graph.fresh(1,origin)?; let receiver = graph.map(key,value)?; let reason = graph.reason(origin,None)?; (receiver,key,vec![graph.require_eligibility(Eligibility::MapKey,key,reason)?]) } else { (graph.list(value)?,graph.atom(Atom::Int)?,Vec::new()) };
                let mut contract = LanguageContract::new(format!("language.index.{}",if map { "Map" } else { "List" }),PreparedLanguageOperation::Index { map },vec![receiver,index],value); contract.requirements = requirements; contract.relations[1] = ArgumentRelation::Assignable;
                candidates.push(catalog.candidate(graph,contract)?);
            }
            Ok(candidates)
        })
    }

    pub(crate) fn constant_key_index_family(&mut self, graph: &mut InferenceContext, field: Name, origin: Span) -> Result<OperationFamilyId, InferenceError> {
        self.family(graph, format!("index.constant.{field}"), |catalog, graph| {
            let index = catalog.index_family(graph, origin)?;
            graph.charge_source_fact_work(graph.family(index)?.len() as u64)?;
            let mut candidates = graph.family(index)?.to_vec();
            let value = graph.fresh(1, origin)?;
            let tail = graph.fresh_row(1, origin)?;
            let row = graph.row(vec![RowField { label: field, ty: value }], Some(tail))?;
            let receiver = graph.record(row)?;
            let key = graph.atom(Atom::Str)?;
            let contract = LanguageContract::new("language.projection.constant_key".into(), PreparedLanguageOperation::ConstantKeyProjection { field }, vec![receiver, key], value);
            candidates.push(catalog.candidate(graph, contract)?);
            Ok(candidates)
        })
    }

    pub(crate) fn slice_family(&mut self, graph: &mut InferenceContext, domain: SliceDomain, origin: Span) -> Result<OperationFamilyId, InferenceError> {
        self.family(graph,format!("slice.{domain:?}"),|catalog,graph| {
            let receiver = match domain { SliceDomain::List => { let item = graph.fresh(1,origin)?; graph.list(item)? },SliceDomain::Str => graph.atom(Atom::Str)?,SliceDomain::Bytes => graph.atom(Atom::Bytes)? };
            let bound = graph.atom(Atom::Int)?;
            let mut contract = LanguageContract::new(format!("language.slice.{domain:?}"),PreparedLanguageOperation::Slice(domain),vec![receiver,bound,bound],receiver);
            for parameter in &mut contract.parameters[1..] { parameter.defaulted = true; }
            contract.relations[1..].fill(ArgumentRelation::Assignable);
            Ok(vec![catalog.candidate(graph,contract)?])
        })
    }

    pub(crate) fn all_slice_family(&mut self, graph: &mut InferenceContext, origin: Span) -> Result<OperationFamilyId, InferenceError> {
        self.family(graph, "slice".into(), |catalog, graph| {
            let mut candidates = Vec::new();
            for domain in [SliceDomain::List, SliceDomain::Str, SliceDomain::Bytes] {
                let family = catalog.slice_family(graph, domain, origin)?;
                candidates.extend_from_slice(graph.family(family)?);
            }
            Ok(candidates)
        })
    }

    pub(crate) fn constructor_family(&mut self, graph: &mut InferenceContext, kind: ValueConstructor, origin: Span) -> Result<OperationFamilyId, InferenceError> {
        self.family(graph,format!("constructor.{kind:?}"),|catalog,graph| {
            let unit = graph.atom(Atom::Unit)?;
            let contracts = match kind {
                ValueConstructor::Ok => {
                    let empty_error = graph.fresh(1,origin)?;
                    let value = graph.fresh(1,origin)?; let error = graph.fresh(1,origin)?;
                    vec![
                        LanguageContract::new("language.constructor.Ok".into(),PreparedLanguageOperation::Constructor { kind,arity:0 },Vec::new(),graph.result(unit,empty_error)?),
                        LanguageContract::new("language.constructor.Ok".into(),PreparedLanguageOperation::Constructor { kind,arity:1 },vec![value],graph.result(value,error)?),
                    ]
                },
                ValueConstructor::Err => {
                    let value = graph.fresh(1,origin)?; let error = graph.fresh(1,origin)?;
                    let reason = graph.reason(origin,None)?;
                    let mut caused = LanguageContract::new("language.constructor.Err".into(),PreparedLanguageOperation::Constructor { kind,arity:2 },vec![error,graph.atom(Atom::Error)?],graph.result(value,error)?);
                    caused.requirements.push(graph.require_eligibility(Eligibility::Error,error,reason)?);
                    caused.relations[1] = ArgumentRelation::Assignable;
                    let data_value = graph.fresh(1,origin)?; let data_error = graph.fresh(1,origin)?;
                    vec![LanguageContract::new("language.constructor.Err".into(),PreparedLanguageOperation::Constructor { kind,arity:1 },vec![data_error],graph.result(data_value,data_error)?),caused]
                },
                ValueConstructor::Path => vec![LanguageContract::new("language.constructor.Path".into(),PreparedLanguageOperation::Constructor { kind,arity:1 },vec![graph.atom(Atom::Str)?],graph.atom(Atom::Path)?)],
                ValueConstructor::Range => { let bound = graph.atom(Atom::Int)?; let result = graph.stream(bound)?; (1..=2).map(|arity| LanguageContract::new("language.constructor.range".into(),PreparedLanguageOperation::Constructor { kind,arity },vec![bound;arity],result)).collect() },
            };
            contracts.into_iter().map(|contract| catalog.candidate(graph,contract)).collect()
        })
    }

    pub(crate) fn error_field_family(&mut self, graph: &mut InferenceContext, receiver: Atom, field: Name) -> Result<OperationFamilyId, InferenceError> {
        if field != "message" || !matches!(receiver, Atom::Error | Atom::ProcessError | Atom::ErrorFamily(_) | Atom::ErrorVariant { .. } | Atom::ErrorFacet(_)) {
            return Err(InferenceError::Boundary("error message selection requires an exact checked error receiver"));
        }
        self.family(graph, format!("error_field.{receiver:?}.{field}"), |catalog, graph| {
            let input = graph.atom(receiver)?;
            let output = graph.atom(Atom::Str)?;
            let contract = LanguageContract::new("language.projection.error_message".into(), PreparedLanguageOperation::ErrorField { receiver, field }, vec![input], output);
            Ok(vec![catalog.candidate(graph, contract)?])
        })
    }

    pub(crate) fn declaration_family(&mut self, graph: &mut InferenceContext, authority_id: &'static str, identity: Name, scheme: SchemeId) -> Result<OperationFamilyId,InferenceError> {
        let descriptor = authority(authority_id)?;
        if descriptor.disposition != LanguageDisposition::ParametricTemplate { return Err(InferenceError::Boundary("declaration authority is not relational")); }
        self.family(graph,format!("declaration.{authority_id}.{}",identity.as_str()),|catalog,graph| {
            let operation = PreparedLanguageOperation::Declaration { authority:authority_id,identity };
            let public_label = identity;
            let identity = Name::intern(&format!("{authority_id}:{}",identity.as_str()));
            let id = graph.register_candidate(CandidateTemplate { failure_projection: None, output_effect_roles:Vec::new(), public_label, effect_roles: Vec::new(), identity,scheme,has_receiver:false,actual_eligibility:Vec::new(),argument_relations:Vec::new() })?;
            catalog.candidates.insert(id,OperationCandidate { identity,scheme,authority:authority_id,operation,argument_order:OperationArgumentOrder::SourceOrder,statement_result_is_unit:false }); Ok(vec![id])
        })
    }

    pub(crate) fn display_family(&mut self, graph: &mut InferenceContext, stderr: bool, arity: usize, origin: Span) -> Result<OperationFamilyId, InferenceError> {
        self.family(graph, format!("display.{stderr}.{arity}"), |catalog, graph| {
            let mut arguments = Vec::with_capacity(arity);
            let mut requirements = Vec::with_capacity(arity);
            let reason = graph.reason(origin, None)?;
            for _ in 0..arity {
                let value = graph.fresh(1, origin)?;
                arguments.push(value);
                requirements.push(graph.require_eligibility(Eligibility::Display, value, reason)?);
            }
            let result = graph.atom(Atom::Unit)?;
            let mut contract = LanguageContract::new(if stderr { "language.command.eprint" } else { "language.command.print" }.into(), PreparedLanguageOperation::Display { stderr }, arguments, result);
            contract.requirements = requirements;
            contract.statement = true;
            Ok(vec![catalog.candidate(graph, contract)?])
        })
    }

    pub(crate) fn wait_family(&mut self, graph: &mut InferenceContext, _origin: Span) -> Result<OperationFamilyId, InferenceError> {
        self.family(graph, "wait".into(), |catalog, graph| {
            let handle = graph.atom(Atom::ProcessHandle)?;
            let status = graph.atom(Atom::Status)?;
            let error = graph.atom(Atom::ProcessError)?;
            let erased = graph.atom(Atom::Any)?;
            let mut candidates = Vec::with_capacity(3);
            for (list, erased_input) in [(false, false), (true, false), (false, true)] {
                let input = if erased_input { erased } else if list { graph.list(handle)? } else { handle };
                let success = if list { graph.list(status)? } else { status };
                let result = graph.result(success, error)?;
                let mut contract = LanguageContract::new("language.wait".into(), PreparedLanguageOperation::Wait { list, erased: erased_input }, vec![input], result);
                contract.effects = EffectSummary::Closed(EffectSet::PROCESS);
                candidates.push(catalog.candidate(graph, contract)?);
            }
            Ok(candidates)
        })
    }
}

pub(crate) fn run_result_type(graph: &mut InferenceContext, kind: RunKind, propagate: bool) -> Result<TypeId, InferenceError> {
    let value = match kind {
        RunKind::Plain | RunKind::Status => return graph.atom(Atom::Status),
        RunKind::CaptureText => graph.atom(Atom::Str)?,
        RunKind::CaptureBytes => graph.atom(Atom::Bytes)?,
        RunKind::CaptureTextRecord | RunKind::CaptureBytesRecord => {
            let output = graph.atom(if kind == RunKind::CaptureTextRecord { Atom::Str } else { Atom::Bytes })?;
            let status = graph.atom(Atom::Status)?;
            let row = graph.row(vec![crate::sema::inference::RowField { label: Name::intern("status"), ty: status },
                crate::sema::inference::RowField { label: Name::intern("stdout"), ty: output }, crate::sema::inference::RowField { label: Name::intern("stderr"), ty: output }], None)?;
            graph.record(row)?
        }
        RunKind::StreamText | RunKind::StreamBytes => {
            let item = graph.atom(if kind == RunKind::StreamText { Atom::Str } else { Atom::Bytes })?;
            graph.stream(item)?
        }
    };
    if propagate { Ok(value) } else { let error = graph.atom(Atom::ProcessError)?; graph.result(value, error) }
}

struct LanguageContract {
    authority: String, operation: PreparedLanguageOperation, parameters: Vec<Parameter>, result: TypeId,
    receiver: bool, statement: bool, relations: Vec<ArgumentRelation>, requirements: Vec<RequirementId>, effects: EffectSummary,
}
impl LanguageContract {
    fn new(authority:String,operation:PreparedLanguageOperation,parameters:Vec<TypeId>,result:TypeId)->Self {
        let relations = vec![ArgumentRelation::Exact;parameters.len()];
        let parameters = parameters.into_iter().enumerate().map(|(index,ty)| Parameter { label:Name::intern(&format!("operand{index}")),ty,defaulted:false,rest:false }).collect();
        Self { authority,operation,parameters,result,receiver:false,statement:false,relations,requirements:Vec::new(),effects:EffectSummary::Closed(EffectSet::EMPTY) }
    }
}
fn authority(id:&str)->Result<&'static LanguageAuthority,InferenceError> { LANGUAGE_AUTHORITIES.iter().chain(std::iter::once(&ERROR_FIELD_AUTHORITY)).find(|authority|authority.id==id).ok_or(InferenceError::Boundary("unknown frozen language authority")) }

fn arithmetic_domains(op:BinaryOp,compound:bool)->Vec<ArithmeticDomain> {
    let mut domains = Vec::new();
    for left in [Atom::Int,Atom::UInt] { for right in [Atom::Int,Atom::UInt] { domains.push(ArithmeticDomain::Integer { left,right }); } }
    if op != BinaryOp::Rem { domains.push(ArithmeticDomain::Float); }
    if matches!(op,BinaryOp::Add|BinaryOp::Sub) { domains.push(ArithmeticDomain::DurationPair); }
    if matches!(op,BinaryOp::Mul|BinaryOp::Div) { domains.push(ArithmeticDomain::DurationScale { duration_left:true }); }
    if op == BinaryOp::Mul && !compound { domains.push(ArithmeticDomain::DurationScale { duration_left:false }); }
    if op == BinaryOp::Div && !compound { domains.push(ArithmeticDomain::DurationRatio); }
    if op == BinaryOp::Add { domains.push(ArithmeticDomain::List); if !compound { domains.push(ArithmeticDomain::Text); } }
    if op == BinaryOp::Div && compound { for right in [Atom::Str,Atom::Path] { domains.push(ArithmeticDomain::PathJoin { right }); } }
    domains
}
fn arithmetic_authority(op:BinaryOp,domain:ArithmeticDomain)->String {
    let suffix = match domain { ArithmeticDomain::Integer { .. } => "integer",ArithmeticDomain::Float => "float",ArithmeticDomain::Text => "text",ArithmeticDomain::List => "list",ArithmeticDomain::DurationPair => "duration",ArithmeticDomain::DurationScale { duration_left:true } => "duration_scale",ArithmeticDomain::DurationScale { duration_left:false } => "duration_scale_reverse",ArithmeticDomain::DurationRatio => "duration_ratio",ArithmeticDomain::PathJoin { .. } => "path" };
    format!("language.binary.{op:?}.{suffix}")
}
fn arithmetic_types(graph:&mut InferenceContext,domain:ArithmeticDomain,origin:Span)->Result<(TypeId,TypeId,TypeId),InferenceError> {
    let atoms = match domain {
        ArithmeticDomain::Integer { left,right } => (left,right,Atom::Int),ArithmeticDomain::Float => (Atom::Float,Atom::Float,Atom::Float),ArithmeticDomain::Text => (Atom::Str,Atom::Str,Atom::Str),ArithmeticDomain::DurationPair => (Atom::Duration,Atom::Duration,Atom::Duration),ArithmeticDomain::DurationScale { duration_left:true } => (Atom::Duration,Atom::Int,Atom::Duration),ArithmeticDomain::DurationScale { duration_left:false } => (Atom::Int,Atom::Duration,Atom::Duration),ArithmeticDomain::DurationRatio => (Atom::Duration,Atom::Duration,Atom::Int),ArithmeticDomain::PathJoin { right } => (Atom::Path,right,Atom::Path),
        ArithmeticDomain::List => { let item = graph.fresh(1,origin)?; let list = graph.list(item)?; return Ok((list,list,list)); }
    };
    Ok((graph.atom(atoms.0)?,graph.atom(atoms.1)?,graph.atom(atoms.2)?))
}
fn membership_name(domain:MembershipDomain)->&'static str { match domain { MembershipDomain::List => "List",MembershipDomain::Map => "Map",MembershipDomain::Str => "Str",MembershipDomain::Bytes => "Bytes",MembershipDomain::Record => "Record",MembershipDomain::Path { .. } => "Path",MembershipDomain::EnvPathList => "EnvPathList" } }
fn membership_types(graph:&mut InferenceContext,domain:MembershipDomain,origin:Span)->Result<(TypeId,TypeId,Vec<RequirementId>),InferenceError> {
    let mut requirements = Vec::new();
    let (receiver,needle) = match domain {
        MembershipDomain::List => { let item = graph.fresh(1,origin)?; (graph.list(item)?,item) },
        MembershipDomain::Map => { let key = graph.fresh(1,origin)?; let value = graph.fresh(1,origin)?; let reason = graph.reason(origin,None)?; requirements.push(graph.require_eligibility(Eligibility::MapKey,key,reason)?); (graph.map(key,value)?,key) },
        MembershipDomain::Str => (graph.atom(Atom::Str)?,graph.atom(Atom::Str)?),MembershipDomain::Bytes => (graph.atom(Atom::Bytes)?,graph.atom(Atom::Bytes)?),MembershipDomain::Record => (graph.atom(Atom::ErasedRecord)?,graph.atom(Atom::Str)?),MembershipDomain::Path { needle } => (graph.atom(Atom::Path)?,graph.atom(needle)?),MembershipDomain::EnvPathList => (graph.atom(Atom::EnvPathList)?,graph.atom(Atom::Path)?),
    };
    Ok((receiver,needle,requirements))
}
fn iterable_types(graph:&mut InferenceContext,domain:IterableDomain,origin:Span)->Result<(TypeId,TypeId,Vec<RequirementId>),InferenceError> {
    let mut requirements = Vec::new();
    let (receiver,item) = match domain {
        IterableDomain::List => { let item = graph.fresh(1,origin)?; (graph.list(item)?,item) },IterableDomain::Stream => { let item = graph.fresh(1,origin)?; (graph.stream(item)?,item) },
        IterableDomain::Map => { let key = graph.fresh(1,origin)?; let value = graph.fresh(1,origin)?; let reason = graph.reason(origin,None)?; requirements.push(graph.require_eligibility(Eligibility::MapKey,key,reason)?); let row = graph.row(vec![RowField { label:Name::intern("key"),ty:key },RowField { label:Name::intern("value"),ty:value }],None)?; (graph.map(key,value)?,graph.record(row)?) },
        IterableDomain::Str => (graph.atom(Atom::Str)?,graph.atom(Atom::Str)?),IterableDomain::Bytes => (graph.atom(Atom::Bytes)?,graph.atom(Atom::Int)?),
    }; Ok((receiver,item,requirements))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::inference::OperationCall;
    use crate::source::SourceId;
    fn span() -> Span { Span::new(SourceId::new(0), 0, 1) }

    #[test]
    fn result_constructor_overloads_preserve_unit_and_independent_error_domains() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default(); let mut catalog = OperationGraph::default();
        let why = graph.reason(span(), None).unwrap();
        let unit = graph.atom(Atom::Unit).unwrap(); let error = graph.atom(Atom::Error).unwrap();
        let int = graph.atom(Atom::Int).unwrap(); let string = graph.atom(Atom::Str).unwrap();
        let family = catalog.constructor_family(&mut graph, ValueConstructor::Ok, span()).unwrap();
        let result = graph.result(unit, error).unwrap();
        let requirement = graph.require_operation(family, call(vec![], result), why).unwrap();
        graph.solve().unwrap();
        let candidate = graph.candidate_evidence(requirement).unwrap().unwrap().candidate;
        assert_eq!(catalog.metadata(&graph, candidate).unwrap().operation, PreparedLanguageOperation::Constructor { kind: ValueConstructor::Ok, arity: 0 });
        assert!(graph.trial(|graph| { graph.require_operation(family, call(vec![unit, unit], result), why)?; graph.solve() }).is_err());

        let family = catalog.constructor_family(&mut graph, ValueConstructor::Err, span()).unwrap();
        let outer = graph.atom(Atom::ErrorFamily(Name::intern("AssertionError"))).unwrap();
        let cause = graph.atom(Atom::ErrorFamily(Name::intern("FsError"))).unwrap();
        let result = graph.result(int, outer).unwrap();
        let requirement = graph.require_operation(family, call(vec![outer, cause], result), why).unwrap();
        graph.solve().unwrap();
        assert_eq!(graph.candidate_evidence(requirement).unwrap().unwrap().result, result);
        let data_error = graph.result(int, string).unwrap();
        assert!(graph.trial(|graph| { graph.require_operation(family, call(vec![string, cause], data_error), why)?; graph.solve() }).is_err());
        assert!(graph.trial(|graph| { graph.require_operation(family, call(vec![outer, string], result), why)?; graph.solve() }).is_err());
        let requirement = graph.require_operation(family, call(vec![string], data_error), why).unwrap();
        graph.solve().unwrap();
        assert_eq!(catalog.metadata(&graph, graph.candidate_evidence(requirement).unwrap().unwrap().candidate).unwrap().operation, PreparedLanguageOperation::Constructor { kind: ValueConstructor::Err, arity: 1 });
    }

    #[test]
    fn duration_operation_domains_preserve_asymmetry_and_uint_rejection() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default(); let mut catalog = OperationGraph::default(); let why = graph.reason(span(), None).unwrap();
        let duration = graph.atom(Atom::Duration).unwrap(); let int = graph.atom(Atom::Int).unwrap(); let uint = graph.atom(Atom::UInt).unwrap();
        let family = catalog.binary_family(&mut graph, BinaryOp::Mul, span()).unwrap();
        for arguments in [[duration, int], [int, duration]] {
            let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings:Vec::new(), effect_bindings: Vec::new(), receiver: None, arguments: arguments.into_iter().map(Some).collect(), result: duration, effects: EffectSummary::Closed(EffectSet::EMPTY) }, why).unwrap();
            graph.solve().unwrap(); assert!(graph.candidate_evidence(requirement).unwrap().is_some());
        }
        assert!(graph.trial(|graph| { graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings:Vec::new(), effect_bindings: Vec::new(), receiver: None, arguments: vec![Some(duration), Some(uint)], result: duration, effects: EffectSummary::Closed(EffectSet::EMPTY) }, why)?; graph.solve() }).is_err());
        let family = catalog.binary_family(&mut graph, BinaryOp::Div, span()).unwrap();
        let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings:Vec::new(), effect_bindings: Vec::new(), receiver: None, arguments: vec![Some(duration), Some(duration)], result: int, effects: EffectSummary::Closed(EffectSet::EMPTY) }, why).unwrap();
        graph.solve().unwrap(); assert!(graph.candidate_evidence(requirement).unwrap().is_some());
    }

    fn call(arguments: Vec<TypeId>, result: TypeId) -> OperationCall { OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings:Vec::new(), effect_bindings: Vec::new(), receiver: None, arguments: arguments.into_iter().map(Some).collect(), result, effects: EffectSummary::Closed(EffectSet(127)) } }

    #[test]
    fn language_catalog_reconciles_every_frozen_nonstage_authority() {
        let inventory = crate::modules::json::parse_raw_json(include_str!("../../bench/typing/operations.json")).unwrap();
        let miniserde::json::Value::Object(inventory) = inventory else { panic!() };
        let miniserde::json::Value::Array(entries) = inventory.get("operations").unwrap() else { panic!() };
        let expected: BTreeMap<_,_> = entries.iter().filter_map(|entry| { let miniserde::json::Value::Object(entry) = entry else { return None }; let miniserde::json::Value::String(id) = entry.get("id")? else { return None }; if !id.starts_with("language.") || id.starts_with("language.stage.") { return None } let miniserde::json::Value::String(class) = entry.get("classification")? else { return None }; Some((id.as_str(),class.as_str())) }).collect();
        assert_eq!(expected.len(),125); assert_eq!(LANGUAGE_AUTHORITIES.len(),125);
        let mut observed = BTreeMap::new();
        for authority in LANGUAGE_AUTHORITIES {
            let class = match authority.disposition { LanguageDisposition::SealedRequirement => "sealed_requirement",LanguageDisposition::ParametricTemplate => "parametric_template",LanguageDisposition::FixedBoundary => "monomorphic",LanguageDisposition::DynamicBoundary => "dynamic_boundary",LanguageDisposition::StaticIdentity => "non_generalizable" };
            assert!(observed.insert(authority.id,class).is_none()); assert!(!authority.family.is_empty());
        }
        assert_eq!(observed,expected);
    }

    #[test]
    fn error_field_authority_is_canonical_without_rewriting_the_frozen_inventory() {
        assert_eq!(LANGUAGE_AUTHORITIES.len(), 125);
        assert!(!LANGUAGE_AUTHORITIES.iter().any(|entry| entry.id == ERROR_FIELD_AUTHORITY.id));
        let selected = authority("language.projection.error_message").unwrap();
        assert_eq!(selected.id, ERROR_FIELD_AUTHORITY.id);
        assert_eq!(selected.family, "error_field");
        assert_eq!(selected.disposition, LanguageDisposition::FixedBoundary);
    }

    #[test]
    fn every_operator_candidate_proves_a_ground_contract_and_rejects_extra_arguments() {
        use crate::sema::inference::TypeNode;
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default(); let mut catalog = OperationGraph::default();
        for op in [BinaryOp::Add,BinaryOp::Sub,BinaryOp::Mul,BinaryOp::Div,BinaryOp::Rem,BinaryOp::Eq,BinaryOp::Ne,BinaryOp::Lt,BinaryOp::Le,BinaryOp::Gt,BinaryOp::Ge,BinaryOp::In,BinaryOp::NotIn,BinaryOp::And,BinaryOp::Or,BinaryOp::ResultFallback] { catalog.binary_family(&mut graph,op,span()).unwrap(); }
        for op in [AssignOp::Set,AssignOp::Add,AssignOp::Sub,AssignOp::Mul,AssignOp::Div,AssignOp::Rem] { catalog.compound_family(&mut graph,op,span()).unwrap(); }
        for op in [UnaryOp::Not,UnaryOp::Neg] { catalog.unary_family(&mut graph,op,span()).unwrap(); }
        catalog.iteration_family(&mut graph,span()).unwrap(); catalog.index_family(&mut graph,span()).unwrap();
        for domain in [SliceDomain::List,SliceDomain::Str,SliceDomain::Bytes] { catalog.slice_family(&mut graph,domain,span()).unwrap(); }
        for kind in [ValueConstructor::Ok,ValueConstructor::Err,ValueConstructor::Path,ValueConstructor::Range] { catalog.constructor_family(&mut graph,kind,span()).unwrap(); }
        assert!(catalog.candidates.len()>125);
        let candidates: Vec<_> = catalog.candidates.keys().copied().collect(); let why = graph.reason(span(),None).unwrap(); let ground = graph.atom(Atom::Str).unwrap();
        for candidate in candidates {
            let metadata = catalog.metadata(&graph,candidate).unwrap().clone();
            let template = graph.candidate(candidate).unwrap().clone(); let instance = graph.instantiate(template.scheme,1,why).unwrap();
            let error_indices: std::collections::BTreeSet<_> = graph.scheme(template.scheme).unwrap().requirements.iter().filter_map(|requirement| match requirement {
                crate::sema::inference::RequirementTemplate::Eligibility { predicate: Eligibility::Error, ty } => match graph.node(*ty).unwrap() { TypeNode::Rigid { index, .. } => Some(*index as usize), _ => None },
                _ => None,
            }).collect();
            for (index, substitution) in instance.substitutions.iter().enumerate() {
                let operand = if error_indices.contains(&index) { graph.atom(Atom::Error).unwrap() } else { ground };
                graph.unify(*substitution,operand,why).unwrap();
            }
            let TypeNode::Arrow(signature) = graph.node(graph.resolved(instance.ty).unwrap()).unwrap() else { panic!() }; let signature = signature.clone();
            let offset = usize::from(template.has_receiver); let receiver = template.has_receiver.then(|| signature.params[0].ty);
            let arguments: Vec<_> = signature.params[offset..].iter().map(|parameter|Some(parameter.ty)).collect();
            let family = graph.register_family(&[candidate]).unwrap();
            let effect_bindings: Vec<_> = template.effect_roles.iter().map(|(role,_)|(*role,EffectSummary::Closed(EffectSet::EMPTY))).collect();
            let valid = OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings:Vec::new(), effect_bindings: effect_bindings.clone(), receiver,arguments:arguments.clone(),result:signature.result,effects:EffectSummary::Closed(EffectSet(127)) };
            let requirement = graph.require_operation(family,valid,why).unwrap(); graph.solve().unwrap_or_else(|error|panic!("{:?}: {error:?}",metadata.operation));
            assert_eq!(graph.candidate_evidence(requirement).unwrap().unwrap().candidate,candidate);
            assert!(graph.trial(|graph| { let mut arguments = arguments.clone(); arguments.push(Some(ground)); graph.require_operation(family,OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings:Vec::new(), effect_bindings: effect_bindings.clone(), receiver,arguments,result:signature.result,effects:EffectSummary::Closed(EffectSet(127)) },why)?; graph.solve() }).is_err(),"{:?}",metadata.operation);
        }
    }

    #[test]
    fn iteration_candidates_retain_exact_pull_close_and_result_permissions() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default(); let mut catalog = OperationGraph::default();
        let family = catalog.iteration_family(&mut graph,span()).unwrap();
        assert_eq!(graph.family(family).unwrap().len(), 10);
        let unresolved = catalog.unresolved_iteration_family(&mut graph,span()).unwrap();
        assert_eq!(graph.family(unresolved).unwrap().len(), 7);
        for candidate in graph.family(unresolved).unwrap() {
            assert!(!matches!(catalog.metadata(&graph,*candidate).unwrap().operation, PreparedLanguageOperation::Iteration { domain: IterableDomain::Map | IterableDomain::Str | IterableDomain::Bytes, outer_result: true }));
        }
        let delegation = catalog.delegation_family(&mut graph,span()).unwrap();
        assert_eq!(graph.family(delegation).unwrap().len(), 2);
        for candidate in graph.family(delegation).unwrap() {
            assert!(matches!(catalog.metadata(&graph,*candidate).unwrap().operation, PreparedLanguageOperation::Iteration { domain: IterableDomain::List | IterableDomain::Stream, outer_result: false }));
        }
        let why = graph.reason(span(),None).unwrap();
        let item = graph.atom(Atom::Int).unwrap();
        for domain in [IterableDomain::List,IterableDomain::Stream] {
            for outer_result in [false,true] {
                let receiver = if domain == IterableDomain::Stream { graph.stream(item).unwrap() } else { graph.list(item).unwrap() };
                let receiver = if outer_result { let error = graph.atom(Atom::Error).unwrap(); graph.result(receiver,error).unwrap() } else { receiver };
                let (pull,close) = if domain == IterableDomain::Stream { (EffectSet::TIME,EffectSet::ENV) } else { (EffectSet::EMPTY,EffectSet::EMPTY) };
                let effects = EffectSummary::Variable(graph.fresh_execution_effect(None).unwrap());
                let requirement = graph.require_operation(family,OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver:None,arguments:vec![Some(receiver)],result:item,effects,effect_bindings:vec![(EffectRole::Pull { source:0 },EffectSummary::Closed(if outer_result { EffectSet::EMPTY } else { pull })),(EffectRole::Close { source:0 },EffectSummary::Closed(if outer_result { EffectSet::EMPTY } else { close })),(EffectRole::PullProjection { source:0,projection:EffectProjection::ResultSuccess },EffectSummary::Closed(if outer_result { pull } else { EffectSet::EMPTY })),(EffectRole::CloseProjection { source:0,projection:EffectProjection::ResultSuccess },EffectSummary::Closed(if outer_result { close } else { EffectSet::EMPTY }))],output_effect_bindings:Vec::new() },why).unwrap();
                graph.solve().unwrap(); graph.seal_derived_effects(&[effects]).unwrap(); graph.solve().unwrap();
                let evidence = graph.candidate_evidence(requirement).unwrap().unwrap();
                assert_eq!(catalog.metadata(&graph,evidence.candidate).unwrap().operation,PreparedLanguageOperation::Iteration { domain,outer_result });
                assert_eq!(graph.closed_effect_summary(effects).unwrap(),EffectSummary::Closed(EffectSet(pull.0 | close.0 | if outer_result { EffectSet::ERROR.0 } else { 0 })));
            }
        }
    }

    #[test]
    fn arithmetic_candidates_match_all_finite_operand_pairs_without_erasure() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default(); let mut catalog = OperationGraph::default(); let why = graph.reason(span(),None).unwrap();
        let domains = [Atom::Int,Atom::UInt,Atom::Float,Atom::Duration,Atom::Str,Atom::Bool,Atom::Any];
        for op in [BinaryOp::Add,BinaryOp::Sub,BinaryOp::Mul,BinaryOp::Div,BinaryOp::Rem] {
            let family = catalog.binary_family(&mut graph,op,span()).unwrap();
            for left in domains { for right in domains {
                let expected = if matches!(left,Atom::Int|Atom::UInt)&&matches!(right,Atom::Int|Atom::UInt) { Some(Atom::Int) }
                    else if left==Atom::Float&&right==Atom::Float&&op!=BinaryOp::Rem { Some(Atom::Float) }
                    else if left==Atom::Duration&&right==Atom::Duration&&matches!(op,BinaryOp::Add|BinaryOp::Sub) { Some(Atom::Duration) }
                    else if left==Atom::Duration&&right==Atom::Duration&&op==BinaryOp::Div { Some(Atom::Int) }
                    else if left==Atom::Duration&&right==Atom::Int&&matches!(op,BinaryOp::Mul|BinaryOp::Div) { Some(Atom::Duration) }
                    else if left==Atom::Int&&right==Atom::Duration&&op==BinaryOp::Mul { Some(Atom::Duration) }
                    else if left==Atom::Str&&right==Atom::Str&&op==BinaryOp::Add { Some(Atom::Str) } else { None };
                let left_ty = graph.atom(left).unwrap(); let right_ty = graph.atom(right).unwrap();
                let result = graph.trial(|graph| { let result = if let Some(atom)=expected { graph.atom(atom)? } else { graph.fresh(1,span())? }; let requirement = graph.require_operation(family,call(vec![left_ty,right_ty],result),why)?; graph.solve()?; if graph.candidate_evidence(requirement)?.is_none() { return Err(InferenceError::Boundary("finite operation remains ambiguous")); } Ok(()) });
                assert_eq!(result.is_ok(),expected.is_some(),"{left:?} {op:?} {right:?}: {result:?}");
            } }
        }
    }

    #[test]
    fn membership_binds_receiver_first_and_keeps_container_invariance() {
        use crate::sema::types::Type;
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default(); let mut catalog = OperationGraph::default(); let why = graph.reason(span(),None).unwrap();
        let family = catalog.binary_family(&mut graph,BinaryOp::In,span()).unwrap(); let boolean = graph.atom(Atom::Bool).unwrap();
        for (receiver,needle,valid) in [(Type::List(Box::new(Type::UInt)),Type::Int,true),(Type::List(Box::new(Type::List(Box::new(Type::UInt)))),Type::List(Box::new(Type::Int)),false),(Type::Map(Box::new(Type::Str),Box::new(Type::Bool)),Type::Str,true),(Type::Map(Box::new(Type::Float),Box::new(Type::Bool)),Type::Float,false),(Type::Str,Type::Bytes,false),(Type::Bytes,Type::Bytes,true),(Type::EnvPathList,Type::Path,true),(Type::EnvPathList,Type::Str,false)] {
            let receiver = graph.import_type(&receiver,1,span()).unwrap(); let needle = graph.import_type(&needle,1,span()).unwrap();
            let outcome = graph.trial(|graph| { let requirement = graph.require_operation(family,OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings:Vec::new(), effect_bindings: Vec::new(), receiver:Some(receiver),arguments:vec![Some(needle)],result:boolean,effects:EffectSummary::Closed(EffectSet::EMPTY) },why)?; graph.solve()?; let evidence = graph.candidate_evidence(requirement)?.ok_or(InferenceError::Boundary("membership remains ambiguous"))?; assert_eq!(catalog.metadata(graph,evidence.candidate)?.argument_order,OperationArgumentOrder::ReceiverThenNeedle); Ok(()) });
            assert_eq!(outcome.is_ok(),valid,"{outcome:?}");
        }
    }

    #[test]
    fn equality_keeps_independent_unknown_operands_until_compatibility_is_known() {
        let symbols=crate::symbol::SymbolOwner::new(); let _symbols=symbols.enter(); let mut graph=InferenceContext::default(); let mut catalog=OperationGraph::default(); let why=graph.reason(span(),None).unwrap();
        let family=catalog.binary_family(&mut graph,BinaryOp::Eq,span()).unwrap(); let left=graph.fresh(1,span()).unwrap(); let right=graph.fresh(1,span()).unwrap(); let boolean=graph.atom(Atom::Bool).unwrap();
        let requirement=graph.require_operation(family,call(vec![left,right],boolean),why).unwrap(); graph.solve().unwrap();
        assert_ne!(graph.resolved(left).unwrap(),graph.resolved(right).unwrap()); assert!(graph.candidate_evidence(requirement).unwrap().is_none());
        let int=graph.atom(Atom::Int).unwrap(); let optional=graph.optional(int).unwrap(); let null=graph.atom(Atom::Null).unwrap(); graph.unify(left,optional,why).unwrap(); graph.unify(right,null,why).unwrap(); graph.solve().unwrap();
        assert!(graph.candidate_evidence(requirement).unwrap().is_some()); assert_eq!(graph.resolved(right).unwrap(),null);
    }

}
