test test_readlink { |ctx|
  let root = test.temp_dir(ctx, name: "readlink")?
  let target = fp"{root}/target.txt"
  let link = fp"{root}/link.txt"
  target.write("ok")
  fs.symlink(target, link)
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/readlink.xsh" -- $link ?
  assert "target.txt" in output
  let resolved = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/readlink.xsh" -- -f $link ?
  assert resolved.trim() == target.resolve()?.display()
  let resolved_long = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/readlink.xsh" -- --canonicalize $link ?
  assert resolved_long.trim() == target.resolve()?.display()
}

test test_readlink_canonicalizes_dangling_and_trailing_missing_paths { |ctx|
  let root = test.temp_dir(ctx, name: "readlink-canonical")?
  let target = fp"{root}/missing-target"
  let dangling = fp"{root}/dangling"
  fs.symlink(target, dangling)
  let resolved = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/readlink.xsh" -- -f $dangling ?
  assert resolved.trim() == target.display()
  let missing_slash = fp"{root}/missing/"
  let resolved_missing = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/readlink.xsh" -- -f $missing_slash ?
  assert resolved_missing.trim() == fp"{root}/missing".display()
}
