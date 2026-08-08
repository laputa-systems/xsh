#![allow(clippy::single_call_fn)]

pub(crate) use std::collections::BTreeMap;
pub(crate) use std::io::{BufRead, BufReader, Read, Write};
pub(crate) use std::os::fd::FromRawFd;
pub(crate) use std::os::unix::ffi::{OsStrExt, OsStringExt};
pub(crate) use std::os::unix::fs::{MetadataExt, PermissionsExt};
pub(crate) use std::path::{Path, PathBuf};
pub(crate) use std::process::{Child, Command, Stdio};
pub(crate) use std::sync::OnceLock;
pub(crate) use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
pub(crate) use std::time::{Duration, Instant};
pub(crate) static PTY_TEST_LOCK: Mutex<()> = Mutex::new(());

struct WorkspaceBinaries {
    xshi: String,
    xsht: String,
}

static WORKSPACE_BINARIES: OnceLock<WorkspaceBinaries> = OnceLock::new();

// The root integration suite exercises all three products. Their binaries are
// package-owned, so build and resolve them from the active Cargo profile rather
// than depending on duplicate root-package targets.
pub(crate) fn workspace_binary(name: &str) -> &'static str {
    let binaries = WORKSPACE_BINARIES.get_or_init(build_workspace_binaries);
    match name {
        "xshi" => binaries.xshi.as_str(),
        "xsht" => binaries.xsht.as_str(),
        _ => panic!("unsupported workspace binary '{name}'"),
    }
}

fn build_workspace_binaries() -> WorkspaceBinaries {
    let profile_dir = std::env::current_exe()
        .expect("locate integration test executable")
        .parent()
        .and_then(Path::parent)
        .expect("locate Cargo profile directory")
        .to_path_buf();
    let profile = profile_dir
        .file_name()
        .and_then(|name| name.to_str())
        .expect("Cargo profile directory name");

    let mut command = Command::new("cargo");
    command
        .current_dir(cargo_env!("CARGO_MANIFEST_DIR"))
        .args(["build", "-p", "xshi", "-p", "xsht", "--bins"]);
    if profile != "debug" {
        command.args(["--profile", profile]);
    }
    let status = command.status().expect("build workspace product binaries");
    assert!(
        status.success(),
        "building workspace product binaries failed: {status}"
    );

    let binary = |name: &str| {
        let path = profile_dir.join(name);
        assert!(
            path.is_file(),
            "workspace binary is missing: {}",
            path.display()
        );
        path.to_str()
            .expect("workspace binary path is UTF-8")
            .to_owned()
    };
    WorkspaceBinaries {
        xshi: binary("xshi"),
        xsht: binary("xsht"),
    }
}

pub(crate) type JsonValue = miniserde::json::Value;

pub(crate) fn pty_test_guard() -> std::sync::MutexGuard<'static, ()> {
    PTY_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn xsh<const N: usize>(args: [&str; N]) -> std::process::Output {
    let mut cmd = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"));
    cmd.args(args);
    cmd.output().expect("run xsh")
}

pub(crate) fn xsht<const N: usize>(args: [&str; N]) -> std::process::Output {
    let mut cmd = Command::new(workspace_binary("xsht"));
    cmd.args(args);
    cmd.output().expect("run xsht")
}

pub(crate) fn json_parse(text: &str) -> JsonValue {
    miniserde::json::from_str(text).expect("parse JSON")
}

pub(crate) fn json_field<'a>(value: &'a JsonValue, key: &str) -> &'a JsonValue {
    match value {
        JsonValue::Object(fields) => fields.get(key).unwrap_or_else(|| panic!("missing {key}")),
        _ => panic!("expected JSON object"),
    }
}

pub(crate) fn json_index(value: &JsonValue, index: usize) -> &JsonValue {
    match value {
        JsonValue::Array(items) => &items[index],
        _ => panic!("expected JSON array"),
    }
}

pub(crate) fn json_array(value: &JsonValue) -> &miniserde::json::Array {
    match value {
        JsonValue::Array(items) => items,
        _ => panic!("expected JSON array"),
    }
}

pub(crate) fn json_str(value: &JsonValue) -> &str {
    match value {
        JsonValue::String(value) => value,
        _ => panic!("expected JSON string"),
    }
}

