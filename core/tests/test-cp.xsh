test test_cp_file_and_recursive_dir { |ctx|
  let root = test.temp_dir(ctx, name: "cp")?
  let src = fp"{root}/src.txt"
  let dst = fp"{root}/dst.txt"
  src.write("hello")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- $src $dst
  assert dst.read_text()? == "hello"
  let dir = fp"{root}/dir"
  dir.mkdir()
  fp"{dir}/nested.txt".write("nested")
  let out = fp"{root}/out"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -R $dir $out
  assert fp"{out}/nested.txt".read_text()? == "nested"
}

test test_cp_preserve_mode_and_nanosecond_times { |ctx|
  let root = test.temp_dir(ctx, name: "cp-preserve")?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("contents")
  source.chmod(0o751)
  fs.set_times(source, atime_ns: 1400000000123456789, mtime_ns: 1500000000987654321)
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -p $source $dest
  let meta = fs.stat(dest)?
  assert meta.mode.bit_and(0o7777) == 0o751
  assert meta.atime_ns == 1400000000123456789
  assert meta.mtime_ns == 1500000000987654321
}

test test_cp_preserves_timestamps_captured_before_source_reads { |ctx|
  let root = test.temp_dir(ctx, name: "cp-atime")?
  let source_dir = fp"{root}/source-dir"
  source_dir.mkdir()
  fp"{source_dir}/file".write("contents")
  let directory_atime = 1400000000000000000
  let directory_mtime = 1500000000000000000
  fs.set_times(source_dir, atime_ns: directory_atime, mtime_ns: directory_mtime)
  let source_file = fp"{source_dir}/file"
  let file_atime = 1400000000123456789
  let file_mtime = 1500000000987654321
  fs.set_times(source_file, atime_ns: file_atime, mtime_ns: file_mtime)
  let copied_dir = fp"{root}/copied-dir"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -p -R $source_dir $copied_dir
  let copied_dir_meta = fs.stat(copied_dir)?
  assert copied_dir_meta.atime_ns == directory_atime
  assert copied_dir_meta.mtime_ns == directory_mtime
  let copied_file_meta = fs.stat(fp"{copied_dir}/file")?
  assert copied_file_meta.atime_ns == file_atime
  assert copied_file_meta.mtime_ns == file_mtime
}

test test_cp_dereference_order_and_recursive_default { |ctx|
  let root = test.temp_dir(ctx, name: "cp-deref")?
  let source = fp"{root}/source"
  source.mkdir()
  fp"{source}/file".write("contents")
  fp"{source}/link".symlink(to: p"file")
  let alias = fp"{root}/alias"
  alias.symlink(to: p"source")
  let copied = fp"{root}/copied"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -R $source $copied
  assert fs.stat(fp"{copied}/link")?.kind == "symlink"
  let followed = fp"{root}/followed"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -RPL $alias $followed
  assert fs.stat(fp"{followed}/link")?.kind == "file"
  let command = fp"{root}/command"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -RH $alias $command
  assert fs.stat(fp"{command}/link")?.kind == "symlink"
  let preserved = fp"{root}/preserved"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -RLP $alias $preserved
  assert fs.stat(preserved)?.kind == "symlink"
}

test test_cp_archive_preserves_hardlinks_and_directory_metadata { |ctx|
  let root = test.temp_dir(ctx, name: "cp-archive")?
  let source = fp"{root}/source"
  source.mkdir()
  fp"{source}/a".write("contents")
  fs.link(fp"{source}/a", fp"{source}/b")
  source.chmod(0o550)
  fs.set_times(source, mtime_ns: 1500000000123456789)
  let dest = fp"{root}/dest"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -a $source $dest
  assert fs.stat(fp"{dest}/a")?.ino == fs.stat(fp"{dest}/b")?.ino
  assert fs.stat(dest)?.mode.bit_and(0o7777) == 0o550
  assert fs.stat(dest)?.mtime_ns == 1500000000123456789
}

test test_cp_update_and_no_clobber_continue_recursing { |ctx|
  let root = test.temp_dir(ctx, name: "cp-update")?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.mkdir()
  dest.mkdir()
  fp"{source}/a".write("new")
  fp"{source}/b".write("other")
  fp"{dest}/a".write("old")
  fs.set_times(fp"{source}/a", mtime_ns: 1000000000)
  fs.set_times(fp"{dest}/a", mtime_ns: 2000000000)
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -RuT $source $dest
  assert fp"{dest}/a".read_text()? == "old"
  assert fp"{dest}/b".read_text()? == "other"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --update=none --update=all -T fp"{source}/a" fp"{dest}/a"
  assert fp"{dest}/a".read_text()? == "new"
}

