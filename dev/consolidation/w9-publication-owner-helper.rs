use std::io::Write as _;

fn w9_hex_bytes(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(DIGITS[(byte >> 4) as usize] as char);
        encoded.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn w9_audit_regular_file(
    path: &std::path::Path,
    allowed_root: &std::path::Path,
) -> std::io::Result<(std::path::PathBuf, Vec<u8>)> {
    let canonical = path.canonicalize()?;
    if canonical.as_path() != path || !canonical.starts_with(allowed_root) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "source path is not canonical beneath an approved root",
        ));
    }
    if !std::fs::metadata(&canonical)?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "source path is not a regular file",
        ));
    }
    Ok((canonical.clone(), std::fs::read(canonical)?))
}

fn w9_reject_symlink_components(path: &std::path::Path) -> std::io::Result<()> {
    let mut prefix = std::path::PathBuf::new();
    for component in path.components() {
        prefix.push(component.as_os_str());
        if std::fs::symlink_metadata(&prefix)?.file_type().is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "module root traverses a symlink",
            ));
        }
    }
    Ok(())
}

struct W9ResolvedProjectModuleConfig {
    config_path: std::path::PathBuf,
    config_bytes: Vec<u8>,
    module_roots: Vec<std::path::PathBuf>,
}

fn w9_resolve_project_module_config(
    program_entry: &std::path::Path,
    program_root: &std::path::Path,
) -> std::io::Result<W9ResolvedProjectModuleConfig> {
    let program_root_canonical = program_root.canonicalize()?;
    if program_root_canonical.as_path() != program_root {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "common program root is not canonical",
        ));
    }
    let (program_entry, _) = w9_audit_regular_file(program_entry, &program_root_canonical)?;
    let config_directory = xsh::frontend::load::nearest_project_config_dir(&program_entry)
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "program config is missing")
        })?;
    let config_path = config_directory.join(xsh::frontend::load::PROJECT_CONFIG_FILE_NAME);
    let (config_path, config_bytes) = w9_audit_regular_file(&config_path, &program_root_canonical)?;
    let config_text = std::str::from_utf8(&config_bytes).map_err(|error| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, error)
    })?;
    let config_span = xsh::frontend::source::Span::new(
        xsh::frontend::source::SourceId::new(0),
        0,
        0,
    );
    let config_fields = match xsh::host::ini::decode(config_text, config_span) {
        Ok(xsh::execution::value::Value::Record(fields)) => fields,
        Ok(_) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "program config does not decode to a record",
            ));
        }
        Err(error) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.message,
            ));
        }
    };
    let configured_module_path = xsh::frontend::load::configured_module_path(&config_fields)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let module_roots =
        xsh::frontend::load::module_roots(&config_directory, &configured_module_path);
    Ok(W9ResolvedProjectModuleConfig {
        config_path,
        config_bytes,
        module_roots,
    })
}

#[test]
fn w9_source_audit_rejects_loaded_bytes_that_differ_from_the_file() {
    let scratch_root = std::path::PathBuf::from(std::env::var_os("TMPDIR").expect("TMPDIR"));
    std::fs::create_dir_all(&scratch_root).expect("create source audit test scratch root");
    let scratch = tempfile::Builder::new()
        .prefix("w9-source-audit-")
        .tempdir_in(scratch_root)
        .expect("create source audit test directory");
    let input_root = scratch.path().join("input");
    let program_root = scratch.path().join("program");
    std::fs::create_dir(&input_root).expect("create input root");
    std::fs::create_dir(&program_root).expect("create program root");
    let program_entry = program_root.join("entry.xsh");
    let program_config = program_root.join(xsh::frontend::load::PROJECT_CONFIG_FILE_NAME);
    let input_file = input_root.join("entry.xsh");
    std::fs::write(&program_entry, "entry\n").expect("write program entry");
    std::fs::write(&program_config, "").expect("write program config");
    std::fs::write(&input_file, "disk bytes\n").expect("write input source file");
    let resolved_config = w9_resolve_project_module_config(&program_entry, &program_root)
        .expect("resolve source audit program config");
    let mut sources = xsh::frontend::source::SourceMap::new();
    sources.add_file(input_file.to_string_lossy().into_owned(), "loaded bytes\n");
    let mut output = Vec::new();

    let error = w9_write_source_audit_rows(
        &mut output,
        &sources,
        &input_root,
        &program_root,
        &resolved_config,
    )
    .expect_err("source audit must reject SourceMap and file byte drift");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(error.to_string(), "SourceMap bytes differ from the resolved source file");
}

