type PermRan = {status: Int, stdout: Str, stderr: Str}

proc perm_run(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[PermRan] {
  let root = test.temp_dir(ctx, name: "capture")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let words = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/chroot.xsh".display(), "--"].extend(args)
  let status = process.run(process.command_argv(ctx.xsh_bin, words, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err))?
  {status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?}
}

test test_chroot_missing_operand_and_skip_chdir_validation { |ctx|
  assert perm_run(ctx, [])?.status == 125
  let root = test.temp_dir(ctx, name: "chroot")?
  let result = perm_run(ctx, ["--skip-chdir", root.display()])?
  assert result.status == 125
  assert result.stderr.find("only permitted") != null
}

test test_chroot_exec_status_and_root_working_directory { |ctx|
  if user.current()?.uid != 0 { test.skip("chroot requires root") }
  let script = test.temp_file(ctx, name: "cwd.xsh", contents: b"print fs.cwd()?\n")?
  let result = perm_run(ctx, ["/", ctx.xsh_bin.display(), script.display()])?
  assert result.status == 0, result.stderr
  assert result.stdout == "/\n"
  assert perm_run(ctx, ["/", "/xsh-missing-command"])?.status == 127
  assert perm_run(ctx, ["/", script.display()])?.status == 126
}

test test_chroot_relative_command_resolves_inside_new_root { |ctx|
  if user.current()?.uid != 0 { test.skip("chroot requires root") }
  let root = test.temp_dir(ctx, name: "isolated-root")?
  let binary = fp"{root}/xsh"
  ctx.xsh_bin.copy(to: binary)
  binary.chmod(0o755)
  fp"{root}/report.xsh".write("print fs.cwd()?\n")
  let result = perm_run(ctx, [root.display(), "./xsh", "/report.xsh"])?
  assert result.status == 0, result.stderr
  assert result.stdout == "/\n"
}
