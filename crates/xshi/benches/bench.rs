#![allow(clippy::single_call_fn)]

#[cfg(not(feature = "perf-metrics"))]
use std::alloc::{GlobalAlloc, Layout, System};
use std::fs;
use std::hint::black_box;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
#[cfg(not(feature = "perf-metrics"))]
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use xsh::runtime::eval::Evaluator;
use xsh::runtime::value::{RecordMap, RecordShape, Value};
use xsh::sema::check::Checker;
use xsh::source::{SourceId, SourceMap};
use xsh::syntax::parser::ArenaParseOutput;
use xsh::syntax::parser::Parser;
use xshi::interactive::bench::{
    BenchLine, BenchSession, HistorySearchRenderBench, RenderBench, completion_grid,
    synthetic_history_45k,
};

static BENCH_RECORD_KEYS: LazyLock<[Arc<str>; 10]> = LazyLock::new(|| {
    [
        Arc::from("accessed"),
        Arc::from("ext"),
        Arc::from("gid"),
        Arc::from("kind"),
        Arc::from("mode"),
        Arc::from("modified"),
        Arc::from("name"),
        Arc::from("path"),
        Arc::from("size"),
        Arc::from("uid"),
    ]
});

static BENCH_RECORD_SHAPE: LazyLock<RecordShape> =
    LazyLock::new(|| RecordShape::new(BENCH_RECORD_KEYS.iter().cloned().collect()));

const SMALL_FRONTEND_MAX_LINES: usize = 200;
const SMALL_FRONTEND_MAX_BYTES: usize = 16 * 1024;

#[cfg(not(feature = "perf-metrics"))]
struct CountingAlloc;

#[cfg(not(feature = "perf-metrics"))]
static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
#[cfg(not(feature = "perf-metrics"))]
static ALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);
#[cfg(not(feature = "perf-metrics"))]
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
#[cfg(not(feature = "perf-metrics"))]
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);

#[cfg(not(feature = "perf-metrics"))]
fn update_peak() {
    let live = LIVE_BYTES.load(Relaxed);
    let mut peak = PEAK_BYTES.load(Relaxed);
    while live > peak {
        match PEAK_BYTES.compare_exchange_weak(peak, live, Relaxed, Relaxed) {
            Ok(_) => break,
            Err(actual) => peak = actual,
        }
    }
}

#[cfg(not(feature = "perf-metrics"))]
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Relaxed);
        ALLOC_BYTES.fetch_add(layout.size(), Relaxed);
        LIVE_BYTES.fetch_add(layout.size(), Relaxed);
        update_peak();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size(), Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Relaxed);
        if new_size > layout.size() {
            let delta = new_size - layout.size();
            ALLOC_BYTES.fetch_add(delta, Relaxed);
            LIVE_BYTES.fetch_add(delta, Relaxed);
        } else {
            let delta = layout.size() - new_size;
            LIVE_BYTES.fetch_sub(delta, Relaxed);
        }
        update_peak();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[cfg(not(feature = "perf-metrics"))]
#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

#[cfg(not(feature = "perf-metrics"))]
fn reset_alloc_counters() {
    ALLOC_COUNT.store(0, Relaxed);
    ALLOC_BYTES.store(0, Relaxed);
    PEAK_BYTES.store(LIVE_BYTES.load(Relaxed), Relaxed);
}

struct AllocStats {
    count: usize,
    bytes: usize,
}

impl AllocStats {
    fn per_file(&self, files: usize) -> String {
        if files == 0 {
            return "n/a".to_string();
        }
        format!(
            "{:.1} allocs, {:.1} KiB",
            self.count as f64 / files as f64,
            self.bytes as f64 / files as f64 / 1024.0
        )
    }
}

impl std::fmt::Display for AllocStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn human(n: usize) -> String {
            if n >= 1_048_576 {
                format!("{:.1} MiB", n as f64 / 1_048_576.0)
            } else if n >= 1024 {
                format!("{:.1} KiB", n as f64 / 1024.0)
            } else {
                format!("{n} B")
            }
        }
        write!(f, "{} allocs, {}", self.count, human(self.bytes))
    }
}

#[cfg(not(feature = "perf-metrics"))]
fn measure_allocs<F: FnOnce()>(f: F) -> AllocStats {
    reset_alloc_counters();
    f();
    AllocStats {
        count: ALLOC_COUNT.load(Relaxed),
        bytes: ALLOC_BYTES.load(Relaxed),
    }
}

#[cfg(feature = "perf-metrics")]
fn measure_allocs<F: FnOnce()>(f: F) -> AllocStats {
    f();
    AllocStats { count: 0, bytes: 0 }
}

fn bench_history(c: &mut Criterion) {
    let mut group = c.benchmark_group("history");
    group.measurement_time(Duration::from_secs(5));

    let history = synthetic_history_45k();
    let session = BenchSession::with_history(history);
    let autosuggest_line = BenchLine::new("git");

    group.bench_function("prefix_search_45k", |b| {
        b.iter(|| black_box(session.prefix_search("git commit")));
    });

    group.bench_function("prefix_search_miss_45k", |b| {
        b.iter(|| black_box(session.prefix_search("zzzznotfound")));
    });

    group.bench_function("autosuggestion_45k", |b| {
        b.iter(|| black_box(session.autosuggestion(&autosuggest_line)));
    });

    group.bench_function("fuzzy_search_45k", |b| {
        b.iter(|| black_box(session.fuzzy_search("gco")));
    });

    group.bench_function("fuzzy_search_miss_45k", |b| {
        b.iter(|| black_box(session.fuzzy_search("zzzznotfound")));
    });

    group.finish();
}

