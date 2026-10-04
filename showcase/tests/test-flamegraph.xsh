test test_flamegraph {
  let output = run.text "xsh" "showcase/flamegraph.xsh" ?
  assert "<svg" in output
  assert "Flamegraph" in output
}

test test_flamegraph_rejects_non_integer_sample_counts { |ctx|
  for count in ["1.5", "null", "\"wrong\""] {
    let folded = f"""
      script;leaf {count}

      """
    let input = test.temp_file(ctx, name: "invalid.folded", contents: bytes.from_text(folded))?
    let captured = run.capture --text "xsh" "showcase/flamegraph.xsh" $input ?
    let rejected = ! captured.status.exited_with(0)
    let rejection_message = f"accepted count {count}"
    assert rejected, rejection_message
    assert captured.stdout == ""
  }
}
