//! User-facing latency benchmarks for the XSH multicall tools.

#[cfg(target_os = "linux")]
use std::ffi::CStr;
use std::fs;
#[cfg(target_os = "linux")]
use std::hint::black_box;
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::sync::OnceLock;

use divan::{AllocProfiler, Bencher};
use xsh::runner::{RunOptions, run_script};
use xshi::interactive::bench::{
    BenchSession, HistorySearchRenderBench, RenderBench, synthetic_history_45k,
};

#[cfg(target_os = "linux")]
#[global_allocator]
static ALLOC: AllocProfiler<mimalloc::MiMalloc> = AllocProfiler::new(mimalloc::MiMalloc);

#[cfg(not(target_os = "linux"))]
#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

#[cfg(target_os = "linux")]
const TRACE_BEGIN: &CStr = c"BENCH_BEGIN";
#[cfg(target_os = "linux")]
const TRACE_END: &CStr = c"BENCH_END";

#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn prctl(option: i32, ...) -> i32;
}

#[cfg(target_os = "linux")]
fn syscall_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SYSCALL_TRACE").is_some())
}

#[cfg(target_os = "linux")]
fn trace_marker(marker: &CStr) {
    if syscall_trace_enabled() {
        unsafe {
            let _ = prctl(15, marker.as_ptr(), 0, 0, 0);
        }
    }
}

#[cfg(target_os = "linux")]
fn bench_operation<O>(bencher: Bencher, mut operation: impl FnMut() -> O) {
    bencher.bench_local(|| {
        trace_marker(TRACE_BEGIN);
        let result = operation();
        trace_marker(TRACE_END);
        black_box(result);
    });
}

#[cfg(not(target_os = "linux"))]
fn bench_operation<O>(bencher: Bencher, operation: impl FnMut() -> O) {
    bencher.bench_local(operation);
}

struct BenchDir(tempfile::TempDir);

impl BenchDir {
    fn new() -> Self {
        Self(tempfile::tempdir().expect("create benchmark directory"))
    }

    fn path(&self) -> &Path {
        self.0.path()
    }
}

struct CurrentDirGuard(PathBuf);

impl CurrentDirGuard {
    fn new() -> Self {
        Self(std::env::current_dir().expect("read current directory"))
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.0).expect("restore current directory");
    }
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repository root")
}

fn benchmark_script(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("benches/scripts")
        .join(name)
}

fn run_benchmark_script(path: &Path, args: Vec<String>) -> usize {
    let output = run_script(RunOptions {
        script: path.to_string_lossy().into_owned(),
        args,
        coverage_trace_dir: None,
        strict_lower: false,
    });
    assert_eq!(output.status, 0, "{}", String::from_utf8_lossy(&output.stderr));
    output.stdout.len()
}

fn make_source_corpus(root: &Path, files: usize) {
    fs::create_dir_all(root).expect("create source corpus");
    for index in 0..files {
        let extension = match index % 5 {
            0 => "rs",
            1 => "xsh",
            2 => "json",
            3 => "md",
            _ => "toml",
        };
        let body = format!(
            "fixture {index}\nname = item-{index}\nvalue = {}\n",
            index * 17
        );
        fs::write(root.join(format!("file-{index:04}.{extension}")), body)
            .expect("write source fixture");
    }
}

fn make_log_corpus(root: &Path) {
    fs::create_dir_all(root).expect("create log corpus");
    for file in 0..10 {
        let mut text = String::new();
        for row in 0..1_000 {
            let index = file * 1_000 + row;
            let level = if index % 7 == 0 { "debug" } else { "info" };
            text.push_str(&format!(
                "{{\"service\":\"svc{}\",\"level\":\"{level}\",\"duration_ms\":{}}}\n",
                index % 4,
                5 + index % 200
            ));
        }
        fs::write(root.join(format!("service-{file}.jsonl")), text).expect("write log fixture");
    }
}

fn make_package_corpus(root: &Path, files: usize) {
    let package = root.join("usr/share/demo");
    fs::create_dir_all(&package).expect("create package corpus");
    for index in 0..files {
        let body = format!(
            "fixture {index}\nname = item-{index}\nvalue = {}\n",
            index * 17
        );
        fs::write(package.join(format!("payload-{index:04}.txt")), body)
            .expect("write package fixture");
    }
}

