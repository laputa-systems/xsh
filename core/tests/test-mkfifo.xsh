type Outcome = {status: Int, stdout: Str, stderr: Str}

proc run_applet(ctx: TestContext, root: Path, args: List[Str], tempdir: Str = "") [fs, process, error] -> Result[Outcome] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/mkfifo.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {TMPDIR: tempdir, LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test test_mkfifo_symbolic_modes_and_continue_after_error { |ctx|
  let root = test.temp_dir(ctx, name: "mkfifo")?
  let result = run_applet(ctx, root, ["-m", "u=rw,g=r,o=", "pipe", "pipe", "other"])?
  assert result.status == 1
  assert "File exists" in result.stderr
  assert fs.stat(fp"{root}/pipe")?.kind == "fifo"
  assert fs.stat(fp"{root}/pipe")?.mode.bit_and(0o777) == 0o640
  assert fs.stat(fp"{root}/other")?.kind == "fifo"
}

test test_mkfifo_invalid_modes_do_not_create { |ctx|
  let root = test.temp_dir(ctx, name: "mkfifo-invalid")?
  assert run_applet(ctx, root, ["-m", "1777", "pipe"])?.status == 1
  assert ! fp"{root}/pipe".exists()?
  assert run_applet(ctx, root, ["-m", "u=invalid", "pipe"])?.status == 1
}


test test_mkfifo_copied_permissions_and_multiple_mode_operations { |ctx|
  let root = test.temp_dir(ctx, name: "mkfifo-mode-copy")?
  let result = run_applet(ctx, root, ["-m", "u=rw-x,g=u-w,o=", "pipe"])?
  assert result.status == 0, result.stderr
  assert fs.stat(fp"{root}/pipe")?.mode.bit_and(0o777) == 0o640
}
