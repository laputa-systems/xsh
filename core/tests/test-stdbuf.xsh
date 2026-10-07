type Ran = {status: Int, stdout: Str, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "stdbuf")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/stdbuf.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.shell_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test stdbuf_refuses_unavailable_buffering_without_running_the_command { |ctx|
  let result = invoke(ctx, ["-o0", "printf", "must-not-run"])?
  assert result.status == 125
  assert result.stdout == ""
  assert "compatible preload library" in result.stderr
}

test stdbuf_rejects_each_valid_mode_when_child_buffering_is_unavailable { |ctx|
  for args in [["-i0", "true"], ["-oL", "true"], ["-e1K", "true"]] {
    let result = invoke(ctx, args)?
    assert result.status == 125, args.join(" ")
    assert result.stdout == "", args.join(" ")
    assert "compatible preload library" in result.stderr, result.stderr
  }
}

test stdbuf_reports_command_launch_failures_before_capability { |ctx|
  let missing = invoke(ctx, ["-o1", "no-such-stdbuf-command"])?
  assert missing.status == 127
  assert "No such file or directory" in missing.stderr, missing.stderr
  assert ! ("compatible preload library" in missing.stderr), missing.stderr

  let directory = invoke(ctx, ["-o1", "/"])?
  assert directory.status == 126
  assert "Permission denied" in directory.stderr, directory.stderr
  assert ! ("compatible preload library" in directory.stderr), directory.stderr
}

test stdbuf_checks_option_contract_before_capability { |ctx|
  assert "line buffering stdin is meaningless" in invoke(ctx, ["-iL", "true"])?.stderr
  assert "invalid mode 'bad'" in invoke(ctx, ["-o", "bad", "true"])?.stderr
  assert invoke(ctx, ["-o0"])?.status == 125
  assert invoke(ctx, ["true"])?.status == 125
  assert invoke(ctx, ["--help"])?.status == 0
}
