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
  let path = fp"{root}/must-not-exist"
  test.error_kind(unix.redirect_fd(-1, path, write: true), "unix-redirect-fd")
  test.error_kind(unix.redirect_fd(2147483648, path, write: true), "unix-redirect-fd")
  test.error_kind(unix.redirect_fd(1, path, append: true), "unix-redirect-fd")
  test.error_kind(unix.redirect_fd(1, path, write: true, mode: -1), "unix-redirect-fd")
  test.error_kind(unix.redirect_fd(1, path, write: true, mode: 4096), "unix-redirect-fd")
  assert ! path.exists()?
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
    Ok(path) => path
    Err(_) => {
      test.skip("unshare is unavailable")
      return
    }
  }
  let probe = run.capture $unshare --mount --pid --fork -- true
  guard probe.status.ok else {
    test.skip("private mount and PID namespaces are unavailable")
    return
  }
  let script = test.temp_file(ctx, name: "credentials.xsh", contents: bytes.from_text(r"""unix.set_groups([123, 456])?
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
  let output = run.capture $unshare --mount --pid --fork -- ${ctx.xsh_bin} $script
  assert output.status.ok, output.stderr
  assert output.stdout == "credentials-changed\n"
}
