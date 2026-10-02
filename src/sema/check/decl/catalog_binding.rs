use super::*;

/// Only original parameter positions in an authenticated catalog module may
/// retain labels that otherwise conflict with a standard module binding.
#[derive(Clone, Default)]
pub(in crate::sema::check) struct CatalogParameterBindings {
    bindings: Vec<(String, Span)>,
}

impl CatalogParameterBindings {
    pub(in crate::sema::check) fn for_module(program: &ArenaProgram, module: &ArenaUserModule) -> Self {
        let Some((catalog, source)) = module.canonical_stdlib_source() else { return Self::default(); };
        if !module.internal || crate::stdlib::find_by_namespace(&module.name.as_str()).is_none_or(|selected| !std::ptr::eq(selected, catalog)) {
            return Self::default();
        }
        let mut bindings = Vec::new();
        for statement in program.module_statements(module) {
            let mut kind = program.arena.stmt(statement).kind;
            if let ArenaStmtKind::Export(inner) = kind { kind = program.arena.stmt(inner).kind; }
            let definition = match kind {
                ArenaStmtKind::PureDef(id) | ArenaStmtKind::ProcDef(id) | ArenaStmtKind::StreamDef(id) => program.arena.function_def(id),
                _ => continue,
            };
            for parameter in program.arena.params(definition.params) {
                let span = program.arena.span(parameter.span);
                if span.source_id == source && module.canonical_parameter_binding_matches(&program.arena, parameter) {
                    bindings.push((parameter.name.as_str().as_str().to_owned(), span));
                }
            }
        }
        Self { bindings }
    }