pub(crate) fn json_u64(value: &JsonValue) -> u64 {
    match value {
        JsonValue::Number(miniserde::json::Number::U64(value)) => *value,
        JsonValue::Number(miniserde::json::Number::I64(value)) => u64::try_from(*value).unwrap(),
        JsonValue::Number(miniserde::json::Number::F64(value))
            if *value >= 0.0 && value.fract() == 0.0 =>
        {
            *value as u64
        }
        _ => panic!("expected JSON u64"),
    }
}

pub(crate) fn json_bool(value: &JsonValue) -> bool {
    match value {
        JsonValue::Bool(value) => *value,
        _ => panic!("expected JSON bool"),
    }
}

pub(crate) fn run_cancelable_temp_script<const N: usize>(
    name: &str,
    source: &str,
    leading_args: [&str; N],
    ready: &std::path::Path,
    signal: i32,
) -> std::process::Output {
    let path = write_temp_script(name, source);
    let mut command = command_for_script_with_leading_args(&path, &leading_args);
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn script");

    wait_for_path(ready, Duration::from_secs(3), &mut child);
    std::thread::sleep(Duration::from_millis(50));
    let result = unsafe { libc::kill(child.id() as libc::pid_t, signal) };
    assert_eq!(result, 0);
    let status = wait_child_status(&mut child, Duration::from_secs(5));

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    child
        .stdout
        .take()
        .expect("child stdout")
        .read_to_end(&mut stdout)
        .expect("read child stdout");
    child
        .stderr
        .take()
        .expect("child stderr")
        .read_to_end(&mut stderr)
        .expect("read child stderr");

    let _ = std::fs::remove_file(path);
    std::process::Output {
        status,
        stdout,
        stderr,
    }
}

pub(crate) fn wait_for_path(
    path: &std::path::Path,
    timeout: Duration,
    child: &mut std::process::Child,
) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() {
            return;
        }
        if let Some(status) = child.try_wait().expect("poll child") {
            panic!("xsh exited before cancellation marker was written: {status}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = child.kill();
    let _ = child.wait();
    panic!("timed out waiting for cancellation marker");
}

pub(crate) fn wait_child_status(
    child: &mut std::process::Child,
    timeout: Duration,
) -> std::process::ExitStatus {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().expect("poll child") {
            return status;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = child.kill();
    let status = child.wait().expect("wait killed child");
    panic!("timed out waiting for canceled xsh process: {status}");
}

pub(crate) fn pstree_parent_child_order(stdout: &str, parent_pid: u32) -> Option<(usize, usize)> {
    let mut parent_line = None;
    let mut child_line = None;
    let parent_marker = format!("[{parent_pid}]");
    for (index, line) in stdout.lines().enumerate() {
        if line.contains(&parent_marker) {
            parent_line = Some(index);
        }
        if parent_line.is_some()
            && line.contains("sleep [")
            && (line.contains("├─sleep")
                || line.contains("└─sleep")
                || line.contains("|-sleep")
                || line.contains("`-sleep"))
        {
            child_line = Some(index);
        }
    }
    parent_line.zip(child_line)
}

pub(crate) fn run_temp_script(name: &str, source: &str) -> std::process::Output {
    run_temp_script_with_args(name, source, [])
}

pub(crate) fn run_temp_script_with_args<const N: usize>(
    name: &str,
    source: &str,
    leading_args: [&str; N],
) -> std::process::Output {
    let path = write_temp_script(name, source);
    let mut command = command_for_script_with_leading_args(&path, &leading_args);
    let output = command.output().expect("run script");
    let _ = std::fs::remove_file(path);
    output
}

pub(crate) fn run_temp_script_with_env<const N: usize>(
    name: &str,
    source: &str,
    leading_args: [&str; N],
    env: &[(&str, &str)],
) -> std::process::Output {
    let path = write_temp_script(name, source);
    let mut command = command_for_script_with_leading_args(&path, &leading_args);
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command.output().expect("run script");
    let _ = std::fs::remove_file(path);
    output
}

fn command_for_script_with_leading_args(path: &Path, leading_args: &[&str]) -> Command {
    let trace_args = translated_trace_args(leading_args);

    if let Some(args) = trace_args {
        let mut cmd = Command::new(workspace_binary("xsht"));
        cmd.arg("trace").args(args).arg(path);
        cmd
    } else {
        let mut cmd = Command::new(cargo_env!("CARGO_BIN_EXE_xsh"));
        cmd.args(leading_args).arg(path);
        cmd
    }
}

fn translated_trace_args(leading_args: &[&str]) -> Option<Vec<String>> {
    let has_trace_arg = leading_args.iter().any(|arg| {
        matches!(
            *arg,
            "--trace"
                | "--raw"
                | "--trace-format"
                | "--trace-file"
                | "--syscalls"
                | "--trace-top-syscalls"
        )
    });
    if !has_trace_arg {
        return None;
    }
    Some(
        leading_args
            .iter()
            .filter(|arg| **arg != "--trace")
            .map(|arg| (*arg).to_string())
            .collect(),
    )
}

pub(crate) fn run_path_target_script(name: &str, target: &std::path::Path) -> std::process::Output {
    run_temp_script(
        name,
        &format!(
            "run (Path({})) ?\n",
            xsh_string_literal(target.to_str().unwrap())
        ),
    )
}

pub(crate) fn write_temp_script(name: &str, source: &str) -> PathBuf {
    let path = temp_xsh_path(name);
    std::fs::write(&path, source).expect("write temp script");
    path
}

pub(crate) fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("xsh-{name}-{}", std::process::id()))
}

