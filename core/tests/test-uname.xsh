test test_uname_all [process, env, error] { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/uname.xsh" -- -a ?
  (output.fields().len() >= 3)
}
