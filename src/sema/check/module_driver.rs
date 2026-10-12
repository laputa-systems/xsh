use super::{
    ArgsDomain, CheckBundleView, CheckOptions, CheckOutput, Checker, EffectSummary,
    FunctionEffectFact, ResourceTransferFacts, UserModuleSig,
};
use crate::source::{SourceId, SourceMap};
use rustc_hash::{FxHashMap, FxHashSet};
use crate::syntax::arena::{ArenaProgram, ArenaStmtKind, ArenaUserModule, TypeDefId};
use std::sync::{Arc, Mutex};
use super::module_record::{
    CheckedModuleFacts, ModuleRecord, ModuleRecordCache, ModuleRecordKey, ModuleRecords,
};
use super::infer_effects::{EffectDeclarationId, EffectGraph};

#[cfg(test)]
thread_local! {
    static ENTRY_CHECKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Keeps the source and arena identities that own reusable checked module facts.
pub struct CheckSession {
    program: Arc<ArenaProgram>,
    type_program: Arc<ArenaProgram>,
    sources: Arc<SourceMap>,
    records: Arc<Mutex<FxHashMap<ModuleRecordKey, Arc<ModuleRecord>>>>,
    view_context: Arc<()>,
}

impl CheckSession {
    /// Create a checker session over a finished immutable workspace.
    pub fn new(program: Arc<ArenaProgram>, sources: Arc<SourceMap>) -> Self {
        Self::new_with_type_program(Arc::clone(&program), sources, program)
    }

    /// Create a session with a distinct arena used to resolve type references.
    pub fn new_with_type_program(
        program: Arc<ArenaProgram>,
        sources: Arc<SourceMap>,
        type_program: Arc<ArenaProgram>,
    ) -> Self {
        Self {
            program,
            type_program,
            sources,
            records: Arc::new(Mutex::new(FxHashMap::default())),
            view_context: Arc::new(()),
        }
    }

    /// Check one entry root while retaining the workspace identities for reuse.
    pub fn check(
        &self,
        root_statements: crate::syntax::arena::ArenaRange,
        entry_source_id: SourceId,
        options: CheckOptions,
    ) -> CheckOutput {
        let view = self.view_for_root(root_statements, entry_source_id);
        self.check_view(&view, options)
    }

    /// Borrow the exact resolved entry scope used by checking and compact preparation.
    pub fn view_for_root(
        &self,
        root_statements: crate::syntax::arena::ArenaRange,
        entry_source_id: SourceId,
    ) -> CheckBundleView<'_> {
        let mut view = CheckBundleView::for_root(
            &self.program,
            root_statements,
            Some(&self.sources),
            Some(entry_source_id),
        );
        view.entry_context = Some(Arc::clone(&self.view_context));
        view
    }

    /// Check an entry-root view borrowed from this session without resolving its imports again.
    pub fn check_view(&self, view: &CheckBundleView<'_>, options: CheckOptions) -> CheckOutput {
        assert!(view
            .entry_context
            .as_ref()
            .is_some_and(|context| Arc::ptr_eq(context, &self.view_context)),
            "session checks require a view created by this session");
        assert!(std::ptr::eq(view.program(), self.program.as_ref()));
        assert!(view
            .sources
            .is_some_and(|sources| std::ptr::eq(sources, self.sources.as_ref())));
        let entry_source_id = view
            .root_source_id
            .expect("session root view retains its entry source identity");
        let source = self
            .sources
            .get(entry_source_id)
            .expect("entry source id belongs to the session source map")
            .text();
        let mut records = if options.interactive_commands.is_some() {
            ModuleRecordCache::default()
        } else {
            ModuleRecordCache::with_shared(self.records.clone())
        };
        Checker::check_bundle_in_session(
            view,
            source,
            options,
            self.type_program.clone(),
            Some(self.sources.as_ref()),
            &mut records,
        )
    }
}

