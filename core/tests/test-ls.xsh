use core.lib.acl

type Ran = {status: Int, out: Bytes, text: Str, err: Str}

# Runs core/ls.xsh through a symlink named `applet` (the installed alias shape,
# so dir and vdir see their own names) with `lib` linked beside it, inside
# `work`, capturing both streams.
proc ls_in(
  ctx: TestContext,
  work: Path,
  args: List[Str],
  vars: Record = {LC_ALL: "C", TZ: "UTC"},
  name = "ls",
  stdout_path: Path? = null,
) [fs, process, error] -> Result[Ran] {
  let bin = fp"{work}/../bin"
  bin.mkdir()

  let script = fp"{bin}/{name}"

  if ! script.exists() {
    script.symlink(to: fp"{ctx.core_dir}/ls.xsh")
  }

  if ! fp"{bin}/lib".exists() {
    fp"{bin}/lib".symlink(to: fp"{ctx.core_dir}/lib")
  }

  let out = stdout_path ?? fp"{work}/../stdout"
  let err = fp"{work}/../stderr"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, work, vars, b"", out, err)
  let status = process.run(plan)?
  let raw = if stdout_path == null { out.read_bytes()? } else { b"" }

  Ok({status: status.exit_code()?, out: raw, text: raw.utf8() ?? "", err: err.read_text()?})
}

proc sandbox(ctx: TestContext) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: "ls")?
  let work = fp"{root}/work"
  work.mkdir()
  Ok(work)
}

test test_ls_lists_names_sorted_one_per_line { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/b".write("")
  fp"{work}/a".write("")
  fp"{work}/.hidden".write("")
  fp"{work}/dir".mkdir()

  let plain = ls_in(ctx, work, [])?
  assert plain.text == "a\nb\ndir\n", plain.text
  assert plain.err == ""
  assert ls_in(ctx, work, ["-a"])?.text == ".\n..\n.hidden\na\nb\ndir\n"
  assert ls_in(ctx, work, ["-A"])?.text == ".hidden\na\nb\ndir\n"
  assert ls_in(ctx, work, ["-A", "-a"])?.text == ".\n..\n.hidden\na\nb\ndir\n", "the last of -a and -A wins"
  assert ls_in(ctx, work, ["-a", "-A"])?.text == ".hidden\na\nb\ndir\n"
  assert ls_in(ctx, work, ["-r"])?.text == "dir\nb\na\n"
}

test test_ls_long_reports_inaccessible_entries_as_minor_failure { |ctx|
  if user.current()?.uid == 0 { test.skip("root bypasses directory search permissions") }
  let work = sandbox(ctx)?
  let dir = fp"{work}/dir"
  dir.mkdir()
  fp"{dir}/file".write("")
  fp"{dir}/link".symlink(to: p"/")
  dir.chmod(0o600)
  defer { dir.chmod(0o700) }

  let result = ls_in(ctx, work, ["-l", "dir"])?
  assert result.status == 1, result.err
  assert "? file" in result.text, result.text
  assert "? link" in result.text, result.text
  assert "total 0\n" in result.text, result.text
  # An inaccessible entry still keeps its right-aligned timestamp column.
  assert " ?            ? file\n" in result.text, result.text
  assert " ?            ? link\n" in result.text, result.text
  assert "cannot access 'dir/file': Permission denied" in result.err, result.err
  assert "cannot access 'dir/link': Permission denied" in result.err, result.err

  dir.chmod(0o000)
  let unreadable = ls_in(ctx, work, ["dir"])?
  assert unreadable.status == 2, unreadable.err
  assert unreadable.err == "ls: cannot open directory 'dir': Permission denied\n", unreadable.err
}

test test_ls_long_marks_default_acl { |ctx|
  let work = sandbox(ctx)?
  let with_acl = fp"{work}/with-acl"
  let without_acl = fp"{work}/without-acl"
  let file = fp"{work}/file"
  with_acl.mkdir()
  without_acl.mkdir()
  file.write("contents")
  let access = acl.parse("u::rw-,u:12345:r--,g::r--,m::r--,o::r--")?
  let installed_access = fs.xattr_set(file, "system.posix_acl_access", acl.encode(access)?)
  if let Err(failure) = installed_access {
    if (failure.errno ?? -1) in [1, 95, 93] { test.skip(f"access ACL fixture unavailable: {failure.message}"); return }
    test.fail(failure.message)
  }
  let entries = acl.parse("u::rwx,u:12345:r-x,g::r-x,m::r-x,o::r-x", defaults: true)?
  let installed = fs.xattr_set(with_acl, "system.posix_acl_default", acl.encode(entries)?)
  if let Err(failure) = installed {
    if (failure.errno ?? -1) in [1, 95, 93] { test.skip(f"default ACL fixture unavailable: {failure.message}"); return }
    test.fail(failure.message)
  }

  let result = ls_in(ctx, work, ["-ld", "with-acl", "without-acl"])?
  assert result.status == 0, result.err
  assert "drwxr-xr-x+" in result.text, result.text
  assert "drwxr-xr-x " in result.text, result.text

  let access_line = ls_in(ctx, work, ["-l", "file"])?
  assert "-rw-r--r--+ 1 " in access_line.text, access_line.text
  let link = fp"{work}/link"
  link.symlink(to: with_acl)
  let followed = ls_in(ctx, work, ["-lLd", "link"])?
  assert "drwxr-xr-x+" in followed.text, followed.text
  let not_followed = ls_in(ctx, work, ["-ld", "link"])?
  assert "lrwxrwxrwx " in not_followed.text, not_followed.text
  assert "lrwxrwxrwx+" not in not_followed.text, not_followed.text
}

