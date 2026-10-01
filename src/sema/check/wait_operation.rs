use super::{CallBinding, Checker, SolvedOperation, Type};
use crate::sema::inference::{EffectSet, EffectSummary, InferenceError, OperationCall};
use crate::syntax::arena::{ArenaProgram, ExprId};

impl Checker {
    pub(super) fn check_graph_wait_operation(&mut self, arena: &ArenaProgram, target: ExprId, operand: &Type) -> Type {
        let span = arena.arena.expr(target).span;
        let Some(expression) = self.current_expression else {
            self.graph_error(span, InferenceError::Boundary("wait requires its source expression identity"));
            return Type::Invalid;
        };
        let identity = self.expression_identity(arena, expression);
        if !self.graph_generation || self.generic.borrow().facts.operations.contains_key(&identity) {
            return self.generic.borrow().facts.operations.get(&identity).map(|operation| self.graph_view(operation.result)).unwrap_or(Type::Invalid);
        }
        let outcome = (|| {
            let input = self.graph_type(operand, span)?;
            let level = self.local_initializer_level.unwrap_or(if self.current_generic.is_some() { 1 } else { 0 });
            let mut state = self.generic.borrow_mut();
            let family = {
                let super::generic::GenericState { facts, language_operations, .. } = &mut *state;
                language_operations.wait_family(&mut facts.graph, span)?
            };
            let graph = &mut state.facts.graph;
            let result = graph.fresh(level, span)?;
            let effects = EffectSummary::Closed(EffectSet::PROCESS);
            let reason = graph.reason(span, None)?;
            let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: vec![Some(input)], result, effects, effect_bindings: Vec::new(), output_effect_bindings: Vec::new() }, reason)?;
            graph.solve()?;
            if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).ok_or(InferenceError::InvalidScheme)?.requirements.push(requirement); }
            state.facts.operations.insert(identity, SolvedOperation { requirement, result, effects, receiver: None, actual_arguments: vec![input], argument_coercions: Vec::new(), binding: CallBinding { supplied_slots: vec![0], default_slots: Vec::new(), rest_slot: None, dynamic: None }, caller: self.current_generic });
            state.facts.expressions.insert(identity, result);
            if let Some(owner) = self.current_generic { state.facts.expression_owners.insert(identity, owner); }
            drop(state);
            Ok(self.graph_view(result))
        })();
        match outcome {
            Ok(result) => result,
            Err(InferenceError::UnsupportedOperation(_)) => {
                self.graph_boundary_error(span, "`wait` expects ProcessHandle or List[ProcessHandle]", "check.wait-target");
                Type::Invalid
            }
            Err(error) => { self.graph_error(span, error); Type::Invalid }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Checker;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    #[test]
    fn source_wait_retains_forwarded_handle_list_and_explicit_erased_contracts() {
        let source = "proc awaited(value) [process] { wait value }\nproc forwarded(value) [process] { awaited(value) }\nproc one(handle: ProcessHandle) [process] -> Result[Status, ProcessError] { forwarded(handle) }\nproc many(handles: List[ProcessHandle]) [process] -> Result[List[Status], ProcessError] { forwarded(handles) }\nproc erased(handle: Any) [process] -> Result[Status, ProcessError] { forwarded(handle) }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(18), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.operations.len(), 1);
        let (identity, operation) = checked.solved.operations.iter().next().unwrap();
        assert_eq!(identity.source, SourceId::new(18));
        assert!(checked.solved.declarations[&operation.caller.unwrap()].source_requirements.contains(&operation.requirement));
        assert_eq!(operation.effects, crate::sema::inference::EffectSummary::Closed(crate::sema::inference::EffectSet::PROCESS));
        let mut concrete_kinds = std::collections::BTreeSet::new();
        for call in checked.solved.calls.values() {
            for &requirement in &call.requirements {
                let Some(evidence) = checked.solved.graph.candidate_evidence(requirement).unwrap() else { continue; };
                let authority = checked.solved.operation_catalog.candidate(&checked.solved.graph, evidence.candidate).unwrap();
                if let super::super::SolvedOperationAuthority::Language(metadata) = authority
                    && let crate::sema::operation_graph::PreparedLanguageOperation::Wait { list, erased } = metadata.operation {
                    concrete_kinds.insert((list, erased));
                }
            }
        }
        assert_eq!(concrete_kinds, [(false, false), (false, true), (true, false)].into());
        drop(parsed);
        checked.solved.validate().unwrap();
    }

    #[test]
    fn source_wait_ground_candidates_keep_exact_results_and_process_permissions() {
        use crate::sema::inference::{EffectSet, EffectSummary};
        use crate::sema::types::Type;
        for (parameter, result, list, erased) in [
            ("ProcessHandle", Type::Status, false, false),
            ("List[ProcessHandle]", Type::List(Box::new(Type::Status)), true, false),
            ("Any", Type::Status, false, true),
        ] {
            let source = format!("proc awaited(value: {parameter}) [process] {{ wait value }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(22), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let operation = checked.solved.operations.values().next().unwrap();
            assert_eq!(checked.solved.graph.export_type(operation.result).unwrap(), Type::Result(Box::new(result), Box::new(Type::ProcessError)));
            assert_eq!(operation.effects, EffectSummary::Closed(EffectSet::PROCESS));
            assert_eq!(operation.binding.supplied_slots, [0]);
            let evidence = checked.solved.graph.candidate_evidence(operation.requirement).unwrap().unwrap();
            let authority = checked.solved.operation_catalog.candidate(&checked.solved.graph, evidence.candidate).unwrap();
            assert!(matches!(authority, super::super::SolvedOperationAuthority::Language(metadata)
                if metadata.operation == crate::sema::operation_graph::PreparedLanguageOperation::Wait { list, erased }));
            let declaration = checked.solved.declarations.values().next().unwrap();
            assert_eq!(declaration.effective_effects, EffectSummary::Closed(EffectSet::PROCESS));
            drop(parsed);
            checked.solved.validate().unwrap();
        }
    }

    #[test]
    fn source_wait_rejects_wrong_domains_and_restricted_effects_before_execution() {
        for source in [
            "proc awaited(value) [process] { wait value }\nproc invalid() [process] { awaited(7) }\n",
            "proc awaited(value) [process] { wait value }\nproc invalid() [process] { awaited([7]) }\n",
            "proc awaited(value) [process] { wait value }\nproc invalid() [process] { awaited({value: 7}) }\n",
            "proc awaited(value) [process] { wait value }\nproc invalid(handle: ProcessHandle) [process] { awaited(Ok(handle)) }\n",
            "proc awaited(value) [process] { wait value }\nproc invalid(handle: ProcessHandle) [] { awaited(handle) }\n",
            "proc invalid(handle: ProcessHandle) [] { wait handle }\n",
            "pure invalid(handle: ProcessHandle) { wait handle }\n",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(23), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(!checked.diagnostics.is_empty(), "invalid wait accepted: {source}");
            drop(parsed);
            checked.solved.validate().unwrap();
        }
    }
}
