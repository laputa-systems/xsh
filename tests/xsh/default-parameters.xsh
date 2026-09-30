test test_default_parameters_infer_const_projection_primitive_and_constructor_types [error] { |ctx|
  let output = test.run_script(ctx, r"""const defaults = {jobs: 4, timeout: 30s}
pure jobs(value = defaults.jobs + 1) -> Int { value }
pure timeout(value = defaults.timeout) -> Duration { value }
pure default_path(value = Path("config")) -> Path { value }
print ${jobs()} ${jobs(9)} ${timeout() == 30s} ${default_path().display()}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "5 9 true config\n")?
}

test test_default_parameters_keep_already_checked_call_defaults [error] { |ctx|
  let output = test.run_script(ctx, r"""pure next() -> Int { 4 }
pure explicit(jobs: Int = next()) -> Int { jobs }
pure inferred(jobs = next()) -> Int { jobs }
print ${explicit()} ${inferred()} ${inferred(9)}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "4 4 9\n")?
}

test test_default_parameters_keep_stream_defaults_lazy_and_supplied_arguments_eager [error] { |ctx|
  let output = test.run_script(ctx, r"""proc value(label: Str) [io] -> Int {
  print $label
  7
}
stream explicit(item: Int = value("explicit default")) [io] -> Stream[Int] {
  print body
  yield item
}
stream inferred(item = value("inferred default")) [io] -> Stream[Int] {
  print body
  yield item
}
let _ = explicit()
let _ = inferred()
let first = explicit()
let second = inferred()
let supplied = inferred(value("supplied"))
print before
for item in first { print $item }
for item in second { print $item }
for item in supplied { print $item }
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "supplied\nbefore\nexplicit default\nbody\n7\ninferred default\nbody\n7\nbody\n7\n")?
}

test test_default_parameters_resolve_imported_constants [fs, error] { |ctx|
  let root = test.temp_dir(ctx, name: "default-module")?
  fp"${root}/config.xsh".write_atomic(r"""##! Build defaults.
## The default worker count.
export const settings = {jobs: 6}
""")?
  let output = test.run_script(ctx, r"""use config as c
pure choose(jobs = c.settings.jobs) -> Int { jobs }
print ${choose()}
""", [], {XSH_MODULE_PATH: root.display()})?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "6\n")?
}

test test_default_parameters_keep_omitted_order_lazy_effects_and_cleanup [error] { |ctx|
  let output = test.run_script(ctx, r"""proc mark(name: Str) [io] { print $name }
proc value(name: Str, count: Int) [io, error] -> Int {
  defer mark("default cleanup")
  print $name
  count
}
proc choose(left = value("left", 1), right = value("right", 2)) [io, error] -> Int { left + right }
print ${choose()}
print ${choose(right: value("supplied", 9))}
print ${choose(8, 9)}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "left\ndefault cleanup\nright\ndefault cleanup\n3\nsupplied\ndefault cleanup\nleft\ndefault cleanup\n10\n17\n")?
}

test test_default_parameters_require_anchors_and_keep_parameter_scope [error] { |ctx|
  for source in [
    "pure choose(value = null) -> Int { 1 }\n",
    "pure choose(value = []) -> Int { 1 }\n",
    "pure choose(value = {}) -> Int { 1 }\n",
    "pure choose(value = map.empty()) -> Int { 1 }\n",
    "pure choose(left: Int = 1, right = left) -> Int { right }\n",
    "pure choose(value) -> Int { 1 }\n",
    "pure choose(...values = [1]) -> Int { 1 }\n",
  ] {
    let output = test.run_script(ctx, source)?
    test.ok(!output.success, output.stdout)?
  }
}

