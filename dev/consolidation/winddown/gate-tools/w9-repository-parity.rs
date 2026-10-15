#[test]
fn w9_repository_parity_receipt() {
    use walkdir::WalkDir;
    use xsh::diagnostic::DiagnosticRenderer;
    use xsh::frontend::check::Checker;
    use xsh::frontend::load::{entry_source_from_text, parse_load_entry_source_arena_only, project_module_roots};
    use std::io::Write;
    use std::time::Instant;

    let root = PathBuf::from(std::env::var("W9_INPUT_ROOT").expect("W9_INPUT_ROOT"));
    let output = PathBuf::from(std::env::var("W9_RECEIPT_OUTPUT").expect("W9_RECEIPT_OUTPUT"));
    let timing_path = PathBuf::from(std::env::var("W9_TIMING_OUTPUT").expect("W9_TIMING_OUTPUT"));
    let mut timing = std::fs::File::create(&timing_path)
        .unwrap_or_else(|error| panic!("create {}: {error}", timing_path.display()));
    let mut facts_output = std::env::var_os("W9_FACT_OUTPUT")
        .map(PathBuf::from)
        .map(|path| {
            std::fs::File::create(&path)
                .unwrap_or_else(|error| panic!("create {}: {error}", path.display()))
        });
    let mut type_owner_output = std::env::var_os("W9_TYPE_OWNER_OUTPUT")
        .map(PathBuf::from)
        .map(|path| {
            std::fs::File::create(&path)
                .unwrap_or_else(|error| panic!("create {}: {error}", path.display()))
        });
    let capture_index = std::env::var("W9_CAPTURE_INDEX")
        .ok()
        .map(|value| value.parse::<usize>().expect("numeric capture index"));
    let capture_profile = std::env::var("W9_CAPTURE_PROFILE").ok();
    let process_started = Instant::now();
    let mut paths = WalkDir::new(&root)
        .into_iter()
        .filter_entry(|entry| {
            let relative = entry.path().strip_prefix(&root).expect("input is under root");
            let excluded = relative == Path::new("dev/compat") || relative.components().any(|part| {
                matches!(part.as_os_str().to_str(), Some(".git" | ".work" | "target"))
            });
            !excluded
        })
        .filter_map(|entry| {
            let entry = entry.expect("walk frozen XSH corpus");
            (entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "xsh"))
                .then(|| entry.into_path())
        })
        .collect::<Vec<_>>();
    paths.sort();
    let source_count = paths.len();
    assert_eq!(source_count, 1_690, "frozen corpus changed; refresh the shard manifest");
    let shard_index = std::env::var("W9_SHARD_INDEX").expect("W9_SHARD_INDEX").parse::<usize>().expect("numeric shard index");
    let shard_size = std::env::var("W9_SHARD_SIZE").expect("W9_SHARD_SIZE").parse::<usize>().expect("numeric shard size");
    assert!(shard_size > 0);
    let shard_count = source_count.div_ceil(shard_size);
    assert!(shard_index < shard_count, "shard index is outside the corpus");
    let default_start = shard_index * shard_size;
    let start = std::env::var("W9_START")
        .ok()
        .map(|value| value.parse::<usize>().expect("numeric start index"))
        .unwrap_or(default_start);
    let default_end = (start + shard_size).min(source_count);
    let end = std::env::var("W9_END")
        .ok()
        .map(|value| value.parse::<usize>().expect("numeric end index"))
        .unwrap_or(default_end);
    assert!(start < end && end <= source_count, "invalid source range {start}..{end}");

    let options = [
        ("default", CheckOptions::default()),
        ("reveal", CheckOptions { reveal_types: true, ..CheckOptions::default() }),
        ("execution", CheckOptions::for_execution()),
        ("check-cli", CheckOptions::for_check_cli()),
    ];
    let selected_profile = std::env::var("W9_PROFILE").ok();
    let active_options = options
        .iter()
        .copied()
        .filter(|(name, _)| selected_profile.as_deref().is_none_or(|selected| selected == *name))
        .collect::<Vec<_>>();
    assert!(!active_options.is_empty(), "unknown CheckOptions profile: {selected_profile:?}");
    let manifest = paths.iter().map(|path| {
        let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let relative = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        format!("{relative}\t{}", blake3::hash(&bytes).to_hex())
    }).collect::<Vec<_>>();
    let input_digest = blake3::hash(manifest.join("\n").as_bytes()).to_hex();
    let mut receipt = std::fs::File::create(&output).unwrap_or_else(|error| panic!("create {}: {error}", output.display()));
    writeln!(receipt, "corpus={source_count}\nshard={shard_index}/{shard_count}\nrange={start}..{end}\nshard_size={shard_size}\ninput_digest={input_digest}\noptions={active_options:?}")
        .expect("write shard header");
    receipt.flush().expect("flush shard header");
    writeln!(timing, "process_start_epoch_ns={}\ninput_digest={input_digest}\nrange={start}..{end}", w9_epoch_nanos())
        .expect("write timing header");
    timing.flush().expect("flush timing header");
    for index in start..end {
        let path = &paths[index];
        let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let source = String::from_utf8(bytes.clone()).expect("XSH corpus is UTF-8");
        let relative = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        let source_hash = &manifest[index][relative.len() + 1..];
        eprintln!("w9 parity shard {shard_index}: starting {index}/{source_count} {relative}");
        let file = path.to_string_lossy().into_owned();
        let roots = project_module_roots(&path).unwrap_or_else(|error| panic!("module roots for {file}: {error}"));
        let entry = entry_source_from_text(&file, source.clone());
        let entry_id = entry.source_id;
        let (sources, parsed) = parse_load_entry_source_arena_only(&file, entry, roots);
        let mut option_results = Vec::new();
        if parsed.diagnostics.is_empty() {
            let program = parsed.arena;
            if capture_index == Some(index)
                && let Some(path) = std::env::var_os("W9_SITE_OWNER_OUTPUT")
            {
                let path = PathBuf::from(path);
                let mut owners = std::fs::File::create(&path)
                    .unwrap_or_else(|error| panic!("create {}: {error}", path.display()));
                for raw in 0..program.arena.stmt_tags.len() {
                    let id = xsh::frontend::syntax::arena::StmtId::from_index(raw);
                    let statement = program.arena.stmt(id);
                    let source = sources
                        .get(statement.span.source_id)
                        .expect("statement span source exists");
                    writeln!(
                        owners,
                        "stmt\t{id:?}\t{}\t{}\t{:?}",
                        source.name(),
                        statement.span.start(),
                        statement.kind
                    )
                    .expect("write statement owner");
                }
                for raw in 0..program.arena.blocks.len() {
                    let id = xsh::frontend::syntax::arena::BlockId::from_index(raw);
                    let block = program.arena.block(id);
                    let span = program.arena.span(block.span);
                    let source = sources.get(span.source_id).expect("block span source exists");
                    writeln!(
                        owners,
                        "block\t{id:?}\t{}\t{}\t{:?}",
                        source.name(),
                        span.start(),
                        block.statements
                    )
                    .expect("write block owner");
                }
                owners.flush().expect("flush site owner map");
            }
            if capture_index == Some(index)
                && let Some(owners) = type_owner_output.as_mut()
            {
                for (module_index, module) in program.modules.iter().enumerate() {
                    let definitions = program
                        .module_statements(module)
                        .filter_map(|statement| {
                            let kind = match program.arena.stmt(statement).kind {
                                xsh::frontend::syntax::arena::ArenaStmtKind::Export(inner) => {
                                    program.arena.stmt(inner).kind
                                }
                                kind => kind,
                            };
                            let xsh::frontend::syntax::arena::ArenaStmtKind::TypeDef(id) =
                                kind
                            else {
                                return None;
                            };
                            let definition = program.arena.type_def(id);
                            Some(format!(
                                "{id:?}\t{}\t{:?}",
                                definition.name.as_str(),
                                program.arena.stmt(statement).span.source_id,
                            ))
                        })
                        .collect::<Vec<_>>();
                    writeln!(
                        owners,
                        "module\t{module_index}\t{}\t{}\t{}",
                        module.key,
                        module.internal,
                        definitions.join("\t")
                    )
                    .expect("write type owner mapping");
                }
                let definitions = program
                    .arena
                    .stmt_ids(program.statements)
                    .filter_map(|statement| {
                        let kind = match program.arena.stmt(statement).kind {
                            xsh::frontend::syntax::arena::ArenaStmtKind::Export(inner) => {
                                program.arena.stmt(inner).kind
                            }
                            kind => kind,
                        };
                        let xsh::frontend::syntax::arena::ArenaStmtKind::TypeDef(id) =
                            kind
                        else {
                            return None;
                        };
                        let definition = program.arena.type_def(id);
                        Some(format!(
                            "{id:?}\t{}\t{:?}",
                            definition.name.as_str(),
                            program.arena.stmt(statement).span.source_id,
                        ))
                    })
                    .collect::<Vec<_>>();
                writeln!(owners, "root\t{}", definitions.join("\t"))
                    .expect("write root type owner mapping");
                for (type_index, definition) in program.arena.type_defs.iter().enumerate() {
                    writeln!(
                        owners,
                        "arena-type\tTypeDefId({type_index})\t{}",
                        definition.name.as_str(),
                    )
                    .expect("write arena type table");
                }
            }
            for (name, option) in active_options.iter().copied() {
                let wall_start_ns = w9_epoch_nanos();
                let check_started = Instant::now();
                let thread_cpu_start_ns = w9_thread_cpu_nanos();
                let checked = Checker::check_arena_with_sources_and_options(
                    &program,
                    &sources,
                    entry_id,
                    option,
                );
                let thread_cpu_end_ns = w9_thread_cpu_nanos();
                let wall_end_ns = w9_epoch_nanos();
                let wall_elapsed_ns = check_started.elapsed().as_nanos();
                let thread_cpu_elapsed_ns = thread_cpu_start_ns
                    .zip(thread_cpu_end_ns)
                    .map(|(start, end)| end.saturating_sub(start));
                let wall_minus_thread_cpu_ns = thread_cpu_elapsed_ns
                    .map(|elapsed| wall_elapsed_ns.saturating_sub(elapsed));
                writeln!(
                    timing,
                    "profile\t{index}\t{relative}\t{name}\t{wall_start_ns}\t{wall_end_ns}\t{wall_elapsed_ns}\t{thread_cpu_elapsed_ns:?}\t{wall_minus_thread_cpu_ns:?}"
                )
                .expect("write profile timing");
                timing.flush().expect("flush profile timing");
                let serialization_start_ns = w9_epoch_nanos();
                let serialization_started = Instant::now();
                let serialization_thread_cpu_start_ns = w9_thread_cpu_nanos();
                let renderer = DiagnosticRenderer::new().with_color(false);
                let mut diagnostics = checked.diagnostics.iter().map(|diagnostic| {
                    renderer.render(std::slice::from_ref(diagnostic), &sources).replace(root.to_string_lossy().as_ref(), "$ROOT")
                }).collect::<Vec<_>>();
                diagnostics.sort();
                let diagnostic_facts = format!("{diagnostics:?}");
                let publication_facts = w9_canonical_debug(&format!("{checked:#?}"));
                let ids = w9_name_symbol_ids(&publication_facts)
                    .into_iter()
                    .collect::<std::collections::BTreeSet<_>>();
                let mut spellings = std::collections::BTreeSet::new();
                let mut resolved = std::collections::BTreeMap::new();
                let mut symbols = std::env::var_os("W9_SYMBOL_MAP_OUTPUT")
                    .filter(|_| capture_index == Some(index) && capture_profile.as_deref() == Some(name))
                    .map(|path| {
                        std::fs::File::create(&path).unwrap_or_else(|error| {
                            panic!("create {}: {error}", Path::new(&path).display())
                        })
                    });
                program.symbol_owner().with_current(|| {
                    for raw in ids {
                        let name = xsh::frontend::symbols::Name::from_symbol(
                            xsh::frontend::symbols::Symbol::from_raw(raw),
                        );
                        let spelling = name.as_str().to_string();
                        assert_ne!(spelling, "<invalid-symbol>", "dangling symbol {raw}");
                        assert!(
                            spellings.insert(spelling.clone()),
                            "distinct symbol IDs resolve to {spelling:?}"
                        );
                        assert!(resolved.insert(raw, spelling.clone()).is_none());
                        if let Some(symbols) = symbols.as_mut() {
                            writeln!(symbols, "{raw}\t{spelling:?}")
                                .expect("write Name spelling");
                        }
                    }
                });
                let rendered_publication_facts = w9_canonical_debug(
                    &w9_render_name_symbols(&publication_facts, &resolved),
                );
                let serialization_thread_cpu_end_ns = w9_thread_cpu_nanos();
                let serialization_end_ns = w9_epoch_nanos();
                let serialization_elapsed_ns = serialization_started.elapsed().as_nanos();
                let serialization_thread_cpu_elapsed_ns = serialization_thread_cpu_start_ns
                    .zip(serialization_thread_cpu_end_ns)
                    .map(|(start, end)| end.saturating_sub(start));
                let serialization_wall_minus_thread_cpu_ns = serialization_thread_cpu_elapsed_ns
                    .map(|elapsed| serialization_elapsed_ns.saturating_sub(elapsed));
                writeln!(
                    timing,
                    "serialization\t{index}\t{relative}\t{name}\t{serialization_start_ns}\t{serialization_end_ns}\t{serialization_elapsed_ns}\t{serialization_thread_cpu_elapsed_ns:?}\t{serialization_wall_minus_thread_cpu_ns:?}"
                )
                .expect("write serialization timing");
                timing.flush().expect("flush serialization timing");
                if capture_index == Some(index) && capture_profile.as_deref() == Some(name) {
                    let facts = facts_output.as_mut().expect("capture output path is set");
                    writeln!(
                        facts,
                        "source\t{index}\t{relative}\nmode\t{name}\ndiagnostics\t{diagnostic_facts}\npublication\t{publication_facts}"
                    )
                    .expect("write captured semantic facts");
                    if let Some(path) = std::env::var_os("W9_RENDERED_OUTPUT") {
                        std::fs::write(&path, &rendered_publication_facts).unwrap_or_else(|error| {
                            panic!("write {}: {error}", Path::new(&path).display())
                        });
                    }
                }
                option_results.push(format!("output\t{index}\t{relative}\t{name}\t{}\t{}", blake3::hash(diagnostic_facts.as_bytes()).to_hex(), blake3::hash(rendered_publication_facts.as_bytes()).to_hex()));
            }
        } else {
            let renderer = DiagnosticRenderer::new().with_color(false);
            let mut diagnostics = parsed.diagnostics.iter().map(|diagnostic| {
                renderer.render(std::slice::from_ref(diagnostic), &sources).replace(root.to_string_lossy().as_ref(), "$ROOT")
            }).collect::<Vec<_>>();
            diagnostics.sort();
            let canonical = format!("load-errors={diagnostics:?}");
            for (name, _) in active_options.iter().copied() {
                if capture_index == Some(index) && capture_profile.as_deref() == Some(name) {
                    let facts = facts_output.as_mut().expect("capture output path is set");
                    writeln!(facts, "source\t{index}\t{relative}\nmode\t{name}\nload-errors\t{canonical}")
                        .expect("write captured load diagnostics");
                }
                option_results.push(format!("output\t{index}\t{relative}\t{name}\t{}\t{}", blake3::hash(canonical.as_bytes()).to_hex(), blake3::hash(b"").to_hex()));
            }
        }
        writeln!(receipt, "input\t{index}\t{relative}\t{source_hash}").expect("write source input hash");
        for result in option_results {
            writeln!(receipt, "{result}").expect("write source output hash");
        }
        receipt.flush().expect("flush completed source receipt");
        eprintln!("w9 parity shard {shard_index}: completed {index}/{source_count} {relative}");
    }
    writeln!(receipt, "terminal\tshard={shard_index}\tcompleted={}", end - start).expect("write terminal shard record");
    receipt.flush().expect("flush terminal shard record");
    writeln!(timing, "process_end_epoch_ns={}\nprocess_elapsed_ns={}", w9_epoch_nanos(), process_started.elapsed().as_nanos())
        .expect("write timing footer");
    timing.flush().expect("flush timing footer");
}

