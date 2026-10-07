test test_unix_cpu_features_are_named {
  let known = ["avx512", "avx2", "pclmul", "sse2", "asimd", "vmull"]
  for feature in unix.cpu_features() { assert feature in known }
}

test test_unix_exec_preserves_command_redirections_and_cwd { |ctx|
  let root = test.temp_dir(ctx, name: "unix-exec-redirections")?
  let input = fp"{root}/input"
  let output = fp"{root}/output"
  let errors = fp"{root}/errors"
  input.write("input")
  output.write("prefix")
  let result = test.expect(ctx, r"""let root = Path(args[0])
let command = process.command_argv("sh", ["sh", "-c", "cat; printf err >&2; test \"$PWD\" = \"$1\"", "sh", root.display()], cwd: root, stdin: fp"{root}/input", stdout: fp"{root}/output", stdout_append: true, stderr: fp"{root}/errors")
unix.exec(command)?
""", status: 0, args: [root])?
  assert result.stdout == ""
  assert result.stderr == ""
  assert output.read_text()? == "prefixinput"
  assert errors.read_text()? == "err"
}

test test_unix_exec_bytes_input_has_no_surviving_writer { |ctx|
  let result = test.expect(ctx, r"""let command = process.command_argv("cat", ["cat"], stdin: bytes.from_text("bytes-input"))
unix.exec(command)?
""", status: 0)?
  assert result.stdout == "bytes-input"
}

test test_unix_descriptor_arguments_validate_before_opening { |ctx|
  let root = test.temp_dir(ctx, name: "unix-fd-validation")?
  let unopened = fp"{root}/must-not-exist"
  test.error_kind(unix.redirect_fd(-1, unopened, write: true), "unix-redirect-fd")
  test.error_kind(unix.redirect_fd(2147483648, unopened, write: true), "unix-redirect-fd")
  test.error_kind(unix.redirect_fd(1, unopened, append: true), "unix-redirect-fd")
  test.error_kind(unix.redirect_fd(1, unopened, write: true, mode: -1), "unix-redirect-fd")
  test.error_kind(unix.redirect_fd(1, unopened, write: true, mode: 4096), "unix-redirect-fd")
  assert ! unopened.exists()?
  test.error_kind(unix.dup_fd(-1, 1), "unix-dup-fd")
  test.error_kind(unix.dup_fd(1, 2147483648), "unix-dup-fd")
  assert unix.dup_fd(2147483647, 2147483647) is Err(is HostIo)
}

test test_unix_redirect_append_private_mode_and_dup_survive_exec { |ctx|
  let root = test.temp_dir(ctx, name: "unix-fd-append")?
  let log = fp"{root}/log"
  for text in ["first", "second"] {
    let result = test.expect(ctx, r"""unix.redirect_fd(1, Path(args[0]), write: true, append: true, mode: 384)?
unix.dup_fd(1, 2)?
unix.dup_fd(2, 2)?
unix.exec(process.command_argv("sh", ["sh", "-c", "printf %s \"$1\"; printf err >&2", "sh", args[1]]))?
""", status: 0, args: [log, text])?
    assert result.stdout == "" and result.stderr == ""
  }
  assert log.read_text()? == "firsterrseconderr"
  assert log.metadata()?.mode % 512 == 384
}

test test_unix_redirect_read_and_write_only_stdin { |ctx|
  let root = test.temp_dir(ctx, name: "unix-fd-stdin")?
  let input = fp"{root}/input"
  input.write("readable")
  let read = test.expect(ctx, r"""unix.redirect_fd(0, Path(args[0]))?
unix.exec(process.command_argv("cat", ["cat"]))?
""", status: 0, args: [input])?
  assert read.stdout == "readable"
  let refused = test.run_script(ctx, r"""unix.redirect_fd(0, /dev/null, write: true)?
unix.exec(process.command_argv("cat", ["cat"]))?
""")?
  assert ! refused.success
  assert "Bad file descriptor" in refused.stderr
}

