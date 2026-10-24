use super::{
    ArgsDomain, CacheTrace, CheckBundleView, CheckOptions, CheckOutput, Checker, EffectSummary,
    FunctionEffectFact, RecordConstructors, ResourceTransferFacts, UserModuleSig,
};
use crate::source::{SourceId, SourceMap};
use rustc_hash::{FxHashMap, FxHashSet};
use crate::syntax::arena::{ArenaProgram, ArenaStmtKind, ArenaUserModule, TypeDefId};
use std::sync::{Arc, Mutex};
use super::module_record::{
    CheckedModuleFacts, ModuleRecord, ModuleRecordCache, ModuleRecordKey, ModuleRecords,
};
use super::infer_effects::{EffectDeclarationId, EffectGraph, EffectScopeFailure};

enum FileInferenceFailure {
    Effects(EffectScopeFailure),
    RejectedEffectInvocation(Vec<crate::diagnostic::Diagnostic>),
    StreamOrigin(EffectDeclarationId),
}

impl From<EffectScopeFailure> for FileInferenceFailure {
    fn from(failure: EffectScopeFailure) -> Self { Self::Effects(failure) }
}

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
    cache_trace: Option<CacheTrace>,
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
            cache_trace: CacheTrace::shared(),
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
            ModuleRecordCache::with_shared(self.records.clone(), self.cache_trace.clone())
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
                origin_summaries,
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
            let checked_entry = active_records.root().check_once(active_records.trace(), || {
                #[cfg(test)]
                ENTRY_CHECKS.with(|checks| checks.set(checks.get() + 1));
                let record_options = CheckOptions {
                    reveal_types: true,
                    embedded_bodies: true,
                    ..options
                };
                let effect_probe = Self::solve_file_effect_probes(
                    |provisional, provisional_origins| {
                        let mut probe = Self::session_effect_checker(
                            record_options,
                            byte_script_arguments,
                            &module_signatures,
                            EffectGraph::declarations_for_root(view),
                            summaries.clone(),
                            true,
                        );
                        probe.effect_summaries.extend(provisional);
                        probe.stream_origin_inputs = origin_summaries.clone();
                        probe.stream_origin_inputs.extend(provisional_origins);
                        probe.check_program_arena_with_type_program_and_sources(
                            view,
                            source,
                            type_program.clone(),
                            &active_records,
                        );
                        (probe, ())
                    },
                    &summaries,
                    &origin_summaries,
                );
                let (mut checker, ()) = match effect_probe {
                    Ok(effects) => effects,
                    Err(failure) => return Self::rejected_file_inference(failure),
                };
                checker.validate_deferred_effects();
                checker.validate_stream_expression_origins(program);
                let resource_check = super::resource::check_root_owner(&checker, view);
                checker.diagnostics.extend(resource_check.diagnostics);
                checker.resource_transfers = resource_check.transfers;
                checker.same_scope_resource_assignments = resource_check.same_scope_assignments;
                let facts = checker.effect_graph.facts(&checker.effect_summaries);
                checker.finalize_function_declarations(view.program, &facts);
                checker.publish_command_binding_sites();
                let module_diagnostic_position = checker.module_diagnostic_position;
                let output = checker.into_output();
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
            let mut module_diagnostics = Vec::new();
            let destination = Arc::new(Mutex::new(crate::sema::scheme_callable::SchemeCallableTable::default()));
            let mut relationships = crate::sema::scheme_callable::SchemeCallableRebaser::new(destination);
            if let Err(error) = super::scheme_values::rebase::output(&mut output, &mut relationships) {
                return CheckOutput { diagnostics: vec![crate::diagnostic::Diagnostic::error(format!("checked callable relationship cannot be published: {error:?}")).with_code(crate::diagnostic::DiagnosticCode::CheckLocalInference)], ..CheckOutput::default() };
            }
            let root_annotation_facts = std::mem::take(&mut output.annotation_facts);
            let root_reveal_types = std::mem::take(&mut output.reveal_types);
            for mut module_output in module_outputs {
                module_diagnostics.append(&mut module_output.diagnostics);
                if let Err(error) = super::scheme_values::rebase::output(&mut module_output, &mut relationships) {
                    return CheckOutput { diagnostics: vec![crate::diagnostic::Diagnostic::error(format!("checked module callable relationship cannot be published: {error:?}")).with_code(crate::diagnostic::DiagnosticCode::CheckLocalInference)], ..CheckOutput::default() };
                }
                output.merge_module_facts(module_output);
            }
            output.scheme_callables = relationships.snapshot();
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
        super::stream_origins::StreamOriginSummaries,
    ) {
        let mut signatures = FxHashMap::default();
        let mut summaries = std::collections::BTreeMap::new();
        let mut origin_summaries = super::stream_origins::StreamOriginSummaries::new();
        let mut outputs = FxHashMap::default();
        // Suppressed embedded outputs still need the transfer-site facts used by lowering.
        let mut suppressed_resource_transfers = ResourceTransferFacts::default();
        for module in view.modules.iter().copied() {
            let record = records
                .module(&module.key)
                .expect("reachable module has a prepared record")
                .clone();
            debug_assert_eq!(record.args_domain(), args_domain);
            let checked = record.check_once(records.trace(), || {
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
                    &origin_summaries,
                )
            });
            if !record.is_embedded() {
                if let Some(signature) = &checked.signature { signatures.insert(module.key.clone(), signature.clone()); }
                else { assert!(checked.output.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error), "a rejected module reports a checking failure"); }
            }
            Self::extend_effect_summaries(&mut summaries, &checked.output.function_effect_facts);
            origin_summaries.extend(checked.output.stream_origin_summaries.clone());
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
        (signatures, summaries, outputs, suppressed_resource_transfers, origin_summaries)
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
        dependency_origins: &super::stream_origins::StreamOriginSummaries,
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
        let effect_probe = Self::solve_file_effect_probes(
            |provisional, provisional_origins| {
                let mut probe = Self::session_effect_checker(
                    options,
                    byte_script_arguments,
                    dependencies,
                    EffectGraph::declarations_for_module(&module_view, module),
                    dependency_summaries.clone(),
                    true,
                );
                probe.effect_summaries.extend(provisional);
                probe.stream_origin_inputs = dependency_origins.clone();
                probe.stream_origin_inputs.extend(provisional_origins);
                record.prepared_facts().install(&mut probe, program);
                probe.record_constructors.replace_instance_facts(
                    &bundle_record.prepared_facts().record_constructors,
                );
                probe.diagnostics.clear();
                let signature = probe.check_user_module_arena(
                    &module_view,
                    record,
                    type_program.clone(),
                    source,
                    module,
                );
                (probe, signature)
            },
            dependency_summaries,
            dependency_origins,
        );
        let (mut checker, mut signature) = match effect_probe {
            Ok(effects) => effects,
            Err(failure) => return Self::rejected_file_inference(failure),
        };

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
        checker.validate_stream_expression_origins(program);
        let mut resource_check = super::resource::check(&checker, &module_view);
        let suppressed_body_transfers =
            std::mem::take(&mut resource_check.suppressed_body_transfers);
        checker.diagnostics.extend(resource_check.diagnostics);
        checker.resource_transfers.extend(resource_check.transfers);
        checker.same_scope_resource_assignments.extend(resource_check.same_scope_assignments);
        let facts = checker.effect_graph.facts(&checker.effect_summaries);
        checker.finalize_function_declarations(program, &facts);
        checker.finalize_user_module_functions(program, module, &mut signature);
        checker.publish_command_binding_sites();
        checker.record_constructors = record.prepared_facts().record_constructors.clone();
        let output = checker.into_output();
        signature.scheme_callables = Some(Arc::clone(&output.scheme_callables));
        CheckedModuleFacts {
            signature: Some(signature),
            output,
            module_diagnostic_position: 0,
            suppressed_body_transfers,
        }
    }

    fn rejected_file_inference(failure: FileInferenceFailure) -> CheckedModuleFacts {
        let failure = match failure {
            FileInferenceFailure::Effects(failure) => failure,
            FileInferenceFailure::RejectedEffectInvocation(diagnostics) => return CheckedModuleFacts {
                signature: None,
                output: CheckOutput { diagnostics, ..CheckOutput::default() },
                module_diagnostic_position: 0,
                suppressed_body_transfers: ResourceTransferFacts::default(),
            },
            FileInferenceFailure::StreamOrigin(owner) => {
                let diagnostic = crate::diagnostic::Diagnostic::error("stream return provenance exceeds its checked projection boundary")
                    .with_code(crate::diagnostic::DiagnosticCode::CheckLocalInference)
                    .with_label(crate::diagnostic::Label::primary(owner.body, "stream origin application is not publishable"));
                return CheckedModuleFacts {
                    signature: None,
                    output: CheckOutput { diagnostics: vec![diagnostic], ..CheckOutput::default() },
                    module_diagnostic_position: 0,
                    suppressed_body_transfers: ResourceTransferFacts::default(),
                };
            }
        };
        let note = match failure.cause {
            crate::sema::constraints::SchemeOperationError::Resolution(_) => "an effect relationship escaped its bounded checking problem",
            crate::sema::constraints::SchemeOperationError::Build(_) => "an effect parameter or call substitution is outside its declaration scope",
            crate::sema::constraints::SchemeOperationError::Constraint(_) => "the selected call has incompatible type relationships",
            crate::sema::constraints::SchemeOperationError::Application(_) => "the callable application has no checked invocation relationship",
            crate::sema::constraints::SchemeOperationError::Effects(_) => "the invocation effect relationships cannot be resolved",
            crate::sema::constraints::SchemeOperationError::Operation(_) => "the sealed arithmetic relationship cannot be resolved",
            crate::sema::constraints::SchemeOperationError::Wrapper(_) => "the wrapper relationship cannot be resolved",
        };
        let diagnostic = crate::diagnostic::Diagnostic::error("effect contract cannot be bound to its checked declaration")
            .with_code(crate::diagnostic::DiagnosticCode::CheckLocalInference)
            .with_label(crate::diagnostic::Label::primary(failure.owner.body, "effect contract is not publishable"))
            .with_note(note);
        CheckedModuleFacts {
            signature: None,
            output: CheckOutput { diagnostics: vec![diagnostic], ..CheckOutput::default() },
            module_diagnostic_position: 0,
            suppressed_body_transfers: ResourceTransferFacts::default(),
        }
    }

    fn failed_effect_scope(&self, failure: EffectScopeFailure) -> FileInferenceFailure {
        if matches!(failure.cause, crate::sema::constraints::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedEffects))
            && let Some(diagnostics) = self.rejected_effect_invocation_diagnostics(failure.owner)
        { FileInferenceFailure::RejectedEffectInvocation(diagnostics) }
        else { FileInferenceFailure::Effects(failure) }
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

    fn solve_file_effect_probes<T>(
        mut probe: impl FnMut(
            std::collections::BTreeMap<EffectDeclarationId, EffectSummary>,
            super::stream_origins::StreamOriginSummaries,
        ) -> (Self, T),
        external: &std::collections::BTreeMap<EffectDeclarationId, EffectSummary>,
        external_origins: &super::stream_origins::StreamOriginSummaries,
    ) -> Result<(Self, T), FileInferenceFailure> {
        let mut provisional = std::collections::BTreeMap::new();
        let mut provisional_origins = super::stream_origins::StreamOriginSummaries::new();
        loop {
            let (mut checker, output) = probe(provisional.clone(), provisional_origins.clone());
            if let Err(failure) = checker.bind_effect_graph() {
                return Err(checker.failed_effect_scope(failure));
            }
            let solved = checker.effect_graph.solve_with_external(external)?;
            checker.normalize_stream_origin_applications()
                .map_err(|(owner, _)| FileInferenceFailure::StreamOrigin(owner))?;
            checker.validate_published_stream_origins()
                .map_err(|(owner, _)| FileInferenceFailure::StreamOrigin(owner))?;
            checker.settle_stream_origin_dependencies(external_origins);
            if (!checker.provisional_effects_read.get() || solved == provisional)
                && (!checker.provisional_stream_origins_read.get() || checker.stream_origin_outputs == provisional_origins)
            {
                checker.effect_summaries.extend(solved);
                checker.stream_origin_inputs = external_origins.clone();
                checker.stream_origin_inputs.extend(checker.stream_origin_outputs.clone());
                return Ok((checker, output));
            }
            provisional = solved;
            provisional_origins = checker.stream_origin_outputs;
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
        checker.record_constructors = RecordConstructors::collect_declarations_bundle(view);
        checker.collect_type_imports_arena(program, view.statement_ids());
        checker.collect_definitions_arena(
            program,
            type_program,
            source,
            view.statement_ids(),
        );
        checker.procs.get(&super::Name::intern("main")).is_some_and(|declaration| {
            checker.declaration_inference.pending(*declaration)
                .and_then(|signature| signature.provisional())
                .is_some_and(|shape| {
                    matches!(shape.params(), [parameter]
                        if parameter.signature.rest && !parameter.signature.defaulted
                            && matches!(&parameter.signature.ty,
                                super::Type::List(item) if item.as_ref() == &super::Type::Bytes))
                })
        })
    }

    fn extend_effect_summaries(
        summaries: &mut std::collections::BTreeMap<EffectDeclarationId, EffectSummary>,
        facts: &std::collections::BTreeMap<EffectDeclarationId, FunctionEffectFact>,
    ) {
        summaries.extend(facts.iter().map(|(&id, fact)| {
            (
                id,
                EffectSummary::from_effect_fact(id, fact).expect("checked module facts retain their declaration effect scope"),
            )
        }));
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::check::CheckOptions;
    use crate::sema::check::decl::USER_MODULE_CHECKS;
    use crate::sema::constraints::{ConstraintType, TypeConstraints};
    use crate::sema::inference_work;
    use crate::sema::schemes::{SchemeRowTail, SchemeType, TypeScheme};
    use crate::sema::types::Type;
    use crate::source::Span;
    use crate::syntax::arena::{ArenaProgramBuilder, ArenaRange};
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

    struct TypingWorkspace {
        program: Arc<ArenaProgram>,
        sources: Arc<SourceMap>,
        roots: Vec<(ArenaRange, SourceId)>,
        module_source: SourceId,
    }

    fn check_imported_effect_union(body: &str) -> CheckOutput {
        let source = format!("use effects\nlet selected = effects.pair\nproc clock(value: Int) [time] -> Int {{ let _ = time.now(); value }}\nproc emit(value: Int) [io] -> Int {{ print \"callback\"; value }}\n{body}\n");
        let module = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/modules/full-inference-effect-union/effects.xsh"));
        let mut sources = SourceMap::new();
        let mut builder = ArenaProgramBuilder::with_token_capacity(256);
        let source_id = sources.add_file("effect-bound.xsh", source.clone());
        let parsed = Parser::parse_source_into_arena_builder(source_id, &source, &mut builder);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let module_id = sources.add_file("effects.xsh", module);
        let imported = Parser::parse_source_into_arena_builder(module_id, module, &mut builder);
        assert!(imported.diagnostics.is_empty(), "{:?}", imported.diagnostics);
        let name = builder.symbol_owner().intern("effects");
        builder.push_arena_module("effects".to_owned(), name, imported.statements);
        for statement in builder.statement_ids(parsed.statements) {
            if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) { builder.set_use_resolved(use_id, Arc::from("effects")); }
        }
        let program = Arc::new(builder.finish_with_statements(parsed.statements));
        CheckSession::new(program, Arc::new(sources)).check(parsed.statements, source_id, CheckOptions::for_execution())
    }

    #[test]
    fn inferred_row_alias_environment_effects_bind_to_their_declaration() {
        let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/runtime/guarded-row-null-input.xsh"));
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("guarded-row-null-input.xsh", source.to_owned());
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let output = Checker::check_arena_with_sources_and_options(&parsed.arena, &sources, source_id, CheckOptions::for_check_cli());
        assert!(output.diagnostics.iter().all(|diagnostic| diagnostic.severity != crate::diagnostic::Severity::Error), "{:?}", output.diagnostics);
        for invalid in [
            source.replace("spec: 1", "spec: true"),
            format!("{source}\nlet _ = execute(true)\n"),
            source.replace("spec: 1", "spec: missing"),
        ] {
            let parsed = Parser::parse_source_arena_only(source_id, &invalid);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let output = Checker::check_arena_with_options(&parsed.arena, &invalid, CheckOptions::for_check_cli());
            assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error), "{invalid}\n{:?}", output.diagnostics);
        }
    }

    #[cfg(feature = "native-tests")]
    #[test]
    fn guarded_row_null_inputs_execute_after_frontend_disposal_on_both_routes() {
        std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
            let source = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/runtime/guarded-row-null-input.xsh"));
            let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only("guarded-row-null-input.xsh", crate::loader::entry_source_from_text("guarded-row-null-input.xsh", source.to_owned()), Vec::new());
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let source_id = parsed.arena.source_text_source_id().unwrap();
            let checked = Checker::check_compact_declarations(&parsed.arena, &sources, source_id);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let prepared = crate::runtime::eval::Evaluator::new_with_sources(Vec::new(), sources).prepare_test_program(Arc::new(parsed.arena), source_id, checked).unwrap();
            for force_frames in [false, true] {
                let result = prepared.eval_test("verify_guarded_row_null_input", crate::runtime::value::Value::Unit, false, force_frames, Vec::new(), crate::runtime::eval::TestCancellation::default());
                assert_eq!(result.output.status, 0, "{:?} {:?}", result.output.diagnostics, result.output.traceback);
                assert!(result.output.diagnostics.is_empty(), "{:?}", result.output.diagnostics);
                assert!(result.output.traceback.is_none(), "{:?}", result.output.traceback);
                assert_eq!(result.output.stdout, b"1\n2\n3\n4\n");
                assert!(result.output.stderr.is_empty(), "{:?}", result.output.stderr);
                assert!(matches!(result.result, Some(crate::runtime::value::Value::Result(crate::runtime::value::ResultValue::Ok(value))) if matches!(value.as_ref(), crate::runtime::value::Value::Unit)));
            }
        }).unwrap().join().unwrap();
    }

    #[test]
    fn imported_effect_union_missing_bound_reports_the_actual_capability() {
        let checked = check_imported_effect_union("proc denied() [io, error] { let _ = selected(clock, emit, 7)? }");
        let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error).collect::<Vec<_>>();
        assert!(!errors.is_empty(), "missing Time must reject");
        assert!(errors.iter().all(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckEffectViolation)), "{errors:?}");
        assert!(errors.iter().any(|diagnostic| diagnostic.labels.iter().any(|label| label.style == crate::diagnostic::LabelStyle::Secondary && label.message.as_deref().is_some_and(|message| message.contains("time")))), "{errors:?}");
    }

    #[test]
    fn imported_effect_union_lexical_bound_reports_the_actual_capability() {
        let checked = check_imported_effect_union("proc denied() [time, io, error] { without time { let _ = selected(clock, emit, 7)? } }");
        let errors = checked.diagnostics.iter().filter(|diagnostic| diagnostic.severity == crate::diagnostic::Severity::Error).collect::<Vec<_>>();
        assert!(!errors.is_empty(), "excluded Time must reject");
        assert!(errors.iter().all(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckEffectViolation)), "{errors:?}");
        assert!(errors.iter().any(|diagnostic| diagnostic.labels.iter().any(|label| label.style == crate::diagnostic::LabelStyle::Secondary && label.message.as_deref().is_some_and(|message| message.contains("without time")))), "{errors:?}");
    }

    #[test]
    fn imported_effect_union_rejection_retains_independent_errors_without_facts() {
        let checked = check_imported_effect_union("proc denied() [io, error] { let _ = selected(clock, emit, 7)? }\nlet invalid: Int = \"wrong\"");
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckEffectViolation)), "{:?}", checked.diagnostics);
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckTypeMismatch)), "{:?}", checked.diagnostics);
        assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckLocalInference)), "{:?}", checked.diagnostics);
        assert!(!checked.checked_declarations.functions.values().any(|function| function.name == "denied"));
    }

    #[test]
    fn imported_effect_union_preserves_successful_promised_bounds() {
        let checked = check_imported_effect_union("proc allowed() [time, io, error] { let _ = selected(clock, emit, 7)? }");
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert!(checked.checked_declarations.functions.values().any(|function| function.name == "allowed"));
    }

    #[test]
    fn imported_effect_union_keeps_unknown_dynamic_callbacks_rejected() {
        let checked = check_imported_effect_union("proc denied(callback: Proc) [] -> Int { let _ = callback.call(); 42 }");
        assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckEffectViolation)), "{:?}", checked.diagnostics);
        assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckLocalInference)), "{:?}", checked.diagnostics);
    }

    #[test]
    fn imported_effect_union_unrelated_errors_do_not_hide_binding_faults() {
        use crate::diagnostic::{Diagnostic, DiagnosticCode, Label};
        for code in [DiagnosticCode::CheckTypeMismatch, DiagnosticCode::CheckEffectViolation] {
            let mut checker = Checker::new(CheckOptions::for_execution());
            let owner = EffectDeclarationId { namespace: None, body: Span::new(SourceId::new(0), 0, 20) };
            checker.diagnostics.push(Diagnostic::error("independent checker failure").with_code(code).with_label(Label::primary(owner.body, "independent")));
            let failure = EffectScopeFailure { owner, cause: crate::sema::constraints::SchemeOperationError::Build(crate::sema::schemes::SchemeBuildError::UnresolvedEffects) };
            let rejected = Checker::rejected_file_inference(checker.failed_effect_scope(failure));
            assert!(rejected.signature.is_none());
            assert!(rejected.output.checked_declarations.functions.is_empty());
            assert_eq!(rejected.output.diagnostics.len(), 1);
            assert_eq!(rejected.output.diagnostics[0].code, Some(DiagnosticCode::CheckLocalInference));
        }
    }

    fn typing_workspace(root_sources: &[&str]) -> TypingWorkspace {
        assert!(!root_sources.is_empty());
        let module_text = concat!(
            "##! Shared generic functions.\n",
            "## Returns its supplied value.\n",
            "export pure identity(value) { value }\n",
            "## Return the required name field.\n",
            "export pure name_of(value) { value.name }\n",
            "## Return a boolean literal.\n",
            "export pure false_value() { false }\n",
            "## Return an explicit dynamic value.\n",
            "export pure explicit_any(value: Any) -> Any { value }\n",
        );
        let mut sources = SourceMap::new();
        let mut builder = ArenaProgramBuilder::with_token_capacity(256);
        let mut roots = Vec::with_capacity(root_sources.len());
        for (index, text) in root_sources.iter().enumerate() {
            let source_id = sources.add_file(format!("root-{index}.xsh"), *text);
            let parsed =
                Parser::parse_source_into_arena_builder(source_id, text, &mut builder);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            roots.push((parsed.statements, source_id));
        }
        let module_source = sources.add_file("shared.xsh", module_text);
        let module = Parser::parse_source_into_arena_builder(
            module_source,
            module_text,
            &mut builder,
        );
        assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
        let module_name = builder.symbol_owner().intern("shared");
        builder.push_arena_module("shared".to_string(), module_name, module.statements);
        for &(statements, _) in &roots {
            for statement in builder.statement_ids(statements) {
                if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
                    builder.set_use_resolved(use_id, Arc::from("shared"));
                }
            }
        }
        let program = Arc::new(builder.finish_with_statements(roots[0].0));
        TypingWorkspace {
            program,
            sources: Arc::new(sources),
            roots,
            module_source,
        }
    }

    fn checked_module_facts(
        session: &CheckSession,
        module_source: SourceId,
    ) -> Arc<CheckedModuleFacts> {
        let record = session
            .records
            .lock()
            .expect("module record cache poisoned")
            .values()
            .find(|record| record.source_id() == module_source)
            .expect("root prepared the shared module")
            .clone();
        record.check_once(None, || unreachable!("module check is cached"))
    }

    #[test]
    fn scheme_callable_nested_module_cache_rebases_fresh_entries_without_mutating_sources() {
        let texts = [
            ("first.xsh", "use bridge\nassert bridge.actions.apply(7) == 7\n"),
            ("second.xsh", "use bridge\nassert bridge.actions.apply(\"demo\") == \"demo\"\n"),
            ("bridge.xsh", "##! Forward callable data.\nuse leaf\n## Retained operations.\nexport let actions = leaf.actions\n"),
            ("leaf.xsh", "##! Callable data.\n## Copy a value.\nexport pure identity(value) { value }\n## Retained operations.\nexport let actions = {apply: identity}\n"),
        ];
        let mut sources = SourceMap::new();
        let mut builder = ArenaProgramBuilder::with_token_capacity(256);
        let mut parsed = Vec::new();
        for (path, text) in texts {
            let source = sources.add_file(path, text);
            let result = Parser::parse_source_into_arena_builder(source, text, &mut builder);
            assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
            parsed.push((result.statements, source));
        }
        for (index, name) in [(2, "bridge"), (3, "leaf")] {
            let symbol = builder.symbol_owner().intern(name);
            builder.push_arena_module(name.to_string(), symbol, parsed[index].0);
        }
        for (index, name) in [(0, "bridge"), (1, "bridge"), (2, "leaf")] {
            for statement in builder.statement_ids(parsed[index].0) {
                if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
                    builder.set_use_resolved(use_id, Arc::from(name));
                }
            }
        }
        let actions_name = builder.symbol_owner().intern("actions");
        let apply_name = builder.symbol_owner().intern("apply");
        let program = Arc::new(builder.finish_with_statements(parsed[0].0));
        let session = CheckSession::new(Arc::clone(&program), Arc::new(sources));
        USER_MODULE_CHECKS.with(|checks| checks.set(0));
        let first = session.check(parsed[0].0, parsed[0].1, CheckOptions::for_execution());
        assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
        assert_eq!(USER_MODULE_CHECKS.with(std::cell::Cell::get), 2);
        let leaf = checked_module_facts(&session, parsed[3].1);
        let bridge = checked_module_facts(&session, parsed[2].1);
        let exported_handle = |facts: &CheckedModuleFacts| {
            let signature = facts.signature.as_ref().unwrap();
            assert!(Arc::ptr_eq(signature.scheme_callables.as_ref().unwrap(), &facts.output.scheme_callables));
            let Type::Record(fields) = &signature.values[&actions_name] else { panic!("the export retains its record type"); };
            let Type::SchemeCallable(handle) = fields[&apply_name] else { panic!("the record leaf retains its scheme"); };
            assert!(facts.output.scheme_callables.get(handle).is_ok());
            handle
        };
        let leaf_handle = exported_handle(&leaf);
        let bridge_handle = exported_handle(&bridge);
        assert_ne!(leaf_handle, bridge_handle);
        let second = session.check(parsed[1].0, parsed[1].1, CheckOptions::for_execution());
        assert!(second.diagnostics.is_empty(), "{:?}", second.diagnostics);
        assert_eq!(USER_MODULE_CHECKS.with(std::cell::Cell::get), 2, "both imported module checks are reused");
        assert!(Arc::ptr_eq(&leaf, &checked_module_facts(&session, parsed[3].1)));
        assert!(Arc::ptr_eq(&bridge, &checked_module_facts(&session, parsed[2].1)));
        assert_eq!(exported_handle(&leaf), leaf_handle);
        assert_eq!(exported_handle(&bridge), bridge_handle);
        let entry_handle = |output: &CheckOutput, source| {
            *output.scheme_callable_calls.iter().find(|(site, _)| site.source.source_id == source
                && site.role == crate::sema::schemes::InstantiationRole::Call).unwrap().1
        };
        let first_handle = entry_handle(&first, parsed[0].1);
        let second_handle = entry_handle(&second, parsed[1].1);
        assert_ne!(first_handle, second_handle);
        assert!(first.scheme_callables.get(first_handle).is_ok());
        assert!(second.scheme_callables.get(second_handle).is_ok());
        assert_eq!(first.scheme_callables.get(second_handle).unwrap_err(), crate::sema::scheme_callable::SchemeCallableError::ForeignOwner);
        assert_eq!(second.scheme_callables.get(first_handle).unwrap_err(), crate::sema::scheme_callable::SchemeCallableError::ForeignOwner);
    }

    fn exported_scheme(facts: &CheckedModuleFacts, name: &str) -> Arc<TypeScheme> {
        facts
            .signature
            .as_ref()
            .expect("checked module signature")
            .pures
            .get(&crate::symbol::Name::intern(name))
            .unwrap_or_else(|| panic!("missing exported pure function `{name}`"))
            .scheme()
            .clone()
    }

    fn assert_no_inference_in_scheme_type(ty: &SchemeType) {
        match ty {
            SchemeType::Ground(ty) => assert!(!ty.contains_inference(), "{ty:?}"),
            SchemeType::SchemeCallable(_) | SchemeType::Parameter(_) => {}
            SchemeType::Callable { monotype, .. } => {
                for parameter in monotype.params() {
                    assert_no_inference_in_scheme_type(&parameter.ty);
                }
                assert_no_inference_in_scheme_type(monotype.return_type());
            }
            SchemeType::List(inner)
            | SchemeType::Stream(inner)
            | SchemeType::Set(inner)
            | SchemeType::Optional(inner)
            | SchemeType::Validated(_, inner) => assert_no_inference_in_scheme_type(inner),
            SchemeType::Map(key, value) | SchemeType::Result(key, value) => {
                assert_no_inference_in_scheme_type(key);
                assert_no_inference_in_scheme_type(value);
            }
            SchemeType::Record { fields, .. } => {
                for field in fields.values() {
                    assert_no_inference_in_scheme_type(field);
                }
            }
            SchemeType::Union(members) => {
                for member in members {
                    assert_no_inference_in_scheme_type(member);
                }
            }
        }
    }

    fn assert_no_inference_in_output(output: &CheckOutput) {
        for ty in output
            .local_binding_types
            .values()
            .chain(output.function_return_types.values())
            .chain(output.parameter_types.values())
            .chain(output.expr_types.values())
        {
            assert!(!ty.contains_inference(), "{ty:?}");
        }
        for instantiation in output.call_instantiations.values().flat_map(|evidence| evidence.instances()) {
            for ty in instantiation.type_arguments().values() {
                assert_no_inference_in_scheme_type(ty);
            }
            for ty in instantiation
                .row_arguments()
                .values()
                .flat_map(|row| row.fields().values())
            {
                assert_no_inference_in_scheme_type(ty);
            }
        }
        for declaration in output.checked_declarations.functions.values() {
            if let Some(scheme) = declaration.scheme.as_ref() {
                for parameter in scheme.params() {
                    assert_no_inference_in_scheme_type(&parameter.ty);
                }
                assert_no_inference_in_scheme_type(scheme.return_type());
            }
            if let Some(facts) = declaration.scheme_body_facts.as_ref() {
                for ty in facts
                    .expression_types()
                    .values()
                    .chain(facts.tail_types().values())
                {
                    assert_no_inference_in_scheme_type(ty);
                }
            }
        }
    }

    #[test]
    fn repeated_generic_calls_add_call_facts_without_increasing_final_checker_body_entries() {
        let check = |call_count: usize| {
            let calls = (0..call_count)
                .map(|value| format!("identity({value})"))
                .collect::<Vec<_>>()
                .join(" + ");
            let source = format!(
                "pure identity(value) {{ value }}\npure exercise_calls() -> Int {{ {calls} }}\n"
            );
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let program = &parsed.arena;
            program.symbol_owner().with_current(|| {
                let (work, output) = inference_work::collect(|| {
                    Checker::check_arena_with_options(
                        program,
                        &source,
                        CheckOptions::for_execution(),
                    )
                });
                assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                let identity = output.checked_declarations.functions.values()
                    .find(|declaration| declaration.name.as_str().as_str() == "identity")
                    .expect("the original identity declaration is published");
                let scheme = identity.scheme.as_ref().expect("the original body has a checked scheme");
                assert_eq!(scheme.params()[0].ty, *scheme.return_type());
                assert!(matches!(scheme.return_type(), SchemeType::Parameter(_)));
                let body = identity.scheme_body_facts.as_ref().expect("the original body retains its proof");
                assert!(body.tail_types().values().any(|ty| ty == scheme.return_type()));
                let body_entries = output.function_body_check_entries;
                let call_facts = output.call_instantiations.len();
                let declarations = Checker::compact_declarations(program, output);
                let lowered = crate::runtime::eval::probe_compact_lower_constructed_bodies(
                    program,
                    &declarations,
                    &source,
                );
                (
                    work,
                    body_entries,
                    call_facts,
                    lowered.functions,
                    lowered.constructed_functions,
                )
            })
        };

        let (one_work, one_body_checks, one_call_facts, one_functions, one_constructed) = check(1);
        let (many_work, many_body_checks, many_call_facts, many_functions, many_constructed) =
            check(8);
        assert_eq!(one_call_facts, 1);
        assert_eq!(many_call_facts, 8);
        assert_eq!(one_body_checks, many_body_checks);
        assert_eq!(one_body_checks, 2);
        assert_eq!(one_work.function_body_check_entries, 2);
        assert_eq!(
            one_work.function_body_check_entries,
            many_work.function_body_check_entries
        );
        assert!(many_work.scheme_instantiations > one_work.scheme_instantiations);
        assert_eq!(one_functions, 2);
        assert_eq!(many_functions, one_functions);
        assert_eq!(one_constructed, 2);
        assert_eq!(many_constructed, one_constructed);
    }

    #[test]
    fn argument_domain_probe_resolves_entry_and_imported_nominal_signatures() {
        let entry_source = concat!(
            "use model\n",
            "nominal type LocalPackage = {id: Str}\n",
            "pure local_id(package: LocalPackage) -> Str { package.id }\n",
            "pure imported_id(package: model.Package) -> Str { package.id }\n",
            "proc main(...argv: List[Bytes]) {}\n",
        );
        let module_source = concat!(
            "##! Package types.\n",
            "## A package identity owned by this module.\n",
            "export nominal type Package = {id: Str}\n",
        );
        let mut sources = SourceMap::new();
        let entry_source_id = sources.add_file("entry.xsh", entry_source);
        let module_source_id = sources.add_file("model.xsh", module_source);
        let mut builder = ArenaProgramBuilder::with_token_capacity(96);
        let entry = Parser::parse_source_into_arena_builder(
            entry_source_id,
            entry_source,
            &mut builder,
        );
        let module = Parser::parse_source_into_arena_builder(
            module_source_id,
            module_source,
            &mut builder,
        );
        assert!(entry.diagnostics.is_empty(), "{:?}", entry.diagnostics);
        assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
        let model_name = builder.symbol_owner().intern("model");
        builder.push_arena_module("model".to_string(), model_name, module.statements);
        for statement in builder.statement_ids(entry.statements) {
            if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
                builder.set_use_resolved(use_id, Arc::from("model"));
            }
        }
        let program = Arc::new(builder.finish_with_statements(entry.statements));
        program.symbol_owner().with_current(|| {
            let view = CheckBundleView::for_root(
                &program,
                entry.statements,
                Some(&sources),
                Some(entry_source_id),
            );
            let options = CheckOptions::default();
            let modules = Checker::module_type_headers(&view, options, Arc::clone(&program));

            assert!(Checker::entry_uses_byte_arguments(
                &view,
                entry_source,
                options,
                Arc::clone(&program),
                &modules,
            ));
        });
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
        assert_eq!(USER_MODULE_CHECKS.with(std::cell::Cell::get), 1);

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
            fact.effective.concrete().expect("finite checked effect fixture")
                .is_some_and(|effects| effects.contains(&crate::syntax::node::Effect::Time))
        }));
        assert!(second_output.function_effect_facts.values().any(|fact| {
            fact.effective.concrete().expect("finite checked effect fixture")
                .is_some_and(|effects| effects.contains(&crate::syntax::node::Effect::Time))
        }));
        let byte_output = session.check(byte.statements, byte_source, CheckOptions::for_execution());
        assert!(byte_output.diagnostics.is_empty(), "{:?}", byte_output.diagnostics);
        assert_eq!(USER_MODULE_CHECKS.with(std::cell::Cell::get), 2);
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
    fn imported_open_row_scheme_instantiates_independently_across_roots() {
        let first_text = "use shared\nlet package: Str = shared.name_of({name: \"package\", version: \"1.0\"})\n";
        let second_text = "use shared\nlet count: Int = shared.name_of({name: 42, enabled: true})\n";
        let module_text = concat!(
            "##! Shared open record projection.\n",
            "## Return the required name field.\n",
            "export pure name_of(value) { value.name }\n",
        );
        let mut sources = SourceMap::new();
        let first_source = sources.add_file("first.xsh", first_text);
        let second_source = sources.add_file("second.xsh", second_text);
        let module_source = sources.add_file("shared.xsh", module_text);
        let mut builder = ArenaProgramBuilder::with_token_capacity(128);
        let first = Parser::parse_source_into_arena_builder(first_source, first_text, &mut builder);
        let second =
            Parser::parse_source_into_arena_builder(second_source, second_text, &mut builder);
        let shared =
            Parser::parse_source_into_arena_builder(module_source, module_text, &mut builder);
        assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
        assert!(second.diagnostics.is_empty(), "{:?}", second.diagnostics);
        assert!(shared.diagnostics.is_empty(), "{:?}", shared.diagnostics);
        let shared_name = builder.symbol_owner().intern("shared");
        builder.push_arena_module("shared".to_string(), shared_name, shared.statements);
        for root in [first.statements, second.statements] {
            for statement in builder.statement_ids(root) {
                if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
                    builder.set_use_resolved(use_id, Arc::from("shared"));
                }
            }
        }
        let program = Arc::new(builder.finish_with_statements(first.statements));
        let _symbols = program.symbol_owner().enter();
        let sources = Arc::new(sources);
        USER_MODULE_CHECKS.with(|checks| checks.set(0));
        let session = CheckSession::new(Arc::clone(&program), sources);
        let first_output = session.check(first.statements, first_source, CheckOptions::default());
        let first_record = session
            .records
            .lock()
            .expect("module record cache poisoned")
            .values()
            .find(|record| record.source_id() == module_source)
            .expect("first root prepared the shared module")
            .clone();
        let first_checked = first_record.check_once(None, || unreachable!("module check is cached"));
        let second_output =
            session.check(second.statements, second_source, CheckOptions::default());
        let second_record = session
            .records
            .lock()
            .expect("module record cache poisoned")
            .values()
            .find(|record| record.source_id() == module_source)
            .expect("second root retained the shared module")
            .clone();
        let second_checked =
            second_record.check_once(None, || unreachable!("module check is cached"));

        assert!(first_output.diagnostics.is_empty(), "{:?}", first_output.diagnostics);
        assert!(second_output.diagnostics.is_empty(), "{:?}", second_output.diagnostics);
        assert_eq!(USER_MODULE_CHECKS.with(std::cell::Cell::get), 1);
        assert!(Arc::ptr_eq(&first_record, &second_record));
        assert!(Arc::ptr_eq(&first_checked, &second_checked));
        let first_scheme = first_checked
            .signature
            .as_ref()
            .expect("shared module signature")
            .pures
            .values()
            .next()
            .expect("exported projection scheme")
            .scheme();
        let second_scheme = second_checked
            .signature
            .as_ref()
            .expect("shared module signature")
            .pures
            .values()
            .next()
            .expect("exported projection scheme")
            .scheme();
        assert!(Arc::ptr_eq(first_scheme, second_scheme));
        assert_eq!(first_scheme.type_parameters().len(), 1);
        assert_eq!(first_scheme.row_parameters().len(), 1);

        let call_span = |source_id, source: &str, call: &str| {
            let start = source.find(call).expect("projection call expression");
            crate::source::Span::new(source_id, start, start + call.len())
        };
        let first_call = "shared.name_of({name: \"package\", version: \"1.0\"})";
        let second_call = "shared.name_of({name: 42, enabled: true})";
        let first_instantiation = first_output
            .call_instantiations
            .get(&crate::sema::schemes::InstantiationSite::call(call_span(first_source, first_text, first_call)))
            .expect("first root has a checked call instantiation").invocation().expect("the first root site is a call");
        let second_instantiation = second_output
            .call_instantiations
            .get(&crate::sema::schemes::InstantiationSite::call(call_span(second_source, second_text, second_call)))
            .expect("second root has a checked call instantiation").invocation().expect("the second root site is a call");
        assert_eq!(
            first_instantiation.type_arguments().values().collect::<Vec<_>>(),
            [&SchemeType::Ground(crate::sema::check::Type::Str)]
        );
        assert_eq!(
            second_instantiation.type_arguments().values().collect::<Vec<_>>(),
            [&SchemeType::Ground(crate::sema::check::Type::Int)]
        );
        let first_row = first_instantiation
            .row_arguments()
            .values()
            .next()
            .expect("first root has open-row evidence")
            .fields();
        let second_row = second_instantiation
            .row_arguments()
            .values()
            .next()
            .expect("second root has open-row evidence")
            .fields();
        assert_eq!(first_row.len(), 1);
        assert_eq!(
            first_row.get(&program.symbol_owner().intern("version")),
            Some(&SchemeType::Ground(crate::sema::check::Type::Str))
        );
        assert_eq!(second_row.len(), 1);
        assert_eq!(
            second_row.get(&program.symbol_owner().intern("enabled")),
            Some(&SchemeType::Ground(crate::sema::check::Type::Bool))
        );
    }

    #[test]
    fn checked_identity_scheme_is_rank1_and_reuses_one_binder() {
        let workspace = typing_workspace(&[
            "use shared\nlet value: Int = shared.identity(42)\n",
        ]);
        let _symbols = workspace.program.symbol_owner().enter();
        let session = CheckSession::new(
            Arc::clone(&workspace.program),
            Arc::clone(&workspace.sources),
        );
        let (root, source) = workspace.roots[0];
        let output = session.check(root, source, CheckOptions::default());
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);

        let facts = checked_module_facts(&session, workspace.module_source);
        let identity = exported_scheme(&facts, "identity");
        let [binder] = identity.type_parameters() else {
            panic!("identity has exactly one type binder: {identity:?}");
        };
        assert!(identity.row_parameters().is_empty());
        assert_eq!(identity.params().len(), 1);
        assert_eq!(
            identity.params()[0].ty,
            SchemeType::Parameter(*binder)
        );
        assert_eq!(identity.return_type(), &SchemeType::Parameter(*binder));

        let checked_identity = facts
            .output
            .checked_declarations
            .functions
            .values()
            .find(|declaration| declaration.name.as_str() == "identity")
            .expect("checked exported identity");
        assert!(checked_identity.exported);
        assert!(Arc::ptr_eq(
            checked_identity.scheme.as_ref().expect("checked identity scheme"),
            &identity,
        ));

        let explicit_any = exported_scheme(&facts, "explicit_any");
        assert!(explicit_any.type_parameters().is_empty());
        assert!(explicit_any.row_parameters().is_empty());
        assert_eq!(
            explicit_any.params()[0].ty,
            SchemeType::Ground(Type::Any)
        );
        assert_eq!(
            explicit_any.return_type(),
            &SchemeType::Ground(Type::Any)
        );

        let false_value = exported_scheme(&facts, "false_value");
        assert!(false_value.type_parameters().is_empty());
        assert!(false_value.row_parameters().is_empty());
        assert!(false_value.params().is_empty());
        assert_eq!(
            false_value.return_type(),
            &SchemeType::Ground(Type::Bool)
        );
        let name_of = exported_scheme(&facts, "name_of");
        let [type_binder] = name_of.type_parameters() else {
            panic!("name_of has exactly one type binder: {name_of:?}");
        };
        let [row_binder] = name_of.row_parameters() else {
            panic!("name_of has exactly one row binder: {name_of:?}");
        };
        assert_eq!(name_of.params().len(), 1);
        let SchemeType::Record { fields, tail } = &name_of.params()[0].ty else {
            panic!("name_of accepts a record parameter: {name_of:?}");
        };
        let name_field = crate::symbol::Name::intern("name");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields.get(&name_field), Some(&SchemeType::Parameter(*type_binder)));
        assert_eq!(*tail, SchemeRowTail::Parameter(row_binder.id));
        assert_eq!(name_of.return_type(), &SchemeType::Parameter(*type_binder));
        assert!(row_binder.lacks.contains(&name_field));
    }

    #[test]
    fn inference_work_counts_imported_module_body_checks() {
        let workspace = typing_workspace(&[
            "use shared\npure exercise() -> Int { shared.identity(42) }\n",
        ]);
        let _symbols = workspace.program.symbol_owner().enter();
        let session = CheckSession::new(
            Arc::clone(&workspace.program),
            Arc::clone(&workspace.sources),
        );
        let (root, source) = workspace.roots[0];
        let (work, output) = inference_work::collect(|| {
            session.check(root, source, CheckOptions::default())
        });

        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert!(output.function_body_check_entries > 1);
        assert!(work.function_body_check_entries >= output.function_body_check_entries as u64);
        assert!(work.fresh_variables > 0);
    }

    #[test]
    fn scheme_instantiation_is_fresh_and_does_not_escape() {
        let workspace = typing_workspace(&[
            "use shared\nlet number: Int = shared.identity(42)\nlet text: Str = shared.identity(\"demo\")\n",
        ]);
        let _symbols = workspace.program.symbol_owner().enter();
        let session = CheckSession::new(
            Arc::clone(&workspace.program),
            Arc::clone(&workspace.sources),
        );
        let (root, source) = workspace.roots[0];
        let output = session.check(root, source, CheckOptions::default());
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(output.call_instantiations.len(), 2);
        assert_no_inference_in_output(&output);

        let facts = checked_module_facts(&session, workspace.module_source);
        assert_no_inference_in_output(&facts.output);
        let scheme = exported_scheme(&facts, "identity");
        let [binder] = scheme.type_parameters() else {
            panic!("identity has exactly one type binder: {scheme:?}");
        };
        let origin = Span::new(source, 0, 0);
        let mut constraints = TypeConstraints::default();
        let integer = constraints
            .instantiate_scheme(&scheme, 1, origin)
            .expect("integer identity instantiation");
        let text = constraints
            .instantiate_scheme(&scheme, 1, origin)
            .expect("text identity instantiation");
        assert_eq!(integer.params.len(), 1);
        assert_eq!(text.params.len(), 1);
        assert_eq!(integer.params[0], integer.return_type);
        assert_eq!(text.params[0], text.return_type);
        assert_eq!(integer.type_arguments.get(binder), Some(&integer.params[0]));
        assert_eq!(text.type_arguments.get(binder), Some(&text.params[0]));
        let Type::Inference(integer_id) = &integer.params[0] else {
            panic!("an ununified scheme instance is an inference variable");
        };
        let Type::Inference(text_id) = &text.params[0] else {
            panic!("an ununified scheme instance is an inference variable");
        };
        assert_ne!(integer_id, text_id, "calls get fresh type variables");

        constraints
            .constrain_types(
                &ConstraintType::Variable(*integer_id),
                &ConstraintType::Ground(Type::Int),
                origin,
            )
            .expect("integer use specializes its own instance");
        constraints
            .constrain_types(
                &ConstraintType::Variable(*text_id),
                &ConstraintType::Ground(Type::Str),
                origin,
            )
            .expect("text use specializes its own instance");
        assert_eq!(constraints.resolve(&integer.return_type), Ok(Type::Int));
        assert_eq!(constraints.resolve(&text.return_type), Ok(Type::Str));

        let arguments = output
            .call_instantiations
            .values()
            .flat_map(|evidence| evidence.instances())
            .flat_map(|instantiation| instantiation.type_arguments().values())
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(arguments.len(), 2);
        assert!(arguments.contains(&SchemeType::Ground(Type::Int)));
        assert!(arguments.contains(&SchemeType::Ground(Type::Str)));
    }

    #[test]
    fn imported_generic_module_callables_survive_constant_projections() {
        let source = concat!(
            "use shared as models\n",
            "const identity_name = \"identity\"\n",
            "let by_field = models.identity\n",
            "let by_get = models.get(identity_name)?\n",
            "let by_index = models[identity_name]\n",
            "let integer: Int = by_field(42)\n",
            "let text: Str = by_get(\"demo\")\n",
            "let boolean: Bool = by_index(false)\n",
        );
        let workspace = typing_workspace(&[source]);
        let _symbols = workspace.program.symbol_owner().enter();
        let session = CheckSession::new(
            Arc::clone(&workspace.program),
            Arc::clone(&workspace.sources),
        );
        let (root, source_id) = workspace.roots[0];
        let output = session.check(root, source_id, CheckOptions::for_execution());

        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(output.call_instantiations.len(), 3);
        assert_eq!(output.projections.len(), 2);
        assert!(output.projections.values().any(|projection| {
            projection.field == crate::symbol::Name::intern("identity")
                && projection.operation == crate::sema::check::ProjectionOperation::Get
        }));
        assert!(output.projections.values().any(|projection| {
            projection.field == crate::symbol::Name::intern("identity")
                && projection.operation == crate::sema::check::ProjectionOperation::Index
        }));
        let declarations = Checker::compact_declarations(&workspace.program, output);
        crate::runtime::eval::Evaluator::probe_embedded_module_lowering(
            &workspace.program,
            &declarations,
            source,
            Arc::clone(&workspace.sources),
            source_id,
        )
        .expect("cold full builder and verifier accept constant generic projections");
        drop(_symbols);

        let dynamic_workspace = typing_workspace(&[
            "use shared as models\npure select(key: Str) { models[key] }\n",
        ]);
        let _dynamic_symbols = dynamic_workspace.program.symbol_owner().enter();
        let dynamic_session = CheckSession::new(
            Arc::clone(&dynamic_workspace.program),
            Arc::clone(&dynamic_workspace.sources),
        );
        let (dynamic_root, dynamic_source) = dynamic_workspace.roots[0];
        let dynamic_output = dynamic_session.check(
            dynamic_root,
            dynamic_source,
            CheckOptions::default(),
        );
        assert!(dynamic_output.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckModuleMember)
        }));
    }

    #[test]
    fn generic_proc_effect_contracts_are_finite_and_checked() {
        let helper = "proc mark(label: Str) [io] -> Int { print $label; 1 }\n";
        let source = format!("{helper}proc traced(value, label: Str) [io] {{ let observed = mark(label); value }}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, &source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let generic = checked.checked_declarations.functions.values().find(|declaration| declaration.scheme.as_ref().is_some_and(|scheme| !scheme.type_parameters().is_empty())).unwrap();
        assert_eq!(generic.scheme.as_ref().unwrap().effects().concrete().unwrap(), Some(vec![crate::syntax::node::Effect::Io]));
        assert!(generic.scheme_body_facts.is_some());
        let source = "var shared = 1\nproc capture(value) { let observed = shared; value }\nlet integer: Result[Int] = capture(7)\nlet text: Result[Str] = capture(\"word\")\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let capture = checked.checked_declarations.functions.values().find(|declaration| declaration.name == "capture").unwrap();
        let scheme = capture.scheme.as_ref().unwrap();
        assert_eq!(scheme.type_parameters().len(), 1);
        let facts = capture.scheme_body_facts.as_ref().unwrap();
        let shared_read = parsed.arena.arena.source_rows().expr_ids(SourceId::new(0)).find(|&expression| {
            let node = parsed.arena.arena.expr(expression);
            node.span.start() == source.find("shared;").unwrap()
                && matches!(node.kind, crate::syntax::arena::ArenaExprKind::Ident(name) if name == "shared")
        }).unwrap();
        assert_eq!(facts.expression_types()[&shared_read].closed_ground_type(), Some(Type::Int));
        let arguments = checked.call_instantiations.values().flat_map(|evidence| evidence.instances()).flat_map(|instantiation| instantiation.type_arguments().values()).cloned().collect::<Vec<_>>();
        assert_eq!(arguments.len(), 2);
        assert!(arguments.contains(&SchemeType::Ground(Type::Int)));
        assert!(arguments.contains(&SchemeType::Ground(Type::Str)));
        for (case, source, code) in [
            ("explicit effect bound", format!("{helper}proc traced(value, label: Str) [] {{ let observed = mark(label); value }}\n"), crate::diagnostic::DiagnosticCode::CheckEffectViolation),
            ("incompatible callback bound", "proc invoke(action: Proc) [] -> Int { let observed = action.call(); 42 }\n".to_string(), crate::diagnostic::DiagnosticCode::CheckEffectViolation),
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, &source,
                CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
            assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckUnresolvedCall)), "{:?}", checked.diagnostics);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(code)), "{case}: {source}: {:?}", checked.diagnostics);
        }
        let source = "proc invoke(value, action: Proc) { let observed = action.call(); value }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(&parsed.arena, source,
            CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
        assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckUnresolvedCall)), "{:?}", checked.diagnostics);
        assert!(checked.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckLocalInference)
                && diagnostic.message == "generic callable body requires immutable bindings and checked value constructors, branches, projections, or selected calls with compatible effects"
        }), "{:?}", checked.diagnostics);
    }

    #[test]
    fn generic_proc_result_contract_is_independent_of_callers() {
        for calls in ["", concat!(
            "let integer: Int = wrap(42)?\n",
            "let text: Str = wrap(\"demo\")?\n",
            "let nested: Result[Int] = wrap(Ok(7))?\n",
        )] {
            let source = format!("proc wrap(value) {{ value }}\n{calls}");
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, &source,
                CheckOptions { embedded_bodies: true, ..CheckOptions::default() });
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let declaration = checked.checked_declarations.functions.values().next().unwrap();
            let scheme = declaration.scheme.as_ref().unwrap();
            assert_eq!(scheme.kind(), crate::sema::types::CallableKind::Proc);
            let [binder] = scheme.type_parameters() else { panic!("one declaration-owned type parameter") };
            assert_eq!(scheme.params()[0].ty, SchemeType::Parameter(*binder));
            assert_eq!(scheme.return_type(), &SchemeType::Result(Box::new(SchemeType::Parameter(*binder)), Box::new(SchemeType::Ground(Type::Error))));
            assert_eq!(scheme.effects().concrete().unwrap(), Some(vec![]));
            assert!(declaration.scheme_body_facts.is_some());
        }
    }

    #[test]
    fn imported_generic_aliases_instantiate_independently_without_changing_exports() {
        let workspace = typing_workspace(&[
            "use shared\nlet identity = shared.identity\nlet project = shared.name_of\nlet number: Int = identity(42)\nlet text: Str = identity(\"demo\")\nlet boolean: Bool = identity(false)\nlet name: Str = project({name: \"package\", version: 1})\n",
            "use shared\nlet identity = shared.identity\nlet text: Str = identity(\"second\")\nlet number: Int = identity(7)\n",
        ]);
        let _symbols = workspace.program.symbol_owner().enter();
        let session = CheckSession::new(
            Arc::clone(&workspace.program),
            Arc::clone(&workspace.sources),
        );
        let mut published = None;
        for (index, &(root, source)) in workspace.roots.iter().enumerate() {
            let output = session.check(root, source, CheckOptions::default());
            assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
            assert_no_inference_in_output(&output);
            assert_call_type_arguments(&output, if index == 0 {
                &[Type::Int, Type::Str, Type::Bool, Type::Str]
            } else {
                &[Type::Str, Type::Int]
            });
            let scheme = exported_scheme(
                &checked_module_facts(&session, workspace.module_source),
                "identity",
            );
            let text = workspace.sources.get(source).unwrap().text();
            let start = text.find("shared.identity").unwrap();
            let initializer = crate::source::Span::new(source, start, start + "shared.identity".len());
            let selected = output.callable_value_contract_at(initializer).expect("generic alias initializer contract");
            assert!(selected.is_generic());
            let declaration = output.checked_declarations.functions.values()
                .find(|declaration| declaration.name.as_str() == "identity").unwrap().callable_contract().unwrap();
            assert!(selected.equivalent_to(&declaration));
            if let Some(previous) = &published {
                assert!(Arc::ptr_eq(previous, &scheme));
            } else {
                published = Some(scheme);
            }
        }
    }

    #[test]
    fn imported_identity_and_projection_schemes_are_order_independent_across_roots() {
        let workspace = typing_workspace(&[
            "use shared\nlet identity: Str = shared.identity(\"demo\")\nlet name: Str = shared.name_of({name: \"package\", version: \"1.0\"})\n",
            "use shared\nlet identity: Int = shared.identity(42)\nlet name: Int = shared.name_of({name: 7, enabled: true})\n",
            "use shared\nlet integer_identity: Int = shared.identity(42)\nlet text_identity: Str = shared.identity(\"demo\")\nlet integer_name: Int = shared.name_of({name: 7, enabled: true})\nlet text_name: Str = shared.name_of({name: \"package\", version: \"1.0\"})\n",
        ]);
        let _symbols = workspace.program.symbol_owner().enter();

        let check_root = |root_index: usize| {
            let session = CheckSession::new(
                Arc::clone(&workspace.program),
                Arc::clone(&workspace.sources),
            );
            let (root, source) = workspace.roots[root_index];
            let output = session.check(root, source, CheckOptions::default());
            assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
            assert_no_inference_in_output(&output);
            let facts = checked_module_facts(&session, workspace.module_source);
            let identity = exported_scheme(&facts, "identity");
            let projection = exported_scheme(&facts, "name_of");
            (session, output, identity, projection)
        };

        let (root_a_only_session, root_a_only, root_a_identity, root_a_projection) =
            check_root(0);
        let (root_b_only_session, root_b_only, root_b_identity, root_b_projection) =
            check_root(1);
        assert_eq!(root_a_identity.as_ref(), root_b_identity.as_ref());
        assert_eq!(root_a_projection.as_ref(), root_b_projection.as_ref());
        assert_call_type_arguments(&root_a_only, &[Type::Str, Type::Str]);
        assert_call_type_arguments(&root_b_only, &[Type::Int, Type::Int]);
        drop(root_a_only_session);
        drop(root_b_only_session);

        let session_ab = CheckSession::new(
            Arc::clone(&workspace.program),
            Arc::clone(&workspace.sources),
        );
        let (root_a, source_a) = workspace.roots[0];
        let output_a = session_ab.check(root_a, source_a, CheckOptions::default());
        assert!(output_a.diagnostics.is_empty(), "{:?}", output_a.diagnostics);
        let identity_after_a =
            exported_scheme(&checked_module_facts(&session_ab, workspace.module_source), "identity");
        let projection_after_a =
            exported_scheme(&checked_module_facts(&session_ab, workspace.module_source), "name_of");
        let (root_b, source_b) = workspace.roots[1];
        let output_b = session_ab.check(root_b, source_b, CheckOptions::default());
        assert!(output_b.diagnostics.is_empty(), "{:?}", output_b.diagnostics);
        let facts_after_b = checked_module_facts(&session_ab, workspace.module_source);
        let identity_after_b = exported_scheme(&facts_after_b, "identity");
        let projection_after_b = exported_scheme(&facts_after_b, "name_of");
        assert!(Arc::ptr_eq(&identity_after_a, &identity_after_b));
        assert!(Arc::ptr_eq(&projection_after_a, &projection_after_b));
        assert_call_type_arguments(&output_a, &[Type::Str, Type::Str]);
        assert_call_type_arguments(&output_b, &[Type::Int, Type::Int]);

        let session_ba = CheckSession::new(
            Arc::clone(&workspace.program),
            Arc::clone(&workspace.sources),
        );
        let output_b = session_ba.check(root_b, source_b, CheckOptions::default());
        assert!(output_b.diagnostics.is_empty(), "{:?}", output_b.diagnostics);
        let identity_after_b =
            exported_scheme(&checked_module_facts(&session_ba, workspace.module_source), "identity");
        let projection_after_b =
            exported_scheme(&checked_module_facts(&session_ba, workspace.module_source), "name_of");
        let output_a = session_ba.check(root_a, source_a, CheckOptions::default());
        assert!(output_a.diagnostics.is_empty(), "{:?}", output_a.diagnostics);
        let facts_after_a = checked_module_facts(&session_ba, workspace.module_source);
        assert!(Arc::ptr_eq(
            &identity_after_b,
            &exported_scheme(&facts_after_a, "identity"),
        ));
        assert!(Arc::ptr_eq(
            &projection_after_b,
            &exported_scheme(&facts_after_a, "name_of"),
        ));
        assert_call_type_arguments(&output_b, &[Type::Int, Type::Int]);
        assert_call_type_arguments(&output_a, &[Type::Str, Type::Str]);

        let (joint_session, joint, joint_identity, joint_projection) = check_root(2);
        assert_eq!(joint_identity.as_ref(), root_a_identity.as_ref());
        assert_eq!(joint_projection.as_ref(), root_a_projection.as_ref());
        assert_call_type_arguments(
            &joint,
            &[Type::Int, Type::Int, Type::Str, Type::Str],
        );
        drop(joint_session);
    }

    fn assert_call_type_arguments(output: &CheckOutput, expected: &[Type]) {
        let mut actual = output
            .call_instantiations
            .values()
            .flat_map(|evidence| evidence.instances())
            .flat_map(|instantiation| instantiation.type_arguments().values())
            .cloned()
            .collect::<Vec<_>>();
        for expected in expected {
            let expected = SchemeType::Ground(expected.clone());
            let Some(position) = actual.iter().position(|actual| actual == &expected) else {
                panic!("missing call argument type {expected:?} in {actual:?}");
            };
            actual.remove(position);
        }
        assert!(actual.is_empty(), "unexpected call argument types: {actual:?}");
    }

    #[test]
    fn module_index_distinguishes_concrete_generic_and_dynamic_keys() {
        let texts = [
            "use shared\nlet concrete = shared[\"false_value\"]\n",
            "use shared\nlet generic = shared[\"identity\"]\nlet number = generic(7)\nlet word = generic(\"ready\")\n",
            "use shared\npure lookup(key: Str) -> Any { shared[key] }\n",
            "use shared\nlet concrete = shared.get(\"false_value\")?\n",
        ];
        let workspace = typing_workspace(&texts);
        let _symbols = workspace.program.symbol_owner().enter();
        let session = CheckSession::new(workspace.program.clone(), workspace.sources.clone());
        for (index, &(statements, source)) in workspace.roots.iter().enumerate() {
            let checked = session.check(statements, source, CheckOptions::default());
            if index == 2 {
                assert!(checked.diagnostics.iter().any(|diagnostic| {
                    diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckModuleMember)
                }), "root {index}: {:?}", checked.diagnostics);
            } else {
                assert!(checked.diagnostics.is_empty(), "root {index}: {:?}", checked.diagnostics);
            }
        }
    }

    #[test]
    fn recursive_inferred_parameters_share_provisional_types_in_both_orders() {
        for reverse in [false, true] {
            for argument in ["\"wrong\"", "value"] {
                let first = format!("pure first(value) -> Int {{ let fixed: Int = value; second({argument}) + fixed }}\n");
                let second = "pure second(value) -> Int { first(value) }\n";
                let source = if reverse {
                    format!("{second}{first}let answer: Int = first(1)\n")
                } else {
                    format!("{first}{second}let answer: Int = first(1)\n")
                };
                let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let checked = Checker::check_arena_with_options(
                    &parsed.arena,
                    &source,
                    CheckOptions::default(),
                );
                if argument == "value" {
                    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
                    for declaration in checked.checked_declarations.functions.values() {
                        let scheme = declaration.scheme.as_ref().expect("checked recursive scheme");
                        assert!(scheme.type_parameters().is_empty());
                        assert!(scheme.row_parameters().is_empty());
                        assert_eq!(scheme.params()[0].ty, SchemeType::Ground(Type::Int));
                    }
                } else {
                    assert!(checked.diagnostics.iter().any(|diagnostic| {
                        diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckTypeMismatch)
                    }), "reverse {reverse}: {:?}", checked.diagnostics);
                }
            }
        }
    }

    #[test]
    fn recursive_parameters_are_solved_without_caller_evidence() {
        for reverse in [false, true] {
            for caller_first in [false, true] {
                let first = "pure first(value) -> Int { second(value) }\n";
                let second = "pure second(value) -> Int { first(value) }\n";
                let definitions = if reverse {
                    format!("{second}{first}")
                } else {
                    format!("{first}{second}")
                };
                let call = "let answer: Int = first(1)\n";
                let source = if caller_first {
                    format!("{call}{definitions}")
                } else {
                    format!("{definitions}{call}")
                };
                let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let checked = Checker::check_arena_with_options(
                    &parsed.arena,
                    &source,
                    CheckOptions::default(),
                );
                assert!(checked.diagnostics.is_empty(), "reverse {reverse}, caller_first {caller_first}: {:?}", checked.diagnostics);
                assert_eq!(checked.checked_declarations.functions.len(), 2);
                for declaration in checked.checked_declarations.functions.values() {
                    let scheme = declaration.scheme.as_ref().expect("a recursive declaration has a checked scheme");
                    assert_eq!(scheme.type_parameters().len(), 1);
                    assert!(matches!(scheme.params()[0].ty, SchemeType::Parameter(_)));
                    assert_eq!(scheme.return_type(), &SchemeType::Ground(Type::Int));
                }
            }
        }
    }

    #[test]
    fn captured_mutable_bindings_are_not_generalized() {
        let source = "var captured = [1]\npure read_captured() { captured }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = parsed.arena.symbol_owner().with_current(|| {
            Checker::check_arena_with_options(
                &parsed.arena,
                source,
                CheckOptions::default(),
            )
        });
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let reader = checked
            .checked_declarations
            .functions
            .values()
            .find(|declaration| declaration.name.as_str() == "read_captured")
            .expect("reader of the mutable binding");
        let scheme = reader.scheme.as_ref().expect("checked reader scheme");
        assert!(scheme.type_parameters().is_empty());
        assert!(scheme.row_parameters().is_empty());
        assert_eq!(
            scheme.return_type(),
            &SchemeType::List(Box::new(SchemeType::Ground(Type::Int)))
        );
        assert_no_inference_in_output(&checked);

        let conflicting_source = concat!(
            "var captured = [1]\n",
            "pure read_captured() { captured }\n",
            "captured = [\"wrong\"]\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), conflicting_source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = parsed.arena.symbol_owner().with_current(|| {
            Checker::check_arena_with_options(
                &parsed.arena,
                conflicting_source,
                CheckOptions::default(),
            )
        });
        assert!(checked.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckTypeMismatch)
        }), "{:?}", checked.diagnostics);
    }

    #[test]
    fn recursive_component_remains_monomorphic() {
        let source = concat!(
            "pure first(value: Int) -> Int { second(value) }\n",
            "pure second(value: Int) -> Int { first(value) }\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(
            &parsed.arena,
            source,
            CheckOptions::default(),
        );
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_no_inference_in_output(&checked);
        assert_eq!(checked.checked_declarations.functions.len(), 2);
        assert_eq!(checked.selected_function_calls.len(), 2);
        assert!(checked.call_instantiations.is_empty());
        for declaration in checked.checked_declarations.functions.values() {
            let scheme = declaration.scheme.as_ref().expect("recursive function scheme");
            assert!(scheme.type_parameters().is_empty());
            assert!(scheme.row_parameters().is_empty());
            assert_eq!(scheme.params().len(), 1);
            assert!(matches!(
                &scheme.params()[0].ty,
                crate::sema::schemes::SchemeType::Ground(crate::sema::check::Type::Int)
            ));
            assert_eq!(
                scheme.return_type(),
                &crate::sema::schemes::SchemeType::Ground(crate::sema::check::Type::Int)
            );
        }
        let first_call_start = source.find("second(value)").expect("first recursive call");
        let first_call = crate::source::Span::new(
            SourceId::new(0),
            first_call_start,
            first_call_start + "second(value)".len(),
        );
        let second_call_start = source.find("first(value)").expect("second recursive call");
        let second_call = crate::source::Span::new(
            SourceId::new(0),
            second_call_start,
            second_call_start + "first(value)".len(),
        );
        assert_eq!(checked.expr_types.get(&first_call), Some(&crate::sema::check::Type::Int));
        assert_eq!(checked.expr_types.get(&second_call), Some(&crate::sema::check::Type::Int));
    }

    #[test]
    fn exported_generic_scheme_is_published_from_module_declaration_identity() {
        let mut sources = SourceMap::new();
        let entry_text = "use shared\nlet number: Int = shared.identity(1)\n";
        let module_text = concat!(
            "##! Shared identity module.\n",
            "## Returns the supplied value.\n",
            "export pure identity(value) { value }\n",
            "pure private_identity(value) { value }\n",
            "pure name_of(value) { value.name }\n",
            "pure helper(value: Int) -> Int { value }\n",
            "pure explicit_any(value: Any) -> Any { value }\n",
        );
        let entry_source = sources.add_file("entry.xsh", entry_text);
        let module_source = sources.add_file("shared.xsh", module_text);
        let mut builder = ArenaProgramBuilder::with_token_capacity(128);
        let entry = Parser::parse_source_into_arena_builder(entry_source, entry_text, &mut builder);
        let module = Parser::parse_source_into_arena_builder(
            module_source,
            module_text,
            &mut builder,
        );
        assert!(entry.diagnostics.is_empty(), "{:?}", entry.diagnostics);
        assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
        let module_name = builder.symbol_owner().intern("shared");
        builder.push_arena_module("shared".to_string(), module_name, module.statements);
        for statement in builder.statement_ids(entry.statements) {
            if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
                builder.set_use_resolved(use_id, Arc::from("shared"));
            }
        }
        let program = Arc::new(builder.finish_with_statements(entry.statements));
        let session = CheckSession::new(Arc::clone(&program), Arc::new(sources));
        let output = session.check(entry.statements, entry_source, CheckOptions::default());
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);

        let record = session
            .records
            .lock()
            .expect("module record cache poisoned")
            .values()
            .find(|record| record.source_id() == module_source)
            .expect("shared module record")
            .clone();
        let checked = record.check_once(None, || unreachable!("module check is cached"));
        let functions = &checked.output.checked_declarations.functions;
        let exported = functions
            .values()
            .find(|declaration| declaration.name.as_str() == "identity")
            .expect("checked exported identity");
        let private = functions
            .values()
            .find(|declaration| declaration.name.as_str() == "helper")
            .expect("checked private helper");
        let private_identity = functions
            .values()
            .find(|declaration| declaration.name.as_str() == "private_identity")
            .expect("checked private identity");
        let name_of = functions
            .values()
            .find(|declaration| declaration.name.as_str() == "name_of")
            .expect("checked private name projection");
        let explicit_any = functions
            .values()
            .find(|declaration| declaration.name.as_str() == "explicit_any")
            .expect("checked explicit Any function");
        assert!(exported.exported);
        assert!(!private.exported);
        assert!(!private_identity.exported);
        assert!(!name_of.exported);

        let source_span = |definition: &str, expression: &str| {
            let definition_start = module_text.find(definition).expect("function source");
            let expression_start = definition.rfind(expression).expect("body expression");
            crate::source::Span::new(
                module_source,
                definition_start + expression_start,
                definition_start + expression_start + expression.len(),
            )
        };
        let exported_identity_body =
            source_span("export pure identity(value) { value }", "value");
        let identity_body = source_span("pure private_identity(value) { value }", "value");
        let name_projection = source_span("pure name_of(value) { value.name }", "value.name");
        let helper_body = source_span("pure helper(value: Int) -> Int { value }", "value");
        let any_body = source_span("pure explicit_any(value: Any) -> Any { value }", "value");
        let private_scheme = private_identity.scheme.as_ref().expect("private identity scheme");
        assert_eq!(private_scheme.type_parameters().len(), 1);
        assert!(exported
            .scheme_body_facts
            .as_ref()
            .expect("exported identity body facts")
            .tail_types()
            .values()
            .any(|ty| matches!(ty, crate::sema::schemes::SchemeType::Parameter(_))));
        assert!(explicit_any.scheme_body_facts.is_none());
        let identity_body_type = private_identity
            .scheme_body_facts
            .as_ref()
            .expect("private identity body facts")
            .tail_types()
            .iter()
            .find_map(|(&statement, ty)| {
                (program.arena.stmt(statement).span == identity_body).then_some(ty)
            });
        assert!(matches!(
            identity_body_type,
            Some(crate::sema::schemes::SchemeType::Parameter(_))
        ));
        let name_scheme = name_of.scheme.as_ref().expect("name projection scheme");
        assert_eq!(name_scheme.type_parameters().len(), 1);
        assert_eq!(name_scheme.row_parameters().len(), 1);
        let name_body_type = name_of
            .scheme_body_facts
            .as_ref()
            .expect("name projection body facts")
            .expression_types()
            .iter()
            .find_map(|(&expression, ty)| {
                (program.arena.expr(expression).span == name_projection).then_some(ty)
            });
        assert!(matches!(
            name_body_type,
            Some(crate::sema::schemes::SchemeType::Parameter(_))
        ));
        assert!(!checked.output.expr_types.contains_key(&identity_body));
        assert!(!checked
            .output
            .expr_types
            .contains_key(&exported_identity_body));
        assert!(!checked.output.expr_types.contains_key(&name_projection));
        assert!(!checked
            .output
            .parameter_types
            .contains_key(&private_identity.parameter_spans[0]));
        assert!(!checked
            .output
            .function_return_types
            .contains_key(&private_identity.body));
        assert_eq!(
            checked.output.expr_types.get(&helper_body),
            Some(&crate::sema::check::Type::Int)
        );
        assert_eq!(
            checked.output.expr_types.get(&any_body),
            Some(&crate::sema::check::Type::Any)
        );
        assert_eq!(
            checked
                .output
                .parameter_types
                .get(&explicit_any.parameter_spans[0]),
            Some(&crate::sema::check::Type::Any)
        );

        program.symbol_owner().with_current(|| {
            let compact = checked.output.checked_declarations.clone().into_compact();
            let key = crate::symbol::QualifiedName::new(
                crate::symbol::Name::intern("shared"),
                crate::symbol::Name::intern("identity"),
            );
            assert_eq!(compact.function_schemes.len(), 1);
            let published = compact.function_schemes.get(&key).expect("exported scheme");
            let checked_scheme = exported.scheme.as_ref().expect("checked scheme");
            assert!(Arc::ptr_eq(published, checked_scheme));
        });
    }

    #[test]
    fn unresolved_nongeneric_material_still_requires_an_annotation() {
        let source = "pure build() { let unresolved = []\n 1 }\n";
        let program = Parser::parse_source_arena_only(SourceId::new(0), source).arena;
        let checked = Checker::check_arena_with_options(
            &program,
            source,
            CheckOptions::default(),
        );
        assert!(checked.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckLocalInference)
        }));
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
            .filter(|((expression, _), _)| {
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
            .filter(|((expression, _), _)| {
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
        let ((expression, _), _) = published_plans.keys().next().expect("published plan");
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
        let root_text = "use shared\nproc main(...argv: shared.Arguments) [io, error] { assert args == argv; assert shared.incoming() == argv; io.write_stdout_bytes(argv[0])? }\nmain(@args)?\n";
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
        let expected_module_checks = view.modules().count();
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
            fact.effective.concrete().expect("finite checked effect fixture")
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
            Some(&crate::sema::effects::CheckedEffects::finite([crate::syntax::node::Effect::Io]))
        );
        assert!(output.function_effect_facts.iter().any(|(identity, fact)| {
            identity.namespace.is_none()
                && identity.body.source_id == root_source
                && fact.effective.concrete().expect("finite checked effect fixture").is_some_and(|effects| {
                    effects.contains(&crate::syntax::node::Effect::Io)
                })
        }));
        assert!(output.function_effect_facts.iter().any(|(identity, fact)| {
            identity.namespace == Some(helper_name)
                && identity.body.source_id == helper_source
                && fact.effective.concrete().expect("finite checked effect fixture").is_some_and(|effects| {
                    effects.contains(&crate::syntax::node::Effect::Time)
                })
        }));
    }

    #[test]
    fn guarded_bindings_keep_their_lexical_callable_capture_boundary() {
        use crate::diagnostic::DiagnosticCode;
        for source in [
            "guard let selected = Ok(7) else { exit 3 }\npure read() -> Int { selected }\nassert read() == 7\n",
            "let candidate: Int? = 7\nguard let selected = candidate else { exit 3 }\npure read() -> Int { selected }\nassert read() == 7\n",
            "guard let {selected} = Ok({selected: 7}) else { exit 3 }\npure read() -> Int { selected }\nassert read() == 7\n",
            "stream rows() [] -> Stream[Int] { yield 7 }\nlet initial = rows()\nguard let selected = Ok(initial) else { exit 3 }\npure read() -> Stream[Int] { selected }\n",
            "stream rows() [] -> Stream[Int] { yield 7 }\nlet initial: Stream[Int]? = rows()\nguard let selected = initial else { exit 3 }\npure read() -> Stream[Int] { selected }\n",
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
            assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
        }
        for (source, expected) in [
            ("pure read() -> Int { return selected }\nguard let selected = Ok(7) else { exit 3 }\n", DiagnosticCode::CheckUnresolvedName),
            ("guard let selected = Ok(7) else { print \"missing\" }\npure read() -> Int { selected }\n", DiagnosticCode::CheckGuardFallthrough),
        ] {
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let checked = Checker::check_arena_with_options(&parsed.arena, source, CheckOptions::for_execution());
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(expected)), "{source}\n{:?}", checked.diagnostics);
        }
    }

    #[test]
    fn invalid_main_parameter_types_produce_ordinary_diagnostics() {
        use crate::diagnostic::DiagnosticCode;
        for (source, expected) in [
            ("proc main(...argv: item[Str]) {}\n", vec![DiagnosticCode::CheckTypeApplication, DiagnosticCode::CheckRestType]),
            ("proc main(value: Missing) {}\n", vec![DiagnosticCode::CheckUnknownType]),
            ("proc main(...argv: List[Missing]) {}\n", vec![DiagnosticCode::CheckUnknownType]),
            ("proc main() -> Missing {}\n", vec![DiagnosticCode::CheckUnknownType, DiagnosticCode::CheckMissingReturn]),
            ("proc main(...argv: item[Str]) {}\nmain(...args)\n", vec![DiagnosticCode::CheckTypeApplication, DiagnosticCode::CheckRestType, DiagnosticCode::CheckNamedSpread]),
            ("proc main(value: Int) {}\n", vec![DiagnosticCode::CompactMainMissingSpread]),
            ("proc main(value: Bytes = b\"x\") {}\n", vec![DiagnosticCode::CompactMainArgs]),
            ("proc main(...argv: List[Bytes]) {}\n", vec![]),
            ("proc main(value: Str) {}\n", vec![]),
        ] {
            let mut sources = SourceMap::new();
            let source_id = sources.add_file("entry.xsh", source);
            let mut builder = ArenaProgramBuilder::with_token_capacity(32);
            let parsed = Parser::parse_source_into_arena_builder(source_id, source, &mut builder);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let program = Arc::new(builder.finish_with_statements(parsed.statements));
            let session = CheckSession::new(program, Arc::new(sources));
            for options in [CheckOptions::default(), CheckOptions::for_execution(), CheckOptions::for_check_cli()] {
                let output = session.check(parsed.statements, source_id, options);
                let actual = output.diagnostics.iter().filter_map(|diagnostic| diagnostic.code.map(|code| code.name())).collect::<std::collections::BTreeSet<_>>();
                assert_eq!(actual, expected.iter().map(|code| code.name()).collect(), "{source}: {:?}", output.diagnostics);
            }
        }
    }

    #[test]
    fn rejected_interfaces_preserve_independent_generalization_diagnostics() {
        use crate::diagnostic::{DiagnosticCode, LabelStyle};
        let independent = "pure empty() { let state = {items: []}; state.items }\n";
        for (rejected, expected) in [
            ("pure rejected(value: Missing) -> Int { 0 }\n", DiagnosticCode::CheckUnknownType),
            ("pure rejected(flag: Bool, value: Any) { if flag { null } else { value } }\n", DiagnosticCode::CheckInferReturn),
        ] {
            for reversed in [false, true] {
                let source = if reversed { format!("{independent}{rejected}") } else { format!("{rejected}{independent}") };
                let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                let checked = Checker::check_arena(&parsed.arena, &source);
                assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code == Some(expected)), "{:?}", checked.diagnostics);
                let start = source.find(independent).unwrap();
                let end = start + independent.len();
                assert!(checked.diagnostics.iter().any(|diagnostic| {
                    diagnostic.code == Some(DiagnosticCode::CheckLocalInference)
                        && diagnostic.labels.iter().any(|label| label.style == LabelStyle::Primary
                            && label.span.start() >= start && label.span.end() <= end)
                }), "independent generic inference diagnostic is retained: {:?}", checked.diagnostics);
            }
        }
    }

    #[test]
    fn recovery_generalization_does_not_hide_independent_inference_errors() {
        use crate::diagnostic::{DiagnosticCode, LabelStyle};
        let continuation = "pure continuation(data, at: Int) -> Bool {\n  let byte = data.byte_at(at) ?? 0\n  byte >= 128 and byte < 192\n}\n\npure use_bytes(data: Bytes, at: Int) -> Bool {\n  continuation(data, at)\n}\n";
        let independent = "pure empty() { let state = {items: []}; state.items }\n";

        for reversed in [false, true] {
            let source = if reversed {
                format!("{independent}{continuation}")
            } else {
                format!("{continuation}{independent}")
            };
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, &source);

            assert!(checked.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == Some(DiagnosticCode::CheckAmbiguousOverload)
            }), "{:?}", checked.diagnostics);
            let continuation_start = source.find("pure continuation").unwrap();
            let continuation_end = continuation_start + continuation.len();
            assert!(!checked.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == Some(DiagnosticCode::CheckLocalInference)
                    && diagnostic.message.contains("callable generalization failed: Build(RecoveryType)")
                    && diagnostic.labels.iter().any(|label| {
                        label.style == LabelStyle::Primary
                            && label.span.start() >= continuation_start
                            && label.span.end() <= continuation_end
                    })
            }), "the reported operation errors already explain this recovery: {:?}", checked.diagnostics);

            let independent_start = source.find(independent).unwrap();
            let independent_end = independent_start + independent.len();
            assert!(checked.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == Some(DiagnosticCode::CheckLocalInference)
                    && diagnostic.labels.iter().any(|label| {
                        label.style == LabelStyle::Primary
                            && label.span.start() >= independent_start
                            && label.span.end() <= independent_end
                    })
            }), "independent generic inference diagnostic is retained: {:?}", checked.diagnostics);
        }

        let same_body = "pure continuation(data, at: Int) -> Bool {\n  let byte = data.byte_at(at) ?? 0\n  let state = {items: []}\n  let _ = state.items\n  byte >= 128 and byte < 192\n}\n\npure use_bytes(data: Bytes, at: Int) -> Bool {\n  continuation(data, at)\n}\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), same_body);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, same_body);
        assert!(checked.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(DiagnosticCode::CheckAmbiguousOverload)
        }), "{:?}", checked.diagnostics);
        let state_start = same_body.find("{items: []}").unwrap();
        let state_end = state_start + "{items: []}".len();
        assert!(checked.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(DiagnosticCode::CheckLocalInference)
                && diagnostic.message == "local type needs an annotation"
                && diagnostic.labels.iter().any(|label| {
                    label.style == LabelStyle::Primary
                        && label.span.start() >= state_start
                        && label.span.end() <= state_end
                })
        }), "same-body local inference diagnostic is retained: {:?}", checked.diagnostics);
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

    #[test]
    fn check_cli_preserves_constant_and_body_constructor_instances() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/xsh/stdlib/rel_path.xsh");
        let file = path.to_string_lossy().into_owned();
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let roots = crate::frontend::load::project_module_roots(&path)
            .unwrap_or_else(|error| panic!("module roots for {}: {error}", path.display()));
        let entry = crate::frontend::load::entry_source_from_text(&file, source.clone());
        let source_id = entry.source_id;
        let (sources, parsed) = crate::frontend::load::parse_load_entry_source_arena_only(
            &file, entry, roots,
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let program = Arc::new(parsed.arena);
        let checked = Checker::check_arena_with_sources_and_options(
            &program,
            &sources,
            source_id,
            CheckOptions::for_check_cli(),
        );
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);

        let instance_names = checked
            .checked_declarations
            .record_constructors
            .instance_facts_for_test()
            .iter()
            .map(|(definition, _, _)| {
                program
                    .arena
                    .type_def(*definition)
                    .name
                    .as_str()
                    .as_str()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(instance_names, ["Defaults", "Entry", "Inventory"]);

        let defaults_span = checked
            .record_constructor_instances
            .keys()
            .find(|span| {
                let owner = sources.get(span.source_id).expect("constructor source exists");
                &owner.text()[span.range()] == "Defaults(mode: 1)"
            })
            .expect("Defaults constructor fact");
        let defaults = checked
            .record_constructor_instances
            .get(defaults_span)
            .expect("Defaults constructor fact");
        assert_eq!(
            checked
                .checked_declarations
                .record_constructors
                .defaults(defaults.instance.definition)
                .and_then(|defaults| {
                    defaults
                        .iter()
                        .find(|(name, _)| name.as_str().as_str() == "rel")
                        .map(|(_, value)| value)
                }),
            Some(&crate::sema::constants::LiteralConstant::Path(Arc::from("etc/default")))
        );
        let invalid_source = source.replacen("\"etc/default\"", "42", 1);
        assert_ne!(invalid_source, source, "regression fixture default changed");
        let invalid_entry = crate::frontend::load::entry_source_from_text(&file, invalid_source);
        let invalid_source_id = invalid_entry.source_id;
        let (invalid_sources, invalid_parsed) =
            crate::frontend::load::parse_load_entry_source_arena_only(
                &file,
                invalid_entry,
                crate::frontend::load::project_module_roots(&path)
                    .expect("project module roots"),
            );
        assert!(invalid_parsed.diagnostics.is_empty(), "{:?}", invalid_parsed.diagnostics);
        let invalid_program = Arc::new(invalid_parsed.arena);
        let invalid_checked = Checker::check_arena_with_sources_and_options(
            &invalid_program,
            &invalid_sources,
            invalid_source_id,
            CheckOptions::for_check_cli(),
        );
        assert!(invalid_checked.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(crate::diagnostic::DiagnosticCode::CheckTypeMismatch)
        }));
    }

    #[test]
    fn check_body_publishes_supplied_record_constructor_fields() {
        let source = concat!(
            "type Options = {provided: Int, defaulted: Int = 0}\n",
            "pure make_options() -> Options { Options(provided: 1) }\n",
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena_with_options(
            &parsed.arena,
            source,
            CheckOptions::for_execution(),
        );
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let span = checked
            .record_constructor_fields
            .keys()
            .find(|span| &source[span.range()] == "Options(provided: 1)")
            .expect("supplied-field constructor fact");
        let fields = checked
            .record_constructor_fields
            .get(span)
            .expect("supplied-field fact");
        assert_eq!(
            fields
                .iter()
                .map(|field| field.as_str().as_str().to_owned())
                .collect::<Vec<_>>(),
            ["provided"]
        );
    }

}
