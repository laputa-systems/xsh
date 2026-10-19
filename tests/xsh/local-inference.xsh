test top_level_blocks_retain_annotated_local_types { |ctx|
  let output = test.expect(
    ctx,
    r"""pure identity(value) { value }
if true {
  let text: Str = "nested"
  let result: Result[Str] = Ok(identity(text))
  print ${result?}
}
for raw in ["first", "last"] {
  let selected: Str = raw
  print $selected
}
""",
    status: 0,
  )?
  assert output.stdout == """nested
first
last
"""
}

test nullable_local_has_one_fixed_optional_type { |ctx|
  let output = test.expect(
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
    status: 0,
  )?
  assert output.stdout == """second
"""
}

test empty_list_collects_loop_contributions_before_earlier_reads { |ctx|
  let output = test.expect(
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
    status: 0,
  )?
  assert output.stdout == """second
"""
}

test private_result_inference_consumes_solved_local_collection { |ctx|
  let output = test.expect(
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
    status: 0,
  )?
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

test nullable_local_uses_existing_path_base_for_relpath_contributions { |ctx|
  let output = test.expect(
    ctx,
    r"""pure path_first(flag: Bool, plain: Path, relative: RelPath) {
  var selected = null
  if flag {
    selected = plain
  } else {
    selected = relative
  }
  selected
}
pure relpath_first(flag: Bool, plain: Path, relative: RelPath) {
  var selected = null
  if flag {
    selected = relative
  } else {
    selected = plain
  }
  selected
}
pure only_relative(relative: RelPath) {
  var selected = null
  selected = relative
  selected
}
print ${path_first(true, p"plain", "relative")?.display() ?? ""}
print ${path_first(false, p"plain", "relative")?.display() ?? ""}
print ${relpath_first(true, p"plain", "relative")?.display() ?? ""}
print ${relpath_first(false, p"plain", "relative")?.display() ?? ""}
print ${only_relative("relative")?.display() ?? ""}
""",
    status: 0,
  )?
  assert output.stdout == """plain
relative
relative
plain
relative
"""

  for body in [
    "selected = plain\n  selected = text",
    "selected = text\n  selected = plain",
  ] {
    let rejected = test.run_script(
      ctx,
      """pure incompatible(plain: Path, text: Str) {
  var selected = null
  """ + body + """
  selected
}
""",
    )?
    assert ! rejected.success
    assert "check.type-mismatch" in rejected.stderr, rejected.stderr
  }

  for source in [
    r"""pure incompatible(first: List[Str], second: List[Path]) {
  var selected = null
  selected = first
  selected = second
  selected
}
""",
    r"""pure incompatible(first: Map[Str, Str], second: Map[Str, Path]) {
  var selected = null
  selected = first
  selected = second
  selected
}
""",
  ] {
    let rejected = test.run_script(ctx, source)?
    assert ! rejected.success
    assert "check.type-mismatch" in rejected.stderr, rejected.stderr
  }
}

test if_expression_uses_existing_optional_branch_in_either_order { |ctx|
  let output = test.expect(
    ctx,
    r"""pure narrowed_first(value: Str?, fallback: Str?) -> Str? {
  let selected = if value != null { value } else { fallback }
  selected
}
pure optional_first(value: Str?, fallback: Str?) -> Str? {
  let selected = if value == null { fallback } else { value }
  selected
}
pure narrowed_first_else_if(flag: Bool, value: Str?, fallback: Str?) -> Str? {
  let selected = if value != null { value } else if flag { fallback } else { fallback }
  selected
}
print ${narrowed_first("present", "fallback") ?? ""}
print ${narrowed_first(null, "fallback") ?? ""}
print ${optional_first(null, "fallback") ?? ""}
print ${optional_first("present", "fallback") ?? ""}
print ${narrowed_first_else_if(true, "present", "fallback") ?? ""}
print ${narrowed_first_else_if(true, null, "fallback") ?? ""}
print ${narrowed_first_else_if(false, null, "fallback") ?? ""}
""",
    status: 0,
  )?
  assert output.stdout == """present
fallback
fallback
present
present
fallback
fallback
"""

  for body in [
    "if flag { null } else { 1 }",
    "if flag { 1 } else { null }",
  ] {
    let rejected = test.run_script(
      ctx,
      """pure incompatible(flag: Bool) -> Int? {
  let selected = """ + body + """
  selected
}
""",
    )?
    assert ! rejected.success
    assert "check.type-mismatch" in rejected.stderr, rejected.stderr
  }

  let strict = test.run_script(
    ctx,
    r"""pure strict(value: Str?, fallback: Str?) -> Str {
  let selected: Str = if value != null { value } else { fallback }
  selected
}
""",
  )?
  assert ! strict.success
  assert "check.type-mismatch" in strict.stderr, strict.stderr
}

test if_expression_forwards_optional_candidates_through_value_tails { |ctx|
  let root = test.temp_dir(ctx, name: "optional-branch-context")?
  fp"{root}/choices.xsh".write_atomic("##! Choices.\n## Enum of choices.\nexport enum Choice { Ready, Stopped }\n")
  let output = test.expect(
    ctx,
    r"""use choices as c

pure value_block(outer: Bool, fallback: Str?, inner: Bool) -> Str? {
  let selected = if outer { fallback } else {
    { if inner { null } else { fallback } }
  }
  selected
}
pure nested_if(value: Str?, fallback: Str?, inner: Bool) -> Str? {
  let selected = if value != null { value } else {
    if inner { fallback } else { fallback }
  }
  selected
}
pure nested_statement_if(outer: Bool, fallback: Str?, inner: Bool) -> Str? {
  let selected = if outer { fallback } else {
    { if inner { null } else { "fallback" } }
  }
  selected
}
pure enum_candidate(flag: Bool) -> c.Choice {
  let selected = if flag { c.Ready } else { .Stopped }
  selected
}
pure nested_enum_candidate(outer: Bool, inner: Bool, fallback: c.Choice?) -> c.Choice? {
  let selected = if outer { fallback } else {
    if inner { null } else { .Stopped }
  }
  selected
}
pure empty_list_context(outer: Bool, inner: Bool, fallback: List[Int]?) -> List[Int]? {
  let selected = if outer { fallback } else {
    if inner { [] } else { fallback }
  }
  selected
}
print ${value_block(true, "present", false) ?? ""}
print (value_block(false, null, true) == null)
print (value_block(false, "fallback", false) == "fallback")
print ${nested_if("present", "fallback", true) ?? ""}
print ${nested_if(null, "fallback", false) ?? ""}
print ${nested_statement_if(true, "present", false) ?? ""}
print (nested_statement_if(false, null, true) == null)
print (nested_statement_if(false, null, false) == "fallback")
print (enum_candidate(true) == c.Ready)
print (enum_candidate(false) == c.Stopped)
print (nested_enum_candidate(true, false, c.Ready) == c.Ready)
print (nested_enum_candidate(false, true, null) == null)
print (nested_enum_candidate(false, false, null) == c.Stopped)
print ${empty_list_context(false, true, null)?.len() ?? -1}
print ${empty_list_context(true, false, null)?.len() ?? -1}
""",
    status: 0,
    env: {XSH_MODULE_PATH: root},
  )?
  assert output.stdout == """present
true
true
present
fallback
present
true
true
true
true
true
true
true
0
-1
"""

  for body in [
    "if flag { \"text\" } else { 1 }",
    "if flag { 1 } else { \"text\" }",
  ] {
    let rejected = test.run_script(
      ctx,
      """pure incompatible(flag: Bool) {
  let selected = """ + body + """
  selected
}
""",
    )?
    assert ! rejected.success
    assert "check.infer-return" in rejected.stderr, rejected.stderr
  }

  for source in [
    r"""use choices as c
enum LocalChoice { Ready, Stopped }
pure malformed_variant(flag: Bool) -> LocalChoice {
  let selected = if flag { LocalChoice.Ready } else { .Missing }
  selected
}
""",
    r"""use choices as c
pure strict_return(value: Str?, fallback: Str?) -> Str {
  if value != null { value } else { fallback }
}
""",
    r"""pure strict_non_tail(value: Str?, fallback: Str?) -> Str? {
  let selected = if value != null {
    let required: Str = fallback
    value
  } else {
    fallback
  }
  selected
}
""",
  ] {
    let rejected = test.run_script(ctx, source, [], {XSH_MODULE_PATH: root})?
    assert ! rejected.success
    let expected_code = if "malformed_variant" in source {
      "check.inferred-variant"
    } else {
      "check.type-mismatch"
    }
    assert expected_code in rejected.stderr, rejected.stderr
  }
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
  let output = test.expect(
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
    status: 0,
  )?
  assert output.stdout == """done
"""
}

test explicit_empty_map_infers_key_and_value_from_indexed_writes { |ctx|
  let output = test.expect(
    ctx,
    r"""proc gather() -> Map[Path, Int] {
  var entries = map.empty()
  entries[p"first"] = 12
  entries
}
let entries = gather()
print ${entries.get(p"first")?}
""",
    status: 0,
  )?
  assert output.stdout == """12
"""
}

test mutable_empty_set_infers_one_element_type_from_branches_and_loops { |ctx|
  let output = test.expect(
    ctx,
    r"""pure gather(words: List[Str], include_branch: Bool) -> Set[Str] {
  var seen = set.empty()
  let alias = seen
  if include_branch and alias.is_empty() { seen = seen.add("branch") }
  for word in words { seen = seen.add(word) }
  seen
}
let empty = gather([], false)
let populated = gather(["one", "two", "one"], true)
assert empty.is_empty()
print ${empty.len()} ${populated.to_list().join(",")}

pure gather_count(words: List[Str], include_branch: Bool) -> Int {
  var seen = set.empty()
  let alias = seen
  let before = alias.len()
  if include_branch and alias.is_empty() { seen = seen.add("branch") }
  for word in words { seen = seen.add(word) }
  before + seen.len()
}
let empty_count = gather_count([], false)
let populated_count = gather_count(["one", "two", "one"], true)
print ${empty_count} ${populated_count}
""",
    status: 0,
  )?
  assert output.stdout == """0 branch,one,two
0 3
"""
}

test mutable_empty_set_rejects_incompatible_and_unsupported_elements { |ctx|
  let incompatible = test.run_script(
    ctx,
    r"""proc inspect(flag: Bool) -> Unit {
  var seen = set.empty()
  if flag { seen = seen.add("text") } else { seen = seen.add(1) }
}
""",
  )?
  assert ! incompatible.success
  assert "check.type-mismatch" in incompatible.stderr, incompatible.stderr

  let incompatible_alias = test.run_script(
    ctx,
    r"""proc inspect() -> Unit {
  var seen = set.empty()
  let alias = seen
  seen = seen.add("text")
  let integers: Set[Int] = alias
}
""",
  )?
  assert ! incompatible_alias.success
  assert "check.type-mismatch" in incompatible_alias.stderr, incompatible_alias.stderr

  let unsupported = test.run_script(
    ctx,
    r"""proc inspect() -> Unit {
  var seen = set.empty()
  seen = seen.add(1.5)
}
""",
  )?
  assert ! unsupported.success
  assert "check.set-element-type" in unsupported.stderr, unsupported.stderr
}

test empty_set_without_mutable_local_seed_still_needs_context { |ctx|
  for source in [
    "proc immutable() -> Unit { let seen = set.empty() }\n",
    "var seen = set.empty()\n",
    "proc unresolved() -> Unit { var seen = set.empty(); let copy = seen }\n",
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success
    assert "check.local-inference" in output.stderr, output.stderr
  }

  let shadowed = test.run_script(
    ctx,
    "proc shadowed() -> Unit { let set = 1; var seen = set.empty() }\n",
  )?
  assert ! shadowed.success
  assert "check.standard-module-shadow" in shadowed.stderr, shadowed.stderr
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
  let output = test.expect(
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
    status: 0,
  )?
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
  let output = test.expect(
    ctx,
    r"""proc count(values: List[Path]) -> Int { values.len() }
proc inspect() -> Int {
  let entries = []
  count(entries)
}
print ${inspect()}
""",
    status: 0,
  )?
  assert output.stdout == """0
"""
}

test stream_local_constraints_are_solved_before_yield_preparation { |ctx|
  let output = test.expect(
    ctx,
    r"""stream destinations() [] -> Stream[Path] {
  var entries = []
  for destination in [p"one", p"two"] { entries += [destination] }
  for destination in entries { yield destination }
}
for destination in destinations() { print ${destination} }
""",
    status: 0,
  )?
  assert output.stdout == """one
two
"""
}

test earlier_nullable_operations_use_the_fixed_solved_type { |ctx|
  let output = test.expect(
    ctx,
    r"""proc choose() -> Path? {
  var selected = null
  let before = selected?.display() ?? "empty"
  selected = p"chosen"
  selected
}
print ${choose()?.display() ?? ""}
""",
    status: 0,
  )?
  assert output.stdout == """chosen
"""
}

test discarded_empty_map_needs_no_artificial_key_or_value_contract { |ctx|
  let output = test.expect(
    ctx,
    r"""pure discard() -> Unit {
  let _ = map.empty()
  map.empty()
}
discard()
print done
""",
    status: 0,
  )?
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
  fp"{root}/collect.xsh".write_atomic(r"""##! Concrete local collection contracts.
## Collect two destinations with a fixed element type.
export pure destinations() -> List[Path] {
  var entries = []
  for destination in [p"one", p"two"] { entries += [destination] }
  entries
}
""")
  let output = test.expect(
    ctx,
    r"""use collect as c
print ${c.destinations()[1].display()}
""",
    status: 0,
    args: [],
    env: {XSH_MODULE_PATH: root},
  )?
  assert output.stdout == """two
"""
}

test empty_list_method_assignments_preserve_the_same_monomorphic_identity { |ctx|
  let output = test.expect(
    ctx,
    r"""proc gather() -> List[Path] {
  var entries = []
  let initial = entries.len()
  for destination in [p"one", p"two"] { entries = entries.push(destination) }
  entries
}
print ${gather()[1].display()}
""",
    status: 0,
  )?
  assert output.stdout == """two
"""
}

test empty_list_result_tails_match_explicit_returns { |ctx|
  let output = test.expect(
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
    status: 0,
  )?
  assert output.stdout == """0
2
"""
}

test declared_return_context_solves_empty_local_identifier_tails { |ctx|
  let output = test.expect(
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
    status: 0,
  )?
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
  let output = test.expect(
    ctx,
    r"""let counts = ["one", "two", "one"] |> fold(map.empty()) { |acc, item|
  acc.set(item, (acc.get(item) ?? 0) + 1)
}
print ${counts.get("one") ?? 0}
""",
    status: 0,
  )?
  assert output.stdout == """2
"""
}

test independently_declared_dynamic_map_domain_solves_nested_local_holes { |ctx|
  let output = test.expect(
    ctx,
    r"""type Stats = {blobs: Map[Any]}
pure empty_stats() -> Stats {
  let stats = {blobs: map.empty()}
  return stats
}
print ${empty_stats().blobs.len()}
""",
    status: 0,
  )?
  assert output.stdout == """0
"""
}

test unannotated_empty_local_is_one_monomorphic_hole_across_aliases { |ctx|
  let accepted = test.expect(
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
    status: 0,
  )?
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
