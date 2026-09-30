type SpreadFirst = {first: Int}
type SpreadOptions = {first: Int, second: Int = 20, label: Str? = "default"}
pure spread_sum(first: Int, second: Int = 20) -> Int { first + second }
pure spread_middle(first: Int, second: Int = 20, third: Int = 30) -> Int { first + second + third }
pure spread_map_default(options: Map[Int] = {}, count: Int = 3) -> Int { options.len() + count }
pure spread_label(first: Int, label: Str? = "default") -> Str? { label }
pure spread_rest(first: Int, ...items: List[Int]) -> List[Int] { [first, @items] }

test test_named_argument_spreads_use_visible_fields_and_constructor_defaults [error] {
  let options = {first: 2, second: 3}
  test.eq(spread_sum(...options), 5)?
  test.eq(spread_sum(...{first: 4}), 24)?
  test.eq(spread_middle(...{first: 1, third: 3}), 24)?
  test.eq(spread_map_default(...{count: 4}), 4)?
  test.eq(spread_sum(...{second: 5}, first: 2), 7)?
  test.eq(spread_sum(2, ...{second: 5}), 7)?
  test.eq(spread_sum(...{first: 2}, ...{second: 5}), 7)?
  let first = 2
  test.eq(spread_sum(first:, ...{second: 5}), 7)?
  let original = {first: 2, second: 99}
  let selected = original.require(SpreadFirst)?
  test.eq(spread_sum(...selected), 22)?
  let config = SpreadOptions(...{first: 8, label: null})
  test.eq(config.second, 20)?
  test.eq(config.label, null)?
  test.eq(spread_label(...{first: 1, label: null}), null)?
}

test test_named_argument_spreads_evaluate_entries_once_in_source_order [error] { |ctx|
  let executed = test.run_script(ctx, r"""type Pair = {first: Int, second: Int}
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
""")?
  test.ok(executed.success, executed.stderr)?
  test.eq(executed.stdout, "7\nspread\n12\n")?
}

test test_named_argument_spreads_reject_unknown_duplicate_and_dynamic_shapes [error] { |ctx|
  for source in [
    "pure f(first: Int) -> Int { first }\nf(...{other: 2})\n",
    "pure f(first: Int) -> Int { first }\nf(first: 1, ...{first: 2})\n",
    "pure f(first: Int) -> Int { first }\nf(1, ...{first: 2})\n",
    "pure f(first: Int) -> Int { first }\nf(...{first: 1}, ...{first: 2})\n",
    "pure f(first: Int) -> Int { first }\nlet value: Any = {first: 1}\nf(...value)\n",
    "pure f(first: Int) -> Int { first }\nlet value: Record = {first: 1}\nf(...value)\n",
    "pure f(first: Int) -> Int { first }\nlet value: Map[Int] = {first: 1}\nf(...value)\n",
    "pure f(first: Int) -> Int { first }\ntype Shape = {first: Int}\nlet value: Shape? = {first: 1}\nf(...value)\n",
    "pure f(first: Int) -> Int { first }\ntype Shape = {first: Int}\nlet value: Result[Shape] = Ok({first: 1})\nf(...value)\n",
    "pure f(first: Int, second: Int) -> Int { first + second }\ntype Shape = {first: Int}\nvar value: Shape? = {first: 1}\nif value != null { f(second: if true { value = null; 2 } else { 2 }, ...value) }\n",
    "let failed = error.fail(...{other: \"wrong label\"})\n",
  ] {
    let rejected = test.run_script(ctx, source)?
    test.ok(! rejected.success, source)?
    test.ok("check." in rejected.stderr)?
  }
}

test test_named_argument_spreads_support_modules_methods_and_rest [error] {
  test.eq("abc".replace(...{from: "b", to: "X"}), "aXc")?
  test.eq(shlex.join(...{argv: ["a", "b"]}), "a b")?
  test.eq(spread_rest(...{first: 1}, 2, 3), [1, 2, 3])?
  let tail = [2, 3]
  test.eq(spread_rest(...{first: 1}, @tail), [1, 2, 3])?
}

test test_named_argument_spreads_project_before_later_mutation [error] {
  var options = {first: 1}
  let total = spread_sum(...options, second: if true {
    let next = 2
    options.first = 99
    next
  } else { 0 })
  test.eq(total, 3)?
  test.eq(options.first, 99)?
}

test test_named_argument_spreads_evaluate_receiver_first_and_stop_on_failure [error] { |ctx|
  let ordered = test.run_script(ctx, r"""type ReplaceOptions = {from: Str, to: Str}
proc receiver() -> Str { print receiver; return "abc" }
proc options() -> ReplaceOptions { print options; return ReplaceOptions(from: "b", to: "X") }
print ${receiver().replace(...options())}
""")?
  test.ok(ordered.success, ordered.stderr)?
  test.eq(ordered.stdout, "receiver\noptions\naXc\n")?
  let stopped = test.run_script(ctx, r"""type Pair = {first: Int, second: Int}
error SpreadFailure = Bad(message: Str)
proc options() -> Result[Pair] { print failed; return Err(SpreadFailure.Bad(message: "stop")) }
proc later() -> Int { print forbidden; return 3 }
pure total(first: Int, second: Int, third: Int) -> Int { first + second + third }
print ${total(...(options()?), third: later())}
""")?
  test.ok(! stopped.success, stopped.stderr)?
  test.ok("SpreadFailure.Bad" in stopped.stderr, stopped.stderr)?
  test.eq(stopped.stdout, "failed\n")?
}

error SpreadPayload = Bad(message: Str, code: Int)
test test_named_argument_spreads_support_static_error_payloads [error] {
  let failure = SpreadPayload.Bad(...{code: 3, message: "supplied"})
  match failure {
    SpreadPayload.Bad {code: code, message: message} => {
      test.eq(code, 3)?
      test.eq(message, "supplied")?
    }
    _ => test.fail("expected supplied error payload")?,
  }
}

type SpreadModule = module {
  export pure total(first: Int, second: Int = 20, third: Int = 30) -> Int
}
test test_named_argument_spreads_bind_checked_loaded_module_contracts [fs, error] { |ctx|
  let root = test.temp_dir(ctx, name: "named-spread-module")?
  let source_path = fp"${root}/math.xsh"
  source_path.write("""##! Checked static argument fixture.
## Adds supplied values and lexical defaults.
export pure total(first: Int, second: Int = 20, third: Int = 30) -> Int { first + second + third }
""")?
  let loaded = module.load(source_path)?.require(SpreadModule)?
  test.eq(loaded.total(...{first: 1, third: 3}), 24)?
}

test test_named_argument_spreads_native_method_signatures [error] {
  test.eq("alphabet".starts_with(...{prefix: "alpha"}), true)?
  test.eq("alphabet".ends_with(...{suffix: "bet"}), true)?
  test.eq("alphabet".find(...{needle: "pha"}), 2)?
  let failure = error.fail(...{message: "spread failure"})
  test.eq(failure is Err(_), true)?
}

test test_named_argument_spreads_preserve_native_omitted_slots [fs, error] { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"${root}/payload.txt"
  let compressed = fp"${root}/payload.gz"
  let restored = fp"${root}/restored.txt"
  source.write("spread defaults")?
  compressed.write("replace this")?
  restored.write("replace this")?
  archive.compress(source, compressed, ...{overwrite: true})?
  archive.decompress(compressed, restored, ...{overwrite: true})?
  test.eq(restored.read_text()?, "spread defaults")?
}
