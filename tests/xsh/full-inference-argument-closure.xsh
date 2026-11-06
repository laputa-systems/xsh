test test_full_inference_generic_arguments_close_byte_at_and_optional_fallback { |ctx|
  let checks = r"""
assert blanks(b"a b\t") == [false, true, false, true, false]
assert first_blank("a b")
assert raw_byte(b"") == null
assert raw_byte(b"A") == 65
print "closed"
"""
  let inferred = test.run_xsh(ctx, r"""
pure is_blank(value) -> Bool { value == 32 or value == 9 }
pure blanks(data: Bytes) -> List[Bool] {
  [is_blank(data.byte_at(index) ?? 0) for index in range(data.len() + 1)]
}
pure first_blank(text: Str) -> Bool { is_blank(text.byte_at(1) ?? 0) }
pure same(value) { value }
pure raw_byte(data: Bytes) { same(data.byte_at(0)) }
""" + checks)?
  let annotated = test.run_xsh(ctx, r"""
pure is_blank(value: Int) -> Bool { value == 32 or value == 9 }
pure blanks(data: Bytes) -> List[Bool] {
  [is_blank(data.byte_at(index) ?? 0) for index in range(data.len() + 1)]
}
pure first_blank(text: Str) -> Bool { is_blank(text.byte_at(1) ?? 0) }
pure same(value: Int?) -> Int? { value }
pure raw_byte(data: Bytes) -> Int? { same(data.byte_at(0)) }
""" + checks)?
  assert annotated.success, annotated.stderr
  assert inferred.success, inferred.stderr
  assert inferred.stdout == annotated.stdout
  assert inferred.stdout == "closed\n"
}

test test_full_inference_record_arguments_close_mutated_locals_beside_empty_lists { |ctx|
  let checks = r"""
pure width(source: Int) -> Int {
  var total = source
  total = total + 1
  describe(source, {total: total, items: []})
}
assert width(4) == 5
print "closed"
"""
  let inferred = test.run_xsh(ctx, r"""
type Entry = {total: Int, items: List[Int]}
pure describe(source, entry: Entry) -> Int { entry.total + entry.items.len() }
""" + checks)?
  let annotated = test.run_xsh(ctx, r"""
type Entry = {total: Int, items: List[Int]}
pure describe(source: Int, entry: Entry) -> Int { entry.total + entry.items.len() }
""" + checks)?
  assert annotated.success, annotated.stderr
  assert inferred.success, inferred.stderr
  assert inferred.stdout == annotated.stdout
  assert inferred.stdout == "closed\n"
}

test test_full_inference_optional_local_assignment_from_generic_element { |ctx|
  let checks = r"""
assert pick(["a", "b"], 1) == "b"
assert pick(["a"], 3) == null
print "assigned"
"""
  let inferred = test.run_xsh(ctx, r"""
pure pick(items, index: Int) -> Str? {
  var found: Str? = null
  if index < items.len() { found = items[index] }
  found
}
""" + checks)?
  let annotated = test.run_xsh(ctx, r"""
pure pick(items: List[Str], index: Int) -> Str? {
  var found: Str? = null
  if index < items.len() { found = items[index] }
  found
}
""" + checks)?
  assert annotated.success, annotated.stderr
  assert inferred.success, inferred.stderr
  assert inferred.stdout == annotated.stdout
  assert inferred.stdout == "assigned\n"
}

test test_full_inference_optional_local_assignment_rejects_other_element_domains { |ctx|
  test.expect(ctx, r"""
pure pick(items, index: Int) -> Str? {
  var found: Str? = null
  if index < items.len() { found = items[index] }
  found
}
let rejected = pick([1, 2], 0)
""", status: 2, stderr: ["[check.callable-mismatch]"])?
}

test test_full_inference_generic_body_generalizes_past_a_returning_match_arm { |ctx|
  let checks = r"""
assert classify(1, "x") == "one"
assert classify(2, "x") == "other"
assert classify(7, "x") == "returned"
print "generalized"
"""
  let inferred = test.run_xsh(ctx, r"""
pure classify(code: Int, tag) -> Str {
  let name = match code {
    1 => "one"
    2 => "other"
    else => { return "returned" }
  }
  let _ = tag == "x"
  name
}
""" + checks)?
  let annotated = test.run_xsh(ctx, r"""
pure classify(code: Int, tag: Str) -> Str {
  let name = match code {
    1 => "one"
    2 => "other"
    else => { return "returned" }
  }
  let _ = tag == "x"
  name
}
""" + checks)?
  assert annotated.success, annotated.stderr
  assert inferred.success, inferred.stderr
  assert inferred.stdout == annotated.stdout
  assert inferred.stdout == "generalized\n"
}