pub(crate) fn temp_xsh_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("xsh-{name}-{}.xsh", std::process::id()))
}

pub(crate) fn xsh_string_literal(text: &str) -> String {
    let mut quoted = String::from("\"");
    for ch in text.chars() {
        match ch {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            '\0' => quoted.push_str("\\0"),
            ch => quoted.push(ch),
        }
    }
    quoted.push('"');
    quoted
}

pub(crate) fn xsh_bytes_literal(bytes: &[u8]) -> String {
    let mut quoted = String::from("b\"");
    for byte in bytes {
        match byte {
            b'\\' => quoted.push_str("\\\\"),
            b'"' => quoted.push_str("\\\""),
            b'\n' => quoted.push_str("\\n"),
            b'\r' => quoted.push_str("\\r"),
            b'\t' => quoted.push_str("\\t"),
            b'\0' => quoted.push_str("\\0"),
            byte if byte.is_ascii_graphic() || *byte == b' ' => quoted.push(*byte as char),
            byte => quoted.push_str(&format!("\\x{byte:02x}")),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(target_os = "linux")]
pub(crate) fn unmount_linux(path: &std::path::Path) {
    match rustix::mount::unmount(path, rustix::mount::UnmountFlags::empty()) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("unmount linux real mount ({}): {error}", path.display(),);
        }
    }
}

pub(crate) fn write_test_tar_file(path: &std::path::Path, name: &str, data: &[u8]) {
    let mut file = std::fs::File::create(path).expect("create test tar");
    write_raw_tar_entry(&mut file, name, b'0', "", data);
    write_raw_tar_end(&mut file);
}

pub(crate) fn write_test_tar_symlink(path: &std::path::Path, name: &str, target: &str) {
    let mut file = std::fs::File::create(path).expect("create test tar");
    write_raw_tar_entry(&mut file, name, b'2', target, &[]);
    write_raw_tar_end(&mut file);
}

pub(crate) fn write_test_tar_hardlink(
    path: &std::path::Path,
    name: &str,
    link_name: &str,
    data: &[u8],
) {
    let mut file = std::fs::File::create(path).expect("create test tar");
    write_raw_tar_entry(&mut file, name, b'0', "", data);
    write_raw_tar_entry(&mut file, link_name, b'1', name, &[]);
    write_raw_tar_end(&mut file);
}

pub(crate) fn write_test_tar_global_header(path: &std::path::Path) {
    let mut file = std::fs::File::create(path).expect("create test tar");
    write_raw_tar_entry(
        &mut file,
        "pax_global_header",
        b'g',
        "",
        b"17 comment=hello\n",
    );
    write_raw_tar_entry(&mut file, "payload.txt", b'0', "", b"payload\n");
    write_raw_tar_end(&mut file);
}

