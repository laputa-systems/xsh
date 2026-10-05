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

  let out = fp"{work}/../stdout"
  let err = fp"{work}/../stderr"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, work, vars, b"", out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?

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
  assert blocked.err == "ls: cannot access 'ok/inside': Not a directory\n", blocked.err
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
  assert style.err.starts_with("ls: invalid argument 'bogus' for 'time style'\nValid arguments are:\n"), style.err
  assert ls_in(ctx, work, ["--time-style=bogus"])?.status == 0, "the style is checked only for long listings"
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

  assert ls_in(ctx, work, ["ln"])?.text == "inside\n", "a symlink to a directory is followed on the command line"
  assert ls_in(ctx, work, ["-l", "-og", "--time-style=+T", "ln"])?.text == "lrwxrwxrwx 1 4 T ln -> real\n"
  assert "inside" in ls_in(ctx, work, ["-lH", "-og", "ln"])?.text
  assert ls_in(ctx, work, ["-d", "ln"])?.text == "ln\n"
  assert ls_in(ctx, work, ["-lLd", "-og", "--time-style=+T", "ln"])?.text.starts_with("drwx")
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

  let colored = ls_in(ctx, work, ["--color=always"])?
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

test test_ls_hyperlink_wraps_names_in_osc_8 { |ctx|
  let work = sandbox(ctx)?
  fp"{work}/sp ace".write("")

  let result = ls_in(ctx, work, ["--hyperlink"])?
  assert "\u{1b}]8;;file://" in result.text, result.text
  assert "/sp%20ace\u{1b}\\sp ace\u{1b}]8;;\u{1b}\\" in result.text, result.text
  assert ls_in(ctx, work, ["--hyperlink=never"])?.text == "sp ace\n"
}

test test_ls_help_and_version_go_to_stdout { |ctx|
  let work = sandbox(ctx)?

  let help = ls_in(ctx, work, ["--help"])?
  assert help.status == 0
  assert help.err == ""
  assert "--version" in help.text

  let version = ls_in(ctx, work, ["--version"])?
  assert version.status == 0
  assert version.text.starts_with("ls (XSH core)"), version.text
}