test test_full_inference_forwarded_map_read_with_text_key_needs_no_uint_validation { |ctx|
  let checks = r"""
let present: Map[Str, Str] = {ec: "x"}
let absent: Map[Str, Str] = {zz: "x"}
assert lookup(present) == "x"
assert lookup(absent) == "none"
print "forwarded"
"""
  let inferred = test.run_xsh(ctx, r"""
pure read(colors, code) -> Str? {
  if let Ok(text) = colors.get(code) { text } else { null }
}
pure lookup(colors) -> Str { read(colors, "ec") ?? "none" }
""" + checks)?
  let annotated = test.run_xsh(ctx, r"""
pure read(colors: Map[Str, Str], code: Str) -> Str? {
  if let Ok(text) = colors.get(code) { text } else { null }
}
pure lookup(colors: Map[Str, Str]) -> Str { read(colors, "ec") ?? "none" }
""" + checks)?
  assert annotated.success, annotated.stderr
  assert inferred.success, inferred.stderr
  assert inferred.stdout == annotated.stdout
  assert inferred.stdout == "forwarded\n"
}

test test_full_inference_comprehension_condition_constrains_an_unresolved_field_to_bool { |ctx|
  let checks = r"""
assert count_quoted([{quoted: true}, {quoted: false}, {quoted: true}]) == 2
print "counted"
"""
  let inferred = test.run_xsh(ctx, r"""
pure count_quoted(files) -> Int { [file for file in files if file.quoted].len() }
""" + checks)?
  let annotated = test.run_xsh(ctx, r"""
type Entry = {quoted: Bool}
pure count_quoted(files: List[Entry]) -> Int { [file for file in files if file.quoted].len() }
""" + checks)?
  assert annotated.success, annotated.stderr
  assert inferred.success, inferred.stderr
  assert inferred.stdout == annotated.stdout
  assert inferred.stdout == "counted\n"
}

test test_full_inference_comprehension_condition_rejects_a_non_bool_field { |ctx|
  test.expect(ctx, r"""
pure count_quoted(files) -> Int { [file for file in files if file.quoted].len() }
let rejected = count_quoted([{quoted: 1}])
""", status: 2, stderr: ["[check.type-mismatch]"])?
}

test test_full_inference_empty_list_argument_without_an_anchor_names_the_value { |ctx|
  test.expect(ctx, r"""
pure selected(words) -> Bool { words[0] == "a" }
pure usage(key: Int) -> Bool { selected([]) }
""", status: 2, stderr: ["[check.local-inference]", "argument type needs an annotation", "the item type of this empty list"])?
}

test test_full_inference_empty_list_argument_is_accepted_when_the_callee_or_a_local_anchors_it { |ctx|
  let output = test.run_xsh(ctx, r"""
pure selected(words) -> Bool { words.is_empty() or words[0] == "a" }
pure typed(words: List[Str]) -> Bool { words.is_empty() }
pure annotated_local() -> Bool {
  let words: List[Str] = []
  selected(words)
}
pure anchored_callee() -> Bool { typed([]) and selected(["a"]) }
pure forwarded(words) -> Bool { selected(words) }
assert forwarded(["a"])
assert annotated_local()
assert anchored_callee()
print "anchored"
""")?
  assert output.success, output.stderr
  assert output.stdout == "anchored\n"
}

test test_full_inference_inline_record_argument_with_an_empty_list_matches_the_let_bound_form { |ctx|
  let checks = r"""
assert inline(3) == bound(3)
assert inline(3).elements.len() == 0
assert inline(3).name == "row"
print "seeded"
"""
  let inferred = test.run_xsh(ctx, r"""
type Element = {id: Int}
type Row = {name: Str, elements: List[Element]}
pure row_for(entry, name: Str) -> Row { {name: name, elements: entry.elements} }
pure inline(count: Int) -> Row { row_for({values: count, elements: []}, "row") }
pure bound(count: Int) -> Row {
  let entry = {values: count, elements: []}
  row_for(entry, "row")
}
""" + checks)?
  let annotated = test.run_xsh(ctx, r"""
type Element = {id: Int}
type Row = {name: Str, elements: List[Element]}
type Entry = {values: Int, elements: List[Element]}
pure row_for(entry: Entry, name: Str) -> Row { {name: name, elements: entry.elements} }
pure inline(count: Int) -> Row { row_for({values: count, elements: []}, "row") }
pure bound(count: Int) -> Row {
  let entry: Entry = {values: count, elements: []}
  row_for(entry, "row")
}
""" + checks)?
  assert annotated.success, annotated.stderr
  assert inferred.success, inferred.stderr
  assert inferred.stdout == annotated.stdout
  assert inferred.stdout == "seeded\n"
}

