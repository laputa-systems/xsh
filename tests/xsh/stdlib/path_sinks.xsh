test test_path_bytes_are_the_native_bytes {
  assert p"usr/lib".bytes() == b"usr/lib"
  assert p"".bytes() == b""
  let raw = Path.parse_bytes(b"dir/bad\xffname")?
  assert raw.bytes() == b"dir/bad\xffname"
  assert Path.parse_bytes(raw.bytes())? == raw

  # Display text has already replaced the byte that is not UTF-8.
  let shown = raw.display()
  assert bytes.from_text(shown) != raw.bytes()
}

test test_command_plan_env_receives_a_path_as_native_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "path-sink-env")?
  let sh = process.which("sh")?
  let raw = Path.parse_bytes(b"dir/bad\xffname")?
  let script = "printf '%s' \"$SINK\""

  let by_argv = fp"{root}/argv.out"
  let argv_plan = process.command_argv(sh, [sh, "-c", script], env: {SINK: raw}, stdout: by_argv)
  assert process.run(argv_plan)?.exited_with(0)
  assert by_argv.read_bytes()? == b"dir/bad\xffname"

  let by_builder = fp"{root}/builder.out"
  let builder_plan = process.command {
    env = {SINK: raw}
    stdout = by_builder
    run $sh -c $script
  }
  assert process.run(builder_plan)?.exited_with(0)
  assert by_builder.read_bytes()? == b"dir/bad\xffname"

  # Text and numbers arrive as before.
  let plain = fp"{root}/plain.out"
  let plain_plan = process.command_argv(sh, [sh, "-c", script], env: {SINK: 8080}, stdout: plain)
  assert process.run(plain_plan)?.exited_with(0)
  assert plain.read_text()? == "8080"
}

test test_command_argv_takes_paths_as_target_and_items { |ctx|
  let root = test.temp_dir(ctx, name: "path-sink-argv")?
  let sh = process.which("sh")?
  let raw = Path.parse_bytes(b"bad\xffname")?
  let out = fp"{root}/argv.out"
  let plan = process.command_argv(sh, [sh, "-c", "printf '%s|' \"$@\"", "arg0", raw, "text"], stdout: out)
  assert process.run(plan)?.exited_with(0)
  assert out.read_bytes()? == b"bad\xffname|text|"
}

test test_native_test_runs_take_path_env_values { |ctx|
  let root = test.temp_dir(ctx, name: "path sink run")?
  let source = "print (e\"SINK_ROOT\"?)\n"
  let script = test.run_script(ctx, source, [], {SINK_ROOT: root})?
  assert script.success, script.stderr
  assert script.stdout == f"{root}\n"
  let direct = test.run_xsh(ctx, source, env: {SINK_ROOT: root})?
  assert direct.success, direct.stderr
  assert direct.stdout == f"{root}\n"
}

test test_path_display_sink_lint_passes_the_path_itself { |ctx|
  let root = test.temp_dir(ctx, name: "path-sink-lint")?
  let source = r"""proc show(sh: Path, marker: Path, out: Path) [fs, process, error] {
  let script = "printf '%s|%s|' \"$1\" \"$SINK\""
  let plan = process.command_argv(sh.display(), [sh.display(), "-c", script, "arg0", marker.display()], env: {SINK: marker.display()}, stdout: out)
  let _ = process.run(plan)?
  let label = marker.display()
  let size = bytes.from_text(marker.display()).len()
  print (out.read_text()?) $label $size
}

show(process.which("sh")?, p"ROOT/a marker", p"ROOT/out.txt")?
""".replace("ROOT", root.display())
  let candidate = test.temp_file(ctx, name: "path-display-sink.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --only lint.path-display-sink --fix $candidate ?
  let applied_succeeded = applied.status.exited_with(0)
  let applied_details = applied.stderr
  assert applied_succeeded, applied_details
  let fixed = candidate.read_text()?

  # A Str binding is a text boundary and keeps its conversion.
  let passed_directly = source.replace(
    r"""process.command_argv(sh.display(), [sh.display(), "-c", script, "arg0", marker.display()], env: {SINK: marker.display()}, stdout: out)""",
    r"""process.command_argv(sh, [sh, "-c", script, "arg0", marker], env: {SINK: marker}, stdout: out)""",
  )
  assert fixed == passed_directly.replace("bytes.from_text(marker.display())", "marker.bytes()")
  assert fixed != source
  assert "let label = marker.display()" in fixed
  let before = test.run_script(ctx, source)?
  let after = test.run_script(ctx, fixed)?
  let {success: succeeded, stderr: failure_details, ..} = after
  assert succeeded, failure_details
  assert after.stdout == before.stdout
  assert f"{root}/a marker|{root}/a marker|" in after.stdout

  let repeated = run.capture --text "xsht" lint --only lint.path-display-sink $candidate ?
  assert repeated.status.exited_with(0)
  assert "lint.path-display-sink" not in repeated.stderr
}
