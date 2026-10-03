test nullable_local_has_one_fixed_optional_type { |ctx|
  let output = test.run_script(
    ctx,
    """proc choose() -> Path? {
  var selected = null
  for destination in [p"first", p"second"] {
    selected = destination
  }
  selected
}
let chosen = choose()
print \${chosen?.display() ?? ""}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """second
"""
}

test empty_list_collects_loop_contributions_before_earlier_reads { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc gather() -> List[Path] {
  var entries = []
  let before = entries
  for destination in [p"first", p"second"] {
    entries += [destination]
  }
  entries
}
print ${gather()[1].display()}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """second
"""
}

test private_result_inference_consumes_solved_local_collection { |ctx|
  let output = test.run_script(
    ctx,
    r"""pure gather() {
  var entries = []
  for destination in [p"first", p"second"] {
    entries += [destination]
  }
  entries
}
print ${gather()[0].display()}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """first
"""
}

test nullable_local_rejects_incompatible_branch_contributions { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc choose(flag: Bool) -> Path? {
  var selected = null
  if flag {
    selected = p"chosen"
  } else {
    selected = 12
  }
  selected
}
""",
  )?
  assert ! output.success
  assert "check.type-mismatch" in output.stderr, output.stderr
  assert "type inference started here" in output.stderr, output.stderr
  assert "type established here" in output.stderr, output.stderr
}

test unconstrained_material_local_requires_annotation { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc inspect() {
  var entries = []
  let copy = entries
}
""",
  )?
  assert ! output.success
  assert "check.local-inference" in output.stderr, output.stderr
}

test immutable_null_and_discarded_inert_literals_keep_their_types { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc unchanged() -> Unit {
  let _ = []
  let _ = null
  let value = null
  let checked: Null = value
  print "done"
}
unchanged()
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """done
"""
}

test explicit_empty_map_infers_key_and_value_from_indexed_writes { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc gather() -> Map[Path, Int] {
  var entries = map.empty()
  entries[p"first"] = 12
  entries
}
let entries = gather()
print ${entries.get(p"first")?}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """12
"""
}

test authored_immutable_collection_contract_is_shared_by_aliases { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc paths(values: List[Path]) -> Unit {}
proc integers(values: List[Int]) -> Unit {}
proc inspect() -> Unit {
  let entries: List[Path] = []
  let alias = entries
  paths(entries)
  integers(alias)
}
""",
  )?
  assert ! output.success
  assert "check.type-mismatch" in output.stderr, output.stderr
}

test zero_iteration_loop_still_contributes_static_element_type { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc gather() -> List[Path] {
  var entries = []
  for unused in [] {
    entries += [p"never"]
  }
  entries
}
print ${gather().len()}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """0
"""
}

test unannotated_empty_record_does_not_become_map_from_later_use { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc inspect() -> Unit {
  var entries = {}
  entries["key"] = 12
}
""",
  )?
  assert ! output.success
  assert "check.assign-target" in output.stderr, output.stderr
}

test independent_parameter_contract_can_solve_an_empty_local { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc count(values: List[Path]) -> Int { values.len() }
proc inspect() -> Int {
  let entries = []
  count(entries)
}
print ${inspect()}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """0
"""
}

