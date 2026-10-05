test test_xattrs_round_trip_binary_and_empty_values_with_strict_modes { |ctx|
  let root = test.temp_dir(ctx, name: "fs-xattr")?
  let file = fp"{root}/file"
  file.write("content")
  let attribute = "user.xsh-test"
  let payload = bytes.concat([b"header\0", bytes.zero(8192)?, b"\xfftail"])
  let created = fs.xattr_set(file, attribute, payload, mode: "create")
  if let Err(failure) = created {
    if failure.errno == 95 or failure.errno == 45 {
      test.skip("extended attributes are unsupported on the filesystem")
      return
    }
    test.fail(f"xattr creation failed: {failure.message}")
  }
  assert fs.xattr_get(file, attribute)? == payload
  assert attribute in fs.xattr_list(file)?
  let duplicate = fs.xattr_set(file, attribute, b"other", mode: "create")
  assert duplicate is Err(_)
  if let Err(failure) = duplicate {
    assert failure.errno == 17
  }
  assert fs.xattr_get(file, attribute)? == payload

  fs.xattr_set(file, attribute, b"", mode: "replace")
  assert fs.xattr_get(file, attribute)? == b""
  fs.xattr_set(file, attribute, b"upsert")
  assert fs.xattr_get(file, attribute)? == b"upsert"
  fs.xattr_remove(file, attribute)
  assert ! (attribute in fs.xattr_list(file)?)
  assert fs.xattr_get(file, attribute) is Err(_)
  assert fs.xattr_remove(file, attribute) is Err(_)
  assert fs.xattr_set(file, attribute, b"x", mode: "replace") is Err(_)
  assert file.read_text()? == "content"
}

test test_xattrs_follow_control_keeps_symlink_and_target_attributes_separate { |ctx|
  let root = test.temp_dir(ctx, name: "fs-xattr-follow")?
  let target = fp"{root}/target"
  let link = fp"{root}/link"
  target.write("content")
  link.symlink(to: p"target")
  let attribute = "user.xsh-follow"
  let created = fs.xattr_set(link, attribute, b"target")
  if let Err(failure) = created {
    if failure.errno == 95 or failure.errno == 45 {
      test.skip("extended attributes are unsupported on the filesystem")
      return
    }
    test.fail(f"xattr creation failed: {failure.message}")
  }
  assert fs.xattr_get(target, attribute)? == b"target"
  assert fs.xattr_get(link, attribute)? == b"target"
  assert ! (attribute in fs.xattr_list(link, follow_symlinks: false)?)
  assert fs.xattr_get(link, attribute, follow_symlinks: false) is Err(_)
  let changed = fs.xattr_set(link, attribute, b"link", follow_symlinks: false)
  if changed is Ok(_) {
    assert fs.xattr_get(link, attribute, follow_symlinks: false)? == b"link"
    fs.xattr_remove(link, attribute, follow_symlinks: false)
  } else if let Err(failure) = changed {
    assert failure.errno == 1 or failure.errno == 95 or failure.errno == 45
  }
  assert fs.xattr_get(target, attribute)? == b"target"
  fs.xattr_remove(link, attribute)
  assert fs.xattr_get(target, attribute) is Err(_)
}

test test_xattrs_invalid_modes_names_and_missing_paths_fail { |ctx|
  let root = test.temp_dir(ctx, name: "fs-xattr-errors")?
  let file = fp"{root}/file"
  file.write("content")
  assert fs.xattr_set(file, "user.xsh-test", b"data", mode: "append") is Err(_)
  assert fs.xattr_set(file, "user.xsh\0test", b"data") is Err(_)
  assert fs.xattr_get(file, "user.xsh\0test") is Err(_)
  assert fs.xattr_remove(file, "user.xsh\0test") is Err(_)
  let missing = fp"{root}/missing"
  assert fs.xattr_list(missing) is Err(is NotFound)
  assert fs.xattr_get(missing, "user.xsh-test") is Err(is NotFound)
  assert fs.xattr_set(missing, "user.xsh-test", b"data") is Err(is NotFound)
  assert fs.xattr_remove(missing, "user.xsh-test") is Err(is NotFound)
}
