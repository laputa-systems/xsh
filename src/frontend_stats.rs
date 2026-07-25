//! Stage-split frontend retained-byte and allocation-peak measurements.
//!
//! Retained totals come from owned structures where the representation exposes
//! them. The current lowered runtime does not yet expose a complete recursive
//! byte walker, so the dedicated stats binary uses allocator live-byte deltas
//! for that stage and the library API reports a labeled shallow estimate when
//! the counting allocator is not installed.

use crate::loader::parse_load_check_text;
use crate::mem_track::{self, AllocTraffic};
use crate::runtime::eval::{Evaluator, probe_compact_lower_constructed_bodies};
use crate::sema::check::{
    CheckOptions, CheckOutput, Checker, CompactDeclOutput, CompactFunctionSig, CompactTypeDefInfo,
};
use crate::sema::types::{ModuleExportType, Type};
use crate::source::{SourceId, Span};
use crate::symbol;
use crate::syntax::cst::SyntaxTree;
use crate::syntax::lexer::Lexer;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::mem::size_of;
use std::path::{Path, PathBuf};

pub const DEFAULT_ROOTS: &[&str] = &[
    "crates/xsh-multicall/benches/scripts",
    "core",
    "examples",
    "showcase",
    "tests/fixtures/syntax",
    "tests/fixtures/sema",
    "tests/fixtures/runtime",
    "tests/fixtures/frontend-campaign",
];

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StageMetrics {
    pub name: &'static str,
    pub retained_bytes: usize,
    pub item_count: usize,
    pub peak_bytes: usize,
    pub alloc_count: usize,
    pub alloc_bytes: usize,
    pub tracking_active: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FileFrontendStats {
    pub path: String,
    pub source_bytes: usize,
    pub stages: Vec<StageMetrics>,
    pub token_count: usize,
    pub token_retained_bytes: usize,
    pub cst_node_count: usize,
    pub cst_retained_bytes: usize,
    pub ast_stmt_count: usize,
    pub ast_expr_count: usize,
    pub ast_pattern_count: usize,
    pub ast_type_count: usize,
    pub ast_extra_items: usize,
    pub ast_retained_bytes: usize,
    pub semantic_type_count: usize,
    pub semantic_retained_bytes: usize,
    pub source_map_retained_bytes: usize,
    pub lowered_function_count: usize,
    pub lowered_constructed_functions: usize,
    pub lowered_statement_count: usize,
    pub lowered_expression_count: usize,
    pub lowered_pattern_count: usize,
    pub lowered_retained_bytes: usize,
    pub lowered_retained_estimated: bool,
    pub lowered_blocker_events: u64,
    pub retained_after_drop_bytes: usize,
    pub dynamic_symbol_count: usize,
    pub dynamic_symbol_bytes: usize,
    pub diagnostics: usize,
    pub components_sum: usize,
    pub reported_total_bytes: usize,
    pub reconcile_delta: i64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CorpusFrontendStats {
    pub roots: Vec<String>,
    pub files: Vec<FileFrontendStats>,
    pub totals: FileFrontendStats,
    pub maxima: FileFrontendStats,
    pub tracking_active: bool,
}

fn stage_from_traffic(
    name: &'static str,
    retained_bytes: usize,
    item_count: usize,
    traffic: AllocTraffic,
) -> StageMetrics {
    StageMetrics {
        name,
        retained_bytes,
        item_count,
        peak_bytes: traffic.peak_bytes,
        alloc_count: traffic.alloc_count,
        alloc_bytes: traffic.alloc_bytes,
        tracking_active: traffic.tracking_active,
    }
}

fn measure_expr_types(types: &BTreeMap<Span, Type>) -> (usize, usize) {
    let bytes = size_of::<BTreeMap<Span, Type>>()
        + types.len() * size_of::<(Span, Type)>()
        + types.values().map(Type::retained_bytes).sum::<usize>();
    (types.len(), bytes)
}

fn measure_check_output(checked: Option<&CheckOutput>) -> (usize, usize) {
    let Some(checked) = checked else {
        return (0, 0);
    };
    let (type_count, expr_type_bytes) = measure_expr_types(&checked.expr_types);
    let annotation_bytes = checked.annotation_facts.capacity()
        * size_of::<crate::sema::check::AnnotationFact>()
        + checked
            .annotation_facts
            .iter()
            .map(|fact| fact.ty.retained_bytes())
            .sum::<usize>();
    let callable_bytes = checked.callable_effects.capacity()
        * size_of::<(String, Option<Vec<crate::syntax::node::Effect>>)>()
        + checked
            .callable_effects
            .iter()
            .map(|(name, effects)| {
                name.capacity()
                    + effects.as_ref().map_or(0, |effects| {
                        effects.capacity() * size_of::<crate::syntax::node::Effect>()
                    })
            })
            .sum::<usize>();
    (
        type_count,
        size_of::<CheckOutput>() + expr_type_bytes + annotation_bytes + callable_bytes,
    )
}

fn type_owned_bytes(ty: &Type) -> usize {
    ty.retained_bytes().saturating_sub(size_of::<Type>())
}

fn measure_function_sig(sig: &CompactFunctionSig) -> (usize, usize) {
    let type_count = sig.params.len() + 1;
    let mut bytes = size_of::<CompactFunctionSig>()
        + sig.params.capacity() * size_of::<crate::sema::types::CallableParamType>()
        + type_owned_bytes(&sig.return_ty);
    bytes += sig
        .params
        .iter()
        .map(|param| type_owned_bytes(&param.ty))
        .sum::<usize>();
    bytes += sig.effects.as_ref().map_or(0, |effects| {
        effects.capacity() * size_of::<crate::syntax::node::Effect>()
    });
    (type_count, bytes)
}

fn measure_compact_declarations(declarations: &CompactDeclOutput) -> (usize, usize) {
    let mut type_count = 0;
    let mut bytes = size_of::<CompactDeclOutput>();

    bytes += declarations.types.capacity()
        * size_of::<(crate::symbol::Name, CompactTypeDefInfo)>();
    for ty in declarations.types.values() {
        match ty {
            CompactTypeDefInfo::Alias(_) | CompactTypeDefInfo::TagUnion => {}
            CompactTypeDefInfo::Record(fields) => {
                type_count += fields.len();
                bytes += size_of::<BTreeMap<crate::symbol::Name, Type>>()
                    + fields.len() * size_of::<(crate::symbol::Name, Type)>()
                    + fields.values().map(type_owned_bytes).sum::<usize>();
            }
            CompactTypeDefInfo::Module(exports) => {
                type_count += exports.len();
                bytes += size_of::<BTreeMap<crate::symbol::Name, ModuleExportType>>()
                    + exports.len()
                        * size_of::<(crate::symbol::Name, ModuleExportType)>()
                    + exports
                        .values()
                        .map(|export| {
                            export
                                .retained_bytes()
                                .saturating_sub(size_of::<ModuleExportType>())
                        })
                        .sum::<usize>();
            }
        }
    }

    bytes += declarations.tag_variants_by_name.capacity()
        * size_of::<(crate::symbol::Name, crate::sema::check::TagVariantInfo)>();
    for variant in declarations.tag_variants_by_name.values() {
        type_count += variant.field_types.len();
        bytes += variant.field_types.capacity() * size_of::<Type>()
            + variant
                .field_types
                .iter()
                .map(type_owned_bytes)
                .sum::<usize>();
    }

    macro_rules! measure_error_families {
        ($families:expr, $key:ty) => {{
            bytes += $families.capacity()
                * size_of::<($key, crate::sema::check::ErrorFamilyInfo)>();
            for family in $families.values() {
                bytes += size_of::<BTreeMap<
                    crate::symbol::Name,
                    crate::sema::check::ErrorVariantInfo,
                >>() + family.variants.len()
                    * size_of::<(
                        crate::symbol::Name,
                        crate::sema::check::ErrorVariantInfo,
                    )>();
                for variant in family.variants.values() {
                    type_count += variant.fields.len();
                    bytes += size_of::<BTreeMap<crate::symbol::Name, Type>>()
                        + variant.fields.len() * size_of::<(crate::symbol::Name, Type)>()
                        + variant.fields.values().map(type_owned_bytes).sum::<usize>()
                        + variant.facets.capacity() * size_of::<crate::symbol::Name>();
                }
            }
        }};
    }
    measure_error_families!(declarations.error_families_by_name, crate::symbol::Name);
    measure_error_families!(
        declarations.qualified_error_families,
        crate::symbol::QualifiedName
    );

    macro_rules! measure_function_map {
        ($functions:expr, $key:ty) => {{
            bytes += $functions.capacity() * size_of::<($key, CompactFunctionSig)>();
            for sig in $functions.values() {
                let (sig_types, sig_bytes) = measure_function_sig(sig);
                type_count += sig_types;
                bytes += sig_bytes.saturating_sub(size_of::<CompactFunctionSig>());
            }
        }};
    }
    measure_function_map!(declarations.procs, crate::symbol::Name);
    measure_function_map!(declarations.pures, crate::symbol::Name);
    measure_function_map!(declarations.streams, crate::symbol::Name);
    measure_function_map!(declarations.qualified_procs, crate::symbol::QualifiedName);
    measure_function_map!(declarations.qualified_pures, crate::symbol::QualifiedName);
    measure_function_map!(declarations.qualified_streams, crate::symbol::QualifiedName);

    (type_count, bytes)
}

fn measure_compact_body_types(
    types: &rustc_hash::FxHashMap<crate::syntax::arena::ExprId, Type>,
) -> (usize, usize) {
    (
        types.len(),
        size_of::<rustc_hash::FxHashMap<crate::syntax::arena::ExprId, Type>>()
            + types.capacity() * size_of::<(crate::syntax::arena::ExprId, Type)>()
            + types.values().map(type_owned_bytes).sum::<usize>(),
    )
}

fn diagnostic_count(checked: &crate::loader::CheckedEntry) -> usize {
    checked.parsed.diagnostics.len()
        + checked
            .checked
            .as_ref()
            .map_or(0, |output| output.diagnostics.len() + output.reveal_types.len())
}

/// Measure one in-memory source through tokens, CST, AST/check, and lowering.
pub fn measure_source(path: &str, source: &str) -> FileFrontendStats {
    let source_id = SourceId::new(0);
    let source_bytes = source.len();
    let mut stages = Vec::with_capacity(5);

    mem_track::begin_stage();
    let lexed = Lexer::new(source_id, source).lex_compact();
    let token_traffic = mem_track::end_stage();
    let token_count = lexed.token_table.len();
    let token_retained_bytes = lexed.token_table.retained_bytes();
    stages.push(stage_from_traffic(
        "tokens",
        token_retained_bytes,
        token_count,
        token_traffic,
    ));

    mem_track::begin_stage();
    let cst = SyntaxTree::from_token_table(source_id, source, lexed.token_table.clone());
    let cst_traffic = mem_track::end_stage();
    let cst_node_count = cst.node_count();
    let cst_retained_bytes = cst.retained_bytes_without_token_table();
    stages.push(stage_from_traffic(
        "cst",
        cst_retained_bytes,
        cst_node_count,
        cst_traffic,
    ));

    mem_track::begin_stage();
    let checked = parse_load_check_text(
        path,
        source.to_string(),
        Vec::new(),
        CheckOptions::default(),
    );
    let _ = checked.parsed.cst.get();
    let ast_traffic = mem_track::end_stage();
    let ast = checked.parsed.arena.stats();
    let ast_retained_bytes = ast.retained_bytes;
    let (checked_type_count, checked_retained_bytes) =
        measure_check_output(checked.checked.as_ref());
    let declarations = Checker::check_compact_declarations(&checked.parsed.arena);
    let bodies = Checker::probe_compact_bodies(&checked.parsed.arena, &declarations);
    let (declaration_type_count, declaration_retained_bytes) =
        measure_compact_declarations(&declarations);
    let (body_type_count, body_retained_bytes) = measure_compact_body_types(&bodies.expr_types);
    let semantic_type_count = checked_type_count + declaration_type_count + body_type_count;
    let semantic_retained_bytes =
        checked_retained_bytes + declaration_retained_bytes + body_retained_bytes;
    let source_map_retained_bytes = checked.sources.retained_bytes();
    stages.push(stage_from_traffic(
        "ast_check",
        ast_retained_bytes + semantic_retained_bytes,
        ast.statements + ast.expressions + ast.patterns + ast.type_exprs,
        ast_traffic,
    ));

    let lower_live_before = mem_track::snapshot().live_bytes;
    mem_track::begin_stage();
    let construct_probe = probe_compact_lower_constructed_bodies(
        &checked.parsed.arena,
        &declarations,
        &bodies,
        source,
    );
    let mut evaluator = Evaluator::new_with_sources(Vec::new(), checked.sources.clone());
    let lower_diagnostics = evaluator
        .install_compact_lowered_program(&checked.parsed.arena, checked.entry_source_id)
        .len();
    let lowered = evaluator.frontend_lowered_stats();
    let lower_traffic = mem_track::end_stage();
    let lowered_function_count = construct_probe.functions;
    let lowered_constructed_functions = construct_probe.constructed_functions;
    let lowered_statement_count = construct_probe.statements;
    let lowered_expression_count = construct_probe.expressions;
    let lowered_pattern_count = construct_probe.patterns;
    let lowered_blocker_events = construct_probe.blocker_events;
    let tracked_lowered_retained = lower_traffic.live_bytes.saturating_sub(lower_live_before);
    let lowered_retained_estimated = !lower_traffic.tracking_active;
    let lowered_retained_bytes = if lower_traffic.tracking_active {
        tracked_lowered_retained
    } else {
        lowered.retained_estimate_bytes
    };
    stages.push(stage_from_traffic(
        "lower",
        lowered_retained_bytes,
        lowered_statement_count + lowered_expression_count + lowered_pattern_count,
        lower_traffic,
    ));

    let diagnostics = lexed.diagnostics.len() + diagnostic_count(&checked) + lower_diagnostics;
    let ast_stmt_count = ast.statements;
    let ast_expr_count = ast.expressions;
    let ast_pattern_count = ast.patterns;
    let ast_type_count = ast.type_exprs;
    let ast_extra_items = ast.extra_items;

    mem_track::begin_stage();
    drop(construct_probe);
    drop(bodies);
    drop(declarations);
    drop(cst);
    drop(lexed);
    drop(checked);
    let after_drop_traffic = mem_track::end_stage();
    let retained_after_drop_bytes = if after_drop_traffic.tracking_active {
        after_drop_traffic.live_bytes
    } else {
        lowered_retained_bytes + source_map_retained_bytes
    };
    stages.push(stage_from_traffic(
        "after_drop",
        retained_after_drop_bytes,
        lowered_constructed_functions,
        after_drop_traffic,
    ));

    let (dynamic_symbol_count, dynamic_symbol_bytes) = symbol::dynamic_symbol_stats();
    mem_track::begin_stage();
    drop(evaluator);
    let _ = mem_track::end_stage();
    let components_sum = token_retained_bytes
        + cst_retained_bytes
        + ast_retained_bytes
        + semantic_retained_bytes
        + source_map_retained_bytes
        + lowered_retained_bytes
        + dynamic_symbol_bytes;
    let reported_total_bytes = components_sum;

    FileFrontendStats {
        path: path.to_string(),
        source_bytes,
        stages,
        token_count,
        token_retained_bytes,
        cst_node_count,
        cst_retained_bytes,
        ast_stmt_count,
        ast_expr_count,
        ast_pattern_count,
        ast_type_count,
        ast_extra_items,
        ast_retained_bytes,
        semantic_type_count,
        semantic_retained_bytes,
        source_map_retained_bytes,
        lowered_function_count,
        lowered_constructed_functions,
        lowered_statement_count,
        lowered_expression_count,
        lowered_pattern_count,
        lowered_retained_bytes,
        lowered_retained_estimated,
        lowered_blocker_events,
        retained_after_drop_bytes,
        dynamic_symbol_count,
        dynamic_symbol_bytes,
        diagnostics,
        components_sum,
        reported_total_bytes,
        reconcile_delta: reported_total_bytes as i64 - components_sum as i64,
    }
}

pub fn measure_path(path: &Path) -> io::Result<FileFrontendStats> {
    let source = fs::read_to_string(path)?;
    Ok(measure_source(&path.to_string_lossy(), &source))
}

pub fn measure_roots(roots: &[PathBuf]) -> io::Result<CorpusFrontendStats> {
    let mut paths = Vec::new();
    for root in roots {
        collect_xsh_paths(root, &mut paths)?;
    }
    paths.sort();
    paths.dedup();

    let files = paths
        .iter()
        .map(|path| measure_path(path))
        .collect::<io::Result<Vec<_>>>()?;
    let totals = aggregate_totals(&files);
    let maxima = aggregate_maxima(&files);
    Ok(CorpusFrontendStats {
        roots: roots
            .iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect(),
        tracking_active: mem_track::tracking_installed(),
        files,
        totals,
        maxima,
    })
}

fn collect_xsh_paths(path: &Path, paths: &mut Vec<PathBuf>) -> io::Result<()> {
    if path.is_file() {
        if path.extension().is_some_and(|extension| extension == "xsh") {
            paths.push(path.to_path_buf());
        }
        return Ok(());
    }
    if !path.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        collect_xsh_paths(&entry.path(), paths)?;
    }
    Ok(())
}

fn aggregate_totals(files: &[FileFrontendStats]) -> FileFrontendStats {
    let mut total = FileFrontendStats {
        path: "<totals>".to_string(),
        lowered_retained_estimated: files
            .iter()
            .any(|file| file.lowered_retained_estimated),
        ..FileFrontendStats::default()
    };
    for file in files {
        add_file_stats(&mut total, file);
    }
    total.components_sum = total.token_retained_bytes
        + total.cst_retained_bytes
        + total.ast_retained_bytes
        + total.semantic_retained_bytes
        + total.source_map_retained_bytes
        + total.lowered_retained_bytes
        + total.dynamic_symbol_bytes;
    total.reported_total_bytes = total.components_sum;
    total.reconcile_delta = 0;
    total.stages = aggregate_stages(files, false);
    total
}

fn aggregate_maxima(files: &[FileFrontendStats]) -> FileFrontendStats {
    let mut maximum = FileFrontendStats {
        path: "<maxima>".to_string(),
        lowered_retained_estimated: files
            .iter()
            .any(|file| file.lowered_retained_estimated),
        ..FileFrontendStats::default()
    };
    for file in files {
        max_file_stats(&mut maximum, file);
    }
    maximum.stages = aggregate_stages(files, true);
    maximum
}

fn aggregate_stages(files: &[FileFrontendStats], maxima: bool) -> Vec<StageMetrics> {
    let mut stages = Vec::<StageMetrics>::new();
    for file in files {
        for stage in &file.stages {
            let target = if let Some(target) = stages.iter_mut().find(|item| item.name == stage.name)
            {
                target
            } else {
                stages.push(StageMetrics {
                    name: stage.name,
                    tracking_active: stage.tracking_active,
                    ..StageMetrics::default()
                });
                stages.last_mut().expect("stage was just pushed")
            };
            target.tracking_active |= stage.tracking_active;
            if maxima {
                target.retained_bytes = target.retained_bytes.max(stage.retained_bytes);
                target.item_count = target.item_count.max(stage.item_count);
                target.peak_bytes = target.peak_bytes.max(stage.peak_bytes);
                target.alloc_count = target.alloc_count.max(stage.alloc_count);
                target.alloc_bytes = target.alloc_bytes.max(stage.alloc_bytes);
            } else if stage.name == "after_drop" {
                target.retained_bytes = target.retained_bytes.max(stage.retained_bytes);
                target.item_count = target.item_count.max(stage.item_count);
                target.peak_bytes = target.peak_bytes.max(stage.peak_bytes);
                target.alloc_count += stage.alloc_count;
                target.alloc_bytes += stage.alloc_bytes;
            } else {
                target.retained_bytes += stage.retained_bytes;
                target.item_count += stage.item_count;
                target.peak_bytes = target.peak_bytes.max(stage.peak_bytes);
                target.alloc_count += stage.alloc_count;
                target.alloc_bytes += stage.alloc_bytes;
            }
        }
    }
    stages
}

macro_rules! numeric_fields {
    ($macro:ident) => {
        $macro!(source_bytes);
        $macro!(token_count);
        $macro!(token_retained_bytes);
        $macro!(cst_node_count);
        $macro!(cst_retained_bytes);
        $macro!(ast_stmt_count);
        $macro!(ast_expr_count);
        $macro!(ast_pattern_count);
        $macro!(ast_type_count);
        $macro!(ast_extra_items);
        $macro!(ast_retained_bytes);
        $macro!(semantic_type_count);
        $macro!(semantic_retained_bytes);
        $macro!(source_map_retained_bytes);
        $macro!(lowered_function_count);
        $macro!(lowered_constructed_functions);
        $macro!(lowered_statement_count);
        $macro!(lowered_expression_count);
        $macro!(lowered_pattern_count);
        $macro!(lowered_retained_bytes);
        $macro!(diagnostics);
    };
}

fn add_file_stats(total: &mut FileFrontendStats, file: &FileFrontendStats) {
    macro_rules! add {
        ($field:ident) => {
            total.$field = total.$field.saturating_add(file.$field)
        };
    }
    numeric_fields!(add);
    total.retained_after_drop_bytes = total
        .retained_after_drop_bytes
        .max(file.retained_after_drop_bytes);
    total.dynamic_symbol_count = total.dynamic_symbol_count.max(file.dynamic_symbol_count);
    total.dynamic_symbol_bytes = total.dynamic_symbol_bytes.max(file.dynamic_symbol_bytes);
    total.lowered_blocker_events = total
        .lowered_blocker_events
        .saturating_add(file.lowered_blocker_events);
}

fn max_file_stats(maximum: &mut FileFrontendStats, file: &FileFrontendStats) {
    macro_rules! maximum_field {
        ($field:ident) => {
            maximum.$field = maximum.$field.max(file.$field)
        };
    }
    numeric_fields!(maximum_field);
    maximum.retained_after_drop_bytes = maximum
        .retained_after_drop_bytes
        .max(file.retained_after_drop_bytes);
    maximum.dynamic_symbol_count = maximum.dynamic_symbol_count.max(file.dynamic_symbol_count);
    maximum.dynamic_symbol_bytes = maximum.dynamic_symbol_bytes.max(file.dynamic_symbol_bytes);
    maximum.components_sum = maximum.components_sum.max(file.components_sum);
    maximum.reported_total_bytes = maximum
        .reported_total_bytes
        .max(file.reported_total_bytes);
    maximum.lowered_blocker_events = maximum
        .lowered_blocker_events
        .max(file.lowered_blocker_events);
    maximum.reconcile_delta = maximum.reconcile_delta.max(file.reconcile_delta);
}

impl CorpusFrontendStats {
    pub fn to_text(&self) -> String {
        let mut output = String::new();
        output.push_str("path\tsource\ttokens\ttoken_bytes\tcst_nodes\tcst_bytes\tast_bytes\tsemantic_bytes\tlowered_bytes\tafter_drop\tblockers\tdiagnostics\treconcile_delta\n");
        for file in &self.files {
            output.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                file.path,
                file.source_bytes,
                file.token_count,
                file.token_retained_bytes,
                file.cst_node_count,
                file.cst_retained_bytes,
                file.ast_retained_bytes,
                file.semantic_retained_bytes,
                file.lowered_retained_bytes,
                file.retained_after_drop_bytes,
                file.lowered_blocker_events,
                file.diagnostics,
                file.reconcile_delta,
            ));
        }
        output.push_str("\nstage\tretained\titems\tpeak\talloc_count\talloc_bytes\n");
        for stage in &self.totals.stages {
            output.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\n",
                stage.name,
                stage.retained_bytes,
                stage.item_count,
                stage.peak_bytes,
                stage.alloc_count,
                stage.alloc_bytes,
            ));
        }
        output
    }

    pub fn to_json(&self) -> String {
        let roots = self
            .roots
            .iter()
            .map(|root| json_string(root))
            .collect::<Vec<_>>()
            .join(",");
        let files = self
            .files
            .iter()
            .map(file_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"tracking_active\":{},\"roots\":[{}],\"files\":[{}],\"totals\":{},\"maxima\":{}}}\n",
            self.tracking_active,
            roots,
            files,
            file_json(&self.totals),
            file_json(&self.maxima),
        )
    }
}

