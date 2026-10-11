//! "Well-typed programs do not go wrong", at a fixed seed set.
//!
//! Every generated program must pass the checker, prepare, run in the
//! sandbox without a runtime or internal error, and print exactly what the
//! reference evaluator predicts. Registry probes the checker accepts must run
//! without internal or type errors, and mutants must never crash the
//! frontend. `make fuzz` runs the same properties over fresh seeds.

#[macro_use]
#[path = "../../../tests/test_binary.rs"]
mod test_binary;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use xsh::frontend::syntax::grammar::earley::Recognizer;
use xsh::frontend::syntax::grammar::{grammar, lex_grammar_tokens};
use xsh_fuzz::driver::{minimize, run_seed};
use xsh_fuzz::generator::GenConfig;
use xsh_fuzz::harness::Sandbox;
use xsh_fuzz::mutate::{check_mutant, corpus_files, mutate};
use xsh_fuzz::rng::Rng;

const GENERATED_SEEDS: u64 = 1500;
const MUTANT_SEEDS: u64 = 1000;

fn sandbox() -> Sandbox {
    Sandbox::new(PathBuf::from(test_bin!("xsh-fuzz")))
}

// The sandbox reads the Rust caller's environment before XSH starts, so a host
// subprocess must arrange and verify this process-launch boundary.
#[test]
fn sandbox_preserves_explicit_llvm_profile_destination_only() {
    const WORKER: &str = "XSH_FUZZ_PROFILE_ENV_TEST_WORKER";
    if std::env::var_os(WORKER).is_some() {
        let destination = std::env::var("LLVM_PROFILE_FILE").expect("profile destination");
        let directory = Path::new(&destination).parent().expect("profile parent");
        let before: std::collections::BTreeSet<_> = std::fs::read_dir(directory)
            .expect("read profile directory")
            .map(|entry| entry.expect("profile entry").path())
            .collect();
        let report = sandbox()
            .run(
                "print (e\"LLVM_PROFILE_FILE\" ?? \"missing\")\n\
                 print (e\"XSH_FUZZ_PROFILE_ENV_SENTINEL\" ?? \"missing\")\n\
                 print (e\"PATH\" ?? \"missing\")\n\
                 print (e\"XSH_FUZZ_PROFILE_ENV_TEST_WORKER\" ?? \"missing\")\n",
            )
            .expect("run sandbox environment probe");
        assert_eq!(report.status, Some(0), "{report:?}");
        assert_eq!(report.stdout, format!("{destination}\nmissing\nmissing\nmissing\n"));
        if std::env::var_os("XSH_FUZZ_PROFILE_EXPECT_COUNTERS").is_some() {
            assert!(std::fs::read_dir(directory)
                .expect("read child profiles")
                .map(|entry| entry.expect("child profile entry").path())
                .any(|path| !before.contains(&path)
                    && path.extension().is_some_and(|extension| extension == "profraw")
                    && std::fs::metadata(path).expect("child profile metadata").len() > 0),
                "instrumented child emitted no profile");
        }
        let forbidden = directory.join("script-file-growth");
        let blocked = sandbox()
            .run(&format!("fp\"{}\".write(\"must not grow\")?\n", forbidden.display()))
            .expect("run file-growth probe");
        assert_eq!(blocked.signal, Some(libc::SIGXFSZ), "{blocked:?}");
        assert_eq!(std::fs::metadata(&forbidden).expect("blocked file metadata").len(), 0);
        std::fs::remove_file(forbidden).expect("remove blocked file");
        return;
    }

    // A separate test process supplies the environment without mutating the
    // environment shared by concurrently running tests.
    let profiles = tempfile::tempdir().expect("profile directory");
    let inherited_profile = std::env::var_os("LLVM_PROFILE_FILE");
    let destination = if let Some(inherited) = &inherited_profile {
        let directory = Path::new(inherited).parent().expect("inherited profile parent");
        let unique = profiles.path().file_name().expect("unique profile name").to_string_lossy();
        directory.join(format!("env-probe-{unique}-%m-%p.profraw"))
    } else {
        profiles.path().join("%m-%p.profraw")
    };
    let mut command = std::process::Command::new(std::env::current_exe().expect("test executable"));
    command.args(["--exact", "sandbox_preserves_explicit_llvm_profile_destination_only", "--nocapture"])
        .env(WORKER, "1")
        .env("LLVM_PROFILE_FILE", destination)
        .env("XSH_FUZZ_PROFILE_ENV_SENTINEL", "must remain outside the sandbox")
        .env("PATH", "must remain outside the sandbox");
    if inherited_profile.is_some() {
        command.env("XSH_FUZZ_PROFILE_EXPECT_COUNTERS", "1");
    }
    let output = command.output().expect("run isolated environment probe");
    assert!(output.status.success(), "{output:?}");
}

