test test_full_inference_defer_contributes_effect_and_runs_before_return { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""proc finish(value) {
  defer {
    let _ = time.now()
    print "inferred cleanup"
  }
  value
}
proc integer_call() [time, error] -> Result[Int] {
  print "integer before"
  let value = finish(42)?
  print "integer after"
  Ok(value)
}
proc text_call() [time, error] -> Result[Str] {
  print "text before"
  let value = finish("ready")?
  print "text after"
  Ok(value)
}
proc grounded(value: Int) [time, error] -> Result[Int] {
  defer {
    let _ = time.now()
    print "ground cleanup"
  }
  Ok(value)
}
let integer = integer_call()?
let text = text_call()?
let grounded_value = grounded(7)?
assert integer == 42
assert text == "ready"
assert grounded_value == 7
print f"{integer} {text} {grounded_value}"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "integer before\ninferred cleanup\ninteger after\ntext before\ninferred cleanup\ntext after\nground cleanup\n42 ready 7\n"
}

test test_full_inference_defer_effect_cannot_escape_a_bound_without_time { |ctx|
  test.expect(
    ctx,
    r"""proc finish(value) {
  defer {
    let _ = time.now()
    print "cleanup"
  }
  value
}
proc denied() [error] -> Result[Int] {
  let value = finish(42)?
  Ok(value)
}
""",
    status: 2,
    stderr: ["[check.effect-violation]", "time"],
  )?
}
