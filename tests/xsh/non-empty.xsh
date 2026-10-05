# `NonEmpty[T]` is a `List[T]` known to hold an element. A value gets the type
# from a literal the checker can judge, from one validation at an explicit
# boundary, or from an operation that cannot empty a list; every other
# operation reads it as the list it is.

type Argv = NonEmpty[Str]

type Step = {argv: Argv, name: Str}

type Defaults = {argv: Argv = ["true"], name: Str}

enum Mode: Str { Fast = "fast", Slow = "slow" }

type Plan = {modes: NonEmpty[Mode]}

const SHELL: Argv = ["sh", "-c"]

pure program(argv: Argv) -> Str {
  argv.first()
}

pure ends(numbers: NonEmpty[Int]) -> Int {
  numbers.first() + numbers.last()
}

# An unannotated return keeps the validation an appending operation preserves.
pure with_flag(argv: Argv) {
  argv.push("--verbose")
}

pure validated(items: Any) -> Result[Str] {
  let names: Argv = items.require()?
  Ok(names.first())
}

proc takes_argv(argv: Argv) [error] -> Result[Str] {
  Ok(argv.last())
}

test test_non_empty_literal_and_guaranteed_operations {
  let argv: Argv = ["git", "status"]
  assert program(argv) == "git"
  assert argv.first() == "git"
  assert argv.last() == "status"
  assert ends([4]) == 8
  assert ends([1, 2, 3]) == 4
  assert SHELL.first() == "sh"
  assert Defaults(name: "noop").argv.first() == "true"

  let step = Step(["make", "all"], "build")
  assert step.argv.last() == "all"
  let nested: List[NonEmpty[Int]] = [[1], [2, 3]]
  var total = 0
  for members in nested {
    total += members.last()
  }

  assert total == 4

  let maybe: Argv? = ["only"]
  assert maybe != null and maybe.first() == "only"
}

test test_appending_and_mapping_keep_the_guarantee {
  let argv: Argv = ["git"]
  let pushed: Argv = argv.push("status")
  let extended: Argv = argv.extend([])
  let after: Argv = argv + ["log"]
  let before: Argv = ["env"] + argv
  let both: Argv = argv + pushed
  let upper: Argv = [word.upper() for word in pushed]
  let spread: Argv = [@argv]
  let prefixed: Argv = ["sudo", @spread]
  assert pushed.last() == "status"
  assert extended == ["git"]
  assert after.last() == "log"
  assert before.first() == "env"
  assert both.len() == 3
  assert upper == ["GIT", "STATUS"]
  assert prefixed == ["sudo", "git"]
  assert with_flag(argv).last() == "--verbose"

  var grown: Argv = ["a"]
  grown += ["b"]
  grown += []
  grown[0] = "z"
  assert grown.first() == "z" and grown.last() == "b"
}

# What does not obviously keep an element gives the list back.
test test_other_operations_read_the_value_as_its_list {
  let argv: Argv = ["tar", "-c", "-f"]
  let plain: List[Str] = argv
  let rest = argv[1..]
  let flags = [word for word in argv if word.starts_with("-")]
  let copied = argv.collect()
  let lists: List[List[Str]] = [argv]
  let any: Any = argv
  assert plain == argv
  assert rest == ["-c", "-f"]
  assert flags.len() == 2
  assert copied.len() == 3
  assert lists.len() == 1
  assert any == argv
  assert argv[0] == "tar"
  assert argv.len() == 3
  assert argv.get(9) is Err(_)
  assert argv.join(" ") == "tar -c -f"
  assert "-c" in argv
  assert "x" not in argv

  var seen = ""
  for index, word in argv {
    seen = f"{seen}{index}{word}"
  }

  assert seen == "0tar1-c2-f"
  assert (argv |> map { |word| word.byte_len() } |> collect()) == [3, 2, 2]

  let shape = match argv {
    [] => "empty",
    [only] => f"one {only}",
    [head, ..tail] => f"{head} and {tail.len()}",
  }
  assert shape == "tar and 2"
}

