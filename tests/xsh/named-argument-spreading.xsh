type SpreadFirst = {first: Int}

type SpreadOptions = {first: Int, second: Int = 20, label: Str? = "default"}

pure spread_sum(first: Int, second = 20) -> Int {
  first + second
}

pure spread_middle(first: Int, second = 20, third = 30) -> Int {
  first + second + third
}

pure spread_map_default(options: Map[Int] = {}, count = 3) -> Int {
  options.len() + count
}

pure spread_label(first: Int, label: Str? = "default") -> Str? {
  let _ = first
  label
}

pure spread_rest(first: Int, ...items: List[Int]) -> List[Int] {
  [first, @items]
}

test test_named_argument_spreads_use_visible_fields_and_constructor_defaults {
  let options = {first: 2, second: 3}
  assert spread_sum(...options) == 5
  assert spread_sum(...{first: 4}) == 24
  assert spread_middle(...{first: 1, third: 3}) == 24
  assert spread_map_default(...{count: 4}) == 4
  assert spread_sum(...{second: 5}, first: 2) == 7
  assert spread_sum(2, ...{second: 5}) == 7
  assert spread_sum(...{first: 2}, ...{second: 5}) == 7
  let first = 2
  assert spread_sum(first:, ...{second: 5}) == 7
  let original = {first: 2, second: 99}
  let selected: SpreadFirst = original
  assert spread_sum(...selected) == 22
  let config = SpreadOptions(...{first: 8, label: null})
  assert config.second == 20
  assert config.label == null
  assert spread_label(...{first: 1, label: null}) == null
}

test test_named_argument_spreads_evaluate_entries_once_in_source_order { |ctx|
  let executed = test.run_script(
    ctx,
    r"""type Pair = {first: Int, second: Int}
proc options() -> Pair {
  print spread
  return {first: 2, second: 3}
}

proc marked(value: Int) -> Int {
  print $value
  return value
}
pure sum(first: Int, second: Int, third: Int) -> Int { first + second + third }
print ${sum(third: marked(7), ...options())}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert executed.stdout == """7
spread
12
"""
}

test test_named_argument_spreads_keep_defaults_in_the_declaration_environment { |ctx|
  let output = test.expect(
    ctx,
    r"""type Third = {third: Int}
const lexical = 4
pure total(left: Int = lexical, right: Int = 2, third: Int = 3) -> Int { left + right + third }
proc marked(value: Int) [io] -> Int { print $value; value }
proc options() [io] -> Third { print spread; {third: 9} }
proc caller() [io] -> Int {
  let lexical = 100
  let _ = lexical
  total(right: marked(7), ...options())
}
print ${caller()}
""",
    status: 0,
  )?
  assert output.stdout == """7
spread
20
"""
}

test test_named_argument_spreads_reject_unknown_duplicate_and_dynamic_shapes { |ctx|
  for source in [
    """pure f(first: Int) -> Int { first }
f(...{other: 2})
""",
    """pure f(first: Int) -> Int { first }
f(first: 1, ...{first: 2})
""",
    """pure f(first: Int) -> Int { first }
f(1, ...{first: 2})
""",
    """pure f(first: Int) -> Int { first }
f(...{first: 1}, ...{first: 2})
""",
    """pure f(first: Int) -> Int { first }
let value: Any = {first: 1}
f(...value)
""",
    """pure f(first: Int) -> Int { first }
let value: Record = {first: 1}
f(...value)
""",
    """pure f(first: Int) -> Int { first }
let value: Map[Int] = {first: 1}
f(...value)
""",
    """pure f(first: Int) -> Int { first }
type Shape = {first: Int}
let value: Shape? = {first: 1}
f(...value)
""",
    """pure f(first: Int) -> Int { first }
type Shape = {first: Int}
let value: Result[Shape] = Ok({first: 1})
f(...value)
""",
    """pure f(first: Int, second: Int) -> Int { first + second }
type Shape = {first: Int}
var value: Shape? = {first: 1}
if value != null { f(second: if true { value = null; 2 } else { 2 }, ...value) }
""",
    """let failed = error.fail(...{other: "wrong label"})
""",
  ] {
    let rejected = test.run_script(ctx, source)?
    {
      let assertion_condition = ! rejected.success
      let assertion_message = source
      assert assertion_condition, assertion_message
    }
    assert "check." in rejected.stderr
  }
}

test test_named_argument_spreads_support_modules_methods_and_rest {
  assert "abc".replace(...{from: "b", with: "X"}) == "aXc"
  assert shlex.join(...{argv: ["a", "b"]}) == "a b"
  assert spread_rest(...{first: 1}, 2, 3) == [1, 2, 3]
  let tail = [2, 3]
  assert spread_rest(...{first: 1}, @tail) == [1, 2, 3]
}

test test_named_argument_spreads_project_before_later_mutation {
  var options = {first: 1}
  let total = spread_sum(
    ...options,
    second: if true {
      let next = 2
      options.first = 99
      next
    } else {
      0
    },
  )
  assert total == 3
  assert options.first == 99
}

test test_named_argument_spreads_evaluate_receiver_first_and_stop_on_failure { |ctx|
  let ordered = test.run_script(
    ctx,
    r"""type ReplaceOptions = {from: Str, with: Str}
proc receiver() -> Str { print receiver; return "abc" }
proc options() -> ReplaceOptions { print options; return ReplaceOptions(from: "b", with: "X") }
print ${receiver().replace(...options())}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = ordered
    assert assertion_condition, assertion_message
  }
  assert ordered.stdout == """receiver
options
aXc
"""
  let stopped = test.run_script(
    ctx,
    r"""type Pair = {first: Int, second: Int}
error SpreadFailure = Bad(message: Str)
proc options() -> Result[Pair] { print failed; return Err(SpreadFailure.Bad(message: "stop")) }
proc later() -> Int { print forbidden; return 3 }
pure total(first: Int, second: Int, third: Int) -> Int { first + second + third }
print ${total(...options()?, third: later())}
""",
  )?
  {
    let assertion_condition = ! stopped.success
    let assertion_message = stopped.stderr
    assert assertion_condition, assertion_message
  }
  {
    let assertion_condition = "SpreadFailure.Bad" in stopped.stderr
    let assertion_message = stopped.stderr
    assert assertion_condition, assertion_message
  }
  assert stopped.stdout == """failed
"""
}

