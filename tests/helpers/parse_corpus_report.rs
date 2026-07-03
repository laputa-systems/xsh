use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Instant;
use xsh::modules::api_spec;
use xsh::perf::{AllocationSnapshot, allocation_snapshot, reset_allocations};
use xsh::runtime::eval::{
    COMPACT_CALL_BLOCKER_KIND_COUNT, COMPACT_COMMAND_BLOCKER_KIND_COUNT, COMPACT_EXPR_KIND_COUNT,
    COMPACT_FUNCTION_BLOCKER_KIND_COUNT, COMPACT_STMT_KIND_COUNT,
    COMPACT_TOP_LEVEL_BLOCKER_KIND_COUNT, COMPACT_TYPE_EXPR_TAG_COUNT, CompactLowerBodyProbeOutput,
    CompactLowerConstructProbeOutput, CompactLowerProbeOutput, CompactRuntimeDeclProbeOutput,
    LoweredFunctionKey, probe_compact_lower_constructed_bodies, probe_compact_lower_declarations,
    probe_compact_runtime_declarations,
};
use xsh::sema::check::{Checker, CompactBodyProbeOutput, CompactDeclOutput};
use xsh::source::{SourceId, Span};
use xsh::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaBuilderEntryKind, ArenaCallArgKind,
    ArenaCommand, ArenaCommandArg, ArenaCommandArgKind, ArenaEnvAssignment,
    ArenaEnvAssignmentValue, ArenaExprKind, ArenaExprOrRun, ArenaFmtPart,
    ArenaModuleContractEntryKind, ArenaPatternKind, ArenaPipeStageKind, ArenaRecordFieldKind,
    ArenaRedirectionTarget, ArenaSpawnTarget, ArenaStmtKind, ArenaStreamStage, ArenaTypeDefBody,
    ArenaWordPart, AssignTargetId, BindingTargetId, BlockId, BuilderBlockId, CommandStmtId,
    ErrorDefId, ExprId, FunctionDefId, PatternId, RunFormId, StmtId, TypeDefId, UseStmtId,
};
use xsh::syntax::arena::{ArenaProgram, ArenaStats};
use xsh::syntax::cst::SyntaxToken;
use xsh::syntax::parser::{ArenaParseOutput, Parser};
use xsh::syntax::token::TokenKind;

struct SourceFile {
    path: String,
    text: String,
}

struct Totals {
    files: usize,
    bytes: usize,
}

fn main() {
    let mut root = PathBuf::from(".");
    let mut repeat = 1usize;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => {
                let Some(value) = args.next() else {
                    usage();
                };
                root = PathBuf::from(value);
            }
            "--repeat" => {
                let Some(value) = args.next() else {
                    usage();
                };
                repeat = value.parse().unwrap_or_else(|_| usage());
            }
            "-h" | "--help" => usage(),
            _ => usage(),
        }
    }
    if repeat == 0 {
        usage();
    }

    let files = load_sources(&root).unwrap_or_else(|err| {
        eprintln!("parse corpus: {err}");
        std::process::exit(1);
    });
    let totals = Totals {
        files: files.len(),
        bytes: files.iter().map(|file| file.text.len()).sum(),
    };

    let ((arena_only, arena_only_counts, file_counts, parse_file_timing), arena_only_metrics) =
        measure_phase(|| parse_arena_only_corpus(&files, repeat));
    let (compact_walk_counts, compact_walk_metrics) =
        measure_phase(|| compact_walk_corpus(&arena_only, repeat));
    let ((compact_decls, compact_decl_counts, decl_file_timing), compact_decl_metrics) =
        measure_phase(|| compact_decl_corpus(&arena_only, repeat));
    let ((compact_body_probes, compact_body_counts, body_file_timing), compact_body_metrics) =
        measure_phase(|| compact_body_corpus(&arena_only, &compact_decls, repeat));
    let (
        (compact_lower_decls, compact_lower_decl_counts, lower_decl_file_timing),
        compact_lower_decl_metrics,
    ) = measure_phase(|| compact_lower_decl_corpus(&arena_only, &compact_decls, repeat));
    let (
        (compact_lower_constructs, compact_lower_construct_counts, lower_construct_file_timing),
        compact_lower_construct_metrics,
    ) = measure_phase(|| {
        compact_lower_construct_corpus(
            &files,
            &arena_only,
            &compact_decls,
            &compact_body_probes,
            repeat,
        )
    });
    let (
        (compact_lower_bodies, compact_lower_body_counts, lower_body_file_timing),
        compact_lower_body_metrics,
    ) = measure_phase(|| compact_lower_body_corpus(&compact_lower_constructs, repeat));
    let (
        (compact_runtime_decls, compact_runtime_decl_counts, runtime_decl_file_timing),
        compact_runtime_decl_metrics,
    ) = measure_phase(|| compact_runtime_decl_corpus(&compact_decls, repeat));
    let compact_hot_path_counts = compact_hot_path_corpus(
        &files,
        &arena_only,
        &compact_decls,
        &compact_body_probes,
        &compact_lower_constructs,
        repeat,
    );
    let phase_file_summaries = PhaseFileSummaries {
        parse_arena_only: parse_file_timing.summary(),
        declaration_probe: decl_file_timing.summary(),
        body_probe: body_file_timing.summary(),
        lowering_declaration_probe: lower_decl_file_timing.summary(),
        lowering_construct_probe: lower_construct_file_timing.summary(),
        lowering_body_probe: lower_body_file_timing.summary(),
        runtime_declaration_registration: runtime_decl_file_timing.summary(),
    };
    let module_graph_readiness = module_graph_readiness(&files, &arena_only, &compact_decls);
    let function_lowering_readiness = function_lowering_readiness(&compact_lower_construct_counts);
    let top_level_readiness = top_level_readiness(&compact_lower_construct_counts);

    print_report(
        &root,
        repeat,
        &totals,
        &file_counts,
        &phase_file_summaries,
        &module_graph_readiness,
        &function_lowering_readiness,
        &top_level_readiness,
        &arena_only_counts,
        &arena_only_metrics,
        &compact_walk_counts,
        &compact_walk_metrics,
        &compact_decl_counts,
        &compact_decl_metrics,
        &compact_body_counts,
        &compact_body_metrics,
        &compact_lower_decl_counts,
        &compact_lower_decl_metrics,
        &compact_lower_body_counts,
        &compact_lower_body_metrics,
        &compact_lower_construct_counts,
        &compact_lower_construct_metrics,
        &compact_runtime_decl_counts,
        &compact_runtime_decl_metrics,
        &compact_hot_path_counts,
    );
    std::hint::black_box((
        &arena_only,
        &compact_decls,
        &compact_body_probes,
        &compact_lower_decls,
        &compact_lower_bodies,
        &compact_lower_constructs,
        &compact_runtime_decls,
    ));
}

fn usage() -> ! {
    eprintln!("usage: xsh-parse-corpus-report [--root DIR] [--repeat N]");
    std::process::exit(2);
}

fn parallel_map_indexed<T, F>(len: usize, f: F) -> Vec<T>
where
    T: Send,
    F: Fn(usize) -> T + Sync,
{
    if len == 0 {
        return Vec::new();
    }
    let jobs = thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .min(len);
    if jobs <= 1 {
        return (0..len).map(f).collect();
    }
    let chunk = len.div_ceil(jobs);
    thread::scope(|scope| {
        let mut handles = Vec::new();
        for start in (0..len).step_by(chunk) {
            let end = (start + chunk).min(len);
            let f = &f;
            handles.push(scope.spawn(move || {
                let mut items = Vec::with_capacity(end - start);
                for index in start..end {
                    items.push((index, f(index)));
                }
                items
            }));
        }
        let mut output = std::iter::repeat_with(|| None)
            .take(len)
            .collect::<Vec<Option<T>>>();
        for handle in handles {
            for (index, item) in handle.join().expect("parallel corpus worker panicked") {
                output[index] = Some(item);
            }
        }
        output
            .into_iter()
            .map(|item| item.expect("parallel corpus worker produced every index"))
            .collect()
    })
}

fn load_sources(root: &Path) -> Result<Vec<SourceFile>, String> {
    let mut paths = Vec::new();
    collect_xsh_paths(root, &mut paths)?;
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let text =
                fs::read_to_string(&path).map_err(|err| format!("{}: {err}", path.display()))?;
            let display_path = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string();
            Ok(SourceFile {
                path: display_path,
                text,
            })
        })
        .collect()
}

fn collect_xsh_paths(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("{}: {err}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|err| format!("{}: {err}", path.display()))?;
        if file_type.is_dir() {
            if should_skip_dir(&path) {
                continue;
            }
            collect_xsh_paths(&path, paths)?;
        } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == "xsh") {
            paths.push(path);
        }
    }
    Ok(())
}

fn should_skip_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git" | "target" | "docs-html")
    )
}

fn parse_arena_only_corpus(
    files: &[SourceFile],
    repeat: usize,
) -> (
    Vec<ArenaParseOutput>,
    ParseCounts,
    Vec<FileCounts>,
    PhaseFileTiming,
) {
    let mut retained = Vec::<ArenaParseOutput>::new();
    let mut file_counts = Vec::<FileCounts>::new();
    let mut file_timing = PhaseFileTiming::default();
    let mut counts = ParseCounts::default();
    for run in 0..repeat {
        retained.clear();
        let outputs = parallel_map_indexed(files.len(), |index| {
            let file = &files[index];
            let started = Instant::now();
            let parsed = Parser::parse_source_arena_only(SourceId::new(index), &file.text);
            let elapsed = started.elapsed().as_nanos();
            let file_counts = (run == 0).then(|| file_counts_for_parsed(file, &parsed));
            (parsed, elapsed, file_counts)
        });
        for (parsed, elapsed, file_count) in outputs {
            file_timing.observe(elapsed);
            let stats = parsed.arena.stats();
            counts.statements += stats.statements;
            counts.modules += stats.modules;
            counts.syntax_tokens += parsed.cst.get().tokens().len();
            counts.compact_token_table_tokens += parsed.cst.token_table().len();
            counts.compact_token_row_bytes += parsed.cst.token_table().row_bytes();
            counts.compact_token_retained_bytes += parsed.cst.token_table().retained_bytes();
            counts.arena.add(stats);
            counts.diagnostics += parsed.diagnostics.len();
            if let Some(file_count) = file_count {
                file_counts.push(file_count);
            }
            retained.push(parsed);
        }
    }
    counts.retained_outputs = retained.len();
    std::hint::black_box(&retained);
    (retained, counts, file_counts, file_timing)
}

fn compact_walk_corpus(parsed: &[ArenaParseOutput], repeat: usize) -> CompactWalkCounts {
    let mut counts = CompactWalkCounts::default();
    for _ in 0..repeat {
        for parsed in parsed {
            counts.programs += 1;
            counts.diagnostics += parsed.diagnostics.len();
            walk_compact_program(&parsed.arena, &mut counts);
        }
    }
    std::hint::black_box(&counts);
    counts
}

fn compact_decl_corpus(
    parsed: &[ArenaParseOutput],
    repeat: usize,
) -> (Vec<CompactDeclOutput>, CompactDeclCounts, PhaseFileTiming) {
    let mut retained = Vec::<CompactDeclOutput>::new();
    let mut file_timing = PhaseFileTiming::default();
    let mut counts = CompactDeclCounts::default();
    for _ in 0..repeat {
        retained.clear();
        let outputs = parallel_map_indexed(parsed.len(), |index| {
            let parsed = &parsed[index];
            let started = Instant::now();
            let output = Checker::check_compact_declarations(&parsed.arena);
            (output, started.elapsed().as_nanos())
        });
        for (output, elapsed) in outputs {
            file_timing.observe(elapsed);
            counts.diagnostics += output.diagnostics.len();
            counts.type_defs += output.type_defs;
            counts.tag_variants += output.tag_variants;
            counts.error_families += output.error_families;
            counts.error_variants += output.error_variants;
            counts.error_fields += output.error_fields;
            counts.function_defs += output.function_defs;
            counts.params += output.params;
            counts.schema_fields += output.schema_fields;
            counts.module_contract_entries += output.module_contract_entries;
            counts.type_states += output.types.len();
            counts.tag_variant_states += output.tag_variants_by_name.len();
            counts.error_family_states += output.error_families_by_name.len();
            counts.proc_sigs += output.procs.len();
            counts.pure_sigs += output.pures.len();
            counts.stream_sigs += output.streams.len();
            counts.qualified_proc_sigs += output.qualified_procs.len();
            counts.qualified_pure_sigs += output.qualified_pures.len();
            counts.qualified_stream_sigs += output.qualified_streams.len();
            retained.push(output);
        }
    }
    counts.retained_outputs = retained.len();
    std::hint::black_box(&retained);
    (retained, counts, file_timing)
}

