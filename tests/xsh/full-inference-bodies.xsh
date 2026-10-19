test test_full_inference_body_patterns_guards_and_while { |ctx|
  let output = test.expect(
    ctx,
    r"""error BodyChoiceError = Missing
pure choose(outcome, fallback) {
  var selected = fallback
  match outcome {
    Ok(value) if value != fallback => selected = value
    Ok(_) => selected = fallback
    Err(_) => selected = fallback
  }
  selected
}
pure count(items) {
  var index = 0
  while index < items.len() {
    index += 1
  }
  index
}
let number = choose(Ok(9), 4)
let word = choose(Ok("ready"), "waiting")
let missing: Result[Int, BodyChoiceError] = Err(BodyChoiceError.Missing)
let fallback = choose(missing, 6)
let numbers = count([1, 2, 3])
let words = count(["one", "two"])
print $number
print $word
print $fallback
print $numbers
print $words
""",
    status: 0,
    stdout: ["9\nready\n6\n3\n2\n"],
  )?
  assert output.stderr == "", output.stderr
}

test test_full_inference_body_record_map_set_updates_and_comprehensions { |ctx|
  let output = test.expect(
    ctx,
    r"""pure rebuild(row) {
  let {name: name, items: items} = row
  let copied = [item for item in items]
  var indexed = {[0]: copied[0]}
  indexed[1] = copied[1]
  let unique = {item for item in copied}
  let updated = {...row, name: name.upper(), items: copied.push(copied[0]), index: indexed, unique: unique}
  updated
}
let numbers = rebuild({name: "nums", items: [3, 1]})
let words = rebuild({name: "words", items: ["b", "a"]})
assert numbers.name == "NUMS"
assert numbers.items == [3, 1, 3]
assert numbers.index[1] == 1
assert numbers.unique == {1, 3}
assert words.name == "WORDS"
assert words.items == ["b", "a", "b"]
assert words.index[1] == "a"
assert words.unique == {"a", "b"}
print numbers.items.len()
print words.items.len()
""",
    status: 0,
    stdout: ["3\n3\n"],
  )?
  assert output.stderr == "", output.stderr
}

test test_full_inference_body_index_slice_and_optional_fields { |ctx|
  let output = test.expect(
    ctx,
    r"""type IntRow = {name: Str, items: List[Int]}
type StrRow = {name: Str, items: List[Str]}
pure inspect(maybe, start, end) {
  let name = maybe?.name ?? "missing"
  let items = maybe?.items ?? []
  let selected = if items.len() == 0 { null } else { items[start] }
  let slice = items[start..end]
  {name: name, selected: selected, slice: slice}
}
let present: IntRow? = {name: "numbers", items: [2, 4, 6]}
let absent: IntRow? = null
let text: StrRow? = {name: "words", items: ["a", "b"]}
let selected = inspect(present, 1, 3)
let missing = inspect(absent, 0, 0)
let word = inspect(text, 1, 2)
print ${selected.name} ${selected.selected ?? -1} ${selected.slice}
print ${missing.name} ${missing.selected ?? -1} ${missing.slice.len()}
print ${word.name} ${word.selected ?? "missing"} ${word.slice}
""",
    status: 0,
    stdout: ["numbers 4 [4, 6]\nmissing -1 0\nwords b [b]\n"],
  )?
  assert output.stderr == "", output.stderr
}

test test_full_inference_body_defaults_spreads_splices_and_source_order { |ctx|
  let output = test.expect(
    ctx,
    r"""proc note(label: Str, value: Int) [io] -> Int {
  print $label
  return value
}
pure score(first: Int, second: Int = 5, ...rest: List[Int]) -> Int {
  var total = first + second
  for value in rest {
    total += value
  }
  total
}
proc forward(row) [io] {
  let {metadata: metadata, first: first, second: second, rest: rest} = row
  let options = {second: note("second", second), first: note("first", first)}
  let spread = score(...options, @rest)
  let defaulted = score(first: note("default", first))
  {metadata: metadata, spread: spread, defaulted: defaulted}
}
let number = forward({metadata: 11, first: 2, second: 3, rest: [4, 6]})?
let word = forward({metadata: "tag", first: 1, second: 2, rest: [3]})?
print ${number.metadata} ${number.spread} ${number.defaulted}
print ${word.metadata} ${word.spread} ${word.defaulted}
""",
    status: 0,
    stdout: ["second\nfirst\ndefault\nsecond\nfirst\ndefault\n11 15 7\ntag 11 6\n"],
  )?
  assert output.stderr == "", output.stderr
}

