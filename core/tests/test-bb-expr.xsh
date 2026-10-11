use support.uu

# origin: busybox expr/expr-big
test test_bb_expr_expr_big_f324ed8b { |ctx|
  let s = uu.scene(ctx)?
  for args in [
    ["0", "<", "3000000000"],
    ["0", "<", "9223372036854775807"],
    ["-9223372036854775800", "<", "9223372036854775807"],
  ] {
    let r = uu.invoke(s, "expr", args)?
    let lines = r.stdout.utf8()?.split("\n")
    assert lines[0] == "1"
    for i in range(1, lines.len()) { assert lines[i] == "" }
  }
}

# origin: busybox expr/expr-works
test test_bb_expr_expr_works_2459ad09 { |ctx|
  let s = uu.scene(ctx)?
  for args in [
    ["1", "|", "1"], ["1", "|", "0"], ["0", "|", "1"],
    ["1", "&", "1"], ["0", "<", "1"], ["1", ">", "0"],
    ["0", "<=", "1"], ["1", "<=", "1"], ["1", ">=", "0"], ["1", ">=", "1"],
    ["1", "+", "2"], ["2", "-", "1"], ["2", "*", "3"], ["12", "/", "2"], ["12", "%", "5"],
  ] {
    let r = uu.invoke(s, "expr", args)?
    uu.succeeds(r)
  }
  for args in [
    ["0", "|", "0"], ["1", "&", "0"], ["0", "&", "1"], ["0", "&", "0"],
    ["1", "<", "0"], ["0", ">", "1"], ["1", "<=", "0"], ["0", ">=", "1"],
  ] {
    let r = uu.invoke(s, "expr", args)?
    uu.fails_with_code(r, 1)
  }
}
