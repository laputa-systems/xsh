type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Runs core/tac.xsh by its real path inside `root`, capturing both streams.
proc tac_run(ctx: TestContext, root: Path, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/tac.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_tac_reverses_lines_and_keeps_the_unterminated_record_first { |ctx|
  let root = test.temp_dir(ctx, name: "tac")?
  fp"{root}/in".write(b"1\n2\n3")

  assert tac_run(ctx, root, ["in"])?.stdout == b"32\n1\n"
  assert tac_run(ctx, root, [], b"100\n200\n300\n400\n500")?.stdout == b"500400\n300\n200\n100\n"
  assert tac_run(ctx, root, ["in", "-", "in"], b"x\ny\n")?.stdout == b"32\n1\ny\nx\n32\n1\n"
  assert tac_run(ctx, root, [], b"")?.stdout == b""
}

test test_tac_preserves_carriage_returns_and_non_utf8_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "tac")?

  assert tac_run(ctx, root, [], b"a\r\nb\r\n")?.stdout == b"b\r\na\r\n"
  assert tac_run(ctx, root, [], b"\xff\0\n\x80\n")?.stdout == b"\x80\n\xff\0\n"
}

test test_tac_before_attaches_the_separator_to_the_next_record { |ctx|
  let root = test.temp_dir(ctx, name: "tac")?

  assert tac_run(ctx, root, ["-b"], b"a\nb\n")?.stdout == b"\n\nba"
  assert tac_run(ctx, root, ["-b", "-s", ":"], b"100:200:300:400:500")?.stdout == b":500:400:300:200100"
  assert tac_run(ctx, root, ["-b"], b"")?.stdout == b""
}

test test_tac_custom_separators_match_from_the_end { |ctx|
  let root = test.temp_dir(ctx, name: "tac")?

  assert tac_run(ctx, root, ["-s", ":"], b"100:200:300")?.stdout == b"300200:100:"
  assert tac_run(ctx, root, ["-s", "xx"], b"axxbxx")?.stdout == b"bxxaxx"
  assert tac_run(ctx, root, ["--separator=xx"], b"axxx")?.stdout == b"axxx"
  assert tac_run(ctx, root, ["-s", "xx"], b"axxxx")?.stdout == b"xxaxx"
  assert tac_run(ctx, root, ["-b", "-s", "xx"], b"axxxx")?.stdout == b"xxxxa"
}

test test_tac_regex_separator { |ctx|
  let root = test.temp_dir(ctx, name: "tac")?

  assert tac_run(ctx, root, ["-r", "-s", "[0-9]+"], b"a1b22c")?.stdout == b"cb22a1"
  assert tac_run(ctx, root, ["-rb", "-s", ":+"], b":a::b:::c")?.stdout == b":::c::b:a"

  let bad = tac_run(ctx, root, ["-r", "-s", "x"], b"\xff")?
  assert bad.status == 1
  assert "valid UTF-8" in bad.stderr
}

test test_tac_empty_separator_is_a_usage_error { |ctx|
  let root = test.temp_dir(ctx, name: "tac")?
  let result = tac_run(ctx, root, ["-s", ""], b"a")?

  assert result.status == 1
  assert result.stderr == "tac: separator cannot be empty\n", result.stderr
}

test test_tac_reports_open_and_read_errors { |ctx|
  let root = test.temp_dir(ctx, name: "tac")?
  fp"{root}/dir".mkdir()
  fp"{root}/ok".write(b"1\n2\n")

  let result = tac_run(ctx, root, ["missing", "dir", "ok"])?
  assert result.status == 1
  assert result.stdout == b"2\n1\n"
  assert result.stderr == "tac: failed to open 'missing' for reading: No such file or directory\ntac: dir: read error: Is a directory\n", result.stderr
}

test test_tac_getopt_diagnostics_and_help { |ctx|
  let root = test.temp_dir(ctx, name: "tac")?

  let bad = tac_run(ctx, root, ["--definitely-invalid"])?
  assert bad.status == 1
  assert bad.stderr == "tac: unrecognized option '--definitely-invalid'\nTry 'tac --help' for more information.\n", bad.stderr

  let missing = tac_run(ctx, root, ["-s"])?
  assert missing.stderr == "tac: option requires an argument -- 's'\nTry 'tac --help' for more information.\n", missing.stderr

  assert "Usage: tac [OPTION]... [FILE]..." in tac_run(ctx, root, ["--help"])?.stdout.utf8()?
  assert tac_run(ctx, root, ["--vers"])?.stdout.starts_with(b"tac")
}
