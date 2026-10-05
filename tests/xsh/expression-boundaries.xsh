# Where an expression ends does not depend on the construct around it: a
# delimiter resets what an enclosing pipeline stage, command argument, or
# `match` arm changed, a typed command argument ends at its line, and the `}`
# of a `${...}` word part does not end an `env` command's line.

test test_a_pipeline_nests_inside_a_stage_expression {
  let rows = [[1, 2], [3]]
  let widths = rows |> map .len() + (rows |> count())
  assert widths == [4, 3]
  let padded = rows |> map [.len(), [7, 8] |> count()]
  assert padded == [[2, 2], [1, 2]]
}

test test_a_typed_command_argument_ends_at_its_line { |ctx|
  let output = test.expect(
    ctx,
    r"""let out = run.text (
  printf "%s" p"line"
  < /dev/null
)?
print $out
""",
    status: 0,
  )?
  assert output.stdout == "line\n"
}

test test_an_env_value_may_interpolate { |ctx|
  let output = test.expect(
    ctx,
    r"""let value = "interpolated"
env XSH_BOUNDARY_VALUE=${value} {
  run printenv XSH_BOUNDARY_VALUE
}
""",
    status: 0,
  )?
  assert output.stdout == "interpolated\n"
}

test test_a_comma_inside_an_arm_block_is_a_command_word { |ctx|
  let output = test.expect(
    ctx,
    r"""let mode = "words"
match mode {
  "words" => if true { print a, b }
  _ => print other
}
""",
    status: 0,
  )?
  assert output.stdout == "a, b\n"
}

test test_a_spaced_try_ends_a_nested_argument_in_a_command { |ctx|
  let output = test.expect(
    ctx,
    r"""proc greeting() [] -> Result[Str] { Ok("hello") }
pure shout(text: Str) -> Str { text + "!" }
print shout(greeting() ?)
""",
    status: 0,
  )?
  assert output.stdout == "hello!\n"
}

test test_a_run_form_heads_a_pipeline_in_any_position { |ctx|
  let count = (run.stream --text printf "a\nb\n")? |> count()
  assert count == 2
  let words = (run.text printf "x y"?).split(" ")
  assert words == ["x", "y"]
  assert (run.stream --text printf "c\n"? |> collect()) == ["c"]
  let output = test.expect(
    ctx,
    r"""run.stream --text printf "a\nb\n" ? |> each { |line| print $line }
""",
    status: 0,
  )?
  assert output.stdout == "a\nb\n"
}

test test_a_map_comprehension_has_exactly_one_entry { |ctx|
  for source in [
    "let m = {a: 1, k: 2 for k in [\"x\"]}\n",
    "let base = {a: 1}\nlet m = {...base, k: 2 for k in [\"x\"]}\n",
    "let m = {[\"a\"]: 1, [k]: 2 for k in [\"x\"]}\n",
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, source
    assert "parse.map-comprehension-entries" in output.stderr, output.stderr
  }
}