fn compact_body_corpus(
    parsed: &[ArenaParseOutput],
    decls: &[CompactDeclOutput],
    repeat: usize,
) -> (
    Vec<CompactBodyProbeOutput>,
    CompactBodyCounts,
    PhaseFileTiming,
) {
    let mut retained = Vec::<CompactBodyProbeOutput>::new();
    let mut file_timing = PhaseFileTiming::default();
    let mut counts = CompactBodyCounts::default();
    for _ in 0..repeat {
        retained.clear();
        let outputs = parallel_map_indexed(parsed.len(), |index| {
            let parsed = &parsed[index];
            let decls = &decls[index];
            let started = Instant::now();
            let output = Checker::probe_compact_bodies(&parsed.arena, decls);
            (output, started.elapsed().as_nanos())
        });
        for (output, elapsed) in outputs {
            file_timing.observe(elapsed);
            counts.diagnostics += output.diagnostics.len();
            counts.statements += output.statements;
            counts.supported_statements += output.supported_statements;
            counts.unsupported_statements += output.unsupported_statements;
            counts.expressions += output.expressions;
            counts.typed_expressions += output.typed_expressions;
            counts.unsupported_expressions += output.unsupported_expressions;
            counts.bindings += output.bindings;
            counts.assignment_targets += output.assignment_targets;
            counts.blocks += output.blocks;
            counts.functions += output.functions;
            counts.commands += output.commands;
            counts.runs += output.runs;
            counts.unsupported_signal_hooks += output.unsupported_signal_hooks;
            counts.unsupported_with_stmts += output.unsupported_with_stmts;
            counts.unsupported_guards += output.unsupported_guards;
            counts.unsupported_guarded_stmts += output.unsupported_guarded_stmts;
            counts.unsupported_item_exprs += output.unsupported_item_exprs;
            counts.unsupported_list_comps += output.unsupported_list_comps;
            counts.unsupported_map_comps += output.unsupported_map_comps;
            counts.unsupported_match_exprs += output.unsupported_match_exprs;
            counts.unsupported_pipeline_exprs += output.unsupported_pipeline_exprs;
            counts.unsupported_structured_pipeline_exprs +=
                output.unsupported_structured_pipeline_exprs;
            counts.unsupported_builder_call_exprs += output.unsupported_builder_call_exprs;
            counts.expr_type_facts += output.expr_types.len();
            retained.push(output);
        }
    }
    counts.retained_outputs = retained.len();
    std::hint::black_box(&retained);
    (retained, counts, file_timing)
}

fn compact_lower_decl_corpus(
    parsed: &[ArenaParseOutput],
    decls: &[CompactDeclOutput],
    repeat: usize,
) -> (
    Vec<CompactLowerProbeOutput>,
    CompactLowerDeclCounts,
    PhaseFileTiming,
) {
    let mut retained = Vec::<CompactLowerProbeOutput>::new();
    let mut file_timing = PhaseFileTiming::default();
    let mut counts = CompactLowerDeclCounts::default();
    for _ in 0..repeat {
        retained.clear();
        let outputs = parallel_map_indexed(parsed.len(), |index| {
            let parsed = &parsed[index];
            let decls = &decls[index];
            let started = Instant::now();
            let output = probe_compact_lower_declarations(&parsed.arena, decls);
            (output, started.elapsed().as_nanos())
        });
        for (output, elapsed) in outputs {
            file_timing.observe(elapsed);
            counts.type_defs += output.type_defs;
            counts.lowered_aliases += output.lowered_aliases;
            counts.lowered_records += output.lowered_records;
            counts.lowered_tag_unions += output.lowered_tag_unions;
            counts.tag_variants += output.tag_variants;
            counts.tag_arities += output.tag_arities;
            retained.push(output);
        }
    }
    counts.retained_outputs = retained.len();
    std::hint::black_box(&retained);
    (retained, counts, file_timing)
}

fn compact_lower_body_corpus(
    constructs: &[CompactLowerConstructProbeOutput],
    repeat: usize,
) -> (
    Vec<CompactLowerBodyProbeOutput>,
    CompactLowerBodyCounts,
    PhaseFileTiming,
) {
    let mut retained = Vec::<CompactLowerBodyProbeOutput>::new();
    let mut file_timing = PhaseFileTiming::default();
    let mut counts = CompactLowerBodyCounts::default();
    for _ in 0..repeat {
        retained.clear();
        let outputs = parallel_map_indexed(constructs.len(), |index| {
            let construct = &constructs[index];
            let started = Instant::now();
            let output = CompactLowerBodyProbeOutput::from_construct(construct);
            (output, started.elapsed().as_nanos())
        });
        for (output, elapsed) in outputs {
            file_timing.observe(elapsed);
            counts.functions += output.functions;
            counts.lowerable_functions += output.lowerable_functions;
            counts.top_level_statements += output.top_level_statements;
            counts.lowerable_top_level_statements += output.lowerable_top_level_statements;
            counts.statements += output.statements;
            counts.lowerable_statements += output.lowerable_statements;
            counts.expressions += output.expressions;
            counts.lowerable_expressions += output.lowerable_expressions;
            counts.patterns += output.patterns;
            counts.lowerable_patterns += output.lowerable_patterns;
            counts.unsupported_statements += output.unsupported_statements;
            counts.unsupported_expressions += output.unsupported_expressions;
            counts.unsupported_patterns += output.unsupported_patterns;
            counts.expr_type_facts += output.expr_type_facts;
            retained.push(output);
        }
    }
    counts.retained_outputs = retained.len();
    std::hint::black_box(&retained);
    (retained, counts, file_timing)
}

fn compact_lower_construct_corpus(
    files: &[SourceFile],
    parsed: &[ArenaParseOutput],
    decls: &[CompactDeclOutput],
    bodies: &[CompactBodyProbeOutput],
    repeat: usize,
) -> (
    Vec<CompactLowerConstructProbeOutput>,
    CompactLowerConstructCounts,
    PhaseFileTiming,
) {
    let mut retained = Vec::<CompactLowerConstructProbeOutput>::new();
    let mut file_timing = PhaseFileTiming::default();
    let mut counts = CompactLowerConstructCounts::default();
    for _ in 0..repeat {
        retained.clear();
        for (((file, parsed), decls), bodies) in files.iter().zip(parsed).zip(decls).zip(bodies) {
            let started = Instant::now();
            let output =
                probe_compact_lower_constructed_bodies(&parsed.arena, decls, bodies, &file.text);
            file_timing.observe(started.elapsed().as_nanos());
            counts.functions += output.functions;
            counts.constructed_functions += output.constructed_functions;
            counts.constructed_auto_main_functions += output.constructed_auto_main_functions;
            add_fixed_counts(&mut counts.function_blockers, &output.function_blockers);
            add_u32_counts(
                &mut counts.function_return_type_tags,
                &output.function_return_type_tags,
            );
            add_u32_counts(
                &mut counts.function_param_type_tags,
                &output.function_param_type_tags,
            );
            add_u32_counts(
                &mut counts.function_body_tail_stmt_kinds,
                &output.function_body_tail_stmt_kinds,
            );
            add_u32_counts(
                &mut counts.function_body_tail_command_kinds,
                &output.function_body_tail_command_kinds,
            );
            add_string_counts(
                &mut counts.function_body_tail_call_callees,
                &output.function_body_tail_call_callees,
            );
            let mut scc_groups = BTreeSet::new();
            for unit in output.function_units() {
                counts.function_dependency_edges += unit.dependency_edges().len();
                for dependency in unit.dependency_edges() {
                    match dependency {
                        LoweredFunctionKey::Qualified(_) => {
                            counts.function_qualified_dependency_edges += 1;
                        }
                        LoweredFunctionKey::Name(_) => {
                            counts.function_unqualified_dependency_edges += 1;
                        }
                    }
                }
                if let Some(group) = unit.scc_group() {
                    scc_groups.insert(group);
                }
            }
            counts.function_sccs += scc_groups.len();
            counts.top_level_statements += output.top_level_statements;
            counts.constructed_top_level_statements += output.constructed_top_level_statements;
            add_fixed_counts(&mut counts.top_level_blockers, &output.top_level_blockers);
            add_span_samples(
                &mut counts.top_level_blocker_samples,
                &output.top_level_blocker_sample_spans,
                file,
            );
            add_u32_counts(
                &mut counts.top_level_blocker_stmt_kinds,
                &output.top_level_blocker_stmt_kinds,
            );
            add_u32_counts(
                &mut counts.top_level_binding_type_annotation_tags,
                &output.top_level_binding_type_annotation_tags,
            );
            add_u32_counts(
                &mut counts.top_level_binding_type_expr_kinds,
                &output.top_level_binding_type_expr_kinds,
            );
            add_u32_counts(
                &mut counts.top_level_binding_type_call_blockers,
                &output.top_level_binding_type_call_blockers,
            );
            add_string_counts(
                &mut counts.top_level_binding_type_call_callees,
                &output.top_level_binding_type_call_callees,
            );
            add_u32_counts(
                &mut counts.top_level_binding_expression_expr_kinds,
                &output.top_level_binding_expression_expr_kinds,
            );
            add_u32_counts(
                &mut counts.top_level_binding_expression_call_blockers,
                &output.top_level_binding_expression_call_blockers,
            );
            add_string_counts(
                &mut counts.top_level_binding_expression_call_callees,
                &output.top_level_binding_expression_call_callees,
            );
            add_u32_counts(
                &mut counts.top_level_expression_expr_kinds,
                &output.top_level_expression_expr_kinds,
            );
            add_u32_counts(
                &mut counts.top_level_expression_call_blockers,
                &output.top_level_expression_call_blockers,
            );
            add_string_counts(
                &mut counts.top_level_expression_call_callees,
                &output.top_level_expression_call_callees,
            );
            add_u32_counts(
                &mut counts.top_level_command_kinds,
                &output.top_level_command_kinds,
            );
            counts.statements += output.statements;
            counts.constructed_statements += output.constructed_statements;
            add_u32_counts(&mut counts.statement_blockers, &output.statement_blockers);
            add_span_samples(
                &mut counts.statement_blocker_samples,
                &output.statement_blocker_sample_spans,
                file,
            );
            counts.expressions += output.expressions;
            counts.constructed_expressions += output.constructed_expressions;
            add_u32_counts(&mut counts.expression_blockers, &output.expression_blockers);
            add_u32_counts(&mut counts.call_blockers, &output.call_blockers);
            add_string_counts(
                &mut counts.call_blocker_callees,
                &output.call_blocker_callees,
            );
            add_string_samples(
                &mut counts.call_blocker_sample_files,
                &output.call_blocker_callees,
                &file.path,
            );
            add_span_samples(
                &mut counts.call_blocker_samples,
                &output.call_blocker_sample_spans,
                file,
            );
            counts.patterns += output.patterns;
            counts.constructed_patterns += output.constructed_patterns;
            counts.expr_type_facts += output.expr_type_facts;
            retained.push(output);
        }
    }
    counts.retained_outputs = retained.len();
    std::hint::black_box(&retained);
    (retained, counts, file_timing)
}

fn compact_runtime_decl_corpus(
    decls: &[CompactDeclOutput],
    repeat: usize,
) -> (
    Vec<CompactRuntimeDeclProbeOutput>,
    CompactRuntimeDeclCounts,
    PhaseFileTiming,
) {
    let mut retained = Vec::<CompactRuntimeDeclProbeOutput>::new();
    let mut file_timing = PhaseFileTiming::default();
    let mut counts = CompactRuntimeDeclCounts::default();
    for _ in 0..repeat {
        retained.clear();
        let outputs = parallel_map_indexed(decls.len(), |index| {
            let decls = &decls[index];
            let started = Instant::now();
            let output = probe_compact_runtime_declarations(decls);
            (output, started.elapsed().as_nanos())
        });
        for (output, elapsed) in outputs {
            file_timing.observe(elapsed);
            counts.type_defs += output.type_defs;
            counts.tag_arities += output.tag_arities;
            counts.error_families += output.error_families;
            counts.error_variants += output.error_variants;
            counts.error_fields += output.error_fields;
            counts.error_facets += output.error_facets;
            counts.procs += output.procs;
            counts.pures += output.pures;
            counts.streams += output.streams;
            retained.push(output);
        }
    }
    counts.retained_outputs = retained.len();
    std::hint::black_box(&retained);
    (retained, counts, file_timing)
}

fn add_fixed_counts<const N: usize>(left: &mut [usize; N], right: &[usize; N]) {
    for (left, right) in left.iter_mut().zip(right.iter()) {
        *left += *right;
    }
}

fn add_u32_counts<const N: usize>(left: &mut [usize; N], right: &[u32; N]) {
    for (left, right) in left.iter_mut().zip(right.iter()) {
        *left += *right as usize;
    }
}

fn add_string_counts(left: &mut BTreeMap<String, usize>, right: &BTreeMap<String, u32>) {
    for (key, value) in right {
        *left.entry(key.clone()).or_insert(0) += *value as usize;
    }
}

fn add_string_samples(
    samples: &mut BTreeMap<String, Vec<String>>,
    labels: &BTreeMap<String, u32>,
    file: &str,
) {
    for key in labels.keys() {
        let files = samples.entry(key.clone()).or_default();
        if files.len() < 8 && !files.iter().any(|sample| sample == file) {
            files.push(file.to_string());
        }
    }
}

fn add_span_samples(
    samples: &mut BTreeMap<String, Vec<String>>,
    spans: &BTreeMap<String, Vec<Span>>,
    file: &SourceFile,
) {
    for (label, label_spans) in spans {
        let label_samples = samples.entry(label.clone()).or_default();
        for span in label_spans {
            if label_samples.len() >= 8 {
                break;
            }
            let sample = format_span_sample(file, *span);
            if !label_samples.iter().any(|existing| existing == &sample) {
                label_samples.push(sample);
            }
        }
    }
}

