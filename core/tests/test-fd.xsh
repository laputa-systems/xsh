test test_fd_finds_by_name_extension_and_type { |ctx|
  let root = test.temp_dir(ctx, name: "fd")?
  fp"{root}/alpha.txt".write("a")
  fp"{root}/beta.log".write("b")
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fd.xsh" -- alpha -t f -e txt $root ?
  assert "alpha.txt" in output
  assert ! ("beta.log" in output)
}

test test_fd_hidden_and_glob { |ctx|
  let root = test.temp_dir(ctx, name: "fd-hidden")?
  fp"{root}/.hidden.txt".write("hidden")
  fp"{root}/visible.txt".write("visible")
  let hidden_default = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fd.xsh" -- hidden $root ?
  assert hidden_default == ""
  let hidden = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fd.xsh" -- --hidden hidden $root ?
  assert ".hidden.txt" in hidden
  let globbed = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fd.xsh" -- --glob "*.txt" $root ?
  assert "visible.txt" in globbed
}

test test_fd_multiple_roots_exclude_depth_and_executable { |ctx|
  let left = test.temp_dir(ctx, name: "fd-left")?
  let right = test.temp_dir(ctx, name: "fd-right")?
  fp"{left}/keep.sh".write("echo keep")
  fs.chmod(fp"{left}/keep.sh", 0o755)
  fp"{left}/skip.log".write("skip")
  fs.mkdir(fp"{left}/nested")
  fp"{left}/nested/deep.sh".write("deep")
  fp"{right}/other.sh".write("other")
  fs.chmod(fp"{right}/other.sh", 0o755)
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fd.xsh" -- --glob "*.sh" -t x -E "skip*" -d1 $left $right ?
  assert "keep.sh" in output
  assert "other.sh" in output
  assert ! ("skip.log" in output)
  assert ! ("deep.sh" in output)
}