pub(crate) fn write_test_zip(path: &std::path::Path, entries: &[(&str, &[u8])]) {
    assert_info_zip_30();
    let _ = std::fs::remove_file(path);
    let staging = path.with_extension("zip-src");
    let _ = std::fs::remove_dir_all(&staging);
    let base = staging.join("base");
    std::fs::create_dir_all(&base).expect("create zip fixture staging root");
    let mut names = Vec::new();
    for (name, data) in entries {
        let source = base.join(name);
        std::fs::create_dir_all(source.parent().expect("zip fixture parent"))
            .expect("create zip fixture parent");
        std::fs::write(&source, data).expect("write zip fixture data");
        names.push(*name);
    }
    let output = Command::new("/usr/bin/zip")
        .current_dir(&base)
        .arg("-q")
        .arg("-0")
        .arg(path)
        .args(names)
        .output()
        .expect("run /usr/bin/zip");
    assert!(
        output.status.success(),
        "/usr/bin/zip failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_dir_all(staging);
}

pub(crate) fn assert_info_zip_30() {
    let output = Command::new("/usr/bin/zip")
        .arg("-v")
        .output()
        .expect("run /usr/bin/zip -v");
    assert!(
        output.status.success(),
        "/usr/bin/zip -v failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut version = String::from_utf8(output.stdout).expect("zip -v utf8 stdout");
    version.push_str(&String::from_utf8(output.stderr).expect("zip -v utf8 stderr"));
    assert!(
        version.contains("Info-ZIP") && version.contains("This is Zip 3.0"),
        "/usr/bin/zip must be Info-ZIP 3.0, got:\n{version}"
    );
}

pub(crate) fn write_raw_tar_entry(
    file: &mut std::fs::File,
    name: &str,
    kind: u8,
    link: &str,
    data: &[u8],
) {
    let mut header = [0_u8; 512];
    write_tar_bytes(&mut header[0..100], name.as_bytes());
    write_tar_octal(&mut header[100..108], 0o644);
    write_tar_octal(&mut header[108..116], 0);
    write_tar_octal(&mut header[116..124], 0);
    write_tar_octal(&mut header[124..136], data.len() as u64);
    write_tar_octal(&mut header[136..148], 0);
    header[148..156].fill(b' ');
    header[156] = kind;
    write_tar_bytes(&mut header[157..257], link.as_bytes());
    write_tar_bytes(&mut header[257..263], b"ustar\0");
    write_tar_bytes(&mut header[263..265], b"00");
    let checksum = header.iter().map(|byte| u64::from(*byte)).sum::<u64>();
    write_tar_octal(&mut header[148..156], checksum);
    file.write_all(&header).expect("write tar header");
    file.write_all(data).expect("write tar data");
    let padding = (512 - (data.len() % 512)) % 512;
    file.write_all(&vec![0; padding])
        .expect("write tar padding");
}

pub(crate) fn write_raw_tar_end(file: &mut std::fs::File) {
    file.write_all(&[0; 1024]).expect("write tar end");
}

pub(crate) fn write_tar_bytes(field: &mut [u8], value: &[u8]) {
    let len = value.len().min(field.len());
    field[..len].copy_from_slice(&value[..len]);
}

pub(crate) fn write_tar_octal(field: &mut [u8], value: u64) {
    field.fill(0);
    let text = format!("{:0width$o}", value, width = field.len() - 1);
    let bytes = text.as_bytes();
    let start = field.len().saturating_sub(1 + bytes.len());
    field[start..start + bytes.len()].copy_from_slice(bytes);
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

pub(crate) fn stdout_text(output: &std::process::Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout UTF-8")
}

pub(crate) fn stderr_text(output: &std::process::Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr UTF-8")
}

pub(crate) fn assert_ok(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "status={:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        stdout_text(output),
        stderr_text(output)
    );
}

pub(crate) fn assert_exit(output: &std::process::Output, code: i32) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "stdout:\n{}\nstderr:\n{}",
        stdout_text(output),
        stderr_text(output)
    );
}

pub(crate) fn assert_stderr_contains(output: &std::process::Output, needle: &str) {
    let stderr = stderr_text(output);
    assert!(stderr.contains(needle), "{stderr}");
}
