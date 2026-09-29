type ExtendedOptions = {
  label: Str = "build",
  arguments: List[Str] = [],
}

error ExtendedError = Missing(message: Str) : NotFound

pure extended_command(command: List[Str]) {
  if let ["build", target, ..rest] = command {
    [target, @rest]
  } else {
    ["ignored"]
  }
}

pure extended_valid_name(name: Str) {
  rx"^[a-z]+$".matches(name)
}

proc test_extended_constructor_splices_patterns_and_inferred_values() [error] {
  let label = "ship"
  let original = ExtendedOptions(label:)
  var options = original
  options.arguments += ["build", "app", "fast"]
  test.eq(original.arguments, [])?
  test.eq(options.label, "ship")?
  test.eq(extended_command([@options.arguments, "quiet"]), ["app", "fast", "quiet"])?
  test.eq(extended_valid_name(options.label), true)?
  test.eq(extended_valid_name("not-valid"), false)?
}

proc test_extended_map_iteration_keeps_typed_list_values() [error] {
  var commands: Map[List[Str]] = {}
  commands["second"] = ["skip"]
  commands["first"] = ["build", "app", "quiet"]
  let selected = [
    f"${key}:${argument}"
    for {key, value} in commands
    if value is ["build", _, ..]
    for argument in extended_command(value)
  ]
  test.eq(selected, ["first:app", "first:quiet"])?
}

proc test_extended_error_fallback_binds_nominal_error() [error] {
  let result: Result[Str, ExtendedError] = Err(ExtendedError.Missing(message: "absent"))
  let label = result ?? { |failure|
    let is_missing = failure is NotFound
    if is_missing { failure.message } else { "other" }
  }
  test.eq(label, "absent")?
}

proc test_extended_while_pattern_updates_spliced_list_values() [error] {
  var commands = [["build", "app"], ["skip"], ["build", "tool", "fast"]]
  var selected: List[Str] = []
  while let [command, ..remaining] = commands {
    commands = remaining
    selected += extended_command(command)
  }
  test.eq(selected, ["app", "ignored", "tool", "fast"])?
}

proc test_extended_delegated_yield_keeps_guarded_control_and_cleanup(ctx: TestContext) [error] {
  let output = test.run_script(ctx, r"""stream child() [io] -> Stream[Int] {
  defer { print "child-close" }
  yield @[1, 2, 3]
}
stream parent() [io] -> Stream[Int] {
  defer { print "parent-close" }
  yield @child()
  yield 4 when true
}
let first = parent() |> take(1) |> collect
print ${first[0]}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "child-close\nparent-close\n1\n")?
}
