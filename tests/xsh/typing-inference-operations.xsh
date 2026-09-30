test generic_sealed_add_two_domains [error] { |ctx|
  let output = test.run_script(ctx, r"""pure add(left, right) { left + right }
let integer: Int = add(7, 11)
let decimal: Float = add(1.25, 2.5)
print ${integer} ${decimal}
""")?
  assert output.success, output.stderr
  output.status == 0
  output.stdout == "18 3.75\n"
}

test generic_sealed_add_forwarded_requirements [error] { |ctx|
  let output = test.run_script(ctx, r"""pure add(left, right) { left + right }
pure forwarded(left, right) { add(left, right) }
let integer: Int = forwarded(19, 23)
let decimal: Float = forwarded(0.5, 0.75)
print ${integer} ${decimal}
""")?
  assert output.success, output.stderr
  output.status == 0
  output.stdout == "42 1.25\n"
}

test generic_sealed_add_unsupported_domain_rejected [error] { |ctx|
  let output = test.run_script(ctx, r"""pure add(left, right) { left + right }
let _ = add(true, false)
""")?
  output.status == 2
  output.stdout == ""
  assert "check." in output.stderr, output.stderr
  assert "Bool" in output.stderr, output.stderr
  assert "parse." not in output.stderr, output.stderr
}

test generic_sealed_add_mixed_domains_rejected [error] { |ctx|
  let output = test.run_script(ctx, r"""pure add(left, right) { left + right }
let _ = add(1, 2.5)
""")?
  output.status == 2
  output.stdout == ""
  assert "check." in output.stderr, output.stderr
  assert "Int" in output.stderr, output.stderr
  assert "Float" in output.stderr, output.stderr
  assert "parse." not in output.stderr, output.stderr
}

test generic_computed_call_scheme_scope [error] { |ctx|
  let output = test.run_script(ctx, r"""pure identity(value) { value }
pure invoke(callback, value) { callback(value) }
let alias = identity
print ${invoke(alias, 7)} ${invoke(alias, "word")} ${invoke(alias, false)}
""")?
  assert output.success, output.stderr
  output.status == 0
  output.stdout == "7 word false\n"
}

test generic_computed_call_rank_one_callback_rejected [error] { |ctx|
  let output = test.run_script(ctx, r"""pure identity(value) { value }
pure twice(callback) {
  let number: Int = callback(1)
  let text: Str = callback("word")
  number
}
let _ = twice(identity)
""")?
  output.status == 2
  output.stdout == ""
  assert "check." in output.stderr, output.stderr
  assert "Int" in output.stderr, output.stderr
  assert "Str" in output.stderr, output.stderr
  assert "parse." not in output.stderr, output.stderr
}
