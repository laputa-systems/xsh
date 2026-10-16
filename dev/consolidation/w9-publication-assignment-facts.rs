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
            program.source_text_source_id(),
        );
        let mut owners = Vec::new();
        if let Some(source_id) = range_source_id(program, view.root_statements())
            .or_else(|| program.source_text_source_id())
        {
            let source_path = source_name(sources, source_id)?;
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
                source_id,
                statements: view.root_statements(),
                identity,
                names,
            });
        }

        for module in view.modules() {
            let Some(source_id) = range_source_id(program, module.statements) else {
                continue;
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
