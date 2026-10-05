type Captured = {status: Status, stdout: Str, stderr: Str}

# Writes `source` to a fresh script outside any project configuration.
proc script(ctx: TestContext, source: Str) [fs, error] -> Result[Path] {
  test.temp_file(ctx, name: "lint.xsh", contents: bytes.from_text(source))
}

# Runs `xsht lint` on `file` with `flags` and requires that the file parsed
# and checked, so that an absent finding is a statement about the linter.
# It runs in the directory of `file`, where no project configuration applies:
# the runner's own directory is the repository, whose configuration turns
# some default rules off.
proc lint(file: Path, flags: List[Str]) [process, env, error] -> Result[Captured] {
  let linted = cd (file.parent()) {
    run.capture --text "xsht" lint @flags $file
  }?

  assert linted.status.exited_with(0) or linted.status.exited_with(1), linted.stderr
  Ok(linted)
}

# How many findings with `code` the default rule set reports for `file`.
proc findings(file: Path, code: Str) [process, env, error] -> Result[Int] {
  let linted = lint(file, [])?
  Ok(linted.stderr.split(f"[{code}]").len() - 1)
}

# The replacement each finding with `code` offers on its `help:` line, in
# report order. A finding without a one-line replacement contributes nothing.
pure replacements(stderr: Str, code: Str) -> List[Str] {
  var current = ""
  let offered = collect {
    for line in stderr.lines() {
      let heading = rx"^(?:warn|note|err)\[([^\]]+)\]".captures(line)
      current = heading[1] when heading.len() == 2
      let help = rx"^help: .*? -> (.*)$".captures(line)
      yield help[1] when help.len() == 2 and current == code
    }
  }

  offered
}

# The text of `file` after the fixes of `code` alone were applied.
proc fixed(file: Path, code: Str) [fs, process, env, error] -> Result[Str] {
  let _ = lint(file, ["--fix", "--only", code])?
  file.read_text()
}

# Requires that `file` checks.
proc assert_checked(file: Path) [process, env, error] {
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
}

# Formats `file` in place and requires that a second pass changes nothing.
proc assert_formats_stably(file: Path) [process, env, error] {
  let formatted = run.capture --text "xsht" fmt $file
  assert formatted.status.exited_with(0), formatted.stderr
  let stable = run.capture --text "xsht" fmt --check $file
  assert stable.status.exited_with(0), stable.stderr
}

test test_record_constructor_fix_keeps_annotation_and_comments_and_converges { |ctx|
  let file = script(ctx, p"tests/fixtures/syntax/valid/record-constructor-explicit.xsh".read_text()?)?
  let reported = lint(file, [])?
  assert reported.stderr.split("[lint.prefer-record-constructor]").len() - 1 == 4, reported.stderr
  assert replacements(reported.stderr, "lint.prefer-record-constructor")[0] == "Config(name:)", reported.stderr
  let text = fixed(file, "lint.prefer-record-constructor")?
  assert "let config: Config = Config(" in text, text
  assert "enabled: observed_default()" in text, text
  assert "let lookup: Lookup = Lookup(value: empty)" in text, text
  assert_checked(file)
  let formatted = run.capture --text "xsht" fmt --check $file
  assert formatted.status.exited_with(0), formatted.stderr

  # The commented literal is still reported, and still has no rewrite.
  assert findings(file, "lint.prefer-record-constructor")? == 1
  assert "  # Keep this field explanation.\n  name: \"kept\",\n" in text, text
  assert fixed(file, "lint.prefer-record-constructor")? == text
}

test test_record_constructor_requires_static_schema_and_keeps_constant_bits { |ctx|
  let file = script(
    ctx,
    "type Signed = {value: Float = -0.0}\nlet same: Signed = {value: -0.0}\nlet different: Signed = {value: 0.0}\nlet source = {value: -0.0}\nlet spread: Signed = {...source}\ntype Lookup = {value: Map[Int]}\nlet contextual: Lookup = {value: {}}\nlet dynamic: Record = {value: -0.0}\n",
  )?
  let reported = lint(file, [])?
  assert reported.stderr.split("[lint.prefer-record-constructor]").len() - 1 == 4, reported.stderr
  assert replacements(reported.stderr, "lint.prefer-record-constructor") == ["Signed()", "Signed(value: 0.0)"], reported.stderr
  assert fixed(file, "lint.prefer-record-constructor")? == "type Signed = {value: Float = -0.0}\nlet same: Signed = Signed()\nlet different: Signed = Signed(value: 0.0)\nlet source = {value: -0.0}\nlet spread: Signed = {...source}\ntype Lookup = {value: Map[Int]}\nlet contextual: Lookup = {value: {}}\nlet dynamic: Record = {value: -0.0}\n"
  assert findings(file, "lint.prefer-record-constructor")? == 2
}