test test_full_inference_body_try_retry_and_captured_error { |ctx|
  let output = test.expect(
    ctx,
    r"""error RetryBodyError = Temporary(attempt: Int)
var attempts = 0
proc attempt(value: Int) [error] -> Result[Int, RetryBodyError] {
  attempts += 1
  if attempts < 2 {
    return Err(RetryBodyError.Temporary(attempt: attempts))
  }
  return Ok(value)
}
proc recover(row) [error] {
  let outcome = try {
    retry [0ms] on (RetryBodyError.Temporary) {
      attempt(row.value)?
    }?
  }
  {outcome: outcome, token: row.token}
}
let number = recover({value: 7, token: 42})?
let word = recover({value: 8, token: "kept"})?
assert number.outcome == Ok(7)
assert number.token == 42
assert word.outcome == Ok(8)
assert word.token == "kept"
print attempts
""",
    status: 0,
    stdout: ["3\n"],
  )?
  assert output.stderr == "", output.stderr
}

test test_full_inference_body_resource_tempdir_and_cleanup { |ctx|
  let output = test.expect(
    ctx,
    r"""proc materialize(row) [fs, error] {
  tempdir directory {
    let file = fp"{directory}/payload"
    file.write(row.text)?
    let text = file.read_text()?
    {length: text.byte_len(), token: row.token, directory: directory}
  }
}
let number = materialize({text: "alpha", token: 17})?
let word = materialize({text: "xy", token: "kept"})?
assert number.length == 5
assert number.token == 17
assert word.length == 2
assert word.token == "kept"
assert ! number.directory.exists()?
assert ! word.directory.exists()?
print number.length
print word.length
""",
    status: 0,
    stdout: ["5\n2\n"],
  )?
  assert output.stderr == "", output.stderr
}

test test_full_inference_body_yield_delegation_closes_nested_streams { |ctx|
  let output = test.expect(
    ctx,
    r"""stream child(value, count) {
  defer { print "child closed" }
  repeat count times {
    yield value
  }
}
stream parent(value, count) {
  defer { print "parent closed" }
  yield @child(value, count)
}
let producer = parent("row", 2)
let numbers = parent(7, 1)
print "created"
for value in producer {
  print $value
  break
}
for value in numbers {
  print $value
  break
}
print "done"
""",
    status: 0,
    stdout: ["created\nrow\nchild closed\nparent closed\n7\nchild closed\nparent closed\ndone\n"],
  )?
  assert output.stderr == "", output.stderr
}

test test_full_inference_body_nested_stream_failure_runs_each_defer { |ctx|
  let output = test.expect(
    ctx,
    r"""stream child(value, fail_after) [error] {
  defer { print "child closed" }
  yield value
  if fail_after {
    fail "stream failed"
  }
}
stream parent(value, fail_after) [error] {
  defer { print "parent closed" }
  yield @child(value, fail_after)
}
let outcome = try {
  for value in parent("row", true) {
    print $value
  }
}
assert outcome is Err(_)
print "failure captured"
""",
    status: 0,
    stdout: ["row\nchild closed\nparent closed\nfailure captured\n"],
  )?
  assert output.stderr == "", output.stderr
}

test test_full_inference_body_rejects_non_indexable_instance { |ctx|
  let output = test.run_script(
    ctx,
    r"""pure first(values) { values[0] }
let invalid = first(42)
""",
  )?
  assert ! output.success, output.stdout
  assert "[check.index-type]" in output.stderr, output.stderr
}

test test_full_inference_body_rejects_incompatible_generic_branch { |ctx|
  let output = test.run_script(
    ctx,
    r"""pure identity(value) { value }
pure choose(flag: Bool, value: Int, token) -> Int {
  let preserved = identity(token)
  if flag { value } else { "different" }
}
let invalid = choose(true, 42, "tag")
""",
  )?
  assert ! output.success, output.stdout
  assert "[check.type-mismatch]" in output.stderr, output.stderr
}
