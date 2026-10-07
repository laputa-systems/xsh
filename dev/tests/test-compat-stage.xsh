test test_compat_stage_preserves_applet_option_separator { |ctx|
  let root = test.temp_dir(ctx)?
  let stage = fp"{root}/stage"
  let setup = process.command_argv("python3", ["python3", "dev/compat/stage.py", "--stage", stage.display()], env: {XSH_BIN: ctx.xsh_bin.display()})
  assert process.run(setup)?.ok
  let argv = [fp"{stage}/xsh-uutests".display(), "echo", "--", "-n"]
  let output = run.bytes @argv ?
  assert output == b"-- -n\n"
}

test test_compat_stage_gnu_path_resolves_applet_libraries { |ctx|
  let root = test.temp_dir(ctx, name: "stage-gnu-path")?
  let stage = fp"{root}/stage"
  let programs = fp"{root}/programs"
  programs.write("expr\n")

  let setup = process.command_argv("python3", ["python3", "dev/compat/stage.py", "--stage", stage.display(), "--gnu-programs", programs.display()], env: {XSH_BIN: ctx.xsh_bin.display()})
  assert process.run(setup)?.ok

  let argv = [fp"{stage}/gnu-bin/expr".display(), "1", "+", "1"]
  let output = run.bytes @argv ?
  assert output == b"2\n"
}
