type Port = Int range 1..=65535

type Scale = pure(value: Int) -> Int

type Payload = Union[Str, Path, List[Str], Bytes]

type Word = Union[Str, Path]

type Counts = {retries: Int?}

type ProofPair = {left: Str?, right: Int}

proc alias_guard_after_exit_mutation(choose: Bool) [] -> Str {
  var value: Str? = "ready"
  let available = value != null
  if choose {
    value = null
    return "early"
  }
  guard available else {
    return "missing"
  }
  proof_string(value)
}

pure proof_string(value: Str) -> Str { value }

pure measure(payload: Payload) -> Str {
  return "text " + payload.trim() when payload is Str
  return "file " + payload.name() when payload is Path
  return "list " + payload.join("+") when payload is List[Str]
  f"bytes {payload.len()}"
}

pure pick(maybe: Word?, fallback: Word = "default") -> Str {
  let word = maybe ?? fallback
  return maybe.upper() when maybe != null and maybe is Str
  if let file is Path = word {
    return file.name()
  }
  word
}

pure number_or_text(value: Union[Int, Float, Str]) -> Str {
  if value is (Int | Float) {
    let number: Union[Int, Float] = value
    return if number is Int { "int" } else { "float" }
  }
  value
}

type Envelope = {values: Map[Int]}

pure widen(port: Port) -> Int { port }

pure twice(value: Int) -> Int { value * 2 }

proc bump(n: Int) -> Int { n + 1 }

proc call_dynamic(handle: Proc, argument: Any) -> Result[Any] {
  handle.call(argument)
}

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

test test_retry_whose_body_always_fails_or_exits_has_no_result_to_check {
  let exhausted = retry [0ms] {
    Err(error.failure("x"))?
  }
  assert exhausted is Err(_)
  if false {
    let never = retry [0ms] {
      exit 7
    }
  }
}

test test_collection_updates_keep_the_callables_they_store {
  let callbacks: List[Scale] = []
  let updated = callbacks.push(twice)
  assert updated[0](4) == 8
  let additions: List[Scale] = [twice]
  let extended = callbacks.extend([@additions])
  assert extended[0](4) == 8
  let by_name: Map[Str, Scale] = {}
  let stored = by_name.set("twice", twice)
  with selected = stored.get("twice")? {
    assert selected(4) == 8
  } else { |failure| test.fail(failure.message) }
}

test test_with_binds_a_relative_path_result_and_proc_call_is_dynamic { |ctx|
  let root = test.temp_dir(ctx, name: "verifier-witness")?
  let link = fp"{root}/link"
  var relative = ""
  with stripped = link.strip_prefix(root)? {
    relative = stripped.display()
  } else { |failure| test.fail(failure.message) }
  assert relative == "link"
  assert call_dynamic(bump, 1) is Ok(_)
}

test test_validated_union_member_is_narrowed_by_a_type_test {
  let names: NonEmpty[Str] = ["x"]
  let word: Union[NonEmpty[Str], Int] = names
  if word is NonEmpty[Str] {
    assert word.first() == "x"
  }
  let plain = ["ls", "-l"]
  if plain is NonEmpty[Str] {
    assert plain.last() == "-l"
  }
  var grown: NonEmpty[Str] = ["a"]
  grown += []
  let maybe: NonEmpty[Str]? = ["only"]
  assert maybe != null and maybe.first() == "only"
}

test test_alternative_type_tests_narrow_a_dynamic_value {
  let dynamic = json.decode("\"item\"")?
  if dynamic is (_ is Str | _ is Str) {
    assert dynamic.upper() == "ITEM"
  }
}

test test_earlier_exits_exclude_union_members {
  assert measure("x") == "text x"
  assert measure(b"ab") == "bytes 2"
  assert measure(["a", "b"]) == "list a+b"
  assert pick("x") == "X"
  assert pick(null, fallback: "y") == "y"
  assert number_or_text(1) == "int"
  assert number_or_text(1.5) == "float"
  assert number_or_text("t") == "t"
}

test test_wildcard_if_let_narrows_the_tested_path {
  let counts = Counts(retries: 3)
  if let _ = counts.retries {
    assert counts.retries + 1 == 4
  }
}

test test_untyped_empty_lists_compare_equal {
  let empty = [@[], @[]]
  assert empty == []
  assert [] == []
}

test test_fallback_over_an_error_constructor_takes_its_type_from_the_fallback {
  let direct = Err(error.failure(" direct ")) ?? { |failure|
    failure.message
  }
  assert direct.trim() == "direct"
}

test test_boolean_alias_survives_a_disjoint_write_and_an_exiting_mutation {
  var pair = ProofPair("ready", 1)
  let available = pair.left != null
  guard available else {
    return error.fail("missing left")
  }
  pair.right = 3
  let left = proof_string(pair.left)
  assert left == "ready"
  assert alias_guard_after_exit_mutation(false) == "ready"
  assert alias_guard_after_exit_mutation(true) == "early"
}
