type Ran = {status: Int, stdout: Str, stderr: Str, bytes: Bytes}

# Runs core/printenv.xsh by its real path (so the invoked name is printenv and
# `lib.gnu` resolves beside it), capturing both streams.
proc applet_run(
  ctx: TestContext,
  args: List[Str],
  vars: Record = {LC_ALL: "C"},
  stdin = b"",
) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "printenv")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/printenv.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, vars, stdin, out, err)
  let status = process.run(plan)?
  let raw = out.read_bytes()?

  Ok({status: status.exit_code()?, stdout: raw.utf8() ?? "", stderr: err.read_text()?, bytes: raw})
}

test test_printenv_without_names_prints_every_pair { |ctx|
  let result = applet_run(ctx, [], vars: {KEY: "VALUE", HOME: "FOO"})?
  assert result.status == 0
  assert "KEY=VALUE\n" in result.stdout
  assert "HOME=FOO\n" in result.stdout
}

test test_printenv_prints_values_and_fails_when_a_name_is_missing { |ctx|
  let found = applet_run(ctx, ["KEY"], vars: {KEY: "VALUE", FOO: "BAR"})?
  assert found.status == 0
  assert found.stdout == "VALUE\n"

  let mixed = applet_run(ctx, ["FOO_MISSING", "KEY"], vars: {KEY: "VALUE"})?
  assert mixed.status == 1
  assert mixed.stdout == "VALUE\n", "later names are still printed"
  assert mixed.stderr == "", "a missing variable is silent"
}

test test_printenv_names_with_equals_never_match { |ctx|
  let result = applet_run(ctx, ["KEY=VALUE", "KEY"], vars: {KEY: "VALUE"})?
  assert result.status == 1
  assert result.stdout == "VALUE\n"
  assert result.stderr == ""
}

test test_printenv_null_ends_lines_with_nul { |ctx|
  let named = applet_run(ctx, ["-0", "KEY", "FOO"], vars: {KEY: "VALUE", FOO: "BAR"})?
  assert named.stdout == "VALUE\0BAR\0"
  assert "KEY=VALUE\0" in applet_run(ctx, ["--null"], vars: {KEY: "VALUE"})?.stdout
}

test test_printenv_usage_errors_exit_with_status_two { |ctx|
  let bad = applet_run(ctx, ["-/"])?
  assert bad.status == 2
  assert bad.stderr == "printenv: invalid option -- '/'\nTry 'printenv --help' for more information.\n", bad.stderr
  assert applet_run(ctx, ["--definitely-invalid"])?.status == 2
}

test test_printenv_help_and_version { |ctx|
  assert "Print the values of the specified environment VARIABLE(s)." in applet_run(ctx, ["--help"])?.stdout
  assert applet_run(ctx, ["--version"])?.stdout.starts_with("printenv ")
}

test test_printenv_values_are_printed_as_bytes { |ctx|
  let raw = Path.parse_bytes(b"/tmp/lib.so\xff")?
  let vars = {LC_ALL: "C", RAW_VALUE: raw, PATHLIKE: "a:b//c/"}

  let named = applet_run(ctx, ["RAW_VALUE"], vars: vars)?
  assert named.status == 0
  assert named.bytes == b"/tmp/lib.so\xff\n"

  assert applet_run(ctx, ["PATHLIKE"], vars: vars)?.stdout == "a:b//c/\n", "a value is not split or normalized"

  let listed = applet_run(ctx, [], vars: vars)?
  assert b"RAW_VALUE=/tmp/lib.so\xff" in listed.bytes.lines()
}
