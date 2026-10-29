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
let missing: Result[Int, BodyChoiceError] = Err(BodyChoiceError.Missing())
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
print ${selected.name} ${selected.selected ?? -1} ${json.encode(selected.slice)?}
print ${missing.name} ${missing.selected ?? -1} ${missing.slice.len()}
print ${word.name} ${word.selected ?? "missing"} ${json.encode(word.slice)?}
""",
    status: 0,
    stdout: ["numbers 4 [4,6]\nmissing -1 0\nwords b [\"b\"]\n"],
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
    stdout: ["second\nfirst\ndefault\nsecond\nfirst\ndefault\n11 15 7\ntag 6 6\n"],
  )?
  assert output.stderr == "", output.stderr
}

test test_full_inference_body_ground_defaults_spreads_splices_and_source_order { |ctx|
  let output = test.expect(
    ctx,
    r"""type GroundForwardRow = {metadata: Str, first: Int, second: Int, rest: List[Int]}
type GroundForwardResult = {metadata: Str, spread: Int, defaulted: Int}
proc note(label: Str, value: Int) [io] -> Int {
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
proc forward(row: GroundForwardRow) [io] -> Result[GroundForwardResult] {
  let {metadata: metadata, first: first, second: second, rest: rest} = row
  let options = {second: note("second", second), first: note("first", first)}
  let spread = score(...options, @rest)
  let defaulted = score(first: note("default", first))
  return Ok({metadata: metadata, spread: spread, defaulted: defaulted})
}
let number = forward({metadata: "number", first: 2, second: 3, rest: [4, 6]})?
let word = forward({metadata: "tag", first: 1, second: 2, rest: [3]})?
print ${number.metadata} ${number.spread} ${number.defaulted}
print ${word.metadata} ${word.spread} ${word.defaulted}
""",
    status: 0,
    stdout: ["second\nfirst\ndefault\nsecond\nfirst\ndefault\nnumber 15 7\ntag 6 6\n"],
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
proc recover(row) [error, time] {
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
print $attempts
""",
    status: 0,
    stdout: ["3\n"],
  )?
  assert output.stderr == "", output.stderr
}

