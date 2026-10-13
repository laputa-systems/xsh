use super::{
    CacheTrace, CheckBundleView, CheckOutput, Checker, Diagnostic, RecordConstructors,
    ResolvedBindings, ResourceTransferFacts, UserModuleSig,
    infer_return::ReturnInferenceIndex, local_inference::LocalInference,
};
use crate::sema::constants::PreparedConstants;
use crate::syntax::arena::{ArenaProgram, ArenaStmtKind, ArenaUserModule, StmtId};
use rustc_hash::{FxHashMap, FxHashSet};
use crate::source::SourceId;
use std::collections::hash_map::Entry;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum ArgsDomain {
    Str,
    Bytes,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum ModuleRecordRole {
    Entry,
    User,
    Embedded,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct ModuleRecordKey {
    module_key: Arc<str>,
    source_id: SourceId,
    start: u32,
    len: u32,
    role: ModuleRecordRole,
    args_domain: ArgsDomain,
}

impl ModuleRecordKey {
    fn trace_event(&self, trace: Option<&CacheTrace>, event: &'static str) {
        if let Some(trace) = trace {
            trace.event(
                event,
                &format!("{:?}", self.module_key),
                &format!("{:?}", self.source_id),
                self.start,
                self.len,
                match self.role {
                    ModuleRecordRole::Entry => "Entry",
                    ModuleRecordRole::User => "User",
                    ModuleRecordRole::Embedded => "Embedded",
                },
                match self.args_domain {
                    ArgsDomain::Str => "Str",
                    ArgsDomain::Bytes => "Bytes",
                },
            );
        }
    }
}

pub(super) struct ModuleRecord {
    key: ModuleRecordKey,
    statements: Arc<[StmtId]>,
    owner_facts: Arc<PreparedModuleFacts>,
    prepared_facts: Arc<PreparedModuleFacts>,
    checked: OnceLock<Arc<CheckedModuleFacts>>,
}

pub(super) struct CheckedModuleFacts {
    pub(super) signature: Option<UserModuleSig>,
    pub(super) output: CheckOutput,
    pub(super) module_diagnostic_position: usize,
    pub(super) suppressed_body_transfers: ResourceTransferFacts,
}

impl ModuleRecord {
    pub(super) fn source_id(&self) -> SourceId {
        self.key.source_id
    }

    pub(super) fn is_embedded(&self) -> bool {
        self.key.role == ModuleRecordRole::Embedded
    }

    pub(super) fn args_domain(&self) -> ArgsDomain {
        self.key.args_domain
    }

    pub(super) fn statements(&self) -> &[StmtId] {
        &self.statements
    }

    pub(super) fn owner_facts(&self) -> &Arc<PreparedModuleFacts> {
        &self.owner_facts
    }

    pub(super) fn prepared_facts(&self) -> &Arc<PreparedModuleFacts> {
        &self.prepared_facts
    }

    pub(super) fn check_once(
        &self,
        trace: Option<&CacheTrace>,
        check: impl FnOnce() -> CheckedModuleFacts,
    ) -> Arc<CheckedModuleFacts> {
        let mut initialized = false;
        let checked = self.checked.get_or_init(|| {
            let facts = check();
            self.key.trace_event(trace, "checked");
            initialized = true;
            Arc::new(facts)
        });
        if !initialized {
            self.key.trace_event(trace, "check_hit");
        }
        checked.clone()
    }

}

pub(super) struct ModuleRecords {
    root: Arc<ModuleRecord>,
    modules: FxHashMap<Arc<str>, Arc<ModuleRecord>>,
    trace: Option<CacheTrace>,
}

impl ModuleRecords {
    fn prepare(
        view: &CheckBundleView<'_>,
        args_domain: ArgsDomain,
        shared: &Mutex<FxHashMap<ModuleRecordKey, Arc<ModuleRecord>>>,
        trace: Option<&CacheTrace>,
        #[cfg(test)] preparations: &Mutex<FxHashMap<ModuleRecordKey, usize>>,
    ) -> Self {
        let program = view.program;
        let mut prepared = FxHashMap::<Arc<str>, Arc<ModuleRecord>>::default();
        for module in view.modules.iter().copied() {
            let source_id = module_source_id(program, module);
            let role = if module.internal {
                ModuleRecordRole::Embedded
            } else {
                ModuleRecordRole::User
            };
            let module_key: Arc<str> = Arc::from(module.key.as_str());
            let key = ModuleRecordKey {
                module_key: Arc::clone(&module_key),
                source_id,
                start: module.statements.start,
                len: module.statements.len,
                role,
                args_domain,
            };
            let mut cache = shared.lock().expect("module record cache poisoned");
            let (record, created) = match cache.entry(key.clone()) {
                Entry::Occupied(entry) => (entry.get().clone(), false),
                Entry::Vacant(entry) => {
                    let dependency_facts = Self::module_dependency_facts(view, module, &prepared);
                    let module_view = CheckBundleView::for_module_owner(
                        program,
                        module,
                        view.sources,
                        source_id,
                    );
                    let owner_facts = Arc::new(PreparedModuleFacts::prepare_owner(
                        &module_view,
                        &dependency_facts,
                    ));
                    let mut visible_facts = dependency_facts;
                    visible_facts.merge_owner(program, &owner_facts);
                    #[cfg(test)]
                    {
                        record_test_preparation(preparations, key.clone());
                    }
                    let record = Arc::new(ModuleRecord {
                        key: key.clone(),
                        statements: program.module_statements(module).collect::<Vec<_>>().into(),
                        owner_facts,
                        prepared_facts: Arc::new(visible_facts),
                        checked: OnceLock::new(),
                    });
                    entry.insert(record.clone());
                    (record, true)
                }
            };
            drop(cache);
            key.trace_event(trace, if created { "prepared" } else { "reuse" });
            prepared.insert(Arc::clone(&module_key), record);
        }

        let root_source_id = view.root_source_id().unwrap_or_else(|| SourceId::new(0));
        let root_key = ModuleRecordKey {
            module_key: Arc::from("<entry>"),
            source_id: root_source_id,
            start: view.root_statements.start,
            len: view.root_statements.len,
            role: ModuleRecordRole::Entry,
            args_domain,
        };
        let root = {
            let mut cache = shared.lock().expect("module record cache poisoned");
            let (record, created) = match cache.entry(root_key.clone()) {
                Entry::Occupied(entry) => (entry.get().clone(), false),
                Entry::Vacant(entry) => {
                    let mut dependency_facts = PreparedModuleFacts::default();
                    for module in view.modules.iter().copied() {
                        let record = prepared
                            .get(module.key.as_str())
                            .expect("reachable module was prepared before entry facts");
                        dependency_facts.merge_owner(program, record.owner_facts());
                    }
                    let root_view = CheckBundleView::for_module(
                        program,
                        view.root_statements,
                        view.sources,
                        Some(root_source_id),
                    );
                    let owner_facts = Arc::new(PreparedModuleFacts::prepare_owner(
                        &root_view,
                        &dependency_facts,
                    ));
                    let mut wire_diagnostics = owner_facts.wire_diagnostics.clone();
                    wire_diagnostics.extend(dependency_facts.wire_diagnostics.iter().cloned());
                    let mut visible_facts = dependency_facts;
                    visible_facts.merge_owner(program, &owner_facts);
                    visible_facts.wire_diagnostics = wire_diagnostics;
                    // The legacy collector visited entry constants before module constants.
                    // Reassemble owner-local instances in that order for stable publication.
                    let mut ordered_instances = owner_facts.record_constructors.detached();
                    for module in view.modules.iter().copied() {
                        let record = prepared
                            .get(module.key.as_str())
                            .expect("reachable module was prepared before entry instances");
                        ordered_instances.merge_instance_facts(
                            &record.owner_facts.record_constructors,
                        );
                    }
                    visible_facts
                        .record_constructors
                        .replace_instance_facts(&ordered_instances);
                    #[cfg(test)]
                    record_test_preparation(preparations, root_key.clone());
                    let record = Arc::new(ModuleRecord {
                        key: root_key.clone(),
                        statements: view.statement_ids().collect::<Vec<_>>().into(),
                        owner_facts,
                        prepared_facts: Arc::new(visible_facts),
                        checked: OnceLock::new(),
                    });
                    entry.insert(record.clone());
                    (record, true)
                }
            };
            drop(cache);
            root_key.trace_event(trace, if created { "prepared" } else { "reuse" });
            record
        };
        Self {
            root,
            modules: prepared,
            trace: trace.cloned(),
        }
    }

    fn module_dependency_facts(
        view: &CheckBundleView<'_>,
        module: &ArenaUserModule,
        prepared: &FxHashMap<Arc<str>, Arc<ModuleRecord>>,
    ) -> PreparedModuleFacts {
        let dependency_keys = module_dependency_keys(view.program, module);
        let mut facts = PreparedModuleFacts::default();
        for dependency in view.modules.iter().copied() {
            if dependency.key != module.key && dependency_keys.contains(&dependency.key) {
                let record = prepared
                    .get(dependency.key.as_str())
                    .unwrap_or_else(|| {
                        panic!(
                            "module `{}` depends on `{}` before it is prepared; {} modules were prepared",
                            module.name.as_str(),
                            dependency.name.as_str(),
                            prepared.len(),
                        )
                    });
                facts.merge_owner(view.program, record.owner_facts());
            }
        }
        facts
    }

    pub(super) fn root(&self) -> &Arc<ModuleRecord> {
        &self.root
    }

    pub(super) fn trace(&self) -> Option<&CacheTrace> {
        self.trace.as_ref()
    }

    pub(super) fn install(&self, checker: &mut Checker, program: &ArenaProgram) {
        self.root.prepared_facts.install(checker, program);
    }

    #[cfg(test)]
    fn facts(&self) -> &Arc<PreparedModuleFacts> {
        &self.root.prepared_facts
    }

    pub(super) fn module(&self, key: &str) -> Option<&Arc<ModuleRecord>> {
        self.modules.get(key)
    }

    #[cfg(test)]
    fn module_count(&self) -> usize {
        self.modules.len()
    }
}

fn module_source_id(program: &ArenaProgram, module: &ArenaUserModule) -> SourceId {
    program
        .module_statements(module)
        .next()
        .map(|statement| program.arena.stmt(statement).span.source_id)
        .or_else(|| program.module_doc_for(module.statements).map(|span| span.source_id))
        .unwrap_or_else(|| panic!("module source identity missing for `{}`", module.name.as_str()))
}

fn module_import_keys<'a>(
    program: &'a ArenaProgram,
    module: &'a ArenaUserModule,
) -> impl Iterator<Item = String> + 'a {
    program.module_statements(module).filter_map(|statement| {
        let ArenaStmtKind::Use(use_id) = program.arena.stmt(statement).kind else {
            return None;
        };
        program.arena.use_stmt(use_id).resolved.as_deref().map(str::to_owned)
    })
}