fn format_span_sample(file: &SourceFile, span: Span) -> String {
    let start = span.start().min(file.text.len());
    let line_start = file.text[..start]
        .rfind('\n')
        .map(|offset| offset + 1)
        .unwrap_or(0);
    let line = file.text[..start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let column = file.text[line_start..start].chars().count() + 1;
    let end = span.end().min(file.text.len());
    let snippet = file
        .text
        .get(start..end)
        .unwrap_or("")
        .replace(['\r', '\n'], " ");
    format!("{}:{}:{}: {}", file.path, line, column, snippet)
}

fn compact_hot_path_corpus(
    files: &[SourceFile],
    parsed: &[ArenaParseOutput],
    decls: &[CompactDeclOutput],
    bodies: &[CompactBodyProbeOutput],
    constructs: &[CompactLowerConstructProbeOutput],
    repeat: usize,
) -> CompactHotPathCounts {
    let mut counts = CompactHotPathCounts {
        unique_files: parsed.len(),
        ..CompactHotPathCounts::default()
    };
    for _ in 0..repeat {
        for ((((file, parsed), decls), bodies), constructs) in files
            .iter()
            .zip(parsed)
            .zip(decls)
            .zip(bodies)
            .zip(constructs)
        {
            counts.file_runs += 1;
            let executable_top_level_statements = parsed
                .arena
                .statement_ids()
                .filter(|stmt| !compact_top_level_stmt_is_skippable(&parsed.arena, *stmt))
                .count();
            counts.executable_top_level_statements += executable_top_level_statements;
            counts.constructed_top_level_statements += constructs.constructed_top_level_statements;

            let parse_blocked = !parsed.diagnostics.is_empty();
            let module_blocked = parsed
                .arena
                .modules
                .iter()
                .any(|module| module.statements.is_empty());
            let decl_blocked = !decls.diagnostics.is_empty();
            let body_blocked = !bodies.diagnostics.is_empty();
            let no_executable = executable_top_level_statements == 0;
            let top_level_blocked =
                constructs.constructed_top_level_statements < executable_top_level_statements;
            let auto_main_blocked = false;

            if parse_blocked {
                counts.parse_diagnostic_file_runs += 1;
            }
            if module_blocked {
                counts.module_file_runs += 1;
            }
            if decl_blocked {
                counts.declaration_diagnostic_file_runs += 1;
            }
            if body_blocked {
                counts.body_diagnostic_file_runs += 1;
            }
            if no_executable {
                counts.no_executable_top_level_file_runs += 1;
            }
            if auto_main_blocked {
                counts.auto_main_file_runs += 1;
            }
            if top_level_blocked {
                counts.unconstructed_top_level_file_runs += 1;
                counts.unconstructed_top_level_statements += executable_top_level_statements
                    .saturating_sub(constructs.constructed_top_level_statements);
            }
            if bodies.unsupported_statements > 0 {
                counts.unsupported_body_statement_file_runs += 1;
            }
            if bodies.unsupported_expressions > 0 {
                counts.unsupported_body_expression_file_runs += 1;
            }
            if constructs.functions > constructs.constructed_functions {
                counts.unconstructed_function_file_runs += 1;
                counts.unconstructed_functions += constructs
                    .functions
                    .saturating_sub(constructs.constructed_functions);
            }

            if parse_blocked
                || module_blocked
                || decl_blocked
                || body_blocked
                || auto_main_blocked
                || top_level_blocked
            {
                counts.compact_blocked_file_runs += 1;
                *counts
                    .compact_blocked_files
                    .entry(file.path.clone())
                    .or_insert(0) += 1;
            } else {
                counts.compact_hot_path_file_runs += 1;
            }
        }
    }
    counts
}

fn walk_compact_program(program: &ArenaProgram, counts: &mut CompactWalkCounts) {
    counts.type_exprs += program.arena.type_expr_tags.len();
    counts.type_defs += program.arena.type_defs.len();
    counts.error_defs += program.arena.error_defs.len();
    for id in program.statement_ids() {
        walk_stmt(program, id, counts);
    }
    for module in &program.modules {
        for id in program.module_statements(module) {
            walk_stmt(program, id, counts);
        }
    }
}

fn walk_stmt(program: &ArenaProgram, id: StmtId, counts: &mut CompactWalkCounts) {
    counts.statements += 1;
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Use(_) => {}
        ArenaStmtKind::Export(stmt) => walk_stmt(program, stmt, counts),
        ArenaStmtKind::TypeDef(def) => walk_type_def(program, def, counts),
        ArenaStmtKind::ErrorDef(def) => walk_error_def(program, def, counts),
        ArenaStmtKind::Let {
            target,
            initializer,
            ..
        }
        | ArenaStmtKind::Var {
            target,
            initializer,
            ..
        }
        | ArenaStmtKind::Guard {
            target,
            initializer,
            ..
        } => {
            walk_binding_target(program, target, counts);
            walk_expr_or_run(program, initializer, counts);
            if let ArenaStmtKind::Guard { else_block, .. } = program.arena.stmt(id).kind {
                walk_block(program, else_block, counts);
            }
        }
        ArenaStmtKind::Assign { target, value, .. } => {
            walk_assign_target(program, target, counts);
            walk_expr_or_run(program, value, counts);
        }
        ArenaStmtKind::ProcDef(function)
        | ArenaStmtKind::PureDef(function)
        | ArenaStmtKind::StreamDef(function) => walk_function_def(program, function, counts),
        ArenaStmtKind::SignalHook(hook) => {
            let hook = program.arena.signal_hook(hook);
            walk_block(program, hook.body, counts);
        }
        ArenaStmtKind::Return(value) => {
            if let Some(value) = value {
                walk_expr_or_run(program, value, counts);
            }
        }
        ArenaStmtKind::Yield(value) | ArenaStmtKind::Defer(value) => {
            walk_expr_or_run(program, value, counts);
        }
        ArenaStmtKind::If {
            branches,
            else_block,
        } => {
            for branch in program.arena.if_branches(branches) {
                walk_expr(program, branch.condition, counts);
                walk_block(program, branch.block, counts);
            }
            if let Some(block) = else_block {
                walk_block(program, block, counts);
            }
        }
        ArenaStmtKind::While { condition, block } => {
            walk_expr(program, condition, counts);
            walk_block(program, block, counts);
        }
        ArenaStmtKind::For {
            target,
            iter,
            block,
        } => {
            walk_binding_target(program, target, counts);
            walk_expr(program, iter, counts);
            walk_block(program, block, counts);
        }
        ArenaStmtKind::With {
            bindings,
            body,
            else_block,
            ..
        } => {
            for binding in program.arena.with_bindings(bindings) {
                walk_expr(program, binding.initializer, counts);
            }
            walk_block(program, body, counts);
            walk_block(program, else_block, counts);
        }
        ArenaStmtKind::Loop { block } => walk_block(program, block, counts),
        ArenaStmtKind::GuardedStmt {
            stmt, condition, ..
        } => {
            walk_stmt(program, stmt, counts);
            walk_expr(program, condition, counts);
        }
        ArenaStmtKind::Break { value } => {
            if let Some(value) = value {
                walk_expr(program, value, counts);
            }
        }
        ArenaStmtKind::Continue | ArenaStmtKind::TailBareIdent(_) => {}
        ArenaStmtKind::Match { value, arms } => {
            walk_expr(program, value, counts);
            for arm in program.arena.match_arms(arms) {
                walk_pattern(program, arm.pattern, counts);
                if let Some(guard) = arm.guard {
                    walk_expr(program, guard, counts);
                }
                walk_block(program, arm.block, counts);
            }
        }
        ArenaStmtKind::Command(command) => walk_command_stmt(program, command, counts),
        ArenaStmtKind::Expr(expr) => walk_expr(program, expr, counts),
    }
}

fn walk_type_def(program: &ArenaProgram, id: TypeDefId, counts: &mut CompactWalkCounts) {
    let def = program.arena.type_def(id);
    match def.body {
        ArenaTypeDefBody::Alias(_) => {}
        ArenaTypeDefBody::RecordSchema(fields) => {
            counts.definition_fields += program.arena.schema_fields(fields).len();
        }
        ArenaTypeDefBody::ModuleContract(entries) => {
            for entry in program.arena.module_contract_entries(entries) {
                counts.definition_fields += 1;
                match entry.kind {
                    ArenaModuleContractEntryKind::Value(_) => {}
                    ArenaModuleContractEntryKind::Proc { params, .. }
                    | ArenaModuleContractEntryKind::Pure { params, .. } => {
                        for param in program.arena.params(params) {
                            counts.params += 1;
                            if let Some(default) = param.default {
                                walk_expr(program, default, counts);
                            }
                        }
                    }
                }
            }
        }
        ArenaTypeDefBody::TagUnion(variants) => {
            for variant in program.arena.tag_variants(variants) {
                counts.definition_fields += program.arena.extra_range(variant.fields).len();
            }
        }
    }
}

fn walk_error_def(program: &ArenaProgram, id: ErrorDefId, counts: &mut CompactWalkCounts) {
    let def = program.arena.error_def(id);
    for variant in program.arena.error_variants(def.variants) {
        counts.definition_fields += 1 + program.arena.error_fields(variant.fields).len();
        counts.definition_fields += program.arena.names(variant.facets).count();
    }
}

fn walk_expr_or_run(program: &ArenaProgram, value: ArenaExprOrRun, counts: &mut CompactWalkCounts) {
    match value {
        ArenaExprOrRun::Expr(expr) => walk_expr(program, expr, counts),
        ArenaExprOrRun::Run(run) => walk_run_form(program, run, counts),
    }
}

fn walk_block(program: &ArenaProgram, id: BlockId, counts: &mut CompactWalkCounts) {
    counts.blocks += 1;
    let block = program.arena.block(id);
    counts.block_params += program.arena.block_params(block.params).len();
    for stmt in program.arena.stmt_ids(block.statements) {
        walk_stmt(program, stmt, counts);
    }
}

fn walk_function_def(program: &ArenaProgram, id: FunctionDefId, counts: &mut CompactWalkCounts) {
    counts.function_defs += 1;
    let function = program.arena.function_def(id);
    for param in program.arena.params(function.params) {
        counts.params += 1;
        if let Some(default) = param.default {
            walk_expr(program, default, counts);
        }
    }
    walk_block(program, function.body, counts);
}

fn walk_binding_target(
    program: &ArenaProgram,
    id: BindingTargetId,
    counts: &mut CompactWalkCounts,
) {
    counts.binding_targets += 1;
    match &program.arena.binding_target(id).kind {
        ArenaBindingTargetKind::Name(_) => {}
        ArenaBindingTargetKind::Record { fields, .. } => {
            counts.destructure_fields += program.arena.destructure_fields(*fields).len();
        }
    }
}

fn walk_assign_target(program: &ArenaProgram, id: AssignTargetId, counts: &mut CompactWalkCounts) {
    counts.assign_targets += 1;
    match &program.arena.assign_target(id).kind {
        ArenaAssignTargetKind::Name(_) => {}
        ArenaAssignTargetKind::Field { base, .. } => walk_assign_target(program, *base, counts),
        ArenaAssignTargetKind::Index { base, index } => {
            walk_assign_target(program, *base, counts);
            walk_expr(program, *index, counts);
        }
    }
}

fn walk_pattern(program: &ArenaProgram, id: PatternId, counts: &mut CompactWalkCounts) {
    counts.patterns += 1;
    match &program.arena.pattern(id).kind {
        ArenaPatternKind::Wildcard | ArenaPatternKind::Binding(_) | ArenaPatternKind::Facet(_) => {}
        ArenaPatternKind::Type { .. } => {}
        ArenaPatternKind::Literal(expr) => walk_expr(program, *expr, counts),
        ArenaPatternKind::Record { fields, .. } => {
            for field in program.arena.pattern_fields(*fields) {
                walk_pattern(program, field.pattern, counts);
            }
        }
        ArenaPatternKind::Alternation(items) | ArenaPatternKind::Tuple(items) => {
            for pattern in program.arena.pattern_ids(*items) {
                walk_pattern(program, pattern, counts);
            }
        }
        ArenaPatternKind::Constructor { arg, .. } => {
            if let Some(arg) = arg {
                walk_pattern(program, *arg, counts);
            }
        }
        ArenaPatternKind::ErrorVariant { fields, .. } => {
            for field in program.arena.pattern_fields(*fields) {
                walk_pattern(program, field.pattern, counts);
            }
        }
    }
}

fn walk_expr(program: &ArenaProgram, id: ExprId, counts: &mut CompactWalkCounts) {
    counts.expressions += 1;
    match program.arena.expr(id).kind {
        ArenaExprKind::Null
        | ArenaExprKind::Bool(_)
        | ArenaExprKind::Int(_)
        | ArenaExprKind::Float(_)
        | ArenaExprKind::Duration(_)
        | ArenaExprKind::Str(_)
        | ArenaExprKind::PathStr(_)
        | ArenaExprKind::GlobStr(_)
        | ArenaExprKind::Bytes(_)
        | ArenaExprKind::Ident(_)
        | ArenaExprKind::Item
        | ArenaExprKind::LastStatus
        | ArenaExprKind::EnvGet { .. }
        | ArenaExprKind::EnvPathList => {}
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
            for part in program.arena.fmt_parts(parts) {
                if let ArenaFmtPart::Expr(expr, _) = part {
                    walk_expr(program, expr, counts);
                }
            }
        }
        ArenaExprKind::List(items) => {
            for expr in program.arena.expr_ids(items) {
                walk_expr(program, expr, counts);
            }
        }
        ArenaExprKind::ListComp {
            expr,
            target,
            iter,
            condition,
        } => {
            walk_binding_target(program, target, counts);
            walk_expr(program, expr, counts);
            walk_expr(program, iter, counts);
            if let Some(condition) = condition {
                walk_expr(program, condition, counts);
            }
        }
        ArenaExprKind::MapComp {
            key,
            value,
            target,
            iter,
            condition,
        } => {
            walk_binding_target(program, target, counts);
            walk_expr(program, key, counts);
            walk_expr(program, value, counts);
            walk_expr(program, iter, counts);
            if let Some(condition) = condition {
                walk_expr(program, condition, counts);
            }
        }
        ArenaExprKind::Record(fields) => {
            for field in program.arena.record_fields(fields) {
                match &field.kind {
                    ArenaRecordFieldKind::Named { value, .. }
                    | ArenaRecordFieldKind::Spread { expr: value, .. } => {
                        walk_expr(program, *value, counts);
                    }
                    ArenaRecordFieldKind::Shorthand { .. } => {}
                }
            }
        }
        ArenaExprKind::If {
            branches,
            else_value,
        } => {
            for branch in program.arena.if_expr_branches(branches) {
                walk_expr(program, branch.condition, counts);
                walk_expr(program, branch.value, counts);
            }
            walk_expr(program, else_value, counts);
        }
        ArenaExprKind::Match { value, arms } => {
            walk_expr(program, value, counts);
            for arm in program.arena.match_expr_arms(arms) {
                walk_pattern(program, arm.pattern, counts);
                if let Some(guard) = arm.guard {
                    walk_expr(program, guard, counts);
                }
                walk_expr(program, arm.value, counts);
            }
        }
        ArenaExprKind::Unary { expr, .. } | ArenaExprKind::Try(expr) => {
            walk_expr(program, expr, counts);
        }
        ArenaExprKind::Binary { left, right, .. } => {
            walk_expr(program, left, counts);
            walk_expr(program, right, counts);
        }
        ArenaExprKind::Call { callee, args } => {
            walk_expr(program, callee, counts);
            for arg in program.arena.call_args(args) {
                match &arg.kind {
                    ArenaCallArgKind::Positional(expr)
                    | ArenaCallArgKind::Splice { value: expr, .. }
                    | ArenaCallArgKind::Named { value: expr, .. } => {
                        walk_expr(program, *expr, counts)
                    }
                }
            }
        }
        ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
            walk_expr(program, base, counts);
        }
        ArenaExprKind::Index { base, index } => {
            walk_expr(program, base, counts);
            walk_expr(program, index, counts);
        }
        ArenaExprKind::Slice { base, start, end } => {
            walk_expr(program, base, counts);
            if let Some(start) = start {
                walk_expr(program, start, counts);
            }
            if let Some(end) = end {
                walk_expr(program, end, counts);
            }
        }
        ArenaExprKind::Pipeline { input, stages } => {
            walk_expr(program, input, counts);
            for stage in program.arena.pipe_stages(stages) {
                match &stage.kind {
                    ArenaPipeStageKind::Expr(expr) => walk_expr(program, *expr, counts),
                    ArenaPipeStageKind::Stream(stage) => walk_stream_stage(program, stage, counts),
                }
            }
        }
        ArenaExprKind::StructuredPipeline { input, stages } => {
            walk_expr(program, input, counts);
            for stage in program.arena.stream_stages(stages) {
                walk_stream_stage(program, stage, counts);
            }
        }
        ArenaExprKind::Run(run) => walk_run_form(program, run, counts),
        ArenaExprKind::Spawn(form) => match form.target {
            ArenaSpawnTarget::Run(run) => walk_run_form(program, run, counts),
            ArenaSpawnTarget::Command(expr) => walk_expr(program, expr, counts),
        },
        ArenaExprKind::Wait(wait) => walk_expr(program, wait.target, counts),
        ArenaExprKind::BuilderCall { call, block } => {
            walk_expr(program, call, counts);
            walk_builder_block(program, block, counts);
        }
        ArenaExprKind::Require { value, .. } => walk_expr(program, value, counts),
        ArenaExprKind::Loop { block } => walk_block(program, block, counts),
        ArenaExprKind::Retry { delays, block } => {
            for delay in program.arena.expr_ids(delays) {
                walk_expr(program, delay, counts);
            }
            walk_block(program, block, counts);
        }
    }
}

