type Ran = {status: Int, stdout: Str, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"", vars: Record = {LC_ALL: "C", TERM: "xterm"}, cwd: Path? = null) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "clear-capture")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/clear.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), cwd ?? root, vars, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8()?, stderr: err.read_text()?})
}

test test_clear_emits_ansi_without_touching_terminal { |ctx|
  assert invoke(ctx, [])?.stdout == "\x1b[H\x1b[2J\x1b[3J"
  assert invoke(ctx, ["-x"])?.stdout == "\x1b[H\x1b[2J"
}

test test_clear_rejects_unknown_terminal { |ctx|
  let result = invoke(ctx, ["-T", "missing-terminal"])?
  assert result.status == 1
  assert result.stdout == ""
}