test test_cp_force_keeps_existing_inode_and_remove_destination_replaces_it { |ctx|
  let root = test.temp_dir(ctx, name: "cp-inodes")?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  let alias = fp"{root}/alias"
  source.write("new")
  dest.write("old")
  fs.link(dest, alias)
  let before = fs.stat(dest)?.ino
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -f $source $dest
  assert fs.stat(dest)?.ino == before
  assert alias.read_text()? == "new"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --remove-destination $source $dest
  assert fs.stat(dest)?.ino != fs.stat(alias)?.ino
}

test test_cp_same_file_fails_without_truncating_and_other_sources_continue { |ctx|
  let root = test.temp_dir(ctx, name: "cp-errors")?
  let source = fp"{root}/source"
  source.write("safe")
  let same = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- $source $source
  assert same.status.exited_with(1)
  assert source.read_text()? == "safe"
  assert same.stderr.find("are the same file") != null
  let dest = fp"{root}/dest"
  dest.mkdir()
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- fp"{root}/missing" $source $dest
  assert result.status.exited_with(1)
  assert fp"{dest}/source".read_text()? == "safe"
}

test test_cp_self_descendant_fails_and_symlink_cycles_are_bounded { |ctx|
  let root = test.temp_dir(ctx, name: "cp-cycle")?
  let source = fp"{root}/source"
  source.mkdir()
  fp"{source}/file".write("safe")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -R $source fp"{source}/nested"
  assert result.status.exited_with(1)
  assert ! fp"{source}/nested".exists()?
  fp"{source}/loop".symlink(to: p".")
  let cycle = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -RL $source fp"{root}/out"
  assert cycle.status.exited_with(1)
  assert fp"{root}/out/file".read_text()? == "safe"
}

test test_cp_backups_numbered_and_source_protection { |ctx|
  let root = test.temp_dir(ctx, name: "cp-backup")?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("new")
  dest.write("old")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --backup=numbered $source $dest
  assert fp"{dest}.~1~".read_text()? == "old"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --backup=existing $source $dest
  assert fp"{dest}.~2~".read_text()? == "new"
  let backup_source = fp"{dest}~"
  backup_source.write("protected")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --backup=simple $backup_source $dest
  assert result.status.exited_with(1)
  assert backup_source.read_text()? == "protected"
}

test test_cp_dangling_symlink_policy_and_no_dereference { |ctx|
  let root = test.temp_dir(ctx, name: "cp-dangling")?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("contents")
  dest.symlink(to: p"missing")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- $source $dest
  assert result.status.exited_with(1)
  assert ! fp"{root}/missing".exists()?
  let link = fp"{root}/link"
  link.symlink(to: p"absent")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -P $link $dest
  assert dest.readlink()? == p"absent"
}

test test_cp_attributes_only_retains_contents_and_special_type { |ctx|
  let root = test.temp_dir(ctx, name: "cp-attributes")?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("new")
  source.chmod(0o740)
  dest.write("old")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --attributes-only --preserve=mode $source $dest
  assert dest.read_text()? == "old"
  assert fs.stat(dest)?.mode.bit_and(0o777) == 0o740
  let fifo = fp"{root}/fifo"
  fs.mknod(fifo, "fifo", 0o600)
  let copied = fp"{root}/fifo-copy"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -a --attributes-only $fifo $copied
  assert fs.stat(copied)?.kind == "fifo"
}

test test_cp_interactive_decline_and_accept { |ctx|
  let root = test.temp_dir(ctx, name: "cp-interactive")?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("new")
  dest.write("old")
  let no = fp"{root}/no"
  no.write("n\n")
  let declined = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -i $source $dest < $no
  assert declined.status.exited_with(1)
  assert dest.read_text()? == "old"
  assert declined.stderr == f"cp: overwrite '{dest}'? "
  let yes = fp"{root}/yes"
  yes.write("y\n")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -i $source $dest < $yes
  assert dest.read_text()? == "new"
}

test test_cp_interactive_eof_and_verbose_decline_are_failures { |ctx|
  let root = test.temp_dir(ctx, name: "cp-interactive-eof")?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("new")
  dest.write("old")
  let empty = fp"{root}/empty"
  empty.write("")
  let eof = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -i $source $dest < $empty
  assert eof.status.exited_with(1)
  assert eof.stderr == f"cp: overwrite '{dest}'? "
  assert dest.read_text()? == "old"

  let no = fp"{root}/no"
  no.write("n\n")
  let verbose = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -v -i $source $dest < $no
  assert verbose.status.exited_with(1)
  assert verbose.stdout == ""
  assert verbose.stderr == f"cp: overwrite '{dest}'? "
  assert dest.read_text()? == "old"
}

test test_cp_update_interactive_decline_is_failure { |ctx|
  let root = test.temp_dir(ctx, name: "cp-update-interactive")?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("new")
  dest.write("old")
  fs.set_times(source, mtime_ns: 2000000000)
  fs.set_times(dest, mtime_ns: 1000000000)
  let no = fp"{root}/no"
  no.write("n\n")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -i -u $source $dest < $no
  assert result.status.exited_with(1)
  assert result.stderr == f"cp: overwrite '{dest}'? "
  assert dest.read_text()? == "old"
}

