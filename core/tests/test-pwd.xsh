type Ran = {status: Int, stdout: Str, stderr: Str}

proc pwd_run(ctx: TestContext, root: Path, cwd: Path, pwd: Str, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/pwd.xsh"
  let argv = [ctx.xsh_bin.display(), script.display(), @args]
  let plan = process.command_argv(ctx.xsh_bin, argv, cwd, {LC_ALL: "C", PWD: pwd}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_pwd { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pwd.xsh" ?
  assert output.trim() == fs.cwd()?.display()
}

test test_pwd_physical_option { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pwd.xsh" -P ?
  assert output.trim() == fs.cwd()?.resolve()?.display()
}

test test_pwd_logical_uses_trusted_symlink_and_rejects_dot_component { |ctx|
  let root = test.temp_dir(ctx, name: "pwd-logical")?
  let physical = fp"{root}/subdir"
  let logical = fp"{root}/symdir"
  physical.mkdir()
  fs.symlink(physical, logical)

  let symlink_pwd = pwd_run(ctx, root, logical, logical.display(), ["-L"])?
  assert symlink_pwd.status == 0, symlink_pwd.stderr
  assert symlink_pwd.stdout == f"{logical.display()}\n"

  let redundant_pwd = pwd_run(ctx, root, logical, f"{logical.display()}/.", ["-L"])?
  assert redundant_pwd.status == 0, redundant_pwd.stderr
  assert redundant_pwd.stdout == f"{physical.display()}\n"
}
