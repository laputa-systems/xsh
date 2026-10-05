type Ran = {status: Int, stdout: Str, stderr: Str}

const BOOT = 2
const USER_PROCESS = 7

proc padded(text: Str, width: Int) [error] -> Result[Bytes, Error] {
  let raw = bytes.from_text(text)
  return Ok(bytes.concat([raw, bytes.zero(width - raw.len())?]))
}

# One 384-byte glibc `struct utmp` record in native byte order.
proc utmp_record(kind: Int, line: Str, account: Str, stamp: Int) [error] -> Result[Bytes, Error] {
  return Ok(bytes.concat([
    bytes.pack_le(kind, 2)?,
    bytes.zero(2)?,
    bytes.zero(4)?,
    padded(line, 32)?,
    padded("", 4)?,
    padded(account, 32)?,
    padded("", 256)?,
    bytes.zero(8)?,
    bytes.pack_le(stamp, 4)?,
    bytes.zero(4)?,
    bytes.zero(36)?,
  ]))
}

proc uptime_run(ctx: TestContext, args: List[Str], sink: Path? = null) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "uptime")?
  let out = sink ?? fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/uptime.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", TZ: "UTC"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: if sink == null { out.read_text()? } else { "" }, stderr: err.read_text()?})
}

proc utmp_file(ctx: TestContext, records: List[Bytes]) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: "uptime-utmp")?
  let file = fp"{root}/utmp"
  file.write(bytes.concat(records))?
  Ok(file)
}

const LINE = rx"^\d{1,2}:\d\d:\d\d up (.*),  (\d+) users?,  load average: \d+\.\d\d, \d+\.\d\d, \d+\.\d\d$"

test test_uptime_line_has_time_uptime_users_and_load { |ctx|
  let result = uptime_run(ctx, [])?
  assert result.status == 0
  assert result.stderr == ""
  assert LINE.matches(result.stdout.trim()), result.stdout
}

test test_uptime_with_a_boot_record_counts_days_and_users { |ctx|
  let now = time.now() / 1000
  let file = utmp_file(ctx, [
    utmp_record(BOOT, "~", "reboot", now - 3 * 86400 - 5 * 3600 - 7 * 60 - 30)?,
    utmp_record(USER_PROCESS, "tty1", "alice", now)?,
    utmp_record(USER_PROCESS, "tty2", "bob", now)?,
  ])?
  let result = uptime_run(ctx, [file.display()])?
  assert result.status == 0
  assert result.stderr == ""
  let parts = LINE.captures(result.stdout.trim())
  assert parts.len() > 0, result.stdout
  assert parts[1] == "3 days  5:07", parts[1]
  assert parts[2] == "2"
}

test test_uptime_singular_day_and_user { |ctx|
  let now = time.now() / 1000
  let file = utmp_file(ctx, [
    utmp_record(BOOT, "~", "reboot", now - 86400 - 60)?,
    utmp_record(USER_PROCESS, "tty1", "alice", now)?,
  ])?
  let result = uptime_run(ctx, [file.display()])?
  assert " up 1 day  0:01,  1 user,  load average" in result.stdout, result.stdout
}

test test_uptime_without_boot_record_names_the_problem_and_fails { |ctx|
  let root = test.temp_dir(ctx, name: "uptime-bad")?
  let junk = fp"{root}/junk"
  junk.write("hello")?
  let result = uptime_run(ctx, [junk.display()])?
  assert result.status == 1
  assert result.stderr == "uptime: couldn't get boot time\n", result.stderr
  assert " up ???? days ??:??,  0 users,  load average: " in result.stdout, result.stdout
}

test test_uptime_unreadable_operands_are_named_by_errno { |ctx|
  let root = test.temp_dir(ctx, name: "uptime-operand")?
  let missing = uptime_run(ctx, [fp"{root}/absent".display()])?
  assert missing.status == 1
  assert missing.stderr == "uptime: couldn't get boot time: No such file or directory\n", missing.stderr
  assert "up ???? days ??:??" in missing.stdout

  let directory = uptime_run(ctx, [root.display()])?
  assert directory.status == 1
  assert directory.stderr == "uptime: couldn't get boot time: Is a directory\n", directory.stderr
  assert "up ???? days ??:??" in directory.stdout
}

test test_uptime_fifo_operand_is_refused_without_opening_it { |ctx|
  let root = test.temp_dir(ctx, name: "uptime-fifo")?
  let fifo = fp"{root}/fifo"
  fs.mkfifo(fifo, 0o600)?
  let result = uptime_run(ctx, [fifo.display()])?
  assert result.status == 1
  assert result.stderr == "uptime: couldn't get boot time: Illegal seek\n", result.stderr
  assert "up ???? days ??:??" in result.stdout
}

test test_uptime_pretty_lists_units { |ctx|
  let result = uptime_run(ctx, ["-p"])?
  assert result.status == 0
  assert rx"^up (\d+ weeks?, )?(\d+ days?, )?(\d+ hours?, )?\d+ minutes?\n$".matches(result.stdout), result.stdout

  let now = time.now() / 1000
  let file = utmp_file(ctx, [utmp_record(BOOT, "~", "reboot", now - 9 * 86400 - 3600 - 120 - 5)?])?
  assert uptime_run(ctx, ["--pretty", file.display()])?.stdout == "up 1 week, 2 days, 1 hour, 2 minutes\n"

  let fresh = utmp_file(ctx, [utmp_record(BOOT, "~", "reboot", now - 5)?])?
  assert uptime_run(ctx, ["-p", fresh.display()])?.stdout == "up 0 minutes\n"
}

test test_uptime_since_prints_the_boot_time_in_the_local_zone { |ctx|
  let file = utmp_file(ctx, [utmp_record(BOOT, "~", "reboot", 1700000000)?])?
  assert uptime_run(ctx, ["-s", file.display()])?.stdout == "2023-11-14 22:13:20\n"
  assert uptime_run(ctx, ["--since", file.display()])?.stdout == "2023-11-14 22:13:20\n"
  assert rx"^\d{4}-\d\d-\d\d \d\d:\d\d:\d\d\n$".matches(uptime_run(ctx, ["--since"])?.stdout)
}

test test_uptime_second_operand_is_a_gnu_usage_error { |ctx|
  let result = uptime_run(ctx, ["a", "b"])?
  assert result.status == 1
  assert result.stdout == ""
  assert result.stderr == "uptime: extra operand 'b'\nTry 'uptime --help' for more information.\n", result.stderr
}

test test_uptime_invalid_option_uses_getopt_wording { |ctx|
  let result = uptime_run(ctx, ["--definitely-invalid"])?
  assert result.status == 1
  assert result.stderr == "uptime: unrecognized option '--definitely-invalid'\nTry 'uptime --help' for more information.\n", result.stderr
}

test test_uptime_help_and_version_go_to_stdout { |ctx|
  let help = uptime_run(ctx, ["--help"])?
  assert help.status == 0
  assert help.stderr == ""
  assert help.stdout.starts_with("Usage: uptime [OPTION]... [FILE]\n")
  let version = uptime_run(ctx, ["--version"])?
  assert version.status == 0
  assert version.stdout.starts_with("uptime (XSH core)")
}

test test_uptime_reports_a_full_device { |ctx|
  if ! p"/dev/full".exists()? {
    test.skip("/dev/full is not available")
  }

  let result = uptime_run(ctx, [], sink: p"/dev/full")?
  assert result.status == 1
  assert result.stderr == "uptime: write error: No space left on device\n", result.stderr
}
