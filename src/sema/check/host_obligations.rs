use super::{ArenaCallArg, ArenaProgram, Checker, DiagnosticCode, MethodReceiver, MethodSig, Span, Type, api_spec};
use crate::sema::arguments::ExpandedArgument;
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
pub(in super::super) struct HostObligations(Vec<HostObligation>);

#[derive(Clone)]
struct HostObligation {
    receiver: Type,
    result: Type,
    expected: Option<Type>,
    arguments: Vec<ExpandedArgument>,
    candidates: Vec<HostMethod>,
    span: Span,
}

impl HostObligation {
    fn dependencies(&self, constraints: &TypeConstraints) -> Result<BTreeSet<Span>, ConstraintResolutionError> {
        std::iter::once(&self.receiver)
            .chain(std::iter::once(&self.result))
            .chain(self.expected.as_ref())
            .chain(self.arguments.iter().map(|argument| &argument.ty))
            .map(|ty| constraints.unresolved(ty))
            .collect::<Result<Vec<_>, _>>()
            .map(|dependencies| dependencies.into_iter().flatten().collect())
    }

    fn constrain_candidate(
        &self,
        candidate: HostMethod,
        constraints: &mut TypeConstraints,
        include_result: bool,
    ) -> Result<(), ConstraintConflict> {
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
        for (argument, slot) in self.arguments.iter().zip(binding.argument_slots) {
            constraints.constrain_context(&params[slot].ty, &argument.ty, argument.span)?;
        }
        if include_result {
            if let Some(expected) = &self.expected {
                instance.constrain_result(expected, constraints, self.span)?;
            }
            constraints.constrain(&self.result, &instance.signature.return_ty, self.span)?;
        }
        Ok(())
    }