test test_cp_recursive_interactive_decline_continues { |ctx|
  let root = test.temp_dir(ctx, name: "cp-interactive-recursive")?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.mkdir()
  fp"{dest}/source".mkdir(parents: true)
  fp"{source}/existing".write("new")
  fp"{source}/other".write("copied")
  fp"{dest}/source/existing".write("old")
  let no = fp"{root}/no"
  no.write("n\n")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -R -i $source $dest < $no
  assert result.status.exited_with(1)
  assert result.stderr.find("overwrite") != null
  assert fp"{dest}/source/existing".read_text()? == "old"
  assert fp"{dest}/source/other".read_text()? == "copied"
}

test test_cp_recursive_verbose_reports_directory_separator_and_replacements { |ctx|
  let root = test.temp_dir(ctx, name: "cp-verbose-recursive")?
  let source = fp"{root}/source"
  source.mkdir()
  fp"{source}/file".write("contents")
  let dest = fp"{root}/dest"
  let copied = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -R -v $source $dest
  assert copied.status.ok
  assert copied.stdout == f"'{source}' -> '{dest}/'\n'{source}/file' -> '{dest}/file'\n"

  let target = fp"{root}/target"
  target.mkdir()
  let link_source = fp"{root}/link-source"
  link_source.mkdir()
  let original = fp"{root}/original"
  original.write("contents")
  fp"{link_source}/link".symlink(to: p"original")
  fp"{target}/link".symlink(to: p"old")
  let replaced = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -R -v -T $link_source $target
  assert replaced.status.ok
  assert replaced.stdout == f"removed '{target}/link'\n'{link_source}/link' -> '{target}/link'\n"
}

test test_cp_force_interactive_replaces_unwritable_destination { |ctx|
  let root = test.temp_dir(ctx, name: "cp-force-interactive")?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("new")
  dest.write("old")
  dest.chmod(0o000)
  let yes = fp"{root}/yes"
  yes.write("y")
  let accepted = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -f -i -v $source $dest < $yes
  assert accepted.status.ok
  assert accepted.stderr == f"cp: replace '{dest}', overriding mode 0000 (---------)? "
  assert accepted.stdout == f"removed '{dest}'\n'{source}' -> '{dest}'\n"
  assert dest.read_text()? == "new"

  let declined_dest = fp"{root}/declined"
  declined_dest.write("old")
  declined_dest.chmod(0o000)
  let empty = fp"{root}/empty"
  empty.write("")
  let declined = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -f -i -v $source $declined_dest < $empty
  assert declined.status.exited_with(1)
  assert declined.stderr == f"cp: replace '{declined_dest}', overriding mode 0000 (---------)? "
  declined_dest.chmod(0o600)
  assert declined_dest.read_text()? == "old"
}

test test_cp_refuses_overwriting_just_created_destination { |ctx|
  let root = test.temp_dir(ctx, name: "cp-created")?
  let a = fp"{root}/a"
  let b = fp"{root}/b"
  let dest = fp"{root}/dest"
  a.mkdir()
  b.mkdir()
  dest.mkdir()
  fp"{a}/file".write("first")
  fp"{b}/file".write("second")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- fp"{a}/file" fp"{b}/file" $dest
  assert result.status.exited_with(1)
  assert result.stderr.find("will not overwrite just-created") != null
  assert fp"{dest}/file".read_text()? == "first"
}

test test_cp_recursive_refuses_copying_through_just_created_symlink { |ctx|
  let root = test.temp_dir(ctx, name: "cp-created-link")?
  let a = fp"{root}/a"
  let b = fp"{root}/b"
  let dest = fp"{root}/dest"
  a.mkdir()
  b.mkdir()
  dest.mkdir()
  let protected = fp"{root}/protected"
  protected.write("protected")
  fp"{a}/file".symlink(to: protected)
  fp"{b}/file".write("overwrite")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -R fp"{a}/." fp"{b}/." $dest
  assert result.status.exited_with(1)
  assert result.stderr.find("through just-created symlink") != null
  assert protected.read_text()? == "protected"
}

test test_cp_duplicate_sources_warn_but_succeed { |ctx|
  let root = test.temp_dir(ctx, name: "cp-duplicate")?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("contents")
  dest.mkdir()
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- $source $source $dest
  assert result.status.ok
  assert result.stderr.find("specified more than once") != null
  assert fp"{dest}/source".read_text()? == "contents"
}

test test_cp_option_order_and_equals_validation { |ctx|
  let root = test.temp_dir(ctx, name: "cp-argv")?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("new")
  dest.write("old")
  let input = fp"{root}/input"
  input.write("y\n")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -n -i $source $dest < $input
  assert dest.read_text()? == "new"
  source.write("changed")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -i -n $source $dest < $input
  assert dest.read_text()? == "new"
  let bad = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --recursive=no $source $dest
  assert bad.status.exited_with(1)
  assert bad.stderr.find("does not allow an argument") != null
}