test stream_local_constraints_are_solved_before_yield_preparation { |ctx|
  let output = test.run_script(
    ctx,
    r"""stream destinations() [] -> Stream[Path] {
  var entries = []
  for destination in [p"one", p"two"] { entries += [destination] }
  for destination in entries { yield destination }
}
for destination in destinations() { print ${destination.display()} }
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """one
two
"""
}

test earlier_nullable_operations_use_the_fixed_solved_type { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc choose() -> Path? {
  var selected = null
  let before = selected?.display() ?? "empty"
  selected = p"chosen"
  selected
}
print ${choose()?.display() ?? ""}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """chosen
"""
}

test discarded_empty_map_needs_no_artificial_key_or_value_contract { |ctx|
  let output = test.run_script(
    ctx,
    r"""pure discard() -> Unit {
  let _ = map.empty()
  map.empty()
}
discard()
print done
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """done
"""
}

test material_empty_map_requires_a_concrete_contract { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc inspect() -> Unit {
  let entries = map.empty()
  print entries.len()
}
""",
  )?
  assert ! output.success
  assert "check.local-inference" in output.stderr, output.stderr
}

test imported_callable_keeps_its_concrete_local_contract { |ctx|
  let root = test.temp_dir(ctx, name: "local-inference-module")?
  fp"${root}/collect.xsh".write_atomic(r"""##! Concrete local collection contracts.
## Collect two destinations with a fixed element type.
export pure destinations() -> List[Path] {
  var entries = []
  for destination in [p"one", p"two"] { entries += [destination] }
  entries
}
""")?
  let output = test.run_script(
    ctx,
    r"""use collect as c
print ${c.destinations()[1].display()}
""",
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  assert output.success, output.stderr
  assert output.stdout == """two
"""
}

test empty_list_method_assignments_preserve_the_same_monomorphic_identity { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc gather() -> List[Path] {
  var entries = []
  let initial = entries.len()
  for destination in [p"one", p"two"] { entries = entries.push(destination) }
  entries
}
print ${gather()[1].display()}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """two
"""
}

test empty_list_result_tails_match_explicit_returns { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc explicit(words: List[Str]) [error] -> Result[List[Str]] {
  var values = []
  for word in words {
    if word not in values { values = values.push(word) }
  }
  return values
}
proc implicit(words: List[Str]) [error] -> Result[List[Str]] {
  var values = []
  for word in words {
    if word not in values { values = values.push(word) }
  }
  values
}
for words in [[], ["one", "two", "one"]] {
  let expected = explicit(words)?
  let actual = implicit(words)?
  assert actual == expected
  print ${actual.len()}
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """0
2
"""
}

test declared_return_context_solves_empty_local_identifier_tails { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc list_tail() -> List[Str] {
  let values = []
  values
}
proc result_tail() [error] -> Result[List[Str]] {
  let values = []
  values
}
proc explicit_result_tail() [error] -> Result[List[Str]] {
  let values = []
  return values
}
proc grouped_result_tail() [error] -> Result[List[Str]] {
  let values = []
  (values)
}
proc wrapped_result_tail() [error] -> Result[List[Str]] {
  let values = []
  let wrapped = Ok(values)
  wrapped
}
proc nested_result_tail() [error] -> Result[Result[List[Str]]] {
  let values = []
  let wrapped = Ok(Ok(values))
  wrapped
}
proc map_tail() [error] -> Result[Map[Int]] {
  let values = map.empty()
  values
}
let nested = nested_result_tail()?
let unwrapped = nested?
print ${list_tail().len()} ${result_tail()?.len()} ${explicit_result_tail()?.len()} ${grouped_result_tail()?.len()} ${wrapped_result_tail()?.len()} ${unwrapped.len()} ${map_tail()?.len()}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """0 0 0 0 0 0 0
"""
}

test local_identifier_tails_reject_incompatible_return_shapes { |ctx|
  for tail in ["values", "(values)", "return values"] {
    let output = test.run_script(
      ctx,
      r"""proc incompatible() [error] -> Result[List[Str]] {
  var values = []
  values = values.push(1)
""" + tail + """\n}
""",
    )?
    assert ! output.success
    assert "check.type-mismatch" in output.stderr, output.stderr
  }

  let nested = test.run_script(
    ctx,
    r"""proc incompatible() [error] -> Result[Result[List[Str]]] {
  var values = []
  values = values.push("one")
  let wrapped = Ok(values)
  wrapped
}
""",
  )?
  assert ! nested.success
  assert "check.type-mismatch" in nested.stderr, nested.stderr
}

test explicit_empty_map_fold_constraints_publish_concrete_earlier_gets { |ctx|
  let output = test.run_script(
    ctx,
    r"""let counts = ["one", "two", "one"] |> fold(map.empty()) { |acc, item|
  acc.set(item, (acc.get(item) ?? 0) + 1)
}
print ${counts.get("one") ?? 0}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """2
"""
}

test independently_declared_dynamic_map_domain_solves_nested_local_holes { |ctx|
  let output = test.run_script(
    ctx,
    r"""type Stats = {blobs: Map[Any]}
pure empty_stats() -> Stats {
  let stats = {blobs: map.empty()}
  return stats
}
print ${empty_stats().blobs.len()}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """0
"""
}

test unannotated_empty_local_is_one_monomorphic_hole_across_aliases { |ctx|
  let accepted = test.run_script(
    ctx,
    r"""proc gather() -> List[Path] {
  var entries = []
  let alias = entries
  for destination in [p"one"] { entries += [destination] }
  let paths: List[Path] = alias
  entries
}
print ${gather()[0].display()}
""",
  )?
  assert accepted.success, accepted.stderr
  assert accepted.stdout == """one
"""
  let rejected = test.run_script(
    ctx,
    r"""proc paths(values: List[Path]) -> Unit {}
proc integers(values: List[Int]) -> Unit {}
proc inspect() -> Unit {
  let entries = []
  let alias = entries
  paths(entries)
  integers(alias)
}
""",
  )?
  assert ! rejected.success
  assert "check.type-mismatch" in rejected.stderr, rejected.stderr
}

test solved_locals_keep_exact_types_without_widening_or_dropping_null { |ctx|
  for body in [
    """var values = []
  values += [1]
  values += [1.5]
""",
    """var entries = []
  entries += [p"one"]
  let wrong: List[Str] = entries
""",
    """var selected = null
  selected = p"chosen"
  let wrong: Path = selected
""",
  ] {
    let output = test.run_script(
      ctx,
      """proc inspect() -> Unit {
  """ + body + """}
""",
    )?
    assert ! output.success
    assert "check.type-mismatch" in output.stderr, output.stderr
  }
}
