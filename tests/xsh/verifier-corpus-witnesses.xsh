type Port = Int range 1..=65535

type Scale = pure(value: Int) -> Int

type Envelope = {values: Map[Int]}

pure widen(port: Port) -> Int { port }

pure twice(value: Int) -> Int { value * 2 }

pure joined(dir: RelPath, name: RelPath) -> RelPath { fp"{dir}/{name}" }

pure only_text(value: Any) -> Str {
  return value when value is Str
  "missing"
}

pure wrapped_label(error: Error) -> Str {
  match Err(error) {
    Err(problem) => problem.message
    else => "other"
  }
}

test test_bounded_literal_is_checked_by_the_verifier_through_an_optional_slot {
  let maybe: Port? = 22
  assert maybe == 22
}

test test_is_test_narrows_an_unannotated_integer_to_a_bounded_type {
  let number = 8080
  if number is Port {
    assert widen(number) == 8080
  }
}

test test_confined_path_interpolation_keeps_its_relative_path_type {
  let maybe: RelPath? = "opt"
  assert joined("a", "b").display() == "a/b"
  assert maybe == "opt"
}

test test_compound_assignment_takes_an_operand_the_operator_allows {
  var pause = 1s
  pause /= 5
  pause *= 3
  pause += 1s
  assert pause == 1600ms
}

test test_count_key_may_be_any_supported_scalar {
  let counts = [1, 2, 3] |> count { |value| value % 2 }
  let flags = ["a", "bb"] |> count { |word| word.byte_len() > 1 }
  assert (counts.get("1") ?? 0) == 2
  assert (flags.get("true") ?? 0) == 1
}

test test_match_subject_may_be_an_error_constructor_or_failure {
  assert wrapped_label(error.failure("x")) == "x"
  match error.fail("disk full") {
    Err(problem) => assert problem.message == "disk full"
  }
}

test test_erased_record_index_and_field_test_have_dynamic_contracts {
  let erased: Record = {name: "demo", nested: {count: 1}}
  assert erased["name"] == "demo"
  let text_field = erased.name is Str
  guard text_field else {
    return error.fail("not text")
  }
}

test test_retry_body_may_end_in_an_empty_collection_literal {
  let counts: Result[Map[Str, Int]] = retry [] {
    {}
  }
  let names: Result[List[Str]] = retry [] {
    []
  }
  assert counts?.len() == 0
  assert names?.len() == 0
}

test test_plain_run_pipeline_value_is_a_status {
  let status = run true | run false
  assert ! status.success
}

test test_stream_run_head_feeds_a_pipeline {
  assert (run.stream --text printf "c\n" |> collect()) == ["c"]
}

test test_returned_dynamic_value_is_narrowed_by_a_trailing_type_test {
  assert only_text("x") == "x"
  assert only_text(42) == "missing"
}

test test_prepared_map_constant_fills_a_record_constructor_field {
  let envelope = Envelope({answer: 47})
  assert envelope.values.get("answer")? == 47
}

test test_callable_lookup_in_an_empty_map_falls_back {
  let callbacks: Map[Str, Scale] = {}
  let selected = callbacks.get("thrice") ?? twice
  assert selected(4) == 8
}
