#![allow(clippy::single_call_fn)]

use std::env;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;

fn command_output(output: &Output) -> String {
    format!(
        "status: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn target_debug_dir() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_xsh"))
        .parent()
        .expect("xsh binary has parent directory")
}

fn path_with_target_debug() -> String {
    let system_paths = [PathBuf::from("/bin"), PathBuf::from("/usr/bin")];
    let current: Vec<PathBuf> = env::var_os("PATH")
        .map(|path| env::split_paths(&path).collect())
        .unwrap_or_default();
    let mut paths = system_paths.to_vec();
    paths.push(target_debug_dir().to_path_buf());
    for path in current {
        if !paths.contains(&path) && !system_paths.contains(&path) {
            paths.push(path);
        }
    }

    env::join_paths(paths)
        .expect("PATH entries join")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn core_tests_run_from_core_directory() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_xsht"));
    command
        .arg("test")
        .current_dir("core")
        .env("PATH", path_with_target_debug());
    // The `host` applet needs the `net` feature; tell its test to skip itself
    // when xsh was built without net so the core suite still passes.
    if !cfg!(feature = "net") {
        command.env("XSH_SKIP_NET_TESTS", "1");
    }
    let output = command.output().expect("run xsht test from core");

    assert!(output.status.success(), "{}", command_output(&output));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("test tests/test-basename.xsh::test_basename_basic ... ok"));
    assert!(
        stdout.contains("test tests/test-basename.xsh::test_basename_suffix_and_multiple ... ok")
    );
    assert!(stdout.contains("test result: ok."));
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn core_basename_runs_as_executable_shebang_script() {
    let script = Path::new("core/basename.xsh");
    let mut permissions = std::fs::metadata(script)
        .expect("basename metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(script, permissions).expect("chmod basename");

    let output = Command::new(script)
        .arg("/tmp/demo.txt")
        .env("PATH", path_with_target_debug())
        .output()
        .expect("run basename executable");

    assert!(output.status.success(), "{}", command_output(&output));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "demo.txt\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}
