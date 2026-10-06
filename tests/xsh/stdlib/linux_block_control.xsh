proc failure_errno(result: Result[Any, Error], kind: Str) [error] -> Int {
  match result {
    Ok(_) => {
      assert false, "host operation unexpectedly succeeded"
      -1
    }
    Err(failure) => {
      test.error_kind(failure, kind)
      assert failure.errno != null, "host errno was lost"
      failure.errno ?? -1
    }
  }
}

test test_linux_block_control_preserves_missing_path_errno { |ctx|
  let root = test.temp_dir(ctx, name: "block-control-missing")?
  let missing = fp"{root}/missing"

  if system.uname()?.sysname != "Linux" {
    test.error_kind(linux.umount(missing), "linux-unsupported")
    test.error_kind(linux.blockdev_info(missing), "linux-unsupported")
    test.error_kind(linux.blockdev_set_read_only(missing, true), "linux-unsupported")
    test.error_kind(linux.blockdev_flush(missing), "linux-unsupported")
    test.error_kind(linux.blockdev_reread_partition_table(missing), "linux-unsupported")
    test.error_kind(linux.fstrim(missing), "linux-unsupported")
    test.error_kind(linux.fsfreeze(missing, true), "linux-unsupported")
    test.error_kind(linux.block_signatures(missing), "linux-unsupported")
    test.error_kind(linux.wipe_block_signatures(missing, [0]), "linux-unsupported")
    return
  }

  assert failure_errno(linux.blockdev_info(missing), "linux-blockdev") == 2
  assert failure_errno(linux.blockdev_set_read_only(missing, true), "linux-blockdev") == 2
  assert failure_errno(linux.blockdev_set_read_only(missing, false), "linux-blockdev") == 2
  assert failure_errno(linux.blockdev_flush(missing), "linux-blockdev") == 2
  assert failure_errno(linux.blockdev_reread_partition_table(missing), "linux-blockdev") == 2
  assert failure_errno(linux.fstrim(missing), "linux-fstrim") == 2
  assert failure_errno(linux.fstrim(missing, offset: 512, length: 1024, minlen: 512), "linux-fstrim") == 2
  assert failure_errno(linux.fsfreeze(missing, true), "linux-fsfreeze") == 2
  assert failure_errno(linux.fsfreeze(missing, false), "linux-fsfreeze") == 2
  assert failure_errno(linux.block_signatures(missing), "linux-block-signatures") == 2
  assert failure_errno(linux.wipe_block_signatures(missing, [0]), "linux-wipe-signatures") == 2
  assert linux.blockdev_info(missing) is Err(is NotFound)
}

test test_linux_block_control_rejects_regular_files_without_modifying_them { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("block-device ioctls are Linux-only")
    return
  }

  let root = test.temp_dir(ctx, name: "block-control-file")?
  let image = fp"{root}/image"
  let contents = bytes.concat([bytes.from_text("owned synthetic block fixture"), bytes.zero(4096)?])
  image.write(contents)

  assert failure_errno(linux.blockdev_info(image), "linux-blockdev") == 25
  assert failure_errno(linux.blockdev_set_read_only(image, true), "linux-blockdev") == 25
  assert failure_errno(linux.blockdev_set_read_only(image, false), "linux-blockdev") == 25
  assert failure_errno(linux.blockdev_flush(image), "linux-blockdev") == 25
  assert failure_errno(linux.blockdev_reread_partition_table(image), "linux-blockdev") == 25
  assert image.read_bytes()? == contents
}

test test_linux_umount_owned_nonmount_retains_failure { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("unmount is Linux-only")
    return
  }

  let root = test.temp_dir(ctx, name: "block-control-unmount")?
  let contents = fp"{root}/contents"
  contents.write("retained")

  # The owned directory is never mounted. Privilege checks may precede path
  # checks, so both an invalid mount target and permission denial are valid.
  for errno in [
    failure_errno(linux.umount(root), "linux-umount"),
    failure_errno(linux.umount(root, lazy: true), "linux-umount"),
    failure_errno(linux.umount(root, force: true), "linux-umount"),
    failure_errno(linux.umount(root, lazy: true, force: true), "linux-umount"),
  ] {
    assert errno == 1 or errno == 22
  }

  assert contents.read_text()? == "retained"
}

