type Ran = {status: Int, stdout: Str, stderr: Str}

# Runs core/wc.xsh by its real path inside `dir`, with `input` as standard
# input, capturing both streams to files.
proc wc_in(ctx: TestContext, dir: Path, args: List[Str], input: Bytes = b"", vars: Record = {LC_ALL: "C"}) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "wc-run")?
  let stdin = test.temp_file(ctx, name: "wc-stdin", contents: input)?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/wc.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, dir, vars, stdin, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

proc wc_run(ctx: TestContext, args: List[Str], input: Bytes = b"") [fs, process, error] -> Result[Ran] {
  wc_in(ctx, test.temp_dir(ctx, name: "wc-cwd")?, args, input)
}

proc wc_text(ctx: TestContext, args: List[Str], input: Bytes = b"") [fs, process, error] -> Result[Str] {
  let result = wc_run(ctx, args, input)?
  assert result.status == 0, f"wc {args.join(" ")}: {result.stderr}"
  Ok(result.stdout)
}

test test_wc_default_counts_and_stdin_width { |ctx|
  assert wc_text(ctx, [], b"one two\nthree\n")? == "      2       3      14\n", "standard input uses a width of 7"
  assert wc_text(ctx, ["-"], b"one two\nthree\n")? == "      2       3      14 -\n"
  assert wc_text(ctx, ["-l"], b"a\nb\n")? == "2\n", "a single count has width 1"
  assert wc_text(ctx, ["-c"], b"abc")? == "3\n"
}

test test_wc_count_selection_and_order { |ctx|
  assert wc_text(ctx, ["-lwmcL"], b"ab cd\nefg\n")? == "      2       3      10      10       5\n"
  assert wc_text(ctx, ["-c", "-l"], b"a\nb\n")? == "      2       4\n", "counts print in a fixed order"
  assert wc_text(ctx, ["--lines", "--words"], b"a b\n")? == "      1       2\n"
  assert wc_text(ctx, ["-ll", "-l"], b"a\n")? == "1\n", "repeated options are legal"
}

test test_wc_characters_and_invalid_bytes { |ctx|
  assert wc_text(ctx, ["-mc"], bytes.from_text("héllo\n"))? == "      6       7\n"
  assert wc_text(ctx, [], b"a \xff b\n")? == "      1       3       6\n", "an invalid byte is a word character and no character"
  assert wc_text(ctx, ["-m"], b"a\xffb")? == "2\n"
  assert wc_text(ctx, ["-lc"], b"\xff\n\0x")? == "      1       4\n", "line and byte counts never decode"
}

test test_wc_words_follow_unicode_white_space_unless_posix { |ctx|
  let text = bytes.from_text("word\u{00A0}word")
  assert wc_text(ctx, ["-w"], text)? == "2\n"
  assert wc_text(ctx, ["-w"], bytes.from_text("foo 💐 bar\n"))? == "3\n"
  assert wc_text(ctx, ["-w"], b"\x01\n")? == "1\n", "control characters are word characters"

  let strict = wc_in(ctx, test.temp_dir(ctx, name: "wc-cwd")?, ["-w"], text, {LC_ALL: "C", POSIXLY_CORRECT: "1"})?
  assert strict.stdout == "1\n"
}

test test_wc_max_line_length_is_display_width { |ctx|
  assert wc_text(ctx, ["-L"], b"\n123456")? == "6\n"
  assert wc_text(ctx, ["-L"], b"a\tb\n")? == "9\n", "a tab advances to the next multiple of 8"
  assert wc_text(ctx, ["-L"], b"abcdefgh\tb")? == "17\n"
  assert wc_text(ctx, ["-L"], bytes.from_text("日本語\n"))? == "6\n", "wide characters take two columns"
  assert wc_text(ctx, ["-L"], bytes.from_text("e\u{0301}x"))? == "2\n", "combining marks take none"
  assert wc_text(ctx, ["-L"], b"abc\rabcdef\x0cx")? == "6\n", "carriage returns and form feeds end a line"
}

test test_wc_files_width_follows_total_size { |ctx|
  let dir = test.temp_dir(ctx, name: "wc-files")?
  fp"{dir}/small.txt".write("a b\n")?
  fp"{dir}/big.txt".write(["x" for n in range(600)].join("\n") + "\n")?

  let result = wc_in(ctx, dir, ["small.txt", "big.txt"])?
  assert result.status == 0
  assert result.stdout == "   1    2    4 small.txt\n 600  600 1200 big.txt\n 601  602 1204 total\n", result.stdout

  assert wc_in(ctx, dir, ["-lw", "small.txt"])?.stdout == "1 2 small.txt\n"
  assert wc_in(ctx, dir, ["-c", "big.txt"])?.stdout == "1200 big.txt\n"
}

