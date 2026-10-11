test test_indexed_pattern_decoder_falls_through_for_dynamic_nonmatches { |ctx|
  let output = test.run_script(
    ctx,
    r"""error DecoderPatternError {
    Missing(detail: Str) : NotFound
    Rejected(message: Str) : InvalidData
}
enum DecoderPatternChoice { Selected(Int), Empty }

pure dynamic_text() -> Any { "ordinary text" }

proc witness() [error] {
  let failure: DecoderPatternError = .Missing(detail: "absent")
  let direct_facet = match failure {
    is NotFound => "not found"
    _ => "wrong facet"
  }
  assert direct_facet == "not found"

  let choice = Selected(17)
  let selected = match choice {
    Selected(number) => number
    _ => 0
  }
  assert selected == 17

  let arbitrary = dynamic_text()
  let facet_mismatch = match arbitrary {
    is NotFound => false
    _ => true
  }
  let tag_mismatch = match arbitrary {
    Selected(number) => number
    _ => 0
  }
  assert facet_mismatch
  assert tag_mismatch == 0
}

witness()
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == ""
}
