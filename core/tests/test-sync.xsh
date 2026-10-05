type Outcome = {status: Int, stdout: Str, stderr: Str}

proc run_applet(ctx: TestContext, root: Path, args: List[Str], tempdir: Str = "") [fs, process, error] -> Result[Outcome] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/sync.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {TMPDIR: tempdir, LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test test_sync_errors_continue_and_data_needs_files { |ctx|
  let root = test.temp_dir(ctx, name: "sync")?
  let result = run_applet(ctx, root, ["--data", "bad1", "bad2"])?
  assert result.status == 1
  assert result.stderr == "sync: error opening 'bad1': No such file or directory\nsync: error opening 'bad2': No such file or directory\n"
  assert run_applet(ctx, root, ["--data"])?.status == 1
  fp"{root}/file".write("data")
  assert run_applet(ctx, root, ["file"])?.status == 0
  assert run_applet(ctx, root, ["--data", "file"])?.status == 0
  assert run_applet(ctx, root, ["--file-system", "file"])?.status == 0
}