fn walk_command_stmt(program: &ArenaProgram, id: CommandStmtId, counts: &mut CompactWalkCounts) {
    counts.command_stmts += 1;
    match &program.arena.command_stmt(id).command {
        ArenaCommand::Proc { args, .. } => {
            for arg in program.arena.command_args(*args) {
                walk_command_arg(program, arg, counts);
            }
        }
        ArenaCommand::Core {
            args, env, block, ..
        } => {
            for env in program.arena.env_assignments(*env) {
                walk_env_assignment(program, env, counts);
            }
            for arg in program.arena.command_args(*args) {
                walk_command_arg(program, arg, counts);
            }
            if let Some(block) = block {
                walk_block(program, *block, counts);
            }
        }
        ArenaCommand::Run(run) => walk_run_form(program, *run, counts),
    }
}

fn walk_run_form(program: &ArenaProgram, id: RunFormId, counts: &mut CompactWalkCounts) {
    counts.run_forms += 1;
    let run = program.arena.run_form(id);
    for segment in program.arena.run_segments(run.segments) {
        if let Some(timeout) = segment.timeout {
            walk_expr(program, timeout, counts);
        }
        if let Some(cpu_max) = segment.cpu_max {
            walk_expr(program, cpu_max, counts);
        }
        for env in program.arena.env_assignments(segment.env) {
            walk_env_assignment(program, env, counts);
        }
        walk_command_arg(program, &segment.target, counts);
        for arg in program.arena.command_args(segment.args) {
            walk_command_arg(program, arg, counts);
        }
        for redirection in program.arena.redirections(segment.redirections) {
            match &redirection.target {
                ArenaRedirectionTarget::Path(arg) | ArenaRedirectionTarget::Fd(arg) => {
                    walk_command_arg(program, arg, counts);
                }
            }
        }
    }
}

fn walk_env_assignment(
    program: &ArenaProgram,
    assignment: &ArenaEnvAssignment,
    counts: &mut CompactWalkCounts,
) {
    match &assignment.value {
        ArenaEnvAssignmentValue::CommandArg(arg) => walk_command_arg(program, arg, counts),
        ArenaEnvAssignmentValue::Expr(expr) => walk_expr(program, *expr, counts),
    }
}

fn walk_command_arg(program: &ArenaProgram, arg: &ArenaCommandArg, counts: &mut CompactWalkCounts) {
    counts.command_args += 1;
    match arg.kind {
        ArenaCommandArgKind::Word(parts) => {
            for part in program.arena.word_parts(parts) {
                match part {
                    ArenaWordPart::Bare(_) | ArenaWordPart::Quoted(_) => {}
                    ArenaWordPart::Shorthand(expr) | ArenaWordPart::Interpolation(expr) => {
                        walk_expr(program, expr, counts);
                    }
                }
            }
        }
        ArenaCommandArgKind::SpliceName(_) => {}
        ArenaCommandArgKind::SpliceExpr(expr) | ArenaCommandArgKind::Typed(expr) => {
            walk_expr(program, expr, counts);
        }
    }
}

fn walk_stream_stage(
    program: &ArenaProgram,
    stage: &ArenaStreamStage,
    counts: &mut CompactWalkCounts,
) {
    for option in program.arena.stream_options(stage.options) {
        if let Some(value) = option.value {
            walk_expr(program, value, counts);
        }
    }
    for arg in program.arena.call_args(stage.args) {
        match &arg.kind {
            ArenaCallArgKind::Positional(expr)
            | ArenaCallArgKind::Splice { value: expr, .. }
            | ArenaCallArgKind::Named { value: expr, .. } => walk_expr(program, *expr, counts),
        }
    }
    if let Some(block) = stage.block {
        walk_block(program, block, counts);
    }
}

fn walk_builder_block(program: &ArenaProgram, id: BuilderBlockId, counts: &mut CompactWalkCounts) {
    counts.builder_blocks += 1;
    let block = program.arena.builder_block(id);
    for entry in program.arena.builder_entries(block.entries) {
        match &entry.kind {
            ArenaBuilderEntryKind::Field { value, .. } => walk_expr(program, *value, counts),
            ArenaBuilderEntryKind::Entry { args, block, .. } => {
                for arg in program.arena.command_args(*args) {
                    walk_command_arg(program, arg, counts);
                }
                if let Some(block) = block {
                    walk_builder_block(program, *block, counts);
                }
            }
            ArenaBuilderEntryKind::Task { block, .. } => walk_block(program, *block, counts),
            ArenaBuilderEntryKind::Stmt(stmt) => walk_stmt(program, *stmt, counts),
        }
    }
}

fn compact_top_level_stmt_is_skippable(program: &ArenaProgram, id: StmtId) -> bool {
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Export(inner) => compact_top_level_stmt_is_skippable(program, inner),
        ArenaStmtKind::Use(use_id) => compact_use_stmt_is_skippable(program, use_id),
        ArenaStmtKind::TypeDef(_)
        | ArenaStmtKind::ErrorDef(_)
        | ArenaStmtKind::ProcDef(_)
        | ArenaStmtKind::PureDef(_)
        | ArenaStmtKind::StreamDef(_) => true,
        _ => false,
    }
}

fn compact_use_stmt_is_skippable(program: &ArenaProgram, id: UseStmtId) -> bool {
    let use_stmt = program.arena.use_stmt(id);
    if use_stmt.alias.is_some() || use_stmt.resolved.is_some() {
        return false;
    }
    let mut path = program.arena.names(use_stmt.path);
    let Some(name) = path.next() else {
        return false;
    };
    path.next().is_none() && api_spec().is_standard_module(name.as_str())
}

fn measure_phase<T>(run: impl FnOnce() -> T) -> (T, PhaseMetrics) {
    reset_allocations();
    let started = Instant::now();
    let output = run();
    let elapsed_ns = started.elapsed().as_nanos();
    let allocations = allocation_snapshot();
    (
        output,
        PhaseMetrics {
            elapsed_ns,
            allocations,
        },
    )
}

#[derive(Default)]
struct PhaseFileTiming {
    samples: Vec<u128>,
}

impl PhaseFileTiming {
    fn observe(&mut self, elapsed_ns: u128) {
        self.samples.push(elapsed_ns);
    }

    fn summary(&self) -> PhaseFileSummary {
        PhaseFileSummary::from_samples(&self.samples)
    }
}

#[derive(Clone, Copy, Default)]
struct PhaseFileSummary {
    total_ns: u128,
    p50_ns: u128,
    p95_ns: u128,
    max_ns: u128,
}

impl PhaseFileSummary {
    fn from_samples(samples: &[u128]) -> Self {
        if samples.is_empty() {
            return Self::default();
        }
        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        Self {
            total_ns: samples.iter().sum(),
            p50_ns: percentile(&sorted, 50),
            p95_ns: percentile(&sorted, 95),
            max_ns: sorted[sorted.len() - 1],
        }
    }
}

struct PhaseFileSummaries {
    parse_arena_only: PhaseFileSummary,
    declaration_probe: PhaseFileSummary,
    body_probe: PhaseFileSummary,
    lowering_declaration_probe: PhaseFileSummary,
    lowering_construct_probe: PhaseFileSummary,
    lowering_body_probe: PhaseFileSummary,
    runtime_declaration_registration: PhaseFileSummary,
}

fn percentile(sorted: &[u128], percentile: usize) -> u128 {
    let index = ((sorted.len() - 1) * percentile).div_ceil(100);
    sorted[index]
}

struct FileCounts {
    path: String,
    bytes: usize,
    tokens: usize,
    syntax_tokens: usize,
    statements: usize,
    imports: usize,
    exports: usize,
    type_defs: usize,
    error_defs: usize,
    function_defs: usize,
    executable_top_level_statements: usize,
}

fn file_counts_for_parsed(file: &SourceFile, parsed: &ArenaParseOutput) -> FileCounts {
    let stats = parsed.arena.stats();
    FileCounts {
        path: file.path.clone(),
        bytes: file.text.len(),
        tokens: parsed.cst.token_table().len(),
        syntax_tokens: parsed.cst.get().tokens().len(),
        statements: stats.statements,
        imports: stats.use_stmts,
        exports: count_exports(&parsed.arena),
        type_defs: stats.type_defs,
        error_defs: stats.error_defs,
        function_defs: stats.function_defs,
        executable_top_level_statements: parsed
            .arena
            .statement_ids()
            .filter(|stmt| !compact_top_level_stmt_is_skippable(&parsed.arena, *stmt))
            .count(),
    }
}

fn count_exports(program: &ArenaProgram) -> usize {
    let root = program
        .statement_ids()
        .filter(|stmt| matches!(program.arena.stmt(*stmt).kind, ArenaStmtKind::Export(_)))
        .count();
    let modules = program
        .modules
        .iter()
        .flat_map(|module| program.module_statements(module))
        .filter(|stmt| matches!(program.arena.stmt(*stmt).kind, ArenaStmtKind::Export(_)))
        .count();
    root + modules
}

struct ModuleGraphReadiness {
    import_edge_count: usize,
    unique_module_count: usize,
    qualified_declaration_count: usize,
    duplicate_diagnostic_count: usize,
    largest_dependency_component: usize,
}

fn module_graph_readiness(
    files: &[SourceFile],
    parsed: &[ArenaParseOutput],
    decls: &[CompactDeclOutput],
) -> ModuleGraphReadiness {
    let mut modules = BTreeSet::new();
    let mut edges = Vec::<(String, String)>::new();
    let mut import_edge_count = 0usize;

    for (file, parsed) in files.iter().zip(parsed) {
        modules.insert(file.path.clone());
        for module in &parsed.arena.modules {
            modules.insert(module.name.as_str().to_string());
        }
        let mut statements = parsed.arena.statement_ids().collect::<Vec<_>>();
        for module in &parsed.arena.modules {
            statements.extend(parsed.arena.module_statements(module));
        }
        for stmt in statements {
            if let ArenaStmtKind::Use(use_id) = parsed.arena.arena.stmt(stmt).kind {
                let use_stmt = parsed.arena.arena.use_stmt(use_id);
                import_edge_count += 1;
                let target = use_stmt
                    .resolved
                    .as_deref()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| {
                        parsed
                            .arena
                            .arena
                            .names(use_stmt.path)
                            .map(|name| name.as_str())
                            .collect::<Vec<_>>()
                            .join(".")
                    });
                modules.insert(target.clone());
                edges.push((file.path.clone(), target));
            }
        }
    }

    let qualified_declaration_count = decls
        .iter()
        .map(|decls| {
            decls.qualified_error_families.len()
                + decls.qualified_procs.len()
                + decls.qualified_pures.len()
                + decls.qualified_streams.len()
        })
        .sum();
    let duplicate_diagnostic_count = decls
        .iter()
        .flat_map(|decls| decls.diagnostics.iter())
        .filter(|diagnostic| diagnostic.message.contains("duplicate"))
        .count();

    ModuleGraphReadiness {
        import_edge_count,
        unique_module_count: modules.len(),
        qualified_declaration_count,
        duplicate_diagnostic_count,
        largest_dependency_component: largest_component_size(&modules, &edges),
    }
}