test test_full_inference_body_retry_delay_requires_time_effect { |ctx|
  test.expect(
    ctx,
    r"""error RetryBodyError = Temporary(attempt: Int)
type RetryRow = {value: Int, token: Int}
type RetryOutcome = {outcome: Result[Int, RetryBodyError], token: Int}
var attempts = 0
proc attempt(value: Int) [error] -> Result[Int, RetryBodyError] {
  attempts += 1
  if attempts < 2 {
    return Err(RetryBodyError.Temporary(attempt: attempts))
  }
  return Ok(value)
}
proc recover(row: RetryRow) [error] -> Result[RetryOutcome] {
  let outcome = try {
    retry [0ms] on (RetryBodyError.Temporary) {
      attempt(row.value)?
    }?
  }
  {outcome: outcome, token: row.token}
}
let number = recover({value: 7, token: 42})?
""",
    status: 2,
    stderr: ["[check.effect-violation]", "`retry` with delays requires the `time` effect"],
  )?
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
print $number.length
print $word.length
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
    r"""proc fail_stream() [] -> Result[Unit, Str] {
  return Err("stream failed")
}
stream child(value, fail_after) [io, error] {
  defer { print "child closed" }
  yield value
  if fail_after {
    fail_stream()?
  }
}
stream parent(value, fail_after) [io, error] {
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

test test_full_inference_body_ground_nested_stream_failure_runs_each_defer { |ctx|
  let output = test.expect(
    ctx,
    r"""proc fail_stream() [] -> Result[Unit, Str] {
  return Err("stream failed")
}
stream child(value: Str, fail_after: Bool) [io, error] -> Stream[Str] {
  defer { print "child closed" }
  yield value
  if fail_after {
    fail_stream()?
  }
}
stream parent(value: Str, fail_after: Bool) [io, error] -> Stream[Str] {
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

test test_full_inference_index_receiver_domains_retain_literal_modes { |ctx|
  let output = test.expect(
    ctx,
    r"""pure select(values, key) { values[key] }
pure last(values) { values[-1] }
pure zeroth(values) { values[-0] }
let keyed: Map[Int, Str] = {[0]: "zero", [1]: "one", [-1]: "negative"}
assert select([3, 4], 1) == 4
assert select(keyed, 1) == "one"
assert last([3, 4]) == 4
assert last(keyed) == "negative"
assert zeroth([3, 4]) == 3
assert zeroth(keyed) == "zero"
print "index modes"
""",
    status: 0,
    stdout: ["index modes\n"],
  )?
  assert output.stderr == "", output.stderr
}

test test_full_inference_slice_receiver_domains_preserve_scalar_units { |ctx|
  let output = test.expect(
    ctx,
    r"""pure clip(values, start, end) { values[start..end] }
assert clip([1, 2, 3], 1, 3) == [2, 3]
assert clip("aé🦀", 1, 3) == "é🦀"
assert clip(b"abcd", -3, -1) == b"bc"
print "slice domains"
""",
    status: 0,
    stdout: ["slice domains\n"],
  )?
  assert output.stderr == "", output.stderr
}

test test_full_inference_collect_yields_a_generic_slice_or_operator_result { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure tails(items) {
  collect { for at in range(items.len()) { yield items[at..] } }
}
pure sums(left, right) {
  collect { for at in range(left.len()) { yield left[at] + right[at] } }
}
let lists = tails([1, 2, 3])
let texts = tails(b"abc")
let integers = sums([1, 2], [3, 4])
let floats = sums([1.5], [2.5])
let second_sum = integers[1]
let first_float = floats[0]
print $lists.len()
print $texts.len()
print $second_sum
print $first_float
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "3\n3\n6\n4\n"
}

test test_full_inference_pattern_condition_binds_an_optional_generic_subject { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure present_or(value, fallback) {
  if let present = value { present } else { fallback }
}
pure current_or(reader, fallback) {
  if let current = reader.current { current } else { fallback }
}
let missing: Int? = null
let known: Int? = 3
let name: Str? = "ada"
let integers = [present_or(missing, 5), present_or(known, 5)]
let text = present_or(name, "none")
let from_field = current_or({current: name}, "none")
let first = integers[0]
let second = integers[1]
print $first
print $second
print $text
print $from_field
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "5\n3\nada\nada\n"
}

test test_full_inference_pattern_condition_keeps_the_payload_and_fallback_one_type { |ctx|
  test.expect(
    ctx,
    r"""pure present_or(value, fallback) {
  if let present = value { present } else { fallback }
}
let name: Str? = "ada"
let rejected = present_or(name, 1)
""",
    status: 2,
    stderr: ["[check."],
  )?
}

test test_full_inference_pattern_capture_in_an_initializer_is_a_generic_argument { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure ident(value) { value }
pure labelled(selected: Bytes) -> Str {
  let text = if let Ok(value) = selected.utf8() { ident(value) } else { "none" }
  text
}
let good = labelled(b"abc")
let bad = labelled(b"\xff")
print $good
print $bad
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "abc\nnone\n"
}

test test_full_inference_enum_annotated_local_reads_a_generic_row_field { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""enum Platform { Linux, Mac }
pure is_mac(value) {
  let platform: Platform = value.platform
  platform == Mac
}
let on_mac = is_mac({platform: Mac})
let on_linux = is_mac({platform: Linux})
print $on_mac
print $on_linux
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "true\nfalse\n"
}

test test_full_inference_null_test_narrows_an_optional_read_in_a_generic_body { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure clip(name: Str, cut: Int?, marker) {
  if cut != null { name.byte_slice(0, length: cut) } else { name }
}
pure width(cut: Int?, marker) {
  if cut == null { 0 } else { cut + 1 }
}
let clipped = clip("abcdef", 2, "m")
let whole = clip("abcdef", null, 7)
let some = width(4, "m")
let none = width(null, "m")
print $clipped
print $whole
print $some
print $none
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "ab\nabcdef\n5\n0\n"
}

test test_full_inference_emptiness_family_is_a_generic_condition { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure label(items) { if items.is_empty() { "none" } else { "some" } }
pure first_or(opts, fallback) { if opts.delimiter.is_empty() { fallback } else { opts.delimiter[-1] } }
let empty: List[Int] = []
let none = label(empty)
let one = label([1])
let last = first_or({delimiter: ["a", "b"]}, "z")
let empty_names: List[Str] = []
let fallback = first_or({delimiter: empty_names}, "z")
print $none
print $one
print $last
print $fallback
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "none\nsome\nb\nz\n"
}

test test_full_inference_optional_method_result_in_a_producer_checked_generic_body { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc digits(raw) [error] -> Result[Int] {
  let total = raw.len()
  var out = []
  var at = 0

  while at < total {
    let next = raw.byte_at(at) ?? 0

    if next == 48 or (next >= 49 and next <= 55) {
      out += [next - 48]
    }

    at += 1
  }

  Ok(bytes.from_ints(out)?.len())
}
proc main() [error, io] {
  let counted = digits(b"0a7")?
  print $counted
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "2\n"
}

test test_full_inference_guard_let_over_an_unresolved_subject_binds_the_present_value { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""type Options = {archive: Str?}
pure label(opts) {
  guard let name = opts.archive else { return "none" }
  name
}
let present: Options = {archive: "a.tar"}
let absent: Options = {archive: null}
let a = label(present)
let b = label(absent)
print $a
print $b
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "a.tar\nnone\n"
}

test test_full_inference_module_overload_with_one_arity_binds_an_unresolved_argument { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc probed(pid) [process, error] -> Int {
  if let Ok(threads) = process.threads(pid) {
    return 1
  }
  1
}
proc main() [process, error] {
  let seen = probed(1)
  print $seen
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "1\n"
}

test test_full_inference_module_overload_still_rejects_an_argument_of_no_overload { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc visible(pid) [process, error] -> Bool {
  if let Ok(threads) = process.threads(pid) {
    return true
  }
  false
}
proc main() [process, error] {
  print $visible("x")
}
""",
  )?
  assert output.success == false
  assert "err[check." in output.stderr, output.stderr
}

test test_full_inference_single_receiver_method_is_a_retained_requirement { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure has_special(mode) { mode.bit_and(0o4000) != 0 }
pure ratio(count, span) { count.float() / span.float() }
pure widened(mode, extra) { mode.bit_or(extra).bit_and(0o7777) }
let a = has_special(0o4755)
let b = has_special(0o755)
let c = ratio(1, 4)
let d = widened(0o700, 0o55)
print $a
print $b
print $c
print $d
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "true\nfalse\n0.25\n493\n"
}

test test_full_inference_single_receiver_method_rejects_another_receiver { |ctx|
  let cases = [
    {name: "text receiver", source: "pure has_special(mode) { mode.bit_and(0o4000) != 0 }\nlet value = has_special(\"x\")\n"},
    {name: "list receiver", source: "pure ratio(count) { count.float() }\nlet value = ratio([1])\n"},
  ]
  for {name, source} in cases {
    let output = test.run_xsh(ctx, source)?
    assert output.success == false, name
    assert "err[check." in output.stderr, f"{name}: {output.stderr}"
  }
}

test test_full_inference_local_stays_narrowed_after_an_early_exit { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""type Row = {n: Int?}
pure after_return(row) {
  let at = row.n
  return -1 when at == null
  at
}
pure after_block(row) {
  let at = row.n
  if at == null { return -1 }
  at + 1
}
pure after_guard(row) {
  let at = row.n
  guard at != null else { return -1 }
  at * 2
}
pure after_found(text) {
  let at = text.find("=")
  return -1 when at == null
  at
}
let present: Row = {n: 4}
let absent: Row = {n: null}
let a = after_return(present)
let b = after_return(absent)
let c = after_block(present)
let d = after_block(absent)
let e = after_found("ab=c")
let f = after_found("abc")
let g = after_guard(present)
let h = after_guard(absent)
print $a
print $b
print $c
print $d
print $e
print $f
print $g
print $h
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "4\n-1\n5\n-1\n2\n-1\n8\n-1\n"
}

test test_full_inference_fallback_after_an_early_exit_reads_the_narrowed_value { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""type Row = {n: Int?}
pure width(row) -> Int? {
  let at = row.n
  return null when at == null
  at ?? 0
}
let present: Row = {n: 4}
let absent: Row = {n: null}
let a = width(present) ?? -1
let b = width(absent) ?? -1
print $a
print $b
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "4\n-1\n"
}

test test_full_inference_value_pipeline_placeholder_is_a_generic_call_argument { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure add(left, right) { left + right }
pure scaled(value, factor) { value |> add(_, factor) |> add(_, 10) }
pure measured(items) { items |> count_of(_) }
pure count_of(values) { values.len() }
let a = scaled(1, 2)
let b = scaled(5, 5)
let c = measured([1, 2, 3])
print $a
print $b
print $c
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "13\n20\n3\n"
}

test test_full_inference_spawn_is_lowered_in_a_generic_body { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc launch(marker) [process, io, error] {
  let child = spawn run echo spawned ?
  let _ = wait child ?
  print $marker
}
proc launch_plan(marker) [process, io, error] {
  let plan = process.command_argv("echo", ["echo", "planned"], p".", {})
  let child = spawn plan?
  let _ = wait child ?
  print $marker
}
proc main() [process, io, error] {
  launch("one")?
  launch_plan("two")?
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "spawned\none\nplanned\ntwo\n"
}

test test_full_inference_type_namespaced_call_is_lowered_in_a_generic_body { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure to_path(value, marker) { Path.parse_bytes(value) }
pure joined(prefix, child) { Path.parse_bytes(bytes.concat([prefix, child])) }
let a = to_path(b"/etc/hosts", 1)?
let b = joined(b"/usr/", b"bin")?
print $a
print $b
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "/etc/hosts\n/usr/bin\n"
}

test test_full_inference_stream_collect_is_lowered_in_a_generic_body { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc entry_count(root, marker) [fs, error] {
  let listing = fs.children(root)?
  let entries = listing.collect()
  entries.len()
}
proc main() [fs, io, error] {
  let counted = entry_count(p".", 1)?
  let plausible = counted >= 0
  print $plausible
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "true\n"
}

test test_full_inference_fallback_handler_block_is_lowered_in_a_generic_body { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc quiet_names(root, marker) [fs, error] {
  let names = fs.xattr_list(root) ?? { |failure| [] }
  let _ = marker
  names.len()
}
proc strict_names(root, marker) [fs, error] {
  let names = fs.xattr_list(root) ?? { |failure|
    if failure.errno == 2 { [] } else { return Err(failure) }
  }
  let _ = marker
  names.len()
}
proc main() [fs, io, error] {
  let missing = p"/nonexistent-xsh-fallback-handler"
  let a = quiet_names(missing, 1)?
  let b = strict_names(missing, 1)?
  print $a
  print $b
  match strict_names(p"/dev/null/below-a-file", 1) {
    Ok(_) => print "completed"
    Err(_) => print "returned the failure"
  }
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "0\n0\nreturned the failure\n"
}

test test_full_inference_inferred_variant_with_payload_is_lowered_in_a_generic_body { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""enum Mode { Once(Int), Pair(Str, Int), Never }
type Plan = {mode: Mode, tag: Str}
pure delayed(tag, delay) {
  let mode: Mode = .Once(delay * 1000)
  let plan: Plan = {mode: mode, tag: tag}
  plan
}
pure fixed(tag) {
  let plan: Plan = {mode: .Pair("fixed", 7), tag: tag}
  plan
}
pure describe(plan: Plan) -> Str {
  match plan.mode {
    Once(after) => f"once {after}"
    Pair(label, count) => f"pair {label} {count}"
    Never => "never"
  }
}
print $describe(delayed("a", 2))
print $describe(fixed("b"))
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "once 2000\npair fixed 7\n"
}

test test_full_inference_regex_literal_is_a_generic_call_argument { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure keep(value, marker: Int) -> Int { marker }
let literal = keep(rx"[0-9]+", 1)
let stored = rx"[a-z]+"
let named = keep(stored, 2)
print $literal
print $named
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "1\n2\n"
}

test test_full_inference_byte_access_is_a_generic_method_over_text_and_bytes { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure byte_or_none(source, at) { source.byte_at(at) ?? -1 }
pure relay(source) { byte_or_none(source, 1) }
let text = byte_or_none("abc", 1)
let data = byte_or_none(b"abc", 2)
let past = byte_or_none("abc", 9)
let relayed_text = relay("xyz")
let relayed_data = relay(b"xyz")
print $text
print $data
print $past
print $relayed_text
print $relayed_data
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "98\n99\n-1\n121\n121\n"
}

test test_full_inference_replace_is_a_generic_method_over_text_and_regex { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure swap(subject, pattern, replacement) { subject.replace(pattern, with: replacement) }
pure dashes(subject) { swap(subject, subject, "+") }
let text = swap("a-b-c", "-", "+")
let pattern_result = swap(rx"[-]", "a-b-c", "+")
let relayed = dashes("same")
print $text
print $pattern_result
print $relayed
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "a+b+c\na+b+c\n+\n"
}

test test_full_inference_result_follows_the_selected_overload_member { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure blank(subject) { subject.trim().is_empty() }
pure position(subject, needle) { subject.find(needle) ?? -1 }
pure entry(table, key) { table.get(key) ?? "none" }
pure names(table) { table.keys().len() }
pure grown(items, extra) { items.push(extra).len() }
let a = blank("   ")
let b = blank(b" x ")
let c = position("hello", "ll")
let strings: Map[Str, Str] = {x: "1"}
let counts: Map[Str, Int] = {x: 1, y: 2}
let f = entry(strings, "x")
let g = entry(strings, "y")
let h = names(counts)
let i = grown([1, 2], 3)
print $a
print $b
print $c
print $f
print $g
print $h
print $i
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "true\nfalse\n2\n1\nnone\n2\n3\n"
}

test test_full_inference_overloaded_method_without_a_unique_signature_is_rejected { |ctx|
  let cases = [
    {name: "receiver without the method", source: "pure byte_or_none(source, at) { source.byte_at(at) ?? -1 }\nlet value = byte_or_none([1, 2], 0)\n"},
    {name: "argument of the wrong type", source: "pure byte_or_none(source, at) { source.byte_at(at) ?? -1 }\nlet value = byte_or_none(\"abc\", \"x\")\n"},
    {name: "dynamic record receiver", source: "pure entry(table, key) { table.get(key) ?? \"none\" }\nlet value = entry({x: \"1\"}, \"x\")\n"},
    {name: "member result outside the body contract", source: "pure position(subject, needle) { subject.find(needle) ?? -1 }\nlet value = position(rx\"a\", \"b\")\n"},
    {name: "replacement pattern of the other receiver", source: "pure swap(subject, pattern) { subject.replace(pattern, with: \"+\") }\nlet value = swap(\"abc\", rx\"b\")\n"},
  ]
  for {name, source} in cases {
    let output = test.run_xsh(ctx, source)?
    assert output.success == false, name
    assert "err[check." in output.stderr, f"{name}: {output.stderr}"
  }
}

test test_full_inference_slice_of_a_typed_value_is_a_generic_argument { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure width(input, marker) { input.len() }
pure head(value: Bytes, start: Int) -> Int { width(value[..start], 1) }
pure words(items: List[Str], count: Int) -> Int { width(items[..count], "m") }
let bytes_width = head(b"abcdef", 3)
let list_width = words(["a", "b", "c"], 2)
print $bytes_width
print $list_width
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "3\n2\n"
}

test test_full_inference_comparison_is_a_generic_call_argument { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""type Spec = {conv: Str}
pure show(flag, tag: Int) -> Int { tag }
pure upper_case(spec: Spec) -> Int { show(spec.conv == "X", 1) }
pure bigger(left: Int, right: Int) -> Int { show(left > right and right != 0, 2) }
pure listed(items: List[Int], wanted: Int) -> Int { show(wanted in items, 3) }
let a = upper_case({conv: "X"})
let b = bigger(4, 2)
let c = listed([1, 2], 2)
print $a
print $b
print $c
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "1\n2\n3\n"
}

test test_full_inference_comprehension_variable_is_a_generic_call_argument { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure scale(marker, value: Int) -> Int { value * 2 }
pure doubled(words: List[Int], tag: Str) -> List[Int] { [scale(tag, word) for word in words] }
pure positive(words: List[Int], tag: Str) -> List[Int] { [scale(tag, word) for word in words if word > 0] }
pure named(words: List[Int], tag: Str) -> Map[Int, Int] { {word: scale(tag, word) for word in words} }
let a = doubled([1, 2], "t")
let b = positive([-1, 3], "t")
let c = named([4], "t")
let first = a[1]
let only = b[0]
let mapped = c[4]
print $first
print $only
print $mapped
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "4\n6\n8\n"
}

test test_full_inference_ordered_generic_index_reads_in_a_condition { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure compare(left, right: List[Int]) -> Int {
  return -1 when left[0] < right[0]
  var at = left.len() - 1
  while at >= 0 {
    if left[at] > right[at] { return 1 }
    at -= 1
  }
  0
}
let smaller = compare([1, 2], [2, 2])
let larger = compare([3, 5], [3, 4])
let equal = compare([1, 2], [1, 2])
print $smaller
print $larger
print $equal
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "-1\n1\n0\n"
}

test test_full_inference_result_context_is_lowered_in_a_generic_body { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure parse_labeled(label, raw: Str) -> Result[Int] {
  raw.parse_int().context("usage", f"unsupported {label} '{raw}'")
}
let good = parse_labeled("count", "5")?
print $good
let bad = parse_labeled("count", "five")
match bad {
  Ok(_) => print "unexpected"
  Err(failure) => print $failure.message
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout.starts_with("5\n")
  assert "unsupported count 'five'" in output.stdout
}

test test_full_inference_command_argv_words_stay_generic_in_a_literal_list { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc echo_word(word) [process, error] -> Result[Int] {
  let status = process.run(process.command_argv("echo", ["echo", "word:", word], p".", {}))?
  status.shell_code()
}
proc main() [process, error, io] {
  let _ = echo_word("text")?
  let _ = echo_word(p"/tmp")?
}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "word: text\nword: /tmp\n"
}
