type Captured = {status: Status, stdout: Str, stderr: Str}

# Writes `source` to a fresh script in a directory of its own.
proc script(ctx: TestContext, source: Str) [fs, error] -> Result[Path] {
  test.temp_file(ctx, name: "lint.xsh", contents: bytes.from_text(source))
}

# Runs `xsht lint` on `file` with `flags` and requires that the file parsed
# and checked, so that an absent finding is a statement about the linter.
# It runs in the directory of `file`, where no project configuration applies:
# the runner's own directory is the repository, whose configuration turns
# some default rules off.
proc lint(file: Path, flags: List[Str]) [process, env, error] -> Result[Captured] {
  let linted = cd (file.parent()) {
    run.capture --text "xsht" lint @flags $file
  }?

  assert linted.status.exited_with(0) or linted.status.exited_with(1), linted.stderr
  Ok(linted)
}

# How many findings with `code` the default rule set reports for `file`.
proc findings(file: Path, code: Str) [process, env, error] -> Result[Int] {
  let linted = lint(file, [])?
  Ok(linted.stderr.split(f"[{code}]").len() - 1)
}

# The text of `file` after the fixes of `code` alone were applied.
proc fixed(file: Path, code: Str) [fs, process, env, error] -> Result[Str] {
  let _ = lint(file, ["--fix", "--only", code])?
  file.read_text()
}

# Requires that `file` parses and checks.
proc assert_checked(file: Path) [process, env, error] {
  let checked = run.capture --text "xsht" check $file
  assert checked.status.exited_with(0), checked.stderr
}

# Formats `file` in place, requires a second pass to change nothing, and
# returns the formatted text.
proc formatted(file: Path) [fs, process, env, error] -> Result[Str] {
  let written = run.capture --text "xsht" fmt $file
  assert written.status.exited_with(0), written.stderr
  let stable = run.capture --text "xsht" fmt --check $file
  assert stable.status.exited_with(0), stable.stderr
  file.read_text()
}

# The text of `source` after every `check.bool-statement` fix was applied.
# Requires that the checker rejects `source` for nothing else.
proc bool_statement_fixed(ctx: TestContext, source: Str, statements: Int) [fs, process, env, error] -> Result[Path] {
  let file = script(ctx, source)?
  let reported = run.capture --text "xsht" check $file
  assert reported.status.exited_with(2), reported.stderr
  assert reported.stderr.split("err[check.bool-statement]").len() - 1 == statements, reported.stderr
  assert reported.stderr.split("err[").len() - 1 == statements, reported.stderr
  let _ = run.capture --text "xsht" lint --only check.bool-statement --fix $file
  Ok(file)
}

test test_scalar_iteration_fixes_recheck_and_converge_with_comments_and_scopes { |ctx|
  let file = script(
    ctx,
    r"""let text = "éx"
for character in text.split("") { print $character }
let characters = [character for character in text.split(separator: "")]
let payload = b"\x00\xff"
for index in range(payload.len()) {
  let octet = payload.byte_at(index)
  # preserve this body comment
  let _ = octet
}
for character in [part for part in text.split("")].join("").split("") { let _ = character }
for character in "ab".split("") { let _ = character }
print ${characters.len()}
""",
  )?
  assert findings(file, "lint.prefer-scalar-iteration")? == 6
  let text = fixed(file, "lint.prefer-scalar-iteration")?
  assert "for character in text {" in text, text
  assert "[character for character in text]" in text, text
  assert "for octet in payload {\n  # preserve this body comment" in text, text
  assert_checked(file)
  assert findings(file, "lint.prefer-scalar-iteration")? == 0
  let _ = formatted(file)?
}

test test_scalar_iteration_keeps_used_adapters_offsets_mutation_and_partial_ranges { |ctx|
  for source in [
    "let text = \"ab\"\nlet parts = text.split(\"\")\nfor part in parts { print \$part }\nprint \${parts.len()}\n",
    "for character in \"ab\".split(\"\", maxsplit: 1) { print \$character }\n",
    "let payload = b\"ab\"\nfor index in range(payload.len()) {\n  let octet = payload.byte_at(index)\n  print \$index\n  let _ = octet\n}\n",
    "var payload = b\"ab\"\nfor index in range(payload.len()) {\n  let octet = payload.byte_at(index)\n  payload = b\"xy\"\n  let _ = octet\n}\n",
    "let payload = b\"ab\"\nfor index in range(1, payload.len()) {\n  let octet = payload.byte_at(index)\n  let _ = octet\n}\n",
    "let text = \"é\"\nfor index in range(text.byte_len()) {\n  let octet = text.byte_at(index)\n  let _ = octet\n}\n",
    "let payload = b\"ab\"\nfor index in range(payload.len()) {\n  let octet = payload.byte_at(index) # preserve extraction\n  let _ = octet\n}\n",
  ] {
    assert findings(script(ctx, source)?, "lint.prefer-scalar-iteration")? == 0, source
  }
}

