test test_failed_host_operations_carry_errno { |ctx|
  let root = test.temp_dir(ctx, name: "fs-errno")?
  let missing = fp"{root}/missing"
  let source = fp"{root}/source"
  source.write("x")

  let read = missing.read_text()
  assert read is Err(is NotFound)
  if let Err(failure) = read {
    assert failure.errno == 2
  }

  let linked = fs.link(source, source)
  assert linked is Err(_)
  if let Err(failure) = linked {
    assert failure.errno == 17
  }

  let fifo = fs.mkfifo(fp"{root}/no-such-dir/fifo", 0o600)
  assert fifo is Err(_)
  if let Err(failure) = fifo {
    assert failure.errno == 2
  }

  let refused = source.copy(to: source)
  assert refused is Err(_)
  if let Err(failure) = refused {
    assert failure.errno == null
  }
}

test test_stat_reports_every_lstat_field { |ctx|
  let root = test.temp_dir(ctx, name: "fs-stat")?
  let file = fp"{root}/file"
  file.write("hello")
  let st = fs.stat(file)?
  assert st.kind == "file"
  assert st.size == 5
  assert st.nlink == 1
  assert st.mode.bit_and(0o170000) == 0o100000
  assert st.mode.bit_and(0o7777) == 0o666.clear_bits(fs.umask()?)
  assert st.uid == applet.current_euid()
  assert st.blksize > 0
  assert st.ino > 0
  assert st.rdev == 0
  assert st.mtime_ns > 1600000000000000000
  assert st.ctime_ns >= st.mtime_ns
  assert (st.birth_ns ?? st.mtime_ns) > 0

  assert fs.stat(root)?.kind == "dir"
  assert fs.stat(/dev/null)?.kind == "char"
}

test test_stat_resolves_relative_paths_against_the_evaluator_directory { |ctx|
  let root = test.temp_dir(ctx, name: "fs-stat-relative")?
  fp"{root}/inner".mkdir()
  fp"{root}/inner/file".write("abc")
  fp"{root}/file".write("a")
  cd root {
    assert fs.stat(p"file")?.size == 1
    assert fs.stat(p"inner/file")?.size == 3
    cd (fp"{root}/inner") {
      assert fs.stat(p"file")?.size == 3
      assert fs.stat(p"../file")?.size == 1
    }
    assert fs.stat(p"file")?.size == 1
  }
  assert fs.stat(p"missing-relative-operand") is Err(_)
}

test test_stat_follows_symlinks_only_when_asked { |ctx|
  let root = test.temp_dir(ctx, name: "fs-stat-link")?
  let file = fp"{root}/file"
  let link = fp"{root}/link"
  let dangling = fp"{root}/dangling"
  file.write("hello")
  link.symlink(to: p"file")
  dangling.symlink(to: p"nowhere")

  let lstat = fs.stat(link)?
  assert lstat.kind == "symlink"
  assert lstat.size == 4
  assert lstat.ino != fs.stat(file)?.ino

  let followed = fs.stat(link, follow_symlinks: true)?
  assert followed.kind == "file"
  assert followed.ino == fs.stat(file)?.ino

  assert fs.stat(dangling)?.kind == "symlink"
  let broken = fs.stat(dangling, follow_symlinks: true)
  assert broken is Err(is NotFound)
  if let Err(failure) = broken {
    assert failure.errno == 2
  }
}

test test_stat_names_fifo_socket_and_hard_link_identity { |ctx|
  let root = test.temp_dir(ctx, name: "fs-stat-kinds")?
  let file = fp"{root}/file"
  let hard = fp"{root}/hard"
  file.write("x")
  fs.link(file, hard)
  let first = fs.stat(file)?
  let second = fs.stat(hard)?
  assert first.dev == second.dev
  assert first.ino == second.ino
  assert first.nlink == 2
  assert second.nlink == 2

  fs.mkfifo(fp"{root}/fifo", 0o600)
  assert fs.stat(fp"{root}/fifo")?.kind == "fifo"
  fs.mknod(fp"{root}/socket", "socket", 0o600)
  assert fs.stat(fp"{root}/socket")?.kind == "socket"
}

