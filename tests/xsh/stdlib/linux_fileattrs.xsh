test test_linux_inode_ioctls_do_not_follow_symbolic_links { |ctx|
  let root = test.temp_dir(ctx, name: "linux-inode-nofollow")?
  let file = fp"{root}/file"
  let link = fp"{root}/link"
  file.write("content")
  link.symlink(to: p"file")
  let before = linux.file_attrs(file)
  if let Err(failure) = before {
    if failure.errno == 95 or failure.errno == 25 { test.skip("inode ioctls unavailable on fixture filesystem"); return }
    test.fail(failure.message)
  }
  let flags = before?.flags
  let read = linux.file_attrs(link)
  assert read is Err(_)
  if let Err(failure) = read { assert failure.errno == 40 }
  let generation = linux.file_version(link)
  assert generation is Err(_)
  if let Err(failure) = generation { assert failure.errno == 40 }
  let write = linux.set_file_attrs(link, flags.bit_or(64))
  assert write is Err(_)
  if let Err(failure) = write { assert failure.errno == 40 }
  assert linux.file_attrs(file)?.flags == flags
  let version = linux.set_file_version(link, 7)
  assert version is Err(_)
  if let Err(failure) = version { assert failure.errno == 40 }
  assert file.read_text()? == "content"
}

test test_linux_inode_open_errors_retain_errno { |ctx|
  let root = test.temp_dir(ctx, name: "linux-inode-errors")?
  let missing = fp"{root}/missing"
  if let Err(failure) = linux.file_attrs(missing) { assert failure.errno == 2 } else { assert false, "missing inode accepted" }
  if let Err(failure) = linux.file_version(missing) { assert failure.errno == 2 } else { assert false, "missing inode accepted" }
}

test test_linux_project_ids_preserve_flags_on_temporary_inodes { |ctx|
  let root = test.temp_dir(ctx, name: "linux-project")?
  let file = fp"{root}/file"
  let directory = fp"{root}/directory"
  file.write("content")
  directory.mkdir()
  for target in [file, directory] {
    let original_project = linux.file_project(target)
    if let Err(failure) = original_project {
      if failure.errno == 95 or failure.errno == 25 { test.skip("project IDs unavailable on fixture filesystem"); return }
      test.fail(failure.message)
    }
    let before = linux.file_attrs(target)?.flags
    let marked = before.bit_or(64)
    linux.set_file_attrs(target, marked)
    let updated = linux.set_file_project(target, 137)
    if let Err(failure) = updated {
      linux.set_file_attrs(target, before)
      if failure.errno == 95 or failure.errno == 1 { test.skip(f"fixture project-ID update refused (errno {failure.errno ?? 0}): {failure.message}"); return }
      test.fail(failure.message)
    }
    assert linux.file_project(target)? == 137
    assert linux.file_attrs(target)?.flags == marked
    linux.set_file_project(target, original_project?)
    linux.set_file_attrs(target, before)
  }
  assert file.read_text()? == "content"
}

test test_linux_project_ids_validate_range_and_never_follow_links { |ctx|
  let root = test.temp_dir(ctx, name: "linux-project-errors")?
  let file = fp"{root}/file"
  let link = fp"{root}/link"
  file.write("content")
  link.symlink(to: p"file")
  assert linux.set_file_project(file, -1) is Err(_)
  assert linux.set_file_project(file, 4294967296) is Err(_)
  let read = linux.file_project(link)
  assert read is Err(_)
  if let Err(failure) = read { assert failure.errno == 40 }
  let write = linux.set_file_project(link, 7)
  assert write is Err(_)
  if let Err(failure) = write { assert failure.errno == 40 }
  let missing = linux.file_project(fp"{root}/missing")
  assert missing is Err(_)
  if let Err(failure) = missing { assert failure.errno == 2 }
}
