type Captured = {status: Status, stdout: Str, stderr: Str}

const rule = "lint.prefer-list-compound-assignment"

# Runs `xsht lint` with `arguments` on `file`.
proc lint(file: Path, arguments: List[Str]) [process, env, error] -> Result[Captured] {
  run.capture --text "xsht" lint @arguments $file
}

# Writes `source` to `file` and returns what the rule reports for it. The
# source must check, so that a silent rule is a decision of the rule.
proc reported(file: Path, source: Str) [fs, process, env, error] -> Result[Str] {
  file.write(source)
  let report = lint(file, ["--only", rule])?
  assert "err[" not in report.stderr, report.stderr
  Ok(report.stderr)
}

# Applies the rule's fixes to `file` and returns the text it then holds.
proc fixed(file: Path) [fs, process, env, error] -> Result[Str] {
  let _ = lint(file, ["--fix", "--only", rule])?
  file.read_text()
}

# Requires `file` to check without a diagnostic.
proc assert_checks(file: Path) [process, env, error] {
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
}

# Requires `file` to be laid out as `xsht fmt` lays it out.
proc assert_formatted(file: Path) [process, env, error] {
  let formatted = run.capture --text "xsht" fmt --check $file
  assert formatted.status.exited_with(0), formatted.stderr
}

# How many updates the report holds.
pure updates(report: Str) -> Int {
  report.split(f"warn[{rule}]").len() - 1
}

test test_list_compound_assignment_is_checked_and_converges { |ctx|
  let root = test.temp_dir(ctx, name: "list-update")?
  let file = fp"{root}/update.xsh"
  let source = "# café\nvar names: List[Str] = []\nlet item = \"value\"\nnames = names.push(item) # Keep this reason.\nlet more = [\"second\"]\nnames = names.extend(more)\nprint names.len()\n"
  let report = reported(file, source)?
  assert updates(report) == 2, report
  assert_checks(file)
  assert_formatted(file)

  let rewritten = fixed(file)?
  assert rewritten == source.replace("names = names.push(item) # Keep", with: "names += [item] # Keep").replace(
    "names = names.extend(more)",
    with: "names += more",
  ), rewritten
  assert_checks(file)
  assert_formatted(file)
  let again = lint(file, [])?
  assert rule not in again.stderr, again.stderr
}

test test_list_compound_assignment_refuses_effectful_and_nested_updates { |ctx|
  let root = test.temp_dir(ctx, name: "list-update-refused")?
  let file = fp"{root}/update.xsh"
  let source = r"""pure item() -> Int {
  return 2
}
var values = [1]
values = values.push(item())
var container = {values: [1]}
container.values = container.values.push(2)
let pushed = values.push(3)
print ${pushed.len()} ${container.values.len()}
"""
  let report = reported(file, source)?
  assert rule not in report, report
  assert_checks(file)
  assert fixed(file)? == source
}

test test_list_compound_assignment_retains_multiline_comments { |ctx|
  let root = test.temp_dir(ctx, name: "list-update-comment")?
  let file = fp"{root}/update.xsh"
  let source = r"""var values = [1]
values = values.push(
  2, # keep
)
print ${values.len()}
"""
  let report = reported(file, source)?
  assert updates(report) == 1, report
  assert "help: " not in report, report
  assert_checks(file)
  assert fixed(file)? == source
}