test test_wc_total_modes_and_abbreviations { |ctx|
  let dir = test.temp_dir(ctx, name: "wc-total")?
  fp"{dir}/a".write("x y\n")?
  fp"{dir}/b".write("z\n")?

  assert wc_in(ctx, dir, ["a"])?.stdout == "1 2 4 a\n"
  assert wc_in(ctx, dir, ["a", "--total=always"])?.stdout == "1 2 4 a\n1 2 4 total\n"
  assert wc_in(ctx, dir, ["a", "b", "--total=never"])?.stdout == "1 2 4 a\n1 1 2 b\n"
  assert wc_in(ctx, dir, ["a", "b", "--total=only"])?.stdout == "2 3 6\n"
  assert wc_in(ctx, dir, ["a", "--tot=al"])?.stdout == "1 2 4 a\n1 2 4 total\n"
  assert wc_in(ctx, dir, ["--total=always", "--total=never", "a"])?.stdout == "1 2 4 a\n", "the last --total wins"

  let bad = wc_in(ctx, dir, ["--total=x", "a"])?
  assert bad.status == 1
  assert bad.stderr == "wc: invalid argument 'x' for '--total'\nValid arguments are:\n  - 'auto'\n  - 'always'\n  - 'only'\n  - 'never'\nTry 'wc --help' for more information.\n", bad.stderr

  let vague = wc_in(ctx, dir, ["--total=a", "a"])?
  assert vague.status == 1
  assert vague.stderr.starts_with("wc: ambiguous argument 'a' for '--total'\n"), vague.stderr
}

test test_wc_files0_from { |ctx|
  let dir = test.temp_dir(ctx, name: "wc-files0")?
  fp"{dir}/a".write("x y\n")?
  fp"{dir}/b".write("z\n")?
  fp"{dir}/list".write(b"a\0b\0")?

  let listed = wc_in(ctx, dir, ["--files0-from=list"])?
  assert listed.stdout == "1 2 4 a\n1 1 2 b\n2 3 6 total\n", listed.stdout

  let piped = wc_in(ctx, dir, ["--files0-from=-"], b"a\0b")?
  assert piped.stdout == "1 2 4 a\n1 1 2 b\n2 3 6 total\n"

  let both = wc_in(ctx, dir, ["--files0-from=list", "a"])?
  assert both.status == 1
  assert both.stdout == ""
  assert both.stderr == "wc: extra operand 'a'\nfile operands cannot be combined with --files0-from\nTry 'wc --help' for more information.\n", both.stderr

  let empty_names = wc_in(ctx, dir, ["--files0-from=-"], b"\0a\0\0")?
  assert empty_names.status == 1
  assert empty_names.stdout == "1 2 4 a\n1 2 4 total\n"
  assert empty_names.stderr == "wc: -:1: invalid zero-length file name\nwc: -:3: invalid zero-length file name\n", empty_names.stderr

  let dash = wc_in(ctx, dir, ["--files0-from=-"], b"-")?
  assert dash.status == 1
  assert dash.stderr == "wc: when reading file names from standard input, no file name of '-' allowed\n", dash.stderr

  let missing = wc_in(ctx, dir, ["--files0-from=nope"])?
  assert missing.status == 1
  assert missing.stderr == "wc: cannot open 'nope' for reading: No such file or directory\n", missing.stderr
}

test test_wc_errors_are_reported_per_input_and_set_the_status { |ctx|
  let dir = test.temp_dir(ctx, name: "wc-errors")?
  fp"{dir}/a".write("x\n")?
  fp"{dir}/sub".mkdir()?

  let result = wc_in(ctx, dir, ["a", "missing", "sub", "a"])?
  assert result.status == 1
  assert result.stdout == "      1       1       2 a\n      0       0       0 sub\n      1       1       2 a\n      2       2       4 total\n", result.stdout
  assert result.stderr == "wc: missing: No such file or directory\nwc: sub: Is a directory\n", result.stderr

  let quoted = wc_in(ctx, dir, ["no such file"])?
  assert quoted.stderr == "wc: 'no such file': No such file or directory\n", quoted.stderr
}

test test_wc_names_with_newlines_are_quoted_in_the_output { |ctx|
  let dir = test.temp_dir(ctx, name: "wc-quote")?
  fp"{dir}/12\n34.txt".write("")?

  assert wc_in(ctx, dir, ["12\n34.txt"])?.stdout == "0 0 0 '12'$'\\n''34.txt'\n"
}

test test_wc_debug_is_an_explicit_failure { |ctx|
  let result = wc_run(ctx, ["--debug"])?
  assert result.status == 1
  assert result.stderr.starts_with("wc: option '--debug' is not supported"), result.stderr
}

test test_wc_help_and_version { |ctx|
  let help = wc_run(ctx, ["--help"])?
  assert help.status == 0
  assert "Usage: wc [OPTION]... [FILE]..." in help.stdout

  let version = wc_run(ctx, ["--version"])?
  assert version.status == 0
  assert version.stdout.starts_with("wc")

  let unknown = wc_run(ctx, ["-q"])?
  assert unknown.status == 1
  assert unknown.stderr == "wc: invalid option -- 'q'\nTry 'wc --help' for more information.\n", unknown.stderr
}
