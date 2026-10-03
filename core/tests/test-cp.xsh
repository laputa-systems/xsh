test test_cp_file_and_recursive_dir { |ctx|
  let root = test.temp_dir(ctx, name: "cp")?
  let src = fp"${root}/src.txt"
  let dst = fp"${root}/dst.txt"
  src.write("hello")?
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/cp.xsh" -- $src $dst ?
  dst.read_text()? == "hello"
  let dir = fp"${root}/dir"
  dir.mkdir()?
  fp"${dir}/nested.txt".write("nested")?
  let out = fp"${root}/out"
  run.text ${ctx.xsh_bin} fp"${ctx.core_dir}/cp.xsh" -- -R $dir $out ?
  fp"${out}/nested.txt".read_text()? == "nested"
}
