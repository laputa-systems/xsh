test test_process_which_takes_a_path_or_text {
  let sh = process.which("sh")?
  assert sh.is_file()? or sh.is_symlink()?

  # A path names the program as text does; a literal stays text.
  assert process.which(sh)? == sh
  assert process.which(fp"{sh.name()}")? == sh
  test.error_kind(process.which(p"xsh-no-such-program"), "not-found")
  test.error_kind(process.which("xsh-no-such-program"), "not-found")
}

test test_an_argv_list_built_first_may_hold_text_and_paths { |ctx|
  let root = test.temp_dir(ctx, name: "path-sink-argv-union")?
  let sh = process.which("sh")?
  let raw = b"bad\xffname" as Path
  let out = fp"{root}/argv.out"

  let argv: List[Union[Str, Path]] = [sh, "-c", "printf '%s|' \"$@\"", "arg0", raw, "text"]
  let plan = process.command_argv(sh, argv, stdout: out)
  assert process.run(plan)?.exited_with(0)
  assert out.read_bytes()? == b"bad\xffname|text|"

  let _ = test.expect(
    ctx,
    "let argv: List[Union[Str, Int]] = [\"true\", 1]\nlet plan = process.command_argv(\"true\", argv)\n",
    status: 2,
    stderr: ["argv must contain Str or Path items"],
  )?
}

test test_native_test_runs_take_path_arguments { |ctx|
  let root = test.temp_dir(ctx, name: "path sink args")?
  let source = "print (args[0]) (args.len())\n"

  # A path in a list written in the call, a list of paths, and a list of
  # text and paths built first.
  let _ = test.expect(ctx, source, status: 0, stdout: [f"{root} 2"], args: [root, "second"])?

  let paths = [root, fp"{root}/below"]
  let by_paths = test.run_script(ctx, source, paths)?
  assert by_paths.stdout == f"{root} 2\n"

  let mixed: List[Union[Str, Path]] = [root, "second", "third"]
  let by_mixed = test.run_script(ctx, source, mixed)?
  assert by_mixed.stdout == f"{root} 3\n"

  # Text alone is still a list of text.
  let by_text = test.run_script(ctx, source, ["first"])?
  assert by_text.stdout == "first 1\n"
}

test test_env_path_takes_a_string_literal_as_a_path { |ctx|
  env ({PATH: "/usr/bin:/bin"}) {
    env.PATH.append("/opt/xsh-appended")
    env.PATH.prepend("/opt/xsh-prepended")
    assert e"PATH"? == "/opt/xsh-prepended:/usr/bin:/bin:/opt/xsh-appended"
    assert "/opt/xsh-appended" in env.PATH
    assert "/usr/bin" in env.PATH
    assert "/usr" not in env.PATH
    assert /opt/xsh-prepended in env.PATH
  }

  # Text that is not a literal is not a path.
  for source in [
    "let dir = \"/opt/bin\"\nenv.PATH.append(dir)\n",
    "let dir = \"/opt/bin\"\nprint (dir in env.PATH)\n",
  ] {
    let rejected = test.run_script(ctx, source)?
    assert rejected.status == 2, source
    assert "Path" in rejected.stderr, rejected.stderr
  }
}