test test_ls_long_keeps_columns_aligned_when_an_entry_has_an_acl { |ctx|
  let work = sandbox(ctx)?
  let with_acl = fp"{work}/with-acl"
  let without_acl = fp"{work}/without-acl"
  with_acl.mkdir()
  without_acl.mkdir()
  let entries = acl.parse("u::rwx,u:12345:r-x,g::r-x,m::r-x,o::r-x", defaults: true)?
  if let Err(failure) = fs.xattr_set(with_acl, "system.posix_acl_default", acl.encode(entries)?) {
    if (failure.errno ?? -1) in [1, 95, 93] { test.skip(f"default ACL fixture unavailable: {failure.message}"); return }
    test.fail(failure.message)
  }

  let result = ls_in(ctx, work, ["-ld", "with-acl", "without-acl"])?
  assert result.status == 0, result.err
  assert result.text.find("drwxr-xr-x+ 2 ") != null, result.text
  assert result.text.find("drwxr-xr-x  2 ") != null, "the entry without an ACL gets a blank marker column"
}

test test_ls_directory_operands_and_headers { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/x".mkdir()
  fp"{work}/y".mkdir()
  fp"{work}/x/f".write("")
  fp"{work}/file".write("")

  assert ls_in(ctx, work, ["x"])?.text == "f\n", "a single directory has no header"
  assert ls_in(ctx, work, ["x", "y"])?.text == "x:\nf\n\ny:\n"
  assert ls_in(ctx, work, ["file", "x"])?.text == "file\n\nx:\nf\n"
  assert ls_in(ctx, work, ["-d", "x", "y"])?.text == "x\ny\n"
  assert ls_in(ctx, work, ["-R"])?.text == ".:\nfile\nx\ny\n\n./x:\nf\n\n./y:\n"
  assert ls_in(ctx, work, ["-aR", "x"])?.text == "x:\n.\n..\nf\n"
}

test test_ls_reports_missing_operands_with_status_2 { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/ok".write("")

  let result = ls_in(ctx, work, ["ok", "nope"])?
  assert result.status == 2
  assert result.text == "ok\n"
  assert result.err == "ls: cannot access 'nope': No such file or directory\n", result.err

  let blocked = ls_in(ctx, work, ["ok/inside"])?
  assert blocked.status == 2, blocked.err
  assert blocked.err == "ls: cannot access 'ok/inside': Not a directory\n", blocked.err
}

test test_ls_stats_proc_fd_entries_while_directory_is_open { |ctx|
  let work = sandbox(ctx)?
  if ! p"/proc/self/fd".is_dir()? { test.skip("requires procfs") }

  let result = ls_in(ctx, work, ["-l", "/proc/self/fd"])?
  assert result.status == 0, result.err
  assert "cannot access" not in result.err, result.err
}

test test_ls_write_errors_keep_serious_status { |ctx|
  if ! p"/dev/full".exists() { test.skip("requires /dev/full"); return }
  let work = sandbox(ctx)?
  fp"{work}/file".write("")

  let plain = ls_in(ctx, work, [], {LC_ALL: "C", TZ: "UTC"}, "ls", p"/dev/full")?
  assert plain.status == 2, plain.err
  assert plain.err == "ls: write error: No space left on device\n", plain.err

  let dired = ls_in(ctx, work, ["--dired", "missing"], {LC_ALL: "C", TZ: "UTC"}, "ls", p"/dev/full")?
  assert dired.status == 2, dired.err
  assert dired.err == "ls: cannot access 'missing': No such file or directory\nls: write error: No space left on device\n", dired.err
}

test test_ls_option_errors_use_getopt_and_argmatch_wording { |ctx|
  let work = sandbox(ctx)?

  let bad = ls_in(ctx, work, ["-j"])?
  assert bad.status == 2
  assert bad.err == "ls: invalid option -- 'j'\nTry 'ls --help' for more information.\n", bad.err

  let ambiguous = ls_in(ctx, work, ["--al"])?
  assert ambiguous.err == "ls: option '--al' is ambiguous; possibilities: '--all' '--almost-all'\nTry 'ls --help' for more information.\n", ambiguous.err

  let value = ls_in(ctx, work, ["--format=nope"])?
  assert value.status == 1
  assert value.out == b""
  assert value.err.starts_with(
    "ls: invalid argument 'nope' for '--format'\nValid arguments are:\n  - 'verbose', 'long'\n",
  ), value.err

  let width = ls_in(ctx, work, ["-w", "1a"])?
  assert width.status == 2
  assert width.err == "ls: invalid line width: '1a'\n", width.err

  let block = ls_in(ctx, work, ["--block-size=0"])?
  assert block.status == 2
  assert block.err == "ls: invalid --block-size argument '0'\n", block.err

  let style = ls_in(ctx, work, ["-l", "--time-style=bogus"])?
  assert style.status == 2
  assert style.err == "ls: invalid --time-style argument 'bogus'\nPossible values are:\n  - [posix-]full-iso\n  - [posix-]long-iso\n  - [posix-]iso\n  - [posix-]locale\n  - +FORMAT (e.g., +%H:%M) for a 'date'-style format\n\nFor more information try --help\n", style.err
  assert ls_in(ctx, work, ["--time-style=bogus"])?.status == 0, "the style is checked only for long listings"
}

test test_ls_posix_prefix_still_validates_the_style_name { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/old".write("")

  for style in ["posix-", "posix-l", "posix-lo", "posix-full-isox", "Locale"] {
    let result = ls_in(ctx, work, ["-l", f"--time-style={style}", "old"], {LC_ALL: "C", TZ: "UTC"})?
    assert result.status == 2, style
    assert result.out == b"", style
    assert "invalid --time-style argument" in result.err, result.err
  }

  let empty = ls_in(ctx, work, ["-l", "--time-style=posix-", "old"], {LC_ALL: "C", TZ: "UTC"})?
  assert empty.err.starts_with("ls: invalid --time-style argument ''\n"), empty.err
}

