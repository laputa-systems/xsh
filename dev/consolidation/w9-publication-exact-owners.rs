pub fn w9_write_exact_adapter_owners(
    outputdir: &std::path::Path,
    index: usize,
    program: &xsh::frontend::syntax::arena::ArenaProgram,
    sources: &xsh::frontend::source::SourceMap,
) -> std::io::Result<()> {
    use std::io::Write as _;
    use xsh::frontend::syntax::arena::{
        ArenaBindingTargetKind, ArenaCallArgKind, ArenaExprKind, ArenaExprOrRun, ArenaPipeStageKind,
        ArenaRecordFieldKind, ArenaStmtKind, ExprId, StmtId,
    };

    let path = format!("owners-{index}.tsv");
    let directory = w9_exact_open_directory(outputdir)?;
    let file = w9_exact_create_file_at(&directory, &path)?;
    let mut output = std::io::BufWriter::new(file);
    let mut owner_rows = std::collections::BTreeSet::new();

    for source in sources.files() {
        writeln!(output, "source\t{:?}\t{}", source.id(), source.text().len())?;
    }

    for raw in 0..program.arena.stmt_tags.len() {
        let id = StmtId::from_index(raw);
        let statement = program.arena.stmt(id);
        let kind = program.arena.stmt_tags[raw];
        owner_rows.insert((statement.span, format!("stmt:{kind:?}")));
        writeln!(output, "stmt\t{id:?}\t{:?}\tstmt:{kind:?}", statement.span)?;
        if let ArenaStmtKind::TailBareIdent(name) = &statement.kind {
            let spelling = program
                .symbol_owner()
                .with_current(|| name.as_str().as_str().to_owned());
            w9_exact_validate_tsv_text(&spelling)?;
            writeln!(
                output,
                "tail\t{id:?}\t{:?}\t{spelling}\tstmt:TailBareIdent",
                statement.span
            )?;
        }
    }

    let (expression_rows, record_rows) = program.symbol_owner().with_current(|| {
        let expression_rows = (0..program.arena.expr_tags.len())
            .map(|raw| {
                let id = ExprId::from_index(raw);
                let expression = program.arena.expr(id);
                let ident = match &expression.kind {
                    ArenaExprKind::Ident(name) => name.as_str().as_str().to_owned(),
                    _ => String::new(),
                };
                (id, expression.span, program.arena.expr_tags[raw], ident)
            })
            .collect::<Vec<_>>();
        let record_rows = (0..program.arena.expr_tags.len())
            .filter_map(|raw| {
                let id = ExprId::from_index(raw);
                let expression = program.arena.expr(id);
                let ArenaExprKind::Record(range) = &expression.kind else {
                    return None;
                };
                let fields = program.arena.record_fields(*range);
                let rows = fields
                    .iter()
                    .enumerate()
                    .map(|(offset, field)| {
                        let field_index = range.start as usize + offset;
                        let (kind, name, span_id, key, value, shorthand) = match &field.kind {
                            ArenaRecordFieldKind::Computed { key, value, span } => (
                                "computed",
                                format!("{:?}", String::new()),
                                *span,
                                Some(*key),
                                Some(*value),
                                None,
                            ),
                            ArenaRecordFieldKind::Path { path, value, span } => (
                                "path",
                                format!(
                                    "{:?}",
                                    program
                                        .arena
                                        .names(*path)
                                        .map(|name| name.as_str().as_str().to_owned())
                                        .collect::<Vec<_>>()
                                ),
                                *span,
                                None,
                                Some(*value),
                                None,
                            ),
                            ArenaRecordFieldKind::Named { name, value, span } => (
                                "named",
                                format!("{:?}", name.as_str().as_str()),
                                *span,
                                None,
                                Some(*value),
                                None,
                            ),
                            ArenaRecordFieldKind::Shorthand { name, span } => {
                                let spelling = name.as_str().as_str().to_owned();
                                (
                                    "shorthand",
                                    format!("{spelling:?}"),
                                    *span,
                                    None,
                                    None,
                                    Some(spelling),
                                )
                            }
                            ArenaRecordFieldKind::Spread { expr, span } => (
                                "spread",
                                format!("{:?}", String::new()),
                                *span,
                                None,
                                Some(*expr),
                                None,
                            ),
                        };
                        (
                            field_index,
                            kind,
                            name,
                            program.arena.span(span_id),
                            key,
                            value,
                            shorthand,
                        )
                    })
                    .collect::<Vec<_>>();
                Some((id, expression.span, rows))
            })
            .collect::<Vec<_>>();
        (expression_rows, record_rows)
    });
    let binding_initializers = program.symbol_owner().with_current(|| {
        (0..program.arena.stmt_tags.len())
            .filter_map(|raw| {
                let id = StmtId::from_index(raw);
                let statement = program.arena.stmt(id);
                let (target, initializer) = match statement.kind {
                    ArenaStmtKind::Const { target, initializer, .. }
                    | ArenaStmtKind::Let { target, initializer, .. }
                    | ArenaStmtKind::Var { target, initializer, .. } => (target, initializer),
                    _ => return None,
                };
                let ArenaExprOrRun::Expr(initializer) = initializer else {
                    return None;
                };
                let ArenaBindingTargetKind::Name(name) =
                    &program.arena.binding_target(target).kind
                else {
                    return None;
                };
                Some((id, name.as_str().as_str().to_owned(), initializer))
            })
            .collect::<Vec<_>>()
    });
    let structured_stages = program.symbol_owner().with_current(|| {
        program
            .arena
            .stream_stages
            .iter()
            .enumerate()
            .map(|(stage_id, stage)| {
                let arguments = program.arena.call_args(stage.args);
                let argument_rows = arguments
                    .iter()
                    .enumerate()
                    .map(|(argument_id, argument)| {
                        let (kind, name, expression) = match &argument.kind {
                            ArenaCallArgKind::Positional(expression) => ("Positional", None, *expression),
                            ArenaCallArgKind::NamedSpread { value, .. } => ("NamedSpread", None, *value),
                            ArenaCallArgKind::Splice { value, .. } => ("Splice", None, *value),
                            ArenaCallArgKind::Named { name, value, .. } => (
                                "Named",
                                Some(name.as_str().as_str().to_owned()),
                                *value,
                            ),
                        };
                        (argument_id, kind, name, expression)
                    })
                    .collect::<Vec<_>>();
                let accepts_callable =
                    xsh_registry::stream_parameters::stage_accepts_callable(stage.kind.as_str());
                let has_spread = arguments.iter().any(|argument| {
                    matches!(
                        &argument.kind,
                        ArenaCallArgKind::NamedSpread { .. } | ArenaCallArgKind::Splice { .. }
                    )
                });
                let named_blocks = arguments
                    .iter()
                    .enumerate()
                    .filter_map(|(argument_id, argument)| match &argument.kind {
                        ArenaCallArgKind::Named { name, value, .. }
                            if name.as_str().as_str() == "block" => Some((argument_id, *value)),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let positionals = arguments
                    .iter()
                    .enumerate()
                    .filter_map(|(argument_id, argument)| match &argument.kind {
                        ArenaCallArgKind::Positional(expression) => Some((argument_id, *expression)),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let callback = if accepts_callable && stage.block.is_none() && !has_spread {
                    match (named_blocks.as_slice(), positionals.as_slice()) {
                        ([block], positionals)
                            if positionals.iter().all(|(argument_id, _)| *argument_id > block.0) =>
                        {
                            Some(block.1)
                        }
                        ([], positionals) if positionals.first().is_some_and(|(argument_id, _)| *argument_id == 0) => {
                            positionals.first().map(|(_, expression)| *expression)
                        }
                        _ => None,
                    }
                } else {
                    None
                }
                .filter(|expression| {
                    matches!(
                        &program.arena.expr(*expression).kind,
                        ArenaExprKind::Ident(_) | ArenaExprKind::Field { .. }
                    )
                });
                (
                    stage_id,
                    stage.span,
                    format!("{:?}", stage.kind),
                    stage.block.is_some(),
                    accepts_callable,
                    argument_rows,
                    callback,
                )
            })
            .collect::<Vec<_>>()
    });

    for (_, span, tag, ident) in &expression_rows {
        let ident = format!("{ident:?}");
        w9_exact_validate_tsv_text(&ident)?;
        owner_rows.insert((*span, format!("expr:{tag:?}")));
    }

    for stage in &program.arena.pipe_stages {
        let span = program.arena.span(stage.span);
        let kind = match &stage.kind {
            ArenaPipeStageKind::Expr(_) => "stage:Expr".to_owned(),
            ArenaPipeStageKind::Stream(stream) => format!("stage:{:?}", stream.kind),
        };
        owner_rows.insert((span, kind));
    }

    for run_form in &program.arena.run_forms {
        owner_rows.insert((
            program.arena.span(run_form.span),
            "run:RunForm".to_owned(),
        ));
    }

    for (span, kind) in owner_rows {
        writeln!(output, "owner\t{span:?}\t{kind}")?;
    }

    for (id, span, tag, ident) in &expression_rows {
        let ident = format!("{ident:?}");
        w9_exact_validate_tsv_text(&ident)?;
        writeln!(output, "expr\t{id:?}\t{span:?}\t{tag:?}\t{ident}")?;
    }

    for raw in 0..program.arena.expr_tags.len() {
        let id = ExprId::from_index(raw);
        let expression = program.arena.expr(id);
        match &expression.kind {
            ArenaExprKind::Call { callee, .. } => {
                writeln!(output, "call\t{id:?}\t{callee:?}")?;
            }
            ArenaExprKind::Field { base, name } => {
                let name = program
                    .symbol_owner()
                    .with_current(|| name.as_str().as_str().to_owned());
                let name = format!("{name:?}");
                w9_exact_validate_tsv_text(&name)?;
                writeln!(output, "field-access\t{id:?}\t{base:?}\t{name}")?;
            }
            _ => {}
        }
    }

    for (raw, run_form) in program.arena.run_forms.iter().enumerate() {
        let id = xsh::frontend::syntax::arena::RunFormId::from_index(raw);
        let span = program.arena.span(run_form.span);
        let head = program
            .arena
            .run_segments(run_form.segments)
            .first()
            .ok_or_else(|| w9_exact_invalid_data("run form has no head segment"))?;
        writeln!(
            output,
            "run\t{id:?}\t{span:?}\t{}\t{:?}",
            run_form.propagate,
            head.kind
        )?;
    }

    let mut identifier_aliases = std::collections::BTreeMap::new();
    for (id, span, _, ident) in &expression_rows {
        if !ident.is_empty() {
            identifier_aliases
                .entry((*span, ident.clone()))
                .or_insert_with(Vec::new)
                .push(*id);
        }
    }
    let mut shorthand_aliases = std::collections::BTreeSet::new();
    let record_field_count = program.arena.record_fields.len();
    for (parent, span, fields) in &record_rows {
        writeln!(output, "record\t{parent:?}\t{span:?}\t{}", fields.len())?;
        for (field_index, kind, name, field_span, key, value, shorthand) in fields {
            w9_exact_validate_tsv_text(name)?;
            if *field_index >= record_field_count {
                return Err(w9_exact_invalid_data("record field index exceeds arena table"));
            }
            let key = key.map(|id| format!("{id:?}")).unwrap_or_default();
            let value = value.map(|id| format!("{id:?}")).unwrap_or_default();
            writeln!(
                output,
                "record-field\t{parent:?}\t{field_index}\t{kind}\t{name}\t{field_span:?}\t{key}\t{value}"
            )?;
            if let Some(shorthand) = shorthand {
                let aliases = identifier_aliases
                    .get(&(*field_span, shorthand.clone()))
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                let [alias] = aliases else {
                    return Err(w9_exact_invalid_data(format!(
                        "shorthand field {field_index} has {} exact identifier aliases",
                        aliases.len()
                    )));
                };
                if !shorthand_aliases.insert((*parent, *field_index, *alias)) {
                    return Err(w9_exact_invalid_data("duplicate shorthand alias link"));
                }
                let shorthand = format!("{shorthand:?}");
                w9_exact_validate_tsv_text(&shorthand)?;
                writeln!(
                    output,
                    "record-shorthand-alias\t{parent:?}\t{field_index}\t{alias:?}\t{field_span:?}\t{shorthand}"
                )?;
            }
        }
    }

    writeln!(
        output,
        "exact-counts\t{}\t{record_field_count}",
        expression_rows.len()
    )?;
    writeln!(output, "run-form-count\t{}", program.arena.run_forms.len())?;
    writeln!(output, "structured-stage-count\t{}", structured_stages.len())?;
    let callback_count = structured_stages
        .iter()
        .filter(|(_, _, _, _, _, _, callback)| callback.is_some())
        .count();
    writeln!(output, "structured-stage-callback-count\t{callback_count}")?;
    for (
        table_index,
        stage_span,
        kind,
        inline_block,
        accepts_callable,
        arguments,
        callback,
    ) in &structured_stages
    {
        let span = program.arena.span(*stage_span);
        writeln!(
            output,
            "structured-stage\t{table_index}\t{span:?}\t{kind}\t{inline_block}\t{accepts_callable}\t{}",
            arguments.len()
        )?;
        for (argument_index, kind, name, expression) in arguments {
            let name = match name {
                Some(name) => format!("{name:?}"),
                None => "-".to_owned(),
            };
            w9_exact_validate_tsv_text(&name)?;
            writeln!(
                output,
                "structured-stage-arg\t{table_index}\t{argument_index}\t{kind}\t{name}\t{expression:?}"
            )?;
        }
        if let Some(callback) = callback {
            writeln!(output, "structured-stage-callback\t{table_index}\t{callback:?}")?;
        }
    }
    writeln!(output, "binding-initializer-count\t{}", binding_initializers.len())?;
    for (statement, name, initializer) in binding_initializers {
        let name = format!("{name:?}");
        w9_exact_validate_tsv_text(&name)?;
        writeln!(output, "binding-initializer\t{statement:?}\t{name}\t{initializer:?}")?;
    }
    output.flush()?;
    let file = output
        .into_inner()
        .map_err(|error| error.into_error())?;
    file.sync_all()?;
    directory.sync_all()
}

fn w9_exact_assert_structured_stage_is_published(outputdir: &std::path::Path) {
    use xsh::frontend::load::{entry_source_from_text, parse_load_entry_source_arena_only};

    let source = "let values = [1, 2] |> drop(1) |> values\n";
    let (sources, parsed) = parse_load_entry_source_arena_only(
        "structured-stage-owner.xsh",
        entry_source_from_text("structured-stage-owner.xsh", source.to_owned()),
        Vec::new(),
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.arena.arena.stream_stages.len(), 1);
    assert_eq!(parsed.arena.arena.pipe_stages.len(), 1);

    let stage = &parsed.arena.arena.stream_stages[0];
    assert_eq!(format!("{:?}", stage.kind), "Drop");
    let span = parsed.arena.arena.span(stage.span);
    let pipe_stage_span = parsed.arena.arena.span(parsed.arena.arena.pipe_stages[0].span);
    assert_ne!(span, pipe_stage_span);
    let directory = tempfile::tempdir_in(outputdir)
        .expect("create structured-stage owner directory");
    w9_write_exact_adapter_owners(directory.path(), 0, &parsed.arena, &sources)
        .expect("write structured-stage owner index");
    let owners = std::fs::read_to_string(directory.path().join("owners-0.tsv"))
        .expect("read structured-stage owner index");
    assert!(
        owners.lines().any(|line| {
            line.starts_with(&format!("structured-stage\t0\t{span:?}\t{:?}\t", stage.kind))
        }),
        "structured stream stage is missing from the exact stage table: {owners}"
    );
    assert!(
        owners
            .lines()
            .any(|line| line == format!("owner\t{pipe_stage_span:?}\tstage:Expr")),
        "generic pipe-stage owner is missing from its own span: {owners}"
    );
    assert!(
        !owners.lines().any(|line| {
            line.starts_with(&format!("owner\t{span:?}\tstage:"))
        }),
        "structured stream stage escaped into the generic owner projection: {owners}"
    );
}

#[test]
fn w9_exact_repository_owners_receipt() {
    use std::io::Read as _;
    use walkdir::WalkDir;
    use xsh::frontend::load::{
        entry_source_from_text, parse_load_entry_source_arena_only, project_module_roots,
    };

    const EXPECTED_CORPUS_BLAKE3: &str =
        "a7b1f70466c4e978222af3d329e9c71b0a15030480e24b8d7f77a5fa4f57f17d";
    const SOURCE_COUNT: usize = 1_690;

    if let Some(root) = std::env::var_os("W9_EXACT_OWNER_HASH_ROOT") {
        let root = std::path::PathBuf::from(root);
        let hashes = w9_exact_hash_owner_directory(&root, SOURCE_COUNT)
            .unwrap_or_else(|error| panic!("hash exact owner sidecars: {error}"));
        for (index, hash) in hashes {
            println!("w9exacthash\t{index}\t{hash}");
        }
        return;
    }

    let root = std::path::PathBuf::from(std::env::var("W9_INPUT_ROOT").expect("W9_INPUT_ROOT"));
    let outputdir = std::path::PathBuf::from(
        std::env::var_os("W9_EXACT_OWNER_OUTPUT_DIR").expect("W9_EXACT_OWNER_OUTPUT_DIR"),
    );
    w9_exact_assert_structured_stage_is_published(&outputdir);
    let receipt_path = std::path::PathBuf::from(
        std::env::var_os("W9_EXACT_OWNER_RECEIPT").expect("W9_EXACT_OWNER_RECEIPT"),
    );
    let mut paths = WalkDir::new(&root)
        .into_iter()
        .filter_entry(|entry| {
            let relative = entry.path().strip_prefix(&root).expect("input is under root");
            let excluded = relative == std::path::Path::new("dev/compat")
                || relative.components().any(|part| {
                    matches!(part.as_os_str().to_str(), Some(".git" | ".work" | "target"))
                });
            !excluded
        })
        .filter_map(|entry| {
            let entry = entry.expect("walk frozen XSH corpus");
            (entry.file_type().is_file()
                && entry.path().extension().is_some_and(|extension| extension == "xsh"))
            .then(|| entry.into_path())
        })
        .collect::<Vec<_>>();
    paths.sort();
    assert_eq!(paths.len(), SOURCE_COUNT, "frozen corpus changed");

    let mut manifest = Vec::with_capacity(paths.len());
    let mut source_hashes = Vec::with_capacity(paths.len());
    for path in &paths {
        let bytes = std::fs::read(path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let relative = path
            .strip_prefix(&root)
            .expect("input is under root")
            .to_string_lossy()
            .replace('\\', "/");
        w9_exact_validate_tsv_text(&relative)
            .unwrap_or_else(|error| panic!("invalid input path: {error}"));
        let hash = blake3::hash(&bytes).to_hex().to_string();
        manifest.push(format!("{relative}\t{hash}"));
        source_hashes.push(hash);
    }
    let input_digest = blake3::hash(manifest.join("\n").as_bytes())
        .to_hex()
        .to_string();
    assert_eq!(input_digest, EXPECTED_CORPUS_BLAKE3, "frozen corpus digest changed");

    let shard_index = std::env::var("W9_SHARD_INDEX")
        .expect("W9_SHARD_INDEX")
        .parse::<usize>()
        .expect("numeric shard index");
    let shard_size = std::env::var("W9_SHARD_SIZE")
        .expect("W9_SHARD_SIZE")
        .parse::<usize>()
        .expect("numeric shard size");
    assert!(shard_size > 0, "shard size must be positive");
    let shard_count = SOURCE_COUNT.div_ceil(shard_size);
    assert!(shard_index < shard_count, "shard index is outside the corpus");
    let default_start = shard_index * shard_size;
    let start = std::env::var("W9_START")
        .ok()
        .map(|value| value.parse::<usize>().expect("numeric start index"))
        .unwrap_or(default_start);
    let default_end = start.saturating_add(shard_size).min(SOURCE_COUNT);
    let end = std::env::var("W9_END")
        .ok()
        .map(|value| value.parse::<usize>().expect("numeric end index"))
        .unwrap_or(default_end);
    assert!(start < end && end <= SOURCE_COUNT, "invalid source range {start}..{end}");

    let mut receipt_rows = Vec::with_capacity(end - start);
    for index in start..end {
        let path = &paths[index];
        let relative = path
            .strip_prefix(&root)
            .expect("input is under root")
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = std::fs::read(path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let parse_hash = blake3::hash(&bytes).to_hex().to_string();
        assert_eq!(
            parse_hash, source_hashes[index],
            "source changed between manifest and parse: {relative}"
        );
        let source = String::from_utf8(bytes).expect("XSH corpus is UTF-8");
        let file = path.to_string_lossy().into_owned();
        let roots = project_module_roots(path)
            .unwrap_or_else(|error| panic!("module roots for {file}: {error}"));
        let entry = entry_source_from_text(&file, source);
        let (sources, parsed) = parse_load_entry_source_arena_only(&file, entry, roots);
        w9_write_exact_adapter_owners(&outputdir, index, &parsed.arena, &sources)
            .unwrap_or_else(|error| panic!("write exact owner sidecar {index}: {error}"));
        let owner_path = format!("owners-{index}.tsv");
        let owner_bytes = w9_exact_read_file_at(&w9_exact_open_directory(&outputdir).expect("open owner output directory"), &owner_path)
            .unwrap_or_else(|error| panic!("read exact owner sidecar {index}: {error}"));
        receipt_rows.push(format!(
            "owner\t{index}\t{relative}\t{}\t{}",
            source_hashes[index],
            blake3::hash(&owner_bytes).to_hex()
        ));
    }

    let mut receipt = format!(
        "corpus={SOURCE_COUNT}\nshard={shard_index}/{shard_count}\nrange={start}..{end}\nshard_size={shard_size}\ninput_digest={input_digest}\n"
    );
    for row in &receipt_rows {
        receipt.push_str(row);
        receipt.push('\n');
    }
    receipt.push_str(&format!("complete={}\n", receipt_rows.len()));
    w9_exact_write_absolute_file(&receipt_path, receipt.as_bytes())
        .unwrap_or_else(|error| panic!("write exact owner receipt: {error}"));
}

fn w9_exact_hash_owner_directory(
    root_path: &std::path::Path,
    count: usize,
) -> std::io::Result<Vec<(usize, String)>> {
    use rustix::fs::{Dir, OFlags};
    use std::collections::BTreeSet;
    use std::io::Read as _;
    use std::os::unix::ffi::OsStrExt as _;

    let root = w9_exact_open_directory(root_path)?;
    let expected = (0..count)
        .map(|index| format!("owners-{index}.tsv"))
        .collect::<BTreeSet<_>>();
    let mut actual = BTreeSet::new();
    let mut entries = Dir::read_from(&root).map_err(std::io::Error::from)?;
    while let Some(entry) = entries.read() {
        let entry = entry.map_err(std::io::Error::from)?;
        let name = std::str::from_utf8(entry.file_name().to_bytes())
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        if name != "." && name != ".." && !actual.insert(name.to_owned()) {
            return Err(w9_exact_invalid_data(format!("duplicate owner sidecar {name}")));
        }
    }
    if actual != expected {
        return Err(w9_exact_invalid_data(format!(
            "owner sidecar set differs: missing={:?} extra={:?}",
            expected.difference(&actual).collect::<Vec<_>>(),
            actual.difference(&expected).collect::<Vec<_>>()
        )));
    }
    let mut hashes = Vec::with_capacity(count);
    for index in 0..count {
        let name = format!("owners-{index}.tsv");
        let file = w9_exact_open_regular_file_at(&root, &name)?;
        let mut bytes = Vec::new();
        let mut file = file;
        file.read_to_end(&mut bytes)?;
        hashes.push((index, blake3::hash(&bytes).to_hex().to_string()));
    }
    Ok(hashes)
}

fn w9_exact_open_directory(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    use rustix::fs::{Mode, OFlags, CWD};
    use std::path::Component;

    if !path.is_absolute() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "exact owner path must be absolute",
        ));
    }
    let mut directory = std::fs::File::from(
        rustix::fs::openat(
            CWD,
            std::path::Path::new("/"),
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(std::io::Error::from)?,
    );
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => {
                directory = std::fs::File::from(
                    rustix::fs::openat(
                        &directory,
                        name,
                        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                        Mode::empty(),
                    )
                    .map_err(std::io::Error::from)?,
                );
            }
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "exact owner path cannot contain parent components",
                ));
            }
        }
    }
    Ok(directory)
}

