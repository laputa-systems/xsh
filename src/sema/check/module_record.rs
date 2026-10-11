use super::{
    BundleView, Checker, Diagnostic, EffectGraph, RecordConstructors, ResolvedBindings,
    infer_return::ReturnInferenceIndex, local_inference::LocalInference,
};
use crate::sema::constants::PreparedConstants;
use crate::sema::wire_enums::PreparedWireEnums;
use crate::syntax::arena::{ArenaProgram, StmtId};
use rustc_hash::FxHashMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum ArgsDomain {
    Str,
    Bytes,
}

#[derive(Clone, Debug)]
pub(super) struct ModuleRecord {
    pub(super) args_domain: ArgsDomain,
    statements: Arc<[StmtId]>,
}

impl ModuleRecord {
    pub(super) fn statements(&self) -> &[StmtId] {
        &self.statements
    }
}

#[derive(Clone)]
pub(super) struct ModuleRecords {
    root: Arc<ModuleRecord>,
    modules: FxHashMap<Arc<str>, Arc<ModuleRecord>>,
    facts: Arc<PreparedModuleFacts>,
}

impl ModuleRecords {
    fn prepare(view: &BundleView<'_>, args_domain: ArgsDomain) -> Self {
        let facts = Arc::new(PreparedModuleFacts::prepare(view));
        let program = view.program;
        let root = Arc::new(ModuleRecord {
            args_domain,
            statements: program.statement_ids().collect::<Vec<_>>().into(),
        });
        let modules = view
            .modules
            .iter()
            .map(|module| {
                (
                    Arc::from(module.key.as_str()),
                    Arc::new(ModuleRecord {
                        args_domain,
                        statements: program.module_statements(module).collect::<Vec<_>>().into(),
                    }),
                )
            })
            .collect();
        Self { root, modules, facts }
    }

    pub(super) fn root(&self) -> &Arc<ModuleRecord> {
        &self.root
    }

    pub(super) fn install(&self, checker: &mut Checker, program: &ArenaProgram) {
        self.facts.install(checker, program);
    }

    #[cfg(test)]
    fn facts(&self) -> &Arc<PreparedModuleFacts> {
        &self.facts
    }

    pub(super) fn module(&self, key: &str) -> Option<&Arc<ModuleRecord>> {
        self.modules.get(key)
    }

    #[cfg(test)]
    fn module_count(&self) -> usize {
        self.modules.len()
    }
}

#[derive(Default)]
pub(super) struct ModuleRecordCache {
    domains: FxHashMap<ArgsDomain, Arc<ModuleRecords>>,
}

impl ModuleRecordCache {
    pub(super) fn get_or_prepare(
        &mut self,
        view: &BundleView<'_>,
        args_domain: ArgsDomain,
    ) -> Arc<ModuleRecords> {
        self.domains
            .entry(args_domain)
            .or_insert_with(|| Arc::new(ModuleRecords::prepare(view, args_domain)))
            .clone()
    }
}

#[derive(Clone)]
pub(super) struct PreparedModuleFacts {
    pub(super) resolved_bindings: Arc<ResolvedBindings>,
    pub(super) return_inference_index: Arc<ReturnInferenceIndex>,
    pub(super) effect_declarations: EffectGraph,
    pub(super) local_inference: LocalInference,
    pub(super) record_constructors: RecordConstructors,
    pub(super) prepared_constants: PreparedConstants,
    pub(super) wire_enums: PreparedWireEnums,
    regex_diagnostics: Vec<Diagnostic>,
    wire_diagnostics: Vec<Diagnostic>,
}

impl PreparedModuleFacts {
    fn prepare(view: &BundleView<'_>) -> Self {
        let program = view.program;
        let resolved_bindings = Arc::new(ResolvedBindings::collect_bundle(view));
        let return_inference_index = Arc::new(ReturnInferenceIndex::collect(view));
        let effect_declarations = EffectGraph::declarations(view);
        let local_inference = LocalInference::prepare(view);
        let regex_diagnostics = Checker::prepare_regex_literals(view);
        let mut record_constructors = RecordConstructors::collect_bundle(view);
        let prepared_constants = PreparedConstants::collect_bundle(
            view,
            &record_constructors,
            &resolved_bindings,
        );
        record_constructors.apply_prepared_defaults_bundle(view, &prepared_constants);
        let (wire_enums, wire_diagnostics) = PreparedWireEnums::prepare_bundle(view, |expr| {
            prepared_constants.analyze_expression(&program.arena, expr)
        });
        Self {
            resolved_bindings,
            return_inference_index,
            effect_declarations,
            local_inference,
            record_constructors,
            prepared_constants,
            wire_enums,
            regex_diagnostics,
            wire_diagnostics,
        }
    }