fn w9_thread_cpu_nanos() -> Option<u128> {
    #[cfg(target_os = "linux")]
    {
        let mut time = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        let status = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) };
        return (status == 0).then_some(
            (time.tv_sec as u128) * 1_000_000_000 + (time.tv_nsec as u128),
        );
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn w9_epoch_nanos() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch")
        .as_nanos()
}

fn w9_quoted_end(input: &str, start: usize) -> Option<usize> {
    let quote = *input.as_bytes().get(start)?;
    if !matches!(quote, b'"' | b'\'') {
        return None;
    }
    let bytes = input.as_bytes();
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            byte if byte == quote => return Some(index + 1),
            byte if quote == b'\'' && byte.is_ascii_whitespace() => return None,
            _ => index += 1,
        }
    }
    None
}

fn w9_name_symbol_ids(input: &str) -> Vec<u32> {
    let bytes = input.as_bytes();
    let marker = b"Name(Symbol(";
    let mut symbols = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if let Some(end) = w9_quoted_end(input, index) {
            index = end;
            continue;
        }
        if bytes[index..].starts_with(marker)
            && (index == 0
                || !(bytes[index - 1].is_ascii_alphanumeric() || bytes[index - 1] == b'_'))
        {
            let start = index + marker.len();
            let close = input[start..]
                .find("))")
                .map(|offset| start + offset)
                .expect("closed Name symbol constructor");
            assert!(input.as_bytes()[start..close].iter().all(u8::is_ascii_digit));
            let after = close + 2;
            assert!(after == bytes.len() || !(bytes[after].is_ascii_alphanumeric() || bytes[after] == b'_'));
            symbols.push(
                input[start..close]
                    .parse::<u32>()
                    .expect("numeric Name symbol"),
            );
            index = after;
            continue;
        }
        let character = input[index..].chars().next().expect("character boundary");
        index += character.len_utf8();
    }
    symbols
}

