type Outcome = {status: Int, stdout: Str, stderr: Str}

proc run_applet(ctx: TestContext, root: Path, args: List[Str], tempdir: Str = "") [fs, process, error] -> Result[Outcome] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/mktemp.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {TMPDIR: tempdir, LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test test_mktemp_templates_suffix_and_private_modes { |ctx|
  let root = test.temp_dir(ctx, name: "mktemp")?
  let file = run_applet(ctx, root, ["--suffix=.txt", "file.XXXXXX"])?
  assert file.status == 0, file.stderr
  let name = file.stdout.trim()
  assert name.starts_with("file.") and name.ends_with(".txt")
  assert fs.stat(fp"{root}/{name}")?.mode.bit_and(0o777) == 0o600.clear_bits(fs.umask()?)
  let dir = run_applet(ctx, root, ["-d", "dir.XXXXXX"])?
  assert dir.status == 0, dir.stderr
  assert fs.stat(fp"{root}/{dir.stdout.trim()}")?.kind == "dir"
  assert fs.stat(fp"{root}/{dir.stdout.trim()}")?.mode.bit_and(0o777) == 0o700.clear_bits(fs.umask()?)
}

test test_mktemp_dry_run_and_tmpdir { |ctx|
  let root = test.temp_dir(ctx, name: "mktemp-dry")?
  let result = run_applet(ctx, root, ["-u", "--tmpdir", "tmp.XXXX"], tempdir: root.display())?
  assert result.status == 0, result.stderr
  assert result.stdout.starts_with(root.display() + "/tmp.")
  assert ! fp"{result.stdout.trim()}".exists()?
  assert run_applet(ctx, root, ["bad.XX"])?.status == 1
}
