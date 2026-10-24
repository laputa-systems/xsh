use super::{ArenaCallArg, ArenaProgram, Checker, DiagnosticCode, HostCallSource, MethodReceiver, MethodSig, Span, Type, api_spec};
use crate::sema::arguments::{ExpandedArgument, StaticArgumentBinding};
use crate::sema::builtin_templates::BuiltinInstantiation;
use crate::sema::constraints::{ConstraintConflict, ConstraintResolutionError, TypeConstraints};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::OnceLock;

#[derive(Clone, Copy)]
struct HostMethod {
    receiver: MethodReceiver,
    signature: &'static MethodSig,
}

fn method_candidates(name: &str) -> Vec<HostMethod> {
    static INDEX: OnceLock<BTreeMap<&'static str, Vec<HostMethod>>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut index = BTreeMap::<_, Vec<_>>::new();
        for (receiver, methods) in api_spec().method_entries() {
            if receiver == MethodReceiver::PathConstructor {
                continue;
            }
            for method in methods {
                index.entry(method.name).or_default().extend(method.overloads.iter().map(|signature| HostMethod { receiver, signature }));
            }
        }
        index
    }).get(name).cloned().unwrap_or_default()
}

#[derive(Clone, Default)]
pub(in super::super) struct HostObligations {
    entries: Vec<HostObligation>,
    next_position: usize,
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
struct HostCallPosition(usize);

impl HostObligations {
    fn reserve_position(&mut self) -> HostCallPosition {
        let position = HostCallPosition(self.next_position);
        self.next_position = self.next_position.checked_add(1).expect("host call positions fit their owning worklist");
        position
    }
}

#[derive(Clone)]
struct SelectedHostMethod {
    method: HostMethod,
    signature: super::ModuleFnSig,
    binding: StaticArgumentBinding,
}

#[derive(Clone)]
struct HostCallPhase {
    position: HostCallPosition,
    context: super::super::infer_effects::HostMethodContext,
}

struct HostCallStart {
    owner: Option<super::super::EffectDeclarationId>,
    source: HostCallSource,
    before_arguments: HostCallPhase,
    name: String,
    receiver_schema: Option<crate::sema::constants::SchemaExpectation>,
}

impl HostCallStart {
    fn finish(self, checker: &mut Checker, arguments: Vec<super::super::args::CheckedArgumentContext>) -> HostCallContinuation {
        let after_arguments = checker.reserve_host_call_phase();
        HostCallContinuation {
            owner: self.owner, source: self.source, before_arguments: self.before_arguments, after_arguments,
            name: self.name, receiver_schema: self.receiver_schema, arguments,
        }
    }
}

#[derive(Clone)]
struct HostCallContinuation {
    owner: Option<super::super::EffectDeclarationId>,
    source: HostCallSource,
    before_arguments: HostCallPhase,
    after_arguments: HostCallPhase,
    name: String,
    receiver_schema: Option<crate::sema::constants::SchemaExpectation>,
    arguments: Vec<super::super::args::CheckedArgumentContext>,
}

#[derive(Clone)]
struct HostObligation {
    continuation: HostCallContinuation,
    candidates: HostMethodCandidates,
}

#[derive(Clone)]
struct HostMethodCandidates {
    selected: Option<SelectedHostMethod>,
    receiver: Type,
    result: Type,
    expected: Option<Type>,
    arguments: Vec<ExpandedArgument>,
    candidates: Vec<HostMethod>,
    span: Span,
}

impl HostCallContinuation {
    fn phase(&self, method: HostMethod, arguments: &[ExpandedArgument]) -> Option<&HostCallPhase> {
        if !matches!(self.source, HostCallSource::Expression(_))
            || !matches!(method.signature.sig.arg_check, super::ApiArgCheck::Standard | super::ApiArgCheck::PathLikeSingle | super::ApiArgCheck::ResultContext)
        { return None; }
        let schemas = crate::sema::builtin_templates::parameter_schema_contexts(&method.signature.sig, method.signature.receiver_ty.as_ref(), self.receiver_schema.as_ref());
        let params = crate::sema::builtin_templates::callable_parameters(&method.signature.sig);
        let binding = crate::sema::arguments::bind_static_arguments(&params, arguments).ok()?;
        assert_eq!(arguments.len(), self.arguments.len());
        if self.arguments.iter().zip(&binding.argument_slots).any(|(context, &slot)| context.schema != schemas.get(slot).cloned().flatten().unwrap_or_default()) { return None; }
        match super::method_with_unique_layout(
            api_spec().method_overloads(method.receiver, &self.name).expect("a selected host method remains registered"),
            Some(arguments),
        ) {
            Some(selected) => std::ptr::eq(selected, method.signature).then_some(&self.before_arguments),
            None => Some(&self.after_arguments),
        }
    }
}

impl HostObligation {
    fn dependencies(&self, arena: &ArenaProgram, constraints: &TypeConstraints) -> Result<BTreeSet<Span>, ConstraintResolutionError> {
        let mut dependencies = self.candidates.dependencies(constraints)?;
        match &self.continuation.source {
            HostCallSource::Expression(call) => {
                assert_eq!(arena.arena.expr(*call).span, self.candidates.span);
            }
            HostCallSource::Callback { callee, argument, argument_span } => {
                assert_eq!(arena.arena.expr(*callee).span.source_id, argument_span.source_id);
                dependencies.extend(constraints.unresolved(argument)?);
            }
        }
        Ok(dependencies)
    }
}

impl HostMethodCandidates {
    fn dependencies(&self, constraints: &TypeConstraints) -> Result<BTreeSet<Span>, ConstraintResolutionError> {
        std::iter::once(&self.receiver)
            .chain(std::iter::once(&self.result))
            .chain(self.expected.as_ref())
            .chain(self.arguments.iter().map(|argument| &argument.ty))
            .map(|ty| constraints.unresolved(ty))
            .collect::<Result<Vec<_>, _>>()
            .map(|dependencies| dependencies.into_iter().flatten().collect())
    }

    fn common_closed_result(&self) -> Option<&Type> {
        let result = &self.candidates.first()?.signature.sig.return_ty;
        if result.contains_any() || result.contains_recovery() || result.contains_inference()
            || result.contains_builtin_parameter() || result.contains_scheme_callable()
        { return None; }
        self.candidates.iter().all(|candidate| candidate.signature.sig.return_ty == *result).then_some(result)
    }

    fn method_family_requirement(&self, name: &str) -> Option<(xsh_registry::signature::MethodFamilyId, Vec<u32>, Type, &'static MethodSig)> {
        let first = *self.candidates.first()?;
        let family = api_spec().method_family_id(first.receiver, name, first.signature)?;
        let policy = api_spec().method_family_result(family)?;
        let result = match &policy {
            crate::modules::signature::MethodFamilyResult::UniformGround(result) => result.clone(),
            crate::modules::signature::MethodFamilyResult::SelectedSignature => self.result.clone(),
        };
        let mut slots = None;
        for candidate in &self.candidates {
            if api_spec().method_family_id(candidate.receiver, name, candidate.signature) != Some(family)
                || matches!(&policy, crate::modules::signature::MethodFamilyResult::UniformGround(result) if candidate.signature.sig.return_ty != *result)
                || candidate.signature.sig.pure != first.signature.sig.pure
                || candidate.signature.sig.effects != first.signature.sig.effects
            {
                return None;
            }
            let params = crate::sema::builtin_templates::callable_parameters(&candidate.signature.sig);
            let binding = crate::sema::arguments::bind_static_arguments(&params, &self.arguments).ok()?;
            let candidate_slots = binding.argument_slots.into_iter().map(u32::try_from).collect::<Result<Vec<_>, _>>().ok()?;
            if slots.as_ref().is_some_and(|slots| slots != &candidate_slots) {
                return None;
            }
            slots = Some(candidate_slots);
        }
        Some((family, slots?, result, first.signature))
    }

    fn constrain_candidate(
        &self,
        candidate: HostMethod,
        constraints: &mut TypeConstraints,
        include_result: bool,
    ) -> Result<SelectedHostMethod, ConstraintConflict> {
        let mut instance = BuiltinInstantiation::new(
            &candidate.signature.sig,
            candidate.signature.receiver_ty.as_ref(),
            Some(&self.receiver),
            constraints,
            self.span,
        )?;
        let params = crate::sema::builtin_templates::callable_parameters(&instance.signature);
        let binding = crate::sema::arguments::bind_static_arguments(&params, &self.arguments)
            .expect("candidate arguments retain their validated layout");
        for (argument, slot) in self.arguments.iter().zip(binding.argument_slots.iter().copied()) {
            constraints.constrain_context(&params[slot].ty, &argument.ty, argument.span)?;
        }
        if include_result {
            if let Some(expected) = &self.expected {
                instance.constrain_result(expected, constraints, self.span)?;
            }
            constraints.constrain(&self.result, &instance.signature.return_ty, self.span)?;
        }
        Ok(SelectedHostMethod { method: candidate, signature: instance.signature, binding })
    }

