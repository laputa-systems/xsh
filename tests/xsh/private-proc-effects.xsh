test test_private_proc_effects_are_inferred_transitively [error] { |ctx|
  let output = test.run_xsh(ctx, """
proc caller() [] -> Int {
  forwarding()
}

proc forwarding() -> Int {
  leaf()
}
proc leaf() -> Int {
  42
}
print \${caller()}
""")?
  output.status == 0
  output.stdout == "42\n"
}


test test_private_proc_effects_reach_recursive_fixed_point [error] { |ctx|
  let output = test.run_xsh(ctx, """
proc caller() [] -> Int { even(4) }
proc even(value: Int) -> Int {
  if value == 0 { return 42 }
  odd(value - 1)
}
proc odd(value: Int) -> Int { even(value - 1) }
print \${caller()}
""")?
  output.status == 0
  output.stdout == "42\n"
}

test test_private_proc_effects_keep_transitive_host_requirements [error] { |ctx|
  let output = test.run_xsh(ctx, """
proc caller() [time] -> Int { forwarding() }
proc forwarding() -> Int { clock() }
proc clock() -> Int {
  let _ = time.now()
  42
}
print \${caller()}
""")?
  output.status == 0
  output.stdout == "42\n"
}

test test_private_proc_effects_enforce_explicit_upper_bounds [error] { |ctx|
  let output = test.run_xsh(ctx, """
proc caller() [] -> Int { deliberate_bound() }
proc deliberate_bound() [time] -> Int { 42 }
""")?
  output.status != 0
  output.stderr.contains("check.effect-violation") == true
  output.stderr.contains("time") == true
}

test test_private_proc_effects_do_not_execute_references [error] { |ctx|
  let output = test.run_xsh(ctx, """
proc caller() [] -> Int {
  let reference = clock
  42
}
proc clock() -> Int {
  let _ = time.now()
  0
}
print \${caller()}
""")?
  output.status == 0
  output.stdout == "42\n"
}

test test_private_proc_effects_preserve_proc_pure_separation [error] { |ctx|
  let output = test.run_xsh(ctx, """
pure caller() -> Int { helper() }
proc helper() -> Int { 42 }
""")?
  output.status != 0
  output.stderr.contains("check.pure-effect") == true
}

test test_private_proc_effects_report_unknown_dynamic_call_chain [error] { |ctx|
  let output = test.run_xsh(ctx, """
proc caller(callback: Proc) [] -> Int { forwarding(callback) }
proc forwarding(callback: Proc) -> Int { dynamic(callback) }
proc dynamic(callback: Proc) -> Int {
  let _ = callback.call()
  42
}
""")?
  output.status != 0
  output.stderr.contains("check.effect-violation") == true
  output.stderr.contains("forwarding -> dynamic -> Proc.call") == true
}

test test_private_proc_effects_include_implicit_assertion_failure [error] { |ctx|
  let output = test.run_xsh(ctx, """
proc caller() [] -> Int { checked() }
proc checked() -> Int {
  true
  42
}
""")?
  output.status != 0
  output.stderr.contains("check.effect-violation") == true
  output.stderr.contains("error") == true
}

test test_private_proc_effects_capture_assertion_error_locally [error] { |ctx|
  let output = test.run_xsh(ctx, """
proc caller() [] -> Int { locally_caught() }
proc locally_caught() -> Int {
  let outcome = try { false; 0 }
  42
}
print \${caller()}
""")?
  output.status == 0
  output.stdout == "42\n"
}

test test_private_proc_effects_capture_keeps_host_requirements [error] { |ctx|
  let output = test.run_xsh(ctx, """
proc caller() [] -> Int { locally_caught() }
proc locally_caught() -> Int {
  let outcome = try {
    let _ = time.now()
    false
  }
  42
}
""")?
  output.status != 0
  output.stderr.contains("check.effect-violation") == true
  output.stderr.contains("time") == true
}

