test test_dirname { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/dirname.xsh" -- /tmp/demo/file.txt ?
  assert output.trim() == "/tmp/demo"
  let many = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/dirname.xsh" -- /tmp/a/one.txt /tmp/b/two.txt ?
  assert "/tmp/a" in many
  assert "/tmp/b" in many
}

test test_dirname_root_empty_and_repeated_separators { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/dirname.xsh" -- "" / /// /usr//lib foo/bar/ ?
  assert output == ".\n/\n/\n/usr\nfoo\n", output
}
