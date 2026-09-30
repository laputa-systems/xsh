test str_iteration_keeps_unicode_scalars_and_nul [error] {
  var characters: List[Str] = []
  for character in "A\u{e9}e\u{301}\u{1f642}\0" {
    characters = characters.extend([character])
  }
  test.eq(characters, ["A", "\u{e9}", "e", "\u{301}", "\u{1f642}", "\0"])?
  test.eq([character for character in ""], [])?
}

test bytes_iteration_keeps_all_octets_without_decoding [error] {
  var octets: List[Int] = []
  for octet in b"\x00\x7f\x80\xff" {
    octets = octets.extend([octet])
  }
  test.eq(octets, [0, 127, 128, 255])?
  test.eq([octet for octet in b""], [])?
}

test scalar_iteration_retains_sources_across_reassignment [error] {
  var text = "\u{e9}ab"
  var characters: List[Str] = []
  for character in text {
    text = "replacement"
    characters = characters.extend([character])
  }
  var payload = b"\x00\xff"
  var octets: List[Int] = []
  for octet in payload {
    payload = b"changed"
    octets = octets.extend([octet])
  }
  test.eq(characters, ["\u{e9}", "a", "b"])?
  test.eq(octets, [0, 255])?
}

test scalar_comprehensions_keep_types_nested_order_and_guards [error] {
  let pairs: List[Str] = [f"$character:$octet" for character in "\u{e9}x" for octet in b"\x01\x02" if octet == 2]
  test.eq(pairs, ["\u{e9}:2", "x:2"])?
  let entries: Map[Int] = {character: character.byte_len() for character in "a\u{e9}"}
  test.eq(entries.get("a")?, 1)?
  test.eq(entries.get("\u{e9}")?, 2)?
}

error ScalarSourceFailure = Missing(source: Str) : NotFound
proc missing_scalar_source() [error] -> Result[Str, ScalarSourceFailure] {
  Err(ScalarSourceFailure.Missing(source: "text"))
}

test scalar_result_iteration_keeps_error_identity_and_cleanup [error] { |ctx|
  let output = test.run_script(ctx, r"""error SourceFailure = Missing(source: Str) : NotFound
proc missing() [error] -> Result[Bytes, SourceFailure] { Err(SourceFailure.Missing(source: "bytes")) }
ctx "iteration" {
  defer { print "cleanup" }
  for octet in missing() { print "unreached" }
}
""")?
  test.ok(! output.success, output.stderr)?
  test.eq(output.stdout, "cleanup\n")?
  test.ok("SourceFailure.Missing" in output.stderr)?
  test.ok("ctx: iteration" in output.stderr)?
  let actual = try { for character in missing_scalar_source() { print "unreached" } }
  match actual {
    Err(ScalarSourceFailure.Missing {source}) => test.eq(source, "text")?
    _ => test.fail("expected unchanged source error")?
  }
  test.eq([character for character in Ok("ab")], ["a", "b"])?
  test.eq([octet for octet in Ok(b"\xff")], [255])?
}

test scalar_iteration_evaluates_source_once_and_keeps_cleanup_transfers [error] { |ctx|
  let output = test.run_script(ctx, r"""proc source() [error] -> Str { print "source"; "abc" }
for character in source() {
  defer { print f"cleanup:$character" }
  continue when character == "a"
  print $character
  break
}
stream octets() [error] -> Stream[Int] {
  for octet in b"\x01\x02" { defer { print f"octet:$octet" }; yield octet }
}
for octet in octets() { print $octet; break }
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "source\ncleanup:a\nb\ncleanup:b\n1\noctet:1\n")?
}

test scalar_iteration_bindings_remain_immutable_and_protocols_stay_bounded [error] { |ctx|
  for source in ["for character in \"ab\" { character = \"x\" }", "for octet in b\"ab\" { octet = 1 }", "let characters = [@\"ab\"]", "stream bad() [] -> Stream[Str] { yield @\"ab\" }"] {
    let output = test.run_script(ctx, source)?
    test.ok(! output.success, f"expected rejection: $source")?
  }
}

test scalar_iteration_preserves_source_view_bounds [error] {
  let text = "a\u{e9}\u{1f642}z".byte_slice(1, 6)
  test.eq([character for character in text], ["\u{e9}", "\u{1f642}"])?
  let payload = b"\x01\x00\xff\x02".slice(1, 2)
  test.eq([octet for octet in payload], [0, 255])?
}

test scalar_comprehension_errors_keep_nominal_payloads [error] {
  let actual = try { [character for character in missing_scalar_source()] }
  match actual {
    Err(ScalarSourceFailure.Missing {source}) => test.eq(source, "text")?
    _ => test.fail("expected unchanged comprehension source error")?
  }
}

test scalar_result_sources_keep_error_effect_checks [error] { |ctx|
  for source in [
    "proc forbidden(value: Result[Str]) [] { for character in value { let _ = character } }",
    "proc forbidden(value: Result[Bytes]) [] { let _ = [octet for octet in value] }",
  ] {
    let output = test.run_script(ctx, source)?
    test.ok(! output.success, f"expected error effect rejection: $source")?
    test.ok("effect" in output.stderr)?
  }
}

test scalar_iteration_returns_and_string_producer_cancellation_keep_cleanup [error] { |ctx|
  let output = test.run_script(ctx, r"""proc pick() [] -> Int {
  for character in "ab" {
    defer { print $character }
    return 7 when character == "b"
  }
  0
}
stream characters() [] -> Stream[Str] {
  for character in "é🙂" {
    defer { print f"cleanup:$character" }
    yield character
  }
}
print ${pick()}
for character in characters() { print $character; break }
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "a\nb\n7\né\ncleanup:é\n")?
}

test scalar_iteration_body_failure_stops_before_later_items_and_finishes_defers [error] { |ctx|
  let output = test.run_script(ctx, r"""ctx "characters" {
  defer { print "outer" }
  for character in "éx🙂" {
    defer { print f"cleanup:$character" }
    print $character
    character != "x"
  }
}
""")?
  test.ok(! output.success, output.stderr)?
  test.eq(output.stdout, "é\ncleanup:é\nx\ncleanup:x\nouter\n")?
  test.ok("AssertionError" in output.stderr)?
  test.ok("ctx: characters" in output.stderr)?
}
