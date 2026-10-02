use super::{CallBinding, Checker, ProducerFlowId, ProducerFlowKind, ProducerFlowSource, ProducerPath, ProducerPathComponent, SolvedOperation, StatementIdentity, StatementPosition, Type};
use crate::sema::inference::{EffectSet, EffectSummary, InferenceError, OperationCall, ProducerRole};
use crate::source::SourceId;
use crate::symbol::Name;
use crate::syntax::arena::{ArenaExprKind, ArenaProgram, RunFormId};
use crate::syntax::node::RunKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct RunIdentity {
    pub source: SourceId,
    pub namespace: Option<Name>,
    pub run: RunFormId,
}

/// Run observations retain the real source plan, including the completion
/// policy that distinguishes a live process cursor from captured output.
#[derive(Clone, Debug)]
pub(crate) struct SolvedRun {
    pub parent: ProducerFlowSource,
    pub kind: RunKind,
    pub policy: bool,
    pub propagate: bool,
    pub usage: RunUse,
    pub operation: SolvedOperation,
    pub producer_flow: ProducerFlowId,
    pub arguments: Vec<RunArgumentGuard>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RunUse { Value, Command }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RunArgumentMode { Single, Environment, Expansion, Display, Splice }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RunArgumentSource {
    Expression(super::ExpressionIdentity),
    NamedSplice { span: crate::source::Span, name: Name },
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RunArgumentGuard {
    pub source: RunArgumentSource,
    pub actual: crate::sema::inference::TypeId,
    pub operand: crate::sema::inference::TypeId,
    pub mode: RunArgumentMode,
    pub requirement: crate::sema::inference::RequirementId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SpawnTarget {
    Run(RunIdentity),
    Command(super::ExpressionIdentity),
}

#[derive(Clone, Debug)]
pub(crate) struct SolvedSpawn {
    pub target: SpawnTarget,
    pub requirement: crate::sema::inference::RequirementId,
    pub arguments: Vec<RunArgumentGuard>,
}

impl Checker {
    pub(super) fn begin_run_arguments(&mut self) { self.generic.borrow_mut().run_argument_stack.push(Vec::new()); }

    pub(super) fn check_graph_run_argument(&mut self, source: RunArgumentSource, actual: &Type, mode: RunArgumentMode, span: crate::source::Span) -> bool {
        if !self.graph_generation || matches!(actual, Type::Unknown | Type::Invalid) { return false; }
        let mut own_requirement = None;
        let outcome = (|| {
            // Expression checking has already published the source port. A
            // closed composite view would allocate another port if imported
            // here, disconnecting rendering authority from that expression.
            let actual = match source {
                RunArgumentSource::Expression(identity) => {
                    let state = self.generic.borrow();
                    if identity.source != span.source_id || identity.namespace != self.current_namespace
                        || state.facts.expression_owners.get(&identity).copied() != self.current_generic {
                        return Err(InferenceError::InvalidScheme);
                    }
                    let original = *state.facts.expressions.get(&identity).ok_or(InferenceError::InvalidScheme)?;
                    state.facts.graph.node(state.facts.graph.resolved(original)?)?;
                    original
                }
                RunArgumentSource::NamedSplice { .. } => self.graph_type(actual, span)?,
            };
            let mut state = self.generic.borrow_mut();
            let reason = state.facts.graph.reason(span, None)?;
            let operand = if mode == RunArgumentMode::Splice {
                let level = if self.current_generic.is_some() { 1 } else { 0 };
                let item = state.facts.graph.fresh(level, span)?;
                let list = state.facts.graph.list(item)?;
                state.facts.graph.unify(actual, list, reason)?;
                item
            } else { actual };
            let predicate = match mode {
                RunArgumentMode::Single | RunArgumentMode::Environment | RunArgumentMode::Splice => crate::sema::inference::Eligibility::ArgvItem,
                RunArgumentMode::Expansion => crate::sema::inference::Eligibility::ArgvExpansion,
                RunArgumentMode::Display => crate::sema::inference::Eligibility::Display,
            };
            let requirement = state.facts.graph.require_eligibility(predicate, operand, reason)?;
            own_requirement = Some(requirement);
            state.facts.graph.solve()?;
            if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).ok_or(InferenceError::InvalidScheme)?.requirements.push(requirement); }
            if let Some(arguments) = state.run_argument_stack.last_mut() {
                arguments.push(RunArgumentGuard { source, actual, operand, mode, requirement });
                state.facts.graph.charge_source_fact_nodes(1)?;
            }
            Ok::<_, InferenceError>(())
        })();
        if let Err(error) = outcome {
            if matches!(error, InferenceError::UnsupportedOperation(requirement) if Some(requirement) == own_requirement) {
                if mode == RunArgumentMode::Environment { self.error(span, "environment value cannot convert to one value", "check.env-value"); }
                else { self.error(span, "invalid argv interpolation conversion", "check.argv-conversion"); }
            } else { self.graph_error(span, error); }
        }
        true
    }

    pub(super) fn record_graph_spawn(&mut self, arena: &ArenaProgram, form: &crate::syntax::arena::ArenaSpawnForm, command: Option<&Type>, checked: &Type) {
        let source_arguments = self.generic.borrow_mut().run_argument_stack.pop().unwrap_or_default();
        if !self.graph_generation { return; }
        let Some(expression) = self.current_expression else { return; };
        let identity = self.expression_identity(arena, expression);
        let span = arena.arena.span(form.span);
        let target = match form.target {
            crate::syntax::arena::ArenaSpawnTarget::Run(run) => SpawnTarget::Run(RunIdentity { source: arena.arena.span(arena.arena.run_form(run).span).source_id, namespace: self.current_namespace, run }),
            crate::syntax::arena::ArenaSpawnTarget::Command(expression) => SpawnTarget::Command(self.expression_identity(arena, expression)),
        };
        if let crate::syntax::arena::ArenaSpawnTarget::Run(run) = form.target {
            let segments = arena.arena.run_segments(arena.arena.run_form(run).segments);
            if segments.len() != 1 || !matches!(segments[0].kind, RunKind::Plain | RunKind::Status) { return; }
        }
        let outcome = (|| {
            let arguments = command.map(|command| self.graph_type(command, span)).transpose()?.into_iter().collect::<Vec<_>>();
            let result = self.graph_type(checked, span)?;
            let mut state = self.generic.borrow_mut();
            let family = { let super::generic::GenericState { facts, language_operations, .. } = &mut *state; language_operations.spawn_family(&mut facts.graph, command.is_some())? };
            let effects = EffectSummary::Closed(EffectSet::PROCESS);
            let reason = state.facts.graph.reason(span, None)?;
            let requirement = state.facts.graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: arguments.iter().copied().map(Some).collect(), result, effects,
                effect_bindings: Vec::new(), output_effect_bindings: Vec::new() }, reason)?;
            state.facts.graph.solve()?;
            state.facts.graph.charge_source_fact_nodes(1)?;
            if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).ok_or(InferenceError::InvalidScheme)?.requirements.push(requirement); state.facts.expression_owners.insert(identity, owner); }
            let supplied_slots = (0..arguments.len()).collect();
            state.facts.operations.insert(identity, SolvedOperation { requirement, result, effects, receiver: None, actual_arguments: arguments, argument_coercions: Vec::new(),
                binding: CallBinding { supplied_slots, default_slots: Vec::new(), rest_slot: None, dynamic: None }, caller: self.current_generic });
            state.facts.expressions.insert(identity, result);
            state.facts.spawn_operations.insert(identity, SolvedSpawn { target, requirement, arguments: source_arguments });
            drop(state);
            self.push_source_producer_flow(ProducerFlowSource::Expression(identity), ProducerFlowKind::Empty, span);
            Ok::<_, InferenceError>(effects)
        })();
        match outcome { Ok(effects) => self.record_graph_effect_summary(effects, span), Err(error) => self.graph_error(span, error) }
    }

    pub(super) fn graph_run_flow(&self, arena: &ArenaProgram, run: RunFormId) -> Option<ProducerFlowId> {
        let identity = RunIdentity { source: arena.arena.span(arena.arena.run_form(run).span).source_id, namespace: self.current_namespace, run };
        self.generic.borrow().facts.run_operations.get(&identity).map(|run| run.producer_flow)
    }

    pub(super) fn record_graph_run(&mut self, arena: &ArenaProgram, run: RunFormId, checked: &Type) {
        let source_arguments = self.generic.borrow_mut().run_argument_stack.pop().unwrap_or_default();
        if !self.graph_generation || matches!(checked, Type::Invalid | Type::Unknown) { return; }
        let form = arena.arena.run_form(run);
        let span = arena.arena.span(form.span);
        let identity = RunIdentity { source: span.source_id, namespace: self.current_namespace, run };
        if self.generic.borrow().facts.run_operations.contains_key(&identity) { return; }
        let Some(segment) = arena.arena.run_segments(form.segments).first() else { return; };
        let kind = segment.kind;
        let policy = segment.accept.is_some();
        let parent = if let Some(expression) = self.current_expression.filter(|&expression| matches!(arena.arena.expr(expression).kind, ArenaExprKind::Run(id) if id == run)) {
            ProducerFlowSource::Expression(self.expression_identity(arena, expression))
        } else if let Some(statement) = self.current_statement {
            ProducerFlowSource::Statement(StatementIdentity { source: arena.arena.stmt(statement).span.source_id, namespace: self.current_namespace, statement })
        } else { self.graph_error(span, InferenceError::Boundary("run observation has no original source parent")); return; };
        let usage = match parent {
            ProducerFlowSource::Statement(identity) => match arena.arena.stmt(identity.statement).kind {
                crate::syntax::arena::ArenaStmtKind::Command(command) if matches!(arena.arena.command_stmt(command).command, crate::syntax::arena::ArenaCommand::Run(id) if id == run) => RunUse::Command,
                _ => RunUse::Value,
            },
            _ => RunUse::Value,
        };
        let outcome = (|| {
            let result = self.graph_type(checked, span)?;
            let mut state = self.generic.borrow_mut();
            let family = {
                let super::generic::GenericState { facts, language_operations, .. } = &mut *state;
                language_operations.run_family(&mut facts.graph, kind, policy, form.propagate)?
            };
            let effects = EffectSummary::Closed(EffectSet(EffectSet::PROCESS.0 | if form.propagate { EffectSet::ERROR.0 } else { 0 }));
            let pull = EffectSummary::Closed(if policy { EffectSet(EffectSet::PROCESS.0 | EffectSet::ERROR.0) } else { EffectSet::EMPTY });
            let close = EffectSummary::Closed(if policy { EffectSet::PROCESS } else { EffectSet::EMPTY });
            let producer = matches!(kind, RunKind::StreamText | RunKind::StreamBytes);
            let reason = state.facts.graph.reason(span, None)?;
            let requirement = state.facts.graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver: None, arguments: Vec::new(), result, effects, effect_bindings: Vec::new(),
                output_effect_bindings: if producer { vec![(ProducerRole::Pull, pull), (ProducerRole::Close, close)] } else { Vec::new() } }, reason)?;
            state.facts.graph.solve()?;
            state.facts.graph.charge_source_fact_nodes(1)?;
            if let Some(owner) = self.current_generic { state.pending.get_mut(&owner).ok_or(InferenceError::InvalidScheme)?.requirements.push(requirement); }
            match parent {
                ProducerFlowSource::Expression(identity) => { state.facts.expressions.insert(identity, result); if let Some(owner) = self.current_generic { state.facts.expression_owners.insert(identity, owner); } }
                ProducerFlowSource::Statement(identity) => { state.facts.statements.entry(identity).or_insert(StatementPosition::Statement); if let Some(owner) = self.current_generic { state.facts.statement_owners.insert(identity, owner); } }
                _ => return Err(InferenceError::InvalidScheme),
            }
            let profile = if producer { [(if form.propagate { ProducerPath::default() } else { ProducerPath(vec![ProducerPathComponent::ResultSuccess]) }, super::ProducerEffects { pull, close })].into() } else { Default::default() };
            let facts = &mut state.facts;
            let flow = facts.producer_flows.push(&mut facts.graph, parent, if producer { ProducerFlowKind::Known(profile) } else { ProducerFlowKind::Empty })?;
            state.facts.run_operations.insert(identity, SolvedRun { parent, kind, policy, propagate: form.propagate, usage, producer_flow: flow, arguments: source_arguments,
                operation: SolvedOperation { requirement, result, effects, receiver: None, actual_arguments: Vec::new(), argument_coercions: Vec::new(),
                    binding: CallBinding { supplied_slots: Vec::new(), default_slots: Vec::new(), rest_slot: None, dynamic: None }, caller: self.current_generic } });
            Ok::<_, InferenceError>(effects)
        })();
        match outcome { Ok(effects) => self.record_graph_effect_summary(effects, span), Err(error) => self.graph_error(span, error) }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Checker;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    #[test]
    fn source_run_observations_retain_all_eight_canonical_operations() {
        for form in ["run", "run.status", "run.text", "run.bytes", "run.capture --text", "run.capture --bytes", "run.stream --text", "run.stream --bytes"] {
            let source = format!("proc observe() [process] -> Unit {{ let value = {form} true; let _ = value }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(41), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            assert_eq!(checked.solved.run_operations.len(), 1, "{form} must retain its checked process observation");
            let (identity, run) = checked.solved.run_operations.iter().next().unwrap();
            assert_eq!(identity.source, SourceId::new(41));
            assert_eq!(run.operation.effects, crate::sema::inference::EffectSummary::Closed(crate::sema::inference::EffectSet::PROCESS));
            let evidence = checked.solved.graph.candidate_evidence(run.operation.requirement).unwrap().unwrap();
            let super::super::SolvedOperationAuthority::Language(authority) = checked.solved.operation_catalog.candidate(&checked.solved.graph, evidence.candidate).unwrap() else { panic!("run must retain its language authority") };
            assert_eq!(authority.authority, format!("language.run.{:?}", run.kind));
            let result = checked.solved.graph.export_type(run.operation.result).unwrap();
            let expected = match run.kind {
                crate::syntax::node::RunKind::Plain | crate::syntax::node::RunKind::Status => super::super::Type::Status,
                crate::syntax::node::RunKind::CaptureText => super::super::Type::Result(Box::new(super::super::Type::Str), Box::new(super::super::Type::ProcessError)),
                crate::syntax::node::RunKind::CaptureBytes => super::super::Type::Result(Box::new(super::super::Type::Bytes), Box::new(super::super::Type::ProcessError)),
                crate::syntax::node::RunKind::CaptureTextRecord | crate::syntax::node::RunKind::CaptureBytesRecord => {
                    let item = if run.kind == crate::syntax::node::RunKind::CaptureTextRecord { super::super::Type::Str } else { super::super::Type::Bytes };
                    let fields = [(crate::symbol::Name::intern("status"), super::super::Type::Status), (crate::symbol::Name::intern("stdout"), item.clone()), (crate::symbol::Name::intern("stderr"), item)].into();
                    super::super::Type::Result(Box::new(super::super::Type::Record(fields)), Box::new(super::super::Type::ProcessError))
                }
                crate::syntax::node::RunKind::StreamText | crate::syntax::node::RunKind::StreamBytes => {
                    let item = if run.kind == crate::syntax::node::RunKind::StreamText { super::super::Type::Str } else { super::super::Type::Bytes };
                    super::super::Type::Result(Box::new(super::super::Type::Stream(Box::new(item))), Box::new(super::super::Type::ProcessError))
                }
            };
            assert_eq!(result, expected, "{form}");
            drop(parsed);
            checked.solved.validate().unwrap();
        }
    }

    #[test]
    fn source_run_stream_policy_keeps_creation_pull_and_cleanup_distinct() {
        for mode in ["--text", "--bytes"] {
            for policy in [false, true] {
                let source = format!("proc produce() [process,error] {{ let rows = run.stream {mode} {} true; let _ = rows }}\n", if policy { "--accept=[0]" } else { "" });
                let parsed = Parser::parse_source_arena_only(SourceId::new(42), &source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let checked = Checker::check_arena(&parsed.arena, &source);
                assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
                let run = checked.solved.run_operations.values().next().unwrap();
                assert_eq!(run.policy, policy);
                assert_eq!(run.operation.effects, crate::sema::inference::EffectSummary::Closed(crate::sema::inference::EffectSet::PROCESS));
                let super::super::ProducerFlowKind::Known(profile) = &checked.solved.producer_flows.node(run.producer_flow).unwrap().kind else { panic!("process observation owns its producer profile") };
                let effects = profile.values().next().unwrap();
                let crate::sema::inference::EffectSummary::Closed(pull) = effects.pull else { panic!() };
                let crate::sema::inference::EffectSummary::Closed(close) = effects.close else { panic!() };
                assert_eq!(pull.0, if policy { crate::sema::inference::EffectSet::PROCESS.0 | crate::sema::inference::EffectSet::ERROR.0 } else { 0 });
                assert_eq!(close.0, if policy { crate::sema::inference::EffectSet::PROCESS.0 } else { 0 });
                checked.solved.validate().unwrap();
            }
        }
    }

    #[test]
    fn source_spawn_retains_command_and_single_run_authorities() {
        for target in ["run true", "run.status true", "process.command_argv(\"true\", [\"true\"])"] {
            let source = format!("proc start() [process] {{ let handle = spawn {target}; let _ = handle }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(43), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            assert_eq!(checked.solved.spawn_operations.len(), 1);
            assert!(checked.solved.operations.values().any(|operation| {
                let Some(evidence) = checked.solved.graph.candidate_evidence(operation.requirement).unwrap() else { return false; };
                matches!(checked.solved.operation_catalog.candidate(&checked.solved.graph, evidence.candidate).unwrap(), super::super::SolvedOperationAuthority::Language(authority) if authority.authority == "language.spawn")
            }), "spawn must retain its own checked operation");
            drop(parsed);
            checked.solved.validate().unwrap();
        }
    }

    #[test]
    fn source_run_and_spawn_preserve_process_permissions_and_reject_invalid_plans() {
        for source in [
            "proc observe() [] -> Unit { let value = run.text true; let _ = value }\n",
            "proc observe() [process] -> Unit { let value = run.text true ?; let _ = value }\n",
            "proc observe() [] { spawn run true }\n",
            "proc observe() [process] { spawn run.text true }\n",
            "proc observe() [process] { spawn run true | run cat }\n",
            "proc observe() [process,error] -> Unit { let value = run --accept=[] true; let _ = value }\n",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(44), source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(!checked.diagnostics.is_empty(), "invalid process source accepted: {source}");
        }
    }

    #[test]
    fn source_run_producer_binding_preserves_live_and_materialized_permissions() {
        for policy in [false, true] {
            let source = format!("let rows = run.stream --text {} true ?\nproc consume() [] -> Unit {{ for item in rows {{ let _ = item; break }} }}\n", if policy { "--accept=[0]" } else { "" });
            let parsed = Parser::parse_source_arena_only(SourceId::new(45), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            if policy {
                assert!(!checked.diagnostics.is_empty(), "live process cursor consumption needs process and error permissions");
            } else {
                assert!(checked.diagnostics.is_empty(), "captured output consumption has no process work: {:?}", checked.diagnostics);
                drop(parsed);
                checked.solved.validate().unwrap();
            }
        }
    }

    #[test]
    fn source_run_forwarded_arguments_retain_native_argv_guards() {
        let prefix = "proc execute(value) [process] { run.text true $value }\nproc forwarded(value) [process] { execute(value) }\n";
        for argument in ["\"word\"", "Path(\"item\")", "7", "unsigned", "true", "1s", "dynamic", "[Path(\"item\"), Path(\"next\")]"] {
            let source = format!("{prefix}let unsigned: UInt = 1\nlet dynamic: Any = 1\nlet _ = forwarded({argument})\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(46), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            checked.solved.validate().unwrap();
        }
        for argument in ["{item: 7}", "[[1]]", "bytes.from_text(\"bad\")", "1.5", "rows()"] {
            let source = format!("stream rows() [] {{ yield 1 }}\n{prefix}let _ = forwarded({argument})\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(46), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(!checked.diagnostics.is_empty(), "invalid native argv item accepted: {argument}");
        }
    }

    #[test]
    fn source_run_splice_and_embedded_conversions_keep_original_guard_modes() {
        for (body, accepted, rejected, mode) in [
            ("run.text true @values", "[Path(\"item\")]", "[bytes.from_text(\"bad\")]", super::RunArgumentMode::Splice),
            ("run.text true \"prefix-${values}\"", "1.5", "{item: 7}", super::RunArgumentMode::Display),
            ("run.text NAME=$values true", "Path(\"item\")", "[Path(\"item\")]", super::RunArgumentMode::Environment),
        ] {
            let prefix = format!("proc execute(values) [process] {{ {body} }}\nproc forwarded(values) [process] {{ execute(values) }}\n");
            let source = format!("{prefix}let _ = forwarded({accepted})\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(47), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            let run = checked.solved.run_operations.values().next().unwrap();
            assert!(run.arguments.iter().any(|argument| argument.mode == mode));
            drop(parsed);
            checked.solved.validate().unwrap();
            let source = format!("{prefix}let _ = forwarded({rejected})\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(47), &source);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(!checked.diagnostics.is_empty(), "invalid conversion accepted: {source}");
        }
    }

    #[test]
    fn published_run_arguments_reject_changed_conversion_modes() {
        let source = "proc execute(value) [process] { run.text true $value }\nlet _ = execute(\"word\")\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(48), source);
        let mut checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        drop(parsed);
        checked.solved.validate().unwrap();
        let solved = std::sync::Arc::get_mut(&mut checked.solved).unwrap();
        let argument = &mut solved.run_operations.values_mut().next().unwrap().arguments[0];
        assert_eq!(argument.mode, super::RunArgumentMode::Expansion);
        argument.mode = super::RunArgumentMode::Display;
        assert!(solved.validate().is_err(), "a different conversion predicate cannot reuse the original guard");
    }

    #[test]
    fn source_plain_run_value_and_command_keep_distinct_consumption_contracts() {
        for (source, usage) in [
            ("proc observe() [process] -> Unit { let value = run true; let _ = value }\n", super::RunUse::Value),
            ("proc observe() [process,error] -> Unit { run true }\n", super::RunUse::Command),
            ("proc observe() [process] -> Unit { run.status false }\n", super::RunUse::Command),
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(49), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            let run = checked.solved.run_operations.values().next().unwrap();
            assert_eq!(run.usage, usage);
            drop(parsed);
            checked.solved.validate().unwrap();
        }
    }
}
