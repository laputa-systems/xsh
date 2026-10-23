test test_full_inference_pipeline_inputs_keep_independent_container_domains { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""stream values() [] -> Stream[Int] { yield 1; yield 2 }
pure collect_values(source) { source |> collect }
pure text_source() -> Result[List[Str], Str] { Ok(["a", "b"]) }
assert collect_values([1, 2]) == [1, 2]
assert collect_values({2, 1}) == [1, 2]
assert collect_values(values()) == [1, 2]
assert collect_values(text_source()) == ["a", "b"]
let wrapped: Result[Stream[Int]] = Ok(values())
assert collect_values(wrapped) == [1, 2]
print "domains preserved"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "domains preserved\n"
}

test test_full_inference_pipeline_inputs_reject_other_iteration_domains { |ctx|
  for source_type in ["Map[Str, Int]", "Str", "Bytes", "Any"] {
    let output = test.run_xsh(
      ctx,
      f"pure collect_values(source) {{ source |> collect }}\npure rejected(source: {source_type}) {{ collect_values(source) }}\n",
    )?
    assert output.status == 2
    assert output.stderr.contains("error[check.")
  }
}
