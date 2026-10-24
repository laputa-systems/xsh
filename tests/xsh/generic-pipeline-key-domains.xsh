test generic_key_domains { |ctx|
  let source = p"tests/fixtures/runtime/generic-pipeline-key-domains.xsh".read_text()?
  test.expect(ctx, source + "\ngeneric_key_domains()?\n", status: 0)?
}
