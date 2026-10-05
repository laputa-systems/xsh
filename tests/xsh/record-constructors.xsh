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
  assert config.options.is_empty()
  assert ConstructorConfig(name: "explicit", options: {}).options.is_empty()
  var first = ConstructorConfig(name: "first")
  let second = ConstructorConfig(name: "second")
  first.names += ["changed"]
  var options = first.options
  options["changed"] = "value"
  assert second.names == ["initial"]
  assert second.options.is_empty()
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
let config = Config("demo", "extra")
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
  fp"{root}/config.xsh".write_atomic("""##! Constructor defaults and aliases.
let name = "module"
## A configuration with lexical immutable defaults.
export type Config = {name: Str = name, nested: List[Int] = [1, 2]}
## An alias shares constructor defaults.
export type Alias = Config
## Uses the owning schema in parameter checks.
export pure render(value: Config) -> Str { value.name.upper() }
""")
  fp"{root}/other.xsh".write_atomic("""##! Another schema with the same local name.
## Defaults belong to this schema.
export type Config = {name: Int = 5, values: List[Str] = ["other"]}
""")
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
    {XSH_MODULE_PATH: root},
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

test test_record_constructors_fill_positional_fields_in_declaration_order { |ctx|
  let executed = test.expect(
    ctx,
    r"""enum Kind { File, Binary }
type Entry = {path: Path, kind: Kind, mode: Int = 420, tags: List[Str] = []}
type Named = {label: Str, count: Int}

proc marked(value: Int) -> Int {
  print $value
  value
}

const shipped: List[Entry] = [Entry(p"usr/bin/xsh", Binary, mode: 493), Entry(p"etc/xsh.conf", File)]
let tool = Entry(p"usr/bin/xsht", .Binary)
let tagged = Entry(p"etc/motd", File, tags: ["doc"], mode: 384)
let counted = Named("hits", marked(2))
print f"{shipped[0].path} {shipped[0].mode} {shipped[1].mode}"
print f"{tool.path} {tool.kind == Binary} {tool.mode} {tool.tags.len()}"
print f"{tagged.mode} {tagged.tags[0]}"
print f"{counted.label}={counted.count}"
""",
    status: 0,
  )?
  assert executed.stdout == """2
usr/bin/xsh 493 420
usr/bin/xsht true 420 0
384 doc
hits=2
"""
}

test test_record_constructors_reject_positional_fields_that_could_swap { |ctx|
  for case in [
    {
      source: """type Pair = {first: Int, second: Int}
let pair = Pair(1, 2)
""",
      message: "fields `first` and `second` can hold the same value",
    },
    {
      source: """type Home = {name: Str, home: Path}
let home = Home("root", "/root")
""",
      message: "fields `name` and `home` can hold the same value",
    },
    {
      source: """type Limit = {count: Int, limit: UInt}
let limit = Limit(1, 2)
""",
      message: "fields `count` and `limit` can hold the same value",
    },
    {
      source: """type Label = {name: Str, label: Str?}
let label = Label("a", null)
""",
      message: "fields `name` and `label` can hold the same value",
    },
    {
      source: """type Lists = {names: List[Str], sizes: List[Int]}
let lists = Lists(["a"], [1])
""",
      message: "fields `names` and `sizes` can hold the same value",
    },
    {
      source: """type Name = Str
type Alias = {first: Name, second: Str}
let alias = Alias("a", "b")
""",
      message: "fields `first` and `second` can hold the same value",
    },
    {
      source: """type Box[T] = {value: T, label: Str}
let box = Box(1, "one")
""",
      message: "fields `value` and `label` can hold the same value",
    },
    {
      source: """type Pair = {first: Int, second: Int}
const pair = Pair(1, 2)
""",
      message: "fields `first` and `second` can hold the same value",
    },
    {
      source: """type Entry = {name: Str, size: Int}
let entry = Entry(name: "a", 1)
""",
      message: "positional constructor arguments must come before named ones",
    },
    {
      source: """type Entry = {name: Str, size: Int}
let entry = Entry("a", 1, 2)
""",
      message: "too many positional constructor arguments",
    },
    {
      source: """type Entry = {name: Str, size: Int}
let entry = Entry("a", name: "b")
""",
      message: "duplicate constructor field",
    },
    {
      source: """type Entry = {name: Str, size: Int}
let base = {size: 1}
let entry = Entry("a", ...base)
""",
      message: "takes only named arguments",
    },
  ] {
    let rejected = test.run_script(ctx, case.source)?
    assert ! rejected.success, case.source
    assert case.message in rejected.stderr, rejected.stderr
  }
}

test test_prefer_positional_constructor_fix_preserves_behavior_and_converges { |ctx|
  let source = r"""enum Kind { File, Binary }

type Entry = {path: Path, kind: Kind, mode: Int = 420}

type Pair = {first: Int, second: Int}

proc marked(file: Path) -> Path {
  print $file
  file
}

let tool = Entry(path: marked(p"usr/bin/xsh"), kind: Binary, mode: 493)
let file = p"etc/conf"
let kind = File
let config = Entry(path: file, kind:)
let reordered = Entry(kind: File, path: p"etc/other")
let pair = Pair(first: 1, second: 2)
print f"{tool.path} {tool.mode} {config.path} {reordered.path} {pair.first}"
"""
  let before = test.expect(ctx, source, status: 0)?
  let root = test.temp_dir(ctx, name: "positional-constructor-lint")?
  let candidate = fp"{root}/main.xsh"
  candidate.write_atomic(source)
  let ignored = run.capture --text "xsht" lint --only lint.prefer-positional-constructor $candidate ?
  assert ignored.status.exited_with(0), "the lint is opt-in"
  fp"{root}/xsht-config.ini".write_atomic("[lint]\nprefer-positional-constructors = true\n")
  let first = run.capture --text "xsht" lint --only lint.prefer-positional-constructor $candidate ?
  assert first.status.exited_with(1), first.stderr
  let fixing = run.capture --text "xsht" lint --fix --only lint.prefer-positional-constructor $candidate ?
  assert fixing.status.exited_with(0), fixing.stderr
  let fixed = candidate.read_text()?
  assert "Entry(marked(p\"usr/bin/xsh\"), Binary, 493)" in fixed, fixed
  assert "Entry(path: file, kind:)" in fixed, fixed
  assert "Entry(kind: File, path: p\"etc/other\")" in fixed, fixed
  assert "Pair(first: 1, second: 2)" in fixed, fixed
  let after = test.expect(ctx, fixed, status: 0)?
  assert after.stdout == before.stdout
  let second = run.capture --text "xsht" lint --only lint.prefer-positional-constructor $candidate ?
  assert second.status.exited_with(0), second.stderr
  let formatted = run.capture --text "xsht" fmt --check $candidate ?
  assert formatted.status.exited_with(0), formatted.stderr
}