fn bench_history_load(c: &mut Criterion) {
    let mut group = c.benchmark_group("history_load");
    let history = synthetic_history_45k();
    let text = history
        .iter()
        .enumerate()
        .map(|(idx, command)| format!("1774022576{idx:04} {command}"))
        .collect::<Vec<_>>()
        .join("\n");

    group.bench_function("parse_compact_45k", |b| {
        b.iter(|| black_box(xshi::interactive::bench::parse_history(&text)));
    });

    group.finish();
}

fn bench_completion(c: &mut Criterion) {
    let mut group = c.benchmark_group("completion");
    let session = BenchSession::with_history(Vec::new());

    group.bench_function("compute_grid_100_entries", |b| {
        b.iter(|| black_box(completion_grid(100, 120)));
    });

    group.bench_function("compute_grid_100_narrow", |b| {
        b.iter(|| black_box(completion_grid(100, 40)));
    });

    group.bench_function("start_completion_repo_prefix", |b| {
        b.iter(|| black_box(session.complete_len("ls Cargo.", "ls Cargo.".len(), 80)));
    });

    group.bench_function("start_completion_src_prefix", |b| {
        b.iter(|| black_box(session.complete_len("ls src/l", "ls src/l".len(), 80)));
    });

    group.finish();
}

fn create_dir_fixture(entries: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create bench dir");
    for index in 0..entries {
        let subdir = dir.path().join(format!("dir-{index:04}"));
        fs::create_dir(&subdir).expect("create bench subdir");
        let file = dir.path().join(format!("file-{index:04}.txt"));
        fs::write(&file, format!("bench file {index}\n")).expect("write bench file");
        if index % 10 == 0 {
            let exe = dir.path().join(format!("tool-{index:04}"));
            fs::write(&exe, "#!/bin/sh\nexit 0\n").expect("write bench executable");
            let mut permissions = fs::metadata(&exe)
                .expect("stat bench executable")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&exe, permissions).expect("chmod bench executable");
        }
    }
    dir
}

fn bench_cwd_snapshot(c: &mut Criterion) {
    let mut listing = c.benchmark_group("listing");
    let dir_100 = create_dir_fixture(100);
    let dir_1000 = create_dir_fixture(1000);

    {
        let mut session = BenchSession::with_history(Vec::new());
        session.set_cwd(dir_100.path());
        listing.bench_function("cwd_no_args_after_cd_100", |b| {
            b.iter(|| black_box(session.list_len(&[])));
        });
    }

    {
        let mut session = BenchSession::with_history(Vec::new());
        session.set_cwd(dir_1000.path());
        listing.bench_function("cwd_no_args_after_cd_1000", |b| {
            b.iter(|| black_box(session.list_len(&[])));
        });
    }

    {
        let mut session = BenchSession::with_history(Vec::new());
        session.set_cwd(dir_1000.path());
        let dot = vec![".".to_string()];
        listing.bench_function("explicit_dot_uncached_1000", |b| {
            b.iter(|| black_box(session.list_len(&dot)));
        });
    }

    listing.finish();

    let mut completion = c.benchmark_group("completion_after_cd");
    {
        let mut session = BenchSession::with_history(Vec::new());
        session.set_cwd(dir_100.path());
        completion.bench_function("first_word_empty_after_cd_100", |b| {
            b.iter(|| black_box(session.complete_len("", 0, 80)));
        });
    }

    {
        let mut session = BenchSession::with_history(Vec::new());
        session.set_cwd(dir_1000.path());
        completion.bench_function("first_word_prefix_after_cd_1000", |b| {
            b.iter(|| black_box(session.complete_len("d", 1, 80)));
        });
    }

    {
        let mut session = BenchSession::with_history(Vec::new());
        session.set_cwd(dir_1000.path());
        completion.bench_function("path_arg_ls_current_dir_after_cd_1000", |b| {
            b.iter(|| black_box(session.complete_len("ls d", 4, 80)));
        });
    }

    {
        let mut session = BenchSession::with_history(Vec::new());
        session.set_cwd(dir_1000.path());
        completion.bench_function("path_arg_cd_current_dir_after_cd_1000", |b| {
            b.iter(|| black_box(session.complete_len("cd d", 4, 80)));
        });
    }
    completion.finish();

    let mut session_group = c.benchmark_group("session");
    {
        let mut session = BenchSession::with_history(Vec::new());
        session_group.bench_function("set_cwd_refresh_1000", |b| {
            b.iter(|| {
                session.set_cwd(dir_1000.path());
                black_box(());
            });
        });
    }
    session_group.finish();

    let mut workflow = c.benchmark_group("workflow");
    {
        let mut session = BenchSession::with_history(Vec::new());
        workflow.bench_function("cd_then_l_then_tab_1000", |b| {
            b.iter(|| black_box(session.workflow_cd_l_completion_len(dir_1000.path())));
        });
    }
    workflow.finish();
}

fn bench_render(c: &mut Criterion) {
    let mut group = c.benchmark_group("render");

    group.bench_function("prompt_rerender_single_line", |b| {
        let mut bench = RenderBench::new("gh api repos/openai/openai/issues", 80);
        b.iter(|| black_box(bench.render_prompt("$ ", 80)));
    });

    group.bench_function("prompt_rerender_wrapped", |b| {
        let mut bench = RenderBench::new("gh api repos/openai/openai/issues/123/comments", 20);
        b.iter(|| black_box(bench.render_prompt("$ ", 20)));
    });

    group.bench_function("completion_repaint_navigation", |b| {
        let mut bench = RenderBench::new("ls a", 30);
        b.iter(|| black_box(bench.render_completion_nav("$ ", 30)));
    });

    group.bench_function("history_search_pager_navigation", |b| {
        let mut bench = HistorySearchRenderBench::new("gco", 24, 80);
        b.iter(|| black_box(bench.render_navigation()));
    });

    group.bench_function("history_search_pager_wrapped_query", |b| {
        let mut bench =
            HistorySearchRenderBench::new("cargo test package xsh semantic regression", 24, 20);
        b.iter(|| black_box(bench.render_wrapped_query()));
    });

    group.finish();
}

