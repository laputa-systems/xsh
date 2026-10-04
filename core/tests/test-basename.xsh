type Ran = {status: Int, stdout: Str, stderr: Str}

# Runs core/basename.xsh by its real path (so the invoked name is `basename` and
# `lib.gnu` resolves beside it), capturing both streams to files.
proc basename_run(ctx: TestContext, args: List[Str], phrase: Str = "") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "basename")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/basename.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(
    ctx.xsh_bin,
    argv,
    root,
    {XSH_EXECUTION_PHRASE: phrase, LC_ALL: "C"},
    b"",
    out,
    err,
  )
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_basename_basic { |ctx|
  let result = basename_run(ctx, ["/tmp/demo.txt"])?
  assert result.stdout == "demo.txt\n"
  assert result.stderr == ""
}

test test_basename_trailing_separators_and_root { |ctx|
  for case in [
    ["foo/bar", "bar\n"],
    ["foo/bar/", "bar\n"],
    ["foo/bar///", "bar\n"],
    ["foo/./", ".\n"],
    ["foo/.//", ".\n"],
    ["/.", ".\n"],
    ["/", "/\n"],
    ["//", "/\n"],
    ["///", "/\n"],
  ] {
    let result = basename_run(ctx, [case[0]])?
    assert result.stdout == case[1], f"basename {case[0]}: {result.stdout}"
  }
}

test test_basename_simple_format_suffix { |ctx|
  assert basename_run(ctx, ["/tmp/demo.txt", ".txt"])?.stdout == "demo\n"
  assert basename_run(ctx, ["/tmp/demo.txt", "demo.txt"])?.stdout == "demo.txt\n", "suffix equal to the name is kept"
  assert basename_run(ctx, ["/tmp/demo.txt", "x.txt"])?.stdout == "demo.txt\n"
  assert basename_run(ctx, ["a-a", "-a"])?.stdout == "a\n", "operands after the first are not options"
  assert basename_run(ctx, ["a-z", "-z"])?.stdout == "a\n"
}

test test_basename_suffix_and_multiple { |ctx|
  let result = basename_run(ctx, ["-a", "-s", ".txt", "/tmp/demo.txt", "/tmp/other.txt"])?
  assert result.stdout == "demo\nother\n"
  assert basename_run(ctx, ["--multiple", "a/b", "c/d"])?.stdout == "b\nd\n"
  assert basename_run(ctx, ["--suffix=.c", "x/a.c", "y/b.c"])?.stdout == "a\nb\n", "-s implies -a"
}

test test_basename_repeated_options_last_wins { |ctx|
  assert basename_run(ctx, ["-s", ".a", "-s", ".b", "x.a", "y.b"])?.stdout == "x.a\ny\n"
  assert basename_run(ctx, ["-a", "-a", "p/q", "r/s"])?.stdout == "q\ns\n"
}

test test_basename_zero_terminates_with_nul { |ctx|
  let result = basename_run(ctx, ["-z", "-a", "a/b", "c/d"])?
  assert result.stdout == "b\0d\0"
  assert basename_run(ctx, ["--zero", "a/b"])?.stdout == "b\0"
}

test test_basename_abbreviated_long_option { |ctx|
  assert basename_run(ctx, ["--mult", "a/b", "c/d"])?.stdout == "b\nd\n"
}

test test_basename_missing_operand_is_a_gnu_usage_error { |ctx|
  let result = basename_run(ctx, [])?
  assert result.status == 1
  assert result.stdout == ""
  assert result.stderr == "basename: missing operand\nTry 'basename --help' for more information.\n", result.stderr
}

test test_basename_extra_operand_is_a_gnu_usage_error { |ctx|
  let result = basename_run(ctx, ["a", "b", "c"])?
  assert result.status == 1
  assert result.stdout == ""
  assert result.stderr == "basename: extra operand 'c'\nTry 'basename --help' for more information.\n", result.stderr
}

test test_basename_invalid_option_uses_getopt_wording { |ctx|
  let short = basename_run(ctx, ["-q", "/foo/bar"])?
  assert short.status == 1
  assert short.stdout == ""
  assert short.stderr == "basename: invalid option -- 'q'\nTry 'basename --help' for more information.\n", short.stderr

  let long = basename_run(ctx, ["--definitely-invalid"])?
  assert long.status == 1
  assert long.stderr == "basename: unrecognized option '--definitely-invalid'\nTry 'basename --help' for more information.\n", long.stderr

  let missing = basename_run(ctx, ["-s"])?
  assert missing.status == 1
  assert missing.stderr == "basename: option requires an argument -- 's'\nTry 'basename --help' for more information.\n", missing.stderr
}

test test_basename_try_hint_honors_the_execution_phrase { |ctx|
  let result = basename_run(ctx, [], phrase: "/opt/multicall basename")?
  assert result.stderr == "basename: missing operand\nTry '/opt/multicall basename --help' for more information.\n", result.stderr
}

test test_basename_help_and_version_go_to_stdout { |ctx|
  let help = basename_run(ctx, ["--help"])?
  assert help.status == 0
  assert help.stderr == ""
  assert "Usage: basename NAME [SUFFIX]" in help.stdout

  let version = basename_run(ctx, ["--version"])?
  assert version.status == 0
  assert version.stderr == ""
  assert version.stdout.starts_with("basename")
}

test test_basename_runs_as_executable_shebang_script { |ctx|
  if ! p"/bin/xsh".exists()? {
    test.skip("/bin/xsh is not installed")
  }

  let script = fp"{ctx.core_dir}/basename.xsh"
  script.chmod(0o755)
  let output = run.text $script -- /tmp/demo.txt ?

  assert output == """demo.txt
"""
}
