test test_bare_blocks_preserve_values_and_cleanup [error] { |ctx|
  let output = test.run_script(ctx, """proc mark(message: Str) [] { print $message }
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
""")?
  test.eq(output.success, true)?
  test.eq(output.stdout, "statement cleanup\nvalue cleanup\n42 false 42 42\n")?
}

test test_bare_statement_blocks_assert_false [error] { |ctx|
  let output = test.run_script(ctx, """proc mark(message: Str) [] { print $message }
{
  defer mark("cleanup")
  false
}

print unreachable
""")?
  test.eq(output.success, false)?
  test.eq(output.stdout, "cleanup\n")?
  test.ok("assertion" in output.stderr)?
}


pure lexical_tail() -> Bool {
  { false }
}

test test_bare_value_tail_preserves_false [error] {
  test.eq(lexical_tail(), false)?
}

test test_bare_blocks_preserve_lexical_transfers [error] { |ctx|
  let output = test.run_script(ctx, """proc mark(message: Str) [] { print $message }
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
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "return cleanup\n7\n1\nloop cleanup\nloop cleanup\nloop cleanup\n")?
}

test test_bare_blocks_preserve_literal_and_callable_tails [error] { |ctx|
  let output = test.run_script(ctx, r"""
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
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "0 1 4 9 5 6 3\n")?
}

test test_bare_blocks_keep_checker_and_result_boundaries [error] { |ctx|
  for source in [
    "{ let hidden = 1 }\nprint $hidden\n",
    "let value = { |input| input }\n",
    "{ Ok(7) }\nprint unreachable\n",
    "{ break }\n",
    "{ continue }\n",
  ] {
    let output = test.run_script(ctx, source)?
    test.ok(! output.success, source + output.stderr)?
  }
  let output = test.run_script(ctx, r"""
error BlockError = Failed(message: Str)
pure failed() -> Result[Unit, BlockError] { Err(BlockError.Failed(message: "identity")) }
let captured = try { { failed()? }; "unreachable" }
print ${captured ?? {|failure| failure.message}}
let data = { Err(BlockError.Failed(message: "data")) }
print ${data ?? {|failure| failure.message}}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "identity\ndata\n")?
}

test test_bare_blocks_run_cleanup_before_exposing_values [error] { |ctx|
  let output = test.run_script(ctx, r"""
proc mark(message: Str) [] { print $message }
error BlockError = Failed(message: Str)
pure failed() -> Result[Unit, BlockError] { Err(BlockError.Failed(message: "cleanup failure")) }
var count = 1
let selected = { defer { count = 2 }; (count) }
print $selected $count
let captured = try { { defer failed()?; "value" } }
print ${captured ?? {|failure| failure.message}}
let primary = try { { defer failed()?; false }; "value" }
print ${primary ?? {|failure| failure.message}}
""")?
  test.ok(output.success, output.stderr)?
  test.ok("1 2\ncleanup failure\n" in output.stdout)?
  test.ok("assertion" in output.stdout)?
}

test test_lexical_block_lint_preserves_scope_comments_and_converges [fs, process, error] { |ctx|
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
  test.ok(before.success, before.stderr)?
  let candidate = test.temp_file(ctx, name: "lexical-block-fix.xsh", contents: bytes.from_text(source))?
  let applied = run.capture --text "xsht" lint --fix $candidate ?
  test.ok(applied.status.exited_with(0), applied.stderr)?
  let fixed = candidate.read_text()?
  test.ok(("if true" not in fixed), fixed)?
  test.ok("# Keep the scope rationale." in fixed)?
  test.ok("defer mark" in fixed)?
  let after = test.run_script(ctx, fixed)?
  test.ok(after.success, after.stderr)?
  test.eq(after.stdout, before.stdout)?
  let repeated = run.capture --text "xsht" lint --fix $candidate ?
  test.ok(repeated.status.exited_with(0), repeated.stderr)?
  test.eq(candidate.read_text()?, fixed)?
}

test test_lexical_block_lint_declines_changed_value_and_comment_boundaries [fs, process, error] { |ctx|
  for source in [
    "if true # condition rationale\n{ let value = 1; print $value }\n",
    "if true { print yes } else { print no }\n",
    "let value = if true { 7 } else { 8 }\nprint $value\n",
    "let value = true\nif true {value}\n",
  ] {
    let candidate = test.temp_file(ctx, name: "lexical-block-no-fix.xsh", contents: bytes.from_text(source))?
    let inspected = run.capture --text "xsht" lint $candidate ?
    test.ok(("lint.lexical-block" not in inspected.stderr), inspected.stderr)?
    let applied = run.capture --text "xsht" lint --fix $candidate ?
    let fixed = candidate.read_text()?
    test.ok("if true" in fixed)?
  }
}

test test_bare_blocks_preserve_stream_cancellation_cleanup [error] { |ctx|
  let output = test.run_script(ctx, r"""
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
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "1\ninner\nouter\ndone\n")?
}

