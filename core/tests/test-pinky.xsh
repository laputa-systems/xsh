type Ran = {status: Int, stdout: Str, stderr: Str}

const USER_PROCESS = 7
const DEAD_PROCESS = 8

# Tue Nov 14 22:13:20 UTC 2023
const STAMP = 1700000000

const HEADING = "Login    Name                 TTY      Idle   When         Where\n"

proc padded(text: Str, width: Int) [error] -> Result[Bytes, Error] {
  let raw = bytes.from_text(text)
  bytes.concat([raw, bytes.zero(width - raw.len())?])
}

# One 384-byte glibc `struct utmp` record in native byte order.
proc utmp_record(kind: Int, line: Str, account: Str, host: Str) [error] -> Result[Bytes, Error] {
  bytes.concat(
    [
      bytes.pack_le(kind, 2)?,
      bytes.zero(2)?,
      bytes.zero(4)?,
      padded(line, 32)?,
      padded("", 4)?,
      padded(account, 32)?,
      padded(host, 256)?,
      bytes.zero(8)?,
      bytes.pack_le(STAMP, 4)?,
      bytes.zero(4)?,
      bytes.zero(36)?,
    ],
  )
}

# A passwd file and utmp file the applet reads through XSH_PASSWD_FILE and
# XSH_UTMP_FILE, plus a home directory for the long format.
type World = {passwd: Path, utmp: Path, home: Path}

proc world(ctx: TestContext, records: List[Bytes]) [fs, error] -> Result[World] {
  let root = test.temp_dir(ctx, name: "pinky")?
  let home = fp"{root}/home"
  home.mkdir()
  let passwd = fp"{root}/passwd"
  passwd.write(f"""alice:x:1000:1000:Alice Liddell,Room 1,555-1234:{home}:/bin/zsh
bob:x:1001:1001:& Builder:/home/bob:/bin/sh
longname:x:1002:1002:An Exceptionally Long Real Name:/home/longname:/bin/sh
blank:x:1003:1003::/home/blank:/bin/sh
""")
  let utmp = fp"{root}/utmp"
  utmp.write(bytes.concat(records))
  Ok({passwd: passwd, utmp: utmp, home: home})
}

proc pinky_run(
  ctx: TestContext,
  args: List[Str],
  w: World,
  sink: Path? = null,
  locale = "C",
) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "pinky-run")?
  let out = sink ?? fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/pinky.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let vars = {LC_ALL: locale, TZ: "UTC", XSH_PASSWD_FILE: w.passwd.display(), XSH_UTMP_FILE: w.utmp.display()}
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: if sink == null { out.read_text()? } else { "" }, stderr: err.read_text()?})
}

proc sessions(ctx: TestContext) [fs, error] -> Result[World] {
  world(
    ctx,
    [
      utmp_record(USER_PROCESS, "ttyNotThere", "alice", "example.org:0")?,
      utmp_record(DEAD_PROCESS, "pts/9", "bob", "")?,
      utmp_record(USER_PROCESS, "pts/8", "bob", "::1")?,
      utmp_record(USER_PROCESS, "pts/7", "stranger", "")?,
    ],
  )
}

test test_pinky_short_format_lists_user_sessions_with_names { |ctx|
  let w = sessions(ctx)?
  let result = pinky_run(ctx, [], w)?
  assert result.status == 0
  assert result.stderr == ""
  assert result.stdout == HEADING + """alice    Alice Liddell       ?ttyNotThere ?????  Nov 14 22:13 example.org:0
bob      Bob Builder         ?pts/8    ?????  Nov 14 22:13 ::1
stranger                 ??? ?pts/7    ?????  Nov 14 22:13
""", result.stdout
}

