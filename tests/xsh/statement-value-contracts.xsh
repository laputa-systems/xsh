test explicit_payloads { |ctx|
  let output = test.run_script(
    ctx,
    r"""pure unit() -> Unit {}
pure boolean() -> Bool { false }
pure early() -> Bool { return false }
pure optional(flag: Bool) -> Int? { if flag { 7 } else { null } }
pure result() -> Result[Int] { 9 }
pure nested() -> Result[Result[Int]] { Ok(Ok(11)) }
let checked: Unit = unit()
print ${boolean()} ${early()} ${optional(false) ?? 0} ${result()?} ${(nested()?)?}
""",
  )?
  assert output.success, output.stderr
  assert output.status == 0
  assert output.stdout == """false false 0 9 11
"""
}

test omitted_private_pure_payloads { |ctx|
  let output = test.run_script(
    ctx,
    r"""pure boolean() { false }
pure early() { return false }
pure optional(flag: Bool) { if flag { 7 } else { null } }
pure result() { Ok(9) }
print ${boolean()} ${early()} ${optional(false) ?? 0} ${result()?}
""",
  )?
  assert output.success, output.stderr
  assert output.status == 0
  assert output.stdout == """false false 0 9
"""
}

test omitted_private_proc_payloads { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc boolean() [] { false }
proc early(flag: Bool) [] { if flag { return 1 }; 2 }
proc branch(flag: Bool) [] { if flag { "yes" } else { "no" } }
proc statement() [io] { print statement }
let unit: Result[Unit] = statement()
print ${boolean()?} ${early(true)?} ${early(false)?} ${branch(false)?} ${unit is Ok(_)}
""",
  )?
  assert output.success, output.stderr
  assert output.status == 0
  assert output.stdout == """statement
false 1 2 no true
"""
}

test non_unit_statement_results_are_rejected { |ctx|
  # Top-level statements, statement blocks, and non-tail body statements all
  # reject a value they would drop.
  for source in [
    """proc data() [] -> Int { 1 }
data()
print done
""",
    """proc inferred() [] { 2 }
inferred()?
print done
""",
    """proc inferred() [] { 2 }
inferred()
print done
""",
    """proc data() [] -> Int { 1 }
if true { data() }
print done
""",
    """proc data() [] -> Int { 1 }
proc caller() [] { data(); print done }
caller()
""",
    """proc text() [] -> Str { "text" }
text()
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert output.status == 2, source
    assert output.stdout == ""
    assert "check.ignored-result" in output.stderr, output.stderr
  }

  let discarded = test.run_script(
    ctx,
    r"""proc data() [io] -> Int { print data; 1 }
proc inferred() [io] { print inferred; 2 }
let _ = data()
let _ = inferred()?
print done
""",
  )?
  assert discarded.success, discarded.stderr
  assert discarded.stdout == """data
inferred
done
"""
}

test final_top_level_int_is_the_exit_status { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc data() [io] -> Int { print data; 7 }
data()
""",
  )?
  assert output.status == 7, output.stderr
  assert output.stdout == """data
"""

  # An entry `main` the last statement does not call runs afterwards and owns
  # the exit status, so that last statement's Int would be dropped.
  let after_main = test.run_script(
    ctx,
    r"""proc main() [] -> Int { 4 }
5
""",
  )?
  assert after_main.status == 2, after_main.stderr
  assert "check.ignored-result" in after_main.stderr, after_main.stderr
  let called_main = test.run_script(
    ctx,
    r"""proc main() [] -> Int { 4 }
main()
""",
  )?
  assert called_main.status == 4, called_main.stderr
}

test unit_consuming_assertions { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc check() [error] -> Unit { assert false }
proc wrapped() [error] -> Result[Unit] { assert false }
let direct: Result[Unit] = try { check() }
let result: Result[Unit] = try { wrapped() }
let tail: Result[Unit] = try { assert false }
print ${direct is Err(_)} ${result is Err(_)} ${tail is Err(_)}
""",
  )?
  assert output.success, output.stderr
  assert output.status == 0
  assert output.stdout == """true true true
"""
}