    fn select(&mut self, constraints: &mut TypeConstraints, name: &str) -> Result<bool, ConstraintResolutionError> {
        let selected_family = self.method_family_requirement(name).and_then(|(family, slots, _, _)| {
            matches!(api_spec().method_family_result(family), Some(crate::modules::signature::MethodFamilyResult::SelectedSignature)).then_some((family, slots))
        });
        let mut matching = Vec::new();
        for candidate in std::mem::take(&mut self.candidates) {
            let params = crate::sema::builtin_templates::callable_parameters(&candidate.signature.sig);
            let mut arguments_match =
                crate::sema::arguments::bind_static_arguments(&params, &self.arguments).is_ok();
            if candidate.signature.sig.arg_check == super::ApiArgCheck::PathLikeSingle {
                arguments_match &= match self.arguments.as_slice() {
                    [argument] => {
                        let actual = constraints.resolve(&argument.ty)?;
                        super::super::expr::is_path_like_type(&actual) || matches!(actual, Type::Inference(_))
                    }
                    _ => false,
                };
            }
            let candidate_matches = if arguments_match {
                constraints.record_host_candidate_trial();
                constraints.trial(|constraints| self.constrain_candidate(candidate, constraints, false).map(|_| ()))?
            } else {
                false
            };
            if candidate_matches {
                matching.push(candidate);
            }
        }
        let mut deferred_result = selected_family.is_some();
        if let Some((family, slots)) = &selected_family {
            let receiver = constraints.resolve(&self.receiver)?;
            let arguments = self.arguments.iter().map(|argument| constraints.resolve(&argument.ty)).collect::<Result<Vec<_>, _>>()?;
            if !receiver.contains_inference() && !arguments.iter().any(Type::contains_inference) {
                deferred_result = false;
                match api_spec().select_method_family(*family, &receiver, slots, &arguments) {
                    crate::modules::signature::MethodFamilySelection::Selected(id) => {
                        let (domain, _, signature) = api_spec().registered_method(id).expect("selected method remains registered");
                        matching.retain(|candidate| candidate.receiver == domain && std::ptr::eq(candidate.signature, signature));
                    }
                    _ => matching.clear(),
                }
            }
        }
        let competing_domains = matching.iter().filter_map(|candidate| {
            (matching.iter().filter(|other| other.receiver == candidate.receiver).count() > 1)
                .then_some(candidate.receiver)
        }).collect::<Vec<_>>();
        for candidate in matching {
            let candidate_matches = if deferred_result || competing_domains.contains(&candidate.receiver) {
                true
            } else {
                constraints.record_host_candidate_trial();
                constraints.trial(|constraints| self.constrain_candidate(candidate, constraints, true).map(|_| ()))?
            };
            if candidate_matches {
                self.candidates.push(candidate);
            }
        }
        if let [candidate] = self.candidates.as_slice()
            && candidate.signature.receiver_ty.is_some()
        {
            match self.constrain_candidate(*candidate, constraints, true) {
                Ok(selected) => { self.selected = Some(selected); return Ok(true); },
                Err(conflict) => match conflict.resolution_error {
                    Some(error) => return Err(error),
                    None => panic!("a selected host candidate must reproduce its successful trial"),
                },
            }
        }
        Ok(false)
    }

}

impl Checker {
    fn reserve_host_call_phase(&mut self) -> HostCallPhase {
        HostCallPhase {
            position: self.local_inference.host_obligations.reserve_position(),
            context: self.host_method_context(),
        }
    }

    fn reserve_host_call_continuation(
        &mut self,
        source: HostCallSource,
        name: &str,
        receiver_schema: Option<crate::sema::constants::SchemaExpectation>,
    ) -> HostCallStart {
        let before_arguments = self.reserve_host_call_phase();
        HostCallStart { owner: self.stream_origin_owner, source, before_arguments, name: name.to_owned(), receiver_schema }
    }

    pub(in super::super) fn check_inferred_host_method(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        call_source: HostCallSource,
        base: crate::syntax::arena::ExprId,
        receiver: Type,
        name: &str,
        args: &[ArenaCallArg],
        span: Span,
        expected: Option<&Type>,
    ) -> Type {
        let receiver_schema = self.schema_expectation_for_expr(arena, base);
        let continuation = self.reserve_host_call_continuation(call_source.clone(), name, receiver_schema);
        let candidates = method_candidates(name);
        let registered = !candidates.is_empty();
        let signatures = candidates.iter().map(|method| method.signature.clone()).collect::<Vec<_>>();
        let contexts = super::method_argument_contexts(arena, args, &signatures, continuation.receiver_schema.as_ref());
        let Some(mut values) = self.check_argument_values_arena(arena, source, args, contexts) else { return Type::Invalid; };
        self.retain_original_method_arguments(arena, &call_source, &mut values);
        let continuation = continuation.finish(self, values.contexts);
        let arguments = values.arguments;
        let result_level = self.callable_solver_level();
        let candidate_result = self.type_constraints.fresh_at_level(span, result_level);
        let mut obligation = HostMethodCandidates { selected: None, receiver: receiver.clone(), result: candidate_result.clone(), expected: expected.cloned(), arguments, candidates, span };
        match obligation.select(&mut self.type_constraints, name) {
            Ok(true) => {
                if obligation.selected.as_ref().is_some_and(|selected| continuation.phase(selected.method, &obligation.arguments).is_some()) {
                    return self.finalize_host_obligation(arena, &mut HostObligation { continuation, candidates: obligation });
                }
                let receiver = match self.type_constraints.resolve(&receiver) {
                    Ok(receiver) => receiver,
                    Err(error) => return self.reject_host_inference_resolution(error, span),
                };
                return self.check_method_dispatch_arena(arena, source, call_source, base, receiver, name, args, span, expected);
            }
            Ok(false) => {}
            Err(error) => return self.reject_host_inference_resolution(error, span),
        }
        if self.collecting_local_constraints() {
            if let Some(result) = obligation.common_closed_result() {
                self.expect_type(&candidate_result, result, span);
            }
            let family_result = continuation.owner.filter(|_| {
                matches!(continuation.source, HostCallSource::Expression(_))
                    && !obligation.receiver.contains_any()
                    && !obligation.receiver.contains_recovery()
                    && obligation.arguments.iter().all(|argument| !argument.ty.contains_any() && !argument.ty.contains_recovery())
            }).and_then(|_| obligation.method_family_requirement(&continuation.name).map(|(_, _, result, _)| result));
            if let Some(owner) = continuation.owner
                && matches!(continuation.source, HostCallSource::Expression(_))
                && let Some((family, argument_slots, result, _)) = obligation.method_family_requirement(name)
                && matches!(api_spec().method_family_result(family), Some(crate::modules::signature::MethodFamilyResult::SelectedSignature))
            {
                let requirement = crate::sema::schemes::MethodFamilyRequirement {
                    family, receiver: obligation.receiver.clone(), argument_slots,
                    arguments: obligation.arguments.iter().map(|argument| argument.ty.clone()).collect(), result,
                };
                if let Err(error) = self.type_constraints.register_method_family(self.constraint_scope_at(owner, span), requirement, Some(span), span) {
                    return self.reject_host_inference_resolution(error, span);
                }
            }
            self.local_inference.host_obligations.entries.push(HostObligation { continuation, candidates: obligation });
            family_result.unwrap_or(candidate_result)
        } else {
            self.reject_unresolved_host_method(&obligation, registered);
            Type::Invalid
        }
    }

    pub(in super::super) fn method_result_matches_written_path_return(&self, expected: &Type, actual: &Type) -> Result<bool, ConstraintResolutionError> {
        let Some(owner) = self.stream_origin_owner else { return Ok(false); };
        let Some(pending) = self.declaration_inference.pending(super::super::FunctionDeclarationId::new(owner)) else { return Ok(false); };
        let Some(shape) = pending.provisional() else { return Ok(false); };
        if !shape.explicit_return { return Ok(false); }
        let actual = match (pending.kind(), &shape.return_ty, expected, actual) {
            (crate::sema::types::CallableKind::Pure, Type::Path, Type::Path, actual) => actual,
            (crate::sema::types::CallableKind::Proc, Type::Result(ok, _), Type::Path, actual) if **ok == Type::Path => actual,
            (crate::sema::types::CallableKind::Proc, header @ Type::Result(ok, _), expected, actual)
                if **ok == Type::Path && expected == header && !matches!(actual, Type::Result(_, _)) => actual,
            (crate::sema::types::CallableKind::Proc, Type::Result(ok, _), Type::Result(expected_ok, expected_error), Type::Result(actual_ok, actual_error))
                if **ok == Type::Path && **expected_ok == Type::Path && actual_error.matches_expected(expected_error) => actual_ok,
            _ => return Ok(false),
        };
        self.type_constraints.owned_method_result_is_path(crate::sema::constraints::CallableApplicationScope::Declaration(owner), actual)
    }

    fn reject_unresolved_host_method(&mut self, candidates: &HostMethodCandidates, registered: bool) {
        let (message, code) = if !registered {
            ("no registered method has this name", DiagnosticCode::CheckUnknownMethod)
        } else if candidates.candidates.is_empty() {
            ("no registered method matches this body contract", DiagnosticCode::CheckTypeMismatch)
        } else if candidates.candidates.len() == 1 && candidates.candidates[0].signature.receiver_ty.is_none() {
            ("registered method receiver needs an annotation", DiagnosticCode::CheckLocalInference)
        } else {
            ("method receiver has no unique registered operation", DiagnosticCode::CheckAmbiguousOverload)
        };
        self.error(candidates.span, message, code);
    }

    pub(in super::super) fn close_unresolved_host_obligations(&mut self, owner: Option<super::super::EffectDeclarationId>, body: Span) {
        let pending = std::mem::take(&mut self.local_inference.host_obligations.entries);
        for obligation in pending {
            let span = obligation.candidates.span;
            if obligation.continuation.owner == owner && obligation.candidates.selected.is_none()
                && (owner.is_some() || (span.source_id == body.source_id && body.start() <= span.start() && span.end() <= body.end()))
            {
                if let Some(owner) = owner
                    && matches!(&obligation.continuation.source, HostCallSource::Expression(_))
                    && let Some((family, argument_slots, result, representative)) = obligation.candidates.method_family_requirement(&obligation.continuation.name)
                    && !obligation.candidates.receiver.contains_any()
                    && !obligation.candidates.receiver.contains_recovery()
                    && obligation.candidates.arguments.iter().all(|argument| !argument.ty.contains_any() && !argument.ty.contains_recovery())
                {
                    let body_result = self.type_constraints.constrain(&obligation.candidates.result, &result, span);
                    let expected_result = if body_result.is_ok() {
                        obligation.candidates.expected.as_ref().map(|expected| {
                            match self.method_result_matches_written_path_return(expected, &result) {
                                Ok(true) => Ok(false),
                                Ok(false) => crate::sema::builtin_templates::BuiltinInstantiation::constrain_result_type(expected, &result, &mut self.type_constraints, span),
                                Err(error) => Err(ConstraintConflict { expected: expected.clone(), actual: result.clone(), initializer: None, established: None, contribution: span, resolution_error: Some(error) }),
                            }
                        })
                    } else {
                        None
                    };
                    let result_matches = body_result.is_ok() && expected_result.as_ref().is_none_or(Result::is_ok);
                    if result_matches {
                        let requirement = crate::sema::schemes::MethodFamilyRequirement {
                            family,
                            receiver: obligation.candidates.receiver.clone(),
                            argument_slots,
                            arguments: obligation.candidates.arguments.iter().map(|argument| argument.ty.clone()).collect(),
                            result: result.clone(),
                        };
                        match self.type_constraints.register_method_family(
                            self.constraint_scope_at(owner, span), requirement, Some(span), span,
                        ) {
                            Ok(_) => self.finalize_host_method_effects(&obligation.continuation.after_arguments.context, &representative.sig, &obligation.continuation.name, span),
                            Err(error) => self.error(span, &format!("method family could not be retained: {error:?}"), DiagnosticCode::CheckLocalInference),
                        }
                    } else {
                        self.error(span, "method family result does not satisfy its body contract", DiagnosticCode::CheckTypeMismatch);
                        self.expr_types.insert(obligation.candidates.span, Type::Invalid);
                    }
                } else {
                    self.reject_unresolved_host_method(&obligation.candidates, !method_candidates(&obligation.continuation.name).is_empty());
                    self.expr_types.insert(obligation.candidates.span, Type::Invalid);
                }
            } else {
                self.local_inference.host_obligations.entries.push(obligation);
            }
        }
    }