impl Checker {
    pub(super) fn check_bundle_in_session(
        view: &CheckBundleView<'_>,
        source: &str,
        options: CheckOptions,
        type_program: Arc<ArenaProgram>,
        sources: Option<&SourceMap>,
        records: &mut ModuleRecordCache,
    ) -> CheckOutput {
        let program = view.program;
        program.symbol_owner().with_current(|| {
            let args_domain = records.checked_entry_domain(view).unwrap_or_else(|| {
                let module_headers = Self::module_type_headers(
                    view,
                    options,
                    type_program.clone(),
                );
                if Self::entry_uses_byte_arguments(
                    view,
                    source,
                    options,
                    type_program.clone(),
                    &module_headers,
                ) {
                    ArgsDomain::Bytes
                } else {
                    ArgsDomain::Str
                }
            });
            let active_records = records.get_or_prepare(view, args_domain);
            let (
                module_signatures,
                summaries,
                module_outputs,
                suppressed_resource_transfers,
            ) = Self::check_modules(
                view,
                &active_records,
                options,
                type_program.clone(),
                sources,
                source,
                args_domain,
            );

            let byte_script_arguments = args_domain == ArgsDomain::Bytes;
            let checked_entry = active_records.root().check_once(|| {
                #[cfg(test)]
                ENTRY_CHECKS.with(|checks| checks.set(checks.get() + 1));
                let record_options = CheckOptions {
                    reveal_types: true,
                    embedded_bodies: true,
                    ..options
                };
                let (effect_graph, root_summaries) = Self::solve_file_effect_probes(
                    |provisional| {
                        let mut probe = Self::session_effect_checker(
                            record_options,
                            byte_script_arguments,
                            &module_signatures,
                            EffectGraph::declarations_for_root(view),
                            summaries.clone(),
                            true,
                        );
                        probe.effect_summaries.extend(provisional);
                        probe.check_program_arena_with_type_program_and_sources(
                            view,
                            source,
                            type_program.clone(),
                            &active_records,
                        );
                        probe
                    },
                    &summaries,
                );
                let mut checker = Self::session_effect_checker(
                    record_options,
                    byte_script_arguments,
                    &module_signatures,
                    effect_graph,
                    summaries,
                    false,
                );
                checker.effect_summaries.extend(root_summaries);
                checker.check_program_arena_with_type_program_and_sources(
                    view,
                    source,
                    type_program,
                    &active_records,
                );
                checker.validate_deferred_effects();
                let facts = checker.effect_graph.facts(&checker.effect_summaries);
                Self::update_signature_effects(&mut checker.procs, &facts);
                Self::update_signature_effects(&mut checker.pures, &facts);
                Self::update_signature_effects(&mut checker.streams, &facts);
                checker.publish_command_binding_sites();
                let module_diagnostic_position = checker.module_diagnostic_position;
                let mut output = checker.into_output();
                Self::update_checked_function_effects(&mut output);
                CheckedModuleFacts {
                    signature: None,
                    output,
                    module_diagnostic_position,
                    suppressed_body_transfers: ResourceTransferFacts::default(),
                }
            });
            let mut output = checked_entry.output.clone();
            let has_constant_declarations = active_records
                .root()
                .prepared_facts()
                .resolved_bindings
                .declarations
                .values()
                .any(|declaration| {
                    matches!(declaration.kind, super::BindingKind::Constant(_))
                });
            if !has_constant_declarations {
                // The legacy collector publishes no preparation facts when no
                // constant declaration triggers collection for the bundle.
                output.prepared_constants.clear_prepared_declaration_facts();
            }
            if !options.reveal_types {
                output.reveal_types.clear();
            }
            output.embedded_bodies_checked = options.embedded_bodies;
            let root_diagnostic_count = output.diagnostics.len();
            // Whole-program checks publish imported-body facts before entry-body facts.
            let root_annotation_facts = std::mem::take(&mut output.annotation_facts);
            let root_reveal_types = std::mem::take(&mut output.reveal_types);
            let mut module_diagnostics = Vec::new();
            for mut module_output in module_outputs {
                module_diagnostics.append(&mut module_output.diagnostics);
                output.merge_module_facts(module_output);
            }
            output.resource_transfers.extend(suppressed_resource_transfers);
            // Keep declaration identities for suppressed embedded modules without
            // exposing their body-derived effect facts.
            for module in view.modules.iter().copied().filter(|module| {
                module.internal && !Self::publish_embedded_record(view.program, module, options)
            }) {
                for (identity, fact) in EffectGraph::declarations_for_module(view, module)
                    .declaration_facts()
                {
                    assert!(
                        output.function_effect_facts.insert(identity, fact).is_none(),
                        "suppressed embedded declarations have no checked module fact"
                    );
                }
            }
            let suppressed_embedded_types = Self::suppressed_embedded_types(view, options);
            if !suppressed_embedded_types.is_empty() {
                // Cached entry facts include the checked embedded-body superset. Keep
                // option projection local so another session check can publish it.
                output.checked_declarations.record_constructors = output
                    .checked_declarations
                    .record_constructors
                    .retaining_instances(|definition| {
                        !suppressed_embedded_types.contains(&definition)
                    });
            }
            output.annotation_facts.extend(root_annotation_facts);
            output.reveal_types.extend(root_reveal_types);
            let all_diagnostics = std::mem::take(&mut output.diagnostics);
            let root_diagnostics = &all_diagnostics[..root_diagnostic_count];
            let split = checked_entry
                .module_diagnostic_position
                .min(root_diagnostics.len());
            output.diagnostics.extend_from_slice(&root_diagnostics[..split]);
            output.diagnostics.extend(module_diagnostics);
            output.diagnostics.extend_from_slice(&root_diagnostics[split..]);
            output
        })
    }

    fn check_modules(
        view: &CheckBundleView<'_>,
        records: &ModuleRecords,
        options: CheckOptions,
        type_program: Arc<ArenaProgram>,
        sources: Option<&SourceMap>,
        entry_source: &str,
        args_domain: ArgsDomain,
    ) -> (
        FxHashMap<String, UserModuleSig>,
        std::collections::BTreeMap<EffectDeclarationId, EffectSummary>,
        Vec<CheckOutput>,
        ResourceTransferFacts,
    ) {
        let mut signatures = FxHashMap::default();
        let mut summaries = std::collections::BTreeMap::new();
        let mut outputs = FxHashMap::default();
        // Suppressed embedded outputs still need the transfer-site facts used by lowering.
        let mut suppressed_resource_transfers = ResourceTransferFacts::default();
        for module in view.modules.iter().copied() {
            let record = records
                .module(&module.key)
                .expect("reachable module has a prepared record")
                .clone();
            debug_assert_eq!(record.args_domain(), args_domain);
            let checked = record.check_once(|| {
                Self::check_module_record(
                    view,
                    module,
                    &record,
                            records.root(),
                    CheckOptions {
                        reveal_types: true,
                        embedded_bodies: true,
                        ..options
                    },
                    type_program.clone(),
                    sources,
                    entry_source,
                    &signatures,
                    &summaries,
                )
            });
            if !record.is_embedded() {
                signatures.insert(
                    module.key.clone(),
                    checked
                        .signature
                        .as_ref()
                        .expect("checked user module retains its exported signature")
                        .clone(),
                );
            }
            Self::extend_effect_summaries(&mut summaries, &checked.output.function_effect_facts);
            if Self::publish_embedded_record(view.program, module, options) {
                let mut output = checked.output.clone();
                if !options.reveal_types {
                    output.reveal_types.clear();
                }
                outputs.insert(module.key.clone(), output);
            } else {
                suppressed_resource_transfers.extend(checked.suppressed_body_transfers.clone());
            }
        }
        let publication_order = view
            .program
            .modules
            .iter()
            .filter(|module| module.internal && module.statements != view.root_statements)
            .map(|module| module.key.clone())
            .chain(
                view.modules
                    .iter()
                    .filter(|module| !module.internal)
                    .map(|module| module.key.clone()),
            );
        let outputs = publication_order
            .filter_map(|key| outputs.remove(&key))
            .collect();
        (signatures, summaries, outputs, suppressed_resource_transfers)
    }

