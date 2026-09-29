test test_enum_singleton_and_alias [error] { |ctx|
  let executed = test.run_script(ctx, r"""enum Token { Present(Str) }
type Alias = Token
pure render(token: Alias) -> Str {
  match token { Present(text) => text }
}
print render(Present("ready"))
print (Present("same") == Present("same"))
""")?
  test.ok(executed.success, executed.stderr)?
  test.eq(executed.stdout, "ready\ntrue\n")?
}

test test_enum_legacy_declaration_is_migration_error [error] { |ctx|
  let rejected = test.run_script(ctx, "type Mode = Fast | Slow\nprint Fast\n")?
  test.ok(! rejected.success, rejected.stderr)?
  test.contains(rejected.stderr, "parse.enum-migration")?
}

test test_enum_rejects_invalid_declarations [error] { |ctx|
  for source in [
    "enum Empty {}\n",
    "enum Duplicate { Repeated, Repeated }\n",
    "enum One { Shared }\nenum Two { Shared }\n",
    "enum One { Collision }\npure Collision() -> Int { 1 }\n",
    "enum One { Entry }\nlet Entry = 1\n",
    "enum One { fs }\n",
    "enum One { Item(Int) }\nlet wrong = Item(\"wrong\")\n",
    "let enum = 1\n",
  ] {
    let rejected = test.run_script(ctx, source)?
    test.ok(! rejected.success, rejected.stderr)?
  }
}

test test_enum_module_constructor_namespace_and_labels [fs, error] { |ctx|
  let root = test.temp_dir(ctx, name: "enum-module")?
  fp"${root}/choice.xsh".write_atomic("""##! Nominal choices.
## A singleton payload.
export enum Choice { Chosen(Int) }
## The same nominal type.
export type Alias = Choice
""")?
  let executed = test.run_script(ctx, r"""use choice as c
let value: c.Alias = c.Chosen(7)
match value { c.Chosen(number) => print $number }
type Metadata = {enum: Str}
let row = Metadata(enum: "label")
let {enum: label} = row
print $row.enum
print $label
let word = (run.text printf "%s" enum)?
print $word
""", [], {XSH_MODULE_PATH: root.display()})?
  test.ok(executed.success, executed.stderr)?
  test.eq(executed.stdout, "7\nlabel\nlabel\nenum\n")?
  let invalid = test.run_script(ctx, "use choice as c\nlet value = c.Choice.Chosen(7)\n", [], {XSH_MODULE_PATH: root.display()})?
  test.ok(! invalid.success, invalid.stderr)?
}
