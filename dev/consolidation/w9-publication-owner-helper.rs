use std::io::Write as _;

pub fn w9_write_adapter_owners(
    outputdir: &std::path::Path,
    index: usize,
    program: &xsh::frontend::syntax::arena::ArenaProgram,
    sources: &xsh::frontend::source::SourceMap,
) -> std::io::Result<()> {
    let mut owner_rows = std::collections::BTreeSet::new();
    std::fs::create_dir_all(outputdir)?;
    let path = outputdir.join(format!("owners-{index}.tsv"));
    let mut output = std::io::BufWriter::new(std::fs::File::create(path)?);

    for source in sources.files() {
        writeln!(output, "source\t{:?}\t{}", source.id(), source.text().len())?;
    }

    for raw in 0..program.arena.stmt_tags.len() {
        let id = xsh::frontend::syntax::arena::StmtId::from_index(raw);
        let statement = program.arena.stmt(id);
        let kind = program.arena.stmt_tags[raw];
        owner_rows.insert((statement.span, format!("stmt:{kind:?}")));
        writeln!(output, "stmt\t{id:?}\t{:?}\tstmt:{kind:?}", statement.span)?;
        if let xsh::frontend::syntax::arena::ArenaStmtKind::TailBareIdent(name) = statement.kind {
            let spelling = program
                .symbol_owner()
                .with_current(|| name.as_str().as_str().to_owned());
            writeln!(
                output,
                "tail\t{id:?}\t{:?}\t{spelling}\tstmt:TailBareIdent",
                statement.span
            )?;
        }
    }

    for raw in 0..program.arena.expr_tags.len() {
        let expression = program
            .arena
            .expr(xsh::frontend::syntax::arena::ExprId::from_index(raw));
        owner_rows.insert((
            expression.span,
            format!("expr:{:?}", program.arena.expr_tags[raw]),
        ));
    }

    for stage in &program.arena.pipe_stages {
        let span = program.arena.span(stage.span);
        let kind = match &stage.kind {
            xsh::frontend::syntax::arena::ArenaPipeStageKind::Expr(_) => "stage:Expr".to_owned(),
            xsh::frontend::syntax::arena::ArenaPipeStageKind::Stream(stream) => {
                format!("stage:{:?}", stream.kind)
            }
        };
        owner_rows.insert((span, kind));
    }

    for (span, kind) in owner_rows {
        writeln!(output, "owner\t{span:?}\t{kind}")?;
    }

    output.flush()
}
