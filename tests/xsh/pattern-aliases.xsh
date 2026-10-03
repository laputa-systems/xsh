enum AliasEvent {
    Added(Str),
    Changed(Str),
    Count(Int),
}

type AliasText = Str

pure alias_event_name(event: AliasEvent) -> Str {
  match event {
    (Added(file) | Changed(file)) as original => {
      let typed: AliasEvent = original
      if typed is Added(_) {
        file
      } else {
        file
      }
    }
    Count(_) => "count"
  }
}

test test_pattern_aliases_capture_whole_nodes_and_preserve_types { |ctx|
  assert alias_event_name(Added("one")) == "one"
  assert alias_event_name(Changed("two")) == "two"
  let output = test.run_script(
    ctx,
    r"""proc witness() [error] {
  let values = [{name: "item", count: 3}]
  let selected = match values {
    [{name, count} as entry] as all => {
      let names: List[Str] = [entry.name, all[0].name, name]
      f"${names.join(":")}:${count}"
    }
    _ => "other"
  }
  assert (selected) == ("item:item:item:3")
}
witness()
""",
  )?
  let {success: assertion_condition, stderr: assertion_message, ..} = output
  assert assertion_condition, assertion_message
  assert output.stdout == ""
}

test test_pattern_alternatives_publish_only_first_complete_match { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc witness() [error] {
  let result = match [[99], [7, 8]] {
    ([[value, ..tail], [99]] | [[99], [value, ..tail]]) as original => {
      assert (original) == ([[99], [7, 8]])
      value + tail.len()
    }
    _ => 0
  }
  assert (result) == (8)
  let first = match [7, 9] {
    [value, _] | [_, value] => value
    _ => 0
  }
  assert (first) == (7)
}
witness()
""",
  )?
  let {success: assertion_condition, stderr: assertion_message, ..} = output
  assert assertion_condition, assertion_message
  assert output.stdout == ""
}

test test_pattern_aliases_work_in_iflet_and_whilelet {
  if let (Added(file) | Changed(file)) as original = Changed("selected") {
    assert file == "selected"
    assert (original is Changed(_)) == true
  } else {
    test.fail("expected selected branch")?
  }

  var current = [1, 2]
  var total = 0
  while let [head, ..tail] as values = current {
    assert values.len() == tail.len() + 1
    total += head
    current = tail
  }

  assert total == 3
}

test test_pattern_aliases_and_alternatives_evaluate_subject_and_guard_once { |ctx|
  let source = r"""proc subject() [io] -> List[Int] { print "subject"; [1] }
proc allowed() [io] -> Bool { print "guard"; false }
let selected = match subject() {
  [value, ..tail] | [value, ..tail] if allowed() => value + tail.len()
  _ => 9
}
print $selected
"""
  let result = test.run_script(ctx, source)?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = result
    assert assertion_condition, assertion_message
  }
  assert result.stdout == """subject
guard
9
"""
}

test test_pattern_tests_accept_only_grouped_capture_free_alternatives {
  assert (1 is (1 | 2)) == true
  assert (3 is (1 | 2)) == false
  assert (["build"] is (["build"] | ["clean"])) == true
  assert (Added("item") is (Added(_) | Changed(_))) == true
}

test test_pattern_aliases_reject_invalid_names_duplicates_and_incompatible_alternatives { |ctx|
  for source in [
    """let result = match [1] { [value] as value => value _ => 0 }
""",
    """let result = match {left: 1, right: 2} { {left: value, right: value} => value _ => 0 }
""",
    """let result = match [1] { [left] | [right] => 1 _ => 0 }
""",
    """enum Event { Number(Int), Text(Str) }
let result = match Number(1) { Number(value) | Text(value) => 1 _ => 0 }
""",
    """let result = match 1 { 1 | 2 as original => 1 _ => 0 }
""",
  ] {
    let result = test.run_script(ctx, source)?
    {
      let assertion_condition = ! result.success
      let assertion_message = result.stderr
      assert assertion_condition, assertion_message
    }
    assert "check.pattern-" in result.stderr
  }

  for source in [
    """let result = match 1 { 1 as _ => 1 _ => 0 }
""",
    """let result = match 1 { (1, 2) => 1 _ => 0 }
""",
    """let result = 1 is 1 | 2
""",
  ] {
    let result = test.run_script(ctx, source)?
    {
      let assertion_condition = ! result.success
      let assertion_message = result.stderr
      assert assertion_condition, assertion_message
    }
    assert "parse." in result.stderr
  }

  for source in [
    """let result = [1] is ([value] | [value])
""",
    """let result = 1 is (1 as value | 2 as value)
""",
  ] {
    let result = test.run_script(ctx, source)?
    {
      let assertion_condition = ! result.success
      let assertion_message = result.stderr
      assert assertion_condition, assertion_message
    }
    assert "check.pattern-test-binding" in result.stderr
  }
}

test test_pattern_alternatives_compare_resolved_type_aliases { |ctx|
  let output = test.run_script(
    ctx,
    r"""type AliasText = Str
type AliasOtherText = Str
proc witness() [error] {
  let dynamic = json.decode("\"item\"")?
  let selected = match dynamic {
    (value is AliasText | value is AliasOtherText) as original => {
      let typed: Any = original
      assert (typed is Str) == (true)
      value.upper()
    }
    _ => "other"
  }
  assert (selected) == ("ITEM")
}
witness()
""",
  )?
  let {success: assertion_condition, stderr: assertion_message, ..} = output
  assert assertion_condition, assertion_message
  assert output.stdout == ""
}

test test_pattern_alternatives_respect_capture_order_and_conservative_narrowing {
  let selected = match [4, 7] {
    [left, right] | [right, left] => left * 10 + right,
    _ => 0,
  }
  assert selected == 47
  let dynamic = json.decode("\"item\"")?
  if dynamic is (_ is Str | _ is AliasText) {
    assert dynamic.upper() == "ITEM"
  } else {
    test.fail("expected string")?
  }
}

test test_pattern_aliases_preserve_nominal_error_identity_and_inferred_returns { |ctx|
  let output = test.run_script(
    ctx,
    r"""error AliasFailure = Missing(message: Str) : NotFound | Denied(message: Str) : PermissionDenied

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
proc witness() [error] {
  assert (alias_failure_message(AliasFailure.Missing("absent"))) == ("absentabsent")
  assert (alias_failure_message(AliasFailure.Denied("denied"))) == ("denieddenied")
  let failure: AliasFailure = AliasFailure.Missing("one")
  if let (AliasFailure.Missing {message} | AliasFailure.Denied {message}) as original = failure {
    assert (original is NotFound) == (true)
    assert (message) == ("one")
  }
  let inferred: Int = alias_inferred_tail([4, 8])
  assert (inferred) == (7)
}
witness()
""",
  )?
  let {success: assertion_condition, stderr: assertion_message, ..} = output
  assert assertion_condition, assertion_message
  assert output.stdout == ""
}

test test_pattern_aliases_keep_list_value_semantics {
  let source = [1, 2]
  if let [head, ..tail] as original = source {
    var copied = original
    copied += [3]
    var rest = tail
    rest += [4]
    assert source == [1, 2]
    assert original == [1, 2]
    assert tail == [2]
    assert copied == [1, 2, 3]
    assert rest == [2, 4]
    assert head == 1
  }
}

test test_pattern_tests_require_grouping_for_nested_alternatives { |ctx|
  let bad = test.run_script(
    ctx,
    """let selected = [1] is [1 | 2]
""",
  )?
  {
    let assertion_condition = ! bad.success
    let assertion_message = bad.stderr
    assert assertion_condition, assertion_message
  }
  assert "check.pattern-test-alternation" in bad.stderr
  assert ([1] is [(1 | 2)]) == true
}
