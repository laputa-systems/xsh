enum AliasEvent { Added(Str), Changed(Str), Count(Int) }
type AliasText = Str
type AliasOtherText = Str
enum AliasMixed { Number(Int), Text(Str) }

pure alias_event_name(event: AliasEvent) -> Str {
  match event {
    (Added(file) | Changed(file)) as original => {
      let typed: AliasEvent = original
      if typed is Added(_) { file } else { file }
    }
    Count(_) => "count"
  }
}

test test_pattern_aliases_capture_whole_nodes_and_preserve_types [error] {
  test.eq(alias_event_name(Added("one")), "one")?
  test.eq(alias_event_name(Changed("two")), "two")?
  let values = [{name: "item", count: 3}]
  let selected = match values {
    [{name, count} as entry] as all => {
      let names: List[Str] = [entry.name, all[0].name, name]
      f"${names.join(":")}:${count}"
    }
    _ => "other"
  }
  test.eq(selected, "item:item:item:3")?
}

test test_pattern_alternatives_publish_only_first_complete_match [error] {
  let result = match [[99], [7, 8]] {
    ([[value, ..tail], [99]] | [[99], [value, ..tail]]) as original => {
      test.eq(original, [[99], [7, 8]])?
      value + tail.len()
    }
    _ => 0
  }
  test.eq(result, 8)?
  let first = match [7, 9] {
    [value, _] | [_, value] => value
    _ => 0
  }
  test.eq(first, 7)?
}

test test_pattern_aliases_work_in_iflet_and_whilelet [error] {
  if let (Added(file) | Changed(file)) as original = Changed("selected") {
    test.eq(file, "selected")?
    test.eq(original is Changed(_), true)?
  } else { test.fail("expected selected branch")? }
  var current = [1, 2]
  var total = 0
  while let [head, ..tail] as values = current {
    test.eq(values.len(), tail.len() + 1)?
    total += head
    current = tail
  }
  test.eq(total, 3)?
}

test test_pattern_aliases_and_alternatives_evaluate_subject_and_guard_once [error] { |ctx|
  let source = r"""proc subject() [io] -> List[Int] { print "subject"; [1] }
proc allowed() [io] -> Bool { print "guard"; false }
let selected = match subject() {
  [value, ..tail] | [value, ..tail] if allowed() => value + tail.len()
  _ => 9
}
print $selected
"""
  let result = test.run_script(ctx, source)?
  test.ok(result.success, result.stderr)?
  test.eq(result.stdout, "subject\nguard\n9\n")?
}

test test_pattern_tests_accept_only_grouped_capture_free_alternatives [error] {
  test.eq(1 is (1 | 2), true)?
  test.eq(3 is (1 | 2), false)?
  test.eq(["build"] is (["build"] | ["clean"]), true)?
  test.eq(Added("item") is (Added(_) | Changed(_)), true)?
}

test test_pattern_aliases_reject_invalid_names_duplicates_and_incompatible_alternatives [error] { |ctx|
  for source in [
    "let result = match [1] { [value] as value => value _ => 0 }\n",
    "let result = match {left: 1, right: 2} { {left: value, right: value} => value _ => 0 }\n",
    "let result = match [1] { [left] | [right] => 1 _ => 0 }\n",
    "enum Event { Number(Int), Text(Str) }\nlet result = match Number(1) { Number(value) | Text(value) => 1 _ => 0 }\n",
    "let result = match 1 { 1 | 2 as original => 1 _ => 0 }\n",
  ] {
    let result = test.run_script(ctx, source)?
    test.ok(! result.success, result.stderr)?
    test.contains(result.stderr, "check.pattern-")?
  }
  for source in [
    "let result = match 1 { 1 as _ => 1 _ => 0 }\n",
    "let result = match 1 { (1, 2) => 1 _ => 0 }\n",
    "let result = 1 is 1 | 2\n",
  ] {
    let result = test.run_script(ctx, source)?
    test.ok(! result.success, result.stderr)?
    test.contains(result.stderr, "parse.")?
  }
  for source in [
    "let result = [1] is ([value] | [value])\n",
    "let result = 1 is (1 as value | 2 as value)\n",
  ] {
    let result = test.run_script(ctx, source)?
    test.ok(! result.success, result.stderr)?
    test.contains(result.stderr, "check.pattern-test-binding")?
  }
}

test test_pattern_alternatives_compare_resolved_type_aliases [error] {
  let dynamic = json.decode("\"item\"")?
  let selected = match dynamic {
    (value is AliasText | value is AliasOtherText) as original => {
      let typed: Any = original
      test.eq(typed is Str, true)?
      value.upper()
    }
    _ => "other"
  }
  test.eq(selected, "ITEM")?
}

test test_pattern_alternatives_respect_capture_order_and_conservative_narrowing [error] {
  let selected = match [4, 7] {
    [left, right] | [right, left] => left * 10 + right
    _ => 0
  }
  test.eq(selected, 47)?
  let dynamic = json.decode("\"item\"")?
  if dynamic is (_ is Str | _ is AliasText) {
    test.eq(dynamic.upper(), "ITEM")?
  } else { test.fail("expected string")? }
}

error AliasFailure = Missing(message: Str) : NotFound | Denied(message: Str) : PermissionDenied

pure alias_failure_message(failure: AliasFailure) -> Str {
  match failure {
    (AliasFailure.Missing {message} | AliasFailure.Denied {message}) as original => {
      let typed: AliasFailure = original
      message + typed.message
    }
    _ => "other"
  }
}

pure alias_inferred_tail(values: List[Int]) {
  match values {
    [head, ..tail] as original => original.len() + head + tail.len()
    _ => 0
  }
}

test test_pattern_aliases_preserve_nominal_error_identity_and_inferred_returns [error] {
  test.eq(alias_failure_message(AliasFailure.Missing("absent")), "absentabsent")?
  test.eq(alias_failure_message(AliasFailure.Denied("denied")), "denieddenied")?
  let failure: AliasFailure = AliasFailure.Missing("one")
  if let (AliasFailure.Missing {message} | AliasFailure.Denied {message}) as original = failure {
    test.eq(original is NotFound, true)?
    test.eq(message, "one")?
  }
  let inferred: Int = alias_inferred_tail([4, 8])
  test.eq(inferred, 7)?
}

test test_pattern_aliases_keep_list_value_semantics [error] {
  let source = [1, 2]
  if let [head, ..tail] as original = source {
    var copied = original
    copied = copied.push(3)
    var rest = tail
    rest = rest.push(4)
    test.eq(source, [1, 2])?
    test.eq(original, [1, 2])?
    test.eq(tail, [2])?
    test.eq(copied, [1, 2, 3])?
    test.eq(rest, [2, 4])?
    test.eq(head, 1)?
  }
}

test test_pattern_tests_require_grouping_for_nested_alternatives [error] { |ctx|
  let bad = test.run_script(ctx, "let selected = [1] is [1 | 2]\n")?
  test.ok(! bad.success, bad.stderr)?
  test.contains(bad.stderr, "check.pattern-test-alternation")?
  test.eq([1] is [(1 | 2)], true)?
}