test test_explicit_accept_policy_keeps_propagation_and_custom_status_handlers { |ctx|
  let file = script(
    ctx,
    "proc main() [process, error] {\n  run --accept=[0,1] grep pattern file ?\n  let status = run.status --accept=[0,1] grep pattern file\n  if status.exited_with(1) { print \"no rows\" }\n}\n",
  )?
  assert findings(file, "lint.run-status")? == 0
  let text = formatted(file)?
  assert "--accept=[0, 1] grep pattern file ?" in text, text
  assert "if status.exited_with(1)" in text, text
  assert_checked(file)
}

test test_redundant_optional_fallback_fix_requires_checked_presence_and_inert_data { |ctx|
  let file = script(
    ctx,
    "pure select(value: Str?) -> Str {\n  let available = value != null\n  guard available else { return \"missing\" }\n  value ?? \"fallback\"\n}\n",
  )?
  assert findings(file, "lint.redundant-optional-fallback")? == 1
  let text = fixed(file, "lint.redundant-optional-fallback")?
  assert "  value\n" in text, text
  assert_checked(file)
  assert findings(file, "lint.redundant-optional-fallback")? == 0
  for source in [
    "pure select(value: Str?) -> Str { value ?? \"fallback\" }\n",
    "pure fallback() -> Str { \"fallback\" }\npure select(value: Str?) -> Str { guard value != null else { return \"missing\" }; value ?? fallback() }\n",
  ] {
    assert findings(script(ctx, source)?, "lint.redundant-optional-fallback")? == 0, source
  }
}

test test_constant_key_projection_identity_require_fix_keeps_boundaries { |ctx|
  let file = script(
    ctx,
    "type Config = {workers: Int}\n# Preserve worker contract α.\nproc read(config: Config) [error] -> Int { config.get(\n# Preserve the selected field.\n\"workers\")?.require(Int)? }\n",
  )?
  assert findings(file, "lint.redundant-require")? == 1
  let text = fixed(file, "lint.redundant-require")?
  assert "# Preserve worker contract α." in text, text
  assert "# Preserve the selected field." in text, text
  assert "require(Int)" not in text, text
  assert_checked(file)
  assert findings(file, "lint.redundant-require")? == 0
  for source in [
    "proc read(config: Record, key: Str) [error] -> Int { config.get(key)?.require(Int)? }\n",
    "type Config = {path: Str}\nproc read(config: Config) [error] -> Path { config.get(\"path\")?.require(Path)? }\n",
    "type Config = {count: UInt}\nproc read(config: Config) [error] -> UInt { config.get(\"count\")?.require(UInt)? }\n",
    "type Wide = {name: Str, extra: Int}\ntype Narrow = {name: Str}\ntype Config = {entry: Wide}\nproc read(config: Config) [error] -> Narrow { config.get(\"entry\")?.require(Narrow)? }\n",
  ] {
    assert findings(script(ctx, source)?, "lint.redundant-require")? == 0, source
  }
}

test test_block_string_fix_in_a_wire_enum_mapping_keeps_the_wire_bytes { |ctx|
  let source = "enum State: Str {\n  Ready = \"ready\\n\" + \"empty\\n\"\n}\nlet state: State = Ready\nprint json.encode(state)?\n"
  let file = script(ctx, source)?
  assert findings(file, "lint.prefer-block-string")? == 1
  let original = run.capture --text "xsht" trace $file
  assert original.status.exited_with(0), original.stderr
  assert fixed(file, "lint.prefer-block-string")? != source
  assert_checked(file)
  let rewritten = run.capture --text "xsht" trace $file
  assert rewritten.status.exited_with(0), rewritten.stderr
  assert original.stdout == "\"ready\\nempty\\n\"\n", original.stdout
  assert rewritten.stdout == original.stdout
}

