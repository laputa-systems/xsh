test test_flamegraph [process, error] {
  let output = run.text "xsh" "showcase/flamegraph.xsh" ?
  "<svg" in output
  "Flamegraph" in output
}

test test_flamegraph_rejects_non_integer_sample_counts [fs, process, error] { |ctx|
  for count in ["1.5", "null", "\"wrong\""] {
    let input = test.temp_file(ctx, name: "invalid.folded", contents: bytes.from_text(f"script;leaf ${count}\n"))?
    let captured = run.capture --text "xsh" "showcase/flamegraph.xsh" $input ?
    let rejected = !captured.status.exited_with(0)
    let rejection_message = f"accepted count ${count}"
    assert rejected, rejection_message
    captured.stdout == ""
  }
}