# The argument is evaluated after the receiver read in `push` and before
# the read of the current value in `+=`; none of these can assign `names`.
test test_list_compound_assignment_reaches_every_argument_that_leaves_the_local_alone { |ctx|
  let root = test.temp_dir(ctx, name: "list-update-arguments")?
  let file = fp"{root}/update.xsh"
  let source = r"""type Row = {name: Str, size: UInt}

pure label(item: Str) -> Str {
  return item.upper()
}

pure shout(items: List[Str]) -> List[Str] {
  return items |> map { |item| item.upper() } |> collect()
}

proc collect_names(items: List[Str], row: Row) [] -> List[Str] {
  var names: List[Str] = []
  var rows: List[Row] = []
  for item in items {
    names = names.push(label(item))
    names = names.push(f"{item}!")
    names = names.push(row.name)
    names = names.push(items[0])
    names = names.push((items |> map { |entry| entry.upper() }).join(","))
    names = names.push("#define X")
    names = names.extend(shout(items))
    rows = rows.push({name: item, size: 3})
  }

  print ${rows.len()}
  names
}
"""
  let report = reported(file, source)?
  assert updates(report) == 8, report
  assert_checks(file)
  assert_formatted(file)

  let rewritten = fixed(file)?
  assert rewritten == source.replace(
    r"""    names = names.push(label(item))
    names = names.push(f"{item}!")
    names = names.push(row.name)
    names = names.push(items[0])
    names = names.push((items |> map { |entry| entry.upper() }).join(","))
    names = names.push("#define X")
    names = names.extend(shout(items))
    rows = rows.push({name: item, size: 3})
""",
    with: r"""    names += [label(item)]
    names += [f"{item}!"]
    names += [row.name]
    names += [items[0]]
    names += [(items |> map { |entry| entry.upper() }).join(",")]
    names += ["#define X"]
    names += shout(items)
    rows += [{name: item, size: 3}]
""",
  ), rewritten
  assert_checks(file)
  assert_formatted(file)
}

# Evaluating the callback overwrites `names` after `push` has already read
# it, so `names += [...]` would keep the overwrite and `push` would not.
test test_list_compound_assignment_keeps_an_argument_that_assigns_the_target { |ctx|
  let root = test.temp_dir(ctx, name: "list-update-overwrite")?
  let file = fp"{root}/update.xsh"
  let source = r"""proc collect_names(items: List[Str]) [] -> List[Str] {
  var names: List[Str] = []
  names = names.push(items |> map { |entry| names = [entry]; entry } |> join(","))
  names = names.push((items |> map { |entry| names = [entry]; entry }).join(","))
  names = names.push(items |> map { |entry| entry.upper() } |> join(","))
  names = names.push((items |> map { |entry| entry.upper() }).join(","))
  names
}
"""
  # The unformatted pipeline and its formatted method-call spelling parse
  # differently and must be judged alike.
  let report = reported(file, source)?
  assert updates(report) == 2, report
  assert fixed(file)? == source.replace(
    "names = names.push(items |> map { |entry| entry.upper() } |> join(\",\"))",
    with: "names += [items |> map { |entry| entry.upper() } |> join(\",\")]",
  )
    .replace(
      "names = names.push((items |> map { |entry| entry.upper() }).join(\",\"))",
      with: "names += [(items |> map { |entry| entry.upper() }).join(\",\")]",
    )
}

# Any proc may assign a module-level variable, so a call between the two
# reads could change which one the update sees.
test test_list_compound_assignment_keeps_module_variables_with_calling_arguments { |ctx|
  let root = test.temp_dir(ctx, name: "list-update-module")?
  let file = fp"{root}/update.xsh"
  let source = r"""var seen: List[Str] = []

proc remember(item: Str) [] -> Str {
  seen = []
  return item
}

proc collect_names(item: Str) [] -> List[Str] {
  seen = seen.push(remember(item))
  seen = seen.push(item)
  seen
}
"""
  let report = reported(file, source)?
  assert updates(report) == 1, report
  assert fixed(file)? == source.replace("seen = seen.push(item)", with: "seen += [item]")
}

