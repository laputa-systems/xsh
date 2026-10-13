# A callable type, `proc(PARAMS) [EFFECTS] -> T` or `pure(PARAMS) -> T`, keeps
# a signature and an effect bound on a callable that is chosen at run time. A
# value gets the type only from a function whose signature fits, a call
# through it is checked like a call by name, and it stays a plain `Proc` or
# `Pure` handle at run time.

type Builder = proc(root: Path) [fs, process, error] -> Result[Str]
type SpecificBuilder = proc(root: Path) [fs, error] -> Result[Str]
type BuilderRecord = {build: Builder}
type SpecificBuilderRecord = {build: SpecificBuilder}

type Scale = pure(n: Int) -> Int
type CallbackRecord = {double: Scale}

type Join = pure(left: Str, right: Str) -> Str

type Step = {name: Str, build: Builder, scale: Scale}
type Holder = {callback: Scale}
type PathHolder = {path: Path}
type CallbackCollection = {callbacks: List[Scale]}
type Bundle = {nested: Holder}
type BroadcastAddress = {broadcast: Str?}
type BroadcastInterface = {ipv4: BroadcastAddress?}

proc debug_build(root: Path) [error] -> Result[Str] {
  Ok(f"debug {root}")
}

proc release_build(root: Path) [fs, error] -> Result[Str] {
  let present = root.exists()?
  Ok(f"release {root} {present}")
}

# The effects are inferred; they must still fit the bound.
proc inferred_build(root: Path) -> Result[Str] {
  let present = root.exists()?
  Ok(f"inferred {root} {present}")
}

proc defaulted_build(root = /default) [error] -> Result[Str] {
  Ok(f"defaulted {root}")
}

pure double(n: Int) -> Int {
  n * 2
}

pure triple(n: Int) -> Int {
  n * 3
}

pure parse_number(value: Int) -> Int {
  var parsed_value = 0
  with parsed = f"{value}".parse_int() {
    parsed_value = parsed
  } else { |failure|
    parsed_value = -1
  }
  parsed_value
}

pure empty_callback_count() -> Int {
  let callbacks: Map[Str, Scale] = {}
  callbacks.len()
}

pure floor_computed_float(value: Float) -> Int {
  var result = 0
  with rounded = (value + 0.5).floor() {
    result = rounded
  } else { |failure|
    result = -1
  }
  result
}

pure parse_computed_string(value: Int) -> Int {
  var result = 0
  with parsed = (f"{value}" + "").parse_int() {
    result = parsed
  } else { |failure|
    result = -1
  }
  result
}

proc callback_from_map(callbacks: Map[Scale]) [error] -> Result[Int] {
  with selected = callbacks.get("double") {
    return Ok(selected(4))
  } else { |failure| return Err(failure) }
}

proc callback_from_record(callbacks: CallbackRecord) [error] -> Result[Int] {
  with selected = callbacks.get("double") {
    return Ok(selected(4))
  } else { |failure| return Err(failure) }
}

pure join(left: Str, right: Str) -> Str {
  left + "-" + right
}

pure get_holder() -> Holder {
  Holder(callback: triple)
}

pure optional_broadcast(item: BroadcastInterface) -> Str {
  var text = "0.0.0.0"
  if let address = item.ipv4 {
    text = address.broadcast ?? text
  }
  text
}

test test_non_callable_field_projection_from_pattern_slot {
  assert optional_broadcast({ipv4: {broadcast: "192.0.2.255"}}) == "192.0.2.255"
}

