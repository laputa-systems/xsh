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
        if self.constrain_pending_component_invocations(&members).is_none() {
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
        let mut recursive_calls = BTreeMap::<crate::sema::schemes::InstantiationSite<Span>, super::PendingInstantiation>::new();
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
            recursive_calls.insert(crate::sema::schemes::InstantiationSite::call(span), crate::sema::value_materialization::InstantiationEvidence::Invocation((Arc::clone(&callee.callable), instance.clone())));
        }
        for ((owner, span), substitution) in substitutions {
            if self.call_instantiations.contains_key(&crate::sema::schemes::InstantiationSite::call(span))
                || prepared.get_mut(&owner).expect("the recursive caller was prepared").calls.insert(crate::sema::schemes::InstantiationSite::call(span), crate::sema::value_materialization::InstantiationEvidence::Invocation(substitution)).is_some()
            {
                self.error(span, "recursive call already has a published substitution", DiagnosticCode::CheckLocalInference);
                self.reject_recursive_component(program, definitions);
                return None;
            }
        }
        let latent_stream_errors = match self.prepare_recursive_stream_error_flows(&members, &recursive_calls) {
            Ok(latent) => latent,
            Err((span, error)) => {
                self.error(span, &format!("recursive stream invocation has no checked error flow: {error:?}"), DiagnosticCode::CheckLocalInference);
                self.reject_recursive_component(program, definitions);
                return None;
            }
        };
        if let Err((owner, _)) = self.resolve_stream_origin_applications(&members, &recursive_calls) {
            self.report_stream_origin_boundary(owner.body);
            self.reject_recursive_component(program, definitions);
            return None;
        }
        self.latent_stream_error_spans.extend(latent_stream_errors);
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
        let rejected_owners = definitions.iter().map(|&definition| self.effect_declaration_id(program, program.arena.function_def(definition).body)).collect::<BTreeSet<_>>();
        self.stream_origin_outputs.reject_declarations(&rejected_owners);
        self.latent_stream_error_spans.retain(|&span| !definitions.iter().any(|&definition| function_contains_span(program, definition, span)));
        for &definition in definitions {
            let def = program.arena.function_def(definition);
            let declaration = FunctionDeclarationId::new(self.effect_declaration_id(program, def.body));
            if let Some(pending) = self.declaration_inference.pending_mut(declaration) {
                pending.reject_generalization();
            }
        }
        let rejected_calls = self.selected_function_calls.iter().filter_map(|(&span, target)| {
            definitions.contains(&target.definition).then_some(span)
        }).chain(self.call_instantiations.keys().map(|site| site.source).filter(|&span| {
            definitions.iter().any(|&definition| function_contains_span(program, definition, span))
        })).collect::<BTreeSet<_>>();
        self.latent_stream_error_spans.retain(|span| !rejected_calls.contains(span));
        self.call_instantiations.retain(|site, _| !rejected_calls.contains(&site.source));
        self.pending_call_instantiations.retain(|site, _| !rejected_calls.contains(&site.source));
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
    fn proc_stream_callback_relay_keeps_result_envelopes_at_independent_types() {
        let source = concat!(
            "stream values(value) [] { yield value }\n",
            "proc relay(action, value) [] { action(value) }\n",
            "let create = values\n",
            "let integers: Result[Stream[Int]] = relay(create, 7)\n",
            "let words: Result[Stream[Str]] = relay(create, \"row\")\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        for expected in [Type::Int, Type::Str] {
            assert!(checked.expr_types.values().any(|ty| ty == &Type::Result(Box::new(Type::Stream(Box::new(expected.clone()))), Box::new(Type::Error))));
        }
        let scheme = checked.checked_declarations.functions.values().find(|declaration| declaration.name == "relay").unwrap().scheme.as_ref().unwrap();
        let applications = &scheme.callable().requirements().applications;
        assert_eq!(applications.len(), 1);
        assert_eq!(applications[0].invocation().unwrap().proc_returns.len(), 1);
        assert_eq!(scheme.return_type(), &applications[0].invocation().unwrap().proc_returns[0].result);
    }

    #[test]
    fn inferred_proc_cleanup_errors_join_the_actual_application_envelope() {
        let source = concat!(
            "error CallbackFailure = Missing(message: Str)\n",
            "proc close() [error] { assert false }\n",
            "pure plain(value: Int) -> Int { value }\n",
            "pure nominal(value: Int) -> Result[Int, CallbackFailure] { Ok(value) }\n",
            "proc relay(action, value) { defer close(); action(value) }\n",
            "let scalar: Result[Int] = relay(plain, 7)\n",
            "let result: Result[Int] = relay(nominal, 8)\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let scheme = checked.checked_declarations.functions.values().find(|declaration| declaration.name == "relay").unwrap().scheme.as_ref().unwrap();
        let applications = &scheme.callable().requirements().applications;
        assert_eq!(applications.len(), 1);
        assert_eq!(applications[0].invocation().unwrap().proc_returns.len(), 1);
        let envelope = &applications[0].invocation().unwrap().proc_returns[0];
        assert_eq!(scheme.return_type(), &envelope.result);
        assert_eq!(envelope.propagated_errors.as_ref(), &[crate::sema::schemes::SchemeType::Ground(Type::Error)]);
    }

    #[test]
    fn proc_stream_callback_relay_result_can_be_propagated_at_independent_types() {
        let source = concat!(
            "stream values(value) [] { yield value }\n",
            "proc relay(action, value) [] { action(value) }\n",
            "proc exercise() [error] { let create = values; let integers = relay(create, 7)?; let words = relay(create, \"row\")? }\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    }

    #[test]
    fn proc_stream_callback_relay_result_can_be_captured_at_independent_types() {
        let source = concat!(
            "stream values(value) [] { yield value }\n",
            "proc relay(action, value) [] { action(value) }\n",
            "proc exercise() [] { let create = values; let integers: Result[Stream[Int]] = try { relay(create, 7)? }; let words: Result[Stream[Str]] = try { relay(create, \"row\")? } }\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    }

    #[test]
    fn proc_callback_envelope_propagation_accepts_pure_and_proc_results() {
        let source = concat!(
            "pure number(value: Int) -> Int { value }\n",
            "proc outcome(value: Int) [] -> Result[Int] { Ok(value) }\n",
            "proc relay(action, value) [] { action(value) }\n",
            "proc exercise() [error] { let scalar: Int = relay(number, 7)?; let result: Int = relay(outcome, 8)? }\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    }

    #[test]
    fn propagation_still_rejects_actual_non_result_values() {
        for source in [
            "proc exercise() [error] { let invalid = 7? }\n",
            "stream values(value) [] { yield value }\nproc exercise() [error] { let invalid = values(7)? }\n",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckTryResult)), "{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn inferred_propagation_inputs_retain_result_and_effect_contracts() {
        let cases = [
            (
                "proc unwrap(value) [] { value? }\n",
                DiagnosticCode::CheckEffectViolation,
            ),
            (
                "proc unwrap(value) { value? }\nlet invalid: Result[Int, Str] = unwrap(7)\n",
                DiagnosticCode::CheckTypeMismatch,
            ),
        ];
        for (source, expected) in cases {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(
                &parsed.arena,
                source,
                CheckOptions::for_execution(),
            );
            assert!(
                checked
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == Some(expected)),
                "{source}\n{:?}",
                checked.diagnostics,
            );
        }
    }

    fn assert_authored_result_branch_preserves_the_original_context(completion: &str) {
        let source = format!("proc read_chunk(source: Path, offset: Int, count = 4) [fs, error, io] -> Result[Bytes, Error] {{ return {completion} }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, &source, CheckOptions::for_execution());
        assert!(checked.diagnostics.is_empty(), "{completion}: {:?}", checked.diagnostics);
    }

    #[test]
    fn authored_proc_return_if_preserves_its_whole_result_context() {
        assert_authored_result_branch_preserves_the_original_context("if offset == 0 { source.read_bytes() } else { Ok(b\"\") }");
    }

    #[test]
    fn authored_proc_return_match_preserves_its_whole_result_context() {
        assert_authored_result_branch_preserves_the_original_context("match offset { 0 => { source.read_bytes() }; _ => { Ok(b\"\") } }");
    }

    #[test]
    fn list_literal_rows_resolve_nested_empty_items_before_field_use() {
        for rows in ["[{args: []}, {args: [\"-z\"]}]", "[{args: [\"-z\"]}, {args: []}]", "[{args: [], labels: [\"a\"]}, {args: [\"-z\"], labels: []}]"] {
            let source = format!("pure sizes() -> List[Int] {{ collect {{ for case in {rows} {{ yield case.args.len() }} }} }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, &source, CheckOptions::for_execution());
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let receiver = crate::source::Span::new(SourceId::new(0), source.find("case.args").unwrap(), source.find("case.args").unwrap() + 4);
            let Type::Record(fields) = checked.expr_types.get(&receiver).expect("checked loop receiver") else { panic!("checked record receiver") };
            assert_eq!(fields.get(&crate::symbol::Name::intern("args")), Some(&Type::List(Box::new(Type::Str))), "{rows}");
        }
    }

    #[test]
    fn list_literal_row_refinement_preserves_authored_any_and_rejects_incompatible_fields() {
        let source = "pure sizes(items: List[Any]) -> List[Int] { collect { for case in [{args: items}, {args: []}] { yield case.args.len() } } }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let start = source.find("case.args").unwrap();
        let Type::Record(fields) = &checked.expr_types[&crate::source::Span::new(SourceId::new(0), start, start + 4)] else { panic!("checked record receiver") };
        assert_eq!(fields.get(&crate::symbol::Name::intern("args")), Some(&Type::List(Box::new(Type::Any))));
        for rows in ["[{args: [7]}, {args: [\"word\"]}]", "[{args: [], count: 7}, {args: [\"word\"]}]"] {
            let field = if rows.contains("count") { "count" } else { "args" };
            let source = format!("pure sizes() -> Int {{ for case in {rows} {{ let _ = case.{field} }}; 7 }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, &source, CheckOptions::for_execution());
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error), "{rows}: incompatible row accepted");
        }
    }


    #[test]
    fn inferred_stream_local_aliases_keep_actual_kind_and_independent_items() {
        for alias in [false, true] {
            let source = format!("stream values(value) {{ yield value }}\n{}let integer: Stream[Int] = {}(7)\nlet text: Stream[Str] = {}(\"word\")\n", if alias { "let create = values\n" } else { "" }, if alias { "create" } else { "values" }, if alias { "create" } else { "values" });
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            parsed.arena.symbol_owner().with_current(|| {
                let checked = Checker::check_arena_with_options(&parsed.arena, &source, CheckOptions::for_execution());
                assert!(checked.diagnostics.is_empty(), "alias={alias}: {:?}", checked.diagnostics);
                assert_eq!(checked.checked_declarations.functions.len(), 1);
                let scheme = checked.checked_declarations.functions.values().next().unwrap().scheme.as_ref().unwrap();
                assert_eq!(scheme.kind(), crate::sema::types::CallableKind::Stream);
                assert_eq!(scheme.type_parameters().len(), 1);
                assert_eq!(checked.call_instantiations.len(), 2);
                assert!(checked.call_instantiations.values().flat_map(|evidence| evidence.instances()).any(|call| call.type_arguments().values().any(|ty| ty == &SchemeType::Ground(Type::Int))));
                assert!(checked.call_instantiations.values().flat_map(|evidence| evidence.instances()).any(|call| call.type_arguments().values().any(|ty| ty == &SchemeType::Ground(Type::Str))));
            });
        }
    }

    #[test]
    fn inferred_stream_imported_aliases_keep_the_module_scheme_across_roots() {
        use crate::sema::check::module_driver::CheckSession;
        use crate::source::SourceMap;
        use crate::syntax::arena::ArenaProgramBuilder;
        for reverse in [false, true] {
            let mut sources = SourceMap::new();
            let roots = [
                "use shared\nlet create = shared.replay\nlet integer: Stream[Int] = create(7)\n",
                "use shared\nlet create = shared.replay\nlet text: Stream[Str] = create(\"word\")\n",
            ];
            let module_text = "##! Generic stream relay.\n## Yields its input without evaluating at creation.\nexport stream replay(value) { yield value }\n";
            let mut builder = ArenaProgramBuilder::with_token_capacity(256);
            let mut entries = Vec::new();
            for (index, source) in roots.iter().enumerate() {
                let source_id = sources.add_file(format!("alias-entry{index}.xsh"), *source);
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
                    if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) { builder.set_use_resolved(use_id, Arc::from("shared")); }
                }
            }
            let program = Arc::new(builder.finish_with_statements(entries[0].0));
            let session = CheckSession::new(program, Arc::new(sources));
            let mut published: Option<(FunctionDefId, Arc<crate::sema::schemes::TypeScheme>)> = None;
            for index in if reverse { [1, 0] } else { [0, 1] } {
                let (statements, source) = entries[index];
                let checked = session.check(statements, source, CheckOptions::default());
                assert!(checked.diagnostics.is_empty(), "root={index}: {:?}", checked.diagnostics);
                let (&definition, declaration) = checked.checked_declarations.functions.iter().find(|(_, declaration)| declaration.namespace == Some(module_name)).unwrap();
                let scheme = declaration.scheme.as_ref().unwrap();
                assert_eq!(scheme.kind(), crate::sema::types::CallableKind::Stream);
                assert_eq!(scheme.type_parameters().len(), 1);
                assert_eq!(checked.call_instantiations.len(), 1);
                let arguments = checked.call_instantiations.values().flat_map(|evidence| evidence.instances())
                    .flat_map(|call| call.type_arguments().values()).cloned().collect::<Vec<_>>();
                assert_eq!(arguments, vec![SchemeType::Ground(if index == 0 { Type::Int } else { Type::Str })]);
                if let Some((previous_definition, previous)) = &published {
                    assert_eq!(*previous_definition, definition);
                    assert_eq!(previous.as_ref(), scheme.as_ref());
                } else { published = Some((definition, Arc::clone(scheme))); }
            }
        }
    }
    #[test]
    fn stream_module_exports_preserve_ground_callable_contracts() {
        use crate::sema::check::module_driver::CheckSession;
        use crate::source::SourceMap;
        use crate::syntax::arena::ArenaProgramBuilder;
        let source = "use shared\nlet create = shared.fixed\nlet output: Stream[Int] = create(7)\n";
        let module_text = "##! Ground stream export.\n## Yields the requested values.\nexport stream fixed(value: Int, count: Int = 1) [io] -> Stream[Int] { repeat count times { yield value } }\n";
        let mut sources = SourceMap::new();
        let mut builder = ArenaProgramBuilder::with_token_capacity(256);
        let source_id = sources.add_file("ground-stream-entry.xsh", source);
        let entry = Parser::parse_source_into_arena_builder(source_id, source, &mut builder);
        assert!(entry.diagnostics.is_empty(), "{:?}", entry.diagnostics);
        let module_id = sources.add_file("shared.xsh", module_text);
        let module = Parser::parse_source_into_arena_builder(module_id, module_text, &mut builder);
        assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
        let module_name = builder.symbol_owner().intern("shared");
        let export_name = builder.symbol_owner().intern("fixed");
        builder.push_arena_module("shared".to_string(), module_name, module.statements);
        for statement in builder.statement_ids(entry.statements) {
            if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) { builder.set_use_resolved(use_id, Arc::from("shared")); }
        }
        let program = Arc::new(builder.finish_with_statements(entry.statements));
        let session = CheckSession::new(program, Arc::new(sources));
        let checked = session.check(entry.statements, source_id, CheckOptions::default());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let callable = checked.expr_types.values().find_map(|ty| {
            let Type::Module(module) = ty else { return None };
            let crate::sema::types::ModuleExportType::Callable { kind, sig, optional: false } = module.exports.get(&export_name)? else { return None };
            Some(Arc::new(crate::sema::types::TypedCallable { kind: *kind, sig: sig.clone() }))
        }).expect("ground stream export retains its actual callable type");
        assert_eq!(callable.kind, crate::sema::types::CallableKind::Stream);
        assert_eq!(callable.sig.params.len(), 2);
        assert_eq!(callable.sig.params[0].ty, Type::Int);
        assert!(!callable.sig.params[0].defaulted);
        assert_eq!(callable.sig.params[1].ty, Type::Int);
        assert!(callable.sig.params[1].defaulted);
        assert_eq!(*callable.sig.return_ty, Type::Stream(Box::new(Type::Int)));
        assert_eq!(callable.sig.effects, crate::sema::types::CallableEffects::finite([crate::syntax::node::Effect::Io]));
        let actual = Type::Callable(Arc::clone(&callable));
        for mutation in 0..3 {
            let mut wrong = callable.as_ref().clone();
            match mutation {
                0 => wrong.kind = crate::sema::types::CallableKind::Proc,
                1 => wrong.sig.params[0].ty = Type::Str,
                2 => wrong.sig.return_ty = Box::new(Type::Stream(Box::new(Type::Str))),
                _ => unreachable!(),
            }
            assert!(!actual.matches_expected(&Type::Callable(Arc::new(wrong))));
        }
    }

    #[test]
    fn generic_command_words_require_available_checked_bindings() {
        let source = "let directory = p\".\"\nproc sample(payload) { fs.fsync directory ?; return payload }\nlet integer: Result[Int] = sample(7)\nlet text: Result[Str] = sample(\"word\")\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let program = &parsed.arena;
            let output = Checker::check_arena_with_options(program, source, CheckOptions::for_execution());
            assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
            let declaration = output.checked_declarations.functions.values().next().unwrap();
            assert!(declaration.scheme.as_ref().unwrap().callable().is_quantified());
            let facts = declaration.scheme_body_facts.as_ref().unwrap();
            let body = program.statement_ids().find_map(|statement| match program.arena.stmt(statement).kind {
                crate::syntax::arena::ArenaStmtKind::ProcDef(definition) => Some(program.arena.function_def(definition).body),
                _ => None,
            }).unwrap();
            let bindings = &output.resolved_bindings;
            let parameters = bindings.declarations.iter().filter_map(|(&id, declaration)| (declaration.name == "payload").then_some(id)).collect::<super::super::FxHashSet<_>>();
            let captures = bindings.declarations.iter().filter_map(|(&id, declaration)| (declaration.name == "directory").then_some((id, super::super::scheme_body::CapturedBindingAccess::ReadOnly))).collect::<super::super::scheme_body::CapturedBindings>();
            let commands = bindings.value_sites.iter().map(|(&site, &binding)| (site, binding)).collect::<BTreeMap<_, _>>();
            let calls = output.module_commands.keys().copied().collect::<super::super::FxHashSet<_>>();
            let accepts = |captures| super::super::scheme_body::supports_generalized_body(
                program, declaration.scheme.as_ref().unwrap().kind(), &output.prepared_constants, &commands, body, bindings, &parameters, captures,
                facts.tail_types(), &output.statement_positions, &calls, facts.registered_calls(), facts.expression_types(),
                facts.constructor_producers(), &output.checked_control_flow.result_constructors, &output.collect_yields, super::super::scheme_body::BodyCompletion::Function,
            );
            assert!(accepts(&captures));
            assert!(!accepts(&super::super::scheme_body::CapturedBindings::default()));
        });
    }

    #[test]
    fn partial_proc_tail_if_checks_result_unit_calls_as_statements() {
        for annotation in ["", " -> Result[Unit, Error]"] {
            for completion in [
                "if flag { require(flag) }",
                "if flag { require(flag) } else if flag == false { if flag { return Err(Failure.Lost(\"nested\")) } }",
                "if flag { assert flag }",
                "if flag { require(flag) } else { require(flag) }",
            ] {
                let source = format!("error Failure = Lost(message: Str)\nproc require(flag: Bool) [error] {{ assert flag }}\nproc validate(flag: Bool){annotation} {{ if flag {{ return Err(Failure.Lost(\"lost\")) }}; {completion} }}\nlet outcome: Result[Unit, Error] = validate(false)\n");
                let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
                assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
                parsed.arena.symbol_owner().with_current(|| {
                    let (work, checked) = crate::sema::inference_work::collect(|| Checker::check_arena_with_options(&parsed.arena, &source, CheckOptions::for_execution()));
                    assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
                    assert_eq!(work.function_body_check_entries, 2, "{source}");
                    let declaration = checked.checked_declarations.functions.values().find(|declaration| declaration.name == "validate").unwrap();
                    let expected = Type::Result(Box::new(Type::Unit), Box::new(Type::Error));
                    assert_eq!(checked.function_return_types[&declaration.body], expected, "{source}");
                    assert_eq!(declaration.scheme.as_ref().unwrap().return_type().closed_ground_type(), Some(expected), "{source}");
                });
            }
        }
        let source = "proc validate(flag: Bool) -> Unit { if flag { return }; if flag { let _ = true } }\nlet outcome: Unit = validate(false)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
            let declaration = checked.checked_declarations.functions.values().next().unwrap();
            assert_eq!(checked.function_return_types[&declaration.body], Type::Unit);
        });
    }

    #[test]
    fn partial_proc_tail_if_preserves_value_and_return_rejections() {
        for (source, code) in [
            ("pure invalid(flag: Bool) { if flag { 7 } }", DiagnosticCode::CheckIfValueElse),
            ("proc invalid(flag: Bool) { if flag { return 7 }; if flag { let _ = true } }", DiagnosticCode::CheckTypeMismatch),
            ("error Failure = Lost(message: Str)\nproc invalid(flag: Bool) { if flag { return Err(Failure.Lost(\"lost\")) }; if flag { 7 } }", DiagnosticCode::CheckIgnoredResult),
            ("error Failure = Lost(message: Str)\nproc invalid(flag: Bool) -> Unit { if flag { return Err(Failure.Lost(\"lost\")) }; if flag { let _ = true } }", DiagnosticCode::CheckTypeMismatch),
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            parsed.arena.symbol_owner().with_current(|| {
                let checked = Checker::check_arena(&parsed.arena, source);
                assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(code)), "{source}\n{:?}", checked.diagnostics);
            });
        }
    }

    #[test]
    fn inferred_unit_proc_completions_match_the_result_unit_contract() {
        for (source, expected, expected_return) in [
            ("error Failure = Lost(message: Str)\nproc only_failure() { Err(Failure.Lost(message: \"lost\")) }\n", None, Some("Lost")),
            ("error Failure = Lost(message: Str)\nproc only_failure() { return Err(Failure.Lost(message: \"lost\")) }\n", None, Some("Lost")),
            ("error Failure = Lost(message: Str)\nproc main() { Err(Failure.Lost(message: \"lost\")) }\n", None, Some("Lost")),
            ("error Failure = Lost(message: Str) | Missing(path: Path)\nproc only_failure(flag: Bool) { if flag { Err(Failure.Lost(message: \"lost\")) } else { Err(Failure.Missing(p\"missing\")) } }\n", None, Some("Family")),
            ("error Failure = Lost(message: Str)\nproc mixed(flag: Bool) { if flag { Err(Failure.Lost(message: \"lost\")) } else { let _ = true } }\n", None, Some("Error")),
            ("proc invalid() { Err(\"lost\") }\n", Some(DiagnosticCode::CheckTypeMismatch), None),
            ("proc unknown(action: Pure) { action.call() }\n", Some(DiagnosticCode::CheckInferReturn), None),
            ("proc invalid(flag: Bool, value: Result[Int, Str]) { if flag { let ignored = value? } else { let _ = true } }\n", None, Some("Str")),
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{source}: {:?}", parsed.diagnostics);
            parsed.arena.symbol_owner().with_current(|| {
                let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
                match expected {
                    Some(code) => assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(code)), "{source}: {:?}", checked.diagnostics),
                    None => {
                        assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
                        let declaration = checked.checked_declarations.functions.values().next().unwrap();
                        let family = crate::symbol::Name::intern("Failure");
                        let expected_return = match expected_return.unwrap() {
                            "Lost" => Type::ErrorVariant { family, variant: crate::symbol::Name::intern("Lost") },
                            "Family" => Type::ErrorFamily(family),
                            "Str" => Type::Str,
                            "Error" => Type::Error,
                            _ => unreachable!(),
                        };
                        let expected_return = Type::Result(Box::new(Type::Unit), Box::new(expected_return));
                        assert_eq!(checked.function_return_types[&declaration.body], expected_return, "{source}");
                        assert_eq!(declaration.scheme.as_ref().unwrap().return_type().closed_ground_type(), Some(expected_return), "{source}");
                    }
                }
            });
        }
    }

    #[test]
    fn generic_effectful_bodies_keep_checked_command_classes() {
        for command in [
            "print \"checked\"", "eprint \"checked\"",
            "env XSH_GENERIC_COMMAND=inside { print \"checked\" }",
            "cd p\".\" { print \"checked\" }",
            "fs.fsync p\".\" ?", "run true",
        ] {
            let source = format!("proc sample(payload) {{ {command}; return payload }}\nlet integer: Result[Int] = sample(7)\nlet text: Result[Str] = sample(\"word\")\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{command}: {:?}", parsed.diagnostics);
            parsed.arena.symbol_owner().with_current(|| {
                let (work, checked) = crate::sema::inference_work::collect(|| {
                    Checker::check_arena_with_options(&parsed.arena, &source, CheckOptions::for_execution())
                });
                assert!(checked.diagnostics.is_empty(), "{command}: {:?}", checked.diagnostics);
                let declaration = checked.checked_declarations.functions.values().next().unwrap();
                assert!(declaration.scheme.as_ref().is_some_and(|scheme| scheme.callable().is_quantified()), "{command}");
                assert_eq!(checked.call_instantiations.len(), 2, "{command}");
                assert_eq!(work.function_body_check_entries, checked.function_body_check_entries as u64, "{command}");
            });
        }
    }

    #[test]
    fn acyclic_inferred_proc_collects_its_checked_body_once() {
        let source = "proc sample(payload: Int) { print \"checked\"; return payload }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let (work, checked) = crate::sema::inference_work::collect(|| {
                Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution())
            });
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            assert_eq!(checked.function_body_check_entries, 1);
            assert_eq!(work.function_body_check_entries, 1);
            let declaration = checked.checked_declarations.functions.values().next().unwrap();
            assert_eq!(checked.function_return_types[&declaration.body], Type::Result(Box::new(Type::Int), Box::new(Type::Error)));
        });
    }

    #[test]
    fn omitted_stream_interfaces_share_original_cells_and_publish_once() {
        for recursive in [false, true] {
            let source = if recursive {
                concat!(
                    "stream left(value, count = 1) { if count > 0 { yield value; yield @right(value, count - 1) } }\n",
                    "stream right(value, count = 1) { if count > 0 { yield value; yield @left(value, count - 1) } }\n",
                    "let integers: Stream[Int] = left(7, 2)\nlet words: Stream[Str] = right(\"word\", 2)\n",
                )
            } else {
                "stream values(value, count = 1) { if count > 0 { yield value } }\nlet integers: Stream[Int] = values(7)\nlet words: Stream[Str] = values(\"word\")\n"
            };
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            parsed.arena.symbol_owner().with_current(|| {
                let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                let count = if recursive { 2 } else { 1 };
                assert_eq!(checked.checked_declarations.functions.len(), count);
                assert_eq!(checked.function_body_check_entries, count);
                for declaration in checked.checked_declarations.functions.values() {
                    let scheme = declaration.scheme.as_ref().expect("producer scheme");
                    assert_eq!(scheme.kind(), crate::sema::types::CallableKind::Stream);
                    assert_eq!(scheme.type_parameters().len(), 1);
                    assert_eq!(scheme.return_type(), &SchemeType::Stream(Box::new(scheme.params()[0].ty.clone())));
                    assert_eq!(scheme.params()[1].ty, SchemeType::Ground(Type::Int));
                }
            });
        }
    }

    #[test]
    fn omitted_stream_recursive_callback_effects_keep_actual_peer_instances() {
        let source = concat!(
            "pure integer(value: Int) -> Int { value }\n",
            "pure text(value: Str) -> Str { value }\n",
            "stream left(action, value, count = 1) { if count > 0 { yield action(value); yield @right(action, value, count - 1) } }\n",
            "stream right(action, value, count = 1) { if count > 0 { yield action(value); yield @left(action, value, count - 1) } }\n",
            "let integers: Stream[Int] = left(integer, 7, 2)\nlet words: Stream[Str] = right(text, \"word\", 2)\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let program = &parsed.arena;
            let view = super::super::CheckBundleView::for_root(program, program.statements, None, Some(SourceId::new(0)));
            let statements = program.statement_ids().collect::<Vec<_>>();
            let phase = |collecting, summaries, origins, graph| {
                let mut checker = Checker::new(CheckOptions::for_execution());
                checker.resolved_bindings = Arc::new(super::super::ResolvedBindings::collect(program));
                checker.effect_graph = graph;
                checker.authoritative_effect_graph = true;
                checker.collecting_effects = collecting;
                checker.record_effects = collecting;
                checker.defer_effect_diagnostics = true;
                checker.effect_summaries = summaries;
                checker.stream_origin_inputs = origins;
                checker.collect_definitions_arena(program, Arc::new(program.clone()), source, statements.iter().copied());
                checker.infer_default_parameter_types(program, source, &statements);
                checker.infer_local_pure_returns(&view, source, &statements);
                for &statement in &statements { checker.check_stmt_arena(program, source, statement); }
                checker
            };
            let mut summaries = BTreeMap::new();
            let mut origins = super::super::stream_origins::StreamOriginSummaries::new();
            let mut graph = None;
            for _ in 0..3 {
                let mut probe = phase(true, summaries, origins, super::super::infer_effects::EffectGraph::declarations_for_root(&view));
                assert!(probe.diagnostics.is_empty(), "{:?}", probe.diagnostics);
                probe.bind_effect_graph().unwrap();
                summaries = probe.effect_graph.solve_with_external(&BTreeMap::new()).unwrap();
                probe.normalize_stream_origin_applications().unwrap();
                probe.settle_stream_origin_dependencies(&super::super::stream_origins::StreamOriginSummaries::new());
                origins = probe.stream_origin_outputs;
                graph = Some(probe.effect_graph);
            }
            let phase = phase(false, summaries, origins, graph.expect("effect probes retain their authoritative graph"));
            assert!(phase.diagnostics.is_empty(), "{:?}", phase.diagnostics);
            let peer_calls = phase.provisional_function_calls.iter().filter_map(|(&span, (caller, callee))| {
                (caller != &callee.effect_declaration()).then_some(span)
            }).collect::<Vec<_>>();
            assert_eq!(peer_calls.len(), 2);
            for span in peer_calls {
                assert!(phase.latent_stream_error_spans.contains(&span));
                let expression = view.expr_ids().into_iter().find(|&expression| program.arena.expr(expression).span == span).expect("actual selected recursive stream expression");
                let mut origin = phase.stream_expr_origin(program, expression);
                assert!(!origin.join(&super::super::stream_origins::StreamOrigins::error()));
            }
            let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let mut peer_calls = Vec::new();
            for declaration in checked.checked_declarations.functions.values().filter(|declaration| matches!(declaration.name.as_str().as_ref(), "left" | "right")) {
                let scheme = declaration.scheme.as_ref().expect("producer callback scheme");
                assert_eq!(scheme.callable().requirements().applications.len(), 2);
                assert!(scheme.effects().is_known());
                peer_calls.extend(checked.selected_function_calls.iter().filter_map(|(&span, target)| {
                    (matches!(target.name.as_str().as_ref(), "left" | "right")
                        && declaration.body.source_id == span.source_id
                        && declaration.body.start() <= span.start() && span.end() <= declaration.body.end()).then_some(span)
                }));
            }
            assert_eq!(peer_calls.len(), 2);
            assert!(peer_calls.iter().all(|span| checked.call_instantiations.contains_key(&crate::sema::schemes::InstantiationSite::call(*span))
                && checked.checked_control_flow.latent_stream_error_spans.contains(span)));
        });
    }

    #[test]
    fn omitted_stream_interfaces_reject_incompatible_yields_and_effect_bounds() {
        let source = "stream values(value) [] { print \"creation effect\"; yield value }\nlet integer: Stream[Int] = values(7)\nlet text: Stream[Str] = values(\"word\")\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.checked_declarations.functions.len(), 1);
        assert_eq!(checked.call_instantiations.len(), 2);
        assert!(checked.checked_declarations.functions.values().all(|declaration| declaration.scheme.as_ref().is_some_and(|scheme| scheme.callable().is_quantified() && scheme.effects().concrete().unwrap() == Some(Vec::new()))));
        for (source, code) in [
            ("stream values(flag: Bool) { if flag { yield 7 } else { yield \"word\" } }", DiagnosticCode::CheckTypeMismatch),
            ("stream values(value) [] { run true; yield value }", DiagnosticCode::CheckEffectViolation),
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(code)), "{:?}", checked.diagnostics);
            assert!(checked.checked_declarations.functions.values().all(|declaration| declaration.scheme.is_none()));
        }
    }

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
            assert_eq!(checked.call_instantiations.values().flat_map(|evidence| evidence.instances()).filter(|call| call.type_arguments().values().any(SchemeType::contains_parameter)).count(), 2);
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
            let callees = [builder.symbol_owner().intern("left"), builder.symbol_owner().intern("right")];
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
            let mut published: Option<BTreeMap<FunctionDefId, Arc<crate::sema::schemes::TypeScheme>>> = None;
            for index in if reverse { [1, 0] } else { [0, 1] } {
                let (statements, source) = entries[index];
                let checked = session.check(statements, source, CheckOptions::default());
                assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                let schemes = checked.checked_declarations.functions.iter().filter(|(_, declaration)| declaration.namespace == Some(module_name))
                    .map(|(&definition, declaration)| (definition, Arc::clone(declaration.scheme.as_ref().expect("an exported recursive helper retains its scheme")))).collect::<BTreeMap<_, _>>();
                assert_eq!(schemes.len(), 2);
                for scheme in schemes.values() {
                    if callbacks {
                        assert_eq!(scheme.callable().requirements().applications.len(), 2);
                    } else {
                        assert_eq!(scheme.type_parameters().len(), 1);
                        assert_eq!(scheme.params()[0].ty, scheme.return_type().clone());
                    }
                }
                let selected = checked.checked_declarations.functions.values().find(|declaration| declaration.namespace == Some(module_name) && declaration.name == callees[index]).unwrap();
                let SchemeType::Parameter(value_parameter) = &selected.scheme.as_ref().unwrap().params()[if callbacks { 2 } else { 0 }].ty else { panic!("the selected recursive helper retains its value parameter"); };
                let calls = checked.call_instantiations.iter().filter(|(site, _)| site.source.source_id == source && site.role == crate::sema::schemes::InstantiationRole::Call)
                    .flat_map(|(_, evidence)| evidence.instances()).collect::<Vec<_>>();
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].type_arguments().get(value_parameter), Some(&SchemeType::Ground(if index == 0 { Type::Int } else { Type::Str })));
                if let Some(previous) = &published { assert_eq!(previous, &schemes); }
                else { published = Some(schemes); }
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
    fn authored_plain_returns_do_not_gain_unchecked_failure_permission() {
        for header in ["pure answer() -> Int", "proc answer() [] -> Int", "proc answer() [fs] -> Int"] {
            let source = format!("pure helper() -> Result[Int] {{ Ok(7) }}\n{header} {{ helper()? }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{header}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, &source, CheckOptions::for_execution());
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckTryContext)), "{header}: {:?}", checked.diagnostics);
        }
    }

    #[test]
    fn authored_proc_plain_payload_keeps_inferred_failure_flow() {
        for effects in ["[error]", ""] {
            let source = format!("pure helper() -> Result[Int] {{ Ok(7) }}\nproc answer() {effects} -> Int {{ helper()? }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{effects}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, &source, CheckOptions::for_execution());
            assert!(checked.diagnostics.is_empty(), "{effects}: {:?}", checked.diagnostics);
            let answer = checked.checked_declarations.functions.values().find(|declaration| declaration.name == "answer").unwrap();
            assert_eq!(answer.scheme.as_ref().unwrap().return_type().closed_ground_type(), Some(Type::Int));
        }
    }

    #[test]
    fn authored_proc_filesystem_payloads_keep_inferred_failure_flow() {
        for effects in ["[fs, error]", ""] {
            let source = format!(
                "proc backup_path(target: Path) {effects} -> Path {{ for entry in fs.children(target.parent())? {{ let _ = entry.path }}; target }}\nproc inside(source: Path, target: Path) {effects} -> Bool {{ let info = fs.stat(source, follow_symlinks: true)?; let absolute = target.resolve()?; let _ = info; absolute == source }}\n"
            );
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{effects}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, &source, CheckOptions::for_execution());
            assert!(checked.diagnostics.is_empty(), "{effects}: {:?}", checked.diagnostics);
            assert!(checked.checked_declarations.functions.values().all(|declaration| declaration.scheme.is_some()));
        }
    }

    fn assert_recursive_proc_return_expression(completion: &str) {
        let source = format!(
            "proc left(action, value, count: Int) {{ return {completion} }}\nproc right(action, value, count: Int) {{ left(action, value, count) }}\npure number(payload: Int) -> Int {{ payload }}\npure text(input: Str) -> Str {{ input }}\nlet integer: Result[Int] = left(number, 7, 2)\nlet string: Result[Str] = right(text, \"value\", 2)\n"
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{completion}: {:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, &source, CheckOptions::for_execution());
        assert!(checked.diagnostics.is_empty(), "{completion}: {:?}", checked.diagnostics);
        for declaration in checked.checked_declarations.functions.values().filter(|declaration| declaration.name == "left" || declaration.name == "right") {
            let scheme = declaration.scheme.as_ref().unwrap();
            assert_eq!(scheme.kind(), crate::sema::types::CallableKind::Proc);
            assert!(scheme.callable().is_quantified());
        }
    }

    #[test]
    fn recursive_proc_return_if_keeps_callback_envelopes() {
        assert_recursive_proc_return_expression("if count == 0 { action(value) } else { right(action, value, count - 1) }");
    }

    #[test]
    fn recursive_proc_return_match_keeps_callback_envelopes() {
        assert_recursive_proc_return_expression("match count { 0 => action(value); _ => right(action, value, count - 1) }");
    }

    #[test]
    fn structured_proc_returns_preserve_context_scope_escape_rejection() {
        let source = "proc values(flag: Bool) [] { return range(0, 2) }\nlet created = values(true)\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        for completion in [
            "range(0, 2)",
            "if flag { range(0, 2) } else { range(0, 2) }",
            "match flag { true => { range(0, 2) }; false => { range(0, 2) } }",
        ] {
            let source = format!("proc escaping(flag: Bool) [env, error] {{ return env ({{X: \"inner\"}}) {{ return {completion} }}? }}\n");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{completion}: {:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, &source, CheckOptions::for_execution());
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckContextScopeEscape)), "{completion}: {:?}", checked.diagnostics);
        }
    }

    #[test]
    fn omitted_proc_return_with_authored_parameters_prepares_unit_constructor_roles() {
        let source = concat!(
            "proc discard(payload: Int) [] { let _ = {items: map.empty(), payload} }\n",
            "let completed: Result[Unit] = discard(7)\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let declaration = checked.checked_declarations.functions.values().find(|declaration| declaration.name == "discard").unwrap();
        let scheme = declaration.scheme.as_ref().unwrap();
        assert_eq!(scheme.params()[0].ty.closed_ground_type(), Some(Type::Int));
        assert_eq!(scheme.return_type().closed_ground_type(), Some(Type::Result(Box::new(Type::Unit), Box::new(Type::Error))));
        assert!(checked.expr_types.values().all(|ty| !ty.contains_inference() && !ty.contains_recovery()));
    }

    #[test]
    fn grounded_proc_relay_preserves_original_application_return_sites() {
        let source = concat!(
            "pure first(payload: Int) -> Int { payload }\n",
            "pure second(payload: Int) -> Int { payload + 1 }\n",
            "proc relay(first, second, value, flag: Bool) { if flag { return first(value) }; second(value) }\n",
            "proc number(flag: Bool) [] { relay(first, second, 7, flag) }\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let sites = checked.checked_declarations.functions.values().filter_map(|declaration| declaration.scheme_body_facts.as_ref())
            .map(|facts| facts.proc_return_site_count()).sum::<usize>();
        assert_eq!(sites, 2);
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
