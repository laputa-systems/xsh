//! Profiling-only allocation-request retention for actual frontend output owners.
//! Facts are destroyed while their parsed arena, source map, and symbol owner
//! remain alive. No recursive type estimate or assumed collection layout enters
//! the byte totals. Each invocation measures one fresh process and one thread.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::mem::size_of;
use std::sync::Arc;
use xsh::diagnostic::{Diagnostic, Severity};
use xsh::execution::script::{RunOptions, prepare_benchmark_script};
use xsh::frontend::check::{CheckOptions, CheckOutput, Checker, CompactBodyProbeOutput, CompactDeclOutput, Type};
use xsh::frontend::load::{entry_source_from_bytes, parse_load_check_file, parse_load_entry_source_arena_only};
use xsh::frontend::symbols::Name;
use xsh::mem_track::{self, AllocTraffic, CountingAllocator};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator::new();

#[derive(Clone, Copy)]
struct Snapshot {
    name: &'static str,
    traffic: AllocTraffic,
}

fn capture(name: &'static str) -> Snapshot {
    Snapshot { name, traffic: mem_track::snapshot() }
}

// Snapshot storage is inline and cannot contaminate a measured drop interval.
struct Snapshots {
    values: [Snapshot; 12],
    len: usize,
}

impl Snapshots {
    fn new() -> Self {
        Self { values: [Snapshot { name: "", traffic: AllocTraffic::default() }; 12], len: 0 }
    }

    fn take(&mut self, name: &'static str) {
        self.values[self.len] = capture(name);
        self.len += 1;
    }

    fn get(&self, name: &str) -> Option<AllocTraffic> {
        self.values[..self.len].iter().find(|value| value.name == name).map(|value| value.traffic)
    }
}

fn exact_drop(snapshots: &Snapshots, before: &str, after: &str) -> Option<usize> {
    let before = snapshots.get(before)?;
    let after = snapshots.get(after)?;
    if before.alloc_count != after.alloc_count || before.alloc_bytes != after.alloc_bytes {
        return None;
    }
    before.live_bytes.checked_sub(after.live_bytes)
}

fn diagnostics(diagnostics: &[Diagnostic], hash: &mut impl Hasher) -> usize {
    let mut errors = 0;
    for diagnostic in diagnostics {
        diagnostic.message.hash(hash);
        diagnostic.code.hash(hash);
        diagnostic.span.hash(hash);
        diagnostic.severity.as_str().hash(hash);
        for label in &diagnostic.labels {
            label.span.hash(hash);
            label.message.hash(hash);
        }
        errors += usize::from(diagnostic.severity == Severity::Error);
    }
    errors
}

fn number(value: Option<usize>) -> String {
    value.map_or_else(|| "null".to_string(), |value| value.to_string())
}

fn emit_snapshots(snapshots: &Snapshots) {
    print!("\"snapshots\":[");
    for (index, snapshot) in snapshots.values[..snapshots.len].iter().enumerate() {
        if index > 0 { print!(","); }
        let traffic = snapshot.traffic;
        print!("{{\"name\":\"{}\",\"live_bytes\":{},\"peak_bytes\":{},\"alloc_count\":{},\"alloc_bytes\":{}}}", snapshot.name, traffic.live_bytes, traffic.peak_bytes, traffic.alloc_count, traffic.alloc_bytes);
    }
    print!("]");
}