test test_unix_credentials_validate_all_ids_before_transition {
  let before = unix.id()?
  test.error_kind(unix.set_uid(-1), "unix-set-uid")
  test.error_kind(unix.set_uid(4294967295), "unix-set-uid")
  test.error_kind(unix.set_gid(-1), "unix-set-gid")
  test.error_kind(unix.set_gid(4294967295), "unix-set-gid")
  test.error_kind(unix.set_groups([-1]), "unix-set-groups")
  test.error_kind(unix.set_groups([4294967295]), "unix-set-groups")
  test.error_kind(unix.set_credentials(uid: -1, gid: before.gid, groups: []), "unix-set-credentials")
  test.error_kind(unix.set_credentials(uid: before.uid, gid: 4294967295, groups: []), "unix-set-credentials")
  test.error_kind(unix.set_credentials(uid: before.uid, gid: before.gid, groups: [-1]), "unix-set-credentials")
  assert unix.id()? == before
}

test test_unix_credentials_transition_inside_namespace { |ctx|
  guard system.uname()?.sysname == "Linux" and unix.id()?.euid == 0 else {
    test.skip("credential transitions require root inside a Linux namespace")
    return
  }
  let unshare = match process.which("unshare") {
    Ok(executable) => executable
    Err(_) => {
      test.skip("unshare is unavailable")
      return
    }
  }
  let probe = process.run(process.command_argv(unshare, [unshare.display(), "--mount", "--pid", "--fork", "--", "true"]))?
  guard probe.ok else {
    test.skip("private mount and PID namespaces are unavailable")
    return
  }
  let script = test.temp_file(ctx, name: "credentials.xsh", contents: bytes.from_text(r"""unix.set_groups([123, 456])?
let original = unix.id()?
unix.set_gid(12346)?
let gid_changed = unix.id()?
assert gid_changed.gid == 12346 and gid_changed.egid == 12346
assert gid_changed.uid == original.uid and gid_changed.euid == original.euid
unix.set_uid(0)?
let before = unix.id()?
assert (before.groups |> any { |group| group.gid == 123 })
assert (before.groups |> any { |group| group.gid == 456 })
unix.set_credentials(uid: 12345, gid: 12346, groups: [12347])?
let after = unix.id()?
assert after.uid == 12345 and after.euid == 12345
assert after.gid == 12346 and after.egid == 12346
assert (after.groups |> any { |group| group.gid == 12347 })
assert ! (after.groups |> any { |group| group.gid == 123 })
assert ! (after.groups |> any { |group| group.gid == 456 })
print "credentials-changed"
"""))?
  let output = test.temp_path(ctx, name: "credentials.stdout")
  let errors = test.temp_path(ctx, name: "credentials.stderr")
  let status = process.run(process.command_argv(unshare, [unshare.display(), "--mount", "--pid", "--fork", "--", ctx.xsh_bin.display(), script.display()], stdout: output, stderr: errors))?
  assert status.ok, errors.read_text()?
  assert output.read_text()? == "credentials-changed\n"
}


test test_unix_identity_does_not_require_a_group_database { |ctx|
  if system.uname()?.sysname != "Linux" or unix.id()?.uid != 0 {
    test.skip("chroot needs Linux root privileges")
  }
  let jail = test.temp_dir(ctx)?
  let output = test.run_xsh(ctx, f"linux.chroot(fp\"{jail}\")?; assert unix.id() is Ok(_)")?
  assert output.success, output.stderr
}

test test_unix_poll_fd_validates_requests {
  test.error_kind(unix.poll_fd(-1, ["readable"]), "unix-poll-fd")
  test.error_kind(unix.poll_fd(2147483648, ["readable"]), "unix-poll-fd")
  test.error_kind(unix.poll_fd(0, ["unknown"]), "unix-poll-fd")
  test.error_kind(unix.poll_fd(0, ["hangup"]), "unix-poll-fd")
  test.error_kind(unix.poll_fd(0, ["error"]), "unix-poll-fd")
  test.error_kind(unix.poll_fd(0, ["invalid"]), "unix-poll-fd")
  test.error_kind(unix.poll_fd(0, [], timeout_ms: -2), "unix-poll-fd")
  assert unix.poll_fd(2147483647, ["readable", "writable"])? == ["invalid"]
}