test unit_consuming_bool_statements_are_rejected { |ctx|
  for source in [
    """proc check() [error] -> Unit { false }
""",
    """proc wrapped() [error] -> Result[Unit] { false }
""",
    """let tail: Result[Unit] = try { false }
""",
    """pure check(value: Int) -> Result[Int] { value > 0; value }
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert output.status == 2, source
    assert "check.bool-statement" in output.stderr, output.stderr
  }
}

test non_tail_concrete_bool { |ctx|
  let output = test.run_script(
    ctx,
    r"""pure check(value: Int) -> Result[Int] { assert value > 0; value }
print ${check(-1) is Err(_)} ${check(3)?}
""",
  )?
  assert output.success, output.stderr
  assert output.status == 0
  assert output.stdout == """true 3
"""
}

test discard_bool_result { |ctx|
  let output = test.run_script(
    ctx,
    r"""error LocalError = Bad(message: Str)
pure unit() -> Unit {}
let _ = false
let failed: Result[Int, LocalError] = Err(LocalError.Bad("discarded"))
let _ = failed
let _ = unit()
print done
""",
  )?
  assert output.success, output.stderr
  assert output.status == 0
  assert output.stdout == """done
"""
}

test discard_initializer_propagation { |ctx|
  let output = test.run_script(
    ctx,
    r"""error LocalError = Bad(message: Str)
proc fail() [] -> Result[Int, LocalError] { Err(LocalError.Bad("kept")) }
proc discarded() [io, error] -> Unit {
  defer { print "cleanup" }
  let _ = fail()?
  print unreachable
}
let result: Result[Unit] = try { discarded() }
print ${result is Err(LocalError.Bad)}
""",
  )?
  assert output.success, output.stderr
  assert output.status == 0
  assert output.stdout == """cleanup
true
"""
}

test statement_result_unit_proc { |ctx|
  let output = test.run_script(
    ctx,
    r"""error LocalError = Bad(message: Str)
proc fail() [] -> Result[Unit, LocalError] { Err(LocalError.Bad("kept")) }
proc caller() [io, error] -> Unit { fail(); print unreachable }
let result: Result[Unit] = try { caller() }
print ${result is Err(LocalError.Bad)}
""",
  )?
  assert output.success, output.stderr
  assert output.status == 0
  assert output.stdout == """true
"""
}

test explicit_data_proc_outward_propagation { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc parsed(value: Str) [error] -> Int { value.parse_int()? }
print ${parsed("7")}
let failure: Result[Int] = try { parsed("bad") }
print ${failure is Err(_)}
""",
  )?
  assert output.success, output.stderr
  assert output.status == 0
  assert output.stdout == """7
true
"""
}

test try_one_layer_and_nesting { |ctx|
  let output = test.run_script(
    ctx,
    r"""error LocalError = Bad(message: Str)
let data: Result[Int, LocalError] = Err(LocalError.Bad("inner"))
let outer = try { data }
let nested = outer?
let captured: Result[Int, LocalError] = try { data? }
print ${nested is Err(LocalError.Bad)} ${captured is Err(LocalError.Bad)}
let no = try { false }?
print $no
""",
  )?
  assert output.success, output.stderr
  assert output.status == 0
  assert output.stdout == """true true
false
"""
}