    fn select(&mut self, constraints: &mut TypeConstraints) -> Result<bool, ConstraintResolutionError> {
        let mut matching = Vec::new();
        for candidate in std::mem::take(&mut self.candidates) {
            let params = crate::sema::builtin_templates::callable_parameters(&candidate.signature.sig);
            if crate::sema::arguments::bind_static_arguments(&params, &self.arguments).is_ok()
                && constraints.trial(|constraints| self.constrain_candidate(candidate, constraints, false))?
            {
                matching.push(candidate);
            }
        }
        let competing_domains = matching.iter().filter_map(|candidate| {
            (matching.iter().filter(|other| other.receiver == candidate.receiver).count() > 1)
                .then_some(candidate.receiver)
        }).collect::<Vec<_>>();
        for candidate in matching {
            if competing_domains.contains(&candidate.receiver)
                || constraints.trial(|constraints| self.constrain_candidate(candidate, constraints, true))?
            {
                self.candidates.push(candidate);
            }
        }
        if let [candidate] = self.candidates.as_slice()
            && candidate.signature.receiver_ty.is_some()
        {
            match self.constrain_candidate(*candidate, constraints, true) {
                Ok(()) => return Ok(true),
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
    pub(in super::super) fn check_inferred_host_method(
        &mut self,
        arena: &ArenaProgram,
        source: &str,
        base: crate::syntax::arena::ExprId,
        receiver: Type,
        name: &str,
        args: &[ArenaCallArg],
        span: Span,
        expected: Option<&Type>,
    ) -> Type {
        let candidates = method_candidates(name);
        let registered = !candidates.is_empty();
        let signatures = candidates.iter().map(|method| method.signature.clone()).collect::<Vec<_>>();
        for (index, argument) in args.iter().enumerate() {
            let expected = super::common_method_overload_expected_arena(args, &signatures, index);
            self.check_call_arg_arena(arena, source, &argument.kind, expected.as_ref());
        }
        let Ok(arguments) = crate::sema::arguments::expand_named_arguments(arena, args, |expression| {
            self.expr_types.get(&arena.arena.expr(expression).span).cloned()
        }) else {
            self.error(span, "method receiver needs an annotation", DiagnosticCode::CheckLocalInference);
            return Type::Invalid;
        };
        let result = self.type_constraints.fresh(span);
        let mut obligation = HostObligation { receiver: receiver.clone(), result: result.clone(), expected: expected.cloned(), arguments, candidates, span };
        match obligation.select(&mut self.type_constraints) {
            Ok(true) => {
                let receiver = match self.type_constraints.resolve(&receiver) {
                    Ok(receiver) => receiver,
                    Err(error) => return self.reject_host_inference_resolution(error, span),
                };
                return self.check_method_dispatch_arena(arena, source, base, receiver, name, args, span, expected);
            }
            Ok(false) => {}
            Err(error) => return self.reject_host_inference_resolution(error, span),
        }
        if self.collecting_local_constraints() {
            self.local_inference.host_obligations.0.push(obligation);
            result
        } else {
            let (message, code) = if !registered {
                ("no registered method has this name", DiagnosticCode::CheckUnknownMethod)
            } else if obligation.candidates.is_empty() {
                ("no registered method matches this body contract", DiagnosticCode::CheckTypeMismatch)
            } else if obligation.candidates.len() == 1 && obligation.candidates[0].signature.receiver_ty.is_none() {
                ("registered method receiver needs an annotation", DiagnosticCode::CheckLocalInference)
            } else {
                ("method receiver has no unique registered operation", DiagnosticCode::CheckAmbiguousOverload)
            };
            self.error(span, message, code);
            Type::Invalid
        }
    }

    pub(in super::super) fn solve_host_obligations(&mut self) -> Result<(), ConstraintResolutionError> {
        let mut pending = std::mem::take(&mut self.local_inference.host_obligations.0)
            .into_iter().map(Some).collect::<Vec<_>>();
        let mut watchers = BTreeMap::<Span, BTreeSet<usize>>::new();
        let mut queue = (0..pending.len()).collect::<VecDeque<_>>();
        let mut queued = (0..pending.len()).collect::<BTreeSet<_>>();
        for (index, obligation) in pending.iter().enumerate() {
            for dependency in obligation.as_ref().expect("pending obligation").dependencies(&self.type_constraints)? {
                watchers.entry(dependency).or_default().insert(index);
            }
        }
        while let Some(index) = queue.pop_front() {
            queued.remove(&index);
            let Some(obligation) = &mut pending[index] else { continue };
            let dependencies = obligation.dependencies(&self.type_constraints)?;
            if obligation.select(&mut self.type_constraints)? {
                pending[index] = None;
                for dependency in dependencies {
                    if let Some(dependents) = watchers.remove(&dependency) {
                        for dependent in dependents {
                            if pending[dependent].is_some() && queued.insert(dependent) {
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
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            checker.check_method_dispatch_arena(&parsed.arena, "\"x\"\n", base, deep, "trim", &[], span, None);
            assert!(checker.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckLocalInference)));
            let mut foreign = TypeConstraints::default();
            let receiver = foreign.fresh(span);
            assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                checker.check_method_dispatch_arena(&parsed.arena, "\"x\"\n", base, receiver, "trim", &[], span, None);
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
                let mut obligation = HostObligation {
                    receiver: actual_receiver,
                    result: Type::Str,
                    expected: None,
                    arguments,
                    candidates: vec![candidate],
                    span,
                };
                assert_eq!(obligation.select(&mut constraints), Err(expected));
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
            let mut obligation = HostObligation {
                receiver: receiver.clone(),
                result: Type::Str,
                expected: Some(Type::Str),
                arguments: Vec::new(),
                candidates: vec![HostMethod { receiver: MethodReceiver::Str, signature: string }, HostMethod { receiver: MethodReceiver::Str, signature: bytes }],
                span,
            };
            assert_eq!(obligation.select(&mut constraints), Ok(false));
            assert_eq!(obligation.candidates.len(), 2);
            assert_eq!(constraints.resolve(&receiver), Ok(receiver));
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
    fn host_obligation_dependencies_preserve_depth_and_foreign_variable_errors() {
        crate::symbol::SymbolOwner::new().with_current(|| {
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
                checker.local_inference.host_obligations.0.push(HostObligation {
                    receiver,
                    result: Type::Unit,
                    expected: None,
                    arguments: Vec::new(),
                    candidates: Vec::new(),
                    span,
                });
                assert_eq!(checker.solve_host_obligations(), Err(expected));
            }
        });
    }
}
