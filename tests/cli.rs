#![allow(clippy::single_call_fn)]

use std::process::Command;
use std::{env, fs};

#[test]
fn xsh_passes_script_args_without_separator() {
    let path = temp_script(
        "xsh-argv-no-separator",
        "for arg in args {\n  print ${arg}\n}\n",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .args([path.to_str().unwrap(), "-f", "needle"])
        .output()
        .expect("run xsh script");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "-f\nneedle\n");
}

#[test]
fn xsh_keeps_separator_compatibility_for_script_args() {
    let path = temp_script(
        "xsh-argv-with-separator",
        "for arg in args {\n  print ${arg}\n}\n",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .args([path.to_str().unwrap(), "--", "-f", "needle"])
        .output()
        .expect("run xsh script");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "-f\nneedle\n");
}

#[test]
fn xsh_runs_dynamic_record_methods_by_default() {
    let path = temp_script("xsh-dynamic-lower-default", dynamic_lowerability_script());
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .arg(path.to_str().unwrap())
        .output()
        .expect("run xsh script");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "non-empty\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn xsh_rejects_removed_strict_lower_option() {
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .arg("--strict-lower")
        .output()
        .expect("run xsh");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unknown xsh option '--strict-lower'"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn dynamic_lowerability_script() -> &'static str {
    "proc main(...argv: List[Str]) [error] -> Result[Unit] {
  let exports: Record = {sources: {name: \"demo\"}}
  let sources = exports.get(\"sources\")?

  if sources.len() != 0 {
    print \"non-empty\"
  }

  return Ok()
}
"
}

fn temp_script(name: &str, source: &str) -> std::path::PathBuf {
    let dir = env::temp_dir().join(format!("{name}-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("create temp script dir");
    let path = dir.join("main.xsh");
    fs::write(&path, source).expect("write temp script");
    path
}