fn module_dependency_keys(program: &ArenaProgram, module: &ArenaUserModule) -> FxHashSet<String> {
    let mut pending = module_import_keys(program, module).collect::<Vec<_>>();
    let mut dependencies = FxHashSet::default();
    while let Some(key) = pending.pop() {
        if !dependencies.insert(key.clone()) {
            continue;
        }
        if let Some(dependency) = program.modules.iter().find(|candidate| candidate.key == key) {
            pending.extend(module_import_keys(program, dependency));
        }
    }
    dependencies
}

#[cfg(test)]
fn test_preparation_count(
    counts: &Mutex<FxHashMap<ModuleRecordKey, usize>>,
    key: &ModuleRecordKey,
) -> usize {
    counts
        .lock()
        .expect("module preparation counter poisoned")
        .get(key)
        .copied()
        .unwrap_or_default()
}

#[cfg(test)]
fn record_test_preparation(
    counts: &Mutex<FxHashMap<ModuleRecordKey, usize>>,
    key: ModuleRecordKey,
) {
    *counts
        .lock()
        .expect("module preparation counter poisoned")
        .entry(key)
        .or_default() += 1;
}

pub(super) struct ModuleRecordCache {
    shared: Arc<Mutex<FxHashMap<ModuleRecordKey, Arc<ModuleRecord>>>>,
    trace: Option<CacheTrace>,
    #[cfg(test)]
    preparations: Mutex<FxHashMap<ModuleRecordKey, usize>>,
}