test test_set_times_sets_nanosecond_values_and_omits_the_rest { |ctx|
  let root = test.temp_dir(ctx, name: "fs-set-times")?
  let file = fp"{root}/file"
  file.write("x")

  fs.set_times(file, atime_ns: 1700000000123456789, mtime_ns: 1600000000987654321)
  let both = fs.stat(file)?
  assert both.atime_ns == 1700000000123456789
  assert both.mtime_ns == 1600000000987654321

  fs.set_times(file, mtime_ns: 1500000000000000001)
  let mtime_only = fs.stat(file)?
  assert mtime_only.atime_ns == 1700000000123456789
  assert mtime_only.mtime_ns == 1500000000000000001

  fs.set_times(file)
  assert fs.stat(file)?.mtime_ns == 1500000000000000001

  fs.set_times(file, mtime_ns: -1500000000)
  assert fs.stat(file)?.mtime_ns == -1500000000
}

test test_set_times_sec_and_nsec_set_instants_the_nanosecond_count_cannot_hold { |ctx|
  let root = test.temp_dir(ctx, name: "fs-set-times-sec")?
  let file = fp"{root}/file"
  file.write("x")

  fs.set_times(file, atime_sec: 1700000000, atime_nsec: 123456789, mtime_sec: 1600000000)
  let both = fs.stat(file)?
  assert both.atime_ns == 1700000000123456789
  assert both.mtime_ns == 1600000000000000000

  fs.set_times(file, mtime_sec: -62167219200)
  let year_zero = fs.stat(file)?
  assert year_zero.atime_ns == 1700000000123456789
  assert year_zero.mtime_ns < -2000000000000000000

  let mixed = fs.set_times(file, mtime_ns: 1, mtime_sec: 1)
  assert mixed is Err(_)
  test.error_kind(mixed, "fs-set-times")
  let orphan = fs.set_times(file, atime_nsec: 5)
  assert orphan is Err(_)
  test.error_kind(orphan, "fs-set-times")
  let range = fs.set_times(file, atime_sec: 1, atime_nsec: 1000000000)
  assert range is Err(_)
  test.error_kind(range, "fs-set-times")
}

test test_set_times_now_uses_the_kernel_clock { |ctx|
  let root = test.temp_dir(ctx, name: "fs-set-times-now")?
  let file = fp"{root}/file"
  file.write("x")
  fs.set_times(file, atime_ns: 1000000000, mtime_ns: 2000000000)

  fs.set_times(file, mtime_now: true)
  let touched = fs.stat(file)?
  assert touched.atime_ns == 1000000000
  assert touched.mtime_ns > 1600000000000000000

  fs.set_times(file, atime_now: true)
  assert fs.stat(file)?.atime_ns > 1600000000000000000

  let both = fs.set_times(file, atime_ns: 1, atime_now: true)
  assert both is Err(_)
  test.error_kind(both, "fs-set-times")
}

test test_set_times_nofollow_changes_the_link_not_its_target { |ctx|
  let root = test.temp_dir(ctx, name: "fs-set-times-link")?
  let file = fp"{root}/file"
  let link = fp"{root}/link"
  file.write("x")
  link.symlink(to: p"file")
  fs.set_times(file, atime_ns: 1000000000, mtime_ns: 1000000000)

  fs.set_times(link, atime_ns: 3000000000, mtime_ns: 3000000000)
  assert fs.stat(link)?.mtime_ns == 3000000000
  assert fs.stat(file)?.mtime_ns == 1000000000

  fs.set_times(link, mtime_ns: 4000000000, follow_symlinks: true)
  assert fs.stat(file)?.mtime_ns == 4000000000
  assert fs.stat(link)?.mtime_ns == 3000000000
}

test test_set_owner_changes_both_ids_and_leaves_null_alone { |ctx|
  let root = test.temp_dir(ctx, name: "fs-set-owner")?
  let file = fp"{root}/file"
  let link = fp"{root}/link"
  file.write("x")
  link.symlink(to: p"file")
  let before = fs.stat(file)?

  fs.set_owner(file)
  fs.set_owner(file, uid: before.uid, gid: before.gid)
  assert fs.stat(file)?.uid == before.uid

  if applet.current_euid() == 0 {
    fs.set_owner(file, uid: 12345, gid: 54321)
    let changed = fs.stat(file)?
    assert changed.uid == 12345
    assert changed.gid == 54321

    fs.set_owner(file, gid: 777)
    assert fs.stat(file)?.uid == 12345
    assert fs.stat(file)?.gid == 777

    fs.set_owner(link, uid: 4242)
    assert fs.stat(link)?.uid == 4242
    assert fs.stat(file)?.uid == 12345

    fs.set_owner(link, uid: 4343, follow_symlinks: true)
    assert fs.stat(file)?.uid == 4343
    assert fs.stat(link)?.uid == 4242
  } else {
    let denied = fs.set_owner(file, uid: 0)
    assert denied is Err(is PermissionDenied)
    if let Err(failure) = denied {
      assert failure.errno == 1
    }
  }
}

