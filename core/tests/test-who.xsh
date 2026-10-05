type Ran = {status: Int, stdout: Str, stderr: Str}

const BOOT = 2
const RUN_LEVEL = 1
const NEW_TIME = 3
const INIT_PROCESS = 5
const LOGIN_PROCESS = 6
const USER_PROCESS = 7
const DEAD_PROCESS = 8

# Tue Nov 14 22:13:20 UTC 2023
const STAMP = 1700000000

proc padded(text: Str, width: Int) [error] -> Result[Bytes, Error] {
  let raw = bytes.from_text(text)
  bytes.concat([raw, bytes.zero(width - raw.len())?])
}

# One 384-byte glibc `struct utmp` record in native byte order.
proc utmp_record(kind: Int, pid: Int, line: Str, id: Str, account: Str, host: Str, termination = 0, code = 0) [error] -> Result[Bytes, Error] {
  bytes.concat([
    bytes.pack_le(kind, 2)?,
    bytes.zero(2)?,
    bytes.pack_le(pid, 4)?,
    padded(line, 32)?,
    padded(id, 4)?,
    padded(account, 32)?,
    padded(host, 256)?,
    bytes.pack_le(termination, 2)?,
    bytes.pack_le(code, 2)?,
    bytes.zero(4)?,
    bytes.pack_le(STAMP, 4)?,
    bytes.zero(4)?,
    bytes.zero(36)?,
  ])
}

proc run_level(previous: Int, current: Int) [error] -> Result[Bytes] {
  utmp_record(RUN_LEVEL, previous * 256 + current, "~", "~~", "runlevel", "")
}

proc fixture(ctx: TestContext) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: "who-utmp")?
  let file = fp"{root}/utmp"
  file.write(bytes.concat([
    utmp_record(BOOT, 0, "~", "~~", "reboot", "")?,
    run_level(78, 53)?,
    utmp_record(NEW_TIME, 0, "{", "", "date", "")?,
    utmp_record(INIT_PROCESS, 1234, "tty9", "si", "", "")?,
    utmp_record(LOGIN_PROCESS, 2345, "tty1", "1", "LOGIN", "")?,
    utmp_record(USER_PROCESS, 3456, "ttyNotThere", "ts/9", "alice", "example.org")?,
    utmp_record(DEAD_PROCESS, 4567, "pts/98", "ts/8", "", "", termination: 1, code: 2)?,
  ]))
  Ok(file)
}

proc who_run(ctx: TestContext, args: List[Str], vars: Record = {LC_ALL: "C", TZ: "UTC"}, sink: Path? = null) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "who")?
  let out = sink ?? fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/who.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: if sink == null { out.read_text()? } else { "" }, stderr: err.read_text()?})
}

proc who_file(ctx: TestContext, args: List[Str], vars: Record = {LC_ALL: "C", TZ: "UTC"}) [fs, process, error] -> Result[Ran] {
  who_run(ctx, args.extend([fixture(ctx)?.display()]), vars)
}

test test_who_default_lists_user_sessions { |ctx|
  let result = who_file(ctx, [])?
  assert result.status == 0
  assert result.stdout == "alice    ttyNotThere  Nov 14 22:13 (example.org)\n", result.stdout
  assert result.stderr == ""
}

test test_who_each_selector_prints_its_own_record_kind { |ctx|
  let cases = [
    ["-b", "         system boot  Nov 14 22:13\n"],
    ["-r", "         run-level 5  Nov 14 22:13                   last=S\n"],
    ["-t", "         clock change Nov 14 22:13\n"],
    ["-p", "         tty9         Nov 14 22:13       1234 id=si\n"],
    ["-l", "LOGIN    tty1         Nov 14 22:13              2345 id=1\n"],
    ["-d", "         pts/98       Nov 14 22:13              4567 id=ts/8  term=1 exit=2\n"],
    ["-u", "alice    ttyNotThere  Nov 14 22:13   ?          3456 (example.org)\n"],
    ["-T", "alice    ? ttyNotThere  Nov 14 22:13 (example.org)\n"],
  ]

  for case in cases {
    let result = who_file(ctx, [case[0]])?
    assert result.stdout == case[1], f"who {case[0]}: {result.stdout}"
  }
}

test test_who_all_combines_every_selector { |ctx|
  let result = who_file(ctx, ["-a"])?
  assert result.stdout == """           system boot  Nov 14 22:13
           run-level 5  Nov 14 22:13                   last=S
           clock change Nov 14 22:13
           tty9         Nov 14 22:13              1234 id=si
LOGIN      tty1         Nov 14 22:13              2345 id=1
alice    ? ttyNotThere  Nov 14 22:13   ?          3456 (example.org)
           pts/98       Nov 14 22:13              4567 id=ts/8  term=1 exit=2
""", result.stdout
  assert who_file(ctx, ["--all"])?.stdout == result.stdout
  assert who_file(ctx, ["-b", "-d", "--login", "-p", "-r", "-t", "-T", "-u"])?.stdout == result.stdout
}

