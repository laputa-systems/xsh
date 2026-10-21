use super::{BTreeMap, BTreeSet, Checker, EffectDeclarationId, FunctionDeclarationId, Span};
use crate::diagnostic::DiagnosticCode;
use crate::syntax::arena::{ArenaProgram, FunctionDefId};
use std::sync::Arc;

impl Checker {
    pub(super) fn generalize_recursive_component(
        &mut self,
        program: &ArenaProgram,
        definitions: &[FunctionDefId],
    ) -> Option<()> {
        let members = definitions.iter().map(|&definition| {
            let def = program.arena.function_def(definition);
            (self.effect_declaration_id(program, def.body), definition)
        }).collect::<BTreeMap<_, _>>();
        let mut edges = BTreeMap::<EffectDeclarationId, BTreeSet<EffectDeclarationId>>::new();
        for &(caller, callee) in self.provisional_function_calls.values() {
            let callee = callee.effect_declaration();
            if members.contains_key(&caller) && members.contains_key(&callee) {
                edges.entry(caller).or_default().insert(callee);
            }
        }
        if let Err(error) = self.retain_function_environment_monotypes(program, definitions) {
            if let Some(owner) = members.keys().next() {
                self.error(owner.body, &format!("recursive capture cannot retain its environment type: {error:?}"), DiagnosticCode::CheckLocalInference);
            }
            self.reject_recursive_component(program, definitions);
            return None;
        }
        let mut prepared = BTreeMap::new();
        for (&owner, &definition) in &members {
            let mut reachable = BTreeSet::new();
            let mut pending = vec![owner];
            while let Some(declaration) = pending.pop() {
                if reachable.insert(declaration) {
                    pending.extend(edges.get(&declaration).into_iter().flatten().copied());
                }
            }
            let Some(member) = self.prepare_function_generalization(program, definition, &reachable) else {
                self.reject_recursive_component(program, definitions);
                return None;
            };
            prepared.insert(owner, member);
        }
        let mut instances = BTreeMap::new();
        for (&owner, member) in &prepared {
            match self.type_constraints.reuse_callable_instance(&member.callable, &member.scope, owner, owner.body) {
                Ok(instance) => { instances.insert(owner, instance); }
                Err(error) => {
                    self.error(owner.body, &format!("recursive callable scope cannot be retained: {error:?}"), DiagnosticCode::CheckLocalInference);
                    self.reject_recursive_component(program, definitions);
                    return None;
                }
            }
        }
        let mut recursive_calls = BTreeMap::<Span, super::PendingCallInstantiation>::new();
        let mut substitutions = BTreeMap::new();
        let calls = self.provisional_function_calls.iter().map(|(&span, &edge)| (span, edge)).collect::<Vec<_>>();
        for (span, (owner, callee)) in calls {
            let (Some(caller), Some(callee)) = (prepared.get(&owner), prepared.get(&callee.effect_declaration())) else { continue; };
            if !caller.expression_spans.contains(&span) {
                self.error(span, "recursive call belongs to another function body", DiagnosticCode::CheckLocalInference);
                self.reject_recursive_component(program, definitions);
                return None;
            }
            if !callee.callable.is_quantified() { continue; }
            let callee_owner = self.effect_declaration_id(program, program.arena.function_def(callee.definition).body);
            let instance = &instances[&callee_owner];
            let substitution = match self.type_constraints.generalize_scoped_call_instantiation(
                &caller.callable, &caller.scope, &callee.callable, instance,
            ) {
                Ok(substitution) => substitution,
                Err(error) => {
                    self.error(span, &format!("recursive call has no shared component substitution: {error:?}"), DiagnosticCode::CheckLocalInference);
                    self.reject_recursive_component(program, definitions);
                    return None;
                }
            };
            substitutions.insert((owner, span), substitution);
            recursive_calls.insert(span, (Arc::clone(&callee.callable), instance.clone()));
        }
        for ((owner, span), substitution) in substitutions {
            if self.call_instantiations.contains_key(&span)
                || prepared.get_mut(&owner).expect("the recursive caller was prepared").calls.insert(span, substitution).is_some()
            {
                self.error(span, "recursive call already has a published substitution", DiagnosticCode::CheckLocalInference);
                self.reject_recursive_component(program, definitions);
                return None;
            }
        }
        if let Err((owner, _)) = self.resolve_stream_origin_applications(&members, &recursive_calls) {
            self.report_stream_origin_boundary(owner.body);
            self.reject_recursive_component(program, definitions);
            return None;
        }
        for member in prepared.into_values() {
            self.publish_function_generalization(program, member);
        }
        self.publish_recursive_invocation_instances(&recursive_calls);
        self.recursive_function_components.push(definitions.to_vec());
        Some(())
    }

