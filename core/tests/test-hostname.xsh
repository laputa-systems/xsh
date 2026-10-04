test test_hostname_short { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/hostname.xsh" -- -s ?
  assert output.trim() != ""
}
