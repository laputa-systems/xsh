test test_sync_path_flushes_files_directories_and_rejects_invalid_modes { |ctx|
  let root = test.temp_dir(ctx, name: "fs-sync-path")?
  let file = fp"{root}/file"
  file.write("durable")
  fs.sync_path(file)
  fs.sync_path(root)
  for mode in ["data", "filesystem"] {
    let result = fs.sync_path(file, mode: mode)
    if let Err(failure) = result {
      assert failure.errno == 95 or failure.errno == 45
    }
  }
  assert fs.sync_path(file, mode: "invalid") is Err(_)
  assert fs.sync_path(fp"{root}/missing") is Err(is NotFound)
  assert file.read_text()? == "durable"
}

test test_sync_path_does_not_wait_for_a_fifo_writer { |ctx|
  let root = test.temp_dir(ctx, name: "fs-sync-fifo")?
  let fifo = fp"{root}/fifo"
  fs.mkfifo(fifo, 0o600)
  for mode in ["all", "data"] {
    let result = fs.sync_path(fifo, mode: mode)
    assert result is Err(_)
    if let Err(failure) = result {
      assert failure.errno == 22 or failure.errno == 95 or failure.errno == 45
    }
  }
}

test test_rename_exchange_swaps_inodes_without_following_symlinks { |ctx|
  let root = test.temp_dir(ctx, name: "fs-exchange")?
  let left = fp"{root}/left"
  let right = fp"{root}/right"
  left.write("left")
  right.write("right")
  let left_ino = fs.stat(left)?.ino
  let right_ino = fs.stat(right)?.ino
  let exchanged = fs.rename_exchange(left, right)
  if let Err(failure) = exchanged {
    if failure.errno == 22 or failure.errno == 95 or failure.errno == 45 {
      test.skip("atomic rename exchange is unsupported")
      return
    }
    test.fail(f"exchange failed: {failure.message}")
  }
  assert left.read_text()? == "right"
  assert right.read_text()? == "left"
  assert fs.stat(left)?.ino == right_ino
  assert fs.stat(right)?.ino == left_ino
  assert fs.rename_exchange(left, fp"{root}/missing") is Err(is NotFound)
  assert left.read_text()? == "right"

  let link = fp"{root}/link"
  link.symlink(to: p"left")
  fs.rename_exchange(link, right)
  assert fs.stat(right)?.kind == "symlink"
  assert right.readlink()? == p"left"
  assert link.read_text()? == "left"
  assert left.read_text()? == "right"
}

test test_path_limits_reports_host_byte_limits_and_missing_paths { |ctx|
  let root = test.temp_dir(ctx, name: "fs-path-limits")?
  let limits = fs.path_limits(root)?
  assert limits.name_max > 0
  assert limits.path_max > limits.name_max
  assert fs.path_limits(fp"{root}/missing") is Err(is NotFound)
}
