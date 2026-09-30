type InferredBox[T] = {value: T, items: List[T] = []}
type InferredObservation[T] = {value: T? = null, samples: List[T] = []}

test test_generic_constructor_fields_establish_concrete_instances [error] { |ctx|
  let executed = test.run_script(ctx, r"""type Box[T] = {value: T, items: List[T] = []}
type Observation[T] = {value: T? = null, samples: List[T] = []}
let count = Box(value: 12)
let text = Box(value: "demo")
let observed = Observation(value: 7)
let samples = Observation(samples: [3, 4])
print ${count.value + (observed.value ?? 0)}
print ${text.value.upper()}
print ${samples.samples[0] + samples.samples[1]}
print ${count.items.len()}
""")?
  test.ok(executed.success, executed.stderr)?
  test.eq(executed.stdout, "19\nDEMO\n7\n0\n")?
}

test test_generic_constructor_context_anchors_null_empty_and_unused_parameters [error] { |ctx|
  let executed = test.run_script(ctx, r"""type Observation[T] = {value: T? = null, samples: List[T] = []}
type Marker[T] = {name: Str}
let absent: Observation[Int] = Observation(value: null)
let empty: Observation[Str] = Observation(samples: [])
let marker: Marker[Int] = Marker(name: "anchored")
print ${absent.value ?? 8}
print ${empty.samples.len()}
print $marker.name
""")?
  test.ok(executed.success, executed.stderr)?
  test.eq(executed.stdout, "8\n0\nanchored\n")?
}

test test_generic_constructor_unresolved_and_conflicting_fields_are_rejected [error] { |ctx|
  for source in [
    "type Box[T] = {value: T?}\nlet missing = Box(value: null)\n",
    "type Box[T] = {values: List[T]}\nlet empty = Box(values: [])\n",
    "type Box[T] = {left: T, right: T}\nlet conflict = Box(left: 1, right: \"wrong\")\n",
    "type Box[T] = {left: T, right: T}\nlet conflict = Box(left: 1, right: 2.0)\n",
    "type Marker[T] = {name: Str}\nlet missing = Marker(name: \"unused\")\n",
  ] {
    let rejected = test.run_script(ctx, source)?
    test.ok(!rejected.success, rejected.stderr)?
    test.contains(rejected.stderr, "check.")?
  }
}

test test_generic_constructor_nested_fields_share_only_occurrence_constraints [error] { |ctx|
  let executed = test.run_script(ctx, r"""type Inner[T] = {value: T?}
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
""")?
  test.ok(executed.success, executed.stderr)?
  test.eq(executed.stdout, "7\ninner\nkept\nlist\n")?
}

test test_generic_constructor_uses_annotated_function_slots_and_returns [error] { |ctx|
  let executed = test.run_script(ctx, r"""type Marker[T] = {name: Str}
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
""")?
  test.ok(executed.success, executed.stderr)?
  test.eq(executed.stdout, "slot\n7\nreturn\nresult\n")?
}

test test_generic_constructor_preserves_named_spreads_puns_and_field_order [error] { |ctx|
  let executed = test.run_script(ctx, r"""type Pair[T] = {left: T, right: T, items: List[T] = []}
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
""")?
  test.ok(executed.success, executed.stderr)?
  test.eq(executed.stdout, "15\n7\n12\n2\n")?
  let duplicate = test.run_script(ctx, "type Pair[T] = {left: T, right: T}\nlet wrong = Pair(...{left: 1}, left: 2, right: 3)\n")?
  test.ok(!duplicate.success, duplicate.stderr)?
  test.contains(duplicate.stderr, "check.")?
}

test test_generic_constructor_constants_use_the_same_field_and_context_constraints [error] { |ctx|
  let executed = test.run_script(ctx, r"""type Inner[T] = {value: T?}
type Outer[T] = {inner: Inner[T], anchor: T, items: List[T] = []}
type Marker[T] = {name: Str}
const inferred = Outer(inner: Inner(value: null), anchor: 7)
const marked: Marker[Int] = Marker(name: "constant")
const spread = Outer(...{inner: {value: "text"}, anchor: "anchor"})
print ${inferred.inner.value ?? inferred.anchor}
print $marked.name
print ${spread.inner.value ?? "missing"}
print ${inferred.items.len()}
""")?
  test.ok(executed.success, executed.stderr)?
  test.eq(executed.stdout, "7\nconstant\ntext\n0\n")?
}

test test_generic_constructor_retains_both_typed_map_parameters [error] { |ctx|
  let executed = test.run_script(ctx, r"""type Table[K, V] = {entries: Map[K, V]}
let numbers = Table(entries: {[1]: "one", [2]: "two"})
let strings = Table(entries: {one: 1, two: 2})
let nested: Table[Int, Str] = Table(entries: {})
const prepared = Table(entries: {[3]: "three"})
print ${numbers.entries[1].upper()}
print ${strings.entries["one"] + strings.entries["two"]}
print ${nested.entries.len()}
print ${prepared.entries[3]}
""")?
  test.ok(executed.success, executed.stderr)?
  test.eq(executed.stdout, "ONE\n3\n0\nthree\n")?
}