fn file_json(file: &FileFrontendStats) -> String {
    let stages = file
        .stages
        .iter()
        .map(|stage| {
            format!(
                "{{\"name\":{},\"retained_bytes\":{},\"item_count\":{},\"peak_bytes\":{},\"alloc_count\":{},\"alloc_bytes\":{},\"tracking_active\":{}}}",
                json_string(stage.name),
                stage.retained_bytes,
                stage.item_count,
                stage.peak_bytes,
                stage.alloc_count,
                stage.alloc_bytes,
                stage.tracking_active,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "{{\"path\":{},\"source_bytes\":{},\"stages\":[{}],",
            "\"token_count\":{},\"token_retained_bytes\":{},",
            "\"cst_node_count\":{},\"cst_retained_bytes\":{},",
            "\"ast_stmt_count\":{},\"ast_expr_count\":{},\"ast_pattern_count\":{},",
            "\"ast_type_count\":{},\"ast_extra_items\":{},\"ast_retained_bytes\":{},",
            "\"semantic_type_count\":{},\"semantic_retained_bytes\":{},",
            "\"source_map_retained_bytes\":{},\"lowered_function_count\":{},",
            "\"lowered_constructed_functions\":{},\"lowered_statement_count\":{},",
            "\"lowered_expression_count\":{},\"lowered_pattern_count\":{},",
            "\"lowered_retained_bytes\":{},\"lowered_retained_estimated\":{},",
            "\"lowered_blocker_events\":{},\"retained_after_drop_bytes\":{},",
            "\"dynamic_symbol_count\":{},\"dynamic_symbol_bytes\":{},",
            "\"diagnostics\":{},\"components_sum\":{},\"reported_total_bytes\":{},",
            "\"reconcile_delta\":{}}}"
        ),
        json_string(&file.path),
        file.source_bytes,
        stages,
        file.token_count,
        file.token_retained_bytes,
        file.cst_node_count,
        file.cst_retained_bytes,
        file.ast_stmt_count,
        file.ast_expr_count,
        file.ast_pattern_count,
        file.ast_type_count,
        file.ast_extra_items,
        file.ast_retained_bytes,
        file.semantic_type_count,
        file.semantic_retained_bytes,
        file.source_map_retained_bytes,
        file.lowered_function_count,
        file.lowered_constructed_functions,
        file.lowered_statement_count,
        file.lowered_expression_count,
        file.lowered_pattern_count,
        file.lowered_retained_bytes,
        file.lowered_retained_estimated,
        file.lowered_blocker_events,
        file.retained_after_drop_bytes,
        file.dynamic_symbol_count,
        file.dynamic_symbol_bytes,
        file.diagnostics,
        file.components_sum,
        file.reported_total_bytes,
        file.reconcile_delta,
    )
}

fn json_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character < ' ' => {
                output.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

#[cfg(test)]
mod tests {
    use super::{measure_source, CorpusFrontendStats, FileFrontendStats};

    const SOURCE: &str = "pure twice(value: Int) -> Int { return value * 2 }\nprint ${twice(3)}\n";
    const VERTICAL_SLICE: &str =
        include_str!("../tests/fixtures/frontend-campaign/vertical-slice.xsh");

    #[test]
    fn source_measurements_are_deterministic_without_allocator_tracking() {
        let first = measure_source("fixture.xsh", SOURCE);
        let second = measure_source("fixture.xsh", SOURCE);

        assert_eq!(first, second);
        assert_eq!(first.reconcile_delta, 0);
        assert_eq!(first.components_sum, first.reported_total_bytes);
    }

    #[test]
    fn json_output_is_stable() {
        let file = measure_source("fixture.xsh", SOURCE);
        let corpus = CorpusFrontendStats {
            files: vec![file.clone()],
            totals: file.clone(),
            maxima: file,
            ..CorpusFrontendStats::default()
        };

        assert_eq!(corpus.to_json(), corpus.to_json());
    }

    #[test]
    fn default_file_stats_reconcile() {
        let stats = FileFrontendStats::default();
        assert_eq!(stats.reconcile_delta, 0);
    }

    #[test]
    fn vertical_slice_retained_columns_are_stable() {
        let first = measure_source("vertical-slice.xsh", VERTICAL_SLICE);
        let second = measure_source("vertical-slice.xsh", VERTICAL_SLICE);

        assert_eq!(first.token_retained_bytes, second.token_retained_bytes);
        assert_eq!(first.cst_retained_bytes, second.cst_retained_bytes);
        assert_eq!(first.ast_retained_bytes, second.ast_retained_bytes);
        assert_eq!(first.semantic_retained_bytes, second.semantic_retained_bytes);
        assert_eq!(first.lowered_retained_bytes, second.lowered_retained_bytes);
        assert_eq!(first.reconcile_delta, 0);
    }
}