test test_set_owner_rejects_ids_the_kernel_reads_as_unchanged { |ctx|
  let root = test.temp_dir(ctx, name: "fs-set-owner-range")?
  let file = fp"{root}/file"
  file.write("x")
  assert fs.set_owner(file, uid: -1) is Err(_)
  assert fs.set_owner(file, gid: 4294967295) is Err(_)
  assert fs.set_owner(file, uid: 4294967296) is Err(_)
}

test test_chmod_can_refuse_to_follow_a_symlink { |ctx|
  let root = test.temp_dir(ctx, name: "fs-chmod-link")?
  let file = fp"{root}/file"
  let link = fp"{root}/link"
  file.write("x")
  link.symlink(to: p"file")
  file.chmod(0o600)

  file.chmod(0o640, follow_symlinks: false)
  assert fs.stat(file)?.mode.bit_and(0o7777) == 0o640

  link.chmod(0o604)
  assert fs.stat(file)?.mode.bit_and(0o7777) == 0o604

  if system.uname()?.sysname == "Linux" {
    let refused = link.chmod(0o600, follow_symlinks: false)
    assert refused is Err(_)
    if let Err(failure) = refused {
      assert failure.errno != null
    }

    assert fs.stat(file)?.mode.bit_and(0o7777) == 0o604
  }

  assert file.chmod(0o10000) is Err(_)
}

test test_chmod_runs_in_every_call_position { |ctx|
  let root = test.temp_dir(ctx, name: "fs-chmod-positions")?
  let file = fp"{root}/file"
  file.write("x")

  assert file.chmod(0o611, follow_symlinks: true) is Ok(_)
  assert fs.stat(file)?.mode.bit_and(0o7777) == 0o611
  assert file.chmod(0o622, follow_symlinks: false) is Ok(_)
  assert fs.stat(file)?.mode.bit_and(0o7777) == 0o622

  let present: Path? = file
  let changed = present?.chmod(0o633)
  assert changed != null
  assert fs.stat(file)?.mode.bit_and(0o7777) == 0o633
  let _ = present?.chmod(0o644, follow_symlinks: false)
  assert fs.stat(file)?.mode.bit_and(0o7777) == 0o644
  let named = present?.chmod(0o655, follow_symlinks: true)
  assert named != null
  assert fs.stat(file)?.mode.bit_and(0o7777) == 0o655

  let absent: Path? = null
  let skipped = absent?.chmod(0o600)
  assert skipped == null
  assert fs.stat(file)?.mode.bit_and(0o7777) == 0o655
}

test test_mknod_creates_nodes_under_the_umask { |ctx|
  let root = test.temp_dir(ctx, name: "fs-mknod")?
  let mask = fs.umask()?
  assert mask >= 0 and mask <= 0o777

  fs.mknod(fp"{root}/fifo", "fifo", 0o666)
  let fifo = fs.stat(fp"{root}/fifo")?
  assert fifo.kind == "fifo"
  assert fifo.mode.bit_and(0o777) == 0o666.clear_bits(mask)

  fs.mknod(fp"{root}/regular", "file", 0o640)
  let regular = fs.stat(fp"{root}/regular")?
  assert regular.kind == "file"
  assert regular.size == 0

  let again = fs.mknod(fp"{root}/fifo", "fifo", 0o600)
  assert again is Err(_)
  if let Err(failure) = again {
    assert failure.errno == 17
  }

  assert fs.mknod(fp"{root}/bad", "pipe", 0o600) is Err(_)
  assert fs.mknod(fp"{root}/bad", "fifo", 0o600, major: 1, minor: 3) is Err(_)
  assert fs.mknod(fp"{root}/bad", "fifo", 0o20000) is Err(_)
}

test test_mknod_device_nodes_need_privilege_and_carry_device_numbers { |ctx|
  let root = test.temp_dir(ctx, name: "fs-mknod-dev")?
  let node = fp"{root}/node"
  if applet.current_euid() == 0 {
    let made = fs.mknod(node, "char", 0o600, major: 1, minor: 3)
    guard made is Ok(_) else {
      test.skip("the environment forbids device nodes")
      return
    }
    let st = fs.stat(node)?
    assert st.kind == "char"
    assert st.rdev == fs.makedev(1, 3)
    assert fs.dev_major(st.rdev) == 1
    assert fs.dev_minor(st.rdev) == 3
  } else {
    let denied = fs.mknod(node, "block", 0o600, major: 7, minor: 0)
    assert denied is Err(is PermissionDenied)
    if let Err(failure) = denied {
      assert failure.errno == 1
    }
  }
}

