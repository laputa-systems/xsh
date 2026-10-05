error OrderError {
    Conflict(path: Path, owner: Str)
    Pair(second: Str, first: Str)
    Triple(zulu: Int, mike: Str, alpha: Bool)
}

error FetchError {
    # A variant without a payload carries only its message.
    Usage
    Offline : Timeout
    Rejected(url: Str, status: Int)
    Refused(message: Str, port: Int) : PermissionDenied, Timeout
}

pure fetch_label(error: Error) -> Str {
  match Err(error) {
    Err(FetchError.Usage {message}) => f"usage: {message}"
    Err(FetchError.Offline) => "offline"
    Err(FetchError.Rejected {url, status}) => f"{url} answered {status}"
    Err(FetchError.Refused {message, port}) => f"{message} on {port}"
    _ => "other"
  }
}

pure pair_fields(error: Error) -> Str {
  match Err(error) {
    Err(OrderError.Pair {second, first}) => f"second={second} first={first}"
    Err(OrderError.Triple {zulu, mike, alpha}) => f"zulu={zulu} mike={mike} alpha={alpha}"
    Err(OrderError.Conflict {path: file, owner}) => f"path={file} owner={owner}"
    _ => "other"
  }
}

test test_positional_error_arguments_fill_fields_in_declaration_order {
  assert pair_fields(OrderError.Triple(1, "m", true)) == "zulu=1 mike=m alpha=true"
  assert pair_fields(OrderError.Triple(1, alpha: true, mike: "m")) == "zulu=1 mike=m alpha=true"
  assert pair_fields(OrderError.Triple(1, "m", alpha: false)) == "zulu=1 mike=m alpha=false"
}

test test_error_equality_ignores_argument_order {
  assert OrderError.Pair(second: "s", first: "f") == OrderError.Pair(first: "f", second: "s")
  assert OrderError.Pair(second: "s", first: "f") != OrderError.Pair(second: "f", first: "s")
  assert OrderError.Triple(1, "m", true) == OrderError.Triple(alpha: true, mike: "m", zulu: 1)
  assert pair_fields(OrderError.Conflict(owner: "o", path: p"x")) == "path=x owner=o"
}

test test_target_typed_error_arguments_fill_fields_in_declaration_order {
  let inferred: OrderError = .Triple(1, "m", true)
  assert pair_fields(inferred) == "zulu=1 mike=m alpha=true"
  assert inferred == OrderError.Triple(1, "m", true)
}