// The absence case needs an uninstrumented child to observe environment
// isolation without LLVM attempting a default-file write at child exit.
#[test]
fn sandbox_without_profile_destination_clears_caller_environment() {
    const WORKER: &str = "XSH_FUZZ_CLEAR_ENV_TEST_WORKER";
    if std::env::var_os(WORKER).is_some() {
        use std::os::unix::fs::PermissionsExt;

        assert!(std::env::var_os("LLVM_PROFILE_FILE").is_none());
        // This host probe is uninstrumented, so absence of the destination
        // cannot trigger an LLVM default-file write in the sandbox child.
        let root = tempfile::tempdir().expect("environment probe directory");
        let executable = root.path().join("environment-probe.sh");
        std::fs::write(&executable,
            "#!/bin/sh\nprintf '%s\\n' \"${LLVM_PROFILE_FILE-missing}\" \"${XSH_FUZZ_PROFILE_ENV_SENTINEL-missing}\" \"${XSH_FUZZ_CLEAR_ENV_TEST_WORKER-missing}\"\n")
            .expect("write uninstrumented environment probe");
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))
            .expect("make environment probe executable");
        let report = Sandbox::new(executable).run("unused").expect("run environment probe");
        assert_eq!(report.status, Some(0), "{report:?}");
        assert_eq!(report.stdout, "missing\nmissing\nmissing\n");
        return;
    }

    let profiles = tempfile::tempdir().expect("default profile directory");
    let inherited_profile = std::env::var_os("LLVM_PROFILE_FILE");
    let directory = inherited_profile.as_ref()
        .map(|value| Path::new(value).parent().expect("inherited profile parent"))
        .unwrap_or(profiles.path());
    let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", "sandbox_without_profile_destination_clears_caller_environment", "--nocapture"])
        .env_remove("LLVM_PROFILE_FILE")
        .env(WORKER, "1")
        .env("XSH_FUZZ_PROFILE_ENV_SENTINEL", "must remain outside the sandbox")
        .current_dir(directory)
        .output()
        .expect("run isolated default environment probe");
    assert!(output.status.success(), "{output:?}");
}

fn jobs() -> usize {
    std::thread::available_parallelism()
        .map_or(2, std::num::NonZeroUsize::get)
        .div_ceil(2)
        .clamp(1, 4)
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

/// Every generated program is a sentence of the grammar's productions
/// (`src/syntax/grammar.rs`), lexed with the real lexer.
#[test]
fn generated_programs_are_grammar_sentences() {
    let recognizer = Recognizer::new(grammar());
    let config = GenConfig::default();
    let failures = for_seeds(GENERATED_SEEDS, |seed| {
        let source = xsh_fuzz::generator::generate(seed, &config).source;
        let Some(tokens) = lex_grammar_tokens(&source) else {
            return Some(format!("seed {seed}: the program does not lex\n{source}"));
        };
        let rejection = recognizer.recognize(&tokens).err()?;
        let near: Vec<&str> = tokens
            [rejection.token.saturating_sub(4)..(rejection.token + 3).min(tokens.len())]
            .iter()
            .map(|token| token.text)
            .collect();
        Some(format!(
            "seed {seed}: rejected near {near:?}; expected {}\n{source}",
            rejection.expected.join(" ")
        ))
    });
    assert!(
        failures.is_empty(),
        "{} of {GENERATED_SEEDS} generated programs are not grammar sentences:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
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
        Some(format!(
            "seed {seed}: {}\n{}\n--- minimized program\n{}",
            failure.kind(),
            failure.detail(),
            small.source
        ))
    });
    assert!(
        failures.is_empty(),
        "{} of {GENERATED_SEEDS} generated programs went wrong:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// Seeds whose programs consume a `for` pipeline whose stage block reads a
/// variable the loop body assigns between pulls, pinning the item-by-item
/// consumption the runtime and the reference evaluator must agree on.
const LAZY_PIPELINE_SEEDS: &[u64] = &[1791136586894360375];

#[test]
fn pipeline_stage_blocks_observe_loop_body_mutations() {
    let sandbox = sandbox();
    let config = GenConfig::default();
    for &seed in LAZY_PIPELINE_SEEDS {
        if let Err((_, failure)) = run_seed(seed, &config, Some(&sandbox)) {
            panic!("seed {seed}: {}: {}", failure.kind(), failure.detail());
        }
    }
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
    assert!(
        failures.is_empty(),
        "{} probe failures:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
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
            (
                "program.xsh".to_string(),
                xsh_fuzz::generator::generate(seed, &config).source,
            )
        } else {
            let (path, text) = &corpus[rng.below(corpus.len())];
            (
                format!("{} as {}", path.display(), xsh_fuzz::mutate::MUTANT_FILE),
                text.clone(),
            )
        };
        let mut mutant = mutate(&text, &mut rng);
        for _ in 0..rng.below(3) {
            mutant = mutate(&mutant, &mut rng);
        }
        let failure = check_mutant(xsh_fuzz::mutate::MUTANT_FILE, &mutant).err()?;
        Some(format!(
            "seed {seed} ({file}): {failure}\n--- mutant\n{mutant}"
        ))
    });
    assert!(
        failures.is_empty(),
        "{} mutants broke the frontend:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

#[test]
fn sandbox_kills_a_child_over_the_memory_limit() {
    // macOS enforces no data rlimit; the parent's footprint sampling must
    // stop a program that keeps doubling a list.
    let mut sandbox = sandbox();
    sandbox.memory_limit = 128 << 20;
    sandbox.timeout = std::time::Duration::from_secs(60);
    let run = sandbox
        .run("var items = [0]\nwhile true {\n  items += items\n}\n")
        .unwrap();
    assert!(run.memory_exceeded.is_some(), "{run:?}");
    assert!(run.elapsed < std::time::Duration::from_secs(30), "{run:?}");
}
