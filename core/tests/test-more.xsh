type Ran = {status: Int, stdout: Str, stderr: Str, raw: Bytes}

# Runs core/more.xsh with a pipe on standard input and a file as standard
# output, so it prints without paging; the paging path needs a terminal on
# both and is covered by the uutils pty tests (a script cannot read a pty's
# master side).
proc more_run(
  ctx: TestContext,
  args: List[Str],
  input = b"",
  files: List[List[Str]] = [],
) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "more")?

  for pair in files {
    fp"{root}/{pair[0]}".write(pair[1])
  }

  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/more.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?
  Ok({status: status.exit_code()?, stdout: raw.utf8() ?? "", stderr: err.read_text()?, raw: raw})
}

test test_more_prints_a_piped_file_in_full_when_there_is_no_terminal { |ctx|
  let result = more_run(ctx, [], b"one\ntwo\nthree\n")?
  assert result.status == 0
  assert result.stdout == "one\ntwo\nthree\n", result.stdout
  assert result.stderr == ""
}

test test_more_prints_files_and_names_each_of_several { |ctx|
  let single = more_run(ctx, ["a.txt"], files: [["a.txt", "alpha\n"]])?
  assert single.stdout == "alpha\n"

  let several = more_run(ctx, ["a.txt", "b.txt"], files: [["a.txt", "alpha\n"], ["b.txt", "beta\n"]])?
  assert several.stdout == "::::::::::::::\na.txt\n::::::::::::::\nalpha\n::::::::::::::\nb.txt\n::::::::::::::\nbeta\n", several.stdout
}

test test_more_squeeze_collapses_blank_lines { |ctx|
  let result = more_run(ctx, ["-s"], b"line1\n\n\n\nline2\n  \n\nline3\n")?
  assert result.stdout == "line1\n\nline2\n  \nline3\n", result.stdout
  assert more_run(ctx, ["--squeeze"], b"a\n\n\nb\n")?.stdout == "a\n\nb\n"
}

test test_more_from_line_and_pattern_choose_the_first_line { |ctx|
  let numbered = b"line1\nline2\nline3\nline4\n"
  assert more_run(ctx, ["-F", "3"], numbered)?.stdout == "line3\nline4\n"
  assert more_run(ctx, ["--from-line", "1"], numbered)?.stdout == "line1\nline2\nline3\nline4\n"
  assert more_run(ctx, ["-F", "0"], numbered)?.stdout == "line1\nline2\nline3\nline4\n"
  assert more_run(ctx, ["-P", "line3"], numbered)?.stdout == "line3\nline4\n"
  assert more_run(ctx, ["--pattern", "-1"], b"x\n-1\ny\n")?.stdout == "-1\ny\n", "a pattern may start with a dash"
  assert more_run(ctx, ["-P", "absent"], numbered)?.stdout == "line1\nline2\nline3\nline4\n", "an unmatched pattern starts at the top"
}

test test_more_plain_strips_underline_and_bold_overstrikes { |ctx|
  let marked = b"_\x08h_\x08i x\x08x\x08xy\n"
  assert more_run(ctx, ["-u"], marked)?.stdout == "hi xy\n"
  assert more_run(ctx, ["--plain"], marked)?.stdout == "hi xy\n"
  assert more_run(ctx, [], marked)?.stdout == "_\u{8}h_\u{8}i x\u{8}x\u{8}xy\n", "without -u the overstrikes are kept"
}

test test_more_keeps_non_utf8_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "more-bytes")?
  let input = fp"{root}/in"
  input.write(b"ok\n\xff\xfe line\n")
  let result = more_run(ctx, [input.display()])?
  assert result.status == 0
  assert result.stderr == ""
  assert result.raw == b"ok\n\xff\xfe line\n"
}

test test_more_reports_directories_and_missing_files_and_goes_on { |ctx|
  let root = test.temp_dir(ctx, name: "more-errors")?
  fp"{root}/folder".mkdir()
  fp"{root}/real.txt".write("real\n")
  let result = more_run(ctx, [fp"{root}/folder".display(), fp"{root}/absent".display(), fp"{root}/real.txt".display()])?
  assert result.status == 0
  assert f"more: '{root}/folder' is a directory.\n" in result.stderr, result.stderr
  assert f"more: cannot open '{root}/absent': No such file or directory\n" in result.stderr, result.stderr
  assert "real\n" in result.stdout, result.stdout
}

test test_more_rejects_bad_numbers_before_reading_anything { |ctx|
  for case in [
    ["--lines", "-10"],
    ["-n", "x"],
    ["--number", "70000"],
    ["--from-line", "-10"],
    ["-F", "1.5"],
  ] {
    let result = more_run(ctx, case, b"data\n")?
    assert result.status == 1, f"more {case.join(" ")}"
    assert result.stdout == ""
    assert result.stderr.starts_with("more: invalid argument"), result.stderr
  }

  assert more_run(ctx, ["-n", "0", "--number", "0", "-F", "0"], b"ok\n")?.stdout == "ok\n"
  assert more_run(ctx, ["-n", "10"], b"ok\n")?.stdout == "ok\n"
}

test test_more_accepts_every_documented_switch { |ctx|
  for flag in [
    "-c",
    "--clean-print",
    "-p",
    "--print-over",
    "-d",
    "--silent",
    "-f",
    "--logical",
    "-l",
    "--no-pause",
    "-e",
    "--exit-on-eof",
  ] {
    let result = more_run(ctx, [flag], b"data\n")?
    assert result.status == 0, flag
    assert result.stdout == "data\n", flag
  }
}

test test_more_invalid_option_uses_getopt_wording { |ctx|
  let result = more_run(ctx, ["--invalid"])?
  assert result.status == 1
  assert result.stderr == "more: unrecognized option '--invalid'\nTry 'more --help' for more information.\n", result.stderr
}

test test_more_help_and_version_go_to_stdout { |ctx|
  let help = more_run(ctx, ["--help"])?
  assert help.status == 0
  assert help.stderr == ""
  assert help.stdout.starts_with("Usage: more [OPTIONS] FILE...\n")
  let version = more_run(ctx, ["--version"])?
  assert version.status == 0
  assert version.stdout.starts_with("more (XSH core)")
}

test test_more_non_utf8_operand_is_opened_by_its_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "more-non-utf8")?
  Path.parse_bytes(bytes.concat([bytes.from_text(f"{root}/"), b"\xff\xfe.txt"]))?.write(b"raw name\n")

  let words: List[Union[Str, Path]] = [ctx.xsh_bin, fp"{ctx.core_dir}/more.xsh", Path.parse_bytes(b"\xff\xfe.txt")?]
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let status = process.run(process.command_argv(ctx.xsh_bin, words, root, {LC_ALL: "C"}, b"", out, err))?
  assert status.exit_code()? == 0
  assert err.read_text()? == ""
  assert out.read_bytes()? == b"raw name\n"
}