test test_bool_statement_fix_prefixes_every_checked_bool_statement { |ctx|
  let file = bool_statement_fixed(
    ctx,
    r"""let flag = true
let xs = [1, 2]
# a comment before
xs == [1, 2] # a trailing comment
(flag)
xs.len() ==
    2
pure positive(n: Int) -> Bool { n > 0 }
proc check(n: Int) {
    if n > 0 {
        n > 0
    }
    for item in xs {
        item > 0
    }
    match n {
        1 => n == 1,
        _ => n >= 0,
    }
    let _ = n == 3
    assert n < 9, "bounded"
    positive(n)
}
check(1)?
flag
assert flag
""",
    9,
  )?
  assert file.read_text()? == r"""let flag = true
let xs = [1, 2]
# a comment before
assert xs == [1, 2] # a trailing comment
assert flag
assert xs.len() ==
    2
pure positive(n: Int) -> Bool { n > 0 }
proc check(n: Int) {
    if n > 0 {
        assert n > 0
    }
    for item in xs {
        assert item > 0
    }
    match n {
        1 => { assert n == 1 },
        _ => { assert n >= 0 },
    }
    let _ = n == 3
    assert n < 9, "bounded"
    assert positive(n)
}
check(1)?
assert flag
assert flag
"""
  assert_checked(file)
}

test test_bool_statement_fix_covers_unit_tails_and_test_declarations { |ctx|
  let file = bool_statement_fixed(
    ctx,
    "proc ready(flag: Bool) [error] -> Result[Unit] {\n    flag\n}\nproc done(n: Int) [error] -> Result[Unit] {\n    n == 1\n}\ntest arithmetic {\n    1 + 1 == 2\n}\nready(true)?\ndone(1)?\n",
    3,
  )?
  assert file.read_text()? == "proc ready(flag: Bool) [error] -> Result[Unit] {\n    assert flag\n}\nproc done(n: Int) [error] -> Result[Unit] {\n    assert n == 1\n}\ntest arithmetic {\n    assert 1 + 1 == 2\n}\nready(true)?\ndone(1)?\n"
  assert_checked(file)
}

test test_assertion_helper_migration_targets_assert { |ctx|
  let file = script(
    ctx,
    "proc compare(actual: Int) {\n    test.eq(actual, 3)?\n    match actual {\n        3 => test.ok(actual > 2)?,\n        _ => {},\n    }\n}\n",
  )?
  assert findings(file, "lint.core-assert")? == 2
  let reported = lint(file, ["--only", "lint.core-assert"])?
  let replacements = collect {
    for line in reported.stderr.lines() {
      let found = rx"^help: .* -> (.*)$".captures(line)
      yield found[1] when found.len() == 2
    }
  }

  assert replacements == ["assert actual == 3", "{ assert actual > 2 }"], reported.stderr
  let _ = fixed(file, "lint.core-assert")?
  assert_checked(file)
}

test test_env_string_fix_replaces_literal_reads_and_keeps_other_lookups { |ctx|
  let file = script(
    ctx,
    r"""let home = env.get("HOME")?
let login = env.Str.USER ?? "nobody"
let named = env.get(name: "SHELL") ?? "sh"
let nested = f"{env.get("TERM") ?? "dumb"}"
let dashed = env.get("NOT-AN-IDENT") ?? ""
let key = "HOME"
let computed = env.get(key) ?? ""
let fallback = env.get_or("HOME", "/")?
let dir = env.Path.HOME ?? /
print $home $login $named $nested $dashed $computed $fallback $dir
""",
  )?

  # Only the four literal identifier reads; `env.get_or` fails on non-UTF-8
  # values where `??` would fall back, so it is not an equivalent rewrite.
  assert findings(file, "lint.prefer-env-string")? == 4
  assert fixed(file, "lint.prefer-env-string")? == r"""let home = e"HOME"?
let login = e"USER" ?? "nobody"
let named = e"SHELL" ?? "sh"
let nested = f"{e"TERM" ?? "dumb"}"
let dashed = env.get("NOT-AN-IDENT") ?? ""
let key = "HOME"
let computed = env.get(key) ?? ""
let fallback = env.get_or("HOME", "/")?
let dir = env.Path.HOME ?? /
print $home $login $named $nested $dashed $computed $fallback $dir
"""
  assert_checked(file)
  let stable = run.capture --text "xsht" fmt --check $file
  assert stable.status.exited_with(0), stable.stderr
  assert findings(file, "lint.prefer-env-string")? == 0
}
