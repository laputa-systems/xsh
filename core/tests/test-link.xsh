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
  let empty = run.capture --text ${ctx.xsh_bin} $script --
  assert empty.status.exited_with(1)
  assert empty.stderr == "link: missing operand\nTry 'link --help' for more information.\n"
  let missing = env ({LC_ALL: "C"}) { run.capture --text ${ctx.xsh_bin} $script -- p"source" }?
  assert missing.status.exited_with(1)
  assert missing.stderr == "link: missing operand after 'source'\nTry 'link --help' for more information.\n"

  let extra = env ({LC_ALL: "C"}) { run.capture --text ${ctx.xsh_bin} $script -- p"source" p"first" p"extra" }?
  assert extra.status.exited_with(1)
  assert extra.stderr == "link: extra operand 'extra'\nTry 'link --help' for more information.\n"
}