test test_bare_blocks_do_not_expand_module_or_integer_exit_permissions [fs, error] { |ctx|
  let root = test.temp_dir(ctx, name: "lexical-module")?
  let module_path = fp"${root}/invalid.xsh"
  module_path.write("##! Invalid executable module.\n{ print forbidden }\n## Exported name.\nexport let name = \"invalid\"\n")?
  let loaded = test.run_script(ctx, f"let _ = module.load(p\"${module_path.display()}\")?\n")?
  test.ok(! loaded.success, f"loaded status=${loaded.status}: ${loaded.stderr}")?
  test.ok("check.module-top-level" in loaded.stderr)?
  test.ok(("forbidden" not in loaded.stdout), loaded.stdout)?
  let imported = test.run_script(ctx, "use invalid\n", [], {XSH_MODULE_PATH: root.display()})?
  test.ok(! imported.success, imported.stderr)?
  test.ok("check.module-top-level" in imported.stderr)?
  test.ok(("forbidden" not in imported.stdout), imported.stdout)?
  let bare = test.run_script(ctx, "{ 7 }\n")?
  test.ok(bare.success, f"bare status=${bare.status}: ${bare.stderr}")?
  test.eq(bare.status, 0, message: f"bare status=${bare.status}: ${bare.stderr}")?
}

test test_bare_block_resource_escape_keeps_explicit_cleanup_validity [error] { |ctx|
  let output = test.run_script(ctx, r"""
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
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "true\nfalse\n")?
}

test test_bare_block_grep_and_refactor_preserve_literal_distinctions [fs, process, error] { |ctx|
  let reference = run.capture --text "xsht" api "language:core.bare-blocks" ?
  test.ok(reference.status.exited_with(0), reference.stderr)?
  test.ok("Bool values may be false" in reference.stdout)?
  test.ok("let grouped = { (selected) }" in reference.stdout)?
  let source = r"""pure calculate() -> Int { 9 }
let answer = { calculate() }
let row = {answer}
print $answer ${row.answer}
"""
  let candidate = test.temp_file(ctx, name: "lexical-block-refactor.xsh", contents: bytes.from_text(source))?
  let found = run.capture --text "xsht" grep "{ (EXPR) }" $candidate ?
  test.ok(found.status.exited_with(0), found.stderr)?
  test.ok("{ calculate() }" in found.stdout)?
  test.ok(("let row" not in found.stdout), found.stdout)?
  let rewritten = run.capture --text "xsht" refactor "{ (EXPR) }" "{ (EXPR) }" $candidate ?
  test.ok(rewritten.status.exited_with(0), rewritten.stderr)?
  let fixed = candidate.read_text()?
  test.ok(("EXPR" not in fixed), fixed)?
  test.ok("let row = {answer}" in fixed)?
  let checked = test.run_script(ctx, fixed)?
  test.ok(checked.success, checked.stderr)?
  test.eq(checked.stdout, "9 9\n")?
}

test test_bare_blocks_preserve_implicit_stream_item_values [error] { |ctx|
  let output = test.run_script(ctx, r"""
let selected = [{value: 7}] |> map { { .value } }
print ${selected[0]}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "7\n")?
}