test test_default_parameters_use_outer_names_and_checked_dependency_signatures [error] { |ctx|
  let output = test.run_script(ctx, r"""let count = 5
pure dependent(value = target()) -> Int { value }
pure target() { 7 }
pure shadow(count = count) -> Int { count }
pure anchored(value: Int = dependent()) -> Int { value }
print ${dependent()} ${shadow()} ${shadow(9)} ${anchored()}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "7 5 9 7\n")?
}

test test_default_parameters_keep_error_propagation_and_skip_later_defaults [error] { |ctx|
  let output = test.run_script(ctx, r"""proc mark(message: Str) [io] { print $message }
error DefaultError = invalid(message: Str)
proc failing() [io, error] -> Result[Int] {
  defer mark("cleanup")
  Err(DefaultError.invalid(message: "default failed"))
}
proc later() [io] -> Int { print "later"; 2 }
proc choose(value = failing()?, other = later()) [io, error] -> Result[Int] { Ok(value + other) }
let captured = try { choose()? }
print ${match captured { Ok(_) => false, Err(_) => true }}
print ${choose(8, 9)?}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "cleanup\ntrue\n17\n")?
}

test test_default_parameters_keep_explicit_anchors_across_declaration_cycles [error] { |ctx|
  let output = test.run_script(ctx, r"""pure first(value = second(3)) -> Int { value }
pure second(value: Int = first(4)) -> Int { value }
print ${first()} ${second()}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "3 4\n")?
  let rejected = test.run_script(ctx, r"""pure first(value = second()) { value }
pure second(value = first()) { value }
""")?
  test.ok(!rejected.success, rejected.stdout)?
}

test test_default_parameters_use_the_callable_return_target [error] { |ctx|
  let output = test.run_script(ctx, r"""pure explicit(value: Int = if true { return 7 } else { 4 }) -> Int { value + 1 }
pure inferred(value = if true { return 7 } else { 4 }) -> Int { value + 1 }
print ${explicit()} ${inferred()} ${inferred(9)}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "7 7 10\n")?
}

test test_default_parameters_nested_calls_use_heap_frames [error] { |ctx|
  var source: Str = "pure step_0() -> Int { 7 }\n"
  for index in range(1, 1201) {
    source = source + f"pure step_${index}(value = step_${index - 1}()) -> Int { value }\n"
  }
  source = source + r"print ${step_1200()}" + "\n"
  let output = test.run_script(ctx, source)?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "7\n")?
}

test test_default_parameters_keep_omitted_effects_in_callable_contracts [error] { |ctx|
  let output = test.run_script(ctx, r"""proc emit() [io] -> Int { print "default"; 4 }
proc choose(value = emit()) -> Int { value }
print ${choose(9)}
print ${choose()}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "9\ndefault\n4\n")?
  let rejected = test.run_script(ctx, r"""proc emit() [io] -> Int { print "default"; 4 }
proc choose(value = emit()) -> Int { value }
proc restricted() [] -> Int { choose(9) }
print ${restricted()}
""")?
  test.ok(!rejected.success, rejected.stdout)?
  let pure_rejected = test.run_script(ctx, r"""proc emit() [io] -> Int { print "default"; 4 }
pure choose(value = emit()) -> Int { value }
print ${choose(9)}
""")?
  test.ok(!pure_rejected.success, pure_rejected.stdout)?
}

test test_default_parameters_preserve_nullable_result_and_record_payload_types [error] { |ctx|
  let output = test.run_script(ctx, r"""const defaults = {jobs: 4}
pure maybe_name() -> Str? { "label" }
pure name(value = maybe_name()) -> Str? { value }
pure config(value = defaults) -> Int { value.jobs }
pure outcome() -> Result[Int] { Ok(7) }
pure result(value = outcome()) -> Result[Int] { value }
print ${name() ?? "missing"} ${name(null) ?? "missing"} ${config()} ${result()?}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "label missing 4 7\n")?
}

test test_default_parameters_local_cleanup_precedes_body_and_lexical_return [error] { |ctx|
  let output = test.run_script(ctx, r"""proc mark(message: Str) [io] -> Unit { print $message }
proc choose(value = { defer mark("local"); 4 }) [io] -> Int { print "body"; value }
proc leaves(value = if true { defer mark("return cleanup"); return 7 } else { 4 }) [io] -> Int { print "unreached"; value }
print ${choose()}
print ${choose(9)}
print ${leaves()}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "local\nbody\n4\nbody\n9\nreturn cleanup\n7\n")?
}

test test_default_parameters_preserve_callable_handle_domains [error] { |ctx|
  let output = test.run_script(ctx, r"""pure identity(value: Int) -> Int { value }
pure choose(callback = identity) -> Pure { callback }
let callback = choose()
print ${callback.call(7).require(Int)?}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "7\n")?
}