impl Default for ModuleRecordCache {
    fn default() -> Self {
        Self {
            shared: Arc::new(Mutex::new(FxHashMap::default())),
            trace: CacheTrace::private(),
            #[cfg(test)]
            preparations: Mutex::new(FxHashMap::default()),
        }
    }
}

impl ModuleRecordCache {
    pub(super) fn with_shared(
        shared: Arc<Mutex<FxHashMap<ModuleRecordKey, Arc<ModuleRecord>>>>,
        trace: Option<CacheTrace>,
    ) -> Self {
        Self {
            shared,
            trace,
            #[cfg(test)]
            preparations: Mutex::new(FxHashMap::default()),
        }
    }

    /// Reuse a selected domain only after that entry has a checked record.
    pub(super) fn checked_entry_domain(
        &self,
        view: &CheckBundleView<'_>,
    ) -> Option<ArgsDomain> {
        let key = |args_domain| ModuleRecordKey {
            module_key: Arc::from("<entry>"),
            source_id: view.root_source_id().unwrap_or_else(|| SourceId::new(0)),
            start: view.root_statements.start,
            len: view.root_statements.len,
            role: ModuleRecordRole::Entry,
            args_domain,
        };
        let shared = self.shared.lock().expect("module record cache poisoned");
        [ArgsDomain::Str, ArgsDomain::Bytes]
            .into_iter()
            .find(|&domain| shared.get(&key(domain)).is_some_and(|record| record.checked.get().is_some()))
    }
}

