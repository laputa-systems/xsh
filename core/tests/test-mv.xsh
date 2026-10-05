test test_mv_file_and_target_directory { |ctx|
  let root = test.temp_dir(ctx, name: "mv")?
  let src = fp"{root}/src.txt"
  src.write("hello")
  let dir = fp"{root}/dir"
  dir.mkdir()
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- -t $dir $src
  assert ! src.exists()?
  assert fp"{dir}/src.txt".read_text()? == "hello"
}

test test_mv_dangling_no_clobber_and_last_option { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("data")
  dest.symlink(to: fp"{root}/missing")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- -fn $source $dest
  assert source.exists()?
  assert fs.stat(dest)?.kind == "symlink"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- -nf $source $dest
  assert ! source.exists()?
  assert dest.read_text()? == "data"
}

test test_mv_backup_and_update { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("new")
  dest.write("old")
  fs.set_times(source, mtime_ns: 1000000000)
  fs.set_times(dest, mtime_ns: 2000000000)
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- -u $source $dest
  assert source.exists()?
  assert dest.read_text()? == "old"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- -b $source $dest
  assert fp"{dest}~".read_text()? == "old"
  assert dest.read_text()? == "new"
}

test test_mv_continues_after_missing_source { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let dest = fp"{root}/dir"
  source.write("data")
  dest.mkdir()
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- fp"{root}/missing" $source $dest
  assert status.exited_with(1)
  assert fp"{dest}/source".read_text()? == "data"
}

test test_mv_numbered_backup_uses_highest_existing_number { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("new")
  dest.write("old")
  fp"{dest}.~3~".write("older")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- --backup=existing $source $dest
  assert fp"{dest}.~4~".read_text()? == "old"
  assert fp"{dest}.~3~".read_text()? == "older"
}

test test_mv_last_update_option_controls_replacement { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("new")
  dest.write("old")
  fs.set_times(source, mtime_ns: 1000000000)
  fs.set_times(dest, mtime_ns: 2000000000)
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- --update=all -u $source $dest
  assert source.exists()?
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- -u --update=all $source $dest
  assert ! source.exists()?
  assert dest.read_text()? == "new"
}

test test_mv_interactive_decline_leaves_both_files { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("new")
  dest.write("old")
  let script = fp"{ctx.core_dir}/mv.xsh"
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let command = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "-i", source.display(), dest.display()],
    root, {}, b"n\n", out, err)
  assert process.run(command)?.exited_with(0)
  assert source.read_text()? == "new"
  assert dest.read_text()? == "old"
  assert "overwrite" in err.read_text()?
}