fn profile_facts(mode: &str, file: &str) {
    let mut snapshots = Snapshots::new();
    mem_track::begin_stage();
    snapshots.take("start");
    let mut entry = parse_load_check_file(file, Vec::new(), CheckOptions::default()).expect("read profiling entry");
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    let parse_errors = diagnostics(&entry.parsed.diagnostics, &mut hash);
    let full_errors = entry.checked.as_ref().map(|checked| diagnostics(&checked.diagnostics, &mut hash));
    let diagnostic_fingerprint = hash.finish();
    let source_files = entry.sources.files().len();
    let source_bytes: usize = entry.sources.files().iter().map(|source| source.len()).sum();
    let full_expr_types = entry.checked.as_ref().map(|checked| checked.expr_types.len());
    let full_present = entry.checked.is_some();
    let symbols = entry.parsed.arena.symbol_owner().clone();
    snapshots.take("full_finalized");
    if mode == "compact" {
        drop(entry.checked.take());
        snapshots.take("full_discarded_before_compact");
    }
    let mut declarations = None;
    let mut bodies = None;
    let mut compact_errors = None;
    let mut compact_expr_types = None;
    if mode != "full" && entry.parsed.diagnostics.is_empty() {
        declarations = Some(Checker::check_compact_declarations(&entry.parsed.arena));
        bodies = Some(Checker::probe_compact_bodies(&entry.parsed.arena, declarations.as_ref().unwrap()));
        let mut compact_hash = std::collections::hash_map::DefaultHasher::new();
        compact_errors = Some(diagnostics(&declarations.as_ref().unwrap().diagnostics, &mut compact_hash)
            + diagnostics(&bodies.as_ref().unwrap().diagnostics, &mut compact_hash));
        compact_expr_types = Some(bodies.as_ref().unwrap().expr_types.len());
    }
    let full_materialized = full_present && mode != "compact";
    let compact_materialized = bodies.is_some();
    snapshots.take("facts_finalized");
    if mode == "combined-compact-first" {
        drop(bodies.take());
        snapshots.take("body_dropped");
        drop(declarations.take());
        snapshots.take("compact_dropped");
        drop(entry.checked.take());
        snapshots.take("all_facts_dropped");
    } else {
        drop(entry.checked.take());
        snapshots.take("full_dropped");
        drop(bodies.take());
        snapshots.take("body_dropped");
        drop(declarations.take());
        snapshots.take("all_facts_dropped");
    }
    let fact_bytes = if full_materialized || compact_materialized {
        exact_drop(&snapshots, "facts_finalized", "all_facts_dropped")
    } else { None };
    let full_marginal = if !full_materialized { None } else if mode == "combined-compact-first" {
        exact_drop(&snapshots, "compact_dropped", "all_facts_dropped")
    } else { exact_drop(&snapshots, "facts_finalized", "full_dropped") };
    let compact_marginal = if !compact_materialized { None } else if mode == "combined-compact-first" {
        exact_drop(&snapshots, "facts_finalized", "compact_dropped")
    } else { exact_drop(&snapshots, "full_dropped", "all_facts_dropped") };
    let xsh::frontend::load::CheckedEntry { sources, parsed, .. } = entry;
    drop(parsed);
    snapshots.take("parsed_dropped");
    drop(sources);
    snapshots.take("sources_dropped");
    drop(symbols);
    snapshots.take("symbol_owner_dropped");
    let final_traffic = mem_track::end_stage();

    // Independent owner teardown deltas can include allocator traffic in symbol
    // release. Keep those signed net deltas visible instead of calling them
    // exact heap ownership, or assuming the global interner releases capacity.
    let final_live = snapshots.get("facts_finalized").unwrap().live_bytes as i128;
    let facts_drop_net = final_live - snapshots.get("all_facts_dropped").unwrap().live_bytes as i128;
    let parsed_drop_net = snapshots.get("all_facts_dropped").unwrap().live_bytes as i128 - snapshots.get("parsed_dropped").unwrap().live_bytes as i128;
    let source_drop_net = snapshots.get("parsed_dropped").unwrap().live_bytes as i128 - snapshots.get("sources_dropped").unwrap().live_bytes as i128;
    let symbol_drop_net = snapshots.get("sources_dropped").unwrap().live_bytes as i128 - final_traffic.live_bytes as i128;
    let reconciled = final_live == facts_drop_net + parsed_drop_net + source_drop_net + symbol_drop_net + final_traffic.live_bytes as i128;
    println!("{{\"schema_version\":1,\"collector\":\"allocation-request-owner-drop\",\"mode\":\"{mode}\",\"tracking_active\":{},\"parse_errors\":{parse_errors},\"full_check_errors\":{},\"full_output_present\":{full_present},\"diagnostic_fingerprint\":\"{diagnostic_fingerprint:016x}\",\"compact_errors\":{},\"source_files\":{source_files},\"source_bytes\":{source_bytes},\"full_expr_types\":{},\"compact_expr_types\":{},\"fact_union_heap_bytes\":{},\"full_drop_heap_bytes\":{},\"compact_drop_heap_bytes\":{},\"fact_drop_exact\":{},\"facts_stack_storage_bytes\":{},\"prepared_after_frontend_drop_heap_bytes\":null,\"prepared_pool_bytes\":null,\"solver_work_counters\":null,\"reconciliation\":{{\"facts_finalized_live_bytes\":{final_live},\"facts_drop_net_bytes\":{facts_drop_net},\"parsed_drop_net_bytes\":{parsed_drop_net},\"source_drop_net_bytes\":{source_drop_net},\"symbol_drop_net_bytes\":{symbol_drop_net},\"final_external_residual_live_bytes\":{},\"reconciled\":{reconciled}}},", final_traffic.tracking_active, number(full_errors), number(compact_errors), number(full_expr_types), number(compact_expr_types), number(fact_bytes), number(full_marginal), number(compact_marginal), fact_bytes.is_some(), usize::from(full_present && mode != "compact") * size_of::<CheckOutput>() + usize::from(compact_expr_types.is_some()) * (size_of::<CompactDeclOutput>() + size_of::<CompactBodyProbeOutput>()), final_traffic.live_bytes);
    emit_snapshots(&snapshots);
    println!("}}");
}

