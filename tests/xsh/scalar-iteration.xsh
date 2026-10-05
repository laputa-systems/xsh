test str_iteration_keeps_unicode_scalars_and_nul {
  var characters = [character for character in "Aéé🙂\0"]
  assert characters == ["A", "é", "e", "́", "🙂", "\0"]
  assert [character for character in ""] == []
}

test bytes_iteration_keeps_all_octets_without_decoding {
  var octets = [octet for octet in b"\0\x7f\x80\xff"]
  assert octets == [0, 127, 128, 255]
  assert [octet for octet in b""] == []
}

test scalar_iteration_retains_sources_across_reassignment {
  var text = "éab"
  let characters = collect {
    for character in text {
      text = "replacement"
      yield character
    }
  }

  var payload = b"\0\xff"
  let octets = collect {
    for octet in payload {
      payload = b"changed"
      yield octet
    }
  }

  assert characters == ["é", "a", "b"]
  assert octets == [0, 255]
}

test scalar_comprehensions_keep_types_nested_order_and_guards {
  let pairs = [
    f"{character}:{octet}"
    for character in "éx"
    for octet in b"\x01\x02"
    if octet == 2
  ]
  assert pairs == ["é:2", "x:2"]
  let entries = {character: character.byte_len() for character in "aé"}
  assert entries.get("a")? == 1
  assert entries.get("é")? == 2
}

error ScalarSourceFailure = Missing(source: Str) : NotFound

proc missing_scalar_source() [error] -> Result[Str, ScalarSourceFailure] {
  Err(.Missing(source: "text"))
}

test scalar_result_iteration_keeps_error_identity_and_cleanup { |ctx|
  let output = test.run_script(
    ctx,
    r"""error SourceFailure = Missing(source: Str) : NotFound
proc missing() [error] -> Result[Bytes, SourceFailure] { Err(SourceFailure.Missing(source: "bytes")) }
ctx "iteration" {
  defer { print "cleanup" }
  for octet in missing() { print "unreached" }
}
""",
  )?
  {
    let assertion_condition = ! output.success
    let assertion_message = output.stderr
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """cleanup
"""
  assert "SourceFailure.Missing" in output.stderr
  assert "ctx: iteration" in output.stderr
  let actual = try {
    for _ in missing_scalar_source() {
      print "unreached"
    }
  }
  if let Err(.Missing {source: source}) = actual {
    assert source == "text"
  } else {
    test.fail("expected unchanged source error")
  }

  assert [character for character in Ok("ab")] == ["a", "b"]
  assert [octet for octet in Ok(b"\xff")] == [255]
}

test scalar_iteration_evaluates_source_once_and_keeps_cleanup_transfers { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc source() [error] -> Str { print "source"; "abc" }
for character in source() {
  defer { print f"cleanup:{character}" }
  continue when character == "a"
  print $character
  break
}
stream octets() [error] -> Stream[Int] {
  for octet in b"\x01\x02" { defer { print f"octet:{octet}" }; yield octet }
}
for octet in octets() { print $octet; break }
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """source
cleanup:a
b
cleanup:b
1
octet:1
"""
}

test scalar_iteration_bindings_remain_immutable_and_protocols_stay_bounded { |ctx|
  for source in [
    "for character in \"ab\" { character = \"x\" }",
    "for octet in b\"ab\" { octet = 1 }",
    "let characters = [@\"ab\"]",
    "stream bad() [] -> Stream[Str] { yield @\"ab\" }",
  ] {
    let output = test.run_script(ctx, source)?
    {
      let assertion_condition = ! output.success
      let assertion_message = f"expected rejection: {source}"
      assert assertion_condition, assertion_message
    }
  }
}

test scalar_iteration_preserves_source_view_bounds {
  let text = "aé🙂z".byte_slice(1, 6)
  assert [character for character in text] == ["é", "🙂"]
  let payload = b"\x01\0\xff\x02"[1..3]
  assert [octet for octet in payload] == [0, 255]
}

test scalar_comprehension_errors_keep_nominal_payloads {
  let actual = try {
    [character for character in missing_scalar_source()]
  }
  if let Err(.Missing {source: source}) = actual {
    assert source == "text"
  } else {
    test.fail("expected unchanged comprehension source error")
  }
}

test scalar_result_sources_keep_error_effect_checks { |ctx|
  for source in [
    "proc forbidden(value: Result[Str]) [] { for character in value { let _ = character } }",
    "proc forbidden(value: Result[Bytes]) [] { let _ = [octet for octet in value] }",
  ] {
    let output = test.run_script(ctx, source)?
    {
      let assertion_condition = ! output.success
      let assertion_message = f"expected error effect rejection: {source}"
      assert assertion_condition, assertion_message
    }
    assert "effect" in output.stderr
  }
}

test scalar_iteration_returns_and_string_producer_cancellation_keep_cleanup { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc pick() [] -> Int {
  for character in "ab" {
    defer { print $character }
    return 7 when character == "b"
  }
  0
}
stream characters() [] -> Stream[Str] {
  for character in "é🙂" {
    defer { print f"cleanup:{character}" }
    yield character
  }
}
print ${pick()}
for character in characters() { print $character; break }
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = output
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """a
b
7
é
cleanup:é
"""
}

test scalar_iteration_body_failure_stops_before_later_items_and_finishes_defers { |ctx|
  let output = test.run_script(
    ctx,
    r"""ctx "characters" {
  defer { print "outer" }
  for character in "éx🙂" {
    defer { print f"cleanup:{character}" }
    print $character
    assert character != "x"
  }
}
""",
  )?
  {
    let assertion_condition = ! output.success
    let assertion_message = output.stderr
    assert assertion_condition, assertion_message
  }
  assert output.stdout == """é
cleanup:é
x
cleanup:x
outer
"""
  assert "AssertionError" in output.stderr
  assert "ctx: characters" in output.stderr
}
