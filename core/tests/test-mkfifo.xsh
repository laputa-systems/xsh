type Ran = {status: Int, stdout: Str, stderr: Str}

proc mkfifo_run(ctx: TestContext, root: Path, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/mkfifo.xsh"
  let argv = [ctx.xsh_bin.display(), script.display(), "--", @args]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_mkfifo_multiple_paths_and_mode { |ctx|
  let root = test.temp_dir(ctx, name: "mkfifo")?
  let result = mkfifo_run(ctx, root, ["-m", "0640", "one", "two"])?
  assert result.status == 0, result.stderr
  assert fs.stat(fp"{root}/one")?.kind == "fifo"
  assert fs.stat(fp"{root}/two")?.kind == "fifo"
  assert fp"{root}/one".metadata()?.mode % 512 == 0o640
}

test test_mkfifo_symbolic_mode { |ctx|
  let root = test.temp_dir(ctx, name: "mkfifo-symbolic")?
  let result = mkfifo_run(ctx, root, ["--mode=a=r", "pipe"])?
  assert result.status == 0, result.stderr
  assert fp"{root}/pipe".metadata()?.mode % 512 == 0o444
}

test test_mkfifo_duplicate_reports_failure { |ctx|
  let root = test.temp_dir(ctx, name: "mkfifo-existing")?
  fs.mkfifo(fp"{root}/pipe", 0o600)
  let result = mkfifo_run(ctx, root, ["pipe"])?
  assert result.status == 1
  assert "cannot create fifo" in result.stderr
}