    pub(in crate::sema::check) fn contains(&self, name: &str, span: Span) -> bool {
        self.bindings.iter().any(|(original, position)| original == name && *position == span)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_hash_parameter_keeps_its_original_public_path_label() {
        crate::runtime::eval::run_eval(|| {
            let (_, parsed) = crate::loader::prepare_stdlib_catalog_module("hash").unwrap();
            assert!(parsed.diagnostics.is_empty());
            let checked = Checker::check_compact_declarations(&parsed.arena);
            assert!(!checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.standard-module-shadow")), "{:?}", checked.diagnostics);
        });
    }

    #[test]
    fn catalog_hash_file_body_uses_the_original_file_digest_permission() {
        crate::runtime::eval::run_eval(|| {
            let (_, parsed) = crate::loader::prepare_stdlib_catalog_module("hash").unwrap();
            assert!(parsed.diagnostics.is_empty());
            let checked = Checker::check_compact_declarations(&parsed.arena);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            parsed.arena.symbol_owner().with_current(|| {
                let namespace = parsed.arena.modules.iter().find(|module| module.canonical_stdlib_source().is_some_and(|(catalog, _)| catalog.identity == "hash")).unwrap().name;
                let (identity, declaration) = checked.solved.declarations.iter().find(|(identity, _)| identity.namespace == Some(namespace)
                    && parsed.arena.arena.function_def(identity.declaration).name == "verify_file").unwrap();
                assert_eq!(declaration.kind, crate::sema::inference::CallableKind::Proc);
                assert_eq!(declaration.effective_effects, crate::sema::inference::EffectSummary::Closed(crate::sema::inference::EffectSet::FS));
                let graph = &checked.solved.graph;
                let crate::sema::inference::TypeNode::Arrow(signature) = graph.node(graph.resolved(declaration.signature).unwrap()).unwrap() else { panic!("the original catalog declaration is callable"); };
                assert_eq!(signature.params[0].label, "path");
                assert_eq!(graph.export_type(signature.params[0].ty).unwrap(), Type::Path);
                assert_eq!(graph.export_type(signature.result).unwrap(), Type::Result(Box::new(Type::Unit), Box::new(Type::Error)));
                assert!(checked.solved.expression_owners.values().any(|owner| owner == identity), "the file verifier owns genuinely checked body expressions");
                assert!(checked.solved.calls.values().any(|call| call.caller == Some(*identity)), "the verifier's checked calls retain their original private caller");
            });
        });
    }

    #[test]
    fn internal_flags_and_catalog_namespace_spellings_do_not_admit_reserved_parameters() {
        crate::runtime::eval::run_eval(|| {
            let source = crate::stdlib::find("hash").unwrap().source;
            for namespace in ["<xsh-stdlib:hash>", "hash", "user-hash"] {
                let mut builder = crate::syntax::arena::ArenaProgramBuilder::with_token_capacity(256);
                let entry = crate::syntax::parser::Parser::parse_source_into_arena_builder(crate::source::SourceId::new(0), "", &mut builder);
                let module = crate::syntax::parser::Parser::parse_source_into_arena_builder(crate::source::SourceId::new(1), source, &mut builder);
                assert!(entry.diagnostics.is_empty() && module.diagnostics.is_empty());
                let name = builder.name(namespace);
                builder.push_internal_arena_module("<xsh-stdlib:hash>".to_owned(), name, module.statements);
                let program = builder.finish_with_statements(entry.statements);
                let checked = Checker::check_compact_declarations(&program);
                assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.standard-module-shadow")), "a catalog spelling cannot admit a user-owned parameter: {:?}", checked.diagnostics);
            }
            let source = "pure caller(path: Path) -> Path { path }\n";
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
            assert!(parsed.diagnostics.is_empty());
            let checked = Checker::check_compact_declarations(&parsed.arena);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.standard-module-shadow")));
            let source = "use hash\npure caller(path: Path) -> Path { path }\n";
            let (_, parsed) = crate::loader::parse_load_entry_source_arena_only("catalog-binding-scope.xsh", crate::loader::entry_source_from_text("catalog-binding-scope.xsh", source.to_owned()), Vec::new());
            assert!(parsed.diagnostics.is_empty());
            let entry_source = parsed.arena.arena.stmt(parsed.arena.statement_ids().next().unwrap()).span.source_id;
            let checked = Checker::check_compact_declarations(&parsed.arena);
            assert!(checked.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("check.standard-module-shadow") && diagnostic.labels.iter().any(|label| label.span.source_id == entry_source)), "catalog parameter authority does not survive into the user module: {:?}", checked.diagnostics);
        });
    }

    #[test]
    fn catalog_parameter_authority_refuses_rewritten_types_defaults_and_rest_flags() {
        crate::runtime::eval::run_eval(|| {
            let (_, parsed) = crate::loader::prepare_stdlib_catalog_module("hash").unwrap();
            let program = parsed.arena;
            let symbols = program.symbol_owner().clone();
            symbols.with_current(|| {
                let module = program.modules.iter().find(|module| module.canonical_stdlib_source().is_some_and(|(catalog, _)| catalog.identity == "hash")).unwrap();
                let parameter = program.arena.params.iter().position(|parameter| parameter.name == "path").unwrap();
                let span = program.arena.span(program.arena.params[parameter].span);
                assert!(CatalogParameterBindings::for_module(&program, module).contains("path", span));
                let other_type = program.arena.params.iter().find(|parameter| parameter.name == "checksum").unwrap().ty;
                for change in 0..3 {
                    let mut changed = program.clone();
                    match change {
                        0 => changed.arena.params[parameter].ty = other_type,
                        1 => changed.arena.params[parameter].default = Some(crate::syntax::arena::ExprId::from_index(0)),
                        _ => changed.arena.params[parameter].rest = true,
                    }
                    let module = changed.modules.iter().find(|module| module.canonical_stdlib_source().is_some_and(|(catalog, _)| catalog.identity == "hash")).unwrap();
                    assert!(!CatalogParameterBindings::for_module(&changed, module).contains("path", span), "matching name and span cannot replace the original formal contract");
                }
            });
        });
    }
}
