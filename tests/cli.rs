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

// The host argv and stdout protocols preserve invalid UTF-8 and empty words.
#[test]
fn xsh_byte_main_receives_exact_os_arguments() {
    let root = tempfile::tempdir().expect("create raw argv script directory");
    let script = root.path().join("bytes.xsh");
    fs::write(
        &script,
        r#"proc main(...argv: List[Bytes]) [io, error] {
  for word in argv {
    io.write_stdout_bytes(word)?
    io.write_stdout_bytes(b"\0")?
  }
}
"#,
    )
    .expect("write byte main");
    let output = Command::new(release_bin!("xsh"))
        .arg(&script)
        .args([
            std::ffi::OsString::from_vec(b"raw\xffarg".to_vec()),
            std::ffi::OsString::from("é"),
            std::ffi::OsString::new(),
            std::ffi::OsString::from("--literal"),
        ])
        .output()
        .expect("run byte main");

    assert_eq!(output.status.code(), Some(0), "{:?}", output.stderr);
    assert_eq!(output.stdout, b"raw\xffarg\0\xc3\xa9\0\0--literal\0");
    assert!(output.stderr.is_empty());
    let empty = Command::new(release_bin!("xsh"))
        .arg(&script).output().expect("run byte main without arguments");
    assert_eq!(empty.status.code(), Some(0), "{:?}", empty.stderr);
    assert!(empty.stdout.is_empty());
    assert!(empty.stderr.is_empty());
    let separator = Command::new(release_bin!("xsh"))
        .arg("--").arg(&script).arg("--")
        .arg(std::ffi::OsString::from_vec(b"raw\xff".to_vec()))
        .output().expect("pass literal option separator to byte main");
    assert_eq!(separator.status.code(), Some(0), "{:?}", separator.stderr);
    assert_eq!(separator.stdout, b"--\0raw\xff\0");
    assert!(separator.stderr.is_empty());
}

#[test]
fn xsh_text_main_rejects_invalid_utf8_before_top_level_effects() {
    let root = tempfile::tempdir().expect("create text argv script directory");
    let script = root.path().join("text.xsh");
    fs::write(
        &script,
        "print \"top-level ran\"\nproc main(...argv: List[Str]) { print argv.len() }\n",
    )
    .expect("write text main");
    let output = Command::new(release_bin!("xsh"))
        .arg(&script)
        .arg(std::ffi::OsString::from_vec(b"raw\xffarg".to_vec()))
        .output()
        .expect("run text main");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, b"xsh: argument 2 is not valid UTF-8\n");
}

#[test]
fn xsh_byte_main_context_is_shared_by_imported_argument_readers() {
    let root = tempfile::tempdir().expect("create byte argument module directory");
    fs::write(
        root.path().join("words.xsh"),
        "##! Read the entry's byte arguments.\n## Return the unchanged arguments.\nexport pure incoming() -> List[Bytes] { args }\n",
    )
    .expect("write argument reader");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "use words\ntype ByteWords = List[Bytes]\nproc main(...argv: ByteWords) [io, error] {\n  assert args == argv\n  assert words.incoming() == argv\n  io.write_stdout_bytes(argv[0])?\n}\n",
    )
    .expect("write byte argument entry");
    let output = Command::new(release_bin!("xsh"))
        .arg(&script)
        .arg(std::ffi::OsString::from_vec(b"argument\xff".to_vec()))
        .current_dir(root.path())
        .output()
        .expect("run byte argument entry");

    assert_eq!(output.status.code(), Some(0), "{:?}", output.stderr);
    assert_eq!(output.stdout, b"argument\xff");
    assert!(output.stderr.is_empty());
}

