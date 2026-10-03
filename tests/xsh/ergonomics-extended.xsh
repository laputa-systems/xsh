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

test test_extended_constructor_splices_patterns_and_inferred_values {
  let label = "ship"
  let original = ExtendedOptions(label:)
  var options = original
  options.arguments += ["build", "app", "fast"]
  assert original.arguments.len() == 0
  assert options.label == "ship"
  assert extended_command([@options.arguments, "quiet"]) == ["app", "fast", "quiet"]
  assert extended_valid_name(options.label)
  assert ! extended_valid_name("not-valid")
}

test test_extended_map_iteration_keeps_typed_list_values {
  var commands: Map[List[Str]] = {}
  commands["second"] = ["skip"]
  commands["first"] = ["build", "app", "quiet"]
  let selected = [
    f"${key}:${argument}"
    for {key, value} in commands
    if value is ["build", _, ..]
    for argument in extended_command(value)
  ]
  assert selected == ["first:app", "first:quiet"]
}

test test_extended_error_fallback_binds_nominal_error {
  let result: Result[Str, ExtendedError] = Err(ExtendedError.Missing(message: "absent"))
  let label = result ?? { |failure|
    let is_missing = failure is NotFound
    if is_missing {
      failure.message
    } else {
      "other"
    }
  }
  assert label == "absent"
}

test test_extended_while_pattern_updates_spliced_list_values {
  var commands = [["build", "app"], ["skip"], ["build", "tool", "fast"]]
  var selected = []
  while let [command, ..remaining] = commands {
    commands = remaining
    selected += extended_command(command)
  }

  assert selected == ["app", "ignored", "tool", "fast"]
}

test test_extended_delegated_yield_keeps_guarded_control_and_cleanup { |ctx|
  let output = test.run_script(
    ctx,
    r"""stream child() [io] -> Stream[Int] {
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
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """child-close
parent-close
1
"""
}