fn bench_prompt_listing(c: &mut Criterion) {
    let mut group = c.benchmark_group("prompt_listing");
    let session = BenchSession::with_history(Vec::new());

    group.bench_function("prompt_render", |b| {
        b.iter(|| black_box(session.prompt_len()));
    });

    group.bench_function("listing_repo_root", |b| {
        b.iter(|| black_box(session.list_len(&[])));
    });

    group.bench_function("listing_src_dir", |b| {
        let args = vec!["src".to_string()];
        b.iter(|| black_box(session.list_len(&args)));
    });

    group.finish();
}

fn bench_record_map(c: &mut Criterion) {
    fn values(index: i64) -> Vec<Value> {
        vec![
            Value::Int(1_700_000_000 + index),
            Value::Str(Arc::from("rs")),
            Value::Int(20),
            Value::Str(Arc::from("file")),
            Value::Int(0o644),
            Value::Int(1_700_000_100 + index),
            Value::Str(Arc::from("main.rs")),
            Value::Str(Arc::from("src/main.rs")),
            Value::Int(4096),
            Value::Int(501),
        ]
    }

    let mut group = c.benchmark_group("record_map");

    group.bench_function("dynamic_fixed_10_construct", |b| {
        b.iter(|| {
            let values = values(black_box(1));
            black_box(RecordMap::from([
                (BENCH_RECORD_KEYS[0].clone(), values[0].clone()),
                (BENCH_RECORD_KEYS[1].clone(), values[1].clone()),
                (BENCH_RECORD_KEYS[2].clone(), values[2].clone()),
                (BENCH_RECORD_KEYS[3].clone(), values[3].clone()),
                (BENCH_RECORD_KEYS[4].clone(), values[4].clone()),
                (BENCH_RECORD_KEYS[5].clone(), values[5].clone()),
                (BENCH_RECORD_KEYS[6].clone(), values[6].clone()),
                (BENCH_RECORD_KEYS[7].clone(), values[7].clone()),
                (BENCH_RECORD_KEYS[8].clone(), values[8].clone()),
                (BENCH_RECORD_KEYS[9].clone(), values[9].clone()),
            ]))
        });
    });

    group.bench_function("shaped_fixed_10_construct", |b| {
        b.iter(|| black_box(RecordMap::shaped(&BENCH_RECORD_SHAPE, values(black_box(1)))));
    });

    let dynamic = RecordMap::from([
        (BENCH_RECORD_KEYS[0].clone(), Value::Int(1)),
        (BENCH_RECORD_KEYS[1].clone(), Value::Str(Arc::from("rs"))),
        (BENCH_RECORD_KEYS[2].clone(), Value::Int(20)),
        (BENCH_RECORD_KEYS[3].clone(), Value::Str(Arc::from("file"))),
        (BENCH_RECORD_KEYS[4].clone(), Value::Int(0o644)),
        (BENCH_RECORD_KEYS[5].clone(), Value::Int(2)),
        (
            BENCH_RECORD_KEYS[6].clone(),
            Value::Str(Arc::from("main.rs")),
        ),
        (
            BENCH_RECORD_KEYS[7].clone(),
            Value::Str(Arc::from("src/main.rs")),
        ),
        (BENCH_RECORD_KEYS[8].clone(), Value::Int(4096)),
        (BENCH_RECORD_KEYS[9].clone(), Value::Int(501)),
    ]);
    let shaped = RecordMap::shaped(&BENCH_RECORD_SHAPE, values(1));

    group.bench_function("dynamic_fixed_10_lookup", |b| {
        b.iter(|| black_box(dynamic.get(black_box("path"))));
    });

    group.bench_function("shaped_fixed_10_lookup", |b| {
        b.iter(|| black_box(shaped.get(black_box("path"))));
    });

    group.finish();
}

fn bench_json(c: &mut Criterion) {
    let mut group = c.benchmark_group("json");
    let text = xsh::runtime::bench::json_fixture_text(1000);
    let value = xsh::runtime::bench::json_fixture_value(1000);

    group.bench_function("decode_1000_records", |b| {
        b.iter(|| black_box(xsh::runtime::bench::parse_json_fixture(black_box(&text))));
    });

    group.bench_function("encode_1000_records", |b| {
        b.iter(|| black_box(xsh::runtime::bench::encode_json_fixture(black_box(&value))));
    });

    group.finish();
}

