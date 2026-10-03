test test_bare_blocks_preserve_values_and_cleanup { |ctx|
  let output = test.run_script(
    ctx,
    """proc mark(message: Str) [] { print $message }
var count = 0
{
  let increment = 1
  defer mark("statement cleanup")
  count += increment
}
let answer = {
  defer mark("value cleanup")
  count + 41
}
let negative = { false }
let grouped = { (answer) }
let shorthand = {answer}
print \${answer} \${negative} \${grouped} \${shorthand.answer}
""",
  )?
  assert output.success
  assert output.stdout == """statement cleanup
value cleanup
42 false 42 42
"""
}

test test_bare_statement_blocks_assert_false { |ctx|
  let output = test.run_script(
    ctx,
    """proc mark(message: Str) [] { print $message }
{
  defer mark("cleanup")
  assert false
}

print unreachable
""",
  )?
  assert ! output.success
  assert output.stdout == """cleanup
"""
  assert "assertion" in output.stderr
}

pure lexical_tail() -> Bool {
  {
    false
  }
}

test test_bare_value_tail_preserves_false {
  assert ! lexical_tail()
}

test test_bare_blocks_preserve_lexical_transfers { |ctx|
  let output = test.run_script(
    ctx,
    """proc mark(message: Str) [] { print $message }
proc answer() [error] -> Int {
  {
    defer mark("return cleanup")
    return 7
  }
  0
}
proc visit() [error] {
  for value in [1, 2, 3] {
    {
      defer mark("loop cleanup")
      if value == 2 { { continue } }
      if value == 3 { { break } }
      print \${value}
    }
  }
}
print \${answer()}
visit()
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """return cleanup
7
1
loop cleanup
loop cleanup
loop cleanup
"""
}

test test_bare_blocks_preserve_literal_and_callable_tails { |ctx|
  let output = test.run_script(
    ctx,
    r"""
pure calculated() -> Int { 9 }
type Row = {value: Int}
pure row() -> Row { {value: 3} }
let empty: Map[Int] = {}
let named = {if: 1}
let computed = {["key"]: 2}
let updated = {...computed, ["next"]: 4}
let called = { calculated() }
let selected = { let value = 5; {value} }
let indexed = { [6][0] }
print ${empty.len()} ${named.if} ${updated.get("next")?} ${called} ${selected.value} ${indexed} ${row().value}
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """0 1 4 9 5 6 3
"""
}

test test_bare_blocks_keep_checker_and_result_boundaries { |ctx|
  for source in [
    """{ let hidden = 1 }
print $hidden
""",
    """let value = { |input| input }
""",
    """{ Ok(7) }
print unreachable
""",
    """{ break }
""",
    """{ continue }
""",
  ] {
    let output = test.run_script(ctx, source)?
    let rejected = ! output.success
    let rejection_details = source + output.stderr
    assert rejected, rejection_details
  }

  let output = test.run_script(
    ctx,
    r"""
error BlockError = Failed(message: Str)
pure failed() -> Result[Unit, BlockError] { Err(BlockError.Failed(message: "identity")) }
let captured = try { { failed()? }; "unreachable" }
print ${captured ?? {|failure| failure.message}}
let data = { Err(BlockError.Failed(message: "data")) }
print ${data ?? {|failure| failure.message}}
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """identity
data
"""
}

test test_bare_blocks_run_cleanup_before_exposing_values { |ctx|
  let output = test.run_script(
    ctx,
    r"""
proc mark(message: Str) [] { print $message }
error BlockError = Failed(message: Str)
pure failed() -> Result[Unit, BlockError] { Err(BlockError.Failed(message: "cleanup failure")) }
var count = 1
let selected = { defer { count = 2 }; (count) }
print $selected $count
let captured = try { { defer failed()?; "value" } }
print ${captured ?? {|failure| failure.message}}
let primary = try { { defer failed()?; assert false }; "value" }
print ${primary ?? {|failure| failure.message}}
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert """1 2
cleanup failure
""" in output.stdout
  assert "assertion" in output.stdout
}

test test_lexical_block_lint_preserves_scope_comments_and_converges { |ctx|
  let source = r"""proc mark(message: Str) [] { print $message }
var selected = 0
if true { # Keep the scope rationale.
  let local = 7
  defer mark("cleanup")
  selected = local
}
print $selected
"""
  let before = test.run_script(ctx, source)?
  let {success: before_succeeded, stderr: before_failure_details, ..} = before
  assert before_succeeded, before_failure_details
  let candidate = test.temp_file(ctx, name: "lexical-block-fix.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --fix $candidate ?
  let applied_succeeded = applied.status.exited_with(0)
  let applied_failure_details = applied.stderr
  assert applied_succeeded, applied_failure_details
  let fixed = candidate.read_text()?
  let condition_removed = "if true" not in fixed
  let fixed_source = fixed
  assert condition_removed, fixed_source
  assert "# Keep the scope rationale." in fixed
  assert "defer mark" in fixed
  let after = test.run_script(ctx, fixed)?
  let {success: after_succeeded, stderr: after_failure_details, ..} = after
  assert after_succeeded, after_failure_details
  assert after.stdout == before.stdout
  let repeated = run.capture --text "xsht" lint --fix $candidate ?
  let repeated_succeeded = repeated.status.exited_with(0)
  let repeated_failure_details = repeated.stderr
  assert repeated_succeeded, repeated_failure_details
  assert candidate.read_text()? == fixed
}

test test_lexical_block_lint_declines_changed_value_and_comment_boundaries { |ctx|
  for source in [
    """if true # condition rationale
{ let value = 1; print $value }
""",
    """if true { print yes } else { print no }
""",
    """let value = if true { 7 } else { 8 }
print $value
""",
  ] {
    let candidate = test.temp_file(ctx, name: "lexical-block-no-fix.xsh", contents: bytes.from_text(source))?
    let inspected = run.capture --text "xsht" lint $candidate ?
    let declined = "lint.lexical-block" not in inspected.stderr
    let inspection_details = inspected.stderr
    assert declined, inspection_details
    let _ = run.capture --text "xsht" lint --fix $candidate ?
    let fixed = candidate.read_text()?
    assert "if true" in fixed
  }
}

test test_bare_blocks_preserve_stream_cancellation_cleanup { |ctx|
  let output = test.run_script(
    ctx,
    r"""
stream values() [] -> Stream[Int] {
  defer { print outer }
  {
    defer { print inner }
    yield 1
    yield 2
  }
}
for value in values() { print $value; break }
print done
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """1
inner
outer
done
"""
}

test test_bare_blocks_do_not_expand_module_or_integer_exit_permissions { |ctx|
  let root = test.temp_dir(ctx, name: "lexical-module")?
  let module_path = fp"${root}/invalid.xsh"
  module_path.write("""##! Invalid executable module.
{ print forbidden }
## Exported name.
export let name = "invalid"
""")?
  let source = f"""
    let _ = module.load(p\"${module_path}\")?

    """
  let loaded = test.run_script(ctx, source)?
  let load_rejected = ! loaded.success
  let load_details = f"loaded status=${loaded.status}: ${loaded.stderr}"
  assert load_rejected, load_details
  assert "check.module-top-level" in loaded.stderr
  let load_silent = "forbidden" not in loaded.stdout
  let load_stdout = loaded.stdout
  assert load_silent, load_stdout
  let imported = test.run_script(
    ctx,
    """use invalid
""",
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  let import_rejected = ! imported.success
  let import_details = imported.stderr
  assert import_rejected, import_details
  assert "check.module-top-level" in imported.stderr
  let import_silent = "forbidden" not in imported.stdout
  let import_stdout = imported.stdout
  assert import_silent, import_stdout
  let bare = test.run_script(
    ctx,
    """{ 7 }
""",
  )?
  let succeeded = bare.success
  let failure_details = f"bare status=${bare.status}: ${bare.stderr}"
  assert succeeded, failure_details
  let status = bare.status
  let expected_status = 0
  let status_details = f"bare status=${bare.status}: ${bare.stderr}"
  assert status == expected_status, status_details
}

test test_bare_statement_discard_preserves_callable_return_contracts { |ctx|
  for source in [
    """pure value() -> Unit { 7 }
""",
    """pure value() -> Unit { { 7 } }
""",
    """proc value() [] -> Unit { 7 }
""",
    """proc value() [] -> Unit { { 7 } }
""",
  ] {
    let output = test.run_script(ctx, source)?
    let {success: succeeded, stderr: diagnostics, ..} = output
    assert ! succeeded, source
    let wrong_return = "expected Unit, found Int" in diagnostics
    assert wrong_return, diagnostics
  }

  let output = test.run_script(
    ctx,
    """proc value() [] { { 7 }; print retained }
value()
""",
  )?
  let {success: succeeded, stderr: diagnostics, stdout: written, ..} = output
  assert succeeded, diagnostics
  assert written == """retained
"""
}

test test_bare_block_resource_escape_keeps_explicit_cleanup_validity { |ctx|
  let output = test.run_script(
    ctx,
    r"""
let live = { fs.tempdir()? }
print ${live.exists(p".")?}
live.close()?
let closed = {
  let root = fs.tempdir()?
  defer root.close()?
  root
}
let inspected = closed.exists(p".")
print ${inspected ?? false}
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """true
false
"""
}

test test_bare_block_grep_and_refactor_preserve_literal_distinctions { |ctx|
  let reference = run.capture --text "xsht" api "language:core.bare-blocks" ?
  let reference_succeeded = reference.status.exited_with(0)
  let reference_failure_details = reference.stderr
  assert reference_succeeded, reference_failure_details
  assert "Bool values may be false" in reference.stdout
  assert "(selected)" in reference.stdout
  assert "let shorthand = {selected}" in reference.stdout
  let source = r"""pure calculate() -> Int { 9 }
let answer = { calculate() }
let row = {answer}
print $answer ${row.answer}
"""
  let candidate = test.temp_file(ctx, name: "lexical-block-refactor.xsh", contents: bytes.from_text(source))?
  let found = run.capture --text "xsht" grep "{ (EXPR) }" $candidate ?
  let found_succeeded = found.status.exited_with(0)
  let found_failure_details = found.stderr
  assert found_succeeded, found_failure_details
  assert "{ calculate() }" in found.stdout
  let row_absent = "let row" not in found.stdout
  let found_text = found.stdout
  assert row_absent, found_text
  let rewritten = run.capture --text "xsht" refactor "{ (EXPR) }" "{ (EXPR) }" $candidate ?
  let rewritten_succeeded = rewritten.status.exited_with(0)
  let rewritten_failure_details = rewritten.stderr
  assert rewritten_succeeded, rewritten_failure_details
  let fixed = candidate.read_text()?
  let holes_absent = "EXPR" not in fixed
  let fixed_source = fixed
  assert holes_absent, fixed_source
  assert "let row = {answer}" in fixed
  let checked = test.run_script(ctx, fixed)?
  let {success: checked_succeeded, stderr: checked_failure_details, ..} = checked
  assert checked_succeeded, checked_failure_details
  assert checked.stdout == """9 9
"""
}

test test_bare_blocks_preserve_implicit_stream_item_values { |ctx|
  let output = test.run_script(
    ctx,
    r"""
let selected = [{value: 7}] |> map { { .value } }
print ${selected[0]}
""",
  )?
  let {success: succeeded, stderr: failure_details, ..} = output
  assert succeeded, failure_details
  assert output.stdout == """7
"""
}
