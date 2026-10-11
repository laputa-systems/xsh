use support.uu as uu

test test_uu_support_standalone_usage_name { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basename", [])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "basename: missing operand\nTry 'basename --help' for more information.\n")
}

test test_uu_support_directory_stdin { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke_from_path(s, "tr", ["1", "1"], s.root)?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "tr: read error: Is a directory")
}

test test_uu_support_non_utf8_argv { |ctx|
  let s = uu.scene(ctx)?
  let name = Path.parse_bytes(b"file\xff")?
  let file = uu.at_bytes(s, b"file\xff")?
  file.write(b"data\xff")?
  let r = uu.invoke_paths(s, "cat", [name])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"data\xff")
}

test test_uu_support_redirected_stdout { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "out", "first\n")?
  let r = uu.invoke(s, "cat", [], stdin: b"second\n",
    stdout: uu.at(s, "out"), stdout_append: true)?
  uu.succeeds(r)
  uu.no_output(r)
  uu.file_is(s, "out", "first\nsecond\n")
}


test test_uu_support_text_assertions_reject_invalid_utf8 { |ctx|
  for assertion in [
    "uu.stdout_is(r, \"(non-UTF-8 output)\")",
    "uu.stderr_is(r, \"(non-UTF-8 output)\")",
    "uu.stdout_contains(r, \"output\")",
    "uu.stderr_contains(r, \"output\")",
    "uu.stdout_str_starts_with(r, \"(non-UTF\")",
  ] {
    let checked = test.run_script(ctx,
      "use support.uu as uu\nlet r: uu.Ran = {util: \"probe\", args: [], status: 0, stdout: b\"\\xff\", stderr: b\"\\xff\"}\n" + assertion + "\n",
      env: {XSH_MODULE_PATH: fp"{ctx.core_dir}/tests"})?
    assert ! checked.success, f"invalid UTF-8 passed {assertion}"
    assert "AssertionError" in checked.stderr, checked.stderr
  }
}

test test_uu_support_file_text_assertion_rejects_invalid_utf8 { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "invalid", b"\xff")?
  let checked = test.run_script(ctx,
    "use support.uu as uu\nlet root = Path(args[0])\nlet context: TestContext = {name: \"assertion probe\", file: root, temp_root: root, core_dir: root, xsh_bin: root}\nlet s: uu.Scene = {ctx: context, root: root}\nuu.file_is(s, \"invalid\", \"(non-UTF-8 output)\")\n",
    args: [s.root.display()], env: {XSH_MODULE_PATH: fp"{ctx.core_dir}/tests"})?
  assert ! checked.success
  assert "AssertionError" in checked.stderr, checked.stderr
}

test test_uu_support_clears_environment_and_preserves_defaults { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "printenv", [])?
  uu.succeeds(r)
  uu.stdout_contains(r, "LC_ALL=C\n")
  uu.stdout_contains(r, "TZ=UTC\n")
  assert "HOME=" not in r.stdout.utf8()?
  assert "TMPDIR=" not in r.stdout.utf8()?
  for line in r.stdout.utf8()?.split("\n") {
    if line != "" {
      assert line.split("=")[0] in ["LC_ALL", "TZ", "PATH", "LD_PRELOAD", "LLVM_PROFILE_FILE"]
    }
  }
  let inherited_path = uu.invoke(s, "printenv", ["PATH"])?
  uu.succeeds(inherited_path)
  uu.stdout_only_bytes(inherited_path, bytes.concat([bytes.from_text(env.get("PATH")?), b"\n"]))
  let custom = uu.invoke(s, "printenv", ["LC_ALL", "TZ", "UU_CUSTOM"], vars: {UU_CUSTOM: "override"})?
  uu.succeeds(custom)
  uu.stdout_only_bytes(custom, b"C\nUTC\noverride\n")
  let overridden = uu.invoke(s, "printenv", ["LC_ALL", "TZ", "PATH"],
    vars: {LC_ALL: "POSIX", TZ: "GMT", PATH: "/caller/path"})?
  uu.succeeds(overridden)
  uu.stdout_only(overridden, "POSIX\nGMT\n/caller/path\n")
}


test test_uu_support_path_stdin_redirected_streams { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "second\n")?
  uu.write(s, "out", "first\n")?
  uu.write(s, "err", "prior error\n")?
  let r = uu.invoke_from_path(s, "cat", [], uu.at(s, "input"),
    stdout: uu.at(s, "out"), stderr: uu.at(s, "err"),
    stdout_append: true, stderr_append: true)?
  uu.succeeds(r)
  uu.no_output(r)
  uu.file_is(s, "out", "first\nsecond\n")
  uu.file_is(s, "err", "prior error\n")
}

test test_uu_support_same_input_append_output { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file", "content")?
  let file = uu.at(s, "file")
  let r = uu.invoke_from_path(s, "cat", [], file, stdout: file, stdout_append: true)?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_contains(r, "input file is output file")
  uu.file_is(s, "file", "content")
}


test test_uu_support_preserves_leading_double_dash { |ctx|
  let s = uu.scene(ctx)?
  uu.stdout_only(uu.invoke(s, "echo", ["--", "-n", "text"])?, "-- -n text\n")
  uu.stdout_only(uu.invoke_paths(s, "echo", [p"--", p"-n", p"text"])?, "-- -n text\n")
  uu.write(s, "input", "ignored")?
  uu.stdout_only(uu.invoke_from_path(s, "echo", ["--", "-n", "text"], uu.at(s, "input"))?, "-- -n text\n")
}


test test_uu_support_timeout_bounds_every_launch_shape { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "")?
  for result in [
    uu.invoke(s, "sleep", ["1"], timeout: 50ms),
    uu.invoke_paths(s, "sleep", [p"1"], timeout: 50ms),
    uu.invoke_from_path(s, "sleep", ["1"], uu.at(s, "input"), timeout: 50ms),
  ] {
    assert result is Err(is Timeout), "applet must stop at its deadline"
  }
  uu.succeeds(uu.invoke(s, "sleep", ["0.01"])?)
}

test test_uu_support_command_preserves_parallel_input_and_append_output { |ctx|
  let s = uu.scene(ctx)?
  let output = uu.at(s, "output")
  output.write("")?
  var children: List[ProcessHandle] = []
  for index in range(3) {
    let plan = uu.command(s, "cat", [], stdin: bytes.from_text(f"child-{index}\n"),
      stdout: output, stderr: uu.at(s, f"stderr-{index}"), stdout_append: true, timeout: 2s)?
    children += [spawn plan?]
  }
  for child in children { assert (wait child?).exited_with(0) }
  let lines = [line for line in output.read_text()?.split("\n") if line != ""] |> sort
  assert lines == ["child-0", "child-1", "child-2"]
  for index in range(3) { uu.file_is(s, f"stderr-{index}", "") }
}