test test_pinky_column_options_drop_columns { |ctx|
  let w = sessions(ctx)?
  assert pinky_run(ctx, ["-f"], w)?.stdout.starts_with("alice    Alice Liddell")
  let no_name = pinky_run(ctx, ["-w"], w)?.stdout
  assert no_name == "Login     TTY      Idle   When         Where\nalice    ?ttyNotThere ?????  Nov 14 22:13 example.org:0\nbob      ?pts/8    ?????  Nov 14 22:13 ::1\nstranger ?pts/7    ?????  Nov 14 22:13\n", no_name
  let no_host = pinky_run(ctx, ["-i"], w)?.stdout
  assert no_host == "Login     TTY      Idle   When        \nalice    ?ttyNotThere ?????  Nov 14 22:13\nbob      ?pts/8    ?????  Nov 14 22:13\nstranger ?pts/7    ?????  Nov 14 22:13\n", no_host
  let quiet = pinky_run(ctx, ["-q"], w)?.stdout
  assert quiet == "Login     TTY      When        \nalice    ?ttyNotThere Nov 14 22:13\nbob      ?pts/8    Nov 14 22:13\nstranger ?pts/7    Nov 14 22:13\n", quiet
  assert pinky_run(ctx, ["-s"], w)?.stdout == pinky_run(ctx, [], w)?.stdout
}

test test_pinky_names_select_sessions { |ctx|
  let w = sessions(ctx)?
  let result = pinky_run(ctx, ["-f", "bob", "nobody"], w)?
  assert result.stdout == "bob      Bob Builder         ?pts/8    ?????  Nov 14 22:13 ::1\n", result.stdout
}

test test_pinky_terminal_file_gives_mesg_and_idle { |ctx|
  # A utmp line holds at most 32 bytes, so the terminal files get short names.
  let open_line = /tmp/xsh-pinky-open
  let shut_line = /tmp/xsh-pinky-shut
  let old_line = /tmp/xsh-pinky-old
  for line in [open_line, shut_line, old_line] {
    line.write("")
  }

  defer open_line.remove()
  defer shut_line.remove()
  defer old_line.remove()
  open_line.chmod(0o620)
  shut_line.chmod(0o600)
  old_line.chmod(0o660)
  let now_ns = time.now() * 1000000
  fs.set_times(open_line, atime_ns: now_ns - 10 * 1000000000)
  fs.set_times(shut_line, atime_ns: now_ns - 3000 * 1000000000)
  fs.set_times(old_line, atime_ns: now_ns - 200000 * 1000000000)

  let w = world(
    ctx,
    [
      utmp_record(USER_PROCESS, open_line.display(), "alice", "")?,
      utmp_record(USER_PROCESS, shut_line.display(), "bob", "")?,
      utmp_record(USER_PROCESS, old_line.display(), "blank", "")?,
    ],
  )?
  let lines = pinky_run(ctx, ["-f", "-w"], w)?.stdout.lines()
  assert lines.len() == 3
  assert lines[0] == "alice     /tmp/xsh-pinky-open        Nov 14 22:13", lines[0]
  assert lines[1] == "bob      */tmp/xsh-pinky-shut 00:50  Nov 14 22:13", lines[1]
  assert lines[2] == "blank     /tmp/xsh-pinky-old 2d     Nov 14 22:13", lines[2]
}

test test_pinky_time_follows_the_locale { |ctx|
  let w = sessions(ctx)?
  let result = pinky_run(ctx, [], w, locale: "C.UTF-8")?
  assert result.stdout.starts_with("Login    Name                 TTY      Idle   When             Where\n"), result.stdout
  assert "2023-11-14 22:13" in result.stdout
}

test test_pinky_long_format_prints_account_details { |ctx|
  let w = sessions(ctx)?
  let result = pinky_run(ctx, ["-l", "alice"], w)?
  assert result.status == 0
  assert result.stdout == f"""Login name: alice                       In real life:  Alice Liddell
Directory: {w.home}{pad(w.home.display(), 29)}Shell:  /bin/zsh

""", result.stdout
}

pure pad(text: Str, width: Int) -> Str {
  var out = ""

  while text.count_chars() + out.count_chars() < width {
    out = f"{out} "
  }

  out
}

test test_pinky_long_format_expands_ampersand_and_omits_unknown_users_details { |ctx|
  let w = sessions(ctx)?
  let result = pinky_run(ctx, ["-lb", "bob", "ghost", "blank"], w)?
  assert result.stdout == """Login name: bob                         In real life:  Bob Builder

Login name: ghost                       In real life:  ???
Login name: blank                       In real life:  

""", result.stdout
}

