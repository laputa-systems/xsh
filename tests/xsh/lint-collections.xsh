type Captured = {status: Status, stdout: Str, stderr: Str}

# Runs `xsht lint` with `arguments` on `file`.
proc lint(file: Path, arguments: List[Str]) [process, env, error] -> Result[Captured] {
  run.capture --text "xsht" lint @arguments $file
}

# Writes `source` to `file` and returns what `rule` alone reports for it. The
# checker must accept the source: a check error would hide the rule.
proc reported_by(file: Path, rule: Str, source: Str) [fs, process, env, error] -> Result[Str] {
  file.write(source)
  let reported = lint(file, ["--only", rule])?
  assert "err[" not in reported.stderr, reported.stderr
  Ok(reported.stderr)
}

# Applies the fixes of `rule` alone to `file` and returns the text the file
# then holds, which must check.
proc fixed_by(file: Path, rule: Str) [fs, process, env, error] -> Result[Str] {
  let fixed = lint(file, ["--fix", "--only", rule])?
  assert fixed.status.exited_with(0) or fixed.status.exited_with(1), fixed.stderr
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
  file.read_text()
}

# Requires `file` to be laid out as `xsht fmt` lays it out.
proc assert_formatted(file: Path) [process, env, error] {
  let formatted = run.capture --text "xsht" fmt --check $file
  assert formatted.status.exited_with(0), formatted.stderr
}

# A loop over `items` whose body is `body`, after declarations the bodies read.
pure accumulation_loop(body: Str) -> Str {
  "let items: List[Str] = []\nlet skipped = false\nvar names: List[Str] = []\nfor item in items {\n" + body + "}\n"
}

test test_accumulation_loop_becomes_a_list_comprehension { |ctx|
  let dir = test.temp_dir(ctx, name: "list-comprehension")?
  let file = fp"{dir}/loop.xsh"
  let source = """type Item = {name: Str}

let items: List[Item] = []
var names: List[Str] = []
for item in items {
  names = names.push(item["name"])
}
"""
  let reported = reported_by(file, "lint.prefer-list-comp", source)?
  assert "warn[lint.prefer-list-comp]" in reported, reported
  assert fixed_by(file, "lint.prefer-list-comp")? == """type Item = {name: Str}

let items: List[Item] = []
var names: List[Str] = [item["name"] for item in items]
"""
  assert_formatted(file)
}

test test_guarded_accumulation_loop_becomes_a_guarded_list_comprehension { |ctx|
  let dir = test.temp_dir(ctx, name: "guarded-list-comprehension")?
  let file = fp"{dir}/loop.xsh"
  let source = """let items: List[Str] = []
var names: List[Str] = []
for item in items {
  if item != "" {
    names = names.push(item.trim())
  }
}
"""
  let reported = reported_by(file, "lint.prefer-list-comp", source)?
  assert "warn[lint.prefer-list-comp]" in reported, reported
  assert fixed_by(file, "lint.prefer-list-comp")? == "let items: List[Str] = []\nvar names: List[Str] = [item.trim() for item in items if item != \"\"]\n"
  assert_formatted(file)
}

test test_branching_list_accumulation_loop_is_not_rewritten { |ctx|
  let dir = test.temp_dir(ctx, name: "branching-accumulation")?
  let file = fp"{dir}/loop.xsh"
  let source = """let items: List[Str] = []
var names: List[Str] = []
for item in items {
  if item != "" {
    names = names.push(item.trim())
  } else {
    names = names.push("missing")
  }
}
"""
  let reported = reported_by(file, "lint.prefer-list-comp", source)?
  assert "lint.prefer-list-comp" not in reported, reported
  assert fixed_by(file, "lint.prefer-list-comp")? == source
}

# The filter reads the list being built, which a comprehension cannot see.
test test_unique_accumulation_loop_is_not_rewritten { |ctx|
  let dir = test.temp_dir(ctx, name: "unique-accumulation")?
  let file = fp"{dir}/loop.xsh"
  let source = """let items: List[Int] = []
var unique: List[Int] = []
for item in items {
  if item not in unique {
    unique = unique.push(item)
  }
}
"""
  let reported = reported_by(file, "lint.prefer-list-comp", source)?
  assert "lint.prefer-list-comp" not in reported, reported
  assert fixed_by(file, "lint.prefer-list-comp")? == source
}

