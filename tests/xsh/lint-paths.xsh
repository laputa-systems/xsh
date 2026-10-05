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

# Requires `file` to be laid out as `xsht fmt` lays it out.
proc assert_formatted(file: Path) [process, env, error] {
  let formatted = run.capture --text "xsht" fmt --check $file
  assert formatted.status.exited_with(0), formatted.stderr
}

# Requires `file` to check without a diagnostic and to be laid out as
# `xsht fmt` lays it out.
proc assert_checks_and_formatted(file: Path) [process, env, error] {
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
  assert_formatted(file)
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

# A displayed fp-string is rebuilt from the same native pieces, so the
# constructor rule owns the round trip and the path-parse rule stays silent.
test test_path_constructor_owns_display_roundtrips_without_utf8_proof { |ctx|
  let dir = test.temp_dir(ctx, name: "path-constructor-display")?
  let file = fp"{dir}/paths.xsh"
  let source = r"""proc parsed(root: Path, value: Str) -> Path {
  return Path(fp"{root}/{value}".display())
}

proc main(root: Path, value: Str) [error] {
  let direct = Path(fp"{root}/{value}".display())
  let nested = Path(fp"{root}/{value}".display())
  print ${direct} ${nested}
}
"""
  file.write(source)
  assert_checks_and_formatted(file)
  let all = lint(file, [])?
  assert "lint.redundant-path-parse" not in all.stderr, all.stderr
  let rebuilt = "fp\"{root}/{value}\""
  assert replacements(file, "lint.path-constructor", source)? == [rebuilt, rebuilt, rebuilt]
  let fixed = fixed_by(file, "lint.path-constructor", source)?
  assert fixed == source.replace("Path(fp\"{root}/{value}\".display())", with: rebuilt)
  assert_checks_and_formatted(file)
}

test test_redundant_type_driven_roundtrips_are_fixed { |ctx|
  let dir = test.temp_dir(ctx, name: "type-driven-roundtrips")?
  let file = fp"{dir}/roundtrips.xsh"
  let source = r"""type Row = {name: Str}

proc main(root: Path, name: Str, row: Row, count: Int, ratio: Float) [error] {
  let parsed_literal = Path("tmp/out")
  let parsed_fmt = Path(f"{root}/{name}")
  let constructed_fmt = Path(f"{root}/{name}")
  let same_path = fp"{root}"
  let same_name = f"{name}"
  let same_row = row.require(Row)?
  let raw: Any = {name}
  let checked_row = raw.require(Row)?
  let same_count = f"{count}".parse_int()?
  let same_ratio = f"{ratio}".parse_float()?
  print ${parsed_literal} ${parsed_fmt} ${constructed_fmt} ${same_path} ${same_name} \
    ${same_row.name} ${checked_row.name} ${same_count} ${same_ratio}
}
"""
  file.write(source)
  assert_formatted(file)
  let constructed = source.replace("Path(\"tmp/out\")", with: "p\"tmp/out\"")
    .replace("Path(f\"{root}/{name}\")", with: "fp\"{root}/{name}\"")
  let parsed = source.replace("f\"{count}\".parse_int()?", with: "count")
    .replace("f\"{ratio}\".parse_float()?", with: "ratio")
  for case in [
    {
      rule: "lint.path-constructor",
      offered: "p\"tmp/out\"\nfp\"{root}/{name}\"\nfp\"{root}/{name}\"",
      fixed: constructed,
    },
    {
      rule: "lint.redundant-path-interpolation",
      offered: "root",
      fixed: source.replace("same_path = fp\"{root}\"", with: "same_path = root"),
    },
    {
      rule: "lint.redundant-string-interpolation",
      offered: "name",
      fixed: source.replace("same_name = f\"{name}\"", with: "same_name = name"),
    },
    {
      rule: "lint.redundant-require",
      offered: "row",
      fixed: source.replace("same_row = row.require(Row)?", with: "same_row = row"),
    },
    {
      rule: "lint.redundant-display-parse",
      offered: "count\nratio",
      fixed: parsed,
    },
  ] {
    # `replacements` requires the checker to accept the source.
    assert replacements(file, case.rule, source)?.join("\n") == case.offered, case.rule
    assert fixed_by(file, case.rule, source)? == case.fixed, case.rule
    assert_formatted(file)
    let _ = replacements(file, case.rule, case.fixed)?
  }

  # Every fix applied together is still accepted by the checker.
  file.write(source)
  let all = lint(file, ["--fix"])?
  assert all.status.exited_with(0), all.stderr
  assert_formatted(file)
  let relinted = lint(file, [])?
  assert "err[" not in relinted.stderr, relinted.stderr
}

test test_redundant_json_and_stream_roundtrips_are_reported { |ctx|
  let dir = test.temp_dir(ctx, name: "json-stream-roundtrips")?
  let file = fp"{dir}/roundtrips.xsh"
  let source = """proc main() [error] {
  let normalized = json.decode(json.encode({name: "pkg"})?)?
  let values = [1, 2, 3] |> where true |> map .
  let _ = normalized
}
"""
  file.write(source)
  assert_checks_and_formatted(file)
  let reported = lint(file, ["--only", "lint.json-roundtrip,lint.redundant-pipeline-stage"])?
  assert reported.stderr.split("warn[lint.json-roundtrip]").len() == 2, reported.stderr
  assert reported.stderr.split("warn[lint.redundant-pipeline-stage]").len() == 3, reported.stderr

  # Each stage's fix deletes it.
  let fixed = fixed_by(file, "lint.redundant-pipeline-stage", source)?
  assert fixed == source.replace(" |> where true |> map .", with: "")
  assert_checks_and_formatted(file)
}

test test_path_constructor_turns_displayed_path_text_into_native_pieces { |ctx|
  let dir = test.temp_dir(ctx, name: "path-constructor-native")?
  let file = fp"{dir}/paths.xsh"
  let source = r"""let raw = Path.parse_bytes(b"raw\xff name")?
let displayed = Path(raw.display())
let formatted = Path(f"{raw}")
let compound = Path(f"{raw}/child")
print $displayed $formatted $compound
"""
  file.write(source)
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
  let rules = "lint.redundant-path-parse,lint.path-constructor"
  assert replacements(file, rules, source)? == ["raw", "raw", "fp\"{raw}/child\""]
  let fixed = fixed_by(file, rules, source)?
  assert fixed == r"""let raw = Path.parse_bytes(b"raw\xff name")?
let displayed = raw
let formatted = raw
let compound = fp"{raw}/child"
print $displayed $formatted $compound
"""
}

test test_path_constructor_utf8_text_fix_rechecks_and_converges { |ctx|
  let dir = test.temp_dir(ctx, name: "path-constructor-utf8")?
  let file = fp"{dir}/paths.xsh"
  let source = "pure known(name: Str, count: Int) -> Path { Path(f\"{name}/{count}\") }\nlet literal = Path(p\"known\".display())\nprint known(\"name\", 2) \$literal\n"
  file.write(source)
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
  let rules = "lint.path-constructor,lint.redundant-path-parse"
  assert replacements(file, rules, source)?.len() == 2
  let fixed = fixed_by(file, rules, source)?
  assert fixed == "pure known(name: Str, count: Int) -> Path { fp\"{name}/{count}\" }\nlet literal = p\"known\"\nprint known(\"name\", 2) \$literal\n"
  let rechecked = run.capture --text "xsht" check $file
  assert rechecked.status.exited_with(0), rechecked.stderr

  # Nothing the two rules still report carries a fix.
  assert replacements(file, rules, fixed)? == []
  assert fixed_by(file, rules, fixed)? == fixed
}
