mod module_reuse_tests {
    use super::*;
    use crate::frontend::query::SolvedQuery;
    use crate::loader::{entry_source_from_text, module_key, parse_load_entry_source_arena_only};
    use std::path::PathBuf;

    struct ModuleFixture { root: PathBuf }

    impl ModuleFixture {
        fn new(name: &str) -> Self {
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let root = std::env::temp_dir().join(format!("xsh-indexed-{name}-{}-{stamp}", std::process::id()));
            std::fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn write(&self, file: &str, source: &str) {
            std::fs::write(self.root.join(file), source).unwrap();
        }

        fn diamond(&self, leaf: &str) {
            self.write("leaf.xsh", leaf);
            for file in ["left.xsh", "right.xsh"] {
                self.write(file, "##! A shared dependency forwarder.\nuse leaf as shared\n## Preserve the defining module's relationship.\nexport pure forward(value) { shared.selected(value) }\n");
            }
        }

        fn load(&self, source: &str) -> (SourceMap, crate::syntax::parser::ArenaParseOutput) {
            let entry = self.root.join("entry.xsh");
            self.write("entry.xsh", source);
            let loaded = parse_load_entry_source_arena_only(entry.to_str().unwrap(),
                entry_source_from_text(entry.to_str().unwrap(), source.to_string()), Vec::new());
            assert!(loaded.1.diagnostics.is_empty(), "{:?}", loaded.1.diagnostics);
            loaded
        }

        fn prepare(&self, source: &str, inspect: impl FnOnce(&FullProgram)) -> (Evaluator, crate::runtime::eval::CompactIndexedRunPlan) {
            let (sources, parsed) = self.load(source);
            let source_id = sources.files()[0].id();
            Checker::reset_module_reuse_counters();
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{source}: {:?}", checked.diagnostics);
            let source_counts = Checker::module_reuse_counters();
            let solved = Arc::downgrade(&checked.solved);
            let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
            let plan = evaluator.prepare_compact_indexed_only_from_checked(&parsed.arena, source_id, &checked)
                .unwrap_or_else(|diagnostic| panic!("the real loaded bundle prepares: {diagnostic:?}"));
            let program = evaluator.indexed_program.as_deref().unwrap();
            program.symbol_owner().with_current(|| inspect(program));
            assert_eq!(Checker::module_reuse_counters(), source_counts, "preparation consumes the checked interfaces without checking source again");
            drop(checked);
            drop(parsed);
            assert!(solved.upgrade().is_none(), "prepared imports cannot retain the inference bundle");
            (evaluator, plan)
        }

        fn assert_diamond_source_counts(&self) {
            let counts = Checker::module_reuse_counters();
            let keys = ["leaf.xsh", "left.xsh", "right.xsh"].map(|file| module_key(&self.root.join(file))).into_iter().collect::<std::collections::BTreeSet<_>>();
            assert_eq!(counts.module_checks.keys().cloned().collect::<std::collections::BTreeSet<_>>(), keys);
            assert!(counts.module_checks.values().all(|&count| count == 1));
            assert_eq!(counts.module_check_completions, counts.module_checks);
            assert_eq!(counts.interface_publications, counts.module_checks);
            assert_eq!(counts.interface_imports.keys().cloned().collect::<std::collections::BTreeSet<_>>(), keys);
            assert_eq!(counts.interface_imports.values().map(Vec::len).sum::<usize>(), 4);
            let leaf = &counts.interface_imports[&module_key(&self.root.join("leaf.xsh"))];
            assert_eq!(leaf.len(), 2);
            assert_ne!(leaf[0].source_id, leaf[1].source_id, "the two actual importers reuse the same published leaf interface");
            assert_eq!(counts.declaration_checks.len(), 3);
            assert!(counts.declaration_checks.iter().all(|(identity, &count)| count == 1 && identity.namespace.is_some_and(|namespace| keys.iter().any(|key| key.as_str() == &*namespace.as_str()))));
            assert_eq!(counts.declaration_generations, counts.declaration_checks);
            assert_eq!(counts.declaration_generalizations, counts.declaration_checks);
            assert_eq!(counts.generalization_components.len(), 3);
            assert!(counts.generalization_components.iter().all(|component| component.len() == 1), "these three acyclic declarations have three singleton generalization components");
        }
    }