#[test]
fn xsh_byte_main_opens_non_utf8_path_without_display_conversion() {
    let root = tempfile::tempdir().expect("create raw path script directory");
    let target = root.path().join(std::ffi::OsString::from_vec(b"file\xff".to_vec()));
    fs::write(&target, b"payload\0\xff\n").expect("write raw path fixture");
    let script = root.path().join("read.xsh");
    fs::write(
        &script,
        "proc main(...argv: List[Bytes]) [fs, io, error] {\n  let target = Path.parse_bytes(argv[0])?\n  io.write_stdout_bytes(target.read_bytes()?)?\n}\n",
    )
    .expect("write raw path entry");
    let output = Command::new(release_bin!("xsh"))
        .arg(&script)
        .arg(&target)
        .output()
        .expect("run raw path entry");

    assert_eq!(output.status.code(), Some(0), "{:?}", output.stderr);
    assert_eq!(output.stdout, b"payload\0\xff\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn xsh_rejects_mixed_byte_entry_signature_before_effects() {
    let root = tempfile::tempdir().expect("create mixed byte entry directory");
    let script = root.path().join("mixed.xsh");
    fs::write(
        &script,
        "print \"must not run\"\nproc main(prefix: Str, ...argv: List[Bytes]) { print argv.len() }\n",
    )
    .expect("write unsupported byte entry");
    let output = Command::new(release_bin!("xsh"))
        .arg(&script)
        .arg("prefix")
        .arg("operand")
        .output()
        .expect("run unsupported byte entry");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8(output.stderr).unwrap().contains("single spread parameter"));
}

#[test]
fn xsh_byte_main_explicit_dispatch_uses_the_same_contextual_arguments() {
    let root = tempfile::tempdir().expect("create explicit byte entry directory");
    let script = root.path().join("explicit.xsh");
    fs::write(
        &script,
        "proc main(...argv: List[Bytes]) [io, error] { io.write_stdout_bytes(argv[0])? }\nmain(@args)?\n",
    )
    .expect("write explicit byte entry");
    let output = Command::new(release_bin!("xsh"))
        .arg(&script)
        .arg(std::ffi::OsString::from_vec(b"explicit\xff".to_vec()))
        .output()
        .expect("run explicit byte entry");

    assert_eq!(output.status.code(), Some(0), "{:?}", output.stderr);
    assert_eq!(output.stdout, b"explicit\xff");
    assert!(output.stderr.is_empty());
}

#[test]
fn xsh_text_entry_imports_keep_their_text_argument_contract() {
    let root = tempfile::tempdir().expect("create text argument module directory");
    fs::write(
        root.path().join("words.xsh"),
        "##! Read the entry's text arguments.\n## Return the unchanged arguments.\nexport pure incoming() -> List[Str] { args }\n",
    )
    .expect("write text argument reader");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "use words\nproc main(...argv: List[Str]) { assert words.incoming() == argv; print argv[0] }\n",
    )
    .expect("write text argument entry");
    let output = Command::new(release_bin!("xsh"))
        .arg(&script)
        .arg("text-é")
        .current_dir(root.path())
        .output()
        .expect("run text argument entry");

    assert_eq!(output.status.code(), Some(0), "{:?}", output.stderr);
    assert_eq!(output.stdout, "text-é\n".as_bytes());
    assert!(output.stderr.is_empty());
}

#[test]
fn xsht_trace_binds_utf8_words_as_bytes_for_a_byte_entry() {
    let root = tempfile::tempdir().expect("create traced byte entry directory");
    let script = root.path().join("traced.xsh");
    fs::write(
        &script,
        "proc main(...argv: List[Bytes]) { assert argv[0] == b\"value\"; assert args == argv }\n",
    )
    .expect("write traced byte entry");
    let xsht = std::path::Path::new(release_bin!("xsh")).with_file_name("xsht");
    assert!(xsht.is_file(), "build the sibling release xsht binary");
    let output = Command::new(xsht)
        .arg("trace")
        .arg(&script)
        .args(["--", "value"])
        .output()
        .expect("trace byte entry");

    assert_eq!(output.status.code(), Some(0), "{:?}", output.stderr);
}

#[test]
fn xsh_byte_entry_rejects_imported_readers_with_a_text_argument_contract() {
    let root = tempfile::tempdir().expect("create incompatible argument reader directory");
    fs::write(
        root.path().join("words.xsh"),
        "##! Read text arguments.\n## Return arguments as text.\nexport pure incoming() -> List[Str] { args }\n",
    )
    .expect("write incompatible reader");
    let script = root.path().join("main.xsh");
    fs::write(
        &script,
        "use words\nprint \"must not run\"\nproc main(...argv: List[Bytes]) { print words.incoming().len() }\n",
    )
    .expect("write byte entry with incompatible reader");
    let output = Command::new(release_bin!("xsh"))
        .arg(&script)
        .arg(std::ffi::OsString::from_vec(b"word\xff".to_vec()))
        .current_dir(root.path())
        .output()
        .expect("check incompatible argument reader");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 check diagnostic");
    assert!(stderr.contains("List[Str]") && stderr.contains("List[Bytes]"), "{stderr}");
}
