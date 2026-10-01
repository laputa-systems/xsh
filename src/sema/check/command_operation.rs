use super::{CallBinding, Checker, SolvedOperation, StatementIdentity, StatementPosition, Type};
use crate::sema::inference::{EffectSet, EffectSummary, InferenceError, OperationCall};
use crate::syntax::arena::{ArenaProgram, StmtId};

impl Checker {
    pub(super) fn check_graph_display_command(&mut self, arena: &ArenaProgram, statement: StmtId, stderr: bool, operands: &[Type]) {
        let span = arena.arena.stmt(statement).span;
        let identity = StatementIdentity { source: span.source_id, namespace: self.current_namespace, statement };
        if !self.graph_generation || self.generic.borrow().facts.statement_operations.contains_key(&identity) { return; }
        let outcome = (|| {
            let arguments = operands.iter().map(|operand| self.graph_type(operand, span)).collect::<Result<Vec<_>, _>>()?;
            let mut state = self.generic.borrow_mut();
            let family = {
                let super::generic::GenericState { facts, language_operations, .. } = &mut *state;
                language_operations.display_family(&mut facts.graph, stderr, arguments.len(), span)?
            };
            let graph = &mut state.facts.graph;
            let result = graph.atom(crate::sema::inference::Atom::Unit)?;
            let effects = EffectSummary::Closed(EffectSet::EMPTY);
            let reason = graph.reason(span, None)?;
            let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: arguments.iter().copied().map(Some).collect(), result, effects, effect_bindings: Vec::new(), output_effect_bindings: Vec::new() }, reason)?;
            graph.solve()?;
            if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).ok_or(InferenceError::InvalidScheme)?.requirements.push(requirement); }
            let supplied_slots = (0..arguments.len()).collect();
            state.facts.statement_operations.insert(identity, SolvedOperation { requirement, result, effects, receiver: None, actual_arguments: arguments, argument_coercions: Vec::new(), binding: CallBinding { supplied_slots, default_slots: Vec::new(), rest_slot: None, dynamic: None }, caller: self.current_generic });
            state.facts.statements.entry(identity).or_insert(StatementPosition::Statement);
            if let Some(owner) = self.current_generic { state.facts.statement_owners.insert(identity, owner); }
            Ok::<_, InferenceError>(())
        })();
        if let Err(error) = outcome { self.graph_error(span, error); }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Checker;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    #[test]
    fn source_display_commands_retain_forwarded_definition_owned_requirements() {
        for command in ["print", "eprint"] {
            for calls in ["emit(7)\nemit(\"word\")\n", "emit(\"word\")\nemit(7)\n"] {
                let source = format!("proc display(value) [] -> Unit {{ {command} $value }}\nproc emit(value) [] -> Unit {{ display(value) }}\n{calls}");
                let parsed = Parser::parse_source_arena_only(SourceId::new(17), &source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let checked = Checker::check_arena(&parsed.arena, &source);
                assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
                assert_eq!(checked.solved.statement_operations.len(), 1);
                let (identity, operation) = checked.solved.statement_operations.iter().next().unwrap();
                assert_eq!(identity.source, SourceId::new(17));
                assert!(checked.solved.declarations[&operation.caller.unwrap()].source_requirements.contains(&operation.requirement));
                assert!(checked.solved.declarations.values().all(|declaration| !checked.solved.graph.scheme(declaration.scheme).unwrap().requirements.is_empty()));
                assert_eq!(operation.actual_arguments.len(), 1);
                assert_eq!(operation.effects, crate::sema::inference::EffectSummary::Closed(crate::sema::inference::EffectSet::EMPTY));
                assert!(checked.solved.graph.candidate_evidence(operation.requirement).unwrap().is_none());
                let crate::sema::inference::RequirementTemplate::Operation { family, .. } = checked.solved.graph.requirement_template(operation.requirement).unwrap() else { panic!("display must retain its operation family") };
                let candidates = checked.solved.graph.family(family).unwrap();
                assert_eq!(candidates.len(), 1);
                let authority = checked.solved.operation_catalog.candidate(&checked.solved.graph, candidates[0]).unwrap();
                assert!(matches!(authority, super::super::SolvedOperationAuthority::Language(metadata)
                    if metadata.authority == format!("language.command.{command}") && metadata.operation == crate::sema::operation_graph::PreparedLanguageOperation::Display { stderr: command == "eprint" }));
                let caller_evidence = checked.solved.calls.values().filter(|call| call.caller.is_none()).flat_map(|call| &call.requirements)
                    .filter_map(|requirement| checked.solved.graph.candidate_evidence(*requirement).unwrap()).collect::<Vec<_>>();
                assert_eq!(caller_evidence.len(), 2);
                assert!(caller_evidence.iter().all(|evidence| evidence.candidate == candidates[0]));
                drop(parsed);
                checked.solved.validate().unwrap();
            }
        }
    }

    #[test]
    fn source_display_commands_keep_each_scalar_domain_and_word_fragment_requirement() {
        let declarations = "proc display(value) [] -> Unit { print $value; eprint prefix${value}suffix }\nproc erased(value: Any) [] -> Unit { display(value) }\n";
        for value in ["7", "1.5", "true", "\"word\"", "p\"path\"", "1s"] {
            let source = format!("{declarations}display({value})\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(20), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            assert_eq!(checked.solved.statement_operations.len(), 2);
            drop(parsed);
            checked.solved.validate().unwrap();
        }
        let source = format!("{declarations}let unsigned: UInt = 7\ndisplay(unsigned)\nprint literal\neprint --flush literal\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(20), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.statement_operations.len(), 4);
        assert!(checked.solved.statement_operations.iter().filter(|(_, operation)| operation.caller.is_none()).all(|(identity, operation)|
            checked.solved.statements.contains_key(identity) && operation.effects == crate::sema::inference::EffectSummary::Closed(crate::sema::inference::EffectSet::EMPTY)));
        drop(parsed);
        checked.solved.validate().unwrap();
    }

    #[test]
    fn source_display_commands_reject_non_scalar_instances_and_disconnected_requirements() {
        for value in ["[7]", "b\"bytes\"", "{value: 7}", "null", "Ok(7)"] {
            let source = format!("proc display(value) [] -> Unit {{ print $value }}\nproc emit(value) [] -> Unit {{ display(value) }}\nemit({value})\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(21), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(!checked.diagnostics.is_empty(), "unsupported display accepted: {source}");
            drop(parsed);
            checked.solved.validate().unwrap();
        }
        let source = "pure identity(value) { value }\nprint ${identity([])}\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(21), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(!checked.diagnostics.is_empty(), "disconnected display contract cannot be hidden");
        drop(parsed);
        checked.solved.validate().unwrap();
    }
}