test test_linux_fstrim_zero_length_is_rejected_before_open { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("filesystem trim is Linux-only")
    return
  }

  let root = test.temp_dir(ctx, name: "block-control-trim")?
  let missing = fp"{root}/missing"
  match linux.fstrim(missing, length: 0) {
    Ok(_) => assert false, "empty trim range was accepted"
    Err(failure) => {
      test.error_kind(failure, "linux-fstrim")
      assert failure.message == "length must be positive"
      assert failure.errno == null
    }
  }
}

test test_linux_block_control_checks_typed_boundaries { |ctx|
  for source in [
    "linux.fstrim(/definitely-not-present, offset: -1)",
    "linux.fstrim(/definitely-not-present, length: -1)",
    "linux.fstrim(/definitely-not-present, minlen: -1)",
    "linux.umount(1)",
    "linux.blockdev_info(1)",
    "linux.blockdev_set_read_only(/definitely-not-present, 1)",
    "linux.fsfreeze(/definitely-not-present, 1)",
  ] {
    let failed = test.run_script(ctx, source)?
    assert ! failed.success, "invalid block-control argument passed checking"
    assert "err[check." in failed.stderr
  }
}

test test_linux_signature_wipe_is_selective_and_validates_all_offsets { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("signature controls are Linux-only")
    return
  }
  let root = test.temp_dir(ctx, name: "block-signatures")?
  let image = fp"{root}/image"
  let contents = bytes.concat([b"XFSB", bytes.zero(1076)?, b"\x53\xef", bytes.zero(100)?, b"retained payload"])
  image.write(contents)
  let signatures = linux.block_signatures(image)?
  assert signatures.len() == 2
  assert signatures[0].offset == 0 and signatures[0].type == "xfs"
  assert signatures[0].kind == "filesystem" and signatures[0].magic == b"XFSB"
  assert signatures[1].offset == 1080 and signatures[1].type == "ext2"
  assert signatures[1].magic == b"\x53\xef"

  test.error_kind(linux.wipe_block_signatures(image, [0, 1]), "linux-wipe-signatures")
  assert image.read_bytes()? == contents
  linux.wipe_block_signatures(image, [1080])
  assert image.read_bytes()? == bytes.concat([contents.slice(0, 1080), b"\0\0", contents.slice(1082)])
  let remaining = linux.block_signatures(image)?
  assert remaining.len() == 1 and remaining[0].type == "xfs"
  linux.wipe_block_signatures(image, [0, 0])
  assert linux.block_signatures(image)? == []
  assert image.read_bytes()?.slice(1182) == b"retained payload"
}

test test_linux_signatures_recognize_owned_files_at_known_offsets { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("signature controls are Linux-only")
    return
  }
  let root = test.temp_dir(ctx, name: "block-signature-formats")?
  let image = fp"{root}/image"
  for fixture in [
    {offset: 0, magic: b"hsqs", type: "squashfs"},
    {offset: 32769, magic: b"CD001", type: "iso9660"},
    {offset: 65600, magic: b"_BHRfS_M", type: "btrfs"},
    {offset: 4086, magic: b"SWAPSPACE2", type: "swap"},
  ] {
    image.write(bytes.concat([bytes.zero(fixture.offset)?, fixture.magic, b"retained payload"]))
    let signatures = linux.block_signatures(image)?
    assert signatures.len() == 1
    assert signatures[0].type == fixture.type and signatures[0].offset == fixture.offset
    assert signatures[0].magic == fixture.magic
    linux.wipe_block_signatures(image, [fixture.offset])
    assert linux.block_signatures(image)? == []
    assert image.read_bytes()?.slice(fixture.offset + fixture.magic.len()) == b"retained payload"
  }
  image.write(b"EFI PART")
  assert linux.block_signatures(image)? == []
  image.write(bytes.concat([bytes.zero(510)?, b"\x55\xaa"]))
  assert linux.block_signatures(image)? == []

  let fat_bytes = bytes.concat([
    bytes.zero(11)?, bytes.pack_le(512, 2)?, b"\x01", bytes.pack_le(32, 2)?, b"\x02",
    bytes.zero(65)?, b"FAT32   ", bytes.zero(420)?, b"\x55\xaa", b"retained payload",
  ])
  image.write(fat_bytes)
  let signatures = linux.block_signatures(image)?
  assert signatures.len() == 1 and signatures[0].type == "vfat"
  assert signatures[0].offset == 82 and signatures[0].magic == b"FAT32   "
  linux.wipe_block_signatures(image, [82])
  assert image.read_bytes()? == bytes.concat([fat_bytes.slice(0, 82), bytes.zero(8)?, fat_bytes.slice(90)])
}