    pub(super) fn install(&self, checker: &mut Checker, program: &ArenaProgram) {
        checker.resolved_bindings = self.resolved_bindings.clone();
        checker.return_inference_index = Some(self.return_inference_index.clone());
        if checker.effect_graph.is_empty() {
            checker.effect_graph = self.effect_declarations.clone();
        }
        checker.local_inference = self.local_inference.clone();
        checker.record_constructors = self.record_constructors.clone();
        checker.prepared_constants = self.prepared_constants.clone();
        checker.wire_enums = self.wire_enums.clone();
        checker.diagnostics.extend(self.regex_diagnostics.iter().cloned());
        checker
            .diagnostics
            .extend(self.prepared_constants.diagnostics.iter().cloned());
        checker.diagnostics.extend(self.wire_diagnostics.iter().cloned());
        for (expression, fact) in &self.prepared_constants.record_constructor_instances {
            checker
                .record_constructor_instances
                .insert(program.arena.expr(*expression).span, fact.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;
    use crate::syntax::arena::ArenaProgramBuilder;
    use crate::syntax::parser::Parser;

    #[test]
    fn probe_rounds_reuse_one_record_per_reachable_module_and_argument_domain() {
        let mut builder = ArenaProgramBuilder::with_token_capacity(128);
        let root = Parser::parse_source_into_arena_builder(
            SourceId::new(0),
            "use left\nuse right\n",
            &mut builder,
        );
        let shared = Parser::parse_source_into_arena_builder(
            SourceId::new(1),
            "export pure identity(value: Int) -> Int { value }\n",
            &mut builder,
        );
        let left = Parser::parse_source_into_arena_builder(
            SourceId::new(2),
            "use shared\n",
            &mut builder,
        );
        let right = Parser::parse_source_into_arena_builder(
            SourceId::new(3),
            "use shared\n",
            &mut builder,
        );
        for parsed in [&root, &shared, &left, &right] {
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        }
        for (name, parsed) in [("shared", &shared), ("left", &left), ("right", &right)] {
            let symbol = builder.symbol_owner().intern(name);
            builder.push_arena_module(name.to_string(), symbol, parsed.statements);
        }
        for (statements, names) in [
            (root.statements, ["left", "right"].as_slice()),
            (left.statements, ["shared"].as_slice()),
            (right.statements, ["shared"].as_slice()),
        ] {
            let mut names = names.iter();
            for statement in builder.statement_ids(statements) {
                if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
                    builder.set_use_resolved(use_id, Arc::from(*names.next().unwrap()));
                }
            }
        }
        let program = builder.finish_with_statements(root.statements);
        let view = BundleView::new(&program, None, Some(SourceId::new(0)));
        let mut cache = ModuleRecordCache::default();
        let first_probe = cache.get_or_prepare(&view, ArgsDomain::Str);
        let next_probe = cache.get_or_prepare(&view, ArgsDomain::Str);
        let byte_probe = cache.get_or_prepare(&view, ArgsDomain::Bytes);
        assert_eq!(cache.domains.len(), 2);
        let shared = first_probe.module("shared").unwrap();
        assert_eq!(first_probe.module_count(), 3);
        assert!(Arc::ptr_eq(&first_probe, &next_probe));
        assert!(Arc::ptr_eq(shared, next_probe.module("shared").unwrap()));
        assert!(!Arc::ptr_eq(shared, byte_probe.module("shared").unwrap()));
        assert_eq!(shared.args_domain, ArgsDomain::Str);
        assert_eq!(byte_probe.module("shared").unwrap().args_domain, ArgsDomain::Bytes);
        assert!(Arc::ptr_eq(first_probe.facts(), next_probe.facts()));
        assert!(!Arc::ptr_eq(first_probe.facts(), byte_probe.facts()));
    }
}