test test_device_numbers_round_trip_extended_encodings {
  assert fs.dev_major(fs.makedev(1, 3)) == 1
  assert fs.dev_minor(fs.makedev(1, 3)) == 3
  assert fs.dev_major(fs.makedev(1234, 567890)) == 1234
  assert fs.dev_minor(fs.makedev(1234, 567890)) == 567890
}

test test_link_follows_a_source_symlink_only_when_asked { |ctx|
  let root = test.temp_dir(ctx, name: "fs-link")?
  let file = fp"{root}/file"
  let link = fp"{root}/link"
  file.write("x")
  link.symlink(to: p"file")

  fs.link(link, fp"{root}/to-link")
  assert fs.stat(fp"{root}/to-link")?.kind == "symlink"
  assert fs.stat(fp"{root}/to-link")?.ino == fs.stat(link)?.ino

  fs.link(link, fp"{root}/to-file", follow_symlinks: true)
  assert fs.stat(fp"{root}/to-file")?.kind == "file"
  assert fs.stat(fp"{root}/to-file")?.ino == fs.stat(file)?.ino
  assert fs.stat(file)?.nlink == 2
}

test test_statvfs_reports_raw_counters_and_mount_identity { |ctx|
  let root = test.temp_dir(ctx, name: "fs-statvfs")?
  let raw = fs.statvfs(root)?
  assert raw.block_size > 0
  assert raw.fragment_size > 0
  assert raw.blocks > 0
  assert raw.blocks_available <= raw.blocks_free
  assert raw.blocks_free <= raw.blocks
  assert raw.files_free <= raw.files or raw.files == 0
  assert raw.name_max > 0
  if system.uname()?.sysname == "Linux" {
    assert raw.type_magic != null
  }

  let summary = fs.filesystem_stats(root)?
  assert summary.blocks_1k == raw.blocks * raw.fragment_size / 1024

  let mount = fs.mount_for(root)?
  assert mount.device == fs.stat(mount.mounted_on)?.dev
  assert mount.readonly == raw.readonly

  let missing = fs.statvfs(fp"{root}/missing")
  assert missing is Err(is NotFound)
}

test test_rename_noreplace_never_replaces_an_existing_destination { |ctx|
  let root = test.temp_dir(ctx, name: "fs-rename-noreplace")?
  let source = fp"{root}/source"
  let taken = fp"{root}/taken"
  let free = fp"{root}/free"
  source.write("source")
  taken.write("taken")

  let refused = fs.rename_noreplace(source, taken)
  assert refused is Err(_)
  if let Err(failure) = refused {
    if failure.errno == 22 or failure.errno == 95 {
      test.skip("the filesystem lacks RENAME_NOREPLACE")
      return
    }

    assert failure.errno == 17
  }

  assert source.read_text()? == "source"
  assert taken.read_text()? == "taken"

  fs.rename_noreplace(source, free)
  assert ! source.exists()?
  assert free.read_text()? == "source"

  let missing = fs.rename_noreplace(source, fp"{root}/elsewhere")
  assert missing is Err(is NotFound)
}

test test_data_ranges_lists_allocated_runs_of_a_sparse_file { |ctx|
  let root = test.temp_dir(ctx, name: "fs-data-ranges")?
  let file = fp"{root}/sparse"
  let size = 4194304
  let _ = bytes.write_at(file, 0, b"head", create: true)?
  let _ = bytes.write_at(file, size - 4, b"tail", create: true)?

  let ranges = fs.data_ranges(file)?
  assert ranges.len() >= 1
  assert ranges[0].offset == 0
  let last = ranges[-1]
  assert last.offset + last.length == size

  var covered = 0
  for range in ranges {
    covered = covered + range.length
  }

  if ranges.len() == 1 {
    test.skip("the filesystem reports no holes")
    return
  }

  assert ranges.len() == 2
  assert covered < size
  assert fs.data_ranges(fp"{root}/never-created") is Err(is NotFound)
}

