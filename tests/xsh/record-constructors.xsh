const record_default_names = ["initial"]

type ConstructorConfig = {
  name: Str,
  enabled: Bool = true,
  names: List[Str] = record_default_names,
  options: Map[Str] = {},
}

type ConstructorAlias = ConstructorConfig

pure constructor_config(name: Str) -> ConstructorConfig {
  ConstructorConfig(name:)
}

test test_record_constructors_defaults_aliases_and_puns {
  let name = "demo"
  let config = ConstructorAlias(name:)
  assert constructor_config("pure").name == "pure"
  assert config.name == "demo"
  assert config.enabled == true
  assert config.names == ["initial"]
  assert config.options.len() == 0
  assert ConstructorConfig(name: "explicit", options: {}).options.len() == 0
  var first = ConstructorConfig(name: "first")
  let second = ConstructorConfig(name: "second")
  first.names += ["changed"]
  var options = first.options
  options["changed"] = "value"
  assert second.names == ["initial"]
  assert second.options.len() == 0
  assert record_default_names == ["initial"]
}

test test_record_constructors_evaluate_supplied_arguments_in_source_order { |ctx|
  let executed = test.run_script(
    ctx,
    r"""type Pair = {first: Int = 0, second: Int = 0}
proc marked(value: Int) -> Int {
  print $value
  return value
}
let pair = Pair(second: marked(2), first: marked(1))
print $pair.first
print $pair.second
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert executed.stdout == """2
1
1
2
"""
}

test test_record_constructors_reject_invalid_calls_and_defaults { |ctx|
  for source in [
    """type Config = {name: Str}
let config = Config()
""",
    """type Config = {name: Str}
let config = Config("demo")
""",
    """type Config = {name: Str}
let config = Config(name: "demo", other: 1)
""",
    """type Config = {name: Str}
let config = Config(name: "demo", name: "again")
""",
  ] {
    let rejected = test.run_script(ctx, source)?
    {
      let assertion_condition = ! rejected.success
      let assertion_message = rejected.stderr
      assert assertion_condition, assertion_message
    }
    assert "check.record-constructor" in rejected.stderr
  }

  for source in [
    """type Config = {name: Str = env.get("HOME")}
""",
    """var name = "demo"
type Config = {name: Str = name}
""",
    """type Config = {first: Int = 1, second: Int = first}
""",
    """type Config = {value: Int = (Ok(1))?}
""",
  ] {
    let rejected = test.run_script(ctx, source)?
    {
      let assertion_condition = ! rejected.success
      let assertion_message = rejected.stderr
      assert assertion_condition, assertion_message
    }
    assert "check.record-default" in rejected.stderr
  }

  let missing = test.run_script(
    ctx,
    """type Config = {name: Str = "demo"}
let config: Config = {}
""",
  )?
  {
    let assertion_condition = ! missing.success
    let assertion_message = missing.stderr
    assert assertion_condition, assertion_message
  }
}

test test_record_constructors_resolve_defaults_in_defining_module { |ctx|
  let root = test.temp_dir(ctx, name: "record-constructor-module")?
  fp"${root}/config.xsh".write_atomic("""##! Constructor defaults and aliases.
let name = "module"
## A configuration with lexical immutable defaults.
export type Config = {name: Str = name, nested: List[Int] = [1, 2]}
## An alias shares constructor defaults.
export type Alias = Config
## Uses the owning schema in parameter checks.
export pure render(value: Config) -> Str { value.name.upper() }
""")?
  fp"${root}/other.xsh".write_atomic("""##! Another schema with the same local name.
## Defaults belong to this schema.
export type Config = {name: Int = 5, values: List[Str] = ["other"]}
""")?
  let executed = test.run_script(
    ctx,
    r"""use config as c
use other as o
let name = "caller"
type Local = c.Alias
let first: c.Config = c.Config()
let second = c.Alias()
let third = Local()
let fourth = o.Config()
print ${fourth.values[0].upper()}
print $fourth.name
print ${first.name.upper()}
print ${c.render(first)}
let maybe: c.Config? = first
print ${maybe?.name ?? "none"}
let raw: Record = {name: "required", nested: [9]}
let checked = raw.require(c.Config)?
print ${checked.name.upper()}
print ${checked.nested[0]}
print $first.name
print $second.name
print $third.name
""",
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert executed.stdout == """OTHER
5
MODULE
MODULE
module
REQUIRED
9
module
module
module
"""
}

test test_record_constructor_defaults_do_not_change_require_or_json_validation { |ctx|
  let executed = test.run_script(
    ctx,
    r"""type Config = {name: Str = "demo"}
let raw: Record = {}
match raw.require(Config) {
  Err(_) => print "missing-record"
  Ok(_) => print "unexpected"
}
let decoded = json.decode("{}")?
match decoded.require(Config) {
  Err(_) => print "missing-json"
  Ok(_) => print "unexpected"
}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert executed.stdout == """missing-record
missing-json
"""
}

test test_record_constructor_tooling_preserves_behavior_and_converges { |ctx|
  let source = p"tests/fixtures/syntax/valid/record-constructor-explicit.xsh".read_text()?
  let before = test.run_script(ctx, source)?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = before
    assert assertion_condition, assertion_message
  }
  let candidate = test.temp_file(ctx, name: "record-constructor-fix.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --fix $candidate ?
  {
    let assertion_condition = applied.status.exited_with(0)
    let assertion_message = applied.stderr
    assert assertion_condition, assertion_message
  }
  let fixed = candidate.read_text()?
  assert "let config: Config = Config(name:)" in fixed
  assert "# Keep this field explanation." in fixed
  let after = test.run_script(ctx, fixed)?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = after
    assert assertion_condition, assertion_message
  }
  assert after.stdout == before.stdout
  let repeated = run.capture --text "xsht" lint --fix $candidate ?
  {
    let assertion_condition = repeated.status.exited_with(1)
    let assertion_message = repeated.stderr
    assert assertion_condition, assertion_message
  }
  assert "lint.prefer-record-constructor" in repeated.stderr
  assert candidate.read_text()? == fixed
}

