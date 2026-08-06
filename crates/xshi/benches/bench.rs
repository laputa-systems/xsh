//! User-facing latency benchmarks for the interactive `xshi` crate.

#[cfg(target_os = "linux")]
use std::ffi::CStr;
use std::fs;
#[cfg(target_os = "linux")]
use std::hint::black_box;
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::sync::OnceLock;

use rustybench::{AllocProfiler, Bencher};
use xshi::interactive::bench::{
    BenchSession, HistorySearchRenderBench, RenderBench, synthetic_history_45k,
};

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

fn make_directory_fixture(entries: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create directory fixture");
    for index in 0..entries {
        let path = dir.path().join(format!("dir-{index:04}"));
        fs::create_dir(&path).expect("create directory entry");
    }
    dir
}

#[rustybench::bench]
fn xshi_prompt_render_long_command(bencher: Bencher) {
    let mut render = RenderBench::new("gh api repos/laputa-systems/xsh/issues", 80);
    bench_operation(bencher, || render.render_prompt("$ ", 80));
}

#[rustybench::bench]
fn xshi_completion_navigation_1000_entries(bencher: Bencher) {
    let dir = make_directory_fixture(1_000);
    let _cwd = CurrentDirGuard::new();
    let mut session = BenchSession::with_history(Vec::new());
    session.set_cwd(dir.path());
    bench_operation(bencher, || session.complete_len("cd dir-09", 9, 80));
}

#[rustybench::bench]
fn xshi_history_search_render_45000_entries(bencher: Bencher) {
    let mut search = HistorySearchRenderBench::new("cargo test xsh", 40, 100);
    bench_operation(bencher, || search.render_navigation());
}

#[rustybench::bench]
fn xshi_cd_list_complete_1000_entries(bencher: Bencher) {
    let dir = make_directory_fixture(1_000);
    let _cwd = CurrentDirGuard::new();
    let mut session = BenchSession::with_history(synthetic_history_45k());
    bench_operation(bencher, || session.workflow_cd_l_completion_len(dir.path()));
}

#[rustybench::bench]
fn xshi_dynamic_name_session(bencher: Bencher) {
    let mut session = BenchSession::with_history(Vec::new());
    let commands = (0..8)
        .map(|index| format!("DYNAMIC_SESSION_{index}=ok /usr/bin/true"))
        .collect::<Vec<_>>();
    let mut index = 0;
    bench_operation(bencher, || {
        let output = session.execute_len(&commands[index % commands.len()]);
        index += 1;
        output
    });
}

fn main() {
    rustybench::main();
}
