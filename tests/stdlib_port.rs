//! Architecture tests for the embedded standard-library port.
//!
//! These assert the properties that make the port sound rather than any one
//! algorithm's behavior: embedded source is prepared before execution and never
//! during it, and selection is proportional to what the program references.
//! Algorithm parity lives in `tests/xsh/stdlib/`, and what a user program
//! observes of the sealed implementation namespace in
//! `tests/xsh/stdlib/embedded_sealing.xsh`.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

use xsh::execution::script::{RunOptions, run_script};
use xsh::frontend::load::{
    StdlibLinkage, entry_source_from_text, parse_load_entry_source_arena_only_with_linkage,
};
use xsh::frontend::stdlib_preparation;

fn write_script(dir: &Path, name: &str, source: &str) -> PathBuf {
    let path = dir.join(name);
    let mut file = std::fs::File::create(&path).expect("create script");
    file.write_all(source.as_bytes()).expect("write script");
    path
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "xsh-stdlib-port-{name}-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Prepare `source` the way the runner does and report how many embedded
/// modules preparation parsed.
fn prepared_module_count(name: &str, source: &str) -> usize {
    stdlib_preparation::reset();
    let (_, parsed) = parse_load_entry_source_arena_only_with_linkage(
        name,
        entry_source_from_text(name, source.to_string()),
        Vec::new(),
        StdlibLinkage::Prepare,
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    stdlib_preparation::parsed_modules()
}

/// A static trivial program prepares zero embedded implementation
/// modules; a script-backed call prepares exactly its module closure, while
/// retained native text methods prepare none.
#[test]
fn preparation_is_proportional_to_referenced_standard_entries() {
    assert_eq!(
        prepared_module_count("trivial.xsh", "proc main() [io] {\n  print \"hi\"\n}\n"),
        0,
        "a program that uses no script-backed entry must prepare no embedded module"
    );

    assert_eq!(
        prepared_module_count(
            "text-methods.xsh",
            "proc main() [io] {\n  print \"a b\".fields().join(\"|\")\n  print \"a b\".wrap(2).join(\"|\")\n}\n"
        ),
        0,
        "native text methods must not prepare an embedded module"
    );

    assert_eq!(
        prepared_module_count(
            "unrelated.xsh",
            "proc main() [io] {\n  print bytes.human(2048)\n}\n"
        ),
        0,
        "native bytes.human must not prepare an embedded module"
    );
    assert_eq!(
        prepared_module_count(
            "native-cli.xsh",
            "proc main() [io, error] {\n  print cli.parse([\"--count\", \"3\"], {count: {kind: \"Int\"}})?.count\n}\n"
        ),
        0,
        "native cli.parse must not prepare an embedded module"
    );
    assert_eq!(
        prepared_module_count(
            "native-shlex.xsh",
            "proc main() [io] {\n  print shlex.quote(\"a b\")\n  print shlex.join([\"a b\"])\n}\n"
        ),
        0,
        "native shlex calls must not prepare an embedded module"
    );
    assert_eq!(
        prepared_module_count(
            "native-ini.xsh",
            "proc main() [io, error, fs] {\n  print ini.encode({s: {k: \"v\"}})?\n  ini.write(p\"out.ini\", {s: {k: \"v\"}})?\n}\n"
        ),
        0,
        "native INI calls must not prepare an embedded module"
    );
    assert_eq!(
        prepared_module_count(
            "native-mime.xsh",
            "proc main() [io, error, fs] {\n  print mime.lookup_ext(\"gz\")\n  print mime.lookup_path(p\"a.gz\")\n  print mime.parse(\"text/plain\")?\n}\n"
        ),
        0,
        "native MIME calls must not prepare an embedded module"
    );
    assert_eq!(
        prepared_module_count(
            "native-json-paths.xsh",
            "proc main() [io, error] {\n  print json.get({a: 1}, [\"a\"])?\n  print json.set({a: 1}, [\"a\"], 2)?\n  print json.remove({a: 1}, [\"a\"])?\n}\n"
        ),
        0,
        "native JSON path calls must not prepare the JSON-lines module"
    );
    assert_eq!(
        prepared_module_count(
            "script-json-lines.xsh",
            "proc main() [io, error] {\n  print json.encode_lines([{a: 1}])?\n}\n"
        ),
        1,
        "JSON Lines must still prepare its embedded implementation"
    );
    assert_eq!(
        prepared_module_count(
            "native-env-conversions.xsh",
            "proc main() [io, env, error] {\n  print env.get_or(\"XSH_MISSING\", \"fallback\")?\n  print env.bool(\"XSH_FLAG\")?\n  print env.int(\"XSH_COUNT\")?\n}\n"
        ),
        0,
        "native environment conversions must not prepare an embedded module"
    );
    assert_eq!(
        prepared_module_count(
            "native-check-line.xsh",
            "proc main() [io, error] {\n  print hash.parse_check_line(\"ab  file\")?\n}\n"
        ),
        0,
        "native checksum parsing must not prepare the verification policy"
    );
    assert_eq!(
        prepared_module_count(
            "script-verify-file.xsh",
            "proc main() [fs, error] {\n  hash.verify_file(p\"file\", sha256: \"ab\")?\n}\n"
        ),
        1,
        "file verification must still prepare its embedded implementation"
    );
}

/// Selection follows resolved spellings, not names that merely look like
/// one: a local binding called `load`, a record field called `load`, and a
/// native call in a mixed standard module are not references to an embedded
/// implementation, while the script-backed entry beside them is.
#[test]
fn selection_ignores_names_that_are_not_script_backed_references() {
    // A local variable and a record field named `load` are not a loading route.
    assert_eq!(
        prepared_module_count(
            "local-load.xsh",
            "proc main() [io] {\n  let load = 3\n  let record = {load: load}\n  print record.load + load\n}\n"
        ),
        0,
        "a binding named `load` must not prepare the whole catalog"
    );

    // A native entry of a module that also has script-backed entries:
    // `hash.parse_check_line` is native, `hash.verify_file` is not.
    assert_eq!(
        prepared_module_count(
            "native-mixed.xsh",
            "proc main() [io, error] {\n  print hash.parse_check_line(\"ab  file\")?\n}\n"
        ),
        0,
        "a native entry in a mixed module must not select its embedded module"
    );
    assert_eq!(
        prepared_module_count(
            "script-mixed.xsh",
            "proc main() [fs, error] {\n  hash.verify_file(p\"file\", sha256: \"ab\")?\n}\n"
        ),
        1,
        "the script-backed entry beside it selects its module"
    );

    // These hot-path calls use retained native operations. Referencing them
    // alone must not prepare the time or tui embedded implementation.
    assert_eq!(
        prepared_module_count(
            "native-duration.xsh",
            "proc main() [io, time] {\n  print time.duration_compact(69)\n}\n"
        ),
        0,
        "native duration formatting must not prepare the time module"
    );
    assert_eq!(
        prepared_module_count(
            "native-pad.xsh",
            "proc main() [io] {\n  print tui.left_pad(\"x\", 3)\n  print tui.right_pad(\"x\", 3)\n}\n"
        ),
        0,
        "native padding must not prepare the tui module"
    );
    assert_eq!(
        prepared_module_count(
            "script-tui.xsh",
            "proc main() [io] {\n  print tui.red()\n}\n"
        ),
        1,
        "script-backed tui sequences must still prepare their module"
    );

    // A module mentioned only as a `use` of that module's own name is not a
    // dynamic loading route either.
    assert_eq!(
        prepared_module_count(
            "plain-use.xsh",
            "use module\n\nproc main() [io] {\n  print \"ok\"\n}\n"
        ),
        0,
        "importing the `module` standard module is not calling `module.load`"
    );
}

/// The complete set is prepared for a resolved `module.load`, including
/// one reached through a `use` alias.
#[test]
fn a_resolved_dynamic_loading_route_prepares_the_complete_set() {
    let catalog = stdlib_catalog_size();
    assert_eq!(
        prepared_module_count(
            "aliased-load.xsh",
            "use module as mods\n\nproc main() [io, error] {\n  let m = mods.load(p\"nothing.xsh\")?\n  print m\n}\n"
        ),
        catalog,
        "a resolved loading route through an alias prepares the complete set"
    );
}

/// Repeated references to one embedded module parse it once, including
/// references that arrive through separately loaded user modules.
#[test]
fn repeated_references_parse_an_embedded_module_once() {
    assert_eq!(
        prepared_module_count(
            "both.xsh",
            "proc main() [io] {\n  print tui.red()\n  print tui.bold()\n}\n"
        ),
        1,
        "two entries of one embedded module prepare it once"
    );

    let dir = temp_dir("repeated-references");
    write_script(
        &dir,
        "helper.xsh",
        "export pure red_seq() -> Str {\n  return tui.red()\n}\n",
    );
    let entry = dir.join("entry.xsh");
    let count = prepared_module_count(
        entry.to_str().expect("utf-8 path"),
        "use helper\n\nproc main() [io] {\n  print helper.red_seq()\n  print tui.bold()\n}\n",
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        count, 1,
        "a reference from a loaded user module and one from the entry parse the module once"
    );
}

/// Embedded preparation does not grow once execution starts.
///
/// A loop that calls a script-backed entry many times, and a run that fails
/// after calling one, must both leave the count at the single preparation-time
/// parse.
#[test]
fn execution_does_not_prepare_embedded_modules() {
    let dir = temp_dir("no-late-preparation");
    let script = write_script(
        &dir,
        "loop.xsh",
        "proc main() [io] {\n  var i = 0\n  while i < 200 {\n    print tui.red()\n    i = i + 1\n  }\n}\n",
    );

    stdlib_preparation::reset();
    let output = run_script(RunOptions {
        script: script.display().to_string(),
        args: Vec::new(),
        coverage_trace_dir: None,
    });
    assert_eq!(
        output.status,
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        stdlib_preparation::parsed_modules(),
        1,
        "repeated calls must not prepare embedded modules again"
    );

    let failing = write_script(
        &dir,
        "failing.xsh",
        "proc main() [io, error] {\n  print tui.red()\n  let value = \"x\".parse_int()?\n  print value\n}\n",
    );
    stdlib_preparation::reset();
    let output = run_script(RunOptions {
        script: failing.display().to_string(),
        args: Vec::new(),
        coverage_trace_dir: None,
    });
    assert_ne!(output.status, 0, "the fallible call must fail");
    assert_eq!(
        stdlib_preparation::parsed_modules(),
        1,
        "a failing run still prepares each embedded module once"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A program that can load user code dynamically prepares the complete
/// applicable implementation set before execution.
#[test]
fn dynamic_loading_prepares_the_complete_set_before_execution() {
    let count = prepared_module_count(
        "loader.xsh",
        "proc main() [io, error] {\n  let m = module.load(p\"nothing.xsh\")?\n  print m\n}\n",
    );
    let catalog = stdlib_catalog_size();
    assert_eq!(
        count, catalog,
        "a dynamic user-code loading route prepares every applicable embedded module"
    );

    // The same program must not prepare anything further while it runs to the
    // point of the failed load.
    let dir = temp_dir("dynamic-load");
    let script = write_script(
        &dir,
        "loader.xsh",
        "proc main() [io, error] {\n  let m = module.load(p\"missing.xsh\")?\n  print m\n}\n",
    );
    stdlib_preparation::reset();
    let output = run_script(RunOptions {
        script: script.display().to_string(),
        args: Vec::new(),
        coverage_trace_dir: None,
    });
    assert_ne!(output.status, 0, "the missing module must fail to load");
    assert_eq!(
        stdlib_preparation::parsed_modules(),
        catalog,
        "dynamic loading must not prepare embedded modules after execution starts"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A dynamically loaded module calls a prepared implementation.
///
/// The loaded module is its own program, so its standard call is a link into
/// the loading program's prepared functions rather than a second copy of them.
/// Resolving the same qualified identity in two programs must not confuse the
/// evaluator's resolved-index cache, and the call must run the prepared
/// implementation rather than reparse or fall back.
#[test]
fn a_loaded_module_calls_prepared_implementations() {
    let dir = temp_dir("dynamic-call");
    let loaded = write_script(
        &dir,
        "loaded.xsh",
        "##! A module that uses a migrated entry.\n\n\
         ## Verify a file against an expected checksum.\n\
         export proc check(file: Path, checksum: Str) [fs] -> Result[Unit, Error] {\n  \
         return hash.verify_file(file, sha256: checksum)\n}\n\n\
         ## The checksum of a file.\n\
         export proc digest_of(file: Path) [fs, error] -> Result[Str, Error] {\n  \
         return hash.sha256(file)?.hex()\n}\n",
    );
    let data = dir.join("data.txt");
    std::fs::write(&data, "abc").expect("write data");
    let script = write_script(
        &dir,
        "loader.xsh",
        &format!(
            "type Loaded = module {{\n  \
             export proc check(file: Path, checksum: Str) [fs] -> Result[Unit, Error]\n  \
             export proc digest_of(file: Path) [fs, error] -> Result[Str, Error]\n}}\n\n\
             proc main() [io, error] {{\n  \
             let loaded = module.load(p\"{}\" )?.require(Loaded)?\n  \
             let hex = loaded.digest_of(p\"{}\" )?\n  \
             loaded.check(p\"{}\" , hex)?\n  \
             print f\"{{hex}}\"\n  \
             match loaded.check(p\"{}\", \"00\") {{\n    \
             Ok(_) => {{ print \"unexpected-ok\" }}\n    \
             Err(failure) => {{ print f\"{{failure.message}}\" }}\n  \
             }}\n}}\n",
            loaded.display(),
            data.display(),
            data.display(),
            data.display(),
        ),
    );
    stdlib_preparation::reset();
    let output = run_script(RunOptions {
        script: script.display().to_string(),
        args: Vec::new(),
        coverage_trace_dir: None,
    });
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status, 0, "the loaded module must run: {stderr}");
    assert!(
        stdout.contains("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
        "the loaded module must reach the prepared digest primitive: {stdout}"
    );
    assert!(
        stdout.contains("sha256 checksum must be 64 hex characters"),
        "the loaded module must reach the prepared policy: {stdout}"
    );
    let prepared = stdlib_preparation::parsed_modules();
    let catalog = stdlib_catalog_size();
    assert_eq!(
        prepared, catalog,
        "a dynamic user-code loading route prepares the complete set once \
         (exit {}, stdout {stdout:?}, stderr {stderr:?})",
        output.status
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The number of embedded modules public bindings can reach on this target.
fn stdlib_catalog_size() -> usize {
    // Keep a native-only control beside the target-aware registry expectation.
    let dir = temp_dir("catalog-size");
    let mut sources = String::new();
    for index in 0..64 {
        sources.push_str(&format!(
            "proc probe_{index}() [io] {{\n  print {index}\n}}\n"
        ));
    }
    let path = write_script(&dir, "probe.xsh", &sources);
    let count = prepared_module_count(path.to_str().expect("utf-8 path"), &sources);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        count, 0,
        "a program with no standard calls prepares nothing"
    );
    dynamic_catalog_size()
}

fn dynamic_catalog_size() -> usize {
    xsh_registry::signature::api_spec()
        .script_impls()
        .into_iter()
        .map(|(_, _, module)| module)
        .collect::<BTreeSet<_>>()
        .len()
}

/// A copied binary runs migrated APIs with no loose stdlib files, no
/// repository cwd, and no project configuration.
#[test]
fn copied_binary_runs_migrated_apis_without_repository_files() {
    let dir = temp_dir("copied-binary");
    let binary = dir.join("xsh-copy");
    std::fs::copy(release_bin!("xsh"), &binary).expect("copy the xsh binary");
    let script = write_script(
        &dir,
        "smoke.xsh",
        "proc main() [io, time] {\n  print shlex.quote(\"a b\")\n  print bytes.human(2048)\n  print time.duration_compact(3661)\n  let banner = tui.bold() + \"x\" + tui.reset()\n  print $banner\n}\n",
    );

    let output = std::process::Command::new(&binary)
        .arg(&script)
        .current_dir(&dir)
        .env_remove("XSH_MODULE_PATH")
        .output()
        .expect("run the copied binary");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "'a b'\n2.0K\n   1h01m\n\u{1b}[1mx\u{1b}[0m\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A user reference to a private implementation helper selects no embedded
/// module. That the reference is also rejected is a native test.
#[test]
fn a_reference_to_a_private_helper_prepares_nothing() {
    let count = prepared_module_count(
        "private.xsh",
        "proc main() [io] {\n  print lines_error(\"nope\")\n}\n",
    );
    assert_eq!(count, 0, "a user reference to a helper prepares nothing");
}

/// The three Linux text readers retain native bindings after their embedded
/// implementations exceeded the cumulative performance budget. The measured uptime
/// entry remains embedded, and `linux.modules` keeps its native stream.
#[cfg(target_os = "linux")]
#[test]
fn linux_text_entries_select_expected_bindings() {
    for (module, name) in [
        ("system", "memory"),
        ("system", "os_release"),
        ("linux", "meminfo"),
        ("linux", "modules"),
    ] {
        let overloads = xsh::api::api_spec()
            .module_overloads(module, name)
            .unwrap_or_else(|| panic!("{module}.{name} is in the standard API"));
        assert!(
            overloads.iter().all(|sig| sig.script_impl().is_none()),
            "{module}.{name} must use its native Linux implementation"
        );
    }
    let uptime = xsh::api::api_spec()
        .module_overloads("unix", "uptime_seconds")
        .expect("unix.uptime_seconds is in the standard API");
    assert!(
        uptime.iter().all(|sig| sig.script_impl().is_some()),
        "unix.uptime_seconds must retain its embedded Linux implementation"
    );
}

/// The public `system.os_release` entry against the fixed paths
/// `/etc/os-release` and `/usr/lib/os-release`.
///
/// The entry is not redirected for this test: it reads the two paths it always
/// reads. The Linux test container stages fixtures at those paths in its own
/// writable layer, outside shared mounts and without covering `/proc` or
/// `/sys`, and selects the scenario with `XSH_OS_RELEASE_SCENARIO`. The fixture
/// contents are committed under `tests/fixtures/stdlib/os_release/fixed-path/`:
///
/// - `etc` — the file is at both paths, so the first read answers;
/// - `fallback` — `/etc/os-release` points at a missing file, so the second
///   path answers;
/// - `neither` — `/etc/os-release` points at a missing file and `/usr/lib/os-release` is
///   a file that is not valid UTF-8, so the failure the whole call reports is
///   the *second* read's, carried under the entry's own kind. Asserting the
///   second read's message is what proves the first read failed and the second
///   was attempted, rather than the first failure being reported.
///
/// Without the variable, the test emits a skip so an unavailable fixture cannot
/// look like a pass. Ordinary runs leave it unset, and the fixtures are never
/// mounted over the container's real files.
#[cfg(target_os = "linux")]
#[test]
fn os_release_entry_reads_the_fixed_paths() {
    let Ok(scenario) = std::env::var("XSH_OS_RELEASE_SCENARIO") else {
        eprintln!(
            "skipped: XSH_OS_RELEASE_SCENARIO is not set, so the fixed-path fixtures are not \
             staged; run the container route recorded in bench/stdlib-port/README.md"
        );
        return;
    };

    // The script reads the entry the way any program does; it takes no path and
    // no override, so the paths it reads are the entry's own.
    let dir = temp_dir("os-release-fixed-path");
    let script = write_script(
        &dir,
        "release.xsh",
        concat!(
            "use system\n",
            "\n",
            "proc main() [io, env, fs, error] {\n",
            "  let release = system.os_release()?\n",
            "  print f\"{release.name}|{release.pretty_name}|{release.version}|{release.version_id}|{release.id}\"\n",
            "}\n",
        ),
    );

    let output = std::process::Command::new(release_bin!("xsh"))
        .arg(&script)
        .current_dir(&dir)
        .output()
        .expect("run the fixed-path release fixture");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    match scenario.as_str() {
        "etc" => {
            assert_eq!(output.status.code(), Some(0), "{stderr}");
            assert_eq!(
                stdout,
                "Fixture Etc|Fixture Etc \"quoted\"|1.0|7|fixture-etc-last\n"
            );
        }
        "fallback" => {
            assert_eq!(output.status.code(), Some(0), "{stderr}");
            assert_eq!(stdout, "Fixture Usr|Fixture Usr|2.0|8|fixture-usr\n");
        }
        "neither" => {
            // The second read's failure, under the entry's kind: the message is
            // the one only `/usr/lib/os-release` can produce.
            assert_ne!(output.status.code(), Some(0), "{stdout}");
            assert!(
                stderr.contains("err: system-os-release: stream did not contain valid UTF-8\n"),
                "{stderr}"
            );
        }
        other => panic!("unknown XSH_OS_RELEASE_SCENARIO `{other}`"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}
