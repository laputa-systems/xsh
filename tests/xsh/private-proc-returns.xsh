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
proc count() [io] -> Int { print counted; 3 }
proc printed() [io] { print printed }
proc mixed(flag: Bool) [io] { if flag { count() } else { print mixed } }
proc early(flag: Bool) [io] {
  if flag { return }
  if flag { count() } else { count() }
}
proc failing() [error] { Err(LocalError.Bad("failed")) }
let a: Result[Unit] = printed()
let b: Result[Unit] = mixed(false)
let c: Result[Unit] = early(false)
let d: Result[Unit] = failing()
printed()
mixed(true)
early(true)
print ${a is Ok(_)} ${b is Ok(_)} ${c is Ok(_)} ${d is Err(LocalError.Bad)}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """printed
mixed
counted
printed
counted
true true true true
"""
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
    {
      source: """proc mismatch(flag: Bool) { if flag { return 1 }
  "one" }
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
  let diagnostics = fp"${source}.err"
  let _ = run.status "xsht" lint $source 2> $diagnostics
  diagnostics.read_text()?
}

test test_redundant_result_unit_lint_keeps_branching_tails { |ctx|
  let plain = test.temp_file(ctx, name: "plain.xsh", contents: b"proc noop() [] -> Result[Unit] {}\nnoop()\n")?
  let branching = test.temp_file(
    ctx,
    name: "branching.xsh",
    contents: b"proc count() [] -> Int { 1 }\nproc pick(flag: Bool) [] -> Result[Unit] { if flag { count() } else { count() } }\npick(true)\n",
  )?
  let plain_output = lint_output(plain)?
  let branching_output = lint_output(branching)?
  assert "lint.redundant-result-unit" in plain_output, plain_output
  assert "lint.redundant-result-unit" not in branching_output, branching_output
}
