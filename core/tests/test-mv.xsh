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
  assert process.run(command)?.exited_with(1)
  assert source.read_text()? == "new"
  assert dest.read_text()? == "old"
  assert err.read_text()? == f"mv: overwrite '{dest}'? "
}

test test_mv_exchange_swaps_file_and_directory_atomically { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/file"
  let dest = fp"{root}/directory"
  source.write("payload")
  dest.mkdir()
  fp"{dest}/child".write("child")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- -T --exchange $source $dest
  assert source.is_dir()?
  assert fp"{source}/child".read_text()? == "child"
  assert dest.read_text()? == "payload"
}

test test_mv_same_entry_fails_and_retains_source { |ctx|
  let source = test.temp_file(ctx, contents: b"keep")?
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- $source $source
  assert status.exited_with(1)
  assert source.read_text()? == "keep"
}

test test_mv_dot_directory_same_file_diagnostic_preserves_operand_spelling { |ctx|
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- p"." p"."
  assert result.status.exited_with(1)
  assert result.stderr == "mv: '.' and '.' are the same file\n"
}

test test_mv_backup_preserves_source_link_named_like_backup { |ctx|
  let root = test.temp_dir(ctx)?
  let dest = fp"{root}/a"
  dest.write("old")
  let actual = fp"{root}/real"
  actual.write("new")
  let source = fp"{root}/a~"
  source.symlink(to: actual)
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- --backup=simple $source $dest
  assert dest.readlink()? == actual
  assert source.read_text()? == "old"
  assert actual.read_text()? == "new"
}

test test_mv_cross_device_file_copies_before_removing_source { |ctx|
  let root = test.temp_dir(ctx, name: "mv-cross-device")?
  let shared = p"/dev/shm"
  let shared_meta = match fs.stat(shared) {
    Ok(meta) => meta
    Err(_) => { test.skip("/dev/shm is not available"); return }
  }
  if fs.stat(root)?.dev == shared_meta.dev { test.skip("requires a second filesystem"); return }

  let destination_dir = fp"{shared}/xsh-mv-{root.name()}"
  destination_dir.mkdir()
  let destination = fp"{destination_dir}/destination"
  defer {
    destination.remove(missing_ok: true)
    destination_dir.remove_dir()
  }

  let source = fp"{root}/source"
  source.write("payload")
  source.chmod(0o6751)
  fs.set_times(source, atime_ns: 1400000000123456789, mtime_ns: 1500000000987654321)
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- $source $destination

  assert ! source.exists()?
  let moved_meta = fs.stat(destination)?
  assert destination.read_text()? == "payload"
  assert moved_meta.mode.bit_and(0o7777) == 0o6751
  assert moved_meta.atime_ns == 1400000000123456789
  assert moved_meta.mtime_ns == 1500000000987654321
}

test test_mv_cross_device_directory_preserves_links_and_fifo { |ctx|
  let root = test.temp_dir(ctx, name: "mv-cross-device-tree")?
  let shared = p"/dev/shm"
  let shared_meta = match fs.stat(shared) {
    Ok(meta) => meta
    Err(_) => { test.skip("/dev/shm is not available"); return }
  }
  if fs.stat(root)?.dev == shared_meta.dev { test.skip("requires a second filesystem"); return }

  let destination_dir = fp"{shared}/xsh-mv-tree-{root.name()}"
  destination_dir.mkdir()
  let destination = fp"{destination_dir}/moved"
  let moved_first = fp"{destination}/first"
  let moved_second = fp"{destination}/second"
  let moved_link = fp"{destination}/dangling"
  let moved_fifo = fp"{destination}/fifo"
  defer {
    moved_first.remove(missing_ok: true)
    moved_second.remove(missing_ok: true)
    moved_link.remove(missing_ok: true)
    moved_fifo.remove(missing_ok: true)
    if destination.exists()? { destination.remove_dir() }
    destination_dir.remove_dir()
  }

  let source = fp"{root}/source"
  source.mkdir()
  fp"{source}/first".write("shared payload")
  fs.link(fp"{source}/first", fp"{source}/second")
  fp"{source}/dangling".symlink(to: p"missing-target")
  fs.mknod(fp"{source}/fifo", "fifo", 0o600)
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- -v $source $destination

  assert ! source.exists()?
  assert moved_first.read_text()? == "shared payload"
  assert fs.stat(moved_first)?.ino == fs.stat(moved_second)?.ino
  assert moved_link.readlink()? == p"missing-target"
  assert fs.stat(moved_fifo)?.kind == "fifo"
  assert f"'{source}/first' -> " in output
  assert f"'{source}/dangling' -> " in output
  assert f"'{source}/fifo' -> " in output
}

test test_mv_operand_errors_include_usage { |ctx|
  let script = fp"{ctx.core_dir}/mv.xsh"
  let missing = run.capture --text ${ctx.xsh_bin} $script -- -t p"."
  assert missing.status.exited_with(1)
  assert "error: the following required arguments were not provided:" in missing.stderr
  assert "<files>..." in missing.stderr
  assert "Usage: mv [OPTION]... [-T] SOURCE DEST" in missing.stderr

  let one = run.capture --text ${ctx.xsh_bin} $script -- p"only"
  assert one.status.exited_with(1)
  assert "requires at least 2 values, but only 1 was provided" in one.stderr
}

