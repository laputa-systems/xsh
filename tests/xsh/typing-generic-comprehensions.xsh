proc typing_generic_comprehension_modules(ctx: TestContext) [fs, error] -> Result[Path] {
  let modules = test.temp_dir(ctx, name: "typing-generic-comprehensions")?
  fp"{modules}/comprehensions.xsh".write(
    r"""##! Generic comprehensions.

## Duplicates a value.
export pure duplicate(value) {
  [item for item in [value, value]]
}

## Maps two string keys to a value.
export pure by_key(value) {
  {key: value for key in ["z", "a"]}
}
""",
  )?
  fp"{modules}/integer_client.xsh".write(
    r"""##! Integer client of inferred comprehension helpers.

use comprehensions as helpers

## Sums values from both exported comprehensions.
export pure total() -> Int {
  let copies = helpers.duplicate(7)
  let indexed = helpers.by_key(7)
  copies[0] + copies[1] + indexed["a"] + indexed["z"]
}
""",
  )?
  fp"{modules}/text_client.xsh".write(
    r"""##! Text client of inferred comprehension helpers.

use comprehensions as helpers

## Concatenates values from both exported comprehensions.
export pure total() -> Str {
  let copies = helpers.duplicate("row")
  let indexed = helpers.by_key("row")
  copies[0] + copies[1] + indexed["a"] + indexed["z"]
}
""",
  )?
  modules
}

test test_typing_generic_comprehension_import_order_keeps_list_and_map_values { |ctx|
  let modules = typing_generic_comprehension_modules(ctx)?
  for source in [
    r"""use integer_client as numbers
use text_client as words
print numbers.total()
print words.total()
""",
    r"""use text_client as words
use integer_client as numbers
print numbers.total()
print words.total()
""",
  ] {
    let output = test.run_xsh(ctx, source, env: {XSH_MODULE_PATH: modules})?
    assert output.success, output.stderr
    assert output.stderr == ""
    assert output.stdout == "28\nrowrowrowrow\n"
  }
}

test test_typing_generic_comprehension_qualifiers_keep_order_scope_and_laziness { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc source() [io] -> List[Str] {
  print "source"
  ["b", "a"]
}
proc keep(include: Bool) [io] -> Bool {
  print "filter"
  include
}
proc expand(value: Str, include: Bool) [io, error] -> List[Str] {
  print "expand"
  if include { return [value + "-1", value + "-2"] }
  let _ = [value][9]
  [value]
}
proc key(value: Str) [io] -> Str {
  print "key"
  value
}
proc mark_value() [io] -> Bool {
  print "value"
  true
}
proc project(value, include: Bool) [io, error] {
  let values = {
    [key(item)]: if mark_value() { {payload: value, ordinal: item} } else { {payload: value, ordinal: item} }
    for item in source()
    if keep(include)
    for item in expand(item, include)
  }
  values
}

let numbers = project(7, true)?
let words = project("row", true)?
let skipped = project(false, false)?
assert numbers.keys() == ["a-1", "a-2", "b-1", "b-2"]
assert numbers.values()[0].ordinal == "a-1"
assert numbers.values()[1].ordinal == "a-2"
assert numbers.values()[2].ordinal == "b-1"
assert numbers.values()[3].ordinal == "b-2"
assert numbers.values()[0].payload == 7
assert numbers.values()[3].payload == 7
assert words.values()[0].ordinal == "a-1"
assert words.values()[3].ordinal == "b-2"
assert words.values()[0].payload == "row"
assert words.values()[3].payload == "row"
assert skipped.len() == 0
""",
  )?
  assert output.success, output.stderr
  assert output.stderr == ""
  assert output.stdout == """source
filter
expand
key
value
key
value
filter
expand
key
value
key
value
source
filter
expand
key
value
key
value
filter
expand
key
value
key
value
source
filter
filter
"""
}

test test_typing_generic_comprehension_preserves_only_unfiltered_nonempty_mapping { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure copy_nonempty(items: NonEmpty[Int]) {
  [item for item in items]
}
pure filter_nonempty(items: NonEmpty[Int]) {
  [item for item in items if item > 0]
}
pure nested_nonempty(items: NonEmpty[Int]) {
  [item for item in items for item in {if item > 0 {[item]} else {[]}}]
}

let copied: NonEmpty[Int] = copy_nonempty([4, 5])
let filtered: List[Int] = filter_nonempty([-1])
let nested: List[Int] = nested_nonempty([-1])
assert copied.first() == 4
assert copied.last() == 5
assert filtered == []
assert nested == []
print "nonempty mapping and empty filtered outputs preserved"
""",
  )?
  assert output.success, output.stderr
  assert output.stderr == ""
  assert output.stdout == "nonempty mapping and empty filtered outputs preserved\n"

  test.expect(
    ctx,
    r"""pure wrong_scope(value) {
  [item for outer in [value]]
}
""",
    status: 2,
    stderr: ["[check.unresolved-name]"],
  )?
  test.expect(
    ctx,
    r"""pure wrong_domain(value) {
  [item for item in 42]
}
""",
    status: 2,
    stderr: ["[check.listcomp-iterator]"],
  )?
}

test test_typing_generic_comprehension_revalidates_unsigned_keys_at_require { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure typed_keys(keys: List[UInt]) -> List[UInt] {
  keys
}
pure by_keys(value, keys: List[UInt]) {
  {key: value for key in typed_keys(keys)}
}

let signed_positive: Any = [2, 1]
let positive_keys: List[UInt] = signed_positive.require(List[UInt])?
let unsigned: Map[UInt, Str] = by_keys("kept", positive_keys)
let unsigned_keys: List[UInt] = unsigned.keys()
let expected_keys: List[UInt] = [1, 2]
assert unsigned_keys == expected_keys
assert unsigned.values() == ["kept", "kept"]
let signed_negative: Any = [-1]
assert signed_negative.require(List[UInt]) is Err(_)
print "unsigned key map preserved"
""",
  )?
  assert output.success, output.stderr
  assert output.stderr == ""
  assert output.stdout == "unsigned key map preserved\n"
}

test test_typing_generic_comprehension_stream_errors_and_early_cleanup_are_observable { |ctx|
  let declarations = r"""error SourceFailure = Missing(message: Str) : NotFound
var created = 0
var pulls = 0
var closed = 0

proc close_stream() [io] {
  closed += 1
  print "stream closed"
}

stream items() [io, error] -> Stream[Int] {
  defer close_stream()
  pulls += 1
  yield 1
  pulls += 1
  print "second pull"
  yield 2
}

proc source(fail: Bool) [io, error] -> Result[Stream[Int], SourceFailure] {
  created += 1
  if fail {
    Err(.Missing(message: "source failed"))
  } else {
    Ok(items())
  }
}

proc copy(value, source_fails: Bool, projection_fails: Bool) [io, error] {
  [
    {
      let ignored = [value][if projection_fails { 9 } else { 0 }]
      value
    }
    for item in source(source_fails)
  ]
}
"""

  let source_failure = test.run_xsh(
    ctx,
    declarations + r"""let failure = copy(7, true, false)
match failure {
  Err(problem) => assert problem.message == "source failed"
  Ok(_) => assert false, "source failure was lost"
}
assert [created, pulls, closed] == [1, 0, 0]
print "source failure preserved"
""",
  )?
  assert source_failure.success, source_failure.stderr
  assert source_failure.stdout == "source failure preserved\n"

  let projection_failure = test.expect(
    ctx,
    declarations + r"""let values = copy("row", false, true)
""",
    status: 3,
    stderr: ["index-out-of-range"],
  )?
  assert projection_failure.stdout == "stream closed\n"
}