test test_linux_signatures_validate_gpt_primary_and_backup_crc { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("signature controls are Linux-only")
    return
  }
  let root = test.temp_dir(ctx, name: "block-signature-gpt")?
  let image = fp"{root}/image"
  image.write(bytes.zero(1048576)?)
  linux.write_partition_table(image, {label: "gpt", id: "11111111-2222-3333-4444-555555555555", sector_size: 512, partitions: [
    {index: 1, start: 64, end: 127, size: 64, type: "0fc63daf-8483-4772-8e79-3d69d8477de4", uuid: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee", name: "owned"},
  ]})
  let signatures = linux.block_signatures(image)?
  assert signatures.len() == 3
  assert signatures[0].offset == 510 and signatures[0].type == "dos"
  assert signatures[1].offset == 512 and signatures[1].type == "gpt"
  assert signatures[2].offset == 1048064 and signatures[2].type == "gpt"
  let contents = image.read_bytes()?

  image.write(bytes.concat([contents.slice(0, 568), b"\xff", contents.slice(569)]))
  let bad_header = linux.block_signatures(image)?
  assert bad_header.len() == 2 and bad_header[1].offset == 1048064
  image.write(contents)

  # Corrupt the primary partition entry name while preserving its header.
  # The backup array is independent and must remain recognizable.
  image.write(bytes.concat([contents.slice(0, 1080), b"\xff", contents.slice(1081)]))
  let corrupt = linux.block_signatures(image)?
  assert corrupt.len() == 2
  assert corrupt[0].type == "dos"
  assert corrupt[1].offset == 1048064
  image.write(contents)

  linux.wipe_block_signatures(image, [512])
  assert image.read_bytes()? == bytes.concat([contents.slice(0, 512), bytes.zero(8)?, contents.slice(520)])
  let remaining = linux.block_signatures(image)?
  assert remaining.len() == 2 and remaining[1].offset == 1048064
}

test test_linux_mount_retains_kernel_errno_on_owned_missing_target { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("mount is Linux-only")
    return
  }
  let root = test.temp_dir(ctx, name: "mount-failure")?
  let missing = fp"{root}/missing"
  let errno = failure_errno(linux.mount("none", missing, fstype: "tmpfs"), "linux-mount")
  assert errno == 1 or errno == 2
  assert ! missing.exists()?
}

test test_linux_loop_attach_requires_writable_backing_without_fallback { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("loop attachment is Linux-only")
    return
  }
  let root = test.temp_dir(ctx, name: "loop-writable-backing")?
  let image = fp"{root}/image"
  let device_fixture = fp"{root}/device-fixture"
  image.write("read-only image")
  device_fixture.write("regular file, never a block device")
  root.chmod(493)
  image.chmod(292)
  device_fixture.chmod(438)
  let credentials = if unix.id()?.euid == 0 {
    "unix.set_credentials(uid: 65534, gid: 65534, groups: [])?\n"
  } else {
    ""
  }
  let source = credentials + f"""match linux.loop_attach(p"{image}", p"{device_fixture}") {{
  Ok(_) => assert false, "read-only backing was accepted"
  Err(failure) => {{
    test.error_kind(failure, "linux-loop")
    assert failure.errno == 13
  }}
}}
"""
  let result = test.run_script(ctx, source)?
  assert result.success, result.stderr
  assert image.read_text()? == "read-only image"
  assert device_fixture.read_text()? == "regular file, never a block device"
}