test test_data_ranges_of_empty_and_dense_files { |ctx|
  let root = test.temp_dir(ctx, name: "fs-data-ranges-dense")?
  let empty = fp"{root}/empty"
  let dense = fp"{root}/dense"
  empty.write("")
  dense.write("0123456789")
  assert fs.data_ranges(empty)?.len() == 0
  let ranges = fs.data_ranges(dense)?
  assert ranges.len() == 1
  assert ranges[0].offset == 0
  assert ranges[0].length == 10
}

test test_copy_file_copies_bytes_with_the_source_mode_and_reports_the_method { |ctx|
  let root = test.temp_dir(ctx, name: "fs-copy-file")?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("hello world", mode: 0o640)

  let copied = fs.copy_file(source, dest)?
  assert ! copied.destination_replaced
  assert copied.reflink_error == null
  assert copied.bytes == 11
  assert copied.hole_bytes == 0
  assert copied.method == "copy_file_range" or copied.method == "read_write"
  assert dest.read_text()? == "hello world"
  assert fs.stat(dest)?.mode.bit_and(0o7777) == 0o640.clear_bits(fs.umask()?)
  assert fs.stat(dest)?.ino != fs.stat(source)?.ino

  let explicit = fp"{root}/explicit"
  let _ = fs.copy_file(source, explicit, mode: 0o600)?
  assert fs.stat(explicit)?.mode.bit_and(0o7777) == 0o600.clear_bits(fs.umask()?)

  let empty = fp"{root}/empty"
  let empty_copy = fp"{root}/empty-copy"
  empty.write("")
  assert fs.copy_file(empty, empty_copy)?.bytes == 0
  assert fs.stat(empty_copy)?.size == 0
}

test test_copy_file_overwrite_truncates_and_exclusive_refuses { |ctx|
  let root = test.temp_dir(ctx, name: "fs-copy-file-overwrite")?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("new")
  dest.write("a much longer previous content")

  let refused = fs.copy_file(source, dest, overwrite: false)
  assert refused is Err(_)
  if let Err(failure) = refused {
    assert failure.errno == 17
  }

  assert dest.read_text()? == "a much longer previous content"

  let overwritten = fs.copy_file(source, dest)?
  assert ! overwritten.destination_replaced
  assert dest.read_text()? == "new"
}

test test_copy_file_refuses_the_same_file_and_directory_sources { |ctx|
  let root = test.temp_dir(ctx, name: "fs-copy-file-refuse")?
  let source = fp"{root}/source"
  let alias = fp"{root}/alias"
  source.write("keep")
  fs.link(source, alias)

  assert fs.copy_file(source, source) is Err(_)
  assert fs.copy_file(source, alias) is Err(_)
  assert source.read_text()? == "keep"

  assert fs.copy_file(root, fp"{root}/dir-copy") is Err(_)
  assert ! fp"{root}/dir-copy".exists()?

  let missing = fs.copy_file(fp"{root}/missing", fp"{root}/out")
  assert missing is Err(is NotFound)
  assert ! fp"{root}/out".exists()?

  assert fs.copy_file(source, fp"{root}/out", sparse: "sometimes") is Err(_)
  assert fs.copy_file(source, fp"{root}/out", reflink: "yes") is Err(_)
  assert ! fp"{root}/out".exists()?
}

test test_copy_file_preserves_holes_unless_told_not_to { |ctx|
  let root = test.temp_dir(ctx, name: "fs-copy-file-sparse")?
  let source = fp"{root}/source"
  let size = 4194304
  let _ = bytes.write_at(source, 0, b"head", create: true)?
  let _ = bytes.write_at(source, size - 4, b"tail", create: true)?
  if fs.data_ranges(source)?.len() < 2 {
    test.skip("the filesystem reports no holes")
    return
  }

  let kept = fp"{root}/kept"
  let report = fs.copy_file(source, kept)?
  assert report.bytes == size
  assert report.hole_bytes > size / 2
  assert fs.stat(kept)?.size == size
  assert fs.data_ranges(kept)?.len() == 2
  assert kept.read_bytes()? == source.read_bytes()?

  let dense = fp"{root}/dense"
  let dense_report = fs.copy_file(source, dense, sparse: "never")?
  assert dense_report.hole_bytes == 0
  assert fs.stat(dense)?.size == size
  assert fs.stat(dense)?.blocks_512 * 512 >= size
  assert dense.read_bytes()? == source.read_bytes()?
}