test test_full_inference_inline_record_argument_rejects_an_unanchored_empty_list { |ctx|
  test.expect(ctx, r"""
pure count(entry) -> Int { entry.elements.len() }
pure unanchored() -> Int { count({elements: []}) }
""", status: 2, stderr: ["[check.local-inference]", "local type needs an annotation"])?
}

test test_full_inference_guard_scope_keeps_repeated_reads_of_an_unrelated_parameter { |ctx|
  let checks = r"""
assert outer(1, {link: "a"}) == 2
assert outer(1, {link: null}) == 0
print "repeated"
"""
  let inferred = test.run_xsh(ctx, r"""
pure show(value) -> Int { 1 }
pure outer(context, entry) -> Int {
  if entry.link != null { show(context) + show(context) } else { 0 }
}
""" + checks)?
  let annotated = test.run_xsh(ctx, r"""
type Entry = {link: Str?}
pure show(value: Int) -> Int { 1 }
pure outer(context: Int, entry: Entry) -> Int {
  if entry.link != null { show(context) + show(context) } else { 0 }
}
""" + checks)?
  assert annotated.success, annotated.stderr
  assert inferred.success, inferred.stderr
  assert inferred.stdout == annotated.stdout
  assert inferred.stdout == "repeated\n"
}

test test_full_inference_annotated_record_with_empty_list_field_closes_generic_argument { |ctx|
  let checks = r"""
let job: Job = {name: "a", files: []}
assert size(job) == 0
let filled: Job = {name: "b", files: ["x", "y"]}
assert size(filled) == 2
print "closed"
"""
  let inferred = test.run_xsh(ctx, r"""
type Job = {name: Str, files: List[Str]}
proc size(job) -> Int { job.files.len() }
""" + checks)?
  let annotated = test.run_xsh(ctx, r"""
type Job = {name: Str, files: List[Str]}
proc size(job: Job) -> Int { job.files.len() }
""" + checks)?
  assert annotated.success, annotated.stderr
  assert inferred.success, inferred.stderr
  assert inferred.stdout == annotated.stdout
  assert inferred.stdout == "closed\n"
}

test test_full_inference_record_field_deferred_to_an_optional_is_closed_by_a_later_use { |ctx|
  let checks = r"""
let base: Options = {output: null, tries: 1}
assert apply(base, "o", "x").output == "x"
assert apply(base, "t", "xyz").tries == 3
assert apply(base, "t", "xyz").output == null
print "closed"
"""
  let inferred = test.run_xsh(ctx, r"""
type Options = {output: Str?, tries: Int}
pure number(text: Str) -> Int { text.byte_len() }
pure apply(options, key: Str, value) -> Options {
  match key {
    "o" => {...options, output: value}
    "t" => {...options, tries: number(value)}
    else => options
  }
}
""" + checks)?
  let annotated = test.run_xsh(ctx, r"""
type Options = {output: Str?, tries: Int}
pure number(text: Str) -> Int { text.byte_len() }
pure apply(options: Options, key: Str, value: Str) -> Options {
  match key {
    "o" => {...options, output: value}
    "t" => {...options, tries: number(value)}
    else => options
  }
}
""" + checks)?
  assert annotated.success, annotated.stderr
  assert inferred.success, inferred.stderr
  assert inferred.stdout == annotated.stdout
  assert inferred.stdout == "closed\n"
}