fn bench_interpreter(c: &mut Criterion) {
    let mut group = c.benchmark_group("interpreter");
    group.measurement_time(Duration::from_secs(5));

    let fib_20 = xsh::runtime::bench::recursive_fib_source(20);
    let prepared_fib_20 = xsh::runtime::bench::prepare_source("bench-fib-20.xsh", &fib_20);
    let loop_10k = xsh::runtime::bench::loop_sum_source(10_000);
    let prepared_loop_10k = xsh::runtime::bench::prepare_source("bench-loop-10k.xsh", &loop_10k);
    let methods_5k = xsh::runtime::bench::method_dispatch_source(5_000);
    let prepared_methods_5k =
        xsh::runtime::bench::prepare_source("bench-methods-5k.xsh", &methods_5k);
    let record_map_2k = xsh::runtime::bench::record_map_source(2_000);
    let prepared_record_map_2k =
        xsh::runtime::bench::prepare_source("bench-record-map-2k.xsh", &record_map_2k);
    let results_5k = xsh::runtime::bench::result_propagation_source(5_000);
    let prepared_results_5k =
        xsh::runtime::bench::prepare_source("bench-results-5k.xsh", &results_5k);
    let pure_loop_20k = xsh::runtime::bench::pure_loop_source(20_000);
    let prepared_pure_loop_20k =
        xsh::runtime::bench::prepare_source("bench-pure-loop-20k.xsh", &pure_loop_20k);
    let pure_call_chain_20k = xsh::runtime::bench::pure_call_chain_source(20_000);
    let prepared_pure_call_chain_20k =
        xsh::runtime::bench::prepare_source("bench-pure-call-chain-20k.xsh", &pure_call_chain_20k);
    let pure_result_validate_10k = xsh::runtime::bench::pure_result_validate_source(10_000);
    let prepared_pure_result_validate_10k = xsh::runtime::bench::prepare_source(
        "bench-pure-result-validate-10k.xsh",
        &pure_result_validate_10k,
    );
    let stream_2k = xsh::runtime::bench::stream_pipeline_source(2_000);
    let prepared_stream_2k = xsh::runtime::bench::prepare_source("bench-stream-2k.xsh", &stream_2k);
    let stream_callback_pure_5k = xsh::runtime::bench::stream_callback_pure_source(5_000);
    let prepared_stream_callback_pure_5k = xsh::runtime::bench::prepare_source(
        "bench-stream-callback-pure-5k.xsh",
        &stream_callback_pure_5k,
    );
    let text_glue_5k = xsh::runtime::bench::text_glue_source(5_000);
    let prepared_text_glue_5k =
        xsh::runtime::bench::prepare_source("bench-text-glue-5k.xsh", &text_glue_5k);
    let record_ir_glue_5k = xsh::runtime::bench::record_ir_glue_source(5_000);
    let prepared_record_ir_glue_5k =
        xsh::runtime::bench::prepare_source("bench-record-ir-glue-5k.xsh", &record_ir_glue_5k);
    let collection_ir_glue_5k = xsh::runtime::bench::collection_ir_glue_source(5_000);
    let prepared_collection_ir_glue_5k = xsh::runtime::bench::prepare_source(
        "bench-collection-ir-glue-5k.xsh",
        &collection_ir_glue_5k,
    );
    let mixed_2k = xsh::runtime::bench::mixed_glue_source(2_000);
    let prepared_mixed_2k = xsh::runtime::bench::prepare_source("bench-mixed-2k.xsh", &mixed_2k);
    let json_record_glue_1k = xsh::runtime::bench::json_record_glue_source(1_000);
    let prepared_json_record_glue_1k =
        xsh::runtime::bench::prepare_source("bench-json-record-glue-1k.xsh", &json_record_glue_1k);

    group.bench_function("parse_check_recursive_fib_20", |b| {
        b.iter(|| {
            xsh::runtime::bench::parse_check_source("bench-fib-20.xsh", black_box(fib_20.as_str()));
        });
    });

    group.bench_function("eval_recursive_fib_20", |b| {
        b.iter(|| {
            let status = xsh::runtime::bench::eval_prepared(black_box(&prepared_fib_20));
            assert_eq!(status, 109);
        });
    });

    group.bench_function("parse_check_loop_sum_10k", |b| {
        b.iter(|| {
            xsh::runtime::bench::parse_check_source(
                "bench-loop-10k.xsh",
                black_box(loop_10k.as_str()),
            );
        });
    });

    group.bench_function("eval_loop_sum_10k", |b| {
        b.iter(|| {
            let status = xsh::runtime::bench::eval_prepared(black_box(&prepared_loop_10k));
            assert_eq!(status, 248);
        });
    });

    group.bench_function("parse_check_method_dispatch_5k", |b| {
        b.iter(|| {
            xsh::runtime::bench::parse_check_source(
                "bench-methods-5k.xsh",
                black_box(methods_5k.as_str()),
            );
        });
    });

    group.bench_function("eval_method_dispatch_5k", |b| {
        b.iter(|| {
            let status = xsh::runtime::bench::eval_prepared(black_box(&prepared_methods_5k));
            black_box(status);
        });
    });

    group.bench_function("parse_check_record_map_2k", |b| {
        b.iter(|| {
            xsh::runtime::bench::parse_check_source(
                "bench-record-map-2k.xsh",
                black_box(record_map_2k.as_str()),
            );
        });
    });

    group.bench_function("eval_record_map_2k", |b| {
        b.iter(|| {
            let status = xsh::runtime::bench::eval_prepared(black_box(&prepared_record_map_2k));
            black_box(status);
        });
    });

    group.bench_function("parse_check_result_propagation_5k", |b| {
        b.iter(|| {
            xsh::runtime::bench::parse_check_source(
                "bench-results-5k.xsh",
                black_box(results_5k.as_str()),
            );
        });
    });

    group.bench_function("eval_result_propagation_5k", |b| {
        b.iter(|| {
            let status = xsh::runtime::bench::eval_prepared(black_box(&prepared_results_5k));
            black_box(status);
        });
    });

    group.bench_function("parse_check_pure_loop_20k", |b| {
        b.iter(|| {
            xsh::runtime::bench::parse_check_source(
                "bench-pure-loop-20k.xsh",
                black_box(pure_loop_20k.as_str()),
            );
        });
    });

    group.bench_function("eval_pure_loop_20k", |b| {
        b.iter(|| {
            let status = xsh::runtime::bench::eval_prepared(black_box(&prepared_pure_loop_20k));
            black_box(status);
        });
    });

    group.bench_function("parse_check_pure_call_chain_20k", |b| {
        b.iter(|| {
            xsh::runtime::bench::parse_check_source(
                "bench-pure-call-chain-20k.xsh",
                black_box(pure_call_chain_20k.as_str()),
            );
        });
    });

    group.bench_function("eval_pure_call_chain_20k", |b| {
        b.iter(|| {
            let status =
                xsh::runtime::bench::eval_prepared(black_box(&prepared_pure_call_chain_20k));
            black_box(status);
        });
    });

    group.bench_function("parse_check_pure_result_validate_10k", |b| {
        b.iter(|| {
            xsh::runtime::bench::parse_check_source(
                "bench-pure-result-validate-10k.xsh",
                black_box(pure_result_validate_10k.as_str()),
            );
        });
    });

    group.bench_function("eval_pure_result_validate_10k", |b| {
        b.iter(|| {
            let status =
                xsh::runtime::bench::eval_prepared(black_box(&prepared_pure_result_validate_10k));
            black_box(status);
        });
    });

    group.bench_function("parse_check_stream_pipeline_2k", |b| {
        b.iter(|| {
            xsh::runtime::bench::parse_check_source(
                "bench-stream-2k.xsh",
                black_box(stream_2k.as_str()),
            );
        });
    });

    group.bench_function("eval_stream_pipeline_2k", |b| {
        b.iter(|| {
            let status = xsh::runtime::bench::eval_prepared(black_box(&prepared_stream_2k));
            black_box(status);
        });
    });

    group.bench_function("parse_check_stream_callback_pure_5k", |b| {
        b.iter(|| {
            xsh::runtime::bench::parse_check_source(
                "bench-stream-callback-pure-5k.xsh",
                black_box(stream_callback_pure_5k.as_str()),
            );
        });
    });

    group.bench_function("eval_stream_callback_pure_5k", |b| {
        b.iter(|| {
            let status =
                xsh::runtime::bench::eval_prepared(black_box(&prepared_stream_callback_pure_5k));
            black_box(status);
        });
    });

    group.bench_function("parse_check_text_glue_5k", |b| {
        b.iter(|| {
            xsh::runtime::bench::parse_check_source(
                "bench-text-glue-5k.xsh",
                black_box(text_glue_5k.as_str()),
            );
        });
    });

    group.bench_function("eval_text_glue_5k", |b| {
        b.iter(|| {
            let status = xsh::runtime::bench::eval_prepared(black_box(&prepared_text_glue_5k));
            black_box(status);
        });
    });

    group.bench_function("parse_check_record_ir_glue_5k", |b| {
        b.iter(|| {
            xsh::runtime::bench::parse_check_source(
                "bench-record-ir-glue-5k.xsh",
                black_box(record_ir_glue_5k.as_str()),
            );
        });
    });

    group.bench_function("eval_record_ir_glue_5k", |b| {
        b.iter(|| {
            let status = xsh::runtime::bench::eval_prepared(black_box(&prepared_record_ir_glue_5k));
            black_box(status);
        });
    });

    group.bench_function("parse_check_collection_ir_glue_5k", |b| {
        b.iter(|| {
            xsh::runtime::bench::parse_check_source(
                "bench-collection-ir-glue-5k.xsh",
                black_box(collection_ir_glue_5k.as_str()),
            );
        });
    });

    group.bench_function("eval_collection_ir_glue_5k", |b| {
        b.iter(|| {
            let status =
                xsh::runtime::bench::eval_prepared(black_box(&prepared_collection_ir_glue_5k));
            black_box(status);
        });
    });

    group.bench_function("parse_check_mixed_glue_2k", |b| {
        b.iter(|| {
            xsh::runtime::bench::parse_check_source(
                "bench-mixed-2k.xsh",
                black_box(mixed_2k.as_str()),
            );
        });
    });

    group.bench_function("eval_mixed_glue_2k", |b| {
        b.iter(|| {
            let status = xsh::runtime::bench::eval_prepared(black_box(&prepared_mixed_2k));
            black_box(status);
        });
    });

    group.bench_function("parse_check_json_record_glue_1k", |b| {
        b.iter(|| {
            xsh::runtime::bench::parse_check_source(
                "bench-json-record-glue-1k.xsh",
                black_box(json_record_glue_1k.as_str()),
            );
        });
    });

    group.bench_function("eval_json_record_glue_1k", |b| {
        b.iter(|| {
            let status =
                xsh::runtime::bench::eval_prepared(black_box(&prepared_json_record_glue_1k));
            black_box(status);
        });
    });

    group.finish();
}

