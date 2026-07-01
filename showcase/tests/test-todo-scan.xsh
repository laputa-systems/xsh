pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_todo_scan(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "todos")?

  fp"${root}/main.rs".write("""// TODO: fix this
fn main() {}
// FIXME: also broken
""")?

  let output = run.text xsh_bin() "showcase/todo-scan.xsh" -- --root $root ?
  test.contains(output, "FIXME")?
  test.contains(output, "TODO")?
  test.contains(output, "findings")?
}
