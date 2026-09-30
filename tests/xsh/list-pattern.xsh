type ListEntry = {labels: List[Str], result: Result[Int]}

pure list_shape(values: List[Int]) -> Str {
  match values {
    [] => "empty"
    [first, second] => f"two:${first}:${second}"
    [head, ..tail] => f"many:${head}:${tail.len()}"
  }
}

test test_list_pattern_exact_and_trailing_rest [error] {
  test.eq(list_shape([]), "empty")?
  test.eq(list_shape([4]), "many:4:0")?
  test.eq(list_shape([4, 5]), "two:4:5")?
  test.eq(list_shape([4, 5, 6]), "many:4:2")?
  let selected = match ["build", "kernel", "verbose"] {
    ["build", target, ..] => target
    _ => "other"
  }
  test.eq(selected, "kernel")?
}

test test_list_pattern_nested_records_and_constructors [error] {
  let values: List[ListEntry] = [
    {labels: ["build", "kernel"], result: Ok(7)},
  ]
  let selected = match values {
    [{labels: ["build", target], result: Ok(count)}, ..rest] => {
      let typed_rest: List[ListEntry] = rest
      f"${target}:${count}:${typed_rest.len()}"
    }
    _ => "other"
  }
  test.eq(selected, "kernel:7:0")?
}

test test_list_pattern_mismatch_does_not_index_or_publish_bindings [error] {
  let short = match [1] {
    [first, 99] => first
    [matched] => matched + 10
    _ => 0
  }
  test.eq(short, 11)?
  let nested = match [[1, 2], [3]] {
    [[first, ..tail], [99]] => first + tail.len()
    [[first, ..tail], [last]] => first + tail.len() + last
    _ => 0
  }
  test.eq(nested, 5)?
}

test test_list_pattern_rest_preserves_value_semantics [error] {
  var values = [1, 2, 3]
  let rest = match values {
    [_, ..tail] => tail
    _ => []
  }
  values += [99]
  test.eq(rest, [2, 3])?
  var changed = rest
  changed += [42]
  test.eq(rest, [2, 3])?
  test.eq(changed, [2, 3, 42])?
  test.eq(values, [1, 2, 3, 99])?
}

test test_list_pattern_nonbinding_predicates [error] {
  let values = ["build", "kernel"]
  test.eq(values is ["build", _], true)?
  test.eq(values is ["build", _, ..], true)?
  test.eq(values is ["build"], false)?
  test.eq(values is [], false)?
  test.eq(values is [..], true)?
}

test test_list_pattern_dynamic_elements_keep_type_narrowing [error] {
  let value = json.decode("[7, \"kernel\"]")?
  let selected = match value {
    [count is Int, name is Str] => f"${count + 1}:${name.upper()}"
    _ => "other"
  }
  test.eq(selected, "8:KERNEL")?
  test.eq(value is [_ is Int, _ is Str], true)?
  let scalar = json.decode("7")?
  test.eq(scalar is [..], false)?
}

test test_list_pattern_rejects_unsupported_subjects_and_bindings [error] { |ctx|
  for source in [
    "let selected = match \"abc\" { [_, ..] => 1 _ => 0 }\n",
    "let selected = match b\"abc\" { [_, ..] => 1 _ => 0 }\n",
    "let selected = match {name: \"x\"} { [_, ..] => 1 _ => 0 }\n",
    "let selected = match Ok([1]) { [_, ..] => 1 _ => 0 }\n",
    "stream numbers() [] -> Stream[Int] { yield 1 }\nlet selected = match numbers() { [_, ..] => 1 _ => 0 }\n",
  ] {
    let result = test.run_script(ctx, source)?
    test.ok(! result.success, result.stderr)?
    test.ok("check.pattern-type" in result.stderr)?
  }
  for source in [
    "let selected = [1, 2] is [first, _]\n",
    "let selected = [1, 2] is [_, ..tail]\n",
  ] {
    let result = test.run_script(ctx, source)?
    test.ok(! result.success, result.stderr)?
    test.ok("check.pattern-test-binding" in result.stderr)?
  }
  let ordinary = test.run_script(ctx, "let [first, second] = [1, 2]\n")?
  test.ok(! ordinary.success, ordinary.stderr)?
}

test test_list_pattern_rejects_middle_and_duplicate_rests [error] { |ctx|
  for source in [
    "let selected = match [1, 2] { [..tail, last] => last _ => 0 }\n",
    "let selected = match [1, 2] { [first, ..tail, ..other] => first _ => 0 }\n",
  ] {
    let result = test.run_script(ctx, source)?
    test.ok(! result.success, result.stderr)?
    test.ok("parse.list-pattern-rest" in result.stderr)?
  }
}

test test_list_pattern_exhaustiveness_is_conservative [error] { |ctx|
  for source in [
    "pure selected(values: List[Int]) -> Int { match values { [] => 0, [_, ..] => 1 } }\n",
    "pure selected(values: List[Int]) -> Int { match values { [..tail] => tail.len() } }\n",
  ] {
    let result = test.run_script(ctx, source)?
    test.ok(result.success, result.stderr)?
  }
  for source in [
    "pure selected(values: List[Int]) -> Int { match values { [] => 0 } }\n",
    "pure selected(values: List[Int]) -> Int { match values { [] => 0, [7, ..] => 1 } }\n",
    "pure selected(values: List[Any]) -> Int { match values { [] => 0, [{}, ..] => 1 } }\n",
  ] {
    let result = test.run_script(ctx, source)?
    test.ok(! result.success, result.stderr)?
    test.ok("check.match-value-exhaustive" in result.stderr)?
  }
}

test test_list_pattern_rejects_duplicates_and_honors_guards [error] { |ctx|
  let guarded = match [1, 2] {
    [head, ..tail] if head == 9 => tail.len()
    [head, ..tail] => head + tail.len()
    [] => 0
  }
  test.eq(guarded, 2)?
  let duplicate = test.run_script(ctx, "let selected = match [1, 2] { [same, same] => same _ => 0 }\n")?
  test.ok(! duplicate.success, duplicate.stderr)?
  test.ok("check.pattern-binding" in duplicate.stderr)?
  let unreachable = test.run_script(ctx, "let selected = match [1] { [..] => 1, [] => 0 }\n")?
  test.ok(unreachable.success, unreachable.stderr)?
}

test test_list_pattern_ordinary_literals [error] {
  let values = json.decode("[null, true, 1.5]")?
  test.eq(values is [null, true, 1.5], true)?
  test.eq([b"abc"] is [b"abc"], true)?
  test.eq([2ms] is [2ms], true)?
}