test test_ls_invalid_value_uses_failure_status { |ctx|
  let work = sandbox(ctx)?
  let result = ls_in(ctx, work, ["--classify=definitely_invalid_value"])?
  assert result.status == 1, result.err
  assert result.err.starts_with("ls: invalid argument 'definitely_invalid_value' for '--classify'\n"), result.err
}

test test_ls_long_format_columns { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/small".write("12")
  fp"{work}/big".write("1234567")
  fp"{work}/link".mkdir()
  fp"{work}/ln".symlink(to: p"small")

  let result = ls_in(ctx, work, ["-l", "--time-style=+T"])?
  let lines = result.text.lines()
  assert lines[0].starts_with("total "), lines[0]
  assert rx"^-rw-r--r-- 1 \S+ \S+ +7 T big$".matches(lines[1]), lines[1]
  assert rx"^drwx\S+ \d+ \S+ \S+ +\d+ T link$".matches(lines[2]), lines[2]
  assert rx"^lrwxrwxrwx 1 \S+ \S+ +5 T ln -> small$".matches(lines[3]), lines[3]
  assert rx"^-rw-r--r-- 1 \S+ \S+ +2 T small$".matches(lines[4]), lines[4]

  let numeric = ls_in(ctx, work, ["-n", "--time-style=+T", "small"])?
  assert rx"^-rw-r--r-- 1 \d+ \d+ 2 T small$".matches(numeric.text.trim()), numeric.text
  let no_owner = ls_in(ctx, work, ["-g", "--time-style=+T", "small"])?
  assert rx"^-rw-r--r-- 1 \S+ 2 T small$".matches(no_owner.text.trim()), no_owner.text
  let neither = ls_in(ctx, work, ["-og", "--time-style=+T", "small"])?
  assert neither.text == "-rw-r--r-- 1 2 T small\n", neither.text
  assert ls_in(ctx, work, ["-og", "-l1", "--time-style=+T", "small"])?.text == "-rw-r--r-- 1 2 T small\n", "-1 after -l changes nothing"
}

test test_ls_human_readable_sizes_round_up { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/f".write(bytes.zero(10250)?)

  assert ls_in(ctx, work, ["-lh", "-og", "--time-style=+T", "f"])?.text == "-rw-r--r-- 1 11K T f\n"
  assert ls_in(ctx, work, ["-l", "--si", "-og", "--time-style=+T", "f"])?.text == "-rw-r--r-- 1 11k T f\n"
  assert ls_in(ctx, work, ["-og", "--block-size=1K", "--time-style=+T", "-l", "f"])?.text == "-rw-r--r-- 1 11 T f\n"
  assert ls_in(ctx, work, ["-og", "--block-size=KB", "--time-style=+T", "-l", "f"])?.text == "-rw-r--r-- 1 11 T f\n"
}

test test_ls_sort_orders { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/a2".write("x")
  fp"{work}/a10".write("xxx")
  fp"{work}/b.txt".write("xx")
  fp"{work}/c.md".write("")
  fp"{work}/dd".write("")

  assert ls_in(ctx, work, ["-S"])?.text == "a10\nb.txt\na2\nc.md\ndd\n"
  assert ls_in(ctx, work, ["-Sr"])?.text == "dd\nc.md\na2\nb.txt\na10\n"
  assert ls_in(ctx, work, ["-v"])?.text == "a2\na10\nb.txt\nc.md\ndd\n"
  assert ls_in(ctx, work, ["--sort=version", "-r"])?.text == "dd\nc.md\nb.txt\na10\na2\n"
  assert ls_in(ctx, work, ["-X"])?.text == "a10\na2\ndd\nc.md\nb.txt\n"
  assert ls_in(ctx, work, ["--sort=width"])?.text == "a2\ndd\na10\nc.md\nb.txt\n"
  assert ls_in(ctx, work, ["-U", "-r"])?.status == 0
  assert ls_in(ctx, work, ["--sort=nope"])?.status == 1
}

test test_ls_groups_directories_first { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/a".write("")
  fp"{work}/m".mkdir()
  fp"{work}/z".mkdir()

  assert ls_in(ctx, work, ["--group-directories-first"])?.text == "m\nz\na\n"
  assert ls_in(ctx, work, ["--group-directories-first", "-r"])?.text == "z\nm\na\n"
  assert ls_in(ctx, work, ["--group-directories-first", "-a"])?.text == ".\n..\nm\nz\na\n"
}

