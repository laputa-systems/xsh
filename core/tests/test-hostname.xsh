test test_hostname_short [process, env, error] { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/hostname.xsh" -- -s ?
  (output.trim() != "")
}
