type Ran = {status: Int, stdout: Str, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"", vars: Record = {LC_ALL: "C", TERM: "xterm"}, cwd: Path? = null) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "reset-capture")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/reset.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), cwd ?? root, vars, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8()?, stderr: err.read_text()?})
}

test test_reset_help_avoids_terminal_mutation { |ctx|
  let result = invoke(ctx, ["--help"])?
  assert result.status == 0
  assert result.stdout.starts_with("Usage: reset")
}

test test_reset_sane_settings_use_mock { |ctx|
  let log = test.temp_path(ctx, name: "reset-mock-log")
  test.unix_fake(ctx, {log: log})
  let source = fp"{ctx.core_dir}/reset.xsh".read_text()?
  let result = test.run_script(ctx, source, env: {TERM: "xterm", XSH_MODULE_PATH: ctx.core_dir.display()})?
  assert result.success, result.stderr
  assert result.stdout == "\x1bc\x1b[?25h"
  assert log.read_text()?.find("set_tty_attrs") != null
}

test test_reset_no_initialization_still_restores_modes_with_mock { |ctx|
  let log = test.temp_path(ctx, name: "reset-no-init-log")
  test.unix_fake(ctx, {log: log})
  let result = test.run_script(ctx, fp"{ctx.core_dir}/reset.xsh".read_text()?, args: ["-I"], env: {TERM: "xterm", XSH_MODULE_PATH: ctx.core_dir.display()})?
  assert result.success, result.stderr
  assert result.stdout == ""
  assert log.read_text()?.find("set_tty_attrs") != null
}