test test_unix_poll_fd_regular_file_ready_and_closed { |ctx|
  let file = test.temp_file(ctx, contents: b"payload")?
  let result = test.run_script(ctx, r"""
let fd = unix.open_fd(Path(args[0]))?
assert unix.poll_fd(fd, ["readable"], timeout_ms: -1)? == ["readable"]
assert unix.poll_fd(fd, ["readable", "readable"])? == ["readable"]
unix.close_fd(fd)?
assert unix.poll_fd(fd, [])? == ["invalid"]
""", args: [file])?
  assert result.success, result.stderr
}

test test_unix_poll_fd_fifo_timeout_and_writer_error { |ctx|
  let root = test.temp_dir(ctx)?
  let fifo = fp"{root}/pipe"
  fs.mkfifo(fifo, mode: 384)?
  let reader = unix.open_fd(fifo, nonblock: true)?
  var reader_open = true
  defer { if reader_open { unix.close_fd(reader) } }
  assert unix.poll_fd(reader, ["readable"], timeout_ms: 5)? == []
  {
    let writer = unix.open_fd(fifo, write: true, nonblock: true)?
    defer unix.close_fd(writer)
    assert unix.poll_fd(writer, ["writable"])? == ["writable"]
  }
  assert "hangup" in unix.poll_fd(reader, ["readable"])?
  run sh -c "printf payload > \"$1\"" sh $fifo
  let ready = unix.poll_fd(reader, ["readable"])?
  assert "readable" in ready and "hangup" in ready
  unix.close_fd(reader)?
  reader_open = false
  let result = test.run_script(ctx, r"""
let fifo = Path(args[0])
let reader = unix.open_fd(fifo, nonblock: true)?
unix.redirect_fd(100, fifo, write: true)?
unix.close_fd(reader)?
assert "error" in unix.poll_fd(100, ["writable"])?
unix.close_fd(100)?
""", args: [fifo])?
  assert result.success, result.stderr
}

test test_unix_read_fd_validates_fd_and_count { |ctx|
  test.error_kind(unix.read_fd(-1, 1), "unix-read-fd")
  test.error_kind(unix.read_fd(2147483648, 1), "unix-read-fd")
  test.error_kind(unix.read_fd(0, 0), "unix-read-fd")
  test.error_kind(unix.read_fd(0, -1), "unix-read-fd")
  assert unix.read_fd(2147483647, 1) is Err(is HostIo)
  let file = test.temp_file(ctx, contents: b"payload")?
  let result = test.run_script(ctx, r"""
unix.redirect_fd(100, Path(args[0]), write: true)?
assert unix.read_fd(100, 1) is Err(is HostIo)
unix.close_fd(100)?
""", args: [file])?
  assert result.success, result.stderr
}

test test_unix_write_fd_writes_one_chunk_and_reports_errors { |ctx|
  let file = test.temp_file(ctx, contents: b"")?
  let result = test.run_script(ctx, r"""
let fd = unix.open_fd(Path(args[0]), write: true)?
assert unix.write_fd(fd, b"-bytes")? == 6
assert unix.write_fd(fd, b"")? == 0
unix.close_fd(fd)?
test.error_kind(unix.write_fd(-1, b"x"), "unix-write-fd")
assert unix.write_fd(2147483647, b"x") is Err(is HostIo)
""", args: [file])?
  assert result.success, result.stderr
  assert file.read_bytes()? == b"-bytes"
}

