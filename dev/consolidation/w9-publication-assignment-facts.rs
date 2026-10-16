pub struct W9AssignmentFactsBinding<'a> {
    pub source_commit: &'a str,
    pub source_tree: &'a str,
    pub helper_sha256: &'a str,
    pub corpus_digest: &'a str,
    pub input_index: usize,
    pub input_path: &'a str,
}

struct W9AssignmentFactsOwner {
    source_id: xsh::frontend::source::SourceId,
    statements: xsh::frontend::syntax::arena::ArenaRange,
    identity: String,
    names: Vec<String>,
}

pub fn w9_assignment_facts_tsv(
    program: &xsh::frontend::syntax::arena::ArenaProgram,
    sources: &xsh::frontend::source::SourceMap,
    root_source_id: xsh::frontend::source::SourceId,
    binding: &W9AssignmentFactsBinding<'_>,
) -> Result<String, String> {
    use std::fmt::Write as _;
    use xsh::frontend::check::CheckBundleView;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::symbols::Name;
    use xsh::frontend::syntax::arena::{
        ArenaAssignTargetKind, ArenaRange, ArenaStmtKind, ArenaSugarOperand, AssignTargetId,
        BlockId, StmtId,
    };

    fn hex(value: &str) -> String {
        value
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn source_name(
        sources: &xsh::frontend::source::SourceMap,
        source_id: SourceId,
    ) -> Result<String, String> {
        sources
            .get(source_id)
            .map(|source| source.name().to_owned())
            .ok_or_else(|| format!("assignment owner names unknown source id {}", source_id.raw()))
    }

    fn range_source_id(
        program: &xsh::frontend::syntax::arena::ArenaProgram,
        statements: ArenaRange,
    ) -> Option<SourceId> {
        program
            .arena
            .stmt_ids(statements)
            .next()
            .map(|statement| program.arena.stmt(statement).span.source_id)
            .or_else(|| program.module_doc_for(statements).map(|span| span.source_id))
    }

    fn module_source_id(
        sources: &xsh::frontend::source::SourceMap,
        module: &xsh::frontend::syntax::arena::ArenaUserModule,
    ) -> Result<SourceId, String> {
        let physical_key = match module.key.split_once('\0') {
            Some((physical_key, context_id))
                if !physical_key.is_empty()
                    && !context_id.is_empty()
                    && context_id.bytes().all(|byte| byte.is_ascii_digit()) =>
            {
                physical_key
            }
            Some(_) => {
                return Err(format!(
                    "assignment module key has an invalid context suffix for `{}`",
                    module.name.as_str().as_str()
                ));
            }
            None => module.key.as_str(),
        };
        let mut matching_sources = sources.files().iter().filter(|source| {
            xsh::frontend::load::module_key(std::path::Path::new(source.name())) == physical_key
        });
        let source_id = matching_sources
            .next()
            .map(|source| source.id())
            .ok_or_else(|| {
                format!(
                    "assignment module has no source matching its physical key `{physical_key}`"
                )
            })?;
        if matching_sources.next().is_some() {
            return Err(format!(
                "assignment module physical key `{physical_key}` matches multiple sources"
            ));
        }
        Ok(source_id)
    }

    fn legacy_assignment_target_root_name(
        program: &xsh::frontend::syntax::arena::ArenaProgram,
        mut target: AssignTargetId,
    ) -> Option<Name> {
        loop {
            match program.arena.assign_target(target).kind {
                ArenaAssignTargetKind::Name(name) => return Some(name),
                ArenaAssignTargetKind::Field { base, .. }
                | ArenaAssignTargetKind::Index { base, .. } => target = base,
                ArenaAssignTargetKind::Env(_) => return None,
            }
        }
    }

    fn legacy_assignment_block(
        program: &xsh::frontend::syntax::arena::ArenaProgram,
        block: BlockId,
        names: &mut std::collections::BTreeSet<Name>,
    ) {
        let statements = program.arena.block(block).statements;
        for statement in program.arena.stmt_ids(statements) {
            legacy_assignment_statement(program, statement, names);
        }
    }

    fn legacy_assignment_statement(
        program: &xsh::frontend::syntax::arena::ArenaProgram,
        statement: StmtId,
        names: &mut std::collections::BTreeSet<Name>,
    ) {
        let kind = program.arena.stmt(statement).kind;
        match kind {
            ArenaStmtKind::Use(_)
            | ArenaStmtKind::TypeDef(_)
            | ArenaStmtKind::ErrorDef(_)
            | ArenaStmtKind::Const { .. }
            | ArenaStmtKind::Let { .. }
            | ArenaStmtKind::Var { .. }
            | ArenaStmtKind::Return(_)
            | ArenaStmtKind::Yield(_)
            | ArenaStmtKind::YieldDelegate(_)
            | ArenaStmtKind::Defer(..)
            | ArenaStmtKind::Assert { .. }
            | ArenaStmtKind::Break { .. }
            | ArenaStmtKind::Continue
            | ArenaStmtKind::Exit(_)
            | ArenaStmtKind::Command(_)
            | ArenaStmtKind::TailBareIdent(_)
            | ArenaStmtKind::Expr(_) => {}
            ArenaStmtKind::Assign { target, .. } => {
                if let Some(name) = legacy_assignment_target_root_name(program, target) {
                    names.insert(name);
                }
            }
            ArenaStmtKind::Export(inner) => legacy_assignment_statement(program, inner, names),
            ArenaStmtKind::ProcDef(def)
            | ArenaStmtKind::CliMain(def)
            | ArenaStmtKind::PureDef(def)
            | ArenaStmtKind::StreamDef(def) => {
                legacy_assignment_block(program, program.arena.function_def(def).body, names)
            }
            ArenaStmtKind::SignalHook(hook) => {
                legacy_assignment_block(program, program.arena.signal_hook(hook).body, names)
            }
            ArenaStmtKind::If {
                branches,
                else_block,
            } => {
                for branch in program.arena.if_branches(branches) {
                    legacy_assignment_block(program, branch.block, names);
                }
                if let Some(block) = else_block {
                    legacy_assignment_block(program, block, names);
                }
            }
            ArenaStmtKind::While { block, .. }
            | ArenaStmtKind::For { block, .. }
            | ArenaStmtKind::Loop { block } => legacy_assignment_block(program, block, names),
            ArenaStmtKind::Sugar { operands, .. } => {
                for operand in program.arena.sugar_operands(operands) {
                    match *operand {
                        ArenaSugarOperand::Block(block) => {
                            legacy_assignment_block(program, block, names)
                        }
                        ArenaSugarOperand::Stmt(statement) => {
                            legacy_assignment_statement(program, statement, names)
                        }
                        ArenaSugarOperand::Expr(_)
                        | ArenaSugarOperand::BindingTarget(_)
                        | ArenaSugarOperand::TypeExpr(_)
                        | ArenaSugarOperand::Name(_) => {}
                    }
                }
            }
            ArenaStmtKind::Guard { else_block, .. } => {
                legacy_assignment_block(program, else_block, names)
            }
            ArenaStmtKind::Match { arms, .. } => {
                for arm in program.arena.match_arms(arms) {
                    legacy_assignment_block(program, arm.block, names);
                }
            }
            ArenaStmtKind::With {
                body, else_block, ..
            } => {
                legacy_assignment_block(program, body, names);
                legacy_assignment_block(program, else_block, names);
            }
        }
    }

    program.symbol_owner().with_current(|| {
        let view = CheckBundleView::for_root(
            program,
            program.statements,
            Some(sources),
            Some(root_source_id),
        );
        let mut owners = Vec::new();
        if range_source_id(program, view.root_statements())
            .is_some_and(|parsed_source_id| parsed_source_id != root_source_id)
        {
            return Err("assignment root source id differs from parsed root".to_owned());
        }
        let source_path = source_name(sources, root_source_id)?;
        let identity = format!("root\0{source_path}");
        let mut names = std::collections::BTreeSet::new();
        for statement in program.arena.stmt_ids(view.root_statements()) {
            legacy_assignment_statement(program, statement, &mut names);
        }
        let mut names = names
            .into_iter()
            .map(|name| name.as_str().as_str().to_owned())
            .collect::<Vec<_>>();
        names.sort();
        owners.push(W9AssignmentFactsOwner {
            source_id: root_source_id,
            statements: view.root_statements(),
            identity,
            names,
        });

        for module in view.modules() {
            let source_id = match range_source_id(program, module.statements) {
                Some(source_id) => source_id,
                None => module_source_id(sources, module)?,
            };
            let source_path = source_name(sources, source_id)?;
            let module_name = module.name.as_str().as_str().to_owned();
            let identity = format!(
                "module\0{source_path}\0{}\0{module_name}\0{}",
                module.key,
                u8::from(module.internal)
            );
            let mut names = std::collections::BTreeSet::new();
            for statement in program.arena.stmt_ids(module.statements) {
                legacy_assignment_statement(program, statement, &mut names);
            }
            let mut names = names
                .into_iter()
                .map(|name| name.as_str().as_str().to_owned())
                .collect::<Vec<_>>();
            names.sort();
            owners.push(W9AssignmentFactsOwner {
                source_id,
                statements: module.statements,
                identity,
                names,
            });
        }

        if owners.iter().filter(|owner| owner.identity.starts_with("root\0")).count() != 1 {
            return Err("assignment proof requires one source-backed file root".to_owned());
        }
        owners.sort_by(|left, right| {
            (
                left.source_id.raw(),
                left.statements.start,
                left.statements.len,
                &left.identity,
            )
                .cmp(&(
                    right.source_id.raw(),
                    right.statements.start,
                    right.statements.len,
                    &right.identity,
                ))
        });
        let mut keys = std::collections::BTreeMap::new();
        let mut identities = std::collections::BTreeSet::new();
        for owner in &owners {
            let key = (
                owner.source_id.raw(),
                owner.statements.start,
                owner.statements.len,
            );
            if !identities.insert(owner.identity.as_str()) {
                return Err(format!("duplicate semantic assignment owner {}", owner.identity));
            }
            if let Some(existing) = keys.get(&key)
                && existing != &owner.names
            {
                return Err(format!("legacy assignment owners disagree for {key:?}"));
            }
            keys.insert(key, owner.names.clone());
        }

        let mut output = String::new();
        writeln!(&mut output, "w9-assignment-facts\t1").unwrap();
        writeln!(
            &mut output,
            "binding\t{}\t{}\t{}\t{}\t{}\t{}",
            binding.source_commit,
            binding.source_tree,
            binding.helper_sha256,
            binding.corpus_digest,
            binding.input_index,
            hex(binding.input_path)
        )
        .unwrap();
        for (index, source) in sources.files().iter().enumerate() {
            if source.id().raw() != index {
                return Err("source map ids are not contiguous".to_owned());
            }
            writeln!(
                &mut output,
                "source\t{}\t{}\t{}",
                source.id().raw(),
                hex(source.name()),
                source.text().len()
            )
            .unwrap();
        }
        let root_count = owners
            .iter()
            .filter(|owner| owner.identity.starts_with("root\0"))
            .count();
        let module_count = owners.len() - root_count;
        writeln!(
            &mut output,
            "counts\t{}\t{}\t{}",
            sources.files().len(),
            root_count,
            module_count
        )
        .unwrap();
        for owner in owners {
            let key = (
                owner.source_id.raw(),
                owner.statements.start,
                owner.statements.len,
            );
            let names = owner
                .names
                .iter()
                .map(|name| hex(name))
                .collect::<Vec<_>>()
                .join(",");
            writeln!(
                &mut output,
                "owner\t{}\t{}\t{}\t{}\t{}",
                key.0,
                key.1,
                key.2,
                hex(&owner.identity),
                names
            )
            .unwrap();
        }
        Ok(output)
    })
}

#[cfg(test)]
mod tests {
    use super::{W9AssignmentFactsBinding, w9_assignment_facts_tsv};
    use std::collections::BTreeSet;
    use xsh::frontend::source::{SourceId, SourceMap};
    use xsh::frontend::syntax::arena::{
        ArenaProgramBuilder, ArenaRange, ArenaStmtKind,
    };
    use xsh::frontend::syntax::parser::Parser;

    fn binding() -> W9AssignmentFactsBinding<'static> {
        W9AssignmentFactsBinding {
            source_commit: "1111111111111111111111111111111111111111",
            source_tree: "2222222222222222222222222222222222222222",
            helper_sha256: "3333333333333333333333333333333333333333333333333333333333333333",
            corpus_digest: "4444444444444444444444444444444444444444444444444444444444444444",
            input_index: 0,
            input_path: "fixtures/assignment-facts.xsh",
        }
    }

    fn owner_rows(output: &str) -> Vec<Vec<&str>> {
        output
            .lines()
            .filter_map(|line| {
                let fields = line.split('\t').collect::<Vec<_>>();
                (fields.first() == Some(&"owner")).then_some(fields)
            })
            .collect()
    }

    fn owner_names(
        output: &str,
        source_id: SourceId,
        statements: ArenaRange,
    ) -> BTreeSet<String> {
        let source = source_id.raw().to_string();
        let start = statements.start.to_string();
        let length = statements.len.to_string();
        let row = owner_rows(output)
            .into_iter()
            .find(|fields| fields.len() == 6
                && fields[1] == source
                && fields[2] == start
                && fields[3] == length)
            .expect("owner row for source and statement range");
        if row[5].is_empty() {
            return BTreeSet::new();
        }
        row[5]
            .split(',')
            .map(|name| {
                let bytes = name
                    .as_bytes()
                    .chunks_exact(2)
                    .map(|pair| {
                        u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()
                    })
                    .collect::<Vec<_>>();
                String::from_utf8(bytes).unwrap()
            })
            .collect()
    }

    fn expected_names(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn legacy_target_roots_collapse_field_index_and_duplicate_assignments() {
        let source = concat!(
            "simple = 1\n",
            "root = 2\n",
            "root.field = 3\n",
            "root[0] = 4\n",
            "e\"XSH_ASSIGNMENT_FACT\" = 5\n",
        );
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("assignment-roots.xsh", source);
        let mut builder = ArenaProgramBuilder::with_token_capacity(96);
        let parsed = Parser::parse_source_into_arena_builder(source_id, source, &mut builder);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let statements = parsed.statements;
        let program = builder.finish_with_statements(statements);
        let proof = w9_assignment_facts_tsv(&program, &sources, source_id, &binding()).unwrap();
        assert_eq!(owner_rows(&proof).len(), 1);
        assert_eq!(
            owner_names(&proof, source_id, statements),
            expected_names(&["root", "simple"])
        );
    }

    #[test]
    fn legacy_collection_follows_raw_sugar_and_skips_expression_and_defer_bodies() {
        let source = concat!(
            "root.field = 1\n",
            "items[0] = 2\n",
            "e\"XSH_ASSIGNMENT_FACT\" = 3\n",
            "let result = try {\n",
            "  nested = 4\n",
            "  0\n",
            "}\n",
            "atomically replace output as temporary {\n",
            "  repeat 1 times { atomic_written = 6 }\n",
            "}\n",
            "total = run.text make ? when ready\n",
            "wait until ready within 5s backoff 1s..4s\n",
            "let expression_result = try {\n",
            "  atomically replace expression_output as expression_temp { expression_hidden = 7 }\n",
            "  0\n",
            "}\n",
            "proc show() {\n",
            "  defer {\n",
            "    atomically replace deferred_output as deferred_temp { deferred_hidden = 8 }\n",
            "  }\n",
            "}\n",
        );
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("assignment-sugar.xsh", source);
        let mut builder = ArenaProgramBuilder::with_token_capacity(256);
        let parsed = Parser::parse_source_into_arena_builder(source_id, source, &mut builder);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let statements = parsed.statements;
        let program = builder.finish_with_statements(statements);
        let proof = w9_assignment_facts_tsv(&program, &sources, source_id, &binding()).unwrap();
        assert_eq!(
            owner_names(&proof, source_id, statements),
            expected_names(&["atomic_written", "items", "root", "total"])
        );
    }

    #[test]
    fn source_backed_empty_root_keeps_an_explicit_empty_owner() {
        let source = "##! Empty assignment root.\n";
        let mut sources = SourceMap::new();
        let preceding_source_id = sources.add_file("preceding.xsh", "unused = 1\n");
        let source_id = sources.add_file("empty-assignment-root.xsh", source);
        assert_ne!(source_id, preceding_source_id);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let statements = parsed.arena.statements;
        let program = parsed.arena;
        let proof = w9_assignment_facts_tsv(&program, &sources, source_id, &binding()).unwrap();
        let rows = owner_rows(&proof);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][1].parse::<usize>().unwrap(), source_id.raw());
        assert_eq!(owner_names(&proof, source_id, statements), BTreeSet::new());
    }

    #[test]
    fn imported_module_names_remain_separate_from_the_root_owner() {
        let root_source = "use child\n";
        let module_source = "nested = 1\n";
        let mut sources = SourceMap::new();
        let root_id = sources.add_file("assignment-root.xsh", root_source);
        let module_id = sources.add_file("assignment-child.xsh", module_source);
        let mut builder = ArenaProgramBuilder::with_token_capacity(64);
        let root =
            Parser::parse_source_into_arena_builder(root_id, root_source, &mut builder);
        assert!(root.diagnostics.is_empty(), "{:?}", root.diagnostics);
        let use_statement = builder
            .ast_arena()
            .stmt_ids(root.statements)
            .next()
            .expect("root import statement");
        let ArenaStmtKind::Use(use_id) = builder.ast_arena().stmt(use_statement).kind else {
            panic!("first root statement is the child import");
        };
        builder.set_use_resolved(use_id, std::sync::Arc::from("child-key"));
        let module_name = builder.name("child");
        let module =
            Parser::parse_source_into_arena_builder(module_id, module_source, &mut builder);
        assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
        builder.push_arena_module("child-key".to_owned(), module_name, module.statements);
        let root_statements = root.statements;
        let module_statements = module.statements;
        let program = builder.finish_with_statements(root_statements);
        let proof = w9_assignment_facts_tsv(&program, &sources, root_id, &binding()).unwrap();
        assert_eq!(owner_rows(&proof).len(), 2);
        assert_eq!(owner_names(&proof, root_id, root_statements), BTreeSet::new());
        assert_eq!(
            owner_names(&proof, module_id, module_statements),
            expected_names(&["nested"])
        );
    }

    #[test]
    fn source_backed_empty_imported_module_keeps_an_explicit_empty_owner() {
        let root_source = "use child\nroot = 1\n";
        let mut sources = SourceMap::new();
        let root_id = sources.add_file("assignment-root.xsh", root_source);
        let module_id = sources.add_file("assignment-child.xsh", "");
        let module_key = xsh::frontend::load::module_key(std::path::Path::new(
            sources.get(module_id).unwrap().name(),
        ));
        let mut builder = ArenaProgramBuilder::with_token_capacity(64);
        let root = Parser::parse_source_into_arena_builder(root_id, root_source, &mut builder);
        assert!(root.diagnostics.is_empty(), "{:?}", root.diagnostics);
        let use_statement = builder
            .ast_arena()
            .stmt_ids(root.statements)
            .next()
            .expect("root import statement");
        let ArenaStmtKind::Use(use_id) = builder.ast_arena().stmt(use_statement).kind else {
            panic!("first root statement is the child import");
        };
        builder.set_use_resolved(use_id, std::sync::Arc::from(module_key.as_str()));
        let module_name = builder.name("child");
        let module = Parser::parse_source_into_arena_builder(module_id, "", &mut builder);
        assert!(module.diagnostics.is_empty(), "{:?}", module.diagnostics);
        let root_statements = root.statements;
        let module_statements = module.statements;
        builder.push_arena_module(module_key, module_name, module_statements);
        let program = builder.finish_with_statements(root_statements);

        let proof = w9_assignment_facts_tsv(&program, &sources, root_id, &binding()).unwrap();
        assert_eq!(owner_rows(&proof).len(), 2);
        assert_eq!(
            owner_names(&proof, root_id, root_statements),
            expected_names(&["root"])
        );
        assert_eq!(
            owner_names(&proof, module_id, module_statements),
            BTreeSet::new()
        );
    }
}
