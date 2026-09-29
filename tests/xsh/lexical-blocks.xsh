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
      if value == 2 { continue }
      if value == 3 { break }
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
print unreachable
""")?
  test.eq(output.success, false)?
  test.eq(output.stdout, "cleanup\n")?
  test.contains(output.stderr, "assertion")?
}
