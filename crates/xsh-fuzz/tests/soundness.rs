//! "Well-typed programs do not go wrong", at a fixed seed set.
//!
//! Every generated program must pass the checker, prepare, run in the
//! sandbox without a runtime or internal error, and print exactly what the
//! reference evaluator predicts. Registry probes the checker accepts must run
//! without internal or type errors, and mutants must never crash the
//! frontend. `make fuzz` runs the same properties over fresh seeds.

#[macro_use]
#[path = "../../../tests/release_binary.rs"]
mod release_binary;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use xsh_fuzz::driver::{minimize, run_seed};
use xsh_fuzz::generator::GenConfig;
use xsh_fuzz::harness::Sandbox;
use xsh_fuzz::mutate::{check_mutant, corpus_files, mutate};
use xsh_fuzz::rng::Rng;

const GENERATED_SEEDS: u64 = 1500;
const MUTANT_SEEDS: u64 = 1000;

fn sandbox() -> Sandbox {
    Sandbox::new(PathBuf::from(release_bin!("xsh-fuzz")))
}

fn jobs() -> usize {
    std::thread::available_parallelism().map_or(2, std::num::NonZeroUsize::get).div_ceil(2).clamp(1, 4)
}

/// Runs `body` for every seed in `0..count` on a few threads, collecting
/// failure reports.
fn for_seeds(count: u64, body: impl Fn(u64) -> Option<String> + Sync) -> Vec<String> {
    let next = AtomicU64::new(0);
    let failures = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..jobs() {
            scope.spawn(|| {
                loop {
                    let seed = next.fetch_add(1, Ordering::Relaxed);
                    if seed >= count {
                        break;
                    }
                    if let Some(failure) = body(seed) {
                        failures.lock().unwrap().push(failure);
                    }
                }
            });
        }
    });
    failures.into_inner().unwrap()
}

#[test]
fn oracle_methods_match_the_registry() {
    xsh_fuzz::methods::verify_against_registry().unwrap();
}

#[test]
fn well_typed_programs_run_and_match_the_reference_evaluator() {
    let sandbox = sandbox();
    let config = GenConfig::default();
    let failures = for_seeds(GENERATED_SEEDS, |seed| {
        let (generated, failure) = run_seed(seed, &config, Some(&sandbox)).err()?;
        let (small, failure) = minimize(&generated, &failure, Some(&sandbox), 400);
        Some(format!("seed {seed}: {}\n{}\n--- minimized program\n{}", failure.kind(), failure.detail(), small.source))
    });
    assert!(failures.is_empty(), "{} of {GENERATED_SEEDS} generated programs went wrong:\n\n{}", failures.len(), failures.join("\n\n"));
}

#[test]
fn accepted_registry_probes_run_without_internal_or_type_errors() {
    let corpus = xsh_fuzz::probes::corpus();
    let summary = xsh_fuzz::probes::run_corpus(&corpus, &sandbox(), 40);
    let failures: Vec<String> = summary
        .failures
        .iter()
        .map(|(probe, failure)| format!("{}: {}\n{failure}", probe.label, probe.call))
        .collect();
    assert!(failures.is_empty(), "{} probe failures:\n\n{}", failures.len(), failures.join("\n\n"));
    // Every registered pure method or function contributes probes, and the
    // checker accepts the calling conventions the registry declares.
    assert!(summary.probes >= 300, "only {} probes", summary.probes);
    assert!(summary.accepted * 10 >= summary.probes * 9, "{summary:?}");
}

fn small_corpus() -> Vec<(PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    corpus_files(&root)
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            (text.len() < 12_000).then_some((path, text))
        })
        .collect()
}

#[test]
fn mutants_get_ordinary_diagnostics() {
    let corpus = small_corpus();
    assert!(corpus.len() > 50, "corpus has {} files", corpus.len());
    let config = GenConfig::default();
    let failures = for_seeds(MUTANT_SEEDS, |seed| {
        let mut rng = Rng::new(seed);
        let (file, text) = if seed % 2 == 0 {
            ("program.xsh".to_string(), xsh_fuzz::generator::generate(seed, &config).source)
        } else {
            let (path, text) = &corpus[rng.below(corpus.len())];
            (format!("{} as {}", path.display(), xsh_fuzz::mutate::MUTANT_FILE), text.clone())
        };
        let mut mutant = mutate(&text, &mut rng);
        for _ in 0..rng.below(3) {
            mutant = mutate(&mutant, &mut rng);
        }
        let failure = check_mutant(xsh_fuzz::mutate::MUTANT_FILE, &mutant).err()?;
        Some(format!("seed {seed} ({file}): {failure}\n--- mutant\n{mutant}"))
    });
    assert!(failures.is_empty(), "{} mutants broke the frontend:\n\n{}", failures.len(), failures.join("\n\n"));
}

#[test]
fn sandbox_kills_a_child_over_the_memory_limit() {
    // macOS enforces no data rlimit; the parent's footprint sampling must
    // stop a program that keeps doubling a list.
    let mut sandbox = sandbox();
    sandbox.memory_limit = 128 << 20;
    sandbox.timeout = std::time::Duration::from_secs(60);
    let run = sandbox.run("var items = [0]\nwhile true {\n  items += items\n}\n").unwrap();
    assert!(run.memory_exceeded.is_some(), "{run:?}");
    assert!(run.elapsed < std::time::Duration::from_secs(30), "{run:?}");
}
