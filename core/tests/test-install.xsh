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

test test_install_compare_checks_bytes_after_first_chunk { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  let chunks = ["x"] |> repeat(65536)
  let prefix = chunks.join("")
  source.write(f"{prefix}a")
  dest.write(f"{prefix}b")
  dest.chmod(0o755)
  let old = fs.stat(dest)?.ino
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -C $source $dest
  assert fs.stat(dest)?.ino != old
  assert dest.read_text()? == f"{prefix}a"
}

test test_install_parallel_publications_do_not_share_staging_names { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  source.write("payload")
  let statuses = range(8) |> par-map(jobs: 4) { |index|
    let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -D $source fp"{root}/shared/child/file-{index}"
    status.exited_with(0)
  }
  for succeeded in statuses { assert succeeded }
  for index in range(8) { assert fp"{root}/shared/child/file-{index}".read_text()? == "payload" }
}

test test_install_directory_trailing_dot_creates_directory { |ctx|
  let root = test.temp_dir(ctx)?
  let directory = fp"{root}/directory/."
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -d $directory
  assert fp"{root}/directory".is_dir()?
}

test test_install_missing_directory_target_does_not_create_it { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  source.write("payload")
  let target = fp"{root}/missing/"
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- -D $source $target
  assert status.exited_with(1)
  assert ! target.exists()?
  assert source.read_text()? == "payload"
}

test test_install_compare_copies_fifo_source_symlink_in_bounded_chunks { |ctx|
  let root = test.temp_dir(ctx)?
  let fifo = fp"{root}/fifo"
  fs.mkfifo(fifo, 0o600)
  let source = fp"{root}/source"
  source.symlink(to: fifo)
  let payload = fp"{root}/payload"
  let chunks = ["x"] |> repeat(131072)
  let expected = bytes.from_text(chunks.join("") + "tail")
  payload.write(expected)
  let writer_script = fp"{root}/writer.xsh"
  writer_script.write(f"fp\"{fifo}\".write(fp\"{payload}\".read_bytes()?)\n")
  let writer = spawn run ${ctx.xsh_bin} $writer_script ?
  defer writer.cancel(kill_after: 100ms)
  let dest = fp"{root}/dest"
  dest.write(b"")
  dest.chmod(0o755)
  let script = fp"{ctx.core_dir}/install.xsh"
  let reader_command = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "-C", source.display(), dest.display()],
    root, {}, b"", fp"{root}/out", fp"{root}/err", timeout: 3s)
  let status = process.run(reader_command)?
  assert status.exited_with(0)
  assert (wait writer?).exited_with(0)
  assert dest.read_bytes()? == expected
  assert fs.stat(dest)?.kind == "file"
}

test test_install_reads_stdin_descriptor_alias { |ctx|
  let root = test.temp_dir(ctx)?
  let script = fp"{ctx.core_dir}/install.xsh"
  let dest = fp"{root}/dest"
  let payload = b"stream\0payload\n"
  let command = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "-m", "640", "/dev/fd/0", dest.display()],
    root, {}, payload, fp"{root}/out", fp"{root}/err")
  assert process.run(command)?.exited_with(0)
  assert dest.read_bytes()? == payload
  assert fs.stat(dest)?.mode.bit_and(0o7777) == 0o640
}

test test_install_source_symlink_cannot_replace_its_own_target { |ctx|
  let root = test.temp_dir(ctx)?
  let target = fp"{root}/target"
  target.write("keep")
  let source = fp"{root}/source"
  source.symlink(to: target)
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/install.xsh" -- $source $target
  assert status.exited_with(1)
  assert target.read_text()? == "keep"
  assert source.readlink()? == target
}
