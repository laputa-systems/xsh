use super::{Checker, ExpressionIdentity, ProducerFlowId, ProducerFlowKind, ProducerFlowSource, RecordUpdateValueSource, SolvedProjection, SolvedRecordUpdate, SolvedRecordUpdateReplacement, Type};
use crate::sema::inference::{InferenceError, TypeId};
use crate::source::Span;
use crate::symbol::Name;
use crate::syntax::arena::{ArenaProgram, ExprId};

impl Checker {
    pub(super) fn graph_record_update_path(&mut self, arena: &ArenaProgram, expression: ExprId, receiver: TypeId, path: &[Name], span: Span) -> Option<(Type, Vec<SolvedProjection>)> {
        let identity = self.expression_identity(arena, expression);
        if !self.graph_generation {
            let state = self.generic.borrow();
            let replacement = state.facts.record_updates.get(&identity)?.replacements.iter().find(|replacement| replacement.path == path)?;
            return Some((self.graph_view(replacement.projections.last()?.result), replacement.projections.clone()));
        }
        let outcome = (|| {
            let level = self.local_initializer_level.unwrap_or(if self.current_generic.is_some() { 1 } else { 0 });
            let mut state = self.generic.borrow_mut();
            let reason = state.facts.graph.reason(span, None)?;
            let mut selected = receiver;
            let mut projections = Vec::new();
            for &field in path {
                let result = state.facts.graph.require_field(selected, field, level, reason)?;
                projections.push(SolvedProjection { receiver: selected, field, result });
                selected = result;
            }
            Ok::<_, InferenceError>((selected, projections))
        })();
        match outcome {
            Ok((selected, projections)) => Some((self.graph_view(selected), projections)),
            Err(InferenceError::MissingField(_) | InferenceError::TypeMismatch { .. }) => {
                self.error(span, "every update target must select an existing field through known records", "check.record-update-field");
                None
            }
            Err(error) => { self.graph_error(span, error); None }
        }
    }

    pub(super) fn record_update_leaf_relation(&mut self, expected: TypeId, actual: TypeId, span: Span) -> Option<usize> {
        if !self.graph_generation { return None; }
        let outcome = (|| {
            let mut state = self.generic.borrow_mut();
            let reason = state.facts.graph.reason(span, None)?;
            let contribution = state.facts.graph.constraint_origins().len();
            state.facts.graph.assignable(expected, actual, reason)?;
            Ok::<_, InferenceError>(contribution)
        })();
        match outcome { Ok(contribution) => Some(contribution), Err(error) => { self.graph_error(span, error); None } }
    }

    pub(super) fn record_update_value_source(&mut self, arena: &ArenaProgram, expression: ExprId, shorthand: Option<Name>, span: Span) -> Option<(RecordUpdateValueSource, ProducerFlowId)> {
        if !self.graph_generation { return None; }
        let outcome = (|| {
            if let Some(name) = shorthand {
                let binding = self.lookup(name).ok_or(InferenceError::Boundary("record update shorthand has no lexical binding"))?;
                let flow = binding.producer_flow.ok_or(InferenceError::Boundary("record update shorthand needs a retained lexical source"))?;
                let source = if let Some((identity, version)) = binding.producer_binding {
                    RecordUpdateValueSource::Binding { identity, version }
                } else {
                    match self.generic.borrow().facts.producer_flows.node(flow)?.source {
                        ProducerFlowSource::Parameter { declaration, index } => RecordUpdateValueSource::Parameter { declaration, index },
                        ProducerFlowSource::Binding { identity, version } => RecordUpdateValueSource::Binding { identity, version },
                        _ => return Err(InferenceError::Boundary("record update shorthand needs a retained lexical source")),
                    }
                };
                return Ok((source, flow));
            }
            let identity = self.expression_identity(arena, expression);
            let flow = self.generic.borrow().facts.expression_producer_flows.get(&identity).copied();
            let flow = if let Some(flow) = flow { flow } else {
                let flow = self.push_source_producer_flow(ProducerFlowSource::Expression(identity), ProducerFlowKind::Opaque, span).ok_or(InferenceError::Boundary("record update replacement has no retained source flow"))?;
                self.generic.borrow_mut().facts.expression_producer_flows.insert(identity, flow);
                flow
            };
            Ok::<_, InferenceError>((RecordUpdateValueSource::Expression(identity), flow))
        })();
        match outcome { Ok(value) => Some(value), Err(error) => { self.graph_error(span, error); None } }
    }

