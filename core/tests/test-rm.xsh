type Ran = {status: Int, stderr: Str}

proc rm_run(ctx: TestContext, root: Path, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/rm.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: ""}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stderr: err.read_text()?})
}

test test_rm_force_recursive { |ctx|
  let root = test.temp_dir(ctx, name: "rm")?
  let dir = fp"{root}/dir"
  dir.mkdir()
  fp"{dir}/nested.txt".write("nested")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/rm.xsh" -rf $dir fp"{root}/missing" ?
  assert ! dir.exists()?
}

test test_rm_reports_unreadable_descendant { |ctx|
  guard applet.current_euid() != 0 else {
    test.skip("root bypasses directory read permissions")
    return
  }
  let root = test.temp_dir(ctx, name: "rm-unreadable")?
  let child = fp"{root}/foo/bar"
  child.mkdir(parents: true)
  fp"{child}/baz".write("data")
  child.chmod(0o000)
  defer child.chmod(0o755)?

  let result = rm_run(ctx, root, ["-rf", "foo"])?
  assert result.status == 1
  assert result.stderr == "rm: cannot remove 'foo/bar': Permission denied\n", result.stderr
}
