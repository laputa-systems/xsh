type Outcome = {status: Int, stdout: Str, stderr: Str}

proc run_applet(ctx: TestContext, root: Path, args: List[Str], tempdir: Str = "") [fs, process, error] -> Result[Outcome] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/pathchk.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {TMPDIR: tempdir, LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test test_pathchk_portability_and_missing_paths { |ctx|
  let root = test.temp_dir(ctx, name: "pathchk")?
  assert run_applet(ctx, root, ["missing/file"])?.status == 0
  assert run_applet(ctx, root, ["-p", "safe/file-name"])?.status == 0
  assert run_applet(ctx, root, ["-p", "unsafe/$name"])?.status == 1
  assert run_applet(ctx, root, ["-p", "123456789012345"])?.status == 1
  assert run_applet(ctx, root, ["-P", "dir/-file"])?.status == 1
  assert run_applet(ctx, root, ["--portability", ""])?.stderr == "pathchk: empty file name\n"
}
