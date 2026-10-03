test test_todo_scan { |ctx|
  let root = test.temp_dir(ctx, name: "todos")?

  fp"${root}/main.rs".write("""// TODO: fix this
fn main() {}
// FIXME: also broken
""")?

  let output = run.text "xsh" "showcase/todo-scan.xsh" -- --root $root ?
  "FIXME" in output
  "TODO" in output
  "findings" in output
}