# A `#` inside a string is not a comment, and a call laid out over several
# lines around a one-line argument still becomes one `+=` statement.
test test_list_compound_assignment_offers_a_fix_around_hash_strings_and_call_layout { |ctx|
  let root = test.temp_dir(ctx, name: "list-update-hash")?
  let file = fp"{root}/update.xsh"
  let source = r"""proc build() [] -> List[Str] {
  var lines: List[Str] = []
  lines = lines.push("#endif /* GUARD_H */")
  lines = lines.push(
    "tail",
  )
  lines
}
"""
  let report = reported(file, source)?
  assert updates(report) == 2, report
  assert "note: " not in report, report
  assert fixed(file)? == r"""proc build() [] -> List[Str] {
  var lines: List[Str] = []
  lines += ["#endif /* GUARD_H */"]
  lines += ["tail"]
  lines
}
"""
}

test test_list_compound_assignment_copies_multiline_arguments_but_not_comments { |ctx|
  let root = test.temp_dir(ctx, name: "list-update-multiline")?
  let file = fp"{root}/update.xsh"
  let source = r"""type Row = {name: Str, size: Int}

proc build() [] -> List[Row] {
  var rows: List[Row] = []
  rows = rows.push(
    {name: "a", size: 1}, # first
  )
  rows = rows.push({
    name: "b",
    size: 2,
  })
  rows
}
"""
  let report = reported(file, source)?
  assert updates(report) == 2, report
  let around_note = report.split("note: comments inside the update require a manual rewrite\n")
  assert around_note.len() == 2, report
  # The note belongs to the first update, which has no fix; the second has
  # a fix and no note.
  assert "help: " not in around_note[0] and ":5:3\n" in around_note[0], report
  assert "help: rewrite as list compound assignment -> rows += [{\n    name: \"b\",\n    size: 2,\n  }]\n" in around_note[1], report
  assert "note: " not in around_note[1], report

  # Copying the argument verbatim keeps the continuation lines' own
  # indentation, so the result is valid source that `fmt` then re-lays out.
  assert fixed(file)? == source.replace(
    "rows = rows.push({\n    name: \"b\",\n    size: 2,\n  })",
    with: "rows += [{\n    name: \"b\",\n    size: 2,\n  }]",
  )
  assert_checks(file)
}

# The formatter lays an overflowing list out one element per line, so the
# fix does too and leaves already formatted source formatted.
test test_list_compound_assignment_wraps_an_update_that_overflows_the_line { |ctx|
  let root = test.temp_dir(ctx, name: "list-update-overflow")?
  let file = fp"{root}/update.xsh"
  let source = r"""proc build(items: List[Str]) [] -> List[Str] {
  var lines: List[Str] = []
  for item in items {
    lines = lines.push(
      f"dependency\t{item.upper()}\t{item.lower()}\t{item.trim()}\t{item.upper()}\t{item.lower()}\t{item.trim()}\t{item}",
    )
    lines = lines.push("short")
  }

  lines
}
"""
  let report = reported(file, source)?
  assert updates(report) == 2, report
  assert_checks(file)
  assert_formatted(file)
  assert fixed(file)? == r"""proc build(items: List[Str]) [] -> List[Str] {
  var lines: List[Str] = []
  for item in items {
    lines += [
      f"dependency\t{item.upper()}\t{item.lower()}\t{item.trim()}\t{item.upper()}\t{item.lower()}\t{item.trim()}\t{item}",
    ]
    lines += ["short"]
  }

  lines
}
"""
  assert_checks(file)
  assert_formatted(file)
}

test test_list_compound_assignment_rewrites_push_chains_only { |ctx|
  let root = test.temp_dir(ctx, name: "list-update-chains")?
  let file = fp"{root}/update.xsh"
  let source = r"""proc build(more: List[Str]) [] -> List[Str] {
  var lines: List[Str] = []
  lines = lines.push("a").push("b")
  lines = lines.push("c").extend(more)
  lines = lines.extend(more).extend(more)
  var index: Map[List[Str]] = {}
  index = index.push("key", "value")
  print ${index.len()}
  lines
}
"""
  let report = reported(file, source)?
  assert updates(report) == 1, report
  assert "note: " not in report, report
  assert fixed(file)? == source.replace("lines = lines.push(\"a\").push(\"b\")", with: "lines += [\"a\", \"b\"]")
}