test test_ls_time_sort_and_styles { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/old".write("")
  fp"{work}/new".write("")
  fs.set_times(fp"{work}/old", 1000000000000000000, 1000000000000000000)
  fs.set_times(fp"{work}/new", 1700000000000000000, 1700000000000000000)

  assert ls_in(ctx, work, ["-t"])?.text == "new\nold\n"
  assert ls_in(ctx, work, ["-tr"])?.text == "old\nnew\n"

  let full = ls_in(ctx, work, ["-og", "--full-time", "old"])?
  assert full.text == "-rw-r--r-- 1 0 2001-09-09 01:46:40.000000000 +0000 old\n", full.text
  assert ls_in(ctx, work, ["-og", "--time-style=long-iso", "old"])?.text == "-rw-r--r-- 1 0 2001-09-09 01:46 old\n"
  assert ls_in(ctx, work, ["-og", "--time-style=iso", "old"])?.text == "-rw-r--r-- 1 0 2001-09-09  old\n"
  assert ls_in(ctx, work, ["-og", "old"])?.text == "-rw-r--r-- 1 0 Sep  9  2001 old\n"
  assert ls_in(ctx, work, ["-og", "--time-style=+%Y/%m/%d %H:%M:%S %Z|%a %b %e", "old"])?.text == "-rw-r--r-- 1 0 2001/09/09 01:46:40 UTC|Sun Sep  9 old\n"
  assert ls_in(ctx, work, ["-og", "old"], {LC_ALL: "C", TZ: "UTC", TIME_STYLE: "long-iso"})?.text == "-rw-r--r-- 1 0 2001-09-09 01:46 old\n"
  let locale = ls_in(ctx, work, ["-og", "--time-style=locale", "old"], {LC_ALL: "C", TZ: "UTC"})?.text
  assert ls_in(ctx, work, ["-og", "--time-style=posix-full-iso", "old"], {LC_ALL: "C", TZ: "UTC"})?.text == locale
  assert ls_in(ctx, work, ["-og", "--time-style=+OLD\nNEW", "old"])?.text == "-rw-r--r-- 1 0 OLD old\n"
}

test test_ls_quoting_styles { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/one two".write("")

  for case in [
    ["--quoting-style=literal", "one two"],
    ["-N", "one two"],
    ["--quoting-style=shell", "'one two'"],
    ["--quoting-style=shell-always", "'one two'"],
    ["--quoting-style=shell-escape", "'one two'"],
    ["--quoting-style=c", "\"one two\""],
    ["-Q", "\"one two\""],
    ["-b", "one\\ two"],
    ["--quoting-style=locale", "'one two'"],
    ["--quoting-style=clocale", "\"one two\""],
  ] {
    let result = ls_in(ctx, work, [case[0]])?
    assert result.text == f"{case[1]}\n", f"{case[0]}: {result.text}"
  }

  fp"{work}/one two".remove()
  fp"{work}/tab\there".write("")
  fp"{work}/it's".write("")

  assert ls_in(ctx, work, ["--quoting-style=shell-escape"])?.text == "\"it's\"\n'tab'$'\\t''here'\n"
  assert ls_in(ctx, work, ["--quoting-style=c"])?.text == "\"it's\"\n\"tab\\there\"\n"
  assert ls_in(ctx, work, ["-q"])?.text == "it's\ntab?here\n"
  assert ls_in(ctx, work, ["--show-control-chars"])?.text == "it's\ntab\there\n"
  assert ls_in(ctx, work, [], {LC_ALL: "C", TZ: "UTC", QUOTING_STYLE: "c"})?.text == "\"it's\"\n\"tab\\there\"\n"

  let bad = ls_in(ctx, work, [], {LC_ALL: "C", TZ: "UTC", QUOTING_STYLE: "nope"})?
  assert bad.err == "ls: ignoring invalid value of environment variable QUOTING_STYLE: 'nope'\n", bad.err
}

test test_ls_indicators { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/d".mkdir()
  fp"{work}/plain".write("")
  fp"{work}/prog".write("", mode: 0o755)
  fp"{work}/ln".symlink(to: p"plain")
  fs.mkfifo(fp"{work}/fifo", 0o644)

  assert ls_in(ctx, work, ["-F"])?.text == "d/\nfifo|\nln@\nplain\nprog*\n"
  assert ls_in(ctx, work, ["--file-type"])?.text == "d/\nfifo|\nln@\nplain\nprog\n"
  assert ls_in(ctx, work, ["-p"])?.text == "d/\nfifo\nln\nplain\nprog\n"
  assert ls_in(ctx, work, ["-F", "--classify=never"])?.text == "d\nfifo\nln\nplain\nprog\n"
}

test test_ls_symlink_dereference_options { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/real".mkdir()
  fp"{work}/real/inside".write("")
  fp"{work}/ln".symlink(to: p"real")
  fp"{work}/dangling".symlink(to: p"missing")

  assert ls_in(ctx, work, ["ln"])?.text == "inside\n", "a symlink to a directory is followed on the command line"
  assert ls_in(ctx, work, ["-l", "-og", "--time-style=+T", "ln"])?.text == "lrwxrwxrwx 1 4 T ln -> real\n"
  assert "inside" in ls_in(ctx, work, ["-lH", "-og", "ln"])?.text
  assert ls_in(ctx, work, ["-d", "ln"])?.text == "ln\n"
  assert ls_in(ctx, work, ["-lLd", "-og", "--time-style=+T", "ln"])?.text.starts_with("drwx")

  # A plain listing never looks up targets, so a dangling link is only a name.
  let plain = ls_in(ctx, work, ["-L"])?
  assert plain.status == 0, plain.err
  assert plain.text == "dangling\nln\nreal\n", plain.text

  # Classifying needs the target's type, so the dangling link is reported.
  let classified = ls_in(ctx, work, ["-FL"])?
  assert classified.status == 1, classified.err
  assert classified.err == "ls: cannot access 'dangling': No such file or directory\n", classified.err
  assert classified.text == "dangling@\nln/\nreal/\n", classified.text
}

test test_ls_command_line_loop_is_listed_as_the_link { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/loop".symlink(to: p"loop")

  # A command-line symlink that cannot be traversed is shown as itself, and only
  # the explicit dereference options make it an error.
  let plain = ls_in(ctx, work, ["loop"])?
  assert plain.status == 0 and plain.text == "loop\n", plain.err
  assert ls_in(ctx, work, ["-l", "-og", "--time-style=+T", "loop"])?.text == "lrwxrwxrwx 1 4 T loop -> loop\n"
  assert ls_in(ctx, work, ["-H", "loop"])?.status == 2
  assert ls_in(ctx, work, ["-L", "loop"])?.status == 2
  assert ls_in(ctx, work, ["-L"])?.text == "loop\n", "a listed loop needs no lookup when only names are shown"
}