fn frontend_import_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create frontend bench dir");
    fs::write(
        dir.path().join("helper.xsh"),
        r#"
export pure invalid_option(option: Str) -> Str {
  return f"invalid option -- ${option}"
}

export pure missing_option_value(option: Str) -> Str {
  return f"missing value for option -- ${option}"
}

export pure score_label(option: Str, index: Int) -> Str {
  if index % 3 == 0 {
    return invalid_option(option)
  }

  return missing_option_value(option)
}
"#,
    )
    .expect("write helper module");
    fs::write(
        dir.path().join("main.xsh"),
        r#"
use helper

pure score(option: Str, index: Int) -> Int {
  let message = helper.score_label(option, index).lower()
  let words = message.split(" ")
  return message.count_chars() + words.len() + words.get(words.len() - 1, "").count_chars()
}

var i = 0
var total = 0

while i < 5000 {
  let option = if i % 2 == 0 { "root" } else { "shell" }
  total += score(option, i)
  i += 1
}

total % 256
"#,
    )
    .expect("write main script");
    dir
}

fn parsed_entry_source_id(parsed: &ArenaParseOutput) -> SourceId {
    parsed
        .arena
        .arena
        .span_source_id
        .or_else(|| {
            parsed
                .arena
                .statement_ids()
                .next()
                .map(|stmt| parsed.arena.arena.stmt(stmt).span.source_id)
        })
        .unwrap_or_else(|| SourceId::new(0))
}

