type Outcome = {status: Int, stdout: Str, stderr: Str}

proc run_applet(ctx: TestContext, root: Path, args: List[Str], logical_pwd: Str = "") [fs, process, error] -> Result[Outcome] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/pwd.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {PWD: logical_pwd, LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

proc run_applet_in_deleted_directory(ctx: TestContext, root: Path) [fs, process, error] -> Result[Outcome] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/pwd.xsh"
  let setup = "mkdir child && cd child && rmdir ../child && exec \"$@\""
  let argv = ["sh", "-c", setup, "sh", ctx.xsh_bin.display(), script.display(), "--"]
  let plan = process.command_argv(p"/bin/sh", argv, root, {PWD: root.display(), LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test test_pwd { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pwd.xsh"
  assert output.trim() == fs.cwd()?.display()
}


test test_pwd_validates_logical_directory_and_warns_for_operands { |ctx|
  let root = test.temp_dir(ctx, name: "pwd-logical")?
  fp"{root}/physical".mkdir()
  fp"{root}/alias".symlink(to: p"physical")
  let physical = fp"{root}/physical"
  let alias = fp"{root}/alias".display()
  assert run_applet(ctx, physical, ["-L"], logical_pwd: alias)?.stdout == alias + "\n"
  assert run_applet(ctx, physical, ["-P"], logical_pwd: alias)?.stdout == physical.display() + "\n"
  assert run_applet(ctx, physical, ["-L"], logical_pwd: root.display())?.stdout == physical.display() + "\n"
  let extra = run_applet(ctx, physical, ["arg"])?
  assert extra.status == 0
  assert extra.stderr == "pwd: ignoring non-option arguments\n"
}


test test_pwd_fails_from_a_deleted_working_directory { |ctx|
  let root = test.temp_dir(ctx, name: "pwd-deleted")?.resolve()?
  let result = run_applet_in_deleted_directory(ctx, root)?
  assert result.status == 1
  assert result.stdout == ""
  assert result.stderr == "pwd: couldn't find directory entry in '..' with matching i-node\n", result.stderr
}