    pub(super) fn recursive_component_has_errors(&self, program: &ArenaProgram, definitions: &[FunctionDefId]) -> bool {
        self.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error
            && diagnostic.labels.iter().any(|label| label.style == crate::diagnostic::LabelStyle::Primary
                && definitions.iter().any(|&definition| function_contains_span(program, definition, label.span))))
    }

    pub(super) fn reject_recursive_component(&mut self, program: &ArenaProgram, definitions: &[FunctionDefId]) {
        for &definition in definitions {
            let def = program.arena.function_def(definition);
            let declaration = FunctionDeclarationId::new(self.effect_declaration_id(program, def.body));
            if let Some(pending) = self.declaration_inference.pending_mut(declaration) {
                pending.reject_generalization();
            }
        }
        let rejected_calls = self.selected_function_calls.iter().filter_map(|(&span, target)| {
            definitions.contains(&target.definition).then_some(span)
        }).chain(self.call_instantiations.keys().copied().filter(|&span| {
            definitions.iter().any(|&definition| function_contains_span(program, definition, span))
        })).collect::<BTreeSet<_>>();
        self.call_instantiations.retain(|span, _| !rejected_calls.contains(span));
        self.pending_call_instantiations.retain(|span, _| !rejected_calls.contains(span));
    }
}