    fn check_module_record(
        view: &CheckBundleView<'_>,
        module: &ArenaUserModule,
        record: &ModuleRecord,
        bundle_record: &ModuleRecord,
        options: CheckOptions,
        type_program: Arc<ArenaProgram>,
        sources: Option<&SourceMap>,
        entry_source: &str,
        dependencies: &FxHashMap<String, UserModuleSig>,
        dependency_summaries: &std::collections::BTreeMap<EffectDeclarationId, EffectSummary>,
    ) -> CheckedModuleFacts {
        let program = view.program;
        let module_view = CheckBundleView::for_module_owner(
            program,
            module,
            sources,
            record.source_id(),
        );
        let source = sources.map_or(entry_source, |sources| {
            sources
                .get(record.source_id())
                .expect("module source belongs to the session source map")
                .text()
        });
        let byte_script_arguments = record.args_domain() == ArgsDomain::Bytes;
        let (effect_graph, local_summaries) = Self::solve_file_effect_probes(
            |provisional| {
                let mut probe = Self::session_effect_checker(
                    options,
                    byte_script_arguments,
                    dependencies,
                    EffectGraph::declarations_for_module(&module_view, module),
                    dependency_summaries.clone(),
                    true,
                );
                probe.effect_summaries.extend(provisional);
                record.prepared_facts().install(&mut probe, program);
                // Keep constructor lookup maps scoped to imports while sharing publication order.
                probe.record_constructors.replace_instance_facts(
                    &bundle_record.prepared_facts().record_constructors,
                );
                probe.diagnostics.clear();
                probe.check_user_module_arena(
                    &module_view,
                    record,
                    type_program.clone(),
                    source,
                    module,
                );
                probe
            },
            dependency_summaries,
        );

        let mut checker = Self::session_effect_checker(
            options,
            byte_script_arguments,
            dependencies,
            effect_graph,
            dependency_summaries.clone(),
            false,
        );
        checker.effect_summaries.extend(local_summaries);
        record.prepared_facts().install(&mut checker, program);
        // Keep constructor lookup maps scoped to imports while sharing publication order.
        checker.record_constructors.replace_instance_facts(
            &bundle_record.prepared_facts().record_constructors,
        );
        checker.diagnostics.clear();
        let signature = checker.check_user_module_arena(
            &module_view,
            record,
            type_program,
            source,
            module,
        );
        checker.resolve_checked_types();
        let instances = checker
            .record_constructor_instances
            .iter()
            .map(|(&span, fact)| (span, fact.clone()))
            .collect::<Vec<_>>();
        for (span, fact) in instances {
            checker.publish_constructor_type_expressions(program, span, &fact);
        }
        checker.validate_deferred_effects();
        let mut resource_check = super::resource::check(&checker, &module_view);
        let suppressed_body_transfers =
            std::mem::take(&mut resource_check.suppressed_body_transfers);
        checker.diagnostics.extend(resource_check.diagnostics);
        checker.resource_transfers.extend(resource_check.transfers);
        let facts = checker.effect_graph.facts(&checker.effect_summaries);
        let mut signature = signature;
        Self::update_module_signature_effects(&mut signature, &facts);
        checker.publish_command_binding_sites();
        checker.record_constructors = record.prepared_facts().record_constructors.clone();
        let mut output = checker.into_output();
        Self::update_checked_function_effects(&mut output);
        CheckedModuleFacts {
            signature: Some(signature),
            output,
            module_diagnostic_position: 0,
            suppressed_body_transfers,
        }
    }

    fn session_effect_checker(
        options: CheckOptions,
        byte_script_arguments: bool,
        modules: &FxHashMap<String, UserModuleSig>,
        effect_graph: EffectGraph,
        effect_summaries: std::collections::BTreeMap<EffectDeclarationId, EffectSummary>,
        collecting_effects: bool,
    ) -> Self {
        let mut checker = Self::new(options);
        checker.byte_script_arguments = byte_script_arguments;
        if byte_script_arguments {
            checker.define_standard_values();
        }
        checker.user_modules = Arc::new(modules.clone());
        checker.effect_graph = effect_graph;
        checker.effect_summaries = effect_summaries;
        checker.authoritative_effect_graph = true;
        checker.collecting_effects = collecting_effects;
        checker.record_effects = collecting_effects;
        checker.defer_effect_diagnostics = true;
        checker
    }

    fn solve_file_effect_probes(
        mut probe: impl FnMut(
            std::collections::BTreeMap<EffectDeclarationId, EffectSummary>,
        ) -> Self,
        external: &std::collections::BTreeMap<EffectDeclarationId, EffectSummary>,
    ) -> (
        EffectGraph,
        std::collections::BTreeMap<EffectDeclarationId, EffectSummary>,
    ) {
        let mut provisional = std::collections::BTreeMap::new();
        loop {
            let checker = probe(provisional.clone());
            let solved = checker.effect_graph.solve_with_external(external);
            if !checker.provisional_effects_read.get() || solved == provisional {
                return (checker.effect_graph, solved);
            }
            provisional = solved;
        }
    }

    fn publish_embedded_record(
        program: &ArenaProgram,
        module: &ArenaUserModule,
        options: CheckOptions,
    ) -> bool {
        if !module.internal || options.embedded_bodies {
            return true;
        }
        program.module_statements(module).any(|id| {
            let kind = match program.arena.stmt(id).kind {
                crate::syntax::arena::ArenaStmtKind::Export(inner) => {
                    program.arena.stmt(inner).kind
                }
                other => other,
            };
            matches!(kind, crate::syntax::arena::ArenaStmtKind::PureDef(id)
                if program.arena.function_def(id).return_ty_defaulted)
        })
    }

    fn suppressed_embedded_types(
        view: &CheckBundleView<'_>,
        options: CheckOptions,
    ) -> FxHashSet<TypeDefId> {
        view.modules
            .iter()
            .filter(|module| {
                module.internal && !Self::publish_embedded_record(view.program, module, options)
            })
            .flat_map(|module| view.program.module_statements(module))
            .filter_map(|statement| {
                let kind = match view.program.arena.stmt(statement).kind {
                    ArenaStmtKind::Export(inner) => view.program.arena.stmt(inner).kind,
                    kind => kind,
                };
                match kind {
                    ArenaStmtKind::TypeDef(definition) => Some(definition),
                    _ => None,
                }
            })
            .collect()
    }

    fn entry_uses_byte_arguments(
        view: &CheckBundleView<'_>,
        source: &str,
        options: CheckOptions,
        type_program: Arc<ArenaProgram>,
        modules: &FxHashMap<String, UserModuleSig>,
    ) -> bool {
        let program = view.program;
        let mut checker = Self::new(options);
        checker.user_modules = Arc::new(modules.clone());
        checker.collect_type_imports_arena(program, view.statement_ids());
        checker.collect_definitions_arena(
            program,
            type_program,
            source,
            view.statement_ids(),
        );
        let uses_bytes = checker.procs.get(&super::Name::intern("main")).is_some_and(|signature| {
            matches!(signature.params.as_slice(), [parameter]
                if parameter.rest && !parameter.defaulted
                    && matches!(&parameter.ty, super::Type::List(item)
                        if item.as_ref() == &super::Type::Bytes))
        });
        uses_bytes
    }

    fn extend_effect_summaries(
        summaries: &mut std::collections::BTreeMap<EffectDeclarationId, EffectSummary>,
        facts: &std::collections::BTreeMap<EffectDeclarationId, FunctionEffectFact>,
    ) {
        summaries.extend(facts.iter().map(|(&id, fact)| {
            (
                id,
                EffectSummary {
                    known: fact.required.clone().unwrap_or_default(),
                    unknown_chain: fact.unknown_chain.clone(),
                },
            )
        }));
    }