test test_who_short_flag_keeps_the_pid_column_out_unless_exit_shows { |ctx|
  assert who_file(ctx, ["-s", "-u"])?.stdout == "alice    ttyNotThere  Nov 14 22:13 (example.org)\n"
  let all_short = who_file(ctx, ["-a", "-s"])?
  assert "1234 id=si" in all_short.stdout, "-a turns the pid column back on"
}

test test_who_count_lists_names_and_a_total { |ctx|
  assert who_file(ctx, ["-q"])?.stdout == "alice\n# users=1\n"
  assert who_file(ctx, ["--count"])?.stdout == "alice\n# users=1\n"
  assert who_file(ctx, ["--cou"])?.stdout == "alice\n# users=1\n"

  let root = test.temp_dir(ctx, name: "who-none")?
  let empty = fp"{root}/utmp"
  empty.write(b"")
  assert who_run(ctx, ["-q", empty.display()])?.stdout == "\n# users=0\n"
}

test test_who_headings_follow_the_selected_columns { |ctx|
  let cases = [
    {args: ["-H"], out: "NAME     LINE         TIME         COMMENT\nalice    ttyNotThere  Nov 14 22:13 (example.org)\n"},
    {args: ["-H", "-T"], out: "NAME       LINE         TIME         COMMENT\nalice    ? ttyNotThere  Nov 14 22:13 (example.org)\n"},
    {args: ["-H", "-u"], out: "NAME     LINE         TIME         IDLE          PID COMMENT\nalice    ttyNotThere  Nov 14 22:13   ?          3456 (example.org)\n"},
    {args: ["-H", "-p"], out: "NAME     LINE         TIME                PID COMMENT\n         tty9         Nov 14 22:13       1234 id=si\n"},
    {args: ["-H", "-d"], out: "NAME     LINE         TIME         IDLE          PID COMMENT  EXIT\n         pts/98       Nov 14 22:13              4567 id=ts/8  term=1 exit=2\n"},
    {args: ["-q", "-H"], out: "alice\n# users=1\n"},
  ]

  for case in cases {
    let result = who_file(ctx, case.args)?
    assert result.stdout == case.out, f"who {case.args.join(" ")}: {result.stdout}"
  }
}

test test_who_run_level_reports_the_previous_level { |ctx|
  let root = test.temp_dir(ctx, name: "who-levels")?
  let file = fp"{root}/utmp"
  file.write(run_level(51, 53)?)
  assert who_run(ctx, ["-r", file.display()])?.stdout == "         run-level 5  Nov 14 22:13                   last=3\n"
  file.write(run_level(0, 53)?)
  assert who_run(ctx, ["-r", file.display()])?.stdout == "         run-level 5  Nov 14 22:13\n"
}

test test_who_formats_time_from_the_locale_and_tz { |ctx|
  let iso = who_file(ctx, [], {LC_ALL: "C.UTF-8", TZ: "UTC"})?
  assert iso.stdout == "alice    ttyNotThere  2023-11-14 22:13 (example.org)\n", iso.stdout
  let shifted = who_file(ctx, [], {LC_ALL: "C", TZ: "EST5"})?
  assert shifted.stdout == "alice    ttyNotThere  Nov 14 17:13 (example.org)\n", shifted.stdout
  let ahead = who_file(ctx, [], {LC_ALL: "C", TZ: "<+03>-3"})?
  assert ahead.stdout == "alice    ttyNotThere  Nov 15 01:13 (example.org)\n", ahead.stdout
}

test test_who_host_display_is_kept_after_the_host { |ctx|
  let root = test.temp_dir(ctx, name: "who-display")?
  let file = fp"{root}/utmp"
  file.write(bytes.concat([
    utmp_record(USER_PROCESS, 1, "tty1", "1", "bob", "box:0")?,
    utmp_record(USER_PROCESS, 2, "tty2", "2", "eve", ":1")?,
  ]))
  assert who_run(ctx, [file.display()])?.stdout == "bob      tty1         Nov 14 22:13 (box:0)\neve      tty2         Nov 14 22:13 (:1)\n"
}

test test_who_lookup_passes_numeric_hosts_and_refuses_names { |ctx|
  let root = test.temp_dir(ctx, name: "who-lookup")?
  let file = fp"{root}/utmp"
  file.write(utmp_record(USER_PROCESS, 1, "tty1", "1", "bob", "192.0.2.7:0")?)
  assert who_run(ctx, ["--lookup", file.display()])?.stdout == "bob      tty1         Nov 14 22:13 (192.0.2.7:0)\n"

  file.write(utmp_record(USER_PROCESS, 1, "tty1", "1", "bob", "box.invalid")?)
  let refused = who_run(ctx, ["--lookup", file.display()])?
  assert refused.status == 1
  assert refused.stderr == "who: --lookup: cannot canonicalize host name 'box.invalid': canonical names are not available\n", refused.stderr
  assert who_run(ctx, [file.display()])?.status == 0, "names pass untouched without --lookup"
  assert who_file(ctx, ["--lookup"])?.status == 1, "the fixture's example.org is a name"
}