# The one runtime check runs at the conversion: `.require`, a type test, or a
# type pattern. A failed validation is ordinary data.
test test_validation_runs_once_at_the_explicit_boundary {
  let plain = ["ls", "-l"]
  let checked = plain.require(Argv)?
  assert checked.first() == "ls"

  let empty: List[Str] = []
  match empty.require(NonEmpty[Str]) {
    Ok(_) => assert false, "an empty list was validated"
    Err(error) => assert "expected NonEmpty[Str], found an empty list" in error.message, error.message
  }

  assert plain is NonEmpty[Str]
  assert ! (empty is NonEmpty[Str])
  if plain is NonEmpty[Str] {
    assert plain.last() == "-l"
  }

  let described = if let known is NonEmpty[Str] = plain { known.first() } else { "nothing" }
  assert described == "ls"

  assert validated(["z"])? == "z"
  assert validated([]) is Err(_)
  assert validated([1]) is Err(_)
  assert validated("z") is Err(_)
}

test test_non_empty_slots_in_schemas_validate_and_convert {
  let step = json.decode("{\"argv\": [\"ls\"], \"name\": \"list\"}")?.require(Step)?
  assert step.argv.first() == "ls"

  match json.decode("{\"argv\": [], \"name\": \"list\"}")?.require(Step) {
    Ok(_) => assert false, "an empty argv was validated"
    Err(error) => assert "schema check failed at argv: expected NonEmpty[Str], found an empty list" in error.message, error.message
  }

  # A failure inside the list is reported at its own path.
  match json.decode("{\"argv\": [\"ls\", 2], \"name\": \"list\"}")?.require(Step) {
    Ok(_) => assert false, "a mistyped element was validated"
    Err(error) => assert "argv[1]" in error.message, error.message
  }

  # The base schema still converts wire strings before the validation runs.
  let plan = json.decode("{\"modes\": [\"fast\", \"slow\"]}")?.require(Plan)?
  assert plan.modes.first() == Fast
  assert plan.modes.last() == Slow
  assert json.decode("{\"modes\": []}")?.require(Plan) is Err(_)
}

test test_union_member_is_decided_by_the_validation {
  let names: Argv = ["x"]
  let word: Union[NonEmpty[Str], Int] = names
  assert word is NonEmpty[Str]
  if word is NonEmpty[Str] {
    assert word.first() == "x"
  }

  let dynamic: Any = []
  assert ! (dynamic is NonEmpty[Str])
  assert dynamic.require(Union[NonEmpty[Str], Int]) is Err(_)
}

test test_run_accepts_a_non_empty_command_vector {
  let argv: Argv = ["printf", "%s:%s", "a", "b"]
  let short: Argv = ["printf", "%s:%s", "a"]
  let text = run.text @argv ?
  assert text == "a:b"
  let more = run.text @short "b" ?
  assert more == "a:b"
  # The remaining runs print nothing: their output is not captured, and the
  # test runner's own report shares that stream.
  let quiet: Argv = ["true"]
  let status = run.status @quiet
  assert status.success
  let piped = run.text @argv | run tr a-z A-Z ?
  assert piped == "A:B"
  let planned = process.run(process.command_argv(quiet.first(), quiet))?
  assert planned.success

  # A plain list is still accepted, and still fails at run time when empty.
  let none: List[Str] = []
  let outcome = try {
    run.text @none
  }
  match outcome {
    Ok(_) => assert false, "an empty command ran"
    Err(error) => assert error is ProcessError.InvalidTarget
  }
}

# A dynamic call passes an unchecked value to a checked parameter, so the
# parameter's type is tested where the call arrives.
test test_dynamic_call_tests_the_validation_at_the_parameter { |ctx|
  let callee = takes_argv
  assert callee.call(["a", "b"])? == "b"

  let output = test.run_script(
    ctx,
    r"""proc takes_argv(argv: NonEmpty[Str]) [error] -> Result[Str] {
  Ok(argv.last())
}

let callee: Proc = takes_argv
let empty: List[Str] = []
let result = callee.call(empty)
print ${result is Err(_)}
""",
  )?
  assert output.status == 3, f"{output.stdout}{output.stderr}"
  assert "expected NonEmpty[Str]" in output.stderr, output.stderr
}

