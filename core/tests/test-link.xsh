test test_link { |ctx|
  let src = test.temp_file(ctx, name: "source.txt", contents: b"same\n")?
  let dst = test.temp_path(ctx, name: "linked.txt")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/link.xsh" -- $src $dst
  assert status.exited_with(0)

  assert dst.read_text()? == """same
"""
}