test test_ls_group_directories_first_counts_symlinks_to_directories { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/dir/b".mkdir()
  fp"{work}/dir/a".write("")
  fp"{work}/dir/bl".symlink(to: p"b")

  assert ls_in(ctx, work, ["--group", "dir"])?.text == "b\nbl\na\n", "a link to a directory groups with the directories"
  assert ls_in(ctx, work, ["--group-directories-first", "-d", "dir/a", "dir/b", "dir/bl"])?.text == "dir/b\ndir/bl\ndir/a\n"
}

test test_ls_version_sort_puts_tilde_backups_before_their_base { |ctx|
  let work = sandbox(ctx)?
  for name in ["zz", "zz~", "a", "a~"] {
    fp"{work}/{name}".write("")
  }

  assert ls_in(ctx, work, ["-v"])?.text == "a~\na\nzz~\nzz\n"
}

test test_ls_indicator_takes_types_from_the_directory_read { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/c/d".mkdir()
  fp"{work}/c".chmod(0o600)
  fs.mkfifo(fp"{work}/fifo", 0o644)

  # Without search permission on c, its entries cannot be statted, but their
  # types come from the directory read and need no stat.
  assert ls_in(ctx, work, ["-p", "c"])?.text == "d/\n", "a directory's type comes from the directory read"
  assert ls_in(ctx, work, ["-F", "c"])?.text == "d/\n", "classifying a directory needs no stat either"
  assert ls_in(ctx, work, ["--file-type"])?.text == "c/\nfifo|\n", "a fifo is read as other and then statted"
}

test test_ls_recursive_stops_at_directory_cycles { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/loop".mkdir()
  fp"{work}/loop/sub".symlink(to: ../loop)

  let result = ls_in(ctx, work, ["-RL", "loop"])?
  assert result.status == 2
  assert result.err == "ls: loop/sub: not listing already-listed directory\n", result.err
}

test test_ls_columns_across_commas_and_width { |ctx|
  let work = sandbox(ctx)?

  for name in ["test-width-1", "test-width-2", "test-width-3", "test-width-4"] {
    fp"{work}/{name}".write("")
  }

  assert ls_in(ctx, work, ["-C", "-w", "100"])?.text == "test-width-1  test-width-2  test-width-3  test-width-4\n"
  assert ls_in(ctx, work, ["-C", "-w=100"])?.text == "test-width-1  test-width-2  test-width-3  test-width-4\n"
  assert ls_in(ctx, work, ["-C", "-w", "50"])?.text == "test-width-1  test-width-3\ntest-width-2  test-width-4\n"
  assert ls_in(ctx, work, ["-x", "-w", "30"])?.text == "test-width-1  test-width-2\ntest-width-3  test-width-4\n"
  assert ls_in(ctx, work, ["-C", "-w", "25"])?.text == "test-width-1\ntest-width-2\ntest-width-3\ntest-width-4\n"
  assert ls_in(ctx, work, ["-C", "-w", "0"])?.text == "test-width-1  test-width-2  test-width-3  test-width-4\n"
  assert ls_in(ctx, work, ["-m", "-w", "30"])?.text == "test-width-1, test-width-2,\ntest-width-3, test-width-4\n"
  assert ls_in(ctx, work, ["-C", "--width=062"])?.text == "test-width-1  test-width-3\ntest-width-2  test-width-4\n", "a leading 0 is octal"
  assert ls_in(ctx, work, ["-C"], {LC_ALL: "C", TZ: "UTC", COLUMNS: "50"})?.text == "test-width-1  test-width-3\ntest-width-2  test-width-4\n"

  let garbage = ls_in(ctx, work, ["-C"], {LC_ALL: "C", TZ: "UTC", COLUMNS: "garbage"})?
  assert garbage.err == "ls: ignoring invalid width in environment variable COLUMNS: 'garbage'\n", garbage.err
}

test test_ls_columns_use_tabs_to_reach_tab_stops { |ctx|
  let work = sandbox(ctx)?

  for name in ["aaaaaaaa", "bbbb", "cccc", "dddddddd"] {
    fp"{work}/{name}".write("")
  }

  assert ls_in(ctx, work, ["-x", "-w18", "-T4"])?.text == "aaaaaaaa  bbbb\ncccc\t  dddddddd\n"
  assert ls_in(ctx, work, ["-C", "-w18", "-T4"])?.text == "aaaaaaaa  cccc\nbbbb\t  dddddddd\n"
  assert ls_in(ctx, work, ["-C", "-w18", "-T0"])?.text == "aaaaaaaa  cccc\nbbbb      dddddddd\n"
}

test test_ls_layout_aliases { |ctx|
  let work = sandbox(ctx)?
  for name in ["first", "second", "third", "fourth"] {
    fp"{work}/{name}".write("")
  }
  fp"{work}/two words".write("")

  assert ls_in(ctx, work, ["--long", "first"])?.text == ls_in(ctx, work, ["-l", "first"])?.text
  assert ls_in(ctx, work, ["--l", "two words"], {LC_ALL: "C", TZ: "UTC", QUOTING_STYLE: "c"})?.text == "two words\n"

  let columns = ls_in(ctx, work, ["-C", "-w", "40"])?.text

  for option in ["--format=column", "--format=columns", "--for=columns"] {
    assert ls_in(ctx, work, [option, "-w", "40"])?.text == columns, option
  }
}