test test_cp_explicit_no_preserve_mode_and_later_preserve { |ctx|
  let root = test.temp_dir(ctx, name: "cp-no-mode")?
  let source = fp"{root}/source"
  source.write("contents")
  source.chmod(0o751)
  let stripped = fp"{root}/stripped"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -a --no-preserve=mode --preserve=timestamp $source $stripped
  assert fs.stat(stripped)?.mode.bit_and(0o7777) == 0o666.clear_bits(fs.umask()?)
  let preserved = fp"{root}/preserved"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --no-preserve=all --preserve=mode $source $preserved
  assert fs.stat(preserved)?.mode.bit_and(0o7777) == 0o751
}

test test_cp_new_directory_drops_setid_but_keeps_sticky { |ctx|
  let root = test.temp_dir(ctx, name: "cp-dir-mode")?
  let source = fp"{root}/source"
  source.mkdir()
  source.chmod(0o7711)
  let dest = fp"{root}/dest"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -R $source $dest
  assert fs.stat(dest)?.mode.bit_and(0o7777) == 0o1711.clear_bits(fs.umask()?)
}

test test_cp_no_preserve_mode_new_directory_keeps_inherited_setgid { |ctx|
  let root = test.temp_dir(ctx, name: "cp-no-mode-setgid")?
  let parent = fp"{root}/parent"
  parent.mkdir()
  parent.chmod(0o2777)
  let source = fp"{root}/source"
  source.mkdir()
  source.chmod(0o705)
  let dest = fp"{parent}/dest"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --no-preserve=mode -r $source $dest
  assert fs.stat(dest)?.mode.bit_and(0o7777) == 0o2777.clear_bits(fs.umask()?)
}

test test_cp_suffix_alone_enables_simple_backups { |ctx|
  let root = test.temp_dir(ctx, name: "cp-suffix-alone")?
  let source = fp"{root}/source"
  source.write("new\n")
  let dest = fp"{root}/dest"
  dest.write("old\n")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --suffix .bak $source $dest
  assert dest.read_text()? == "new\n"
  assert fp"{dest}.bak".read_text()? == "old\n"
}

test test_cp_backup_protects_source_with_two_spaces_in_message { |ctx|
  let root = test.temp_dir(ctx, name: "cp-protect-source")?
  let source = fp"{root}/file~"
  source.write("")
  let dest = fp"{root}/file"
  dest.write("kept\n")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --backup=simple $source $dest
  assert result.status.exited_with(1)
  assert result.stderr == f"cp: backing up '{dest}' might destroy source;  '{source}' not copied\n", result.stderr
  assert dest.read_text()? == "kept\n"
}

test test_cp_backup_rejects_update_none_modes { |ctx|
  let root = test.temp_dir(ctx, name: "cp-backup-update")?
  let source = fp"{root}/source"
  source.write("a")
  let dest = fp"{root}/dest"
  dest.write("b")
  for update in ["--update=none", "--update=none-fail"] {
    let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -b $update $source $dest
    assert result.status.exited_with(1)
    assert result.stderr.find("--backup is mutually exclusive with -n or --update=none-fail") != null, result.stderr
  }
  assert dest.read_text()? == "b"
}

test test_cp_parents_refuses_dot_dot_destination { |ctx|
  let root = test.temp_dir(ctx, name: "cp-parents-dotdot")?
  fp"{root}/src/sub".mkdir(parents: true)
  fp"{root}/src/sub/f".write("x\n")
  fp"{root}/d".mkdir()
  cd $root {
    let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --parents -r src/sub/.. d
    assert result.status.exited_with(1)
    assert result.stderr.find("cp: cannot create directory 'd/src/sub/..': File exists") != null, result.stderr
  }
  assert ! fp"{root}/d/src/sub/f".exists()?
}

test test_cp_special_file_creation_failure_names_the_file { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("mkfifo fixture is Linux-specific") }
  if user.current()?.uid == 0 { test.skip("root can create special files in a read-only directory") }
  let root = test.temp_dir(ctx, name: "cp-special-create")?
  let source = fp"{root}/fifo"
  fs.mknod(source, "fifo", 0o644)
  let locked = fp"{root}/locked"
  locked.mkdir()
  locked.chmod(0o555)
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -R $source fp"{locked}/copy"
  locked.chmod(0o755)
  assert result.status.exited_with(1)
  assert result.stderr.find(f"cp: cannot create special file '{locked}/copy': ") != null, result.stderr
}

test test_cp_undecodable_operands_reach_the_kernel_unchanged { |ctx|
  let root = test.temp_dir(ctx, name: "cp-raw-operands")?
  let source = Path.parse_bytes(bytes.concat([root.bytes(), b"/raw-\xff"]))?
  source.write("raw\n")
  let dest = Path.parse_bytes(bytes.concat([root.bytes(), b"/out-\xfe"]))?
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- $source $dest
  assert dest.read_bytes()? == b"raw\n"

  let dir = Path.parse_bytes(bytes.concat([root.bytes(), b"/dir-\xfd"]))?
  dir.mkdir()
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -t $dir $source
  let copied = Path.parse_bytes(bytes.concat([dir.bytes(), b"/raw-\xff"]))?
  assert copied.read_bytes()? == b"raw\n"
}