#[divan::bench]
fn xsh_short_script(bencher: Bencher) {
    let script = benchmark_script("short-script.xsh");
    bench_operation(bencher, || run_benchmark_script(&script, Vec::new()));
}

#[divan::bench]
fn xsh_process_pipeline(bencher: Bencher) {
    let fixture = BenchDir::new();
    let script = benchmark_script("process-pipeline.xsh");
    let output = fixture.path().join("output.txt").to_string_lossy().into_owned();
    bench_operation(bencher, || {
        run_benchmark_script(&script, vec![output.clone()])
    });
}

#[divan::bench]
fn xsh_extension_count_1000_files(bencher: Bencher) {
    let fixture = BenchDir::new();
    let root = fixture.path().join("src");
    make_source_corpus(&root, 1_000);
    let script = benchmark_script("extension-count.xsh");
    let root = root.to_string_lossy().into_owned();
    bench_operation(bencher, || {
        run_benchmark_script(&script, vec![root.clone()])
    });
}

#[divan::bench]
fn xsh_json_log_rollup_10000_rows(bencher: Bencher) {
    let fixture = BenchDir::new();
    let root = fixture.path().join("logs");
    make_log_corpus(&root);
    let script = benchmark_script("json-log-rollup.xsh");
    let root = root.to_string_lossy().into_owned();
    bench_operation(bencher, || {
        run_benchmark_script(&script, vec![root.clone()])
    });
}

#[divan::bench]
fn xsh_manifest_hash_1000_files(bencher: Bencher) {
    let fixture = BenchDir::new();
    let root = fixture.path().join("pkgroot");
    make_package_corpus(&root, 1_000);
    let script = benchmark_script("manifest-hash.xsh");
    let root = root.to_string_lossy().into_owned();
    bench_operation(bencher, || {
        run_benchmark_script(&script, vec![root.clone()])
    });
}

fn make_directory_fixture(entries: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create directory fixture");
    for index in 0..entries {
        let path = dir.path().join(format!("dir-{index:04}"));
        fs::create_dir(&path).expect("create directory entry");
    }
    dir
}

#[divan::bench]
fn xshi_prompt_render_long_command(bencher: Bencher) {
    let mut render = RenderBench::new("gh api repos/laputa-systems/xsh/issues", 80);
    bench_operation(bencher, || render.render_prompt("$ ", 80));
}

#[divan::bench]
fn xshi_completion_navigation_1000_entries(bencher: Bencher) {
    let dir = make_directory_fixture(1_000);
    // Interactive sessions change the process cwd. Restore it before the
    // fixture is removed so later repository benchmarks see the real corpus.
    let _cwd = CurrentDirGuard::new();
    let mut session = BenchSession::with_history(Vec::new());
    session.set_cwd(dir.path());
    bench_operation(bencher, || session.complete_len("cd dir-09", 9, 80));
}

#[divan::bench]
fn xshi_history_search_render_45000_entries(bencher: Bencher) {
    let mut search = HistorySearchRenderBench::new("cargo test xsh", 40, 100);
    bench_operation(bencher, || search.render_navigation());
}

#[divan::bench]
fn xshi_cd_list_complete_1000_entries(bencher: Bencher) {
    let dir = make_directory_fixture(1_000);
    let _cwd = CurrentDirGuard::new();
    let mut session = BenchSession::with_history(synthetic_history_45k());
    bench_operation(bencher, || {
        session.workflow_cd_l_completion_len(dir.path())
    });
}

#[divan::bench]
fn xsht_check_xsh_repository(bencher: Bencher) {
    let paths = Vec::new();
    bench_operation(bencher, || {
        let output = xsht::check_paths_with_options(&paths, false, None);
        (output.status, output.stdout.len(), output.stderr.len())
    });
}

#[divan::bench]
fn xsht_format_check_xsh_repository(bencher: Bencher) {
    let paths = Vec::new();
    bench_operation(bencher, || {
        let output = xsht::format_files(&paths, true);
        (output.status, output.stdout.len(), output.stderr.len())
    });
}

#[divan::bench]
fn xsht_lint_xsh_repository(bencher: Bencher) {
    let paths = Vec::new();
    bench_operation(bencher, || {
        let output = xsht::lint_files(&paths, false, false);
        (output.status, output.stdout.len(), output.stderr.len())
    });
}

fn main() {
    std::env::set_current_dir(repository_root()).expect("enter repository root");
    divan::main();
}
