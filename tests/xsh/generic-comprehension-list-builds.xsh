test generic_comprehension_list_builds { |ctx|
  let source = p"tests/fixtures/runtime/generic-comprehension-list-builds.xsh".read_text()?
  test.expect(ctx, source + "\nverify_generic_comprehension_list_builds()?\n", status: 0)?
}

test generic_comprehension_list_builds_reject_unproven_nonempty { |ctx|
  let source = p"tests/fixtures/runtime/generic-comprehension-list-builds.xsh".read_text()?
  for value in ["[]", "[@values]"] {
    let output = test.run_xsh(ctx, source + "\npure rejected(values: List[Path]) -> Int { build_nonempty_marker(7, " + value + ") }\n")?
    assert output.status == 2
  }
}
