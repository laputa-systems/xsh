test test_flamegraph [process, error] {
  let output = run.text "xsh" "showcase/flamegraph.xsh" ?
  test.contains(output, "<svg")?
  test.contains(output, "Flamegraph")?
}

test test_flamegraph_rejects_non_integer_sample_counts [fs, process, error] { |ctx|
  for count in ["1.5", "null", "\"wrong\""] {
    let input = test.temp_file(ctx, name: "invalid.folded", contents: bytes.from_text(f"script;leaf ${count}\n"))?
    let captured = run.capture --text "xsh" "showcase/flamegraph.xsh" $input ?
    test.ok(!captured.status.exited_with(0), f"accepted count ${count}")?
    test.eq(captured.stdout, "")?
  }
}
