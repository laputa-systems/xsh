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

test test_compat_stage_missing_gnu_programs_behave_as_false { |ctx|
  let root = test.temp_dir(ctx, name: "stage-gnu-missing")?
  let stage = fp"{root}/stage"
  let programs = fp"{root}/programs"
  let tools = fp"{root}/tools"
  tools.mkdir()
  programs.write("coreutils\n")
  fp"{tools}/false".write(
    r"""#!/bin/sh
case "$0" in
  */false) exit 1 ;;
  *) printf '%s: applet not found\n' "${0##*/}" >&2; exit 127 ;;
esac
""",
    mode: 0o755,
  )

  let inherited_path = env.get_or("PATH", "")?
  let setup_path = f"{tools}:{inherited_path}"
  let setup = process.command_argv(
    "python3",
    ["python3", "dev/compat/stage.py", "--stage", stage.display(), "--gnu-programs", programs.display()],
    env: {XSH_BIN: ctx.xsh_bin.display(), PATH: setup_path},
  )
  assert process.run(setup)?.exited_with(0)

  let missing = fp"{stage}/gnu-bin/coreutils"
  let status = process.run(process.command_argv(missing, [missing.display(), "--invalid"]))?
  assert status.exited_with(1)
}