test test_copy_file_sparse_always_turns_zero_blocks_into_holes { |ctx|
  let root = test.temp_dir(ctx, name: "fs-copy-file-zeros")?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  let zeros = bytes.zero(1048576)?
  source.write(bytes.concat([zeros, b"data", zeros]))
  assert fs.stat(source)?.blocks_512 * 512 >= 2097152

  let report = fs.copy_file(source, dest, sparse: "always")?
  assert report.bytes == 2097156
  assert report.hole_bytes >= 2097152 - 8192
  assert dest.read_bytes()? == source.read_bytes()?
  let kept = fs.data_ranges(dest)?
  if kept.len() == 1 and kept[0].length >= report.bytes {
    test.skip("the filesystem does not keep holes")
    return
  }

  assert kept.len() == 1
  assert kept[0].offset >= 1048576 - 4096
  assert fs.stat(dest)?.blocks_512 * 512 < 1048576
}

test test_copy_file_reflink_clones_or_fails_with_an_errno { |ctx|
  let root = test.temp_dir(ctx, name: "fs-copy-file-reflink")?
  let source = fp"{root}/source"
  let auto = fp"{root}/auto"
  let strict = fp"{root}/strict"
  source.write("reflink me")

  let fallback = fs.copy_file(source, auto, reflink: "auto")?
  assert auto.read_text()? == "reflink me"
  assert fallback.method == "clone" or fallback.method == "copy_file_range" or fallback.method == "read_write"

  let always = fs.copy_file(source, strict, reflink: "always")
  if let Ok(cloned) = always {
    assert cloned.method == "clone"
    assert cloned.reflink_error == null and cloned.offload_error == null
    assert strict.read_text()? == "reflink me"
  } else if let Err(failure) = always {
    assert failure.errno != null
    if system.uname()?.sysname == "Linux" {
      assert fallback.reflink_error?.errno == failure.errno
    } else {
      assert fallback.reflink_error == null
    }
    assert ! strict.exists()?
    test.skip("the filesystem cannot clone files")
  }
}

test test_copy_file_never_clones_by_default { |ctx|
  let root = test.temp_dir(ctx, name: "fs-copy-file-never")?
  let source = fp"{root}/source"
  source.write("data")
  assert fs.copy_file(source, fp"{root}/dest")?.method != "clone"
  assert fs.copy_file(source, fp"{root}/dest2", reflink: "never")?.method != "clone"
}

test test_copy_file_reads_zero_size_virtual_files_until_eof { |ctx|
  let source = /proc/version
  if ! source.exists()? {
    test.skip("procfs is unavailable")
    return
  }

  let root = test.temp_dir(ctx, name: "fs-copy-proc")?
  let proc_root = fs.open_root(/proc)?
  defer proc_root.close()
  let expected = proc_root.read_bytes(p"version")?
  assert fs.stat(source)?.size == 0
  assert expected.len() > 0
  for sparse in ["auto", "always", "never"] {
    let dest = fp"{root}/{sparse}"
    let report = fs.copy_file(source, dest, sparse: sparse, reflink: "auto")?
    if sparse == "always" {
      assert report.offload_error == null
    } else {
      assert report.offload_error?.errno == 18
    }
    assert report.bytes == expected.len()
    assert dest.read_bytes()? == expected
    assert report.method == "read_write"
  }
}

test test_copy_file_does_not_pad_sysfs_files_to_their_reported_size { |ctx|
  let source = /sys/kernel/uevent_seqnum
  if ! source.exists()? {
    test.skip("sysfs is unavailable")
    return
  }

  let root = test.temp_dir(ctx, name: "fs-copy-sys")?
  for sparse in ["auto", "always", "never"] {
    let dest = fp"{root}/{sparse}"
    let report = fs.copy_file(source, dest, sparse: sparse)?
    let content = dest.read_bytes()?
    assert report.bytes == content.len()
    assert content.len() > 0
    assert content.len() < fs.stat(source)?.size
    assert dest.read_text()?.ends_with("\n")
    assert ! (b"\0" in content)
  }
}

test test_copy_file_streams_fifo_bytes_and_refuses_same_fifo_without_a_writer { |ctx|
  let root = test.temp_dir(ctx, name: "fs-copy-fifo")?
  let fifo = fp"{root}/fifo"
  let dest = fp"{root}/dest"
  fs.mknod(fifo, "fifo", 0o600)
  assert fs.copy_file(fifo, fifo) is Err(_)
  let writer = spawn run sh -c "printf \"fifo payload\" > \"$1\"" sh $fifo ?
  let report = fs.copy_file(fifo, dest)?
  assert (wait writer?).ok
  assert report.bytes == 12
  assert report.hole_bytes == 0
  assert report.method == "read_write"
  assert dest.read_bytes()? == b"fifo payload"

  let sparse = fp"{root}/sparse"
  let zeros = spawn run sh -c "printf '\\000\\000\\000' > \"$1\"" sh $fifo ?
  let holes = fs.copy_file(fifo, sparse, sparse: "always")?
  assert (wait zeros?).ok
  assert holes.bytes == 3
  assert holes.hole_bytes == 3
  assert sparse.read_bytes()? == bytes.zero(3)?
}

