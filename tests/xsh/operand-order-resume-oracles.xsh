test test_path_mkdir_and_remove_evaluate_paths_before_optional_values { |ctx|
  let root = test.temp_dir(ctx, name: "operand-order-path-options")?
  let output = test.expect(
    ctx,
    r"""proc mark_path(label: Str, value: Path) [io] -> Path {
  print $label
  return value
}
proc mark_bool(label: Str, value: Bool) [io] -> Bool {
  print $label
  return value
}
let root = Path(args[0])
let directory = fp"{root}/created"
mark_path("mkdir-path", directory).mkdir(parents: mark_bool("mkdir-parents", true))
assert directory.exists()?
let file = fp"{root}/remove-me"
file.write("payload")
mark_path("remove-path", file).remove(missing_ok: mark_bool("remove-missing", false))
assert ! file.exists()?
""",
    status: 0,
    args: [root],
  )?
  assert output.stdout == """mkdir-path
mkdir-parents
remove-path
remove-missing
""", output.stdout
}

test test_module_method_named_arguments_keep_written_order { |ctx|
  let output = test.expect(
    ctx,
    r"""proc mark_path(label: Str, value: Path) [io] -> Path {
  print $label
  return value
}
let root = fs.tempdir()?
root.write(p"data", "payload")?
root.symlink(
  path: mark_path("module-path", p"link"),
  target: mark_path("module-target", p"data"),
)?
assert root.readlink(p"link")? == p"data"
""",
    status: 0,
  )?
  assert output.stdout == """module-path
module-target
""", output.stdout
}

test test_pipeline_callbacks_run_across_stage_boundaries_in_order { |ctx|
  let output = test.expect(
    ctx,
    r"""stream numbers() [] -> Stream[Int] {
  print "pipeline-pull-1"
  yield 1
  print "pipeline-pull-2"
  yield 2
}
proc step(label: Str, value: Int) [io] -> Int {
  print f"{label}:{value}"
  return value + 10
}
let values = numbers()
  |> map { |value| step("pipeline-first", value) }
  |> map { |value| step("pipeline-second", value) }
  |> collect()
assert values == [21, 22]
""",
    status: 0,
  )?
  assert output.stdout == """pipeline-pull-1
pipeline-first:1
pipeline-second:11
pipeline-pull-2
pipeline-first:2
pipeline-second:12
""", output.stdout
}

test test_self_call_runs_multiple_named_arguments_in_written_order { |ctx|
  let output = test.expect(
    ctx,
    r"""proc mark_int(label: Str, value: Int) [io] -> Int {
  print $label
  return value
}
proc descend(depth: Int, total: Int) [io] -> Int {
  print f"self-body:{depth}:{total}"
  if depth == 0 {
    return total
  }
  return descend(
    total: mark_int("self-total", total + 1),
    depth: mark_int("self-depth", depth - 1),
  )
}
assert descend(2, 0) == 2
""",
    status: 0,
  )?
  assert output.stdout == """self-body:2:0
self-total
self-depth
self-body:1:1
self-total
self-depth
self-body:0:2
""", output.stdout
}
