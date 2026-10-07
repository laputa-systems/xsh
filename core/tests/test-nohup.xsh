type Ran = {status: Int, stdout: Str, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], posix: Bool = false) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "nohup")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/nohup.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = if posix {
    process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: "", POSIXLY_CORRECT: "1"}, b"", out, err)
  } else {
    process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err)
  }
  let status = process.run(plan)?
  Ok({status: status.shell_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test nohup_preserves_command_arguments_and_exit_status { |ctx|
  assert invoke(ctx, ["sh", "-c", "exit 7"])?.status == 7
  assert invoke(ctx, ["printf", "%s", "--help"])?.stdout == "--help"
}

test nohup_version_uses_the_xsh_core_identity { |ctx|
  let result = invoke(ctx, ["--version"])?
  assert result.status == 0
  assert result.stdout.starts_with("nohup (XSH core) ")
  assert result.stderr == ""
}

test nohup_ignores_hup_across_exec { |ctx|
  let result = invoke(ctx, ["sh", "-c", "kill -HUP $$; printf survived"])?
  assert result.status == 0
  assert result.stdout == "survived"
  assert result.stderr == ""
}

test nohup_launch_failures_are_conventional { |ctx|
  assert invoke(ctx, ["/nonexistent/xsh-command"])?.status == 127
  assert invoke(ctx, ["/"])?.status == 126
  assert invoke(ctx, [])?.status == 125
}


test nohup_posix_environment_changes_only_internal_failure_status { |ctx|
  assert invoke(ctx, [], posix: true)?.status == 127
  assert invoke(ctx, ["--invalid"], posix: true)?.status == 127
  assert invoke(ctx, ["sh", "-c", "exit 7"], posix: true)?.status == 7
}
