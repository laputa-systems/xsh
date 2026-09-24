//! The migrated-API cases behave identically across the supported build profiles.
//!
//! The same script is run through each available `xsh` binary and the bytes of
//! stdout, stderr, and the exit status are compared across debug/release and
//! default/no-default-feature builds. The script is
//! built from the cases this port is most likely to break by re-encoding
//! something: newlines inside values, non-ASCII text and escapes, path bytes,
//! and the exact kind and message of a rejected call.
//!
//! Build the extra binaries beside the debug one to widen the comparison:
//!
//! ```sh
//! cargo build --release -p xsh --bin xsh
//! CARGO_TARGET_DIR=target/no-default cargo build -p xsh --bin xsh --no-default-features
//! CARGO_TARGET_DIR=target/no-default cargo build --release -p xsh --bin xsh --no-default-features
//! ```
//!
//! On Linux, use `CARGO_TARGET_DIR=target/no-default-linux` for the two
//! no-default-feature builds. Build and run all products in Dockerfile.test with
//! `--target aarch64-unknown-linux-musl` and the flags from
//! `dev/targets.xsh::docker_test_env`. A missing alternative is reported as a
//! skip with its exact build command; a missing debug baseline is an error.

use std::io::Write;
use std::process::Command;

fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn target_paths(
    root: &std::path::Path,
) -> Option<(std::path::PathBuf, std::path::PathBuf, &'static str)> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some((
            root.join("target"),
            root.join("target/no-default"),
            "",
        )),
        ("linux", "aarch64") => Some((
            root.join("target/aarch64-unknown-linux-musl"),
            root.join("target/no-default-linux/aarch64-unknown-linux-musl"),
            " --target aarch64-unknown-linux-musl",
        )),
        _ => None,
    }
}

/// The cases, as one script. Every line prints something whose bytes depend on
/// a newline, a non-ASCII scalar, a path, or an error the port composes.
const SCRIPT: &str = r#"proc main() [io, error, time] {
  fs.write(p"parity.txt", "abc")?
  print shlex.quote("two words")
  print shlex.quote("""a
b""")
  print shlex.join(["install", "two words", "can't"])
  print shlex.quote("h\u{e9}llo")
  print tui.left_pad("plain", 8)
  print tui.left_pad("\x1b[31mred\x1b[0m", 8)
  print tui.left_pad("wide 日本", 10)
  print bytes.human(0)
  print bytes.human(1024)
  print bytes.human(1536)
  print bytes.human(-1)
  print time.duration_compact(0)
  print time.duration_compact(3661)
  print time.duration_compact(90061)
  let digest = hash.sha256(p"parity.txt")?
  print digest.hex()
  hash.verify_file(p"parity.txt", sha256: digest.hex())?
  match hash.verify_file(p"parity.txt", sha256: "00") {
    Ok(_) => { print "unexpected" }
    Err(failure) => { print f"${failure.message}" }
  }
  let line = hash.parse_check_line(f"${digest.hex()}  build/out.bin")?
  print f"${line.hex} ${line.path} ${line.binary}"
  match hash.parse_check_line("nope") {
    Ok(_) => { print "unexpected" }
    Err(failure) => { print f"${failure.message}" }
  }
  let txt = mime.lookup_ext("txt") ?? {mime: "none", exts: []}
  print f"${txt.mime}"
  let unknown = mime.lookup_ext("nope") ?? {mime: "none", exts: []}
  print f"${unknown.mime}"
  match ini.encode({section: {key: "value"}}) {
    Ok(text) => { print f"${text}" }
    Err(failure) => { print f"${failure.message}" }
  }
  match json.get({a: {b: 1}}, ["a", "b"]) {
    Ok(found) => { print f"${found}" }
    Err(failure) => { print f"${failure.message}" }
  }
  match json.get({a: 1}, ["missing"]) {
    Ok(_) => { print "unexpected" }
    Err(failure) => { print f"${failure.message}" }
  }
  match json.encode_lines([{a: 1}, "two words"]) {
    Ok(text) => { print f"${text}" }
    Err(failure) => { print f"${failure.message}" }
  }
  print env.get_or("XSH_PARITY_UNSET_VARIABLE", "fallback")?
  fs.remove(p"parity.txt")?
}
"#;

