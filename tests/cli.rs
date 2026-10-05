#![allow(clippy::single_call_fn)]

use std::os::unix::ffi::OsStringExt;
use std::process::Command;
use std::{env, fs};

// An argument that is not UTF-8 cannot be written as an XSH string, so this
// argv is built here; the other `xsh` command-line cases are native tests.
#[test]
fn xsh_reports_non_utf8_script_argument_without_panicking() {
    let dir = env::temp_dir().join(format!("xsh-non-utf8-argv-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("create temp script dir");
    let path = dir.join("main.xsh");
    fs::write(&path, "print \"ready\"\n").expect("write temp script");
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
