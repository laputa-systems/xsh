test test_link { |ctx|
  let src = test.temp_file(ctx, name: "source.txt", contents: b"same\n")?
  let dst = test.temp_path(ctx, name: "linked.txt")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/link.xsh" -- $src $dst
  assert status.exited_with(0)

  assert dst.read_text()? == """same
"""
}

test test_link_rejects_operand_counts_with_usage { |ctx|
  let script = fp"{ctx.core_dir}/link.xsh"
  let missing = run.capture --text ${ctx.xsh_bin} $script -- p"source"
  assert missing.status.exited_with(1)
  assert "2 values required" in missing.stderr
  assert "Usage: link FILE1 FILE2" in missing.stderr

  let extra = run.capture --text ${ctx.xsh_bin} $script -- p"source" p"first" p"extra"
  assert extra.status.exited_with(1)
  assert "2 values required" in extra.stderr
  assert "Usage: link FILE1 FILE2" in extra.stderr
}