test test_root_functions_keep_signatures_distinct_from_same_named_imports { |ctx|
  let root = test.temp_dir(ctx, name: "typed-callable-signature-owners")?
  fp"{root}/helper.xsh".write(r"""##! Functions that share names with root declarations.
## Returns the joined imported words.
export pure choose(left: Str, right: Str) -> Str { left + right }
## Returns the imported procedure value.
export proc capture(left: Str, right: Str) -> Str { left + right }
""")
  let output = test.run_script(
    ctx,
    r"""use helper as source
pure choose(value: Int) -> Int { value + 1 }
proc capture(value: Int) -> Int { value + 2 }
print source.choose("module", "pure")
print choose(40)
print source.capture("module", "proc")
print capture(40)
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  assert output.success, output.stderr
  assert output.stdout == """modulepure
41
moduleproc
42
"""
}

proc choose(debug: Bool) -> Builder {
  if debug { debug_build } else { release_build }
}

proc choose_by_name(name: Str) -> Builder {
  match name {
    "debug" => debug_build
    "inferred" => inferred_build
    else => release_build
  }
}

proc run_with(build: Builder, root: Path) [fs, process, error] -> Result[Str] {
  build(root)
}

# The bound of `Builder` is what this proc is inferred to need.
proc run_inferred(build: Builder, root: Path) {
  build(root)
}

proc run_default(root: Path, build: Builder = debug_build) [fs, process, error] -> Result[Str] {
  build(root:)
}

pure apply(scale: Scale, n: Int) -> Int {
  scale(n)
}

let top_level: Builder = release_build

proc run_top_level(root: Path) [fs, process, error] -> Result[Str] {
  top_level(root)
}

test test_typed_callable_selected_by_if_is_called_with_checked_arguments {
  let debug: Builder = if true { debug_build } else { release_build }
  let release: Builder = if false { debug_build } else { release_build }
  assert debug(/src)? == "debug /src"
  assert release(/src)? == "release /src false"
  assert release(root: /src)? == "release /src false"
  # A string literal takes the parameter's `Path` type, as in a call by name.
  assert debug("/literal")? == "debug /literal"
}

test test_typed_callable_values_from_checked_expression_families_remain_callable {
  let callbacks: List[Scale] = [double, triple]
  let indexed = callbacks[0]
  assert indexed(4) == 8

  let branched: Scale = if true {
    let marker = 1
    let _ = marker
    triple
  } else {
    let marker = 2
    let _ = marker
    double
  }
  assert branched(4) == 12

  let matched: Scale = match true {
    true => double,
    false => triple,
  }
  assert matched(5) == 10

  let absent: Scale? = null
  let recovered = absent ?? double
  assert recovered(6) == 12

  let looped: Scale = loop { break triple }
  assert looped(3) == 9

  let scoped: Scale = within 5s {
    let result = double
    result
  }?
  assert scoped(7) == 14

  let directory_scoped: Scale = cd (p".") {
    let result = triple
    result
  }?
  assert directory_scoped(2) == 6
}

pure recover_with_typed_failure(result: Result[Int, Scale]) -> Int {
  guard let value = result else { |failure| return failure(3) }
  value
}

pure recover_with_typed_with_failure(
  first: Result[Int, Scale],
  second: Result[Int, Scale],
) -> Result[Int, Scale] {
  var value = 0
  with direct = first, left = first?, right = second? {
    value = direct + left + right
  } else { |failure| return Ok(failure(4)) }
  Ok(value)
}

test test_typed_callable_guard_success_binding_keeps_its_checked_type {
  assert recover_with_typed_failure(Ok(9)) == 9
}

test test_typed_callable_guard_failure_binding_keeps_its_checked_type {
  assert recover_with_typed_failure(Err(double)) == 6
}

test test_typed_callable_with_failure_binding_keeps_its_checked_type {
  let result = recover_with_typed_with_failure(Ok(2), Ok(3))
  let value = match result { Ok(value) => value, Err(failure) => failure(4) }
  assert value == 7
}

test test_typed_callable_with_propagated_failure_binding_keeps_its_checked_type {
  let result = recover_with_typed_with_failure(Ok(2), Err(double))
  let value = match result { Ok(value) => value, Err(failure) => failure(4) }
  assert value == 8
}

test test_typed_callable_with_initial_failure_binding_keeps_its_checked_type {
  let result = recover_with_typed_with_failure(Err(double), Ok(3))
  let value = match result { Ok(value) => value, Err(failure) => failure(4) }
  assert value == 8
}

test test_typed_callable_is_returned_passed_and_stored {
  assert choose(true)(/a)? == "debug /a"
  assert choose_by_name("inferred")(/a)? == "inferred /a false"
  assert run_with(choose(false), /b)? == "release /b false"
  assert run_with(debug_build, root: /c)? == "debug /c"
  assert run_inferred(inferred_build, /d)? == "inferred /d false"
  assert run_default(/e)? == "debug /e"
  assert run_default(/e, build: release_build)? == "release /e false"
  assert run_top_level(/f)? == "release /f false"

  let chosen = choose(true)
  assert chosen(/g)? == "debug /g"

  var current: Builder = debug_build
  current = release_build
  assert current(/h)? == "release /h false"

  let builders: List[Builder] = [debug_build, release_build, inferred_build]
  var seen = []
  for build in builders {
    seen = [@seen, build(/i)?]
  }

  assert seen == ["debug /i", "release /i false", "inferred /i false"]
}

test test_typed_callable_record_field_is_called_like_a_method {
  assert get_holder().callback(7) == 21

  var holder: Holder = Holder(callback: double)
  holder.callback = triple
  assert holder.callback(3) == 9

  var bundle: Bundle = Bundle(nested: Holder(callback: double))
  bundle.nested.callback = triple
  assert bundle.nested.callback(3) == 9

  let updated = {...bundle, nested.callback: double}
  assert updated.nested.callback(4) == 8

  let step = Step(name: "release", build: release_build, scale: double)
  assert step.build(/a)? == "release /a false"
  assert step.scale(21) == 42

  let debug = Step(name: "debug", build: debug_build, scale: triple)
  assert debug.build(root: /b)? == "debug /b"
  assert debug.scale(n: 2) == 6

  let field = step.build
  assert field(/c)? == "release /c false"
}

pure apply_validated_collection(callbacks: NonEmpty[Scale]) -> Int {
  var holder: CallbackCollection = CallbackCollection(callbacks: [double])
  holder.callbacks = callbacks
  let updated = {...holder, callbacks: callbacks}
  updated.callbacks[0](7)
}

pure flatten_nested_callbacks(callbacks: List[NonEmpty[Scale]]) -> List[List[Scale]] {
  [@callbacks]
}

test test_validated_callable_collection_fits_its_base_record_field {
  assert apply_validated_collection([double]) == 14
}

test test_nonempty_nested_callable_lists_fit_a_spliced_list {
  let callbacks = flatten_nested_callbacks([[double], [triple]])
  assert callbacks[0][0](4) == 8
  assert callbacks[1][0](4) == 12
}

test test_pure_typed_callable_is_callable_from_pure_code {
  let scale: Scale = if true { triple } else { double }
  assert scale(5) == 15
  assert apply(double, 4) == 8
  assert apply(scale, n: 4) == 12

  let joined: Join = join
  assert joined("a", right: "b") == "a-b"
  assert joined(right: "b", left: "a") == "a-b"

  let scaled = [1, 2, 3] |> map scale(.) |> collect()
  assert scaled == [3, 6, 9]
}

proc noted(log: Path, label: Str) [fs, error] -> Result[Str] {
  log.write(log.read_text()? + label)
  Ok(label)
}

# Named entries are evaluated as written, then bound to their parameters,
# exactly as in a call by function name.
test test_typed_call_evaluates_named_arguments_in_source_order { |ctx|
  let log = test.temp_file(ctx, name: "order.log", contents: bytes.from_text(""))?
  let joined: Join = join
  assert joined(right: noted(log, "r")?, left: noted(log, "l")?) == "l-r"
  assert log.read_text()? == "rl"

  log.write("")
  assert join(right: noted(log, "r")?, left: noted(log, "l")?) == "l-r"
  assert log.read_text()? == "rl"

  log.write("")
  let step = Step(name: "s", build: debug_build, scale: double)
  assert step.scale(n: noted(log, "n")?.byte_len()) == 2
  assert log.read_text()? == "n"
}

type Count = pure(text: Str) -> Int

pure byte_count(text: Str) -> Int {
  text.byte_len()
}

pure use_with_callbacks(result: Result[Count, Scale]) -> Int {
  var value = 0
  with candidate = result {
    value = candidate("success")
  } else { |failure| value = failure(4) }
  value
}

test test_typed_callable_with_success_and_error_slots_keep_their_own_types {
  assert use_with_callbacks(Ok(byte_count)) == 7
  assert use_with_callbacks(Err(double)) == 8
}

test test_typed_callable_success_value_binds_with_direct_with {
  let result: Result[Scale] = Ok(double)
  with selected = result {
    assert selected(4) == 8
  } else { |failure|
    fail failure.message
  }
}

pure apply_present(maybe: Scale?) -> Int {
  if let scale = maybe {
    return scale(4)
  }
  0
}

# An optional binding calls through the non-null value, not the optional.
test test_typed_callable_optional_binding_calls_the_present_value {
  assert apply_present(double) == 8
  assert apply_present(null) == 0
}

# A top-level statement owns its callable-typed bindings (`with`, `for`,
# patterns, comprehension targets, pipeline stage parameters) as a function
# body does, so each is checked against the value that writes it.
test test_typed_callable_bindings_inside_top_level_statements { |ctx|
  let result = test.run_xsh(
    ctx,
    r"""type Scale = pure(n: Int) -> Int
pure double(n: Int) -> Int { n * 2 }
pure triple(n: Int) -> Int { n * 3 }
let value: Result[Scale] = Ok(double)
let scales: List[Scale] = [double, triple]
let maybe: Scale? = double

with selected = value {
  assert selected(4) == 8
} else { |failure|
  assert false, failure.message
}

var total = 0
for scale in scales {
  total += scale(1)
}
assert total == 5

match value {
  Ok(scale) => assert scale(4) == 8
  Err(failure) => assert false, failure.message
}

if let scale = maybe {
  assert scale(4) == 8
}

var rounds = 0
while let scale = maybe {
  assert scale(4) == 8
  rounds += 1
  if rounds == 2 {
    break
  }
}

let doubled = [scale(2) for scale in scales]
assert doubled == [4, 6]

let piped = scales |> map { |scale| scale(2) } |> collect()
assert piped == [4, 6]
""",
  )?
  assert result.success, result.stderr
}

# A callable value captured by a stage block, and one that crosses the
# workers of `par-map`, is the same checked call.
test test_typed_callable_is_called_inside_stage_blocks {
  let counter: Count = byte_count
  let serial = ["a", "bb", "ccc"] |> map counter(.) |> collect()
  assert serial == [1, 2, 3]

  let parallel = ["a", "bb", "ccc", "dddd"] |> par-map counter(.) |> collect()
  assert parallel == [1, 2, 3, 4]

  let counters: List[Count] = [byte_count, byte_count]
  var total = 0
  for each in counters {
    total += each("xyz")
  }

  assert total == 6
}

test test_function_with_defaults_fits_and_is_called_with_every_argument {
  let build: Builder = defaulted_build
  assert build(/explicit)? == "defaulted /explicit"
}

test test_typed_callable_fits_the_dynamic_handle_of_its_kind {
  let build: Builder = debug_build
  let dynamic: Proc = build
  let text = dynamic.call(/a)?.require(Str)?
  assert text == "debug /a"

  let scale: Scale = double
  let handle: Pure = scale
  assert handle.call(4).require(Int)? == 8
}

test test_typed_callable_container_methods_keep_their_element_types {
  let callbacks: List[Scale] = []
  let updated = callbacks.push(double)
  assert updated[0](4) == 8
  let additions: List[Scale] = [double]
  let extended = callbacks.extend([@additions])
  assert extended[0](4) == 8

  let by_name: Map[Str, Scale] = {}
  let stored = by_name.set("double", double)
  with selected = stored.get("double")? {
    assert selected(4) == 8
  } else { |failure| test.fail(failure.message) }
}

test test_registered_method_checks_intrinsic_formatted_string_receiver {
  assert parse_number(12) == 12
}

test test_empty_typed_callable_map_is_a_valid_initializer {
  assert empty_callback_count() == 0
}

test test_registered_method_uses_computed_float_receiver {
  assert floor_computed_float(1.6) == 2
}

test test_registered_method_uses_computed_string_receiver {
  assert parse_computed_string(12) == 12
}

proc bump(n: Int) -> Int {
  n + 1
}

proc announce(log: Path) [fs, error] {
  log.write("written")
}

proc call_dynamic(handle: Proc, argument: Any) -> Result[Any] {
  handle.call(argument)
}

# A dynamic `Proc` handle does not say what its proc returns, so `.call` is a
# Result for every proc: the proc's own Result, or `Ok` of any other value.
test test_proc_call_is_a_result_whatever_the_proc_returns { |ctx|
  assert call_dynamic(bump, 1) is Ok(_)
  assert call_dynamic(bump, 1)?.require(Int)? == 2
  let seen = if let Ok(value) = call_dynamic(bump, 4) { value.require(Int)? } else { -1 }
  assert seen == 5

  assert call_dynamic(strict_stage, "a")?.require(Str)? == "stage a"
  assert call_dynamic(strict_stage, "") is Err(_)

  let log = fp"{test.temp_dir(ctx, name: "proc-call")?}/log"
  assert call_dynamic(announce, log) is Ok(_)
  assert log.read_text()? == "written"
}

test test_alias_that_kept_its_signature_fits_a_callable_type {
  let alias = release_build
  let build: Builder = alias
  let chosen: Builder = if true { alias } else { debug_build }
  assert build(/a)? == "release /a false"
  assert chosen(/b)? == "release /b false"
}

test test_record_spread_keeps_a_compatible_callable_field {
  let specific: SpecificBuilderRecord = SpecificBuilderRecord(build: release_build)
  let widened: BuilderRecord = {...specific}
  assert widened.build(/a)? == "release /a false"
}

proc check_errors(ctx: TestContext, source: Str) [fs, process, env, error] -> Result[Str] {
  let output = test.run_script(ctx, source)?
  assert output.status == 2, f"{output.stdout}{output.stderr}"
  assert output.stdout == "", output.stdout
  output.stderr
}

pure count(text: Str, needle: Str) -> Int {
  text.split(needle).len() - 1
}

# A call through the type passes every argument, so a parameter is a label
# and a type and nothing else.
test test_callable_type_rejects_parameters_a_call_could_not_supply { |ctx|
  let stderr = check_errors(
    ctx,
    r"""type Defaulted = proc(root: Path = p".") -> Result[Unit]
type Rest = proc(...roots: List[Path]) -> Result[Unit]
type Twice = pure(n: Int, n: Int) -> Int
type Untyped = pure(n = 1) -> Int
type Member = Union[pure(n: Int) -> Int, Str]

print "unreachable"
""",
  )?
  assert count(stderr, "err[check.callable-type]") == 4, stderr
  assert "parameter `root` of a callable type cannot have a default" in stderr, stderr
  assert "a callable type cannot have a rest parameter (`...roots`)" in stderr, stderr
  assert "a callable type names parameter `n` twice" in stderr, stderr
  assert "parameter `n` of a callable type needs a type" in stderr, stderr
  assert count(stderr, "err[check.union-type]") == 1, stderr
  assert "a member cannot be a callable type" in stderr, stderr
}

test test_function_that_does_not_fit_a_callable_type_is_rejected { |ctx|
  let stderr = check_errors(
    ctx,
    r"""type Builder = proc(root: Path) [fs, error] -> Result[Unit]
type Scale = pure(n: Int) -> Int

proc fetch(root: Path) [fs, net, error] -> Result[Unit] {
  return
}

proc unrestricted(root: Path) [io, error] -> Result[Unit] {
  return
}

proc relabeled(dir: Path) [fs, error] -> Result[Unit] {
  return
}

proc retyped(root: Str) [fs, error] -> Result[Unit] {
  return
}

proc other_result(root: Path) [fs, error] -> Result[Int] {
  Ok(1)
}

proc wider(root: Path, jobs: Int = 1) [fs, error] -> Result[Unit] {
  return
}

proc collecting(...roots: List[Path]) [fs, error] -> Result[Unit] {
  return
}

proc fits(root: Path) [fs] -> Result[Unit] {
  return
}

pure double(n: Int) -> Int {
  n * 2
}

proc select(dynamic: Proc, flag: Bool) -> Result[Unit] {
  let a: Builder = fetch
  let b: Builder = unrestricted
  let c: Builder = relabeled
  let d: Builder = retyped
  let e: Builder = other_result
  let f: Builder = wider
  let g: Builder = collecting
  let h: Builder = double
  let i: Scale = fits
  let j: Builder = dynamic
  let k: Builder = if flag { fits } else { fetch }
  return
}
""",
  )?
  assert count(stderr, "err[check.callable-mismatch]") == 11, stderr
  assert count(stderr, "err[check.callable-mismatch]: `fetch` does not fit") == 2, stderr
  assert "it requires the `net` effect, which `[fs, error]` does not allow" in stderr, stderr
  assert "it requires the `io` effect" in stderr, stderr
  assert "parameter 1 is named `dir`, expected `root`" in stderr, stderr
  assert "parameter `root` has type Str, expected Path" in stderr, stderr
  assert "it returns Result[Int, Error], expected Result[Unit, Error]" in stderr, stderr
  assert "expected 1 parameters, found 2" in stderr, stderr
  assert "parameter 1 `roots` is a rest parameter" in stderr, stderr
  assert "expected a proc, found a pure function" in stderr, stderr
  assert "expected a pure function, found a proc" in stderr, stderr
  assert "a dynamic `Proc` has no checked signature" in stderr, stderr
}

# The callable's own effects may be fewer than the bound; the bound is what a
# call charges, whatever function is behind the value.
test test_typed_callable_call_charges_its_effect_bound_to_the_caller { |ctx|
  let stderr = check_errors(
    ctx,
    r"""type Builder = proc(root: Path) [fs, error] -> Result[Unit]
type Open = proc(root: Path) -> Result[Unit]

proc narrow(build: Builder, root: Path) [error] -> Result[Unit] {
  build(root)
}

pure from_pure(build: Builder, root: Path) -> Int {
  let _ = build(root)
  1
}

proc excluded(build: Builder, root: Path) -> Result[Unit] {
  without fs {
    build(root)?
  }

  return
}

proc open_call(open: Open, root: Path) {
  open(root)
}

proc bounded(open: Open, root: Path) [fs, net, process, env, time, error] -> Result[Unit] {
  open(root)
}

proc through_inferred(open: Open, root: Path) [fs, net, process, env, time, error] -> Result[Unit] {
  open_call(open, root)
}
""",
  )?
  assert "effect `fs` required by `build` is not in caller's declared effects" in stderr, stderr
  assert "err[check.pure-effect]: effectful proc is not allowed in pure functions" in stderr, stderr
  assert "effect `fs` required by `build` is excluded by `without fs`" in stderr, stderr
  assert "callable `open` has an unknown or unrestricted effect contract" in stderr, stderr
  assert "proc `open_call` has an unknown effect summary: open_call -> open" in stderr, stderr
  assert count(stderr, "err[check.effect-violation]") == 4, stderr
}

test test_typed_callable_call_checks_arguments_and_result { |ctx|
  let stderr = check_errors(
    ctx,
    r"""type Builder = proc(root: Path, jobs: Int) [fs, error] -> Result[Str]

type Fields = {root: Path, jobs: Int}

proc build(root: Path, jobs: Int) [fs, error] -> Result[Str] {
  Ok("built")
}

proc main(roots: List[Path], fields: Fields) -> Result[Unit] {
  let chosen: Builder = build
  chosen(p"/a")?
  chosen(p"/a", "four")?
  chosen(p"/a", workers: 4)?
  let count: Int = chosen(p"/a", 4)?
  chosen(@roots)?
  chosen(...fields)?
  chosen.call(p"/a", 4)?
  return
}
""",
  )?
  assert "err[check.arity]: incorrect function arity: expected 2 arguments, found 1" in stderr, stderr
  assert count(stderr, "err[check.type-mismatch]") == 2, stderr
  assert "err[check.named-arg]" in stderr, stderr
  assert count(stderr, "err[check.callable-mismatch]: a call through a callable type passes each argument explicitly") == 2, stderr
  assert "is called directly, as `name(...)`; it has no `.call` method" in stderr, stderr
}

# A handle does not carry its signature, so nothing at run time can vouch for
# one: the type is never a validation target or a type test.
test test_callable_type_is_never_a_runtime_test { |ctx|
  let stderr = check_errors(
    ctx,
    r"""type Scale = pure(n: Int) -> Int
type Holder = {name: Str, scale: Scale}

pure double(n: Int) -> Int {
  n * 2
}

proc main(raw: Any, handle: Pure) -> Result[Unit] {
  let a = raw.require(Scale)?
  let b = raw.require(Holder)?
  let c: Holder = raw.require()?
  let d = raw is Scale
  let e: Scale = raw
  let f: Scale = handle
  let g = [1, 2] |> map(a) |> collect
  return
}
""",
  )?
  assert count(stderr, "err[check.callable-type]") == 3, stderr
  assert "cannot be tested at run time: a callable value does not carry its signature" in stderr, stderr
  assert "err[check.require-target]" in stderr, stderr
  assert "a callable signature cannot be validated at run time" in stderr, stderr
  assert "a dynamic `Pure` has no checked signature" in stderr, stderr
}

# A mismatch at the top level is reported once, though a file with a
# return-inferred function types its top-level bindings in an earlier pass.
test test_top_level_mismatch_is_reported_once { |ctx|
  let stderr = check_errors(
    ctx,
    r"""type Builder = proc(root: Path) [fs, error] -> Result[Unit]

proc fetch(root: Path) [fs, net, error] -> Result[Unit] {
  return
}

proc inferred(root: Path) {
  print f"{root}"
}

let build: Builder = fetch
let count: Int = "text"
inferred(p"/a")
""",
  )?
  assert count(stderr, "err[check.callable-mismatch]") == 1, stderr
  assert count(stderr, "err[check.type-mismatch]") == 1, stderr
}

test test_stage_descriptor_still_names_a_function { |ctx|
  let stderr = check_errors(
    ctx,
    r"""type Scale = pure(n: Int) -> Int

pure double(n: Int) -> Int {
  n * 2
}

let scale: Scale = double
let scaled = [1, 2] |> map(scale) |> collect
print ${scaled.len()}
""",
  )?
  assert "err[check.stream-callable]" in stderr, stderr
}

error StageError = Missing(name: Str)

type Stage = proc(name: Str) [error] -> Result[Str]

type StrictStage = proc(name: Str) [error] -> Result[Str, StageError]

type Locate = pure(name: Str) -> Result[Path]

proc strict_stage(name: Str) [error] -> Result[Str, StageError] {
  fail .Missing(name) when name == ""
  Ok(f"stage {name}")
}

proc plain_stage(name: Str) [error] -> Result[Str] {
  Ok(f"plain {name}")
}

pure locate(name: Str) -> Result[RelPath] {
  fp"etc/{name}".require()
}

proc run_stage(stage: Stage, name: Str) [error] -> Result[Str] {
  stage(name)
}

# The error position of a returned Result is covariant: a function that fails
# only with one family fits a type whose calls may fail with any error. The
# value position takes the same type, or a validated type where its base is
# written.
test test_function_with_a_family_error_fits_a_type_that_returns_any_error {
  let stage: Stage = strict_stage
  assert stage("a")? == "stage a"
  assert run_stage(strict_stage, "b")? == "stage b"
  assert run_stage(plain_stage, "c")? == "plain c"
  assert run_stage(strict_stage, "") is Err(_)

  let strict: StrictStage = strict_stage
  let widened: Stage = strict
  assert widened("d")? == "stage d"
  let chosen: Stage = if widened("e") is Ok(_) { strict_stage } else { plain_stage }
  assert chosen("f")? == "stage f"

  let find: Locate = locate
  assert find("hosts")? == "etc/hosts"
}

test test_callable_return_is_not_narrowed_or_retyped { |ctx|
  let stderr = check_errors(
    ctx,
    r"""error StageError = Missing(name: Str)
error OtherError = Lost(name: Str)

type Stage = proc(name: Str) [error] -> Result[Str]
type StrictStage = proc(name: Str) [error] -> Result[Str, StageError]
type Rel = pure(name: Str) -> Result[RelPath]

proc plain(name: Str) [error] -> Result[Str] {
  Ok(name)
}

proc other(name: Str) [error] -> Result[Str, OtherError] {
  Ok(name)
}

proc counted(name: Str) [error] -> Result[Int, StageError] {
  Ok(name.byte_len())
}

pure anywhere(name: Str) -> Result[Path] {
  Ok(Path(name))
}

proc select(stage: Stage) [error] -> Result[Unit] {
  let a: StrictStage = plain
  let b: StrictStage = other
  let c: Stage = counted
  let d: Rel = anywhere
  let e: StrictStage = stage
  return
}
""",
  )?
  assert count(stderr, "err[check.callable-mismatch]") == 4, stderr
  assert count(stderr, "err[check.type-mismatch]") == 1, stderr
  assert "it returns Result[Str, Error], expected Result[Str, StageError]" in stderr, stderr
  assert "it returns Result[Str, OtherError], expected Result[Str, StageError]" in stderr, stderr
  assert "it returns Result[Int, StageError], expected Result[Str, Error]" in stderr, stderr
  assert "it returns Result[Path, Error], expected Result[RelPath, Error]" in stderr, stderr
}

# A module's exported value of a callable type is called as `module.name(args)`
# with the type's labels, and its effect bound is charged to the caller.
test test_module_exported_callable_value_is_called_through_its_type { |ctx|
  let root = test.temp_dir(ctx, name: "typed-callable-module")?
  fp"{root}/nested".mkdir()
  fp"{root}/nested/handlers.xsh".write(r"""
##! Handlers chosen when the module loads.
## What a handler is called with and returns.
export type Handler = proc(name: Str) [fs, error] -> Result[Str, Error]

proc greet(name: Str) [error] -> Result[Str] {
  Ok(f"hello {name}")
}

## The handler in use.
export let handler: Handler = greet
""")
  let accepted = test.run_xsh(
    ctx,
    r"""use nested.handlers
use nested.handlers as callback_module

proc relay(name: Str) [fs, error] -> Result[Str] {
  callback_module.handler(name)
}

print handlers.handler("one")?
print callback_module.handler(name: "two")?
let kept: callback_module.Handler = callback_module.handler
print kept("three")?
print relay("four")?
""",
    env: {XSH_MODULE_PATH: root},
  )?
  assert accepted.success, accepted.stderr
  assert accepted.stdout == "hello one\nhello two\nhello three\nhello four\n", accepted.stdout

  let rejected = test.run_xsh(
    ctx,
    r"""use nested.handlers as callback_module

proc bounded(name: Str) [error] -> Result[Str] {
  callback_module.handler(name)
}

print callback_module.handler(1)?
print callback_module.handler(label: "two")?
print bounded("three")?
""",
    env: {XSH_MODULE_PATH: root},
  )?
  assert rejected.status == 2, rejected.stderr
  assert "expected Str, found Int" in rejected.stderr, rejected.stderr
  assert "err[check.named-arg]: unknown named parameter `label`" in rejected.stderr, rejected.stderr
  assert "effect `fs` required by `handler` is not in caller's declared effects" in rejected.stderr, rejected.stderr
}

test test_path_read_accepts_paths_returned_by_methods_and_try { |ctx|
  let root = test.temp_dir(ctx, name: "typed-callable-path-producers")?
  let file = fp"{root}/note"
  file.write("contents")

  # The receiver expressions have no stored callable fact; their Path type
  # comes from the Path method and fs.cwd() result contracts.
  var parent_read = ""
  with text = file.normalize().read_text() {
    parent_read = text
  } else { |failure|
    parent_read = failure.message
  }
  var current_directory_read = ""
  with text = fs.cwd()?.read_text() {
    current_directory_read = text
  } else { |failure|
    current_directory_read = failure.message
  }
  assert parent_read == "contents"
  assert current_directory_read != ""
}

test test_path_read_checks_each_inline_path_result { |ctx|
  let root = test.temp_dir(ctx, name: "typed-callable-path-branches")?
  let file = fp"{root}/note"
  file.write("contents")

  var conditional = ""
  with text = (if true { fp"{root}/note" } else { fp"{root}/note" }).read_text() {
    conditional = text
  } else { |failure|
    conditional = failure.message
  }
  assert conditional == "contents"

  var selected = ""
  with text = (match "selected" {
    "other" => fp"{root}/other"
    _ => fp"{root}/note"
  }).read_text() {
    selected = text
  } else { |failure|
    selected = failure.message
  }
  assert selected == "contents"

  let maybe_file: Path? = null
  var fallback = ""
  with text = (maybe_file ?? fp"{root}/note").read_text() {
    fallback = text
  } else { |failure|
    fallback = failure.message
  }
  assert fallback == "contents"

  var resolved = ""
  with text = (file.resolve() ?? fp"{root}/note").read_text() {
    resolved = text
  } else { |failure|
    resolved = failure.message
  }
  assert resolved == "contents"

  var projected = ""
  with text = {path: fp"{root}/note"}.path.read_text() {
    projected = text
  } else { |failure|
    projected = failure.message
  }
  assert projected == "contents"

  var updated_projection = ""
  with text = {...{path: fp"{root}/other"}, path: fp"{root}/note"}.path.read_text() {
    updated_projection = text
  } else { |failure|
    updated_projection = failure.message
  }
  assert updated_projection == "contents"

  let checked_holder: PathHolder = PathHolder(path: fp"{root}/note")
  var checked_projection = ""
  with text = checked_holder.path.read_text() {
    checked_projection = text
  } else { |failure|
    checked_projection = failure.message
  }
  assert checked_projection == "contents"

  let indexed_paths = [fp"{root}/note"]
  var indexed = ""
  with text = indexed_paths[0].read_text() {
    indexed = text
  } else { |failure|
    indexed = failure.message
  }
  assert indexed == "contents"

  let named_paths: Map[Str, Path] = {note: fp"{root}/note"}
  var mapped = ""
  with text = named_paths["note"].read_text() {
    mapped = text
  } else { |failure|
    mapped = failure.message
  }
  assert mapped == "contents"

  var mutable_path: Path = fp"{root}/other"
  mutable_path = fp"{root}/note"
  var mutable_read = ""
  with text = mutable_path.read_text() {
    mutable_read = text
  } else { |failure|
    mutable_read = failure.message
  }
  assert mutable_read == "contents"
}

test test_with_keeps_registry_results_for_environment_path_methods {
  var outcome = ""
  with popped = env.PATH.pop() {
    outcome = popped.display()
  } else { |failure|
    outcome = failure.message
  }
  assert outcome != ""
}

test test_with_try_path_results_keep_registry_error_types { |ctx|
  let root = test.temp_dir(ctx, name: "typed-callable-path-error-types")?
  let file = fp"{root}/file"
  file.write("contents")
  let link = fp"{root}/link"
  fs.symlink(file, link)

  var resolved = ""
  with resolved_path = link.resolve()? {
    resolved = resolved_path.display()
  } else { |failure|
    resolved = failure.message
  }
  assert resolved == file.resolve()?.display()

  var link_target = ""
  with target_path = link.readlink()? {
    link_target = target_path.display()
  } else { |failure|
    link_target = failure.message
  }
  assert link_target == file.display()

  var relative_link = ""
  with stripped = link.strip_prefix(root)? {
    relative_link = stripped.display()
  } else { |failure|
    relative_link = failure.message
  }
  assert relative_link == "link"

  let callbacks: List[Scale] = [double]
  with listed = callbacks.get(0)? {
    assert listed(4) == 8
  } else { |failure| test.fail(failure.message) }

  let callback_result: Result[Scale] = Ok(double)
  with contextual = callback_result.context("typed-callee")? {
    assert contextual(4) == 8
  } else { |failure| test.fail(failure.message) }

  with parsed = "12" as Int {
    assert parsed == 12
  } else { |failure| test.fail(failure.message) }
}
