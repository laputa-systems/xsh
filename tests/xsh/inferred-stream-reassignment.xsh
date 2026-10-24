test inferred_stream_reassignment { |ctx|
  let source = p"tests/fixtures/runtime/inferred-stream-reassignment.xsh".read_text()?
  test.expect(ctx, source + "\nverify_inferred_stream_reassignment()?\n", status: 0)?
}

test inferred_stream_reassignment_rejects_fallible_and_unknown_writers { |ctx|
  for source in [
    "stream ready() -> Stream[Int] { yield 1 }\nstream fallible() -> Stream[Int] { let _ = \"late\".parse_int()?; yield 1 }\nproc read() [] { var rows = ready(); rows = fallible(); for item in rows {} }",
    "stream ready() -> Stream[Int] { yield 1 }\nproc read(incoming: Stream[Int]) [] { var rows = ready(); rows = incoming; for item in rows {} }",
  ] {
    let output = test.run_xsh(ctx, source)?
    assert output.status == 2
    assert "check.effect-violation" in output.stderr
  }
}
