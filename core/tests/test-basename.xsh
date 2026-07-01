pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_basename_basic() [process, error] {
  let output = run.text xsh_bin() basename.xsh -- /tmp/demo.txt ?
  test.eq(output.trim(), "demo.txt")?
}

proc test_basename_suffix_and_multiple() [process, error] {
  let output = run.text xsh_bin() basename.xsh -- -a -s .txt /tmp/demo.txt /tmp/other.txt ?

  test.eq(
    output.trim(),
    """demo
other""",
  )?
}
