test test_which_finds_shell { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/which.xsh" sh ?
  assert "sh" in output
}

test test_which_processes_all_names_before_missing_status { |ctx|
  let out = test.temp_path(ctx, name: "which.out")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/which.xsh" sh xsh-core-missing-command > $out
  assert ! status.exited_with(0)
  assert "sh" in out.read_text()?
}

test test_which_all_lists_every_executable_in_path_order { |ctx|
  let root = test.temp_dir(ctx, name: "which-all")?
  let first_dir = fp"{root}/first"
  let second_dir = fp"{root}/second"
  fs.mkdir(first_dir)
  fs.mkdir(second_dir)

  let first = fp"{first_dir}/xsh-test-command"
  let second = fp"{second_dir}/xsh-test-command"
  fs.write(first, "#!/bin/sh\nexit 0\n")
  first.chmod(0o755)
  fs.symlink(first, second)

  let path_value = f"{first_dir}:{second_dir}"
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/which.xsh"
  let argv = [ctx.xsh_bin.display(), script.display(), "-a", "xsh-test-command"]
  let status = process.run(
    process.command_argv(ctx.xsh_bin, argv, root, {PATH: path_value}, b"", out, err),
  )?

  assert status.exit_code()? == 0, err.read_text()?
  assert out.read_text()? == f"{first}\n{second}\n", out.read_text()?

  let missing_out = fp"{root}/missing-stdout"
  let missing_err = fp"{root}/missing-stderr"
  let missing_argv = [ctx.xsh_bin.display(), script.display(), "-a", "xsh-test-command", "xsh-test-missing"]
  let missing_status = process.run(
    process.command_argv(ctx.xsh_bin, missing_argv, root, {PATH: path_value}, b"", missing_out, missing_err),
  )?
  assert missing_status.exit_code()? == 1, missing_err.read_text()?
  assert missing_out.read_text()? == f"{first}\n{second}\n", missing_out.read_text()?
}
