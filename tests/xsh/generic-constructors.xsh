test test_generic_constructor_fields_establish_concrete_instances { |ctx|
  let executed = test.run_script(
    ctx,
    r"""type Box[T] = {value: T, items: List[T] = []}
type Observation[T] = {value: T? = null, samples: List[T] = []}
let count = Box(value: 12)
let text = Box(value: "demo")
let observed = Observation(value: 7)
let samples = Observation(samples: [3, 4])
print ${count.value + (observed.value ?? 0)}
print ${text.value.upper()}
print ${samples.samples[0] + samples.samples[1]}
print ${count.items.len()}
""",
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """19
DEMO
7
0
"""
}

test test_generic_constructor_context_anchors_null_empty_and_unused_parameters { |ctx|
  let executed = test.run_script(
    ctx,
    r"""type Observation[T] = {value: T? = null, samples: List[T] = []}
type Marker[T] = {name: Str}
let absent: Observation[Int] = Observation(value: null)
let empty: Observation[Str] = Observation(samples: [])
let marker: Marker[Int] = Marker(name: "anchored")
print ${absent.value ?? 8}
print ${empty.samples.len()}
print $marker.name
""",
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """8
0
anchored
"""
}

test test_generic_constructor_unresolved_and_conflicting_fields_are_rejected { |ctx|
  for source in [
    """type Box[T] = {value: T?}
let missing = Box(value: null)
""",
    """type Box[T] = {values: List[T]}
let empty = Box(values: [])
""",
    """type Box[T] = {left: T, right: T}
let conflict = Box(left: 1, right: "wrong")
""",
    """type Box[T] = {left: T, right: T}
let conflict = Box(left: 1, right: 2.0)
""",
    """type Marker[T] = {name: Str}
let missing = Marker(name: "unused")
""",
  ] {
    let rejected = test.run_script(ctx, source)?
    assert ! rejected.success, rejected.stderr
    assert "check." in rejected.stderr
  }
}

test test_generic_constructor_nested_fields_share_only_occurrence_constraints { |ctx|
  let executed = test.run_script(
    ctx,
    r"""type Inner[T] = {value: T?}
type Outer[T] = {inner: Inner[T], anchor: T}
type Marker[T] = {name: Str}
type Marked[T] = {marker: Marker[T], anchor: T}
let first = Outer(inner: Inner(value: null), anchor: 7)
let second = Outer(inner: Inner(value: "inner"), anchor: "outer")
let marked = Marked(marker: Marker(name: "kept"), anchor: 3)
let contextual: List[Marker[Int]] = [Marker(name: "list")]
print ${first.inner.value ?? first.anchor}
print ${second.inner.value ?? "missing"}
print $marked.marker.name
print ${contextual[0].name}
""",
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """7
inner
kept
list
"""
}

test test_generic_constructor_uses_annotated_function_slots_and_returns { |ctx|
  let executed = test.run_script(
    ctx,
    r"""type Marker[T] = {name: Str}
type Observation[T] = {value: T?}
pure use_marker(value: Marker[Int]) -> Str {
  value.name
}
pure absent() -> Observation[Int] {
  Observation(value: null)
}
pure marker() -> Marker[Int] {
  return Marker(name: "return")
}
pure result_marker() -> Result[Marker[Int]] {
  Ok(Marker(name: "result"))
}
print (use_marker(Marker(name: "slot")))
print ${absent().value ?? 7}
print (marker().name)
print (result_marker()?.name)
""",
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """slot
7
return
result
"""
}

test test_generic_constructor_preserves_named_spreads_puns_and_field_order { |ctx|
  let executed = test.run_script(
    ctx,
    r"""type Pair[T] = {left: T, right: T, items: List[T] = []}
var visits = 0
proc next() -> Int {
  visits += 1
  visits
}
let left = 7
let punned = Pair(left:, right: 8)
let spread = Pair(...{left: 3}, right: 4)
let ordered = Pair(left: next(), right: next())
print ${punned.left + punned.right}
print ${spread.left + spread.right}
print ${ordered.left * 10 + ordered.right}
print $visits
""",
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """15
7
12
2
"""
  let duplicate = test.run_script(
    ctx,
    """type Pair[T] = {left: T, right: T}
let wrong = Pair(...{left: 1}, left: 2, right: 3)
""",
  )?
  assert ! duplicate.success, duplicate.stderr
  assert "check." in duplicate.stderr
}

test test_generic_constructor_constants_use_the_same_field_and_context_constraints { |ctx|
  let executed = test.run_script(
    ctx,
    r"""type Inner[T] = {value: T?}
type Outer[T] = {inner: Inner[T], anchor: T, items: List[T] = []}
type Marker[T] = {name: Str}
const inferred = Outer(inner: Inner(value: null), anchor: 7)
const marked: Marker[Int] = Marker(name: "constant")
const spread = Outer(...{inner: {value: "text"}, anchor: "anchor"})
print ${inferred.inner.value ?? inferred.anchor}
print $marked.name
print ${spread.inner.value ?? "missing"}
print ${inferred.items.len()}
""",
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """7
constant
text
0
"""
}

test test_generic_constructor_retains_both_typed_map_parameters { |ctx|
  let executed = test.run_script(
    ctx,
    r"""type Table[K, V] = {entries: Map[K, V]}
let numbers = Table(entries: {[1]: "one", [2]: "two"})
let strings = Table(entries: {one: 1, two: 2})
let nested: Table[Int, Str] = Table(entries: {})
const prepared = Table(entries: {[3]: "three"})
print ${numbers.entries[1].upper()}
print ${strings.entries["one"] + strings.entries["two"]}
print ${nested.entries.len()}
print ${prepared.entries[3]}
""",
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """ONE
3
0
three
"""
}

test test_generic_constructor_context_reaches_nested_container_instances { |ctx|
  let executed = test.run_script(
    ctx,
    r"""type Marker[T] = {name: Str}
type Batch[T] = {markers: List[Marker[T]], keyed: Map[Marker[T]], anchor: T}
let value = Batch(markers: [Marker(name: "list")], keyed: {first: Marker(name: "map")}, anchor: 7)
let selected: List[Marker[Int]] = [Marker(name: "expected")]
const made = Batch(markers: [Marker(name: "constant")], keyed: {first: Marker(name: "prepared")}, anchor: "anchor")
print ${value.markers[0].name}
print ${value.keyed["first"].name}
print ${selected[0].name}
print ${made.markers[0].name}
print ${made.keyed["first"].name}
""",
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """list
map
expected
constant
prepared
"""
}

test test_generic_constructor_phantom_arguments_preserve_structural_assignability { |ctx|
  let executed = test.run_script(
    ctx,
    r"""type Marker[T] = {name: Str}
type Wrap[T] = {marker: Marker[T]}
let marker: Marker[Str] = Marker(name: "structural")
let wrapped: Wrap[Int] = Wrap(marker: marker)
print $wrapped.marker.name
""",
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """structural
"""
  let unresolved = test.run_script(
    ctx,
    """type Marker[T] = {name: Str}
type Wrap[T] = {marker: Marker[T]}
let marker: Marker[Str] = Marker(name: "value")
let wrapped = Wrap(marker: marker)
""",
  )?
  assert ! unresolved.success, unresolved.stderr
  assert "check.constructor-inference" in unresolved.stderr
}

test test_generic_constructor_qualified_aliases_keep_private_schema_owners { |ctx|
  let root = test.temp_dir(ctx, name: "generic-constructor-module")?
  fp"${root}/model.xsh".write_atomic("""##! Schemas retain private field ownership.
type Local = {name: Str}
## A generic record with a private dependency.
export type Box[T] = {value: T, owner: Local}
## A generic alias with a nested argument.
export type Alias[T] = Box[List[T]]
""")?
  let executed = test.run_script(
    ctx,
    r"""use model as m
type Local = {name: Int}
let count = m.Box(value: 7, owner: {name: "private"})
let values = m.Alias(value: [3, 4], owner: {name: "alias"})
print ${count.value + values.value[0]}
print ${count.owner.name.upper()}
""",
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """10
PRIVATE
"""
}

test test_generic_constructor_explicit_any_and_null_are_authoritative { |ctx|
  let executed = test.run_script(
    ctx,
    r"""type Box[T] = {value: T}
let dynamic: Box[Any] = Box(value: 7)
let nothing: Box[Null] = Box(value: null)
print $dynamic.value
print ${nothing.value == null}
""",
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """7
true
"""
  let rejected = test.run_script(
    ctx,
    """type Box[T] = {value: T}
let raw: Any = 7
let unknown = Box(value: raw)
""",
  )?
  assert ! rejected.success, rejected.stderr
  assert "check.constructor-inference" in rejected.stderr
}

test test_generic_constructor_receiver_slots_retain_declared_application_context { |ctx|
  let executed = test.run_script(
    ctx,
    r"""type Marker[T] = {name: Str}
type Holder[T] = {items: List[Marker[T]]}
pure initial() -> List[Marker[Int]] { [] }
let declared: List[Marker[Int]] = []
let first = declared.push(Marker(name: "binding"))
let holder: Holder[Str] = Holder(items: [])
let {items: retained} = holder
let second = retained.push(Marker(name: "destructured"))
let third = initial().push(Marker(name: "return"))
let keyed: Map[Int, Marker[Int]] = {}
let fourth = keyed.set(3, Marker(name: "map"))
let fifth = declared.extend([Marker(name: "nested")])
print ${first[0].name}
print ${second[0].name}
print ${third[0].name}
print ${fourth[3].name}
print ${fifth[0].name}
""",
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """binding
destructured
return
map
nested
"""
  let rejected = test.run_script(
    ctx,
    """type Marker[T] = {name: Str}
let values = [{name: "plain"}]
let guessed = values.push(Marker(name: "unobservable"))
""",
  )?
  assert ! rejected.success, rejected.stderr
  assert "check.constructor-inference" in rejected.stderr
}

test test_generic_constructor_infers_exact_instances_without_later_use_evidence { |ctx|
  let declaration = r"""enum ObservationState { Observed, Absent }
type Observation[T] = {state: ObservationState, value: T?}
"""
  let accepted = test.run_script(
    ctx,
    declaration + r"""let measured = Observation(state: Observed, value: 12)
let exact: Observation[Int] = measured
let value: Int? = measured.value
print ${(value ?? 0) + (exact.value ?? 0)} ${measured.state == Observed}
""",
  )?
  assert accepted.success, accepted.stderr
  assert accepted.stdout == """24 true
"""
  for source in [
    """let measured = Observation(state: Observed, value: 12)
let wrong: Observation[Str] = measured
""",
    """let measured = Observation(state: Observed, value: 12)
let wrong: Str? = measured.value
""",
  ] {
    let rejected = test.run_script(ctx, declaration + source)?
    assert ! rejected.success, source
    assert "check.type-mismatch" in rejected.stderr, rejected.stderr
  }

  for source in [
    """let absent = Observation(state: Absent, value: null)
""",
    """let absent = Observation(state: Absent, value: null)
let later: Int = absent.value ?? 0
""",
  ] {
    let rejected = test.run_script(ctx, declaration + source)?
    assert ! rejected.success, source
    assert "check.constructor-inference" in rejected.stderr, rejected.stderr
  }
}
