##! Transcribed from the MIT-licensed uutils sync integration tests.

use support.uu as uu

# origin: uutils test_sync::test_invalid_arg
test test_uu_sync_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(uu.invoke(s, "sync", ["--definitely-invalid"], timeout: 30s)?, 1)
}

# origin: uutils test_sync::test_sync_data
test test_uu_sync_sync_data { |ctx|
  let s = uu.scene(ctx)?
  let directory = test.temp_dir(ctx, name: "sync-data")?
  uu.succeeds(uu.invoke(s, "sync", ["--data", directory.display()], timeout: 30s)?)
}

# origin: uutils test_sync::test_sync_data_but_not_file
test test_uu_sync_sync_data_but_not_file { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sync", ["--data"], timeout: 30s)?
  uu.fails(r)
  uu.stderr_contains(r, "sync: --data needs at least one argument")
}

# origin: uutils test_sync::test_sync_data_fifo_fails_immediately
test test_uu_sync_sync_data_fifo_fails_immediately { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "test-fifo")?
  let r = uu.invoke(s, "sync", ["--data", uu.at(s, "test-fifo").display()], timeout: 2s)?
  uu.fails(r)
  uu.stderr_contains(r, "error syncing")
  uu.stderr_contains(r, "test-fifo")
  uu.stderr_contains(r, "Invalid argument")
}

# origin: uutils test_sync::test_sync_data_nonblock_flag_reset
test test_uu_sync_sync_data_nonblock_flag_reset { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "test_file.txt", "test content")?
  uu.succeeds(uu.invoke(s, "sync", ["--data", "test_file.txt"], timeout: 30s)?)
}

# origin: uutils test_sync::test_sync_default
test test_uu_sync_sync_default { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "sync", [], timeout: 30s)?)
}

# origin: uutils test_sync::test_sync_fdatasync_error_handling
test test_uu_sync_sync_fdatasync_error_handling { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sync", ["--data", "/nonexistent/path/to/file"], timeout: 30s)?
  uu.fails(r)
  uu.stderr_contains(r, "error opening")
}

# origin: uutils test_sync::test_sync_fs
test test_uu_sync_sync_fs { |ctx|
  let s = uu.scene(ctx)?
  let directory = test.temp_dir(ctx, name: "sync-fs")?
  uu.succeeds(uu.invoke(s, "sync", ["--file-system", directory.display()], timeout: 30s)?)
}

# origin: uutils test_sync::test_sync_fs_nonblock_flag_reset
test test_uu_sync_sync_fs_nonblock_flag_reset { |ctx|
  let s = uu.scene(ctx)?
  let directory = test.temp_dir(ctx, name: "sync-fs-flags")?
  uu.succeeds(uu.invoke(s, "sync", ["--file-system", directory.display()], timeout: 30s)?)
}

# origin: uutils test_sync::test_sync_fs_without_files_falls_back_to_full_sync
test test_uu_sync_sync_fs_without_files_falls_back_to_full_sync { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "sync", ["--file-system"], timeout: 30s)?)
}

# origin: uutils test_sync::test_sync_incorrect_arg
test test_uu_sync_sync_incorrect_arg { |ctx|
  let s = uu.scene(ctx)?
  uu.fails(uu.invoke(s, "sync", ["--foo"], timeout: 30s)?)
}

# origin: uutils test_sync::test_sync_multiple_files
test test_uu_sync_sync_multiple_files { |ctx|
  let s = uu.scene(ctx)?
  let directory = test.temp_dir(ctx, name: "sync-multiple")?
  let file1 = fp"{directory}/file1.txt"
  let file2 = fp"{directory}/file2.txt"
  file1.write("content1")?
  file2.write("content2")?
  uu.succeeds(uu.invoke(s, "sync", ["--data", file1.display(), file2.display()], timeout: 30s)?)
}

# origin: uutils test_sync::test_sync_multiple_nonexistent_files
test test_uu_sync_sync_multiple_nonexistent_files { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sync", ["--data", "bad1", "bad2"], timeout: 30s)?
  uu.fails(r)
  uu.stderr_is(r, "sync: error opening 'bad1': No such file or directory\nsync: error opening 'bad2': No such file or directory\n")
}

# origin: uutils test_sync::test_sync_no_existing_files
test test_uu_sync_sync_no_existing_files { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sync", ["--data", "do-no-exist"], timeout: 30s)?
  uu.fails(r)
  uu.stderr_contains(r, "error opening")
}

# origin: uutils test_sync::test_sync_no_permission_dir
test test_uu_sync_sync_no_permission_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir_all(s, "foo")?
  uu.set_mode(s, "foo", 0o000)?
  defer uu.set_mode(s, "foo", 0o700)
  for args in [["--data", "foo"], ["foo"]] {
    let r = uu.invoke(s, "sync", args, timeout: 30s)?
    uu.fails(r)
    uu.stderr_contains(r, "sync: error opening 'foo': Permission denied")
  }
}

# origin: uutils test_sync::test_sync_no_permission_file
test test_uu_sync_sync_no_permission_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.set_mode(s, "file", 0o200)?
  uu.succeeds(uu.invoke(s, "sync", ["file"], timeout: 30s)?)
}