fn w9_render_name_symbols(input: &str, symbols: &std::collections::BTreeMap<u32, String>) -> String {
    let mut spellings = std::collections::BTreeSet::new();
    for spelling in symbols.values() {
        assert_ne!(spelling, "<invalid-symbol>", "invalid symbol spelling");
        assert!(spellings.insert(spelling), "symbol spellings are not injective");
    }
    let bytes = input.as_bytes();
    let marker = b"Name(Symbol(";
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < bytes.len() {
        if let Some(end) = w9_quoted_end(input, index) {
            output.push_str(&input[index..end]);
            index = end;
            continue;
        }
        if bytes[index..].starts_with(marker)
            && (index == 0
                || !(bytes[index - 1].is_ascii_alphanumeric() || bytes[index - 1] == b'_'))
        {
            let start = index + marker.len();
            let close = input[start..]
                .find("))")
                .map(|offset| start + offset)
                .expect("closed Name symbol constructor");
            assert!(input.as_bytes()[start..close].iter().all(u8::is_ascii_digit));
            let raw = input[start..close]
                .parse::<u32>()
                .expect("numeric Name symbol");
            let after = close + 2;
            assert!(after == bytes.len() || !(bytes[after].is_ascii_alphanumeric() || bytes[after] == b'_'));
            let spelling = symbols.get(&raw).expect("captured Name symbol has a spelling");
            output.push_str(&format!("Name({spelling:?})"));
            index = after;
            continue;
        }
        let character = input[index..].chars().next().expect("character boundary");
        output.push(character);
        index += character.len_utf8();
    }
    output
}