    fn update_signature_effects(
        signatures: &mut rustc_hash::FxHashMap<super::Name, super::FunctionSig>,
        facts: &std::collections::BTreeMap<EffectDeclarationId, FunctionEffectFact>,
    ) {
        for signature in signatures.values_mut() {
            let Some(fact) = signature.effect_declaration.and_then(|id| facts.get(&id)) else {
                continue;
            };
            signature.effects = fact.effective.clone();
            signature.inferred_effects = fact.inferred;
        }
    }

    fn update_module_signature_effects(
        signature: &mut UserModuleSig,
        facts: &std::collections::BTreeMap<EffectDeclarationId, FunctionEffectFact>,
    ) {
        Self::update_signature_effects(&mut signature.procs, facts);
        Self::update_signature_effects(&mut signature.pures, facts);
        Self::update_signature_effects(&mut signature.streams, facts);
    }

    fn update_checked_function_effects(output: &mut CheckOutput) {
        for declaration in output.checked_declarations.functions.values_mut() {
            let Some(signature) = &mut declaration.signature else {
                continue;
            };
            let id = EffectDeclarationId {
                namespace: declaration.namespace,
                body: declaration.body,
            };
            let Some(fact) = output.function_effect_facts.get(&id) else {
                continue;
            };
            signature.effects = fact.effective.clone();
            signature.inferred_effects = fact.inferred;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::CheckOptions;
    use crate::sema::check::decl::USER_MODULE_CHECKS;
    use crate::syntax::arena::ArenaProgramBuilder;
    use crate::syntax::parser::Parser;

    fn no_interactive_commands(_: &str) -> bool {
        false
    }

    fn permutations(values: &mut [usize], index: usize, output: &mut Vec<Vec<usize>>) {
        if index == values.len() {
            output.push(values.to_vec());
            return;
        }
        for next in index..values.len() {
            values.swap(index, next);
            permutations(values, index + 1, output);
            values.swap(index, next);
        }
    }

    fn assert_same_facts(actual: &CheckOutput, expected: &CheckOutput) {
        assert_eq!(actual.diagnostics, expected.diagnostics);
        assert_eq!(actual.reveal_types, expected.reveal_types);
        assert_eq!(actual.function_return_types, expected.function_return_types);
        assert_eq!(actual.parameter_types, expected.parameter_types);
        assert_eq!(actual.function_effect_facts, expected.function_effect_facts);
        assert_eq!(actual.resource_transfers, expected.resource_transfers);
        assert_eq!(actual.statement_positions, expected.statement_positions);
        assert_eq!(actual.embedded_bodies_checked, expected.embedded_bodies_checked);
        assert_eq!(
            actual.checked_declarations.functions.keys().collect::<Vec<_>>(),
            expected.checked_declarations.functions.keys().collect::<Vec<_>>()
        );
        assert_eq!(
            actual.checked_declarations.cli_entries.len(),
            expected.checked_declarations.cli_entries.len()
        );
    }

    #[test]
    fn embedded_test_interpolation_publishes_nested_expression_types() {
        let source = concat!(
            "test test_nested_interpolation {\n",
            "  let nested = f\"\"\"{ {name: \"demo\"}.name }\"\"\"\n",
            "  assert nested == \"demo\"\n",
            "}\n",
        );
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("entry.xsh", source);
        let mut builder = ArenaProgramBuilder::with_token_capacity(32);
        let parsed = Parser::parse_source_into_arena_builder(source_id, source, &mut builder);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let program = Arc::new(builder.finish_with_statements(parsed.statements));
        let session = CheckSession::new(Arc::clone(&program), Arc::new(sources));
        let record_start = source
            .find("{name: \"demo\"}")
            .expect("record expression in interpolation");
        let string_start = source.find("\"demo\"").expect("string literal in record");
        for options in [CheckOptions::for_execution(), CheckOptions::for_check_cli()] {
            let output = session.check(parsed.statements, source_id, options);
            assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
            assert!(
                matches!(
                    output.expr_types.get(&crate::source::Span::new(
                        source_id,
                        record_start,
                        record_start + "{name: \"demo\"}".len(),
                    )),
                    Some(crate::sema::check::Type::Record(fields))
                        if fields.get(&crate::sema::check::Name::intern("name"))
                            == Some(&crate::sema::check::Type::Str)
                ),
                "the interpolation's record literal retains its checked type"
            );
            assert_eq!(
                output.expr_types.get(&crate::source::Span::new(
                    source_id,
                    string_start,
                    string_start + "\"demo\"".len(),
                )),
                Some(&crate::sema::check::Type::Str),
                "the interpolation's string literal retains its checked type"
            );
        }
    }

    #[test]
    fn deferred_effect_errors_do_not_publish_static_call_spans() {
        fn check(source: &str) -> (CheckOutput, crate::source::SourceId) {
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("entry.xsh", source);
            let mut builder = ArenaProgramBuilder::with_token_capacity(48);
            let parsed = Parser::parse_source_into_arena_builder(source_id, source, &mut builder);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let program = Arc::new(builder.finish_with_statements(parsed.statements));
            let sources = Arc::new(sources);
            let session = CheckSession::new(Arc::clone(&program), Arc::clone(&sources));
            let view = session.view_for_root(parsed.statements, source_id);
            let output = session.check_view(&view, CheckOptions::default());
            (output, source_id)
        }

        fn call_span(
            source: &str,
            call: &str,
            source_id: crate::source::SourceId,
        ) -> crate::source::Span {
            let start = source.find(call).expect("call expression");
            crate::source::Span::new(source_id, start, start + call.len())
        }

        let missing_net_source = concat!(
            "pure passthrough(value: Result[Int, Error]) -> Result[Int, Error] { value }\n",
            "pure identity(value: Int) -> Int { value }\n",
            "proc fifo_copy() [fs, process, env, time, error] -> Result[Int, Error] {\n",
            "  let status = identity(1)\n",
            "  passthrough(unix.write_fd(1, b\"\"))\n",
            "}\n",
        );
        let (missing_net, source_id) = check(missing_net_source);
        let inner_call = call_span(missing_net_source, "unix.write_fd(1, b\"\")", source_id);
        let outer_call = call_span(
            missing_net_source,
            "passthrough(unix.write_fd(1, b\"\"))",
            source_id,
        );
        let valid_sibling = call_span(missing_net_source, "identity(1)", source_id);
        assert!(missing_net.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckEffectViolation)
                && diagnostic.labels.iter().any(|label| label.span == inner_call)
        }),
            "the missing `net` effect must be reported at the call: {:?}",
            missing_net.diagnostics
        );
        assert!(
            missing_net.statically_resolved_call_spans.contains(&valid_sibling),
            "an unrelated valid sibling call remains statically resolved"
        );
        assert!(
            !missing_net.statically_resolved_call_spans.contains(&inner_call)
                && !missing_net.statically_resolved_call_spans.contains(&outer_call),
            "a deferred effect error invalidates inner and enclosing call spans: inner={}, outer={}",
            missing_net.statically_resolved_call_spans.contains(&inner_call),
            missing_net.statically_resolved_call_spans.contains(&outer_call),
        );