fn profile_prepared(mode: &str, file: &str, argv: &[String]) {
    let mut snapshots = Snapshots::new();
    mem_track::begin_stage();
    snapshots.take("start");
    // Retaining a second parser's symbol owner avoids releasing names shared
    // with that parse during prepared-owner teardown. Preparation-only names
    // can still grow the interner free list, in which case exact bytes remain
    // unavailable. Construction includes an extra parse, so this context mode
    // cannot establish ordinary startup pressure or timing.
    let symbol_context = if mode == "prepared-symbol-context" {
        let (sources, parsed) = parse_load_entry_source_arena_only(file,
            entry_source_from_bytes(file, std::fs::read(file).expect("read symbol context")), Vec::new());
        let owner = parsed.arena.symbol_owner().clone();
        drop(parsed);
        drop(sources);
        Some(owner)
    } else { None };
    snapshots.take("before_preparation");
    let prepared = prepare_benchmark_script(RunOptions { script: file.to_string(), args: argv.to_vec(), coverage_trace_dir: None });
    let status = prepared.as_ref().err().map(|output| output.status);
    let success = prepared.is_ok();
    snapshots.take("prepared_after_frontend_drop");
    drop(prepared);
    snapshots.take("prepared_dropped");
    drop(symbol_context);
    snapshots.take("symbol_context_dropped");
    let traffic = mem_track::end_stage();
    let retained = if success { exact_drop(&snapshots, "prepared_after_frontend_drop", "prepared_dropped") } else { None };
    let before_drop = snapshots.get("prepared_after_frontend_drop").unwrap();
    let after_drop = snapshots.get("prepared_dropped").unwrap();
    let net_drop = before_drop.live_bytes as i128 - after_drop.live_bytes as i128;
    let cleanup_alloc_bytes = after_drop.alloc_bytes - before_drop.alloc_bytes;
    let cleanup_alloc_count = after_drop.alloc_count - before_drop.alloc_count;
    print!("{{\"schema_version\":1,\"collector\":\"allocation-request-owner-drop\",\"mode\":\"{mode}\",\"tracking_active\":{},\"preparation_succeeded\":{success},\"preparation_exit_status\":{},\"prepared_after_frontend_drop_heap_bytes\":{},\"prepared_drop_net_heap_bytes\":{net_drop},\"prepared_drop_alloc_bytes\":{cleanup_alloc_bytes},\"prepared_drop_alloc_count\":{cleanup_alloc_count},\"prepared_pool_bytes\":null,\"prepared_typing_only_heap_bytes\":null,\"solver_work_counters\":null,\"final_external_residual_live_bytes\":{},", traffic.tracking_active, number(status.map(usize::from)), number(retained), traffic.live_bytes);
    emit_snapshots(&snapshots);
    println!("}}");
}

