test typed_set_producers { |ctx|
  let source = p"tests/fixtures/runtime/typed-set-producers.xsh".read_text()?
  test.expect(ctx, source + "\nverify_typed_set_producers()?\n", status: 0)?
}

test typed_set_producers_reject_incompatible_domains { |ctx|
  for operator in ["|", "&"] {
    for source in [
      "pure rejected(left: Set[Str], right: Set[Int]) -> Set[Str] { left " + operator + " right }",
      "pure rejected(left: Any, right: Set[Str]) -> Set[Str] { left " + operator + " right }",
      "pure rejected(left: List[Str], right: Set[Str]) -> Set[Str] { left " + operator + " right }",
      "pure rejected(left: Set[Float], right: Set[Float]) -> Set[Float] { left " + operator + " right }",
      "pure rejected(left: Set[Str], right: Set[Str]) -> Set[Int] { left " + operator + " right }",
    ] {
      let output = test.run_xsh(ctx, source)?
      assert output.status == 2
    }
  }
}