test test_cp_force_backup_same_named_source { |ctx|
  let root = test.temp_dir(ctx, name: "cp-self-backup")?
  let source = fp"{root}/source"
  source.write("contents")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -fb $source $source
  assert source.read_text()? == "contents"
  assert fp"{source}~".read_text()? == "contents"
  assert fs.stat(source)?.ino != fs.stat(fp"{source}~")?.ino
}

test test_cp_parents_preserves_new_parent_modes { |ctx|
  let root = test.temp_dir(ctx, name: "cp-parents")?
  let source = fp"{root}/source"
  source.mkdir()
  let inner = fp"{source}/inner"
  inner.mkdir()
  fp"{inner}/file".write("contents")
  inner.chmod(0o711)
  source.chmod(0o750)
  let dest = fp"{root}/dest"
  dest.mkdir()
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -p --parents fp"{inner}/file" $dest
  let relative = source.display().byte_slice(1)
  assert fp"{dest}/{relative}/inner/file".read_text()? == "contents"
  assert fs.stat(fp"{dest}/{relative}")?.mode.bit_and(0o7777) == 0o750
  assert fs.stat(fp"{dest}/{relative}/inner")?.mode.bit_and(0o7777) == 0o711
}

test test_cp_parents_preserves_existing_parent_modes { |ctx|
  let root = test.temp_dir(ctx, name: "cp-parents-existing")?
  let source = fp"{root}/source"
  let p1 = fp"{source}/p1"
  let p2 = fp"{p1}/p2"
  let first = fp"{p2}/first/file"
  let second = fp"{p2}/second/file"
  first.parent().mkdir(parents: true)
  second.parent().mkdir(parents: true)
  first.write("first")
  second.write("second")
  p1.chmod(0o755)
  p2.chmod(0o711)
  let dest = fp"{root}/dest"
  dest.mkdir()
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -p --parents $first $dest
  let relative = source.display().byte_slice(1)
  let copied_p1 = fp"{dest}/{relative}/p1"
  let copied_p2 = fp"{copied_p1}/p2"
  copied_p1.chmod(0o700)
  copied_p2.chmod(0o700)
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -p --parents $second $dest
  assert fs.stat(copied_p1)?.mode.bit_and(0o7777) == 0o755
  assert fs.stat(copied_p2)?.mode.bit_and(0o7777) == 0o711
}

test test_cp_xattr_preservation_is_selected_and_binary { |ctx|
  let root = test.temp_dir(ctx, name: "cp-xattr")?
  let source = fp"{root}/source"
  source.write("contents")
  let payload = b"\0value\xff"
  fs.xattr_set(source, "user.cp_test", payload)
  let archived = fp"{root}/archive"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -a $source $archived
  assert fs.xattr_get(archived, "user.cp_test")? == payload
  let plain = fp"{root}/plain"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -p $source $plain
  assert "user.cp_test" not in fs.xattr_list(plain)?
  let explicit = fp"{root}/explicit"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --preserve=xattr $source $explicit
  assert fs.xattr_get(explicit, "user.cp_test")? == payload
  source.chmod(0o444)
  let readonly = fp"{root}/readonly"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --preserve=xattr $source $readonly
  assert fs.xattr_get(readonly, "user.cp_test")? == payload
  assert fs.stat(readonly)?.mode.bit_and(0o777) == 0o444
  let omitted = fp"{root}/omitted"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -a --no-preserve=xattr $source $omitted
  assert "user.cp_test" not in fs.xattr_list(omitted)?
}

test test_cp_preserve_mode_copies_posix_acl_and_removes_stale_acl { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("POSIX ACL xattr encoding is Linux-specific") }
  let root = test.temp_dir(ctx, name: "cp-acl")?
  let source = fp"{root}/source"
  source.write("contents")
  let acl = bytes.from_ints([
    2,0,0,0,
    1,0,6,0,255,255,255,255,
    2,0,4,0,57,48,0,0,
    4,0,0,0,255,255,255,255,
    16,0,4,0,255,255,255,255,
    32,0,0,0,255,255,255,255,
  ])?
  fs.xattr_set(source, "system.posix_acl_access", acl)
  let dest = fp"{root}/dest"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -p $source $dest
  assert fs.xattr_get(dest, "system.posix_acl_access")? == acl
  let plain = fp"{root}/plain"
  plain.write("plain")
  plain.chmod(0o600)
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -p $plain $dest
  assert "system.posix_acl_access" not in fs.xattr_list(dest)?
  assert fs.stat(dest)?.mode.bit_and(0o7777) == 0o600
}