fn self_test() {
    mem_track::begin_stage();
    let start = capture("start").traffic;
    let boxed = std::hint::black_box(Box::new(Type::Int));
    let live_box = capture("box").traffic;
    assert_eq!(live_box.live_bytes - start.live_bytes, size_of::<Type>());
    drop(boxed);
    let after_box = capture("after_box").traffic;
    assert_eq!(after_box.live_bytes, start.live_bytes);
    let _ = mem_track::end_stage();

    // Preloaded names avoid introducing interner ownership into this known graph.
    let field = Name::intern("name");
    mem_track::begin_stage();
    let before_record = capture("before_record").traffic;
    let record = std::hint::black_box(Type::Record(BTreeMap::from([(field, Type::List(Box::new(Type::Int)))])));
    let record_live = capture("record").traffic;
    let alias = std::hint::black_box(record.clone());
    let alias_live = capture("alias").traffic;
    assert_eq!(record_live.live_bytes - before_record.live_bytes, alias_live.live_bytes - record_live.live_bytes);
    drop(record);
    let first_record_drop = capture("first_record_drop").traffic;
    assert_eq!(alias_live.live_bytes - first_record_drop.live_bytes, record_live.live_bytes - before_record.live_bytes);
    assert_eq!(alias, Type::Record(BTreeMap::from([(field, Type::List(Box::new(Type::Int)))])));
    drop(alias);
    assert_eq!(capture("all_records_dropped").traffic.live_bytes, before_record.live_bytes);
    let _ = mem_track::end_stage();

    mem_track::begin_stage();
    let before_shared = capture("before_shared").traffic;
    let shared = std::hint::black_box(Arc::new([0_u8; 257]));
    let shared_live = capture("shared").traffic;
    let alias = std::hint::black_box(Arc::clone(&shared));
    assert_eq!(capture("shared_alias").traffic.live_bytes, shared_live.live_bytes);
    drop(shared);
    assert_eq!(capture("first_shared_drop").traffic.live_bytes, shared_live.live_bytes);
    drop(alias);
    assert_eq!(capture("last_shared_drop").traffic.live_bytes, before_shared.live_bytes);
    let _ = mem_track::end_stage();
    println!("{{\"status\":\"passed\",\"tests\":[\"boxed_type_single_heap_allocation\",\"record_alias_owns_independent_backing\",\"shared_graph_last_owner_drop\"]}}");
}

fn main() {
    CountingAllocator::install_marker();
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 2 && args[1] == "--self-test" { self_test(); return; }
    if args.len() < 3 { panic!("usage: retained-facts MODE ENTRY [ARGS...]; MODE = full|compact|combined-full-first|combined-compact-first|prepared|prepared-symbol-context"); }
    match args[1].as_str() {
        "full" | "compact" | "combined-full-first" | "combined-compact-first" => {
            assert_eq!(args.len(), 3, "fact modes do not consume runtime argv");
            profile_facts(&args[1], &args[2]);
        }
        "prepared" | "prepared-symbol-context" => profile_prepared(&args[1], &args[2], &args[3..]),
        _ => panic!("unknown retention mode"),
    }
}
