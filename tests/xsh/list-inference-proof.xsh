test test_inferred_lists_preserve_opaque_rows_before_projection { |ctx|
  let source = p"tests/fixtures/runtime/list-inference-opaque-rows.xsh".read_text()?
  let output = test.expect(ctx, source, status: 0)?
  assert output.stdout == "3 3\n"
}

test test_inferred_lists_do_not_invent_unvalidated_any_fields { |ctx|
  let prefix = p"tests/fixtures/runtime/list-inference-opaque-rows.xsh".read_text()?
  for body in [
    r"""proc invalid(input: Any) [] -> Int {
  var tasks = []
  tasks = tasks.push(input)
  let named = names([task.name for task in tasks])
  count(tasks)
}
""",
    r"""proc invalid(input: Any) [] -> Int {
  var tasks = []
  tasks = tasks.push({name: "unvalidated", argv: input})
  let named = names([task.name for task in tasks])
  count(tasks)
}
""",
  ] {
    let output = test.run_script(ctx, prefix + body)?
    assert output.status == 2
    assert output.stdout == ""
  }
}