fn largest_component_size(nodes: &BTreeSet<String>, edges: &[(String, String)]) -> usize {
    let mut graph = BTreeMap::<String, Vec<String>>::new();
    for node in nodes {
        graph.entry(node.clone()).or_default();
    }
    for (left, right) in edges {
        graph.entry(left.clone()).or_default().push(right.clone());
        graph.entry(right.clone()).or_default().push(left.clone());
    }

    let mut seen = BTreeSet::new();
    let mut largest = 0usize;
    for node in graph.keys() {
        if seen.contains(node) {
            continue;
        }
        let mut stack = vec![node.clone()];
        let mut size = 0usize;
        while let Some(current) = stack.pop() {
            if !seen.insert(current.clone()) {
                continue;
            }
            size += 1;
            if let Some(neighbors) = graph.get(&current) {
                for neighbor in neighbors {
                    if !seen.contains(neighbor) {
                        stack.push(neighbor.clone());
                    }
                }
            }
        }
        largest = largest.max(size);
    }
    largest
}

struct FunctionLoweringReadiness {
    attempted_functions: usize,
    lowered_functions: usize,
    blocked_functions: usize,
    dependency_edge_count: usize,
    scc_count: usize,
    blocker_kind_counts: BTreeMap<String, usize>,
    qualified_call_count: usize,
    unqualified_call_count: usize,
}

fn function_lowering_readiness(counts: &CompactLowerConstructCounts) -> FunctionLoweringReadiness {
    let mut blocker_kind_counts = BTreeMap::new();
    for (index, count) in counts.function_blockers.iter().enumerate() {
        blocker_kind_counts.insert(function_blocker_label(index).to_string(), *count);
    }
    FunctionLoweringReadiness {
        attempted_functions: counts.functions,
        lowered_functions: counts.constructed_functions,
        blocked_functions: counts
            .functions
            .saturating_sub(counts.constructed_functions),
        dependency_edge_count: counts.function_dependency_edges,
        scc_count: counts.function_sccs,
        blocker_kind_counts,
        qualified_call_count: counts.function_qualified_dependency_edges,
        unqualified_call_count: counts.function_unqualified_dependency_edges,
    }
}

fn function_blocker_label(index: usize) -> &'static str {
    match index {
        0 => "return_type",
        1 => "param_default",
        2 => "param_type",
        3 => "block_params",
        4 => "body",
        5 => "no_return",
        _ => "unknown",
    }
}

struct TopLevelReadiness {
    attempted_statements: usize,
    lowered_statements: usize,
    skipped_statements: usize,
    blocked_statements: usize,
    fallback_reason_counts: BTreeMap<String, usize>,
}

fn top_level_readiness(counts: &CompactLowerConstructCounts) -> TopLevelReadiness {
    let mut fallback_reason_counts = BTreeMap::new();
    for (index, count) in counts.top_level_blockers.iter().enumerate() {
        fallback_reason_counts.insert(top_level_blocker_label(index).to_string(), *count);
    }
    let blocked_statements = fallback_reason_counts.values().sum();

    TopLevelReadiness {
        attempted_statements: counts.top_level_statements,
        lowered_statements: counts.constructed_top_level_statements,
        skipped_statements: counts
            .top_level_statements
            .saturating_sub(counts.constructed_top_level_statements)
            .saturating_sub(blocked_statements),
        blocked_statements,
        fallback_reason_counts,
    }
}

fn top_level_blocker_label(index: usize) -> &'static str {
    match index {
        0 => "use",
        1 => "binding_target",
        2 => "binding_type",
        3 => "binding_expression",
        4 => "assign_target",
        5 => "assign_expression",
        6 => "control",
        7 => "command",
        8 => "expression",
        9 => "defer",
        10 => "other",
        _ => "unknown",
    }
}

#[derive(Default)]
struct ParseCounts {
    statements: usize,
    modules: usize,
    syntax_tokens: usize,
    compact_token_table_tokens: usize,
    compact_token_row_bytes: usize,
    compact_token_retained_bytes: usize,
    arena: ArenaCounts,
    diagnostics: usize,
    retained_outputs: usize,
}

#[derive(Default)]
struct CompactWalkCounts {
    programs: usize,
    diagnostics: usize,
    statements: usize,
    expressions: usize,
    patterns: usize,
    type_exprs: usize,
    blocks: usize,
    block_params: usize,
    params: usize,
    binding_targets: usize,
    assign_targets: usize,
    destructure_fields: usize,
    function_defs: usize,
    type_defs: usize,
    error_defs: usize,
    definition_fields: usize,
    command_stmts: usize,
    command_args: usize,
    run_forms: usize,
    builder_blocks: usize,
    compat_payloads: usize,
    unsupported_statements: usize,
}

#[derive(Default)]
struct CompactDeclCounts {
    diagnostics: usize,
    type_defs: usize,
    tag_variants: usize,
    error_families: usize,
    error_variants: usize,
    error_fields: usize,
    function_defs: usize,
    params: usize,
    schema_fields: usize,
    module_contract_entries: usize,
    type_states: usize,
    tag_variant_states: usize,
    error_family_states: usize,
    proc_sigs: usize,
    pure_sigs: usize,
    stream_sigs: usize,
    qualified_proc_sigs: usize,
    qualified_pure_sigs: usize,
    qualified_stream_sigs: usize,
    retained_outputs: usize,
}

#[derive(Default)]
struct CompactBodyCounts {
    diagnostics: usize,
    statements: usize,
    supported_statements: usize,
    unsupported_statements: usize,
    expressions: usize,
    typed_expressions: usize,
    unsupported_expressions: usize,
    bindings: usize,
    assignment_targets: usize,
    blocks: usize,
    functions: usize,
    commands: usize,
    runs: usize,
    unsupported_signal_hooks: usize,
    unsupported_with_stmts: usize,
    unsupported_guards: usize,
    unsupported_guarded_stmts: usize,
    unsupported_item_exprs: usize,
    unsupported_list_comps: usize,
    unsupported_map_comps: usize,
    unsupported_match_exprs: usize,
    unsupported_pipeline_exprs: usize,
    unsupported_structured_pipeline_exprs: usize,
    unsupported_builder_call_exprs: usize,
    expr_type_facts: usize,
    retained_outputs: usize,
}

#[derive(Default)]
struct CompactLowerDeclCounts {
    type_defs: usize,
    lowered_aliases: usize,
    lowered_records: usize,
    lowered_tag_unions: usize,
    tag_variants: usize,
    tag_arities: usize,
    retained_outputs: usize,
}

#[derive(Default)]
struct CompactLowerBodyCounts {
    functions: usize,
    lowerable_functions: usize,
    top_level_statements: usize,
    lowerable_top_level_statements: usize,
    statements: usize,
    lowerable_statements: usize,
    expressions: usize,
    lowerable_expressions: usize,
    patterns: usize,
    lowerable_patterns: usize,
    unsupported_statements: usize,
    unsupported_expressions: usize,
    unsupported_patterns: usize,
    expr_type_facts: usize,
    retained_outputs: usize,
}

struct CompactLowerConstructCounts {
    functions: usize,
    constructed_functions: usize,
    constructed_auto_main_functions: usize,
    function_blockers: [usize; COMPACT_FUNCTION_BLOCKER_KIND_COUNT],
    function_return_type_tags: [usize; COMPACT_TYPE_EXPR_TAG_COUNT],
    function_param_type_tags: [usize; COMPACT_TYPE_EXPR_TAG_COUNT],
    function_body_tail_stmt_kinds: [usize; COMPACT_STMT_KIND_COUNT],
    function_body_tail_command_kinds: [usize; COMPACT_COMMAND_BLOCKER_KIND_COUNT],
    function_body_tail_call_callees: BTreeMap<String, usize>,
    function_dependency_edges: usize,
    function_sccs: usize,
    function_qualified_dependency_edges: usize,
    function_unqualified_dependency_edges: usize,
    top_level_statements: usize,
    constructed_top_level_statements: usize,
    top_level_blockers: [usize; COMPACT_TOP_LEVEL_BLOCKER_KIND_COUNT],
    top_level_blocker_samples: BTreeMap<String, Vec<String>>,
    top_level_blocker_stmt_kinds: [usize; COMPACT_STMT_KIND_COUNT],
    top_level_binding_type_annotation_tags: [usize; COMPACT_TYPE_EXPR_TAG_COUNT],
    top_level_binding_type_expr_kinds: [usize; COMPACT_EXPR_KIND_COUNT],
    top_level_binding_type_call_blockers: [usize; COMPACT_CALL_BLOCKER_KIND_COUNT],
    top_level_binding_type_call_callees: BTreeMap<String, usize>,
    top_level_binding_expression_expr_kinds: [usize; COMPACT_EXPR_KIND_COUNT],
    top_level_binding_expression_call_blockers: [usize; COMPACT_CALL_BLOCKER_KIND_COUNT],
    top_level_binding_expression_call_callees: BTreeMap<String, usize>,
    top_level_expression_expr_kinds: [usize; COMPACT_EXPR_KIND_COUNT],
    top_level_expression_call_blockers: [usize; COMPACT_CALL_BLOCKER_KIND_COUNT],
    top_level_expression_call_callees: BTreeMap<String, usize>,
    top_level_command_kinds: [usize; COMPACT_COMMAND_BLOCKER_KIND_COUNT],
    statements: usize,
    constructed_statements: usize,
    statement_blockers: [usize; COMPACT_STMT_KIND_COUNT],
    statement_blocker_samples: BTreeMap<String, Vec<String>>,
    expressions: usize,
    constructed_expressions: usize,
    expression_blockers: [usize; COMPACT_EXPR_KIND_COUNT],
    call_blockers: [usize; COMPACT_CALL_BLOCKER_KIND_COUNT],
    call_blocker_callees: BTreeMap<String, usize>,
    call_blocker_sample_files: BTreeMap<String, Vec<String>>,
    call_blocker_samples: BTreeMap<String, Vec<String>>,
    patterns: usize,
    constructed_patterns: usize,
    expr_type_facts: usize,
    retained_outputs: usize,
}

impl Default for CompactLowerConstructCounts {
    fn default() -> Self {
        Self {
            functions: 0,
            constructed_functions: 0,
            constructed_auto_main_functions: 0,
            function_blockers: [0; COMPACT_FUNCTION_BLOCKER_KIND_COUNT],
            function_return_type_tags: [0; COMPACT_TYPE_EXPR_TAG_COUNT],
            function_param_type_tags: [0; COMPACT_TYPE_EXPR_TAG_COUNT],
            function_body_tail_stmt_kinds: [0; COMPACT_STMT_KIND_COUNT],
            function_body_tail_command_kinds: [0; COMPACT_COMMAND_BLOCKER_KIND_COUNT],
            function_body_tail_call_callees: BTreeMap::new(),
            function_dependency_edges: 0,
            function_sccs: 0,
            function_qualified_dependency_edges: 0,
            function_unqualified_dependency_edges: 0,
            top_level_statements: 0,
            constructed_top_level_statements: 0,
            top_level_blockers: [0; COMPACT_TOP_LEVEL_BLOCKER_KIND_COUNT],
            top_level_blocker_samples: BTreeMap::new(),
            top_level_blocker_stmt_kinds: [0; COMPACT_STMT_KIND_COUNT],
            top_level_binding_type_annotation_tags: [0; COMPACT_TYPE_EXPR_TAG_COUNT],
            top_level_binding_type_expr_kinds: [0; COMPACT_EXPR_KIND_COUNT],
            top_level_binding_type_call_blockers: [0; COMPACT_CALL_BLOCKER_KIND_COUNT],
            top_level_binding_type_call_callees: BTreeMap::new(),
            top_level_binding_expression_expr_kinds: [0; COMPACT_EXPR_KIND_COUNT],
            top_level_binding_expression_call_blockers: [0; COMPACT_CALL_BLOCKER_KIND_COUNT],
            top_level_binding_expression_call_callees: BTreeMap::new(),
            top_level_expression_expr_kinds: [0; COMPACT_EXPR_KIND_COUNT],
            top_level_expression_call_blockers: [0; COMPACT_CALL_BLOCKER_KIND_COUNT],
            top_level_expression_call_callees: BTreeMap::new(),
            top_level_command_kinds: [0; COMPACT_COMMAND_BLOCKER_KIND_COUNT],
            statements: 0,
            constructed_statements: 0,
            statement_blockers: [0; COMPACT_STMT_KIND_COUNT],
            statement_blocker_samples: BTreeMap::new(),
            expressions: 0,
            constructed_expressions: 0,
            expression_blockers: [0; COMPACT_EXPR_KIND_COUNT],
            call_blockers: [0; COMPACT_CALL_BLOCKER_KIND_COUNT],
            call_blocker_callees: BTreeMap::new(),
            call_blocker_sample_files: BTreeMap::new(),
            call_blocker_samples: BTreeMap::new(),
            patterns: 0,
            constructed_patterns: 0,
            expr_type_facts: 0,
            retained_outputs: 0,
        }
    }
}

#[derive(Default)]
struct CompactRuntimeDeclCounts {
    type_defs: usize,
    tag_arities: usize,
    error_families: usize,
    error_variants: usize,
    error_fields: usize,
    error_facets: usize,
    procs: usize,
    pures: usize,
    streams: usize,
    retained_outputs: usize,
}

#[derive(Default)]
struct CompactHotPathCounts {
    unique_files: usize,
    file_runs: usize,
    compact_hot_path_file_runs: usize,
    compact_blocked_file_runs: usize,
    executable_top_level_statements: usize,
    constructed_top_level_statements: usize,
    unconstructed_top_level_statements: usize,
    parse_diagnostic_file_runs: usize,
    module_file_runs: usize,
    declaration_diagnostic_file_runs: usize,
    body_diagnostic_file_runs: usize,
    no_executable_top_level_file_runs: usize,
    auto_main_file_runs: usize,
    unconstructed_top_level_file_runs: usize,
    unsupported_body_statement_file_runs: usize,
    unsupported_body_expression_file_runs: usize,
    unconstructed_function_file_runs: usize,
    unconstructed_functions: usize,
    compact_blocked_files: BTreeMap<String, usize>,
}

