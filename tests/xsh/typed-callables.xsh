# A callable type, `proc(PARAMS) [EFFECTS] -> T` or `pure(PARAMS) -> T`, keeps
# a signature and an effect bound on a callable that is chosen at run time. A
# value gets the type only from a function whose signature fits, a call
# through it is checked like a call by name, and it stays a plain `Proc` or
# `Pure` handle at run time.

type Builder = proc(root: Path) [fs, process, error] -> Result[Str]

type Scale = pure(n: Int) -> Int

type Join = pure(left: Str, right: Str) -> Str

type Step = {name: Str, build: Builder, scale: Scale}

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

pure join(left: Str, right: Str) -> Str {
  left + "-" + right
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
  let step = Step(name: "release", build: release_build, scale: double)
  assert step.build(/a)? == "release /a false"
  assert step.scale(21) == 42

  let debug = Step(name: "debug", build: debug_build, scale: triple)
  assert debug.build(root: /b)? == "debug /b"
  assert debug.scale(n: 2) == 6

  let field = step.build
  assert field(/c)? == "release /c false"
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

test test_alias_that_kept_its_signature_fits_a_callable_type {
  let alias = release_build
  let build: Builder = alias
  let chosen: Builder = if true { alias } else { debug_build }
  assert build(/a)? == "release /a false"
  assert chosen(/b)? == "release /b false"
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
