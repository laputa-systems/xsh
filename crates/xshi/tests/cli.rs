use std::os::unix::ffi::OsStringExt;
use std::process::Command;

#[test]
fn xshi_reports_non_utf8_command_without_panicking() {
    let raw_command = std::ffi::OsString::from_vec(b"print \"\xff\"".to_vec());
    let output = Command::new(env!("CARGO_BIN_EXE_xshi"))
        .arg("-c")
        .arg(raw_command)
        .output()
        .expect("run xshi");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "xshi: argument 2 is not valid UTF-8\n"
    );
}