#[derive(Default)]
struct ArenaCounts {
    modules: usize,
    statements: usize,
    blocks: usize,
    expressions: usize,
    patterns: usize,
    binding_targets: usize,
    assign_targets: usize,
    type_exprs: usize,
    use_stmts: usize,
    type_defs: usize,
    error_defs: usize,
    function_defs: usize,
    signal_hooks: usize,
    command_stmts: usize,
    int_literals: usize,
    float_literals: usize,
    duration_literals: usize,
    string_literals: usize,
    bytes_literals: usize,
    text_literals: usize,
    source_text_literals: usize,
    cooked_text_literals: usize,
    run_forms: usize,
    builder_blocks: usize,
    spans: usize,
    span_source_overrides: usize,
    extra_items: usize,
    fmt_parts: usize,
    command_args: usize,
    word_parts: usize,
    list_items: usize,
    span_storage_bytes: usize,
    stmt_storage_bytes: usize,
    expr_storage_bytes: usize,
    type_expr_storage_bytes: usize,
    extra_storage_bytes: usize,
    text_storage_bytes: usize,
    cooked_text_storage_bytes: usize,
    definition_storage_bytes: usize,
    literal_storage_bytes: usize,
    pattern_storage_bytes: usize,
    block_storage_bytes: usize,
    control_storage_bytes: usize,
    call_record_storage_bytes: usize,
    builder_storage_bytes: usize,
    command_storage_bytes: usize,
    side_table_storage_bytes: usize,
    retained_bytes: usize,
}

impl ArenaCounts {
    fn add(&mut self, stats: ArenaStats) {
        self.modules += stats.modules;
        self.statements += stats.statements;
        self.blocks += stats.blocks;
        self.expressions += stats.expressions;
        self.patterns += stats.patterns;
        self.binding_targets += stats.binding_targets;
        self.assign_targets += stats.assign_targets;
        self.type_exprs += stats.type_exprs;
        self.use_stmts += stats.use_stmts;
        self.type_defs += stats.type_defs;
        self.error_defs += stats.error_defs;
        self.function_defs += stats.function_defs;
        self.signal_hooks += stats.signal_hooks;
        self.command_stmts += stats.command_stmts;
        self.int_literals += stats.int_literals;
        self.float_literals += stats.float_literals;
        self.duration_literals += stats.duration_literals;
        self.string_literals += stats.string_literals;
        self.bytes_literals += stats.bytes_literals;
        self.text_literals += stats.text_literals;
        self.source_text_literals += stats.source_text_literals;
        self.cooked_text_literals += stats.cooked_text_literals;
        self.run_forms += stats.run_forms;
        self.builder_blocks += stats.builder_blocks;
        self.spans += stats.spans;
        self.span_source_overrides += stats.span_source_overrides;
        self.extra_items += stats.extra_items;
        self.fmt_parts += stats.fmt_parts;
        self.command_args += stats.command_args;
        self.word_parts += stats.word_parts;
        self.list_items += stats.list_items;
        self.span_storage_bytes += stats.span_storage_bytes;
        self.stmt_storage_bytes += stats.stmt_storage_bytes;
        self.expr_storage_bytes += stats.expr_storage_bytes;
        self.type_expr_storage_bytes += stats.type_expr_storage_bytes;
        self.extra_storage_bytes += stats.extra_storage_bytes;
        self.text_storage_bytes += stats.text_storage_bytes;
        self.cooked_text_storage_bytes += stats.cooked_text_storage_bytes;
        self.definition_storage_bytes += stats.definition_storage_bytes;
        self.literal_storage_bytes += stats.literal_storage_bytes;
        self.pattern_storage_bytes += stats.pattern_storage_bytes;
        self.block_storage_bytes += stats.block_storage_bytes;
        self.control_storage_bytes += stats.control_storage_bytes;
        self.call_record_storage_bytes += stats.call_record_storage_bytes;
        self.builder_storage_bytes += stats.builder_storage_bytes;
        self.command_storage_bytes += stats.command_storage_bytes;
        self.side_table_storage_bytes += stats.side_table_storage_bytes;
        self.retained_bytes += stats.retained_bytes;
    }
}

struct PhaseMetrics {
    elapsed_ns: u128,
    allocations: Option<AllocationSnapshot>,
}

fn print_report(
    root: &Path,
    repeat: usize,
    totals: &Totals,
    file_counts: &[FileCounts],
    phase_file_summaries: &PhaseFileSummaries,
    module_graph_readiness: &ModuleGraphReadiness,
    function_lowering_readiness: &FunctionLoweringReadiness,
    top_level_readiness: &TopLevelReadiness,
    arena_only_counts: &ParseCounts,
    arena_only_metrics: &PhaseMetrics,
    compact_walk_counts: &CompactWalkCounts,
    compact_walk_metrics: &PhaseMetrics,
    compact_decl_counts: &CompactDeclCounts,
    compact_decl_metrics: &PhaseMetrics,
    compact_body_counts: &CompactBodyCounts,
    compact_body_metrics: &PhaseMetrics,
    compact_lower_decl_counts: &CompactLowerDeclCounts,
    compact_lower_decl_metrics: &PhaseMetrics,
    compact_lower_body_counts: &CompactLowerBodyCounts,
    compact_lower_body_metrics: &PhaseMetrics,
    compact_lower_construct_counts: &CompactLowerConstructCounts,
    compact_lower_construct_metrics: &PhaseMetrics,
    compact_runtime_decl_counts: &CompactRuntimeDeclCounts,
    compact_runtime_decl_metrics: &PhaseMetrics,
    compact_hot_path_counts: &CompactHotPathCounts,
) {
    println!("{{");
    println!("  \"kind\": \"parse-corpus-report\",");
    println!(
        "  \"root\": \"{}\",",
        escape_json(&root.display().to_string())
    );
    println!("  \"repeat\": {repeat},");
    println!("  \"files\": {},", totals.files);
    println!("  \"bytes\": {},", totals.bytes);
    print_file_counts(file_counts);
    print_phase_file_summaries(phase_file_summaries);
    print_module_graph_readiness(module_graph_readiness);
    print_function_lowering_readiness(function_lowering_readiness);
    print_top_level_readiness(top_level_readiness);
    print_named_parse_phase("parse_arena_only", arena_only_counts, arena_only_metrics);
    print_compact_walk_phase(compact_walk_counts, compact_walk_metrics);
    print_compact_decl_phase(compact_decl_counts, compact_decl_metrics);
    print_compact_body_phase(compact_body_counts, compact_body_metrics);
    print_compact_lower_decl_phase(compact_lower_decl_counts, compact_lower_decl_metrics);
    print_compact_lower_body_phase(compact_lower_body_counts, compact_lower_body_metrics);
    print_compact_lower_construct_phase(
        compact_lower_construct_counts,
        compact_lower_construct_metrics,
    );
    print_compact_runtime_decl_phase(compact_runtime_decl_counts, compact_runtime_decl_metrics);
    print_compact_hot_path_phase(compact_hot_path_counts);
    println!("}}");
}

fn print_file_counts(file_counts: &[FileCounts]) {
    println!("  \"per_file_counts\": [");
    for (index, counts) in file_counts.iter().enumerate() {
        let suffix = if index + 1 == file_counts.len() {
            ""
        } else {
            ","
        };
        println!("    {{");
        println!("      \"path\": \"{}\",", escape_json(&counts.path));
        println!("      \"bytes\": {},", counts.bytes);
        println!("      \"tokens\": {},", counts.tokens);
        println!("      \"syntax_tokens\": {},", counts.syntax_tokens);
        println!("      \"statements\": {},", counts.statements);
        println!("      \"imports\": {},", counts.imports);
        println!("      \"exports\": {},", counts.exports);
        println!("      \"type_defs\": {},", counts.type_defs);
        println!("      \"error_defs\": {},", counts.error_defs);
        println!("      \"function_defs\": {},", counts.function_defs);
        println!(
            "      \"executable_top_level_statements\": {}",
            counts.executable_top_level_statements
        );
        println!("    }}{suffix}");
    }
    println!("  ],");
}

fn print_phase_file_summaries(summaries: &PhaseFileSummaries) {
    println!("  \"per_phase_file_summaries\": {{");
    print_phase_file_summary("    ", "parse_arena_only", summaries.parse_arena_only, true);
    print_phase_file_summary(
        "    ",
        "declaration_probe",
        summaries.declaration_probe,
        true,
    );
    print_phase_file_summary("    ", "body_probe", summaries.body_probe, true);
    print_phase_file_summary(
        "    ",
        "lowering_declaration_probe",
        summaries.lowering_declaration_probe,
        true,
    );
    print_phase_file_summary(
        "    ",
        "lowering_construct_probe",
        summaries.lowering_construct_probe,
        true,
    );
    print_phase_file_summary(
        "    ",
        "lowering_body_probe",
        summaries.lowering_body_probe,
        true,
    );
    print_phase_file_summary(
        "    ",
        "runtime_declaration_registration",
        summaries.runtime_declaration_registration,
        false,
    );
    println!("  }},");
}

fn print_phase_file_summary(indent: &str, name: &str, summary: PhaseFileSummary, comma: bool) {
    println!("{indent}\"{name}\": {{");
    println!("{indent}  \"total_ns\": {},", summary.total_ns);
    println!("{indent}  \"p50_ns\": {},", summary.p50_ns);
    println!("{indent}  \"p95_ns\": {},", summary.p95_ns);
    println!("{indent}  \"max_ns\": {}", summary.max_ns);
    let suffix = if comma { "," } else { "" };
    println!("{indent}}}{suffix}");
}

fn print_module_graph_readiness(readiness: &ModuleGraphReadiness) {
    println!("  \"module_graph_readiness\": {{");
    println!(
        "    \"import_edge_count\": {},",
        readiness.import_edge_count
    );
    println!(
        "    \"unique_module_count\": {},",
        readiness.unique_module_count
    );
    println!(
        "    \"qualified_declaration_count\": {},",
        readiness.qualified_declaration_count
    );
    println!(
        "    \"duplicate_diagnostic_count\": {},",
        readiness.duplicate_diagnostic_count
    );
    println!(
        "    \"largest_dependency_component\": {}",
        readiness.largest_dependency_component
    );
    println!("  }},");
}

fn print_function_lowering_readiness(readiness: &FunctionLoweringReadiness) {
    println!("  \"function_lowering_readiness\": {{");
    println!(
        "    \"attempted_functions\": {},",
        readiness.attempted_functions
    );
    println!(
        "    \"lowered_functions\": {},",
        readiness.lowered_functions
    );
    println!(
        "    \"blocked_functions\": {},",
        readiness.blocked_functions
    );
    println!(
        "    \"dependency_edge_count\": {},",
        readiness.dependency_edge_count
    );
    println!("    \"scc_count\": {},", readiness.scc_count);
    print_string_count_object(
        "    ",
        "blocker_kind_counts",
        &readiness.blocker_kind_counts,
        true,
    );
    println!(
        "    \"qualified_call_count\": {},",
        readiness.qualified_call_count
    );
    println!(
        "    \"unqualified_call_count\": {}",
        readiness.unqualified_call_count
    );
    println!("  }},");
}

fn print_top_level_readiness(readiness: &TopLevelReadiness) {
    println!("  \"top_level_readiness\": {{");
    println!(
        "    \"attempted_statements\": {},",
        readiness.attempted_statements
    );
    println!(
        "    \"lowered_statements\": {},",
        readiness.lowered_statements
    );
    println!(
        "    \"skipped_statements\": {},",
        readiness.skipped_statements
    );
    println!(
        "    \"blocked_statements\": {},",
        readiness.blocked_statements
    );
    print_string_count_object(
        "    ",
        "fallback_reason_counts",
        &readiness.fallback_reason_counts,
        false,
    );
    println!("  }},");
}

fn print_named_parse_phase(name: &str, counts: &ParseCounts, metrics: &PhaseMetrics) {
    println!("  \"{name}\": {{");
    println!("    \"elapsed_ns\": {},", metrics.elapsed_ns);
    println!("    \"statements\": {},", counts.statements);
    println!("    \"modules\": {},", counts.modules);
    println!("    \"syntax_tokens\": {},", counts.syntax_tokens);
    println!(
        "    \"compact_token_table_tokens\": {},",
        counts.compact_token_table_tokens
    );
    println!("    \"compat_token_inline_bytes\": 0,");
    println!(
        "    \"legacy_compat_token_inline_bytes\": {},",
        counts.compact_token_table_tokens * (size_of::<TokenKind>() + size_of::<Span>())
    );
    println!(
        "    \"syntax_token_inline_bytes\": {},",
        counts.syntax_tokens * size_of::<SyntaxToken>()
    );
    println!(
        "    \"compact_token_row_bytes\": {},",
        counts.compact_token_row_bytes
    );
    println!(
        "    \"compact_token_retained_bytes\": {},",
        counts.compact_token_retained_bytes
    );
    print_arena_counts("    \"compact_arena\"", &counts.arena);
    println!("    \"diagnostics\": {},", counts.diagnostics);
    println!("    \"retained_outputs\": {},", counts.retained_outputs);
    print_allocations("    ", metrics.allocations);
    println!("  }},");
}

