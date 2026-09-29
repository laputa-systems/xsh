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
  test.contains(output.stderr, "assertion")?
}


pure lexical_tail() -> Bool {
  { false }
}

proc test_bare_value_tail_preserves_false() [error] {
  test.eq(lexical_tail(), false)?
}

proc test_bare_blocks_preserve_lexical_transfers(ctx: TestContext) [error] {
  let output = test.run_script(ctx, """proc mark(message: Str) [] { print $message }
proc answer() [] -> Int {
  {
    defer mark("return cleanup")
    return 7
  }
  0
}
proc visit() [] {
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
  test.eq(output.success, true)?
  test.eq(output.stdout, "return cleanup\n7\n1\nloop cleanup\nloop cleanup\nloop cleanup\n")?
}

proc test_bare_blocks_preserve_literal_and_callable_tails(ctx: TestContext) [error] {
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

proc test_bare_blocks_keep_checker_and_result_boundaries(ctx: TestContext) [error] {
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

proc test_bare_blocks_run_cleanup_before_exposing_values(ctx: TestContext) [error] {
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
  test.contains(output.stdout, "1 2\ncleanup failure\n")?
  test.contains(output.stdout, "assertion")?
}

proc test_lexical_block_lint_preserves_scope_comments_and_converges(ctx: TestContext) [fs, process, error] {
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
  test.ok(! fixed.contains("if true"), fixed)?
  test.contains(fixed, "# Keep the scope rationale.")?
  test.contains(fixed, "defer mark")?
  let after = test.run_script(ctx, fixed)?
  test.ok(after.success, after.stderr)?
  test.eq(after.stdout, before.stdout)?
  let repeated = run.capture --text "xsht" lint --fix $candidate ?
  test.ok(repeated.status.exited_with(0), repeated.stderr)?
  test.eq(candidate.read_text()?, fixed)?
}

proc test_lexical_block_lint_declines_changed_value_and_comment_boundaries(ctx: TestContext) [fs, process, error] {
  for source in [
    "if true # condition rationale\n{ let value = 1; print $value }\n",
    "if true { print yes } else { print no }\n",
    "let value = if true { 7 } else { 8 }\nprint $value\n",
    "let value = true\nif true {value}\n",
  ] {
    let candidate = test.temp_file(ctx, name: "lexical-block-no-fix.xsh", contents: bytes.from_text(source))?
    let inspected = run.capture --text "xsht" lint $candidate ?
    test.ok(! inspected.stderr.contains("lint.lexical-block"), inspected.stderr)?
    let applied = run.capture --text "xsht" lint --fix $candidate ?
    let fixed = candidate.read_text()?
    test.contains(fixed, "if true")?
  }
}

proc test_bare_blocks_preserve_stream_cancellation_cleanup(ctx: TestContext) [error] {
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

proc test_bare_blocks_do_not_expand_module_or_integer_exit_permissions(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "lexical-module")?
  let module_path = fp"${root}/invalid.xsh"
  module_path.write("##! Invalid executable module.\n{ print forbidden }\n## Exported name.\nexport let name = \"invalid\"\n")?
  let loaded = test.run_script(ctx, f"let _ = module.load(p\"${module_path.display()}\")?\n")?
  test.ok(! loaded.success, f"loaded status=${loaded.status}: ${loaded.stderr}")?
  test.contains(loaded.stderr, "check.module-top-level")?
  test.ok(! loaded.stdout.contains("forbidden"), loaded.stdout)?
  let imported = test.run_script(ctx, "use invalid\n", [], {XSH_MODULE_PATH: root.display()})?
  test.ok(! imported.success, imported.stderr)?
  test.contains(imported.stderr, "check.module-top-level")?
  test.ok(! imported.stdout.contains("forbidden"), imported.stdout)?
  let bare = test.run_script(ctx, "{ 7 }\n")?
  test.ok(bare.success, f"bare status=${bare.status}: ${bare.stderr}")?
  test.eq(bare.status, 0, message: f"bare status=${bare.status}: ${bare.stderr}")?
}

proc test_bare_block_resource_escape_keeps_explicit_cleanup_validity(ctx: TestContext) [error] {
  let output = test.run_script(ctx, r"""
let live = { fs.tempdir()? }
print ${fs.root_exists(live, p".")?}
fs.close_root(live)?
let closed = {
  let root = fs.tempdir()?
  defer fs.close_root(root)?
  root
}
let inspected = fs.root_exists(closed, p".")
print ${inspected ?? false}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "true\nfalse\n")?
}

proc test_bare_block_grep_and_refactor_preserve_literal_distinctions(ctx: TestContext) [fs, process, error] {
  let reference = run.capture --text "xsht" api "language:core.bare-blocks" ?
  test.ok(reference.status.exited_with(0), reference.stderr)?
  test.contains(reference.stdout, "Bool values may be false")?
  test.contains(reference.stdout, "let grouped = { (selected) }")?
  let source = r"""pure calculate() -> Int { 9 }
let answer = { calculate() }
let row = {answer}
print $answer ${row.answer}
"""
  let candidate = test.temp_file(ctx, name: "lexical-block-refactor.xsh", contents: bytes.from_text(source))?
  let found = run.capture --text "xsht" grep "{ (EXPR) }" $candidate ?
  test.ok(found.status.exited_with(0), found.stderr)?
  test.contains(found.stdout, "{ calculate() }")?
  test.ok(! found.stdout.contains("let row"), found.stdout)?
  let rewritten = run.capture --text "xsht" refactor "{ (EXPR) }" "{ (EXPR) }" $candidate ?
  test.ok(rewritten.status.exited_with(0), rewritten.stderr)?
  let fixed = candidate.read_text()?
  test.ok(! fixed.contains("EXPR"), fixed)?
  test.contains(fixed, "let row = {answer}")?
  let checked = test.run_script(ctx, fixed)?
  test.ok(checked.success, checked.stderr)?
  test.eq(checked.stdout, "9 9\n")?
}

proc test_bare_blocks_preserve_implicit_stream_item_values(ctx: TestContext) [error] {
  let output = test.run_script(ctx, r"""
let selected = [{value: 7}] |> map { { .value } }
print ${selected[0]}
""")?
  test.ok(output.success, output.stderr)?
  test.eq(output.stdout, "7\n")?
}
