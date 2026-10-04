proc inferred_count(items: List[Str]) [io] {
  print items.len()
  items.len()
}

proc inferred_label(name: Str) {
  name.trim().lower()
}

proc inferred_items() {
  ["a", "b"]
}

proc inferred_record(value: Int) {
  {value, label: "ready"}
}

proc inferred_optional(flag: Bool) {
  if flag {
    7
  } else {
    null
  }
}

proc inferred_flag(value: Int) {
  value > 0
}

proc inferred_early(value: Int) {
  guard value >= 0 else {
    return "negative"
  }
  "positive"
}

proc inferred_parsed(text: Str) {
  text.parse_int()? + 1
}

proc inferred_match(text: Str) {
  match text {
    "one" => 1
    _ => 0
  }
}

proc inferred_forward(text: Str) {
  inferred_parsed(text)? * 2
}

test test_private_proc_value_tails_infer_result_success_types {
  assert inferred_count(["x", "y"])? == 2
  assert inferred_label(" Label ")? == "label"
  assert inferred_items()? == ["a", "b"]
  assert inferred_record(4)?.value == 4
  assert inferred_optional(false)? == null
  assert inferred_optional(true)? == 7
  assert inferred_flag(-1)? == false
  assert inferred_early(-1)? == "negative"
  assert inferred_early(1)? == "positive"
  assert inferred_match("one")? == 1
  assert inferred_forward("4")? == 10
  let failed = inferred_forward("bad")
  assert failed is Err(_)
  let captured: Result[Int] = try {
    inferred_parsed("bad")?
  }
  assert captured is Err(_)
}

test test_private_proc_inference_follows_declarations_not_source_order { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc first(value: Int) { second(value)? + 1 }
proc second(value: Int) { value * 2 }
let checked: Result[Int] = first(3)
print ${checked?}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """7
"""
}

test test_private_proc_statement_completions_keep_result_unit { |ctx|
  let output = test.run_script(
    ctx,
    r"""error LocalError = Bad(message: Str)
proc printed() [io] { print printed }
proc branching(flag: Bool) [io, error] { if flag { printed() } else { print branching } }
proc early(flag: Bool) [io] {
  if flag { return }
  if flag { print never }
}
proc failing() [error] { Err(LocalError.Bad("failed")) }
let a: Result[Unit] = printed()
let b: Result[Unit] = branching(false)
let c: Result[Unit] = early(false)
let d: Result[Unit] = failing()
branching(true)
early(true)
print ${a is Ok(_)} ${b is Ok(_)} ${c is Ok(_)} ${d is Err(LocalError.Bad)}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """printed
branching
printed
true true true true
"""
}

test test_private_proc_failure_only_completion_joins_value_tails { |ctx|
  let output = test.run_script(
    ctx,
    r"""error LocalError = Bad(message: Str)
proc checked(value: Int) [error] {
  if value < 0 { Err(LocalError.Bad("negative")) } else { value * 2 }
}
let doubled: Result[Int] = checked(4)
print ${doubled?} ${checked(-1) is Err(LocalError.Bad)}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """8 true
"""
}

test test_private_proc_disagreeing_completions_are_type_mismatches { |ctx|
  for case in [
    {
      source: r"""proc pick(flag: Bool) { if flag { 1 } else { "one" } }
let v = pick(true)?
print "got ${v}"
""",
      message: "completions of proc `pick` produce `Int` and `Str`",
    },
    {
      source: r"""proc pick(flag: Bool) { if flag { return 1 }
  "one" }
""",
      message: "completions of proc `pick` produce `Int` and `Str`",
    },
    {
      source: r"""proc count() [io] -> Int { print counted; 3 }
proc pick(flag: Bool) [io] { if flag { count() } else { print none } }
""",
      message: "completions of proc `pick` produce `Int` and `Unit`",
    },
    {
      source: r"""proc pick(flag: Bool) {
  if flag { return }
  7
}
""",
      message: "completions of proc `pick` produce `Int` and `Unit`",
    },
  ] {
    let output = test.run_script(ctx, case.source)?
    assert output.status == 2, case.source
    assert "check.type-mismatch" in output.stderr, output.stderr
    assert case.message in output.stderr, output.stderr
    assert "-> Result[Int]" in output.stderr, output.stderr
    assert "check.argv-conversion" not in output.stderr, output.stderr
  }
}

test test_private_proc_inference_rejects_boundaries { |ctx|
  for case in [
    {
      source: """export proc public_value() { 1 }
""",
      code: "check.required-return",
    },
    {
      source: """proc main() { 1 }
""",
      code: "check.required-return",
    },
    {
      source: """proc count(depth: Int) { if depth == 0 { 0 } else { count(depth - 1)? + 1 } }
""",
      code: "check.required-return",
    },
    {
      source: """proc empty() { [] }
""",
      code: "check.infer-return",
    },
  ] {
    let output = test.run_script(ctx, case.source)?
    assert output.status == 2, case.source
    assert case.code in output.stderr, output.stderr
  }
}

test test_private_proc_recursive_statement_bodies_remain_unannotated { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc walk(depth: Int) [io, error] {
  if depth > 0 { print $depth; walk(depth - 1) }
}
walk(2)
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """2
1
"""
}

proc lint_output(source: Path) [fs, process, error] {
  let diagnostics = fp"{source}.err"
  let _ = run.status "xsht" lint $source 2> $diagnostics
  diagnostics.read_text()?
}

test test_redundant_result_unit_lint_keeps_branching_tails { |ctx|
  let plain = test.temp_file(ctx, name: "plain.xsh", contents: b"proc noop() [] -> Result[Unit] {}\nnoop()\n")?
  let branching = test.temp_file(
    ctx,
    name: "branching.xsh",
    contents: b"proc pick(flag: Bool) [process] -> Result[Unit] { if flag { run.status true } else { run.status false } }\npick(true)\n",
  )?
  let plain_output = lint_output(plain)?
  let branching_output = lint_output(branching)?
  assert "lint.redundant-result-unit" in plain_output, plain_output
  assert "err[" not in branching_output, branching_output
  assert "lint.redundant-result-unit" not in branching_output, branching_output
}

test test_private_proc_early_err_returns_join_propagated_errors { |ctx|
  let output = test.run_script(
    ctx,
    r"""error PortError = Missing(file: Path) | Invalid(text: Str)
proc read_port(file: Path) [fs, error] {
  guard file.exists()? else {
    return Err(PortError.Missing(file:))
  }
  let text = file.read_text()?.trim()
  text.parse_int() ?? { |_| Err(PortError.Invalid(text:))? }
}
proc classify(text: Str) [error] {
  if text == "" { return Err(PortError.Missing(file: p"")) }
  if text == "x" { return Err(PortError.Invalid(text:)) }
  text
}
let missing = read_port(p"/nonexistent/port")
let port: Result[Int] = read_port(p"/nonexistent/port")
let family: Result[Str, PortError] = classify("ok")
print ${missing is Err(PortError.Missing)} ${port is Err(_)} ${family?} ${classify("x") is Err(PortError.Invalid)}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """true true ok true
"""
}