test test_copy_file_streams_device_sources_and_keeps_same_inode_symlinks { |ctx|
  let root = test.temp_dir(ctx, name: "fs-copy-device")?
  let empty = fp"{root}/empty"
  assert fs.copy_file(/dev/null, empty)?.bytes == 0
  assert empty.read_bytes()? == b""
  let source = fp"{root}/source"
  let alias = fp"{root}/alias"
  source.write("keep")
  alias.symlink(to: p"source")
  assert fs.copy_file(source, alias) is Err(_)
  assert source.read_text()? == "keep"
}

test test_copy_file_streams_to_devices_without_truncating_or_claiming_holes { |ctx|
  let root = test.temp_dir(ctx, name: "fs-copy-device-output")?
  let source = fp"{root}/source"
  let payload = bytes.concat([bytes.zero(65536)?, b"payload"])
  source.write(payload)
  for sparse in ["auto", "always", "never"] {
    let report = fs.copy_file(source, /dev/null, sparse: sparse, reflink: "auto")?
    assert report.bytes == payload.len()
    assert report.hole_bytes == 0
    assert report.method == "read_write"
    assert fs.stat(/dev/null)?.kind == "char"
  }
  assert fs.copy_file(source, /dev/null, reflink: "always") is Err(_)
  if p"/dev/full".exists()? {
    let full = fs.copy_file(source, /dev/full)
    assert full is Err(_)
    if let Err(failure) = full {
      assert failure.errno == 28
    }
    assert fs.stat(/dev/full)?.kind == "char"
  }
}

test test_copy_file_reflink_always_reports_the_kernel_errno_for_devices { |ctx|
  let root = test.temp_dir(ctx, name: "fs-copy-reflink-device")?
  if p"/dev/full".exists()? {
    let device = fs.copy_file(/dev/null, /dev/full, reflink: "always")
    assert device is Err(_)
    test.error_kind(device, "fs-copy")
    if let Err(failure) = device { assert failure.errno == 22 }
  }
  let target = fp"{root}/target"
  let crossing = fs.copy_file(/dev/null, target, reflink: "always")
  assert crossing is Err(_)
  if let Err(failure) = crossing { assert failure.errno == 18 }
  assert ! target.exists()?
}

test test_copy_file_streams_every_byte_to_a_fifo_destination { |ctx|
  let root = test.temp_dir(ctx, name: "fs-copy-fifo-output")?
  let source = fp"{root}/source"
  let fifo = fp"{root}/fifo"
  let received = fp"{root}/received"
  let payload = bytes.concat([bytes.zero(131072)?, b"tail"])
  source.write(payload)
  fs.mknod(fifo, "fifo", 0o600)
  # The child owns the reader and exits at EOF; the native spawn lifecycle
  # cleans it up if copying or an assertion fails.
  let reader = spawn run sh -c "cat < \"$1\" > \"$2\"" sh $fifo $received ?
  let report = fs.copy_file(source, fifo, sparse: "always", reflink: "auto")?
  assert (wait reader?).ok
  assert report.bytes == payload.len()
  assert report.hole_bytes == 0
  assert report.method == "read_write"
  assert received.read_bytes()? == payload
  assert fs.stat(fifo)?.kind == "fifo"
  assert fs.copy_file(source, fifo, reflink: "always") is Err(_)
}

test test_copy_file_force_preserves_destinations_for_missing_and_invalid_sources { |ctx|
  let root = test.temp_dir(ctx, name: "fs-copy-force-source-errors")?
  let dest = fp"{root}/dest"
  dest.write("keep")
  let ino = fs.stat(dest)?.ino
  assert fs.copy_file(fp"{root}/missing", dest, force: true) is Err(is NotFound)
  assert dest.read_text()? == "keep"
  assert fs.stat(dest)?.ino == ino
  assert fs.copy_file(root, dest, force: true) is Err(_)
  assert dest.read_text()? == "keep"
  assert fs.copy_file(dest, dest, force: true) is Err(_)
  assert dest.read_text()? == "keep"
}