fn run(binary: &std::path::Path, script: &std::path::Path) -> (i32, Vec<u8>, Vec<u8>) {
    let output = Command::new(binary)
        .arg(script)
        .current_dir(script.parent().expect("script directory"))
        .output()
        .unwrap_or_else(|error| panic!("run {}: {error}", binary.display()));
    (
        output.status.code().unwrap_or(-1),
        output.stdout,
        output.stderr,
    )
}

#[test]
fn one_case_set_behaves_identically_across_supported_builds() {
    let root = workspace_root();
    let Some((default_dir, no_default_dir, target_arg)) = target_paths(&root) else {
        eprintln!(
            "skipped profile parity: unsupported host {} / {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        return;
    };
    let (no_default_target_dir, build_context) = if target_arg.is_empty() {
        ("target/no-default", "")
    } else {
        (
            "target/no-default-linux",
            "inside Dockerfile.test with dev/targets.xsh::docker_test_env flags: ",
        )
    };
    let debug = default_dir.join("debug/xsh");
    assert!(
        debug.is_file(),
        "missing debug baseline: {}. Build {build_context}cargo build -p xsh --bin xsh{target_arg}",
        debug.display()
    );
    let alternatives = [
        (
            "release",
            default_dir.join("release/xsh"),
            format!("cargo build --release -p xsh --bin xsh{target_arg}"),
        ),
        (
            "no-default-features",
            no_default_dir.join("debug/xsh"),
            format!(
                "CARGO_TARGET_DIR={no_default_target_dir} cargo build -p xsh --bin xsh --no-default-features{target_arg}"
            ),
        ),
        (
            "release-no-default-features",
            no_default_dir.join("release/xsh"),
            format!(
                "CARGO_TARGET_DIR={no_default_target_dir} cargo build --release -p xsh --bin xsh --no-default-features{target_arg}"
            ),
        ),
    ];
    let mut compared = vec![("debug", debug.clone())];
    for (name, binary, command) in alternatives {
        if binary.is_file() {
            compared.push((name, binary));
        } else {
            eprintln!(
                "skipped {name}: {} is not built; build {build_context}{command}",
                binary.display()
            );
        }
    }

    let dir = std::env::temp_dir().join(format!(
        "xsh-profile-parity-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let script = dir.join("parity.xsh");
    let mut file = std::fs::File::create(&script).expect("create script");
    file.write_all(SCRIPT.as_bytes()).expect("write script");
    drop(file);

    let runs = compared
        .iter()
        .map(|(name, binary)| (*name, run(binary, &script)))
        .collect::<Vec<_>>();
    let _ = std::fs::remove_dir_all(&dir);

    let (baseline_name, baseline) = &runs[0];
    assert!(
        baseline.1.starts_with(b"'two words'\n"),
        "the script must have produced its first line: {}",
        String::from_utf8_lossy(&baseline.1)
    );
    assert_eq!(baseline.0, 0, "debug baseline exited with an error");
    assert!(baseline.2.is_empty(), "debug baseline wrote to stderr");

    for (name, candidate) in &runs[1..] {
        let context = |field: &str| {
            format!(
                "{field} differs between {baseline_name} and {name}\n-- {baseline_name} stdout --\n{}\n-- {name} stdout --\n{}\n-- {baseline_name} stderr --\n{}\n-- {name} stderr --\n{}",
                String::from_utf8_lossy(&baseline.1),
                String::from_utf8_lossy(&candidate.1),
                String::from_utf8_lossy(&baseline.2),
                String::from_utf8_lossy(&candidate.2),
            )
        };
        assert_eq!(
            baseline.0,
            candidate.0,
            "exit status differs / {}",
            context("exit status")
        );
        assert_eq!(baseline.1, candidate.1, "{}", context("stdout"));
        assert_eq!(baseline.2, candidate.2, "{}", context("stderr"));
    }
}
