test nullable_local_has_one_fixed_optional_type [error] { |ctx|
  let output = test.run_script(ctx, """proc choose() -> Path? {
  var selected = null
  for destination in [p"first", p"second"] {
    selected = destination
  }
  selected
}
let chosen = choose()
print \${chosen?.display() ?? ""}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "second\n")?
}

test empty_list_collects_loop_contributions_before_earlier_reads [error] { |ctx|
    let output = test.run_script(ctx, r"""proc gather() -> List[Path] {
  var entries = []
  let before = entries
  for destination in [p"first", p"second"] {
    entries += [destination]
  }
  entries
}
print ${gather()[1].display()}
""")?
    test.ok(output.success, output.stderr)?
    test.eq(output.stdout, "second\n")?
}

test private_result_inference_consumes_solved_local_collection [error] { |ctx|
    let output = test.run_script(ctx, r"""pure gather() {
  var entries = []
  for destination in [p"first", p"second"] {
    entries += [destination]
  }
  entries
}
print ${gather()[0].display()}
""")?
    test.ok(output.success, output.stderr)?
    test.eq(output.stdout, "first\n")?
}

test nullable_local_rejects_incompatible_branch_contributions [error] { |ctx|
    let output = test.run_script(ctx, r"""proc choose(flag: Bool) -> Path? {
  var selected = null
  if flag {
    selected = p"chosen"
  } else {
    selected = 12
  }
  selected
}
""")?
    test.eq(output.success, false)?
    test.ok(output.stderr.contains("check.type-mismatch"), output.stderr)?
    test.ok(output.stderr.contains("type inference started here"), output.stderr)?
    test.ok(output.stderr.contains("type established here"), output.stderr)?
}

test unconstrained_material_local_requires_annotation [error] { |ctx|
    let output = test.run_script(ctx, r"""proc inspect() {
  var entries = []
  let copy = entries
}
""")?
    test.eq(output.success, false)?
    test.ok(output.stderr.contains("check.local-inference"), output.stderr)?
}

test immutable_null_and_discarded_inert_literals_keep_their_types [error] { |ctx|
    let output = test.run_script(ctx, r"""proc unchanged() -> Unit {
  let _ = []
  let _ = null
  let value = null
  let checked: Null = value
  print "done"
}
unchanged()
""")?
    test.ok(output.success, output.stderr)?
    test.eq(output.stdout, "done\n")?
}

test explicit_empty_map_infers_key_and_value_from_indexed_writes [error] { |ctx|
    let output = test.run_script(ctx, r"""proc gather() -> Map[Path, Int] {
  var entries = map.empty()
  entries[p"first"] = 12
  entries
}
let entries = gather()
print ${entries.get(p"first")?}
""")?
    test.ok(output.success, output.stderr)?
    test.eq(output.stdout, "12\n")?
}

test immutable_aliases_share_one_collection_type [error] { |ctx|
    let output = test.run_script(ctx, r"""proc paths(values: List[Path]) -> Unit {}
proc integers(values: List[Int]) -> Unit {}
proc inspect() -> Unit {
  let entries = []
  let alias = entries
  paths(entries)
  integers(alias)
}
""")?
    test.eq(output.success, false)?
    test.ok(output.stderr.contains("check.type-mismatch"), output.stderr)?
}

test zero_iteration_loop_still_contributes_static_element_type [error] { |ctx|
    let output = test.run_script(ctx, r"""proc gather() -> List[Path] {
  var entries = []
  for unused in [] {
    entries += [p"never"]
  }
  entries
}
print ${gather().len()}
""")?
    test.ok(output.success, output.stderr)?
    test.eq(output.stdout, "0\n")?
}

test unannotated_empty_record_does_not_become_map_from_later_use [error] { |ctx|
    let output = test.run_script(ctx, r"""proc inspect() -> Unit {
  var entries = {}
  entries["key"] = 12
}
""")?
    test.eq(output.success, false)?
    test.ok(output.stderr.contains("check.assign-target"), output.stderr)?
}

test independent_parameter_contract_can_solve_an_empty_local [error] { |ctx|
    let output = test.run_script(ctx, r"""proc count(values: List[Path]) -> Int { values.len() }
proc inspect() -> Int {
  let entries = []
  count(entries)
}
print ${inspect()}
""")?
    test.ok(output.success, output.stderr)?
    test.eq(output.stdout, "0\n")?
}

test stream_local_constraints_are_solved_before_yield_preparation [error] { |ctx|
    let output = test.run_script(ctx, r"""stream destinations() [] -> Stream[Path] {
  var entries = []
  for destination in [p"one", p"two"] { entries += [destination] }
  for destination in entries { yield destination }
}
for destination in destinations() { print ${destination.display()} }
""")?
    test.ok(output.success, output.stderr)?
    test.eq(output.stdout, "one\ntwo\n")?
}

test earlier_nullable_operations_use_the_fixed_solved_type [error] { |ctx|
    let output = test.run_script(ctx, r"""proc choose() -> Path? {
  var selected = null
  let before = selected?.display() ?? "empty"
  selected = p"chosen"
  selected
}
print ${choose()?.display() ?? ""}
""")?
    test.ok(output.success, output.stderr)?
    test.eq(output.stdout, "chosen\n")?
}

test discarded_empty_map_needs_no_artificial_key_or_value_contract [error] { |ctx|
    let output = test.run_script(ctx, r"""pure discard() -> Unit {
  let _ = map.empty()
  map.empty()
}
discard()
print done
""")?
    test.ok(output.success, output.stderr)?
    test.eq(output.stdout, "done\n")?
}

test material_empty_map_requires_a_concrete_contract [error] { |ctx|
    let output = test.run_script(ctx, r"""proc inspect() -> Unit {
  let entries = map.empty()
  print entries.len()
}
""")?
    test.eq(output.success, false)?
    test.ok(output.stderr.contains("check.local-inference"), output.stderr)?
}

test imported_callable_keeps_its_concrete_local_contract [fs, error] { |ctx|
    let root = test.temp_dir(ctx, name: "local-inference-module")?
    fp"${root}/collect.xsh".write_atomic(r"""##! Concrete local collection contracts.
## Collect two destinations with a fixed element type.
export pure destinations() -> List[Path] {
  var entries = []
  for destination in [p"one", p"two"] { entries += [destination] }
  entries
}
""")?
    let output = test.run_script(ctx, r"""use collect as c
print ${c.destinations()[1].display()}
""", [], {XSH_MODULE_PATH: root.display()})?
    test.ok(output.success, output.stderr)?
    test.eq(output.stdout, "two\n")?
}

test empty_list_method_assignments_preserve_the_same_monomorphic_identity [error] { |ctx|
    let output = test.run_script(ctx, r"""proc gather() -> List[Path] {
  var entries = []
  let initial = entries.len()
  for destination in [p"one", p"two"] { entries = entries.push(destination) }
  entries
}
print ${gather()[1].display()}
""")?
    test.ok(output.success, output.stderr)?
    test.eq(output.stdout, "two\n")?
}

test explicit_empty_map_fold_constraints_publish_concrete_earlier_gets [error] { |ctx|
    let output = test.run_script(ctx, r"""let counts = ["one", "two", "one"] |> fold(map.empty()) { |acc, item|
  acc.set(item, (acc.get(item) ?? 0) + 1)
}
print ${counts.get("one") ?? 0}
""")?
    test.ok(output.success, output.stderr)?
    test.eq(output.stdout, "2\n")?
}

test independently_declared_dynamic_map_domain_solves_nested_local_holes [error] { |ctx|
    let output = test.run_script(ctx, r"""type Stats = {blobs: Map[Any]}
pure empty_stats() -> Stats {
  let stats = {blobs: map.empty()}
  return stats
}
print ${empty_stats().blobs.len()}
""")?
    test.ok(output.success, output.stderr)?
    test.eq(output.stdout, "0\n")?
}
