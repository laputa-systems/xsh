type Ran = {status: Int, stdout: Str, stderr: Str}

proc padded(text: Str, width: Int) [error] -> Result[Bytes, Error] {
  let raw = bytes.from_text(text)
  bytes.concat([raw, bytes.zero(width - raw.len())?])
}

# One 384-byte glibc `struct utmp` record in native byte order.
proc utmp_record(kind: Int, pid: Int, line: Str, account: Str) [error] -> Result[Bytes, Error] {
  bytes.concat([
    bytes.pack_le(kind, 2)?,
    bytes.zero(2)?,
    bytes.pack_le(pid, 4)?,
    padded(line, 32)?,
    padded("", 4)?,
    padded(account, 32)?,
    padded("", 256)?,
    bytes.zero(4)?,
    bytes.zero(4)?,
    bytes.pack_le(1700000000, 4)?,
    bytes.zero(4)?,
    bytes.zero(36)?,
  ])
}

proc users_run(ctx: TestContext, args: List[Str], sink: Path? = null) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "users")?
  let out = sink ?? fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/users.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: if sink == null { out.read_text()? } else { "" }, stderr: err.read_text()?})
}

proc session_file(ctx: TestContext, records: List[Bytes]) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: "users-utmp")?
  let file = fp"{root}/utmp"
  file.write(bytes.concat(records))
  Ok(file)
}

test test_users_lists_user_sessions_sorted_on_one_line { |ctx|
  let file = session_file(ctx, [
    utmp_record(7, 1, "tty1", "zed")?,
    utmp_record(2, 0, "~", "reboot")?,
    utmp_record(7, 2, "tty2", "bob")?,
    utmp_record(8, 3, "tty3", "gone")?,
    utmp_record(7, 4, "tty4", "")?,
    utmp_record(7, 5, "tty5", "alice")?,
    utmp_record(7, 6, "tty6", "bob")?,
  ])?
  let result = users_run(ctx, [file.display()])?
  assert result.status == 0
  assert result.stdout == "alice bob bob zed\n", result.stdout
  assert result.stderr == ""
}

test test_users_prints_nothing_without_user_sessions { |ctx|
  let file = session_file(ctx, [utmp_record(2, 0, "~", "reboot")?])?
  let result = users_run(ctx, [file.display()])?
  assert result.status == 0
  assert result.stdout == ""
}

test test_users_missing_or_unreadable_file_lists_nobody_like_glibc { |ctx|
  let root = test.temp_dir(ctx, name: "users-missing")?
  assert users_run(ctx, [fp"{root}/absent".display()])?.stdout == ""
  let directory = users_run(ctx, [root.display()])?
  assert directory.status == 0
  assert directory.stdout == ""
  assert directory.stderr == ""
}

test test_users_second_operand_is_a_gnu_usage_error { |ctx|
  let result = users_run(ctx, ["a", "b"])?
  assert result.status == 1
  assert result.stdout == ""
  assert result.stderr == "users: extra operand 'b'\nTry 'users --help' for more information.\n", result.stderr
}

test test_users_invalid_option_uses_getopt_wording { |ctx|
  let short = users_run(ctx, ["-q"])?
  assert short.status == 1
  assert short.stderr == "users: invalid option -- 'q'\nTry 'users --help' for more information.\n", short.stderr
  let long = users_run(ctx, ["--definitely-invalid"])?
  assert long.status == 1
  assert long.stderr == "users: unrecognized option '--definitely-invalid'\nTry 'users --help' for more information.\n", long.stderr
}

test test_users_help_and_version_go_to_stdout { |ctx|
  let help = users_run(ctx, ["--help"])?
  assert help.status == 0
  assert help.stderr == ""
  assert help.stdout.starts_with("Usage: users [OPTION]... [FILE]\n")
  let version = users_run(ctx, ["--version"])?
  assert version.status == 0
  assert version.stdout.starts_with("users (XSH core)")
}

test test_users_reports_a_full_device { |ctx|
  if ! p"/dev/full".exists() {
    test.skip("/dev/full is not available")
  }

  let file = session_file(ctx, [utmp_record(7, 1, "tty1", "alice")?])?
  let result = users_run(ctx, [file.display()], sink: p"/dev/full")?
  assert result.status == 1
  assert result.stderr == "users: write error: No space left on device\n", result.stderr
}