    pub(super) fn record_graph_record_update(&mut self, arena: &ArenaProgram, expression: ExprId, base: ExprId, receiver: TypeId, replacements: Vec<SolvedRecordUpdateReplacement>) {
        if !self.graph_generation { return; }
        let identity: ExpressionIdentity = self.expression_identity(arena, expression);
        let base = self.expression_identity(arena, base);
        let mut state = self.generic.borrow_mut();
        state.facts.expressions.insert(identity, receiver);
        if let Some(owner) = self.current_generic { state.facts.expression_owners.insert(identity, owner); }
        state.facts.record_updates.insert(identity, SolvedRecordUpdate { base, receiver, result: receiver, replacements, caller: self.current_generic });
    }
}

#[cfg(test)]
mod tests {
    use super::super::Checker;
    use crate::sema::inference::TypeNode;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    #[test]
    fn generic_record_updates_preserve_complete_rows_through_forwarding() {
        let declarations = "type SmallSettings = {count: Int}\ntype LargeSettings = {count: Str, enabled: Bool}\ntype Small = {settings: SmallSettings}\ntype Large = {settings: LargeSettings, label: Str}\npure updated(base, value) { {...base, settings.count: value} }\npure forwarded(base, value) { updated(base, value) }\n";
        let calls = ["let small: Small = forwarded(Small(settings: SmallSettings(count: 1)), 7)\n", "let large: Large = forwarded(Large(settings: LargeSettings(count: \"old\", enabled: true), label: \"kept\"), \"new\")\nlet enabled: Bool = large.settings.enabled\nlet label: Str = large.label\n"];
        for reverse in [false, true] {
            let mut calls = calls.to_vec();
            if reverse { calls.reverse(); }
            let source = format!("{declarations}{}", calls.concat());
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let mut checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            assert_eq!(checked.solved.record_updates.len(), 1);
            let (identity, update) = checked.solved.record_updates.iter().next().unwrap();
            assert_eq!(update.receiver, update.result);
            assert_eq!(checked.solved.expression_owners.get(identity).copied(), update.caller);
            assert_eq!(update.replacements.len(), 1);
            assert_eq!(update.replacements[0].path.iter().map(|name| name.as_str()).collect::<Vec<_>>(), vec!["settings", "count"]);
            assert_eq!(update.replacements[0].projections.len(), 2);
            let forwarded = checked.solved.declarations.keys().find(|identity| parsed.arena.arena.function_def(identity.declaration).name == "forwarded").copied().unwrap();
            let calls: Vec<_> = checked.solved.calls.values().filter(|call| call.declaration == Some(forwarded)).collect();
            assert_eq!(calls.len(), 2);
            for call in calls {
                let TypeNode::Arrow(signature) = checked.solved.graph.node(call.signature).unwrap() else { panic!("the source call retains its signature") };
                assert_eq!(checked.solved.graph.resolved(signature.result).unwrap(), checked.solved.graph.resolved(signature.params[0].ty).unwrap());
                assert_eq!(checked.solved.graph.export_type(signature.result).unwrap(), checked.solved.graph.export_type(call.actual_arguments[0]).unwrap());
            }
            drop(parsed);
            checked.solved.validate().unwrap();
            let unrelated = checked.solved.expressions.values().copied().find(|ty| matches!(checked.solved.graph.node(checked.solved.graph.resolved(*ty).unwrap()).unwrap(), TypeNode::Atom(crate::sema::inference::Atom::Bool))).unwrap();
            let identity = *checked.solved.record_updates.keys().next().unwrap();
            let original = checked.solved.record_updates[&identity].replacements[0].value;
            std::sync::Arc::get_mut(&mut checked.solved).unwrap().record_updates.get_mut(&identity).unwrap().replacements[0].value = unrelated;
            assert!(checked.solved.validate().is_err(), "a different replacement type cannot reuse the original field assignment proof");
            std::sync::Arc::get_mut(&mut checked.solved).unwrap().record_updates.get_mut(&identity).unwrap().replacements[0].value = original;
            checked.solved.validate().unwrap();
        }
    }
    #[test]
    fn generic_record_updates_retain_disjoint_paths_nominals_and_lexical_versions() {
        let source = "enum Mode { Fast, Slow }\ntype Settings = {count: Int, enabled: Bool}\ntype Config = {settings: Settings, mode: Mode, label: Str, untouched: Bytes}\npure changed(base, count, enabled, mode: Mode, label) { {...base, settings.count: count, settings.enabled: enabled, mode, label} }\npure renamed(base, text) { var label = text; label = text; {...base, settings.count: base.settings.count, label} }\nlet original = Config(settings: Settings(count: 1, enabled: false), mode: Fast, label: \"old\", untouched: b\"kept\")\nlet changed: Config = changed(original, 7, true, Slow, \"new\")\nlet renamed: Config = renamed(original, \"renamed\")\nlet kept: Bytes = renamed.untouched\nlet mode: Mode = changed.mode\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.solved.record_updates.len(), 2);
        for update in checked.solved.record_updates.values() {
            assert_eq!(update.receiver, update.result);
            if update.replacements.len() == 4 {
                assert_eq!(update.replacements.iter().map(|replacement| replacement.path.iter().map(|name| name.as_str()).collect::<Vec<_>>()).collect::<Vec<_>>(), vec![vec!["settings", "count"], vec!["settings", "enabled"], vec!["mode"], vec!["label"]]);
                let mode = &update.replacements[2];
                assert!(matches!(mode.source, super::RecordUpdateValueSource::Parameter { index: 3, .. }));
                assert!(checked.solved.nominals.contains_key(&checked.solved.graph.resolved(mode.projections.last().unwrap().result).unwrap()));
                assert!(matches!(update.replacements[3].source, super::RecordUpdateValueSource::Parameter { index: 4, .. }));
            } else {
                assert!(matches!(update.replacements[1].source, super::RecordUpdateValueSource::Binding { version: 1, .. }));
            }
            for replacement in &update.replacements { checked.solved.producer_flows.node(replacement.producer_flow).unwrap(); }
        }
        drop(parsed);
        checked.solved.validate().unwrap();
    }