        let allowed_net_source = missing_net_source.replace(
            "[fs, process, env, time, error]",
            "[fs, process, env, time, error, net]",
        );
        let (allowed_net, source_id) = check(&allowed_net_source);
        let inner_call = call_span(&allowed_net_source, "unix.write_fd(1, b\"\")", source_id);
        let outer_call = call_span(
            &allowed_net_source,
            "passthrough(unix.write_fd(1, b\"\"))",
            source_id,
        );
        let valid_sibling = call_span(&allowed_net_source, "identity(1)", source_id);
        assert!(
            allowed_net.diagnostics.is_empty(),
            "{:?}",
            allowed_net.diagnostics
        );
        assert!(
            allowed_net.statically_resolved_call_spans.contains(&inner_call)
                && allowed_net.statically_resolved_call_spans.contains(&outer_call)
                && allowed_net.statically_resolved_call_spans.contains(&valid_sibling),
            "statically named calls with a valid effect contract remain published"
        );
    }

    #[test]
    fn checking_two_entries_reuses_shared_module_preparation_per_session() {
        let mut sources = SourceMap::new();
        let first_text = "use shared\nconst first = 1\n";
        let second_text = "use shared\nconst second = 2\n";
        let byte_text = "use shared\nproc main(...argv: List[Bytes]) {}\n";
        let shared_text = "##! Shared.\n## Read the clock.\nexport proc observe() { reveal_type(args); let _ = time.now() }\npure incoming() { args }\n";
        let first_source = sources.add_file("first.xsh", first_text);
        let second_source = sources.add_file("second.xsh", second_text);
        let byte_source = sources.add_file("byte-entry.xsh", byte_text);
        let shared_source = sources.add_file("shared.xsh", shared_text);

        let mut builder = ArenaProgramBuilder::with_token_capacity(128);
        let first = Parser::parse_source_into_arena_builder(first_source, first_text, &mut builder);
        let second = Parser::parse_source_into_arena_builder(second_source, second_text, &mut builder);
        let byte = Parser::parse_source_into_arena_builder(byte_source, byte_text, &mut builder);
        let shared = Parser::parse_source_into_arena_builder(shared_source, shared_text, &mut builder);
        assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
        assert!(second.diagnostics.is_empty(), "{:?}", second.diagnostics);
        assert!(byte.diagnostics.is_empty(), "{:?}", byte.diagnostics);
        assert!(shared.diagnostics.is_empty(), "{:?}", shared.diagnostics);
        let shared_name = builder.symbol_owner().intern("shared");
        builder.push_arena_module("shared".to_string(), shared_name, shared.statements);
        for root in [first.statements, second.statements, byte.statements] {
            for statement in builder.statement_ids(root) {
                if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
                    builder.set_use_resolved(use_id, Arc::from("shared"));
                }
            }
        }
        let program = Arc::new(builder.finish_with_statements(first.statements));
        let sources = Arc::new(sources);
        USER_MODULE_CHECKS.with(|checks| checks.set(0));
        let session = CheckSession::new(program.clone(), sources);
        let first_output = session.check(first.statements, first_source, CheckOptions::for_execution());
        let first_record = session
            .records
            .lock()
            .expect("module record cache poisoned")
            .values()
            .next()
            .expect("first entry prepared the shared module")
            .clone();
        let second_output = session.check(second.statements, second_source, CheckOptions::for_check_cli());
        assert_eq!(USER_MODULE_CHECKS.with(std::cell::Cell::get), 2);

        assert!(first_output.diagnostics.is_empty(), "{:?}", first_output.diagnostics);
        assert!(second_output.diagnostics.is_empty(), "{:?}", second_output.diagnostics);
        assert!(first_output.reveal_types.is_empty());
        assert!(second_output.reveal_types.iter().any(|diagnostic| {
            diagnostic
                .labels
                .iter()
                .any(|label| label.span.source_id == shared_source)
        }));
        let second_record = session
            .records
            .lock()
            .expect("module record cache poisoned")
            .values()
            .next()
            .expect("second entry retains the shared module")
            .clone();
        assert!(Arc::ptr_eq(&first_record, &second_record));
        assert!(first_output.function_effect_facts.values().any(|fact| {
            fact.effective
                .as_ref()
                .is_some_and(|effects| effects.contains(&crate::syntax::node::Effect::Time))
        }));
        assert!(second_output.function_effect_facts.values().any(|fact| {
            fact.effective
                .as_ref()
                .is_some_and(|effects| effects.contains(&crate::syntax::node::Effect::Time))
        }));
        let byte_output = session.check(byte.statements, byte_source, CheckOptions::for_execution());
        assert!(byte_output.diagnostics.is_empty(), "{:?}", byte_output.diagnostics);
        assert_eq!(USER_MODULE_CHECKS.with(std::cell::Cell::get), 4);
        assert_eq!(session.records.lock().expect("module record cache poisoned").len(), 5);
        let incoming = |output: &CheckOutput| {
            output
                .checked_declarations
                .functions
                .values()
                .find(|declaration| declaration.name.as_str() == "incoming")
                .and_then(|declaration| declaration.signature.as_ref())
                .map(|signature| signature.return_ty.clone())
                .expect("the imported pure declaration is projected into checked facts")
        };
        assert_eq!(
            incoming(&byte_output),
            crate::sema::check::Type::List(Box::new(crate::sema::check::Type::Bytes))
        );
        assert_eq!(
            incoming(&second_output),
            crate::sema::check::Type::List(Box::new(crate::sema::check::Type::Str))
        );
    }

    #[test]
    fn b2sum_publishes_cli_plans_from_its_imported_module() {
        let mut sources = SourceMap::new();
        let root_text = "use shared\nlet options = shared.options([])\n";
        let module_text = "##! Shared CLI options.\n## Parse the shared applet options.\nexport pure options(argv: List[Str]) -> Result[Record, Error] { cli.applet(argv, {jobs: {kind: \"Int\", form: \"-j --jobs N\", default: 4}}) }\n";
        let root_source = sources.add_file("entry.xsh", root_text);
        let module_source = sources.add_file("shared.xsh", module_text);
        let mut builder = ArenaProgramBuilder::with_token_capacity(128);
        let root = Parser::parse_source_into_arena_builder(root_source, root_text, &mut builder);
        let module = Parser::parse_source_into_arena_builder(module_source, module_text, &mut builder);
        assert!(root.diagnostics.is_empty(), "{:?}", root.diagnostics);
        assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
        let module_name = builder.symbol_owner().intern("shared");
        builder.push_arena_module("shared".to_string(), module_name, module.statements);
        for statement in builder.statement_ids(root.statements) {
            if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
                builder.set_use_resolved(use_id, Arc::from("shared"));
            }
        }

        let program = Arc::new(builder.finish_with_statements(root.statements));
        let sources = Arc::new(sources);
        let session = CheckSession::new(Arc::clone(&program), sources);
        let output = session.check(
            root.statements,
            root_source,
            CheckOptions::default(),
        );
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let module_plans = output
            .prepared_constants
            .cli_plans
            .lock()
            .expect("CLI plan cache poisoned")
            .keys()
            .filter(|(expression, _)| {
                program.arena.expr(*expression).span.source_id == module_source
            })
            .count();
        assert_eq!(module_plans, 1, "the imported applet plan must be published");
    }

    #[test]
    fn wire_enum_lookup_facts_do_not_leak_into_empty_constant_publication() {
        let source_text = concat!(
            "enum State: Str { Ready = \"ready\" }\n",
            "let state: State = Ready\n",
        );
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("entry.xsh", source_text);
        let mut builder = ArenaProgramBuilder::with_token_capacity(64);
        let parsed = Parser::parse_source_into_arena_builder(source_id, source_text, &mut builder);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let program = Arc::new(builder.finish_with_statements(parsed.statements));
        let session = CheckSession::new(Arc::clone(&program), Arc::new(sources));
        let options = [
            CheckOptions::default(),
            CheckOptions {
                reveal_types: true,
                ..CheckOptions::default()
            },
            CheckOptions::for_execution(),
            CheckOptions::for_check_cli(),
        ];

        for option in options {
            let output = session.check(parsed.statements, source_id, option);
            assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
            assert!(
                output.prepared_constants.values.is_empty()
                    && output.prepared_constants.types.is_empty()
                    && output.prepared_constants.record_constructor_instances.is_empty()
                    && output.prepared_constants.cli_wire_enums.mappings.is_empty(),
                "an owner with no constants publishes the legacy empty prepared-constants facts"
            );
            assert!(
                output
                    .checked_declarations
                    .wire_enums
                    .mappings
                    .keys()
                    .any(|name| name.as_str() == "State"),
                "wire-enum lookup facts remain available independently of constant publication"
            );
        }
    }

    #[test]
    fn imported_module_cli_plans_are_published_with_entry_facts() {
        use crate::frontend::load::{
            entry_source_from_text, parse_load_entry_source_arena_only, project_module_roots,
        };
        use std::path::PathBuf;

        let entry_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("core/b2sum.xsh");
        let entry_file = entry_path.to_str().expect("entry path is UTF-8");
        let entry_text = std::fs::read_to_string(&entry_path).expect("read b2sum source");
        let entry = entry_source_from_text(entry_file, entry_text);
        let entry_source = entry.source_id;
        let (sources, parsed) = parse_load_entry_source_arena_only(
            entry_file,
            entry,
            project_module_roots(&entry_path).expect("resolve core module roots"),
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let module_path = entry_path
            .parent()
            .expect("core source has a parent")
            .join("lib/checksums.xsh");
        let module_file = module_path.to_str().expect("module path is UTF-8");
        let module_source = sources
            .files()
            .iter()
            .find(|source| source.name() == module_file)
            .expect("checksums module was loaded")
            .id();
        let root_statements = parsed.arena.statements;
        let program = Arc::new(parsed.arena);
        let session = CheckSession::new(Arc::clone(&program), Arc::new(sources));
        let output = session.check(root_statements, entry_source, CheckOptions::default());
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let module_plans = output
            .prepared_constants
            .cli_plans
            .lock()
            .expect("CLI plan cache poisoned")
            .keys()
            .filter(|(expression, _)| {
                program.arena.expr(*expression).span.source_id == module_source
            })
            .count();
        assert_eq!(module_plans, 1, "the imported applet plan must be published");
    }

    #[test]
    fn diamond_imported_cli_schemas_publish_owner_local_plan_keys() {
        let mut sources = SourceMap::new();
        let root_text = "use left\nuse right\nlet left_options = left.options([])\nlet right_options = right.options([])\n";
        let left_text = "##! Left branch.\nuse shared\n## Calls the shared applet descriptor.\nexport pure options(argv: List[Str]) -> Result[Record, Error] { cli.applet(argv, shared.options) }\n";
        let right_text = "##! Right branch.\nuse shared\n## Calls the shared applet descriptor.\nexport pure options(argv: List[Str]) -> Result[Record, Error] { cli.applet(argv, shared.options) }\n";
        let shared_text = "##! Shared CLI schema.\n## Defines a descriptor consumed by both branches.\nexport const options = {jobs: {kind: \"Int\", form: \"-j --jobs N\", default: 4}}\n";
        let root_source = sources.add_file("entry.xsh", root_text);
        let left_source = sources.add_file("left.xsh", left_text);
        let right_source = sources.add_file("right.xsh", right_text);
        let shared_source = sources.add_file("shared.xsh", shared_text);
        let mut builder = ArenaProgramBuilder::with_token_capacity(192);
        let root = Parser::parse_source_into_arena_builder(root_source, root_text, &mut builder);
        let left = Parser::parse_source_into_arena_builder(left_source, left_text, &mut builder);
        let right = Parser::parse_source_into_arena_builder(right_source, right_text, &mut builder);
        let shared = Parser::parse_source_into_arena_builder(shared_source, shared_text, &mut builder);
        assert!(root.diagnostics.is_empty(), "{:?}", root.diagnostics);
        assert!(left.diagnostics.is_empty(), "{:?}", left.diagnostics);
        assert!(right.diagnostics.is_empty(), "{:?}", right.diagnostics);
        assert!(shared.diagnostics.is_empty(), "{:?}", shared.diagnostics);

        for (key, module) in [
            ("left", &left),
            ("right", &right),
            ("shared", &shared),
        ] {
            let module_name = builder.symbol_owner().intern(key);
            builder.push_arena_module(key.to_string(), module_name, module.statements);
        }
        for (index, statement) in builder
            .statement_ids(root.statements)
            .into_iter()
            .enumerate()
        {
            if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
                let key = if index == 0 { "left" } else { "right" };
                builder.set_use_resolved(use_id, Arc::from(key));
            }
        }
        for module in [&left, &right] {
            for statement in builder.statement_ids(module.statements) {
                if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
                    builder.set_use_resolved(use_id, Arc::from("shared"));
                }
            }
        }

        let program = Arc::new(builder.finish_with_statements(root.statements));
        let sources = Arc::new(sources);
        let session = CheckSession::new(Arc::clone(&program), sources);
        let output = session.check(
            root.statements,
            root_source,
            CheckOptions::default(),
        );
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let published_plans = output
            .prepared_constants
            .cli_plans
            .lock()
            .expect("CLI plan cache poisoned");
        assert_eq!(
            published_plans.len(),
            1,
            "the shared canonical descriptor has one published plan"
        );
        let (expression, _) = published_plans.keys().next().expect("published plan");
        assert_eq!(
            program.arena.expr(*expression).span.source_id,
            shared_source,
            "the plan key remains owned by its canonical descriptor"
        );
    }

    #[test]
    fn cached_embedded_resource_check_visits_its_owner_once() {
        let mut sources = SourceMap::new();
        let entry_text = "1\n";
        let module_text = "##! Managed resource example.\n## Reports a use after release.\nexport proc bad() [fs, error] -> Result[Unit] { let root = fs.tempdir()?; root.close()?; root.close()?; return Ok() }\n";
        let entry_source = sources.add_file("entry.xsh", entry_text);
        let module_source = sources.add_file("owned.xsh", module_text);
        let mut builder = ArenaProgramBuilder::with_token_capacity(96);
        let entry = Parser::parse_source_into_arena_builder(entry_source, entry_text, &mut builder);
        let module = Parser::parse_source_into_arena_builder(module_source, module_text, &mut builder);
        assert!(entry.diagnostics.is_empty(), "{:?}", entry.diagnostics);
        assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
        let module_name = builder.symbol_owner().intern("owned");
        builder.push_internal_arena_module("owned".to_string(), module_name, module.statements);

        let program = Arc::new(builder.finish_with_statements(entry.statements));
        let sources = Arc::new(sources);
        let session = CheckSession::new(Arc::clone(&program), Arc::clone(&sources));
        let view = session.view_for_root(entry.statements, entry_source);
        super::super::resource::W9_RESOURCE_VISITS
            .with(|visits| *visits.borrow_mut() = (0, Default::default()));

        let default_output = session.check_view(&view, CheckOptions::default());
        let default_resource_diagnostics = default_output
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckUseAfterRelease)
                    && diagnostic.labels.iter().any(|label| label.span.source_id == module_source)
            })
            .count();
        assert_eq!(default_resource_diagnostics, 0, "{:?}", default_output.diagnostics);
        let first_visits = super::super::resource::W9_RESOURCE_VISITS
            .with(|visits| visits.borrow().clone());
        assert_eq!(first_visits.1.get(&module_source), Some(&1));

        let execution_output = session.check_view(&view, CheckOptions::for_execution());
        let execution_resource_diagnostics = execution_output
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckUseAfterRelease)
                    && diagnostic.labels.iter().any(|label| label.span.source_id == module_source)
            })
            .count();
        assert_eq!(execution_resource_diagnostics, 1, "{:?}", execution_output.diagnostics);
        assert_eq!(
            super::super::resource::W9_RESOURCE_VISITS.with(|visits| visits.borrow().clone()),
            first_visits,
            "reusing the checked embedded owner should not repeat resource analysis"
        );
    }

    #[test]
    fn byte_entry_resolves_imported_argument_alias_before_checking_module_bodies() {
        let mut sources = SourceMap::new();
        let root_text = "use shared\nproc main(...argv: shared.Arguments) {}\n";
        let shared_text = "##! Byte argument aliases.\n## Shared declarations for the entry check.\n## Arguments accepted as byte strings.\nexport type Arguments = List[Bytes]\n## Return the imported script arguments.\nexport pure incoming() -> List[Bytes] { args }\n";
        let root_source = sources.add_file("byte-entry.xsh", root_text);
        let shared_source = sources.add_file("shared.xsh", shared_text);

        let mut builder = ArenaProgramBuilder::with_token_capacity(128);
        let root = Parser::parse_source_into_arena_builder(root_source, root_text, &mut builder);
        let shared =
            Parser::parse_source_into_arena_builder(shared_source, shared_text, &mut builder);
        assert!(root.diagnostics.is_empty(), "{:?}", root.diagnostics);
        assert!(shared.diagnostics.is_empty(), "{:?}", shared.diagnostics);
        let shared_name = builder.symbol_owner().intern("shared");
        builder.push_arena_module("shared".to_string(), shared_name, shared.statements);
        for statement in builder.statement_ids(root.statements) {
            if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
                builder.set_use_resolved(use_id, Arc::from("shared"));
            }
        }

        let program = Arc::new(builder.finish_with_statements(root.statements));
        let sources = Arc::new(sources);
        let session = CheckSession::new(Arc::clone(&program), Arc::clone(&sources));
        let view = session.view_for_root(root.statements, root_source);
        let expected_module_checks = view.modules().count() * 2;
        USER_MODULE_CHECKS.with(|checks| checks.set(0));

        let output = session.check_view(&view, CheckOptions::for_execution());

        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(USER_MODULE_CHECKS.with(std::cell::Cell::get), expected_module_checks);
        let declaration = output
            .checked_declarations
            .functions
            .values()
            .find(|declaration| declaration.name.as_str() == "incoming")
            .expect("the imported function is projected into checked facts");
        let incoming = declaration
            .signature
            .as_ref()
            .expect("the imported function signature is projected into checked facts");
        assert_eq!(
            incoming.return_ty,
            crate::sema::check::Type::List(Box::new(crate::sema::check::Type::Bytes))
        );
        assert_eq!(
            declaration.body.source_id,
            shared_source,
            "the cached function fact keeps its source identity"
        );
    }

    #[test]
    fn imported_effects_flow_into_entry_inference() {
        let mut sources = SourceMap::new();
        let root_text = "use helper\nproc label() -> Str { helper.fetch() }\nprint label()\n";
        let helper_text = "##! Effectful provider.\n## Prints before returning a label.\nexport proc fetch() [io] -> Str { print \"fetch\"; \"ready\" }\n";
        let root_source = sources.add_file("main.xsh", root_text);
        let helper_source = sources.add_file("helper.xsh", helper_text);
        let mut builder = ArenaProgramBuilder::with_token_capacity(128);
        let root = Parser::parse_source_into_arena_builder(root_source, root_text, &mut builder);
        let helper = Parser::parse_source_into_arena_builder(helper_source, helper_text, &mut builder);
        assert!(root.diagnostics.is_empty(), "{:?}", root.diagnostics);
        assert!(helper.diagnostics.is_empty(), "{:?}", helper.diagnostics);
        let helper_name = builder.symbol_owner().intern("helper");
        builder.push_arena_module("helper".to_string(), helper_name, helper.statements);
        for statement in builder.statement_ids(root.statements) {
            if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
                builder.set_use_resolved(use_id, Arc::from("helper"));
            }
        }
        let program = Arc::new(builder.finish_with_statements(root.statements));
        let sources = Arc::new(sources);
        let session = CheckSession::new(Arc::clone(&program), Arc::clone(&sources));
        let options = CheckOptions::for_execution();
        let output = session.check(root.statements, root_source, options);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(
            output.diagnostics.iter().map(|diagnostic| diagnostic.code).collect::<Vec<_>>(),
            Checker::check_bundle_view_with_type_program(
                &CheckBundleView::for_root(
                    &program,
                    root.statements,
                    Some(&sources),
                    Some(root_source),
                ),
                root_text,
                options,
                Arc::clone(&program),
            )
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        );
        assert!(output.function_effect_facts.values().any(|fact| {
            fact.effective
                .as_ref()
                .is_some_and(|effects| effects.contains(&crate::syntax::node::Effect::Io))
        }));
    }

    #[test]
    fn imported_same_name_effects_do_not_replace_root_callable_projection() {
        let mut sources = SourceMap::new();
        let root_text = "use helper\nproc observe() [io] { print \"root\" }\nobserve()\n";
        let helper_text = "##! Imported effect source.\n## Reads the clock.\nexport proc observe() { let _ = time.now() }\n";
        let root_source = sources.add_file("main.xsh", root_text);
        let helper_source = sources.add_file("helper.xsh", helper_text);
        let mut builder = ArenaProgramBuilder::with_token_capacity(128);
        let root = Parser::parse_source_into_arena_builder(root_source, root_text, &mut builder);
        let helper = Parser::parse_source_into_arena_builder(helper_source, helper_text, &mut builder);
        assert!(root.diagnostics.is_empty(), "{:?}", root.diagnostics);
        assert!(helper.diagnostics.is_empty(), "{:?}", helper.diagnostics);
        let helper_name = builder.symbol_owner().intern("helper");
        builder.push_arena_module("helper".to_string(), helper_name, helper.statements);
        for statement in builder.statement_ids(root.statements) {
            if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
                builder.set_use_resolved(use_id, Arc::from("helper"));
            }
        }
        let program = Arc::new(builder.finish_with_statements(root.statements));
        let sources = Arc::new(sources);
        let output = CheckSession::new(Arc::clone(&program), Arc::clone(&sources)).check(
            root.statements,
            root_source,
            CheckOptions::for_execution(),
        );
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(
            output.callable_effects.get("observe"),
            Some(&Some(vec![crate::syntax::node::Effect::Io]))
        );
        assert!(output.function_effect_facts.iter().any(|(identity, fact)| {
            identity.namespace.is_none()
                && identity.body.source_id == root_source
                && fact.effective.as_ref().is_some_and(|effects| {
                    effects.contains(&crate::syntax::node::Effect::Io)
                })
        }));
        assert!(output.function_effect_facts.iter().any(|(identity, fact)| {
            identity.namespace == Some(helper_name)
                && identity.body.source_id == helper_source
                && fact.effective.as_ref().is_some_and(|effects| {
                    effects.contains(&crate::syntax::node::Effect::Time)
                })
        }));
    }

    #[test]
    fn cached_module_facts_match_fresh_checks_for_every_option_order() {
        let mut sources = SourceMap::new();
        let root_text = "use shared\nproc main(value: Int) {}\n";
        let shared_text = "export proc observe() { reveal_type(args); let _ = time.now() }\npure incoming() { args }\n";
        let embedded_text = "pure inferred() { missing_value }\n";
        let root_source = sources.add_file("entry.xsh", root_text);
        let shared_source = sources.add_file("shared.xsh", shared_text);
        let embedded_source = sources.add_file("embedded.xsh", embedded_text);

        let mut builder = ArenaProgramBuilder::with_token_capacity(128);
        let root = Parser::parse_source_into_arena_builder(root_source, root_text, &mut builder);
        let shared =
            Parser::parse_source_into_arena_builder(shared_source, shared_text, &mut builder);
        let embedded = Parser::parse_source_into_arena_builder(
            embedded_source,
            embedded_text,
            &mut builder,
        );
        for parsed in [&root, &shared, &embedded] {
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        }
        let shared_name = builder.symbol_owner().intern("shared");
        builder.push_arena_module("shared".to_string(), shared_name, shared.statements);
        let embedded_name = builder.symbol_owner().intern("embedded_probe");
        builder.push_internal_arena_module(
            "embedded-probe".to_string(),
            embedded_name,
            embedded.statements,
        );
        for statement in builder.statement_ids(root.statements) {
            if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
                builder.set_use_resolved(use_id, Arc::from("shared"));
            }
        }
        let program = Arc::new(builder.finish_with_statements(root.statements));
        let sources = Arc::new(sources);
        let view = CheckBundleView::for_root(
            &program,
            root.statements,
            Some(&sources),
            Some(root_source),
        );
        let options = [
            CheckOptions::default(),
            CheckOptions::for_execution(),
            CheckOptions::for_check_cli(),
            CheckOptions::for_interactive_commands(no_interactive_commands),
        ];
        let expected = options.map(|options| {
            Checker::check_bundle_view_with_type_program(
                &view,
                root_text,
                options,
                Arc::clone(&program),
            )
        });
        assert!(expected[0]
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code
                == Some(crate::diagnostic::DiagnosticCode::CompactMainMissingSpread)));
        assert!(expected[3]
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code
                != Some(crate::diagnostic::DiagnosticCode::CompactMainMissingSpread)));
        assert!(expected[0].diagnostics.iter().any(|diagnostic| {
            diagnostic.labels.iter().any(|label| label.span.source_id == embedded_source)
        }));
        assert!(expected[0].reveal_types.is_empty());
        assert!(expected[2].reveal_types.iter().any(|diagnostic| {
            diagnostic.labels.iter().any(|label| label.span.source_id == shared_source)
        }));

        let mut orders = Vec::new();
        permutations(&mut [0, 1, 2, 3], 0, &mut orders);
        assert_eq!(orders.len(), 24);
        for order in orders {
            let session = CheckSession::new(Arc::clone(&program), Arc::clone(&sources));
            ENTRY_CHECKS.with(|checks| checks.set(0));
            for index in order {
                let actual = session.check(root.statements, root_source, options[index]);
                assert_same_facts(&actual, &expected[index]);
            }
            assert_eq!(ENTRY_CHECKS.with(std::cell::Cell::get), 2);
        }
    }

}