test test_ls_commas_reserve_space_for_the_next_separator { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/a".write("")
  fp"{work}/bb".write("")
  fp"{work}/c".write("")
  fp"{work}/com,ma".write("")

  let result = ls_in(ctx, work, ["-m", "-w5", "a", "bb", "c"])?
  assert result.text == "a,\nbb, c\n", result.text

  let quoted = ls_in(ctx, work, ["-m", "--quoting-style=shell", "com,ma"])?
  assert quoted.text == "'com,ma'\n", quoted.text
}

test test_ls_commas_hide_newlines_unless_control_chars_are_shown { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/n\nl".write("")

  let hidden = ls_in(ctx, work, ["-m", "n\nl"])?
  assert hidden.text == "n?l\n", hidden.text

  let shown = ls_in(ctx, work, ["-m", "--show-control-chars", "n\nl"])?
  assert shown.text == "n\nl\n", shown.text
}

test test_ls_ignore_hide_and_backups { |ctx|
  let work = sandbox(ctx)?

  for name in ["README.md", "notes.md", "some_file", "backup~", ".hidden.yml"] {
    fp"{work}/{name}".write("")
  }

  assert ls_in(ctx, work, ["--ignore=*.md"])?.text == "backup~\nsome_file\n"
  assert ls_in(ctx, work, ["-I", "[!s]*"])?.text == "some_file\n"
  assert ls_in(ctx, work, ["-B"])?.text == "README.md\nnotes.md\nsome_file\n"
  assert ls_in(ctx, work, ["--hide=*.md"])?.text == "backup~\nsome_file\n"
  assert ls_in(ctx, work, ["-a", "--hide=*.md"])?.text == ".\n..\n.hidden.yml\nREADME.md\nbackup~\nnotes.md\nsome_file\n", "--hide yields to -a"
  assert ".hidden.yml" in ls_in(ctx, work, ["-a", "--ignore=*.yml"])?.text, "a leading period needs an explicit period"
  assert ".hidden.yml" not in ls_in(ctx, work, ["-a", "--ignore=.*.yml"])?.text
}

test test_ls_inode_size_and_block_totals { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/f".write(bytes.zero(5000)?)

  let inode = ls_in(ctx, work, ["-i"])?
  assert rx"^\d+ f\n$".matches(inode.text), inode.text
  assert ls_in(ctx, work, ["-s"], {LC_ALL: "C", TZ: "UTC", POSIXLY_CORRECT: "1"})?.text == "total 16\n16 f\n"
  assert ls_in(ctx, work, ["-s", "--block-size=512"])?.text == "total 16\n16 f\n"
  assert ls_in(ctx, work, ["-sh"])?.text == "total 8.0K\n8.0K f\n"
  assert ls_in(ctx, work, ["-og", "--time-style=+T", "-l"], {LC_ALL: "C", TZ: "UTC", LS_BLOCK_SIZE: "512"})?.text == "total 16\n-rw-r--r-- 1 10 T f\n"
  assert ls_in(ctx, work, ["-og", "--time-style=+T", "-lk"], {LC_ALL: "C", TZ: "UTC", LS_BLOCK_SIZE: "512"})?.text == "total 8\n-rw-r--r-- 1 10 T f\n"
}

test test_ls_color_uses_gnu_default_and_ls_colors_sequences { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/d".mkdir()
  fp"{work}/plain".write("")
  fp"{work}/run".write("", mode: 0o755)
  fp"{work}/dangling".symlink(to: p"missing")

  let colored = ls_in(ctx, work, ["--color=always"], {LC_ALL: "C", TZ: "UTC", LS_COLORS: "", TERM: "xterm", COLORTERM: ""})?
  assert colored.text == "\u{1b}[0m\u{1b}[01;34md\u{1b}[0m\n\u{1b}[01;36mdangling\u{1b}[0m\nplain\n\u{1b}[01;32mrun\u{1b}[0m\n", colored.text
  assert ls_in(ctx, work, ["--color=never"])?.text == "d\ndangling\nplain\nrun\n"
  assert ls_in(ctx, work, ["--color=auto"])?.text == "d\ndangling\nplain\nrun\n", "stdout is not a terminal"

  let custom = ls_in(
    ctx,
    work,
    ["--color=always", "plain", "run"],
    {LC_ALL: "C", TZ: "UTC", LS_COLORS: "*.xyz=1:ex=4;31"},
  )?
  assert custom.text == "plain\n\u{1b}[0m\u{1b}[4;31mrun\u{1b}[0m\n", custom.text

  let orphan = ls_in(
    ctx,
    work,
    ["--color=always", "dangling"],
    {LC_ALL: "C", TZ: "UTC", LS_COLORS: "ln=target:or=40:mi=34"},
  )?
  assert orphan.text == "\u{1b}[0m\u{1b}[40mdangling\u{1b}[0m\n", orphan.text

  let broken = ls_in(ctx, work, ["--color=always", "plain"], {LC_ALL: "C", TZ: "UTC", LS_COLORS: "di=1;35:stray"})?
  assert broken.text == "plain\n"
  assert broken.err == "ls: unparsable value for LS_COLORS environment variable\n", broken.err

  let prefix = ls_in(ctx, work, ["--color=always", "plain"], {LC_ALL: "C", TZ: "UTC", LS_COLORS: "qq=1:stray"})?
  assert prefix.err == "ls: unrecognized prefix: 'qq'\nls: unparsable value for LS_COLORS environment variable\n", prefix.err
}

test test_ls_color_preserves_nonzero_normal_sgr { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/f".write("")

  let result = ls_in(ctx, work, ["--color=always", "f"], {LC_ALL: "C", TZ: "UTC", LS_COLORS: "no=39"})?
  assert result.text == "\u{1b}[0m\u{1b}[39mf\u{1b}[0m\n", result.text
}

