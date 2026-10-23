test test_full_inference_collect_expressions_keep_items_and_yield_targets { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""pure gather(value) { collect { yield value; yield @collect { yield value }; yield value } }
pure delegated(values) { collect { yield @values } }
pure stopped(value, stop: Bool) { collect { yield value; if stop { return [value] }; yield value } }
assert gather(7) == [7, 7, 7]
assert gather("kept") == ["kept", "kept", "kept"]
assert delegated([1, 2]) == [1, 2]
assert delegated(["a", "b"]) == ["a", "b"]
assert stopped(7, true) == [7]
assert stopped("kept", false) == ["kept", "kept"]
let data: Result[Result[Int, Str], Str] = Ok(Err("opaque"))
let items = gather(data)
assert match items[0] { Ok(inner) => match inner { Ok(_) => false, Err(message) => message == "opaque" }, Err(_) => false }
print "collect preserved"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "collect preserved\n"
}

test test_full_inference_collect_expressions_reject_stream_items_and_non_list_delegation { |ctx|
  test.expect(
    ctx,
    "stream values() [] -> Stream[Int] { yield 1 }\npure gather(value) { collect { yield value } }\nlet _ = gather(values())\n",
    status: 2,
    stderr: ["[check.yield-stream]"],
  )?
  for source_type in ["Stream[Int]", "Map[Str, Int]", "Set[Int]", "Str", "Bytes", "Any", "Result[List[Int]]"] {
    let output = test.run_xsh(
      ctx,
      f"pure gather(values) {{ collect {{ yield @values }} }}\npure rejected(value: {source_type}) {{ gather(value) }}\n",
    )?
    assert output.status == 2
    assert "err[check." in output.stderr
  }
}