test test_mv_backup_conflicts_with_no_clobber_and_update_none { |ctx|
  let script = fp"{ctx.core_dir}/mv.xsh"
  for flag in ["--no-clobber", "--update=none", "--update=none-fail"] {
    let result = run.capture --text ${ctx.xsh_bin} $script -- --backup $flag p"source" p"target"
    assert result.status.exited_with(1)
    assert "cannot combine --backup with -n/--no-clobber or --update=none-fail" in result.stderr
  }
}

test test_mv_slash_operand_diagnostics_name_the_unreachable_path { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let target = fp"{root}/target"
  source.write("source")
  target.write("target")

  let target_slash = fp"{target}/"
  let destination = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- $source $target_slash
  assert destination.status.exited_with(1)
  assert f"failed to access '{target}/': Not a directory" in destination.stderr
  assert source.read_text()? == "source"

  let source_slash = fp"{source}/"
  let source_error = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- $source_slash $target
  assert source_error.status.exited_with(1)
  assert f"cannot stat '{source}/': Not a directory" in source_error.stderr
}

test test_mv_progress_option_accepts_verbose_hardlink_batch { |ctx|
  let root = test.temp_dir(ctx)?
  let first = fp"{root}/first"
  first.write("shared")
  fs.link(first, fp"{root}/second")
  fp"{root}/third".write("other")
  let target = fp"{root}/target"
  target.mkdir()
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- --progress --verbose $first fp"{root}/second" fp"{root}/third" $target
  assert ! first.exists()?
  assert fp"{target}/first".read_text()? == "shared"
  assert fp"{target}/second".read_text()? == "shared"
  assert fp"{target}/third".read_text()? == "other"
  assert fs.stat(fp"{target}/first")?.ino == fs.stat(fp"{target}/second")?.ino
}

test test_mv_context_option_is_accepted_without_selinux { |ctx|
  let root = test.temp_dir(ctx)?
  let script = fp"{ctx.core_dir}/mv.xsh"
  for flag in ["-Z", "--context", "--cont"] {
    let source = fp"{root}/source"
    let dest = fp"{root}/dest"
    source.write("data")
    let result = run.capture --text ${ctx.xsh_bin} $script -- $flag $source $dest
    assert result.status.exited_with(0), f"mv {flag}: {result.stderr}"
    assert result.stdout == "" and result.stderr == ""
    assert ! source.exists()?
    assert dest.read_text()? == "data"
    dest.remove()?
  }
}

test test_mv_context_rejects_an_argument_like_gnu { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  source.write("data")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- --context=unconfined_u:object_r:user_tmp_t:s0 $source fp"{root}/dest"
  assert result.status.exited_with(1)
  assert result.stderr == "mv: option '--context' doesn't allow an argument\nTry 'mv --help' for more information.\n"
  assert source.read_text()? == "data"
}

test test_mv_backup_source_message_keeps_gnu_double_space { |ctx|
  let root = test.temp_dir(ctx)?
  let target = fp"{root}/a"
  let source = fp"{root}/a~"
  target.write("a")
  source.write("a2")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- --b=simple $source $target
  assert result.status.exited_with(1)
  assert result.stderr == f"mv: backing up '{target}' might destroy source;  '{source}' not moved\n", result.stderr
  assert source.read_text()? == "a2"
}

test test_mv_no_target_directory_renames_onto_directory_with_trailing_slash { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  source.mkdir()
  fp"{source}/inside".write("moved")
  let dest = fp"{root}/dest"
  dest.mkdir()
  fp"{dest}/old".write("old")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- -T --backup=numbered $source fp"{dest}/"
  assert ! source.exists()?
  assert fp"{dest}/inside".read_text()? == "moved"
  assert fp"{root}/dest.~1~/old".read_text()? == "old"
}

test test_mv_missing_duplicate_source_is_reported_before_overwrite_check { |ctx|
  let root = test.temp_dir(ctx)?
  let script = fp"{ctx.core_dir}/mv.xsh"
  cd root {
    p"a".write("a")
    p"d".mkdir()
    let result = run.capture --text ${ctx.xsh_bin} $script -- p"a" p"a" p"d/"
    assert result.status.exited_with(1)
    assert result.stderr == "mv: cannot stat 'a': No such file or directory\n", result.stderr
    assert p"d/a".read_text()? == "a"
  }
}

test test_mv_cross_device_unremovable_target_reports_inter_device_failure { |ctx|
  let root = test.temp_dir(ctx, name: "mv-unremovable-target")?
  let shared = p"/dev/shm"
  let shared_meta = match fs.stat(shared) {
    Ok(meta) => meta
    Err(_) => { test.skip("/dev/shm is not available"); return }
  }
  if fs.stat(root)?.dev == shared_meta.dev { test.skip("requires a second filesystem"); return }
  if applet.current_euid() == 0 { test.skip("requires an unprivileged caller"); return }

  let destination_dir = fp"{shared}/xsh-mv-locked-{root.name()}"
  destination_dir.mkdir()
  let destination = fp"{destination_dir}/k"
  defer {
    destination_dir.chmod(0o700)
    destination.remove(missing_ok: true)
    destination_dir.remove_dir()
  }
  destination.write("old")
  let source = fp"{root}/k"
  source.write("new")
  destination_dir.chmod(0o500)

  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/mv.xsh" -- -f $source $destination
  assert result.status.exited_with(1)
  assert result.stderr == f"mv: inter-device move failed: '{source}' to '{destination}'; unable to remove target: Permission denied\n", result.stderr
  assert destination.read_text()? == "old"
  assert source.read_text()? == "new"
}