test test_private_proc_effects_distinguish_captured_mutation_from_host_effects [error] { |ctx|
  let output = test.run_xsh(ctx, """
var count = 0
proc caller() [] -> Int { increment() }
proc increment() -> Int {
  count = count + 1
  count
}
print \${caller()}
""")?
  output.status == 0
  output.stdout == "1\n"
}

test test_private_proc_effects_propagate_host_requirement_through_recursion [error] { |ctx|
  let output = test.run_xsh(ctx, """
proc caller() [time] -> Int { second(1) }
proc first(value: Int) -> Int {
  if value == 0 {
    let _ = time.now()
    return 42
  }
  second(value - 1)
}
proc second(value: Int) -> Int { first(value) }
print \${caller()}
""")?
  output.status == 0
  output.stdout == "42\n"
}

test test_private_proc_effects_preserve_public_unrestricted_boundary [error] { |ctx|
  let output = test.run_xsh(ctx, """
##! Public effect boundary.
## Leaves its public effect contract unrestricted.
export proc published() -> Int { 42 }
proc caller() [] -> Int { published() }
""")?
  output.status != 0
  output.stderr.contains("check.effect-violation") == true
  output.stderr.contains("unrestricted") == true
}

test test_private_proc_effects_preserve_stream_unrestricted_boundary [error] { |ctx|
  let output = test.run_xsh(ctx, """
stream values() -> Stream[Int] { yield 42 }
proc caller() [] -> Stream[Int] { values() }
""")?
  output.status != 0
  output.stderr.contains("check.effect-violation") == true
  output.stderr.contains("unrestricted") == true
}

test test_private_proc_effects_capture_transitive_plain_return_failure [error] { |ctx|
  let output = test.run_xsh(ctx, """
proc caller() [] -> Int { locally_caught() }
proc locally_caught() -> Int {
  let outcome = try { failing() }
  if outcome is Err(_) { return 42 }
  0
}
proc failing() -> Int {
  assert false, "captured"
  0
}
print \${caller()}
""")?
  output.status == 0
  output.stdout == "42\n"
}

test test_private_proc_effects_include_explicit_result_propagation [error] { |ctx|
  let output = test.run_xsh(ctx, """
proc caller() [error] -> Int { parsed() }
proc parsed() -> Int { "42".parse_int()? }
print \${caller()}
""")?
  output.status == 0
  output.stdout == "42\n"
}

test test_private_proc_effects_keep_erased_pure_calls_unknown [error] { |ctx|
  let output = test.run_xsh(ctx, """
proc caller(callback: Pure) [] -> Int { forwarding(callback) }
proc forwarding(callback: Pure) -> Int {
  let _ = callback.call()
  42
}
""")?
  output.status != 0
  output.stderr.contains("check.effect-violation") == true
  output.stderr.contains("forwarding -> Pure.call") == true
}

test test_private_proc_effects_include_typed_method_requirements [error] { |ctx|
  let accepted = test.run_xsh(ctx, """
proc caller(file: Path) [fs, error] -> Str { reader(file) }
proc reader(file: Path) -> Str { file.read_text()? }
""")?
  accepted.status == 0
  let rejected = test.run_xsh(ctx, """
proc caller(file: Path) [] -> Str { reader(file) }
proc reader(file: Path) -> Str { file.read_text()? }
""")?
  rejected.status != 0
  rejected.stderr.contains("check.effect-violation") == true
  rejected.stderr.contains("fs") == true
}

test test_private_proc_effects_include_executed_stage_body_requirements [error] { |ctx|
  let accepted = test.run_xsh(ctx, """
proc caller() [time] -> List[Int] { projected() }
proc projected() -> List[Int] {
  [42] |> map { |value|
    let _ = time.now()
    value
  } |> collect
}
""")?
  accepted.status == 0
  let rejected = test.run_xsh(ctx, """
proc caller() [] -> List[Int] { projected() }
proc projected() -> List[Int] {
  [42] |> map { |value|
    let _ = time.now()
    value
  } |> collect
}
""")?
  rejected.status != 0
  rejected.stderr.contains("check.effect-violation") == true
  rejected.stderr.contains("time") == true
}
