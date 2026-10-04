test test_nproc { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/nproc.xsh" ?
  assert output.trim() != ""
}