fn parse_check_disk_script(script: &Path) {
    let (sources, parsed) =
        xsh::parse_script_with_module_roots(script.to_str().expect("utf-8 bench path"), &[])
            .expect("parse script");
    assert!(
        parsed.diagnostics.is_empty(),
        "benchmark source must parse: {:?}",
        parsed.diagnostics
    );
    let source_id = parsed_entry_source_id(&parsed);
    let source = sources
        .get(source_id)
        .expect("benchmark source must exist")
        .text();
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(
        checked.diagnostics.is_empty(),
        "benchmark source must check: {:?}",
        checked.diagnostics
    );
    black_box((sources, parsed, checked));
}

fn parse_check_lower_disk_script(script: &Path) {
    let (sources, parsed) =
        xsh::parse_script_with_module_roots(script.to_str().expect("utf-8 bench path"), &[])
            .expect("parse script");
    assert!(
        parsed.diagnostics.is_empty(),
        "benchmark source must parse: {:?}",
        parsed.diagnostics
    );
    let source_id = parsed_entry_source_id(&parsed);
    {
        let mut evaluator = Evaluator::new_with_sources(Vec::new(), sources);
        let diagnostics = evaluator.install_compact_lowered_program(&parsed.arena, source_id);
        assert!(
            diagnostics.is_empty(),
            "benchmark source must compact-check/lower: {diagnostics:?}"
        );
        black_box(evaluator);
    }
    black_box(parsed);
}

#[derive(Clone)]
struct SmallFrontendSource {
    name: String,
    text: String,
    lines: usize,
    bytes: usize,
}

struct SmallFrontendCorpus {
    sources: Vec<SmallFrontendSource>,
    skipped_large: usize,
    skipped_parse: usize,
    skipped_check: usize,
}

impl SmallFrontendCorpus {
    fn total_lines(&self) -> usize {
        self.sources.iter().map(|source| source.lines).sum()
    }

    fn total_bytes(&self) -> usize {
        self.sources.iter().map(|source| source.bytes).sum()
    }
}

fn frontend_bench_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join("Cargo.toml").is_file()
            && dir.join("src").join("lib.rs").is_file()
            && dir.join("perf").is_dir()
        {
            return dir;
        }
        assert!(dir.pop(), "could not find xsh workspace root");
    }
}

fn load_small_frontend_corpus() -> SmallFrontendCorpus {
    let root = frontend_bench_root();
    let mut paths = Vec::new();
    collect_small_frontend_paths(&root, &mut paths);
    paths.sort();

    let mut sources = Vec::new();
    let mut skipped_large = 0usize;
    let mut skipped_parse = 0usize;
    let mut skipped_check = 0usize;

    for path in paths {
        let text = fs::read_to_string(&path).expect("read small frontend source");
        let lines = text.lines().count();
        let bytes = text.len();
        if lines > SMALL_FRONTEND_MAX_LINES || bytes > SMALL_FRONTEND_MAX_BYTES {
            skipped_large += 1;
            continue;
        }
        let name = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .display()
            .to_string();
        let mut source_map = SourceMap::new();
        let source_id = source_map.add_file(&name, &text);
        let parsed = Parser::parse_source_arena_only(source_id, &text);
        if !parsed.diagnostics.is_empty() {
            skipped_parse += 1;
            continue;
        }
        let checked = Checker::check_arena(&parsed.arena, &text);
        if !checked.diagnostics.is_empty() {
            skipped_check += 1;
            continue;
        }
        sources.push(SmallFrontendSource {
            name,
            text,
            lines,
            bytes,
        });
    }

    SmallFrontendCorpus {
        sources,
        skipped_large,
        skipped_parse,
        skipped_check,
    }
}

fn collect_small_frontend_paths(dir: &Path, paths: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).expect("read frontend corpus dir");
    for entry in entries {
        let entry = entry.expect("read frontend corpus entry");
        let path = entry.path();
        let file_type = entry.file_type().expect("read frontend corpus file type");
        if file_type.is_dir() {
            if should_skip_frontend_bench_dir(&path) {
                continue;
            }
            collect_small_frontend_paths(&path, paths);
        } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == "xsh") {
            paths.push(path);
        }
    }
}

fn should_skip_frontend_bench_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git" | "target" | "docs-html")
    )
}

fn parse_source_only(name: &str, source: &str) {
    let mut sources = SourceMap::new();
    let source_id = sources.add_file(name, source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "benchmark source must parse: {:?}",
        parsed.diagnostics
    );
    black_box((sources, parsed));
}

fn parse_small_frontend_corpus(corpus: &SmallFrontendCorpus) {
    for source in &corpus.sources {
        parse_source_only(&source.name, &source.text);
    }
}

fn parse_check_small_frontend_corpus(corpus: &SmallFrontendCorpus) {
    for source in &corpus.sources {
        xsh::runtime::bench::parse_check_source(&source.name, &source.text);
    }
}

fn parse_check_lower_small_frontend_corpus(corpus: &SmallFrontendCorpus) {
    for source in &corpus.sources {
        xsh::runtime::bench::prepare_and_lower_source(&source.name, &source.text);
    }
}

