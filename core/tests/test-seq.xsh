test test_seq_range { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/seq.xsh" -- 2 2 6 ?

  output == """2
4
6
"""
}

test test_seq_descending_negative_separator_and_width { |ctx|
  let descending = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/seq.xsh" -- 3 -2 -1 ?

  descending == """3
1
-1
"""

  let separated = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/seq.xsh" -- -s, 1 3 ?

  separated == """1,2,3
"""

  let padded = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/seq.xsh" -- -w 8 10 ?

  padded == """08
09
10
"""
}

test test_seq_rejects_zero_step { |ctx|
  let err = test.temp_path(ctx, name: "seq.err")
  let status = run.status ${ctx.xsh_bin} fp"${ctx.core_dir}/seq.xsh" -- 1 0 3 2> $err
  ! status.exited_with(0)
  "increment cannot be zero" in err.read_text()?
}
