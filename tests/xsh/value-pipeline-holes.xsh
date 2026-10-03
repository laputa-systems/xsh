pure pipeline_join(prefix: Str, value: Str, suffix: Str) -> Str { prefix + value + suffix }
pure pipeline_number(value: Int) -> Int { value + 1 }

test test_value_pipeline_holes_choose_positional_and_named_arguments { |ctx|
  ("middle" |> pipeline_join("[", _, "]")) == ("[middle]")
  ("middle" |> pipeline_join("[", value: _, suffix: "]")) == ("[middle]")
  ("middle" |> pipeline_join("[", value: _, ...{suffix: "]"})) == ("[middle]")
  (2 |> pipeline_number((_))) == (3)
  let cast = test.run_script(ctx, r"""("x" |> Path(_)) == p"x"
""")?
  let {success: cast_success, stderr: cast_message, ..} = cast
  assert cast_success, cast_message
  ([1, 2] |> map { |_| 0 } |> collect()) == ([0, 0])
  (" middle " |> trim() |> pipeline_join("[", _, "]")) == ("[middle]")
}

test test_value_pipeline_holes_evaluate_input_before_remaining_arguments { |ctx|
  let result = test.run_script(ctx, """
proc mark(label: Str) [] -> Str { print $label; label }
pure join(first: Str, second: Str, third: Str) -> Str { first + second + third }
let result = mark("input") |> join(mark("first"), _, mark("last"))
print $result
""", [], {}, b"", "pipeline-hole-order.xsh")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = result
    assert assertion_condition, assertion_message
  }
  (result.stdout) == ("input\nfirst\nlast\nfirstinputlast\n")
}

test test_value_pipeline_holes_reject_other_placeholder_contexts { |ctx|
  for source in [
    "let value = _\n",
    "pure read(_: Int) -> Int { _ }\n",
    "pure read(_: Int) -> Record { {_} }\n",
    "pure join(left: Int, right: Int) -> Int { left + right }\nlet value = 1 |> join(_, _)\n",
    "pure join(left: Int, right: Int) -> Int { left + right }\nlet value = 1 |> join(_ + 1, 2)\n",
    "pure number(value: Int) -> Int { value }\nlet value = 1 |> number(number(_))\n",
    "pure number(value: Int) -> Int { value }\nlet value = 1 |> number(...{value: _})\n",
    "pure number(value: Int) -> Int { value }\nlet value = 1 |> number(if true { _ } else { 0 })\n",
    "let value = [1] |> collect(_)\n",
    "pure number(value: Int) -> Int { value }\nlet value = \"1\".parse_int() |> number(_)\n",
  ] {
    let result = test.run_script(ctx, source, [], {}, b"", "pipeline-hole-rejected.xsh")?
    {
      let assertion_condition = !result.success
      let assertion_message = source
      assert assertion_condition, assertion_message
    }
  }
}

test test_value_pipeline_holes_preserve_optional_argument_laziness { |ctx|
  let result = test.run_script(ctx, """
proc input() [] -> Str { print "input"; "a" }
proc receiver() [] -> Str? { print "receiver"; null }
proc other() [] -> Str { print "other"; "b" }
let selected = input() |> receiver()?.replace(_, other())
print (selected ?? "missing")
""", [], {}, b"", "pipeline-hole-optional.xsh")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = result
    assert assertion_condition, assertion_message
  }
  (result.stdout) == ("input\nreceiver\nmissing\n")
}

test test_value_pipeline_holes_preserve_explicit_result_boundaries { |ctx|
  let result = test.run_script(ctx, """
pure parsed(value: Str) -> Result[Int] { value.parse_int() }
pure trim(value: Str) -> Str { "function:" + value }
let parsed_number = "3" |> parsed(_)?
let explicit = " x " |> trim(_)
let implicit = " x " |> trim()
print $parsed_number $explicit $implicit
""", [], {}, b"", "pipeline-hole-results.xsh")?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = result
    assert assertion_condition, assertion_message
  }
  (result.stdout) == ("3 function: x  x\n")
  let failed = test.run_script(ctx, """
error Stop = Stopped(message: Str)
proc input() [] -> Result[Str, Stop] { print "input"; Err(Stop.Stopped(message: "stop")) }
proc receiver() [] -> Str { print "receiver"; "a" }
proc other() [] -> Str { print "other"; "b" }
let selected = input()? |> receiver().replace(_, other())
print $selected
""", [], {}, b"", "pipeline-hole-input-error.xsh")?
  {
    let assertion_condition = !failed.success
    let assertion_message = failed.stderr
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "stop" in failed.stderr
    let assertion_message = failed.stderr
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = failed.stdout == "input\n"
    let assertion_message = failed.stderr
    assert assertion_condition, assertion_message
  }
}