test test_record_constructors_bound_scalar_defaults_and_static_identity { |ctx|
  let accepted = test.run_script(
    ctx,
    r"""let base_path: Path = p"base"
type Paths = {root: Path = base_path}
print ${Paths().root}
type Defaults = {integer: Int = -3, fraction: Float = -1.5, elapsed: Duration = 2s, data: Bytes = b"ok", location: Path = p"demo", maybe: Str? = null, items: List[Int] = []}
let value = Defaults()
let supplied = Defaults(location: p"supplied")
print ${supplied.location}
print $value.integer
print ${value.fraction.format(precision: 1)}
print ${value.data.utf8()?}
print ${value.location}
print ${value.items.len()}
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = accepted
    assert assertion_condition, assertion_message
  }
  assert accepted.stdout == """base
supplied
-3
-1.5
ok
demo
0
"""
  let first_class = test.run_script(
    ctx,
    """type Config = {name: Str = "demo"}
let factory = Config
""",
  )?
  {
    let assertion_condition = ! first_class.success
    let assertion_message = first_class.stderr
    assert assertion_condition, assertion_message
  }
  assert "check.unresolved-name" in first_class.stderr
  let collision = test.run_script(
    ctx,
    """type Config = {name: Str}
pure Config(name: Str) -> Str { name }
""",
  )?
  {
    let assertion_condition = ! collision.success
    let assertion_message = collision.stderr
    assert assertion_condition, assertion_message
  }
  assert "check.duplicate-name" in collision.stderr
  let wrong_default = test.run_script(
    ctx,
    """type Config = {name: Str = 1}
""",
  )?
  {
    let assertion_condition = ! wrong_default.success
    let assertion_message = wrong_default.stderr
    assert assertion_condition, assertion_message
  }
  assert "check.type-mismatch" in wrong_default.stderr
}