test test_pinky_long_format_copies_project_and_plan_files { |ctx|
  let w = sessions(ctx)?
  fp"{w.home}/.project".write("Compiler\nsecond line\n")
  fp"{w.home}/.plan".write("Ship it\n\nsoon")

  let full = pinky_run(ctx, ["-l", "alice"], w)?.stdout
  assert full.ends_with("Project: Compiler\nsecond line\nPlan:\nShip it\n\nsoon\n"), full
  let no_project = pinky_run(ctx, ["-lh", "alice"], w)?.stdout
  assert no_project.ends_with("Shell:  /bin/zsh\nPlan:\nShip it\n\nsoon\n"), no_project
  let no_plan = pinky_run(ctx, ["-lp", "alice"], w)?.stdout
  assert no_plan.ends_with("Shell:  /bin/zsh\nProject: Compiler\nsecond line\n\n"), no_plan
  let bare = pinky_run(ctx, ["-lbhp", "alice"], w)?.stdout
  assert bare == "Login name: alice                       In real life:  Alice Liddell\n\n", bare
}

test test_pinky_long_format_needs_a_user { |ctx|
  let w = sessions(ctx)?
  let result = pinky_run(ctx, ["-l"], w)?
  assert result.status == 1
  assert result.stdout == ""
  assert result.stderr == "pinky: no username specified; at least one must be specified when using -l\nTry 'pinky --help' for more information.\n", result.stderr
}

test test_pinky_last_of_long_and_short_decides { |ctx|
  let w = sessions(ctx)?
  assert pinky_run(ctx, ["-l", "-s"], w)?.stdout.starts_with("Login    Name")
  assert pinky_run(ctx, ["-s", "-l", "alice"], w)?.stdout.starts_with("Login name: alice")
}

test test_pinky_lookup_passes_numeric_hosts_and_refuses_names { |ctx|
  let numeric = world(ctx, [utmp_record(USER_PROCESS, "pts/1", "alice", "192.0.2.7:0")?])?
  let kept = pinky_run(ctx, ["--lookup", "-f"], numeric)?
  assert kept.status == 0
  assert kept.stdout.ends_with("Nov 14 22:13 192.0.2.7:0\n"), kept.stdout

  let named = world(ctx, [utmp_record(USER_PROCESS, "pts/1", "alice", "box.invalid:0")?])?
  let refused = pinky_run(ctx, ["--lookup", "-f"], named)?
  assert refused.status == 1
  assert refused.stderr == "pinky: --lookup: cannot canonicalize host name 'box.invalid': canonical names are not available\n", refused.stderr

  let nobody = pinky_run(ctx, ["--lookup"], world(ctx, [])?)?
  assert nobody.status == 0, "--lookup with no hosts to canonicalize is harmless"
}

test test_pinky_missing_utmp_lists_only_the_heading { |ctx|
  let w = sessions(ctx)?
  w.utmp.remove()
  let result = pinky_run(ctx, [], w)?
  assert result.status == 0
  assert result.stdout == HEADING
}

test test_pinky_invalid_option_uses_getopt_wording { |ctx|
  let w = sessions(ctx)?
  let result = pinky_run(ctx, ["--definitely-invalid"], w)?
  assert result.status == 1
  assert result.stderr == "pinky: unrecognized option '--definitely-invalid'\nTry 'pinky --help' for more information.\n", result.stderr
  let short = pinky_run(ctx, ["-x"], w)?
  assert short.stderr == "pinky: invalid option -- 'x'\nTry 'pinky --help' for more information.\n", short.stderr
}

test test_pinky_help_and_version_go_to_stdout { |ctx|
  let w = sessions(ctx)?
  let help = pinky_run(ctx, ["--help"], w)?
  assert help.status == 0
  assert help.stderr == ""
  assert help.stdout.starts_with("Usage: pinky [OPTION]... [USER]...\n")
  let version = pinky_run(ctx, ["--version"], w)?
  assert version.status == 0
  assert version.stdout.starts_with("pinky (XSH core)")
}

test test_pinky_reports_a_full_device { |ctx|
  if ! p"/dev/full".exists() {
    test.skip("/dev/full is not available")
  }

  let w = sessions(ctx)?
  let result = pinky_run(ctx, [], w, sink: /dev/full)?
  assert result.status == 1
  assert result.stderr == "pinky: write error: No space left on device\n", result.stderr
}
