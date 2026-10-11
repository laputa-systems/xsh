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

test nohup_combines_input_and_error_redirection_notice { |ctx|
  let pty = unix.open_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  let root = test.temp_dir(ctx, name: "nohup-notice")?
  let out = fp"{root}/stdout"
  let script = fp"{ctx.core_dir}/nohup.xsh"
  let plan = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin, script, p"sh", p"-c", p"printf child >&2"], root,
    {LC_ALL: "C"}, fp"{pty.name}", out, fp"{pty.name}", timeout: 5s)
  assert process.run(plan)?.shell_code()? == 0
  assert out.read_text()? == "child"
  assert unix.read_fd(pty.master, 8192)?.utf8()?.replace("\r\n", with: "\n") ==
    "nohup: ignoring input and redirecting standard error to standard output\n"
}

test nohup_full_advisory_output_prevents_command_execution { |ctx|
  let pty = unix.open_pty()?
  defer unix.close_fd(pty.master)
  defer unix.close_fd(pty.replica)
  let root = test.temp_dir(ctx, name: "nohup-full")?
  let script = fp"{ctx.core_dir}/nohup.xsh"
  for posix in [false, true] {
    let environment: Record = if posix { {LC_ALL: "C", POSIXLY_CORRECT: "1"} } else { {LC_ALL: "C"} }
    let plan = process.command_argv(ctx.xsh_bin,
      [ctx.xsh_bin, script, p"sh", p"-c", p"printf launched > child"], root,
      environment, fp"{pty.name}", fp"{pty.name}", p"/dev/full", timeout: 5s)
    assert process.run(plan)?.shell_code()? == (if posix { 127 } else { 125 })
    assert ! fp"{root}/child".exists()?
    assert fp"{root}/nohup.out".read_text()? == ""
  }
}
