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
    assert "err[check." in output.stderr
  }
}

test test_full_inference_pipeline_source_views_reject_nested_result_layers { |ctx|
  for payload in ["List[Int]", "Stream[Int]"] {
    let output = test.run_xsh(
      ctx,
      f"pure collect_values(values: Result[Result[{payload}]]) -> List[Int] {{ values |> collect }}\n",
    )?
    assert output.status == 2
    assert "err[check.stream-input]" in output.stderr
  }
}

test test_full_inference_pipeline_source_views_preserve_explicit_peeling_and_items { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure collect_values(values: Result[List[Result[Int, Str]]]) -> List[Result[Int, Str]] { values |> collect }
pure collect_nested(values: Result[Result[List[Int]]]) -> Result[List[Int]] { values? |> collect }
stream source_values() [] -> Stream[Int] { yield 1; yield 2 }
proc collect_nested_stream(values: Result[Result[Stream[Int]]]) [error] -> Result[List[Int]] { values? |> collect }
let values: Result[List[Result[Int, Str]]] = Ok([Err("opaque")])
let observed = collect_values(values)
assert match observed[0] { Ok(_) => false, Err(message) => message == "opaque" }
let nested: Result[Result[List[Int]]] = Ok(Ok([1, 2]))
assert collect_nested(nested)? == [1, 2]
let nested_stream: Result[Result[Stream[Int]]] = Ok(Ok(source_values()))
assert collect_nested_stream(nested_stream)? == [1, 2]
print "views preserved"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "views preserved\n"
}