test test_cp_explicit_missing_security_context_fails { |ctx|
  let root = test.temp_dir(ctx, name: "cp-context")?
  let source = fp"{root}/source"
  source.write("contents")
  if "security.selinux" in fs.xattr_list(source)? { test.skip("source has a security label") }
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --preserve=context $source fp"{root}/dest"
  assert result.status.exited_with(1)
  assert result.stderr.find("failed to get security context") != null
}

test test_cp_recursive_preserves_non_utf8_child_paths { |ctx|
  let root = test.temp_dir(ctx, name: "cp-path-bytes")?
  let source = fp"{root}/source"
  source.mkdir()
  let name = b"file\xff" as Path
  fp"{source}/{name}".write("contents")
  let dest = fp"{root}/dest"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -R $source $dest
  assert fp"{dest}/{name}".read_text()? == "contents"
}

test test_cp_same_file_aliases_preserve_source_under_replace_modes { |ctx|
  let root = test.temp_dir(ctx, name: "cp-same-alias")?
  let source = fp"{root}/source"
  source.write("contents")
  let link = fp"{root}/link"
  link.symlink(to: p"source")
  let refused = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -d $source $link
  assert refused.status.exited_with(1)
  assert source.read_text()? == "contents"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -b $source $link
  assert fs.stat(link)?.kind == "file"
  assert fp"{link}~".readlink()? == p"source"
  let hard = fp"{root}/hard"
  fs.link(source, hard)
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --remove-destination $source $hard
  assert fs.stat(hard)?.ino != fs.stat(source)?.ino
  let alias = fp"{root}/alias"
  alias.symlink(to: p"source")
  let danger = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --remove-destination $alias $source
  assert danger.status.exited_with(1)
  assert source.read_text()? == "contents"
}

test test_cp_hardlink_source_dereference_and_existing_destination { |ctx|
  let root = test.temp_dir(ctx, name: "cp-link-deref")?
  let source = fp"{root}/source"
  source.write("contents")
  let link = fp"{root}/link"
  link.symlink(to: p"source")
  let followed = fp"{root}/followed"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -Rl $link $followed
  assert fs.stat(followed)?.ino == fs.stat(source)?.ino
  let preserved = fp"{root}/preserved"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -Pl $link $preserved
  assert fs.stat(preserved)?.ino == fs.stat(link)?.ino
  let dest = fp"{root}/dest"
  dest.write("old")
  let refused = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -l $source $dest
  assert refused.status.exited_with(1)
  assert dest.read_text()? == "old"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -fl $source $dest
  assert fs.stat(dest)?.ino == fs.stat(source)?.ino
}

test test_cp_virtual_zero_size_files_are_copied_to_eof { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("procfs is Linux-specific") }
  let root = test.temp_dir(ctx, name: "cp-virtual")?
  let source = /proc/version
  assert fs.stat(source)?.size == 0
  let dest = fp"{root}/dest"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- $source $dest
  assert dest.read_text()?.starts_with("Linux version ")
}

test test_cp_special_destination_receives_bytes_and_reports_full { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("device fixtures are Linux-specific") }
  let root = test.temp_dir(ctx, name: "cp-device")?
  let source = fp"{root}/source"
  source.write("contents")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- $source /dev/null
  let full = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- $source /dev/full
  assert full.status.exited_with(1)
  assert full.stderr.find("No space left on device") != null
}

test test_cp_current_directory_can_be_copied_to_sibling { |ctx|
  let root = test.temp_dir(ctx, name: "cp-cwd")?
  let source = fp"{root}/source"
  source.mkdir()
  fp"{source}/file".write("contents")
  let dest = fp"{root}/dest"
  cd $source {
    run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -R . ../dest/
  }
  assert fp"{dest}/file".read_text()? == "contents"
}

test test_cp_attributes_only_same_inode_fails_without_changes { |ctx|
  let root = test.temp_dir(ctx, name: "cp-attributes-self")?
  let source = fp"{root}/source"
  source.write("contents")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --attributes-only $source $source
  assert result.status.exited_with(1)
  assert source.read_text()? == "contents"
  let link = fp"{root}/link"
  link.symlink(to: p"source")
  let dest = fp"{root}/dest"
  dest.write("old")
  let blocked = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -P --attributes-only $link $dest
  assert blocked.status.exited_with(1)
  assert dest.read_text()? == "old"
}

test test_cp_force_replaces_destination_symlink_loop { |ctx|
  let root = test.temp_dir(ctx, name: "cp-loop-dest")?
  let source = fp"{root}/source"
  source.write("contents")
  let dest = fp"{root}/loop"
  dest.symlink(to: p"loop")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -f $source $dest
  assert fs.stat(dest)?.kind == "file"
  assert dest.read_text()? == "contents"
}