fn w9_group_end(input: &str, start: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let close = match bytes.get(start)? {
        b'{' => b'}',
        b'[' => b']',
        b'(' => b')',
        _ => return None,
    };
    let mut stack = vec![close];
    let mut index = start + 1;
    while index < bytes.len() {
        if let Some(end) = w9_quoted_end(input, index) {
            index = end;
            continue;
        }
        match bytes[index] {
            b'{' => stack.push(b'}'),
            b'[' => stack.push(b']'),
            b'(' => stack.push(b')'),
            b'}' | b']' | b')' => {
                if stack.pop() != Some(bytes[index]) {
                    return None;
                }
                if stack.is_empty() {
                    return Some(index);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

fn w9_split_debug_entries(input: &str) -> Vec<&str> {
    let bytes = input.as_bytes();
    let mut entries = Vec::new();
    let mut stack = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        if let Some(end) = w9_quoted_end(input, index) {
            index = end;
            continue;
        }
        match bytes[index] {
            b'{' => stack.push(b'}'),
            b'[' => stack.push(b']'),
            b'(' => stack.push(b')'),
            b'}' | b']' | b')' => {
                assert_eq!(stack.pop(), Some(bytes[index]), "unbalanced Debug input: {input}");
            }
            b',' if stack.is_empty() => {
                entries.push(&input[start..index]);
                start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }
    if start < input.len() {
        entries.push(&input[start..]);
    }
    entries
}

fn w9_named_struct_before(input: &str, open: usize) -> bool {
    let prefix = input[..open].trim_end();
    let token_start = prefix
        .rfind(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .map_or(0, |index| index + 1);
    if token_start == prefix.len() {
        return false;
    }
    let before_token = &prefix[..token_start];
    let Some(previous) = before_token.chars().rev().find(|character| !character.is_whitespace()) else {
        return true;
    };
    previous != ':'
        || before_token.ends_with("::")
        || before_token.chars().last().is_some_and(char::is_whitespace)
}

fn w9_canonical_debug(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < bytes.len() {
        if let Some(end) = w9_quoted_end(input, index) {
            output.push_str(&input[index..end]);
            index = end;
            continue;
        }
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        let Some(end) = w9_group_end(input, index) else {
            let character = input[index..].chars().next().expect("character boundary");
            output.push(character);
            index += character.len_utf8();
            continue;
        };
        let delimiter = bytes[index];
        let contents = &input[index + 1..end];
        let mut entries = w9_split_debug_entries(contents)
            .into_iter()
            .map(w9_canonical_debug)
            .filter(|entry| !entry.is_empty())
            .collect::<Vec<_>>();
        if delimiter == b'{' && !w9_named_struct_before(input, index) {
            entries.sort();
        }
        output.push(delimiter as char);
        output.push_str(&entries.join(","));
        output.push(match delimiter {
            b'{' => '}',
            b'[' => ']',
            b'(' => ')',
            _ => unreachable!(),
        });
        index = end + 1;
    }
    output
}

fn w9_flat_debug_lines(input: &str) -> Vec<&str> {
    let mut lines = input.lines().collect::<Vec<_>>();
    lines.sort_unstable();
    lines
}

#[test]
fn w9_debug_canonicalizer_preserves_owner_associations_and_sequence_order() {
    let left = r#"Output {
  bindings: {
    "owner-a": Binding {
      sites: [Span { source: 1 }, Span { source: 2 }],
    },
    "owner-b": Binding {
      sites: [Span { source: 3 }],
    },
  },
}"#;
    let reassociated = r#"Output {
  bindings: {
    "owner-a": Binding {
      sites: [Span { source: 3 }],
    },
    "owner-b": Binding {
      sites: [Span { source: 1 }, Span { source: 2 }],
    },
  },
}"#;
    assert_eq!(w9_flat_debug_lines(left), w9_flat_debug_lines(reassociated));
    assert_ne!(w9_canonical_debug(left), w9_canonical_debug(reassociated));

    let reordered_map = r#"Output { bindings: { "owner-b": Binding { sites: [Span { source: 3 }] }, "owner-a": Binding { sites: [Span { source: 1 }, Span { source: 2 }] } } }"#;
    assert_eq!(w9_canonical_debug(left), w9_canonical_debug(reordered_map));

    let vector_left = "Output {\n  values: [\n    1,\n    2,\n    3,\n  ],\n}";
    let vector_reordered = "Output {\n  values: [\n    3,\n    2,\n    1,\n  ],\n}";
    assert_eq!(w9_flat_debug_lines(vector_left), w9_flat_debug_lines(vector_reordered));
    assert_ne!(w9_canonical_debug(vector_left), w9_canonical_debug(vector_reordered));

    let diagnostics_left = r#"Output {
  diagnostics: [
    Diagnostic { code: "first", labels: [Label { span: 1 }] },
    Diagnostic { code: "second", labels: [Label { span: 2 }] },
  ],
}"#;
    let diagnostics_reordered = r#"Output {
  diagnostics: [
    Diagnostic { code: "second", labels: [Label { span: 2 }] },
    Diagnostic { code: "first", labels: [Label { span: 1 }] },
  ],
}"#;
    assert_eq!(w9_flat_debug_lines(diagnostics_left), w9_flat_debug_lines(diagnostics_reordered));
    assert_ne!(w9_canonical_debug(diagnostics_left), w9_canonical_debug(diagnostics_reordered));

    let labels_reassociated = r#"Output { diagnostics: [Diagnostic { code: "first", labels: [Label { span: 2 }] }, Diagnostic { code: "second", labels: [Label { span: 1 }] }] }"#;
    assert_ne!(w9_canonical_debug(diagnostics_left), w9_canonical_debug(labels_reassociated));

    let escaped = r#"Output { message: "escaped \\\" , } ] } [ and newline \\n", nested: [Span { source: 4 }] }"#;
    assert!(w9_canonical_debug(escaped).contains(r#"message:"escaped \\\" , } ] } [ and newline \\n""#));
}

#[test]
fn w9_name_symbol_renderer_changes_only_validated_constructors() {
    use std::collections::BTreeMap;

    let symbols = BTreeMap::from([(7, "alpha".to_owned()), (8, "beta".to_owned())]);
    let text = r#"Output { message: "Name(Symbol(7))", Name(Symbol(7)): [Name(Symbol(8))] }"#;
    assert_eq!(w9_name_symbol_ids(text), [7, 8]);
    let rendered = w9_render_name_symbols(text, &symbols);
    assert!(rendered.contains(r#"message: "Name(Symbol(7))""#));
    assert!(rendered.contains(r#"Name("alpha"): [Name("beta")]"#));
    assert_eq!(w9_render_name_symbols("OtherName(Symbol(7))", &symbols), "OtherName(Symbol(7))");

    let altered = BTreeMap::from([(7, "changed".to_owned()), (8, "beta".to_owned())]);
    assert_ne!(rendered, w9_render_name_symbols(text, &altered));

    let association_left = r#"Output { entries: { Name(Symbol(7)): "left", Name(Symbol(8)): "right" } }"#;
    let association_reordered = r#"Output { entries: { Name(Symbol(8)): "right", Name(Symbol(7)): "left" } }"#;
    let association_changed = r#"Output { entries: { Name(Symbol(7)): "right", Name(Symbol(8)): "left" } }"#;
    assert_eq!(
        w9_canonical_debug(&w9_render_name_symbols(association_left, &symbols)),
        w9_canonical_debug(&w9_render_name_symbols(association_reordered, &symbols)),
    );
    assert_ne!(
        w9_canonical_debug(&w9_render_name_symbols(association_left, &symbols)),
        w9_canonical_debug(&w9_render_name_symbols(association_changed, &symbols)),
    );

    let vector = "Values([Name(Symbol(7)), Name(Symbol(8))])";
    let vector_reordered = "Values([Name(Symbol(8)), Name(Symbol(7))])";
    assert_ne!(
        w9_canonical_debug(&w9_render_name_symbols(vector, &symbols)),
        w9_canonical_debug(&w9_render_name_symbols(vector_reordered, &symbols)),
    );

    assert!(std::panic::catch_unwind(|| w9_render_name_symbols("Name(Symbol(9))", &symbols)).is_err());
    assert!(std::panic::catch_unwind(|| {
        w9_render_name_symbols(
            "Name(Symbol(7))",
            &BTreeMap::from([(7, "same".to_owned()), (8, "same".to_owned())]),
        )
    })
    .is_err());
    assert!(std::panic::catch_unwind(|| {
        w9_render_name_symbols(
            "Name(Symbol(7))",
            &BTreeMap::from([(7, "<invalid-symbol>".to_owned())]),
        )
    })
    .is_err());
}