test test_field_label_fixes_keep_key_bytes_conversions_and_comments_and_converge { |ctx|
  let labels = "lint.prefer-bare-field-label,lint.prefer-known-field-access"
  let file = script(
    ctx,
    r"""let row = {"type": "file", r"in": 2, "a.b": 3, "x-y": 4, "size": 5} # retained
let label: Str = row.get("type")?
print $label ${row.in}
""",
  )?
  assert fixed(file, labels)? == r"""let row = {type: "file", in: 2, "a.b": 3, "x-y": 4, size: 5} # retained
let label: Str = row.type
print $label ${row.in}
"""
  assert_checked(file)
  assert_formats_stably(file)
  assert findings(file, "lint.prefer-bare-field-label")? == 0
  assert findings(file, "lint.prefer-known-field-access")? == 0
}

test test_known_field_access_keeps_dynamic_results_context_recovery_and_consumers { |ctx|
  let file = script(
    ctx,
    r"""let row = {type: "file", in: 2}
let unknown_consumer = row.get("type")?
let consumed = row.get("type")
let contextual: Str = row.get("type").context("wire")?
let missing = row.get("absent")
let recovered = row.get("type") ?? "none"
let commented: Str = row.get(
  # keep
  "type",
)?
let dynamic: Record = {}
let selected = dynamic.get("type")?.require(Str)?
print $unknown_consumer $contextual $recovered $commented $selected
""",
  )?
  let reported = lint(file, [])?
  assert reported.stderr.split("[lint.prefer-known-field-access]").len() - 1 == 1, reported.stderr
  assert replacements(reported.stderr, "lint.prefer-known-field-access") == ["row.type"], reported.stderr
}

test test_known_field_access_fix_keeps_nullable_values_and_aliases_and_converges { |ctx|
  let file = script(
    ctx,
    r"""# café
let row = {type: "file", value: null}
let alias = row
let label = alias.get("type")?
let absent = row.get(field: "value")?
print $label
""",
  )?
  let reported = lint(file, [])?
  assert replacements(reported.stderr, "lint.prefer-known-field-access").len() == 2, reported.stderr
  let text = fixed(file, "lint.prefer-known-field-access")?
  assert "let label = alias.type" in text, text
  assert "let absent = row.value" in text, text
  assert_checked(file)
  assert findings(file, "lint.prefer-known-field-access")? == 0
}

test test_map_literal_fix_rechecks_a_set_chain_and_keeps_unicode_order { |ctx|
  let file = script(
    ctx,
    "# café\nlet key = \"β\"\nlet counts = {[key]: 1}.set(\"alpha\", 2).set(key, 3)\nprint counts.len()\n",
  )?
  let text = fixed(file, "lint.prefer-map-literal")?
  assert "{[key]: 1, [\"alpha\"]: 2, [key]: 3}" in text, text
  assert_checked(file)
  assert_formats_stably(file)
  assert findings(file, "lint.prefer-map-literal")? == 0
}

test test_fresh_map_initialization_fix_keeps_annotation_and_refuses_observations { |ctx|
  let file = script(
    ctx,
    "let name = \"entry\"\nvar counts: Map[Int] = {}\ncounts = counts.set(name, 1)\ncounts = counts.set(\"total\", 2)\nprint counts.len()\n",
  )?
  let text = fixed(file, "lint.prefer-map-literal")?
  assert "var counts: Map[Int] = {[name]: 1, [\"total\"]: 2}" in text, text
  assert_checked(file)
  assert_formats_stably(file)
  for source in [
    "var counts: Map[Int] = {}\nprint counts.len()\ncounts = counts.set(\"one\", 1)\n",
    "var counts: Map[Int] = {}\ncounts = counts.set(\"one\", counts.len())\n",
    "var counts: Map[Int] = {}\nlet alias = counts\ncounts = counts.set(\"one\", 1)\n",
    "type Row = {value: Int}\nlet values = map.empty().set(\"one\", Row(value: 1))\n",
  ] {
    assert findings(script(ctx, source)?, "lint.prefer-map-literal")? == 0, source
  }
}

test test_map_literal_commented_initialization_is_reported_without_a_fix { |ctx|
  let source = "var counts: Map[Int] = {}\n# retain initialization note\ncounts = counts.set(\"one\", 1)\nprint counts.len()\n"
  let file = script(ctx, source)?
  assert findings(file, "lint.prefer-map-literal")? > 0
  assert fixed(file, "lint.prefer-map-literal")? == source
}

test test_list_element_assignment_fix_rechecks_exact_bounds_and_converges { |ctx|
  let file = script(
    ctx,
    "# café\nvar values: List[Int] = [1, 2, 3]\nvalues = [@values[..1], 8, @values[2..]] # keep\nprint values.len()\n",
  )?
  let text = fixed(file, "lint.prefer-list-element-assignment")?
  assert "values[1] = 8 # keep" in text, text
  assert "var values: List[Int] = [1, 2, 3]" in text, text
  assert_checked(file)
  assert_formats_stably(file)
  assert findings(file, "lint.prefer-list-element-assignment")? == 0
}

