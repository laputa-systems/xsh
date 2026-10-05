type ListEntry = {labels: List[Str], result: Result[Int]}

pure list_shape(values: List[Int]) -> Str {
  match values {
    [] => "empty"
    [first, second] => f"two:{first}:{second}"
    [head, ..tail] => f"many:{head}:{tail.len()}"
  }
}

test test_list_pattern_exact_and_trailing_rest {
  assert list_shape([]) == "empty"
  assert list_shape([4]) == "many:4:0"
  assert list_shape([4, 5]) == "two:4:5"
  assert list_shape([4, 5, 6]) == "many:4:2"
  let selected = if let ["build", target, ..] = ["build", "kernel", "verbose"] {
    target
  } else {
    "other"
  }
  assert selected == "kernel"
}

test test_list_pattern_nested_records_and_constructors {
  let values: List[ListEntry] = [
    {
      labels: [
        "build",
        "kernel",
      ],
      result: Ok(7),
    },
  ]
  let selected = if let [{labels: ["build", target], result: Ok(count)}, ..rest] = values {
    let typed_rest: List[ListEntry] = rest
    f"{target}:{count}:{typed_rest.len()}"
  } else {
    "other"
  }
  assert selected == "kernel:7:0"
}

test test_list_pattern_mismatch_does_not_index_or_publish_bindings {
  let short = match [1] {
    [first, 99] => first,
    [matched] => matched + 10,
    else => 0,
  }
  assert short == 11
  let nested = match [[1, 2], [3]] {
    [[first, ..tail], [99]] => first + tail.len(),
    [[first, ..tail], [last]] => first + tail.len() + last,
    else => 0,
  }
  assert nested == 5
}

test test_list_pattern_rest_preserves_value_semantics {
  var values = [1, 2, 3]
  let rest = if let [_, ..tail] = values {
    tail
  } else {
    []
  }
  values += [99]
  assert rest == [2, 3]
  var changed = rest
  changed += [42]
  assert rest == [2, 3]
  assert changed == [2, 3, 42]
  assert values == [1, 2, 3, 99]
}

test test_list_pattern_nonbinding_predicates {
  let values = ["build", "kernel"]
  assert values is ["build", _]
  assert values is ["build", _, ..]
  assert ! (values is ["build"])
  assert ! (values is [])
  assert values is [..]
}

test test_list_pattern_dynamic_elements_keep_type_narrowing {
  let value = json.decode("[7, \"kernel\"]")?
  let selected = if let [count is Int, name is Str] = value {
    f"{count + 1}:{name.upper()}"
  } else {
    "other"
  }
  assert selected == "8:KERNEL"
  assert value is [_ is Int, _ is Str]
  let scalar = json.decode("7")?
  assert ! (scalar is [..])
}

test test_list_pattern_rejects_unsupported_subjects_and_bindings { |ctx|
  for source in [
    """let selected = match "abc" { [_, ..] => 1 _ => 0 }
""",
    """let selected = match b"abc" { [_, ..] => 1 _ => 0 }
""",
    """let selected = match {name: "x"} { [_, ..] => 1 _ => 0 }
""",
    """let selected = match Ok([1]) { [_, ..] => 1 _ => 0 }
""",
    """stream numbers() [] -> Stream[Int] { yield 1 }
let selected = match numbers() { [_, ..] => 1 _ => 0 }
""",
  ] {
    let result = test.run_script(ctx, source)?
    let rejected = ! result.success
    let rejection_details = result.stderr
    assert rejected, rejection_details
    assert "check.pattern-type" in result.stderr
  }

  for source in [
    """let selected = [1, 2] is [first, _]
""",
    """let selected = [1, 2] is [_, ..tail]
""",
  ] {
    let result = test.run_script(ctx, source)?
    let rejected = ! result.success
    let rejection_details = result.stderr
    assert rejected, rejection_details
    assert "check.pattern-test-binding" in result.stderr
  }

  let ordinary = test.run_script(
    ctx,
    """let [first, second] = [1, 2]
""",
  )?
  let rejected = ! ordinary.success
  let rejection_details = ordinary.stderr
  assert rejected, rejection_details
}

test test_list_pattern_rejects_middle_and_duplicate_rests { |ctx|
  for source in [
    """let selected = match [1, 2] { [..tail, last] => last _ => 0 }
""",
    """let selected = match [1, 2] { [first, ..tail, ..other] => first _ => 0 }
""",
  ] {
    let result = test.run_script(ctx, source)?
    let rejected = ! result.success
    let rejection_details = result.stderr
    assert rejected, rejection_details
    assert "parse.list-pattern-rest" in result.stderr
  }
}

test test_list_pattern_exhaustiveness_is_conservative { |ctx|
  for source in [
    """pure selected(values: List[Int]) -> Int { match values { [] => 0, [_, ..] => 1 } }
""",
    """pure selected(values: List[Int]) -> Int { match values { [..tail] => tail.len() } }
""",
  ] {
    let result = test.run_script(ctx, source)?
    let {success: succeeded, stderr: failure_details, ..} = result
    assert succeeded, failure_details
  }

  for source in [
    """pure selected(values: List[Int]) -> Int { match values { [] => 0 } }
""",
    """pure selected(values: List[Int]) -> Int { match values { [] => 0, [7, ..] => 1 } }
""",
    """pure selected(values: List[Any]) -> Int { match values { [] => 0, [{}, ..] => 1 } }
""",
  ] {
    let result = test.run_script(ctx, source)?
    let rejected = ! result.success
    let rejection_details = result.stderr
    assert rejected, rejection_details
    assert "check.match-value-exhaustive" in result.stderr
  }
}

test test_list_pattern_rejects_duplicates_and_honors_guards { |ctx|
  let guarded = match [1, 2] {
    [head, ..tail] if head == 9 => tail.len(),
    [head, ..tail] => head + tail.len(),
    [] => 0,
  }
  assert guarded == 2
  let duplicate = test.run_script(
    ctx,
    """let selected = match [1, 2] { [same, same] => same _ => 0 }
""",
  )?
  let rejected = ! duplicate.success
  let rejection_details = duplicate.stderr
  assert rejected, rejection_details
  assert "check.pattern-binding" in duplicate.stderr
  let unreachable = test.run_script(
    ctx,
    """let selected = match [1] { [..] => 1, [] => 0 }
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = unreachable
  assert succeeded, failure_details
}

test test_list_pattern_ordinary_literals {
  let values = json.decode("[null, true, 1.5]")?
  assert values is [null, true, 1.5]
  assert [b"abc"] is [b"abc"]
  assert [2ms] is [2ms]
}