fn parse_check_lower_source_timings(
    corpus: &SmallFrontendCorpus,
) -> Vec<(String, xsh::runtime::bench::FrontendLowerTimings)> {
    corpus
        .sources
        .iter()
        .map(|source| {
            let timings =
                xsh::runtime::bench::time_prepare_and_lower_source(&source.name, &source.text);
            (source.name.clone(), timings)
        })
        .collect::<Vec<_>>()
}

fn sum_parse_check_lower_timings(
    timings: &[(String, xsh::runtime::bench::FrontendLowerTimings)],
) -> xsh::runtime::bench::FrontendLowerTimings {
    let mut total = xsh::runtime::bench::FrontendLowerTimings::default();
    for (_, timing) in timings {
        total.parse += timing.parse;
        total.evaluator_init += timing.evaluator_init;
        total.evaluator_current_dir += timing.evaluator_current_dir;
        total.evaluator_struct_init += timing.evaluator_struct_init;
        total.evaluator_args_bindings += timing.evaluator_args_bindings;
        total.compact_declarations += timing.compact_declarations;
        total.compact_runtime_declarations += timing.compact_runtime_declarations;
        total.compact_bodies += timing.compact_bodies;
        total.lower_functions += timing.lower_functions;
        total.lower_top_level += timing.lower_top_level;
        total.compact_commit += timing.compact_commit;
        total.compact_install += timing.compact_install;
        total.teardown += timing.teardown;
        total.total += timing.total;
    }
    total
}

fn slowest_parse_check_lower_sources(
    timings: &[(String, xsh::runtime::bench::FrontendLowerTimings)],
    limit: usize,
) -> Vec<(String, xsh::runtime::bench::FrontendLowerTimings)> {
    let mut timings = timings.to_vec();
    timings.sort_unstable_by_key(|(_name, timings)| std::cmp::Reverse(timings.total));
    timings.truncate(limit);
    timings
}

