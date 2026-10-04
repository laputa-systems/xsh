test test_default_parameters_infer_const_projection_primitive_and_constructor_types { |ctx|
  let output = test.run_script(
    ctx,
    r"""const defaults = {jobs: 4, timeout: 30s}
pure jobs(value = defaults.jobs + 1) -> Int { value }
pure timeout(value = defaults.timeout) -> Duration { value }
pure default_path(value = Path("config")) -> Path { value }
print ${jobs()} ${jobs(9)} ${timeout() == 30s} ${default_path().display()}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """5 9 true config
"""
}

test test_default_parameters_keep_already_checked_call_defaults { |ctx|
  let output = test.run_script(
    ctx,
    r"""pure next() -> Int { 4 }
pure explicit(jobs: Int = next()) -> Int { jobs }
pure inferred(jobs = next()) -> Int { jobs }
print ${explicit()} ${inferred()} ${inferred(9)}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """4 4 9
"""
}

test test_default_parameters_cli_uses_checked_inferred_prepared_types { |ctx|
  let source = r"""const defaults = {jobs: 4, delay: 20ms, verbose: false, tags: [1, 2]}
cli main(jobs = defaults.jobs + 1, delay = defaults.delay, verbose = defaults.verbose, tags = defaults.tags) [] {
  let checked_jobs: Int = jobs
  let checked_delay: Duration = delay
  let checked_verbose: Bool = verbose
  let checked_tags: List[Int] = tags
  print $checked_jobs $checked_delay $checked_verbose ${[f"{tag}" for tag in checked_tags].join(",")}
}
"""
  let omitted = test.run_script(ctx, source)?
  assert omitted.success, omitted.stderr
  assert omitted.stdout == """5 20ms false 1,2
"""
  let supplied = test.run_script(ctx, source, ["--jobs=9", "--delay=30ms", "--verbose", "--tags=3"])?
  assert supplied.success, supplied.stderr
  assert supplied.stdout == """9 30ms true 1,2,3
"""
  let help = test.run_script(ctx, source, ["--help"])?
  assert help.success, help.stderr
  assert "Int, default: 5" in help.stdout, help.stdout
  assert "Duration, default: 20ms" in help.stdout, help.stdout
}

test test_default_parameters_cli_rejects_runtime_defaults_without_execution { |ctx|
  let source = r"""proc runtime_default() [io] -> Int { print DEFAULT_EXECUTED; 4 }
cli main(jobs = runtime_default()) [io] { print BODY_EXECUTED }
"""
  for arguments in [[], ["--help"], ["--jobs=9"]] {
    let rejected = test.run_script(ctx, source, arguments)?
    assert ! rejected.success, rejected.stderr
    assert rejected.stdout == ""
    assert "check.infer-param" not in rejected.stderr, rejected.stderr
  }
}

test test_default_parameters_static_alias_keeps_effectful_defaults_once_and_named_slots { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc default_value(label: Str, value: Int) [io] -> Int { print $label; value }
proc combine(left = default_value("left default", 1), right = default_value("right default", 2)) [io] -> Int { left + right }
let shared = combine
let named = shared
let omitted: Int = named()
print $omitted
print ${named(right: default_value("supplied", 8))}
print ${shared(left: 9, right: 10)}
print ${named.call(left: 9)}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """left default
right default
3
supplied
left default
9
19
right default
11
"""
}

test test_default_parameters_keep_generic_constructor_grounding_in_the_default { |ctx|
  let output = test.run_script(
    ctx,
    r"""type Envelope[T] = {value: T}
type IntEnvelope = Envelope[Int]
pure explicit(box = IntEnvelope(value: 4)) -> Int { box.value }
pure inferred(box = Envelope(value: 5)) -> Int { box.value }
print ${explicit()} ${inferred()}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """4 5
"""
  let rejected = test.run_script(
    ctx,
    r"""type Phantom[T] = {label: Str}
pure unresolved(box = Phantom(label: "empty")) -> Str { box.label }
print ${unresolved()}
""",
  )?
  assert ! rejected.success, rejected.stdout
}

test test_default_parameters_nested_local_alias_calls_keep_heap_frames { |ctx|
  let output = test.run_script(
    ctx,
    r"""pure depth(value = 0) -> Int {
  if value < 1200 {
    let next = depth
    next(value: value + 1)
  } else { value }
}
print ${depth()}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """1200
"""
}

test test_default_parameters_keep_stream_defaults_lazy_and_supplied_arguments_eager { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc value(label: Str) [io] -> Int {
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
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """supplied
before
explicit default
body
7
inferred default
body
7
body
7
"""
}

test test_default_parameters_resolve_imported_constants { |ctx|
  let root = test.temp_dir(ctx, name: "default-module")?
  fp"{root}/config.xsh".write_atomic(r"""##! Build defaults.
## The default worker count.
export const settings = {jobs: 6}
""")?
  let output = test.run_script(
    ctx,
    r"""use config as c
pure choose(jobs = c.settings.jobs) -> Int { jobs }
print ${choose()}
""",
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  assert output.success, output.stderr
  assert output.stdout == """6
"""
}

test test_default_parameters_keep_omitted_order_lazy_effects_and_cleanup { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc mark(name: Str) [io] { print $name }
proc value(name: Str, count: Int) [io, error] -> Int {
  defer mark("default cleanup")
  print $name
  count
}
proc choose(left = value("left", 1), right = value("right", 2)) [io, error] -> Int { left + right }
print ${choose()}
print ${choose(right: value("supplied", 9))}
print ${choose(8, 9)}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """left
default cleanup
right
default cleanup
3
supplied
default cleanup
left
default cleanup
10
17
"""
}

test test_default_parameters_require_anchors_and_keep_parameter_scope { |ctx|
  for source in [
    """pure choose(value = null) -> Int { 1 }
""",
    """pure choose(value = []) -> Int { 1 }
""",
    """pure choose(value = {}) -> Int { 1 }
""",
    """pure choose(value = map.empty()) -> Int { 1 }
""",
    """pure choose(left: Int = 1, right = left) -> Int { right }
""",
    """pure choose(value) -> Int { 1 }
""",
    """pure choose(...values = [1]) -> Int { 1 }
""",
  ] {
    let output = test.run_script(ctx, source)?
    assert ! output.success, output.stdout
  }
}

test test_default_parameters_use_outer_names_and_checked_dependency_signatures { |ctx|
  let output = test.run_script(
    ctx,
    r"""let count = 5
pure dependent(value = target()) -> Int { value }
pure target() { 7 }
pure shadow(count = count) -> Int { count }
pure anchored(value: Int = dependent()) -> Int { value }
print ${dependent()} ${shadow()} ${shadow(9)} ${anchored()}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """7 5 9 7
"""
}

test test_default_parameters_keep_error_propagation_and_skip_later_defaults { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc mark(message: Str) [io] { print $message }
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
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """cleanup
true
17
"""
}

test test_default_parameters_keep_explicit_anchors_across_declaration_cycles { |ctx|
  let output = test.run_script(
    ctx,
    r"""pure first(value = second(3)) -> Int { value }
pure second(value: Int = first(4)) -> Int { value }
print ${first()} ${second()}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """3 4
"""
  let rejected = test.run_script(
    ctx,
    r"""pure first(value = second()) { value }
pure second(value = first()) { value }
""",
  )?
  assert ! rejected.success, rejected.stdout
}

test test_default_parameters_use_the_callable_return_target { |ctx|
  let output = test.run_script(
    ctx,
    r"""pure explicit(value: Int = if true { return 7 } else { 4 }) -> Int { value + 1 }
pure inferred(value = if true { return 7 } else { 4 }) -> Int { value + 1 }
print ${explicit()} ${inferred()} ${inferred(9)}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """7 7 10
"""
}

test test_default_parameters_nested_calls_use_heap_frames { |ctx|
  var source = """pure step_0() -> Int { 7 }
"""
  for index in range(1, 1201) {
    let step = f"""
      pure step_{index}(value = step_{index - 1}()) -> Int {{ value }}

      """
    source = source + step
  }

  source = source + r"print ${step_1200()}" + "\n"
  let output = test.run_script(ctx, source)?
  assert output.success, output.stderr
  assert output.stdout == """7
"""
}

test test_default_parameters_keep_omitted_effects_in_callable_contracts { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc emit() [io] -> Int { print "default"; 4 }
proc choose(value = emit()) -> Int { value }
print ${choose(9)}
print ${choose()}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """9
default
4
"""
  let rejected = test.run_script(
    ctx,
    r"""proc emit() [io] -> Int { print "default"; 4 }
proc choose(value = emit()) -> Int { value }
proc restricted() [] -> Int { choose(9) }
print ${restricted()}
""",
  )?
  assert ! rejected.success, rejected.stdout
  let pure_rejected = test.run_script(
    ctx,
    r"""proc emit() [io] -> Int { print "default"; 4 }
pure choose(value = emit()) -> Int { value }
print ${choose(9)}
""",
  )?
  assert ! pure_rejected.success, pure_rejected.stdout
}

test test_default_parameters_preserve_nullable_result_and_record_payload_types { |ctx|
  let output = test.run_script(
    ctx,
    r"""const defaults = {jobs: 4}
pure maybe_name() -> Str? { "label" }
pure name(value = maybe_name()) -> Str? { value }
pure config(value = defaults) -> Int { value.jobs }
pure outcome() -> Result[Int] { Ok(7) }
pure result(value = outcome()) -> Result[Int] { value }
print ${name() ?? "missing"} ${name(null) ?? "missing"} ${config()} ${result()?}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """label missing 4 7
"""
}

test test_default_parameters_local_cleanup_precedes_body_and_lexical_return { |ctx|
  let output = test.run_script(
    ctx,
    r"""proc mark(message: Str) [io] -> Unit { print $message }
proc choose(value = { defer mark("local"); 4 }) [io] -> Int { print "body"; value }
proc leaves(value = if true { defer mark("return cleanup"); return 7 } else { 4 }) [io] -> Int { print "unreached"; value }
print ${choose()}
print ${choose(9)}
print ${leaves()}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """local
body
4
body
9
return cleanup
7
"""
}

test test_default_parameters_preserve_callable_handle_domains { |ctx|
  let output = test.run_script(
    ctx,
    r"""pure identity(value: Int) -> Int { value }
pure choose(callback = identity) -> Pure { callback }
let callback = choose()
print ${callback.call(7).require(Int)?}
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == """7
"""
}

test test_default_parameters_inferred_types_constrain_arguments_spreads_and_bodies { |ctx|
  let declaration = r"""const defaults = {jobs: 4, timeout: 30s}
proc build(jobs = defaults.jobs, timeout = defaults.timeout) -> Str { f"{jobs} {timeout}" }
"""
  let accepted = test.run_script(
    ctx,
    declaration + r"""pure optional(jobs: Int? = defaults.jobs) -> Int { jobs ?? 0 }
let options = {jobs: 8, timeout: 5s}
print ${build()} ${build(...options)} ${build(timeout: 1s)} ${optional()} ${optional(null)}
""",
  )?
  assert accepted.success, accepted.stderr
  assert accepted.stdout == """4 30s 8 5s 4 1s 4 0
"""
  for source in [
    """let _ = build("many")
""",
    """let _ = build(null)
""",
    """let bad = {jobs: "8"}
let _ = build(...bad)
""",
    """pure render(timeout = defaults.timeout) -> Str { timeout }
""",
  ] {
    let rejected = test.run_script(ctx, declaration + source)?
    assert ! rejected.success, source
    assert "check.type-mismatch" in rejected.stderr, rejected.stderr
  }
}