#[test]
fn w9_source_audit_keeps_roots_bound_to_the_config_bytes_it_resolved() {
    let scratch_root = std::path::PathBuf::from(std::env::var_os("TMPDIR").expect("TMPDIR"));
    std::fs::create_dir_all(&scratch_root).expect("create source audit test scratch root");
    let scratch = tempfile::Builder::new()
        .prefix("w9-source-config-audit-")
        .tempdir_in(scratch_root)
        .expect("create source audit test directory");
    let input_root = scratch.path().join("input");
    let program_root = scratch.path().join("program");
    let original_root = program_root.join("original");
    let changed_root = program_root.join("changed");
    let module_name = format!("w9_config_binding_{}", std::process::id());
    std::fs::create_dir(&input_root).expect("create input root");
    std::fs::create_dir(&program_root).expect("create program root");
    std::fs::create_dir(&original_root).expect("create original module root");
    std::fs::create_dir(&changed_root).expect("create changed module root");
    let program_entry = program_root.join("entry.xsh");
    let program_config = program_root.join(xsh::frontend::load::PROJECT_CONFIG_FILE_NAME);
    let input_file = input_root.join("entry.xsh");
    let original_module = original_root.join(format!("{module_name}.xsh"));
    let changed_module = changed_root.join(format!("{module_name}.xsh"));
    let original_config = b"module_path = original\n";
    let changed_config = b"module_path = changed\n";
    let entry_source = format!("use {module_name}\n");
    std::fs::write(&program_entry, "entry\n").expect("write program entry");
    std::fs::write(&program_config, original_config).expect("write original program config");
    std::fs::write(&input_file, &entry_source).expect("write input source file");
    std::fs::write(&original_module, "export const source = \"original\"\n")
        .expect("write original module");
    std::fs::write(&changed_module, "export const source = \"changed\"\n")
        .expect("write changed module");
    let resolved_config = w9_resolve_project_module_config(&program_entry, &program_root)
        .expect("resolve original program config");
    let changed_config_text = std::str::from_utf8(changed_config).expect("changed config is UTF-8");
    let changed_config_fields = match xsh::host::ini::decode(
        changed_config_text,
        xsh::frontend::source::Span::new(xsh::frontend::source::SourceId::new(0), 0, 0),
    )
    .expect("decode changed config")
    {
        xsh::execution::value::Value::Record(fields) => fields,
        _ => panic!("changed config does not decode to a record"),
    };
    let changed_module_path =
        xsh::frontend::load::configured_module_path(&changed_config_fields)
            .expect("read changed module path");
    let changed_roots = xsh::frontend::load::module_roots(&program_root, &changed_module_path);
    assert_ne!(resolved_config.module_roots, changed_roots);
    std::fs::write(&program_config, changed_config).expect("change program config after resolution");

    let input_file_text = input_file.to_string_lossy().into_owned();
    let entry = xsh::frontend::load::entry_source_from_text(&input_file_text, entry_source);
    let root_source_id = entry.source_id;
    let (sources, parsed) = xsh::frontend::load::parse_load_entry_source_arena_only(
        &input_file_text,
        entry,
        resolved_config.module_roots.clone(),
    );
    assert!(sources.get(root_source_id).is_some());
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let loaded_original = original_module
        .canonicalize()
        .expect("canonicalize original module")
        .to_string_lossy()
        .into_owned();
    let loaded_changed = changed_module
        .canonicalize()
        .expect("canonicalize changed module")
        .to_string_lossy()
        .into_owned();
    assert!(sources.files().iter().any(|source| source.name() == loaded_original));
    assert!(!sources.files().iter().any(|source| source.name() == loaded_changed));
    let mut output = Vec::new();
    w9_write_source_audit_rows(
        &mut output,
        &sources,
        &input_root,
        &program_root,
        &resolved_config,
    )
    .expect("write source audit from resolved config");
    let rows = String::from_utf8(output).expect("source audit output is UTF-8");
    let config_path_hex = w9_hex_bytes(
        resolved_config
            .config_path
            .to_str()
            .expect("config path is UTF-8")
            .as_bytes(),
    );
    let config_hash = blake3::hash(original_config).to_hex();
    let original_root_hex = w9_hex_bytes(
        original_root
            .canonicalize()
            .expect("canonicalize original root")
            .to_str()
            .expect("original root is UTF-8")
            .as_bytes(),
    );
    let changed_root_hex = w9_hex_bytes(
        changed_root
            .canonicalize()
            .expect("canonicalize changed root")
            .to_str()
            .expect("changed root is UTF-8")
            .as_bytes(),
    );
    assert!(rows.contains(&format!(
        "module-config\t{config_path_hex}\t{}\t{config_hash}",
        original_config.len()
    )));
    assert!(rows.contains(&format!("module-root\t{original_root_hex}")));
    assert!(!rows.contains(&format!("module-root\t{changed_root_hex}")));
}