test test_ls_color_clears_to_eol_after_a_long_name_wraps { |ctx|
  let work = sandbox(ctx)?
  let name = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz.foo"
  fp"{work}/{name}".write("")

  let vars = {LC_ALL: "C", TZ: "UTC", TERM: "xterm", COLUMNS: "80", LS_COLORS: "*.foo=0;31;42", TIME_STYLE: "+T"}
  let result = ls_in(ctx, work, ["-og", "--color", name], vars)?
  let colored_name = f"\u{1b}[0m\u{1b}[0;31;42m{name}\u{1b}[0m\u{1b}[K"
  assert colored_name in result.text, result.text
}

test test_ls_color_fallback_requires_a_known_terminal { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/exe".write("", mode: 0o755)

  let plain = ls_in(ctx, work, ["--color=always", "exe"], {LC_ALL: "C", TZ: "UTC", LS_COLORS: "", TERM: "", COLORTERM: ""})?
  assert plain.text == "exe\n", plain.text

  let xterm = ls_in(ctx, work, ["--color=always", "exe"], {LC_ALL: "C", TZ: "UTC", LS_COLORS: "", TERM: "xterm", COLORTERM: ""})?
  assert xterm.text == "\u{1b}[0m\u{1b}[01;32mexe\u{1b}[0m\n", xterm.text

  let dumb = ls_in(ctx, work, ["--color=always", "exe"], {LC_ALL: "C", TZ: "UTC", LS_COLORS: "", TERM: "dumb", COLORTERM: ""})?
  assert dumb.text == "exe\n", dumb.text

  let colorterm = ls_in(ctx, work, ["--color=always", "exe"], {LC_ALL: "C", TZ: "UTC", LS_COLORS: "", TERM: "", COLORTERM: "true"})?
  assert colorterm.text == "\u{1b}[0m\u{1b}[01;32mexe\u{1b}[0m\n", colorterm.text
}

test test_ls_color_requires_a_capable_term_even_with_ls_colors { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/exe".write("", mode: 0o755)

  let dumb = ls_in(ctx, work, ["--color=always", "exe"], {LC_ALL: "C", TZ: "UTC", LS_COLORS: "ex=1;31", TERM: "dumb", COLORTERM: ""})?
  assert dumb.text == "exe\n", dumb.text
}

test test_ls_no_style_precedes_the_total_line { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/d".mkdir()
  fp"{work}/d/f".write("")

  let vars = {LC_ALL: "C", TZ: "UTC", LS_COLORS: "no=35", TERM: "xterm", COLORTERM: ""}
  let result = ls_in(ctx, work, ["-l", "--color=always", "d"], vars)?
  assert result.text.starts_with("\u{1b}[0m\u{1b}[35mtotal 0\n"), result.text
}

test test_ls_explicit_color_survives_format_options { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/dir".mkdir()
  let vars = {LC_ALL: "C", TZ: "UTC", LS_COLORS: "", TERM: "xterm", COLORTERM: ""}

  for args in [["--color=always", "-f"], ["-f", "--color=always"]] {
    assert ls_in(ctx, work, args, vars)?.text.find("\u{1b}[01;34m") != null
  }

  let zero_resets = ls_in(ctx, work, ["--color=always", "--zero"], vars)?
  assert zero_resets.text.find("\u{1b}") == null, zero_resets.text

  let color_after_zero = ls_in(ctx, work, ["--zero", "--color=always"], vars)?
  assert color_after_zero.text.find("\u{1b}[01;34m") != null, color_after_zero.text
}

test test_ls_explicit_literal_style_preserves_newlines { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/name\npart".write("")

  assert ls_in(ctx, work, ["--quoting-style=literal"])?.text == "name\npart\n"
  assert ls_in(ctx, work, [], {LC_ALL: "C", TZ: "UTC", QUOTING_STYLE: "literal"})?.text == "name\npart\n"
}

test test_ls_color_normal_attributes_apply_to_long_fields { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/exe".write("", mode: 0o755)
  fp"{work}/no_color".write("", mode: 0o444)

  let result = ls_in(
    ctx,
    work,
    ["-gGU", "--color", "exe", "no_color"],
    {LC_ALL: "C", TZ: "UTC", LS_COLORS: "no=7:ex=01;32", TIME_STYLE: "+norm"},
  )?
  assert "\u{1b}[7m" in result.text, result.text
  assert "\u{1b}[01;32mexe" in result.text, result.text
}

test test_ls_color_suffix_case_rules { |ctx|
  let work = sandbox(ctx)?

  for name in ["a.jpg", "B.JPG", "c.JpG"] {
    fp"{work}/{name}".write("")
  }

  let same = ls_in(
    ctx,
    work,
    ["--color=always", "-U1", "a.jpg", "B.JPG", "c.JpG"],
    {LC_ALL: "C", TZ: "UTC", LS_COLORS: "*.jpg=01;35"},
  )?
  assert same.text == "\u{1b}[0m\u{1b}[01;35ma.jpg\u{1b}[0m\n\u{1b}[01;35mB.JPG\u{1b}[0m\n\u{1b}[01;35mc.JpG\u{1b}[0m\n", same.text

  let split = ls_in(
    ctx,
    work,
    ["--color=always", "-U1", "a.jpg", "B.JPG", "c.JpG"],
    {LC_ALL: "C", TZ: "UTC", LS_COLORS: "*.jpg=01;35:*.JPG=01;35;46"},
  )?
  assert split.text == "\u{1b}[0m\u{1b}[01;35ma.jpg\u{1b}[0m\n\u{1b}[01;35;46mB.JPG\u{1b}[0m\nc.JpG\n", split.text
}

