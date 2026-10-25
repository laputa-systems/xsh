test test_private_import_alias_preserves_exported_error_identity { |ctx|
  let root = test.temp_dir(ctx, name: "private-error-family")?
  fp"{root}/model.xsh".write(r"""##! Error identities.
## A missing value.
export error Trouble = Missing(reason: Str)
## A missing outcome.
export type Outcome = Result[Int, Trouble]
## A generic missing outcome.
export type GenericOutcome[T] = Result[T, Trouble]
## Return a missing value.
export pure missing() -> Result[Int, Trouble] { Err(Trouble.Missing("broken")) }
## Return an inferred missing variant.
export pure inferred() -> GenericOutcome[Int] { Err(.Missing("inferred")) }
""")
  fp"{root}/facade.xsh".write(r"""##! Export a private dependency's error.
use model as internal
## Return a missing value.
export pure missing() -> Result[Int, internal.Trouble] { internal.missing() }
""")
  let output = test.expect(ctx, r"""use model as local
use model as other
use facade
let failure: Result[Int, local.Trouble] = facade.missing()
let same: Result[Int, other.Trouble] = failure
let typed: local.Outcome = facade.missing()
let generic: local.GenericOutcome[Int] = facade.missing()
let inferred: Result[Int, local.Trouble] = local.inferred()
pure message() -> Str {
  match same { Err(local.Trouble.Missing {reason}) => reason; _ => "unexpected" }
}
print ${message()}
match inferred { Err(local.Trouble.Missing {reason}) => print $reason; _ => print "unexpected" }
let _ = inferred?
""", status: 3, env: {XSH_MODULE_PATH: root})?
  assert output.stdout == "broken\ninferred\n"
  assert "Trouble.Missing" in output.stderr
  assert ".xsh.Trouble" not in output.stderr
  fp"{root}/unrelated.xsh".write(r"""##! A distinct error identity.
## An unrelated missing value.
export error Trouble = Missing(reason: Str)
""")
  let rejected = test.run_script(ctx, r"""use facade
use unrelated
let failure: Result[Int, unrelated.Trouble] = facade.missing()
""", env: {XSH_MODULE_PATH: root})?
  assert !rejected.success
  assert "type mismatch" in rejected.stderr
  assert "expected Result[Int, unrelated.Trouble], found Result[Int, model.Trouble]" in rejected.stderr
}
