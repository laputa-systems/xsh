test test_callable_alias_retains_labels_defaults_and_alias_chain { |ctx|
  let result = test.run_script(
    ctx,
    """
pure render(value: Str, prefix: Str = "label:") -> Str { prefix + value }
let format = render
let again = format
print format("one") again(prefix: "item:", value: "two")
let fields = {value: "three", prefix: "name:"}
print again(...fields) format.call(value: "four")
""",
    [],
    {},
    b"",
    "callable-alias.xsh",
  )?
  assert result.success, result.stderr
  assert result.stdout == """label:one item:two
name:three label:four
"""
}

test test_callable_alias_preserves_capture_snapshot_and_initializer_timing { |ctx|
  let result = test.run_script(
    ctx,
    """
let prefix = "captured:"
pure render(value: Str) -> Str { prefix + value }
let format = render
print format(value: "one")
""",
    [],
    {},
    b"",
    "callable-alias-capture.xsh",
  )?
  assert result.success, result.stderr
  assert result.stdout == """captured:one
"""
}

type AliasApi = module {
  export pure format(value: Str, prefix: Str = "label:") -> Str
}

test test_callable_alias_exports_preserve_module_contracts { |ctx|
  let root = test.temp_dir(ctx, name: "callable-alias-exports")?
  fp"{root}/implementation.xsh".write("""
##! Callable implementation.
let captured = "private:"
## Formats a value.
export pure render(value: Str, prefix: Str = "label:") -> Str { captured + prefix + value }
""")?
  let api = fp"{root}/api.xsh"
  api.write("""
##! Callable aliases.
use implementation
## Public formatting alias.
export let format = implementation.render
""")?
  let loaded = module.load(api)?.require(AliasApi)?
  assert loaded.format(value: "one") == "private:label:one"
  let entry = fp"{root}/entry.xsh"
  entry.write("""
use api
let format = api.format
print format(prefix: "item:", value: "two")
""")?
  let result = test.run_xsh(ctx, entry.read_text()?, env: {XSH_MODULE_PATH: root.display()})?
  assert result.success, result.stderr
  assert result.stdout == """private:item:two
"""
}

test test_callable_alias_keeps_effects_and_erased_boundaries { |ctx|
  for source in [
    """pure inferred(value: Int) { value }; export let public = inferred
""",
    """proc effect() [env] -> Result[Unit] { env.set("ALIAS_TEST", "yes")? }; let alias = effect; pure invalid() -> Result[Unit] { alias() }
""",
    """pure render(value: Str) -> Str { value }; let erased: Pure = render; print erased(value: "one")
""",
    """pure render(value: Str) -> Str { value }; var selected = render; print selected(value: "one")
""",
    """let left = right; let right = left; print left()
""",
    """pure render(value: Str) -> Str { value }; const invalid = render
""",
  ] {
    let result = test.run_script(ctx, source, [], {}, b"", "callable-alias-rejected.xsh")?
    assert ! result.success, source
  }
}

test test_callable_alias_keeps_lexical_shadowing_and_captured_aliases { |ctx|
  let result = test.run_script(
    ctx,
    """
pure increment(value: Int) -> Int { value + 1 }
pure ten_more(value: Int) -> Int { value + 10 }
let selected = increment
proc choose(local: Bool) [] -> Int {
  if local { let selected = ten_more; return selected(value: 1) }
  selected(value: 1)
}
print choose(true) choose(false) selected(value: 2)
""",
    [],
    {},
    b"",
    "callable-alias-shadow.xsh",
  )?
  assert result.success, result.stderr
  assert result.stdout == """11 2 3
"""
}

test test_callable_alias_retains_argument_order_and_typed_conversions { |ctx|
  let result = test.run_script(
    ctx,
    """
proc mark(value: Int) [io] -> Int { print $value; value }
proc combine(left: Int, right: Int = 10) [] -> Int { left + right }
let sum = combine
print sum(right: mark(2), left: mark(1)) sum(left: 3)
pure file(value: Path) -> Str { value.display() }
let file_name = file
print file_name(value: p"config")
pure parse(value: Str) -> Result[Int] { value.parse_int() }
let parsed = parse
print (parsed(value: "4")?)
""",
    [],
    {},
    b"",
    "callable-alias-order.xsh",
  )?
  assert result.success, result.stderr
  assert result.stdout == """2
1
3 13
config
4
"""
}

test test_callable_alias_retains_checked_module_projection_signature { |ctx|
  let root = test.temp_dir(ctx, name: "callable-alias-projection")?
  fp"{root}/plugin.xsh".write("""
##! Callable plugin.
let prefix = "captured:"
## Formats an exported value.
export pure render(value: Str, suffix: Str = "!") -> Str { prefix + value + suffix }
""")?
  let script = f"""
type Plugin = module {{ export pure render(value: Str, suffix: Str = "!") -> Str }}
let plugin = module.load(p"{root}/plugin.xsh")?.require(Plugin)?
let format = plugin.get("render")?
let again = format
print format(value: "one") again.call(suffix: "?", value: "two")
let projected = ["three"] |> map(format)
print projected[0]
"""
  let result = test.run_script(ctx, script, [], {}, b"", "callable-alias-projection.xsh")?
  assert result.success, result.stderr
  assert result.stdout == """captured:one! captured:two?
captured:three!
"""
}

test test_callable_alias_retains_inferred_proc_effect_identity { |ctx|
  let result = test.run_script(
    ctx,
    """
proc increment(value: Int) -> Int { value + 1 }
let invoke = increment
proc forward(value: Int) -> Int { invoke(value: value) }
proc bounded(value: Int) [] -> Int { forward(value) }
print bounded(4)
""",
    [],
    {},
    b"",
    "callable-alias-inferred-effects.xsh",
  )?
  assert result.success, result.stderr
  assert result.stdout == """5
"""
  for source in [
    """proc effect() -> Int { let _ = time.now(); 1 }; let invoke = effect; proc bounded() [] -> Int { invoke() }; print bounded()
""",
    """proc inferred(value: Int) -> Int { value }; export let public = inferred
""",
  ] {
    let rejected = test.run_script(ctx, source, [], {}, b"", "callable-alias-inferred-effects-rejected.xsh")?
    assert ! rejected.success, source
  }
}

test test_proc_alias_declared_inside_a_body_calls_the_proc { |ctx|
  # Inside a body the alias used to be prepared as a pure function value,
  # so the checked call failed at runtime as an unresolved call.
  let result = test.run_script(
    ctx,
    r"""proc helper(value: Int, step: Int = 2) -> Int { value + step }
pure double(value: Int) -> Int { value * 2 }
proc body() [] -> Result[Int] {
  try {
    let invoke = helper
    let twice = double
    invoke(1, step: twice(3))
  }
}
print ${body()?}
""",
  )?
  assert result.success, result.stderr
  assert result.stdout == "7\n"
}

test test_callable_alias_retains_stream_stage_signature { |ctx|
  let result = test.run_script(
    ctx,
    r"""
let prefix = "item:"
pure render(value: Str, suffix: Str = "!") -> Str { prefix + value + suffix }
let format = render
let values = ["one", "two"] |> map(format)
print values[0] values[1]
proc increment(value: Int) -> Int { value + 1 }
let next = increment
proc bounded() [] -> List[Int] { [1, 2] |> map(next) }
print ${bounded()[0]}
""",
    [],
    {},
    b"",
    "callable-alias-stage.xsh",
  )?
  assert result.success, result.stderr
  assert result.stdout == """item:one! item:two!
2
"""
}