fn function_contains_span(program: &ArenaProgram, definition: FunctionDefId, span: Span) -> bool {
    let def = program.arena.function_def(definition);
    let contains = |owner: Span| owner.source_id == span.source_id && owner.start() <= span.start() && span.end() <= owner.end();
    contains(program.arena.span(program.arena.block(def.body).span))
        || program.arena.params(def.params).iter().filter_map(|parameter| parameter.default)
            .any(|expression| contains(program.arena.expr(expression).span))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::CheckOptions;
    use crate::sema::schemes::SchemeType;
    use crate::sema::types::Type;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;

    #[test]
    fn recursive_default_parameters_share_header_cells_before_peer_bodies() {
        for invalid in [false, true] {
            let source = format!(
                "pure left(value = 0, count = 0) {{ if count == 0 {{ value }} else {{ right({}, count - 1) }} }}\npure right(value = 0, count = 0) {{ if count == 0 {{ value }} else {{ left(value, count - 1) }} }}\nlet result: Int = left(42, 2)\n",
                if invalid { "\"wrong\"" } else { "value" },
            );
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            parsed.arena.symbol_owner().with_current(|| {
                let checked = Checker::check_arena_with_options(&parsed.arena, &source, CheckOptions::for_execution());
                if invalid {
                    assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckTypeMismatch)), "{:?}", checked.diagnostics);
                    assert!(checked.checked_declarations.functions.values().all(|declaration| declaration.scheme.is_none()));
                } else { assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics); }
            });
        }
    }

    #[test]
    fn recursive_proc_completions_join_success_cells_before_result_envelopes() {
        for source in [
            concat!(
                "proc descend(value, count) { if count == 0 { return value }; descend(value, count - 1) }\n",
                "let integer: Int = descend(42, 2)?\nlet text: Str = descend(\"demo\", 3)?\n",
            ),
            concat!(
                "proc left(action, value, count) { if count == 0 { return value }; right(action, value, count - 1) }\n",
                "proc right(action, value, count) { if count == 0 { return action(value) }; left(action, value, count - 1) }\n",
                "proc report(value: Str) [io] -> Str { print $value; return value }\n",
                "let result: Str = left(report, \"peer\", 1)?\n",
            ),
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            parsed.arena.symbol_owner().with_current(|| {
                let program = &parsed.arena;
                let view = super::super::CheckBundleView::for_root(program, program.statements, None, Some(SourceId::new(0)));
                let statements = program.statement_ids().collect::<Vec<_>>();
                let mut summaries = BTreeMap::new();
                let mut origins = super::super::stream_origins::StreamOriginSummaries::new();
                for _ in 0..3 {
                let mut checker = Checker::new(CheckOptions::for_execution());
                checker.resolved_bindings = Arc::new(super::super::ResolvedBindings::collect(program));
                checker.effect_graph = super::super::infer_effects::EffectGraph::declarations_for_root(&view);
                checker.authoritative_effect_graph = true;
                checker.record_effects = true;
                checker.collecting_effects = true;
                checker.defer_effect_diagnostics = true;
                checker.effect_summaries = summaries;
                checker.stream_origin_inputs = origins;
                checker.collect_definitions_arena(program, Arc::new(program.clone()), source, statements.iter().copied());
                checker.infer_local_pure_returns(&view, source, &statements);
                for &statement in &statements { checker.check_stmt_arena(program, source, statement); }
                assert!(checker.diagnostics.is_empty(), "{:?}", checker.diagnostics);
                checker.bind_effect_graph().unwrap();
                summaries = checker.effect_graph.solve_with_external(&BTreeMap::new()).unwrap();
                checker.normalize_stream_origin_applications().unwrap();
                checker.settle_stream_origin_dependencies(&super::super::stream_origins::StreamOriginSummaries::new());
                origins = checker.stream_origin_outputs;
                }
                let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                for declaration in checked.checked_declarations.functions.values() {
                    assert!(declaration.scheme.is_some());
                }
            });
        }
    }

    #[test]
    fn recursive_schemes_publish_independent_calls_and_actual_peer_substitutions() {
        let source = concat!(
            "pure left(value, count) { if count == 0 { value } else { right(value, count - 1) } }\n",
            "pure right(value, count) { if count == 0 { value } else { left(value, count - 1) } }\n",
            "let integer = left(42, 2)\nlet text = right(\"demo\", 3)\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            assert_eq!(checked.checked_declarations.functions.len(), 2);
            assert_eq!(checked.function_body_check_entries, 2);
            for declaration in checked.checked_declarations.functions.values() {
                let scheme = declaration.scheme.as_ref().unwrap();
                assert_eq!(scheme.type_parameters().len(), 1);
                assert_eq!(scheme.params()[0].ty, scheme.return_type().clone());
                assert_eq!(scheme.params()[1].ty, SchemeType::Ground(Type::Int));
            }
            assert_eq!(checked.call_instantiations.len(), 4);
            assert_eq!(checked.call_instantiations.values().filter(|call| call.type_arguments().values().any(SchemeType::contains_parameter)).count(), 2);
        });
    }

    #[test]
    fn exported_recursive_schemes_are_independent_of_client_root_order() {
        use crate::sema::check::module_driver::CheckSession;
        use crate::source::SourceMap;
        use crate::syntax::arena::ArenaProgramBuilder;
        for callbacks in [false, true] {
        for reverse in [false, true] {
            let mut sources = SourceMap::new();
            let roots = if callbacks { [
                "use shared\npure first(value: Int) -> Int { value + 1 }\npure second(payload: Int) -> Int { payload + 2 }\nlet integer: Int = shared.left(first, second, 42, 2)\n",
                "use shared\npure first(value: Str) -> Str { value + \"!\" }\npure second(payload: Str) -> Str { payload + \"?\" }\nlet text: Str = shared.right(first, second, \"demo\", 3)\n",
            ] } else { [
                "use shared\nlet integer: Int = shared.left(42, 2)\n",
                "use shared\nlet text: Str = shared.right(\"demo\", 3)\n",
            ] };
            let module_text = if callbacks { concat!(
                "##! Recursive callback helpers.\n",
                "## Returns through either callback and its recursive peer.\n",
                "export pure left(action, other, value, count) { if count == 0 { action(value) } else { right(action, other, value, count - 1) } }\n",
                "## Returns through either callback and its recursive peer.\n",
                "export pure right(action, other, value, count) { if count == 0 { other(payload: value) } else { left(action, other, value, count - 1) } }\n",
            ) } else { concat!(
                "##! Recursive helpers.\n",
                "## Returns a value through its recursive peer.\n",
                "export pure left(value, count) { if count == 0 { value } else { right(value, count - 1) } }\n",
                "## Returns a value through its recursive peer.\n",
                "export pure right(value, count) { if count == 0 { value } else { left(value, count - 1) } }\n",
            ) };
            let mut builder = ArenaProgramBuilder::with_token_capacity(256);
            let mut entries = Vec::new();
            for (index, source) in roots.iter().enumerate() {
                let source_id = sources.add_file(format!("entry{index}.xsh"), *source);
                let parsed = Parser::parse_source_into_arena_builder(source_id, source, &mut builder);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                entries.push((parsed.statements, source_id));
            }
            let module_id = sources.add_file("shared.xsh", module_text);
            let module = Parser::parse_source_into_arena_builder(module_id, module_text, &mut builder);
            assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
            let module_name = builder.symbol_owner().intern("shared");
            builder.push_arena_module("shared".to_string(), module_name, module.statements);
            for &(statements, _) in &entries {
                for statement in builder.statement_ids(statements) {
                    if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
                        builder.set_use_resolved(use_id, Arc::from("shared"));
                    }
                }
            }
            let program = Arc::new(builder.finish_with_statements(entries[0].0));
            let session = CheckSession::new(program, Arc::new(sources));
            let mut published: Option<Vec<Arc<crate::sema::schemes::TypeScheme>>> = None;
            for index in if reverse { [1, 0] } else { [0, 1] } {
                let (statements, source) = entries[index];
                let checked = session.check(statements, source, CheckOptions::default());
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                let schemes = checked.checked_declarations.functions.values().filter(|declaration| declaration.namespace == Some(module_name))
                    .map(|declaration| Arc::clone(declaration.scheme.as_ref().expect("an exported recursive helper retains its scheme"))).collect::<Vec<_>>();
                assert_eq!(schemes.len(), 2);
                for scheme in &schemes {
                    if callbacks {
                        assert_eq!(scheme.callable().requirements().applications.len(), 2);
                    } else {
                        assert_eq!(scheme.type_parameters().len(), 1);
                        assert_eq!(scheme.params()[0].ty, scheme.return_type().clone());
                    }
                }
                if let Some(previous) = &published {
                    for scheme in &schemes {
                        assert!(previous.iter().any(|previous| Arc::ptr_eq(previous, scheme)));
                    }
                } else { published = Some(schemes); }
            }
        }
        }
    }

    #[test]
    fn deferred_recursive_effect_failure_publishes_no_peer_scheme_or_substitution() {
        for forbidden in [false, true] {
            let fixture = concat!(
                "proc read_environment() [env] -> Int { let _ = env.get(\"XSH_SCC_TEST\"); 0 }\n",
                "proc left(value, count: Int) [] -> Int { if count == 0 { 0 } else { right(value, count - 1) } }\n",
                "proc right(value, count: Int) -> Int { if count == 0 { read_environment() } else { left(value, count - 1) } }\n",
            );
            let source_text = fixture.replace("[]", if forbidden { "[]" } else { "[env]" });
            let source = source_text.as_str();
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let program = Arc::new(parsed.arena);
            program.symbol_owner().with_current(|| {
                let output = Checker::check_arena_with_options(&program, source, CheckOptions::default());
                if forbidden {
                    assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckEffectViolation)), "{:?}", output.diagnostics);
                } else { assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics); }
                let peers = output.checked_declarations.functions.values().filter(|declaration| matches!(declaration.name.as_str().as_ref(), "left" | "right")).collect::<Vec<_>>();
                assert_eq!(peers.len(), 2);
                if forbidden {
                    assert!(peers.iter().all(|declaration| declaration.scheme.is_none() && declaration.scheme_body_facts.is_none()));
                    assert!(output.call_instantiations.is_empty());
                } else {
                    assert!(peers.iter().all(|declaration| declaration.scheme.is_some() && declaration.scheme_body_facts.is_some()));
                    assert_eq!(output.call_instantiations.len(), 2);
                }
            });
        }
    }

    #[test]
    fn recursive_component_failure_publishes_no_member_scheme() {
        let source = concat!(
            "pure left(value, count) { if count == 0 { value } else { right(value, count - 1) } }\n",
            "pure right(value, count) { if count == 0 { \"wrong\" + 1 } else { left(value, count - 1) } }\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::default());
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error));
        assert!(checked.checked_declarations.functions.values().all(|declaration| declaration.scheme.is_none() && declaration.scheme_body_facts.is_none()));
        assert!(checked.call_instantiations.is_empty());
    }
}