test test_ls_zero_terminates_with_nul { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/a b".write("")
  fp"{work}/c".write("")

  assert ls_in(ctx, work, ["--zero"])?.out == b"a b\0c\0"
  assert ls_in(ctx, work, ["--zero", "-m"])?.out == b"a b, c\0"
  assert ls_in(ctx, work, ["--zero", "--quoting-style=c"])?.out == b"\"a b\"\0\"c\"\0"
  assert ls_in(ctx, work, ["--quoting-style=c", "--zero"])?.out == b"a b\0c\0", "--zero resets the quoting style"
  assert ls_in(ctx, work, ["--dired", "-l", "--zero"])?.status == 2
}

test test_ls_dired_offsets_name_the_files { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/d".mkdir()
  fp"{work}/d/aa".write("")
  fp"{work}/d/bbb".write("")

  let result = ls_in(ctx, work, ["--dired", "-l", "d", "--time-style=+T"])?
  assert result.status == 0
  let at = result.text.find("//DIRED// ") ?? -1
  assert at > 0, result.text
  let line = result.text.byte_slice(at).lines()[0]
  let numbers = [n.parse_int() ?? -1 for n in line.fields()[1..]]
  assert numbers.len() == 4
  assert result.out[numbers[0]..numbers[1]] == b"aa"
  assert result.out[numbers[2]..numbers[3]] == b"bbb"
  assert "  total 0\n" in result.text
  assert result.text.ends_with("//DIRED-OPTIONS// --quoting-style=literal\n")

  let recursive = ls_in(ctx, work, ["--dired", "-lR", "d"])?
  assert "//SUBDIRED// 2 3\n" in recursive.text
}

test test_ls_dired_preserves_literal_newlines { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/d".mkdir()
  fp"{work}/d/n\nl".write("")

  let result = ls_in(ctx, work, ["--dired", "-l", "d", "--time-style=+T"])?
  assert " T n\nl\n" in result.text, result.text
}

test test_ls_hyperlink_wraps_names_in_osc_8 { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/sp ace".write("")

  let result = ls_in(ctx, work, ["--hyperlink"])?
  assert "\u{1b}]8;;file://" in result.text, result.text
  assert "/sp%20ace\u{1b}\\sp ace\u{1b}]8;;\u{1b}\\" in result.text, result.text
  assert ls_in(ctx, work, ["--hyperlink=never"])?.text == "sp ace\n"
}

test test_ls_hyperlink_names_the_canonical_location { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/testdir".mkdir()
  fp"{work}/testdir/inner".write("")
  fp"{work}/testdirl".symlink(to: p"testdir")

  # Entries and headers of a symlinked operand link to the directory they resolve to.
  let text = ls_in(ctx, work, ["--hyperlink", "testdirl", "testdir"])?.text
  assert "/testdir\u{1b}\\testdirl\u{1b}]8;;\u{1b}\\:" in text, text
  assert "/testdir/inner\u{1b}\\inner\u{1b}]8;;\u{1b}\\" in text, text
}

test test_ls_c_maybe_quotes_commas_without_escaping { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/com,ma".write("")

  assert ls_in(ctx, work, ["-m", "--quoting-style=c-maybe", "com,ma"])?.text == "\"com,ma\"\n"
  assert ls_in(ctx, work, ["-m", "--quoting-style=escape", "com,ma"])?.text == "com\\,ma\n"
}

test test_ls_width_is_an_inclusive_maximum { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/a".write("")
  fp"{work}/aa".write("")
  fp"{work}/b".write("")
  fp"{work}/c".write("")

  assert ls_in(ctx, work, ["-w4", "-x", "a", "b"])?.text == "a  b\n", "a line of exactly the width fits"
  assert ls_in(ctx, work, ["-w5", "-x", "aa", "b", "c"])?.text == "aa  b\nc\n"
  assert ls_in(ctx, work, ["-w5", "-C", "aa", "b", "c"])?.text == "aa  c\nb\n"
  assert ls_in(ctx, work, ["-w0", "-x", "a", "b"])?.text == "a  b\n", "a zero width is unlimited"
}

test test_ls_quoted_name_padding_stays_outside_the_color_sequence { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/d".mkdir()
  fp"{work}/d/a b".write("")
  fp"{work}/d/c.foo".write("")
  let vars = {LC_ALL: "C", TZ: "UTC", TERM: "xterm", LS_COLORS: "*.foo=31;42"}

  # Alignment is only applied in long format and in width-limited columns.
  assert ls_in(ctx, work, ["-1", "--color=always", "--quoting-style=shell-escape", "d"], vars)?.text == "'a b'\n\u{1b}[0m\u{1b}[31;42mc.foo\u{1b}[0m\n"
  assert ls_in(ctx, work, ["-x", "-w40", "--color=always", "--quoting-style=shell-escape", "d"], vars)?.text == "'a b'   \u{1b}[0m\u{1b}[31;42mc.foo\u{1b}[0m\n"
  assert ls_in(ctx, work, ["-x", "-w0", "--color=always", "--quoting-style=shell-escape", "d"], vars)?.text == "'a b'  \u{1b}[0m\u{1b}[31;42mc.foo\u{1b}[0m\n"
}

test test_ls_help_and_version_go_to_stdout { |ctx|
  let work = sandbox(ctx)?

  let help = ls_in(ctx, work, ["--help"])?
  assert help.status == 0
  assert help.err == ""
  assert "--version" in help.text
  assert "-l, --long" in help.text
  assert "column(s) -C" in help.text

  let version = ls_in(ctx, work, ["--version"])?
  assert version.status == 0
  assert version.text.starts_with("ls (XSH core)"), version.text
}