test retry_lexical_return_and_cleanup { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc escape() [io, error] -> Result[Str] {
  let captured: Result[Int] = retry [] {
    defer { print "cleanup" }
    return Ok("outer")
  }
  Ok("after")
}
print ${escape()?}
""",
  )?
  assert output.success, output.stderr
  assert output.status == 0
  assert output.stdout == """cleanup
outer
"""
}

test try_loop_transfers_and_cleanup { |ctx|
  let output = test.run_script(
    ctx,
    r"""var rounds = 0
while rounds < 3 {
  rounds += 1
  let captured: Result[Unit] = try {
    defer { print "cleanup" }
    continue when rounds < 3
    break
  }
}
print $rounds
""",
  )?
  assert output.success, output.stderr
  assert output.status == 0
  assert output.stdout == """cleanup
cleanup
cleanup
3
"""
}

test callback_bool_and_result_data { |ctx|
  let output = test.run_script(
    ctx,
    r"""let filtered = [1, 2] |> where { false } |> collect()
let values = [1, 2] |> map { |value| Ok(value) } |> collect()
print ${filtered.len()} ${values[0]?} ${values[1]?}
""",
  )?
  assert output.success, output.stderr
  assert output.status == 0
  assert output.stdout == """0 1 2
"""
}

test default_order_and_outer_scope { |ctx|
  let output = test.run_script(
    ctx,
    r"""let seed = 6
proc mark(label: Str, value: Int) [io] -> Int { print $label; value }
proc combine(left: Int = mark("left", seed), right: Int = mark("right", 2)) [io] -> Int { left + right }
print ${combine()}
print ${combine(right: mark("supplied", 9))}
pure outer(seed: Int = seed) -> Int { seed }
print ${outer()}
""",
  )?
  assert output.success, output.stderr
  assert output.status == 0
  assert output.stdout == """left
right
8
supplied
left
15
6
"""
}

test producer_lazy_default_delegation_cancellation { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc value(label: Str) [time] -> Int { let _ = time.now(); print $label; 7 }
stream child(item: Int = value("default")) [time, error] -> Stream[Int] {
  defer { print "child-close" }
  yield item
  print unreachable
  yield 8
}
stream parent() [time, error] -> Stream[Int] {
  defer { print "parent-close" }
  yield @child()
}
let _ = child()
let source = parent()
let _ = child(value("supplied"))
print created
for item in source { print $item; break }
print after
""",
  )?
  assert output.success, output.stderr
  assert output.status == 0
  assert output.stdout == """supplied
created
default
7
child-close
parent-close
after
"""
}

test status_as_data { |ctx|
  let output = test.run_script(
    ctx,
    r"""let status = run.status --accept=[1] false
print ${status.exit_code()?}
let _ = status
print done
""",
  )?
  assert output.success, output.stderr
  assert output.status == 0
  assert output.stdout == """1
done
"""
}

test explicit_nested_error { |ctx|
  let output = test.run_script(
    ctx,
    r"""error LocalError = Bad(message: Str)
pure nested() -> Result[Result[Int, LocalError]] {
  let data: Result[Int, LocalError] = Err(LocalError.Bad("inner"))
  Ok(data)
}
print ${nested()? is Err(LocalError.Bad)}
""",
  )?
  assert output.success, output.stderr
  assert output.status == 0
  assert output.stdout == """true
"""
}

test empty_proc_success { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc empty() [] {}
let value: Result[Unit] = empty()
print ${value is Ok(_)}
""",
  )?
  assert output.success, output.stderr
  assert output.status == 0
  assert output.stdout == """true
"""
}

test explicit_propagation_failure { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc parsed(value: Str) [error] -> Int { value.parse_int()? }
print ${parsed("bad")}
print unreachable
""",
  )?
  assert output.status == 3
  assert output.stdout == ""
  assert "parse-int" in output.stderr, output.stderr
}

test annotated_value_result_statement_rejected { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc leaf() [] -> Result[Int] { Ok(1) }
proc bad() [error] -> Int { leaf(); 2 }
print ${bad()}
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "Result" in output.stderr, output.stderr
}

test annotated_effect_bound_rejected { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc bad() [] -> Unit { let _ = time.now() }
bad()
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.effect-violation" in output.stderr, output.stderr
}

test annotated_default_parameter_scope_rejected { |ctx|
  let output = test.run_script(
    ctx,
    r"""pure bad(left: Int = 1, right: Int = left) -> Int { right }
print ${bad()}
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "left" in output.stderr, output.stderr
}

test try_error_only_rejected { |ctx|
  let output = test.run_script(
    ctx,
    r"""error LocalError = Bad(message: Str)
let result = try { Err(LocalError.Bad("unknown"))? }
let _ = result
""",
  )?
  assert output.status == 2
  assert output.stdout == ""
  assert "check.try-success-type" in output.stderr, output.stderr
}

test unreachable_does_not_add_unit { |ctx|
  let output = test.run_script(
    ctx,
    r"""pure early() { return 7; return "unreachable" }
pure branch(flag: Bool) -> Int { if flag { return 8 } else { 9 } }
print ${early()} ${branch(true)} ${branch(false)}
""",
  )?
  assert output.success, output.stderr
  assert output.status == 0
  assert output.stdout == """7 8 9
"""
}

test discard_keeps_control_transfer { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc early() [io] -> Int {
  defer { print "cleanup" }
  let _ = { return 7; 9 }
  11
}
print ${early()}
""",
  )?
  assert output.success, output.stderr
  assert output.status == 0
  assert output.stdout == """cleanup
7
"""
}
