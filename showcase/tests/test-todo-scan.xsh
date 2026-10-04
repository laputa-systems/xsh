test test_todo_scan { |ctx|
  let root = test.temp_dir(ctx, name: "todos")?

  fp"{root}/main.rs".write("""// TODO: fix this
fn main() {}
// FIXME: also broken
""")?

  let output = run.text "xsh" "showcase/todo-scan.xsh" -- --root $root ?
  assert "FIXME" in output
  assert "TODO" in output
  assert "findings" in output
}
