test test_duplicate_declaration_diagnostics_are_reported_once { |ctx|
  let rejected = test.expect(
    ctx,
    r"""proc repeated() {}
proc repeated() {}
""",
    status: 2,
    stderr: ["check.duplicate-name"],
  )?
  assert rejected.stderr.split("err[check.duplicate-name]").len() - 1 == 1, rejected.stderr
}

test test_generic_annotations_keep_each_concrete_application { |ctx|
  test.expect(
    ctx,
    r"""type Envelope[T] = {value: T, history: List[T] = []}
let integer: Envelope[Int] = Envelope(value: 7)
let text: Envelope[Str] = Envelope(value: "seven")
assert integer.value + 1 == 8
assert text.value.upper() == "SEVEN"
assert integer.history.len() == 0
assert text.history.len() == 0
""",
    status: 0,
  )?
}

test test_imported_declarations_keep_their_own_alias_context { |ctx|
  let root = test.temp_dir(ctx, name: "declaration-context")?
  fp"{root}/counts.xsh".write_atomic(r"""##! Count declarations.
## Count value.
export type Scalar = Int
## Count envelope.
export type Envelope = {value: Scalar}
## Count extraction.
export pure extract(input: Envelope) -> Scalar { input.value }
""")
  fp"{root}/names.xsh".write_atomic(r"""##! Name declarations.
## Name value.
export type Scalar = Str
## Name envelope.
export type Envelope = {value: Scalar}
## Name extraction.
export pure extract(input: Envelope) -> Scalar { input.value }
""")
  let output = test.run_script(
    ctx,
    r"""use counts as c
use names as n
let count: c.Envelope = c.Envelope(value: 7)
let name: n.Envelope = n.Envelope(value: "seven")
assert c.extract(count) + 1 == 8
assert n.extract(name).upper() == "SEVEN"
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  assert output.success, output.stderr
}

test test_null_storage_keeps_explicit_boundary_type { |ctx|
  let returned = test.run_script(
    ctx,
    "pure absent() -> Null { null }\nassert absent() == null\n",
  )?
  assert returned.success, returned.stderr

  let accepted = test.run_script(
    ctx,
    "let exact: Null = null\nassert exact == null\n",
  )?
  assert accepted.success, accepted.stderr

  let rejected = test.run_script(
    ctx,
    "let raw: Any = \"not null\"\ntest.error_kind(raw.require(Null), \"schema\")\n",
  )?
  assert rejected.success, rejected.stderr
}
