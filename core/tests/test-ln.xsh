test test_ln_symbolic_force { |ctx|
  let root = test.temp_dir(ctx, name: "ln")?
  let src = fp"{root}/src.txt"
  let dst = fp"{root}/dst.txt"
  src.write("new")
  dst.write("old")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/ln.xsh" -- -sf $src $dst
  assert "src.txt" in dst.readlink()?.display()
}

test test_ln_no_dereference_replaces_directory_symlink { |ctx|
  let root = test.temp_dir(ctx)?
  let directory = fp"{root}/directory"
  directory.mkdir()
  let dest = fp"{root}/dest"
  dest.symlink(to: directory)
  let source = fp"{root}/source"
  source.write("data")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/ln.xsh" -- -sfn $source $dest
  assert dest.readlink()? == source
  assert ! fp"{directory}/source".exists()?
}

test test_ln_failed_force_preserves_destination { |ctx|
  let root = test.temp_dir(ctx)?
  let dest = fp"{root}/dest"
  dest.write("keep")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/ln.xsh" -- -f fp"{root}/missing" $dest
  assert status.exited_with(1)
  assert dest.read_text()? == "keep"
}

test test_ln_relative_dangling_and_physical_hard_links { |ctx|
  let root = test.temp_dir(ctx)?
  let dest = fp"{root}/dest"
  let missing = fp"{root}/missing"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/ln.xsh" -- -sr $missing $dest
  assert dest.readlink()? == p"missing"
  let hard = fp"{root}/hard"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/ln.xsh" -- $dest $hard
  assert fs.stat(hard)?.ino == fs.stat(dest)?.ino
  assert fs.stat(hard)?.kind == "symlink"
}

test test_ln_backup_restored_on_failed_link { |ctx|
  let root = test.temp_dir(ctx)?
  let dest = fp"{root}/dest"
  dest.write("keep")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/ln.xsh" -- -b fp"{root}/missing" $dest
  assert status.exited_with(1)
  assert dest.read_text()? == "keep"
  assert ! fp"{dest}~".exists()?
}

test test_ln_force_same_entry_keeps_source { |ctx|
  let source = test.temp_file(ctx, contents: b"keep")?
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/ln.xsh" -- -sf $source $source
  assert status.exited_with(1)
  assert fs.stat(source)?.kind == "file"
  assert source.read_text()? == "keep"
}

test test_ln_interactive_respects_last_force_option { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("new")
  dest.write("old")
  let script = fp"{ctx.core_dir}/ln.xsh"
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let decline = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "-sfi", source.display(), dest.display()],
    root, {}, b"n\n", out, err)
  assert process.run(decline)?.exited_with(0)
  assert dest.read_text()? == "old"
  let force = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "-sif", source.display(), dest.display()],
    root, {}, b"n\n", out, err)
  assert process.run(force)?.exited_with(0)
  assert dest.readlink()? == source
  assert err.read_text()? == ""
}