test test_who_message_state_and_idle_come_from_the_terminal_file { |ctx|
  # A utmp line holds at most 32 bytes, so the terminal files get short names.
  let root = test.temp_dir(ctx, name: "who-tty")?
  let writable = p"/tmp/xsh-who-writable"
  let closed = p"/tmp/xsh-who-closed"
  defer writable.remove()
  defer closed.remove()
  writable.write("")
  closed.write("")
  writable.chmod(0o620)
  closed.chmod(0o600)
  let now_ns = time.now() * 1000000
  fs.set_times(writable, atime_ns: now_ns - 7200 * 1000000000)
  fs.set_times(closed, atime_ns: now_ns - 200000 * 1000000000)

  let file = fp"{root}/utmp"
  file.write(bytes.concat([
    utmp_record(USER_PROCESS, 1, writable.display(), "1", "bob", "")?,
    utmp_record(USER_PROCESS, 2, closed.display(), "2", "eve", "")?,
  ]))

  let result = who_run(ctx, ["-T", "-u", file.display()])?
  let lines = result.stdout.lines()
  assert lines.len() == 2
  assert lines[0].starts_with("bob      + "), lines[0]
  assert "02:00" in lines[0], lines[0]
  assert lines[1].starts_with("eve      - "), lines[1]
  assert " old " in lines[1], lines[1]
}

test test_who_missing_file_lists_nothing_like_glibc { |ctx|
  let root = test.temp_dir(ctx, name: "who-missing")?
  let result = who_run(ctx, [fp"{root}/absent".display()])?
  assert result.status == 0
  assert result.stdout == ""
  assert result.stderr == ""
  assert who_run(ctx, ["-H", fp"{root}/absent".display()])?.stdout == "NAME     LINE         TIME         COMMENT\n"
}

test test_who_am_i_needs_a_terminal_on_stdin { |ctx|
  let silent = who_file(ctx, ["-m"])?
  assert silent.status == 0
  assert silent.stdout == ""
  let root = test.temp_dir(ctx, name: "who-ami")?
  let file = fp"{root}/utmp"
  file.write(utmp_record(USER_PROCESS, 1, "tty1", "1", "bob", "")?)
  assert who_run(ctx, ["am", "i"])?.stdout == "", "two operands mean -m with the default file"
  assert who_run(ctx, ["-H", "-m", file.display()])?.stdout == "NAME     LINE         TIME         COMMENT\n"
}

test test_who_third_operand_is_a_gnu_usage_error { |ctx|
  let result = who_run(ctx, ["am", "i", "u"])?
  assert result.status == 1
  assert result.stdout == ""
  assert result.stderr == "who: extra operand 'u'\nTry 'who --help' for more information.\n", result.stderr
}

test test_who_invalid_options_use_getopt_wording { |ctx|
  let short = who_run(ctx, ["-i"])?
  assert short.status == 1
  assert short.stderr == "who: invalid option -- 'i'\nTry 'who --help' for more information.\n", short.stderr
  let long = who_run(ctx, ["--definitely-invalid"])?
  assert long.status == 1
  assert long.stderr == "who: unrecognized option '--definitely-invalid'\nTry 'who --help' for more information.\n", long.stderr
  let ambiguous = who_run(ctx, ["--l"])?
  assert ambiguous.status == 1
  assert ambiguous.stderr.starts_with("who: option '--l' is ambiguous"), ambiguous.stderr
  assert who_file(ctx, ["--m"])?.stdout == "alice    ? ttyNotThere  Nov 14 22:13 (example.org)\n", "--m is --mesg or --message, which are one option"
}

test test_who_help_and_version_go_to_stdout { |ctx|
  let help = who_run(ctx, ["--help"])?
  assert help.status == 0
  assert help.stderr == ""
  assert help.stdout.starts_with("Usage: who [OPTION]... [ FILE | ARG1 ARG2 ]\n")
  let version = who_run(ctx, ["--version"])?
  assert version.status == 0
  assert version.stdout.starts_with("who (XSH core)")
}

test test_who_reports_a_full_device { |ctx|
  if ! p"/dev/full".exists() {
    test.skip("/dev/full is not available")
  }

  for flags in [[], ["-q"], ["--heading"]] {
    let result = who_run(ctx, flags.extend([fixture(ctx)?.display()]), sink: p"/dev/full")?
    assert result.status == 1
    assert result.stderr == "who: write error: No space left on device\n", result.stderr
  }
}