test test_error_arguments_evaluate_in_source_order { |ctx|
  let executed = test.run_script(
    ctx,
    r"""error E = Mixed(second: Int, first: Str) | Pair(left: Int, right: Int)
proc marked(value: Int) -> Int {
  print $value
  return value
}
proc labeled(value: Str) -> Str {
  print $value
  return value
}
match Err(E.Mixed(marked(1), labeled("b"))) {
  Err(E.Mixed {second, first}) => print f"second={second} first={first}"
  _ => print "other"
}
match Err(E.Pair(right: marked(2), left: marked(3))) {
  Err(E.Pair {left, right}) => print f"left={left} right={right}"
  _ => print "other"
}
""",
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """1
b
second=1 first=b
2
3
left=3 right=2
"""
}

test test_positional_error_arguments_are_checked_against_declaration_order { |ctx|
  let swapped = test.run_script(
    ctx,
    """error E = Triple(zulu: Int, mike: Str, alpha: Bool)
let triple = E.Triple("m", 1, true)
""",
  )?
  assert ! swapped.success, swapped.stderr
  assert "check.type-mismatch" in swapped.stderr
  assert "expected Int, found Str" in swapped.stderr

  let extra = test.run_script(
    ctx,
    """error E = Triple(zulu: Int, mike: Str, alpha: Bool)
let triple = E.Triple(1, "m", true, 4)
""",
  )?
  assert ! extra.success, extra.stderr
  assert "too many error constructor arguments" in extra.stderr
}

test test_brace_error_families_declare_variants_payloads_and_facets {
  assert fetch_label(FetchError.Rejected("https://example.test", 503)) == "https://example.test answered 503"
  assert fetch_label(FetchError.Refused("refused", 22)) == "refused on 22"
  assert fetch_label(FetchError.Offline()) == "offline"
  let offline: Error = FetchError.Offline("no route")
  assert offline is Timeout == true
  assert offline is PermissionDenied == false
  let refused: Error = FetchError.Refused("refused", 22)
  assert refused is PermissionDenied == true
  assert refused is Timeout == true
  let usage: Error = FetchError.Usage("bad flag")
  assert usage is Timeout == false
}

test test_payloadless_variants_take_their_message_positionally {
  assert FetchError.Usage("not a URL").message == "not a URL"
  assert FetchError.Usage().message == "FetchError.Usage"
  assert fetch_label(FetchError.Usage("not a URL")) == "usage: not a URL"
  assert fetch_label(FetchError.Usage()) == "usage: FetchError.Usage"
  assert fetch_label(FetchError.Offline("no route")) == "offline"
  assert FetchError.Usage("a") == FetchError.Usage("a")
  assert FetchError.Usage("a") != FetchError.Usage("b")
  assert FetchError.Usage() == FetchError.Usage("FetchError.Usage")
  let inferred: FetchError = .Usage("inferred")
  assert inferred.message == "inferred"
  assert inferred == FetchError.Usage("inferred")
}

test test_declared_payloads_keep_their_message_rule {
  assert FetchError.Rejected("https://example.test", 503).message == "FetchError.Rejected"
  assert FetchError.Refused("refused", 22).message == "refused"
  assert FetchError.Refused(port: 22, message: "refused").message == "refused"
}

test test_payloadless_variants_reject_other_arguments { |ctx|
  let named = test.run_script(
    ctx,
    """error E {
    Usage
    Other(code: Int)
}
let usage = E.Usage(message: "named")
""",
  )?
  assert ! named.success, named.stderr
  assert "check.error-constructor" in named.stderr
  assert "pass its message positionally" in named.stderr

  let extra = test.run_script(
    ctx,
    """error E = Usage | Other(code: Int)
let usage = E.Usage("one", "two")
""",
  )?
  assert ! extra.success, extra.stderr
  assert "check.arity" in extra.stderr

  let typed = test.run_script(
    ctx,
    """error E = Usage | Other(code: Int)
let usage = E.Usage(1)
""",
  )?
  assert ! typed.success, typed.stderr
  assert "check.type-mismatch" in typed.stderr
  assert "expected Str, found Int" in typed.stderr

  let field = test.run_script(
    ctx,
    """error E = Usage | Other(code: Int)
match Err(E.Usage("x")) {
  Err(E.Usage {code}) => print $code
  _ => print "other"
}
""",
  )?
  assert ! field.success, field.stderr
  assert "check.pattern-field" in field.stderr

  let missing = test.run_script(
    ctx,
    """error E = Usage | Other(code: Int)
let other = E.Other()
""",
  )?
  assert ! missing.success, missing.stderr
  assert "missing error payload field" in missing.stderr
}

test test_brace_error_families_reject_separators_and_empty_bodies { |ctx|
  for source in [
    """error E {
    Usage,
    Other(code: Int)
}
""",
    """error E { Usage | Other(code: Int) }
""",
    """error E { Usage Other }
""",
    """error E {
}
""",
  ] {
    let rejected = test.run_script(ctx, source)?
    assert ! rejected.success, rejected.stderr
    assert "parse.error-variant" in rejected.stderr, rejected.stderr
  }

  let generic = test.run_script(
    ctx,
    """error E[T] {
    Usage
}
""",
  )?
  assert ! generic.success, generic.stderr
  assert "parse.generic-error-family" in generic.stderr, generic.stderr
}

test test_brace_error_families_export_and_match_across_modules { |ctx|
  let root = test.temp_dir(ctx, name: "brace-error-module")?
  fp"{root}/fetch.xsh".write_atomic("""##! Fetch failures.
## Why a fetch failed.
export error FetchError {
    Usage
    Rejected(url: Str, status: Int) : PermissionDenied
}
## Fails with a usage error.
export proc refuse(reason: Str) -> Result[Unit, FetchError] {
  return Err(.Usage(reason))
}
""")?
  let executed = test.run_script(
    ctx,
    r"""use fetch
match fetch.refuse("not a URL") {
  Err(fetch.FetchError.Usage {message}) => print f"usage: {message}"
  _ => print "other"
}
print ${fetch.FetchError.Usage().message}
print ${fetch.FetchError.Usage("local").message}
let rejected: Error = fetch.FetchError.Rejected("https://example.test", 403)
print ${rejected is PermissionDenied}
""",
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  assert executed.success, executed.stderr
  assert executed.stdout == """usage: not a URL
FetchError.Usage
local
true
"""
}

test test_prefer_implicit_message_fix_preserves_behavior_and_converges { |ctx|
  let root = test.temp_dir(ctx, name: "implicit-message-lint")?
  fp"{root}/xsht-config.ini".write_atomic("[lint]\nprefer-implicit-messages = true\n")?
  let source = r"""error ProofError = Usage(message: Str) | Failed(kind: Str, message: Str) | Missing(message: Str) : NotFound

error StageError {
    Skipped(message: Str)
    Broken(detail: Str)
}

proc check(name: Str) -> Result[Unit, ProofError] {
  let message = f"no {name}"
  return Err(ProofError.Usage(message: "empty name")) when name == ""
  return Err(.Missing(message:)) when name == "gone"
  return Err(ProofError.Failed(kind: "kind", message:)) when name == "bad"
}

for name in ["", "gone", "bad", "ok"] {
  match check(name) {
    Err(ProofError.Usage {message}) => print f"usage: {message}"
    Err(ProofError.Missing {message: text}) => print f"missing: {text}"
    Err(error) => print $error.message
    Ok(_) => print "ok"
  }
}
print ${StageError.Skipped("positional already").message}
print ${StageError.Broken("detail").message}
"""
  let before = test.run_script(ctx, source)?
  assert before.success, before.stderr
  let candidate = fp"{root}/main.xsh"
  candidate.write_atomic(source)?
  let first = run.capture --text "xsht" lint --only lint.prefer-implicit-message $candidate ?
  assert first.status.exited_with(1), first.stderr
  assert "lint.prefer-implicit-message" in first.stderr
  assert "`Usage` takes its message positionally" in first.stderr, first.stderr
  assert "variant `Missing` restates the message every error carries" in first.stderr, first.stderr
  assert "first pass the message positionally at each `Usage(message: ...)` call" in first.stderr, first.stderr
  let fixing = run.capture --text "xsht" lint --fix --only lint.prefer-implicit-message $candidate ?
  assert fixing.status.exited_with(0), fixing.stderr
  let fixed = candidate.read_text()?
  assert "error ProofError = Usage | Failed(kind: Str, message: Str) | Missing : NotFound" in fixed, fixed
  assert "    Skipped\n    Broken(detail: Str)\n" in fixed, fixed
  assert "ProofError.Usage(\"empty name\")" in fixed, fixed
  assert "Err(.Missing(message))" in fixed, fixed
  assert "ProofError.Failed(kind: \"kind\", message:)" in fixed, fixed
  let after = test.run_script(ctx, fixed)?
  assert after.success, after.stderr
  assert after.stdout == before.stdout
  let second = run.capture --text "xsht" lint --only lint.prefer-implicit-message $candidate ?
  assert second.status.exited_with(0), second.stderr
}

test test_prefer_implicit_message_leaves_exported_declarations_to_the_author { |ctx|
  let root = test.temp_dir(ctx, name: "implicit-message-exported")?
  fp"{root}/xsht-config.ini".write_atomic("[lint]\nprefer-implicit-messages = true\n")?
  let module_source = """##! Fetch failures.
## Why a fetch failed.
export error FetchError = Usage(message: Str) | Rejected(url: Str, status: Int)
## Fails with a usage error.
export proc refuse(reason: Str) -> Result[Unit, FetchError] {
  return Err(FetchError.Usage(message: reason))
}
"""
  let module_file = fp"{root}/fetch.xsh"
  module_file.write_atomic(module_source)?
  let importer_source = r"""use fetch
match fetch.refuse("not a URL") {
  Err(fetch.FetchError.Usage {message}) => print f"usage: {message}"
  _ => print "other"
}
print ${fetch.FetchError.Usage(message: "imported").message}
"""
  let module_env = {XSH_MODULE_PATH: root.display()}
  let before = test.run_script(ctx, importer_source, [], module_env)?
  assert before.success, before.stderr
  let importer = fp"{root}/main.xsh"
  importer.write_atomic(importer_source)?

  let reported = run.capture --text "xsht" lint --only lint.prefer-implicit-message $module_file ?
  assert reported.status.exited_with(1), reported.stderr
  assert "`FetchError` is exported" in reported.stderr, reported.stderr
  assert "delete `(message: Str)` here" in reported.stderr, reported.stderr

  # Each call is fixed where it is written; the exported declaration is not.
  let fixing_module = run.capture --text "xsht" lint --fix --only lint.prefer-implicit-message $module_file ?
  assert ! fixing_module.status.exited_with(2), fixing_module.stderr
  let fixed_module = module_file.read_text()?
  assert "export error FetchError = Usage(message: Str) | Rejected(url: Str, status: Int)" in fixed_module, fixed_module
  assert "Err(FetchError.Usage(reason))" in fixed_module, fixed_module
  let remaining = run.capture --text "xsht" lint --only lint.prefer-implicit-message $module_file ?
  assert remaining.status.exited_with(1), remaining.stderr
  assert "`FetchError` is exported" in remaining.stderr, remaining.stderr
  assert "takes its message positionally" not in remaining.stderr, remaining.stderr
  let fixing_importer = run.capture --text "xsht" lint --fix --only lint.prefer-implicit-message $importer ?
  assert ! fixing_importer.status.exited_with(2), fixing_importer.stderr
  let fixed_importer = importer.read_text()?
  assert "fetch.FetchError.Usage(\"imported\")" in fixed_importer, fixed_importer
  let after = test.run_script(ctx, fixed_importer, [], module_env)?
  assert after.success, after.stderr
  assert after.stdout == before.stdout

  # With every call positional, the manual declaration edit keeps behavior.
  module_file.write_atomic(fixed_module.replace("Usage(message: Str)", "Usage"))?
  let migrated = test.run_script(ctx, fixed_importer, [], module_env)?
  assert migrated.success, migrated.stderr
  assert migrated.stdout == before.stdout
}

test test_prefer_implicit_message_is_off_by_default { |ctx|
  let root = test.temp_dir(ctx, name: "implicit-message-default")?
  let candidate = fp"{root}/main.xsh"
  candidate.write_atomic(r"""error ProofError = Usage(message: Str)
print ${ProofError.Usage(message: "named").message}
""")?
  let linted = run.capture --text "xsht" lint --only lint.prefer-implicit-message $candidate ?
  assert linted.status.exited_with(0), linted.stderr
}

test test_positional_error_arguments_fix_names_the_fields_they_fill { |ctx|
  let root = test.temp_dir(ctx, name: "positional-error-arguments")?
  let candidate = fp"{root}/main.xsh"
  candidate.write_atomic(
    r"""error ScriptError = Failed(kind: Str, message: Str) | Triple(zulu: Int, mike: Str, alpha: Int)

proc check(kind: Str) -> Result[Unit, ScriptError] {
  let message = f"bad {kind}"
  return Err(ScriptError.Failed(kind, message)) when kind == "a"
  return Err(.Failed("usage", f"{kind} again")) when kind == "b"
  return Err(ScriptError.Triple(alpha: 2, 1, "m")) when kind == "c"
  return Err(ScriptError.Triple(3, "distinct", alpha: 4)) when kind == "d"
}

for kind in ["a", "b", "c", "d", "e"] {
  match check(kind) {
    Err(ScriptError.Failed {kind: label, message}) => print f"{label}: {message}"
    Err(ScriptError.Triple {zulu, mike, alpha}) => print f"{zulu} {mike} {alpha}"
    Err(error) => print $error.message
    Ok(_) => print "ok"
  }
}
""",
  )?
  let first = run.capture --text "xsht" lint --only lint.positional-error-arguments $candidate ?
  assert ! first.status.exited_with(0), first.stderr
  assert "fields `kind` and `message` can hold the same value" in first.stderr, first.stderr
  assert "positional error constructor arguments must come before named ones" in first.stderr, first.stderr
  let fixing = run.capture --text "xsht" lint --fix --only lint.positional-error-arguments $candidate ?
  assert fixing.status.exited_with(0), fixing.stderr
  let fixed = candidate.read_text()?
  assert "ScriptError.Failed(kind:, message:)" in fixed, fixed
  assert ".Failed(kind: \"usage\", message: f\"{kind} again\")" in fixed, fixed
  assert "ScriptError.Triple(alpha: 2, zulu: 1, mike: \"m\")" in fixed, fixed
  assert "ScriptError.Triple(3, \"distinct\", alpha: 4)" in fixed, fixed
  let after = test.run_script(ctx, fixed)?
  assert after.success, after.stderr
  assert after.stdout == """a: bad a
usage: b again
1 m 2
3 distinct 4
ok
"""
  let second = run.capture --text "xsht" lint --only lint.positional-error-arguments $candidate ?
  assert second.status.exited_with(0), second.stderr
}
