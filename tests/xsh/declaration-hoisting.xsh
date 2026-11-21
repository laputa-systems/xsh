type ScriptRun = { status: Int, output: Str, errors: Str }

proc run_script(ctx: TestContext, name: Str, source: Str) [error, io, process] -> ScriptRun {
  let root = test.temp_dir(ctx, name: name)?.resolve()?
  let script = fp"{root}/main.xsh"
  script.write(source)?
  let stdout = fp"{root}/stdout.txt"
  let stderr = fp"{root}/stderr.txt"
  let xsh = ctx.xsh_bin
  let command = process.command_argv(xsh, [xsh, script], cwd: root, stdout: stdout, stderr: stderr)
  let status = process.run(command)?
  ScriptRun(
    status: status.shell_code()?,
    output: stdout.read_text()?,
    errors: stderr.read_text()?,
  )
}

test test_declarations_are_visible_before_the_line_that_declares_them { |ctx|
  let result = run_script(ctx, "hoisted", r"""print (early())
print (rows().collect().len())
print (double(7))
print (Point(x: 1).y)
print (name(Info))
print (Fetch.Missing("gone").message)
proc early() -> Int { 1 }
stream rows() [] -> Stream[Int] {
  yield 1
}
pure double(value: Int) -> Int { value * 2 }
type Point = { x: Int, y: Int = 7 }
enum Level { Info, Warn }
pure name(level: Level) -> Str {
  match level {
    Info => "info"
    Warn => "warn"
  }
}
error Fetch = Missing
""")
  assert result.status == 0, result.errors
  assert result.output == "1\n1\n14\n7\ninfo\ngone\n", result.output
}

test test_a_hoisted_callable_fails_when_its_captured_binding_has_not_run { |ctx|
  let result = run_script(ctx, "uninitialized", r"""print (read())
var count = 5
proc read() -> Int {
  count
}
""")
  assert result.status == 3, f"status {result.status}: {result.errors}"
  assert "unknown captured name `count`" in result.errors, result.errors
}

test test_a_callable_does_not_see_top_level_bindings_declared_after_it { |ctx|
  let result = run_script(ctx, "later-binding", r"""proc read() -> Int {
  count
}
var count = 5
print (read())
""")
  assert result.status == 2, f"status {result.status}: {result.errors}"
  assert "count" in result.errors, result.errors
}
