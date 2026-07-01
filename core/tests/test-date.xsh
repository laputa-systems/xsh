pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_date_format() [process, error] {
  let output = run.text xsh_bin() date.xsh -- -u +%Y ?
  test.eq(output.trim().count_chars(), 4)?
  let offset = run.text xsh_bin() date.xsh -- -u +%:z ?
  test.eq(offset.trim(), "+00:00")?
}
