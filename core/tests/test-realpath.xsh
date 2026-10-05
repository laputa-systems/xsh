type Outcome = {status: Int, stdout: Str, stderr: Str}

proc run_applet(ctx: TestContext, root: Path, args: List[Str], tempdir: Str = "") [fs, process, error] -> Result[Outcome] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/realpath.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {TMPDIR: tempdir, LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test test_realpath { |ctx|
  let root = test.temp_dir(ctx, name: "realpath")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/realpath.xsh" -- $root
  assert output.trim() == root.resolve()?.display()
}


test test_realpath_physical_logical_relative_and_missing { |ctx|
  let root = test.temp_dir(ctx, name: "realpath-resolution")?
  fp"{root}/dir/inner".mkdir(parents: true)
  fp"{root}/alias".symlink(to: p"dir/inner")
  assert run_applet(ctx, root, ["alias/.."])?.stdout == root.display() + "/dir\n"
  assert run_applet(ctx, root, ["-L", "alias/.."])?.stdout == root.display() + "\n"
  assert run_applet(ctx, root, ["--relative-to=dir", "dir/inner"])?.stdout == "inner\n"
  assert run_applet(ctx, root, ["-m", "--relative-to=dir", "absent"])?.stdout == "../absent\n"
  assert run_applet(ctx, root, ["-e", "absent"])?.status == 1
  assert run_applet(ctx, root, ["-s", "alias"])?.stdout == root.display() + "/alias\n"
}