error SpreadPayload = Bad(message: Str, code: Int)

test test_named_argument_spreads_support_static_error_payloads {
  let failure = SpreadPayload.Bad(...{code: 3, message: "supplied"})
  if let .Bad {code: code, message: message} = failure {
    assert code == 3
    assert message == "supplied"
  } else {
    test.fail("expected supplied error payload")
  }
}

type SpreadModule = module {
  export pure total(first: Int, second: Int = 20, third: Int = 30) -> Int
}

test test_named_argument_spreads_bind_checked_loaded_module_contracts { |ctx|
  let root = test.temp_dir(ctx, name: "named-spread-module")?
  let source_path = fp"{root}/math.xsh"
  source_path.write("""##! Checked static argument fixture.
## Adds supplied values and lexical defaults.
export pure total(first: Int, second: Int = 20, third: Int = 30) -> Int { first + second + third }
""")
  let loaded = module.load(source_path)?.require(SpreadModule)?
  assert loaded.total(...{first: 1, third: 3}) == 24
}

test test_named_argument_spreads_native_method_signatures {
  assert "alphabet".starts_with(...{prefix: "alpha"}) == true
  assert "alphabet".ends_with(...{suffix: "bet"}) == true
  assert "alphabet".find(...{needle: "pha"}) == 2
  let failure = error.fail(...{message: "spread failure"})
  assert failure is Err(_) == true
}

test test_named_argument_spreads_preserve_native_omitted_slots { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/payload.txt"
  let compressed = fp"{root}/payload.gz"
  let restored = fp"{root}/restored.txt"
  source.write("spread defaults")
  compressed.write("replace this")
  restored.write("replace this")
  archive.compress(source, compressed, ...{overwrite: true})
  archive.decompress(compressed, restored, ...{overwrite: true})
  assert restored.read_text()? == "spread defaults"
}