fn bench_frontend(c: &mut Criterion) {
    let mut group = c.benchmark_group("frontend");
    group.measurement_time(Duration::from_secs(5));

    let empty = "";
    let loop_10k = xsh::runtime::bench::loop_sum_source(10_000);
    let pure_call_chain_20k = xsh::runtime::bench::pure_call_chain_source(20_000);
    let stream_callback_pure_5k = xsh::runtime::bench::stream_callback_pure_source(5_000);
    let mixed_2k = xsh::runtime::bench::mixed_glue_source(2_000);
    let import_dir = frontend_import_fixture();
    let import_script = import_dir.path().join("main.xsh");
    let small_corpus = load_small_frontend_corpus();

    eprintln!();
    eprintln!("  -- frontend allocation audit --");
    for (name, source) in [
        ("empty", empty),
        ("loop_10k", loop_10k.as_str()),
        ("pure_call_chain_20k", pure_call_chain_20k.as_str()),
        ("stream_callback_pure_5k", stream_callback_pure_5k.as_str()),
        ("mixed_glue_2k", mixed_2k.as_str()),
    ] {
        let parse_check = measure_allocs(|| {
            xsh::runtime::bench::parse_check_source(name, source);
        });
        let parse_check_lower = measure_allocs(|| {
            xsh::runtime::bench::prepare_and_lower_source(name, source);
        });
        eprintln!("  [alloc] {name}: parse/check {parse_check}; +lower {parse_check_lower}");
    }
    let parse_check = measure_allocs(|| parse_check_disk_script(&import_script));
    let parse_check_lower = measure_allocs(|| parse_check_lower_disk_script(&import_script));
    eprintln!("  [alloc] import_disk: parse/check {parse_check}; +lower {parse_check_lower}");
    eprintln!(
        "  [small corpus] files={} lines={} bytes={} skipped_large={} skipped_parse={} skipped_check={} thresholds={} lines/{} bytes",
        small_corpus.sources.len(),
        small_corpus.total_lines(),
        small_corpus.total_bytes(),
        small_corpus.skipped_large,
        small_corpus.skipped_parse,
        small_corpus.skipped_check,
        SMALL_FRONTEND_MAX_LINES,
        SMALL_FRONTEND_MAX_BYTES
    );
    let parse_only = measure_allocs(|| parse_small_frontend_corpus(&small_corpus));
    let parse_check = measure_allocs(|| parse_check_small_frontend_corpus(&small_corpus));
    let parse_check_lower =
        measure_allocs(|| parse_check_lower_small_frontend_corpus(&small_corpus));
    eprintln!(
        "  [alloc] small_corpus: parse {parse_only}; parse/check {parse_check}; +lower {parse_check_lower}"
    );
    eprintln!(
        "  [alloc] small_corpus_per_file: parse {}; parse/check {}; +lower {}",
        parse_only.per_file(small_corpus.sources.len()),
        parse_check.per_file(small_corpus.sources.len()),
        parse_check_lower.per_file(small_corpus.sources.len())
    );
    let small_corpus_timings = parse_check_lower_source_timings(&small_corpus);
    let total_timings = sum_parse_check_lower_timings(&small_corpus_timings);
    eprintln!(
        "  [small corpus phase total] total={:?} parse={:?} evaluator_init={:?} cwd={:?} struct={:?} args={:?} decl={:?} runtime_decl={:?} body={:?} functions={:?} top_level={:?} commit={:?} compact_install={:?} teardown={:?}",
        total_timings.total,
        total_timings.parse,
        total_timings.evaluator_init,
        total_timings.evaluator_current_dir,
        total_timings.evaluator_struct_init,
        total_timings.evaluator_args_bindings,
        total_timings.compact_declarations,
        total_timings.compact_runtime_declarations,
        total_timings.compact_bodies,
        total_timings.lower_functions,
        total_timings.lower_top_level,
        total_timings.compact_commit,
        total_timings.compact_install,
        total_timings.teardown
    );
    for (name, timings) in slowest_parse_check_lower_sources(&small_corpus_timings, 10) {
        eprintln!(
            "  [small corpus slow] total={:?} parse={:?} evaluator_init={:?} cwd={:?} struct={:?} args={:?} decl={:?} runtime_decl={:?} body={:?} functions={:?} top_level={:?} commit={:?} compact_install={:?} teardown={:?} {name}",
            timings.total,
            timings.parse,
            timings.evaluator_init,
            timings.evaluator_current_dir,
            timings.evaluator_struct_init,
            timings.evaluator_args_bindings,
            timings.compact_declarations,
            timings.compact_runtime_declarations,
            timings.compact_bodies,
            timings.lower_functions,
            timings.lower_top_level,
            timings.compact_commit,
            timings.compact_install,
            timings.teardown
        );
    }

    group.bench_function("parse_check_empty", |b| {
        b.iter(|| {
            xsh::runtime::bench::parse_check_source("bench-empty.xsh", black_box(empty));
        });
    });
    group.bench_function("parse_check_lower_empty", |b| {
        b.iter(|| {
            xsh::runtime::bench::prepare_and_lower_source("bench-empty.xsh", black_box(empty));
        });
    });
    group.bench_function("parse_check_loop_10k", |b| {
        b.iter(|| {
            xsh::runtime::bench::parse_check_source(
                "bench-loop-10k.xsh",
                black_box(loop_10k.as_str()),
            );
        });
    });
    group.bench_function("parse_check_lower_loop_10k", |b| {
        b.iter(|| {
            xsh::runtime::bench::prepare_and_lower_source(
                "bench-loop-10k.xsh",
                black_box(loop_10k.as_str()),
            );
        });
    });
    group.bench_function("parse_check_pure_call_chain_20k", |b| {
        b.iter(|| {
            xsh::runtime::bench::parse_check_source(
                "bench-pure-call-chain-20k.xsh",
                black_box(pure_call_chain_20k.as_str()),
            );
        });
    });
    group.bench_function("parse_check_lower_pure_call_chain_20k", |b| {
        b.iter(|| {
            xsh::runtime::bench::prepare_and_lower_source(
                "bench-pure-call-chain-20k.xsh",
                black_box(pure_call_chain_20k.as_str()),
            );
        });
    });
    group.bench_function("parse_check_stream_callback_pure_5k", |b| {
        b.iter(|| {
            xsh::runtime::bench::parse_check_source(
                "bench-stream-callback-pure-5k.xsh",
                black_box(stream_callback_pure_5k.as_str()),
            );
        });
    });
    group.bench_function("parse_check_lower_stream_callback_pure_5k", |b| {
        b.iter(|| {
            xsh::runtime::bench::prepare_and_lower_source(
                "bench-stream-callback-pure-5k.xsh",
                black_box(stream_callback_pure_5k.as_str()),
            );
        });
    });
    group.bench_function("parse_check_mixed_glue_2k", |b| {
        b.iter(|| {
            xsh::runtime::bench::parse_check_source(
                "bench-mixed-glue-2k.xsh",
                black_box(mixed_2k.as_str()),
            );
        });
    });
    group.bench_function("parse_check_lower_mixed_glue_2k", |b| {
        b.iter(|| {
            xsh::runtime::bench::prepare_and_lower_source(
                "bench-mixed-glue-2k.xsh",
                black_box(mixed_2k.as_str()),
            );
        });
    });
    group.bench_function("parse_check_import_disk", |b| {
        b.iter(|| parse_check_disk_script(black_box(&import_script)));
    });
    group.bench_function("parse_check_lower_import_disk", |b| {
        b.iter(|| parse_check_lower_disk_script(black_box(&import_script)));
    });
    group.bench_function("parse_small_corpus_le200_lines_16k", |b| {
        b.iter(|| parse_small_frontend_corpus(black_box(&small_corpus)));
    });
    group.bench_function("parse_check_small_corpus_le200_lines_16k", |b| {
        b.iter(|| parse_check_small_frontend_corpus(black_box(&small_corpus)));
    });
    group.bench_function("parse_check_lower_small_corpus_le200_lines_16k", |b| {
        b.iter(|| parse_check_lower_small_frontend_corpus(black_box(&small_corpus)));
    });

    group.finish();
}

fn bench_alloc_audit(c: &mut Criterion) {
    let mut group = c.benchmark_group("alloc_audit");

    eprintln!();
    eprintln!("  -- allocation audit --");
    let history = synthetic_history_45k();
    let session = BenchSession::with_history(history);
    let autosuggest_line = BenchLine::new("git");

    let stats = measure_allocs(|| {
        black_box(session.prefix_search("git commit"));
    });
    eprintln!("  [alloc] history_prefix_search_45k: {stats}");

    let stats = measure_allocs(|| {
        black_box(session.autosuggestion(&autosuggest_line));
    });
    eprintln!("  [alloc] autosuggestion_45k:       {stats}");

    let stats = measure_allocs(|| {
        black_box(session.fuzzy_search("gco"));
    });
    eprintln!("  [alloc] fuzzy_search_45k:         {stats}");

    group.bench_function("noop", |b| b.iter(|| black_box(1)));
    group.finish();
}

criterion_group!(
    benches,
    bench_history,
    bench_history_load,
    bench_completion,
    bench_cwd_snapshot,
    bench_render,
    bench_prompt_listing,
    bench_record_map,
    bench_json,
    bench_interpreter,
    bench_frontend,
    bench_alloc_audit
);
criterion_main!(benches);
