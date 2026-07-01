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

fn temp_script(name: &str, source: &str) -> std::path::PathBuf {
    let dir = env::temp_dir().join(format!("{name}-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("create temp script dir");
    let path = dir.join("main.xsh");
    fs::write(&path, source).expect("write temp script");
    path
}
