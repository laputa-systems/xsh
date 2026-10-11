proc enum_modules(ctx: TestContext) -> Result[Path] {
  let root = test.temp_dir(ctx, name: "enum.display.identity")?
  let source = """##! Declared file kinds.
## A file kind with a stable wire spelling.
export enum Kind: Str { File = "file", Directory = "directory" }
"""
  fp"{root}/kinds.xsh".write(source)
  fp"{root}/other.xsh".write(source)
  root
}

test test_imported_enum_diagnostics_name_the_declaration_at_every_depth { |ctx|
  let root = enum_modules(ctx)?
  for example in [
    {
      source: "let value: Str = kinds.File\n",
      expected: "expected Str, found Kind",
    },
    {
      source: "let items = [kinds.File]\nlet value: List[Str] = items\n",
      expected: "expected List[Str], found List[Kind]",
    },
    {
      source: "let optional: kinds.Kind? = kinds.File\nlet value: Str? = optional\n",
      expected: "expected Str?, found Kind?",
    },
    {
      source: "let held: Map[List[kinds.Kind?]] = {value: [kinds.File]}\nlet value: Map[List[Str?]] = held\n",
      expected: "expected Map[List[Str?]], found Map[List[Kind?]]",
    },
  ] {
    let rejected = test.run_xsh(
      ctx,
      "use kinds\n" + example.source,
      env: {XSH_MODULE_PATH: root},
    )?
    assert rejected.status == 2, rejected.stderr
    assert "check.type-mismatch" in rejected.stderr, rejected.stderr
    assert example.expected in rejected.stderr, rejected.stderr
    assert root.display() not in rejected.stderr, rejected.stderr
  }
}

test test_imported_enum_display_preserves_distinct_nominal_and_wire_identities { |ctx|
  let root = enum_modules(ctx)?
  let executed = test.run_xsh(
    ctx,
    r"""use kinds as first
use other as second
let left: Any = first.File
let right: Any = second.File
print (left == right)
print (right is first.Kind)
print (right is second.Kind)
print (right.require(first.Kind) is Err(_))
print (json.encode(first.File)? == json.encode(second.File)?)
print ("file".require(first.Kind)? == first.File)
""",
    env: {XSH_MODULE_PATH: root},
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == "false\nfalse\ntrue\ntrue\ntrue\ntrue\n", executed.stdout

  let rejected = test.run_xsh(
    ctx,
    "use kinds as first\nuse other as second\nlet value: first.Kind = second.File\n",
    env: {XSH_MODULE_PATH: root},
  )?
  assert rejected.status == 2, rejected.stderr
  assert "check.type-mismatch" in rejected.stderr, rejected.stderr
  assert "expected Kind, found Kind" in rejected.stderr, rejected.stderr
  assert root.display() not in rejected.stderr, rejected.stderr
}