fn w9_write_source_audit_rows(
    output: &mut impl std::io::Write,
    sources: &xsh::frontend::source::SourceMap,
    input_root: &std::path::Path,
    program_root: &std::path::Path,
    resolved_config: &W9ResolvedProjectModuleConfig,
) -> std::io::Result<()> {
    let input_root_path = input_root;
    let program_root_path = program_root;
    let input_root = input_root_path.canonicalize()?;
    let program_root = program_root_path.canonicalize()?;
    if input_root.as_path() != input_root_path || program_root.as_path() != program_root_path {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "source roots are not canonical",
        ));
    }
    writeln!(
        output,
        "module-config\t{}\t{}\t{}",
        w9_hex_bytes(resolved_config.config_path.to_str().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "program config path is not UTF-8")
        })?.as_bytes()),
        resolved_config.config_bytes.len(),
        blake3::hash(&resolved_config.config_bytes).to_hex()
    )?;
    for module_root in &resolved_config.module_roots {
        w9_reject_symlink_components(module_root)?;
        let canonical = module_root.canonicalize()?;
        if !canonical.starts_with(&program_root) || !canonical.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "module root is not a directory beneath the common program root",
            ));
        }
        let encoded = canonical.to_str().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "module root path is not UTF-8")
        })?;
        writeln!(output, "module-root\t{}", w9_hex_bytes(encoded.as_bytes()))?;
    }
    for (expected, source) in sources.files().iter().enumerate() {
        if source.id().raw() != expected {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "source map ids are not sequential",
            ));
        }
        let source_bytes = source.text().as_bytes();
        if let Some(identity) = source
            .name()
            .strip_prefix("<xsh-stdlib:")
            .and_then(|name| name.strip_suffix('>'))
        {
            if identity.is_empty()
                || !identity
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "embedded standard-library source identity is malformed",
                ));
            }
            writeln!(
                output,
                "source-audit\t{:?}\tstdlib\t{}\t{}\t{}",
                source.id(),
                w9_hex_bytes(source.name().as_bytes()),
                source_bytes.len(),
                blake3::hash(source_bytes).to_hex()
            )?;
            continue;
        }
        let source_path = std::path::Path::new(source.name());
        if !source_path.is_absolute() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "file-backed source path is not absolute",
            ));
        }
        let (canonical, file_bytes) = if source_path.starts_with(&input_root) {
            w9_audit_regular_file(source_path, &input_root)?
        } else if source_path.starts_with(&program_root) {
            w9_audit_regular_file(source_path, &program_root)?
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "file-backed source is outside the input and program roots",
            ));
        };
        if file_bytes.as_slice() != source_bytes {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "SourceMap bytes differ from the resolved source file",
            ));
        }
        let path_text = canonical.to_str().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "source path is not UTF-8")
        })?;
        writeln!(
            output,
            "source-audit\t{:?}\tfile\t{}\t{}\t{}",
            source.id(),
            w9_hex_bytes(path_text.as_bytes()),
            file_bytes.len(),
            blake3::hash(&file_bytes).to_hex()
        )?;
    }
    Ok(())
}

fn w9_write_adapter_owners(
    outputdir: &std::path::Path,
    index: usize,
    program: &xsh::frontend::syntax::arena::ArenaProgram,
    sources: &xsh::frontend::source::SourceMap,
    input_root: &std::path::Path,
    program_root: &std::path::Path,
    resolved_config: &W9ResolvedProjectModuleConfig,
) -> std::io::Result<()> {
    let mut owner_rows = std::collections::BTreeSet::new();
    std::fs::create_dir_all(outputdir)?;
    let path = outputdir.join(format!("owners-{index}.tsv"));
    let mut output = std::io::BufWriter::new(std::fs::File::create(path)?);

    for source in sources.files() {
        writeln!(output, "source\t{:?}\t{}", source.id(), source.text().len())?;
    }
    w9_write_source_audit_rows(
        &mut output,
        sources,
        input_root,
        program_root,
        resolved_config,
    )?;

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
