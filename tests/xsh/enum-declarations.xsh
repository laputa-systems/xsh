test test_enum_singleton_and_alias { |ctx|
  let executed = test.run_script(
    ctx,
    r"""enum Token { Present(Str) }
type Alias = Token
pure render(token: Alias) -> Str {
  match token { Present(text) => text }
}
print render(Present("ready"))
print (Present("same") == Present("same"))
""",
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """ready
true
"""
}

test test_enum_legacy_declaration_is_migration_error { |ctx|
  let rejected = test.run_script(
    ctx,
    """type Mode = Fast | Slow
print Fast
""",
  )?
  assert ! rejected.success, rejected.stderr
  assert "parse.enum-migration" in rejected.stderr
}

test test_enum_rejects_invalid_declarations { |ctx|
  for source in [
    """enum Empty {}
""",
    """enum Duplicate { Repeated, Repeated }
""",
    """enum One { Shared }
enum Two { Shared }
""",
    """enum One { Collision }
pure Collision() -> Int { 1 }
""",
    """enum One { Entry }
let Entry = 1
""",
    """enum One { fs }
""",
    """enum One { Item(Int) }
let wrong = Item("wrong")
""",
    """let enum = 1
""",
  ] {
    let rejected = test.run_script(ctx, source)?
    assert ! rejected.success, rejected.stderr
  }
}

test test_enum_module_constructor_namespace_and_labels { |ctx|
  let root = test.temp_dir(ctx, name: "enum-module")?
  fp"{root}/choice.xsh".write_atomic("""##! Nominal choices.
## A singleton payload.
export enum Choice { Chosen(Int) }
## The same nominal type.
export type Alias = Choice
""")?
  let executed = test.run_script(
    ctx,
    r"""use choice as c
let value: c.Alias = c.Chosen(7)
match value { c.Chosen(number) => print $number }
type Metadata = {enum: Str}
let row = Metadata(enum: "label")
let {enum: label} = row
print $row.enum
print $label
let word = (run.text printf "%s" enum)?
print $word
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """7
label
label
enum
"""
  let invalid = test.run_script(
    ctx,
    """use choice as c
let value = c.Choice.Chosen(7)
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  assert ! invalid.success, invalid.stderr
}

test test_enum_multiline_payload_equality_and_exhaustive_matches { |ctx|
  let declaration = r"""enum Mode {
  Fast,
  Thorough,
  Custom(Int),
}
"""
  let executed = test.run_script(
    ctx,
    declaration + r"""pure describe(mode: Mode) -> Str {
  match mode {
    Fast => "fast"
    Thorough => "thorough"
    Custom(level) => f"custom:{level}"
  }
}
print ${describe(Fast)} ${describe(Custom(3))} ${Custom(3) == Custom(3)} ${Custom(3) == Custom(4)}
""",
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """fast custom:3 true false
"""
  let incomplete = test.run_script(
    ctx,
    declaration + r"""pure describe(mode: Mode) -> Str {
  match mode {
    Fast => "fast"
    Custom(level) => f"custom:{level}"
  }
}
""",
  )?
  assert ! incomplete.success, incomplete.stderr
  assert "check.match-value-exhaustive" in incomplete.stderr
  assert "Thorough" in incomplete.stderr
  let qualified = test.run_script(
    ctx,
    declaration + """let value = Mode.Fast
""",
  )?
  assert ! qualified.success, qualified.stderr
}
