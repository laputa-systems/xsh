pure xsh_bin() -> Path {
  return ../target/debug/xsh
}

proc test_seq_range() [process, error] {
  let output = run.text xsh_bin() seq.xsh -- 2 2 6 ?

  test.eq(
    output,
    """2
4
6
""",
  )?
}

proc test_seq_descending_negative_separator_and_width() [process, error] {
  let descending = run.text xsh_bin() seq.xsh -- 3 -2 -1 ?

  test.eq(
    descending,
    """3
1
-1
""",
  )?

  let separated = run.text xsh_bin() seq.xsh -- -s, 1 3 ?

  test.eq(
    separated,
    """1,2,3
""",
  )?

  let padded = run.text xsh_bin() seq.xsh -- -w 8 10 ?

  test.eq(
    padded,
    """08
09
10
""",
  )?
}

proc test_seq_rejects_zero_step(ctx: TestContext) [fs, process, error] {
  let err = test.temp_path(ctx, name: "seq.err")
  let status = run.status xsh_bin() seq.xsh -- 1 0 3 2> $err
  test.ok(! status.exited_with(0))?
  test.contains(err.read_text()?, "increment cannot be zero")?
}
