test test_install_follows_source_and_replaces_destination_link { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  source.write("new")
  let source_link = fp"{root}/source-link"
  source_link.symlink(to: source)
  let old = fp"{root}/old"
  old.write("keep")
  let dest = fp"{root}/dest"
  dest.symlink(to: old)
  fs.set_times(source, atime_ns: 1230000000, mtime_ns: 4560000000)
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -pm 640 $source_link $dest
  assert dest.read_text()? == "new"
  assert old.read_text()? == "keep"
  assert fs.stat(dest)?.kind == "file"
  assert fs.stat(dest)?.mode.bit_and(0o7777) == 0o640
  assert fs.stat(dest)?.mtime_ns == 4560000000
}

test test_install_directory_symbolic_mode_and_parents { |ctx|
  let root = test.temp_dir(ctx)?
  let dest = fp"{root}/a/b"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -dm u=rwx,g=rx,o= $dest
  assert dest.is_dir()?
  assert fs.stat(dest)?.mode.bit_and(0o7777) == 0o750
  let source = fp"{root}/source"
  source.write("payload")
  let file = fp"{root}/c/d/file"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -D $source $file
  assert file.read_text()? == "payload"
  assert fs.stat(file)?.mode.bit_and(0o7777) == 0o755
}

test test_install_compare_preserves_inode_and_backup_keeps_previous { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("payload")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- $source $dest
  let ino = fs.stat(dest)?.ino
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -C $source $dest
  assert fs.stat(dest)?.ino == ino
  source.write("changed")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -b $source $dest
  assert fp"{dest}~".read_text()? == "payload"
  assert dest.read_text()? == "changed"
}

test test_install_backup_cannot_destroy_source { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/file~"
  let dest = fp"{root}/file"
  source.write("source")
  dest.write("destination")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -b $source $dest
  assert status.exited_with(1)
  assert source.read_text()? == "source"
  assert dest.read_text()? == "destination"
}

test test_install_null_device_and_conditional_execute_mode { |ctx|
  let dest = test.temp_path(ctx)
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -m ug+rwX,o+rX /dev/null $dest
  assert dest.read_bytes()? == b""
  assert fs.stat(dest)?.mode.bit_and(0o7777) == 0o664
}

test test_install_directory_continues_after_existing_file { |ctx|
  let root = test.temp_dir(ctx)?
  let existing = fp"{root}/existing"
  existing.write("keep")
  let later = fp"{root}/later"
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -d $existing $later
  assert status.exited_with(1)
  assert existing.read_text()? == "keep"
  assert later.is_dir()?
}