test test_copy_file_force_replaces_only_an_unopenable_destination { |ctx|
  if applet.current_euid() == 0 {
    test.skip("requires unprivileged ownership to exercise open permission failures")
    return
  }
  let root = test.temp_dir(ctx, name: "fs-copy-force-permissions")?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  let retained = fp"{root}/retained"
  source.write("copied")
  dest.write("keep", mode: 0o400)
  fs.link(dest, retained)
  let old_ino = fs.stat(dest)?.ino
  assert fs.copy_file(source, dest) is Err(_)
  assert dest.read_text()? == "keep"
  assert fs.copy_file(source, dest, force: true, overwrite: false) is Err(_)
  assert fs.stat(dest)?.ino == old_ino
  source.chmod(0o000)
  let refused_source = fs.copy_file(source, dest, force: true)
  assert refused_source is Err(_)
  if let Err(failure) = refused_source {
    assert failure.errno == 13
  }
  assert dest.read_text()? == "keep"
  assert fs.stat(dest)?.ino == old_ino
  source.chmod(0o600)
  let report = fs.copy_file(source, dest, force: true)?
  assert report.destination_replaced
  assert report.bytes == 6
  assert dest.read_text()? == "copied"
  assert fs.stat(dest)?.ino != old_ino
  assert retained.read_text()? == "keep"
}

test test_copy_file_force_replaces_only_proven_final_symlink_loops { |ctx|
  let root = test.temp_dir(ctx, name: "fs-copy-force-loop")?
  let source = fp"{root}/source"
  let loop_path = fp"{root}/loop"
  source.write("copied")
  loop_path.symlink(to: p"loop")
  assert fs.copy_file(source, loop_path) is Err(_)
  assert fs.copy_file(source, loop_path, force: true, overwrite: false) is Err(_)
  assert loop_path.readlink()? == p"loop"
  let missing = fp"{root}/missing"
  assert fs.copy_file(missing, loop_path, force: true) is Err(is NotFound)
  assert loop_path.readlink()? == p"loop"
  let replaced = fs.copy_file(source, loop_path, force: true)?
  assert replaced.destination_replaced
  assert replaced.bytes == 6
  assert fs.stat(loop_path)?.kind == "file"
  assert loop_path.read_text()? == "copied"

  let ancestor = fp"{root}/ancestor"
  ancestor.symlink(to: p"ancestor")
  let refused = fs.copy_file(source, fp"{ancestor}/child", force: true)
  assert refused is Err(_)
  if let Err(failure) = refused {
    assert failure.errno == 40 or failure.errno == 62
  }
  assert ancestor.readlink()? == p"ancestor"
  assert source.read_text()? == "copied"
}

test test_copy_file_reports_cross_device_fallback_without_replacing_or_losing_bytes { |ctx|
  if system.uname()?.sysname != "Linux" or ! p"/dev/shm".exists()? {
    test.skip("cross-device fallback requires Linux tmpfs")
    return
  }
  let root = test.temp_dir(ctx, name: "copy-cross-device")?
  let remote = fs.tempdir_in(/dev/shm)?
  defer remote.close()
  let remote_path = remote.host_path()?
  if fs.stat(root)?.dev == fs.stat(remote_path)?.dev {
    test.skip("test root and tmpfs share a device")
    return
  }
  let source = fp"{root}/source"
  let dest = fp"{remote_path}/dest"
  source.write("cross-device payload")
  dest.write("old destination with a longer tail")
  let before = fs.stat(dest)?

  let copied = fs.copy_file(source, dest, reflink: "auto", sparse: "never")?
  assert copied.reflink_error?.errno == 18
  assert copied.offload_error?.errno == 18
  assert copied.method == "read_write"
  assert copied.bytes == 20 and copied.hole_bytes == 0
  assert ! copied.destination_replaced
  assert fs.stat(dest)?.ino == before.ino
  assert source.read_text()? == "cross-device payload"
  assert dest.read_bytes()? == source.read_bytes()?

  let strict = fp"{remote_path}/strict"
  let refused = fs.copy_file(source, strict, reflink: "always")
  assert refused is Err(_)
  if let Err(failure) = refused { assert failure.errno == copied.reflink_error?.errno }
  assert ! strict.exists()?

  let streamed = fs.copy_file(source, fp"{remote_path}/streamed", sparse: "always", reflink: "never")?
  assert streamed.reflink_error == null and streamed.offload_error == null
  assert fp"{remote_path}/streamed".read_bytes()? == source.read_bytes()?
}