fn print_compact_walk_phase(counts: &CompactWalkCounts, metrics: &PhaseMetrics) {
    println!("  \"compact_walk\": {{");
    println!("    \"elapsed_ns\": {},", metrics.elapsed_ns);
    println!("    \"programs\": {},", counts.programs);
    println!("    \"diagnostics\": {},", counts.diagnostics);
    println!("    \"statements\": {},", counts.statements);
    println!("    \"expressions\": {},", counts.expressions);
    println!("    \"patterns\": {},", counts.patterns);
    println!("    \"type_exprs\": {},", counts.type_exprs);
    println!("    \"blocks\": {},", counts.blocks);
    println!("    \"block_params\": {},", counts.block_params);
    println!("    \"params\": {},", counts.params);
    println!("    \"binding_targets\": {},", counts.binding_targets);
    println!("    \"assign_targets\": {},", counts.assign_targets);
    println!("    \"destructure_fields\": {},", counts.destructure_fields);
    println!("    \"function_defs\": {},", counts.function_defs);
    println!("    \"type_defs\": {},", counts.type_defs);
    println!("    \"error_defs\": {},", counts.error_defs);
    println!("    \"definition_fields\": {},", counts.definition_fields);
    println!("    \"command_stmts\": {},", counts.command_stmts);
    println!("    \"command_args\": {},", counts.command_args);
    println!("    \"run_forms\": {},", counts.run_forms);
    println!("    \"builder_blocks\": {},", counts.builder_blocks);
    println!("    \"compat_payloads\": {},", counts.compat_payloads);
    println!(
        "    \"unsupported_statements\": {},",
        counts.unsupported_statements
    );
    print_allocations("    ", metrics.allocations);
    println!("  }},");
}

fn print_compact_decl_phase(counts: &CompactDeclCounts, metrics: &PhaseMetrics) {
    println!("  \"compact_decl\": {{");
    println!("    \"elapsed_ns\": {},", metrics.elapsed_ns);
    println!("    \"diagnostics\": {},", counts.diagnostics);
    println!("    \"type_defs\": {},", counts.type_defs);
    println!("    \"tag_variants\": {},", counts.tag_variants);
    println!("    \"error_families\": {},", counts.error_families);
    println!("    \"error_variants\": {},", counts.error_variants);
    println!("    \"error_fields\": {},", counts.error_fields);
    println!("    \"function_defs\": {},", counts.function_defs);
    println!("    \"params\": {},", counts.params);
    println!("    \"schema_fields\": {},", counts.schema_fields);
    println!(
        "    \"module_contract_entries\": {},",
        counts.module_contract_entries
    );
    println!("    \"type_states\": {},", counts.type_states);
    println!("    \"tag_variant_states\": {},", counts.tag_variant_states);
    println!(
        "    \"error_family_states\": {},",
        counts.error_family_states
    );
    println!("    \"proc_sigs\": {},", counts.proc_sigs);
    println!("    \"pure_sigs\": {},", counts.pure_sigs);
    println!("    \"stream_sigs\": {},", counts.stream_sigs);
    println!(
        "    \"qualified_proc_sigs\": {},",
        counts.qualified_proc_sigs
    );
    println!(
        "    \"qualified_pure_sigs\": {},",
        counts.qualified_pure_sigs
    );
    println!(
        "    \"qualified_stream_sigs\": {},",
        counts.qualified_stream_sigs
    );
    println!("    \"retained_outputs\": {},", counts.retained_outputs);
    print_allocations("    ", metrics.allocations);
    println!("  }},");
}

fn print_compact_body_phase(counts: &CompactBodyCounts, metrics: &PhaseMetrics) {
    println!("  \"compact_body\": {{");
    println!("    \"elapsed_ns\": {},", metrics.elapsed_ns);
    println!("    \"diagnostics\": {},", counts.diagnostics);
    println!("    \"statements\": {},", counts.statements);
    println!(
        "    \"supported_statements\": {},",
        counts.supported_statements
    );
    println!(
        "    \"unsupported_statements\": {},",
        counts.unsupported_statements
    );
    println!("    \"expressions\": {},", counts.expressions);
    println!("    \"typed_expressions\": {},", counts.typed_expressions);
    println!(
        "    \"unsupported_expressions\": {},",
        counts.unsupported_expressions
    );
    println!("    \"bindings\": {},", counts.bindings);
    println!("    \"assignment_targets\": {},", counts.assignment_targets);
    println!("    \"blocks\": {},", counts.blocks);
    println!("    \"functions\": {},", counts.functions);
    println!("    \"commands\": {},", counts.commands);
    println!("    \"runs\": {},", counts.runs);
    println!(
        "    \"unsupported_signal_hooks\": {},",
        counts.unsupported_signal_hooks
    );
    println!(
        "    \"unsupported_with_stmts\": {},",
        counts.unsupported_with_stmts
    );
    println!("    \"unsupported_guards\": {},", counts.unsupported_guards);
    println!(
        "    \"unsupported_guarded_stmts\": {},",
        counts.unsupported_guarded_stmts
    );
    println!(
        "    \"unsupported_item_exprs\": {},",
        counts.unsupported_item_exprs
    );
    println!(
        "    \"unsupported_list_comps\": {},",
        counts.unsupported_list_comps
    );
    println!(
        "    \"unsupported_map_comps\": {},",
        counts.unsupported_map_comps
    );
    println!(
        "    \"unsupported_match_exprs\": {},",
        counts.unsupported_match_exprs
    );
    println!(
        "    \"unsupported_pipeline_exprs\": {},",
        counts.unsupported_pipeline_exprs
    );
    println!(
        "    \"unsupported_structured_pipeline_exprs\": {},",
        counts.unsupported_structured_pipeline_exprs
    );
    println!(
        "    \"unsupported_builder_call_exprs\": {},",
        counts.unsupported_builder_call_exprs
    );
    println!("    \"expr_type_facts\": {},", counts.expr_type_facts);
    println!("    \"retained_outputs\": {},", counts.retained_outputs);
    print_allocations("    ", metrics.allocations);
    println!("  }},");
}

fn print_compact_lower_decl_phase(counts: &CompactLowerDeclCounts, metrics: &PhaseMetrics) {
    println!("  \"compact_lower_decl\": {{");
    println!("    \"elapsed_ns\": {},", metrics.elapsed_ns);
    println!("    \"type_defs\": {},", counts.type_defs);
    println!("    \"lowered_aliases\": {},", counts.lowered_aliases);
    println!("    \"lowered_records\": {},", counts.lowered_records);
    println!("    \"lowered_tag_unions\": {},", counts.lowered_tag_unions);
    println!("    \"tag_variants\": {},", counts.tag_variants);
    println!("    \"tag_arities\": {},", counts.tag_arities);
    println!("    \"retained_outputs\": {},", counts.retained_outputs);
    print_allocations("    ", metrics.allocations);
    println!("  }},");
}

fn print_compact_lower_body_phase(counts: &CompactLowerBodyCounts, metrics: &PhaseMetrics) {
    println!("  \"compact_lower_body\": {{");
    println!("    \"elapsed_ns\": {},", metrics.elapsed_ns);
    println!("    \"functions\": {},", counts.functions);
    println!(
        "    \"lowerable_functions\": {},",
        counts.lowerable_functions
    );
    println!(
        "    \"top_level_statements\": {},",
        counts.top_level_statements
    );
    println!(
        "    \"lowerable_top_level_statements\": {},",
        counts.lowerable_top_level_statements
    );
    println!("    \"statements\": {},", counts.statements);
    println!(
        "    \"lowerable_statements\": {},",
        counts.lowerable_statements
    );
    println!("    \"expressions\": {},", counts.expressions);
    println!(
        "    \"lowerable_expressions\": {},",
        counts.lowerable_expressions
    );
    println!("    \"patterns\": {},", counts.patterns);
    println!("    \"lowerable_patterns\": {},", counts.lowerable_patterns);
    println!(
        "    \"unsupported_statements\": {},",
        counts.unsupported_statements
    );
    println!(
        "    \"unsupported_expressions\": {},",
        counts.unsupported_expressions
    );
    println!(
        "    \"unsupported_patterns\": {},",
        counts.unsupported_patterns
    );
    println!("    \"expr_type_facts\": {},", counts.expr_type_facts);
    println!("    \"retained_outputs\": {},", counts.retained_outputs);
    print_allocations("    ", metrics.allocations);
    println!("  }},");
}

fn print_compact_lower_construct_phase(
    counts: &CompactLowerConstructCounts,
    metrics: &PhaseMetrics,
) {
    const FUNCTION_BLOCKER_LABELS: [&str; COMPACT_FUNCTION_BLOCKER_KIND_COUNT] = [
        "return_type",
        "param_default",
        "param_type",
        "block_params",
        "body",
        "no_return",
    ];
    const TOP_LEVEL_BLOCKER_LABELS: [&str; COMPACT_TOP_LEVEL_BLOCKER_KIND_COUNT] = [
        "use",
        "binding_target",
        "binding_type",
        "binding_expression",
        "assign_target",
        "assign_expression",
        "control",
        "command",
        "expression",
        "defer",
        "other",
    ];
    const TYPE_EXPR_TAG_LABELS: [&str; COMPACT_TYPE_EXPR_TAG_COUNT] = [
        "named",
        "qualified",
        "list",
        "map",
        "stream",
        "module",
        "result",
        "optional",
    ];
    const STMT_KIND_LABELS: [&str; COMPACT_STMT_KIND_COUNT] = [
        "use",
        "export",
        "type_def",
        "error_def",
        "let",
        "var",
        "assign",
        "proc_def",
        "pure_def",
        "stream_def",
        "signal_hook",
        "return",
        "yield",
        "defer",
        "if",
        "while",
        "for",
        "with",
        "loop",
        "guard",
        "guarded_stmt",
        "break",
        "continue",
        "match",
        "command",
        "tail_bare_ident",
        "expr",
    ];
    const EXPR_KIND_LABELS: [&str; COMPACT_EXPR_KIND_COUNT] = [
        "null",
        "bool",
        "int",
        "float",
        "duration",
        "str",
        "path_str",
        "glob_str",
        "fmt_string",
        "path_fmt_string",
        "bytes",
        "ident",
        "item",
        "last_status",
        "list",
        "list_comp",
        "map_comp",
        "record",
        "if",
        "match",
        "unary",
        "binary",
        "call",
        "field",
        "null_safe_field",
        "index",
        "slice",
        "env_get",
        "env_path_list",
        "pipeline",
        "structured_pipeline",
        "run",
        "spawn",
        "wait",
        "builder_call",
        "try",
        "require",
        "loop",
        "retry",
    ];
    const CALL_BLOCKER_LABELS: [&str; COMPACT_CALL_BLOCKER_KIND_COUNT] = [
        "ident",
        "field_ident_base",
        "field_other_base",
        "null_safe_ident_base",
        "null_safe_other_base",
        "dynamic_callee",
    ];
    const COMMAND_BLOCKER_LABELS: [&str; COMPACT_COMMAND_BLOCKER_KIND_COUNT] = [
        "proc",
        "core_print",
        "core_eprint",
        "core_cd",
        "core_env",
        "run",
    ];
    println!("  \"compact_lower_construct\": {{");
    println!("    \"elapsed_ns\": {},", metrics.elapsed_ns);
    println!("    \"functions\": {},", counts.functions);
    println!(
        "    \"constructed_functions\": {},",
        counts.constructed_functions
    );
    println!(
        "    \"constructed_auto_main_functions\": {},",
        counts.constructed_auto_main_functions
    );
    print_fixed_count_object(
        "    ",
        "function_blockers",
        &FUNCTION_BLOCKER_LABELS,
        &counts.function_blockers,
        true,
    );
    print_fixed_count_object(
        "    ",
        "function_return_type_tags",
        &TYPE_EXPR_TAG_LABELS,
        &counts.function_return_type_tags,
        true,
    );
    print_fixed_count_object(
        "    ",
        "function_param_type_tags",
        &TYPE_EXPR_TAG_LABELS,
        &counts.function_param_type_tags,
        true,
    );
    print_fixed_count_object(
        "    ",
        "function_body_tail_stmt_kinds",
        &STMT_KIND_LABELS,
        &counts.function_body_tail_stmt_kinds,
        true,
    );
    print_fixed_count_object(
        "    ",
        "function_body_tail_command_kinds",
        &COMMAND_BLOCKER_LABELS,
        &counts.function_body_tail_command_kinds,
        true,
    );
    print_string_count_object(
        "    ",
        "function_body_tail_call_callees",
        &counts.function_body_tail_call_callees,
        true,
    );
    println!(
        "    \"function_dependency_edges\": {},",
        counts.function_dependency_edges
    );
    println!("    \"function_sccs\": {},", counts.function_sccs);
    println!(
        "    \"function_qualified_dependency_edges\": {},",
        counts.function_qualified_dependency_edges
    );
    println!(
        "    \"function_unqualified_dependency_edges\": {},",
        counts.function_unqualified_dependency_edges
    );
    println!(
        "    \"top_level_statements\": {},",
        counts.top_level_statements
    );
    println!(
        "    \"constructed_top_level_statements\": {},",
        counts.constructed_top_level_statements
    );
    print_fixed_count_object(
        "    ",
        "top_level_blockers",
        &TOP_LEVEL_BLOCKER_LABELS,
        &counts.top_level_blockers,
        true,
    );
    print_string_samples_object(
        "    ",
        "top_level_blocker_samples",
        &counts.top_level_blocker_samples,
        true,
    );
    print_fixed_count_object(
        "    ",
        "top_level_blocker_stmt_kinds",
        &STMT_KIND_LABELS,
        &counts.top_level_blocker_stmt_kinds,
        true,
    );
    print_fixed_count_object(
        "    ",
        "top_level_binding_type_annotation_tags",
        &TYPE_EXPR_TAG_LABELS,
        &counts.top_level_binding_type_annotation_tags,
        true,
    );
    print_fixed_count_object(
        "    ",
        "top_level_binding_type_expr_kinds",
        &EXPR_KIND_LABELS,
        &counts.top_level_binding_type_expr_kinds,
        true,
    );
    print_fixed_count_object(
        "    ",
        "top_level_binding_type_call_blockers",
        &CALL_BLOCKER_LABELS,
        &counts.top_level_binding_type_call_blockers,
        true,
    );
    print_string_count_object(
        "    ",
        "top_level_binding_type_call_callees",
        &counts.top_level_binding_type_call_callees,
        true,
    );
    print_fixed_count_object(
        "    ",
        "top_level_binding_expression_expr_kinds",
        &EXPR_KIND_LABELS,
        &counts.top_level_binding_expression_expr_kinds,
        true,
    );
    print_fixed_count_object(
        "    ",
        "top_level_binding_expression_call_blockers",
        &CALL_BLOCKER_LABELS,
        &counts.top_level_binding_expression_call_blockers,
        true,
    );
    print_string_count_object(
        "    ",
        "top_level_binding_expression_call_callees",
        &counts.top_level_binding_expression_call_callees,
        true,
    );
    print_fixed_count_object(
        "    ",
        "top_level_expression_expr_kinds",
        &EXPR_KIND_LABELS,
        &counts.top_level_expression_expr_kinds,
        true,
    );
    print_fixed_count_object(
        "    ",
        "top_level_expression_call_blockers",
        &CALL_BLOCKER_LABELS,
        &counts.top_level_expression_call_blockers,
        true,
    );
    print_string_count_object(
        "    ",
        "top_level_expression_call_callees",
        &counts.top_level_expression_call_callees,
        true,
    );
    print_fixed_count_object(
        "    ",
        "top_level_command_kinds",
        &COMMAND_BLOCKER_LABELS,
        &counts.top_level_command_kinds,
        true,
    );
    println!("    \"statements\": {},", counts.statements);
    println!(
        "    \"constructed_statements\": {},",
        counts.constructed_statements
    );
    print_fixed_count_object(
        "    ",
        "statement_blockers",
        &STMT_KIND_LABELS,
        &counts.statement_blockers,
        true,
    );
    print_string_samples_object(
        "    ",
        "statement_blocker_samples",
        &counts.statement_blocker_samples,
        true,
    );
    println!("    \"expressions\": {},", counts.expressions);
    println!(
        "    \"constructed_expressions\": {},",
        counts.constructed_expressions
    );
    print_fixed_count_object(
        "    ",
        "expression_blockers",
        &EXPR_KIND_LABELS,
        &counts.expression_blockers,
        true,
    );
    print_fixed_count_object(
        "    ",
        "call_blockers",
        &CALL_BLOCKER_LABELS,
        &counts.call_blockers,
        true,
    );
    print_string_count_object(
        "    ",
        "call_blocker_callees",
        &counts.call_blocker_callees,
        true,
    );
    print_string_samples_object(
        "    ",
        "call_blocker_sample_files",
        &counts.call_blocker_sample_files,
        true,
    );
    print_string_samples_object(
        "    ",
        "call_blocker_samples",
        &counts.call_blocker_samples,
        true,
    );
    println!("    \"patterns\": {},", counts.patterns);
    println!(
        "    \"constructed_patterns\": {},",
        counts.constructed_patterns
    );
    println!("    \"expr_type_facts\": {},", counts.expr_type_facts);
    println!("    \"retained_outputs\": {},", counts.retained_outputs);
    print_allocations("    ", metrics.allocations);
    println!("  }},");
}