fn w9_exact_open_parent(path: &std::path::Path) -> std::io::Result<(std::fs::File, String)> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "exact owner file has no parent")
    })?;
    let name = path.file_name().and_then(std::ffi::OsStr::to_str).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "exact owner filename is not UTF-8")
    })?;
    if name.is_empty() || name == "." || name == ".." || name.contains('/') {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid exact owner filename",
        ));
    }
    Ok((w9_exact_open_directory(parent)?, name.to_owned()))
}

fn w9_exact_create_file_at(
    directory: &std::fs::File,
    name: &str,
) -> std::io::Result<std::fs::File> {
    use rustix::fs::{Mode, OFlags};

    rustix::fs::openat(
        directory,
        name,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o600),
    )
    .map(std::fs::File::from)
    .map_err(std::io::Error::from)
}

fn w9_exact_open_regular_file_at(
    directory: &std::fs::File,
    name: &str,
) -> std::io::Result<std::fs::File> {
    use rustix::fs::{FileType, OFlags};

    let file = std::fs::File::from(
        rustix::fs::openat(
            directory,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .map_err(std::io::Error::from)?,
    );
    let metadata = rustix::fs::fstat(&file).map_err(std::io::Error::from)?;
    if FileType::from_raw_mode(metadata.st_mode as _) != FileType::RegularFile {
        return Err(w9_exact_invalid_data(format!("{name} is not a regular file")));
    }
    Ok(file)
}

fn w9_exact_read_file_at(
    directory: &std::fs::File,
    name: &str,
) -> std::io::Result<Vec<u8>> {
    use std::io::Read as _;

    let mut file = w9_exact_open_regular_file_at(directory, name)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn w9_exact_write_absolute_file(path: &std::path::Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;

    let (directory, name) = w9_exact_open_parent(path)?;
    let mut file = w9_exact_create_file_at(&directory, &name)?;
    directory.sync_all()?;
    file.write_all(contents)?;
    file.sync_all()?;
    directory.sync_all()
}

fn w9_exact_validate_tsv_text(text: &str) -> std::io::Result<()> {
    if text.chars().any(|character| matches!(character, '\t' | '\n' | '\r')) {
        return Err(w9_exact_invalid_data("TSV field contains a line separator"));
    }
    Ok(())
}

fn w9_exact_invalid_data(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into())
}