# `first()` on a dynamic receiver has no checked guarantee and fails where it
# runs; this is also what a `NonEmpty[T]` operation does if an empty list ever
# reaches it.
test test_first_on_a_dynamic_empty_list_fails_at_run_time { |ctx|
  let output = test.run_script(
    ctx,
    r"""let dynamic: Any = []
let value = dynamic.first()
print ${value == null}
""",
  )?
  assert output.status == 3, f"{output.stdout}{output.stderr}"
  assert "index-out-of-bounds: `first()` needs a non-empty list, found an empty list" in output.stderr, output.stderr
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

test test_literal_that_may_be_empty_is_rejected { |ctx|
  let stderr = check_errors(
    ctx,
    r"""type Step = {argv: NonEmpty[Str], name: Str}

pure none() -> NonEmpty[Str] {
  []
}

pure fallback(argv: NonEmpty[Str] = ["true"]) -> Str {
  argv.first()
}

let plain: List[Str] = ["a"]
let empty: NonEmpty[Str] = []
let spread: NonEmpty[Str] = [@plain]
let step = Step(argv: [], name: "x")
let literal: Step = {argv: [], name: "x"}
var grown: NonEmpty[Str] = ["a"]
grown = []
print ${fallback([])} ${none().len()} ${empty.len()} ${spread.len()} ${step.name} ${literal.name}
""",
  )?
  assert count(stderr, "err[check.validated-literal]") == 7, stderr
  assert count(stderr, "err[") == 7, stderr
  assert "an empty list literal is not a NonEmpty[Str]" in stderr, stderr
  assert "this list literal may be empty, so it is not a NonEmpty[Str]" in stderr, stderr
  assert "write at least one element outside an `@` splice" in stderr, stderr
}

test test_constant_that_fails_the_validation_is_rejected { |ctx|
  let stderr = check_errors(
    ctx,
    r"""const NONE: NonEmpty[Str] = []
type Defaults = {argv: NonEmpty[Str] = [], name: Str}
print ${NONE.len()} ${Defaults(name: "x").name}
""",
  )?
  assert count(stderr, "err[check.const]") >= 1, stderr
  assert count(stderr, "err[check.validated-literal]") + count(stderr, "err[check.record-default]") >= 1, stderr
}

# A list is never a `NonEmpty` without the validation, wherever it comes from.
test test_unvalidated_list_never_fits_non_empty { |ctx|
  let stderr = check_errors(
    ctx,
    r"""pure program(argv: NonEmpty[Str]) -> Str {
  argv.first()
}

pure rest(argv: NonEmpty[Str]) -> NonEmpty[Str] {
  argv[1..]
}

let plain: List[Str] = ["a"]
let argv: NonEmpty[Str] = ["a"]
let direct: NonEmpty[Str] = plain
let filtered: NonEmpty[Str] = [word for word in argv if word != "a"]
let nested: NonEmpty[Str] = [word for word in argv for other in argv]
let numbers: NonEmpty[Int] = argv
let lists: List[List[Str]] = [argv]
let wrapped: List[NonEmpty[Str]] = lists
var kept: NonEmpty[Str] = ["a"]
kept = plain
kept += 1
let pushed = argv.push(1)
print ${program(plain)} ${plain.first()} ${plain.last()} ${rest(argv).len()}
print ${direct.len()} ${filtered.len()} ${nested.len()} ${numbers.len()} ${wrapped.len()} ${pushed.len()}
""",
  )?
  assert count(stderr, "err[check.type-mismatch]") == 10, stderr
  assert count(stderr, "err[check.unknown-method]") == 2, stderr
  assert count(stderr, "err[") == 12, stderr
  assert "expected NonEmpty[Str], found List[Str]" in stderr, stderr
  assert "validate it with `.require(NonEmpty[Str])?`" in stderr, stderr
  assert "`first()` is a method of NonEmpty[Str]" in stderr, stderr
  assert "expected NonEmpty[Int], found NonEmpty[Str]" in stderr, stderr
}

test test_non_empty_type_is_well_formed_only_where_it_can_be_checked { |ctx|
  let stderr = check_errors(
    ctx,
    r"""type Both = Union[NonEmpty[Str], List[Str]]

proc rest(...argv: NonEmpty[Str]) -> Int {
  argv.len()
}

let dynamic: Any = ["a"]
let unchecked: NonEmpty[Str] = dynamic
print ${rest("a")} ${unchecked.len()}
""",
  )?
  assert "every `NonEmpty[Str]` already fits the member `List[Str]`" in stderr, stderr
  assert "err[check.rest-type]" in stderr, stderr
  assert "err[check.dynamic-boundary]" in stderr, stderr
  assert "unchecked Any cannot establish NonEmpty[Str]" in stderr, stderr
}