    fn finalize_host_obligation(&mut self, arena: &ArenaProgram, obligation: &mut HostObligation) -> Type {
        let continuation = &obligation.continuation;
        let HostCallSource::Expression(call) = continuation.source else { panic!("callback methods retain their canonical argument checker") };
        assert_eq!(arena.arena.expr(call).span, obligation.candidates.span);
        assert!(matches!(arena.arena.expr(call).kind, crate::syntax::arena::ArenaExprKind::Call { .. }));
        let selected = obligation.candidates.selected.take().expect("a finalized host call retains the committed selection");
        let phase = continuation.phase(selected.method, &obligation.candidates.arguments).expect("a finalized host call has canonical selection order");
        self.finalize_selected_host_method(
            arena, selected.method.receiver, &continuation.name, selected.method.signature, selected.signature,
            super::super::args::SelectedMethodArguments::Values {
                values: super::super::args::CheckedArgumentValues { arguments: obligation.candidates.arguments.clone(), contexts: continuation.arguments.clone() },
                binding: selected.binding,
                schemas: crate::sema::builtin_templates::parameter_schema_contexts(&selected.method.signature.sig, selected.method.signature.receiver_ty.as_ref(), continuation.receiver_schema.as_ref()),
            }, &phase.context, obligation.candidates.span,
        )
    }

