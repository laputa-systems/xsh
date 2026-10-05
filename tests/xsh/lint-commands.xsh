type Captured = {status: Status, stdout: Str, stderr: Str}

# Runs `xsht lint` with `arguments` on `file`.
proc lint(file: Path, arguments: List[Str]) [process, env, error] -> Result[Captured] {
  run.capture --text "xsht" lint @arguments $file
}

# Writes `source` to `file`, applies the fixes of `rule` alone, and returns
# the text the file then holds.
proc fixed_by(file: Path, rule: Str, source: Str) [fs, process, env, error] -> Result[Str] {
  file.write(source)
  let fixed = lint(file, ["--fix", "--only", rule])?
  assert fixed.status.exited_with(0) or fixed.status.exited_with(1) or fixed.status.exited_with(2), fixed.stderr
  file.read_text()
}

# Requires `file` to check without a diagnostic and to be laid out as
# `xsht fmt` lays it out.
proc assert_checks_and_formatted(file: Path) [process, env, error] {
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
  let formatted = run.capture --text "xsht" fmt --check $file
  assert formatted.status.exited_with(0), formatted.stderr
}

# The replacement each single-line fix of `rule` offers for `source`, in
# report order.
proc replacements(file: Path, rule: Str, source: Str) [fs, process, env, error] -> Result[List[Str]] {
  file.write(source)
  let reported = lint(file, ["--only", rule])?
  assert "err[" not in reported.stderr, reported.stderr
  let offered = collect {
    for line in reported.stderr.lines() {
      let found = rx"^help: [^>]* -> (.*)$".captures(line)
      yield found[1] when found.len() == 2
    }
  }

  Ok(offered)
}

test test_single_value_command_fstrings_are_fixed { |ctx|
  let dir = test.temp_dir(ctx, name: "command-fstrings")?
  let file = fp"{dir}/commands.xsh"
  let source = """proc main(manifest: Path, name: Str) {
  print f"{manifest.display()}"
  print f"{name}"
}
"""
  file.write(source)
  assert_checks_and_formatted(file)
  let all = lint(file, [])?
  assert "lint.redundant-string-interpolation" not in all.stderr, all.stderr
  assert replacements(file, "lint.redundant-command-fmt", source)? == ["\$name"]
  let fixed = fixed_by(file, "lint.redundant-command-fmt", source)?
  assert fixed == source.replace("print f\"{name}\"", with: "print \$name")
  assert_checks_and_formatted(file)
}

test test_redundant_command_interpolations_for_run_args_are_fixed { |ctx|
  let dir = test.temp_dir(ctx, name: "command-interpolation")?
  let file = fp"{dir}/commands.xsh"
  let source = r"""proc main(name: Str) {
  run echo ${name.lower()}
}
"""
  file.write(source)
  assert_checks_and_formatted(file)
  assert replacements(file, "lint.redundant-command-interpolation", source)? == ["name.lower()"]
  let fixed = fixed_by(file, "lint.redundant-command-interpolation", source)?
  assert fixed == "proc main(name: Str) {\n  run echo name.lower()\n}\n"
  assert_checks_and_formatted(file)
}

test test_path_display_is_dropped_from_command_words { |ctx|
  let dir = test.temp_dir(ctx, name: "command-path-display")?
  let file = fp"{dir}/commands.xsh"
  let source = r"""proc main(foo: Path) {
  print $foo.display()
  print ${foo.display()}
  print foo.display()
  print ${foo.parent().display()}
}
"""
  file.write(source)
  assert_checks_and_formatted(file)

  # One fix per display.
  assert replacements(file, "lint.redundant-path-display", source)?.len() == 4
  let reported = lint(file, ["--only", "lint.redundant-path-display"])?
  assert reported.stderr.split("warn[lint.redundant-path-display]").len() == 5, reported.stderr
  let fixed = fixed_by(file, "lint.redundant-path-display", source)?
  assert fixed == r"""proc main(foo: Path) {
  print $foo
  print ${foo}
  print $foo
  print ${foo.parent()}
}
"""
  assert_checks_and_formatted(file)
}

test test_command_path_display_fixes_pass_native_bytes { |ctx|
  let dir = test.temp_dir(ctx, name: "command-path-bytes")?
  let file = fp"{dir}/commands.xsh"
  let source = r"""let raw = Path.parse_bytes(b"raw\xff name")?
run printf "%s" "--target=${raw.display()}" ?
run printf "%s" ${raw.display()} ?
run printf "%s" (raw.display()) ?
run printf "%s" f"{raw.display()}" ?
run printf "%s" f"{raw}" ?
"""
  file.write(source)
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr

  # An f-string word asks for text, so it is not unwrapped without proof
  # that the Path's bytes are UTF-8.
  assert fixed_by(file, "lint.redundant-command-fmt", source)? == source

  # Dropping `.display()` passes the native bytes the text would replace.
  assert replacements(file, "lint.redundant-path-display", source)? == ["raw", "raw", "\$raw", "raw"]
  let fixed = fixed_by(file, "lint.redundant-path-display", source)?
  assert fixed == r"""let raw = Path.parse_bytes(b"raw\xff name")?
run printf "%s" "--target=${raw}" ?
run printf "%s" ${raw} ?
run printf "%s" $raw ?
run printf "%s" f"{raw}" ?
run printf "%s" f"{raw}" ?
"""
  assert fixed_by(file, "lint.redundant-command-fmt", fixed)? == fixed
}
