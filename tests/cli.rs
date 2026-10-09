#![allow(clippy::single_call_fn)]

use std::os::unix::ffi::OsStringExt;
use std::process::Command;
use std::{env, fs};

#[test]
fn xsh_passes_script_args_without_separator() {
    let path = temp_script(
        "xsh-argv-no-separator",
        "for arg in args {\n  print ${arg}\n}\n",
    );
    let output = Command::new(release_bin!("xsh"))
        .args([path.to_str().unwrap(), "-f", "needle"])
        .output()
        .expect("run xsh script");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "-f\nneedle\n");
}

#[test]
fn xsh_passes_double_dash_after_script_to_script_args() {
    let path = temp_script(
        "xsh-argv-double-dash",
        "for arg in args {\n  print ${arg}\n}\n",
    );
    let output = Command::new(release_bin!("xsh"))
        .args([path.to_str().unwrap(), "--", "-f", "needle"])
        .output()
        .expect("run xsh script");

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "--\n-f\nneedle\n");
}

#[test]
fn xsh_reports_non_utf8_script_argument_without_panicking() {
    let path = temp_script("xsh-non-utf8-argv", "print \"ready\"\n");
    let raw_arg = std::ffi::OsString::from_vec(b"raw\xffarg".to_vec());
    let output = Command::new(release_bin!("xsh"))
        .arg(path)
        .arg(raw_arg)
        .output()
        .expect("run xsh script");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "xsh: argument 2 is not valid UTF-8\n"
    );
}

#[test]
fn xsh_runs_dynamic_record_methods_by_default() {
    let path = temp_script("xsh-dynamic-lower-default", dynamic_lowerability_script());
    let output = Command::new(release_bin!("xsh"))
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
    let output = Command::new(release_bin!("xsh"))
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
    "proc main(...argv: List[Str]) -> Result[Unit] {
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

#[test]
fn signature_cli_preflight_precedes_imported_and_entry_initializers() {
    let root = env::temp_dir().join(format!("xsh-signature-preflight-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let imported_marker = root.join("imported-marker");
    let entry_marker = root.join("entry-marker");
    let module = format!(
        "##! Initializer marker module.\n## An unsigned worker count.\nexport type WorkerCount = UInt\nproc initialize() [fs, error] -> Int {{ fs.write(p\"{}\", \"ran\")?; 1 }}\nlet initialized_marker = initialize()\n## A callable exported value.\nexport pure value() -> Int {{ initialized_marker }}\n",
        imported_marker.display(),
    );
    fs::write(root.join("marker.xsh"), module).unwrap();
    let source = format!(
        "##! A checked signature CLI.\nuse marker\nproc initialize() [fs, error] -> Int {{ fs.write(p\"{}\", \"ran\")?; 1 }}\nlet initialized = initialize()\ncli main(root: Path, jobs: marker.WorkerCount = 4) [error] {{ print ${{marker.value()}} $initialized $jobs }}\n",
        entry_marker.display(),
    );
    let script = root.join("entry.xsh");
    fs::write(&script, source).unwrap();
    for (arguments, expected_status) in [
        (vec!["--help"], 0),
        (vec![], 2),
        (vec!["operand", "--jobs=nope"], 2),
        (vec!["operand", "--jobs=-1"], 2),
    ] {
        let output = Command::new(release_bin!("xsh"))
            .arg(&script)
            .args(arguments)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(expected_status),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!imported_marker.exists());
        assert!(!entry_marker.exists());
        let usage = if expected_status == 0 {
            &output.stdout
        } else {
            &output.stderr
        };
        assert!(String::from_utf8_lossy(usage).contains("usage:"));
    }
    let output = Command::new(release_bin!("xsh"))
        .arg(&script)
        .args(["missing-path-is-allowed", "--jobs=8"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"1 1 8\n");
    assert!(imported_marker.exists());
    assert!(entry_marker.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn signature_cli_preserves_entry_exit_status_and_errors() {
    for (body, effects, expected_status) in [
        ("exit 7", "error", 7),
        ("error.fail(\"entry failed\")?", "error", 3),
    ] {
        let script = temp_script(
            &format!("xsh-signature-status-{expected_status}"),
            &format!("cli main() [{effects}] {{ {body} }}\n"),
        );
        let output = Command::new(release_bin!("xsh"))
            .arg(&script)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(expected_status),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::remove_file(script).unwrap();
    }
}