test test_unix_seek_fd_uses_absolute_positions { |ctx|
  let file = test.temp_file(ctx, contents: b"abcdef")?
  let fd = unix.open_fd(file)?
  defer unix.close_fd(fd)
  assert unix.seek_fd(fd, 3)? == 3
  assert unix.read_fd(fd, 2)? == b"de"
  assert unix.seek_fd(fd, 0)? == 0
  assert unix.read_fd(fd, 1)? == b"a"
  test.error_kind(unix.seek_fd(-1, 0), "unix-seek-fd")
  test.error_kind(unix.seek_fd(fd, -1), "unix-seek-fd")
  test.error_kind(unix.seek_fd(2147483648, 0), "unix-seek-fd")

  let fifo_dir = test.temp_dir(ctx, name: "seek-fifo")?
  let fifo = fp"{fifo_dir}/pipe"
  fs.mkfifo(fifo, mode: 0o600)?
  let reader = unix.open_fd(fifo, nonblock: true)?
  defer unix.close_fd(reader)
  assert unix.seek_fd(reader, 0) is Err(is HostIo)
}

test test_unix_read_fd_preserves_byte_cursor_and_eof { |ctx|
  let file = test.temp_file(ctx, contents: b"\0\xffabc")?
  let result = test.run_script(ctx, r"""
let reader = unix.open_fd(Path(args[0]))?
assert unix.read_fd(reader, 2)? == b"\0\xff"
assert unix.read_fd(reader, 1)? == b"a"
assert unix.read_fd(reader, 8)? == b"bc"
assert unix.read_fd(reader, 1)? == b""
unix.close_fd(reader)?
assert unix.read_fd(reader, 1) is Err(is HostIo)
""", args: [file])?
  assert result.success, result.stderr
}

test test_unix_read_fd_and_stdin_read_share_descriptor_cursor { |ctx|
  let result = test.run_script(ctx, """
assert unix.read_fd(0, 1)? == b"a"
assert io.stdin_read(1)? == b"b"
assert unix.read_fd(0, 1)? == b"c"
assert io.stdin_line()? == "d"
assert unix.read_fd(0, 8)? == b"ef"
assert io.stdin_bytes()? == b""
""", stdin: b"abcd\nef")?
  assert result.success, result.stderr
}

test test_unix_read_fd_reads_fifo_without_seeking_or_waiting_for_requested_size { |ctx|
  let root = test.temp_dir(ctx)?
  let fifo = fp"{root}/pipe"
  fs.mkfifo(fifo, mode: 384)?
  let reader = unix.open_fd(fifo, nonblock: true)?
  defer unix.close_fd(reader)
  run sh -c "printf payload > \"$1\"" sh $fifo
  assert unix.read_fd(reader, 2)? == b"pa"
  assert unix.read_fd(reader, 100)? == b"yload"
  assert unix.read_fd(reader, 1)? == b""
}

test test_unix_exec_env_replaces_inherited_and_command_environment { |ctx|
  let result = test.expect(ctx, r"""let command = process.command_argv(p"/usr/bin/env", ["env"], env: {COMMAND_ONLY: "discard"})
let environment: Map[Str, Str] = {"ONLY": "kept"}
unix.exec_env(command, environment)?
""", status: 0, env: {INHERITED_ONLY: "discard"})?
  assert result.stdout == "ONLY=kept\n"
}

test test_unix_exec_env_applies_explicit_argv0 { |ctx|
  let result = test.expect(ctx, r"""let command = process.command_argv(p"/bin/sh", ["sh", "-c", "printf '%s' \"$0\""])
let environment: Map[Str, Str] = {}
unix.exec_env(command, environment, argv0: "-sh")?
""", status: 0)?
  assert result.stdout == "-sh"
}

test test_unix_exec_env_rejects_invalid_strings_before_redirection { |ctx|
  let root = test.temp_dir(ctx, name: "exec-env-validation")?
  let output = fp"{root}/must-not-exist"
  let command = process.command_argv(p"/bin/true", ["true"], stdout: output)
  let bad_key: Map[Str, Str] = {"BAD=KEY": "value"}
  let bad_value: Map[Str, Str] = {"VALID": "bad\u{0}value"}
  let empty: Map[Str, Str] = {}
  test.error_kind(unix.exec_env(command, bad_key), "unix-exec-env")
  test.error_kind(unix.exec_env(command, bad_value), "unix-exec-env")
  test.error_kind(unix.exec_env(command, empty, argv0: "bad\u{0}name"), "unix-exec-env")
  assert ! output.exists()?
}