    pub(in super::super) fn solve_host_obligations(&mut self, arena: &ArenaProgram) -> Result<(), ConstraintResolutionError> {
        let mut pending = std::mem::take(&mut self.local_inference.host_obligations.entries);
        let mut watchers = BTreeMap::<Span, BTreeSet<usize>>::new();
        let mut queue = (0..pending.len()).collect::<VecDeque<_>>();
        let mut queued = (0..pending.len()).collect::<BTreeSet<_>>();
        for (index, obligation) in pending.iter().enumerate() {
            for dependency in obligation.dependencies(arena, &self.type_constraints)? {
                watchers.entry(dependency).or_default().insert(index);
            }
        }
        while let Some(index) = queue.pop_front() {
            queued.remove(&index);
            if pending[index].candidates.selected.is_some() { continue; }
            let obligation = &mut pending[index];
            let dependencies = obligation.dependencies(arena, &self.type_constraints)?;
            if obligation.candidates.select(&mut self.type_constraints, &obligation.continuation.name)? {
                for dependency in dependencies {
                    if let Some(dependents) = watchers.remove(&dependency) {
                        for dependent in dependents {
                            if pending[dependent].candidates.selected.is_none() && queued.insert(dependent) {
                                self.type_constraints.record_host_obligation_requeue();
                                queue.push_back(dependent);
                            }
                        }
                    }
                }
            } else {
                for dependency in dependencies {
                    watchers.entry(dependency).or_default().insert(index);
                }
            }
        }
        pending.sort_by_key(|obligation| obligation.candidates.selected.as_ref()
            .and_then(|selected| obligation.continuation.phase(selected.method, &obligation.candidates.arguments))
            .unwrap_or(&obligation.continuation.before_arguments).position);
        let mut inserted = 0;
        for mut obligation in pending {
            Self::shift_host_method_context(&mut obligation.continuation.before_arguments.context, inserted);
            Self::shift_host_method_context(&mut obligation.continuation.after_arguments.context, inserted);
            match &obligation.candidates.selected {
                Some(selected) if obligation.continuation.phase(selected.method, &obligation.candidates.arguments).is_some() => {
                    let before = self.diagnostics.len();
                    self.finalize_host_obligation(arena, &mut obligation);
                    inserted += self.diagnostics.len() - before;
                }
                _ => self.local_inference.host_obligations.entries.push(obligation),
            }
        }
        Ok(())
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed_obligation(
        checker: &mut Checker,
        arena: &ArenaProgram,
        statement_index: usize,
        name: &str,
        mut candidates: HostMethodCandidates,
    ) -> HostObligation {
        let statement = arena.arena.stmt_ids(arena.statements).nth(statement_index).unwrap();
        let crate::syntax::arena::ArenaStmtKind::Expr(call) = arena.arena.stmt(statement).kind else { panic!("host call statement") };
        let crate::syntax::arena::ArenaExprKind::Call { args, .. } = arena.arena.expr(call).kind else { panic!("host call") };
        candidates.span = arena.arena.expr(call).span;
        let mut types = candidates.arguments.iter().map(|argument| argument.ty.clone()).collect::<Vec<_>>().into_iter();
        candidates.arguments = crate::sema::arguments::expand_named_arguments(arena, arena.arena.call_args(args), |_| types.next()).unwrap();
        assert!(types.next().is_none());
        HostObligation {
            continuation: checker.reserve_host_call_continuation(HostCallSource::Expression(call), name, None).finish(checker, vec![super::super::super::args::CheckedArgumentContext::default(); candidates.arguments.len()]),
            candidates,
        }
    }

    #[test]
    fn method_dispatch_preserves_receiver_resolution_error_categories() {
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), "\"x\"\n");
        parsed.arena.symbol_owner().with_current(|| {
            let span = Span::at(crate::source::SourceId::new(0), 0);
            let base = crate::syntax::arena::ExprId::from_index(0);
            let mut checker = Checker::new(crate::sema::check::CheckOptions::default());
            let mut deep = checker.type_constraints.fresh(span);
            for _ in 0..300 {
                deep = Type::List(Box::new(deep));
            }
            checker.check_method_dispatch_arena(&parsed.arena, "\"x\"\n", HostCallSource::Callback { callee: base, argument: Type::Unit, argument_span: span }, base, deep, "trim", &[], span, None);
            assert!(checker.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckLocalInference)));
            let mut foreign = TypeConstraints::default();
            let receiver = foreign.fresh(span);
            assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                checker.check_method_dispatch_arena(&parsed.arena, "\"x\"\n", HostCallSource::Callback { callee: base, argument: Type::Unit, argument_span: span }, base, receiver, "trim", &[], span, None);
            })).is_err());
        });
    }

    #[test]
    fn host_candidate_trials_preserve_resolution_error_categories() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let span = Span::at(crate::source::SourceId::new(0), 0);
            let string = &api_spec().method_overloads(MethodReceiver::Str, "trim").unwrap()[0];
            let push = &api_spec().method_overloads(MethodReceiver::List, "push").unwrap()[0];
            let mut constraints = TypeConstraints::default();
            let receiver = constraints.fresh(span);
            let mut deep = constraints.fresh(span);
            for _ in 0..300 {
                deep = Type::List(Box::new(deep));
            }
            let mut foreign = TypeConstraints::default();
            for (actual_receiver, arguments, candidate, expected) in [
                (receiver.clone(), vec![ExpandedArgument { entry_index: 0, name: None, value: crate::sema::arguments::ArgumentValueSource::CheckedType, ty: deep, span }], HostMethod { receiver: MethodReceiver::List, signature: push }, ConstraintResolutionError::TypeDepth),
                (foreign.fresh(span), Vec::new(), HostMethod { receiver: MethodReceiver::Str, signature: string }, ConstraintResolutionError::ForeignVariable),
            ] {
                let mut obligation = HostMethodCandidates { selected: None,
                    receiver: actual_receiver,
                    result: Type::Str,
                    expected: None,
                    arguments,
                    candidates: vec![candidate],
                    span,
                };
                assert_eq!(obligation.select(&mut constraints, "trim"), Err(expected));
                assert_eq!(constraints.resolve(&receiver), Ok(receiver.clone()));
            }
        });
    }

    #[test]
    fn host_result_context_does_not_choose_overloads_within_one_receiver_domain() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let span = Span::at(crate::source::SourceId::new(0), 0);
            let string = &api_spec().method_overloads(crate::modules::signature::MethodReceiver::Str, "trim").unwrap()[0];
            let mut bytes = string.clone();
            bytes.sig.return_ty = Type::Bytes;
            let bytes = Box::leak(Box::new(bytes));
            let mut constraints = TypeConstraints::default();
            let receiver = constraints.fresh(span);
            let mut obligation = HostMethodCandidates { selected: None,
                receiver: receiver.clone(),
                result: Type::Str,
                expected: Some(Type::Str),
                arguments: Vec::new(),
                candidates: vec![HostMethod { receiver: MethodReceiver::Str, signature: string }, HostMethod { receiver: MethodReceiver::Str, signature: bytes }],
                span,
            };
            assert_eq!(obligation.select(&mut constraints, "trim"), Ok(false));
            assert_eq!(obligation.candidates.len(), 2);
            assert_eq!(constraints.resolve(&receiver), Ok(receiver));
            assert_eq!(constraints.metrics().host_candidate_trials, 2);
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), "");
            let overloads = [string.clone(), bytes.clone()];
            for context in [Type::Str, Type::Bytes] {
                let mut checker = Checker::new(crate::sema::check::CheckOptions::default());
                checker.boundary.current_return = Some(crate::sema::check::ReturnContext::Function(context));
                checker.choose_method_sig_arena(&parsed.arena, "", "trim", &[], &overloads, span);
                assert!(checker.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckAmbiguousOverload)));
            }
        });
    }

    #[test]
    fn host_candidate_metrics_exclude_the_committed_selection() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let span = Span::at(crate::source::SourceId::new(0), 0);
            let signature = &api_spec().method_overloads(MethodReceiver::Str, "trim").unwrap()[0];
            let mut constraints = TypeConstraints::default();
            let receiver = constraints.fresh(span);
            let result = constraints.fresh(span);
            let mut obligation = HostMethodCandidates { selected: None,
                receiver: receiver.clone(),
                result: result.clone(),
                expected: None,
                arguments: Vec::new(),
                candidates: vec![HostMethod { receiver: MethodReceiver::Str, signature }],
                span,
            };

            assert_eq!(obligation.select(&mut constraints, "trim"), Ok(true));
            assert_eq!(obligation.candidates.len(), 1);
            assert_eq!(constraints.resolve(&receiver), Ok(Type::Str));
            assert_eq!(constraints.resolve(&result), Ok(Type::Str));
            assert_eq!(constraints.metrics().host_candidate_trials, 2);
            assert_eq!(constraints.metrics().host_obligation_requeues, 0);
        });
    }

    #[test]
    fn host_obligation_metrics_count_deduplicated_dependency_requeues() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), "first.byte_at(0)\nsecond.byte_at(0)\n");
            assert!(parsed.diagnostics.is_empty());
            let receiver_span = Span::at(crate::source::SourceId::new(0), 0);
            let result_span = Span::at(crate::source::SourceId::new(0), 1);
            let mut checker = Checker::new(crate::sema::check::CheckOptions::default());
            let receiver = checker.type_constraints.fresh(receiver_span);
            let byte_result = checker.type_constraints.fresh(result_span);
            let byte_at = [MethodReceiver::Bytes, MethodReceiver::Str]
                .map(|receiver_domain| HostMethod {
                    receiver: receiver_domain,
                    signature: &api_spec()
                        .method_overloads(receiver_domain, "byte_at")
                        .expect("registered byte_at receiver domain")[0],
                });
            let candidates = HostMethodCandidates { selected: None,
                receiver: receiver.clone(),
                result: byte_result.clone(),
                expected: None,
                arguments: vec![ExpandedArgument {
                    entry_index: 0,
                    name: None,
                    value: crate::sema::arguments::ArgumentValueSource::CheckedType,
                    ty: Type::Int,
                    span: receiver_span,
                }],
                candidates: byte_at.into(),
                span: receiver_span,
            };
            let obligation = parsed_obligation(&mut checker, &parsed.arena, 0, "byte_at", candidates);
            checker.local_inference.host_obligations.entries.push(obligation);
            let candidates = HostMethodCandidates { selected: None,
                receiver: receiver.clone(),
                result: byte_result.clone(),
                expected: None,
                arguments: vec![ExpandedArgument {
                    entry_index: 0,
                    name: None,
                    value: crate::sema::arguments::ArgumentValueSource::CheckedType,
                    ty: Type::Int,
                    span: receiver_span,
                }],
                candidates: vec![HostMethod {
                    receiver: MethodReceiver::Str,
                    signature: &api_spec()
                        .method_overloads(MethodReceiver::Str, "byte_at")
                        .expect("registered Str.byte_at method")[0],
                }],
                span: receiver_span,
            };
            let obligation = parsed_obligation(&mut checker, &parsed.arena, 1, "byte_at", candidates);
            checker.local_inference.host_obligations.entries.push(obligation);

            assert_eq!(checker.solve_host_obligations(&parsed.arena), Ok(()));
            assert_eq!(checker.type_constraints.resolve(&receiver), Ok(Type::Str));
            assert_eq!(
                checker.type_constraints.resolve(&byte_result),
                Ok(Type::Optional(Box::new(Type::Int)))
            );
            assert!(checker.local_inference.host_obligations.entries.is_empty());
            let metrics = checker.type_constraints.metrics();
            assert_eq!(metrics.host_candidate_trials, 9);
            assert_eq!(metrics.host_obligation_requeues, 1);
        });
    }

    #[test]
    fn late_host_selection_publishes_the_original_call_binding() {
        let source = "raw.byte_at(0)\n";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let _symbols = parsed.arena.symbol_owner().enter();
        let statement = parsed.arena.arena.stmt_ids(parsed.arena.statements).next().unwrap();
        let crate::syntax::arena::ArenaStmtKind::Expr(call) = parsed.arena.arena.stmt(statement).kind else { panic!("call statement") };
        let crate::syntax::arena::ArenaExprKind::Call { args, .. } = parsed.arena.arena.expr(call).kind else { panic!("original call") };
        let span = parsed.arena.arena.expr(call).span;
        let mut checker = Checker::new(crate::sema::check::CheckOptions::default());
        let receiver = checker.type_constraints.fresh(span);
        let result = checker.type_constraints.fresh(span);
        let arguments = crate::sema::arguments::expand_named_arguments(&parsed.arena, parsed.arena.arena.call_args(args), |expression| {
            Some(checker.check_expr_arena(&parsed.arena, source, expression, None))
        }).unwrap();
        assert!(matches!(arguments[0].value, crate::sema::arguments::ArgumentValueSource::Expression(_)));
        let continuation = checker.reserve_host_call_continuation(HostCallSource::Expression(call), "byte_at", None).finish(&mut checker, vec![super::super::super::args::CheckedArgumentContext::default(); arguments.len()]);
        checker.local_inference.host_obligations.entries.push(HostObligation { continuation, candidates: HostMethodCandidates { selected: None,
            receiver: receiver.clone(), result: result.clone(), expected: None, arguments,
            candidates: method_candidates("byte_at"), span,
        } });
        assert_eq!(checker.solve_host_obligations(&parsed.arena), Ok(()));
        assert_eq!(checker.local_inference.host_obligations.entries.len(), 1);
        assert!(checker.api_calls.is_empty());
        checker.type_constraints.constrain(&receiver, &Type::Str, span).unwrap();
        assert_eq!(checker.solve_host_obligations(&parsed.arena), Ok(()));
        assert!(checker.diagnostics.is_empty(), "{:?}", checker.diagnostics);
        assert_eq!(checker.type_constraints.resolve(&result), Ok(Type::Optional(Box::new(Type::Int))));
        let evidence = checker.api_calls.get(&span).expect("the solved original call publishes its selected operation");
        assert_eq!(evidence.receiver, Some(MethodReceiver::Str));
        assert_eq!(evidence.params, [Type::Int]);
        assert_eq!(evidence.argument_slots, [0]);
        assert!(checker.local_inference.host_obligations.entries.is_empty());
    }

    #[test]
    fn late_host_selection_keeps_named_spread_sources() {
        let source = "raw.replace(...{from: \"b\", with: \"X\"})\n";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let _symbols = parsed.arena.symbol_owner().enter();
        let statement = parsed.arena.arena.stmt_ids(parsed.arena.statements).next().unwrap();
        let crate::syntax::arena::ArenaStmtKind::Expr(call) = parsed.arena.arena.stmt(statement).kind else { panic!("host call") };
        let crate::syntax::arena::ArenaExprKind::Call { args, .. } = parsed.arena.arena.expr(call).kind else { panic!("host call") };
        let span = parsed.arena.arena.expr(call).span;
        let mut checker = Checker::new(crate::sema::check::CheckOptions::default());
        let arguments = crate::sema::arguments::expand_named_arguments(&parsed.arena, parsed.arena.arena.call_args(args), |expression| Some(checker.check_expr_arena(&parsed.arena, source, expression, None))).unwrap();
        assert_eq!(arguments.len(), 2);
        assert!(arguments.iter().all(|argument| matches!(argument.value, crate::sema::arguments::ArgumentValueSource::RecordField { .. })));
        let receiver = checker.type_constraints.fresh(span);
        let result = checker.type_constraints.fresh(span);
        let continuation = checker.reserve_host_call_continuation(HostCallSource::Expression(call), "replace", None).finish(&mut checker, vec![super::super::super::args::CheckedArgumentContext::default(); arguments.len()]);
        checker.local_inference.host_obligations.entries.push(HostObligation {
            continuation,
            candidates: HostMethodCandidates { selected: None, receiver: receiver.clone(), result: result.clone(), expected: None, arguments, candidates: method_candidates("replace"), span },
        });
        checker.type_constraints.constrain(&receiver, &Type::Str, span).unwrap();
        assert_eq!(checker.solve_host_obligations(&parsed.arena), Ok(()));
        assert!(checker.diagnostics.is_empty(), "{:?}", checker.diagnostics);
        let call = checker.api_calls.get(&span).unwrap();
        assert_eq!(call.receiver, Some(MethodReceiver::Str));
        assert_eq!(call.params, [Type::Str, Type::Str]);
        assert_eq!(call.argument_slots, [0, 1]);
        assert_eq!(checker.type_constraints.resolve(&result), Ok(Type::Str));
    }

    #[test]
    fn late_host_effects_use_the_original_boundary() {
        for (source, name, receiver_kind, receiver_type) in [
            ("proc original() { raw.resolve() }\nproc elsewhere() {}\n", "resolve", MethodReceiver::Path, Type::Path),
            ("proc original() { raw.write(\"data\") }\nproc elsewhere() {}\n", "write", MethodReceiver::Path, Type::Path),
            ("proc original() { raw.prepend(p\"data\") }\nproc elsewhere() {}\n", "prepend", MethodReceiver::EnvPathList, Type::EnvPathList),
            ("proc original() { raw.append(p\"data\") }\nproc elsewhere() {}\n", "append", MethodReceiver::EnvPathList, Type::EnvPathList),
        ] {
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let _symbols = parsed.arena.symbol_owner().enter();
        let statement = parsed.arena.arena.stmt_ids(parsed.arena.statements).next().unwrap();
        let crate::syntax::arena::ArenaStmtKind::ProcDef(definition) = parsed.arena.arena.stmt(statement).kind else { panic!("original owner") };
        let body = parsed.arena.arena.function_def(definition).body;
        let statement = parsed.arena.arena.stmt_ids(parsed.arena.arena.block(body).statements).next().unwrap();
        let crate::syntax::arena::ArenaStmtKind::Expr(call) = parsed.arena.arena.stmt(statement).kind else { panic!("host call") };
        let elsewhere = parsed.arena.arena.stmt_ids(parsed.arena.statements).nth(1).unwrap();
        let crate::syntax::arena::ArenaStmtKind::ProcDef(other_definition) = parsed.arena.arena.stmt(elsewhere).kind else { panic!("solver owner") };
        let crate::syntax::arena::ArenaExprKind::Call { callee, args } = parsed.arena.arena.expr(call).kind else { panic!("host call") };
        let args = parsed.arena.arena.call_args(args);
        let crate::syntax::arena::ArenaExprKind::Field { base, .. } = parsed.arena.arena.expr(callee).kind else { panic!("method receiver") };
        let span = parsed.arena.arena.expr(call).span;
        let method = &api_spec().method_overloads(receiver_kind, name).unwrap()[0];
        assert!(!method.sig.pure);
        let required = method.sig.effects.to_vec();
        assert!(!required.is_empty());
        for (pure, allowed) in [(false, false), (false, true), (true, true)] {
            let mut original = Checker::new(crate::sema::check::CheckOptions::default());
            let owner = original.effect_declaration_id(&parsed.arena, body);
            let other = original.effect_declaration_id(&parsed.arena, parsed.arena.arena.function_def(other_definition).body);
            original.effect_graph.declare(owner, "original".into(), true, None);
            original.effect_graph.declare(other, "elsewhere".into(), true, None);
            original.boundary.effect_owner = Some(owner);
            original.record_effects = true;
            original.defer_effect_diagnostics = true;
            original.boundary.in_pure = pure;
            original.boundary.current_effects = Some(if allowed { required.clone() } else { Vec::new() });
            let mut delayed = original.clone();
            original.check_method_dispatch_arena(&parsed.arena, source, HostCallSource::Expression(call), base, receiver_type.clone(), name, args, span, None);
            original.validate_deferred_effects();
            let receiver = delayed.type_constraints.fresh(span);
            let result = delayed.type_constraints.fresh(span);
            let continuation = delayed.reserve_host_call_continuation(HostCallSource::Expression(call), name, None);
            let arguments = crate::sema::arguments::expand_named_arguments(&parsed.arena, args, |expression| {
                Some(delayed.check_expr_arena(&parsed.arena, source, expression, None))
            }).unwrap();
            let continuation = continuation.finish(&mut delayed, vec![super::super::super::args::CheckedArgumentContext::default(); arguments.len()]);
            delayed.local_inference.host_obligations.entries.push(HostObligation {
                continuation,
                candidates: HostMethodCandidates { selected: None, receiver: receiver.clone(), result, expected: None, arguments, candidates: method_candidates(name), span },
            });
            delayed.boundary.effect_owner = Some(other);
            delayed.boundary.in_pure = false;
            delayed.boundary.current_effects = Some(required.clone());
            delayed.type_constraints.constrain(&receiver, &receiver_type, span).unwrap();
            assert_eq!(delayed.solve_host_obligations(&parsed.arena), Ok(()));
            delayed.validate_deferred_effects();
            let diagnostics = |checker: &Checker| checker.diagnostics.iter().map(|diagnostic| (diagnostic.code, diagnostic.message.clone())).collect::<Vec<_>>();
            assert_eq!(diagnostics(&delayed), diagnostics(&original));
            assert_eq!(delayed.diagnostics.is_empty(), allowed && !pure);
            assert_eq!(delayed.boundary.effect_owner, Some(other));
            let expected_graph = original.effect_graph.solve_with_external(&BTreeMap::new()).unwrap();
            let actual_graph = delayed.effect_graph.solve_with_external(&BTreeMap::new()).unwrap();
            assert_eq!(actual_graph[&owner].effects(), expected_graph[&owner].effects());
            assert_eq!(actual_graph[&other].effects(), expected_graph[&other].effects());
            assert!(!delayed.boundary.in_pure);
            assert_eq!(delayed.boundary.current_effects, Some(required.clone()));
        }
        }
    }

    #[test]
    fn late_host_nested_diagnostics_keep_the_original_order() {
        let source = "p\"outer\".mkdir(parents: p\"inner\".exists() ?? false)\n";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let _symbols = parsed.arena.symbol_owner().enter();
        let statement = parsed.arena.arena.stmt_ids(parsed.arena.statements).next().unwrap();
        let crate::syntax::arena::ArenaStmtKind::Expr(outer) = parsed.arena.arena.stmt(statement).kind else { panic!("outer host call") };
        let crate::syntax::arena::ArenaExprKind::Call { args, .. } = parsed.arena.arena.expr(outer).kind else { panic!("outer host call") };
        let crate::syntax::arena::ArenaCallArgKind::Named { value, .. } = parsed.arena.arena.call_args(args)[0].kind else { panic!("parents argument") };
        let crate::syntax::arena::ArenaExprKind::Binary { left: inner, .. } = parsed.arena.arena.expr(value).kind else { panic!("existing boolean fallback") };
        let mut reference = Checker::new(crate::sema::check::CheckOptions::default());
        reference.boundary.in_pure = true;
        let mut delayed = reference.clone();
        reference.check_expr_arena(&parsed.arena, source, outer, None);
        let outer_continuation = delayed.reserve_host_call_continuation(HostCallSource::Expression(outer), "mkdir", None);
        let inner_continuation = delayed.reserve_host_call_continuation(HostCallSource::Expression(inner), "exists", None).finish(&mut delayed, Vec::new());
        let outer_continuation = outer_continuation.finish(&mut delayed, vec![super::super::super::args::CheckedArgumentContext::default(); 1]);
        for (call, name, continuation, arguments) in [
            (inner, "exists", inner_continuation, Vec::new()),
            (outer, "mkdir", outer_continuation, crate::sema::arguments::expand_named_arguments(&parsed.arena, parsed.arena.arena.call_args(args), |_| Some(Type::Bool)).unwrap()),
        ] {
            let span = parsed.arena.arena.expr(call).span;
            let result = delayed.type_constraints.fresh(span);
            delayed.local_inference.host_obligations.entries.push(HostObligation {
                continuation,
                candidates: HostMethodCandidates { selected: None, receiver: Type::Path, result, expected: None, arguments, candidates: method_candidates(name), span },
            });
        }
        assert_eq!(delayed.solve_host_obligations(&parsed.arena), Ok(()));
        let order = |checker: &Checker| checker.diagnostics.iter().filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckPureEffect)).map(|diagnostic| diagnostic.labels.iter().map(|label| label.span).collect::<Vec<_>>()).collect::<Vec<_>>();
        assert_eq!(order(&reference).len(), 2, "{:?}", reference.diagnostics);
        assert_eq!(order(&delayed), order(&reference), "{:?}", delayed.diagnostics);
    }

    #[test]
    fn known_receiver_overloads_check_argument_effects_once() {
        let source = "p\"outer\".write(p\"inner\".read_text() ?? \"\")\n";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let _symbols = parsed.arena.symbol_owner().enter();
        let statement = parsed.arena.arena.stmt_ids(parsed.arena.statements).next().unwrap();
        let crate::syntax::arena::ArenaStmtKind::Expr(outer) = parsed.arena.arena.stmt(statement).kind else { panic!("outer host call") };
        let crate::syntax::arena::ArenaExprKind::Call { args, .. } = parsed.arena.arena.expr(outer).kind else { panic!("outer host call") };
        let crate::syntax::arena::ArenaCallArgKind::Positional(value) = parsed.arena.arena.call_args(args)[0].kind else { panic!("write argument") };
        let crate::syntax::arena::ArenaExprKind::Binary { left: inner, .. } = parsed.arena.arena.expr(value).kind else { panic!("read fallback") };
        let mut checker = Checker::new(crate::sema::check::CheckOptions::default());
        checker.boundary.in_pure = true;
        let (work, _) = crate::sema::inference_work::collect(|| checker.check_expr_arena(&parsed.arena, source, outer, None));
        assert_eq!(work.function_body_check_entries, 0);
        let order = checker.diagnostics.iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckPureEffect))
            .flat_map(|diagnostic| diagnostic.labels.iter().map(|label| label.span))
            .collect::<Vec<_>>();
        assert_eq!(order, [parsed.arena.arena.expr(inner).span, parsed.arena.arena.expr(outer).span]);
        assert!(checker.api_calls.contains_key(&parsed.arena.arena.expr(inner).span));
        assert!(checker.api_calls.contains_key(&parsed.arena.arena.expr(outer).span));
    }

    #[test]
    fn registered_method_arguments_retain_receiver_schema_evidence() {
        for (definition, argument, expected) in [
            ("type Box[T] = {value: Int}\n", "Box(value: 1)", Type::Str),
            ("type Box[T] = {value: Int}\ntype Alias = Box[List[Int]]\n", "Box(value: 1)", Type::List(Box::new(Type::Int))),
            ("type Box[T] = {value: T}\n", "Box(value: [])", Type::List(Box::new(Type::Int))),
        ] {
            let annotation = if definition.contains("type Alias") { "Alias" } else if expected == Type::Str { "Box[Str]" } else { "Box[List[Int]]" };
            let source = format!("{definition}let values: List[{annotation}] = []\nlet _ = values.push({argument})\n");
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let _symbols = parsed.arena.symbol_owner().enter();
            let (work, checked) = crate::sema::inference_work::collect(|| Checker::check_arena_with_options(&parsed.arena, &source, crate::sema::check::CheckOptions::for_execution()));
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            assert_eq!(work.function_body_check_entries, 0);
            let constructor = checked.record_constructor_instances.iter().find(|(span, _)| &source[span.range()] == argument).expect("the original contextual constructor publishes its actual schema instance");
            assert_eq!(constructor.1.instance.arguments, [expected]);
            assert_eq!(parsed.arena.arena.type_def(constructor.1.instance.definition).name.as_str().as_str(), "Box");
            let call = checked.api_calls.iter().find(|(span, _)| source[span.range()].starts_with("values.push(")).expect("the selected List push publishes its checked binding").1;
            assert_eq!(call.receiver, Some(MethodReceiver::List));
            assert_eq!(call.argument_slots, [0]);
            assert_eq!(call.params.as_slice(), std::slice::from_ref(&constructor.1.ty));
        }
        for source in [
            "type Box[T] = {value: T}\nlet values: List[Box[Int]] = []\nlet _ = values.push(Box(value: false))\n",
            "type Box[T] = {value: T}\nlet values: List[Box[List[Int]]] = []\nlet stored: List[Str] = [\"wrong\"]\nlet _ = values.push(Box(value: stored))\n",
        ] {
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions::for_execution());
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error), "{source}: {:?}", checked.diagnostics);
        }
    }

    #[test]
    fn registered_method_arguments_retain_requirement_targets() {
        let source = "type Box[T] = {value: Int}\nlet values: List[Box[Str]] = []\nlet incoming: Any = {value: 7}\nlet _ = values.push(incoming.require()?)\n";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let _symbols = parsed.arena.symbol_owner().enter();
        let checked = Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions::for_execution());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let target = checked.requirement_targets.iter().find(|(span, _)| &source[span.range()] == "incoming.require()").expect("the original require expression retains its contextual target").1;
        assert_eq!(target.context.instances.len(), 1);
        assert_eq!(target.context.instances[0].arguments, [Type::Str]);
        assert_eq!(parsed.arena.arena.type_def(target.context.instances[0].definition).name.as_str().as_str(), "Box");
        let call = checked.api_calls.iter().find(|(span, _)| source[span.range()].starts_with("values.push(")).expect("the enclosing method retains its actual selected parameter").1;
        assert_eq!(call.params.as_slice(), std::slice::from_ref(&target.ty));
    }

    #[test]
    fn late_result_context_methods_preserve_original_result_contract() {
        for (arguments_source, accepted) in [
            ("\"wrapped\"", true),
            ("\"wrapped\", \"extra\"", true),
            ("kind: \"wrapped\", message: \"extra\"", true),
            ("message: \"extra\", kind: \"wrapped\"", true),
            ("message: \"extra\"", false),
            ("false", false),
            ("\"wrapped\", false", false),
            ("", false),
            ("\"wrapped\", \"extra\", 7", false),
        ] {
            let source = format!("error TestBaseError = Base\nlet raw: Result[Int, TestBaseError] = Err(TestBaseError.Base(\"original\"))\nlet _ = raw.context({arguments_source})\n");
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let _symbols = parsed.arena.symbol_owner().enter();
            let statement = parsed.arena.arena.stmt_ids(parsed.arena.statements).last().unwrap();
            let crate::syntax::arena::ArenaStmtKind::Let { initializer: crate::syntax::arena::ArenaExprOrRun::Expr(call), .. } = parsed.arena.arena.stmt(statement).kind else { panic!("context call") };
            let crate::syntax::arena::ArenaExprKind::Call { callee, args } = parsed.arena.arena.expr(call).kind else { panic!("context call") };
            let crate::syntax::arena::ArenaExprKind::Field { base, .. } = parsed.arena.arena.expr(callee).kind else { panic!("context receiver") };
            let span = parsed.arena.arena.expr(call).span;
            let reference = Checker::check_arena_with_options(&parsed.arena, &source, crate::sema::check::CheckOptions::for_execution());
            let actual_receiver = reference.expr_types[&parsed.arena.arena.expr(base).span].clone();
            assert!(matches!(&actual_receiver, Type::Result(ok, error) if **ok == Type::Int && **error == Type::ErrorFamily(crate::symbol::Name::intern("TestBaseError"))));
            let mut delayed = Checker::new(crate::sema::check::CheckOptions::default());
            let continuation = delayed.reserve_host_call_continuation(HostCallSource::Expression(call), "context", None);
            let arguments = crate::sema::arguments::expand_named_arguments(&parsed.arena, parsed.arena.arena.call_args(args), |expression| {
                Some(delayed.check_expr_arena(&parsed.arena, &source, expression, Some(&Type::Str)))
            }).unwrap();
            let continuation = continuation.finish(&mut delayed, vec![super::super::super::args::CheckedArgumentContext::default(); arguments.len()]);
            let receiver = delayed.type_constraints.fresh(span);
            let result = delayed.type_constraints.fresh(span);
            delayed.local_inference.host_obligations.entries.push(HostObligation {
                continuation,
                candidates: HostMethodCandidates { selected: None, receiver: receiver.clone(), result: result.clone(), expected: None, arguments, candidates: method_candidates("context"), span },
            });
            delayed.type_constraints.constrain(&receiver, &actual_receiver, span).unwrap();
            assert_eq!(delayed.solve_host_obligations(&parsed.arena), Ok(()));
            if accepted {
                assert!(reference.diagnostics.is_empty(), "{source}: {:?}", reference.diagnostics);
                assert!(delayed.diagnostics.is_empty(), "{source}: {:?}", delayed.diagnostics);
                assert_eq!(delayed.type_constraints.resolve(&result), Ok(actual_receiver.clone()));
                assert_eq!(reference.expr_types[&span], actual_receiver);
                let actual = delayed.api_calls.get(&span).expect("committed context selection publishes its original call");
                let expected = reference.api_calls.get(&span).unwrap();
                assert!(std::ptr::eq(actual.sig, expected.sig));
                assert_eq!(actual.params, expected.params);
                assert_eq!(actual.argument_slots, expected.argument_slots);
                assert!(delayed.local_inference.host_obligations.entries.is_empty());
            } else {
                assert!(!reference.diagnostics.is_empty(), "{source}");
                assert!(!delayed.api_calls.contains_key(&span), "{source}");
                assert_eq!(delayed.local_inference.host_obligations.entries.len(), 1, "{source}");
                assert!(delayed.local_inference.host_obligations.entries[0].candidates.selected.is_none(), "{source}");
            }
        }
    }

    #[test]
    fn late_path_like_nested_effects_precede_argument_effects() {
        let source = "let raw = env.PATH\nraw.append(p\"inner\".resolve() ?? p\"fallback\")\n";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let _symbols = parsed.arena.symbol_owner().enter();
        let statement = parsed.arena.arena.stmt_ids(parsed.arena.statements).last().unwrap();
        let crate::syntax::arena::ArenaStmtKind::Expr(outer) = parsed.arena.arena.stmt(statement).kind else { panic!("outer host call") };
        let crate::syntax::arena::ArenaExprKind::Call { args, .. } = parsed.arena.arena.expr(outer).kind else { panic!("outer host call") };
        let crate::syntax::arena::ArenaCallArgKind::Positional(value) = parsed.arena.arena.call_args(args)[0].kind else { panic!("path argument") };
        let crate::syntax::arena::ArenaExprKind::Binary { left: inner, .. } = parsed.arena.arena.expr(value).kind else { panic!("resolved path fallback") };
        let mut reference = Checker::new(crate::sema::check::CheckOptions::default());
        reference.resolved_bindings = std::sync::Arc::new(super::super::super::ResolvedBindings::collect(&parsed.arena));
        let declaration = parsed.arena.arena.stmt_ids(parsed.arena.statements).next().unwrap();
        reference.check_stmt_arena(&parsed.arena, source, declaration);
        assert!(reference.diagnostics.is_empty(), "{:?}", reference.diagnostics);
        reference.boundary.in_pure = true;
        let mut delayed = reference.clone();
        reference.check_expr_arena(&parsed.arena, source, outer, None);
        let outer_continuation = delayed.reserve_host_call_continuation(HostCallSource::Expression(outer), "append", None);
        let inner_continuation = delayed.reserve_host_call_continuation(HostCallSource::Expression(inner), "resolve", None).finish(&mut delayed, Vec::new());
        let outer_continuation = outer_continuation.finish(&mut delayed, vec![super::super::super::args::CheckedArgumentContext::default(); 1]);
        for (call, name, receiver, continuation, arguments) in [
            (inner, "resolve", Type::Path, inner_continuation, Vec::new()),
            (outer, "append", Type::EnvPathList, outer_continuation, crate::sema::arguments::expand_named_arguments(&parsed.arena, parsed.arena.arena.call_args(args), |_| Some(Type::Path)).unwrap()),
        ] {
            let span = parsed.arena.arena.expr(call).span;
            let result = delayed.type_constraints.fresh(span);
            delayed.local_inference.host_obligations.entries.push(HostObligation {
                continuation,
                candidates: HostMethodCandidates { selected: None, receiver, result, expected: None, arguments, candidates: method_candidates(name), span },
            });
        }
        assert_eq!(delayed.solve_host_obligations(&parsed.arena), Ok(()));
        let order = |checker: &Checker| checker.diagnostics.iter().filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckPureEffect)).flat_map(|diagnostic| diagnostic.labels.iter().map(|label| label.span)).collect::<Vec<_>>();
        assert_eq!(order(&reference), [parsed.arena.arena.expr(outer).span, parsed.arena.arena.expr(inner).span]);
        assert_eq!(order(&delayed), order(&reference));
        assert!(delayed.local_inference.host_obligations.entries.is_empty());
        for call in [inner, outer] {
            let span = parsed.arena.arena.expr(call).span;
            assert!(std::ptr::eq(delayed.api_calls[&span].sig, reference.api_calls[&span].sig));
        }
    }

    #[test]
    fn late_path_like_methods_preserve_checked_arguments() {
        for (name, receiver_kind, receiver_type) in [
            ("strip_prefix", MethodReceiver::Path, Type::Path),
            ("prepend", MethodReceiver::EnvPathList, Type::EnvPathList),
            ("append", MethodReceiver::EnvPathList, Type::EnvPathList),
        ] {
            for (input, binding_type, accepted) in [
                ("p\"child\"", None, true),
                ("\"child\"", None, true),
                ("value", Some(Type::Str), false),
                ("value", Some(Type::Any), false),
                ("false", None, false),
                ("", None, false),
                ("p\"one\", p\"two\"", None, false),
            ] {
                for actual_receiver in [&receiver_type, &Type::Str] {
                let accepted = accepted && actual_receiver == &receiver_type;
                let declaration = binding_type.as_ref().map(|ty| format!("let value: {ty} = \"stored\"\n")).unwrap_or_default();
                let source = format!("{declaration}raw.{name}({input})\n");
                let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let _symbols = parsed.arena.symbol_owner().enter();
                let statement = parsed.arena.arena.stmt_ids(parsed.arena.statements).last().unwrap();
                let crate::syntax::arena::ArenaStmtKind::Expr(call) = parsed.arena.arena.stmt(statement).kind else { panic!("host call") };
                let crate::syntax::arena::ArenaExprKind::Call { args, .. } = parsed.arena.arena.expr(call).kind else { panic!("host call") };
                let args = parsed.arena.arena.call_args(args);
                let span = parsed.arena.arena.expr(call).span;
                let mut reference = Checker::new(crate::sema::check::CheckOptions::default());
                reference.resolved_bindings = std::sync::Arc::new(super::super::super::ResolvedBindings::collect(&parsed.arena));
                if binding_type.is_some() {
                    let declaration = parsed.arena.arena.stmt_ids(parsed.arena.statements).next().unwrap();
                    reference.check_stmt_arena(&parsed.arena, &source, declaration);
                    assert!(reference.diagnostics.is_empty(), "{source}: {:?}", reference.diagnostics);
                }
                let mut delayed = reference.clone();
                reference.check_registered_method_arena(&parsed.arena, &source, HostCallSource::Expression(call), receiver_kind, name, args, span, actual_receiver, DiagnosticCode::CheckUnknownMethod, None, None);
                let continuation = delayed.reserve_host_call_continuation(HostCallSource::Expression(call), name, None);
                let arguments = crate::sema::arguments::expand_named_arguments(&parsed.arena, args, |expression| {
                    Some(delayed.check_expr_arena(&parsed.arena, &source, expression, Some(&Type::Path)))
                }).unwrap();
                let continuation = continuation.finish(&mut delayed, vec![super::super::super::args::CheckedArgumentContext::default(); arguments.len()]);
                let receiver = delayed.type_constraints.fresh(span);
                let result = delayed.type_constraints.fresh(span);
                delayed.local_inference.host_obligations.entries.push(HostObligation {
                    continuation,
                    candidates: HostMethodCandidates { selected: None, receiver: receiver.clone(), result, expected: None, arguments, candidates: method_candidates(name), span },
                });
                delayed.type_constraints.constrain(&receiver, actual_receiver, span).unwrap();
                assert_eq!(delayed.solve_host_obligations(&parsed.arena), Ok(()));
                if accepted {
                    assert!(reference.diagnostics.is_empty(), "{source}: {:?}", reference.diagnostics);
                    assert!(delayed.diagnostics.is_empty(), "{source}: {:?}", delayed.diagnostics);
                    let actual = delayed.api_calls.get(&span).expect("Path-like continuation publishes its canonical operation");
                    let expected = reference.api_calls.get(&span).unwrap();
                    assert!(std::ptr::eq(actual.sig, expected.sig));
                    assert_eq!(actual.params, expected.params);
                    assert_eq!(actual.argument_slots, expected.argument_slots);
                    assert!(delayed.local_inference.host_obligations.entries.is_empty());
                } else {
                    assert!(!reference.diagnostics.is_empty(), "{source}");
                    assert!(!delayed.api_calls.contains_key(&span), "{source}");
                    assert_eq!(delayed.local_inference.host_obligations.entries.len(), 1, "{source}");
                    assert!(delayed.local_inference.host_obligations.entries[0].candidates.selected.is_none(), "{source}");
                }
                }
            }
        }
    }

    #[test]
    fn late_host_argument_overloads_publish_the_checked_selection() {
        for (source, accepted) in [
            ("p\"output\".write(\"payload\")\n", true),
            ("p\"output\".write(b\"payload\")\n", true),
            ("p\"output\".write(false)\n", false),
        ] {
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let _symbols = parsed.arena.symbol_owner().enter();
            let statement = parsed.arena.arena.stmt_ids(parsed.arena.statements).next().unwrap();
            let crate::syntax::arena::ArenaStmtKind::Expr(call) = parsed.arena.arena.stmt(statement).kind else { panic!("host call") };
            let crate::syntax::arena::ArenaExprKind::Call { args, .. } = parsed.arena.arena.expr(call).kind else { panic!("host call") };
            let span = parsed.arena.arena.expr(call).span;
            let mut reference = Checker::new(crate::sema::check::CheckOptions::default());
            reference.check_expr_arena(&parsed.arena, source, call, None);
            let mut delayed = Checker::new(crate::sema::check::CheckOptions::default());
            let arguments = crate::sema::arguments::expand_named_arguments(&parsed.arena, parsed.arena.arena.call_args(args), |expression| {
                Some(delayed.check_expr_arena(&parsed.arena, source, expression, None))
            }).unwrap();
            let receiver = delayed.type_constraints.fresh(span);
            let result = delayed.type_constraints.fresh(span);
            let continuation = delayed.reserve_host_call_continuation(HostCallSource::Expression(call), "write", None).finish(&mut delayed, vec![super::super::super::args::CheckedArgumentContext::default(); arguments.len()]);
            delayed.local_inference.host_obligations.entries.push(HostObligation {
                continuation,
                candidates: HostMethodCandidates { selected: None, receiver: receiver.clone(), result, expected: None, arguments, candidates: method_candidates("write"), span },
            });
            delayed.type_constraints.constrain(&receiver, &Type::Path, span).unwrap();
            assert_eq!(delayed.solve_host_obligations(&parsed.arena), Ok(()));
            if accepted {
                assert!(reference.diagnostics.is_empty(), "{:?}", reference.diagnostics);
                assert!(delayed.diagnostics.is_empty(), "{:?}", delayed.diagnostics);
                let actual = delayed.api_calls.get(&span).expect("argument-selected host call publishes its committed overload");
                let expected = reference.api_calls.get(&span).unwrap();
                assert!(std::ptr::eq(actual.sig, expected.sig));
                assert_eq!(actual.params, expected.params);
                assert_eq!(actual.argument_slots, expected.argument_slots);
                assert!(delayed.local_inference.host_obligations.entries.is_empty());
            } else {
                assert!(!reference.diagnostics.is_empty());
                assert!(!delayed.api_calls.contains_key(&span));
                assert_eq!(delayed.local_inference.host_obligations.entries.len(), 1);
                assert!(delayed.local_inference.host_obligations.entries[0].candidates.candidates.is_empty());
                assert!(delayed.local_inference.host_obligations.entries[0].candidates.selected.is_none());
            }
        }
    }

    #[test]
    fn late_host_overloaded_nested_calls_finalize_after_arguments() {
        let source = "p\"outer\".write(p\"inner\".read_text() ?? \"\")\n";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let _symbols = parsed.arena.symbol_owner().enter();
        let statement = parsed.arena.arena.stmt_ids(parsed.arena.statements).next().unwrap();
        let crate::syntax::arena::ArenaStmtKind::Expr(outer) = parsed.arena.arena.stmt(statement).kind else { panic!("outer host call") };
        let crate::syntax::arena::ArenaExprKind::Call { args, .. } = parsed.arena.arena.expr(outer).kind else { panic!("outer host call") };
        let crate::syntax::arena::ArenaCallArgKind::Positional(value) = parsed.arena.arena.call_args(args)[0].kind else { panic!("data argument") };
        let crate::syntax::arena::ArenaExprKind::Binary { left: inner, .. } = parsed.arena.arena.expr(value).kind else { panic!("existing string fallback") };
        let mut reference = Checker::new(crate::sema::check::CheckOptions::default());
        reference.boundary.in_pure = true;
        let mut delayed = reference.clone();
        reference.check_expr_arena(&parsed.arena, source, outer, None);
        let outer_continuation = delayed.reserve_host_call_continuation(HostCallSource::Expression(outer), "write", None);
        let inner_continuation = delayed.reserve_host_call_continuation(HostCallSource::Expression(inner), "read_text", None).finish(&mut delayed, Vec::new());
        let outer_continuation = outer_continuation.finish(&mut delayed, vec![super::super::super::args::CheckedArgumentContext::default(); 1]);
        for (call, name, continuation, arguments) in [
            (inner, "read_text", inner_continuation, Vec::new()),
            (outer, "write", outer_continuation, crate::sema::arguments::expand_named_arguments(&parsed.arena, parsed.arena.arena.call_args(args), |_| Some(Type::Str)).unwrap()),
        ] {
            let span = parsed.arena.arena.expr(call).span;
            let result = delayed.type_constraints.fresh(span);
            delayed.local_inference.host_obligations.entries.push(HostObligation {
                continuation,
                candidates: HostMethodCandidates { selected: None, receiver: Type::Path, result, expected: None, arguments, candidates: method_candidates(name), span },
            });
        }
        assert_eq!(delayed.solve_host_obligations(&parsed.arena), Ok(()));
        assert!(delayed.local_inference.host_obligations.entries.is_empty());
        let outer_api = delayed.api_calls.get(&parsed.arena.arena.expr(outer).span).unwrap();
        let reference_api = reference.api_calls.get(&parsed.arena.arena.expr(outer).span).unwrap();
        assert!(std::ptr::eq(outer_api.sig, reference_api.sig));
        assert_eq!(outer_api.params, reference_api.params);
        assert_eq!(outer_api.argument_slots, reference_api.argument_slots);
        assert!(delayed.api_calls.contains_key(&parsed.arena.arena.expr(inner).span));
        let order = |checker: &Checker| checker.diagnostics.iter().filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckPureEffect)).flat_map(|diagnostic| diagnostic.labels.iter().map(|label| label.span)).collect::<Vec<_>>();
        let reference_order = order(&reference);
        assert_eq!(reference_order.first(), Some(&parsed.arena.arena.expr(inner).span));
        assert!(reference_order.contains(&parsed.arena.arena.expr(outer).span));
        assert_eq!(order(&delayed), [parsed.arena.arena.expr(inner).span, parsed.arena.arena.expr(outer).span]);
    }

    #[test]
    fn late_host_receiver_constraints_do_not_train_from_callers() {
        let mut previous_body_entries = None;
        for calls in ["let first: Int? = first_byte(\"A\")\nlet second: Int? = first_byte(\"B\")", "let second: Int? = first_byte(\"B\")\nlet first: Int? = first_byte(\"A\")"] {
            let source = format!("pure first_byte(raw) {{ let found = raw.byte_at(0); let anchored: Str = raw; found }}\n{calls}\n");
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let _symbols = parsed.arena.symbol_owner().enter();
            let (work, checked) = crate::sema::inference_work::collect(|| Checker::check_arena_with_options(&parsed.arena, &source, crate::sema::check::CheckOptions::for_execution()));
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            assert_eq!(checked.api_calls.len(), 1);
            let call = checked.api_calls.values().next().unwrap();
            assert_eq!(call.receiver, Some(MethodReceiver::Str));
            assert_eq!(call.params, [Type::Int]);
            assert_eq!(call.argument_slots, [0]);
            assert!(work.function_body_check_entries > 0);
            if let Some(previous) = previous_body_entries { assert_eq!(work.function_body_check_entries, previous); }
            previous_body_entries = Some(work.function_body_check_entries);
        }
    }

    #[test]
    fn host_obligation_selection_publishes_api_evidence_and_real_ambiguity() {
        let selected_source = "pure first_byte(raw: Str) -> Int? { raw.byte_at(0) }\n";
        let selected = crate::syntax::parser::Parser::parse_source_arena_only(
            crate::source::SourceId::new(0),
            selected_source,
        );
        assert!(selected.diagnostics.is_empty(), "{:?}", selected.diagnostics);
        let checked = Checker::check_arena_with_options(
            &selected.arena,
            selected_source,
            crate::sema::check::CheckOptions::for_execution(),
        );
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.api_calls.len(), 1);
        let call = checked.api_calls.values().next().expect("selected API call");
        assert_eq!(call.receiver, Some(MethodReceiver::Str));
        assert_eq!(call.params, vec![Type::Int]);
        assert_eq!(call.argument_slots, vec![0]);

        let ambiguous_source =
            "pure ambiguous(raw) -> Int? { raw.byte_at(0) }\nlet output = ambiguous(b\"A\")\n";
        let ambiguous = crate::syntax::parser::Parser::parse_source_arena_only(
            crate::source::SourceId::new(0),
            ambiguous_source,
        );
        assert!(ambiguous.diagnostics.is_empty(), "{:?}", ambiguous.diagnostics);
        let checked = Checker::check_arena_with_options(
            &ambiguous.arena,
            ambiguous_source,
            crate::sema::check::CheckOptions::for_execution(),
        );
        assert!(checked.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(DiagnosticCode::CheckAmbiguousOverload)
        }));
        assert!(checked.api_calls.is_empty());
    }

    #[test]
    fn host_obligation_dependencies_preserve_depth_and_foreign_variable_errors() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), "raw.trim()\n");
            assert!(parsed.diagnostics.is_empty());
            let span = Span::at(crate::source::SourceId::new(0), 0);
            let mut checker = Checker::new(crate::sema::check::CheckOptions::default());
            let mut deep = checker.type_constraints.fresh(span);
            for _ in 0..300 {
                deep = Type::List(Box::new(deep));
            }
            let mut foreign = TypeConstraints::default();
            for (receiver, expected) in [
                (deep, ConstraintResolutionError::TypeDepth),
                (foreign.fresh(span), ConstraintResolutionError::ForeignVariable),
            ] {
                let candidates = HostMethodCandidates { selected: None,
                    receiver,
                    result: Type::Unit,
                    expected: None,
                    arguments: Vec::new(),
                    candidates: Vec::new(),
                    span,
                };
                let obligation = parsed_obligation(&mut checker, &parsed.arena, 0, "trim", candidates);
                checker.local_inference.host_obligations.entries.push(obligation);
                assert_eq!(checker.solve_host_obligations(&parsed.arena), Err(expected));
            }
        });
    }

    fn assert_late_byte_at_receiver(parameter: &str, domain: &str, argument: &str, expected_receiver: MethodReceiver) {
        let length_method = if expected_receiver == MethodReceiver::Str { "byte_len" } else { "len" };
        let source = format!(r#"pure receiver_length(raw: {domain}) -> Int {{ raw.{length_method}() }}
pure probe({parameter}) {{
    let equal = raw.byte_at(0) == 65
    let missing = raw.byte_at(9) == null
    let size = receiver_length(raw)
    {{equal, missing, size}}
}}
let observed = probe({argument})
"#);
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, &source, crate::sema::check::CheckOptions::for_execution());
        assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
        let calls = checked.api_calls.iter().filter(|(span, _)| source[span.range()].contains(".byte_at(")).collect::<Vec<_>>();
        assert_eq!(calls.len(), 2);
        assert!(calls.iter().all(|(_, call)| call.receiver == Some(expected_receiver)));
    }

    #[test]
    fn typed_byte_at_comparisons_keep_optional_results() {
        assert_late_byte_at_receiver("raw: Bytes", "Bytes", "b\"A\"", MethodReceiver::Bytes);
    }

    #[test]
    fn late_bytes_receiver_keeps_the_common_optional_method_result() {
        assert_late_byte_at_receiver("raw", "Bytes", "b\"A\"", MethodReceiver::Bytes);
    }

    #[test]
    fn late_text_receiver_keeps_the_common_optional_method_result() {
        assert_late_byte_at_receiver("raw", "Str", "\"A\"", MethodReceiver::Str);
    }

    #[test]
    fn common_host_results_preserve_receiver_ambiguity_and_distinct_return_domains() {
        for source in [
            "pure ambiguous(raw) { raw.byte_at(0) == 65 }\n",
            "pure ambiguous(raw) { raw.trim() }\n",
        ] {
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions::for_execution());
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckAmbiguousOverload)), "{source}\n{:?}", checked.diagnostics);
            assert!(checked.api_calls.is_empty());
        }
        let source = "pure anchor(raw: Bytes) -> Bytes { raw }\npure narrowed(raw) { let trimmed = raw.trim(); let known = anchor(raw); trimmed }\nlet actual: Bytes = narrowed(b\" A \")\n";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions::for_execution());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.api_calls.values().next().unwrap().receiver, Some(MethodReceiver::Bytes));
    }
    #[test]
    fn typed_path_transform_methods_keep_their_canonical_result_domains() {
        let source = "pure parent(location: Path) -> Path { location.parent() }\npure relative_parent(location: RelPath) -> RelPath { location.parent() }\npure normal(location: Path) -> Path { location.normalize() }\npure relative_normal(location: RelPath) -> RelPath { location.normalize() }\n";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions::for_execution());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    }

    #[test]
    fn inferred_path_transform_methods_retain_independent_result_domains() {
        for method in ["parent", "normalize"] {
            for callers in ["", "pure absolute(location: Path) -> Path { transform(location) }\npure relative(location: RelPath) -> RelPath { transform(location) }", "pure relative(location: RelPath) -> RelPath { transform(location) }\npure absolute(location: Path) -> Path { transform(location) }"] {
                let source = format!("pure transform(location) {{ location.{method}() }}\n{callers}\n");
                let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
                assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
                let checked = Checker::check_arena_with_options(&parsed.arena, &source, crate::sema::check::CheckOptions::for_execution());
                assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
            }
        }
    }

    #[test]
    fn path_transform_results_cannot_select_the_actual_receiver() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let span = Span::at(crate::source::SourceId::new(0), 0);
            let mut constraints = TypeConstraints::default();
            let receiver = constraints.fresh_at_level(span, 1);
            let result = constraints.fresh_at_level(span, 1);
            let mut obligation = HostMethodCandidates {
                selected: None, receiver: receiver.clone(), result: result.clone(),
                expected: Some(Type::rel_path()), arguments: Vec::new(), candidates: method_candidates("parent"), span,
            };
            assert_eq!(obligation.select(&mut constraints, "parent"), Ok(false));
            assert_eq!(constraints.resolve(&receiver).unwrap(), receiver);
            assert_eq!(constraints.resolve(&result).unwrap(), result);
            assert_eq!(obligation.candidates.len(), 2);
            constraints.constrain(&receiver, &Type::Path, span).unwrap();
            assert_eq!(obligation.select(&mut constraints, "parent"), Ok(false));
            assert!(obligation.candidates.is_empty());
        });
    }

    #[test]
    fn inferred_path_transforms_reject_incompatible_inputs_and_results() {
        for source in [
            "pure parent(location) { location.parent() }\nlet _ = parent(7)",
            "pure parent(location) { location.parent() }\nlet _ = parent(\"a/b\")",
            "pure parent(location) { location.parent() }\nproc wrong(location: Any) [] { parent(location) }",
            "pure parent(location) { location.parent() }\npure wrong(location: Path) -> RelPath { parent(location) }",
            "pure normal(location) { location.normalize() }\npure wrong(location: Path) -> RelPath { normal(location) }",
            "pure parent(location) { location.parent() }\npure incompatible(location) { let first = parent(location); need_number(first) }\npure need_number(number: Int) -> Int { number }\nlet _ = incompatible(p\"/a/b\")",
        ] {
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, source, crate::sema::check::CheckOptions::for_execution());
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error), "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn authored_path_results_preserve_both_canonical_receiver_domains() {
        for method in ["parent", "normalize"] {
            let source = format!("pure projected(location) -> Path {{ location.{method}() }}\npure absolute(location: Path) -> Path {{ projected(location) }}\npure relative(location: RelPath) -> Path {{ projected(location) }}\n");
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, &source, crate::sema::check::CheckOptions::for_execution());
            assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
        }
    }

}