    impl Drop for ModuleFixture {
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.root); }
    }

    fn execute((evaluator, plan): (Evaluator, crate::runtime::eval::CompactIndexedRunPlan), recursive: bool) -> (u8, Vec<u8>, Vec<u8>) {
        execute_observing((evaluator, plan), recursive, None)
    }

    fn execute_observing((evaluator, plan): (Evaluator, crate::runtime::eval::CompactIndexedRunPlan), recursive: bool, function: Option<LoweredFunctionKey>) -> (u8, Vec<u8>, Vec<u8>) {
        let symbols = evaluator.indexed_program.as_ref().unwrap().symbol_owner().clone();
        let output = crate::runtime::eval::run_eval(move || symbols.with_current(|| {
            let source_checks = Checker::module_reuse_counters();
            let run = || {
                assert_eq!(crate::runtime::eval::lowered_run::recursive_fast_path_forced(), recursive);
                evaluator.try_eval_installed_compact_indexed_only_inner(plan)
                    .unwrap_or_else(|_| panic!("the prepared bundle remains installed after frontend disposal"))
            };
            let output = if let Some(function) = function {
                crate::runtime::eval::lowered_run::with_observed_indexed_call_route(function, recursive, run)
            } else if recursive { crate::runtime::eval::lowered_run::with_forced_recursive_fast_path(run) } else { run() };
            assert_eq!(Checker::module_reuse_counters(), source_checks, "the execution worker cannot check or solve imported source again");
            output
        }));
        (output.status, output.stdout, output.stderr)
    }

    #[test]
    fn zero_slot_import_driver_keeps_loaded_values_and_original_live_captures_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let fixture = ModuleFixture::new("zero-slot-live-import");
            fixture.write("leaf.xsh", "##! A private module binding.\nlet selected: Int = 7\n## Read the original module binding.\nexport pure chosen() -> Int { selected }\n");
            for recursive in [false, true] {
                let prepared = fixture.prepare("use leaf as shared\nvar observed: Int = 0\nproc advance() [] -> Int { observed += shared.chosen(); observed }\nprint ${advance()} ${advance()}\n", |program| {
                    let import = program.driver_step_view(0).unwrap();
                    assert_eq!(import.tag(), FullDriverTag::Use);
                    assert_eq!(import.slot_count(), 0, "an import without prior root bindings owns no slot allocation");
                    assert!(program.generic_evidence().unwrap().lexical_captures().any(|(_, capture)| capture.mutable));
                });
                assert_eq!(execute(prepared, recursive), (0, b"7 14\n".to_vec(), Vec::new()));
            }
        });
    }

    #[test]
    fn private_module_captures_keep_their_original_namespace_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let fixture = ModuleFixture::new("private-capture-owners");
            fixture.write("left.xsh", "##! A private integer capture.\nlet chosen: Int = 7\n## Read the private capture.\nexport pure selected() -> Int { chosen }\n");
            fixture.write("right.xsh", "##! A distinct private integer capture.\nlet chosen: Int = 19\n## Read the private capture.\nexport pure selected() -> Int { chosen }\n");
            for source in ["use left as left\nuse right as right\nprint ${left.selected()} ${right.selected()}\n",
                "use right as right\nuse left as left\nprint ${left.selected()} ${right.selected()}\n"] {
                for recursive in [false, true] {
                    let prepared = fixture.prepare(source, |program| {
                        for file in ["left.xsh", "right.xsh"] {
                            let namespace = Name::intern(module_key(&fixture.root.join(file)));
                            let key = LoweredFunctionKey::Qualified(QualifiedName::new(namespace, Name::intern("selected")));
                            let view = program.function_view(key, LoweredFunctionKind::Pure).unwrap().unwrap();
                            let header = view.header().unwrap();
                            let captures = header.captures.iter().filter(|capture| capture.name == Name::intern("chosen")).collect::<Vec<_>>();
                            assert_eq!(captures.len(), 1);
                            assert_eq!(captures[0].name, Name::intern("chosen"));
                            assert_eq!(captures[0].kind, LoweredType::Int);
                        }
                    });
                    assert_eq!(execute(prepared, recursive), (0, b"7 19\n".to_vec(), Vec::new()));
                }
            }
        });
    }

    #[test]
    fn nested_imports_and_root_shadowing_keep_private_capture_owners() {
        crate::runtime::eval::run_eval(|| {
            let fixture = ModuleFixture::new("nested-private-capture-owners");
            fixture.write("leaf.xsh", "##! A separate private binding.\nlet chosen: Int = 99\n## Read the private binding.\nexport pure selected() -> Int { chosen }\n");
            for (file, initial) in [("left.xsh", 7), ("right.xsh", 19)] {
                fixture.write(file, &format!("##! A private parent binding.\nlet chosen: Int = {initial}\nuse leaf as shared\n## Read the nested dependency.\nexport pure nested() -> Int {{ shared.selected() }}\n## Read this module's private binding.\nexport pure selected() -> Int {{ chosen }}\n"));
            }
            for recursive in [false, true] {
                let prepared = fixture.prepare("use left as left\nuse right as right\nlet chosen: Int = 100\nprint ${left.nested()} ${right.nested()}\nprint ${left.selected()} ${right.selected()} ${chosen}\n", |_| {});
                assert_eq!(execute(prepared, recursive), (0, b"99 99\n7 19 100\n".to_vec(), Vec::new()));
            }
        });
    }

    #[test]
    fn imported_generic_diamond_shares_qualified_bodies_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let fixture = ModuleFixture::new("generic-diamond");
            fixture.diamond("##! A definition-owned row projection.\n## Select the original record's name.\nexport pure selected(value) { value.name }\n");
            for recursive in [false, true] {
                let unused = fixture.prepare("use left\nuse right\nprint \"unused\"\n", |program| {
                    assert_eq!(program.function_count(), 3);
                    let evidence = program.generic_evidence().unwrap();
                    assert_eq!(evidence.scopes().count(), 3);
                    assert_eq!(evidence.instances().count(), 0, "an imported definition needs no observed concrete client");
                    let forwarded = evidence.calls().iter().filter_map(|call| match call.evidence {
                        crate::runtime::eval::indexed::generic::CallEvidence::Forwarded(plan) => Some(evidence.forwarding(plan).unwrap()),
                        _ => None,
                    }).collect::<Vec<_>>();
                    assert_eq!(forwarded.len(), 2);
                    assert!(forwarded.iter().all(|plan| !plan.requirements.is_empty() && plan.instances.is_empty()));
                });
                fixture.assert_diamond_source_counts();
                let source_counts = Checker::module_reuse_counters();
                assert_eq!(execute(unused, recursive), (0, b"unused\n".to_vec(), Vec::new()));
                assert_eq!(Checker::module_reuse_counters(), source_counts);
                for calls in [
                    "let narrow = left.forward({name: \"narrow\"})\nlet wide = right.forward({age: 11, name: 17})\n",
                    "let wide = right.forward({age: 11, name: 17})\nlet narrow = left.forward({name: \"narrow\"})\n",
                ] {
                    let source = format!("use left\nuse right\n{calls}print ${{narrow}} ${{wide}}\n");
                    crate::loader::reset_module_load_counters();
                    let prepared = fixture.prepare(&source, |program| {
                        assert_eq!(program.function_count(), 3, "each original qualified definition has one body");
                        let evidence = program.generic_evidence().unwrap();
                        let mut scopes = std::collections::HashSet::new();
                        for (file, name) in [("leaf.xsh", "selected"), ("left.xsh", "forward"), ("right.xsh", "forward")] {
                            let key = LoweredFunctionKey::Qualified(QualifiedName::new(Name::intern(module_key(&fixture.root.join(file))), Name::intern(name)));
                            let view = program.function_view(key, LoweredFunctionKind::Pure).unwrap().unwrap();
                            let scope = view.generic_scope().expect("imports retain their definition-owned scheme");
                            assert!(scopes.insert(scope));
                            assert_eq!(evidence.scopes().filter(|(id, _)| *id == scope).count(), 1);
                        }
                        assert_eq!(evidence.scopes().count(), 3);
                        let mut slots = std::collections::BTreeSet::new();
                        let mut concrete = std::collections::HashSet::new();
                        for (id, instance) in evidence.instances() {
                            concrete.insert(id);
                            for witness in &instance.requirements {
                                if let crate::runtime::eval::indexed::generic::RequirementWitness::Projection { field_slot, .. } = witness { slots.insert(*field_slot); }
                            }
                        }
                        assert_eq!(concrete.len(), 4, "two leaf layouts and one concrete client per forwarder share three bodies");
                        assert_eq!(slots, [0, 1].into_iter().collect(), "one body uses the two original physical layouts");
                    });
                    let counts = crate::loader::module_load_counters();
                    assert_eq!(counts.successful_reads.values().sum::<usize>(), 3);
                    assert_eq!(counts.parsed_modules.values().sum::<usize>(), 3);
                    assert_eq!(counts.resolved_edges.values().sum::<usize>(), 4);
                    assert_eq!(counts.reused_modules.get(&module_key(&fixture.root.join("leaf.xsh"))), Some(&1));
                    fixture.assert_diamond_source_counts();
                    let source_counts = Checker::module_reuse_counters();
                    assert_eq!(execute(prepared, recursive), (0, b"narrow 17\n".to_vec(), Vec::new()));
                    assert_eq!(Checker::module_reuse_counters(), source_counts, "invocation cannot generate or solve declarations or instantiate module interfaces");
                }
            }
        });
    }

    #[test]
    fn independently_changed_dependency_produces_a_fresh_bundle_without_retraining_prepared_code() {
        crate::runtime::eval::run_eval(|| {
            for recursive in [false, true] {
                let fixture = ModuleFixture::new("fresh-dependency");
                fixture.diamond("##! An immutable generic dependency.\n## Preserve the supplied value.\nexport pure selected(value) { value }\n");
                let old = fixture.prepare("use left\nuse right\nprint ${left.forward(7)} ${right.forward(\"word\")}\n", |_| {});
                fixture.write("leaf.xsh", "##! A changed numeric dependency.\n## Increment the supplied value.\nexport pure selected(value) { value + 1 }\n");
                let fresh = fixture.prepare("use left\nuse right\nlet value: Int = left.forward(7)\nprint ${value}\n", |_| {});
                let source = "use left\nuse right\nlet rejected = right.forward(\"word\")\n";
                let (_, parsed) = fixture.load(source);
                let checked = Checker::check_arena(&parsed.arena, source);
                assert!(checked.diagnostics.iter().any(|diagnostic| matches!(diagnostic.code.as_deref(), Some("check.type-mismatch" | "check.type-relationship"))), "fresh clients see the changed definition: {:?}", checked.diagnostics);
                assert_eq!(execute(old, recursive), (0, b"7 word\n".to_vec(), Vec::new()));
                assert_eq!(execute(fresh, recursive), (0, b"8\n".to_vec(), Vec::new()));
            }
        });
    }

    #[test]
    fn repeated_import_aliases_share_schemes_and_private_nominals_after_frontend_drop() {
        crate::runtime::eval::run_eval(|| {
            let fixture = ModuleFixture::new("repeated-private-owner");
            let module = "##! A private nominal and a reusable exported scheme.\nenum Hidden { Entry(Int) }\n## Export the private owner's public spelling.\nexport type Public = Hidden\n## Preserve each independent client's type.\nexport pure identity(value) { value }\n## Build a value in this module.\nexport pure make(value: Int) -> Hidden { Entry(value) }\n## Read this module's value.\nexport pure read(value: Hidden) -> Int { match value { Entry(number) => number } }\n";
            fixture.write("alpha.xsh", module);
            fixture.write("beta.xsh", module);
            let source = "use alpha as first\nuse alpha as again\nuse beta as other\nlet number = first.identity(7)\nlet word = again.identity(\"word\")\nlet local: again.Public = first.make(number)\nlet foreign: other.Public = other.make(19)\nprint ${again.read(local)} ${other.read(foreign)} ${word}\n";
            for recursive in [false, true] {
                crate::loader::reset_module_load_counters();
                let prepared = fixture.prepare(source, |program| {
                    assert_eq!(program.function_count(), 6, "two aliases retain one body for each original exported definition");
                    let evidence = program.generic_evidence().unwrap();
                    let namespace = Name::intern(module_key(&fixture.root.join("alpha.xsh")));
                    let key = LoweredFunctionKey::Qualified(QualifiedName::new(namespace, Name::intern("identity")));
                    let view = program.function_view(key, LoweredFunctionKind::Pure).unwrap().unwrap();
                    let scope = view.generic_scope().unwrap();
                    assert_eq!(evidence.scopes().filter(|(id, _)| *id == scope).count(), 1);
                });
                let alpha = module_key(&fixture.root.join("alpha.xsh"));
                let loads = crate::loader::module_load_counters();
                assert_eq!(loads.successful_reads.get(&alpha), Some(&1));
                assert_eq!(loads.parsed_modules.get(&alpha), Some(&1));
                assert_eq!(loads.resolved_edges.get(&alpha), Some(&2));
                let checks = Checker::module_reuse_counters();
                assert!(checks.module_checks.values().all(|&count| count == 1));
                assert_eq!(checks.module_checks.len(), 2);
                assert_eq!(checks.module_check_completions, checks.module_checks);
                assert_eq!(checks.interface_publications, checks.module_checks);
                assert_eq!(checks.interface_imports[&alpha].len(), 2);
                assert_eq!(checks.declaration_generations, checks.declaration_checks);
                assert_eq!(checks.declaration_generalizations, checks.declaration_checks);
                assert!(checks.declaration_checks.values().all(|&count| count == 1));
                let read = prepared.0.indexed_program.as_ref().unwrap().symbol_owner().with_current(|| {
                    LoweredFunctionKey::Qualified(QualifiedName::new(Name::intern(&alpha), Name::intern("read")))
                });
                assert_eq!(execute_observing(prepared, recursive, Some(read)), (0, b"7 19 word\n".to_vec(), Vec::new()));
                assert_eq!(Checker::module_reuse_counters(), checks);
            }
            let rejected = "use alpha as first\nuse alpha as again\nuse beta as other\nlet value = again.read(other.make(19))\n";
            let (_, parsed) = fixture.load(rejected);
            let checked = Checker::check_arena(&parsed.arena, rejected);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.type-mismatch")), "a repeated alias cannot admit another module's private owner: {:?}", checked.diagnostics);
        });
    }

    #[test]
    fn real_module_keys_preserve_private_nominal_owners_and_reject_cross_module_values() {
        crate::runtime::eval::run_eval(|| {
            let fixture = ModuleFixture::new("private-nominals");
            let module = "##! A private nominal owner.\nenum Hidden { Entry(Int) }\n## An exported view of the same private nominal.\nexport type Public = Hidden\n## Build a value belonging to this module.\nexport pure make(value: Int) -> Hidden { Entry(value) }\n## Read only this module's nominal value.\nexport pure read(value: Hidden) -> Int { match value { Entry(number) => number } }\n";
            fixture.write("alpha.xsh", module);
            fixture.write("beta.xsh", module);
            let source = "use alpha\nuse beta\nlet first: alpha.Public = alpha.make(7)\nlet second: beta.Public = beta.make(8)\nlet left: Int = alpha.read(first)\nlet right: Int = beta.read(second)\n";
            let (_, parsed) = fixture.load(source);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let owners = checked.solved.nominals.values().filter_map(|identity| match identity {
                crate::sema::check::QualifiedNominalIdentity::Source { source, namespace: Some(namespace), .. } => Some((*source, *namespace)),
                _ => None,
            }).collect::<std::collections::BTreeSet<_>>();
            assert_eq!(owners.len(), 2, "equal hidden spellings and local spans retain two declaring owners");
            let calls = checked.solved.calls.keys().copied().collect::<Vec<_>>();
            drop(parsed);
            checked.solved.validate().unwrap();
            let before = checked.solved.graph.counters().clone();
            let query = SolvedQuery::new(&checked.solved, checked.solved.symbol_owner());
            for call in calls { query.call_binding(call).unwrap(); }
            assert_eq!(checked.solved.graph.counters(), &before);
            for (source, code) in [
                ("use alpha\nuse beta\nlet value = alpha.read(beta.make(8))\n", "check.type-mismatch"),
                ("use alpha\nlet value: alpha.Hidden = alpha.make(7)\n", "check.unknown-type"),
            ] {
                let (_, parsed) = fixture.load(source);
                let checked = Checker::check_arena(&parsed.arena, source);
                assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some(code)), "{source}: {:?}", checked.diagnostics);
            }
        });
    }
}