test test_cp_verbose_stdout_failure_is_reported { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("/dev/full is a Linux fixture") }
  let root = test.temp_dir(ctx, name: "cp-verbose-failure")?
  let source = fp"{root}/source"
  source.write("contents")
  # Bind the child's stdout descriptor to the device before executing the
  # applet, so this observes kernel write errors directly.
  let fixture = "exec \"$@\" > /dev/full"
  let result = run.capture --text sh -c $fixture sh ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -v $source fp"{root}/dest"
  assert result.status.exited_with(1), result.stderr
  assert result.stderr.find("write error") != null
}

test test_cp_invalid_destination_parent_reports_creation_error { |ctx|
  let root = test.temp_dir(ctx, name: "cp-not-dir")?
  let source = fp"{root}/source"
  let parent = fp"{root}/parent"
  source.write("contents")
  parent.write("parent contents")
  let dest = fp"{parent}/child"
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- $source $dest
  assert result.status.exited_with(1)
  assert result.stderr == f"cp: cannot create regular file '{dest}': Not a directory\n"
  assert parent.read_text()? == "parent contents"
}

test test_cp_preserve_through_destination_symlink_changes_referent { |ctx|
  let root = test.temp_dir(ctx, name: "cp-preserve-link")?
  let source = fp"{root}/source"
  source.write("new")
  source.chmod(0o640)
  fs.set_times(source, mtime_ns: 1500000000123456789)
  let referent = fp"{root}/referent"
  referent.write("old")
  referent.chmod(0o600)
  let dest = fp"{root}/dest"
  dest.symlink(to: p"referent")
  let link_time = fs.stat(dest)?.mtime_ns
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -p $source $dest
  assert referent.read_text()? == "new"
  assert fs.stat(referent)?.mode.bit_and(0o7777) == 0o640
  assert fs.stat(referent)?.mtime_ns == 1500000000123456789
  assert dest.readlink()? == p"referent"
  assert fs.stat(dest)?.mtime_ns == link_time
}

test test_cp_ownership_only_finishes_new_file_and_directory_modes { |ctx|
  let root = test.temp_dir(ctx, name: "cp-owner-mode")?
  let source = fp"{root}/source"
  source.mkdir()
  source.chmod(0o751)
  let file = fp"{source}/file"
  file.write("contents")
  file.chmod(0o740)
  let dest = fp"{root}/dest"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -R --preserve=ownership $source $dest
  assert fs.stat(dest)?.mode.bit_and(0o7777) == 0o751.clear_bits(fs.umask()?)
  assert fs.stat(fp"{dest}/file")?.mode.bit_and(0o7777) == 0o740.clear_bits(fs.umask()?)
  assert fp"{dest}/file".read_text()? == "contents"
}

test test_cp_does_not_reuse_destination_symlink_for_second_source { |ctx|
  let root = test.temp_dir(ctx, name: "cp-used-link")?
  let a = fp"{root}/a"
  let b = fp"{root}/b"
  let dest = fp"{root}/dest"
  a.mkdir()
  b.mkdir()
  dest.mkdir()
  fp"{a}/file".write("first")
  fp"{b}/file".write("second")
  let referent = fp"{root}/referent"
  referent.write("old")
  fp"{dest}/file".symlink(to: referent)
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- fp"{a}/file" fp"{b}/file" $dest
  assert result.status.exited_with(1)
  assert result.stderr.find("through just-created symlink") != null
  assert referent.read_text()? == "first"
}

test test_cp_force_unreadable_source_keeps_destination { |ctx|
  let privilege = if unix.id()?.uid == 0 { "unix.set_uid(65534)\n" } else { "" }
  let root = test.temp_dir(ctx, name: "cp-force-source")?
  root.parent().chmod(0o755)
  root.chmod(0o777)
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("unreadable")
  source.chmod(0o000)
  dest.write("protected")
  let helper = fp"{root}/helper.xsh"
  helper.write(f"""{privilege}let status = run.status p"{ctx.xsh_bin}" p"{ctx.core_dir}/cp.xsh" -- -f p"{source}" p"{dest}"
exit status.shell_code()?
""")
  helper.chmod(0o644)
  let result = run.capture --text ${ctx.xsh_bin} $helper
  assert result.status.exited_with(1), result.stderr
  assert dest.read_text()? == "protected"
}

test test_cp_force_readonly_destination_restores_creation_mode { |ctx|
  if unix.id()?.uid == 0 { test.skip("root can write a read-only destination") }
  let root = test.temp_dir(ctx, name: "cp-force-mode")?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("new")
  source.chmod(0o740)
  dest.write("old")
  dest.chmod(0o400)
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -f --preserve=ownership $source $dest
  assert dest.read_text()? == "new"
  assert fs.stat(dest)?.mode.bit_and(0o7777) == 0o740.clear_bits(fs.umask()?)
}