test test_list_element_assignment_refuses_clipped_bounds_effects_and_comments { |ctx|
  for source in [
    "var values = [1]\nvalues = [@values[..1], 8, @values[2..]]\n",
    "var values = [1, 2, 3]\nprint values.len()\nvalues = [@values[..1], 8, @values[2..]]\n",
    "pure replacement() -> Int { return 8 }\nvar values = [1, 2, 3]\nvalues = [@values[..1], replacement(), @values[2..]]\n",
    "var values = [1, 2, 3]\nvalues = [@values[..1], # reason\n  8, @values[2..]]\n",
  ] {
    let file = script(ctx, source)?
    let reported = lint(file, [])?
    assert "[lint.prefer-list-element-assignment]" in reported.stderr, reported.stderr
    assert replacements(reported.stderr, "lint.prefer-list-element-assignment") == [], reported.stderr
    assert fixed(file, "lint.prefer-list-element-assignment")? == source, source
  }
}

test test_list_element_assignment_reaches_every_element_that_leaves_the_local_alone { |ctx|
  # The list literal reads the list before and after the element; the
  # element assignment reads it after. A call, a read of the list, or a
  # pipeline cannot assign a local, so both spellings store the same list.
  let file = script(
    ctx,
    """pure double(n: Int) -> Int {
  n * 2
}

proc build(items: List[Int]) [] -> List[Int] {
  var values: List[Int] = [1, 2, 3]
  values = [@values[..1], double(items[0]) + values[0], @values[2..]]
  var sums = [0, 0]
  sums = [@sums[..0], items |> sum, @sums[1..]]
  var counts = [0, 0]
  counts = [@counts[..1], (items |> map . + 1).len(), @counts[2..]]
  values + sums + counts
}
""",
  )?
  let reported = lint(file, [])?
  assert replacements(reported.stderr, "lint.prefer-list-element-assignment") == [
    "values[1] = double(items[0]) + values[0]",
    "sums[0] = items |> sum",
    "counts[1] = (items |> map . + 1).len()",
  ], reported.stderr
  assert reported.stderr.split("[lint.prefer-list-element-assignment]").len() - 1 == 3, reported.stderr
  let _ = fixed(file, "lint.prefer-list-element-assignment")?
  assert_checked(file)
  let formatted = run.capture --text "xsht" fmt --check $file
  assert formatted.status.exited_with(0), formatted.stderr
}

test test_list_element_assignment_keeps_an_element_that_assigns_the_list { |ctx|
  # The callback overwrites `kept` after the literal has read its first
  # slice, so the literal mixes the old and new lists and the element
  # assignment would not.
  let source = """proc build(items: List[Int]) [] -> List[Int] {
  var kept = [1, 2]
  kept = [
    @kept[..0],
    items |> map {
      kept = [
        .,
        .,
      ]
      .
    } |> sum,
    @kept[1..],
  ]
  kept
}
"""
  let file = script(ctx, source)?
  let reported = lint(file, [])?
  assert reported.stderr.split("[lint.prefer-list-element-assignment]").len() - 1 == 1, reported.stderr
  assert replacements(reported.stderr, "lint.prefer-list-element-assignment") == [], reported.stderr
  assert fixed(file, "lint.prefer-list-element-assignment")? == source
}

test test_fmt_list_element_assignment_keeps_nested_selectors_and_comments { |ctx|
  let file = script(
    ctx,
    "var rows = [{count: 1}]\nrows[if true { # selector\n  0\n} else { 0 }].count += 1 # update\n",
  )?
  assert_formats_stably(file)
  let text = file.read_text()?
  assert "# selector" in text, text
  assert "# update" in text, text
  assert_checked(file)
}

test test_nested_record_update_fix_rechecks_and_converges { |ctx|
  let file = script(
    ctx,
    r"""let base = {a: {b: 1, c: 2}}
let after = {...base, a: {...base.a, b: 3}}
print $after.a.b
""",
  )?
  let text = fixed(file, "lint.prefer-nested-record-update")?
  assert "{...base, a.b: 3}" in text, text
  assert_checked(file)
  assert findings(file, "lint.prefer-nested-record-update")? == 0
  assert_formats_stably(file)
}

test test_nested_record_update_keeps_unstable_reads_comments_and_new_fields { |ctx|
  for source in [
    "pure change(value: Any) -> Unit { let base = {a: {b: 1}}; let after = {...base, a: {...base.a, b: value}} }\n",
    "var base = {a: {b: 1}}\nlet after = {...base, a: {...base.a, b: 3}}\n",
    "let base = {a: {b: 1}}\nlet after = {...base, a: {...base.a, b: 3 # worker count\n}}\n",
    "let base = {a: {b: 1}}\nlet after = {...base, a: {...base.a, new: 3}}\n",
    "let base = {a: {b: 1}}\nlet after = {...base, a: {...base.a, b: 3}, ...base}\n",
  ] {
    assert findings(script(ctx, source)?, "lint.prefer-nested-record-update")? == 0, source
  }
}