test test_full_inference_annotated_record_literal_defers_optional_field_to_a_later_use { |ctx|
  let checks = r"""
assert make("xyz").tries == 3
assert make("xyz").output == "xyz"
print "closed"
"""
  let inferred = test.run_xsh(ctx, r"""
type Options = {output: Str?, tries: Int}
pure number(text: Str) -> Int { text.byte_len() }
pure make(value) -> Options {
  let options: Options = {output: value, tries: 1}
  {...options, tries: number(value)}
}
""" + checks)?
  let annotated = test.run_xsh(ctx, r"""
type Options = {output: Str?, tries: Int}
pure number(text: Str) -> Int { text.byte_len() }
pure make(value: Str) -> Options {
  let options: Options = {output: value, tries: 1}
  {...options, tries: number(value)}
}
""" + checks)?
  assert annotated.success, annotated.stderr
  assert inferred.success, inferred.stderr
  assert inferred.stdout == annotated.stdout
  assert inferred.stdout == "closed\n"
}

test test_full_inference_open_spread_empty_list_field_closes_from_a_later_use_in_a_loop { |ctx|
  let checks = r"""
let start: Parsed = {code: [7], at: 0}
assert alt(start, 2).at == 3
assert alt(start, 2).code.len() == 0
assert alt({code: [], at: 5}, 0).at == 5
print "closed"
"""
  let inferred = test.run_xsh(ctx, r"""
type Parsed = {code: List[Int], at: Int}
pure branch(state, depth: Int) -> Parsed {
  var st = {...state, code: []}
  if depth > 0 { let inner = alt(st, depth - 1) }
  loop {
    break when st.at >= 3
    st = {...st, at: st.at + 1}
  }
  st
}
pure alt(state: Parsed, depth: Int) -> Parsed { branch(state, depth) }
""" + checks)?
  let annotated = test.run_xsh(ctx, r"""
type Parsed = {code: List[Int], at: Int}
pure branch(state: Parsed, depth: Int) -> Parsed {
  var st = {...state, code: []}
  if depth > 0 { let inner = alt(st, depth - 1) }
  loop {
    break when st.at >= 3
    st = {...st, at: st.at + 1}
  }
  st
}
pure alt(state: Parsed, depth: Int) -> Parsed { branch(state, depth) }
""" + checks)?
  assert annotated.success, annotated.stderr
  assert inferred.success, inferred.stderr
  assert inferred.stdout == annotated.stdout
  assert inferred.stdout == "closed\n"
}

test test_full_inference_open_spread_record_argument_closes_a_generic_call { |ctx|
  let checks = r"""
let start: Parsed = {code: [7], at: 4}
assert width(start) == 4
assert reset(start).code.len() == 0
assert reset(start).at == 4
print "closed"
"""
  let generic = test.run_xsh(ctx, r"""
type Parsed = {code: List[Int], at: Int}
pure same(value) { value }
pure show(value: Parsed) -> Int { value.at }
pure width(state: Parsed) -> Int { let cleared = same({...state, code: []}); show(cleared) }
pure reset(state: Parsed) -> Parsed { same({...state, code: []}) }
""" + checks)?
  let monomorphic = test.run_xsh(ctx, r"""
type Parsed = {code: List[Int], at: Int}
pure same(value: Parsed) -> Parsed { value }
pure show(value: Parsed) -> Int { value.at }
pure width(state: Parsed) -> Int { let cleared = same({...state, code: []}); show(cleared) }
pure reset(state: Parsed) -> Parsed { same({...state, code: []}) }
""" + checks)?
  assert monomorphic.success, monomorphic.stderr
  assert generic.success, generic.stderr
  assert generic.stdout == monomorphic.stdout
  assert generic.stdout == "closed\n"
}

test test_full_inference_erased_open_spread_record_argument_closes_a_generic_call { |ctx|
  let checks = r"""
let start: Parsed = {code: [7], at: 4}
assert width(start) == 4
assert reset(start).code.len() == 0
assert reset(start).at == 4
print "closed"
"""
  let erased = test.run_xsh(ctx, r"""
type Parsed = {code: List[Int], at: Int}
pure same(value) { value }
pure show(value: Parsed) -> Int { value.at }
pure width(state) { let cleared = same({...state, code: []}); show(cleared) }
pure reset(state: Parsed) -> Parsed { same({...state, code: []}) }
""" + checks)?
  let annotated = test.run_xsh(ctx, r"""
type Parsed = {code: List[Int], at: Int}
pure same(value) { value }
pure show(value: Parsed) -> Int { value.at }
pure width(state: Parsed) -> Int { let cleared = same({...state, code: []}); show(cleared) }
pure reset(state: Parsed) -> Parsed { same({...state, code: []}) }
""" + checks)?
  assert annotated.success, annotated.stderr
  assert erased.success, erased.stderr
  assert erased.stdout == annotated.stdout
  assert erased.stdout == "closed\n"
}
