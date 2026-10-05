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

test test_access_checks_effective_permissions_and_requires_all_requested_modes { |ctx|
  let root = test.temp_dir(ctx, name: "fs-access")?
  let file = fp"{root}/file"
  file.write("content", mode: 0o600)
  assert fs.access(file)?
  assert fs.access(file, read: true, write: true)?
  assert ! fs.access(file, execute: true)?
  assert ! fs.access(file, read: true, execute: true)?
  file.chmod(0o700)
  assert fs.access(file, read: true, write: true, execute: true)?
  file.chmod(0o400)
  assert fs.access(file, read: true)?
  if applet.current_euid() != 0 {
    assert ! fs.access(file, write: true)?
    assert ! fs.access(file, read: true, write: true)?
  }
  assert fs.access(root, execute: true)?
}

test test_access_follow_control_and_host_errors_remain_observable { |ctx|
  let root = test.temp_dir(ctx, name: "fs-access-follow")?
  let target = fp"{root}/target"
  let link = fp"{root}/link"
  let dangling = fp"{root}/dangling"
  target.write("content", mode: 0o600)
  link.symlink(to: p"target")
  dangling.symlink(to: p"missing")
  assert fs.access(link, read: true)?
  assert ! fs.access(link, execute: true)?
  assert fs.access(link, follow_symlinks: false)?
  assert fs.access(dangling, follow_symlinks: false)?
  assert fs.access(dangling) is Err(is NotFound)
  assert fs.access(fp"{root}/missing") is Err(is NotFound)
  let not_directory = fs.access(fp"{target}/child")
  assert not_directory is Err(_)
  if let Err(failure) = not_directory {
    assert failure.errno == 20
  }
}
