use super::*;
use super::super::LoweredCompQualifier;
use crate::sema::check::{BindingIdentity, ComprehensionIdentity, ProducerFlowSource};
use crate::sema::inference::{CandidateId, TypeId};
use crate::sema::operation_graph::{IterableDomain, PreparedLanguageOperation};
use crate::source::SourceId;

#[derive(Clone, Copy)]
pub(super) enum CheckedIterationOrigin {
    Statement(StmtId),
    Comprehension { expression: ExprId, qualifier: u32 },
}

pub(super) struct CheckedIterationProjection {
    pub source: ProducerFlowSource,
    pub iterator: ExpressionIdentity,
    pub candidate: CandidateId,
    pub authority: Name,
    pub item_type: TypeId,
    pub item: Type,
    pub domain: IterableDomain,
    pub outer_result: bool,
}

struct CheckedIterationFact {
    candidate: CandidateId,
    authority: Name,
    item_type: TypeId,
    domain: IterableDomain,
    outer_result: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum BuildIterationProducer {
    Literal,
    Parameter,
    Lines { receiver: ExpressionIdentity, receiver_type: TypeId, receiver_scope: Option<crate::sema::inference::SchemeId>, selected: CandidateId, parameter: (crate::sema::check::DeclarationIdentity, u32) },
    NativeStreamCall { selected: CandidateId },
    UserStreamCall { declaration: crate::sema::check::DeclarationIdentity },
    TopLevelBinding { identity: BindingIdentity, initializer: ExpressionIdentity, name: Name, slot: usize },
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildLineScanOperation {
    pub origin: ExpressionIdentity,
    pub selected: CandidateId,
    pub receiver: Option<ExpressionIdentity>,
    pub arguments: Vec<ExpressionIdentity>,
    pub roots: Vec<crate::sema::inference::ScopedRoot>,
    pub result: crate::sema::inference::ScopedRoot,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildLineScanCheck {
    pub predicate: BuildLineScanOperation,
    pub condition: ScanCondition,
    pub counter: super::mutable_binding::BuildMutableBindingOrigin,
    pub increment: super::mutable_binding::BuildMutableBindingWrite,
    pub increment_span: Span,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildLineScanOrigin {
    pub iteration: super::super::BuildIterationBindingOrigin,
    pub trim: Option<BuildLineScanOperation>,
    pub checks: Vec<BuildLineScanCheck>,
    pub span: Span,
}

impl BuildLineScanOperation {
    pub(in crate::runtime::eval) fn validate(&self, solved: &SolvedTypes, caller: Option<crate::sema::check::DeclarationIdentity>) -> Option<()> {
        let operation = solved.operations.get(&self.origin)?;
        if operation.caller != caller || operation.receiver.is_some() != self.receiver.is_some()
            || operation.actual_arguments.len() != self.arguments.len()
            || operation.binding.supplied_slots != (0..self.arguments.len()).collect::<Vec<_>>()
            || !operation.binding.default_slots.is_empty() || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some()
            || solved.graph.candidate_evidence(operation.requirement).ok()??.candidate != self.selected { return None; }
        if solved.graph.closed_effect_summary(operation.effects).ok()? != crate::sema::inference::EffectSummary::Closed(crate::sema::inference::EffectSet::EMPTY) { return None; }
        let roots = self.receiver.iter().chain(self.arguments.iter()).map(|origin| {
            if origin.source != self.origin.source || origin.namespace != self.origin.namespace { return None; }
            Some(crate::sema::inference::ScopedRoot { ty: *solved.expressions.get(origin)?, scope: solved.expression_scope(*origin, caller).ok()? })
        }).collect::<Option<Vec<_>>>()?;
        if roots != self.roots { return None; }
        for (root, operation_root) in roots.iter().zip(operation.receiver.iter().chain(operation.actual_arguments.iter())) {
            solved.graph.validate_scoped(*root).ok()?;
            if solved.graph.export_type(root.ty).ok()? != Type::Bytes || solved.graph.export_type(*operation_root).ok()? != Type::Bytes { return None; }
        }
        let result = crate::sema::inference::ScopedRoot { ty: operation.result, scope: solved.operation_scope(ProducerFlowSource::Expression(self.origin), operation).ok()? };
        if result != self.result || solved.graph.export_type(operation.result).ok()? != solved.graph.export_type(*solved.expressions.get(&self.origin)?).ok()? { return None; }
        solved.graph.validate_scoped(result).ok()?;
        let node = solved.producer_flows.node(*solved.expression_producer_flows.get(&self.origin)?).ok()?;
        if node.source != ProducerFlowSource::Expression(self.origin) { return None; }
        match solved.operation_catalog.candidate(&solved.graph, self.selected).ok()? {
            crate::sema::check::SolvedOperationAuthority::Registry(_) if matches!(node.kind, crate::sema::check::ProducerFlowKind::Operation { requirement, .. } if requirement == operation.requirement) => {},
            // Equality has no producer handles to transfer. Its original
            // selected requirement still owns the predicate and operand roots.
            crate::sema::check::SolvedOperationAuthority::Language(metadata) if metadata.operation == (PreparedLanguageOperation::Equality { op: BinaryOp::Eq }) && matches!(node.kind, crate::sema::check::ProducerFlowKind::Empty) => {},
            _ => return None,
        }
        Some(())
    }
}

// The original operand endpoint and selected canonical member jointly
// authorize item storage and failure transport. A pending family cannot
// choose a carrier here; it needs a checked instance before preparation.
fn checked_iteration_fact(solved: &SolvedTypes, source: ProducerFlowSource, iterator: ExpressionIdentity) -> Option<CheckedIterationFact> {
    let operation = match source {
        ProducerFlowSource::Statement(identity) if identity.source == iterator.source && identity.namespace == iterator.namespace => solved.statement_operations.get(&identity)?,
        ProducerFlowSource::Comprehension(identity) if identity.expression.source == iterator.source && identity.expression.namespace == iterator.namespace => &solved.comprehension_operations.get(&identity)?.operation,
        _ => return None,
    };
    if operation.receiver.is_some() || operation.actual_arguments.len() != 1
        || operation.binding.supplied_slots != [0] || !operation.binding.default_slots.is_empty()
        || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some() { return None; }
    let graph = &solved.graph;
    let expression = *solved.expressions.get(&iterator)?;
    if graph.resolved(operation.actual_arguments[0]).ok()? != graph.resolved(expression).ok()? { return None; }
    let selected = graph.candidate_evidence(operation.requirement).ok()??;
    let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(graph, selected.candidate).ok()? else { return None; };
    let PreparedLanguageOperation::Iteration { domain, outer_result } = metadata.operation else { return None; };
    Some(CheckedIterationFact { candidate: selected.candidate, authority: metadata.identity, item_type: operation.result, domain, outer_result })
}

impl super::super::BuildIterationBindingOrigin {
    pub(in crate::runtime::eval) fn original_lines(solved: &SolvedTypes, iterator: ExpressionIdentity, receiver: ExpressionIdentity, caller: Option<crate::sema::check::DeclarationIdentity>) -> Option<CandidateId> {
        let operation = solved.operations.get(&iterator)?;
        if operation.caller != caller || !operation.actual_arguments.is_empty() || !operation.binding.supplied_slots.is_empty()
            || !operation.binding.default_slots.is_empty() || operation.binding.rest_slot.is_some() || operation.binding.dynamic.is_some() { return None; }
        let receiver_type = *solved.expressions.get(&receiver)?;
        // The selected operation and its expression can retain distinct closed
        // container roots. Their source identities and producer requirement,
        // rather than an incidental type-node address, identify this member.
        if iterator.source != receiver.source || iterator.namespace != receiver.namespace
            || solved.graph.export_type(operation.receiver?).ok()? != solved.graph.export_type(receiver_type).ok()?
            || solved.graph.export_type(operation.result).ok()? != solved.graph.export_type(*solved.expressions.get(&iterator)?).ok()? { return None; }
        let selected = solved.graph.candidate_evidence(operation.requirement).ok()??;
        let crate::sema::check::SolvedOperationAuthority::Registry(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate).ok()? else { return None; };
        if metadata.binding != crate::modules::signature::ImplBinding::Native || metadata.semantic_rule != crate::modules::signature::SemanticRule::Standard { return None; }
        let item = match (metadata.owner, metadata.operation, solved.graph.export_type(receiver_type).ok()?) {
            (crate::sema::registry_graph::RegistryOwner::Method(MethodReceiver::Str), RuntimeOp::TextStreamLines, Type::Str) => Type::Str,
            (crate::sema::registry_graph::RegistryOwner::Method(MethodReceiver::Bytes), RuntimeOp::BytesStreamLines, Type::Bytes) => Type::Bytes,
            _ => return None,
        };
        if solved.graph.export_type(operation.result).ok()? != Type::List(Box::new(item)) { return None; }
        let node = solved.producer_flows.node(*solved.expression_producer_flows.get(&iterator)?).ok()?;
        if node.source != ProducerFlowSource::Expression(iterator) || !matches!(node.kind, crate::sema::check::ProducerFlowKind::Operation { requirement, .. } if requirement == operation.requirement) { return None; }
        Some(selected.candidate)
    }

    pub(in crate::runtime::eval) fn original_fs_children(solved: &SolvedTypes, iterator: ExpressionIdentity, caller: Option<crate::sema::check::DeclarationIdentity>) -> Option<CandidateId> {
        let operation = solved.operations.get(&iterator)?;
        if operation.caller != caller { return None; }
        let selected = solved.graph.candidate_evidence(operation.requirement).ok()??;
        let crate::sema::check::SolvedOperationAuthority::Registry(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate).ok()? else { return None; };
        if metadata.owner != crate::sema::registry_graph::RegistryOwner::Module("fs") || metadata.operation != RuntimeOp::FsChildren
            || metadata.binding != crate::modules::signature::ImplBinding::Native || metadata.semantic_rule != crate::modules::signature::SemanticRule::Standard { return None; }
        let node = solved.producer_flows.node(*solved.expression_producer_flows.get(&iterator)?).ok()?;
        if node.source != ProducerFlowSource::Expression(iterator) || !matches!(node.kind, crate::sema::check::ProducerFlowKind::Operation { requirement, .. } if requirement == operation.requirement) { return None; }
        Some(selected.candidate)
    }

    // A formal producer port identifies the original parameter independently
    // of the type shared by other parameters or their emitted slot words.
    pub(in crate::runtime::eval) fn original_parameter(solved: &SolvedTypes, iterator: ExpressionIdentity, caller: Option<crate::sema::check::DeclarationIdentity>) -> Option<Option<(crate::sema::check::DeclarationIdentity, u32)>> {
        use crate::sema::check::ProducerFlowKind;
        let flow = *solved.expression_producer_flows.get(&iterator)?;
        let node = solved.producer_flows.node(flow).ok()?;
        if node.source != ProducerFlowSource::Expression(iterator) { return None; }
        let ProducerFlowKind::Join { inputs } = &node.kind else { return Some(None); };
        let [input] = inputs.as_slice() else { return Some(None); };
        let node = solved.producer_flows.node(*input).ok()?;
        let ProducerFlowKind::Parameter { declaration, index } = node.kind else { return Some(None); };
        if caller != Some(declaration) || declaration.source != iterator.source || declaration.namespace != iterator.namespace
            || node.source != (ProducerFlowSource::Parameter { declaration, index }) { return None; }
        let function = solved.declarations.get(&declaration)?;
        if function.parameter_producer_flows.get(index as usize) != Some(input) { return None; }
        let signature = solved.graph.callable_signature(function.signature).ok()?;
        let crate::sema::inference::TypeNode::Arrow(arrow) = solved.graph.node(solved.graph.resolved(signature).ok()?).ok()? else { return None; };
        if index as usize >= arrow.params.len() { return None; }
        Some(Some((declaration, index)))
    }
}

pub(in crate::runtime::eval) fn original_user_stream_call(solved: &SolvedTypes, iterator: ExpressionIdentity, caller: Option<crate::sema::check::DeclarationIdentity>) -> Option<crate::sema::check::DeclarationIdentity> {
    use crate::sema::check::ProducerFlowKind;
    let call = solved.calls.get(&iterator)?;
    if call.caller != caller { return None; }
    let declaration = call.declaration?;
    if solved.declarations.get(&declaration)?.kind != crate::sema::inference::CallableKind::Stream { return None; }
    let flow = *solved.expression_producer_flows.get(&iterator)?;
    let node = solved.producer_flows.node(flow).ok()?;
    if node.source != ProducerFlowSource::Expression(iterator) { return None; }
    let ProducerFlowKind::Apply { call, .. } = &node.kind else { return None; };
    if *call != iterator { return None; }
    Some(declaration)
}

// Immutable driver reads retain a canonical binding version and its authored
// initializer; a named dense slot alone cannot identify that producer.
pub(in crate::runtime::eval) fn original_top_level_iterator_binding(solved: &SolvedTypes, iterator: ExpressionIdentity) -> Option<(BindingIdentity, ExpressionIdentity)> {
    use crate::sema::check::ProducerFlowKind;
    let flow = *solved.expression_producer_flows.get(&iterator)?;
    let node = solved.producer_flows.node(flow).ok()?;
    if node.source != ProducerFlowSource::Expression(iterator) { return None; }
    let ProducerFlowKind::CapturedBinding { identity, version: 0, input } = &node.kind else { return None; };
    let binding = solved.bindings.get(identity)?;
    if binding.mutable || binding.owner.is_some() || solved.binding_producer_flows.get(&(*identity, 0)) != Some(input) { return None; }
    let node = solved.producer_flows.node(*input).ok()?;
    if node.source != (ProducerFlowSource::Binding { identity: *identity, version: 0 }) { return None; }
    let ProducerFlowKind::Join { inputs } = &node.kind else { return None; };
    let [input] = inputs.as_slice() else { return None; };
    let ProducerFlowSource::Expression(initializer) = solved.producer_flows.node(*input).ok()?.source else { return None; };
    if solved.expression_producer_flows.get(&initializer) != Some(input) || solved.expression_owners.get(&initializer).copied().is_some() { return None; }
    Some((*identity, initializer))
}

impl CompactLowerConstructProbe<'_, '_> {
    fn original_line_scan_operation(&self, expression: ExprId, receiver: Option<ExprId>, arguments: &[ExprId]) -> Option<BuildLineScanOperation> {
        let origin = self.expression_identity(expression);
        let operation = self.solved().operations.get(&origin)?;
        let selected = self.solved().graph.candidate_evidence(operation.requirement).ok()??.candidate;
        let receiver = receiver.map(|expression| self.expression_identity(expression));
        let arguments = arguments.iter().map(|&expression| self.expression_identity(expression)).collect::<Vec<_>>();
        let roots = receiver.iter().chain(arguments.iter()).map(|origin| Some(crate::sema::inference::ScopedRoot { ty: *self.solved().expressions.get(origin)?, scope: self.solved().expression_scope(*origin, operation.caller).ok()? })).collect::<Option<Vec<_>>>()?;
        let result = crate::sema::inference::ScopedRoot { ty: operation.result, scope: self.solved().operation_scope(ProducerFlowSource::Expression(origin), operation).ok()? };
        let source = BuildLineScanOperation { origin, selected, receiver, arguments, roots, result };
        source.validate(self.solved(), operation.caller)?;
        Some(source)
    }

    fn original_line_scan_condition(&self, expression: ExprId, line: BindingIdentity, trimmed: Option<BindingIdentity>) -> Option<(BuildLineScanOperation, ScanCondition)> {
        let expected = trimmed.unwrap_or(line);
        match self.program.arena.expr(expression).kind {
            ArenaExprKind::Binary { op: BinaryOp::Eq, left, right } if trimmed.is_some() => {
                if comprehension_read_binding(self.solved(), self.expression_identity(left)) != Some(expected) { return None; }
                let ArenaExprKind::Bytes(literal) = self.program.arena.expr(right).kind else { return None; };
                if !self.program.arena.bytes_literal(literal).is_empty() { return None; }
                let source = self.original_line_scan_operation(expression, None, &[left, right])?;
                if !matches!(self.solved().operation_catalog.candidate(&self.solved().graph, source.selected).ok()?, crate::sema::check::SolvedOperationAuthority::Language(metadata) if metadata.operation == (PreparedLanguageOperation::Equality { op: BinaryOp::Eq })) { return None; }
                Some((source, ScanCondition::TrimEmpty))
            }
            ArenaExprKind::Call { callee, args } => {
                let ArenaExprKind::Field { base, .. } = self.program.arena.expr(callee).kind else { return None; };
                if comprehension_read_binding(self.solved(), self.expression_identity(base)) != Some(expected) { return None; }
                let [argument] = self.program.arena.call_args(args) else { return None; };
                let ArenaCallArgKind::Positional(argument) = argument.kind else { return None; };
                let ArenaExprKind::Bytes(literal) = self.program.arena.expr(argument).kind else { return None; };
                let source = self.original_line_scan_operation(expression, Some(base), &[argument])?;
                if !matches!(self.solved().operation_catalog.candidate(&self.solved().graph, source.selected).ok()?, crate::sema::check::SolvedOperationAuthority::Registry(metadata) if metadata.owner == crate::sema::registry_graph::RegistryOwner::Method(MethodReceiver::Bytes) && metadata.operation == RuntimeOp::BytesStartsWith && metadata.binding == crate::modules::signature::ImplBinding::Native && metadata.semantic_rule == crate::modules::signature::SemanticRule::Standard) { return None; }
                let needle = self.program.arena.bytes_literal(literal).to_vec();
                Some((source, if trimmed.is_some() { ScanCondition::TrimStartsWith(needle) } else { ScanCondition::StartsWith(needle) }))
            }
            _ => None,
        }
    }

    fn collect_original_line_scan_checks(&self, statement: StmtId, row: BuildStmtId, line: BindingIdentity, trimmed: Option<BindingIdentity>, checks: &mut Vec<BuildLineScanCheck>) -> Option<()> {
        let ArenaStmtKind::If { branches, else_block } = self.program.arena.stmt(statement).kind else { return None; };
        let original = self.program.arena.if_branches(branches);
        let (bodies, otherwise) = match self.scratch.borrow().statements.get(row.index())? {
            BuildStmtRow::If { branches, else_body } => (branches.iter().map(|(_, body)| body.clone()).collect::<Vec<_>>(), else_body.clone()),
            BuildStmtRow::IfBool { branches, else_body } => (branches.iter().map(|(_, body)| body.clone()).collect::<Vec<_>>(), else_body.clone()),
            _ => return None,
        };
        if original.len() != bodies.len() { return None; }
        for (branch, body) in original.iter().zip(&bodies) {
            let [increment_row] = body.as_slice() else { return None; };
            let authored = self.program.arena.stmt_ids(self.program.arena.block(branch.block).statements).collect::<Vec<_>>();
            let [increment_statement] = authored.as_slice() else { return None; };
            let (predicate, condition) = self.original_line_scan_condition(branch.condition, line, trimmed)?;
            if self.solved().graph.export_type(predicate.result.ty).ok()? != Type::Bool { return None; }
            let scratch = self.scratch.borrow();
            let increment = scratch.mutable_binding_writes.get(increment_row)?.clone();
            let counter = scratch.mutable_binding_origins.get(&increment.binding)?.clone();
            if increment.statement != self.statement_identity(*increment_statement) || increment.capture.is_some() || increment.binding != counter.binding { return None; }
            let slot = match (&increment.emitted, scratch.statements.get(increment_row.index())?) {
                (super::mutable_binding::BuildMutableStatement::Integer { slot, value, assignment: Some(AssignOp::Add) }, BuildStmtRow::AssignInt { slot: actual, value: actual_value, op: AssignOp::Add, .. }) if slot == actual && value == actual_value && matches!(scratch.ints.get(value.index()), Some(BuildIntRow::Int(1))) => *slot,
                (super::mutable_binding::BuildMutableStatement::Value { slot, value, assignment: Some(AssignOp::Add), check: None }, BuildStmtRow::Assign { slot: actual, value: actual_value, op: AssignOp::Add, check: None, .. }) if slot == actual && value == actual_value && matches!(scratch.expressions.get(value.index()), Some(BuildExprRow::Int(1))) => *slot,
                _ => return None,
            };
            if slot != counter.slot || self.solved().graph.export_type(counter.source_type.ty).ok()? != Type::Int || self.solved().graph.export_type(increment.value_type.ty).ok()? != Type::Int { return None; }
            checks.push(BuildLineScanCheck { predicate, condition, counter, increment, increment_span: self.program.arena.stmt(*increment_statement).span });
        }
        match (else_block, otherwise) {
            (None, None) => Some(()),
            (Some(block), Some(body)) => {
                let authored = self.program.arena.stmt_ids(self.program.arena.block(block).statements).collect::<Vec<_>>();
                let ([statement], [row]) = (authored.as_slice(), body.as_slice()) else { return None; };
                self.collect_original_line_scan_checks(*statement, *row, line, trimmed, checks)
            }
            _ => None,
        }
    }

    pub(super) fn try_lower_original_scan_lines(&self, statement: StmtId, target: BindingTargetId, projection: &CheckedIterationProjection, text: BuildExprId, line_slot: usize, body: &[BuildStmtId], span: Span) -> Option<BuildStmtId> {
        let Some(BuildIterationProducer::Lines { .. }) = self.checked_line_iteration_source(projection) else { return None; };
        if projection.item != Type::Bytes { return None; }
        let text_slot = match self.scratch.borrow().expressions.get(text.index())? { BuildExprRow::Param(slot) => *slot, _ => return None };
        let ArenaStmtKind::For { block, .. } = self.program.arena.stmt(statement).kind else { return None; };
        let authored = self.program.arena.stmt_ids(self.program.arena.block(block).statements).collect::<Vec<_>>();
        let line = BindingIdentity { source: projection.iterator.source, namespace: projection.iterator.namespace, target };
        let (original_if, lowered_if, trim, trimmed) = match (authored.as_slice(), body) {
            ([original], [lowered]) => (*original, *lowered, None, None),
            ([original_trim, original_if], [lowered_trim, lowered_if]) => {
                let ArenaStmtKind::Let { target, initializer: ArenaExprOrRun::Expr(value), .. } = self.program.arena.stmt(*original_trim).kind else { return None; };
                let ArenaExprKind::Call { callee, args } = self.program.arena.expr(value).kind else { return None; };
                if !self.program.arena.call_args(args).is_empty() { return None; }
                let ArenaExprKind::Field { base, .. } = self.program.arena.expr(callee).kind else { return None; };
                if comprehension_read_binding(self.solved(), self.expression_identity(base)) != Some(line) { return None; }
                let source = self.original_line_scan_operation(value, Some(base), &[])?;
                if !matches!(self.solved().operation_catalog.candidate(&self.solved().graph, source.selected).ok()?, crate::sema::check::SolvedOperationAuthority::Registry(metadata) if metadata.owner == crate::sema::registry_graph::RegistryOwner::Method(MethodReceiver::Bytes) && metadata.operation == RuntimeOp::BytesTrim && metadata.binding == crate::modules::signature::ImplBinding::Native && metadata.semantic_rule == crate::modules::signature::SemanticRule::Standard) { return None; }
                let scratch = self.scratch.borrow();
                let BuildStmtRow::Let { value: emitted, .. } = scratch.statements.get(lowered_trim.index())? else { return None; };
                if self.expression_origins.get(emitted) != Some(&source.origin) { return None; }
                (*original_if, *lowered_if, Some(source), Some(BindingIdentity { source: line.source, namespace: line.namespace, target }))
            }
            _ => return None,
        };
        let mut checks = Vec::new();
        self.collect_original_line_scan_checks(original_if, lowered_if, line, trimmed, &mut checks)?;
        if checks.is_empty() { return None; }
        let emitted = checks.iter().map(|check| ScanCheck { condition: check.condition.clone(), counter_slot: check.counter.slot }).collect();
        let row = push_build_row!(self, stmt, BuildStmtRow::ScanLines { text_slot, line_slot, checks: emitted, span });
        self.record_checked_iteration_binding(statement, target, projection, row, text, line_slot)?;
        let mut scratch = self.scratch.borrow_mut();
        let iteration = scratch.iteration_binding_origins.get(&line)?.clone();
        scratch.line_scan_origins.insert(row, BuildLineScanOrigin { iteration, trim, checks, span });
        Some(row)
    }

    pub(super) fn checked_iteration_projection(&self, origin: CheckedIterationOrigin, iter: ExprId) -> Option<CheckedIterationProjection> {
        let source = match origin {
            CheckedIterationOrigin::Statement(statement) => {
                match self.program.arena.stmt(statement).kind {
                    ArenaStmtKind::For { iter: original, .. } | ArenaStmtKind::YieldDelegate(original) if original == iter => {},
                    _ => return None,
                }
                ProducerFlowSource::Statement(self.statement_identity(statement))
            }
            CheckedIterationOrigin::Comprehension { expression, qualifier } => {
                let range = match self.program.arena.expr(expression).kind {
                    ArenaExprKind::ListComp { qualifiers, .. } | ArenaExprKind::MapComp { qualifiers, .. } => qualifiers,
                    _ => return None,
                };
                match self.program.arena.comp_qualifiers(range).get(qualifier as usize)? {
                    crate::syntax::arena::ArenaCompQualifier::For { iter: original, .. } if *original == iter => {},
                    _ => return None,
                }
                ProducerFlowSource::Comprehension(ComprehensionIdentity { expression: self.expression_identity(expression), qualifier })
            }
        };
        let iterator = self.expression_identity(iter);
        let fact = checked_iteration_fact(self.solved(), source, iterator)?;
        let item = self.solved_type(fact.item_type)?;
        Some(CheckedIterationProjection { source, iterator, candidate: fact.candidate, authority: fact.authority, item_type: fact.item_type, item, domain: fact.domain, outer_result: fact.outer_result })
    }

    // The checked iterable selects failure transport. Materialized map and
    // scalar sources propagate their original Result error lexically; list
    // and stream adapters retain their own runtime error transport.
    pub(super) fn lower_checked_iterable(&mut self, origin: CheckedIterationOrigin, iter: ExprId, slots: &mut SlotScope, current_function: Option<Name>, item_slot: Option<usize>) -> Option<(BuildExprId, CheckedIterationProjection)> {
        let Some(projection) = self.checked_iteration_projection(origin, iter) else {
            self.last_blocker_detail = Some((self.program.arena.expr(iter).span, "iteration has no selected original source operation".into()));
            return None;
        };
        let lowered = self.lower_expr(iter, slots, current_function, item_slot)?;
        let lowered = if projection.outer_result && matches!(projection.domain, IterableDomain::Map | IterableDomain::Str | IterableDomain::Bytes) {
            push_build_row!(self, expr, BuildExprRow::Try(lowered))
        } else { lowered };
        Some((lowered, projection))
    }

    pub(super) fn checked_line_iteration_source(&self, projection: &CheckedIterationProjection) -> Option<BuildIterationProducer> {
        if projection.domain != IterableDomain::List || projection.outer_result || !matches!(projection.item, Type::Str | Type::Bytes) { return None; }
        let ProducerFlowSource::Statement(statement) = projection.source else { return None; };
        let operation = self.solved().statement_operations.get(&statement)?;
        let ArenaExprKind::Call { callee, args } = self.program.arena.expr(projection.iterator.expression).kind else { return None; };
        if !self.program.arena.call_args(args).is_empty() { return None; }
        let ArenaExprKind::Field { base, .. } = self.program.arena.expr(callee).kind else { return None; };
        let receiver = self.expression_identity(base);
        let selected = super::super::BuildIterationBindingOrigin::original_lines(self.solved(), projection.iterator, receiver, operation.caller)?;
        let parameter = super::super::BuildIterationBindingOrigin::original_parameter(self.solved(), receiver, operation.caller)??;
        let receiver_type = *self.solved().expressions.get(&receiver)?;
        let receiver_scope = self.solved().expression_scope(receiver, operation.caller).ok()?;
        self.solved().graph.validate_scoped(crate::sema::inference::ScopedRoot { ty: receiver_type, scope: receiver_scope }).ok()?;
        Some(BuildIterationProducer::Lines { receiver, receiver_type, receiver_scope, selected, parameter })
    }

    pub(super) fn checked_iteration_binding_supported(&self, projection: &CheckedIterationProjection) -> bool {
        if self.checked_line_iteration_source(projection).is_some() { return true; }
        let protocol = matches!((projection.domain, projection.outer_result, self.solved().graph.export_type(projection.item_type)),
            (IterableDomain::List, false, Ok(Type::Str | Type::Bytes | Type::Int | Type::Path | Type::Record(_))) | (IterableDomain::Str, false, Ok(Type::Str)) | (IterableDomain::Bytes, _, Ok(Type::Int)) | (IterableDomain::Stream, false, Ok(Type::Int)) | (IterableDomain::Stream, true, Ok(Type::Record(_))));
        if !protocol { return false; }
        let ProducerFlowSource::Statement(statement) = projection.source else { return false; };
        let Some(operation) = self.solved().statement_operations.get(&statement) else { return true; };
        if matches!(projection.item, Type::Record(_)) { return match (projection.domain, projection.outer_result) {
            (IterableDomain::List, false) => original_top_level_iterator_binding(self.solved(), projection.iterator).is_some()
                || matches!(self.program.arena.expr(projection.iterator.expression).kind, ArenaExprKind::List(_)),
            (IterableDomain::Stream, true) => super::super::BuildIterationBindingOrigin::original_fs_children(self.solved(), projection.iterator, operation.caller).is_some(),
            _ => false,
        }; }
        match super::super::BuildIterationBindingOrigin::original_parameter(self.solved(), projection.iterator, operation.caller) {
            Some(Some(_)) | None => true,
            Some(None) => {
                let source = self.program.arena.expr(projection.iterator.expression).kind;
                if projection.domain == IterableDomain::Stream { return original_user_stream_call(self.solved(), projection.iterator, operation.caller).is_some() || original_top_level_iterator_binding(self.solved(), projection.iterator).is_some(); }
                if projection.outer_result { matches!(source, ArenaExprKind::Call { callee, .. } if matches!(self.program.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == Name::intern("Ok"))) }
                else { matches!(source, ArenaExprKind::List(_) | ArenaExprKind::Str(_) | ArenaExprKind::Bytes(_)) }
            }
        }
    }

    // Only the selected source operation authorizes the item. A storage kind
    // or a same-typed slot never identifies which loop initialized a read.
    pub(super) fn record_checked_iteration_binding(&self, statement: StmtId, target: BindingTargetId, projection: &CheckedIterationProjection, row: BuildStmtId, iterator: BuildExprId, slot: usize) -> Option<()> {
        if !self.checked_iteration_binding_supported(projection) { return Some(()); }
        let graph = &self.solved().graph;
        let statement = self.statement_identity(statement);
        let ProducerFlowSource::Statement(source) = projection.source else { return None; };
        if source != statement { return None; }
        let binding = BindingIdentity { source: statement.source, namespace: statement.namespace, target };
        let target_source = self.program.arena.binding_target(target);
        if target_source.span.is_some_and(|span| self.program.arena.span(span).source_id != statement.source) { return None; }
        if !matches!(target_source.kind, ArenaBindingTargetKind::Name(name) if !is_discard_name(name)) { return Some(()); }
        let solved = self.solved();
        let operation = solved.statement_operations.get(&statement)?;
        let original = solved.bindings.get(&binding)?;
        if original.mutable || original.owner != operation.caller { return None; }
        let input = crate::sema::inference::ScopedRoot { ty: *solved.expressions.get(&projection.iterator)?, scope: solved.expression_scope(projection.iterator, operation.caller).ok()? };
        let item = crate::sema::inference::ScopedRoot { ty: operation.result, scope: solved.operation_scope(projection.source, operation).ok()? };
        let binding_type = crate::sema::inference::ScopedRoot { ty: original.ty, scope: original.scheme.or(item.scope) };
        for root in [input, item, binding_type] { graph.validate_scoped(root).ok()?; }
        let expected_item = graph.export_type(projection.item_type).ok()?;
        let expected_input = match projection.domain { IterableDomain::List => Type::List(Box::new(expected_item.clone())), IterableDomain::Str => Type::Str, IterableDomain::Bytes => Type::Bytes, IterableDomain::Stream => Type::Stream(Box::new(expected_item.clone())), _ => return None };
        let checked_input = graph.export_type(input.ty).ok()?;
        if if projection.outer_result { !matches!(&checked_input, Type::Result(success, _) if **success == expected_input) } else { checked_input != expected_input }
            || graph.export_type(item.ty).ok()? != expected_item || graph.export_type(binding_type.ty).ok()? != expected_item { return None; }
        let lines = self.checked_line_iteration_source(projection);
        let iterator_parameter = super::super::BuildIterationBindingOrigin::original_parameter(solved, projection.iterator, operation.caller)?;
        let mut scratch = self.scratch.borrow_mut();
        let carrier = if projection.outer_result && projection.domain != IterableDomain::Stream {
            let Some(BuildExprRow::Try(carrier)) = scratch.expressions.get(iterator.index()) else { return None; };
            Some(*carrier)
        } else { None };
        let authored_iterator = carrier.unwrap_or(iterator);
        let expected_origin = match lines { Some(BuildIterationProducer::Lines { receiver, .. }) => receiver, _ => projection.iterator };
        if self.expression_origins.get(&authored_iterator) != Some(&expected_origin) { return None; }
        let producer = if let Some(lines) = lines { lines } else if iterator_parameter.is_some() { BuildIterationProducer::Parameter }
            else if let Some(selected) = super::super::BuildIterationBindingOrigin::original_fs_children(solved, projection.iterator, operation.caller) { BuildIterationProducer::NativeStreamCall { selected } }
            else if let Some(declaration) = original_user_stream_call(solved, projection.iterator, operation.caller) { BuildIterationProducer::UserStreamCall { declaration } }
            else if let Some((identity, initializer)) = original_top_level_iterator_binding(solved, projection.iterator) {
                let ArenaExprKind::Ident(name) = self.program.arena.expr(projection.iterator.expression).kind else { return None; };
                let Some(BuildExprRow::Param(slot)) = scratch.expressions.get(authored_iterator.index()) else { return None; };
                BuildIterationProducer::TopLevelBinding { identity, initializer, name, slot: *slot }
            } else { BuildIterationProducer::Literal };
        match iterator_parameter {
            Some((_, index)) if matches!(scratch.expressions.get(authored_iterator.index()), Some(BuildExprRow::Param(slot)) if *slot == index as usize) => {},
            None if projection.outer_result && matches!(scratch.expressions.get(authored_iterator.index()), Some(BuildExprRow::Ok(_))) => {},
            None if matches!((self.program.arena.expr(projection.iterator.expression).kind, scratch.expressions.get(authored_iterator.index())),
                (ArenaExprKind::List(_), Some(BuildExprRow::List(_))) | (ArenaExprKind::Str(_), Some(BuildExprRow::Str(_))) | (ArenaExprKind::Bytes(_), Some(BuildExprRow::Bytes(_)))) => {},
            None if matches!(producer, BuildIterationProducer::Lines { parameter: (_, index), .. } if matches!(scratch.expressions.get(authored_iterator.index()), Some(BuildExprRow::Param(slot)) if *slot == index as usize)) => {},
            None if matches!(producer, BuildIterationProducer::NativeStreamCall { .. }) && matches!(scratch.expressions.get(authored_iterator.index()), Some(BuildExprRow::ModuleCall { op: RuntimeOp::FsChildren, .. } | BuildExprRow::FsList { op: RuntimeOp::FsChildren, .. })) => {},
            None if matches!(producer, BuildIterationProducer::UserStreamCall { .. }) && matches!(scratch.expressions.get(authored_iterator.index()), Some(BuildExprRow::Call { .. } | BuildExprRow::DirectPureCall { .. })) => {},
            None if matches!(producer, BuildIterationProducer::TopLevelBinding { slot, .. } if matches!(scratch.expressions.get(authored_iterator.index()), Some(BuildExprRow::Param(actual)) if *actual == slot)) => {},
            _ => return None,
        }
        if scratch.iteration_binding_origins.contains_key(&binding) { return None; }
        scratch.iteration_binding_origins.insert(binding, super::super::BuildIterationBindingOrigin {
            statement, binding, iterator_source: projection.iterator, caller: operation.caller, selected: projection.candidate,
            input, item, binding_type, iterator_parameter, producer, row, iterator, carrier, slot,
        });
        Some(())
    }

    // Each destructured leaf owns a checked binding fact, including nested
    // fields whose item is represented by a quantified structural endpoint.
    pub(super) fn lower_checked_iteration_target(&self, target: BindingTargetId, source: SourceId, slots: &mut SlotScope) -> Option<LoweredCompTarget> {
        match self.program.arena.binding_target(target).kind {
            ArenaBindingTargetKind::Name(name) => {
                if is_discard_name(name) { return Some(LoweredCompTarget::Discard); }
                if slots.is_declared_here(name) { return None; }
                let identity = BindingIdentity { source, namespace: self.current_namespace, target };
                let binding = self.solved().bindings.get(&identity)?;
                let ty = self.solved_type(binding.ty)?;
                Some(LoweredCompTarget::Slot(slots.declare_with_type(name, Some(ty))))
            }
            ArenaBindingTargetKind::Record { fields, .. } => {
                let mut lowered = LoweredCompFields::new();
                for field in self.program.arena.destructure_fields(fields) {
                    let span = self.program.arena.span(field.span);
                    if span.source_id != source { return None; }
                    let child = self.lower_checked_iteration_target(field.target, source, slots)?;
                    lowered.push((field.name, Box::new(child), span));
                }
                Some(LoweredCompTarget::Record { fields: lowered })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::Checker;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    #[test]
    fn original_list_item_binding_transport_keeps_loop_and_read_authority_after_arena_disposal() {
        crate::runtime::eval::run_eval(|| {
            let source = "proc quoted(values: List[Str]) [io] {\n for value in values { print ${shlex.quote(value)} }\n}\nquoted([\"one\", \"two\"])\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("iteration-item-binding.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            assert!(Arc::ptr_eq(&checked.solved, &bodies.solved));
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let mut functions = Vec::new();
            lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources, StdlibLowerLinkage::Local, |unit| {
                assert!(unit.is_lowered(), "{:?}", unit.blocker_detail);
                functions.push(unit.body.unwrap()); Ok(())
            }).unwrap();
            let operation = *checked.solved.statement_operations.keys().find(|identity| matches!(parsed.arena.arena.stmt(identity.statement).kind, ArenaStmtKind::For { .. })).unwrap();
            let ArenaStmtKind::For { target, iter, .. } = parsed.arena.arena.stmt(operation.statement).kind else { unreachable!() };
            let binding = BindingIdentity { source: operation.source, namespace: operation.namespace, target };
            let iterator = ExpressionIdentity { source: operation.source, namespace: operation.namespace, expression: iter };
            drop(parsed);
            let scratch = functions[0].scratch.borrow();
            let original = scratch.iteration_binding_origins.get(&binding).expect("the original loop item retains its own source operation and physical binding");
            assert_eq!(original.statement, operation);
            assert_eq!(original.iterator_source, iterator);
            assert!(matches!(&scratch.statements[original.row.index()], BuildStmtRow::For { slot, iter, .. } if *slot == original.slot && *iter == original.iterator));
            assert!(scratch.iteration_binding_uses.values().any(|actual| *actual == binding), "actual native argument reads retain the original iteration binding");
            assert_eq!(checked.solved.graph.export_type(original.input.ty).unwrap(), Type::List(Box::new(Type::Str)));
            assert_eq!(checked.solved.graph.export_type(original.item.ty).unwrap(), Type::Str);
            assert_eq!(checked.solved.graph.export_type(original.binding_type.ty).unwrap(), Type::Str);
        });
    }

    #[test]
    fn original_line_members_retain_selected_receiver_and_formal_producer() {
        crate::runtime::eval::run_eval(|| {
            for source in ["proc shown(text: Str) [io] { for line in text.lines() { print ${line} } }\n", "proc shown(text: Bytes) [io] { for line in text.lines() { print ${line.len()} } }\n"] {
                let parsed = Parser::parse_source_arena_only(SourceId::new(52), source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let checked = Checker::check_arena(&parsed.arena, source);
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                let _symbols = parsed.arena.symbol_owner().enter();
                let (&statement, operation) = checked.solved.statement_operations.iter().find(|(identity, _)| matches!(parsed.arena.arena.stmt(identity.statement).kind, ArenaStmtKind::For { .. })).unwrap();
                let ArenaStmtKind::For { iter, .. } = parsed.arena.arena.stmt(statement.statement).kind else { unreachable!() };
                let ArenaExprKind::Call { callee, .. } = parsed.arena.arena.expr(iter).kind else { panic!("lines source is an authored call") };
                let ArenaExprKind::Field { base, .. } = parsed.arena.arena.expr(callee).kind else { panic!("lines source is an authored member") };
                let iterator = ExpressionIdentity { source: statement.source, namespace: statement.namespace, expression: iter };
                let receiver = ExpressionIdentity { expression: base, ..iterator };
                let flow = checked.solved.expression_producer_flows.get(&iterator).and_then(|flow| checked.solved.producer_flows.node(*flow).ok());
                let native = checked.solved.operations.get(&iterator).unwrap();
                let selected = checked.solved.graph.candidate_evidence(native.requirement).unwrap().unwrap();
                let metadata = checked.solved.operation_catalog.candidate(&checked.solved.graph, selected.candidate).unwrap();
                assert!(super::super::super::BuildIterationBindingOrigin::original_lines(&checked.solved, iterator, receiver, operation.caller).is_some(), "the original lines member must retain selected receiver authority: operation {:?}, flow {:?}, authority {:?}, receiver {:?}, receiver expression {:?}, result {:?}, result expression {:?}", checked.solved.operations.get(&iterator), flow, metadata, checked.solved.graph.export_type(native.receiver.unwrap()), checked.solved.expressions.get(&receiver).map(|ty| checked.solved.graph.export_type(*ty)), checked.solved.graph.export_type(native.result), checked.solved.expressions.get(&iterator).map(|ty| checked.solved.graph.export_type(*ty)));
                assert!(matches!(super::super::super::BuildIterationBindingOrigin::original_parameter(&checked.solved, receiver, operation.caller), Some(Some(_))), "the original lines receiver must retain its formal producer port");
            }
        });
    }

    #[test]
    fn original_optimized_line_items_execute_on_both_routes_after_frontend_drop() {
        execute_after_arena_disposal("proc shown(text: Str) [io] { for line in text.lines() { print ${line} } }\nshown(\"a\\r\\nbc\\n\")\n", b"a\nbc\n", Some("shown"));
        execute_after_arena_disposal("proc shown(text: Bytes) [io] { for line in text.lines() { print ${line.len()} } }\nshown(b\"a\\r\\nbc\\n\")\n", b"1\n2\n", Some("shown"));
    }

    #[test]
    fn original_bytes_list_items_execute_on_both_routes_after_frontend_drop() {
        execute_after_arena_disposal("proc shown(values: List[Bytes]) [io] { for value in values { print ${value.len()} } for value in [b\"abc\"] { print ${value.len()} } }\nshown([b\"a\", b\"bc\"])\n", b"1\n2\n3\n", Some("shown"));
    }

    #[test]
    fn original_line_scanner_members_and_counter_writes_execute_both_routes_after_frontend_drop() {
        execute_after_arena_disposal_with_scans("proc shown(text: Bytes) [io] { var blanks = 0; var comments = 0; for line in text.lines() { let trimmed = line.trim(); if trimmed == b\"\" { blanks += 1 } else if trimmed.starts_with(b\"#\") { comments += 1 } }; print ${blanks} ${comments} }\nshown(b\"  # comment\\r\\nvalue\\n \\n#second\\n\")\n", b"1 2\n", Some("shown"), Some(1));
    }

    #[test]
    fn original_captured_stream_call_retains_its_declaration_and_producer_port() {
        crate::runtime::eval::run_eval(|| {
            let source = "let factor = 3\nstream rows(value: Int) -> Stream[Int] { yield value * factor }\nfor row in rows(2) { print ${row.bit_and(1)} }\n";
            let parsed = Parser::parse_source_arena_only(SourceId::new(51), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let _symbols = parsed.arena.symbol_owner().enter();
            let (&statement, operation) = checked.solved.statement_operations.iter().find(|(identity, _)| matches!(parsed.arena.arena.stmt(identity.statement).kind, ArenaStmtKind::For { .. })).unwrap();
            let ArenaStmtKind::For { iter, .. } = parsed.arena.arena.stmt(statement.statement).kind else { unreachable!() };
            let iterator = ExpressionIdentity { source: statement.source, namespace: statement.namespace, expression: iter };
            let flow = checked.solved.expression_producer_flows.get(&iterator).and_then(|flow| checked.solved.producer_flows.node(*flow).ok());
            assert!(original_user_stream_call(&checked.solved, iterator, operation.caller).is_some(), "the original captured stream call must retain its declaration and producer: call {:?}, invocation {:?}, flow {:?}, caller {:?}", checked.solved.calls.get(&iterator), checked.solved.invocations.get(&iterator), flow, operation.caller);
        });
    }

    #[test]
    fn original_immutable_driver_record_list_items_execute_on_both_routes_after_frontend_drop() {
        execute_after_arena_disposal("pure made() { [{name: \"one\", count: 2}] }\nlet rows = made()\nfor row in rows { print ${row.name} ${row.count.bit_and(1)} }\n", b"one 0\n", Some("made"));
    }

    #[test]
    fn original_literal_record_list_items_execute_both_routes_after_frontend_drop() {
        execute_after_arena_disposal("proc shown() [io] { for sample in [{name: \"one\", count: 2}, {name: \"two\", count: 3}] { print ${sample.name} ${sample.count.bit_and(1)} } }\nshown()\n", b"one 0\ntwo 1\n", Some("shown"));
    }

    #[test]
    fn original_fs_children_record_item_executes_on_both_routes_after_frontend_drop() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let directory = std::env::temp_dir().join(format!("xsh-iteration-fs-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("one.txt"), b"one").unwrap();
        let source = format!("proc shown() [io, fs, error] -> Result[Unit] {{ for entry in fs.children(p\"{}\") {{ print ${{entry.name}} }} }}\nshown()?\n", directory.display());
        let result = std::panic::catch_unwind(|| execute_after_arena_disposal(&source, b"one.txt\n", Some("shown")));
        std::fs::remove_dir_all(&directory).unwrap();
        if let Err(panic) = result { std::panic::resume_unwind(panic); }
    }

    #[test]
    fn original_stream_item_producers_execute_on_both_routes_after_frontend_drop() {
        execute_after_arena_disposal("stream rows(value: Int) [] -> Stream[Int] { yield value }\nproc shown() [io] { for value in rows(2) { print ${value.bit_and(1)} } }\nshown()\nlet values = rows(1)\nfor value in values { print ${value.bit_and(1)} }\n", b"0\n1\n", Some("shown"));
    }

    #[test]
    fn original_list_item_native_operand_executes_on_both_routes_after_frontend_drop() {
        execute_after_arena_disposal("proc quoted(values: List[Str]) [io] {\n for value in values { print ${shlex.quote(value)} }\n}\nquoted([\"one\", \"two\"])\n", b"one\ntwo\n", Some("quoted"));
    }

    #[test]
    fn original_str_item_native_operand_executes_on_both_routes_after_frontend_drop() {
        execute_after_arena_disposal("proc quoted(text: Str) [io] {\n for character in text { print ${shlex.quote(character)} }\n for character in \"ab\" { print ${character.byte_len()} }\n}\nquoted(\"xy\")\n", b"x\ny\n1\n1\n", Some("quoted"));
    }

    #[test]
    fn original_scalar_comprehension_producer_executes_on_both_routes_after_frontend_drop() {
        execute_after_arena_disposal("proc collected() [] -> List[Int] { [octet for character in \"ab\" for octet in b\"\\x01\\x02\" if character == \"a\"] }\nlet values = collected()\nprint ${values[0]} ${values[1]}\n", b"1 2\n", Some("collected"));
    }

    #[test]
    fn original_int_comprehension_generator_continuation_executes_on_both_routes_after_frontend_drop() {
        execute_after_arena_disposal("proc collected() [] -> List[Int] { [inner for outer in [1, 2] if outer > 0 for inner in [outer]] }\nlet values = collected()\nprint ${values[0]} ${values[1]}\n", b"1 2\n", Some("collected"));
    }

    #[test]
    fn original_int_list_item_native_operand_executes_on_both_routes_after_frontend_drop() {
        execute_after_arena_disposal("proc masked(values: List[Int]) [io] {\n for value in values { print ${value.bit_and(1)} }\n for value in [2, 3] { print ${value.bit_and(1)} }\n}\nmasked([4, 5])\n", b"0\n1\n0\n1\n", Some("masked"));
    }

    #[test]
    fn original_int_list_item_arithmetic_operand_executes_on_both_routes_after_frontend_drop() {
        execute_after_arena_disposal("proc shifted(values: List[Int]) [io] {\n for value in values { print ${value + 1} }\n}\nshifted([4, 5])\n", b"5\n6\n", Some("shifted"));
        execute_after_arena_disposal("proc summed(values: List[Int]) [] -> Int {\n var sum: Int = 0\n for value in values { sum = sum + value }\n sum\n}\nprint ${summed([4, 5])}\n", b"9\n", Some("summed"));
    }

    #[test]
    fn original_bytes_item_arithmetic_operand_executes_on_both_routes_after_frontend_drop() {
        execute_after_arena_disposal("proc octets(values: Bytes) [io] {\n for octet in values { print ${octet.bit_and(255)} }\n for octet in b\"\\x00\\xff\" { print ${octet + 1} }\n}\noctets(b\"\\x01\\xfe\")\n", b"1\n254\n1\n256\n", Some("octets"));
    }

    #[test]
    fn original_path_list_items_keep_opaque_native_operands_and_shadows_on_both_routes_after_frontend_drop() {
        execute_after_arena_disposal("proc displayed(values: List[Path]) [io] {\n for destination in values {\n print ${destination.display()}\n for destination in [p\"nested\"] { print ${destination.display()} }\n if true { let destination = p\"shadow\"; print ${destination.display()} }\n print ${destination.display()}\n }\n for destination in [p\"literal\"] { print ${destination.display()} }\n}\ndisplayed([p\"first\"])\n", b"first\nnested\nshadow\nfirst\nliteral\n", Some("displayed"));
        execute_after_arena_disposal("proc gathered(values: List[Path]) [] -> List[Path] {\n var entries: List[Path] = []\n for destination in values { entries += [destination] }\n entries\n}\nprint ${gathered([p\"first\"])[0].display()}\n", b"first\n", Some("gathered"));
    }

    #[test]
    fn original_result_bytes_item_projection_executes_on_both_routes_after_frontend_drop() {
        execute_after_arena_disposal("proc summed(values: Result[Bytes]) [error] -> Result[Int] {\n var sum: Int = 0\n for octet in values { sum = sum + octet }\n for octet in Ok(b\"\\x03\") { sum = sum + octet }\n sum\n}\nprint ${summed(Ok(b\"\\x01\\x02\"))?}\n", b"6\n", Some("summed"));
    }

    #[test]
    fn original_map_comprehension_keys_values_and_projected_items_execute_both_routes_after_frontend_drop() {
        execute_after_arena_disposal("proc mapped(values: Map[Str, Int]) [] -> Map[Str, Int] {\n {key: value for {key, value} in values if key != \"skip\"}\n}\nlet result = mapped({[\"keep\"]: 7, [\"skip\"]: 9})\nprint ${result[\"keep\"]}\n", b"7\n", Some("mapped"));
        execute_after_arena_disposal("proc collected() [] -> Map[Str, Int] {\n {inner_key: inner_value for {key, value} in {[\"outer\"]: 1} if key == \"outer\" for {key: inner_key, value: inner_value} in {[key]: value}}\n}\nlet result = collected()\nprint ${result[\"outer\"]}\n", b"1\n", Some("collected"));
    }

    #[test]
    fn original_list_item_uses_follow_nested_loops_and_restore_after_local_shadows() {
        crate::runtime::eval::run_eval(|| {
            let source = "proc quoted(values: List[Str]) [io] {\n for value in values {\n  print ${shlex.quote(value)}\n  for value in values { print ${shlex.quote(value)} }\n  if true { let value = \"shadow\"; print ${shlex.quote(value)} }\n  print ${shlex.quote(value)}\n }\n}\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("iteration-item-shadow.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
            let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            let mut loops = checked.solved.statement_operations.keys().filter_map(|identity| {
                let statement = parsed.arena.arena.stmt(identity.statement);
                let ArenaStmtKind::For { target, .. } = statement.kind else { return None; };
                Some((statement.span.start(), BindingIdentity { source: identity.source, namespace: identity.namespace, target }))
            }).collect::<Vec<_>>();
            loops.sort_by_key(|(start, _)| *start);
            assert_eq!(loops.len(), 2);
            let reads = source.match_indices("quote(value)").map(|(start, _)| {
                *checked.solved.expressions.keys().find(|identity| {
                    let expression = parsed.arena.arena.expr(identity.expression);
                    matches!(expression.kind, ArenaExprKind::Ident(_)) && expression.span.start() as usize == start + "quote(".len()
                }).expect("each actual native argument owns an original expression")
            }).collect::<Vec<_>>();
            assert_eq!(reads.len(), 4);
            let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
            let mut functions = Vec::new();
            lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources, StdlibLowerLinkage::Local, |unit| {
                assert!(unit.is_lowered(), "{:?}", unit.blocker_detail);
                functions.push(unit.body.unwrap()); Ok(())
            }).unwrap();
            drop(parsed);
            let scratch = functions[0].scratch.borrow();
            assert_eq!(scratch.iteration_binding_origins.len(), 2);
            let expected = [Some(loops[0].1), Some(loops[1].1), None, Some(loops[0].1)];
            for (read, binding) in reads.into_iter().zip(expected) {
                assert_eq!(scratch.iteration_binding_uses.get(&read).copied(), binding);
            }
            assert_ne!(scratch.iteration_binding_origins[&loops[0].1].slot, scratch.iteration_binding_origins[&loops[1].1].slot);
        });
    }

    #[test]
    fn original_list_item_parameter_ports_refuse_missing_and_foreign_expression_mappings() {
        crate::runtime::eval::run_eval(|| {
            let source = "proc quoted(first: List[Str], second: List[Str]) [io] {\n for left in first { print ${shlex.quote(left)} }\n for right in second { print ${shlex.quote(right)} }\n}\n";
            for missing in [true, false] {
                let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let mut declarations = Checker::check_compact_declarations(&parsed.arena);
                assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
                let mut bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
                assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
                parsed.arena.symbol_owner().with_current(|| {
                    let valid = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
                    assert_eq!(valid.blocker_events, 0);
                    let iterators = bodies.solved.statement_operations.keys().filter_map(|identity| {
                        let ArenaStmtKind::For { iter, .. } = parsed.arena.arena.stmt(identity.statement).kind else { return None; };
                        Some(ExpressionIdentity { source: identity.source, namespace: identity.namespace, expression: iter })
                    }).collect::<Vec<_>>();
                    assert_eq!(iterators.len(), 2);
                    declarations.solved = Default::default();
                    let solved = Arc::get_mut(&mut bodies.solved).expect("the mutation owns the original solved snapshot");
                    if missing {
                        solved.expression_producer_flows.remove(&iterators[0]);
                    } else {
                        let other = solved.expression_producer_flows[&iterators[1]];
                        solved.expression_producer_flows.insert(iterators[0], other);
                    }
                    let refused = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
                    assert!(refused.blocker_events > 0, "a parameter type cannot replace the original expression-to-formal producer port, missing={missing}");
                });
            }
        });
    }

    #[test]
    fn checked_iteration_domains_and_failure_transport_survive_arena_disposal() {
        crate::runtime::eval::run_eval(|| {
            let source = "stream numbers() -> Stream[Int] { yield 1 }\nfor item in [1] { let _ = item }\nfor item in numbers() { let _ = item }\nfor item in {[\"key\"]: 1} { let _ = item }\nfor item in \"word\" { let _ = item }\nfor item in b\"word\" { let _ = item }\nfor item in Ok([1]) { let _ = item }\nfor item in Ok(numbers()) { let _ = item }\nfor item in Ok({[\"key\"]: 1}) { let _ = item }\nfor item in Ok(\"word\") { let _ = item }\nfor item in Ok(b\"word\") { let _ = item }\n";
            let parsed = Parser::parse_source_arena_only(SourceId::new(51), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let roots = checked.solved.statement_operations.keys().filter_map(|identity| {
                let ArenaStmtKind::For { iter, .. } = parsed.arena.arena.stmt(identity.statement).kind else { return None; };
                Some((ProducerFlowSource::Statement(*identity), ExpressionIdentity { source: parsed.arena.arena.expr(iter).span.source_id, namespace: identity.namespace, expression: iter }))
            }).collect::<Vec<_>>();
            assert_eq!(roots.len(), 10);
            drop(parsed);
            let symbols = checked.solved.symbol_owner().clone(); let _symbols = symbols.enter();
            let mut selected = Vec::new();
            for (source, iterator) in roots {
                let fact = checked_iteration_fact(&checked.solved, source, iterator).expect("each original source keeps a selected iteration contract");
                assert!(checked.solved.graph.export_type(fact.item_type).is_ok());
                let metadata = checked.solved.operation_catalog.candidate(&checked.solved.graph, fact.candidate).unwrap();
                assert!(matches!(metadata, crate::sema::check::SolvedOperationAuthority::Language(candidate) if candidate.identity == fact.authority));
                assert!(checked_iteration_fact(&checked.solved, source, ExpressionIdentity { source: SourceId::new(52), ..iterator }).is_none());
                selected.push((fact.domain, fact.outer_result));
            }
            for domain in [IterableDomain::List, IterableDomain::Stream, IterableDomain::Map, IterableDomain::Str, IterableDomain::Bytes] {
                for outer_result in [false, true] { assert!(selected.contains(&(domain, outer_result))); }
            }
        });
    }

    #[test]
    fn comprehension_iteration_keeps_generator_ordinals_across_filters() {
        crate::runtime::eval::run_eval(|| {
            let source = "let pairs = [left + right for left in [1, 2] if left > 0 for right in [3, 4]]\n";
            let parsed = Parser::parse_source_arena_only(SourceId::new(53), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let roots = checked.solved.comprehension_operations.keys().map(|identity| {
                let ArenaExprKind::ListComp { qualifiers, .. } = parsed.arena.arena.expr(identity.expression.expression).kind else { panic!("fixture owns a list comprehension") };
                let crate::syntax::arena::ArenaCompQualifier::For { iter, .. } = parsed.arena.arena.comp_qualifiers(qualifiers)[identity.qualifier as usize] else { panic!("only actual generator ordinals own iteration proofs") };
                (*identity, ExpressionIdentity { expression: iter, ..identity.expression })
            }).collect::<Vec<_>>();
            assert_eq!(roots.iter().map(|(identity, _)| identity.qualifier).collect::<Vec<_>>(), vec![0, 2]);
            drop(parsed);
            let symbols = checked.solved.symbol_owner().clone(); let _symbols = symbols.enter();
            for (identity, iterator) in roots {
                let fact = checked_iteration_fact(&checked.solved, ProducerFlowSource::Comprehension(identity), iterator).unwrap();
                assert_eq!(fact.domain, IterableDomain::List);
                assert!(!fact.outer_result);
                assert!(checked_iteration_fact(&checked.solved, ProducerFlowSource::Comprehension(ComprehensionIdentity { qualifier: 1, ..identity }), iterator).is_none());
            }
        });
    }

    #[test]
    fn original_for_operation_cannot_be_reconstructed_from_expression_types() {
        crate::runtime::eval::run_eval(|| {
            let source = "for value in [1, 2] { print ${value} }\n";
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let mut declarations = Checker::check_compact_declarations(&parsed.arena);
            assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
            let mut bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
            parsed.arena.symbol_owner().with_current(|| {
                let valid = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
                assert_eq!(valid.blocker_events, 0, "checked iteration must lower before its proof is removed");
                let original = *bodies.solved.statement_operations.keys().find(|identity| {
                    matches!(parsed.arena.arena.stmt(identity.statement).kind, ArenaStmtKind::For { .. })
                }).expect("the checked loop retains its own operation");
                declarations.solved = Default::default();
                Arc::get_mut(&mut bodies.solved).expect("the fixture owns its solved snapshot").statement_operations.remove(&original);
                let absent = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
                assert!(absent.blocker_events > 0, "iteration without its original source operation must refuse lowering");
            });
        });
    }

    #[test]
    fn original_comprehension_operations_cannot_be_reconstructed_from_expression_types() {
        crate::runtime::eval::run_eval(|| {
            let source = "let pairs = [left + right for left in [1, 2] if left > 0 for right in [3, 4]]\n";
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let mut declarations = Checker::check_compact_declarations(&parsed.arena);
            assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
            let mut bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
            assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
            parsed.arena.symbol_owner().with_current(|| {
                let valid = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
                assert_eq!(valid.blocker_events, 0);
                let original = *bodies.solved.comprehension_operations.keys().find(|identity| identity.qualifier == 2).unwrap();
                declarations.solved = Default::default();
                Arc::get_mut(&mut bodies.solved).expect("the fixture owns its solved snapshot").comprehension_operations.remove(&original);
                let absent = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
                assert!(absent.constructed_top_level_statements < valid.constructed_top_level_statements, "the second generator must keep its own original source operation");
                assert!(absent.top_level_blockers.iter().any(|count| *count > 0), "the missing generator proof must make the containing source statement unavailable");
            });
        });
    }

    #[test]
    fn original_yield_delegation_operation_cannot_be_reconstructed_from_expression_types() {
        crate::runtime::eval::run_eval(|| {
            let source = "stream copied() -> Stream[Int] { yield @[1, 2] }\nlet values = copied() |> collect\n";
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("checked-delegation.xsh", source);
            let parsed = Parser::parse_source_arena_only(source_id, source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let mut checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            parsed.arena.symbol_owner().with_current(|| {
                let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources.clone());
                assert!(evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).is_ok(), "the original delegation prepares before its proof is removed");
                drop(evaluator);
                let original = *checked.solved.statement_operations.keys().find(|identity| matches!(parsed.arena.arena.stmt(identity.statement).kind, ArenaStmtKind::YieldDelegate(_))).unwrap();
                Arc::get_mut(&mut checked.solved).expect("the fixture owns its solved snapshot").statement_operations.remove(&original);
                let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
                assert!(evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked).is_err(), "delegation without its own source operation must not produce a prepared program");
            });
        });
    }

    #[test]
    fn checked_iteration_executes_all_carriers_and_nested_targets_after_arena_disposal() {
        let source = r#"stream numbers() -> Stream[Int] { yield 1 }
for item in [1] { print ${item} }
for item in numbers() { print ${item} }
for {value} in {["key"]: 1} { print ${value} }
for item in "a" { print ${item} }
for item in b"A" { print ${item} }
for item in Ok([1]) { print ${item} }
for item in Ok(numbers()) { print ${item} }
for {value} in Ok({["key"]: 1}) { print ${value} }
for item in Ok("a") { print ${item} }
for item in Ok(b"A") { print ${item} }
for {nested: {tag}} in [{nested: {tag: 7}}] { print ${tag} }
let pairs = [left + right for left in [1, 2] if left > 0 for right in [3, 4]]
let rows = {key: value for {key, value} in {["row"]: 9}}
print ${pairs[0]} ${pairs[3]} ${rows["row"]}
"#;
        execute_after_arena_disposal(source, b"1\n1\n1\na\n65\n1\n1\n1\na\n65\n7\n4 6 9\n", None);
    }

    #[test]
    fn checked_stream_iteration_keeps_pull_and_cleanup_lazy_after_arena_disposal() {
        let source = r#"stream numbers() [io] -> Stream[Int] {
    defer { print "close" }
    print "pull"
    yield 1
    yield 2
}
let values = numbers()
print "created"
for value in values { print ${value}; break }
print "done"
"#;
        execute_after_arena_disposal(source, b"created\npull\n1\nclose\ndone\n", None);
    }

    fn execute_after_arena_disposal(source: &str, expected: &[u8], observed_function: Option<&'static str>) {
        execute_after_arena_disposal_with_scans(source, expected, observed_function, None);
    }

    fn execute_after_arena_disposal_with_scans(source: &str, expected: &[u8], observed_function: Option<&'static str>, expected_scans: Option<usize>) {
        let source = source.to_owned();
        let expected = expected.to_vec();
        crate::runtime::eval::run_eval(move || {
            for recursive in [false, true] {
                let mut sources = SourceMap::new();
                let source_id = sources.add_file("checked-iteration.xsh", source.as_str());
                let parsed = Parser::parse_source_arena_only(source_id, &source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let checked = Checker::check_arena(&parsed.arena, &source);
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                let solved = Arc::downgrade(&checked.solved);
                let mut evaluator = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources);
                let plan = parsed.arena.symbol_owner().with_current(|| evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked))
                    .expect("original checked iteration must prepare an entirely indexed program");
                if let Some(expected_scans) = expected_scans {
                    let program = evaluator.indexed_program.as_ref().unwrap();
                    let emitted = (0..program.function_count()).map(|index| program.function_view_at(index).unwrap().instruction_tags().unwrap().iter().filter(|&&tag| tag == crate::runtime::eval::indexed::full::FullTag::StmtScanLines).count()).sum::<usize>();
                    assert_eq!(emitted, expected_scans, "the observed workers must execute the actual fused scanner");
                    assert_eq!(program.generic_evidence().unwrap().line_scans().count(), expected_scans);
                }
                let symbols = evaluator.indexed_program.as_ref().expect("preparation installs indexed code").symbol_owner().clone();
                drop(checked);
                drop(parsed);
                assert!(solved.upgrade().is_none(), "prepared iteration releases its original inference bundle");
                let evaluated = crate::runtime::eval::run_eval(move || symbols.with_current(|| {
                    let execute = || {
                        assert_eq!(crate::runtime::eval::lowered_run::recursive_fast_path_forced(), recursive);
                        evaluator.try_eval_installed_compact_indexed_only_inner(plan)
                    };
                    if let Some(function) = observed_function {
                        crate::runtime::eval::lowered_run::with_observed_indexed_call_route(crate::runtime::eval::LoweredFunctionKey::Name(Name::intern(function)), recursive, execute)
                    } else if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(execute) }
                    else { execute() }
                }));
                let output = match evaluated { Ok(output) => output, Err(_) => panic!("prepared iteration must execute after arena disposal") };
                assert_eq!(output.status, 0);
                assert_eq!(output.stdout, expected, "{}", String::from_utf8_lossy(&output.stderr));
                assert!(output.stderr.is_empty(), "{:?}", output.stderr);
                assert!(output.traceback.is_none(), "{:?}", output.traceback);
            }
        });
    }
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildComprehensionGenerator {
    pub identity: ComprehensionIdentity,
    pub iterator_source: ExpressionIdentity,
    pub iterator: BuildExprId,
    pub selected: CandidateId,
    pub input: crate::sema::inference::ScopedRoot,
    pub item: crate::sema::inference::ScopedRoot,
    pub bindings: Vec<BuildComprehensionBinding>,
    pub target: crate::runtime::eval::indexed::generic::ComprehensionTarget,
    pub input_flow: crate::sema::check::ProducerFlowId,
    pub item_flow: crate::sema::check::ProducerFlowId,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildComprehensionBinding {
    pub identity: BindingIdentity,
    pub ty: crate::sema::inference::ScopedRoot,
    pub slot: usize,
    pub path: Vec<Name>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildComprehensionRoot {
    pub source: ExpressionIdentity,
    pub row: BuildExprId,
    pub ty: crate::sema::inference::ScopedRoot,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct BuildComprehensionOrigin {
    pub origin: ExpressionIdentity,
    pub result: crate::sema::inference::ScopedRoot,
    pub value: BuildComprehensionRoot,
    pub key: Option<BuildComprehensionRoot>,
    pub generators: Vec<BuildComprehensionGenerator>,
    pub filters: Vec<(u32, BuildComprehensionRoot)>,
    pub reads: Vec<(BuildComprehensionRoot, BindingIdentity)>,
}

// Original producer edges identify the lexical binding before names disappear.
// A slot or an equal item type cannot substitute another generator's item.
fn comprehension_read_binding(solved: &SolvedTypes, source: ExpressionIdentity) -> Option<BindingIdentity> {
    use crate::sema::check::ProducerFlowKind;
    let mut flow = *solved.expression_producer_flows.get(&source)?;
    if solved.producer_flows.node(flow).ok()?.source != ProducerFlowSource::Expression(source) { return None; }
    for _ in 0..=256 {
        let node = solved.producer_flows.node(flow).ok()?;
        if let ProducerFlowSource::Binding { identity, version: 0 } = node.source { return Some(identity); }
        match &node.kind {
            ProducerFlowKind::CapturedBinding { identity, version: 0, input } => {
                if solved.producer_flows.node(*input).ok()?.source != (ProducerFlowSource::Binding { identity: *identity, version: 0 }) { return None; }
                flow = *input;
            }
            ProducerFlowKind::Join { inputs } => { let [input] = inputs.as_slice() else { return None; }; flow = *input; }
            _ => return None,
        }
    }
    None
}

impl CompactLowerConstructProbe<'_, '_> {
    fn checked_comprehension_target(&self, target: BindingTargetId, lowered: &LoweredCompTarget, source: SourceId, owner: Option<crate::sema::check::DeclarationIdentity>, scope: Option<crate::sema::inference::SchemeId>, path: &mut Vec<Name>, bindings: &mut Vec<BuildComprehensionBinding>) -> Option<crate::runtime::eval::indexed::generic::ComprehensionTarget> {
        use crate::runtime::eval::indexed::generic::ComprehensionTarget;
        match (&self.program.arena.binding_target(target).kind, lowered) {
            (ArenaBindingTargetKind::Name(name), LoweredCompTarget::Discard) if is_discard_name(*name) => Some(ComprehensionTarget::Discard),
            (ArenaBindingTargetKind::Name(name), LoweredCompTarget::Slot(slot)) if !is_discard_name(*name) => {
                if self.program.arena.binding_target(target).span.is_some_and(|span| self.program.arena.span(span).source_id != source) { return None; }
                let identity = BindingIdentity { source, namespace: self.current_namespace, target };
                let definition = self.solved().bindings.get(&identity)?;
                if definition.mutable || definition.owner != owner { return None; }
                let ty = crate::sema::inference::ScopedRoot { ty: definition.ty, scope: definition.scheme.or(scope) };
                self.solved().graph.validate_scoped(ty).ok()?;
                self.solved().graph.export_type(ty.ty).ok()?;
                let index = u32::try_from(bindings.len()).ok()?;
                bindings.push(BuildComprehensionBinding { identity, ty, slot: *slot, path: path.clone() });
                Some(ComprehensionTarget::Slot(index))
            }
            (ArenaBindingTargetKind::Record { fields, .. }, LoweredCompTarget::Record { fields: lowered }) => {
                let source_fields = self.program.arena.destructure_fields(*fields);
                if source_fields.len() != lowered.len() { return None; }
                let mut result = Vec::new();
                for (field, (name, target, span)) in source_fields.iter().zip(lowered) {
                    if field.name != *name || self.program.arena.span(field.span) != *span || span.source_id != source { return None; }
                    path.push(*name);
                    let child = self.checked_comprehension_target(field.target, target, source, owner, scope, path, bindings)?;
                    path.pop();
                    result.push((*name, child));
                }
                Some(ComprehensionTarget::Record(result))
            }
            _ => None,
        }
    }

    pub(super) fn record_checked_comprehension(&self, expression: ExprId, row: BuildExprId) -> Option<()> {
        use crate::sema::inference::ScopedRoot;
        let (key_source, value_source, source_qualifiers) = match self.program.arena.expr(expression).kind {
            ArenaExprKind::ListComp { expr, qualifiers } => (None, expr, qualifiers),
            ArenaExprKind::MapComp { key, value, qualifiers } => (Some(key), value, qualifiers),
            _ => return None,
        };
        let origin = self.expression_identity(expression);
        let solved = self.solved();
        let owner = solved.expression_owners.get(&origin).copied();
        let root = |source: ExpressionIdentity, row| -> Option<BuildComprehensionRoot> {
            let ty = ScopedRoot { ty: *solved.expressions.get(&source)?, scope: solved.expression_scope(source, owner).ok()? };
            solved.graph.validate_scoped(ty).ok()?;
            solved.graph.export_type(ty.ty).ok()?;
            Some(BuildComprehensionRoot { source, row, ty })
        };
        let result = crate::sema::inference::ScopedRoot { ty: *solved.expressions.get(&origin)?, scope: solved.expression_scope(origin, owner).ok()? };
        solved.graph.validate_scoped(result).ok()?;
        if solved.graph.export_type(result.ty).is_err() { return Some(()); }
        let mut scratch = self.scratch.borrow_mut();
        let (key, value, qualifiers) = match scratch.expressions.get(row.index())? {
            BuildExprRow::ListComp { value, qualifiers, .. } => (None, *value, qualifiers),
            BuildExprRow::MapComp { key, value, qualifiers, .. } => (Some(*key), *value, qualifiers),
            _ => return None,
        };
        let value = root(self.expression_identity(value_source), value)?;
        let key = match (key_source, key) { (None, None) => None, (Some(source), Some(row)) => Some(root(self.expression_identity(source), row)?), _ => return None };
        let source_qualifiers = self.program.arena.comp_qualifiers(source_qualifiers);
        if source_qualifiers.len() != qualifiers.0.len() { return None; }
        let mut generators = Vec::new();
        let mut filters = Vec::new();
        for (ordinal, (source, lowered)) in source_qualifiers.iter().zip(&qualifiers.0).enumerate() {
            match (source, lowered) {
                (crate::syntax::arena::ArenaCompQualifier::For { target, iter, .. }, LoweredCompQualifier::For { target: lowered_target, iter: iterator, .. }) => {
                    let identity = ComprehensionIdentity { expression: origin, qualifier: ordinal as u32 };
                    let projection = self.checked_iteration_projection(CheckedIterationOrigin::Comprehension { expression, qualifier: ordinal as u32 }, *iter)?;
                    if projection.outer_result || !matches!(projection.domain, IterableDomain::List | IterableDomain::Str | IterableDomain::Bytes | IterableDomain::Map) { return Some(()); }
                    let clause = solved.comprehension_operations.get(&identity)?;
                    if clause.operation.caller != owner { return None; }
                    let input = ScopedRoot { ty: *solved.expressions.get(&projection.iterator)?, scope: solved.expression_scope(projection.iterator, owner).ok()? };
                    solved.graph.validate_scoped(input).ok()?;
                    if solved.graph.export_type(input.ty).is_err() { return Some(()); }
                    let item = ScopedRoot { ty: clause.operation.result, scope: solved.operation_scope(ProducerFlowSource::Comprehension(identity), &clause.operation).ok()? };
                    solved.graph.validate_scoped(item).ok()?;
                    if solved.graph.export_type(item.ty).is_err() { return Some(()); }
                    let mut bindings = Vec::new();
                    let target = self.checked_comprehension_target(*target, lowered_target, origin.source, owner, item.scope, &mut Vec::new(), &mut bindings)?;
                    generators.push(BuildComprehensionGenerator { identity, iterator_source: projection.iterator, iterator: *iterator, selected: projection.candidate, input, item, bindings, target, input_flow: clause.input_producer_flow, item_flow: clause.item_producer_flow });
                }
                (crate::syntax::arena::ArenaCompQualifier::If { condition, .. }, LoweredCompQualifier::If { condition: lowered, .. }) => {
                    let condition = root(self.expression_identity(*condition), *lowered)?;
                    if solved.graph.export_type(condition.ty.ty).ok()? != Type::Bool { return None; }
                    if matches!(scratch.expressions.get(lowered.index()), Some(BuildExprRow::Binary { .. })) {
                        let Some(operation) = solved.operations.get(&condition.source) else {
                            let ArenaExprKind::Binary { left, right, .. } = self.program.arena.expr(condition.source.expression).kind else { return None; };
                            let left = *solved.expressions.get(&self.expression_identity(left))?;
                            let right = *solved.expressions.get(&self.expression_identity(right))?;
                            if [left, right].iter().any(|ty| solved.graph.export_type(*ty) == Ok(Type::Any)) { return Some(()); }
                            return None;
                        };
                        let selected = solved.graph.candidate_evidence(operation.requirement).ok()??;
                        let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate).ok()? else { return Some(()); };
                        let prepared = match metadata.operation {
                            PreparedLanguageOperation::Equality { .. } => selected.actual_arguments.iter().all(|argument| argument.is_some_and(|ty| solved.graph.export_type(ty) == Ok(Type::Str))),
                            PreparedLanguageOperation::Ordering { left: crate::sema::inference::Atom::Str, right: crate::sema::inference::Atom::Str, .. }
                            | PreparedLanguageOperation::Ordering { left: crate::sema::inference::Atom::Int, right: crate::sema::inference::Atom::Int, .. } => true,
                            _ => false,
                        };
                        if !prepared { return Some(()); }
                    }
                    filters.push((ordinal as u32, condition));
                }
                _ => return None,
            }
        }
        let expected = match &key { Some(key) => Type::Map(Box::new(solved.graph.export_type(key.ty.ty).ok()?), Box::new(solved.graph.export_type(value.ty.ty).ok()?)), None => Type::List(Box::new(solved.graph.export_type(value.ty.ty).ok()?)) };
        if solved.graph.export_type(result.ty).ok()? != expected { return None; }
        let span = self.program.arena.expr(expression).span;
        let mut reads = Vec::new();
        for (&row, &source) in &self.expression_origins {
            if source.source != origin.source || source.namespace != origin.namespace { continue; }
            let original = self.program.arena.expr(source.expression);
            if original.span.start() < span.start() || original.span.end() > span.end() || !matches!(original.kind, ArenaExprKind::Ident(_)) { continue; }
            let Some(binding) = comprehension_read_binding(solved, source) else { continue; };
            let Some(binding) = generators.iter().flat_map(|generator| &generator.bindings).find(|candidate| candidate.identity == binding) else { continue; };
            if !matches!(scratch.expressions.get(row.index()), Some(BuildExprRow::Param(slot)) if *slot == binding.slot) { return None; }
            reads.push((root(source, row)?, binding.identity));
        }
        if scratch.comprehension_origins.insert(row, BuildComprehensionOrigin { origin, result, value, key, generators, filters, reads }).is_some() { return None; }
        Some(())
    }
}
