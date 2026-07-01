pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_stat(ctx: TestContext) [fs, process, error] {
  let target = test.temp_file(ctx, name: "stat.txt", contents: b"hello")?
  let output = run.text xsh_bin() stat.xsh -- $target ?
  test.contains(output, "kind file")?
  test.contains(output, "size 5")?
  let formatted = run.text xsh_bin() stat.xsh -- -c "%s %F %n" $target ?
  test.contains(formatted, "5 regular file")?
  test.contains(formatted, "stat.txt")?
  let modes = run.text xsh_bin() stat.xsh -- -c "%a %A %U %G" $target ?
  test.contains(modes, "rw")?
}