test test_cp_debug_reports_copy_policy_statuses { |ctx|
  let root = test.temp_dir(ctx, name: "cp-debug-status")?
  let source = fp"{root}/source"
  source.write("")
  let automatic = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --debug --reflink=auto $source fp"{root}/automatic"
  assert automatic.status.ok
  assert automatic.stdout.find("copy offload: unknown, reflink: unsupported, sparse detection: no") != null

  let sparse = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --debug --sparse=always --reflink=never $source fp"{root}/sparse"
  assert sparse.status.ok
  assert sparse.stdout.find("copy offload: avoided, reflink: no, sparse detection: zeros") != null

  let never = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --debug --sparse=never $source fp"{root}/never"
  assert never.status.ok
  assert never.stdout.find("copy offload: avoided, reflink: no, sparse detection: no") != null

  let small_source = fp"{root}/small"
  small_source.write("data")
  let small = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --debug $small_source fp"{root}/small-copy"
  assert small.status.ok
  assert small.stdout.find("copy offload: yes, reflink: unsupported, sparse detection: no") != null

  let holes_source = fp"{root}/holes"
  holes_source.write("")
  holes_source.truncate(4096)
  let holes = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- --debug --reflink=never --sparse=never $holes_source fp"{root}/holes-copy"
  assert holes.status.ok
  assert holes.stdout.find("copy offload: unknown, reflink: no, sparse detection: SEEK_HOLE") != null
}

# Shared fixture: a source directory with one file and one subdirectory, plus an
# existing directory that the destination symlink points at.
proc keep_link_fixture(ctx: TestContext, name: Str) [fs, env, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name:)?
  let source = fp"{root}/origin/src"
  source.parent().mkdir()
  source.mkdir()
  fp"{source}/top".write("top\n")
  fp"{source}/sub".mkdir()
  fp"{source}/sub/f".write("f\n")
  let elsewhere = fp"{root}/elsewhere"
  elsewhere.mkdir()
  fp"{elsewhere}/pre".write("pre\n")
  let dest = fp"{root}/dest"
  dest.mkdir()
  fp"{dest}/src".symlink(to: elsewhere)
  Ok(root)
}

test test_cp_keep_directory_symlink_with_copy_contents_merges_into_linked_directory { |ctx|
  let root = keep_link_fixture(ctx, "cp-keep-link")?
  let source = fp"{root}/origin/src"
  let dest = fp"{root}/dest"
  let elsewhere = fp"{root}/elsewhere"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -r --copy-contents --keep-directory-symlink $source $dest
  assert fs.stat(fp"{dest}/src")?.kind == "symlink"
  assert fp"{elsewhere}/top".read_text()? == "top\n"
  assert fp"{elsewhere}/sub/f".read_text()? == "f\n"
  assert fp"{elsewhere}/pre".read_text()? == "pre\n"
}

test test_cp_keep_directory_symlink_archive_sets_linked_directory_mode { |ctx|
  let root = keep_link_fixture(ctx, "cp-keep-link-archive")?
  let source = fp"{root}/origin/src"
  let elsewhere = fp"{root}/elsewhere"
  source.chmod(0o700)
  elsewhere.chmod(0o750)
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -a --copy-contents --keep-directory-symlink $source fp"{root}/dest"
  assert fs.stat(elsewhere)?.mode.bit_and(0o7777) == 0o700
  assert fp"{elsewhere}/sub/f".read_text()? == "f\n"
}

test test_cp_keep_directory_symlink_alone_still_rejects_directory_over_link { |ctx|
  let root = keep_link_fixture(ctx, "cp-keep-link-alone")?
  let source = fp"{root}/origin/src"
  let dest = fp"{root}/dest"
  let elsewhere = fp"{root}/elsewhere"
  for flags in [["-r", "--keep-directory-symlink"], ["-r", "--copy-contents"]] {
    let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- @flags $source $dest
    assert result.status.exited_with(1)
    assert result.stderr.find("cannot overwrite non-directory") != null, result.stderr
    assert ! fp"{elsewhere}/top".exists()?
  }
}

test test_cp_keep_directory_symlink_dangling_link_reports_existing_file { |ctx|
  let root = keep_link_fixture(ctx, "cp-keep-link-dangling")?
  let source = fp"{root}/origin/src"
  let dest = fp"{root}/dest"
  fp"{dest}/src".remove()
  fp"{dest}/src".symlink(to: fp"{root}/missing")
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -r --copy-contents --keep-directory-symlink $source $dest
  assert result.status.exited_with(1)
  assert result.stderr.find("cannot create directory") != null, result.stderr
  assert result.stderr.find("File exists") != null, result.stderr
  assert fs.stat(fp"{dest}/src")?.kind == "symlink"
  assert ! fp"{root}/missing".exists()?
}

test test_cp_keep_directory_symlink_accepts_unique_abbreviation { |ctx|
  let root = keep_link_fixture(ctx, "cp-keep-link-abbrev")?
  let source = fp"{root}/origin/src"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/cp.xsh" -- -r --copy-contents --keep $source fp"{root}/dest"
  assert fp"{root}/elsewhere/top".read_text()? == "top\n"
}
