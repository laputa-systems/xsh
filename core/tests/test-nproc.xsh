test test_nproc [process, env, error] { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/nproc.xsh" ?
  test.ok(output.trim() != "")?
}
