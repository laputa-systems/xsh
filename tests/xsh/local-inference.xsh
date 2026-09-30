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
