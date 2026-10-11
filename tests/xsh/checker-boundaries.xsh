stream first_rows() [] -> Stream[Int] {
  yield 1
}

test test_checker_accepts_top_level_producers_beside_tests {
  assert first_rows().collect() == [1]
  assert later_rows().collect() == [2]
}

stream later_rows() [] -> Stream[Int] {
  yield 2
}

test test_checker_test_files_keep_rejecting_top_level_execution { |ctx|
  let root = test.temp_dir(ctx, name: "top-level")?
  let directory = fp"{root}/tests"
  directory.mkdir()
  let fixture = fp"{directory}/rejected.xsh"
  for source in ["print \"executed\"", "var count = 0\ncount = 1", "for item in [1] {}"] {
    fixture.write(f"{source}\ntest test_case {{ assert true }}\n")
    cd (root) {
      let rejected = run.capture --text "xsht" test
      assert rejected.status.exited_with(1), rejected.stderr
      assert "err[check.test-top-level]" in rejected.stdout, rejected.stdout
      assert "executed\n" not in rejected.stdout, rejected.stdout
    }
  }
}

test test_checker_stage_yields_do_not_suspend_an_enclosing_producer { |ctx|
  for body in [
    "let _ = [1] |> map { |item| yield item; item }",
    "let _ = [1] |> each { |item| yield item }",
    "let _ = [1] |> fold(0) { |sum, item| yield item; sum + item }",
    "let _ = [1] |> map { |item| yield @[item]; item }",
    "let _ = [1] |> map { |item| let _ = [item] |> map { |inner| yield inner; inner }; item }",
  ] {
    let source = f"stream rows() [] -> Stream[Int] {{\n  {body}\n  yield 2\n}}\n"
    let rejected = test.expect(ctx, source, status: 2, stderr: ["err[check.yield]"])?
    assert "stream producers" in rejected.stderr, rejected.stderr
  }
}

test test_checker_stage_yields_do_not_append_to_an_enclosing_collect { |ctx|
  test.expect(
    ctx,
    "let values = collect { let _ = [1] |> map { |item| yield item; item }; yield 2 }\n",
    status: 2,
    stderr: ["err[check.yield]"],
  )?
}

test test_checker_stage_loop_control_cannot_target_an_enclosing_loop { |ctx|
  for transfer in ["break", "continue"] {
    let source = f"for item in [1] {{\n  let _ = [item] |> each {{ {transfer} }}\n}}\n"
    test.expect(ctx, source, status: 2, stderr: ["err[check.loop-control]"])?
  }
}

test test_checker_restores_producer_collect_and_loop_contexts_after_callbacks { |ctx|
  let accepted = test.expect(
    ctx,
    r"""stream rows() [] -> Stream[Int] {
  for item in [1, 2] {
    let doubled = [item] |> map { |value| value * 2 }
    let nested = collect {
      let tripled = [item] |> map { |value| value * 3 }
      yield tripled[0]
    }
    yield doubled[0]
    yield @nested
    break
  }
  yield 9
}

assert rows().collect() == [2, 3, 9]
let outer = collect {
  for item in [4, 5] {
    let nested = [item] |> map { |value| collect { yield value; yield value + 1 } }
    yield @nested[0]
    continue
  }
}
assert outer == [4, 5, 5, 6]
""",
    status: 0,
  )?
  assert accepted.stdout == "", accepted.stdout
}

test test_checker_deferred_context_keeps_its_item_and_restores_the_producer { |ctx|
  let accepted = test.expect(
    ctx,
    r"""stream rows() [] -> Stream[Int] {
  defer {
    for item in [1] {
      let _ = [item] |> each { defer { print f"{.}" }; print f"{.}" }
      break
    }
  }
  yield 2
}
assert rows().collect() == [2]
""",
    status: 0,
  )?
  assert accepted.stdout == "1\n1\n", accepted.stdout
}

test test_checker_callback_errors_keep_the_lexical_error_boundary { |ctx|
  test.expect(
    ctx,
    r"""error RowError = Missing(row: Int)
pure row(item: Int) -> Result[Int, RowError] {
  Err(.Missing(row: item))
}
let result: Result[List[Int], RowError] = try {
  [1] |> map { |item| row(item)? }
}
assert result is Err(_)
""",
    status: 0,
  )?
}

test test_checker_producer_callbacks_keep_inferred_error_propagation { |ctx|
  test.expect(
    ctx,
    r"""pure row(item: Int) -> Result[Int] {
  Ok(item)
}
stream rows() -> Stream[Int] {
  let values = [1] |> map { |item| row(item)? }
  yield values[0]
}
assert rows().collect() == [1]
""",
    status: 0,
  )?
}

test test_checker_callbacks_keep_enclosing_negative_effect_bounds { |ctx|
  test.expect(
    ctx,
    r"""without process {
  let _ = [1] |> each { let _ = run.status true }
}
""",
    status: 2,
    stderr: ["err[check.effect-violation]"],
  )?
}