impl ModuleRecordCache {
    pub(super) fn get_or_prepare(
        &mut self,
        view: &CheckBundleView<'_>,
        args_domain: ArgsDomain,
    ) -> ModuleRecords {
        ModuleRecords::prepare(
            view,
            args_domain,
            &self.shared,
            self.trace.as_ref(),
            #[cfg(test)]
            &self.preparations,
        )
    }

    #[cfg(test)]
    fn preparation_count(&self, key: &ModuleRecordKey) -> usize {
        test_preparation_count(&self.preparations, key)
    }
}

#[derive(Clone, Default)]
pub(super) struct PreparedModuleFacts {
    pub(super) resolved_bindings: Arc<ResolvedBindings>,
    pub(super) return_inference_index: Arc<ReturnInferenceIndex>,
    pub(super) local_inference: LocalInference,
    pub(super) record_constructors: RecordConstructors,
    pub(super) prepared_constants: PreparedConstants,
    regex_diagnostics: Vec<Diagnostic>,
    wire_diagnostics: Vec<Diagnostic>,
}

impl PreparedModuleFacts {
    fn prepare_owner(
        view: &CheckBundleView<'_>,
        dependencies: &PreparedModuleFacts,
    ) -> Self {
        let program = view.program;
        let owner_bindings = ResolvedBindings::collect_bundle(view);
        let mut visible_bindings = dependencies
            .resolved_bindings
            .merge_visible(&owner_bindings);
        let declarations = visible_bindings.declarations.clone();
        visible_bindings.resolve_module_fields(program, &declarations);
        let mut resolved_bindings = owner_bindings;
        resolved_bindings.resolve_module_fields(program, &declarations);
        let resolved_bindings = Arc::new(resolved_bindings);
        let return_inference_index = Arc::new(ReturnInferenceIndex::collect(view));
        let local_inference = LocalInference::prepare(view);
        let regex_diagnostics = Checker::prepare_regex_literals(view);
        let mut record_constructors = RecordConstructors::collect_bundle(view);
        let mut visible_constructors = dependencies.record_constructors.clone();
        let owner_instances = visible_constructors.merge_visible(&record_constructors);
        let prepared_instance_start = visible_constructors.instance_count();
        let (mut prepared_for_owner, wire_diagnostics) =
            PreparedConstants::collect_bundle_with_visible_and_diagnostics(
                view,
                &visible_constructors,
                &visible_bindings,
                &dependencies.prepared_constants,
            );
        record_constructors.merge_instance_facts(&owner_instances);
        // Constant preparation may admit owner schemas through the shared visible index.
        // Transfer only its appended facts; dependency facts were already admitted separately.
        let prepared_instances = visible_constructors.instance_facts_since(prepared_instance_start);
        record_constructors.merge_instance_facts(&prepared_instances);
        record_constructors.apply_prepared_defaults_bundle(view, &prepared_for_owner);
        prepared_for_owner.retain_owner_source(
            program,
            view.root_source_id().unwrap_or_else(|| SourceId::new(0)),
            &visible_bindings,
        );
        let facts = Self {
            resolved_bindings,
            return_inference_index,
            local_inference,
            record_constructors,
            prepared_constants: prepared_for_owner,
            regex_diagnostics,
            wire_diagnostics,
        };
        facts
    }