fn print_compact_runtime_decl_phase(counts: &CompactRuntimeDeclCounts, metrics: &PhaseMetrics) {
    println!("  \"compact_runtime_decl\": {{");
    println!("    \"elapsed_ns\": {},", metrics.elapsed_ns);
    println!("    \"type_defs\": {},", counts.type_defs);
    println!("    \"tag_arities\": {},", counts.tag_arities);
    println!("    \"error_families\": {},", counts.error_families);
    println!("    \"error_variants\": {},", counts.error_variants);
    println!("    \"error_fields\": {},", counts.error_fields);
    println!("    \"error_facets\": {},", counts.error_facets);
    println!("    \"procs\": {},", counts.procs);
    println!("    \"pures\": {},", counts.pures);
    println!("    \"streams\": {},", counts.streams);
    println!("    \"retained_outputs\": {},", counts.retained_outputs);
    print_allocations("    ", metrics.allocations);
    println!("  }},");
}

fn print_compact_hot_path_phase(counts: &CompactHotPathCounts) {
    println!("  \"compact_hot_path\": {{");
    println!("    \"unique_files\": {},", counts.unique_files);
    println!("    \"file_runs\": {},", counts.file_runs);
    println!(
        "    \"compact_hot_path_file_runs\": {},",
        counts.compact_hot_path_file_runs
    );
    println!(
        "    \"compact_blocked_file_runs\": {},",
        counts.compact_blocked_file_runs
    );
    println!(
        "    \"executable_top_level_statements\": {},",
        counts.executable_top_level_statements
    );
    println!(
        "    \"constructed_top_level_statements\": {},",
        counts.constructed_top_level_statements
    );
    println!(
        "    \"unconstructed_top_level_statements\": {},",
        counts.unconstructed_top_level_statements
    );
    println!(
        "    \"parse_diagnostic_file_runs\": {},",
        counts.parse_diagnostic_file_runs
    );
    println!("    \"module_file_runs\": {},", counts.module_file_runs);
    println!(
        "    \"declaration_diagnostic_file_runs\": {},",
        counts.declaration_diagnostic_file_runs
    );
    println!(
        "    \"body_diagnostic_file_runs\": {},",
        counts.body_diagnostic_file_runs
    );
    println!(
        "    \"no_executable_top_level_file_runs\": {},",
        counts.no_executable_top_level_file_runs
    );
    println!(
        "    \"auto_main_file_runs\": {},",
        counts.auto_main_file_runs
    );
    println!(
        "    \"unconstructed_top_level_file_runs\": {},",
        counts.unconstructed_top_level_file_runs
    );
    println!(
        "    \"unsupported_body_statement_file_runs\": {},",
        counts.unsupported_body_statement_file_runs
    );
    println!(
        "    \"unsupported_body_expression_file_runs\": {},",
        counts.unsupported_body_expression_file_runs
    );
    println!(
        "    \"unconstructed_function_file_runs\": {},",
        counts.unconstructed_function_file_runs
    );
    println!(
        "    \"unconstructed_functions\": {},",
        counts.unconstructed_functions
    );
    print_string_count_object(
        "    ",
        "compact_blocked_files",
        &counts.compact_blocked_files,
        false,
    );
    println!("  }}");
}

fn print_arena_counts(name: &str, counts: &ArenaCounts) {
    println!("{name}: {{");
    println!("      \"modules\": {},", counts.modules);
    println!("      \"statements\": {},", counts.statements);
    println!("      \"blocks\": {},", counts.blocks);
    println!("      \"expressions\": {},", counts.expressions);
    println!("      \"patterns\": {},", counts.patterns);
    println!("      \"binding_targets\": {},", counts.binding_targets);
    println!("      \"assign_targets\": {},", counts.assign_targets);
    println!("      \"type_exprs\": {},", counts.type_exprs);
    println!("      \"use_stmts\": {},", counts.use_stmts);
    println!("      \"type_defs\": {},", counts.type_defs);
    println!("      \"error_defs\": {},", counts.error_defs);
    println!("      \"function_defs\": {},", counts.function_defs);
    println!("      \"signal_hooks\": {},", counts.signal_hooks);
    println!("      \"command_stmts\": {},", counts.command_stmts);
    println!("      \"int_literals\": {},", counts.int_literals);
    println!("      \"float_literals\": {},", counts.float_literals);
    println!("      \"duration_literals\": {},", counts.duration_literals);
    println!("      \"string_literals\": {},", counts.string_literals);
    println!("      \"bytes_literals\": {},", counts.bytes_literals);
    println!("      \"text_literals\": {},", counts.text_literals);
    println!(
        "      \"source_text_literals\": {},",
        counts.source_text_literals
    );
    println!(
        "      \"cooked_text_literals\": {},",
        counts.cooked_text_literals
    );
    println!("      \"run_forms\": {},", counts.run_forms);
    println!("      \"builder_blocks\": {},", counts.builder_blocks);
    println!("      \"spans\": {},", counts.spans);
    println!(
        "      \"span_source_overrides\": {},",
        counts.span_source_overrides
    );
    println!("      \"extra_items\": {},", counts.extra_items);
    println!("      \"fmt_parts\": {},", counts.fmt_parts);
    println!("      \"command_args\": {},", counts.command_args);
    println!("      \"word_parts\": {},", counts.word_parts);
    println!("      \"list_items\": {},", counts.list_items);
    println!(
        "      \"span_storage_bytes\": {},",
        counts.span_storage_bytes
    );
    println!(
        "      \"stmt_storage_bytes\": {},",
        counts.stmt_storage_bytes
    );
    println!(
        "      \"expr_storage_bytes\": {},",
        counts.expr_storage_bytes
    );
    println!(
        "      \"type_expr_storage_bytes\": {},",
        counts.type_expr_storage_bytes
    );
    println!(
        "      \"extra_storage_bytes\": {},",
        counts.extra_storage_bytes
    );
    println!(
        "      \"text_storage_bytes\": {},",
        counts.text_storage_bytes
    );
    println!(
        "      \"cooked_text_storage_bytes\": {},",
        counts.cooked_text_storage_bytes
    );
    println!(
        "      \"definition_storage_bytes\": {},",
        counts.definition_storage_bytes
    );
    println!(
        "      \"literal_storage_bytes\": {},",
        counts.literal_storage_bytes
    );
    println!(
        "      \"pattern_storage_bytes\": {},",
        counts.pattern_storage_bytes
    );
    println!(
        "      \"block_storage_bytes\": {},",
        counts.block_storage_bytes
    );
    println!(
        "      \"control_storage_bytes\": {},",
        counts.control_storage_bytes
    );
    println!(
        "      \"call_record_storage_bytes\": {},",
        counts.call_record_storage_bytes
    );
    println!(
        "      \"builder_storage_bytes\": {},",
        counts.builder_storage_bytes
    );
    println!(
        "      \"command_storage_bytes\": {},",
        counts.command_storage_bytes
    );
    println!(
        "      \"side_table_storage_bytes\": {},",
        counts.side_table_storage_bytes
    );
    println!("      \"retained_bytes\": {}", counts.retained_bytes);
    println!("    }},");
}

fn print_fixed_count_object<const N: usize>(
    indent: &str,
    name: &str,
    labels: &[&str; N],
    counts: &[usize; N],
    comma: bool,
) {
    println!("{indent}\"{name}\": {{");
    for (index, (label, count)) in labels.iter().zip(counts.iter()).enumerate() {
        let suffix = if index + 1 == N { "" } else { "," };
        println!("{indent}  \"{label}\": {count}{suffix}");
    }
    let suffix = if comma { "," } else { "" };
    println!("{indent}}}{suffix}");
}

fn print_string_count_object(
    indent: &str,
    name: &str,
    counts: &BTreeMap<String, usize>,
    comma: bool,
) {
    println!("{indent}\"{name}\": {{");
    let mut entries = counts.iter().collect::<Vec<_>>();
    entries.sort_by(|(left_key, left_count), (right_key, right_count)| {
        right_count
            .cmp(left_count)
            .then_with(|| left_key.cmp(right_key))
    });
    for (index, (label, count)) in entries.iter().enumerate() {
        let suffix = if index + 1 == entries.len() { "" } else { "," };
        println!("{indent}  \"{}\": {count}{suffix}", escape_json(label));
    }
    let suffix = if comma { "," } else { "" };
    println!("{indent}}}{suffix}");
}

fn print_string_samples_object(
    indent: &str,
    name: &str,
    samples: &BTreeMap<String, Vec<String>>,
    comma: bool,
) {
    println!("{indent}\"{name}\": {{");
    for (index, (label, files)) in samples.iter().enumerate() {
        let suffix = if index + 1 == samples.len() { "" } else { "," };
        print!("{indent}  \"{}\": [", escape_json(label));
        for (file_index, file) in files.iter().enumerate() {
            let item_suffix = if file_index + 1 == files.len() {
                ""
            } else {
                ","
            };
            print!("\"{}\"{item_suffix}", escape_json(file));
        }
        println!("]{suffix}");
    }
    let suffix = if comma { "," } else { "" };
    println!("{indent}}}{suffix}");
}

fn print_allocations(indent: &str, allocations: Option<AllocationSnapshot>) {
    if let Some(allocations) = allocations {
        println!(
            "{indent}\"allocation_calls\": {},",
            allocations.allocation_calls
        );
        println!(
            "{indent}\"allocation_bytes\": {},",
            allocations.allocation_bytes
        );
        println!(
            "{indent}\"deallocation_calls\": {},",
            allocations.deallocation_calls
        );
        println!(
            "{indent}\"deallocation_bytes\": {},",
            allocations.deallocation_bytes
        );
        println!(
            "{indent}\"reallocation_calls\": {},",
            allocations.reallocation_calls
        );
        println!(
            "{indent}\"reallocation_bytes\": {},",
            allocations.reallocation_bytes
        );
        println!(
            "{indent}\"alloc_calls_le16\": {},",
            allocations.alloc_calls_le16
        );
        println!(
            "{indent}\"alloc_calls_le64\": {},",
            allocations.alloc_calls_le64
        );
        println!(
            "{indent}\"alloc_calls_le256\": {},",
            allocations.alloc_calls_le256
        );
        println!(
            "{indent}\"alloc_calls_le4096\": {},",
            allocations.alloc_calls_le4096
        );
        println!(
            "{indent}\"alloc_calls_gt4096\": {},",
            allocations.alloc_calls_gt4096
        );
        println!("{indent}\"peak_rss_bytes\": {}", allocations.peak_rss_bytes);
    } else {
        println!("{indent}\"allocation_calls\": null,");
        println!("{indent}\"allocation_bytes\": null,");
        println!("{indent}\"deallocation_calls\": null,");
        println!("{indent}\"deallocation_bytes\": null,");
        println!("{indent}\"reallocation_calls\": null,");
        println!("{indent}\"reallocation_bytes\": null,");
        println!("{indent}\"alloc_calls_le16\": null,");
        println!("{indent}\"alloc_calls_le64\": null,");
        println!("{indent}\"alloc_calls_le256\": null,");
        println!("{indent}\"alloc_calls_le4096\": null,");
        println!("{indent}\"alloc_calls_gt4096\": null,");
        println!("{indent}\"peak_rss_bytes\": null");
    }
}

fn escape_json(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            _ => vec![ch],
        })
        .collect()
}