# `continue unless c` keeps the items `if c` keeps; `continue when c` keeps
# the others, spelled without new grouping.
test test_leading_continue_guards_become_comprehension_filters { |ctx|
  let dir = test.temp_dir(ctx, name: "continue-guards")?
  let file = fp"{dir}/loop.xsh"
  for case in [
    {
      body: "  continue unless item != \"\"\n  names += [item.trim()]\n",
      comprehension: "var names: List[Str] = [item.trim() for item in items if item != \"\"]\n",
    },
    {
      body: "  continue when item == \"\"\n  names += [item.trim()]\n",
      comprehension: "var names: List[Str] = [item.trim() for item in items if item != \"\"]\n",
    },
    {
      body: "  continue when skipped\n  names = names.push(item)\n",
      comprehension: "var names: List[Str] = [item for item in items if ! skipped]\n",
    },
    {
      body: "  continue when ! skipped\n  names += [item]\n",
      comprehension: "var names: List[Str] = [item for item in items if skipped]\n",
    },
    # Guards filter in the order they ran, before a nested condition.
    {
      body: "  continue unless item != \"\"\n  continue when item == \"-\"\n  if ! skipped {\n    names += [item]\n  }\n",
      comprehension: "var names: List[Str] = [\n  item\n  for item in items\n  if item != \"\"\n  if item != \"-\"\n  if ! skipped\n]\n",
    },
  ] {
    let source = accumulation_loop(case.body)
    let reported = reported_by(file, "lint.prefer-list-comp", source)?
    assert "warn[lint.prefer-list-comp]" in reported, f"{source}{reported}"
    let fixed = fixed_by(file, "lint.prefer-list-comp")?
    assert fixed == "let items: List[Str] = []\nlet skipped = false\n" + case.comprehension, case.body
    assert_formatted(file)
  }
}

test test_continue_guards_that_are_not_plain_filters_keep_the_loop { |ctx|
  let dir = test.temp_dir(ctx, name: "continue-guards-kept")?
  let file = fp"{dir}/loop.xsh"
  for body in [
    # Negating a compound condition needs grouping the lint does not add.
    "  continue when item == \"\" or skipped\n  names += [item]\n",
    # The filter reads the list being built.
    "  continue unless names.len() < 3\n  names += [item]\n",
    # A guard after the accumulation skips nothing, and `break` ends the loop.
    "  names += [item]\n  continue unless item != \"\"\n",
    "  break unless item != \"\"\n  names += [item]\n",
    # Two elements at once are not one projection.
    "  continue unless item != \"\"\n  names += [item, item]\n",
  ] {
    let source = accumulation_loop(body)
    let reported = reported_by(file, "lint.prefer-list-comp", source)?
    assert "lint.prefer-list-comp" not in reported, f"{body}{reported}"
    assert fixed_by(file, "lint.prefer-list-comp")? == source, body
  }
}

test test_map_building_loop_becomes_a_map_comprehension { |ctx|
  let dir = test.temp_dir(ctx, name: "map-comprehension")?
  let file = fp"{dir}/loop.xsh"
  let source = """let buckets = [{key: "pkg", items: ["one"]}]
var by_key: Map[List[Str]] = map.empty()

for bucket in buckets {
  by_key[bucket.key] = bucket.items
}
"""
  let reported = reported_by(file, "lint.prefer-map-comp", source)?
  assert "warn[lint.prefer-map-comp]" in reported, reported
  assert fixed_by(file, "lint.prefer-map-comp")? == "let buckets = [{key: \"pkg\", items: [\"one\"]}]\nvar by_key: Map[List[Str]] = {bucket.key: bucket.items for bucket in buckets}\n"
  assert_formatted(file)
}

test test_map_empty_becomes_an_empty_map_literal { |ctx|
  let dir = test.temp_dir(ctx, name: "empty-map-literal")?
  let file = fp"{dir}/map.xsh"
  let reported = reported_by(file, "lint.prefer-empty-map-literal", "let counts: Map[Int] = map.empty()\n")?
  assert "warn[lint.prefer-empty-map-literal]" in reported, reported
  assert fixed_by(file, "lint.prefer-empty-map-literal")? == "let counts: Map[Int] = {}\n"
  assert_formatted(file)
}

test test_stream_producer_is_suggested_for_a_lazily_consumed_list_accumulator { |ctx|
  let dir = test.temp_dir(ctx, name: "stream-producer")?
  let file = fp"{dir}/rows.xsh"
  let definitions = """proc rows(items: List[Str]) [error] -> Result[List[Str]] {
  var out: List[Str] = []

  for item in items {
    if item != "" {
      out = out.push(item)
    }
  }

  return out |> sort-by .
}

pure pure_rows(items: List[Str]) -> List[Str] {
  var out: List[Str] = []

  for item in items {
    out = out.push(item)
  }

  return out
}
"""

  # The definitions alone do not warn: nothing consumes the list lazily.
  let alone = reported_by(file, "lint.prefer-stream-producer", definitions)?
  assert "lint.prefer-stream-producer" not in alone, alone

  let consumed = reported_by(
    file,
    "lint.prefer-stream-producer",
    definitions + "\nlet count = rows([\"a\"])? |> count()\n",
  )?
  assert consumed.split("warn[lint.prefer-stream-producer]").len() == 2, consumed
}

test test_join_with_an_empty_separator_becomes_string_concatenation { |ctx|
  let dir = test.temp_dir(ctx, name: "string-concat")?
  let file = fp"{dir}/join.xsh"
  let reported = reported_by(file, "lint.prefer-string-concat", "let x = [\"a\", \"b\"].join(\"\")\n")?
  assert "warn[lint.prefer-string-concat]" in reported, reported
  assert fixed_by(file, "lint.prefer-string-concat")? == "let x = \"a\" + \"b\"\n"
  assert_formatted(file)
}