    #[test]
    fn generic_record_updates_reject_changed_fields_missing_paths_and_overlaps() {
        let update = "pure updated(base, value) { {...base, settings.count: value} }\npure forwarded(base, value) { updated(base, value) }\n";
        for (source, code) in [
            (format!("{update}let invalid = forwarded({{settings: {{count: 1}}}}, false)\n"), "check.type-mismatch"),
            (format!("{update}let invalid = forwarded({{settings: [1]}}, 2)\n"), "check.type-mismatch"),
            (format!("{update}let invalid = forwarded({{settings: {{different: 1}}}}, 2)\n"), "check.type-relationship"),
            ("pure updated(base, value: Any) { {...base, settings.count: value} }\n".to_string(), "check.record-update-value"),
            ("pure updated(base) { {...base, settings.count: 1, settings: {count: 2}} }\n".to_string(), "check.record-update-overlap"),
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some(code)), "{source}: {:?}", checked.diagnostics);
            if code == "check.type-relationship" { assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.message.contains("count")), "{:?}", checked.diagnostics); }
        }
    }

    #[test]
    fn generic_record_updates_replace_only_selected_producer_subtrees() {
        let source = "stream delayed() [time] -> Stream[Int] { let _ = time.now(); yield 1 }\nstream quiet() [] -> Stream[Int] { yield 2 }\npure changed(base, value) { {...base, nested.rows: value} }\npure forwarded(base, value) { changed(base, value) }\nproc consume(base, value) -> Unit { let updated = forwarded(base, value); for item in updated.nested.rows { let _ = item; break } }\nlet original = {nested: {rows: delayed()}, retained: delayed()}\nlet updated = changed(original, quiet())\nproc accepted() [] -> Unit { for item in updated.nested.rows { let _ = item; break }; consume(original, quiet()) }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let consume = checked.solved.declarations.iter().find(|(identity, _)| parsed.arena.arena.function_def(identity.declaration).name == "consume").unwrap().1;
        assert!(consume.parameter_producers[0].is_empty(), "the replaced base subtree cannot become a producer demand");
        assert!(consume.parameter_producers[1].contains_key(&super::super::ProducerPath::default()));
        drop(parsed);
        checked.solved.validate().unwrap();
        for denied in [source.replace("updated.nested.rows", "original.nested.rows"), source.replace("updated.nested.rows", "updated.retained"), source.replace("consume(original, quiet())", "consume(original, delayed())")] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &denied);
            let checked = Checker::check_arena(&parsed.arena, &denied);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.effect-violation") && diagnostic.message.contains("time")), "{:?}", checked.diagnostics);
        }
    }

}