    fn merge_owner(&mut self, program: &ArenaProgram, owner: &Self) {
        let mut bindings = self
            .resolved_bindings
            .merge_visible(&owner.resolved_bindings);
        let declarations = bindings.declarations.clone();
        bindings.resolve_module_fields(program, &declarations);
        self.resolved_bindings = Arc::new(bindings);
        Arc::make_mut(&mut self.return_inference_index)
            .merge_owner(&owner.return_inference_index);
        self.local_inference
            .merge_prepared_owner(&owner.local_inference);
        self.record_constructors
            .merge_visible(&owner.record_constructors);
        self.prepared_constants
            .merge_visible(&owner.prepared_constants);
        self.regex_diagnostics
            .extend(owner.regex_diagnostics.iter().cloned());
        self.wire_diagnostics
            .extend(owner.wire_diagnostics.iter().cloned());
    }

    pub(super) fn install(&self, checker: &mut Checker, program: &ArenaProgram) {
        checker.resolved_bindings = self.resolved_bindings.clone();
        checker.return_inference_index = Some(self.return_inference_index.clone());
        checker.local_inference = self.local_inference.clone();
        checker.record_constructors = self.record_constructors.clone();
        checker.prepared_constants = self.prepared_constants.clone();
        checker.wire_enums = self.prepared_constants.cli_wire_enums.clone();
        checker.diagnostics.extend(self.regex_diagnostics.iter().cloned());
        checker
            .diagnostics
            .extend(self.prepared_constants.diagnostics.iter().cloned());
        checker
            .diagnostics
            .extend(self.wire_diagnostics.iter().cloned());
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
    fn trace_counts_prepare_reuse_and_check_once_per_cache_owner() {
        let source = "pure value() -> Int { 1 }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let program = parsed.arena;
        let _symbol_owner = program.symbol_owner().enter();
        let view = CheckBundleView::new(&program, None, Some(SourceId::new(0)));

        let (first_trace, first_bytes) = CacheTrace::capture(10, "private");
        let mut first_cache = ModuleRecordCache::default();
        first_cache.trace = Some(first_trace.clone());
        let first = first_cache.get_or_prepare(&view, ArgsDomain::Str);
        let again = first_cache.get_or_prepare(&view, ArgsDomain::Str);
        assert!(Arc::ptr_eq(first.root(), again.root()));
        let checked = || CheckedModuleFacts {
            signature: None,
            output: CheckOutput::default(),
            module_diagnostic_position: 0,
            suppressed_body_transfers: ResourceTransferFacts::default(),
        };
        first.root().check_once(first.trace(), checked);
        again.root().check_once(again.trace(), checked);

        let (second_trace, second_bytes) = CacheTrace::capture(11, "private");
        let mut second_cache = ModuleRecordCache::default();
        second_cache.trace = Some(second_trace);
        let second = second_cache.get_or_prepare(&view, ArgsDomain::Str);
        second.root().check_once(second.trace(), checked);

        let first_rows = String::from_utf8(
            first_bytes.lock().expect("first trace buffer").clone(),
        )
        .expect("first trace UTF-8");
        let second_rows = String::from_utf8(
            second_bytes.lock().expect("second trace buffer").clone(),
        )
        .expect("second trace UTF-8");
        for (event, expected) in [
            ("prepared", 1),
            ("reuse", 1),
            ("checked", 1),
            ("check_hit", 1),
        ] {
            assert_eq!(
                first_rows
                    .lines()
                    .filter(|line| line.split('\t').nth(6) == Some(event))
                    .count(),
                expected,
                "{event}: {first_rows}"
            );
        }
        assert_eq!(
            second_rows
                .lines()
                .filter(|line| line.split('\t').nth(6) == Some("prepared"))
                .count(),
            1,
            "{second_rows}"
        );
        assert_eq!(
            second_rows
                .lines()
                .filter(|line| line.split('\t').nth(6) == Some("checked"))
                .count(),
            1,
            "{second_rows}"
        );
    }

    #[test]
    fn prepared_record_instances_follow_root_then_module_owner_order() {
        let root_source = "type RootSpec = {value: Int}\nconst root_spec: RootSpec = {value: 1}\n";
        let module_source = "##! Imported record values.\n## A module-owned schema.\nexport type ModuleSpec = {value: Int}\n## A typed module constant.\nexport const module_spec: ModuleSpec = {value: 2}\n";
        let mut builder = ArenaProgramBuilder::with_token_capacity(96);
        let root = Parser::parse_source_into_arena_builder(
            SourceId::new(0),
            root_source,
            &mut builder,
        );
        let module = Parser::parse_source_into_arena_builder(
            SourceId::new(1),
            module_source,
            &mut builder,
        );
        assert!(root.diagnostics.is_empty(), "{:?}", root.diagnostics);
        assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
        let module_name = builder.symbol_owner().intern("owned");
        builder.push_internal_arena_module("owned".to_string(), module_name, module.statements);

        let program = builder.finish_with_statements(root.statements);
        let _symbol_owner = program.symbol_owner().enter();
        let view = CheckBundleView::for_root(
            &program,
            root.statements,
            None,
            Some(SourceId::new(0)),
        );
        let root_type = program
            .statement_ids()
            .find_map(|statement| match program.arena.stmt(statement).kind {
                ArenaStmtKind::TypeDef(definition) => Some(definition),
                _ => None,
            })
            .expect("entry schema");
        let module = view.modules.first().expect("embedded module");
        let module_type = program
            .module_statements(module)
            .find_map(|statement| match program.arena.stmt(statement).kind {
                ArenaStmtKind::Export(inner) => match program.arena.stmt(inner).kind {
                    ArenaStmtKind::TypeDef(definition) => Some(definition),
                    _ => None,
                },
                _ => None,
            })
            .expect("module schema");
        let mut cache = ModuleRecordCache::default();
        let records = cache.get_or_prepare(&view, ArgsDomain::Str);
        let constructor_facts = format!("{:?}", records.root().prepared_facts.record_constructors);
        let root_instance = constructor_facts
            .find(&format!("({root_type:?}, [], Record"))
            .expect("entry type instance is prepared");
        let module_instance = constructor_facts
            .find(&format!("({module_type:?}, [], Record"))
            .expect("module type instance is prepared");
        assert!(
            root_instance < module_instance,
            "the entry owner's prepared instance must precede imported owner instances: {constructor_facts}"
        );
    }

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
        let alias_root = Parser::parse_source_into_arena_builder(
            SourceId::new(7),
            "use shared_alias\n",
            &mut builder,
        );
        let embedded_shared = Parser::parse_source_into_arena_builder(
            SourceId::new(6),
            "export pure identity(value: Int) -> Int { value }\n",
            &mut builder,
        );
        for parsed in [&root, &shared, &left, &right, &alias_root, &embedded_shared] {
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        }
        for (name, parsed) in [("shared", &shared), ("left", &left), ("right", &right)] {
            let symbol = builder.symbol_owner().intern(name);
            builder.push_arena_module(name.to_string(), symbol, parsed.statements);
        }
        let alias = builder.symbol_owner().intern("shared_alias");
        builder.push_arena_module("shared_alias".to_string(), alias, shared.statements);
        let embedded_name = builder.symbol_owner().intern("shared");
        builder.push_internal_arena_module(
            "embedded-shared".to_string(),
            embedded_name,
            embedded_shared.statements,
        );
        for (statements, names) in [
            (root.statements, ["left", "right"].as_slice()),
            (left.statements, ["shared"].as_slice()),
            (right.statements, ["shared"].as_slice()),
            (alias_root.statements, ["shared_alias"].as_slice()),
        ] {
            let mut names = names.iter();
            for statement in builder.statement_ids(statements) {
                if let Some((use_id, _, _)) = builder.use_stmt_for_statement(statement) {
                    builder.set_use_resolved(use_id, Arc::from(*names.next().unwrap()));
                }
            }
        }
        let program = builder.finish_with_statements(root.statements);
        let _symbol_owner = program.symbol_owner().enter();
        let view = CheckBundleView::new(&program, None, Some(SourceId::new(0)));
        let mut cache = ModuleRecordCache::default();
        let first_probe = cache.get_or_prepare(&view, ArgsDomain::Str);
        let next_probe = cache.get_or_prepare(&view, ArgsDomain::Str);
        let byte_probe = cache.get_or_prepare(&view, ArgsDomain::Bytes);
        let shared = first_probe.module("shared").unwrap();
        assert_eq!(first_probe.module_count(), 4);
        assert!(Arc::ptr_eq(first_probe.root(), next_probe.root()));
        assert!(Arc::ptr_eq(shared, next_probe.module("shared").unwrap()));
        assert!(!Arc::ptr_eq(shared, byte_probe.module("shared").unwrap()));
        assert!(
            !Arc::ptr_eq(shared, first_probe.module("embedded-shared").unwrap()),
            "an embedded implementation has a separate record role from a user module"
        );
        assert_eq!(shared.args_domain(), ArgsDomain::Str);
        assert_eq!(byte_probe.module("shared").unwrap().args_domain(), ArgsDomain::Bytes);
        assert!(Arc::ptr_eq(first_probe.facts(), next_probe.facts()));
        assert!(!Arc::ptr_eq(first_probe.facts(), byte_probe.facts()));

        let shared_range = program
            .modules
            .iter()
            .find(|module| module.key == "shared")
            .expect("shared module registered")
            .statements;
        let shared_key = ModuleRecordKey {
            module_key: Arc::from("shared"),
            source_id: SourceId::new(1),
            start: shared_range.start,
            len: shared_range.len,
            role: ModuleRecordRole::User,
            args_domain: ArgsDomain::Str,
        };
        assert_eq!(cache.preparation_count(&shared_key), 1);
        let root_key = ModuleRecordKey {
            module_key: Arc::from("<entry>"),
            source_id: SourceId::new(0),
            start: root.statements.start,
            len: root.statements.len,
            role: ModuleRecordRole::Entry,
            args_domain: ArgsDomain::Str,
        };
        assert_eq!(cache.preparation_count(&root_key), 1);
        let byte_shared_key = ModuleRecordKey {
            args_domain: ArgsDomain::Bytes,
            ..shared_key.clone()
        };
        assert_eq!(cache.preparation_count(&byte_shared_key), 1);

        let left_view = CheckBundleView::for_root(
            &program,
            left.statements,
            None,
            Some(SourceId::new(2)),
        );
        let mut left_cache =
            ModuleRecordCache::with_shared(cache.shared.clone(), cache.trace.clone());
        let left_entry = left_cache.get_or_prepare(&left_view, ArgsDomain::Str);
        assert!(Arc::ptr_eq(shared, left_entry.module("shared").unwrap()));
        assert_eq!(left_cache.preparation_count(&shared_key), 0);
        assert_eq!(left_entry.module_count(), 2);
        let left_root_key = ModuleRecordKey {
            module_key: Arc::from("<entry>"),
            source_id: SourceId::new(2),
            start: left.statements.start,
            len: left.statements.len,
            role: ModuleRecordRole::Entry,
            args_domain: ArgsDomain::Str,
        };
        assert_eq!(left_cache.preparation_count(&left_root_key), 1);

        let right_view = CheckBundleView::for_root(
            &program,
            right.statements,
            None,
            Some(SourceId::new(3)),
        );
        let mut right_cache =
            ModuleRecordCache::with_shared(cache.shared.clone(), cache.trace.clone());
        let right_entry = right_cache.get_or_prepare(&right_view, ArgsDomain::Str);
        assert!(Arc::ptr_eq(shared, right_entry.module("shared").unwrap()));
        assert_eq!(right_cache.preparation_count(&shared_key), 0);

        let alias_view = CheckBundleView::for_root(
            &program,
            alias_root.statements,
            None,
            Some(SourceId::new(7)),
        );
        let mut alias_cache =
            ModuleRecordCache::with_shared(cache.shared.clone(), cache.trace.clone());
        let alias_entry = alias_cache.get_or_prepare(&alias_view, ArgsDomain::Str);
        let shared_alias = alias_entry.module("shared_alias").unwrap();
        assert!(!Arc::ptr_eq(shared, shared_alias));
        let alias_key = ModuleRecordKey {
            module_key: Arc::from("shared_alias"),
            ..shared_key.clone()
        };
        assert_eq!(alias_cache.preparation_count(&alias_key), 1);

        let mut other_builder = ArenaProgramBuilder::with_token_capacity(128);
        let other_root = Parser::parse_source_into_arena_builder(
            SourceId::new(4),
            "use shared\n",
            &mut other_builder,
        );
        let other_shared = Parser::parse_source_into_arena_builder(
            SourceId::new(5),
            "export pure identity(value: Int) -> Int { value }\n",
            &mut other_builder,
        );
        let other_name = other_builder.symbol_owner().intern("shared");
        other_builder.push_arena_module(
            "shared".to_string(),
            other_name,
            other_shared.statements,
        );
        for statement in other_builder.statement_ids(other_root.statements) {
            if let Some((use_id, _, _)) = other_builder.use_stmt_for_statement(statement) {
                other_builder.set_use_resolved(use_id, Arc::from("shared"));
            }
        }
        let other_program = other_builder.finish_with_statements(other_root.statements);
        let other_view = CheckBundleView::new(&other_program, None, Some(SourceId::new(4)));
        let other_module = other_program.symbol_owner().with_current(|| {
            ModuleRecordCache::with_shared(cache.shared.clone(), cache.trace.clone())
                .get_or_prepare(&other_view, ArgsDomain::Str)
                .module("shared")
                .unwrap()
                .clone()
        });
        assert!(!Arc::ptr_eq(shared, &other_module));
        assert_ne!(shared.source_id(), other_module.source_id());

    }
}
